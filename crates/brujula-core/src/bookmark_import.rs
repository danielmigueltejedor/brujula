//! Bookmarks from other browsers, so nobody starts from nothing.
//!
//! Reads what those browsers keep or export: Chromium's `Bookmarks` file (Chrome, Chromium,
//! Brave, Edge, Vivaldi), the compressed JSON backups of Firefox and its relatives (Zen,
//! LibreWolf, Floorp: `*.jsonlz4`) and the HTML file that every browser, Arc included, can export.
//! A file is untrusted: only http(s) addresses come out, text is cleaned, depth and size are
//! bounded, and the decompressor checks every index.

use serde_json::Value;

/// How many bookmarks are taken from one file.
pub const MAX_IMPORTED: usize = 5_000;
const MAX_TITLE: usize = 200;
const MAX_URL: usize = 2_048;
const MAX_DEPTH: usize = 40;
/// The most a backup may unpack to.
const MAX_UNPACKED: usize = 64 * 1024 * 1024;
const MOZ_MAGIC: &[u8; 8] = b"mozLz40\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    /// The file is not a format that is read.
    Unknown,
    /// The file is damaged or claims more than is allowed.
    Corrupt,
}

fn push(out: &mut Vec<Imported>, url: &str, title: &str) {
    if out.len() >= MAX_IMPORTED || url.len() > MAX_URL {
        return;
    }
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return;
    };
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return;
    }
    let url = parsed.to_string();
    if out.iter().any(|b| b.url == url) {
        return;
    }
    let title: String = title
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_TITLE)
        .collect();
    out.push(Imported { url, title });
}

fn walk_chromium(node: &Value, depth: usize, out: &mut Vec<Imported>) {
    if depth > MAX_DEPTH {
        return;
    }
    if node.get("type").and_then(Value::as_str) == Some("url") {
        if let Some(u) = node.get("url").and_then(Value::as_str) {
            push(
                out,
                u,
                node.get("name").and_then(Value::as_str).unwrap_or(""),
            );
        }
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for c in children {
            walk_chromium(c, depth + 1, out);
        }
    }
}

fn walk_mozilla(node: &Value, depth: usize, out: &mut Vec<Imported>) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(u) = node.get("uri").and_then(Value::as_str) {
        push(
            out,
            u,
            node.get("title").and_then(Value::as_str).unwrap_or(""),
        );
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for c in children {
            walk_mozilla(c, depth + 1, out);
        }
    }
}

/// Chromium's `Bookmarks` file.
pub fn from_chromium_json(text: &str) -> Result<Vec<Imported>, ImportError> {
    let v: Value = serde_json::from_str(text).map_err(|_| ImportError::Corrupt)?;
    let roots = v.get("roots").ok_or(ImportError::Unknown)?;
    let mut out = Vec::new();
    if let Some(map) = roots.as_object() {
        for node in map.values().filter(|n| n.is_object()) {
            walk_chromium(node, 0, &mut out);
        }
    }
    Ok(out)
}

/// A bookmarks backup of Firefox or a browser made from it.
pub fn from_mozilla_json(text: &str) -> Result<Vec<Imported>, ImportError> {
    let v: Value = serde_json::from_str(text).map_err(|_| ImportError::Corrupt)?;
    if v.get("children").is_none() && v.get("uri").is_none() {
        return Err(ImportError::Unknown);
    }
    let mut out = Vec::new();
    walk_mozilla(&v, 0, &mut out);
    Ok(out)
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|e| *e <= 8);
        let replaced = end.and_then(|e| match &rest[1..e] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" | "#x27" => Some('\''),
            n => n
                .strip_prefix('#')
                .and_then(|d| d.parse::<u32>().ok())
                .and_then(char::from_u32),
        });
        match (replaced, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The Netscape bookmarks file (`<A HREF="…">title</A>`) that browsers export.
pub fn from_html(text: &str) -> Result<Vec<Imported>, ImportError> {
    let lower = text.to_ascii_lowercase();
    if !lower.contains("<a ") {
        return Err(ImportError::Unknown);
    }
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<a ") {
        let start = at + i;
        let Some(tag_end) = lower[start..].find('>').map(|e| start + e) else {
            break;
        };
        let tag = &text[start..tag_end];
        let tag_lower = &lower[start..tag_end];
        let href = tag_lower.find("href=").and_then(|h| {
            let v = &tag[h + 5..];
            let quote = v.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &v[1..];
                body.find(quote).map(|e| &body[..e])
            } else {
                None
            }
        });
        let close = lower[tag_end..].find("</a>").map(|e| tag_end + e);
        if let (Some(href), Some(close)) = (href, close) {
            let title = unescape(&text[tag_end + 1..close]);
            push(&mut out, &unescape(href), &title);
            at = close;
        } else {
            at = tag_end;
        }
        if out.len() >= MAX_IMPORTED {
            break;
        }
    }
    Ok(out)
}

