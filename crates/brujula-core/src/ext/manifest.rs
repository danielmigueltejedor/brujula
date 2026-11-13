//! `manifest.json` of a Chromium extension, the parts Brújula uses. Manifest versions 2 and 3
//! are read; anything it does not understand is ignored, so an extension that asks for more
//! than is supported still installs and the missing parts simply do nothing.

use std::collections::BTreeMap;

use serde_json::Value;

use super::messages::{localize, Messages};
use super::pattern::{parse_all, Pattern};

const MAX_SCRIPTS: usize = 50;
const MAX_FILES_PER_SCRIPT: usize = 50;

/// A path inside the extension: relative, no `..`, no backslashes or control characters.
pub fn safe_path(path: &str) -> Option<String> {
    let p = path.trim_start_matches("./").trim_start_matches('/');
    let ok = !p.is_empty()
        && p.len() <= 300
        && !p.contains(['\\', '\0'])
        && !p.chars().any(char::is_control)
        && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..");
    ok.then(|| p.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunAt {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentScript {
    pub matches: Vec<Pattern>,
    pub excludes: Vec<Pattern>,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub run_at: RunAt,
    pub all_frames: bool,
    /// Runs in the page's own world, without the extension API.
    pub main_world: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Action {
    pub popup: Option<String>,
    pub title: String,
    /// Icon file by size.
    pub icons: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Background {
    /// Scripts to run, in order (one for a service worker).
    pub scripts: Vec<String>,
    pub module: bool,
    /// An HTML page that holds them instead (Manifest V2 `background.page`).
    pub page: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleResource {
    pub id: String,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub manifest_version: u8,
    /// API permissions (`storage`, `tabs`, …), host patterns not included.
    pub permissions: Vec<String>,
    /// Hosts the extension asks to reach (`host_permissions`, or the MV2 ones in `permissions`).
    pub hosts: Vec<Pattern>,
    pub hosts_text: Vec<String>,
    pub content_scripts: Vec<ContentScript>,
    pub action: Option<Action>,
    pub background: Option<Background>,
    pub options_page: Option<String>,
    pub default_locale: Option<String>,
    pub icons: BTreeMap<u32, String>,
    pub rulesets: Vec<RuleResource>,
    /// Files that web pages may load from the extension (`web_accessible_resources`), as globs.
    pub web_accessible: Vec<String>,
    /// `key` of the manifest (base64 public key), which fixes the extension's id.
    pub key: Option<String>,
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(500)
        .collect()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .take(500)
                .collect()
        })
        .unwrap_or_default()
}

fn paths(v: Option<&Value>) -> Vec<String> {
    strings(v)
        .iter()
        .filter_map(|p| safe_path(p))
        .take(MAX_FILES_PER_SCRIPT)
        .collect()
}

fn icon_map(v: Option<&Value>) -> BTreeMap<u32, String> {
    match v {
        Some(Value::Object(o)) => o
            .iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, safe_path(v.as_str()?)?)))
            .collect(),
        // A single file for every size.
        Some(Value::String(s)) => safe_path(s)
            .map(|p| BTreeMap::from([(32, p)]))
            .unwrap_or_default(),
        _ => BTreeMap::new(),
    }
}

fn is_host_pattern(p: &str) -> bool {
    p == "<all_urls>" || p.contains("://")
}

/// `web_accessible_resources`: a list of paths (MV2) or `{resources: [...]}` objects (MV3).
fn web_accessible(v: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(a)) = v else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in a.iter().take(200) {
        match item {
            Value::String(s) => out.extend(safe_glob(s)),
            Value::Object(_) => out.extend(
                strings(item.get("resources"))
                    .iter()
                    .filter_map(|r| safe_glob(r)),
            ),
            _ => {}
        }
    }
    out
}

fn safe_glob(g: &str) -> Option<String> {
    let t = g.trim_start_matches('/');
    let ok = !t.is_empty()
        && t.len() <= 300
        && !t.contains(['\\', '\0'])
        && !t.split('/').any(|s| s == ".." || s == ".");
    ok.then(|| t.to_owned())
}

fn nearest(icons: &BTreeMap<u32, String>, size: u32) -> Option<&str> {
    icons
        .iter()
        .min_by_key(|(s, _)| (i64::from(**s) - i64::from(size)).abs())
        .map(|(_, p)| p.as_str())
}

