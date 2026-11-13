//! Web apps: one site in a window of its own, started with `brujula --webapp <id>`.
//!
//! Each app is its own process with its own profile (cookies, storage and cache apart from the
//! browser and from the other apps), its own application id (so the launcher and the dock tell
//! it from the browser) and the ad blocker. It has no tabs and no address bar: the links of its
//! own site stay in the window, any other goes to the default browser.

use std::cell::RefCell;
use std::rc::Rc;

use brujula_core::settings::Settings;
use brujula_core::webapps::{self, WebApp, Webapps};
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use ndl::gtk;
use nodalia_gtk as ndl;
use webkit6::prelude::*;
use webkit6::{
    LoadEvent, NavigationPolicyDecision, NavigationType, NetworkSession, PolicyDecisionType,
    UserContentManager, UserMediaPermissionRequest, WebView,
};

use crate::dialogs;
use crate::filters::Filters;
use crate::icons;
use crate::notify::Notifier;
use crate::paths;

pub fn registry() -> Webapps {
    std::fs::read_to_string(paths::webapps_file())
        .map(|t| Webapps::from_json(&t))
        .unwrap_or_default()
}

pub fn save_registry(w: &Webapps) -> std::io::Result<()> {
    paths::write_atomic(&paths::webapps_file(), &w.to_json())
}

fn launcher_path(id: &str) -> std::path::PathBuf {
    glib::user_data_dir()
        .join("applications")
        .join(format!("brujula-webapp-{id}.desktop"))
}

fn icon_path(id: &str) -> std::path::PathBuf {
    paths::webapp_data_dir(id).join("icon.png")
}

/// How the launcher starts the browser: the AppImage itself when that is what is running.
fn launcher_exe() -> String {
    std::env::var("APPIMAGE")
        .ok()
        .filter(|p| p.starts_with('/'))
        .unwrap_or_else(|| "brujula".to_owned())
}

/// Writes the launcher entry of an app; the icon is the favicon when there is one.
pub fn write_launcher(app: &WebApp) -> std::io::Result<()> {
    let icon = if icon_path(&app.id).exists() {
        icon_path(&app.id).to_string_lossy().into_owned()
    } else {
        paths::APP_ID.to_owned()
    };
    let text = webapps::desktop_entry(app, &launcher_exe(), &icon, paths::APP_ID);
    paths::write_atomic(&launcher_path(&app.id), &text)
}

/// Keeps the icon of a site for its launcher, on a rounded square (see `tile`).
pub fn save_icon(app: &WebApp, texture: &gdk::Texture) {
    let path = icon_path(&app.id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if crate::tile::write_tile(texture, &path).is_ok() {
        let _ = write_launcher(app);
    }
}

/// Deletes an app: its launcher, its profile and its place in the list.
pub fn remove(id: &str) {
    let mut w = registry();
    w.remove(id);
    let _ = save_registry(&w);
    let _ = std::fs::remove_file(launcher_path(id));
    let _ = std::fs::remove_dir_all(paths::webapp_data_dir(id));
    let _ = std::fs::remove_dir_all(paths::webapp_cache_dir(id));
}

/// Starts an app in a process of its own.
pub fn launch(id: &str) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let child = std::process::Command::new(exe)
        .arg("--webapp")
        .arg(id)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(mut c) = child {
        std::thread::spawn(move || {
            let _ = c.wait();
        });
    }
}

