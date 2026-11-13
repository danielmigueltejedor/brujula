//! The welcome page shown the first time Brújula opens: a typed greeting in the system's
//! language, then bookmarks, default browser, essentials and (when there is no profile yet) the
//! first profile. The page keeps its steps on its own; what needs the browser (reading files,
//! setting the default, saving) goes out as `brujula://welcome?key=value` addresses.

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

struct Words {
    code: &'static str,
    welcome: &'static str,
    tagline: &'static str,
    next: &'static str,
    back: &'static str,
    skip: &'static str,
    import_title: &'static str,
    import_hint: &'static str,
    import_none: &'static str,
    import_file: &'static str,
    import_from: &'static str,
    default_title: &'static str,
    default_hint: &'static str,
    default_button: &'static str,
    ess_title: &'static str,
    ess_hint: &'static str,
    user_title: &'static str,
    user_hint: &'static str,
    name: &'static str,
    name_ph: &'static str,
    avatar: &'static str,
    password: &'static str,
    password_ph: &'static str,
    password_note: &'static str,
    password_short: &'static str,
    done_title: &'static str,
    done_hint: &'static str,
    done_button: &'static str,
}

const WORDS: &[Words] = &[
    Words {
        code: "es",
        welcome: "Bienvenido a Brújula",
        tagline: "Un navegador rápido, seguro y a tu medida. Vamos a dejarlo a tu gusto en un momento.",
        next: "Siguiente",
        back: "Atrás",
        skip: "Omitir",
        import_title: "Trae tus marcadores",
        import_hint: "Importa los de otro navegador para no empezar de cero. Solo se leen los marcadores, nada más.",
        import_none: "No se ha encontrado ningún navegador. Puedes elegir un archivo de marcadores.",
        import_file: "Elegir un archivo…",
        import_from: "Importar de",
        default_title: "Tu navegador de siempre",
        default_hint: "Haz que los enlaces de otras aplicaciones se abran en Brújula.",
        default_button: "Usar Brújula como navegador predeterminado",
        ess_title: "Tus esenciales",
        ess_hint: "Los sitios que abres todo el día, a un clic en la barra lateral. Elige los que quieras.",
        user_title: "¿Quién eres?",
        user_hint: "Cada persona del equipo puede tener su perfil, con sus marcadores y pestañas.",
        name: "Nombre",
        name_ph: "Tu nombre",
        avatar: "Imagen",
        password: "Contraseña (opcional)",
        password_ph: "Déjala vacía si no quieres",
        password_note: "Se pedirá al elegir tu perfil. No cifra tus archivos: quien tenga acceso a esta cuenta del equipo podría leerlos.",
        password_short: "La contraseña necesita al menos 4 caracteres.",
        done_title: "Todo listo",
        done_hint: "Puedes cambiar cualquiera de estas cosas en Preferencias cuando quieras.",
        done_button: "Empezar a navegar",
    },
    Words {
        code: "en",
        welcome: "Welcome to Brújula",
        tagline: "A fast, safe browser that is yours to shape. Let's set it up in a moment.",
        next: "Next",
        back: "Back",
        skip: "Skip",
        import_title: "Bring your bookmarks",
        import_hint: "Import them from another browser so you do not start from scratch. Only bookmarks are read, nothing else.",
        import_none: "No browser was found. You can pick a bookmarks file.",
        import_file: "Choose a file…",
        import_from: "Import from",
        default_title: "Your everyday browser",
        default_hint: "Open the links from other apps in Brújula.",
        default_button: "Make Brújula the default browser",
        ess_title: "Your essentials",
        ess_hint: "The sites you open all day, one click away in the sidebar. Pick the ones you want.",
        user_title: "Who are you?",
        user_hint: "Everyone who shares this computer can have a profile with their own bookmarks and tabs.",
        name: "Name",
        name_ph: "Your name",
        avatar: "Picture",
        password: "Password (optional)",
        password_ph: "Leave empty for none",
        password_note: "It is asked when you pick your profile. It does not encrypt your files: anyone with access to this computer account could read them.",
        password_short: "The password needs at least 4 characters.",
        done_title: "All set",
        done_hint: "You can change any of this in Preferences whenever you like.",
        done_button: "Start browsing",
    },
    Words {
        code: "fr",
        welcome: "Bienvenue sur Brújula",
        tagline: "Un navigateur rapide, sûr et à votre mesure. Réglons-le en un instant.",
        next: "Suivant",
        back: "Retour",
        skip: "Passer",
        import_title: "Récupérez vos favoris",
        import_hint: "Importez ceux d'un autre navigateur pour ne pas repartir de zéro. Seuls les favoris sont lus.",
        import_none: "Aucun navigateur trouvé. Vous pouvez choisir un fichier de favoris.",
        import_file: "Choisir un fichier…",
        import_from: "Importer depuis",
        default_title: "Votre navigateur de tous les jours",
        default_hint: "Ouvrez dans Brújula les liens des autres applications.",
        default_button: "Définir Brújula comme navigateur par défaut",
        ess_title: "Vos essentiels",
        ess_hint: "Les sites que vous ouvrez toute la journée, à un clic dans la barre latérale.",
        user_title: "Qui êtes-vous ?",
        user_hint: "Chaque personne de l'ordinateur peut avoir son profil, avec ses favoris et ses onglets.",
        name: "Nom",
        name_ph: "Votre nom",
        avatar: "Image",
        password: "Mot de passe (facultatif)",
        password_ph: "Laissez vide pour aucun",
        password_note: "Il est demandé au choix du profil. Il ne chiffre pas vos fichiers : quiconque accède à ce compte pourrait les lire.",
        password_short: "Le mot de passe doit faire au moins 4 caractères.",
        done_title: "Tout est prêt",
        done_hint: "Vous pourrez tout modifier dans les Préférences.",
        done_button: "Commencer à naviguer",
    },
    Words {
        code: "de",
        welcome: "Willkommen bei Brújula",
        tagline: "Ein schneller, sicherer Browser nach deinem Geschmack. Richten wir ihn kurz ein.",
        next: "Weiter",
        back: "Zurück",
        skip: "Überspringen",
        import_title: "Lesezeichen mitbringen",
        import_hint: "Importiere sie aus einem anderen Browser. Es werden nur Lesezeichen gelesen.",
        import_none: "Kein Browser gefunden. Du kannst eine Lesezeichen-Datei wählen.",
        import_file: "Datei wählen …",
        import_from: "Importieren aus",
        default_title: "Dein Alltags-Browser",
        default_hint: "Links aus anderen Apps in Brújula öffnen.",
        default_button: "Brújula als Standardbrowser festlegen",
        ess_title: "Deine Favoriten-Seiten",
        ess_hint: "Die Seiten, die du ständig öffnest, mit einem Klick in der Seitenleiste.",
        user_title: "Wer bist du?",
        user_hint: "Jede Person am Computer kann ein eigenes Profil mit Lesezeichen und Tabs haben.",
        name: "Name",
        name_ph: "Dein Name",
        avatar: "Bild",
        password: "Passwort (optional)",
        password_ph: "Leer lassen für keins",
        password_note: "Es wird bei der Profilwahl gefragt. Es verschlüsselt deine Dateien nicht: Wer Zugriff auf dieses Konto hat, könnte sie lesen.",
        password_short: "Das Passwort braucht mindestens 4 Zeichen.",
        done_title: "Alles bereit",
        done_hint: "Du kannst alles später in den Einstellungen ändern.",
        done_button: "Los geht's",
    },
    Words {
        code: "pt",
        welcome: "Bem-vindo ao Brújula",
        tagline: "Um navegador rápido, seguro e à sua medida. Vamos configurá-lo num instante.",
        next: "Seguinte",
        back: "Voltar",
        skip: "Saltar",
        import_title: "Traga os seus favoritos",
        import_hint: "Importe-os de outro navegador. Só os favoritos são lidos.",
        import_none: "Nenhum navegador encontrado. Pode escolher um ficheiro de favoritos.",
        import_file: "Escolher um ficheiro…",
        import_from: "Importar de",
        default_title: "O seu navegador de sempre",
        default_hint: "Abra no Brújula os links das outras aplicações.",
        default_button: "Usar o Brújula como navegador predefinido",
        ess_title: "Os seus essenciais",
        ess_hint: "Os sites que abre o dia todo, a um clique na barra lateral.",
        user_title: "Quem é você?",
        user_hint: "Cada pessoa do computador pode ter o seu perfil, com favoritos e separadores.",
        name: "Nome",
        name_ph: "O seu nome",
        avatar: "Imagem",
        password: "Palavra-passe (opcional)",
        password_ph: "Deixe vazio para nenhuma",
        password_note: "É pedida ao escolher o perfil. Não cifra os seus ficheiros: quem tiver acesso a esta conta poderia lê-los.",
        password_short: "A palavra-passe precisa de pelo menos 4 caracteres.",
        done_title: "Tudo pronto",
        done_hint: "Pode alterar tudo isto nas Preferências.",
        done_button: "Começar a navegar",
    },
];

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
    words(lang).welcome
}

