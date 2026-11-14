//! Archiving the tabs that were left alone, and bringing them back.

use super::*;

use brujula_core::archive::{self, Entry};

/// How often tabs are looked over.
const SWEEP_SECONDS: u32 = 300;

impl Browser {
    pub(super) fn save_archive(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::archive_file(), &self.archive.borrow().to_json());
    }

    /// Looks over the tabs shortly after start and then every few minutes.
    pub(super) fn start_archiving(self: &Rc<Self>) {
        let first = Rc::downgrade(self);
        glib::timeout_add_seconds_local_once(20, move || {
            if let Some(b) = first.upgrade() {
                b.archive_sweep();
            }
        });
        let this = Rc::downgrade(self);
        glib::timeout_add_seconds_local(SWEEP_SECONDS, move || match this.upgrade() {
            Some(b) => {
                b.archive_sweep();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }

    /// Closes the tabs that went unused for longer than the setting says, keeping them in the
    /// archive.
    fn archive_sweep(self: &Rc<Self>) {
        let now = now_ms();
        let after = self.settings.borrow().archive_after_hours;
        let selected = self.model.borrow().selected();
        if let Some(sel) = selected {
            // The tab in front is in use right now, however long ago it was selected.
            self.model.borrow_mut().touch(sel, now);
        }
        let stale = archive::stale(&self.model.borrow(), &self.folders.borrow(), now, after);
        if stale.is_empty() {
            return;
        }
        for id in stale {
            let entry = self.model.borrow().get(id).map(|t| Entry {
                url: t.url.clone(),
                title: t.title.clone(),
                space: t.space,
                at: now,
            });
            if let Some(entry) = entry {
                self.archive.borrow_mut().push(entry);
                self.close_tab(id);
            }
        }
        self.save_archive();
        self.refresh_newtabs();
    }

    /// A button of the archive section in the preferences: `open` brings the page back in the
    /// space in front, `remove` forgets it.
    pub(super) fn change_archive(self: &Rc<Self>, what: &str, url: &str) {
        let entry = self.archive.borrow_mut().take(url);
        let Some(entry) = entry else { return };
        self.save_archive();
        if what == "open" {
            self.open_tab(&entry.url, None, true);
        }
    }

    /// Arc's "Clear": the tabs of this space that are not pinned, in a folder or in front go to
    /// the archive at once, and a note offers to bring them back.
    pub(super) fn clear_tabs(self: &Rc<Self>) {
        let now = now_ms();
        let selected = self.model.borrow().selected();
        let cleared: Vec<(TabId, Entry)> = {
            let m = self.model.borrow();
            let folders = self.folders.borrow();
            m.in_space(m.current_space())
                .filter(|t| {
                    Some(t.id) != selected
                        && !t.pinned
                        && t.essential.is_none()
                        && folders
                            .folder_of(t.folder, t.space, &t.url, &t.title)
                            .is_none()
                })
                .map(|t| {
                    (
                        t.id,
                        Entry {
                            url: t.url.clone(),
                            title: t.title.clone(),
                            space: t.space,
                            at: now,
                        },
                    )
                })
                .collect()
        };
        if cleared.is_empty() {
            self.toast
                .flash("ndl-check-symbolic", "No hay pestañas que limpiar");
            return;
        }
        let mut urls = Vec::new();
        for (id, entry) in cleared {
            // Only web pages are worth keeping; the rest just close.
            if entry.url.starts_with("http://") || entry.url.starts_with("https://") {
                urls.push(entry.url.clone());
                self.archive.borrow_mut().push(entry);
            }
            self.close_tab(id);
        }
        self.save_archive();
        self.refresh_newtabs();
        let n = urls.len();
        let text = if n == 1 {
            "1 pestaña archivada".to_owned()
        } else {
            format!("{n} pestañas archivadas")
        };
        let this = Rc::downgrade(self);
        self.toast.show(
            None,
            "ndl-archive-symbolic",
            &text,
            vec![(
                "Deshacer".to_owned(),
                true,
                Box::new(move || {
                    let Some(b) = this.upgrade() else { return };
                    for url in &urls {
                        if let Some(entry) = b.archive.borrow_mut().take(url) {
                            b.open_tab(&entry.url, None, false);
                        }
                    }
                    b.save_archive();
                    b.refresh_newtabs();
                }),
            )],
        );
    }
}