/// Runs the app `id` until its window closes.
pub fn run(id: &str) -> glib::ExitCode {
    let Some(app) = registry().get(id).cloned() else {
        eprintln!("brujula: there is no web app «{id}»");
        return glib::ExitCode::FAILURE;
    };
    glib::set_prgname(Some("brujula"));
    glib::set_application_name(&app.name);
    let gapp = gtk::Application::builder()
        .application_id(webapps::app_id(paths::APP_ID, &app.id))
        .build();
    gapp.connect_startup(|app| {
        ndl::init();
        icons::install();
        app.set_accels_for_action("win.back", &["<Alt>Left"]);
        app.set_accels_for_action("win.forward", &["<Alt>Right"]);
        app.set_accels_for_action("win.reload", &["<Ctrl>r", "F5"]);
    });
    // What the window needs to outlive the call that builds it.
    type Kept = (Rc<Filters>, Rc<Notifier>);
    let holder: Rc<RefCell<Option<Kept>>> = Rc::new(RefCell::new(None));
    gapp.connect_activate(move |gapp| {
        if let Some(w) = gapp.active_window() {
            w.present();
            return;
        }
        *holder.borrow_mut() = Some(present(gapp, &app));
    });
    gapp.run_with_args(&["brujula"])
}

fn present(gapp: &gtk::Application, app: &WebApp) -> (Rc<Filters>, Rc<Notifier>) {
    let settings = std::fs::read_to_string(paths::settings_file())
        .map(|t| Settings::from_json(&t))
        .unwrap_or_default();
    let session = NetworkSession::new(
        Some(&paths::webapp_data_dir(&app.id).to_string_lossy()),
        Some(&paths::webapp_cache_dir(&app.id).to_string_lossy()),
    );
    if let Some(dm) = session.website_data_manager() {
        dm.set_favicons_enabled(true);
    }
    crate::browser::keep_cookies(&session, &paths::webapp_cookies_file(&app.id));
    let ucm = UserContentManager::new();
    let filters = Filters::new(ucm.clone());
    filters.set_allow(settings.allowlist.clone());
    let view = WebView::builder()
        .network_session(&session)
        .user_content_manager(&ucm)
        .hexpand(true)
        .vexpand(true)
        .build();
    crate::browser::harden_settings(&view);

    // The look the user chose for the browser: theme and accent.
    apply_look(&settings);
    let win = ndl::ApplicationWindow::new(gapp);
    win.set_title(Some(&app.name));
    win.set_default_size(1100, 760);
    let back = nav_button("ndl-arrow-left-symbolic", "Atrás (Alt+←)", "win.back");
    let forward = nav_button(
        "ndl-arrow-right-symbolic",
        "Adelante (Alt+→)",
        "win.forward",
    );
    let reload = nav_button("ndl-refresh-symbolic", "Recargar (Ctrl+R)", "win.reload");
    let header = ndl::HeaderBar::new();
    header.pack_start(&back);
    header.pack_start(&forward);
    header.pack_start(&reload);
    let toolbar = ndl::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&view));
    win.set_content(Some(&toolbar));

    // Notifications: asked about once, kept in the profile of this app.
    let notifier = Notifier::new(
        gapp,
        &win,
        Some(paths::webapp_data_dir(&app.id).join("site-permissions.json")),
        "Se recuerda para esta aplicación.",
    );
    {
        let win = win.clone();
        notifier.attach(&view, Rc::new(move |_| win.present()));
    }

    for (name, f) in [
        (
            "back",
            Box::new(|v: &WebView| v.go_back()) as Box<dyn Fn(&WebView)>,
        ),
        ("forward", Box::new(|v: &WebView| v.go_forward())),
        (
            "reload",
            Box::new(|v: &WebView| {
                if v.is_loading() {
                    v.stop_loading()
                } else {
                    v.reload()
                }
            }),
        ),
    ] {
        let a = gio::SimpleAction::new(name, None);
        let v = view.clone();
        a.connect_activate(move |_, _| f(&v));
        win.add_action(&a);
    }

    let sync_nav = {
        let (back, forward) = (back.clone(), forward.clone());
        move |v: &WebView| {
            back.set_sensitive(v.can_go_back());
            forward.set_sensitive(v.can_go_forward());
        }
    };
    sync_nav(&view);
    {
        let (win, name) = (win.clone(), app.name.clone());
        view.connect_title_notify(move |v| {
            let t = v.title().map(|t| t.to_string()).unwrap_or_default();
            win.set_title(Some(if t.is_empty() { &name } else { &t }));
        });
    }
    {
        let filters = filters.clone();
        view.connect_load_changed(move |v, ev| {
            sync_nav(v);
            if ev == LoadEvent::Finished {
                crate::browser::hide_elements(&filters, v);
            }
        });
    }
    {
        // The first favicon becomes the icon of the launcher.
        let app = app.clone();
        view.connect_favicon_notify(move |v| {
            if let Some(tex) = v.favicon() {
                if !icon_path(&app.id).exists() {
                    save_icon(&app, &tex);
                }
            }
        });
    }
    {
        let home = app.url.clone();
        view.connect_decide_policy(move |v, decision, kind| {
            if kind != PolicyDecisionType::NavigationAction
                && kind != PolicyDecisionType::NewWindowAction
            {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<NavigationPolicyDecision>() else {
                return false;
            };
            let Some(action) = nav.navigation_action() else {
                return false;
            };
            let uri = action
                .request()
                .and_then(|r| r.uri())
                .map(|u| u.to_string())
                .unwrap_or_default();
            let web = uri.starts_with("https://") || uri.starts_with("http://");
            let mail = uri.starts_with("mailto:") || uri.starts_with("tel:");
            let new_window = kind == PolicyDecisionType::NewWindowAction;
            let clicked = action.navigation_type() == NavigationType::LinkClicked;
            if web && webapps::same_site(&home, &uri) {
                if new_window {
                    decision.ignore();
                    v.load_uri(&uri);
                    return true;
                }
                return false;
            }
            if web && (new_window || clicked) || mail {
                decision.ignore();
                let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
                return true;
            }
            false
        });
    }
    {
        let win = win.clone();
        view.connect_permission_request(move |_, request| {
            let Some(media) = request.downcast_ref::<UserMediaPermissionRequest>() else {
                request.deny();
                return true;
            };
            let what = match (media.is_for_audio_device(), media.is_for_video_device()) {
                (true, true) => "la cámara y el micrófono",
                (false, true) => "la cámara",
                _ => "el micrófono",
            };
            let request = request.clone();
            dialogs::ask(
                &win,
                &format!("¿Permitir usar {what}?"),
                "Solo mientras esta aplicación esté abierta.",
                &[
                    dialogs::Choice::plain("deny", "No permitir"),
                    dialogs::Choice::suggested("allow", "Permitir"),
                ],
                "deny",
                "deny",
                move |choice| {
                    if choice == "allow" {
                        request.allow();
                    } else {
                        request.deny();
                    }
                },
            );
            true
        });
    }
    // Downloads: the Downloads folder, cleaned names, a program only if the person says so.
    {
        let win = win.clone();
        session.connect_download_started(move |_, download| {
            let win = win.clone();
            download.connect_decide_destination(move |d, suggested| {
                crate::save_download::decide(&win, d, suggested, |_| {});
                true
            });
        });
    }

    win.present();
    let url = app.url.clone();
    let blocker = filters.clone();
    let on = settings.adblock_enabled;
    glib::spawn_future_local(async move {
        if on {
            blocker.load_compiled().await;
        }
        view.load_uri(&url);
    });
    (filters, notifier)
}

/// Theme and accent of the browser's preferences; «Sistema» follows the desktop.
fn apply_look(settings: &Settings) {
    use brujula_core::settings::{text_on, Theme, ACCENT_DEFAULT};
    let a = &settings.appearance;
    let style = ndl::StyleManager::default();
    match a.theme {
        Theme::Light => style.set_color_scheme(ndl::ColorScheme::ForceLight),
        Theme::Dark => style.set_color_scheme(ndl::ColorScheme::ForceDark),
        Theme::System => {}
    }
    if a.accent != ACCENT_DEFAULT {
        style.set_accent(Some((&a.accent, text_on(&a.accent))));
    }
}

fn nav_button(icon: &str, tip: &str, action: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .action_name(action)
        .css_classes(["flat"])
        .build()
}
