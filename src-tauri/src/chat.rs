//! Conversación del bot: decisión pura de cuándo responder a un mensaje.

use crate::asistente::OrdenAgente;
use crate::db::GuildSettings;
use std::path::{Path, PathBuf};

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

/// Agente de opencode que ARGUS define para las respuestas en Discord.
pub const AGENTE_CHAT: &str = "argus-chat";

/// Herramientas integradas de claude, todas desactivadas en el chat.
const CLAUDE_TODAS: [&str; 11] = [
    "Bash",
    "Edit",
    "Write",
    "Read",
    "Glob",
    "Grep",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "Task",
    "TodoWrite",
];

/// Herramientas integradas de opencode, todas desactivadas en el chat.
const OPENCODE_TODAS: [&str; 12] = [
    "bash",
    "edit",
    "write",
    "read",
    "glob",
    "grep",
    "list",
    "patch",
    "webfetch",
    "websearch",
    "task",
    "todowrite",
];

/// Carpeta de trabajo propia del chat (no se crea aquí).
pub fn directorio_chat() -> PathBuf {
    std::env::temp_dir().join("argus-chat")
}

/// Orden para conversar con un agente sin ninguna herramienta (ni MCP). El
/// prompt va por stdin y la respuesta es texto plano.
pub fn orden_chat(
    agente: &str,
    ruta_agente: &Path,
    modelo: &str,
    prompt: &str,
) -> Result<OrdenAgente, String> {
    let mut archivos = Vec::new();
    let (args, env) = match agente {
        "claude" => {
            let a: Vec<String> = vec![
                "-p".into(),
                "--output-format".into(),
                "text".into(),
                "--model".into(),
                modelo.into(),
                // Sin ninguna herramienta integrada (también las futuras) ni skills.
                "--tools".into(),
                "".into(),
                "--disable-slash-commands".into(),
                "--strict-mcp-config".into(),
                "--mcp-config".into(),
                serde_json::json!({ "mcpServers": {} }).to_string(),
                "--disallowedTools".into(),
                CLAUDE_TODAS.join(","),
            ];
            (a, Vec::new())
        }
        "opencode" => {
            let mut tools = serde_json::Map::new();
            tools.insert("*".into(), false.into());
            for t in OPENCODE_TODAS {
                tools.insert(t.into(), false.into());
            }
            // Agente propio con TODO denegado: integradas (read, execute…) y los
            // MCP del usuario. opencode v2 lo lee del opencode.json de la carpeta.
            let cfg = serde_json::json!({
                "mcp": {},
                "tools": tools,
                "agent": { AGENTE_CHAT: {
                    "description": "ARGUS Discord replies: text only, no tools",
                    "mode": "primary",
                    "permission": { "*": "deny" },
                } },
            });
            archivos.push(("opencode.json".to_string(), cfg.to_string()));
            (
                vec![
                    "run".into(),
                    "--agent".into(),
                    AGENTE_CHAT.into(),
                    "--model".into(),
                    modelo.into(),
                ],
                vec![("OPENCODE_CONFIG_CONTENT".to_string(), cfg.to_string())],
            )
        }
        otro => return Err(format!("Unknown AI agent: {otro}")),
    };
    Ok(OrdenAgente {
        programa: ruta_agente.to_path_buf(),
        args,
        env,
        stdin: prompt.to_string(),
        directorio: directorio_chat(),
        archivos,
    })
}

/// Mensaje previo del canal para el contexto.
#[derive(Debug, Clone)]
pub struct MensajeContexto {
    pub autor: String,
    pub texto: String,
}

