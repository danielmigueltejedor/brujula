//! The optional AI assistant. Off until the user turns it on in the preferences; while it is
//! off Brújula sends nothing to any AI, shows no button and does not touch the keyring.
//!
//! The user picks the provider and brings their own key, which lives in the system keyring
//! (`keyring.rs`). The assistant answers in a panel beside the page and, when it needs to, uses
//! tools on the tabs. Every tool call goes through `brujula_core::ai::policy::decide`, which
//! says whether it may go ahead, must be asked first or is refused; this file only carries out
//! what that allows. Everything the assistant is shown or does is written in the panel.

use std::time::Duration;

use super::ai_panel::{AiUi, Answer};
use super::*;

use brujula_core::ai::policy::{self, Call, Decision, Grant, Target};
use brujula_core::ai::wire::{self, Message, Part, Role, ToolCall};
use brujula_core::ai::{page, prompt, AiSettings};
use glib::translate::IntoGlib;
use serde_json::json;

/// Largest answer of a provider that is read, in bytes.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
/// How much page text a conversation keeps before the oldest is dropped, in characters.
const HISTORY_BUDGET: usize = 60_000;
/// What a tool may hand back to the model, in characters.
const MAX_RESULT: usize = 14_000;

pub(super) struct AiState {
    pub ui: AiUi,
    pub settings: RefCell<AiSettings>,
    history: RefCell<Vec<Message>>,
    session: RefCell<policy::Session>,
    /// The last reading of each tab, so that «click element 3» is about what was read.
    snapshots: RefCell<HashMap<TabId, page::Snapshot>>,
    busy: Cell<bool>,
    /// Changes whenever a conversation is stopped or cleared: work that started before it ends.
    run: Cell<u64>,
    http: RefCell<Option<soup::Session>>,
    /// Whether the keyring has a key for the provider in use (asked while the assistant is on).
    key_set: Cell<bool>,
    /// The result of the last «Probar conexión», for the preferences page.
    status: RefCell<String>,
}

impl AiState {
    pub fn new(private: bool) -> Self {
        let settings = if private {
            // A private window keeps nothing and shares nothing: no assistant.
            AiSettings::default()
        } else {
            std::fs::read_to_string(paths::ai_file())
                .map(|t| AiSettings::from_json(&t))
                .unwrap_or_default()
        };
        Self {
            ui: AiUi::new(),
            settings: RefCell::new(settings),
            history: RefCell::new(Vec::new()),
            session: RefCell::new(policy::Session::default()),
            snapshots: RefCell::new(HashMap::new()),
            busy: Cell::new(false),
            run: Cell::new(0),
            http: RefCell::new(None),
            key_set: Cell::new(false),
            status: RefCell::new(String::new()),
        }
    }
}

fn clip(text: &str, max: usize) -> String {
    let t: String = text.chars().filter(|c| !c.is_control()).collect();
    if t.chars().count() > max {
        format!("{}…", t.chars().take(max).collect::<String>())
    } else {
        t
    }
}

impl Browser {
    pub(super) fn ai_enabled(&self) -> bool {
        !self.private && self.ai.settings.borrow().enabled
    }

