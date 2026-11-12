//! The browser window: vertical tab sidebar, toolbar, web views.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use brujula_core::omnibox;
use brujula_core::session::Session;
use brujula_core::settings::Settings;
use brujula_core::tabs::{Change, TabId, TabModel};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use ndl::gtk;
use nodalia_gtk as ndl;
use webkit6::prelude::*;
use webkit6::{LoadEvent, NetworkSession, UserContentManager, WebView};

use crate::filters::Filters;
use crate::paths;

type Handler = Box<dyn Fn(&Rc<Browser>)>;

const CSS: &str = include_str!("style.css");

pub fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn install_app_actions(app: &gtk::Application) {
    app.set_accels_for_action("win.new-tab", &["<Ctrl>t"]);
    app.set_accels_for_action("win.close-tab", &["<Ctrl>w"]);
    app.set_accels_for_action("win.reopen-tab", &["<Ctrl><Shift>t"]);
    app.set_accels_for_action("win.focus-url", &["<Ctrl>l", "F6"]);
    app.set_accels_for_action("win.reload", &["<Ctrl>r", "F5"]);
    app.set_accels_for_action("win.back", &["<Alt>Left"]);
    app.set_accels_for_action("win.forward", &["<Alt>Right"]);
    app.set_accels_for_action("win.next-tab", &["<Ctrl>Tab", "<Ctrl>Page_Down"]);
    app.set_accels_for_action("win.prev-tab", &["<Ctrl><Shift>Tab", "<Ctrl>Page_Up"]);
    app.set_accels_for_action("win.toggle-sidebar", &["<Ctrl>b"]);
    app.set_accels_for_action("win.toggle-adblock", &["<Ctrl><Shift>b"]);
}

pub fn present(app: &gtk::Application, urls: &[String]) {
    let existing = BROWSER.with(|b| b.borrow().clone());
    if let Some(b) = existing {
        b.window.present();
        for u in urls {
            b.open_tab(u, None, true);
        }
        return;
    }
    let browser = Browser::new(app);
    BROWSER.with(|b| *b.borrow_mut() = Some(browser.clone()));
    browser.start(urls);
    browser.window.present();
}

thread_local! {
    /// The one browser window. A second launch hands its URLs to it.
    static BROWSER: RefCell<Option<Rc<Browser>>> = const { RefCell::new(None) };
}

struct Row {
    row: gtk::ListBoxRow,
    favicon: gtk::Image,
    title: gtk::Label,
    close: gtk::Button,
    audio: gtk::Image,
    pin: gtk::Image,
    spinner: gtk::Spinner,
}

pub struct Browser {
    window: ndl::ApplicationWindow,
    split: ndl::OverlaySplitView,
    list: gtk::ListBox,
    stack: gtk::Stack,
    entry: gtk::Entry,
    progress: gtk::ProgressBar,
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    shield: gtk::ToggleButton,

    session: NetworkSession,
    ucm: UserContentManager,
    filters: Rc<Filters>,

    settings: RefCell<Settings>,
    model: RefCell<TabModel>,
    views: RefCell<HashMap<TabId, WebView>>,
    rows: RefCell<HashMap<TabId, Row>>,
    /// True while we change the list selection ourselves.
    syncing: Cell<bool>,
}

