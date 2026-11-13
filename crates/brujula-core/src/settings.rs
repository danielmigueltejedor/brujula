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

/// A texture over the frame and the panels, under the colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Texture {
    #[default]
    None,
    /// Fine noise, like paper.
    Grain,
    /// Thin diagonal lines.
    Lines,
    Dots,
    Grid,
}

impl Texture {
    /// The id used by the preferences page and the settings file, and the name people see.
    pub const ALL: [(Texture, &'static str, &'static str); 5] = [
        (Texture::None, "none", "Ninguna"),
        (Texture::Grain, "grain", "Granulado"),
        (Texture::Lines, "lines", "Líneas"),
        (Texture::Dots, "dots", "Puntos"),
        (Texture::Grid, "grid", "Cuadrícula"),
    ];

    pub fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(t, ..)| *t == self)
            .map_or("none", |(_, id, _)| id)
    }

    pub fn from_id(id: &str) -> Option<Texture> {
        Self::ALL
            .iter()
            .find(|(_, i, _)| *i == id)
            .map(|(t, ..)| *t)
    }
}

pub const TEXTURE_AMOUNT_DEFAULT: u32 = 40;

pub const ACCENT_DEFAULT: &str = "#0c9a58";
pub const SIDEBAR_WIDTH_DEFAULT: u32 = 200;
pub const SIDEBAR_WIDTH_MIN: u32 = 160;
pub const SIDEBAR_WIDTH_MAX: u32 = 420;
/// The defaults of earlier versions, which were too wide. A file still holding one of them gets
/// the new default instead.
const SIDEBAR_WIDTH_OLD_DEFAULTS: [u32; 3] = [280, 232, 180];

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
    /// Two to four colours that the frame fades through, in place of the plain colour; empty for
    /// none. `#rrggbb` each.
    pub frame_gradient: Vec<String>,
    /// Direction of the gradient in degrees, a multiple of 45 (0 fades upwards, 90 to the right).
    pub frame_angle: u32,
    /// How opaque the sidebar and window frame are, 50 to 100 per cent.
    pub opacity: u32,
    /// Corner radius of the islands, in pixels.
    pub radius: u32,
    /// Gap between the islands and the window edge, in pixels.
    pub gap: u32,
    /// Width of the sidebar, in pixels.
    pub sidebar_width: u32,
    pub texture: Texture,
    /// How visible the texture is, 10 to 100 per cent.
    pub texture_amount: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            layout: Layout::Islands,
            tabs: TabPosition::Vertical,
            accent: ACCENT_DEFAULT.to_owned(),
            frame_color: String::new(),
            frame_gradient: Vec::new(),
            frame_angle: 135,
            opacity: 100,
            radius: 16,
            gap: 8,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
            texture: Texture::None,
            texture_amount: TEXTURE_AMOUNT_DEFAULT,
        }
    }
}

/// Most colours a gradient has.
pub const MAX_STOPS: usize = 4;

/// The average of some `#rrggbb` colours, for deciding what ink reads on a gradient.
pub fn mix_colors(colors: &[String]) -> Option<String> {
    let parsed: Vec<[u32; 3]> = colors
        .iter()
        .filter_map(|c| parse_hex_color(c))
        .map(|h| {
            let ch = |i: usize| u32::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0));
            [ch(1), ch(3), ch(5)]
        })
        .collect();
    let n = u32::try_from(parsed.len()).ok().filter(|n| *n > 0)?;
    let avg = |k: usize| parsed.iter().map(|c| c[k]).sum::<u32>() / n;
    Some(format!("#{:02x}{:02x}{:02x}", avg(0), avg(1), avg(2)))
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
        self.frame_gradient = self
            .frame_gradient
            .iter()
            .filter_map(|c| parse_hex_color(c))
            .take(MAX_STOPS)
            .collect();
        if self.frame_gradient.len() < 2 {
            self.frame_gradient.clear();
        }
        self.frame_angle = (self.frame_angle.min(360) / 45 * 45) % 360;
        self.opacity = self.opacity.clamp(50, 100);
        self.radius = self.radius.min(28);
        self.gap = self.gap.min(20);
        if SIDEBAR_WIDTH_OLD_DEFAULTS.contains(&self.sidebar_width) {
            self.sidebar_width = SIDEBAR_WIDTH_DEFAULT;
        }
        self.sidebar_width = self
            .sidebar_width
            .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
        self.texture_amount = self.texture_amount.clamp(10, 100);
        self
    }
}

