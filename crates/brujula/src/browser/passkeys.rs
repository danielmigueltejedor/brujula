//! Passkeys with the phone as the authenticator: a page asks, Brújula shows a QR code, the
//! phone scans it, finds the computer over Bluetooth and makes or signs with the passkey. The
//! protocol is in `brujula_passkeys`; this is what a browser window adds around it.
//!
//! Two scripts take part (see their headers). `passkeys_main.js` gives the page the WebAuthn
//! objects and can be seen by it. `passkeys.js` runs in a world of its own, in the top frame
//! only, and is the only one that reaches the browser. The page cannot say who it is: the
//! browser finds the options by asking the tab in front, and takes the origin from that tab's
//! own address. A request that is not waiting in the tab in front is turned down, which also
//! stops a tab in the background from asking in the name of the one the person is looking at.
//! The person always sees the dialog, with the site's name in it, and has to scan with a phone
//! they hold; nothing is done without that.

use super::*;

use brujula_core::vault::origin_of;
use brujula_passkeys::{
    Canceller, Ceremony, Event, Failure, Kind, QrMatrix, Reason, Request, Stage,
};
use serde_json::{json, Value};
use webkit6::{javascriptcore, UserContentInjectedFrames, UserScript, UserScriptInjectionTime};

const WORLD: &str = "brujula-wa";
const HANDLER: &str = "brujulaWa";
/// What the page sees.
const PAGE_SCRIPT: &str = include_str!("passkeys_main.js");
/// What talks to the browser.
const WORLD_SCRIPT: &str = include_str!("passkeys.js");
/// The QR code, in pixels a side, before the quiet zone.
const QR_PX: i32 = 216;
/// Light squares around the code, as the QR standard asks.
const QUIET: usize = 2;

/// The state of the passkeys in a window.
#[derive(Default)]
pub struct PkState {
    /// A ceremony is on screen: no second one is started.
    active: RefCell<Option<Active>>,
}

struct Active {
    id: String,
    cancel: Canceller,
}

type Done = Box<dyn FnOnce(Value)>;

impl Browser {
    pub(super) fn start_passkeys(self: &Rc<Self>) {
        if !self.settings.borrow().passkeys_enabled {
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
                b.pk_call(
                    &text,
                    Box::new(move |out| {
                        let v = javascriptcore::Value::new_string(&ctx, Some(&out.to_string()));
                        reply.return_value(&v);
                    }),
                );
                true
            },
        );
        // The page's half goes in the world of the page, which is the one the page reads.
        self.ucm.add_script(&UserScript::new(
            PAGE_SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::Start,
            &[],
            &[],
        ));
        self.ucm.add_script(&UserScript::for_world(
            WORLD_SCRIPT,
            UserContentInjectedFrames::TopFrame,
            UserScriptInjectionTime::Start,
            WORLD,
            &[],
            &[],
        ));
    }

    /// A message from the script that runs beside the page.
    fn pk_call(self: &Rc<Self>, text: &str, done: Done) {
        let refuse = |done: Done, msg: &str| {
            done(json!({"err": "NotAllowedError", "msg": msg}));
        };
        if !self.settings.borrow().passkeys_enabled || text.len() > 1024 {
            return refuse(done, "Passkeys are turned off.");
        }
        let Ok(m) = serde_json::from_str::<Value>(text) else {
            return refuse(done, "Not understood.");
        };
        let Some(id) = m["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .map(str::to_owned)
        else {
            return refuse(done, "Not understood.");
        };
        match m["t"].as_str().unwrap_or("") {
            "cancel" => {
                if let Some(a) = self.pk.active.borrow().as_ref().filter(|a| a.id == id) {
                    a.cancel.cancel();
                }
                done(json!({}));
            }
            "req" => self.pk_request(id, done),
            _ => refuse(done, "Not understood."),
        }
    }

    /// A page asks for a passkey. Who it is comes from the tab in front, not from the message.
    fn pk_request(self: &Rc<Self>, id: String, done: Done) {
        if self.pk.active.borrow().is_some() {
            return done(
                json!({"err": "NotAllowedError", "msg": "Another passkey request is in progress."}),
            );
        }
        let Some(view) = self.current_view() else {
            return done(json!({"err": "NotAllowedError", "msg": "No page in front."}));
        };
        let Some(origin) = view.uri().and_then(|u| origin_of(u.as_str())) else {
            return done(json!({"err": "SecurityError", "msg": "This page cannot use passkeys."}));
        };
        let script = format!("window.__brujulaWa ? window.__brujulaWa.take(\"{id}\") : null");
        let this = self.clone();
        view.evaluate_javascript(
            &script,
            Some(WORLD),
            None,
            gio::Cancellable::NONE,
            move |result| {
                let taken = result
                    .ok()
                    .filter(|v| v.is_string())
                    .and_then(|v| serde_json::from_str::<Value>(v.to_str().as_str()).ok());
                let Some(t) = taken else {
                    // Not in the tab in front: a background tab, or one that has moved on.
                    return done(json!({"err": "NotAllowedError", "msg": "The request is not from the page in front."}));
                };
                let kind = match t["kind"].as_str() {
                    Some("create") => Kind::Create,
                    Some("get") => Kind::Get,
                    _ => return done(json!({"err": "TypeError", "msg": "Unknown request."})),
                };
                let Some(options) = t["options"].as_str() else {
                    return done(json!({"err": "TypeError", "msg": "No options."}));
                };
                // The tab may have gone somewhere else while the script answered.
                if this
                    .current_view()
                    .and_then(|v| v.uri())
                    .and_then(|u| origin_of(u.as_str()))
                    .as_deref()
                    != Some(origin.as_str())
                {
                    return done(json!({"err": "NotAllowedError", "msg": "The page changed."}));
                }
                this.pk_ceremony(
                    id,
                    Request {
                        kind,
                        origin,
                        options: options.to_owned(),
                    },
                    done,
                );
            },
        );
    }

    fn pk_ceremony(self: &Rc<Self>, id: String, request: Request, done: Done) {
        let host = host_of(&request.origin);
        let kind = request.kind;
        let ceremony = brujula_passkeys::start(request);
        *self.pk.active.borrow_mut() = Some(Active {
            id,
            cancel: ceremony.canceller(),
        });
        let this = self.clone();
        glib::spawn_future_local(async move {
            let outcome = this.pk_watch(ceremony, &host, kind).await;
            this.pk.active.borrow_mut().take();
            done(match outcome {
                Ok(json) => json!({"ok": json}),
                Err(f) => json!({"err": f.reason.dom_error(), "msg": message_for_page(&f)}),
            });
        });
    }

    /// Shows the ceremony and follows it to its end.
    async fn pk_watch(
        self: &Rc<Self>,
        mut ceremony: Ceremony,
        host: &str,
        kind: Kind,
    ) -> Result<String, Failure> {
        let mut shown: Option<QrDialog> = None;
        let canceller = ceremony.canceller();
        while let Some(event) = ceremony.next().await {
            match event {
                Event::Qr(text) => {
                    let Some(matrix) = QrMatrix::new(&text) else {
                        canceller.cancel();
                        continue;
                    };
                    let d = QrDialog::new(host, kind, matrix);
                    let stop = canceller.clone();
                    d.dialog.connect_response(None, move |_, _| stop.cancel());
                    d.dialog.present(Some(&self.window));
                    shown = Some(d);
                }
                Event::Stage(stage) => {
                    if let Some(d) = &shown {
                        d.status.set_label(stage_text(stage));
                    }
                }
                Event::Done(json) => {
                    close(&shown);
                    return Ok(json);
                }
                Event::Failed(f) => {
                    close(&shown);
                    if let Some((heading, body)) = explain(&f, host) {
                        dialogs::notice(&self.window, heading, &body);
                    }
                    return Err(f);
                }
            }
        }
        close(&shown);
        Err(Failure {
            reason: Reason::Other,
            detail: "the ceremony ended without an answer".into(),
        })
    }
}

