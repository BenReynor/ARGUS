//! Almacén genérico de credenciales de conectores (clave de API o token del propio
//! servicio). Cifradas con crypto.rs (llavero); ligadas a servicio, cuenta y servidor.
//! Nunca se devuelven al frontend, a diagnósticos, logs ni prompts. OAuth: más adelante.
use crate::crypto;
use anyhow::{anyhow, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// Entrada del catálogo relevante para credenciales.
pub struct ConnectorSpec {
    pub id: &'static str,
    /// false: el servicio no admite credenciales propias (p. ej. el token privado de un bot ajeno).
    pub accepts_credential: bool,
    pub read_tools: &'static [&'static str],
}

/// Espejo del catálogo versionado del frontend (`src/externalServices.ts`).
/// Discord no admite credencial: nunca se pide el token de un bot ajeno.
pub const CATALOGUE: &[ConnectorSpec] = &[
    ConnectorSpec {
        id: "github",
        accepts_credential: true,
        read_tools: &["list_installation_repositories"],
    },
    ConnectorSpec {
        id: "discord",
        accepts_credential: false,
        read_tools: &[],
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialKind {
    ApiKey,
    ServiceToken,
}
impl CredentialKind {
    pub fn parse(kind: &str) -> Result<Self> {
        match kind {
            "api_key" => Ok(Self::ApiKey),
            "service_token" => Ok(Self::ServiceToken),
            "oauth" => Err(anyhow!("OAuth is not supported yet")),
            _ => Err(anyhow!("Unsupported credential type")),
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::ServiceToken => "service_token",
        }
    }
}

/// Respuesta del propio servicio sobre una credencial.
#[derive(Debug, PartialEq, Eq)]
pub enum Authorization {
    Valid,
    Revoked,
}
pub trait Authorizer: Sync {
    fn check(&self, service: &str, secret: &str) -> Result<Authorization>;
}

#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct CredentialInfo {
    pub service: String,
    pub account: String,
    pub guild_id: String,
    pub kind: String,
    pub expires_at: Option<i64>,
    pub expired: bool,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn spec<'a>(catalogue: &'a [ConnectorSpec], service: &str) -> Result<&'a ConnectorSpec> {
    catalogue
        .iter()
        .find(|s| s.id == service)
        .ok_or_else(|| anyhow!("Unknown connector"))
}

/// Guarda la credencial cifrada tras validarla con el servicio. Los errores nunca incluyen el secreto.
#[allow(clippy::too_many_arguments)]
pub async fn connect(
    pool: &SqlitePool,
    catalogue: &[ConnectorSpec],
    authorizer: &dyn Authorizer,
    service: &str,
    account: &str,
    guild_id: &str,
    kind: &str,
    secret: &str,
    expires_at: Option<i64>,
    now: i64,
) -> Result<()> {
    if !spec(catalogue, service)?.accepts_credential {
        return Err(anyhow!("This service does not accept credentials in ARGUS"));
    }
    let kind = CredentialKind::parse(kind)?;
    if account.trim().is_empty() || guild_id.trim().is_empty() || secret.trim().is_empty() {
        return Err(anyhow!("Account, server and credential are required"));
    }
    if expires_at.is_some_and(|e| e <= now) {
        return Err(anyhow!("The credential has already expired"));
    }
    if authorizer.check(service, secret)? == Authorization::Revoked {
        return Err(anyhow!("The service rejected the credential"));
    }
    let cipher = crypto::encrypt(secret)?;
    sqlx::query(
        "INSERT INTO connector_credentials (service, account, guild_id, kind, credential_ciphertext, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(service, account, guild_id) DO UPDATE SET kind = excluded.kind,
           credential_ciphertext = excluded.credential_ciphertext, expires_at = excluded.expires_at,
           created_at = excluded.created_at",
    )
    .bind(service)
    .bind(account)
    .bind(guild_id)
    .bind(kind.as_str())
    .bind(cipher)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Borra la credencial; el conector deja de ofrecer sus herramientas.
pub async fn disconnect(
    pool: &SqlitePool,
    service: &str,
    account: &str,
    guild_id: &str,
) -> Result<bool> {
    let r = sqlx::query(
        "DELETE FROM connector_credentials WHERE service = ? AND account = ? AND guild_id = ?",
    )
    .bind(service)
    .bind(account)
    .bind(guild_id)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Estado sin secretos, para la interfaz y los diagnósticos.
pub async fn list(pool: &SqlitePool, guild_id: &str, now: i64) -> Result<Vec<CredentialInfo>> {
    let rows = sqlx::query(
        "SELECT service, account, guild_id, kind, expires_at FROM connector_credentials
         WHERE guild_id = ? ORDER BY service, account",
    )
    .bind(guild_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let expires_at: Option<i64> = r.get("expires_at");
            CredentialInfo {
                service: r.get("service"),
                account: r.get("account"),
                guild_id: r.get("guild_id"),
                kind: r.get("kind"),
                expires_at,
                expired: expires_at.is_some_and(|e| e <= now),
            }
        })
        .collect())
}

