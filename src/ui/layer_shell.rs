//! The one place that talks to gtk4-layer-shell (Q0.4).
//!
//! The library is linked by `build.rs` (via `pkg-config gtk4-layer-shell-0`)
//! only with the `layer-shell` feature; without it every function here is a
//! no-op and the windows stay ordinary toplevels. Each surface is described as
//! data ([`Surface`]) so the only `unsafe` block is [`apply`].

use gtk::ApplicationWindow;
use gtk::prelude::*;

/// `GtkLayerShellKeyboardMode` from `gtk4-layer-shell.h`.
#[cfg_attr(not(feature = "layer-shell"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyboardMode {
    None = 0,
    Exclusive = 1,
}

/// `GtkLayerShellEdge` from `gtk4-layer-shell.h`.
#[cfg_attr(not(feature = "layer-shell"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Right = 1,
    Top = 2,
}

/// How one window sits on the overlay layer.
#[cfg_attr(not(feature = "layer-shell"), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
struct Surface {
    /// `None` keeps the library default namespace.
    namespace: Option<&'static std::ffi::CStr>,
    keyboard: KeyboardMode,
    /// Anchored edges; none means the compositor centers the surface.
    anchors: &'static [Edge],
    /// Margin applied to every anchored edge.
    margin: i32,
}

/// The palette: centered, and it owns the keyboard while shown.
const LAUNCHER: Surface = Surface {
    namespace: None,
    keyboard: KeyboardMode::Exclusive,
    anchors: &[],
    margin: 0,
};

/// Volume/brightness/layout pills: centered, never take the keyboard.
const OSD: Surface = Surface {
    namespace: Some(c"zeshicast-osd"),
    keyboard: KeyboardMode::None,
    anchors: &[],
    margin: 0,
};

/// Notification toasts: top-right corner, never take the keyboard.
const NOTIFICATION: Surface = Surface {
    namespace: Some(c"zeshicast-notification"),
    keyboard: KeyboardMode::None,
    anchors: &[Edge::Top, Edge::Right],
    margin: 8,
};

/// Configure the launcher window. Passed to `ensure_ui` by the GTK binary.
pub fn configure_launcher(window: &ApplicationWindow) {
    apply(window, &LAUNCHER);
}

pub(crate) fn configure_osd(window: &gtk::Window) {
    apply(window, &OSD);
}

pub(crate) fn configure_notification(window: &gtk::Window) {
    apply(window, &NOTIFICATION);
}

#[cfg(feature = "layer-shell")]
mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    /// `GTK_LAYER_SHELL_LAYER_OVERLAY`.
    pub(super) const LAYER_OVERLAY: c_int = 3;

    unsafe extern "C" {
        pub(super) fn gtk_layer_is_supported() -> c_int;
        pub(super) fn gtk_layer_init_for_window(window: *mut c_void);
        pub(super) fn gtk_layer_set_layer(window: *mut c_void, layer: c_int);
        pub(super) fn gtk_layer_set_keyboard_mode(window: *mut c_void, mode: c_int);
        pub(super) fn gtk_layer_set_namespace(window: *mut c_void, name_space: *const c_char);
        pub(super) fn gtk_layer_set_anchor(window: *mut c_void, edge: c_int, anchor: c_int);
        pub(super) fn gtk_layer_set_margin(window: *mut c_void, edge: c_int, margin: c_int);
    }
}

#[cfg(feature = "layer-shell")]
fn apply(window: &impl IsA<gtk::Window>, surface: &Surface) {
    let ptr = window
        .upcast_ref::<gtk::Window>()
        .as_ptr()
        .cast::<std::ffi::c_void>();
    // SAFETY: `ptr` is a live GtkWindow borrowed for the duration of the calls;
    // gtk4-layer-shell only requires that it is not yet mapped, which holds for
    // every caller (they configure right after building the window). The
    // namespace is a `'static` NUL-terminated string the library copies.
    unsafe {
        if ffi::gtk_layer_is_supported() == 0 {
            return;
        }
        ffi::gtk_layer_init_for_window(ptr);
        ffi::gtk_layer_set_layer(ptr, ffi::LAYER_OVERLAY);
        ffi::gtk_layer_set_keyboard_mode(ptr, surface.keyboard as i32);
        if let Some(namespace) = surface.namespace {
            ffi::gtk_layer_set_namespace(ptr, namespace.as_ptr());
        }
        for &edge in surface.anchors {
            ffi::gtk_layer_set_anchor(ptr, edge as i32, 1);
            ffi::gtk_layer_set_margin(ptr, edge as i32, surface.margin);
        }
    }
}

#[cfg(not(feature = "layer-shell"))]
fn apply(_window: &impl IsA<gtk::Window>, _surface: &Surface) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_launcher_takes_the_keyboard() {
        // An OSD or a toast that grabbed the keyboard would steal typing from
        // the focused application every time the volume changes.
        assert_eq!(LAUNCHER.keyboard, KeyboardMode::Exclusive);
        assert_eq!(OSD.keyboard, KeyboardMode::None);
        assert_eq!(NOTIFICATION.keyboard, KeyboardMode::None);
    }

    #[test]
    fn surfaces_keep_their_placement() {
        assert!(LAUNCHER.anchors.is_empty(), "the palette is centered");
        assert!(OSD.anchors.is_empty(), "the OSD is centered");
        assert_eq!(NOTIFICATION.anchors, &[Edge::Top, Edge::Right]);
        assert_eq!(NOTIFICATION.margin, 8);
    }

    #[test]
    fn enum_values_match_the_c_header() {
        assert_eq!(KeyboardMode::None as i32, 0);
        assert_eq!(KeyboardMode::Exclusive as i32, 1);
        assert_eq!(Edge::Right as i32, 1);
        assert_eq!(Edge::Top as i32, 2);
    }
}
