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
/// permisos). Lo que ya existe con el mismo nombre no se repite ni se toca.
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
    let mut nombres_vistos: Vec<String> = Vec::new();
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
        if cat_existente.is_none() {
            categorias_acc.push(Accion {
                herramienta: "crear_canal".into(),
                args: json!({ "nombre": cat.nombre, "tipo": "categoria" }),
                descripcion: format!("Create category «{}»", cat.nombre),
            });
            let mut ow = Overwrites::default();
            ow.aplicar(&cat.permisos, TipoCanal::Texto);
            emitir_permisos(
                &mut permisos_acc,
                &mut avisos,
                &ow,
                &cat.nombre,
                &roles_conocidos,
            );
        }
        let mut vistos_en_cat: Vec<String> = Vec::new();
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
            let ya_existe = cat_existente.is_some_and(|ce| {
                existentes
                    .canales
                    .iter()
                    .any(|x| x.parent_id.as_deref() == Some(ce.id.as_str()) && clave(&x.name) == k)
            });
            if ya_existe {
                continue;
            }
            // Los permisos se resuelven por nombre de canal: avisa si es ambiguo.
            let en_otro_sitio = nombres_vistos.contains(&k)
                || existentes
                    .canales
                    .iter()
                    .any(|x| clave(&x.name) == k && cat_existente.is_none_or(|ce| ce.id != x.id));
            nombres_vistos.push(k);
            if en_otro_sitio {
                avisos.push(format!(
                    "Line {}: channel «{}» has the same name as another channel; its permissions may be applied to the wrong one",
                    c.linea, c.nombre
                ));
            }
            let mut args = json!({
                "nombre": c.nombre,
                "tipo": nombre_tipo(c.tipo),
                "categoria_id": cat.nombre,
            });
            match &c.descripcion {
                Some(Descripcion::Literal(t)) => args["tema"] = Value::String(t.clone()),
                Some(Descripcion::Generar(i)) => pend_rel.push((
                    canales_acc.len(),
                    cat.nombre.clone(),
                    c.nombre.clone(),
                    i.clone(),
                )),
                None => {}
            }
            canales_acc.push(Accion {
                herramienta: "crear_canal".into(),
                args,
                descripcion: format!("Create channel «{}» in «{}»", c.nombre, cat.nombre),
            });
            let mut ow = Overwrites::default();
            ow.aplicar(&cat.permisos, c.tipo);
            ow.aplicar(&c.permisos, c.tipo);
            emitir_permisos(
                &mut permisos_acc,
                &mut avisos,
                &ow,
                &c.nombre,
                &roles_conocidos,
            );
        }
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

    fn permiso<'a>(c: &'a Compilacion, canal: &str, rol: &str) -> &'a Accion {
        c.propuesta
            .acciones
            .iter()
            .find(|a| {
                a.herramienta == "permiso_canal"
                    && a.args["channel_id"] == canal
                    && a.args["target_id"] == rol
            })
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
        assert_eq!(lista(&ev.args["denegar"]), ["ViewChannel"]);
        assert!(ev.args.get("permitir").is_none());
        let sup = permiso(&c, "Supervisión", "Supervisor");
        assert_eq!(
            lista(&sup.args["permitir"]),
            ["ViewChannel", "SendMessages"]
        );
        let dir = permiso(&c, "Supervisión", "Director");
        assert_eq!(lista(&dir.args["permitir"]), ["ViewChannel"]);
        assert!(dir.args.get("denegar").is_none());
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
            lista(&ev.args["permitir"]),
            ["ViewChannel", "ReadMessageHistory"]
        );
        assert_eq!(lista(&ev.args["denegar"]), ["SendMessages"]);
        // Voz: escribir de la categoría = Connect y Speak.
        let v = permiso(&c, "Sala de control", "Supervisor");
        assert_eq!(
            lista(&v.args["permitir"]),
            ["ViewChannel", "Connect", "Speak"]
        );
        // Ocultar a.
        let o = permiso(&c, "noticias", "Muteado");
        assert_eq!(lista(&o.args["denegar"]), ["ViewChannel"]);
        // Una categoría sin permisos no genera permiso_canal.
        assert!(!c
            .propuesta
            .acciones
            .iter()
            .any(|a| a.herramienta == "permiso_canal" && a.args["channel_id"] == "Plaza"));
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
        // supervision-1..3, Sala de control (hereda de la categoría).
        assert_eq!(c.pendientes.len(), 4);
        let p = &c.pendientes[0];
        assert_eq!(p.canal, "supervision-1");
        assert_eq!(p.categoria, "Supervisión");
        assert_eq!(p.indicacion, "larga");
        let a = &c.propuesta.acciones[p.indice];
        assert_eq!(a.args["nombre"], "supervision-1");
        assert!(a.args.get("tema").is_none());
        assert_eq!(c.pendientes[3].indicacion, "épica");
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
        // La categoría existe: no se vuelve a tocar su permiso.
        assert!(!c
            .propuesta
            .acciones
            .iter()
            .any(|a| a.herramienta == "permiso_canal" && a.args["channel_id"] == "Supervisión"));
        // Pero sus canales nuevos siguen heredando el acceso privado.
        permiso(&c, "supervision-1", "@everyone");
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
        assert!(!c
            .propuesta
            .acciones
            .iter()
            .any(|a| a.args["target_id"] == "Fantasma"));
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
    fn una_categoria_de_120_canales_es_valida() {
        let plan = analizar("## Categoría: Grande | ver: @everyone\n- texto: sala-{1..120} | descripción: generar: x");
        let (roles, canales) = servidor();
        let c = compilar(
            &plan,
            &Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert_eq!(c.pendientes.len(), 120);
        assert_eq!(acciones(&c, "crear_canal").len(), 121);
        let v = validar_acciones(&c.propuesta, &Nombres::desde(&roles, &canales));
        assert!(v.errores.is_empty(), "{:?}", v.errores);
    }
}
