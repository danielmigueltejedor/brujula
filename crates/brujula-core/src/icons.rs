//! Icons for the things a user names: a space, a search engine.
//!
//! Spaces take their icon from this set, which is drawn the way Nodalia Design draws (24 px
//! grid, 3 px of margin, 1.75 px round strokes, one colour). Most of it is NDL's own; the few
//! NDL has no icon for yet (a backpack, a briefcase…) are drawn here in the same style and
//! marked `drawn_here`, so the browser installs them in the icon theme until NDL has them.
//!
//! The same shapes serve the GTK sidebar (as `ndl-<id>-symbolic`) and the HTML pages (inline
//! SVG), so the user sees one icon everywhere.

/// One icon of the set.
pub struct Icon {
    pub id: &'static str,
    pub label: &'static str,
    /// Not in NDL (yet): the browser must install it in the icon theme.
    pub drawn_here: bool,
    /// The SVG elements, in a 24 x 24 box, drawn with `currentColor`.
    pub body: &'static [&'static str],
}

/// The icon a space has until its owner picks one.
pub const DEFAULT: &str = "workspace";

pub const ICONS: &[Icon] = &[
    Icon {
        id: "user",
        label: "Persona",
        drawn_here: false,
        body: &[
            r#"<circle cx="12" cy="8" r="4"/>"#,
            r#"<path d="M4.5 20a7.5 7.5 0 0 1 15 0"/>"#,
        ],
    },
    Icon {
        id: "home",
        label: "Casa",
        drawn_here: false,
        body: &[
            r#"<path d="M3.5 11.5l8.5-7.5 8.5 7.5"/>"#,
            r#"<path d="M6 9.5V19a1 1 0 0 0 1 1h3.5v-5.5h3V20H17a1 1 0 0 0 1-1V9.5"/>"#,
        ],
    },
    Icon {
        id: "briefcase",
        label: "Trabajo",
        drawn_here: true,
        body: &[
            r#"<rect x="3.5" y="7.5" width="17" height="12" rx="2"/>"#,
            r#"<path d="M9 7.5V6a1.5 1.5 0 0 1 1.5-1.5h3A1.5 1.5 0 0 1 15 6v1.5"/>"#,
            r#"<path d="M3.5 13h17"/>"#,
            r#"<path d="M10.5 13v1.75h3V13"/>"#,
        ],
    },
    Icon {
        id: "backpack",
        label: "Clase",
        drawn_here: true,
        body: &[
            r#"<path d="M9 6.5V6a3 3 0 0 1 6 0v.5"/>"#,
            r#"<path d="M7 6.5h10a2 2 0 0 1 2 2V19a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19V8.5a2 2 0 0 1 2-2z"/>"#,
            r#"<path d="M5.5 11c2 1 4 1.5 6.5 1.5s4.5-.5 6.5-1.5"/>"#,
            r#"<rect x="9" y="14" width="6" height="4" rx="1"/>"#,
        ],
    },
    Icon {
        id: "graduation-cap",
        label: "Estudios",
        drawn_here: true,
        body: &[
            r#"<path d="M12 5 3 9.75l9 4.75 9-4.75z"/>"#,
            r#"<path d="M7 12.25V16c0 1.5 2.2 3 5 3s5-1.5 5-3v-3.75"/>"#,
            r#"<path d="M21 9.75v5"/>"#,
        ],
    },
    Icon {
        id: "users",
        label: "Amigos",
        drawn_here: false,
        body: &[
            r#"<circle cx="9" cy="8.5" r="3.5"/>"#,
            r#"<path d="M2.5 19.5a6.5 6.5 0 0 1 13 0"/>"#,
            r#"<path d="M16 5.3a3.5 3.5 0 0 1 0 6.4"/>"#,
            r#"<path d="M18 14.2a6.5 6.5 0 0 1 3.5 5.3"/>"#,
        ],
    },
    Icon {
        id: "book",
        label: "Lectura",
        drawn_here: false,
        body: &[
            r#"<path d="M12 6.5C10 5 7.5 4.5 4 4.5V18c3.5 0 6 .5 8 2 2-1.5 4.5-2 8-2V4.5c-3.5 0-6 .5-8 2z"/>"#,
            r#"<path d="M12 6.5V20"/>"#,
        ],
    },
    Icon {
        id: "code",
        label: "Código",
        drawn_here: false,
        body: &[r#"<path d="M8.5 7l-5 5 5 5M15.5 7l5 5-5 5"/>"#],
    },
    Icon {
        id: "terminal",
        label: "Terminal",
        drawn_here: false,
        body: &[
            r#"<rect x="3.5" y="5" width="17" height="14" rx="1.5"/>"#,
            r#"<path d="M7 9.5l3 2.5-3 2.5"/>"#,
            r#"<path d="M12.5 15h4"/>"#,
        ],
    },
    Icon {
        id: "gamepad",
        label: "Juegos",
        drawn_here: true,
        body: &[
            r#"<path d="M8 7.5h8a4.5 4.5 0 0 1 4.4 3.6l.6 3.4a2.6 2.6 0 0 1-4.5 2.1L14.7 15.5H9.3l-1.8 1.1A2.6 2.6 0 0 1 3 14.5l.6-3.4A4.5 4.5 0 0 1 8 7.5z"/>"#,
            r#"<path d="M8 10.5v3M6.5 12h3"/>"#,
            r#"<circle cx="15.5" cy="11.5" r="1" fill="currentColor" stroke="none"/>"#,
            r#"<circle cx="17.5" cy="13.5" r="1" fill="currentColor" stroke="none"/>"#,
        ],
    },
    Icon {
        id: "music",
        label: "Música",
        drawn_here: false,
        body: &[
            r#"<circle cx="7.5" cy="17.5" r="2.5"/>"#,
            r#"<circle cx="17.5" cy="15.5" r="2.5"/>"#,
            r#"<path d="M10 17.5V6l10-2v11.5"/>"#,
        ],
    },
    Icon {
        id: "film",
        label: "Cine",
        drawn_here: false,
        body: &[
            r#"<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/>"#,
            r#"<path d="M7.5 4.5v15M16.5 4.5v15"/>"#,
            r#"<path d="M3.5 9.5h4M3.5 14.5h4M16.5 9.5h4M16.5 14.5h4"/>"#,
        ],
    },
    Icon {
        id: "camera",
        label: "Fotos",
        drawn_here: false,
        body: &[
            r#"<path d="M4.5 7.5h3L9 5h6l1.5 2.5h3A1.5 1.5 0 0 1 21 9v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18V9a1.5 1.5 0 0 1 1.5-1.5z"/>"#,
            r#"<circle cx="12" cy="13" r="3.5"/>"#,
        ],
    },
    Icon {
        id: "heart",
        label: "Favoritos",
        drawn_here: false,
        body: &[
            r#"<path d="M12 20s-7.5-4.5-7.5-10A4.25 4.25 0 0 1 12 7.3 4.25 4.25 0 0 1 19.5 10c0 5.5-7.5 10-7.5 10z"/>"#,
        ],
    },
    Icon {
        id: "star",
        label: "Destacados",
        drawn_here: false,
        body: &[
            r#"<path d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1.05 5.85L12 16.9l-5.25 2.75 1.05-5.85L3.5 9.7l5.9-.8z"/>"#,
        ],
    },
    Icon {
        id: "palette",
        label: "Diseño",
        drawn_here: false,
        body: &[
            r#"<path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.2 0 2-.8 2-1.8 0-.5-.2-.9-.5-1.3-.300-.3-.5-.800-.5-1.2 0-1 .800-1.7 1.8-1.7H17a3.5 3.5 0 0 0 3.5-3.5c0-4.1-3.8-7.5-8.5-7.5z"/>"#,
            r#"<circle cx="7.5" cy="11.5" r="1.15" fill="currentColor" stroke="none"/>"#,
            r#"<circle cx="10" cy="7.5" r="1.15" fill="currentColor" stroke="none"/>"#,
            r#"<circle cx="14.5" cy="7.5" r="1.15" fill="currentColor" stroke="none"/>"#,
        ],
    },
    Icon {
        id: "shopping-bag",
        label: "Compras",
        drawn_here: true,
        body: &[
            r#"<path d="M5.5 8h13l1 11.5a1.5 1.5 0 0 1-1.5 1.5H6a1.5 1.5 0 0 1-1.5-1.5z"/>"#,
            r#"<path d="M9 10.5V7a3 3 0 0 1 6 0v3.5"/>"#,
        ],
    },
    Icon {
        id: "coffee",
        label: "Café",
        drawn_here: true,
        body: &[
            r#"<path d="M5 9h11v5a5 5 0 0 1-5 5h-1a5 5 0 0 1-5-5z"/>"#,
            r#"<path d="M16 10.5h1.5a2.5 2.5 0 0 1 0 5H16"/>"#,
            r#"<path d="M8.5 3.5V6M12 3.5V6"/>"#,
        ],
    },
    Icon {
        id: "globe",
        label: "Mundo",
        drawn_here: false,
        body: &[
            r#"<circle cx="12" cy="12" r="8.5"/>"#,
            r#"<path d="M3.5 12h17"/>"#,
            r#"<path d="M12 3.5c2.5 2.3 3.8 5.2 3.8 8.5s-1.3 6.2-3.8 8.5c-2.5-2.3-3.8-5.2-3.8-8.5S9.5 5.8 12 3.5z"/>"#,
        ],
    },
    Icon {
        id: "cloud",
        label: "Nube",
        drawn_here: false,
        body: &[r#"<path d="M7 18.5a4.5 4.5 0 0 1-.6-8.95A6 6 0 0 1 18 10.5a4 4 0 0 1-.5 8z"/>"#],
    },
    Icon {
        id: "mail",
        label: "Correo",
        drawn_here: false,
        body: &[
            r#"<rect x="3.5" y="5.5" width="17" height="13" rx="1.5"/>"#,
            r#"<path d="M4 7l8 6.5L20 7"/>"#,
        ],
    },
    Icon {
        id: "flash",
        label: "Energía",
        drawn_here: false,
        body: &[r#"<path d="M13 3.5 5.5 13.5H11l-1 7 7.5-10H12z"/>"#],
    },
    Icon {
        id: "sun",
        label: "Sol",
        drawn_here: false,
        body: &[
            r#"<circle cx="12" cy="12" r="4"/>"#,
            r#"<path d="M12 2.5V5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3l1.75 1.75M16.95 16.95l1.75 1.75M5.3 18.7l1.75-1.75M16.95 7.05l1.75-1.75"/>"#,
        ],
    },
    Icon {
        id: "moon",
        label: "Noche",
        drawn_here: false,
        body: &[r#"<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"/>"#],
    },
    Icon {
        id: "package",
        label: "Paquetes",
        drawn_here: false,
        body: &[
            r#"<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9z"/>"#,
            r#"<path d="M4 7.5l8 4.5 8-4.5M12 12v9"/>"#,
            r#"<path d="M8 5.25l8 4.5v3"/>"#,
        ],
    },
    Icon {
        id: "calendar",
        label: "Agenda",
        drawn_here: false,
        body: &[
            r#"<rect x="4" y="5.5" width="16" height="14.5" rx="1.5"/>"#,
            r#"<path d="M4 10h16"/>"#,
            r#"<path d="M8 3.5v4M16 3.5v4"/>"#,
        ],
    },
    Icon {
        id: "workspace",
        label: "Espacio",
        drawn_here: false,
        body: &[r#"<path d="M3 4L21 4L21 20L3 20ZM9 4L9 20M9 9L21 9"/>"#],
    },
];

/// Icons the pages use for their own controls; the user does not pick them.
pub const UI: &[Icon] = &[
    Icon {
        id: "search",
        label: "",
        drawn_here: false,
        body: &[
            r#"<circle cx="10.5" cy="10.5" r="6.5"/>"#,
            r#"<path d="M15.5 15.5l5 5"/>"#,
        ],
    },
    Icon {
        id: "settings",
        label: "",
        drawn_here: false,
        body: &[
            r#"<path d="M4 7h8M18 7h2"/>"#,
            r#"<circle cx="15" cy="7" r="2.5"/>"#,
            r#"<path d="M4 17h2M12 17h8"/>"#,
            r#"<circle cx="9" cy="17" r="2.5"/>"#,
        ],
    },
    Icon {
        id: "layout",
        label: "",
        drawn_here: false,
        body: &[
            r#"<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/>"#,
            r#"<path d="M3.5 9.5h17M10 9.5v10"/>"#,
        ],
    },
    Icon {
        id: "sidebar",
        label: "",
        drawn_here: false,
        body: &[
            r#"<rect x="3.5" y="4.5" width="17" height="15" rx="1.5"/>"#,
            r#"<path d="M9.5 4.5v15"/>"#,
        ],
    },
    Icon {
        id: "bookmark",
        label: "",
        drawn_here: false,
        body: &[r#"<path d="M7 4h10a1 1 0 0 1 1 1v15.5l-6-4-6 4V5a1 1 0 0 1 1-1z"/>"#],
    },
    Icon {
        id: "shield-check",
        label: "",
        drawn_here: false,
        body: &[
            r#"<path d="M12 3.5l7 2.5v5.5c0 4.5-3 7.7-7 9-4-1.3-7-4.5-7-9V6z"/>"#,
            r#"<path d="M9 12l2.2 2.2 4-4.4"/>"#,
        ],
    },
    Icon {
        id: "delete",
        label: "",
        drawn_here: false,
        body: &[
            r#"<path d="M4 7h16"/>"#,
            r#"<path d="M9.5 7V5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v2"/>"#,
            r#"<path d="M6.5 7l.8 12.1a1.5 1.5 0 0 0 1.5 1.4h6.4a1.5 1.5 0 0 0 1.5-1.4L17.5 7"/>"#,
            r#"<path d="M10 11v5.5M14 11v5.5"/>"#,
        ],
    },
    Icon {
        id: "plus",
        label: "",
        drawn_here: false,
        body: &[r#"<path d="M12 5v14M5 12h14"/>"#],
    },
    Icon {
        id: "info",
        label: "",
        drawn_here: false,
        body: &[
            r#"<circle cx="12" cy="12" r="8.5"/>"#,
            r#"<path d="M12 11v5.5"/>"#,
            r#"<circle cx="12" cy="7.75" r="1.15" fill="currentColor" stroke="none"/>"#,
        ],
    },
    Icon {
        id: "lock",
        label: "",
        drawn_here: false,
        body: &[
            r#"<rect x="5" y="11" width="14" height="9.5" rx="1.5"/>"#,
            r#"<path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#,
        ],
    },
    Icon {
        id: "clock",
        label: "",
        drawn_here: false,
        body: &[
            r#"<circle cx="12" cy="12" r="8.5"/>"#,
            r#"<path d="M12 7.5V12l3 2"/>"#,
        ],
    },
    Icon {
        id: "chevron-right",
        label: "",
        drawn_here: false,
        body: &[r#"<path d="M9.5 6l6 6-6 6"/>"#],
    },
    Icon {
        id: "check",
        label: "",
        drawn_here: false,
        body: &[r#"<path d="M4.5 12.5l5 5 10-11"/>"#],
    },
];

pub fn get(id: &str) -> Option<&'static Icon> {
    ICONS.iter().chain(UI).find(|i| i.id == id)
}

/// Whether the user may give `id` to a space.
pub fn is_known(id: &str) -> bool {
    ICONS.iter().any(|i| i.id == id)
}

/// The name GTK's icon theme knows the icon by.
pub fn gtk_name(id: &str) -> String {
    format!("ndl-{}-symbolic", if is_known(id) { id } else { DEFAULT })
}

/// The icon as an inline `<svg>` for a web page; it takes the colour of the text around it.
pub fn web_svg(id: &str, size: u32) -> String {
    let icon = get(id)
        .or_else(|| get(DEFAULT))
        .expect("default icon exists");
    format!(
        r#"<svg class="ico" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{}</svg>"#,
        icon.body.concat()
    )
}

/// The icon as a GTK symbolic SVG (what the icon theme stores).
pub fn gtk_svg(icon: &Icon) -> String {
    const STROKE: &str = r#" class="transparent-fill foreground-stroke" fill="none" stroke="rgb(0,0,0)" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round""#;
    const SOLID: &str = r#" class="foreground-fill" fill="rgb(0,0,0)" stroke="none""#;
    let shapes: String = icon
        .body
        .iter()
        .map(|element| {
            let (tag, rest) = element.split_at(element.find(' ').unwrap_or(element.len()));
            if let Some(rest) = rest.strip_suffix(r#"fill="currentColor" stroke="none"/>"#) {
                format!("{tag}{rest}{}/>", SOLID.trim_start())
            } else {
                format!("{tag}{STROKE}{rest}")
            }
        })
        .collect();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{shapes}</svg>"#
    )
}

/// A space's saved icon, or `""` when it is not one of ours (an emoji from an earlier version,
/// or something a damaged file or a crafted address made up).
pub fn clean(id: &str) -> String {
    if is_known(id) {
        id.to_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_the_default_is_in_the_set() {
        for (n, icon) in ICONS.iter().enumerate() {
            assert!(
                ICONS[n + 1..].iter().all(|o| o.id != icon.id),
                "{}",
                icon.id
            );
            assert!(!icon.body.is_empty() && !icon.label.is_empty());
        }
        assert!(is_known(DEFAULT));
    }

    #[test]
    fn the_icons_the_user_asked_for_exist() {
        for id in ["user", "backpack", "briefcase", "home", "graduation-cap"] {
            assert!(is_known(id), "{id}");
        }
    }

    #[test]
    fn web_icons_are_inline_and_unknown_ones_fall_back() {
        let svg = web_svg("backpack", 20);
        assert!(svg.starts_with("<svg") && svg.contains("currentColor"));
        assert_eq!(web_svg("nope", 20), web_svg(DEFAULT, 20));
        assert_eq!(gtk_name("user"), "ndl-user-symbolic");
        assert_eq!(gtk_name("<script>"), "ndl-workspace-symbolic");
    }

    #[test]
    fn gtk_icons_are_symbolic_shapes() {
        let backpack = get("backpack").unwrap();
        let svg = gtk_svg(backpack);
        assert!(svg.contains("foreground-stroke") && !svg.contains("currentColor"));
        let pad = gtk_svg(get("gamepad").unwrap());
        assert!(pad.contains("foreground-fill"));
        assert!(pad.matches("<circle").count() == 2);
    }

    #[test]
    fn a_saved_icon_must_be_one_of_ours() {
        assert_eq!(clean("backpack"), "backpack");
        assert_eq!(clean("🚀"), "");
        assert_eq!(clean("x\" onclick=\"y"), "");
    }
}
