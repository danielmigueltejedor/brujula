//! The password vault: logins kept in one file, encrypted with a key made from a master
//! password. Nothing in the file is readable without it, not even which sites are saved.
//!
//! - The key comes from Argon2id (64 MiB, 3 passes) over the master password and a random salt.
//! - The text is sealed with XChaCha20-Poly1305 under a fresh random nonce each time it is
//!   written, so a file never repeats a nonce and a changed byte fails the open.
//! - Key, master password and every password are wiped from memory when they are dropped.
//! - A login is saved and filled only for one origin: `https://host` (or `http://` for a local
//!   address), never for a sibling subdomain or another scheme.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{AeadCore, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

const FORMAT: u32 = 1;
/// Argon2id cost: memory in KiB, passes, lanes.
const MEMORY_KIB: u32 = 64 * 1024;
const PASSES: u32 = 3;
const LANES: u32 = 1;
/// Ceilings for the cost read from a file, so a crafted file cannot make opening it take
/// minutes or gigabytes.
const MAX_MEMORY_KIB: u32 = 1024 * 1024;
const MAX_PASSES: u32 = 10;
const MAX_LOGINS: usize = 10_000;
const MAX_FIELD: usize = 1024;
pub const MIN_MASTER: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// Wrong master password, or the file was changed.
    Unlock,
    /// Not a vault file, or one from a newer version.
    Format,
    /// The master password is too short.
    Weak,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unlock => "contraseña maestra incorrecta",
            Self::Format => "el archivo de contraseñas no es válido",
            Self::Weak => "la contraseña maestra debe tener al menos 8 caracteres",
        })
    }
}

/// What a saved login holds. The password is wiped when the login is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Login {
    #[zeroize(skip)]
    pub id: u64,
    #[zeroize(skip)]
    pub origin: String,
    pub username: String,
    pub password: String,
    #[zeroize(skip)]
    pub created: u64,
    #[zeroize(skip)]
    pub modified: u64,
}

/// What is sealed in the file.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Contents {
    logins: Vec<Login>,
    /// Sites the user asked never to be offered a save for.
    never: Vec<String>,
    next_id: u64,
}

#[derive(Serialize, Deserialize)]
struct File {
    v: u32,
    memory: u32,
    passes: u32,
    lanes: u32,
    salt: String,
    nonce: String,
    data: String,
}

/// An unlocked vault: the logins, and the key to write them back.
pub struct Vault {
    contents: Contents,
    key: Zeroizing<[u8; 32]>,
    memory: u32,
    passes: u32,
    lanes: u32,
    salt: [u8; 16],
}

fn derive(
    master: &str,
    salt: &[u8],
    memory: u32,
    passes: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    if memory > MAX_MEMORY_KIB || passes > MAX_PASSES || lanes == 0 || lanes > 8 {
        return Err(VaultError::Format);
    }
    let params = Params::new(memory, passes, lanes, Some(32)).map_err(|_| VaultError::Format)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(master.as_bytes(), salt, &mut *key)
        .map_err(|_| VaultError::Format)?;
    Ok(key)
}

fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD)
        .collect()
}

/// `https://example.com` of a page address, or `None` when logins are not kept for it (not a web
/// page, or `http://` for anything but a local address, where a password would cross in clear).
pub fn origin_of(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let local = host == "localhost" || host == "127.0.0.1" || host == "[::1]";
    match parsed.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return None,
    }
    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
    Some(format!("{}://{host}{port}", parsed.scheme()))
}

impl Vault {
    /// A new, empty vault under `master`.
    pub fn create(master: &str) -> Result<Self, VaultError> {
        Self::create_with(master, MEMORY_KIB, PASSES, LANES)
    }

    fn create_with(master: &str, memory: u32, passes: u32, lanes: u32) -> Result<Self, VaultError> {
        if master.chars().count() < MIN_MASTER {
            return Err(VaultError::Weak);
        }
        let mut salt = [0u8; 16];
        chacha20poly1305::aead::rand_core::RngCore::fill_bytes(&mut OsRng, &mut salt);
        let key = derive(master, &salt, memory, passes, lanes)?;
        Ok(Self {
            contents: Contents::default(),
            key,
            memory,
            passes,
            lanes,
            salt,
        })
    }

