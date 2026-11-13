//! The list of installed extensions, kept in `extensions.json`.

use serde::{Deserialize, Serialize};

use super::package::valid_id;

const MAX_EXTENSIONS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    /// A Chrome id; it is also the name of the folder the extension lives in.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Where it came from: `store`, `crx` or `folder`.
    #[serde(default)]
    pub source: String,
    /// When it was installed, in milliseconds since 1970.
    #[serde(default)]
    pub installed: u64,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registry {
    items: Vec<Installed>,
}

impl Registry {
    pub fn from_json(text: &str) -> Self {
        let mut r: Self = serde_json::from_str(text).unwrap_or_default();
        r.items.retain(|i| valid_id(&i.id));
        r.items.truncate(MAX_EXTENSIONS);
        for i in &mut r.items {
            i.name = i.name.chars().filter(|c| !c.is_control()).take(200).collect();
            i.version = i.version.chars().filter(|c| !c.is_control()).take(40).collect();
            if !matches!(i.source.as_str(), "store" | "crx" | "folder") {
                i.source = "folder".to_owned();
            }
        }
        r
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn items(&self) -> &[Installed] {
        &self.items
    }

    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Adds an extension, or replaces the one with the same id (an update), keeping whether it
    /// was switched on.
    pub fn put(&mut self, mut item: Installed) -> bool {
        if !valid_id(&item.id) {
            return false;
        }
        if let Some(old) = self.items.iter_mut().find(|i| i.id == item.id) {
            item.enabled = old.enabled;
            *old = item;
            return true;
        }
        if self.items.len() >= MAX_EXTENSIONS {
            return false;
        }
        self.items.push(item);
        true
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        self.items
            .iter_mut()
            .find(|i| i.id == id)
            .map(|i| i.enabled = enabled)
            .is_some()
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> Installed {
        Installed {
            id: id.into(),
            name: "N".into(),
            version: "1".into(),
            enabled: true,
            source: "crx".into(),
            installed: 1,
        }
    }

    #[test]
    fn put_replaces_by_id_and_keeps_the_switch() {
        let id = "a".repeat(32);
        let mut r = Registry::default();
        assert!(r.put(item(&id)));
        r.set_enabled(&id, false);
        let mut newer = item(&id);
        newer.version = "2".into();
        assert!(r.put(newer));
        assert_eq!(r.items().len(), 1);
        assert_eq!(r.get(&id).unwrap().version, "2");
        assert!(!r.get(&id).unwrap().enabled);
        assert!(!r.put(item("../x")));
        assert!(r.remove(&id) && !r.remove(&id));
    }

    #[test]
    fn a_saved_list_is_checked_when_read() {
        let good = "b".repeat(32);
        let json = format!(
            r#"{{"items":[{{"id":"{good}","name":"x\ny","version":"1","source":"evil"}},{{"id":"../../etc","name":"z","version":"1"}}]}}"#
        );
        let r = Registry::from_json(&json);
        assert_eq!(r.items().len(), 1);
        assert_eq!(r.items()[0].name, "xy");
        assert_eq!(r.items()[0].source, "folder");
        assert!(r.items()[0].enabled);
        assert!(Registry::from_json("{no").items().is_empty());
    }
}
