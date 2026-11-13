//! The Content-Security-Policy of the browser's own pages (new tab, preferences, welcome, news).
//!
//! These pages are drawn by Brújula from text that is partly somebody else's (a page title in the
//! history, the name of an extension, an address in a list) and they can ask the browser to
//! change its settings. Every piece of that text is escaped, and this is the second wall behind
//! it: a page of the browser runs only the script the browser wrote into it (named by its hash),
//! loads nothing from the network and has no frames, forms, plug-ins or `<base>`. Text that
//! slipped through the escaping would be shown, not run.

use base64::Engine;
use sha2::{Digest, Sha256};

/// The `<meta>` that sets the policy. `script` is the text between the `<script>` tags of the
/// page, exactly, or `None` for a page that has no script at all. It goes first in `<head>`,
/// before anything the policy is to govern.
pub fn meta(script: Option<&str>) -> String {
    let script_src = match script {
        Some(text) => format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(text.as_bytes()))
        ),
        None => "'none'".to_owned(),
    };
    format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src {script_src}; style-src 'unsafe-inline'; font-src brujula-font:; \
         img-src data:; base-uri 'none'; form-action 'none'; frame-src 'none'; \
         object-src 'none'\">"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_without_script_runs_none() {
        let m = meta(None);
        assert!(m.contains("script-src 'none'"));
        assert!(m.contains("default-src 'none'"));
    }

    #[test]
    fn a_page_with_script_runs_only_that_script() {
        let m = meta(Some("alert(1)"));
        // sha256("alert(1)"), base64.
        assert!(m.contains("'sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI='"));
        assert!(!m.contains("script-src 'unsafe-inline'") && !m.contains("unsafe-eval"));
        assert_ne!(meta(Some("alert(2)")), m);
    }

    #[test]
    fn the_policy_has_no_way_out_to_the_network_or_to_other_pages() {
        let m = meta(None);
        for needed in [
            "base-uri 'none'",
            "form-action 'none'",
            "frame-src 'none'",
            "object-src 'none'",
            "img-src data:",
            "font-src brujula-font:",
        ] {
            assert!(m.contains(needed), "{needed}");
        }
        assert!(!m.contains("http:") && !m.contains("https:"));
    }
}
