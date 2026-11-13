//! Passkeys without a UI: the phone as the authenticator.
//!
//! WebKitGTK has no WebAuthn of its own, so a page that asks for a passkey is answered by
//! Brújula. This crate is the part that talks to the phone. It shows a QR code, waits for the
//! phone to find the computer over Bluetooth, meets it through the tunnel server of the
//! phone's maker (the "hybrid" transport of the FIDO Alliance, also known as caBLE) and runs
//! the WebAuthn ceremony there. The private key never leaves the phone, and the phone checks
//! that the person is who they say they are.
//!
//! The protocol itself is `libwebauthn`'s (LGPL-2.1-or-later). What this crate adds is what a
//! browser has to get right and a library cannot decide for it:
//!
//! * the origin comes from the browser, never from the page, and has to be a secure one;
//! * `rpId` is checked against that origin with the public suffix list;
//! * the answer goes back to the page as the WebAuthn JSON form, which the page script turns
//!   into a credential;
//! * every way a ceremony can end is one of a few [`Reason`]s, each with its DOM error.
//!
//! A ceremony runs on a thread of its own, with a tokio runtime that lives as long as it does,
//! and reports through [`Ceremony::next`], which the main loop can await.

mod psl_list;
pub mod qr;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::StreamExt;
use libwebauthn::ops::webauthn::{
    GetAssertionRequest, JsonFormat, MakeCredentialRequest, OriginValidation, RelatedOrigins,
    RequestOrigin, RequestSettings, WebAuthnIDLResponse,
};
use libwebauthn::proto::CtapError;
use libwebauthn::transport::cable::channel::{CableUpdate, CableUxUpdate};
use libwebauthn::transport::cable::error::CableError;
use libwebauthn::transport::cable::qr_code_device::{
    CableQrCodeDevice, CableTransports, QrCodeOperationHint,
};
use libwebauthn::transport::{Channel as _, ChannelSettings, Device as _};
use libwebauthn::webauthn::{PlatformError, WebAuthn as _, WebAuthnError};
use libwebauthn::UvUpdate;
use tokio::sync::oneshot;

pub use qr::QrMatrix;

/// How long a ceremony waits when the page gives no time (or an absurd one), and the bounds of
/// what it accepts. The phone has to be found and the person has to unlock it, so a few
/// seconds is never enough.
const DEFAULT_WAIT: Duration = Duration::from_secs(180);
const SHORTEST_WAIT: Duration = Duration::from_secs(60);
const LONGEST_WAIT: Duration = Duration::from_secs(600);
/// The largest options a page may send.
pub const MAX_OPTIONS: usize = 64 * 1024;

/// What the page asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `navigator.credentials.create`: make a passkey.
    Create,
    /// `navigator.credentials.get`: sign in with one.
    Get,
}

/// A request from a page, as the browser hands it over.
#[derive(Debug, Clone)]
pub struct Request {
    pub kind: Kind,
    /// The origin of the page that asked, as the browser knows it: `https://example.com`.
    pub origin: String,
    /// `PublicKeyCredentialCreationOptionsJSON` or `PublicKeyCredentialRequestOptionsJSON`.
    pub options: String,
}

/// Where the ceremony is, for the words under the QR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The code is up and the phone has not shown itself yet.
    Waiting,
    /// The phone is near: the Bluetooth check is done or being done.
    Nearby,
    /// Reaching the phone through the tunnel server.
    Connecting,
    /// Setting up the encrypted channel with the phone.
    Authenticating,
    /// Linked: the phone is asking the person to confirm.
    Connected,
    /// The phone is waiting for a touch or a look.
    Touch,
}

/// Why a ceremony ended without a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The computer has no Bluetooth, or it is off.
    NoBluetooth,
    /// The person closed the dialog.
    Cancelled,
    /// Nobody scanned the code, or nobody confirmed, in time.
    TimedOut,
    /// The person said no on the phone.
    Declined,
    /// `rpId` is not one this site may use.
    WrongSite,
    /// The page sent something that is not valid WebAuthn.
    Invalid,
    /// The phone already has a passkey for this account.
    AlreadyRegistered,
    /// The phone has no passkey for this site.
    NoPasskey,
    /// The phone cannot do what the page asked.
    Unsupported,
    /// The link to the phone broke (the tunnel server, the network).
    Connection,
    /// Anything else.
    Other,
}

