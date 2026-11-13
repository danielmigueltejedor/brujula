//! The preferences page (`brujula://settings`): the HTML, and how a change made on it is
//! checked and stored.
//!
//! The page changes settings by navigating to `brujula://set?key=value`, which the browser
//! intercepts (and only honours when it comes from this page). Every value is validated in
//! [`Settings::apply`], so the page, a hand-typed address or a crafted link can only ever set
//! something inside the allowed range.

use std::sync::LazyLock;

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
    /// The user asked to pick (or drop) the folder to sync through.
    pub folder: Option<FolderChoice>,
}

/// How the user chose the folder on the page. The page never says a path: it names one of the
/// folders the browser offered, or asks for the system's folder dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderChoice {
    Dialog,
    Offered(usize),
    Forget,
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
            "frame_gradient" => {
                // A preset or a custom one: two to four colours, `#rrggbb` separated by commas.
                // Empty goes back to the plain colour.
                a.frame_gradient = if value.is_empty() {
                    Vec::new()
                } else {
                    let stops: Vec<String> = value
                        .split(',')
                        .map(|c| parse_hex_color(c).ok_or("not a colour"))
                        .collect::<Result<_, _>>()?;
                    if !(2..=crate::settings::MAX_STOPS).contains(&stops.len()) {
                        return Err("a gradient has two to four colours".to_owned());
                    }
                    stops
                };
                effect.appearance = true;
            }
            "frame_stop1" | "frame_stop2" | "frame_stop3" => {
                // One colour of the gradient; the first two always exist, the third can go.
                let at = usize::from(key.as_bytes()[10] - b'1');
                if a.frame_gradient.len() < 2 {
                    let first = if a.frame_color.is_empty() {
                        "#dbe7e0".to_owned()
                    } else {
                        a.frame_color.clone()
                    };
                    a.frame_gradient = vec![first, "#dbe4f3".to_owned()];
                }
                if value.is_empty() && at == 2 {
                    a.frame_gradient.truncate(2);
                } else {
                    let colour = parse_hex_color(value).ok_or("not a colour")?;
                    if at < a.frame_gradient.len() {
                        a.frame_gradient[at] = colour;
                    } else {
                        a.frame_gradient.push(colour);
                    }
                }
                effect.appearance = true;
            }
            "frame_angle" => {
                let degrees = number(value)?;
                if degrees >= 360 || degrees % 45 != 0 {
                    return Err(format!("not an angle: {degrees}"));
                }
                a.frame_angle = degrees;
                effect.appearance = true;
            }
            "texture" => {
                a.texture = crate::settings::Texture::from_id(value)
                    .ok_or_else(|| format!("unknown texture: {value}"))?;
                effect.appearance = true;
            }
            "texture_amount" => {
                a.texture_amount = number(value)?.clamp(10, 100);
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
            "https_first" => {
                self.https_first = flag(value)?;
            }
            "history_enabled" => {
                self.history_enabled = flag(value)?;
            }
            "update_check" => {
                self.update_check = flag(value)?;
            }
            "passwords_enabled" => {
                self.passwords_enabled = flag(value)?;
            }
            "passkeys_enabled" => {
                self.passkeys_enabled = flag(value)?;
            }
            "sleep_after" => {
                let minutes = number(value)?;
                if !crate::settings::SLEEP_CHOICES.contains(&minutes) {
                    return Err(format!("not a choice: {minutes}"));
                }
                self.sleep_after_minutes = minutes;
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
                if !value.is_empty() {
                    self.sync.folder.clear();
                }
                effect.sync = true;
            }
            "sync_folder" => {
                effect.folder = Some(match value {
                    "dialog" => FolderChoice::Dialog,
                    "forget" => {
                        self.sync.folder.clear();
                        effect.sync = true;
                        FolderChoice::Forget
                    }
                    n if n.len() <= 2 => {
                        FolderChoice::Offered(n.parse().map_err(|_| format!("not a folder: {n}"))?)
                    }
                    _ => return Err(format!("not a folder: {value}")),
                });
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

/// Gradients for the frame: label and colours.
const FRAME_GRADIENTS: &[(&str, &[&str])] = &[
    ("Aurora", &["#c9f0e3", "#d7e0fb", "#f1d9f5"]),
    ("Atardecer", &["#fde2c8", "#f9c5d1", "#d9c4f5"]),
    ("Océano", &["#cfe8f7", "#c6d4f7", "#d9d0f5"]),
    ("Bosque", &["#d4ead8", "#cfe6d0", "#e8efc9"]),
    ("Lavanda", &["#e6dcf6", "#f1d9ec", "#dbe4f7"]),
    ("Nube", &["#eef0f4", "#dfe4ec", "#f4eef0"]),
    ("Medianoche", &["#1b2540", "#2b2250", "#16303a"]),
    ("Brasas", &["#3a1d2a", "#4a2a1c", "#2a1a3a"]),
    ("Esmeralda", &["#0f2f2a", "#14423a", "#1d2f4a"]),
    ("Neón", &["#6a5cff", "#00c2a8", "#ff5ca8"]),
];

/// The gradient picker: presets, the colours one by one, the direction and the way back.
fn gradient_picker(a: &crate::settings::Appearance) -> String {
    let current = a.frame_gradient.join(",");
    let presets: String = FRAME_GRADIENTS
        .iter()
        .map(|(label, stops)| {
            let value = stops.join(",");
            let on = if value == current { " on" } else { "" };
            format!(
                r##"<button type="button" class="dot grad{on}" style="background:linear-gradient(135deg,{value})" data-k="frame_gradient" data-v="{value}" data-redraw="1" aria-label="{label}" title="{label}"></button>"##
            )
        })
        .collect();
    let base = if a.frame_gradient.len() >= 2 {
        a.frame_gradient.clone()
    } else {
        vec![
            if a.frame_color.is_empty() {
                "#dbe7e0".to_owned()
            } else {
                a.frame_color.clone()
            },
            "#dbe4f3".to_owned(),
        ]
    };
    let stop = |i: usize| {
        base.get(i).map_or_else(String::new, |c| {
            format!(
                r#"<input type="color" data-k="frame_stop{n}" value="{c}" aria-label="Color {n} del degradado">"#,
                n = i + 1
            )
        })
    };
    let third = if base.len() > 2 {
        format!(
            r#"{}<button type="button" class="seg plain" data-k="frame_stop3" data-v="" data-redraw="1">Quitar el tercero</button>"#,
            stop(2)
        )
    } else {
        r##"<button type="button" class="seg plain" data-k="frame_stop3" data-v="#f1d9f5" data-redraw="1">Añadir un tercer color</button>"##.to_owned()
    };
    let angles: String = [
        (0, "↑"),
        (45, "↗"),
        (90, "→"),
        (135, "↘"),
        (180, "↓"),
        (225, "↙"),
        (270, "←"),
        (315, "↖"),
    ]
    .iter()
    .map(|(deg, arrow)| {
        let on = if *deg == a.frame_angle { " on" } else { "" };
        format!(r#"<button type="button" class="seg{on}" data-k="frame_angle" data-v="{deg}">{arrow}</button>"#)
    })
    .collect();
    format!(
        r#"<div class="dots grads">{presets}</div><div class="actions">{}{}{third}<button type="button" class="seg plain" data-k="frame_gradient" data-v="" data-redraw="1">Sin degradado</button></div><div class="actions"><div class="segs" role="group">{angles}</div></div>"#,
        stop(0),
        stop(1),
    )
}

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
    /// Folders of file-sync clients found in the home directory, offered as places to sync.
    pub folders: Vec<String>,
    /// How the last round went, in words.
    pub status: String,
    /// Archived tabs: address and title.
    pub archived: Vec<(String, String)>,
    /// Boosts: site, dark mode, CSS.
    pub boosts: Vec<(String, bool, String)>,
    /// The Chromium extensions that are installed.
    pub extensions: Vec<ExtItem>,
    /// The pages visited last: address and title, newest first.
    pub history: Vec<(String, String)>,
    /// How many pages the history holds in all.
    pub history_total: usize,
    /// The password vault: whether it exists, whether it is open, and what it holds.
    pub vault: VaultView,
    /// The browsers found on this computer to import from: name, and whether their passwords
    /// can be read (Firefox's only come in an exported file).
    pub import: Vec<(String, bool)>,
    /// The web apps: id, name and address.
    pub webapps: Vec<(String, String, String)>,
    /// The essentials: id, name and address.
    pub essentials: Vec<(u32, String, String)>,
    /// The profiles: id, name, avatar and whether a password locks it. Empty until a second one
    /// is made or the welcome page made the first.
    pub profiles: Vec<(String, String, String, bool)>,
    /// The id of the profile this window runs as.
    pub profile: String,
    /// What the update check found, in words (empty before the first check).
    pub update_status: String,
    /// The newer version that is waiting, if any.
    pub update_version: String,
    /// The AI assistant: what the user chose and whether the keyring holds a key.
    pub ai: AiView,
}

/// What the preferences page shows of the AI assistant. Never a key.
#[derive(Debug, Default, Clone)]
pub struct AiView {
    pub settings: crate::ai::AiSettings,
    /// The keyring has a key for the provider chosen.
    pub key_set: bool,
    /// How the last connection test went, in words.
    pub status: String,
}

/// What the preferences page shows of the password vault. Never a password.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VaultView {
    pub exists: bool,
    pub open: bool,
    /// The saved logins: id, site and user name.
    pub logins: Vec<(u64, String, String)>,
    /// The ids of the logins that have a secret for one-time codes (2FA).
    pub totp: Vec<u64>,
    /// The sites the user asked never to be offered a save for.
    pub never: Vec<String>,
}

/// An installed extension, as the page lists it.
#[derive(Debug, Default, Clone)]
pub struct ExtItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    /// Whether it has an options page to open.
    pub options: bool,
    /// What it can do, in words.
    pub permissions: Vec<String>,
    /// It is on but could not be read.
    pub problem: bool,
}

