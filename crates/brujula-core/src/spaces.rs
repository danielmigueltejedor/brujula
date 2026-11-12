//! Spaces: named groups of tabs, like Zen's workspaces. Each tab lives in one space and the
//! sidebar shows the tabs of the space in front.

use serde::{Deserialize, Serialize};

pub const FIRST_SPACE: u32 = 1;
const MAX_SPACES: usize = 12;
const MAX_NAME: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: u32,
    pub name: String,
    /// One emoji or letter shown on the switcher; empty shows the name's first letter.
    #[serde(default)]
    pub icon: String,
}

impl Space {
    /// What the switcher button shows.
    pub fn badge(&self) -> String {
        if !self.icon.is_empty() {
            return self.icon.clone();
        }
        self.name
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_else(|| "·".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spaces {
    list: Vec<Space>,
    current: u32,
}

impl Default for Spaces {
    fn default() -> Self {
        Self {
            list: vec![Space {
                id: FIRST_SPACE,
                name: "Principal".to_owned(),
                icon: String::new(),
            }],
            current: FIRST_SPACE,
        }
    }
}

/// Text from a form or a file, limited to what fits on a button.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

impl Spaces {
    /// Reads spaces back from a saved session: never empty, ids unique, the current one exists.
    pub fn sanitized(mut self) -> Self {
        let mut seen = Vec::new();
        self.list.retain(|s| {
            let fresh = !seen.contains(&s.id) && s.id != 0;
            seen.push(s.id);
            fresh
        });
        self.list.truncate(MAX_SPACES);
        for s in &mut self.list {
            s.name = clean(&s.name, MAX_NAME);
            if s.name.is_empty() {
                s.name = "Espacio".to_owned();
            }
            s.icon = clean(&s.icon, 2);
        }
        if self.list.is_empty() {
            return Self::default();
        }
        if !self.list.iter().any(|s| s.id == self.current) {
            self.current = self.list[0].id;
        }
        self
    }

    pub fn all(&self) -> &[Space] {
        &self.list
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn get(&self, id: u32) -> Option<&Space> {
        self.list.iter().find(|s| s.id == id)
    }

    pub fn contains(&self, id: u32) -> bool {
        self.get(id).is_some()
    }

    pub fn set_current(&mut self, id: u32) -> bool {
        if self.contains(id) && self.current != id {
            self.current = id;
            return true;
        }
        false
    }

    /// A new space after the others, if there is room. Returns its id.
    pub fn add(&mut self, name: &str) -> Option<u32> {
        if self.list.len() >= MAX_SPACES {
            return None;
        }
        let id = self.list.iter().map(|s| s.id).max().unwrap_or(0) + 1;
        let mut name = clean(name, MAX_NAME);
        if name.is_empty() {
            name = format!("Espacio {}", self.list.len() + 1);
        }
        self.list.push(Space {
            id,
            name,
            icon: String::new(),
        });
        Some(id)
    }

    pub fn rename(&mut self, id: u32, name: &str, icon: &str) {
        let name = clean(name, MAX_NAME);
        if let Some(s) = self.list.iter_mut().find(|s| s.id == id) {
            if !name.is_empty() {
                s.name = name;
            }
            s.icon = clean(icon, 2);
        }
    }

    /// Removes a space; the last one cannot be removed. Returns where its tabs should go.
    pub fn remove(&mut self, id: u32) -> Option<u32> {
        if self.list.len() <= 1 {
            return None;
        }
        let at = self.list.iter().position(|s| s.id == id)?;
        self.list.remove(at);
        let heir = self.list[at.saturating_sub(1).min(self.list.len() - 1)].id;
        if self.current == id {
            self.current = heir;
        }
        Some(heir)
    }

    /// The space after (`+1`) or before (`-1`) the current one, wrapping.
    pub fn relative(&self, delta: isize) -> u32 {
        let at = self
            .list
            .iter()
            .position(|s| s.id == self.current)
            .unwrap_or(0) as isize;
        let n = self.list.len() as isize;
        self.list[(at + delta).rem_euclid(n) as usize].id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_one_space() {
        let s = Spaces::default();
        assert_eq!(s.all().len(), 1);
        assert_eq!(s.current(), FIRST_SPACE);
    }

    #[test]
    fn adding_gives_fresh_ids_and_default_names() {
        let mut s = Spaces::default();
        let a = s.add("").unwrap();
        let b = s.add("  Trabajo  ").unwrap();
        assert_ne!(a, b);
        assert_eq!(s.get(b).unwrap().name, "Trabajo");
        assert_eq!(s.get(a).unwrap().name, "Espacio 2");
    }

    #[test]
    fn the_number_of_spaces_is_limited() {
        let mut s = Spaces::default();
        while s.add("x").is_some() {}
        assert_eq!(s.all().len(), MAX_SPACES);
    }

    #[test]
    fn the_last_space_cannot_be_removed_and_tabs_have_an_heir() {
        let mut s = Spaces::default();
        assert_eq!(s.remove(FIRST_SPACE), None);
        let b = s.add("B").unwrap();
        s.set_current(b);
        assert_eq!(s.remove(b), Some(FIRST_SPACE));
        assert_eq!(s.current(), FIRST_SPACE);
    }

    #[test]
    fn relative_wraps_around() {
        let mut s = Spaces::default();
        let b = s.add("B").unwrap();
        assert_eq!(s.relative(1), b);
        assert_eq!(s.relative(-1), b);
    }

    #[test]
    fn rename_keeps_the_name_when_blank_and_cleans_the_icon() {
        let mut s = Spaces::default();
        s.rename(FIRST_SPACE, "   ", "🚀🚀🚀");
        assert_eq!(s.get(FIRST_SPACE).unwrap().name, "Principal");
        assert_eq!(s.get(FIRST_SPACE).unwrap().icon.chars().count(), 2);
    }

    #[test]
    fn a_damaged_file_still_gives_a_usable_set() {
        let broken = Spaces {
            list: vec![
                Space {
                    id: 0,
                    name: "".into(),
                    icon: "".into(),
                },
                Space {
                    id: 3,
                    name: "a\nb".into(),
                    icon: "".into(),
                },
                Space {
                    id: 3,
                    name: "dup".into(),
                    icon: "".into(),
                },
            ],
            current: 99,
        }
        .sanitized();
        assert_eq!(broken.all().len(), 1);
        assert_eq!(broken.current(), 3);
        assert_eq!(broken.get(3).unwrap().name, "ab");
        assert_eq!(
            Spaces {
                list: vec![],
                current: 5
            }
            .sanitized(),
            Spaces::default()
        );
    }

    #[test]
    fn badge_falls_back_to_the_initial() {
        let mut s = Spaces::default();
        assert_eq!(s.get(FIRST_SPACE).unwrap().badge(), "P");
        s.rename(FIRST_SPACE, "Principal", "🏠");
        assert_eq!(s.get(FIRST_SPACE).unwrap().badge(), "🏠");
    }
}
