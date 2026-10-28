//! `argus-mcp`: servidor MCP (stdio) para gestionar Discord con los bots de ARGUS.
//!
//! stdout es solo para el protocolo; los avisos van a stderr y nunca incluyen
//! el token del bot.

use argus_lib::{db, mcp::ArgusMcp};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .init();

    if let Err(e) = servir().await {
        eprintln!("argus-mcp: {e}");
        std::process::exit(1);
    }
}

async fn servir() -> Result<(), String> {
    let ruta = db::ruta_por_defecto().map_err(|e| e.to_string())?;
    let pool = db::abrir_bd_por_ruta(&ruta)
        .await
        .map_err(|e| format!("No se pudo abrir la base de datos de ARGUS: {e}"))?;
    let servidor = ArgusMcp::nuevo(pool, None).await?;
    let sesion = servidor
        .serve(stdio())
        .await
        .map_err(|e| format!("No se pudo iniciar el servidor MCP: {e}"))?;
    sesion.waiting().await.map_err(|e| e.to_string())?;
    Ok(())
}
