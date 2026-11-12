//! The Spotlight: a centred search box over the page that opens from the address pill.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use brujula_core::suggest::Action;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use nodalia_gtk::gtk;

/// A row the Spotlight shows.
pub struct Item {
    pub action: Action,
    pub title: String,
    pub detail: String,
    pub icon: &'static str,
}

type Query = Box<dyn Fn(&str) -> Vec<Item>>;
type Activate = Box<dyn Fn(Action)>;

pub struct Spotlight {
    pub root: gtk::Box,
    entry: gtk::Entry,
    list: gtk::ListBox,
    results: gtk::ScrolledWindow,
    items: RefCell<Vec<Action>>,
    query: Query,
    activate: Activate,
    on_closed: RefCell<Option<Box<dyn Fn()>>>,
    /// Ignore `changed` while we set the text ourselves.
    quiet: Cell<bool>,
}

impl Spotlight {
    pub fn new(
        query: impl Fn(&str) -> Vec<Item> + 'static,
        activate: impl Fn(Action) + 'static,
    ) -> Rc<Self> {
        let entry = gtk::Entry::builder()
            .placeholder_text("Buscar o escribir una dirección")
            .primary_icon_name("ndl-search-symbolic")
            .hexpand(true)
            .build();
        entry.add_css_class("brujula-spotlight-entry");

        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk::SelectionMode::Single);
        let results = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(360)
            .visible(false)
            .build();

        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("ndl-island");
        card.add_css_class("brujula-spotlight");
        card.set_size_request(560, -1);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Start);
        card.set_margin_top(96);
        card.append(&entry);
        card.append(&results);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("brujula-scrim");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_visible(false);
        root.append(&card);

        let this = Rc::new(Self {
            root,
            entry,
            list,
            results,
            items: RefCell::new(Vec::new()),
            query: Box::new(query),
            activate: Box::new(activate),
            on_closed: RefCell::new(None),
            quiet: Cell::new(false),
        });
        this.wire(&card);
        this
    }

    pub fn set_on_closed(&self, f: impl Fn() + 'static) {
        *self.on_closed.borrow_mut() = Some(Box::new(f));
    }

    /// Shows the Spotlight with `initial` selected, so typing replaces it.
    pub fn open(&self, initial: &str) {
        self.quiet.set(true);
        self.entry.set_text(initial);
        self.quiet.set(false);
        self.root.set_visible(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
        self.refresh();
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    pub fn close(&self) {
        if !self.root.is_visible() {
            return;
        }
        self.root.set_visible(false);
        if let Some(f) = self.on_closed.borrow().as_ref() {
            f();
        }
    }

    fn refresh(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let text = self.entry.text();
        let items = (self.query)(&text);
        let mut actions = Vec::with_capacity(items.len());
        for item in items {
            self.list.append(&row(&item));
            actions.push(item.action);
        }
        self.results.set_visible(!actions.is_empty());
        if !actions.is_empty() {
            if let Some(first) = self.list.row_at_index(0) {
                self.list.select_row(Some(&first));
            }
        }
        *self.items.borrow_mut() = actions;
    }

    fn move_selection(&self, delta: i32) {
        let n = self.items.borrow().len() as i32;
        if n == 0 {
            return;
        }
        let cur = self.list.selected_row().map_or(-1, |r| r.index());
        let next = (cur + delta).rem_euclid(n);
        if let Some(r) = self.list.row_at_index(next) {
            self.list.select_row(Some(&r));
        }
    }

    fn run(&self, index: Option<i32>) {
        let action = index
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.items.borrow().get(i).cloned());
        let Some(action) = action else { return };
        self.close();
        (self.activate)(action);
    }

    fn wire(self: &Rc<Self>, card: &gtk::Box) {
        let this = Rc::downgrade(self);
        self.entry.connect_changed(move |_| {
            if let Some(s) = this.upgrade() {
                if !s.quiet.get() {
                    s.refresh();
                }
            }
        });
        let this = Rc::downgrade(self);
        self.entry.connect_activate(move |_| {
            if let Some(s) = this.upgrade() {
                let sel = s.list.selected_row().map(|r| r.index());
                s.run(sel);
            }
        });
        let this = Rc::downgrade(self);
        self.list.connect_row_activated(move |_, row| {
            if let Some(s) = this.upgrade() {
                s.run(Some(row.index()));
            }
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(s) = this.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gdk::Key::Escape => {
                    s.close();
                    glib::Propagation::Stop
                }
                gdk::Key::Down => {
                    s.move_selection(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Up => {
                    s.move_selection(-1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.root.add_controller(keys);

        // A click outside the card closes it.
        let click = gtk::GestureClick::new();
        let this = Rc::downgrade(self);
        let card = card.clone();
        click.connect_pressed(move |g, _, x, y| {
            let Some(s) = this.upgrade() else { return };
            let outside = s
                .root
                .pick(x, y, gtk::PickFlags::DEFAULT)
                .is_none_or(|w| !w.is_ancestor(&card) && w != card);
            if outside {
                g.set_state(gtk::EventSequenceState::Claimed);
                s.close();
            }
        });
        self.root.add_controller(click);
    }
}

fn row(item: &Item) -> gtk::ListBoxRow {
    let icon = gtk::Image::from_icon_name(item.icon);
    icon.set_pixel_size(16);
    let title = gtk::Label::builder()
        .label(&item.title)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&title);
    if !item.detail.is_empty() {
        let detail = gtk::Label::builder()
            .label(&item.detail)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["dim-label", "caption"])
            .build();
        text.append(&detail);
    }
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.add_css_class("brujula-tab");
    content.append(&icon);
    content.append(&text);
    let row = gtk::ListBoxRow::new();
    row.add_css_class("brujula-spotlight-row");
    row.set_child(Some(&content));
    row
}
