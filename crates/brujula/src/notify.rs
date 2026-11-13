//! Web notifications: the question «¿Permitir notificaciones?», the answer kept between sessions,
//! and the notice itself on the desktop.
//!
//! WebKit does not ask, remember or show anything by itself. A page that calls
//! `Notification.requestPermission()` gets a `permission-request` the browser has to answer, the
//! answer is lost when the process ends, and a page that is allowed hands the text of each
//! notice to `show-notification` for the browser to put on the screen. Before this module the
//! first was refused and the third never happened, so no site could ever notify.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use brujula_core::sitepermissions::{self, Decision, SitePermissions};
use nodalia_gtk::gtk;
use nodalia_gtk::gtk::gio;
use nodalia_gtk::gtk::glib;
use nodalia_gtk::gtk::prelude::*;
use webkit6::prelude::*;
use webkit6::{
    Notification, NotificationPermissionRequest, PermissionRequest, PermissionState,
    SecurityOrigin, WebContext, WebView,
};

use crate::dialogs;
use crate::paths;

/// The app action a notice runs when it is clicked.
const CLICK_ACTION: &str = "notification-click";
/// Notices kept to answer a click; the oldest are forgotten after that.
const MAX_LIVE: usize = 64;
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 500;

/// Brings the page that sent a notice to the front.
pub type Focus = Rc<dyn Fn(&WebView)>;

/// A notice on the screen that a click can still answer.
struct Live {
    notification: Notification,
    view: glib::WeakRef<WebView>,
    focus: Focus,
}

thread_local! {
    static LIVE: RefCell<Vec<(String, Live)>> = const { RefCell::new(Vec::new()) };
}

/// Answers the permission questions of the pages of one browser window and shows their
/// notices.
pub struct Notifier {
    app: gtk::Application,
    /// Where the questions are asked.
    parent: glib::WeakRef<gtk::Widget>,
    store: RefCell<SitePermissions>,
    /// Where the answers are kept; none in a private window, where nothing is.
    file: Option<PathBuf>,
    /// Sites that have a question open, with the requests waiting for its answer.
    pending: RefCell<HashMap<String, Vec<PermissionRequest>>>,
    /// Said where the person decides: the preferences page, or the app itself.
    where_to_change: &'static str,
}

