//! The commands the Spotlight offers when the text matches what they do, like Arc's command
//! bar: type "capt" and "Capturar la página" is there. Each one is an action of the window, so
//! it does exactly what its menu entry or shortcut does.

/// One command: the action it runs, how it is called and found, and its shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// The action to activate, with its prefix (`win.find`).
    pub action: &'static str,
    pub title: &'static str,
    /// Other words that find it.
    pub keywords: &'static str,
    pub shortcut: &'static str,
    pub icon: &'static str,
}

pub const COMMANDS: &[Command] = &[
    Command {
        action: "win.find",
        title: "Buscar en la página",
        keywords: "encontrar texto palabra",
        shortcut: "Ctrl+F",
        icon: "ndl-search-symbolic",
    },
    Command {
        action: "win.copy-link",
        title: "Copiar el enlace de la página",
        keywords: "url direccion portapapeles",
        shortcut: "Ctrl+Mayús+C",
        icon: "ndl-link-symbolic",
    },
    Command {
        action: "win.screenshot",
        title: "Capturar la página",
        keywords: "captura pantallazo imagen foto screenshot",
        shortcut: "Ctrl+Mayús+S",
        icon: "ndl-camera-symbolic",
    },
    Command {
        action: "win.toggle-mute",
        title: "Silenciar o dar sonido a la pestaña",
        keywords: "mute audio volumen sonido",
        shortcut: "Ctrl+M",
        icon: "ndl-mute-symbolic",
    },
    Command {
        action: "win.zoom-in",
        title: "Acercar la página",
        keywords: "zoom aumentar mas grande",
        shortcut: "Ctrl++",
        icon: "ndl-plus-symbolic",
    },
    Command {
        action: "win.zoom-out",
        title: "Alejar la página",
        keywords: "zoom reducir mas pequeña",
        shortcut: "Ctrl+-",
        icon: "ndl-minus-symbolic",
    },
    Command {
        action: "win.zoom-reset",
        title: "Zoom normal",
        keywords: "restablecer 100 tamaño",
        shortcut: "Ctrl+0",
        icon: "ndl-fit-symbolic",
    },
    Command {
        action: "win.print",
        title: "Imprimir la página",
        keywords: "pdf papel impresora",
        shortcut: "Ctrl+P",
        icon: "ndl-print-symbolic",
    },
    Command {
        action: "win.fullscreen",
        title: "Pantalla completa",
        keywords: "ventana maximizar",
        shortcut: "F11",
        icon: "ndl-fullscreen-symbolic",
    },
    Command {
        action: "win.inspector",
        title: "Inspeccionar la página",
        keywords: "desarrollador consola codigo devtools",
        shortcut: "F12",
        icon: "ndl-code-symbolic",
    },
    Command {
        action: "win.new-tab",
        title: "Nueva pestaña",
        keywords: "abrir",
        shortcut: "Ctrl+T",
        icon: "ndl-plus-symbolic",
    },
    Command {
        action: "win.reopen-tab",
        title: "Reabrir la pestaña cerrada",
        keywords: "restaurar deshacer cerrar",
        shortcut: "Ctrl+Mayús+T",
        icon: "ndl-history-symbolic",
    },
    Command {
        action: "app.new-private-window",
        title: "Nueva ventana privada",
        keywords: "incognito navegacion",
        shortcut: "Ctrl+Mayús+N",
        icon: "ndl-eye-off-symbolic",
    },
    Command {
        action: "win.toggle-sidebar",
        title: "Mostrar u ocultar la barra lateral",
        keywords: "pestañas anclar",
        shortcut: "Ctrl+B",
        icon: "ndl-sidebar-symbolic",
    },
    Command {
        action: "win.toggle-history",
        title: "Historial",
        keywords: "visitadas paginas",
        shortcut: "Ctrl+H",
        icon: "ndl-history-symbolic",
    },
    Command {
        action: "win.add-essential",
        title: "Añadir a esenciales",
        keywords: "fijar favoritos",
        shortcut: "",
        icon: "ndl-pin-symbolic",
    },
    Command {
        action: "win.open-drm",
        title: "Abrir con soporte de DRM (Netflix…)",
        keywords: "widevine netflix disney spotify video protegido",
        shortcut: "",
        icon: "ndl-shield-check-symbolic",
    },
];

/// Lower case, without accents, so "pagina" finds "página".
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'Á' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// The commands that match `text`: every word typed has to start a word of the title or the
/// keywords. At least two letters are needed, so a single key does not bring a list up. Titles
/// that match come before keywords that match.
pub fn search(text: &str, limit: usize) -> Vec<&'static Command> {
    let query = plain(text.trim());
    if query.chars().count() < 2 {
        return Vec::new();
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut hits: Vec<(usize, &'static Command)> = COMMANDS
        .iter()
        .filter_map(|c| {
            let title = plain(c.title);
            let keywords = plain(c.keywords);
            let starts = |haystack: &str, w: &str| {
                haystack
                    .split(|ch: char| !ch.is_alphanumeric())
                    .any(|h| h.starts_with(w))
            };
            let all = words
                .iter()
                .all(|w| starts(&title, w) || starts(&keywords, w));
            if !all {
                return None;
            }
            let in_title = words.iter().all(|w| starts(&title, w));
            Some((usize::from(!in_title), c))
        })
        .collect();
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, c)| c).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_the_start_of_a_word_finds_the_command() {
        let found = search("capt", 5);
        assert_eq!(found[0].action, "win.screenshot");
        assert_eq!(search("imprim", 5)[0].action, "win.print");
    }

    #[test]
    fn accents_and_case_do_not_matter() {
        assert_eq!(search("PAGINA", 20).len(), search("página", 20).len());
        assert!(search("pestana", 5)
            .iter()
            .any(|c| c.action == "win.new-tab"));
    }

    #[test]
    fn every_word_has_to_match_and_keywords_count() {
        assert!(search("zoom normal", 5)
            .iter()
            .any(|c| c.action == "win.zoom-reset"));
        assert_eq!(search("netflix", 5)[0].action, "win.open-drm");
        assert!(search("zzzz", 5).is_empty());
    }

    #[test]
    fn one_letter_and_nothing_bring_no_commands() {
        assert!(search("", 5).is_empty());
        assert!(search("c", 5).is_empty());
        assert!(search("   ", 5).is_empty());
    }

    #[test]
    fn titles_come_before_keywords_and_the_limit_holds() {
        // "pantalla" is in a title; "captura" only in keywords of another command.
        let found = search("zoom", 2);
        assert_eq!(found.len(), 2);
        let all = search("pagina", 50);
        assert!(all.len() > 3);
        assert_eq!(search("pagina", 3).len(), 3);
    }

    #[test]
    fn actions_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(c.action.starts_with("win.") || c.action.starts_with("app."));
            assert!(seen.insert(c.action), "{}", c.action);
            assert!(!c.title.is_empty() && !c.icon.is_empty());
        }
    }
}
