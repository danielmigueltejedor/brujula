//! Extension packages: `.crx` files (Chrome's), plain zip files and the ids Chrome gives to
//! extensions. Nothing in a package is trusted: names are checked so that nothing is written
//! outside the folder it is unpacked into, and the sizes are capped.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use base64::Engine;
use sha2::{Digest, Sha256};

const MAX_FILES: usize = 4000;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// Chrome ids are 32 letters from `a` to `p`: the first 16 bytes of a hash, a nibble per letter.
pub fn id_from_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(16)
        .flat_map(|b| [b >> 4, b & 15])
        .map(|n| char::from(b'a' + n))
        .collect()
}

/// The id Chrome derives from the `key` of a manifest (a base64 public key).
pub fn id_from_key(key: &str) -> Option<String> {
    let clean: String = key.chars().filter(|c| !c.is_whitespace()).collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(clean.as_bytes())
        .ok()?;
    (!der.is_empty()).then(|| id_from_bytes(&Sha256::digest(der)))
}

/// An id for an extension that has no key: from where it came from, stable for the same source.
pub fn id_from_source(source: &str) -> String {
    id_from_bytes(&Sha256::digest(source.as_bytes()))
}

/// Whether `id` looks like a Chrome id (which is also what lets it name a folder safely).
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

fn varint(data: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The length-delimited fields of a protobuf message: `(field number, bytes)`.
fn fields(data: &[u8]) -> Vec<(u64, &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let Some(tag) = varint(data, &mut at) else {
            break;
        };
        match tag & 7 {
            0 => {
                if varint(data, &mut at).is_none() {
                    break;
                }
            }
            2 => {
                let Some(len) = varint(data, &mut at).and_then(|l| usize::try_from(l).ok()) else {
                    break;
                };
                let Some(end) = at.checked_add(len).filter(|e| *e <= data.len()) else {
                    break;
                };
                out.push((tag >> 3, &data[at..end]));
                at = end;
            }
            _ => break,
        }
    }
    out
}

/// The zip inside a `.crx` and the extension's id when the header says it. A file that is not a
/// `.crx` is taken as the zip itself. Signatures are not checked: a `.crx` is a zip with a
/// header, and the user is the one who chose to install it.
pub fn crx_zip(bytes: &[u8]) -> Result<(&[u8], Option<String>), String> {
    if bytes.len() < 4 || &bytes[..4] != b"Cr24" {
        return Ok((bytes, None));
    }
    let word = |at: usize| -> Option<usize> {
        let b = bytes.get(at..at + 4)?;
        usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
    };
    match word(4) {
        Some(2) => {
            let (key_len, sig_len) = (
                word(8).ok_or("crx truncado")?,
                word(12).ok_or("crx truncado")?,
            );
            let key_end = 16usize.checked_add(key_len).ok_or("crx truncado")?;
            let zip_at = key_end.checked_add(sig_len).ok_or("crx truncado")?;
            let key = bytes.get(16..key_end).ok_or("crx truncado")?;
            let zip = bytes.get(zip_at..).ok_or("crx truncado")?;
            Ok((zip, Some(id_from_bytes(&Sha256::digest(key)))))
        }
        Some(3) => {
            let header_len = word(8).ok_or("crx truncado")?;
            let zip_at = 12usize.checked_add(header_len).ok_or("crx truncado")?;
            let header = bytes.get(12..zip_at).ok_or("crx truncado")?;
            let zip = bytes.get(zip_at..).ok_or("crx truncado")?;
            // signed_header_data (field 10000) holds the crx_id (field 1).
            let id = fields(header)
                .into_iter()
                .find(|(n, _)| *n == 10000)
                .and_then(|(_, data)| {
                    fields(data)
                        .into_iter()
                        .find(|(n, v)| *n == 1 && v.len() == 16)
                        .map(|(_, v)| id_from_bytes(v))
                });
            Ok((zip, id))
        }
        _ => Err("versión de .crx no soportada".to_owned()),
    }
}