/// What the browser tells the page after it did something.
pub enum Say {
    /// This many bookmarks were added.
    Imported(usize),
    /// The file is not one that is read, or it is damaged.
    ImportFailed,
    DefaultDone,
    DefaultFailed,
}

pub fn say(lang: &str, what: &Say) -> String {
    let code = words(lang).code;
    match (what, code) {
        (Say::Imported(0), "es") => "No había marcadores nuevos".into(),
        (Say::Imported(0), "fr") => "Aucun nouveau favori".into(),
        (Say::Imported(0), "de") => "Keine neuen Lesezeichen".into(),
        (Say::Imported(0), "pt") => "Nenhum favorito novo".into(),
        (Say::Imported(0), _) => "There were no new bookmarks".into(),
        (Say::Imported(n), "es") => format!("{n} marcadores importados"),
        (Say::Imported(n), "fr") => format!("{n} favoris importés"),
        (Say::Imported(n), "de") => format!("{n} Lesezeichen importiert"),
        (Say::Imported(n), "pt") => format!("{n} favoritos importados"),
        (Say::Imported(n), _) => format!("{n} bookmarks imported"),
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
        escape(w.avatar)
    )
}

fn essentials_grid() -> String {
    SUGGESTED
        .iter()
        .map(|s| {
            format!(
                r#"<button type="button" class="ess" data-url="{url}" aria-pressed="false"><span class="tile" style="background:{color}">{initial}</span><b>{name}</b><i class="tick" aria-hidden="true"></i></button>"#,
                url = escape(s.url),
                color = s.color,
                initial = escape(&s.name.chars().next().map(String::from).unwrap_or_default()),
                name = escape(s.name),
            )
        })
        .collect()
}

