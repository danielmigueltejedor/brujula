//! A notepad per space, like the notes of Arc: text to keep at hand next to the tabs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Longest note, in characters.
pub const MAX_CHARS: usize = 20_000;
const MAX_SPACES: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notes {
    /// Space id to its text.
    by_space: BTreeMap<u32, String>,
}

/// Text as a note keeps it: line breaks and tabs stay, other control characters go, and it is
/// cut at [`MAX_CHARS`].
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(MAX_CHARS)
        .collect()
}

impl Notes {
    pub fn from_json(text: &str) -> Self {
        let mut n: Self = serde_json::from_str(text).unwrap_or_default();
        n.by_space = std::mem::take(&mut n.by_space)
            .into_iter()
            .map(|(id, t)| (id, clean(&t)))
            .filter(|(_, t)| !t.trim().is_empty())
            .take(MAX_SPACES)
            .collect();
        n
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn get(&self, space: u32) -> &str {
        self.by_space.get(&space).map_or("", String::as_str)
    }

    /// Sets the note of a space; returns whether it changed.
    pub fn set(&mut self, space: u32, text: &str) -> bool {
        let text = clean(text);
        if self.get(space) == text {
            return false;
        }
        if text.trim().is_empty() {
            self.by_space.remove(&space);
        } else if self.by_space.contains_key(&space) || self.by_space.len() < MAX_SPACES {
            self.by_space.insert(space, text);
        } else {
            return false;
        }
        true
    }

    /// Forgets the note of a space that was deleted.
    pub fn forget(&mut self, space: u32) {
        self.by_space.remove(&space);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_kept_per_space_and_reports_changes() {
        let mut n = Notes::default();
        assert!(n.set(1, "comprar pan"));
        assert!(!n.set(1, "comprar pan"));
        assert!(n.set(2, "otra"));
        assert_eq!((n.get(1), n.get(2), n.get(3)), ("comprar pan", "otra", ""));
        assert!(n.set(1, "  \n"));
        assert_eq!(n.get(1), "");
        n.forget(2);
        assert_eq!(n.get(2), "");
    }

    #[test]
    fn notes_are_cleaned_and_bounded() {
        let mut n = Notes::default();
        n.set(1, "a\u{7}b\nc\td\u{1b}");
        assert_eq!(n.get(1), "ab\nc\td");
        n.set(1, &"x".repeat(MAX_CHARS * 2));
        assert_eq!(n.get(1).chars().count(), MAX_CHARS);
        for id in 0..100 {
            n.set(id + 10, "t");
        }
        assert!(n.to_json().matches("\"t\"").count() <= MAX_SPACES);
    }

    #[test]
    fn a_file_is_read_forgivingly() {
        assert_eq!(Notes::from_json("{no"), Notes::default());
        let n = Notes::from_json(r#"{"by_space":{"1":"hola\u0000","2":"  "}}"#);
        assert_eq!(n.get(1), "hola");
        assert_eq!(n.get(2), "");
        assert_eq!(Notes::from_json(&n.to_json()), n);
    }
}
