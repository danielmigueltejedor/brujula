//! Chromium extensions: what is in the package, which pages it works on, and the rules it
//! brings. The engine that runs them is in the app.

pub mod css;
pub mod dnr;
pub mod manifest;
pub mod messages;
pub mod package;
pub mod pattern;
pub mod registry;

use std::path::Path;

use manifest::Manifest;
use messages::Messages;

/// Reads an unpacked extension: its manifest, with the texts of the user's language.
pub fn load_dir(dir: &Path, languages: &[String]) -> Result<(Manifest, Messages), String> {
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|_| "No hay manifest.json en la carpeta".to_owned())?;
    let text = text.trim_start_matches('\u{feff}');
    let default = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("default_locale")
                .and_then(|d| d.as_str().map(str::to_owned))
        });
    let mut messages = Messages::default();
    for locale in messages::locale_candidates(languages, default.as_deref()) {
        if let Ok(t) =
            std::fs::read_to_string(dir.join("_locales").join(&locale).join("messages.json"))
        {
            messages.fill_from(&Messages::parse(t.trim_start_matches('\u{feff}')));
        }
    }
    let manifest = Manifest::parse(text, &messages)?;
    Ok((manifest, messages))
}

/// What installing an extension lets it do, in words, for the user to read before saying yes.
pub fn permission_summary(m: &Manifest) -> Vec<String> {
    let mut out = Vec::new();
    let mut hosts: Vec<&str> = m.hosts_text.iter().map(String::as_str).collect();
    let script_hosts: Vec<String> = m
        .content_scripts
        .iter()
        .flat_map(|c| c.matches.iter().flat_map(|p| p.webkit()))
        .collect();
    hosts.extend(script_hosts.iter().map(String::as_str));
    let everywhere = hosts.iter().any(|h| {
        *h == "<all_urls>"
            || h.starts_with("*://*/")
            || h.starts_with("http://*/")
            || h.starts_with("https://*/")
    });
    if everywhere {
        out.push("Leer y cambiar tus datos en todas las páginas web".to_owned());
    } else if !hosts.is_empty() {
        let mut names: Vec<String> = hosts
            .iter()
            .filter_map(|h| h.split("://").nth(1))
            .filter_map(|r| r.split('/').next())
            .map(|h| h.trim_start_matches("*.").to_owned())
            .collect();
        names.sort();
        names.dedup();
        let shown: Vec<String> = names.iter().take(6).cloned().collect();
        let more = if names.len() > 6 {
            format!(" y {} más", names.len() - 6)
        } else {
            String::new()
        };
        out.push(format!(
            "Leer y cambiar tus datos en: {}{more}",
            shown.join(", ")
        ));
    }
    const WORDS: &[(&str, &str)] = &[
        ("tabs", "Ver las direcciones y los títulos de tus pestañas"),
        ("history", "Leer tu historial de navegación"),
        ("bookmarks", "Leer y cambiar tus marcadores"),
        ("cookies", "Leer y cambiar tus cookies"),
        ("downloads", "Gestionar tus descargas"),
        ("webRequest", "Observar el tráfico de red"),
        ("webRequestBlocking", "Bloquear o cambiar peticiones de red"),
        ("declarativeNetRequest", "Bloquear contenido en las páginas"),
        (
            "declarativeNetRequestWithHostAccess",
            "Bloquear contenido en las páginas",
        ),
        ("clipboardRead", "Leer el portapapeles"),
        ("clipboardWrite", "Escribir en el portapapeles"),
        ("notifications", "Mostrar notificaciones"),
        ("geolocation", "Conocer tu ubicación"),
        ("nativeMessaging", "Hablar con programas de tu equipo"),
        ("management", "Gestionar otras extensiones"),
        ("proxy", "Controlar el proxy del navegador"),
        ("privacy", "Cambiar ajustes de privacidad"),
        ("scripting", "Ejecutar scripts en las páginas"),
        ("debugger", "Depurar páginas"),
    ];
    for (key, text) in WORDS {
        if m.has_permission(key) && !out.iter().any(|o| o == text) {
            out.push((*text).to_owned());
        }
    }
    out
}

