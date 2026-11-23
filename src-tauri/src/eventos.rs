//! Eventos programados del servidor (REST de Discord, vía twilight).

use crate::discord::{parse_id, pedir_json};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use twilight_http::routing::Route;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, ScheduledEventMarker};

/// Datos de un evento. Fechas en ISO 8601 con zona (`2030-05-01T20:00:00+02:00`).
#[derive(Debug, Clone, Default)]
pub struct DatosEvento {
    pub nombre: Option<String>,
    pub descripcion: Option<String>,
    pub inicio: Option<String>,
    pub fin: Option<String>,
    /// Lugar externo (evento externo; exige fin).
    pub lugar: Option<String>,
    /// Canal de voz o escenario donde se celebra.
    pub canal_id: Option<String>,
}

fn fecha(texto: &str, que: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(texto.trim())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| {
            format!("Invalid {que} date: use ISO 8601 with a time zone (2030-05-01T20:00:00+02:00)")
        })
}

fn iso(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn validar_fechas(inicio: Option<DateTime<Utc>>, fin: Option<DateTime<Utc>>) -> Result<(), String> {
    if let Some(i) = inicio {
        if i <= Utc::now() {
            return Err("The event start must be in the future".to_string());
        }
    }
    if let (Some(i), Some(f)) = (inicio, fin) {
        if f <= i {
            return Err("The event end must be after its start".to_string());
        }
    }
    Ok(())
}

fn validar_textos(d: &DatosEvento) -> Result<(), String> {
    if let Some(n) = &d.nombre {
        if n.trim().is_empty() {
            return Err("The event name cannot be empty".to_string());
        }
        if n.chars().count() > 100 {
            return Err("The event name is too long (the maximum is 100 characters)".to_string());
        }
    }
    if let Some(desc) = &d.descripcion {
        if desc.chars().count() > 1000 {
            return Err(
                "The event description is too long (the maximum is 1000 characters)".to_string(),
            );
        }
    }
    if let Some(l) = &d.lugar {
        if l.trim().is_empty() || l.chars().count() > 100 {
            return Err("The event location must be between 1 and 100 characters".to_string());
        }
    }
    Ok(())
}

/// Tipo de entidad según el canal: escenario (13) = 1, voz (2) = 2.
async fn tipo_de_canal(token: &str, base_url: Option<&str>, canal: &str) -> Result<u8, String> {
    let cid = parse_id::<ChannelMarker>(canal.trim(), "channel")?;
    let c = pedir_json(
        token,
        base_url,
        Route::GetChannel {
            channel_id: cid.get(),
        },
        None,
    )
    .await?;
    match c.get("type").and_then(|t| t.as_u64()) {
        Some(13) => Ok(1),
        Some(2) => Ok(2),
        _ => Err("The event channel must be a voice or stage channel".to_string()),
    }
}

fn resumen(e: &Value) -> Value {
    let s = |k: &str| e.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let tipo = match e.get("entity_type").and_then(|t| t.as_u64()) {
        Some(1) => "stage",
        Some(2) => "voice",
        Some(3) => "external",
        _ => "unknown",
    };
    let estado = match e.get("status").and_then(|t| t.as_u64()) {
        Some(1) => "scheduled",
        Some(2) => "active",
        Some(3) => "completed",
        Some(4) => "canceled",
        _ => "unknown",
    };
    json!({
        "id": s("id"),
        "nombre": s("name"),
        "descripcion": s("description"),
        "inicio": s("scheduled_start_time"),
        "fin": s("scheduled_end_time"),
        "tipo": tipo,
        "estado": estado,
        "canal_id": s("channel_id"),
        "lugar": e.get("entity_metadata").and_then(|m| m.get("location")).and_then(|l| l.as_str()),
    })
}

/// `GET /guilds/{id}/scheduled-events`.
pub async fn listar(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<Value>, String> {
    let gid = parse_id::<GuildMarker>(guild_id, "server")?;
    let r = pedir_json(
        token,
        base_url,
        Route::GetGuildScheduledEvents {
            guild_id: gid.get(),
            with_user_count: false,
        },
        None,
    )
    .await?;
    Ok(r.as_array()
        .map(|a| a.iter().map(resumen).collect())
        .unwrap_or_default())
}

/// `POST /guilds/{id}/scheduled-events` (`privacy_level` 2: solo miembros). Devuelve el id.
pub async fn crear(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    d: &DatosEvento,
) -> Result<String, String> {
    let gid = parse_id::<GuildMarker>(guild_id, "server")?;
    validar_textos(d)?;
    if d.nombre.is_none() {
        return Err("The event name cannot be empty".to_string());
    }
    let inicio = fecha(d.inicio.as_deref().unwrap_or(""), "start")?;
    let fin = d.fin.as_deref().map(|f| fecha(f, "end")).transpose()?;
    validar_fechas(Some(inicio), fin)?;
    let mut cuerpo = json!({
        "name": d.nombre.as_deref().unwrap_or_default().trim(),
        "privacy_level": 2,
        "scheduled_start_time": iso(inicio),
    });
    if let Some(f) = fin {
        cuerpo["scheduled_end_time"] = json!(iso(f));
    }
    if let Some(desc) = &d.descripcion {
        cuerpo["description"] = json!(desc);
    }
    match (&d.lugar, &d.canal_id) {
        (Some(_), Some(_)) | (None, None) => {
            return Err("Provide either an external location or a voice/stage channel".to_string())
        }
        (Some(lugar), None) => {
            if fin.is_none() {
                return Err("An event with an external location needs an end time".to_string());
            }
            cuerpo["entity_type"] = json!(3);
            cuerpo["entity_metadata"] = json!({"location": lugar.trim()});
        }
        (None, Some(canal)) => {
            cuerpo["entity_type"] = json!(tipo_de_canal(token, base_url, canal).await?);
            cuerpo["channel_id"] = json!(canal.trim());
        }
    }
    let r = pedir_json(
        token,
        base_url,
        Route::CreateGuildScheduledEvent {
            guild_id: gid.get(),
        },
        Some(cuerpo),
    )
    .await?;
    r.get("id")
        .and_then(|i| i.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Invalid response from Discord: missing event id".to_string())
}

/// `PATCH /guilds/{id}/scheduled-events/{id}`: solo envía lo indicado.
pub async fn editar(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    event_id: &str,
    d: &DatosEvento,
) -> Result<(), String> {
    let gid = parse_id::<GuildMarker>(guild_id, "server")?;
    let eid = parse_id::<ScheduledEventMarker>(event_id.trim(), "event")?;
    validar_textos(d)?;
    if d.lugar.is_some() && d.canal_id.is_some() {
        return Err("Provide either an external location or a voice/stage channel".to_string());
    }
    let inicio = d.inicio.as_deref().map(|i| fecha(i, "start")).transpose()?;
    let fin = d.fin.as_deref().map(|f| fecha(f, "end")).transpose()?;
    validar_fechas(inicio, fin)?;
    // Con solo una de las dos fechas, se compara con la otra del evento actual.
    if inicio.is_some() != fin.is_some() {
        let actual = pedir_json(
            token,
            base_url,
            Route::GetGuildScheduledEvent {
                guild_id: gid.get(),
                scheduled_event_id: eid.get(),
                with_user_count: false,
            },
            None,
        )
        .await?;
        let leer = |k: &str| {
            actual
                .get(k)
                .and_then(|v| v.as_str())
                .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
                .map(|v| v.with_timezone(&Utc))
        };
        let i = inicio.or_else(|| leer("scheduled_start_time"));
        let f = fin.or_else(|| leer("scheduled_end_time"));
        if let (Some(i), Some(f)) = (i, f) {
            if f <= i {
                return Err("The event end must be after its start".to_string());
            }
        }
    }
    let mut cuerpo = json!({});
    if let Some(n) = &d.nombre {
        cuerpo["name"] = json!(n.trim());
    }
    if let Some(desc) = &d.descripcion {
        cuerpo["description"] = json!(desc);
    }
    if let Some(i) = inicio {
        cuerpo["scheduled_start_time"] = json!(iso(i));
    }
    if let Some(f) = fin {
        cuerpo["scheduled_end_time"] = json!(iso(f));
    }
    if let Some(lugar) = &d.lugar {
        cuerpo["entity_type"] = json!(3);
        cuerpo["channel_id"] = Value::Null;
        cuerpo["entity_metadata"] = json!({"location": lugar.trim()});
    }
    if let Some(canal) = &d.canal_id {
        cuerpo["entity_type"] = json!(tipo_de_canal(token, base_url, canal).await?);
        cuerpo["channel_id"] = json!(canal.trim());
    }
    if cuerpo.as_object().map_or(true, |o| o.is_empty()) {
        return Err("Provide at least one field to change".to_string());
    }
    pedir_json(
        token,
        base_url,
        Route::UpdateGuildScheduledEvent {
            guild_id: gid.get(),
            scheduled_event_id: eid.get(),
        },
        Some(cuerpo),
    )
    .await
    .map(|_| ())
}

/// `DELETE /guilds/{id}/scheduled-events/{id}`: cancela y elimina el evento.
pub async fn cancelar(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    event_id: &str,
) -> Result<(), String> {
    let gid = parse_id::<GuildMarker>(guild_id, "server")?;
    let eid = parse_id::<ScheduledEventMarker>(event_id.trim(), "event")?;
    pedir_json(
        token,
        base_url,
        Route::DeleteGuildScheduledEvent {
            guild_id: gid.get(),
            scheduled_event_id: eid.get(),
        },
        None,
    )
    .await
    .map(|_| ())
}
