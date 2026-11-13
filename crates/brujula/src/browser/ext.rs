//! Chromium extensions in the window: loading them, the `brujula-ext://` pages, their scripts in
//! the pages, the background page, the popup and the button that opens it.
//!
//! The two kinds of context are kept apart on purpose. The scripts an extension puts in a web
//! page run in a world of their own (`brujula-ext-<id>`) of the tabs' content manager, and only
//! that world can talk to the extension's message handler, so a web page has no way to reach it.
//! The extension's own pages (background, popup, options) use another content manager and never
//! show web content: they are not allowed to go anywhere but their own files.

use super::*;

use std::collections::HashSet;
use std::path::PathBuf;

use brujula_core::ext::manifest::{Manifest, RunAt};
use brujula_core::ext::messages::Messages;
use brujula_core::ext::registry::{Installed, Registry};
use brujula_core::ext::{self, dnr};
use serde_json::{json, Map, Value};
use webkit6::{
    UserContentFilter, UserContentInjectedFrames, UserScript, UserScriptInjectionTime,
    UserStyleLevel, UserStyleSheet, WebContext,
};

/// The answer to a call from an extension, given when it is ready.
pub(super) type Done = Box<dyn FnOnce(Result<Value, String>)>;

const SHIM: &str = include_str!("../../../brujula-core/src/ext/shim.js");

/// An extension that is installed and understood.
pub(super) struct Loaded {
    pub manifest: Manifest,
    pub messages: Messages,
    /// The manifest as it was written, for `chrome.runtime.getManifest()`.
    pub raw: Value,
    pub dir: PathBuf,
}

/// The two sides of a port: where it was opened and where it was received.
pub(super) struct PortPair {
    pub opener: (WebView, Option<String>),
    pub receivers: Vec<(WebView, Option<String>)>,
}

/// What the extension set on its toolbar button.
#[derive(Default, Clone)]
pub(super) struct ActionState {
    pub badge: String,
    pub color: String,
    pub title: Option<String>,
    pub popup: Option<String>,
}

#[derive(Default)]
pub struct ExtState {
    pub(super) registry: Registry,
    pub(super) loaded: HashMap<String, Loaded>,
    handlers: HashSet<String>,
    /// The scripts and styles put in web pages, and in the extension's own pages.
    scripts: HashMap<String, Vec<UserScript>>,
    page_scripts: HashMap<String, Vec<UserScript>>,
    /// The content manager of each extension's own pages: one each, so a page of one extension
    /// cannot reach the message handler of another.
    page_ucms: HashMap<String, UserContentManager>,
    sheets: HashMap<String, Vec<UserStyleSheet>>,
    filters: HashMap<String, Vec<UserContentFilter>>,
    pub(super) background: HashMap<String, WebView>,
    pub(super) popups: HashMap<String, WebView>,
    /// Open ports: who opened each and who answers.
    pub(super) ports: HashMap<(String, String), PortPair>,
    /// Pages of an extension open in windows (options, welcome pages).
    pub(super) pages: HashMap<String, Vec<(WebView, gtk::Window)>>,
    /// Addresses the browser itself asked an extension view to load.
    trusted: HashSet<String>,
    pub(super) storage: HashMap<(String, String), Map<String, Value>>,
    pub(super) pending: HashMap<u64, Done>,
    pub(super) next_req: u64,
    pub(super) action: HashMap<String, ActionState>,
    /// The background pages that have loaded, and what waits for the others.
    pub(super) bg_ready: HashSet<String>,
    pub(super) bg_wait: HashMap<String, Vec<Box<dyn FnOnce()>>>,
    /// Extensions installed in this run, whose background page gets `onInstalled`.
    fresh: HashSet<String>,
    pub(super) http: Option<soup::Session>,
}

pub(super) fn handler_name(id: &str) -> String {
    format!("brujulaExt_{id}")
}

pub(super) fn world_name(id: &str) -> String {
    format!("brujula-ext-{id}")
}

pub(super) fn pkg_dir(id: &str) -> PathBuf {
    paths::extensions_dir().join(id).join("pkg")
}

