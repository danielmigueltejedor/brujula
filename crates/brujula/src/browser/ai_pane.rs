//! The assistant's settings, inside its own panel: the button with the sliders slides this over
//! the conversation (as the history slides over the tabs) so the provider, the key and what the
//! assistant may do can be changed without leaving the page. It holds no logic: `ai.rs` connects
//! every control to `change_ai`, the same entry point the preferences page uses, so both always
//! say the same thing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use brujula_core::ai::{Autonomy, Provider};
use brujula_core::prefs::AiView;

use super::*;

type Remove = Rc<RefCell<Option<Box<dyn Fn(&str)>>>>;

pub(super) struct AiPane {
    pub root: gtk::Box,
    pub back: gtk::Button,
    pub provider: gtk::DropDown,
    pub model: gtk::Entry,
    pub address: gtk::Entry,
    address_field: gtk::Box,
    key_field: gtk::Box,
    key_state: gtk::Label,
    pub key_set: gtk::Button,
    pub key_clear: gtk::Button,
    pub test: gtk::Button,
    status: gtk::Label,
    pub read: gtk::Switch,
    pub confirm: gtk::Switch,
    confirm_row: gtk::Box,
    pub open_auto: gtk::Switch,
    pub interact: gtk::Switch,
    pub search: gtk::Switch,
    pub blocked_entry: gtk::Entry,
    pub blocked_add: gtk::Button,
    blocked_list: gtk::Box,
    pub off: gtk::Button,
    pub all: gtk::Button,
    /// True while `sync` sets the controls: what they report then is not the user's doing.
    syncing: Cell<bool>,
    on_remove: Remove,
}

fn small(text: &str, class: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .max_width_chars(34)
        .css_classes([class])
        .build()
}

/// A title, a line under it saying what it does, and the switch at the end.
fn switch_row(title: &str, hint: &str, switch: &gtk::Switch) -> gtk::Box {
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    texts.append(&small(title, "brujula-ai-row-title"));
    texts.append(&small(hint, "brujula-ai-row-hint"));
    switch.set_valign(gtk::Align::Center);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("brujula-ai-row");
    row.append(&texts);
    row.append(switch);
    row
}

/// A title with its control under it, for what needs room to type.
fn field(title: &str, hint: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 4);
    col.add_css_class("brujula-ai-row");
    col.append(&small(title, "brujula-ai-row-title"));
    col.append(control);
    if !hint.is_empty() {
        col.append(&small(hint, "brujula-ai-row-hint"));
    }
    col
}

fn heading(text: &str) -> gtk::Label {
    let l = small(text, "brujula-ai-section");
    l.set_margin_top(10);
    l
}

impl AiPane {
    pub fn new() -> Self {
        let back = gtk::Button::builder()
            .icon_name("ndl-arrow-left-symbolic")
            .css_classes(["flat", "circular"])
            .tooltip_text("Volver a la conversación")
            .valign(gtk::Align::Center)
            .build();
        let title = gtk::Label::builder()
            .label("Ajustes del asistente")
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["brujula-ai-title"])
            .build();
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.add_css_class("brujula-ai-head");
        head.append(&back);
        head.append(&title);

        let names: Vec<&str> = Provider::ALL.iter().map(|p| p.label()).collect();
        let provider = gtk::DropDown::from_strings(&names);
        provider.set_hexpand(true);
        let model = gtk::Entry::builder().hexpand(true).max_length(80).build();
        let address = gtk::Entry::builder()
            .hexpand(true)
            .max_length(300)
            .input_purpose(gtk::InputPurpose::Url)
            .build();

        let key_state = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .wrap(true)
            .css_classes(["brujula-ai-row-hint"])
            .build();
        let key_set = gtk::Button::builder().css_classes(["pill"]).build();
        let key_clear = gtk::Button::builder()
            .label("Quitar")
            .css_classes(["pill", "flat"])
            .build();
        let key_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        key_buttons.append(&key_set);
        key_buttons.append(&key_clear);
        let key_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        key_box.append(&key_state);
        key_box.append(&key_buttons);
        let key_field = field("Clave de API", "", &key_box);

        let test = gtk::Button::builder()
            .label("Probar la conexión")
            .css_classes(["pill"])
            .halign(gtk::Align::Start)
            .build();
        let status = small("", "brujula-ai-row-hint");
        let test_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        test_box.append(&test);
        test_box.append(&status);

        let read = gtk::Switch::new();
        let confirm = gtk::Switch::new();
        let open_auto = gtk::Switch::new();
        let interact = gtk::Switch::new();
        let search = gtk::Switch::new();
        let confirm_row = switch_row(
            "Preguntar antes de leer un sitio",
            "La primera vez que lee un sitio en cada conversación",
            &confirm,
        );

        let blocked_entry = gtk::Entry::builder()
            .placeholder_text("banco.com")
            .hexpand(true)
            .max_length(120)
            .build();
        let blocked_add = gtk::Button::builder()
            .icon_name("ndl-plus-symbolic")
            .css_classes(["circular"])
            .tooltip_text("Excluir este sitio")
            .valign(gtk::Align::Center)
            .build();
        let blocked_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        blocked_line.append(&blocked_entry);
        blocked_line.append(&blocked_add);
        let blocked_list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let blocked_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        blocked_box.append(&blocked_line);
        blocked_box.append(&blocked_list);

