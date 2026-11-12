//! The style sheet that follows from the user's appearance settings.
//!
//! Everything here is plain GTK CSS built from validated numbers and colours (see
//! [`Appearance::sanitized`]), so a setting can never put arbitrary text in the sheet.

use crate::settings::{Appearance, Layout, TabPosition};

/// CSS that goes on top of the browser's own sheet. The window carries the class
/// `brujula-window` (and `brujula-frame` in the frame layout, `brujula-horizontal` with the tab
/// bar on top).
pub fn css(a: &Appearance) -> String {
    let a = a.clone().sanitized();
    let opacity = f64::from(a.opacity) / 100.0;
    let frame = if a.frame_color.is_empty() {
        "@window_bg_color".to_owned()
    } else {
        a.frame_color.clone()
    };
    let mut css = String::new();
    css.push_str(&format!(
        "window.brujula-window {{ background-color: alpha({frame}, {opacity:.2}); }}\n\
         ndl-overlay-split-view > .ndl-sidebar-pane,\n\
         ndl-overlay-split-view > .ndl-content-pane {{ border-radius: {r}px; margin: {g}px; }}\n\
         ndl-overlay-split-view > .ndl-sidebar-pane {{ background-color: alpha(var(--ndl-color-surface-panel), {opacity:.2}); }}\n\
         .brujula-topbar {{ margin: {g}px {g}px 0 {g}px; }}\n",
        r = a.radius,
        g = a.gap,
    ));
    // Sidebar not pinned: the page fills the window, whatever the layout.
    css.push_str(
        "ndl-overlay-split-view.collapsed > .ndl-content-pane { margin: 0; border-radius: 0; }\n",
    );
    if a.layout == Layout::Frame {
        // One island: the page. What surrounds it sits on the window colour.
        css.push_str(&format!(
            "window.brujula-frame ndl-overlay-split-view > .ndl-sidebar-pane {{ background-color: transparent; box-shadow: none; border-radius: 0; }}\n\
             window.brujula-frame ndl-overlay-split-view.collapsed > .ndl-sidebar-pane {{ background-color: alpha({frame}, 0.98); border-radius: {r}px; box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35); }}\n\
             window.brujula-frame .brujula-side {{ background-color: transparent; }}\n\
             window.brujula-frame .brujula-topbar {{ background-color: transparent; }}\n",
            r = a.radius,
        ));
    }
    if a.tabs == TabPosition::Horizontal {
        css.push_str(&format!(
            "window.brujula-horizontal ndl-overlay-split-view > .ndl-content-pane {{ margin-top: {g}px; }}\n",
            g = a.gap
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_end_up_in_the_sheet() {
        let a = Appearance {
            radius: 20,
            gap: 4,
            opacity: 80,
            ..Appearance::default()
        };
        let css = css(&a);
        assert!(css.contains("border-radius: 20px; margin: 4px"));
        assert!(css.contains("0.80"));
    }

    #[test]
    fn frame_layout_removes_the_sidebar_island() {
        let islands = css(&Appearance::default());
        assert!(!islands.contains("brujula-frame"));
        let frame = css(&Appearance {
            layout: Layout::Frame,
            ..Appearance::default()
        });
        assert!(frame.contains("brujula-frame ndl-overlay-split-view > .ndl-sidebar-pane"));
    }

    #[test]
    fn frame_colour_replaces_the_theme_window_colour() {
        let plain = css(&Appearance::default());
        assert!(plain.contains("@window_bg_color"));
        let tinted = css(&Appearance {
            frame_color: "#203040".into(),
            ..Appearance::default()
        });
        assert!(tinted.contains("alpha(#203040,"));
    }

    #[test]
    fn out_of_range_values_are_clamped_before_they_reach_the_sheet() {
        let css = css(&Appearance {
            radius: 9999,
            opacity: 0,
            accent: "x".into(),
            frame_color: "url(evil)".into(),
            ..Appearance::default()
        });
        assert!(css.contains("28px"));
        assert!(css.contains("0.50"));
        assert!(!css.contains("evil"));
    }
}
