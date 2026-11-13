//! Updates and the page of news. The shared `nodalia-update` crate looks on GitHub for a newer
//! release from time to time. An AppImage can replace itself (the download is checked against
//! the digest GitHub publishes for it); a copy installed by the system (Nodalix's package) is
//! only told, and the system's own updater does the work.

use brujula_core::news::NEWS_URI;
use nodalia_update::{App, Check, InstallKind, Release};

use super::*;
use crate::dialogs::Choice;

/// What the last check found, and whether a check or a download is going.
#[derive(Default)]
pub(super) struct UpdateState {
    latest: Option<Release>,
    status: String,
    busy: bool,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

const APP: App = App {
    name: "Brújula",
    repo: "getnodalia/brujula",
    bundle: "Brújula.app",
    version: VERSION,
    edition: None,
};

impl Browser {
    /// Looks for a newer version if the person allows it, it is time and this copy is not one a
    /// package manager looks after. Called when the window opens.
    pub(super) fn startup_updates(self: &Rc<Self>) {
        let (on, last) = {
            let s = self.settings.borrow();
            (s.update_check, s.update_checked_at)
        };
        if !self.private
            && on
            && nodalia_update::install_kind(&APP).checks_at_start()
            && brujula_core::settings::update_check_due(last, now_ms())
        {
            self.check_updates(false);
        }
    }

    /// Opens the page of news once after an update. A profile that has never been opened is
    /// shown the welcome instead and starts out as having seen them.
    pub(super) fn show_news_once(self: &Rc<Self>) -> bool {
        if self.private || self.needs_welcome() {
            return false;
        }
        if self.settings.borrow().seen_version == VERSION {
            return false;
        }
        self.settings.borrow_mut().seen_version = VERSION.to_owned();
        self.save_settings();
        self.open_tab(NEWS_URI, None, true);
        true
    }

