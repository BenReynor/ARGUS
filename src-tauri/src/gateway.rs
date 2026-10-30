//! Conexión al Gateway de Discord (twilight-gateway) mientras el bot está activo.
//! Cada evento recibido se aplica a los mapas con `mapa::aplicar_evento`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use twilight_gateway::{Intents, Shard, ShardId};
use twilight_model::gateway::event::Event;

use crate::mapa::{aplicar_evento, ResumenCambio};
use crate::state::ServerMap;

/// Intents que ARGUS necesita: servidores/canales/roles y miembros.
pub fn intents() -> Intents {
    Intents::GUILDS | Intents::GUILD_MEMBERS
}

/// Aplica un evento a todos los mapas (cada uno ignora los de otros servidores).
/// Devuelve cada cambio con el id de su servidor.
/// El lock se toma y se suelta aquí, sin `.await` de por medio.
pub fn aplicar_a_mapas(
    mapas: &Mutex<HashMap<String, ServerMap>>,
    evento: &Event,
) -> Vec<(String, ResumenCambio)> {
    let mut mapas = mapas.lock().unwrap();
    mapas
        .values_mut()
        .filter_map(|m| aplicar_evento(m, evento).map(|c| (m.guild.id.clone(), c)))
        .collect()
}

/// Nombre del evento Tauri que avisa a la UI de que el mapa cambió.
pub const EVENTO_MAPA: &str = "map-updated";

/// Espera de agrupación: varios eventos seguidos producen un solo aviso.
pub const DEBOUNCE: Duration = Duration::from_millis(500);

/// Nombre del evento Tauri que avisa a la UI de un cambio del estado del bot.
pub const EVENTO_ESTADO: &str = "bot-status-change";

/// Emite `bot-status-change` con el estado actual.
/// El último error va en el idioma de la config.
pub fn emitir_estado(app: &AppHandle, status: &crate::state::BotStatus) {
    let lang = app
        .try_state::<crate::state::AppState>()
        .map(|s| crate::i18n::idioma(&s))
        .unwrap_or_else(|| crate::i18n::IDIOMA_DEFECTO.to_string());
    let status = crate::i18n::estado_traducido(&lang, status.clone());
    if let Err(e) = app.emit(EVENTO_ESTADO, status) {
        tracing::warn!("No se pudo emitir bot-status-change: {e}");
    }
}

/// Aborta la tarea de avisos cuando se descarta (p. ej. al abortar el Gateway).
struct AbortAlSoltar(tokio::task::JoinHandle<()>);

impl Drop for AbortAlSoltar {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Espera cada `notify`, deja pasar `DEBOUNCE` y llama a `emitir` una sola vez.
async fn avisar_con_debounce(notify: Arc<Notify>, emitir: impl Fn()) {
    loop {
        notify.notified().await;
        tokio::time::sleep(DEBOUNCE).await;
        emitir();
    }
}

/// Lanza la tarea del Gateway. Twilight gestiona heartbeats y reconexión.
/// Tras aplicar eventos emite `map-updated` (agrupado con debounce).
pub fn lanzar(
    app: AppHandle,
    token: String,
    mapas: Arc<Mutex<HashMap<String, ServerMap>>>,
    estado: Arc<Mutex<crate::state::BotStatus>>,
    pool: Option<sqlx::SqlitePool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let notify = Arc::new(Notify::new());
        let app_estado = app.clone();
        let _avisos = AbortAlSoltar(tokio::spawn(avisar_con_debounce(
            notify.clone(),
            move || {
                if let Err(e) = app.emit(EVENTO_MAPA, ()) {
                    tracing::warn!("No se pudo emitir map-updated: {e}");
                }
            },
        )));
        let mut shard = Shard::new(ShardId::ONE, token, intents());
        loop {
            match shard.next_event().await {
                Ok(evento) => {
                    let cambios = aplicar_a_mapas(&mapas, &evento);
                    for (guild_id, cambio) in &cambios {
                        tracing::info!("Mapa actualizado: {}", cambio.tipo);
                        if let Some(pool) = &pool {
                            if let Err(e) =
                                crate::persistencia::registrar_cambio(pool, guild_id, cambio).await
                            {
                                tracing::warn!("No se pudo registrar el cambio: {e}");
                            }
                        }
                    }
                    if !cambios.is_empty() {
                        notify.notify_one();
                    }
                }
                Err(e) => {
                    tracing::warn!("Gateway error: {e}");
                    if e.is_fatal() {
                        let nuevo = {
                            let mut st = estado.lock().unwrap();
                            st.running = false;
                            st.connected = false;
                            st.last_error = Some(format!("Gateway error: {e}"));
                            st.clone()
                        };
                        emitir_estado(&app_estado, &nuevo);
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intents_son_guilds_y_miembros() {
        assert_eq!(intents(), Intents::GUILDS | Intents::GUILD_MEMBERS);
    }

    #[tokio::test]
    async fn los_avisos_seguidos_se_agrupan_en_uno() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let notify = Arc::new(Notify::new());
        let cuenta = Arc::new(AtomicU32::new(0));
        let c = cuenta.clone();
        let tarea = tokio::spawn(avisar_con_debounce(notify.clone(), move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        notify.notify_one();
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            notify.notify_one();
        }
        tokio::time::sleep(DEBOUNCE * 2).await;
        assert!(cuenta.load(Ordering::SeqCst) <= 2);
        assert!(cuenta.load(Ordering::SeqCst) >= 1);
        tarea.abort();
    }

    #[test]
    fn evento_sin_mapas_no_cambia_nada() {
        let mapas = Mutex::new(HashMap::new());
        assert!(aplicar_a_mapas(&mapas, &Event::GatewayHeartbeatAck).is_empty());
    }
}
