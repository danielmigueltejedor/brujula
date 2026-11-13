//! The cloud accounts of the desktop (Nodalix Ajustes sets them up, one folder per account under
//! `~/Nube`). Brújula does not talk to any cloud itself: it reads and writes plain files in that
//! folder, and the system's own mount keeps them in sync. So there is no second sign-in and no
//! password or token in the browser.
//!
//! The list lives in `~/.config/nodalix/cloud.json`, a JSON array of `{id, provider, name}`. The
//! folder name is derived from the account name exactly as Ajustes does, so both find the same
//! directory.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the accounts are listed, relative to the home directory.
pub const ACCOUNTS_FILE: &str = ".config/nodalix/cloud.json";
/// Where the accounts are mounted, relative to the home directory.
pub const MOUNTS_DIR: &str = "Nube";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub provider: &'static str,
    /// Name given in Ajustes.
    pub name: String,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    id: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    name: String,
}

/// `(key in cloud.json, how to call the service)`.
const PROVIDERS: &[(&str, &str)] = &[
    ("drive", "Google Drive"),
    ("onedrive", "OneDrive"),
    ("dropbox", "Dropbox"),
    ("iclouddrive", "iCloud Drive"),
];

fn provider_title(key: &str) -> Option<(&'static str, &'static str)> {
    PROVIDERS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(k, t)| (*k, *t))
}

/// Account ids name services and files, so they are only letters, digits, `-` and `_`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A folder the user picked for synchronisation: an absolute path with no control characters and
/// no `..`, so what is stored in the settings can only name one place.
pub fn valid_folder(path: &str) -> bool {
    let p = Path::new(path);
    p.is_absolute()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && p.components()
            .all(|c| !matches!(c, std::path::Component::ParentDir))
}

/// Folders that the desktop clients of the usual services keep in sync, by name in the home
/// directory. Only the ones that exist are offered.
pub const KNOWN_SYNC_FOLDERS: &[&str] = &[
    "Dropbox",
    "Nextcloud",
    "ownCloud",
    "OneDrive",
    "Google Drive",
    "iCloud Drive",
    "iCloudDrive",
    "MEGA",
    "pCloudDrive",
    "Sync",
    "Syncthing",
    "Insync",
];

/// The accounts in `json`. Entries that are not understood are skipped, so a newer Ajustes
/// that adds a service does not break this one.
pub fn parse_accounts(json: &str) -> Vec<Account> {
    let Ok(raw) = serde_json::from_str::<Vec<Raw>>(json) else {
        return Vec::new();
    };
    raw.into_iter()
        .filter(|r| valid_id(&r.id))
        .filter_map(|r| {
            let (provider, _) = provider_title(&r.provider)?;
            Some(Account {
                id: r.id,
                provider,
                name: r.name,
            })
        })
        .collect()
}

impl Account {
    /// The service's name.
    pub fn provider_title(&self) -> &'static str {
        provider_title(self.provider).map_or("Nube", |(_, t)| t)
    }

    /// Name of the account's folder under `~/Nube`: the account name with `/` and control
    /// characters turned into `-`; the service's name when that leaves nothing usable.
    pub fn folder_name(&self) -> String {
        let cleaned: String = self
            .name
            .trim()
            .chars()
            .map(|c| if c == '/' || c.is_control() { '-' } else { c })
            .collect();
        if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
            self.provider_title().to_owned()
        } else {
            cleaned
        }
    }

    /// Where the account is mounted.
    pub fn mount_dir(&self, home: &Path) -> PathBuf {
        home.join(MOUNTS_DIR).join(self.folder_name())
    }

    /// What the account is called in the preferences.
    pub fn label(&self) -> String {
        let name = self.folder_name();
        let title = self.provider_title();
        if name == title {
            name
        } else {
            format!("{name} · {title}")
        }
    }
}

/// Whether `dir` is a mount point listed in `mountinfo` (the text of `/proc/self/mountinfo`).
/// Without this check a cloud that is not connected would look like an ordinary empty folder, and
/// the sync would quietly fill the local disk instead of the cloud.
pub fn is_mounted(mountinfo: &str, dir: &Path) -> bool {
    let want = dir.to_string_lossy();
    mountinfo.lines().any(|line| {
        line.split(' ')
            .nth(4)
            .is_some_and(|mount| unescape_mount(mount) == want)
    })
}

/// `mountinfo` writes spaces and a few other characters as octal escapes (`\040`).
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4].iter().all(u8::is_ascii_digit)
        {
            let code = u32::from(bytes[i + 1] - b'0') * 64
                + u32::from(bytes[i + 2] - b'0') * 8
                + u32::from(bytes[i + 3] - b'0');
            if let Ok(b) = u8::try_from(code) {
                out.push(b);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_are_read_and_unknown_ones_skipped() {
        let json = r#"[
            {"id":"a1","provider":"drive","name":"Personal"},
            {"id":"b2","provider":"dropbox","name":""},
            {"id":"c3","provider":"mega","name":"No"},
            {"id":"../x","provider":"onedrive","name":"Mala"},
            {"id":"d4","provider":"iclouddrive"}
        ]"#;
        let a = parse_accounts(json);
        assert_eq!(a.len(), 3);
        assert_eq!(a[0].folder_name(), "Personal");
        assert_eq!(a[1].folder_name(), "Dropbox");
        assert_eq!(a[2].folder_name(), "iCloud Drive");
        assert!(parse_accounts("{no").is_empty());
        assert!(parse_accounts("").is_empty());
    }

    #[test]
    fn folder_names_cannot_leave_the_mount_directory() {
        let make = |name: &str| Account {
            id: "x".into(),
            provider: "drive",
            name: name.into(),
        };
        assert_eq!(make("a/b").folder_name(), "a-b");
        assert_eq!(make("..").folder_name(), "Google Drive");
        assert_eq!(make("  ").folder_name(), "Google Drive");
        assert_eq!(make("../..").folder_name(), "..-..");
        let dir = make("a/../b").mount_dir(Path::new("/home/u"));
        assert!(dir.starts_with("/home/u/Nube"));
        assert_eq!(dir.components().count(), 5);
    }

    #[test]
    fn a_picked_folder_names_one_place() {
        assert!(valid_folder("/home/u/Dropbox"));
        assert!(valid_folder("/home/u/Mi nube"));
        assert!(!valid_folder("Dropbox"));
        assert!(!valid_folder(""));
        assert!(!valid_folder("/home/u/../root"));
        assert!(!valid_folder("/home/u/a\nb"));
    }

    #[test]
    fn label_shows_the_service_when_the_name_differs() {
        let a = Account {
            id: "x".into(),
            provider: "dropbox",
            name: "Trabajo".into(),
        };
        assert_eq!(a.label(), "Trabajo · Dropbox");
        let b = Account {
            name: String::new(),
            ..a
        };
        assert_eq!(b.label(), "Dropbox");
    }

    #[test]
    fn only_a_real_mount_point_counts() {
        let info = "36 1 8:1 / / rw - ext4 /dev/sda1 rw\n\
                    90 36 0:50 / /home/u/Nube/Mi\\040Drive rw,nosuid - fuse.rclone nodalix-a rw\n";
        assert!(is_mounted(info, Path::new("/home/u/Nube/Mi Drive")));
        assert!(!is_mounted(info, Path::new("/home/u/Nube/Otro")));
        assert!(!is_mounted(info, Path::new("/home/u/Nube")));
        assert!(!is_mounted("", Path::new("/home/u/Nube/Mi Drive")));
    }
}
