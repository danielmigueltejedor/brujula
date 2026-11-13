//! Brújula's logic without a UI, so it can be tested without GTK or WebKit.

#![forbid(unsafe_code)]

pub mod adblock;
pub mod appearance;
pub mod archive;
pub mod bookmarks;
pub mod boosts;
pub mod cloud;
pub mod downloads;
pub mod edge;
pub mod essentials;
pub mod ext;
pub mod folders;
pub mod history;
pub mod https;
pub mod icons;
pub mod newtab;
pub mod notes;
pub mod omnibox;
pub mod prefs;
pub mod scriptlets;
pub mod search;
pub mod session;
pub mod settings;
pub mod sleep;
pub mod spaces;
pub mod suggest;
pub mod sync;
pub mod tabs;
pub mod vault;
pub mod webapps;

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
