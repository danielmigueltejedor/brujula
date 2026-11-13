//! Saved passwords: the page script that sees login forms, the questions the browser asks, and
//! the vault that keeps them (see `brujula_core::vault`). None of it exists in a private window.
//!
//! The script runs in a world of its own, in the top frame only, so a page cannot see it or call
//! its message handler. A password leaves the vault only when the vault is open, the page is on
//! the very origin it was saved for, and the request came from a real click or key press in the
//! page; the origin is taken from the world the script runs in, and checked again against the
//! tabs that are open, never from what the page says about itself.

use super::*;

use std::collections::HashSet;

use brujula_core::vault::{origin_of, Offer, Vault, VaultError};
use serde_json::{json, Value};
use webkit6::{javascriptcore, UserContentInjectedFrames, UserScript, UserScriptInjectionTime};
use zeroize::Zeroizing;

const WORLD: &str = "brujula-pw";
const HANDLER: &str = "brujulaPw";
const SCRIPT: &str = include_str!("passwords.js");
/// The vault closes itself after this long without being used.
const LOCK_AFTER_SECS: u32 = 600;
/// A password copied from the preferences is taken off the clipboard after this long.
const CLIPBOARD_SECS: u32 = 30;
const MAX_FIELD: usize = 1024;

/// The state of the vault in a window.
#[derive(Default)]
pub struct PwState {
    /// The vault, while it is open.
    vault: RefCell<Option<Vault>>,
    timer: RefCell<Option<glib::SourceId>>,
    /// A question is on screen: no second one is asked.
    busy: Cell<bool>,
    /// Sites already asked to unlock for, until the vault opens.
    asked: RefCell<HashSet<String>>,
}

type Done = Box<dyn FnOnce(bool)>;
/// Runs with what was typed in the master password window; the text says why it was refused.
type Check = Box<dyn Fn(&Rc<Browser>, &str) -> Result<(), String>>;

impl Browser {
    pub(super) fn start_passwords(self: &Rc<Self>) {
        if self.private {
            return;
        }
        self.ucm
            .register_script_message_handler_with_reply(HANDLER, Some(WORLD));
        let this = Rc::downgrade(self);
        self.ucm.connect_script_message_with_reply_received(
            Some(HANDLER),
            move |_, value, reply| {
                let Some(b) = this.upgrade() else {
                    reply.return_error_message("browser closed");
                    return true;
                };
                let Some(ctx) = value.context() else {
                    reply.return_error_message("no context");
                    return true;
                };
                let text = value.to_str().to_string();
                let reply = reply.clone();
                b.pw_call(
                    &text,
                    Box::new(move |out| {
                        let v = javascriptcore::Value::new_string(&ctx, Some(&out.to_string()));
                        reply.return_value(&v);
                    }),
                );
                true
            },
        );
        self.ucm.add_script(&UserScript::for_world(
            SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::End,
            WORLD,
            &[],
            &[],
        ));
    }

    fn vault_exists() -> bool {
        paths::vault_file().exists()
    }

    fn page_is_open(&self, origin: &str) -> bool {
        self.views
            .borrow()
            .values()
            .any(|v| v.uri().and_then(|u| origin_of(u.as_str())).as_deref() == Some(origin))
    }

