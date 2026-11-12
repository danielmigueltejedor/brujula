//! Bookmarks, spaces and the horizontal tab bar of the browser window.

use super::*;

use brujula_core::icons;

impl Browser {
    pub(super) fn horizontal(&self) -> bool {
        self.settings.borrow().appearance.tabs == TabPosition::Horizontal
    }

    pub(super) fn save_bookmarks(&self) {
        let _ = paths::write_atomic(&paths::bookmarks_file(), &self.bookmarks.borrow().to_json());
    }

    /// Adds the page in front to the bookmarks, or takes it out.
    pub(super) fn toggle_bookmark(&self) {
        let Some(v) = self.current_view() else { return };
        let Some(url) = v.uri().map(|u| u.to_string()) else {
            return;
        };
        let title = v.title().map(|t| t.to_string()).unwrap_or_default();
        self.bookmarks.borrow_mut().toggle(&url, &title);
        self.save_bookmarks();
        self.refresh_bookmark_action();
    }

    /// Keeps the star in step with the page in front.
    pub(super) fn refresh_bookmark_action(&self) {
        let url = self
            .current_view()
            .and_then(|v| v.uri())
            .map(|u| u.to_string())
            .unwrap_or_default();
        let marked = self.bookmarks.borrow().contains(&url);
        if let Some(a) = self.window.lookup_action("toggle-bookmark") {
            if let Some(a) = a.downcast_ref::<gio::SimpleAction>() {
                a.set_state(&marked.to_variant());
                a.set_enabled(url.starts_with("https://") || url.starts_with("http://"));
            }
        }
        let tip = if marked {
            "Quitar de marcadores (Ctrl+D)"
        } else {
            "Añadir a marcadores (Ctrl+D)"
        };
        for c in &self.chromes {
            c.star.set_tooltip_text(Some(tip));
        }
    }

    /// Makes the space of tab `id` the one in front, e.g. after picking a tab in the Spotlight.
    pub(super) fn follow_space_of(self: &Rc<Self>, id: TabId) {
        let Some(space) = self.model.borrow().get(id).map(|t| t.space) else {
            return;
        };
        self.last_in_space.borrow_mut().insert(space, id);
        let changed = self.spaces.borrow_mut().set_current(space);
        if changed {
            self.model.borrow_mut().set_current_space(space);
            self.list.invalidate_filter();
            self.rebuild_spaces();
        }
    }

    pub(super) fn switch_space(self: &Rc<Self>, id: u32) {
        if self.spaces.borrow().current() == id || !self.spaces.borrow().contains(id) {
            return;
        }
        self.spaces.borrow_mut().set_current(id);
        self.model.borrow_mut().set_current_space(id);
        self.list.invalidate_filter();
        self.rebuild_spaces();
        let target = {
            let m = self.model.borrow();
            self.last_in_space
                .borrow()
                .get(&id)
                .copied()
                .filter(|t| m.get(*t).is_some_and(|x| x.space == id))
                .or_else(|| m.in_space(id).next().map(|t| t.id))
        };
        match target {
            Some(t) => {
                let c = self.model.borrow_mut().select(t);
                self.apply(c);
            }
            None => self.open_tab(newtab::NEWTAB_URI, None, true),
        }
    }

    pub(super) fn move_tab_to_space(self: &Rc<Self>, id: TabId, space: u32) {
        if !self.spaces.borrow().contains(space) {
            return;
        }
        let (from, selected) = {
            let m = self.model.borrow();
            let Some(tab) = m.get(id) else { return };
            (tab.space, m.selected() == Some(id))
        };
        if from == space {
            return;
        }
        if selected {
            // Something else of this space comes to the front, or a blank tab.
            let other = self
                .model
                .borrow()
                .in_space(from)
                .find(|t| t.id != id)
                .map(|t| t.id);
            match other {
                Some(o) => {
                    let c = self.model.borrow_mut().select(o);
                    self.apply(c);
                }
                None => self.open_tab(newtab::NEWTAB_URI, None, true),
            }
        }
        let c = self.model.borrow_mut().set_space(id, space);
        self.apply(c);
    }

