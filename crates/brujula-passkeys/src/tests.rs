use super::*;

const CREATE: &str = r#"{
    "rp": {"id": "example.com", "name": "Example"},
    "user": {"id": "dXNlcjE", "name": "ana", "displayName": "Ana"},
    "challenge": "MTIzNDU2NzgxMjM0NTY3ODEyMzQ1Njc4MTIzNDU2Nzg",
    "pubKeyCredParams": [{"type": "public-key", "alg": -7}],
    "timeout": 120000,
    "authenticatorSelection": {"residentKey": "preferred", "userVerification": "preferred"},
    "attestation": "none"
}"#;

const GET: &str = r#"{
    "challenge": "MTIzNDU2NzgxMjM0NTY3ODEyMzQ1Njc4MTIzNDU2Nzg",
    "rpId": "example.com",
    "userVerification": "preferred",
    "timeout": 60000
}"#;

fn request(kind: Kind, origin: &str, options: &str) -> Request {
    Request {
        kind,
        origin: origin.to_owned(),
        options: options.to_owned(),
    }
}

fn failed(r: Result<Prepared, Failure>) -> Failure {
    match r {
        Ok(_) => panic!("a request that should have been refused went through"),
        Err(f) => f,
    }
}

#[tokio::test]
async fn a_site_may_use_its_own_domain() {
    assert!(
        prepare(&request(Kind::Create, "https://example.com", CREATE))
            .await
            .is_ok()
    );
    assert!(prepare(&request(Kind::Get, "https://example.com", GET))
        .await
        .is_ok());
}

