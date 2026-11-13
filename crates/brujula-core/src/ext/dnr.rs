//! `declarativeNetRequest` rules of an extension, turned into WebKit content-blocker rules. Only
//! what a content blocker can say is converted (block, allow, upgrade to HTTPS); redirects and
//! header changes are skipped and counted.

use serde_json::{json, Value};

const MAX_RULES: usize = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// A WebKit content-blocker list, as JSON.
    pub json: String,
    pub converted: usize,
    pub skipped: usize,
}

/// `||example.com^*ads` and friends as the regular expression subset WebKit accepts.
pub fn filter_to_regex(filter: &str) -> Option<String> {
    let mut f = filter;
    let mut out = String::new();
    if let Some(rest) = f.strip_prefix("||") {
        out.push_str("^[a-z][a-z0-9+.-]*://([^/]*\\.)?");
        f = rest;
    } else if let Some(rest) = f.strip_prefix('|') {
        out.push('^');
        f = rest;
    }
    let (body, end) = match f.strip_suffix('|') {
        Some(b) => (b, true),
        None => (f, false),
    };
    for c in body.chars() {
        match c {
            '*' => out.push_str(".*"),
            // A separator: anything that is not a letter, a digit or one of `_ - . %`.
            '^' => out.push_str("[^a-zA-Z0-9_.%-]"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '$' | '\\' | '/' | '-' | '|' => {
                if c == '/' || c == '-' {
                    out.push(c);
                } else {
                    out.push('\\');
                    out.push(c);
                }
            }
            c if c.is_control() || !c.is_ascii() => return None,
            c => out.push(c),
        }
    }
    if end {
        out.push('$');
    }
    if out.is_empty() {
        out.push_str(".*");
    }
    (out.len() <= 1000).then_some(out)
}

fn resource_types(names: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for n in names {
        let t = match n.as_str() {
            "main_frame" | "sub_frame" => "document",
            "stylesheet" => "style-sheet",
            "script" => "script",
            "image" => "image",
            "font" => "font",
            "object" | "media" => "media",
            "xmlhttprequest" | "other" | "webtransport" | "webbundle" => "raw",
            "ping" | "csp_report" => "ping",
            "websocket" => "websocket",
            _ => continue,
        };
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

const NOT_DOCUMENTS: &[&str] = &[
    "image", "style-sheet", "script", "font", "raw", "media", "ping", "websocket",
];

fn domains(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|d| {
                    !d.is_empty()
                        && d.len() <= 253
                        && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                })
                .map(|d| format!("*{}", d.to_ascii_lowercase()))
                .take(1000)
                .collect()
        })
        .unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// One rule as a WebKit rule, or `None` when it cannot be expressed.
fn convert_rule(rule: &Value) -> Option<(i64, bool, Value)> {
    let cond = rule.get("condition")?;
    let action = rule.get("action")?;
    let kind = match action.get("type")?.as_str()? {
        "block" => json!({"type": "block"}),
        "allow" => json!({"type": "ignore-previous-rules"}),
        "upgradeScheme" => json!({"type": "make-https"}),
        _ => return None,
    };
    let allow = kind["type"] == "ignore-previous-rules";
    // A host list for the request cannot be combined with a filter; leave such a rule out
    // rather than make it wider than it was written.
    if cond.get("requestDomains").is_some() || cond.get("excludedRequestDomains").is_some() {
        return None;
    }
    let regex = if let Some(r) = cond.get("regexFilter").and_then(Value::as_str) {
        // WebKit's expressions have no alternation and no counted repetition.
        if r.contains(['|', '{', '}']) {
            return None;
        }
        r.to_owned()
    } else {
        filter_to_regex(cond.get("urlFilter").and_then(Value::as_str).unwrap_or(""))?
    };
    let mut trigger = json!({
        "url-filter": regex,
        "url-filter-is-case-sensitive": cond.get("isUrlFilterCaseSensitive").and_then(Value::as_bool) == Some(true),
    });
    let included = strings(cond.get("resourceTypes"));
    let excluded = strings(cond.get("excludedResourceTypes"));
    let mut types = if included.is_empty() {
        // DNR leaves the main frame out unless it is asked for.
        let mut all: Vec<&'static str> = NOT_DOCUMENTS.to_vec();
        all.push("svg-document");
        all.push("popup");
        all
    } else {
        resource_types(&included)
    };
    let drop = resource_types(&excluded);
    types.retain(|t| !drop.contains(t));
    if types.is_empty() {
        return None;
    }
    trigger["resource-type"] = json!(types);
    match cond.get("domainType").and_then(Value::as_str) {
        Some("thirdParty") => trigger["load-type"] = json!(["third-party"]),
        Some("firstParty") => trigger["load-type"] = json!(["first-party"]),
        _ => {}
    }
    let if_domain = domains(cond.get("initiatorDomains").or_else(|| cond.get("domains")));
    if !if_domain.is_empty() {
        trigger["if-domain"] = json!(if_domain);
    } else {
        let unless = domains(
            cond.get("excludedInitiatorDomains")
                .or_else(|| cond.get("excludedDomains")),
        );
        if !unless.is_empty() {
            trigger["unless-domain"] = json!(unless);
        }
    }
    let priority = rule.get("priority").and_then(Value::as_i64).unwrap_or(1);
    Some((priority, allow, json!({"trigger": trigger, "action": kind})))
}

/// Converts a rules file (a JSON array). Lower priorities come first and, at the same priority,
/// blocks before allows, so that the later rule wins the way DNR says it should.
pub fn convert(rules_json: &str) -> Result<Converted, String> {
    let Ok(Value::Array(rules)) = serde_json::from_str::<Value>(rules_json) else {
        return Err("las reglas no son una lista JSON".to_owned());
    };
    let total = rules.len().min(MAX_RULES);
    let mut done: Vec<(i64, bool, Value)> =
        rules.iter().take(MAX_RULES).filter_map(convert_rule).collect();
    done.sort_by_key(|(p, allow, _)| (*p, *allow));
    let converted = done.len();
    let list: Vec<Value> = done.into_iter().map(|(_, _, r)| r).collect();
    Ok(Converted {
        json: serde_json::to_string(&list).map_err(|e| e.to_string())?,
        converted,
        skipped: total - converted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_filters_become_regular_expressions() {
        assert_eq!(filter_to_regex("ads").unwrap(), "ads");
        assert_eq!(filter_to_regex("*/ads/*.js").unwrap(), ".*/ads/.*\\.js");
        assert_eq!(
            filter_to_regex("||example.com^").unwrap(),
            "^[a-z][a-z0-9+.-]*://([^/]*\\.)?example\\.com[^a-zA-Z0-9_.%-]"
        );
        assert_eq!(filter_to_regex("|https://a.b/x|").unwrap(), "^https://a\\.b/x$");
        assert_eq!(filter_to_regex("").unwrap(), ".*");
        assert_eq!(filter_to_regex("a?b=1").unwrap(), "a\\?b=1");
        assert!(filter_to_regex("ñ").is_none());
    }

    #[test]
    fn rules_are_converted_ordered_and_counted() {
        let rules = r#"[
          {"id":1,"priority":1,"action":{"type":"block"},"condition":{"urlFilter":"||tracker.example^","resourceTypes":["script","image"]}},
          {"id":2,"priority":2,"action":{"type":"allow"},"condition":{"urlFilter":"||tracker.example/ok^","initiatorDomains":["Good.com"]}},
          {"id":3,"priority":1,"action":{"type":"allow"},"condition":{"urlFilter":"x"}},
          {"id":4,"action":{"type":"redirect","redirect":{"url":"https://a/"}},"condition":{"urlFilter":"y"}},
          {"id":5,"action":{"type":"block"},"condition":{"urlFilter":"z","domainType":"thirdParty","excludedResourceTypes":["image","script","font","xmlhttprequest","media","ping","websocket","other","stylesheet"]}},
          {"id":6,"action":{"type":"block"},"condition":{"requestDomains":["a.com"]}},
          {"id":7,"action":{"type":"upgradeScheme"},"condition":{"urlFilter":"|http://","resourceTypes":["main_frame"]}}
        ]"#;
        let c = convert(rules).unwrap();
        assert_eq!((c.converted, c.skipped), (5, 2));
        let list: Vec<Value> = serde_json::from_str(&c.json).unwrap();
        // The default priority is 1. At the same priority the blocks come before the allow, and
        // priority 2 comes last.
        assert_eq!(list[0]["action"]["type"], "block");
        assert_eq!(list[0]["trigger"]["resource-type"], json!(["script", "image"]));
        assert_eq!(list[1]["trigger"]["load-type"], json!(["third-party"]));
        assert_eq!(list[2]["action"]["type"], "make-https");
        assert_eq!(list[3]["action"]["type"], "ignore-previous-rules");
        assert_eq!(list[4]["trigger"]["if-domain"], json!(["*good.com"]));
        // Without resourceTypes the main frame is left out.
        assert!(list[3]["trigger"]["resource-type"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t != "document"));
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(convert("{").is_err());
        assert!(convert("{}").is_err());
        assert_eq!(convert("[1, null, {}]").unwrap().converted, 0);
    }
}
