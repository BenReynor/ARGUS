//! Persistencia del mapa en vivo: guarda los mapas en `server_maps` (como mucho
//! cada 30 s y solo si cambiaron), registra cada cambio resumido en `incidents`
//! y recupera el último mapa guardado al arrancar.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::SqlitePool;

use crate::db;
use crate::mapa::ResumenCambio;
use crate::state::{DynError, ServerMap};

/// Intervalo mínimo entre dos guardados del mapa.
pub const INTERVALO_GUARDADO: Duration = Duration::from_secs(30);

/// Carga los últimos mapas guardados. Un registro ilegible se ignora con un aviso.
pub async fn cargar_mapas(pool: &SqlitePool) -> Result<HashMap<String, ServerMap>, DynError> {
    let mut mapas = HashMap::new();
    for (guild_id, data) in db::list_server_maps(pool).await? {
        match serde_json::from_str::<ServerMap>(&data) {
            Ok(mapa) => {
                mapas.insert(guild_id, mapa);
            }
            Err(e) => tracing::warn!("Mapa guardado ilegible ({guild_id}): {e}"),
        }
    }
    Ok(mapas)
}

/// Guarda en `server_maps` los mapas cuyo contenido difiere del último guardado
/// (`ultimo`: guild_id → JSON). Devuelve cuántos guardó. El lock no cruza el `.await`.
pub async fn guardar_si_cambio(
    pool: &SqlitePool,
    mapas: &Mutex<HashMap<String, ServerMap>>,
    ultimo: &mut HashMap<String, String>,
) -> Result<usize, DynError> {
    let cambiados: Vec<(String, String)> = {
        let mapas = mapas.lock().unwrap();
        mapas
            .iter()
            .filter_map(|(id, m)| serde_json::to_string(m).ok().map(|j| (id.clone(), j)))
            .filter(|(id, json)| ultimo.get(id) != Some(json))
            .collect()
    };
    let total = cambiados.len();
    for (guild_id, json) in cambiados {
        db::upsert_server_map(pool, &guild_id, &json).await?;
        ultimo.insert(guild_id, json);
    }
    Ok(total)
}

/// Inserta un cambio resumido del mapa en `incidents`.
pub async fn registrar_cambio(
    pool: &SqlitePool,
    guild_id: &str,
    cambio: &ResumenCambio,
) -> Result<(), DynError> {
    db::insert_incident(
        pool,
        &db::Incident {
            id: 0,
            guild_id: Some(guild_id.to_string()),
            type_: cambio.tipo.to_string(),
            payload: cambio.descripcion.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
    .await?;
    Ok(())
}

/// Tarea que guarda los mapas cambiados cada `intervalo`. `ultimo` parte de lo
/// ya guardado (lo cargado al arrancar) para no reescribirlo sin cambios.
pub fn lanzar_guardado(
    pool: SqlitePool,
    mapas: Arc<Mutex<HashMap<String, ServerMap>>>,
    mut ultimo: HashMap<String, String>,
    intervalo: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reloj = tokio::time::interval(intervalo);
        reloj.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reloj.tick().await; // el primer tick es inmediato
        loop {
            reloj.tick().await;
            if let Err(e) = guardar_si_cambio(&pool, &mapas, &mut ultimo).await {
                tracing::error!("No se pudo guardar el mapa: {e}");
            }
        }
    })
}

