//! Downloads the filter lists, compiles them off the main thread and hands WebKit the result.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use brujula_core::adblock::{self, Cosmetic, FilterList, PageCosmetic};
use brujula_core::scriptlets::Scriptlets;
use brujula_core::settings::Settings;
use nodalia_gtk::gtk::gio;
use nodalia_gtk::gtk::glib;
use soup::prelude::*;
use webkit6::{
    UserContentFilterStore, UserContentInjectedFrames, UserContentManager, UserScript,
    UserScriptInjectionTime,
};

use crate::paths;

type AppliedHandler = Box<dyn Fn(bool)>;

pub struct Filters {
    ucm: UserContentManager,
    store: UserContentFilterStore,
    cosmetic: RefCell<Option<Arc<Cosmetic>>>,
    rule_count: RefCell<usize>,
    /// Set while a download/compile is running so refreshes do not pile up.
    busy: RefCell<bool>,
    /// Called each time new rules are in place (pages already open can then be re-processed).
    on_applied: RefCell<Option<AppliedHandler>>,
    /// Sites where blocking is off; baked into the rules, so a change recompiles them.
    allow: RefCell<Vec<String>>,
    /// The lists in use, for recompiling without going to the network.
    lists: RefCell<Vec<FilterList>>,
    /// Bumped by every compile and by `disable`; a compile that finishes after another one
    /// started (or after blocking was turned off) throws its result away.
    generation: Cell<u64>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    format: String,
    chunks: usize,
    #[serde(default)]
    rules: usize,
    #[serde(default)]
    allow: Vec<String>,
}

fn manifest_path() -> std::path::PathBuf {
    paths::cache_dir().join("filters-manifest.json")
}

fn scriptlets_path() -> std::path::PathBuf {
    paths::cache_dir().join("filters-scriptlets.json")
}

// Small and local, read on the main loop at start: plain helpers keep the callers readable.
fn read_text(path: &std::path::Path) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

