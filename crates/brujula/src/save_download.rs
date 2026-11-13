//! Where a download goes, and what happens to it once it has arrived. The page picks the file
//! name, so it is cleaned. A program or a script is only saved when the person says so, and
//! without permission to run. Every file is written under a hidden name first, checked when it
//! has arrived (see `brujula_core::scan`; the check does not slow the download down) and only
//! then given its name.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use brujula_core::downloads;
use brujula_core::scan::{self, Finding};
use nodalia_gtk::gtk::prelude::*;
use nodalia_gtk::gtk::{self, gio, glib};
use webkit6::Download;

use crate::dialogs::{self, Choice};

/// What happens to a download, as it happens.
pub enum Decision {
    /// It is being saved; this is the path it will have.
    Saving(PathBuf),
    /// The person did not want this program or script; the name it had.
    Refused(String),
    /// It has arrived and is being looked at.
    Scanning,
    /// It is in place, at the path it was said to have.
    Finished,
    /// It was deleted: why.
    Removed(String),
}

/// Takes away the permission to run, so the file is not started by a double click.
fn strip_run_permission(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode() & !0o111;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
}

/// How long ClamAV may take before the file is let through without its opinion.
const CLAM_SECONDS: u64 = 90;

/// The first program of ClamAV that is installed.
fn clam_programs() -> Vec<&'static str> {
    ["clamdscan", "clamscan"]
        .into_iter()
        .filter(|p| {
            std::env::var_os("PATH")
                .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(p).is_file()))
        })
        .collect()
}

/// What ClamAV says about `path`, if it is installed and answered in time.
fn clam_scan(path: &Path) -> Option<Finding> {
    for program in clam_programs() {
        let mut command = std::process::Command::new(program);
        if program == "clamdscan" {
            command.arg("--fdpass");
        }
        let child = command
            .arg("--no-summary")
            .arg("--")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let Ok(mut child) = child else { continue };
        let start = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if start.elapsed().as_secs() < CLAM_SECONDS => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let status = status?;
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            use std::io::Read;
            let _ = stdout.read_to_string(&mut out);
        }
        // 0 clean, 1 found something, 2 could not scan (a daemon that is not running, say): try
        // the next program.
        match status.code() {
            Some(1) => return scan::parse_clam(&out).map(Finding::Malware),
            Some(0) => return None,
            _ => {}
        }
    }
    None
}

/// Looks at a file that has arrived. Blocking.
fn check(path: &Path, name: &str) -> Option<Finding> {
    use std::io::Read;
    let mut header = vec![0u8; scan::HEADER_LEN];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut header))
        .unwrap_or(0);
    header.truncate(n);
    scan::disguised(name, &header).or_else(|| clam_scan(path))
}

/// Gives the file its name: where it was said to go, or the next free one.
fn finish(quarantine: &Path, wanted: &Path, strip: bool) -> Result<(), String> {
    let target = if wanted.exists() {
        let folder = wanted.parent().unwrap_or(Path::new("."));
        let name = wanted
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        downloads::available_path(folder, &name)
    } else {
        wanted.to_owned()
    };
    std::fs::rename(quarantine, &target).map_err(|e| e.to_string())?;
    if strip {
        strip_run_permission(&target);
    }
    Ok(())
}

fn save(
    parent: &gtk::Window,
    download: &Download,
    name: &str,
    report: Rc<dyn Fn(Decision)>,
    strip: bool,
) {
    let folder =
        glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir);
    let path = downloads::available_path(&folder, name);
    let quarantine = downloads::available_path(&folder, &format!(".{name}.brujula-part"));
    download.set_destination(&quarantine.to_string_lossy());
    report(Decision::Saving(path.clone()));

    let part = quarantine.clone();
    download.connect_failed(move |_, _| {
        let _ = std::fs::remove_file(&part);
    });
    let (parent, name) = (parent.clone(), name.to_owned());
    download.connect_finished(move |_| {
        report(Decision::Scanning);
        let (parent, name, path, quarantine, report) = (
            parent.clone(),
            name.clone(),
            path.clone(),
            quarantine.clone(),
            report.clone(),
        );
        glib::spawn_future_local(async move {
            let (part, label) = (quarantine.clone(), name.clone());
            let found = gio::spawn_blocking(move || check(&part, &label))
                .await
                .ok()
                .flatten();
            let Some(found) = found else {
                conclude(quarantine, path, strip, report);
                return;
            };
            let why = found.reason();
            dialogs::ask(
                &parent,
                "Brújula ha visto algo raro en una descarga",
                &format!("«{name}». {why}\n\n¿La borras?"),
                &[
                    Choice::plain("keep", "Conservarla"),
                    Choice::destructive("delete", "Borrarla"),
                ],
                "delete",
                "delete",
                move |id| {
                    if id == "delete" {
                        remove(quarantine.clone(), why.clone(), report.clone());
                    } else {
                        // Kept on purpose: still without permission to run.
                        conclude(quarantine.clone(), path.clone(), true, report.clone());
                    }
                },
            );
        });
    });
}

/// Puts the file in place and says so. The file work happens off the main thread.
fn conclude(quarantine: PathBuf, path: PathBuf, strip: bool, report: Rc<dyn Fn(Decision)>) {
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || finish(&quarantine, &path, strip))
            .await
            .map_err(|_| "se interrumpió".to_owned())
            .and_then(|r| r);
        match result {
            Ok(()) => report(Decision::Finished),
            Err(why) => report(Decision::Removed(format!("no se pudo guardar ({why})"))),
        }
    });
}

/// Deletes a file that was found suspect, and says why.
fn remove(quarantine: PathBuf, why: String, report: Rc<dyn Fn(Decision)>) {
    glib::spawn_future_local(async move {
        let _ = gio::spawn_blocking(move || std::fs::remove_file(&quarantine)).await;
        report(Decision::Removed(why));
    });
}

/// Decides where `download` is saved. Ordinary files go to the Downloads folder at once; a file
/// that could run asks first and waits for the answer. `report` hears what happens to it.
pub fn decide(
    parent: &impl IsA<gtk::Window>,
    download: &Download,
    suggested: &str,
    report: impl Fn(Decision) + 'static,
) {
    let parent = parent.upcast_ref::<gtk::Window>();
    let name = downloads::safe_file_name(suggested);
    let report: Rc<dyn Fn(Decision)> = Rc::new(report);
    if !downloads::is_dangerous(&name) {
        save(parent, download, &name, report, false);
        return;
    }
    let download = download.clone();
    let shown = name.clone();
    let window = parent.clone();
    dialogs::ask_program_download(parent, &shown, move |yes| {
        if yes {
            save(&window, &download, &name, report.clone(), true);
        } else {
            download.cancel();
            report(Decision::Refused(name.clone()));
        }
    });
}