    pub(super) fn render_news(&self, view: &WebView) {
        let settings = self.settings.borrow();
        let theme = match settings.appearance.theme {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        let html = brujula_core::news::render(theme, &settings.appearance.accent);
        drop(settings);
        view.load_html(&html, Some(NEWS_URI));
    }

    /// The About dialog, as the other Nodalia apps have it.
    pub(super) fn show_about(&self) {
        ndl::AboutDialog::builder()
            .application_icon(paths::APP_ID)
            .application_name("Brújula")
            .developer_name("Nodalia")
            .version(VERSION)
            .comments("El navegador de Nodalix: rápido, seguro y a tu manera.")
            .website("https://getnodalia.com/brujula/")
            .license_type(gtk::License::Gpl30)
            .release_notes(brujula_core::news::notes_html())
            .release_notes_version(VERSION)
            .build()
            .present(Some(&self.window));
    }

    /// What the preferences page says about updates.
    pub(super) fn update_view(&self) -> (String, String) {
        let state = self.update.borrow();
        (
            state.status.clone(),
            state
                .latest
                .as_ref()
                .map(|l| l.version.clone())
                .unwrap_or_default(),
        )
    }

    /// One of the `update.*` buttons of the preferences page.
    pub(super) fn change_update(self: &Rc<Self>, what: &str) {
        match what {
            "check" => self.check_updates(true),
            "install" => self.offer_or_install(),
            _ => {}
        }
    }

    fn set_update_status(&self, text: String) {
        self.update.borrow_mut().status = text;
        self.refresh_about();
    }

    /// Draws the preferences page again if it is open, at its last section.
    fn refresh_about(&self) {
        let views: Vec<WebView> = self.views.borrow().values().cloned().collect();
        for v in views {
            if v.uri().as_deref() == Some(prefs::SETTINGS_URI) {
                *self.pending_section.borrow_mut() = Some("about".to_owned());
                self.render_settings(&v);
            }
        }
    }

    /// Asks GitHub for the latest release. `manual` is a person pressing the button: it always
    /// answers, while the check at start says nothing unless there is something new.
    pub(super) fn check_updates(self: &Rc<Self>, manual: bool) {
        if self.update.borrow().busy {
            return;
        }
        self.update.borrow_mut().busy = true;
        let this = self.clone();
        glib::spawn_future_local(async move {
            let found = gio::spawn_blocking(|| nodalia_update::check(&APP))
                .await
                .map_err(|_| "la comprobación se interrumpió".to_owned())
                .and_then(|r| r.map_err(|e| e.to_string()));
            this.update.borrow_mut().busy = false;
            if !this.private {
                this.settings.borrow_mut().update_checked_at = now_ms();
                this.save_settings();
            }
            match found {
                Ok(Check::Available(release)) => {
                    let version = release.version.clone();
                    this.update.borrow_mut().latest = Some(release);
                    this.set_update_status(format!(
                        "Hay una versión nueva: la {version} (tienes la {VERSION})."
                    ));
                    let told = this.settings.borrow().update_notified == version;
                    if manual || !told {
                        this.settings.borrow_mut().update_notified = version;
                        this.save_settings();
                        this.offer_or_install();
                    }
                }
                Ok(Check::UpToDate) => {
                    this.update.borrow_mut().latest = None;
                    this.set_update_status(format!("Tienes la última versión ({VERSION})."));
                }
                Err(why) => {
                    this.set_update_status("No se pudo comprobar si hay versión nueva.".to_owned());
                    if manual {
                        dialogs::notice(
                            &this.window,
                            "No se pudo comprobar",
                            &format!("No se llegó a GitHub: {why}. Prueba de nuevo más tarde."),
                        );
                    }
                }
            }
        });
    }

    /// Tells the person about the version that is waiting, in the way that suits how Brújula
    /// was installed.
    fn offer_or_install(self: &Rc<Self>) {
        let Some(release) = self.update.borrow().latest.clone() else {
            return;
        };
        let version = release.version.clone();
        let kind = nodalia_update::install_kind(&APP);
        if kind.can_apply_in_place() && release.asset.is_some() {
            let this = self.clone();
            dialogs::ask(
                &self.window,
                &format!("Brújula {version} está disponible"),
                &format!(
                    "Tienes la {VERSION}. Se descarga la nueva, se comprueba y sustituye a este \
                     AppImage; tus pestañas y datos no se tocan."
                ),
                &[
                    Choice::plain("later", "Más tarde"),
                    Choice::suggested("update", "Actualizar"),
                ],
                "update",
                "later",
                move |id| {
                    if id == "update" {
                        this.install_update(release.clone());
                    }
                },
            );
            return;
        }
        let how = match kind {
            InstallKind::Managed => {
                "Esta copia de Brújula la gestiona el sistema: actualízala con su gestor de \
                 paquetes (en Nodalix, desde las actualizaciones del sistema)."
            }
            _ => "Esta copia no se actualiza sola: descarga la nueva de la página de la versión.",
        };
        let this = self.clone();
        let page = release.page.clone();
        dialogs::ask(
            &self.window,
            &format!("Brújula {version} está disponible"),
            &format!("Tienes la {VERSION}. {how}"),
            &[
                Choice::plain("close", "Cerrar"),
                Choice::suggested("open", "Ver la versión"),
            ],
            "open",
            "close",
            move |id| {
                if id == "open" {
                    this.open_tab(&page, None, true);
                }
            },
        );
    }

    /// Downloads the package (checked against the digest of the release) and offers to restart.
    fn install_update(self: &Rc<Self>, release: Release) {
        if self.update.borrow().busy {
            return;
        }
        self.update.borrow_mut().busy = true;
        self.set_update_status(format!("Descargando la {}…", release.version));
        let this = self.clone();
        glib::spawn_future_local(async move {
            let job = release.clone();
            let result =
                gio::spawn_blocking(move || nodalia_update::download(&APP, &job, |_, _| {}))
                    .await
                    .map_err(|_| "la descarga se interrumpió".to_owned())
                    .and_then(|r| r.map_err(|e| e.to_string()));
            this.update.borrow_mut().busy = false;
            match result {
                Ok(package) => {
                    this.set_update_status(format!(
                        "La {} está lista. Reinicia para usarla.",
                        release.version
                    ));
                    this.offer_restart(&release.version, package);
                }
                Err(why) => {
                    this.set_update_status("No se pudo actualizar.".to_owned());
                    dialogs::notice(
                        &this.window,
                        "No se pudo actualizar",
                        &format!(
                            "{why}. Este Brújula sigue como estaba; puedes descargar la nueva de la \
                             página de la versión."
                        ),
                    );
                }
            }
        });
    }

    fn offer_restart(self: &Rc<Self>, version: &str, package: std::path::PathBuf) {
        let this = self.clone();
        dialogs::ask(
            &self.window,
            &format!("Brújula {version} descargada"),
            "Reinicia para empezar a usarla. Tus pestañas se recuperan.",
            &[
                Choice::plain("later", "Más tarde"),
                Choice::suggested("restart", "Reiniciar ahora"),
            ],
            "restart",
            "later",
            move |id| {
                if id == "restart" {
                    this.restart_into(package.clone());
                }
            },
        );
    }

    /// Saves what the window holds, puts the new version in place and starts it.
    fn restart_into(self: &Rc<Self>, package: std::path::PathBuf) {
        self.save_session();
        self.save_notes();
        self.flush_history();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let result =
                gio::spawn_blocking(move || nodalia_update::apply(&APP, &package, || Ok(())))
                    .await
                    .map_err(|_| "la instalación se interrumpió".to_owned())
                    .and_then(|r| r.map_err(|e| e.to_string()));
            match result {
                // The new copy is already starting; this process has nothing left to do.
                Ok(()) => std::process::exit(0),
                Err(why) => dialogs::notice(
                    &this.window,
                    "No se pudo reiniciar",
                    &format!("{why}. Cierra y abre Brújula tú mismo."),
                ),
            }
        });
    }
}
