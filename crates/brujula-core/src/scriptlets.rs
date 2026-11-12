//! Scriptlets: the small scripts filter lists run inside pages (`site.com##+js(name, arg)`).
//!
//! Network and hiding rules cannot stop everything. YouTube's video ads, for example, are
//! removed by changing the page's own data before its player reads it, which is what these
//! scripts do. This module reads the rules from the lists and builds the table the browser
//! hands to the page; uBlock Origin's scriptlet code (see `scripts/bundle-ubo-scriptlets.py`)
//! does the work there.
//!
//! Only rules that name sites are used: a scriptlet that runs on every page would cost every
//! page load, and the lists keep those for a handful of cases.

use serde::{Deserialize, Serialize};

/// One `domains##+js(name, args…)` rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Sites it runs on; a site includes its subdomains.
    pub domains: Vec<String>,
    /// Sites it does not run on.
    pub except: Vec<String>,
    /// Scriptlet name without the `.js` suffix, as written in the list (`set`, `json-prune`…).
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scriptlets {
    pub rules: Vec<Rule>,
}

/// An exception: `domains#@#+js(name, args…)`.
struct Exception {
    domains: Vec<String>,
    /// `None` for `#@#+js()`, which switches every scriptlet off on those sites.
    token: Option<(String, Vec<String>)>,
}

/// Reads the scriptlet rules out of filter lists.
pub fn parse<'a>(lists: impl IntoIterator<Item = &'a str>) -> Scriptlets {
    let mut rules = Vec::new();
    let mut exceptions = Vec::new();
    for text in lists {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('!') || !line.contains("+js(") {
                continue;
            }
            let (is_exception, domains_part, body) =
                if let Some((d, b)) = line.split_once("#@#+js(") {
                    (true, d, b)
                } else if let Some((d, b)) = line.split_once("##+js(") {
                    (false, d, b)
                } else {
                    continue;
                };
            let Some(body) = body.strip_suffix(')') else {
                continue;
            };
            let mut positive = Vec::new();
            let mut negative = Vec::new();
            let mut usable = true;
            for d in domains_part.split(',').filter(|d| !d.trim().is_empty()) {
                let d = d.trim();
                let (neg, d) = d.strip_prefix('~').map_or((false, d), |d| (true, d));
                // Entities (`site.*`) and regular expressions are not supported.
                if d.contains('*') || d.starts_with('/') || !is_host(d) {
                    if !neg {
                        usable = false;
                    }
                    continue;
                }
                let d = d.to_ascii_lowercase();
                if neg {
                    negative.push(d);
                } else {
                    positive.push(d);
                }
            }
            let args = split_args(body);
            if is_exception {
                if usable {
                    let token = if body.trim().is_empty() {
                        None
                    } else {
                        args.split_first()
                            .map(|(n, rest)| (normalise(n), rest.to_vec()))
                    };
                    exceptions.push(Exception {
                        domains: positive,
                        token,
                    });
                }
                continue;
            }
            let Some((name, rest)) = args.split_first() else {
                continue;
            };
            if !usable || positive.is_empty() {
                continue;
            }
            rules.push(Rule {
                domains: positive,
                except: negative,
                name: normalise(name),
                args: rest.to_vec(),
            });
        }
    }
    apply_exceptions(&mut rules, &exceptions);
    Scriptlets { rules }
}

fn is_host(d: &str) -> bool {
    d.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
}

/// `set.js` and `set` are the same scriptlet.
fn normalise(name: &str) -> String {
    name.trim().trim_end_matches(".js").to_owned()
}

fn apply_exceptions(rules: &mut Vec<Rule>, exceptions: &[Exception]) {
    for e in exceptions {
        if e.domains.is_empty() {
            // A global exception removes the rules it names.
            rules.retain(|r| {
                !e.token
                    .as_ref()
                    .is_none_or(|(n, a)| *n == r.name && *a == r.args)
            });
            continue;
        }
        for r in rules.iter_mut() {
            let same = e
                .token
                .as_ref()
                .is_none_or(|(n, a)| *n == r.name && *a == r.args);
            if same {
                // Only sites the rule can reach matter; the others would just pad the table.
                let reach = |x: &String| {
                    r.domains.iter().any(|d| {
                        x == d || x.ends_with(&format!(".{d}")) || d.ends_with(&format!(".{x}"))
                    })
                };
                let relevant: Vec<String> =
                    e.domains.iter().filter(|x| reach(x)).cloned().collect();
                r.except.extend(relevant);
            }
        }
    }
}

