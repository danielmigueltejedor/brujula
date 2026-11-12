//! The browser window: vertical tab sidebar, toolbar, web views.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use brujula_core::adblock;
use brujula_core::appearance;
use brujula_core::archive::Archive;
use brujula_core::bookmarks::now_ms;
use brujula_core::bookmarks::Bookmarks;
use brujula_core::downloads;
use brujula_core::edge::EdgeDetector;
use brujula_core::folders::{Folders, NO_FOLDER};
use brujula_core::newtab;
use brujula_core::omnibox;
use brujula_core::prefs;
use brujula_core::search;
use brujula_core::session::Session;
use brujula_core::settings::{
    text_on, Layout, Settings, TabPosition, Theme, ACCENT_DEFAULT, SIDEBAR_WIDTH_DEFAULT,
    SIDEBAR_WIDTH_MIN,
};
use brujula_core::spaces::Spaces;
use brujula_core::suggest::{self, Action, Kind};
use brujula_core::tabs::{Change, TabId, TabModel};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use ndl::gtk;
use nodalia_gtk as ndl;
use webkit6::prelude::*;
use webkit6::{
    LoadEvent, NavigationPolicyDecision, NavigationType, NetworkSession, PolicyDecisionType,
    UserContentManager, WebView,
};

use crate::favicons;
use crate::filters::Filters;
use crate::paths;
use crate::spotlight::{Item, Spotlight};

mod archive;
mod cloudsync;
mod extras;
mod folders;
mod peek;
mod split;

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
    app.set_accels_for_action("win.open-spotlight", &["<Ctrl>l", "F6"]);
    app.set_accels_for_action("win.reload", &["<Ctrl>r", "F5"]);
    app.set_accels_for_action("win.back", &["<Alt>Left"]);
    app.set_accels_for_action("win.forward", &["<Alt>Right"]);
    app.set_accels_for_action("win.next-tab", &["<Ctrl>Tab", "<Ctrl>Page_Down"]);
    app.set_accels_for_action("win.prev-tab", &["<Ctrl><Shift>Tab", "<Ctrl>Page_Up"]);
    app.set_accels_for_action("win.toggle-sidebar", &["<Ctrl>b"]);
    app.set_accels_for_action("win.open-settings", &["<Ctrl>comma"]);
    app.set_accels_for_action("win.toggle-bookmark", &["<Ctrl>d"]);
    app.set_accels_for_action("win.next-space", &["<Alt>Down"]);
    app.set_accels_for_action("win.prev-space", &["<Alt>Up"]);
    app.set_accels_for_action("win.toggle-site-adblock", &["<Ctrl><Shift>b"]);
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
    /// Always 16x16: its size comes from an empty spacer, and the site's icon
    /// (`favicon_pic`) or the generic one (`favicon_icon`) is laid over it and scaled to fit, so
    /// a large favicon cannot spill out of the row.
    favicon: gtk::Overlay,
    favicon_pic: gtk::Picture,
    favicon_icon: gtk::Image,
    title: gtk::Label,
    close: gtk::Button,
    audio: gtk::Image,
    pin: gtk::Image,
    spinner: gtk::Spinner,
}

/// The buttons that sit above the page. Built twice: in the sidebar, and in a slim bar that
/// shows only while the sidebar is hidden. They share actions, so they stay in step.
struct Chrome {
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    shield: gtk::ToggleButton,
    star: gtk::ToggleButton,
    toggle: gtk::ToggleButton,
    menu: gtk::MenuButton,
    url_button: gtk::Button,
    url_icon: gtk::Image,
    url_label: gtk::Label,
}

fn build_chrome() -> Chrome {
    let url_icon = gtk::Image::from_icon_name("ndl-search-symbolic");
    url_icon.set_pixel_size(14);
    let url_label = gtk::Label::builder()
        .label(URL_PLACEHOLDER)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let url_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    url_content.append(&url_icon);
    url_content.append(&url_label);
    let url_button = gtk::Button::builder()
        .child(&url_content)
        .action_name("win.open-spotlight")
        .css_classes(["flat", "brujula-urlbutton"])
        .hexpand(true)
        .build();
    url_button.set_tooltip_text(Some("Buscar o escribir una dirección (Ctrl+L)"));

    let shield = gtk::ToggleButton::builder()
        .icon_name("ndl-shield-check-symbolic")
        .action_name("win.toggle-site-adblock")
        .css_classes(["flat"])
        .build();
    shield.set_tooltip_text(Some("Bloqueador de anuncios (Ctrl+Mayús+B)"));
    let star = gtk::ToggleButton::builder()
        .icon_name("ndl-star-symbolic")
        .action_name("win.toggle-bookmark")
        .css_classes(["flat"])
        .build();
    star.set_tooltip_text(Some("Añadir a marcadores (Ctrl+D)"));
    let toggle = gtk::ToggleButton::builder()
        .icon_name("ndl-sidebar-symbolic")
        .tooltip_text("Barra lateral (Ctrl+B)")
        .action_name("win.toggle-sidebar")
        .css_classes(["flat"])
        .build();
    let menu = gtk::MenuButton::builder()
        .icon_name("ndl-menu-symbolic")
        .menu_model(&main_menu())
        .css_classes(["flat"])
        .build();
    Chrome {
        back: icon_button("ndl-arrow-left-symbolic", "Atrás (Alt+←)", "win.back"),
        forward: icon_button(
            "ndl-arrow-right-symbolic",
            "Adelante (Alt+→)",
            "win.forward",
        ),
        reload: icon_button("ndl-refresh-symbolic", "Recargar (Ctrl+R)", "win.reload"),
        shield,
        star,
        toggle,
        menu,
        url_button,
        url_icon,
        url_label,
    }
}

const URL_PLACEHOLDER: &str = "Buscar o escribir una dirección";

pub struct Browser {
    window: ndl::ApplicationWindow,
    split: ndl::OverlaySplitView,
    /// The window-controls bar, shown while the pointer is at the top edge.
    bar_revealer: gtk::Revealer,
    top_hot: gtk::Box,
    /// Wakes the sidebar when it is not pinned.
    left_hot: gtk::Box,
    side: gtk::Box,
    /// Tab bar and navigation row of the horizontal layout.
    topbar: gtk::Box,
    strip: gtk::Box,
    /// The space switcher at the foot of the sidebar.
    space_bar: gtk::Box,
    bookmarks: RefCell<Bookmarks>,
    spaces: RefCell<Spaces>,
    folders: RefCell<Folders>,
    /// The rows of the folders in the sidebar list, between the rows of their tabs.
    folder_rows: RefCell<HashMap<u32, folders::FolderRow>>,
    /// The button that makes a folder.
    folder_button: gtk::Button,
    /// Set while a save is queued, so a burst of changes writes the session once.
    save_queued: Cell<bool>,
    /// The card that shows a link in a preview, and the page in it.
    peek: peek::PeekCard,
    peek_view: RefCell<Option<WebView>>,
    /// The two tabs shown side by side, and the `Paned` that holds their pages.
    paired: RefCell<Option<(TabId, TabId)>>,
    split_paned: gtk::Paned,
    /// Tabs closed because they were not used for long.
    archive: RefCell<Archive>,
    /// How the last synchronisation round went, in words for the preferences page.
    sync_status: RefCell<String>,
    /// True while a round is reading and writing the cloud folder.
    sync_busy: Cell<bool>,
    /// What this device last wrote to the cloud, so an unchanged state is not uploaded again.
    sync_sent: RefCell<String>,
    /// The other devices' files as of the last round.
    sync_others: RefCell<Vec<brujula_core::sync::Snapshot>>,
    /// The tab each space had in front when it was left.
    last_in_space: RefCell<HashMap<u32, TabId>>,
    /// The page's column; its width keeps the split view's minimum honest.
    content: ndl::ToolbarView,
    /// Colours, radius, gap and transparency the user chose, on top of the built-in sheet.
    appearance_css: gtk::CssProvider,
    /// Tells a pointer heading for the left edge from one just passing by.
    edge: RefCell<EdgeDetector>,
    /// The preferences section to scroll to once the page has loaded.
    pending_section: RefCell<Option<String>>,
    /// Restored tabs that have not been opened yet: their page loads when they are selected, so
    /// starting with many tabs does not start many page loads.
    lazy: RefCell<HashMap<TabId, String>>,
    /// True while the saved tabs are being put back; the tab that ends up in front loads then.
    restoring: Cell<bool>,
    hide_bar: RefCell<Option<glib::SourceId>>,
    hide_side: RefCell<Option<glib::SourceId>>,
    list: gtk::ListBox,
    stack: gtk::Stack,
    progress: gtk::ProgressBar,
    chromes: Vec<Chrome>,
    spotlight: Rc<Spotlight>,
    /// Where the Spotlight sends its result: the current tab, or a new one.
    session: NetworkSession,
    ucm: UserContentManager,
    filters: Rc<Filters>,

