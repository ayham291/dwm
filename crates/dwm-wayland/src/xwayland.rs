//! XWayland integration (feature `xwayland`).
//!
//! Spawns `Xwayland`, inserts it into the calloop event loop, and on Ready
//! starts [`X11Wm`]. X11 windows are managed as `dwm-core` clients and
//! configured from arrange geometries.

use std::process::Stdio;

use calloop::LoopHandle;
use dwm_core::{Client as DwmClient, ClientId, Rect};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::{
    xwm::{Reorder, ResizeEdge as X11ResizeEdge, X11Window, XwmId},
    X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler,
};
use smithay::delegate_xwayland_shell;
use tracing::{info, warn};
use wayland_server::Resource;

use crate::state::DwmState;

impl DwmState {
    pub(crate) fn set_xwm(&mut self, wm: X11Wm) {
        self.xwm = Some(wm);
    }

    pub(crate) fn clear_xwm(&mut self) {
        self.xwm = None;
    }

    pub(crate) fn set_xdisplay(&mut self, display_number: u32) {
        self.xdisplay = Some(display_number);
        std::env::set_var("DISPLAY", format!(":{display_number}"));
        info!("XWayland ready — DISPLAY=:{display_number}");
    }

    fn client_id_for_x11(&self, window: &X11Surface) -> Option<ClientId> {
        self.x11_surfaces
            .iter()
            .find(|(_, surf)| *surf == window)
            .map(|(id, _)| *id)
    }

    fn manage_x11_window(&mut self, window: X11Surface, floating: bool) -> ClientId {
        let title = window.title();
        let class = window.class();
        let mut client = DwmClient::new(ClientId(0), if title.is_empty() { class.clone() } else { title });
        client.app_id = class;
        if floating {
            let geo = window.geometry();
            client.floating = true;
            client.float_geom = Rect::new(geo.loc.x, geo.loc.y, geo.size.w, geo.size.h);
            client.geom = client.float_geom;
        }
        let id = self.monitor.manage(client);
        if let Some(surf) = window.wl_surface() {
            self.surface_clients
                .insert(surf.id().protocol_id(), id);
        }
        self.x11_surfaces.insert(id, window);
        id
    }

    fn unmanage_x11_window(&mut self, window: &X11Surface) {
        if let Some(id) = self.client_id_for_x11(window) {
            self.x11_surfaces.remove(&id);
            if let Some(surf) = window.wl_surface() {
                self.surface_clients.remove(&surf.id().protocol_id());
            }
            self.monitor.unmanage(id);
            let (geoms, symbol) = self.monitor.arrange();
            self.layout_symbol = symbol;
            self.push_configures(&geoms);
            if let Some(focus) = self.monitor.sel {
                self.focus_client(focus);
            }
            info!("X11 window destroyed client={id:?}");
        }
    }
}

/// Spawn XWayland and insert into `handle`. On Ready, `on_ready` receives the WM
/// and display number so the loop state can store them.
pub fn spawn_xwayland<D, F>(
    handle: LoopHandle<'static, D>,
    dh: &DisplayHandle,
    mut on_ready: F,
) -> Result<(), String>
where
    D: XwmHandler + XWaylandShellHandler + 'static,
    F: FnMut(&mut D, X11Wm, u32) + 'static,
{
    let (xwayland, client) = XWayland::spawn(
        dh,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| (),
    )
    .map_err(|e| format!("XWayland::spawn: {e}"))?;

    let handle_clone = handle.clone();
    handle
        .insert_source(xwayland, move |event, _, data| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => match X11Wm::start_wm(handle_clone.clone(), x11_socket, client.clone()) {
                Ok(wm) => on_ready(data, wm, display_number),
                Err(err) => warn!("X11Wm::start_wm failed: {err}"),
            },
            XWaylandEvent::Error => {
                warn!("XWayland crashed on startup");
            }
        })
        .map_err(|e| format!("insert XWayland source: {e}"))?;

    info!("XWayland: spawn requested (waiting for Ready)");
    Ok(())
}

/// Convenience for loop states whose XWayland fields live on an inner [`DwmState`].
pub fn on_ready_store_on_dwm(dwm: &mut DwmState, wm: X11Wm, display_number: u32) {
    dwm.set_xwm(wm);
    dwm.set_xdisplay(display_number);
}

impl XWaylandShellHandler for DwmState {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(
        &mut self,
        _xwm: XwmId,
        wl_surface: wayland_server::protocol::wl_surface::WlSurface,
        surface: X11Surface,
    ) {
        if let Some(id) = self.client_id_for_x11(&surface) {
            self.surface_clients
                .insert(wl_surface.id().protocol_id(), id);
            info!("X11 surface associated client={id:?}");
        }
    }
}