/// Arma el prompt: personalidad, servidor, canal, los últimos `contexto`
/// mensajes del canal y el mensaje a responder.
pub fn prompt_chat(
    personalidad: &str,
    servidor: &str,
    canal: &str,
    previos: &[MensajeContexto],
    contexto: usize,
    autor: &str,
    mensaje: &str,
) -> String {
    let desde = previos.len().saturating_sub(contexto);
    let historial: Vec<String> = previos[desde..]
        .iter()
        .map(|m| format!("{}: {}", m.autor, m.texto))
        .collect();
    let historial = if historial.is_empty() {
        "(none)".to_string()
    } else {
        historial.join("\n")
    };
    format!(
        "{personalidad}\n\n\
         You are a Discord bot chatting in the server \"{servidor}\", channel \"#{canal}\".\n\
         Rules: reply in the same language as the person writing; keep the reply under 1800 characters; \
         never mention @everyone, @here or any role; you have no tools, answer with plain text only.\n\n\
         Recent messages in the channel (author: text):\n{historial}\n\n\
         Message to reply to:\n{autor}: {mensaje}\n"
    )
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

    fn orden(agente: &str) -> OrdenAgente {
        orden_chat(agente, Path::new("/bin/ag"), "mod/x", "hola").unwrap()
    }

    #[test]
    fn orden_claude_sin_herramientas() {
        let o = orden("claude");
        assert_eq!(
            &o.args[..5],
            ["-p", "--output-format", "text", "--model", "mod/x"]
        );
        assert!(o.args.contains(&"--strict-mcp-config".to_string()));
        let i = o.args.iter().position(|a| a == "--mcp-config").unwrap();
        let mcp: serde_json::Value = serde_json::from_str(&o.args[i + 1]).unwrap();
        assert_eq!(mcp["mcpServers"].as_object().unwrap().len(), 0);
        let i = o
            .args
            .iter()
            .position(|a| a == "--disallowedTools")
            .unwrap();
        let prohibidas: Vec<&str> = o.args[i + 1].split(',').collect();
        for t in [
            "Bash",
            "Edit",
            "Write",
            "Read",
            "Glob",
            "Grep",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
            "Task",
            "TodoWrite",
        ] {
            assert!(prohibidas.contains(&t), "falta {t}");
        }
        assert!(!o.args.contains(&"--allowedTools".to_string()));
        // Ninguna herramienta integrada (ni las futuras) y sin skills.
        let i = o.args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(o.args[i + 1], "");
        assert!(o.args.contains(&"--disable-slash-commands".to_string()));
        assert!(o.archivos.is_empty());
        assert_eq!(o.stdin, "hola");
        assert_eq!(o.directorio, directorio_chat());
        assert!(o.env.is_empty());
    }

    #[test]
    fn orden_opencode_sin_herramientas() {
        let o = orden("opencode");
        assert_eq!(o.args, ["run", "--agent", AGENTE_CHAT, "--model", "mod/x"]);
        // opencode v2 lee el agente del opencode.json de la carpeta de trabajo:
        // todo denegado, sin ninguna excepción.
        assert_eq!(o.archivos.len(), 1);
        assert_eq!(o.archivos[0].0, "opencode.json");
        let archivo: serde_json::Value = serde_json::from_str(&o.archivos[0].1).unwrap();
        let permisos = archivo["agent"][AGENTE_CHAT]["permission"]
            .as_object()
            .unwrap();
        assert_eq!(permisos.len(), 1);
        assert_eq!(permisos["*"], "deny");
        assert_eq!(archivo["mcp"].as_object().unwrap().len(), 0);
        let (k, v) = &o.env[0];
        assert_eq!(k, "OPENCODE_CONFIG_CONTENT");
        let cfg: serde_json::Value = serde_json::from_str(v).unwrap();
        assert_eq!(cfg["mcp"].as_object().unwrap().len(), 0);
        let tools = cfg["tools"].as_object().unwrap();
        assert!(!tools.is_empty());
        assert!(tools.values().all(|v| v == false));
        for t in ["bash", "edit", "write", "read", "webfetch"] {
            assert_eq!(tools[t], false);
        }
    }

    #[test]
    fn orden_agente_desconocido() {
        assert!(orden_chat("otro", Path::new("x"), "m", "p").is_err());
    }

    #[test]
    fn prompt_incluye_todo_y_limita_contexto() {
        let previos: Vec<MensajeContexto> = (1..=5)
            .map(|i| MensajeContexto {
                autor: format!("u{i}"),
                texto: format!("t{i}"),
            })
            .collect();
        let p = prompt_chat(
            "Eres Pepe.",
            "Mi Server",
            "general",
            &previos,
            2,
            "Ana",
            "¿Qué tal?",
        );
        assert!(p.starts_with("Eres Pepe."));
        assert!(p.contains("Mi Server") && p.contains("general"));
        assert!(p.contains("u4: t4\nu5: t5"));
        assert!(!p.contains("u3: t3"));
        assert!(p.contains("Ana: ¿Qué tal?"));
        assert!(p.contains("same language"));
        assert!(p.contains("1800"));
        assert!(p.contains("@everyone") && p.contains("@here") && p.contains("role"));
    }

    #[test]
    fn prompt_sin_contexto() {
        let p = prompt_chat("P", "S", "c", &[], 10, "A", "hi");
        assert!(p.contains("(none)"));
    }
}
