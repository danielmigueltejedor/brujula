//! Protected video and music (DRM).
//!
//! Netflix, Disney+, Prime Video, Spotify and the like encrypt their streams and only hand the
//! key to a *content decryption module* (Widevine) that the browser loads through the Encrypted
//! Media Extensions (EME). WebKitGTK, the engine of Brújula, cannot do that: the builds that
//! distributions ship leave EME out (`ENABLE_ENCRYPTED_MEDIA` is only on for experimental
//! builds), and even with it on WebKitGTK has no Widevine loader (Thunder/OpenCDM, the only way
//! in, is a developer-only build option). `navigator.requestMediaKeySystemAccess` does not
//! exist in the page, whatever the settings say.
//!
//! So Brújula does the honest thing: it knows the sites that need DRM, says so instead of
//! showing a player that never starts, and hands the page to a browser on this computer that
//! does carry Widevine (Chrome, Edge, Firefox, Vivaldi, Brave, Opera…). This module is the logic
//! with no interface: which pages need it, which browsers can take them and how to start one.

/// Sites whose video or music is protected: the host (and its subdomains) and, when only part
/// of the site is protected, the path it has to start with.
const SITES: &[(&str, &str)] = &[
    ("netflix.com", ""),
    ("disneyplus.com", ""),
    ("hotstar.com", ""),
    ("max.com", ""),
    ("hbomax.com", ""),
    ("primevideo.com", ""),
    ("amazon.com", "/gp/video"),
    ("amazon.es", "/gp/video"),
    ("amazon.de", "/gp/video"),
    ("amazon.fr", "/gp/video"),
    ("amazon.it", "/gp/video"),
    ("amazon.co.uk", "/gp/video"),
    ("hulu.com", ""),
    ("paramountplus.com", ""),
    ("peacocktv.com", ""),
    ("tv.apple.com", ""),
    ("crunchyroll.com", ""),
    ("skyshowtime.com", ""),
    ("movistarplus.es", ""),
    ("filmin.es", ""),
    ("rakuten.tv", ""),
    ("dazn.com", ""),
    ("atresplayer.com", ""),
    ("mitele.es", ""),
    ("discoveryplus.com", ""),
    ("starz.com", ""),
    ("open.spotify.com", ""),
    ("listen.tidal.com", ""),
    ("music.amazon.com", ""),
    ("music.amazon.es", ""),
];

/// How many sites the user may add on top of the built-in ones, and how long a name can be.
pub const MAX_EXTRA_SITES: usize = 200;

/// The protected site `url` belongs to, as the host to show and to remember (`netflix.com`), or
/// `None` when it is not one. `extra` holds the hosts the user marked themselves.
pub fn needs_drm(url: &str, extra: &[String]) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    let path = parsed.path();
    for (site, prefix) in SITES {
        if (host == *site || host.ends_with(&format!(".{site}"))) && path.starts_with(prefix) {
            return Some((*site).to_owned());
        }
    }
    extra
        .iter()
        .find(|e| host == **e || host.ends_with(&format!(".{e}")))
        .cloned()
}

/// The engine behind a browser decides how its window is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Chrome and everything built on it: `--app=<url>` opens a bare window with only the site.
    Chromium,
    /// Firefox has no such mode: `--new-window <url>`.
    Firefox,
}

/// Where the Widevine of a browser comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Widevine {
    /// It ships with the browser.
    Included,
    /// The browser fetches it the first time a protected site asks (Firefox says so and asks).
    OnFirstUse,
    /// Open-source builds that distributions package leave it out.
    NotIncluded,
}

/// A browser that can play protected content, if it is installed.
#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub id: &'static str,
    pub name: &'static str,
    /// Commands to look for in `PATH`, then Flatpak ids (their launchers are in `exports/bin`).
    pub programs: &'static [&'static str],
    pub family: Family,
    pub widevine: Widevine,
}

/// In the order they are offered: the ones that play out of the box first.
pub const CANDIDATES: &[Candidate] = &[
    Candidate {
        id: "chrome",
        name: "Google Chrome",
        programs: &["google-chrome-stable", "google-chrome", "com.google.Chrome"],
        family: Family::Chromium,
        widevine: Widevine::Included,
    },
    Candidate {
        id: "edge",
        name: "Microsoft Edge",
        programs: &[
            "microsoft-edge-stable",
            "microsoft-edge",
            "com.microsoft.Edge",
        ],
        family: Family::Chromium,
        widevine: Widevine::Included,
    },
    Candidate {
        id: "firefox",
        name: "Firefox",
        programs: &["firefox", "firefox-esr", "org.mozilla.firefox"],
        family: Family::Firefox,
        widevine: Widevine::OnFirstUse,
    },
    Candidate {
        id: "vivaldi",
        name: "Vivaldi",
        programs: &["vivaldi-stable", "vivaldi", "com.vivaldi.Vivaldi"],
        family: Family::Chromium,
        widevine: Widevine::OnFirstUse,
    },
    Candidate {
        id: "brave",
        name: "Brave",
        programs: &["brave", "brave-browser", "com.brave.Browser"],
        family: Family::Chromium,
        widevine: Widevine::OnFirstUse,
    },
    Candidate {
        id: "opera",
        name: "Opera",
        programs: &["opera", "com.opera.Opera"],
        family: Family::Chromium,
        widevine: Widevine::OnFirstUse,
    },
    Candidate {
        id: "chromium",
        name: "Chromium",
        programs: &["chromium", "chromium-browser", "org.chromium.Chromium"],
        family: Family::Chromium,
        widevine: Widevine::NotIncluded,
    },
];

