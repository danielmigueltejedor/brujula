//! The tabs to bring back on the next start.

use serde::{Deserialize, Serialize};

use crate::folders::{Folders, NO_FOLDER};
use crate::spaces::{Spaces, FIRST_SPACE};
use crate::tabs::TabModel;

fn first_space() -> u32 {
    FIRST_SPACE
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTab {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "first_space")]
    pub space: u32,
    #[serde(default)]
    pub folder: Option<u32>,
    /// When it was last in front (see `Tab::active`).
    #[serde(default)]
    pub active: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    pub selected: usize,
    pub spaces: Spaces,
    pub folders: Folders,
    /// When it was written, in milliseconds since 1970: what `SavedTab::active` is measured
    /// against, so time the browser was closed does not count as time a tab went unused.
    pub saved_at: u64,
}

/// Only these schemes are restored: a session file must not be able to make the browser open
/// `file:` paths or `javascript:` URLs on its own.
fn restorable(url: &str) -> bool {
    url == "about:blank"
        || url == crate::newtab::NEWTAB_URI
        || url.starts_with("https://")
        || url.starts_with("http://")
}

impl Session {
    pub fn capture(model: &TabModel, spaces: &Spaces, folders: &Folders) -> Self {
        let kept: Vec<_> = model.tabs().iter().filter(|t| restorable(&t.url)).collect();
        let selected = model
            .selected()
            .and_then(|sel| kept.iter().position(|t| t.id == sel))
            .unwrap_or(0);
        let tabs = kept
            .iter()
            .map(|t| SavedTab {
                url: t.url.clone(),
                title: t.title.clone(),
                pinned: t.pinned,
                space: t.space,
                folder: t.folder,
                active: t.active,
            })
            .collect();
        Self {
            tabs,
            selected,
            spaces: spaces.clone(),
            folders: folders.clone(),
            saved_at: 0,
        }
    }

    /// Notes when the session was written.
    pub fn stamped(mut self, now: u64) -> Self {
        self.saved_at = now;
        self
    }

    pub fn from_json(text: &str) -> Self {
        let mut s: Session = serde_json::from_str(text).unwrap_or_default();
        s.tabs.retain(|t| restorable(&t.url));
        s.spaces = s.spaces.sanitized();
        s.folders = std::mem::take(&mut s.folders).sanitized(&s.spaces);
        let home = s.spaces.all()[0].id;
        for t in &mut s.tabs {
            if !s.spaces.contains(t.space) {
                t.space = home;
            }
            // A folder that is gone leaves the tab to the smart folders.
            if t.folder
                .is_some_and(|f| f != NO_FOLDER && s.folders.get(f).is_none())
            {
                t.folder = None;
            }
        }
        if s.selected >= s.tabs.len() {
            s.selected = 0;
        }
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_and_restore_keep_order_pinning_and_selection() {
        let mut m = TabModel::new();
        let (a, _) = m.open("https://a.com", None, false);
        let (b, _) = m.open("https://b.com", None, true);
        m.set_pinned(a, true);
        let s = Session::capture(&m, &Spaces::default(), &Folders::default());
        assert_eq!(s.tabs.len(), 2);
        assert!(s.tabs[0].pinned);
        assert_eq!(s.selected, 1);
        assert_eq!(Session::from_json(&s.to_json()), s);
        let _ = b;
    }

    #[test]
    fn when_a_tab_was_last_used_and_when_the_session_was_written_come_back() {
        let mut m = TabModel::new();
        let (a, _) = m.open("https://a.com", None, true);
        m.touch(a, 5_000);
        let s = Session::capture(&m, &Spaces::default(), &Folders::default()).stamped(9_000);
        let back = Session::from_json(&s.to_json());
        assert_eq!((back.tabs[0].active, back.saved_at), (5_000, 9_000));
        // An old file without the fields still reads.
        let old = Session::from_json(r#"{"tabs":[{"url":"https://a.com"}]}"#);
        assert_eq!((old.tabs[0].active, old.saved_at), (0, 0));
    }

    #[test]
    fn folders_and_the_tabs_in_them_come_back() {
        use crate::folders::Rule;
        let mut folders = Folders::default();
        let f = folders
            .add(FIRST_SPACE, "Código", Rule::Site("github.com".into()))
            .unwrap();
        let mut m = TabModel::new();
        let (a, _) = m.open("https://github.com/x", None, false);
        let (b, _) = m.open("https://b.com", None, false);
        let (c, _) = m.open("https://c.com", None, false);
        m.set_folder(a, Some(f));
        m.set_folder(b, Some(NO_FOLDER));
        let _ = c;
        let s = Session::from_json(&Session::capture(&m, &Spaces::default(), &folders).to_json());
        assert_eq!(s.folders.all().len(), 1);
        assert_eq!(
            s.tabs.iter().map(|t| t.folder).collect::<Vec<_>>(),
            vec![Some(f), Some(NO_FOLDER), None]
        );
        // A tab pointing at a folder the file no longer has is left to the smart folders.
        let stray = Session::from_json(
            r#"{"tabs":[{"url":"https://a.com","folder":9},{"url":"https://b.com","folder":0}]}"#,
        );
        assert_eq!(stray.tabs[0].folder, None);
        assert_eq!(stray.tabs[1].folder, Some(NO_FOLDER));
    }

    #[test]
    fn tabs_keep_their_space_and_old_files_land_in_the_first() {
        let mut spaces = Spaces::default();
        let work = spaces.add("Trabajo").unwrap();
        let mut m = TabModel::new();
        m.open("https://a.com", None, false);
        m.set_current_space(work);
        m.open("https://b.com", None, true);
        let s = Session::from_json(&Session::capture(&m, &spaces, &Folders::default()).to_json());
        assert_eq!(s.tabs[0].space, FIRST_SPACE);
        assert_eq!(s.tabs[1].space, work);
        assert_eq!(s.spaces.all().len(), 2);
        let old = Session::from_json(r#"{"tabs":[{"url":"https://ok.com"}],"selected":0}"#);
        assert_eq!(old.tabs[0].space, FIRST_SPACE);
        let stray = Session::from_json(r#"{"tabs":[{"url":"https://ok.com","space":42}]}"#);
        assert_eq!(stray.tabs[0].space, FIRST_SPACE);
    }

    #[test]
    fn dangerous_schemes_are_dropped() {
        let s = Session::from_json(
            r#"{"tabs":[{"url":"file:///etc/passwd"},{"url":"javascript:alert(1)"},{"url":"https://ok.com"}],"selected":2}"#,
        );
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.tabs[0].url, "https://ok.com");
        assert_eq!(s.selected, 0, "selection pointing past the end resets");
    }

    #[test]
    fn corrupt_session_is_empty() {
        assert_eq!(Session::from_json("garbage"), Session::default());
    }
}
