//! Making web apps: from the page that is open, or from an address on the preferences page.
//! The apps themselves run in their own process (see `crate::webapp`).

use super::*;

use crate::webapp;

impl Browser {
    /// Menu → «Instalar como aplicación web…»: asks for a name and makes the app.
    pub(super) fn install_webapp_dialog(self: &Rc<Self>) {
        if self.private {
            return;
        }
        let Some(view) = self.current_view() else {
            return;
        };
        let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return self.webapp_alert(
                "Esta página no se puede instalar",
                "Solo las páginas web (http o https) pueden ser una aplicación.",
            );
        }
        let title = view.title().map(|t| t.to_string()).unwrap_or_default();
        let name = if title.trim().is_empty() {
            host_of(&url)
        } else {
            title.chars().take(40).collect()
        };
        let icon = view.favicon();
        let this = self.clone();
        self.ask_text(
            "Instalar como aplicación web",
            "Se abre en su propia ventana, sin pestañas, con su icono en el lanzador y su propio perfil (cookies y sesiones aparte).",
            &name,
            "Instalar",
            Box::new(move |name| this.create_webapp(&name, &url, icon.as_ref())),
        );
    }

    fn webapp_alert(&self, message: &str, detail: &str) {
        gtk::AlertDialog::builder()
            .message(message)
            .detail(detail)
            .buttons(["Entendido"])
            .build()
            .show(Some(&self.window));
    }

    /// Adds the app to the list, writes its launcher, and offers to open it.
    fn create_webapp(self: &Rc<Self>, name: &str, url: &str, icon: Option<&gdk::Texture>) {
        let mut apps = webapp::registry();
        let Some(app) = apps.add(name, url).cloned() else {
            return self.webapp_alert(
                "No se pudo crear la aplicación",
                "La dirección no es válida o ya hay demasiadas aplicaciones.",
            );
        };
        if webapp::save_registry(&apps).is_err() {
            return self.webapp_alert(
                "No se pudo crear la aplicación",
                "No se pudo guardar la lista.",
            );
        }
        if let Some(tex) = icon {
            webapp::save_icon(&app, tex);
        }
        if webapp::write_launcher(&app).is_err() {
            return self.webapp_alert(
                "No se pudo crear el lanzador",
                "No se pudo escribir en la carpeta de aplicaciones.",
            );
        }
        self.redraw_settings();
        let id = app.id.clone();
        gtk::AlertDialog::builder()
            .message(format!("«{}» está lista", app.name))
            .detail("Búscala por su nombre en el lanzador de aplicaciones.")
            .buttons(["Cerrar", "Abrir ahora"])
            .cancel_button(0)
            .default_button(1)
            .build()
            .choose(Some(&self.window), gio::Cancellable::NONE, move |res| {
                if res.ok() == Some(1) {
                    webapp::launch(&id);
                }
            });
    }

    /// One of the `webapp.*` changes of the preferences page.
    pub(super) fn change_webapp(self: &Rc<Self>, what: &str, value: &str) {
        if self.private {
            return;
        }
        match what {
            "add" => {
                let (url, name) = value.split_once('\n').unwrap_or((value, ""));
                let url = url.trim();
                let url = if url.contains("://") {
                    url.to_owned()
                } else {
                    format!("https://{url}")
                };
                self.create_webapp(name, &url, None);
            }
            "open" => webapp::launch(value),
            "remove" => webapp::remove(value),
            _ => {}
        }
    }

    /// A small window that asks for a line of text.
    fn ask_text(
        self: &Rc<Self>,
        title: &str,
        detail: &str,
        initial: &str,
        ok_label: &str,
        done: Box<dyn FnOnce(String)>,
    ) {
        let win = gtk::Window::builder()
            .transient_for(&self.window)
            .modal(true)
            .resizable(false)
            .default_width(420)
            .title(title)
            .build();
        let col = gtk::Box::new(gtk::Orientation::Vertical, 12);
        for set in [
            gtk::Widget::set_margin_top,
            gtk::Widget::set_margin_bottom,
            gtk::Widget::set_margin_start,
            gtk::Widget::set_margin_end,
        ] {
            set(col.upcast_ref(), 20);
        }
        col.append(
            &gtk::Label::builder()
                .label(title)
                .css_classes(["title-3"])
                .xalign(0.0)
                .build(),
        );
        col.append(
            &gtk::Label::builder()
                .label(detail)
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        let entry = gtk::Entry::builder().text(initial).max_length(40).build();
        col.append(&entry);
        let cancel = gtk::Button::with_label("Cancelar");
        let ok = gtk::Button::builder()
            .label(ok_label)
            .css_classes(["suggested-action"])
            .build();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&cancel);
        buttons.append(&ok);
        col.append(&buttons);
        win.set_child(Some(&col));

        let done = Rc::new(RefCell::new(Some(done)));
        let accept = {
            let (win, entry, done) = (win.clone(), entry.clone(), done.clone());
            Rc::new(move || {
                let text = entry.text().to_string();
                let d = done.borrow_mut().take();
                win.close();
                if let Some(d) = d {
                    d(text);
                }
            })
        };
        {
            let accept = accept.clone();
            ok.connect_clicked(move |_| accept());
        }
        {
            let accept = accept.clone();
            entry.connect_activate(move |_| accept());
        }
        {
            let win = win.clone();
            cancel.connect_clicked(move |_| win.close());
        }
        win.present();
        entry.grab_focus();
    }
}
