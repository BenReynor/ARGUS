//! Aplicar una plantilla con el motor de propuestas.
//!
//! La plantilla se traduce a una `Propuesta` normal (crear roles, categorías y
//! canales con sus permisos), se valida contra el servidor activo y se envía al
//! asistente como cualquier otra: el usuario la aprueba y se ejecuta con
//! `aplicar_propuesta`, con su orden, su Stop y la verificación de la FASE 20.
//! Nada se reutiliza solo por compartir nombre: todo lo de la plantilla se crea.
use crate::propuesta::{Accion, Propuesta};
use crate::template_transfer::{parse, Document};
use serde_json::{json, Value};
use tauri::Emitter;

fn tipo(kind: &str) -> &'static str {
    match kind {
        "category" => "categoria",
        "voice" => "voz",
        "announcement" => "anuncios",
        "forum" => "foro",
        _ => "texto",
    }
}

/// Traduce el documento a una propuesta: roles, luego categorías y luego
/// canales, con los permisos de cada uno por nombre de rol o `@everyone`.
pub fn propuesta_de_documento(d: &Document) -> Propuesta {
    let mut acciones = Vec::new();
    let nombre_rol = |clave: &str| {
        d.roles
            .iter()
            .find(|r| r.key == clave)
            .map(|r| r.name.clone())
    };
    for r in d
        .roles
        .iter()
        .filter(|r| !r.name.eq_ignore_ascii_case("@everyone"))
    {
        acciones.push(Accion {
            herramienta: "crear_rol".into(),
            args: json!({
                "nombre": r.name,
                "color": r.color,
                "permisos": r.permissions.to_string(),
            }),
            descripcion: format!("Create role «{}»", r.name),
        });
    }
    let categorias = d.channels.iter().filter(|c| c.kind == "category");
    let resto = d.channels.iter().filter(|c| c.kind != "category");
    for c in categorias.chain(resto) {
        let mut args = json!({ "nombre": c.name, "tipo": tipo(&c.kind) });
        if let Some(padre) = c
            .parent
            .as_ref()
            .and_then(|p| d.channels.iter().find(|x| &x.key == p))
        {
            args["categoria_id"] = Value::String(padre.name.clone());
        }
        if let Some(tema) = c
            .description
            .as_ref()
            .filter(|_| !matches!(c.kind.as_str(), "voice" | "category"))
        {
            args["tema"] = Value::String(tema.clone());
        }
        let permisos: Vec<Value> = c
            .overwrites
            .iter()
            .filter_map(|o| {
                Some(json!({
                    "objetivo": nombre_rol(&o.role)?,
                    "tipo": "rol",
                    "permitir": crate::discord::nombres_api_de_permisos(o.allow),
                    "denegar": crate::discord::nombres_api_de_permisos(o.deny),
                }))
            })
            .collect();
        if !permisos.is_empty() {
            args["permisos"] = Value::Array(permisos);
        }
        let descripcion = match &c.parent {
            Some(_) => format!(
                "Create channel «{}» in «{}»",
                c.name,
                args["categoria_id"].as_str().unwrap_or_default()
            ),
            None if c.kind == "category" => format!("Create category «{}»", c.name),
            None => format!("Create channel «{}»", c.name),
        };
        acciones.push(Accion {
            herramienta: "crear_canal".into(),
            args,
            descripcion,
        });
    }
    Propuesta {
        resumen: format!("Apply template «{}»: {} actions", d.name, acciones.len()),
        acciones,
    }
}

