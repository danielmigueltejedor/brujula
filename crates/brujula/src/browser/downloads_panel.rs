//! The downloads panel: a button (bottom left of the sidebar, in the bar when the tabs are on
//! top) that opens a list of what was downloaded, with its progress, and opens the file or its
//! folder. The rules about what may be saved are in `brujula_core::downloads`.

use std::path::PathBuf;

use webkit6::Download;

use super::*;
use crate::save_download::Decision;

/// How many downloads the list keeps.
const KEEP: usize = 30;

#[derive(Clone, PartialEq)]
enum State {
    Running,
    /// It has arrived and is being looked at.
    Scanning,
    Done,
    Failed(String),
    Cancelled,
    Blocked,
    /// Deleted after the check: why.
    Removed(String),
}

struct Item {
    row: gtk::ListBoxRow,
    download: Download,
    state: Rc<RefCell<State>>,
}

/// The button, its panel and what is in it.
pub(super) struct DownloadsUi {
    pub button: gtk::MenuButton,
    pub slot_side: gtk::Box,
    pub slot_top: gtk::Box,
    list: gtk::ListBox,
    empty: gtk::Label,
    clear: gtk::Button,
    items: RefCell<Vec<Item>>,
}

impl DownloadsUi {
    pub fn new() -> Self {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("brujula-downloads-list");
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(340)
            .max_content_height(380)
            .propagate_natural_height(true)
            .build();
        let empty = gtk::Label::builder()
            .label("Todavía no has descargado nada.")
            .css_classes(["dim-label"])
            .margin_top(18)
            .margin_bottom(18)
            .build();
        let title = gtk::Label::builder()
            .label("Descargas")
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-4"])
            .build();
        let clear = gtk::Button::builder()
            .label("Limpiar")
            .css_classes(["flat"])
            .tooltip_text("Quita de la lista lo que ya terminó")
            .build();
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.set_margin_start(12);
        head.set_margin_end(8);
        head.set_margin_top(8);
        head.append(&title);
        head.append(&clear);
        let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
        col.set_margin_bottom(8);
        col.append(&head);
        col.append(&empty);
        col.append(&scroller);
        let popover = gtk::Popover::new();
        popover.set_child(Some(&col));
        popover.add_css_class("brujula-downloads");
        let button = gtk::MenuButton::builder()
            .icon_name("ndl-import-symbolic")
            .tooltip_text("Descargas")
            .css_classes(["flat", "brujula-newfolder"])
            .popover(&popover)
            .build();
        let slot_side = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        slot_side.append(&button);
        let slot_top = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let mut me = Self {
            button,
            slot_side,
            slot_top,
            list,
            empty,
            clear: gtk::Button::new(),
            items: RefCell::new(Vec::new()),
        };
        me.sync_empty();
        me.clear = clear;
        me
    }

    /// Takes what is no longer running off the list.
    fn forget_finished(&self) {
        let mut items = self.items.borrow_mut();
        items.retain(|i| {
            let running = matches!(*i.state.borrow(), State::Running | State::Scanning);
            if !running {
                self.list.remove(&i.row);
            }
            running
        });
        drop(items);
        self.sync_empty();
    }

    fn sync_empty(&self) {
        self.empty.set_visible(self.items.borrow().is_empty());
    }

    /// Puts the button where the layout wants it: bottom of the sidebar, or the bar on top.
    pub fn place(&self, horizontal: bool) {
        let (from, to) = if horizontal {
            (&self.slot_side, &self.slot_top)
        } else {
            (&self.slot_top, &self.slot_side)
        };
        if self.button.parent().as_ref() == Some(to.upcast_ref()) {
            return;
        }
        if self.button.parent().as_ref() == Some(from.upcast_ref()) {
            from.remove(&self.button);
        }
        to.append(&self.button);
    }
}