fn close(d: &Option<QrDialog>) {
    if let Some(d) = d {
        d.dialog.dialog().force_close();
    }
}

/// What the page script allows as a request id.
fn valid_id(id: &str) -> bool {
    (8..=80).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn stage_text(stage: Stage) -> &'static str {
    match stage {
        Stage::Waiting => "Esperando al teléfono…",
        Stage::Nearby => "Comprobando que el teléfono está cerca…",
        Stage::Connecting => "Conectando con el teléfono…",
        Stage::Authenticating => "Creando una conexión segura…",
        Stage::Connected | Stage::Touch => "Conectado. Confirma en el teléfono.",
    }
}

/// What the person is told when a ceremony fails, if anything: closing the dialog, saying no on
/// the phone and a page that sent nonsense need no explanation.
fn explain(f: &Failure, host: &str) -> Option<(&'static str, String)> {
    Some(match f.reason {
        Reason::NoBluetooth => (
            "No se puede usar el teléfono",
            "Este equipo no tiene Bluetooth o está apagado. Brújula lo usa para comprobar que el \
             teléfono está cerca, así nadie puede pedirte la llave desde otro lugar."
                .to_owned(),
        ),
        Reason::TimedOut => (
            "Se acabó el tiempo",
            "No se terminó a tiempo. Puedes volver a intentarlo desde la página.".to_owned(),
        ),
        Reason::Connection => (
            "No se pudo hablar con el teléfono",
            "Comprueba que el teléfono está cerca, con el Bluetooth encendido y con internet, y \
             vuelve a intentarlo."
                .to_owned(),
        ),
        Reason::AlreadyRegistered => (
            "Ya tienes una llave de acceso",
            format!("Este teléfono ya tiene una llave de acceso para esa cuenta de {host}."),
        ),
        Reason::NoPasskey => (
            "No hay llave de acceso",
            format!("El teléfono no tiene ninguna llave de acceso para {host}."),
        ),
        Reason::Unsupported => (
            "El teléfono no puede hacerlo",
            format!("El teléfono no admite lo que pide {host}."),
        ),
        Reason::Other => (
            "No se pudo completar",
            "Algo falló al hablar con el teléfono. Vuelve a intentarlo.".to_owned(),
        ),
        Reason::Cancelled | Reason::Declined | Reason::WrongSite | Reason::Invalid => return None,
    })
}

