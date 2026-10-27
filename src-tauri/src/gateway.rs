//! Conexión al Gateway de Discord (twilight-gateway) mientras el bot está activo.
//! Cada evento recibido se aplica a los mapas con `mapa::aplicar_evento`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use twilight_gateway::{Intents, Shard, ShardId};
use twilight_model::gateway::event::Event;

use crate::mapa::{aplicar_evento, ResumenCambio};
use crate::state::ServerMap;

/// Intents que ARGUS necesita: servidores/canales/roles y miembros.
pub fn intents() -> Intents {
    Intents::GUILDS | Intents::GUILD_MEMBERS
}

/// Aplica un evento a todos los mapas (cada uno ignora los de otros servidores).
/// El lock se toma y se suelta aquí, sin `.await` de por medio.
pub fn aplicar_a_mapas(
    mapas: &Mutex<HashMap<String, ServerMap>>,
    evento: &Event,
) -> Vec<ResumenCambio> {
    let mut mapas = mapas.lock().unwrap();
    mapas
        .values_mut()
        .filter_map(|m| aplicar_evento(m, evento))
        .collect()
}

/// Lanza la tarea del Gateway. Twilight gestiona heartbeats y reconexión.
pub fn lanzar(
    token: String,
    mapas: Arc<Mutex<HashMap<String, ServerMap>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut shard = Shard::new(ShardId::ONE, token, intents());
        loop {
            match shard.next_event().await {
                Ok(evento) => {
                    for cambio in aplicar_a_mapas(&mapas, &evento) {
                        tracing::info!("Mapa actualizado: {}", cambio.tipo);
                    }
                }
                Err(e) => {
                    tracing::warn!("Error del Gateway: {e}");
                    if e.is_fatal() {
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

    #[test]
    fn evento_sin_mapas_no_cambia_nada() {
        let mapas = Mutex::new(HashMap::new());
        assert!(aplicar_a_mapas(&mapas, &Event::GatewayHeartbeatAck).is_empty());
    }
}
