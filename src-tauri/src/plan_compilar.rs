//! Compila el plan estructurado a una propuesta (FASE 13): función pura, sin IA,
//! sin red ni BD.
//!
//! `compilar(plan, existentes)` genera `crear_rol`, `crear_canal` (categorías y
//! canales, con `categoria_id` por nombre) y `permiso_canal`, sin repetir lo que
//! ya existe con el mismo nombre (roles por nombre; canales por nombre dentro de
//! su categoría). Las descripciones `generar:` no se escriben: quedan en
//! `pendientes` para la llamada barata a la IA. Los textos están en inglés.
//!
//! Reglas de permisos (los de un canal se suman a los de su categoría y los del
//! canal mandan):
//! - `ver` / `escribir` → `@everyone` sin `ViewChannel`; esos roles con `ViewChannel`.
//! - `escribir` → además `SendMessages` (o `Connect` y `Speak` en voz).
//! - `solo leer` → `ViewChannel` y `ReadMessageHistory` permitidos, `SendMessages`
//!   denegado (en voz: `Connect` permitido y `Speak` denegado).
//! - `ocultar a` → `ViewChannel` denegado.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::plan_estructurado::{Descripcion, PermisosPlan, PlanEstructurado, RolPlan, TipoCanal};
use crate::propuesta::{clave, Accion, Propuesta};
use crate::state::{Channel, Role};

const TIPO_CATEGORIA: u8 = 4;
const EVERYONE: &str = "@everyone";
/// Límites de Discord: canales dentro de una categoría y canales del servidor
/// (las categorías cuentan para los 500).
const MAX_CANALES_POR_CATEGORIA: usize = 50;
const MAX_CANALES_POR_SERVIDOR: usize = 500;

/// Lo que ya hay en el servidor (roles y canales/categorías del mapa).
#[derive(Debug, Clone, Copy)]
pub struct Existentes<'a> {
    pub roles: &'a [Role],
    pub canales: &'a [Channel],
}

/// Descripción `generar:` que la IA debe escribir después.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescripcionPendiente {
    /// Posición de la acción `crear_canal` en `propuesta.acciones`; al rellenar
    /// el `tema` de esa acción se resuelve la descripción.
    pub indice: usize,
    pub canal: String,
    pub categoria: String,
    pub indicacion: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compilacion {
    pub propuesta: Propuesta,
    pub pendientes: Vec<DescripcionPendiente>,
    /// Problemas que no impiden compilar (rol desconocido, nombre ambiguo…).
    pub avisos: Vec<String>,
}

/// Permisos acumulados de un destino (rol) sobre un canal.
struct Destino {
    rol: String,
    permitir: Vec<&'static str>,
    denegar: Vec<&'static str>,
}

#[derive(Default)]
struct Overwrites(Vec<Destino>);

impl Overwrites {
    fn destino(&mut self, rol: &str) -> &mut Destino {
        let k = clave(rol);
        let i = match self.0.iter().position(|d| clave(&d.rol) == k) {
            Some(i) => i,
            None => {
                self.0.push(Destino {
                    rol: rol.to_string(),
                    permitir: vec![],
                    denegar: vec![],
                });
                self.0.len() - 1
            }
        };
        &mut self.0[i]
    }
    fn permitir(&mut self, rol: &str, p: &'static str) {
        let d = self.destino(rol);
        d.denegar.retain(|x| *x != p);
        if !d.permitir.contains(&p) {
            d.permitir.push(p);
        }
    }
    fn denegar(&mut self, rol: &str, p: &'static str) {
        let d = self.destino(rol);
        d.permitir.retain(|x| *x != p);
        if !d.denegar.contains(&p) {
            d.denegar.push(p);
        }
    }

    /// Aplica los permisos de un bloque del plan (los posteriores mandan).
    fn aplicar(&mut self, pp: &PermisosPlan, tipo: TipoCanal) {
        let voz = tipo == TipoCanal::Voz;
        let escritura: &[&'static str] = if voz {
            &["Connect", "Speak"]
        } else {
            &["SendMessages"]
        };
        if !pp.ver.is_empty() || !pp.escribir.is_empty() {
            self.denegar(EVERYONE, "ViewChannel");
        }
        for r in &pp.ver {
            self.permitir(r, "ViewChannel");
        }
        for r in &pp.escribir {
            self.permitir(r, "ViewChannel");
            for p in escritura {
                self.permitir(r, p);
            }
        }
        for r in &pp.solo_leer {
            self.permitir(r, "ViewChannel");
            if voz {
                self.permitir(r, "Connect");
                self.denegar(r, "Speak");
            } else {
                self.permitir(r, "ReadMessageHistory");
                self.denegar(r, "SendMessages");
            }
        }
        for r in &pp.ocultar_a {
            self.denegar(r, "ViewChannel");
        }
    }
}

