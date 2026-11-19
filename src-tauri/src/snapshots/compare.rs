//! Comparación de estructura por identidad, independiente del orden REST y de nombres auxiliares.
use super::*;
use crate::asistente::{diferencias_lineas, LineaDiff};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
pub struct Counts {
    missing: usize,
    changed: usize,
    extra: usize,
}
#[derive(Serialize)]
pub struct Entry {
    id: String,
    label: String,
    status: &'static str,
    lineas: Vec<LineaDiff>,
}
#[derive(Serialize)]
pub struct Group {
    kind: &'static str,
    counts: Counts,
    entries: Vec<Entry>,
}
#[derive(Serialize)]
pub struct Comparison {
    counts: Counts,
    groups: Vec<Group>,
}

type Items = BTreeMap<String, (String, Value)>;

fn label(v: &Value) -> String {
    format!(
        "{} ({})",
        v["name"].as_str().unwrap_or_default(),
        v["id"].as_str().unwrap_or_default()
    )
}
fn structure(items: &[Value], excluded: &[&str]) -> Items {
    items
        .iter()
        .map(|v| {
            let mut data = v.clone();
            if let Some(obj) = data.as_object_mut() {
                for key in excluded {
                    obj.remove(*key);
                }
            }
            (
                v["id"].as_str().unwrap_or_default().to_owned(),
                (label(v), data),
            )
        })
        .collect()
}
fn permissions(roles: &[Value], channels: &[Value]) -> Items {
    let mut out = Items::new();
    for role in roles {
        out.insert(
            format!("role:{}", role["id"].as_str().unwrap_or_default()),
            (label(role), json!({"permissions":role["permissions"]})),
        );
    }
    for channel in channels {
        if let Some(overwrites) = channel["permission_overwrites"].as_array() {
            for o in overwrites {
                let target = o["name"]
                    .as_str()
                    .or_else(|| name(roles, o["id"].as_str().unwrap_or_default()))
                    .unwrap_or_default();
                out.insert(
                    format!(
                        "channel:{}:{}:{}",
                        channel["id"].as_str().unwrap_or_default(),
                        o["type"],
                        o["id"].as_str().unwrap_or_default()
                    ),
                    (
                        format!(
                            "{} → {} ({})",
                            label(channel),
                            target,
                            o["id"].as_str().unwrap_or_default()
                        ),
                        json!({"allow":o["allow"],"deny":o["deny"]}),
                    ),
                );
            }
        }
    }
    out
}
fn assignments(members: &[Value]) -> Items {
    members
        .iter()
        .map(|m| {
            let roles: BTreeSet<&str> = m["roles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r.as_str().or_else(|| r["id"].as_str()))
                .collect();
            (
                m["id"].as_str().unwrap_or_default().to_owned(),
                (label(m), json!({"roles":roles})),
            )
        })
        .collect()
}
fn group(kind: &'static str, saved: Items, current: Items) -> Group {
    let ids: BTreeSet<&String> = saved.keys().chain(current.keys()).collect();
    let mut result = Group {
        kind,
        counts: Counts::default(),
        entries: Vec::new(),
    };
    for id in ids {
        let before = saved.get(id);
        let after = current.get(id);
        if before.map(|v| &v.1) == after.map(|v| &v.1) {
            continue;
        }
        let status = match (before, after) {
            (Some(_), None) => {
                result.counts.missing += 1;
                "missing"
            }
            (None, Some(_)) => {
                result.counts.extra += 1;
                "extra"
            }
            _ => {
                result.counts.changed += 1;
                "changed"
            }
        };
        let text = |v: Option<&(String, Value)>| {
            v.map(|v| serde_json::to_string_pretty(&v.1).unwrap())
                .unwrap_or_default()
        };
        result.entries.push(Entry {
            id: id.clone(),
            label: after.or(before).unwrap().0.clone(),
            status,
            lineas: diferencias_lineas(&text(before), &text(after)),
        });
    }
    result
}
fn compare(
    saved: &ServerSnapshot,
    roles: &[Value],
    channels: &[Value],
    members: &[Value],
) -> Comparison {
    let groups = vec![
        group(
            "channels",
            structure(
                &saved.channels,
                &[
                    "permission_overwrites",
                    "last_message_id",
                    "last_pin_timestamp",
                    "message_count",
                    "member_count",
                    "total_message_sent",
                ],
            ),
            structure(
                channels,
                &[
                    "permission_overwrites",
                    "last_message_id",
                    "last_pin_timestamp",
                    "message_count",
                    "member_count",
                    "total_message_sent",
                ],
            ),
        ),
        group(
            "roles",
            structure(&saved.roles, &["permissions", "icon_image"]),
            structure(roles, &["permissions", "icon_image"]),
        ),
        group(
            "permissions",
            permissions(&saved.roles, &saved.channels),
            permissions(roles, channels),
        ),
        group(
            "member_roles",
            assignments(&saved.members),
            assignments(members),
        ),
    ];
    let mut counts = Counts::default();
    for g in &groups {
        counts.missing += g.counts.missing;
        counts.changed += g.counts.changed;
        counts.extra += g.counts.extra;
    }
    Comparison { counts, groups }
}

/// Solo lee las colecciones necesarias: no requiere permisos de bans, webhooks o CDN.
pub(crate) async fn compare_saved(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    id: &str,
    token: &str,
    base: Option<&str>,
) -> Result<Comparison> {
    let saved = load(pool, bot, guild, id).await?;
    let guild_id = guild.parse::<u64>()?;
    let roles = array(get(token, base, Route::GetGuildRoles { guild_id }).await?)?;
    let channels = array(get(token, base, Route::GetChannels { guild_id }).await?)?;
    let mut members = Vec::new();
    let mut after = None;
    loop {
        let page = array(
            get(
                token,
                base,
                Route::GetGuildMembers {
                    guild_id,
                    after,
                    limit: Some(1000),
                },
            )
            .await?,
        )?;
        let full = page.len() == 1000;
        let next = page
            .last()
            .and_then(|m| m["user"]["id"].as_str())
            .map(str::parse::<u64>)
            .transpose()?;
        for m in page {
            members.push(json!({"id":m["user"]["id"],"name":m["nick"].as_str().or(m["user"]["username"].as_str()).unwrap_or_default(),"roles":m["roles"]}));
        }
        if !full {
            break;
        }
        if next.is_none() || next <= after {
            bail!("Invalid response from Discord");
        }
        after = next;
    }
    Ok(compare(&saved, &roles, &channels, &members))
}
