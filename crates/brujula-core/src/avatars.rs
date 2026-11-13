//! The pictures a person can choose for their profile: a coloured disc with a simple white
//! glyph. They are drawn here as SVG so the welcome page and the profile chooser show the same.

use std::sync::LazyLock;

/// One avatar: its id (what is saved), its colour and the glyph, drawn in a 24x24 box.
pub struct Avatar {
    pub id: &'static str,
    pub name: &'static str,
    pub color: &'static str,
    glyph: &'static str,
}

/// One avatar per line: id, name, colour and glyph, separated by `|`.
const TABLE: &str = r##"compass|Brújula|#0c9a58|<circle cx="12" cy="12" r="8.5" fill="none" stroke="#fff" stroke-width="1.9"/><path d="M15.5 8.5l-2 5-5 2 2-5z" fill="#fff"/>
star|Estrella|#ca8a04|<path d="M12 3.5l2.5 5.3 5.8.8-4.2 4.1 1 5.8L12 16.7 6.9 19.5l1-5.8-4.2-4.1 5.8-.8z" fill="#fff"/>
moon|Luna|#4f46e5|<path d="M19 14.5A7.5 7.5 0 019.5 5a7.5 7.5 0 109.5 9.500z" fill="#fff"/>
sun|Sol|#ea580c|<circle cx="12" cy="12" r="4" fill="#fff"/><path d="M12 3v2.500M12 18.500V21M3 12h2.500M18.500 12H21M5.600 5.600l1.800 1.800M16.600 16.600l1.800 1.800M5.600 18.400l1.800-1.800M16.600 7.400l1.800-1.800" fill="none" stroke="#fff" stroke-width="1.900" stroke-linecap="round"/>
leaf|Hoja|#059669|<path d="M5 19c0-8 5-13 14-14 0 9-5 14-13 14z" fill="#fff"/><path d="M5 19l8-8" fill="none" stroke="#059669" stroke-width="1.500" stroke-linecap="round"/>
wave|Ola|#0891b2|<path d="M3 9c3-3 5 3 9 0s6 3 9 0M3 15c3-3 5 3 9 0s6 3 9 0" fill="none" stroke="#fff" stroke-width="1.900" stroke-linecap="round" stroke-linejoin="round"/>
bolt|Rayo|#7c3aed|<path d="M13 3L6 13.500h5L10 21l8-11h-5.500z" fill="#fff"/>
heart|Corazón|#db2777|<path d="M12 19.500S4.500 15 4.500 9.500A4 4 0 0112 7.800 4 4 0 0119.500 9.500C19.500 15 12 19.500 12 19.500z" fill="#fff"/>
mountain|Montaña|#475569|<path d="M3 19l6.500-11 3.500 6 2-3L21 19z" fill="#fff"/>
flag|Bandera|#dc2626|<path d="M6 21V4" fill="none" stroke="#fff" stroke-width="1.900" stroke-linecap="round"/><path d="M6 5h11l-2.500 4 2.500 4H6z" fill="#fff"/>"##;

pub static AVATARS: LazyLock<Vec<Avatar>> = LazyLock::new(|| {
    TABLE
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '|');
            Some(Avatar {
                id: parts.next()?,
                name: parts.next()?,
                color: parts.next()?,
                glyph: parts.next()?,
            })
        })
        .collect()
});

/// The avatar with this id, or the first one when the id is not known.
pub fn get(id: &str) -> &'static Avatar {
    AVATARS.iter().find(|a| a.id == id).unwrap_or(&AVATARS[0])
}

pub fn is_known(id: &str) -> bool {
    AVATARS.iter().any(|a| a.id == id)
}

impl Avatar {
    /// The picture as a standalone SVG of `size` pixels.
    pub fn svg(&self, size: u32) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 24 24"><circle cx="12" cy="12" r="12" fill="{}"/><g transform="translate(3.600 3.600) scale(.7)">{}</g></svg>"#,
            self.color, self.glyph
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_avatar_is_distinct_and_makes_a_closed_svg() {
        let mut ids: Vec<&str> = AVATARS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), AVATARS.len());
        for a in AVATARS.iter() {
            let svg = a.svg(64);
            assert!(
                svg.starts_with("<svg") && svg.ends_with("</svg>"),
                "{}",
                a.id
            );
            assert_eq!(
                svg.matches('<').count(),
                svg.matches('>').count(),
                "{}",
                a.id
            );
        }
    }

    #[test]
    fn an_unknown_avatar_falls_back_to_the_first() {
        assert_eq!(get("no-existe").id, AVATARS[0].id);
        assert!(is_known("moon") && !is_known("../x"));
    }
}
