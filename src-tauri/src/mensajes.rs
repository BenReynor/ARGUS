//! Mensajes, invitaciones y webhooks de Discord (REST con twilight).
//!
//! Se usan peticiones directas con `serde_json::Value`, igual que el registro de
//! auditoría, para no depender de los tipos de twilight 0.15.

use serde_json::Value;
use twilight_http::routing::Route;

use crate::discord::{build_client, traducir_error};

/// Mensajes que devuelve `channel_messages`.
pub const MENSAJES_LIMITE: u16 = 50;
/// Antigüedad máxima (ms) para borrar en masa: 14 días.
const CATORCE_DIAS_MS: i64 = 14 * 24 * 60 * 60 * 1000;
const EPOCA_DISCORD_MS: i64 = 1_420_070_400_000;
/// Caducidad máxima de una invitación (7 días, en segundos).
pub const INVITACION_MAX_EDAD: u32 = 604_800;
pub const INVITACION_MAX_USOS: u32 = 100;

fn id(texto: &str, que: &str) -> Result<u64, String> {
    texto
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("ID de {que} inválido"))
}

/// Envía la petición y devuelve el JSON de la respuesta (`Null` si viene vacía).
async fn pedir(
    token: &str,
    base_url: Option<&str>,
    ruta: Route<'_>,
    cuerpo: Option<&Value>,
) -> Result<Value, String> {
    let mut b = twilight_http::request::Request::builder(&ruta);
    if let Some(c) = cuerpo {
        b = b.json(c).map_err(|e| format!("Petición inválida: {e}"))?;
    }
    let bytes = build_client(token, base_url)
        .request::<twilight_http::response::marker::EmptyBody>(b.build())
        .await
        .map_err(traducir_error)?
        .bytes()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Respuesta de Discord inválida: {e}"))
}

fn texto(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}

fn numero(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(|x| x.as_u64()).unwrap_or(0) as u32
}

// ---------------------------------------------------------------- mensajes

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MensajeCanal {
    pub id: String,
    pub content: String,
    pub author_id: String,
    pub author_name: String,
    pub author_bot: bool,
    /// ISO 8601 UTC, tal como lo envía Discord.
    pub timestamp: String,
    pub pinned: bool,
}

