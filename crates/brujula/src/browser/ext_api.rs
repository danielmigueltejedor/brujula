//! The `chrome.*` calls an extension makes, answered by the browser.

use super::ext::{pkg_dir, world_name, ActionState, Done};
use super::*;

use base64::Engine as _;
use brujula_core::ext::pattern::Pattern;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

use glib::translate::IntoGlib;

/// The most an extension can keep in one storage area.
const STORAGE_LIMIT: usize = 10 * 1024 * 1024;
/// The largest answer to a request an extension made.
const FETCH_LIMIT: usize = 25 * 1024 * 1024;

/// Who is calling.
struct Call {
    id: String,
    /// `content`, `background`, `popup` or `page`.
    kind: String,
    href: String,
    tab: Option<u64>,
}

/// Chrome's tab ids start at 1 (0 reads as nothing in a lot of extensions); the model's start at 0.
pub(super) fn js_tab(id: TabId) -> u64 {
    id.0 + 1
}

pub(super) fn tab_of(js: u64) -> Option<TabId> {
    js.checked_sub(1).map(TabId)
}

fn storage_file(id: &str, area: &str) -> PathBuf {
    paths::extensions_dir()
        .join(id)
        .join(format!("storage-{area}.json"))
}

fn selected_keys(map: &Map<String, Value>, keys: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    match keys {
        Value::Null => return map.clone(),
        Value::String(k) => {
            if let Some(v) = map.get(k) {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Array(list) => {
            for k in list.iter().filter_map(Value::as_str) {
                if let Some(v) = map.get(k) {
                    out.insert(k.to_owned(), v.clone());
                }
            }
        }
        // An object lists the keys and what to give when there is nothing stored.
        Value::Object(defaults) => {
            for (k, d) in defaults {
                out.insert(k.clone(), map.get(k).cloned().unwrap_or_else(|| d.clone()));
            }
        }
        _ => {}
    }
    out
}

fn string_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

impl Browser {
    /// One message from an extension: `{op, args, href, tab, kind}` as text.
    pub(super) fn ext_call(self: &Rc<Self>, id: &str, text: &str, done: Done) {
        let Ok(req) = serde_json::from_str::<Value>(text) else {
            return done(Err("mensaje no válido".to_owned()));
        };
        if !self.ext.borrow().loaded.contains_key(id) {
            return done(Err("la extensión está apagada".to_owned()));
        }
        let kind = match req["kind"].as_str() {
            Some(k @ ("content" | "background" | "popup" | "page")) => k.to_owned(),
            _ => return done(Err("origen desconocido".to_owned())),
        };
        let call = Call {
            id: id.to_owned(),
            kind,
            href: req["href"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(2000)
                .collect(),
            tab: req["tab"].as_u64().and_then(tab_of).map(|t| t.0),
        };
        let args = req["args"].clone();
        let op = req["op"].as_str().unwrap_or("");
        match op {
            "storage.get" | "storage.set" | "storage.remove" | "storage.clear"
            | "storage.bytes" => {
                done(self.storage_op(&call, op, &args));
            }
            "storage.noop" => done(Ok(Value::Null)),
            "runtime.getPlatformInfo" => {
                done(Ok(
                    json!({"os": "linux", "arch": "x86-64", "nacl_arch": "x86-64"}),
                ));
            }
            "runtime.openOptionsPage" => {
                let page = self
                    .ext
                    .borrow()
                    .loaded
                    .get(id)
                    .and_then(|l| l.manifest.options_page.clone());
                if let Some(p) = page {
                    self.open_ext_page(id, &p);
                }
                done(Ok(Value::Null));
            }
            "runtime.sendMessage" => self.send_to_pages(&call, &args["msg"], done),
            "deliver.response" => {
                let req_id = args["reqId"].as_u64().unwrap_or(0);
                let waiting = self.ext.borrow_mut().pending.remove(&req_id);
                if let Some(w) = waiting {
                    w(Ok(args["value"].clone()));
                }
                done(Ok(Value::Null));
            }
            "tabs.sendMessage" => self.send_to_tab(&call, &args, done),
            "port.open" => done(Ok(json!(self.port_open(&call, &args)))),
            "port.post" => {
                self.port_send(&call, &args, "msg", args["msg"].clone());
                done(Ok(Value::Null));
            }
            "port.close" => {
                self.port_send(&call, &args, "close", Value::Null);
                self.ext.borrow_mut().ports.remove(&(
                    call.id.clone(),
                    args["port"].as_str().unwrap_or("").to_owned(),
                ));
                done(Ok(Value::Null));
            }
            "tabs.query" => done(Ok(self.tabs_query(&call, &args["query"]))),
            "tabs.get" => done(
                args["id"]
                    .as_u64()
                    .and_then(tab_of)
                    .and_then(|n| self.tab_json(id, n))
                    .ok_or_else(|| "No hay ninguna pestaña con ese id".to_owned()),
            ),
            "tabs.getCurrent" => done(Ok(Value::Null)),
            "tabs.create" => done(self.tabs_create(&call, &args["props"])),
            "tabs.update" => done(self.tabs_update(&call, &args)),
            "tabs.remove" => {
                for n in args["ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .filter_map(tab_of)
                {
                    if self.model.borrow().get(n).is_some() {
                        self.close_tab(n);
                    }
                }
                done(Ok(Value::Null));
            }
            "tabs.reload" => {
                let tab = args["id"]
                    .as_u64()
                    .and_then(tab_of)
                    .or_else(|| self.model.borrow().selected());
                let view = tab.and_then(|t| self.views.borrow().get(&t).cloned());
                if let Some(v) = view {
                    self.reload_view(&v);
                }
                done(Ok(Value::Null));
            }
            "windows.get" => done(Ok(
                json!({"id": 1, "focused": true, "incognito": false, "type": "normal", "state": "normal"}),
            )),
            "windows.getAll" => done(Ok(
                json!([{"id": 1, "focused": true, "incognito": false, "type": "normal", "state": "normal"}]),
            )),
            "scripting.executeScript" => self.script_exec(&call, &args, done),
            "scripting.insertCSS" => done(self.insert_css(&call, &args)),
            "action.set" => {
                {
                    let mut s = self.ext.borrow_mut();
                    let a: &mut ActionState = s.action.entry(id.to_owned()).or_default();
                    if let Some(t) = args["badge"].as_str() {
                        a.badge = t.chars().take(8).collect();
                    }
                    if let Some(c) = args["color"].as_str() {
                        a.color = c.chars().take(40).collect();
                    }
                    if let Some(t) = args["title"].as_str() {
                        a.title = Some(t.chars().take(200).collect());
                    }
                    if let Some(p) = args["popup"].as_str() {
                        a.popup = brujula_core::ext::manifest::safe_path(p);
                    }
                }
                done(Ok(Value::Null));
            }
            "action.get" => {
                let s = self.ext.borrow();
                let a = s.action.get(id).cloned().unwrap_or_default();
                done(Ok(match args["what"].as_str() {
                    Some("badge") => json!(a.badge),
                    Some("title") => json!(a.title.unwrap_or_default()),
                    _ => json!(a.popup.unwrap_or_default()),
                }));
            }
            "action.openPopup" => done(Ok(Value::Null)),
            "ui.resize" => {
                let w = args["w"].as_i64().unwrap_or(0).clamp(120, 800);
                let h = args["h"].as_i64().unwrap_or(0).clamp(60, 600);
                if let Some(v) = self.ext.borrow().popups.get(id) {
                    v.set_size_request(
                        i32::try_from(w).unwrap_or(320),
                        i32::try_from(h).unwrap_or(200),
                    );
                }
                done(Ok(Value::Null));
            }
            "net.fetch" => self.net_fetch(&call, &args, done),
            other => done(Err(format!("«{other}» no está disponible en Brújula"))),
        }
    }

    // ---- storage

    fn storage_op(&self, call: &Call, op: &str, args: &Value) -> Result<Value, String> {
        let area = args["area"].as_str().unwrap_or("local");
        if !matches!(area, "local" | "sync" | "session" | "managed") {
            return Err("área de almacenamiento desconocida".to_owned());
        }
        let key = (call.id.clone(), area.to_owned());
        let mut changes = Map::new();
        let out = {
            let mut s = self.ext.borrow_mut();
            let map = s.storage.entry(key.clone()).or_insert_with(|| {
                if area == "session" || area == "managed" {
                    return Map::new();
                }
                std::fs::read_to_string(storage_file(&call.id, area))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Map<String, Value>>(&t).ok())
                    .unwrap_or_default()
            });
            match op {
                "storage.get" => return Ok(Value::Object(selected_keys(map, &args["keys"]))),
                "storage.bytes" => return Ok(json!(Value::Object(map.clone()).to_string().len())),
                "storage.set" => {
                    if area == "managed" {
                        return Err("managed storage is read-only".to_owned());
                    }
                    let Some(items) = args["items"].as_object() else {
                        return Err("se esperaba un objeto".to_owned());
                    };
                    let mut next = map.clone();
                    for (k, v) in items {
                        let old = next.insert(k.clone(), v.clone());
                        let mut c = Map::new();
                        if let Some(o) = old {
                            c.insert("oldValue".to_owned(), o);
                        }
                        c.insert("newValue".to_owned(), v.clone());
                        changes.insert(k.clone(), Value::Object(c));
                    }
                    if Value::Object(next.clone()).to_string().len() > STORAGE_LIMIT {
                        return Err("QUOTA_BYTES quota exceeded".to_owned());
                    }
                    *map = next;
                }
                "storage.remove" => {
                    for k in string_list(&args["keys"]) {
                        if let Some(o) = map.remove(&k) {
                            changes.insert(k, json!({"oldValue": o}));
                        }
                    }
                }
                _ => {
                    for (k, o) in std::mem::take(map) {
                        changes.insert(k, json!({"oldValue": o}));
                    }
                }
            }
            if area != "session" {
                let text = Value::Object(map.clone()).to_string();
                if let Err(e) = paths::write_atomic(&storage_file(&call.id, area), &text) {
                    eprintln!("brujula: could not save the storage of {}: {e}", call.id);
                }
            }
            Value::Null
        };
        if !changes.is_empty() {
            self.emit_everywhere(&call.id, "storage.onChanged", &json!([changes, area]));
        }
        Ok(out)
    }

    // ---- tabs

    /// The tab as an extension sees it. Address and title only with the right to read them.
    pub(super) fn tab_json(&self, ext_id: &str, id: TabId) -> Option<Value> {
        let m = self.model.borrow();
        let t = m.get(id)?;
        let selected = m.selected() == Some(id);
        let mut o = json!({
            "id": js_tab(t.id),
            "index": m.position(id).unwrap_or(0),
            "windowId": 1,
            "active": selected,
            "highlighted": selected,
            "pinned": t.pinned,
            "audible": t.audible,
            "mutedInfo": {"muted": false},
            "discarded": self.lazy.borrow().contains_key(&id),
            "incognito": false,
            "groupId": -1,
            "status": if t.loading { "loading" } else { "complete" },
        });
        let allowed = self
            .ext
            .borrow()
            .loaded
            .get(ext_id)
            .is_some_and(|l| l.manifest.has_permission("tabs") || l.manifest.can_reach(&t.url));
        if allowed {
            o["url"] = json!(t.url);
            o["title"] = json!(t.title);
        }
        Some(o)
    }

    fn tabs_query(&self, call: &Call, q: &Value) -> Value {
        let ids: Vec<TabId> = self.model.borrow().tabs().iter().map(|t| t.id).collect();
        let patterns: Vec<Pattern> = string_list(&q["url"])
            .iter()
            .filter_map(|p| Pattern::parse(p))
            .collect();
        let want_url = !q["url"].is_null();
        let mut out = Vec::new();
        for id in ids {
            let Some(tab) = self.tab_json(&call.id, id) else {
                continue;
            };
            let flag = |k: &str| q[k].as_bool();
            if flag("active").is_some_and(|a| a != tab["active"].as_bool().unwrap_or(false)) {
                continue;
            }
            if flag("pinned").is_some_and(|a| a != tab["pinned"].as_bool().unwrap_or(false)) {
                continue;
            }
            if flag("audible").is_some_and(|a| a != tab["audible"].as_bool().unwrap_or(false)) {
                continue;
            }
            if q["status"]
                .as_str()
                .is_some_and(|s| Some(s) != tab["status"].as_str())
            {
                continue;
            }
            if want_url {
                let url = self
                    .model
                    .borrow()
                    .get(id)
                    .map(|t| t.url.clone())
                    .unwrap_or_default();
                let reach = self.ext.borrow().loaded.get(&call.id).is_some_and(|l| {
                    l.manifest.has_permission("tabs") || l.manifest.can_reach(&url)
                });
                if !reach || !patterns.iter().any(|p| p.matches(&url)) {
                    continue;
                }
            }
            out.push(tab);
        }
        Value::Array(out)
    }

    /// A tab address an extension may open: the web, a blank page or its own pages.
    fn allowed_url(&self, ext_id: &str, url: &str) -> bool {
        url == "about:blank"
            || url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with(&format!("brujula-ext://{ext_id}/"))
    }

    fn tabs_create(self: &Rc<Self>, call: &Call, props: &Value) -> Result<Value, String> {
        let url = props["url"].as_str().unwrap_or("about:blank");
        if !self.allowed_url(&call.id, url) {
            return Err("Esa dirección no se puede abrir desde una extensión".to_owned());
        }
        if let Some(rest) = url.strip_prefix(&format!("brujula-ext://{}/", call.id)) {
            self.open_ext_page(&call.id, rest);
            return Ok(Value::Null);
        }
        let active = props["active"].as_bool().unwrap_or(true);
        let id = self.open_tab(url, None, active);
        if props["pinned"].as_bool() == Some(true) {
            let c = self.model.borrow_mut().set_pinned(id, true);
            self.apply(c);
        }
        Ok(self.tab_json(&call.id, id).unwrap_or(Value::Null))
    }

    fn tabs_update(self: &Rc<Self>, call: &Call, args: &Value) -> Result<Value, String> {
        let props = &args["props"];
        let id = args["id"]
            .as_u64()
            .and_then(tab_of)
            .or_else(|| self.model.borrow().selected())
            .ok_or_else(|| "No hay pestaña".to_owned())?;
        if self.model.borrow().get(id).is_none() {
            return Err("No hay ninguna pestaña con ese id".to_owned());
        }
        if let Some(url) = props["url"].as_str() {
            if !self.allowed_url(&call.id, url) {
                return Err("Esa dirección no se puede abrir desde una extensión".to_owned());
            }
            let view = self.views.borrow().get(&id).cloned();
            self.lazy.borrow_mut().remove(&id);
            if let Some(v) = view {
                v.load_uri(url);
            }
        }
        if let Some(p) = props["pinned"].as_bool() {
            let c = self.model.borrow_mut().set_pinned(id, p);
            self.apply(c);
        }
        if props["active"].as_bool() == Some(true) {
            let c = self.model.borrow_mut().select(id);
            self.apply(c);
        }
        Ok(self.tab_json(&call.id, id).unwrap_or(Value::Null))
    }

    /// The tab a content script is in: the id its page was given, or the one at its address.
    fn sender_tab(&self, call: &Call) -> Option<TabId> {
        if let Some(n) = call.tab.map(TabId) {
            if self.model.borrow().get(n).is_some() {
                return Some(n);
            }
        }
        let selected = self.model.borrow().selected();
        let views = self.views.borrow();
        let same = |id: &TabId| {
            views
                .get(id)
                .is_some_and(|v| v.uri().as_deref() == Some(call.href.as_str()))
        };
        selected
            .filter(same)
            .or_else(|| views.keys().copied().find(same))
            .or(selected)
    }

    // ---- messages between the parts of an extension

    /// Every place a script of the extension may be listening: its pages, and the web pages its
    /// content scripts work on.
    fn contexts(&self, id: &str) -> Vec<(WebView, Option<String>)> {
        let s = self.ext.borrow();
        let mut out: Vec<(WebView, Option<String>)> = Vec::new();
        out.extend(s.background.get(id).map(|v| (v.clone(), None)));
        out.extend(s.popups.get(id).map(|v| (v.clone(), None)));
        for (v, _) in s.pages.get(id).into_iter().flatten() {
            out.push((v.clone(), None));
        }
        if let Some(l) = s.loaded.get(id) {
            let patterns: Vec<&Pattern> = l
                .manifest
                .content_scripts
                .iter()
                .flat_map(|c| &c.matches)
                .collect();
            for v in self.views.borrow().values() {
                if let Some(uri) = v.uri() {
                    if patterns.iter().any(|p| p.matches(&uri)) {
                        out.push((v.clone(), Some(world_name(id))));
                    }
                }
            }
        }
        out
    }

    fn eval_in(view: &WebView, world: Option<&str>, js: &str) {
        view.evaluate_javascript(js, world, None, gio::Cancellable::NONE, |_| {});
    }

    /// Runs `js` in the view; for a background page that is still loading, once it has.
    fn eval_when_ready(&self, id: &str, view: &WebView, world: Option<String>, js: String) {
        let is_bg = self
            .ext
            .borrow()
            .background
            .get(id)
            .is_some_and(|b| b == view);
        let ready = self.ext.borrow().bg_ready.contains(id);
        if is_bg && !ready {
            let view = view.clone();
            self.ext
                .borrow_mut()
                .bg_wait
                .entry(id.to_owned())
                .or_default()
                .push(Box::new(move || {
                    Self::eval_in(&view, world.as_deref(), &js)
                }));
        } else {
            Self::eval_in(view, world.as_deref(), &js);
        }
    }

    /// Fires an event of the extension API in every place the extension has a script.
    pub(super) fn emit_everywhere(&self, id: &str, name: &str, args: &Value) {
        let js = format!(
            "globalThis.__brujulaEmit&&globalThis.__brujulaEmit({},{})",
            Value::String(name.to_owned()),
            args
        );
        for (v, world) in self.contexts(id) {
            Self::eval_in(&v, world.as_deref(), &js);
        }
    }

    /// The same, only in the extension's own pages (the background page and the popup).
    pub(super) fn emit_to_pages(&self, id: &str, name: &str, args: &Value) {
        let js = format!(
            "globalThis.__brujulaEmit&&globalThis.__brujulaEmit({},{})",
            Value::String(name.to_owned()),
            args
        );
        for (v, world) in self.contexts(id).into_iter().filter(|(_, w)| w.is_none()) {
            Self::eval_in(&v, world.as_deref(), &js);
        }
    }

    /// Tells the extensions about a change to the tabs.
    pub(super) fn emit_tab_event(&self, name: &str, build: impl Fn(&str) -> Value) {
        let ids: Vec<String> = self.ext.borrow().loaded.keys().cloned().collect();
        for id in ids {
            let args = build(&id);
            if !args.is_null() {
                self.emit_to_pages(&id, name, &args);
            }
        }
    }

    /// A page started loading in a tab: its content scripts are told which tab they are in.
    pub(super) fn ext_committed(&self, tab: TabId, view: &WebView) {
        let Some(uri) = view.uri() else { return };
        let ids: Vec<String> = self.ext.borrow().loaded.keys().cloned().collect();
        for id in &ids {
            let matches = self.ext.borrow().loaded.get(id).is_some_and(|l| {
                l.manifest
                    .content_scripts
                    .iter()
                    .any(|c| c.matches.iter().any(|p| p.matches(&uri)))
                    || l.manifest.has_permission("scripting")
                    || l.manifest.has_permission("activeTab")
            });
            if matches {
                let js = format!("globalThis.__brujulaTab={}", js_tab(tab));
                Self::eval_in(view, Some(&world_name(id)), &js);
            }
        }
        let url = uri.to_string();
        self.emit_tab_event("tabs.onUpdated", |ext| match self.tab_json(ext, tab) {
            Some(t) => {
                let mut change = json!({"status": "loading"});
                if t.get("url").is_some() {
                    change["url"] = json!(url);
                }
                json!([js_tab(tab), change, t])
            }
            None => Value::Null,
        });
    }

    pub(super) fn ext_finished(&self, tab: TabId) {
        self.emit_tab_event("tabs.onUpdated", |ext| match self.tab_json(ext, tab) {
            Some(t) => json!([js_tab(tab), {"status": "complete"}, t]),
            None => Value::Null,
        });
    }

    /// The tabs changed: the extensions hear about it.
    pub(super) fn emit_tab_change(&self, change: &Change) {
        if self.ext.borrow().loaded.is_empty() {
            return;
        }
        match *change {
            Change::Opened(id) => self.emit_tab_event("tabs.onCreated", |ext| {
                self.tab_json(ext, id).map_or(Value::Null, |t| json!([t]))
            }),
            Change::Closed(id) => self.emit_tab_event(
                "tabs.onRemoved",
                |_| json!([js_tab(id), {"windowId": 1, "isWindowClosing": false}]),
            ),
            Change::Selected(id) => self.emit_tab_event(
                "tabs.onActivated",
                |_| json!([{"tabId": js_tab(id), "windowId": 1}]),
            ),
            _ => {}
        }
    }

    /// The view and world the caller itself lives in.
    fn own_context(&self, call: &Call) -> Option<(WebView, Option<String>)> {
        match call.kind.as_str() {
            "background" => self
                .ext
                .borrow()
                .background
                .get(&call.id)
                .map(|v| (v.clone(), None)),
            "popup" => self
                .ext
                .borrow()
                .popups
                .get(&call.id)
                .map(|v| (v.clone(), None)),
            "page" => self
                .ext
                .borrow()
                .pages
                .get(&call.id)
                .and_then(|p| p.first())
                .map(|(v, _)| (v.clone(), None)),
            _ => {
                let tab = self.sender_tab(call)?;
                let view = self.views.borrow().get(&tab).cloned()?;
                Some((view, Some(world_name(&call.id))))
            }
        }
    }

    /// `runtime.connect` and `tabs.connect`: a port to the other parts of the extension.
    fn port_open(self: &Rc<Self>, call: &Call, args: &Value) -> bool {
        let Some(port) = args["port"]
            .as_str()
            .filter(|p| p.len() < 80)
            .map(str::to_owned)
        else {
            return false;
        };
        let Some(opener) = self.own_context(call) else {
            return false;
        };
        let receivers: Vec<(WebView, Option<String>)> = if args["target"]["kind"] == "tab" {
            args["target"]["tab"]
                .as_u64()
                .and_then(tab_of)
                .and_then(|t| self.views.borrow().get(&t).cloned())
                .map(|v| vec![(v, Some(world_name(&call.id)))])
                .unwrap_or_default()
        } else {
            let s = self.ext.borrow();
            let mut t = Vec::new();
            if call.kind != "background" {
                t.extend(s.background.get(&call.id).map(|v| (v.clone(), None)));
            }
            if call.kind != "popup" {
                t.extend(s.popups.get(&call.id).map(|v| (v.clone(), None)));
            }
            if call.kind != "page" {
                for (v, _) in s.pages.get(&call.id).into_iter().flatten() {
                    t.push((v.clone(), None));
                }
            }
            t
        };
        if receivers.is_empty() {
            return false;
        }
        let mut sender = json!({"id": call.id, "url": call.href});
        if call.kind == "content" {
            if let Some(tab) = self
                .sender_tab(call)
                .and_then(|t| self.tab_json(&call.id, t))
            {
                sender["tab"] = tab;
                sender["frameId"] = json!(0);
            }
        }
        let name = args["name"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(100)
            .collect::<String>();
        let js = format!(
            "globalThis.__brujulaPort&&globalThis.__brujulaPort('open',{},{},{sender})",
            Value::String(port.clone()),
            json!({"name": name})
        );
        for (v, w) in &receivers {
            self.eval_when_ready(&call.id, v, w.clone(), js.clone());
        }
        let mut s = self.ext.borrow_mut();
        if s.ports.len() > 500 {
            s.ports.clear();
        }
        s.ports.insert(
            (call.id.clone(), port),
            super::ext::PortPair { opener, receivers },
        );
        true
    }

    /// A message or a close on a port goes to the side that did not send it.
    fn port_send(&self, call: &Call, args: &Value, what: &str, data: Value) {
        let Some(port) = args["port"].as_str() else {
            return;
        };
        let targets: Vec<(WebView, Option<String>)> = {
            let s = self.ext.borrow();
            let Some(pair) = s.ports.get(&(call.id.clone(), port.to_owned())) else {
                return;
            };
            if args["opener"].as_bool().unwrap_or(false) {
                pair.receivers.clone()
            } else {
                vec![pair.opener.clone()]
            }
        };
        let js = format!(
            "globalThis.__brujulaPort&&globalThis.__brujulaPort({},{},{data})",
            Value::String(what.to_owned()),
            Value::String(port.to_owned())
        );
        for (v, w) in targets {
            self.eval_when_ready(&call.id, &v, w, js.clone());
        }
    }

    /// `runtime.sendMessage`: to the extension's other pages.
    fn send_to_pages(self: &Rc<Self>, call: &Call, msg: &Value, done: Done) {
        let targets: Vec<(WebView, Option<String>)> = {
            let s = self.ext.borrow();
            let mut t = Vec::new();
            if call.kind != "background" {
                t.extend(s.background.get(&call.id).map(|v| (v.clone(), None)));
            }
            if call.kind != "popup" {
                t.extend(s.popups.get(&call.id).map(|v| (v.clone(), None)));
            }
            if call.kind != "page" {
                for (v, _) in s.pages.get(&call.id).into_iter().flatten() {
                    t.push((v.clone(), None));
                }
            }
            t
        };
        let mut sender = json!({"id": call.id, "url": call.href});
        if call.kind == "content" {
            if let Some(tab) = self
                .sender_tab(call)
                .and_then(|t| self.tab_json(&call.id, t))
            {
                sender["tab"] = tab;
                sender["frameId"] = json!(0);
            }
        }
        self.deliver(&call.id, targets, msg, &sender, done);
    }

    /// `tabs.sendMessage`: to the content scripts of the extension in a tab.
    fn send_to_tab(self: &Rc<Self>, call: &Call, args: &Value, done: Done) {
        let Some(id) = args["id"].as_u64().and_then(tab_of) else {
            return done(Err("falta el id de la pestaña".to_owned()));
        };
        let view = self.views.borrow().get(&id).cloned();
        let Some(view) = view else {
            return done(Err("No hay ninguna pestaña con ese id".to_owned()));
        };
        let sender = json!({"id": call.id});
        self.deliver(
            &call.id,
            vec![(view, Some(world_name(&call.id)))],
            &args["msg"],
            &sender,
            done,
        );
    }

    /// Sends a message to each of `targets` and answers with the first reply that says
    /// something; with nothing from any of them, with nothing.
    fn deliver(
        self: &Rc<Self>,
        id: &str,
        targets: Vec<(WebView, Option<String>)>,
        msg: &Value,
        sender: &Value,
        done: Done,
    ) {
        if targets.is_empty() {
            return done(Err(
                "Could not establish connection. Receiving end does not exist.".to_owned(),
            ));
        }
        let state = Rc::new(RefCell::new((targets.len(), Some(done))));
        for (view, world) in targets {
            let req_id = {
                let mut s = self.ext.borrow_mut();
                s.next_req += 1;
                s.next_req
            };
            let st = state.clone();
            let respond: Done = Box::new(move |r| {
                let v = r.unwrap_or(Value::Null);
                let finish = {
                    let mut g = st.borrow_mut();
                    g.0 = g.0.saturating_sub(1);
                    if !v.is_null() || g.0 == 0 {
                        g.1.take()
                    } else {
                        None
                    }
                };
                if let Some(d) = finish {
                    d(Ok(v));
                }
            });
            self.ext.borrow_mut().pending.insert(req_id, respond);
            let js = format!(
                "globalThis.__brujulaDeliver&&globalThis.__brujulaDeliver({req_id},{msg},{sender})"
            );
            self.eval_when_ready(id, &view, world, js);
            // A page that goes away without answering does not keep the sender waiting.
            let this = Rc::downgrade(self);
            glib::timeout_add_seconds_local_once(30, move || {
                if let Some(b) = this.upgrade() {
                    let waiting = b.ext.borrow_mut().pending.remove(&req_id);
                    if let Some(w) = waiting {
                        w(Ok(Value::Null));
                    }
                }
            });
        }
    }

    // ---- scripts and styles in a tab

    /// The tab and its view an extension may run code in: it needs the address allowed by its
    /// host permissions, or `activeTab`.
    fn script_target(&self, call: &Call, id: Option<u64>) -> Result<(TabId, WebView), String> {
        let tab = id
            .and_then(tab_of)
            .or_else(|| self.model.borrow().selected())
            .ok_or_else(|| "No hay pestaña".to_owned())?;
        let url = self
            .model
            .borrow()
            .get(tab)
            .map(|t| t.url.clone())
            .ok_or_else(|| "No hay ninguna pestaña con ese id".to_owned())?;
        let allowed =
            self.ext.borrow().loaded.get(&call.id).is_some_and(|l| {
                l.manifest.can_reach(&url) || l.manifest.has_permission("activeTab")
            });
        if !allowed || !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("La extensión no tiene permiso para esta página".to_owned());
        }
        let view = self
            .views
            .borrow()
            .get(&tab)
            .cloned()
            .ok_or_else(|| "La pestaña está dormida".to_owned())?;
        Ok((tab, view))
    }

    fn read_files(&self, id: &str, files: &Value) -> Result<String, String> {
        let dir = pkg_dir(id);
        let mut out = Vec::new();
        for f in string_list(files) {
            let path = brujula_core::ext::manifest::safe_path(&f).ok_or("ruta no válida")?;
            out.push(
                std::fs::read_to_string(dir.join(path))
                    .map_err(|_| format!("No se puede leer {f}"))?,
            );
        }
        Ok(out.join("\n;\n"))
    }

    fn script_exec(self: &Rc<Self>, call: &Call, args: &Value, done: Done) {
        let (tab, view) = match self.script_target(call, args["id"].as_u64()) {
            Ok(t) => t,
            Err(e) => return done(Err(e)),
        };
        let source = match (args["code"].as_str(), args["files"].is_null()) {
            (Some(code), _) => code.to_owned(),
            (None, false) => match self.read_files(&call.id, &args["files"]) {
                Ok(s) => s,
                Err(e) => return done(Err(e)),
            },
            _ => return done(Err("Falta el código o los archivos".to_owned())),
        };
        let Some(shim) = self.ext_prelude(&call.id, "content") else {
            return done(Err("la extensión está apagada".to_owned()));
        };
        // The script runs as the last statement of the body, in the extension's world, which
        // has the API; what it evaluates to comes back as text.
        let body = format!(
            "{shim}\nglobalThis.__brujulaTab = {};\nconst r = await (0, eval)({});\nreturn JSON.stringify(r === undefined ? null : r);",
            js_tab(tab),
            Value::String(source)
        );
        let mv3 = args["mv3"].as_bool().unwrap_or(false);
        view.call_async_javascript_function(
            &body,
            None,
            Some(&world_name(&call.id)),
            None,
            gio::Cancellable::NONE,
            move |res| match res {
                Ok(v) => {
                    let value: Value = serde_json::from_str(&v.to_str()).unwrap_or(Value::Null);
                    done(Ok(if mv3 {
                        json!([{"frameId": 0, "result": value}])
                    } else {
                        json!([value])
                    }));
                }
                Err(e) => done(Err(e.message().to_owned())),
            },
        );
    }

    fn insert_css(&self, call: &Call, args: &Value) -> Result<Value, String> {
        let (_, view) = self.script_target(call, args["id"].as_u64())?;
        let css = match args["css"].as_str() {
            Some(c) => c.to_owned(),
            None => self.read_files(&call.id, &args["files"])?,
        };
        let js = format!(
            "(function(){{const s=document.createElement('style');s.textContent={};(document.head||document.documentElement).appendChild(s)}})()",
            Value::String(css)
        );
        Self::eval_in(&view, Some(&world_name(&call.id)), &js);
        Ok(Value::Null)
    }

    // ---- requests of an extension's pages to the web

    fn net_fetch(self: &Rc<Self>, call: &Call, args: &Value, done: Done) {
        let url = args["url"].as_str().unwrap_or("").to_owned();
        let allowed = (url.starts_with("https://") || url.starts_with("http://"))
            && self
                .ext
                .borrow()
                .loaded
                .get(&call.id)
                .is_some_and(|l| l.manifest.can_reach(&url));
        if !allowed {
            return done(Err(
                "La extensión no tiene permiso para pedir esa dirección".to_owned(),
            ));
        }
        let method = args["method"]
            .as_str()
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let session = {
            let mut s = self.ext.borrow_mut();
            s.http
                .get_or_insert_with(|| {
                    let session = soup::Session::new();
                    session.set_timeout(30);
                    session.set_user_agent("Brujula");
                    session
                })
                .clone()
        };
        let Ok(message) = soup::Message::new(&method, &url) else {
            return done(Err("petición no válida".to_owned()));
        };
        if let Some(headers) = message.request_headers() {
            for h in args["headers"].as_array().into_iter().flatten() {
                let (Some(n), Some(v)) = (h[0].as_str(), h[1].as_str()) else {
                    continue;
                };
                // The page does not get to pick who it says it is, or send cookies it has no access to.
                if matches!(
                    n.to_ascii_lowercase().as_str(),
                    "host" | "cookie" | "origin" | "content-length"
                ) {
                    continue;
                }
                headers.replace(n, v);
            }
        }
        if let Some(body) = args["body"].as_str().filter(|b| !b.is_empty()) {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(body) {
                message.set_request_body_from_bytes(None, Some(&glib::Bytes::from_owned(bytes)));
            }
        }
        glib::spawn_future_local(async move {
            let result = session
                .send_and_read_future(&message, glib::Priority::DEFAULT)
                .await;
            match result {
                Ok(bytes) if bytes.len() <= FETCH_LIMIT => {
                    let mut headers = Vec::new();
                    if let Some(h) = message.response_headers() {
                        h.foreach(|n, v| headers.push(json!([n.to_string(), v.to_string()])));
                    }
                    done(Ok(json!({
                        "status": message.status().into_glib(),
                        "statusText": message.reason_phrase().map(|s| s.to_string()).unwrap_or_default(),
                        "headers": headers,
                        "body": base64::engine::general_purpose::STANDARD.encode(&bytes),
                    })));
                }
                Ok(_) => done(Err("La respuesta es demasiado grande".to_owned())),
                Err(e) => done(Err(e.message().to_owned())),
            }
        });
    }
}
