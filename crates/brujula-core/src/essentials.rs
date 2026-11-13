//! Essentials: the sites a person opens all day (mail, video, chat), kept as icons at the top of
//! the sidebar, in every space, like in Arc and Zen. One click opens the site, or goes to the
//! tab that already has it.

use serde::{Deserialize, Serialize};

/// How many essentials there can be: more would not fit in the sidebar anyway.
pub const MAX_ESSENTIALS: usize = 12;
const MAX_NAME: usize = 24;
const MAX_URL: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Essential {
    pub id: u32,
    pub name: String,
    pub url: String,
}

impl Essential {
    /// `example.com` of `https://www.example.com/x`.
    pub fn host(&self) -> String {
        host_of(&self.url).unwrap_or_default()
    }

    /// The letter shown while the site's own icon is not known yet.
    pub fn initial(&self) -> String {
        self.name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_else(|| "?".to_owned())
    }

    /// A colour for the placeholder tile: the site's brand colour when it is one of the
    /// suggested sites, else one picked from the host so the same site always has the same one.
    pub fn color(&self) -> &'static str {
        color_for(&self.host())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Essentials {
    list: Vec<Essential>,
}

/// A site offered when choosing essentials: name, address, brand colour.
pub struct Suggestion {
    pub name: &'static str,
    pub url: &'static str,
    pub color: &'static str,
}

/// The usual ones, in the order they are offered.
pub const SUGGESTED: &[Suggestion] = &[
    Suggestion {
        name: "YouTube",
        url: "https://www.youtube.com/",
        color: "#FF0033",
    },
    Suggestion {
        name: "Discord",
        url: "https://discord.com/app",
        color: "#5865F2",
    },
    Suggestion {
        name: "WhatsApp",
        url: "https://web.whatsapp.com/",
        color: "#25D366",
    },
    Suggestion {
        name: "Gmail",
        url: "https://mail.google.com/",
        color: "#EA4335",
    },
    Suggestion {
        name: "GitHub",
        url: "https://github.com/",
        color: "#24292F",
    },
    Suggestion {
        name: "X",
        url: "https://x.com/",
        color: "#14171A",
    },
    Suggestion {
        name: "Reddit",
        url: "https://www.reddit.com/",
        color: "#FF4500",
    },
    Suggestion {
        name: "Spotify",
        url: "https://open.spotify.com/",
        color: "#1DB954",
    },
    Suggestion {
        name: "Netflix",
        url: "https://www.netflix.com/",
        color: "#E50914",
    },
    Suggestion {
        name: "Twitch",
        url: "https://www.twitch.tv/",
        color: "#9146FF",
    },
    Suggestion {
        name: "Instagram",
        url: "https://www.instagram.com/",
        color: "#E4405F",
    },
    Suggestion {
        name: "Telegram",
        url: "https://web.telegram.org/",
        color: "#26A5E4",
    },
    Suggestion {
        name: "ChatGPT",
        url: "https://chatgpt.com/",
        color: "#10A37F",
    },
    Suggestion {
        name: "Calendar",
        url: "https://calendar.google.com/",
        color: "#4285F4",
    },
];

const PALETTE: &[&str] = &[
    "#6C7AE0", "#E0766C", "#4FA67F", "#C78B3C", "#8A6CE0", "#3C9CC7", "#C7487A", "#5E6B7A",
];

/// The brand colour of a suggested host, or a stable one for any other.
pub fn color_for(host: &str) -> &'static str {
    let host = host.trim_start_matches("www.");
    for s in SUGGESTED {
        if host_of(s.url).is_some_and(|h| h.trim_start_matches("www.") == host) {
            return s.color;
        }
    }
    let sum = host.bytes().fold(0usize, |a, b| {
        a.wrapping_mul(31).wrapping_add(usize::from(b))
    });
    PALETTE[sum % PALETTE.len()]
}

fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(str::to_ascii_lowercase)
}

