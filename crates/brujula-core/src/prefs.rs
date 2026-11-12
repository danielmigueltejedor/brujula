//! The preferences page (`brujula://settings`): the HTML, and how a change made on it is
//! checked and stored.
//!
//! The page changes settings by navigating to `brujula://set?key=value`, which the browser
//! intercepts (and only honours when it comes from this page). Every value is validated in
//! [`Settings::apply`], so the page, a hand-typed address or a crafted link can only ever set
//! something inside the allowed range.

use crate::adblock::DEFAULT_LISTS;
use crate::bookmarks::Bookmarks;
use crate::icons;
use crate::newtab::escape;
use crate::search::ENGINES;
use crate::settings::{
    parse_hex_color, Layout, Settings, TabPosition, Theme, SIDEBAR_WIDTH_DEFAULT,
    SIDEBAR_WIDTH_MAX, SIDEBAR_WIDTH_MIN,
};
use crate::spaces::Spaces;

pub const SETTINGS_URI: &str = "brujula://settings";
pub const SET_PREFIX: &str = "brujula://set?";

/// What a change touches, so the browser redoes only that.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// Colours, sizes, theme.
    pub appearance: bool,
    /// Layout, tab position, sidebar width: the window is put together again.
    pub layout: bool,
    /// Blocker on/off, lists, sites: the rules are compiled again.
    pub blocker: bool,
    /// Synchronisation settings changed: start, stop or redo it.
    pub sync: bool,
    /// Sync now, without waiting for the next round.
    pub sync_now: bool,
}

/// `key` and `value` of a `brujula://set?key=value` address.
pub fn parse_change(uri: &str) -> Option<(String, String)> {
    if !uri.starts_with(SET_PREFIX) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    let mut pairs = url.query_pairs();
    let (k, v) = pairs.next()?;
    pairs
        .next()
        .is_none()
        .then(|| (k.into_owned(), v.into_owned()))
}

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        other => Err(format!("not a switch: {other}")),
    }
}

fn number(value: &str) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("not a number: {value}"))
}

impl Settings {
    /// Applies one change from the preferences page. Unknown keys and bad values are refused.
    pub fn apply(&mut self, key: &str, value: &str) -> Result<Effect, String> {
        let mut effect = Effect::default();
        let a = &mut self.appearance;
        match key {
            "theme" => {
                a.theme = match value {
                    "system" => Theme::System,
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    other => return Err(format!("unknown theme: {other}")),
                };
                effect.appearance = true;
            }
            "layout" => {
                a.layout = match value {
                    "islands" => Layout::Islands,
                    "frame" => Layout::Frame,
                    other => return Err(format!("unknown layout: {other}")),
                };
                effect.appearance = true;
                effect.layout = true;
            }
            "tabs" => {
                a.tabs = match value {
                    "vertical" => TabPosition::Vertical,
                    "horizontal" => TabPosition::Horizontal,
                    other => return Err(format!("unknown tab position: {other}")),
                };
                effect.appearance = true;
                effect.layout = true;
            }
            "accent" => {
                a.accent = parse_hex_color(value).ok_or("not a colour")?;
                effect.appearance = true;
            }
            "frame_color" => {
                a.frame_color = if value.is_empty() {
                    String::new()
                } else {
                    parse_hex_color(value).ok_or("not a colour")?
                };
                effect.appearance = true;
            }
            "opacity" => {
                a.opacity = number(value)?.clamp(50, 100);
                effect.appearance = true;
            }
            "radius" => {
                a.radius = number(value)?.min(28);
                effect.appearance = true;
            }
            "gap" => {
                a.gap = number(value)?.min(20);
                effect.appearance = true;
            }
            "sidebar_width" => {
                a.sidebar_width = number(value)?.clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
                effect.appearance = true;
                effect.layout = true;
            }
            "reset_appearance" => {
                *a = crate::settings::Appearance::default();
                effect.appearance = true;
                effect.layout = true;
            }
            "edge_reveal" => {
                self.edge_reveal = flag(value)?;
                effect.layout = true;
            }
            "adblock" => {
                self.adblock_enabled = flag(value)?;
                effect.blocker = true;
            }
            "engine" => {
                crate::search::by_id(value).ok_or("unknown search engine")?;
                self.search_engine = value.to_owned();
                self.search_engine_chosen = true;
            }
            "archive_after" => {
                let hours = number(value)?;
                if !crate::settings::ARCHIVE_CHOICES.contains(&hours) {
                    return Err(format!("not a choice: {hours}"));
                }
                self.archive_after_hours = hours;
            }
            "sync" => {
                self.sync.enabled = flag(value)?;
                effect.sync = true;
            }
            "sync_account" => {
                if !value.is_empty() && !crate::cloud::valid_id(value) {
                    return Err("not an account".to_owned());
                }
                self.sync.account = value.to_owned();
                effect.sync = true;
            }
            "sync_bookmarks" => {
                self.sync.bookmarks = flag(value)?;
                effect.sync = true;
            }
            "sync_spaces" => {
                self.sync.spaces = flag(value)?;
                effect.sync = true;
            }
            "sync_tabs" => {
                self.sync.tabs = flag(value)?;
                effect.sync = true;
            }
            "sync_now" => {
                effect.sync_now = true;
            }
            "allow.remove" => {
                self.set_site_allowed(value, false);
                effect.blocker = true;
            }
            _ => {
                if let Some(id) = key.strip_prefix("list.") {
                    let list = DEFAULT_LISTS
                        .iter()
                        .find(|l| l.id == id)
                        .ok_or("unknown list")?;
                    let on = flag(value)?;
                    self.disabled_lists.retain(|d| d != list.id);
                    if !on {
                        self.disabled_lists.push(list.id.to_owned());
                    }
                    effect.blocker = true;
                } else {
                    return Err(format!("unknown setting: {key}"));
                }
            }
        }
        Ok(effect)
    }
}

const SWATCHES: &[&str] = &[
    "#0c9a58", "#2563eb", "#7c3aed", "#db2777", "#e11d48", "#ea580c", "#ca8a04", "#0891b2",
    "#475569",
];

const FRAME_SWATCHES: &[&str] = &[
    "#dbe7e0", "#dbe4f3", "#e6dff3", "#f3dfe8", "#f3e6d8", "#e9e9e9", "#1a2420", "#1a1f2e",
    "#241a2e", "#2b2b2b",
];

