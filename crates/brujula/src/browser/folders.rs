//! Folders in the sidebar: rows between the tabs, folded and unfolded by clicking them, made
//! and edited from a small popover. Smart folders (a rule on the site or the title) take in
//! the tabs that match without anyone moving them.

use brujula_core::folders::{Folder, Rule};

use super::*;

/// The sidebar row of one folder.
pub(super) struct FolderRow {
    pub row: gtk::ListBoxRow,
    chevron: gtk::Image,
    icon: gtk::Image,
    name: gtk::Label,
    count: gtk::Label,
}

impl FolderRow {
    /// Shows the folder as it is now: name, fold state, how many tabs it holds.
    pub fn sync(&self, folder: &Folder, tabs: usize) {
        self.name.set_text(&folder.name);
        self.chevron.set_icon_name(Some(if folder.open {
            "ndl-chevron-down-symbolic"
        } else {
            "ndl-chevron-right-symbolic"
        }));
        self.icon.set_icon_name(Some(if folder.rule.is_smart() {
            "ndl-filter-symbolic"
        } else {
            "ndl-folder-symbolic"
        }));
        self.count.set_text(&if tabs == 0 {
            String::new()
        } else {
            tabs.to_string()
        });
        self.row.set_tooltip_text(Some(&match &folder.rule {
            Rule::Manual => format!("{} (carpeta)", folder.name),
            Rule::Site(s) => format!("{}: recoge las pestañas de «{s}»", folder.name),
            Rule::Text(t) => format!("{}: recoge las pestañas que mencionan «{t}»", folder.name),
        }));
    }
}

fn kind_index(kind: &str) -> u32 {
    match kind {
        "site" => 1,
        "text" => 2,
        _ => 0,
    }
}

impl Browser {
    pub(super) fn wire_folders(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        self.folder_button.connect_clicked(move |button| {
            if let Some(b) = this.upgrade() {
                b.show_folder_editor(None, button.upcast_ref());
            }
        });
    }

    /// Builds the rows of every folder again (after a restore, or when one is made or removed).
    pub(super) fn rebuild_folder_rows(self: &Rc<Self>) {
        for (_, fr) in self.folder_rows.borrow_mut().drain() {
            self.list.remove(&fr.row);
        }
        let ids: Vec<u32> = self.folders.borrow().all().iter().map(|f| f.id).collect();
        for id in ids {
            let fr = self.make_folder_row(id);
            self.list.append(&fr.row);
            self.folder_rows.borrow_mut().insert(id, fr);
        }
        self.reorder_rows();
        self.list.invalidate_filter();
        self.schedule_save();
    }