/// An address a person typed, as a web address: `https://` is added when missing, and anything
/// that is not http(s) (a `javascript:` or `file:` address) is refused.
pub fn normalize(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > MAX_URL || text.chars().any(char::is_control) {
        return None;
    }
    let full = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    let parsed = url::Url::parse(&full).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.username().is_empty() {
        return None;
    }
    let host = parsed.host_str()?;
    (host.contains('.') || host == "localhost").then(|| parsed.to_string())
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

impl Essentials {
    /// Reads the saved file; anything wrong in it is dropped.
    pub fn from_json(text: &str) -> Self {
        let mut me: Self = serde_json::from_str(text).unwrap_or_default();
        let mut seen = Vec::new();
        me.list.retain_mut(|e| {
            let Some(url) = normalize(&e.url) else {
                return false;
            };
            e.url = url;
            e.name = clean_name(&e.name);
            if e.name.is_empty() {
                e.name = e.host();
            }
            let fresh = e.id != 0 && !seen.contains(&e.id);
            seen.push(e.id);
            fresh
        });
        me.list.truncate(MAX_ESSENTIALS);
        me
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn items(&self) -> &[Essential] {
        &self.list
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&Essential> {
        self.list.iter().find(|e| e.id == id)
    }

    /// Whether `url` is the address of an essential (same site and path).
    pub fn contains(&self, url: &str) -> bool {
        normalize(url).is_some_and(|u| self.list.iter().any(|e| e.url == u))
    }

    /// Adds a site. `None` if the address is not a web address, it is there already, or there
    /// are too many.
    pub fn add(&mut self, name: &str, url: &str) -> Option<u32> {
        let url = normalize(url)?;
        if self.list.len() >= MAX_ESSENTIALS || self.list.iter().any(|e| e.url == url) {
            return None;
        }
        let id = self.list.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let mut name = clean_name(name);
        if name.is_empty() {
            name = host_of(&url)
                .unwrap_or_default()
                .trim_start_matches("www.")
                .to_owned();
        }
        self.list.push(Essential { id, name, url });
        Some(id)
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.list.len();
        self.list.retain(|e| e.id != id);
        self.list.len() != before
    }

    /// Moves one a place earlier (`-1`) or later (`1`).
    pub fn shift(&mut self, id: u32, by: isize) {
        let Some(i) = self.list.iter().position(|e| e.id == id) else {
            return;
        };
        let to = i.saturating_add_signed(by).min(self.list.len() - 1);
        let item = self.list.remove(i);
        self.list.insert(to, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_made_web_addresses_or_refused() {
        assert_eq!(
            normalize("youtube.com").as_deref(),
            Some("https://youtube.com/")
        );
        assert_eq!(
            normalize(" https://a.example/x ").as_deref(),
            Some("https://a.example/x")
        );
        for bad in [
            "",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "ftp://a.example",
            "https://user:pw@a.example",
            "sin punto",
            "a.example\n/b",
        ] {
            assert_eq!(normalize(bad), None, "{bad}");
        }
        assert_eq!(normalize(&format!("a.example/{}", "x".repeat(3000))), None);
    }

    #[test]
    fn essentials_are_added_once_named_and_limited() {
        let mut e = Essentials::default();
        let id = e.add("  ", "www.youtube.com").unwrap();
        assert_eq!(e.get(id).unwrap().name, "youtube.com");
        assert_eq!(e.add("Otra", "https://www.youtube.com/"), None);
        assert!(e.contains("https://www.youtube.com"));
        for n in 0..MAX_ESSENTIALS * 2 {
            e.add("x", &format!("s{n}.example"));
        }
        assert_eq!(e.items().len(), MAX_ESSENTIALS);
        assert!(e.remove(id));
        assert!(!e.remove(id));
    }

    #[test]
    fn essentials_move_without_leaving_the_list() {
        let mut e = Essentials::default();
        let a = e.add("a", "a.example").unwrap();
        let b = e.add("b", "b.example").unwrap();
        let c = e.add("c", "c.example").unwrap();
        e.shift(c, -1);
        e.shift(a, 99);
        let ids: Vec<u32> = e.items().iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![c, b, a]);
        e.shift(b, -5);
        assert_eq!(e.items()[0].id, b);
    }

    #[test]
    fn a_damaged_file_loses_only_the_bad_entries() {
        let text = r#"{"list":[
            {"id":1,"name":"Ok","url":"https://ok.example/"},
            {"id":2,"name":"Mala","url":"javascript:alert(1)"},
            {"id":1,"name":"Repetida","url":"https://dup.example/"},
            {"id":0,"name":"Cero","url":"https://zero.example/"},
            {"id":3,"name":"\u0007Con\ncontrol","url":"https://ctl.example/"}]}"#;
        let e = Essentials::from_json(text);
        let names: Vec<&str> = e.items().iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["Ok", "Concontrol"]);
        assert_eq!(Essentials::from_json("no es json"), Essentials::default());
    }

    #[test]
    fn placeholders_have_a_letter_and_a_steady_colour() {
        let e = Essential {
            id: 1,
            name: "¡hola!".into(),
            url: "https://www.youtube.com/".into(),
        };
        assert_eq!(e.initial(), "H");
        assert_eq!(e.color(), "#FF0033");
        assert_eq!(color_for("zzz.example"), color_for("zzz.example"));
        for s in SUGGESTED {
            assert!(normalize(s.url).is_some(), "{}", s.name);
        }
    }
}
