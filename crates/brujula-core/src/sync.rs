//! Syncing through a folder in one of the user's cloud drives.
//!
//! Every device writes one file of its own (`<device id>.json`) and only reads the others', so
//! two devices never write the same file and there is nothing to lock or to conflict. A file is a
//! [`Snapshot`]: the device's bookmarks (and the ones it removed), its spaces and folders, and
//! its open tabs. This module is the part with no I/O: building a snapshot, reading one back
//! safely (the file comes from a place other programs can write to, so every field is limited
//! and cleaned), and merging what other devices say into the local data.
//!
//! The merge only adds or removes what other devices clearly added or removed; it never rewrites
//! the local layout. Bookmarks use the time they were saved and removed; spaces and folders are
//! matched by name; open tabs of other devices are only listed, never opened by themselves.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::bookmarks::{Bookmarks, Removal};
use crate::folders::{Folders, Rule};
use crate::spaces::Spaces;
use crate::tabs::TabModel;

pub const VERSION: u32 = 1;
/// A file bigger than this is not read.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DEVICES: usize = 20;
const MAX_TABS: usize = 300;
const MAX_BOOKMARKS: usize = 5000;
const MAX_TEXT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncBookmark {
    pub url: String,
    pub title: String,
    pub added: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSpace {
    pub name: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncFolder {
    /// Name of the space it lives in.
    pub space: String,
    pub name: String,
    #[serde(default)]
    pub rule: Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTab {
    pub url: String,
    pub title: String,
    /// Names, not ids: ids mean nothing on another device.
    pub space: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub pinned: bool,
}

/// What one device says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub version: u32,
    pub device_id: String,
    pub device_name: String,
    /// When it was written, in milliseconds since 1970.
    pub updated: u64,
    pub bookmarks: Vec<SyncBookmark>,
    pub removed: Vec<Removal>,
    pub spaces: Vec<SyncSpace>,
    pub folders: Vec<SyncFolder>,
    pub tabs: Vec<SyncTab>,
}

/// What a device chose to share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    pub bookmarks: bool,
    pub spaces: bool,
    pub tabs: bool,
}

fn text(t: &str) -> String {
    t.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// A device id names a file, so it is only ever letters and digits.
pub fn valid_device_id(id: &str) -> bool {
    (8..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}

impl Snapshot {
    /// This device's state, limited to what it shares.
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        device_id: &str,
        device_name: &str,
        now: u64,
        share: Share,
        bookmarks: &Bookmarks,
        spaces: &Spaces,
        folders: &Folders,
        model: &TabModel,
    ) -> Self {
        let space_name = |id: u32| spaces.get(id).map(|s| s.name.clone());
        let mut snap = Self {
            version: VERSION,
            device_id: device_id.to_owned(),
            device_name: text(device_name),
            updated: now,
            ..Self::default()
        };
        if share.bookmarks {
            snap.bookmarks = bookmarks
                .items()
                .iter()
                .map(|b| SyncBookmark {
                    url: b.url.clone(),
                    title: b.title.clone(),
                    added: b.added,
                })
                .collect();
            snap.removed = bookmarks.removals().to_vec();
        }
        if share.spaces {
            snap.spaces = spaces
                .all()
                .iter()
                .map(|s| SyncSpace {
                    name: s.name.clone(),
                    icon: s.icon.clone(),
                })
                .collect();
            snap.folders = folders
                .all()
                .iter()
                .filter_map(|f| {
                    Some(SyncFolder {
                        space: space_name(f.space)?,
                        name: f.name.clone(),
                        rule: f.rule.clone(),
                    })
                })
                .collect();
        }
        if share.tabs {
            snap.tabs = model
                .tabs()
                .iter()
                .filter(|t| is_web(&t.url))
                .filter_map(|t| {
                    let folder = folders
                        .folder_of(t.folder, t.space, &t.url, &t.title)
                        .and_then(|f| folders.get(f))
                        .map(|f| f.name.clone())
                        .unwrap_or_default();
                    Some(SyncTab {
                        url: t.url.clone(),
                        title: t.title.clone(),
                        space: space_name(t.space)?,
                        folder,
                        pinned: t.pinned,
                    })
                })
                .take(MAX_TABS)
                .collect();
        }
        snap
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Reads a file another device wrote, or `None` if it is not one of ours. Everything in it
    /// is limited and cleaned: only web addresses, short text without control characters.
    pub fn from_json(json: &str) -> Option<Self> {
        let mut s: Self = serde_json::from_str(json).ok()?;
        if s.version == 0 || s.version > VERSION || !valid_device_id(&s.device_id) {
            return None;
        }
        s.device_name = text(&s.device_name);
        if s.device_name.is_empty() {
            s.device_name = "Otro dispositivo".to_owned();
        }
        s.bookmarks.retain(|b| is_web(&b.url));
        s.bookmarks.truncate(MAX_BOOKMARKS);
        for b in &mut s.bookmarks {
            b.title = text(&b.title);
        }
        s.removed.retain(|r| is_web(&r.url));
        s.removed.truncate(MAX_BOOKMARKS);
        s.spaces.truncate(12);
        for sp in &mut s.spaces {
            sp.name = text(&sp.name);
            sp.icon = crate::icons::clean(&sp.icon);
        }
        s.spaces.retain(|sp| !sp.name.is_empty());
        s.folders.truncate(40);
        for f in &mut s.folders {
            f.space = text(&f.space);
            f.name = text(&f.name);
            f.rule = Rule::from_form(f.rule.parts().0, f.rule.parts().1);
        }
        s.folders
            .retain(|f| !f.name.is_empty() && !f.space.is_empty());
        s.tabs.retain(|t| is_web(&t.url));
        s.tabs.truncate(MAX_TABS);
        for t in &mut s.tabs {
            t.title = text(&t.title);
            t.space = text(&t.space);
            t.folder = text(&t.folder);
        }
        Some(s)
    }
}

/// What a merge changed here.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub bookmarks_added: usize,
    pub bookmarks_removed: usize,
    pub spaces_added: usize,
    pub folders_added: usize,
}

impl Report {
    pub fn changed(&self) -> bool {
        *self != Self::default()
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Brings what the other devices say into the local data.
pub fn merge(
    remotes: &[Snapshot],
    own_id: &str,
    share: Share,
    bookmarks: &mut Bookmarks,
    spaces: &mut Spaces,
    folders: &mut Folders,
) -> Report {
    let mut report = Report::default();
    let others = remotes
        .iter()
        .filter(|r| r.device_id != own_id)
        .take(MAX_DEVICES);
    for r in others {
        if share.bookmarks {
            for b in &r.bookmarks {
                // Removed here after it was saved there: it stays removed.
                if bookmarks.removed_at(&b.url).is_some_and(|at| at >= b.added) {
                    continue;
                }
                if bookmarks.insert_synced(&b.url, &b.title, b.added) {
                    report.bookmarks_added += 1;
                }
            }
            for rem in &r.removed {
                // Saved here before it was removed there: it goes.
                if bookmarks.added_at(&rem.url).is_some_and(|at| at < rem.at)
                    && bookmarks.remove_at(&rem.url, rem.at)
                {
                    report.bookmarks_removed += 1;
                }
            }
        }
        if share.spaces {
            for sp in &r.spaces {
                if spaces.all().iter().any(|l| same_name(&l.name, &sp.name)) {
                    continue;
                }
                if let Some(id) = spaces.add(&sp.name) {
                    let name = spaces.get(id).map(|s| s.name.clone()).unwrap_or_default();
                    spaces.rename(id, &name, &sp.icon);
                    report.spaces_added += 1;
                }
            }
            for f in &r.folders {
                let Some(space) = spaces
                    .all()
                    .iter()
                    .find(|l| same_name(&l.name, &f.space))
                    .map(|l| l.id)
                else {
                    continue;
                };
                if folders.in_space(space).any(|l| same_name(&l.name, &f.name)) {
                    continue;
                }
                if folders.add(space, &f.name, f.rule.clone()).is_some() {
                    report.folders_added += 1;
                }
            }
        }
    }
    report
}

/// Name of the folder inside the cloud account.
pub const FOLDER: &str = "Brujula";

/// Writes this device's file (when `own` is given) in `<account folder>/Brujula` and reads the
/// files of the other devices, newest first.
///
/// Only plain files named `<device id>.json` are read, never links, never big ones, and a file
/// must name the device its own name says: what is in a cloud folder is not trusted. The account
/// folder must already exist (it is the mount), and is never created here.
pub fn exchange(
    account_dir: &Path,
    own_id: &str,
    own: Option<&Snapshot>,
) -> Result<Vec<Snapshot>, String> {
    if !valid_device_id(own_id) {
        return Err("device id".to_owned());
    }
    if !std::fs::metadata(account_dir).is_ok_and(|m| m.is_dir()) {
        return Err("la carpeta de la nube no existe".to_owned());
    }
    let dir = account_dir.join(FOLDER);
    match std::fs::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(format!("«{FOLDER}» no es una carpeta")),
        Err(_) => {
            std::fs::create_dir(&dir).map_err(|e| format!("no se pudo crear «{FOLDER}»: {e}"))?
        }
    }
    if let Some(own) = own {
        let tmp = dir.join(format!(".{own_id}.tmp"));
        std::fs::write(&tmp, own.to_json()).map_err(|e| format!("no se pudo escribir: {e}"))?;
        std::fs::rename(&tmp, dir.join(format!("{own_id}.json")))
            .map_err(|e| format!("no se pudo escribir: {e}"))?;
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("no se pudo leer: {e}"))?;
    let mut found: Vec<Snapshot> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(".json")?;
            if id == own_id || !valid_device_id(id) {
                return None;
            }
            let meta = std::fs::symlink_metadata(entry.path()).ok()?;
            if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
                return None;
            }
            let snap = Snapshot::from_json(&std::fs::read_to_string(entry.path()).ok()?)?;
            (snap.device_id == id).then_some(snap)
        })
        .collect();
    found.sort_by_key(|s| std::cmp::Reverse(s.updated));
    found.truncate(MAX_DEVICES);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "aaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbb";
    const ALL: Share = Share {
        bookmarks: true,
        spaces: true,
        tabs: true,
    };

    fn snap(id: &str) -> Snapshot {
        Snapshot {
            version: VERSION,
            device_id: id.into(),
            device_name: id.into(),
            updated: 1,
            ..Snapshot::default()
        }
    }

    fn local() -> (Bookmarks, Spaces, Folders) {
        (Bookmarks::default(), Spaces::default(), Folders::default())
    }

    #[test]
    fn a_snapshot_survives_its_own_json() {
        let (mut b, mut s, mut f) = local();
        b.toggle_at("https://a.com/", "A", 10);
        b.toggle_at("https://gone.com/", "G", 11);
        b.remove_at("https://gone.com/", 12);
        let work = s.add("Trabajo").unwrap();
        f.add(work, "Docs", Rule::Site("docs.rs".into())).unwrap();
        let mut m = TabModel::new();
        m.set_current_space(work);
        m.open("https://docs.rs/x", None, true);
        m.open("brujula://settings", None, false);
        let snap = Snapshot::capture(ID_A, "Mi PC", 99, ALL, &b, &s, &f, &m);
        assert_eq!(snap.tabs.len(), 1, "only web pages are shared");
        assert_eq!(snap.tabs[0].space, "Trabajo");
        assert_eq!(snap.tabs[0].folder, "Docs");
        assert_eq!(Snapshot::from_json(&snap.to_json()), Some(snap));
    }

    #[test]
    fn what_is_not_shared_is_not_written() {
        let (mut b, s, f) = local();
        b.toggle("https://a.com/", "A");
        let mut m = TabModel::new();
        m.open("https://a.com/", None, true);
        let none = Share {
            bookmarks: false,
            spaces: false,
            tabs: false,
        };
        let snap = Snapshot::capture(ID_A, "x", 1, none, &b, &s, &f, &m);
        assert!(snap.bookmarks.is_empty() && snap.tabs.is_empty() && snap.spaces.is_empty());
    }

    #[test]
    fn a_hostile_file_is_cleaned_or_refused() {
        assert!(Snapshot::from_json("garbage").is_none());
        assert!(Snapshot::from_json(r#"{"version":1,"device_id":"../../etc"}"#).is_none());
        assert!(Snapshot::from_json(r#"{"version":9,"device_id":"aaaaaaaaaaaa"}"#).is_none());
        let json = serde_json::json!({
            "version": 1, "device_id": ID_B, "device_name": "x\u{7}\ny",
            "bookmarks": [
                {"url": "javascript:alert(1)", "title": "a", "added": 1},
                {"url": "file:///etc/passwd", "title": "a", "added": 1},
                {"url": "https://ok.com", "title": "t\u{0}itle", "added": 1}
            ],
            "tabs": [{"url": "data:text/html,x", "title": "a", "space": "s"},
                     {"url": "https://t.com", "title": "ok", "space": "s\n1"}],
            "spaces": [{"name": "  ", "icon": ""}, {"name": "Ok", "icon": "<script>"}],
            "folders": [{"space": "Ok", "name": "F", "rule": {"kind": "site", "text": " "}}],
        })
        .to_string();
        let s = Snapshot::from_json(&json).unwrap();
        assert_eq!(s.device_name, "xy");
        assert_eq!(s.bookmarks.len(), 1);
        assert_eq!(s.bookmarks[0].title, "title");
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].space, "s1");
        assert_eq!(s.spaces.len(), 1);
        assert_eq!(s.spaces[0].icon, "");
        assert_eq!(s.folders[0].rule, Rule::Manual);
    }

    #[test]
    fn device_ids_cannot_name_other_files() {
        assert!(valid_device_id(ID_A));
        for bad in [
            "",
            "short",
            "../x../x../x",
            "a/b/c/d/e/f/g/h",
            "a b c d e f g h",
            &"a".repeat(41),
        ] {
            assert!(!valid_device_id(bad), "{bad}");
        }
    }

    #[test]
    fn bookmarks_from_other_devices_arrive_and_own_data_is_not_touched() {
        let (mut b, mut s, mut f) = local();
        b.toggle_at("https://mine.com/", "Mine", 5);
        let mut r = snap(ID_B);
        r.bookmarks = vec![
            SyncBookmark {
                url: "https://theirs.com/".into(),
                title: "Theirs".into(),
                added: 20,
            },
            SyncBookmark {
                url: "https://mine.com".into(),
                title: "Mine?".into(),
                added: 6,
            },
        ];
        let report = merge(&[r.clone()], ID_A, ALL, &mut b, &mut s, &mut f);
        assert_eq!(report.bookmarks_added, 1);
        assert_eq!(b.items()[0].url, "https://theirs.com/", "newest first");
        assert_eq!(b.items().iter().filter(|i| i.title == "Mine").count(), 1);
        // The second time nothing changes.
        assert!(!merge(&[r], ID_A, ALL, &mut b, &mut s, &mut f).changed());
    }

    #[test]
    fn a_device_never_merges_its_own_file() {
        let (mut b, mut s, mut f) = local();
        let mut mine = snap(ID_A);
        mine.bookmarks = vec![SyncBookmark {
            url: "https://a.com/".into(),
            title: "A".into(),
            added: 1,
        }];
        assert!(!merge(&[mine], ID_A, ALL, &mut b, &mut s, &mut f).changed());
        assert!(b.items().is_empty());
    }

    #[test]
    fn removing_on_one_device_removes_on_the_other_unless_saved_again_later() {
        let (mut b, mut s, mut f) = local();
        b.toggle_at("https://a.com/", "A", 10);
        b.toggle_at("https://b.com/", "B", 50);
        let mut r = snap(ID_B);
        r.removed = vec![
            Removal {
                url: "https://a.com/".into(),
                at: 30,
            },
            Removal {
                url: "https://b.com/".into(),
                at: 30,
            },
        ];
        let report = merge(&[r], ID_A, ALL, &mut b, &mut s, &mut f);
        assert_eq!(report.bookmarks_removed, 1);
        assert!(!b.contains("https://a.com/"));
        assert!(b.contains("https://b.com/"), "saved after the removal");
        // And what was removed here is not brought back by a device that still has it.
        let mut stale = snap(ID_B);
        stale.bookmarks = vec![SyncBookmark {
            url: "https://a.com/".into(),
            title: "A".into(),
            added: 10,
        }];
        assert!(!merge(&[stale], ID_A, ALL, &mut b, &mut s, &mut f).changed());
        assert!(!b.contains("https://a.com/"));
    }

    #[test]
    fn spaces_and_folders_are_matched_by_name_and_only_added() {
        let (mut b, mut s, mut f) = local();
        let mut r = snap(ID_B);
        r.spaces = vec![
            SyncSpace {
                name: "principal".into(),
                icon: "user".into(),
            },
            SyncSpace {
                name: "Clase".into(),
                icon: "backpack".into(),
            },
        ];
        r.folders = vec![
            SyncFolder {
                space: "Clase".into(),
                name: "Mates".into(),
                rule: Rule::Manual,
            },
            SyncFolder {
                space: "Nada".into(),
                name: "Perdida".into(),
                rule: Rule::Manual,
            },
        ];
        let report = merge(&[r.clone()], ID_A, ALL, &mut b, &mut s, &mut f);
        assert_eq!((report.spaces_added, report.folders_added), (1, 1));
        let clase = s.all().iter().find(|x| x.name == "Clase").unwrap();
        assert_eq!(clase.icon, "backpack");
        assert_eq!(s.all().len(), 2, "Principal is the same as principal");
        assert_eq!(f.in_space(clase.id).count(), 1);
        assert!(!merge(&[r], ID_A, ALL, &mut b, &mut s, &mut f).changed());
    }

    #[test]
    fn only_what_is_shared_is_merged() {
        let (mut b, mut s, mut f) = local();
        let mut r = snap(ID_B);
        r.bookmarks = vec![SyncBookmark {
            url: "https://a.com/".into(),
            title: "A".into(),
            added: 1,
        }];
        r.spaces = vec![SyncSpace {
            name: "Clase".into(),
            icon: String::new(),
        }];
        let only_spaces = Share {
            bookmarks: false,
            spaces: true,
            tabs: false,
        };
        let report = merge(&[r], ID_A, only_spaces, &mut b, &mut s, &mut f);
        assert_eq!((report.bookmarks_added, report.spaces_added), (0, 1));
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("brujula-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn devices_exchange_through_a_folder() {
        let cloud = scratch("exchange");
        let mut a = snap(ID_A);
        a.bookmarks.push(SyncBookmark {
            url: "https://a.example/".into(),
            title: "A".into(),
            added: 5,
        });
        // The first device finds nobody else; the second finds the first and not itself.
        assert!(exchange(&cloud, ID_A, Some(&a)).unwrap().is_empty());
        let seen = exchange(&cloud, ID_B, Some(&snap(ID_B))).unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].device_id, ID_A);
        assert_eq!(seen[0].bookmarks.len(), 1);
        assert!(cloud.join(FOLDER).join(format!("{ID_A}.json")).is_file());
        // Reading without writing leaves the files alone.
        assert_eq!(exchange(&cloud, ID_B, None).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(cloud);
    }

    #[test]
    fn a_missing_cloud_folder_is_not_created() {
        let cloud = scratch("missing");
        let gone = cloud.join("Nube-desconectada");
        assert!(exchange(&gone, ID_A, Some(&snap(ID_A))).is_err());
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(cloud);
    }

    #[test]
    fn strange_files_in_the_cloud_folder_are_ignored() {
        let cloud = scratch("strange");
        let dir = cloud.join(FOLDER);
        std::fs::create_dir_all(&dir).unwrap();
        // Good file, a file that lies about its device, junk, a name that is not an id, a huge
        // file and a link.
        std::fs::write(dir.join(format!("{ID_B}.json")), snap(ID_B).to_json()).unwrap();
        std::fs::write(
            dir.join("cccccccccccccccc.json"),
            snap("dddddddddddddddd").to_json(),
        )
        .unwrap();
        std::fs::write(dir.join("eeeeeeeeeeeeeeee.json"), "{not json").unwrap();
        std::fs::write(dir.join("..json"), snap(ID_B).to_json()).unwrap();
        std::fs::write(dir.join("notes.json"), snap(ID_B).to_json()).unwrap();
        let big = "x".repeat(usize::try_from(MAX_FILE_BYTES).unwrap() + 1);
        std::fs::write(dir.join("ffffffffffffffff.json"), big).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            dir.join(format!("{ID_B}.json")),
            dir.join("gggggggggggggggg.json"),
        )
        .unwrap();
        let seen = exchange(&cloud, ID_A, None).unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].device_id, ID_B);
        let _ = std::fs::remove_dir_all(cloud);
    }

    #[test]
    fn a_folder_that_is_really_a_file_stops_the_round() {
        let cloud = scratch("notdir");
        std::fs::write(cloud.join(FOLDER), "x").unwrap();
        assert!(exchange(&cloud, ID_A, Some(&snap(ID_A))).is_err());
        let _ = std::fs::remove_dir_all(cloud);
    }
}
