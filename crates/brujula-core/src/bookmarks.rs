//! Bookmarks: a flat list, newest first. Only web addresses are kept, so a saved file cannot
//! make the browser open anything else.

use serde::{Deserialize, Serialize};

const MAX_BOOKMARKS: usize = 5000;
const MAX_TITLE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    /// When it was saved, in milliseconds since 1970 (0 for bookmarks from before this was
    /// kept). Sync uses it to tell an old bookmark from one saved after another device removed it.
    #[serde(default)]
    pub added: u64,
}

/// A bookmark that was removed, remembered for a while so that removing it on one device removes
/// it on the others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removal {
    pub url: String,
    pub at: u64,
}

const MAX_REMOVALS: usize = 2000;
/// How long a removal is remembered: a device that stays off longer than this brings the
/// bookmark back.
const REMOVAL_LIFETIME_MS: u64 = 90 * 24 * 3600 * 1000;

/// Milliseconds since 1970.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bookmarks {
    items: Vec<Bookmark>,
    removed: Vec<Removal>,
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The same page spelled two ways (a trailing slash, an anchor) is one bookmark.
fn same_page(a: &str, b: &str) -> bool {
    let key = |u: &str| {
        u.split('#')
            .next()
            .unwrap_or(u)
            .trim_end_matches('/')
            .to_owned()
    };
    key(a) == key(b)
}

impl Bookmarks {
    pub fn from_json(text: &str) -> Self {
        let mut b: Self = serde_json::from_str(text).unwrap_or_default();
        b.items.retain(|i| is_web(&i.url));
        for i in &mut b.items {
            i.title = i
                .title
                .chars()
                .filter(|c| !c.is_control())
                .take(MAX_TITLE)
                .collect();
        }
        b.items.truncate(MAX_BOOKMARKS);
        b.removed.retain(|r| is_web(&r.url));
        b.prune_removals(now_ms());
        b
    }

    /// Forgets removals that are too old (or too many).
    fn prune_removals(&mut self, now: u64) {
        self.removed
            .retain(|r| now.saturating_sub(r.at) < REMOVAL_LIFETIME_MS);
        self.removed.sort_by_key(|r| std::cmp::Reverse(r.at));
        self.removed.truncate(MAX_REMOVALS);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn items(&self) -> &[Bookmark] {
        &self.items
    }

    pub fn contains(&self, url: &str) -> bool {
        self.items.iter().any(|i| same_page(&i.url, url))
    }

    /// Adds a page, or removes it if it is already there. Returns whether it is bookmarked now.
    pub fn toggle(&mut self, url: &str, title: &str) -> bool {
        self.toggle_at(url, title, now_ms())
    }

    /// [`toggle`](Self::toggle) at a given time.
    pub fn toggle_at(&mut self, url: &str, title: &str, now: u64) -> bool {
        if self.items.iter().any(|i| same_page(&i.url, url)) {
            self.remove_at(url, now);
            return false;
        }
        if !is_web(url) {
            return false;
        }
        let title: String = title
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_TITLE)
            .collect();
        self.items.insert(
            0,
            Bookmark {
                url: url.to_owned(),
                title: if title.trim().is_empty() {
                    url.to_owned()
                } else {
                    title
                },
                added: now,
            },
        );
        self.items.truncate(MAX_BOOKMARKS);
        // Saving a page again cancels the memory of having removed it.
        self.removed.retain(|r| !same_page(&r.url, url));
        true
    }

    pub fn remove(&mut self, url: &str) -> bool {
        self.remove_at(url, now_ms())
    }

    /// Removes a page and remembers when, for the other devices.
    pub fn remove_at(&mut self, url: &str, at: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|i| !same_page(&i.url, url));
        let went = self.items.len() != before;
        if is_web(url) {
            self.removed.retain(|r| !same_page(&r.url, url));
            self.removed.push(Removal {
                url: url.to_owned(),
                at,
            });
            self.prune_removals(at);
        }
        went
    }

    /// The removals still remembered.
    pub fn removals(&self) -> &[Removal] {
        &self.removed
    }

    /// When the page was saved, if it is bookmarked.
    pub fn added_at(&self, url: &str) -> Option<u64> {
        self.items
            .iter()
            .find(|i| same_page(&i.url, url))
            .map(|i| i.added)
    }

    /// When the page was last removed, if it was.
    pub fn removed_at(&self, url: &str) -> Option<u64> {
        self.removed
            .iter()
            .filter(|r| same_page(&r.url, url))
            .map(|r| r.at)
            .max()
    }

    /// Adds a bookmark that came from another device. Does nothing if the page is there already
    /// or is not a web page. Keeps the list newest first.
    pub fn insert_synced(&mut self, url: &str, title: &str, added: u64) -> bool {
        if !is_web(url) || self.contains(url) || self.items.len() >= MAX_BOOKMARKS {
            return false;
        }
        let title: String = title
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_TITLE)
            .collect();
        self.items.push(Bookmark {
            url: url.to_owned(),
            title: if title.trim().is_empty() {
                url.to_owned()
            } else {
                title
            },
            added,
        });
        self.items.sort_by_key(|b| std::cmp::Reverse(b.added));
        true
    }

    /// Bookmarks whose title or address contains every word of `query`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Bookmark> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.items
            .iter()
            .filter(|i| {
                let hay = format!("{} {}", i.title, i.url).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(limit)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_adds_then_removes() {
        let mut b = Bookmarks::default();
        assert!(b.toggle("https://a.com/", "A"));
        assert!(b.contains("https://a.com"), "slash does not matter");
        assert!(b.contains("https://a.com/#x"), "anchor does not matter");
        assert!(!b.toggle("https://a.com#top", "A"));
        assert!(b.items().is_empty());
    }

    #[test]
    fn newest_first_and_blank_titles_use_the_address() {
        let mut b = Bookmarks::default();
        b.toggle("https://a.com/", "A");
        b.toggle("https://b.com/", "  ");
        assert_eq!(b.items()[0].title, "https://b.com/");
        assert_eq!(b.items()[1].title, "A");
    }

    #[test]
    fn only_web_pages_can_be_bookmarked() {
        let mut b = Bookmarks::default();
        assert!(!b.toggle("brujula://settings", "x"));
        assert!(!b.toggle("javascript:alert(1)", "x"));
        assert!(!b.toggle("file:///etc/passwd", "x"));
        assert!(b.items().is_empty());
    }

    #[test]
    fn a_file_cannot_smuggle_other_schemes_or_control_characters() {
        let b = Bookmarks::from_json(
            r#"{"items":[{"url":"file:///x","title":"x"},{"url":"https://ok.com","title":"a\u0007b"}]}"#,
        );
        assert_eq!(b.items().len(), 1);
        assert_eq!(b.items()[0].title, "ab");
        assert_eq!(Bookmarks::from_json("garbage"), Bookmarks::default());
    }

    #[test]
    fn search_needs_every_word() {
        let mut b = Bookmarks::default();
        b.toggle("https://rust-lang.org/", "Rust language");
        b.toggle("https://gtk.org/", "GTK");
        assert_eq!(b.search("rust lang", 5).len(), 1);
        assert_eq!(b.search("rust gtk", 5).len(), 0);
        assert_eq!(b.search("org", 1).len(), 1, "limited");
    }

    #[test]
    fn remove_reports_whether_anything_went() {
        let mut b = Bookmarks::default();
        b.toggle("https://a.com/", "A");
        assert!(b.remove("https://a.com"));
        assert!(!b.remove("https://a.com"));
    }
}
