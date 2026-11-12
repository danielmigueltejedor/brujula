//! Turns what the user typed in the address bar into a URL to load.

use url::Url;

/// Search engine used when the input is not a URL. `%s` is replaced by the query.
pub const DEFAULT_SEARCH: &str = "https://duckduckgo.com/?q=%s";

/// Resolves address-bar input.
///
/// - A string with a scheme (`https://…`, `about:blank`, `file:///…`) is used as is, unless the
///   scheme can run code (`javascript:`, `data:`…): then it is searched for as text.
/// - Something that looks like a host (`example.com`, `localhost:3000`, `192.168.1.1/admin`)
///   gets `https://` (`http://` for localhost and private addresses, which rarely have TLS).
/// - Anything else is a search.
///
/// Returns `None` for blank input.
pub fn resolve(input: &str, search_template: &str) -> Option<Url> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }

    if has_scheme(text) {
        if let Some(url) = parse_allowed(text) {
            return Some(url);
        }
        // `javascript:`, `data:` and the like are never loaded from the address bar: pasting
        // them is a common trick to run a stranger's code in the page in front. They are
        // searched for as text instead.
        return Some(search_url(text, search_template));
    }

    if !text.contains(char::is_whitespace) {
        if let Some(url) = host_like(text) {
            return Some(url);
        }
    }

    Some(search_url(text, search_template))
}

/// True when `input` is a URL or a host, false when it would be searched for.
pub fn looks_like_address(input: &str) -> bool {
    let text = input.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return false;
    }
    (has_scheme(text) && parse_allowed(text).is_some()) || host_like(text).is_some()
}

/// Schemes the address bar may open. Anything else (`javascript:`, `data:`, `blob:`, `vbscript:`,
/// custom handlers) is refused.
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "file", "about"];

fn parse_allowed(text: &str) -> Option<Url> {
    let url = Url::parse(text).ok()?;
    ALLOWED_SCHEMES.contains(&url.scheme()).then_some(url)
}

fn has_scheme(text: &str) -> bool {
    // "localhost:3000" and "example.com:8080" parse as a scheme followed by a port, so a
    // scheme only counts when what follows the colon is not a port number.
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return false;
    }
    let port_like = rest
        .split(['/', '?', '#'])
        .next()
        .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    !port_like
}

fn host_like(text: &str) -> Option<Url> {
    let authority = text.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host_only = if let Some(stripped) = host.strip_prefix('[') {
        // IPv6 literal
        stripped.split(']').next()?
    } else {
        host.rsplit_once(':').map_or(host, |(h, p)| {
            if p.chars().all(|c| c.is_ascii_digit()) {
                h
            } else {
                host
            }
        })
    };
    if host_only.is_empty() {
        return None;
    }

    let is_local = host_only == "localhost" || host_only.ends_with(".localhost");
    let is_ip = host_only.parse::<std::net::IpAddr>().is_ok();
    let is_domain = host_only.contains('.')
        && !host_only.starts_with('.')
        && !host_only.ends_with('.')
        && host_only
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-'))
        && host_only
            .rsplit('.')
            .next()
            .is_some_and(|tld| tld.chars().any(|c| c.is_alphabetic()) || is_ip);

    if !(is_local || is_ip || is_domain) {
        return None;
    }

    let scheme = if is_local || is_private_ip(host_only) {
        "http"
    } else {
        "https"
    };
    Url::parse(&format!("{scheme}://{text}")).ok()
}

fn is_private_ip(host: &str) -> bool {
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// Builds the search URL, percent-encoding the query.
pub fn search_url(query: &str, template: &str) -> Url {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let template = if template.contains("%s") {
        template
    } else {
        DEFAULT_SEARCH
    };
    Url::parse(&template.replace("%s", &encoded))
        .unwrap_or_else(|_| Url::parse(&DEFAULT_SEARCH.replace("%s", &encoded)).expect("valid"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn code_schemes_are_searched_not_loaded() {
        for input in [
            "javascript:alert(document.cookie)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "blob:https://example.com/abc",
            "view-source:https://example.com",
            "ms-msdt:/id",
        ] {
            let url = resolve(input, DEFAULT_SEARCH).unwrap();
            assert_eq!(url.host_str(), Some("duckduckgo.com"), "{input} -> {url}");
            assert!(!looks_like_address(input), "{input}");
        }
    }

    #[test]
    fn web_and_local_schemes_still_open() {
        for input in [
            "https://example.com/a",
            "http://localhost:3000",
            "about:blank",
            "file:///tmp/a.html",
        ] {
            let url = resolve(input, DEFAULT_SEARCH).unwrap();
            assert_eq!(
                url.as_str().trim_end_matches('/'),
                input.trim_end_matches('/')
            );
        }
    }

    use super::*;

    fn r(s: &str) -> String {
        resolve(s, DEFAULT_SEARCH).unwrap().to_string()
    }

    #[test]
    fn blank_is_nothing() {
        assert!(resolve("   ", DEFAULT_SEARCH).is_none());
    }

    #[test]
    fn urls_with_scheme_are_kept() {
        assert_eq!(r("https://example.com/a?b=1"), "https://example.com/a?b=1");
        assert_eq!(r("about:blank"), "about:blank");
        assert_eq!(r("file:///tmp/x.html"), "file:///tmp/x.html");
    }

    #[test]
    fn hosts_get_https() {
        assert_eq!(r("example.com"), "https://example.com/");
        assert_eq!(
            r("sub.example.co.uk/path?q=1"),
            "https://sub.example.co.uk/path?q=1"
        );
        assert_eq!(r("example.com:8443"), "https://example.com:8443/");
    }

    #[test]
    fn local_and_private_get_http() {
        assert_eq!(r("localhost:3000"), "http://localhost:3000/");
        assert_eq!(r("192.168.1.1/admin"), "http://192.168.1.1/admin");
        assert_eq!(r("8.8.8.8"), "https://8.8.8.8/");
    }

    #[test]
    fn everything_else_is_a_search() {
        assert_eq!(
            r("rust gtk4 tutorial"),
            "https://duckduckgo.com/?q=rust+gtk4+tutorial"
        );
        assert_eq!(r("nodalix"), "https://duckduckgo.com/?q=nodalix");
        assert_eq!(r("a&b=c"), "https://duckduckgo.com/?q=a%26b%3Dc");
        assert_eq!(
            r("what is example.com"),
            "https://duckduckgo.com/?q=what+is+example.com"
        );
    }

    #[test]
    fn file_name_like_input_is_not_a_domain_with_numeric_tld() {
        // "v1.2.3" must be a search, not https://v1.2.3
        assert!(r("v1.2.3").starts_with("https://duckduckgo.com/"));
    }

    #[test]
    fn address_or_search() {
        assert!(looks_like_address("example.com"));
        assert!(looks_like_address("https://example.com/x"));
        assert!(looks_like_address("localhost:3000"));
        assert!(!looks_like_address("hola mundo"));
        assert!(!looks_like_address("nodalix"));
        assert!(!looks_like_address(""));
    }

    #[test]
    fn custom_template() {
        let u = resolve("hola mundo", "https://www.ecosia.org/search?q=%s").unwrap();
        assert_eq!(u.as_str(), "https://www.ecosia.org/search?q=hola+mundo");
    }

    #[test]
    fn broken_template_falls_back() {
        let u = resolve("hola", "no-placeholder").unwrap();
        assert_eq!(u.as_str(), "https://duckduckgo.com/?q=hola");
    }
}
