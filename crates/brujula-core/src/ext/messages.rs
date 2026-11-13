//! `_locales/<lang>/messages.json`: the texts of an extension in each language, and `__MSG_x__`
//! references to them in its manifest.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Messages {
    map: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    message: String,
    /// `name → content` of `placeholders`, names in lower case.
    placeholders: BTreeMap<String, String>,
}

impl Messages {
    pub fn parse(json: &str) -> Self {
        let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(json) else {
            return Self::default();
        };
        let mut map = BTreeMap::new();
        for (key, value) in obj.iter().take(5000) {
            let Some(message) = value.get("message").and_then(Value::as_str) else {
                continue;
            };
            let placeholders = value
                .get("placeholders")
                .and_then(Value::as_object)
                .map(|p| {
                    p.iter()
                        .filter_map(|(k, v)| {
                            Some((
                                k.to_ascii_lowercase(),
                                v.get("content")?.as_str()?.to_owned(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            map.insert(
                key.to_ascii_lowercase(),
                Entry {
                    message: message.to_owned(),
                    placeholders,
                },
            );
        }
        Self { map }
    }

    /// Text of `name` with `$NAME$` placeholders and `$1`..`$9` substitutions filled in.
    pub fn get(&self, name: &str, substitutions: &[String]) -> Option<String> {
        let entry = self.map.get(&name.to_ascii_lowercase())?;
        let mut out = String::new();
        let mut rest = entry.message.as_str();
        while let Some(i) = rest.find('$') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            if after.starts_with('$') {
                out.push('$');
                rest = &after[1..];
            } else if let Some(d) = after.chars().next().filter(char::is_ascii_digit) {
                let n = d.to_digit(10).unwrap_or(0) as usize;
                if n >= 1 {
                    out.push_str(substitutions.get(n - 1).map_or("", String::as_str));
                }
                rest = &after[1..];
            } else if let Some(j) = after.find('$') {
                let content = entry.placeholders.get(&after[..j].to_ascii_lowercase());
                if let Some(content) = content {
                    // A placeholder's content is itself `$1`-style.
                    let n = content
                        .strip_prefix('$')
                        .and_then(|c| c.parse::<usize>().ok());
                    match n {
                        Some(n) if n >= 1 => {
                            out.push_str(substitutions.get(n - 1).map_or("", String::as_str));
                        }
                        _ => out.push_str(content),
                    }
                }
                rest = &after[j + 1..];
            } else {
                out.push('$');
                rest = after;
            }
        }
        out.push_str(rest);
        Some(out)
    }

    /// The texts as the page script wants them: `{name: {message, placeholders}}`, names in
    /// lower case.
    pub fn to_js_json(&self) -> String {
        let obj: serde_json::Map<String, Value> = self
            .map
            .iter()
            .map(|(k, e)| {
                (
                    k.clone(),
                    serde_json::json!({"message": e.message, "placeholders": e.placeholders}),
                )
            })
            .collect();
        Value::Object(obj).to_string()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The `_locales` folders to try, best first: the user's language with and without the region,
/// the extension's default and English.
pub fn locale_candidates(languages: &[String], default_locale: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |l: &str| {
        let l = l.replace('-', "_");
        if !l.is_empty() && l.len() <= 16 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !out.contains(&l) {
            out.push(l);
        }
    };
    for lang in languages {
        let base = lang.split(['.', '@']).next().unwrap_or("");
        push(base);
        if let Some((short, _)) = base.split_once(['_', '-']) {
            push(short);
        }
    }
    if let Some(d) = default_locale {
        push(d);
    }
    push("en");
    out
}

/// `__MSG_name__` in a manifest text, or the text as it is.
pub fn localize(text: &str, messages: &Messages) -> String {
    text.strip_prefix("__MSG_")
        .and_then(|t| t.strip_suffix("__"))
        .and_then(|name| messages.get(name, &[]))
        .unwrap_or_else(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
        "appName": {"message": "Mi extensión"},
        "hello": {"message": "Hola, $USER$ ($1) — $$5", "placeholders": {"user": {"content": "$1"}}},
        "Mixed": {"message": "ok"}
    }"#;

    #[test]
    fn messages_resolve_with_placeholders_and_escapes() {
        let m = Messages::parse(JSON);
        assert_eq!(m.get("appName", &[]).as_deref(), Some("Mi extensión"));
        assert_eq!(
            m.get("hello", &["Ana".into()]).as_deref(),
            Some("Hola, Ana (Ana) — $5")
        );
        assert_eq!(m.get("mixed", &[]).as_deref(), Some("ok"));
        assert_eq!(m.get("nope", &[]), None);
    }

    #[test]
    fn manifest_texts_are_localized() {
        let m = Messages::parse(JSON);
        assert_eq!(localize("__MSG_appName__", &m), "Mi extensión");
        assert_eq!(localize("__MSG_missing__", &m), "__MSG_missing__");
        assert_eq!(localize("plain", &m), "plain");
    }

    #[test]
    fn locales_come_from_the_language_then_the_default_then_english() {
        let langs = vec!["es_ES.UTF-8".to_owned(), "C".to_owned()];
        assert_eq!(
            locale_candidates(&langs, Some("de")),
            vec!["es_ES", "es", "C", "de", "en"]
        );
        assert_eq!(locale_candidates(&[], None), vec!["en"]);
        assert_eq!(locale_candidates(&["../x".to_owned()], None), vec!["en"]);
        let js = Messages::parse(JSON).to_js_json();
        assert!(js.contains(r#""appname":{"message":"Mi extensión""#));
    }

    #[test]
    fn broken_json_is_empty() {
        assert!(Messages::parse("{no").is_empty());
        assert!(Messages::parse("[]").is_empty());
    }
}