fn languages() -> Vec<String> {
    glib::language_names()
        .iter()
        .map(|l| l.to_string())
        .collect()
}

/// The type a file is served as.
fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

fn web_patterns(patterns: &[brujula_core::ext::pattern::Pattern]) -> Vec<String> {
    patterns.iter().flat_map(|p| p.webkit()).collect()
}

impl Browser {
    /// Registers the extension pages' scheme and starts the extensions that are on.
    pub(super) fn start_extensions(self: &Rc<Self>) {
        if let Some(context) = WebContext::default() {
            let this = Rc::downgrade(self);
            context.register_uri_scheme("brujula-ext", move |req| match this.upgrade() {
                Some(b) => b.serve_ext(req),
                None => req.finish_error(&mut glib::Error::new(
                    gio::IOErrorEnum::Failed,
                    "the browser is closing",
                )),
            });
        }
        let registry = std::fs::read_to_string(paths::extensions_file())
            .map(|t| Registry::from_json(&t))
            .unwrap_or_default();
        let on: Vec<String> = registry
            .items()
            .iter()
            .filter(|i| i.enabled)
            .map(|i| i.id.clone())
            .collect();
        self.ext.borrow_mut().registry = registry;
        for id in on {
            if let Err(why) = self.activate(&id) {
                eprintln!("brujula: extension {id} not started: {why}");
            }
        }
        self.sync_ext_buttons();
    }

    pub(super) fn save_extensions(&self) {
        if self.private {
            return;
        }
        let json = self.ext.borrow().registry.to_json();
        if let Err(e) = paths::write_atomic(&paths::extensions_file(), &json) {
            eprintln!("brujula: could not save the extensions: {e}");
        }
    }

    /// Shows the toolbar button only when there is an extension to open.
    pub(super) fn sync_ext_buttons(&self) {
        let any = self.ext.borrow().registry.items().iter().any(|i| i.enabled);
        for c in &self.chromes {
            c.ext.set_visible(any);
        }
    }

    // ---- the pages of an extension, at brujula-ext://<id>/<path>

    fn serve_ext(self: &Rc<Self>, req: &webkit6::URISchemeRequest) {
        let fail = |code: gio::IOErrorEnum, why: &str| {
            req.finish_error(&mut glib::Error::new(code, why));
        };
        let uri = req.uri().map(|u| u.to_string()).unwrap_or_default();
        let Ok(url) = url::Url::parse(&uri) else {
            return fail(gio::IOErrorEnum::InvalidArgument, "bad address");
        };
        let id = url.host_str().unwrap_or("").to_owned();
        let path = percent_decode(url.path())
            .trim_start_matches('/')
            .to_owned();
        let (manifest_ok, web_ok, background) = {
            let s = self.ext.borrow();
            let Some(l) = s.loaded.get(&id) else {
                return fail(gio::IOErrorEnum::NotFound, "no such extension");
            };
            (
                brujula_core::ext::manifest::safe_path(&path).is_some()
                    || path == "__background.html",
                l.manifest.is_web_accessible(&path),
                l.manifest.background.clone(),
            )
        };
        if !manifest_ok {
            return fail(gio::IOErrorEnum::PermissionDenied, "bad path");
        }
        // The extension's own pages may read everything; a web page, only what the manifest
        // says is web accessible.
        let from_view = req
            .web_view()
            .and_then(|v| v.uri())
            .is_some_and(|u| u.starts_with(&format!("brujula-ext://{id}/")));
        let trusted = self.ext.borrow().trusted.contains(&uri);
        if !(from_view || trusted || web_ok) {
            return fail(gio::IOErrorEnum::PermissionDenied, "not web accessible");
        }
        let (bytes, kind) = if path == "__background.html" {
            let Some(bg) = background else {
                return fail(gio::IOErrorEnum::NotFound, "no background page");
            };
            (
                background_page(&bg).into_bytes(),
                "text/html; charset=utf-8",
            )
        } else {
            let dir = pkg_dir(&id);
            match std::fs::read(dir.join(&path)) {
                Ok(b) => (b, content_type(&path)),
                Err(_) => return fail(gio::IOErrorEnum::NotFound, "no such file"),
            }
        };
        let len = i64::try_from(bytes.len()).unwrap_or(-1);
        let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
        req.finish(&stream, len, Some(kind));
    }

