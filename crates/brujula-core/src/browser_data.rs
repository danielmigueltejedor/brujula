//! History, cookies and passwords from other browsers, so nobody starts from nothing.
//!
//! Reads what those browsers keep on disk: the SQLite files of Chromium (`History`, `Cookies`,
//! `Login Data`) and of Firefox and its relatives (`places.sqlite`, `cookies.sqlite`), and the
//! CSV file that every browser and password manager can export for passwords. A browser may be
//! running, so a file is first copied to a private folder and read from the copy. Everything in
//! there is untrusted: only web addresses come out, text is cleaned, and how much is taken is
//! bounded.
//!
//! Chromium encrypts cookie values and passwords with a key that the desktop's keyring holds.
//! This module only decrypts with a key it is given (see [`chromium_key`]); getting the key from
//! the keyring is the application's job. What cannot be decrypted is counted, not guessed.

use std::path::{Path, PathBuf};

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, KeyIvInit};
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

/// How much is taken from one browser.
pub const MAX_HISTORY: usize = 5_000;
pub const MAX_COOKIES: usize = 20_000;
pub const MAX_LOGINS: usize = 5_000;
const MAX_URL: usize = 2_048;
const MAX_TITLE: usize = 200;
const MAX_FIELD: usize = 4_096;
/// The biggest database file that is copied and read.
const MAX_DB_BYTES: u64 = 512 * 1024 * 1024;
/// Milliseconds between 1601-01-01 (Chromium's clock) and 1970-01-01.
const WEBKIT_EPOCH_MS: u64 = 11_644_473_600_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataError {
    /// The file is not there, is too big, or cannot be copied.
    Unreadable,
    /// The file is not a database of the kind that is read, or is damaged.
    Corrupt,
}

impl From<rusqlite::Error> for DataError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Corrupt
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visits: u32,
    /// The last visit, in milliseconds since 1970.
    pub last: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Unset,
    None,
    Lax,
    Strict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    /// The domain as the browser stored it: a leading dot means "and its subdomains".
    pub host: String,
    pub name: String,
    pub value: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// When it expires, in seconds since 1970; `None` for a cookie of the session.
    pub expires: Option<u64>,
    pub same_site: SameSite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub url: String,
    pub username: String,
    pub password: String,
}

/// What was read, and how much was left out because it could not be decrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read<T> {
    pub items: Vec<T>,
    /// Entries that are encrypted with a key that was not available.
    pub unreadable: usize,
}

impl<T> Default for Read<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            unreadable: 0,
        }
    }
}

/// A database file copied to a private folder, opened read-only. The copy goes away with it.
pub struct Snapshot {
    dir: PathBuf,
    conn: Connection,
}

