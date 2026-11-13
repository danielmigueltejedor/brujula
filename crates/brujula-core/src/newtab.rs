//! The new-tab page, as one self-contained HTML string (no scripts, no network).
//!
//! Searching is a plain form that submits to the chosen engine. Changing the engine is a
//! navigation to a `brujula://` address that the browser intercepts; it never reaches the net.

use crate::search::{self, Engine, ENGINES};

pub const NEWTAB_URI: &str = "brujula://newtab";
pub const SET_ENGINE_PREFIX: &str = "brujula://set-engine/";
pub const CHOOSE_ENGINE_URI: &str = "brujula://choose-engine";

pub struct Strings {
    pub title: &'static str,
    pub placeholder: &'static str,
    pub choose_title: &'static str,
    pub choose_hint: &'static str,
    pub change: &'static str,
    pub bookmarks: &'static str,
    pub no_bookmarks: &'static str,
    pub recent: &'static str,
    /// Heading of the tabs open on other devices; the device's name follows.
    pub elsewhere: &'static str,
}

pub fn strings(lang: &str) -> Strings {
    if lang.to_lowercase().starts_with("es") {
        Strings {
            title: "Nueva pestaña",
            placeholder: "Buscar con",
            choose_title: "Elige tu buscador",
            choose_hint: "Puedes cambiarlo cuando quieras desde esta página.",
            change: "Cambiar buscador",
            bookmarks: "Marcadores",
            no_bookmarks: "Guarda páginas con la estrella de la barra (Ctrl+D) y las tendrás aquí.",
            recent: "Recientes",
            elsewhere: "En",
        }
    } else {
        Strings {
            title: "New tab",
            placeholder: "Search with",
            choose_title: "Choose your search engine",
            choose_hint: "You can change it any time from this page.",
            change: "Change search engine",
            bookmarks: "Bookmarks",
            no_bookmarks: "Save pages with the star in the toolbar (Ctrl+D) and they will be here.",
            recent: "Recent",
            elsewhere: "On",
        }
    }
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A page the new-tab page offers: a bookmark, or a tab that was open or closed lately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    /// `https://…`, or `brujula://tab/<id>` to switch to an open tab. Anything else is dropped.
    pub href: String,
    /// The page's own address (what `href` stands for when it switches to a tab), for the site
    /// name shown under the title.
    pub site: String,
    /// The site's icon as a `data:image/png;base64,…` address, if one was seen.
    pub icon: Option<String>,
    /// A word shown next to a recent tab ("Abierta", "Cerrada").
    pub tag: &'static str,
}

/// Prefix of the address that switches to an open tab.
pub const TAB_PREFIX: &str = "brujula://tab/";

/// The tab id in a `brujula://tab/<id>` address.
pub fn parse_tab(uri: &str) -> Option<u64> {
    uri.strip_prefix(TAB_PREFIX)?
        .trim_end_matches('/')
        .parse()
        .ok()
}

fn link_is_safe(href: &str) -> bool {
    href.starts_with("https://") || href.starts_with("http://") || parse_tab(href).is_some()
}

fn icon_is_safe(icon: &str) -> bool {
    icon.strip_prefix("data:image/png;base64,")
        .is_some_and(|b| {
            !b.is_empty()
                && b.len() < 64 * 1024
                && b.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
        })
}

/// The tabs open on another device, as its last sync left them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTabs {
    pub name: String,
    pub links: Vec<Link>,
}

const MAX_DEVICES: usize = 3;
const MAX_DEVICE_TABS: usize = 5;

/// What the page needs to draw itself.
pub struct Page<'a> {
    pub engine: &'a Engine,
    /// Shows the engine picker instead of the search box (first run, or the user asked).
    pub choosing: bool,
    pub lang: &'a str,
    /// `system`, `light` or `dark`: the user's choice in the preferences.
    pub theme: &'a str,
    pub accent: &'a str,
    /// Hour of the day, 0 to 23, for the greeting.
    pub hour: u32,
    pub bookmarks: &'a [Link],
    pub recent: &'a [Link],
    pub devices: &'a [DeviceTabs],
}

