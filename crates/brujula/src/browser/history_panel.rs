//! The history button: it slides the history in over the list of tabs in the sidebar (and back),
//! or, with the tabs on top, drops it from the bar as a list. A click on a page opens it in a new
//! tab. What the history keeps and how it is cleaned is in `history.rs`.

use super::*;

/// How many pages the list shows.
const SHOWN: usize = 200;

/// The widgets of the history list.
pub(super) struct HistoryUi {
    /// Sidebar: the button at the bottom, which slides the history in.
    pub side_button: gtk::ToggleButton,
    /// Tabs on top: the button of the bar, which drops the history as a list.
    pub top_button: gtk::MenuButton,
    /// The tabs and the history, one over the other in the sidebar.
    pub stack: gtk::Stack,
    pane: gtk::Box,
    popover: gtk::Popover,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    empty: gtk::Label,
    back: gtk::Button,
    /// The page that each row of the list opens (`None` for a heading).
    urls: RefCell<Vec<Option<String>>>,
}

fn history_icon() -> &'static str {
    if nodalia_gtk::has_icon("ndl-history-symbolic") {
        "ndl-history-symbolic"
    } else {
        "document-open-recent-symbolic"
    }
}

/// «Hoy», «Ayer» or the date, for the heading over the pages of a day.
fn day_label(last_ms: u64) -> String {
    let (Ok(then), Ok(now)) = (
        glib::DateTime::from_unix_local(i64::try_from(last_ms / 1000).unwrap_or(0)),
        glib::DateTime::now_local(),
    ) else {
        return String::new();
    };
    if then.ymd() == now.ymd() {
        return "Hoy".to_owned();
    }
    if now
        .add_days(-1)
        .is_ok_and(|yesterday| yesterday.ymd() == then.ymd())
    {
        return "Ayer".to_owned();
    }
    then.format("%d/%m/%Y")
        .map(|s| s.to_string())
        .unwrap_or_default()
}

impl HistoryUi {
    /// `tabs` is the list of tabs, which this puts in the stack as its first page.
    pub fn new(tabs: &impl IsA<gtk::Widget>) -> Self {
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::SlideLeftRight)
            .transition_duration(220)
            .vexpand(true)
            .build();
        stack.add_named(tabs, Some("tabs"));

        let back = gtk::Button::builder()
            .icon_name("ndl-arrow-left-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Volver a las pestañas")
            .build();
        let title = gtk::Label::builder()
            .label("Historial")
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-4"])
            .build();
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        head.set_margin_start(4);
        head.set_margin_end(8);
        head.set_margin_bottom(4);
        head.append(&back);
        head.append(&title);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Buscar en el historial")
            .margin_start(8)
            .margin_end(8)
            .margin_bottom(6)
            .build();
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");
        list.add_css_class("brujula-history-list");
        list.set_activate_on_single_click(true);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .margin_start(8)
            .margin_end(8)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let empty = gtk::Label::builder()
            .label("No hay nada en el historial.")
            .css_classes(["dim-label"])
            .wrap(true)
            .max_width_chars(1)
            .margin_top(18)
            .margin_bottom(18)
            .build();
        let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pane.add_css_class("brujula-history");
        pane.append(&head);
        pane.append(&search);
        pane.append(&empty);
        pane.append(&scroller);
        stack.add_named(&pane, Some("history"));

        let side_button = gtk::ToggleButton::builder()
            .icon_name(history_icon())
            .tooltip_text("Historial (Ctrl+H)")
            .css_classes(["flat", "brujula-newfolder"])
            .build();
        let popover = gtk::Popover::new();
        popover.add_css_class("brujula-history-pop");
        let top_button = gtk::MenuButton::builder()
            .icon_name(history_icon())
            .tooltip_text("Historial (Ctrl+H)")
            .css_classes(["flat", "brujula-newfolder"])
            .popover(&popover)
            .visible(false)
            .build();
        Self {
            side_button,
            top_button,
            stack,
            pane,
            popover,
            list,
            search,
            empty,
            back,
            urls: RefCell::new(Vec::new()),
        }
    }

    /// Whether the history list is showing, in the sidebar or in the popover.
    fn open(&self) -> bool {
        self.side_button.is_active() || self.popover.is_visible()
    }
}

