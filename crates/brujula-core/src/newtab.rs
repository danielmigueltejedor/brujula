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
}

pub fn strings(lang: &str) -> Strings {
    if lang.to_lowercase().starts_with("es") {
        Strings {
            title: "Nueva pestaña",
            placeholder: "Buscar con",
            choose_title: "Elige tu buscador",
            choose_hint: "Puedes cambiarlo cuando quieras desde esta página.",
            change: "Cambiar buscador",
        }
    } else {
        Strings {
            title: "New tab",
            placeholder: "Search with",
            choose_title: "Choose your search engine",
            choose_hint: "You can change it any time from this page.",
            change: "Change search engine",
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

/// `choosing` shows the engine picker instead of the search box (first run, or the user asked).
pub fn render(engine: &Engine, choosing: bool, lang: &str) -> String {
    let s = strings(lang);
    let body = if choosing {
        picker(&s)
    } else {
        search_box(engine, &s)
    };
    format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{css}</style></head>
<body><main>
<svg class="mark" viewBox="0 0 64 64" aria-hidden="true"><circle cx="32" cy="32" r="28" fill="none" stroke="currentColor" stroke-width="3"/><path d="M32 12l8 20-8 20-8-20z" fill="none" stroke="currentColor" stroke-width="3" stroke-linejoin="round"/><path d="M32 12l8 20h-16z" fill="currentColor"/></svg>
<h1>Brújula</h1>
{body}
</main></body></html>
"#,
        lang = escape(if lang.is_empty() { "en" } else { lang }),
        title = escape(s.title),
        css = CSS,
        body = body,
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
<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" stroke-width="2"/><path d="M16 16l5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
<input type="search" name="{name}" placeholder="{placeholder} {engine}" aria-label="{placeholder} {engine}" autofocus autocomplete="off">
</form>
<a class="change" href="{choose}">{change}</a>"#,
        action = escape(&parts.action),
        hidden = hidden,
        name = escape(&parts.query_name),
        placeholder = escape(s.placeholder),
        engine = escape(engine.name),
        choose = CHOOSE_ENGINE_URI,
        change = escape(s.change),
    )
}

fn picker(s: &Strings) -> String {
    let buttons: String = ENGINES
        .iter()
        .map(|e| {
            format!(
                r#"<a class="engine" href="{SET_ENGINE_PREFIX}{}">{}</a>"#,
                escape(e.id),
                escape(e.name)
            )
        })
        .collect();
    format!(
        r#"<h2>{}</h2><p class="hint">{}</p><nav class="engines">{buttons}</nav>"#,
        escape(s.choose_title),
        escape(s.choose_hint),
    )
}

/// The engine id in a `brujula://set-engine/<id>` address, if it is a known engine.
pub fn parse_set_engine(uri: &str) -> Option<&'static Engine> {
    search::by_id(uri.strip_prefix(SET_ENGINE_PREFIX)?.trim_end_matches('/'))
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f6f7f8;--fg:#14181b;--muted:#5b6770;--card:#fff;--line:#dfe4e8;--accent:#0c9a58}
@media (prefers-color-scheme:dark){:root{--bg:#101315;--fg:#eef1f3;--muted:#9aa7b0;--card:#1a1f22;--line:#2a3237;--accent:#2fcf86}}
*{box-sizing:border-box}
html,body{height:100%;margin:0}
body{font-family:system-ui,sans-serif;background:radial-gradient(60rem 40rem at 50% -10%,color-mix(in srgb,var(--accent) 16%,transparent),transparent),var(--bg);color:var(--fg);display:grid;place-items:center}
main{width:min(36rem,calc(100% - 3rem));text-align:center;margin-top:-6vh}
.mark{width:4rem;height:4rem;color:var(--accent)}
h1{font-size:2.75rem;font-weight:600;letter-spacing:-.04em;margin:.5rem 0 2rem}
h2{font-size:1.25rem;font-weight:600;margin:0 0 .5rem}
form{position:relative}
form svg{position:absolute;left:1.1rem;top:50%;width:1.25rem;height:1.25rem;transform:translateY(-50%);color:var(--muted);pointer-events:none}
input[type=search]{width:100%;font:inherit;font-size:1.1rem;color:var(--fg);background:var(--card);border:1px solid var(--line);border-radius:999px;padding:1rem 1.25rem 1rem 3.25rem;outline:none;box-shadow:0 1px 2px rgba(0,0,0,.06),0 8px 24px rgba(0,0,0,.06)}
input[type=search]:focus{border-color:var(--accent);box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 28%,transparent)}
input::placeholder{color:var(--muted)}
.change{display:inline-block;margin-top:1.25rem;color:var(--muted);font-size:.9rem;text-decoration:none}
.change:hover{color:var(--fg)}
.hint{color:var(--muted);margin:0 0 1.5rem}
.engines{display:flex;flex-wrap:wrap;gap:.75rem;justify-content:center}
.engine{padding:.75rem 1.25rem;border-radius:999px;background:var(--card);border:1px solid var(--line);color:var(--fg);text-decoration:none;font-weight:500}
.engine:hover,.engine:focus-visible{border-color:var(--accent);outline:none}
@media (prefers-reduced-motion:no-preference){.engine,input{transition:border-color .15s,box-shadow .15s}}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_page_submits_to_the_chosen_engine() {
        let e = search::by_id("bing").unwrap();
        let html = render(e, false, "es");
        assert!(
            html.contains(r#"action="https://www.bing.com/search""#),
            "{html}"
        );
        assert!(html.contains(r#"name="q""#));
        assert!(html.contains("Buscar con Bing"));
        assert!(html.contains(CHOOSE_ENGINE_URI));
    }

    #[test]
    fn picker_lists_every_engine_and_no_search_box() {
        let html = render(search::by_id("google").unwrap(), true, "en");
        for e in ENGINES {
            assert!(
                html.contains(&format!("{SET_ENGINE_PREFIX}{}", e.id)),
                "{}",
                e.id
            );
        }
        assert!(!html.contains("<form"));
        assert!(html.contains("Choose your search engine"));
    }

    #[test]
    fn the_page_runs_no_scripts() {
        let html = render(search::by_id("google").unwrap(), false, "es");
        assert!(!html.contains("<script"));
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        // A lang attribute from the environment must not break out of the attribute.
        let html = render(
            search::by_id("google").unwrap(),
            false,
            r#"es"><script>x</script>"#,
        );
        assert!(!html.contains("<script>x"));
    }

    #[test]
    fn set_engine_addresses_are_parsed_strictly() {
        assert_eq!(
            parse_set_engine("brujula://set-engine/duckduckgo")
                .unwrap()
                .id,
            "duckduckgo"
        );
        assert!(parse_set_engine("brujula://set-engine/evil").is_none());
        assert!(parse_set_engine("https://set-engine/google").is_none());
    }
}
