//! WebKit puts the style sheets an app adds at the user level, below the page's own rules. An
//! extension's sheet is meant to win over the page, so each declaration is made `!important`.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    /// Holds rules (the top level, `@media`, `@supports`...).
    Rules,
    /// Holds declarations (a style rule).
    Decls,
    /// Left alone (`@keyframes`, `@font-face`...: `!important` would break them).
    Skip,
}

fn kind_of(prelude: &str, inside: Block) -> Block {
    if inside == Block::Skip {
        return Block::Skip;
    }
    let p = prelude.trim_start();
    if let Some(rest) = p.strip_prefix('@') {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_ascii_lowercase();
        return match name.as_str() {
            "media" | "supports" | "layer" | "container" | "document" | "scope"
            | "starting-style" => Block::Rules,
            _ => Block::Skip,
        };
    }
    Block::Decls
}

fn flush(cur: &str, out: &mut String) {
    let body = cur.trim_end();
    let lead = &cur[..cur.len() - cur.trim_start().len()];
    let lower = body.to_ascii_lowercase().replace(' ', "");
    if body.trim().is_empty() || !body.contains(':') || lower.contains("!important") {
        out.push_str(cur);
    } else {
        out.push_str(lead);
        out.push_str(body.trim_start());
        out.push_str(" !important");
        out.push_str(&cur[body.len()..]);
    }
}

/// `css` with `!important` on every declaration of a style rule.
pub fn important(css: &str) -> String {
    let mut out = String::with_capacity(css.len() + css.len() / 8);
    let mut cur = String::new();
    let mut stack: Vec<Block> = vec![Block::Rules];
    let mut quote: Option<char> = None;
    let mut parens = 0usize;
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '/' if chars.peek() == Some(&'*') => {
                cur.push(c);
                let mut prev = '\0';
                for n in chars.by_ref() {
                    cur.push(n);
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            '(' => {
                parens += 1;
                cur.push(c);
            }
            ')' => {
                parens = parens.saturating_sub(1);
                cur.push(c);
            }
            '{' if parens == 0 => {
                let top = *stack.last().unwrap_or(&Block::Rules);
                stack.push(kind_of(&cur, top));
                out.push_str(&cur);
                out.push('{');
                cur.clear();
            }
            ';' if parens == 0 => {
                if stack.last() == Some(&Block::Decls) {
                    flush(&cur, &mut out);
                } else {
                    out.push_str(&cur);
                }
                out.push(';');
                cur.clear();
            }
            '}' if parens == 0 => {
                if stack.last() == Some(&Block::Decls) {
                    flush(&cur, &mut out);
                } else {
                    out.push_str(&cur);
                }
                out.push('}');
                cur.clear();
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            _ => cur.push(c),
        }
    }
    out.push_str(&cur);
    out
}

#[cfg(test)]
mod tests {
    use super::important;

    #[test]
    fn declarations_become_important() {
        assert_eq!(
            important("a { color: red; margin: 0 }"),
            "a { color: red !important; margin: 0 !important }"
        );
        assert_eq!(
            important("p{color:red!important}"),
            "p{color:red!important}"
        );
    }

    #[test]
    fn rules_inside_media_are_changed_and_keyframes_and_fonts_are_not() {
        let css = "@media (min-width: 1px) { b { top: 1px } } @keyframes k { from { top: 0 } to { top: 9px } } @font-face { font-family: x; src: url(a.woff2) }";
        let out = important(css);
        assert!(out.contains("b { top: 1px !important }"));
        assert!(out.contains("from { top: 0 }") && out.contains("to { top: 9px }"));
        assert!(out.contains("src: url(a.woff2) }"));
    }

    #[test]
    fn semicolons_in_urls_and_strings_do_not_split_declarations() {
        let out = important(
            "i { background: url(data:image/png;base64,AAA) no-repeat; content: \"a;b\" }",
        );
        assert!(out.contains("no-repeat !important;"));
        assert!(out.contains("content: \"a;b\" !important }"));
        assert!(!out.contains("AAA) !important"));
    }

    #[test]
    fn imports_and_comments_are_kept() {
        let out = important("@import url(x.css); /* hi; there */ s { a: b }");
        assert!(out.starts_with("@import url(x.css);"));
        assert!(out.contains("/* hi; there */"));
        assert!(out.contains("a: b !important }"));
    }
}
