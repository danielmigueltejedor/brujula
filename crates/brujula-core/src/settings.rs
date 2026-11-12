//! User settings, stored as JSON. Reading is forgiving (missing or unknown fields are fine, a
//! corrupt file gives the defaults) so an update or a half-written file never stops the browser
//! from starting.

use serde::{Deserialize, Serialize};

use crate::search::{self, DEFAULT_ENGINE};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Vertical tab sidebar visible. On by default: it is the browser's tab strip.
    pub sidebar_visible: bool,
    pub adblock_enabled: bool,
    /// Sites (see `adblock::normalize_host`) where the blocker is switched off.
    pub allowlist: Vec<String>,
    /// Ids of the filter lists turned off by the user (so new default lists are enabled).
    pub disabled_lists: Vec<String>,
    /// Id of the chosen search engine (see `search::ENGINES`).
    pub search_engine: String,
    /// False until the user has picked one; the new-tab page asks on first run.
    pub search_engine_chosen: bool,
    pub homepage: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            adblock_enabled: true,
            allowlist: Vec::new(),
            disabled_lists: Vec::new(),
            search_engine: DEFAULT_ENGINE.to_owned(),
            search_engine_chosen: false,
            homepage: "about:blank".to_owned(),
        }
    }
}

impl Settings {
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Search URL of the chosen engine, `%s` is the query.
    pub fn search_template(&self) -> &'static str {
        search::engine_or_default(&self.search_engine).template
    }

    /// Turns the blocker off (`true`) or on (`false`) for one site. Returns whether the list
    /// changed.
    pub fn set_site_allowed(&mut self, host: &str, allowed: bool) -> bool {
        let present = self.allowlist.iter().any(|h| h == host);
        match (allowed, present) {
            (true, false) => {
                self.allowlist.push(host.to_owned());
                self.allowlist.sort();
                true
            }
            (false, true) => {
                self.allowlist.retain(|h| h != host);
                true
            }
            _ => false,
        }
    }

    pub fn list_enabled(&self, id: &str, default: bool) -> bool {
        if self.disabled_lists.iter().any(|d| d == id) {
            return false;
        }
        default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product() {
        let s = Settings::default();
        assert!(s.sidebar_visible, "vertical tabs are on by default");
        assert!(s.adblock_enabled);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s = Settings::from_json(r#"{"adblock_enabled": false}"#);
        assert!(!s.adblock_enabled);
        assert!(s.sidebar_visible);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        assert_eq!(Settings::from_json("{not json"), Settings::default());
        assert_eq!(Settings::from_json(""), Settings::default());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s = Settings::from_json(r#"{"future_option": 1, "sidebar_visible": false}"#);
        assert!(!s.sidebar_visible);
    }

    #[test]
    fn search_defaults_to_google_and_unknown_ids_fall_back() {
        let s = Settings::default();
        assert!(!s.search_engine_chosen);
        assert!(s.search_template().contains("google.com"));
        let s = Settings::from_json(r#"{"search_engine": "gone"}"#);
        assert!(s.search_template().contains("google.com"));
        let s = Settings::from_json(r#"{"search_engine": "bing", "search_engine_chosen": true}"#);
        assert!(s.search_template().contains("bing.com"));
    }

    #[test]
    fn site_allow_list_is_kept_sorted_and_unique() {
        let mut s = Settings::default();
        assert!(s.set_site_allowed("b.com", true));
        assert!(s.set_site_allowed("a.com", true));
        assert!(!s.set_site_allowed("a.com", true));
        assert_eq!(s.allowlist, vec!["a.com", "b.com"]);
        assert!(s.set_site_allowed("a.com", false));
        assert!(!s.set_site_allowed("a.com", false));
        assert_eq!(s.allowlist, vec!["b.com"]);
    }

    #[test]
    fn roundtrip() {
        let mut s = Settings::default();
        s.disabled_lists.push("easylist".into());
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert!(!s.list_enabled("easylist", true));
        assert!(s.list_enabled("easyprivacy", true));
        assert!(!s.list_enabled("fanboy-annoyance", false));
    }
}
