//! The typefaces of Nodalia Design (Inter for the text, Sora for titles) for the browser's own
//! pages. `nodalia-gtk` bundles them and writes them to the cache when the app starts, for the
//! toolkit's own widgets; a web page cannot see fonts registered with Pango, so the pages get
//! them from this address: `brujula-font://inter` and `brujula-font://sora`.

use std::path::PathBuf;

use nodalia_gtk::gtk::gio;
use nodalia_gtk::gtk::glib;
use webkit6::URISchemeRequest;

pub const SCHEME: &str = "brujula-font";

/// Which file each name stands for.
const FILES: [(&str, &str); 2] = [
    ("inter", "Inter[opsz,wght].ttf"),
    ("sora", "Sora[wght].ttf"),
];

/// The folder `nodalia-gtk` put its fonts in (the newest, if there are several).
fn font_dir() -> Option<PathBuf> {
    let base = glib::user_cache_dir().join("nodalia-gtk");
    std::fs::read_dir(base)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("fonts-"))
        .filter(|e| FILES.iter().all(|(_, f)| e.path().join(f).is_file()))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

pub fn serve(req: &URISchemeRequest) {
    let fail = |why: &str| {
        req.finish_error(&mut glib::Error::new(gio::IOErrorEnum::NotFound, why));
    };
    let uri = req.uri().map(|u| u.to_string()).unwrap_or_default();
    let name = uri
        .strip_prefix("brujula-font://")
        .map(|n| n.trim_end_matches('/'));
    let file = FILES
        .iter()
        .find(|(n, _)| Some(*n) == name)
        .map(|(_, f)| *f);
    let (Some(file), Some(dir)) = (file, font_dir()) else {
        return fail("unknown font");
    };
    match std::fs::read(dir.join(file)) {
        Ok(bytes) => {
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
            let response = webkit6::URISchemeResponse::new(&stream, -1);
            response.set_content_type("font/ttf");
            let headers =
                webkit6::soup::MessageHeaders::new(webkit6::soup::MessageHeadersType::Response);
            headers.append("Access-Control-Allow-Origin", "*");
            response.set_http_headers(headers);
            req.finish_with_response(&response);
        }
        Err(_) => fail("font not found"),
    }
}
