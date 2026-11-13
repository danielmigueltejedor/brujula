//! Chrome match patterns (`https://*.example.com/*`, `<all_urls>`), the way extensions say which
//! pages they work on.

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// `*` (http and https) or one scheme.
    scheme: String,
    /// `*`, `*.example.com` or an exact host; empty for `file`.
    host: String,
    /// Always starts with `/`; `*` matches anything.
    path: String,
}

impl Pattern {
    /// Parses a pattern. `<all_urls>` is every web page (http, https; local files and the
    /// browser's own pages are never matched).
    pub fn parse(text: &str) -> Option<Self> {
        if text == "<all_urls>" {
            return Some(Self {
                scheme: "*".into(),
                host: "*".into(),
                path: "/*".into(),
            });
        }
        let (scheme, rest) = text.split_once("://")?;
        if !(scheme == "*" || matches!(scheme, "http" | "https")) {
            return None;
        }
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => return None,
        };
        if host.is_empty()
            || host.contains(['@', ':', ' '])
            || host.matches('*').count() > 1
            || (host.contains('*') && host != "*" && !host.starts_with("*."))
        {
            return None;
        }
        Some(Self {
            scheme: scheme.to_owned(),
            host: host.to_ascii_lowercase(),
            path: path.to_owned(),
        })
    }

    pub fn matches(&self, url: &str) -> bool {
        let Ok(u) = Url::parse(url) else {
            return false;
        };
        let scheme_ok = match self.scheme.as_str() {
            "*" => matches!(u.scheme(), "http" | "https"),
            s => u.scheme() == s,
        };
        if !scheme_ok {
            return false;
        }
        let Some(host) = u.host_str() else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        let host_ok = if self.host == "*" {
            true
        } else if let Some(suffix) = self.host.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else {
            host == self.host
        };
        if !host_ok {
            return false;
        }
        let mut target = u.path().to_owned();
        if let Some(q) = u.query() {
            target.push('?');
            target.push_str(q);
        }
        glob_match(&self.path, &target)
    }

    /// The patterns WebKit's allow and block lists understand for this one: it has no `*`
    /// scheme, so that is spelled out.
    pub fn webkit(&self) -> Vec<String> {
        let schemes: &[&str] = if self.scheme == "*" {
            &["http", "https"]
        } else if self.scheme == "http" {
            &["http"]
        } else {
            &["https"]
        };
        schemes
            .iter()
            .map(|s| format!("{s}://{}{}", self.host, self.path))
            .collect()
    }
}

/// Matches `text` against `pattern` where `*` stands for anything and the rest is literal.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// A list of patterns; unreadable ones are dropped.
pub fn parse_all<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<Pattern> {
    texts.into_iter().filter_map(Pattern::parse).collect()
}

pub fn any_match(patterns: &[Pattern], url: &str) -> bool {
    patterns.iter().any(|p| p.matches(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, url: &str) -> bool {
        Pattern::parse(p).is_some_and(|p| p.matches(url))
    }

    #[test]
    fn hosts_and_wildcards() {
        assert!(m("https://*.example.com/*", "https://a.b.example.com/x?y=1"));
        assert!(m("https://*.example.com/*", "https://example.com/"));
        assert!(!m("https://*.example.com/*", "https://badexample.com/"));
        assert!(!m("https://*.example.com/*", "http://a.example.com/"));
        assert!(m("*://example.com/*", "http://example.com/a"));
        assert!(m("*://example.com/*", "https://example.com/"));
        assert!(!m("https://example.com/*", "https://example.com.evil.org/"));
        assert!(!m("https://example.com/*", "https://evil.org/https://example.com/"));
    }

    #[test]
    fn paths_and_queries() {
        assert!(m("https://example.com/a/*", "https://example.com/a/b/c"));
        assert!(!m("https://example.com/a/*", "https://example.com/b/a/"));
        assert!(m("https://example.com/*/watch*", "https://example.com/x/watch?v=1"));
        assert!(m("https://example.com/search?q=*", "https://example.com/search?q=rust"));
        assert!(!m("https://example.com/exact", "https://example.com/exact2"));
    }

    #[test]
    fn all_urls_means_web_pages_only() {
        assert!(m("<all_urls>", "https://anything.example/x"));
        assert!(m("<all_urls>", "http://a.b/"));
        assert!(!m("<all_urls>", "file:///etc/passwd"));
        assert!(!m("<all_urls>", "brujula://settings"));
        assert!(!m("<all_urls>", "brujula-ext://abc/popup.html"));
    }

    #[test]
    fn bad_patterns_are_refused() {
        for p in [
            "", "example.com", "https://", "https://example.com", "ftp://example.com/*",
            "https://exa*mple.com/*", "https://*.*.example.com/*", "https://user@example.com/*",
            "https://example.com:8080/*", "javascript:alert(1)",
        ] {
            assert!(Pattern::parse(p).is_none(), "{p}");
        }
    }

    #[test]
    fn webkit_spelling_has_no_star_scheme() {
        let p = Pattern::parse("*://*.example.com/a*").unwrap();
        assert_eq!(
            p.webkit(),
            vec!["http://*.example.com/a*", "https://*.example.com/a*"]
        );
        assert_eq!(
            Pattern::parse("<all_urls>").unwrap().webkit(),
            vec!["http://*/*", "https://*/*"]
        );
    }
}
