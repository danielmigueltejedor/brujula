//! The assistant's API keys, kept in the system keyring (GNOME Keyring, KWallet or any other
//! Secret Service) through `secret-tool`. A key is never written to a file of Brújula, never
//! put in an address and never printed: it goes to the keyring through the standard input of
//! `secret-tool`, so it is not in the process list either, and is read back only for the moment
//! a request needs it.

use nodalia_gtk::gtk::gio;
use zeroize::Zeroizing;

/// Longest key accepted. Real ones are far shorter.
const MAX_KEY: usize = 512;

fn attributes(provider: &str) -> Vec<String> {
    vec![
        "service".to_owned(),
        "brujula".to_owned(),
        "kind".to_owned(),
        "ai-key".to_owned(),
        "provider".to_owned(),
        provider.to_owned(),
        "profile".to_owned(),
        crate::paths::profile().to_owned(),
    ]
}

/// A key as the user pasted it: one word, printable.
pub fn clean_key(text: &str) -> Option<Zeroizing<String>> {
    let key = text.trim();
    (!key.is_empty() && key.len() <= MAX_KEY && key.chars().all(|c| c.is_ascii_graphic()))
        .then(|| Zeroizing::new(key.to_owned()))
}

/// Runs `secret-tool` with `args`, giving it `input` on its standard input. Returns whether it
/// succeeded and what it printed.
async fn run(args: Vec<String>, input: Option<&str>) -> Result<(bool, Zeroizing<String>), String> {
    let mut argv: Vec<&str> = vec!["secret-tool"];
    argv.extend(args.iter().map(String::as_str));
    let argv: Vec<&std::ffi::OsStr> = argv.into_iter().map(std::ffi::OsStr::new).collect();
    let flags = gio::SubprocessFlags::STDOUT_PIPE
        | gio::SubprocessFlags::STDERR_SILENCE
        | if input.is_some() {
            gio::SubprocessFlags::STDIN_PIPE
        } else {
            gio::SubprocessFlags::STDIN_INHERIT
        };
    let process = gio::Subprocess::newv(&argv, flags).map_err(|_| {
        "Falta la herramienta secret-tool (paquete libsecret) para usar el llavero del sistema"
            .to_owned()
    })?;
    let (out, _) = process
        .communicate_utf8_future(input.map(str::to_owned))
        .await
        .map_err(|_| "No se pudo hablar con el llavero".to_owned())?;
    Ok((
        process.is_successful(),
        Zeroizing::new(out.map(|s| s.to_string()).unwrap_or_default()),
    ))
}

/// Keeps `key` for `provider`, replacing the one there was.
pub async fn store(provider: &str, label: &str, key: &str) -> Result<(), String> {
    let mut args = vec!["store".to_owned(), format!("--label={label}")];
    args.extend(attributes(provider));
    let (ok, _) = run(args, Some(key)).await?;
    if ok {
        Ok(())
    } else {
        Err("El llavero del sistema no guardó la clave (¿está desbloqueado?)".to_owned())
    }
}

/// The key of `provider`, if there is one.
pub async fn lookup(provider: &str) -> Option<Zeroizing<String>> {
    let mut args = vec!["lookup".to_owned()];
    args.extend(attributes(provider));
    let (ok, out) = run(args, None).await.ok()?;
    if !ok {
        return None;
    }
    let key = out.trim_end_matches(['\n', '\r']);
    clean_key(key)
}

/// Forgets the key of `provider`.
pub async fn clear(provider: &str) -> Result<(), String> {
    let mut args = vec!["clear".to_owned()];
    args.extend(attributes(provider));
    run(args, None).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_one_printable_word() {
        assert_eq!(clean_key("  sk-abc_123 \n").unwrap().as_str(), "sk-abc_123");
        assert!(clean_key("").is_none());
        assert!(clean_key("   ").is_none());
        assert!(clean_key("two words").is_none());
        assert!(clean_key("clave\u{7}").is_none());
        assert!(clean_key("clavé").is_none());
        assert!(clean_key(&"k".repeat(MAX_KEY + 1)).is_none());
    }

    #[test]
    fn a_key_is_filed_by_provider_and_profile_never_by_its_value() {
        let a = attributes("openai");
        assert!(a.contains(&"openai".to_owned()));
        assert!(a.contains(&"profile".to_owned()));
        assert_eq!(a.len() % 2, 0);
    }
}