impl Notifier {
    /// `file` is where the answers live; `None` refuses every site (a private window).
    pub fn new(
        app: &gtk::Application,
        parent: &impl IsA<gtk::Widget>,
        file: Option<PathBuf>,
        where_to_change: &'static str,
    ) -> Rc<Self> {
        let store = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| SitePermissions::from_json(&t))
            .unwrap_or_default();
        install_click_action(app);
        let me = Rc::new(Self {
            app: app.clone(),
            parent: parent.upcast_ref::<gtk::Widget>().downgrade(),
            store: RefCell::new(store),
            file,
            pending: RefCell::new(HashMap::new()),
            where_to_change,
        });
        if me.file.is_some() {
            me.tell_webkit_the_allowed();
        }
        me
    }

    /// WebKit forgets what it was told when the process ends. It asks (once per web process)
    /// for the sites that were allowed before, so `Notification.permission` is `granted` from
    /// the first page on and nobody is asked twice.
    fn tell_webkit_the_allowed(self: &Rc<Self>) {
        let Some(context) = WebContext::default() else {
            return;
        };
        let this = Rc::downgrade(self);
        context.connect_initialize_notification_permissions(move |context| {
            let Some(this) = this.upgrade() else { return };
            let origins: Vec<SecurityOrigin> = this
                .store
                .borrow()
                .notifications()
                .iter()
                .filter(|n| n.decision == Decision::Allow)
                .map(|n| SecurityOrigin::for_uri(&n.origin))
                .collect();
            let allowed: Vec<&SecurityOrigin> = origins.iter().collect();
            context.initialize_notification_permissions(&allowed, &[]);
        });
    }

    /// Makes `view` ask, remember and show. Connect it before any handler that refuses every
    /// permission: this one leaves alone the requests that are not about notifications.
    pub fn attach(self: &Rc<Self>, view: &WebView, focus: Focus) {
        let this = Rc::downgrade(self);
        view.connect_permission_request(move |v, request| {
            let Some(request) = request.downcast_ref::<NotificationPermissionRequest>() else {
                return false;
            };
            if let Some(this) = this.upgrade() {
                this.ask(v, request.upcast_ref::<PermissionRequest>());
            } else {
                request.deny();
            }
            true
        });

        // `navigator.permissions.query({name: 'notifications'})` says what was answered.
        let this = Rc::downgrade(self);
        view.connect_query_permission_state(move |_, query| {
            if query.name().as_deref() != Some("notifications") {
                return false;
            }
            let state = match (this.upgrade(), query.security_origin()) {
                (Some(this), Some(origin)) => match this.decision_for(&origin.to_str()) {
                    _ if this.file.is_none() => PermissionState::Denied,
                    Some(Decision::Allow) => PermissionState::Granted,
                    Some(Decision::Deny) => PermissionState::Denied,
                    None => PermissionState::Prompt,
                },
                _ => PermissionState::Denied,
            };
            query.finish(state);
            true
        });

        let this = Rc::downgrade(self);
        view.connect_show_notification(move |v, notification| {
            if let Some(this) = this.upgrade() {
                this.show(v, notification, focus.clone());
            }
            true
        });
    }

    fn decision_for(&self, url: &str) -> Option<Decision> {
        self.store.borrow().notifications_for(url)
    }

    /// A page asked for permission to notify.
    fn ask(self: &Rc<Self>, view: &WebView, request: &PermissionRequest) {
        let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
        let origin = match (&self.file, sitepermissions::origin_of(&url)) {
            (Some(_), Some(origin)) => origin,
            // A private window, or a page that is not a secure site: no.
            _ => return request.deny(),
        };
        match self.decision_for(&url) {
            Some(Decision::Allow) => return request.allow(),
            Some(Decision::Deny) => return request.deny(),
            None => {}
        }
        // The same page asking twice waits for the same answer.
        {
            let mut pending = self.pending.borrow_mut();
            if let Some(waiting) = pending.get_mut(&origin) {
                waiting.push(request.clone());
                return;
            }
            pending.insert(origin.clone(), vec![request.clone()]);
        }
        let Some(parent) = self.parent.upgrade() else {
            self.answer(&origin, &url, "later");
            return;
        };
        let site = sitepermissions::label_of(&origin);
        let this = self.clone();
        dialogs::ask(
            &parent,
            &format!("¿Permitir notificaciones de {site}?"),
            &format!(
                "{site} podrá mostrar avisos en el escritorio mientras Brújula esté abierta. {}",
                self.where_to_change
            ),
            &[
                dialogs::Choice::plain("later", "Ahora no"),
                dialogs::Choice::plain("block", "Bloquear"),
                dialogs::Choice::suggested("allow", "Permitir"),
            ],
            "allow",
            "later",
            move |choice| this.answer(&origin, &url, choice),
        );
    }

    /// The person chose: tell every request that waited, and keep the answer unless it was
    /// «Ahora no».
    fn answer(&self, origin: &str, url: &str, choice: &str) {
        let waiting = self.pending.borrow_mut().remove(origin).unwrap_or_default();
        let allow = choice == "allow";
        for request in &waiting {
            if allow {
                request.allow();
            } else {
                request.deny();
            }
        }
        if choice == "allow" || choice == "block" {
            let decision = if allow {
                Decision::Allow
            } else {
                Decision::Deny
            };
            if self.store.borrow_mut().set_notifications(url, decision) {
                self.save();
            }
        }
    }

    fn save(&self) {
        if let Some(file) = &self.file {
            let _ = paths::write_atomic(file, &self.store.borrow().to_json());
        }
    }

    /// Takes the answer for a site back: it asks again the next time.
    pub fn forget(&self, origin: &str) {
        if self.store.borrow_mut().forget_notifications(origin) {
            self.save();
        }
    }

    /// What was answered, for the preferences page: origin, name to show, allowed.
    pub fn rows(&self) -> Vec<(String, String, bool)> {
        self.store
            .borrow()
            .notifications()
            .iter()
            .map(|n| {
                (
                    n.origin.clone(),
                    sitepermissions::label_of(&n.origin),
                    n.decision == Decision::Allow,
                )
            })
            .collect()
    }

    /// Puts a notice from a page on the desktop, if that page's site is allowed.
    fn show(&self, view: &WebView, notification: &Notification, focus: Focus) {
        let url = view.uri().map(|u| u.to_string()).unwrap_or_default();
        // The browser's own record decides, not only what WebKit remembers: a site that was
        // taken back (or never allowed) shows nothing.
        let Some(origin) = sitepermissions::origin_of(&url) else {
            return;
        };
        if self.file.is_none() || self.decision_for(&url) != Some(Decision::Allow) {
            return;
        }
        let site = sitepermissions::label_of(&origin);
        let title = clean(
            notification.title().as_deref().unwrap_or_default(),
            MAX_TITLE,
        );
        let body = clean(notification.body().as_deref().unwrap_or_default(), MAX_BODY);
        let title = if title.is_empty() {
            site.clone()
        } else {
            title
        };

        // A notice with a tag takes the place of the earlier one with the same tag.
        let id = match notification.tag().filter(|t| !t.is_empty()) {
            Some(tag) => format!("tag-{origin}-{}", clean(&tag, 64)),
            None => format!("web-{}-{}", origin, notification.id()),
        };

        let desktop = gio::Notification::new(&title);
        if !body.is_empty() {
            desktop.set_body(Some(&body));
        }
        desktop.set_priority(gio::NotificationPriority::Normal);
        desktop.set_default_action_and_target_value(
            &format!("app.{CLICK_ACTION}"),
            Some(&id.to_variant()),
        );
        self.app.send_notification(Some(&id), &desktop);

        // The page can take its notice back (`notification.close()`).
        let app = self.app.clone();
        let closing = id.clone();
        notification.connect_closed(move |_| {
            app.withdraw_notification(&closing);
            forget_live(&closing);
        });
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            live.retain(|(k, _)| *k != id);
            live.push((
                id,
                Live {
                    notification: notification.clone(),
                    view: view.downgrade(),
                    focus,
                },
            ));
            if live.len() > MAX_LIVE {
                live.remove(0);
            }
        });
    }
}

