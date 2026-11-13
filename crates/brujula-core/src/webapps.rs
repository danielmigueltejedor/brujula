//! Web apps: a site opened in a window of its own, with a launcher entry, like an installed
//! application. The list is kept in a file; the launcher only ever carries the id of an entry
//! (letters, digits and hyphens), never the address, so nothing a page says can end up on a
//! command line.

use serde::{Deserialize, Serialize};

const MAX_APPS: usize = 100;
const MAX_NAME: usize = 40;
const MAX_URL: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebApp {
    /// Letters, digits and hyphens: it names the launcher, the icon and the profile folder.
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Webapps {
    items: Vec<WebApp>,
}

fn is_web(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= MAX_URL
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
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

/// The host of a web address, without `www.`.
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or("");
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// The last two labels of a host: `mail.example.com` and `example.com` belong together.
fn base_domain(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').collect();
    labels[labels.len().saturating_sub(2)..].join(".")
}

/// Whether `url` is a page of the same site as the app's `home` address: the links of an app
/// stay in its window, anything else goes to the browser.
pub fn same_site(home: &str, url: &str) -> bool {
    let (a, b) = (host_of(home), host_of(url));
    !a.is_empty() && base_domain(&a) == base_domain(&b)
}

/// An id made from a name: `Mi correo` is `mi-correo`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        };
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(32).collect();
    out.trim_end_matches('-').to_owned()
}

impl Webapps {
    pub fn from_json(text: &str) -> Self {
        let mut w: Self = serde_json::from_str(text).unwrap_or_default();
        w.items
            .retain(|i| valid_id(&i.id) && is_web(&i.url) && !i.name.trim().is_empty());
        for i in &mut w.items {
            i.name = clean_name(&i.name);
        }
        let mut seen = std::collections::HashSet::new();
        w.items.retain(|i| seen.insert(i.id.clone()));
        w.items.truncate(MAX_APPS);
        w
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn items(&self) -> &[WebApp] {
        &self.items
    }

    pub fn get(&self, id: &str) -> Option<&WebApp> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Makes an app, or replaces the one with the same address. The id comes from the name and
    /// is made unique. `None` for an address that is not a web page, or when the list is full.
    pub fn add(&mut self, name: &str, url: &str) -> Option<&WebApp> {
        if !is_web(url) {
            return None;
        }
        let mut name = clean_name(name);
        if name.is_empty() {
            name = host_of(url);
        }
        if name.is_empty() {
            return None;
        }
        if let Some(at) = self.items.iter().position(|i| i.url == url) {
            self.items[at].name = name;
            return self.items.get(at);
        }
        if self.items.len() >= MAX_APPS {
            return None;
        }
        let base = match slug(&name) {
            s if s.is_empty() => "app".to_owned(),
            s => s,
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.items.iter().any(|i| i.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.items.push(WebApp {
            id,
            name,
            url: url.to_owned(),
        });
        self.items.last()
    }

    pub fn remove(&mut self, id: &str) {
        self.items.retain(|i| i.id != id);
    }
}

/// The application id of an app's own process (it has its own profile and its own window).
pub fn app_id(browser_id: &str, id: &str) -> String {
    format!("{browser_id}.WebApp.w{}", id.replace('-', "_"))
}

fn desktop_escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('\\', "\\\\")
}

/// The launcher entry of an app. `exe` is how to start the browser (`brujula`, or the path of
/// the AppImage); it is quoted for the `Exec` line.
pub fn desktop_entry(app: &WebApp, exe: &str, icon: &str, browser_id: &str) -> String {
    let exe: String = exe
        .chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| match c {
            '"' | '`' | '$' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nComment={url}\nExec=\"{exe}\" --webapp {id}\nIcon={icon}\nTerminal=false\nStartupNotify=true\nStartupWMClass={wm}\nCategories=Network;\nX-Brujula-WebApp={id}\n",
        name = desktop_escape(&app.name),
        url = desktop_escape(&app.url),
        id = app.id,
        icon = desktop_escape(icon),
        wm = app_id(browser_id, &app.id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_gets_an_id_from_its_name_and_it_is_unique() {
        let mut w = Webapps::default();
        let a = w
            .add("Mi correo", "https://mail.example.com/")
            .unwrap()
            .clone();
        assert_eq!(a.id, "mi-correo");
        let b = w
            .add("Mi correo", "https://other.example.org/")
            .unwrap()
            .clone();
        assert_eq!(b.id, "mi-correo-2");
        assert_eq!(w.items().len(), 2);
        // The same address again renames the app instead of making another.
        w.add("Correo", "https://mail.example.com/").unwrap();
        assert_eq!(w.items().len(), 2);
        assert_eq!(w.get("mi-correo").unwrap().name, "Correo");
    }

    #[test]
    fn only_web_addresses_make_an_app() {
        let mut w = Webapps::default();
        assert!(w.add("x", "javascript:alert(1)").is_none());
        assert!(w.add("x", "file:///etc/passwd").is_none());
        assert!(w.add("x", "https://a.example/ b").is_none());
        assert!(w.items().is_empty());
        // No name: the host serves.
        assert_eq!(
            w.add("  ", "https://www.example.com/").unwrap().name,
            "example.com"
        );
    }

    #[test]
    fn names_become_safe_ids() {
        assert_eq!(slug("Calendário Ñandú!"), "calendario-nandu");
        assert_eq!(slug("--"), "");
        assert!(valid_id(&slug("A/../b")));
        assert_eq!(slug("A/../b"), "a-b");
    }

    #[test]
    fn a_saved_file_cannot_smuggle_ids_or_addresses() {
        let w = Webapps::from_json(
            r#"{"items":[{"id":"../x","name":"a","url":"https://a.example/"},{"id":"ok","name":"B","url":"https://b.example/"},{"id":"bad","name":"B","url":"data:text/html,hi"},{"id":"ok","name":"dup","url":"https://c.example/"}]}"#,
        );
        assert_eq!(w.items().len(), 1);
        assert_eq!(w.items()[0].id, "ok");
        let again = Webapps::from_json(&w.to_json());
        assert_eq!(again, w);
    }

    #[test]
    fn links_stay_in_the_app_only_inside_its_site() {
        let home = "https://mail.example.com/inbox";
        assert!(same_site(home, "https://example.com/x"));
        assert!(same_site(home, "https://www.example.com/"));
        assert!(same_site(home, "https://accounts.example.com/login"));
        assert!(!same_site(home, "https://evil-example.com/"));
        assert!(!same_site(home, "https://example.org/"));
        assert!(!same_site("", "https://example.org/"));
    }

    #[test]
    fn the_launcher_names_the_app_by_id_only() {
        let app = WebApp {
            id: "mi-correo".into(),
            name: "Mi\ncorreo".into(),
            url: "https://mail.example.com/".into(),
        };
        let text = desktop_entry(
            &app,
            "/opt/Brujula App/Brujula.AppImage",
            "brujula-webapp-mi-correo",
            "dev.x.B",
        );
        assert!(text.contains("Name=Micorreo\n"));
        assert!(text.contains("Exec=\"/opt/Brujula App/Brujula.AppImage\" --webapp mi-correo\n"));
        assert!(text.contains("StartupWMClass=dev.x.B.WebApp.wmi_correo\n"));
        assert!(!text
            .lines()
            .any(|l| l.starts_with("Exec=") && l.contains("example.com")));
    }
}