    /// Opens a vault file with its master password.
    pub fn open(text: &str, master: &str) -> Result<Self, VaultError> {
        let file: File = serde_json::from_str(text).map_err(|_| VaultError::Format)?;
        if file.v != FORMAT {
            return Err(VaultError::Format);
        }
        let salt: [u8; 16] = B64
            .decode(&file.salt)
            .ok()
            .and_then(|s| s.try_into().ok())
            .ok_or(VaultError::Format)?;
        let nonce = B64.decode(&file.nonce).map_err(|_| VaultError::Format)?;
        if nonce.len() != 24 {
            return Err(VaultError::Format);
        }
        let data = B64.decode(&file.data).map_err(|_| VaultError::Format)?;
        let key = derive(master, &salt, file.memory, file.passes, file.lanes)?;
        let cipher = XChaCha20Poly1305::new((&*key).into());
        let plain = Zeroizing::new(
            cipher
                .decrypt(XNonce::from_slice(&nonce), data.as_ref())
                .map_err(|_| VaultError::Unlock)?,
        );
        let mut contents: Contents =
            serde_json::from_slice(&plain).map_err(|_| VaultError::Format)?;
        contents.logins.truncate(MAX_LOGINS);
        contents
            .logins
            .retain(|l| origin_of(&l.origin).as_deref() == Some(l.origin.as_str()));
        Ok(Self {
            contents,
            key,
            memory: file.memory,
            passes: file.passes,
            lanes: file.lanes,
            salt,
        })
    }

    /// The text to write to the file: the vault sealed under a fresh nonce.
    pub fn seal(&self) -> String {
        let cipher = XChaCha20Poly1305::new((&*self.key).into());
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let plain = Zeroizing::new(serde_json::to_vec(&self.contents).unwrap_or_default());
        let data = cipher.encrypt(&nonce, plain.as_ref()).unwrap_or_default();
        serde_json::to_string(&File {
            v: FORMAT,
            memory: self.memory,
            passes: self.passes,
            lanes: self.lanes,
            salt: B64.encode(self.salt),
            nonce: B64.encode(nonce),
            data: B64.encode(data),
        })
        .unwrap_or_default()
    }

    /// Changes the master password: a new salt and key, the same logins.
    pub fn change_master(&mut self, master: &str) -> Result<(), VaultError> {
        if master.chars().count() < MIN_MASTER {
            return Err(VaultError::Weak);
        }
        let mut salt = [0u8; 16];
        chacha20poly1305::aead::rand_core::RngCore::fill_bytes(&mut OsRng, &mut salt);
        self.key = derive(master, &salt, self.memory, self.passes, self.lanes)?;
        self.salt = salt;
        Ok(())
    }

    /// Checks `master` against this vault without touching it.
    pub fn is_master(&self, master: &str) -> bool {
        derive(master, &self.salt, self.memory, self.passes, self.lanes)
            .is_ok_and(|k| *k == *self.key)
    }

    pub fn logins(&self) -> &[Login] {
        &self.contents.logins
    }

    pub fn never(&self) -> &[String] {
        &self.contents.never
    }

    /// The logins saved for the origin of `url`, the latest used first.
    pub fn for_page(&self, url: &str) -> Vec<&Login> {
        let Some(origin) = origin_of(url) else {
            return Vec::new();
        };
        let mut found: Vec<&Login> = self
            .contents
            .logins
            .iter()
            .filter(|l| l.origin == origin)
            .collect();
        found.sort_by_key(|l| std::cmp::Reverse(l.modified));
        found
    }

    /// What saving `username`/`password` for `url` would do.
    pub fn check(&self, url: &str, username: &str, password: &str) -> Offer {
        let Some(origin) = origin_of(url) else {
            return Offer::Nothing;
        };
        if password.is_empty() || self.contents.never.contains(&origin) {
            return Offer::Nothing;
        }
        match self
            .contents
            .logins
            .iter()
            .find(|l| l.origin == origin && l.username == username)
        {
            Some(l) if l.password == password => Offer::Nothing,
            Some(l) => Offer::Update(l.id),
            None => Offer::Save,
        }
    }