    /// A message from the page script.
    fn pw_call(self: &Rc<Self>, text: &str, done: Box<dyn FnOnce(Value)>) {
        let nothing = || json!({});
        if self.private || !self.settings.borrow().passwords_enabled || text.len() > 8192 {
            return done(nothing());
        }
        let Ok(m) = serde_json::from_str::<Value>(text) else {
            return done(nothing());
        };
        let href = m["href"].as_str().unwrap_or("").to_owned();
        let Some(origin) = origin_of(&href) else {
            return done(nothing());
        };
        if !self.page_is_open(&origin) {
            return done(nothing());
        }
        match m["t"].as_str().unwrap_or("") {
            "list" => self.pw_list(&href, origin, done),
            "fill" => {
                let found = m["id"].as_u64().and_then(|id| {
                    let g = self.pw.vault.borrow();
                    let l = g.as_ref()?.find(id)?;
                    (l.origin == origin).then(|| json!({"u": l.username, "p": l.password}))
                });
                if found.is_some() {
                    self.pw_touch();
                }
                done(found.unwrap_or_else(nothing));
            }
            "submit" => {
                done(nothing());
                let user = m["u"].as_str().unwrap_or("").to_owned();
                let pass = Zeroizing::new(m["p"].as_str().unwrap_or("").to_owned());
                self.pw_submit(&href, user, pass);
            }
            _ => done(nothing()),
        }
    }

    /// The saved users for a page, never their passwords. A closed vault is asked to open once
    /// per site, when the user touches a login field.
    fn pw_list(self: &Rc<Self>, href: &str, origin: String, done: Box<dyn FnOnce(Value)>) {
        let list = |b: &Browser, href: &str| {
            let g = b.pw.vault.borrow();
            let logins: Vec<Value> = g
                .as_ref()
                .map(|v| {
                    v.for_page(href)
                        .iter()
                        .map(|l| json!({"id": l.id, "u": l.username}))
                        .collect()
                })
                .unwrap_or_default();
            json!({ "logins": logins })
        };
        if self.pw.vault.borrow().is_some() {
            self.pw_touch();
            return done(list(self, href));
        }
        if !Self::vault_exists() || self.pw.busy.get() || !self.pw.asked.borrow_mut().insert(origin)
        {
            return done(json!({}));
        }
        self.pw.busy.set(true);
        let this = self.clone();
        let href = href.to_owned();
        self.pw_unlock(Box::new(move |ok| {
            this.pw.busy.set(false);
            done(if ok { list(&this, &href) } else { json!({}) });
        }));
    }

