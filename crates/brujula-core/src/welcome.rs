//! The welcome page shown the first time Brújula opens: a typed greeting in the system's
//! language, then bookmarks, default browser, essentials and (when there is no profile yet) the
//! first profile. The page keeps its steps on its own; what needs the browser (reading files,
//! setting the default, saving) goes out as `brujula://welcome?key=value` addresses.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::essentials::SUGGESTED;
use crate::newtab::escape;

pub const WELCOME_URI: &str = "brujula://welcome";
pub const ACTION_PREFIX: &str = "brujula://welcome?";
/// The longest `finish` payload read: a name, a password and a dozen addresses fit well within.
const MAX_PAYLOAD: usize = 8 * 1024;

/// Something found on this computer that bookmarks can be read from.
pub struct Source {
    pub name: String,
}

pub struct Page<'a> {
    pub lang: &'a str,
    pub theme: &'a str,
    pub accent: &'a str,
    pub sources: &'a [Source],
    /// The app icons the icon theme has for the suggested sites, as `data:` addresses: site
    /// address and picture. A site without one gets a letter on its brand colour.
    pub icons: &'a [(String, String)],
    /// Whether the last step makes the first profile (false once there is one).
    pub ask_profile: bool,
}

/// What the page asks the browser to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Read the bookmarks of the n-th detected source.
    Import(usize),
    /// Ask for a file to read bookmarks from.
    ImportFile,
    DefaultBrowser,
    Finish(Finish),
    /// Leave without choosing anything.
    Skip,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Finish {
    pub name: String,
    pub avatar: String,
    pub password: String,
    pub essentials: Vec<String>,
}

/// The action in a `brujula://welcome?key=value` address.
pub fn parse_action(uri: &str) -> Option<Action> {
    if !uri.starts_with(ACTION_PREFIX) {
        return None;
    }
    let url = url::Url::parse(uri).ok()?;
    let mut pairs = url.query_pairs();
    let (key, value) = pairs.next()?;
    if pairs.next().is_some() {
        return None;
    }
    match &*key {
        "import" if &*value == "file" => Some(Action::ImportFile),
        "import" => value.parse().ok().map(Action::Import),
        "default" => Some(Action::DefaultBrowser),
        "skip" => Some(Action::Skip),
        "finish" if value.len() <= MAX_PAYLOAD => {
            let mut f: Finish = serde_json::from_str(&value).ok()?;
            f.essentials.truncate(crate::essentials::MAX_ESSENTIALS);
            Some(Action::Finish(f))
        }
        _ => None,
    }
}

/// The texts of the page, in the order each language lists them.
#[derive(Clone, Copy)]
enum Text {
    Welcome,
    Tagline,
    Next,
    Back,
    Skip,
    ImportTitle,
    ImportHint,
    ImportNone,
    ImportFile,
    ImportFrom,
    DefaultTitle,
    DefaultHint,
    DefaultButton,
    EssTitle,
    EssHint,
    UserTitle,
    UserHint,
    Name,
    NamePh,
    Avatar,
    Password,
    PasswordPh,
    PasswordNote,
    PasswordShort,
    DoneTitle,
    DoneHint,
    DoneButton,
}

/// One language: its code and its texts.
struct Words {
    code: &'static str,
    lines: Vec<&'static str>,
}

impl Words {
    fn t(&self, text: Text) -> &'static str {
        self.lines[text as usize]
    }
}

/// Texts in the language `es`, one per line.
const ES: &str = "Bienvenido a Brújula\n\
Un navegador rápido, seguro y a tu medida. Vamos a dejarlo a tu gusto en un momento.\n\
Siguiente\n\
Atrás\n\
Omitir\n\
Trae tus datos\n\
Importa de otro navegador tus marcadores, historial, cookies y contraseñas para no empezar de cero. Las contraseñas se guardan cifradas con una contraseña maestra.\n\
No se ha encontrado ningún navegador. Puedes elegir un archivo de marcadores o de contraseñas (CSV).\n\
Elegir un archivo…\n\
Importar de\n\
Tu navegador de siempre\n\
Haz que los enlaces de otras aplicaciones se abran en Brújula.\n\
Usar Brújula como navegador predeterminado\n\
Tus esenciales\n\
Los sitios que abres todo el día, a un clic en la barra lateral. Elige los que quieras.\n\
¿Quién eres?\n\
Cada persona del equipo puede tener su perfil, con sus marcadores y pestañas.\n\
Nombre\n\
Tu nombre\n\
Imagen\n\
Contraseña (opcional)\n\
Déjala vacía si no quieres\n\
Se pedirá al elegir tu perfil. No cifra tus archivos: quien tenga acceso a esta cuenta del equipo podría leerlos.\n\
La contraseña necesita al menos 4 caracteres.\n\
Todo listo\n\
Puedes cambiar cualquiera de estas cosas en Preferencias cuando quieras.\n\
Empezar a navegar";

