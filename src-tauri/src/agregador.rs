//! Capa de agregación de conectores dentro del MCP de ARGUS. Reúne las herramientas
//! de lectura de varios MCP externos con nombres `conector__herramienta`. Solo se
//! exponen las que el catálogo versionado lista como lectura para un conector
//! habilitado: las escrituras y las herramientas desconocidas nunca llegan al modelo.
//! La política es la misma para Claude, Codex y OpenCode (los tres usan este MCP).
//! Cada conector tiene sus límites y su estado: la caída de uno no afecta a los demás.
use crate::conectores::ConnectorSpec;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Cliente de un MCP externo (en producción stdio/HTTP; en tests, uno falso en proceso).
pub trait Upstream: Send + Sync {
    /// Nombres de las herramientas que declara el MCP externo.
    fn list_tools(&self) -> BoxFut<'_, Result<Vec<String>>>;
    fn call(&self, tool: &str, args: Value) -> BoxFut<'_, Result<Value>>;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub timeout: Duration,
    pub max_result_bytes: usize,
    pub max_concurrent: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            max_result_bytes: 256 * 1024,
            max_concurrent: 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Health {
    Unknown,
    Up,
    Down(String),
}

struct Slot {
    upstream: Arc<dyn Upstream>,
    limits: Limits,
    gate: Arc<Semaphore>,
    health: Mutex<Health>,
}

pub struct Aggregator {
    /// (id, herramientas de lectura) del catálogo, solo de conectores habilitados.
    allowed: Vec<(&'static str, &'static [&'static str])>,
    slots: HashMap<&'static str, Slot>,
}

impl Aggregator {
    pub fn new(
        catalogue: &'static [ConnectorSpec],
        enabled: &[String],
        upstreams: Vec<(&'static str, Arc<dyn Upstream>, Limits)>,
    ) -> Self {
        let mut allowed = Vec::new();
        let mut slots = HashMap::new();
        for spec in catalogue {
            if !enabled.iter().any(|e| e == spec.id) {
                continue;
            }
            if let Some((_, up, limits)) = upstreams.iter().find(|(id, _, _)| *id == spec.id) {
                allowed.push((spec.id, spec.read_tools));
                slots.insert(
                    spec.id,
                    Slot {
                        upstream: up.clone(),
                        limits: *limits,
                        gate: Arc::new(Semaphore::new(limits.max_concurrent.max(1))),
                        health: Mutex::new(Health::Unknown),
                    },
                );
            }
        }
        Self { allowed, slots }
    }

    pub fn health(&self, connector: &str) -> Option<Health> {
        self.slots
            .get(connector)
            .map(|s| s.health.lock().unwrap().clone())
    }

    fn set_health(&self, id: &str, h: Health) {
        if let Some(s) = self.slots.get(id) {
            *s.health.lock().unwrap() = h;
        }
    }

    /// Herramientas expuestas (`conector__herramienta`): catálogo ∩ declaradas por el MCP.
    /// Un conector caído o lento se omite sin afectar a los demás.
    pub async fn list_tools(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (id, read_tools) in &self.allowed {
            let slot = &self.slots[id];
            let listed =
                match tokio::time::timeout(slot.limits.timeout, slot.upstream.list_tools()).await {
                    Ok(r) => r,
                    Err(_) => Err(anyhow!("Connector timed out")),
                };
            match listed {
                Ok(declared) => {
                    self.set_health(id, Health::Up);
                    for t in read_tools
                        .iter()
                        .filter(|t| declared.iter().any(|d| d == *t))
                    {
                        out.push(format!("{id}__{t}"));
                    }
                }
                Err(e) => self.set_health(id, Health::Down(e.to_string())),
            }
        }
        out
    }

    /// Llama a una herramienta de lectura. `cancel` pasa a `true` para cancelar.
    pub async fn call(
        &self,
        name: &str,
        args: Value,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<Value> {
        let (id, tool) = name
            .split_once("__")
            .ok_or_else(|| anyhow!("Unknown connector tool"))?;
        let ok = self
            .allowed
            .iter()
            .find(|(cid, _)| *cid == id)
            .is_some_and(|(_, tools)| tools.contains(&tool));
        if !ok {
            return Err(anyhow!("Unknown connector tool"));
        }
        let slot = &self.slots[id];
        let _permit = slot
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| anyhow!("Connector is busy"))?;
        if *cancel.borrow() {
            return Err(anyhow!("Cancelled"));
        }
        let wait_cancel = async {
            loop {
                if cancel.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
                if *cancel.borrow() {
                    return;
                }
            }
        };
        let res = tokio::select! {
            r = tokio::time::timeout(slot.limits.timeout, slot.upstream.call(tool, args)) => match r {
                Ok(r) => r,
                Err(_) => Err(anyhow!("Connector timed out")),
            },
            _ = wait_cancel => return Err(anyhow!("Cancelled")),
        };
        match res {
            Ok(v) => {
                if v.to_string().len() > slot.limits.max_result_bytes {
                    return Err(anyhow!("Connector result too large"));
                }
                self.set_health(id, Health::Up);
                Ok(v)
            }
            Err(e) => {
                self.set_health(id, Health::Down(e.to_string()));
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Fake {
        tools: Vec<&'static str>,
        down: AtomicBool,
        hang: bool,
        dropped: Arc<AtomicBool>,
        tag: &'static str,
    }
    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    impl Fake {
        fn new(tag: &'static str, tools: Vec<&'static str>) -> Self {
            Self {
                tools,
                down: AtomicBool::new(false),
                hang: false,
                dropped: Arc::new(AtomicBool::new(false)),
                tag,
            }
        }
    }
    impl Upstream for Fake {
        fn list_tools(&self) -> BoxFut<'_, Result<Vec<String>>> {
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("down"));
                }
                Ok(self.tools.iter().map(|s| s.to_string()).collect())
            })
        }
        fn call(&self, tool: &str, _args: Value) -> BoxFut<'_, Result<Value>> {
            let tool = tool.to_string();
            Box::pin(async move {
                if self.down.load(Ordering::SeqCst) {
                    return Err(anyhow!("down"));
                }
                if self.hang {
                    let _g = DropFlag(self.dropped.clone());
                    std::future::pending::<()>().await;
                }
                Ok(json!({"from": self.tag, "tool": tool}))
            })
        }
    }