    settings: RefCell<Settings>,
    model: RefCell<TabModel>,
    views: RefCell<HashMap<TabId, WebView>>,
    rows: RefCell<HashMap<TabId, Row>>,
    /// True while we change the list selection ourselves.
    syncing: Cell<bool>,
    /// Set when the rules are being rebuilt for a site switch; the page reloads once they land.
    pending_reload: Cell<bool>,
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
        // A bad certificate is a failure, never a click-through; trackers get no free ride.
        session.set_tls_errors_policy(webkit6::TLSErrorsPolicy::Fail);
        session.set_itp_enabled(true);
        let ucm = UserContentManager::new();
        let filters = Filters::new(ucm.clone());

        let window = ndl::ApplicationWindow::new(app);
        window.set_title(Some("Brújula"));
        window.set_default_size(1280, 800);
        window.set_icon_name(Some(paths::APP_ID));

        let side_chrome = build_chrome();
        let slim_chrome = build_chrome();
        let top_chrome = build_chrome();

        // Sidebar, top to bottom: window controls, navigation, address, tabs, new tab.
        let side_header = ndl::HeaderBar::new();
        side_header.set_show_end_title_buttons(false);
        side_header.set_show_start_title_buttons(false);
        side_header.pack_end(&side_chrome.toggle);
        // The window's title says nothing the tab does not, and takes room the sidebar needs.
        side_header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));

        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        nav.add_css_class("brujula-nav");
        nav.set_margin_start(8);
        nav.set_margin_end(8);
        nav.set_margin_bottom(6);
        nav.append(&side_chrome.back);
        nav.append(&side_chrome.forward);
        nav.append(&side_chrome.reload);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        nav.append(&spacer);
        nav.append(&side_chrome.star);
        nav.append(&side_chrome.shield);
        nav.append(&side_chrome.menu);

        side_chrome.url_button.set_margin_start(8);
        side_chrome.url_button.set_margin_end(8);
        side_chrome.url_button.set_margin_bottom(6);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");
        list.add_css_class("brujula-tabs");
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .margin_start(8)
            .margin_end(8)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let new_tab_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        new_tab_content.set_halign(gtk::Align::Center);
        new_tab_content.append(&gtk::Image::from_icon_name("ndl-plus-symbolic"));
        new_tab_content.append(&gtk::Label::new(Some("Nueva pestaña")));
        let new_tab = gtk::Button::builder()
            .child(&new_tab_content)
            .css_classes(["flat", "brujula-newtab"])
            .action_name("win.new-tab")
            .build();
        new_tab.set_tooltip_text(Some("Nueva pestaña (Ctrl+T)"));

        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.add_css_class("ndl-island");
        side.add_css_class("brujula-side");
        side.set_size_request(i32::try_from(SIDEBAR_WIDTH_MIN).unwrap_or(180), -1);
        side.append(&side_header);
        side.append(&nav);
        side.append(&side_chrome.url_button);
        side.append(&scroller);
        let folder_button = gtk::Button::builder()
            .icon_name("ndl-folder-plus-symbolic")
            .css_classes(["flat", "brujula-newfolder"])
            .tooltip_text("Nueva carpeta")
            .build();
        new_tab.set_hexpand(true);
        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bottom.append(&new_tab);
        bottom.append(&folder_button);
        bottom.set_margin_end(8);
        side.append(&bottom);
        let space_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        space_bar.add_css_class("brujula-spaces");
        space_bar.set_halign(gtk::Align::Center);
        space_bar.set_margin_top(4);
        space_bar.set_margin_bottom(2);
        side.append(&space_bar);

        // Window controls live in a bar that slides down from the top edge of the page when
        // the pointer gets there, top right like on every desktop. Nothing else sits over the
        // page: with the sidebar hidden the site fills the whole window.
        let bar = ndl::HeaderBar::new();
        bar.add_css_class("brujula-floatbar");
        bar.set_show_start_title_buttons(false);
        bar.set_show_end_title_buttons(true);
        bar.pack_start(&slim_chrome.toggle);
        let bar_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(160)
            .valign(gtk::Align::Start)
            .halign(gtk::Align::Fill)
            .child(&bar)
            .build();
        // A few pixels along the top edge, and along the left edge, wake the bar and the sidebar.
        let top_hot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        top_hot.set_size_request(-1, 4);
        top_hot.set_valign(gtk::Align::Start);
        top_hot.set_halign(gtk::Align::Fill);
        let left_hot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left_hot.set_size_request(6, -1);
        left_hot.set_halign(gtk::Align::Start);
        left_hot.set_valign(gtk::Align::Fill);
        left_hot.set_visible(!settings.sidebar_visible);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("osd");
        progress.add_css_class("brujula-progress");
        progress.set_visible(false);

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        let split_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        split_paned.add_css_class("brujula-splitview");
        split_paned.set_resize_start_child(true);
        split_paned.set_resize_end_child(true);
        split_paned.set_shrink_start_child(false);
        split_paned.set_shrink_end_child(false);
        stack.add_named(&split_paned, Some("split"));
        let page = gtk::Overlay::new();
        page.set_child(Some(&stack));
        progress.set_valign(gtk::Align::Start);
        page.add_overlay(&progress);
        let peek = peek::build_peek();
        page.add_overlay(&peek.layer);

        // The tab bar for the horizontal layout: the tabs and the window controls on one row, the
        // navigation and the address on the next. Hidden while the tabs are in the sidebar.
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        strip.add_css_class("brujula-strip");
        let strip_scroll = gtk::ScrolledWindow::builder()
            .child(&strip)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let top_header = ndl::HeaderBar::new();
        top_header.set_show_start_title_buttons(false);
        top_header.set_show_end_title_buttons(true);
        top_header.set_title_widget(Some(&strip_scroll));
        top_header.pack_end(&icon_button(
            "ndl-plus-symbolic",
            "Nueva pestaña (Ctrl+T)",
            "win.new-tab",
        ));
        let top_nav = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        top_nav.set_margin_start(8);
        top_nav.set_margin_end(8);
        top_nav.set_margin_bottom(6);
        top_nav.append(&top_chrome.back);
        top_nav.append(&top_chrome.forward);
        top_nav.append(&top_chrome.reload);
        top_chrome.url_button.set_margin_start(8);
        top_chrome.url_button.set_margin_end(8);
        top_nav.append(&top_chrome.url_button);
        top_nav.append(&top_chrome.star);
        top_nav.append(&top_chrome.shield);
        top_nav.append(&top_chrome.menu);
        let topbar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        topbar.add_css_class("brujula-topbar");
        topbar.append(&top_header);
        topbar.append(&top_nav);
        topbar.set_visible(false);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&topbar);
        column.append(&page);
        page.set_vexpand(true);

        let toolbar = ndl::ToolbarView::new();
        toolbar.set_content(Some(&column));
        toolbar.set_reveal_top_bars(false);
        // The split view reports a natural width smaller than its minimum when it is not pinned
        // (GTK warns about it) unless the content says it is at least as wide as the sidebar.
        toolbar.set_size_request(i32::try_from(SIDEBAR_WIDTH_MIN).unwrap_or(180) + 80, -1);

        let split = ndl::OverlaySplitView::new();
        split.set_min_sidebar_width(f64::from(SIDEBAR_WIDTH_MIN));
        split.set_max_sidebar_width(f64::from(SIDEBAR_WIDTH_DEFAULT));
        split.set_sidebar(Some(&side));
        split.set_content(Some(&toolbar));
        // Pinned: the sidebar sits beside the page. Not pinned: it floats over the page and only
        // shows while the pointer is at the left edge.
        split.set_collapsed(!settings.sidebar_visible);
        split.set_show_sidebar(settings.sidebar_visible);

        // The Spotlight sits over everything. Its query and result are wired in `wire`, once
        // there is an `Rc<Browser>` to hold on to.
        let weak: Rc<RefCell<Option<std::rc::Weak<Browser>>>> = Rc::new(RefCell::new(None));
        let w1 = weak.clone();
        let w2 = weak.clone();
        let spotlight = Spotlight::new(
            move |text| {
                w1.borrow()
                    .as_ref()
                    .and_then(std::rc::Weak::upgrade)
                    .map(|b| b.spotlight_items(text))
                    .unwrap_or_default()
            },
            move |action| {
                if let Some(b) = w2.borrow().as_ref().and_then(std::rc::Weak::upgrade) {
                    b.run_action(action);
                }
            },
        );
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&split));
        overlay.add_overlay(&top_hot);
        overlay.add_overlay(&left_hot);
        overlay.add_overlay(&bar_revealer);
        overlay.add_overlay(&spotlight.root);
        window.set_content(Some(&overlay));

        let this = Rc::new(Self {
            window,
            split,
            bar_revealer,
            top_hot,
            left_hot,
            side,
            topbar,
            strip,
            space_bar,
            bookmarks: RefCell::new(
                std::fs::read_to_string(paths::bookmarks_file())
                    .map(|t| Bookmarks::from_json(&t))
                    .unwrap_or_default(),
            ),
            spaces: RefCell::new(Spaces::default()),
            folders: RefCell::new(Folders::default()),
            folder_rows: RefCell::new(HashMap::new()),
            folder_button,
            save_queued: Cell::new(false),
            peek,
            peek_view: RefCell::new(None),
            paired: RefCell::new(None),
            split_paned,
            archive: RefCell::new(
                std::fs::read_to_string(paths::archive_file())
                    .map(|t| Archive::from_json(&t))
                    .unwrap_or_default(),
            ),
            sync_status: RefCell::new(String::new()),
            sync_busy: Cell::new(false),
            sync_sent: RefCell::new(String::new()),
            sync_others: RefCell::new(Vec::new()),
            last_in_space: RefCell::new(HashMap::new()),
            content: toolbar,
            appearance_css: gtk::CssProvider::new(),
            edge: RefCell::new(EdgeDetector::default()),
            pending_section: RefCell::new(None),
            lazy: RefCell::new(HashMap::new()),
            restoring: Cell::new(false),
            hide_bar: RefCell::new(None),
            hide_side: RefCell::new(None),
            list,
            stack,
            progress,
            chromes: vec![side_chrome, slim_chrome, top_chrome],
            spotlight,
            session,
            ucm,
            filters,
            settings: RefCell::new(settings),
            model: RefCell::new(TabModel::new()),
            views: RefCell::new(HashMap::new()),
            rows: RefCell::new(HashMap::new()),
            syncing: Cell::new(false),
            pending_reload: Cell::new(false),
        });
        *weak.borrow_mut() = Some(Rc::downgrade(&this));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &this.appearance_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        this.apply_appearance();
        this.apply_layout();
        this.wire();
        this
    }

    fn wire(self: &Rc<Self>) {
        let w = self.window.upcast_ref::<gtk::ApplicationWindow>().clone();
        self.wire_peek();

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
                b.open_tab(newtab::NEWTAB_URI, None, true);
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
            "open-spotlight",
            Box::new(|b| {
                let current = b
                    .model
                    .borrow()
                    .selected()
                    .and_then(|id| b.model.borrow().get(id).map(|t| t.url.clone()))
                    .unwrap_or_default();
                let shown = if current == "about:blank" || current.starts_with("brujula://") {
                    String::new()
                } else {
                    current
                };
                b.spotlight.open(&shown);
            }),
        );
        action(
            "change-engine",
            Box::new(|b| {
                b.open_tab(newtab::NEWTAB_URI, None, true);
                if let Some(v) = b.current_view() {
                    b.render_newtab(&v, true);
                }
            }),
        );
        action(
            "reload",
            Box::new(|b| {
                if let Some(v) = b.current_view() {
                    if v.is_loading() {
                        v.stop_loading()
                    } else {
                        b.reload_view(&v);
                    }
                }
            }),
        );
        action("open-settings", Box::new(|b| b.open_settings(None)));
        action(
            "next-space",
            Box::new(|b| {
                let id = b.spaces.borrow().relative(1);
                b.switch_space(id);
            }),
        );
        action(
            "prev-space",
            Box::new(|b| {
                let id = b.spaces.borrow().relative(-1);
                b.switch_space(id);
            }),
        );
        let star = gio::SimpleAction::new_stateful("toggle-bookmark", None, &false.to_variant());
        let this = Rc::downgrade(self);
        star.connect_activate(move |_, _| {
            if let Some(b) = this.upgrade() {
                b.toggle_bookmark();
            }
        });
        w.add_action(&star);
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
            &self.settings.borrow().sidebar_visible.to_variant(),
        );
        let this = Rc::downgrade(self);
        toggle.connect_activate(move |a, _| {
            if let Some(b) = this.upgrade() {
                let pinned = !b.settings.borrow().sidebar_visible;
                b.settings.borrow_mut().sidebar_visible = pinned;
                a.set_state(&pinned.to_variant());
                b.apply_layout();
                b.save_settings();
            }
        });
        w.add_action(&toggle);

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
                b.refresh_site_action();
                if let Some(v) = b.current_view() {
                    b.reload_view(&v);
                }
            }
        });
        w.add_action(&adblock);

        // The shield: blocking on or off for the site in front. Rules are baked per site, so the
        // lists are compiled again and the page reloads once they are in place.
        let site = gio::SimpleAction::new_stateful(
            "toggle-site-adblock",
            None,
            &self.settings.borrow().adblock_enabled.to_variant(),
        );
        let this = Rc::downgrade(self);
        site.connect_activate(move |_, _| {
            let Some(b) = this.upgrade() else { return };
            if !b.settings.borrow().adblock_enabled {
                // Blocking is off everywhere: the shield turns it back on.
                if let Some(a) = b.window.lookup_action("toggle-adblock") {
                    a.activate(None);
                }
                return;
            }
            let Some(host) = b.current_host() else { return };
            let allowed = adblock::host_allowed(&host, &b.settings.borrow().allowlist);
            b.settings.borrow_mut().set_site_allowed(&host, !allowed);
            b.save_settings();
            let allow = b.settings.borrow().allowlist.clone();
            b.filters.set_allow(allow);
            b.pending_reload.set(true);
            b.filters.rebuild();
            b.refresh_site_action();
        });
        w.add_action(&site);

        // Back and forward are enabled by what the page can do; the buttons follow.
        for name in ["back", "forward"] {
            if let Some(a) = w.lookup_action(name).and_downcast::<gio::SimpleAction>() {
                a.set_enabled(false);
            }
        }

        // Closing the Spotlight hands the keyboard back to the page.
        let this = Rc::downgrade(self);
        self.spotlight.set_on_closed(move || {
            if let Some(b) = this.upgrade() {
                if let Some(v) = b.current_view() {
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
            // A folder is not a tab: clicking it folds or unfolds it, and the tab in front stays
            // selected.
            if let Some(folder) = row
                .widget_name()
                .strip_prefix("folder-")
                .and_then(|n| n.parse::<u32>().ok())
            {
                b.toggle_folder(folder);
                return;
            }
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
            b.set_shield_tooltip();
            let views: Vec<WebView> = b.views.borrow().values().cloned().collect();
            for v in &views {
                hide_elements(&b.filters, v);
            }
            // Either a site switch is waiting for the new rules, or nothing was applied before
            // and the page in front loaded unfiltered.
            let site_switch = b.pending_reload.take();
            if site_switch || first {
                if let Some(v) = b.current_view() {
                    b.reload_view(&v);
                }
            }
        });

        self.wire_hover();
        self.wire_folders();

        // The sidebar lists the tabs of the space in front.
        let this = Rc::downgrade(self);
        self.list.set_filter_func(move |row| {
            let Some(b) = this.upgrade() else { return true };
            let Some(id) = row
                .widget_name()
                .strip_prefix("tab-")
                .and_then(|n| n.parse::<u64>().ok())
            else {
                // A folder row shows in the space the folder belongs to.
                let folder = row
                    .widget_name()
                    .strip_prefix("folder-")
                    .and_then(|n| n.parse::<u32>().ok());
                return match (folder, b.model.try_borrow(), b.folders.try_borrow()) {
                    (Some(f), Ok(model), Ok(folders)) => folders
                        .get(f)
                        .is_none_or(|f| f.space == model.current_space()),
                    _ => true,
                };
            };
            // Being changed right now: show it, the filter runs again when that is done.
            let (Ok(model), Ok(folders)) = (b.model.try_borrow(), b.folders.try_borrow()) else {
                return true;
            };
            model.get(TabId(id)).is_none_or(|t| {
                if t.space != model.current_space() {
                    return false;
                }
                // Folded away, unless it is the tab in front.
                match folders.folder_of(t.folder, t.space, &t.url, &t.title) {
                    Some(f) => {
                        model.selected() == Some(t.id) || folders.get(f).is_none_or(|f| f.open)
                    }
                    None => true,
                }
            })
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

    /// Shows the window-controls bar at the top edge and, while the sidebar is not pinned, the
    /// sidebar at the left edge; both hide a moment after the pointer leaves them.
    fn wire_hover(self: &Rc<Self>) {
        // Top bar.
        let enter = gtk::EventControllerMotion::new();
        let this = Rc::downgrade(self);
        enter.connect_enter(move |_, _, _| {
            if let Some(b) = this.upgrade() {
                if let Some(s) = b.hide_bar.take() {
                    s.remove();
                }
                b.bar_revealer.set_reveal_child(true);
            }
        });
        let this = Rc::downgrade(self);
        enter.connect_leave(move |_| {
            if let Some(b) = this.upgrade() {
                b.schedule_hide_bar();
            }
        });
        self.top_hot.add_controller(enter);
        let bar_motion = gtk::EventControllerMotion::new();
        let this = Rc::downgrade(self);
        bar_motion.connect_enter(move |_, _, _| {
            if let Some(b) = this.upgrade() {
                if let Some(s) = b.hide_bar.take() {
                    s.remove();
                }
            }
        });
        let this = Rc::downgrade(self);
        bar_motion.connect_leave(move |_| {
            if let Some(b) = this.upgrade() {
                b.schedule_hide_bar();
            }
        });
        self.bar_revealer.add_controller(bar_motion);

        // Heading for the left edge: a quick move to the side shows the sidebar before the
        // pointer gets there. Seen first (capture), so the page cannot swallow it.
        let motion = gtk::EventControllerMotion::new();
        motion.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = Rc::downgrade(self);
        motion.connect_motion(move |_, x, _| {
            let Some(b) = this.upgrade() else { return };
            let (floating, wakes, width) = {
                let s = b.settings.borrow();
                (
                    !s.sidebar_visible && s.appearance.tabs == TabPosition::Vertical,
                    s.edge_reveal,
                    f64::from(s.appearance.sidebar_width) + 24.0,
                )
            };
            if !floating {
                return;
            }
            if b.split.shows_sidebar() {
                // Over the sidebar it stays; out on the page it goes away after a moment.
                if x <= width {
                    if let Some(s) = b.hide_side.take() {
                        s.remove();
                    }
                } else if b.hide_side.borrow().is_none() {
                    b.schedule_hide_side();
                }
            } else if wakes && b.edge.borrow_mut().feed(x, glib::monotonic_time()) {
                b.split.set_show_sidebar(true);
                // The pointer has not entered the sidebar yet: give it time to get there.
                b.schedule_hide_side_after(1500);
            }
        });
        let this = Rc::downgrade(self);
        motion.connect_leave(move |_| {
            if let Some(b) = this.upgrade() {
                b.edge.borrow_mut().reset();
            }
        });
        if let Some(root) = self.left_hot.parent() {
            root.add_controller(motion);
        }

        // Left edge: the sidebar, when it is not pinned.
        let edge = gtk::EventControllerMotion::new();
        let this = Rc::downgrade(self);
        edge.connect_enter(move |_, _, _| {
            if let Some(b) = this.upgrade() {
                if let Some(s) = b.hide_side.take() {
                    s.remove();
                }
                if !b.settings.borrow().sidebar_visible {
                    b.split.set_show_sidebar(true);
                }
            }
        });
        let this = Rc::downgrade(self);
        edge.connect_leave(move |_| {
            if let Some(b) = this.upgrade() {
                b.schedule_hide_side();
            }
        });
        self.left_hot.add_controller(edge);
        let side_motion = gtk::EventControllerMotion::new();
        let this = Rc::downgrade(self);
        side_motion.connect_enter(move |_, _, _| {
            if let Some(b) = this.upgrade() {
                if let Some(s) = b.hide_side.take() {
                    s.remove();
                }
            }
        });
        let this = Rc::downgrade(self);
        side_motion.connect_leave(move |_| {
            if let Some(b) = this.upgrade() {
                b.schedule_hide_side();
            }
        });
        self.side.add_controller(side_motion);
    }

    fn schedule_hide_bar(self: &Rc<Self>) {
        if let Some(old) = self.hide_bar.take() {
            old.remove();
        }
        let this = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(450), move || {
            if let Some(b) = this.upgrade() {
                // `take` first: the source is finished, so it must not be removed again.
                b.hide_bar.take();
                b.bar_revealer.set_reveal_child(false);
            }
        });
        *self.hide_bar.borrow_mut() = Some(id);
    }

    fn schedule_hide_side(self: &Rc<Self>) {
        self.schedule_hide_side_after(450);
    }

    fn schedule_hide_side_after(self: &Rc<Self>, millis: u64) {
        if let Some(old) = self.hide_side.take() {
            old.remove();
        }
        let this = Rc::downgrade(self);
        let id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(millis), move || {
                if let Some(b) = this.upgrade() {
                    b.hide_side.take();
                    // The Spotlight and the tab menu take the pointer: keep the sidebar open
                    // while one is up, or the page flickers.
                    if !b.settings.borrow().sidebar_visible && !b.spotlight.is_open() {
                        b.split.set_show_sidebar(false);
                    }
                }
            });
        *self.hide_side.borrow_mut() = Some(id);
    }

    fn start(self: &Rc<Self>, urls: &[String]) {
        let this = self.clone();
        let urls = urls.to_vec();
        glib::spawn_future_local(async move {
            // Rules compiled last time go in first, so no page loads unfiltered.
            let allow = this.settings.borrow().allowlist.clone();
            this.filters.set_allow(allow);
            if this.settings.borrow().adblock_enabled && this.filters.load_compiled().await {
                this.set_shield_tooltip();
            }
            this.restore(&urls);
            this.start_sync();
            this.start_archiving();
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

        *self.spaces.borrow_mut() = session.spaces.clone();
        *self.folders.borrow_mut() = session.folders.clone();
        self.restoring.set(true);
        let mut opened = Vec::new();
        for t in &session.tabs {
            self.model.borrow_mut().set_current_space(t.space);
            let (id, ch) = self.model.borrow_mut().open(&t.url, None, false);
            if t.pinned {
                self.model.borrow_mut().set_pinned(id, true);
            }
            self.model.borrow_mut().set_folder(id, t.folder);
            self.model
                .borrow_mut()
                .update(id, |tab| tab.title = t.title.clone());
            if t.url == newtab::NEWTAB_URI || t.url.is_empty() {
                self.create_view(id, &t.url, None);
            } else {
                self.create_view(id, "", None);
                self.lazy.borrow_mut().insert(id, t.url.clone());
            }
            self.apply(ch);
            // Only the time the browser was open counts as time unused.
            let idle = if t.active > 0 {
                session.saved_at.saturating_sub(t.active)
            } else {
                0
            };
            self.model
                .borrow_mut()
                .touch(id, now_ms().saturating_sub(idle));
            opened.push(id);
        }
        let current = self.spaces.borrow().current();
        self.model.borrow_mut().set_current_space(current);
        if let Some(id) = opened.get(session.selected) {
            let c = self.model.borrow_mut().select(*id);
            self.apply(c);
        }
        self.restoring.set(false);
        self.rebuild_folder_rows();
        self.list.invalidate_filter();
        self.rebuild_spaces();
        self.load_if_lazy();
        for u in urls {
            self.open_tab(u, None, true);
        }
        if self.model.borrow().is_empty() {
            self.open_tab(newtab::NEWTAB_URI, None, true);
        }
    }

    /// Loads the page of the tab in front if it was restored but never opened.
    fn load_if_lazy(&self) {
        if self.restoring.get() {
            return;
        }
        let Some(id) = self.model.borrow().selected() else {
            return;
        };
        let url = self.lazy.borrow_mut().remove(&id);
        if let (Some(url), Some(v)) = (url, self.views.borrow().get(&id)) {
            v.load_uri(&url);
        }
    }

    fn save_settings(&self) {
        let _ = paths::write_atomic(&paths::settings_file(), &self.settings.borrow().to_json());
    }

    fn save_session(&self) {
        let s = Session::capture(
            &self.model.borrow(),
            &self.spaces.borrow(),
            &self.folders.borrow(),
        )
        .stamped(now_ms());
        let _ = paths::write_atomic(&paths::session_file(), &s.to_json());
    }

    fn current_view(&self) -> Option<WebView> {
        let id = self.model.borrow().selected()?;
        self.views.borrow().get(&id).cloned()
    }

    pub fn open_tab(self: &Rc<Self>, url: &str, after: Option<TabId>, select: bool) {
        let url = if url.trim().is_empty() {
            "about:blank".to_owned()
        } else if url == newtab::NEWTAB_URI || url == prefs::SETTINGS_URI {
            url.to_owned()
        } else {
            let template = self.settings.borrow().search_template().to_owned();
            omnibox::resolve(url, &template)
                .map_or_else(|| "about:blank".to_owned(), |u| u.to_string())
        };
        let (id, changes) = self.model.borrow_mut().open(&url, after, select);
        self.create_view(id, &url, None);
        self.apply(changes);
    }

    fn close_tab(self: &Rc<Self>, id: TabId) {
        // The last tab of a space gives way to a blank one: the user stays in the space.
        let alone = {
            let m = self.model.borrow();
            m.get(id)
                .is_some_and(|t| m.in_space(t.space).count() == 1 && m.len() > 1)
        };
        if alone {
            if self
                .model
                .borrow()
                .get(id)
                .is_some_and(|t| t.url == newtab::NEWTAB_URI)
            {
                return;
            }
            self.open_tab(newtab::NEWTAB_URI, None, true);
        }
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
        harden(&view);

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
                    // A load that fails (no network) leaves the view without an address; the tab
                    // keeps the one it was opened with, so it is still there on the next start.
                    let Some(uri) = v.uri().map(|u| u.to_string()).filter(|u| !u.is_empty()) else {
                        return;
                    };
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
            if let (Some(tex), Some(uri)) = (v.favicon(), v.uri()) {
                favicons::remember(&uri, &tex);
            }
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
                    b.scroll_settings(v);
                    b.push_sync_status(v);
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
        // brujula:// addresses are the new-tab page talking to the browser. They never load.
        let this = Rc::downgrade(self);
        view.connect_decide_policy(move |v, decision, kind| {
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
                .map(|u| u.to_string())
                .unwrap_or_default();
            // Shift+click on a link: have a look at it in a card over this page.
            if let Some(action) = nav.navigation_action() {
                let shift = action.modifiers() & gdk::ModifierType::SHIFT_MASK.bits() != 0;
                let web = uri.starts_with("https://") || uri.starts_with("http://");
                if shift && web && action.navigation_type() == NavigationType::LinkClicked {
                    decision.ignore();
                    if let Some(b) = this.upgrade() {
                        b.peek(&uri);
                    }
                    return true;
                }
            }
            // A web page never gets to point the browser at a local file.
            let from_web = v
                .uri()
                .is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"));
            if from_web && uri.to_ascii_lowercase().starts_with("file:") {
                decision.ignore();
                return true;
            }
            if !uri.starts_with("brujula://")
                || uri == newtab::NEWTAB_URI
                || uri == prefs::SETTINGS_URI
            {
                return false;
            }
            decision.ignore();
            // Only the browser's own pages may change settings; a web page linking here does
            // nothing.
            let page = v.uri().map(|u| u.to_string()).unwrap_or_default();
            let Some(b) = this.upgrade() else { return true };
            if page == prefs::SETTINGS_URI {
                if let Some((key, value)) = prefs::parse_change(&uri) {
                    b.change_setting(&key, &value);
                }
                return true;
            }
            if page != newtab::NEWTAB_URI {
                return true;
            }
            if let Some(tab) = newtab::parse_tab(&uri) {
                b.switch_from_newtab(tab);
            } else if uri == newtab::CHOOSE_ENGINE_URI {
                b.render_newtab(v, true);
            } else if let Some(engine) = newtab::parse_set_engine(&uri) {
                {
                    let mut s = b.settings.borrow_mut();
                    s.search_engine = engine.id.to_owned();
                    s.search_engine_chosen = true;
                }
                b.save_settings();
                b.render_newtab(v, false);
            }
            true
        });
        // Downloads go to the user's Downloads folder, without a prompt. The page picks the name,
        // so it is cleaned, and files that can run (scripts, launchers, installers) are refused:
        // a page must not be able to drop one where a double click would start it.
        let this = Rc::downgrade(self);
        self.session.connect_download_started(move |_, download| {
            let this = this.clone();
            download.connect_decide_destination(move |d, suggested| {
                let dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                    .unwrap_or_else(glib::home_dir);
                let name = downloads::safe_file_name(suggested);
                if downloads::is_dangerous(&name) {
                    d.cancel();
                    if let Some(b) = this.upgrade() {
                        gtk::AlertDialog::builder()
                            .message("Descarga bloqueada")
                            .detail(format!(
                                "«{name}» es un archivo que se puede ejecutar. Brújula no lo \
                                 guarda para que ningún sitio web pueda dejar un programa o script \
                                 en tu equipo."
                            ))
                            .buttons(["Entendido"])
                            .build()
                            .show(Some(&b.window));
                    }
                    return true;
                }
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
        self.follow_focus(id, &view);
        if url == newtab::NEWTAB_URI {
            self.render_newtab(&view, false);
        } else if url == prefs::SETTINGS_URI {
            self.render_settings(&view);
        } else if !url.is_empty() && url != "about:blank" {
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
                Change::Opened(id) => {
                    self.model.borrow_mut().touch(id, now_ms());
                    self.add_row(id);
                }
                Change::Closed(id) => {
                    if self.in_split(id) {
                        self.end_split();
                    }
                    self.lazy.borrow_mut().remove(&id);
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
                    self.model.borrow_mut().touch(id, now_ms());
                    self.follow_space_of(id);
                    self.show_page(id);
                    self.syncing.set(true);
                    for (rid, r) in self.rows.borrow().iter() {
                        if *rid == id {
                            r.row.add_css_class("brujula-current");
                            self.list.select_row(Some(&r.row));
                        } else {
                            r.row.remove_css_class("brujula-current");
                        }
                    }
                    self.syncing.set(false);
                    self.load_if_lazy();
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
        self.list.invalidate_filter();
        self.schedule_save();
        if self.horizontal() {
            self.rebuild_strip();
        }
    }

    fn add_row(self: &Rc<Self>, id: TabId) {
        let favicon_icon = gtk::Image::from_icon_name("ndl-globe-symbolic");
        favicon_icon.set_pixel_size(16);
        let favicon_pic = gtk::Picture::new();
        favicon_pic.set_content_fit(gtk::ContentFit::Contain);
        favicon_pic.set_can_shrink(true);
        favicon_pic.set_visible(false);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_size_request(16, 16);
        let favicon = gtk::Overlay::new();
        favicon.set_child(Some(&spacer));
        favicon.add_overlay(&favicon_icon);
        favicon.add_overlay(&favicon_pic);
        favicon.set_valign(gtk::Align::Center);
        favicon.set_halign(gtk::Align::Center);
        favicon.set_overflow(gtk::Overflow::Hidden);
        favicon.add_css_class("brujula-tab-favicon");
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["brujula-tab-title"])
            .build();
        let audio = gtk::Image::from_icon_name("ndl-volume-symbolic");
        audio.set_pixel_size(14);
        audio.set_visible(false);
        let pin = gtk::Image::from_icon_name("ndl-pin-symbolic");
        pin.set_pixel_size(14);
        pin.set_visible(false);
        let close = gtk::Button::builder()
            .icon_name("ndl-close-symbolic")
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
        row.add_css_class("brujula-row");
        row.set_widget_name(&format!("tab-{}", id.0));
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
                favicon_pic,
                favicon_icon,
                title,
                close,
                audio,
                pin,
                spinner,
            },
        );
        self.sync_row(id);
    }

    /// Puts the rows in display order: the pinned tabs that are in no folder, then each folder
    /// with its tabs under it, then the rest. Tabs of a smart folder land in it on their own.
    fn reorder_rows(&self) {
        let model = self.model.borrow();
        let folders = self.folders.borrow();
        let rows = self.rows.borrow();
        let folder_rows = self.folder_rows.borrow();
        let folder_of =
            |t: &brujula_core::tabs::Tab| folders.folder_of(t.folder, t.space, &t.url, &t.title);
        let mut order: Vec<gtk::ListBoxRow> = Vec::new();
        let tab_row = |t: &brujula_core::tabs::Tab| rows.get(&t.id).map(|r| r.row.clone());
        order.extend(
            model
                .tabs()
                .iter()
                .filter(|t| t.pinned && folder_of(t).is_none())
                .filter_map(tab_row),
        );
        for folder in folders.all() {
            let members: Vec<_> = model
                .tabs()
                .iter()
                .filter(|t| folder_of(t) == Some(folder.id))
                .collect();
            if let Some(fr) = folder_rows.get(&folder.id) {
                fr.sync(folder, members.len());
                order.push(fr.row.clone());
            }
            order.extend(members.iter().filter_map(|t| tab_row(t)));
        }
        order.extend(
            model
                .tabs()
                .iter()
                .filter(|t| !t.pinned && folder_of(t).is_none())
                .filter_map(tab_row),
        );
        for (i, row) in order.iter().enumerate() {
            if row.index() != i as i32 {
                self.list.remove(row);
                self.list.insert(row, i as i32);
            }
        }
        // Tabs inside a folder are indented.
        for t in model.tabs() {
            if let Some(r) = rows.get(&t.id) {
                if folder_of(t).is_some() {
                    r.row.add_css_class("brujula-in-folder");
                } else {
                    r.row.remove_css_class("brujula-in-folder");
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

    /// Writes the session soon, once, however many changes come before then, so a crash or a
    /// kill costs at most the last second or two instead of the whole session.
    fn schedule_save(self: &Rc<Self>) {
        if self.restoring.get() || self.save_queued.replace(true) {
            return;
        }
        let this = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            if let Some(b) = this.upgrade() {
                b.save_queued.set(false);
                b.save_session();
            }
        });
    }

    fn sync_row(&self, id: TabId) {
        let model = self.model.borrow();
        let (Some(tab), rows) = (model.get(id), self.rows.borrow()) else {
            return;
        };
        let Some(r) = rows.get(&id) else { return };
        let title = if !tab.title.is_empty() {
            tab.title.clone()
        } else if tab.url == prefs::SETTINGS_URI {
            "Preferencias".to_owned()
        } else if tab.url.is_empty() || tab.url == "about:blank" || tab.url == newtab::NEWTAB_URI {
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
                Some(tex) => {
                    r.favicon_pic.set_paintable(Some(&tex));
                    r.favicon_pic.set_visible(true);
                    r.favicon_icon.set_visible(false);
                }
                None => {
                    r.favicon_pic.set_visible(false);
                    r.favicon_icon.set_visible(true);
                }
            }
        }
    }

    /// Address bar, navigation buttons, window title, progress for the selected tab.
    fn sync_chrome(&self) {
        let model = self.model.borrow();
        let Some(tab) = model.selected().and_then(|id| model.get(id)) else {
            return;
        };
        let on_newtab =
            tab.url == newtab::NEWTAB_URI || tab.url.is_empty() || tab.url == "about:blank";
        let (label, icon) = if on_newtab {
            (URL_PLACEHOLDER.to_owned(), "ndl-search-symbolic")
        } else if tab.url == prefs::SETTINGS_URI {
            ("Preferencias".to_owned(), "ndl-lock-symbolic")
        } else if tab.url.starts_with("https://") {
            (host_of(&tab.url), "ndl-lock-symbolic")
        } else {
            (host_of(&tab.url), "ndl-unlock-symbolic")
        };
        let title = if tab.title.is_empty() {
            "Brújula".to_owned()
        } else {
            format!("{} – Brújula", tab.title)
        };
        let reload_icon = if tab.loading {
            "ndl-stop-symbolic"
        } else {
            "ndl-refresh-symbolic"
        };
        drop(model);
        self.window.set_title(Some(&title));
        for c in &self.chromes {
            c.url_label.set_label(&label);
            c.url_label.set_tooltip_text(Some(&label));
            c.url_icon.set_icon_name(Some(icon));
            c.reload.set_icon_name(reload_icon);
        }
        self.update_nav();
        self.sync_progress();
        self.refresh_site_action();
        self.refresh_bookmark_action();
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        let a = self.window.lookup_action(name);
        if let Some(a) = a.and_downcast::<gio::SimpleAction>() {
            a.set_enabled(enabled);
        }
    }

    fn update_nav(&self) {
        if let Some(v) = self.current_view() {
            self.set_action_enabled("back", v.can_go_back());
            self.set_action_enabled("forward", v.can_go_forward());
        }
    }

    /// The host of the page in front, as the allow list spells it.
    fn current_host(&self) -> Option<String> {
        let uri = self.current_view()?.uri()?;
        if !(uri.starts_with("http://") || uri.starts_with("https://")) {
            return None;
        }
        adblock::normalize_host(&uri)
    }

    fn site_blocked(&self) -> bool {
        let s = self.settings.borrow();
        s.adblock_enabled
            && self
                .current_host()
                .is_none_or(|h| !adblock::host_allowed(&h, &s.allowlist))
    }

    /// Keeps the shield (and its tooltip) in step with the site in front.
    fn refresh_site_action(&self) {
        if let Some(a) = self.window.lookup_action("toggle-site-adblock") {
            if let Some(a) = a.downcast_ref::<gio::SimpleAction>() {
                a.set_state(&self.site_blocked().to_variant());
            }
        }
        self.set_shield_tooltip();
    }

    fn set_shield_tooltip(&self) {
        let host = self.current_host();
        let text = if !self.settings.borrow().adblock_enabled {
            "Bloqueador de anuncios desactivado (Ctrl+Mayús+B para activarlo)".to_owned()
        } else if let Some(h) = host.filter(|_| !self.site_blocked()) {
            format!("Bloqueador desactivado en {h}; pulsa para volver a activarlo (Ctrl+Mayús+B)")
        } else {
            format!(
                "Bloqueador de anuncios activo: {} reglas. Pulsa para desactivarlo en este sitio (Ctrl+Mayús+B)",
                self.filters.rule_count()
            )
        };
        for c in &self.chromes {
            c.shield.set_tooltip_text(Some(&text));
        }
    }

    /// Reloads a page; the browser's own pages are drawn again instead.
    fn reload_view(&self, view: &WebView) {
        match view.uri().as_deref() {
            Some(prefs::SETTINGS_URI) => self.render_settings(view),
            Some(newtab::NEWTAB_URI) => self.render_newtab(view, false),
            _ => view.reload(),
        }
    }

    /// The preferences page, in its own tab (one is enough: it is reused).
    fn open_settings(self: &Rc<Self>, section: Option<&str>) {
        let existing = self
            .model
            .borrow()
            .tabs()
            .iter()
            .find(|t| t.url == prefs::SETTINGS_URI)
            .map(|t| t.id);
        *self.pending_section.borrow_mut() = section
            .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(str::to_owned);
        match existing {
            Some(id) => {
                let c = self.model.borrow_mut().select(id);
                self.apply(c);
                if let Some(v) = self.current_view() {
                    self.scroll_settings(&v);
                }
            }
            None => self.open_tab(prefs::SETTINGS_URI, None, true),
        }
    }

    fn render_settings(&self, view: &WebView) {
        let html = prefs::render(
            &self.settings.borrow(),
            &self.bookmarks.borrow(),
            &self.spaces.borrow(),
            &self.extras(),
        );
        view.load_html(&html, Some(prefs::SETTINGS_URI));
    }

    /// Scrolls the preferences page to the section the Spotlight pointed at.
    fn scroll_settings(&self, view: &WebView) {
        if view.uri().as_deref() != Some(prefs::SETTINGS_URI) || view.is_loading() {
            return;
        }
        let Some(section) = self.pending_section.borrow_mut().take() else {
            return;
        };
        // The id is alphanumeric (checked when it was stored), so it cannot break the script.
        let script = format!("document.getElementById('{section}')?.scrollIntoView()");
        view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }

    /// One change made on the preferences page: checked, stored and put into effect.
    fn change_setting(self: &Rc<Self>, key: &str, value: &str) {
        // Bookmarks and spaces are not settings: they have their own stores.
        if let Some(rest) = key.strip_prefix("space.") {
            self.change_space(rest, value);
            return;
        }
        if let Some(what) = key.strip_prefix("archive.") {
            self.change_archive(what, value);
            return;
        }
        if key == "bookmark.remove" {
            self.bookmarks.borrow_mut().remove(value);
            self.save_bookmarks();
            self.refresh_bookmark_action();
            return;
        }
        let result = self.settings.borrow_mut().apply(key, value);
        let effect = match result {
            Ok(effect) => effect,
            Err(why) => {
                eprintln!("brujula: setting refused: {why}");
                return;
            }
        };
        self.save_settings();
        if effect.appearance {
            self.apply_appearance();
        }
        if effect.layout {
            self.apply_layout();
        }
        if effect.blocker {
            self.apply_blocker();
        }
        if effect.sync || effect.sync_now {
            self.sync_round();
        }
        if key == "engine" || effect.appearance {
            // The new-tab pages still open show the old engine, theme or accent.
            self.refresh_newtabs();
        }
    }

    /// Theme, accent and the rest of the look the user chose.
    fn apply_appearance(&self) {
        let a = self.settings.borrow().appearance.clone();
        self.appearance_css.load_from_string(&appearance::css(&a));
        let style = ndl::StyleManager::default();
        style.set_color_scheme(match a.theme {
            Theme::System => ndl::ColorScheme::Default,
            Theme::Light => ndl::ColorScheme::ForceLight,
            Theme::Dark => ndl::ColorScheme::ForceDark,
        });
        if a.accent == ACCENT_DEFAULT {
            style.set_accent(None);
        } else {
            style.set_accent(Some((&a.accent, text_on(&a.accent))));
        }
        self.window.add_css_class("brujula-window");
        if a.layout == Layout::Frame {
            self.window.add_css_class("brujula-frame");
        } else {
            self.window.remove_css_class("brujula-frame");
        }
    }

    /// Sidebar width, pinned or floating, and whether the edge wakes it.
    fn apply_layout(&self) {
        let s = self.settings.borrow();
        let width = i32::try_from(s.appearance.sidebar_width).unwrap_or(232);
        self.side.set_size_request(width, -1);
        self.split.set_min_sidebar_width(f64::from(width));
        self.split.set_max_sidebar_width(f64::from(width));
        let horizontal = s.appearance.tabs == TabPosition::Horizontal;
        self.content.set_size_request(width + 80, -1);
        // With the tabs on top there is no sidebar at all: the page fills the rest.
        let pinned = s.sidebar_visible && !horizontal;
        self.split.set_collapsed(!pinned);
        self.split.set_show_sidebar(pinned);
        self.left_hot
            .set_visible(!pinned && !horizontal && s.edge_reveal);
        self.top_hot.set_visible(!horizontal);
        self.topbar.set_visible(horizontal);
        if horizontal {
            self.bar_revealer.set_reveal_child(false);
            self.split.add_css_class("brujula-horizontal");
            self.window.add_css_class("brujula-horizontal");
        } else {
            self.split.remove_css_class("brujula-horizontal");
            self.window.remove_css_class("brujula-horizontal");
        }
        drop(s);
        self.set_action_enabled("toggle-sidebar", !horizontal);
    }

    /// The blocker was switched, or its lists or sites changed: rules are compiled again. Open
    /// pages keep what they loaded with until they are reloaded.
    fn apply_blocker(&self) {
        let s = self.settings.borrow().clone();
        if let Some(a) = self.window.lookup_action("toggle-adblock") {
            if let Some(a) = a.downcast_ref::<gio::SimpleAction>() {
                a.set_state(&s.adblock_enabled.to_variant());
            }
        }
        self.filters.set_allow(s.allowlist.clone());
        if s.adblock_enabled {
            self.filters.start(&s);
        } else {
            self.filters.disable();
        }
        self.refresh_site_action();
    }

    /// Fills the new-tab page. `choosing` shows the search-engine picker instead of the search
    /// box (first run, or when the user asked to change it).
    fn render_newtab(&self, view: &WebView, choosing: bool) {
        let html = self.newtab_html(choosing);
        // Loading runs handlers that borrow the model and the settings: nothing may be held.
        view.load_html(&html, Some(newtab::NEWTAB_URI));
    }

    fn newtab_html(&self, choosing: bool) -> String {
        let settings = self.settings.borrow();
        let engine = search::engine_or_default(&settings.search_engine);
        let choosing = choosing || !settings.search_engine_chosen;
        let lang = glib::language_names()
            .first()
            .map(|l| l.to_string())
            .unwrap_or_default();
        let hour = glib::DateTime::now_local()
            .ok()
            .and_then(|d| u32::try_from(d.hour()).ok())
            .unwrap_or(12);
        let link = |title: &str, href: String, site: &str, tag: &'static str| newtab::Link {
            title: title.to_owned(),
            href,
            site: site.to_owned(),
            icon: favicons::data_uri(site),
            tag,
        };
        let bookmarks: Vec<newtab::Link> = self
            .bookmarks
            .borrow()
            .items()
            .iter()
            .map(|b| link(&b.title, b.url.clone(), &b.url, ""))
            .collect();
        let model = self.model.borrow();
        let blank = |url: &str| {
            url.is_empty()
                || url == "about:blank"
                || url == newtab::NEWTAB_URI
                || url == prefs::SETTINGS_URI
        };
        // Tabs open in this space (newest first), then the ones closed lately.
        let mut recent: Vec<newtab::Link> = model
            .in_space(model.current_space())
            .filter(|t| !blank(&t.url))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(3)
            .map(|t| {
                link(
                    &t.title,
                    format!("{}{}", newtab::TAB_PREFIX, t.id.0),
                    &t.url,
                    "Abierta",
                )
            })
            .collect();
        recent.extend(
            model
                .recently_closed(8)
                .into_iter()
                .filter(|(_, url)| !blank(url))
                .take(3)
                .map(|(title, url)| link(title, url.to_owned(), url, "Cerrada")),
        );
        let devices = self.devices_for_newtab(&link);
        // Pages archived for lack of use come after the closed ones, which they also are.
        let archive = self.archive.borrow();
        recent.retain(|r| archive.items().iter().all(|e| e.url != r.href));
        recent.extend(
            archive
                .items()
                .iter()
                .take(3)
                .map(|e| link(&e.title, e.url.clone(), &e.url, "Archivada")),
        );
        let theme = match settings.appearance.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        newtab::render(&newtab::Page {
            engine,
            choosing,
            lang: &lang,
            theme,
            accent: &settings.appearance.accent,
            hour,
            bookmarks: &bookmarks,
            recent: &recent,
            devices: &devices,
        })
    }

    /// Shows the open tab a link on the new-tab page points at, and drops the blank tab that was
    /// showing the page.
    fn switch_from_newtab(self: &Rc<Self>, target: u64) {
        let target = TabId(target);
        if self.model.borrow().get(target).is_none() {
            return;
        }
        let blank = self.model.borrow().selected();
        let changes = self.model.borrow_mut().select(target);
        self.apply(changes);
        if let Some(blank) = blank.filter(|b| *b != target) {
            let is_newtab = self
                .model
                .borrow()
                .get(blank)
                .is_some_and(|t| t.url == newtab::NEWTAB_URI);
            if is_newtab {
                self.close_tab(blank);
            }
        }
    }

    /// What the Spotlight lists for `text`.
    fn spotlight_items(&self, text: &str) -> Vec<Item> {
        let settings = self.settings.borrow();
        let engine = search::engine_or_default(&settings.search_engine);
        let model = self.model.borrow();
        let tabs: Vec<_> = model
            .tabs()
            .iter()
            .filter(|t| t.url != newtab::NEWTAB_URI && t.url != prefs::SETTINGS_URI)
            .cloned()
            .collect();
        let marks = self.bookmarks.borrow();
        let views = self.views.borrow();
        suggest::suggestions(text, &tabs, marks.items(), settings.search_template())
            .into_iter()
            .map(|s| {
                let texture = match &s.action {
                    Action::Go(url) => favicons::lookup(url),
                    Action::SwitchTab(id) => views
                        .get(id)
                        .and_then(|v| v.favicon())
                        .or_else(|| model.get(*id).and_then(|t| favicons::lookup(&t.url))),
                    _ => None,
                };
                let icon = match (&s.kind, &s.action) {
                    (Kind::Typed, Action::Search(_)) => "ndl-search-symbolic",
                    (Kind::Bookmark, _) => "ndl-bookmark-symbolic",
                    (Kind::Setting, _) => "ndl-settings-symbolic",
                    _ => "ndl-globe-symbolic",
                };
                let title = match (&s.kind, &s.action) {
                    (Kind::Typed, Action::Search(q)) => {
                        format!("Buscar con {}: {q}", engine.name)
                    }
                    (Kind::Typed, _) => format!("Ir a {}", s.title),
                    _ => s.title.clone(),
                };
                let detail = if s.kind == Kind::Typed {
                    match &s.action {
                        Action::Go(url) => url.clone(),
                        _ => String::new(),
                    }
                } else {
                    s.detail.clone()
                };
                Item {
                    title,
                    detail,
                    icon,
                    texture,
                    action: s.action,
                }
            })
            .collect()
    }

    /// Carries out what the user chose in the Spotlight, in the tab in front.
    fn run_action(self: &Rc<Self>, action: Action) {
        match action {
            Action::SwitchTab(id) => {
                let c = self.model.borrow_mut().select(id);
                self.apply(c);
            }
            Action::Go(url) => self.navigate(&url),
            Action::Settings(section) => self.open_settings(Some(&section)),
            Action::Search(q) => {
                let template = self.settings.borrow().search_template().to_owned();
                let url = omnibox::search_url(&q, &template);
                self.navigate(url.as_str());
            }
        }
    }

    fn navigate(self: &Rc<Self>, url: &str) {
        match self.current_view() {
            Some(v) => v.load_uri(url),
            None => self.open_tab(url, None, true),
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

    fn show_tab_menu(self: &Rc<Self>, id: TabId, row: &impl IsA<gtk::Widget>, x: f64, y: f64) {
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
        // Folders: move the tab into one, out of it, or into a new one.
        // Split view: this tab next to the one in front, or back to one page.
        let in_front = self.model.borrow().selected();
        if self.in_split(id) {
            add("Salir de la vista dividida", Box::new(|b| b.end_split()));
        } else if let Some(front) = in_front.filter(|f| *f != id) {
            add(
                "Ver junto a la pestaña actual",
                Box::new(move |b| b.enter_split(front, id)),
            );
        }
        let (space, current_folder) = {
            let m = self.model.borrow();
            m.get(id).map_or((m.current_space(), None), |t| {
                (
                    t.space,
                    self.folders
                        .borrow()
                        .folder_of(t.folder, t.space, &t.url, &t.title),
                )
            })
        };
        let targets: Vec<(u32, String)> = self
            .folders
            .borrow()
            .in_space(space)
            .filter(|f| Some(f.id) != current_folder)
            .map(|f| (f.id, f.name.clone()))
            .collect();
        for (folder, name) in targets {
            add(
                &format!("Mover a la carpeta {name}"),
                Box::new(move |b| b.put_tab_in_folder(id, Some(folder))),
            );
        }
        if current_folder.is_some() {
            add(
                "Sacar de la carpeta",
                Box::new(move |b| b.put_tab_in_folder(id, Some(NO_FOLDER))),
            );
        }
        add(
            "Nueva carpeta con esta pestaña",
            Box::new(move |b| b.new_folder_with(id)),
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
                let ids = {
                    let m = b.model.borrow();
                    let space = m.get(id).map_or(m.current_space(), |t| t.space);
                    m.in_space(space)
                        .filter(|t| t.id != id && !t.pinned)
                        .map(|t| t.id)
                        .collect::<Vec<_>>()
                };
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
                    let space = m.get(id).map_or(m.current_space(), |t| t.space);
                    m.tabs()[pos..]
                        .iter()
                        .filter(|t| !t.pinned && t.space == space)
                        .map(|t| t.id)
                        .collect::<Vec<_>>()
                };
                for i in ids {
                    b.close_tab(i);
                }
            }),
        );
        let here = self.model.borrow().get(id).map_or(0, |t| t.space);
        let others: Vec<(u32, String)> = self
            .spaces
            .borrow()
            .all()
            .iter()
            .filter(|s| s.id != here)
            .map(|s| (s.id, s.name.clone()))
            .collect();
        for (space, label) in others {
            add(
                &format!("Mover a {label}"),
                Box::new(move |b| b.move_tab_to_space(id, space)),
            );
        }
        pop.set_child(Some(&col));
        let pop2 = pop.clone();
        pop.connect_closed(move |_| pop2.unparent());
        pop.popup();
    }
}

/// The settings every page of the browser gets: no windows of its own, no local files, no
/// permissions. Camera, microphone, location, notifications and the like are refused until there
/// is a prompt to ask the user.
fn harden(view: &WebView) {
    if let Some(s) = WebViewExt::settings(view) {
        s.set_enable_developer_extras(true);
        s.set_enable_back_forward_navigation_gestures(true);
        s.set_javascript_can_open_windows_automatically(false);
        s.set_allow_file_access_from_file_urls(false);
        s.set_allow_universal_access_from_file_urls(false);
        s.set_allow_modal_dialogs(false);
        s.set_enable_write_console_messages_to_stdout(false);
    }
    view.connect_permission_request(|_, request| {
        request.deny();
        true
    });
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
    opts.append(Some("Barra lateral"), Some("win.toggle-sidebar"));
    opts.append(Some("Bloqueador de anuncios"), Some("win.toggle-adblock"));
    opts.append(Some("Cambiar buscador"), Some("win.change-engine"));
    opts.append(Some("Preferencias"), Some("win.open-settings"));
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
