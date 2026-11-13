//! Reordering the tabs by dragging them: the dragged tab goes before or after the one it is
//! dropped on, and takes that tab's folder. A line shows where it will land.

use super::*;

/// The half of a row the pointer is over decides whether the tab lands before or after it.
fn after_half(y: f64, height: i32) -> bool {
    y > f64::from(height) / 2.0
}

/// Where in the model the dragged tab at `from` ends up when dropped on the tab at `target`:
/// `to` is an index in the list as it is once the dragged tab has been taken out and put back.
fn landing(from: usize, target: usize, after: bool) -> usize {
    match (from < target, after) {
        (true, true) => target,
        (true, false) => target - 1,
        (false, true) => target + 1,
        (false, false) => target,
    }
}

impl Browser {
    /// Makes the row of tab `id` draggable and a place to drop other tabs.
    pub(super) fn wire_tab_drag(self: &Rc<Self>, row: &gtk::ListBoxRow, id: TabId) {
        let source = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE)
            .build();
        source.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(
                &id.0.to_string().to_value(),
            ))
        });
        let dragged = row.clone();
        source.connect_drag_begin(move |s, _| {
            s.set_icon(Some(&gtk::WidgetPaintable::new(Some(&dragged))), 0, 0);
            dragged.add_css_class("brujula-dragging");
        });
        let dragged = row.clone();
        source.connect_drag_end(move |_, _, _| dragged.remove_css_class("brujula-dragging"));
        row.add_controller(source);

        let target = gtk::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        let over = row.clone();
        target.connect_motion(move |_, _, y| {
            let after = after_half(y, over.height());
            over.add_css_class(if after {
                "brujula-drop-after"
            } else {
                "brujula-drop-before"
            });
            over.remove_css_class(if after {
                "brujula-drop-before"
            } else {
                "brujula-drop-after"
            });
            gdk::DragAction::MOVE
        });
        let over = row.clone();
        target.connect_leave(move |_| {
            over.remove_css_class("brujula-drop-before");
            over.remove_css_class("brujula-drop-after");
        });
        let this = Rc::downgrade(self);
        let over = row.clone();
        target.connect_drop(move |_, value, _, y| {
            over.remove_css_class("brujula-drop-before");
            over.remove_css_class("brujula-drop-after");
            let (Some(b), Ok(text)) = (this.upgrade(), value.get::<String>()) else {
                return false;
            };
            let Ok(from) = text.parse::<u64>() else {
                return false;
            };
            b.drop_tab(TabId(from), id, after_half(y, over.height()));
            true
        });
        row.add_controller(target);
    }

    /// Puts `dragged` before or after `target`, in `target`'s folder.
    fn drop_tab(self: &Rc<Self>, dragged: TabId, target: TabId, after: bool) {
        if dragged == target {
            return;
        }
        let (positions, folder) = {
            let m = self.model.borrow();
            let (Some(from), Some(to), Some(t), Some(d)) = (
                m.position(dragged),
                m.position(target),
                m.get(target),
                m.get(dragged),
            ) else {
                return;
            };
            // Only inside a space: the rows of the others are not on the list.
            if t.space != d.space {
                return;
            }
            let folder = self
                .folders
                .borrow()
                .folder_of(t.folder, t.space, &t.url, &t.title);
            ((from, to), folder)
        };
        let changes = self
            .model
            .borrow_mut()
            .move_to(dragged, landing(positions.0, positions.1, after));
        self.apply(changes);
        let mine = {
            let m = self.model.borrow();
            m.get(dragged).and_then(|t| {
                self.folders
                    .borrow()
                    .folder_of(t.folder, t.space, &t.url, &t.title)
            })
        };
        if mine != folder {
            self.put_tab_in_folder(dragged, Some(folder.unwrap_or(NO_FOLDER)));
        }
        self.reorder_rows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_lands_next_to_the_tab_it_is_on() {
        // Tabs 0 1 2 3: drag 0 below 2 -> it ends at index 2; above 2 -> index 1.
        assert_eq!(landing(0, 2, true), 2);
        assert_eq!(landing(0, 2, false), 1);
        // Drag 3 above 1 -> index 1; below 1 -> index 2.
        assert_eq!(landing(3, 1, false), 1);
        assert_eq!(landing(3, 1, true), 2);
    }

    #[test]
    fn the_lower_half_of_a_row_is_after() {
        assert!(!after_half(5.0, 40));
        assert!(after_half(30.0, 40));
    }
}
