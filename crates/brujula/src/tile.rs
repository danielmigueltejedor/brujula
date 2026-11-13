//! The icon of a web app: the site's own icon on a rounded square, like the app icons of the
//! Colloid theme, instead of a bare favicon that looks lost in a launcher.

use std::path::Path;

use nodalia_gtk::gtk::gdk;
use nodalia_gtk::gtk::gdk::cairo;
use nodalia_gtk::gtk::gdk::prelude::*;

/// Size of the finished icon.
const SIZE: i32 = 256;
/// Space around the square, so it does not touch the edge of the image.
const MARGIN: f64 = 12.0;

/// How light the icon is, 0 (black) to 1 (white), counting only what is not transparent.
fn luminance(texture: &gdk::Texture) -> f64 {
    let (w, h) = (texture.width() as usize, texture.height() as usize);
    let stride = w * 4;
    let mut data = vec![0u8; stride * h];
    texture.download(&mut data, stride);
    let (mut sum, mut weight) = (0.0, 0.0);
    for px in data.chunks_exact(4) {
        // Premultiplied BGRA: undo the premultiplication to get the colour itself.
        let a = f64::from(px[3]) / 255.0;
        if a < 0.2 {
            continue;
        }
        let (b, g, r) = (
            f64::from(px[0]) / 255.0 / a,
            f64::from(px[1]) / 255.0 / a,
            f64::from(px[2]) / 255.0 / a,
        );
        sum += (0.2126 * r + 0.7152 * g + 0.0722 * b).min(1.0) * a;
        weight += a;
    }
    if weight == 0.0 {
        0.5
    } else {
        sum / weight
    }
}

fn rounded_square(cr: &cairo::Context, x: f64, y: f64, size: f64, radius: f64) {
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + size - radius, y + radius, radius, -pi / 2.0, 0.0);
    cr.arc(x + size - radius, y + size - radius, radius, 0.0, pi / 2.0);
    cr.arc(x + radius, y + size - radius, radius, pi / 2.0, pi);
    cr.arc(x + radius, y + radius, radius, pi, 3.0 * pi / 2.0);
    cr.close_path();
}

/// Draws the tile and writes it to `path` as a PNG.
pub fn write_tile(texture: &gdk::Texture, path: &Path) -> Result<(), String> {
    let side = f64::from(SIZE) - 2.0 * MARGIN;
    let radius = side * 0.24;
    let dark_site = luminance(texture) > 0.7;
    // A light icon (white logos are common) goes on a dark square, anything else on a light one.
    let (top, bottom) = if dark_site {
        ((0.20, 0.22, 0.29), (0.11, 0.12, 0.17))
    } else {
        ((0.98, 0.98, 1.0), (0.89, 0.90, 0.94))
    };

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SIZE, SIZE)
        .map_err(|e| e.to_string())?;
    {
        let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        // A soft shadow under the square.
        rounded_square(&cr, MARGIN, MARGIN + 4.0, side, radius);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.22);
        cr.fill().map_err(|e| e.to_string())?;
        // The square, lighter at the top.
        rounded_square(&cr, MARGIN, MARGIN, side, radius);
        let grad = cairo::LinearGradient::new(0.0, MARGIN, 0.0, MARGIN + side);
        grad.add_color_stop_rgb(0.0, top.0, top.1, top.2);
        grad.add_color_stop_rgb(1.0, bottom.0, bottom.1, bottom.2);
        cr.set_source(&grad).map_err(|e| e.to_string())?;
        cr.fill_preserve().map_err(|e| e.to_string())?;
        // A thin edge, to keep it apart from a light launcher.
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.14);
        cr.set_line_width(1.5);
        cr.stroke().map_err(|e| e.to_string())?;

        // The site's icon, centred, about 60 % of the square and never enlarged more than 8x.
        let (w, h) = (f64::from(texture.width()), f64::from(texture.height()));
        let scale = (side * 0.60 / w.max(h)).min(8.0);
        let mut icon =
            cairo::ImageSurface::create(cairo::Format::ARgb32, texture.width(), texture.height())
                .map_err(|e| e.to_string())?;
        {
            let stride = usize::try_from(icon.stride()).map_err(|e| e.to_string())?;
            let mut data = icon.data().map_err(|e| e.to_string())?;
            texture.download(&mut data, stride);
        }
        icon.mark_dirty();
        cr.save().map_err(|e| e.to_string())?;
        cr.translate(
            f64::from(SIZE) / 2.0 - w * scale / 2.0,
            f64::from(SIZE) / 2.0 - h * scale / 2.0,
        );
        cr.scale(scale, scale);
        cr.set_source_surface(&icon, 0.0, 0.0)
            .map_err(|e| e.to_string())?;
        cr.source().set_filter(cairo::Filter::Good);
        cr.paint().map_err(|e| e.to_string())?;
        cr.restore().map_err(|e| e.to_string())?;
    }
    // cairo's pixels are premultiplied BGRA, which is what a GDK memory texture takes.
    surface.flush();
    let stride = usize::try_from(surface.stride()).map_err(|e| e.to_string())?;
    let bytes = {
        let data = surface.data().map_err(|e| e.to_string())?;
        nodalia_gtk::gtk::glib::Bytes::from(&data[..])
    };
    let tex = gdk::MemoryTexture::new(
        SIZE,
        SIZE,
        gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &bytes,
        stride,
    );
    tex.save_to_png(path).map_err(|e| e.to_string())
}
