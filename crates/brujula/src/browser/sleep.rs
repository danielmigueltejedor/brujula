//! Putting the tabs that were left alone to sleep: the page is dropped and the tab keeps its
//! address, title and place in the sidebar. It loads again when the tab is opened, the same way
//! a tab restored from the last session does.

use super::*;

use brujula_core::sleep;

/// How often tabs are looked over.
const SWEEP_SECONDS: u32 = 60;

impl Browser {
    pub(super) fn start_sleeping(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        glib::timeout_add_seconds_local(SWEEP_SECONDS, move || match this.upgrade() {
            Some(b) => {
                b.sleep_sweep();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }

    fn sleep_sweep(self: &Rc<Self>) {
        let after = self.settings.borrow().sleep_after_minutes;
        if after == 0 || self.restoring.get() {
            return;
        }
        let now = now_ms();
        let selected = self.model.borrow().selected();
        if let Some(sel) = selected {
            // The tab in front is in use right now, however long ago it was selected.
            self.model.borrow_mut().touch(sel, now);
        }
        let drowsy = sleep::drowsy(&self.model.borrow(), now, after);
        for id in drowsy {
            self.sleep_tab(id);
        }
    }

    /// Replaces the page of a tab with an empty one and remembers the address to load again.
    fn sleep_tab(self: &Rc<Self>, id: TabId) {
        if self.lazy.borrow().contains_key(&id)
            || self.in_split(id)
            || self.is_floating(id)
            || !self.views.borrow().contains_key(&id)
        {
            return;
        }
        let Some(url) = self.model.borrow().get(id).map(|t| t.url.clone()) else {
            return;
        };
        if let Some(old) = self.views.borrow_mut().remove(&id) {
            self.stack.remove(&old);
        }
        self.create_view(id, "", None);
        self.lazy.borrow_mut().insert(id, url);
    }
}