/// Texts in the language `en`, one per line.
const EN: &str = "Welcome to Brújula\n\
A fast, safe browser that is yours to shape. Let's set it up in a moment.\n\
Next\n\
Back\n\
Skip\n\
Bring your data\n\
Import your bookmarks, history, cookies and passwords from another browser so you do not start from scratch. Passwords are kept encrypted with a master password.\n\
No browser was found. You can pick a bookmarks file or a passwords file (CSV).\n\
Choose a file…\n\
Import from\n\
Your everyday browser\n\
Open the links from other apps in Brújula.\n\
Make Brújula the default browser\n\
Your essentials\n\
The sites you open all day, one click away in the sidebar. Pick the ones you want.\n\
Who are you?\n\
Everyone who shares this computer can have a profile with their own bookmarks and tabs.\n\
Name\n\
Your name\n\
Picture\n\
Password (optional)\n\
Leave empty for none\n\
It is asked when you pick your profile. It does not encrypt your files: anyone with access to this computer account could read them.\n\
The password needs at least 4 characters.\n\
All set\n\
You can change any of this in Preferences whenever you like.\n\
Start browsing";

/// Texts in the language `fr`, one per line.
const FR: &str = "Bienvenue sur Brújula\n\
Un navigateur rapide, sûr et à votre mesure. Réglons-le en un instant.\n\
Suivant\n\
Retour\n\
Passer\n\
Récupérez vos données\n\
Importez d'un autre navigateur vos favoris, votre historique, vos cookies et vos mots de passe pour ne pas repartir de zéro. Les mots de passe sont chiffrés par un mot de passe principal.\n\
Aucun navigateur trouvé. Vous pouvez choisir un fichier de favoris ou de mots de passe (CSV).\n\
Choisir un fichier…\n\
Importer depuis\n\
Votre navigateur de tous les jours\n\
Ouvrez dans Brújula les liens des autres applications.\n\
Définir Brújula comme navigateur par défaut\n\
Vos essentiels\n\
Les sites que vous ouvrez toute la journée, à un clic dans la barre latérale.\n\
Qui êtes-vous ?\n\
Chaque personne de l'ordinateur peut avoir son profil, avec ses favoris et ses onglets.\n\
Nom\n\
Votre nom\n\
Image\n\
Mot de passe (facultatif)\n\
Laissez vide pour aucun\n\
Il est demandé au choix du profil. Il ne chiffre pas vos fichiers : quiconque accède à ce compte pourrait les lire.\n\
Le mot de passe doit faire au moins 4 caractères.\n\
Tout est prêt\n\
Vous pourrez tout modifier dans les Préférences.\n\
Commencer à naviguer";

/// Texts in the language `de`, one per line.
const DE: &str = "Willkommen bei Brújula\n\
Ein schneller, sicherer Browser nach deinem Geschmack. Richten wir ihn kurz ein.\n\
Weiter\n\
Zurück\n\
Überspringen\n\
Daten mitbringen\n\
Importiere Lesezeichen, Verlauf, Cookies und Passwörter aus einem anderen Browser. Passwörter werden mit einem Master-Passwort verschlüsselt gespeichert.\n\
Kein Browser gefunden. Du kannst eine Lesezeichen- oder Passwort-Datei (CSV) wählen.\n\
Datei wählen …\n\
Importieren aus\n\
Dein Alltags-Browser\n\
Links aus anderen Apps in Brújula öffnen.\n\
Brújula als Standardbrowser festlegen\n\
Deine Favoriten-Seiten\n\
Die Seiten, die du ständig öffnest, mit einem Klick in der Seitenleiste.\n\
Wer bist du?\n\
Jede Person am Computer kann ein eigenes Profil mit Lesezeichen und Tabs haben.\n\
Name\n\
Dein Name\n\
Bild\n\
Passwort (optional)\n\
Leer lassen für keins\n\
Es wird bei der Profilwahl gefragt. Es verschlüsselt deine Dateien nicht: Wer Zugriff auf dieses Konto hat, könnte sie lesen.\n\
Das Passwort braucht mindestens 4 Zeichen.\n\
Alles bereit\n\
Du kannst alles später in den Einstellungen ändern.\n\
Los geht's";

