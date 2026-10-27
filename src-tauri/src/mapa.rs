//! Mantiene al día un `ServerMap` aplicando eventos del Gateway de Discord.
//!
//! Todo es síncrono y sin red: `aplicar_evento` recibe el mapa y un `Event` de
//! `twilight_model` y devuelve un resumen del cambio (o `None` si el evento no
//! afecta al mapa).

use crate::state::{Member, ServerMap};
use twilight_model::gateway::event::Event;

/// Resumen de un cambio aplicado al mapa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumenCambio {
    /// Tipo del evento de Discord, p. ej. `CHANNEL_CREATE`.
    pub tipo: &'static str,
    /// Descripción legible en español.
    pub descripcion: String,
}

fn resumen(tipo: &'static str, descripcion: String) -> Option<ResumenCambio> {
    Some(ResumenCambio { tipo, descripcion })
}

/// Tipos de canal que son hilos (anuncios, públicos y privados).
fn es_hilo(channel_type: u8) -> bool {
    matches!(channel_type, 10..=12)
}

/// Reconstruye `categories` igual que la sincronización completa: canales (no
/// hilos) agrupados por su `parent_id`.
fn reconstruir_categorias(mapa: &mut ServerMap) {
    mapa.categories.clear();
    for canal in mapa.channels.iter().filter(|c| !es_hilo(c.channel_type)) {
        if let Some(padre) = &canal.parent_id {
            mapa.categories
                .entry(padre.clone())
                .or_default()
                .push(canal.clone());
        }
    }
}

fn tipo_de_canal(channel_type: u8) -> &'static str {
    match channel_type {
        4 => "categoría",
        10..=12 => "hilo",
        _ => "canal",
    }
}

/// Inserta o reemplaza un canal; devuelve `true` si ya existía.
fn guardar_canal(mapa: &mut ServerMap, canal: crate::state::Channel) -> bool {
    let existia = match mapa.channels.iter_mut().find(|c| c.id == canal.id) {
        Some(actual) => {
            *actual = canal;
            true
        }
        None => {
            mapa.channels.push(canal);
            false
        }
    };
    reconstruir_categorias(mapa);
    existia
}

fn quitar_canal(mapa: &mut ServerMap, id: &str) -> Option<crate::state::Channel> {
    let pos = mapa.channels.iter().position(|c| c.id == id)?;
    let canal = mapa.channels.remove(pos);
    reconstruir_categorias(mapa);
    Some(canal)
}

fn canal_cambiado(
    mapa: &mut ServerMap,
    tipo: &'static str,
    c: &twilight_model::channel::Channel,
) -> Option<ResumenCambio> {
    if c.guild_id.is_some_and(|g| g.to_string() != mapa.guild.id) {
        return None;
    }
    let canal = crate::discord::convertir_canal(c.clone());
    let (nombre, clase) = (canal.name.clone(), tipo_de_canal(canal.channel_type));
    let existia = guardar_canal(mapa, canal);
    let accion = if existia { "actualizado" } else { "creado" };
    resumen(tipo, format!("{clase} {nombre} {accion}"))
}

fn rol_cambiado(
    mapa: &mut ServerMap,
    tipo: &'static str,
    rol: &twilight_model::guild::Role,
) -> Option<ResumenCambio> {
    let mut nuevo = crate::discord::convertir_rol(rol.clone());
    let nombre = nuevo.name.clone();
    let existia = match mapa.roles.iter_mut().find(|r| r.id == nuevo.id) {
        Some(actual) => {
            // Discord no informa del recuento por rol: se conserva el nuestro.
            nuevo.member_count = actual.member_count;
            *actual = nuevo;
            true
        }
        None => {
            mapa.roles.push(nuevo);
            false
        }
    };
    let accion = if existia { "actualizado" } else { "creado" };
    resumen(tipo, format!("rol {nombre} {accion}"))
}

fn recontar_rol(mapa: &mut ServerMap, rol_id: &str, delta: i64) {
    if let Some(rol) = mapa.roles.iter_mut().find(|r| r.id == rol_id) {
        rol.member_count = (rol.member_count as i64 + delta).max(0) as u32;
    }
}

