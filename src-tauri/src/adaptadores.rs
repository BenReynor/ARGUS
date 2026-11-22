//! Configuración de otros bots mediante adaptadores controlados.
//!
//! La IA solo *propone*: entrega una acción tipada (conector, cuenta, servidor y
//! cambios) que se valida contra las capacidades que declara el adaptador. El
//! usuario la aprueba y solo entonces ARGUS la ejecuta, validando de nuevo el
//! destino con la credencial del llavero y verificando lo que el servicio permita.
//! Una escritura arbitraria del modelo (URL, método, cuerpo, tipo no declarado)
//! nunca llega al adaptador. Sin adaptador real para un servicio, no se ejecuta nada.
use crate::conectores;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Mutex;

/// Un tipo de cambio que el adaptador sabe hacer y el permiso que necesita en el servicio.
pub struct Capability {
    pub kind: &'static str,
    pub permission: &'static str,
}

/// Cambio tipado pedido por la IA. `params` lo valida el adaptador.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub kind: String,
    #[serde(default)]
    pub params: Value,
}

/// Forma exacta aceptada de la IA: cualquier otro campo se rechaza.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalInput {
    connector: String,
    account: String,
    guild_id: String,
    changes: Vec<Change>,
    #[serde(default)]
    summary: String,
}

pub enum TargetCheck {
    /// El secreto da acceso a esa cuenta y a ese servidor.
    Confirmed,
    /// La cuenta o el servidor no coinciden (motivo sin secretos).
    Wrong(String),
}

pub enum Applied {
    Done,
    /// El servicio aceptó solo una parte.
    Partial(String),
}

pub enum Verification {
    Verified,
    Mismatch(String),
    /// El servicio no permite comprobarlo.
    Unverifiable,
}

/// Adaptador de un servicio. Es la única vía de escritura hacia ese servicio.
pub trait Adapter: Send + Sync {
    fn connector(&self) -> &'static str;
    fn capabilities(&self) -> &'static [Capability];
    /// Valida los parámetros de un cambio de un tipo ya declarado.
    fn validate(&self, change: &Change) -> Result<()>;
    fn resolve_target(&self, secret: &str, account: &str, guild_id: &str) -> Result<TargetCheck>;
    fn apply(
        &self,
        secret: &str,
        account: &str,
        guild_id: &str,
        change: &Change,
    ) -> Result<Applied>;
    fn verify(
        &self,
        secret: &str,
        account: &str,
        guild_id: &str,
        change: &Change,
    ) -> Result<Verification>;
}

/// Acción preparada: lo que el usuario ve antes de aprobar.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectorProposal {
    pub connector: String,
    pub account: String,
    pub guild_id: String,
    pub changes: Vec<Change>,
    /// Permisos que exigen los cambios, tomados del adaptador, no del modelo.
    pub permissions: Vec<String>,
    pub summary: String,
}

/// Prueba de aprobación: solo se obtiene con `approve`.
pub struct Approved(ConnectorProposal);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Verified,
    /// Aplicado, pero el servicio no permite comprobarlo («no verificado»).
    Unverified,
    Partial,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeResult {
    pub kind: String,
    pub status: ChangeStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub status: ChangeStatus,
    pub results: Vec<ChangeResult>,
}

