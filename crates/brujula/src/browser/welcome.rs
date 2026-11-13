//! The welcome page of the first run: bookmarks from another browser, the default browser,
//! essentials and the first profile. The page itself is in `brujula_core::welcome`; this is what
//! it asks the browser to do.

use std::path::{Path, PathBuf};

use brujula_core::bookmark_import::{self, Imported};
use brujula_core::welcome::{self, Action as Ask, Finish, Say, Source};

use super::*;

/// The most a bookmarks file may weigh: more is not a bookmarks file.
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// A browser on this computer whose bookmarks can be read.
struct Found {
    name: &'static str,
    path: PathBuf,
}

/// Browsers of the Chromium family: where their folder is, under the home folder, natively and
/// as a Flatpak or Snap.
const CHROMIUM: &[(&str, &[&str])] = &[
    (
        "Chrome",
        &[
            ".config/google-chrome",
            ".var/app/com.google.Chrome/config/google-chrome",
        ],
    ),
    (
        "Chromium",
        &[
            ".config/chromium",
            ".var/app/org.chromium.Chromium/config/chromium",
            "snap/chromium/common/chromium",
        ],
    ),
    (
        "Brave",
        &[
            ".config/BraveSoftware/Brave-Browser",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
        ],
    ),
    (
        "Edge",
        &[
            ".config/microsoft-edge",
            ".var/app/com.microsoft.Edge/config/microsoft-edge",
        ],
    ),
    (
        "Vivaldi",
        &[
            ".config/vivaldi",
            ".var/app/com.vivaldi.Vivaldi/config/vivaldi",
        ],
    ),
    ("Opera", &[".config/opera"]),
];