    // ---- turning an extension on and off

    /// Reads the extension from disk and starts everything it brings.
    pub(super) fn activate(self: &Rc<Self>, id: &str) -> Result<(), String> {
        let dir = pkg_dir(id);
        let (manifest, messages) = ext::load_dir(&dir, &languages())?;
        let raw = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
            .unwrap_or(Value::Null);
        self.ext.borrow_mut().loaded.insert(
            id.to_owned(),
            Loaded {
                manifest,
                messages,
                raw,
                dir,
            },
        );
        self.register_handlers(id);
        self.install_scripts(id);
        self.load_rules(id);
        self.start_background(id);
        Ok(())
    }

    /// Stops an extension: its scripts, rules, background page and windows go away.
    pub(super) fn deactivate(&self, id: &str) {
        let (scripts, pages, sheets, filters, background, popup, windows) = {
            let mut s = self.ext.borrow_mut();
            s.loaded.remove(id);
            s.action.remove(id);
            s.bg_ready.remove(id);
            s.bg_wait.remove(id);
            s.ports.retain(|(e, _), _| e != id);
            (
                s.scripts.remove(id),
                s.page_scripts.remove(id),
                s.sheets.remove(id),
                s.filters.remove(id),
                s.background.remove(id),
                s.popups.remove(id),
                s.pages.remove(id),
            )
        };
        for sc in scripts.into_iter().flatten() {
            self.ucm.remove_script(&sc);
        }
        drop(pages);
        self.ext.borrow_mut().page_ucms.remove(id);
        for st in sheets.into_iter().flatten() {
            self.ucm.remove_style_sheet(&st);
        }
        if filters.is_some() {
            self.refresh_ext_rules();
        }
        if let Some(v) = background {
            v.terminate_web_process();
        }
        if let Some(v) = popup {
            v.terminate_web_process();
        }
        for (v, w) in windows.into_iter().flatten() {
            v.terminate_web_process();
            w.close();
        }
    }

    /// The content manager for the pages of `id`.
    fn page_ucm(&self, id: &str) -> UserContentManager {
        self.ext
            .borrow_mut()
            .page_ucms
            .entry(id.to_owned())
            .or_default()
            .clone()
    }

    fn register_handlers(self: &Rc<Self>, id: &str) {
        let name = handler_name(id);
        // The tabs' manager keeps its handler for good (it only answers in the extension's
        // world); the pages' manager is new each time the extension starts.
        if self.ext.borrow_mut().handlers.insert(name.clone()) {
            self.ucm
                .register_script_message_handler_with_reply(&name, Some(&world_name(id)));
            self.connect_handler(&self.ucm.clone(), id, &name);
        }
        let page_ucm = self.page_ucm(id);
        page_ucm.register_script_message_handler_with_reply(&name, None);
        self.connect_handler(&page_ucm, id, &name);
    }

    fn connect_handler(self: &Rc<Self>, ucm: &UserContentManager, id: &str, name: &str) {
        let this = Rc::downgrade(self);
        let id = id.to_owned();
        ucm.connect_script_message_with_reply_received(Some(name), move |_, value, reply| {
            let Some(b) = this.upgrade() else {
                reply.return_error_message("browser closed");
                return true;
            };
            let Some(ctx) = value.context() else {
                reply.return_error_message("no context");
                return true;
            };
            let text = value.to_str().to_string();
            let reply = reply.clone();
            b.ext_call(
                &id,
                &text,
                Box::new(move |res| {
                    let out = match res {
                        Ok(v) => json!({"ok": true, "value": v}),
                        Err(e) => json!({"ok": false, "error": e}),
                    };
                    let v =
                        webkit6::javascriptcore::Value::new_string(&ctx, Some(&out.to_string()));
                    reply.return_value(&v);
                }),
            );
            true
        });
    }

