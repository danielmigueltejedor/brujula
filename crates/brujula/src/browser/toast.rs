//! A notice that rises over the foot of the page, tied to the tab it is about, with buttons.
//! It slides and fades in and out, and goes away on its own when that tab is left.

use super::*;

/// One notice at a time. The page overlay holds `root`.
pub(super) struct Toast {
    pub root: gtk::Revealer,
    icon: gtk::Image,
    text: gtk::Label,
    actions: gtk::Box,
    tab: Cell<Option<TabId>>,
    /// Counts the notices shown, so a timer only closes the one it was set for.
    serial: Cell<u32>,
}

/// A button of a notice: label, whether it is the main one, what it does.
pub(super) type ToastAction = (String, bool, Box<dyn Fn()>);

impl Toast {
    pub(super) fn new() -> Rc<Self> {
        let icon = gtk::Image::new();
        icon.set_pixel_size(18);
        icon.add_css_class("brujula-toast-icon");
        let text = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .hexpand(true)
            .max_width_chars(60)
            .css_classes(["brujula-toast-text"])
            .build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let close = gtk::Button::builder()
            .icon_name("ndl-close-symbolic")
            .tooltip_text("Cerrar")
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Center)
            .build();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.append(&icon);
        row.append(&text);
        row.append(&actions);
        row.append(&close);
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("brujula-toast");
        card.append(&row);
        let root = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .transition_duration(240)
            .reveal_child(false)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .child(&card)
            .build();
        // Nothing of it takes clicks while it is hidden.
        root.set_can_target(false);
        let this = Rc::new(Self {
            root,
            icon,
            text,
            actions,
            tab: Cell::new(None),
            serial: Cell::new(0),
        });
        let weak = Rc::downgrade(&this);
        close.connect_clicked(move |_| {
            if let Some(t) = weak.upgrade() {
                t.hide();
            }
        });
        this
    }

    /// Shows `text` for the tab `tab`; an earlier notice is replaced.
    pub(super) fn show(
        self: &Rc<Self>,
        tab: Option<TabId>,
        icon: &str,
        text: &str,
        actions: Vec<ToastAction>,
    ) {
        while let Some(child) = self.actions.first_child() {
            self.actions.remove(&child);
        }
        self.icon.set_icon_name(Some(icon));
        self.text.set_label(text);
        for (label, main, run) in actions {
            let button = gtk::Button::builder()
                .label(&label)
                .css_classes(if main {
                    vec!["suggested-action"]
                } else {
                    vec!["flat"]
                })
                .valign(gtk::Align::Center)
                .build();
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(t) = weak.upgrade() {
                    t.hide();
                }
                run();
            });
            self.actions.append(&button);
        }
        self.tab.set(tab);
        self.serial.set(self.serial.get().wrapping_add(1));
        self.root.set_can_target(true);
        self.root.set_reveal_child(true);
    }

    /// A short note with no buttons that goes away by itself ("Enlace copiado").
    pub(super) fn flash(self: &Rc<Self>, icon: &str, text: &str) {
        self.show(None, icon, text, Vec::new());
        let serial = self.serial.get();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(1800), move || {
            if let Some(t) = weak.upgrade() {
                if t.serial.get() == serial {
                    t.hide();
                }
            }
        });
    }

    pub(super) fn hide(&self) {
        self.serial.set(self.serial.get().wrapping_add(1));
        self.tab.set(None);
        self.root.set_reveal_child(false);
        self.root.set_can_target(false);
    }

    /// Hides the notice when it is about a tab other than `selected`.
    pub(super) fn hide_unless(&self, selected: Option<TabId>) {
        if self.tab.get().is_some() && self.tab.get() != selected {
            self.hide();
        }
    }
}