/// Unpacks a zip into `dest` (which must not hold anything yet).
pub fn extract(zip_bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| format!("zip no válido: {e}"))?;
    if archive.len() > MAX_FILES {
        return Err("demasiados archivos".to_owned());
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("zip: {e}"))?;
        // `enclosed_name` is None for absolute paths and `..`.
        let Some(rel) = file.enclosed_name() else {
            return Err(format!("nombre peligroso en el zip: {}", file.name()));
        };
        let target: PathBuf = dest.join(&rel);
        if file.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        // Symbolic links and other special files are not extensions' business.
        if file
            .unix_mode()
            .is_some_and(|m| m & 0o170_000 != 0o100_000 && m & 0o170_000 != 0)
        {
            continue;
        }
        if file.size() > MAX_FILE_BYTES {
            return Err("archivo demasiado grande".to_owned());
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::new();
        file.by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| format!("zip: {e}"))?;
        total += data.len() as u64;
        if data.len() as u64 > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
            return Err("el paquete es demasiado grande".to_owned());
        }
        std::fs::write(&target, data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("brujula-pkg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ids_are_letters_a_to_p() {
        let id = id_from_bytes(&[0x00, 0xff, 0x1a]);
        assert_eq!(id, "aappbk");
        assert!(valid_id(&id_from_source("x")));
        assert!(!valid_id("short") && !valid_id(&"q".repeat(32)));
        // The id of a key is stable.
        assert_eq!(id_from_key("AQID"), id_from_key("AQ ID\n"));
        assert!(id_from_key("***").is_none());
    }

    #[test]
    fn a_plain_zip_unpacks_and_a_crx3_header_is_skipped() {
        let zip = zip_of(&[("manifest.json", b"{}"), ("js/a.js", b"1")]);
        let dest = temp("plain");
        extract(&zip, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("js/a.js")).unwrap(), b"1");
        let (inner, id) = crx_zip(&zip).unwrap();
        assert_eq!(inner, zip.as_slice());
        assert!(id.is_none());

        // crx3: header = signed_header_data { crx_id: 16 bytes }
        let crx_id = [7u8; 16];
        let mut signed = vec![0x0a, 16];
        signed.extend_from_slice(&crx_id);
        let mut header = vec![0x82, 0xf1, 0x04, u8::try_from(signed.len()).unwrap()];
        header.extend_from_slice(&signed);
        let mut crx = b"Cr24".to_vec();
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&u32::try_from(header.len()).unwrap().to_le_bytes());
        crx.extend_from_slice(&header);
        crx.extend_from_slice(&zip);
        let (inner, id) = crx_zip(&crx).unwrap();
        assert_eq!(inner, zip.as_slice());
        assert_eq!(id.unwrap(), id_from_bytes(&crx_id));
    }

    #[test]
    fn a_crx2_gives_the_id_of_its_key() {
        let zip = zip_of(&[("manifest.json", b"{}")]);
        let key = b"public-key";
        let mut crx = b"Cr24".to_vec();
        crx.extend_from_slice(&2u32.to_le_bytes());
        crx.extend_from_slice(&u32::try_from(key.len()).unwrap().to_le_bytes());
        crx.extend_from_slice(&2u32.to_le_bytes());
        crx.extend_from_slice(key);
        crx.extend_from_slice(&[9, 9]);
        crx.extend_from_slice(&zip);
        let (inner, id) = crx_zip(&crx).unwrap();
        assert_eq!(inner, zip.as_slice());
        assert_eq!(id.unwrap(), id_from_bytes(&Sha256::digest(key)));
    }

    #[test]
    fn truncated_or_unknown_crx_is_refused() {
        assert!(crx_zip(b"Cr24\x03\0\0\0\xff\xff\0\0").is_err());
        assert!(crx_zip(b"Cr24\x09\0\0\0").is_err());
    }

    #[test]
    fn names_that_climb_out_are_refused() {
        let dest = temp("slip");
        let bad = zip_of(&[("../outside.txt", b"x")]);
        assert!(extract(&bad, &dest).is_err());
        assert!(!dest.parent().unwrap().join("outside.txt").exists());
        let abs = zip_of(&[("/abs.txt", b"x")]);
        assert!(extract(&abs, &dest).is_err());
        assert!(extract(b"not a zip", &dest).is_err());
    }
}
