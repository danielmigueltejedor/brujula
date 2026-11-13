//! HTTPS first: an `http://` address of a public site is tried as `https://` before anything is
//! sent in the clear. When the site does not answer on HTTPS at all, the browser falls back to
//! the address that was asked for (a failed certificate check is never a reason to fall back).

use url::{Host, Url};

/// The `https://` address to try instead of `http_url`, or `None` when it should be left alone:
/// not plain HTTP, an address that cannot have a public certificate (numeric addresses, single
/// word names, `.local` and friends) or a port other than the default one.
pub fn upgrade(http_url: &str) -> Option<String> {
    let url = Url::parse(http_url).ok()?;
    if url.scheme() != "http" || url.port().is_some() || !url.username().is_empty() {
        return None;
    }
    let Some(Host::Domain(host)) = url.host() else {
        return None;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if !host.contains('.') || is_private_name(&host) {
        return None;
    }
    let mut https = url;
    https.set_scheme("https").ok()?;
    Some(https.to_string())
}

/// The name of the site in an address, for remembering where HTTPS does not work.
pub fn host_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()?
        .host_str()
        .map(|h| h.trim_end_matches('.').to_ascii_lowercase())
}

fn is_private_name(host: &str) -> bool {
    const PRIVATE: &[&str] = &[
        "local",
        "localhost",
        "lan",
        "home",
        "internal",
        "intranet",
        "corp",
        "home.arpa",
        "test",
        "invalid",
        "example",
    ];
    PRIVATE
        .iter()
        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_http_addresses_are_upgraded_with_path_and_query_kept() {
        assert_eq!(
            upgrade("http://www.example.com/a/b?q=1#f").as_deref(),
            Some("https://www.example.com/a/b?q=1#f")
        );
        assert_eq!(
            upgrade("HTTP://Rust-Lang.org").as_deref(),
            Some("https://rust-lang.org/")
        );
    }

    #[test]
    fn everything_else_is_left_alone() {
        for url in [
            "https://example.com/",
            "http://localhost/",
            "http://localhost:8000/",
            "http://127.0.0.1/",
            "http://192.168.1.1/",
            "http://[::1]/",
            "http://router/",
            "http://nas.local/",
            "http://printer.lan/",
            "http://app.internal/x",
            "http://example.com:8080/",
            "http://user:pw@example.com/",
            "ftp://example.com/",
            "brujula://newtab",
            "file:///etc/passwd",
            "not a url",
            "",
        ] {
            assert_eq!(upgrade(url), None, "{url}");
        }
    }

    #[test]
    fn the_host_is_lower_case_without_the_final_dot() {
        assert_eq!(host_of("http://Foo.COM./x").as_deref(), Some("foo.com"));
        assert_eq!(host_of("nope"), None);
    }
}
