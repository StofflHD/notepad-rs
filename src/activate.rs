//! Brings the window to the front on Wayland.
//!
//! winit ignores focus requests there: a compositor only raises a window when it is handed an
//! xdg-activation token. The launcher (e.g. Dolphin) gives one to every process it starts in
//! XDG_ACTIVATION_TOKEN; a second start passes it to the running instance (see ipc.rs), which
//! activates its own surface with it here.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<XdgActivationV1, ()> for State {
    fn event(_: &mut Self, _: &XdgActivationV1, _: <XdgActivationV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

/// Activates the window with `token`. Returns false when not on Wayland or the compositor
/// does not support xdg-activation.
pub fn wayland(frame: &eframe::Frame, token: &str) -> bool {
    try_activate(frame, token).is_some()
}

fn try_activate(frame: &eframe::Frame, token: &str) -> Option<()> {
    let RawDisplayHandle::Wayland(display) = frame.display_handle().ok()?.as_raw() else { return None };
    let RawWindowHandle::Wayland(window) = frame.window_handle().ok()?.as_raw() else { return None };
    // Safety: the pointers come from winit's live connection and window; the foreign display
    // is not closed when our Connection is dropped.
    let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) });
    let surface_id = unsafe { ObjectId::from_ptr(WlSurface::interface(), window.surface.as_ptr().cast()) }.ok()?;
    let surface = WlSurface::from_id(&conn, surface_id).ok()?;
    // Own event queue, so winit's queue is not touched.
    let (globals, queue) = registry_queue_init::<State>(&conn).ok()?;
    let activation: XdgActivationV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
    activation.activate(token.to_owned(), &surface);
    activation.destroy();
    conn.flush().ok()
}
