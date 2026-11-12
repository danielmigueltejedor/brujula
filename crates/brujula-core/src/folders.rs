//! Folders in the sidebar: named groups of tabs inside a space, like Arc's.
//!
//! A folder is either manual (the user puts tabs in it) or smart: it has a rule and takes on its
//! own every tab of its space whose site or title matches. A tab can be moved out of a smart
//! folder by hand, and then stays out.

use serde::{Deserialize, Serialize};

use crate::spaces::Spaces;

const MAX_FOLDERS: usize = 40;
const MAX_NAME: usize = 24;
const MAX_RULE: usize = 60;

/// Stored in a tab's `folder` to say "in no folder, whatever the smart folders would say".
pub const NO_FOLDER: u32 = 0;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "lowercase")]
pub enum Rule {
    /// The user fills it by hand.
    #[default]
    Manual,
    /// Tabs whose site contains this text (`github` or `github.com`).
    Site(String),
    /// Tabs whose title or address contains this text.
    Text(String),
}

impl Rule {
    /// Builds a rule from what a form gave: `kind` is `site` or `text`, anything else is manual.
    /// An empty text makes it manual.
    pub fn from_form(kind: &str, text: &str) -> Self {
        let clean = |t: &str| -> String {
            t.chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim()
                .to_lowercase()
                .chars()
                .take(MAX_RULE)
                .collect()
        };
        match kind {
            "site" => {
                let host = host_of(text).unwrap_or_else(|| clean(text));
                if host.is_empty() {
                    Self::Manual
                } else {
                    Self::Site(host)
                }
            }
            "text" => {
                let t = clean(text);
                if t.is_empty() {
                    Self::Manual
                } else {
                    Self::Text(t)
                }
            }
            _ => Self::Manual,
        }
    }

    pub fn is_smart(&self) -> bool {
        *self != Self::Manual
    }

    /// The kind and text a form shows for this rule.
    pub fn parts(&self) -> (&'static str, &str) {
        match self {
            Self::Manual => ("manual", ""),
            Self::Site(t) => ("site", t),
            Self::Text(t) => ("text", t),
        }
    }

    pub fn matches(&self, url: &str, title: &str) -> bool {
        match self {
            Self::Manual => false,
            Self::Site(s) => host_of(url).is_some_and(|h| h.contains(s.as_str())),
            Self::Text(t) => {
                title.to_lowercase().contains(t.as_str()) || url.to_lowercase().contains(t.as_str())
            }
        }
    }
}

/// `github.com` of `https://www.github.com/x`; also reads a bare `github.com/x`.
fn host_of(text: &str) -> Option<String> {
    let text = text.trim();
    let parsed = url::Url::parse(text)
        .ok()
        .filter(|u| u.has_host())
        .or_else(|| url::Url::parse(&format!("https://{text}")).ok())?;
    let host = parsed.host_str()?.to_lowercase();
    let host = host.trim_start_matches("www.").to_owned();
    (!host.is_empty() && host.contains(|c: char| c.is_ascii_alphanumeric())).then_some(host)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: u32,
    pub space: u32,
    pub name: String,
    /// Showing its tabs (as opposed to folded).
    #[serde(default = "yes")]
    pub open: bool,
    #[serde(default)]
    pub rule: Rule,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Folders {
    list: Vec<Folder>,
}

fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NAME)
        .collect()
}

impl Folders {
    /// Reads folders back from a saved session: ids unique and not zero, every folder in a space
    /// that exists (else the first one), names usable.
    pub fn sanitized(mut self, spaces: &Spaces) -> Self {
        let mut seen = Vec::new();
        self.list.retain(|f| {
            let fresh = f.id != NO_FOLDER && !seen.contains(&f.id);
            seen.push(f.id);
            fresh
        });
        self.list.truncate(MAX_FOLDERS);
        let home = spaces.all()[0].id;
        for f in &mut self.list {
            f.name = clean_name(&f.name);
            if f.name.is_empty() {
                f.name = "Carpeta".to_owned();
            }
            if !spaces.contains(f.space) {
                f.space = home;
            }
            f.rule = Rule::from_form(f.rule.parts().0, f.rule.parts().1);
        }
        self
    }