const MAX_BOOKMARKS: usize = 8;
const MAX_RECENT: usize = 9;

fn greeting(hour: u32, spanish: bool) -> &'static str {
    match (hour, spanish) {
        (5..=11, true) => "Buenos días",
        (12..=19, true) => "Buenas tardes",
        (_, true) => "Buenas noches",
        (5..=11, false) => "Good morning",
        (12..=17, false) => "Good afternoon",
        (18..=21, false) => "Good evening",
        (_, false) => "Good night",
    }
}

fn host_of(href: &str) -> String {
    url::Url::parse(href)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_owned())
        })
        .unwrap_or_default()
}

/// The site's icon, or its first letter when none was seen.
fn favicon(link: &Link) -> String {
    match link.icon.as_deref().filter(|i| icon_is_safe(i)) {
        Some(data) => format!(r#"<img src="{data}" alt="" width="20" height="20">"#),
        None => {
            let letter = link
                .title
                .chars()
                .chain(host_of(&link.site).chars())
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_else(|| "·".to_owned());
            format!(r#"<span class="letter">{}</span>"#, escape(&letter))
        }
    }
}

fn title_of(link: &Link) -> String {
    let title = link.title.trim();
    if title.is_empty() {
        host_of(&link.site)
    } else {
        title.to_owned()
    }
}

fn bookmark_tiles(links: &[Link], s: &Strings) -> String {
    let tiles: String = links
        .iter()
        .filter(|l| link_is_safe(&l.href))
        .take(MAX_BOOKMARKS)
        .map(|l| {
            format!(
                r#"<a class="site" href="{}"><span class="fav">{}</span><span class="name">{}</span></a>"#,
                escape(&l.href),
                favicon(l),
                escape(&title_of(l))
            )
        })
        .collect();
    if tiles.is_empty() {
        return format!(
            r#"<section><h2>{}</h2><p class="empty">{}</p></section>"#,
            escape(s.bookmarks),
            escape(s.no_bookmarks)
        );
    }
    format!(
        r#"<section><h2>{}</h2><div class="sites">{tiles}</div></section>"#,
        escape(s.bookmarks)
    )
}

/// Rows for `links`; `web_only` leaves out the ones that switch to a tab here.
fn link_rows(links: &[Link], max: usize, web_only: bool) -> String {
    links
        .iter()
        .filter(|l| link_is_safe(&l.href) && (!web_only || parse_tab(&l.href).is_none()))
        .take(max)
        .map(|l| {
            let tag = if l.tag.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="tag">{}</span>"#, escape(l.tag))
            };
            format!(
                r#"<li><a href="{}"><span class="fav">{}</span><span class="what"><b>{}</b><small>{}</small></span>{tag}</a></li>"#,
                escape(&l.href),
                favicon(l),
                escape(&title_of(l)),
                escape(&host_of(&l.site)),
            )
        })
        .collect()
}

fn recent_list(links: &[Link], s: &Strings) -> String {
    let rows = link_rows(links, MAX_RECENT, false);
    if rows.is_empty() {
        return String::new();
    }
    format!(
        r#"<section><h2>{}</h2><ul class="recent">{rows}</ul></section>"#,
        escape(s.recent)
    )
}

/// One list per device that has tabs open, headed with the device's name.
fn device_lists(devices: &[DeviceTabs], s: &Strings) -> String {
    devices
        .iter()
        .take(MAX_DEVICES)
        .filter_map(|d| {
            let rows = link_rows(&d.links, MAX_DEVICE_TABS, true);
            (!rows.is_empty()).then(|| {
                format!(
                    r#"<section><h2>{} {}</h2><ul class="recent">{rows}</ul></section>"#,
                    escape(s.elsewhere),
                    escape(&d.name)
                )
            })
        })
        .collect()
}

pub fn render(page: &Page) -> String {
    let s = strings(page.lang);
    let spanish = page.lang.to_lowercase().starts_with("es");
    let body = if page.choosing {
        picker(&s)
    } else {
        format!(
            "{}{}{}{}",
            search_box(page.engine, &s),
            bookmark_tiles(page.bookmarks, &s),
            recent_list(page.recent, &s),
            device_lists(page.devices, &s)
        )
    };
    let theme = match page.theme {
        "light" | "dark" => page.theme,
        _ => "system",
    };
    let accent = crate::settings::parse_hex_color(page.accent)
        .unwrap_or_else(|| crate::settings::ACCENT_DEFAULT.to_owned());
    format!(
        r#"<!doctype html>
<html lang="{lang}" data-theme="{theme}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{css}</style></head>
<body style="--accent:{accent}"><main>
<header><svg class="mark" viewBox="0 0 64 64" aria-hidden="true"><circle cx="32" cy="32" r="28" fill="none" stroke="currentColor" stroke-width="3"/><path d="M32 12l8 20-8 20-8-20z" fill="none" stroke="currentColor" stroke-width="3" stroke-linejoin="round"/><path d="M32 12l8 20h-16z" fill="currentColor"/></svg>
<h1>{greeting}</h1></header>
{body}
</main></body></html>
"#,
        lang = escape(if page.lang.is_empty() {
            "en"
        } else {
            page.lang
        }),
        title = escape(s.title),
        css = CSS,
        greeting = greeting(page.hour, spanish),
    )
}

fn search_box(engine: &Engine, s: &Strings) -> String {
    let Some(parts) = search::form_parts(engine.template) else {
        return String::new();
    };
    let hidden: String = parts
        .hidden
        .iter()
        .map(|(k, v)| {
            format!(
                r#"<input type="hidden" name="{}" value="{}">"#,
                escape(k),
                escape(v)
            )
        })
        .collect();
    format!(
        r#"<form action="{action}" method="get" role="search">{hidden}
<a class="engbtn" href="{choose}" title="{change}" aria-label="{change}">{badge}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 10l5 5 5-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></a>
<input type="search" name="{name}" placeholder="{placeholder} {engine}" aria-label="{placeholder} {engine}" autofocus autocomplete="off">
</form>"#,
        action = escape(&parts.action),
        hidden = hidden,
        name = escape(&parts.query_name),
        placeholder = escape(s.placeholder),
        engine = escape(engine.name),
        choose = CHOOSE_ENGINE_URI,
        change = escape(s.change),
        badge = engine.badge(),
    )
}

fn picker(s: &Strings) -> String {
    let buttons: String = ENGINES
        .iter()
        .map(|e| {
            format!(
                r#"<a class="engine" href="{SET_ENGINE_PREFIX}{}">{}<b>{}</b></a>"#,
                escape(e.id),
                e.badge(),
                escape(e.name)
            )
        })
        .collect();
    format!(
        r#"<section class="choose"><h2>{}</h2><p class="hint">{}</p><nav class="engines">{buttons}</nav></section>"#,
        escape(s.choose_title),
        escape(s.choose_hint),
    )
}

/// The engine id in a `brujula://set-engine/<id>` address, if it is a known engine.
pub fn parse_set_engine(uri: &str) -> Option<&'static Engine> {
    search::by_id(uri.strip_prefix(SET_ENGINE_PREFIX)?.trim_end_matches('/'))
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
@media (prefers-color-scheme:dark){:root{--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}}
:root[data-theme=light]{color-scheme:light;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
:root[data-theme=dark]{color-scheme:dark;--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}
*{box-sizing:border-box}
html,body{min-height:100%;margin:0}
body{font-family:"Adwaita Sans",Inter,system-ui,sans-serif;background:radial-gradient(70rem 36rem at 50% -12%,color-mix(in srgb,var(--accent) 15%,transparent),transparent),var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased}
main{width:min(40rem,calc(100% - 3rem));margin:0 auto;padding:15vh 0 4rem}
header{text-align:center;margin-bottom:2rem}
.mark{width:2.6rem;height:2.6rem;color:var(--accent)}
h1{font-size:1.9rem;font-weight:500;letter-spacing:-.03em;margin:.6rem 0 0;color:var(--fg)}
h2{font-size:.75rem;font-weight:600;letter-spacing:.09em;text-transform:uppercase;color:var(--muted);margin:0 0 .8rem;text-align:center}
section{margin-top:2.6rem}
form{position:relative;display:flex;align-items:center;background:var(--card);border:1px solid var(--line);border-radius:999px;box-shadow:var(--shadow);transition:border-color .15s,box-shadow .15s}
form:focus-within{border-color:var(--accent);box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 26%,transparent),var(--shadow)}
.engbtn{display:flex;align-items:center;gap:.1rem;margin-left:.55rem;padding:.3rem .35rem .3rem .3rem;border-radius:999px;color:var(--muted);text-decoration:none}
.engbtn:hover{background:var(--chip);color:var(--fg)}
.engbtn svg{width:1rem;height:1rem}
.eng svg{width:64%;height:64%;display:block}
.eng{display:grid;place-items:center;flex:none;width:1.9rem;height:1.9rem;border-radius:.6rem;color:#fff;font-weight:750;font-size:1rem}
input[type=search]{flex:1;min-width:0;font:inherit;font-size:1.1rem;color:var(--fg);background:transparent;border:0;padding:1rem 1.4rem 1rem .6rem;outline:none}
input[type=search]::-webkit-search-cancel-button{display:none}
input::placeholder{color:var(--muted)}
.sites{display:flex;flex-wrap:wrap;justify-content:center;gap:.5rem}
.site{display:flex;flex-direction:column;align-items:center;gap:.5rem;width:5.6rem;padding:.8rem .4rem .7rem;border-radius:1rem;color:var(--fg);text-decoration:none;transition:background .15s}
.site:hover,.site:focus-visible{background:var(--card);box-shadow:var(--shadow);outline:none}
.fav{display:grid;place-items:center;flex:none;width:2.7rem;height:2.7rem;border-radius:.9rem;background:var(--card);border:1px solid var(--line)}
.fav img{width:1.5rem;height:1.5rem;object-fit:contain}
.letter{font-weight:650;color:var(--accent);font-size:1.05rem}
.name{max-width:100%;font-size:.82rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.recent{list-style:none;margin:0;padding:.3rem;background:var(--card);border:1px solid var(--line);border-radius:1.1rem;box-shadow:var(--shadow)}
.recent a{display:flex;align-items:center;gap:.85rem;padding:.55rem .65rem;border-radius:.8rem;color:var(--fg);text-decoration:none}
.recent a:hover,.recent a:focus-visible{background:var(--chip);outline:none}
.recent .fav{width:2.2rem;height:2.2rem;border-radius:.7rem;background:var(--chip);border:0}
.recent .fav img{width:1.25rem;height:1.25rem}
.what{flex:1;min-width:0;display:flex;flex-direction:column}
.what b{font-weight:520;font-size:.93rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.what small{color:var(--muted);font-size:.78rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.tag{font-size:.72rem;color:var(--muted);background:var(--chip);padding:.15rem .55rem;border-radius:999px}
.recent a:hover .tag{background:var(--card)}
.empty{margin:0;color:var(--muted);font-size:.9rem;padding:.2rem;text-align:center}
.choose{text-align:center;margin-top:0}
.choose h2{font-size:1.25rem;letter-spacing:0;text-transform:none;color:var(--fg);font-weight:600;margin:0 0 .4rem}
.hint{color:var(--muted);margin:0 0 1.6rem}
.engines{display:flex;flex-wrap:wrap;gap:.7rem;justify-content:center}
.engine{display:flex;align-items:center;gap:.7rem;padding:.65rem 1.1rem .65rem .7rem;border-radius:1rem;background:var(--card);border:1px solid var(--line);color:var(--fg);text-decoration:none;font-weight:500;box-shadow:var(--shadow)}
.engine:hover,.engine:focus-visible{border-color:var(--accent);outline:none}
@media (prefers-reduced-motion:no-preference){.engine,.site{transition:border-color .15s,background .15s,box-shadow .15s}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn page<'a>(engine: &'a Engine, choosing: bool, lang: &'a str) -> Page<'a> {
        Page {
            engine,
            choosing,
            lang,
            theme: "system",
            accent: "#0c9a58",
            hour: 10,
            bookmarks: &[],
            recent: &[],
            devices: &[],
        }
    }

    fn link(title: &str, href: &str) -> Link {
        Link {
            title: title.into(),
            href: href.into(),
            site: "https://site.example/".into(),
            icon: None,
            tag: "Abierta",
        }
    }

    #[test]
    fn search_page_submits_to_the_chosen_engine() {
        let e = search::by_id("bing").unwrap();
        let html = render(&page(e, false, "es"));
        assert!(
            html.contains(r#"action="https://www.bing.com/search""#),
            "{html}"
        );
        assert!(html.contains(r#"name="q""#));
        assert!(html.contains("Buscar con Bing"));
        assert!(html.contains(CHOOSE_ENGINE_URI));
        // The engine shows its badge next to the box.
        assert!(html.contains(&e.badge()));
    }

    #[test]
    fn picker_lists_every_engine_with_its_badge_and_no_search_box() {
        let html = render(&page(search::by_id("google").unwrap(), true, "en"));
        for e in ENGINES {
            assert!(
                html.contains(&format!("{SET_ENGINE_PREFIX}{}", e.id)),
                "{}",
                e.id
            );
            assert!(html.contains(&e.badge()), "{}", e.id);
        }
        assert!(!html.contains("<form"));
        assert!(html.contains("Choose your search engine"));
    }

    #[test]
    fn the_page_runs_no_scripts() {
        let mut p = page(search::by_id("google").unwrap(), false, "es");
        let links = [link("Uno", "https://uno.example/")];
        p.bookmarks = &links;
        p.recent = &links;
        assert!(!render(&p).contains("<script"));
    }

    #[test]
    fn bookmarks_and_recent_tabs_are_listed() {
        let mut p = page(search::by_id("google").unwrap(), false, "es");
        let marks = [link("Rust", "https://www.rust-lang.org/")];
        let recent = [
            Link {
                site: "https://hilo.example/a".into(),
                ..link("Hilo", "brujula://tab/7")
            },
            Link {
                tag: "Cerrada",
                ..link("Viejo", "https://old.example/")
            },
        ];
        p.bookmarks = &marks;
        p.recent = &recent;
        let html = render(&p);
        assert!(html.contains(r#"href="https://www.rust-lang.org/""#));
        assert!(html.contains("rust-lang.org"));
        assert!(html.contains(r#"href="brujula://tab/7""#));
        // The line under a tab's title is its site, not the address that switches to it.
        assert!(html.contains("<small>hilo.example</small>"));
        assert!(html.contains("Cerrada"));
        assert!(html.contains("Recientes"));
    }

    #[test]
    fn tabs_of_other_devices_are_listed_under_the_device_name_and_only_as_web_pages() {
        let mut p = page(search::by_id("google").unwrap(), false, "es");
        let devices = [
            DeviceTabs {
                name: "<Portátil>".into(),
                links: vec![
                    Link {
                        tag: "",
                        ..link("Lejos", "https://lejos.example/")
                    },
                    link("Local", "brujula://tab/3"),
                ],
            },
            DeviceTabs {
                name: "Vacío".into(),
                links: Vec::new(),
            },
        ];
        p.devices = &devices;
        let html = render(&p);
        assert!(html.contains("En &lt;Portátil&gt;"));
        assert!(html.contains(r#"href="https://lejos.example/""#));
        assert!(!html.contains("brujula://tab/3"));
        assert!(!html.contains("Vacío"));
        assert!(!html.contains(r#"class="tag""#));
    }

    #[test]
    fn an_empty_page_explains_how_to_get_bookmarks_and_hides_recents() {
        let html = render(&page(search::by_id("google").unwrap(), false, "es"));
        assert!(html.contains("Ctrl+D"));
        assert!(!html.contains("Recientes"));
    }

    #[test]
    fn only_safe_addresses_and_icons_reach_the_page() {
        let mut p = page(search::by_id("google").unwrap(), false, "es");
        let evil = [
            link("a", "javascript:alert(1)"),
            link("b", "file:///etc/passwd"),
            link("c", "data:text/html,x"),
            link("d", "brujula://set?theme=dark"),
            Link {
                icon: Some("data:image/svg+xml;base64,AAAA".into()),
                ..link("e", "https://e.example/")
            },
            Link {
                icon: Some("data:image/png;base64,AA\" onerror=\"x".into()),
                ..link("f", "https://f.example/")
            },
        ];
        p.bookmarks = &evil;
        p.recent = &evil;
        let html = render(&p);
        for bad in [
            "javascript:",
            "file:",
            "data:text",
            "brujula://set",
            "svg+xml",
            "onerror",
        ] {
            assert!(!html.contains(bad), "{bad}");
        }
        assert!(html.contains("e.example"));
        // A site without an icon gets its first letter.
        assert!(html.contains(r#"<span class="letter">E</span>"#));
    }

    #[test]
    fn titles_and_hosts_are_escaped() {
        let mut p = page(search::by_id("google").unwrap(), false, "es");
        let marks = [link("<img src=x onerror=y>", "https://a.example/?q=\"1\"")];
        p.bookmarks = &marks;
        let html = render(&p);
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img src=x onerror=y&gt;"));
        assert!(!html.contains("q=\"1\""));
    }

    #[test]
    fn the_greeting_follows_the_hour_and_the_language() {
        let e = search::by_id("google").unwrap();
        let mut p = page(e, false, "es");
        assert!(render(&p).contains("Buenos días"));
        p.hour = 15;
        assert!(render(&p).contains("Buenas tardes"));
        p.hour = 2;
        p.lang = "en";
        assert!(render(&p).contains("Good night"));
    }

    #[test]
    fn the_theme_and_accent_come_from_the_settings_and_are_checked() {
        let e = search::by_id("google").unwrap();
        let mut p = page(e, false, "es");
        p.theme = "dark";
        p.accent = "#2563eb";
        let html = render(&p);
        assert!(html.contains(r#"data-theme="dark""#) && html.contains("--accent:#2563eb"));
        p.theme = "x\"><script>";
        p.accent = "red;background:url(x)";
        let html = render(&p);
        assert!(html.contains(r#"data-theme="system""#));
        assert!(html.contains("--accent:#0c9a58") && !html.contains("url(x)"));
    }

    #[test]
    fn tab_and_engine_addresses_are_parsed_strictly() {
        assert_eq!(parse_tab("brujula://tab/12"), Some(12));
        assert_eq!(parse_tab("brujula://tab/x"), None);
        assert_eq!(parse_tab("https://tab/12"), None);
        assert_eq!(
            parse_set_engine("brujula://set-engine/duckduckgo")
                .unwrap()
                .id,
            "duckduckgo"
        );
        assert!(parse_set_engine("brujula://set-engine/evil").is_none());
        assert!(parse_set_engine("https://set-engine/google").is_none());
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        // A lang attribute from the environment must not break out of the attribute.
        let html = render(&page(
            search::by_id("google").unwrap(),
            false,
            r#"es"><script>x</script>"#,
        ));
        assert!(!html.contains("<script>x"));
    }
}
