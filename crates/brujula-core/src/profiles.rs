//! Profiles: one per person who uses the browser on the same computer. Each has a name, a
//! picture and, if they want one, a password that is asked when the profile is chosen.
//!
//! The password keeps someone else from opening your profile from the chooser. It does **not**
//! encrypt the profile's files on disk: anyone with access to the account's files can read them.
//! (The saved logins have their own encryption, with the master password of the vault.)

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{rand_core::RngCore, OsRng};
use serde::{Deserialize, Serialize};

use crate::avatars;

/// The id of the first profile: it keeps the files the browser had before profiles existed.
pub const DEFAULT_ID: &str = "default";
pub const MAX_PROFILES: usize = 12;
const MAX_NAME: usize = 24;
pub const MIN_PASSWORD: usize = 4;
const MAX_PASSWORD: usize = 200;
/// Argon2id cost of the profile lock: memory in KiB and passes. Light, because it is asked
/// every time the browser opens; the ceilings keep a crafted file from making it slow.
const MEMORY_KIB: u32 = 19 * 1024;
const PASSES: u32 = 2;
const MAX_MEMORY_KIB: u32 = 256 * 1024;
const MAX_PASSES: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    salt: String,
    hash: String,
    memory: u32,
    passes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub avatar: String,
    #[serde(default)]
    pub lock: Option<Lock>,
}

impl Profile {
    pub fn has_lock(&self) -> bool {
        self.lock.is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    list: Vec<Profile>,
}

/// An id is a short name for a folder: lowercase letters, digits and hyphens.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 24
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn derive(password: &str, salt: &[u8], memory: u32, passes: u32) -> Option<[u8; 32]> {
    if memory > MAX_MEMORY_KIB || passes > MAX_PASSES || passes == 0 {
        return None;
    }
    let params = Params::new(memory, passes, 1, Some(32)).ok()?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .ok()?;
    Some(out)
}

/// Compares without stopping at the first difference.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn make_lock(password: &str, memory: u32, passes: u32) -> Option<Lock> {
    let mut rng = OsRng;
    let salt: [u8; 16] = std::array::from_fn(|_| rng.next_u32() as u8);
    let hash = derive(password, &salt, memory, passes)?;
    Some(Lock {
        salt: B64.encode(salt),
        hash: B64.encode(hash),
        memory,
        passes,
    })
}

impl Profiles {
    pub fn from_json(text: &str) -> Self {
        let mut me: Self = serde_json::from_str(text).unwrap_or_default();
        let mut seen: Vec<String> = Vec::new();
        me.list.retain_mut(|p| {
            p.name = crate::clean_name(&p.name, MAX_NAME);
            if p.name.is_empty() {
                p.name = "Perfil".to_owned();
            }
            if !avatars::is_known(&p.avatar) {
                p.avatar = avatars::AVATARS[0].id.to_owned();
            }
            let fresh = valid_id(&p.id) && !seen.contains(&p.id);
            seen.push(p.id.clone());
            fresh
        });
        me.list.truncate(MAX_PROFILES);
        me
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn list(&self) -> &[Profile] {
        &self.list
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.list.iter().find(|p| p.id == id)
    }

    /// Makes a profile. The first one is `default`. `None` means no lock; a password shorter than
    /// `MIN_PASSWORD` or longer than the limit is refused (`None`).
    pub fn add(&mut self, name: &str, avatar: &str, password: Option<&str>) -> Option<String> {
        self.add_with(name, avatar, password, MEMORY_KIB, PASSES)
    }

    fn add_with(
        &mut self,
        name: &str,
        avatar: &str,
        password: Option<&str>,
        memory: u32,
        passes: u32,
    ) -> Option<String> {
        if self.list.len() >= MAX_PROFILES {
            return None;
        }
        let lock = match password {
            None => None,
            Some(password) if (MIN_PASSWORD..=MAX_PASSWORD).contains(&password.chars().count()) => {
                Some(make_lock(password, memory, passes)?)
            }
            Some(_) => return None,
        };
        let id = if self.list.is_empty() {
            DEFAULT_ID.to_owned()
        } else {
            (1u32..)
                .map(|n| format!("p{n}"))
                .find(|id| self.get(id).is_none())?
        };
        let mut name = crate::clean_name(name, MAX_NAME);
        if name.is_empty() {
            name = "Perfil".to_owned();
        }
        let avatar = if avatars::is_known(avatar) {
            avatar.to_owned()
        } else {
            avatars::AVATARS[0].id.to_owned()
        };
        self.list.push(Profile {
            id: id.clone(),
            name,
            avatar,
            lock,
        });
        Some(id)
    }

    /// Changes the name and the picture.
    pub fn edit(&mut self, id: &str, name: &str, avatar: &str) {
        let name = crate::clean_name(name, MAX_NAME);
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            if !name.is_empty() {
                p.name = name;
            }
            if avatars::is_known(avatar) {
                p.avatar = avatar.to_owned();
            }
        }
    }

