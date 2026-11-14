//! The optional AI assistant. Off until the user turns it on in the preferences; while it is
//! off Brújula sends nothing to any AI, shows no button and does not touch the keyring.
//!
//! The user picks the provider and brings their own key, which lives in the system keyring
//! (`keyring.rs`). The assistant answers in a panel beside the page and, when it needs to, uses
//! tools on the tabs. Every tool call goes through `brujula_core::ai::policy::decide`, which
//! says whether it may go ahead, must be asked first or is refused; this file only carries out
//! what that allows. Everything the assistant is shown or does is written in the panel.

use std::time::Duration;

use super::ai_pane::TaskAct;
use super::ai_panel::{AiUi, Answer};
use super::*;

use brujula_core::ai::organize::{self, Op};
use brujula_core::ai::policy::{self, Call, Decision, Grant, Target};
use brujula_core::ai::tasks::{self, Task, Tasks};
use brujula_core::ai::wire::{self, Message, Part, Role, ToolCall};
use brujula_core::ai::{page, prompt, AiSettings, Provider};
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
    /// The scheduled tasks of this profile.
    tasks: RefCell<Tasks>,
    /// The task that is running now, if any.
    running: RefCell<Option<Task>>,
    /// How the running task ended: what it answered, or why it could not.
    outcome: RefCell<Option<Result<String, String>>>,
    /// Tabs the running task opened, so they are closed when it ends.
    task_tabs: RefCell<Vec<TabId>>,
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
            tasks: RefCell::new(if private {
                Tasks::default()
            } else {
                load_tasks()
            }),
            running: RefCell::new(None),
            outcome: RefCell::new(None),
            task_tabs: RefCell::new(Vec::new()),
        }
    }
}

fn load_tasks() -> Tasks {
    std::fs::read_to_string(paths::ai_tasks_file())
        .map(|t| Tasks::from_json(&t))
        .unwrap_or_default()
}

