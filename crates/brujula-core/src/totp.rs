//! Time-based one-time codes (RFC 6238), the six digits that an authenticator app shows. A
//! secret comes as the key a site shows next to its QR code (base32, with or without spaces) or
//! as the `otpauth://totp/...` address inside that QR code.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop};

const MAX_SECRET: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

/// A secret and how its codes are made. The secret is wiped when this is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct Totp {
    secret: Vec<u8>,
    #[zeroize(skip)]
    algorithm: Algorithm,
    #[zeroize(skip)]
    digits: u32,
    #[zeroize(skip)]
    period: u64,
}

/// The bytes of a base32 text (RFC 4648): upper or lower case, spaces, hyphens and `=` ignored.
fn base32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in text.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            ' ' | '-' | '=' | '\t' | '\n' | '\r' => continue,
            _ => return None,
        };
        buffer = (buffer << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).ok()?);
            buffer &= (1 << bits) - 1;
        }
    }
    (!out.is_empty() && out.len() <= MAX_SECRET).then_some(out)
}

impl Totp {
    /// Reads what the person pasted: an `otpauth://totp/...` address or the bare key.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > 2048 {
            return None;
        }
        if !text.to_ascii_lowercase().starts_with("otpauth:") {
            return Some(Self {
                secret: base32(text)?,
                algorithm: Algorithm::Sha1,
                digits: 6,
                period: 30,
            });
        }
        let url = Url::parse(text).ok()?;
        if !url.scheme().eq_ignore_ascii_case("otpauth") || url.host_str() != Some("totp") {
            return None;
        }
        let (mut secret, mut algorithm, mut digits, mut period) = (None, Algorithm::Sha1, 6, 30);
        for (k, v) in url.query_pairs() {
            match k.to_ascii_lowercase().as_str() {
                "secret" => secret = base32(&v),
                "algorithm" => {
                    algorithm = match v.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return None,
                    }
                }
                "digits" => digits = v.parse().ok().filter(|d| (6..=8).contains(d))?,
                "period" => period = v.parse().ok().filter(|p| (1..=300).contains(p))?,
                _ => {}
            }
        }
        Some(Self {
            secret: secret?,
            algorithm,
            digits,
            period,
        })
    }

    fn mac(&self, counter: u64) -> Vec<u8> {
        let message = counter.to_be_bytes();
        macro_rules! run {
            ($hash:ty) => {{
                let Ok(mut m) = Hmac::<$hash>::new_from_slice(&self.secret) else {
                    return Vec::new();
                };
                m.update(&message);
                m.finalize().into_bytes().to_vec()
            }};
        }
        match self.algorithm {
            Algorithm::Sha1 => run!(Sha1),
            Algorithm::Sha256 => run!(Sha256),
            Algorithm::Sha512 => run!(Sha512),
        }
    }

    /// The code for the moment `unix` (seconds since 1970), with its leading zeros.
    pub fn code(&self, unix: u64) -> String {
        let hash = self.mac(unix / self.period);
        let Some(&last) = hash.last() else {
            return String::new();
        };
        let at = usize::from(last & 0x0f);
        let Some(chunk) = hash.get(at..at + 4) else {
            return String::new();
        };
        let n = u32::from_be_bytes([chunk[0] & 0x7f, chunk[1], chunk[2], chunk[3]]);
        let code = u64::from(n) % 10u64.pow(self.digits);
        format!("{code:0width$}", width = self.digits as usize)
    }

    /// Seconds left before `unix`'s code stops being the current one.
    pub fn seconds_left(&self, unix: u64) -> u64 {
        self.period - unix % self.period
    }

    pub fn period(&self) -> u64 {
        self.period
    }
}

/// A code split in two halves to be read at a glance: `123 456`.
pub fn grouped(code: &str) -> String {
    let half = code.len().div_ceil(2);
    match (code.get(..half), code.get(half..)) {
        (Some(a), Some(b)) if !b.is_empty() => format!("{a} {b}"),
        _ => code.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238, appendix B: the key is "12345678901234567890" for SHA-1, the same text
    // stretched to 32 bytes for SHA-256 and to 64 for SHA-512; the codes have eight digits.
    fn rfc(algorithm: Algorithm, key: &str) -> Totp {
        Totp {
            secret: key.as_bytes().to_vec(),
            algorithm,
            digits: 8,
            period: 30,
        }
    }

    #[test]
    fn the_codes_of_rfc_6238_come_out() {
        let sha1 = rfc(Algorithm::Sha1, "12345678901234567890");
        let sha256 = rfc(Algorithm::Sha256, "12345678901234567890123456789012");
        let sha512 = rfc(
            Algorithm::Sha512,
            "1234567890123456789012345678901234567890123456789012345678901234",
        );
        for (t, a, b, c) in [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
        ] {
            assert_eq!(sha1.code(t), a, "sha1 {t}");
            assert_eq!(sha256.code(t), b, "sha256 {t}");
            assert_eq!(sha512.code(t), c, "sha512 {t}");
        }
    }

    #[test]
    fn a_bare_key_gives_six_digits_every_thirty_seconds() {
        // "12345678901234567890" in base32, as a site would show it, with spaces and lowercase.
        let t = Totp::parse("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap();
        assert_eq!(t.code(59), "287082");
        assert_eq!(t.code(1_111_111_109), "081804");
        assert_eq!(t.seconds_left(59), 1);
        assert_eq!(t.seconds_left(60), 30);
    }

    #[test]
    fn an_otpauth_address_carries_its_own_settings() {
        let t = Totp::parse(
            "otpauth://totp/Example:ana@example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example&digits=8&period=30",
        )
        .unwrap();
        assert_eq!(t.code(59), "94287082");
        let t = Totp::parse(
            "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&algorithm=SHA256&period=60",
        )
        .unwrap();
        assert_eq!(t.period(), 60);
        assert_eq!(t.seconds_left(61), 59);
    }

    #[test]
    fn what_is_not_a_secret_is_refused() {
        for bad in [
            "",
            "   ",
            "hello world!",
            "0189",
            "otpauth://hotp/x?secret=GEZDGNBV&counter=1",
            "otpauth://totp/x",
            "otpauth://totp/x?secret=GEZDGNBV&digits=12",
            "otpauth://totp/x?secret=GEZDGNBV&period=0",
            "otpauth://totp/x?secret=GEZDGNBV&algorithm=MD5",
            "https://example.com/?secret=GEZDGNBV",
        ] {
            assert!(Totp::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn codes_are_grouped_to_be_read() {
        assert_eq!(grouped("123456"), "123 456");
        assert_eq!(grouped("12345678"), "1234 5678");
        assert_eq!(grouped("1234567"), "1234 567");
    }
}
