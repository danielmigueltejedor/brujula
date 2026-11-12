//! The preferences page (`brujula://settings`): the HTML, and how a change made on it is
//! checked and stored.
//!
//! The page changes settings by navigating to `brujula://set?key=value`, which the browser
//! intercepts (and only honours when it comes from this page). Every value is validated in
//! [`Settings::apply`], so the page, a hand-typed address or a crafted link can only ever set
//! something inside the allowed range.

use crate::adblock::DEFAULT_LISTS;
use crate::newtab::escape;
use crate::search::ENGINES;
use crate::settings::{
    parse_hex_color, Layout, Settings, TabPosition, Theme, SIDEBAR_WIDTH_DEFAULT,
};

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
                a.sidebar_width = number(value)?.clamp(200, 480);
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

fn seg(key: &str, current: &str, options: &[(&str, &str)]) -> String {
    let buttons: String = options
        .iter()
        .map(|(value, label)| {
            let on = if *value == current { " on" } else { "" };
            format!(
                r#"<button type="button" class="seg{on}" data-k="{key}" data-v="{value}">{label}</button>"#,
                label = escape(label)
            )
        })
        .collect();
    format!(r#"<div class="segs" role="group">{buttons}</div>"#)
}

fn switch(key: &str, on: bool) -> String {
    format!(
        r#"<button type="button" class="switch{cls}" role="switch" aria-checked="{on}" data-k="{key}" data-toggle="1"><i></i></button>"#,
        cls = if on { " on" } else { "" },
    )
}

fn slider(key: &str, value: u32, min: u32, max: u32, unit: &str) -> String {
    format!(
        r#"<div class="slider"><input type="range" data-k="{key}" min="{min}" max="{max}" value="{value}"><output>{value}{unit}</output></div>"#
    )
}

fn row(title: &str, hint: &str, control: &str) -> String {
    let hint = if hint.is_empty() {
        String::new()
    } else {
        format!(r#"<p>{}</p>"#, escape(hint))
    };
    format!(
        r#"<div class="row"><div class="what"><b>{}</b>{hint}</div><div class="ctl">{control}</div></div>"#,
        escape(title)
    )
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

fn section(id: &str, title: &str, body: &str) -> String {
    format!(
        r#"<section id="{id}"><h2>{}</h2><div class="card">{body}</div></section>"#,
        escape(title)
    )
}

/// The preferences page for the current settings.
pub fn render(s: &Settings) -> String {
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
                r#"<button type="button" class="seg{on}" data-k="engine" data-v="{}">{}</button>"#,
                e.id,
                escape(e.name)
            )
        })
        .collect();

    let general = section(
        "general",
        "General",
        &row(
            "Buscador",
            "Se usa en la barra de direcciones, en el Spotlight y en la página de nueva pestaña.",
            &format!(r#"<div class="segs wrap" role="group">{engines}</div>"#),
        ),
    );

    let appearance = section(
        "appearance",
        "Apariencia",
        &[
            row(
                "Tema",
                "",
                &seg(
                    "theme",
                    theme,
                    &[("system", "Sistema"), ("light", "Claro"), ("dark", "Oscuro")],
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
            row(
                "Color del marco",
                "Lo que rodea a la web. Vacío usa el del tema.",
                &format!(
                    r##"{}<input type="color" data-k="frame_color" value="{}" aria-label="Color personalizado"><button type="button" class="seg" data-k="frame_color" data-v="">Tema</button>"##,
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
                r#"<button type="button" class="seg" data-k="reset_appearance" data-v="1">Restablecer apariencia</button>"#,
            ),
        ]
        .concat(),
    );

    let layout_section = section(
        "layout",
        "Diseño",
        &[
            row(
                "Disposición",
                "Islas: barra lateral y web son dos paneles. Marco: solo la web es un panel y la barra queda rodeándola, como Zen.",
                &seg(
                    "layout",
                    layout,
                    &[("islands", "Islas"), ("frame", "Marco")],
                ),
            ),
            row(
                "Pestañas",
                "En la barra lateral o en una barra encima de la web.",
                &seg(
                    "tabs",
                    tabs,
                    &[("vertical", "Verticales"), ("horizontal", "Horizontales")],
                ),
            ),
            row(
                "Ancho de la barra lateral",
                &format!("Por defecto {SIDEBAR_WIDTH_DEFAULT} px; se recuerda."),
                &slider("sidebar_width", a.sidebar_width, 200, 480, " px"),
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
                    r#"<div class="row site"><div class="what"><b>{h}</b></div><div class="ctl"><button type="button" class="seg" data-k="allow.remove" data-v="{h}" data-remove="1">Quitar</button></div></div>"#,
                    h = escape(h)
                )
            })
            .collect()
    };
    let privacy = section(
        "privacy",
        "Privacidad y seguridad",
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
    let sites = section("sites", "Sitios con el bloqueador desactivado", &allowed);
    let security = section(
        "security",
        "Protecciones activas",
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
        "Acerca de",
        &row(
            "Brújula",
            "El navegador de Nodalix. WebKit, bloqueador integrado, pestañas verticales y el lenguaje de diseño NDL.",
            "",
        ),
    );

    format!(
        r##"<!doctype html>
<html lang="es" data-theme="{theme}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Preferencias</title>
<style>{css}</style></head>
<body style="--accent:{accent};--on-accent:{on_accent}">
<div class="shell">
<nav class="nav" aria-label="Secciones"><h1>Preferencias</h1>
<a href="#general">General</a><a href="#appearance">Apariencia</a><a href="#layout">Diseño</a>
<a href="#privacy">Privacidad</a><a href="#sites">Sitios</a><a href="#security">Seguridad</a><a href="#about">Acerca de</a></nav>
<main>{general}{appearance}{layout_section}{privacy}{sites}{security}{about}</main>
</div>
<script>{js}</script>
</body></html>
"##,
        css = CSS,
        js = JS,
        theme = match a.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        },
        accent = escape(&a.accent),
        on_accent = crate::settings::text_on(&a.accent),
        general = general,
        appearance = appearance,
        layout_section = layout_section,
        privacy = privacy,
        sites = sites,
        security = security,
        about = about,
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
        "Bloqueador de anuncios",
        "adblock anuncios rastreadores listas",
        "privacy",
    ),
    ("Sitios sin bloqueador", "excepciones permitidos", "sites"),
    ("Seguridad", "descargas permisos certificados", "security"),
];

/// The heading of a section, as the page shows it.
pub fn section_title(id: &str) -> &'static str {
    match id {
        "general" => "General",
        "appearance" => "Apariencia",
        "layout" => "Diseño",
        "privacy" => "Privacidad",
        "sites" => "Sitios",
        "security" => "Seguridad",
        "about" => "Acerca de",
        _ => "Preferencias",
    }
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f4f5f7;--fg:#14181b;--muted:#5b6770;--card:#fff;--line:#e1e5ea;--chip:#eef0f3}
@media (prefers-color-scheme:dark){:root{--bg:#101315;--fg:#eef1f3;--muted:#9aa7b0;--card:#181d20;--line:#272e33;--chip:#222a2e}}
:root[data-theme=light]{color-scheme:light;--bg:#f4f5f7;--fg:#14181b;--muted:#5b6770;--card:#fff;--line:#e1e5ea;--chip:#eef0f3}
:root[data-theme=dark]{color-scheme:dark;--bg:#101315;--fg:#eef1f3;--muted:#9aa7b0;--card:#181d20;--line:#272e33;--chip:#222a2e}
*{box-sizing:border-box}
html{scroll-behavior:smooth}
body{margin:0;font-family:system-ui,sans-serif;background:var(--bg);color:var(--fg);line-height:1.45}
.shell{display:grid;grid-template-columns:14rem 1fr;gap:2rem;max-width:62rem;margin:0 auto;padding:2rem 1.5rem 6rem}
.nav{position:sticky;top:2rem;align-self:start;display:flex;flex-direction:column;gap:.15rem}
.nav h1{font-size:1.5rem;font-weight:650;letter-spacing:-.02em;margin:0 0 1rem}
.nav a{color:var(--muted);text-decoration:none;padding:.5rem .75rem;border-radius:.6rem;font-size:.95rem}
.nav a:hover,.nav a:focus-visible{background:var(--chip);color:var(--fg);outline:none}
section{margin-bottom:2rem;scroll-margin-top:1.5rem}
h2{font-size:.8rem;text-transform:uppercase;letter-spacing:.08em;color:var(--muted);margin:0 0 .6rem .25rem;font-weight:600}
.card{background:var(--card);border:1px solid var(--line);border-radius:1rem;overflow:hidden}
.row{display:flex;gap:1.5rem;justify-content:space-between;align-items:center;padding:.9rem 1.1rem;border-top:1px solid var(--line)}
.row:first-child{border-top:0}
.what{min-width:0}.what b{font-weight:550}.what p{margin:.15rem 0 0;color:var(--muted);font-size:.85rem;max-width:30rem}
.ctl{display:flex;gap:.6rem;align-items:center;flex-wrap:wrap;justify-content:flex-end}
.segs{display:flex;gap:.25rem;background:var(--chip);padding:.2rem;border-radius:.8rem}.segs.wrap{flex-wrap:wrap;max-width:24rem}
.seg{font:inherit;font-size:.9rem;color:var(--fg);background:transparent;border:0;border-radius:.6rem;padding:.4rem .85rem;cursor:pointer}
.seg:hover{background:color-mix(in srgb,var(--fg) 8%,transparent)}
.seg.on{background:var(--accent);color:var(--on-accent)}
.ctl>.seg{background:var(--chip)}
.switch{position:relative;width:2.6rem;height:1.5rem;border-radius:999px;border:0;background:color-mix(in srgb,var(--fg) 22%,transparent);cursor:pointer;padding:0;transition:background .15s}
.switch i{position:absolute;top:.2rem;left:.2rem;width:1.1rem;height:1.1rem;border-radius:50%;background:#fff;transition:transform .15s}
.switch.on{background:var(--accent)}.switch.on i{transform:translateX(1.1rem)}
.dots{display:flex;gap:.4rem;flex-wrap:wrap}
.dot{width:1.6rem;height:1.6rem;border-radius:50%;border:2px solid transparent;cursor:pointer;box-shadow:inset 0 0 0 1px rgba(0,0,0,.18)}
.dot.on{border-color:var(--fg)}
input[type=color]{width:2rem;height:2rem;padding:0;border:0;background:none;cursor:pointer}
.slider{display:flex;gap:.8rem;align-items:center}
input[type=range]{width:12rem;accent-color:var(--accent)}
output{min-width:3.5rem;text-align:right;color:var(--muted);font-variant-numeric:tabular-nums}
.facts{margin:0;padding:1rem 1.1rem 1rem 2.4rem;color:var(--muted);font-size:.92rem}.facts li{margin:.3rem 0}
code{background:var(--chip);padding:.05rem .35rem;border-radius:.3rem;font-size:.85em}
.empty{margin:0;padding:1rem 1.1rem;color:var(--muted)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
@media (max-width:760px){.shell{grid-template-columns:1fr}.nav{position:static;flex-direction:row;flex-wrap:wrap}.nav h1{width:100%}}
"#;

const JS: &str = r#"
const send=(k,v)=>{location.href='brujula://set?'+encodeURIComponent(k)+'='+encodeURIComponent(v)};
let timer;
const later=(f)=>{clearTimeout(timer);timer=setTimeout(f,90)};
const unit=(el)=>el.parentElement.querySelector('output');
document.addEventListener('click',e=>{
  const b=e.target.closest('[data-k]');if(!b||b.tagName==='INPUT')return;
  const k=b.dataset.k;
  if(b.dataset.toggle){const on=!b.classList.contains('on');b.classList.toggle('on',on);b.setAttribute('aria-checked',on);send(k,on?'1':'0');return}
  const v=b.dataset.v;
  if(b.dataset.remove){b.closest('.row').remove();send(k,v);return}
  send(k,v);
  if(k==='accent'){document.body.style.setProperty('--accent',v)}
  const group=b.closest('.segs,.dots');
  if(group){group.querySelectorAll('.on').forEach(x=>x.classList.remove('on'));b.classList.add('on')}
  if(k==='reset_appearance'){setTimeout(()=>location.reload(),150)}
});
document.addEventListener('input',e=>{
  const i=e.target;if(!i.dataset||!i.dataset.k)return;
  if(i.type==='range'){const o=unit(i);if(o)o.textContent=i.value+o.textContent.replace(/^[0-9]+/,'');later(()=>send(i.dataset.k,i.value))}
  else if(i.type==='color'){if(i.dataset.k==='accent')document.body.style.setProperty('--accent',i.value);later(()=>send(i.dataset.k,i.value))}
});
"#;

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
        assert_eq!(s.appearance.sidebar_width, 480);
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
        let html = render(&s);
        assert!(html.contains(r#"class="seg on" data-k="theme" data-v="dark""#));
        assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("--accent:#0c9a58"));
    }

    #[test]
    fn every_section_the_spotlight_offers_exists_on_the_page() {
        let html = render(&Settings::default());
        for (_, _, id) in SECTIONS {
            assert!(html.contains(&format!(r#"id="{id}""#)), "{id}");
        }
    }
}
