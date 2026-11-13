//! The profiles of this computer: where they are kept, who is asked for at launch ("¿Quién
//! navega?", like a streaming service) and how a password is asked for.

use std::cell::RefCell;
use std::rc::Rc;

use brujula_core::avatars;
use brujula_core::profiles::Profiles;
use nodalia_gtk as ndl;
use nodalia_gtk::gtk;
use nodalia_gtk::gtk::gio;
use nodalia_gtk::gtk::prelude::*;

use crate::dialogs;
use crate::paths;

pub fn load() -> Profiles {
    std::fs::read_to_string(paths::profiles_file())
        .map(|t| Profiles::from_json(&t))
        .unwrap_or_default()
}

/// Saved readable only by their owner: the file holds the hashes of the passwords.
pub fn save(profiles: &Profiles) -> std::io::Result<()> {
    paths::write_private(&paths::profiles_file(), &profiles.to_json())
}

/// The chooser is shown when there is more than one person, or when the only profile has a
/// password.
pub fn needs_chooser(profiles: &Profiles) -> bool {
    profiles.list().len() > 1 || profiles.list().iter().any(|p| p.has_lock())
}

/// Writes the avatar to a file the pictures of the chooser can read.
fn avatar_file(id: &str) -> std::path::PathBuf {
    let avatar = avatars::get(id);
    let dir = paths::root_cache_dir().join("avatars");
    let file = dir.join(format!("{}.svg", avatar.id));
    let svg = avatar.svg(96);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(svg.as_str()) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&file, svg);
    }
    file
}

/// Asks for the password of a profile until it is right or the person gives up, then runs `ok`.
pub fn unlock<P>(parent: &P, profiles: Rc<Profiles>, id: String, wrong: bool, ok: Rc<dyn Fn()>)
where
    P: IsA<gtk::Widget> + Clone + 'static,
{
    let Some(profile) = profiles.get(&id) else {
        return;
    };
    if !profile.has_lock() {
        ok();
        return;
    }
    let body = if wrong {
        "Esa contraseña no es correcta. Inténtalo de nuevo."
    } else {
        "Escribe la contraseña de este perfil para entrar."
    };
    let heading = format!("Perfil de {}", profile.name);
    let again = parent.clone();
    dialogs::ask_secret(parent, &heading, body, "Entrar", move |password| {
        if profiles.check(&id, &password) {
            ok();
        } else {
            unlock(&again, profiles.clone(), id.clone(), true, ok.clone());
        }
    });
}

/// Shows who is browsing and returns the profile picked, or `None` if the window was closed.
pub fn choose(profiles: Profiles) -> Option<String> {
    let chosen: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let app = gtk::Application::builder()
        .application_id(format!("{}.Chooser", paths::APP_ID))
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_startup(|_| {
        ndl::init();
        crate::icons::install();
        crate::browser::install_css();
    });
    let profiles = Rc::new(profiles);
    {
        let chosen = chosen.clone();
        app.connect_activate(move |app| {
            let window = ndl::ApplicationWindow::new(app);
            window.set_title(Some("Brújula"));
            window.set_default_size(720, 520);
            window.set_icon_name(Some(paths::APP_ID));

            let title = gtk::Label::new(Some("¿Quién navega?"));
            title.add_css_class("brujula-who-title");
            let cards = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .homogeneous(true)
                // A flow box takes the room of a full row even when it has one child.
                .max_children_per_line(
                    u32::try_from(profiles.list().len().clamp(1, 4)).unwrap_or(4),
                )
                .row_spacing(18)
                .column_spacing(18)
                .halign(gtk::Align::Center)
                .build();
            for p in profiles.list() {
                let face = gtk::Picture::for_filename(avatar_file(&p.avatar));
                face.set_size_request(96, 96);
                face.set_can_shrink(false);
                face.add_css_class("brujula-who-face");
                let name = gtk::Label::new(Some(&p.name));
                name.set_max_width_chars(14);
                name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                name.add_css_class("brujula-who-name");
                let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
                col.append(&face);
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                line.set_halign(gtk::Align::Center);
                if p.has_lock() {
                    line.append(&gtk::Image::from_icon_name("ndl-lock-symbolic"));
                }
                line.append(&name);
                col.append(&line);
                let button = gtk::Button::builder()
                    .child(&col)
                    .css_classes(["flat", "brujula-who"])
                    .tooltip_text(&p.name)
                    .build();
                let id = p.id.clone();
                let profiles = profiles.clone();
                let chosen = chosen.clone();
                let app = app.clone();
                let win = window.clone();
                button.connect_clicked(move |_| {
                    let chosen = chosen.clone();
                    let app = app.clone();
                    unlock(
                        &win,
                        profiles.clone(),
                        id.clone(),
                        false,
                        Rc::new({
                            let id = id.clone();
                            move || {
                                *chosen.borrow_mut() = Some(id.clone());
                                app.quit();
                            }
                        }),
                    );
                });
                cards.append(&button);
            }
            let page = gtk::Box::new(gtk::Orientation::Vertical, 28);
            page.set_valign(gtk::Align::Center);
            page.set_halign(gtk::Align::Center);
            page.set_margin_top(32);
            page.set_margin_bottom(32);
            page.append(&title);
            page.append(&cards);
            window.set_content(Some(&page));
            window.present();
        });
    }
    app.run_with_args::<&str>(&[]);
    let id = chosen.borrow().clone();
    id
}
