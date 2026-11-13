//! Questions and notices in the look of the app: Nodalia Design dialogs over the window, not
//! the plain GTK ones.

use nodalia_gtk as ndl;
use nodalia_gtk::gtk;
use nodalia_gtk::gtk::prelude::*;
use nodalia_gtk::ResponseAppearance;

/// A message with one button.
pub fn notice(parent: &impl IsA<gtk::Widget>, heading: &str, body: &str) {
    let dialog = ndl::AlertDialog::new(Some(heading), Some(body));
    dialog.add_response("ok", "Entendido");
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// One answer of a question: its id, its label and how it looks.
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
    pub look: ResponseAppearance,
}

impl Choice {
    pub const fn plain(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            look: ResponseAppearance::Default,
        }
    }

    pub const fn suggested(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            look: ResponseAppearance::Suggested,
        }
    }

    pub const fn destructive(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            look: ResponseAppearance::Destructive,
        }
    }
}

/// A question with answers; `cancel` is what Escape or a click outside answers, `default` what
/// Enter answers. `done` gets the id of the answer.
pub fn ask(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    body: &str,
    choices: &[Choice],
    default: &str,
    cancel: &str,
    done: impl Fn(&str) + 'static,
) {
    let dialog = ndl::AlertDialog::new(Some(heading), Some(body));
    for c in choices {
        dialog.add_response(c.id, c.label);
        dialog.set_response_appearance(c.id, c.look);
    }
    dialog.set_default_response(Some(default));
    dialog.set_close_response(cancel);
    dialog.connect_response(None, move |_, id| done(id));
    dialog.present(Some(parent));
}

/// A dialog with `entry` under the text, «Cancelar» and a suggested «ok_label» that Enter answers.
fn entry_dialog(
    heading: &str,
    body: &str,
    ok_label: &str,
    entry: &impl IsA<gtk::Widget>,
) -> ndl::AlertDialog {
    let dialog = ndl::AlertDialog::new(Some(heading), Some(body));
    dialog.set_extra_child(Some(entry));
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("ok", ok_label);
    dialog.set_response_appearance("ok", ResponseAppearance::Suggested);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("cancel");
    dialog
}

/// Presents `dialog` over `parent`: «ok» hands what is in `entry` to `done`, and the entry has the
/// focus.
fn show_entry(
    dialog: &ndl::AlertDialog,
    parent: &impl IsA<gtk::Widget>,
    entry: &(impl IsA<gtk::Editable> + IsA<gtk::Widget>),
    done: impl Fn(String) + 'static,
) {
    let text = entry.clone();
    dialog.connect_response(Some("ok"), move |_, _| done(text.text().to_string()));
    dialog.present(Some(parent));
    entry.grab_focus();
}

/// A line of text to fill in. `done` gets it when «OK» is pressed or Enter is.
pub fn ask_text(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    body: &str,
    initial: &str,
    ok_label: &str,
    max_len: i32,
    done: impl Fn(String) + 'static,
) {
    let entry = gtk::Entry::builder()
        .text(initial)
        .max_length(max_len)
        .activates_default(false)
        .build();
    let dialog = entry_dialog(heading, body, ok_label, &entry);
    let activate = dialog.clone();
    show_entry(&dialog, parent, &entry, done);
    entry.connect_activate(move |_| activate.activate_default());
}

/// A password to type. `done` gets it when «ok_label» is pressed or Enter is; it may be empty.
pub fn ask_secret(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    body: &str,
    ok_label: &str,
    done: impl Fn(String) + 'static,
) {
    let entry = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .activates_default(false)
        .build();
    let dialog = entry_dialog(heading, body, ok_label, &entry);
    let activate = dialog.clone();
    show_entry(&dialog, parent, &entry, done);
    entry.connect_activate(move |_| activate.activate_default());
}