    /// Saves a login, or replaces the password of the one with the same user on that origin.
    /// Returns the id, or `None` for what is not kept.
    pub fn put(&mut self, url: &str, username: &str, password: &str, now: u64) -> Option<u64> {
        let origin = origin_of(url)?;
        if password.is_empty() || password.len() > MAX_FIELD {
            return None;
        }
        let username = clean(username);
        if let Some(l) = self
            .contents
            .logins
            .iter_mut()
            .find(|l| l.origin == origin && l.username == username)
        {
            l.password = password.to_owned();
            l.modified = now;
            return Some(l.id);
        }
        if self.contents.logins.len() >= MAX_LOGINS {
            return None;
        }
        self.contents.next_id += 1;
        let id = self.contents.next_id;
        self.contents.logins.insert(
            0,
            Login {
                id,
                origin,
                username,
                password: password.to_owned(),
                created: now,
                modified: now,
            },
        );
        Some(id)
    }

    pub fn remove(&mut self, id: u64) {
        self.contents.logins.retain(|l| l.id != id);
    }

    pub fn find(&self, id: u64) -> Option<&Login> {
        self.contents.logins.iter().find(|l| l.id == id)
    }

    /// Never offer to save for the origin of `url`.
    pub fn never_for(&mut self, url: &str) {
        if let Some(origin) = origin_of(url) {
            if !self.contents.never.contains(&origin) {
                self.contents.never.push(origin);
            }
        }
    }

    pub fn forget_never(&mut self, origin: &str) {
        self.contents.never.retain(|o| o != origin);
    }
}