    fn make_folder_row(self: &Rc<Self>, id: u32) -> FolderRow {
        let chevron = gtk::Image::from_icon_name("ndl-chevron-down-symbolic");
        chevron.set_pixel_size(12);
        let icon = gtk::Image::from_icon_name("ndl-folder-symbolic");
        icon.set_pixel_size(16);
        let name = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["brujula-tab-title", "brujula-folder-name"])
            .build();
        let count = gtk::Label::builder()
            .css_classes(["dim-label", "brujula-folder-count"])
            .build();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.add_css_class("brujula-tab");
        content.append(&chevron);
        content.append(&icon);
        content.append(&name);
        content.append(&count);
        let row = gtk::ListBoxRow::new();
        row.add_css_class("brujula-row");
        row.add_css_class("brujula-folder");
        row.set_widget_name(&format!("folder-{id}"));
        row.set_child(Some(&content));

        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        let this = Rc::downgrade(self);
        let anchor = row.clone();
        click.connect_pressed(move |_, _, x, y| {
            if let Some(b) = this.upgrade() {
                b.show_folder_menu(id, &anchor, x, y);
            }
        });
        row.add_controller(click);
        FolderRow {
            row,
            chevron,
            icon,
            name,
            count,
        }
    }

    pub(super) fn toggle_folder(self: &Rc<Self>, id: u32) {
        self.folders.borrow_mut().toggle(id);
        // The click selected the folder's row: the tab in front gets the selection back.
        self.reorder_rows();
        self.list.invalidate_filter();
        self.schedule_save();
    }

    /// Puts a tab in a folder (`Some(id)`), takes it out of every folder (`Some(NO_FOLDER)`) or
    /// leaves it to the smart folders (`None`).
    pub(super) fn put_tab_in_folder(self: &Rc<Self>, tab: TabId, folder: Option<u32>) {
        let changes = self.model.borrow_mut().set_folder(tab, folder);
        // Showing the tab is what the user wants after moving it.
        if let Some(f) = folder.filter(|f| *f != NO_FOLDER) {
            self.folders.borrow_mut().open_folder(f);
        }
        self.apply(changes);
        self.reorder_rows();
        self.list.invalidate_filter();
        self.schedule_save();
    }

    /// A manual folder holding `tab`, for "new folder with this tab".
    pub(super) fn new_folder_with(self: &Rc<Self>, tab: TabId) {
        let Some(space) = self.model.borrow().get(tab).map(|t| t.space) else {
            return;
        };
        let id = self
            .folders
            .borrow_mut()
            .add(space, "Carpeta", Rule::Manual);
        if let Some(id) = id {
            self.rebuild_folder_rows();
            self.put_tab_in_folder(tab, Some(id));
            self.edit_folder_later(id);
        }
    }

    /// Opens the editor of a folder once the list has settled.
    fn edit_folder_later(self: &Rc<Self>, id: u32) {
        let this = self.clone();
        glib::idle_add_local_once(move || {
            let row = this.folder_rows.borrow().get(&id).map(|f| f.row.clone());
            if let Some(row) = row {
                this.show_folder_editor(Some(id), row.upcast_ref());
            }
        });
    }

    pub(super) fn delete_folder(self: &Rc<Self>, id: u32) {
        self.folders.borrow_mut().remove(id);
        // Its tabs stay where they are, out of any folder (a smart rule that is gone no longer
        // holds them; a manual choice of this folder falls back to the rules of others).
        let members: Vec<TabId> = self
            .model
            .borrow()
            .tabs()
            .iter()
            .filter(|t| t.folder == Some(id))
            .map(|t| t.id)
            .collect();
        for t in members {
            self.model.borrow_mut().set_folder(t, None);
        }
        self.rebuild_folder_rows();
    }

    fn show_folder_menu(self: &Rc<Self>, id: u32, row: &gtk::ListBoxRow, x: f64, y: f64) {
        let pop = gtk::Popover::new();
        pop.set_parent(row);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let entry = |label: &str, f: Handler| {
            let btn = gtk::Button::builder()
                .label(label)
                .css_classes(["flat"])
                .build();
            if let Some(l) = btn.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let this = Rc::downgrade(self);
            let pop = pop.clone();
            btn.connect_clicked(move |_| {
                pop.popdown();
                if let Some(b) = this.upgrade() {
                    f(&b);
                }
            });
            col.append(&btn);
        };
        entry(
            "Editar carpeta…",
            Box::new(move |b| b.edit_folder_later(id)),
        );
        entry(
            "Cerrar las pestañas de la carpeta",
            Box::new(move |b| {
                let ids: Vec<TabId> = {
                    let m = b.model.borrow();
                    let f = b.folders.borrow();
                    m.tabs()
                        .iter()
                        .filter(|t| {
                            f.folder_of(t.folder, t.space, &t.url, &t.title) == Some(id)
                                && !t.pinned
                        })
                        .map(|t| t.id)
                        .collect()
                };
                for t in ids {
                    b.close_tab(t);
                }
            }),
        );
        entry("Eliminar carpeta", Box::new(move |b| b.delete_folder(id)));
        pop.set_child(Some(&col));
        let pop2 = pop.clone();
        pop.connect_closed(move |_| pop2.unparent());
        pop.popup();
    }

    /// The popover that makes a folder (`id` is `None`) or edits one: a name and, optionally, a
    /// rule that turns it into a smart folder.
    pub(super) fn show_folder_editor(self: &Rc<Self>, id: Option<u32>, anchor: &gtk::Widget) {
        let current = id.and_then(|i| self.folders.borrow().get(i).cloned());
        let pop = gtk::Popover::new();
        pop.set_parent(anchor);
        let col = gtk::Box::new(gtk::Orientation::Vertical, 10);
        col.set_margin_top(12);
        col.set_margin_bottom(12);
        col.set_margin_start(12);
        col.set_margin_end(12);
        col.set_size_request(260, -1);
        let name = gtk::Entry::builder()
            .text(current.as_ref().map_or("", |f| f.name.as_str()))
            .placeholder_text("Nombre de la carpeta")
            .max_length(24)
            .build();
        let (kind_now, text_now) = current.as_ref().map_or(("manual", ""), |f| f.rule.parts());
        let kind = gtk::DropDown::from_strings(&[
            "Manual: tú pones las pestañas",
            "Inteligente: por sitio",
            "Inteligente: por texto",
        ]);
        kind.set_selected(kind_index(kind_now));
        let rule = gtk::Entry::builder().text(text_now).max_length(60).build();
        let sync_rule = {
            let rule = rule.clone();
            move |dropdown: &gtk::DropDown| {
                let selected = dropdown.selected();
                rule.set_visible(selected != 0);
                rule.set_placeholder_text(Some(if selected == 1 {
                    "github.com"
                } else {
                    "receta, factura…"
                }));
            }
        };
        sync_rule(&kind);
        kind.connect_selected_notify(sync_rule);
        let hint = gtk::Label::builder()
            .label("Una carpeta inteligente recoge sola las pestañas que coinciden.")
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        let save = gtk::Button::builder()
            .label(if id.is_some() { "Guardar" } else { "Crear" })
            .css_classes(["suggested-action"])
            .build();
        col.append(&name);
        col.append(&kind);
        col.append(&rule);
        col.append(&hint);
        col.append(&save);
        pop.set_child(Some(&col));

        let this = Rc::downgrade(self);
        let pop2 = pop.clone();
        let (name2, kind2, rule2) = (name.clone(), kind.clone(), rule.clone());
        save.connect_clicked(move |_| {
            pop2.popdown();
            let Some(b) = this.upgrade() else { return };
            let kind = match kind2.selected() {
                1 => "site",
                2 => "text",
                _ => "manual",
            };
            let rule = Rule::from_form(kind, &rule2.text());
            match id {
                Some(id) => b.folders.borrow_mut().edit(id, &name2.text(), rule),
                None => {
                    let space = b.model.borrow().current_space();
                    b.folders.borrow_mut().add(space, &name2.text(), rule);
                }
            }
            // After the popover has gone: its anchor may be replaced.
            glib::idle_add_local_once(move || b.rebuild_folder_rows());
        });
        let save2 = save.clone();
        name.connect_activate(move |_| save2.emit_clicked());
        let pop3 = pop.clone();
        pop.connect_closed(move |_| pop3.unparent());
        pop.popup();
        name.grab_focus();
    }
}
