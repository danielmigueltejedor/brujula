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
        let mut h: Self = serde_json::from_str(text).unwrap_or_default();
        h.items.retain(|i| is_web(&i.url) && i.url.len() <= MAX_URL);
        for i in &mut h.items {
            i.title = clean_title(&i.title);
        }
        h.items.sort_by_key(|i| std::cmp::Reverse(i.last));
        h.items.truncate(MAX_ENTRIES);
        h
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

    /// Gives a title to a page that already has an entry (the title often arrives after the
    /// page does).
    pub fn set_title(&mut self, url: &str, title: &str) {
        let title = clean_title(title);
        if title.is_empty() {
            return;
        }
        if let Some(i) = self.items.iter_mut().find(|i| key(&i.url) == key(url)) {
            i.title = title;
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
        h.set_title("https://a.example", "Nuevo\ntítulo");
        assert_eq!(h.items()[0].title, "Nuevotítulo");
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
}