fn row_for(
    download: &Download,
    state: &Rc<RefCell<State>>,
    destination: &Rc<RefCell<Option<PathBuf>>>,
    window: &gtk::Window,
) -> (gtk::ListBoxRow, Box<dyn Fn()>) {
    let name = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .css_classes(["brujula-download-name"])
        .build();
    let status = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["dim-label", "brujula-download-status"])
        .build();
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("brujula-download-bar");
    let open = gtk::Button::builder()
        .icon_name("ndl-export-symbolic")
        .tooltip_text("Abrir")
        .css_classes(["flat"])
        .build();
    let folder = gtk::Button::builder()
        .icon_name("ndl-folder-symbolic")
        .tooltip_text("Mostrar en la carpeta")
        .css_classes(["flat"])
        .build();
    let cancel = gtk::Button::builder()
        .icon_name("ndl-close-symbolic")
        .tooltip_text("Cancelar la descarga")
        .css_classes(["flat"])
        .build();
    let icon = gtk::Image::from_icon_name("ndl-file-symbolic");
    icon.set_pixel_size(20);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 3);
    texts.set_hexpand(true);
    texts.append(&name);
    texts.append(&bar);
    texts.append(&status);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_start(12);
    line.set_margin_end(6);
    line.set_margin_top(6);
    line.set_margin_bottom(6);
    line.append(&icon);
    line.append(&texts);
    line.append(&open);
    line.append(&folder);
    line.append(&cancel);
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&line));

    {
        let d = download.clone();
        cancel.connect_clicked(move |_| d.cancel());
    }
    for (button, containing) in [(&open, false), (&folder, true)] {
        let destination = destination.clone();
        let window = window.clone();
        button.connect_clicked(move |_| {
            let Some(path) = destination.borrow().clone() else {
                return;
            };
            let file = gio::File::for_path(path);
            let launcher = gtk::FileLauncher::new(Some(&file));
            if containing {
                launcher.open_containing_folder(Some(&window), gio::Cancellable::NONE, |_| {});
            } else {
                launcher.launch(Some(&window), gio::Cancellable::NONE, |_| {});
            }
        });
    }

    let (download, state, destination) = (download.clone(), state.clone(), destination.clone());
    let row2 = row.clone();
    let refresh: Box<dyn Fn()> = Box::new(move || {
        let path = destination.borrow().clone();
        name.set_text(
            &path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Descarga".to_owned()),
        );
        let got = download.received_data_length();
        let total = download.response().map(|r| r.content_length()).unwrap_or(0);
        let st = state.borrow().clone();
        let running = st == State::Running;
        bar.set_visible(running);
        cancel.set_visible(running);
        open.set_visible(st == State::Done);
        folder.set_visible(st == State::Done);
        if running {
            if total > 0 {
                bar.set_fraction((got as f64 / total as f64).clamp(0.0, 1.0));
            } else {
                bar.pulse();
            }
        }
        status.set_text(&match &st {
            State::Running if total > 0 => {
                format!("{} de {}", glib::format_size(got), glib::format_size(total))
            }
            State::Running => glib::format_size(got).to_string(),
            State::Scanning => "Analizando…".to_owned(),
            State::Done => format!("Completada · {}", glib::format_size(got)),
            State::Removed(why) => format!("Borrada: {why}"),
            State::Failed(why) => format!("No se pudo descargar: {why}"),
            State::Cancelled => "Cancelada".to_owned(),
            State::Blocked => "No descargada: es un programa o un script".to_owned(),
        });
        if running {
            row2.remove_css_class("brujula-download-done");
        } else {
            row2.add_css_class("brujula-download-done");
        }
    });
    refresh();
    (row, refresh)
}

impl Browser {
    /// Downloads go to the user's Downloads folder, without a prompt. The page picks the name,
    /// so it is cleaned, and files that can run (scripts, launchers, installers) are refused: a
    /// page must not be able to drop one where a double click would start it.
    pub(super) fn wire_downloads(self: &Rc<Self>) {
        self.dl.place(self.horizontal());
        let this = Rc::downgrade(self);
        self.dl.clear.connect_clicked(move |_| {
            if let Some(b) = this.upgrade() {
                b.dl.forget_finished();
            }
        });
        let this = Rc::downgrade(self);
        self.session.connect_download_started(move |_, download| {
            let Some(b) = this.upgrade() else { return };
            b.track_download(download);
        });
    }

    fn track_download(self: &Rc<Self>, download: &Download) {
        let state = Rc::new(RefCell::new(State::Running));
        let destination: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
        let (row, refresh) = row_for(
            download,
            &state,
            &destination,
            self.window.upcast_ref::<gtk::Window>(),
        );
        let refresh: Rc<dyn Fn()> = Rc::from(refresh);
        {
            // Newest first; the oldest go when there are too many.
            self.dl.list.prepend(&row);
            let mut items = self.dl.items.borrow_mut();
            items.insert(
                0,
                Item {
                    row: row.clone(),
                    download: download.clone(),
                    state: state.clone(),
                },
            );
            while items.len() > KEEP {
                if let Some(old) = items.pop() {
                    self.dl.list.remove(&old.row);
                    if *old.state.borrow() == State::Running {
                        old.download.cancel();
                    }
                }
            }
        }
        self.dl.sync_empty();

        let this = Rc::downgrade(self);
        let (st, dest, rf) = (state.clone(), destination.clone(), refresh.clone());
        download.connect_decide_destination(move |d, suggested| {
            let Some(b) = this.upgrade() else {
                d.cancel();
                return true;
            };
            let (st, dest, rf) = (st.clone(), dest.clone(), rf.clone());
            let shown = this.clone();
            crate::save_download::decide(&b.window, d, suggested, move |decision| {
                match decision {
                    Decision::Saving(path) => {
                        *dest.borrow_mut() = Some(path);
                        // The list opens when the download starts, not while a question about it
                        // is still open (its first click would only close the list).
                        if let Some(b) = shown.upgrade() {
                            b.dl.button.popup();
                        }
                    }
                    Decision::Refused(name) => {
                        *st.borrow_mut() = State::Blocked;
                        *dest.borrow_mut() = Some(PathBuf::from(name));
                    }
                    Decision::Scanning => *st.borrow_mut() = State::Scanning,
                    Decision::Finished => *st.borrow_mut() = State::Done,
                    Decision::Removed(why) => *st.borrow_mut() = State::Removed(why),
                }
                rf();
            });
            true
        });
        let rf = refresh.clone();
        download.connect_received_data(move |_, _| rf());
        let (st, rf) = (state.clone(), refresh.clone());
        download.connect_failed(move |_, error| {
            let mut s = st.borrow_mut();
            if *s == State::Running {
                *s = if error.matches(webkit6::DownloadError::CancelledByUser) {
                    State::Cancelled
                } else {
                    State::Failed(error.message().to_string())
                };
            }
            drop(s);
            rf();
        });
    }
}
