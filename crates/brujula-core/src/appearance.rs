//! The style sheet that follows from the user's appearance settings.
//!
//! Everything here is plain GTK CSS built from validated numbers and colours (see
//! [`Appearance::sanitized`]), so a setting can never put arbitrary text in the sheet.

use crate::settings::{text_on, Appearance, Layout, TabPosition};

/// Dark ink on a light frame, light ink on a dark one.
fn text_on_frame(frame: &str) -> &'static str {
    if text_on(frame) == "#ffffff" {
        "#f2f4f5"
    } else {
        "#14181b"
    }
}

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
         .brujula-topbar {{ margin: {g}px {g}px {g}px {g}px; border-radius: {r}px; background-color: alpha(var(--ndl-color-surface-panel), {opacity:.2}); }}\n",
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
             window.brujula-frame .brujula-topbar {{ background-color: transparent; border-radius: 0; margin-bottom: 0; }}\n",
            r = a.radius,
        ));
        // The sidebar and the tab bar sit straight on the frame colour, so their text must read
        // on it whatever the theme is.
        if !a.frame_color.is_empty() {
            css.push_str(&format!(
                "window.brujula-frame .brujula-side, window.brujula-frame .brujula-topbar {{ color: {ink}; }}\n\
                 window.brujula-frame .brujula-side button.flat:not(:checked), window.brujula-frame .brujula-topbar button.flat:not(:checked) {{ color: inherit; }}\n\
                 window.brujula-frame .brujula-side button.flat:hover:not(:checked), window.brujula-frame .brujula-topbar button.flat:hover:not(:checked), window.brujula-frame .brujula-side row:hover:not(:selected), window.brujula-frame .brujula-side .brujula-space-current {{ background-color: alpha({ink}, 0.14); }}\n\
                 window.brujula-frame .brujula-side row:not(:selected), window.brujula-frame .brujula-side row:not(:selected) label, window.brujula-frame .brujula-side row:not(:selected) image {{ color: {ink}; }}\n",
                ink = text_on_frame(&a.frame_color),
            ));
        }
    }
    if a.tabs == TabPosition::Horizontal {
        // The pane around the tab bar and the page paints nothing: the tab bar is its own island
        // (or part of the frame) and the page is another one under it.
        css.push_str(&format!(
            "ndl-overlay-split-view.collapsed.brujula-horizontal > .ndl-content-pane {{ margin: 0; border-radius: 0; background-color: transparent; box-shadow: none; }}\n\
             .brujula-horizontal .brujula-page {{ margin: 0 {g}px {g}px {g}px; border-radius: {r}px; overflow: hidden; background-color: var(--ndl-color-surface-panel); }}\n",
            g = a.gap,
            r = a.radius,
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
    fn the_tab_bar_is_an_island_with_islands_and_part_of_the_frame_with_the_frame() {
        let islands = css(&Appearance {
            tabs: TabPosition::Horizontal,
            ..Appearance::default()
        });
        assert!(islands.contains(".brujula-topbar { margin: 8px 8px 8px 8px; border-radius: 16px; background-color: alpha(var(--ndl-color-surface-panel), 1.00)"));
        assert!(!islands.contains("brujula-frame .brujula-topbar"));
        let frame = css(&Appearance {
            tabs: TabPosition::Horizontal,
            layout: Layout::Frame,
            ..Appearance::default()
        });
        assert!(frame.contains("window.brujula-frame .brujula-topbar { background-color: transparent; border-radius: 0; margin-bottom: 0; }"));
    }

    #[test]
    fn text_on_the_frame_reads_on_the_frame_colour() {
        let on = |frame: &str| {
            css(&Appearance {
                layout: Layout::Frame,
                frame_color: frame.into(),
                ..Appearance::default()
            })
        };
        assert!(on("#dbe7e0").contains("color: #14181b;"));
        assert!(on("#1a2420").contains("color: #f2f4f5;"));
        // With the theme's own frame colour the theme's ink is already right.
        assert!(!on("").contains("color: #14181b;"));
        // And islands keep their own panels, so nothing is forced on them.
        assert!(!css(&Appearance {
            frame_color: "#dbe7e0".into(),
            ..Appearance::default()
        })
        .contains("color: #14181b;"));
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
    fn horizontal_tabs_keep_the_page_as_an_island() {
        let vertical = css(&Appearance::default());
        assert!(!vertical.contains("brujula-horizontal"));
        let top = css(&Appearance {
            tabs: TabPosition::Horizontal,
            gap: 6,
            ..Appearance::default()
        });
        assert!(top.contains(
            ".brujula-horizontal .brujula-page { margin: 0 6px 6px 6px; border-radius: 16px"
        ));
        assert!(top.contains(
            "> .ndl-content-pane { margin: 0; border-radius: 0; background-color: transparent"
        ));
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