    /// The text that sets `__brujulaCtx` for a context of `id`.
    pub(super) fn ext_prelude(&self, id: &str, kind: &str) -> Option<String> {
        let s = self.ext.borrow();
        let l = s.loaded.get(id)?;
        let messages: Value = serde_json::from_str(&l.messages.to_js_json()).unwrap_or(Value::Null);
        let lang = languages()
            .first()
            .map(|l| l.split(['_', '.', '@']).next().unwrap_or("en").to_owned())
            .unwrap_or_else(|| "en".to_owned());
        let ctx = json!({
            "id": id,
            "handler": handler_name(id),
            "kind": kind,
            "manifest": l.raw,
            "messages": messages,
            "base": format!("brujula-ext://{id}/"),
            "lang": lang,
            "popup": s.action.get(id).and_then(|a| a.popup.clone())
                .or_else(|| l.manifest.action.as_ref().and_then(|a| a.popup.clone())),
            "fresh": s.fresh.contains(id),
        });
        Some(format!("globalThis.__brujulaCtx={ctx};\n{SHIM}\n"))
    }

    fn install_scripts(&self, id: &str) {
        let Some(page_shim) = self.ext_prelude(id, "extpage") else {
            return;
        };
        let content_shim = self.ext_prelude(id, "content").unwrap_or_default();
        let world = world_name(id);
        let mut scripts = Vec::new();
        let mut sheets = Vec::new();
        let (content, dir) = {
            let s = self.ext.borrow();
            let Some(l) = s.loaded.get(id) else { return };
            (l.manifest.content_scripts.clone(), l.dir.clone())
        };
        // The extension's own pages get the API in the default world.
        let page_allow = format!("brujula-ext://{id}/*");
        let page = UserScript::new(
            &page_shim,
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &[page_allow.as_str()],
            &[],
        );
        self.page_ucm(id).add_script(&page);
        self.ext
            .borrow_mut()
            .page_scripts
            .insert(id.to_owned(), vec![page]);

        // The same API in the pages the content scripts work on.
        let everywhere: Vec<String> = content
            .iter()
            .flat_map(|c| web_patterns(&c.matches))
            .collect();
        let has_api_scripts = content.iter().any(|c| !c.main_world);
        if has_api_scripts
            || self.ext.borrow().loaded.get(id).is_some_and(|l| {
                l.manifest.has_permission("scripting") || l.manifest.has_permission("activeTab")
            })
        {
            let allow: Vec<&str> = everywhere.iter().map(String::as_str).collect();
            if !allow.is_empty() {
                scripts.push(UserScript::for_world(
                    &content_shim,
                    UserContentInjectedFrames::AllFrames,
                    UserScriptInjectionTime::Start,
                    &world,
                    &allow,
                    &[],
                ));
            }
        }
        for c in &content {
            let allow_owned = web_patterns(&c.matches);
            let block_owned = web_patterns(&c.excludes);
            let allow: Vec<&str> = allow_owned.iter().map(String::as_str).collect();
            let block: Vec<&str> = block_owned.iter().map(String::as_str).collect();
            let frames = if c.all_frames {
                UserContentInjectedFrames::AllFrames
            } else {
                UserContentInjectedFrames::TopFrame
            };
            let time = match c.run_at {
                RunAt::Start => UserScriptInjectionTime::Start,
                RunAt::End => UserScriptInjectionTime::End,
            };
            let js: Vec<String> =
                c.js.iter()
                    .filter_map(|p| std::fs::read_to_string(dir.join(p)).ok())
                    .collect();
            if !js.is_empty() {
                let source = js.join("\n;\n");
                scripts.push(if c.main_world {
                    UserScript::new(&source, frames, time, &allow, &block)
                } else {
                    UserScript::for_world(&source, frames, time, &world, &allow, &block)
                });
            }
            // User-level sheets lose to the page's own rules; an extension's are meant to win.
            for sheet in c
                .css
                .iter()
                .filter_map(|p| std::fs::read_to_string(dir.join(p)).ok())
            {
                sheets.push(UserStyleSheet::new(
                    &ext::css::important(&sheet),
                    frames,
                    UserStyleLevel::User,
                    &allow,
                    &block,
                ));
            }
        }
        for sc in &scripts {
            self.ucm.add_script(sc);
        }
        for st in &sheets {
            self.ucm.add_style_sheet(st);
        }
        let mut s = self.ext.borrow_mut();
        s.scripts.insert(id.to_owned(), scripts);
        s.sheets.insert(id.to_owned(), sheets);
    }

