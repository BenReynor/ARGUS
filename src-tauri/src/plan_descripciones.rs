//! Descripciones de canales con una sola llamada barata a la IA (FASE 13).
//!
//! Las descripciones `generar:` que dejó `compilar` como pendientes se piden al
//! agente constructor (claude u opencode, el modelo elegido) **sin ninguna
//! herramienta** (la misma orden que el chat de Discord), con el contexto del
//! servidor y una lista compacta (id, canal, categoría, indicación). El agente
//! devuelve un JSON `{id: descripción}`; cada una admite como mucho 1024
//! caracteres (límite de Discord). Las listas largas se parten en tandas de 40.
//! Si el agente falla o falta alguna descripción, la propuesta sigue con ella
//! vacía y se añade un aviso. Los textos están en inglés.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::plan_compilar::{Compilacion, DescripcionPendiente};

/// Máximo de caracteres de una descripción (tema de canal en Discord).
pub const MAX_DESCRIPCION: usize = 1024;
/// Descripciones por llamada.
pub const TANDA: usize = 40;

/// Prompt de una tanda. El id de cada pendiente es el índice de su acción.
pub fn prompt_descripciones(
    contexto: &str,
    idioma: &str,
    tanda: &[DescripcionPendiente],
) -> String {
    let lista: Vec<Value> = tanda
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.indice.to_string(),
                "channel": p.canal,
                "category": p.categoria,
                "hint": p.indicacion,
            })
        })
        .collect();
    let contexto = if contexto.trim().is_empty() {
        "(none)"
    } else {
        contexto.trim()
    };
    let lista = Value::Array(lista);
    format!(
        "You write Discord channel descriptions. You have no tools: just answer.\n\
         Server context (theme, tone, naming style):\n{contexto}\n\n\
         Write each description in this language: {idioma}.\n\
         Each description must be at most {MAX_DESCRIPCION} characters, plain text, \
         and follow its hint.\n\
         Channels (JSON):\n{lista}\n\n\
         Reply with ONLY one JSON object mapping each id to its description, for \
         example {{\"12\": \"text\"}}. No Markdown fences, no other text."
    )
}

/// Extrae el objeto `{id: descripción}` de la respuesta (admite texto o vallas
/// alrededor). Ignora valores que no sean texto o estén vacíos y recorta cada
/// descripción a `MAX_DESCRIPCION` caracteres.
pub fn parsear_descripciones(texto: &str) -> Result<HashMap<String, String>, String> {
    let (Some(i), Some(f)) = (texto.find('{'), texto.rfind('}')) else {
        return Err("The AI reply has no JSON object".into());
    };
    if f < i {
        return Err("The AI reply has no JSON object".into());
    }
    let valor: Value = serde_json::from_str(&texto[i..=f])
        .map_err(|e| format!("The AI reply is not valid JSON: {e}"))?;
    let Some(obj) = valor.as_object() else {
        return Err("The AI reply has no JSON object".into());
    };
    let mut salida = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            let s = s.trim();
            if !s.is_empty() {
                salida.insert(k.clone(), s.chars().take(MAX_DESCRIPCION).collect());
            }
        }
    }
    Ok(salida)
}

/// Aplica lo parseado al `tema` de cada `crear_canal`; devuelve cuántas se
/// rellenaron y deja en `faltan` los canales sin descripción.
fn aplicar(
    comp: &mut Compilacion,
    tanda: &[DescripcionPendiente],
    hechas: &HashMap<String, String>,
    faltan: &mut Vec<String>,
) -> usize {
    let mut n = 0;
    for p in tanda {
        match hechas.get(&p.indice.to_string()) {
            Some(d) => {
                if let Some(a) = comp.propuesta.acciones.get_mut(p.indice) {
                    a.args["tema"] = Value::String(d.clone());
                    n += 1;
                }
            }
            None => faltan.push(p.canal.clone()),
        }
    }
    n
}

