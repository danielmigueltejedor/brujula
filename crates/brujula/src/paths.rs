//! Where Brújula keeps its files.

use std::path::PathBuf;

pub const APP_ID: &str = "dev.danielmiguelt.Brujula";

pub fn data_dir() -> PathBuf {
    gtk::glib::user_data_dir().join("brujula")
}

pub fn cache_dir() -> PathBuf {
    gtk::glib::user_cache_dir().join("brujula")
}

pub fn config_dir() -> PathBuf {
    gtk::glib::user_config_dir().join("brujula")
}

pub fn lists_dir() -> PathBuf {
    cache_dir().join("filter-lists")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn bookmarks_file() -> PathBuf {
    data_dir().join("bookmarks.json")
}

pub fn archive_file() -> PathBuf {
    data_dir().join("archive.json")
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.json")
}

pub fn notes_file() -> PathBuf {
    data_dir().join("notes.json")
}

pub fn boosts_file() -> PathBuf {
    data_dir().join("boosts.json")
}

pub fn session_file() -> PathBuf {
    data_dir().join("session.json")
}

use nodalia_gtk::gtk;

/// Writes atomically so a crash never leaves half a file.
pub fn write_atomic(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Where the installed extensions live: one folder per id, with the unpacked package in `pkg`.
pub fn extensions_dir() -> PathBuf {
    data_dir().join("extensions")
}

pub fn extensions_file() -> PathBuf {
    data_dir().join("extensions.json")
}
