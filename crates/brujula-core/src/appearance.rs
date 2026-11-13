//! The style sheet that follows from the user's appearance settings.
//!
//! Everything here is plain GTK CSS built from validated numbers and colours (see
//! [`Appearance::sanitized`]), so a setting can never put arbitrary text in the sheet.

use crate::settings::{mix_colors, text_on, Appearance, Layout, TabPosition, Texture};

/// Dark ink on a light frame, light ink on a dark one.
fn text_on_frame(frame: &str) -> &'static str {
    if text_on(frame) == "#ffffff" {
        "#f2f4f5"
    } else {
        "#14181b"
    }
}

/// The texture as a CSS background image and the size it repeats at, or `None` for no texture.
/// Mid grey with some alpha reads on a light frame and on a dark one.
fn texture_layer(a: &Appearance) -> Option<(String, &'static str)> {
    // (images, sizes), one size for each image.
    let strength = f64::from(a.texture_amount) / 100.0;
    let grey = |alpha: f64| format!("rgba(128, 128, 128, {alpha:.2})");
    match a.texture {
        Texture::None => None,
        Texture::Grain => Some((
            // Noise drawn by the SVG renderer; `%23` is the `#` of the filter's reference.
            format!(
                "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'>\
                 <filter id='n'><feTurbulence type='fractalNoise' baseFrequency='.85' numOctaves='2' stitchTiles='stitch'/>\
                 <feColorMatrix values='0 0 0 0 .5 0 0 0 0 .5 0 0 0 0 .5 0 0 0 {:.2} 0'/></filter>\
                 <rect width='160' height='160' filter='url(%23n)'/></svg>\")",
                0.15 + strength * 1.1
            ),
            "160px 160px",
        )),
        Texture::Lines => Some((
            format!(
                "repeating-linear-gradient(45deg, {c} 0px, {c} 1px, transparent 1px, transparent 6px)",
                c = grey(0.06 + strength * 0.2)
            ),
            "auto",
        )),
        Texture::Dots => Some((
            format!(
                "radial-gradient(circle, {c} 0px, {c} 1.2px, transparent 1.6px)",
                c = grey(0.12 + strength * 0.4)
            ),
            "12px 12px",
        )),
        Texture::Grid => Some((
            format!(
                "linear-gradient({c} 1px, transparent 1px), linear-gradient(90deg, {c} 1px, transparent 1px)",
                c = grey(0.04 + strength * 0.12)
            ),
            "24px 24px, 24px 24px",
        )),
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
    // A gradient replaces the plain colour of the frame; its first colour stands in wherever a
    // single one is needed. The texture goes over it.
    let fade = (a.frame_gradient.len() >= 2).then(|| {
        let stops: Vec<String> = a
            .frame_gradient
            .iter()
            .map(|c| format!("alpha({c}, {opacity:.2})"))
            .collect();
        format!(
            "linear-gradient({}deg, {})",
            a.frame_angle,
            stops.join(", ")
        )
    });
    let texture = texture_layer(&a);
    let images: Vec<&str> = texture
        .iter()
        .map(|(i, _)| i.as_str())
        .chain(fade.as_deref())
        .collect();
    let sizes: Vec<&str> = texture
        .iter()
        .map(|(_, s)| *s)
        .chain(fade.as_ref().map(|_| "auto"))
        .collect();
    let gradient = if images.is_empty() {
        String::new()
    } else {
        format!(
            " background-image: {}; background-size: {};",
            images.join(", "),
            sizes.join(", ")
        )
    };
    let frame = a.frame_gradient.first().cloned().unwrap_or(frame);
    css.push_str(&format!(
        "window.brujula-window {{ background-color: alpha({frame}, {opacity:.2});{gradient} }}\n\
         ndl-overlay-split-view > .ndl-sidebar-pane,\n\
         ndl-overlay-split-view > .ndl-content-pane {{ border-radius: {r}px; margin: {g}px; }}\n\
         ndl-overlay-split-view > .ndl-sidebar-pane {{ background-color: alpha(var(--ndl-color-surface-panel), {opacity:.2}); }}\n\
         .brujula-topbar {{ margin: {g}px {g}px {g}px {g}px; border-radius: {r}px; background-color: alpha(var(--ndl-color-surface-panel), {opacity:.2}); }}\n",
        r = a.radius,
        g = a.gap,
    ));
    if let (Some((image, size)), Layout::Islands) = (&texture, a.layout) {
        css.push_str(&format!(
            "ndl-overlay-split-view > .ndl-sidebar-pane, .brujula-topbar {{ background-image: {image}; background-size: {size}; }}\n"
        ));
    }
    // Dialogs and their rounded corners follow the look chosen: a veil of the frame's colours
    // over the panel, so a gradient reaches them too.
    css.push_str(&format!(
        "ndl-dialog {{ border-radius: {}px; }}\n",
        a.radius
    ));
    let veil = if a.frame_gradient.len() >= 2 {
        let stops: Vec<String> = a
            .frame_gradient
            .iter()
            .map(|c| format!("alpha({c}, 0.16)"))
            .collect();
        format!(
            "linear-gradient({}deg, {})",
            a.frame_angle,
            stops.join(", ")
        )
    } else if !a.frame_color.is_empty() {
        format!(
            "linear-gradient(alpha({c}, 0.14), alpha({c}, 0.14))",
            c = a.frame_color
        )
    } else {
        String::new()
    };
    if !veil.is_empty() {
        css.push_str(&format!("ndl-dialog {{ background-image: {veil}; }}\n"));
    }
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
        let ink_base = if a.frame_gradient.is_empty() {
            a.frame_color.clone()
        } else {
            mix_colors(&a.frame_gradient).unwrap_or_default()
        };
        if !ink_base.is_empty() {
            css.push_str(&format!(
                "window.brujula-frame .brujula-side, window.brujula-frame .brujula-topbar {{ color: {ink}; }}\n\
                 window.brujula-frame .brujula-side button.flat:not(:checked), window.brujula-frame .brujula-topbar button.flat:not(:checked) {{ color: inherit; }}\n\
                 window.brujula-frame .brujula-side button.flat:hover:not(:checked), window.brujula-frame .brujula-topbar button.flat:hover:not(:checked), window.brujula-frame .brujula-side row:hover:not(:selected), window.brujula-frame .brujula-side .brujula-space-current {{ background-color: alpha({ink}, 0.14); }}\n\
                 window.brujula-frame .brujula-side row:not(:selected), window.brujula-frame .brujula-side row:not(:selected) label, window.brujula-frame .brujula-side row:not(:selected) image {{ color: {ink}; }}\n",
                ink = text_on_frame(&ink_base),
            ));
        }
    }
    if a.tabs == TabPosition::Horizontal {
        // The pane around the tab bar and the page paints nothing: the tab bar is its own island
        // (or part of the frame) and the page is another one under it.
        css.push_str(&format!(
            "ndl-overlay-split-view.collapsed.brujula-horizontal > .ndl-content-pane {{ margin: 0; border-radius: 0; background-color: transparent; box-shadow: none; }}\n\
             .brujula-horizontal .brujula-page {{ margin: 0 {g}px {g}px {g}px; border-radius: {r}px; overflow: hidden; background-color: var(--ndl-color-surface-panel); }}\n\
             .brujula-horizontal .brujula-rail {{ margin: 0 0 {g}px {g}px; border-radius: {r}px; background-color: alpha(var(--ndl-color-surface-panel), {opacity:.2}); }}\n",
            g = a.gap,
            r = a.radius,
        ));
        if a.layout == Layout::Frame {
            css.push_str("window.brujula-frame .brujula-rail { background-color: transparent; }\n");
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogs_wear_the_gradient_and_the_corners_of_the_look() {
        let plain = css(&Appearance::default());
        assert!(plain.contains("ndl-dialog { border-radius: 16px; }"));
        assert!(!plain.contains("ndl-dialog { background-image"));
        let gradient = css(&Appearance {
            frame_gradient: vec!["#112233".into(), "#445566".into()],
            frame_angle: 90,
            radius: 24,
            ..Appearance::default()
        });
        assert!(gradient.contains("ndl-dialog { border-radius: 24px; }"));
        assert!(gradient.contains(
            "ndl-dialog { background-image: linear-gradient(90deg, alpha(#112233, 0.16), alpha(#445566, 0.16)); }"
        ));
        let solid = css(&Appearance {
            frame_color: "#abcdef".into(),
            ..Appearance::default()
        });
        assert!(
            solid.contains("ndl-dialog { background-image: linear-gradient(alpha(#abcdef, 0.14)")
        );
    }

    #[test]
    fn a_texture_goes_over_the_gradient_and_is_never_text_from_outside() {
        let none = css(&Appearance::default());
        assert!(!none.contains("repeating-linear-gradient") && !none.contains("data:image"));
        let lines = css(&Appearance {
            texture: Texture::Lines,
            frame_gradient: vec!["#112233".into(), "#445566".into()],
            ..Appearance::default()
        });
        assert!(lines.contains("background-image: repeating-linear-gradient(45deg"));
        assert!(lines.contains("linear-gradient(135deg, alpha(#112233"));
        assert!(lines.contains("background-size: auto, auto;"));
        let grain = css(&Appearance {
            texture: Texture::Grain,
            ..Appearance::default()
        });
        assert!(grain.contains("feTurbulence") && grain.contains("160px 160px"));
        // The `%` of a data address must be an escape: GTK refuses the sheet otherwise.
        assert!(!grain.contains("%'") && grain.contains("%23n"));
        let grid = css(&Appearance {
            texture: Texture::Grid,
            ..Appearance::default()
        });
        assert!(grid.contains("24px 24px, 24px 24px"));
        // Stronger is more visible.
        let faint = css(&Appearance {
            texture: Texture::Dots,
            texture_amount: 10,
            ..Appearance::default()
        });
        let strong = css(&Appearance {
            texture: Texture::Dots,
            texture_amount: 100,
            ..Appearance::default()
        });
        assert_ne!(faint, strong);
    }

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
    fn a_gradient_replaces_the_frame_colour_and_sets_the_ink_from_its_mean() {
        let plain = css(&Appearance::default());
        assert!(!plain.contains("linear-gradient"));
        let dark = css(&Appearance {
            layout: Layout::Frame,
            frame_gradient: vec!["#101830".into(), "#201040".into()],
            frame_angle: 90,
            ..Appearance::default()
        });
        assert!(dark.contains("linear-gradient(90deg, alpha(#101830, 1.00), alpha(#201040, 1.00))"));
        assert!(dark.contains("color: #f2f4f5"));
        let light = css(&Appearance {
            layout: Layout::Frame,
            frame_gradient: vec!["#f0f0f0".into(), "#e0e8f0".into()],
            ..Appearance::default()
        });
        assert!(light.contains("color: #14181b"));
        // One colour is not a gradient.
        let one = css(&Appearance {
            frame_gradient: vec!["#101830".into()],
            ..Appearance::default()
        });
        assert!(!one.contains("linear-gradient"));
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
