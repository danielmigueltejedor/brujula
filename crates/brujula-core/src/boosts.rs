//! Boosts: a site's look changed by the user, as Arc's boosts. Each one is a dark mode switch
//! and some CSS of the user's own, and applies only to that site and its subdomains.
//!
//! Only style: no script, so a boost cannot read or send what the page holds. The CSS is the
//! user's, written in the preferences, and is injected as a user style sheet (never as HTML).

use serde::{Deserialize, Serialize};

use crate::adblock::normalize_host;

/// Longest CSS of one boost, in characters.
pub const MAX_CSS: usize = 8 * 1024;
const MAX_BOOSTS: usize = 100;

/// Dark mode for pages that have none: the colours are inverted, and what should not be
/// (pictures, video, embedded pages) is inverted back.
const DARK: &str = "html { filter: invert(1) hue-rotate(180deg) !important; background: #fff; }\n\
img, picture, video, canvas, iframe, svg image { filter: invert(1) hue-rotate(180deg) !important; }\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boost {
    /// The site, without `www.` (see `adblock::normalize_host`).
    pub host: String,
    #[serde(default)]
    pub dark: bool,
    #[serde(default)]
    pub css: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Boosts {
    items: Vec<Boost>,
}

fn clean_css(css: &str) -> String {
    css.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(MAX_CSS)
        .collect()
}

/// The site a user typed: an address or a bare host.
pub fn host_of(text: &str) -> Option<String> {
    let t = text.trim();
    if t.starts_with("https://") || t.starts_with("http://") {
        return normalize_host(t);
    }
    // A bare host: no scheme, port or path to smuggle anything in.
    if t.contains([':', '/', '?', '#', '@']) {
        return None;
    }
    normalize_host(&format!("https://{t}/"))
}

impl Boosts {
    pub fn from_json(text: &str) -> Self {
        let mut b: Self = serde_json::from_str(text).unwrap_or_default();
        let mut seen: Vec<String> = Vec::new();
        b.items.retain_mut(|i| {
            let Some(host) = host_of(&i.host) else {
                return false;
            };
            i.host = host;
            i.css = clean_css(&i.css);
            if seen.contains(&i.host) {
                return false;
            }
            seen.push(i.host.clone());
            true
        });
        b.items.truncate(MAX_BOOSTS);
        b
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn items(&self) -> &[Boost] {
        &self.items
    }

    pub fn get(&self, host: &str) -> Option<&Boost> {
        self.items.iter().find(|b| b.host == host)
    }

    /// Starts a boost for a site; returns its host, or `None` for something that is not a site
    /// or when there are too many.
    pub fn add(&mut self, text: &str) -> Option<String> {
        let host = host_of(text)?;
        if self.get(&host).is_none() {
            if self.items.len() >= MAX_BOOSTS {
                return None;
            }
            self.items.push(Boost {
                host: host.clone(),
                dark: false,
                css: String::new(),
            });
        }
        Some(host)
    }

    pub fn remove(&mut self, host: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|b| b.host != host);
        self.items.len() != before
    }

    pub fn set_dark(&mut self, host: &str, dark: bool) -> bool {
        match self.items.iter_mut().find(|b| b.host == host) {
            Some(b) if b.dark != dark => {
                b.dark = dark;
                true
            }
            _ => false,
        }
    }

    pub fn set_css(&mut self, host: &str, css: &str) -> bool {
        let css = clean_css(css);
        match self.items.iter_mut().find(|b| b.host == host) {
            Some(b) if b.css != css => {
                b.css = css;
                true
            }
            _ => false,
        }
    }
}

impl Boost {
    /// The style sheet to inject, or `None` when the boost does nothing.
    pub fn stylesheet(&self) -> Option<String> {
        let mut sheet = String::new();
        if self.dark {
            sheet.push_str(DARK);
        }
        sheet.push_str(&self.css);
        (!sheet.trim().is_empty()).then_some(sheet)
    }

    /// WebKit match patterns for the site and its subdomains.
    pub fn patterns(&self) -> Vec<String> {
        let h = &self.host;
        vec![
            format!("https://{h}/*"),
            format!("http://{h}/*"),
            format!("https://*.{h}/*"),
            format!("http://*.{h}/*"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_are_taken_from_addresses_or_hosts_and_nothing_else() {
        assert_eq!(
            host_of("https://www.Example.com/a?b").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("  example.com ").as_deref(), Some("example.com"));
        for bad in [
            "",
            "javascript:alert(1)",
            "a b",
            "exa*mple.com",
            "file:///x",
            "http://",
        ] {
            assert_eq!(host_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_boost_is_added_once_and_edited() {
        let mut b = Boosts::default();
        assert_eq!(b.add("https://www.wiki.org/x").as_deref(), Some("wiki.org"));
        assert_eq!(b.add("wiki.org").as_deref(), Some("wiki.org"));
        assert_eq!(b.items().len(), 1);
        assert!(b.set_dark("wiki.org", true));
        assert!(!b.set_dark("wiki.org", true));
        assert!(b.set_css("wiki.org", "body{font-size:20px}"));
        assert!(!b.set_css("nowhere.org", "x"));
        let sheet = b.get("wiki.org").unwrap().stylesheet().unwrap();
        assert!(sheet.contains("invert(1)") && sheet.ends_with("font-size:20px}"));
        assert!(b.remove("wiki.org"));
        assert!(!b.remove("wiki.org"));
    }

    #[test]
    fn a_boost_that_changes_nothing_has_no_sheet() {
        let mut b = Boosts::default();
        b.add("a.org");
        assert_eq!(b.get("a.org").unwrap().stylesheet(), None);
        b.set_css("a.org", "  \n");
        assert_eq!(b.get("a.org").unwrap().stylesheet(), None);
    }

    #[test]
    fn patterns_name_the_site_and_its_subdomains_only() {
        let mut b = Boosts::default();
        b.add("a.org");
        let p = b.get("a.org").unwrap().patterns();
        assert!(p.contains(&"https://a.org/*".to_owned()));
        assert!(p.contains(&"https://*.a.org/*".to_owned()));
        assert!(p.iter().all(|x| !x.starts_with("*") && x.ends_with("/*")));
    }

    #[test]
    fn css_is_cleaned_and_bounded_and_a_file_is_read_forgivingly() {
        let mut b = Boosts::default();
        b.add("a.org");
        b.set_css("a.org", &format!("a{{}}\u{0}{}", "x".repeat(MAX_CSS * 2)));
        let css = &b.get("a.org").unwrap().css;
        assert_eq!(css.chars().count(), MAX_CSS);
        assert!(!css.contains('\u{0}'));
        let back = Boosts::from_json(
            r#"{"items":[{"host":"b.org"},{"host":"b.org"},{"host":"bad host"},{"host":"https://c.org/"}]}"#,
        );
        let hosts: Vec<&str> = back.items().iter().map(|i| i.host.as_str()).collect();
        assert_eq!(hosts, ["b.org", "c.org"]);
        assert_eq!(Boosts::from_json("{no"), Boosts::default());
    }
}