impl Reason {
    /// The name of the exception the page gets. `TypeError` is a plain `TypeError`, the others
    /// are `DOMException`s.
    pub fn dom_error(self) -> &'static str {
        match self {
            Reason::WrongSite => "SecurityError",
            Reason::Invalid => "TypeError",
            Reason::AlreadyRegistered => "InvalidStateError",
            Reason::Unsupported => "NotSupportedError",
            // The specification wants the same answer for "no passkey" and "said no", so a
            // page cannot tell which accounts a person has.
            Reason::NoBluetooth
            | Reason::Cancelled
            | Reason::TimedOut
            | Reason::Declined
            | Reason::NoPasskey
            | Reason::Connection
            | Reason::Other => "NotAllowedError",
        }
    }
}

/// A ceremony that ended without a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub reason: Reason,
    /// For logs and for the page's exception; not shown to the person as is.
    pub detail: String,
}

impl Failure {
    fn new(reason: Reason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

/// What a ceremony reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The text to draw as a QR code. Comes first.
    Qr(String),
    Stage(Stage),
    /// The credential, as `RegistrationResponseJSON` or `AuthenticationResponseJSON`.
    Done(String),
    Failed(Failure),
}

/// A ceremony in progress. Dropping it cancels it.
pub struct Ceremony {
    events: UnboundedReceiver<Event>,
    cancel: Canceller,
}

/// Stops a ceremony from somewhere else than the loop that reads its events.
#[derive(Clone)]
pub struct Canceller(Arc<Mutex<Option<oneshot::Sender<()>>>>);

impl Canceller {
    /// The phone is not waited for any more; the ceremony ends with [`Reason::Cancelled`].
    pub fn cancel(&self) {
        let sender = self.0.lock().ok().and_then(|mut g| g.take());
        if let Some(c) = sender {
            let _ = c.send(());
        }
    }
}

impl Ceremony {
    /// The next thing that happened; `None` once the ceremony is over and everything was read.
    pub async fn next(&mut self) -> Option<Event> {
        self.events.next().await
    }

    /// Stops the ceremony: the phone is not waited for any more.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// A handle that stops the ceremony while something else reads its events.
    pub fn canceller(&self) -> Canceller {
        self.cancel.clone()
    }
}

impl Drop for Ceremony {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Starts a ceremony on a thread of its own.
pub fn start(request: Request) -> Ceremony {
    let (tx, rx) = unbounded();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("passkeys".into())
        .spawn({
            let tx = tx.clone();
            move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.unbounded_send(Event::Failed(Failure::new(
                            Reason::Other,
                            format!("no runtime: {e}"),
                        )));
                        return;
                    }
                };
                let last = runtime.block_on(async {
                    tokio::select! {
                        _ = cancel_rx => Event::Failed(Failure::new(Reason::Cancelled, "cancelled")),
                        r = tokio::time::timeout(wait_of(&request.options), run(&request, &tx)) => match r {
                            Ok(Ok(json)) => Event::Done(json),
                            Ok(Err(f)) => Event::Failed(f),
                            Err(_) => Event::Failed(Failure::new(Reason::TimedOut, "timed out")),
                        },
                    }
                });
                let _ = tx.unbounded_send(last);
                // Whatever is still running (the tunnel, the Bluetooth scan) stops now.
                runtime.shutdown_background();
            }
        });
    if spawned.is_err() {
        let _ = tx.unbounded_send(Event::Failed(Failure::new(Reason::Other, "no thread")));
    }
    Ceremony {
        events: rx,
        cancel: Canceller(Arc::new(Mutex::new(Some(cancel_tx)))),
    }
}

