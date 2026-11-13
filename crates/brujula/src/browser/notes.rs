//! The notepad of each space, in a popover from the sidebar.

use super::*;

use brujula_core::notes::Notes;

pub(super) fn load() -> Notes {
    std::fs::read_to_string(paths::notes_file())
        .map(|t| Notes::from_json(&t))
        .unwrap_or_default()
}

impl Browser {
    pub(super) fn wire_notes(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        self.notes_button.connect_clicked(move |button| {
            if let Some(b) = this.upgrade() {
                b.show_notes(button);
            }
        });
    }

    fn show_notes(self: &Rc<Self>, button: &gtk::Button) {
        let (space, name) = {
            let spaces = self.spaces.borrow();
            let id = spaces.current();
            (
                id,
                spaces.get(id).map(|s| s.name.clone()).unwrap_or_default(),
            )
        };
        let pop = gtk::Popover::new();
        pop.set_parent(button);
        let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
        col.set_margin_top(8);
        col.set_margin_bottom(8);
        col.set_margin_start(8);
        col.set_margin_end(8);
        let title = gtk::Label::new(Some(&format!("Notas · {name}")));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        let text = gtk::TextView::new();
        text.set_wrap_mode(gtk::WrapMode::WordChar);
        text.set_left_margin(6);
        text.set_right_margin(6);
        text.set_top_margin(6);
        text.set_bottom_margin(6);
        text.buffer().set_text(self.notes.borrow().get(space));
        let scroller = gtk::ScrolledWindow::builder()
            .child(&text)
            .min_content_width(300)
            .min_content_height(280)
            .build();
        scroller.add_css_class("brujula-notes");
        col.append(&title);
        col.append(&scroller);
        pop.set_child(Some(&col));
        let this = Rc::downgrade(self);
        text.buffer().connect_changed(move |buffer| {
            let Some(b) = this.upgrade() else { return };
            let (start, end) = buffer.bounds();
            let content = buffer.text(&start, &end, false).to_string();
            if b.notes.borrow_mut().set(space, &content) {
                b.schedule_notes_save();
            }
        });
        let this = Rc::downgrade(self);
        pop.connect_closed(move |p| {
            if let Some(b) = this.upgrade() {
                b.save_notes();
            }
            p.unparent();
        });
        pop.popup();
        text.grab_focus();
    }

    pub(super) fn save_notes(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::notes_file(), &self.notes.borrow().to_json());
    }

    /// Writes the notes soon, once, while the user is typing.
    fn schedule_notes_save(self: &Rc<Self>) {
        if self.notes_save_queued.replace(true) {
            return;
        }
        let this = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
            if let Some(b) = this.upgrade() {
                b.notes_save_queued.set(false);
                b.save_notes();
            }
        });
    }
}