#[tokio::test]
async fn a_subdomain_may_use_the_registrable_domain() {
    assert!(
        prepare(&request(Kind::Create, "https://login.example.com", CREATE))
            .await
            .is_ok()
    );
    assert!(
        prepare(&request(Kind::Get, "https://login.example.com:8443", GET))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn another_site_may_not_use_the_domain() {
    let f = failed(prepare(&request(Kind::Create, "https://evil.example.org", CREATE)).await);
    assert_eq!(f.reason, Reason::WrongSite);
    assert_eq!(f.reason.dom_error(), "SecurityError");
    let f = failed(prepare(&request(Kind::Get, "https://evil.example.org", GET)).await);
    assert_eq!(f.reason, Reason::WrongSite);
}

#[tokio::test]
async fn a_look_alike_domain_is_another_site() {
    // «notexample.com» ends with «example.com» but is not a subdomain of it.
    let f = failed(prepare(&request(Kind::Get, "https://notexample.com", GET)).await);
    assert_eq!(f.reason, Reason::WrongSite);
}

#[tokio::test]
async fn a_public_suffix_cannot_be_claimed() {
    let options = GET.replace("example.com", "github.io");
    let f = failed(prepare(&request(Kind::Get, "https://someone.github.io", &options)).await);
    assert_eq!(f.reason, Reason::WrongSite);
    let options = CREATE.replace("example.com", "com");
    let f = failed(prepare(&request(Kind::Create, "https://example.com", &options)).await);
    assert_eq!(f.reason, Reason::WrongSite);
}

#[tokio::test]
async fn a_page_without_a_secure_origin_is_refused_before_anything_else() {
    for origin in [
        "http://example.com",
        "ftp://example.com",
        "file:///etc/passwd",
        "about:blank",
        "https://user:pw@example.com",
        "",
    ] {
        let f = failed(prepare(&request(Kind::Get, origin, GET)).await);
        assert_eq!(f.reason, Reason::WrongSite, "{origin}");
    }
}

#[tokio::test]
async fn localhost_is_allowed_over_http() {
    let options = GET.replace("example.com", "localhost");
    assert!(
        prepare(&request(Kind::Get, "http://localhost:8000", &options))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn what_is_not_webauthn_is_invalid() {
    for options in ["", "null", "[]", "{}", "{\"challenge\": 5}", "not json"] {
        for kind in [Kind::Create, Kind::Get] {
            let f = failed(prepare(&request(kind, "https://example.com", options)).await);
            assert_eq!(f.reason, Reason::Invalid, "{options:?}");
            assert_eq!(f.reason.dom_error(), "TypeError");
        }
    }
}

#[tokio::test]
async fn options_too_large_are_refused_without_being_read() {
    let big = format!("{{\"challenge\": \"{}\"}}", "A".repeat(MAX_OPTIONS));
    let f = failed(prepare(&request(Kind::Get, "https://example.com", &big)).await);
    assert_eq!(f.reason, Reason::Invalid);
}

#[test]
fn secure_origins() {
    for ok in [
        "https://example.com",
        "https://example.com:8443",
        "http://localhost",
        "http://localhost:3000",
        "http://127.0.0.1:8080",
        "http://app.localhost",
    ] {
        assert!(is_secure_origin(ok), "{ok}");
    }
    for bad in [
        "http://example.com",
        "http://localhost.evil.com",
        "http://127.0.0.1.evil.com",
        "https://",
        "javascript:alert(1)",
        "data:text/html,hi",
        "https://a@example.com",
    ] {
        assert!(!is_secure_origin(bad), "{bad}");
    }
}

#[test]
fn the_wait_is_what_the_page_asked_within_bounds() {
    assert_eq!(wait_of(r#"{"timeout": 120000}"#), Duration::from_secs(120));
    assert_eq!(wait_of(r#"{"timeout": 1000}"#), SHORTEST_WAIT);
    assert_eq!(wait_of(r#"{"timeout": 99999999999}"#), LONGEST_WAIT);
    assert_eq!(wait_of("{}"), DEFAULT_WAIT);
    assert_eq!(wait_of("nonsense"), DEFAULT_WAIT);
    assert_eq!(wait_of(r#"{"timeout": -5}"#), DEFAULT_WAIT);
}

#[test]
fn the_ways_to_fail_become_the_errors_a_page_expects() {
    use CtapError as C;
    assert_eq!(
        reason_of_ctap(&C::CredentialExcluded),
        Reason::AlreadyRegistered
    );
    assert_eq!(Reason::AlreadyRegistered.dom_error(), "InvalidStateError");
    assert_eq!(reason_of_ctap(&C::NoCredentials), Reason::NoPasskey);
    assert_eq!(reason_of_ctap(&C::OperationDenied), Reason::Declined);
    assert_eq!(reason_of_ctap(&C::UserActionTimeout), Reason::TimedOut);
    assert_eq!(
        reason_of_ctap(&C::UnsupportedAlgorithm),
        Reason::Unsupported
    );
    assert_eq!(Reason::Unsupported.dom_error(), "NotSupportedError");
    // A page cannot tell «no passkey here» from «the person said no».
    assert_eq!(Reason::NoPasskey.dom_error(), Reason::Declined.dom_error());
    assert_eq!(Reason::Declined.dom_error(), "NotAllowedError");
}

#[test]
fn a_broken_link_is_a_connection_error_and_a_slow_one_is_a_timeout() {
    assert_eq!(
        failure_of(WebAuthnError::Transport(CableError::ConnectionLost)).reason,
        Reason::Connection
    );
    assert_eq!(
        failure_of(WebAuthnError::Transport(CableError::Timeout)).reason,
        Reason::TimedOut
    );
    assert_eq!(
        failure_of(WebAuthnError::Platform(PlatformError::Cancelled)).reason,
        Reason::Cancelled
    );
}

#[test]
fn a_ceremony_that_is_dropped_stops() {
    let mut c = start(request(Kind::Get, "https://example.com", GET));
    c.cancel();
    // Whatever it was doing (here: finding no Bluetooth on a build machine, or cancelling), it
    // ends with one last event instead of hanging.
    let last = futures_executor_block_on(async {
        let mut last = None;
        while let Some(e) = c.next().await {
            last = Some(e);
        }
        last
    });
    assert!(matches!(last, Some(Event::Failed(_))));
}

#[test]
fn no_bluetooth_is_said_and_no_code_is_shown_before_it() {
    // A machine without an adapter must fail before showing a code nobody could use.
    let mut c = start(request(Kind::Create, "https://example.com", CREATE));
    let events = futures_executor_block_on(async {
        let mut all = Vec::new();
        while let Some(e) = c.next().await {
            all.push(e);
        }
        all
    });
    let Some(Event::Failed(f)) = events.last() else {
        panic!("a ceremony ends with a failure or a credential: {events:?}");
    };
    // Build machines have no adapter; where there is one the ceremony waits for a phone until
    // the timeout, which this test does not wait for, so only this case is checked.
    if f.reason == Reason::NoBluetooth {
        assert!(!events.iter().any(|e| matches!(e, Event::Qr(_))));
    }
}

/// Runs a future to its end without bringing in an executor crate for it.
fn futures_executor_block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(f)
}
