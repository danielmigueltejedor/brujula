//! The ad blocker. Filter lists in the usual ABP/uBlock syntax are the single source of truth;
//! from them we derive
//!
//! - WebKit content-blocker rules (network blocking, done inside WebKit before a request
//!   leaves the process), and
//! - cosmetic rules (element hiding), applied per page as a user stylesheet.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use adblock::lists::{FilterSet, ParseOptions};
use adblock::Engine;

/// WebKit refuses a content-filter list over this many rules, so we ship the rules in chunks.
pub const MAX_RULES_PER_FILTER: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterList {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub enabled_by_default: bool,
}

/// Lists enabled out of the box: ads, trackers, annoyances, cookie banners and malware.
pub const DEFAULT_LISTS: &[FilterList] = &[
    FilterList {
        id: "easylist",
        name: "EasyList",
        url: "https://easylist.to/easylist/easylist.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "easyprivacy",
        name: "EasyPrivacy",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "ublock-filters",
        name: "uBlock filters",
        url: "https://ublockorigin.github.io/uAssets/filters/filters.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "ublock-privacy",
        name: "uBlock filters – Privacy",
        url: "https://ublockorigin.github.io/uAssets/filters/privacy.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "ublock-badware",
        name: "uBlock filters – Badware risks",
        url: "https://ublockorigin.github.io/uAssets/filters/badware.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "ublock-unbreak",
        name: "uBlock filters – Unbreak",
        url: "https://ublockorigin.github.io/uAssets/filters/unbreak.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "fanboy-cookiemonster",
        name: "EasyList Cookie",
        url: "https://secure.fanboy.co.nz/fanboy-cookiemonster.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "easylist-spanish",
        name: "EasyList Spanish",
        url: "https://easylist-downloads.adblockplus.org/easylistspanish.txt",
        enabled_by_default: true,
    },
    FilterList {
        id: "fanboy-annoyance",
        name: "Fanboy's Annoyances",
        url: "https://secure.fanboy.co.nz/fanboy-annoyance.txt",
        enabled_by_default: false,
    },
];

/// Names the format of the cached compiled rules. Change it whenever the `adblock` crate is
/// upgraded: serialised data is not guaranteed to load across versions.
pub const CACHE_FORMAT: &str = "adblock-0.13-exceptions-per-list";

/// How long a downloaded list is trusted before it is fetched again.
pub const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// The same on metered connections.
pub const REFRESH_AFTER_METERED: Duration = Duration::from_secs(14 * 24 * 60 * 60);

pub fn needs_refresh(last_update: Option<SystemTime>, now: SystemTime, metered: bool) -> bool {
    let limit = if metered {
        REFRESH_AFTER_METERED
    } else {
        REFRESH_AFTER
    };
    match last_update {
        None => true,
        // A modification time in the future means the clock moved; refresh rather than trust.
        Some(t) => now.duration_since(t).map_or(true, |age| age >= limit),
    }
}

/// Everything compiled from the enabled lists.
pub struct Compiled {
    /// WebKit content-blocker JSON documents, each within [`MAX_RULES_PER_FILTER`].
    pub content_filters: Vec<String>,
    pub cosmetic: Cosmetic,
    pub rule_count: usize,
}

/// Compiles filter lists given as text. Lines that fail to parse are skipped, as every blocker
/// does: lists are written for many engines and each only understands part of them.
///
/// `allow` lists sites where blocking is off (see [`normalize_host`]).
pub fn compile<'a>(lists: impl IntoIterator<Item = &'a str>, allow: &[String]) -> Compiled {
    let lists: Vec<&str> = lists.into_iter().collect();
    let options = ParseOptions::default();

    let mut set = FilterSet::new(true);
    for text in &lists {
        set.add_filter_list(text.to_string(), options);
    }
    // `into_content_blocking` consumes the set, so the cosmetic engine gets its own.
    let mut cosmetic_set = FilterSet::new(false);
    for text in &lists {
        cosmetic_set.add_filter_list(text.to_string(), options);
    }

    let (rules, _used) = set.into_content_blocking().unwrap_or_default();
    let rule_count = rules.len();
    let values: Vec<serde_json::Value> = rules
        .iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();

    Compiled {
        content_filters: into_filters(values, allow),
        cosmetic: Cosmetic {
            engine: Engine::new_with_filter_set(cosmetic_set),
            allow: allow.to_vec(),
        },
        rule_count,
    }
}

fn is_exception(rule: &serde_json::Value) -> bool {
    rule["action"]["type"] == "ignore-previous-rules"
}

