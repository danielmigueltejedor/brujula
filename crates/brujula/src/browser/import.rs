//! Bringing what other browsers keep: bookmarks, history, cookies and passwords. Where each
//! browser keeps its files is here; how they are read is `brujula_core::browser_data` and
//! `brujula_core::bookmark_import`. The files are read off the main thread, from a private copy.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use brujula_core::bookmark_import::{self, Imported};
use brujula_core::browser_data::{
    self, Cookie, HistoryEntry, Keys, Login, Read as Decrypted, SameSite, Snapshot,
};
use brujula_core::history::Visit;
use brujula_core::welcome::{self as words, Counts, Say};
use webkit6::soup;

use super::welcome::system_lang;
use super::*;

/// The most a bookmarks file may weigh: more is not a bookmarks file.
const MAX_FILE: u64 = 64 * 1024 * 1024;
/// How long the keyring may take to hand over a key (it may be asking to be unlocked).
const KEYRING_SECONDS: u64 = 30;

/// What the person wants to bring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Wants {
    pub bookmarks: bool,
    pub history: bool,
    pub cookies: bool,
    pub passwords: bool,
}

impl Wants {
    pub const ALL: Self = Self {
        bookmarks: true,
        history: true,
        cookies: true,
        passwords: true,
    };

    /// What a button of the preferences page asks for.
    pub fn of(kind: &str) -> Option<Self> {
        let none = Self {
            bookmarks: false,
            history: false,
            cookies: false,
            passwords: false,
        };
        Some(match kind {
            "all" => Self::ALL,
            "bookmarks" => Self {
                bookmarks: true,
                ..none
            },
            "history" => Self {
                history: true,
                ..none
            },
            "cookies" => Self {
                cookies: true,
                ..none
            },
            "passwords" => Self {
                passwords: true,
                ..none
            },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Family {
    Chromium,
    Mozilla,
}

/// A browser on this computer that has something to bring.
#[derive(Debug, Clone)]
pub(super) struct Source {
    pub name: &'static str,
    pub family: Family,
    /// The folder of its profile.
    pub profile: PathBuf,
    /// The name its keyring entry carries (Chromium family).
    keyring: &'static str,
}

/// Browsers of the Chromium family: where their folder is under the home folder, natively and as
/// a Flatpak or Snap, and the name their keyring entry carries.
const CHROMIUM: &[(&str, &str, &[&str])] = &[
    (
        "Chrome",
        "chrome",
        &[
            ".config/google-chrome",
            ".var/app/com.google.Chrome/config/google-chrome",
        ],
    ),
    (
        "Chromium",
        "chromium",
        &[
            ".config/chromium",
            ".var/app/org.chromium.Chromium/config/chromium",
            "snap/chromium/common/chromium",
        ],
    ),
    (
        "Brave",
        "brave",
        &[
            ".config/BraveSoftware/Brave-Browser",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
        ],
    ),
    (
        "Edge",
        "microsoft-edge",
        &[
            ".config/microsoft-edge",
            ".var/app/com.microsoft.Edge/config/microsoft-edge",
        ],
    ),
    (
        "Vivaldi",
        "vivaldi",
        &[
            ".config/vivaldi",
            ".var/app/com.vivaldi.Vivaldi/config/vivaldi",
        ],
    ),
    ("Opera", "opera", &[".config/opera"]),
];

/// Firefox and the browsers made from it.
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

/// The profile of a Mozilla browser in `base` whose history was written last.
fn newest_profile(base: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for profile in std::fs::read_dir(base).ok()?.flatten() {
        let path = profile.path();
        let Some(at) = modified(&path.join("places.sqlite")) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| at > *t) {
            best = Some((at, path));
        }
    }
    best.map(|(_, p)| p)
}

/// The browsers of this computer that have something to bring.
pub(super) fn detect() -> Vec<Source> {
    let home = glib::home_dir();
    let mut found = Vec::new();
    for (name, keyring, dirs) in CHROMIUM {
        let profile = dirs
            .iter()
            .map(|d| home.join(d).join("Default"))
            .find(|p| p.join("Bookmarks").is_file() || p.join("History").is_file());
        if let Some(profile) = profile {
            found.push(Source {
                name,
                family: Family::Chromium,
                profile,
                keyring,
            });
        }
    }
    for (name, dirs) in MOZILLA {
        if let Some(profile) = dirs.iter().find_map(|d| newest_profile(&home.join(d))) {
            found.push(Source {
                name,
                family: Family::Mozilla,
                profile,
                keyring: "",
            });
        }
    }
    found
}

/// Firefox keeps a dated backup of the bookmarks in each profile.
fn newest_backup(profile: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for file in std::fs::read_dir(profile.join("bookmarkbackups"))
        .ok()?
        .flatten()
    {
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
    best.map(|(_, p)| p)
}

fn read_bookmarks(path: &Path) -> Option<Vec<Imported>> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    bookmark_import::parse(&std::fs::read(path).ok()?).ok()
}

/// The secret Chromium keeps in the desktop's keyring, if `secret-tool` is installed and the
/// keyring answers in time.
fn keyring_secret(application: &str) -> Option<Vec<u8>> {
    let mut child = Command::new("secret-tool")
        .args(["lookup", "application", application])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(KEYRING_SECONDS) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut out = Vec::new();
    use std::io::Read;
    child.stdout.take()?.read_to_end(&mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

/// Everything that was read, before any of it is used.
#[derive(Default)]
pub(super) struct Bundle {
    bookmarks: Vec<Imported>,
    history: Vec<HistoryEntry>,
    cookies: Decrypted<Cookie>,
    logins: Decrypted<Login>,
}

/// The first of `names` that exists inside `profile`.
fn first_file(profile: &Path, names: &[&str]) -> Option<PathBuf> {
    names.iter().map(|n| profile.join(n)).find(|p| p.is_file())
}

/// Reads what `wants` from `source`. Blocking.
fn read_source(source: &Source, wants: Wants) -> Bundle {
    let mut out = Bundle::default();
    match source.family {
        Family::Chromium => {
            if wants.bookmarks {
                out.bookmarks =
                    read_bookmarks(&source.profile.join("Bookmarks")).unwrap_or_default();
            }
            if wants.history {
                out.history = first_file(&source.profile, &["History"])
                    .and_then(|p| Snapshot::open(&p).ok())
                    .and_then(|s| browser_data::history_chromium(s.conn()).ok())
                    .unwrap_or_default();
            }
            if wants.cookies || wants.passwords {
                let secret = keyring_secret(source.keyring);
                let keys = Keys::with_keyring(secret.as_deref());
                if wants.cookies {
                    out.cookies = first_file(&source.profile, &["Network/Cookies", "Cookies"])
                        .and_then(|p| Snapshot::open(&p).ok())
                        .and_then(|s| browser_data::cookies_chromium(s.conn(), &keys).ok())
                        .unwrap_or_default();
                }
                if wants.passwords {
                    out.logins = first_file(&source.profile, &["Login Data"])
                        .and_then(|p| Snapshot::open(&p).ok())
                        .and_then(|s| browser_data::logins_chromium(s.conn(), &keys).ok())
                        .unwrap_or_default();
                }
            }
        }
        Family::Mozilla => {
            if wants.bookmarks {
                out.bookmarks = newest_backup(&source.profile)
                    .and_then(|p| read_bookmarks(&p))
                    .unwrap_or_default();
            }
            if wants.history {
                out.history = first_file(&source.profile, &["places.sqlite"])
                    .and_then(|p| Snapshot::open(&p).ok())
                    .and_then(|s| browser_data::history_firefox(s.conn()).ok())
                    .unwrap_or_default();
            }
            if wants.cookies {
                out.cookies = first_file(&source.profile, &["cookies.sqlite"])
                    .and_then(|p| Snapshot::open(&p).ok())
                    .and_then(|s| browser_data::cookies_firefox(s.conn()).ok())
                    .unwrap_or_default();
            }
            // Firefox keeps its passwords behind a key of its own (NSS): they come from the
            // CSV file it can export.
        }
    }
    out
}

fn same_site(policy: SameSite) -> soup::SameSitePolicy {
    match policy {
        SameSite::None => soup::SameSitePolicy::None,
        SameSite::Strict => soup::SameSitePolicy::Strict,
        SameSite::Lax | SameSite::Unset => soup::SameSitePolicy::Lax,
    }
}

impl Browser {
    /// Reads `wants` from the `n`-th browser found and brings it. `done` hears how much came.
    pub(super) fn import_from(
        self: &Rc<Self>,
        n: usize,
        wants: Wants,
        done: Box<dyn FnOnce(Option<Counts>)>,
    ) {
        if self.private {
            done(None);
            return;
        }
        let Some(source) = detect().into_iter().nth(n) else {
            done(None);
            return;
        };
        let this = self.clone();
        glib::spawn_future_local(async move {
            let bundle = gio::spawn_blocking(move || read_source(&source, wants))
                .await
                .ok();
            match bundle {
                Some(bundle) => this.bring(bundle, done),
                None => done(None),
            }
        });
    }

    /// Reads a file the person picked: a passwords file (CSV) or a bookmarks file.
    pub(super) fn import_file(
        self: &Rc<Self>,
        path: PathBuf,
        done: Box<dyn FnOnce(Option<Counts>)>,
    ) {
        if self.private {
            done(None);
            return;
        }
        let this = self.clone();
        glib::spawn_future_local(async move {
            let bundle = gio::spawn_blocking(move || read_file(&path))
                .await
                .ok()
                .flatten();
            match bundle {
                Some(bundle) => this.bring(bundle, done),
                None => done(None),
            }
        });
    }

    /// Puts what was read where it belongs, and counts what was new.
    fn bring(self: &Rc<Self>, bundle: Bundle, done: Box<dyn FnOnce(Option<Counts>)>) {
        let mut counts = Counts {
            unreadable: bundle.cookies.unreadable + bundle.logins.unreadable,
            ..Counts::default()
        };
        counts.bookmarks = self.add_bookmarks(&bundle.bookmarks);
        counts.history = self.add_history(bundle.history);
        counts.cookies = self.add_cookies(&bundle.cookies.items);
        if bundle.logins.items.is_empty() {
            done(Some(counts));
            return;
        }
        // The passwords go into the vault, which asks for its master password (or makes one).
        self.pw_import(
            bundle.logins.items,
            Box::new(move |added| {
                counts.logins = added;
                done(Some(counts));
            }),
        );
    }

    fn add_bookmarks(&self, items: &[Imported]) -> usize {
        if items.is_empty() {
            return 0;
        }
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
        added
    }

    fn add_history(&self, items: Vec<HistoryEntry>) -> usize {
        if items.is_empty() {
            return 0;
        }
        let added = self
            .history
            .borrow_mut()
            .import(items.into_iter().map(|h| Visit {
                url: h.url,
                title: h.title,
                visits: h.visits,
                last: h.last,
            }));
        self.save_history();
        added
    }

    /// Hands the cookies to the network session. Each one is added on its own; the count is how
    /// many were handed over.
    fn add_cookies(&self, items: &[Cookie]) -> usize {
        let Some(manager) = self.session.cookie_manager() else {
            return 0;
        };
        let now = now_ms() / 1000;
        let mut handed = 0;
        for c in items {
            let max_age = match c.expires {
                Some(at) => i32::try_from(at.saturating_sub(now)).unwrap_or(i32::MAX),
                None => -1,
            };
            let mut cookie = soup::Cookie::new(&c.name, &c.value, &c.host, &c.path, max_age);
            cookie.set_secure(c.secure);
            cookie.set_http_only(c.http_only);
            cookie.set_same_site_policy(same_site(c.same_site));
            manager.add_cookie(&cookie, gio::Cancellable::NONE, |_| {});
            handed += 1;
        }
        handed
    }

    /// What the preferences page offers: the names of the browsers found, and whether each one
    /// lets its passwords be read.
    pub(super) fn import_view(&self) -> Vec<(String, bool)> {
        detect()
            .into_iter()
            .map(|s| (s.name.to_owned(), s.family == Family::Chromium))
            .collect()
    }

    /// One of the `import.*` changes of the preferences page.
    pub(super) fn change_import(self: &Rc<Self>, what: &str, value: &str) {
        match what {
            "run" => {
                let parsed = value
                    .split_once(':')
                    .and_then(|(n, kind)| Some((n.parse::<usize>().ok()?, Wants::of(kind)?)));
                if let Some((n, wants)) = parsed {
                    let this = self.clone();
                    self.import_from(n, wants, Box::new(move |c| this.tell_import(c)));
                }
            }
            "file" => {
                let dialog = gtk::FileDialog::builder()
                    .title("Archivo de marcadores o de contraseñas (CSV)")
                    .modal(true)
                    .build();
                dialog.set_initial_folder(Some(&gio::File::for_path(glib::home_dir())));
                let this = self.clone();
                dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
                    if let Some(path) = result.ok().and_then(|f| f.path()) {
                        let that = this.clone();
                        this.import_file(path, Box::new(move |c| that.tell_import(c)));
                    }
                });
            }
            _ => {}
        }
    }

    /// Says how an import went, in a window, and shows the new numbers on the page.
    fn tell_import(self: &Rc<Self>, counts: Option<Counts>) {
        let say = match counts {
            Some(c) => Say::Imported(c),
            None => Say::ImportFailed,
        };
        dialogs::notice(&self.window, "Importar", &words::say(&system_lang(), &say));
        self.redraw_settings();
    }
}

/// A file the person picked: a passwords file (CSV) or a bookmarks file.
fn read_file(path: &Path) -> Option<Bundle> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mut out = Bundle::default();
    if let Ok(items) = bookmark_import::parse(&bytes) {
        out.bookmarks = items;
        return Some(out);
    }
    let logins = browser_data::logins_csv(&String::from_utf8_lossy(&bytes)).ok()?;
    out.logins = Decrypted {
        items: logins,
        unreadable: 0,
    };
    Some(out)
}