/// The sections of the page, in order: id, nav label, heading, one line under the heading, icon.
/// The sections of the page, one per line: id, name in the list, title, hint and icon, separated
/// by `|`.
const PAGES_TABLE: &str = r#"general|General|General|El buscador que usa Brújula en todas partes.|settings
appearance|Apariencia|Apariencia|Tema, colores y formas. Los cambios se ven al momento.|palette
layout|Diseño|Diseño|Cómo se reparten la barra lateral, las pestañas y la web.|layout
spaces|Espacios|Espacios|Grupos de pestañas, cada uno con su icono. Se crean desde el selector de la barra lateral.|workspace
profiles|Perfiles|Perfiles|Una persona por perfil, con sus propios marcadores, pestañas, contraseñas y ajustes. Se elige al abrir el navegador.|user
essentials|Esenciales|Esenciales|Los sitios que abres a todas horas, como iconos en la barra lateral y en todos los espacios. Un clic los abre o vuelve a su pestaña.|pin
bookmarks|Marcadores|Marcadores|Las páginas que guardaste con la estrella.|bookmark
history|Historial|Historial|Las páginas que visitas, guardadas solo en este equipo. Una ventana privada no deja rastro.|history
passwords|Contraseñas|Contraseñas|Se guardan cifradas con una contraseña maestra, solo en este equipo. No se sincronizan ni se usan en ventanas privadas.|lock
import|Importar|Importar de otro navegador|Trae tus marcadores, historial, cookies y contraseñas de otro navegador de este equipo, o de un archivo. Lo que ya tienes no se borra.|archive
webapps|Aplicaciones web|Aplicaciones web|Un sitio en su propia ventana, con su icono en el lanzador y su propio perfil. También desde el menú, con «Instalar como aplicación web…».|globe
boosts|Boosts|Boosts|Cambia el aspecto de un sitio: modo oscuro y tu propio CSS. Solo estilo, nunca scripts. Para ganar al CSS de la web, termina cada regla con !important.|sparkle
ai|IA|Inteligencia artificial|Un asistente opcional que responde y puede ayudarte con tus pestañas, con tu propia clave. Está apagado hasta que lo actives.|sparkle
extensions|Extensiones|Extensiones|Extensiones de Chromium: instálalas desde una carpeta, un archivo .crx o la Chrome Web Store. Funcionan con scripts de página, ventanas emergentes, segundo plano y reglas de bloqueo; no todas funcionarán.|blocks
archive|Archivo|Archivo|Las pestañas que llevan tiempo sin usarse se cierran solas y quedan guardadas aquí.|clock
sync|Sincronización|Sincronización|Lleva marcadores, espacios y pestañas a tus otros dispositivos con tu nube.|cloud
privacy|Privacidad|Privacidad|Bloqueador de anuncios y rastreadores.|shield-check
sites|Sitios|Sitios sin bloqueador|Páginas donde el bloqueador está apagado.|globe
security|Seguridad|Seguridad|Lo que Brújula hace por ti sin preguntar.|lock
about|Acerca de|Acerca de||info"#;

/// A section of the page: id, name in the list, title, hint and icon.
type Page = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

static PAGES: LazyLock<Vec<Page>> = LazyLock::new(|| {
    PAGES_TABLE
        .lines()
        .filter_map(|line| {
            let mut p = line.splitn(5, '|');
            Some((p.next()?, p.next()?, p.next()?, p.next()?, p.next()?))
        })
        .collect()
});

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

/// The boosts: a form to add a site, and one card for each.
fn boosts_section(view: &Extras) -> String {
    let add = stacked(
        "Añadir un sitio",
        "La dirección o el nombre, como wikipedia.org. Vale también para sus subdominios.",
        r#"<div class="addrow"><input type="text" id="boost-host" class="field wide" placeholder="sitio.com" maxlength="120" aria-label="Sitio"><button type="button" class="seg plain" data-addboost="1">Añadir</button></div>"#,
    );
    let cards: String = view
        .boosts
        .iter()
        .map(|(host, dark, css)| {
            let h = escape(host);
            format!(
                r#"<div class="boost"><div class="row"><div class="what"><b>{h}</b></div><div class="ctl"><span class="status">Modo oscuro</span>{dark}<button type="button" class="seg plain danger" data-k="boost.remove" data-v="{h}" data-removeall="1">Quitar</button></div></div><div class="row stack"><textarea class="css" data-k="boost.css.{h}" rows="6" maxlength="{max}" spellcheck="false" placeholder="CSS de este sitio, por ejemplo: body {{ font-size: 18px !important; }}" aria-label="CSS de {h}">{css}</textarea></div></div>"#,
                dark = switch(&format!("boost.dark.{host}"), *dark),
                max = crate::boosts::MAX_CSS,
                css = escape(css),
            )
        })
        .collect();
    section("boosts", &format!("{add}{cards}"))
}

/// How many pages of history the page lists; the rest are still kept.
const HISTORY_SHOWN: usize = 100;

/// The history: the switch, ways to clear it, and the pages visited last.
fn history_section(s: &Settings, view: &Extras) -> String {
    let controls = [
        row(
            "Guardar el historial",
            "Si lo apagas, las páginas nuevas no se guardan. Las que ya hay siguen hasta que las borres.",
            &switch("history_enabled", s.history_enabled),
        ),
        row(
            "Borrar",
            &format!("Ahora hay {} páginas guardadas.", view.history_total),
            r#"<button type="button" class="seg plain danger" data-k="history.clear" data-v="hour" data-redraw="1">La última hora</button><button type="button" class="seg plain danger" data-k="history.clear" data-v="day" data-redraw="1">Últimas 24 horas</button><button type="button" class="seg plain danger" data-k="history.clear" data-v="all" data-redraw="1">Todo</button>"#,
        ),
    ]
    .concat();
    let items: String = view
        .history
        .iter()
        .take(HISTORY_SHOWN)
        .map(|(url, title)| {
            let u = escape(url);
            let name = if title.is_empty() { url } else { title };
            format!(
                r#"<div class="row"><div class="what"><b>{name}</b><p>{u}</p></div><div class="ctl"><button type="button" class="seg plain danger" data-k="history.remove" data-v="{u}" data-remove="1">Quitar</button></div></div>"#,
                name = escape(name),
            )
        })
        .collect();
    let empty = if view.history.is_empty() {
        r#"<p class="empty">Todavía no hay páginas guardadas.</p>"#
    } else {
        ""
    };
    section("history", &format!("{controls}{items}{empty}"))
}

