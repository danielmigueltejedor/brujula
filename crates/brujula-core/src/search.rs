//! Search engines the user can choose from, and how to turn one into an HTML form (the new-tab
//! page searches with a plain form, no scripts).

use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Engine {
    pub id: &'static str,
    pub name: &'static str,
    /// Search URL, `%s` is the query.
    pub template: &'static str,
    /// The letter of the engine's badge, and the colour behind it.
    pub mark: &'static str,
    pub color: &'static str,
}

pub const ENGINES: &[Engine] = &[
    Engine {
        id: "google",
        name: "Google",
        template: "https://www.google.com/search?q=%s",
        mark: "G",
        color: "#4285f4",
    },
    Engine {
        id: "duckduckgo",
        name: "DuckDuckGo",
        template: "https://duckduckgo.com/?q=%s",
        mark: "D",
        color: "#de5833",
    },
    Engine {
        id: "bing",
        name: "Bing",
        template: "https://www.bing.com/search?q=%s",
        mark: "B",
        color: "#0b8a8a",
    },
    Engine {
        id: "brave",
        name: "Brave Search",
        template: "https://search.brave.com/search?q=%s",
        mark: "B",
        color: "#fb542b",
    },
    Engine {
        id: "ecosia",
        name: "Ecosia",
        template: "https://www.ecosia.org/search?q=%s",
        mark: "E",
        color: "#3f9d4a",
    },
    Engine {
        id: "startpage",
        name: "Startpage",
        template: "https://www.startpage.com/do/search?q=%s",
        mark: "S",
        color: "#5a67f2",
    },
    Engine {
        id: "qwant",
        name: "Qwant",
        template: "https://www.qwant.com/?q=%s",
        mark: "Q",
        color: "#2f4bd6",
    },
];

impl Engine {
    /// The engine's badge for an HTML page: its letter on its colour (the engines' own logos are
    /// theirs, so the page draws a plain badge).
    pub fn badge(&self) -> String {
        format!(
            r#"<span class="eng" style="background:{}" aria-hidden="true">{}</span>"#,
            self.color, self.mark
        )
    }
}

pub const DEFAULT_ENGINE: &str = "google";

pub fn by_id(id: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|e| e.id == id)
}

/// The engine for `id`, or the default when it is unknown (a settings file from a newer or older
/// version, or edited by hand).
pub fn engine_or_default(id: &str) -> &'static Engine {
    by_id(id).unwrap_or_else(|| by_id(DEFAULT_ENGINE).expect("default engine exists"))
}

/// A search template split for an HTML form: where it submits, the fixed parameters, and the
/// name of the field that carries the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormParts {
    pub action: String,
    pub hidden: Vec<(String, String)>,
    pub query_name: String,
}

pub fn form_parts(template: &str) -> Option<FormParts> {
    let url = Url::parse(&template.replace("%s", "QUERY_PLACEHOLDER")).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    let mut query_name = None;
    let mut hidden = Vec::new();
    for (k, v) in url.query_pairs() {
        if v == "QUERY_PLACEHOLDER" {
            query_name = Some(k.into_owned());
        } else {
            hidden.push((k.into_owned(), v.into_owned()));
        }
    }
    let mut action = url.clone();
    action.set_query(None);
    action.set_fragment(None);
    Some(FormParts {
        action: action.to_string(),
        hidden,
        query_name: query_name?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_is_https_and_has_a_form() {
        let mut ids = std::collections::HashSet::new();
        for e in ENGINES {
            assert!(e.template.starts_with("https://"), "{}", e.id);
            assert!(e.template.contains("%s"), "{}", e.id);
            assert!(ids.insert(e.id), "duplicate {}", e.id);
            assert!(form_parts(e.template).is_some(), "{}", e.id);
        }
    }

    #[test]
    fn google_is_the_default_and_first() {
        assert_eq!(ENGINES[0].id, DEFAULT_ENGINE);
        assert_eq!(engine_or_default("nope").id, "google");
        assert_eq!(engine_or_default("bing").name, "Bing");
    }

    #[test]
    fn form_parts_split_the_template() {
        let p = form_parts("https://example.com/search?hl=es&q=%s").unwrap();
        assert_eq!(p.action, "https://example.com/search");
        assert_eq!(p.query_name, "q");
        assert_eq!(p.hidden, vec![("hl".to_owned(), "es".to_owned())]);
    }

    #[test]
    fn form_parts_reject_templates_without_a_query_field() {
        assert!(form_parts("https://example.com/%s").is_none());
        assert!(form_parts("javascript:alert(1)//?q=%s").is_none());
        assert!(form_parts("not a url").is_none());
    }
}