/// A candidate that is installed, with the program that starts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: &'static str,
    pub name: &'static str,
    pub program: String,
    pub family: Family,
    pub widevine: Widevine,
}

/// The installed browsers, in offer order. `locate` answers with the full path of a program
/// when it exists; the caller decides where to look (`PATH`, the Flatpak exports).
pub fn installed(locate: impl Fn(&str) -> Option<String>) -> Vec<Found> {
    CANDIDATES
        .iter()
        .filter_map(|c| {
            let program = c.programs.iter().find_map(|p| locate(p))?;
            Some(Found {
                id: c.id,
                name: c.name,
                program,
                family: c.family,
                widevine: c.widevine,
            })
        })
        .collect()
}

/// The one to open a page with: the person's choice when it is installed, else the first one
/// that carries Widevine, else the first that can get it, else whatever is there.
pub fn choose<'a>(found: &'a [Found], preferred: &str) -> Option<&'a Found> {
    found
        .iter()
        .find(|f| !preferred.is_empty() && f.id == preferred)
        .or_else(|| found.iter().find(|f| f.widevine == Widevine::Included))
        .or_else(|| found.iter().find(|f| f.widevine == Widevine::OnFirstUse))
        .or_else(|| found.first())
}

/// Whether `id` names a browser in the table (what `drm_browser` may hold).
pub fn known_browser(id: &str) -> bool {
    CANDIDATES.iter().any(|c| c.id == id)
}

/// What to run to open `url` in `found`: the program and its arguments, never a shell line.
/// Only plain web addresses go out of Brújula this way.
pub fn command(found: &Found, url: &str) -> Result<(String, Vec<String>), String> {
    let parsed = url::Url::parse(url).map_err(|_| "not an address".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only web addresses".to_owned());
    }
    // The parsed form is escaped: no spaces, quotes or control characters get through.
    let url = parsed.as_str().to_owned();
    let args = match found.family {
        Family::Chromium => vec![format!("--app={url}")],
        Family::Firefox => vec!["--new-window".to_owned(), url],
    };
    Ok((found.program.clone(), args))
}

/// The sentence of the notice for the site `host`.
pub fn notice(host: &str, found: Option<&Found>) -> String {
    match found {
        Some(f) => format!(
            "{host} usa contenido con protección (DRM) y el motor de Brújula no puede reproducirlo. {} sí.",
            f.name
        ),
        None => format!(
            "{host} usa contenido con protección (DRM) y el motor de Brújula no puede reproducirlo. Instala Chrome o Firefox para verlo."
        ),
    }
}

/// A note for the preferences page: what is installed and what it will do.
pub fn summary(found: &[Found], preferred: &str) -> String {
    match choose(found, preferred) {
        None => "No hay ningún navegador con Widevine instalado (Chrome, Edge, Firefox, Vivaldi, Brave u Opera).".to_owned(),
        Some(f) => {
            let how = match f.widevine {
                Widevine::Included => "reproduce DRM sin pasos extra",
                Widevine::OnFirstUse => "descarga Widevine la primera vez que se lo pide una web",
                Widevine::NotIncluded => {
                    "normalmente no trae Widevine: puede que tampoco reproduzca DRM"
                }
            };
            format!("Se abriría en {}, que {how}.", f.name)
        }
    }
}

