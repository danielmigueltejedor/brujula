//! The browser window: vertical tab sidebar, toolbar, web views.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use brujula_core::adblock;
use brujula_core::appearance;
use brujula_core::archive::Archive;
use brujula_core::bookmarks::now_ms;
use brujula_core::bookmarks::Bookmarks;
use brujula_core::edge::EdgeDetector;
use brujula_core::folders::{Folders, NO_FOLDER};
use brujula_core::history::History;
use brujula_core::https;
use brujula_core::news::NEWS_URI;
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
use brujula_core::welcome::WELCOME_URI;
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

use crate::dialogs;
use crate::favicons;
use crate::filters::Filters;
use crate::paths;
use crate::spotlight::{Item, Spotlight};

mod ai;
mod ai_panel;
mod archive;
mod boosts;
mod cloudsync;
mod downloads_panel;
mod essentials;
mod ext;
mod ext_api;
mod ext_install;
mod extras;
mod float;
mod folders;
mod history;
mod history_panel;
mod import;
mod notes;
mod page_action;
mod passkeys;
mod passwords;
mod peek;
mod profiles_ui;
mod sleep;
mod split;
mod system_theme;
mod tab_drag;
mod updates;
mod webapps;
mod welcome;

type Handler = Box<dyn Fn(&Rc<Browser>)>;

fn append_popover_entry(
    browser: &Rc<Browser>,
    popover: &gtk::Popover,
    column: &gtk::Box,
    label: &str,
    handler: Handler,
) {
    let button = gtk::Button::builder()
        .label(label)
        .css_classes(["flat"])
        .build();
    if let Some(label) = button.child().and_downcast::<gtk::Label>() {
        label.set_xalign(0.0);
    }
    let browser = Rc::downgrade(browser);
    let popover = popover.clone();
    button.connect_clicked(move |_| {
        popover.popdown();
        if let Some(browser) = browser.upgrade() {
            handler(&browser);
        }
    });
    column.append(&button);
}

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
    app.set_accels_for_action("win.toggle-history", &["<Ctrl>h"]);
    app.set_accels_for_action("win.toggle-assistant", &["<Ctrl><Shift>a"]);
    app.set_accels_for_action("win.open-settings", &["<Ctrl>comma"]);
    app.set_accels_for_action("win.toggle-bookmark", &["<Ctrl>d"]);
    app.set_accels_for_action("win.next-space", &["<Alt>Down"]);
    app.set_accels_for_action("win.prev-space", &["<Alt>Up"]);
    app.set_accels_for_action("win.toggle-site-adblock", &["<Ctrl><Shift>b"]);
    // A window that keeps nothing: no history, no cookies, no saved tabs.
    let private = gio::SimpleAction::new("new-private-window", None);
    let weak = app.downgrade();
    private.connect_activate(move |_, _| {
        if let Some(app) = weak.upgrade() {
            present_private(&app, &[]);
        }
    });
    app.add_action(&private);
    app.set_accels_for_action("app.new-private-window", &["<Ctrl><Shift>n"]);
}

/// Opens a private window: its own session in memory, nothing of it written to disk.
pub fn present_private(app: &gtk::Application, urls: &[String]) {
    let browser = Browser::new(app, true);
    PRIVATE.with(|p| p.borrow_mut().push(browser.clone()));
    browser.start(urls);
    browser.window.present();
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
    let browser = Browser::new(app, false);
    BROWSER.with(|b| *b.borrow_mut() = Some(browser.clone()));
    browser.start(urls);
    browser.window.present();
}

thread_local! {
    /// The private windows that are open; each is forgotten when it closes.
    static PRIVATE: RefCell<Vec<Rc<Browser>>> = const { RefCell::new(Vec::new()) };
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
    /// Opens the list of extensions; shown once there is one.
    ext: gtk::MenuButton,
    star: gtk::ToggleButton,
    toggle: gtk::ToggleButton,
    menu: gtk::MenuButton,
    url_button: gtk::Button,
    url_icon: gtk::Image,
    url_label: gtk::Label,
    /// What the page in front allows: add an extension, install a web app. Hidden otherwise.
    page_action: gtk::Button,
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
    let ext = gtk::MenuButton::builder()
        .icon_name("ndl-blocks-symbolic")
        .tooltip_text("Extensiones")
        .css_classes(["flat"])
        .visible(false)
        .build();
    ext.set_popover(Some(&gtk::Popover::new()));
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
        ext,
        star,
        toggle,
        menu,
        url_button,
        url_icon,
        url_label,
        page_action: page_action::build_button(),
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
    /// The sites kept as icons at the top of the sidebar.
    essentials: RefCell<brujula_core::essentials::Essentials>,
    ess: essentials::EssentialsUi,
    /// The downloads button and its panel.
    dl: downloads_panel::DownloadsUi,
    /// The optional AI assistant (see `ai.rs`).
    ai: ai::AiState,
    hist: history_panel::HistoryUi,
    /// Opens the notepad of the space in front.
    notes_button: gtk::Button,
    notes: RefCell<brujula_core::notes::Notes>,
    notes_save_queued: Cell<bool>,
    /// Set while a save is queued, so a burst of changes writes the session once.
    save_queued: Cell<bool>,
    /// Tabs shown in a window of their own.
    floats: RefCell<HashMap<TabId, float::Floating>>,
    /// The sites the user restyled, and the style sheets that carry it to the pages.
    boosts: RefCell<brujula_core::boosts::Boosts>,
    boost_sheets: RefCell<Vec<webkit6::UserStyleSheet>>,
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
    /// Where the preferences page was scrolled to before it was drawn again.
    pending_scroll: Cell<Option<i32>>,
    /// Restored tabs that have not been opened yet: their page loads when they are selected, so
    /// starting with many tabs does not start many page loads.
    lazy: RefCell<HashMap<TabId, String>>,
    /// Addresses that were upgraded to HTTPS and have not loaded yet, with the one asked for.
    https_tried: RefCell<HashMap<String, String>>,
    /// Whether the desktop prefers a dark look (see `system_theme`).
    system_dark: Cell<bool>,
    /// What the tab chips of the horizontal layout last showed.
    strip_key: RefCell<String>,
    /// Sites that did not answer on HTTPS; they open as asked until the browser is closed.
    http_only: RefCell<std::collections::HashSet<String>>,
    /// True while the saved tabs are being put back; the tab that ends up in front loads then.
    restoring: Cell<bool>,
    hide_bar: RefCell<Option<glib::SourceId>>,
    hide_side: RefCell<Option<glib::SourceId>>,
    list: gtk::ListBox,
    stack: gtk::Stack,
    progress: gtk::ProgressBar,
    chromes: Vec<Chrome>,
    spotlight: Rc<Spotlight>,
    /// The blank tab a new-tab Spotlight opened, until the user has chosen something in it.
    fresh_tab: Cell<Option<TabId>>,
    session: NetworkSession,
    ucm: UserContentManager,
    /// A private window: nothing it does is written to disk, and it has its own session.
    private: bool,
    /// The pages visited.
    history: RefCell<History>,
    history_save_queued: Cell<bool>,
    /// The saved passwords, while the vault is open.
    pw: passwords::PwState,
    pk: passkeys::PkState,
    /// The Chromium extensions installed and running.
    ext: RefCell<ext::ExtState>,
    filters: Rc<Filters>,

    settings: RefCell<Settings>,
    model: RefCell<TabModel>,
    views: RefCell<HashMap<TabId, WebView>>,
    rows: RefCell<HashMap<TabId, Row>>,
    /// True while we change the list selection ourselves.
    syncing: Cell<bool>,
    /// Set when the rules are being rebuilt for a site switch; the page reloads once they land.
    pending_reload: Cell<bool>,
    /// The address of the last page asked about its web manifest, and whether it has one.
    page_manifest: RefCell<(String, bool)>,
    /// What the last look for a newer version found.
    update: RefCell<updates::UpdateState>,
}

