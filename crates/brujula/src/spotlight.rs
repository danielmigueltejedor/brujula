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
    /// The site's own icon, when there is one; it replaces `icon`.
    pub texture: Option<gdk::Texture>,
}

type Query = Box<dyn Fn(&str) -> Vec<Item>>;
type Tiles = Box<dyn Fn() -> Vec<Item>>;
type Closed = Box<dyn Fn(bool)>;
type Activate = Box<dyn Fn(Action)>;

pub struct Spotlight {
    pub root: gtk::Box,
    card: gtk::Box,
    entry: gtk::Entry,
    list: gtk::ListBox,
    results: gtk::ScrolledWindow,
    /// The icons shown while nothing is typed (the bookmarks), and what each one does.
    tiles: gtk::FlowBox,
    tile_items: Tiles,
    tile_actions: RefCell<Vec<Action>>,
    items: RefCell<Vec<Action>>,
    query: Query,
    activate: Activate,
    /// Called when it closes; `true` if the person chose something.
    on_closed: RefCell<Option<Closed>>,
    /// Ignore `changed` while we set the text ourselves.
    quiet: Cell<bool>,
}

impl Spotlight {
    pub fn new(
        query: impl Fn(&str) -> Vec<Item> + 'static,
        activate: impl Fn(Action) + 'static,
        tiles: impl Fn() -> Vec<Item> + 'static,
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

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(1)
            .max_children_per_line(10)
            .column_spacing(6)
            .row_spacing(6)
            .halign(gtk::Align::Center)
            .activate_on_single_click(true)
            .visible(false)
            .build();
        flow.add_css_class("brujula-marks");

        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("ndl-island");
        card.add_css_class("brujula-spotlight");
        card.set_size_request(560, -1);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Start);
        card.set_margin_top(180);
        card.append(&entry);
        card.append(&flow);
        card.append(&results);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("brujula-scrim");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_visible(false);
        root.append(&card);

        let this = Rc::new(Self {
            root,
            card: card.clone(),
            entry,
            list,
            results,
            tiles: flow,
            tile_items: Box::new(tiles),
            tile_actions: RefCell::new(Vec::new()),
            items: RefCell::new(Vec::new()),
            query: Box::new(query),
            activate: Box::new(activate),
            on_closed: RefCell::new(None),
            quiet: Cell::new(false),
        });
        this.wire(&card);
        this
    }

    pub fn set_on_closed(&self, f: impl Fn(bool) + 'static) {
        *self.on_closed.borrow_mut() = Some(Box::new(f));
    }

    /// Shows the Spotlight with `initial` selected, so typing replaces it.
    pub fn open(self: &Rc<Self>, initial: &str) {
        self.quiet.set(true);
        self.entry.set_text(initial);
        self.quiet.set(false);
        self.place();
        self.root.set_visible(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
        self.refresh();
    }

    /// Puts the search box in the middle of the window, a little above the exact centre so the
    /// results that open below it still fit. It stays where it is while they come and go.
    fn place(&self) {
        let height = self
            .root
            .parent()
            .map_or(0, |p| p.height())
            .max(self.root.height());
        let height = if height > 0 { height } else { 800 };
        // Room under the box for the results (at most 360 px) and some air.
        let room = height - 60 - 360 - 32;
        let centre = height * 40 / 100 - 30;
        self.card.set_margin_top(centre.min(room).max(24));
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    pub fn close(&self) {
        self.close_with(false);
    }

    fn close_with(&self, chosen: bool) {
        if !self.root.is_visible() {
            return;
        }
        self.root.set_visible(false);
        if let Some(f) = self.on_closed.borrow().as_ref() {
            f(chosen);
        }
    }

    fn refresh(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let text = self.entry.text();
        self.refresh_tiles(text.trim().is_empty());
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

    /// Shows the bookmarks as icons while nothing is typed, and hides them as soon as there is
    /// something to search for.
    fn refresh_tiles(self: &Rc<Self>, idle: bool) {
        while let Some(child) = self.tiles.first_child() {
            self.tiles.remove(&child);
        }
        let items = if idle {
            (self.tile_items)()
        } else {
            Vec::new()
        };
        let mut actions = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let button = tile(item);
            let this = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(s) = this.upgrade() {
                    let action = s.tile_actions.borrow().get(i).cloned();
                    if let Some(action) = action {
                        s.close_with(true);
                        (s.activate)(action);
                    }
                }
            });
            self.tiles.append(&button);
            actions.push(item.action.clone());
        }
        self.tiles.set_visible(!actions.is_empty());
        *self.tile_actions.borrow_mut() = actions;
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
        self.close_with(true);
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

/// A bookmark as an icon alone, in the manner of the essentials; its name is the tooltip.
fn tile(item: &Item) -> gtk::Button {
    let button = gtk::Button::builder()
        .css_classes(["flat", "brujula-ess"])
        .tooltip_text(&item.title)
        .halign(gtk::Align::Center)
        .build();
    let face: gtk::Widget = match &item.texture {
        Some(texture) => crate::favicons::picture_sized(texture, 24),
        None => {
            let letter = gtk::Label::new(Some(&initial(&item.title, &item.detail)));
            letter.add_css_class("brujula-mark-initial");
            letter.upcast()
        }
    };
    button.set_child(Some(&face));
    button
}

/// The first letter of a bookmark's name, or of its address when the name is empty.
fn initial(title: &str, url: &str) -> String {
    let source = if title.trim().is_empty() {
        url.trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.")
    } else {
        title.trim()
    };
    source
        .chars()
        .next()
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect())
}

fn row(item: &Item) -> gtk::ListBoxRow {
    let icon: gtk::Widget = match &item.texture {
        Some(texture) => crate::favicons::picture(texture),
        None => {
            let img = gtk::Image::from_icon_name(item.icon);
            img.set_pixel_size(16);
            img.upcast()
        }
    };
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
