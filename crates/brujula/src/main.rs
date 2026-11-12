mod browser;
mod filters;
mod paths;

use gtk::prelude::*;
use ndl::gtk;
use nodalia_gtk as ndl;

fn main() -> gtk::glib::ExitCode {
    gtk::glib::set_prgname(Some("brujula"));
    gtk::glib::set_application_name("Brújula");

    let app = gtk::Application::builder()
        .application_id(paths::APP_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|app| {
        ndl::init();
        browser::install_css();
        browser::install_app_actions(app);
    });
    app.connect_activate(|app| browser::present(app, &[]));
    app.connect_open(|app, files, _| {
        let urls: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        browser::present(app, &urls);
    });
    app.run()
}