        let off = gtk::Button::builder()
            .label("Apagar el asistente")
            .css_classes(["pill"])
            .build();
        let all = gtk::Button::builder()
            .label("Todos los ajustes")
            .css_classes(["pill", "flat"])
            .build();
        let end = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        end.set_margin_top(14);
        end.set_homogeneous(true);
        end.append(&off);
        end.append(&all);

        let address_field = field(
            "Dirección del servidor",
            "https, o http solo si está en este equipo",
            &address,
        );
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_margin_start(14);
        list.set_margin_end(14);
        list.set_margin_bottom(14);
        list.append(&heading("Proveedor"));
        list.append(&field("Quién responde", "", &provider));
        list.append(&field("Modelo", "", &model));
        list.append(&address_field);
        list.append(&key_field);
        list.append(&field("Conexión", "", &test_box));
        list.append(&heading("Qué puede hacer"));
        list.append(&switch_row(
            "Ver tus pestañas y leer páginas",
            "Solo las que apruebes; nunca contraseñas ni pagos",
            &read,
        ));
        list.append(&confirm_row);
        list.append(&switch_row(
            "Abrir pestañas sin preguntar",
            "Si no, pregunta cada vez",
            &open_auto,
        ));
        list.append(&switch_row(
            "Pulsar y escribir en páginas",
            "Siempre pregunta antes de cada acción",
            &interact,
        ));
        list.append(&heading("Buscador"));
        list.append(&switch_row(
            "Que la IA gestione el buscador",
            "En el Spotlight, «Preguntar a…» va primero",
            &search,
        ));
        list.append(&heading("Sitios excluidos"));
        list.append(&field("Nunca ve ni toca estos sitios", "", &blocked_box));
        list.append(&end);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(false)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&head);
        root.append(&scroller);

        Self {
            root,
            back,
            provider,
            model,
            address,
            address_field,
            key_field,
            key_state,
            key_set,
            key_clear,
            test,
            status,
            read,
            confirm,
            confirm_row,
            open_auto,
            interact,
            search,
            blocked_entry,
            blocked_add,
            blocked_list,
            off,
            all,
            syncing: Cell::new(false),
            on_remove: Rc::new(RefCell::new(None)),
        }
    }

    /// True while the controls are being set from the state, not by the user.
    pub fn syncing(&self) -> bool {
        self.syncing.get()
    }

    /// Called with the site of an exclusion the user removed.
    pub fn set_on_remove(&self, f: impl Fn(&str) + 'static) {
        *self.on_remove.borrow_mut() = Some(Box::new(f));
    }

    /// A line under «Probar la conexión».
    pub fn set_status(&self, text: &str) {
        self.status.set_text(text);
        self.status.set_visible(!text.is_empty());
    }

    /// Puts the controls in line with the settings.
    pub fn sync(&self, view: &AiView) {
        self.syncing.set(true);
        let s = &view.settings;
        let at = Provider::ALL
            .iter()
            .position(|p| *p == s.provider)
            .unwrap_or(0);
        self.provider.set_selected(u32::try_from(at).unwrap_or(0));

        self.model
            .set_placeholder_text(Some(s.provider.default_model()));
        if self.model.text() != s.model {
            self.model.set_text(&s.model);
        }
        self.address_field.set_visible(s.provider.takes_address());
        self.address
            .set_placeholder_text(Some(s.provider.default_base()));
        if self.address.text() != s.base_url {
            self.address.set_text(&s.base_url);
        }

        self.key_field.set_visible(s.provider.can_have_key());
        self.key_state.set_text(if view.key_set {
            "Guardada en el llavero del sistema."
        } else if s.provider.needs_key() {
            "Hace falta una clave. Se guarda en el llavero del sistema, no en un archivo."
        } else {
            "Opcional para este servidor. Se guarda en el llavero del sistema."
        });
        self.key_set.set_label(if view.key_set {
            "Cambiar"
        } else {
            "Poner la clave"
        });
        self.key_clear.set_visible(view.key_set);
        self.set_status(&view.status);

        self.read.set_active(s.read);
        self.confirm.set_active(s.confirm_reads);
        self.confirm_row.set_visible(s.read);
        self.open_auto.set_active(s.open == Autonomy::Auto);
        self.interact.set_active(s.interact);
        self.search.set_active(s.search);

        while let Some(child) = self.blocked_list.first_child() {
            self.blocked_list.remove(&child);
        }
        for host in &s.blocked {
            let label = gtk::Label::builder()
                .label(host)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let remove = gtk::Button::builder()
                .icon_name("ndl-close-symbolic")
                .css_classes(["flat", "circular"])
                .tooltip_text("Quitar de la lista")
                .build();
            let chip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            chip.add_css_class("brujula-ai-chip");
            chip.append(&label);
            chip.append(&remove);
            let on_remove = self.on_remove.clone();
            let host = host.clone();
            remove.connect_clicked(move |_| {
                if let Some(f) = on_remove.borrow().as_ref() {
                    f(&host);
                }
            });
            self.blocked_list.append(&chip);
        }
        self.syncing.set(false);
    }
}
