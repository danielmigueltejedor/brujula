//! The public suffix list that decides which `rpId` a site may use: `example.com` may ask for
//! `example.com` but not for `com`, and `a.github.io` may not ask for `github.io`.
//!
//! The list comes with the `psl` crate, so Brújula does not depend on a system package for it.

use libwebauthn::ops::webauthn::PublicSuffixList;

pub struct BuiltInList;

impl PublicSuffixList for BuiltInList {
    fn public_suffix(&self, host: &str) -> Option<String> {
        psl::suffix_str(&host.to_ascii_lowercase()).map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_plain_and_private_suffixes() {
        let l = BuiltInList;
        assert_eq!(l.public_suffix("www.example.com").as_deref(), Some("com"));
        assert_eq!(l.public_suffix("bbc.co.uk").as_deref(), Some("co.uk"));
        assert_eq!(
            l.public_suffix("someone.github.io").as_deref(),
            Some("github.io")
        );
    }

    #[test]
    fn registrable_domain_is_one_label_over_the_suffix() {
        let l = BuiltInList;
        assert_eq!(
            l.registrable_domain("login.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(l.registrable_domain("co.uk"), None);
        assert_eq!(
            l.registrable_domain("a.b.github.io").as_deref(),
            Some("b.github.io")
        );
    }
}