    // ---- network rules (declarativeNetRequest), as WebKit content blockers

    fn load_rules(self: &Rc<Self>, id: &str) {
        let sets: Vec<(String, PathBuf)> = {
            let s = self.ext.borrow();
            let Some(l) = s.loaded.get(id) else { return };
            l.manifest
                .rulesets
                .iter()
                .filter(|r| r.enabled)
                .map(|r| (r.id.clone(), l.dir.join(&r.path)))
                .collect()
        };
        if sets.is_empty() {
            return;
        }
        let this = self.clone();
        let id = id.to_owned();
        glib::spawn_future_local(async move {
            let mut made = Vec::new();
            for (name, path) in sets {
                let text = gio::spawn_blocking(move || std::fs::read_to_string(path))
                    .await
                    .ok()
                    .and_then(Result::ok);
                let Some(text) = text else { continue };
                let converted = match dnr::convert(&text) {
                    Ok(c) if c.converted > 0 => c,
                    _ => continue,
                };
                let bytes = glib::Bytes::from_owned(converted.json.into_bytes());
                let ident = format!("ext-{id}-{name}");
                match this.filters.store().save_future(&ident, &bytes).await {
                    Ok(f) => made.push(f),
                    Err(e) => eprintln!("brujula: WebKit rejected the rules of {id}/{name}: {e}"),
                }
            }
            if made.is_empty() || !this.ext.borrow().loaded.contains_key(&id) {
                return;
            }
            this.ext.borrow_mut().filters.insert(id, made);
            this.refresh_ext_rules();
        });
    }

    /// Hands the blocker every extension's rules.
    fn refresh_ext_rules(&self) {
        let all: Vec<UserContentFilter> = self
            .ext
            .borrow()
            .filters
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();
        self.filters.set_extra(all);
        self.filters.refresh_extra();
    }

    // ---- the background page

    fn start_background(self: &Rc<Self>, id: &str) {
        let has = self
            .ext
            .borrow()
            .loaded
            .get(id)
            .is_some_and(|l| l.manifest.background.is_some());
        if !has {
            return;
        }
        let url = format!("brujula-ext://{id}/__background.html");
        let view = self.ext_view(id);
        self.ext.borrow_mut().trusted.insert(url.clone());
        {
            let mut s = self.ext.borrow_mut();
            s.bg_ready.remove(id);
            s.bg_wait.remove(id);
        }
        let this = Rc::downgrade(self);
        let eid = id.to_owned();
        view.connect_load_changed(move |_, ev| {
            if ev != LoadEvent::Finished {
                return;
            }
            let Some(b) = this.upgrade() else { return };
            let waiting = {
                let mut s = b.ext.borrow_mut();
                s.bg_ready.insert(eid.clone());
                s.bg_wait.remove(&eid).unwrap_or_default()
            };
            for f in waiting {
                f();
            }
        });
        view.load_uri(&url);
        self.ext.borrow_mut().background.insert(id.to_owned(), view);
    }