impl Browser {
    fn new(app: &gtk::Application, private: bool) -> Rc<Self> {
        let first_run = !paths::settings_file().exists();
        let mut settings = std::fs::read_to_string(paths::settings_file())
            .map(|t| Settings::from_json(&t))
            .unwrap_or_default();
        // Settings saved before the welcome page existed count as having seen it; a profile
        // that has never been opened has not.
        if first_run && !private {
            settings.welcome_done = false;
            // A new profile has nothing to be told that is new.
            settings.seen_version = env!("CARGO_PKG_VERSION").to_owned();
        }

        let session = if private {
            // Cookies, cache and storage live in memory and go with the window.
            NetworkSession::new_ephemeral()
        } else {
            NetworkSession::new(
                Some(&paths::data_dir().to_string_lossy()),
                Some(&paths::cache_dir().to_string_lossy()),
            )
        };
        if !private {
            if let Some(dm) = session.website_data_manager() {
                dm.set_favicons_enabled(true);
            }
            keep_cookies(&session, &paths::cookies_file());
        }
        // A bad certificate is a failure, never a click-through; trackers get no free ride.
        session.set_tls_errors_policy(webkit6::TLSErrorsPolicy::Fail);
        session.set_itp_enabled(true);
        let ucm = UserContentManager::new();
        let filters = Filters::new(ucm.clone());

        let window = ndl::ApplicationWindow::new(app);
        let title = if private {
            "Brújula · Privado".to_owned()
        } else if paths::profile() != brujula_core::profiles::DEFAULT_ID {
            // Windows of different people must be told apart.
            let profiles = crate::profiles::load();
            match profiles.get(paths::profile()) {
                Some(p) => format!("Brújula · {}", p.name),
                None => "Brújula".to_owned(),
            }
        } else {
            "Brújula".to_owned()
        };
        window.set_title(Some(&title));
        if private {
            window.add_css_class("brujula-private");
        }
        window.set_default_size(1280, 800);
        window.set_icon_name(Some(paths::APP_ID));

        let ai = ai::AiState::new(private);
        let side_chrome = build_chrome();
        let slim_chrome = build_chrome();
        let top_chrome = build_chrome();

        // Sidebar, top to bottom: window controls, navigation, address, essentials, new tab, tabs, downloads.
        let side_header = ndl::HeaderBar::new();
        side_header.set_show_end_title_buttons(false);
        side_header.set_show_start_title_buttons(false);
        side_header.pack_end(&side_chrome.toggle);
        side_header.pack_end(&side_chrome.page_action);
        let notes_button = gtk::Button::builder()
            .icon_name("ndl-file-text-symbolic")
            .css_classes(["flat", "brujula-newfolder"])
            .tooltip_text("Notas de este espacio")
            .build();
        // The menu is the first button of the top row and the notes sit next to it, which leaves
        // the row under it for the buttons of the page.
        side_header.pack_start(&side_chrome.menu);
        side_header.pack_start(&notes_button);
        if private {
            let badge = gtk::Label::new(Some("Privado"));
            badge.add_css_class("brujula-privatebadge");
            badge.set_tooltip_text(Some(
                "Ventana privada: no guarda historial, cookies ni pestañas",
            ));
            side_header.pack_start(&badge);
        }
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
        nav.append(&side_chrome.ext);
        nav.append(&side_chrome.shield);

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
        // The label gives way when the sidebar is narrow, so the buttons beside it never keep it
        // from shrinking.
        let new_tab_label = gtk::Label::new(Some("Nueva pestaña"));
        new_tab_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        new_tab_content.append(&new_tab_label);
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
        let ess = essentials::EssentialsUi::new();
        side.append(&ess.grid);
        let new_tab_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        side.append(&new_tab_row);
        let hist = history_panel::HistoryUi::new(&scroller);
        side.append(&hist.stack);
        // Next to the notes, in the top row.
        side_header.pack_start(&hist.side_button);
        side_header.pack_start(&ai.ui.side_button);
        let folder_button = gtk::Button::builder()
            .icon_name("ndl-folder-plus-symbolic")
            .css_classes(["flat", "brujula-newfolder"])
            .tooltip_text("Nueva carpeta")
            .build();
        new_tab.set_hexpand(true);
        let dl = downloads_panel::DownloadsUi::new();
        // The button of a new tab sits right above the tabs, with the one of a new folder.
        new_tab_row.set_margin_start(8);
        new_tab_row.set_margin_end(8);
        new_tab_row.append(&new_tab);
        new_tab_row.append(&folder_button);
        // The downloads button stays at the bottom, on its own.
        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bottom.set_margin_start(8);
        bottom.append(&dl.slot_side);
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
        // The bar covers the top of the sidebar while it is shown, and with it the page button
        // that sits there: it has to be in the bar too.
        bar.pack_start(&slim_chrome.page_action);
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
        page.add_css_class("brujula-page");
        // Rounded corners (from the CSS radius) clip the web view too.
        page.set_overflow(gtk::Overflow::Hidden);
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
        // The address row is on top, with the window controls; the tabs are under it.
        let top_nav = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        top_nav.set_hexpand(true);
        top_nav.append(&top_chrome.back);
        top_nav.append(&top_chrome.forward);
        top_nav.append(&top_chrome.reload);
        top_chrome.url_button.set_margin_start(8);
        top_chrome.url_button.set_margin_end(8);
        top_nav.append(&top_chrome.url_button);
        top_nav.append(&top_chrome.page_action);
        top_nav.append(&top_chrome.star);
        top_nav.append(&top_chrome.ext);
        top_nav.append(&top_chrome.shield);
        top_nav.append(&dl.slot_top);
        top_nav.append(&ai.ui.top_button);
        top_nav.append(&hist.top_button);
        top_nav.append(&top_chrome.menu);
        let top_header = ndl::HeaderBar::new();
        top_header.set_show_start_title_buttons(false);
        top_header.set_show_end_title_buttons(true);
        top_header.set_title_widget(Some(&top_nav));
        let tabs_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tabs_row.set_margin_start(8);
        tabs_row.set_margin_end(8);
        tabs_row.set_margin_bottom(6);
        tabs_row.append(&strip_scroll);
        tabs_row.append(&icon_button(
            "ndl-plus-symbolic",
            "Nueva pestaña (Ctrl+T)",
            "win.new-tab",
        ));
        let topbar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        topbar.add_css_class("brujula-topbar");
        topbar.append(&top_header);
        topbar.append(&tabs_row);
        topbar.set_visible(false);
        // With the tabs on top the essentials sit in a slim column left of the page.
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&ess.rail);
        body.append(&page);
        body.append(&ai.ui.panel);
        page.set_hexpand(true);
        page.set_vexpand(true);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&topbar);
        column.append(&body);
        body.set_vexpand(true);

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
        let w3 = weak.clone();
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
            move || {
                w3.borrow()
                    .as_ref()
                    .and_then(std::rc::Weak::upgrade)
                    .map(|b| b.bookmark_icons())
                    .unwrap_or_default()
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
            essentials: RefCell::new(essentials::load()),
            ess,
            dl,
            ai,
            hist,
            notes_button,
            notes: RefCell::new(notes::load()),
            notes_save_queued: Cell::new(false),
            save_queued: Cell::new(false),
            floats: RefCell::new(HashMap::new()),
            boosts: RefCell::new(boosts::load()),
            boost_sheets: RefCell::new(Vec::new()),
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
            pending_scroll: Cell::new(None),
            lazy: RefCell::new(HashMap::new()),
            https_tried: RefCell::new(HashMap::new()),
            strip_key: RefCell::new(String::new()),
            system_dark: Cell::new(gtk::Settings::default().is_some_and(|s| {
                s.gtk_interface_color_scheme() == gtk::InterfaceColorScheme::Dark
            })),
            http_only: RefCell::new(std::collections::HashSet::new()),
            restoring: Cell::new(false),
            hide_bar: RefCell::new(None),
            hide_side: RefCell::new(None),
            list,
            stack,
            progress,
            chromes: vec![side_chrome, slim_chrome, top_chrome],
            spotlight,
            fresh_tab: Cell::new(None),
            session,
            ucm,
            private,
            history: RefCell::new(if private {
                History::default()
            } else {
                load_history()
            }),
            history_save_queued: Cell::new(false),
            pw: passwords::PwState::default(),
            pk: passkeys::PkState::default(),
            ext: RefCell::new(ext::ExtState::default()),
            filters,
            settings: RefCell::new(settings),
            model: RefCell::new(TabModel::new()),
            views: RefCell::new(HashMap::new()),
            rows: RefCell::new(HashMap::new()),
            syncing: Cell::new(false),
            pending_reload: Cell::new(false),
            page_manifest: RefCell::new((String::new(), false)),
            update: RefCell::new(updates::UpdateState::default()),
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
        this.watch_system_theme();
        this.apply_layout();
        this.wire();
        this.wire_downloads();
        this.wire_history_panel();
        this.wire_ai();
        this.refresh_essentials();
        this
    }

    fn wire(self: &Rc<Self>) {
        let w = self.window.upcast_ref::<gtk::ApplicationWindow>().clone();
        for c in &self.chromes {
            let this = Rc::downgrade(self);
            c.ext.set_create_popup_func(move |mb| {
                if let (Some(b), Some(p)) = (this.upgrade(), mb.popover()) {
                    b.fill_ext_popover(&p);
                }
            });
            let this = Rc::downgrade(self);
            c.page_action.connect_clicked(move |_| {
                if let Some(b) = this.upgrade() {
                    b.run_page_action();
                }
            });
        }
        self.wire_peek();
        self.apply_boosts();
        self.wire_notes();

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
        action("new-tab", Box::new(|b| b.new_tab(false)));
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
        action("about", Box::new(|b| b.show_about()));
        action("install-webapp", Box::new(|b| b.install_webapp_dialog()));
        action("toggle-history", Box::new(|b| b.toggle_history()));
        action("add-essential", Box::new(|b| b.add_current_to_essentials()));
        action(
            "open-extensions",
            Box::new(|b| b.open_settings(Some("extensions"))),
        );
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
                if let Some(v) = b.current_view().filter(|v| !shows_own_page(v)) {
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
        self.spotlight.set_on_closed(move |chosen| {
            if let Some(b) = this.upgrade() {
                // Left without choosing: the blank tab the new-tab Spotlight opened goes again.
                if !chosen {
                    b.cancel_new_tab();
                }
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
            // and the page in front loaded unfiltered. The browser's own pages have no ads, and
            // reloading one starts it over: on the first run that was the welcome, typing its
            // greeting again a few seconds in, once the lists had been downloaded.
            let site_switch = b.pending_reload.take();
            if site_switch || first {
                if let Some(v) = b.current_view().filter(|v| !shows_own_page(v)) {
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
                    (Some(f), Ok(model), Ok(folders)) => {
                        folders
                            .get(f)
                            .is_none_or(|f| f.space == model.current_space())
                            && folders.shown(f)
                    }
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
                        model.selected() == Some(t.id)
                            || (folders.get(f).is_none_or(|f| f.open) && folders.shown(f))
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
                b.save_notes();
                b.flush_history();
                b.pw_lock();
                b.unfloat_all();
                if b.private {
                    // The window goes, and with it the session and everything it held.
                    PRIVATE.with(|p| p.borrow_mut().retain(|x| !Rc::ptr_eq(x, &b)));
                }
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
            if b.window.has_css_class("brujula-fullscreen") {
                return;
            }
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
                    if b.settings.borrow().sidebar_visible {
                        return;
                    }
                    if b.spotlight.is_open() || has_open_popover(b.side.upcast_ref()) {
                        // A popup of the sidebar (a space's name and icon, a folder, the notes)
                        // is up: its surface takes the pointer from the sidebar, so leaving
                        // does not mean the user is done. Look again once it is closed.
                        b.schedule_hide_side_after(450);
                    } else {
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
            if !this.private {
                this.start_extensions();
                this.start_passwords();
                this.start_passkeys();
            }
            this.restore(&urls);
            if !this.private {
                this.start_sync();
                this.start_archiving();
            }
            this.start_sleeping();
            // A private window reads the rules compiled before; the downloads and the cache are
            // the normal window's.
            if !this.private && this.settings.borrow().adblock_enabled {
                let s = this.settings.borrow().clone();
                this.filters.start(&s);
            }
        });
    }

    fn restore(self: &Rc<Self>, urls: &[String]) {
        let session = if self.private {
            Session::default()
        } else {
            std::fs::read_to_string(paths::session_file())
                .map(|t| Session::from_json(&t))
                .unwrap_or_default()
        };

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
        if self.needs_welcome() {
            // The first thing a person sees; the new-tab page waits behind it.
            self.open_tab(WELCOME_URI, None, true);
        } else if self.model.borrow().is_empty() {
            let this = Rc::downgrade(self);
            // The window is not on the screen yet: the Spotlight opens once it is.
            glib::idle_add_local_once(move || {
                if let Some(b) = this.upgrade() {
                    b.new_tab(true);
                }
            });
        }
        // After an update the first thing to see is what is new; then the check for the next.
        self.show_news_once();
        self.startup_updates();
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
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::settings_file(), &self.settings.borrow().to_json());
    }

    fn save_session(&self) {
        if self.private {
            return;
        }
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

    pub fn open_tab(self: &Rc<Self>, url: &str, after: Option<TabId>, select: bool) -> TabId {
        let url = if url.trim().is_empty() {
            "about:blank".to_owned()
        } else if url == newtab::NEWTAB_URI
            || url == prefs::SETTINGS_URI
            || url == WELCOME_URI
            || url == NEWS_URI
        {
            url.to_owned()
        } else {
            let template = self.settings.borrow().search_template().to_owned();
            omnibox::resolve(url, &template)
                .map_or_else(|| "about:blank".to_owned(), |u| u.to_string())
        };
        let (id, changes) = self.model.borrow_mut().open(&url, after, select);
        self.create_view(id, &url, None);
        self.apply(changes);
        id
    }

    fn close_tab(self: &Rc<Self>, id: TabId) {
        // The last tab of a space gives way to a blank one: the user stays in the space.
        let alone = {
            let m = self.model.borrow();
            m.get(id).is_some_and(|t| {
                if t.essential.is_some() {
                    // An essential's page closing with nothing else listed leaves a blank tab.
                    m.in_space(m.current_space()).count() == 0
                } else {
                    m.in_space(t.space).count() == 1 && m.len() > 1
                }
            })
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
                    b.note_title(v, &title);
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
                b.audio_changed(v);
            }
        });
        let this = Rc::downgrade(self);
        view.connect_favicon_notify(move |v| {
            if let Some(b) = this.upgrade() {
                // The icons are kept on disk by host: a private window leaves none.
                if let (Some(tex), Some(uri), false) = (v.favicon(), v.uri(), b.private) {
                    favicons::remember(&uri, &tex);
                    b.schedule_essentials();
                }
                if let Some(id) = b.id_of(v) {
                    b.sync_row(id);
                }
            }
        });
        // A video (or anything else) that asks for the full screen gets the whole window: the
        // sidebar, the bars and the frame go away until it is over. The default handler of the
        // web view then makes the window itself full screen.
        let this = Rc::downgrade(self);
        view.connect_enter_fullscreen(move |v| {
            if let Some(b) = this.upgrade() {
                if b.current_view().as_ref() == Some(v) {
                    b.set_web_fullscreen(true);
                }
            }
            false
        });
        let this = Rc::downgrade(self);
        view.connect_leave_fullscreen(move |v| {
            if let Some(b) = this.upgrade() {
                if b.current_view().as_ref() == Some(v) {
                    b.set_web_fullscreen(false);
                }
            }
            false
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
                if ev == LoadEvent::Committed {
                    if let Some(uri) = v.uri() {
                        b.https_tried.borrow_mut().remove(uri.as_str());
                    }
                    if let Some(id) = b.id_of(v) {
                        b.ext_committed(id, v);
                    }
                }
                if ev == LoadEvent::Finished {
                    if let Some(id) = b.id_of(v) {
                        b.ext_finished(id);
                    }
                }
                if ev == LoadEvent::Finished {
                    b.record_visit(v);
                    b.detect_manifest(v);
                    hide_elements(&b.filters, v);
                    b.scroll_settings(v);
                    b.push_sync_status(v);
                }
            }
        });
        // A site that does not speak HTTPS at all opens as it was asked for. A bad certificate is
        // different: the site does speak HTTPS, so there is no falling back, and the error stays.
        let this = Rc::downgrade(self);
        view.connect_load_failed_with_tls_errors(move |_, uri, _, _| {
            if let Some(b) = this.upgrade() {
                b.https_tried.borrow_mut().remove(uri);
            }
            false
        });
        let this = Rc::downgrade(self);
        view.connect_load_failed(move |v, _, uri, error| {
            let Some(b) = this.upgrade() else {
                return false;
            };
            let original = b.https_tried.borrow_mut().remove(uri);
            let Some(original) = original else {
                return false;
            };
            if error.matches(webkit6::NetworkError::Cancelled) {
                return false;
            }
            if let Some(host) = https::host_of(&original) {
                b.http_only.borrow_mut().insert(host);
            }
            let v = v.clone();
            glib::idle_add_local_once(move || v.load_uri(&original));
            true
        });
        // window.open / target=_blank: a new tab next to this one.
        let this = Rc::downgrade(self);
        view.connect_create(move |v, action| {
            let b = this.upgrade()?;
            let opener = b.id_of(v);
            // A link of an essential that leads to another site: a card over the page, no tab.
            if let (Some(id), Some(uri)) = (
                opener,
                action
                    .clone()
                    .request()
                    .and_then(|r| r.uri())
                    .map(|u| u.to_string()),
            ) {
                if action.clone().navigation_type() == NavigationType::LinkClicked
                    && b.leaves_essential(id, &uri)
                {
                    b.peek(&uri);
                    return None;
                }
            }
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
                // From an essential's page, a link to another site is shown the same way
                // without Shift: the essential stays on its own site.
                let away = this
                    .upgrade()
                    .is_some_and(|b| b.id_of(v).is_some_and(|id| b.leaves_essential(id, &uri)));
                if (shift || away) && web && action.navigation_type() == NavigationType::LinkClicked
                {
                    decision.ignore();
                    if let Some(b) = this.upgrade() {
                        b.peek(&uri);
                    }
                    return true;
                }
            }
            // HTTPS first: plain HTTP to a public site is tried over HTTPS before it is sent.
            if let (Some(b), Some(action)) = (this.upgrade(), nav.navigation_action()) {
                let get = action
                    .request()
                    .and_then(|r| r.http_method())
                    .is_none_or(|m| m.eq_ignore_ascii_case("GET"));
                let known_bad =
                    https::host_of(&uri).is_some_and(|h| b.http_only.borrow().contains(&h));
                if get && b.settings.borrow().https_first && !known_bad {
                    if let Some(secure) = https::upgrade(&uri) {
                        decision.ignore();
                        {
                            let mut tried = b.https_tried.borrow_mut();
                            if tried.len() > 64 {
                                tried.clear();
                            }
                            tried.insert(secure.clone(), uri.clone());
                        }
                        let v = v.clone();
                        glib::idle_add_local_once(move || v.load_uri(&secure));
                        return true;
                    }
                }
            }
            // A web page never gets to point the browser at a local file.
            let from_web = v
                .uri()
                .is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"));
            // An extension's page that its manifest lists as web accessible may be shown inside
            // a web page (a panel in an iframe); nothing else of an extension is reachable.
            if from_web && uri.to_ascii_lowercase().starts_with("brujula-ext:") {
                let open = this.upgrade().is_some_and(|b| b.ext_web_accessible(&uri));
                if !open {
                    decision.ignore();
                    return true;
                }
            }
            if from_web && uri.to_ascii_lowercase().starts_with("file:") {
                decision.ignore();
                return true;
            }
            if !uri.starts_with("brujula://")
                || uri == newtab::NEWTAB_URI
                || uri == prefs::SETTINGS_URI
                || uri == WELCOME_URI
                || uri == NEWS_URI
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
            if page == WELCOME_URI {
                if let Some(ask) = brujula_core::welcome::parse_action(&uri) {
                    b.welcome_ask(v, ask);
                }
                return true;
            }
            if page != newtab::NEWTAB_URI {
                return true;
            }
            if uri == newtab::CHOOSE_ENGINE_URI {
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
        self.stack.add_named(&view, Some(&format!("tab-{}", id.0)));
        self.views.borrow_mut().insert(id, view.clone());
        self.follow_focus(id, &view);
        if url == newtab::NEWTAB_URI {
            self.render_newtab(&view, false);
        } else if url == prefs::SETTINGS_URI {
            self.render_settings(&view);
        } else if url == WELCOME_URI {
            self.render_welcome(&view);
        } else if url == NEWS_URI {
            self.render_news(&view);
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
            self.emit_tab_change(&change);
            match change {
                Change::Opened(id) => {
                    self.model.borrow_mut().touch(id, now_ms());
                    self.add_row(id);
                }
                Change::Closed(id) => {
                    if self.in_split(id) {
                        self.end_split();
                    }
                    self.unfloat(id, true);
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
                    if !self.rows.borrow().contains_key(&id) {
                        // An essential's page: no row of the list is the current one.
                        self.list.unselect_all();
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
        self.schedule_essentials();
    }

    /// The page started or stopped making sound. A video in a tab in the background flips this
    /// all the time (every pause for buffering, every silent second), and each flip used to run
    /// everything a tab change runs: the rows put in order, the filter run again, the session
    /// saved. That is churn in the window while the tab in front is the one being used, so the
    /// indicator is all that is touched, and a page that goes quiet is given a moment to start
    /// again before the indicator goes away.
    fn audio_changed(self: &Rc<Self>, view: &WebView) {
        let Some(id) = self.id_of(view) else { return };
        if view.is_playing_audio() {
            self.set_audible(id, true);
            return;
        }
        let this = Rc::downgrade(self);
        let view = view.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
            if let Some(b) = this.upgrade() {
                if !view.is_playing_audio() {
                    b.set_audible(id, false);
                }
            }
        });
    }

    fn set_audible(&self, id: TabId, audible: bool) {
        if self
            .model
            .borrow()
            .get(id)
            .is_none_or(|t| t.audible == audible)
        {
            return;
        }
        let changes = self.model.borrow_mut().update(id, |t| t.audible = audible);
        for c in &changes {
            self.emit_tab_change(c);
        }
        self.sync_row(id);
    }

    fn add_row(self: &Rc<Self>, id: TabId) {
        // The page of an essential has no row: its icon is its tab.
        if self
            .model
            .borrow()
            .get(id)
            .is_some_and(|t| t.essential.is_some())
        {
            return;
        }
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
        self.wire_tab_drag(&row, id);

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
        for fid in folders.ordered() {
            let Some(folder) = folders.get(fid) else {
                continue;
            };
            let members: Vec<_> = model
                .tabs()
                .iter()
                .filter(|t| t.essential.is_none() && folder_of(t) == Some(folder.id))
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
        // Tabs inside a folder are indented, and so are folders inside folders: one step for
        // each folder that holds them.
        const STEP: i32 = 14;
        for t in model.tabs() {
            if let Some(r) = rows.get(&t.id) {
                let inside = folder_of(t);
                if inside.is_some() {
                    r.row.add_css_class("brujula-in-folder");
                } else {
                    r.row.remove_css_class("brujula-in-folder");
                }
                if let Some(content) = r.row.child() {
                    let depth = inside.map_or(0, |f| folders.depth(f));
                    content.set_margin_start(STEP * i32::try_from(depth).unwrap_or(0));
                }
            }
        }
        for folder in folders.all() {
            if let Some(content) = folder_rows.get(&folder.id).and_then(|fr| fr.row.child()) {
                content
                    .set_margin_start(STEP * i32::try_from(folders.depth(folder.id)).unwrap_or(0));
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
        } else if tab.url == WELCOME_URI {
            "Bienvenida".to_owned()
        } else if tab.url == NEWS_URI {
            "Novedades".to_owned()
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
        } else if tab.url == WELCOME_URI || tab.url == NEWS_URI {
            (
                if tab.url == NEWS_URI {
                    "Novedades"
                } else {
                    "Bienvenida"
                }
                .to_owned(),
                "ndl-lock-symbolic",
            )
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
        self.refresh_page_action();
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
            Some(WELCOME_URI) => self.render_welcome(view),
            Some(NEWS_URI) => self.render_news(view),
            _ => view.reload(),
        }
    }

    /// The preferences page, in its own tab (one is enough: it is reused).
    fn open_settings(self: &Rc<Self>, section: Option<&str>) {
        // The preferences belong to the normal window: a private one writes nothing.
        if self.private {
            return;
        }
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
            None => {
                self.open_tab(prefs::SETTINGS_URI, None, true);
            }
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
        let top = self.pending_scroll.take();
        let section = self.pending_section.borrow_mut().take();
        if top.is_none() && section.is_none() {
            return;
        }
        // The id is alphanumeric (checked when it was stored), so it cannot break the script.
        let mut script = String::new();
        if let Some(section) = section {
            script.push_str(&format!("window.showSection?.('{section}');"));
        }
        if let Some(top) = top {
            script.push_str(&format!("document.scrollingElement.scrollTop = {top};"));
        }
        view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }

    /// One change made on the preferences page: checked, stored and put into effect.
    fn change_setting(self: &Rc<Self>, key: &str, value: &str) {
        // Bookmarks and spaces are not settings: they have their own stores.
        if let Some(rest) = key.strip_prefix("space.") {
            self.change_space(rest, value);
            return;
        }
        if let Some(what) = key.strip_prefix("boost.") {
            self.change_boost(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("archive.") {
            self.change_archive(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("profile.") {
            self.change_profile(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("essential.") {
            self.change_essential(what, value);
            return;
        }
        if key == "bookmark.remove" {
            self.bookmarks.borrow_mut().remove(value);
            self.save_bookmarks();
            self.refresh_bookmark_action();
            return;
        }
        if key == "redraw" {
            self.redraw_settings();
            return;
        }
        if let Some(what) = key.strip_prefix("ext.") {
            self.change_extension(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("history.") {
            self.change_history(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("update.") {
            self.change_update(what);
            return;
        }
        if let Some(what) = key.strip_prefix("webapp.") {
            self.change_webapp(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("import.") {
            self.change_import(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("ai.") {
            self.change_ai(what, value);
            return;
        }
        if let Some(what) = key.strip_prefix("pw.") {
            self.change_passwords(what, value);
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
        if let Some(choice) = effect.folder {
            self.choose_sync_folder(choice);
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
            Theme::System if self.system_dark.get() => ndl::ColorScheme::ForceDark,
            Theme::System => ndl::ColorScheme::ForceLight,
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

    /// Hides everything around the page while a page is shown full screen, and puts it back
    /// when it leaves.
    fn set_web_fullscreen(&self, on: bool) {
        if on {
            self.window.add_css_class("brujula-fullscreen");
            self.split.set_collapsed(true);
            self.split.set_show_sidebar(false);
            self.left_hot.set_visible(false);
            self.top_hot.set_visible(false);
            self.bar_revealer.set_reveal_child(false);
            self.bar_revealer.set_visible(false);
            self.topbar.set_visible(false);
            self.ess.rail.set_visible(false);
        } else {
            self.window.remove_css_class("brujula-fullscreen");
            self.bar_revealer.set_visible(true);
            self.apply_layout();
        }
    }

    /// Sidebar width, pinned or floating, and whether the edge wakes it.
    fn apply_layout(&self) {
        let s = self.settings.borrow();
        let width = i32::try_from(s.appearance.sidebar_width).unwrap_or(200);
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
        self.dl.place(horizontal);
        self.place_history(horizontal);
        let has_essentials = !self.essentials.borrow().is_empty();
        self.ess.grid.set_visible(has_essentials && !horizontal);
        self.ess.rail.set_visible(has_essentials && horizontal);
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
        let choosing = choosing || !settings.search_engine_chosen;
        let lang = glib::language_names()
            .first()
            .map(|l| l.to_string())
            .unwrap_or_default();
        let hour = glib::DateTime::now_local()
            .ok()
            .and_then(|d| u32::try_from(d.hour()).ok())
            .unwrap_or(12);
        let theme = match settings.appearance.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        newtab::render(&newtab::Page {
            choosing,
            lang: &lang,
            theme,
            accent: &settings.appearance.accent,
            hour,
        })
    }

    /// A new tab: a blank page in front and the Spotlight over it, with the bookmarks as icons.
    /// Leaving the Spotlight without choosing anything takes the blank tab away again, unless
    /// `keep` says it is all there is to show (the browser has just opened).
    fn new_tab(self: &Rc<Self>, keep: bool) {
        let blank = self
            .current_tab_url()
            .is_some_and(|u| u == newtab::NEWTAB_URI)
            .then(|| self.model.borrow().selected())
            .flatten();
        let reused = blank.is_some();
        let id = blank.unwrap_or_else(|| self.open_tab(newtab::NEWTAB_URI, None, true));
        // Until a search engine is chosen the page shows the picker; the Spotlight would hide it.
        if !self.settings.borrow().search_engine_chosen {
            return;
        }
        self.fresh_tab.set((!keep && !reused).then_some(id));
        self.spotlight.open("");
    }

    fn current_tab_url(&self) -> Option<String> {
        let m = self.model.borrow();
        m.selected().and_then(|id| m.get(id).map(|t| t.url.clone()))
    }

    /// The Spotlight of a new tab was closed with nothing chosen: the blank tab goes, and the
    /// one before it is shown again. A lone blank tab stays.
    fn cancel_new_tab(self: &Rc<Self>) {
        let Some(id) = self.fresh_tab.take() else {
            return;
        };
        let blank = self
            .model
            .borrow()
            .get(id)
            .is_some_and(|t| t.url == newtab::NEWTAB_URI);
        if blank && self.model.borrow().selected() == Some(id) && self.model.borrow().len() > 1 {
            self.close_tab(id);
        }
    }

    /// The tab a new-tab Spotlight opened, if it is still blank and not the one in front: the
    /// user went somewhere else, so it has no reason to stay.
    fn drop_fresh_tab(self: &Rc<Self>) {
        let Some(id) = self.fresh_tab.take() else {
            return;
        };
        let blank = self
            .model
            .borrow()
            .get(id)
            .is_some_and(|t| t.url == newtab::NEWTAB_URI);
        if blank && self.model.borrow().selected() != Some(id) {
            self.close_tab(id);
        }
    }

    /// The bookmarks as the icons the Spotlight shows while nothing is typed.
    fn bookmark_icons(&self) -> Vec<Item> {
        self.bookmarks
            .borrow()
            .items()
            .iter()
            .take(24)
            .map(|b| Item {
                action: Action::Go(b.url.clone()),
                title: b.title.clone(),
                detail: b.url.clone(),
                icon: "ndl-bookmark-symbolic",
                texture: favicons::lookup(&b.url),
            })
            .collect()
    }

    /// What the Spotlight lists for `text`.
    fn spotlight_items(&self, text: &str) -> Vec<Item> {
        // Nothing typed: the Spotlight shows the bookmarks as icons, not as rows.
        if text.trim().is_empty() {
            return Vec::new();
        }
        let settings = self.settings.borrow();
        let engine = search::engine_or_default(&settings.search_engine);
        let model = self.model.borrow();
        let tabs: Vec<_> = model
            .tabs()
            .iter()
            .filter(|t| {
                t.url != newtab::NEWTAB_URI
                    && t.url != prefs::SETTINGS_URI
                    && t.url != WELCOME_URI
                    && t.url != NEWS_URI
            })
            .cloned()
            .collect();
        let marks = self.bookmarks.borrow();
        let views = self.views.borrow();
        let history = self.history.borrow();
        let visited = history.search(text, 8);
        let ask = self.ai_search_row(text);
        let items = suggest::suggestions_with_history(
            text,
            &tabs,
            marks.items(),
            &visited,
            settings.search_template(),
        )
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
                (Kind::History, _) if nodalia_gtk::has_icon("ndl-history-symbolic") => {
                    "ndl-history-symbolic"
                }
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
        .collect::<Vec<Item>>();
        // With the assistant running the search box, a question or a task comes first.
        Self::with_ask(items, ask)
    }

    fn with_ask(mut items: Vec<Item>, ask: Option<Item>) -> Vec<Item> {
        if let Some(a) = ask {
            items.insert(0, a);
        }
        items
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
            Action::Ask(text) => self.ai_ask(&text),
            Action::Search(q) => {
                let template = self.settings.borrow().search_template().to_owned();
                let url = omnibox::search_url(&q, &template);
                self.navigate(url.as_str());
            }
        }
        self.drop_fresh_tab();
    }

    fn navigate(self: &Rc<Self>, url: &str) {
        match self.current_view() {
            Some(v) => v.load_uri(url),
            None => {
                self.open_tab(url, None, true);
            }
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
        let add = |label: &str, f: Handler| {
            append_popover_entry(self, &pop, &col, label, f);
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
        if !self.in_split(id) && !self.is_floating(id) {
            add(
                "Sacar a una ventana flotante",
                Box::new(move |b| b.float_tab(id)),
            );
        }
        add(
            "Boost de este sitio…",
            Box::new(move |b| b.boost_site_of(id)),
        );
        if !self.private {
            add(
                "Añadir a esenciales",
                Box::new(move |b| b.add_tab_to_essentials(id)),
            );
        }
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
            .map(|f| (f.id, folder_path(&self.folders.borrow(), f.id)))
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
/// `Trabajo › Proyectos` for a folder inside another.
fn folder_path(folders: &Folders, id: u32) -> String {
    let mut names = Vec::new();
    let mut at = Some(id);
    while let Some(i) = at {
        let Some(f) = folders.get(i) else { break };
        names.push(f.name.clone());
        at = f.parent;
        if names.len() > 4 {
            break;
        }
    }
    names.reverse();
    names.join(" › ")
}

fn harden(view: &WebView) {
    harden_settings(view);
    view.connect_permission_request(|_, request| {
        request.deny();
        true
    });
}

/// The settings every view gets; a view that asks the user about permissions (a web app) adds
/// its own handler instead of the one that refuses all.
pub(crate) fn harden_settings(view: &WebView) {
    if let Some(s) = WebViewExt::settings(view) {
        s.set_enable_developer_extras(true);
        s.set_enable_back_forward_navigation_gestures(true);
        s.set_javascript_can_open_windows_automatically(false);
        s.set_allow_file_access_from_file_urls(false);
        s.set_allow_universal_access_from_file_urls(false);
        s.set_allow_modal_dialogs(false);
        s.set_enable_write_console_messages_to_stdout(false);
        // Linux, always: WebKitGTK would say Macintosh to some sites.
        s.set_user_agent(Some(brujula_core::useragent::USER_AGENT));
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
    tabs.append(
        Some("Nueva ventana privada"),
        Some("app.new-private-window"),
    );
    m.append_section(None, &tabs);
    let opts = gio::Menu::new();
    opts.append(Some("Barra lateral"), Some("win.toggle-sidebar"));
    opts.append(Some("Bloqueador de anuncios"), Some("win.toggle-adblock"));
    opts.append(Some("Cambiar buscador"), Some("win.change-engine"));
    opts.append(Some("Extensiones"), Some("win.open-extensions"));
    opts.append(Some("Añadir a esenciales"), Some("win.add-essential"));
    opts.append(
        Some("Instalar como aplicación web…"),
        Some("win.install-webapp"),
    );
    opts.append(Some("Asistente de IA"), Some("win.toggle-assistant"));
    opts.append(Some("Preferencias"), Some("win.open-settings"));
    m.append_section(None, &opts);
    let about = gio::Menu::new();
    about.append(Some("Acerca de Brújula"), Some("win.about"));
    m.append_section(None, &about);
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

/// Whether `widget` or anything inside it has a popover showing. Popovers are children of the
/// widget they belong to, so walking the tree finds them.
fn has_open_popover(widget: &gtk::Widget) -> bool {
    if widget
        .downcast_ref::<gtk::Popover>()
        .is_some_and(|p| p.is_visible())
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if has_open_popover(&c) {
            return true;
        }
        child = c.next_sibling();
    }
    false
}

/// Whether a view shows one of the browser's own pages rather than a web page.
fn shows_own_page(view: &WebView) -> bool {
    view.uri().is_some_and(|u| brujula_core::is_own_page(&u))
}

/// Gives a session a file for its cookies. Without one WebKit keeps them in memory only, and a
/// restart (an update, a crash, closing the window) signs the person out of everything.
pub(crate) fn keep_cookies(session: &NetworkSession, file: &std::path::Path) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Some(cookies) = session.cookie_manager() {
        cookies.set_persistent_storage(
            &file.to_string_lossy(),
            webkit6::CookiePersistentStorage::Sqlite,
        );
    }
}

/// The saved history. A file that cannot be read is moved aside, never written over with an
/// empty history, so the pages it holds can still be recovered.
fn load_history() -> History {
    let file = paths::history_file();
    let Ok(text) = std::fs::read_to_string(&file) else {
        return History::default();
    };
    History::parse(&text).unwrap_or_else(|| {
        let aside = file.with_extension(format!("json.unreadable-{}", now_ms()));
        let _ = std::fs::rename(&file, aside);
        History::default()
    })
}
