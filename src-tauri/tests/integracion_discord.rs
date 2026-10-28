//! Tests de integración contra un servidor real de Discord.
//!
//! Están marcados `#[ignore]`: `cargo test` normal no los ejecuta. Se lanzan con
//! `cargo test -- --ignored` y leen del entorno ARGUS_TEST_TOKEN,
//! ARGUS_TEST_GUILD, ARGUS_TEST_CHANNEL, ARGUS_TEST_ROLE y ARGUS_TEST_USER. Si
//! falta alguna, cada test termina sin hacer nada. Ver docs/PRUEBA_REAL.md.
//!
//! Usan las mismas funciones de `discord.rs` que los comandos de la app. El crate
//! es binario, así que se incluyen los módulos necesarios por ruta.

#![allow(dead_code, unused_imports)]

#[path = "../src/crypto.rs"]
mod crypto;
#[path = "../src/db.rs"]
mod db;
#[path = "../src/discord.rs"]
mod discord;
#[path = "../src/state.rs"]
mod state;

struct Entorno {
    token: String,
    guild: String,
    channel: String,
    role: String,
    user: String,
}

fn leer(nombre: &str) -> Option<String> {
    std::env::var(nombre).ok().filter(|v| !v.trim().is_empty())
}

/// Lee las cinco variables; `None` (y aviso) si falta alguna o está vacía.
fn entorno() -> Option<Entorno> {
    let datos = (|| {
        Some(Entorno {
            token: leer("ARGUS_TEST_TOKEN")?,
            guild: leer("ARGUS_TEST_GUILD")?,
            channel: leer("ARGUS_TEST_CHANNEL")?,
            role: leer("ARGUS_TEST_ROLE")?,
            user: leer("ARGUS_TEST_USER")?,
        })
    })();
    if datos.is_none() {
        eprintln!("Faltan variables ARGUS_TEST_*: test omitido");
    }
    datos
}

#[tokio::test]
#[ignore]
async fn valida_el_token() {
    let Some(e) = entorno() else { return };
    let usuario = discord::fetch_current_user(&e.token, None)
        .await
        .expect("el token debería ser válido");
    assert!(!usuario.name.is_empty());
    assert!(usuario.bot, "el token debe ser de un bot");
}

#[tokio::test]
#[ignore]
async fn el_servidor_esta_entre_los_del_bot() {
    let Some(e) = entorno() else { return };
    let guilds = discord::fetch_current_user_guilds(&e.token, None)
        .await
        .expect("listar servidores del bot");
    assert!(
        guilds.iter().any(|g| g.id.to_string() == e.guild),
        "ARGUS_TEST_GUILD no está entre los servidores del bot"
    );
}

#[tokio::test]
#[ignore]
async fn lee_canales_roles_y_miembros() {
    let Some(e) = entorno() else { return };
    let canales = discord::fetch_guild_channels(&e.token, None, &e.guild)
        .await
        .expect("leer canales");
    assert!(canales.iter().any(|c| c.id == e.channel));
    let roles = discord::fetch_guild_roles(&e.token, None, &e.guild)
        .await
        .expect("leer roles");
    assert!(roles.iter().any(|r| r.id == e.role));
    let miembros = discord::fetch_guild_members(&e.token, None, &e.guild)
        .await
        .expect("leer miembros");
    assert!(miembros.iter().any(|m| m.user_id == e.user));
}

#[tokio::test]
#[ignore]
async fn calcula_los_permisos_del_bot() {
    let Some(e) = entorno() else { return };
    let bot = discord::fetch_current_user(&e.token, None)
        .await
        .expect("token válido");
    let permisos = discord::fetch_bot_permissions(&e.token, None, &e.guild, &bot.id.to_string())
        .await
        .expect("calcular permisos del bot");
    assert!(!permisos.is_empty(), "el bot no tiene ningún permiso");
}

#[tokio::test]
#[ignore]
async fn envia_un_mensaje_al_canal() {
    let Some(e) = entorno() else { return };
    let id = discord::send_channel_message(
        &e.token,
        None,
        &e.channel,
        "Prueba de integración de ARGUS (puede borrarse)",
    )
    .await
    .expect("enviar mensaje al canal");
    assert!(!id.is_empty());
}

#[tokio::test]
#[ignore]
async fn envia_un_dm_al_usuario() {
    let Some(e) = entorno() else { return };
    let id = discord::send_direct_message(
        &e.token,
        None,
        &e.user,
        "Prueba de integración de ARGUS (puede borrarse)",
    )
    .await
    .expect("enviar DM (el usuario debe aceptar mensajes directos)");
    assert!(!id.is_empty());
}

#[tokio::test]
#[ignore]
async fn da_y_quita_un_rol() {
    let Some(e) = entorno() else { return };
    let dar = discord::set_member_role(&e.token, None, &e.guild, &e.user, &e.role, true).await;
    // Se retira siempre, aunque dar haya fallado, para no dejar rastro.
    let quitar = discord::set_member_role(&e.token, None, &e.guild, &e.user, &e.role, false).await;
    dar.expect("dar el rol");
    quitar.expect("quitar el rol");
}

#[tokio::test]
#[ignore]
async fn aplica_y_retira_un_timeout() {
    let Some(e) = entorno() else { return };
    let aplicar = discord::set_member_timeout(&e.token, None, &e.guild, &e.user, 1).await;
    let retirar = discord::set_member_timeout(&e.token, None, &e.guild, &e.user, 0).await;
    aplicar.expect("aplicar timeout de 1 minuto (el usuario no puede ser admin ni dueño)");
    retirar.expect("retirar el timeout");
}
