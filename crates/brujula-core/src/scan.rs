//! The check a download goes through once it has arrived, before it is given its name: a file that
//! says it is a document, a picture or a video but is really a program, and, when ClamAV is
//! installed, what it finds. No widgets and no running of programs here; the browser does that
//! and hands the results to these functions.

/// What a file turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The name says one thing and the content is a program.
    Disguised { name_says: String, is: &'static str },
    /// ClamAV recognised it as this.
    Malware(String),
}

impl Finding {
    /// What to tell the person.
    pub fn reason(&self) -> String {
        match self {
            Finding::Disguised { name_says, is } => format!(
                "Dice ser «.{name_says}», pero es {is}: así es como se esconde un programa."
            ),
            Finding::Malware(name) => format!("ClamAV lo reconoce como «{name}»."),
        }
    }
}

/// How many bytes of the start of the file the check reads.
pub const HEADER_LEN: usize = 4096;

/// Extensions of files that are only ever read or played, never run.
const PASSIVE: &[&str] = &[
    "pdf", "txt", "md", "rtf", "csv", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods",
    "odp", "epub", "jpg", "jpeg", "png", "gif", "webp", "bmp", "ico", "tif", "tiff", "heic",
    "avif", "mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "mp4", "mkv", "webm", "avi", "mov",
    "m4v", "zip", "gz", "xz", "bz2", "zst", "7z", "rar", "tar", "iso",
];

/// `Some("un programa de Linux (ELF)")` and so on when `header` starts like a program.
pub fn program_kind(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(b"\x7fELF") {
        return Some("un programa de Linux (ELF)");
    }
    // Mach-O, thin and fat, both byte orders.
    const MACHO: [[u8; 4]; 6] = [
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xbe, 0xba, 0xfe, 0xca],
    ];
    if header.len() >= 4 && MACHO.iter().any(|m| header[..4] == m[..]) {
        // `cafebabe` is also the start of a Java class; a program for macOS or Java is a program.
        return Some("un programa de macOS o Java");
    }
    // A Windows program: "MZ", and the offset of the PE header says "PE\0\0".
    if header.starts_with(b"MZ") && header.len() >= 0x40 {
        let at = u32::from_le_bytes([header[0x3c], header[0x3d], header[0x3e], header[0x3f]]);
        let at = usize::try_from(at).unwrap_or(usize::MAX);
        if header
            .get(at..at.saturating_add(4))
            .is_some_and(|sig| sig == b"PE\0\0")
        {
            return Some("un programa de Windows");
        }
    }
    None
}

/// The last extension of a name, in lower case.
fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.trim().to_lowercase())
}

/// A file whose name promises something passive and whose start says program. A file that is
/// already known to be able to run (see `downloads::is_dangerous`) was confirmed by the person
/// and is not looked at here.
pub fn disguised(name: &str, header: &[u8]) -> Option<Finding> {
    if crate::downloads::is_dangerous(name) {
        return None;
    }
    let ext = extension(name).filter(|e| PASSIVE.contains(&e.as_str()))?;
    program_kind(header).map(|is| Finding::Disguised { name_says: ext, is })
}

/// The name ClamAV gives a file it found something in, from its output (`path: Name FOUND`).
pub fn parse_clam(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let found = line.trim_end().strip_suffix(" FOUND")?;
        let (_, name) = found.rsplit_once(": ")?;
        let name = name.trim();
        (!name.is_empty()).then(|| name.chars().take(80).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pe() -> Vec<u8> {
        let mut h = vec![0u8; 0x100];
        h[0] = b'M';
        h[1] = b'Z';
        h[0x3c] = 0x80;
        h[0x80..0x84].copy_from_slice(b"PE\0\0");
        h
    }

    #[test]
    fn programs_are_told_by_their_first_bytes() {
        assert!(program_kind(b"\x7fELF\x02\x01\x01")
            .unwrap()
            .contains("Linux"));
        assert!(program_kind(&pe()).unwrap().contains("Windows"));
        assert!(program_kind(&[0xcf, 0xfa, 0xed, 0xfe, 0, 0])
            .unwrap()
            .contains("macOS"));
        assert_eq!(program_kind(b"%PDF-1.7"), None);
        assert_eq!(program_kind(b""), None);
        // Text that happens to start with MZ is not a program.
        let mut text = b"MZ was a nice year".to_vec();
        text.resize(0x100, b' ');
        assert_eq!(program_kind(&text), None);
        // A PE offset that points outside the header is not believed.
        let mut odd = pe();
        odd[0x3c] = 0xff;
        odd[0x3d] = 0xff;
        odd[0x3e] = 0xff;
        odd[0x3f] = 0xff;
        assert_eq!(program_kind(&odd), None);
    }

    #[test]
    fn a_program_dressed_as_a_document_is_found() {
        let found = disguised("factura.PDF", b"\x7fELF....").unwrap();
        assert_eq!(
            found,
            Finding::Disguised {
                name_says: "pdf".into(),
                is: "un programa de Linux (ELF)"
            }
        );
        assert!(found.reason().contains(".pdf"));
        assert!(disguised("foto.jpg", &pe()).is_some());
    }

    #[test]
    fn honest_files_and_confirmed_programs_pass() {
        assert_eq!(disguised("factura.pdf", b"%PDF-1.7 ..."), None);
        // A program with a program's name was already asked about.
        assert_eq!(disguised("setup.exe", &pe()), None);
        // No extension, or one that is not passive: not this check's business.
        assert_eq!(disguised("programa", b"\x7fELF"), None);
        assert_eq!(disguised("datos.bin2", b"\x7fELF"), None);
        assert_eq!(disguised(".pdf", b"\x7fELF"), None);
    }

    #[test]
    fn clamav_output_is_read() {
        assert_eq!(
            parse_clam("/home/u/x.zip: Win.Test.EICAR_HDB-1 FOUND\n"),
            Some("Win.Test.EICAR_HDB-1".to_owned())
        );
        assert_eq!(parse_clam("/home/u/x.zip: OK\n"), None);
        assert_eq!(parse_clam(""), None);
        assert_eq!(parse_clam("/a: b: Name.X FOUND"), Some("Name.X".to_owned()));
    }
}