    pub fn all(&self) -> &[Folder] {
        &self.list
    }

    pub fn get(&self, id: u32) -> Option<&Folder> {
        self.list.iter().find(|f| f.id == id)
    }

    pub fn in_space(&self, space: u32) -> impl Iterator<Item = &Folder> {
        self.list.iter().filter(move |f| f.space == space)
    }

    /// A new folder at the end of the list, if there is room. Returns its id.
    pub fn add(&mut self, space: u32, name: &str, rule: Rule) -> Option<u32> {
        if self.list.len() >= MAX_FOLDERS {
            return None;
        }
        let id = self.list.iter().map(|f| f.id).max().unwrap_or(0) + 1;
        let mut name = clean_name(name);
        if name.is_empty() {
            name = "Carpeta".to_owned();
        }
        self.list.push(Folder {
            id,
            space,
            name,
            open: true,
            rule,
        });
        Some(id)
    }

    /// Changes name and rule; a blank name is kept as it was.
    pub fn edit(&mut self, id: u32, name: &str, rule: Rule) {
        let name = clean_name(name);
        if let Some(f) = self.list.iter_mut().find(|f| f.id == id) {
            if !name.is_empty() {
                f.name = name;
            }
            f.rule = rule;
        }
    }

    /// Folds or unfolds a folder. Returns whether it is open now.
    pub fn toggle(&mut self, id: u32) -> Option<bool> {
        let f = self.list.iter_mut().find(|f| f.id == id)?;
        f.open = !f.open;
        Some(f.open)
    }

    /// Unfolds a folder (a tab was just put in it).
    pub fn open_folder(&mut self, id: u32) {
        if let Some(f) = self.list.iter_mut().find(|f| f.id == id) {
            f.open = true;
        }
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.list.len();
        self.list.retain(|f| f.id != id);
        self.list.len() != before
    }

    /// The folder a tab is in: the one the user chose, or else the first smart folder of its
    /// space that matches. `chosen` is the tab's own `folder` field.
    pub fn folder_of(
        &self,
        chosen: Option<u32>,
        space: u32,
        url: &str,
        title: &str,
    ) -> Option<u32> {
        match chosen {
            Some(NO_FOLDER) => None,
            Some(id) => self
                .get(id)
                .filter(|f| f.space == space)
                .map(|f| f.id)
                .or_else(|| self.smart_match(space, url, title)),
            None => self.smart_match(space, url, title),
        }
    }