    static CAT: &[ConnectorSpec] = &[
        ConnectorSpec {
            id: "one",
            accepts_credential: true,
            read_tools: &["search", "get"],
        },
        ConnectorSpec {
            id: "two",
            accepts_credential: true,
            read_tools: &["search"],
        },
        ConnectorSpec {
            id: "off",
            accepts_credential: true,
            read_tools: &["search"],
        },
    ];

    fn lim() -> Limits {
        Limits {
            timeout: Duration::from_millis(200),
            ..Limits::default()
        }
    }
    fn no_cancel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
        watch::channel(false)
    }
    fn agg(one: Arc<Fake>, two: Arc<Fake>) -> Aggregator {
        Aggregator::new(
            CAT,
            &["one".into(), "two".into()],
            vec![
                ("one", one as Arc<dyn Upstream>, lim()),
                ("two", two as Arc<dyn Upstream>, lim()),
            ],
        )
    }

    #[tokio::test]
    async fn solo_lectura_del_catalogo_con_homonimas_sin_colision() {
        // `one` declara además escrituras y una desconocida: no deben salir.
        let one = Arc::new(Fake::new(
            "one",
            vec!["search", "get", "delete_repo", "weird"],
        ));
        let two = Arc::new(Fake::new("two", vec!["search", "create_issue"]));
        let a = agg(one, two);
        let mut t = a.list_tools().await;
        t.sort();
        assert_eq!(t, vec!["one__get", "one__search", "two__search"]);
        let (_tx, rx) = no_cancel();
        let r1 = a.call("one__search", json!({}), rx.clone()).await.unwrap();
        let r2 = a.call("two__search", json!({}), rx.clone()).await.unwrap();
        assert_eq!(r1["from"], "one");
        assert_eq!(r2["from"], "two");
        for bad in [
            "one__delete_repo",
            "two__create_issue",
            "one__weird",
            "search",
            "off__search",
            "nope__x",
        ] {
            assert!(a.call(bad, json!({}), rx.clone()).await.is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn conector_caido_no_cambia_el_estado_de_los_otros() {
        let one = Arc::new(Fake::new("one", vec!["search"]));
        let two = Arc::new(Fake::new("two", vec!["search"]));
        let a = agg(one.clone(), two);
        one.down.store(true, Ordering::SeqCst);
        assert_eq!(a.list_tools().await, vec!["two__search"]);
        assert!(matches!(a.health("one"), Some(Health::Down(_))));
        assert_eq!(a.health("two"), Some(Health::Up));
        let (_tx, rx) = no_cancel();
        assert!(a.call("one__search", json!({}), rx.clone()).await.is_err());
        assert!(a.call("two__search", json!({}), rx).await.is_ok());
        assert_eq!(a.health("two"), Some(Health::Up));
        one.down.store(false, Ordering::SeqCst);
        assert_eq!(a.list_tools().await.len(), 2);
        assert_eq!(a.health("one"), Some(Health::Up));
    }

    #[tokio::test]
    async fn cancelacion_y_limite_de_tiempo_liberan_la_llamada() {
        let mut slow = Fake::new("one", vec!["search"]);
        slow.hang = true;
        let dropped = slow.dropped.clone();
        let a = Arc::new(agg(
            Arc::new(slow),
            Arc::new(Fake::new("two", vec!["search"])),
        ));
        let (tx, rx) = no_cancel();
        let a2 = a.clone();
        let h = tokio::spawn(async move { a2.call("one__search", json!({}), rx).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        tx.send(true).unwrap();
        assert_eq!(h.await.unwrap().unwrap_err().to_string(), "Cancelled");
        assert!(dropped.load(Ordering::SeqCst));
        // Sin cancelar vence el límite de tiempo.
        let (_tx2, rx2) = no_cancel();
        let e = a
            .call("one__search", json!({}), rx2.clone())
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "Connector timed out");
        // El otro conector sigue bien.
        assert!(a.call("two__search", json!({}), rx2).await.is_ok());
        assert_eq!(a.health("two"), Some(Health::Up));
    }

    #[tokio::test]
    async fn limites_por_conector_y_no_habilitados() {
        let one = Arc::new(Fake::new("one", vec!["search"]));
        let two = Arc::new(Fake::new("two", vec!["search"]));
        let a = Aggregator::new(
            CAT,
            &["one".into()],
            vec![
                (
                    "one",
                    one as Arc<dyn Upstream>,
                    Limits {
                        max_result_bytes: 5,
                        ..lim()
                    },
                ),
                ("two", two as Arc<dyn Upstream>, lim()),
            ],
        );
        let (_tx, rx) = no_cancel();
        assert_eq!(a.list_tools().await, vec!["one__search"]);
        assert!(a.call("two__search", json!({}), rx.clone()).await.is_err());
        assert_eq!(
            a.call("one__search", json!({}), rx)
                .await
                .unwrap_err()
                .to_string(),
            "Connector result too large"
        );
    }
}