/// The password vault: the switch, the master password, and what is saved (never the passwords).
fn passwords_section(s: &Settings, view: &Extras) -> String {
    let v = &view.vault;
    let switch_row = row(
        "Guardar contraseñas",
        "Brújula pregunta antes de guardar una y solo la rellena en el sitio exacto donde se guardó, cuando tú tocas el campo.",
        &switch("passwords_enabled", s.passwords_enabled),
    );
    let passkeys_row = row(
        "Llaves de acceso con el teléfono",
        "Cuando un sitio te deja entrar con una llave de acceso, Brújula muestra un código QR: lo escaneas con el teléfono, que está cerca (hace falta Bluetooth), y confirmas allí. La llave no sale del teléfono.",
        &switch("passkeys_enabled", s.passkeys_enabled),
    );
    let state = if !v.exists {
        row(
            "Contraseña maestra",
            "Todavía no hay contraseñas guardadas. La maestra cifra el archivo; si la olvidas no se puede recuperar.",
            r#"<button type="button" class="seg plain" data-k="pw.create" data-v="1">Crear…</button>"#,
        )
    } else if !v.open {
        row(
            "Bloqueadas",
            "Se piden la contraseña maestra para verlas o rellenarlas.",
            r#"<button type="button" class="seg plain" data-k="pw.unlock" data-v="1">Desbloquear…</button><button type="button" class="seg plain danger" data-k="pw.wipe" data-v="1">Borrar…</button>"#,
        )
    } else {
        row(
            "Desbloqueadas",
            "Se bloquean solas tras unos minutos sin usarlas y al cerrar la ventana.",
            r#"<button type="button" class="seg plain" data-k="pw.lock" data-v="1" data-redraw="1">Bloquear</button><button type="button" class="seg plain" data-k="pw.change" data-v="1">Cambiar maestra…</button><button type="button" class="seg plain danger" data-k="pw.wipe" data-v="1">Borrar…</button>"#,
        )
    };
    let logins: String = v
        .logins
        .iter()
        .filter(|_| v.open)
        .map(|(id, site, user)| {
            // The codes of two-step verification: show them, or add the secret.
            let codes = if v.totp.contains(id) {
                format!(
                    r#"<button type="button" class="seg plain" data-k="pw.totp_show" data-v="{id}">Código 2FA</button><button type="button" class="seg plain danger" data-k="pw.totp_remove" data-v="{id}" data-redraw="1">Quitar 2FA</button>"#
                )
            } else {
                format!(
                    r#"<button type="button" class="seg plain" data-k="pw.totp_add" data-v="{id}">Añadir 2FA…</button>"#
                )
            };
            format!(
                r#"<div class="row"><div class="what"><b>{site}</b><p>{user}</p></div><div class="ctl"><button type="button" class="seg plain" data-k="pw.copy" data-v="{id}">Copiar</button>{codes}<button type="button" class="seg plain danger" data-k="pw.remove" data-v="{id}" data-remove="1">Quitar</button></div></div>"#,
                site = escape(site),
                user = escape(user),
            )
        })
        .collect();
    let never: String = v
        .never
        .iter()
        .filter(|_| v.open)
        .map(|site| {
            format!(
                r#"<div class="row"><div class="what"><b>{site}</b><p>No se ofrece guardar contraseñas aquí.</p></div><div class="ctl"><button type="button" class="seg plain" data-k="pw.unnever" data-v="{site}" data-remove="1">Volver a ofrecer</button></div></div>"#,
                site = escape(site),
            )
        })
        .collect();
    let empty = if v.open && v.logins.is_empty() {
        r#"<p class="empty">Todavía no hay contraseñas guardadas.</p>"#
    } else {
        ""
    };
    section(
        "passwords",
        &format!("{switch_row}{passkeys_row}{state}{logins}{empty}{never}"),
    )
}

/// Bringing data from other browsers: one row for each one found, and a file.
fn import_section(view: &Extras) -> String {
    let browsers: String = view
        .import
        .iter()
        .enumerate()
        .map(|(n, (name, passwords))| {
            let button = |kind: &str, label: &str| {
                format!(
                    r#"<button type="button" class="seg plain" data-k="import.run" data-v="{n}:{kind}">{label}</button>"#
                )
            };
            let pw = if *passwords {
                button("passwords", "Contraseñas")
            } else {
                String::new()
            };
            let hint = if *passwords {
                "Las contraseñas piden tu contraseña maestra; si el escritorio guarda la clave de este navegador, puede pedirte permiso."
            } else {
                "Sus contraseñas no se pueden leer directamente: expórtalas a un archivo CSV desde ese navegador y elígelo abajo."
            };
            row(
                name,
                hint,
                &format!(
                    "{}{}{}{}{}",
                    button("all", "Todo"),
                    button("bookmarks", "Marcadores"),
                    button("history", "Historial"),
                    button("cookies", "Cookies"),
                    pw
                ),
            )
        })
        .collect();
    let none = if view.import.is_empty() {
        r#"<p class="empty">No se ha encontrado ningún navegador en este equipo.</p>"#
    } else {
        ""
    };
    let file = row(
        "Desde un archivo",
        "Marcadores (HTML o JSON) o contraseñas exportadas (CSV) de cualquier navegador o gestor.",
        r#"<button type="button" class="seg plain" data-k="import.file" data-v="1">Elegir archivo…</button>"#,
    );
    section("import", &format!("{browsers}{none}{file}"))
}