/// The text of the exception the page gets. Short and without anything about the phone.
fn message_for_page(f: &Failure) -> &'static str {
    match f.reason {
        Reason::WrongSite => "The relying party ID is not valid for this origin.",
        Reason::Invalid => "The options are not valid.",
        Reason::AlreadyRegistered => "A credential for this account already exists.",
        Reason::Unsupported => "The authenticator does not support the request.",
        _ => "The operation either timed out or was not allowed.",
    }
}

/// The dialog with the code.
struct QrDialog {
    dialog: ndl::AlertDialog,
    status: gtk::Label,
}

impl QrDialog {
    fn new(host: &str, kind: Kind, matrix: QrMatrix) -> Self {
        let (heading, body) = match kind {
            Kind::Create => (
                "Crear una llave de acceso",
                format!("{host} quiere guardar una llave de acceso en tu teléfono. Escanea el código con su cámara. El teléfono tiene que estar cerca y con el Bluetooth encendido."),
            ),
            Kind::Get => (
                "Entrar con una llave de acceso",
                format!("Entra en {host} con la llave de acceso de tu teléfono. Escanea el código con su cámara. El teléfono tiene que estar cerca y con el Bluetooth encendido."),
            ),
        };
        let dialog = ndl::AlertDialog::new(Some(heading), Some(&body));
        let code = gtk::DrawingArea::builder()
            .content_width(QR_PX)
            .content_height(QR_PX)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .build();
        code.set_draw_func(move |_, cr, w, h| paint(&matrix, cr, w, h));
        let status = gtk::Label::builder()
            .label(stage_text(Stage::Waiting))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .css_classes(["dim-label"])
            .margin_top(12)
            .build();
        let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
        column.append(&code);
        column.append(&status);
        dialog.set_extra_child(Some(&column));
        dialog.add_response("cancel", "Cancelar");
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        Self { dialog, status }
    }
}

/// The code on a white card, whatever the theme: a phone reads dark squares on light.
fn paint(matrix: &QrMatrix, cr: &gtk::cairo::Context, width: i32, height: i32) {
    let side = width.min(height) as f64;
    let (x0, y0) = ((width as f64 - side) / 2.0, (height as f64 - side) / 2.0);
    let radius = 12.0;
    cr.new_sub_path();
    cr.arc(
        x0 + side - radius,
        y0 + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x0 + side - radius,
        y0 + side - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x0 + radius,
        y0 + side - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x0 + radius,
        y0 + radius,
        radius,
        std::f64::consts::PI,
        3.0 * std::f64::consts::FRAC_PI_2,
    );
    cr.close_path();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.fill_preserve();
    // A hairline, so the white card shows on a light dialog too.
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.14);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
    // Whole pixels for each square keeps the edges sharp, which a scanner likes.
    let squares = (matrix.size() + 2 * QUIET) as f64;
    let cell = (side / squares).floor().max(1.0);
    let used = cell * squares;
    let (ox, oy) = (
        x0 + (side - used) / 2.0 + cell * QUIET as f64,
        y0 + (side - used) / 2.0 + cell * QUIET as f64,
    );
    cr.set_source_rgb(0.07, 0.07, 0.07);
    for y in 0..matrix.size() {
        for x in 0..matrix.size() {
            if matrix.is_dark(x, y) {
                cr.rectangle(ox + x as f64 * cell, oy + y as f64 * cell, cell, cell);
            }
        }
    }
    let _ = cr.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_ids_the_script_makes_are_accepted() {
        assert!(valid_id("w0a1b2c3d4e5f6a7b8c9d0e1"));
        for bad in [
            "",
            "short",
            "UPPERCASE12345",
            "has space 12345",
            "quote\"12345678",
            "a;b12345678",
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert!(!valid_id(&"a".repeat(81)));
    }

    #[test]
    fn what_needs_no_words_gets_none() {
        let f = |reason| Failure {
            reason,
            detail: String::new(),
        };
        for r in [
            Reason::Cancelled,
            Reason::Declined,
            Reason::WrongSite,
            Reason::Invalid,
        ] {
            assert!(explain(&f(r), "example.com").is_none(), "{r:?}");
        }
        for r in [
            Reason::NoBluetooth,
            Reason::TimedOut,
            Reason::Connection,
            Reason::Other,
        ] {
            assert!(explain(&f(r), "example.com").is_some(), "{r:?}");
        }
    }

    #[test]
    fn the_page_is_not_told_what_the_phone_has() {
        let f = |reason| Failure {
            reason,
            detail: "secret detail".into(),
        };
        for r in [
            Reason::NoPasskey,
            Reason::Declined,
            Reason::Cancelled,
            Reason::TimedOut,
            Reason::NoBluetooth,
        ] {
            assert_eq!(
                message_for_page(&f(r)),
                "The operation either timed out or was not allowed."
            );
        }
        assert!(!message_for_page(&f(Reason::Other)).contains("secret"));
    }
}
