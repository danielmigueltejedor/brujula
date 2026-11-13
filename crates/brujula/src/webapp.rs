//! Web apps: one site in a window of its own, started with `brujula --webapp <id>`.
//!
//! Each app is its own process with its own profile (cookies, storage and cache apart from the
//! browser and from the other apps), its own application id (so the launcher and the dock tell
//! it from the browser) and the ad blocker. It has no tabs and no address bar: the links of its
//! own site stay in the window, any other goes to the default browser.

use std::cell::RefCell;
use std::rc::Rc;

use brujula_core::downloads;
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

use crate::filters::Filters;
use crate::icons;
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

/// Keeps the icon of a site for its launcher.
pub fn save_icon(app: &WebApp, texture: &gdk::Texture) {
    let path = icon_path(&app.id);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if texture.save_to_png(&path).is_ok() {
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
    let holder: Rc<RefCell<Option<Rc<Filters>>>> = Rc::new(RefCell::new(None));
    gapp.connect_activate(move |gapp| {
        if let Some(w) = gapp.active_window() {
            w.present();
            return;
        }
        let filters = present(gapp, &app);
        *holder.borrow_mut() = Some(filters);
    });
    gapp.run_with_args(&["brujula"])
}

fn present(gapp: &gtk::Application, app: &WebApp) -> Rc<Filters> {
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

    let win = gtk::ApplicationWindow::builder()
        .application(gapp)
        .title(&app.name)
        .default_width(1100)
        .default_height(760)
        .build();
    let back = nav_button("ndl-arrow-left-symbolic", "Atrás (Alt+←)", "win.back");
    let forward = nav_button(
        "ndl-arrow-right-symbolic",
        "Adelante (Alt+→)",
        "win.forward",
    );
    let reload = nav_button("ndl-refresh-symbolic", "Recargar (Ctrl+R)", "win.reload");
    let header = gtk::HeaderBar::new();
    header.pack_start(&back);
    header.pack_start(&forward);
    header.pack_start(&reload);
    win.set_titlebar(Some(&header));
    win.set_child(Some(&view));

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
            gtk::AlertDialog::builder()
                .message(format!("¿Permitir usar {what}?"))
                .detail("Solo mientras esta aplicación esté abierta.")
                .buttons(["No permitir", "Permitir"])
                .cancel_button(0)
                .default_button(0)
                .build()
                .choose(Some(&win), gio::Cancellable::NONE, move |res| {
                    if res.ok() == Some(1) {
                        request.allow();
                    } else {
                        request.deny();
                    }
                });
            true
        });
    }
    // Downloads: the Downloads folder, cleaned names, nothing that can run.
    {
        let win = win.clone();
        session.connect_download_started(move |_, download| {
            let win = win.clone();
            download.connect_decide_destination(move |d, suggested| {
                let dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                    .unwrap_or_else(glib::home_dir);
                let name = downloads::safe_file_name(suggested);
                if downloads::is_dangerous(&name) {
                    d.cancel();
                    gtk::AlertDialog::builder()
                        .message("Descarga bloqueada")
                        .detail(format!("«{name}» es un archivo que se puede ejecutar."))
                        .buttons(["Entendido"])
                        .build()
                        .show(Some(&win));
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
    filters
}

fn nav_button(icon: &str, tip: &str, action: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .action_name(action)
        .css_classes(["flat"])
        .build()
}
