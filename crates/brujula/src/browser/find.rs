//! Find in page (Ctrl+F): a small bar at the top right of the page, over it, with the number of
//! matches and the arrows. It is a view of WebKit's `FindController`.

use super::*;

use webkit6::FindController;
use webkit6::FindOptions;

/// How many matches are counted at most.
const MAX_MATCHES: u32 = 1000;

pub(super) struct FindBar {
    pub root: gtk::Revealer,
    entry: gtk::SearchEntry,
    count: gtk::Label,
    /// The page the bar is searching in, and what listens to it.
    view: RefCell<Option<WebView>>,
    listening: RefCell<Vec<(FindController, glib::SignalHandlerId)>>,
}

impl FindBar {
    pub(super) fn new() -> Rc<Self> {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Buscar en la página")
            .width_chars(24)
            .build();
        let count = gtk::Label::builder()
            .css_classes(["dim-label", "brujula-find-count"])
            .width_chars(11)
            .xalign(1.0)
            .build();
        let prev = flat("ndl-chevron-up-symbolic", "Anterior (Mayús+Intro)");
        let next = flat("ndl-chevron-down-symbolic", "Siguiente (Intro)");
        let close = flat("ndl-close-symbolic", "Cerrar (Esc)");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.add_css_class("brujula-find");
        row.append(&entry);
        row.append(&count);
        row.append(&prev);
        row.append(&next);
        row.append(&close);
        let root = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(180)
            .reveal_child(false)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .margin_top(10)
            .margin_end(14)
            .child(&row)
            .build();
        root.set_can_target(false);
        let bar = Rc::new(Self {
            root,
            entry,
            count,
            view: RefCell::new(None),
            listening: RefCell::new(Vec::new()),
        });

        let this = Rc::downgrade(&bar);
        bar.entry.connect_search_changed(move |_| {
            if let Some(b) = this.upgrade() {
                b.search();
            }
        });
        let this = Rc::downgrade(&bar);
        bar.entry.connect_activate(move |_| {
            if let Some(b) = this.upgrade() {
                b.step(true);
            }
        });
        let this = Rc::downgrade(&bar);
        bar.entry.connect_stop_search(move |_| {
            if let Some(b) = this.upgrade() {
                b.close();
            }
        });
        // Shift+Enter goes back, like everywhere else.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = Rc::downgrade(&bar);
        keys.connect_key_pressed(move |_, key, _, state| {
            let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);
            if enter && state.contains(gdk::ModifierType::SHIFT_MASK) {
                if let Some(b) = this.upgrade() {
                    b.step(false);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        bar.entry.add_controller(keys);
        let this = Rc::downgrade(&bar);
        prev.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.step(false);
            }
        });
        let this = Rc::downgrade(&bar);
        next.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.step(true);
            }
        });
        let this = Rc::downgrade(&bar);
        close.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.close();
            }
        });
        bar
    }

    /// Starts searching in `view` and puts the cursor in the box.
    pub(super) fn open(self: &Rc<Self>, view: &WebView) {
        if self.view.borrow().as_ref() != Some(view) {
            self.finish();
            self.attach(view);
        }
        self.root.set_can_target(true);
        self.root.set_reveal_child(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
        if !self.entry.text().is_empty() {
            self.search();
        }
    }

    pub(super) fn close(&self) {
        self.finish();
        self.root.set_reveal_child(false);
        self.root.set_can_target(false);
        if let Some(v) = self.view.borrow_mut().take() {
            v.grab_focus();
        }
    }

    /// Closes the bar when the page in front is not the one it searches in.
    pub(super) fn close_unless(&self, front: Option<&WebView>) {
        let current = self.view.borrow().clone();
        if current.is_some() && current.as_ref() != front {
            self.finish();
            self.root.set_reveal_child(false);
            self.root.set_can_target(false);
            *self.view.borrow_mut() = None;
        }
    }

    fn attach(self: &Rc<Self>, view: &WebView) {
        *self.view.borrow_mut() = Some(view.clone());
        let Some(fc) = view.find_controller() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let found = fc.connect_found_text(move |_, n| {
            if let Some(b) = weak.upgrade() {
                b.show_count(n);
            }
        });
        let weak = Rc::downgrade(self);
        let failed = fc.connect_failed_to_find_text(move |_| {
            if let Some(b) = weak.upgrade() {
                b.show_count(0);
            }
        });
        let weak = Rc::downgrade(self);
        let counted = fc.connect_counted_matches(move |_, n| {
            if let Some(b) = weak.upgrade() {
                b.show_count(n);
            }
        });
        self.listening.borrow_mut().extend([
            (fc.clone(), found),
            (fc.clone(), failed),
            (fc, counted),
        ]);
    }

    /// Clears the highlights and lets go of the page's find controller.
    fn finish(&self) {
        for (fc, id) in self.listening.borrow_mut().drain(..) {
            fc.disconnect(id);
        }
        if let Some(fc) = self
            .view
            .borrow()
            .as_ref()
            .and_then(|v| v.find_controller())
        {
            fc.search_finish();
        }
        self.count.set_label("");
    }

    fn options() -> u32 {
        (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits()
    }

    fn search(&self) {
        let Some(fc) = self
            .view
            .borrow()
            .as_ref()
            .and_then(|v| v.find_controller())
        else {
            return;
        };
        let text = self.entry.text();
        if text.is_empty() {
            fc.search_finish();
            self.count.set_label("");
            self.entry.remove_css_class("error");
            return;
        }
        fc.search(&text, Self::options(), MAX_MATCHES);
        fc.count_matches(&text, Self::options(), MAX_MATCHES);
    }

    fn step(&self, forward: bool) {
        let Some(fc) = self
            .view
            .borrow()
            .as_ref()
            .and_then(|v| v.find_controller())
        else {
            return;
        };
        if self.entry.text().is_empty() {
            return;
        }
        if forward {
            fc.search_next();
        } else {
            fc.search_previous();
        }
    }

    fn show_count(&self, n: u32) {
        if self.entry.text().is_empty() {
            self.count.set_label("");
            return;
        }
        match n {
            0 => {
                self.count.set_label("Sin resultados");
                self.entry.add_css_class("error");
            }
            1 => {
                self.count.set_label("1 resultado");
                self.entry.remove_css_class("error");
            }
            n if n >= MAX_MATCHES => {
                self.count.set_label(&format!("más de {MAX_MATCHES}"));
                self.entry.remove_css_class("error");
            }
            n => {
                self.count.set_label(&format!("{n} resultados"));
                self.entry.remove_css_class("error");
            }
        }
    }
}

fn flat(icon: &str, tip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build()
}
