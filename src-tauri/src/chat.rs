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
    /// Id del mensaje al que responde, si es una respuesta.
    pub responde_a_mensaje: Option<u64>,
}

/// Lo que el usuario contesta a una propuesta pendiente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmacion {
    Aplicar,
    Descartar,
}

/// «sí», «si», «yes», «ok» o «aplicar» aplican; «no» o «cancelar» descartan.
/// Se ignoran mayúsculas, puntuación alrededor y las menciones (`<@id>`).
pub fn confirmacion_de(texto: &str) -> Option<Confirmacion> {
    let limpio: Vec<&str> = texto
        .split_whitespace()
        .filter(|p| !p.starts_with("<@"))
        .collect();
    if limpio.len() != 1 {
        return None;
    }
    let palabra = limpio[0]
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    match palabra.as_str() {
        "sí" | "si" | "yes" | "ok" | "aplicar" => Some(Confirmacion::Aplicar),
        "no" | "cancelar" => Some(Confirmacion::Descartar),
        _ => None,
    }
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
    let autor = msg.autor_id.to_string();
    if !ajustes.chat_usuarios.is_empty() && !ajustes.chat_usuarios.contains(&autor) {
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

/// Máximo de caracteres del contexto del servidor en el prompt del chat.
pub const MAX_CONTEXTO_CHAT: usize = 1500;

/// Arma el prompt: personalidad, servidor, canal, los últimos `contexto`
/// mensajes del canal y el mensaje a responder.
pub fn prompt_chat(
    personalidad: &str,
    servidor: &str,
    contexto_servidor: &str,
    canal: &str,
    previos: &[MensajeContexto],
    contexto: usize,
    autor: &str,
    mensaje: &str,
    ordenes: bool,
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
    let contexto_servidor: String = contexto_servidor
        .trim()
        .chars()
        .take(MAX_CONTEXTO_CHAT)
        .collect();
    let contexto_servidor = if contexto_servidor.is_empty() {
        "(none)".to_string()
    } else {
        contexto_servidor
    };
    let regla_ordenes = if ordenes {
        "If the user asks for a change in the server (create, edit or delete channels, categories, \
         roles or permissions, give roles...), reply briefly and end with one exact line, complete and on a single line: \
         `ORDEN[estructura]: <the user's request>` when it changes the structure of the server \
         (channels, categories, roles, permissions), or \
         `ORDEN[moderacion]: <the user's request>` when it is about members or their messages \
         (warn, mute, kick, ban, delete messages, give or remove a role to a member...)."
    } else {
        "You cannot make changes in the server; if asked, say so. \
         Never write a line starting with `ORDEN:` or `ORDEN[`."
    };
    format!(
        "{personalidad}\n\n\
         You are a Discord bot chatting in the server \"{servidor}\", channel \"#{canal}\".\n\
         Rules: reply in the same language as the person writing; keep the reply under 1800 characters; \
         never mention @everyone, @here or any role; you have no tools, answer with plain text only.\n\
         {regla_ordenes}\n\n\
         Server context:\n{contexto_servidor}\n\n\
         Recent messages in the channel (author: text):\n{historial}\n\n\
         Message to reply to:\n{autor}: {mensaje}\n"
    )
}

/// Tipo de una orden de Discord: construir/cambiar la estructura o moderar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoOrden {
    Estructura,
    Moderacion,
}

/// Separa la respuesta del modelo en el texto visible y la orden con su tipo
/// (`ORDEN[estructura]: ...`, `ORDEN[moderacion]: ...`; `ORDEN: ...` = estructura).
/// Las líneas de orden nunca forman parte del texto visible. Si el autor no
/// tiene `ordenes_activas`, la orden se descarta aunque el modelo la escriba.
pub fn separar_orden_tipo(
    respuesta: &str,
    autorizado: bool,
) -> (String, Option<(TipoOrden, String)>) {
    let mut visibles: Vec<&str> = Vec::new();
    let mut orden: Option<(TipoOrden, String)> = None;
    for linea in respuesta.lines() {
        match marca_orden(linea.trim_start()) {
            Some((tipo, resto)) => {
                let resto = resto.trim();
                if autorizado && orden.is_none() && !resto.is_empty() {
                    if let Some(tipo) = tipo {
                        orden = Some((tipo, resto.to_string()));
                    }
                }
            }
            None => visibles.push(linea),
        }
    }
    (visibles.join("\n").trim().to_string(), orden)
}

/// Si la línea es una marca de orden devuelve (tipo, resto); el tipo es `None`
/// cuando la marca lleva un tipo desconocido (la línea se oculta y se ignora).
fn marca_orden(linea: &str) -> Option<(Option<TipoOrden>, &str)> {
    let resto = linea.strip_prefix("ORDEN")?;
    if let Some(r) = resto.strip_prefix(':') {
        return Some((Some(TipoOrden::Estructura), r));
    }
    let r = resto.strip_prefix('[')?;
    let (tipo, r) = r.split_once("]:")?;
    let tipo = match tipo.trim().to_lowercase().as_str() {
        "estructura" => Some(TipoOrden::Estructura),
        "moderacion" | "moderación" => Some(TipoOrden::Moderacion),
        _ => None,
    };
    Some((tipo, r))
}

/// Como `separar_orden_tipo`, pero solo devuelve el texto de la orden.
pub fn separar_orden(respuesta: &str, autorizado: bool) -> (String, Option<String>) {
    let (t, o) = separar_orden_tipo(respuesta, autorizado);
    (t, o.map(|(_, texto)| texto))
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
    fn confirmacion_reconoce_las_palabras_y_nada_mas() {
        for t in ["sí", "Si", "YES", "ok!", " <@900> sí. ", "aplicar"] {
            assert_eq!(confirmacion_de(t), Some(Confirmacion::Aplicar), "{t}");
        }
        for t in ["no", "Cancelar", "<@900> no"] {
            assert_eq!(confirmacion_de(t), Some(Confirmacion::Descartar), "{t}");
        }
        for t in ["", "sí pero cambia el nombre", "quizá", "hola"] {
            assert_eq!(confirmacion_de(t), None, "{t}");
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
    fn solo_responde_a_los_usuarios_elegidos() {
        let a = GuildSettings {
            chat_usuarios: vec!["2".into()],
            ..ajustes()
        };
        // Autor 1 no está en la lista.
        assert!(!debe_responder(&mencion(), BOT, &a, None, 100));
        let mut m = mencion();
        m.autor_id = 2;
        assert!(debe_responder(&m, BOT, &a, None, 100));
    }

    #[test]
    fn con_la_lista_de_usuarios_vacia_responde_a_cualquiera() {
        assert!(ajustes().chat_usuarios.is_empty());
        assert!(debe_responder(&mencion(), BOT, &ajustes(), None, 100));
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
            "",
            "general",
            &previos,
            2,
            "Ana",
            "¿Qué tal?",
            false,
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
    fn prompt_incluye_el_contexto_del_servidor_recortado() {
        let p = prompt_chat("P", "S", "Tema: piratas", "c", &[], 10, "A", "hi", false);
        assert!(p.contains("Server context:\nTema: piratas"));
        let largo = "x".repeat(2000);
        let p = prompt_chat("P", "S", &largo, "c", &[], 10, "A", "hi", false);
        assert!(p.contains(&"x".repeat(1500)));
        assert!(!p.contains(&"x".repeat(1501)));
    }

    #[test]
    fn prompt_sin_contexto() {
        let p = prompt_chat("P", "S", "", "c", &[], 10, "A", "hi", false);
        assert!(p.contains("(none)"));
    }

    #[test]
    fn prompt_con_ordenes_pide_la_linea_orden() {
        let p = prompt_chat("P", "S", "", "c", &[], 10, "A", "hi", true);
        assert!(p.contains("`ORDEN[estructura]: <"));
        assert!(p.contains("`ORDEN[moderacion]: <"));
        assert!(p.contains("channels, categories"));
        assert!(!p.contains("cannot make changes"));
    }

    #[test]
    fn prompt_sin_ordenes_prohibe_la_linea_orden() {
        let p = prompt_chat("P", "S", "", "c", &[], 10, "A", "hi", false);
        assert!(p.contains("cannot make changes"));
        assert!(p.contains("Never write a line starting with `ORDEN:`"));
        assert!(!p.contains("ORDEN[estructura]: <"));
    }

    #[test]
    fn separa_las_dos_marcas_con_su_tipo() {
        let (t, o) = separar_orden_tipo("Vale.\nORDEN[estructura]: crea #a", true);
        assert_eq!(t, "Vale.");
        assert_eq!(o, Some((TipoOrden::Estructura, "crea #a".to_string())));
        let (t, o) = separar_orden_tipo("Ok.\nORDEN[moderacion]: banea a Ana", true);
        assert_eq!(t, "Ok.");
        assert_eq!(o, Some((TipoOrden::Moderacion, "banea a Ana".to_string())));
    }

    #[test]
    fn orden_sin_tipo_cuenta_como_estructura() {
        let (_, o) = separar_orden_tipo("ORDEN: crea un rol", true);
        assert_eq!(o, Some((TipoOrden::Estructura, "crea un rol".to_string())));
    }

    #[test]
    fn tipo_desconocido_o_no_autorizado_se_oculta_y_se_descarta() {
        let (t, o) = separar_orden_tipo("Hola\nORDEN[otra]: x", true);
        assert_eq!(t, "Hola");
        assert_eq!(o, None);
        let (t, o) = separar_orden_tipo("Hola\nORDEN[moderacion]: x", false);
        assert_eq!(t, "Hola");
        assert_eq!(o, None);
    }

    #[test]
    fn separa_el_texto_y_la_orden() {
        let (t, o) = separar_orden("Claro, lo hago.\nORDEN: crea un canal #a", true);
        assert_eq!(t, "Claro, lo hago.");
        assert_eq!(o.as_deref(), Some("crea un canal #a"));
    }

    #[test]
    fn sin_orden_devuelve_todo_el_texto() {
        let (t, o) = separar_orden("Hola\nque tal", true);
        assert_eq!(t, "Hola\nque tal");
        assert_eq!(o, None);
    }

    #[test]
    fn descarta_la_orden_de_un_autor_no_autorizado() {
        let (t, o) = separar_orden("No puedo.\nORDEN: borra todo", false);
        assert_eq!(t, "No puedo.");
        assert_eq!(o, None);
    }

    #[test]
    fn la_orden_en_medio_del_texto_no_se_publica() {
        let (t, o) = separar_orden("Uno\nORDEN: haz x\nDos", true);
        assert_eq!(t, "Uno\nDos");
        assert_eq!(o.as_deref(), Some("haz x"));
    }
}
