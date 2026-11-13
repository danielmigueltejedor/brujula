//! The assistant's panel: a column beside the page with the conversation, what the assistant is
//! allowed to see, the questions it asks before it acts, and a line to type in. It holds no logic
//! of its own: `ai.rs` drives it.

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::oneshot;

use super::*;

/// What the user answered to a question of the assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    Yes,
    /// Yes, and the same from now on in this conversation.
    Always,
    No,
}

type Waiting = Rc<RefCell<Option<oneshot::Sender<Answer>>>>;
type LinkHandler = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;

pub(super) struct AiUi {
    /// The whole column; hidden until the assistant is opened.
    pub panel: gtk::Box,
    /// Opens the assistant, in the sidebar and in the bar of the horizontal layout. Both hidden
    /// while the assistant is off.
    pub side_button: gtk::Button,
    pub top_button: gtk::Button,
    pub entry: gtk::Entry,
    pub send: gtk::Button,
    pub new_chat: gtk::Button,
    pub close: gtk::Button,
    pub prefs: gtk::Button,
    messages: gtk::Box,
    scroller: gtk::ScrolledWindow,
    sees: gtk::Label,
    who: gtk::Label,
    empty: gtk::Label,
    spinner: gtk::Spinner,
    waiting: Waiting,
    card: RefCell<Option<gtk::Box>>,
    on_link: LinkHandler,
}

fn icon() -> &'static str {
    "ndl-sparkle-symbolic"
}

fn flat_button(icon: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .icon_name(icon)
        .css_classes(["flat"])
        .tooltip_text(tip)
        .build();
    b.set_valign(gtk::Align::Center);
    b
}

impl AiUi {
    pub fn new() -> Self {
        let side_button = gtk::Button::builder()
            .icon_name(icon())
            .css_classes(["flat", "brujula-newfolder"])
            .tooltip_text("Asistente de IA (Ctrl+Mayús+A)")
            .action_name("win.toggle-assistant")
            .visible(false)
            .build();
        let top_button = gtk::Button::builder()
            .icon_name(icon())
            .css_classes(["flat"])
            .tooltip_text("Asistente de IA (Ctrl+Mayús+A)")
            .action_name("win.toggle-assistant")
            .visible(false)
            .build();

        let title = gtk::Label::builder()
            .label("Asistente")
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let who = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["brujula-ai-note"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
        titles.set_hexpand(true);
        titles.set_valign(gtk::Align::Center);
        titles.append(&title);
        titles.append(&who);
        let new_chat = flat_button("ndl-plus-symbolic", "Conversación nueva");
        let prefs = flat_button("ndl-settings-symbolic", "Ajustes del asistente");
        let close = flat_button("ndl-close-symbolic", "Cerrar el asistente");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        head.add_css_class("brujula-ai-head");
        head.append(&titles);
        head.append(&new_chat);
        head.append(&prefs);
        head.append(&close);

        let sees = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(42)
            .css_classes(["brujula-ai-sees"])
            .build();

        let messages = gtk::Box::new(gtk::Orientation::Vertical, 8);
        messages.set_margin_start(12);
        messages.set_margin_end(12);
        messages.set_margin_top(4);
        messages.set_margin_bottom(8);
        let empty = gtk::Label::builder()
            .label(
                "Pregunta lo que quieras o pídele una tarea con tus pestañas: «resume esta página», «búscame sitios baratos para viajar a Lisboa».\n\nAntes de abrir, leer o pulsar algo, te lo pregunta.",
            )
            .wrap(true)
            .xalign(0.0)
            .max_width_chars(42)
            .css_classes(["brujula-ai-note"])
            .build();
        messages.append(&empty);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&messages)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .build();

        let entry = gtk::Entry::builder()
            .placeholder_text("Escribe aquí…")
            .hexpand(true)
            .max_length(4000)
            .build();
        let send = gtk::Button::builder()
            .icon_name("ndl-arrow-right-symbolic")
            .css_classes(["suggested-action"])
            .tooltip_text("Enviar")
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let foot = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        foot.set_margin_start(12);
        foot.set_margin_end(12);
        foot.set_margin_bottom(12);
        foot.set_margin_top(4);
        foot.append(&spinner);
        foot.append(&entry);
        foot.append(&send);

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel.add_css_class("ndl-island");
        panel.add_css_class("brujula-ai");
        panel.set_size_request(340, -1);
        // The page takes what is left: the panel never grows with its text.
        panel.set_hexpand(false);
        panel.set_halign(gtk::Align::End);
        panel.set_visible(false);
        panel.append(&head);
        panel.append(&sees);
        panel.append(&scroller);
        panel.append(&foot);

        Self {
            panel,
            side_button,
            top_button,
            entry,
            send,
            new_chat,
            close,
            prefs,
            messages,
            scroller,
            sees,
            who,
            empty,
            spinner,
            waiting: Rc::new(RefCell::new(None)),
            card: RefCell::new(None),
            on_link: Rc::new(RefCell::new(None)),
        }
    }