    /// What the page typed in a login form: offer to keep it.
    fn pw_submit(self: &Rc<Self>, href: &str, user: String, pass: Zeroizing<String>) {
        let user: String = user
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_FIELD)
            .collect();
        if pass.is_empty() || pass.len() > MAX_FIELD || self.pw.busy.get() {
            return;
        }
        let update = match self.pw.vault.borrow().as_ref() {
            Some(v) => match v.check(href, &user, &pass) {
                Offer::Nothing => return,
                Offer::Save => false,
                Offer::Update(_) => true,
            },
            None => false,
        };
        self.pw.busy.set(true);
        let shown = if user.is_empty() {
            "(sin usuario)".to_owned()
        } else {
            user.clone()
        };
        let message = format!(
            "¿{} la contraseña de {shown} en {}?",
            if update { "Actualizar" } else { "Guardar" },
            host_of(href)
        );
        let detail = if Self::vault_exists() {
            "Se guarda cifrada con tu contraseña maestra."
        } else {
            "Se guarda cifrada con una contraseña maestra que vas a crear ahora."
        };
        let this = self.clone();
        let href = href.to_owned();
        dialogs::ask(
            &self.window,
            &message,
            detail,
            &[
                dialogs::Choice::plain("never", "Nunca para este sitio"),
                dialogs::Choice::plain("later", "Ahora no"),
                dialogs::Choice::suggested("save", "Guardar"),
            ],
            "save",
            "later",
            move |choice| {
                if choice != "never" && choice != "save" {
                    this.pw.busy.set(false);
                    return;
                }
                let never = choice == "never";
                let href = href.clone();
                let (user, pass) = (user.clone(), pass.clone());
                let that = this.clone();
                this.pw_with_vault(Box::new(move |ok| {
                    that.pw.busy.set(false);
                    if !ok {
                        return;
                    }
                    if let Some(v) = that.pw.vault.borrow_mut().as_mut() {
                        if never {
                            v.never_for(&href);
                        } else {
                            v.put(&href, &user, &pass, now_ms());
                        }
                    }
                    that.save_vault();
                    that.redraw_settings();
                }));
            },
        );
    }

    /// Puts logins brought from another browser into the vault, which is opened (or made) first.
    /// `done` hears how many were new: a login for a site and user that is already saved is left
    /// as it is. If the master password is not given, nothing is saved and the count is 0.
    pub(super) fn pw_import(
        self: &Rc<Self>,
        logins: Vec<brujula_core::browser_data::Login>,
        done: Box<dyn FnOnce(usize)>,
    ) {
        if self.private || self.pw.busy.replace(true) {
            done(0);
            return;
        }
        let this = self.clone();
        self.pw_with_vault(Box::new(move |ok| {
            this.pw.busy.set(false);
            let mut added = 0;
            if ok {
                let now = now_ms();
                if let Some(vault) = this.pw.vault.borrow_mut().as_mut() {
                    for l in logins {
                        let origin = origin_of(&l.url);
                        let known = origin.as_ref().is_some_and(|o| {
                            vault
                                .logins()
                                .iter()
                                .any(|k| &k.origin == o && k.username == l.username)
                        });
                        if !known && vault.put(&l.url, &l.username, &l.password, now).is_some() {
                            added += 1;
                        }
                    }
                }
                this.save_vault();
            }
            done(added);
        }));
    }

    /// Runs `then` once the vault is open: opening it, or making it, as needed.
    fn pw_with_vault(self: &Rc<Self>, then: Done) {
        if self.pw.vault.borrow().is_some() {
            self.pw_touch();
            then(true);
        } else if Self::vault_exists() {
            self.pw_unlock(then);
        } else {
            self.pw_create(then);
        }
    }

    fn write_vault(&self, vault: &Vault) -> bool {
        paths::write_private(&paths::vault_file(), &vault.seal()).is_ok()
    }

    fn save_vault(&self) {
        if let Some(v) = self.pw.vault.borrow().as_ref() {
            if !self.write_vault(v) {
                eprintln!("brujula: the password file could not be written");
            }
        }
    }

    fn pw_create(self: &Rc<Self>, done: Done) {
        self.ask_master(
            "Crea la contraseña maestra",
            "Protege todas tus contraseñas guardadas. Si la olvidas no se puede recuperar. Mínimo 8 caracteres.",
            true,
            Box::new(|b, master| {
                let vault = Vault::create(master).map_err(|e| e.to_string())?;
                if !b.write_vault(&vault) {
                    return Err("No se pudo guardar el archivo.".to_owned());
                }
                *b.pw.vault.borrow_mut() = Some(vault);
                b.pw.asked.borrow_mut().clear();
                b.pw_touch();
                Ok(())
            }),
            done,
        );
    }

    fn pw_unlock(self: &Rc<Self>, done: Done) {
        self.ask_master(
            "Desbloquear las contraseñas",
            "Escribe tu contraseña maestra.",
            false,
            Box::new(|b, master| {
                let text = std::fs::read_to_string(paths::vault_file())
                    .map_err(|_| "No se pudo leer el archivo de contraseñas.".to_owned())?;
                match Vault::open(&text, master) {
                    Ok(v) => {
                        *b.pw.vault.borrow_mut() = Some(v);
                        b.pw.asked.borrow_mut().clear();
                        b.pw_touch();
                        Ok(())
                    }
                    Err(VaultError::Unlock) => Err("Contraseña incorrecta.".to_owned()),
                    Err(e) => Err(e.to_string()),
                }
            }),
            done,
        );
    }

    fn pw_change_master(self: &Rc<Self>) {
        if self.pw.vault.borrow().is_none() || self.pw.busy.replace(true) {
            return;
        }
        let this = self.clone();
        self.ask_master(
            "Nueva contraseña maestra",
            "Reemplaza a la actual. Mínimo 8 caracteres.",
            true,
            Box::new(|b, master| {
                let text = {
                    let mut g = b.pw.vault.borrow_mut();
                    let v = g.as_mut().ok_or("Las contraseñas están bloqueadas.")?;
                    v.change_master(master).map_err(|e| e.to_string())?;
                    v.seal()
                };
                paths::write_private(&paths::vault_file(), &text)
                    .map_err(|_| "No se pudo guardar el archivo.".to_owned())
            }),
            Box::new(move |_| this.pw.busy.set(false)),
        );
    }

    /// Closes the vault: what was open is wiped from memory.
    pub(super) fn pw_lock(&self) {
        *self.pw.vault.borrow_mut() = None;
        self.pw.asked.borrow_mut().clear();
        if let Some(id) = self.pw.timer.borrow_mut().take() {
            id.remove();
        }
    }

    /// The vault was used: it stays open a while longer.
    fn pw_touch(self: &Rc<Self>) {
        if let Some(id) = self.pw.timer.borrow_mut().take() {
            id.remove();
        }
        let this = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local(LOCK_AFTER_SECS, move || {
            if let Some(b) = this.upgrade() {
                b.pw.timer.borrow_mut().take();
                b.pw_lock();
                b.redraw_settings();
            }
            glib::ControlFlow::Break
        });
        *self.pw.timer.borrow_mut() = Some(id);
    }

    /// A window that asks for the master password. `check` runs with what was typed and says
    /// why not when it fails, so the window stays for another try; `done` hears how it ended.
    fn ask_master(
        self: &Rc<Self>,
        title: &str,
        detail: &str,
        confirm: bool,
        check: Check,
        done: Done,
    ) {
        let win = ndl::Dialog::builder()
            .title(title)
            .content_width(420)
            .build();
        let col = gtk::Box::new(gtk::Orientation::Vertical, 12);
        col.set_margin_top(20);
        col.set_margin_bottom(20);
        col.set_margin_start(20);
        col.set_margin_end(20);
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
        let first = gtk::PasswordEntry::builder()
            .show_peek_icon(true)
            .placeholder_text("Contraseña maestra")
            .activates_default(false)
            .build();
        col.append(&first);
        let second = gtk::PasswordEntry::builder()
            .show_peek_icon(true)
            .placeholder_text("Repítela")
            .build();
        if confirm {
            col.append(&second);
        }
        let error = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["error"])
            .build();
        col.append(&error);
        let cancel = gtk::Button::with_label("Cancelar");
        let ok = gtk::Button::builder()
            .label("Aceptar")
            .css_classes(["suggested-action"])
            .build();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_homogeneous(true);
        buttons.append(&cancel);
        buttons.append(&ok);
        col.append(&buttons);
        win.set_child(Some(&col));

        let done: Rc<RefCell<Option<Done>>> = Rc::new(RefCell::new(Some(done)));
        let finish = {
            let done = done.clone();
            let win = win.clone();
            Rc::new(move |ok: bool| {
                let d = done.borrow_mut().take();
                win.force_close();
                if let Some(d) = d {
                    d(ok);
                }
            })
        };
        {
            let done = done.clone();
            win.connect_closed(move |_| {
                if let Some(d) = done.borrow_mut().take() {
                    d(false);
                }
            });
        }
        let submit = {
            let this = self.clone();
            let (first, second, error, finish) =
                (first.clone(), second.clone(), error.clone(), finish.clone());
            Rc::new(move || {
                let a = Zeroizing::new(first.text().to_string());
                if confirm && *a != *second.text() {
                    error.set_label("Las contraseñas no coinciden.");
                    return;
                }
                match check(&this, &a) {
                    Ok(()) => finish(true),
                    Err(why) => {
                        error.set_label(&why);
                        first.set_text("");
                        second.set_text("");
                        first.grab_focus();
                    }
                }
            })
        };
        {
            let finish = finish.clone();
            cancel.connect_clicked(move |_| finish(false));
        }
        {
            let submit = submit.clone();
            ok.connect_clicked(move |_| submit());
        }
        {
            let submit = submit.clone();
            let second = second.clone();
            first.connect_activate(move |_| {
                if confirm {
                    second.grab_focus();
                } else {
                    submit();
                }
            });
        }
        {
            let submit = submit.clone();
            second.connect_activate(move |_| submit());
        }
        win.present(Some(&self.window));
        first.grab_focus();
    }

    /// One of the `pw.*` changes of the preferences page.
    pub(super) fn change_passwords(self: &Rc<Self>, what: &str, value: &str) {
        if self.private {
            return;
        }
        let redraw = |this: &Rc<Self>| {
            let this = this.clone();
            Box::new(move |_: bool| {
                this.pw.busy.set(false);
                this.redraw_settings();
            }) as Done
        };
        match what {
            "create" if !Self::vault_exists() && !self.pw.busy.replace(true) => {
                self.pw_create(redraw(self));
            }
            "unlock"
                if Self::vault_exists()
                    && self.pw.vault.borrow().is_none()
                    && !self.pw.busy.replace(true) =>
            {
                self.pw_unlock(redraw(self));
            }
            "lock" => self.pw_lock(),
            "change" => self.pw_change_master(),
            "wipe" => self.pw_wipe(),
            "copy" => self.pw_copy(value),
            "remove" => {
                if let (Ok(id), Some(v)) =
                    (value.parse::<u64>(), self.pw.vault.borrow_mut().as_mut())
                {
                    v.remove(id);
                }
                self.pw_touch();
                self.save_vault();
            }
            "unnever" => {
                if let Some(v) = self.pw.vault.borrow_mut().as_mut() {
                    v.forget_never(value);
                }
                self.save_vault();
            }
            _ => {}
        }
    }

    /// Deletes the vault file, after asking: this is what is left when the master is forgotten.
    fn pw_wipe(self: &Rc<Self>) {
        if self.pw.busy.replace(true) {
            return;
        }
        let this = self.clone();
        dialogs::ask(
            &self.window,
            "¿Borrar todas las contraseñas guardadas?",
            "No se pueden recuperar.",
            &[
                dialogs::Choice::plain("cancel", "Cancelar"),
                dialogs::Choice::destructive("wipe", "Borrar todas"),
            ],
            "cancel",
            "cancel",
            move |choice| {
                this.pw.busy.set(false);
                if choice == "wipe" {
                    let _ = std::fs::remove_file(paths::vault_file());
                    this.pw_lock();
                    this.redraw_settings();
                }
            },
        );
    }

    /// Puts a saved password on the clipboard, and takes it off a moment later.
    fn pw_copy(self: &Rc<Self>, id: &str) {
        let Some(secret) = id.parse::<u64>().ok().and_then(|id| {
            self.pw
                .vault
                .borrow()
                .as_ref()?
                .find(id)
                .map(|l| Zeroizing::new(l.password.clone()))
        }) else {
            return;
        };
        self.pw_touch();
        let clip = self.window.clipboard();
        clip.set_text(&secret);
        glib::timeout_add_seconds_local_once(CLIPBOARD_SECS, move || {
            glib::spawn_future_local(async move {
                if let Ok(Some(now)) = clip.read_text_future().await {
                    if now.as_str() == secret.as_str() {
                        clip.set_text("");
                    }
                }
            });
        });
    }

    /// What the preferences page shows of the vault: never a password.
    pub(super) fn vault_view(&self) -> prefs::VaultView {
        let g = self.pw.vault.borrow();
        prefs::VaultView {
            exists: Self::vault_exists(),
            open: g.is_some(),
            logins: g
                .as_ref()
                .map(|v| {
                    v.logins()
                        .iter()
                        .map(|l| (l.id, l.origin.clone(), l.username.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            never: g.as_ref().map(|v| v.never().to_vec()).unwrap_or_default(),
        }
    }
}
