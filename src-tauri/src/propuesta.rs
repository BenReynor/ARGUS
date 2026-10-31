//! Propuesta estructurada del asistente: lista de acciones (herramientas de
//! escritura del MCP) que el usuario revisa y ARGUS ejecuta en un orden seguro.
//!
//! Todo lo de este archivo es puro (sin red ni BD): ordenar, resolver nombres a
//! IDs y validar. Quien ejecuta es `ArgusMcp::ejecutar_propuesta`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mcp::validar_accion;
use crate::state::{AppState, Channel, Role};

/// Tipo de canal de Discord que es una categoría.
const TIPO_CATEGORIA: u8 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Accion {
    /// Nombre de una herramienta de escritura del MCP (`crear_rol`, …).
    pub herramienta: String,
    /// Argumentos de la herramienta. Donde iría un ID de rol, canal o categoría
    /// puede ir su nombre.
    #[serde(default)]
    pub args: Value,
    /// Texto legible para la vista previa.
    #[serde(default)]
    pub descripcion: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Propuesta {
    #[serde(default)]
    pub resumen: String,
    #[serde(default)]
    pub acciones: Vec<Accion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EstadoAccion {
    Ok,
    Error,
    /// No se intentó: una referencia por nombre no se pudo resolver.
    Omitida,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultadoAccion {
    pub indice: usize,
    pub herramienta: String,
    pub descripcion: String,
    pub estado: EstadoAccion,
    pub mensaje: String,
}

/// Resumen final de aplicar una propuesta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumenPropuesta {
    pub total: usize,
    pub hechas: usize,
    pub fallidas: usize,
    pub omitidas: usize,
    /// Acciones sin intentar porque se canceló a mitad.
    pub canceladas: usize,
    pub resultados: Vec<ResultadoAccion>,
}

impl ResumenPropuesta {
    pub fn de(total: usize, resultados: Vec<ResultadoAccion>) -> Self {
        let cuenta = |e: EstadoAccion| resultados.iter().filter(|r| r.estado == e).count();
        Self {
            total,
            hechas: cuenta(EstadoAccion::Ok),
            fallidas: cuenta(EstadoAccion::Error),
            omitidas: cuenta(EstadoAccion::Omitida),
            canceladas: total - resultados.len(),
            resultados,
        }
    }
}

/// Evento que se emite tras cada acción aplicada.
pub const EVENTO_ACCION: &str = "asistente-accion";
/// Evento con el resumen final.
pub const EVENTO_RESUMEN: &str = "asistente-resumen";

/// Aviso de avance de una acción (evento `asistente-accion`).
#[derive(Debug, Clone, Serialize)]
pub struct AvanceAccion {
    pub indice: usize,
    pub total: usize,
    pub descripcion: String,
    pub estado: EstadoAccion,
    pub mensaje: String,
}

impl AvanceAccion {
    pub fn de(total: usize, r: &ResultadoAccion) -> Self {
        Self {
            indice: r.indice,
            total,
            descripcion: r.descripcion.clone(),
            estado: r.estado.clone(),
            mensaje: r.mensaje.clone(),
        }
    }
}

/// Marca de cancelación de la aplicación en curso.
static CANCELAR: AtomicBool = AtomicBool::new(false);

/// Aplica la propuesta aprobada: ejecuta las acciones en orden seguro con la
/// lógica del MCP y emite `asistente-accion` por cada una.
#[tauri::command]
pub async fn aplicar_propuesta(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    propuesta: Propuesta,
) -> Result<ResumenPropuesta, String> {
    use tauri::Emitter;
    // El servidor es el elegido en la app, no el «servidor activo» del MCP
    // (que la app nunca fija): sin esto, aplicar fallaba con «No active server».
    let guild_id = state
        .get_active_guild()
        .ok_or_else(|| "No server selected: pick one in the left column".to_string())?;
    let propuesta = con_servidor(&propuesta, &guild_id);
    let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let mcp = crate::mcp::ArgusMcp::nuevo(pool, None).await?;
    CANCELAR.store(false, Ordering::SeqCst);
    let emisor = app.clone();
    let mut avance = move |total: usize, r: &ResultadoAccion| {
        if let Err(e) = emisor.emit(EVENTO_ACCION, AvanceAccion::de(total, r)) {
            tracing::warn!("no se pudo emitir {EVENTO_ACCION}: {e}");
        }
    };
    let resumen = mcp
        .ejecutar_propuesta_con(&propuesta, &CANCELAR, &mut avance)
        .await?;
    if let Err(e) = app.emit(EVENTO_RESUMEN, resumen.clone()) {
        tracing::warn!("no se pudo emitir {EVENTO_RESUMEN}: {e}");
    }
    Ok(resumen)
}

/// Copia de `p` con `guild_id` en los args de cada acción cuya herramienta lo
/// admite y no lo trae (el asistente no lo escribe: usa el servidor activo).
pub fn con_servidor(p: &Propuesta, guild_id: &str) -> Propuesta {
    let mut copia = p.clone();
    for a in &mut copia.acciones {
        let admite = crate::mcp::esquema_args(&a.herramienta)
            .is_some_and(|e| e["properties"].get("guild_id").is_some());
        if let (true, Some(args)) = (admite, a.args.as_object_mut()) {
            args.entry("guild_id")
                .or_insert_with(|| Value::String(guild_id.to_string()));
        }
    }
    copia
}

/// Cancela la aplicación en curso: la acción en vuelo termina y no empieza otra.
#[tauri::command]
pub fn cancelar_asistente() {
    CANCELAR.store(true, Ordering::SeqCst);
    crate::asistente::cancelar_agente();
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorValidacion {
    /// Posición (desde 0) en la propuesta ya ordenada.
    pub indice: usize,
    pub herramienta: String,
    pub mensaje: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Validacion {
    /// Cambios legibles («Roles to create: 2», …).
    pub cambios: Vec<String>,
    pub errores: Vec<ErrorValidacion>,
}

/// Grupo de ejecución: crear/editar roles → categorías → canales → permisos de
/// canal → dar/quitar roles → resto.
/// Grupo de orden de una acción. Los permisos de una categoría que crea la
/// propuesta van antes de los canales: al crearse dentro, el canal copia los
/// permisos de su categoría (como en la app de Discord), así que tienen que
/// estar puestos ya.
fn grupo(a: &Accion, categorias: &std::collections::HashSet<String>) -> u8 {
    let sobre_categoria = || {
        a.args
            .get("channel_id")
            .and_then(Value::as_str)
            .is_some_and(|c| categorias.contains(&clave(c)))
    };
    match a.herramienta.as_str() {
        "crear_rol" | "editar_rol" => 0,
        "crear_canal" if es_categoria(&a.args) => 1,
        "permiso_canal" | "quitar_permiso_canal" if sobre_categoria() => 2,
        "crear_canal" | "editar_canal" => 3,
        "permiso_canal" | "quitar_permiso_canal" => 4,
        "dar_rol" | "quitar_rol" => 5,
        _ => 6,
    }
}

fn es_categoria(args: &Value) -> bool {
    matches!(
        args.get("tipo")
            .and_then(Value::as_str)
            .map(clave)
            .as_deref(),
        Some("categoria" | "categoría" | "category")
    )
}

/// Deja las acciones en orden seguro sin cambiar el orden relativo dentro de
/// cada grupo.
pub fn ordenar_propuesta(p: &Propuesta) -> Propuesta {
    let categorias: std::collections::HashSet<String> = p
        .acciones
        .iter()
        .filter(|a| a.herramienta == "crear_canal" && es_categoria(&a.args))
        .filter_map(|a| a.args.get("nombre").and_then(Value::as_str).map(clave))
        .collect();
    let mut acciones = p.acciones.clone();
    acciones.sort_by_key(|a| grupo(a, &categorias)); // estable
    Propuesta {
        resumen: p.resumen.clone(),
        acciones,
    }
}

/// Qué clase de cosa nombra un argumento.
#[derive(Clone, Copy, PartialEq)]
enum Ref {
    Rol,
    Categoria,
    Canal,
}

/// Argumentos de `herramienta` que pueden ser un nombre en vez de un ID.
fn referencias(herramienta: &str, args: &Value) -> Vec<(&'static str, Ref)> {
    match herramienta {
        "crear_canal" => vec![("categoria_id", Ref::Categoria)],
        "editar_canal" => vec![("channel_id", Ref::Canal), ("categoria_id", Ref::Categoria)],
        "borrar_canal" | "crear_invitacion" | "crear_webhook" | "enviar_mensaje"
        | "borrar_mensaje" | "borrar_mensajes" | "fijar_mensaje" | "desfijar_mensaje" => {
            vec![("channel_id", Ref::Canal)]
        }
        "editar_rol" | "borrar_rol" | "dar_rol" | "quitar_rol" => vec![("role_id", Ref::Rol)],
        "permiso_canal" | "quitar_permiso_canal" => {
            let mut v = vec![("channel_id", Ref::Canal)];
            let es_rol = matches!(
                args.get("tipo")
                    .and_then(Value::as_str)
                    .map(clave)
                    .as_deref(),
                Some("rol" | "role" | "0")
            );
            if es_rol {
                v.push(("target_id", Ref::Rol));
            }
            v
        }
        _ => vec![],
    }
}

/// Clave de comparación de nombres: sin mayúsculas ni espacios sobrantes, y con
/// los espacios como guiones (Discord guarda así los nombres de canales).
fn clave(t: &str) -> String {
    t.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn es_id(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// Roles y canales que ya existen o que la propuesta ya creó, por nombre.
#[derive(Debug, Default, Clone)]
pub struct Nombres {
    roles: HashMap<String, String>,
    canales: HashMap<String, String>,
    categorias: HashMap<String, String>,
}

impl Nombres {
    pub fn desde(roles: &[Role], canales: &[Channel]) -> Self {
        let mut n = Self::default();
        for r in roles {
            n.roles.insert(clave(&r.name), r.id.clone());
        }
        for c in canales {
            n.registrar_canal(&c.name, c.channel_type == TIPO_CATEGORIA, &c.id);
        }
        n
    }

    pub fn registrar_rol(&mut self, nombre: &str, id: &str) {
        self.roles.insert(clave(nombre), id.to_string());
    }

    pub fn registrar_canal(&mut self, nombre: &str, categoria: bool, id: &str) {
        if categoria {
            self.categorias.insert(clave(nombre), id.to_string());
        }
        self.canales.insert(clave(nombre), id.to_string());
    }

    fn buscar(&self, tipo: Ref, nombre: &str) -> Option<&String> {
        let mapa = match tipo {
            Ref::Rol => &self.roles,
            Ref::Categoria => &self.categorias,
            Ref::Canal => &self.canales,
        };
        mapa.get(&clave(nombre))
    }

    /// Devuelve `args` con los nombres sustituidos por IDs (`parent` vale como
    /// `categoria_id`). Falla si un nombre no existe ni se ha creado antes.
    pub fn resolver_args(&self, herramienta: &str, args: &Value) -> Result<Value, String> {
        let mut args = match args {
            Value::Null => json!({}),
            otro => otro.clone(),
        };
        let Some(obj) = args.as_object_mut() else {
            return Ok(args); // la validación de argumentos lo rechazará
        };
        if matches!(herramienta, "crear_canal" | "editar_canal")
            && !obj.contains_key("categoria_id")
        {
            if let Some(p) = obj.remove("parent") {
                obj.insert("categoria_id".to_string(), p);
            }
        }
        let refs = referencias(herramienta, &Value::Object(obj.clone()));
        for (campo, tipo) in refs {
            let Some(texto) = obj.get(campo).and_then(Value::as_str) else {
                continue;
            };
            let texto = texto.trim();
            if texto.is_empty() || es_id(texto) {
                continue;
            }
            let id = self.buscar(tipo, texto).ok_or_else(|| {
                let que = match tipo {
                    Ref::Rol => "role",
                    Ref::Categoria => "category",
                    Ref::Canal => "channel",
                };
                format!("The {que} «{texto}» does not exist and is not created before this action")
            })?;
            let id = id.clone();
            obj.insert(campo.to_string(), Value::String(id));
        }
        Ok(args)
    }

    /// Apunta lo que acaba de crear una acción, para las siguientes.
    /// `salida` es el JSON que devuelve la herramienta.
    pub fn registrar_creado(&mut self, herramienta: &str, args: &Value, salida: &str) {
        let Some(nombre) = args.get("nombre").and_then(Value::as_str) else {
            return;
        };
        let salida: Value = serde_json::from_str(salida).unwrap_or(Value::Null);
        match herramienta {
            "crear_rol" => {
                if let Some(id) = salida["rol"]["id"].as_str() {
                    self.registrar_rol(nombre, id);
                }
            }
            "crear_canal" => {
                if let Some(id) = salida["canal"]["id"].as_str() {
                    self.registrar_canal(nombre, es_categoria(args), id);
                }
            }
            _ => {}
        }
    }
}

/// Valida la propuesta sin tocar Discord: herramientas conocidas, argumentos
/// válidos y referencias por nombre que existen o se crean antes. Con ella sale
/// la lista de cambios legible.
pub fn validar_acciones(p: &Propuesta, existentes: &Nombres) -> Validacion {
    let ordenada = ordenar_propuesta(p);
    let mut nombres = existentes.clone();
    let mut errores = Vec::new();
    let (mut crear_roles, mut editar_roles) = (0, 0);
    let (mut categorias, mut canales, mut editar_canales) = (0, 0, 0);
    let (mut permisos, mut miembros_roles, mut otras) = (0, 0, 0);

    for (indice, a) in ordenada.acciones.iter().enumerate() {
        let h = a.herramienta.as_str();
        let comprobada = nombres
            .resolver_args(h, &a.args)
            .and_then(|args| validar_accion(h, &args).map(|_| args));
        let args = match comprobada {
            Ok(args) => args,
            Err(mensaje) => {
                errores.push(ErrorValidacion {
                    indice,
                    herramienta: a.herramienta.clone(),
                    mensaje,
                });
                continue;
            }
        };
        match h {
            "crear_rol" => crear_roles += 1,
            "editar_rol" => editar_roles += 1,
            "crear_canal" if es_categoria(&args) => categorias += 1,
            "crear_canal" => canales += 1,
            "editar_canal" => editar_canales += 1,
            "permiso_canal" | "quitar_permiso_canal" => permisos += 1,
            "dar_rol" | "quitar_rol" => miembros_roles += 1,
            _ => otras += 1,
        }
        // Un ID provisional basta para que las siguientes acciones resuelvan.
        if let Some(nombre) = args.get("nombre").and_then(Value::as_str) {
            match h {
                "crear_rol" => nombres.registrar_rol(nombre, "0"),
                "crear_canal" => nombres.registrar_canal(nombre, es_categoria(&args), "0"),
                _ => {}
            }
        }
    }

    let cambios = [
        ("Roles to create", crear_roles),
        ("Roles to edit", editar_roles),
        ("Categories to create", categorias),
        ("Channels to create", canales),
        ("Channels to edit", editar_canales),
        ("Channel permissions to change", permisos),
        ("Member roles to change", miembros_roles),
        ("Other actions", otras),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .map(|(texto, n)| format!("{texto}: {n}"))
    .collect();

    Validacion { cambios, errores }
}

/// Vista previa de una propuesta: cambios y errores de validación, en el idioma
/// de la UI. Solo mira el mapa guardado del servidor activo; no llama a Discord.
#[tauri::command]
pub async fn validar_propuesta(
    state: tauri::State<'_, AppState>,
    propuesta: Propuesta,
) -> Result<Validacion, String> {
    Ok(validar_en_estado(&state, &propuesta))
}

/// Lo de `validar_propuesta` sin pasar por Tauri (lo usa también el asistente).
pub fn validar_en_estado(state: &AppState, propuesta: &Propuesta) -> Validacion {
    let existentes = {
        let mapas = state.server_maps.lock().unwrap();
        state
            .get_active_guild()
            .and_then(|g| mapas.get(&g).map(|m| Nombres::desde(&m.roles, &m.channels)))
            .unwrap_or_default()
    };
    let mut v = validar_acciones(propuesta, &existentes);
    let lang = crate::i18n::idioma(state);
    v.cambios = v
        .cambios
        .iter()
        .map(|c| crate::i18n::tr(&lang, c))
        .collect();
    for e in &mut v.errores {
        e.mensaje = crate::i18n::tr(&lang, &e.mensaje);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(h: &str, args: Value) -> Accion {
        Accion {
            herramienta: h.to_string(),
            args,
            descripcion: String::new(),
        }
    }

    fn prop(acciones: Vec<Accion>) -> Propuesta {
        Propuesta {
            resumen: "r".to_string(),
            acciones,
        }
    }

    fn orden(p: &Propuesta) -> Vec<(String, String)> {
        ordenar_propuesta(p)
            .acciones
            .iter()
            .map(|a| {
                (
                    a.herramienta.clone(),
                    a.args["nombre"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    }

    fn mezcla() -> Propuesta {
        prop(vec![
            acc("dar_rol", json!({"user_id": "1", "role_id": "Mods"})),
            acc(
                "permiso_canal",
                json!({"channel_id": "c", "target_id": "Mods", "tipo": "rol"}),
            ),
            acc("crear_canal", json!({"nombre": "uno"})),
            acc(
                "crear_canal",
                json!({"nombre": "Cat A", "tipo": "categoria"}),
            ),
            acc("banear", json!({"user_id": "9"})),
            acc("crear_rol", json!({"nombre": "R1"})),
            acc("crear_canal", json!({"nombre": "dos", "tipo": "voz"})),
            acc("editar_rol", json!({"role_id": "1", "nombre": "R2"})),
            acc(
                "crear_canal",
                json!({"nombre": "Cat B", "tipo": "category"}),
            ),
            acc("quitar_rol", json!({"user_id": "1", "role_id": "2"})),
        ])
    }

    #[test]
    fn con_servidor_pone_el_guild_en_cada_accion_que_lo_admite() {
        let acc = |h: &str, args: Value| Accion {
            herramienta: h.into(),
            args,
            descripcion: String::new(),
        };
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                acc("crear_rol", json!({"nombre": "O5"})),
                acc("crear_canal", json!({"nombre": "a", "guild_id": "999"})),
                acc(
                    "permiso_canal",
                    json!({"channel_id": "a", "target_id": "O5", "tipo": "rol"}),
                ),
            ],
        };
        let c = con_servidor(&p, "555");
        assert_eq!(c.acciones[0].args["guild_id"], "555");
        assert_eq!(
            c.acciones[1].args["guild_id"], "999",
            "no pisa uno explícito"
        );
        // permiso_canal no lleva guild_id en sus params: no se añade.
        let admite = crate::mcp::esquema_args("permiso_canal").unwrap()["properties"]
            .get("guild_id")
            .is_some();
        assert_eq!(c.acciones[2].args.get("guild_id").is_some(), admite);
    }

    #[test]
    fn permisos_de_categoria_van_antes_de_sus_canales() {
        let acc = |h: &str, args: Value| Accion {
            herramienta: h.into(),
            args,
            descripcion: String::new(),
        };
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                acc(
                    "crear_canal",
                    json!({"nombre": "keter", "categoria_id": "Archivos"}),
                ),
                acc(
                    "permiso_canal",
                    json!({"channel_id": "keter", "target_id": "O5", "tipo": "rol"}),
                ),
                acc(
                    "permiso_canal",
                    json!({"channel_id": "Archivos", "target_id": "O5", "tipo": "rol"}),
                ),
                acc(
                    "crear_canal",
                    json!({"nombre": "Archivos", "tipo": "categoria"}),
                ),
                acc("crear_rol", json!({"nombre": "O5"})),
            ],
        };
        let orden: Vec<String> = ordenar_propuesta(&p)
            .acciones
            .iter()
            .map(|a| {
                format!(
                    "{} {}",
                    a.herramienta,
                    a.args.get("channel_id").or(a.args.get("nombre")).unwrap()
                )
            })
            .collect();
        assert_eq!(
            orden,
            [
                "crear_rol \"O5\"",
                "crear_canal \"Archivos\"",
                "permiso_canal \"Archivos\"",
                "crear_canal \"keter\"",
                "permiso_canal \"keter\"",
            ]
        );
    }

    #[test]
    fn ordenar_agrupa_y_conserva_el_orden_dentro_de_cada_grupo() {
        let o = orden(&mezcla());
        let hs: Vec<&str> = o.iter().map(|(h, _)| h.as_str()).collect();
        assert_eq!(
            hs,
            [
                "crear_rol",
                "editar_rol",
                "crear_canal",
                "crear_canal",
                "crear_canal",
                "crear_canal",
                "permiso_canal",
                "dar_rol",
                "quitar_rol",
                "banear"
            ]
        );
        let nombres: Vec<&str> = o.iter().map(|(_, n)| n.as_str()).collect();
        // Categorías (Cat A, Cat B) antes que canales (uno, dos), cada grupo en su orden.
        assert_eq!(&nombres[2..6], ["Cat A", "Cat B", "uno", "dos"]);
    }

    #[test]
    fn ordenar_es_idempotente_y_no_pierde_acciones() {
        let una = ordenar_propuesta(&mezcla());
        assert_eq!(una.acciones.len(), 10);
        assert_eq!(ordenar_propuesta(&una), una);
    }

    #[test]
    fn la_propuesta_se_serializa_y_acepta_campos_omitidos() {
        let p = mezcla();
        let t = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Propuesta>(&t).unwrap(), p);
        let m: Propuesta = serde_json::from_value(json!({
            "resumen": "x",
            "acciones": [{"herramienta": "crear_rol"}]
        }))
        .unwrap();
        assert_eq!(m.acciones[0].args, Value::Null);
        assert_eq!(m.acciones[0].descripcion, "");
    }

    fn existentes() -> Nombres {
        let mut n = Nombres::default();
        n.registrar_rol("Admin", "70");
        n.registrar_canal("Info", true, "80");
        n.registrar_canal("reglas", false, "81");
        n
    }

    #[test]
    fn resolver_nombres_a_ids() {
        let mut n = existentes();
        n.registrar_rol("Mods", "1000");
        n.registrar_canal("Cat A", true, "2000");
        n.registrar_canal("avisos", false, "2001");

        // `parent` vale como categoria_id; ignora mayúsculas.
        let a = n
            .resolver_args("crear_canal", &json!({"nombre": "x", "parent": "cat a"}))
            .unwrap();
        assert_eq!(a["categoria_id"], "2000");
        assert!(a.get("parent").is_none());
        // Categoría existente y canal con espacios (Discord usa guiones).
        let a = n
            .resolver_args(
                "crear_canal",
                &json!({"nombre": "x", "categoria_id": "Info"}),
            )
            .unwrap();
        assert_eq!(a["categoria_id"], "80");
        // Rol y canal por nombre en permiso_canal.
        let a = n
            .resolver_args(
                "permiso_canal",
                &json!({"channel_id": "avisos", "target_id": "Mods", "tipo": "rol"}),
            )
            .unwrap();
        assert_eq!(
            (a["channel_id"].as_str(), a["target_id"].as_str()),
            (Some("2001"), Some("1000"))
        );
        // Un ID numérico se deja tal cual, y un miembro no se resuelve como rol.
        let a = n
            .resolver_args(
                "permiso_canal",
                &json!({"channel_id": "81", "target_id": "Mods", "tipo": "miembro"}),
            )
            .unwrap();
        assert_eq!(
            (a["channel_id"].as_str(), a["target_id"].as_str()),
            (Some("81"), Some("Mods"))
        );
        // Un canal de texto no vale como categoría.
        let e = n
            .resolver_args(
                "crear_canal",
                &json!({"nombre": "x", "categoria_id": "reglas"}),
            )
            .unwrap_err();
        assert!(e.contains("category «reglas»"));
        let e = n
            .resolver_args("dar_rol", &json!({"user_id": "1", "role_id": "Fantasma"}))
            .unwrap_err();
        assert!(e.contains("role «Fantasma»"));
    }

    #[test]
    fn lo_creado_despues_pisa_a_lo_existente() {
        let mut n = existentes();
        n.registrar_creado(
            "crear_rol",
            &json!({"nombre": "admin"}),
            r#"{"rol":{"id":"999"}}"#,
        );
        let a = n
            .resolver_args("dar_rol", &json!({"user_id": "1", "role_id": "Admin"}))
            .unwrap();
        assert_eq!(a["role_id"], "999");
        n.registrar_creado(
            "crear_canal",
            &json!({"nombre": "Zona", "tipo": "categoria"}),
            r#"{"canal":{"id":"55"}}"#,
        );
        let a = n
            .resolver_args("crear_canal", &json!({"nombre": "x", "parent": "Zona"}))
            .unwrap();
        assert_eq!(a["categoria_id"], "55");
    }

    #[test]
    fn validar_resume_los_cambios_y_acepta_referencias_a_lo_creado() {
        let p = prop(vec![
            // Desordenada a propósito: el canal va antes que su categoría y rol.
            acc(
                "permiso_canal",
                json!({"channel_id": "general", "target_id": "Mods", "tipo": "rol"}),
            ),
            acc(
                "crear_canal",
                json!({"nombre": "general", "parent": "Zona"}),
            ),
            acc(
                "crear_canal",
                json!({"nombre": "Zona", "tipo": "categoria"}),
            ),
            acc("crear_rol", json!({"nombre": "Mods"})),
            acc(
                "crear_rol",
                json!({"nombre": "VIP", "permisos": ["ViewChannel"]}),
            ),
            acc("dar_rol", json!({"user_id": "5", "role_id": "VIP"})),
        ]);
        let v = validar_acciones(&p, &Nombres::default());
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        assert_eq!(
            v.cambios,
            [
                "Roles to create: 2",
                "Categories to create: 1",
                "Channels to create: 1",
                "Channel permissions to change: 1",
                "Member roles to change: 1"
            ]
        );
    }

    #[test]
    fn validar_detecta_errores_sin_tocar_discord() {
        let p = prop(vec![
            acc("inventada", json!({})),
            acc("listar_roles", json!({})),
            acc("crear_rol", json!({"color": 1})),
            acc("crear_canal", json!({"nombre": "a", "parent": "No existe"})),
            acc("dar_rol", json!({"user_id": "1", "role_id": "Admin"})),
        ]);
        let v = validar_acciones(&p, &existentes());
        // Orden seguro: crear_canal, dar_rol, y luego el resto.
        let por_herramienta = |h: &str| {
            v.errores
                .iter()
                .find(|e| e.herramienta == h)
                .unwrap_or_else(|| panic!("sin error de {h}"))
                .mensaje
                .clone()
        };
        assert!(por_herramienta("inventada").contains("Unknown action"));
        assert!(por_herramienta("listar_roles").contains("read-only"));
        assert!(por_herramienta("crear_rol").contains("Invalid arguments"));
        assert!(por_herramienta("crear_canal").contains("«No existe»"));
        assert_eq!(v.errores.len(), 4);
        // La acción válida (dar_rol a un rol existente) sí cuenta.
        assert_eq!(v.cambios, ["Member roles to change: 1"]);
    }

    #[test]
    fn un_nombre_creado_mas_tarde_no_cuenta_si_no_se_crea() {
        // El permiso se ordena tras crear roles, pero el rol nunca se crea.
        let p = prop(vec![acc(
            "permiso_canal",
            json!({"channel_id": "81", "target_id": "Mods", "tipo": "rol"}),
        )]);
        let v = validar_acciones(&p, &existentes());
        assert_eq!(v.errores.len(), 1);
        assert!(v.errores[0].mensaje.contains("role «Mods»"));
    }
}
