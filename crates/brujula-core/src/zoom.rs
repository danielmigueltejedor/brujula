//! Page zoom, remembered per site: the level the person chose for a site comes back every
//! time they visit it, like in every other browser.

use std::collections::BTreeMap;

/// The levels Ctrl+plus and Ctrl+minus step through, in percent.
pub const STEPS: [u32; 15] = [
    30, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400,
];

pub const DEFAULT: u32 = 100;

/// How many sites keep a level of their own; the oldest entries go first (alphabetically, which
/// is as good as any order for a list this long).
const MAX_SITES: usize = 500;

/// The next level up or down from `current`, staying inside the steps.
pub fn step(current: u32, up: bool) -> u32 {
    if up {
        STEPS
            .iter()
            .copied()
            .find(|s| *s > current)
            .unwrap_or(STEPS[STEPS.len() - 1])
    } else {
        STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| *s < current)
            .unwrap_or(STEPS[0])
    }
}

/// `125 %`, as shown to the person.
pub fn label(percent: u32) -> String {
    format!("{percent} %")
}

/// The zoom of each site that is not at 100 %.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Zooms {
    sites: BTreeMap<String, u32>,
}

impl Zooms {
    pub fn from_json(text: &str) -> Self {
        let raw: BTreeMap<String, u32> = serde_json::from_str(text).unwrap_or_default();
        let mut zooms = Self::default();
        for (host, percent) in raw {
            zooms.set(&host, percent);
        }
        zooms
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.sites).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The level for `host` (the name from `adblock::normalize_host`).
    pub fn get(&self, host: &str) -> u32 {
        self.sites.get(host).copied().unwrap_or(DEFAULT)
    }

    /// Sets the level; 100 % forgets the site. Out-of-range levels and odd hosts are refused.
    pub fn set(&mut self, host: &str, percent: u32) -> bool {
        let valid_host = !host.is_empty()
            && host.len() <= 253
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if !valid_host || !(STEPS[0]..=STEPS[STEPS.len() - 1]).contains(&percent) {
            return false;
        }
        if percent == DEFAULT {
            self.sites.remove(host);
            return true;
        }
        if !self.sites.contains_key(host) && self.sites.len() >= MAX_SITES {
            return false;
        }
        self.sites.insert(host.to_owned(), percent);
        true
    }

    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_up_and_down_and_stop_at_the_ends() {
        assert_eq!(step(100, true), 110);
        assert_eq!(step(100, false), 90);
        assert_eq!(step(400, true), 400);
        assert_eq!(step(30, false), 30);
        // A level between two steps (set by a script, say) lands on the next one.
        assert_eq!(step(105, true), 110);
        assert_eq!(step(105, false), 100);
    }

    #[test]
    fn a_site_keeps_its_level_and_100_forgets_it() {
        let mut z = Zooms::default();
        assert_eq!(z.get("example.com"), 100);
        assert!(z.set("example.com", 125));
        assert_eq!(z.get("example.com"), 125);
        assert!(z.set("example.com", 100));
        assert!(z.is_empty());
    }

    #[test]
    fn levels_out_of_range_and_odd_hosts_are_refused() {
        let mut z = Zooms::default();
        assert!(!z.set("example.com", 10));
        assert!(!z.set("example.com", 1000));
        assert!(!z.set("", 125));
        assert!(!z.set("a b", 125));
        assert!(z.is_empty());
    }

    #[test]
    fn the_list_is_capped_and_round_trips() {
        let mut z = Zooms::default();
        for i in 0..MAX_SITES {
            assert!(z.set(&format!("s{i}.example"), 125));
        }
        assert!(!z.set("one-too-many.example", 125));
        // An existing site can still change.
        assert!(z.set("s1.example", 150));
        let back = Zooms::from_json(&z.to_json());
        assert_eq!(back, z);
        assert_eq!(back.len(), MAX_SITES);
        assert_eq!(Zooms::from_json("not json"), Zooms::default());
        assert_eq!(label(125), "125 %");
    }
}