/// Now, in seconds since 1970, and how far local time is from UTC, in seconds.
fn clock() -> (u64, i64) {
    let now = u64::try_from(glib::real_time() / 1_000_000).unwrap_or(0);
    let offset = glib::DateTime::now_local()
        .map(|d| d.utc_offset().as_seconds())
        .unwrap_or(0);
    (now, offset)
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
            let open = !this.ai.ui.settings_open();
            this.ai_sync_pane();
            this.ai.ui.show_settings(open);
        });
        self.wire_ai_pane();
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
        // Tasks are looked at once a minute while the browser is open.
        let weak = Rc::downgrade(self);
        glib::timeout_add_seconds_local(60, move || match weak.upgrade() {
            Some(this) => {
                this.ai_tick();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
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
                "Puede ver los títulos y direcciones de tus pestañas y leer la página que miras; otras, solo si le dejas. Busca y abre pestañas por su cuenta. ",
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
        drop(s);
        self.ai_sync_pane();
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
        // Stopping a task ends it: what comes next is the user's own conversation.
        *self.ai.running.borrow_mut() = None;
        self.ai.session.borrow_mut().task_hosts = None;
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
        *self.ai.running.borrow_mut() = None;
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
        self.ai_end(Err(why.to_owned()));
    }

    /// Writes down how the running task ended (nothing, for a conversation).
    fn ai_end(&self, result: Result<String, String>) {
        if self.ai.running.borrow().is_some() {
            *self.ai.outcome.borrow_mut() = Some(result);
        }
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
            let (system, tools) = match self.ai.running.borrow().as_ref() {
                Some(t) => (
                    prompt::task_system(&t.name, &t.prompt, &t.hosts, &today),
                    policy::task_specs(),
                ),
                None => (
                    prompt::system(&settings, &today, engine),
                    policy::specs(&settings),
                ),
            };
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
                self.ai_end(Err("La IA no respondió nada".to_owned()));
                return;
            }
            self.ai.history.borrow_mut().push(Message {
                role: Role::Assistant,
                parts,
            });
            if reply.calls.is_empty() {
                self.ai_end(Ok(reply.text.clone()));
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
                self.ai_end(Err("Llegó al límite de acciones".to_owned()));
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
            policy::ORGANIZE => self.ai_organize(&settings, args),
            policy::SCHEDULE => self.ai_schedule(args),
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
                active: self.model.borrow().selected() == Some(id),
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
        let detail = if call.name == policy::ORGANIZE {
            self.ai_plan_text(&call.args)
        } else {
            detail
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

    /// The title of a tab, for showing a plan.
    fn ai_title_of(&self, id: u64) -> String {
        self.model.borrow().get(TabId(id)).map_or_else(
            || format!("pestaña {id}"),
            |t| {
                // A page with no title yet is known by its address.
                if t.title.trim().is_empty() {
                    clip(&t.url, 60)
                } else {
                    clip(&t.title, 60)
                }
            },
        )
    }

    /// A plan to tidy the tabs, in words: what the user is asked to allow.
    fn ai_plan_text(&self, args: &serde_json::Value) -> String {
        match organize::parse(args) {
            Ok(plan) => plan.describe(|id| self.ai_title_of(id)).join("\n"),
            Err(_) => String::new(),
        }
    }

    /// Carries out a plan to tidy the tabs. Only tabs the assistant may see, in the order given;
    /// a step that cannot be done is skipped and reported, and the rest go ahead.
    fn ai_organize(
        self: &Rc<Self>,
        settings: &AiSettings,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let plan = organize::parse(args)?;
        let usable = |id: u64| -> Result<TabId, String> {
            let m = self.model.borrow();
            let t = m
                .get(TabId(id))
                .ok_or_else(|| format!("la pestaña {id} no existe"))?;
            if t.essential.is_some() {
                return Err(format!("la pestaña {id} es un esencial"));
            }
            policy::visible(settings, &t.url)
                .map_err(|_| format!("la pestaña {id} no se puede tocar"))?;
            Ok(t.id)
        };
        let (mut done, mut skipped) = (0usize, Vec::new());
        for op in &plan.ops {
            let result: Result<(), String> = match op {
                Op::Pin(id) | Op::Unpin(id) => usable(*id).map(|id| {
                    let pin = matches!(op, Op::Pin(_));
                    let c = self.model.borrow_mut().set_pinned(id, pin);
                    self.apply(c);
                }),
                Op::Close(id) => usable(*id).map(|id| {
                    self.ai.session.borrow_mut().opened.remove(&id.0);
                    self.ai.snapshots.borrow_mut().remove(&id);
                    self.close_tab(id);
                }),
                Op::Move {
                    tab,
                    next_to,
                    after,
                } => usable(*tab).and_then(|id| {
                    let other = usable(*next_to)?;
                    let (pos, at) = {
                        let m = self.model.borrow();
                        (m.position(id), m.position(other))
                    };
                    let (Some(pos), Some(at)) = (pos, at) else {
                        return Err("la pestaña ya no está".to_owned());
                    };
                    let to = match (pos < at, *after) {
                        (true, false) => at - 1,
                        (true, true) => at,
                        (false, false) => at,
                        (false, true) => at + 1,
                    };
                    let c = self.model.borrow_mut().move_to(id, to);
                    self.apply(c);
                    self.reorder_rows();
                    Ok(())
                }),
                Op::Folder { name, tabs } => self.ai_file_tabs(&usable, name, tabs),
                Op::Space { tab, space } => usable(*tab).and_then(|id| {
                    let target = self
                        .spaces
                        .borrow()
                        .all()
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(space))
                        .map(|s| s.id)
                        .ok_or_else(|| format!("no hay un espacio «{space}»"))?;
                    self.move_tab_to_space(id, target);
                    Ok(())
                }),
            };
            match result {
                Ok(()) => done += 1,
                Err(why) => skipped.push(why),
            }
        }
        self.reorder_rows();
        let closed = plan.closes().len();
        self.ai.ui.add_note(&format!(
            "Organizó las pestañas: {done} de {} pasos{}.",
            plan.ops.len(),
            if closed > 0 {
                format!(" ({closed} cerradas; Ctrl+Mayús+T las recupera)")
            } else {
                String::new()
            }
        ));
        if skipped.is_empty() {
            Ok(format!("Hecho: {done} pasos."))
        } else {
            Ok(format!(
                "Hecho: {done} pasos. No se pudo: {}.",
                skipped.join("; ")
            ))
        }
    }

    /// Puts tabs in the folder with this name (made in the space of the first tab if there is
    /// none). Tabs of another space stay out: a folder belongs to one space.
    fn ai_file_tabs(
        self: &Rc<Self>,
        usable: &dyn Fn(u64) -> Result<TabId, String>,
        name: &str,
        tabs: &[u64],
    ) -> Result<(), String> {
        let ids: Vec<TabId> = tabs.iter().filter_map(|t| usable(*t).ok()).collect();
        let first = *ids
            .first()
            .ok_or("ninguna de las pestañas se puede tocar")?;
        let space = self
            .model
            .borrow()
            .get(first)
            .map(|t| t.space)
            .ok_or("la pestaña ya no está")?;
        let existing = self
            .folders
            .borrow()
            .in_space(space)
            .find(|f| {
                f.name.eq_ignore_ascii_case(name) && f.rule == brujula_core::folders::Rule::Manual
            })
            .map(|f| f.id);
        let folder = match existing {
            Some(f) => f,
            None => {
                let made =
                    self.folders
                        .borrow_mut()
                        .add(space, name, brujula_core::folders::Rule::Manual);
                let made = made.ok_or("ya hay demasiadas carpetas")?;
                self.rebuild_folder_rows();
                made
            }
        };
        for id in ids {
            let same = self
                .model
                .borrow()
                .get(id)
                .is_some_and(|t| t.space == space);
            if same {
                self.put_tab_in_folder(id, Some(folder));
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
        if s.task_hosts.is_some() {
            self.ai.task_tabs.borrow_mut().push(id);
        }
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
                let folder = self
                    .folders
                    .borrow()
                    .folder_of(t.folder, t.space, &t.url, &t.title)
                    .and_then(|f| self.folders.borrow().get(f).map(|f| clip(&f.name, 40)));
                let space = self.spaces.borrow().get(t.space).map(|s| clip(&s.name, 40));
                json!({
                    "id": t.id.0,
                    "title": clip(&t.title, 120),
                    "url": clip(&t.url, 300),
                    "active": Some(t.id) == selected,
                    "pinned": t.pinned,
                    "folder": folder,
                    "space": space,
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
        if let Some(hosts) = &self.ai.session.borrow().task_hosts {
            let host = policy::host_of(&snapshot.url).unwrap_or_default();
            if !policy::in_task_hosts(hosts, &host) {
                return Err(format!(
                    "La página acabó en {host}, que no es de los sitios de la tarea"
                ));
            }
        }
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
    /// Reads the tasks from the disk, changes them and writes them back if they changed: several
    /// windows share the file, so none works from a copy that has gone stale.
    fn tasks_edit<R>(&self, f: impl FnOnce(&mut Tasks) -> R) -> R {
        let mut fresh = load_tasks();
        let before = fresh.to_json();
        let r = f(&mut fresh);
        if !self.private && fresh.to_json() != before {
            let _ = paths::write_atomic(&paths::ai_tasks_file(), &fresh.to_json());
        }
        *self.ai.tasks.borrow_mut() = fresh;
        r
    }

    /// The model proposed a task and the user said yes: keep it.
    fn ai_schedule(self: &Rc<Self>, args: &serde_json::Value) -> Result<String, String> {
        let (now, _) = clock();
        let task = tasks::parse(args, now)?;
        let when = task.schedule.describe();
        let id = self.tasks_edit(|t| t.add(task)).ok_or_else(|| {
            format!(
                "Ya hay {} tareas: el usuario tiene que borrar alguna",
                tasks::MAX_TASKS
            )
        })?;
        self.ai_sync_pane();
        self.ai.ui.add_note("Guardó una tarea programada.");
        Ok(format!(
            "Tarea guardada (nº {id}): {when}. Se ejecuta con Brújula abierto."
        ))
    }

    /// Once a minute: runs the task whose time has come, if the assistant has nothing else to do.
    /// A task never interrupts a conversation, nor takes over one that is open.
    fn ai_tick(self: &Rc<Self>) {
        if !self.ai_enabled() || self.ai.busy.get() || !self.ai.history.borrow().is_empty() {
            return;
        }
        {
            let s = self.ai.settings.borrow();
            if !(s.read && s.tasks) {
                return;
            }
        }
        let (now, offset) = clock();
        let due = self.tasks_edit(|t| {
            let id = t.due(now, offset)?;
            t.start(id, now);
            t.get(id).cloned()
        });
        if let Some(task) = due {
            let this = self.clone();
            glib::spawn_future_local(async move { this.ai_run_task(task).await });
        }
    }

    /// Runs a task: the assistant reads its sites with nothing else of the browser, writes what it
    /// found, and the tabs it opened are closed. Whatever happens is kept with the task.
    async fn ai_run_task(self: &Rc<Self>, task: Task) {
        let settings = self.ai.settings.borrow().clone();
        if !settings.read {
            self.ai_show_panel();
            self.ai
                .ui
                .add_note("Para las tareas hace falta que la IA pueda leer páginas.");
            return;
        }
        self.ai_reset();
        self.ai.session.borrow_mut().task_hosts = Some(task.hosts.clone());
        *self.ai.running.borrow_mut() = Some(task.clone());
        *self.ai.outcome.borrow_mut() = None;
        self.ai.task_tabs.borrow_mut().clear();
        self.ai.ui.add_note(&format!(
            "Tarea programada: «{}». Solo abre y lee {}.",
            task.name,
            task.hosts.join(", ")
        ));
        self.ai.ui.add_note(&if settings.is_remote() {
            format!(
                "Se envía a {} ({}).",
                settings.provider.short(),
                settings.model()
            )
        } else {
            format!("Se queda en este equipo ({}).", settings.model())
        });
        self.ai
            .history
            .borrow_mut()
            .push(Message::user("Haz la tarea ahora."));
        self.ai.busy.set(true);
        self.ai.ui.set_busy(true);
        let run = self.ai.run.get();
        self.ai_loop(run).await;
        let stopped = self.ai_stale(run);
        let result = self
            .ai
            .outcome
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Err("Se interrumpió".to_owned()));
        let tabs: Vec<TabId> = self.ai.task_tabs.borrow_mut().drain(..).collect();
        for id in tabs {
            if self.model.borrow().get(id).is_some() {
                self.close_tab(id);
            }
        }
        let (text, ok) = match result {
            Ok(t) if !t.trim().is_empty() => (clip(t.trim(), 4000), true),
            Ok(_) => ("La IA no escribió nada.".to_owned(), false),
            Err(why) => (format!("No se pudo hacer: {why}"), false),
        };
        let (now, _) = clock();
        self.tasks_edit(|t| t.record(task.id, now, &text));
        if stopped {
            // The user stopped it or started something else: what is on screen is theirs.
            self.ai_sync_pane();
            return;
        }
        let watching = self.ai.ui.panel.is_visible();
        self.ai_reset();
        if watching {
            self.ai
                .ui
                .add_note(&format!("Tarea «{}» terminada.", task.name));
            if ok {
                self.ai.ui.add_assistant(&text);
            } else {
                self.ai.ui.add_error(&text);
            }
        } else {
            self.ai_notify_task(&task, &text, ok);
        }
        self.ai_sync_pane();
    }

    /// Tells the user, outside the browser, what a task found.
    fn ai_notify_task(&self, task: &Task, text: &str, ok: bool) {
        let Some(app) = self.window.application() else {
            return;
        };
        let n = gio::Notification::new(&if ok {
            task.name.clone()
        } else {
            format!("{} no se pudo hacer", task.name)
        });
        n.set_body(Some(&clip(text, 240)));
        n.set_priority(gio::NotificationPriority::Normal);
        app.send_notification(Some(&format!("brujula-task-{}", task.id)), &n);
    }

    /// What the user did to a task in the list of the panel.
    fn ai_task_act(self: &Rc<Self>, id: u32, act: TaskAct) {
        match act {
            TaskAct::Enable(on) => {
                self.tasks_edit(|t| t.set_enabled(id, on));
                return;
            }
            TaskAct::Delete => {
                self.tasks_edit(|t| t.remove(id));
            }
            TaskAct::Run => {
                if self.ai.busy.get() {
                    self.ai
                        .ui
                        .add_note("Espera a que termine lo anterior, o pulsa Detener.");
                    self.ai.ui.show_settings(false);
                    return;
                }
                let task = self.tasks_edit(|t| t.get(id).cloned());
                if let Some(task) = task {
                    self.ai.ui.show_settings(false);
                    let this = self.clone();
                    glib::spawn_future_local(async move { this.ai_run_task(task).await });
                }
                return;
            }
        }
        self.ai_sync_pane();
    }

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
                this.ai_redraw();
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
                    this.ai_redraw();
                });
            }
            "test" => self.ai_test(),
            _ => {
                let result = self.ai.settings.borrow_mut().apply(what, value);
                if let Err(why) = result {
                    eprintln!("brujula: setting refused: {why}");
                    // Said in the panel too, or the entry would go back to the old value silently.
                    *self.ai.status.borrow_mut() = format!("No se guardó: {why}");
                    self.ai_sync_pane();
                    return;
                }
                self.ai_save();
                self.ai.status.borrow_mut().clear();
                self.ai_refresh_ui();
                // The page changes shape with these, and the key belongs to the provider.
                if matches!(what, "enabled" | "provider") {
                    self.ai.key_set.set(false);
                    self.ai_refresh_key(false);
                    self.ai_redraw();
                } else if what == "base_url" || what == "model" {
                    self.ai_redraw();
                }
            }
        }
    }

    /// Draws the preferences page again and puts the panel's own settings in line with it.
    fn ai_redraw(self: &Rc<Self>) {
        self.redraw_settings();
        self.ai_sync_pane();
    }

    fn ai_sync_pane(&self) {
        self.ai.ui.pane.sync(&self.ai_view());
        self.ai.ui.pane.sync_tasks(self.ai.tasks.borrow().all());
    }

    /// Connects the settings inside the panel to the same changes the preferences page makes.
    fn wire_ai_pane(self: &Rc<Self>) {
        let pane = &self.ai.ui.pane;
        let this = self.clone();
        pane.back
            .connect_clicked(move |_| this.ai.ui.show_settings(false));

        let this = self.clone();
        pane.provider.connect_selected_notify(move |d| {
            if this.ai.ui.pane.syncing() {
                return;
            }
            if let Some(p) = Provider::ALL.get(d.selected() as usize) {
                if *p != this.ai.settings.borrow().provider {
                    this.change_ai("provider", p.id());
                }
            }
        });

        // An entry keeps what is typed when Enter is pressed or when it loses the focus.
        let text_entry = |entry: &gtk::Entry, key: &'static str| {
            let commit = {
                let this = self.clone();
                let entry = entry.clone();
                Rc::new(move || {
                    if this.ai.ui.pane.syncing() {
                        return;
                    }
                    let text = entry.text().trim().to_owned();
                    let same = {
                        let s = this.ai.settings.borrow();
                        match key {
                            "model" => s.model == text,
                            _ => s.base_url == text,
                        }
                    };
                    if !same {
                        this.change_ai(key, &text);
                    }
                })
            };
            let c = commit.clone();
            entry.connect_activate(move |_| c());
            let focus = gtk::EventControllerFocus::new();
            focus.connect_leave(move |_| commit());
            entry.add_controller(focus);
        };
        text_entry(&pane.model, "model");
        text_entry(&pane.address, "base_url");

        let this = self.clone();
        pane.key_set
            .connect_clicked(move |_| this.change_ai("key.set", ""));
        let this = self.clone();
        pane.key_clear
            .connect_clicked(move |_| this.change_ai("key.clear", ""));
        let this = self.clone();
        pane.test
            .connect_clicked(move |_| this.change_ai("test", ""));

        let switch = |sw: &gtk::Switch, key: &'static str, on: &'static str, off: &'static str| {
            let this = self.clone();
            sw.connect_active_notify(move |sw| {
                if !this.ai.ui.pane.syncing() {
                    this.change_ai(key, if sw.is_active() { on } else { off });
                }
            });
        };
        switch(&pane.read, "read", "1", "0");
        switch(&pane.confirm, "confirm_reads", "1", "0");
        switch(&pane.open_auto, "open", "auto", "ask");
        switch(&pane.interact, "interact", "1", "0");
        switch(&pane.organize, "organize", "1", "0");
        switch(&pane.tasks, "tasks", "1", "0");
        switch(&pane.search, "search", "1", "0");

        let add = {
            let this = self.clone();
            Rc::new(move || {
                let entry = &this.ai.ui.pane.blocked_entry;
                let text = entry.text().trim().to_owned();
                if text.is_empty() {
                    return;
                }
                entry.set_text("");
                this.change_ai("blocked.add", &text);
            })
        };
        let a = add.clone();
        pane.blocked_entry.connect_activate(move |_| a());
        pane.blocked_add.connect_clicked(move |_| add());
        let this = self.clone();
        pane.set_on_remove(move |host| this.change_ai("blocked.remove", host));
        let this = self.clone();
        pane.set_on_task(move |id, act| this.ai_task_act(id, act));

        let this = self.clone();
        pane.off
            .connect_clicked(move |_| this.change_ai("enabled", "0"));
        let this = self.clone();
        pane.all.connect_clicked(move |_| {
            // The page needs the room the panel takes.
            this.ai_hide_panel();
            this.open_settings(Some("ai"));
        });
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
                    this.ai_redraw();
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
            this.ai_redraw();
        });
    }

    /// Changes the line under «Probar la conexión» on the preferences page without drawing it again.
    fn ai_say_in_page(&self, text: &str) {
        self.ai.ui.pane.set_status(text);
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