fn es_everyone(nombre: &str) -> bool {
    matches!(clave(nombre).as_str(), "@everyone" | "everyone")
}

fn nombre_tipo(t: TipoCanal) -> &'static str {
    match t {
        TipoCanal::Texto => "texto",
        TipoCanal::Voz => "voz",
        TipoCanal::Anuncios => "anuncios",
        TipoCanal::Foro => "foro",
    }
}

fn accion_rol(r: &RolPlan) -> Accion {
    let mut args = json!({ "nombre": r.nombre });
    if let Some(c) = &r.color {
        args["color"] = json!(c);
    }
    if let Some(s) = r.separado {
        args["separado"] = json!(s);
    }
    if let Some(m) = r.mencionable {
        args["mencionable"] = json!(m);
    }
    if !r.permisos.is_empty() {
        args["permisos"] = json!(r.permisos);
    }
    Accion {
        herramienta: "crear_rol".into(),
        args,
        descripcion: format!("Create role «{}»", r.nombre),
    }
}

/// Convierte el plan en una propuesta ordenada (roles → categorías → canales →
/// permisos de destinos existentes). Lo que ya existe no se vuelve a crear.
pub fn compilar(plan: &PlanEstructurado, existentes: &Existentes) -> Compilacion {
    let mut acciones: Vec<Accion> = Vec::new();
    let mut permisos_acc: Vec<Accion> = Vec::new();
    let mut pendientes = Vec::new();
    let mut avisos = Vec::new();

    // Roles conocidos (para avisar de nombres desconocidos en los permisos).
    let mut roles_conocidos: Vec<String> =
        existentes.roles.iter().map(|r| clave(&r.name)).collect();

    for r in &plan.roles {
        let k = clave(&r.nombre);
        if roles_conocidos.contains(&k) {
            continue;
        }
        roles_conocidos.push(k);
        acciones.push(accion_rol(r));
    }

    // Categorías y canales.
    let mut categorias_acc: Vec<Accion> = Vec::new();
    let mut canales_acc: Vec<Accion> = Vec::new();

    // (indice relativo en canales_acc, categoria, canal, indicacion)
    let mut pend_rel: Vec<(usize, String, String, String)> = Vec::new();

    for cat in &plan.categorias {
        let ck = clave(&cat.nombre);
        let cat_existente = existentes
            .canales
            .iter()
            .find(|c| c.channel_type == TIPO_CATEGORIA && clave(&c.name) == ck);
        let mut ow = Overwrites::default();
        ow.aplicar(&cat.permisos, TipoCanal::Texto);
        if let Some(existente) = cat_existente {
            emitir_permisos(
                &mut permisos_acc,
                &mut avisos,
                &ow,
                &existente.id,
                &roles_conocidos,
            );
        } else {
            let mut args = json!({ "nombre": cat.nombre, "tipo": "categoria" });
            incluir_permisos(&mut args, &mut avisos, &ow, &cat.nombre, &roles_conocidos);
            categorias_acc.push(Accion {
                herramienta: "crear_canal".into(),
                args,
                descripcion: format!("Create category «{}»", cat.nombre),
            });
        }
        let mut vistos_en_cat: Vec<String> = Vec::new();
        // Discord rechaza el canal 51 de una categoría: los que no caben se
        // omiten con aviso para que Apply no falle a mitad de la construcción.
        let mut ocupados = cat_existente.map_or(0, |ce| {
            existentes
                .canales
                .iter()
                .filter(|x| x.parent_id.as_deref() == Some(ce.id.as_str()))
                .count()
        });
        let mut sobrantes = 0usize;
        for c in &cat.canales {
            let k = clave(&c.nombre);
            if vistos_en_cat.contains(&k) {
                avisos.push(format!(
                    "Line {}: channel «{}» is repeated in «{}»; skipped",
                    c.linea, c.nombre, cat.nombre
                ));
                continue;
            }
            vistos_en_cat.push(k.clone());
            let existente = cat_existente.and_then(|ce| {
                existentes
                    .canales
                    .iter()
                    .find(|x| x.parent_id.as_deref() == Some(ce.id.as_str()) && clave(&x.name) == k)
            });
            let mut ow = Overwrites::default();
            ow.aplicar(&cat.permisos, c.tipo);
            ow.aplicar(&c.permisos, c.tipo);
            if let Some(existente) = existente {
                emitir_permisos(
                    &mut permisos_acc,
                    &mut avisos,
                    &ow,
                    &existente.id,
                    &roles_conocidos,
                );
                continue;
            }
            if ocupados >= MAX_CANALES_POR_CATEGORIA {
                sobrantes += 1;
                continue;
            }
            ocupados += 1;
            let mut args = json!({
                "nombre": c.nombre,
                "tipo": nombre_tipo(c.tipo),
                "categoria_id": cat.nombre,
            });
            // Los canales de voz no tienen descripción: Discord rechaza el
            // canal entero si se manda una. Tampoco se le pide a la IA.
            let admite_tema = nombre_tipo(c.tipo) != "voz";
            match &c.descripcion {
                _ if !admite_tema => {}
                Some(Descripcion::Literal(t)) => args["tema"] = Value::String(t.clone()),
                Some(Descripcion::Generar(i)) => pend_rel.push((
                    canales_acc.len(),
                    cat.nombre.clone(),
                    c.nombre.clone(),
                    i.clone(),
                )),
                None => {}
            }
            incluir_permisos(&mut args, &mut avisos, &ow, &c.nombre, &roles_conocidos);
            canales_acc.push(Accion {
                herramienta: "crear_canal".into(),
                args,
                descripcion: format!("Create channel «{}» in «{}»", c.nombre, cat.nombre),
            });
        }
        if sobrantes > 0 {
            avisos.push(format!(
                "Category «{}» can hold at most 50 channels (Discord's limit); {} channels were skipped. Split them into several categories",
                cat.nombre, sobrantes
            ));
        }
    }

    let total = existentes.canales.len() + categorias_acc.len() + canales_acc.len();
    if total > MAX_CANALES_POR_SERVIDOR {
        avisos.push(format!(
            "The server would have {} channels and categories; Discord allows at most 500, so Apply will stop when it reaches the limit",
            total
        ));
    }

    let base = acciones.len() + categorias_acc.len();
    for (rel, categoria, canal, indicacion) in pend_rel {
        pendientes.push(DescripcionPendiente {
            indice: base + rel,
            canal,
            categoria,
            indicacion,
        });
    }
    acciones.extend(categorias_acc);
    acciones.extend(canales_acc);
    acciones.extend(permisos_acc);

    let resumen = format!("Build from the structured plan: {} actions", acciones.len());
    Compilacion {
        propuesta: Propuesta { resumen, acciones },
        pendientes,
        avisos,
    }
}

