//! The typefaces of Nodalia Design for the browser's own pages: Inter for the text and Sora for
//! the titles, as every Nodalia app. The browser serves the files at `brujula-font://`; where
//! they cannot be loaded the system's own typeface stands in.

/// The CSS that declares the two typefaces and puts Sora on the titles. It goes first in a
/// page's style sheet.
pub const FACES: &str = "@font-face{font-family:Inter;font-weight:100 900;src:url(brujula-font://inter) format('truetype')}\
@font-face{font-family:Sora;font-weight:100 800;src:url(brujula-font://sora) format('truetype')}\
h1,h2{font-family:Sora,Inter,\"Adwaita Sans\",system-ui,sans-serif}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pages_name_the_two_faces_and_have_a_fallback() {
        assert!(FACES.contains("brujula-font://inter") && FACES.contains("brujula-font://sora"));
        assert!(FACES.contains("system-ui"));
    }
}
