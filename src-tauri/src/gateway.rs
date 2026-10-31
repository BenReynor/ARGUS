//! Conexión al Gateway de Discord (twilight-gateway) mientras el bot está activo.
//! Cada evento recibido se aplica a los mapas con `mapa::aplicar_evento`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use twilight_gateway::{Intents, Shard, ShardId};
use twilight_model::gateway::event::Event;

use crate::chat::MensajeEntrante;
use crate::mapa::{aplicar_evento, ResumenCambio};
use crate::responder::{Conversador, Coordinador, Entrada, LIMITE_AGENTE};
use crate::state::ServerMap;

/// Intents que ARGUS necesita: servidores/canales/roles, miembros y mensajes de
/// servidor (para responder a menciones).
pub fn intents() -> Intents {
    Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_MESSAGES
}

/// Convierte un mensaje de servidor en la entrada del conversador. `None` si no
/// es de un servidor. Los nombres salen de los mapas (o un valor genérico).
pub fn entrada_de(
    m: &twilight_model::channel::Message,
    mapas: &Mutex<HashMap<String, ServerMap>>,
) -> Option<Entrada> {
    let guild = m.guild_id?;
    let (servidor, canal) = {
        let mapas = mapas.lock().unwrap();
        let mapa = mapas.get(&guild.to_string());
        (
            mapa.map(|x| x.guild.name.clone())
                .unwrap_or_else(|| guild.to_string()),
            mapa.and_then(|x| {
                x.channels
                    .iter()
                    .find(|c| c.id == m.channel_id.to_string())
                    .map(|c| c.name.clone())
            })
            .unwrap_or_else(|| m.channel_id.to_string()),
        )
    };
    Some(Entrada {
        msg: MensajeEntrante {
            autor_id: m.author.id.get(),
            autor_es_bot: m.author.bot,
            canal_id: m.channel_id.get(),
            menciones: m.mentions.iter().map(|u| u.id.get()).collect(),
            responde_a_autor: m.referenced_message.as_ref().map(|r| r.author.id.get()),
        },
        mensaje_id: m.id.get(),
        guild_id: guild.get(),
        texto: m.content.clone(),
        autor_nombre: m
            .author
            .global_name
            .clone()
            .unwrap_or_else(|| m.author.name.clone()),
        servidor,
        canal,
    })
}

/// Crea el conversador de un bot: sus avisos van a Logs (evento `bot-log`).
fn conversador(app: &AppHandle, token: &str, bot_id: u64) -> Conversador {
    let app = app.clone();
    Conversador {
        token: token.to_string(),
        base_url: None,
        bot_id,
        coord: Arc::new(Coordinador::default()),
        limite: LIMITE_AGENTE,
        registro: Arc::new(move |texto| {
            let lang = app
                .try_state::<crate::state::AppState>()
                .map(|s| crate::i18n::idioma(&s))
                .unwrap_or_else(|| crate::i18n::IDIOMA_DEFECTO.to_string());
            let texto = crate::i18n::tr(&lang, &texto);
            if let Err(e) = app.emit("bot-log", texto) {
                tracing::warn!("No se pudo emitir bot-log: {e}");
            }
        }),
        buscar_agente: Arc::new(|nombre| {
            crate::asistente::buscar_binario(nombre, &crate::asistente::carpetas_busqueda())
        }),
    }
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
    bot_id: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let conversador = bot_id
            .parse::<u64>()
            .ok()
            .map(|id| conversador(&app, &token, id));
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
                    if let (Event::MessageCreate(m), Some(conv), Some(pool)) =
                        (&evento, &conversador, &pool)
                    {
                        if let Some(entrada) = entrada_de(m, &mapas) {
                            let (conv, pool, bot_id) = (conv.clone(), pool.clone(), bot_id.clone());
                            tokio::spawn(async move {
                                let guild = entrada.guild_id.to_string();
                                match crate::db::get_guild_settings(&pool, &bot_id, &guild).await {
                                    Ok(ajustes) => {
                                        conv.atender(entrada, &ajustes).await;
                                    }
                                    Err(e) => {
                                        tracing::warn!("No se pudieron leer los ajustes: {e}")
                                    }
                                }
                            });
                        }
                    }
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
    fn intents_son_guilds_miembros_y_mensajes() {
        assert_eq!(
            intents(),
            Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_MESSAGES
        );
        assert!(!intents().contains(Intents::MESSAGE_CONTENT));
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