impl Browser {
    fn new(app: &gtk::Application) -> Rc<Self> {
        let settings = std::fs::read_to_string(paths::settings_file())
            .map(|t| Settings::from_json(&t))
            .unwrap_or_default();

        let session = NetworkSession::new(
            Some(&paths::data_dir().to_string_lossy()),
            Some(&paths::cache_dir().to_string_lossy()),
        );
        if let Some(dm) = session.website_data_manager() {
            dm.set_favicons_enabled(true);
        }
        let ucm = UserContentManager::new();
        let filters = Filters::new(ucm.clone());

        let window = ndl::ApplicationWindow::new(app);
        window.set_title(Some("Brújula"));
        window.set_default_size(1280, 800);
        window.set_icon_name(Some(paths::APP_ID));

        // Sidebar: new tab button above the tab list.
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");
        list.add_css_class("brujula-tabs");
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let new_tab_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        new_tab_content.set_halign(gtk::Align::Center);
        new_tab_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        new_tab_content.append(&gtk::Label::new(Some("Nueva pestaña")));
        let new_tab = gtk::Button::builder()
            .child(&new_tab_content)
            .css_classes(["flat"])
            .action_name("win.new-tab")
            .build();
        new_tab.set_tooltip_text(Some("Nueva pestaña (Ctrl+T)"));
        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.add_css_class("ndl-island");
        side.set_size_request(240, -1);
        side.append(&scroller);
        side.append(&new_tab);

        // Content: header bar + page.
        let back = icon_button("go-previous-symbolic", "Atrás", "win.back");
        let forward = icon_button("go-next-symbolic", "Adelante", "win.forward");
        let reload = icon_button("view-refresh-symbolic", "Recargar", "win.reload");
        let sidebar_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Pestañas laterales (Ctrl+B)")
            .action_name("win.toggle-sidebar")
            .css_classes(["flat"])
            .build();
        let entry = gtk::Entry::builder()
            .placeholder_text("Buscar o escribir una dirección")
            .hexpand(true)
            .input_purpose(gtk::InputPurpose::Url)
            .build();
        entry.add_css_class("brujula-url");
        let shield = gtk::ToggleButton::builder()
            .icon_name("ndl-shield-check-symbolic")
            .action_name("win.toggle-adblock")
            .css_classes(["flat"])
            .build();
        shield.set_tooltip_text(Some("Bloqueador de anuncios (Ctrl+Mayús+B)"));
        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu())
            .css_classes(["flat"])
            .build();

        let header = ndl::HeaderBar::new();
        header.pack_start(&sidebar_toggle);
        header.pack_start(&back);
        header.pack_start(&forward);
        header.pack_start(&reload);
        header.set_title_widget(Some(&entry));
        header.pack_end(&menu);
        header.pack_end(&shield);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("osd");
        progress.add_css_class("brujula-progress");
        progress.set_visible(false);

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        let page = gtk::Overlay::new();
        page.set_child(Some(&stack));
        progress.set_valign(gtk::Align::Start);
        page.add_overlay(&progress);

