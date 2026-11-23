//! Encuestas nativas de Discord (mensajes con `poll`).

use crate::discord::{parse_id, pedir_json, validar_emoji, EmojiReaccion};
use serde_json::{json, Value};
use twilight_http::routing::Route;
use twilight_model::id::marker::{ChannelMarker, MessageMarker};

pub const MAX_PREGUNTA: usize = 300;
pub const MAX_RESPUESTA: usize = 55;
pub const MIN_RESPUESTAS: usize = 2;
pub const MAX_RESPUESTAS: usize = 10;
pub const MIN_HORAS: u32 = 1;
pub const MAX_HORAS: u32 = 768;

/// Una opción de la encuesta; `emoji` es Unicode o `nombre:id`.
#[derive(Debug, Clone, Default)]
pub struct RespuestaEncuesta {
    pub texto: String,
    pub emoji: Option<String>,
}

/// Valida los límites y construye el objeto `poll` de Discord.
pub fn construir_poll(
    pregunta: &str,
    respuestas: &[RespuestaEncuesta],
    horas: u32,
    multiple: bool,
) -> Result<Value, String> {
    if pregunta.trim().is_empty() {
        return Err("The poll question cannot be empty".to_string());
    }
    if pregunta.chars().count() > MAX_PREGUNTA {
        return Err("The poll question is too long (the maximum is 300 characters)".to_string());
    }
    if !(MIN_RESPUESTAS..=MAX_RESPUESTAS).contains(&respuestas.len()) {
        return Err("A poll needs between 2 and 10 answers".to_string());
    }
    if !(MIN_HORAS..=MAX_HORAS).contains(&horas) {
        return Err("The poll duration must be between 1 and 768 hours".to_string());
    }
    let mut lista = Vec::new();
    for r in respuestas {
        if r.texto.trim().is_empty() {
            return Err("A poll answer cannot be empty".to_string());
        }
        if r.texto.chars().count() > MAX_RESPUESTA {
            return Err("A poll answer is too long (the maximum is 55 characters)".to_string());
        }
        let mut media = json!({"text": r.texto.trim()});
        if let Some(e) = r.emoji.as_deref().filter(|e| !e.trim().is_empty()) {
            media["emoji"] = match validar_emoji(e)? {
                EmojiReaccion::Unicode(n) => json!({"name": n}),
                EmojiReaccion::Personalizado { nombre, id } => json!({"name": nombre, "id": id}),
            };
        }
        lista.push(json!({"poll_media": media}));
    }
    Ok(json!({
        "question": {"text": pregunta.trim()},
        "answers": lista,
        "duration": horas,
        "allow_multiselect": multiple,
        "layout_type": 1,
    }))
}

/// `POST /channels/{id}/messages` con `poll`. Devuelve el id del mensaje.
pub async fn crear(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    pregunta: &str,
    respuestas: &[RespuestaEncuesta],
    horas: u32,
    multiple: bool,
) -> Result<String, String> {
    let cid = parse_id::<ChannelMarker>(channel_id.trim(), "channel")?;
    let poll = construir_poll(pregunta, respuestas, horas, multiple)?;
    let r = pedir_json(
        token,
        base_url,
        Route::CreateMessage {
            channel_id: cid.get(),
        },
        Some(json!({"poll": poll})),
    )
    .await?;
    r.get("id")
        .and_then(|i| i.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Invalid response from Discord: missing message id".to_string())
}

/// Recuentos por respuesta a partir del mensaje de Discord.
pub fn resumen_resultados(mensaje: &Value) -> Result<Value, String> {
    let poll = mensaje
        .get("poll")
        .filter(|p| p.is_object())
        .ok_or_else(|| "That message has no poll".to_string())?;
    let cuentas = poll
        .get("results")
        .and_then(|r| r.get("answer_counts"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    let mut total = 0u64;
    let respuestas: Vec<Value> = poll
        .get("answers")
        .and_then(|a| a.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|r| {
            let id = r.get("answer_id").and_then(|i| i.as_u64());
            let votos = cuentas
                .iter()
                .find(|c| c.get("id").and_then(|i| i.as_u64()) == id)
                .and_then(|c| c.get("count").and_then(|n| n.as_u64()))
                .unwrap_or(0);
            total += votos;
            let media = r.get("poll_media");
            json!({
                "id": id,
                "texto": media.and_then(|m| m.get("text")).and_then(|t| t.as_str()),
                "emoji": media.and_then(|m| m.get("emoji")).and_then(|e| e.get("name")).and_then(|t| t.as_str()),
                "votos": votos,
            })
        })
        .collect();
    Ok(json!({
        "pregunta": poll.get("question").and_then(|q| q.get("text")).and_then(|t| t.as_str()),
        "multiple": poll.get("allow_multiselect").and_then(|b| b.as_bool()).unwrap_or(false),
        "terminada": poll
            .get("results")
            .and_then(|r| r.get("is_finalized"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        "expira": poll.get("expiry").and_then(|e| e.as_str()),
        "total_votos": total,
        "respuestas": respuestas,
    }))
}

/// `GET /channels/{id}/messages/{id}` y recuento por respuesta.
pub async fn resultados(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    message_id: &str,
) -> Result<Value, String> {
    let cid = parse_id::<ChannelMarker>(channel_id.trim(), "channel")?;
    let mid = parse_id::<MessageMarker>(message_id.trim(), "message")?;
    let m = pedir_json(
        token,
        base_url,
        Route::GetMessage {
            channel_id: cid.get(),
            message_id: mid.get(),
        },
        None,
    )
    .await?;
    resumen_resultados(&m)
}

/// `POST /channels/{id}/polls/{id}/expire`: termina la encuesta del bot.
/// twilight 0.15 no tiene esta ruta, así que se hace la petición a mano.
pub async fn cerrar(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    message_id: &str,
) -> Result<(), String> {
    use hyper::body::HttpBody;
    let cid = parse_id::<ChannelMarker>(channel_id.trim(), "channel")?;
    let mid = parse_id::<MessageMarker>(message_id.trim(), "message")?;
    let url = match base_url {
        Some(b) => format!("http://{b}/api/v10/channels/{cid}/polls/{mid}/expire"),
        None => format!("https://discord.com/api/v10/channels/{cid}/polls/{mid}/expire"),
    };
    let peticion = hyper::Request::builder()
        .method("POST")
        .uri(url)
        .header("authorization", format!("Bot {token}"))
        .header("user-agent", "DiscordBot (argus, 1)")
        .header("content-length", "0")
        .body(hyper::Body::empty())
        .map_err(|e| format!("Invalid request: {e}"))?;
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build();
    let cliente = hyper::Client::builder().build::<_, hyper::Body>(https);
    let enviar = async {
        let resp = cliente
            .request(peticion)
            .await
            .map_err(|e| format!("Could not reach Discord: {e}"))?;
        let estado = resp.status();
        let mut cuerpo = resp.into_body();
        let mut bytes = Vec::new();
        while let Some(t) = cuerpo.data().await {
            bytes.extend_from_slice(&t.map_err(|e| format!("Invalid response from Discord: {e}"))?);
        }
        if estado.is_success() {
            return Ok(());
        }
        let msg = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();
        Err(format!(
            "Discord rejected the request (HTTP {}): {msg}",
            estado.as_u16()
        ))
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), enviar)
        .await
        .map_err(|_| "Could not reach Discord: timed out".to_string())?
}