/// Texts in the language `pt`, one per line.
const PT: &str = "Bem-vindo ao Brújula\n\
Um navegador rápido, seguro e à sua medida. Vamos configurá-lo num instante.\n\
Seguinte\n\
Voltar\n\
Saltar\n\
Traga os seus dados\n\
Importe de outro navegador os favoritos, o histórico, os cookies e as palavras-passe. As palavras-passe ficam cifradas com uma palavra-passe mestra.\n\
Nenhum navegador encontrado. Pode escolher um ficheiro de favoritos ou de palavras-passe (CSV).\n\
Escolher um ficheiro…\n\
Importar de\n\
O seu navegador de sempre\n\
Abra no Brújula os links das outras aplicações.\n\
Usar o Brújula como navegador predefinido\n\
Os seus essenciais\n\
Os sites que abre o dia todo, a um clique na barra lateral.\n\
Quem é você?\n\
Cada pessoa do computador pode ter o seu perfil, com favoritos e separadores.\n\
Nome\n\
O seu nome\n\
Imagem\n\
Palavra-passe (opcional)\n\
Deixe vazio para nenhuma\n\
É pedida ao escolher o perfil. Não cifra os seus ficheiros: quem tiver acesso a esta conta poderia lê-los.\n\
A palavra-passe precisa de pelo menos 4 caracteres.\n\
Tudo pronto\n\
Pode alterar tudo isto nas Preferências.\n\
Começar a navegar";

static WORDS: LazyLock<Vec<Words>> = LazyLock::new(|| {
    [("es", ES), ("en", EN), ("fr", FR), ("de", DE), ("pt", PT)]
        .into_iter()
        .map(|(code, text)| Words {
            code,
            lines: text.lines().collect(),
        })
        .collect()
});

/// The words for a language tag such as `es_ES.UTF-8` or `pt-BR`; English for the rest.
fn words(lang: &str) -> &'static Words {
    let code = lang
        .to_ascii_lowercase()
        .split(['_', '-', '.', '@'])
        .next()
        .unwrap_or("")
        .to_owned();
    WORDS.iter().find(|w| w.code == code).unwrap_or(&WORDS[1])
}

/// The languages the page speaks.
pub fn languages() -> Vec<&'static str> {
    WORDS.iter().map(|w| w.code).collect()
}

/// The greeting, as typed on the first screen.
pub fn greeting(lang: &str) -> &'static str {
    words(lang).t(Text::Welcome)
}

/// What was brought from another browser, in numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub bookmarks: usize,
    pub history: usize,
    pub cookies: usize,
    pub logins: usize,
    /// Cookies and passwords that are encrypted with a key that was not available.
    pub unreadable: usize,
}

/// What the browser tells the page after it did something.
pub enum Say {
    /// This much was brought.
    Imported(Counts),
    /// The file is not one that is read, or it is damaged.
    ImportFailed,
    DefaultDone,
    DefaultFailed,
}

/// The words for each kind of data, in the order they are listed: bookmarks, history, cookies,
/// passwords, and the ones that could not be read.
fn kinds(code: &str) -> [&'static str; 5] {
    match code {
        "es" => [
            "marcadores",
            "páginas del historial",
            "cookies",
            "contraseñas",
            "cifradas que no se pudieron leer",
        ],
        "fr" => [
            "favoris",
            "pages de l'historique",
            "cookies",
            "mots de passe",
            "chiffrés illisibles",
        ],
        "de" => [
            "Lesezeichen",
            "Verlaufseinträge",
            "Cookies",
            "Passwörter",
            "verschlüsselte, nicht lesbar",
        ],
        "pt" => [
            "favoritos",
            "páginas do histórico",
            "cookies",
            "palavras-passe",
            "cifrados que não foi possível ler",
        ],
        _ => [
            "bookmarks",
            "history pages",
            "cookies",
            "passwords",
            "encrypted ones that could not be read",
        ],
    }
}

pub fn say(lang: &str, what: &Say) -> String {
    let code = words(lang).code;
    match (what, code) {
        (Say::Imported(c), _) => {
            let words = kinds(code);
            let parts: Vec<String> = [c.bookmarks, c.history, c.cookies, c.logins, c.unreadable]
                .iter()
                .zip(words)
                .filter(|(n, _)| **n > 0)
                .map(|(n, w)| format!("{n} {w}"))
                .collect();
            if parts.is_empty() {
                match code {
                    "es" => "No había nada nuevo".into(),
                    "fr" => "Rien de nouveau".into(),
                    "de" => "Nichts Neues".into(),
                    "pt" => "Não havia nada de novo".into(),
                    _ => "There was nothing new".into(),
                }
            } else {
                let head = match code {
                    "es" => "Importado",
                    "fr" => "Importé",
                    "de" => "Importiert",
                    "pt" => "Importado",
                    _ => "Imported",
                };
                format!("{head}: {}", parts.join(", "))
            }
        }
        (Say::ImportFailed, "es") => "No se pudo leer ese archivo".into(),
        (Say::ImportFailed, "fr") => "Ce fichier n'a pas pu être lu".into(),
        (Say::ImportFailed, "de") => "Die Datei konnte nicht gelesen werden".into(),
        (Say::ImportFailed, "pt") => "Não foi possível ler esse ficheiro".into(),
        (Say::ImportFailed, _) => "That file could not be read".into(),
        (Say::DefaultDone, "es") => "Listo: Brújula es tu navegador predeterminado".into(),
        (Say::DefaultDone, "fr") => "Fait : Brújula est votre navigateur par défaut".into(),
        (Say::DefaultDone, "de") => "Fertig: Brújula ist dein Standardbrowser".into(),
        (Say::DefaultDone, "pt") => "Feito: o Brújula é o seu navegador predefinido".into(),
        (Say::DefaultDone, _) => "Done: Brújula is your default browser".into(),
        (Say::DefaultFailed, "es") => {
            "No se pudo cambiar. Elígelo en los ajustes del sistema (Aplicaciones predeterminadas)"
                .into()
        }
        (Say::DefaultFailed, "fr") => {
            "Impossible. Choisissez-le dans les paramètres du système (Applications par défaut)"
                .into()
        }
        (Say::DefaultFailed, "de") => {
            "Das ging nicht. Wähle ihn in den Systemeinstellungen (Standardanwendungen)".into()
        }
        (Say::DefaultFailed, "pt") => {
            "Não foi possível. Escolha-o nas definições do sistema (Aplicações predefinidas)".into()
        }
        (Say::DefaultFailed, _) => {
            "That did not work. Pick it in the system settings (Default applications)".into()
        }
    }
}

