//! Peek: a link opened with Shift+click shows its page in a card over the current one, to have
//! a look without leaving it. «Abrir en pestaña» keeps it, closing (Esc, or a click outside the
//! card) throws it away.
//!
//! The card's page is as locked down as a tab's (same network session, same blocker, no
//! permissions, no new windows) and only ever shows web addresses.

use super::*;

/// What the card is made of. Built once, at the start; the page in it comes and goes.
pub(super) struct PeekCard {
    pub layer: gtk::Box,
    pub card: gtk::Box,
    pub title: gtk::Label,
    pub open: gtk::Button,
    pub close: gtk::Button,
}

pub(super) fn build_peek() -> PeekCard {
    let layer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layer.add_css_class("brujula-peek");
    layer.set_visible(false);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("brujula-peek-card");
    card.set_overflow(gtk::Overflow::Hidden);
    card.set_hexpand(true);
    card.set_vexpand(true);
    card.set_margin_top(36);
    card.set_margin_bottom(36);
    card.set_margin_start(56);
    card.set_margin_end(56);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.add_css_class("brujula-peek-header");
    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let open = gtk::Button::builder()
        .label("Abrir en pestaña")
        .css_classes(["flat"])
        .build();
    let close = gtk::Button::builder()
        .icon_name("ndl-close-symbolic")
        .tooltip_text("Cerrar (Esc)")
        .css_classes(["flat"])
        .build();
    header.append(&title);
    header.append(&open);
    header.append(&close);
    card.append(&header);
    layer.append(&card);
    PeekCard {
        layer,
        card,
        title,
        open,
        close,
    }
}

impl Browser {
    pub(super) fn wire_peek(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        self.peek.close.connect_clicked({
            let this = this.clone();
            move |_| {
                if let Some(b) = this.upgrade() {
                    b.close_peek();
                }
            }
        });
        self.peek.open.connect_clicked({
            let this = this.clone();
            move |_| {
                if let Some(b) = this.upgrade() {
                    b.keep_peek();
                }
            }
        });
        // Esc closes it, whatever has the focus.
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed({
            let this = this.clone();
            move |_, keyval, _, _| match this.upgrade() {
                Some(b) if keyval == gdk::Key::Escape && b.peek.layer.is_visible() => {
                    b.close_peek();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.peek.layer.add_controller(key);
        // A click outside the card closes it.
        let click = gtk::GestureClick::new();
        click.connect_released(move |g, _, x, y| {
            let Some(b) = this.upgrade() else { return };
            let outside = b
                .peek
                .layer
                .pick(x, y, gtk::PickFlags::DEFAULT)
                .is_some_and(|w| w == b.peek.layer);
            if outside {
                g.set_state(gtk::EventSequenceState::Claimed);
                b.close_peek();
            }
        });
        self.peek.layer.add_controller(click);
    }

    /// Shows `uri` in the card. Only web addresses.
    pub(super) fn peek(self: &Rc<Self>, uri: &str) {
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return;
        }
        self.close_peek();
        let view = WebView::builder()
            .network_session(&self.session)
            .user_content_manager(&self.ucm)
            .hexpand(true)
            .vexpand(true)
            .build();
        harden(&view);
        // A page in the card cannot open windows, and can only go to web addresses.
        view.connect_create(|_, _| None);
        view.connect_decide_policy(|_, decision, kind| {
            if kind != PolicyDecisionType::NavigationAction {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else {
                return false;
            };
            let uri = nav
                .navigation_action()
                .and_then(|a| a.request())
                .and_then(|r| r.uri())
                .map(|u| u.to_ascii_lowercase())
                .unwrap_or_default();
            let web = uri.starts_with("https://") || uri.starts_with("http://");
            if !web && uri != "about:blank" {
                decision.ignore();
                return true;
            }
            false
        });
        let this = Rc::downgrade(self);
        view.connect_title_notify(move |v| {
            if let Some(b) = this.upgrade() {
                let title = v.title().map(|t| t.to_string()).unwrap_or_default();
                let shown = if title.is_empty() {
                    v.uri().map(|u| u.to_string()).unwrap_or_default()
                } else {
                    title
                };
                b.peek.title.set_text(&shown);
            }
        });
        let this = Rc::downgrade(self);
        view.connect_load_changed(move |v, ev| {
            if let (Some(b), LoadEvent::Finished) = (this.upgrade(), ev) {
                hide_elements(&b.filters, v);
            }
        });
        self.peek.title.set_text(uri);
        self.peek.card.append(&view);
        self.peek.layer.set_visible(true);
        view.load_uri(uri);
        view.grab_focus();
        *self.peek_view.borrow_mut() = Some(view);
    }

    pub(super) fn close_peek(&self) {
        let view = self.peek_view.borrow_mut().take();
        if let Some(v) = view {
            self.peek.card.remove(&v);
        }
        self.peek.layer.set_visible(false);
        if let Some(v) = self.current_view() {
            v.grab_focus();
        }
    }

    /// Turns the peek into a tab next to the one in front.
    fn keep_peek(self: &Rc<Self>) {
        let uri = self
            .peek_view
            .borrow()
            .as_ref()
            .and_then(|v| v.uri())
            .map(|u| u.to_string());
        self.close_peek();
        if let Some(uri) = uri {
            let after = self.model.borrow().selected();
            self.open_tab(&uri, after, true);
        }
    }
}
