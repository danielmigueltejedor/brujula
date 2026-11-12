//! Icons for the things a user names: a space, a search engine.
//!
//! Spaces take their icon from this set, which is drawn the way Nodalia Design draws (24 px
//! grid, 3 px of margin, 1.75 px round strokes, one colour). Most of it is NDL's own; the few
//! NDL has no icon for yet (a backpack, a briefcase…) are drawn here in the same style and
//! marked `drawn_here`, so the browser installs them in the icon theme until NDL has them.
//!
//! The same shapes serve the GTK sidebar (as `ndl-<id>-symbolic`) and the HTML pages (inline
//! SVG), so the user sees one icon everywhere.
//!
//! The drawings are data, in `icons.txt`; this module reads them once.

use std::sync::LazyLock;

/// One icon of the set.
pub struct Icon {
    pub id: &'static str,
    pub label: &'static str,
    /// Not in NDL (yet): the browser must install it in the icon theme.
    pub drawn_here: bool,
    /// The SVG elements, in a 24 x 24 box, drawn with `currentColor`.
    pub body: Vec<&'static str>,
}

/// The icon a space has until its owner picks one.
pub const DEFAULT: &str = "workspace";

const DATA: &str = include_str!("icons.txt");

/// Splits `icons.txt` into the icons the user can pick and the ones the pages use themselves.
static SETS: LazyLock<(Vec<Icon>, Vec<Icon>)> = LazyLock::new(|| {
    let (mut picked, mut ui) = (Vec::new(), Vec::new());
    let mut in_ui = false;
    for line in DATA.lines().map(str::trim_end) {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line == "@ui" {
            in_ui = true;
        } else if let Some(header) = line.strip_prefix('[') {
            let Some((id, label)) = header.split_once("] ") else {
                continue;
            };
            let (label, drawn_here) = match label.strip_suffix(" drawn") {
                Some(label) => (label, true),
                None => (label, false),
            };
            let set = if in_ui { &mut ui } else { &mut picked };
            set.push(Icon {
                id,
                label,
                drawn_here,
                body: Vec::new(),
            });
        } else if let Some(icon) = if in_ui {
            ui.last_mut()
        } else {
            picked.last_mut()
        } {
            icon.body.push(line);
        }
    }
    (picked, ui)
});

/// The icons a space can have, in the order the pickers show them.
pub fn all() -> &'static [Icon] {
    &SETS.0
}

/// Icons the pages use for their own controls; the user does not pick them.
pub fn ui() -> &'static [Icon] {
    &SETS.1
}

pub fn get(id: &str) -> Option<&'static Icon> {
    all().iter().chain(ui()).find(|i| i.id == id)
}

/// Whether the user may give `id` to a space.
pub fn is_known(id: &str) -> bool {
    all().iter().any(|i| i.id == id)
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
        for (n, icon) in all().iter().enumerate() {
            assert!(
                all()[n + 1..].iter().all(|o| o.id != icon.id),
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