/// A notice that was clicked: its page comes to the front and is told (so its own `onclick`
/// runs, as WhatsApp's does to open the chat).
fn install_click_action(app: &gtk::Application) {
    if app.lookup_action(CLICK_ACTION).is_some() {
        return;
    }
    let action = gio::SimpleAction::new(CLICK_ACTION, Some(glib::VariantTy::STRING));
    action.connect_activate(|_, id| {
        let Some(id) = id.and_then(|v| v.str()) else {
            return;
        };
        let live = LIVE.with(|live| {
            let mut live = live.borrow_mut();
            let at = live.iter().position(|(k, _)| k == id)?;
            Some(live.remove(at).1)
        });
        let Some(live) = live else { return };
        if let Some(view) = live.view.upgrade() {
            (live.focus)(&view);
        }
        live.notification.clicked();
    });
    app.add_action(&action);
}

fn forget_live(id: &str) {
    LIVE.with(|live| live.borrow_mut().retain(|(k, _)| k != id));
}

/// Text from a page for a notice: no control characters, one line of spaces, a limit.
fn clean(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if out.chars().count() >= max {
            break;
        }
        match c {
            '\n' | '\t' | '\r' => out.push(if c == '\n' { '\n' } else { ' ' }),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn page_text_loses_control_characters_and_is_cut() {
        assert_eq!(clean("  hola\u{7}\u{0}mundo  ", 50), "holamundo");
        assert_eq!(clean("a\tb", 50), "a b");
        assert_eq!(clean("línea 1\nlínea 2", 50), "línea 1\nlínea 2");
        assert_eq!(clean(&"x".repeat(1000), 10), "xxxxxxxxxx");
        assert_eq!(clean("", 10), "");
    }
}