/// Solo para el adaptador del propio conector (uso interno): descifra si no ha expirado.
pub async fn credential(
    pool: &SqlitePool,
    service: &str,
    account: &str,
    guild_id: &str,
    now: i64,
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT credential_ciphertext, expires_at FROM connector_credentials
         WHERE service = ? AND account = ? AND guild_id = ?",
    )
    .bind(service)
    .bind(account)
    .bind(guild_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let expires_at: Option<i64> = row.get("expires_at");
    if expires_at.is_some_and(|e| e <= now) {
        return Err(anyhow!("The credential has expired; reconnect the service"));
    }
    Ok(Some(crypto::decrypt(
        &row.get::<String, _>("credential_ciphertext"),
    )?))
}

/// Pregunta al servicio si sigue autorizada; si fue revocada la borra. Devuelve si sigue válida.
pub async fn verify(
    pool: &SqlitePool,
    authorizer: &dyn Authorizer,
    service: &str,
    account: &str,
    guild_id: &str,
    now: i64,
) -> Result<bool> {
    let Some(secret) = credential(pool, service, account, guild_id, now).await? else {
        return Ok(false);
    };
    match authorizer.check(service, &secret)? {
        Authorization::Valid => Ok(true),
        Authorization::Revoked => {
            disconnect(pool, service, account, guild_id).await?;
            Ok(false)
        }
    }
}

/// Herramientas de lectura ofrecidas: solo de conectores habilitados con credencial vigente.
pub async fn offered_tools(
    pool: &SqlitePool,
    catalogue: &[ConnectorSpec],
    guild_id: &str,
    enabled: &[String],
    now: i64,
) -> Result<Vec<String>> {
    let creds = list(pool, guild_id, now).await?;
    let mut tools = Vec::new();
    for spec in catalogue {
        if enabled.iter().any(|e| e == spec.id)
            && creds.iter().any(|c| c.service == spec.id && !c.expired)
        {
            tools.extend(
                spec.read_tools
                    .iter()
                    .map(|t| format!("{}__{}", spec.id, t)),
            );
        }
    }
    Ok(tools)
}

/// Autorizador sin red: ningún servicio real está implementado todavía, así que solo
/// comprueba el formato (no vacío). Los adaptadores reales lo sustituirán.
pub struct FormatOnlyAuthorizer;
impl Authorizer for FormatOnlyAuthorizer {
    fn check(&self, _service: &str, secret: &str) -> Result<Authorization> {
        Ok(if secret.trim().is_empty() {
            Authorization::Revoked
        } else {
            Authorization::Valid
        })
    }
}

/// Estado de un conector del catálogo para la pantalla (sin secretos).
#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct ConnectorState {
    pub service: String,
    pub accepts_credential: bool,
    /// "none" | "connected" | "expired"
    pub status: String,
    pub account: Option<String>,
    pub expires_at: Option<i64>,
}

