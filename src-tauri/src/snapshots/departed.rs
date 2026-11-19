//! Miembros ausentes e invitaciones; Discord nunca permite reinsertarlos.
use super::*;
use std::collections::HashMap;
use twilight_model::gateway::event::Event;

pub async fn missing(
    saved: &ServerSnapshot,
    token: &str,
    base: Option<&str>,
) -> Result<Vec<Value>> {
    let current = discord::fetch_guild_members(token, base, &saved.guild_id)
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(saved
        .members
        .iter()
        .filter(|m| {
            !current
                .iter()
                .any(|c| Some(c.user_id.as_str()) == m["id"].as_str())
        })
        .cloned()
        .collect())
}

pub async fn invite(
    token: &str,
    base: Option<&str>,
    channel: &str,
    members: &[Value],
) -> Result<Value> {
    let invitation = crate::mensajes::create_invite(token, base, channel, 86400, 0)
        .await
        .map_err(anyhow::Error::msg)?;
    let url = format!("https://discord.gg/{}", invitation.code);
    let mut results = Vec::new();
    for member in members {
        let id = member["id"]
            .as_str()
            .context("Invalid response from Discord")?;
        let sent = discord::send_direct_message(token, base, id, &url).await;
        results.push(json!({"id":id,"name":member["name"],"sent":sent.is_ok(),"error":sent.err()}));
    }
    Ok(json!({"url":url,"results":results}))
}

pub async fn enable(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    members: &[Value],
    roles: &HashMap<String, String>,
) -> Result<()> {
    let mut members = members.to_vec();
    for m in &mut members {
        m["roles"] = json!(m["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| roles.get(r["id"].as_str()?))
            .collect::<Vec<_>>());
    }
    sqlx::query("INSERT INTO snapshot_return_roles(bot_id,guild_id,enabled,members) VALUES(?,?,1,?) ON CONFLICT(bot_id,guild_id) DO UPDATE SET enabled=1,members=excluded.members")
        .bind(bot).bind(guild).bind(serde_json::to_string(&members)?).execute(pool).await?;
    Ok(())
}
pub async fn enabled(pool: &SqlitePool, bot: &str, guild: &str) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT enabled FROM snapshot_return_roles WHERE bot_id=? AND guild_id=?",
    )
    .bind(bot)
    .bind(guild)
    .fetch_optional(pool)
    .await?
    .unwrap_or(false))
}

/// Consume solo altas reales; no asigna roles por eventos de otro servidor/bot.
/// El resultado se persiste y el Gateway lo publica en Logs en el idioma activo.
pub async fn on_event(
    pool: &SqlitePool,
    bot: &str,
    token: &str,
    base: Option<&str>,
    event: &Event,
) -> Result<Vec<String>> {
    let Event::MemberAdd(member) = event else {
        return Ok(vec![]);
    };
    let guild = member.guild_id.to_string();
    let user = member.user.id.to_string();
    let row = sqlx::query(
        "SELECT members FROM snapshot_return_roles WHERE bot_id=? AND guild_id=? AND enabled=1",
    )
    .bind(bot)
    .bind(&guild)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(vec![]) };
    let members: Vec<Value> = serde_json::from_str(row.get("members"))?;
    let Some(saved) = members.iter().find(|m| m["id"] == user) else {
        return Ok(vec![]);
    };
    let permissions = discord::fetch_bot_permissions(token, base, &guild, bot)
        .await
        .map_err(anyhow::Error::msg)?;
    let roles = array(
        get(
            token,
            base,
            Route::GetGuildRoles {
                guild_id: guild.parse()?,
            },
        )
        .await?,
    )?;
    let me = discord::fetch_guild_member(token, base, &guild, bot)
        .await
        .map_err(anyhow::Error::msg)?;
    let highest = roles
        .iter()
        .filter(|r| {
            me.roles
                .iter()
                .any(|id| Some(id.as_str()) == r["id"].as_str())
        })
        .filter_map(|r| r["position"].as_i64())
        .max()
        .unwrap_or(0);
    let mut logs = Vec::new();
    for role in saved["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if member.roles.iter().any(|r| r.to_string() == role) {
            continue;
        }
        let assignable = permissions.iter().any(|p| p == "Gestionar roles")
            && role != guild
            && roles.iter().any(|r| {
                r["id"] == role
                    && r["managed"] != true
                    && r["position"].as_i64().is_some_and(|p| p < highest)
            });
        let result = if assignable {
            discord::set_member_role(token, base, &guild, &user, role, true).await
        } else {
            Err("Role is unavailable or above the bot".into())
        };
        let text = match result {
            Ok(()) => format!("Restored role {role} to returning member {user}"),
            Err(e) => format!("Could not restore role {role} to returning member {user}: {e}"),
        };
        db::insert_incident(
            pool,
            &db::Incident {
                id: 0,
                guild_id: Some(guild.clone()),
                type_: "MEMBER_ROLES_RESTORED".into(),
                payload: text.clone(),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await?;
        logs.push(text);
    }
    Ok(logs)
}

#[cfg(test)]
mod tests;
