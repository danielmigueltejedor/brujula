//! What the Spotlight offers for what the user has typed.

use crate::bookmarks::Bookmark;
use crate::history::Visit;
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
    /// Open the preferences at this section (see `prefs::SECTIONS`).
    Settings(String),
    /// Hand this text to the AI assistant (only when the user made it the search box).
    Ask(String),
}

/// Where a suggestion comes from, so the Spotlight can pick its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// What was typed: an address to visit or a search.
    Typed,
    Tab,
    Bookmark,
    History,
    Setting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub kind: Kind,
    pub action: Action,
    pub title: String,
    pub detail: String,
}

const MAX_TABS: usize = 6;
const MAX_BOOKMARKS: usize = 5;
const MAX_HISTORY: usize = 4;
const MAX_IDLE_BOOKMARKS: usize = 6;
const MAX_SETTINGS: usize = 4;

/// Preferences whose name or keywords contain every word typed.
fn settings_matches(query: &str) -> Vec<Suggestion> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| fold(&w.to_lowercase()))
        .collect();
    if words.is_empty() || words.iter().all(|w| w.chars().count() < 2) {
        return Vec::new();
    }
    crate::prefs::SECTIONS
        .iter()
        .filter(|(title, keywords, _)| {
            let hay = fold(&format!("{title} {keywords}").to_lowercase());
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .take(MAX_SETTINGS)
        .map(|(title, _, id)| Suggestion {
            kind: Kind::Setting,
            action: Action::Settings((*id).to_owned()),
            title: (*title).to_owned(),
            detail: format!("Preferencias · {}", crate::prefs::section_title(id)),
        })
        .collect()
}

/// Lowercase text without the accents of Spanish, so "pestanas" finds "pestañas".
fn fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// The first suggestion is what Enter does. Open tabs that match come after it.
pub fn suggestions(
    query: &str,
    tabs: &[Tab],
    bookmarks: &[Bookmark],
    search_template: &str,
) -> Vec<Suggestion> {
    suggestions_with_history(query, tabs, bookmarks, &[], search_template)
}

/// Like [`suggestions`], with the pages visited before as one more source: they come after the
/// bookmarks, and never repeat a page that is a tab, a bookmark or what was typed.
pub fn suggestions_with_history(
    query: &str,
    tabs: &[Tab],
    bookmarks: &[Bookmark],
    history: &[&Visit],
    search_template: &str,
) -> Vec<Suggestion> {
    let q = query.trim();
    let mut out = Vec::new();
    let bookmark = |b: &Bookmark| Suggestion {
        kind: Kind::Bookmark,
        action: Action::Go(b.url.clone()),
        title: b.title.clone(),
        detail: b.url.clone(),
    };
    if q.is_empty() {
        // Nothing typed yet: the bookmarks, newest first.
        return bookmarks
            .iter()
            .take(MAX_IDLE_BOOKMARKS)
            .map(bookmark)
            .collect();
    }

    if let Some(url) = omnibox::resolve(q, search_template) {
        let is_search = !omnibox::looks_like_address(q);
        out.push(Suggestion {
            kind: Kind::Typed,
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
            kind: Kind::Tab,
            action: Action::SwitchTab(t.id),
            title: if t.title.is_empty() {
                t.url.clone()
            } else {
                t.title.clone()
            },
            detail: t.url.clone(),
        })
        .collect::<Vec<_>>();
    // A bookmark of a page that is open already is the same row: the tab wins.
    let open_urls: Vec<String> = matches
        .iter()
        .filter_map(|m| match m.action {
            Action::SwitchTab(_) => Some(m.detail.clone()),
            _ => None,
        })
        .collect();
    out.append(&mut matches);
    let needle_words: Vec<&str> = needle.split_whitespace().collect();
    let listed: Vec<Action> = out.iter().map(|o| o.action.clone()).collect();
    out.extend(
        bookmarks
            .iter()
            .filter(|b| {
                let hay = format!("{} {}", b.title, b.url).to_lowercase();
                needle_words.iter().all(|w| hay.contains(w))
            })
            .filter(|b| !listed.contains(&Action::Go(b.url.clone())))
            .filter(|b| !open_urls.contains(&b.url))
            .take(MAX_BOOKMARKS)
            .map(bookmark),
    );
    let shown: Vec<String> = out
        .iter()
        .filter_map(|o| match &o.action {
            Action::Go(u) => Some(u.trim_end_matches('/').to_owned()),
            _ => None,
        })
        .chain(open_urls.iter().map(|u| u.trim_end_matches('/').to_owned()))
        .collect();
    out.extend(
        history
            .iter()
            .filter(|h| !shown.contains(&h.url.trim_end_matches('/').to_owned()))
            .take(MAX_HISTORY)
            .map(|h| Suggestion {
                kind: Kind::History,
                action: Action::Go(h.url.clone()),
                title: if h.title.is_empty() {
                    h.url.clone()
                } else {
                    h.title.clone()
                },
                detail: h.url.clone(),
            }),
    );
    out.append(&mut settings_matches(q));
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
            space: 1,
            folder: None,
            active: 0,
            essential: None,
        }
    }

    const T: &str = ENGINES[0].template;

    #[test]
    fn blank_query_offers_nothing() {
        assert!(suggestions("  ", &[], &[], T).is_empty());
    }

    #[test]
    fn words_become_a_search_and_addresses_a_visit() {
        let s = suggestions("rust gtk", &[], &[], T);
        assert_eq!(s[0].action, Action::Search("rust gtk".into()));
        let s = suggestions("example.com", &[], &[], T);
        assert_eq!(s[0].action, Action::Go("https://example.com/".into()));
    }

    #[test]
    fn matching_tabs_follow_the_first_suggestion() {
        let tabs = [
            tab(1, "YouTube", "https://youtube.com/"),
            tab(2, "Otra", "https://otra.com/"),
        ];
        let s = suggestions("youtu", &tabs, &[], T);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].action, Action::SwitchTab(TabId(1)));
    }

    #[test]
    fn preferences_are_found_by_name_or_keyword_without_accents() {
        let s = suggestions("pestanas arriba", &[], &[], T);
        assert!(s
            .iter()
            .any(|x| x.action == Action::Settings("layout".into())));
        let s = suggestions("transparencia", &[], &[], T);
        assert!(s
            .iter()
            .any(|x| matches!(&x.action, Action::Settings(id) if id == "appearance")));
        assert!(suggestions("zzzz qqqq", &[], &[], T)
            .iter()
            .all(|x| !matches!(x.action, Action::Settings(_))));
    }

    #[test]
    fn one_letter_does_not_flood_the_list_with_preferences() {
        assert!(suggestions("a", &[], &[], T)
            .iter()
            .all(|x| !matches!(x.action, Action::Settings(_))));
    }

    fn bm(url: &str, title: &str) -> Bookmark {
        Bookmark {
            url: url.into(),
            title: title.into(),
            added: 0,
        }
    }

    #[test]
    fn bookmarks_show_when_nothing_is_typed_and_match_by_title_or_address() {
        let marks = [
            bm("https://rust-lang.org/", "Rust"),
            bm("https://gtk.org/", "GTK"),
        ];
        let idle = suggestions("", &[], &marks, T);
        assert_eq!(idle.len(), 2);
        assert!(idle.iter().all(|s| s.kind == Kind::Bookmark));
        let s = suggestions("gtk", &[], &marks, T);
        assert!(
            s.iter()
                .any(|x| x.kind == Kind::Bookmark
                    && x.action == Action::Go("https://gtk.org/".into()))
        );
    }

    #[test]
    fn a_bookmark_is_not_listed_twice_when_it_is_what_was_typed() {
        let marks = [bm("https://example.com/", "Example")];
        let s = suggestions("example.com", &[], &marks, T);
        let visits = s
            .iter()
            .filter(|x| x.action == Action::Go("https://example.com/".into()))
            .count();
        assert_eq!(visits, 1);
    }

    #[test]
    fn a_bookmark_of_an_open_page_is_not_listed_again() {
        let tabs = [tab(1, "Rust", "https://www.rust-lang.org/")];
        let marks = [
            bm("https://www.rust-lang.org/", "Rust"),
            bm("https://rust.example/", "Rust blog"),
        ];
        let s = suggestions("rust", &tabs, &marks, T);
        let rows = |k: Kind| s.iter().filter(|x| x.kind == k).count();
        assert_eq!(rows(Kind::Tab), 1);
        assert_eq!(rows(Kind::Bookmark), 1);
    }

    fn visit(url: &str, title: &str) -> Visit {
        Visit {
            url: url.into(),
            title: title.into(),
            visits: 1,
            last: 1,
        }
    }

    #[test]
    fn visited_pages_follow_bookmarks_and_are_not_repeated() {
        let marks = [bm("https://rust-lang.org/", "Rust")];
        let tabs = [tab(1, "GTK", "https://gtk.org/")];
        let (a, b, c) = (
            visit("https://rust-lang.org", "Rust"),
            visit("https://gtk.org/", "GTK"),
            visit("https://rust.example/blog", "Rust blog"),
        );
        let s = suggestions_with_history("rust", &tabs, &marks, &[&a, &c], T);
        let history: Vec<_> = s.iter().filter(|x| x.kind == Kind::History).collect();
        assert_eq!(history.len(), 1);
        assert_eq!(
            history[0].action,
            Action::Go("https://rust.example/blog".into())
        );
        let s = suggestions_with_history("gtk", &tabs, &[], &[&b], T);
        assert!(s.iter().all(|x| x.kind != Kind::History));
    }

    #[test]
    fn visited_pages_are_limited() {
        let pages: Vec<Visit> = (0..10)
            .map(|i| visit(&format!("https://r{i}.example/"), "Rust"))
            .collect();
        let refs: Vec<&Visit> = pages.iter().collect();
        let s = suggestions_with_history("rust", &[], &[], &refs, T);
        assert_eq!(
            s.iter().filter(|x| x.kind == Kind::History).count(),
            MAX_HISTORY
        );
    }

    #[test]
    fn blank_tabs_never_match() {
        let tabs = [tab(1, "", "about:blank")];
        assert_eq!(suggestions("about", &tabs, &[], T).len(), 1);
    }

    #[test]
    fn tab_matches_are_limited() {
        let tabs: Vec<Tab> = (0..20)
            .map(|i| tab(i, "Rust", &format!("https://r{i}.com/")))
            .collect();
        assert_eq!(suggestions("rust", &tabs, &[], T).len(), 1 + MAX_TABS);
    }
}
