//! Essentials: site icons at the top of the sidebar (a grid), or in a slim column on the left
//! when the tabs are on top. A click opens the site or goes to the tab that already has it.

use brujula_core::essentials::{Essential, Essentials};

use super::*;

/// A name for a site from its host: `Youtube` for `www.youtube.com`, `Google` for
/// `mail.google.com`.
fn site_name(host: &str) -> String {
    let labels: Vec<&str> = host.trim_start_matches("www.").split('.').collect();
    let pick = if labels.len() >= 2 {
        labels[labels.len() - 2]
    } else {
        labels[0]
    };
    let mut chars = pick.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(super) fn load() -> Essentials {
    std::fs::read_to_string(paths::essentials_file())
        .map(|t| Essentials::from_json(&t))
        .unwrap_or_default()
}

/// The pieces that show the essentials: the grid in the sidebar, the column of the horizontal
/// layout, and the sheet that colours the tiles whose site has no icon yet.
pub(super) struct EssentialsUi {
    pub grid: gtk::FlowBox,
    pub rail: gtk::Box,
    colors: gtk::CssProvider,
    queued: Cell<bool>,
}

impl EssentialsUi {
    pub fn new() -> Self {
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(3)
            .max_children_per_line(5)
            .row_spacing(6)
            .column_spacing(6)
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(8)
            .css_classes(["brujula-ess-grid"])
            .visible(false)
            .build();
        let rail = gtk::Box::new(gtk::Orientation::Vertical, 6);
        rail.add_css_class("brujula-rail");
        rail.set_valign(gtk::Align::Start);
        rail.set_visible(false);
        let colors = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &colors,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        Self {
            grid,
            rail,
            colors,
            queued: Cell::new(false),
        }
    }
}

impl Browser {
    pub(super) fn save_essentials(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(
            &paths::essentials_file(),
            &self.essentials.borrow().to_json(),
        );
    }

    /// Draws the tiles again, once, however many changes ask for it in the same moment.
    pub(super) fn schedule_essentials(self: &Rc<Self>) {
        if self.ess.queued.replace(true) {
            return;
        }
        let this = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(b) = this.upgrade() {
                b.ess.queued.set(false);
                b.refresh_essentials();
            }
        });
    }

    /// The tab of this space that has the essential's site open, if any.
    fn essential_tab(&self, e: &Essential) -> Option<TabId> {
        let host = e.host();
        let host = host.trim_start_matches("www.");
        let m = self.model.borrow();
        let front = m.selected();
        let mut found = None;
        for t in m.in_space(m.current_space()) {
            if t.url.starts_with("http") && host_of(&t.url) == host {
                // The one in front wins, else the first.
                if Some(t.id) == front {
                    return Some(t.id);
                }
                found.get_or_insert(t.id);
            }
        }
        found
    }

    pub(super) fn refresh_essentials(self: &Rc<Self>) {
        let list: Vec<Essential> = self.essentials.borrow().items().to_vec();
        let horizontal = self.settings.borrow().appearance.tabs == TabPosition::Horizontal;
        let front = self
            .model
            .borrow()
            .selected()
            .and_then(|id| self.model.borrow().get(id).map(|t| t.url.clone()))
            .map(|u| host_of(&u));

        while let Some(child) = self.ess.grid.first_child() {
            self.ess.grid.remove(&child);
        }
        while let Some(child) = self.ess.rail.first_child() {
            self.ess.rail.remove(&child);
        }
        let mut css = String::new();
        for e in &list {
            css.push_str(&format!(
                ".brujula-ess-c{} {{ background: {}; color: #ffffff; }}\n",
                e.id,
                e.color()
            ));
            let active = front
                .as_deref()
                .is_some_and(|h| h == e.host().trim_start_matches("www."));
            let open = self.essential_tab(e).is_some();
            self.ess.grid.append(&self.essential_tile(e, active, open));
            self.ess.rail.append(&self.essential_tile(e, active, open));
        }
        self.ess.colors.load_from_string(&css);
        self.ess.grid.set_visible(!list.is_empty() && !horizontal);
        self.ess.rail.set_visible(!list.is_empty() && horizontal);
    }

    fn essential_tile(self: &Rc<Self>, e: &Essential, active: bool, open: bool) -> gtk::Button {
        let button = gtk::Button::builder()
            .css_classes(["flat", "brujula-ess"])
            .tooltip_text(&e.name)
            .halign(gtk::Align::Center)
            .build();
        if active {
            button.add_css_class("brujula-ess-active");
        }
        let face: gtk::Widget = match favicons::lookup(&e.url) {
            Some(texture) => {
                let pic = gtk::Picture::for_paintable(&texture);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_can_shrink(true);
                pic.set_size_request(24, 24);
                pic.set_halign(gtk::Align::Center);
                pic.set_valign(gtk::Align::Center);
                pic.upcast()
            }
            None => {
                button.add_css_class("brujula-ess-letter");
                button.add_css_class(&format!("brujula-ess-c{}", e.id));
                let letter = gtk::Label::new(Some(&e.initial()));
                letter.add_css_class("brujula-ess-initial");
                letter.upcast()
            }
        };
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&face));
        if open {
            // A small dot: the site is open in a tab of this space.
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("brujula-ess-dot");
            dot.set_halign(gtk::Align::Center);
            dot.set_valign(gtk::Align::End);
            overlay.add_overlay(&dot);
        }
        button.set_child(Some(&overlay));

        let id = e.id;
        let this = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.open_essential(id);
            }
        });
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let this = Rc::downgrade(self);
        let anchor = button.clone();
        click.connect_pressed(move |_, _, x, y| {
            if let Some(b) = this.upgrade() {
                b.show_essential_menu(id, &anchor, x, y);
            }
        });
        button.add_controller(click);
        button
    }

    /// Goes to the tab that has the site, or opens it.
    pub(super) fn open_essential(self: &Rc<Self>, id: u32) {
        let Some(e) = self.essentials.borrow().get(id).cloned() else {
            return;
        };
        match self.essential_tab(&e) {
            Some(tab) => {
                let c = self.model.borrow_mut().select(tab);
                self.apply(c);
            }
            None => {
                self.open_tab(&e.url, None, true);
            }
        }
        self.schedule_essentials();
    }

    /// Adds the site of the page in front (or of `url`) to the essentials.
    pub(super) fn add_essential(self: &Rc<Self>, name: &str, url: &str) -> bool {
        if self.private {
            return false;
        }
        let added = self.essentials.borrow_mut().add(name, url).is_some();
        if added {
            self.save_essentials();
            self.refresh_essentials();
        }
        added
    }

    /// Menu → «Añadir a esenciales»: the page in front.
    pub(super) fn add_current_to_essentials(self: &Rc<Self>) {
        if let Some(view) = self.current_view() {
            self.add_view_to_essentials(&view);
        }
    }

    /// The site of a tab, from its menu.
    pub(super) fn add_tab_to_essentials(self: &Rc<Self>, id: TabId) {
        let view = self.views.borrow().get(&id).cloned();
        if let Some(view) = view {
            self.add_view_to_essentials(&view);
        }
    }

    fn add_view_to_essentials(self: &Rc<Self>, view: &WebView) {
        if self.private {
            return;
        }
        let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
        // The icon of the site is the one this page showed.
        if let (Some(tex), false) = (view.favicon(), url.is_empty()) {
            favicons::remember(&url, &tex);
        }
        let origin = url::Url::parse(&url)
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
            .map(|u| u.origin().ascii_serialization());
        let Some(origin) = origin else {
            return self.essential_alert(
                "Esta página no se puede añadir",
                "Solo las páginas web (http o https) pueden ser un esencial.",
            );
        };
        // An essential is a site: its address is the front door, not the page that was open.
        let short = site_name(&host_of(&origin));
        if !self.add_essential(&short, &origin) {
            self.essential_alert(
                "No se pudo añadir",
                "Ese sitio ya es un esencial, o ya hay el máximo.",
            );
        }
    }

    fn essential_alert(&self, heading: &str, body: &str) {
        crate::dialogs::notice(&self.window, heading, body);
    }

    /// One of the `essential.*` changes of the preferences page.
    pub(super) fn change_essential(self: &Rc<Self>, what: &str, value: &str) {
        if self.private {
            return;
        }
        match what {
            "add" => {
                let (url, name) = value.split_once('\n').unwrap_or((value, ""));
                if self.add_essential(name, url) {
                    self.redraw_settings();
                } else {
                    self.essential_alert(
                        "No se pudo añadir",
                        "La dirección no es válida, ya es un esencial o ya hay el máximo.",
                    );
                }
            }
            "suggest" => {
                self.add_essential("", value);
            }
            "remove" => {
                if let Ok(id) = value.parse() {
                    self.essentials.borrow_mut().remove(id);
                }
            }
            "up" | "down" => {
                if let Ok(id) = value.parse() {
                    self.essentials
                        .borrow_mut()
                        .shift(id, if what == "up" { -1 } else { 1 });
                }
            }
            _ => return,
        }
        self.save_essentials();
        self.refresh_essentials();
    }

    fn show_essential_menu(self: &Rc<Self>, id: u32, anchor: &gtk::Button, x: f64, y: f64) {
        let pop = gtk::Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let entry = |label: &str, f: Handler| {
            let btn = gtk::Button::builder()
                .label(label)
                .css_classes(["flat"])
                .build();
            if let Some(l) = btn.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let this = Rc::downgrade(self);
            let pop = pop.clone();
            btn.connect_clicked(move |_| {
                pop.popdown();
                if let Some(b) = this.upgrade() {
                    f(&b);
                }
            });
            col.append(&btn);
        };
        entry(
            "Abrir en una pestaña nueva",
            Box::new(move |b| {
                let url = b.essentials.borrow().get(id).map(|e| e.url.clone());
                if let Some(url) = url {
                    b.open_tab(&url, None, true);
                }
            }),
        );
        entry(
            "Mover antes",
            Box::new(move |b| b.change_essential("up", &id.to_string())),
        );
        entry(
            "Mover después",
            Box::new(move |b| b.change_essential("down", &id.to_string())),
        );
        entry(
            "Quitar de esenciales",
            Box::new(move |b| b.change_essential("remove", &id.to_string())),
        );
        pop.set_child(Some(&col));
        let pop2 = pop.clone();
        pop.connect_closed(move |_| pop2.unparent());
        pop.popup();
    }
}