/// Splits `name, arg, 'quoted, arg'` the way uBlock Origin does: commas separate arguments, a
/// quote opens a span in which commas do not count, `\,` is a comma that is not a separator.
pub fn split_args(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut arg = String::new();
        match chars.peek().copied() {
            Some(q @ ('\'' | '"' | '`')) => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\\' && chars.peek() == Some(&q) {
                        arg.push(q);
                        chars.next();
                    } else if c == q {
                        break;
                    } else {
                        arg.push(c);
                    }
                }
                // Anything between the closing quote and the next comma is dropped.
                for c in chars.by_ref() {
                    if c == ',' {
                        break;
                    }
                }
            }
            _ => {
                let mut ended = true;
                while let Some(c) = chars.next() {
                    if c == '\\' && chars.peek() == Some(&',') {
                        arg.push(',');
                        chars.next();
                    } else if c == ',' {
                        ended = false;
                        break;
                    } else {
                        arg.push(c);
                    }
                }
                out.push(arg.trim_end().to_owned());
                if ended {
                    return out;
                }
                continue;
            }
        }
        out.push(arg);
        if chars.peek().is_none() {
            return out;
        }
    }
}

impl Scriptlets {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// URL patterns of the sites that have a rule, in WebKit's user-script syntax. The scripts
    /// are only put into pages on these sites.
    pub fn url_patterns(&self) -> Vec<String> {
        let mut hosts: Vec<&str> = self
            .rules
            .iter()
            .flat_map(|r| r.domains.iter().map(String::as_str))
            .collect();
        hosts.sort_unstable();
        hosts.dedup();
        hosts.into_iter().map(|h| format!("*://*.{h}/*")).collect()
    }

