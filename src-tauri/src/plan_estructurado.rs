//! Análisis del plan estructurado del servidor (FASE 13): función pura, sin red ni BD.
//!
//! `analizar` convierte el texto del plan en roles, categorías con sus canales,
//! texto libre y avisos. Las líneas que no siguen la sintaxis se conservan en
//! `texto_libre`; los errores de una línea van a `avisos` con su número de línea.
//! Los mensajes de los avisos están en inglés (se traducen al salir con `i18n::tr`).

use serde::{Deserialize, Serialize};

/// Máximo de elementos que puede generar un rango (límite de canales de Discord).
pub const MAX_RANGO: u64 = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Descripcion {
    /// Texto literal, ya con `{n}` sustituido en los canales de un rango.
    Literal(String),
    /// Indicación para que la IA escriba la descripción.
    Generar(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoCanal {
    Texto,
    Voz,
    Anuncios,
    Foro,
}

/// Permisos de visibilidad de una categoría o canal (listas de nombres de rol).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PermisosPlan {
    pub ver: Vec<String>,
    pub escribir: Vec<String>,
    pub solo_leer: Vec<String>,
    pub ocultar_a: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RolPlan {
    pub nombre: String,
    pub color: Option<String>,
    pub permisos: Vec<String>,
    pub separado: Option<bool>,
    pub mencionable: Option<bool>,
    pub linea: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanalPlan {
    pub tipo: TipoCanal,
    pub nombre: String,
    pub descripcion: Option<Descripcion>,
    /// Permisos propios del canal; se suman a los de su categoría.
    pub permisos: PermisosPlan,
    pub linea: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoriaPlan {
    pub nombre: String,
    /// Valor por defecto para los canales sin descripción propia.
    pub descripcion: Option<Descripcion>,
    pub permisos: PermisosPlan,
    pub canales: Vec<CanalPlan>,
    pub linea: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aviso {
    pub linea: usize,
    pub mensaje: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanEstructurado {
    pub roles: Vec<RolPlan>,
    pub categorias: Vec<CategoriaPlan>,
    /// Líneas que no encajan en la sintaxis, tal como se escribieron.
    pub texto_libre: Vec<String>,
    pub avisos: Vec<Aviso>,
}

impl PlanEstructurado {
    /// ¿Tiene alguna estructura (roles o categorías)?
    pub fn tiene_estructura(&self) -> bool {
        !self.roles.is_empty() || !self.categorias.is_empty()
    }
}

/// Minúsculas, sin tildes y con los espacios colapsados: para comparar palabras clave.
fn norm(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        out.push(match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        });
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Recorre `s` indicando, por carácter, si está dentro de comillas.
fn fuera_de_comillas(s: &str) -> impl Iterator<Item = (usize, char, bool)> + '_ {
    let mut dentro = false;
    s.char_indices().map(move |(i, c)| {
        let estaba = dentro;
        match c {
            '"' => dentro = !dentro,
            '“' => dentro = true,
            '”' => dentro = false,
            _ => {}
        }
        // Las comillas en sí cuentan como "dentro" para no separar por ellas.
        (i, c, estaba || dentro)
    })
}

/// Quita un comentario `//` (al inicio o tras un espacio, fuera de comillas).
fn quitar_comentario(linea: &str) -> &str {
    let mut previo: Option<char> = None;
    let mut cortar = None;
    let mut iter = fuera_de_comillas(linea).peekable();
    while let Some((i, c, dentro)) = iter.next() {
        if !dentro
            && c == '/'
            && previo.is_none_or(char::is_whitespace)
            && iter.peek().is_some_and(|(_, d, _)| *d == '/')
        {
            cortar = Some(i);
            break;
        }
        previo = Some(c);
    }
    match cortar {
        Some(i) => &linea[..i],
        None => linea,
    }
}

/// Parte `s` por `sep` ignorando los separadores que estén entre comillas.
fn partir(s: &str, sep: char) -> Vec<&str> {
    let mut partes = Vec::new();
    let mut inicio = 0;
    for (i, c, dentro) in fuera_de_comillas(s) {
        if c == sep && !dentro {
            partes.push(&s[inicio..i]);
            inicio = i + c.len_utf8();
        }
    }
    partes.push(&s[inicio..]);
    partes
}

fn lista(valor: &str) -> Vec<String> {
    valor
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn color_valido(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit())
}

fn booleano(v: &str) -> Option<bool> {
    match norm(v).as_str() {
        "" | "si" | "yes" | "true" | "1" | "verdadero" => Some(true),
        "no" | "false" | "0" | "falso" => Some(false),
        _ => None,
    }
}

fn tipo_de(clave: &str) -> Option<TipoCanal> {
    match clave {
        "texto" | "text" => Some(TipoCanal::Texto),
        "voz" | "voice" => Some(TipoCanal::Voz),
        "anuncios" | "announcement" | "announcements" => Some(TipoCanal::Anuncios),
        "foro" | "forum" => Some(TipoCanal::Foro),
        _ => None,
    }
}

/// Un atributo `clave: valor` (o solo `clave`) ya separado y normalizado.
struct Atributo<'a> {
    clave: String,
    valor: &'a str,
}

fn atributo(segmento: &str) -> Atributo<'_> {
    match segmento.split_once(':') {
        Some((k, v)) => Atributo {
            clave: norm(k),
            valor: v.trim(),
        },
        None => Atributo {
            clave: norm(segmento),
            valor: "",
        },
    }
}

/// Interpreta una descripción: `"literal"` o `generar: indicación`.
fn descripcion(valor: &str) -> Result<Descripcion, String> {
    let v = valor.trim();
    if v.starts_with(['"', '“']) {
        let cuerpo = &v[v.chars().next().map_or(1, char::len_utf8)..];
        let fin = cuerpo.rfind(['"', '”']);
        return match fin {
            Some(i) => Ok(Descripcion::Literal(cuerpo[..i].to_string())),
            None => Err("Invalid description: the quoted text is not closed".into()),
        };
    }
    if let Some((k, resto)) = v.split_once(':') {
        if matches!(norm(k).as_str(), "generar" | "generate") {
            let indicacion = resto.trim();
            if indicacion.is_empty() {
                return Err("Empty hint after «generar:»".into());
            }
            return Ok(Descripcion::Generar(indicacion.to_string()));
        }
    }
    Err("Invalid description: write the text in quotes or use «generar:» followed by a hint".into())
}

/// Aplica los atributos comunes a categorías y canales (descripción y permisos).
/// Devuelve `true` si reconoció la clave.
fn atributo_comun(
    a: &Atributo,
    desc: &mut Option<Descripcion>,
    permisos: &mut PermisosPlan,
    avisos: &mut Vec<String>,
) -> bool {
    match a.clave.as_str() {
        "descripcion" | "description" | "tema" | "topic" => {
            match descripcion(a.valor) {
                Ok(d) => *desc = Some(d),
                Err(m) => avisos.push(m),
            }
            true
        }
        "ver" | "view" => {
            permisos.ver.extend(lista(a.valor));
            true
        }
        "escribir" | "write" => {
            permisos.escribir.extend(lista(a.valor));
            true
        }
        "solo leer" | "read only" => {
            permisos.solo_leer.extend(lista(a.valor));
            true
        }
        "ocultar a" | "hide from" => {
            permisos.ocultar_a.extend(lista(a.valor));
            true
        }
        _ => false,
    }
}

struct Rango {
    inicio: usize,
    fin: usize,
    desde: u64,
    hasta: u64,
    ancho: usize,
}

impl Rango {
    fn numero(&self, n: u64) -> String {
        format!("{:0>ancho$}", n, ancho = self.ancho)
    }
}

/// Busca un rango `{a..b}` en el nombre. `Err` lleva el mensaje del aviso.
fn buscar_rango(nombre: &str) -> Result<Option<Rango>, String> {
    let mut encontrado: Option<Rango> = None;
    let mut desde_pos = 0;
    while let Some(ini) = nombre[desde_pos..].find('{').map(|i| i + desde_pos) {
        let Some(fin) = nombre[ini..].find('}').map(|i| i + ini) else {
            break;
        };
        let dentro = &nombre[ini + 1..fin];
        desde_pos = fin + 1;
        let Some((a, b)) = dentro.split_once("..") else {
            continue;
        };
        let texto = &nombre[ini..=fin];
        if encontrado.is_some() {
            return Err("Only one range per line is supported".into());
        }
        let (a, b) = (a.trim(), b.trim());
        let digitos = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        let invalido = || {
            format!("Invalid range «{texto}»: use two whole numbers, the first not greater than the second")
        };
        if !digitos(a) || !digitos(b) {
            return Err(invalido());
        }
        let (Ok(da), Ok(db)) = (a.parse::<u64>(), b.parse::<u64>()) else {
            return Err(invalido());
        };
        if da > db {
            return Err(invalido());
        }
        if db - da + 1 > MAX_RANGO {
            return Err(format!(
                "Range «{texto}» is too large (maximum {MAX_RANGO} items)"
            ));
        }
        let con_cero = |s: &str| s.len() > 1 && s.starts_with('0');
        let ancho = if con_cero(a) || con_cero(b) {
            a.len().max(b.len())
        } else {
            0
        };
        encontrado = Some(Rango {
            inicio: ini,
            fin: fin + 1,
            desde: da,
            hasta: db,
            ancho,
        });
    }
    Ok(encontrado)
}

fn con_numero(d: &Descripcion, numero: Option<&str>) -> Descripcion {
    match (d, numero) {
        (Descripcion::Literal(t), Some(n)) => Descripcion::Literal(t.replace("{n}", n)),
        _ => d.clone(),
    }
}

/// Analiza el texto del plan. Nunca falla: lo que no encaja va a `texto_libre`.
pub fn analizar(texto: &str) -> PlanEstructurado {
    let mut plan = PlanEstructurado::default();

    for (idx, cruda) in texto.lines().enumerate() {
        let n_linea = idx + 1;
        let linea = quitar_comentario(cruda).trim();
        if linea.is_empty() {
            continue;
        }
        let libre = cruda.trim_end().to_string();
        let mut avisos: Vec<String> = Vec::new();

        let aceptada = if let Some(resto) = linea.strip_prefix("##") {
            cabecera(resto.trim(), n_linea, &mut plan, &mut avisos)
        } else if let Some(resto) = linea.strip_prefix("- ") {
            canal(resto.trim(), n_linea, &mut plan, &mut avisos)
        } else {
            false
        };

        if !aceptada {
            plan.texto_libre.push(libre);
        }
        plan.avisos.extend(avisos.into_iter().map(|mensaje| Aviso {
            linea: n_linea,
            mensaje,
        }));
    }
    plan
}

/// `## Rol: …` / `## Categoría: …`. `false` si no es una cabecera reconocida o
/// no se puede construir (el aviso explica por qué).
fn cabecera(
    resto: &str,
    n_linea: usize,
    plan: &mut PlanEstructurado,
    avisos: &mut Vec<String>,
) -> bool {
    let segmentos = partir(resto, '|');
    let Some((clave, nombre)) = segmentos[0].split_once(':') else {
        return false;
    };
    let clave = norm(clave);
    let es_rol = matches!(clave.as_str(), "rol" | "role");
    let es_categoria = matches!(clave.as_str(), "categoria" | "category");
    if !es_rol && !es_categoria {
        return false;
    }
    let nombre = nombre.trim();
    if nombre.is_empty() {
        avisos.push(format!(
            "Missing name after «{}»",
            segmentos[0].split(':').next().unwrap_or("").trim()
        ));
        return false;
    }

    if es_rol {
        let mut rol = RolPlan {
            nombre: nombre.to_string(),
            linea: n_linea,
            ..Default::default()
        };
        for seg in &segmentos[1..] {
            if seg.trim().is_empty() {
                continue;
            }
            let a = atributo(seg);
            match a.clave.as_str() {
                "color" => {
                    if color_valido(a.valor) {
                        rol.color = Some(a.valor.to_string());
                    } else {
                        avisos.push(format!("Invalid color «{}» (use #RRGGBB)", a.valor));
                    }
                }
                "permisos" | "permissions" => rol.permisos.extend(lista(a.valor)),
                "separado" | "hoist" | "mencionable" | "mentionable" => match booleano(a.valor) {
                    Some(b) if a.clave.starts_with(['s', 'h']) => rol.separado = Some(b),
                    Some(b) => rol.mencionable = Some(b),
                    None => avisos.push(format!(
                        "Invalid value «{}» for «{}» (use yes or no)",
                        a.valor, a.clave
                    )),
                },
                _ => avisos.push(format!("Unknown attribute «{}»", a.clave)),
            }
        }
        plan.roles.push(rol);
    } else {
        let mut cat = CategoriaPlan {
            nombre: nombre.to_string(),
            descripcion: None,
            permisos: PermisosPlan::default(),
            canales: Vec::new(),
            linea: n_linea,
        };
        for seg in &segmentos[1..] {
            if seg.trim().is_empty() {
                continue;
            }
            let a = atributo(seg);
            if !atributo_comun(&a, &mut cat.descripcion, &mut cat.permisos, avisos) {
                avisos.push(format!("Unknown attribute «{}»", a.clave));
            }
        }
        plan.categorias.push(cat);
    }
    true
}

/// `- tipo: nombre | atributos`, dentro de la última categoría.
fn canal(
    resto: &str,
    n_linea: usize,
    plan: &mut PlanEstructurado,
    avisos: &mut Vec<String>,
) -> bool {
    let segmentos = partir(resto, '|');
    let Some((clave, nombre)) = segmentos[0].split_once(':') else {
        return false;
    };
    let Some(tipo) = tipo_de(&norm(clave)) else {
        return false;
    };
    // Una categoría solo "contiene" canales si fue la última cabecera de estructura
    // (un rol posterior la cierra): se comprueba por posición de línea.
    let Some(cat_idx) = plan.categorias.len().checked_sub(1) else {
        return false;
    };
    if plan
        .roles
        .last()
        .is_some_and(|r| r.linea > plan.categorias[cat_idx].linea)
    {
        return false;
    }
    let nombre = nombre.trim();
    if nombre.is_empty() {
        avisos.push(format!("Missing name after «{}»", clave.trim()));
        return false;
    }
    let rango = match buscar_rango(nombre) {
        Ok(r) => r,
        Err(m) => {
            avisos.push(m);
            return false;
        }
    };

    let mut desc = None;
    let mut permisos = PermisosPlan::default();
    for seg in &segmentos[1..] {
        if seg.trim().is_empty() {
            continue;
        }
        let a = atributo(seg);
        if !atributo_comun(&a, &mut desc, &mut permisos, avisos) {
            avisos.push(format!("Unknown attribute «{}»", a.clave));
        }
    }

    let cat = &mut plan.categorias[cat_idx];
    let por_defecto = cat.descripcion.clone();
    let mut crear = |nombre: String, numero: Option<String>| {
        let d = desc
            .as_ref()
            .or(por_defecto.as_ref())
            .map(|d| con_numero(d, numero.as_deref()));
        cat.canales.push(CanalPlan {
            tipo,
            nombre,
            descripcion: d,
            permisos: permisos.clone(),
            linea: n_linea,
        });
    };
    match rango {
        None => crear(nombre.to_string(), None),
        Some(r) => {
            for n in r.desde..=r.hasta {
                let num = r.numero(n);
                let nom = format!("{}{}{}", &nombre[..r.inicio], num, &nombre[r.fin..]);
                crear(nom, Some(num));
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una_cat(texto: &str) -> CategoriaPlan {
        let p = analizar(texto);
        assert!(p.avisos.is_empty(), "avisos: {:?}", p.avisos);
        p.categorias.into_iter().next().expect("una categoría")
    }

    #[test]
    fn rol_con_todos_sus_atributos() {
        let p = analizar(
            "## Rol: Supervisor | color: #ffaa00 | permisos: ManageMessages, ModerateMembers | separado | mencionable: no",
        );
        assert!(p.avisos.is_empty(), "{:?}", p.avisos);
        assert_eq!(p.roles.len(), 1);
        let r = &p.roles[0];
        assert_eq!(r.nombre, "Supervisor");
        assert_eq!(r.color.as_deref(), Some("#ffaa00"));
        assert_eq!(r.permisos, ["ManageMessages", "ModerateMembers"]);
        assert_eq!(r.separado, Some(true));
        assert_eq!(r.mencionable, Some(false));
        assert_eq!(r.linea, 1);
    }

    #[test]
    fn palabras_clave_en_ingles_y_sin_distinguir_mayusculas_ni_tildes() {
        let p = analizar(
            "## ROLE: Mod | Permissions: KickMembers | HOIST: yes | mentionable\n\
             ## CATEGORÍA: Zona | VER: Mod\n\
             - VOICE: Sala\n\
             - Anuncios: Noticias\n\
             - FORUM: Ideas\n\
             - texto: chat\n\
             ## Category: Other | Description: \"x\"",
        );
        assert!(p.avisos.is_empty(), "{:?}", p.avisos);
        assert_eq!(p.roles[0].permisos, ["KickMembers"]);
        assert_eq!(p.roles[0].separado, Some(true));
        assert_eq!(p.roles[0].mencionable, Some(true));
        assert_eq!(p.categorias.len(), 2);
        let tipos: Vec<_> = p.categorias[0].canales.iter().map(|c| c.tipo).collect();
        assert_eq!(
            tipos,
            [
                TipoCanal::Voz,
                TipoCanal::Anuncios,
                TipoCanal::Foro,
                TipoCanal::Texto
            ]
        );
        assert_eq!(p.categorias[0].permisos.ver, ["Mod"]);
    }

    #[test]
    fn tildes_en_las_claves_se_ignoran() {
        let c = una_cat("## Categoría: A | descripción: \"d\" | Solo Leér: @everyone");
        assert_eq!(c.descripcion, Some(Descripcion::Literal("d".into())));
        assert_eq!(c.permisos.solo_leer, ["@everyone"]);
    }

    #[test]
    fn comentarios_de_linea_y_de_final_de_linea() {
        let p = analizar(
            "// todo comentado\n## Rol: A // comentario\n## Categoría: C // otra\n- texto: x\n  // otro\n## Categoría: D | descripción: \"ver http://ejemplo.com // no es comentario\"",
        );
        assert!(p.avisos.is_empty());
        assert!(p.texto_libre.is_empty(), "{:?}", p.texto_libre);
        assert_eq!(p.roles[0].nombre, "A");
        assert_eq!(
            p.categorias[1].descripcion,
            Some(Descripcion::Literal(
                "ver http://ejemplo.com // no es comentario".into()
            ))
        );
    }

    #[test]
    fn rango_simple_repite_la_linea() {
        let c = una_cat("## Categoría: C\n- texto: supervision-{1..10}");
        let nombres: Vec<_> = c.canales.iter().map(|c| c.nombre.as_str()).collect();
        assert_eq!(nombres.len(), 10);
        assert_eq!(nombres[0], "supervision-1");
        assert_eq!(nombres[9], "supervision-10");
    }

    #[test]
    fn rango_con_ceros_conserva_el_ancho() {
        let c = una_cat("## Categoría: C\n- voz: sala-{01..10}");
        assert_eq!(c.canales[0].nombre, "sala-01");
        assert_eq!(c.canales[8].nombre, "sala-09");
        assert_eq!(c.canales[9].nombre, "sala-10");
    }

    #[test]
    fn rango_invalido_da_aviso_con_su_linea_y_no_pierde_la_linea() {
        for malo in ["{5..2}", "{a..3}", "{1..}", "{1..600}"] {
            let texto = format!("## Categoría: C\n- texto: x-{malo}");
            let p = analizar(&texto);
            assert_eq!(p.avisos.len(), 1, "{malo}");
            assert_eq!(p.avisos[0].linea, 2, "{malo}");
            assert!(p.avisos[0].mensaje.contains(malo), "{malo}");
            assert!(p.categorias[0].canales.is_empty());
            assert_eq!(p.texto_libre, [format!("- texto: x-{malo}")]);
        }
    }

    #[test]
    fn dos_rangos_en_una_linea_dan_aviso() {
        let p = analizar("## Categoría: C\n- texto: a-{1..2}-{1..2}");
        assert_eq!(p.avisos.len(), 1);
        assert_eq!(p.avisos[0].linea, 2);
    }

    #[test]
    fn llaves_que_no_son_rango_se_dejan_tal_cual() {
        let c = una_cat("## Categoría: C\n- texto: a{b}c");
        assert_eq!(c.canales[0].nombre, "a{b}c");
    }

    #[test]
    fn color_mal_escrito_da_aviso_pero_el_rol_se_crea() {
        let p = analizar("## Rol: A\n## Rol: B | color: rojo\n## Rol: C | color: #12345");
        assert_eq!(p.roles.len(), 3);
        assert_eq!(p.roles[1].color, None);
        let lineas: Vec<_> = p.avisos.iter().map(|a| a.linea).collect();
        assert_eq!(lineas, [2, 3]);
    }

    #[test]
    fn descripcion_literal_y_generar() {
        let c = una_cat(
            "## Categoría: C | descripción: generar: épica, sobre la vigilancia\n\
             - texto: a | descripción: \"Normas: leer, y obedecer | siempre.\"\n\
             - texto: b | tema: Generate: algo corto",
        );
        assert_eq!(
            c.descripcion,
            Some(Descripcion::Generar("épica, sobre la vigilancia".into()))
        );
        assert_eq!(
            c.canales[0].descripcion,
            Some(Descripcion::Literal(
                "Normas: leer, y obedecer | siempre.".into()
            ))
        );
        assert_eq!(
            c.canales[1].descripcion,
            Some(Descripcion::Generar("algo corto".into()))
        );
    }

    #[test]
    fn descripcion_mal_escrita_da_aviso() {
        let p = analizar("## Categoría: C\n- texto: a | descripción: sin comillas\n- texto: b | descripción: generar:");
        assert_eq!(p.avisos.len(), 2);
        assert_eq!(p.avisos[0].linea, 2);
        assert_eq!(p.avisos[1].linea, 3);
        assert_eq!(p.categorias[0].canales.len(), 2);
        assert!(p.categorias[0].canales[0].descripcion.is_none());
    }

    #[test]
    fn n_en_la_descripcion_literal_es_el_numero() {
        let c = una_cat(
            "## Categoría: C\n- texto: t-{01..03} | descripción: \"Mesa {n}\"\n- texto: s | descripción: \"Sola {n}\"",
        );
        assert_eq!(
            c.canales[0].descripcion,
            Some(Descripcion::Literal("Mesa 01".into()))
        );
        assert_eq!(
            c.canales[2].descripcion,
            Some(Descripcion::Literal("Mesa 03".into()))
        );
        // Sin rango, {n} se queda como está.
        assert_eq!(
            c.canales[3].descripcion,
            Some(Descripcion::Literal("Sola {n}".into()))
        );
    }

    #[test]
    fn la_descripcion_de_la_categoria_es_el_valor_por_defecto() {
        let c = una_cat(
            "## Categoría: C | descripción: \"Zona {n}\"\n\
             - texto: a-{1..2}\n\
             - texto: b | descripción: \"Propia\"\n\
             - voz: c | descripción: generar: calma",
        );
        assert_eq!(
            c.canales[0].descripcion,
            Some(Descripcion::Literal("Zona 1".into()))
        );
        assert_eq!(
            c.canales[1].descripcion,
            Some(Descripcion::Literal("Zona 2".into()))
        );
        assert_eq!(
            c.canales[2].descripcion,
            Some(Descripcion::Literal("Propia".into()))
        );
        assert_eq!(
            c.canales[3].descripcion,
            Some(Descripcion::Generar("calma".into()))
        );
        // La categoría conserva su valor original.
        assert_eq!(c.descripcion, Some(Descripcion::Literal("Zona {n}".into())));
    }

    #[test]
    fn permisos_ver_escribir_solo_leer_y_ocultar() {
        let c = una_cat(
            "## Categoría: Sup | ver: Supervisor, Director | escribir: Supervisor\n\
             - texto: reglas | solo leer: @everyone | ocultar a: Muteado, Nuevo\n\
             - voice: sala | view: Supervisor | write: Director | read only: A | hide from: B",
        );
        assert_eq!(c.permisos.ver, ["Supervisor", "Director"]);
        assert_eq!(c.permisos.escribir, ["Supervisor"]);
        assert_eq!(c.canales[0].permisos.solo_leer, ["@everyone"]);
        assert_eq!(c.canales[0].permisos.ocultar_a, ["Muteado", "Nuevo"]);
        assert!(c.canales[0].permisos.ver.is_empty());
        let v = &c.canales[1].permisos;
        assert_eq!(
            (&v.ver, &v.escribir, &v.solo_leer, &v.ocultar_a),
            (
                &vec!["Supervisor".to_string()],
                &vec!["Director".to_string()],
                &vec!["A".to_string()],
                &vec!["B".to_string()]
            )
        );
    }

    #[test]
    fn atributo_desconocido_da_aviso_y_la_linea_se_acepta() {
        let p = analizar("## Categoría: C | color: #fff\n- texto: a | rareza: alta");
        assert_eq!(p.avisos.len(), 2);
        assert_eq!(p.avisos[1].linea, 2);
        assert_eq!(p.categorias[0].canales.len(), 1);
    }

    #[test]
    fn lineas_que_no_encajan_van_a_texto_libre() {
        let p = analizar(
            "# Mi servidor\nQuiero algo medieval.\n- comprar pan\n- texto: huérfano\n## Notas\n## Rol:\n",
        );
        assert!(!p.tiene_estructura());
        assert_eq!(
            p.texto_libre,
            [
                "# Mi servidor",
                "Quiero algo medieval.",
                "- comprar pan",
                "- texto: huérfano",
                "## Notas",
                "## Rol:"
            ]
        );
        // `## Rol:` sin nombre avisa.
        assert_eq!(p.avisos.len(), 1);
        assert_eq!(p.avisos[0].linea, 6);
    }

    #[test]
    fn un_rol_cierra_la_categoria_anterior() {
        let p = analizar("## Categoría: C\n- texto: a\n## Rol: R\n- texto: suelto");
        assert_eq!(p.categorias[0].canales.len(), 1);
        assert_eq!(p.texto_libre, ["- texto: suelto"]);
    }

    #[test]
    fn plan_mixto_estructura_y_texto_libre() {
        let texto = "\
# Servidor medieval
Quiero que se sienta como una taberna.

## Rol: Supervisor | color: #ffaa00 | permisos: ManageMessages
## Rol: Director | color: #oops
## Categoría: Supervisión | ver: Supervisor, Director | escribir: Supervisor | descripción: generar: épica
- texto: supervision-{1..3} | descripción: generar: larga
- texto: reglas | descripción: \"Normas del área.\" | solo leer: @everyone
- voz: Sala de control | ver: Supervisor
Añade también algo para eventos, ya veremos.
## Categoría: Plaza
- anuncios: noticias
";
        let p = analizar(texto);
        assert_eq!(p.roles.len(), 2);
        assert_eq!(p.categorias.len(), 2);
        assert_eq!(p.categorias[0].canales.len(), 5);
        assert_eq!(p.categorias[1].canales.len(), 1);
        assert_eq!(
            p.texto_libre,
            [
                "# Servidor medieval",
                "Quiero que se sienta como una taberna.",
                "Añade también algo para eventos, ya veremos."
            ]
        );
        assert_eq!(p.avisos.len(), 1);
        assert_eq!(p.avisos[0].linea, 5);
        assert!(p.tiene_estructura());
        // Los canales sin descripción propia heredan la de la categoría.
        assert_eq!(
            p.categorias[0].canales[4].descripcion,
            Some(Descripcion::Generar("épica".into()))
        );
        assert!(p.categorias[1].canales[0].descripcion.is_none());
    }

    #[test]
    fn rango_de_120_canales() {
        let c = una_cat(
            "## Categoría: Mesas | descripción: generar: ambiente\n\
             - texto: mesa-{001..120} | escribir: Jugador | descripción: \"Mesa {n}\"",
        );
        assert_eq!(c.canales.len(), 120);
        assert_eq!(c.canales[0].nombre, "mesa-001");
        assert_eq!(c.canales[119].nombre, "mesa-120");
        assert_eq!(
            c.canales[41].descripcion,
            Some(Descripcion::Literal("Mesa 042".into()))
        );
        assert!(c.canales.iter().all(|c| c.permisos.escribir == ["Jugador"]));
        assert!(c.canales.iter().all(|c| c.linea == 2));
        let distintos: std::collections::HashSet<_> = c.canales.iter().map(|c| &c.nombre).collect();
        assert_eq!(distintos.len(), 120);
    }
}