    fn ai_save(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::ai_file(), &self.ai.settings.borrow().to_json());
    }

    /// Connects the panel and the buttons. Called once, with the window.
    pub(super) fn wire_ai(self: &Rc<Self>) {
        let toggle = gio::SimpleAction::new("toggle-assistant", None);
        let this = self.clone();
        toggle.connect_activate(move |_, _| {
            if this.ai.ui.panel.is_visible() {
                this.ai_hide_panel();
            } else {
                this.ai_show_panel();
            }
        });
        self.window.add_action(&toggle);

        let ui = &self.ai.ui;
        let this = self.clone();
        ui.entry.connect_activate(move |_| this.ai_submit());
        let this = self.clone();
        ui.send.connect_clicked(move |_| {
            if this.ai.busy.get() {
                this.ai_cancel();
            } else {
                this.ai_submit();
            }
        });
        let this = self.clone();
        ui.new_chat.connect_clicked(move |_| this.ai_reset());
        let this = self.clone();
        ui.close.connect_clicked(move |_| this.ai_hide_panel());
        let this = self.clone();
        ui.prefs.connect_clicked(move |_| {
            // The preferences need the room the panel takes.
            this.ai_hide_panel();
            this.open_settings(Some("ai"));
        });
        let weak = Rc::downgrade(self);
        ui.set_on_link(move |uri| {
            // A link in an answer is the user's to follow, but only on the web.
            if let Some(this) = weak.upgrade() {
                if policy::openable(uri).is_ok() || uri.starts_with("https://") {
                    let after = this.model.borrow().selected();
                    this.open_tab(uri, after, true);
                }
            }
        });
        self.ai_refresh_ui();
        if self.ai_enabled() {
            self.ai_refresh_key(false);
        }
    }

    /// Shows or hides the buttons and the panel to match the switch in the preferences.
    pub(super) fn ai_refresh_ui(self: &Rc<Self>) {
        let on = self.ai_enabled();
        let ui = &self.ai.ui;
        if let Some(a) = self.window.lookup_action("toggle-assistant") {
            if let Some(a) = a.downcast_ref::<gio::SimpleAction>() {
                a.set_enabled(on);
            }
        }
        ui.side_button.set_visible(on);
        ui.top_button.set_visible(on);
        if !on {
            self.ai_reset();
            ui.panel.set_visible(false);
            return;
        }
        let s = self.ai.settings.borrow();
        ui.set_who(&format!("{} · {}", s.provider.short(), s.model()));
        let mut sees = String::new();
        if s.read {
            sees.push_str(
                "Puede ver los títulos y direcciones de tus pestañas y leer las páginas que le apruebes. ",
            );
        } else {
            sees.push_str("Solo ve lo que le escribes aquí: no ve tus pestañas. ");
        }
        sees.push_str("Nunca ve contraseñas, campos de pago ni ventanas privadas. ");
        if s.interact {
            sees.push_str("Para pulsar o escribir en una página te pregunta siempre. ");
        }
        sees.push_str(if s.is_remote() {
            "Lo que lea se envía a "
        } else {
            "Todo se queda en este equipo"
        });
        if s.is_remote() {
            sees.push_str(s.provider.short());
            sees.push('.');
        } else {
            sees.push('.');
        }
        ui.set_sees(&sees);
    }

    fn ai_show_panel(self: &Rc<Self>) {
        if !self.ai_enabled() {
            return;
        }
        self.ai.ui.panel.set_visible(true);
        self.ai.ui.entry.grab_focus();
    }

    fn ai_hide_panel(self: &Rc<Self>) {
        // A question nobody can see cannot be answered.
        self.ai.ui.cancel_question();
        self.ai.ui.panel.set_visible(false);
    }

    /// Stops what the assistant is doing, without clearing what was said.
    fn ai_cancel(self: &Rc<Self>) {
        if !self.ai.busy.get() {
            return;
        }
        self.ai.run.set(self.ai.run.get() + 1);
        self.ai.busy.set(false);
        self.ai.ui.set_busy(false);
        self.ai.ui.cancel_question();
        if let Some(s) = self.ai.http.borrow().as_ref() {
            s.abort();
        }
        self.ai_repair_history();
        self.ai.ui.add_note("Detenido.");
    }

    /// A stopped or failed turn can leave a tool call without its result, which providers
    /// refuse: take back everything after the last question.
    fn ai_repair_history(&self) {
        let mut h = self.ai.history.borrow_mut();
        if let Some(last_user) = h.iter().rposition(|m| {
            m.role == Role::User && m.parts.iter().any(|p| matches!(p, Part::Text(_)))
        }) {
            h.truncate(last_user + 1);
        }
    }

    /// A new conversation: nothing of the old one is kept, and what it was allowed is forgotten.
    pub(super) fn ai_reset(self: &Rc<Self>) {
        self.ai.run.set(self.ai.run.get() + 1);
        self.ai.busy.set(false);
        self.ai.ui.set_busy(false);
        if let Some(s) = self.ai.http.borrow().as_ref() {
            s.abort();
        }
        self.ai.history.borrow_mut().clear();
        *self.ai.session.borrow_mut() = policy::Session::default();
        self.ai.snapshots.borrow_mut().clear();
        self.ai.ui.clear();
    }

    fn ai_submit(self: &Rc<Self>) {
        let text = self.ai.ui.entry.text().to_string();
        self.ai.ui.entry.set_text("");
        self.ai_ask(&text);
    }

    /// The user asks something: from the panel, or from the search box when the assistant runs it.
    pub(super) fn ai_ask(self: &Rc<Self>, text: &str) {
        if !self.ai_enabled() {
            return;
        }
        let text: String = text.trim().chars().take(4000).collect();
        if text.is_empty() {
            return;
        }
        self.ai_show_panel();
        if self.ai.busy.get() {
            self.ai
                .ui
                .add_note("Espera a que termine lo anterior, o pulsa Detener.");
            return;
        }
        self.ai.ui.add_user(&text);
        {
            // An address the user typed is theirs to have opened.
            let mut s = self.ai.session.borrow_mut();
            for word in text.split_whitespace() {
                let word = word.trim_matches(|c: char| matches!(c, '.' | ',' | ')' | '(' | '"'));
                if word.starts_with("http://") || word.starts_with("https://") {
                    s.known_urls.insert(word.to_owned());
                }
            }
        }
        if self.ai.history.borrow().is_empty() {
            let s = self.ai.settings.borrow();
            let note = if s.is_remote() {
                format!("Se envía a {} ({}).", s.provider.short(), s.model())
            } else {
                format!("Se queda en este equipo ({}).", s.model())
            };
            drop(s);
            self.ai.ui.add_note(&note);
        }
        self.ai.history.borrow_mut().push(Message::user(&text));
        self.ai.busy.set(true);
        self.ai.ui.set_busy(true);
        let run = self.ai.run.get();
        let this = self.clone();
        glib::spawn_future_local(async move {
            this.ai_loop(run).await;
            if this.ai.run.get() == run {
                this.ai.busy.set(false);
                this.ai.ui.set_busy(false);
            }
        });
    }

    fn ai_stale(&self, run: u64) -> bool {
        self.ai.run.get() != run
    }

    fn ai_fail(&self, run: u64, why: &str) {
        if self.ai_stale(run) {
            return;
        }
        self.ai_repair_history();
        self.ai.ui.add_error(why);
    }

    /// The key of the provider in use, if it has one.
    async fn ai_key(&self, settings: &AiSettings) -> Option<zeroize::Zeroizing<String>> {
        if settings.provider.can_have_key() {
            crate::keyring::lookup(settings.provider.id()).await
        } else {
            None
        }
    }

    /// One question: ask the model, do what it asks with the tabs, tell it how it went, and
    /// repeat until it answers in words.
    async fn ai_loop(self: &Rc<Self>, run: u64) {
        loop {
            if self.ai_stale(run) {
                return;
            }
            let settings = self.ai.settings.borrow().clone();
            let key = self.ai_key(&settings).await;
            if let Some(why) = settings.not_ready(key.is_some()) {
                self.ai_fail(run, &why);
                return;
            }
            let engine = {
                let s = self.settings.borrow();
                search::engine_or_default(&s.search_engine).name
            };
            let today = glib::DateTime::now_local()
                .ok()
                .and_then(|d| d.format("%A %e de %B de %Y").ok())
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            let system = prompt::system(&settings, &today, engine);
            let tools = policy::specs(&settings);
            let history = {
                let mut h = self.ai.history.borrow_mut();
                wire::trim_results(&mut h, HISTORY_BUDGET);
                h.clone()
            };
            let request = wire::build_request(
                &settings,
                key.as_ref().map_or("", |k| k.as_str()),
                &system,
                &history,
                &tools,
            );
            drop(key);
            let request = match request {
                Ok(r) => r,
                Err(why) => return self.ai_fail(run, &why),
            };
            let (status, body) = match self.ai_post(request).await {
                Ok(r) => r,
                Err(why) => return self.ai_fail(run, &why),
            };
            if self.ai_stale(run) {
                return;
            }
            if !(200..300).contains(&status) {
                return self.ai_fail(run, &wire::describe_error(status, &body));
            }
            let reply = match wire::parse_reply(settings.provider, &body) {
                Ok(r) => r,
                Err(why) => return self.ai_fail(run, &why),
            };
            let mut parts = Vec::new();
            if !reply.text.trim().is_empty() {
                parts.push(Part::Text(reply.text.clone()));
                self.ai.ui.add_assistant(&reply.text);
            }
            parts.extend(reply.calls.iter().cloned().map(Part::Call));
            if parts.is_empty() {
                self.ai.ui.add_note("La IA no respondió nada.");
                return;
            }
            self.ai.history.borrow_mut().push(Message {
                role: Role::Assistant,
                parts,
            });
            if reply.calls.is_empty() {
                return;
            }
            let mut results = Vec::new();
            for call in &reply.calls {
                if self.ai_stale(run) {
                    return;
                }
                results.push(self.ai_tool(call, run).await);
            }
            if self.ai_stale(run) {
                return;
            }
            self.ai.history.borrow_mut().push(Message {
                role: Role::Tool,
                parts: results,
            });
            // The policy refuses calls past the limit; this stops a model that keeps asking.
            if self.ai.session.borrow().steps > policy::MAX_STEPS + 2 {
                self.ai
                    .ui
                    .add_note("La IA llegó al límite de acciones y se detuvo.");
                self.ai_repair_history();
                return;
            }
        }
    }

    /// Sends a request and returns the status and the body.
    async fn ai_post(&self, request: wire::Request) -> Result<(u32, String), String> {
        let session = self
            .ai
            .http
            .borrow_mut()
            .get_or_insert_with(|| {
                let s = soup::Session::new();
                s.set_timeout(120);
                s.set_user_agent("Brujula");
                s
            })
            .clone();
        let message = soup::Message::new("POST", &request.url)
            .map_err(|_| "La dirección del proveedor no es válida".to_owned())?;
        // A redirect could carry the key to another host: the answer must come from where it
        // was asked.
        message.set_flags(soup::MessageFlags::NO_REDIRECT);
        if let Some(h) = message.request_headers() {
            for (k, v) in &request.headers {
                h.replace(k, v);
            }
        }
        message.set_request_body_from_bytes(
            Some("application/json"),
            Some(&glib::Bytes::from_owned(request.body.into_bytes())),
        );
        let bytes = session
            .send_and_read_future(&message, glib::Priority::DEFAULT)
            .await
            .map_err(|e| format!("No se pudo hablar con el proveedor: {}", e.message()))?;
        if bytes.len() > MAX_RESPONSE {
            return Err("La respuesta es demasiado grande".to_owned());
        }
        let status = u32::try_from(message.status().into_glib()).unwrap_or(0);
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// The address and title of a tab, as the page says them now.
    fn ai_tab_info(&self, id: TabId) -> Option<(String, String)> {
        let model = self.model.borrow();
        let tab = model.get(id)?;
        let view = self.views.borrow().get(&id).cloned();
        let url = view
            .as_ref()
            .and_then(|v| v.uri())
            .map_or_else(|| tab.url.clone(), |u| u.to_string());
        let title = view
            .as_ref()
            .and_then(|v| v.title())
            .map_or_else(|| tab.title.clone(), |t| t.to_string());
        Some((url, title))
    }

    async fn ai_wait_loaded(&self, view: &WebView) {
        glib::timeout_future(Duration::from_millis(350)).await;
        for _ in 0..80 {
            if !view.is_loading() {
                break;
            }
            glib::timeout_future(Duration::from_millis(150)).await;
        }
        glib::timeout_future(Duration::from_millis(250)).await;
    }

    /// Carries out a call of the model, after the policy says it may, and says how it went.
    async fn ai_tool(self: &Rc<Self>, call: &ToolCall, run: u64) -> Part {
        let outcome = self.ai_tool_inner(call, run).await;
        let (content, error) = match outcome {
            Ok(text) => (text, false),
            Err(text) => (text, true),
        };
        Part::Result {
            id: call.id.clone(),
            name: call.name.clone(),
            content: clip_keep_newlines(&content, MAX_RESULT),
            error,
        }
    }

    async fn ai_tool_inner(self: &Rc<Self>, call: &ToolCall, run: u64) -> Result<String, String> {
        let settings = self.ai.settings.borrow().clone();
        let args = &call.args;
        let tab = self.ai_tab_of(call);
        let info = tab.and_then(|id| self.ai_tab_info(id));
        self.ai_gate(&settings, call, tab, info.as_ref(), run)
            .await?;
        self.ai.session.borrow_mut().steps += 1;
        match call.name.as_str() {
            policy::LIST_TABS => Ok(self.ai_list_tabs(&settings)),
            policy::READ_PAGE => self.ai_read(&settings, tab, run).await,
            policy::SEARCH => {
                let query = clip(args["query"].as_str().unwrap_or_default(), 200);
                let template = self.settings.borrow().search_template().to_owned();
                let url = omnibox::search_url(&query, &template).to_string();
                let id = self.ai_open(&url);
                self.ai
                    .ui
                    .add_note(&format!("Abrió una búsqueda: «{query}»"));
                Ok(format!(
                    "Pestaña {} abierta con los resultados de «{query}». Léela con read_page.",
                    id.0
                ))
            }
            policy::OPEN_TAB => {
                let url = policy::openable(args["url"].as_str().unwrap_or_default())?.to_string();
                let id = self.ai_open(&url);
                self.ai.ui.add_note(&format!(
                    "Abrió {}",
                    clip(&policy::host_of(&url).unwrap_or(url.clone()), 60)
                ));
                Ok(format!("Pestaña {} abierta.", id.0))
            }
            policy::SWITCH_TAB => {
                let id = tab_arg(args).ok_or("Falta tab_id")?;
                let changes = self.model.borrow_mut().select(id);
                self.apply(changes);
                Ok("Hecho.".to_owned())
            }
            policy::CLOSE_TAB => {
                let id = tab_arg(args).ok_or("Falta tab_id")?;
                self.ai.session.borrow_mut().opened.remove(&id.0);
                self.ai.snapshots.borrow_mut().remove(&id);
                self.close_tab(id);
                self.ai.ui.add_note("Cerró una pestaña que había abierto.");
                Ok("Hecho.".to_owned())
            }
            policy::CLICK | policy::TYPE_TEXT => self.ai_act(call, tab, run).await,
            _ => Err("Herramienta desconocida".to_owned()),
        }
    }

    /// The tab a call is about: the one it names, or the one in front for reading and acting.
    fn ai_tab_of(&self, call: &ToolCall) -> Option<TabId> {
        let needs_tab = matches!(
            call.name.as_str(),
            policy::READ_PAGE | policy::CLICK | policy::TYPE_TEXT
        );
        match call.args["tab_id"].as_u64() {
            Some(id) => Some(TabId(id)),
            None if needs_tab => self.model.borrow().selected(),
            None => None,
        }
    }

    /// Asks the policy whether `call` may go ahead and, if it says to ask, asks the user. `Ok`
    /// means go ahead; the error says why not, and goes back to the model.
    async fn ai_gate(
        self: &Rc<Self>,
        settings: &AiSettings,
        call: &ToolCall,
        tab: Option<TabId>,
        info: Option<&(String, String)>,
        run: u64,
    ) -> Result<(), String> {
        let element = match (call.name.as_str(), tab, call.args["element"].as_u64()) {
            (policy::CLICK | policy::TYPE_TEXT, Some(id), Some(n)) => {
                self.ai.snapshots.borrow().get(&id).and_then(|s| {
                    s.element(u32::try_from(n).unwrap_or(0))
                        .map(page::Element::describe)
                })
            }
            _ => None,
        };
        let target = match (info, tab) {
            (Some((url, title)), Some(id)) => Some(Target {
                id: id.0,
                url,
                title,
                element: element.as_deref(),
            }),
            _ => None,
        };
        let decision = policy::decide(
            settings,
            &self.ai.session.borrow(),
            &Call {
                name: &call.name,
                args: &call.args,
            },
            target,
        );
        let (title, detail, always) = match decision {
            Decision::Allow => return Ok(()),
            Decision::Deny(why) => {
                self.ai
                    .ui
                    .add_note(&format!("Se impidió: {}", short_call(call, &why)));
                return Err(why);
            }
            Decision::Ask {
                title,
                detail,
                always,
            } => (title, detail, always),
        };
        let answer = self.ai.ui.ask(&title, &detail, always.is_some()).await;
        if self.ai_stale(run) {
            return Err("Cancelado".to_owned());
        }
        match answer {
            Answer::No => return Err("El usuario denegó esta acción.".to_owned()),
            Answer::Always if always == Some(Grant::Open) => {
                self.ai.session.borrow_mut().open_ok = true;
            }
            Answer::Always | Answer::Yes => {}
        }
        // A yes to reading a site covers the site for the rest of the conversation.
        if call.name == policy::READ_PAGE {
            if let Some(h) = info.and_then(|(u, _)| policy::host_of(u)) {
                self.ai.session.borrow_mut().read_hosts.insert(h);
            }
        }
        Ok(())
    }

    /// Opens a tab for the assistant, behind the one in front, and remembers it is its own.
    fn ai_open(self: &Rc<Self>, url: &str) -> TabId {
        let after = self.model.borrow().selected();
        let id = self.open_tab(url, after, false);
        let mut s = self.ai.session.borrow_mut();
        s.opened.insert(id.0);
        s.known_urls.insert(url.to_owned());
        id
    }

    fn ai_list_tabs(&self, settings: &AiSettings) -> String {
        let selected = self.model.borrow().selected();
        let tabs: Vec<serde_json::Value> = self
            .model
            .borrow()
            .tabs()
            .iter()
            .filter(|t| policy::visible(settings, &t.url).is_ok())
            .take(60)
            .map(|t| {
                json!({
                    "id": t.id.0,
                    "title": clip(&t.title, 120),
                    "url": clip(&t.url, 300),
                    "active": Some(t.id) == selected,
                })
            })
            .collect();
        // Titles are the pages' words too.
        self.ai.session.borrow_mut().tainted = true;
        self.ai
            .ui
            .add_note(&format!("Vio la lista de pestañas ({}).", tabs.len()));
        serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".to_owned())
    }

    async fn ai_read(
        self: &Rc<Self>,
        settings: &AiSettings,
        tab: Option<TabId>,
        run: u64,
    ) -> Result<String, String> {
        let id = tab.ok_or("No existe esa pestaña")?;
        let view = self
            .views
            .borrow()
            .get(&id)
            .cloned()
            .ok_or("No existe esa pestaña")?;
        if self.lazy.borrow().contains_key(&id) {
            return Err(
                "Esa pestaña aún no se ha cargado: ponla delante con switch_tab".to_owned(),
            );
        }
        self.ai_wait_loaded(&view).await;
        if self.ai_stale(run) {
            return Err("Cancelado".to_owned());
        }
        let script = page::snapshot_script(settings.interact);
        let value = view
            .evaluate_javascript_future(&script, Some("brujula-ai"), None)
            .await
            .map_err(|_| "Esta página no se puede leer".to_owned())?;
        let snapshot = page::parse_snapshot(&value.to_str())?;
        // The page may have moved on (a redirect) to somewhere the assistant must not see.
        policy::visible(settings, &snapshot.url)?;
        let sent = snapshot.sent_chars();
        {
            let mut s = self.ai.session.borrow_mut();
            s.tainted = true;
            s.known_urls.extend(snapshot.link_urls());
        }
        self.ai.ui.add_note(&format!(
            "Leyó «{}» ({}): {} caracteres{}.",
            clip(&snapshot.title, 60),
            policy::host_of(&snapshot.url).unwrap_or_default(),
            sent,
            if settings.is_remote() {
                format!(" enviados a {}", settings.provider.short())
            } else {
                String::new()
            }
        ));
        let text = snapshot.for_model();
        self.ai.snapshots.borrow_mut().insert(id, snapshot);
        Ok(text)
    }

    async fn ai_act(
        self: &Rc<Self>,
        call: &ToolCall,
        tab: Option<TabId>,
        run: u64,
    ) -> Result<String, String> {
        let id = tab.ok_or("No existe esa pestaña")?;
        let view = self
            .views
            .borrow()
            .get(&id)
            .cloned()
            .ok_or("No existe esa pestaña")?;
        let n = u32::try_from(call.args["element"].as_u64().unwrap_or(0)).unwrap_or(0);
        let (gen, label, kind) = {
            let snaps = self.ai.snapshots.borrow();
            let snap = snaps
                .get(&id)
                .ok_or("Lee la página primero con read_page")?;
            let e = snap
                .element(n)
                .ok_or("Ese elemento no existe: lee la página otra vez con read_page")?;
            (snap.gen.clone(), e.label.clone(), e.describe())
        };
        let script = if call.name == policy::CLICK {
            page::click_script(n, &gen, &label)
        } else {
            page::type_script(
                n,
                &gen,
                &label,
                call.args["text"].as_str().unwrap_or_default(),
                call.args["submit"].as_bool() == Some(true),
            )
        };
        let value = view
            .evaluate_javascript_future(&script, Some("brujula-ai"), None)
            .await
            .map_err(|_| "No se pudo actuar en la página".to_owned())?;
        page::parse_action(&value.to_str())?;
        // A click (or Enter in a field) can change the page: what was read is out of date. Typing
        // alone does not, so the next field can be filled without reading again.
        if call.name == policy::CLICK || call.args["submit"].as_bool() == Some(true) {
            self.ai.snapshots.borrow_mut().remove(&id);
        }
        self.ai.ui.add_note(&format!(
            "{} {kind}.",
            if call.name == policy::CLICK {
                "Pulsó"
            } else {
                "Escribió en"
            }
        ));
        self.ai_wait_loaded(&view).await;
        if self.ai_stale(run) {
            return Err("Cancelado".to_owned());
        }
        Ok("Hecho. La página pudo cambiar: léela otra vez si hace falta.".to_owned())
    }

    // ---- The preferences page ----

    /// What the preferences page shows about the assistant.
    pub(super) fn ai_view(&self) -> prefs::AiView {
        prefs::AiView {
            settings: self.ai.settings.borrow().clone(),
            key_set: self.ai.key_set.get(),
            status: self.ai.status.borrow().clone(),
        }
    }

    /// Finds out whether the keyring has a key for the provider in use. Only while the assistant
    /// is on: the keyring is not touched otherwise.
    fn ai_refresh_key(self: &Rc<Self>, redraw: bool) {
        if !self.ai_enabled() {
            self.ai.key_set.set(false);
            return;
        }
        let provider = self.ai.settings.borrow().provider;
        if !provider.can_have_key() {
            self.ai.key_set.set(false);
            return;
        }
        let this = self.clone();
        glib::spawn_future_local(async move {
            let has = crate::keyring::lookup(provider.id()).await.is_some();
            // The provider may have changed while the keyring answered.
            if this.ai.settings.borrow().provider != provider {
                return;
            }
            let changed = this.ai.key_set.replace(has) != has;
            if redraw || changed {
                this.redraw_settings();
            }
        });
    }

    /// A change on the preferences page (`ai.<what>`).
    pub(super) fn change_ai(self: &Rc<Self>, what: &str, value: &str) {
        if self.private {
            return;
        }
        match what {
            "key.set" => self.ai_ask_key(),
            "key.clear" => {
                let provider = self.ai.settings.borrow().provider;
                let this = self.clone();
                glib::spawn_future_local(async move {
                    let _ = crate::keyring::clear(provider.id()).await;
                    this.ai.key_set.set(false);
                    this.redraw_settings();
                });
            }
            "test" => self.ai_test(),
            _ => {
                let result = self.ai.settings.borrow_mut().apply(what, value);
                if let Err(why) = result {
                    eprintln!("brujula: setting refused: {why}");
                    return;
                }
                self.ai_save();
                self.ai.status.borrow_mut().clear();
                self.ai_refresh_ui();
                // The page changes shape with these, and the key belongs to the provider.
                if matches!(what, "enabled" | "provider") {
                    self.ai.key_set.set(false);
                    self.ai_refresh_key(false);
                    self.redraw_settings();
                } else if what == "base_url" || what == "model" {
                    self.redraw_settings();
                }
            }
        }
    }

    fn ai_ask_key(self: &Rc<Self>) {
        let provider = self.ai.settings.borrow().provider;
        if !provider.can_have_key() {
            return;
        }
        let this = self.clone();
        dialogs::ask_secret(
            &self.window,
            &format!("Clave de {}", provider.short()),
            "Pega aquí tu clave de API. Se guarda en el llavero del sistema, no en ningún archivo de Brújula, y solo se envía a este proveedor.",
            "Guardar",
            move |text| {
                let Some(key) = crate::keyring::clean_key(&text) else {
                    if !text.trim().is_empty() {
                        dialogs::notice(
                            &this.window,
                            "La clave no es válida",
                            "Una clave de API es una sola palabra, sin espacios.",
                        );
                    }
                    return;
                };
                let this = this.clone();
                glib::spawn_future_local(async move {
                    // ASCII: `secret-tool` fails on non-ASCII text in a locale that is not UTF-8.
                    let label = format!("Brujula - clave de API ({})", provider.short());
                    match crate::keyring::store(provider.id(), &label, &key).await {
                        Ok(()) => {
                            this.ai.key_set.set(true);
                            *this.ai.status.borrow_mut() = String::new();
                        }
                        Err(why) => dialogs::notice(&this.window, "No se guardó la clave", &why),
                    }
                    drop(key);
                    this.redraw_settings();
                });
            },
        );
    }

    /// «Probar conexión»: one small question with no tools, to see the key and the model work.
    fn ai_test(self: &Rc<Self>) {
        if !self.ai_enabled() {
            return;
        }
        // Said in place: drawing the page twice in a row loses where it was scrolled to.
        self.ai_say_in_page("Probando…");
        let this = self.clone();
        glib::spawn_future_local(async move {
            let settings = this.ai.settings.borrow().clone();
            let key = this.ai_key(&settings).await;
            let outcome = async {
                if let Some(why) = settings.not_ready(key.is_some()) {
                    return Err(why);
                }
                let request = wire::build_request(
                    &settings,
                    key.as_ref().map_or("", |k| k.as_str()),
                    "Responde con una sola palabra.",
                    &[Message::user("Responde: ok")],
                    &[],
                )?;
                let (status, body) = this.ai_post(request).await?;
                if !(200..300).contains(&status) {
                    return Err(wire::describe_error(status, &body));
                }
                wire::parse_reply(settings.provider, &body).map(|r| clip(r.text.trim(), 40))
            }
            .await;
            drop(key);
            *this.ai.status.borrow_mut() = match outcome {
                Ok(text) => format!("Funciona: {} respondió «{text}».", settings.model()),
                Err(why) => format!("No funciona: {why}"),
            };
            this.redraw_settings();
        });
    }

    /// Changes the line under «Probar la conexión» on the preferences page without drawing it again.
    fn ai_say_in_page(&self, text: &str) {
        let view = self
            .views
            .borrow()
            .values()
            .find(|v| v.uri().as_deref() == Some(prefs::SETTINGS_URI))
            .cloned();
        let Some(view) = view else { return };
        let literal = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned());
        let script = format!(
            "(function(t){{const p=document.querySelector('[data-k=\"ai.test\"]')?.closest('.row')?.querySelector('.what p');if(p)p.textContent=t}})({literal})"
        );
        view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }

    /// The row the Spotlight adds when the search box belongs to the assistant.
    pub(super) fn ai_search_row(&self, text: &str) -> Option<Item> {
        let s = self.ai.settings.borrow();
        if !self.ai_enabled() || !s.search || text.trim().is_empty() {
            return None;
        }
        if omnibox::looks_like_address(text) {
            return None;
        }
        Some(Item {
            action: Action::Ask(text.trim().to_owned()),
            title: format!("Preguntar a {}: {}", s.provider.short(), text.trim()),
            detail: "Responde o hace la tarea con tus pestañas · ↓ para buscar de forma normal"
                .to_owned(),
            icon: "ndl-sparkle-symbolic",
            texture: None,
        })
    }
}

fn tab_arg(args: &serde_json::Value) -> Option<TabId> {
    args["tab_id"].as_u64().map(TabId)
}

fn short_call(call: &ToolCall, why: &str) -> String {
    format!("{} ({})", call.name, clip(why, 100))
}

fn clip_keep_newlines(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max).collect::<String>())
    } else {
        text.to_owned()
    }
}
