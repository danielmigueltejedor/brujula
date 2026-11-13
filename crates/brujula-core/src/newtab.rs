//! The page a new tab shows behind the Spotlight: a greeting and nothing to click. Searching and
//! the bookmarks are in the Spotlight, which opens over it. The page also holds the search-engine
//! picker of the first run, which is a plain list of `brujula://` addresses that the browser
//! intercepts; none of it reaches the net, and it runs no scripts.

use crate::search::{self, Engine, ENGINES};

pub const NEWTAB_URI: &str = "brujula://newtab";
pub const SET_ENGINE_PREFIX: &str = "brujula://set-engine/";
pub const CHOOSE_ENGINE_URI: &str = "brujula://choose-engine";

pub struct Strings {
    pub title: &'static str,
    pub choose_title: &'static str,
    pub choose_hint: &'static str,
    pub search_hint: &'static str,
}

pub fn strings(lang: &str) -> Strings {
    if lang.to_lowercase().starts_with("es") {
        Strings {
            title: "Nueva pestaña",
            choose_title: "Elige tu buscador",
            choose_hint: "Puedes cambiarlo cuando quieras en los ajustes.",
            search_hint: "Pulsa Ctrl+L para buscar o ir a una página.",
        }
    } else {
        Strings {
            title: "New tab",
            choose_title: "Choose your search engine",
            choose_hint: "You can change it any time in the settings.",
            search_hint: "Press Ctrl+L to search or go to a page.",
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

/// What the page needs to draw itself.
pub struct Page<'a> {
    /// Shows the engine picker (first run, or the user asked) instead of the hint.
    pub choosing: bool,
    pub lang: &'a str,
    /// `system`, `light` or `dark`: the user's choice in the preferences.
    pub theme: &'a str,
    pub accent: &'a str,
    /// Hour of the day, 0 to 23, for the greeting.
    pub hour: u32,
}

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

pub fn render(page: &Page) -> String {
    let s = strings(page.lang);
    let spanish = page.lang.to_lowercase().starts_with("es");
    let body = if page.choosing {
        picker(&s)
    } else {
        format!(r#"<p class="hint">{}</p>"#, escape(s.search_hint))
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
<style>{faces}{css}</style></head>
<body style="--accent:{accent}"><main>
<header>{mascot}
<h1>{greeting}</h1></header>
{body}
</main></body></html>
"#,
        mascot = MASCOT,
        lang = escape(if page.lang.is_empty() {
            "en"
        } else {
            page.lang
        }),
        title = escape(s.title),
        faces = crate::fonts::FACES,
        css = CSS,
        greeting = greeting(page.hour, spanish),
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

/// The browser's mascot, Elcano, the navigator of the app icon, in a compass dial (see `mascot.svg`).
pub(crate) const MASCOT: &str = include_str!("mascot.svg");

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
@media (prefers-color-scheme:dark){:root{--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}}
:root[data-theme=light]{color-scheme:light;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
:root[data-theme=dark]{color-scheme:dark;--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}
*{box-sizing:border-box}
html,body{min-height:100%;margin:0}
body{font-family:Inter,"Adwaita Sans",system-ui,sans-serif;background:radial-gradient(70rem 36rem at 50% -12%,color-mix(in srgb,var(--accent) 15%,transparent),transparent),var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased}
main{width:min(40rem,calc(100% - 3rem));margin:0 auto;padding:15vh 0 4rem}
header{text-align:center;margin-bottom:2rem}
header h1{margin-top:.4rem}
.mascot{width:8.6rem;height:8.6rem;overflow:visible}
.mascot .bob{animation:bob 5s ease-in-out infinite;transform-origin:60px 120px}
@keyframes bob{0%,100%{transform:translateY(0)}50%{transform:translateY(-2px)}}
@media (prefers-reduced-motion:reduce){.mascot .bob{animation:none}}
h1{font-size:1.9rem;font-weight:500;letter-spacing:-.03em;margin:.6rem 0 0;color:var(--fg)}
h2{font-size:.75rem;font-weight:600;letter-spacing:.09em;text-transform:uppercase;color:var(--muted);margin:0 0 .8rem;text-align:center}
.eng svg{width:64%;height:64%;display:block}
.eng{display:grid;place-items:center;flex:none;width:1.9rem;height:1.9rem;border-radius:.6rem;color:#fff;font-weight:750;font-size:1rem}
.choose{text-align:center;margin-top:0}
.choose h2{font-size:1.25rem;letter-spacing:0;text-transform:none;color:var(--fg);font-weight:600;margin:0 0 .4rem}
.hint{color:var(--muted);margin:0 0 1.6rem}
.engines{display:flex;flex-wrap:wrap;gap:.7rem;justify-content:center}
.engine{display:flex;align-items:center;gap:.7rem;padding:.65rem 1.1rem .65rem .7rem;border-radius:1rem;background:var(--card);border:1px solid var(--line);color:var(--fg);text-decoration:none;font-weight:500;box-shadow:var(--shadow)}
.engine:hover,.engine:focus-visible{border-color:var(--accent);outline:none}
@media (prefers-reduced-motion:no-preference){.engine{transition:border-color .15s,background .15s,box-shadow .15s}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn page(choosing: bool, lang: &str) -> Page<'_> {
        Page {
            choosing,
            lang,
            theme: "system",
            accent: "#0c9a58",
            hour: 10,
        }
    }

    #[test]
    fn the_page_has_no_search_box_and_runs_no_scripts() {
        let html = render(&page(false, "es"));
        assert!(!html.contains("<form") && !html.contains("<input"));
        assert!(!html.contains("<script"));
        assert!(html.contains("Ctrl+L"));
    }

    #[test]
    fn picker_lists_every_engine_with_its_badge() {
        let html = render(&page(true, "en"));
        for e in ENGINES {
            assert!(
                html.contains(&format!("{SET_ENGINE_PREFIX}{}", e.id)),
                "{}",
                e.id
            );
            assert!(html.contains(&e.badge()), "{}", e.id);
        }
        assert!(html.contains("Choose your search engine"));
        assert!(!html.contains("Ctrl+L"));
    }

    #[test]
    fn the_greeting_follows_the_hour_and_the_language() {
        let mut p = page(false, "es");
        assert!(render(&p).contains("Buenos días"));
        p.hour = 15;
        assert!(render(&p).contains("Buenas tardes"));
        p.hour = 2;
        p.lang = "en";
        assert!(render(&p).contains("Good night"));
    }

    #[test]
    fn the_theme_and_accent_come_from_the_settings_and_are_checked() {
        let mut p = page(false, "es");
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
    fn engine_addresses_are_parsed_strictly() {
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
        let html = render(&page(false, r#"es"><script>x</script>"#));
        assert!(!html.contains("<script>x"));
    }
}