/// How long to wait: what the page asked for, within bounds.
pub fn wait_of(options: &str) -> Duration {
    serde_json::from_str::<serde_json::Value>(options)
        .ok()
        .and_then(|v| v["timeout"].as_u64())
        .map_or(DEFAULT_WAIT, |ms| {
            Duration::from_millis(ms).clamp(SHORTEST_WAIT, LONGEST_WAIT)
        })
}

/// Whether a page at `origin` may use WebAuthn: a secure context, so https, or http on the
/// computer itself (for developers).
pub fn is_secure_origin(origin: &str) -> bool {
    let Ok(u) = url::Url::parse(origin) else {
        return false;
    };
    if !u.username().is_empty() || u.password().is_some() {
        return false;
    }
    match u.scheme() {
        "https" => u.host_str().is_some(),
        "http" => {
            matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
                || u.host_str().is_some_and(|h| h.ends_with(".localhost"))
        }
        _ => false,
    }
}

/// The request checked and ready to be sent to a phone.
enum Prepared {
    Create(Box<MakeCredentialRequest>),
    Get(Box<GetAssertionRequest>),
}

async fn prepare(request: &Request) -> Result<Prepared, Failure> {
    if request.options.len() > MAX_OPTIONS {
        return Err(Failure::new(Reason::Invalid, "options too large"));
    }
    if !is_secure_origin(&request.origin) {
        return Err(Failure::new(Reason::WrongSite, "not a secure origin"));
    }
    let origin = RequestOrigin::try_from(request.origin.as_str())
        .map_err(|e| Failure::new(Reason::WrongSite, e.to_string()))?;
    let list = psl_list::BuiltInList;
    let settings = RequestSettings {
        origin: OriginValidation::Validate {
            public_suffix_list: &list,
            related_origins: RelatedOrigins::Disabled,
        },
    };
    match request.kind {
        Kind::Create => MakeCredentialRequest::prepare(&origin, &request.options, &settings)
            .await
            .map(|r| Prepared::Create(Box::new(r)))
            .map_err(|e| {
                use libwebauthn::ops::webauthn::MakeCredentialPrepareError as E;
                let reason = match &e {
                    E::EncodingError(_) => Reason::Invalid,
                    E::InvalidRelyingPartyId(_) | E::MismatchingRelyingPartyId(..) => {
                        Reason::WrongSite
                    }
                    E::InvalidAppId(_) => Reason::WrongSite,
                };
                Failure::new(reason, e.to_string())
            }),
        Kind::Get => GetAssertionRequest::prepare(&origin, &request.options, &settings)
            .await
            .map(|r| Prepared::Get(Box::new(r)))
            .map_err(|e| {
                use libwebauthn::ops::webauthn::GetAssertionPrepareError as E;
                let reason = match &e {
                    E::EncodingError(_) | E::UnexpectedLengthError(..) => Reason::Invalid,
                    E::NotSupported(_) => Reason::Unsupported,
                    E::InvalidRelyingPartyId(_)
                    | E::MismatchingRelyingPartyId(..)
                    | E::InvalidAppId(_) => Reason::WrongSite,
                };
                Failure::new(reason, e.to_string())
            }),
    }
}