    /// A view for pages of the extension: its own content manager, no way out to the web.
    pub(super) fn ext_view(self: &Rc<Self>, id: &str) -> WebView {
        let view = WebView::builder()
            .network_session(&self.session)
            .user_content_manager(&self.page_ucm(id))
            .build();
        harden(&view);
        let this = Rc::downgrade(self);
        let prefix = format!("brujula-ext://{id}/");
        view.connect_decide_policy(move |_, decision, kind| {
            if !matches!(
                kind,
                PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction
            ) {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else {
                return false;
            };
            let Some(action) = nav.navigation_action() else {
                return false;
            };
            let uri = action
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            if uri.starts_with(&prefix) || uri == "about:blank" {
                return false;
            }
            decision.ignore();
            // A link the user followed opens as a tab; the page itself stays where it is.
            if uri.starts_with("https://") || uri.starts_with("http://") {
                if let Some(b) = this.upgrade() {
                    if action.is_user_gesture() {
                        b.open_tab(&uri, None, true);
                    }
                }
            }
            true
        });
        let this = Rc::downgrade(self);
        view.connect_create(move |_, action| {
            let uri = action
                .clone()
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            if let Some(b) = this.upgrade() {
                if uri.starts_with("https://") || uri.starts_with("http://") {
                    b.open_tab(&uri, None, true);
                }
            }
            None
        });
        view
    }

    /// Opens a page of the extension in a window of its own.
    pub(super) fn open_ext_page(self: &Rc<Self>, id: &str, path: &str) {
        let Some(path) = brujula_core::ext::manifest::safe_path(path) else {
            return;
        };
        let title = self
            .ext
            .borrow()
            .loaded
            .get(id)
            .map(|l| l.manifest.name.clone())
            .unwrap_or_default();
        let url = format!("brujula-ext://{id}/{path}");
        // One window for each page: asking again brings it forward.
        let open = self
            .ext
            .borrow()
            .pages
            .get(id)
            .and_then(|v| {
                v.iter()
                    .find(|(view, _)| view.uri().as_deref() == Some(url.as_str()))
            })
            .map(|(_, w)| w.clone());
        if let Some(w) = open {
            w.present();
            return;
        }
        let view = self.ext_view(id);
        self.ext.borrow_mut().trusted.insert(url.clone());
        view.load_uri(&url);
        let window = gtk::Window::builder()
            .transient_for(&self.window)
            .default_width(900)
            .default_height(700)
            .title(title)
            .child(&view)
            .build();
        let w = window.clone();
        view.connect_title_notify(move |v| {
            if let Some(t) = v.title().filter(|t| !t.is_empty()) {
                w.set_title(Some(&t));
            }
        });
        self.ext
            .borrow_mut()
            .pages
            .entry(id.to_owned())
            .or_default()
            .push((view.clone(), window.clone()));
        let this = Rc::downgrade(self);
        let closing = view.clone();
        let eid = id.to_owned();
        window.connect_close_request(move |_| {
            if let Some(b) = this.upgrade() {
                if let Some(v) = b.ext.borrow_mut().pages.get_mut(&eid) {
                    v.retain(|(view, _)| view != &closing);
                }
            }
            glib::Propagation::Proceed
        });
        window.present();
    }

    // ---- the button in the toolbar

    /// Fills the popover of the extensions button: a list, and a popup when one is opened.
    pub(super) fn fill_ext_popover(self: &Rc<Self>, pop: &gtk::Popover) {
        let stack = gtk::Stack::new();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_margin_top(6);
        list.set_margin_bottom(6);
        list.set_margin_start(6);
        list.set_margin_end(6);
        list.set_size_request(260, -1);
        let mut items: Vec<(String, String)> = self
            .ext
            .borrow()
            .loaded
            .iter()
            .map(|(id, l)| (l.manifest.name.clone(), id.clone()))
            .collect();
        items.sort();
        for (name, id) in items {
            let (icon, popup, badge, has_action) = {
                let s = self.ext.borrow();
                let Some(l) = s.loaded.get(&id) else { continue };
                let st = s.action.get(&id).cloned().unwrap_or_default();
                (
                    l.manifest.icon_for(32).map(|p| l.dir.join(p)),
                    st.popup
                        .or_else(|| l.manifest.action.as_ref().and_then(|a| a.popup.clone())),
                    st.badge,
                    l.manifest.action.is_some(),
                )
            };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let image = match icon.filter(|p| p.exists()) {
                Some(p) => gtk::Image::from_file(p),
                None => gtk::Image::from_icon_name("ndl-blocks-symbolic"),
            };
            image.set_pixel_size(20);
            row.append(&image);
            let label = gtk::Label::new(Some(&name));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&label);
            if !badge.is_empty() {
                let b = gtk::Label::new(Some(&badge));
                b.add_css_class("dim-label");
                row.append(&b);
            }
            let button = gtk::Button::builder()
                .child(&row)
                .css_classes(["flat"])
                .sensitive(has_action || popup.is_some())
                .build();
            let this = Rc::downgrade(self);
            let (stack, pop) = (stack.clone(), pop.clone());
            button.connect_clicked(move |_| {
                let Some(b) = this.upgrade() else { return };
                match &popup {
                    Some(path) => b.show_ext_popup(&id, path, &stack),
                    None => {
                        b.ext_action_clicked(&id);
                        pop.popdown();
                    }
                }
            });
            list.append(&button);
        }
        let manage = gtk::Button::builder()
            .label("Administrar extensiones")
            .css_classes(["flat"])
            .build();
        let this = Rc::downgrade(self);
        let p = pop.clone();
        manage.connect_clicked(move |_| {
            p.popdown();
            if let Some(b) = this.upgrade() {
                b.open_settings(Some("extensions"));
            }
        });
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        list.append(&manage);
        stack.add_named(&list, Some("list"));
        pop.set_child(Some(&stack));
        // The popup goes when the popover does.
        let this = Rc::downgrade(self);
        pop.connect_closed(move |_| {
            if let Some(b) = this.upgrade() {
                let popups: Vec<WebView> =
                    b.ext.borrow_mut().popups.drain().map(|(_, v)| v).collect();
                for v in popups {
                    v.terminate_web_process();
                }
            }
        });
    }