/// Últimos 50 mensajes de un canal (`GET /channels/{id}/messages`).
pub async fn fetch_channel_messages(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
) -> Result<Vec<MensajeCanal>, String> {
    let cid = id(channel_id, "canal")?;
    let json = pedir(
        token,
        base_url,
        Route::GetMessages {
            after: None,
            around: None,
            before: None,
            channel_id: cid,
            limit: Some(MENSAJES_LIMITE),
        },
        None,
    )
    .await?;
    Ok(json
        .as_array()
        .map(|a| {
            a.iter()
                .take(MENSAJES_LIMITE as usize)
                .map(|m| {
                    let autor = m.get("author").cloned().unwrap_or(Value::Null);
                    MensajeCanal {
                        id: texto(m, "id").unwrap_or_default(),
                        content: texto(m, "content").unwrap_or_default(),
                        author_id: texto(&autor, "id").unwrap_or_default(),
                        author_name: texto(&autor, "global_name")
                            .or_else(|| texto(&autor, "username"))
                            .unwrap_or_else(|| "Desconocido".to_string()),
                        author_bot: autor.get("bot").and_then(|b| b.as_bool()).unwrap_or(false),
                        timestamp: texto(m, "timestamp").unwrap_or_default(),
                        pinned: m.get("pinned").and_then(|b| b.as_bool()).unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete_message(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let channel_id = id(channel_id, "canal")?;
    let message_id = id(message_id, "mensaje")?;
    pedir(
        token,
        base_url,
        Route::DeleteMessage {
            channel_id,
            message_id,
        },
        None,
    )
    .await
    .map(|_| ())
}

/// Valida los ids del borrado en masa: 2–100, únicos y de menos de 14 días.
pub fn validar_borrado_masivo(ids: &[String], ahora_ms: i64) -> Result<Vec<String>, String> {
    let mut limpios: Vec<String> = Vec::new();
    for m in ids {
        id(m, "mensaje")?;
        let m = m.trim().to_string();
        if !limpios.contains(&m) {
            limpios.push(m);
        }
    }
    if limpios.len() < 2 || limpios.len() > 100 {
        return Err("El borrado en masa necesita entre 2 y 100 mensajes".to_string());
    }
    let viejos = limpios
        .iter()
        .filter(|m| {
            let creado = ((m.parse::<u64>().unwrap_or(0) >> 22) as i64) + EPOCA_DISCORD_MS;
            ahora_ms - creado >= CATORCE_DIAS_MS
        })
        .count();
    if viejos > 0 {
        return Err(format!(
            "Discord no permite borrar en masa mensajes de más de 14 días: {viejos} de {} son más antiguos. Quítalos de la selección o bórralos uno a uno",
            limpios.len()
        ));
    }
    Ok(limpios)
}

pub async fn bulk_delete_messages(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    message_ids: &[String],
) -> Result<usize, String> {
    let channel_id = id(channel_id, "canal")?;
    let limpios = validar_borrado_masivo(message_ids, chrono::Utc::now().timestamp_millis())?;
    pedir(
        token,
        base_url,
        Route::DeleteMessages { channel_id },
        Some(&serde_json::json!({ "messages": limpios })),
    )
    .await?;
    Ok(limpios.len())
}

pub async fn set_pin(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    message_id: &str,
    fijar: bool,
) -> Result<(), String> {
    let channel_id = id(channel_id, "canal")?;
    let message_id = id(message_id, "mensaje")?;
    let ruta = if fijar {
        Route::PinMessage {
            channel_id,
            message_id,
        }
    } else {
        Route::UnpinMessage {
            channel_id,
            message_id,
        }
    };
    pedir(token, base_url, ruta, None).await.map(|_| ())
}

// ------------------------------------------------------------ invitaciones

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Invitacion {
    pub code: String,
    pub url: String,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub inviter: Option<String>,
    pub uses: u32,
    /// 0 = sin límite.
    pub max_uses: u32,
    /// Segundos; 0 = no caduca.
    pub max_age: u32,
    pub expires_at: Option<String>,
}

fn invitacion(v: &Value) -> Invitacion {
    let code = texto(v, "code").unwrap_or_default();
    let canal = v.get("channel").cloned().unwrap_or(Value::Null);
    Invitacion {
        url: format!("https://discord.gg/{code}"),
        code,
        channel_id: texto(&canal, "id"),
        channel_name: texto(&canal, "name"),
        inviter: v
            .get("inviter")
            .and_then(|i| texto(i, "global_name").or_else(|| texto(i, "username"))),
        uses: numero(v, "uses"),
        max_uses: numero(v, "max_uses"),
        max_age: numero(v, "max_age"),
        expires_at: texto(v, "expires_at"),
    }
}

/// Crea una invitación (`POST /channels/{id}/invites`). `max_age` en segundos
/// (0 = no caduca, máx. 7 días) y `max_uses` (0 = ilimitado, máx. 100).
pub async fn create_invite(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    max_age: u32,
    max_uses: u32,
) -> Result<Invitacion, String> {
    let channel_id = id(channel_id, "canal")?;
    if max_age > INVITACION_MAX_EDAD {
        return Err("La caducidad no puede superar 7 días (604800 segundos)".to_string());
    }
    if max_uses > INVITACION_MAX_USOS {
        return Err("Los usos máximos no pueden superar 100".to_string());
    }
    let json = pedir(
        token,
        base_url,
        Route::CreateInvite { channel_id },
        Some(&serde_json::json!({
            "max_age": max_age, "max_uses": max_uses, "temporary": false, "unique": true
        })),
    )
    .await?;
    Ok(invitacion(&json))
}

/// Invitaciones del servidor (`GET /guilds/{id}/invites`).
pub async fn fetch_invites(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<Invitacion>, String> {
    let guild_id = id(guild_id, "servidor")?;
    let json = pedir(token, base_url, Route::GetGuildInvites { guild_id }, None).await?;
    Ok(json
        .as_array()
        .map(|a| a.iter().map(invitacion).collect())
        .unwrap_or_default())
}

pub async fn delete_invite(token: &str, base_url: Option<&str>, code: &str) -> Result<(), String> {
    let code = code.trim();
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Código de invitación inválido".to_string());
    }
    pedir(token, base_url, Route::DeleteInvite { code }, None)
        .await
        .map(|_| ())
}

// ---------------------------------------------------------------- webhooks

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Webhook {
    pub id: String,
    pub name: String,
    pub channel_id: Option<String>,
    /// Solo si Discord entrega el token (al crearlo o si lo creó este bot).
    pub url: Option<String>,
}

fn webhook(v: &Value) -> Webhook {
    let wid = texto(v, "id").unwrap_or_default();
    Webhook {
        url: texto(v, "token").map(|t| format!("https://discord.com/api/webhooks/{wid}/{t}")),
        id: wid,
        name: texto(v, "name").unwrap_or_default(),
        channel_id: texto(v, "channel_id"),
    }
}

pub async fn fetch_webhooks(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<Webhook>, String> {
    let guild_id = id(guild_id, "servidor")?;
    let json = pedir(token, base_url, Route::GetGuildWebhooks { guild_id }, None).await?;
    Ok(json
        .as_array()
        .map(|a| a.iter().map(webhook).collect())
        .unwrap_or_default())
}

pub async fn create_webhook(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    name: &str,
) -> Result<Webhook, String> {
    let channel_id = id(channel_id, "canal")?;
    let nombre = name.trim();
    if nombre.is_empty() || nombre.chars().count() > 80 {
        return Err("El nombre del webhook debe tener entre 1 y 80 caracteres".to_string());
    }
    let json = pedir(
        token,
        base_url,
        Route::CreateWebhook { channel_id },
        Some(&serde_json::json!({ "name": nombre })),
    )
    .await?;
    Ok(webhook(&json))
}

pub async fn delete_webhook(
    token: &str,
    base_url: Option<&str>,
    webhook_id: &str,
) -> Result<(), String> {
    let webhook_id = id(webhook_id, "webhook")?;
    pedir(
        token,
        base_url,
        Route::DeleteWebhook {
            token: None,
            webhook_id,
        },
        None,
    )
    .await
    .map(|_| ())
}