/// The title of the file picker for bookmarks.
pub fn file_title(lang: &str) -> &'static str {
    match words(lang).code {
        "es" => "Archivo de marcadores",
        "fr" => "Fichier de favoris",
        "de" => "Lesezeichen-Datei",
        "pt" => "Ficheiro de favoritos",
        _ => "Bookmarks file",
    }
}

fn avatar_picker(w: &Words) -> String {
    let discs: String = crate::avatars::AVATARS
        .iter()
        .enumerate()
        .map(|(i, a)| {
            format!(
                r#"<label class="av" title="{name}"><input type="radio" name="avatar" value="{id}"{checked}>{svg}</label>"#,
                name = escape(a.name),
                id = a.id,
                checked = if i == 0 { " checked" } else { "" },
                svg = a.svg(56)
            )
        })
        .collect();
    format!(
        r#"<fieldset class="avatars"><legend>{}</legend>{discs}</fieldset>"#,
        escape(w.t(Text::Avatar))
    )
}

fn essentials_grid(icons: &[(String, String)]) -> String {
    SUGGESTED
        .iter()
        .map(|s| {
            let tile = match icons.iter().find(|(url, _)| url == s.url) {
                Some((_, data)) => format!(r#"<img class="tile app" src="{}" alt="">"#, escape(data)),
                None => format!(
                    r#"<span class="tile" style="background:{}">{}</span>"#,
                    s.color,
                    escape(&s.name.chars().next().map(String::from).unwrap_or_default()),
                ),
            };
            format!(
                r#"<button type="button" class="ess" data-url="{url}" aria-pressed="false">{tile}<b>{name}</b><i class="tick" aria-hidden="true"></i></button>"#,
                url = escape(s.url),
                name = escape(s.name),
            )
        })
        .collect()
}

fn sources_list(sources: &[Source], w: &Words) -> String {
    if sources.is_empty() {
        return format!(r#"<p class="hint">{}</p>"#, escape(w.t(Text::ImportNone)));
    }
    let rows: String = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                r#"<button type="button" class="src" data-source="{i}"><b>{from} {name}</b><small id="src-{i}"></small></button>"#,
                from = escape(w.t(Text::ImportFrom)),
                name = escape(&s.name),
            )
        })
        .collect();
    format!(r#"<div class="sources">{rows}</div>"#)
}