    fn smart_match(&self, space: u32, url: &str, title: &str) -> Option<u32> {
        self.in_space(space)
            .find(|f| f.rule.matches(url, title))
            .map(|f| f.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_rules_match_hosts_not_paths() {
        let r = Rule::from_form("site", "https://www.GitHub.com/rust");
        assert_eq!(r, Rule::Site("github.com".into()));
        assert!(r.matches("https://gist.github.com/x", ""));
        assert!(r.matches("https://github.com/", ""));
        assert!(!r.matches("https://example.com/github.com", ""));
        assert!(!r.matches("about:blank", ""));
        assert_eq!(Rule::from_form("site", "git"), Rule::Site("git".into()));
    }

    #[test]
    fn text_rules_look_at_title_and_address() {
        let r = Rule::from_form("text", "  Receta ");
        assert!(r.matches("https://a.com/", "Una receta de pan"));
        assert!(r.matches("https://a.com/receta", ""));
        assert!(!r.matches("https://a.com/", "Otra cosa"));
    }

    #[test]
    fn forms_with_nothing_in_them_make_manual_folders() {
        assert_eq!(Rule::from_form("site", "   "), Rule::Manual);
        assert_eq!(Rule::from_form("text", ""), Rule::Manual);
        assert_eq!(Rule::from_form("whatever", "x"), Rule::Manual);
        assert!(!Rule::Manual.matches("https://a.com", "a"));
        assert_eq!(
            Rule::from_form("text", "a\u{0}b\nc"),
            Rule::Text("abc".into())
        );
        assert!(Rule::from_form("text", &"x".repeat(500)).parts().1.len() <= MAX_RULE);
    }

    #[test]
    fn adding_gives_fresh_ids_and_clean_names() {
        let mut f = Folders::default();
        let a = f.add(1, "  Trabajo\n", Rule::Manual).unwrap();
        let b = f.add(1, "", Rule::Manual).unwrap();
        assert_ne!(a, b);
        assert_ne!(a, NO_FOLDER);
        assert_eq!(f.get(a).unwrap().name, "Trabajo");
        assert_eq!(f.get(b).unwrap().name, "Carpeta");
    }

    #[test]
    fn the_number_of_folders_is_limited() {
        let mut f = Folders::default();
        while f.add(1, "x", Rule::Manual).is_some() {}
        assert_eq!(f.all().len(), MAX_FOLDERS);
    }

    #[test]
    fn folding_editing_and_removing() {
        let mut f = Folders::default();
        let a = f.add(1, "A", Rule::Manual).unwrap();
        assert_eq!(f.toggle(a), Some(false));
        assert_eq!(f.toggle(a), Some(true));
        assert_eq!(f.toggle(99), None);
        f.edit(a, "  ", Rule::Site("a.com".into()));
        assert_eq!(f.get(a).unwrap().name, "A");
        assert!(f.get(a).unwrap().rule.is_smart());
        assert!(f.remove(a));
        assert!(!f.remove(a));
    }

    #[test]
    fn a_tab_goes_where_the_user_put_it_or_where_a_smart_folder_takes_it() {
        let mut f = Folders::default();
        let manual = f.add(1, "Mía", Rule::Manual).unwrap();
        let smart = f.add(1, "Código", Rule::Site("github.com".into())).unwrap();
        let other_space = f.add(2, "Otra", Rule::Site("github.com".into())).unwrap();
        let gh = "https://github.com/x";
        assert_eq!(f.folder_of(None, 1, gh, ""), Some(smart));
        assert_eq!(f.folder_of(None, 2, gh, ""), Some(other_space));
        assert_eq!(f.folder_of(None, 1, "https://a.com", ""), None);
        // Chosen by hand wins over the rule, and "no folder" stays out of the smart one.
        assert_eq!(f.folder_of(Some(manual), 1, gh, ""), Some(manual));
        assert_eq!(f.folder_of(Some(NO_FOLDER), 1, gh, ""), None);
        // A folder of another space, or one that is gone, does not hold the tab.
        assert_eq!(f.folder_of(Some(other_space), 1, gh, ""), Some(smart));
        assert_eq!(f.folder_of(Some(77), 1, "https://a.com", ""), None);
    }

    #[test]
    fn a_damaged_file_still_gives_usable_folders() {
        let spaces = Spaces::default();
        let broken = Folders {
            list: vec![
                Folder {
                    id: 0,
                    space: 1,
                    name: "zero".into(),
                    open: true,
                    rule: Rule::Manual,
                },
                Folder {
                    id: 5,
                    space: 99,
                    name: "\n".into(),
                    open: false,
                    rule: Rule::Site("  ".into()),
                },
                Folder {
                    id: 5,
                    space: 1,
                    name: "dup".into(),
                    open: true,
                    rule: Rule::Manual,
                },
            ],
        }
        .sanitized(&spaces);
        assert_eq!(broken.all().len(), 1);
        let f = &broken.all()[0];
        assert_eq!((f.id, f.space, f.name.as_str()), (5, 1, "Carpeta"));
        assert_eq!(f.rule, Rule::Manual);
        let json = serde_json::to_string(&broken).unwrap();
        assert_eq!(Folders::default(), serde_json::from_str("{}").unwrap());
        assert_eq!(broken, serde_json::from_str::<Folders>(&json).unwrap());
    }
}