/// Choices drawn as miniatures of the window: `(value, label, preview class)`.
fn tiles(key: &str, current: &str, options: &[(&str, &str, &str)]) -> String {
    let buttons: String = options
        .iter()
        .map(|(value, label, class)| {
            let on = if *value == current { " on" } else { "" };
            let preview = if *class == "t-system" {
                r#"<span class="mini t-light"><i class="sb"></i><i class="pg"></i></span><span class="mini t-dark over"><i class="sb"></i><i class="pg"></i></span>"#.to_owned()
            } else {
                format!(r#"<span class="mini {class}"><i class="sb"></i><i class="pg"></i></span>"#)
            };
            format!(
                r#"<button type="button" class="tile{on}" data-k="{key}" data-v="{value}"><span class="prev">{preview}</span><b>{label}</b></button>"#,
                label = escape(label)
            )
        })
        .collect();
    format!(r#"<div class="tiles" role="group">{buttons}</div>"#)
}

fn switch(key: &str, on: bool) -> String {
    format!(
        r#"<button type="button" class="switch{cls}" role="switch" aria-checked="{on}" data-k="{key}" data-toggle="1"><i></i></button>"#,
        cls = if on { " on" } else { "" },
    )
}

fn slider(key: &str, value: u32, min: u32, max: u32, unit: &str) -> String {
    let span = max.saturating_sub(min).max(1);
    let filled = value.saturating_sub(min) * 100 / span;
    format!(
        r#"<div class="slider"><input type="range" data-k="{key}" min="{min}" max="{max}" value="{value}" style="--p:{filled}%"><output>{value}{unit}</output></div>"#
    )
}

fn row(title: &str, hint: &str, control: &str) -> String {
    let hint = if hint.is_empty() {
        String::new()
    } else {
        format!(r#"<p>{}</p>"#, escape(hint))
    };
    let what = if title.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="what"><b>{}</b>{hint}</div>"#, escape(title))
    };
    format!(r#"<div class="row">{what}<div class="ctl">{control}</div></div>"#)
}

/// A row whose control is wide: the control goes under the text.
fn stacked(title: &str, hint: &str, control: &str) -> String {
    row(title, hint, control).replacen(r#"class="row""#, r#"class="row stack""#, 1)
}

fn swatches(key: &str, colours: &[&str], current: &str) -> String {
    let dots: String = colours
        .iter()
        .map(|c| {
            let on = if c.eq_ignore_ascii_case(current) {
                " on"
            } else {
                ""
            };
            format!(
                r##"<button type="button" class="dot{on}" style="background:{c}" data-k="{key}" data-v="{c}" aria-label="{c}"></button>"##
            )
        })
        .collect();
    format!(r#"<div class="dots">{dots}</div>"#)
}

/// What some sections show besides the settings: the cloud accounts and the archive.
#[derive(Debug, Default, Clone)]
pub struct Extras {
    /// The cloud accounts of the desktop: id, label, whether the folder is mounted now.
    pub accounts: Vec<(String, String, bool)>,
    /// How the last round went, in words.
    pub status: String,
    /// Archived tabs: address and title.
    pub archived: Vec<(String, String)>,
}

/// The sections of the page, in order: id, nav label, heading, one line under the heading, icon.
const PAGES: &[(&str, &str, &str, &str, &str)] = &[
    (
        "general",
        "General",
        "General",
        "El buscador que usa Brújula en todas partes.",
        "settings",
    ),
    (
        "appearance",
        "Apariencia",
        "Apariencia",
        "Tema, colores y formas. Los cambios se ven al momento.",
        "palette",
    ),
    (
        "layout",
        "Diseño",
        "Diseño",
        "Cómo se reparten la barra lateral, las pestañas y la web.",
        "layout",
    ),
    (
        "spaces",
        "Espacios",
        "Espacios",
        "Grupos de pestañas, cada uno con su icono. Se crean desde el selector de la barra lateral.",
        "workspace",
    ),
    (
        "bookmarks",
        "Marcadores",
        "Marcadores",
        "Las páginas que guardaste con la estrella.",
        "bookmark",
    ),
    (
        "archive",
        "Archivo",
        "Archivo",
        "Las pestañas que llevan tiempo sin usarse se cierran solas y quedan guardadas aquí.",
        "clock",
    ),
    (
        "sync",
        "Sincronización",
        "Sincronización",
        "Lleva marcadores, espacios y pestañas a tus otros dispositivos con tu nube.",
        "cloud",
    ),
    (
        "privacy",
        "Privacidad",
        "Privacidad",
        "Bloqueador de anuncios y rastreadores.",
        "shield-check",
    ),
    (
        "sites",
        "Sitios",
        "Sitios sin bloqueador",
        "Páginas donde el bloqueador está apagado.",
        "globe",
    ),
    (
        "security",
        "Seguridad",
        "Seguridad",
        "Lo que Brújula hace por ti sin preguntar.",
        "lock",
    ),
    (
        "about",
        "Acerca de",
        "Acerca de",
        "",
        "info",
    ),
];

fn section(id: &str, body: &str) -> String {
    let (_, _, title, hint, icon) = PAGES
        .iter()
        .find(|p| p.0 == id)
        .copied()
        .unwrap_or((id, id, id, "", "settings"));
    let hint = if hint.is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", escape(hint))
    };
    format!(
        r#"<section id="{id}"><header class="shead"><span class="sicon">{svg}</span><div><h2>{title}</h2>{hint}</div></header><div class="card">{body}</div></section>"#,
        svg = icons::web_svg(icon, 20),
        title = escape(title),
    )
}

/// The icon grid that picks a space's icon.
fn icon_grid(space_id: u32, current: &str) -> String {
    let buttons: String = icons::all()
        .iter()
        .map(|i| {
            let on = if i.id == current { " on" } else { "" };
            format!(
                r#"<button type="button" class="ibtn{on}" data-k="space.icon.{space_id}" data-v="{id}" data-pick="1" title="{label}" aria-label="{label}">{svg}</button>"#,
                id = i.id,
                label = escape(i.label),
                svg = icons::web_svg(i.id, 20),
            )
        })
        .collect();
    format!(r#"<div class="iconsgrid">{buttons}</div>"#)
}

/// The archive: when tabs are archived, and the ones that were.
fn archive_section(s: &Settings, view: &Extras) -> String {
    let choices: String = [
        (0, "Nunca"),
        (12, "12 horas"),
        (24, "1 día"),
        (168, "1 semana"),
        (720, "30 días"),
    ]
    .iter()
    .map(|(hours, label)| {
        let on = if *hours == s.archive_after_hours { " on" } else { "" };
        format!(r#"<button type="button" class="seg{on}" data-k="archive_after" data-v="{hours}">{label}</button>"#)
    })
    .collect();
    let list = if view.archived.is_empty() {
        r#"<p class="empty">Ninguna todavía.</p>"#.to_owned()
    } else {
        view.archived
            .iter()
            .map(|(url, title)| {
                let name = if title.is_empty() { url } else { title };
                format!(
                    r#"<div class="row"><div class="what"><b>{}</b><p>{}</p></div><div class="ctl"><button type="button" class="seg plain" data-k="archive.open" data-v="{url}" data-remove="1">Abrir</button><button type="button" class="seg plain danger" data-k="archive.remove" data-v="{url}" data-remove="1">Quitar</button></div></div>"#,
                    escape(name),
                    escape(url),
                    url = escape(url)
                )
            })
            .collect()
    };
    section(
        "archive",
        &[
            row(
                "Archivar las pestañas sin usar",
                "Las ancladas, las de una carpeta y la que tienes delante no se archivan.",
                &format!(r#"<div class="segs" role="group">{choices}</div>"#),
            ),
            list,
        ]
        .concat(),
    )
}

/// The synchronisation section.
fn sync_section(s: &Settings, view: &Extras) -> String {
    let c = &s.sync;
    let accounts = if view.accounts.is_empty() {
        r#"<p class="empty">Ninguna cuenta. Añade Google Drive, OneDrive, Dropbox o iCloud Drive en Ajustes, en las cuentas en la nube.</p>"#.to_owned()
    } else {
        let buttons: String = view
            .accounts
            .iter()
            .map(|(id, label, mounted)| {
                let on = if *id == c.account { " on" } else { "" };
                let state = if *mounted { "Conectada" } else { "Sin conectar" };
                format!(
                    r#"<button type="button" class="engine{on}" data-k="sync_account" data-v="{id}">{icon}<span class="acct"><b>{label}</b><small>{state}</small></span></button>"#,
                    id = escape(id),
                    icon = icons::web_svg("cloud", 20),
                    label = escape(label),
                )
            })
            .collect();
        format!(r#"<div class="engines" role="group">{buttons}</div>"#)
    };
    let body = [
        row(
            "Sincronizar",
            "Cada dispositivo guarda su estado en un archivo de tu nube y lee el de los demás. No hay servidor de Brújula.",
            &switch("sync", c.enabled),
        ),
        stacked(
            "Cuenta",
            "Las cuentas son las de Nodalix: Brújula no pide contraseñas ni inicia sesión en ningún servicio.",
            &accounts,
        ),
        row(
            "Marcadores",
            "",
            &switch("sync_bookmarks", c.bookmarks),
        ),
        row(
            "Espacios y carpetas",
            "Se unen por nombre; nunca se borra un espacio por sincronizar.",
            &switch("sync_spaces", c.spaces),
        ),
        row(
            "Pestañas abiertas",
            "Las de tus otros dispositivos aparecen en la página de nueva pestaña.",
            &switch("sync_tabs", c.tabs),
        ),
        row(
            "Estado",
            "",
            &format!(
                r#"<span id="sync-status" class="status">{}</span><button type="button" class="seg plain" data-k="sync_now" data-v="1">Sincronizar ahora</button>"#,
                escape(&view.status)
            ),
        ),
        r#"<p class="note">Los datos van a tu nube sin cifrar, en la carpeta Brújula: quien tenga acceso a tu cuenta podrá leerlos. Solo se guardan direcciones web http y https, nunca contraseñas ni cookies.</p>"#
            .to_owned(),
    ]
    .concat();
    section("sync", &body)
}

/// The preferences page for the current settings.
pub fn render(s: &Settings, marks: &Bookmarks, spaces: &Spaces, sync: &Extras) -> String {
    let a = &s.appearance;
    let theme = match a.theme {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    };
    let layout = match a.layout {
        Layout::Islands => "islands",
        Layout::Frame => "frame",
    };
    let tabs = match a.tabs {
        TabPosition::Vertical => "vertical",
        TabPosition::Horizontal => "horizontal",
    };

    let engines: String = ENGINES
        .iter()
        .map(|e| {
            let on = if e.id == s.search_engine { " on" } else { "" };
            format!(
                r#"<button type="button" class="engine{on}" data-k="engine" data-v="{}">{}<b>{}</b></button>"#,
                e.id,
                e.badge(),
                escape(e.name)
            )
        })
        .collect();

    let general = section(
        "general",
        &stacked(
            "Buscador",
            "Se usa en la barra de direcciones, en el Spotlight y en la página de nueva pestaña.",
            &format!(r#"<div class="engines" role="group">{engines}</div>"#),
        ),
    );

    let appearance = section(
        "appearance",
        &[
            stacked(
                "Tema",
                "",
                &tiles(
                    "theme",
                    theme,
                    &[
                        ("system", "Sistema", "t-system"),
                        ("light", "Claro", "t-light"),
                        ("dark", "Oscuro", "t-dark"),
                    ],
                ),
            ),
            row(
                "Color de acento",
                "Botones, selección y enlaces.",
                &format!(
                    r#"{}<input type="color" data-k="accent" value="{}" aria-label="Color personalizado">"#,
                    swatches("accent", SWATCHES, &a.accent),
                    escape(&a.accent)
                ),
            ),
            stacked(
                "Color del marco",
                "Lo que rodea a la web. «Tema» usa el del tema.",
                &format!(
                    r##"{}<input type="color" data-k="frame_color" value="{}" aria-label="Color personalizado"><button type="button" class="seg plain" data-k="frame_color" data-v="">Tema</button>"##,
                    swatches("frame_color", FRAME_SWATCHES, &a.frame_color),
                    escape(if a.frame_color.is_empty() {
                        "#e4e6eb"
                    } else {
                        &a.frame_color
                    })
                ),
            ),
            row(
                "Opacidad de la barra y el marco",
                "Con un compositor que lo permita, deja ver el escritorio.",
                &slider("opacity", a.opacity, 50, 100, " %"),
            ),
            row(
                "Redondeo de esquinas",
                "",
                &slider("radius", a.radius, 0, 28, " px"),
            ),
            row(
                "Separación",
                "Espacio entre los paneles y el borde de la ventana.",
                &slider("gap", a.gap, 0, 20, " px"),
            ),
            row(
                "",
                "",
                r#"<button type="button" class="seg plain" data-k="reset_appearance" data-v="1">Restablecer apariencia</button>"#,
            ),
        ]
        .concat(),
    );

    let layout_section = section(
        "layout",
        &[
            stacked(
                "Disposición",
                "Islas: barra y web son dos paneles. Marco: solo la web es un panel y la barra queda a su alrededor, como en Zen.",
                &tiles(
                    "layout",
                    layout,
                    &[
                        ("islands", "Islas", "l-islands"),
                        ("frame", "Marco", "l-frame"),
                    ],
                ),
            ),
            stacked(
                "Pestañas",
                "En la barra lateral o en una barra encima de la web.",
                &tiles(
                    "tabs",
                    tabs,
                    &[
                        ("vertical", "Verticales", "v-vertical"),
                        ("horizontal", "Horizontales", "v-horizontal"),
                    ],
                ),
            ),
            row(
                "Ancho de la barra lateral",
                &format!("Por defecto {SIDEBAR_WIDTH_DEFAULT} px; se recuerda."),
                &slider(
                    "sidebar_width",
                    a.sidebar_width,
                    SIDEBAR_WIDTH_MIN,
                    SIDEBAR_WIDTH_MAX,
                    " px",
                ),
            ),
            row(
                "Mostrar la barra al acercar el ratón",
                "Con la barra lateral suelta, un movimiento rápido hacia el borde izquierdo la muestra.",
                &switch("edge_reveal", s.edge_reveal),
            ),
        ]
        .concat(),
    );

    let lists: String = DEFAULT_LISTS
        .iter()
        .map(|l| {
            row(
                l.name,
                "",
                &switch(
                    &format!("list.{}", l.id),
                    s.list_enabled(l.id, l.enabled_by_default),
                ),
            )
        })
        .collect();
    let allowed = if s.allowlist.is_empty() {
        r#"<p class="empty">Ningún sitio. Se añaden desde el escudo de la barra.</p>"#.to_owned()
    } else {
        s.allowlist
            .iter()
            .map(|h| {
                format!(
                    r#"<div class="row"><div class="what"><b>{h}</b></div><div class="ctl"><button type="button" class="seg plain" data-k="allow.remove" data-v="{h}" data-remove="1">Quitar</button></div></div>"#,
                    h = escape(h)
                )
            })
            .collect()
    };
    let privacy = section(
        "privacy",
        &[
            row(
                "Bloqueador de anuncios y rastreadores",
                "Reglas de red, ocultado de elementos y scripts de uBlock Origin.",
                &switch("adblock", s.adblock_enabled),
            ),
            lists,
        ]
        .concat(),
    );
    let sites = section("sites", &allowed);
    let spaces_section = section(
        "spaces",
        &spaces
            .all()
            .iter()
            .map(|sp| {
                let remove = if spaces.all().len() > 1 {
                    format!(
                        r#"<button type="button" class="seg plain danger" data-k="space.remove" data-v="{}" data-remove="1">Eliminar</button>"#,
                        sp.id
                    )
                } else {
                    String::new()
                };
                let chosen = if icons::is_known(&sp.icon) {
                    sp.icon.as_str()
                } else {
                    icons::DEFAULT
                };
                format!(
                    r#"<div class="space"><div class="spacehead"><button type="button" class="iconbtn" data-open="1" aria-label="Elegir icono" title="Elegir icono"><span class="cur">{cur}</span>{chev}</button><input type="text" class="field wide" data-k="space.name.{id}" value="{name}" maxlength="24" aria-label="Nombre del espacio">{remove}</div>{grid}</div>"#,
                    cur = icons::web_svg(chosen, 22),
                    chev = icons::web_svg("chevron-right", 14),
                    id = sp.id,
                    name = escape(&sp.name),
                    grid = icon_grid(sp.id, chosen),
                )
            })
            .collect::<String>(),
    );
    let bookmarks_body = if marks.items().is_empty() {
        r#"<p class="empty">Ninguno todavía. Pulsa la estrella de la barra (Ctrl+D) para guardar una página.</p>"#
            .to_owned()
    } else {
        marks
            .items()
            .iter()
            .map(|b| {
                format!(
                    r#"<div class="row"><div class="what"><b>{}</b><p>{}</p></div><div class="ctl"><button type="button" class="seg plain" data-k="bookmark.remove" data-v="{}" data-remove="1">Quitar</button></div></div>"#,
                    escape(&b.title),
                    escape(&b.url),
                    escape(&b.url)
                )
            })
            .collect()
    };
    let bookmarks_section = section("bookmarks", &bookmarks_body);
    let sync_section = sync_section(s, sync);
    let archive_section = archive_section(s, sync);
    let security = section(
        "security",
        r#"<ul class="facts">
<li>Las descargas de programas y scripts se bloquean, y los nombres de archivo se limpian.</li>
<li><code>javascript:</code>, <code>data:</code> y <code>blob:</code> no se abren desde la barra de direcciones.</li>
<li>Los errores de certificado nunca se saltan.</li>
<li>Cámara, micrófono, ubicación y notificaciones se deniegan.</li>
<li>Sin ventanas emergentes automáticas; una web no puede abrir archivos locales.</li>
<li>Prevención de rastreo inteligente (ITP) y aislamiento por procesos de WebKit.</li>
</ul>"#,
    );

    let about = section(
        "about",
        &row(
            &format!("Brújula {}", env!("CARGO_PKG_VERSION")),
            "El navegador de Nodalix. WebKit, bloqueador integrado, pestañas verticales y el lenguaje de diseño NDL.",
            "",
        ),
    );

    let nav: String = PAGES
        .iter()
        .map(|(id, label, _, _, icon)| {
            format!(
                r##"<a href="#{id}" data-nav="{id}">{}<span>{}</span></a>"##,
                icons::web_svg(icon, 18),
                escape(label)
            )
        })
        .collect();

    format!(
        r##"<!doctype html>
<html lang="es" data-theme="{theme}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Preferencias</title>
<style>{css}</style></head>
<body style="--accent:{accent};--on-accent:{on_accent}">
<div class="shell">
<nav class="nav" aria-label="Secciones"><h1>Preferencias</h1>{nav}</nav>
<main>{general}{appearance}{layout_section}{spaces_section}{bookmarks_section}{archive_section}{sync_section}{privacy}{sites}{security}{about}</main>
</div>
<script>{js}</script>
</body></html>
"##,
        css = CSS,
        js = JS,
        accent = escape(&a.accent),
        on_accent = crate::settings::text_on(&a.accent),
    )
}

/// Page used by the Spotlight to offer settings: `(title, keywords, section id)`.
pub const SECTIONS: &[(&str, &str, &str)] = &[
    ("Buscador", "motor google duckduckgo bing buscar", "general"),
    ("Tema", "claro oscuro modo sistema", "appearance"),
    ("Color de acento", "color tinte acento", "appearance"),
    ("Color del marco", "marco fondo color", "appearance"),
    (
        "Transparencia",
        "opacidad transparente cristal",
        "appearance",
    ),
    (
        "Redondeo de esquinas",
        "bordes radio esquinas",
        "appearance",
    ),
    ("Disposición", "islas marco zen diseño layout", "layout"),
    (
        "Pestañas horizontales",
        "pestañas arriba horizontal vertical",
        "layout",
    ),
    (
        "Ancho de la barra lateral",
        "sidebar tamaño ancho",
        "layout",
    ),
    (
        "Espacios",
        "espacio espacios grupos perfiles workspaces iconos",
        "spaces",
    ),
    (
        "Marcadores",
        "favoritos marcadores guardados estrella",
        "bookmarks",
    ),
    (
        "Archivo de pestañas",
        "archivar archivo pestañas antiguas cerradas recuperar",
        "archive",
    ),
    (
        "Sincronización",
        "sincronizar nube icloud onedrive google drive dropbox dispositivos",
        "sync",
    ),
    (
        "Bloqueador de anuncios",
        "adblock anuncios rastreadores listas",
        "privacy",
    ),
    ("Sitios sin bloqueador", "excepciones permitidos", "sites"),
    ("Seguridad", "descargas permisos certificados", "security"),
];

/// The heading of a section, as the page shows it.
pub fn section_title(id: &str) -> &'static str {
    PAGES
        .iter()
        .find(|p| p.0 == id)
        .map_or("Preferencias", |p| p.1)
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f3f4f6;--fg:#12161a;--muted:#5d6872;--card:#fff;--line:#e4e7ec;--chip:#eef0f4;--mf:#dfe3e8;--sb:#f9fafb;--pg:#fff;--line2:#d3d8de;--shadow:0 1px 2px rgba(16,24,40,.05),0 8px 24px -12px rgba(16,24,40,.08)}
@media (prefers-color-scheme:dark){:root{--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--mf:#1a1f23;--sb:#252b30;--pg:#2f363c;--line2:#454d54;--shadow:0 1px 2px rgba(0,0,0,.3)}}
:root[data-theme=light]{color-scheme:light;--bg:#f3f4f6;--fg:#12161a;--muted:#5d6872;--card:#fff;--line:#e4e7ec;--chip:#eef0f4;--mf:#dfe3e8;--sb:#f9fafb;--pg:#fff;--line2:#d3d8de;--shadow:0 1px 2px rgba(16,24,40,.05),0 8px 24px -12px rgba(16,24,40,.08)}
:root[data-theme=dark]{color-scheme:dark;--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--mf:#1a1f23;--sb:#252b30;--pg:#2f363c;--line2:#454d54;--shadow:0 1px 2px rgba(0,0,0,.3)}
*{box-sizing:border-box}
html{scroll-behavior:smooth}
body{margin:0;font-family:"Adwaita Sans",Inter,system-ui,sans-serif;background:var(--bg);color:var(--fg);line-height:1.45;-webkit-font-smoothing:antialiased}
.shell{display:grid;grid-template-columns:14.5rem minmax(0,44rem);justify-content:center;gap:3.5rem;padding:3.25rem 2rem 8rem}
.nav{position:sticky;top:3rem;align-self:start;display:flex;flex-direction:column;gap:.15rem}
.nav h1{font-size:1.65rem;font-weight:700;letter-spacing:-.03em;margin:0 0 1.1rem .8rem}
.nav a{display:flex;align-items:center;gap:.75rem;color:var(--muted);text-decoration:none;padding:.55rem .8rem;border-radius:.75rem;font-size:.95rem;transition:background .15s,color .15s}
.nav a:hover{background:var(--chip);color:var(--fg)}
.nav a.on{background:var(--card);color:var(--fg);box-shadow:var(--shadow);font-weight:560}
.nav a.on .ico{color:var(--accent)}
.ico{flex:none}
section{margin-bottom:3rem;scroll-margin-top:2.5rem}
.shead{display:flex;gap:.9rem;align-items:center;margin:0 .25rem 1rem}
.sicon{display:grid;place-items:center;flex:none;width:2.5rem;height:2.5rem;border-radius:.85rem;background:color-mix(in srgb,var(--accent) 14%,transparent);color:var(--accent)}
h2{font-size:1.4rem;font-weight:660;letter-spacing:-.02em;margin:0;line-height:1.2}
.shead p{margin:.15rem 0 0;color:var(--muted);font-size:.9rem}
.card{background:var(--card);border:1px solid var(--line);border-radius:1.15rem;box-shadow:var(--shadow)}
.row{display:flex;gap:1.5rem;justify-content:space-between;align-items:center;padding:1rem 1.25rem;border-top:1px solid var(--line)}
.row:first-child{border-top:0}
.row.stack{flex-direction:column;align-items:stretch;gap:.9rem}
.what{min-width:0}.what b{font-weight:580}.what p{margin:.15rem 0 0;color:var(--muted);font-size:.86rem;max-width:32rem}
.ctl{display:flex;gap:.6rem;align-items:center;flex-wrap:wrap;justify-content:flex-end}
.stack .ctl{justify-content:flex-start;gap:.6rem .6rem}
.segs{display:flex;gap:.2rem;background:var(--chip);padding:.2rem;border-radius:.85rem}
.seg{white-space:nowrap;font:inherit;font-size:.9rem;color:var(--fg);background:transparent;border:0;border-radius:.65rem;padding:.42rem .9rem;cursor:pointer}
.seg:hover{background:color-mix(in srgb,var(--fg) 8%,transparent)}
.seg.on{background:var(--accent);color:var(--on-accent)}
.seg.plain{background:var(--chip)}.seg.plain:hover{background:color-mix(in srgb,var(--fg) 12%,var(--chip))}
.seg.danger{color:#d33}
.tiles{display:grid;grid-template-columns:repeat(auto-fill,minmax(8.5rem,1fr));gap:.75rem;width:100%}
.tile{font:inherit;color:var(--fg);background:transparent;border:1px solid var(--line);border-radius:.95rem;padding:.55rem .55rem .7rem;cursor:pointer;display:flex;flex-direction:column;gap:.55rem;align-items:center;transition:border-color .15s,box-shadow .15s}
.tile:hover{border-color:color-mix(in srgb,var(--fg) 30%,var(--line))}
.tile.on{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent);background:color-mix(in srgb,var(--accent) 7%,transparent)}
.tile b{font-weight:560;font-size:.9rem}
.prev{position:relative;display:block;width:100%}
.mini{display:block;position:relative;width:100%;aspect-ratio:16/10;border-radius:.55rem;overflow:hidden;background:var(--mf);box-shadow:inset 0 0 0 1px rgba(127,127,127,.18)}
.mini.over{position:absolute;inset:0;clip-path:polygon(58% 0,100% 0,100% 100%,42% 100%)}
.mini i{position:absolute;display:block}
.mini .sb{left:5%;top:8%;bottom:8%;width:27%;border-radius:.3rem;background:var(--sb)}
.mini .sb::before{content:"";position:absolute;left:16%;right:16%;top:14%;height:7%;border-radius:2px;background:var(--line2);box-shadow:0 .5rem 0 var(--line2),0 1rem 0 var(--line2)}
.mini .pg{left:36%;right:5%;top:8%;bottom:8%;border-radius:.3rem;background:var(--pg)}
.t-light{--mf:#dfe3e8;--sb:#f9fafb;--pg:#fff;--line2:#d3d8de}
.t-dark{--mf:#1a1f23;--sb:#252b30;--pg:#2f363c;--line2:#454d54}
.l-frame .sb{background:transparent}
.l-frame{background:var(--mf)}
.l-frame .pg{left:34%;right:4%;top:5%;bottom:5%}
.v-horizontal .sb{left:5%;right:5%;top:8%;bottom:auto;width:auto;height:18%}
.v-horizontal .sb::before{left:5%;right:auto;top:32%;width:20%;height:36%;box-shadow:.95rem 0 0 var(--line2),1.9rem 0 0 var(--line2)}
.v-horizontal .pg{left:5%;right:5%;top:30%;bottom:8%}
.switch{position:relative;width:2.75rem;height:1.6rem;border-radius:999px;border:0;background:color-mix(in srgb,var(--fg) 22%,transparent);cursor:pointer;padding:0;transition:background .15s}
.switch i{position:absolute;top:.2rem;left:.2rem;width:1.2rem;height:1.2rem;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,.3);transition:transform .15s}
.switch.on{background:var(--accent)}.switch.on i{transform:translateX(1.15rem)}
.dots{display:flex;gap:.5rem;flex-wrap:wrap}
.dot{width:1.75rem;height:1.75rem;border-radius:50%;border:0;cursor:pointer;box-shadow:inset 0 0 0 1px rgba(0,0,0,.18);transition:transform .12s}
.dot:hover{transform:scale(1.1)}
.dot.on{box-shadow:0 0 0 2px var(--card),0 0 0 4px var(--fg)}
input[type=color]{width:1.9rem;height:1.9rem;padding:0;border:0;background:none;cursor:pointer;border-radius:50%}
input[type=color]::-webkit-color-swatch-wrapper{padding:0}
input[type=color]::-webkit-color-swatch{border:0;border-radius:50%;box-shadow:inset 0 0 0 1px rgba(0,0,0,.18)}
.slider{display:flex;gap:.9rem;align-items:center}
input[type=range]{-webkit-appearance:none;appearance:none;width:13rem;height:.4rem;border-radius:999px;background:linear-gradient(to right,var(--accent) var(--p,50%),color-mix(in srgb,var(--fg) 14%,transparent) var(--p,50%));outline:none}
input[type=range]::-webkit-slider-thumb{-webkit-appearance:none;width:1.15rem;height:1.15rem;border-radius:50%;background:#fff;border:0;box-shadow:0 1px 4px rgba(0,0,0,.35)}
output{min-width:3.6rem;text-align:right;color:var(--muted);font-variant-numeric:tabular-nums;font-size:.9rem}
.engines{display:grid;grid-template-columns:repeat(auto-fill,minmax(10rem,1fr));gap:.6rem;width:100%}
.engine{display:flex;align-items:center;gap:.7rem;padding:.65rem .8rem;border:1px solid var(--line);border-radius:.9rem;background:transparent;font:inherit;color:var(--fg);cursor:pointer;text-align:left;transition:border-color .15s,box-shadow .15s}
.engine:hover{border-color:color-mix(in srgb,var(--fg) 30%,var(--line))}
.engine.on{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent);background:color-mix(in srgb,var(--accent) 7%,transparent)}
.engine b{font-weight:560;font-size:.92rem}
.acct{display:flex;flex-direction:column;min-width:0}.acct small{color:var(--muted);font-size:.78rem}
.status{color:var(--muted);font-size:.9rem;margin-right:.8rem;text-align:right;max-width:24rem}
.note{margin:0;padding:1rem 1.25rem;border-top:1px solid var(--line);color:var(--muted);font-size:.85rem}
.eng{display:grid;place-items:center;flex:none;width:1.9rem;height:1.9rem;border-radius:.6rem;color:#fff;font-weight:750;font-size:1rem;font-family:system-ui,sans-serif}
.space{padding:.85rem 1.25rem;border-top:1px solid var(--line)}
.space:first-child{border-top:0}
.spacehead{display:flex;gap:.7rem;align-items:center}
.iconbtn{display:flex;align-items:center;gap:.35rem;font:inherit;color:var(--accent);background:color-mix(in srgb,var(--accent) 12%,transparent);border:0;border-radius:.8rem;padding:.55rem .6rem .55rem .7rem;cursor:pointer}
.iconbtn:hover{background:color-mix(in srgb,var(--accent) 20%,transparent)}
.iconbtn .cur{display:grid;place-items:center}
.iconbtn>.ico{opacity:.7;transition:transform .15s}
.space.open .iconbtn>.ico{transform:rotate(90deg)}
.iconsgrid{display:none;grid-template-columns:repeat(auto-fill,minmax(2.6rem,1fr));gap:.35rem;margin-top:.8rem;padding:.6rem;background:var(--chip);border-radius:.9rem}
.space.open .iconsgrid{display:grid}
.ibtn{display:grid;place-items:center;height:2.6rem;color:var(--fg);background:transparent;border:0;border-radius:.65rem;cursor:pointer}
.ibtn:hover{background:color-mix(in srgb,var(--fg) 10%,transparent)}
.ibtn.on{background:var(--accent);color:var(--on-accent)}
input.field{font:inherit;background:var(--chip);color:var(--fg);border:1px solid transparent;border-radius:.7rem;padding:.5rem .75rem;width:4rem;transition:border-color .15s}
input.field.wide{flex:1;min-width:0}
input.field:focus{border-color:var(--accent);outline:none}
.facts{margin:0;padding:1.1rem 1.25rem 1.1rem 2.5rem;color:var(--muted);font-size:.92rem}.facts li{margin:.35rem 0}
code{background:var(--chip);padding:.05rem .35rem;border-radius:.3rem;font-size:.85em}
.empty{margin:0;padding:1.1rem 1.25rem;color:var(--muted)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
@media (max-width:820px){.shell{grid-template-columns:1fr;gap:1.5rem}.nav{position:static;flex-direction:row;flex-wrap:wrap}.nav h1{width:100%}.nav a span{display:none}}
"#;

const JS: &str = r##"
const send=(k,v)=>{location.href='brujula://set?'+encodeURIComponent(k)+'='+encodeURIComponent(v)};
let timer;
const later=(f)=>{clearTimeout(timer);timer=setTimeout(f,90)};
const unit=(el)=>el.parentElement.querySelector('output');
const fill=(i)=>i.style.setProperty('--p',((i.value-i.min)*100/(i.max-i.min||1))+'%');
document.addEventListener('click',e=>{
  const nav=e.target.closest('a[href^="#"]');
  if(nav){e.preventDefault();const t=document.querySelector(nav.getAttribute('href'));if(t)t.scrollIntoView({behavior:'smooth',block:'start'});return}
  const open=e.target.closest('[data-open]');
  if(open){open.closest('.space').classList.toggle('open');return}
  const b=e.target.closest('[data-k]');if(!b||b.tagName==='INPUT')return;
  const k=b.dataset.k;
  if(b.dataset.toggle){const on=!b.classList.contains('on');b.classList.toggle('on',on);b.setAttribute('aria-checked',on);send(k,on?'1':'0');return}
  const v=b.dataset.v;
  if(b.dataset.remove){const r=b.closest('.row,.space');if(r)r.remove();send(k,v);return}
  send(k,v);
  if(k==='accent'){document.body.style.setProperty('--accent',v)}
  const group=b.closest('.segs,.dots,.tiles,.engines,.iconsgrid');
  if(group){group.querySelectorAll('.on').forEach(x=>x.classList.remove('on'));b.classList.add('on')}
  if(b.dataset.pick){const s=b.closest('.space');const cur=s.querySelector('.iconbtn .cur');if(cur){cur.innerHTML=b.innerHTML;const g=cur.firstElementChild;if(g){g.setAttribute('width',22);g.setAttribute('height',22)}}s.classList.remove('open')}
  if(k==='theme'){document.documentElement.dataset.theme=v}
  if(k==='reset_appearance'){setTimeout(()=>location.reload(),150)}
});
document.addEventListener('change',e=>{
  const i=e.target;if(i.type==='text'&&i.dataset.k)send(i.dataset.k,i.value)
});
document.addEventListener('input',e=>{
  const i=e.target;if(!i.dataset||!i.dataset.k)return;
  if(i.type==='range'){fill(i);const o=unit(i);if(o)o.textContent=i.value+o.textContent.replace(/^[0-9]+/,'');later(()=>send(i.dataset.k,i.value))}
  else if(i.type==='color'){if(i.dataset.k==='accent')document.body.style.setProperty('--accent',i.value);later(()=>send(i.dataset.k,i.value))}
});
const links=[...document.querySelectorAll('[data-nav]')];
const mark=(id)=>links.forEach(a=>a.classList.toggle('on',a.dataset.nav===id));
if('IntersectionObserver' in window){
  const seen=new Map();
  const io=new IntersectionObserver(es=>{es.forEach(x=>seen.set(x.target.id,x.isIntersecting));
    const first=[...document.querySelectorAll('main section')].find(s=>seen.get(s.id));if(first)mark(first.id)},{rootMargin:'-20% 0px -60% 0px'});
  document.querySelectorAll('main section').forEach(s=>io.observe(s));
}
mark('general');
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_read_from_the_address() {
        assert_eq!(
            parse_change("brujula://set?theme=dark"),
            Some(("theme".into(), "dark".into()))
        );
        assert_eq!(
            parse_change("brujula://set?allow.remove=a%2Eb.com"),
            Some(("allow.remove".into(), "a.b.com".into()))
        );
        assert_eq!(parse_change("brujula://newtab"), None);
        assert_eq!(parse_change("https://set?theme=dark"), None);
        assert_eq!(parse_change("brujula://set?a=1&b=2"), None);
        assert_eq!(parse_change("brujula://set?"), None);
    }

    #[test]
    fn appearance_changes_are_applied_and_clamped() {
        let mut s = Settings::default();
        assert!(s.apply("theme", "dark").unwrap().appearance);
        assert_eq!(s.appearance.theme, Theme::Dark);
        s.apply("opacity", "5").unwrap();
        assert_eq!(s.appearance.opacity, 50);
        s.apply("sidebar_width", "9999").unwrap();
        assert_eq!(s.appearance.sidebar_width, SIDEBAR_WIDTH_MAX);
        let e = s.apply("layout", "frame").unwrap();
        assert!(e.layout && e.appearance);
        assert_eq!(s.appearance.layout, Layout::Frame);
        s.apply("tabs", "horizontal").unwrap();
        assert_eq!(s.appearance.tabs, TabPosition::Horizontal);
        s.apply("reset_appearance", "1").unwrap();
        assert_eq!(s.appearance, crate::settings::Appearance::default());
    }

    #[test]
    fn bad_values_and_unknown_keys_are_refused() {
        let mut s = Settings::default();
        let before = s.clone();
        for (k, v) in [
            ("theme", "purple"),
            ("accent", "red"),
            ("accent", "#12345"),
            ("frame_color", "url(x)"),
            ("opacity", "lots"),
            ("edge_reveal", "maybe"),
            ("engine", "nope"),
            ("list.nope", "1"),
            ("homepage", "javascript:alert(1)"),
            ("", ""),
        ] {
            assert!(s.apply(k, v).is_err(), "{k}={v}");
        }
        assert_eq!(s, before);
    }

    #[test]
    fn blocker_switches() {
        let mut s = Settings::default();
        assert!(s.apply("adblock", "0").unwrap().blocker);
        assert!(!s.adblock_enabled);
        s.apply("list.easylist", "0").unwrap();
        assert!(s.disabled_lists.contains(&"easylist".to_owned()));
        s.apply("list.easylist", "1").unwrap();
        assert!(s.disabled_lists.is_empty());
        s.set_site_allowed("a.com", true);
        s.apply("allow.remove", "a.com").unwrap();
        assert!(s.allowlist.is_empty());
    }

    #[test]
    fn sync_settings_are_applied_and_checked() {
        let mut s = Settings::default();
        assert!(s.apply("sync", "1").unwrap().sync);
        assert!(s.sync.enabled);
        s.apply("sync_account", "abc-1_x").unwrap();
        assert_eq!(s.sync.account, "abc-1_x");
        s.apply("sync_tabs", "0").unwrap();
        assert!(!s.sync.tabs);
        assert!(s.apply("sync_now", "1").unwrap().sync_now);
        let before = s.clone();
        for (k, v) in [
            ("sync_account", "../etc"),
            ("sync_account", "a b"),
            ("sync", "maybe"),
            ("sync_bookmarks", "2"),
        ] {
            assert!(s.apply(k, v).is_err(), "{k}={v}");
        }
        assert_eq!(s, before);
        s.apply("sync_account", "").unwrap();
        assert!(s.sync.account.is_empty());
    }

    #[test]
    fn the_sync_section_lists_accounts_and_escapes_them() {
        let mut s = Settings::default();
        s.sync.account = "a1".into();
        let view = Extras {
            accounts: vec![
                ("a1".into(), "Casa · Dropbox".into(), true),
                ("b2".into(), "<b>x</b>".into(), false),
            ],
            status: "Hecho".into(),
            ..Extras::default()
        };
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &view);
        assert!(html.contains(r#"class="engine on" data-k="sync_account" data-v="a1""#));
        assert!(html.contains("Conectada") && html.contains("Sin conectar"));
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;") && !html.contains("<b>x</b>"));
        assert!(html.contains(r#"id="sync-status""#) && html.contains("Hecho"));
        let empty = render(
            &s,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(empty.contains("Ninguna cuenta"));
    }

    #[test]
    fn archive_choices_are_checked_and_the_list_is_escaped() {
        let mut s = Settings::default();
        assert_eq!(s.archive_after_hours, 24);
        s.apply("archive_after", "168").unwrap();
        assert_eq!(s.archive_after_hours, 168);
        s.apply("archive_after", "0").unwrap();
        assert!(s.apply("archive_after", "5").is_err());
        assert!(s.apply("archive_after", "x").is_err());
        assert_eq!(s.archive_after_hours, 0);
        let view = Extras {
            archived: vec![("https://a.example/?q=\"1\"".into(), "<i>x</i>".into())],
            ..Extras::default()
        };
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &view);
        assert!(html.contains(r#"class="seg on" data-k="archive_after" data-v="0""#));
        assert!(html.contains("&lt;i&gt;x&lt;/i&gt;") && !html.contains("<i>x</i>"));
        assert!(html.contains(r#"data-k="archive.open""#));
        assert!(!html.contains("q=\"1\""));
    }

    #[test]
    fn engine_is_stored() {
        let mut s = Settings::default();
        s.apply("engine", "bing").unwrap();
        assert_eq!(s.search_engine, "bing");
        assert!(s.search_engine_chosen);
    }

    #[test]
    fn the_page_reflects_the_settings_and_escapes_sites() {
        let mut s = Settings::default();
        s.appearance.theme = Theme::Dark;
        s.allowlist = vec!["<script>x</script>".into()];
        let html = render(
            &s,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(html.contains(r#"class="tile on" data-k="theme" data-v="dark""#));
        assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("--accent:#0c9a58"));
    }

    #[test]
    fn spaces_and_bookmarks_are_listed_and_escaped() {
        let mut spaces = Spaces::default();
        spaces.add("<b>x</b>");
        let mut marks = Bookmarks::default();
        marks.toggle("https://a.com/?q=\"1\"", "<i>t</i>");
        let html = render(&Settings::default(), &marks, &spaces, &Extras::default());
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"));
        assert!(html.contains("&lt;i&gt;t&lt;/i&gt;"));
        assert!(!html.contains("<i>t</i>"));
        assert!(html.contains(r#"data-k="space.remove""#));
        assert!(html.contains(r#"data-k="bookmark.remove""#));
    }

    #[test]
    fn every_space_has_an_icon_picker_with_our_icons_only() {
        let mut spaces = Spaces::default();
        let id = spaces.add("Clase").unwrap();
        spaces.rename(id, "Clase", "backpack");
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &spaces,
            &Extras::default(),
        );
        assert!(html.contains(&format!(r#"data-k="space.icon.{id}" data-v="backpack""#)));
        // The chosen one is marked, and each icon is drawn inline (nothing is fetched).
        assert!(html.contains(r#"class="ibtn on" data-k="space.icon."#));
        assert!(html.contains(r#"data-v="user""#));
        assert!(!html.contains("<img"));
    }

    #[test]
    fn each_search_engine_has_its_badge_on_the_page() {
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        for e in ENGINES {
            assert!(html.contains(&e.badge()), "{}", e.id);
        }
    }

    #[test]
    fn the_nav_links_to_every_section_and_the_page_scrolls_to_them() {
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        for (id, ..) in PAGES {
            assert!(html.contains(&format!(r##"href="#{id}""##)), "{id}");
            assert!(html.contains(&format!(r#"<section id="{id}""#)), "{id}");
        }
        // Fragments are not navigations the browser accepts, so the page scrolls itself.
        assert!(JS.contains("scrollIntoView"));
    }

    #[test]
    fn the_sidebar_slider_covers_the_range_the_browser_accepts() {
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(html.contains(&format!(
            r#"data-k="sidebar_width" min="{SIDEBAR_WIDTH_MIN}" max="{SIDEBAR_WIDTH_MAX}""#
        )));
    }

    #[test]
    fn every_section_the_spotlight_offers_exists_on_the_page() {
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        for (_, _, id) in SECTIONS {
            assert!(html.contains(&format!(r#"id="{id}""#)), "{id}");
        }
    }
}