pub fn render(page: &Page) -> String {
    let w = words(page.lang);
    let theme = match page.theme {
        "light" | "dark" => page.theme,
        _ => "system",
    };
    let accent = crate::settings::parse_hex_color(page.accent)
        .unwrap_or_else(|| crate::settings::ACCENT_DEFAULT.to_owned());
    let profile_step = if page.ask_profile {
        format!(
            r#"<section class="step" data-step="user"><h2>{title}</h2><p class="hint">{hint}</p>
<label class="field"><span>{name}</span><input id="name" type="text" maxlength="24" placeholder="{name_ph}" autocomplete="off"></label>
{avatars}
<label class="field"><span>{password}</span><input id="password" type="password" maxlength="200" placeholder="{password_ph}" autocomplete="new-password"></label>
<p class="note">{note}</p><p class="error" id="pw-error" hidden>{short}</p></section>"#,
            title = escape(w.t(Text::UserTitle)),
            hint = escape(w.t(Text::UserHint)),
            name = escape(w.t(Text::Name)),
            name_ph = escape(w.t(Text::NamePh)),
            avatars = avatar_picker(w),
            password = escape(w.t(Text::Password)),
            password_ph = escape(w.t(Text::PasswordPh)),
            note = escape(w.t(Text::PasswordNote)),
            short = escape(w.t(Text::PasswordShort)),
        )
    } else {
        String::new()
    };
    let text = |s: &str| {
        serde_json::to_string(s)
            .unwrap_or_default()
            // Nothing in a string may close the script it sits in.
            .replace('<', "\\u003c")
    };
    format!(
        r##"<!doctype html>
<html lang="{lang}" data-theme="{theme}"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>{faces}{css}</style></head>
<body style="--accent:{accent}"><main>
<div class="stage">
<section class="step intro" data-step="intro">{mascot}<h1 id="type" aria-label="{title}"></h1><p class="tagline">{tagline}</p></section>
<section class="step" data-step="import"><h2>{import_title}</h2><p class="hint">{import_hint}</p>{sources}
<button type="button" class="ghost" id="pick-file">{import_file}</button></section>
<section class="step" data-step="default"><h2>{default_title}</h2><p class="hint">{default_hint}</p>
<button type="button" class="primary wide" id="make-default">{default_button}</button><p class="msg" id="msg-default" role="status"></p></section>
<section class="step" data-step="essentials"><h2>{ess_title}</h2><p class="hint">{ess_hint}</p><div class="esgrid">{essentials}</div></section>
{profile_step}
<section class="step" data-step="done"><h2>{done_title}</h2><p class="hint">{done_hint}</p></section>
</div>
<footer><button type="button" class="ghost" id="back">{back}</button><span class="dots" id="dots"></span><button type="button" class="primary" id="next">{next}</button></footer>
</main>
<script>
const T={{welcome:{welcome_js},next:{next_js},skip:{skip_js},done:{done_js}}};
{js}
</script></body></html>
"##,
        lang = escape(w.code),
        title = escape(w.t(Text::Welcome)),
        faces = crate::fonts::FACES,
        css = CSS,
        mascot = crate::newtab::MASCOT,
        tagline = escape(w.t(Text::Tagline)),
        import_title = escape(w.t(Text::ImportTitle)),
        import_hint = escape(w.t(Text::ImportHint)),
        sources = sources_list(page.sources, w),
        import_file = escape(w.t(Text::ImportFile)),
        default_title = escape(w.t(Text::DefaultTitle)),
        default_hint = escape(w.t(Text::DefaultHint)),
        default_button = escape(w.t(Text::DefaultButton)),
        ess_title = escape(w.t(Text::EssTitle)),
        ess_hint = escape(w.t(Text::EssHint)),
        essentials = essentials_grid(page.icons),
        done_title = escape(w.t(Text::DoneTitle)),
        done_hint = escape(w.t(Text::DoneHint)),
        back = escape(w.t(Text::Back)),
        next = escape(w.t(Text::Next)),
        welcome_js = text(w.t(Text::Welcome)),
        next_js = text(w.t(Text::Next)),
        skip_js = text(w.t(Text::Skip)),
        done_js = text(w.t(Text::DoneButton)),
        js = JS,
    )
}