/// Reutiliza la misma selección de destinos y permisos para la creación.
fn incluir_permisos(
    args: &mut Value,
    avisos: &mut Vec<String>,
    ow: &Overwrites,
    canal: &str,
    roles_conocidos: &[String],
) {
    let mut permisos = Vec::new();
    emitir_permisos(&mut permisos, avisos, ow, canal, roles_conocidos);
    if !permisos.is_empty() {
        args["permisos"] = Value::Array(
            permisos
                .into_iter()
                .map(|a| {
                    let mut permiso = a.args;
                    let objetivo = permiso
                        .as_object_mut()
                        .unwrap()
                        .remove("target_id")
                        .unwrap();
                    permiso.as_object_mut().unwrap().remove("channel_id");
                    permiso["objetivo"] = objetivo;
                    permiso
                })
                .collect(),
        );
    }
}

/// Convierte los permisos acumulados de un canal en acciones `permiso_canal`.
fn emitir_permisos(
    salida: &mut Vec<Accion>,
    avisos: &mut Vec<String>,
    ow: &Overwrites,
    canal: &str,
    roles_conocidos: &[String],
) {
    // @everyone primero: es el que cierra el acceso.
    let mut orden: Vec<&Destino> = ow.0.iter().filter(|d| es_everyone(&d.rol)).collect();
    orden.extend(ow.0.iter().filter(|d| !es_everyone(&d.rol)));
    for d in orden {
        if d.permitir.is_empty() && d.denegar.is_empty() {
            continue;
        }
        let objetivo = if es_everyone(&d.rol) {
            EVERYONE.to_string()
        } else if roles_conocidos.contains(&clave(&d.rol)) {
            d.rol.clone()
        } else {
            let msg = format!(
                "Role «{}» is not in the plan nor on the server; its permissions on «{}» were skipped",
                d.rol, canal
            );
            if !avisos.contains(&msg) {
                avisos.push(msg);
            }
            continue;
        };
        let mut args = json!({
            "channel_id": canal,
            "target_id": objetivo,
            "tipo": "rol",
        });
        if !d.permitir.is_empty() {
            args["permitir"] = json!(d.permitir);
        }
        if !d.denegar.is_empty() {
            args["denegar"] = json!(d.denegar);
        }
        salida.push(Accion {
            herramienta: "permiso_canal".into(),
            args,
            descripcion: format!("Set permissions of «{}» on «{}»", objetivo, canal),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_estructurado::analizar;
    use crate::propuesta::{validar_acciones, Nombres};

    fn rol(id: &str, nombre: &str) -> Role {
        Role {
            id: id.into(),
            name: nombre.into(),
            color: 0,
            position: 0,
            permissions: "0".into(),
            permissions_bitfield: None,
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }

    fn canal(id: &str, nombre: &str, tipo: u8, padre: Option<&str>) -> Channel {
        Channel {
            id: id.into(),
            name: nombre.into(),
            channel_type: tipo,
            parent_id: padre.map(String::from),
            position: 0,
            overwrites: None,
            permissions_bitfield: None,
            topic: None,
        }
    }

    const EJEMPLO: &str = "\
## Rol: Supervisor | color: #ffaa00 | permisos: ManageMessages
## Rol: Director
## Categoría: Supervisión | ver: Supervisor, Director | escribir: Supervisor | descripción: generar: épica
- texto: supervision-{1..3} | descripción: generar: larga
- texto: reglas | descripción: \"Normas del área.\" | solo leer: @everyone
- voz: Sala de control | ver: Supervisor
## Categoría: Plaza
- anuncios: noticias | ocultar a: Muteado
";

    fn acciones<'a>(c: &'a Compilacion, h: &str) -> Vec<&'a Accion> {
        c.propuesta
            .acciones
            .iter()
            .filter(|a| a.herramienta == h)
            .collect()
    }

    fn permiso<'a>(c: &'a Compilacion, canal: &str, rol: &str) -> &'a Value {
        c.propuesta
            .acciones
            .iter()
            .find(|a| a.herramienta == "crear_canal" && a.args["nombre"] == canal)
            .and_then(|a| a.args["permisos"].as_array())
            .and_then(|ps| ps.iter().find(|p| p["objetivo"] == rol))
            .unwrap_or_else(|| panic!("sin permiso {rol} en {canal}"))
    }

    fn lista(v: &Value) -> Vec<&str> {
        v.as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    fn servidor() -> (Vec<Role>, Vec<Channel>) {
        (vec![rol("555", "@everyone"), rol("9", "Muteado")], vec![])
    }

    #[test]
    fn el_mapa_de_ejemplo_genera_roles_categorias_y_canales() {
        let plan = analizar(EJEMPLO);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert_eq!(acciones(&c, "crear_rol").len(), 2);
        let crear = acciones(&c, "crear_canal");
        assert_eq!(crear.len(), 2 + 5 + 1); // 2 categorías + 5 + 1 canales
        assert_eq!(crear[0].args["tipo"], "categoria");
        assert_eq!(crear[0].args["nombre"], "Supervisión");
        let sup1 = crear
            .iter()
            .find(|a| a.args["nombre"] == "supervision-1")
            .unwrap();
        assert_eq!(sup1.args["categoria_id"], "Supervisión");
        assert_eq!(sup1.args["tipo"], "texto");
        let reglas = crear.iter().find(|a| a.args["nombre"] == "reglas").unwrap();
        assert_eq!(reglas.args["tema"], "Normas del área.");
        let voz = crear
            .iter()
            .find(|a| a.args["nombre"] == "Sala de control")
            .unwrap();
        assert_eq!(voz.args["tipo"], "voz");
        let rol_sup = &acciones(&c, "crear_rol")[0].args;
        assert_eq!(rol_sup["color"], "#ffaa00");
        assert_eq!(rol_sup["permisos"], json!(["ManageMessages"]));
        assert!(c.avisos.is_empty(), "{:?}", c.avisos);
    }

    #[test]
    fn ver_y_escribir_en_la_categoria() {
        let plan = analizar(EJEMPLO);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        let ev = permiso(&c, "Supervisión", "@everyone");
        assert_eq!(lista(&ev["denegar"]), ["ViewChannel"]);
        assert!(ev.get("permitir").is_none());
        let sup = permiso(&c, "Supervisión", "Supervisor");
        assert_eq!(lista(&sup["permitir"]), ["ViewChannel", "SendMessages"]);
        let dir = permiso(&c, "Supervisión", "Director");
        assert_eq!(lista(&dir["permitir"]), ["ViewChannel"]);
        assert!(dir.get("denegar").is_none());
    }

    #[test]
    fn solo_leer_y_voz_y_ocultar() {
        let plan = analizar(EJEMPLO);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        // Solo leer @everyone en un canal de categoría privada: el canal manda.
        let ev = permiso(&c, "reglas", "@everyone");
        assert_eq!(
            lista(&ev["permitir"]),
            ["ViewChannel", "ReadMessageHistory"]
        );
        assert_eq!(lista(&ev["denegar"]), ["SendMessages"]);
        // Voz: escribir de la categoría = Connect y Speak.
        let v = permiso(&c, "Sala de control", "Supervisor");
        assert_eq!(lista(&v["permitir"]), ["ViewChannel", "Connect", "Speak"]);
        // Ocultar a.
        let o = permiso(&c, "noticias", "Muteado");
        assert_eq!(lista(&o["denegar"]), ["ViewChannel"]);
        // Una categoría sin permisos no genera permiso_canal.
        assert!(!c
            .propuesta
            .acciones
            .iter()
            .any(|a| a.herramienta == "permiso_canal" && a.args["channel_id"] == "Plaza"));
    }

    #[test]
    fn archivo_de_sitio_19_compila_a_siete_acciones_con_los_mismos_permisos() {
        let plan = analizar(
            "# Archivo de Sitio-19\n\
            ## Rol: Archivista\n\
            ## Categoría: Archivo | ver: Archivista | escribir: Archivista | ocultar a: Muteado\n\
            - texto: expediente-{1..5}",
        );
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(c.avisos.is_empty());
        assert_eq!(c.propuesta.acciones.len(), 7);
        assert_eq!(acciones(&c, "crear_rol").len(), 1);
        assert_eq!(acciones(&c, "crear_canal").len(), 6);
        assert!(acciones(&c, "permiso_canal").is_empty());
        assert_eq!(
            c.propuesta.resumen,
            "Build from the structured plan: 7 actions"
        );
        for nombre in
            std::iter::once("Archivo".to_string()).chain((1..=5).map(|i| format!("expediente-{i}")))
        {
            assert_eq!(
                permiso(&c, &nombre, "@everyone"),
                &json!({
                    "objetivo": "@everyone", "tipo": "rol", "denegar": ["ViewChannel"]
                })
            );
            assert_eq!(
                permiso(&c, &nombre, "Muteado"),
                &json!({
                    "objetivo": "Muteado", "tipo": "rol", "denegar": ["ViewChannel"]
                })
            );
            assert_eq!(
                permiso(&c, &nombre, "Archivista"),
                &json!({
                    "objetivo": "Archivista", "tipo": "rol",
                    "permitir": ["ViewChannel", "SendMessages"]
                })
            );
        }
        let v = crate::propuesta::validar_traducida(
            &c.propuesta,
            &Nombres::desde(&roles, &canales),
            "es",
        );
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        assert!(v
            .cambios
            .contains(&"Permisos incluidos en la creación: 18".to_string()));
        assert!(!v.cambios.iter().any(|c| c.contains("a cambiar")));
    }

    #[test]
    fn solo_leer_un_rol_dentro_de_categoria_privada() {
        let plan = analizar(
            "## Rol: X\n## Categoría: Privada | escribir: X\n\
            - texto: lectura | solo leer: X",
        );
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(acciones(&c, "permiso_canal").is_empty());
        assert_eq!(
            lista(&permiso(&c, "lectura", "@everyone")["denegar"]),
            ["ViewChannel"]
        );
        let x = permiso(&c, "lectura", "X");
        assert_eq!(lista(&x["permitir"]), ["ViewChannel", "ReadMessageHistory"]);
        assert_eq!(lista(&x["denegar"]), ["SendMessages"]);
        assert_eq!(
            lista(&permiso(&c, "Privada", "X")["permitir"]),
            ["ViewChannel", "SendMessages"]
        );
    }

    #[test]
    fn los_canales_de_voz_no_llevan_descripcion_ni_piden_ia() {
        let p = analizar(
            "## Categoría: Labs | descripción: generar: científica\n\
             - voz: Sala | descripción: \"literal\"\n- voz: Otra\n- texto: lab",
        );
        let (roles, canales) = (Vec::new(), Vec::new());
        let c = compilar(
            &p,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        let voces: Vec<&Accion> = c
            .propuesta
            .acciones
            .iter()
            .filter(|a| a.args.get("tipo").and_then(Value::as_str) == Some("voz"))
            .collect();
        assert_eq!(voces.len(), 2);
        assert!(voces.iter().all(|a| a.args.get("tema").is_none()));
        // Solo el canal de texto hereda el `generar:` de la categoría.
        assert_eq!(c.pendientes.len(), 1, "{:?}", c.pendientes);
    }

    #[test]
    fn las_descripciones_generar_quedan_pendientes() {
        let plan = analizar(EJEMPLO);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        // supervision-1..3. «Sala de control» es de voz: no tiene descripción
        // (Discord rechaza el canal si se le manda una), así que no se pide.
        assert_eq!(c.pendientes.len(), 3);
        let p = &c.pendientes[0];
        assert_eq!(p.canal, "supervision-1");
        assert_eq!(p.categoria, "Supervisión");
        assert_eq!(p.indicacion, "larga");
        let a = &c.propuesta.acciones[p.indice];
        assert_eq!(a.args["nombre"], "supervision-1");
        assert!(a.args.get("tema").is_none());
        assert!(c.pendientes.iter().all(|p| p.canal != "Sala de control"));
    }

    #[test]
    fn no_repite_lo_que_ya_existe() {
        let plan = analizar(EJEMPLO);
        let roles = vec![
            rol("555", "@everyone"),
            rol("9", "Muteado"),
            rol("7", "supervisor"),
        ];
        let canales = vec![
            canal("20", "supervisión", 4, None),
            canal("21", "Reglas", 0, Some("20")),
            canal("22", "supervision-2", 0, Some("99")), // otra categoría: no cuenta
        ];
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        let nombres: Vec<_> = acciones(&c, "crear_rol")
            .iter()
            .map(|a| a.args["nombre"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(nombres, ["Director"]);
        let crear: Vec<_> = acciones(&c, "crear_canal")
            .iter()
            .map(|a| a.args["nombre"].as_str().unwrap().to_string())
            .collect();
        assert!(!crear.contains(&"Supervisión".to_string()));
        assert!(!crear.contains(&"reglas".to_string()));
        assert!(crear.contains(&"supervision-2".to_string()));
        assert!(crear.contains(&"Plaza".to_string()));
        // Los destinos existentes se actualizan por ID con permiso_canal.
        assert!(acciones(&c, "permiso_canal")
            .iter()
            .any(|a| a.args["channel_id"] == "20"));
        assert!(acciones(&c, "permiso_canal")
            .iter()
            .any(|a| a.args["channel_id"] == "21"));
        assert!(acciones(&c, "permiso_canal")
            .iter()
            .all(|a| { a.args["channel_id"] == "20" || a.args["channel_id"] == "21" }));
        // Pero sus canales nuevos siguen heredando el acceso privado.
        permiso(&c, "supervision-1", "@everyone");
    }

    #[test]
    fn canal_existente_conserva_permiso_canal_y_su_resumen() {
        let plan = analizar("## Categoría: Privada | escribir: X\n- texto: lectura | solo leer: X");
        let roles = vec![rol("555", "@everyone"), rol("9", "X")];
        let canales = vec![
            canal("20", "Privada", 4, None),
            canal("21", "lectura", 0, Some("20")),
        ];
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(acciones(&c, "crear_canal").is_empty());
        assert_eq!(c.propuesta.acciones.len(), 4);
        let permisos = acciones(&c, "permiso_canal");
        let x = permisos
            .iter()
            .find(|a| a.args["channel_id"] == "21" && a.args["target_id"] == "X")
            .unwrap();
        assert_eq!(
            lista(&x.args["permitir"]),
            ["ViewChannel", "ReadMessageHistory"]
        );
        assert_eq!(lista(&x.args["denegar"]), ["SendMessages"]);
        let v = validar_acciones(&c.propuesta, &Nombres::desde(&roles, &canales));
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        assert_eq!(v.cambios, ["Channel permissions to change: 4"]);
    }

    #[test]
    fn rol_desconocido_se_omite_con_aviso() {
        let plan = analizar("## Categoría: A | ver: Fantasma\n- texto: x");
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(c.avisos.iter().any(|a| a.contains("Fantasma")));
        assert!(acciones(&c, "crear_canal").iter().all(|a| {
            a.args["permisos"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["objetivo"] != "Fantasma")
        }));
    }

    #[test]
    fn la_propuesta_compilada_es_valida() {
        let plan = analizar(EJEMPLO);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        let v = validar_acciones(&c.propuesta, &Nombres::desde(&roles, &canales));
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        assert!(v.cambios.iter().any(|x| x == "Roles to create: 2"));
        assert!(v.cambios.iter().any(|x| x == "Categories to create: 2"));
        assert!(v.cambios.iter().any(|x| x == "Channels to create: 6"));
    }

    #[test]
    fn ciento_veinte_canales_en_tres_categorias_es_valido() {
        let plan = analizar(
            "## Categoría: Salas A | ver: @everyone\n- texto: sala-{1..40} | descripción: generar: x\n\
             ## Categoría: Salas B | ver: @everyone\n- texto: sala-{41..80} | descripción: generar: x\n\
             ## Categoría: Salas C | ver: @everyone\n- texto: sala-{81..120} | descripción: generar: x",
        );
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(c.avisos.is_empty(), "{:?}", c.avisos);
        assert_eq!(c.pendientes.len(), 120);
        assert_eq!(acciones(&c, "crear_canal").len(), 123);
        let v = validar_acciones(&c.propuesta, &Nombres::desde(&roles, &canales));
        assert!(v.errores.is_empty(), "{:?}", v.errores);
    }

    #[test]
    fn una_categoria_no_pasa_de_50_canales() {
        let plan =
            analizar("## Categoría: Grande\n- texto: sala-{1..120} | descripción: generar: x");
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        // La categoría y sus primeros 50 canales; los 70 restantes, con aviso.
        assert_eq!(acciones(&c, "crear_canal").len(), 51);
        assert_eq!(c.pendientes.len(), 50);
        assert!(c
            .avisos
            .iter()
            .any(|a| a.contains("«Grande»") && a.contains("70 channels were skipped")));
        for p in &c.pendientes {
            assert_eq!(c.propuesta.acciones[p.indice].herramienta, "crear_canal");
        }
    }

    #[test]
    fn el_limite_cuenta_los_canales_que_ya_hay_en_la_categoria() {
        let plan = analizar("## Categoría: Llena\n- texto: nueva-{1..5}");
        let (roles, _) = servidor();
        let mut canales = vec![canal("700", "Llena", 4, None)];
        for i in 0..48 {
            canales.push(canal(
                &format!("8{i:02}"),
                &format!("vieja-{i}"),
                0,
                Some("700"),
            ));
        }
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert_eq!(acciones(&c, "crear_canal").len(), 2);
        assert!(c
            .avisos
            .iter()
            .any(|a| a.contains("3 channels were skipped")));
    }

    #[test]
    fn avisa_si_el_servidor_pasaria_de_500_canales() {
        let mut texto = String::new();
        for i in 0..11 {
            texto.push_str(&format!(
                "## Categoría: Zona {i}\n- texto: sala-{{1..45}}\n"
            ));
        }
        let plan = analizar(&texto);
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(
            c.avisos
                .iter()
                .any(|a| a.contains("Discord allows at most 500")),
            "{:?}",
            c.avisos
        );
    }
}

/// Los ejemplos de la documentación y la plantilla de la app deben analizarse y
/// compilarse sin avisos ni errores: es lo primero que copia un usuario.
#[cfg(test)]
mod tests_ejemplos {
    use super::*;
    use crate::plan_estructurado::analizar;
    use crate::propuesta::{validar_acciones, Nombres};
    use crate::state::Role;

    const DOC_EN: &str = include_str!("../../docs/PLAN_SERVIDOR.md");
    const DOC_ES: &str = include_str!("../../docs/PLAN_SERVIDOR.es.md");
    const VISTA: &str = include_str!("../../src/components/AsistenteView.tsx");

    /// Bloques de código con estructura (alguna línea `## `).
    fn planes_de(doc: &str) -> Vec<String> {
        let mut planes = Vec::new();
        let mut dentro: Option<Vec<&str>> = None;
        for l in doc.lines() {
            if l.trim_start().starts_with("```") {
                match dentro.take() {
                    Some(b) => {
                        if b.iter().any(|x| x.starts_with("## ")) {
                            planes.push(b.join("\n"));
                        }
                    }
                    None => dentro = Some(Vec::new()),
                }
            } else if let Some(b) = dentro.as_mut() {
                b.push(l);
            }
        }
        planes
    }

    /// Las líneas de la plantilla que no pasan por `t()` (las de estructura).
    fn plantilla() -> String {
        let i = VISTA
            .find("const insertarPlantilla")
            .expect("insertarPlantilla");
        let cuerpo = &VISTA[i..];
        let cuerpo = &cuerpo[..cuerpo.find("];").expect("fin de la plantilla")];
        cuerpo
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('\''))
            .map(|l| l.trim_end_matches(',').trim_matches('\'').to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn rol(nombre: &str) -> Role {
        Role {
            id: "1".into(),
            name: nombre.into(),
            color: 0,
            position: 0,
            permissions: "0".into(),
            permissions_bitfield: None,
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }

    /// `@everyone` más los roles que el documento declara en cualquier bloque
    /// (los fragmentos de sintaxis usan roles definidos en otro fragmento).
    fn roles_del_documento(planes: &[String]) -> Vec<Role> {
        let mut roles = vec![rol("@everyone")];
        for p in planes {
            roles.extend(analizar(p).roles.iter().map(|r| rol(&r.nombre)));
        }
        roles
    }

    fn comprobar(nombre: &str, texto: &str, roles: &[Role]) {
        let plan = analizar(texto);
        assert!(plan.tiene_estructura(), "{nombre}: sin estructura");
        assert!(plan.avisos.is_empty(), "{nombre}: {:?}", plan.avisos);
        assert!(
            plan.texto_libre.is_empty(),
            "{nombre}: {:?}",
            plan.texto_libre
        );
        // Los roles del propio bloque se crean: no cuentan como existentes.
        let propios: Vec<String> = plan.roles.iter().map(|r| clave(&r.nombre)).collect();
        let roles: Vec<Role> = roles
            .iter()
            .filter(|r| !propios.contains(&clave(&r.name)))
            .cloned()
            .collect();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &[],
            },
        );
        assert!(c.avisos.is_empty(), "{nombre}: {:?}", c.avisos);
        let v = validar_acciones(&c.propuesta, &Nombres::desde(&roles, &[]));
        assert!(v.errores.is_empty(), "{nombre}: {:?}", v.errores);
    }

    #[test]
    fn los_ejemplos_de_la_documentacion_compilan_sin_avisos() {
        for (doc, texto) in [
            ("PLAN_SERVIDOR.md", DOC_EN),
            ("PLAN_SERVIDOR.es.md", DOC_ES),
        ] {
            let planes = planes_de(texto);
            assert!(planes.len() >= 3, "{doc}: solo {} planes", planes.len());
            let roles = roles_del_documento(&planes);
            for (n, p) in planes.iter().enumerate() {
                comprobar(&format!("{doc} bloque {n}"), p, &roles);
            }
        }
    }

    #[test]
    fn la_plantilla_de_la_app_compila_sin_avisos() {
        let p = plantilla();
        assert!(p.contains("## Category: Staff"), "{p}");
        comprobar("plantilla", &p, &[rol("@everyone")]);
    }
}