/// Decompresses an LZ4 block into at most `limit` bytes. Every offset and length is checked.
pub fn lz4_block(src: &[u8], expected: usize, limit: usize) -> Result<Vec<u8>, ImportError> {
    if expected > limit {
        return Err(ImportError::Corrupt);
    }
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    let mut i = 0usize;
    let length = |i: &mut usize, base: usize| -> Result<usize, ImportError> {
        let mut n = base;
        if base == 15 {
            loop {
                let b = *src.get(*i).ok_or(ImportError::Corrupt)?;
                *i += 1;
                n = n.checked_add(usize::from(b)).ok_or(ImportError::Corrupt)?;
                if n > limit {
                    return Err(ImportError::Corrupt);
                }
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    };
    while i < src.len() {
        let token = src[i];
        i += 1;
        let literals = length(&mut i, usize::from(token >> 4))?;
        let end = i.checked_add(literals).ok_or(ImportError::Corrupt)?;
        let lit = src.get(i..end).ok_or(ImportError::Corrupt)?;
        if out.len() + literals > limit {
            return Err(ImportError::Corrupt);
        }
        out.extend_from_slice(lit);
        i = end;
        if i >= src.len() {
            break;
        }
        let off = src.get(i..i + 2).ok_or(ImportError::Corrupt)?;
        let offset = usize::from(u16::from_le_bytes([off[0], off[1]]));
        i += 2;
        if offset == 0 || offset > out.len() {
            return Err(ImportError::Corrupt);
        }
        let m = length(&mut i, usize::from(token & 15))? + 4;
        if out.len() + m > limit {
            return Err(ImportError::Corrupt);
        }
        // The copy may overlap what it is writing, so it goes a byte at a time.
        let from = out.len() - offset;
        for k in 0..m {
            let b = out[from + k];
            out.push(b);
        }
    }
    if out.len() != expected {
        return Err(ImportError::Corrupt);
    }
    Ok(out)
}

/// The text of a `.jsonlz4` file.
pub fn unpack_mozlz4(bytes: &[u8]) -> Result<String, ImportError> {
    let rest = bytes.strip_prefix(MOZ_MAGIC).ok_or(ImportError::Unknown)?;
    let size = rest.get(..4).ok_or(ImportError::Corrupt)?;
    let size = u32::from_le_bytes([size[0], size[1], size[2], size[3]]) as usize;
    let data = lz4_block(&rest[4..], size, MAX_UNPACKED)?;
    String::from_utf8(data).map_err(|_| ImportError::Corrupt)
}

/// Reads a file whatever its format is.
pub fn parse(bytes: &[u8]) -> Result<Vec<Imported>, ImportError> {
    if bytes.starts_with(MOZ_MAGIC) {
        return from_mozilla_json(&unpack_mozlz4(bytes)?);
    }
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') {
        return match from_chromium_json(trimmed) {
            Err(ImportError::Unknown) => from_mozilla_json(trimmed),
            other => other,
        };
    }
    from_html(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_bookmarks_come_out_flat_and_clean() {
        let text = r#"{"roots":{"bookmark_bar":{"type":"folder","children":[
            {"type":"url","name":"Rust\n","url":"https://www.rust-lang.org/"},
            {"type":"folder","children":[{"type":"url","name":"Dup","url":"https://www.rust-lang.org/"},
              {"type":"url","name":"Mal","url":"javascript:alert(1)"},
              {"type":"url","name":"Local","url":"file:///etc/passwd"}]}]},
            "other":{"type":"folder","children":[{"type":"url","name":"Otro","url":"http://a.example/x?y=1"}]}}}"#;
        let got = from_chromium_json(text).unwrap();
        let urls: Vec<&str> = got.iter().map(|b| b.url.as_str()).collect();
        assert_eq!(
            urls,
            vec!["https://www.rust-lang.org/", "http://a.example/x?y=1"]
        );
        assert_eq!(got[0].title, "Rust");
        assert_eq!(from_chromium_json("{}"), Err(ImportError::Unknown));
        assert_eq!(from_chromium_json("no"), Err(ImportError::Corrupt));
    }

    #[test]
    fn mozilla_backups_skip_places_and_folders() {
        let text = r#"{"typeCode":2,"title":"","children":[
            {"typeCode":2,"title":"Menu","children":[
                {"typeCode":1,"title":"Zen","uri":"https://zen-browser.app/"},
                {"typeCode":1,"title":"Consulta","uri":"place:type=6"}]}]}"#;
        let got = from_mozilla_json(text).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].title, "Zen");
        assert_eq!(from_mozilla_json("[]"), Err(ImportError::Unknown));
    }

    #[test]
    fn exported_html_is_read_with_its_entities() {
        let text = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1><DL><p>
            <DT><A HREF="https://a.example/?x=1&amp;y=2" ADD_DATE="1">Tom &amp; Jerry</A>
            <DT><a href='https://b.example/'>Otro &#233;xito</a>
            <DT><A HREF="javascript:void(0)">Nada</A>
            <DT><A HREF=sin-comillas>Roto</A>
            <DT><A HREF="https://c.example/">Sin cierre"#;
        let got = from_html(text).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].url, "https://a.example/?x=1&y=2");
        assert_eq!(got[0].title, "Tom & Jerry");
        assert_eq!(got[1].title, "Otro éxito");
        assert_eq!(from_html("hola"), Err(ImportError::Unknown));
    }

    #[test]
    fn the_number_of_bookmarks_and_their_length_are_limited() {
        let mut html = String::new();
        for n in 0..MAX_IMPORTED + 50 {
            html.push_str(&format!(r#"<A HREF="https://s{n}.example/">t</A>"#));
        }
        html.push_str(&format!(
            r#"<A HREF="https://long.example/{}">x</A>"#,
            "a".repeat(3000)
        ));
        assert_eq!(from_html(&html).unwrap().len(), MAX_IMPORTED);
    }

    #[test]
    fn lz4_blocks_unpack_with_literals_and_overlapping_matches() {
        // "abc" then a copy of 9 bytes from 3 back, then the literal "d".
        let block = [0x35, b'a', b'b', b'c', 3, 0, 0x10, b'd'];
        assert_eq!(lz4_block(&block, 13, 1000).unwrap(), b"abcabcabcabcd");
        // A literal run longer than 15 uses an extension byte.
        let mut long = vec![0xF0, 5];
        long.extend(std::iter::repeat_n(b'z', 20));
        assert_eq!(lz4_block(&long, 20, 1000).unwrap(), vec![b'z'; 20]);
    }

    #[test]
    fn lz4_refuses_anything_that_would_read_or_grow_out_of_bounds() {
        // Offset 0, an offset before the start, a truncated literal run, a wrong size, a size
        // over the limit, and a length extension that never ends.
        assert!(lz4_block(&[0x11, b'a', 0, 0], 5, 100).is_err());
        assert!(lz4_block(&[0x11, b'a', 9, 0, 0x10, b'b'], 6, 100).is_err());
        assert!(lz4_block(&[0x50, b'a', b'b'], 5, 100).is_err());
        assert!(lz4_block(&[0x10, b'a'], 2, 100).is_err());
        assert!(lz4_block(&[0x10, b'a'], 1, 0).is_err());
        let endless = vec![0xF0u8, 255, 255, 255, 255, 255, 255];
        assert!(lz4_block(&endless, 10, 1000).is_err());
        assert!(lz4_block(&[0x0F, 1, 0, 255, 255, 255], 10, 100).is_err());
    }

    #[test]
    fn a_mozilla_backup_unpacks_and_a_file_is_recognised_by_its_start() {
        let json = r#"{"children":[{"uri":"https://zen-browser.app/","title":"Zen"}]}"#;
        // Literals only: token with the length in the high nibble, an extension for 15 or more.
        let mut block = vec![0xF0u8];
        let mut n = json.len() - 15;
        while n >= 255 {
            block.push(255);
            n -= 255;
        }
        block.push(n as u8);
        block.extend_from_slice(json.as_bytes());
        let mut file = MOZ_MAGIC.to_vec();
        file.extend_from_slice(&(json.len() as u32).to_le_bytes());
        file.extend_from_slice(&block);
        assert_eq!(parse(&file).unwrap()[0].title, "Zen");
        assert_eq!(parse(&file[..12]), Err(ImportError::Corrupt));
        assert_eq!(parse(json.as_bytes()).unwrap().len(), 1);
        assert_eq!(parse(b"\xef\xbb\xbf  {\"roots\":{}}").unwrap().len(), 0);
        assert_eq!(parse(b"nada de nada"), Err(ImportError::Unknown));
    }
}
