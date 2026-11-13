//! What each site was allowed to do, kept between sessions: for now, to show notifications.
//!
//! WebKit forgets the answer to a permission prompt when the browser closes, so a site that was
//! allowed (WhatsApp Web, a mail page) would ask again on every start, or never be able to
//! notify. The browser keeps the answers here, per origin, and the person can take any of them
//! back on the preferences page.

use serde::{Deserialize, Serialize};

/// How many origins are remembered: more than anyone will ever answer, and a bound for the file.
pub const MAX_SITES: usize = 500;
const MAX_ORIGIN: usize = 256;

/// The answer given to a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
}

/// One remembered answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteNotifications {
    /// `https://web.whatsapp.com`: scheme, host and a port that is not the default one.
    pub origin: String,
    pub decision: Decision,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SitePermissions {
    notifications: Vec<SiteNotifications>,
}

/// `https://web.whatsapp.com` of any address in that site, or `None` when the page is not one a
/// notification can come from. Notifications need a secure context, so only `https://` (and a
/// local `http://` address for people who develop) count.
pub fn origin_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let secure = match parsed.scheme() {
        "https" => true,
        "http" => matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]"),
        _ => false,
    };
    if !secure {
        return None;
    }
    let mut origin = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        origin.push_str(&format!(":{port}"));
    }
    (origin.len() <= MAX_ORIGIN).then_some(origin)
}

/// The site name to show a person: the host, without a leading `www.`.
pub fn label_of(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| {
                let h = h.trim_start_matches("www.");
                match u.port() {
                    Some(p) => format!("{h}:{p}"),
                    None => h.to_owned(),
                }
            })
        })
        .unwrap_or_else(|| origin.to_owned())
}

