//! Bookmarks: a flat list, newest first. Only web addresses are kept, so a saved file cannot
//! make the browser open anything else.

use serde::{Deserialize, Serialize};

const MAX_BOOKMARKS: usize = 5000;
const MAX_TITLE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bookmarks {
    items: Vec<Bookmark>,
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
        b
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
        if let Some(at) = self.items.iter().position(|i| same_page(&i.url, url)) {
            self.items.remove(at);
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
            },
        );
        self.items.truncate(MAX_BOOKMARKS);
        true
    }

    pub fn remove(&mut self, url: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|i| !same_page(&i.url, url));
        self.items.len() != before
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