impl XwmHandler for DwmState {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("X11Wm missing")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Err(err) = window.set_mapped(true) {
            warn!("X11 set_mapped: {err:?}");
            return;
        }
        let id = self.manage_x11_window(window.clone(), false);
        let (geoms, symbol) = self.monitor.arrange();
        self.layout_symbol = symbol;
        self.push_configures(&geoms);
        self.focus_client(id);
        info!("X11 map_window_request client={id:?}");
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let id = self.manage_x11_window(window, true);
        info!("X11 override-redirect mapped client={id:?}");
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        if !window.is_override_redirect() {
            let _ = window.set_mapped(false);
        }
        self.unmanage_x11_window(&window);
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.unmanage_x11_window(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let mut geo = window.geometry();
        if let Some(id) = self.client_id_for_x11(&window) {
            if let Some(c) = self.monitor.client(id) {
                if c.floating {
                    if let Some(x) = x {
                        geo.loc.x = x;
                    }
                    if let Some(y) = y {
                        geo.loc.y = y;
                    }
                    if let Some(w) = w {
                        geo.size.w = w as i32;
                    }
                    if let Some(h) = h {
                        geo.size.h = h as i32;
                    }
                    if let Some(c) = self.monitor.client_mut(id) {
                        c.float_geom = Rect::new(geo.loc.x, geo.loc.y, geo.size.w, geo.size.h);
                        c.geom = c.float_geom;
                    }
                } else {
                    geo = Rectangle::new(
                        (c.geom.x, c.geom.y).into(),
                        (c.geom.w, c.geom.h).into(),
                    );
                }
            }
        } else {
            if let Some(w) = w {
                geo.size.w = w as i32;
            }
            if let Some(h) = h {
                geo.size.h = h as i32;
            }
        }
        if let Err(err) = window.configure(geo) {
            warn!("X11 configure_request: {err:?}");
        }
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<X11Window>,
    ) {
        if !window.is_override_redirect() {
            return;
        }
        if let Some(id) = self.client_id_for_x11(&window) {
            if let Some(c) = self.monitor.client_mut(id) {
                c.floating = true;
                c.float_geom = Rect::new(
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                );
                c.geom = c.float_geom;
            }
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.client_id_for_x11(&window) {
            if let Some(c) = self.monitor.client_mut(id) {
                c.fullscreen = true;
            }
            let _ = window.set_fullscreen(true);
            self.rearrange();
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        if let Some(id) = self.client_id_for_x11(&window) {
            if let Some(c) = self.monitor.client_mut(id) {
                c.fullscreen = false;
            }
            let _ = window.set_fullscreen(false);
            self.rearrange();
        }
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _resize_edge: X11ResizeEdge,
    ) {
        // Mod+mouse resize already covers floating X11 clients via dwm grabs.
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {
        // Mod+mouse move already covers floating X11 clients via dwm grabs.
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        warn!("XWayland disconnected");
        self.clear_xwm();
        self.xdisplay = None;
    }
}

delegate_xwayland_shell!(DwmState);

/// Forward [`XwmHandler`] + [`XWaylandShellHandler`] to an inner `dwm: DwmState` field.
#[macro_export]
macro_rules! forward_xwayland_to_dwm {
    ($ty:ty) => {
        impl smithay::wayland::xwayland_shell::XWaylandShellHandler for $ty {
            fn xwayland_shell_state(
                &mut self,
            ) -> &mut smithay::wayland::xwayland_shell::XWaylandShellState {
                smithay::wayland::xwayland_shell::XWaylandShellHandler::xwayland_shell_state(
                    &mut self.dwm,
                )
            }

            fn surface_associated(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                wl_surface: wayland_server::protocol::wl_surface::WlSurface,
                surface: smithay::xwayland::X11Surface,
            ) {
                smithay::wayland::xwayland_shell::XWaylandShellHandler::surface_associated(
                    &mut self.dwm,
                    xwm,
                    wl_surface,
                    surface,
                )
            }
        }

        impl smithay::xwayland::XwmHandler for $ty {
            fn xwm_state(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
            ) -> &mut smithay::xwayland::X11Wm {
                smithay::xwayland::XwmHandler::xwm_state(&mut self.dwm, xwm)
            }

            fn new_window(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::new_window(&mut self.dwm, xwm, window)
            }

            fn new_override_redirect_window(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::new_override_redirect_window(&mut self.dwm, xwm, window)
            }

            fn map_window_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::map_window_request(&mut self.dwm, xwm, window)
            }

            fn mapped_override_redirect_window(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::mapped_override_redirect_window(
                    &mut self.dwm, xwm, window,
                )
            }

            fn unmapped_window(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::unmapped_window(&mut self.dwm, xwm, window)
            }

            fn destroyed_window(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::destroyed_window(&mut self.dwm, xwm, window)
            }

            fn configure_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
                x: Option<i32>,
                y: Option<i32>,
                w: Option<u32>,
                h: Option<u32>,
                reorder: Option<smithay::xwayland::xwm::Reorder>,
            ) {
                smithay::xwayland::XwmHandler::configure_request(
                    &mut self.dwm, xwm, window, x, y, w, h, reorder,
                )
            }

            fn configure_notify(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
                geometry: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
                above: Option<smithay::xwayland::xwm::X11Window>,
            ) {
                smithay::xwayland::XwmHandler::configure_notify(
                    &mut self.dwm, xwm, window, geometry, above,
                )
            }

            fn fullscreen_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::fullscreen_request(&mut self.dwm, xwm, window)
            }

            fn unfullscreen_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
            ) {
                smithay::xwayland::XwmHandler::unfullscreen_request(&mut self.dwm, xwm, window)
            }

            fn resize_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
                button: u32,
                resize_edge: smithay::xwayland::xwm::ResizeEdge,
            ) {
                smithay::xwayland::XwmHandler::resize_request(
                    &mut self.dwm, xwm, window, button, resize_edge,
                )
            }

            fn move_request(
                &mut self,
                xwm: smithay::xwayland::xwm::XwmId,
                window: smithay::xwayland::X11Surface,
                button: u32,
            ) {
                smithay::xwayland::XwmHandler::move_request(&mut self.dwm, xwm, window, button)
            }

            fn disconnected(&mut self, xwm: smithay::xwayland::xwm::XwmId) {
                smithay::xwayland::XwmHandler::disconnected(&mut self.dwm, xwm)
            }
        }
    };
}
