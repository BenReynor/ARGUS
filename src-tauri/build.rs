use std::{env, fs, path::Path};

fn main() {
    // `bundle.externalBin` exige que `binaries/argus-mcp-<triple>` exista al compilar.
    // En desarrollo y en los tests basta un marcador vacío; `npm run build:mcp`
    // lo sustituye por el binario real antes de empaquetar la app.
    if let Ok(triple) = env::var("TARGET") {
        let ext = if triple.contains("windows") {
            ".exe"
        } else {
            ""
        };
        let ruta = Path::new("binaries").join(format!("argus-mcp-{triple}{ext}"));
        if !ruta.exists() {
            let _ = fs::create_dir_all("binaries");
            let _ = fs::write(&ruta, b"");
        }
    }
    tauri_build::build()
}