impl Browser {
    pub(super) fn wire_history_panel(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        self.hist.side_button.connect_toggled(move |button| {
            let Some(b) = this.upgrade() else { return };
            b.hist.stack.set_visible_child_name(if button.is_active() {
                "history"
            } else {
                "tabs"
            });
            if button.is_active() {
                b.refill_history();
                b.hist.search.grab_focus();
            }
        });
        let button = self.hist.side_button.clone();
        self.hist.back.connect_clicked(move |_| {
            button.set_active(false);
        });
        let popover = self.hist.popover.clone();
        self.hist.back.connect_clicked(move |_| popover.popdown());
        let this = Rc::downgrade(self);
        self.hist.popover.connect_show(move |_| {
            if let Some(b) = this.upgrade() {
                b.refill_history();
                b.hist.search.grab_focus();
            }
        });
        let this = Rc::downgrade(self);
        self.hist.search.connect_search_changed(move |_| {
            if let Some(b) = this.upgrade() {
                b.refill_history();
            }
        });
        let this = Rc::downgrade(self);
        self.hist.list.connect_row_activated(move |_, row| {
            let Some(b) = this.upgrade() else { return };
            let url = usize::try_from(row.index())
                .ok()
                .and_then(|i| b.hist.urls.borrow().get(i).cloned().flatten());
            if let Some(url) = url {
                b.hist.side_button.set_active(false);
                b.hist.popover.popdown();
                b.open_tab(&url, None, true);
            }
        });
        self.place_history(self.horizontal());
    }

    /// The history goes where the layout wants it: over the tabs in the sidebar, or in the
    /// popover of the bar.
    pub(super) fn place_history(&self, horizontal: bool) {
        let ui = &self.hist;
        ui.side_button.set_visible(!horizontal);
        ui.top_button.set_visible(horizontal);
        let in_stack = ui.pane.parent().as_ref() == Some(ui.stack.upcast_ref());
        if horizontal && in_stack {
            ui.side_button.set_active(false);
            ui.stack.set_visible_child_name("tabs");
            ui.stack.remove(&ui.pane);
            ui.pane.set_size_request(340, 420);
            ui.popover.set_child(Some(&ui.pane));
        } else if !horizontal && !in_stack {
            ui.popover.popdown();
            ui.popover.set_child(None::<&gtk::Widget>);
            ui.pane.set_size_request(-1, -1);
            ui.stack.add_named(&ui.pane, Some("history"));
        }
    }

    /// Ctrl+H: shows the history or goes back to the tabs.
    pub(super) fn toggle_history(self: &Rc<Self>) {
        if self.horizontal() {
            if self.hist.popover.is_visible() {
                self.hist.popover.popdown();
            } else {
                self.hist.top_button.popup();
            }
        } else {
            let on = !self.hist.side_button.is_active();
            self.hist.side_button.set_active(on);
        }
    }

    /// Fills the list with the pages that match what is typed in the search, newest first, under
    /// a heading for each day.
    fn refill_history(&self) {
        let ui = &self.hist;
        while let Some(row) = ui.list.first_child() {
            ui.list.remove(&row);
        }
        let query = ui.search.text().to_string();
        let visits: Vec<_> = {
            let h = self.history.borrow();
            if query.trim().is_empty() {
                h.items().iter().take(SHOWN).cloned().collect()
            } else {
                h.search(&query, SHOWN).into_iter().cloned().collect()
            }
        };
        let mut urls = Vec::with_capacity(visits.len());
        let mut day = String::new();
        for visit in &visits {
            let label = day_label(visit.last);
            if label != day {
                day = label;
                let heading = gtk::Label::builder()
                    .label(&day)
                    .xalign(0.0)
                    .margin_start(8)
                    .margin_top(10)
                    .margin_bottom(2)
                    .css_classes(["dim-label", "caption-heading"])
                    .build();
                let row = gtk::ListBoxRow::new();
                row.set_activatable(false);
                row.set_selectable(false);
                row.set_child(Some(&heading));
                ui.list.append(&row);
                urls.push(None);
            }
            ui.list.append(&history_row(visit));
            urls.push(Some(visit.url.clone()));
        }
        ui.empty.set_visible(visits.is_empty());
        ui.empty.set_label(if query.trim().is_empty() {
            "No hay nada en el historial."
        } else {
            "No hay nada que coincida."
        });
        *ui.urls.borrow_mut() = urls;
    }

    /// The history changed (a page was visited, cleaned): redraws the list if it is showing.
    pub(super) fn history_changed(&self) {
        if self.hist.open() {
            self.refill_history();
        }
    }
}

fn history_row(visit: &brujula_core::history::Visit) -> gtk::ListBoxRow {
    let title = if visit.title.is_empty() {
        host_of(&visit.url)
    } else {
        visit.title.clone()
    };
    let name = gtk::Label::builder()
        .label(&title)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["brujula-tab-title"])
        .build();
    let host = gtk::Label::builder()
        .label(host_of(&visit.url))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["dim-label", "caption"])
        .build();
    let col = gtk::Box::new(gtk::Orientation::Vertical, 1);
    col.set_margin_start(8);
    col.set_margin_end(8);
    col.set_margin_top(5);
    col.set_margin_bottom(5);
    col.append(&name);
    col.append(&host);
    let row = gtk::ListBoxRow::new();
    row.add_css_class("brujula-row");
    row.set_tooltip_text(Some(&visit.url));
    row.set_child(Some(&col));
    row
}