/// Synchronisation through one of the cloud accounts of the desktop (see `cloud`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sync {
    pub enabled: bool,
    /// Id of the account in `~/.config/nodalix/cloud.json`; empty until one is chosen.
    pub account: String,
    /// A folder the user chose instead of an account: one that Dropbox, Nextcloud, iCloud Drive,
    /// Syncthing or the like already keeps in sync. Empty when an account is used.
    pub folder: String,
    pub bookmarks: bool,
    pub spaces: bool,
    pub tabs: bool,
    /// Names this device's file in the cloud. Made on first use and never shown.
    pub device_id: String,
    pub device_name: String,
}

impl Default for Sync {
    fn default() -> Self {
        Self {
            enabled: false,
            account: String::new(),
            folder: String::new(),
            bookmarks: true,
            spaces: true,
            tabs: true,
            device_id: String::new(),
            device_name: String::new(),
        }
    }
}

impl Sync {
    pub fn share(&self) -> crate::sync::Share {
        crate::sync::Share {
            bookmarks: self.bookmarks,
            spaces: self.spaces,
            tabs: self.tabs,
        }
    }

    /// On, with an account or a folder, and with something to share.
    pub fn active(&self) -> bool {
        self.enabled
            && !(self.account.is_empty() && self.folder.is_empty())
            && crate::sync::valid_device_id(&self.device_id)
            && (self.bookmarks || self.spaces || self.tabs)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub sync: Sync,
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
    /// Hours a tab can stay out of sight before it is archived; 0 never archives.
    pub archive_after_hours: u32,
    /// Minutes a tab can stay out of sight before its page is dropped from memory; 0 never.
    pub sleep_after_minutes: u32,
    /// Try `https://` first for public sites that were asked for over plain HTTP.
    pub https_first: bool,
    /// Remember the pages visited (never in a private window).
    pub history_enabled: bool,
    /// Offer to save the passwords typed in pages, and fill them in (never in a private window).
    pub passwords_enabled: bool,
    /// Answer the pages that ask for a passkey, with the phone as the authenticator (a QR code).
    pub passkeys_enabled: bool,
    /// The welcome page has been gone through. True for settings saved before it existed; a
    /// first run starts it false.
    pub welcome_done: bool,
    /// Look for a newer version of Brújula on its own, from time to time.
    pub update_check: bool,
    /// When Brújula last looked, in milliseconds since 1970.
    pub update_checked_at: u64,
    /// The newest version the person was already told about, so it is said once.
    pub update_notified: String,
    /// The version whose news were shown last; empty before this was kept.
    pub seen_version: String,
}

/// How long to wait between two looks for a newer version made on their own.
pub const UPDATE_CHECK_EVERY_MS: u64 = 12 * 60 * 60 * 1000;

/// Whether it is time to look again.
pub fn update_check_due(last_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_ms) >= UPDATE_CHECK_EVERY_MS
}

/// `0.4.0`: digits and dots, nothing else, and not too long.
fn is_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 20
        && text
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The times the user can choose to archive after, in hours (0 is never).
pub const ARCHIVE_CHOICES: [u32; 5] = [0, 12, 24, 168, 720];

/// The times the user can choose to put tabs to sleep after, in minutes (0 is never).
pub const SLEEP_CHOICES: [u32; 5] = [0, 10, 30, 60, 180];

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            sync: Sync::default(),
            edge_reveal: true,
            sidebar_visible: true,
            adblock_enabled: true,
            allowlist: Vec::new(),
            disabled_lists: Vec::new(),
            search_engine: DEFAULT_ENGINE.to_owned(),
            search_engine_chosen: false,
            homepage: "about:blank".to_owned(),
            archive_after_hours: 24,
            sleep_after_minutes: 30,
            https_first: true,
            history_enabled: true,
            passwords_enabled: true,
            passkeys_enabled: true,
            welcome_done: true,
            update_check: true,
            update_checked_at: 0,
            update_notified: String::new(),
            seen_version: String::new(),
        }
    }
}