    /// Called with the address of a link the user clicked in an answer.
    pub fn set_on_link(&self, f: impl Fn(&str) + 'static) {
        *self.on_link.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_sees(&self, text: &str) {
        self.sees.set_text(text);
    }

    pub fn set_who(&self, text: &str) {
        self.who.set_text(text);
    }

    pub fn set_busy(&self, busy: bool) {
        self.spinner.set_visible(busy);
        self.spinner.set_spinning(busy);
        self.send.set_icon_name(if busy {
            "ndl-stop-symbolic"
        } else {
            "ndl-arrow-right-symbolic"
        });
        self.send
            .set_tooltip_text(Some(if busy { "Detener" } else { "Enviar" }));
    }

    fn push(&self, widget: &impl IsA<gtk::Widget>) {
        self.empty.set_visible(false);
        self.messages.append(widget);
        let adj = self.scroller.vadjustment();
        // After the new row has its size.
        glib::idle_add_local_once(move || adj.set_value(adj.upper() - adj.page_size()));
    }

    fn bubble(&self, class: &str, align: gtk::Align) -> gtk::Label {
        let label = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .halign(align)
            .max_width_chars(44)
            .css_classes(["brujula-ai-bubble", class])
            .build();
        self.push(&label);
        label
    }

    pub fn add_user(&self, text: &str) {
        self.bubble("brujula-ai-user", gtk::Align::End)
            .set_text(text);
    }

    /// An answer: Markdown turned into markup (see `brujula_core::ai::markup`).
    pub fn add_assistant(&self, text: &str) {
        let label = self.bubble("brujula-ai-bot", gtk::Align::Start);
        label.set_use_markup(true);
        label.set_markup(&brujula_core::ai::markup::pango(text));
        let on_link = self.on_link.clone();
        label.connect_activate_link(move |_, uri| {
            if let Some(f) = on_link.borrow().as_ref() {
                f(uri);
            }
            glib::Propagation::Stop
        });
    }

    /// A line of what the assistant did or was shown: it keeps the user informed.
    pub fn add_note(&self, text: &str) {
        let label = gtk::Label::builder()
            .label(text)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .max_width_chars(48)
            .css_classes(["brujula-ai-note"])
            .build();
        self.push(&label);
    }

    pub fn add_error(&self, text: &str) {
        let label = self.bubble("brujula-ai-error", gtk::Align::Start);
        label.set_text(text);
    }

    /// Empties the conversation.
    pub fn clear(&self) {
        self.cancel_question();
        while let Some(child) = self.messages.first_child() {
            self.messages.remove(&child);
        }
        self.empty.set_visible(true);
        self.messages.append(&self.empty);
    }

    /// The assistant asks before it acts. Resolves when the user answers; closing or clearing
    /// the panel answers no.
    pub fn ask(
        &self,
        title: &str,
        detail: &str,
        offer_always: bool,
    ) -> impl std::future::Future<Output = Answer> {
        self.cancel_question();
        let (tx, rx) = oneshot::channel();
        *self.waiting.borrow_mut() = Some(tx);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        card.add_css_class("brujula-ai-ask");
        let head = gtk::Label::builder()
            .label(title)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let body = gtk::Label::builder()
            .label(detail)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .max_width_chars(44)
            .build();
        let yes = gtk::Button::with_label("Permitir");
        yes.add_css_class("suggested-action");
        let no = gtk::Button::with_label("Denegar");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&no);
        row.append(&yes);
        card.append(&head);
        card.append(&body);
        card.append(&row);
        let always = offer_always.then(|| {
            let b = gtk::Button::with_label("Permitirlo en toda esta conversación");
            b.add_css_class("flat");
            card.append(&b);
            b
        });

        let answer = |waiting: &Waiting, a: Answer| {
            if let Some(tx) = waiting.borrow_mut().take() {
                let _ = tx.send(a);
            }
        };
        let w = self.waiting.clone();
        yes.connect_clicked(move |_| answer(&w, Answer::Yes));
        let w = self.waiting.clone();
        no.connect_clicked(move |_| answer(&w, Answer::No));
        if let Some(b) = always {
            let w = self.waiting.clone();
            b.connect_clicked(move |_| answer(&w, Answer::Always));
        }
        self.push(&card);
        *self.card.borrow_mut() = Some(card);
        yes.grab_focus();
        let card_slot = self.card.borrow().clone();
        let title_text = title.to_owned();
        async move {
            let a = rx.await.unwrap_or(Answer::No);
            // The question is answered: its buttons go away, the line says how.
            if let Some(card) = card_slot {
                while let Some(child) = card.last_child() {
                    card.remove(&child);
                }
                let said = match a {
                    Answer::Yes => "Permitido",
                    Answer::Always => "Permitido para esta conversación",
                    Answer::No => "Denegado",
                };
                let l = gtk::Label::builder()
                    .label(format!("{said}: {title_text}"))
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(["heading"])
                    .build();
                card.append(&l);
            }
            a
        }
    }

    /// Answers «no» to the question that is waiting, if there is one.
    pub fn cancel_question(&self) {
        if let Some(tx) = self.waiting.borrow_mut().take() {
            let _ = tx.send(Answer::No);
        }
        self.card.borrow_mut().take();
    }
}
