//! Brújula's logic without a UI, so it can be tested without GTK or WebKit.

#![forbid(unsafe_code)]

pub mod adblock;
pub mod ai;
pub mod appearance;
pub mod archive;
pub mod avatars;
pub mod bookmark_import;
pub mod bookmarks;
pub mod boosts;
pub mod browser_data;
pub mod cloud;
pub mod downloads;
pub mod edge;
pub mod essentials;
pub mod ext;
pub mod folders;
pub mod fonts;
pub mod history;
pub mod https;
pub mod icons;
pub mod news;
pub mod newtab;
pub mod notes;
pub mod omnibox;
pub mod prefs;
pub mod profiles;
pub mod scan;
pub mod scriptlets;
pub mod search;
pub mod session;
pub mod settings;
pub mod sleep;
pub mod spaces;
pub mod suggest;
pub mod sync;
pub mod tabs;
pub mod totp;
pub mod useragent;
pub mod vault;
pub mod webapps;
pub mod welcome;

/// Whether an address is one of the browser's own pages (new tab, preferences, welcome, news).
/// They are drawn by Brújula, hold no web content and keep state a reload would throw away
/// (the step of the welcome, a half-typed field), so nothing reloads them on its own.
pub fn is_own_page(uri: &str) -> bool {
    [
        newtab::NEWTAB_URI,
        newtab::CHOOSE_ENGINE_URI,
        prefs::SETTINGS_URI,
        welcome::WELCOME_URI,
        news::NEWS_URI,
    ]
    .contains(&uri)
}

/// Text suitable for a user-visible name: no control characters, surrounding whitespace or
/// more characters than the field can store.
pub(crate) fn clean_name(name: &str, max: usize) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_pages_are_told_apart_from_web_pages() {
        for own in [
            "brujula://newtab",
            "brujula://choose-engine",
            "brujula://settings",
            "brujula://welcome",
            "brujula://news",
        ] {
            assert!(is_own_page(own), "{own}");
        }
        for web in [
            "https://example.com/",
            "about:blank",
            "",
            "brujula://welcome?next=1",
        ] {
            assert!(!is_own_page(web), "{web}");
        }
    }
}
