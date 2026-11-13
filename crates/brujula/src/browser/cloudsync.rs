//! Synchronisation through the cloud accounts of the desktop: which account, when to run, and
//! putting what other devices say into the window. The files and the merge are in
//! `brujula_core::sync`; nothing here talks to a cloud service, it only reads and writes a folder
//! the system has mounted.

use super::*;

use brujula_core::cloud::{self, Account};
use brujula_core::sync::{self, Snapshot};

/// How often a round runs on its own.
const ROUND_SECONDS: u32 = 45;

/// The home directory the accounts are looked up in. `BRUJULA_SYNC_HOME` is for tests only.
fn home() -> std::path::PathBuf {
    std::env::var_os("BRUJULA_SYNC_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(glib::home_dir)
}

fn accounts() -> Vec<Account> {
    std::fs::read_to_string(home().join(cloud::ACCOUNTS_FILE))
        .map(|t| cloud::parse_accounts(&t))
        .unwrap_or_default()
}

/// Whether the account's folder is a live mount. A cloud that is not connected leaves an
/// ordinary empty folder behind, and writing there would fill the local disk instead.
/// `BRUJULA_SYNC_SKIP_MOUNT_CHECK` is for tests only.
fn mounted(dir: &std::path::Path) -> bool {
    if std::env::var_os("BRUJULA_SYNC_SKIP_MOUNT_CHECK").is_some() {
        return dir.is_dir();
    }
    let real = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    std::fs::read_to_string("/proc/self/mountinfo")
        .is_ok_and(|info| cloud::is_mounted(&info, &real))
}

/// The folders of file-sync clients in the home directory that exist, in a fixed order: the page
/// names one by its place in this list.
fn offered_folders() -> Vec<String> {
    let home = home();
    cloud::KNOWN_SYNC_FOLDERS
        .iter()
        .map(|name| home.join(name))
        .filter(|p| p.is_dir())
        .filter_map(|p| p.to_str().map(str::to_owned))
        .filter(|p| cloud::valid_folder(p))
        .collect()
}

impl Browser {
    /// Acts on what the page asked for the folder: the system's dialog, one the browser offered,
    /// or no folder.
    pub(super) fn choose_sync_folder(self: &Rc<Self>, choice: prefs::FolderChoice) {
        match choice {
            prefs::FolderChoice::Forget => {}
            prefs::FolderChoice::Offered(i) => {
                if let Some(path) = offered_folders().into_iter().nth(i) {
                    self.use_sync_folder(&path);
                }
            }
            prefs::FolderChoice::Dialog => {
                let dialog = gtk::FileDialog::builder()
                    .title("Carpeta para sincronizar")
                    .modal(true)
                    .build();
                dialog.set_initial_folder(Some(&gio::File::for_path(home())));
                let this = self.clone();
                dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |result| {
                    if let Some(path) = result.ok().and_then(|f| f.path()) {
                        if let Some(path) = path.to_str() {
                            this.use_sync_folder(path);
                        }
                    }
                });
            }
        }
    }

    /// Syncs through `path` from now on, in place of any account.
    fn use_sync_folder(self: &Rc<Self>, path: &str) {
        if !cloud::valid_folder(path) || !std::path::Path::new(path).is_dir() {
            return;
        }
        {
            let mut settings = self.settings.borrow_mut();
            settings.sync.folder = path.to_owned();
            settings.sync.account.clear();
            settings.sync.enabled = true;
        }
        self.save_settings();
        self.sync_busy.set(false);
        self.sync_round();
        self.refresh_settings_pages();
    }

    /// Draws the preferences page again if it is open, so it shows what changed under it.
    fn refresh_settings_pages(&self) {
        let views: Vec<WebView> = self.views.borrow().values().cloned().collect();
        for v in views {
            if v.uri().as_deref() == Some(prefs::SETTINGS_URI) {
                // The page starts at the top again: put it back on the section being edited.
                *self.pending_section.borrow_mut() = Some("sync".to_owned());
                self.render_settings(&v);
            }
        }
    }

    /// What the preferences page shows about the accounts and the last round.
    pub(super) fn extras(&self) -> prefs::Extras {
        let home = home();
        let (history, history_total) = self.history_view();
        let (update_status, update_version) = self.update_view();
        prefs::Extras {
            ai: self.ai_view(),
            update_status,
            update_version,
            history,
            history_total,
            vault: self.vault_view(),
            import: self.import_view(),
            profiles: self.profile_rows(),
            profile: paths::profile().to_owned(),
            essentials: self
                .essentials
                .borrow()
                .items()
                .iter()
                .map(|e| (e.id, e.name.clone(), e.url.clone()))
                .collect(),
            webapps: crate::webapp::registry()
                .items()
                .iter()
                .map(|a| (a.id.clone(), a.name.clone(), a.url.clone()))
                .collect(),
            folders: offered_folders(),
            accounts: accounts()
                .iter()
                .map(|a| (a.id.clone(), a.label(), mounted(&a.mount_dir(&home))))
                .collect(),
            status: self.sync_status.borrow().clone(),
            extensions: self.ext_view_list(),
            boosts: self
                .boosts
                .borrow()
                .items()
                .iter()
                .map(|b| (b.host.clone(), b.dark, b.css.clone()))
                .collect(),
            archived: self
                .archive
                .borrow()
                .items()
                .iter()
                .map(|e| (e.url.clone(), e.title.clone()))
                .collect(),
        }
    }

    /// Makes this device's id and name if it has none yet. Returns whether anything changed.
    fn ensure_device(&self) -> bool {
        let name: String = glib::host_name().chars().take(40).collect();
        let mut settings = self.settings.borrow_mut();
        let sync = &mut settings.sync;
        let mut changed = false;
        if !sync::valid_device_id(&sync.device_id) {
            sync.device_id = format!(
                "{:08x}{:08x}{:08x}",
                glib::random_int(),
                glib::random_int(),
                glib::random_int()
            );
            changed = true;
        }
        if sync.device_name != name {
            sync.device_name = name;
            changed = true;
        }
        changed
    }

    /// Runs a round now and then every few seconds while the window lives.
    pub(super) fn start_sync(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        glib::timeout_add_seconds_local(ROUND_SECONDS, move || match this.upgrade() {
            Some(b) => {
                b.sync_round();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        self.sync_round();
    }

    fn set_sync_status(&self, text: &str) {
        *self.sync_status.borrow_mut() = text.to_owned();
        let view = self
            .views
            .borrow()
            .values()
            .find(|v| v.uri().as_deref() == Some(prefs::SETTINGS_URI))
            .cloned();
        if let Some(view) = view {
            self.push_sync_status(&view);
        }
    }

    /// Writes the status into the preferences page, if that is what `view` shows. A round can
    /// end before the page has loaded, so this also runs when a page finishes loading.
    pub(super) fn push_sync_status(&self, view: &WebView) {
        if view.uri().as_deref() != Some(prefs::SETTINGS_URI) {
            return;
        }
        let quoted = serde_json::to_string(&*self.sync_status.borrow())
            .unwrap_or_else(|_| "\"\"".to_owned());
        let script = format!(
            "{{const e=document.getElementById('sync-status');if(e)e.textContent={quoted}}}"
        );
        view.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }

    /// Draws the new-tab pages again, so they show what other devices have open.
    pub(super) fn refresh_newtabs(&self) {
        let views: Vec<WebView> = self.views.borrow().values().cloned().collect();
        for v in views {
            if v.uri().as_deref() == Some(newtab::NEWTAB_URI) {
                self.render_newtab(&v, false);
            }
        }
    }

    /// One round: write this device's file if it changed, read the others and merge them.
    pub(super) fn sync_round(self: &Rc<Self>) {
        if self.sync_busy.get() || self.restoring.get() {
            return;
        }
        if !self.settings.borrow().sync.enabled {
            self.set_sync_status("");
            if !self.sync_others.borrow().is_empty() {
                self.sync_others.borrow_mut().clear();
                self.refresh_newtabs();
            }
            return;
        }
        if self.ensure_device() {
            self.save_settings();
        }
        let cfg = self.settings.borrow().sync.clone();
        if cfg.account.is_empty() && cfg.folder.is_empty() {
            self.set_sync_status("Elige una cuenta de la nube o una carpeta.");
            return;
        }
        if !(cfg.bookmarks || cfg.spaces || cfg.tabs) {
            self.set_sync_status("Elige qué sincronizar.");
            return;
        }
        let dir = if cfg.folder.is_empty() {
            let Some(account) = accounts().into_iter().find(|a| a.id == cfg.account) else {
                self.set_sync_status("Esa cuenta ya no está en Ajustes.");
                return;
            };
            let dir = account.mount_dir(&home());
            if !mounted(&dir) {
                self.set_sync_status(&format!(
                    "«{}» no está conectada. Conéctala en Ajustes.",
                    account.label()
                ));
                return;
            }
            dir
        } else {
            // A folder the user picked: the client that keeps it in sync is theirs, so there is
            // no mount to check, only that the folder is still there.
            let dir = std::path::PathBuf::from(&cfg.folder);
            if !cloud::valid_folder(&cfg.folder) || !dir.is_dir() {
                self.set_sync_status("La carpeta elegida ya no existe.");
                return;
            }
            dir
        };
        let snap = Snapshot::capture(
            &cfg.device_id,
            &cfg.device_name,
            brujula_core::bookmarks::now_ms(),
            cfg.share(),
            &self.bookmarks.borrow(),
            &self.spaces.borrow(),
            &self.folders.borrow(),
            &self.model.borrow(),
        );
        // The time it was written does not count: nothing is uploaded when nothing changed.
        let key = Snapshot {
            updated: 0,
            ..snap.clone()
        }
        .to_json();
        let own = (*self.sync_sent.borrow() != key).then_some(snap);
        let written = own.is_some().then_some(key);
        self.sync_busy.set(true);
        self.set_sync_status("Sincronizando…");
        let this = self.clone();
        glib::spawn_future_local(async move {
            let id = cfg.device_id.clone();
            let result = gio::spawn_blocking(move || sync::exchange(&dir, &id, own.as_ref())).await;
            this.sync_busy.set(false);
            match result {
                Ok(Ok(others)) => this.sync_arrived(others, written),
                Ok(Err(why)) => this.set_sync_status(&format!("No se pudo sincronizar: {why}")),
                Err(_) => this.set_sync_status("La sincronización falló."),
            }
        });
    }

    /// Puts what the other devices wrote into the window.
    fn sync_arrived(self: &Rc<Self>, others: Vec<Snapshot>, written: Option<String>) {
        if let Some(key) = written {
            *self.sync_sent.borrow_mut() = key;
        }
        let cfg = self.settings.borrow().sync.clone();
        if !cfg.active() {
            // Switched off while the round ran.
            return;
        }
        let report = sync::merge(
            &others,
            &cfg.device_id,
            cfg.share(),
            &mut self.bookmarks.borrow_mut(),
            &mut self.spaces.borrow_mut(),
            &mut self.folders.borrow_mut(),
        );
        let marks_changed = report.bookmarks_added + report.bookmarks_removed > 0;
        if marks_changed {
            self.save_bookmarks();
            self.refresh_bookmark_action();
        }
        if report.spaces_added + report.folders_added > 0 {
            self.rebuild_spaces();
            self.rebuild_folder_rows();
            self.list.invalidate_filter();
        }
        let tabs_changed = cfg.tabs
            && self
                .sync_others
                .borrow()
                .iter()
                .map(|d| &d.tabs)
                .ne(others.iter().map(|d| &d.tabs));
        *self.sync_others.borrow_mut() = others.clone();
        if tabs_changed || marks_changed {
            self.refresh_newtabs();
        }
        let time = glib::DateTime::now_local()
            .ok()
            .and_then(|d| d.format("%H:%M").ok())
            .map(|t| t.to_string())
            .unwrap_or_default();
        let devices = match others.len() {
            0 => "sin otros dispositivos todavía".to_owned(),
            1 => "con 1 dispositivo más".to_owned(),
            n => format!("con {n} dispositivos más"),
        };
        self.set_sync_status(&format!("Sincronizado a las {time}, {devices}."));
    }

    /// The tabs other devices have open, for the new-tab page.
    pub(super) fn devices_for_newtab(
        &self,
        link: &dyn Fn(&str, String, &str, &'static str) -> newtab::Link,
    ) -> Vec<newtab::DeviceTabs> {
        let cfg = self.settings.borrow();
        if !(cfg.sync.enabled && cfg.sync.tabs) {
            return Vec::new();
        }
        self.sync_others
            .borrow()
            .iter()
            .map(|d| newtab::DeviceTabs {
                name: d.device_name.clone(),
                links: d
                    .tabs
                    .iter()
                    .map(|t| link(&t.title, t.url.clone(), &t.url, ""))
                    .collect(),
            })
            .collect()
    }
}
