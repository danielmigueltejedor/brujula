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

/// The essential's app icon, if the icon theme has one.
fn theme_icon(e: &Essential) -> Option<&'static str> {
    let name = e.icon_name()?;
    let display = gdk::Display::default()?;
    gtk::IconTheme::for_display(&display)
        .has_icon(name)
        .then_some(name)
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
    /// Hidden pages that are fetching the icon of a site, and the hosts already tried.
    loaders: RefCell<Vec<WebView>>,
    tried: RefCell<std::collections::HashSet<String>>,
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
            loaders: RefCell::new(Vec::new()),
            tried: RefCell::new(std::collections::HashSet::new()),
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

    /// Looks up, once per site, the icon of an essential that has neither an app icon nor one
    /// the browser has seen: a hidden page loads the site only to read its icon.
    fn fetch_missing_icons(self: &Rc<Self>, list: &[Essential]) {
        if self.private {
            return;
        }
        for e in list {
            let host = e.host();
            if theme_icon(e).is_some()
                || favicons::lookup(&e.url).is_some()
                || !self.ess.tried.borrow_mut().insert(host)
            {
                continue;
            }
            self.fetch_icon(&e.url);
        }
    }

    fn fetch_icon(self: &Rc<Self>, url: &str) {
        let settings = webkit6::Settings::new();
        // Only the page's head is needed: no scripts, no media.
        settings.set_enable_javascript(false);
        settings.set_enable_media(false);
        let view = WebView::builder()
            .network_session(&self.session)
            .settings(&settings)
            .build();
        let this = Rc::downgrade(self);
        let page = url.to_owned();
        view.connect_favicon_notify(move |v| {
            if let (Some(tex), Some(b)) = (v.favicon(), this.upgrade()) {
                favicons::remember(&page, &tex);
                b.ess.loaders.borrow_mut().retain(|l| l != v);
                b.schedule_essentials();
            }
        });
        view.load_uri(url);
        self.ess.loaders.borrow_mut().push(view.clone());
        // A site that never shows an icon does not keep a page alive.
        let this = Rc::downgrade(self);
        glib::timeout_add_seconds_local_once(30, move || {
            if let Some(b) = this.upgrade() {
                b.ess.loaders.borrow_mut().retain(|l| *l != view);
            }
        });
    }

    pub(super) fn refresh_essentials(self: &Rc<Self>) {
        let list: Vec<Essential> = self.essentials.borrow().items().to_vec();
        let horizontal = self.settings.borrow().appearance.tabs == TabPosition::Horizontal;
        // The essential whose page is in front, if it is one.
        let front = {
            let m = self.model.borrow();
            m.selected()
                .and_then(|id| m.get(id))
                .and_then(|t| t.essential)
        };

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
            let active = front == Some(e.id);
            let open = self.model.borrow().essential_tab(e.id).is_some();
            self.ess.grid.append(&self.essential_tile(e, active, open));
            self.ess.rail.append(&self.essential_tile(e, active, open));
        }
        self.ess.colors.load_from_string(&css);
        self.fetch_missing_icons(&list);
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
        let face: gtk::Widget = match (theme_icon(e), favicons::lookup(&e.url)) {
            // The app's own icon from the icon theme, drawn as the app draws it.
            (Some(name), _) => {
                let icon = gtk::Image::from_icon_name(name);
                icon.set_pixel_size(28);
                icon.set_halign(gtk::Align::Center);
                icon.set_valign(gtk::Align::Center);
                icon.upcast()
            }
            (None, Some(texture)) => favicons::picture_sized(&texture, 24),
            (None, None) => {
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

    /// The essential's page is its own tab: it comes to the front, or opens the first time. It
    /// is not in the list of tabs.
    pub(super) fn open_essential(self: &Rc<Self>, id: u32) {
        let Some(e) = self.essentials.borrow().get(id).cloned() else {
            return;
        };
        let existing = self.model.borrow().essential_tab(id);
        match existing {
            Some(tab) => {
                // It lives in the space where it is wanted now.
                let space = self.model.borrow().current_space();
                self.model.borrow_mut().set_space(tab, space);
                let c = self.model.borrow_mut().select(tab);
                self.apply(c);
            }
            None => {
                let (tab, changes) = self.model.borrow_mut().open_essential(&e.url, id, true);
                self.create_view(tab, &e.url, None);
                self.apply(changes);
            }
        }
        self.schedule_essentials();
    }

    /// Whether `uri` is a place the essential of tab `id` should not be taken to: a link to
    /// another site is shown in a card over the page (what Shift+click does for any page).
    pub(super) fn leaves_essential(&self, id: TabId, uri: &str) -> bool {
        let home = {
            let m = self.model.borrow();
            m.get(id)
                .and_then(|t| t.essential)
                .and_then(|e| self.essentials.borrow().get(e).map(|e| e.url.clone()))
        };
        home.is_some_and(|home| {
            (uri.starts_with("https://") || uri.starts_with("http://"))
                && !brujula_core::webapps::same_site(&home, uri)
        })
    }

    /// Closes the essential's page (the icon stays).
    fn close_essential_page(self: &Rc<Self>, id: u32) {
        let tab = self.model.borrow().essential_tab(id);
        if let Some(tab) = tab {
            self.close_tab(tab);
        }
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

    pub(super) fn essential_alert(&self, heading: &str, body: &str) {
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
                    self.close_essential_page(id);
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
            append_popover_entry(self, &pop, &col, label, f);
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
        if self.model.borrow().essential_tab(id).is_some() {
            entry(
                "Cerrar la página",
                Box::new(move |b| b.close_essential_page(id)),
            );
        }
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
