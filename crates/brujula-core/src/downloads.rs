//! What Brújula lets a download be called, and which downloads it refuses.
//!
//! A page chooses the suggested file name, so it is untrusted input: it may contain path
//! separators, control characters, a leading dot that hides the file, or an extension that makes
//! the desktop run the file when it is opened.

use std::path::{Path, PathBuf};

/// Extensions the desktop can run, or that carry code, so a page cannot drop one in Downloads.
const DANGEROUS: &[&str] = &[
    "desktop",
    "sh",
    "bash",
    "zsh",
    "fish",
    "csh",
    "run",
    "bin",
    "appimage",
    "flatpakref",
    "flatpakrepo",
    "snap",
    "deb",
    "rpm",
    "pkg",
    "exe",
    "msi",
    "bat",
    "cmd",
    "com",
    "scr",
    "ps1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "jar",
    "dll",
    "so",
    "py",
    "pl",
    "rb",
    "php",
    "lnk",
    "reg",
    "dmg",
    "app",
    "command",
    "apk",
    "hta",
    "cpl",
    "pif",
    "gadget",
    "workflow",
];

/// A file name that is safe to create in the Downloads folder: no path, no control characters,
/// no leading dots, not too long, never empty.
pub fn safe_file_name(suggested: &str) -> String {
    let cleaned: String = suggested
        .trim()
        .chars()
        // Bidirectional overrides make "gpj.exe" read as "exe.jpg".
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    let name: String = trimmed.chars().take(180).collect();
    if name.is_empty() {
        "download".to_owned()
    } else {
        name
    }
}

/// A path in `directory` that does not overwrite an existing download.
pub fn available_path(directory: &Path, file_name: &str) -> PathBuf {
    let mut path = directory.join(file_name);
    let mut n = 1;
    while path.exists() {
        path = directory.join(format!("{n}-{file_name}"));
        n += 1;
    }
    path
}

/// True when the file could be run, or run something, if the user opened it. Every extension
/// counts, so `report.pdf.sh` is caught as well as `run.sh`.
pub fn is_dangerous(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower
        .split('.')
        .skip(1)
        .any(|ext| DANGEROUS.contains(&ext.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_escape_the_folder() {
        assert_eq!(safe_file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(safe_file_name("a\\b.txt"), "a_b.txt");
        assert!(!safe_file_name("x/y").contains('/'));
    }

    #[test]
    fn names_cannot_hide_or_be_empty() {
        assert_eq!(safe_file_name(".bashrc"), "bashrc");
        assert_eq!(safe_file_name("..."), "download");
        assert_eq!(safe_file_name(""), "download");
        assert_eq!(safe_file_name("  \n "), "download");
    }

    #[test]
    fn control_and_bidi_characters_are_removed() {
        assert_eq!(safe_file_name("a\u{0}b\n.txt"), "a_b_.txt");
        assert_eq!(safe_file_name("photo\u{202e}gpj.exe"), "photogpj.exe");
    }

    #[test]
    fn long_names_are_cut() {
        assert_eq!(safe_file_name(&"a".repeat(500)).chars().count(), 180);
    }

    #[test]
    fn runnable_files_are_dangerous() {
        for name in [
            "setup.exe",
            "run.SH",
            "game.AppImage",
            "evil.desktop",
            "report.pdf.sh",
            "a.tar.gz.bin",
            "x.jar",
        ] {
            assert!(is_dangerous(name), "{name}");
        }
    }

    #[test]
    fn documents_and_media_are_not() {
        for name in [
            "report.pdf",
            "photo.jpg",
            "movie.mkv",
            "archive.tar.gz",
            "notes.txt",
            "sh",
            "exe",
        ] {
            assert!(!is_dangerous(name), "{name}");
        }
    }

    #[test]
    fn existing_downloads_are_not_overwritten() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "brujula-download-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("report.pdf"), b"old").unwrap();
        std::fs::write(directory.join("1-report.pdf"), b"old").unwrap();

        assert_eq!(
            available_path(&directory, "report.pdf"),
            directory.join("2-report.pdf")
        );

        std::fs::remove_dir_all(directory).unwrap();
    }
}