/// Firefox and the browsers made from it keep a dated backup of the bookmarks in each profile.
const MOZILLA: &[(&str, &[&str])] = &[
    (
        "Firefox",
        &[
            ".mozilla/firefox",
            ".var/app/org.mozilla.firefox/.mozilla/firefox",
            "snap/firefox/common/.mozilla/firefox",
        ],
    ),
    ("Zen", &[".zen", ".var/app/app.zen_browser.zen/.zen"]),
    ("LibreWolf", &[".librewolf"]),
    ("Floorp", &[".floorp"]),
];

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The newest backup of the bookmarks among the profiles in `base`.
fn newest_backup(base: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for profile in std::fs::read_dir(base).ok()?.flatten() {
        let Ok(backups) = std::fs::read_dir(profile.path().join("bookmarkbackups")) else {
            continue;
        };
        for file in backups.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonlz4") {
                continue;
            }
            if let Some(at) = modified(&path) {
                if best.as_ref().is_none_or(|(t, _)| at > *t) {
                    best = Some((at, path));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The browsers of this computer that have bookmarks to bring.
fn detect() -> Vec<Found> {
    let home = glib::home_dir();
    let mut found = Vec::new();
    for (name, dirs) in CHROMIUM {
        let file = dirs
            .iter()
            .map(|d| home.join(d).join("Default").join("Bookmarks"))
            .find(|p| p.is_file());
        if let Some(path) = file {
            found.push(Found { name, path });
        }
    }
    for (name, dirs) in MOZILLA {
        let file = dirs.iter().find_map(|d| newest_backup(&home.join(d)));
        if let Some(path) = file {
            found.push(Found { name, path });
        }
    }
    found
}

fn read_bookmarks(path: &Path) -> Option<Vec<Imported>> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    bookmark_import::parse(&std::fs::read(path).ok()?).ok()
}

/// The icon theme's pictures of the suggested sites, as `data:` addresses (SVG or PNG).
fn suggested_icons() -> Vec<(String, String)> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let theme = gtk::IconTheme::for_display(&display);
    brujula_core::essentials::SUGGESTED
        .iter()
        .filter(|s| !s.icon.is_empty() && theme.has_icon(s.icon))
        .filter_map(|s| {
            let file = theme
                .lookup_icon(
                    s.icon,
                    &[],
                    64,
                    1,
                    gtk::TextDirection::None,
                    gtk::IconLookupFlags::empty(),
                )
                .file()?;
            let path = file.path()?;
            let kind = match path.extension()?.to_str()? {
                "svg" => "image/svg+xml",
                "png" => "image/png",
                _ => return None,
            };
            // An icon file is a few KB; a huge one is not an icon.
            if std::fs::metadata(&path).ok()?.len() > 256 * 1024 {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            Some((
                s.url.to_owned(),
                format!("data:{kind};base64,{}", glib::base64_encode(&bytes)),
            ))
        })
        .collect()
}

fn system_lang() -> String {
    glib::language_names()
        .first()
        .map(|l| l.to_string())
        .unwrap_or_default()
}

/// Runs a line of script in the page.
fn tell(view: &WebView, script: &str) {
    view.evaluate_javascript(script, None, None, gio::Cancellable::NONE, |_| {});
}

/// A string as a script literal that cannot close the script around it.
fn literal(text: &str) -> String {
    serde_json::to_string(text)
        .unwrap_or_default()
        .replace('<', "\\u003c")
}

impl Browser {
    /// Whether the first run still has to go through the welcome page.
    pub(super) fn needs_welcome(&self) -> bool {
        !self.private && !self.settings.borrow().welcome_done
    }

    pub(super) fn render_welcome(&self, view: &WebView) {
        let sources: Vec<Source> = detect()
            .into_iter()
            .map(|f| Source {
                name: f.name.to_owned(),
            })
            .collect();
        let settings = self.settings.borrow();
        let theme = match settings.appearance.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        let html = welcome::render(&welcome::Page {
            lang: &system_lang(),
            theme,
            accent: &settings.appearance.accent,
            sources: &sources,
            icons: &suggested_icons(),
            ask_profile: crate::profiles::load().is_empty(),
        });
        drop(settings);
        view.load_html(&html, Some(welcome::WELCOME_URI));
    }

    /// One thing the welcome page asked for.
    pub(super) fn welcome_ask(self: &Rc<Self>, view: &WebView, ask: Ask) {
        match ask {
            Ask::Import(n) => {
                let path = detect().into_iter().nth(n).map(|f| f.path);
                self.welcome_import(view, &n.to_string(), path.as_deref());
            }
            Ask::ImportFile => self.welcome_pick_file(view),
            Ask::DefaultBrowser => {
                let say = if make_default() {
                    Say::DefaultDone
                } else {
                    Say::DefaultFailed
                };
                let ok = matches!(say, Say::DefaultDone);
                let text = literal(&welcome::say(&system_lang(), &say));
                tell(view, &format!("welcome.defaulted({ok}, {text})"));
            }
            Ask::Finish(f) => self.welcome_finish(view, f),
            Ask::Skip => self.welcome_leave(view),
        }
    }

    /// Reads the bookmarks of `path`, adds the new ones and tells the page how it went.
    fn welcome_import(&self, view: &WebView, which: &str, path: Option<&Path>) {
        let lang = system_lang();
        let items = path.and_then(read_bookmarks);
        let say = match items {
            Some(items) => {
                let now = now_ms();
                let mut marks = self.bookmarks.borrow_mut();
                let before = marks.items().len();
                // The first of the file ends up on top.
                for i in items.iter().rev() {
                    if !marks.contains(&i.url) {
                        marks.toggle_at(&i.url, &i.title, now);
                    }
                }
                let added = marks.items().len().saturating_sub(before);
                drop(marks);
                self.save_bookmarks();
                self.refresh_bookmark_action();
                Say::Imported(added)
            }
            None => Say::ImportFailed,
        };
        let count = match say {
            Say::Imported(n) => n,
            _ => 0,
        };
        let text = literal(&welcome::say(&lang, &say));
        tell(
            view,
            &format!("welcome.imported({}, {count}, {text})", literal(which)),
        );
    }

    fn welcome_pick_file(self: &Rc<Self>, view: &WebView) {
        let dialog = gtk::FileDialog::builder()
            .title(welcome::file_title(&system_lang()))
            .modal(true)
            .build();
        dialog.set_initial_folder(Some(&gio::File::for_path(glib::home_dir())));
        let this = self.clone();
        let view = view.clone();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            if let Some(path) = result.ok().and_then(|f| f.path()) {
                this.welcome_import(&view, "file", Some(&path));
            }
        });
    }

    /// Saves what was chosen and goes to the new-tab page.
    fn welcome_finish(self: &Rc<Self>, view: &WebView, f: Finish) {
        let mut profiles = crate::profiles::load();
        if profiles.is_empty() {
            match profiles.add(&f.name, &f.avatar, &f.password) {
                Some(_) => {
                    if crate::profiles::save(&profiles).is_err() {
                        self.essential_alert(
                            "No se pudo guardar el perfil",
                            "No se pudo escribir el archivo de perfiles. Comprueba el espacio y los permisos de tu carpeta personal.",
                        );
                        return;
                    }
                }
                None => {
                    // The page checks the length; this is only a page that skipped the check.
                    tell(view, "document.getElementById('pw-error').hidden=false");
                    return;
                }
            }
        }
        for url in &f.essentials {
            let name = brujula_core::essentials::SUGGESTED
                .iter()
                .find(|s| s.url == url)
                .map_or("", |s| s.name);
            self.essentials.borrow_mut().add(name, url);
        }
        self.save_essentials();
        self.refresh_essentials();
        self.welcome_leave(view);
    }

    /// Marks the welcome as seen and replaces its tab with a new one.
    fn welcome_leave(self: &Rc<Self>, view: &WebView) {
        self.settings.borrow_mut().welcome_done = true;
        self.save_settings();
        let welcome_tab = self.id_of(view);
        self.open_tab(newtab::NEWTAB_URI, None, true);
        if let Some(id) = welcome_tab {
            self.close_tab(id);
        }
    }
}

/// Sets Brújula as the program for web pages and links. False if it is not installed (its
/// launcher is not in a place the system looks in) or the system refuses.
fn make_default() -> bool {
    let id = format!("{}.desktop", paths::APP_ID);
    let Some(info) = gio::AppInfo::all()
        .into_iter()
        .find(|a| a.id().is_some_and(|i| i == id.as_str()))
    else {
        return false;
    };
    [
        "text/html",
        "x-scheme-handler/http",
        "x-scheme-handler/https",
    ]
    .iter()
    .all(|kind| info.set_as_default_for_type(kind).is_ok())
}