impl SitePermissions {
    /// Reads the saved file; anything wrong in it is dropped.
    pub fn from_json(text: &str) -> Self {
        let mut me: Self = serde_json::from_str(text).unwrap_or_default();
        let mut seen: Vec<String> = Vec::new();
        me.notifications.retain_mut(|n| {
            // Written by hand or by a newer version: keep only what is a clean origin.
            let Some(origin) = origin_of(&n.origin).filter(|o| *o == n.origin) else {
                return false;
            };
            if seen.contains(&origin) {
                return false;
            }
            seen.push(origin);
            true
        });
        me.notifications.truncate(MAX_SITES);
        me
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// What was answered for the site of `url`, if anything.
    pub fn notifications_for(&self, url: &str) -> Option<Decision> {
        let origin = origin_of(url)?;
        self.notifications
            .iter()
            .find(|n| n.origin == origin)
            .map(|n| n.decision)
    }

    /// Remembers the answer for the site of `url`. `false` if the page is not a site that can be
    /// remembered, or there are too many.
    pub fn set_notifications(&mut self, url: &str, decision: Decision) -> bool {
        let Some(origin) = origin_of(url) else {
            return false;
        };
        if let Some(n) = self.notifications.iter_mut().find(|n| n.origin == origin) {
            n.decision = decision;
            return true;
        }
        if self.notifications.len() >= MAX_SITES {
            return false;
        }
        self.notifications
            .push(SiteNotifications { origin, decision });
        true
    }

    /// Takes the answer back, so the site has to ask again. `true` if there was one.
    pub fn forget_notifications(&mut self, origin: &str) -> bool {
        let before = self.notifications.len();
        self.notifications.retain(|n| n.origin != origin);
        self.notifications.len() != before
    }

    /// Every remembered answer, the allowed sites first and each group in alphabetical order.
    pub fn notifications(&self) -> Vec<&SiteNotifications> {
        let mut all: Vec<&SiteNotifications> = self.notifications.iter().collect();
        all.sort_by(|a, b| {
            (a.decision != Decision::Allow, &a.origin)
                .cmp(&(b.decision != Decision::Allow, &b.origin))
        });
        all
    }

    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_scheme_host_and_odd_port() {
        assert_eq!(
            origin_of("https://Web.WhatsApp.com/path?q=1#x").as_deref(),
            Some("https://web.whatsapp.com")
        );
        assert_eq!(
            origin_of("https://example.com:8443/a").as_deref(),
            Some("https://example.com:8443")
        );
        assert_eq!(
            origin_of("https://example.com:443/a").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn insecure_and_odd_schemes_have_no_origin() {
        assert_eq!(origin_of("http://example.com/"), None);
        assert_eq!(origin_of("file:///etc/passwd"), None);
        assert_eq!(origin_of("javascript:alert(1)"), None);
        assert_eq!(origin_of("brujula-ext://abc/page.html"), None);
        assert_eq!(origin_of("about:blank"), None);
        assert_eq!(origin_of("not a url"), None);
        assert_eq!(
            origin_of("http://localhost:3000/").as_deref(),
            Some("http://localhost:3000")
        );
    }

    #[test]
    fn answers_are_per_origin() {
        let mut p = SitePermissions::default();
        assert_eq!(p.notifications_for("https://web.whatsapp.com/"), None);
        assert!(p.set_notifications("https://web.whatsapp.com/", Decision::Allow));
        assert_eq!(
            p.notifications_for("https://web.whatsapp.com/a/b"),
            Some(Decision::Allow)
        );
        assert_eq!(p.notifications_for("https://evil.example/"), None);
        assert_eq!(p.notifications_for("https://whatsapp.com/"), None);
        assert!(p.set_notifications("https://web.whatsapp.com/", Decision::Deny));
        assert_eq!(
            p.notifications_for("https://web.whatsapp.com/"),
            Some(Decision::Deny)
        );
        assert_eq!(p.notifications().len(), 1);
    }

    #[test]
    fn a_page_that_is_not_a_site_is_not_remembered() {
        let mut p = SitePermissions::default();
        assert!(!p.set_notifications("about:blank", Decision::Allow));
        assert!(p.is_empty());
    }

    #[test]
    fn forgetting_makes_the_site_ask_again() {
        let mut p = SitePermissions::default();
        p.set_notifications("https://a.example/", Decision::Allow);
        assert!(p.forget_notifications("https://a.example"));
        assert!(!p.forget_notifications("https://a.example"));
        assert_eq!(p.notifications_for("https://a.example/"), None);
    }

    #[test]
    fn file_round_trip_and_junk_is_dropped() {
        let mut p = SitePermissions::default();
        p.set_notifications("https://b.example/", Decision::Deny);
        p.set_notifications("https://a.example/", Decision::Allow);
        let back = SitePermissions::from_json(&p.to_json());
        assert_eq!(back, p);

        let junk = r#"{"notifications":[
            {"origin":"https://ok.example","decision":"allow"},
            {"origin":"https://ok.example","decision":"deny"},
            {"origin":"javascript:alert(1)","decision":"allow"},
            {"origin":"https://UPPER.example/with/path","decision":"allow"},
            {"origin":"http://plain.example","decision":"allow"}
        ]}"#;
        let p = SitePermissions::from_json(junk);
        assert_eq!(p.notifications().len(), 1);
        assert_eq!(
            p.notifications_for("https://ok.example/"),
            Some(Decision::Allow)
        );
        assert!(SitePermissions::from_json("not json").is_empty());
    }

    #[test]
    fn allowed_sites_are_listed_first() {
        let mut p = SitePermissions::default();
        p.set_notifications("https://z.example/", Decision::Allow);
        p.set_notifications("https://a.example/", Decision::Deny);
        p.set_notifications("https://m.example/", Decision::Allow);
        let order: Vec<&str> = p
            .notifications()
            .iter()
            .map(|n| n.origin.as_str())
            .collect();
        assert_eq!(
            order,
            [
                "https://m.example",
                "https://z.example",
                "https://a.example"
            ]
        );
    }

    #[test]
    fn the_list_has_a_limit() {
        let mut p = SitePermissions::default();
        for i in 0..MAX_SITES {
            assert!(p.set_notifications(&format!("https://s{i}.example/"), Decision::Allow));
        }
        assert!(!p.set_notifications("https://one-too-many.example/", Decision::Allow));
        // Changing one that is there still works.
        assert!(p.set_notifications("https://s0.example/", Decision::Deny));
    }

    #[test]
    fn label_drops_www_and_keeps_port() {
        assert_eq!(label_of("https://www.example.com"), "example.com");
        assert_eq!(label_of("https://example.com:8443"), "example.com:8443");
    }
}
