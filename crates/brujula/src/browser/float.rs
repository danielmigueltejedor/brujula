//! A tab in a small window of its own: the page keeps playing while the main window is used
//! for something else. It is what WebKitGTK allows in place of picture in picture, which it
//! does not have. In the main window the tab shows a note and a button to bring it back.

use super::*;

/// The window of a floating tab and the note left in its place.
pub(super) struct Floating {
    window: gtk::Window,
    note: gtk::Box,
}

impl Browser {
    pub(super) fn is_floating(&self, id: TabId) -> bool {
        self.floats.borrow().contains_key(&id)
    }

    /// Moves the page of a tab into a window of its own.
    pub(super) fn float_tab(self: &Rc<Self>, id: TabId) {
        if self.in_split(id) || self.is_floating(id) {
            return;
        }
        let Some(view) = self.views.borrow().get(&id).cloned() else {
            return;
        };
        let title = self
            .model
            .borrow()
            .get(id)
            .map(|t| t.title.clone())
            .unwrap_or_default();
        let mut builder = gtk::Window::builder()
            .title(if title.is_empty() { "Brújula" } else { &title })
            .default_width(480)
            .default_height(320);
        if let Some(app) = self.window.application() {
            builder = builder.application(&app);
        }
        let window = builder.build();
        let note = gtk::Box::new(gtk::Orientation::Vertical, 12);
        note.set_halign(gtk::Align::Center);
        note.set_valign(gtk::Align::Center);
        note.append(&gtk::Label::new(Some(
            "Esta pestaña está en una ventana flotante.",
        )));
        let back = gtk::Button::with_label("Devolverla a esta ventana");
        back.set_halign(gtk::Align::Center);
        note.append(&back);
        let this = Rc::downgrade(self);
        back.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.unfloat(id, true);
            }
        });
        let name = format!("tab-{}", id.0);
        self.stack.remove(&view);
        self.stack.add_named(&note, Some(&name));
        window.set_child(Some(&view));
        let this = Rc::downgrade(self);
        window.connect_close_request(move |_| {
            if let Some(b) = this.upgrade() {
                b.unfloat(id, false);
            }
            glib::Propagation::Proceed
        });
        let win = window.clone();
        view.connect_title_notify(move |v| {
            if let Some(t) = v.title() {
                win.set_title(Some(&t));
            }
        });
        self.floats.borrow_mut().insert(
            id,
            Floating {
                window: window.clone(),
                note,
            },
        );
        window.present();
        if self.model.borrow().selected() == Some(id) {
            self.show_page(id);
        }
    }

    /// Brings the page back. `close` is false when the window is already closing.
    pub(super) fn unfloat(&self, id: TabId, close: bool) {
        let Some(f) = self.floats.borrow_mut().remove(&id) else {
            return;
        };
        f.window.set_child(None::<&gtk::Widget>);
        self.stack.remove(&f.note);
        if let Some(v) = self.views.borrow().get(&id) {
            self.stack.add_named(v, Some(&format!("tab-{}", id.0)));
        }
        if close {
            f.window.destroy();
        }
        if self.model.borrow().selected() == Some(id) {
            self.show_page(id);
        }
    }

    /// Closes every floating window (the main window is going away).
    pub(super) fn unfloat_all(&self) {
        let ids: Vec<TabId> = self.floats.borrow().keys().copied().collect();
        for id in ids {
            self.unfloat(id, true);
        }
    }
}
