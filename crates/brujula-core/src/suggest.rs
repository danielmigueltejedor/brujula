//! What the Spotlight offers for what the user has typed.

use crate::omnibox;
use crate::tabs::{Tab, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Load this URL.
    Go(String),
    /// Search for this text.
    Search(String),
    /// Switch to an open tab.
    SwitchTab(TabId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub action: Action,
    pub title: String,
    pub detail: String,
}

const MAX_TABS: usize = 6;

/// The first suggestion is what Enter does. Open tabs that match come after it.
pub fn suggestions(query: &str, tabs: &[Tab], search_template: &str) -> Vec<Suggestion> {
    let q = query.trim();
    let mut out = Vec::new();
    if q.is_empty() {
        return out;
    }

    if let Some(url) = omnibox::resolve(q, search_template) {
        let is_search = !omnibox::looks_like_address(q);
        out.push(Suggestion {
            action: if is_search {
                Action::Search(q.to_owned())
            } else {
                Action::Go(url.to_string())
            },
            title: q.to_owned(),
            detail: String::new(),
        });
    }

    let needle = q.to_lowercase();
    let mut matches = tabs
        .iter()
        .filter(|t| {
            t.url != "about:blank"
                && (t.title.to_lowercase().contains(&needle)
                    || t.url.to_lowercase().contains(&needle))
        })
        .take(MAX_TABS)
        .map(|t| Suggestion {
            action: Action::SwitchTab(t.id),
            title: if t.title.is_empty() {
                t.url.clone()
            } else {
                t.title.clone()
            },
            detail: t.url.clone(),
        })
        .collect::<Vec<_>>();
    out.append(&mut matches);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::ENGINES;

    fn tab(id: u64, title: &str, url: &str) -> Tab {
        Tab {
            id: TabId(id),
            title: title.into(),
            url: url.into(),
            pinned: false,
            loading: false,
            audible: false,
        }
    }

    const T: &str = ENGINES[0].template;

    #[test]
    fn blank_query_offers_nothing() {
        assert!(suggestions("  ", &[], T).is_empty());
    }

    #[test]
    fn words_become_a_search_and_addresses_a_visit() {
        let s = suggestions("rust gtk", &[], T);
        assert_eq!(s[0].action, Action::Search("rust gtk".into()));
        let s = suggestions("example.com", &[], T);
        assert_eq!(s[0].action, Action::Go("https://example.com/".into()));
    }

    #[test]
    fn matching_tabs_follow_the_first_suggestion() {
        let tabs = [
            tab(1, "YouTube", "https://youtube.com/"),
            tab(2, "Otra", "https://otra.com/"),
        ];
        let s = suggestions("youtu", &tabs, T);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].action, Action::SwitchTab(TabId(1)));
    }

    #[test]
    fn blank_tabs_never_match() {
        let tabs = [tab(1, "", "about:blank")];
        assert_eq!(suggestions("about", &tabs, T).len(), 1);
    }

    #[test]
    fn tab_matches_are_limited() {
        let tabs: Vec<Tab> = (0..20)
            .map(|i| tab(i, "Rust", &format!("https://r{i}.com/")))
            .collect();
        assert_eq!(suggestions("rust", &tabs, T).len(), 1 + MAX_TABS);
    }
}
