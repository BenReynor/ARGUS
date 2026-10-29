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
    // Windows: el manifiesto (controles comunes v6) se incrusta en TODOS los
    // ejecutables, también en los de test. Si lo pone tauri-build, solo lo
    // reciben los binarios de la app y `cargo test` muere al arrancar con
    // STATUS_ENTRYPOINT_NOT_FOUND.
    let mut atributos = tauri_build::Attributes::new();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifiesto =
            Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifiesto.display()
        );
        atributos = atributos
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(atributos).expect("falló el build script de Tauri")
}
