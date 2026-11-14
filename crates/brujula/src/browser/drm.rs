//! Protected video and music (DRM). WebKitGTK cannot play it (see `brujula_core::drm`), so a
//! page that needs it gets a notice and can be opened in a browser that carries Widevine.

use super::*;

use brujula_core::drm;

/// The full path of `program` in `PATH` or in the Flatpak launchers, if it is an executable file.
fn locate(program: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.push("/var/lib/flatpak/exports/bin".into());
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::Path::new(&home).join(".local/share/flatpak/exports/bin"));
    }
    // A copy of Brújula inside an AppImage must not find its own bundled tools.
    let appdir = std::env::var("APPDIR").ok();
    dirs.iter()
        .filter(|d| {
            appdir
                .as_deref()
                .is_none_or(|a| !a.is_empty() && !d.starts_with(a))
        })
        .map(|d| d.join(program))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .map(|p| p.to_string_lossy().into_owned())
}

/// What the AppImage added to the environment, taken out: the browser that is started is not
/// part of Brújula and must not load its libraries or its WebKit.
fn clean_environment(command: &mut std::process::Command) {
    let Ok(appdir) = std::env::var("APPDIR") else {
        return;
    };
    for var in [
        "APPDIR",
        "APPIMAGE",
        "ARGV0",
        "OWD",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "GIO_EXTRA_MODULES",
        "WEBKIT_EXEC_PATH",
        "WEBKIT_INJECTED_BUNDLE_PATH",
        "GTK_PATH",
        "GTK_EXE_PREFIX",
        "GTK_DATA_PREFIX",
        "GDK_PIXBUF_MODULE_FILE",
        "GSETTINGS_SCHEMA_DIR",
    ] {
        command.env_remove(var);
    }
    for var in ["XDG_DATA_DIRS", "PATH"] {
        if let Some(value) = std::env::var_os(var) {
            let kept: Vec<_> = std::env::split_paths(&value)
                .filter(|p| !p.starts_with(&appdir))
                .collect();
            if let Ok(joined) = std::env::join_paths(kept) {
                command.env(var, joined);
            }
        }
    }
}

impl Browser {
    /// The browsers on this computer that can play protected content.
    pub(super) fn drm_browsers(&self) -> Vec<drm::Found> {
        drm::installed(locate)
    }

    /// For the preferences page: the installed browsers and what a protected page will do.
    pub(super) fn drm_view(&self) -> (Vec<(String, String)>, String) {
        let found = self.drm_browsers();
        let summary = drm::summary(&found, &self.settings.borrow().drm_browser);
        let names = found
            .iter()
            .map(|f| (f.id.to_owned(), f.name.to_owned()))
            .collect();
        (names, summary)
    }

    /// Starts the browser for `url`. The page is no longer private there: a private window says so.
    fn drm_launch(&self, url: &str) -> Result<String, String> {
        let found = self.drm_browsers();
        let preferred = self.settings.borrow().drm_browser.clone();
        let browser = drm::choose(&found, &preferred)
            .ok_or("No hay ningún navegador con Widevine instalado.")?;
        let (program, args) = drm::command(browser, url)?;
        let mut command = std::process::Command::new(program);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        clean_environment(&mut command);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        // Reaped when it ends; the browser may stay open for as long as the film lasts.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(browser.name.to_owned())
    }

    /// The page in front, handed to a browser with Widevine; it also marks the site, so the
    /// notice comes next time.
    pub(super) fn drm_open_current(self: &Rc<Self>) {
        let Some(url) = self.current_view().and_then(|v| v.uri()) else {
            return;
        };
        let url = url.to_string();
        if self.drm_open(&url, true) {
            if let Some(host) = brujula_core::adblock::normalize_host(&url) {
                let known = drm::needs_drm(&url, &self.settings.borrow().drm_sites).is_some();
                if !known && drm::remember(&mut self.settings.borrow_mut().drm_sites, &host) {
                    self.save_settings();
                }
            }
        }
    }

    /// Opens `url` in the browser with Widevine; false (after saying why) if it could not.
    fn drm_open(self: &Rc<Self>, url: &str, say_failure: bool) -> bool {
        match self.drm_launch(url) {
            Ok(_) => true,
            Err(why) => {
                if say_failure {
                    dialogs::notice(&self.window, "No se pudo abrir", &why);
                }
                false
            }
        }
    }

    /// Called when a page has started to arrive: tells about DRM, once per site per session.
    pub(super) fn drm_check(self: &Rc<Self>, view: &WebView) {
        let Some(id) = self.id_of(view) else { return };
        if self.model.borrow().selected() != Some(id) || shows_own_page(view) {
            return;
        }
        let Some(url) = view.uri().map(|u| u.to_string()) else {
            return;
        };
        let (notice, host) = {
            let s = self.settings.borrow();
            let host = drm::needs_drm(&url, &s.drm_sites);
            (
                s.drm_notice,
                host.filter(|h| !s.drm_silenced.iter().any(|x| x == h)),
            )
        };
        let Some(host) = host.filter(|_| notice) else {
            return;
        };
        if !self.drm_told.borrow_mut().insert(host.clone()) {
            return;
        }
        let found = self.drm_browsers();
        let preferred = self.settings.borrow().drm_browser.clone();
        let chosen = drm::choose(&found, &preferred);
        let text = drm::notice(&host, chosen);
        let mut actions: Vec<toast::ToastAction> = Vec::new();
        if let Some(c) = chosen {
            let this = Rc::downgrade(self);
            let page = url.clone();
            let label = if self.private {
                format!("Abrir en {} (no será privado)", c.name)
            } else {
                format!("Abrir en {}", c.name)
            };
            actions.push((
                label,
                true,
                Box::new(move || {
                    if let Some(b) = this.upgrade() {
                        b.drm_open(&page, true);
                    }
                }),
            ));
        }
        let this = Rc::downgrade(self);
        let silenced = host.clone();
        actions.push((
            "No avisar de este sitio".to_owned(),
            false,
            Box::new(move || {
                if let Some(b) = this.upgrade() {
                    if drm::remember(&mut b.settings.borrow_mut().drm_silenced, &silenced) {
                        b.save_settings();
                    }
                }
            }),
        ));
        self.toast
            .show(Some(id), "ndl-shield-check-symbolic", &text, actions);
    }
}