fn find<'a>(adapters: &'a [&'a dyn Adapter], connector: &str) -> Result<&'a dyn Adapter> {
    adapters
        .iter()
        .copied()
        .find(|a| a.connector() == connector)
        .ok_or_else(|| anyhow!("No adapter is available for this connector"))
}

/// Comprueba cada cambio contra las capacidades del adaptador y devuelve los permisos.
fn check_changes(adapter: &dyn Adapter, changes: &[Change]) -> Result<Vec<String>> {
    if changes.is_empty() {
        return Err(anyhow!("The action has no changes"));
    }
    let mut permissions: Vec<String> = Vec::new();
    for change in changes {
        let cap = adapter
            .capabilities()
            .iter()
            .find(|c| c.kind == change.kind)
            .ok_or_else(|| anyhow!("The connector does not support this change type"))?;
        adapter.validate(change)?;
        if !permissions.iter().any(|p| p == cap.permission) {
            permissions.push(cap.permission.to_string());
        }
    }
    Ok(permissions)
}

/// Convierte lo que escribió la IA en una propuesta tipada. No ejecuta nada.
pub fn prepare(
    adapters: &[&dyn Adapter],
    active_guild: &str,
    raw: &Value,
) -> Result<ConnectorProposal> {
    let input: ProposalInput = serde_json::from_value(raw.clone())
        .map_err(|_| anyhow!("The action is not a valid typed connector action"))?;
    if input.guild_id != active_guild {
        return Err(anyhow!("The action targets a different server"));
    }
    if input.account.trim().is_empty() {
        return Err(anyhow!("The action must name an account"));
    }
    let adapter = find(adapters, &input.connector)?;
    let permissions = check_changes(adapter, &input.changes)?;
    Ok(ConnectorProposal {
        connector: input.connector,
        account: input.account,
        guild_id: input.guild_id,
        changes: input.changes,
        permissions,
        summary: input.summary,
    })
}

/// El usuario aprueba exactamente lo que vio.
pub fn approve(proposal: ConnectorProposal) -> Approved {
    Approved(proposal)
}

fn worst(results: &[ChangeResult]) -> ChangeStatus {
    use ChangeStatus::*;
    if results.iter().all(|r| r.status == Verified) {
        Verified
    } else if results.iter().any(|r| matches!(r.status, Failed | Skipped)) {
        if results
            .iter()
            .any(|r| matches!(r.status, Verified | Unverified | Partial))
        {
            Partial
        } else {
            Failed
        }
    } else if results.iter().any(|r| r.status == Partial) {
        Partial
    } else {
        Unverified
    }
}

/// Ejecuta una acción aprobada. Antes de escribir vuelve a validar servidor activo,
/// credencial vigente, cuenta y cambios; la primera discrepancia aborta sin escribir.
pub async fn execute(
    pool: &SqlitePool,
    adapters: &[&dyn Adapter],
    approved: &Approved,
    active_guild: &str,
    now: i64,
) -> Result<Report> {
    let p = &approved.0;
    if p.guild_id != active_guild {
        return Err(anyhow!(
            "The active server changed since the action was approved"
        ));
    }
    let adapter = find(adapters, &p.connector)?;
    check_changes(adapter, &p.changes)?;
    let secret = conectores::credential(pool, &p.connector, &p.account, &p.guild_id, now)
        .await?
        .ok_or_else(|| anyhow!("The connector is not connected for this account and server"))?;
    match adapter.resolve_target(&secret, &p.account, &p.guild_id)? {
        TargetCheck::Confirmed => {}
        TargetCheck::Wrong(reason) => {
            return Err(anyhow!("The target does not match: {reason}"));
        }
    }
    let mut results: Vec<ChangeResult> = Vec::new();
    let mut stop = false;
    for change in &p.changes {
        let r = |status, message: String| ChangeResult {
            kind: change.kind.clone(),
            status,
            message,
        };
        if stop {
            results.push(r(ChangeStatus::Skipped, "Not attempted".into()));
            continue;
        }
        let result = match adapter.apply(&secret, &p.account, &p.guild_id, change) {
            Err(_) => {
                stop = true;
                r(
                    ChangeStatus::Failed,
                    "The service rejected the change".into(),
                )
            }
            Ok(Applied::Partial(detail)) => r(ChangeStatus::Partial, detail),
            Ok(Applied::Done) => match adapter.verify(&secret, &p.account, &p.guild_id, change) {
                Ok(Verification::Verified) => r(ChangeStatus::Verified, "Verified".into()),
                Ok(Verification::Mismatch(d)) => r(ChangeStatus::Partial, d),
                Ok(Verification::Unverifiable) | Err(_) => {
                    r(ChangeStatus::Unverified, "Not verified".into())
                }
            },
        };
        results.push(result);
    }
    Ok(Report {
        status: worst(&results),
        results,
    })
}

/// Acciones preparadas pendientes de aprobación (en memoria; un solo uso).
#[derive(Default)]
pub struct Pending {
    map: Mutex<HashMap<String, ConnectorProposal>>,
    next: Mutex<u64>,
}

impl Pending {
    pub fn add(&self, p: ConnectorProposal) -> String {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            format!("conn-{n}")
        };
        self.map.lock().unwrap().insert(id.clone(), p);
        id
    }
    /// Retira la propuesta para aprobarla; aprobar dos veces no es posible.
    pub fn take_approved(&self, id: &str) -> Result<Approved> {
        let p = self
            .map
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| anyhow!("There is no pending action with that id"))?;
        Ok(approve(p))
    }
}

static PENDING: std::sync::OnceLock<Pending> = std::sync::OnceLock::new();
fn pending() -> &'static Pending {
    PENDING.get_or_init(Pending::default)
}

/// Adaptadores reales disponibles. Ningún servicio externo tiene aún un adaptador
/// implementado: la lista está vacía y nada se ejecuta contra un servicio real.
fn adaptadores_reales() -> Vec<&'static dyn Adapter> {
    Vec::new()
}

#[derive(Serialize)]
pub struct PreparedAction {
    pub id: String,
    pub proposal: ConnectorProposal,
}