async fn run(request: &Request, tx: &UnboundedSender<Event>) -> Result<String, Failure> {
    let prepared = prepare(request).await?;
    if !libwebauthn::transport::cable::is_available().await {
        return Err(Failure::new(Reason::NoBluetooth, "no Bluetooth adapter"));
    }
    let hint = match request.kind {
        Kind::Create => QrCodeOperationHint::MakeCredential,
        Kind::Get => QrCodeOperationHint::GetAssertionRequest,
    };
    // Only the tunnel server is advertised: phones that do not know the direct Bluetooth
    // channel of newer FIDO versions would refuse a code that offers it.
    let mut device = CableQrCodeDevice::new_transient(hint, CableTransports::CloudAssistedOnly)
        .map_err(|e| Failure::new(Reason::Other, e.to_string()))?;
    let _ = tx.unbounded_send(Event::Qr(device.qr_code.to_string()));
    let _ = tx.unbounded_send(Event::Stage(Stage::Waiting));

    let mut channel = device
        .channel(ChannelSettings::default())
        .await
        .map_err(failure_of)?;
    let mut updates = channel.get_ux_update_receiver();
    let stages = tx.clone();
    tokio::spawn(async move {
        while let Ok(update) = updates.recv().await {
            match update {
                CableUxUpdate::CableUpdate(c) => {
                    let stage = match c {
                        CableUpdate::ProximityCheck => Stage::Nearby,
                        CableUpdate::Connecting => Stage::Connecting,
                        CableUpdate::Authenticating => Stage::Authenticating,
                        CableUpdate::Connected => Stage::Connected,
                        CableUpdate::Error(_) => continue,
                    };
                    let _ = stages.unbounded_send(Event::Stage(stage));
                }
                CableUxUpdate::UvUpdate(UvUpdate::PresenceRequired) => {
                    let _ = stages.unbounded_send(Event::Stage(Stage::Touch));
                }
                // A phone checks the person itself; a PIN prompt here would be a computer
                // asking for a secret the page could imitate. Ask for none.
                CableUxUpdate::UvUpdate(UvUpdate::PinRequired(u)) => u.cancel(),
                CableUxUpdate::UvUpdate(UvUpdate::PinNotSet(u)) => u.cancel(),
                CableUxUpdate::UvUpdate(UvUpdate::UvRetry { .. }) => {}
            }
        }
    });

    match prepared {
        Prepared::Create(req) => {
            let response = channel
                .webauthn_make_credential(&req)
                .await
                .map_err(failure_of)?;
            response
                .to_json_string(&req, JsonFormat::Minified)
                .map_err(|e| Failure::new(Reason::Other, e.to_string()))
        }
        Prepared::Get(req) => {
            let response = channel
                .webauthn_get_assertion(&req)
                .await
                .map_err(failure_of)?;
            // A phone that holds several passkeys for the site has the person choose one and
            // answers with that one.
            let assertion = response
                .assertions
                .first()
                .ok_or_else(|| Failure::new(Reason::NoPasskey, "no assertion"))?;
            assertion
                .to_json_string(&req, JsonFormat::Minified)
                .map_err(|e| Failure::new(Reason::Other, e.to_string()))
        }
    }
}

fn failure_of(e: WebAuthnError<CableError>) -> Failure {
    let detail = e.to_string();
    let reason = match &e {
        WebAuthnError::Ctap(c) => reason_of_ctap(c),
        WebAuthnError::Platform(PlatformError::Cancelled) => Reason::Cancelled,
        WebAuthnError::Platform(PlatformError::NotSupported | PlatformError::NoUvAvailable) => {
            Reason::Unsupported
        }
        WebAuthnError::Platform(PlatformError::SyntaxError) => Reason::Invalid,
        WebAuthnError::Platform(_) => Reason::Other,
        WebAuthnError::Transport(CableError::Timeout) => Reason::TimedOut,
        WebAuthnError::Transport(_) => Reason::Connection,
        _ => Reason::Other,
    };
    Failure::new(reason, detail)
}

fn reason_of_ctap(e: &CtapError) -> Reason {
    match e {
        CtapError::CredentialExcluded => Reason::AlreadyRegistered,
        CtapError::NoCredentials => Reason::NoPasskey,
        CtapError::OperationDenied | CtapError::NotAllowed | CtapError::KeepAliveCancel => {
            Reason::Declined
        }
        CtapError::UserActionTimeout | CtapError::ActionTimeout | CtapError::Timeout => {
            Reason::TimedOut
        }
        CtapError::UnsupportedAlgorithm
        | CtapError::UnsupportedExtension
        | CtapError::UnsupportedOption => Reason::Unsupported,
        CtapError::InvalidParameter | CtapError::MissingParameter | CtapError::InvalidCbor => {
            Reason::Invalid
        }
        _ => Reason::Other,
    }
}

#[cfg(test)]
mod tests;
