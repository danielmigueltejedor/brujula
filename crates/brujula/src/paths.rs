//! Where Brújula keeps its files.

use std::path::PathBuf;

pub const APP_ID: &str = "dev.danielmiguelt.Brujula";

/// The profile this process runs as: `None` for the first one (`default`), which keeps the
/// folders every earlier version used.
static PROFILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Chooses the profile of this process. Only the first call counts.
pub fn set_profile(id: &str) {
    if id != brujula_core::profiles::DEFAULT_ID && brujula_core::profiles::valid_id(id) {
        let _ = PROFILE.set(id.to_owned());
    }
}

/// The profile id, `default` unless one was chosen.
pub fn profile() -> &'static str {
    PROFILE
        .get()
        .map_or(brujula_core::profiles::DEFAULT_ID, String::as_str)
}

/// Folders shared by every profile: the list of profiles and the web apps.
pub fn root_data_dir() -> PathBuf {
    gtk::glib::user_data_dir().join("brujula")
}

pub fn root_cache_dir() -> PathBuf {
    gtk::glib::user_cache_dir().join("brujula")
}

pub fn root_config_dir() -> PathBuf {
    gtk::glib::user_config_dir().join("brujula")
}

fn of_profile(root: PathBuf) -> PathBuf {
    match PROFILE.get() {
        Some(id) => root.join("profiles").join(id),
        None => root,
    }
}

pub fn data_dir() -> PathBuf {
    of_profile(root_data_dir())
}

pub fn cache_dir() -> PathBuf {
    of_profile(root_cache_dir())
}

pub fn config_dir() -> PathBuf {
    of_profile(root_config_dir())
}

pub fn profiles_file() -> PathBuf {
    root_config_dir().join("profiles.json")
}

/// The application id of a profile's browser: each profile is its own instance.
pub fn app_id() -> String {
    match PROFILE.get() {
        Some(id) => format!("{APP_ID}.P{}", id.replace('-', "_")),
        None => APP_ID.to_owned(),
    }
}

pub fn lists_dir() -> PathBuf {
    cache_dir().join("filter-lists")
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn cookies_file() -> PathBuf {
    brujula_core::cookie_store(&data_dir())
}

pub fn ai_file() -> PathBuf {
    config_dir().join("ai.json")
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

pub fn vault_file() -> PathBuf {
    data_dir().join("passwords.vault")
}

pub fn essentials_file() -> PathBuf {
    data_dir().join("essentials.json")
}

pub fn webapps_file() -> PathBuf {
    root_data_dir().join("webapps.json")
}

/// The profile of a web app: its own cookies and storage.
pub fn webapp_data_dir(id: &str) -> PathBuf {
    root_data_dir().join("webapps").join(id)
}

pub fn webapp_cookies_file(id: &str) -> PathBuf {
    brujula_core::cookie_store(&webapp_data_dir(id))
}

pub fn webapp_cache_dir(id: &str) -> PathBuf {
    root_cache_dir().join("webapps").join(id)
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

/// Like `write_atomic`, for a file only its owner may read.
pub fn write_private(path: &std::path::Path, bytes: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}