    fn show_ext_popup(self: &Rc<Self>, id: &str, path: &str, stack: &gtk::Stack) {
        let url = format!("brujula-ext://{id}/{path}");
        let view = self.ext_view(id);
        view.set_size_request(320, 200);
        self.ext.borrow_mut().trusted.insert(url.clone());
        view.load_uri(&url);
        let old = self
            .ext
            .borrow_mut()
            .popups
            .insert(id.to_owned(), view.clone());
        if let Some(old) = old {
            old.terminate_web_process();
        }
        if let Some(prev) = stack.child_by_name("popup") {
            stack.remove(&prev);
        }
        stack.add_named(&view, Some("popup"));
        stack.set_visible_child_name("popup");
    }

    /// The extension had no popup: its page script is told the button was pressed.
    fn ext_action_clicked(self: &Rc<Self>, id: &str) {
        let tab = self
            .model
            .borrow()
            .selected()
            .and_then(|t| self.tab_json(id, t));
        self.emit_to_pages(id, "action.onClicked", &json!([tab]));
    }

    // ---- what the preferences page shows

    pub(super) fn ext_view_list(&self) -> Vec<brujula_core::prefs::ExtItem> {
        let s = self.ext.borrow();
        s.registry
            .items()
            .iter()
            .map(|i| {
                let l = s.loaded.get(&i.id);
                brujula_core::prefs::ExtItem {
                    id: i.id.clone(),
                    name: i.name.clone(),
                    version: i.version.clone(),
                    description: l
                        .map(|l| l.manifest.description.clone())
                        .unwrap_or_default(),
                    enabled: i.enabled,
                    options: l.is_some_and(|l| l.manifest.options_page.is_some()),
                    permissions: l
                        .map(|l| ext::permission_summary(&l.manifest))
                        .unwrap_or_default(),
                    problem: i.enabled && l.is_none(),
                }
            })
            .collect()
    }

    /// Marks an extension as freshly installed, so its background page gets `onInstalled`.
    pub(super) fn mark_fresh(&self, id: &str) {
        self.ext.borrow_mut().fresh.insert(id.to_owned());
    }
}

/// The page that holds a background script or service worker.
fn background_page(bg: &brujula_core::ext::manifest::Background) -> String {
    if let Some(page) = &bg.page {
        return format!(
            "<!doctype html><meta charset=utf-8><body><iframe src=\"{}\" style=\"display:none\"></iframe>",
            page.replace('"', "%22")
        );
    }
    let kind = if bg.module { " type=\"module\"" } else { "" };
    let scripts: String = bg
        .scripts
        .iter()
        .map(|s| format!("<script{kind} src=\"{}\"></script>", s.replace('"', "%22")))
        .collect();
    format!("<!doctype html><meta charset=utf-8><title>background</title>{scripts}")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) fn new_installed(id: &str, name: &str, version: &str, source: &str) -> Installed {
    Installed {
        id: id.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        enabled: true,
        source: source.to_owned(),
        installed: now_ms(),
    }
}
