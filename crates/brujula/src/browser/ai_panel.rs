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
    empty: gtk::Box,
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

        let mark = gtk::Image::from_icon_name(icon());
        mark.set_pixel_size(18);
        let badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        badge.add_css_class("brujula-ai-badge");
        badge.set_valign(gtk::Align::Center);
        badge.set_halign(gtk::Align::Center);
        badge.append(&mark);
        let title = gtk::Label::builder()
            .label("Asistente")
            .xalign(0.0)
            .css_classes(["brujula-ai-title"])
            .build();
        let who = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["brujula-ai-who"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 1);
        titles.set_hexpand(true);
        titles.set_valign(gtk::Align::Center);
        titles.append(&title);
        titles.append(&who);
        let new_chat = flat_button("ndl-plus-symbolic", "Conversación nueva");
        let prefs = flat_button("ndl-settings-symbolic", "Ajustes del asistente");
        let close = flat_button("ndl-close-symbolic", "Cerrar el asistente");
        for b in [&new_chat, &prefs, &close] {
            b.add_css_class("circular");
        }
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        head.add_css_class("brujula-ai-head");
        head.append(&badge);
        head.append(&titles);
        head.append(&new_chat);
        head.append(&prefs);
        head.append(&close);

        // What the assistant can see, always in view but folded to two lines: a click opens it.
        let sees_icon = gtk::Image::from_icon_name("ndl-shield-symbolic");
        sees_icon.set_pixel_size(14);
        sees_icon.set_valign(gtk::Align::Start);
        let sees = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(40)
            .hexpand(true)
            .css_classes(["brujula-ai-sees-text"])
            .build();
        let sees_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        sees_box.add_css_class("brujula-ai-sees");
        sees_box.set_tooltip_text(Some("Pulsa para ver todo o resumir"));
        sees_box.append(&sees_icon);
        sees_box.append(&sees);
        let fold = gtk::GestureClick::new();
        let label = sees.clone();
        fold.connect_released(move |_, _, _, _| {
            let open = label.lines() < 0;
            label.set_lines(if open { 2 } else { -1 });
            label.set_ellipsize(if open {
                gtk::pango::EllipsizeMode::End
            } else {
                gtk::pango::EllipsizeMode::None
            });
        });
        sees_box.add_controller(fold);

        let messages = gtk::Box::new(gtk::Orientation::Vertical, 10);
        messages.set_margin_start(14);
        messages.set_margin_end(14);
        messages.set_margin_top(6);
        messages.set_margin_bottom(10);

        // The empty state: what it is for, and three things to try.
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.add_css_class("brujula-ai-empty");
        let big = gtk::Image::from_icon_name(icon());
        big.set_pixel_size(26);
        let halo = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        halo.add_css_class("brujula-ai-halo");
        halo.set_halign(gtk::Align::Center);
        halo.append(&big);
        let hello = gtk::Label::builder()
            .label("¿En qué te ayudo?")
            .css_classes(["brujula-ai-hello"])
            .build();
        let hint = gtk::Label::builder()
            .label("Pregunta, o pídele una tarea con tus pestañas. Antes de abrir, leer o pulsar algo, te pregunta.")
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(36)
            .css_classes(["brujula-ai-hint"])
            .build();
        empty.append(&halo);
        empty.append(&hello);
        empty.append(&hint);
        let try_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        try_box.set_margin_top(10);
        let tries = Rc::new(RefCell::new(Vec::<gtk::Button>::new()));
        for text in [
            "Resume esta página",
            "¿Qué pestañas tengo abiertas?",
            "Búscame sitios baratos para viajar",
        ] {
            let b = gtk::Button::builder()
                .label(text)
                .css_classes(["brujula-ai-try"])
                .build();
            if let Some(l) = b.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                l.set_xalign(0.0);
                l.set_wrap(true);
            }
            try_box.append(&b);
            tries.borrow_mut().push(b);
        }
        empty.append(&try_box);
        messages.append(&empty);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&messages)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .build();

        // The line to type in: one rounded box with the entry and the send button inside.
        let entry = gtk::Entry::builder()
            .placeholder_text("Escribe aquí…")
            .hexpand(true)
            .max_length(4000)
            .css_classes(["brujula-ai-entry"])
            .build();
        let send = gtk::Button::builder()
            .icon_name("ndl-arrow-right-symbolic")
            .css_classes(["suggested-action", "circular", "brujula-ai-send"])
            .tooltip_text("Enviar")
            .valign(gtk::Align::Center)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let compose = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        compose.add_css_class("brujula-ai-compose");
        compose.append(&spinner);
        compose.append(&entry);
        compose.append(&send);
        let foot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        foot.set_margin_start(12);
        foot.set_margin_end(12);
        foot.set_margin_bottom(12);
        foot.set_margin_top(4);
        foot.append(&compose);
        for b in tries.borrow().iter() {
            let entry = entry.clone();
            let send = send.clone();
            b.connect_clicked(move |b| {
                entry.set_text(&b.label().unwrap_or_default());
                send.emit_clicked();
            });
        }

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel.add_css_class("ndl-island");
        panel.add_css_class("brujula-ai");
        panel.set_size_request(340, -1);
        // The page takes what is left: the panel never grows with its text.
        panel.set_hexpand(false);
        panel.set_halign(gtk::Align::End);
        panel.set_visible(false);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&head);
        column.append(&sees_box);
        column.append(&scroller);
        column.append(&foot);
        // A scrolled window that never scrolls: it asks for no more than its width, so the
        // text inside wraps instead of widening the panel.
        let clamp = gtk::ScrolledWindow::builder()
            .child(&column)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .propagate_natural_height(false)
            .min_content_width(340)
            .vexpand(true)
            .build();
        panel.append(&clamp);

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
        let icon = if text.starts_with("Leyó") {
            "ndl-file-text-symbolic"
        } else if text.starts_with("Abrió una búsqueda") {
            "ndl-search-symbolic"
        } else if text.starts_with("Abrió")
            || text.starts_with("Cambió")
            || text.starts_with("Cerró")
        {
            "ndl-globe-symbolic"
        } else {
            "ndl-sparkle-symbolic"
        };
        let mark = gtk::Image::from_icon_name(icon);
        mark.set_pixel_size(13);
        mark.set_valign(gtk::Align::Start);
        mark.add_css_class("brujula-ai-note-icon");
        let label = gtk::Label::builder()
            .label(text)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .max_width_chars(44)
            .css_classes(["brujula-ai-note"])
            .build();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        row.add_css_class("brujula-ai-noterow");
        row.set_halign(gtk::Align::Start);
        row.append(&mark);
        row.append(&label);
        self.push(&row);
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

        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("brujula-ai-ask");
        let shield = gtk::Image::from_icon_name("ndl-shield-symbolic");
        shield.set_pixel_size(16);
        shield.set_valign(gtk::Align::Start);
        shield.add_css_class("brujula-ai-ask-icon");
        let head = gtk::Label::builder()
            .label(title)
            .wrap(true)
            .xalign(0.0)
            .hexpand(true)
            .max_width_chars(36)
            .css_classes(["brujula-ai-ask-title"])
            .build();
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top.append(&shield);
        top.append(&head);
        let body = gtk::Label::builder()
            .label(detail)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .max_width_chars(44)
            .css_classes(["brujula-ai-ask-detail"])
            .build();
        body.set_visible(!detail.trim().is_empty());
        let yes = gtk::Button::with_label("Permitir");
        yes.add_css_class("suggested-action");
        yes.add_css_class("pill");
        yes.set_hexpand(true);
        let no = gtk::Button::with_label("Denegar");
        no.add_css_class("pill");
        no.set_hexpand(true);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_homogeneous(true);
        row.append(&no);
        row.append(&yes);
        card.append(&top);
        card.append(&body);
        card.append(&row);
        let always = offer_always.then(|| {
            let b = gtk::Button::with_label("Permitirlo en toda esta conversación");
            b.add_css_class("flat");
            b.add_css_class("brujula-ai-always");
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
                card.set_spacing(0);
                card.add_css_class("brujula-ai-ask-done");
                let (said, icon, class) = match a {
                    Answer::Yes => ("Permitido", "ndl-check-symbolic", "brujula-ai-yes"),
                    Answer::Always => (
                        "Permitido para esta conversación",
                        "ndl-check-symbolic",
                        "brujula-ai-yes",
                    ),
                    Answer::No => ("Denegado", "ndl-close-symbolic", "brujula-ai-no"),
                };
                card.add_css_class(class);
                let mark = gtk::Image::from_icon_name(icon);
                mark.set_pixel_size(14);
                mark.set_valign(gtk::Align::Start);
                let l = gtk::Label::builder()
                    .label(format!(
                        "{said} · {}",
                        title_text.trim_matches(|c| c == '¿' || c == '?')
                    ))
                    .wrap(true)
                    .xalign(0.0)
                    .max_width_chars(40)
                    .css_classes(["brujula-ai-ask-title"])
                    .build();
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                line.append(&mark);
                line.append(&l);
                card.append(&line);
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