impl Settings {
    pub fn from_json(text: &str) -> Self {
        let mut s: Self = serde_json::from_str(text).unwrap_or_default();
        s.appearance = s.appearance.sanitized();
        if !crate::sync::valid_device_id(&s.sync.device_id) {
            s.sync.device_id.clear();
        }
        if !crate::cloud::valid_id(&s.sync.account) {
            s.sync.account.clear();
        }
        for version in [&mut s.update_notified, &mut s.seen_version] {
            if !is_version(version) {
                version.clear();
            }
        }
        if !ARCHIVE_CHOICES.contains(&s.archive_after_hours) {
            s.archive_after_hours = 24;
        }
        if !SLEEP_CHOICES.contains(&s.sleep_after_minutes) {
            s.sleep_after_minutes = 30;
        }
        if !crate::cloud::valid_folder(&s.sync.folder) {
            s.sync.folder.clear();
        }
        s.sync.device_name = s.sync.device_name.chars().take(40).collect();
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
    fn the_check_for_updates_waits_between_looks() {
        assert!(update_check_due(0, UPDATE_CHECK_EVERY_MS));
        assert!(!update_check_due(1000, 1000 + UPDATE_CHECK_EVERY_MS - 1));
        // A clock that went back does not make it due.
        assert!(!update_check_due(5000, 1000));
    }

    #[test]
    fn versions_kept_in_the_settings_are_plain() {
        let s = Settings::from_json(r#"{"update_notified":"0.4.0","seen_version":"<b>x</b>"}"#);
        assert_eq!(s.update_notified, "0.4.0");
        assert_eq!(s.seen_version, "");
        assert!(!is_version("1..2") && !is_version("") && !is_version(&"1.".repeat(20)));
    }

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
        assert_eq!(
            (a.opacity, a.radius, a.sidebar_width),
            (50, 28, SIDEBAR_WIDTH_MIN)
        );
        assert_eq!(a.accent, ACCENT_DEFAULT);
        assert_eq!(a.frame_color, "#abcdef");
        assert_eq!(a.layout, Layout::Islands);
    }

    #[test]
    fn the_old_wide_defaults_are_replaced_by_the_new_one() {
        for old in [280, 232, 180] {
            let s =
                Settings::from_json(&format!(r#"{{"appearance": {{"sidebar_width": {old}}}}}"#));
            assert_eq!(s.appearance.sidebar_width, SIDEBAR_WIDTH_DEFAULT);
        }
        // Anything else the person chose stays, down to the narrowest.
        let s = Settings::from_json(r#"{"appearance": {"sidebar_width": 165}}"#);
        assert_eq!(s.appearance.sidebar_width, 165);
        assert_eq!(SIDEBAR_WIDTH_DEFAULT, 200);
        let s = Settings::from_json(r#"{"appearance": {"sidebar_width": 300}}"#);
        assert_eq!(s.appearance.sidebar_width, 300);
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
    fn sync_is_off_until_an_account_and_a_device_exist() {
        let mut s = Settings::default();
        assert!(!s.sync.active());
        s.sync.enabled = true;
        s.sync.account = "abc".into();
        assert!(!s.sync.active(), "no device id yet");
        s.sync.device_id = "abcdefgh12345678".into();
        assert!(s.sync.active());
        s.sync.bookmarks = false;
        s.sync.spaces = false;
        s.sync.tabs = false;
        assert!(!s.sync.active(), "nothing to share");
    }

    #[test]
    fn a_bad_account_or_device_id_in_the_file_is_dropped() {
        let s = Settings::from_json(
            r#"{"sync": {"enabled": true, "account": "../x", "device_id": "no/pe"}}"#,
        );
        assert!(s.sync.account.is_empty());
        assert!(s.sync.device_id.is_empty());
    }

    #[test]
    fn archiving_is_on_by_default_and_odd_values_go_back_to_it() {
        assert_eq!(Settings::default().archive_after_hours, 24);
        assert_eq!(
            Settings::from_json(r#"{"archive_after_hours": 0}"#).archive_after_hours,
            0
        );
        assert_eq!(
            Settings::from_json(r#"{"archive_after_hours": 7}"#).archive_after_hours,
            24
        );
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
