//! User settings, stored as JSON. Reading is forgiving (missing or unknown fields are fine, a
//! corrupt file gives the defaults) so an update or a half-written file never stops the browser
//! from starting.

use serde::{Deserialize, Serialize};

use crate::search::{self, DEFAULT_ENGINE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follows the desktop.
    #[default]
    System,
    Light,
    Dark,
}

/// How the window is put together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// Sidebar and page are two islands side by side.
    #[default]
    Islands,
    /// One island, the page, with the sidebar (or tab bar) around it on the window's own colour,
    /// like Zen.
    Frame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TabPosition {
    #[default]
    Vertical,
    Horizontal,
}

pub const ACCENT_DEFAULT: &str = "#0c9a58";
pub const SIDEBAR_WIDTH_DEFAULT: u32 = 280;

/// How the browser looks. Every field has a valid range; [`Appearance::sanitized`] puts a value
/// read from a file or a web form back inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Theme,
    pub layout: Layout,
    pub tabs: TabPosition,
    /// `#rrggbb`.
    pub accent: String,
    /// Colour of the window frame (what shows around the page in `Frame` layout); empty is the
    /// theme's own. `#rrggbb`.
    pub frame_color: String,
    /// How opaque the sidebar and window frame are, 50 to 100 per cent.
    pub opacity: u32,
    /// Corner radius of the islands, in pixels.
    pub radius: u32,
    /// Gap between the islands and the window edge, in pixels.
    pub gap: u32,
    /// Width of the sidebar, in pixels.
    pub sidebar_width: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            layout: Layout::Islands,
            tabs: TabPosition::Vertical,
            accent: ACCENT_DEFAULT.to_owned(),
            frame_color: String::new(),
            opacity: 100,
            radius: 16,
            gap: 8,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
        }
    }
}

/// `#rrggbb` (any case) as lowercase, or `None`.
pub fn parse_hex_color(text: &str) -> Option<String> {
    let t = text.trim();
    let digits = t.strip_prefix('#')?;
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

/// Black or white, whichever reads better on `hex`.
pub fn text_on(hex: &str) -> &'static str {
    let Some(h) = parse_hex_color(hex) else {
        return "#ffffff";
    };
    let channel = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
    let lin = |c: f64| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * lin(channel(1)) + 0.7152 * lin(channel(3)) + 0.0722 * lin(channel(5));
    if luminance > 0.35 {
        "#111111"
    } else {
        "#ffffff"
    }
}

impl Appearance {
    pub fn sanitized(mut self) -> Self {
        self.accent = parse_hex_color(&self.accent).unwrap_or_else(|| ACCENT_DEFAULT.to_owned());
        self.frame_color = if self.frame_color.is_empty() {
            String::new()
        } else {
            parse_hex_color(&self.frame_color).unwrap_or_default()
        };
        self.opacity = self.opacity.clamp(50, 100);
        self.radius = self.radius.min(28);
        self.gap = self.gap.min(20);
        self.sidebar_width = self.sidebar_width.clamp(200, 480);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    /// The sidebar shows when the pointer goes to the left edge, if it is not pinned.
    pub edge_reveal: bool,
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
            appearance: Appearance::default(),
            edge_reveal: true,
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
        let mut s: Self = serde_json::from_str(text).unwrap_or_default();
        s.appearance = s.appearance.sanitized();
        s
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
    fn appearance_values_are_pulled_into_range() {
        let s = Settings::from_json(
            r##"{"appearance": {"opacity": 3, "radius": 900, "sidebar_width": 1, "accent": "red", "frame_color": "#ABCDEF"}}"##,
        );
        let a = &s.appearance;
        assert_eq!((a.opacity, a.radius, a.sidebar_width), (50, 28, 200));
        assert_eq!(a.accent, ACCENT_DEFAULT);
        assert_eq!(a.frame_color, "#abcdef");
        assert_eq!(a.layout, Layout::Islands);
    }

    #[test]
    fn colours_must_be_six_hex_digits() {
        assert_eq!(parse_hex_color(" #0C9A58 ").as_deref(), Some("#0c9a58"));
        for bad in ["", "0c9a58", "#fff", "#12345g", "rgb(1,2,3)", "#0c9a58;x"] {
            assert_eq!(parse_hex_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn text_colour_follows_the_background() {
        assert_eq!(text_on("#ffffff"), "#111111");
        assert_eq!(text_on("#000000"), "#ffffff");
        assert_eq!(text_on(ACCENT_DEFAULT), "#ffffff");
        assert_eq!(text_on("#ffdd00"), "#111111");
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