/// Adds `host` to `list` (kept short and sorted, no repeats); false if it was not a host or the
/// list is full.
pub fn remember(list: &mut Vec<String>, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let ok = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !ok {
        return false;
    }
    if list.contains(&host) {
        return true;
    }
    if list.len() >= MAX_EXTRA_SITES {
        return false;
    }
    list.push(host);
    list.sort();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here(names: &'static [&'static str]) -> impl Fn(&str) -> Option<String> {
        move |p| names.contains(&p).then(|| format!("/usr/bin/{p}"))
    }

    #[test]
    fn known_sites_and_their_subdomains_need_drm() {
        for url in [
            "https://www.netflix.com/browse",
            "https://netflix.com/",
            "https://www.disneyplus.com/es-es/",
            "https://open.spotify.com/track/1",
            "https://play.max.com/x",
            "http://tv.apple.com/",
        ] {
            assert!(needs_drm(url, &[]).is_some(), "{url}");
        }
        assert_eq!(
            needs_drm("https://www.netflix.com/browse", &[]).as_deref(),
            Some("netflix.com")
        );
    }

    #[test]
    fn lookalikes_and_ordinary_sites_do_not() {
        for url in [
            "https://example.com/",
            "https://notnetflix.com/",
            "https://netflix.com.evil.example/",
            "https://youtube.com/watch?v=1",
            "https://spotify.com/",
            "about:blank",
            "brujula://settings",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert!(needs_drm(url, &[]).is_none(), "{url}");
        }
    }

    #[test]
    fn only_the_video_part_of_amazon_is_protected() {
        assert!(needs_drm("https://www.amazon.es/gp/video/detail/X", &[]).is_some());
        assert!(needs_drm("https://www.amazon.es/dp/B000", &[]).is_none());
        assert!(needs_drm("https://www.primevideo.com/", &[]).is_some());
    }

    #[test]
    fn sites_the_user_added_count_and_match_subdomains() {
        let extra = vec!["mi-tele.example".to_owned()];
        assert_eq!(
            needs_drm("https://www.mi-tele.example/x", &extra).as_deref(),
            Some("mi-tele.example")
        );
        assert_eq!(
            needs_drm("https://play.mi-tele.example/x", &extra).as_deref(),
            Some("mi-tele.example")
        );
        assert!(needs_drm("https://otra.example/", &extra).is_none());
    }

    #[test]
    fn installed_browsers_keep_the_offer_order_and_use_the_first_program_found() {
        let found = installed(here(&["firefox", "google-chrome", "google-chrome-stable"]));
        let ids: Vec<_> = found.iter().map(|f| f.id).collect();
        assert_eq!(ids, ["chrome", "firefox"]);
        assert_eq!(found[0].program, "/usr/bin/google-chrome-stable");
        assert!(installed(here(&[])).is_empty());
    }

    #[test]
    fn flatpak_launchers_are_found_too() {
        let found = installed(here(&["org.mozilla.firefox"]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "firefox");
    }

    #[test]
    fn the_choice_prefers_the_persons_pick_then_widevine_that_ships() {
        let found = installed(here(&["firefox", "chromium", "google-chrome"]));
        assert_eq!(choose(&found, "").map(|f| f.id), Some("chrome"));
        assert_eq!(choose(&found, "firefox").map(|f| f.id), Some("firefox"));
        // A pick that is not installed is ignored.
        assert_eq!(choose(&found, "brave").map(|f| f.id), Some("chrome"));
        let no_chrome = installed(here(&["chromium", "firefox"]));
        assert_eq!(choose(&no_chrome, "").map(|f| f.id), Some("firefox"));
        let only_chromium = installed(here(&["chromium"]));
        assert_eq!(choose(&only_chromium, "").map(|f| f.id), Some("chromium"));
        assert!(choose(&[], "").is_none());
    }

    #[test]
    fn the_command_is_built_per_family_and_never_through_a_shell() {
        let found = installed(here(&["firefox", "google-chrome-stable"]));
        let (program, args) = command(&found[0], "https://www.netflix.com/title/1?a=b").unwrap();
        assert_eq!(program, "/usr/bin/google-chrome-stable");
        assert_eq!(args, ["--app=https://www.netflix.com/title/1?a=b"]);
        let (_, args) = command(&found[1], "https://www.netflix.com/").unwrap();
        assert_eq!(args, ["--new-window", "https://www.netflix.com/"]);
    }

    #[test]
    fn the_command_refuses_what_is_not_a_web_address() {
        let found = installed(here(&["firefox"]));
        for url in [
            "--remote-debugging-port=1",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "brujula://settings",
            "",
        ] {
            assert!(command(&found[0], url).is_err(), "{url}");
        }
        // Spaces and quotes come out escaped, in one argument.
        let (_, args) = command(&found[0], "https://a.example/x y\"z").unwrap();
        assert_eq!(args.len(), 2);
        assert!(!args[1].contains(' ') && !args[1].contains('"'));
    }

    #[test]
    fn the_notice_names_the_browser_or_says_what_is_missing() {
        let found = installed(here(&["firefox"]));
        assert!(notice("netflix.com", found.first()).contains("Firefox"));
        assert!(notice("netflix.com", None).contains("Instala"));
        assert!(summary(&found, "").contains("Firefox"));
        assert!(summary(&[], "").contains("No hay"));
    }

    #[test]
    fn remembering_a_site_validates_dedups_and_caps() {
        let mut list = Vec::new();
        assert!(remember(&mut list, "Mi-Tele.Example"));
        assert!(remember(&mut list, "mi-tele.example"));
        assert_eq!(list, ["mi-tele.example"]);
        for bad in ["", "localhost", "a b.example", "x/y.example", "a;b.example"] {
            assert!(!remember(&mut list, bad), "{bad}");
        }
        for i in 0..MAX_EXTRA_SITES {
            remember(&mut list, &format!("s{i}.example"));
        }
        assert_eq!(list.len(), MAX_EXTRA_SITES);
        assert!(!remember(&mut list, "mas.example"));
        assert!(known_browser("firefox") && !known_browser("netscape"));
    }
}
