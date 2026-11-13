//! The button that offers what the page in front allows: add an extension when it is a Chrome
//! Web Store page, install a web app when the site says it is one (it has a web manifest).

use super::*;

use brujula_core::ext;
use brujula_core::webapps::same_site;

/// What the button does for the page in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PageAction {
    /// Add this Chrome Web Store extension.
    Extension(String),
    /// Install the site as a web app.
    WebApp,
}

pub(super) fn build_button() -> gtk::Button {
    gtk::Button::builder()
        .css_classes(["flat", "brujula-pageaction"])
        .visible(false)
        .build()
}

impl Browser {
    /// What the page in front offers, if anything.
    fn page_action(&self) -> Option<PageAction> {
        if self.private {
            return None;
        }
        let uri = self.current_view()?.uri()?.to_string();
        if let Some(id) = ext::store_page_id(&uri) {
            return Some(PageAction::Extension(id));
        }
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return None;
        }
        let has_manifest = {
            let seen = self.page_manifest.borrow();
            seen.0 == uri && seen.1
        };
        let installed = crate::webapp::registry()
            .items()
            .iter()
            .any(|a| same_site(&a.url, &uri));
        (has_manifest && !installed).then_some(PageAction::WebApp)
    }

    /// Shows, hides or words the button for the page in front.
    pub(super) fn refresh_page_action(&self) {
        let action = self.page_action();
        let (icon, tip) = match &action {
            Some(PageAction::Extension(_)) => {
                ("ndl-blocks-symbolic", "Añadir esta extensión a Brújula")
            }
            Some(PageAction::WebApp) => (
                "ndl-window-symbolic",
                "Instalar este sitio como aplicación web",
            ),
            None => ("ndl-window-symbolic", ""),
        };
        for c in &self.chromes {
            c.page_action.set_icon_name(icon);
            c.page_action.set_tooltip_text(Some(tip));
            c.page_action.set_visible(action.is_some());
        }
    }

    /// The button was pressed.
    pub(super) fn run_page_action(self: &Rc<Self>) {
        match self.page_action() {
            Some(PageAction::Extension(id)) => self.change_extension("install_store", &id),
            Some(PageAction::WebApp) => self.install_webapp_dialog(),
            None => {}
        }
    }

    /// Asks the page in front whether it declares a web manifest, and shows the button if so.
    pub(super) fn detect_manifest(self: &Rc<Self>, view: &WebView) {
        let Some(uri) = view.uri().map(|u| u.to_string()) else {
            return;
        };
        if self.private || !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return;
        }
        let this = Rc::downgrade(self);
        view.evaluate_javascript(
            "!!document.querySelector('link[rel~=\"manifest\"]')",
            None,
            None,
            gio::Cancellable::NONE,
            move |result| {
                let found = result.ok().is_some_and(|v| v.to_boolean());
                if let Some(b) = this.upgrade() {
                    *b.page_manifest.borrow_mut() = (uri, found);
                    b.refresh_page_action();
                }
            },
        );
    }
}
