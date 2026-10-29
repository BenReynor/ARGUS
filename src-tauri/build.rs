use std::{env, path::Path};

fn main() {
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