    /// The switcher at the foot of the sidebar: one button per space and a plus.
    pub(super) fn rebuild_spaces(self: &Rc<Self>) {
        while let Some(child) = self.space_bar.first_child() {
            self.space_bar.remove(&child);
        }
        let (all, current) = {
            let s = self.spaces.borrow();
            (s.all().to_vec(), s.current())
        };
        for space in all {
            let button = gtk::Button::builder()
                .icon_name(space.icon_name())
                .tooltip_text(&space.name)
                .css_classes(["flat", "brujula-space"])
                .build();
            button.set_widget_name(&format!("space-{}", space.id));
            if space.id == current {
                button.add_css_class("brujula-space-current");
            }
            let id = space.id;
            let this = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(b) = this.upgrade() {
                    b.switch_space(id);
                }
            });
            let click = gtk::GestureClick::builder()
                .button(gdk::BUTTON_SECONDARY)
                .build();
            let this = Rc::downgrade(self);
            let anchor = button.clone();
            click.connect_pressed(move |_, _, _, _| {
                if let Some(b) = this.upgrade() {
                    b.show_space_menu(id, &anchor);
                }
            });
            button.add_controller(click);
            self.space_bar.append(&button);
        }
        let add = gtk::Button::builder()
            .icon_name("ndl-plus-symbolic")
            .tooltip_text("Nuevo espacio")
            .css_classes(["flat", "brujula-space"])
            .build();
        let this = Rc::downgrade(self);
        add.connect_clicked(move |_| {
            let Some(b) = this.upgrade() else { return };
            let id = b.spaces.borrow_mut().add("");
            if let Some(id) = id {
                b.switch_space(id);
                // A new space starts with a name and an icon the user probably wants to change.
                glib::idle_add_local_once(move || b.edit_space(id));
            }
        });
        self.space_bar.append(&add);
    }

    /// A change to a space made on the preferences page: `remove`, `name.<id>` or `icon.<id>`.
    pub(super) fn change_space(self: &Rc<Self>, what: &str, value: &str) {
        if what == "remove" {
            if let Ok(id) = value.parse::<u32>() {
                self.delete_space(id);
            }
            return;
        }
        let Some((field, id)) = what.split_once('.') else {
            return;
        };
        let Ok(id) = id.parse::<u32>() else { return };
        let Some(space) = self.spaces.borrow().get(id).cloned() else {
            return;
        };
        match field {
            "name" => self.spaces.borrow_mut().rename(id, value, &space.icon),
            "icon" => self.spaces.borrow_mut().rename(id, &space.name, value),
            _ => return,
        }
        self.rebuild_spaces();
    }

    /// Removes a space; its tabs go to the one before it.
    pub(super) fn delete_space(self: &Rc<Self>, id: u32) {
        let heir = self.spaces.borrow_mut().remove(id);
        let Some(heir) = heir else { return };
        self.last_in_space.borrow_mut().remove(&id);
        let current = self.spaces.borrow().current();
        self.model.borrow_mut().set_current_space(current);
        let changes = self.model.borrow_mut().move_space(id, heir);
        self.apply(changes);
        let this = self.clone();
        glib::idle_add_local_once(move || {
            this.list.invalidate_filter();
            this.rebuild_spaces();
        });
    }

    /// Opens the editor of a space from its button in the switcher.
    pub(super) fn edit_space(self: &Rc<Self>, id: u32) {
        let name = format!("space-{id}");
        let mut child = self.space_bar.first_child();
        while let Some(w) = child {
            if w.widget_name() == name {
                if let Ok(button) = w.clone().downcast::<gtk::Button>() {
                    self.show_space_menu(id, &button);
                }
                return;
            }
            child = w.next_sibling();
        }
    }

    /// Rename, pick the icon of, or delete a space.
    fn show_space_menu(self: &Rc<Self>, id: u32, anchor: &gtk::Button) {
        let Some(space) = self.spaces.borrow().get(id).cloned() else {
            return;
        };
        let pop = gtk::Popover::new();
        pop.set_parent(anchor);
        let col = gtk::Box::new(gtk::Orientation::Vertical, 10);
        col.set_margin_top(12);
        col.set_margin_bottom(12);
        col.set_margin_start(12);
        col.set_margin_end(12);
        let name = gtk::Entry::builder()
            .text(&space.name)
            .placeholder_text("Nombre")
            .max_length(24)
            .build();
        // The icon is one of the set Brújula draws in NDL's style, picked from a grid.
        let chosen = Rc::new(RefCell::new(space.icon.clone()));
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(6)
            .max_children_per_line(6)
            .row_spacing(2)
            .column_spacing(2)
            .homogeneous(true)
            .build();
        let buttons: Rc<RefCell<Vec<(String, gtk::Button)>>> = Rc::new(RefCell::new(Vec::new()));
        let mark = {
            let buttons = buttons.clone();
            move |current: &str| {
                for (icon_id, button) in buttons.borrow().iter() {
                    if icon_id == current {
                        button.add_css_class("brujula-icon-current");
                    } else {
                        button.remove_css_class("brujula-icon-current");
                    }
                }
            }
        };
        for icon in icons::all() {
            let button = gtk::Button::builder()
                .icon_name(icons::gtk_name(icon.id))
                .tooltip_text(icon.label)
                .css_classes(["flat", "brujula-icon-choice"])
                .build();
            let chosen = chosen.clone();
            let mark = mark.clone();
            let icon_id = icon.id;
            button.connect_clicked(move |_| {
                *chosen.borrow_mut() = icon_id.to_owned();
                mark(icon_id);
            });
            grid.append(&button);
            buttons.borrow_mut().push((icon.id.to_owned(), button));
        }
        let current_icon = if icons::is_known(&space.icon) {
            space.icon.clone()
        } else {
            icons::DEFAULT.to_owned()
        };
        *chosen.borrow_mut() = current_icon.clone();
        mark(&current_icon);
        let save = gtk::Button::builder()
            .label("Guardar")
            .css_classes(["suggested-action"])
            .build();
        let delete = gtk::Button::builder()
            .label("Eliminar espacio")
            .css_classes(["flat", "destructive-action"])
            .sensitive(self.spaces.borrow().all().len() > 1)
            .build();
        col.append(&name);
        col.append(&grid);
        col.append(&save);
        col.append(&delete);
        pop.set_child(Some(&col));

        let this = Rc::downgrade(self);
        let pop2 = pop.clone();
        let name2 = name.clone();
        let chosen2 = chosen.clone();
        save.connect_clicked(move |_| {
            pop2.popdown();
            let Some(b) = this.upgrade() else { return };
            b.spaces
                .borrow_mut()
                .rename(id, &name2.text(), &chosen2.borrow());
            // After the popover has gone: its anchor is about to be replaced.
            glib::idle_add_local_once(move || b.rebuild_spaces());
        });
        let save2 = save.clone();
        name.connect_activate(move |_| save2.emit_clicked());

        let this = Rc::downgrade(self);
        let pop3 = pop.clone();
        delete.connect_clicked(move |_| {
            pop3.popdown();
            if let Some(b) = this.upgrade() {
                b.delete_space(id);
            }
        });
        let pop4 = pop.clone();
        pop.connect_closed(move |_| pop4.unparent());
        pop.popup();
        name.grab_focus();
    }

    /// The tabs of the space in front as chips, for the horizontal layout.
    pub(super) fn rebuild_strip(self: &Rc<Self>) {
        while let Some(child) = self.strip.first_child() {
            self.strip.remove(&child);
        }
        let (tabs, selected) = {
            let m = self.model.borrow();
            (
                m.in_space(m.current_space()).cloned().collect::<Vec<_>>(),
                m.selected(),
            )
        };
        for tab in tabs {
            let id = tab.id;
            let title = if !tab.title.is_empty() {
                tab.title.clone()
            } else if tab.url == prefs::SETTINGS_URI {
                "Preferencias".to_owned()
            } else if tab.url.is_empty()
                || tab.url == "about:blank"
                || tab.url == newtab::NEWTAB_URI
            {
                "Nueva pestaña".to_owned()
            } else {
                host_of(&tab.url)
            };
            let icon: gtk::Widget = match self.views.borrow().get(&id).and_then(|v| v.favicon()) {
                Some(tex) => favicons::picture(&tex),
                None => {
                    let img = gtk::Image::from_icon_name("ndl-globe-symbolic");
                    img.set_pixel_size(16);
                    img.upcast()
                }
            };
            let label = gtk::Label::builder()
                .label(&title)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(20)
                .xalign(0.0)
                .build();
            let close = gtk::Button::builder()
                .icon_name("ndl-close-symbolic")
                .css_classes(["flat", "circular", "brujula-tab-close"])
                .tooltip_text("Cerrar pestaña")
                .valign(gtk::Align::Center)
                .build();
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            inner.append(&icon);
            if !tab.pinned {
                inner.append(&label);
                inner.append(&close);
            }
            let chip = gtk::Button::builder()
                .child(&inner)
                .tooltip_text(&title)
                .css_classes(["flat", "brujula-chip"])
                .build();
            if selected == Some(id) {
                chip.add_css_class("brujula-chip-current");
                chip.add_css_class("brujula-current");
            }
            let this = Rc::downgrade(self);
            chip.connect_clicked(move |_| {
                if let Some(b) = this.upgrade() {
                    let c = b.model.borrow_mut().select(id);
                    b.apply(c);
                }
            });
            let this = Rc::downgrade(self);
            close.connect_clicked(move |_| {
                if let Some(b) = this.upgrade() {
                    b.close_tab(id);
                }
            });
            let click = gtk::GestureClick::builder().button(0).build();
            let this = Rc::downgrade(self);
            let anchor = chip.clone();
            click.connect_pressed(move |g, _, x, y| {
                let Some(b) = this.upgrade() else { return };
                match g.current_button() {
                    gdk::BUTTON_MIDDLE => b.close_tab(id),
                    gdk::BUTTON_SECONDARY => b.show_tab_menu(id, &anchor, x, y),
                    _ => {}
                }
            });
            chip.add_controller(click);
            self.strip.append(&chip);
        }
    }
}