    /// The rules as a JSON array for the in-page runner:
    /// `[{"d": [sites], "x": [except], "f": "name", "a": [args]}, …]`.
    pub fn table_json(&self) -> String {
        let rows: Vec<serde_json::Value> = self
            .rules
            .iter()
            .map(|r| serde_json::json!({ "d": r.domains, "x": r.except, "f": r.name, "a": r.args }))
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned())
    }

    /// The script put into pages: uBlock Origin's scriptlet code, the table, and a runner that
    /// calls the scriptlets whose rule names the page's site. `bundle` is the code from
    /// `scripts/bundle-ubo-scriptlets.py`; `off` lists sites where blocking is switched off.
    pub fn user_script(&self, bundle: &str, off: &[String]) -> String {
        let off = serde_json::to_string(off).unwrap_or_else(|_| "[]".to_owned());
        format!(
            "(function() {{\n'use strict';\n\
             const host = location.hostname.toLowerCase();\n\
             const within = (list, h) => list.some(d => h === d || h.endsWith('.' + d));\n\
             if (!host || within({off}, host)) return;\n\
             const rules = {table};\n\
             const mine = rules.filter(r => within(r.d, host) && !within(r.x, host));\n\
             if (mine.length === 0) return;\n\
             const registered = {bundle};\n\
             for (const r of mine) {{\n\
               const name = r.f + '.js';\n\
               const d = registered.find(s => s.name === name || (s.aliases && s.aliases.includes(name)));\n\
               if (!d || d.world === 'ISOLATED' || typeof d.fn !== 'function') continue;\n\
               try {{ d.fn(...r.a); }} catch (e) {{}}\n\
             }}\n\
             }})();\n",
            table = self.table_json(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(rule: &str) -> Scriptlets {
        parse([rule])
    }

    #[test]
    fn a_site_rule_is_read() {
        let s = one("www.youtube.com##+js(set, ytInitialPlayerResponse.adPlacements, undefined)");
        assert_eq!(s.rules.len(), 1);
        let r = &s.rules[0];
        assert_eq!(r.domains, ["www.youtube.com"]);
        assert_eq!(r.name, "set");
        assert_eq!(
            r.args,
            ["ytInitialPlayerResponse.adPlacements", "undefined"]
        );
    }

    #[test]
    fn several_sites_and_negated_ones() {
        let s = one("a.com,b.org,~sub.a.com##+js(nowoif)");
        let r = &s.rules[0];
        assert_eq!(r.domains, ["a.com", "b.org"]);
        assert_eq!(r.except, ["sub.a.com"]);
        assert!(r.args.is_empty());
    }

    #[test]
    fn quotes_and_escaped_commas() {
        let s = one(
            r#"y.com##+js(trusted-replace-xhr-response, '"adPlacements"', '"no_ads"', /playlist\?list=|\/player(?:\?.+)?$/)"#,
        );
        let r = &s.rules[0];
        assert_eq!(r.name, "trusted-replace-xhr-response");
        assert_eq!(r.args[0], "\"adPlacements\"");
        assert_eq!(r.args[1], "\"no_ads\"");
        assert_eq!(r.args[2], r"/playlist\?list=|\/player(?:\?.+)?$/");

        let s = one(r"y.com##+js(rmnt, script, /\w{2\,4}/, x)");
        assert_eq!(s.rules[0].args, ["script", r"/\w{2,4}/", "x"]);
    }

    #[test]
    fn an_empty_argument_stays_empty() {
        let s = one("y.com##+js(trusted-replace-xhr-response, /a.*?b/gms, , /player/)");
        assert_eq!(s.rules[0].args, ["/a.*?b/gms", "", "/player/"]);
    }

    #[test]
    fn js_suffix_is_optional() {
        assert_eq!(one("y.com##+js(set.js, a, 1)").rules[0].name, "set");
    }

    #[test]
    fn rules_for_every_site_and_entities_are_skipped() {
        assert!(one("##+js(nowoif)").rules.is_empty());
        assert!(one("google.*##+js(set, a, 1)").rules.is_empty());
        assert!(one("/regex/##+js(set, a, 1)").rules.is_empty());
        assert!(one("! y.com##+js(set, a, 1)").rules.is_empty());
        assert!(one("y.com##.banner").rules.is_empty());
    }

    #[test]
    fn exceptions_switch_a_rule_off_on_their_sites() {
        let s = parse([
            "a.com,b.com##+js(set, x, 1)\nb.com#@#+js(set, x, 1)\n",
            "c.com##+js(set, y, 2)\nc.com#@#+js()\n",
        ]);
        assert_eq!(s.rules[0].except, ["b.com"]);
        assert_eq!(s.rules[1].except, ["c.com"]);
    }

    #[test]
    fn a_global_exception_removes_the_rule() {
        let s = parse(["a.com##+js(set, x, 1)\n#@#+js(set, x, 1)\n"]);
        assert!(s.rules.is_empty());
    }

    #[test]
    fn patterns_cover_each_site_once() {
        let s = parse(["a.com,b.com##+js(set, x, 1)\na.com##+js(set, y, 2)\n"]);
        assert_eq!(s.url_patterns(), ["*://*.a.com/*", "*://*.b.com/*"]);
    }

    #[test]
    fn the_script_embeds_table_bundle_and_off_list() {
        let s = one("a.com##+js(set, x, 1)");
        let js = s.user_script("[/*bundle*/]", &["off.com".to_owned()]);
        assert!(js.contains("[/*bundle*/]"));
        assert!(js.contains(r#"["off.com"]"#));
        assert!(js.contains(r#""f":"set""#));
        assert!(js.contains(r#""a":["x","1"]"#));
    }

    /// Runs against real lists when `BRUJULA_LISTS_DIR` points at a folder with them.
    #[test]
    fn real_lists_have_youtube_rules() {
        let Ok(dir) = std::env::var("BRUJULA_LISTS_DIR") else {
            return;
        };
        let texts: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
            .collect();
        let s = parse(texts.iter().map(String::as_str));
        eprintln!(
            "{} scriptlet rules, {} sites, table {} bytes",
            s.rules.len(),
            s.url_patterns().len(),
            s.table_json().len()
        );
        assert!(s
            .rules
            .iter()
            .any(|r| r.domains.iter().any(|d| d == "www.youtube.com")));
    }
}
