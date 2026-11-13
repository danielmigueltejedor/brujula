//! Boosts: the look of a site as the user changed it, applied as user style sheets that only
//! match that site.

use super::*;

use brujula_core::boosts::{self, Boosts};
use webkit6::{UserContentInjectedFrames, UserStyleLevel, UserStyleSheet};

pub(super) fn load() -> Boosts {
    std::fs::read_to_string(paths::boosts_file())
        .map(|t| Boosts::from_json(&t))
        .unwrap_or_default()
}

impl Browser {
    fn save_boosts(&self) {
        if self.private {
            return;
        }
        let _ = paths::write_atomic(&paths::boosts_file(), &self.boosts.borrow().to_json());
    }

    /// Puts the style sheets of the boosts in the content manager, in place of the old ones.
    pub(super) fn apply_boosts(&self) {
        let mut sheets = self.boost_sheets.borrow_mut();
        for old in sheets.drain(..) {
            self.ucm.remove_style_sheet(&old);
        }
        for boost in self.boosts.borrow().items() {
            let Some(css) = boost.stylesheet() else {
                continue;
            };
            let patterns = boost.patterns();
            let allow: Vec<&str> = patterns.iter().map(String::as_str).collect();
            let sheet = UserStyleSheet::new(
                &css,
                UserContentInjectedFrames::AllFrames,
                UserStyleLevel::User,
                &allow,
                &[],
            );
            self.ucm.add_style_sheet(&sheet);
            sheets.push(sheet);
        }
    }

    /// Loads again the open pages of a site, so a change to its boost shows.
    fn reload_site(&self, host: &str) {
        let views: Vec<WebView> = self.views.borrow().values().cloned().collect();
        for v in views {
            let same = v
                .uri()
                .and_then(|u| adblock::normalize_host(&u))
                .is_some_and(|h| h == host || h.ends_with(&format!(".{host}")));
            if same {
                v.reload();
            }
        }
    }

    /// A control of the Boosts section: `add`, `remove`, `dark.<site>` or `css.<site>`.
    pub(super) fn change_boost(self: &Rc<Self>, what: &str, value: &str) {
        let site = |key: &str| what.strip_prefix(key).map(str::to_owned);
        let host = if what == "add" {
            let added = self.boosts.borrow_mut().add(value);
            if added.is_some() {
                // The page lists the new site: draw it again once this handler is done, and
                // come back to this section.
                *self.pending_section.borrow_mut() = Some("boosts".to_owned());
                let this = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(b) = this.upgrade() {
                        b.redraw_settings();
                    }
                });
            }
            None
        } else if what == "remove" {
            let host = boosts::host_of(value);
            if let Some(h) = &host {
                self.boosts.borrow_mut().remove(h);
            }
            host
        } else if let Some(h) = site("dark.") {
            let on = matches!(value, "1" | "true" | "on");
            self.boosts.borrow_mut().set_dark(&h, on).then_some(h)
        } else if let Some(h) = site("css.") {
            self.boosts.borrow_mut().set_css(&h, value).then_some(h)
        } else {
            return;
        };
        self.save_boosts();
        self.apply_boosts();
        if let Some(h) = host {
            self.reload_site(&h);
        }
    }

    /// Draws the preferences page again where it was scrolled to, for changes that alter what the
    /// page shows (a gradient losing its third colour, appearance back to the defaults).
    pub(super) fn redraw_settings(self: &Rc<Self>) {
        let view = self
            .views
            .borrow()
            .values()
            .find(|v| v.uri().as_deref() == Some(prefs::SETTINGS_URI))
            .cloned();
        let Some(view) = view else { return };
        let this = self.clone();
        let v = view.clone();
        view.evaluate_javascript(
            "document.scrollingElement.scrollTop",
            None,
            None,
            gio::Cancellable::NONE,
            move |value| {
                let top = value.ok().map(|j| j.to_int32());
                this.pending_scroll.set(top);
                this.render_settings(&v);
            },
        );
    }

    /// Starts a boost for the site of the tab and shows it in the preferences.
    pub(super) fn boost_site_of(self: &Rc<Self>, id: TabId) {
        let host = self
            .model
            .borrow()
            .get(id)
            .and_then(|t| adblock::normalize_host(&t.url));
        let Some(host) = host else { return };
        self.boosts.borrow_mut().add(&host);
        self.save_boosts();
        self.open_settings(Some("boosts"));
        self.redraw_settings();
    }
}
