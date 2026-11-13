//! The history: the pages visited, kept on this computer, and what the user can do with them.
//! A private window has an empty history that is never written.

use super::*;

/// The history is written a moment after the last page, not at every page.
const SAVE_DELAY_MS: u64 = 3000;

impl Browser {
    fn save_history(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::history_file(), &self.history.borrow().to_json());
    }

    /// Writes the history soon, once for a burst of pages.
    fn schedule_history_save(self: &Rc<Self>) {
        if self.private || self.history_save_queued.replace(true) {
            return;
        }
        let this = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(SAVE_DELAY_MS), move || {
            if let Some(b) = this.upgrade() {
                b.history_save_queued.set(false);
                b.save_history();
            }
        });
    }

    /// Writes what is pending; the window is closing.
    pub(super) fn flush_history(&self) {
        self.save_history();
    }

    /// Notes the page a view finished loading.
    pub(super) fn record_visit(self: &Rc<Self>, view: &WebView) {
        if self.private || !self.settings.borrow().history_enabled {
            return;
        }
        let Some(uri) = view.uri() else { return };
        let title = view.title().map(|t| t.to_string()).unwrap_or_default();
        if self.history.borrow_mut().record(&uri, &title, now_ms()) {
            self.history_changed();
            self.schedule_history_save();
        }
    }

    /// The title of a page often arrives after the page does.
    pub(super) fn note_title(self: &Rc<Self>, view: &WebView, title: &str) {
        if self.private || !self.settings.borrow().history_enabled || title.is_empty() {
            return;
        }
        let Some(uri) = view.uri() else { return };
        self.history.borrow_mut().set_title(&uri, title);
        self.schedule_history_save();
    }

    /// One of the `history.*` changes of the preferences page.
    pub(super) fn change_history(self: &Rc<Self>, what: &str, value: &str) {
        match what {
            "clear" => {
                let now = now_ms();
                let mut h = self.history.borrow_mut();
                match value {
                    "hour" => h.clear_since(now.saturating_sub(3_600_000)),
                    "day" => h.clear_since(now.saturating_sub(86_400_000)),
                    "all" => h.clear(),
                    _ => return,
                }
            }
            "remove" => self.history.borrow_mut().remove(value),
            _ => return,
        }
        self.history_changed();
        self.save_history();
    }

    /// The pages the preferences list: newest first.
    pub(super) fn history_view(&self) -> (Vec<(String, String)>, usize) {
        let h = self.history.borrow();
        (
            h.items()
                .iter()
                .take(200)
                .map(|v| (v.url.clone(), v.title.clone()))
                .collect(),
            h.len(),
        )
    }
}
