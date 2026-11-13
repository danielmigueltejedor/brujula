//! Where a download goes. The page picks the file name, so it is cleaned, and a program or a
//! script is only saved when the person says so, and without permission to run.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use brujula_core::downloads;
use nodalia_gtk::gtk::prelude::*;
use nodalia_gtk::gtk::{self, glib};
use webkit6::Download;

use crate::dialogs;

/// What was decided for a download.
pub enum Decision {
    /// It is saved at this path.
    Saving(PathBuf),
    /// The person did not want this program or script; the name it had.
    Refused(String),
}

/// Takes away the permission to run, so the file is not started by a double click.
fn strip_run_permission(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode() & !0o111;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
}

fn save(download: &Download, name: &str, report: &dyn Fn(Decision)) {
    let folder =
        glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir);
    let path = downloads::available_path(&folder, name);
    download.set_destination(&path.to_string_lossy());
    if downloads::is_dangerous(name) {
        let saved = path.clone();
        download.connect_finished(move |_| strip_run_permission(&saved));
    }
    report(Decision::Saving(path));
}

/// Decides where `download` is saved. Ordinary files go to the Downloads folder at once; a file
/// that could run asks first and waits for the answer. `report` hears what was decided.
pub fn decide(
    parent: &impl IsA<gtk::Widget>,
    download: &Download,
    suggested: &str,
    report: impl Fn(Decision) + 'static,
) {
    let name = downloads::safe_file_name(suggested);
    if !downloads::is_dangerous(&name) {
        save(download, &name, &report);
        return;
    }
    let report = Rc::new(report);
    let download = download.clone();
    let shown = name.clone();
    dialogs::ask_program_download(parent, &shown, move |yes| {
        if yes {
            save(&download, &name, &*report);
        } else {
            download.cancel();
            report(Decision::Refused(name.clone()));
        }
    });
}