/// La IA prepara la acción; el usuario la ve y la aprueba aparte.
#[tauri::command]
pub fn preparar_accion_conector(
    state: tauri::State<'_, crate::state::AppState>,
    accion: Value,
) -> Result<PreparedAction, String> {
    let guild = { state.config.lock().unwrap().active_guild_id.clone() }
        .ok_or_else(|| "No server selected: pick one in the left column".to_string())?;
    let r = prepare(&adaptadores_reales(), &guild, &accion)
        .map(|proposal| PreparedAction {
            id: pending().add(proposal.clone()),
            proposal,
        })
        .map_err(|e| e.to_string());
    crate::i18n::localizar_error(&state, r)
}

/// Aprobación del usuario: ejecuta lo preparado y devuelve el resultado verificado.
#[tauri::command]
pub async fn aprobar_accion_conector(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    id: String,
) -> Result<Report, String> {
    let guild = { state.config.lock().unwrap().active_guild_id.clone() }
        .ok_or_else(|| "No server selected: pick one in the left column".to_string())?;
    let r = async {
        let approved = pending().take_approved(&id).map_err(|e| e.to_string())?;
        let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
        execute(
            &pool,
            &adaptadores_reales(),
            &approved,
            &guild,
            conectores::now(),
        )
        .await
        .map_err(|e| e.to_string())
    }
    .await;
    crate::i18n::localizar_error(&state, r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conectores::{Authorization, Authorizer, ConnectorSpec};
    use crate::db;
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const CAPS: &[Capability] = &[
        Capability {
            kind: "set_welcome",
            permission: "manage_config",
        },
        Capability {
            kind: "set_role",
            permission: "manage_roles",
        },
    ];
    const CAT: &[ConnectorSpec] = &[ConnectorSpec {
        id: "fakebot",
        accepts_credential: true,
        read_tools: &[],
    }];

    struct Auth;
    impl Authorizer for Auth {
        fn check(&self, _: &str, _: &str) -> Result<Authorization> {
            Ok(Authorization::Valid)
        }
    }

    #[derive(Default)]
    struct Fake {
        wrong_account: bool,
        partial: bool,
        unverifiable: bool,
        mismatch: bool,
        fail_kind: Option<&'static str>,
        applied: AtomicUsize,
    }
    impl Adapter for Fake {
        fn connector(&self) -> &'static str {
            "fakebot"
        }
        fn capabilities(&self) -> &'static [Capability] {
            CAPS
        }
        fn validate(&self, c: &Change) -> Result<()> {
            if c.params.get("value").and_then(Value::as_str).is_none() {
                return Err(anyhow!("value is required"));
            }
            Ok(())
        }
        fn resolve_target(&self, _: &str, _: &str, _: &str) -> Result<TargetCheck> {
            Ok(if self.wrong_account {
                TargetCheck::Wrong("account has no access".into())
            } else {
                TargetCheck::Confirmed
            })
        }
        fn apply(&self, _: &str, _: &str, _: &str, c: &Change) -> Result<Applied> {
            self.applied.fetch_add(1, Ordering::SeqCst);
            if self.fail_kind == Some(c.kind.as_str()) {
                return Err(anyhow!("boom sk-fake-SECRET"));
            }
            Ok(if self.partial {
                Applied::Partial("only 1 of 2 fields saved".into())
            } else {
                Applied::Done
            })
        }
        fn verify(&self, _: &str, _: &str, _: &str, _: &Change) -> Result<Verification> {
            Ok(if self.unverifiable {
                Verification::Unverifiable
            } else if self.mismatch {
                Verification::Mismatch("stored value differs".into())
            } else {
                Verification::Verified
            })
        }
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::run_migrations(&pool).await.unwrap();
        conectores::connect(
            &pool,
            CAT,
            &Auth,
            "fakebot",
            "acct",
            "g1",
            "api_key",
            "sk-fake-SECRET",
            None,
            100,
        )
        .await
        .unwrap();
        pool
    }

    fn raw() -> Value {
        json!({"connector":"fakebot","account":"acct","guild_id":"g1","summary":"s",
          "changes":[{"kind":"set_welcome","params":{"value":"hi"}},
                     {"kind":"set_role","params":{"value":"r"}}]})
    }

    #[test]
    fn prepare_identifies_everything_and_takes_permissions_from_adapter() {
        let a = Fake::default();
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        assert_eq!(
            (
                p.connector.as_str(),
                p.account.as_str(),
                p.guild_id.as_str()
            ),
            ("fakebot", "acct", "g1")
        );
        assert_eq!(p.changes.len(), 2);
        assert_eq!(p.permissions, vec!["manage_config", "manage_roles"]);
        assert_eq!(a.applied.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn approved_action_is_applied_and_verified() {
        let pool = pool().await;
        let a = Fake::default();
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let rep = execute(&pool, &[&a], &approve(p), "g1", 200).await.unwrap();
        assert_eq!(rep.status, ChangeStatus::Verified);
        assert_eq!(a.applied.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn unverifiable_service_shows_not_verified() {
        let pool = pool().await;
        let a = Fake {
            unverifiable: true,
            ..Default::default()
        };
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let rep = execute(&pool, &[&a], &approve(p), "g1", 200).await.unwrap();
        assert_eq!(rep.status, ChangeStatus::Unverified);
        assert!(rep.results.iter().all(|r| r.message == "Not verified"));
    }

    #[tokio::test]
    async fn wrong_account_aborts_before_any_write() {
        let pool = pool().await;
        let a = Fake {
            wrong_account: true,
            ..Default::default()
        };
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let err = execute(&pool, &[&a], &approve(p), "g1", 200)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not match"));
        assert_eq!(a.applied.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn partial_response_is_never_total_success() {
        let pool = pool().await;
        let a = Fake {
            partial: true,
            ..Default::default()
        };
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let rep = execute(&pool, &[&a], &approve(p), "g1", 200).await.unwrap();
        assert_eq!(rep.status, ChangeStatus::Partial);

        let a = Fake {
            mismatch: true,
            ..Default::default()
        };
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let rep = execute(&pool, &[&a], &approve(p), "g1", 200).await.unwrap();
        assert_eq!(rep.status, ChangeStatus::Partial);
    }

    #[tokio::test]
    async fn failure_stops_next_changes_and_hides_the_secret() {
        let pool = pool().await;
        let a = Fake {
            fail_kind: Some("set_welcome"),
            ..Default::default()
        };
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        let rep = execute(&pool, &[&a], &approve(p), "g1", 200).await.unwrap();
        assert_eq!(rep.status, ChangeStatus::Failed);
        assert_eq!(rep.results[1].status, ChangeStatus::Skipped);
        assert_eq!(a.applied.load(Ordering::SeqCst), 1);
        assert!(!serde_json::to_string(&rep).unwrap().contains("SECRET"));
    }

    #[test]
    fn direct_arbitrary_writes_from_the_model_are_rejected() {
        let a = Fake::default();
        // Campos de escritura libre.
        for extra in ["url", "method", "body", "request", "headers"] {
            let mut r = raw();
            r[extra] = json!("x");
            assert!(prepare(&[&a], "g1", &r).is_err(), "{extra}");
        }
        let mut r = raw();
        r["changes"][0]["body"] = json!({"any": 1});
        assert!(prepare(&[&a], "g1", &r).is_err());
        // Tipo de cambio no declarado, parámetros inválidos y cambios vacíos.
        let mut r = raw();
        r["changes"][0]["kind"] = json!("delete_everything");
        assert!(prepare(&[&a], "g1", &r).is_err());
        let mut r = raw();
        r["changes"][0]["params"] = json!({});
        assert!(prepare(&[&a], "g1", &r).is_err());
        let mut r = raw();
        r["changes"] = json!([]);
        assert!(prepare(&[&a], "g1", &r).is_err());
        // Conector sin adaptador y servidor distinto del activo.
        let mut r = raw();
        r["connector"] = json!("unknown");
        assert!(prepare(&[&a], "g1", &r).is_err());
        assert!(prepare(&[&a], "g2", &raw()).is_err());
        assert_eq!(a.applied.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn nothing_runs_without_approval_and_approval_is_single_use() {
        let a = Fake::default();
        let pend = Pending::default();
        let id = pend.add(prepare(&[&a], "g1", &raw()).unwrap());
        assert!(pend.take_approved("conn-999").is_err());
        assert!(pend.take_approved(&id).is_ok());
        assert!(pend.take_approved(&id).is_err());
        assert_eq!(a.applied.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn target_is_validated_again_at_execution() {
        let pool = pool().await;
        let a = Fake::default();
        // El servidor activo cambió tras aprobar.
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        assert!(execute(&pool, &[&a], &approve(p), "g2", 200).await.is_err());
        // Sin credencial para esa cuenta.
        let mut r = raw();
        r["account"] = json!("other");
        let p = prepare(&[&a], "g1", &r).unwrap();
        assert!(execute(&pool, &[&a], &approve(p), "g1", 200).await.is_err());
        // Credencial expirada.
        conectores::connect(
            &pool,
            CAT,
            &Auth,
            "fakebot",
            "acct",
            "g1",
            "api_key",
            "sk-fake-SECRET",
            Some(300),
            100,
        )
        .await
        .unwrap();
        let p = prepare(&[&a], "g1", &raw()).unwrap();
        assert!(execute(&pool, &[&a], &approve(p), "g1", 400).await.is_err());
        assert_eq!(a.applied.load(Ordering::SeqCst), 0);
    }
}