/// Prepara la propuesta de la plantilla para el servidor activo y la envía al
/// asistente para que el usuario la revise y la apruebe.
#[tauri::command]
pub async fn proponer_plantilla(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    template_id: String,
    guild_id: String,
) -> Result<crate::asistente::EventoPropuesta, String> {
    if state.get_active_guild().as_deref() != Some(guild_id.as_str()) {
        return Err("Select this server as the active server to apply the template".into());
    }
    let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let plantilla = crate::db::get_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Template not found")?;
    let documento = parse(&plantilla.data)?;
    let idioma = crate::i18n::idioma(&state);
    let mut propuesta = propuesta_de_documento(&documento);
    propuesta.resumen = crate::i18n::tr(&idioma, &propuesta.resumen);
    for a in &mut propuesta.acciones {
        a.descripcion = crate::i18n::tr(&idioma, &a.descripcion);
    }
    let validacion = crate::propuesta::validar_en_estado(&state, &propuesta);
    let ev = crate::asistente::EventoPropuesta {
        propuesta,
        validacion,
    };
    app.emit(crate::asistente::EVENTO_PROPUESTA, ev.clone())
        .map_err(|e| e.to_string())?;
    Ok(ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propuesta::{orden_de_ejecucion, validar_acciones, Nombres};
    use crate::state::{Channel, Role};

    fn documento() -> Document {
        parse(r#"{"format":"argus-template","version":1,"name":"Comunidad","description":"","roles":[{"key":"everyone","name":"@everyone","color":0,"permissions":0},{"key":"staff","name":"Staff","color":12,"permissions":8192}],"channels":[{"key":"chat","name":"chat","kind":"text","parent":"group","description":"Normas","overwrites":[{"role":"everyone","allow":0,"deny":1024},{"role":"staff","allow":3072,"deny":0}]},{"key":"group","name":"Grupo","kind":"category","parent":null,"overwrites":[]},{"key":"sala","name":"sala","kind":"voice","parent":"group","description":"ignorado","overwrites":[]}]}"#).unwrap()
    }

    fn rol(id: &str, nombre: &str) -> Role {
        Role {
            id: id.into(),
            name: nombre.into(),
            color: 0,
            position: 1,
            permissions: "0".into(),
            permissions_bitfield: None,
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }

    fn canal(id: &str, nombre: &str, tipo: u8) -> Channel {
        Channel {
            id: id.into(),
            name: nombre.into(),
            channel_type: tipo,
            parent_id: None,
            position: 0,
            overwrites: None,
            permissions_bitfield: None,
            topic: None,
        }
    }

    #[test]
    fn template_compiles_to_an_ordered_proposal() {
        let p = propuesta_de_documento(&documento());
        let herramientas: Vec<_> = orden_de_ejecucion(&p)
            .into_iter()
            .map(|i| {
                (
                    p.acciones[i].herramienta.as_str(),
                    p.acciones[i].args["nombre"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            herramientas,
            vec![
                ("crear_rol", "Staff"),
                ("crear_canal", "Grupo"),
                ("crear_canal", "chat"),
                ("crear_canal", "sala")
            ]
        );
        let chat = &p.acciones[2].args;
        assert_eq!(chat["categoria_id"], "Grupo");
        assert_eq!(chat["tema"], "Normas");
        assert_eq!(
            chat["permisos"],
            json!([
                {"objetivo": "@everyone", "tipo": "rol", "permitir": [], "denegar": ["ViewChannel"]},
                {"objetivo": "Staff", "tipo": "rol", "permitir": ["ViewChannel", "SendMessages"], "denegar": []}
            ])
        );
        assert!(
            p.acciones[3].args.get("tema").is_none(),
            "voice channels have no topic"
        );
        assert!(p
            .acciones
            .iter()
            .all(|a| a.herramienta != "borrar_canal" && a.herramienta != "borrar_rol"));
    }

    #[test]
    fn existing_resources_with_the_same_name_are_not_reused() {
        // El servidor ya tiene «Staff» y «Grupo»: la propuesta crea los suyos
        // y valida sin errores, sin editar ni borrar lo que ya existía.
        let existentes = Nombres::desde(
            &[rol("555", "@everyone"), rol("700", "Staff")],
            &[canal("800", "Grupo", 4), canal("801", "chat", 0)],
        );
        let p = propuesta_de_documento(&documento());
        let v = validar_acciones(&p, &existentes);
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        assert_eq!(
            p.acciones
                .iter()
                .filter(|a| a.herramienta == "crear_rol")
                .count(),
            1
        );
        assert_eq!(
            p.acciones
                .iter()
                .filter(|a| a.herramienta == "crear_canal")
                .count(),
            3
        );
        // Tras crear la categoría nueva, el canal se resuelve a ella, no a la antigua.
        let mut nombres = existentes.clone();
        nombres.registrar_creado("crear_rol", &p.acciones[0].args, r#"{"rol":{"id":"900"}}"#);
        nombres.registrar_creado(
            "crear_canal",
            &p.acciones[1].args,
            r#"{"canal":{"id":"901"}}"#,
        );
        let args = nombres
            .resolver_args("crear_canal", &p.acciones[2].args)
            .unwrap();
        assert_eq!(args["categoria_id"], "901");
        assert_eq!(args["permisos"][1]["objetivo"], "900");
    }
}
