//! Installs the icons NDL has no drawing for yet (see `brujula_core::icons`) next to the ones it
//! has, so a space can use a backpack or a briefcase like any other `ndl-…-symbolic` icon.

use brujula_core::icons;
use nodalia_gtk::gtk;

use crate::paths;

/// Writes the extra icons under the cache and adds that folder to the icon search path.
/// Without a display (tests) or a writable cache the sidebar falls back to a generic icon.
pub fn install() {
    let root = paths::cache_dir().join("icons");
    let actions = root.join("hicolor").join("scalable").join("actions");
    if std::fs::create_dir_all(&actions).is_err() {
        return;
    }
    for icon in icons::ICONS.iter().filter(|i| i.drawn_here) {
        let svg = icons::gtk_svg(icon);
        let file = actions.join(format!("ndl-{}-symbolic.svg", icon.id));
        if std::fs::read_to_string(&file).ok().as_deref() != Some(svg.as_str()) {
            let _ = std::fs::write(&file, svg);
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_a_space_can_have_is_in_ndl_or_installed_here() {
        for icon in icons::ICONS {
            let name = icons::gtk_name(icon.id);
            assert!(
                icon.drawn_here || nodalia_gtk::has_icon(&name),
                "{} is neither in NDL nor drawn here",
                icon.id
            );
        }
    }

    #[test]
    fn the_icons_drawn_here_are_not_in_ndl_yet() {
        // When NDL adds one, drop it from the table here so there is one drawing of it.
        for icon in icons::ICONS.iter().filter(|i| i.drawn_here) {
            assert!(
                !nodalia_gtk::has_icon(&icons::gtk_name(icon.id)),
                "{}",
                icon.id
            );
        }
    }
}
