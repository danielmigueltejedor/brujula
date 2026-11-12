//! Site icons, kept on disk so the Spotlight and the bookmark lists can show them without
//! loading the page. One small PNG per host.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use nodalia_gtk::gtk;
use nodalia_gtk::gtk::gdk;
use nodalia_gtk::gtk::prelude::*;

use crate::paths;

thread_local! {
    static MEMORY: RefCell<HashMap<String, Option<gdk::Texture>>> = RefCell::new(HashMap::new());
}

/// `example.com` of an `https://example.com/x` address, if it is a web page.
fn host_of(url: &str) -> Option<String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    let safe = host.len() <= 100
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    safe.then_some(host)
}

fn file_for(host: &str) -> PathBuf {
    paths::cache_dir()
        .join("favicons")
        .join(format!("{host}.png"))
}

/// Keeps the icon a page just showed.
pub fn remember(url: &str, texture: &gdk::Texture) {
    let Some(host) = host_of(url) else { return };
    let path = file_for(&host);
    MEMORY.with(|m| m.borrow_mut().insert(host, Some(texture.clone())));
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = texture.save_to_png(&path);
    }
}

/// The icon last seen for the site of `url`.
pub fn lookup(url: &str) -> Option<gdk::Texture> {
    let host = host_of(url)?;
    MEMORY.with(|m| {
        m.borrow_mut()
            .entry(host.clone())
            .or_insert_with(|| gdk::Texture::from_filename(file_for(&host)).ok())
            .clone()
    })
}

/// A 16x16 widget showing `texture`, scaled to fit whatever its natural size is: a large icon
/// must not push the row it sits in out of shape.
pub fn picture(texture: &gdk::Texture) -> gtk::Widget {
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_size_request(16, 16);
    let pic = gtk::Picture::for_paintable(texture);
    pic.set_content_fit(gtk::ContentFit::Contain);
    pic.set_can_shrink(true);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&spacer));
    overlay.add_overlay(&pic);
    overlay.set_halign(gtk::Align::Center);
    overlay.set_valign(gtk::Align::Center);
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.upcast()
}