/// Una llamada al agente por tanda. Devuelve cuántas descripciones se
/// escribieron. Nunca falla: los problemas quedan en `comp.avisos`.
pub async fn escribir_descripciones(
    comp: &mut Compilacion,
    contexto: &str,
    idioma: &str,
    agente: &str,
    ruta_agente: &Path,
    modelo: &str,
    limite: Duration,
) -> usize {
    let pendientes = comp.pendientes.clone();
    let mut escritas = 0;
    for tanda in pendientes.chunks(TANDA) {
        let prompt = prompt_descripciones(contexto, idioma, tanda);
        let respuesta = match crate::chat::orden_chat(agente, ruta_agente, modelo, &prompt) {
            Ok(orden) => crate::responder::ejecutar_texto(&orden, limite).await,
            Err(e) => Err(e),
        };
        let mut faltan = Vec::new();
        match respuesta.and_then(|t| parsear_descripciones(&t)) {
            Ok(hechas) => escritas += aplicar(comp, tanda, &hechas, &mut faltan),
            Err(e) => {
                comp.avisos.push(format!(
                    "Could not write {} channel descriptions: {e}",
                    tanda.len()
                ));
                continue;
            }
        }
        if !faltan.is_empty() {
            comp.avisos.push(format!(
                "The AI did not write a description for: {}",
                faltan.join(", ")
            ));
        }
    }
    escritas
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::propuesta::{Accion, Propuesta};
    use std::os::unix::fs::PermissionsExt;

    fn comp(n: usize) -> Compilacion {
        let acciones = (0..n)
            .map(|i| Accion {
                herramienta: "crear_canal".into(),
                args: serde_json::json!({ "nombre": format!("c{i}") }),
                descripcion: String::new(),
            })
            .collect();
        let pendientes = (0..n)
            .map(|i| DescripcionPendiente {
                indice: i,
                canal: format!("c{i}"),
                categoria: "Cat".into(),
                indicacion: "epic".into(),
            })
            .collect();
        Compilacion {
            propuesta: Propuesta {
                resumen: String::new(),
                acciones,
            },
            pendientes,
            avisos: Vec::new(),
        }
    }

    fn agente(nombre: &str, script: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-desc-{}-{nombre}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("agente.sh");
        std::fs::write(&ruta, format!("#!/bin/sh\ncat > /dev/null\n{script}\n")).unwrap();
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755)).unwrap();
        ruta
    }

    async fn correr(c: &mut Compilacion, ruta: &Path) -> usize {
        escribir_descripciones(
            c,
            "medieval",
            "en",
            "claude",
            ruta,
            "haiku",
            Duration::from_secs(20),
        )
        .await
    }

    #[test]
    fn parsea_con_vallas_y_recorta() {
        let largo = "x".repeat(2000);
        let t =
            format!("```json\n{{\"1\": \"hola\", \"2\": \"\", \"3\": 5, \"4\": \"{largo}\"}}\n```");
        let m = parsear_descripciones(&t).unwrap();
        assert_eq!(m["1"], "hola");
        assert!(!m.contains_key("2") && !m.contains_key("3"));
        assert_eq!(m["4"].chars().count(), MAX_DESCRIPCION);
        assert!(parsear_descripciones("nada").is_err());
        assert!(parsear_descripciones("{roto").is_err());
    }

    #[test]
    fn el_prompt_lleva_contexto_y_lista() {
        let c = comp(2);
        let p = prompt_descripciones("tema SCP", "es", &c.pendientes);
        assert!(p.contains("tema SCP") && p.contains("\"id\":\"1\"") && p.contains("1024"));
    }

    #[tokio::test]
    async fn respuesta_valida_rellena_los_temas() {
        let ruta = agente("valida", r#"echo '{"0": "Primera", "1": "Segunda"}'"#);
        let mut c = comp(2);
        assert_eq!(correr(&mut c, &ruta).await, 2);
        assert_eq!(c.propuesta.acciones[0].args["tema"], "Primera");
        assert_eq!(c.propuesta.acciones[1].args["tema"], "Segunda");
        assert!(c.avisos.is_empty());
    }

    #[tokio::test]
    async fn respuesta_incompleta_avisa_y_deja_vacia() {
        let ruta = agente("incompleta", r#"echo '{"0": "Solo una"}'"#);
        let mut c = comp(2);
        assert_eq!(correr(&mut c, &ruta).await, 1);
        assert_eq!(c.propuesta.acciones[0].args["tema"], "Solo una");
        assert!(c.propuesta.acciones[1].args.get("tema").is_none());
        assert_eq!(c.avisos.len(), 1);
        assert!(c.avisos[0].contains("c1"));
    }

    #[tokio::test]
    async fn respuesta_rota_o_agente_caido_no_rompe() {
        for (n, s) in [("rota", "echo 'esto no es json'"), ("cae", "exit 3")] {
            let ruta = agente(n, s);
            let mut c = comp(2);
            assert_eq!(correr(&mut c, &ruta).await, 0);
            assert!(c
                .propuesta
                .acciones
                .iter()
                .all(|a| a.args.get("tema").is_none()));
            assert_eq!(c.avisos.len(), 1, "{n}");
        }
    }

    #[tokio::test]
    async fn cien_canales_son_tres_tandas() {
        // El agente cuenta sus llamadas en un archivo y responde con todos los ids.
        let dir = std::env::temp_dir().join(format!("argus-desc-cuenta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cuenta = dir.join("n");
        let _ = std::fs::remove_file(&cuenta);
        let script = format!(
            r#"echo x >> {}; i=0; printf '{{'; while [ $i -lt 100 ]; do [ $i -gt 0 ] && printf ','; printf '"%s":"d"' $i; i=$((i+1)); done; printf '}}'"#,
            cuenta.display()
        );
        let ruta = agente("tandas", &script);
        let mut c = comp(100);
        assert_eq!(correr(&mut c, &ruta).await, 100);
        let llamadas = std::fs::read_to_string(&cuenta).unwrap().lines().count();
        assert_eq!(llamadas, 3);
        assert!(c.avisos.is_empty());
    }
}