        let toolbar = ndl::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&page));

        let split = ndl::OverlaySplitView::new();
        split.set_sidebar(Some(&side));
        split.set_content(Some(&toolbar));
        split.set_show_sidebar(settings.sidebar_visible);
        window.set_content(Some(&split));

        let this = Rc::new(Self {
            window,
            split,
            list,
            stack,
            entry,
            progress,
            back,
            forward,
            reload,
            shield,
            session,
            ucm,
            filters,
            settings: RefCell::new(settings),
            model: RefCell::new(TabModel::new()),
            views: RefCell::new(HashMap::new()),
            rows: RefCell::new(HashMap::new()),
            syncing: Cell::new(false),
        });
        this.wire(&sidebar_toggle);
        this
    }

    fn wire(self: &Rc<Self>, sidebar_toggle: &gtk::ToggleButton) {
        let w = self.window.upcast_ref::<gtk::ApplicationWindow>().clone();

        let action = |name: &str, f: Handler| {
            let a = gio::SimpleAction::new(name, None);
            let this = Rc::downgrade(self);
            a.connect_activate(move |_, _| {
                if let Some(b) = this.upgrade() {
                    f(&b);
                }
            });
            w.add_action(&a);
        };
        action(
            "new-tab",
            Box::new(|b| {
                let home = b.settings.borrow().homepage.clone();
                b.open_tab(&home, None, true);
                b.entry.grab_focus();
            }),
        );
        action(
            "close-tab",
            Box::new(|b| {
                if let Some(id) = b.model.borrow().selected() {
                    b.close_tab(id);
                }
            }),
        );
        action(
            "reopen-tab",
            Box::new(|b| {
                let reopened = b.model.borrow_mut().reopen();
                if let Some((id, changes)) = reopened {
                    let url = b
                        .model
                        .borrow()
                        .get(id)
                        .map(|t| t.url.clone())
                        .unwrap_or_default();
                    b.create_view(id, &url, None);
                    b.apply(changes);
                }
            }),
        );
        action(
            "focus-url",
            Box::new(|b| {
                b.entry.grab_focus();
                b.entry.select_region(0, -1);
            }),
        );
        action(
            "reload",
            Box::new(|b| {
                if let Some(v) = b.current_view() {
                    if v.is_loading() {
                        v.stop_loading()
                    } else {
                        v.reload()
                    }
                }
            }),
        );
        action(
            "back",
            Box::new(|b| {
                if let Some(v) = b.current_view() {
                    v.go_back()
                }
            }),
        );
        action(
            "forward",
            Box::new(|b| {
                if let Some(v) = b.current_view() {
                    v.go_forward()
                }
            }),
        );
        action(
            "next-tab",
            Box::new(|b| {
                let c = b.model.borrow_mut().select_relative(1);
                b.apply(c);
            }),
        );
        action(
            "prev-tab",
            Box::new(|b| {
                let c = b.model.borrow_mut().select_relative(-1);
                b.apply(c);
            }),
        );

        // Sidebar toggle is stateful so the button and Ctrl+B agree.
        let toggle = gio::SimpleAction::new_stateful(
            "toggle-sidebar",
            None,
            &self.split.shows_sidebar().to_variant(),
        );
        let this = Rc::downgrade(self);
        toggle.connect_activate(move |a, _| {
            if let Some(b) = this.upgrade() {
                let show = !b.split.shows_sidebar();
                b.split.set_show_sidebar(show);
                a.set_state(&show.to_variant());
                b.settings.borrow_mut().sidebar_visible = show;
                b.save_settings();
            }
        });
        w.add_action(&toggle);
        sidebar_toggle.set_active(self.split.shows_sidebar());

        let adblock = gio::SimpleAction::new_stateful(
            "toggle-adblock",
            None,
            &self.settings.borrow().adblock_enabled.to_variant(),
        );
        let this = Rc::downgrade(self);
        adblock.connect_activate(move |a, _| {
            if let Some(b) = this.upgrade() {
                let on = !b.settings.borrow().adblock_enabled;
                b.settings.borrow_mut().adblock_enabled = on;
                a.set_state(&on.to_variant());
                b.save_settings();
                if on {
                    let s = b.settings.borrow().clone();
                    b.filters.start(&s);
                } else {
                    b.filters.disable();
                }
                if let Some(v) = b.current_view() {
                    v.reload()
                }
            }
        });
        w.add_action(&adblock);
        self.shield
            .set_active(self.settings.borrow().adblock_enabled);

        // Address bar.
        let this = Rc::downgrade(self);
        self.entry.connect_activate(move |e| {
            let Some(b) = this.upgrade() else { return };
            let template = b.settings.borrow().search_template.clone();
            if let Some(url) = omnibox::resolve(&e.text(), &template) {
                if b.model.borrow().selected().is_none() {
                    b.open_tab(url.as_str(), None, true);
                } else if let Some(v) = b.current_view() {
                    v.load_uri(url.as_str());
                    v.grab_focus();
                }
            }
        });

        // Selecting a row selects the tab.
        let this = Rc::downgrade(self);
        self.list.connect_row_selected(move |_, row| {
            let Some(b) = this.upgrade() else { return };
            if b.syncing.get() {
                return;
            }
            let Some(row) = row else { return };
            let id = b
                .rows
                .borrow()
                .iter()
                .find(|(_, r)| &r.row == row)
                .map(|(id, _)| *id);
            if let Some(id) = id {
                let c = b.model.borrow_mut().select(id);
                b.apply(c);
            }
        });

        // New rules: hide ads on pages already open; if nothing was applied before, those
        // pages loaded unfiltered, so reload the one in front.
        let this = Rc::downgrade(self);
        self.filters.set_on_applied(move |first| {
            let Some(b) = this.upgrade() else { return };
            b.shield.set_tooltip_text(Some(&format!(
                "Bloqueador de anuncios activo: {} reglas (Ctrl+Mayús+B)",
                b.filters.rule_count()
            )));
            let views: Vec<WebView> = b.views.borrow().values().cloned().collect();
            for v in &views {
                hide_elements(&b.filters, v);
            }
            if first {
                if let Some(v) = b.current_view() {
                    v.reload();
                }
            }
        });

        // Save the session on close.
        let this = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(b) = this.upgrade() {
                b.save_session();
            }
            glib::Propagation::Proceed
        });
    }

    fn start(self: &Rc<Self>, urls: &[String]) {
        let this = self.clone();
        let urls = urls.to_vec();
        glib::spawn_future_local(async move {
            // Rules compiled last time go in first, so no page loads unfiltered.
            if this.settings.borrow().adblock_enabled && this.filters.load_compiled().await {
                this.shield.set_tooltip_text(Some(&format!(
                    "Bloqueador de anuncios activo: {} reglas (Ctrl+Mayús+B)",
                    this.filters.rule_count()
                )));
            }
            this.restore(&urls);
            if this.settings.borrow().adblock_enabled {
                let s = this.settings.borrow().clone();
                this.filters.start(&s);
            }
        });
    }

    fn restore(self: &Rc<Self>, urls: &[String]) {
        let session = std::fs::read_to_string(paths::session_file())
            .map(|t| Session::from_json(&t))
            .unwrap_or_default();

        let mut opened = Vec::new();
        for t in &session.tabs {
            let (id, ch) = self.model.borrow_mut().open(&t.url, None, false);
            if t.pinned {
                self.model.borrow_mut().set_pinned(id, true);
            }
            self.model
                .borrow_mut()
                .update(id, |tab| tab.title = t.title.clone());
            self.create_view(id, &t.url, None);
            self.apply(ch);
            opened.push(id);
        }
        if let Some(id) = opened.get(session.selected) {
            let c = self.model.borrow_mut().select(*id);
            self.apply(c);
        }
        for u in urls {
            self.open_tab(u, None, true);
        }
        if self.model.borrow().is_empty() {
            let home = self.settings.borrow().homepage.clone();
            self.open_tab(&home, None, true);
            self.entry.grab_focus();
        }
    }

    fn save_settings(&self) {
        let _ = paths::write_atomic(&paths::settings_file(), &self.settings.borrow().to_json());
    }

    fn save_session(&self) {
        let s = Session::capture(&self.model.borrow());
        let _ = paths::write_atomic(&paths::session_file(), &s.to_json());
    }

    fn current_view(&self) -> Option<WebView> {
        let id = self.model.borrow().selected()?;
        self.views.borrow().get(&id).cloned()
    }

    pub fn open_tab(self: &Rc<Self>, url: &str, after: Option<TabId>, select: bool) {
        let url = if url.trim().is_empty() {
            "about:blank".to_owned()
        } else {
            let template = self.settings.borrow().search_template.clone();
            omnibox::resolve(url, &template)
                .map_or_else(|| "about:blank".to_owned(), |u| u.to_string())
        };
        let (id, changes) = self.model.borrow_mut().open(&url, after, select);
        self.create_view(id, &url, None);
        self.apply(changes);
    }

    fn close_tab(self: &Rc<Self>, id: TabId) {
        let changes = self.model.borrow_mut().close(id);
        self.apply(changes);
        if self.model.borrow().is_empty() {
            self.save_session();
            self.window.close();
        }
    }

    fn create_view(self: &Rc<Self>, id: TabId, url: &str, related: Option<&WebView>) -> WebView {
        let mut b = WebView::builder()
            .network_session(&self.session)
            .user_content_manager(&self.ucm)
            .hexpand(true)
            .vexpand(true);
        if let Some(r) = related {
            b = b.related_view(r);
        }
        let view = b.build();
        if let Some(s) = WebViewExt::settings(&view) {
            s.set_enable_developer_extras(true);
            s.set_enable_back_forward_navigation_gestures(true);
        }

        let this = Rc::downgrade(self);
        view.connect_title_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if let Some(id) = b.id_of(v) {
                    let title = v.title().map(|t| t.to_string()).unwrap_or_default();
                    let c = b.model.borrow_mut().update(id, |t| t.title = title);
                    b.apply(c);
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_uri_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if let Some(id) = b.id_of(v) {
                    let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
                    let c = b.model.borrow_mut().update(id, |t| t.url = uri);
                    b.apply(c);
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_is_loading_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if let Some(id) = b.id_of(v) {
                    let loading = v.is_loading();
                    let c = b.model.borrow_mut().update(id, |t| t.loading = loading);
                    b.apply(c);
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_is_playing_audio_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if let Some(id) = b.id_of(v) {
                    let audible = v.is_playing_audio();
                    let c = b.model.borrow_mut().update(id, |t| t.audible = audible);
                    b.apply(c);
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_favicon_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if let Some(id) = b.id_of(v) {
                    b.sync_row(id);
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_estimated_load_progress_notify(move |v| {
            if let Some(b) = this.upgrade() {
                if b.current_view().as_ref() == Some(v) {
                    b.sync_progress();
                }
            }
        });
        let this = Rc::downgrade(self);
        view.connect_load_changed(move |v, ev| {
            if let Some(b) = this.upgrade() {
                if ev == LoadEvent::Committed || ev == LoadEvent::Finished {
                    b.update_nav();
                }
                if ev == LoadEvent::Finished {
                    hide_elements(&b.filters, v);
                }
            }
        });
        // window.open / target=_blank: a new tab next to this one.
        let this = Rc::downgrade(self);
        view.connect_create(move |v, _action| {
            let b = this.upgrade()?;
            let opener = b.id_of(v);
            let (id, changes) = b.model.borrow_mut().open("", opener, true);
            let nv = b.create_view(id, "", Some(v));
            b.apply(changes);
            Some(nv.upcast())
        });
        // Downloads go to the user's Downloads folder, without a prompt.
        let this = Rc::downgrade(self);
        self.session.connect_download_started(move |_, download| {
            let _ = &this;
            download.connect_decide_destination(|d, suggested| {
                let dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                    .unwrap_or_else(glib::home_dir);
                let name = if suggested.is_empty() {
                    "download"
                } else {
                    suggested
                };
                let name = name.replace(['/', '\\'], "_");
                let mut path = dir.join(&name);
                let mut n = 1;
                while path.exists() {
                    path = dir.join(format!("{n}-{name}"));
                    n += 1;
                }
                d.set_destination(&path.to_string_lossy());
                true
            });
        });

        self.stack.add_named(&view, Some(&format!("tab-{}", id.0)));
        self.views.borrow_mut().insert(id, view.clone());
        if !url.is_empty() && url != "about:blank" {
            view.load_uri(url);
        }
        view
    }

    fn id_of(&self, view: &WebView) -> Option<TabId> {
        self.views
            .borrow()
            .iter()
            .find(|(_, v)| *v == view)
            .map(|(id, _)| *id)
    }

    /// Brings the widgets in line with the model after a change.
    fn apply(self: &Rc<Self>, changes: Vec<Change>) {
        for change in changes {
            match change {
                Change::Opened(id) => self.add_row(id),
                Change::Closed(id) => {
                    if let Some(r) = self.rows.borrow_mut().remove(&id) {
                        self.list.remove(&r.row);
                    }
                    if let Some(v) = self.views.borrow_mut().remove(&id) {
                        self.stack.remove(&v);
                    }
                }
                Change::Moved(_) => self.reorder_rows(),
                Change::Updated(id) => {
                    self.sync_row(id);
                    if self.model.borrow().selected() == Some(id) {
                        self.sync_chrome();
                    }
                }
                Change::Selected(id) => {
                    self.stack.set_visible_child_name(&format!("tab-{}", id.0));
                    self.syncing.set(true);
                    if let Some(r) = self.rows.borrow().get(&id) {
                        self.list.select_row(Some(&r.row));
                    }
                    self.syncing.set(false);
                    self.sync_chrome();
                    if let Some(v) = self.views.borrow().get(&id) {
                        v.grab_focus();
                    }
                }
            }
        }
        if self.model.borrow().is_empty() {
            return;
        }
        // Opened tabs appear at their model position.
        self.reorder_rows();
    }

    fn add_row(self: &Rc<Self>, id: TabId) {
        let favicon = gtk::Image::from_icon_name("web-browser-symbolic");
        favicon.add_css_class("brujula-tab-favicon");
        favicon.set_pixel_size(16);
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["brujula-tab-title"])
            .build();
        let audio = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        audio.set_pixel_size(14);
        audio.set_visible(false);
        let pin = gtk::Image::from_icon_name("ndl-pin-symbolic");
        pin.set_pixel_size(14);
        pin.set_visible(false);
        let close = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "brujula-tab-close"])
            .tooltip_text("Cerrar pestaña")
            .valign(gtk::Align::Center)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.add_css_class("brujula-tab");
        content.append(&favicon);
        content.append(&spinner);
        content.append(&title);
        content.append(&audio);
        content.append(&pin);
        content.append(&close);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&content));

        let this = Rc::downgrade(self);
        close.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.close_tab(id);
            }
        });

        // Middle click closes; right click opens the tab menu.
        let click = gtk::GestureClick::builder().button(0).build();
        let this = Rc::downgrade(self);
        let row_w = row.clone();
        click.connect_pressed(move |g, _, x, y| {
            let Some(b) = this.upgrade() else { return };
            match g.current_button() {
                gdk::BUTTON_MIDDLE => b.close_tab(id),
                gdk::BUTTON_SECONDARY => b.show_tab_menu(id, &row_w, x, y),
                _ => {}
            }
        });
        row.add_controller(click);

        self.list.append(&row);
        self.rows.borrow_mut().insert(
            id,
            Row {
                row,
                favicon,
                title,
                close,
                audio,
                pin,
                spinner,
            },
        );
        self.sync_row(id);
    }

    fn reorder_rows(&self) {
        let model = self.model.borrow();
        let rows = self.rows.borrow();
        for (i, tab) in model.tabs().iter().enumerate() {
            if let Some(r) = rows.get(&tab.id) {
                if r.row.index() != i as i32 {
                    self.list.remove(&r.row);
                    self.list.insert(&r.row, i as i32);
                }
            }
        }
        // Moving rows drops the selection in GTK; put it back.
        if let Some(sel) = model.selected() {
            if let Some(r) = rows.get(&sel) {
                self.syncing.set(true);
                self.list.select_row(Some(&r.row));
                self.syncing.set(false);
            }
        }
    }

    fn sync_row(&self, id: TabId) {
        let model = self.model.borrow();
        let (Some(tab), rows) = (model.get(id), self.rows.borrow()) else {
            return;
        };
        let Some(r) = rows.get(&id) else { return };
        let title = if !tab.title.is_empty() {
            tab.title.clone()
        } else if tab.url.is_empty() || tab.url == "about:blank" {
            "Nueva pestaña".to_owned()
        } else {
            host_of(&tab.url)
        };
        r.title.set_label(&title);
        r.title.set_tooltip_text(Some(&title));
        r.spinner.set_visible(tab.loading);
        r.spinner.set_spinning(tab.loading);
        r.favicon.set_visible(!tab.loading);
        r.audio.set_visible(tab.audible);
        r.close.set_visible(!tab.pinned);
        r.pin.set_visible(tab.pinned);
        if let Some(v) = self.views.borrow().get(&id) {
            match v.favicon() {
                Some(tex) => r.favicon.set_paintable(Some(&tex)),
                None => r.favicon.set_icon_name(Some("web-browser-symbolic")),
            }
        }
    }

    /// Address bar, navigation buttons, window title, progress for the selected tab.
    fn sync_chrome(&self) {
        let model = self.model.borrow();
        let Some(tab) = model.selected().and_then(|id| model.get(id)) else {
            return;
        };
        if !self.entry.has_focus() {
            let shown = if tab.url == "about:blank" {
                ""
            } else {
                tab.url.as_str()
            };
            self.entry.set_text(shown);
        }
        let title = if tab.title.is_empty() {
            "Brújula".to_owned()
        } else {
            format!("{} – Brújula", tab.title)
        };
        self.window.set_title(Some(&title));
        self.reload.set_icon_name(if tab.loading {
            "ndl-stop-symbolic"
        } else {
            "view-refresh-symbolic"
        });
        drop(model);
        self.update_nav();
        self.sync_progress();
    }

    fn update_nav(&self) {
        if let Some(v) = self.current_view() {
            self.back.set_sensitive(v.can_go_back());
            self.forward.set_sensitive(v.can_go_forward());
        }
    }

    fn sync_progress(&self) {
        match self.current_view() {
            Some(v) if v.is_loading() => {
                self.progress.set_visible(true);
                self.progress.set_fraction(v.estimated_load_progress());
            }
            _ => self.progress.set_visible(false),
        }
    }

    fn show_tab_menu(self: &Rc<Self>, id: TabId, row: &gtk::ListBoxRow, x: f64, y: f64) {
        let pinned = self.model.borrow().get(id).is_some_and(|t| t.pinned);
        let pop = gtk::Popover::new();
        pop.set_parent(row);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let this = Rc::downgrade(self);
        let add = |label: &str, f: Handler| {
            let btn = gtk::Button::builder()
                .label(label)
                .css_classes(["flat"])
                .build();
            if let Some(l) = btn.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let this = this.clone();
            let pop = pop.clone();
            btn.connect_clicked(move |_| {
                pop.popdown();
                if let Some(b) = this.upgrade() {
                    f(&b);
                }
            });
            col.append(&btn);
        };
        add(
            if pinned {
                "Desanclar pestaña"
            } else {
                "Anclar pestaña"
            },
            Box::new(move |b| {
                let c = b.model.borrow_mut().set_pinned(id, !pinned);
                b.apply(c);
            }),
        );
        add(
            "Recargar",
            Box::new(move |b| {
                if let Some(v) = b.views.borrow().get(&id) {
                    v.reload()
                }
            }),
        );
        add(
            "Duplicar",
            Box::new(move |b| {
                let url = b
                    .model
                    .borrow()
                    .get(id)
                    .map(|t| t.url.clone())
                    .unwrap_or_default();
                b.open_tab(&url, Some(id), true);
            }),
        );
        add("Cerrar", Box::new(move |b| b.close_tab(id)));
        add(
            "Cerrar las demás",
            Box::new(move |b| {
                let ids = b
                    .model
                    .borrow()
                    .tabs()
                    .iter()
                    .filter(|t| t.id != id && !t.pinned)
                    .map(|t| t.id)
                    .collect::<Vec<_>>();
                for i in ids {
                    b.close_tab(i);
                }
            }),
        );
        add(
            "Cerrar las de abajo",
            Box::new(move |b| {
                let ids = {
                    let m = b.model.borrow();
                    let pos = m.position(id).map_or(0, |p| p + 1);
                    m.tabs()[pos..]
                        .iter()
                        .filter(|t| !t.pinned)
                        .map(|t| t.id)
                        .collect::<Vec<_>>()
                };
                for i in ids {
                    b.close_tab(i);
                }
            }),
        );
        pop.set_child(Some(&col));
        let pop2 = pop.clone();
        pop.connect_closed(move |_| pop2.unparent());
        pop.popup();
    }
}