/// The id in what the user typed for the store: a bare id or a Chrome Web Store address.
pub fn store_id(text: &str) -> Option<String> {
    let t = text.trim();
    if package::valid_id(t) {
        return Some(t.to_owned());
    }
    let url = url::Url::parse(t).ok()?;
    if !matches!(
        url.host_str(),
        Some("chromewebstore.google.com" | "chrome.google.com")
    ) {
        return None;
    }
    url.path_segments()?
        .find(|s| package::valid_id(s))
        .map(str::to_owned)
}

/// The id of the extension a Chrome Web Store page (`https://…/detail/<name>/<id>`) is about.
/// Other pages of the store (the home, a search, a category) give `None`.
pub fn store_page_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let old_store = host == "chrome.google.com" && parsed.path().starts_with("/webstore/detail/");
    let new_store = host == "chromewebstore.google.com" && parsed.path().starts_with("/detail/");
    if parsed.scheme() != "https" || !(old_store || new_store) {
        return None;
    }
    store_id(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use messages::Messages;

    #[test]
    fn store_page_id_only_for_detail_pages_of_the_store() {
        let id = "cjpalhdlnbpafiamejdnhcphjbkeiagm";
        let page = format!("https://chromewebstore.google.com/detail/ublock-origin/{id}?hl=es");
        assert_eq!(store_page_id(&page).as_deref(), Some(id));
        let old = format!("https://chrome.google.com/webstore/detail/ublock-origin/{id}");
        assert_eq!(store_page_id(&old).as_deref(), Some(id));
        assert_eq!(store_page_id("https://chromewebstore.google.com/"), None);
        assert_eq!(
            store_page_id(&format!("http://chromewebstore.google.com/detail/x/{id}")),
            None
        );
        assert_eq!(
            store_page_id(&format!("https://example.com/detail/x/{id}")),
            None
        );
        // The slug may hold accents (percent-encoded or not), and the address may carry a
        // language, a query and an anchor, or end in a slash.
        let accents = format!(
            "https://chromewebstore.google.com/detail/write-it-escritura-autom%C3%A1tica-para-docs/{id}?hl=es&authuser=0#reviews"
        );
        assert_eq!(store_page_id(&accents).as_deref(), Some(id));
        let raw = format!("https://chromewebstore.google.com/detail/escritura-automática/{id}/");
        assert_eq!(store_page_id(&raw).as_deref(), Some(id));
        let no_slug = format!("https://chromewebstore.google.com/detail/{id}");
        assert_eq!(store_page_id(&no_slug).as_deref(), Some(id));
        let reviews = format!("https://chromewebstore.google.com/detail/x/{id}/reviews");
        assert_eq!(store_page_id(&reviews).as_deref(), Some(id));
        // Not a page: an id typed alone.
        assert_eq!(store_page_id(id), None);
    }

    #[test]
    fn the_permission_summary_reads_like_a_warning() {
        let m = Manifest::parse(
            r#"{"manifest_version":3,"name":"x","version":"1","permissions":["tabs","storage","cookies"],
                "host_permissions":["<all_urls>"]}"#,
            &Messages::default(),
        )
        .unwrap();
        let s = permission_summary(&m);
        assert_eq!(s[0], "Leer y cambiar tus datos en todas las páginas web");
        assert!(
            s.iter().any(|l| l.contains("pestañas")) && s.iter().any(|l| l.contains("cookies"))
        );
        assert!(!s.iter().any(|l| l.contains("storage")));
        let few = Manifest::parse(
            r#"{"manifest_version":3,"name":"x","version":"1","content_scripts":[{"matches":["https://*.a.com/*","https://b.org/*"],"js":["a.js"]}]}"#,
            &Messages::default(),
        )
        .unwrap();
        assert_eq!(
            permission_summary(&few),
            vec!["Leer y cambiar tus datos en: a.com, b.org"]
        );
    }

    #[test]
    fn the_store_id_comes_from_an_id_or_a_store_address() {
        let id = "a".repeat(32);
        assert_eq!(store_id(&id).as_deref(), Some(id.as_str()));
        assert_eq!(
            store_id(&format!(
                "https://chromewebstore.google.com/detail/ublock/{id}?hl=es"
            ))
            .as_deref(),
            Some(id.as_str())
        );
        assert_eq!(
            store_id(&format!("https://chrome.google.com/webstore/detail/x/{id}")).as_deref(),
            Some(id.as_str())
        );
        assert!(store_id(&format!("https://evil.example/detail/x/{id}")).is_none());
        assert!(store_id("hola").is_none());
    }
}
