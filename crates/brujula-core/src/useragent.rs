//! What the browser says it is to the sites it visits.
//!
//! WebKitGTK's own user agent already says Linux, but it swaps in a Macintosh one for a few
//! sites (its quirks list), and then those sites hand out Mac downloads and shortcuts. Brújula
//! runs on Linux, so it always says so, with the engine's usual tokens so that sites keep
//! recognising it as a current WebKit.

pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_says_linux_and_never_mac() {
        assert!(USER_AGENT.contains("X11; Linux x86_64"));
        for word in ["Macintosh", "Mac OS", "iPhone", "Windows"] {
            assert!(!USER_AGENT.contains(word), "{word}");
        }
    }
}