fn icon_button(icon: &str, tip: &str, action: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .action_name(action)
        .css_classes(["flat"])
        .build()
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_owned())
        })
        .unwrap_or_else(|| url.to_owned())
}

fn main_menu() -> gio::Menu {
    let m = gio::Menu::new();
    let tabs = gio::Menu::new();
    tabs.append(Some("Nueva pestaña"), Some("win.new-tab"));
    tabs.append(Some("Reabrir pestaña cerrada"), Some("win.reopen-tab"));
    m.append_section(None, &tabs);
    let opts = gio::Menu::new();
    opts.append(Some("Pestañas laterales"), Some("win.toggle-sidebar"));
    opts.append(Some("Bloqueador de anuncios"), Some("win.toggle-adblock"));
    m.append_section(None, &opts);
    m
}

/// Hides the page's ads: site-specific rules at once, generic rules for the classes and ids the
/// page actually has.
pub fn hide_elements(filters: &Rc<Filters>, view: &WebView) {
    let Some(uri) = view.uri() else { return };
    if !(uri.starts_with("http://") || uri.starts_with("https://")) {
        return;
    }
    let Some((cosmetic, page)) = filters.cosmetic_for_page(&uri) else {
        return;
    };
    let view = view.clone();
    glib::spawn_future_local(async move {
        let mut css = page.css.clone();
        if !page.generichide {
            const COLLECT: &str = "(function(){const c=new Set(),i=new Set();\
                document.querySelectorAll('[class]').forEach(e=>e.classList.forEach(x=>c.add(x)));\
                document.querySelectorAll('[id]').forEach(e=>i.add(e.id));\
                return JSON.stringify([[...c],[...i]]);})()";
            if let Ok(v) = view
                .evaluate_javascript_future(COLLECT, Some("brujula"), None)
                .await
            {
                if let Ok(parsed) = serde_json::from_str::<(Vec<String>, Vec<String>)>(&v.to_str())
                {
                    css.push_str(&cosmetic.generic_css(parsed.0, parsed.1, &page.exceptions));
                }
            }
        }
        if css.is_empty() {
            return;
        }
        let literal = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
        let script = format!(
            "(function(css){{let s=document.getElementById('brujula-cosmetic');\
             if(!s){{s=document.createElement('style');s.id='brujula-cosmetic';\
             (document.head||document.documentElement).appendChild(s);}}s.textContent=css;}})({literal})"
        );
        let _ = view
            .evaluate_javascript_future(&script, Some("brujula"), None)
            .await;
    });
}
