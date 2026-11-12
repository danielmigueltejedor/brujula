//! User settings, stored as JSON. Reading is forgiving (missing or unknown fields are fine, a
//! corrupt file gives the defaults) so an update or a half-written file never stops the browser
//! from starting.

use serde::{Deserialize, Serialize};

use crate::omnibox::DEFAULT_SEARCH;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Vertical tab sidebar visible. On by default: it is the browser's tab strip.
    pub sidebar_visible: bool,
    pub adblock_enabled: bool,
    /// Ids of the filter lists turned off by the user (so new default lists are enabled).
    pub disabled_lists: Vec<String>,
    /// Search URL, `%s` is the query.
    pub search_template: String,
    pub homepage: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            adblock_enabled: true,
            disabled_lists: Vec::new(),
            search_template: DEFAULT_SEARCH.to_owned(),
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
    fn roundtrip() {
        let mut s = Settings::default();
        s.disabled_lists.push("easylist".into());
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert!(!s.list_enabled("easylist", true));
        assert!(s.list_enabled("easyprivacy", true));
        assert!(!s.list_enabled("fanboy-annoyance", false));
    }
}
