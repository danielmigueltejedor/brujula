//! What the desktop's light/dark preference is, for the "Sistema" theme.
//!
//! NDL's own "follow the system" reads GTK's `gtk-interface-color-scheme`, but forcing a theme
//! writes that same setting, so after one switch to Claro or Oscuro the system's value is gone
//! and "Sistema" keeps showing the forced one. The real preference comes from the settings portal
//! (`org.freedesktop.appearance` `color-scheme`) instead, and "Sistema" is applied as a forced
//! light or dark that follows it.

use super::*;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// `color-scheme` is 1 for dark, 2 for light and 0 for no preference.
fn dark_from(value: &glib::Variant) -> Option<bool> {
    let mut v = value.clone();
    // The portal wraps the value in one or two variants.
    while v.type_().is_variant() {
        v = v.child_value(0);
    }
    v.get::<u32>().map(|n| n == 1)
}

thread_local! {
    /// The desktop's own settings, kept alive so that their changes keep arriving.
    static DESKTOP: RefCell<Vec<gio::Settings>> = const { RefCell::new(Vec::new()) };
}

/// How many times, and how long apart, the portal is asked again when it did not answer: right
/// after logging in it may not be up yet.
const RETRIES: [u32; 3] = [0, 3, 10];

/// `org.gnome.desktop.interface` `color-scheme`, which is what the desktop's settings write and
/// what the portal reports. `None` where the schema is not installed.
fn desktop_settings() -> Option<gio::Settings> {
    let schema =
        gio::SettingsSchemaSource::default()?.lookup("org.gnome.desktop.interface", true)?;
    schema
        .has_key("color-scheme")
        .then(|| gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
}

fn dark_from_desktop(settings: &gio::Settings) -> bool {
    settings.string("color-scheme") == "prefer-dark"
}

impl Browser {
    /// Reads the preference now and follows it while the window lives.
    pub(super) fn watch_system_theme(self: &Rc<Self>) {
        // First the desktop's own setting, which needs no bus: it is right at once where the
        // portal is slow or missing. The portal, when it answers, has the last word.
        let from_desktop = desktop_settings();
        if let Some(settings) = &from_desktop {
            self.set_system_dark(dark_from_desktop(settings));
            let this = Rc::downgrade(self);
            settings.connect_changed(Some("color-scheme"), move |s, _| {
                if let Some(b) = this.upgrade() {
                    b.set_system_dark(dark_from_desktop(s));
                }
            });
        }
        let initial = self.system_dark.get();
        let from_gtk = gtk::Settings::default()
            .map(|s| s.gtk_interface_color_scheme() == gtk::InterfaceColorScheme::Dark);
        if let Some(settings) = from_desktop {
            DESKTOP.with(|d| d.borrow_mut().push(settings));
        }
        let this = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(conn) = gio::bus_get_future(gio::BusType::Session).await else {
                eprintln!(
                    "brujula: tema del sistema: sin bus de sesión (GtkSettings: {from_gtk:?})"
                );
                return;
            };
            let params = (NAMESPACE, KEY).to_variant();
            let mut portal = None;
            for wait in RETRIES {
                if wait > 0 {
                    glib::timeout_future_seconds(wait).await;
                }
                let reply = conn
                    .call_future(
                        Some(PORTAL),
                        PATH,
                        IFACE,
                        "Read",
                        Some(&params),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                    )
                    .await;
                let Some(b) = this.upgrade() else { return };
                match reply {
                    Ok(reply) => {
                        portal = dark_from(&reply.child_value(0));
                        if let Some(dark) = portal {
                            b.set_system_dark(dark);
                            break;
                        }
                    }
                    Err(e) => eprintln!("brujula: el portal no dio el tema ({e})"),
                }
            }
            eprintln!(
                "brujula: tema del sistema: al arrancar {initial}, GtkSettings {from_gtk:?}, portal {portal:?}"
            );
            let this = this.clone();
            // The replacement hands back a guard that unsubscribes when dropped; this one lasts as
            // long as the connection, which is what is wanted here.
            #[allow(deprecated)]
            let _ = conn.signal_subscribe(
                Some(PORTAL),
                Some(IFACE),
                Some("SettingChanged"),
                Some(PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |_, _, _, _, _, args| {
                    let namespace = args.child_value(0).get::<String>();
                    let key = args.child_value(1).get::<String>();
                    if namespace.as_deref() == Some(NAMESPACE) && key.as_deref() == Some(KEY) {
                        if let (Some(b), Some(dark)) =
                            (this.upgrade(), dark_from(&args.child_value(2)))
                        {
                            b.set_system_dark(dark);
                        }
                    }
                },
            );
        });
    }

    fn set_system_dark(&self, dark: bool) {
        if self.system_dark.replace(dark) != dark {
            self.apply_appearance();
            self.refresh_newtabs();
        }
    }
}