/// The AI assistant. Off by default; while off, this is one switch and what it means.
fn ai_section(view: &AiView) -> String {
    use crate::ai::{Autonomy, Provider};
    let s = &view.settings;
    let master = row(
        "Usar un asistente de IA",
        "Opcional y apagado por defecto. Apagado, Brújula no envía nada a ninguna IA, no enseña el botón y no toca el llavero.",
        &switch("ai.enabled", s.enabled),
    );
    if !s.enabled {
        return section("ai", &master);
    }
    let providers: String = Provider::ALL
        .iter()
        .map(|p| {
            let on = if *p == s.provider { " on" } else { "" };
            format!(
                r#"<button type="button" class="seg{on}" data-k="ai.provider" data-v="{}">{}</button>"#,
                p.id(),
                escape(p.label())
            )
        })
        .collect();
    let provider = stacked(
        "Proveedor",
        "Tú eliges quién responde y con qué clave. «Otro servidor» vale para cualquiera que hable como OpenAI (LM Studio, OpenRouter, la pasarela de tu empresa…).",
        &format!(r#"<div class="segs" role="group" style="flex-wrap:wrap">{providers}</div>"#),
    );
    let model = stacked(
        "Modelo",
        "El nombre exacto del modelo. Déjalo vacío para usar el habitual del proveedor.",
        &format!(
            r#"<div class="addrow"><input type="text" class="field wide" data-k="ai.model" value="{}" placeholder="{}" maxlength="80" spellcheck="false" aria-label="Modelo"></div>"#,
            escape(&s.model),
            escape(s.provider.default_model())
        ),
    );
    let address = if s.provider.takes_address() {
        stacked(
            "Dirección del servidor",
            "https, o http solo en este mismo equipo (por ejemplo http://localhost:11434/v1). Fuera del equipo la conversación no viaja sin cifrar.",
            &format!(
                r#"<div class="addrow"><input type="text" class="field wide" data-k="ai.base_url" value="{}" placeholder="{}" maxlength="200" spellcheck="false" aria-label="Dirección del servidor"></div>"#,
                escape(&s.base_url),
                escape(s.provider.default_base())
            ),
        )
    } else {
        String::new()
    };
    let key = if s.provider.can_have_key() {
        let hint = if view.key_set {
            "Guardada en el llavero del sistema. Brújula no la puede mostrar de nuevo."
        } else if s.provider.needs_key() {
            "Falta. Se guarda en el llavero del sistema (no en un archivo) y solo se envía a este proveedor."
        } else {
            "Opcional para este servidor. Si la pones, se guarda en el llavero del sistema."
        };
        let remove = if view.key_set {
            r#"<button type="button" class="seg plain danger" data-k="ai.key.clear" data-v="1">Quitar</button>"#
        } else {
            ""
        };
        row(
            "Clave de API",
            hint,
            &format!(
                r#"<button type="button" class="seg plain" data-k="ai.key.set" data-v="1">{}</button>{remove}"#,
                if view.key_set {
                    "Cambiar…"
                } else {
                    "Poner clave…"
                }
            ),
        )
    } else {
        String::new()
    };
    let test = row(
        "Probar la conexión",
        if view.status.is_empty() {
            "Hace una pregunta mínima para ver que la clave y el modelo responden."
        } else {
            &view.status
        },
        r#"<button type="button" class="seg plain" data-k="ai.test" data-v="1">Probar</button>"#,
    );
    let open_modes = [("ask", "Preguntar siempre"), ("auto", "Sin preguntar")]
        .iter()
        .map(|(id, label)| {
            let on = if (*id == "auto") == (s.open == Autonomy::Auto) {
                " on"
            } else {
                ""
            };
            format!(r#"<button type="button" class="seg{on}" data-k="ai.open" data-v="{id}">{label}</button>"#)
        })
        .collect::<String>();
    let permissions = [
        row(
            "Ver mis pestañas y leer páginas",
            "Si lo apagas, la IA solo ve lo que le escribes. Nunca ve contraseñas, campos de pago, ventanas privadas ni las páginas del propio navegador.",
            &switch("ai.read", s.read),
        ),
        row(
            "Preguntar antes de leer cada sitio",
            "La primera vez que la IA quiere leer un sitio en una conversación te lo pregunta. Las pestañas que abre ella misma para la tarea se leen sin preguntar.",
            &switch("ai.confirm_reads", s.confirm_reads),
        ),
        row(
            "Abrir pestañas",
            "«Sin preguntar» deja que abra búsquedas y enlaces que ha visto; una dirección que no salía en lo que leyó se pregunta siempre. Nunca abre direcciones de la red local.",
            &format!(r#"<div class="segs" role="group">{open_modes}</div>"#),
        ),
        row(
            "Pulsar y escribir en las páginas",
            "Apagado por defecto. Encendido, la IA puede proponer pulsar un botón o rellenar un campo, y cada vez te lo pregunta enseñándote qué va a hacer. Nunca escribe en campos de contraseña ni de pago.",
            &switch("ai.interact", s.interact),
        ),
    ]
    .concat();
    let search = row(
        "La barra de búsqueda la gestiona la IA",
        "Lo que escribes en el Spotlight que no es una dirección va al asistente: si es una pregunta, responde; si es una tarea («búscame sitios baratos para viajar»), decide buscar y abrir pestañas. La búsqueda normal sigue debajo.",
        &switch("ai.search", s.search),
    );
    let blocked: String = s
        .blocked
        .iter()
        .map(|h| {
            format!(
                r#"<div class="row"><div class="what"><b>{h}</b><p>La IA no ve ni toca este sitio.</p></div><div class="ctl"><button type="button" class="seg plain danger" data-k="ai.blocked.remove" data-v="{h}" data-remove="1">Quitar</button></div></div>"#,
                h = escape(h)
            )
        })
        .collect();
    let exclude = stacked(
        "Sitios que la IA no puede ver",
        "Escribe un sitio y pulsa Intro (por ejemplo, tu banco). Vale también para sus subdominios.",
        r#"<div class="addrow"><input type="text" class="field wide" data-k="ai.blocked.add" placeholder="banco.com" maxlength="120" spellcheck="false" aria-label="Sitio excluido"></div>"#,
    );
    let sent = if s.is_remote() {
        format!(
            "Se envía a {}: lo que escribes y, si lo permites, los títulos y direcciones de tus pestañas y el texto de las páginas que apruebes. Cada página que lee queda anotada en el panel con los caracteres enviados.",
            escape(s.provider.short())
        )
    } else {
        "Todo se queda en este equipo: el servidor está aquí mismo.".to_owned()
    };
    let facts = format!(
        r#"<ul class="facts"><li>{sent}</li><li>Nunca se envían: contraseñas guardadas, el valor de los campos de contraseña o de pago, cookies, ni nada de las ventanas privadas (allí no hay asistente) o de los sitios excluidos.</li><li>El texto de una página no son órdenes: si una página intenta darle instrucciones a la IA, se trata como texto, y abrir algo que no salía en lo leído se pregunta aunque hayas puesto «sin preguntar».</li><li>La IA puede equivocarse. Revisa lo que haga antes de fiarte de ello, sobre todo si hay dinero o datos personales de por medio.</li></ul>"#
    );
    section(
        "ai",
        &[
            master,
            provider,
            model,
            address,
            key,
            test,
            permissions,
            search,
            exclude,
            blocked,
            facts,
        ]
        .concat(),
    )
}

/// The essentials: the ones there are, a form for another, and the usual sites to pick from.
fn essentials_section(view: &Extras) -> String {
    use crate::essentials::{MAX_ESSENTIALS, SUGGESTED};
    let have: Vec<String> = view.essentials.iter().map(|(_, _, u)| u.clone()).collect();
    let items: String = view
        .essentials
        .iter()
        .map(|(id, name, url)| {
            format!(
                r#"<div class="row"><div class="what"><b>{}</b><p>{}</p></div><div class="ctl"><button type="button" class="seg plain" data-k="essential.up" data-v="{id}" data-redraw="1">Subir</button><button type="button" class="seg plain" data-k="essential.down" data-v="{id}" data-redraw="1">Bajar</button><button type="button" class="seg plain danger" data-k="essential.remove" data-v="{id}" data-remove="1">Quitar</button></div></div>"#,
                escape(name),
                escape(url),
            )
        })
        .collect();
    let empty = if view.essentials.is_empty() {
        r#"<p class="empty">Todavía no hay esenciales.</p>"#
    } else {
        ""
    };
    let full = view.essentials.len() >= MAX_ESSENTIALS;
    let add = if full {
        stacked(
            "Añadir un esencial",
            &format!("Ya hay {MAX_ESSENTIALS}, el máximo. Quita alguno para añadir otro."),
            "",
        )
    } else {
        let picks: String = SUGGESTED
            .iter()
            .filter(|s| !have.iter().any(|u| u.trim_end_matches('/') == s.url.trim_end_matches('/')))
            .map(|s| {
                format!(
                    r#"<button type="button" class="seg plain" data-k="essential.suggest" data-v="{}" data-redraw="1">{}</button>"#,
                    escape(s.url),
                    escape(s.name),
                )
            })
            .collect();
        stacked(
            "Añadir un esencial",
            "Elige uno de los habituales o escribe la dirección de cualquier sitio. También desde el menú de una pestaña, con «Añadir a esenciales».",
            &format!(
                r#"<div class="addrow">{picks}</div><div class="addrow"><input type="text" id="essential-name" class="field" placeholder="Nombre (opcional)" maxlength="24" aria-label="Nombre"><input type="text" id="essential-url" class="field wide" placeholder="https://…" maxlength="2048" aria-label="Dirección"><button type="button" class="seg plain" data-addessential="1">Añadir</button></div>"#
            ),
        )
    };
    section("essentials", &format!("{items}{empty}{add}"))
}

/// The profiles: the ones there are and a form for another.
fn profiles_section(view: &Extras) -> String {
    use crate::profiles::{DEFAULT_ID, MAX_PROFILES};
    let items: String = view
        .profiles
        .iter()
        .map(|(id, name, avatar, locked)| {
            let current = *id == view.profile;
            let is_default = id == DEFAULT_ID;
            let id = escape(id);
            let state = match (current, *locked) {
                (true, true) => "Este perfil · con contraseña",
                (true, false) => "Este perfil",
                (false, true) => "Con contraseña",
                (false, false) => "Sin contraseña",
            };
            let open = if current {
                String::new()
            } else {
                format!(r#"<button type="button" class="seg plain" data-k="profile.open" data-v="{id}">Abrir</button>"#)
            };
            let remove = if current || is_default {
                String::new()
            } else {
                format!(r#"<button type="button" class="seg plain danger" data-k="profile.remove" data-v="{id}">Quitar</button>"#)
            };
            format!(
                r#"<div class="row"><span class="pface">{svg}</span><div class="what"><b>{name}</b><p>{state}</p></div><div class="ctl">{open}<button type="button" class="seg plain" data-k="profile.password" data-v="{id}">Contraseña</button>{remove}</div></div>"#,
                svg = crate::avatars::get(avatar).svg(40),
                name = escape(name),
            )
        })
        .collect();
    let empty = if view.profiles.is_empty() {
        r#"<p class="empty">Solo tienes el perfil principal. Añade otro para que cada persona tenga sus marcadores, pestañas y contraseñas.</p>"#
    } else {
        ""
    };
    let note = stacked(
        "Qué protege la contraseña",
        "Solo impide abrir tu perfil desde el selector. No cifra los archivos del perfil: quien tenga acceso a esta cuenta del equipo podría leerlos. Para eso usa la bóveda de contraseñas y el cifrado del disco.",
        "",
    );
    let add = if view.profiles.len() >= MAX_PROFILES {
        stacked(
            "Añadir un perfil",
            &format!("Ya hay {MAX_PROFILES}, el máximo."),
            "",
        )
    } else {
        let faces: String = crate::avatars::AVATARS
            .iter()
            .enumerate()
            .map(|(i, a)| {
                format!(
                    r#"<label class="pav" title="{}"><input type="radio" name="pavatar" value="{}"{}>{}</label>"#,
                    escape(a.name),
                    a.id,
                    if i == 0 { " checked" } else { "" },
                    a.svg(36),
                )
            })
            .collect();
        stacked(
            "Añadir un perfil",
            "Un nombre y una imagen. La contraseña, si la quieres, se pone después con el botón «Contraseña».",
            &format!(
                r#"<div class="pavs">{faces}</div><div class="addrow"><input type="text" id="profile-name" class="field wide" placeholder="Nombre" maxlength="24" aria-label="Nombre"><button type="button" class="seg plain" data-addprofile="1">Añadir</button></div>"#
            ),
        )
    };
    section("profiles", &format!("{items}{empty}{add}{note}"))
}

/// The web apps: a form to make one, and the ones that exist.
fn webapps_section(view: &Extras) -> String {
    let add = stacked(
        "Crear una aplicación web",
        "La dirección del sitio y, si quieres, un nombre. Su perfil (cookies y sesiones) queda aparte del navegador y de las demás aplicaciones; los enlaces de otros sitios se abren en tu navegador.",
        r#"<div class="addrow"><input type="text" id="webapp-name" class="field" placeholder="Nombre (opcional)" maxlength="40" aria-label="Nombre"><input type="text" id="webapp-url" class="field wide" placeholder="https://…" maxlength="2048" aria-label="Dirección"><button type="button" class="seg plain" data-addwebapp="1">Crear</button></div>"#,
    );
    let items: String = view
        .webapps
        .iter()
        .map(|(id, name, url)| {
            let id = escape(id);
            format!(
                r#"<div class="row"><div class="what"><b>{}</b><p>{}</p></div><div class="ctl"><button type="button" class="seg plain" data-k="webapp.open" data-v="{id}">Abrir</button><button type="button" class="seg plain danger" data-k="webapp.remove" data-v="{id}" data-remove="1">Quitar</button></div></div>"#,
                escape(name),
                escape(url),
            )
        })
        .collect();
    let empty = if view.webapps.is_empty() {
        r#"<p class="empty">Todavía no hay aplicaciones web.</p>"#
    } else {
        ""
    };
    section("webapps", &format!("{add}{items}{empty}"))
}

/// The extensions: ways to add one, and a card for each that is installed.
fn extensions_section(view: &Extras) -> String {
    let add = stacked(
        "Instalar una extensión",
        "Desde una carpeta con su manifest.json, un archivo .crx o .zip, o la dirección (o el identificador) de su página en la Chrome Web Store. Antes de instalar verás lo que podrá hacer.",
        r#"<div class="addrow"><button type="button" class="seg plain" data-k="ext.install_folder" data-v="1">Elegir carpeta…</button><button type="button" class="seg plain" data-k="ext.install_file" data-v="1">Elegir archivo…</button></div><div class="addrow"><input type="text" id="ext-store" class="field wide" placeholder="https://chromewebstore.google.com/detail/… o identificador" maxlength="300" aria-label="Chrome Web Store"><button type="button" class="seg plain" data-addext="1">Instalar</button></div>"#,
    );
    let cards: String = view
        .extensions
        .iter()
        .map(|e| {
            let id = escape(&e.id);
            let perms: String = if e.permissions.is_empty() {
                String::new()
            } else {
                let items: String = e
                    .permissions
                    .iter()
                    .map(|p| format!("<li>{}</li>", escape(p)))
                    .collect();
                format!(r#"<ul class="facts">{items}</ul>"#)
            };
            let note = if e.problem {
                r#"<span class="status">No se pudo leer: reinstálala.</span>"#.to_owned()
            } else {
                String::new()
            };
            let options = if e.options {
                format!(r#"<button type="button" class="seg plain" data-k="ext.options" data-v="{id}">Opciones</button>"#)
            } else {
                String::new()
            };
            format!(
                r#"<div class="boost"><div class="row"><div class="what"><b>{name}</b> <span class="status">{version}</span><p>{desc}</p>{note}</div><div class="ctl">{options}{switch}<button type="button" class="seg plain danger" data-k="ext.remove" data-v="{id}" data-removeall="1">Quitar</button></div></div>{perms}</div>"#,
                name = escape(&e.name),
                version = escape(&e.version),
                desc = escape(&e.description),
                switch = switch(&format!("ext.enable.{}", e.id), e.enabled),
            )
        })
        .collect();
    let empty = if view.extensions.is_empty() {
        r#"<p class="empty">Todavía no hay extensiones instaladas.</p>"#
    } else {
        ""
    };
    section("extensions", &format!("{add}{cards}{empty}"))
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
    let sleep: String = [
        (0, "Nunca"),
        (10, "10 minutos"),
        (30, "30 minutos"),
        (60, "1 hora"),
        (180, "3 horas"),
    ]
    .iter()
    .map(|(minutes, label)| {
        let on = if *minutes == s.sleep_after_minutes { " on" } else { "" };
        format!(r#"<button type="button" class="seg{on}" data-k="sleep_after" data-v="{minutes}">{label}</button>"#)
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
                "Dormir las pestañas sin usar",
                "Su página sale de la memoria y se vuelve a cargar al abrirlas. Siguen en la barra; no se cierran. Las que suenan o cargan no duermen.",
                &format!(r#"<div class="segs" role="group">{sleep}</div>"#),
            ),
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

/// The folders the browser offers, the one in use and the buttons to pick another.
fn folder_picker(c: &crate::settings::Sync, view: &Extras) -> String {
    let offered: String = view
        .folders
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let on = if *path == c.folder { " on" } else { "" };
            format!(
                r#"<button type="button" class="engine{on}" data-k="sync_folder" data-v="{i}">{icon}<span class="acct"><b>{name}</b><small>{path}</small></span></button>"#,
                icon = icons::web_svg("cloud", 20),
                name = escape(
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path)
                ),
                path = escape(path),
            )
        })
        .collect();
    let current = if c.folder.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="status">En uso: <b>{}</b></span>"#,
            escape(&c.folder)
        )
    };
    let forget = if c.folder.is_empty() {
        ""
    } else {
        r#"<button type="button" class="seg plain" data-k="sync_folder" data-v="forget">Dejar de usarla</button>"#
    };
    format!(
        r#"<div class="engines" role="group">{offered}</div><div class="actions"><button type="button" class="seg plain" data-k="sync_folder" data-v="dialog">Elegir carpeta…</button>{forget}{current}</div>"#
    )
}

/// The synchronisation section.
fn sync_section(s: &Settings, view: &Extras) -> String {
    let c = &s.sync;
    let accounts = if view.accounts.is_empty() {
        r#"<p class="empty">Ninguna cuenta de Nodalix. Puedes añadir Google Drive, OneDrive, Dropbox o iCloud Drive en Ajustes, o elegir una carpeta aquí abajo.</p>"#.to_owned()
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
    let folders = folder_picker(c, view);
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
        stacked(
            "Carpeta",
            "Sin cuentas de Nodalix, usa una carpeta que ya sincronice tu nube: Dropbox, Nextcloud, iCloud Drive, Syncthing o una unidad de red.",
            &folders,
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

/// The buttons that pick the texture, and how strong it is.
fn texture_picker(a: &crate::settings::Appearance) -> String {
    let buttons: String = crate::settings::Texture::ALL
        .iter()
        .map(|(t, id, label)| {
            let on = if *t == a.texture { " on" } else { "" };
            format!(r#"<button type="button" class="seg{on}" data-k="texture" data-v="{id}">{label}</button>"#)
        })
        .collect();
    format!(
        r#"<div class="actions"><div class="segs" role="group">{buttons}</div></div>{}"#,
        slider("texture_amount", a.texture_amount, 10, 100, " %")
    )
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
            stacked(
                "Degradado del marco",
                "Varios colores que se funden; se usa en lugar del color liso. Elige uno hecho o monta el tuyo.",
                &gradient_picker(a),
            ),
            stacked(
                "Textura",
                "Un relieve fino sobre el marco y los paneles, bajo los colores.",
                &texture_picker(a),
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
    let boosts_section = boosts_section(sync);
    let history_section = history_section(s, sync);
    let passwords_section = passwords_section(s, sync);
    let import_section = import_section(sync);
    let webapps_section = webapps_section(sync);
    let essentials_section = essentials_section(sync);
    let profiles_section = profiles_section(sync);
    let ai_section = ai_section(&sync.ai);
    let extensions_section = extensions_section(sync);
    let security = section(
        "security",
        &[
            row(
                "HTTPS primero",
                "Las direcciones http:// de sitios públicos se prueban antes como https://. Si el sitio no responde en HTTPS, se abre la dirección original; un certificado malo nunca se salta.",
                &switch("https_first", s.https_first),
            ),
            r#"<ul class="facts">
<li>Los programas y scripts (.AppImage, .sh, .deb…) piden confirmación antes de bajarse y se guardan sin permiso de ejecución; los nombres de archivo se limpian.</li>
<li>Cada descarga se guarda con un nombre oculto, se analiza al llegar (sin frenar la descarga) y solo entonces recibe su nombre. Si dice ser un documento, una imagen o un vídeo y por dentro es un programa, o si ClamAV está instalado y la reconoce como malware, te pregunta si la borras.</li>
<li><code>javascript:</code>, <code>data:</code> y <code>blob:</code> no se abren desde la barra de direcciones.</li>
<li>Los errores de certificado nunca se saltan.</li>
<li>Cámara, micrófono, ubicación y notificaciones se deniegan.</li>
<li>Sin ventanas emergentes automáticas; una web no puede abrir archivos locales.</li>
<li>Prevención de rastreo inteligente (ITP) y aislamiento por procesos de WebKit.</li>
</ul>"#
                .to_owned(),
        ]
        .concat(),
    );

    let update_button = if sync.update_version.is_empty() {
        r#"<button type="button" class="seg plain" data-k="update.check" data-v="now">Buscar ahora</button>"#.to_owned()
    } else {
        format!(
            r#"<button type="button" class="seg" data-k="update.install" data-v="now">Actualizar a {}</button>"#,
            escape(&sync.update_version)
        )
    };
    let update_hint = if sync.update_status.is_empty() {
        "Brújula mira en GitHub si hay una versión nueva. Solo pide el número de versión: no envía nada tuyo."
    } else {
        &sync.update_status
    };
    let about = section(
        "about",
        &[
            row(
                &format!("Brújula {}", env!("CARGO_PKG_VERSION")),
                "El navegador de Nodalix. WebKit, bloqueador integrado, pestañas verticales y el lenguaje de diseño NDL.",
                "",
            ),
            row("Actualizaciones", update_hint, &update_button),
            row(
                "Buscar al abrir",
                "Mira una vez cada doce horas si hay una versión nueva y te lo dice. El AppImage se actualiza solo si quieres; una copia instalada con el sistema se actualiza con el sistema.",
                &switch("update_check", s.update_check),
            ),
        ]
        .concat(),
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
<style>{faces}{css}</style></head>
<body style="--accent:{accent};--on-accent:{on_accent}">
<div class="shell">
<nav class="nav" aria-label="Secciones"><h1>Preferencias</h1>{nav}</nav>
<main>{general}{appearance}{layout_section}{spaces_section}{profiles_section}{essentials_section}{bookmarks_section}{history_section}{passwords_section}{import_section}{webapps_section}{boosts_section}{ai_section}{extensions_section}{archive_section}{sync_section}{privacy}{sites}{security}{about}</main>
</div>
<script>{js}</script>
</body></html>
"##,
        faces = crate::fonts::FACES,
        css = CSS,
        js = JS,
        accent = escape(&a.accent),
        on_accent = crate::settings::text_on(&a.accent),
    )
}

const SECTIONS_TABLE: &str = r#"Buscador|motor google duckduckgo bing buscar|general
Tema|claro oscuro modo sistema|appearance
Color de acento|color tinte acento|appearance
Color del marco|marco fondo color|appearance
Transparencia|opacidad transparente cristal|appearance
Textura|granulado grano relieve puntos líneas cuadrícula|appearance
Redondeo de esquinas|bordes radio esquinas|appearance
Disposición|islas marco zen diseño layout|layout
Pestañas horizontales|pestañas arriba horizontal vertical|layout
Ancho de la barra lateral|sidebar tamaño ancho|layout
Espacios|espacio espacios grupos perfiles workspaces iconos|spaces
Marcadores|favoritos marcadores guardados estrella|bookmarks
Historial|historial visitadas recientes borrar navegación pasado|history
Ventana privada|privado privada incógnito incognito sin rastro navegación|history
Perfiles|perfil perfiles usuario usuarios personas contraseña selector cuenta avatar|profiles
Esenciales|esenciales fijos iconos barra lateral atajos sitios rápidos|essentials
Aplicaciones web|webapp webapps aplicaciones web instalar pwa ventana lanzador atajo|webapps
Contraseñas|contraseñas claves login usuario guardar rellenar maestra bóveda cifrado|passwords
Boosts|boost estilo css modo oscuro sitio aspecto personalizar|boosts
Inteligencia artificial|ia ai inteligencia artificial asistente chatgpt openai claude anthropic gemini ollama gpt llm modelo clave api chat|ai
Extensiones|extensiones chrome chromium complementos plugins addons instalar crx|extensions
Archivo de pestañas|archivar archivo pestañas antiguas cerradas recuperar|archive
Sincronización|sincronizar nube icloud onedrive google drive dropbox dispositivos|sync
Bloqueador de anuncios|adblock anuncios rastreadores listas|privacy
Sitios sin bloqueador|excepciones permitidos|sites
Seguridad|descargas permisos certificados|security
Actualizaciones|actualizar actualización versión nueva novedades update|about"#;

/// Page used by the Spotlight to offer settings: `(title, keywords, section id)`.
pub static SECTIONS: LazyLock<Vec<(&'static str, &'static str, &'static str)>> =
    LazyLock::new(|| {
        SECTIONS_TABLE
            .lines()
            .filter_map(|line| {
                let mut p = line.splitn(3, '|');
                Some((p.next()?, p.next()?, p.next()?))
            })
            .collect()
    });

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
body{margin:0;font-family:Inter,"Adwaita Sans",system-ui,sans-serif;background:var(--bg);color:var(--fg);line-height:1.45;-webkit-font-smoothing:antialiased}
.shell{display:grid;grid-template-columns:14.5rem minmax(0,44rem);justify-content:center;gap:3.5rem;padding:3.25rem 2rem 8rem}
.nav{position:sticky;top:3rem;align-self:start;display:flex;flex-direction:column;gap:.15rem}
.nav h1{font-size:1.65rem;font-weight:700;letter-spacing:-.03em;margin:0 0 1.1rem .8rem}
.nav a{display:flex;align-items:center;gap:.75rem;color:var(--muted);text-decoration:none;padding:.55rem .8rem;border-radius:.75rem;font-size:.95rem;transition:background .15s,color .15s}
.nav a:hover{background:var(--chip);color:var(--fg)}
.nav a.on{background:var(--card);color:var(--fg);box-shadow:var(--shadow);font-weight:560}
.nav a.on .ico{color:var(--accent)}
.ico{flex:none}
main>section{display:none;margin-bottom:3rem}
main>section.on{display:block}
.shead{display:flex;gap:.9rem;align-items:center;margin:0 .25rem 1rem}
.sicon{display:grid;place-items:center;flex:none;width:2.5rem;height:2.5rem;border-radius:.85rem;background:color-mix(in srgb,var(--accent) 14%,transparent);color:var(--accent)}
h2{font-size:1.4rem;font-weight:660;letter-spacing:-.02em;margin:0;line-height:1.2}
.shead p{margin:.15rem 0 0;color:var(--muted);font-size:.9rem}
.card{background:var(--card);border:1px solid var(--line);border-radius:1.15rem;box-shadow:var(--shadow)}
.row{display:flex;gap:1.5rem;justify-content:space-between;align-items:center;padding:1rem 1.25rem;border-top:1px solid var(--line)}
.row:first-child{border-top:0}
.row.stack{flex-direction:column;align-items:stretch;gap:.9rem}
.what{min-width:0;flex:1 1 auto}.what b,.what p{overflow-wrap:anywhere}.what b{font-weight:580}.what p{margin:.15rem 0 0;color:var(--muted);font-size:.86rem;max-width:32rem}
.row>.ctl{flex:none}
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
.actions{display:flex;flex-wrap:wrap;gap:.5rem;margin-top:.6rem}
.dot.grad{width:2.6rem;border-radius:.7rem}
.engines{display:grid;grid-template-columns:repeat(auto-fill,minmax(10rem,1fr));gap:.6rem;width:100%}
.engine{display:flex;align-items:center;gap:.7rem;padding:.65rem .8rem;border:1px solid var(--line);border-radius:.9rem;background:transparent;font:inherit;color:var(--fg);cursor:pointer;text-align:left;transition:border-color .15s,box-shadow .15s}
.engine:hover{border-color:color-mix(in srgb,var(--fg) 30%,var(--line))}
.engine.on{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent);background:color-mix(in srgb,var(--accent) 7%,transparent)}
.engine b{font-weight:560;font-size:.92rem}
.css{width:100%;font:12.5px ui-monospace,monospace;background:var(--chip);color:var(--fg);border:1px solid var(--line);border-radius:.7rem;padding:.7rem;resize:vertical}
.boost .row.stack{padding-top:0;border-top:0}
.pface{display:grid;place-items:center;flex:none;margin-right:.9rem}.pface+.what{flex:1}.pface svg{width:2.5rem;height:2.5rem;border-radius:50%}
.pavs{display:flex;flex-wrap:wrap;gap:.4rem;width:100%}
.pav{position:relative;display:block;border-radius:50%;cursor:pointer}.pav input{position:absolute;opacity:0;inset:0}
.pav svg{display:block;width:2.3rem;height:2.3rem;border-radius:50%;outline:3px solid transparent;outline-offset:2px;transition:outline-color .15s}
.pav input:checked+svg{outline-color:var(--accent)}.pav input:focus-visible+svg{outline-color:var(--fg)}
.addrow{display:flex;flex-wrap:wrap;gap:.6rem;width:100%}.addrow input{flex:1;min-width:0}
.acct{display:flex;flex-direction:column;min-width:0}.acct small{color:var(--muted);font-size:.78rem}
.status{color:var(--muted);font-size:.9rem;margin-right:.8rem;text-align:right;max-width:24rem}
.note{margin:0;padding:1rem 1.25rem;border-top:1px solid var(--line);color:var(--muted);font-size:.85rem}
.eng svg{width:64%;height:64%;display:block}
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
  if(nav){e.preventDefault();show(nav.getAttribute('href').slice(1));return}
  const add=e.target.closest('[data-addboost]');
  if(add){const h=document.getElementById('boost-host');if(h&&h.value.trim())send('boost.add',h.value.trim());return}
  const es=e.target.closest('[data-addessential]');
  if(es){const u=document.getElementById('essential-url'),n=document.getElementById('essential-name');if(u&&u.value.trim()){send('essential.add',u.value.trim()+'\n'+(n?n.value.trim():''));u.value='';if(n)n.value=''}return}
  const pf=e.target.closest('[data-addprofile]');
  if(pf){const n=document.getElementById('profile-name'),a=document.querySelector('input[name=pavatar]:checked');if(n&&n.value.trim()){send('profile.add',n.value.trim()+'\n'+(a?a.value:''));n.value=''}return}
  const wa=e.target.closest('[data-addwebapp]');
  if(wa){const u=document.getElementById('webapp-url'),n=document.getElementById('webapp-name');if(u&&u.value.trim()){send('webapp.add',u.value.trim()+'\n'+(n?n.value.trim():''));u.value='';if(n)n.value=''}return}
  const ext=e.target.closest('[data-addext]');
  if(ext){const h=document.getElementById('ext-store');if(h&&h.value.trim())send('ext.install_store',h.value.trim());return}
  const open=e.target.closest('[data-open]');
  if(open){open.closest('.space').classList.toggle('open');return}
  const b=e.target.closest('[data-k]');if(!b||b.tagName==='INPUT'||b.tagName==='TEXTAREA')return;
  const k=b.dataset.k;
  if(b.dataset.toggle){const on=!b.classList.contains('on');b.classList.toggle('on',on);b.setAttribute('aria-checked',on);send(k,on?'1':'0');return}
  const v=b.dataset.v;
  if(b.dataset.removeall){const r=b.closest('.boost');if(r)r.remove();send(k,b.dataset.v);return}
  if(b.dataset.redraw){send(k,v);setTimeout(()=>send('redraw','1'),200);return}
  if(b.dataset.remove){const r=b.closest('.row,.space');if(r)r.remove();send(k,v);return}
  send(k,v);
  if(k==='accent'){document.body.style.setProperty('--accent',v)}
  const group=b.closest('.segs,.dots,.tiles,.engines,.iconsgrid');
  if(group){group.querySelectorAll('.on').forEach(x=>x.classList.remove('on'));b.classList.add('on')}
  if(b.dataset.pick){const s=b.closest('.space');const cur=s.querySelector('.iconbtn .cur');if(cur){cur.innerHTML=b.innerHTML;const g=cur.firstElementChild;if(g){g.setAttribute('width',22);g.setAttribute('height',22)}}s.classList.remove('open')}
  if(k==='theme'){document.documentElement.dataset.theme=v}
  if(k==='reset_appearance'){setTimeout(()=>send('redraw','1'),150)}
});
document.addEventListener('change',e=>{
  const i=e.target;if((i.type==='text'||i.tagName==='TEXTAREA')&&i.dataset.k)send(i.dataset.k,i.value)
});
document.addEventListener('input',e=>{
  const i=e.target;if(!i.dataset||!i.dataset.k)return;
  if(i.type==='range'){fill(i);const o=unit(i);if(o)o.textContent=i.value+o.textContent.replace(/^[0-9]+/,'');later(()=>send(i.dataset.k,i.value))}
  else if(i.type==='color'){if(i.dataset.k==='accent')document.body.style.setProperty('--accent',i.value);later(()=>send(i.dataset.k,i.value))}
});
const links=[...document.querySelectorAll('[data-nav]')];
const secs=[...document.querySelectorAll('main>section')];
const show=(id)=>{
  const t=secs.find(s=>s.id===id)||secs[0];if(!t)return;
  secs.forEach(s=>s.classList.toggle('on',s===t));
  links.forEach(a=>{const on=a.dataset.nav===t.id;a.classList.toggle('on',on);if(on)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current')});
  window.scrollTo(0,0);
};
window.showSection=show;
show('general');
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
        s.apply("texture", "grain").unwrap();
        assert_eq!(s.appearance.texture, crate::settings::Texture::Grain);
        assert!(s.apply("texture", "<script>").is_err());
        s.apply("texture_amount", "999").unwrap();
        assert_eq!(s.appearance.texture_amount, 100);
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
    fn the_frame_gradient_takes_presets_single_stops_and_an_angle() {
        let mut s = Settings::default();
        s.apply("frame_gradient", "#112233,#AABBCC,#445566")
            .unwrap();
        assert_eq!(
            s.appearance.frame_gradient,
            vec!["#112233", "#aabbcc", "#445566"]
        );
        for bad in [
            "#112233",
            "#112233,red",
            "#1,#2",
            "a,b,c,d,e",
            "#112233;x,#445566",
        ] {
            assert!(s.apply("frame_gradient", bad).is_err(), "{bad}");
        }
        s.apply("frame_stop2", "#ff0000").unwrap();
        assert_eq!(s.appearance.frame_gradient[1], "#ff0000");
        s.apply("frame_stop3", "").unwrap();
        assert_eq!(s.appearance.frame_gradient.len(), 2);
        s.apply("frame_stop3", "#00ff00").unwrap();
        assert_eq!(s.appearance.frame_gradient.len(), 3);
        assert!(s.apply("frame_stop1", "url(x)").is_err());
        s.apply("frame_angle", "90").unwrap();
        assert_eq!(s.appearance.frame_angle, 90);
        assert!(s.apply("frame_angle", "50").is_err());
        assert!(s.apply("frame_angle", "360").is_err());
        // One stop on its own starts a gradient from the plain colour.
        let mut t = Settings::default();
        t.apply("frame_stop2", "#123456").unwrap();
        assert_eq!(t.appearance.frame_gradient.len(), 2);
        s.apply("frame_gradient", "").unwrap();
        assert!(s.appearance.frame_gradient.is_empty());
        let html = render(
            &t,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(html.contains(r#"data-k="frame_angle" data-v="135""#));
        assert!(html.contains(r#"data-k="frame_gradient""#));
    }

    #[test]
    fn https_first_is_on_and_can_be_switched() {
        let mut s = Settings::default();
        assert!(s.https_first);
        s.apply("https_first", "0").unwrap();
        assert!(!s.https_first);
        assert!(s.apply("https_first", "yes").is_err());
        let html = render(
            &s,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(html.contains(r#"data-k="https_first""#));
    }

    #[test]
    fn the_sleep_time_is_one_of_the_offered_ones() {
        let mut s = Settings::default();
        assert_eq!(s.sleep_after_minutes, 30);
        s.apply("sleep_after", "10").unwrap();
        s.apply("sleep_after", "0").unwrap();
        assert_eq!(s.sleep_after_minutes, 0);
        assert!(s.apply("sleep_after", "7").is_err());
        assert!(s.apply("sleep_after", "x").is_err());
        let html = render(
            &s,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(html.contains(r#"class="seg on" data-k="sleep_after" data-v="0""#));
        assert_eq!(
            Settings::from_json(r#"{"sleep_after_minutes": 5}"#).sleep_after_minutes,
            30
        );
        assert!(Settings::from_json(r#"{"sync": {"folder": "../x"}}"#)
            .sync
            .folder
            .is_empty());
    }

    #[test]
    fn the_folder_is_picked_by_dialog_or_from_the_offered_list_never_by_path() {
        let mut s = Settings::default();
        s.sync.account = "a1".into();
        s.sync.folder = "/home/u/Dropbox".into();
        assert_eq!(
            s.apply("sync_folder", "dialog").unwrap().folder,
            Some(FolderChoice::Dialog)
        );
        assert_eq!(
            s.apply("sync_folder", "2").unwrap().folder,
            Some(FolderChoice::Offered(2))
        );
        // The page cannot name a path.
        for bad in ["/etc", "../x", "x", "123", "-1", ""] {
            assert!(s.apply("sync_folder", bad).is_err(), "{bad}");
        }
        assert_eq!(s.sync.folder, "/home/u/Dropbox");
        let e = s.apply("sync_folder", "forget").unwrap();
        assert_eq!(e.folder, Some(FolderChoice::Forget));
        assert!(e.sync && s.sync.folder.is_empty());
        // An account and a folder do not go together: the last one picked wins.
        s.sync.folder = "/home/u/Dropbox".into();
        s.apply("sync_account", "a1").unwrap();
        assert!(s.sync.folder.is_empty());
    }

    #[test]
    fn the_folder_section_lists_offered_folders_and_the_one_in_use() {
        let mut s = Settings::default();
        s.sync.folder = "/home/u/Nextcloud".into();
        let view = Extras {
            folders: vec!["/home/u/Dropbox".into(), "/home/u/Nextcloud".into()],
            ..Extras::default()
        };
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &view);
        assert!(html.contains(r#"class="engine" data-k="sync_folder" data-v="0""#));
        assert!(html.contains(r#"class="engine on" data-k="sync_folder" data-v="1""#));
        assert!(html.contains(r#"data-k="sync_folder" data-v="dialog""#));
        assert!(html.contains(r#"data-k="sync_folder" data-v="forget""#));
        s.sync.folder.clear();
        let none = render(&s, &Bookmarks::default(), &Spaces::default(), &view);
        assert!(!none.contains(r#"data-v="forget""#));
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
    fn boosts_are_listed_with_their_css_escaped() {
        let view = Extras {
            boosts: vec![(
                "wiki.org".into(),
                true,
                "</textarea><script>x()</script>body{}".into(),
            )],
            ..Extras::default()
        };
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &view,
        );
        assert!(html.contains(r#"data-k="boost.dark.wiki.org""#));
        assert!(html.contains(r#"data-k="boost.css.wiki.org""#));
        assert!(html.contains("&lt;/textarea&gt;&lt;script&gt;x()"));
        assert!(!html.contains("<script>x()"));
        assert!(html.contains(r#"id="boost-host""#));
    }

    #[test]
    fn extensions_are_listed_with_their_text_escaped() {
        let view = Extras {
            extensions: vec![ExtItem {
                id: "a".repeat(32),
                name: "<img src=x>".into(),
                version: "1.0".into(),
                description: "<b>d</b>".into(),
                enabled: true,
                options: true,
                permissions: vec!["Leer <todo>".into()],
                problem: false,
            }],
            ..Extras::default()
        };
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &view,
        );
        assert!(html.contains(&format!(r#"data-k="ext.enable.{}""#, "a".repeat(32))));
        assert!(
            html.contains(r#"data-k="ext.options""#) && html.contains(r#"data-k="ext.remove""#)
        );
        assert!(html.contains("&lt;img src=x&gt;") && html.contains("Leer &lt;todo&gt;"));
        assert!(!html.contains("<img src=x>") && !html.contains("<b>d</b>"));
        assert!(html.contains(r#"id="ext-store""#));
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
    fn the_nav_links_to_every_section_and_the_page_shows_one_at_a_time() {
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        for (id, ..) in PAGES.iter() {
            assert!(html.contains(&format!(r##"href="#{id}""##)), "{id}");
            assert!(html.contains(&format!(r#"<section id="{id}""#)), "{id}");
        }
        // Fragments are not navigations the browser accepts, and a long page scrolled with
        // animation shows unpainted tiles: the page shows one section at a time instead.
        assert!(JS.contains("window.showSection"));
        assert!(!JS.contains("scrollIntoView"));
        assert!(!CSS.contains("scroll-behavior"));
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
        for (_, _, id) in SECTIONS.iter() {
            assert!(html.contains(&format!(r#"id="{id}""#)), "{id}");
        }
    }

    #[test]
    fn the_import_page_offers_each_browser_found_and_a_file() {
        let view = Extras {
            import: vec![("Chrome".into(), true), ("Fire<fox>".into(), false)],
            ..Extras::default()
        };
        let html = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &view,
        );
        assert!(html.contains(r#"id="import""#));
        for kind in ["all", "bookmarks", "history", "cookies"] {
            for n in [0, 1] {
                assert!(html.contains(&format!(r#"data-k="import.run" data-v="{n}:{kind}""#)));
            }
        }
        // Chromium's passwords can be read; Firefox's come in an exported file.
        assert!(html.contains(r#"data-v="0:passwords""#));
        assert!(!html.contains(r#"data-v="1:passwords""#));
        assert!(html.contains(r#"data-k="import.file""#));
        assert!(html.contains("Fire&lt;fox&gt;") && !html.contains("Fire<fox>"));
        let none = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(none.contains("No se ha encontrado ningún navegador"));
    }

    #[test]
    fn the_ai_section_is_one_switch_until_it_is_turned_on() {
        let mut extras = Extras::default();
        let off = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &extras,
        );
        assert!(off.contains(r#"data-k="ai.enabled""#));
        assert!(!off.contains(r#"data-k="ai.provider""#));
        assert!(!off.contains(r#"data-k="ai.key.set""#));

        extras.ai.settings.enabled = true;
        extras.ai.settings.blocked = vec!["<b>banco</b>.es".into()];
        let on = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &extras,
        );
        for k in [
            "ai.provider",
            "ai.model",
            "ai.key.set",
            "ai.test",
            "ai.read",
            "ai.confirm_reads",
            "ai.open",
            "ai.interact",
            "ai.search",
            "ai.blocked.add",
        ] {
            assert!(on.contains(&format!(r#"data-k="{k}""#)), "{k}");
        }
        // Not asked for: the address of a cloud service, a key to remove.
        assert!(!on.contains(r#"data-k="ai.base_url""#));
        assert!(!on.contains(r#"data-k="ai.key.clear""#));
        assert!(!on.contains("<b>banco</b>"));
        assert!(on.contains("Se envía a OpenAI"));

        extras.ai.key_set = true;
        extras.ai.settings.provider = crate::ai::Provider::Ollama;
        let local = render(
            &Settings::default(),
            &Bookmarks::default(),
            &Spaces::default(),
            &extras,
        );
        assert!(local.contains(r#"data-k="ai.base_url""#));
        assert!(local.contains("Todo se queda en este equipo"));
    }

    #[test]
    fn the_passwords_page_lists_sites_and_users_but_never_a_password() {
        let s = Settings::default();
        let mut extras = Extras::default();
        let closed = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(closed.contains(r#"data-k="pw.create""#));
        extras.vault = VaultView {
            exists: true,
            open: true,
            logins: vec![
                (7, "https://a.example".into(), "ana<b>".into()),
                (8, "https://b.example".into(), "beto".into()),
            ],
            totp: vec![8],
            never: vec!["https://n.example".into()],
        };
        let open = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(open.contains(r#"data-k="pw.copy" data-v="7""#));
        // The login with a secret for codes shows them; the other offers to add one.
        assert!(open.contains(r#"data-k="pw.totp_show" data-v="8""#));
        assert!(open.contains(r#"data-k="pw.totp_remove" data-v="8""#));
        assert!(open.contains(r#"data-k="pw.totp_add" data-v="7""#));
        assert!(!open.contains(r#"data-k="pw.totp_add" data-v="8""#));
        assert!(open.contains("ana&lt;b&gt;"));
        assert!(open.contains(r#"data-k="pw.unnever""#));
        extras.vault.open = false;
        let locked = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(locked.contains(r#"data-k="pw.unlock""#));
        assert!(!locked.contains("https://a.example"));
        let mut s = s;
        s.apply("passwords_enabled", "0").unwrap();
        assert!(!s.passwords_enabled);
    }

    #[test]
    fn passkeys_are_on_by_default_and_the_page_has_a_switch() {
        let mut s = Settings::default();
        assert!(s.passkeys_enabled);
        let page = render(
            &s,
            &Bookmarks::default(),
            &Spaces::default(),
            &Extras::default(),
        );
        assert!(page.contains(r#"data-k="passkeys_enabled""#));
        s.apply("passkeys_enabled", "0").unwrap();
        assert!(!s.passkeys_enabled);
        assert!(s.apply("passkeys_enabled", "quizá").is_err());
        // Settings saved before passkeys existed keep them on.
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(old.passkeys_enabled);
    }

    #[test]
    fn the_web_apps_page_lists_the_apps_and_has_a_form() {
        let s = Settings::default();
        let mut extras = Extras::default();
        let empty = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(empty.contains(r#"data-addwebapp="1""#));
        extras.webapps = vec![(
            "mi-correo".into(),
            "Mi <correo>".into(),
            "https://mail.example/".into(),
        )];
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(html.contains(r#"data-k="webapp.open" data-v="mi-correo""#));
        assert!(html.contains("Mi &lt;correo&gt;"));
    }

    #[test]
    fn the_essentials_page_lists_them_and_offers_the_usual_sites() {
        let s = Settings::default();
        let mut extras = Extras::default();
        let empty = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(empty.contains(r#"data-addessential="1""#));
        assert!(empty.contains(r#"data-k="essential.suggest" data-v="https://www.youtube.com/""#));
        extras.essentials = vec![(3, "Tu <vídeo>".into(), "https://www.youtube.com/".into())];
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(html.contains(r#"data-k="essential.remove" data-v="3""#));
        assert!(html.contains("Tu &lt;vídeo&gt;"));
        assert!(
            !html.contains(r#"data-k="essential.suggest" data-v="https://www.youtube.com/""#),
            "a site that is an essential is not offered again"
        );
    }

    #[test]
    fn the_profiles_page_lists_them_and_never_offers_to_remove_the_one_in_use() {
        let s = Settings::default();
        let mut extras = Extras {
            profile: "p1".into(),
            ..Extras::default()
        };
        let empty = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(empty.contains(r#"data-addprofile="1""#));
        assert!(empty.contains("No cifra los archivos"));
        extras.profiles = vec![
            ("default".into(), "Ana <x>".into(), "moon".into(), true),
            ("p1".into(), "Luis".into(), "sun".into(), false),
            ("p2".into(), "Eva".into(), "leaf".into(), false),
        ];
        let html = render(&s, &Bookmarks::default(), &Spaces::default(), &extras);
        assert!(html.contains("Ana &lt;x&gt;"));
        // The main profile and the one in use cannot be removed; the other can.
        assert!(!html.contains(r#"data-k="profile.remove" data-v="default""#));
        assert!(!html.contains(r#"data-k="profile.remove" data-v="p1""#));
        assert!(html.contains(r#"data-k="profile.remove" data-v="p2""#));
        // The one in use is not opened from itself.
        assert!(!html.contains(r#"data-k="profile.open" data-v="p1""#));
        assert!(html.contains(r#"data-k="profile.open" data-v="default""#));
        assert!(html.contains("Con contraseña"));
    }
}