    /// Sets, changes (`Some`) or removes (`None`) the password. False if it is refused.
    pub fn set_password(&mut self, id: &str, password: Option<&str>) -> bool {
        let lock = match password {
            None => None,
            Some(p) if (MIN_PASSWORD..=MAX_PASSWORD).contains(&p.chars().count()) => {
                match make_lock(p, MEMORY_KIB, PASSES) {
                    Some(l) => Some(l),
                    None => return false,
                }
            }
            Some(_) => return false,
        };
        match self.list.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.lock = lock;
                true
            }
            None => false,
        }
    }

    /// Whether `password` opens the profile (true when it has none).
    pub fn check(&self, id: &str, password: &str) -> bool {
        let Some(p) = self.get(id) else {
            return false;
        };
        let Some(lock) = &p.lock else {
            return true;
        };
        let (Ok(salt), Ok(want)) = (B64.decode(&lock.salt), B64.decode(&lock.hash)) else {
            return false;
        };
        derive(password, &salt, lock.memory, lock.passes).is_some_and(|got| same(&got, &want))
    }

    /// Removes a profile; the last one cannot be removed. The caller deletes its files.
    pub fn remove(&mut self, id: &str) -> bool {
        if self.list.len() <= 1 {
            return false;
        }
        let before = self.list.len();
        self.list.retain(|p| p.id != id);
        self.list.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick(p: &mut Profiles, name: &str, pw: Option<&str>) -> Option<String> {
        p.add_with(name, "moon", pw, 8, 1)
    }

    fn test_password() -> String {
        format!(
            "test-password-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn the_first_profile_is_the_default_and_the_rest_get_fresh_ids() {
        let mut p = Profiles::default();
        assert_eq!(quick(&mut p, "Ana", None).as_deref(), Some(DEFAULT_ID));
        assert_eq!(quick(&mut p, "Luis", None).as_deref(), Some("p1"));
        assert_eq!(quick(&mut p, "Eva", None).as_deref(), Some("p2"));
        p.remove("p1");
        assert_eq!(quick(&mut p, "Otro", None).as_deref(), Some("p1"));
        for id in ["default", "p1", "p2"] {
            assert!(valid_id(id));
        }
    }

    #[test]
    fn a_password_opens_only_its_profile_and_is_never_stored_as_text() {
        let mut p = Profiles::default();
        let password = test_password();
        let wrong = format!("{password}-wrong");
        let id = quick(&mut p, "Ana", Some(&password)).unwrap();
        assert!(p.check(&id, &password));
        assert!(!p.check(&id, &wrong));
        assert!(!p.check(&id, &password[..MIN_PASSWORD - 1]));
        assert!(!p.check("nadie", &password));
        assert!(!p.to_json().contains(&password));
        let open = quick(&mut p, "Luis", None).unwrap();
        assert!(p.check(&open, &password));
        assert!(!p.get(&open).unwrap().has_lock());
    }

    #[test]
    fn short_or_huge_passwords_are_refused_and_nothing_is_added() {
        let mut p = Profiles::default();
        let password = test_password();
        let short = &password[..MIN_PASSWORD - 1];
        let huge = password.repeat(MAX_PASSWORD + 1);
        assert_eq!(quick(&mut p, "Ana", Some(short)), None);
        assert_eq!(quick(&mut p, "Ana", Some(&huge)), None);
        assert!(p.is_empty());
        let id = quick(&mut p, "Ana", None).unwrap();
        assert!(!p.set_password(&id, Some(short)));
        assert!(p.set_password(&id, Some(&password)));
        assert!(p.get(&id).unwrap().has_lock());
        assert!(p.set_password(&id, None));
        assert!(!p.get(&id).unwrap().has_lock());
    }

    #[test]
    fn names_and_pictures_are_cleaned_and_the_number_of_profiles_is_limited() {
        let mut p = Profiles::default();
        let id = p
            .add_with("  \u{7}Ana\n  ", "no-existe", None, 8, 1)
            .unwrap();
        assert_eq!(p.get(&id).unwrap().name, "Ana");
        assert_eq!(p.get(&id).unwrap().avatar, avatars::AVATARS[0].id);
        p.edit(&id, "   ", "star");
        assert_eq!(p.get(&id).unwrap().name, "Ana");
        assert_eq!(p.get(&id).unwrap().avatar, "star");
        while quick(&mut p, "x", None).is_some() {}
        assert_eq!(p.list().len(), MAX_PROFILES);
    }

    #[test]
    fn the_last_profile_cannot_be_removed() {
        let mut p = Profiles::default();
        let a = quick(&mut p, "Ana", None).unwrap();
        assert!(!p.remove(&a));
        let b = quick(&mut p, "Luis", None).unwrap();
        assert!(p.remove(&b));
        assert!(!p.remove(&b));
        assert_eq!(p.list().len(), 1);
    }

    #[test]
    fn a_damaged_file_keeps_only_what_is_usable() {
        let text = r#"{"list":[
            {"id":"default","name":"Ana","avatar":"moon"},
            {"id":"../etc","name":"Mala","avatar":"moon"},
            {"id":"default","name":"Repetida","avatar":"moon"},
            {"id":"p1","name":"","avatar":"???"},
            {"id":"p2","name":"Bloqueada","avatar":"star","lock":{"salt":"x","hash":"y","memory":4294967295,"passes":1}}]}"#;
        let p = Profiles::from_json(text);
        let ids: Vec<&str> = p.list().iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["default", "p1", "p2"]);
        assert_eq!(p.get("p1").unwrap().name, "Perfil");
        // A lock with a cost beyond the ceiling, or with broken text, never opens.
        assert!(!p.check("p2", &test_password()));
        assert_eq!(Profiles::from_json("no es json"), Profiles::default());
    }
}