/// A rule that turns blocking off on `host` and its subdomains.
fn allow_rule(host: &str) -> serde_json::Value {
    serde_json::json!({
        "trigger": { "url-filter": ".*", "if-domain": [format!("*{host}")] },
        "action": { "type": "ignore-previous-rules" }
    })
}

/// Splits the rules into WebKit lists.
///
/// WebKit evaluates each list on its own: an `ignore-previous-rules` exception only cancels
/// rules in *its own* list. The converter puts every exception at the very end, so cutting the
/// rules into lists would leave the exceptions in the last list and make them useless for all
/// the others (a site's allow-listed script stays blocked). So every list ends with all the
/// exceptions, and the allowed sites.
fn into_filters(rules: Vec<serde_json::Value>, allow: &[String]) -> Vec<String> {
    let (mut exceptions, rest): (Vec<_>, Vec<_>) = rules.into_iter().partition(is_exception);
    exceptions.extend(
        allow
            .iter()
            .filter(|h| is_valid_host(h))
            .map(|h| allow_rule(h)),
    );
    if rest.is_empty() {
        return Vec::new();
    }
    // The exceptions share every list's budget; keep room for real rules whatever happens.
    let room = MAX_RULES_PER_FILTER
        .saturating_sub(exceptions.len())
        .max(MAX_RULES_PER_FILTER / 4);
    exceptions.truncate(MAX_RULES_PER_FILTER - room);
    rest.chunks(room)
        .filter_map(|chunk| {
            let mut all: Vec<&serde_json::Value> = chunk.iter().collect();
            all.extend(exceptions.iter());
            serde_json::to_string(&all).ok()
        })
        .collect()
}

/// `example.com` from a page address, without `www.`; `None` for anything that is not an
/// http(s) page with a host. This is what the allow list stores.
pub fn normalize_host(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    is_valid_host(&host).then_some(host)
}

/// Letters, digits, dots and dashes only: the host ends up inside a rule, so nothing else may.
fn is_valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// True when `host` is in the allow list or is a subdomain of an entry.
pub fn host_allowed(host: &str, allow: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    allow
        .iter()
        .any(|a| host == a || host.ends_with(&format!(".{a}")))
}

/// What the lists say about one page.
pub struct PageCosmetic {
    /// Stylesheet hiding what is specific to this site.
    pub css: String,
    /// Class/id selectors for which generic rules must not apply on this page.
    pub exceptions: HashSet<String>,
    /// The site opted out of generic hiding (`$generichide`).
    pub generichide: bool,
}

/// Element hiding.
pub struct Cosmetic {
    engine: Engine,
    allow: Vec<String>,
}

impl Cosmetic {
    /// A blocker with no rules.
    pub fn empty() -> Self {
        Self {
            engine: Engine::default(),
            allow: Vec::new(),
        }
    }

    /// Sites where nothing is hidden.
    pub fn with_allow(mut self, allow: Vec<String>) -> Self {
        self.allow = allow;
        self
    }

    /// Hiding rules specific to `page_url`. The CSS is empty when there is nothing to hide.
    pub fn for_page(&self, page_url: &str) -> PageCosmetic {
        if normalize_host(page_url).is_some_and(|h| host_allowed(&h, &self.allow)) {
            return PageCosmetic {
                css: String::new(),
                exceptions: HashSet::new(),
                generichide: true,
            };
        }
        let res = self.engine.url_cosmetic_resources(page_url);
        let mut selectors: Vec<&String> = res.hide_selectors.iter().collect();
        selectors.sort();
        PageCosmetic {
            css: to_css(selectors),
            exceptions: res.exceptions,
            generichide: res.generichide,
        }
    }

    /// Generic rules that apply to the classes and ids found on the page. Generic rules are
    /// not sent up front (there are tens of thousands); the page reports what it contains.
    pub fn generic_css<C: AsRef<str>, I: AsRef<str>>(
        &self,
        classes: impl IntoIterator<Item = C>,
        ids: impl IntoIterator<Item = I>,
        exceptions: &HashSet<String>,
    ) -> String {
        let mut selectors = self
            .engine
            .hidden_class_id_selectors(classes, ids, exceptions);
        selectors.sort();
        to_css(selectors.iter())
    }

    /// The compiled rules in a form that loads in milliseconds. Only valid for the same
    /// [`CACHE_FORMAT`]; keep it in the user's own cache directory, never accept it from
    /// elsewhere.
    pub fn serialize(&self) -> Vec<u8> {
        self.engine.serialize()
    }

    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        let mut engine = Engine::default();
        engine.deserialize(bytes).ok()?;
        Some(Self {
            engine,
            allow: Vec::new(),
        })
    }

    /// Shorthand for tests and callers that only want the site-specific CSS.
    pub fn css_for(&self, page_url: &str) -> String {
        self.for_page(page_url).css
    }
}

