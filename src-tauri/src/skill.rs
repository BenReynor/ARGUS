//! Skill de ARGUS para agentes: el texto versionado en `skills/argus/SKILL.md`
//! y su instalación en la carpeta de skills de Claude, Codex u OpenCode.
//! Nunca sobrescribe una skill existente sin que el usuario lo confirme.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::state::AppState;

/// Contenido de la skill, incrustado en el binario.
pub const SKILL_MD: &str = include_str!("../../skills/argus/SKILL.md");

const NOMBRE: &str = "argus";

/// Dónde se instalaría la skill y si ya existe.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DestinoSkill {
    pub path: String,
    pub exists: bool,
}

/// Carpeta de skills de un agente dentro de `home`.
pub fn carpeta_de_skills(agente: &str, home: &Path) -> Result<PathBuf, String> {
    match agente {
        "claude" => Ok(home.join(".claude").join("skills")),
        "codex" => Ok(home.join(".codex").join("skills")),
        "opencode" => Ok(home.join(".config").join("opencode").join("skills")),
        _ => Err("Unknown agent".to_string()),
    }
}

/// Archivo `SKILL.md` de destino para un agente.
pub fn archivo_destino(agente: &str, home: &Path) -> Result<PathBuf, String> {
    Ok(carpeta_de_skills(agente, home)?
        .join(NOMBRE)
        .join("SKILL.md"))
}

pub fn destino(agente: &str, home: &Path) -> Result<DestinoSkill, String> {
    let f = archivo_destino(agente, home)?;
    Ok(DestinoSkill {
        exists: f.exists(),
        path: f.to_string_lossy().into_owned(),
    })
}

/// Instala la skill. Si ya existe y `sobrescribir` es falso, no toca nada y
/// devuelve un error.
pub fn instalar(agente: &str, home: &Path, sobrescribir: bool) -> Result<String, String> {
    let f = archivo_destino(agente, home)?;
    if f.exists() && !sobrescribir {
        return Err("The skill already exists: confirm to overwrite it".to_string());
    }
    let dir = f.parent().ok_or("Invalid skill path")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create the skills folder: {e}"))?;
    std::fs::write(&f, SKILL_MD).map_err(|e| format!("Could not write the skill: {e}"))?;
    Ok(f.to_string_lossy().into_owned())
}

fn carpeta_personal() -> Result<PathBuf, String> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .ok_or_else(|| "Could not find your home folder".to_string())
}

#[tauri::command]
pub fn skill_destino(state: State<'_, AppState>, agent: String) -> Result<DestinoSkill, String> {
    crate::i18n::localizar_error(&state, carpeta_personal().and_then(|h| destino(&agent, &h)))
}

#[tauri::command]
pub fn instalar_skill(
    state: State<'_, AppState>,
    agent: String,
    overwrite: bool,
) -> Result<String, String> {
    crate::i18n::localizar_error(
        &state,
        carpeta_personal().and_then(|h| instalar(&agent, &h, overwrite)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{nombres_registrados, HERRAMIENTAS_ESCRITURA, HERRAMIENTAS_LECTURA};

    fn temporal() -> PathBuf {
        let p = std::env::temp_dir().join(format!("argus-skill-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn el_frontmatter_tiene_name_y_description() {
        let resto = SKILL_MD.strip_prefix("---\n").expect("frontmatter");
        let (fm, _) = resto.split_once("\n---\n").expect("cierre del frontmatter");
        assert!(fm.lines().any(|l| l.starts_with("name: argus")));
        assert!(fm
            .lines()
            .any(|l| l.starts_with("description: ") && l.len() > 20));
    }

    #[test]
    fn las_herramientas_citadas_existen_en_el_mcp() {
        let registradas = nombres_registrados();
        let mut citadas = Vec::new();
        for (i, trozo) in SKILL_MD.split('`').enumerate() {
            // Los trozos impares están entre comillas invertidas.
            if i % 2 == 1
                && trozo.contains('_')
                && trozo.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            {
                citadas.push(trozo.to_string());
            }
        }
        assert!(!citadas.is_empty());
        for c in &citadas {
            assert!(
                registradas.contains(c),
                "la skill cita «{c}», que no existe"
            );
        }
        for clave in ["mapa_servidor", "informe_permisos", "proponer_plantilla"] {
            assert!(citadas.iter().any(|c| c == clave), "falta citar {clave}");
        }
        assert!(SKILL_MD.contains("`estado`"));
        // Las listas del MCP siguen cubriendo todo lo citado.
        for c in &citadas {
            assert!(
                HERRAMIENTAS_LECTURA.contains(&c.as_str())
                    || HERRAMIENTAS_ESCRITURA.contains(&c.as_str())
            );
        }
    }

    #[test]
    fn cada_agente_tiene_su_carpeta() {
        let home = Path::new("/h");
        for (a, ruta) in [
            ("claude", "/h/.claude/skills/argus/SKILL.md"),
            ("codex", "/h/.codex/skills/argus/SKILL.md"),
            ("opencode", "/h/.config/opencode/skills/argus/SKILL.md"),
        ] {
            assert_eq!(archivo_destino(a, home).unwrap(), Path::new(ruta));
        }
        assert!(archivo_destino("otro", home).is_err());
    }

    #[test]
    fn instala_en_una_carpeta_temporal_y_no_sobrescribe_sin_confirmar() {
        let home = temporal();
        let d = destino("claude", &home).unwrap();
        assert!(!d.exists);
        let ruta = instalar("claude", &home, false).unwrap();
        assert_eq!(ruta, d.path);
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), SKILL_MD);
        assert!(destino("claude", &home).unwrap().exists);

        // Una skill existente (distinta) no se toca sin confirmación.
        std::fs::write(&ruta, "mine").unwrap();
        let e = instalar("claude", &home, false).unwrap_err();
        assert!(e.contains("already exists"));
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "mine");

        instalar("claude", &home, true).unwrap();
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), SKILL_MD);

        instalar("opencode", &home, false).unwrap();
        instalar("codex", &home, false).unwrap();
        assert!(home.join(".config/opencode/skills/argus/SKILL.md").exists());
        assert!(home.join(".codex/skills/argus/SKILL.md").exists());
        let _ = std::fs::remove_dir_all(&home);
    }
}
