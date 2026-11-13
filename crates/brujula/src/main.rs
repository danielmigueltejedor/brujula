mod browser;
mod dialogs;
mod favicons;
mod filters;
mod fonts;
mod icons;
mod keyring;
mod notify;
mod paths;
mod profiles;
mod save_download;
mod spotlight;
mod tile;
mod webapp;

use gtk::prelude::*;
use ndl::gtk;
use nodalia_gtk as ndl;

thread_local! {
    /// Set when the first launch asked for a private window.
    static OPEN_PRIVATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn main() -> gtk::glib::ExitCode {
    paths::secure_dirs();
    // `brujula --webapp <id>` is a web app: a site in a window of its own, in its own process.
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--webapp") {
        return match args.get(at + 1) {
            Some(id) => webapp::run(id),
            None => {
                eprintln!("brujula: --webapp needs the id of a web app");
                gtk::glib::ExitCode::FAILURE
            }
        };
    }
    gtk::glib::set_prgname(Some("brujula"));
    gtk::glib::set_application_name("Brújula");

    // `brujula --profile <id>` runs as that profile. Without it, and when there is more than one
    // person (or the only profile has a password), the chooser asks who is browsing first.
    match args.iter().position(|a| a == "--profile") {
        Some(at) => {
            if let Some(id) = args.get(at + 1) {
                paths::set_profile(id);
            }
        }
        None => {
            let profiles = profiles::load();
            let private = args.iter().any(|a| a == "--private" || a == "-p");
            if !private && profiles::needs_chooser(&profiles) {
                let Some(id) = profiles::choose(profiles) else {
                    return gtk::glib::ExitCode::SUCCESS;
                };
                // The browser itself starts again as that profile: its application id, its
                // folders and its windows are all decided before it starts.
                use std::os::unix::process::CommandExt;
                if let Ok(exe) = std::env::current_exe() {
                    let error = std::process::Command::new(exe)
                        .arg("--profile")
                        .arg(&id)
                        .args(&args[1..])
                        .exec();
                    eprintln!("brujula: could not start the profile: {error}");
                    return gtk::glib::ExitCode::FAILURE;
                }
                paths::set_profile(&id);
            }
        }
    }

    let app = gtk::Application::builder()
        .application_id(paths::app_id())
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|app| {
        // Window controls at the left, over the sidebar, like Zen and Arc.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_decoration_layout(Some(":minimize,maximize,close"));
        }
        ndl::init();
        icons::install();
        browser::install_css();
        browser::install_app_actions(app);
    });
    // `brujula --private` opens a private window: at once if the browser is not running, through
    // the running one if it is.
    app.add_main_option(
        "private",
        gtk::glib::Char::from(b'p'),
        gtk::glib::OptionFlags::NONE,
        gtk::glib::OptionArg::None,
        "Abre una ventana privada",
        None,
    );
    // Accepted here so the option parser does not refuse it; `main` read it before.
    app.add_main_option(
        "profile",
        gtk::glib::Char::from(0),
        gtk::glib::OptionFlags::NONE,
        gtk::glib::OptionArg::String,
        "Abre Brújula con ese perfil",
        Some("ID"),
    );
    app.connect_handle_local_options(|app, options| {
        if !options.contains("private") {
            return std::ops::ControlFlow::Continue(());
        }
        if app.register(gtk::gio::Cancellable::NONE).is_err() {
            return std::ops::ControlFlow::Break(gtk::glib::ExitCode::FAILURE);
        }
        if app.is_remote() {
            app.activate_action("new-private-window", None);
            return std::ops::ControlFlow::Break(gtk::glib::ExitCode::SUCCESS);
        }
        OPEN_PRIVATE.with(|f| f.set(true));
        std::ops::ControlFlow::Continue(())
    });
    app.connect_activate(|app| {
        if OPEN_PRIVATE.with(std::cell::Cell::take) {
            browser::present_private(app, &[]);
        } else {
            browser::present(app, &[]);
        }
    });
    app.connect_open(|app, files, _| {
        let urls: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        if OPEN_PRIVATE.with(std::cell::Cell::take) {
            browser::present_private(app, &urls);
        } else {
            browser::present(app, &urls);
        }
    });
    app.run()
}