impl Snapshot {
    /// Copies `path` (and its write-ahead log, which holds what a running browser has not
    /// merged yet) and opens the copy.
    pub fn open(path: &Path) -> Result<Self, DataError> {
        let size = std::fs::metadata(path)
            .map_err(|_| DataError::Unreadable)?
            .len();
        if size > MAX_DB_BYTES {
            return Err(DataError::Unreadable);
        }
        let dir = private_dir()?;
        let result = Self::copy_and_open(path, &dir);
        match result {
            Ok(conn) => Ok(Self { dir, conn }),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    }

    fn copy_and_open(path: &Path, dir: &Path) -> Result<Connection, DataError> {
        let copy = dir.join("db");
        std::fs::copy(path, &copy).map_err(|_| DataError::Unreadable)?;
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        let wal = PathBuf::from(wal);
        if wal.is_file() && std::fs::metadata(&wal).is_ok_and(|m| m.len() <= MAX_DB_BYTES) {
            let _ = std::fs::copy(&wal, dir.join("db-wal"));
        }
        let conn = Connection::open_with_flags(
            &copy,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // A database that is not one fails here, not at the first query.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
        Ok(conn)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn private_dir() -> Result<PathBuf, DataError> {
    use std::os::unix::fs::DirBuilderExt;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("brujula-import-{}-{nanos}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|_| DataError::Unreadable)?;
    Ok(dir)
}

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// An address that is kept: http or https, with a host.
fn web_url(url: &str) -> Option<String> {
    if url.len() > MAX_URL {
        return None;
    }
    let parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.to_string())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ---------------------------------------------------------------------------- history

fn history_rows(
    conn: &Connection,
    sql: &str,
    to_ms: impl Fn(i64) -> u64,
) -> Result<Vec<HistoryEntry>, DataError> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<i64>>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })?;
    let mut out: Vec<HistoryEntry> = Vec::new();
    for row in rows.flatten() {
        if out.len() >= MAX_HISTORY {
            break;
        }
        let (url, title, visits, last) = row;
        let Some(url) = web_url(&url) else { continue };
        out.push(HistoryEntry {
            url,
            title: clean(&title.unwrap_or_default(), MAX_TITLE),
            visits: u32::try_from(visits.unwrap_or(1).max(1)).unwrap_or(u32::MAX),
            last: to_ms(last.unwrap_or(0)),
        });
    }
    Ok(out)
}

/// Chromium's `History` file: the pages, newest first.
pub fn history_chromium(conn: &Connection) -> Result<Vec<HistoryEntry>, DataError> {
    history_rows(
        conn,
        "SELECT url, title, visit_count, last_visit_time FROM urls \
         WHERE last_visit_time > 0 AND hidden = 0 ORDER BY last_visit_time DESC LIMIT 20000",
        |t| u64::try_from(t / 1000).map_or(0, |ms| ms.saturating_sub(WEBKIT_EPOCH_MS)),
    )
}

/// Firefox's `places.sqlite`: the pages, newest first.
pub fn history_firefox(conn: &Connection) -> Result<Vec<HistoryEntry>, DataError> {
    history_rows(
        conn,
        "SELECT url, title, visit_count, last_visit_date FROM moz_places \
         WHERE last_visit_date IS NOT NULL AND hidden = 0 ORDER BY last_visit_date DESC LIMIT 20000",
        |t| u64::try_from(t / 1000).unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------- decryption

/// Chromium's key for Linux: PBKDF2 (HMAC-SHA1, one round, the salt `saltysalt`) over the secret
/// the keyring holds. Without a keyring Chromium uses the secret `peanuts`.
pub fn chromium_key(secret: &[u8]) -> [u8; 16] {
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(secret, b"saltysalt", 1, &mut key);
    key
}

/// The keys to decrypt with. `v10` values use the fixed secret, `v11` ones the keyring's.
#[derive(Debug, Clone, Default)]
pub struct Keys {
    pub v10: Option<[u8; 16]>,
    pub v11: Option<[u8; 16]>,
}

impl Keys {
    /// The fixed key of `v10` plus, when there is one, the key from the keyring.
    pub fn with_keyring(secret: Option<&[u8]>) -> Self {
        Self {
            v10: Some(chromium_key(b"peanuts")),
            v11: secret.map(chromium_key),
        }
    }
}

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Decrypts a Chromium `v10`/`v11` value: AES-128-CBC with an IV of sixteen spaces.
fn decrypt(blob: &[u8], keys: &Keys) -> Option<Vec<u8>> {
    let (key, data) = if let Some(d) = blob.strip_prefix(b"v10") {
        (keys.v10?, d)
    } else {
        (keys.v11?, blob.strip_prefix(b"v11")?)
    };
    Aes128CbcDec::new(&key.into(), &[b' '; 16].into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .ok()
}

/// Newer Chromium puts the SHA-256 of the cookie's domain in front of the value.
fn without_host_hash(plain: Vec<u8>, host: &str) -> Vec<u8> {
    if plain.len() >= 32 && plain[..32] == Sha256::digest(host.as_bytes())[..] {
        plain[32..].to_vec()
    } else {
        plain
    }
}

// ---------------------------------------------------------------------------- cookies

fn cookie_text_ok(text: &str) -> bool {
    text.len() <= MAX_FIELD && !text.chars().any(|c| c.is_control() || c == ';')
}

fn host_ok(host: &str) -> bool {
    let bare = host.trim_start_matches('.');
    !bare.is_empty()
        && bare.len() <= 253
        && bare
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// Builds a cookie if everything about it is acceptable and it has not expired.
#[allow(clippy::too_many_arguments)]
fn make_cookie(
    host: &str,
    name: &str,
    value: &str,
    path: &str,
    secure: bool,
    http_only: bool,
    expires: Option<u64>,
    same_site: SameSite,
) -> Option<Cookie> {
    if !host_ok(host) || name.is_empty() || !cookie_text_ok(name) || !cookie_text_ok(value) {
        return None;
    }
    if !path.starts_with('/') || !cookie_text_ok(path) {
        return None;
    }
    if expires.is_some_and(|e| e <= now_secs()) {
        return None;
    }
    Some(Cookie {
        host: host.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
        path: path.to_owned(),
        secure,
        http_only,
        expires,
        same_site,
    })
}

/// Firefox's `cookies.sqlite`. Its values are not encrypted.
pub fn cookies_firefox(conn: &Connection) -> Result<Read<Cookie>, DataError> {
    let mut statement = conn.prepare(
        "SELECT host, name, value, path, expiry, isSecure, isHttpOnly, sameSite \
         FROM moz_cookies LIMIT 100000",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)? != 0,
            r.get::<_, i64>(6)? != 0,
            r.get::<_, i64>(7)?,
        ))
    })?;
    let mut out = Read::default();
    for (host, name, value, path, expiry, secure, http_only, same) in rows.flatten() {
        if out.items.len() >= MAX_COOKIES {
            break;
        }
        // Seconds, or milliseconds in newer versions: no date in seconds is that far away.
        let expiry = u64::try_from(expiry).unwrap_or(0);
        let expires = if expiry > 100_000_000_000 {
            expiry / 1000
        } else {
            expiry
        };
        let same_site = match same {
            0 => SameSite::None,
            1 => SameSite::Lax,
            2 => SameSite::Strict,
            _ => SameSite::Unset,
        };
        let expires = (expires > 0).then_some(expires);
        out.items.extend(make_cookie(
            &host, &name, &value, &path, secure, http_only, expires, same_site,
        ));
    }
    Ok(out)
}

/// Chromium's `Cookies` file, decrypting what is encrypted with `keys`.
pub fn cookies_chromium(conn: &Connection, keys: &Keys) -> Result<Read<Cookie>, DataError> {
    let mut statement = conn.prepare(
        "SELECT host_key, name, value, encrypted_value, path, expires_utc, is_secure, \
         is_httponly, samesite FROM cookies LIMIT 100000",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)? != 0,
            r.get::<_, i64>(7)? != 0,
            r.get::<_, i64>(8)?,
        ))
    })?;
    let mut out = Read::default();
    for (host, name, plain, encrypted, path, expires_utc, secure, http_only, same) in rows.flatten()
    {
        if out.items.len() >= MAX_COOKIES {
            break;
        }
        let value = if encrypted.is_empty() {
            plain
        } else if let Some(bytes) = decrypt(&encrypted, keys) {
            String::from_utf8_lossy(&without_host_hash(bytes, &host)).into_owned()
        } else {
            out.unreadable += 1;
            continue;
        };
        // Microseconds since 1601; 0 is a cookie of the session.
        let expires = u64::try_from(expires_utc / 1_000_000)
            .ok()
            .filter(|s| *s > 0)
            .map(|s| s.saturating_sub(WEBKIT_EPOCH_MS / 1000));
        let same_site = match same {
            0 => SameSite::None,
            1 => SameSite::Lax,
            2 => SameSite::Strict,
            _ => SameSite::Unset,
        };
        out.items.extend(make_cookie(
            &host, &name, &value, &path, secure, http_only, expires, same_site,
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------- passwords

fn push_login(out: &mut Vec<Login>, url: &str, username: &str, password: &str) {
    if out.len() >= MAX_LOGINS || password.is_empty() || password.len() > MAX_FIELD {
        return;
    }
    let Some(url) = web_url(url) else { return };
    let username = clean(username, MAX_FIELD);
    if out
        .iter()
        .any(|l| l.url == url && l.username == username && l.password == password)
    {
        return;
    }
    out.push(Login {
        url,
        username,
        password: password.to_owned(),
    });
}

/// Chromium's `Login Data`, decrypting the passwords with `keys`.
pub fn logins_chromium(conn: &Connection, keys: &Keys) -> Result<Read<Login>, DataError> {
    let mut statement = conn.prepare(
        "SELECT origin_url, username_value, password_value FROM logins \
         WHERE blacklisted_by_user = 0 LIMIT 100000",
    )?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    let mut out = Read::default();
    for (url, username, blob) in rows.flatten() {
        match decrypt(&blob, keys) {
            Some(bytes) => {
                if let Ok(password) = String::from_utf8(bytes) {
                    push_login(&mut out.items, &url, &username, &password);
                }
            }
            None if !blob.is_empty() => out.unreadable += 1,
            None => {}
        }
    }
    Ok(out)
}

/// Splits CSV text into records of fields: quoted fields may hold commas, quotes (doubled)
/// and line breaks.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => record.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            _ => field.push(c),
        }
        if records.len() > 2 * MAX_LOGINS {
            break;
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// The passwords file that Chrome, Edge, Brave, Firefox, Safari, Bitwarden or 1Password export:
/// the first line names the columns.
pub fn logins_csv(text: &str) -> Result<Vec<Login>, DataError> {
    let records = csv_records(text);
    let header = records.first().ok_or(DataError::Corrupt)?;
    let find = |names: &[&str]| {
        header.iter().position(|h| {
            let h = h.trim().to_ascii_lowercase();
            names.contains(&h.as_str())
        })
    };
    let url = find(&[
        "url",
        "login_uri",
        "origin_url",
        "website",
        "web site",
        "address",
    ]);
    let user = find(&["username", "login_username", "user", "login", "email"]);
    let pass = find(&["password", "login_password"]);
    let (Some(url), Some(pass)) = (url, pass) else {
        return Err(DataError::Corrupt);
    };
    let mut out = Vec::new();
    for record in &records[1..] {
        let get = |i: Option<usize>| i.and_then(|i| record.get(i)).map_or("", String::as_str);
        push_login(&mut out, get(Some(url)), get(user), get(Some(pass)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockEncryptMut;

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    fn encrypt(prefix: &str, key: [u8; 16], plain: &[u8]) -> Vec<u8> {
        let mut out = prefix.as_bytes().to_vec();
        out.extend(
            Aes128CbcEnc::new(&key.into(), &[b' '; 16].into())
                .encrypt_padded_vec_mut::<Pkcs7>(plain),
        );
        out
    }

    #[test]
    fn the_key_is_the_one_chromium_derives_on_linux() {
        // PBKDF2-HMAC-SHA1("peanuts", "saltysalt", 1 round, 16 bytes): the well-known v10 key.
        assert_eq!(
            chromium_key(b"peanuts"),
            [
                0xfd, 0x62, 0x1f, 0xe5, 0xa2, 0xb4, 0x02, 0x53, 0x9d, 0xfa, 0x14, 0x7c, 0xa9, 0x27,
                0x27, 0x78
            ]
        );
    }

    #[test]
    fn values_decrypt_with_their_own_key_and_nothing_else() {
        let v10 = chromium_key(b"peanuts");
        let v11 = chromium_key(b"secret from the keyring");
        let keys = Keys {
            v10: Some(v10),
            v11: Some(v11),
        };
        assert_eq!(
            decrypt(&encrypt("v10", v10, b"hello"), &keys).unwrap(),
            b"hello"
        );
        assert_eq!(
            decrypt(&encrypt("v11", v11, b"world"), &keys).unwrap(),
            b"world"
        );
        // Without the keyring's key a v11 value is left alone, never guessed.
        let only_v10 = Keys::with_keyring(None);
        assert!(decrypt(&encrypt("v11", v11, b"world"), &only_v10).is_none());
        // The wrong key, a plain value and a damaged one fail cleanly.
        assert!(decrypt(&encrypt("v10", v11, b"hello"), &keys).is_none());
        assert!(decrypt(b"plain", &keys).is_none());
        assert!(decrypt(b"v10short", &keys).is_none());
        assert!(decrypt(b"", &keys).is_none());
    }

    #[test]
    fn a_domain_hash_in_front_of_a_value_is_taken_off() {
        let mut with = Sha256::digest(b".example.com").to_vec();
        with.extend_from_slice(b"value");
        assert_eq!(without_host_hash(with, ".example.com"), b"value");
        assert_eq!(
            without_host_hash(b"value".to_vec(), ".example.com"),
            b"value"
        );
    }

    fn chromium_history() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, \
               last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls VALUES (1, 'https://a.example/x', 'A \u{7}page', 3, 13300000000000000, 0);
             INSERT INTO urls VALUES (2, 'javascript:alert(1)', 'no', 1, 13300000000000001, 0);
             INSERT INTO urls VALUES (3, 'file:///etc/passwd', 'no', 1, 13300000000000002, 0);
             INSERT INTO urls VALUES (4, 'https://hidden.example/', 'h', 1, 13300000000000003, 1);
             INSERT INTO urls VALUES (5, 'http://b.example/', NULL, 1, 13200000000000000, 0);",
        )
        .unwrap();
        c
    }

    #[test]
    fn chromium_history_comes_out_clean_and_with_unix_times() {
        let got = history_chromium(&chromium_history()).unwrap();
        let urls: Vec<&str> = got.iter().map(|h| h.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.example/x", "http://b.example/"]);
        assert_eq!(got[0].title, "A page");
        assert_eq!(got[0].visits, 3);
        // 13300000000000000 µs since 1601 is 1655526400 s since 1970.
        assert_eq!(got[0].last, 1_655_526_400_000);
        assert_eq!(got[1].title, "");
    }

    #[test]
    fn firefox_history_skips_what_was_never_visited() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, \
               hidden INTEGER, last_visit_date INTEGER);
             INSERT INTO moz_places VALUES (1, 'https://zen-browser.app/', 'Zen', 2, 0, 1700000000000000);
             INSERT INTO moz_places VALUES (2, 'https://only-bookmarked.example/', 'B', 0, 0, NULL);
             INSERT INTO moz_places VALUES (3, 'place:type=6', 'q', 1, 0, 1700000000000001);",
        )
        .unwrap();
        let got = history_firefox(&c).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].last, 1_700_000_000_000);
    }

    #[test]
    fn a_missing_table_is_an_error_not_a_panic() {
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(history_chromium(&c), Err(DataError::Corrupt));
        assert!(cookies_firefox(&c).is_err());
        assert!(logins_chromium(&c, &Keys::default()).is_err());
    }

    #[test]
    fn firefox_cookies_are_checked_and_expired_ones_dropped() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE moz_cookies (host TEXT, name TEXT, value TEXT, path TEXT, expiry INTEGER, \
               isSecure INTEGER, isHttpOnly INTEGER, sameSite INTEGER);
             INSERT INTO moz_cookies VALUES ('.example.com', 'sid', 'abc', '/', 4102444800, 1, 1, 1);
             INSERT INTO moz_cookies VALUES ('ms.example.com', 'ms', 'v', '/', 4102444800000, 0, 0, 0);
             INSERT INTO moz_cookies VALUES ('session.example.com', 's', 'v', '/', 0, 0, 0, 2);
             INSERT INTO moz_cookies VALUES ('old.example.com', 'o', 'v', '/', 1000, 0, 0, 0);
             INSERT INTO moz_cookies VALUES ('bad host', 'x', 'v', '/', 4102444800, 0, 0, 0);
             INSERT INTO moz_cookies VALUES ('ok.example.com', 'x', 'a;b', '/', 4102444800, 0, 0, 0);
             INSERT INTO moz_cookies VALUES ('ok.example.com', 'x', 'v', 'nopath', 4102444800, 0, 0, 0);",
        )
        .unwrap();
        let got = cookies_firefox(&c).unwrap().items;
        let names: Vec<(&str, Option<u64>)> =
            got.iter().map(|c| (c.host.as_str(), c.expires)).collect();
        assert_eq!(
            names,
            vec![
                (".example.com", Some(4_102_444_800)),
                ("ms.example.com", Some(4_102_444_800)),
                ("session.example.com", None),
            ]
        );
        assert!(got[0].secure && got[0].http_only);
        assert_eq!(got[0].same_site, SameSite::Lax);
        assert_eq!(got[2].same_site, SameSite::Strict);
    }

    #[test]
    fn chromium_cookies_decrypt_and_count_what_they_cannot() {
        let keys = Keys::with_keyring(Some(b"keyring"));
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE cookies (host_key TEXT, name TEXT, value TEXT, encrypted_value BLOB, path TEXT, \
               expires_utc INTEGER, is_secure INTEGER, is_httponly INTEGER, samesite INTEGER);",
        )
        .unwrap();
        // 15000000000000000 µs since 1601 is 3355526400 s since 1970 (year 2076).
        let far = 15_000_000_000_000_000i64;
        let mut hashed = Sha256::digest(b".example.com").to_vec();
        hashed.extend_from_slice(b"with hash");
        let rows: Vec<(&str, &str, &str, Vec<u8>, i64)> = vec![
            (".example.com", "plain", "visible", vec![], far),
            (
                ".example.com",
                "v11",
                "",
                encrypt("v11", keys.v11.unwrap(), b"from keyring"),
                far,
            ),
            (
                ".example.com",
                "v10",
                "",
                encrypt("v10", keys.v10.unwrap(), b"fixed key"),
                far,
            ),
            (
                ".example.com",
                "hash",
                "",
                encrypt("v11", keys.v11.unwrap(), &hashed),
                far,
            ),
            (
                ".example.com",
                "lost",
                "",
                encrypt("v11", chromium_key(b"other"), b"x"),
                far,
            ),
            (".example.com", "session", "s", vec![], 0),
        ];
        for (host, name, value, blob, exp) in rows {
            c.execute(
                "INSERT INTO cookies VALUES (?1, ?2, ?3, ?4, '/', ?5, 1, 0, -1)",
                rusqlite::params![host, name, value, blob, exp],
            )
            .unwrap();
        }
        let got = cookies_chromium(&c, &keys).unwrap();
        let values: Vec<(&str, &str)> = got
            .items
            .iter()
            .map(|c| (c.name.as_str(), c.value.as_str()))
            .collect();
        assert_eq!(
            values,
            vec![
                ("plain", "visible"),
                ("v11", "from keyring"),
                ("v10", "fixed key"),
                ("hash", "with hash"),
                ("session", "s"),
            ]
        );
        assert_eq!(got.unreadable, 1);
        assert_eq!(got.items[0].expires, Some(3_355_526_400));
        assert_eq!(got.items[4].expires, None);
        assert_eq!(got.items[0].same_site, SameSite::Unset);
        // Without the keyring's key every v11 value is counted instead.
        let no_ring = cookies_chromium(&c, &Keys::with_keyring(None)).unwrap();
        assert_eq!(no_ring.unreadable, 3);
    }

    #[test]
    fn chromium_logins_decrypt_and_only_keep_web_addresses() {
        let keys = Keys::with_keyring(Some(b"keyring"));
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB, \
               blacklisted_by_user INTEGER);",
        )
        .unwrap();
        let blob = encrypt("v11", keys.v11.unwrap(), b"hunter2");
        for (url, user, b, never) in [
            ("https://a.example/login", "ana", blob.clone(), 0),
            ("https://a.example/login", "ana", blob.clone(), 0),
            ("android://x@com.app", "ana", blob.clone(), 0),
            ("https://never.example/", "", vec![], 1),
            (
                "https://lost.example/",
                "x",
                encrypt("v11", chromium_key(b"other"), b"p"),
                0,
            ),
        ] {
            c.execute(
                "INSERT INTO logins VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![url, user, b, never],
            )
            .unwrap();
        }
        let got = logins_chromium(&c, &keys).unwrap();
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.items[0].password, "hunter2");
        assert_eq!(got.items[0].url, "https://a.example/login");
        assert_eq!(got.unreadable, 1);
    }

    #[test]
    fn csv_exports_of_the_usual_browsers_and_managers_are_read() {
        let chrome = "name,url,username,password,note\nA,https://a.example/,ana,pw1,\n\
                      \"B, inc\",\"https://b.example/\",\"bo\"\"b\",\"p\nw2\",x\n\
                      C,android://x,c,pw3,\nD,https://d.example/,d,,\n";
        let got = logins_csv(chrome).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].username, "ana");
        assert_eq!(got[1].username, "bo\"b");
        assert_eq!(got[1].password, "p\nw2");
        let firefox = "\"url\",\"username\",\"password\",\"httpRealm\"\r\n\"https://f.example\",\"fox\",\"pw\",\"\"\r\n";
        assert_eq!(logins_csv(firefox).unwrap()[0].url, "https://f.example/");
        let bitwarden = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                         ,,login,n,,,0,https://w.example/,wal,pw,\n";
        assert_eq!(logins_csv(bitwarden).unwrap()[0].username, "wal");
        let onepassword = "Title,URL,Username,Password\nT,https://o.example/,one,pw\n";
        assert_eq!(logins_csv(onepassword).unwrap().len(), 1);
        assert_eq!(logins_csv("nothing,here\n1,2\n"), Err(DataError::Corrupt));
        assert_eq!(logins_csv(""), Err(DataError::Corrupt));
    }

    #[test]
    fn the_number_of_logins_is_limited() {
        let mut csv = String::from("url,username,password\n");
        for n in 0..MAX_LOGINS + 50 {
            csv.push_str(&format!("https://s{n}.example/,u,p{n}\n"));
        }
        assert_eq!(logins_csv(&csv).unwrap().len(), MAX_LOGINS);
    }

    #[test]
    fn a_snapshot_reads_a_copy_and_removes_it() {
        let dir = private_dir().unwrap();
        let source = dir.join("History");
        {
            let c = Connection::open(&source).unwrap();
            c.execute_batch("CREATE TABLE t (a); INSERT INTO t VALUES (1);")
                .unwrap();
        }
        let copy_dir;
        {
            let snap = Snapshot::open(&source).unwrap();
            let n: i64 = snap
                .conn()
                .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1);
            copy_dir = snap.dir.clone();
            assert!(copy_dir.exists());
        }
        assert!(!copy_dir.exists(), "the copy is deleted with the snapshot");
        // What is not a database, and what is not there, are errors.
        std::fs::write(dir.join("junk"), b"not a database at all, just text").unwrap();
        assert!(Snapshot::open(&dir.join("junk")).is_err());
        assert_eq!(
            Snapshot::open(&dir.join("absent")).err(),
            Some(DataError::Unreadable)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
