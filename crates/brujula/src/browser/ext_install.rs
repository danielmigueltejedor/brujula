//! Putting extensions in and taking them out: from a folder, a `.crx` or `.zip` file, or the
//! Chrome Web Store. Nothing is installed before the user has read what it will be able to do.

use super::ext::{new_installed, pkg_dir};
use super::*;

use std::path::{Path, PathBuf};

use brujula_core::ext::{self, package};

const MAX_DOWNLOAD: usize = 100 * 1024 * 1024;
const MAX_COPY_FILES: usize = 6000;
const MAX_COPY_BYTES: u64 = 150 * 1024 * 1024;

enum Source {
    Folder(PathBuf),
    /// A package (`.crx` or zip) and where it came from: `crx` or `store`.
    Package(Vec<u8>, &'static str, String),
}

/// An extension unpacked and read, waiting for the user's answer.
struct Staged {
    dir: PathBuf,
    id: String,
    name: String,
    version: String,
    summary: Vec<String>,
    source: &'static str,
}

/// Throws away an unpacked extension that was not installed.
fn discard(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn copy_tree(from: &Path, to: &Path, files: &mut usize, bytes: &mut u64) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        // Links could lead out of the folder; version control and build folders are not part
        // of an extension.
        if kind.is_symlink() || name_text == ".git" || name_text == "node_modules" {
            continue;
        }
        let target = to.join(&name);
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, files, bytes)?;
        } else if kind.is_file() {
            *files += 1;
            *bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            if *files > MAX_COPY_FILES || *bytes > MAX_COPY_BYTES {
                return Err("La carpeta es demasiado grande para ser una extensión".to_owned());
            }
            std::fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Unpacks `source` into `staging` and reads its manifest.
fn prepare(source: Source, staging: &Path, languages: &[String]) -> Result<Staged, String> {
    let _ = std::fs::remove_dir_all(staging);
    let (known_id, kind, label) = match &source {
        Source::Folder(path) => {
            let (mut files, mut bytes) = (0, 0);
            copy_tree(path, staging, &mut files, &mut bytes)?;
            (None, "folder", path.to_string_lossy().into_owned())
        }
        Source::Package(data, kind, label) => {
            let (zip, id) = package::crx_zip(data)?;
            package::extract(zip, staging)?;
            (id, *kind, label.clone())
        }
    };
    let (manifest, _) = ext::load_dir(staging, languages)?;
    if !matches!(manifest.manifest_version, 2 | 3) {
        return Err("Solo se admiten extensiones de Manifest V2 y V3".to_owned());
    }
    let id = known_id
        .or_else(|| manifest.key.as_deref().and_then(package::id_from_key))
        .unwrap_or_else(|| package::id_from_source(&label));
    Ok(Staged {
        dir: staging.to_owned(),
        id,
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        summary: ext::permission_summary(&manifest),
        source: kind,
    })
}

impl Browser {
    /// One of the `ext.*` changes of the preferences page.
    pub(super) fn change_extension(self: &Rc<Self>, what: &str, value: &str) {
        match what {
            "install_folder" => self.pick_extension(true),
            "install_file" => self.pick_extension(false),
            "install_store" => self.install_from_store(value),
            "remove" if package::valid_id(value) => self.remove_extension(value),
            "options" if package::valid_id(value) => {
                let page = self
                    .ext
                    .borrow()
                    .loaded
                    .get(value)
                    .and_then(|l| l.manifest.options_page.clone());
                if let Some(p) = page {
                    self.open_ext_page(value, &p);
                }
            }
            w => {
                if let Some(id) = w.strip_prefix("enable.").filter(|i| package::valid_id(i)) {
                    self.enable_extension(id, value == "1");
                }
            }
        }
    }

    fn alert(&self, message: &str, detail: &str) {
        gtk::AlertDialog::builder()
            .message(message)
            .detail(detail)
            .buttons(["Entendido"])
            .build()
            .show(Some(&self.window));
    }

    fn show_extensions_page(self: &Rc<Self>) {
        *self.pending_section.borrow_mut() = Some("extensions".to_owned());
        self.redraw_settings();
    }

    fn pick_extension(self: &Rc<Self>, folder: bool) {
        let dialog = gtk::FileDialog::builder()
            .title(if folder {
                "Elige la carpeta de la extensión"
            } else {
                "Elige un archivo .crx o .zip"
            })
            .build();
        let this = self.clone();
        let done = move |res: Result<gio::File, glib::Error>| {
            let Some(path) = res.ok().and_then(|f| f.path()) else {
                return;
            };
            if folder {
                this.stage(Source::Folder(path));
            } else {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match std::fs::metadata(&path) {
                    Ok(m) if usize::try_from(m.len()).is_ok_and(|n| n <= MAX_DOWNLOAD) => {}
                    _ => return this.alert("No se pudo leer el archivo", "Es demasiado grande."),
                }
                let this2 = this.clone();
                glib::spawn_future_local(async move {
                    let p = path.clone();
                    let data = gio::spawn_blocking(move || std::fs::read(p)).await;
                    match data {
                        Ok(Ok(bytes)) => this2.stage(Source::Package(
                            bytes,
                            "crx",
                            path.to_string_lossy().into_owned(),
                        )),
                        _ => this2.alert("No se pudo leer el archivo", &name),
                    }
                });
            }
        };
        if folder {
            dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, done);
        } else {
            dialog.open(Some(&self.window), gio::Cancellable::NONE, done);
        }
    }

    /// Downloads an extension of the Chrome Web Store, given its address or id.
    fn install_from_store(self: &Rc<Self>, text: &str) {
        let Some(id) = ext::store_id(text) else {
            return self.alert(
                "No es una extensión de la Chrome Web Store",
                "Pega la dirección de su página en la tienda, o su identificador de 32 letras.",
            );
        };
        let url = format!(
            "https://clients2.google.com/service/update2/crx?response=redirect&os=linux&arch=x64&os_arch=x86_64&nacl_arch=x86-64&prod=chromium&prodchannel=unknown&prodversion=130.0.0.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
        );
        let this = self.clone();
        glib::spawn_future_local(async move {
            let session = soup::Session::new();
            session.set_timeout(60);
            let Ok(message) = soup::Message::new("GET", &url) else {
                return;
            };
            match session
                .send_and_read_future(&message, glib::Priority::DEFAULT)
                .await
            {
                Ok(bytes)
                    if message.status() == soup::Status::Ok
                        && bytes.len() > 100
                        && bytes.len() <= MAX_DOWNLOAD =>
                {
                    this.stage(Source::Package(bytes.to_vec(), "store", id));
                }
                Ok(_) => this.alert(
                    "No se pudo descargar la extensión",
                    "La tienda no la ofrece para este navegador, o ya no existe.",
                ),
                Err(e) => this.alert("No se pudo descargar la extensión", e.message()),
            }
        });
    }

    /// Unpacks a source off the main loop, then asks.
    fn stage(self: &Rc<Self>, source: Source) {
        let staging = paths::extensions_dir().join(format!(".staging-{}", now_ms()));
        let languages: Vec<String> = glib::language_names()
            .iter()
            .map(|l| l.to_string())
            .collect();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let s2 = staging.clone();
            let result = gio::spawn_blocking(move || prepare(source, &s2, &languages)).await;
            match result {
                Ok(Ok(staged)) => this.confirm(staged),
                Ok(Err(why)) => {
                    discard(&staging);
                    this.alert("No se pudo instalar la extensión", &why);
                }
                Err(_) => {
                    discard(&staging);
                    this.alert("No se pudo instalar la extensión", "Falló al leerla.");
                }
            }
        });
    }

    fn confirm(self: &Rc<Self>, staged: Staged) {
        let can = if staged.summary.is_empty() {
            "No pide ningún permiso especial.".to_owned()
        } else {
            let lines: Vec<String> = staged.summary.iter().map(|l| format!("• {l}")).collect();
            format!("Podrá:\n{}", lines.join("\n"))
        };
        let update = self.ext.borrow().registry.get(&staged.id).is_some();
        let message = format!(
            "¿{} «{}» {}?",
            if update { "Actualizar" } else { "Instalar" },
            staged.name,
            staged.version
        );
        let this = self.clone();
        gtk::AlertDialog::builder()
            .message(message)
            .detail(format!(
                "{can}\n\nInstala solo extensiones en las que confíes: pueden ver lo que haces en las páginas a las que tienen acceso."
            ))
            .buttons(["Cancelar", "Instalar"])
            .cancel_button(0)
            .default_button(0)
            .build()
            .choose(Some(&self.window), gio::Cancellable::NONE, move |res| {
                if res.ok() == Some(1) {
                    this.commit(&staged);
                } else {
                    discard(&staged.dir);
                }
            });
    }

    fn commit(self: &Rc<Self>, staged: &Staged) {
        let id = &staged.id;
        if self.ext.borrow().loaded.contains_key(id) {
            self.deactivate(id);
        }
        let root = paths::extensions_dir().join(id);
        let pkg = pkg_dir(id);
        let moved = std::fs::create_dir_all(&root)
            .and_then(|()| {
                let _ = std::fs::remove_dir_all(&pkg);
                std::fs::rename(&staged.dir, &pkg)
            })
            .map_err(|e| e.to_string());
        if let Err(why) = moved {
            let _ = std::fs::remove_dir_all(&staged.dir);
            return self.alert("No se pudo instalar la extensión", &why);
        }
        self.ext.borrow_mut().registry.put(new_installed(
            id,
            &staged.name,
            &staged.version,
            staged.source,
        ));
        self.save_extensions();
        self.mark_fresh(id);
        if let Err(why) = self.activate(id) {
            self.alert("La extensión no arranca", &why);
        }
        self.sync_ext_buttons();
        self.show_extensions_page();
    }

    fn enable_extension(self: &Rc<Self>, id: &str, on: bool) {
        let changed = self.ext.borrow_mut().registry.set_enabled(id, on);
        if !changed {
            return;
        }
        self.save_extensions();
        if on {
            if let Err(why) = self.activate(id) {
                self.alert("La extensión no arranca", &why);
            }
        } else {
            self.deactivate(id);
        }
        self.sync_ext_buttons();
    }

    fn remove_extension(self: &Rc<Self>, id: &str) {
        self.deactivate(id);
        self.ext.borrow_mut().registry.remove(id);
        self.save_extensions();
        let _ = std::fs::remove_dir_all(paths::extensions_dir().join(id));
        self.ext.borrow_mut().storage.retain(|(e, _), _| e != id);
        self.sync_ext_buttons();
    }
}
