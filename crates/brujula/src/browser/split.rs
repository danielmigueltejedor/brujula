//! Split view: two tabs side by side in the page area.
//!
//! The pair lives in a `Paned` that is one page of the stack, so selecting either tab shows
//! both. The tab that has the focus is the selected one: the address bar, the buttons and the
//! sidebar follow it, and clicking into the other page selects that tab.

use super::*;

impl Browser {
    /// The pair of tabs on show side by side, if any.
    pub(super) fn split_pair(&self) -> Option<(TabId, TabId)> {
        *self.paired.borrow()
    }

    pub(super) fn in_split(&self, id: TabId) -> bool {
        self.split_pair().is_some_and(|(a, b)| a == id || b == id)
    }

    /// Shows the page of a tab: its own, or the pair's when it is in the split view.
    pub(super) fn show_page(&self, id: TabId) {
        let name = if self.in_split(id) {
            "split".to_owned()
        } else {
            format!("tab-{}", id.0)
        };
        let before = self.stack.visible_child_name();
        self.stack.set_visible_child_name(&name);
        // A page coming to the front eases in (the CSS has the animation).
        if before.as_deref() != Some(name.as_str()) {
            if let Some(page) = self.stack.visible_child() {
                page.add_css_class("brujula-enter");
                glib::timeout_add_local_once(std::time::Duration::from_millis(260), move || {
                    page.remove_css_class("brujula-enter");
                });
            }
        }
    }

    /// Puts `a` and `b` side by side and selects `b`.
    pub(super) fn enter_split(self: &Rc<Self>, a: TabId, b: TabId) {
        if a == b || self.is_floating(a) || self.is_floating(b) {
            return;
        }
        let (va, vb) = {
            let views = self.views.borrow();
            match (views.get(&a), views.get(&b)) {
                (Some(va), Some(vb)) => (va.clone(), vb.clone()),
                _ => return,
            }
        };
        self.end_split();
        self.stack.remove(&va);
        self.stack.remove(&vb);
        self.split_paned.set_start_child(Some(&va));
        self.split_paned.set_end_child(Some(&vb));
        *self.paired.borrow_mut() = Some((a, b));
        let paned = self.split_paned.clone();
        let stack = self.stack.clone();
        glib::idle_add_local_once(move || paned.set_position(stack.width() / 2));
        self.mark_split_rows();
        let changes = self.model.borrow_mut().select(b);
        if changes.is_empty() {
            // `b` was already in front: only the page changes.
            self.show_page(b);
        }
        self.apply(changes);
    }

    /// Gives the two pages back to the stack, one each.
    pub(super) fn end_split(&self) {
        let Some((a, b)) = self.paired.borrow_mut().take() else {
            return;
        };
        self.split_paned.set_start_child(None::<&gtk::Widget>);
        self.split_paned.set_end_child(None::<&gtk::Widget>);
        for id in [a, b] {
            if let Some(v) = self.views.borrow().get(&id) {
                self.stack.add_named(v, Some(&format!("tab-{}", id.0)));
            }
        }
        self.mark_split_rows();
        if let Some(sel) = self.model.borrow().selected() {
            self.show_page(sel);
        }
    }

    fn mark_split_rows(&self) {
        let pair = self.split_pair();
        for (id, r) in self.rows.borrow().iter() {
            if pair.is_some_and(|(a, b)| a == *id || b == *id) {
                r.row.add_css_class("brujula-split");
            } else {
                r.row.remove_css_class("brujula-split");
            }
        }
    }

    /// Clicking into a page of the split view selects its tab.
    pub(super) fn follow_focus(self: &Rc<Self>, id: TabId, view: &WebView) {
        let focus = gtk::EventControllerFocus::new();
        let this = Rc::downgrade(self);
        focus.connect_enter(move |_| {
            if let Some(b) = this.upgrade() {
                if b.in_split(id) && b.model.borrow().selected() != Some(id) {
                    let changes = b.model.borrow_mut().select(id);
                    b.apply(changes);
                }
            }
        });
        view.add_controller(focus);
    }
}
