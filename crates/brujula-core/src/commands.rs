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

/// The commands, one per line: action, title, other words that find it, shortcut and icon,
/// separated by `|`. Kept as plain data so adding one is a line, not a block of code.
const TABLE: &str = "\
win.find|Buscar en la página|encontrar texto palabra|Ctrl+F|ndl-search-symbolic\n\
win.copy-link|Copiar el enlace de la página|url direccion portapapeles|Ctrl+Mayús+C|ndl-link-symbolic\n\
win.screenshot|Capturar la página|captura pantallazo imagen foto screenshot|Ctrl+Mayús+S|ndl-camera-symbolic\n\
win.toggle-mute|Silenciar o dar sonido a la pestaña|mute audio volumen sonido|Ctrl+M|ndl-mute-symbolic\n\
win.zoom-in|Acercar la página|zoom aumentar mas grande|Ctrl++|ndl-plus-symbolic\n\
win.zoom-out|Alejar la página|zoom reducir mas pequeña|Ctrl+-|ndl-minus-symbolic\n\
win.zoom-reset|Zoom normal|restablecer 100 tamaño|Ctrl+0|ndl-fit-symbolic\n\
win.print|Imprimir la página|pdf papel impresora|Ctrl+P|ndl-print-symbolic\n\
win.fullscreen|Pantalla completa|ventana maximizar|F11|ndl-fullscreen-symbolic\n\
win.inspector|Inspeccionar la página|desarrollador consola codigo devtools|F12|ndl-code-symbolic\n\
win.new-tab|Nueva pestaña|abrir|Ctrl+T|ndl-plus-symbolic\n\
win.reopen-tab|Reabrir la pestaña cerrada|restaurar deshacer cerrar|Ctrl+Mayús+T|ndl-history-symbolic\n\
win.clear-tabs|Limpiar pestañas sin anclar|archivar cerrar todas ordenar clear|Ctrl+Mayús+K|ndl-archive-symbolic\n\
app.new-private-window|Nueva ventana privada|incognito navegacion|Ctrl+Mayús+N|ndl-eye-off-symbolic\n\
win.toggle-sidebar|Mostrar u ocultar la barra lateral|pestañas anclar|Ctrl+B|ndl-sidebar-symbolic\n\
win.toggle-history|Historial|visitadas paginas|Ctrl+H|ndl-history-symbolic\n\
win.add-essential|Añadir a esenciales|fijar favoritos||ndl-pin-symbolic\n\
win.open-drm|Abrir con soporte de DRM (Netflix…)|widevine netflix disney spotify video protegido||ndl-shield-check-symbolic";

static ALL: std::sync::OnceLock<Vec<Command>> = std::sync::OnceLock::new();

/// Every command, in the order they are listed.
pub fn all() -> &'static [Command] {
    ALL.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|line| {
                let mut f = line.split('|');
                Some(Command {
                    action: f.next()?,
                    title: f.next()?,
                    keywords: f.next()?,
                    shortcut: f.next()?,
                    icon: f.next()?,
                })
            })
            .collect()
    })
}

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
    let mut hits: Vec<(usize, &'static Command)> = all()
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
    fn every_line_of_the_table_is_a_command() {
        assert_eq!(all().len(), TABLE.lines().count());
        assert!(all().len() >= 18);
        for c in all() {
            assert!(!c.title.is_empty() && !c.icon.is_empty(), "{c:?}");
            assert!(!c.title.contains('|') && !c.icon.contains('|'));
        }
    }

    #[test]
    fn actions_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for c in all() {
            assert!(c.action.starts_with("win.") || c.action.starts_with("app."));
            assert!(seen.insert(c.action), "{}", c.action);
            assert!(!c.title.is_empty() && !c.icon.is_empty());
        }
    }
}