impl Manifest {
    /// Reads the manifest; `messages` are the extension's texts for `__MSG_x__`.
    pub fn parse(json: &str, messages: &Messages) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("manifest.json: {e}"))?;
        let manifest_version = match v.get("manifest_version").and_then(Value::as_u64) {
            Some(2) => 2,
            Some(3) => 3,
            _ => return Err("solo manifest_version 2 o 3".to_owned()),
        };
        let name = localize(&text(&v, "name"), messages);
        if name.trim().is_empty() {
            return Err("la extensión no tiene nombre".to_owned());
        }
        let mut permissions = Vec::new();
        let mut host_texts = strings(v.get("host_permissions"));
        for p in strings(v.get("permissions")) {
            if is_host_pattern(&p) {
                host_texts.push(p);
            } else {
                permissions.push(p);
            }
        }
        // MV3 lists optional hosts apart; they are not granted until asked for.
        let hosts = parse_all(host_texts.iter().map(String::as_str));
        let hosts_text: Vec<String> = host_texts
            .into_iter()
            .filter(|t| Pattern::parse(t).is_some())
            .collect();

        let content_scripts = v
            .get("content_scripts")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .take(MAX_SCRIPTS)
                    .filter_map(|c| {
                        let matches =
                            parse_all(strings(c.get("matches")).iter().map(String::as_str));
                        let js = paths(c.get("js"));
                        let css = paths(c.get("css"));
                        if matches.is_empty() || (js.is_empty() && css.is_empty()) {
                            return None;
                        }
                        Some(ContentScript {
                            matches,
                            excludes: parse_all(
                                strings(c.get("exclude_matches")).iter().map(String::as_str),
                            ),
                            js,
                            css,
                            run_at: match c.get("run_at").and_then(Value::as_str) {
                                Some("document_start") => RunAt::Start,
                                _ => RunAt::End,
                            },
                            all_frames: c.get("all_frames").and_then(Value::as_bool) == Some(true),
                            main_world: c.get("world").and_then(Value::as_str) == Some("MAIN"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let action_key = ["action", "browser_action", "page_action"]
            .into_iter()
            .find(|k| v.get(*k).is_some_and(Value::is_object));
        let action = action_key.and_then(|k| v.get(k)).map(|a| Action {
            popup: a
                .get("default_popup")
                .and_then(Value::as_str)
                .and_then(safe_path),
            title: localize(&text(a, "default_title"), messages),
            icons: icon_map(a.get("default_icon")),
        });

        let background = v.get("background").and_then(|b| {
            let scripts = if let Some(s) = b.get("service_worker").and_then(Value::as_str) {
                safe_path(s).into_iter().collect()
            } else {
                paths(b.get("scripts"))
            };
            let page = b.get("page").and_then(Value::as_str).and_then(safe_path);
            (!scripts.is_empty() || page.is_some()).then(|| Background {
                scripts,
                module: b.get("type").and_then(Value::as_str) == Some("module"),
                page,
            })
        });

        let options_page = v
            .get("options_ui")
            .and_then(|o| o.get("page"))
            .or_else(|| v.get("options_page"))
            .and_then(Value::as_str)
            .and_then(safe_path);

        let rulesets = v
            .get("declarative_net_request")
            .and_then(|d| d.get("rule_resources"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .take(10)
                    .filter_map(|r| {
                        Some(RuleResource {
                            id: r.get("id")?.as_str()?.chars().take(60).collect(),
                            enabled: r.get("enabled").and_then(Value::as_bool) == Some(true),
                            path: safe_path(r.get("path")?.as_str()?)?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            name,
            version: text(&v, "version"),
            description: localize(&text(&v, "description"), messages),
            manifest_version,
            permissions,
            hosts,
            hosts_text,
            content_scripts,
            action,
            background,
            options_page,
            default_locale: v
                .get("default_locale")
                .and_then(Value::as_str)
                .filter(|l| {
                    l.len() <= 16
                        && l.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                })
                .map(str::to_owned),
            icons: icon_map(v.get("icons")),
            rulesets,
            web_accessible: web_accessible(v.get("web_accessible_resources")),
            key: v.get("key").and_then(Value::as_str).map(str::to_owned),
        })
    }

    /// Whether a web page may load `path` from the extension.
    pub fn is_web_accessible(&self, path: &str) -> bool {
        self.web_accessible
            .iter()
            .any(|g| super::pattern::glob_match(g, path))
    }

    pub fn has_permission(&self, name: &str) -> bool {
        self.permissions.iter().any(|p| p == name)
    }

    /// Whether the extension may reach `url` from its own pages (cross-origin requests).
    pub fn can_reach(&self, url: &str) -> bool {
        super::pattern::any_match(&self.hosts, url)
    }

    /// The icon of the extension nearest to `size`, from the action or the manifest.
    pub fn icon_for(&self, size: u32) -> Option<&str> {
        nearest(&self.icons, size)
            .or_else(|| self.action.as_ref().and_then(|a| nearest(&a.icons, size)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MV3: &str = r#"{
      "manifest_version": 3, "name": "__MSG_n__", "version": "1.2", "description": "d",
      "permissions": ["storage", "tabs", "https://legacy.example/*"],
      "host_permissions": ["https://*.example.com/*"],
      "content_scripts": [
        {"matches": ["https://*.example.com/*"], "exclude_matches": ["https://x.example.com/*"],
         "js": ["a.js", "../evil.js", "/b.js"], "css": ["c.css"], "run_at": "document_start", "all_frames": true},
        {"matches": ["nonsense"], "js": ["x.js"]},
        {"matches": ["<all_urls>"], "world": "MAIN", "js": ["m.js"]}
      ],
      "action": {"default_popup": "popup.html", "default_title": "T", "default_icon": {"16": "i16.png", "48": "i48.png"}},
      "background": {"service_worker": "sw.js", "type": "module"},
      "options_ui": {"page": "options.html"},
      "icons": {"128": "big.png", "16": "small.png"},
      "default_locale": "es",
      "web_accessible_resources": [{"resources": ["img/*", "/inject.js"], "matches": ["<all_urls>"]}],
      "declarative_net_request": {"rule_resources": [{"id": "r1", "enabled": true, "path": "rules.json"}]}
    }"#;

    fn messages() -> Messages {
        Messages::parse(r#"{"n": {"message": "Nombre"}}"#)
    }

    #[test]
    fn a_manifest_v3_is_read() {
        let m = Manifest::parse(MV3, &messages()).unwrap();
        assert_eq!(m.name, "Nombre");
        assert_eq!(m.manifest_version, 3);
        assert_eq!(m.permissions, vec!["storage", "tabs"]);
        assert_eq!(
            m.hosts_text,
            vec!["https://*.example.com/*", "https://legacy.example/*"]
        );
        assert_eq!(m.content_scripts.len(), 2);
        let c = &m.content_scripts[0];
        assert_eq!(c.js, vec!["a.js", "b.js"]);
        assert_eq!(c.css, vec!["c.css"]);
        assert_eq!(c.run_at, RunAt::Start);
        assert!(c.all_frames && !c.main_world);
        assert!(c.excludes[0].matches("https://x.example.com/"));
        assert!(m.content_scripts[1].main_world);
        let a = m.action.as_ref().unwrap();
        assert_eq!(a.popup.as_deref(), Some("popup.html"));
        let b = m.background.as_ref().unwrap();
        assert_eq!(b.scripts, vec!["sw.js"]);
        assert!(b.module);
        assert_eq!(m.options_page.as_deref(), Some("options.html"));
        assert_eq!(m.default_locale.as_deref(), Some("es"));
        assert_eq!(m.rulesets[0].path, "rules.json");
        assert!(m.is_web_accessible("img/a.png") && m.is_web_accessible("inject.js"));
        assert!(!m.is_web_accessible("popup.html"));
        assert!(m.has_permission("tabs") && !m.has_permission("cookies"));
        assert!(m.can_reach("https://api.example.com/x"));
        assert!(!m.can_reach("https://other.org/"));
    }

    #[test]
    fn icons_pick_the_nearest_size() {
        let m = Manifest::parse(MV3, &messages()).unwrap();
        assert_eq!(m.icon_for(20), Some("small.png"));
        assert_eq!(m.icon_for(100), Some("big.png"));
    }

    #[test]
    fn a_manifest_v2_with_a_browser_action_and_background_scripts_is_read() {
        let m = Manifest::parse(
            r#"{"manifest_version": 2, "name": "Old", "version": "1",
                "permissions": ["<all_urls>", "storage"],
                "browser_action": {"default_popup": "p.html"},
                "background": {"scripts": ["a.js", "b.js"]}}"#,
            &Messages::default(),
        )
        .unwrap();
        assert_eq!(m.manifest_version, 2);
        assert!(m.can_reach("https://anything.org/"));
        assert_eq!(m.background.unwrap().scripts, vec!["a.js", "b.js"]);
        assert_eq!(m.action.unwrap().popup.as_deref(), Some("p.html"));
    }

    #[test]
    fn bad_manifests_are_refused() {
        let none = Messages::default();
        assert!(Manifest::parse("{no", &none).is_err());
        assert!(Manifest::parse(r#"{"manifest_version": 1, "name": "x"}"#, &none).is_err());
        assert!(Manifest::parse(r#"{"manifest_version": 3}"#, &none).is_err());
        assert!(Manifest::parse(r#"{"manifest_version": 3, "name": " "}"#, &none).is_err());
    }

    #[test]
    fn paths_cannot_leave_the_extension() {
        assert_eq!(safe_path("./a/b.js").as_deref(), Some("a/b.js"));
        assert_eq!(safe_path("/a.js").as_deref(), Some("a.js"));
        for bad in ["", "..", "a/../b", "a//b", "a\\b", "a/./b/..", "a\0b"] {
            assert_eq!(safe_path(bad), None, "{bad:?}");
        }
    }
}
