//! Tabs that were left alone long enough are archived: closed, but kept in a list from which
//! they can be brought back. Like Arc, pinned tabs, tabs in a folder, the tab in front, tabs that
//! play sound or are loading, and pages that are not web pages are never archived.

use serde::{Deserialize, Serialize};

use crate::folders::Folders;
use crate::tabs::{TabId, TabModel};

const MAX_ENTRIES: usize = 200;
const HOUR_MS: u64 = 3_600_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// The space the tab lived in.
    #[serde(default)]
    pub space: u32,
    /// When it was archived, in milliseconds since 1970.
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Archive {
    items: Vec<Entry>,
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

impl Archive {
    pub fn from_json(text: &str) -> Self {
        let mut a: Self = serde_json::from_str(text).unwrap_or_default();
        a.items.retain(|e| is_web(&e.url));
        a.items.truncate(MAX_ENTRIES);
        for e in &mut a.items {
            e.title = e
                .title
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect();
        }
        a
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Newest first.
    pub fn items(&self) -> &[Entry] {
        &self.items
    }

    /// Adds a page; the same address is listed once, at the top.
    pub fn push(&mut self, entry: Entry) {
        if !is_web(&entry.url) {
            return;
        }
        self.items.retain(|e| e.url != entry.url);
        self.items.insert(0, entry);
        self.items.truncate(MAX_ENTRIES);
    }

    /// Takes the page with this address out of the list.
    pub fn take(&mut self, url: &str) -> Option<Entry> {
        let at = self.items.iter().position(|e| e.url == url)?;
        Some(self.items.remove(at))
    }
}

/// The tabs to archive at `now` when the limit is `after_hours` (0: none).
pub fn stale(model: &TabModel, folders: &Folders, now: u64, after_hours: u32) -> Vec<TabId> {
    if after_hours == 0 {
        return Vec::new();
    }
    let limit = u64::from(after_hours) * HOUR_MS;
    model
        .tabs()
        .iter()
        .filter(|t| {
            !t.pinned
                && t.essential.is_none()
                && Some(t.id) != model.selected()
                && !t.audible
                && !t.loading
                && is_web(&t.url)
                && t.active > 0
                && now.saturating_sub(t.active) >= limit
                && folders
                    .folder_of(t.folder, t.space, &t.url, &t.title)
                    .is_none()
        })
        .map(|t| t.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folders::Rule;
    use crate::spaces::FIRST_SPACE;

    const NOW: u64 = 100 * HOUR_MS;

    fn entry(url: &str) -> Entry {
        Entry {
            url: url.into(),
            title: "t".into(),
            space: 1,
            at: 1,
        }
    }

    fn model(ages_h: &[u64]) -> (TabModel, Vec<TabId>) {
        let mut m = TabModel::new();
        let mut ids = Vec::new();
        for (i, age) in ages_h.iter().enumerate() {
            let (id, _) = m.open(&format!("https://t{i}.example/"), None, false);
            m.touch(id, NOW - age * HOUR_MS);
            ids.push(id);
        }
        (m, ids)
    }

    #[test]
    fn only_tabs_older_than_the_limit_are_archived() {
        let (m, ids) = model(&[1, 30, 50]);
        // The first tab is selected (first tab opened); it is also the fresh one.
        let s = stale(&m, &Folders::default(), NOW, 24);
        assert_eq!(s, vec![ids[1], ids[2]]);
        assert!(stale(&m, &Folders::default(), NOW, 0).is_empty());
        assert_eq!(stale(&m, &Folders::default(), NOW, 40), vec![ids[2]]);
    }

    #[test]
    fn the_tab_in_front_pinned_noisy_loading_and_unknown_age_stay() {
        let (mut m, ids) = model(&[50, 50, 50, 50, 50]);
        m.select(ids[1]);
        m.set_pinned(ids[2], true);
        m.update(ids[3], |t| t.audible = true);
        m.update(ids[4], |t| t.loading = true);
        let (unknown, _) = m.open("https://unknown.example/", None, false);
        assert_eq!(unknown.0, ids[4].0 + 1);
        // 0 is "never in front that we know of": not archived on a guess.
        assert_eq!(stale(&m, &Folders::default(), NOW, 24), vec![ids[0]]);
    }

    #[test]
    fn tabs_in_folders_and_other_pages_stay() {
        let (mut m, ids) = model(&[50, 50, 50]);
        m.select(ids[2]);
        m.update(ids[0], |t| t.url = "brujula://settings".into());
        let mut folders = Folders::default();
        folders
            .add(FIRST_SPACE, "t1", Rule::Site("t1.example".into()))
            .unwrap();
        assert!(stale(&m, &folders, NOW, 24).is_empty());
        let _ = ids;
    }

    #[test]
    fn the_list_keeps_each_page_once_newest_first_and_is_bounded() {
        let mut a = Archive::default();
        a.push(entry("https://a.example/"));
        a.push(entry("https://b.example/"));
        a.push(entry("https://a.example/"));
        assert_eq!(a.items()[0].url, "https://a.example/");
        assert_eq!(a.items().len(), 2);
        a.push(entry("javascript:alert(1)"));
        assert_eq!(a.items().len(), 2);
        assert!(a.take("https://b.example/").is_some());
        assert!(a.take("https://b.example/").is_none());
        for i in 0..300 {
            a.push(entry(&format!("https://x{i}.example/")));
        }
        assert_eq!(a.items().len(), MAX_ENTRIES);
    }

    #[test]
    fn a_file_with_bad_entries_is_cleaned() {
        let a = Archive::from_json(
            r#"{"items":[{"url":"file:///etc/passwd"},{"url":"https://ok.example/","title":"a\u0007b"}]}"#,
        );
        assert_eq!(a.items().len(), 1);
        assert_eq!(a.items()[0].title, "ab");
        assert!(Archive::from_json("{no").items().is_empty());
        assert_eq!(Archive::from_json(&a.to_json()), a);
    }
}