/// JSON de cada mapa tal como se guardaría (estado inicial de `lanzar_guardado`).
pub fn instantanea(mapas: &HashMap<String, ServerMap>) -> HashMap<String, String> {
    mapas
        .iter()
        .filter_map(|(id, m)| serde_json::to_string(m).ok().map(|j| (id.clone(), j)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Guild;
    use sqlx::sqlite::SqlitePoolOptions;

    fn mapa(guild: &str, nombre: &str) -> ServerMap {
        ServerMap {
            guild: Guild {
                id: guild.into(),
                name: nombre.into(),
                member_count: 0,
                icon: None,
                joined_at: None,
            },
            channels: vec![],
            members: vec![],
            roles: vec![],
            categories: HashMap::new(),
            last_updated: String::new(),
        }
    }

    async fn pool_temporal(nombre: &str) -> (SqlitePool, std::path::PathBuf) {
        let ruta = std::env::temp_dir().join(format!(
            "argus_persistencia_{}_{}.db",
            nombre,
            std::process::id()
        ));
        let _ = std::fs::remove_file(&ruta);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", ruta.display()))
            .await
            .expect("BD temporal");
        db::run_migrations(&pool).await.expect("migraciones");
        (pool, ruta)
    }

    #[tokio::test]
    async fn guarda_solo_si_cambio_y_se_recupera_al_arrancar() {
        let (pool, ruta) = pool_temporal("guardar").await;
        let mapas = Mutex::new(HashMap::from([("g1".to_string(), mapa("g1", "Uno"))]));
        let mut ultimo = HashMap::new();

        assert_eq!(
            guardar_si_cambio(&pool, &mapas, &mut ultimo).await.unwrap(),
            1
        );
        // Sin cambios no se vuelve a escribir.
        assert_eq!(
            guardar_si_cambio(&pool, &mapas, &mut ultimo).await.unwrap(),
            0
        );

        mapas.lock().unwrap().get_mut("g1").unwrap().guild.name = "Renombrado".into();
        mapas.lock().unwrap().insert("g2".into(), mapa("g2", "Dos"));
        assert_eq!(
            guardar_si_cambio(&pool, &mapas, &mut ultimo).await.unwrap(),
            2
        );

        // «Arranque»: se cargan los últimos mapas guardados.
        let cargados = cargar_mapas(&pool).await.unwrap();
        assert_eq!(cargados.len(), 2);
        assert_eq!(cargados["g1"].guild.name, "Renombrado");
        assert_eq!(cargados["g2"].guild.name, "Dos");

        // Con la instantánea de lo cargado, no hay nada que reescribir.
        let mut ultimo = instantanea(&cargados);
        let mapas = Mutex::new(cargados);
        assert_eq!(
            guardar_si_cambio(&pool, &mapas, &mut ultimo).await.unwrap(),
            0
        );
        pool.close().await;
        let _ = std::fs::remove_file(ruta);
    }

    #[tokio::test]
    async fn cada_cambio_resumido_se_inserta_en_incidents() {
        let (pool, ruta) = pool_temporal("incidents").await;
        let cambio = ResumenCambio {
            tipo: "CHANNEL_CREATE",
            descripcion: "Canal creado: general".into(),
        };
        registrar_cambio(&pool, "g1", &cambio).await.unwrap();
        registrar_cambio(&pool, "g2", &cambio).await.unwrap();

        let de_g1 = db::list_incidents(&pool, Some("g1"), 10).await.unwrap();
        assert_eq!(de_g1.len(), 1);
        assert_eq!(de_g1[0].type_, "CHANNEL_CREATE");
        assert_eq!(de_g1[0].payload, "Canal creado: general");
        assert_eq!(db::list_incidents(&pool, None, 10).await.unwrap().len(), 2);
        pool.close().await;
        let _ = std::fs::remove_file(ruta);
    }

    #[tokio::test]
    async fn la_tarea_guarda_periodicamente_lo_cambiado() {
        let (pool, ruta) = pool_temporal("tarea").await;
        let mapas = Arc::new(Mutex::new(HashMap::new()));
        let tarea = lanzar_guardado(
            pool.clone(),
            mapas.clone(),
            HashMap::new(),
            Duration::from_millis(40),
        );
        mapas.lock().unwrap().insert("g1".into(), mapa("g1", "Uno"));
        tokio::time::sleep(Duration::from_millis(300)).await;
        tarea.abort();
        assert!(cargar_mapas(&pool).await.unwrap().contains_key("g1"));
        pool.close().await;
        let _ = std::fs::remove_file(ruta);
    }
}