const CSS: &str = r#"
:root{color-scheme:light dark;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
@media (prefers-color-scheme:dark){:root{--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}}
:root[data-theme=light]{color-scheme:light;--bg:#f5f6f8;--fg:#12161a;--muted:#617079;--card:#fff;--line:#e3e7ec;--chip:#edf0f3;--shadow:0 1px 2px rgba(16,24,40,.05),0 12px 32px -14px rgba(16,24,40,.18)}
:root[data-theme=dark]{color-scheme:dark;--bg:#0f1214;--fg:#eef1f3;--muted:#98a5ae;--card:#171c1f;--line:#262d32;--chip:#20272b;--shadow:0 1px 2px rgba(0,0,0,.3),0 12px 32px -14px rgba(0,0,0,.6)}
*{box-sizing:border-box}
html,body{height:100%;margin:0}
body{font-family:Inter,"Adwaita Sans",system-ui,sans-serif;background:var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased}
main{height:100%;width:min(38rem,calc(100% - 3rem));margin:0 auto;display:flex;flex-direction:column;padding:0 0 1.6rem}
.stage{flex:1;min-height:0;position:relative;overflow-y:auto;display:flex;flex-direction:column;justify-content:center;margin:0 -.75rem}
.step{display:none;animation:in .35s ease both;padding:1.5rem .75rem}
.step.on{display:block}
@keyframes in{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}
.intro{text-align:center}
.mascot{width:10rem;height:10rem;overflow:visible}
.mascot .bob{animation:bob 5s ease-in-out infinite;transform-origin:60px 120px}
@keyframes bob{0%,100%{transform:translateY(0)}50%{transform:translateY(-2px)}}
h1{font-size:2.3rem;font-weight:500;letter-spacing:-.035em;margin:.8rem 0 .4rem;min-height:2.9rem}
h1.typing::after{content:"";display:inline-block;width:.12em;height:1em;margin-left:.08em;vertical-align:-.12em;background:var(--accent);animation:caret .9s steps(1) infinite}
@keyframes caret{50%{opacity:0}}
.tagline{color:var(--muted);font-size:1.05rem;line-height:1.5;margin:0 auto;max-width:28rem;opacity:0;transition:opacity .6s}
.tagline.on{opacity:1}
h2{font-size:1.65rem;font-weight:520;letter-spacing:-.025em;margin:0 0 .4rem}
.hint{color:var(--muted);line-height:1.5;margin:0 0 1.4rem}
.note{color:var(--muted);font-size:.82rem;line-height:1.45;margin:.2rem 0 0}
.error{color:#d8334a;font-size:.85rem;margin:.5rem 0 0}
.msg{color:var(--muted);margin:.9rem 0 0;min-height:1.3rem}
.msg.ok{color:#2f9e63}.msg.bad{color:#d8334a}
button{font:inherit;color:inherit;cursor:pointer}
button:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.primary{border:0;border-radius:999px;background:var(--accent);color:#fff;font-weight:560;padding:.7rem 1.6rem;transition:filter .15s,transform .1s}
.primary:hover{filter:brightness(1.08)}.primary:active{transform:scale(.97)}
.primary.wide{padding:.85rem 1.4rem}
.ghost{border:1px solid var(--line);background:var(--card);border-radius:999px;padding:.55rem 1.2rem;transition:background .15s}
.ghost:hover{background:var(--chip)}
.sources{display:flex;flex-direction:column;gap:.5rem;margin-bottom:1rem}
.src{display:flex;justify-content:space-between;align-items:center;gap:1rem;text-align:left;background:var(--card);border:1px solid var(--line);border-radius:1rem;padding:.85rem 1.1rem;box-shadow:var(--shadow)}
.src:hover{border-color:var(--accent)}
.src small{color:var(--muted)}.src small.ok{color:#2f9e63}
.esgrid{display:grid;grid-template-columns:repeat(auto-fill,minmax(10.2rem,1fr));gap:.6rem}
.ess{position:relative;display:flex;align-items:center;gap:.65rem;text-align:left;background:var(--card);border:1px solid var(--line);border-radius:1rem;padding:.6rem .8rem;transition:border-color .15s,box-shadow .15s}
.ess:hover{border-color:color-mix(in srgb,var(--accent) 50%,var(--line))}
.ess[aria-pressed=true]{border-color:var(--accent);box-shadow:inset 0 0 0 1px var(--accent)}
.tile{display:grid;place-items:center;flex:none;width:2rem;height:2rem;border-radius:.65rem;color:#fff;font-weight:700}
.tile.app{background:none;border-radius:.5rem;object-fit:contain}
.ess b{font-weight:520;font-size:.92rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.tick{margin-left:auto;width:.9rem;height:.9rem;border-radius:50%;border:1.5px solid var(--line)}
.ess[aria-pressed=true] .tick{background:var(--accent);border-color:var(--accent);box-shadow:inset 0 0 0 2.5px var(--card)}
.field{display:block;margin-bottom:1rem}
.field span,legend{display:block;font-size:.78rem;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);margin-bottom:.4rem;padding:0}
.field input{width:100%;font:inherit;font-size:1.02rem;color:var(--fg);background:var(--card);border:1px solid var(--line);border-radius:.9rem;padding:.75rem 1rem;box-shadow:var(--shadow)}
.field input:focus{border-color:var(--accent);outline:none;box-shadow:inset 0 0 0 1px var(--accent)}
.avatars{border:0;margin:0 0 1rem;padding:0;display:flex;flex-wrap:wrap;gap:.5rem}
.avatars legend{width:100%}
.av{position:relative;display:block;border-radius:50%;cursor:pointer;transition:transform .12s}
.av:hover{transform:scale(1.07)}
.av input{position:absolute;opacity:0;inset:0}
.av svg{display:block;width:3.1rem;height:3.1rem;border-radius:50%;outline:3px solid transparent;outline-offset:2px;transition:outline-color .15s}
.av input:checked+svg{outline-color:var(--accent)}
.av input:focus-visible+svg{outline-color:var(--fg)}
footer{display:grid;grid-template-columns:1fr auto 1fr;align-items:center;gap:1rem;padding-top:.8rem;opacity:0;pointer-events:none;transition:opacity .5s}
footer.on{opacity:1;pointer-events:auto}
footer .ghost{justify-self:start}footer .primary{justify-self:end}footer .ghost[hidden]{visibility:hidden;display:inline-block}
.dots{display:flex;gap:.4rem}
.dots i{width:.45rem;height:.45rem;border-radius:50%;background:var(--line);transition:background .2s,width .2s}
.dots i.on{background:var(--accent);width:1.2rem;border-radius:999px}
@media (prefers-reduced-motion:reduce){.step,.tagline,footer{animation:none;transition:none}.mascot .bob{animation:none}h1.typing::after{animation:none}}
"#;

const JS: &str = r#"
const $=s=>document.querySelector(s),$$=s=>[...document.querySelectorAll(s)];
const send=(k,v)=>{location.href='brujula://welcome?'+k+'='+encodeURIComponent(v)};
const steps=$$('.step'),dots=$('#dots'),next=$('#next'),back=$('#back'),foot=document.querySelector('footer');
let at=0;
steps.forEach(()=>dots.appendChild(document.createElement('i')));
const calm=matchMedia('(prefers-reduced-motion:reduce)').matches;
function name(){return steps[at].dataset.step}
function show(n){
  at=Math.max(0,Math.min(steps.length-1,n));
  steps.forEach((s,i)=>s.classList.toggle('on',i===at));
  [...dots.children].forEach((d,i)=>d.classList.toggle('on',i===at));
  back.hidden=at<=1;
  const last=at===steps.length-1,s=name();
  next.textContent=last?T.done:(s==='essentials'&&!$$('.ess[aria-pressed=true]').length)||(s==='import'&&!imported)||(s==='default'&&!madeDefault)?T.skip:T.next;
  if(s==='user')setTimeout(()=>$('#name')?.focus(),50);
}
let imported=false,madeDefault=false;
function finish(){
  const pw=$('#password')?.value||'';
  if(pw&&pw.length<4){$('#pw-error').hidden=false;return}
  const pick=$$('.ess[aria-pressed=true]').map(b=>b.dataset.url);
  const av=document.querySelector('input[name=avatar]:checked');
  send('finish',JSON.stringify({name:$('#name')?.value||'',avatar:av?av.value:'',password:pw,essentials:pick}));
}
next.onclick=()=>{if(at===steps.length-1)finish();else show(at+1)};
back.onclick=()=>show(at-1);
$$('.ess').forEach(b=>b.onclick=()=>{b.setAttribute('aria-pressed',b.getAttribute('aria-pressed')!=='true');show(at)});
$$('.src').forEach(b=>b.onclick=()=>send('import',b.dataset.source));
$('#pick-file').onclick=()=>send('import','file');
$('#make-default').onclick=()=>send('default','1');
$('#password')?.addEventListener('input',()=>{$('#pw-error').hidden=true});
// Called by the browser after it did what was asked.
window.welcome={
  imported(which,count,text){imported=count>0;const s=$('#src-'+which);if(s){s.textContent=text;s.className=count>0?'ok':''}
    if(which==='file'){const m=$('#pick-file');m.textContent=text}show(at)},
  defaulted(ok,text){madeDefault=ok;const m=$('#msg-default');m.textContent=text;m.className='msg '+(ok?'ok':'bad');show(at)},
};
// The greeting is typed, one letter at a time.
function type(){
  const h=$('#type'),tag=$('.tagline');
  const done=()=>{h.classList.remove('typing');tag.classList.add('on');foot.classList.add('on')};
  if(calm){h.textContent=T.welcome;return done()}
  h.classList.add('typing');
  let i=0;
  const tick=()=>{h.textContent=T.welcome.slice(0,++i);if(i<T.welcome.length)setTimeout(tick,70+Math.random()*50);else setTimeout(done,350)};
  setTimeout(tick,500);
}
show(0);type();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn page(lang: &str, ask_profile: bool) -> String {
        render(&Page {
            lang,
            theme: "",
            accent: "",
            sources: &[Source {
                name: "Chrome <b>".into(),
            }],
            icons: &[(
                "https://www.youtube.com/".into(),
                "data:image/svg+xml;base64,AAAA".into(),
            )],
            ask_profile,
        })
    }

    #[test]
    fn the_page_draws_no_glow_and_no_halo() {
        // Nodalia Design: no decorative gradients or glows; a focused or picked control
        // changes its border, it does not grow a blurred ring.
        assert!(!CSS.contains("gradient"));
        assert!(!CSS.contains("0 0 0 3px"));
    }

    #[test]
    fn rings_and_outlines_fit_inside_the_scrolling_stage() {
        // The stage scrolls, so it clips whatever crosses its sides: the ring of the picked
        // avatar (3px outline 2px away) and the focus outlines. The steps keep that much room
        // inside it, and the stage reaches out by as much so they still line up with the footer.
        assert!(CSS.contains(".stage{") && CSS.contains("margin:0 -.75rem}"));
        assert!(
            CSS.contains(".step{display:none;animation:in .35s ease both;padding:1.5rem .75rem}")
        );
    }

    #[test]
    fn the_greeting_follows_the_system_language() {
        assert_eq!(greeting("es_ES.UTF-8"), "Bienvenido a Brújula");
        assert_eq!(greeting("pt-BR"), "Bem-vindo ao Brújula");
        assert_eq!(greeting("de_DE"), "Willkommen bei Brújula");
        assert_eq!(greeting("fr"), "Bienvenue sur Brújula");
        assert_eq!(greeting("ja_JP"), "Welcome to Brújula");
        assert_eq!(greeting(""), "Welcome to Brújula");
        assert_eq!(languages().len(), 5);
    }

    #[test]
    fn every_message_exists_in_every_language() {
        for lang in languages() {
            for what in [
                Say::Imported(Counts::default()),
                Say::Imported(Counts {
                    bookmarks: 7,
                    ..Counts::default()
                }),
                Say::ImportFailed,
                Say::DefaultDone,
                Say::DefaultFailed,
            ] {
                assert!(!say(lang, &what).is_empty(), "{lang}");
            }
            let all = Counts {
                bookmarks: 1,
                history: 22,
                cookies: 333,
                logins: 4444,
                unreadable: 55555,
            };
            let text = say(lang, &Say::Imported(all));
            for n in ["1 ", "22 ", "333 ", "4444 ", "55555 "] {
                assert!(text.contains(n), "{lang}: {text}");
            }
            assert!(!file_title(lang).is_empty());
        }
    }

    #[test]
    fn every_language_fills_every_text() {
        for w in WORDS.iter() {
            assert_eq!(w.lines.len(), Text::DoneButton as usize + 1, "{}", w.code);
            for t in &w.lines {
                assert!(!t.trim().is_empty(), "{}", w.code);
            }
            // The password warning has to say the files are not encrypted, in every language.
            assert!(w.t(Text::PasswordNote).len() > 40, "{}", w.code);
        }
    }

    #[test]
    fn the_page_has_every_step_and_escapes_what_it_shows() {
        let html = page("es", true);
        for step in ["intro", "import", "default", "essentials", "user", "done"] {
            assert!(html.contains(&format!(r#"data-step="{step}""#)), "{step}");
        }
        assert!(html.contains("Chrome &lt;b&gt;"));
        assert!(!html.contains("Chrome <b>"));
        assert_eq!(
            html.matches(r#"name="avatar""#).count(),
            crate::avatars::AVATARS.len()
        );
        for s in SUGGESTED.iter() {
            assert!(html.contains(s.url), "{}", s.name);
        }
        // A site with an icon from the theme shows it; the rest show a letter.
        assert!(html.contains(r#"<img class="tile app" src="data:image/svg+xml;base64,AAAA""#));
        assert_eq!(html.matches("<img class=\"tile app\"").count(), 1);
        // Nothing in the typed text can close the script early: there is one script, one end.
        assert_eq!(html.matches("</script>").count(), 1);
    }

    #[test]
    fn the_profile_step_is_left_out_when_there_is_a_profile() {
        let html = page("en", false);
        assert!(!html.contains(r#"data-step="user""#));
        assert!(!html.contains("id=\"password\""));
    }

    #[test]
    fn actions_are_read_and_only_sane_ones() {
        let go = |u: &str| parse_action(&format!("{ACTION_PREFIX}{u}"));
        assert_eq!(go("import=2"), Some(Action::Import(2)));
        assert_eq!(go("import=file"), Some(Action::ImportFile));
        assert_eq!(go("import=-1"), None);
        assert_eq!(go("import=x"), None);
        assert_eq!(go("default=1"), Some(Action::DefaultBrowser));
        assert_eq!(go("skip=1"), Some(Action::Skip));
        assert_eq!(go("other=1"), None);
        assert_eq!(go("import=1&skip=1"), None);
        assert_eq!(parse_action("brujula://set?import=1"), None);
        assert_eq!(parse_action("https://a.example/?import=1"), None);

        let json = r#"{"name":"Ana","avatar":"moon","password":"abcd","essentials":["https://x.example/"]}"#;
        let enc: String = url::form_urlencoded::byte_serialize(json.as_bytes()).collect();
        match go(&format!("finish={enc}")) {
            Some(Action::Finish(f)) => {
                assert_eq!(f.name, "Ana");
                assert_eq!(f.avatar, "moon");
                assert_eq!(f.password, "abcd");
                assert_eq!(f.essentials, vec!["https://x.example/".to_owned()]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(go("finish=no%20es%20json"), None);
        // Too many essentials are cut, and a huge payload is refused.
        let many = format!(
            r#"{{"essentials":[{}]}}"#,
            vec![r#""https://a.example/""#; 100].join(",")
        );
        let enc: String = url::form_urlencoded::byte_serialize(many.as_bytes()).collect();
        match go(&format!("finish={enc}")) {
            Some(Action::Finish(f)) => {
                assert_eq!(f.essentials.len(), crate::essentials::MAX_ESSENTIALS)
            }
            other => panic!("{other:?}"),
        }
        let huge = "a".repeat(MAX_PAYLOAD + 1);
        assert_eq!(go(&format!("finish={huge}")), None);
    }
}
