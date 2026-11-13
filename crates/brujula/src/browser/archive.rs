//! Archiving the tabs that were left alone, and bringing them back.

use super::*;

use brujula_core::archive::{self, Entry};

/// How often tabs are looked over.
const SWEEP_SECONDS: u32 = 300;

impl Browser {
    pub(super) fn save_archive(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::archive_file(), &self.archive.borrow().to_json());
    }

    /// Looks over the tabs shortly after start and then every few minutes.
    pub(super) fn start_archiving(self: &Rc<Self>) {
        let first = Rc::downgrade(self);
        glib::timeout_add_seconds_local_once(20, move || {
            if let Some(b) = first.upgrade() {
                b.archive_sweep();
            }
        });
        let this = Rc::downgrade(self);
        glib::timeout_add_seconds_local(SWEEP_SECONDS, move || match this.upgrade() {
            Some(b) => {
                b.archive_sweep();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }

    /// Closes the tabs that went unused for longer than the setting says, keeping them in the
    /// archive.
    fn archive_sweep(self: &Rc<Self>) {
        let now = now_ms();
        let after = self.settings.borrow().archive_after_hours;
        let selected = self.model.borrow().selected();
        if let Some(sel) = selected {
            // The tab in front is in use right now, however long ago it was selected.
            self.model.borrow_mut().touch(sel, now);
        }
        let stale = archive::stale(&self.model.borrow(), &self.folders.borrow(), now, after);
        if stale.is_empty() {
            return;
        }
        for id in stale {
            let entry = self.model.borrow().get(id).map(|t| Entry {
                url: t.url.clone(),
                title: t.title.clone(),
                space: t.space,
                at: now,
            });
            if let Some(entry) = entry {
                self.archive.borrow_mut().push(entry);
                self.close_tab(id);
            }
        }
        self.save_archive();
        self.refresh_newtabs();
    }

    /// A button of the archive section in the preferences: `open` brings the page back in the
    /// space in front, `remove` forgets it.
    pub(super) fn change_archive(self: &Rc<Self>, what: &str, url: &str) {
        let entry = self.archive.borrow_mut().take(url);
        let Some(entry) = entry else { return };
        self.save_archive();
        if what == "open" {
            self.open_tab(&entry.url, None, true);
        }
    }
}