fn sources_list(sources: &[Source], w: &Words) -> String {
    if sources.is_empty() {
        return format!(r#"<p class="hint">{}</p>"#, escape(w.import_none));
    }
    let rows: String = sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                r#"<button type="button" class="src" data-source="{i}"><b>{from} {name}</b><small id="src-{i}"></small></button>"#,
                from = escape(w.import_from),
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
            title = escape(w.user_title),
            hint = escape(w.user_hint),
            name = escape(w.name),
            name_ph = escape(w.name_ph),
            avatars = avatar_picker(w),
            password = escape(w.password),
            password_ph = escape(w.password_ph),
            note = escape(w.password_note),
            short = escape(w.password_short),
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
<style>{css}</style></head>
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
        title = escape(w.welcome),
        css = CSS,
        mascot = crate::newtab::MASCOT,
        tagline = escape(w.tagline),
        import_title = escape(w.import_title),
        import_hint = escape(w.import_hint),
        sources = sources_list(page.sources, w),
        import_file = escape(w.import_file),
        default_title = escape(w.default_title),
        default_hint = escape(w.default_hint),
        default_button = escape(w.default_button),
        ess_title = escape(w.ess_title),
        ess_hint = escape(w.ess_hint),
        essentials = essentials_grid(),
        done_title = escape(w.done_title),
        done_hint = escape(w.done_hint),
        back = escape(w.back),
        next = escape(w.next),
        welcome_js = text(w.welcome),
        next_js = text(w.next),
        skip_js = text(w.skip),
        done_js = text(w.done_button),
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
body{font-family:"Adwaita Sans",Inter,system-ui,sans-serif;background:radial-gradient(70rem 36rem at 50% -12%,color-mix(in srgb,var(--accent) 17%,transparent),transparent),var(--bg);color:var(--fg);-webkit-font-smoothing:antialiased}
main{height:100%;width:min(38rem,calc(100% - 3rem));margin:0 auto;display:flex;flex-direction:column;padding:0 0 1.6rem}
.stage{flex:1;min-height:0;position:relative;overflow-y:auto;display:flex;flex-direction:column;justify-content:center}
.step{display:none;animation:in .35s ease both;padding:1.5rem 0}
.step.on{display:block}
@keyframes in{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}
.intro{text-align:center}
.mascot{width:8rem;height:8rem;overflow:visible}
.mascot .bob{animation:bob 5s ease-in-out infinite;transform-origin:60px 120px}
.mascot .eyes{animation:blink 6s infinite;transform-origin:60px 63px;transform-box:view-box}
@keyframes bob{0%,100%{transform:translateY(0)}50%{transform:translateY(-2px)}}
@keyframes blink{0%,94%,100%{transform:scaleY(1)}96.5%{transform:scaleY(.1)}}
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
.ess[aria-pressed=true]{border-color:var(--accent);box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 22%,transparent)}
.tile{display:grid;place-items:center;flex:none;width:2rem;height:2rem;border-radius:.65rem;color:#fff;font-weight:700}
.ess b{font-weight:520;font-size:.92rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.tick{margin-left:auto;width:.9rem;height:.9rem;border-radius:50%;border:1.5px solid var(--line)}
.ess[aria-pressed=true] .tick{background:var(--accent);border-color:var(--accent);box-shadow:inset 0 0 0 2.5px var(--card)}
.field{display:block;margin-bottom:1rem}
.field span,legend{display:block;font-size:.78rem;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted);margin-bottom:.4rem;padding:0}
.field input{width:100%;font:inherit;font-size:1.02rem;color:var(--fg);background:var(--card);border:1px solid var(--line);border-radius:.9rem;padding:.75rem 1rem;box-shadow:var(--shadow)}
.field input:focus{border-color:var(--accent);outline:none;box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 24%,transparent)}
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
@media (prefers-reduced-motion:reduce){.step,.tagline,footer{animation:none;transition:none}.mascot .bob,.mascot .eyes{animation:none}h1.typing::after{animation:none}}
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
            ask_profile,
        })
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
                Say::Imported(0),
                Say::Imported(7),
                Say::ImportFailed,
                Say::DefaultDone,
                Say::DefaultFailed,
            ] {
                assert!(!say(lang, &what).is_empty(), "{lang}");
            }
            assert!(say(lang, &Say::Imported(7)).contains('7'), "{lang}");
            assert!(!file_title(lang).is_empty());
        }
    }

    #[test]
    fn every_language_fills_every_text() {
        for w in WORDS {
            for t in [
                w.welcome,
                w.tagline,
                w.next,
                w.back,
                w.skip,
                w.import_title,
                w.import_hint,
                w.import_none,
                w.import_file,
                w.import_from,
                w.default_title,
                w.default_hint,
                w.default_button,
                w.ess_title,
                w.ess_hint,
                w.user_title,
                w.user_hint,
                w.name,
                w.name_ph,
                w.avatar,
                w.password,
                w.password_ph,
                w.password_note,
                w.password_short,
                w.done_title,
                w.done_hint,
                w.done_button,
            ] {
                assert!(!t.trim().is_empty(), "{}", w.code);
            }
            // The password warning has to say the files are not encrypted, in every language.
            assert!(w.password_note.len() > 40, "{}", w.code);
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
        for s in SUGGESTED {
            assert!(html.contains(s.url), "{}", s.name);
        }
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
