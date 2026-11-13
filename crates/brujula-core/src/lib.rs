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
pub mod csp;
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
pub mod sitepermissions;
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

/// Where a profile (or a web app) keeps its cookies: a SQLite file in its own data folder.
/// WebKit keeps cookies in memory unless it is given a file, so without it every restart, and
/// so every update, signed the person out of every site.
pub fn cookie_store(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("cookies.sqlite")
}

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

/// Whether an address belongs to the browser itself (`brujula://…`): its own pages and the
/// requests they make. Compared without regard to case, as schemes are.
pub fn is_internal_address(uri: &str) -> bool {
    uri.trim_start()
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("brujula:"))
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
    fn cookies_live_in_the_data_folder_they_belong_to() {
        let dir = std::path::Path::new("/home/a/.local/share/brujula/profiles/x");
        let file = cookie_store(dir);
        assert!(file.starts_with(dir));
        assert_eq!(
            file.file_name().and_then(|n| n.to_str()),
            Some("cookies.sqlite")
        );
    }

    #[test]
    fn the_browsers_own_addresses_are_told_apart_from_the_web() {
        for own in [
            "brujula://settings",
            "BRUJULA://settings/set?x=1",
            "  brujula://newtab",
        ] {
            assert!(is_internal_address(own), "{own}");
        }
        for other in [
            "https://brujula.example/",
            "brujula-ext://abc/page.html",
            "brujula-font://inter",
            "brujula",
            "ñandú://x",
            "",
        ] {
            assert!(!is_internal_address(other), "{other}");
        }
    }

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
