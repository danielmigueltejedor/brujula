//! The «Perfiles» section of the preferences: adding, opening, locking and removing profiles.

use brujula_core::profiles::{self as core_profiles, Profiles};

use super::*;

/// The name given to the profile the browser has been using until now, the first time a second
/// one is made.
fn main_name() -> String {
    let name = glib::real_name().to_string_lossy().trim().to_owned();
    if name.is_empty() || name == "Unknown" {
        "Principal".to_owned()
    } else {
        name
    }
}

/// Deletes the files of a profile (never the main one: its folders are the root of the others).
fn delete_files(id: &str) {
    if id == core_profiles::DEFAULT_ID || !core_profiles::valid_id(id) {
        return;
    }
    for root in [
        paths::root_data_dir(),
        paths::root_cache_dir(),
        paths::root_config_dir(),
    ] {
        let _ = std::fs::remove_dir_all(root.join("profiles").join(id));
    }
}

impl Browser {
    pub(super) fn profile_rows(&self) -> Vec<(String, String, String, bool)> {
        crate::profiles::load()
            .list()
            .iter()
            .map(|p| (p.id.clone(), p.name.clone(), p.avatar.clone(), p.has_lock()))
            .collect()
    }

    fn save_profiles(&self, profiles: &Profiles) -> bool {
        let ok = crate::profiles::save(profiles).is_ok();
        if !ok {
            self.essential_alert(
                "No se pudo guardar",
                "No se pudo escribir el archivo de perfiles. Comprueba el espacio y los permisos de tu carpeta personal.",
            );
        }
        ok
    }

    /// One of the `profile.*` changes of the preferences page.
    pub(super) fn change_profile(self: &Rc<Self>, what: &str, value: &str) {
        if self.private {
            return;
        }
        let mut profiles = crate::profiles::load();
        match what {
            "add" => {
                let (name, avatar) = value.split_once('\n').unwrap_or((value, ""));
                // The profile in use until now becomes the main one.
                if profiles.is_empty() {
                    profiles.add(&main_name(), "compass", None);
                }
                if profiles.add(name, avatar, None).is_none() {
                    self.essential_alert(
                        "No se pudo añadir el perfil",
                        "Ya hay el máximo de perfiles.",
                    );
                    return;
                }
                if self.save_profiles(&profiles) {
                    self.redraw_settings();
                }
            }
            "open" => {
                let id = value.to_owned();
                if !core_profiles::valid_id(&id) || profiles.get(&id).is_none() {
                    return;
                }
                let launch: Rc<dyn Fn()> = Rc::new({
                    let id = id.clone();
                    move || {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe)
                                .arg("--profile")
                                .arg(&id)
                                .spawn();
                        }
                    }
                });
                crate::profiles::unlock(&self.window, Rc::new(profiles), id, false, launch);
            }
            "password" => {
                let id = value.to_owned();
                let Some(name) = profiles.get(&id).map(|p| p.name.clone()) else {
                    return;
                };
                let this = self.clone();
                let target = id.clone();
                let ask_new: Rc<dyn Fn()> = Rc::new(move || {
                    let this2 = this.clone();
                    let target = target.clone();
                    dialogs::ask_secret(
                        &this.window,
                        &format!("Contraseña de {name}"),
                        "Escribe la contraseña nueva (mínimo 4 caracteres) o déjala vacía para quitarla. Solo protege el selector de perfiles: no cifra los archivos.",
                        "Guardar",
                        move |password| {
                            let mut profiles = crate::profiles::load();
                            let password = (!password.is_empty()).then_some(password.as_str());
                            if profiles.set_password(&target, password) {
                                if this2.save_profiles(&profiles) {
                                    this2.redraw_settings();
                                }
                            } else {
                                this2.essential_alert(
                                    "Contraseña no válida",
                                    "Necesita al menos 4 caracteres.",
                                );
                            }
                        },
                    );
                });
                crate::profiles::unlock(&self.window, Rc::new(profiles), id, false, ask_new);
            }
            "remove" => {
                let id = value.to_owned();
                let Some(name) = profiles.get(&id).map(|p| p.name.clone()) else {
                    return;
                };
                if id == core_profiles::DEFAULT_ID || id == paths::profile() {
                    return;
                }
                let this = self.clone();
                let target = id.clone();
                let confirm: Rc<dyn Fn()> = Rc::new(move || {
                    let this2 = this.clone();
                    let target = target.clone();
                    dialogs::ask(
                        &this.window,
                        &format!("¿Quitar el perfil de {name}?"),
                        "Se borran sus marcadores, historial, contraseñas y ajustes de este equipo. No se puede deshacer.",
                        &[
                            dialogs::Choice::plain("cancel", "Cancelar"),
                            dialogs::Choice::destructive("remove", "Quitar perfil"),
                        ],
                        "cancel",
                        "cancel",
                        move |answer| {
                            if answer != "remove" {
                                return;
                            }
                            let mut profiles = crate::profiles::load();
                            if profiles.remove(&target) && this2.save_profiles(&profiles) {
                                delete_files(&target);
                                this2.redraw_settings();
                            }
                        },
                    );
                });
                crate::profiles::unlock(&self.window, Rc::new(profiles), id, false, confirm);
            }
            _ => {}
        }
    }
}
