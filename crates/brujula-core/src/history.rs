//! History: the web pages visited, newest first, kept on this computer only. Only web addresses
//! are kept, so a saved file cannot make the browser open anything else, and a page seen in a
//! private window never gets here.

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 5000;
const MAX_TITLE: usize = 200;
const MAX_URL: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    pub url: String,
    pub title: String,
    /// How many times it was opened.
    #[serde(default)]
    pub visits: u32,
    /// The last time it was opened, in milliseconds since 1970.
    #[serde(default)]
    pub last: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    items: Vec<Visit>,
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The same page spelled two ways (a trailing slash, an anchor) is one entry.
fn key(url: &str) -> &str {
    url.split('#').next().unwrap_or(url).trim_end_matches('/')
}

fn clean_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE)
        .collect()
}

impl History {
    pub fn from_json(text: &str) -> Self {
        Self::parse(text).unwrap_or_default()
    }

    /// Reads a saved history, or `None` when the text is not one. The browser then keeps the
    /// file aside instead of writing an empty history over it.
    pub fn parse(text: &str) -> Option<Self> {
        let mut h: Self = serde_json::from_str(text).ok()?;
        h.items.retain(|i| is_web(&i.url) && i.url.len() <= MAX_URL);
        for i in &mut h.items {
            i.title = clean_title(&i.title);
        }
        h.items.sort_by_key(|i| std::cmp::Reverse(i.last));
        h.items.truncate(MAX_ENTRIES);
        Some(h)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Notes that `url` was opened at `now`. Returns false for what is not kept.
    pub fn record(&mut self, url: &str, title: &str, now: u64) -> bool {
        if !is_web(url) || url.len() > MAX_URL {
            return false;
        }
        let title = clean_title(title);
        let existing = self.items.iter().position(|i| key(&i.url) == key(url));
        let mut entry = match existing {
            Some(at) => self.items.remove(at),
            None => Visit {
                url: url.to_owned(),
                title: String::new(),
                visits: 0,
                last: 0,
            },
        };
        entry.url = url.to_owned();
        if !title.is_empty() {
            entry.title = title;
        }
        entry.visits = entry.visits.saturating_add(1);
        entry.last = now;
        self.items.insert(0, entry);
        self.items.truncate(MAX_ENTRIES);
        true
    }

    /// Adds the pages visited in another browser. A page that is already here keeps its entry
    /// (the visits add up, the later time wins); the rest are put in by date. Returns how many
    /// pages were new. What does not fit in the history is the oldest.
    pub fn import(&mut self, visits: impl IntoIterator<Item = Visit>) -> usize {
        let mut at: std::collections::HashMap<String, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, v)| (key(&v.url).to_owned(), i))
            .collect();
        let mut added = 0;
        for v in visits {
            if !is_web(&v.url) || v.url.len() > MAX_URL {
                continue;
            }
            match at.get(key(&v.url)) {
                Some(&i) => {
                    let mine = &mut self.items[i];
                    mine.visits = mine.visits.saturating_add(v.visits);
                    mine.last = mine.last.max(v.last);
                    if mine.title.is_empty() {
                        mine.title = clean_title(&v.title);
                    }
                }
                None => {
                    at.insert(key(&v.url).to_owned(), self.items.len());
                    self.items.push(Visit {
                        title: clean_title(&v.title),
                        ..v
                    });
                    added += 1;
                }
            }
        }
        self.items.sort_by_key(|i| std::cmp::Reverse(i.last));
        self.items.truncate(MAX_ENTRIES);
        added
    }

    /// Gives a title to a page that already has an entry (the title often arrives after the
    /// page does).
    /// Returns whether anything changed: pages that keep setting the same title (a video in a
    /// tab at the back, a chat) do not ask for the history to be written again.
    pub fn set_title(&mut self, url: &str, title: &str) -> bool {
        let title = clean_title(title);
        if title.is_empty() {
            return false;
        }
        match self.items.iter_mut().find(|i| key(&i.url) == key(url)) {
            Some(i) if i.title != title => {
                i.title = title;
                true
            }
            _ => false,
        }
    }

    pub fn items(&self) -> &[Visit] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn remove(&mut self, url: &str) {
        self.items.retain(|i| key(&i.url) != key(url));
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Forgets what was visited at or after `since`.
    pub fn clear_since(&mut self, since: u64) {
        self.items.retain(|i| i.last < since);
    }

    /// Entries whose title or address contains every word of `query`, the most visited and
    /// most recent first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Visit> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(usize, &Visit)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                let hay = format!("{} {}", i.title, i.url).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect();
        // Stable: ties keep the newest first.
        found.sort_by(|a, b| b.1.visits.cmp(&a.1.visits).then(a.0.cmp(&b.0)));
        found.into_iter().take(limit).map(|(_, v)| v).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_visited_again_moves_to_the_front_and_counts() {
        let mut h = History::default();
        h.record("https://a.example/", "A", 1);
        h.record("https://b.example/", "B", 2);
        h.record("https://a.example/#top", "", 3);
        assert_eq!(h.len(), 2);
        assert_eq!(h.items()[0].url, "https://a.example/#top");
        assert_eq!(h.items()[0].visits, 2);
        // An empty title does not erase the one known.
        assert_eq!(h.items()[0].title, "A");
    }

    #[test]
    fn only_web_pages_are_kept() {
        let mut h = History::default();
        assert!(!h.record("javascript:alert(1)", "x", 1));
        assert!(!h.record("file:///etc/passwd", "x", 1));
        assert!(!h.record("brujula-ext://abc/page.html", "x", 1));
        assert!(h.is_empty());
        let loaded = History::from_json(
            r#"{"items":[{"url":"data:text/html,hi","title":"x"},{"url":"http://ok.example/","title":"ok","last":5}]}"#,
        );
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn search_needs_every_word_and_prefers_what_is_visited_most() {
        let mut h = History::default();
        h.record("https://rust-lang.org/", "Rust", 1);
        h.record("https://gtk.org/", "GTK toolkit", 2);
        h.record("https://rust-lang.org/", "Rust", 3);
        h.record("https://rust.example/blog", "Rust blog", 4);
        let found = h.search("rust", 10);
        assert_eq!(found[0].url, "https://rust-lang.org/");
        assert_eq!(found.len(), 2);
        assert!(h.search("rust gtk", 10).is_empty());
        assert!(h.search("   ", 10).is_empty());
        assert_eq!(h.search("rust", 1).len(), 1);
    }

    #[test]
    fn titles_arrive_late_and_are_cleaned() {
        let mut h = History::default();
        h.record("https://a.example/", "", 1);
        assert!(h.set_title("https://a.example", "Nuevo\ntítulo"));
        assert_eq!(h.items()[0].title, "Nuevotítulo");
    }

    #[test]
    fn a_title_that_is_already_there_changes_nothing() {
        let mut h = History::default();
        h.record("https://a.example/", "A", 1);
        // The same title again, an empty one and a page that was never visited.
        assert!(!h.set_title("https://a.example/", "A"));
        assert!(!h.set_title("https://a.example/", ""));
        assert!(!h.set_title("https://nowhere.example/", "A"));
        assert!(h.set_title("https://a.example/", "B"));
        assert_eq!(h.items()[0].title, "B");
    }

    #[test]
    fn forgetting_by_page_by_time_and_all() {
        let mut h = History::default();
        h.record("https://a.example/", "A", 10);
        h.record("https://b.example/", "B", 20);
        h.record("https://c.example/", "C", 30);
        h.clear_since(20);
        assert_eq!(h.len(), 1);
        h.remove("https://a.example");
        assert!(h.is_empty());
        h.record("https://a.example/", "A", 1);
        h.clear();
        assert!(h.is_empty());
    }

    #[test]
    fn the_list_is_limited_and_survives_a_round_trip() {
        let mut h = History::default();
        for i in 0..(MAX_ENTRIES + 10) {
            h.record(&format!("https://e{i}.example/"), "", i as u64);
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        let again = History::from_json(&h.to_json());
        assert_eq!(again, h);
    }

    #[test]
    fn imported_pages_merge_with_what_is_here_and_stay_in_order() {
        let mut h = History::default();
        h.record("https://a.example/", "A", 5_000);
        let visit = |url: &str, title: &str, visits: u32, last: u64| Visit {
            url: url.into(),
            title: title.into(),
            visits,
            last,
        };
        let added = h.import(vec![
            visit("https://a.example", "", 4, 9_000),
            visit("https://b.example/", "B\u{7}", 2, 7_000),
            visit("javascript:alert(1)", "x", 1, 8_000),
            visit("https://old.example/", "Old", 1, 1_000),
        ]);
        assert_eq!(added, 2);
        let urls: Vec<&str> = h.items().iter().map(|v| v.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://a.example/",
                "https://b.example/",
                "https://old.example/"
            ]
        );
        // The existing entry gained the visits and the later time, and kept its title.
        assert_eq!((h.items()[0].visits, h.items()[0].last), (5, 9_000));
        assert_eq!(h.items()[0].title, "A");
        assert_eq!(h.items()[1].title, "B");
    }

    #[test]
    fn importing_never_grows_the_history_past_its_limit() {
        let mut h = History::default();
        let many = (0..MAX_ENTRIES as u64 + 100).map(|n| Visit {
            url: format!("https://s{n}.example/"),
            title: String::new(),
            visits: 1,
            last: n,
        });
        h.import(many);
        assert_eq!(h.len(), MAX_ENTRIES);
        // The newest are the ones kept.
        assert_eq!(h.items()[0].last, MAX_ENTRIES as u64 + 99);
    }

    #[test]
    fn a_saved_history_reads_back_and_a_broken_one_is_told_apart() {
        let mut h = History::default();
        h.record("https://example.com/", "Example", 5);
        let saved = h.to_json();
        assert_eq!(History::parse(&saved), Some(h));
        // Neither a cut-off file nor something else may pass for an empty history, or the next
        // save would write that over the pages the person had.
        assert_eq!(History::parse(&saved[..saved.len() / 2]), None);
        assert_eq!(History::parse("not json"), None);
        assert_eq!(History::parse("{}"), Some(History::default()));
    }
}
