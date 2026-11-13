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

impl Browser {
    /// Reads the preference now and follows it while the window lives.
    pub(super) fn watch_system_theme(self: &Rc<Self>) {
        let this = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(conn) = gio::bus_get_future(gio::BusType::Session).await else {
                return;
            };
            let params = (NAMESPACE, KEY).to_variant();
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
            if let (Some(b), Ok(reply)) = (this.upgrade(), reply) {
                if let Some(dark) = dark_from(&reply.child_value(0)) {
                    b.set_system_dark(dark);
                }
            }
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
