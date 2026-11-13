//! The little Markdown models write, turned into Pango markup for a label: bold, code, lists,
//! headings and links. Anything else is shown as typed. Text is always escaped first, so an
//! answer can never inject markup, and only `http`/`https` links are linked.

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// A link the label may carry: web only, no spaces.
fn safe_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 2000
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Plain text with bare addresses linked.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("http") {
        let (head, tail) = rest.split_at(i);
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | ')' | ']'))
            .unwrap_or(tail.len());
        let candidate = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if safe_url(candidate) {
            out.push_str(&escape(head));
            let url = escape(candidate);
            out.push_str(&format!("<a href=\"{url}\">{url}</a>"));
            rest = &tail[candidate.len()..];
        } else {
            out.push_str(&escape(&rest[..i + 4]));
            rest = &rest[i + 4..];
        }
    }
    out.push_str(&escape(rest));
    out
}

/// One line: `**bold**`, `` `code` `` and `[text](url)`.
fn inline(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while !rest.is_empty() {
        // The nearest opener decides what comes next.
        let next = ["**", "`", "["]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, *m)))
            .min_by_key(|(i, _)| *i);
        let Some((i, marker)) = next else {
            out.push_str(&plain(rest));
            break;
        };
        out.push_str(&plain(&rest[..i]));
        let after = &rest[i + marker.len()..];
        match marker {
            "**" => match after.find("**") {
                Some(j) if j > 0 => {
                    out.push_str(&format!("<b>{}</b>", plain(&after[..j])));
                    rest = &after[j + 2..];
                }
                _ => {
                    out.push_str("**");
                    rest = after;
                }
            },
            "`" => match after.find('`') {
                Some(j) if j > 0 => {
                    out.push_str(&format!("<tt>{}</tt>", escape(&after[..j])));
                    rest = &after[j + 1..];
                }
                _ => {
                    out.push('`');
                    rest = after;
                }
            },
            _ => {
                let link = after.find("](").and_then(|j| {
                    let url_end = after[j + 2..].find(')')?;
                    Some((j, j + 2 + url_end))
                });
                match link {
                    Some((j, end))
                        if safe_url(&after[j + 2..end]) && !after[..j].contains('\n') =>
                    {
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape(&after[j + 2..end]),
                            escape(&after[..j])
                        ));
                        rest = &after[end + 1..];
                    }
                    _ => {
                        out.push('[');
                        rest = after;
                    }
                }
            }
        }
    }
    out
}

/// The whole answer as Pango markup.
pub fn pango(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;
    for line in text.trim().lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(format!("<tt>{}</tt>", escape(line)));
        } else if let Some(h) = trimmed.strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            out.push(format!("<b>{}</b>", inline(h)));
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let indent = " ".repeat((line.len() - trimmed.len()).min(8));
            out.push(format!("{indent}• {}", inline(item)));
        } else {
            out.push(inline(line));
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_cannot_be_injected() {
        let out = pango("<b onclick=x>hola</b> & <span foreground='red'>");
        assert!(!out.contains("<b onclick"));
        assert!(!out.contains("<span"));
        assert!(out.contains("&lt;b onclick=x&gt;"));
        assert!(out.contains("&amp;"));
    }

    #[test]
    fn the_usual_markdown_becomes_pango() {
        assert_eq!(pango("**hola** `x < y`"), "<b>hola</b> <tt>x &lt; y</tt>");
        assert_eq!(
            pango("# Título\n- uno\n* dos"),
            "<b>Título</b>\n• uno\n• dos"
        );
        assert_eq!(pango("```\nlet a = 1;\n```"), "<tt>let a = 1;</tt>");
        assert_eq!(pango("**sin cerrar"), "**sin cerrar");
        assert_eq!(pango("a [b"), "a [b");
    }

    #[test]
    fn only_web_links_are_linked() {
        assert_eq!(
            pango("[Ver](https://a.example/x?y=1&z=2)"),
            r#"<a href="https://a.example/x?y=1&amp;z=2">Ver</a>"#
        );
        for bad in [
            "[x](javascript:alert(1))",
            "[x](file:///etc/passwd)",
            "[x](brujula://settings)",
            "[x](ftp://a.example/)",
        ] {
            assert!(!pango(bad).contains("<a "), "{bad}");
        }
        assert_eq!(
            pango("mira https://a.example/p, gracias"),
            r#"mira <a href="https://a.example/p">https://a.example/p</a>, gracias"#
        );
        assert!(!pango("http es un protocolo").contains("<a"));
        // A quote cannot break out of the attribute.
        assert!(!pango(r#"[x](https://a.example/"onclick="y)"#).contains(r#"onclick="y"#));
    }
}
