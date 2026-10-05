//! D-Bus notification server: zeshicast owns `org.freedesktop.Notifications`
//! and records incoming notifications into the in-process store
//! (`services::notifications`). No external daemon (swaync/dunst) is involved.
//! Popups are rendered as an on-screen toast in the top-right corner.

use std::cell::RefCell;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";

const INTROSPECTION: &str = r#"
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="Notify">
      <arg type="s" name="app_name" direction="in"/>
      <arg type="u" name="replaces_id" direction="in"/>
      <arg type="s" name="app_icon" direction="in"/>
      <arg type="s" name="summary" direction="in"/>
      <arg type="s" name="body" direction="in"/>
      <arg type="as" name="actions" direction="in"/>
      <arg type="a{sv}" name="hints" direction="in"/>
      <arg type="i" name="expire_timeout" direction="in"/>
      <arg type="u" name="id" direction="out"/>
    </method>
    <method name="CloseNotification">
      <arg type="u" name="id" direction="in"/>
    </method>
    <method name="GetCapabilities">
      <arg type="as" name="capabilities" direction="out"/>
    </method>
    <method name="GetServerInformation">
      <arg type="s" name="name" direction="out"/>
      <arg type="s" name="vendor" direction="out"/>
      <arg type="s" name="version" direction="out"/>
      <arg type="s" name="spec_version" direction="out"/>
    </method>
    <signal name="NotificationClosed">
      <arg type="u" name="id"/>
      <arg type="u" name="reason"/>
    </signal>
    <signal name="ActionInvoked">
      <arg type="u" name="id"/>
      <arg type="s" name="action_key"/>
    </signal>
  </interface>
</node>
"#;

thread_local! {
    // Keep the registration handles alive for the life of the process.
    static OWNER: RefCell<Option<gio::OwnerId>> = const { RefCell::new(None) };
    static REGISTRATION: RefCell<Option<gio::RegistrationId>> = const { RefCell::new(None) };
}

/// Acquire `org.freedesktop.Notifications` and start recording notifications.
pub fn install_notification_server() {
    let owner = gio::bus_own_name(
        gio::BusType::Session,
        NAME,
        gio::BusNameOwnerFlags::REPLACE,
        |connection, _name| register_object(&connection),
        |_connection, _name| crate::services::notifications::mark_server_active(),
        |_connection, _name| {
            eprintln!("notifications: name lost — another daemon owns {NAME}");
            crate::services::notifications::mark_server_inactive();
        },
    );
    OWNER.with(|cell| *cell.borrow_mut() = Some(owner));
}

fn register_object(connection: &gio::DBusConnection) {
    let node = match gio::DBusNodeInfo::for_xml(INTROSPECTION) {
        Ok(node) => node,
        Err(error) => {
            eprintln!("notifications: introspection error: {error}");
            return;
        }
    };
    let Some(interface) = node.lookup_interface(IFACE) else {
        return;
    };

    match connection
        .register_object(PATH, &interface)
        .method_call(handle_method_call)
        .build()
    {
        Ok(id) => REGISTRATION.with(|cell| *cell.borrow_mut() = Some(id)),
        Err(error) => eprintln!("notifications: register_object error: {error}"),
    }
}

fn handle_method_call(
    connection: gio::DBusConnection,
    _sender: Option<&str>,
    _path: &str,
    _interface: Option<&str>,
    method: &str,
    params: glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    match method {
        // Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire) -> id
        "Notify" => {
            // GDBus normally validates the signature, but a short tuple must not
            // panic the daemon (n_children guard).
            let child_string = |index: usize| -> String {
                if params.n_children() > index {
                    params
                        .child_value(index)
                        .get::<String>()
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            };
            let child_u32 = |index: usize| -> u32 {
                if params.n_children() > index {
                    params.child_value(index).get::<u32>().unwrap_or(0)
                } else {
                    0
                }
            };
            let child_i32 = |index: usize| -> i32 {
                if params.n_children() > index {
                    params.child_value(index).get::<i32>().unwrap_or(-1)
                } else {
                    -1
                }
            };
            let app_name = child_string(0);
            let replaces_id = child_u32(1);
            let app_icon = child_string(2);
            let summary = child_string(3);
            let body = child_string(4);
            let expire_timeout = child_i32(7);
            let id = crate::services::notifications::push_notification_from(
                &app_name,
                &summary,
                &body,
                replaces_id,
                _sender,
            );

            if !crate::services::notifications::is_dnd_enabled() {
                super::osd::show_notification_osd(
                    None,
                    &app_name,
                    &summary,
                    &body,
                    &app_icon,
                    expire_timeout,
                );
            }

            invocation.return_value(Some(&(id,).to_variant()));
        }
        "CloseNotification" => {
            let id = if params.n_children() > 0 {
                params.child_value(0).get::<u32>().unwrap_or(0)
            } else {
                0
            };
            crate::services::notifications::close_notification_from(id, _sender);
            super::osd::dismiss_notification_osd();
            // reason 3 = closed by a call to CloseNotification.
            let _ = connection.emit_signal(
                None,
                PATH,
                IFACE,
                "NotificationClosed",
                Some(&(id, 3u32).to_variant()),
            );
            invocation.return_value(None);
        }
        "GetCapabilities" => {
            let caps = vec![
                "body".to_string(),
                "persistence".to_string(),
                "icon-static".to_string(),
            ];
            invocation.return_value(Some(&(caps,).to_variant()));
        }
        "GetServerInformation" => {
            invocation.return_value(Some(
                &("zeshicast", "zeshi", env!("CARGO_PKG_VERSION"), "1.2").to_variant(),
            ));
        }
        other => invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod",
            &format!("Unknown method {other}"),
        ),
    }
}