fn to_css<S: AsRef<str>>(selectors: impl IntoIterator<Item = S>) -> String {
    let mut out = String::new();
    // One rule per selector: a single invalid selector in a combined rule would drop all.
    for s in selectors {
        let s = s.as_ref();
        if is_safe_selector(s) {
            out.push_str(s);
            out.push_str("{display:none!important}\n");
        }
    }
    out
}

/// Filter lists are untrusted input that ends up in a stylesheet. A selector must not be able
/// to close its rule or open another.
fn is_safe_selector(s: &str) -> bool {
    !s.is_empty() && !s.contains(['{', '}', ';', '\n', '\r', '@', '\\']) && !s.contains("/*")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\
! Title: test
||ads.example.com^
||tracker.example.net^$third-party
/banner/*/img^$image
example.com##.ad-banner
##.generic-ad
example.com#@#.generic-ad
news.example.org##div[data-ad=\"1\"]
";

    #[test]
    fn network_rules_become_content_blocker_json() {
        let c = compile([LIST], &[]);
        assert!(c.rule_count >= 3, "got {}", c.rule_count);
        assert_eq!(c.content_filters.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&c.content_filters[0]).unwrap();
        let rules = v.as_array().unwrap();
        assert!(rules.iter().any(|r| r["action"]["type"] == "block"
            && r["trigger"]["url-filter"]
                .as_str()
                .unwrap_or("")
                .contains("ads\\.example\\.com")));
    }

    #[test]
    fn cosmetic_rules_apply_per_site() {
        let c = compile([LIST], &[]);
        let css = c.cosmetic.css_for("https://example.com/page");
        assert!(css.contains(".ad-banner{display:none!important}"), "{css}");
        let other = c.cosmetic.css_for("https://other.org/");
        assert!(!other.contains(".ad-banner"), "{other}");
    }

    #[test]
    fn generic_rules_apply_to_what_the_page_has() {
        let c = compile([LIST], &[]);
        let page = c.cosmetic.for_page("https://other.org/");
        let css = c.cosmetic.generic_css(
            ["generic-ad", "nothing"],
            Vec::<&str>::new(),
            &page.exceptions,
        );
        assert!(css.contains(".generic-ad{display:none!important}"), "{css}");
        let none = c
            .cosmetic
            .generic_css(["nothing"], Vec::<&str>::new(), &page.exceptions);
        assert!(none.is_empty());
    }

    #[test]
    fn generic_rules_respect_site_exceptions() {
        let c = compile([LIST], &[]);
        let page = c.cosmetic.for_page("https://example.com/");
        let css = c
            .cosmetic
            .generic_css(["generic-ad"], Vec::<&str>::new(), &page.exceptions);
        assert!(
            !css.contains(".generic-ad"),
            "exception for example.com ignored: {css}"
        );
    }

    #[test]
    fn selectors_cannot_break_out_of_the_rule() {
        assert!(!is_safe_selector("a{}body{display:none}"));
        assert!(!is_safe_selector(".x;color:red"));
        assert!(!is_safe_selector("@import url(x)"));
        assert!(!is_safe_selector("/* x */"));
        assert!(is_safe_selector("div[data-ad=\"1\"] > .banner"));
    }

    #[test]
    fn serialised_rules_behave_the_same() {
        let c = compile([LIST], &[]);
        let bytes = c.cosmetic.serialize();
        let back = Cosmetic::deserialize(&bytes).expect("loads");
        assert_eq!(
            back.css_for("https://example.com/"),
            c.cosmetic.css_for("https://example.com/")
        );
        assert!(Cosmetic::deserialize(b"garbage").is_none());
        assert!(Cosmetic::deserialize(&[]).is_none());
    }

    #[test]
    fn empty_blocker_does_nothing() {
        assert!(Cosmetic::empty().css_for("https://example.com/").is_empty());
    }

    #[test]
    fn rules_are_chunked() {
        let mut text = String::new();
        for i in 0..(MAX_RULES_PER_FILTER + 10) {
            text.push_str(&format!("||h{i}.example^\n"));
        }
        let c = compile([text.as_str()], &[]);
        assert_eq!(c.content_filters.len(), 2);
        let first: serde_json::Value = serde_json::from_str(&c.content_filters[0]).unwrap();
        assert!(first.as_array().unwrap().len() <= MAX_RULES_PER_FILTER);
    }

    fn rules_of(filter: &str) -> Vec<serde_json::Value> {
        serde_json::from_str(filter).unwrap()
    }

    #[test]
    fn every_list_carries_all_the_exceptions() {
        // WebKit applies an exception only inside its own list, so a split must not leave
        // them all in the last one.
        let mut text = String::from("@@||h0.example^\n@@||h5.example^$script\n");
        for i in 0..(MAX_RULES_PER_FILTER + 10) {
            text.push_str(&format!("||h{i}.example^\n"));
        }
        let c = compile([text.as_str()], &[]);
        assert_eq!(c.content_filters.len(), 2);
        let counts: Vec<usize> = c
            .content_filters
            .iter()
            .map(|f| rules_of(f).iter().filter(|r| is_exception(r)).count())
            .collect();
        assert!(counts[0] >= 2, "{counts:?}");
        assert_eq!(counts[0], counts[1], "{counts:?}");
        for f in &c.content_filters {
            let rules = rules_of(f);
            assert!(rules.len() <= MAX_RULES_PER_FILTER);
            // Exceptions come last: they only cancel what is before them.
            let first_exception = rules.iter().position(is_exception).unwrap();
            assert!(rules[first_exception..].iter().all(is_exception));
        }
    }

    #[test]
    fn allowed_sites_are_exempt_in_every_list() {
        let mut text = String::new();
        for i in 0..(MAX_RULES_PER_FILTER + 10) {
            text.push_str(&format!("||h{i}.example^\n"));
        }
        let c = compile(
            [text.as_str()],
            &["good.example".to_owned(), "bad host!".to_owned()],
        );
        assert_eq!(c.content_filters.len(), 2);
        for f in &c.content_filters {
            let rules = rules_of(f);
            let last = rules.last().unwrap();
            assert!(is_exception(last));
            assert_eq!(last["trigger"]["if-domain"][0], "*good.example");
            assert!(
                !f.contains("bad host"),
                "an invalid host must never reach a rule"
            );
        }
    }

    #[test]
    fn allowed_sites_get_no_cosmetic_rules() {
        let c = compile([LIST], &["example.com".to_owned()]);
        assert!(c.cosmetic.for_page("https://example.com/").css.is_empty());
        assert!(c
            .cosmetic
            .for_page("https://www.example.com/x")
            .css
            .is_empty());
        assert!(c
            .cosmetic
            .for_page("https://sub.example.com/")
            .css
            .is_empty());
        assert!(!c
            .cosmetic
            .for_page("https://news.example.org/")
            .css
            .is_empty());
    }

    #[test]
    fn hosts_are_normalised() {
        assert_eq!(
            normalize_host("https://www.Example.com/a?b").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_host("http://localhost:3000/").as_deref(),
            Some("localhost")
        );
        assert_eq!(normalize_host("about:blank"), None);
        assert_eq!(normalize_host("brujula://newtab"), None);
        assert_eq!(normalize_host("file:///etc/passwd"), None);
        assert!(host_allowed(
            "www.a.example.com",
            &["example.com".to_owned()]
        ));
        assert!(!host_allowed("notexample.com", &["example.com".to_owned()]));
    }

    #[test]
    fn refresh_policy() {
        let now = SystemTime::now();
        assert!(needs_refresh(None, now, false));
        assert!(!needs_refresh(
            Some(now - Duration::from_secs(3600)),
            now,
            false
        ));
        assert!(needs_refresh(
            Some(now - Duration::from_secs(2 * 86400)),
            now,
            false
        ));
        assert!(!needs_refresh(
            Some(now - Duration::from_secs(2 * 86400)),
            now,
            true
        ));
        assert!(needs_refresh(
            Some(now + Duration::from_secs(60)),
            now,
            false
        ));
    }

    #[test]
    fn default_lists_are_unique_https() {
        let mut ids = HashSet::new();
        for l in DEFAULT_LISTS {
            assert!(l.url.starts_with("https://"), "{}", l.id);
            assert!(ids.insert(l.id), "duplicate {}", l.id);
        }
    }
}

#[cfg(test)]
mod real_lists {
    //! Runs against real lists when `BRUJULA_LISTS_DIR` points at a folder with them.
    use super::*;

    #[test]
    fn real_lists_compile() {
        let Ok(dir) = std::env::var("BRUJULA_LISTS_DIR") else {
            return;
        };
        let texts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
            .collect();
        let t = std::time::Instant::now();
        let c = compile(texts.iter().map(String::as_str), &[]);
        eprintln!(
            "compiled {} rules in {:?}, {} chunks",
            c.rule_count,
            t.elapsed(),
            c.content_filters.len()
        );
        assert!(c.rule_count > 10_000);
        for f in &c.content_filters {
            assert!(serde_json::from_str::<serde_json::Value>(f).is_ok());
        }
    }
}
