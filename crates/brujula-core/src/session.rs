//! The tabs to bring back on the next start.

use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    pub selected: usize,
    pub spaces: Spaces,
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
    pub fn capture(model: &TabModel, spaces: &Spaces) -> Self {
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
            })
            .collect();
        Self {
            tabs,
            selected,
            spaces: spaces.clone(),
        }
    }

    pub fn from_json(text: &str) -> Self {
        let mut s: Session = serde_json::from_str(text).unwrap_or_default();
        s.tabs.retain(|t| restorable(&t.url));
        s.spaces = s.spaces.sanitized();
        let home = s.spaces.all()[0].id;
        for t in &mut s.tabs {
            if !s.spaces.contains(t.space) {
                t.space = home;
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
        let s = Session::capture(&m, &Spaces::default());
        assert_eq!(s.tabs.len(), 2);
        assert!(s.tabs[0].pinned);
        assert_eq!(s.selected, 1);
        assert_eq!(Session::from_json(&s.to_json()), s);
        let _ = b;
    }

    #[test]
    fn tabs_keep_their_space_and_old_files_land_in_the_first() {
        let mut spaces = Spaces::default();
        let work = spaces.add("Trabajo").unwrap();
        let mut m = TabModel::new();
        m.open("https://a.com", None, false);
        m.set_current_space(work);
        m.open("https://b.com", None, true);
        let s = Session::from_json(&Session::capture(&m, &spaces).to_json());
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