/// What the browser should offer after a page sent a login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    Nothing,
    Save,
    Update(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Argon2 at full cost takes a second; tests use a small one.
    fn vault() -> Vault {
        Vault::create_with("correct horse", 64, 1, 1).unwrap()
    }

    #[test]
    fn a_vault_opens_only_with_its_master_password() {
        let mut v = vault();
        v.put("https://example.com/login", "anita.lopez", "s3cret", 1);
        let text = v.seal();
        // Needles long enough that base64 of random bytes cannot contain them by chance.
        assert!(
            !text.contains("example.com")
                && !text.contains("s3cret")
                && !text.contains("anita.lopez")
        );
        let again = Vault::open(&text, "correct horse").unwrap();
        assert_eq!(again.logins().len(), 1);
        assert_eq!(again.logins()[0].password, "s3cret");
        assert_eq!(
            Vault::open(&text, "wrong horse").err(),
            Some(VaultError::Unlock)
        );
    }

    #[test]
    fn a_changed_file_does_not_open() {
        let mut v = vault();
        v.put("https://example.com/", "ana", "pw", 1);
        let text = v.seal();
        let mut file: File = serde_json::from_str(&text).unwrap();
        let mut data = B64.decode(&file.data).unwrap();
        data[3] ^= 1;
        file.data = B64.encode(data);
        let broken = serde_json::to_string(&file).unwrap();
        assert_eq!(
            Vault::open(&broken, "correct horse").err(),
            Some(VaultError::Unlock)
        );
        assert_eq!(Vault::open("not json", "x").err(), Some(VaultError::Format));
        assert_eq!(
            Vault::open(&text.replace("\"v\":1", "\"v\":2"), "correct horse").err(),
            Some(VaultError::Format)
        );
    }

    #[test]
    fn every_seal_uses_a_new_nonce() {
        let v = vault();
        let (a, b): (File, File) = (
            serde_json::from_str(&v.seal()).unwrap(),
            serde_json::from_str(&v.seal()).unwrap(),
        );
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.data, b.data);
    }

    #[test]
    fn a_crafted_cost_is_refused_before_any_work() {
        let v = vault();
        let mut file: File = serde_json::from_str(&v.seal()).unwrap();
        file.memory = u32::MAX;
        let text = serde_json::to_string(&file).unwrap();
        assert_eq!(
            Vault::open(&text, "correct horse").err(),
            Some(VaultError::Format)
        );
    }

    #[test]
    fn short_master_passwords_are_refused() {
        assert_eq!(
            Vault::create_with("short", 64, 1, 1).err(),
            Some(VaultError::Weak)
        );
        let mut v = vault();
        assert_eq!(v.change_master("1234567").err(), Some(VaultError::Weak));
    }

    #[test]
    fn changing_the_master_keeps_the_logins() {
        let mut v = vault();
        v.put("https://a.example/", "ana", "pw", 1);
        v.change_master("another long one").unwrap();
        let text = v.seal();
        assert!(Vault::open(&text, "correct horse").is_err());
        assert_eq!(
            Vault::open(&text, "another long one")
                .unwrap()
                .logins()
                .len(),
            1
        );
        assert!(v.is_master("another long one") && !v.is_master("correct horse"));
    }

    #[test]
    fn logins_belong_to_one_origin() {
        let mut v = vault();
        v.put("https://example.com/a", "ana", "pw1", 1);
        assert_eq!(v.for_page("https://example.com/other?x=1").len(), 1);
        assert!(v.for_page("https://sub.example.com/").is_empty());
        assert!(v.for_page("http://example.com/").is_empty());
        assert!(v.for_page("https://example.com:8443/").is_empty());
        assert!(v.for_page("https://example.com.evil.test/").is_empty());
    }

    #[test]
    fn only_secure_or_local_pages_keep_logins() {
        assert_eq!(
            origin_of("https://Example.com/x"),
            Some("https://example.com".into())
        );
        assert_eq!(
            origin_of("http://localhost:3000/"),
            Some("http://localhost:3000".into())
        );
        assert_eq!(origin_of("http://example.com/"), None);
        assert_eq!(origin_of("file:///etc/passwd"), None);
        assert_eq!(origin_of("javascript:alert(1)"), None);
        let mut v = vault();
        assert_eq!(v.put("http://example.com/", "a", "b", 1), None);
    }

    #[test]
    fn saving_the_same_user_updates_the_password() {
        let mut v = vault();
        assert_eq!(v.check("https://e.example/", "ana", "one"), Offer::Save);
        let id = v.put("https://e.example/", "ana", "one", 1).unwrap();
        assert_eq!(v.check("https://e.example/", "ana", "one"), Offer::Nothing);
        assert_eq!(
            v.check("https://e.example/", "ana", "two"),
            Offer::Update(id)
        );
        assert_eq!(v.put("https://e.example/", "ana", "two", 2), Some(id));
        assert_eq!(v.logins().len(), 1);
        assert_eq!(v.find(id).unwrap().password, "two");
        assert_eq!(v.check("https://e.example/", "ben", "x"), Offer::Save);
        assert_eq!(v.check("https://e.example/", "ana", ""), Offer::Nothing);
    }

    #[test]
    fn a_site_can_be_marked_never() {
        let mut v = vault();
        v.never_for("https://bank.example/login");
        assert_eq!(v.check("https://bank.example/", "a", "b"), Offer::Nothing);
        v.forget_never("https://bank.example");
        assert_eq!(v.check("https://bank.example/", "a", "b"), Offer::Save);
    }

    #[test]
    fn the_latest_login_comes_first_and_removal_works() {
        let mut v = vault();
        let a = v.put("https://e.example/", "ana", "1", 1).unwrap();
        let b = v.put("https://e.example/", "ben", "2", 5).unwrap();
        assert_eq!(v.for_page("https://e.example/")[0].id, b);
        v.remove(b);
        assert_eq!(v.for_page("https://e.example/")[0].id, a);
        assert!(v.find(b).is_none());
    }

    #[test]
    fn control_characters_are_cleaned_from_the_user_name() {
        let mut v = vault();
        let id = v.put("https://e.example/", "an\na\u{0}", "pw", 1).unwrap();
        assert_eq!(v.find(id).unwrap().username, "ana");
    }
}