/// Aplica un evento del Gateway al mapa. Devuelve un resumen del cambio, o
/// `None` si el evento es de otro servidor o no afecta al mapa.
pub fn aplicar_evento(mapa: &mut ServerMap, evento: &Event) -> Option<ResumenCambio> {
    let guild_id = mapa.guild.id.clone();
    let es_de_otro = |g: &dyn ToString| g.to_string() != guild_id;

    match evento {
        Event::GuildUpdate(g) => {
            if es_de_otro(&g.id) {
                return None;
            }
            mapa.guild.name = g.name.clone();
            mapa.guild.icon = g.icon.map(|i| i.to_string());
            resumen(
                "GUILD_UPDATE",
                format!("servidor actualizado: {}", mapa.guild.name),
            )
        }
        Event::ChannelCreate(c) => canal_cambiado(mapa, "CHANNEL_CREATE", c),
        Event::ChannelUpdate(c) => canal_cambiado(mapa, "CHANNEL_UPDATE", c),
        Event::ChannelDelete(c) => {
            if c.guild_id.is_some_and(|g| es_de_otro(&g)) {
                return None;
            }
            let quitado = quitar_canal(mapa, &c.id.to_string())?;
            resumen(
                "CHANNEL_DELETE",
                format!(
                    "{} {} eliminado",
                    tipo_de_canal(quitado.channel_type),
                    quitado.name
                ),
            )
        }
        Event::ThreadCreate(c) => canal_cambiado(mapa, "THREAD_CREATE", c),
        Event::ThreadUpdate(c) => canal_cambiado(mapa, "THREAD_UPDATE", c),
        Event::ThreadDelete(t) => {
            if es_de_otro(&t.guild_id) {
                return None;
            }
            let quitado = quitar_canal(mapa, &t.id.to_string())?;
            resumen("THREAD_DELETE", format!("hilo {} eliminado", quitado.name))
        }
        Event::RoleCreate(r) => {
            if es_de_otro(&r.guild_id) {
                return None;
            }
            rol_cambiado(mapa, "ROLE_CREATE", &r.role)
        }
        Event::RoleUpdate(r) => {
            if es_de_otro(&r.guild_id) {
                return None;
            }
            rol_cambiado(mapa, "ROLE_UPDATE", &r.role)
        }
        Event::RoleDelete(r) => {
            if es_de_otro(&r.guild_id) {
                return None;
            }
            let id = r.role_id.to_string();
            let pos = mapa.roles.iter().position(|x| x.id == id)?;
            let rol = mapa.roles.remove(pos);
            // El rol borrado desaparece también de los miembros.
            for m in mapa.members.iter_mut() {
                m.roles.retain(|x| *x != id);
            }
            resumen("ROLE_DELETE", format!("rol {} eliminado", rol.name))
        }
        Event::MemberAdd(m) => {
            if es_de_otro(&m.guild_id) {
                return None;
            }
            let nuevo = crate::discord::convertir_miembro(m.member.clone());
            let nombre = nuevo.username.clone();
            let id = nuevo.user_id.clone();
            let roles = nuevo.roles.clone();
            if let Some(pos) = mapa.members.iter().position(|x| x.user_id == id) {
                mapa.members[pos] = nuevo;
            } else {
                mapa.members.push(nuevo);
                mapa.guild.member_count = mapa.guild.member_count.saturating_add(1);
                for r in &roles {
                    recontar_rol(mapa, r, 1);
                }
            }
            resumen("GUILD_MEMBER_ADD", format!("{nombre} se unió al servidor"))
        }
        Event::MemberUpdate(m) => {
            if es_de_otro(&m.guild_id) {
                return None;
            }
            let id = m.user.id.to_string();
            let roles: Vec<String> = m.roles.iter().map(|r| r.to_string()).collect();
            let nombre = m.user.name.clone();
            let avatar = m.avatar.or(m.user.avatar).map(|a| a.to_string());
            let (antes, despues) = match mapa.members.iter_mut().find(|x| x.user_id == id) {
                Some(actual) => {
                    let antes = std::mem::replace(&mut actual.roles, roles.clone());
                    actual.username = nombre.clone();
                    actual.nickname = m.nick.clone();
                    actual.avatar = avatar;
                    actual.bot = m.user.bot;
                    (antes, roles)
                }
                None => {
                    // Miembro aún no conocido: se añade con lo que trae el evento.
                    mapa.members.push(Member {
                        user_id: id,
                        username: nombre.clone(),
                        nickname: m.nick.clone(),
                        roles: roles.clone(),
                        bot: m.user.bot,
                        joined_at: Some(m.joined_at.iso_8601().to_string()),
                        avatar,
                        flags: None,
                    });
                    (Vec::new(), roles)
                }
            };
            for r in antes.iter().filter(|r| !despues.contains(r)) {
                recontar_rol(mapa, r, -1);
            }
            for r in despues.iter().filter(|r| !antes.contains(r)) {
                recontar_rol(mapa, r, 1);
            }
            resumen(
                "GUILD_MEMBER_UPDATE",
                format!("miembro {nombre} actualizado"),
            )
        }
        Event::MemberRemove(m) => {
            if es_de_otro(&m.guild_id) {
                return None;
            }
            let id = m.user.id.to_string();
            let pos = mapa.members.iter().position(|x| x.user_id == id)?;
            let miembro = mapa.members.remove(pos);
            mapa.guild.member_count = mapa.guild.member_count.saturating_sub(1);
            for r in &miembro.roles {
                recontar_rol(mapa, r, -1);
            }
            resumen(
                "GUILD_MEMBER_REMOVE",
                format!("{} salió del servidor", miembro.username),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Guild, Role};
    use serde_json::json;
    use std::collections::HashMap;

    const GUILD: &str = "100";

    fn mapa_vacio() -> ServerMap {
        ServerMap {
            guild: Guild {
                id: GUILD.into(),
                name: "Prueba".into(),
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

    /// Construye un `Event` desde el JSON del Gateway (`t` + `d`).
    fn evento(tipo: &str, d: serde_json::Value) -> Event {
        use serde::de::DeserializeSeed;
        let texto = json!({"op": 0, "s": 1, "t": tipo, "d": d}).to_string();
        let seed = twilight_model::gateway::event::GatewayEventDeserializer::from_json(&texto)
            .expect("evento válido");
        let mut de = serde_json::Deserializer::from_str(&texto);
        let ev = seed.deserialize(&mut de).expect("deserializa");
        twilight_model::gateway::event::GatewayEvent::into(ev)
    }

    fn canal_json(id: &str, nombre: &str, tipo: u8, padre: Option<&str>) -> serde_json::Value {
        json!({"id": id, "type": tipo, "guild_id": GUILD, "name": nombre,
               "parent_id": padre, "position": 1})
    }

    fn rol_json(id: &str, nombre: &str) -> serde_json::Value {
        json!({"id": id, "name": nombre, "color": 0, "hoist": false, "managed": false,
               "mentionable": true, "permissions": "8", "position": 2, "flags": 0})
    }

    fn miembro_json(id: &str, nombre: &str, roles: &[&str]) -> serde_json::Value {
        json!({"guild_id": GUILD, "user": {"id": id, "username": nombre,
               "discriminator": "0", "avatar": null},
               "roles": roles, "joined_at": "2024-01-01T00:00:00.000000+00:00",
               "deaf": false, "mute": false, "flags": 0})
    }

    fn rol(id: &str) -> Role {
        Role {
            id: id.into(),
            name: "r".into(),
            color: 0,
            position: 0,
            permissions: "0".into(),
            permissions_bitfield: None,
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }

    #[test]
    fn guild_update_cambia_nombre() {
        let mut mapa = mapa_vacio();
        let ev = evento(
            "GUILD_UPDATE",
            json!({"id": GUILD, "name": "Nuevo", "owner_id": "1", "afk_timeout": 60,
                   "default_message_notifications": 0, "explicit_content_filter": 0,
                   "features": [], "mfa_level": 0, "nsfw_level": 0, "icon": null,
                   "preferred_locale": "es-ES", "premium_tier": 0, "roles": [],
                   "system_channel_flags": 0, "verification_level": 0,
                   "emojis": [], "premium_progress_bar_enabled": false}),
        );
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "GUILD_UPDATE");
        assert_eq!(mapa.guild.name, "Nuevo");
    }

    #[test]
    fn canal_create_update_delete() {
        let mut mapa = mapa_vacio();
        let ev = evento("CHANNEL_CREATE", canal_json("5", "general", 0, Some("4")));
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "CHANNEL_CREATE");
        assert_eq!(mapa.channels.len(), 1);
        assert_eq!(mapa.categories["4"].len(), 1);

        let ev = evento("CHANNEL_UPDATE", canal_json("5", "charla", 0, Some("6")));
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "CHANNEL_UPDATE");
        assert_eq!(mapa.channels.len(), 1);
        assert_eq!(mapa.channels[0].name, "charla");
        assert!(!mapa.categories.contains_key("4"));
        assert_eq!(mapa.categories["6"][0].name, "charla");

        let ev = evento("CHANNEL_DELETE", canal_json("5", "charla", 0, Some("6")));
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "CHANNEL_DELETE");
        assert!(mapa.channels.is_empty());
        assert!(mapa.categories.is_empty());
        // Borrar uno desconocido no cambia nada.
        assert!(aplicar_evento(&mut mapa, &ev).is_none());
    }

    #[test]
    fn eventos_de_otro_servidor_se_ignoran() {
        let mut mapa = mapa_vacio();
        let mut c = canal_json("5", "x", 0, None);
        c["guild_id"] = json!("999");
        assert!(aplicar_evento(&mut mapa, &evento("CHANNEL_CREATE", c)).is_none());
        assert!(mapa.channels.is_empty());
    }

    #[test]
    fn role_create_update_delete() {
        let mut mapa = mapa_vacio();
        mapa.members.push(Member {
            user_id: "7".into(),
            username: "u".into(),
            nickname: None,
            roles: vec!["30".into(), "31".into()],
            bot: false,
            joined_at: None,
            avatar: None,
            flags: None,
        });
        let ev = evento(
            "GUILD_ROLE_CREATE",
            json!({"guild_id": GUILD, "role": rol_json("30", "Mod")}),
        );
        assert_eq!(aplicar_evento(&mut mapa, &ev).unwrap().tipo, "ROLE_CREATE");
        assert_eq!(mapa.roles[0].name, "Mod");
        mapa.roles[0].member_count = 4;

        let ev = evento(
            "GUILD_ROLE_UPDATE",
            json!({"guild_id": GUILD, "role": rol_json("30", "Admin")}),
        );
        assert_eq!(aplicar_evento(&mut mapa, &ev).unwrap().tipo, "ROLE_UPDATE");
        assert_eq!(mapa.roles.len(), 1);
        assert_eq!(mapa.roles[0].name, "Admin");
        assert_eq!(mapa.roles[0].member_count, 4);

        let ev = evento(
            "GUILD_ROLE_DELETE",
            json!({"guild_id": GUILD, "role_id": "30"}),
        );
        assert_eq!(aplicar_evento(&mut mapa, &ev).unwrap().tipo, "ROLE_DELETE");
        assert!(mapa.roles.is_empty());
        assert_eq!(mapa.members[0].roles, vec!["31".to_string()]);
    }

    #[test]
    fn miembro_add_update_remove() {
        let mut mapa = mapa_vacio();
        mapa.roles.push(rol("30"));
        mapa.roles.push(rol("31"));

        let ev = evento("GUILD_MEMBER_ADD", miembro_json("7", "ana", &["30"]));
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "GUILD_MEMBER_ADD");
        assert_eq!(mapa.members.len(), 1);
        assert_eq!(mapa.guild.member_count, 1);
        assert_eq!(mapa.roles[0].member_count, 1);

        let mut upd = miembro_json("7", "ana", &["31"]);
        upd["nick"] = json!("Anita");
        let r = aplicar_evento(&mut mapa, &evento("GUILD_MEMBER_UPDATE", upd)).unwrap();
        assert_eq!(r.tipo, "GUILD_MEMBER_UPDATE");
        assert_eq!(mapa.members.len(), 1);
        assert_eq!(mapa.members[0].nickname.as_deref(), Some("Anita"));
        assert_eq!(mapa.members[0].roles, vec!["31".to_string()]);
        assert_eq!(mapa.roles[0].member_count, 0);
        assert_eq!(mapa.roles[1].member_count, 1);

        let ev = evento(
            "GUILD_MEMBER_REMOVE",
            json!({"guild_id": GUILD, "user": {"id": "7", "username": "ana",
                   "discriminator": "0", "avatar": null}}),
        );
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "GUILD_MEMBER_REMOVE");
        assert!(mapa.members.is_empty());
        assert_eq!(mapa.guild.member_count, 0);
        assert_eq!(mapa.roles[1].member_count, 0);
        assert!(aplicar_evento(&mut mapa, &ev).is_none());
    }

    #[test]
    fn hilos_create_update_delete() {
        let mut mapa = mapa_vacio();
        let hilo = |nombre: &str| canal_json("50", nombre, 11, Some("5"));
        let r = aplicar_evento(&mut mapa, &evento("THREAD_CREATE", hilo("tema"))).unwrap();
        assert_eq!(r.tipo, "THREAD_CREATE");
        assert_eq!(mapa.channels.len(), 1);
        // Los hilos no cuentan como hijos de una categoría.
        assert!(mapa.categories.is_empty());

        let r = aplicar_evento(&mut mapa, &evento("THREAD_UPDATE", hilo("tema2"))).unwrap();
        assert_eq!(r.tipo, "THREAD_UPDATE");
        assert_eq!(mapa.channels[0].name, "tema2");

        let ev = evento(
            "THREAD_DELETE",
            json!({"id": "50", "guild_id": GUILD, "parent_id": "5", "type": 11}),
        );
        let r = aplicar_evento(&mut mapa, &ev).unwrap();
        assert_eq!(r.tipo, "THREAD_DELETE");
        assert!(mapa.channels.is_empty());
    }

    #[test]
    fn evento_irrelevante_no_cambia_nada() {
        let mut mapa = mapa_vacio();
        assert!(aplicar_evento(&mut mapa, &Event::GatewayHeartbeatAck).is_none());
    }
}