pub async fn states(
    pool: &SqlitePool,
    catalogue: &[ConnectorSpec],
    guild_id: &str,
    now: i64,
) -> Result<Vec<ConnectorState>> {
    let creds = list(pool, guild_id, now).await?;
    Ok(catalogue
        .iter()
        .map(|s| {
            let c = creds.iter().find(|c| c.service == s.id);
            ConnectorState {
                service: s.id.to_string(),
                accepts_credential: s.accepts_credential,
                status: match c {
                    None => "none",
                    Some(c) if c.expired => "expired",
                    Some(_) => "connected",
                }
                .to_string(),
                account: c.map(|c| c.account.clone()),
                expires_at: c.and_then(|c| c.expires_at),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicBool, Ordering};

    const FAKE: &[ConnectorSpec] = &[
        ConnectorSpec {
            id: "fake",
            accepts_credential: true,
            read_tools: &["read_a", "read_b"],
        },
        ConnectorSpec {
            id: "other_bot",
            accepts_credential: false,
            read_tools: &["x"],
        },
    ];
    const SECRET: &str = "sk-fake-SECRET-123";

    struct FakeAuth(AtomicBool);
    impl Authorizer for FakeAuth {
        fn check(&self, _: &str, _: &str) -> Result<Authorization> {
            Ok(if self.0.load(Ordering::SeqCst) {
                Authorization::Valid
            } else {
                Authorization::Revoked
            })
        }
    }

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::run_migrations(&pool).await.unwrap();
        pool
    }

    async fn conectar(p: &SqlitePool, a: &FakeAuth, exp: Option<i64>) -> Result<()> {
        connect(
            p, FAKE, a, "fake", "acct", "g1", "api_key", SECRET, exp, 100,
        )
        .await
    }

    #[tokio::test]
    async fn stores_encrypted_and_scoped_without_leaking() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(true));
        conectar(&p, &a, None).await.unwrap();
        let raw: String =
            sqlx::query_scalar("SELECT credential_ciphertext FROM connector_credentials")
                .fetch_one(&p)
                .await
                .unwrap();
        assert!(!raw.contains(SECRET));
        assert_eq!(
            credential(&p, "fake", "acct", "g1", 100)
                .await
                .unwrap()
                .as_deref(),
            Some(SECRET)
        );
        assert_eq!(
            credential(&p, "fake", "acct", "g2", 100).await.unwrap(),
            None
        );
        assert_eq!(
            credential(&p, "fake", "otra", "g1", 100).await.unwrap(),
            None
        );
        let listed = serde_json::to_string(&list(&p, "g1", 100).await.unwrap()).unwrap();
        assert!(listed.contains("acct") && !listed.contains(SECRET));
    }

    #[tokio::test]
    async fn rejects_foreign_bot_tokens_oauth_and_bad_input_without_echoing_secret() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(true));
        let e = connect(
            &p,
            FAKE,
            &a,
            "other_bot",
            "a",
            "g1",
            "service_token",
            SECRET,
            None,
            1,
        )
        .await
        .unwrap_err();
        assert!(!e.to_string().contains(SECRET));
        let e = connect(&p, FAKE, &a, "fake", "a", "g1", "oauth", SECRET, None, 1)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("OAuth") && !e.to_string().contains(SECRET));
        assert!(connect(
            &p,
            FAKE,
            &a,
            "fake",
            "a",
            "g1",
            "user_token",
            SECRET,
            None,
            1
        )
        .await
        .is_err());
        assert!(
            connect(&p, FAKE, &a, "nope", "a", "g1", "api_key", SECRET, None, 1)
                .await
                .is_err()
        );
        assert!(
            connect(&p, FAKE, &a, "fake", "a", "g1", "api_key", " ", None, 1)
                .await
                .is_err()
        );
        // Catálogo real: Discord (bot ajeno) no acepta credencial.
        assert!(connect(
            &p,
            CATALOGUE,
            &a,
            "discord",
            "a",
            "g1",
            "service_token",
            SECRET,
            None,
            1
        )
        .await
        .is_err());
        assert!(list(&p, "g1", 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fake_authorization_rejection_stores_nothing() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(false));
        let e = conectar(&p, &a, None).await.unwrap_err();
        assert!(!e.to_string().contains(SECRET));
        assert!(list(&p, "g1", 100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn expiry_stops_use_and_tools() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(true));
        assert!(conectar(&p, &a, Some(100)).await.is_err());
        conectar(&p, &a, Some(500)).await.unwrap();
        let en = vec!["fake".to_string()];
        assert_eq!(
            offered_tools(&p, FAKE, "g1", &en, 200).await.unwrap().len(),
            2
        );
        assert!(list(&p, "g1", 600).await.unwrap()[0].expired);
        assert!(credential(&p, "fake", "acct", "g1", 600).await.is_err());
        assert!(offered_tools(&p, FAKE, "g1", &en, 600)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn revocation_deletes_credential_and_tools() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(true));
        conectar(&p, &a, None).await.unwrap();
        assert!(verify(&p, &a, "fake", "acct", "g1", 100).await.unwrap());
        a.0.store(false, Ordering::SeqCst);
        assert!(!verify(&p, &a, "fake", "acct", "g1", 100).await.unwrap());
        assert!(list(&p, "g1", 100).await.unwrap().is_empty());
        assert!(offered_tools(&p, FAKE, "g1", &["fake".into()], 100)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn screen_states_connect_disconnect_and_never_return_credential() {
        let p = pool().await;
        let a = FormatOnlyAuthorizer;
        let st = states(&p, FAKE, "g1", 100).await.unwrap();
        assert!(st.iter().all(|s| s.status == "none"));
        assert!(
            !st.iter()
                .find(|s| s.service == "other_bot")
                .unwrap()
                .accepts_credential
        );
        connect(
            &p,
            FAKE,
            &a,
            "fake",
            "acct",
            "g1",
            "api_key",
            SECRET,
            Some(500),
            100,
        )
        .await
        .unwrap();
        let st = states(&p, FAKE, "g1", 100).await.unwrap();
        assert_eq!(
            st.iter().find(|s| s.service == "fake").unwrap().status,
            "connected"
        );
        assert!(!serde_json::to_string(&st).unwrap().contains(SECRET));
        assert_eq!(
            states(&p, FAKE, "g1", 600).await.unwrap()[0].status,
            "expired"
        );
        assert!(states(&p, FAKE, "g2", 100)
            .await
            .unwrap()
            .iter()
            .all(|s| s.status == "none"));
        assert!(disconnect(&p, "fake", "acct", "g1").await.unwrap());
        assert!(states(&p, FAKE, "g1", 100)
            .await
            .unwrap()
            .iter()
            .all(|s| s.status == "none"));
    }

    #[tokio::test]
    async fn disconnect_deletes_and_withdraws_tools_and_requires_enabling() {
        let p = pool().await;
        let a = FakeAuth(AtomicBool::new(true));
        conectar(&p, &a, None).await.unwrap();
        assert!(offered_tools(&p, FAKE, "g1", &[], 100)
            .await
            .unwrap()
            .is_empty());
        let en = vec!["fake".to_string()];
        assert_eq!(
            offered_tools(&p, FAKE, "g1", &en, 100).await.unwrap(),
            vec!["fake__read_a", "fake__read_b"]
        );
        assert!(disconnect(&p, "fake", "acct", "g1").await.unwrap());
        assert!(!disconnect(&p, "fake", "acct", "g1").await.unwrap());
        assert!(offered_tools(&p, FAKE, "g1", &en, 100)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            credential(&p, "fake", "acct", "g1", 100).await.unwrap(),
            None
        );
    }
}
