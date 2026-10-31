//! Conversación del bot: decisión pura de cuándo responder a un mensaje.

use crate::db::GuildSettings;

/// Datos de un mensaje de servidor que importan para decidir si responder.
#[derive(Debug, Clone, Default)]
pub struct MensajeEntrante {
    pub autor_id: u64,
    pub autor_es_bot: bool,
    pub canal_id: u64,
    /// Solo menciones directas de usuarios; `@everyone`, `@here` y las
    /// menciones de rol no cuentan y no se incluyen aquí.
    pub menciones: Vec<u64>,
    /// Autor del mensaje al que responde, si es una respuesta.
    pub responde_a_autor: Option<u64>,
}

/// Devuelve true si el bot debe responder. `ultima_respuesta_s` es la hora
/// (segundos) del último mensaje respondido a este usuario y `ahora_s` la actual.
pub fn debe_responder(
    msg: &MensajeEntrante,
    bot_id: u64,
    ajustes: &GuildSettings,
    ultima_respuesta_s: Option<u64>,
    ahora_s: u64,
) -> bool {
    if !ajustes.personality_enabled || msg.autor_es_bot || msg.autor_id == bot_id {
        return false;
    }
    let dirigido = msg.menciones.contains(&bot_id) || msg.responde_a_autor == Some(bot_id);
    if !dirigido {
        return false;
    }
    let canal = msg.canal_id.to_string();
    if !ajustes.chat_canales.is_empty() && !ajustes.chat_canales.contains(&canal) {
        return false;
    }
    match ultima_respuesta_s {
        Some(ultima) => ahora_s.saturating_sub(ultima) >= u64::from(ajustes.chat_espera_s),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOT: u64 = 900;

    fn ajustes() -> GuildSettings {
        GuildSettings {
            personality_enabled: true,
            ..GuildSettings::default()
        }
    }

    fn mencion() -> MensajeEntrante {
        MensajeEntrante {
            autor_id: 1,
            canal_id: 10,
            menciones: vec![BOT],
            ..Default::default()
        }
    }

    #[test]
    fn responde_a_una_mencion() {
        assert!(debe_responder(&mencion(), BOT, &ajustes(), None, 100));
    }

    #[test]
    fn responde_a_una_respuesta_a_un_mensaje_del_bot() {
        let m = MensajeEntrante {
            autor_id: 1,
            canal_id: 10,
            responde_a_autor: Some(BOT),
            ..Default::default()
        };
        assert!(debe_responder(&m, BOT, &ajustes(), None, 100));
    }

    #[test]
    fn no_responde_si_la_personalidad_esta_desactivada() {
        let a = GuildSettings {
            personality_enabled: false,
            ..ajustes()
        };
        assert!(!debe_responder(&mencion(), BOT, &a, None, 100));
    }

    #[test]
    fn no_responde_a_bots_ni_a_si_mismo() {
        let mut m = mencion();
        m.autor_es_bot = true;
        assert!(!debe_responder(&m, BOT, &ajustes(), None, 100));
        let mut m = mencion();
        m.autor_id = BOT;
        assert!(!debe_responder(&m, BOT, &ajustes(), None, 100));
    }

    #[test]
    fn no_responde_a_un_mensaje_normal() {
        let m = MensajeEntrante {
            autor_id: 1,
            canal_id: 10,
            menciones: vec![42],
            responde_a_autor: Some(42),
            ..Default::default()
        };
        assert!(!debe_responder(&m, BOT, &ajustes(), None, 100));
    }

    #[test]
    fn everyone_here_y_roles_no_cuentan() {
        // @everyone/@here/rol no añaden al bot a las menciones de usuario.
        let m = MensajeEntrante {
            autor_id: 1,
            canal_id: 10,
            ..Default::default()
        };
        assert!(!debe_responder(&m, BOT, &ajustes(), None, 100));
    }

    #[test]
    fn respeta_los_canales_permitidos() {
        let mut a = ajustes();
        a.chat_canales = vec!["11".into()];
        assert!(!debe_responder(&mencion(), BOT, &a, None, 100));
        a.chat_canales = vec!["11".into(), "10".into()];
        assert!(debe_responder(&mencion(), BOT, &a, None, 100));
    }

    #[test]
    fn respeta_la_espera_por_usuario() {
        let a = ajustes(); // 10 s
        assert!(!debe_responder(&mencion(), BOT, &a, Some(95), 100));
        assert!(debe_responder(&mencion(), BOT, &a, Some(90), 100));
        assert!(debe_responder(&mencion(), BOT, &a, Some(50), 100));
        // Reloj que retrocede: no responde hasta que pase la espera.
        assert!(!debe_responder(&mencion(), BOT, &a, Some(200), 100));
    }
}
