//! What the page in front can be asked to do besides loading: zoom (kept per site), copy its
//! link, capture it, print it, mute it, inspect it, and jump to the nth tab. Each is an action
//! of the window, so a shortcut, a menu entry and the Spotlight's command list all run the same.

use super::*;

use brujula_core::zoom;
use webkit6::{SnapshotOptions, SnapshotRegion};

impl Browser {
    /// Registers the actions and their shortcuts' targets (the shortcuts are in
    /// `install_app_actions`).
    pub(super) fn wire_page_tools(self: &Rc<Self>) {
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
            "find",
            Box::new(|b| {
                if let Some(v) = b.current_view() {
                    b.find.open(&v);
                }
            }),
        );
        action("zoom-in", Box::new(|b| b.zoom_by(true)));
        action("zoom-out", Box::new(|b| b.zoom_by(false)));
        action("zoom-reset", Box::new(|b| b.zoom_to(zoom::DEFAULT)));
        action("clear-tabs", Box::new(|b| b.clear_tabs()));
        action("copy-link", Box::new(|b| b.copy_link()));
        action("screenshot", Box::new(|b| b.capture_page()));
        action("print", Box::new(|b| b.print_page()));
        action("toggle-mute", Box::new(|b| b.toggle_mute_current()));
        action(
            "inspector",
            Box::new(|b| {
                if let Some(i) = b.current_view().and_then(|v| v.inspector()) {
                    i.show();
                }
            }),
        );
        action(
            "fullscreen",
            Box::new(|b| {
                let window = b.window.upcast_ref::<gtk::Window>();
                window.set_fullscreened(!window.is_fullscreen());
            }),
        );
        // Ctrl+1 … Ctrl+8 go to that tab of the space, Ctrl+9 to the last.
        let goto = gio::SimpleAction::new("goto-tab", Some(glib::VariantTy::INT32));
        let this = Rc::downgrade(self);
        goto.connect_activate(move |_, n| {
            if let (Some(b), Some(n)) = (this.upgrade(), n.and_then(|n| n.get::<i32>())) {
                b.goto_tab(n);
            }
        });
        w.add_action(&goto);
    }

    fn goto_tab(self: &Rc<Self>, n: i32) {
        let target = {
            let model = self.model.borrow();
            let tabs: Vec<TabId> = model
                .in_space(model.current_space())
                .filter(|t| t.essential.is_none())
                .map(|t| t.id)
                .collect();
            if n >= 9 {
                tabs.last().copied()
            } else {
                usize::try_from(n - 1)
                    .ok()
                    .and_then(|i| tabs.get(i).copied())
            }
        };
        if let Some(id) = target {
            let changes = self.model.borrow_mut().select(id);
            self.apply(changes);
        }
    }

    /// The level the page in front shows now, in percent.
    fn zoom_now(view: &WebView) -> u32 {
        (view.zoom_level() * 100.0).round().clamp(1.0, 1000.0) as u32
    }

    fn zoom_by(self: &Rc<Self>, up: bool) {
        let Some(v) = self.current_view() else { return };
        self.zoom_to(zoom::step(Self::zoom_now(&v), up));
    }

    fn zoom_to(self: &Rc<Self>, percent: u32) {
        let Some(v) = self.current_view() else { return };
        v.set_zoom_level(f64::from(percent) / 100.0);
        // Only web pages keep a level of their own, and never in a private window.
        let host = v
            .uri()
            .and_then(|u| brujula_core::adblock::normalize_host(u.as_str()));
        if let Some(host) = host.filter(|_| !self.private) {
            if self.zooms.borrow_mut().set(&host, percent) {
                let _ = paths::write_atomic(&paths::zoom_file(), &self.zooms.borrow().to_json());
            }
        }
        self.toast.flash(
            "ndl-search-symbolic",
            &format!("Zoom {}", zoom::label(percent)),
        );
    }

    /// Puts the level this site had last time on a page that has just started to load.
    pub(super) fn apply_zoom(&self, view: &WebView) {
        let host = view
            .uri()
            .and_then(|u| brujula_core::adblock::normalize_host(u.as_str()));
        let percent = host.map_or(zoom::DEFAULT, |h| self.zooms.borrow().get(&h));
        let level = f64::from(percent) / 100.0;
        if (view.zoom_level() - level).abs() > 0.001 {
            view.set_zoom_level(level);
        }
    }

    fn copy_link(self: &Rc<Self>) {
        let Some(url) = self.current_view().and_then(|v| v.uri()) else {
            return;
        };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return;
        }
        self.window.clipboard().set_text(&url);
        self.toast.flash("ndl-link-symbolic", "Enlace copiado");
    }

    /// The visible part of the page, saved in Pictures and put on the clipboard.
    fn capture_page(self: &Rc<Self>) {
        let Some(view) = self.current_view() else {
            return;
        };
        let this = Rc::downgrade(self);
        view.snapshot(
            SnapshotRegion::Visible,
            SnapshotOptions::NONE,
            None::<&gio::Cancellable>,
            move |result| {
                let Some(b) = this.upgrade() else { return };
                let texture = match result {
                    Ok(t) => t,
                    Err(e) => {
                        dialogs::notice(&b.window, "No se pudo capturar la página", &e.to_string());
                        return;
                    }
                };
                let dir = glib::user_special_dir(glib::UserDirectory::Pictures)
                    .unwrap_or_else(glib::home_dir);
                let stamp = glib::DateTime::now_local()
                    .ok()
                    .and_then(|d| d.format("%Y%m%d-%H%M%S").ok())
                    .map_or_else(|| "captura".to_owned(), |s| s.to_string());
                let file = dir.join(format!("Brujula-{stamp}.png"));
                b.window.clipboard().set_texture(&texture);
                match texture.save_to_png(&file) {
                    Ok(()) => {
                        let shown = gio::File::for_path(&file);
                        let window = b.window.clone();
                        b.toast.show(
                            None,
                            "ndl-camera-symbolic",
                            "Captura guardada en Imágenes y copiada",
                            vec![(
                                "Mostrar".to_owned(),
                                true,
                                Box::new(move || {
                                    gtk::FileLauncher::new(Some(&shown)).open_containing_folder(
                                        Some(&window),
                                        gio::Cancellable::NONE,
                                        |_| {},
                                    );
                                }),
                            )],
                        );
                    }
                    Err(e) => {
                        dialogs::notice(&b.window, "No se pudo guardar la captura", &e.to_string())
                    }
                }
            },
        );
    }

    fn print_page(self: &Rc<Self>) {
        let Some(view) = self.current_view() else {
            return;
        };
        webkit6::PrintOperation::new(&view)
            .run_dialog(Some(self.window.upcast_ref::<gtk::Window>()));
    }

    fn toggle_mute_current(self: &Rc<Self>) {
        if let Some(id) = self.model.borrow().selected() {
            self.toggle_mute(id);
        }
    }

    /// Mutes or unmutes a tab; the speaker in its row says which.
    pub(super) fn toggle_mute(self: &Rc<Self>, id: TabId) {
        let Some(view) = self.views.borrow().get(&id).cloned() else {
            return;
        };
        let muted = !view.is_muted();
        view.set_is_muted(muted);
        self.sync_row(id);
        self.toast.flash(
            if muted {
                "ndl-mute-symbolic"
            } else {
                "ndl-volume-symbolic"
            },
            if muted {
                "Pestaña silenciada"
            } else {
                "Sonido de la pestaña activado"
            },
        );
    }
}
