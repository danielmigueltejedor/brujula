//! The page of what is new, shown once after Brújula was updated. The notes are the ones of the
//! release (`docs/release-<version>.md`), built into the program, so they need no network.

use crate::newtab::escape;

/// The address of the page.
pub const NEWS_URI: &str = "brujula://news";

/// The notes of this version. A new release changes this path (and the test below says so).
const NOTES: &str = include_str!("../../../docs/release-0.5.1.md");

/// Sections of the notes that are not news.
const SKIPPED: &[&str] = &["Instalar"];

/// `**bold**` and `` `code` `` over text that is already escaped.
fn inline(text: &str) -> String {
    let escaped = escape(text);
    let mut out = String::with_capacity(escaped.len() + 16);
    for (i, part) in escaped.split("**").enumerate() {
        if i > 0 {
            out.push_str(if i % 2 == 1 { "<strong>" } else { "</strong>" });
        }
        for (j, piece) in part.split('`').enumerate() {
            if j > 0 {
                out.push_str(if j % 2 == 1 { "<code>" } else { "</code>" });
            }
            out.push_str(piece);
        }
        // A code span left open does not leak into what follows.
        if part.matches('`').count() % 2 == 1 {
            out.push_str("</code>");
        }
    }
    if escaped.matches("**").count() % 2 == 1 {
        out.push_str("</strong>");
    }
    out
}

/// The body of the notes as HTML: headings, paragraphs and bullet lists.
fn body(notes: &str) -> String {
    let mut out = String::new();
    let mut list = false;
    let mut skipping = false;
    for line in notes.lines() {
        let line = line.trim_end();
        if let Some(item) = line.strip_prefix("- ") {
            if skipping {
                continue;
            }
            if !list {
                out.push_str("<ul>");
                list = true;
            }
            out.push_str(&format!("<li>{}</li>", inline(item)));
            continue;
        }
        if list {
            out.push_str("</ul>");
            list = false;
        }
        if let Some(title) = line.strip_prefix("## ") {
            skipping = SKIPPED.contains(&title.trim());
            if !skipping {
                out.push_str(&format!("<h2>{}</h2>", inline(title)));
            }
        } else if line.starts_with('#') || line.is_empty() || skipping {
            // The first heading is the page's own title.
        } else {
            out.push_str(&format!("<p>{}</p>", inline(line)));
        }
    }
    if list {
        out.push_str("</ul>");
    }
    out
}

/// The notes as the small HTML the About dialog takes (`p`, `ul`, `li`): no headings, no
/// formatting marks, and without the sections that are not news.
pub fn notes_html() -> String {
    let mut out = String::new();
    let mut list = false;
    // Whatever comes before the first section is an introduction, not news.
    let mut skipping = true;
    for line in NOTES.lines().map(str::trim_end) {
        if let Some(item) = line.strip_prefix("- ") {
            if !skipping {
                if !list {
                    out.push_str("<ul>");
                    list = true;
                }
                out.push_str(&format!("<li>{}</li>", plain(item)));
            }
            continue;
        }
        if list {
            out.push_str("</ul>");
            list = false;
        }
        if let Some(title) = line.strip_prefix("## ") {
            skipping = SKIPPED.contains(&title.trim());
        } else if !(line.starts_with('#') || line.is_empty() || skipping) {
            out.push_str(&format!("<p>{}</p>", plain(line)));
        }
    }
    if list {
        out.push_str("</ul>");
    }
    out
}

/// A line of the notes without its marks, escaped.
fn plain(text: &str) -> String {
    escape(&text.replace("**", "").replace('`', ""))
}

/// The page, in the theme and accent of the browser.
pub fn render(theme: &str, accent: &str) -> String {
    let theme = match theme {
        "light" | "dark" => theme,
        _ => "system",
    };
    let accent = crate::settings::parse_hex_color(accent)
        .unwrap_or_else(|| crate::settings::ACCENT_DEFAULT.to_owned());
    format!(
        r#"<!doctype html>
<html lang="es" data-theme="{theme}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Novedades</title>
<style>{faces}{css}</style></head>
<body style="--accent:{accent}"><main>
<p class="kicker">Brújula se ha actualizado</p>
<h1>Novedades de la {version}</h1>
{notes}
</main></body></html>
"#,
        faces = crate::fonts::FACES,
        css = CSS,
        version = env!("CARGO_PKG_VERSION"),
        notes = body(NOTES),
    )
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3}
@media (prefers-color-scheme:dark){:root{--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b}}
:root[data-theme=light]{color-scheme:light;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3}
:root[data-theme=dark]{color-scheme:dark;--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;font-family:Inter,"Adwaita Sans",system-ui,sans-serif;background:radial-gradient(70rem 30rem at 50% -10%,color-mix(in srgb,var(--accent) 15%,transparent),transparent),var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased}
main{width:min(42rem,calc(100% - 3rem));margin:0 auto;padding:4rem 0 5rem}
.kicker{margin:0 0 .4rem;color:var(--accent);font-weight:650;font-size:.85rem;letter-spacing:.04em;text-transform:uppercase}
h1{margin:0 0 1.6rem;font-size:2.1rem;letter-spacing:-.02em}
h2{margin:2rem 0 .7rem;font-size:1.15rem}
p{color:var(--muted);line-height:1.55;margin:.5rem 0}
ul{list-style:none;margin:0;padding:0;background:var(--card);border:1px solid var(--line);border-radius:16px;overflow:hidden}
li{padding:.85rem 1.1rem;line-height:1.5;overflow-wrap:anywhere}
li+li{border-top:1px solid var(--line)}
strong{font-weight:650}
code{background:var(--chip);border-radius:6px;padding:.08rem .38rem;font-size:.92em}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_are_those_of_this_version() {
        let title = NOTES.lines().next().unwrap_or_default();
        assert!(
            title.contains(env!("CARGO_PKG_VERSION")),
            "docs/release-<version>.md for {} must exist and be the path in news.rs (it has: {title})",
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn markdown_becomes_escaped_html() {
        let html = body("# T\n\nuna **negrita** y `<code>`\n\n## Novedades\n\n- uno <b>\n- dos\n\n## Instalar\n\n- no\n");
        assert!(html.contains("<p>una <strong>negrita</strong> y <code>&lt;code&gt;</code></p>"));
        assert!(html.contains("<h2>Novedades</h2><ul><li>uno &lt;b&gt;</li><li>dos</li></ul>"));
        assert!(!html.contains("Instalar") && !html.contains("no</li>"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn the_notes_for_the_about_dialog_have_no_marks_or_headings() {
        let html = notes_html();
        assert!(html.contains("<ul><li>"));
        assert!(!html.contains("**") && !html.contains('`') && !html.contains("<h"));
        assert!(
            !html.contains("chmod +x Brujula"),
            "the install section is not news"
        );
    }

    #[test]
    fn unclosed_marks_do_not_leak() {
        assert_eq!(inline("a **b"), "a <strong>b</strong>");
        assert_eq!(inline("a `b"), "a <code>b</code>");
        assert_eq!(inline("sin marcas"), "sin marcas");
    }

    #[test]
    fn the_page_follows_the_browser_theme_and_accent() {
        let page = render("dark", "#112233");
        assert!(page.contains(r#"data-theme="dark""#));
        assert!(page.contains("--accent:#112233"));
        let odd = render("neon", "x\"><script>");
        assert!(odd.contains(r#"data-theme="system""#));
        assert!(!odd.contains("<script>"));
        assert!(page.contains(env!("CARGO_PKG_VERSION")));
    }
}
