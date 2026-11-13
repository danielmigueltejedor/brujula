//! The QR code as a grid of squares, for the interface to draw however it likes.

use qrcode::{Color, EcLevel, QrCode};

/// A QR code: `size` × `size` squares, row by row, `true` where a square is dark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrMatrix {
    size: usize,
    dark: Vec<bool>,
}

impl QrMatrix {
    /// The code for `text`, or `None` when it is too long for any QR code.
    pub fn new(text: &str) -> Option<Self> {
        // Medium correction: a phone camera reads a screen easily, and a smaller code is
        // bigger on the same dialog.
        let code = QrCode::with_error_correction_level(text, EcLevel::M).ok()?;
        Some(Self {
            size: code.width(),
            dark: code
                .to_colors()
                .into_iter()
                .map(|c| c == Color::Dark)
                .collect(),
        })
    }

    /// How many squares a side has.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Whether the square in column `x` and row `y` is dark. Anything outside is light, which is
    /// the quiet zone the code needs around it.
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.dark[y * self.size + x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_starts_with_the_finder_pattern() {
        let m = QrMatrix::new("FIDO:/0123456789").unwrap();
        let last = m.size() - 1;
        // The top row of a finder pattern is seven dark squares, in three of the corners.
        for i in 0..7 {
            assert!(
                m.is_dark(i, 0) && m.is_dark(last - i, 0) && m.is_dark(i, last),
                "{i}"
            );
        }
        // The square after it is light: the separator.
        assert!(!m.is_dark(7, 0));
    }

    #[test]
    fn the_code_a_phone_has_to_scan_fits() {
        let device = libwebauthn::transport::cable::qr_code_device::CableQrCodeDevice::new_transient(
            libwebauthn::transport::cable::qr_code_device::QrCodeOperationHint::GetAssertionRequest,
            libwebauthn::transport::cable::qr_code_device::CableTransports::CloudAssistedOnly,
        )
        .unwrap();
        let text = device.qr_code.to_string();
        assert!(text.starts_with("FIDO:/"));
        let m = QrMatrix::new(&text).unwrap();
        // A big code is hard to scan from across a desk: this one stays small.
        assert!(m.size() <= 61, "{}", m.size());
    }

    #[test]
    fn outside_the_code_is_light() {
        let m = QrMatrix::new("hola").unwrap();
        assert!(!m.is_dark(m.size(), 0));
        assert!(!m.is_dark(0, m.size() + 5));
    }

    #[test]
    fn a_text_too_long_gives_nothing() {
        assert!(QrMatrix::new(&"1".repeat(10_000)).is_none());
    }
}