fn read_bytes(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The scriptlet rules saved with the engine, or none if the cache is missing or unreadable.
fn load_scriptlets() -> Scriptlets {
    std::fs::read_to_string(scriptlets_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Scriptlets>(&t).ok())
        .unwrap_or_default()
}

/// Saves the cosmetic engine and the scriptlet rules next to the manifest.
fn store_cache(engine_bytes: &[u8], scriptlets: &Scriptlets) -> std::io::Result<()> {
    std::fs::create_dir_all(paths::cache_dir())?;
    std::fs::write(engine_path(), engine_bytes)?;
    std::fs::write(
        scriptlets_path(),
        serde_json::to_string(scriptlets).unwrap_or_default(),
    )
}

/// uBlock Origin's scriptlets as one script (see `scripts/bundle-ubo-scriptlets.py`).
const UBO_SCRIPTLETS: &str = include_str!("../resources/ubo-scriptlets.js.txt");

fn engine_path() -> std::path::PathBuf {
    paths::cache_dir().join("filters-engine.bin")
}

impl Filters {
    pub fn new(ucm: UserContentManager) -> Rc<Self> {
        let store = UserContentFilterStore::new(
            &paths::cache_dir().join("content-filters").to_string_lossy(),
        );
        Rc::new(Self {
            ucm,
            store,
            cosmetic: RefCell::new(None),
            rule_count: RefCell::new(0),
            busy: RefCell::new(false),
            on_applied: RefCell::new(None),
            allow: RefCell::new(Vec::new()),
            lists: RefCell::new(Vec::new()),
            generation: Cell::new(0),
        })
    }

    /// `f(first)` runs after rules are applied; `first` is true when there was nothing
    /// applied before (so pages loaded until now were not filtered).
    pub fn set_on_applied(&self, f: impl Fn(bool) + 'static) {
        *self.on_applied.borrow_mut() = Some(Box::new(f));
    }

    /// Sets the sites where blocking is off. Call before [`load_compiled`](Self::load_compiled)
    /// and [`start`](Self::start); to change it later use [`rebuild`](Self::rebuild).
    pub fn set_allow(&self, allow: Vec<String>) {
        *self.allow.borrow_mut() = allow;
    }

    /// Compiles again from the lists on disk with the current allow list.
    pub fn rebuild(self: &Rc<Self>) {
        let lists = self.lists.borrow().clone();
        if lists.is_empty() {
            return;
        }
        let this = self.clone();
        glib::spawn_future_local(async move {
            let cached = read_cached(&lists).await;
            if !cached.is_empty() {
                this.apply(cached).await;
            }
        });
    }

    /// Loads the rules compiled in a previous run. Takes milliseconds, so the browser can do it
    /// before loading any page. Returns whether anything was loaded.
    pub async fn load_compiled(self: &Rc<Self>) -> bool {
        let Ok(text) = read_text(&manifest_path()) else {
            return false;
        };
        let Ok(manifest) = serde_json::from_str::<Manifest>(&text) else {
            return false;
        };
        if manifest.format != adblock::CACHE_FORMAT || manifest.allow != *self.allow.borrow() {
            return false;
        }
        let Ok(engine_bytes) = read_bytes(&engine_path()) else {
            return false;
        };
        let cosmetic = gio::spawn_blocking(move || Cosmetic::deserialize(&engine_bytes)).await;
        let Ok(Some(cosmetic)) = cosmetic else {
            return false;
        };
        let cosmetic = cosmetic.with_allow(self.allow.borrow().clone());

        let mut filters = Vec::new();
        for i in 0..manifest.chunks {
            match self.store.load_future(&format!("brujula-{i}")).await {
                Ok(f) => filters.push(f),
                Err(_) => return false,
            }
        }
        self.ucm.remove_all_filters();
        for f in &filters {
            self.ucm.add_filter(f);
        }
        *self.cosmetic.borrow_mut() = Some(Arc::new(cosmetic));
        *self.rule_count.borrow_mut() = manifest.rules;
        let scriptlets = load_scriptlets();
        self.install_scriptlets(&scriptlets);
        true
    }

    pub fn rule_count(&self) -> usize {
        *self.rule_count.borrow()
    }

    pub fn cosmetic_for_page(&self, url: &str) -> Option<(Arc<Cosmetic>, PageCosmetic)> {
        let c = self.cosmetic.borrow().clone()?;
        let page = c.for_page(url);
        Some((c, page))
    }

    /// Puts the scriptlets into pages of the sites that have rules for them. They run before
    /// the page's own scripts, which is the only moment they can change what those read.
    fn install_scriptlets(&self, scriptlets: &Scriptlets) {
        self.ucm.remove_all_scripts();
        if scriptlets.is_empty() {
            return;
        }
        let patterns = scriptlets.url_patterns();
        let patterns: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let source = scriptlets.user_script(UBO_SCRIPTLETS, &self.allow.borrow());
        self.ucm.add_script(&UserScript::new(
            &source,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &patterns,
            &[],
        ));
    }

    /// Stops all blocking (network rules removed from WebKit, cosmetic dropped).
    pub fn disable(&self) {
        self.generation.set(self.generation.get() + 1);
        self.ucm.remove_all_filters();
        self.ucm.remove_all_scripts();
        *self.cosmetic.borrow_mut() = None;
        *self.rule_count.borrow_mut() = 0;
    }

    /// Loads what is cached right away, then refreshes stale lists in the background and
    /// reloads the result. Safe to call again; it does nothing while one run is in progress.
    pub fn start(self: &Rc<Self>, settings: &Settings) {
        if *self.busy.borrow() {
            return;
        }
        *self.busy.borrow_mut() = true;
        let lists: Vec<FilterList> = adblock::DEFAULT_LISTS
            .iter()
            .copied()
            .filter(|l| settings.list_enabled(l.id, l.enabled_by_default))
            .collect();
        *self.lists.borrow_mut() = lists.clone();
        let this = self.clone();
        glib::spawn_future_local(async move {
            this.run(lists).await;
            *this.busy.borrow_mut() = false;
        });
    }

    async fn run(self: &Rc<Self>, lists: Vec<FilterList>) {
        // 1. Rules compiled last time, already in place if the browser loaded them. When there
        // are none, compile whatever lists are on disk so blocking starts at once.
        if self.cosmetic.borrow().is_none() {
            let cached = read_cached(&lists).await;
            if !cached.is_empty() {
                self.apply(cached).await;
            }
        }

        // 2. Refresh what is missing or old.
        let metered = gio::NetworkMonitor::default().is_network_metered();
        let session = soup::Session::new();
        session.set_timeout(30);
        let mut changed = false;
        for list in &lists {
            let path = paths::lists_dir().join(format!("{}.txt", list.id));
            let stamp = modified(&path);
            if !adblock::needs_refresh(stamp, SystemTime::now(), metered) {
                continue;
            }
            match download(&session, list.url).await {
                Ok(text) => {
                    if paths::write_atomic(&path, &text).is_ok() {
                        changed = true;
                    }
                }
                Err(e) => eprintln!("brujula: could not update {}: {e}", list.name),
            }
        }
        if changed {
            let fresh = read_cached(&lists).await;
            self.apply(fresh).await;
        }
    }

    async fn apply(self: &Rc<Self>, texts: Vec<String>) {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let allow = self.allow.borrow().clone();
        let allow_for_manifest = allow.clone();
        let compiled = gio::spawn_blocking(move || {
            let c = adblock::compile(texts.iter().map(String::as_str), &allow);
            let engine = c.cosmetic.serialize();
            (c, engine)
        })
        .await;
        let Ok((compiled, engine_bytes)) = compiled else {
            eprintln!("brujula: compiling filters failed");
            return;
        };
        if self.generation.get() != generation {
            return;
        }
        let first = self.cosmetic.borrow().is_none();

        let mut filters = Vec::new();
        for (i, json) in compiled.content_filters.iter().enumerate() {
            let bytes = glib::Bytes::from_owned(json.clone().into_bytes());
            match self
                .store
                .save_future(&format!("brujula-{i}"), &bytes)
                .await
            {
                Ok(f) => filters.push(f),
                Err(e) => eprintln!("brujula: WebKit rejected filter chunk {i}: {e}"),
            }
        }
        if filters.len() != compiled.content_filters.len() || self.generation.get() != generation {
            // Do not half-apply, and do not undo a newer compile or a switch-off.
            return;
        }
        self.ucm.remove_all_filters();
        for f in &filters {
            self.ucm.add_filter(f);
        }
        self.install_scriptlets(&compiled.scriptlets);
        *self.cosmetic.borrow_mut() = Some(Arc::new(compiled.cosmetic));
        *self.rule_count.borrow_mut() = compiled.rule_count;
        eprintln!(
            "brujula: ad blocker ready, {} network rules",
            compiled.rule_count
        );

        // Remember it for the next start.
        let manifest = Manifest {
            format: adblock::CACHE_FORMAT.to_owned(),
            chunks: filters.len(),
            rules: compiled.rule_count,
            allow: allow_for_manifest,
        };
        if store_cache(&engine_bytes, &compiled.scriptlets).is_ok() {
            let _ = paths::write_atomic(
                &manifest_path(),
                &serde_json::to_string(&manifest).unwrap_or_default(),
            );
        }
        if let Some(f) = self.on_applied.borrow().as_ref() {
            f(first);
        }
    }
}

async fn read_cached(lists: &[FilterList]) -> Vec<String> {
    let ids: Vec<&'static str> = lists.iter().map(|l| l.id).collect();
    gio::spawn_blocking(move || {
        ids.iter()
            .filter_map(|id| read_text(&paths::lists_dir().join(format!("{id}.txt"))).ok())
            .collect()
    })
    .await
    .unwrap_or_default()
}

async fn download(session: &soup::Session, url: &str) -> Result<String, String> {
    let msg = soup::Message::new("GET", url).map_err(|e| e.to_string())?;
    let bytes = session
        .send_and_read_future(&msg, glib::Priority::LOW)
        .await
        .map_err(|e| e.to_string())?;
    if msg.status() != soup::Status::Ok {
        return Err(format!("HTTP {:?}", msg.status()));
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // A captive portal or an error page is not a filter list.
    if text.len() < 200
        || text.trim_start().starts_with("<!")
        || text.trim_start().starts_with("<html")
    {
        return Err("not a filter list".to_owned());
    }
    Ok(text)
}
