//! Smithay compositor state wired to `dwm-core`.

use std::collections::HashMap;
use std::os::unix::io::OwnedFd;
use std::process::Command as ProcCommand;
use std::sync::Arc;
use std::time::Instant;

use dwm_core::{Client as DwmClient, ClientId, Command, Config, Monitor, Rect};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};
use smithay::backend::renderer::utils::{draw_render_elements, on_commit_buffer_handler};
use smithay::backend::renderer::{Color32F, Frame, Renderer};
use smithay::backend::winit::{self as winit_backend, WinitEvent};
use smithay::desktop::{find_popup_root_surface, PopupKind, PopupManager};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::reexports::wayland_server::protocol::{wl_output, wl_seat};
#[cfg(feature = "session")]
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{Logical, Physical, Point, Rectangle, Serial, Size, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    with_surface_tree_downward, CompositorClientState, CompositorHandler, CompositorState,
    SurfaceAttributes, TraversalAction,
};
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::xdg::decoration::{XdgDecorationHandler, XdgDecorationState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_kde_decoration, delegate_output,
    delegate_primary_selection, delegate_seat, delegate_shm, delegate_xdg_decoration,
    delegate_xdg_shell,
};
use tracing::{info, warn};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as XdgDecoMode;
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::Mode as KdeDecoMode;
use wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDecoDefaultMode;
use wayland_server::backend::{ClientData, ClientId as WlClientId, DisconnectReason};
use wayland_server::protocol::wl_buffer;
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{Client, DisplayHandle, ListeningSocket, Resource, WEnum};
use winit::platform::pump_events::PumpStatus;

use crate::bar::StatusBar;
use crate::bindings;
use crate::color;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wayland display: {0}")]
    Display(#[from] wayland_server::backend::InitError),
    #[error("winit backend: {0}")]
    Winit(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub struct DwmState {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    _xdg_decoration_state: XdgDecorationState,
    kde_decoration_state: KdeDecorationState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    primary_selection_state: PrimarySelectionState,
    _output_manager_state: OutputManagerState,
    pub(crate) seat: Seat<Self>,
    output: Output,

    pub monitor: Monitor,
    pub config: Config,
    pub bindings: Vec<bindings::Binding>,

    /// Map dwm ClientId → xdg toplevel.
    pub toplevels: HashMap<ClientId, ToplevelSurface>,
    /// Map wl_surface id → ClientId for commit lookups.
    surface_clients: HashMap<u32, ClientId>,

    should_quit: bool,
    pub layout_symbol: String,
    pub pointer_location: Point<f64, Logical>,
    pub popups: PopupManager,
    status_bar: StatusBar,
    status_path: String,
    last_status_read: Instant,
    /// Scratchpad index waiting to claim the next new toplevel.
    pending_scratch: Option<usize>,
}

impl DwmState {
    fn new(dh: &DisplayHandle, screen: Rect) -> Self {
        let output = Output::new(
            "dwm-0".into(),
            PhysicalProperties {
                size: (screen.w, screen.h).into(),
                subpixel: Subpixel::Unknown,
                make: "dwm-wayland".into(),
                model: "winit".into(),
            },
        );
        let _global = output.create_global::<Self>(dh);
        let mode = OutputMode {
            size: (screen.w, screen.h).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.add_mode(mode);
        Self::new_with_output(dh, "dwm", screen, output)
    }

    /// Build compositor state against an already-configured [`Output`] (session / nested).
    pub(crate) fn new_with_output(
        dh: &DisplayHandle,
        seat_name: impl Into<String>,
        screen: Rect,
        output: Output,
    ) -> Self {
        let config = Config::default();
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(dh, seat_name);
        seat.add_pointer();

        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);

        Self {
            compositor_state: CompositorState::new::<Self>(dh),
            xdg_shell_state: XdgShellState::new::<Self>(dh),
            _xdg_decoration_state: XdgDecorationState::new::<Self>(dh),
            kde_decoration_state: KdeDecorationState::new::<Self>(dh, KdeDecoDefaultMode::Server),
            shm_state: ShmState::new::<Self>(dh, vec![]),
            seat_state,
            data_device_state: DataDeviceState::new::<Self>(dh),
            primary_selection_state: PrimarySelectionState::new::<Self>(dh),
            _output_manager_state: output_manager_state,
            seat,
            output,
            status_path: config.status_file.clone(),
            monitor: Monitor::from_config(&config, screen),
            config,
            bindings: bindings::default_bindings(),
            toplevels: HashMap::new(),
            surface_clients: HashMap::new(),
            should_quit: false,
            layout_symbol: "[]=".into(),
            pointer_location: Point::from((screen.w as f64 / 2.0, screen.h as f64 / 2.0)),
            popups: PopupManager::default(),
            status_bar: StatusBar::new(),
            last_status_read: Instant::now(),
            pending_scratch: None,
        }
    }

    #[cfg(feature = "session")]
    pub(crate) fn should_quit(&self) -> bool {
        self.should_quit
    }

    #[cfg(feature = "session")]
    pub(crate) fn update_shm_formats(
        &mut self,
        formats: impl IntoIterator<Item = wl_shm::Format>,
    ) {
        self.shm_state.update_formats(formats);
    }

    pub(crate) fn refresh_status(&mut self) {
        self.refresh_status_file();
    }

    /// Clear the framebuffer and draw borders, bar, clients, and popups (shared by winit/udev).
    ///
    /// Returns popup surfaces that need `wl_surface.frame` callbacks after the draw.
    pub(crate) fn render_gles_frame(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut GlesTarget<'_>,
        size: Size<i32, Physical>,
        transform: Transform,
    ) -> Result<Vec<WlSurface>, String> {
        let damage = Rectangle::from_size(size);
        let tagset = self.monitor.tagset();
        let bw = self.config.border_px as i32;
        let sel_border = color::parse_hex(&self.config.sel_border);
        let norm_border = color::parse_hex(&self.config.norm_border);
        let focused = self.monitor.sel;
        let bar_model = self.monitor.bar_model();
        let bar_element = self.status_bar.element(
            renderer,
            &bar_model,
            self.monitor.screen.h,
            &self.config.norm_bg,
            &self.config.norm_fg,
            &self.config.sel_bg,
            &self.config.sel_fg,
        );

        let mut visible: Vec<_> = self
            .monitor
            .visible_clients()
            .into_iter()
            .filter(|c| c.geom.w > 0 && c.geom.h > 0)
            .collect();
        visible.sort_by_key(|c| c.floating || c.fullscreen);

        let mut draws: Vec<(
            Rectangle<i32, Logical>,
            Color32F,
            Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
            Rectangle<i32, Logical>,
        )> = Vec::new();
        let mut popup_draws: Vec<Vec<WaylandSurfaceRenderElement<GlesRenderer>>> = Vec::new();
        let mut popup_frame_surfaces: Vec<WlSurface> = Vec::new();
        for client in visible {
            let Some(toplevel) = self.toplevels.get(&client.id) else {
                continue;
            };
            if !client.is_visible(tagset) {
                continue;
            }
            let border_color = if focused == Some(client.id) {
                sel_border
            } else {
                norm_border
            };
            let outer = Rectangle::new(
                (client.geom.x - bw, client.geom.y - bw).into(),
                (client.geom.w + 2 * bw, client.geom.h + 2 * bw).into(),
            );
            let clip = Rectangle::new(
                (client.geom.x, client.geom.y).into(),
                (client.geom.w, client.geom.h).into(),
            );
            let loc: Point<i32, Logical> = (client.geom.x, client.geom.y).into();
            let surf = render_elements_from_surface_tree(
                renderer,
                toplevel.wl_surface(),
                loc.to_physical_precise_round(1.0),
                1.0,
                1.0,
                Kind::Unspecified,
            );
            draws.push((outer, border_color, surf, clip));

            for (popup, offset) in PopupManager::popups_for_surface(toplevel.wl_surface()) {
                let geo = popup.geometry();
                let popup_loc: Point<i32, Logical> = (
                    client.geom.x + offset.x + geo.loc.x,
                    client.geom.y + offset.y + geo.loc.y,
                )
                    .into();
                let popup_surf = render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    popup_loc.to_physical_precise_round(1.0),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                );
                popup_draws.push(popup_surf);
                popup_frame_surfaces.push(popup.wl_surface().clone());
            }
        }

        let mut frame = renderer
            .render(framebuffer, size, transform)
            .map_err(|e| format!("render: {e:?}"))?;
        frame
            .clear(color::parse_hex(&self.config.norm_bg), &[damage])
            .map_err(|e| format!("clear: {e:?}"))?;
        for (rect, border_color, _, _) in &draws {
            let phys = rect.to_physical_precise_round(1.0);
            frame
                .clear(*border_color, &[phys])
                .map_err(|e| format!("border: {e:?}"))?;
        }
        if let Some(bar) = &bar_element {
            draw_render_elements(&mut frame, 1.0, &[bar], &[damage])
                .map_err(|e| format!("bar: {e:?}"))?;
        }
        for (_, _, surf, clip) in &draws {
            let clip_phys = clip.to_physical_precise_round(1.0);
            draw_render_elements(&mut frame, 1.0, surf, &[clip_phys])
                .map_err(|e| format!("draw: {e:?}"))?;
        }
        for surf in &popup_draws {
            draw_render_elements(&mut frame, 1.0, surf, &[damage])
                .map_err(|e| format!("popup: {e:?}"))?;
        }
        let _ = frame.finish().map_err(|e| format!("finish: {e:?}"))?;

        Ok(popup_frame_surfaces)
    }

    pub(crate) fn send_frames(&mut self, time: u32, popup_surfaces: &[WlSurface]) {
        for surface in self.xdg_shell_state.toplevel_surfaces() {
            send_frames_surface_tree(surface.wl_surface(), time);
        }
        for surface in popup_surfaces {
            send_frames_surface_tree(surface, time);
        }
        self.popups.cleanup();
    }

    /// Re-run the layout engine and push configures to all mapped clients.
    pub fn rearrange(&mut self) {
        let (geoms, symbol) = self.monitor.arrange();
        self.layout_symbol = symbol;
        self.push_configures(&geoms);
    }

    fn resize_output(&mut self, w: i32, h: i32) {
        let mode = OutputMode {
            size: (w, h).into(),
            refresh: 60_000,
        };
        self.output.delete_mode(
            self.output
                .current_mode()
                .unwrap_or(mode),
        );
        self.output.add_mode(mode);
        self.output.set_preferred(mode);
        self.output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
    }

    pub(crate) fn apply_command(&mut self, cmd: Command) {
        let pending = match cmd {
            Command::ToggleScratch { index } => Some(index),
            _ => None,
        };
        let frame = self.monitor.apply(cmd);
        self.layout_symbol = frame.layout_symbol;
        if frame.quit {
            self.should_quit = true;
        }
        if let Some(id) = frame.kill {
            if let Some(toplevel) = self.toplevels.get(&id) {
                toplevel.send_close();
            }
        }
        if let Some(argv) = frame.spawn {
            if let Some(index) = pending {
                self.pending_scratch = Some(index);
                info!("scratchpad {index}: spawning, pending claim");
            }
            spawn_argv(&argv);
        } else if let Some(index) = pending {
            let shown = self
                .monitor
                .tagset()
                .contains(dwm_core::TagMask::scratch(index as u32));
            info!(
                "scratchpad {index}: toggled {}",
                if shown { "shown" } else { "hidden" }
            );
        }
        self.push_configures(&frame.geometries);
        if let Some(focus) = frame.focused {
            self.focus_client(focus);
        }
    }

    fn refresh_status_file(&mut self) {
        if self.last_status_read.elapsed() < std::time::Duration::from_millis(500) {
            return;
        }
        self.last_status_read = Instant::now();
        if let Ok(text) = std::fs::read_to_string(&self.status_path) {
            let trimmed = text.lines().next().unwrap_or("").trim();
            if !trimmed.is_empty() {
                self.monitor.set_status(trimmed.to_string());
            }
        }
    }

    fn sync_client_meta(&mut self, id: ClientId) {
        let Some(toplevel) = self.toplevels.get(&id) else {
            return;
        };
        // Title / app_id live on the xdg surface attributes once set by the client.
        let (title, app_id) = smithay::wayland::compositor::with_states(
            toplevel.wl_surface(),
            |states| {
                use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
                let guard = states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().ok());
                guard
                    .map(|g| (g.title.clone().unwrap_or_default(), g.app_id.clone().unwrap_or_default()))
                    .unwrap_or_default()
            },
        );
        if self.monitor.apply_scratchpad_rule(id, &title, &app_id) {
            let (geoms, symbol) = self.monitor.arrange();
            self.layout_symbol = symbol;
            self.push_configures(&geoms);
            return;
        }
        if let Some(c) = self.monitor.client_mut(id) {
            if !title.is_empty() {
                c.name = title;
            }
            if !app_id.is_empty() {
                c.app_id = app_id;
            }
        }
    }

    pub(crate) fn push_configures(&mut self, geometries: &[(ClientId, Rect)]) {
        for (id, rect) in geometries {
            let Some(toplevel) = self.toplevels.get(id) else {
                continue;
            };
            if rect.w <= 0 || rect.h <= 0 {
                toplevel.with_pending_state(|state| {
                    state.states.unset(xdg_toplevel::State::Activated);
                    state.states.unset(xdg_toplevel::State::Fullscreen);
                });
                toplevel.send_configure();
                continue;
            }
            let size = Size::<i32, Logical>::from((rect.w, rect.h));
            let activated = self.monitor.sel == Some(*id);
            let fullscreen = self
                .monitor
                .client(*id)
                .map(|c| c.fullscreen)
                .unwrap_or(false);
            toplevel.with_pending_state(|state| {
                state.size = Some(size);
                if activated {
                    state.states.set(xdg_toplevel::State::Activated);
                } else {
                    state.states.unset(xdg_toplevel::State::Activated);
                }
                if fullscreen {
                    state.states.set(xdg_toplevel::State::Fullscreen);
                } else {
                    state.states.unset(xdg_toplevel::State::Fullscreen);
                }
            });
            toplevel.send_configure();
        }
    }

    pub(crate) fn focus_client(&mut self, id: ClientId) {
        if let Some(toplevel) = self.toplevels.get(&id) {
            let surface = toplevel.wl_surface().clone();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surface), Serial::from(0));
            }
        }
    }

    fn unconstrain_popup(&self, surface: &PopupSurface, positioner: PositionerState) {
        // Constraint rectangle in parent-local coordinates covering the output.
        let mut target = Rectangle::new(
            (0, 0).into(),
            (self.monitor.screen.w, self.monitor.screen.h).into(),
        );
        if let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(surface.clone())) {
            if let Some(id) = self.surface_clients.get(&root.id().protocol_id()).copied() {
                if let Some(c) = self.monitor.client(id) {
                    target = Rectangle::new(
                        (-c.geom.x, -c.geom.y).into(),
                        (self.monitor.screen.w, self.monitor.screen.h).into(),
                    );
                }
            }
        }
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_unconstrained_geometry(target);
        });
    }
}

pub(crate) fn spawn_program(bin: &str, args: &[&str]) {
    match ProcCommand::new(bin).args(args).spawn() {
        Ok(_) => info!("spawned {bin}"),
        Err(e) => warn!("failed to spawn {bin}: {e}"),
    }
}

fn spawn_argv(argv: &[String]) {
    let Some((bin, rest)) = argv.split_first() else {
        return;
    };
    match ProcCommand::new(bin).args(rest).spawn() {
        Ok(_) => info!("spawned {argv:?}"),
        Err(e) => warn!("failed to spawn {argv:?}: {e}"),
    }
}

impl BufferHandler for DwmState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for DwmState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let client = DwmClient::new(ClientId(0), "toplevel");
        let id = self.monitor.manage(client);
        self.surface_clients
            .insert(surface.wl_surface().id().protocol_id(), id);
        self.toplevels.insert(id, surface.clone());

        // dwm draws borders itself — ask clients not to use CSD title bars.
        surface.with_pending_state(|state| {
            state.decoration_mode = Some(XdgDecoMode::ServerSide);
        });

        if let Some(index) = self.pending_scratch.take() {
            // Force scratchpad role immediately (title may arrive later).
            if let Some(sp) = self.monitor.scratchpads.get(index).cloned() {
                let tags = dwm_core::TagMask::scratch(index as u32);
                let geom = self.monitor.scratch_float_geom(sp.width, sp.height);
                if let Some(c) = self.monitor.client_mut(id) {
                    c.tags = tags;
                    c.floating = true;
                    c.name = sp.name.clone();
                    c.float_geom = geom;
                    c.geom = geom;
                }
                self.monitor.tagset_mut_union(tags);
                info!(
                    "claimed scratchpad index={index} client={id:?} geom={}x{}+{}+{}",
                    geom.w, geom.h, geom.x, geom.y
                );
            }
        }

        let (geoms, symbol) = self.monitor.arrange();
        self.layout_symbol = symbol;
        self.push_configures(&geoms);
        self.focus_client(id);
        info!("new toplevel client={id:?} layout={}", self.layout_symbol);
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        self.unconstrain_popup(&surface, positioner);
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface.clone())) {
            warn!("failed to track popup: {err:?}");
        }
        if let Err(err) = surface.send_configure() {
            warn!("popup initial configure: {err:?}");
        }
    }

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        // Full seat grab wiring needs KeyboardFocus: From<PopupKind>; stub for now.
        info!(
            "popup grab requested serial={serial:?} surface={:?}",
            surface.wl_surface().id()
        );
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        self.unconstrain_popup(&surface, positioner);
        surface.send_repositioned(token);
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<wl_output::WlOutput>,
    ) {
        let protocol_id = surface.wl_surface().id().protocol_id();
        if let Some(id) = self.surface_clients.get(&protocol_id).copied() {
            if let Some(c) = self.monitor.client_mut(id) {
                c.fullscreen = true;
            }
            self.rearrange();
            info!("fullscreen client={id:?}");
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        let protocol_id = surface.wl_surface().id().protocol_id();
        if let Some(id) = self.surface_clients.get(&protocol_id).copied() {
            if let Some(c) = self.monitor.client_mut(id) {
                c.fullscreen = false;
            }
            self.rearrange();
            info!("unfullscreen client={id:?}");
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let protocol_id = surface.wl_surface().id().protocol_id();
        if let Some(id) = self.surface_clients.remove(&protocol_id) {
            self.toplevels.remove(&id);
            self.monitor.unmanage(id);
            let (geoms, symbol) = self.monitor.arrange();
            self.layout_symbol = symbol;
            self.push_configures(&geoms);
            if let Some(focus) = self.monitor.sel {
                self.focus_client(focus);
            }
            info!("toplevel destroyed client={id:?}");
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self
            .surface_clients
            .get(&surface.wl_surface().id().protocol_id())
            .copied()
        {
            self.sync_client_meta(id);
        }
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self
            .surface_clients
            .get(&surface.wl_surface().id().protocol_id())
            .copied()
        {
            self.sync_client_meta(id);
        }
    }
}

impl SelectionHandler for DwmState {
    type SelectionUserData = ();
}

impl PrimarySelectionHandler for DwmState {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}

impl DataDeviceHandler for DwmState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for DwmState {}
impl ServerDndGrabHandler for DwmState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl CompositorHandler for DwmState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.popups.commit(surface);

        // Send initial / pending popup configure after role commit.
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) {
            if popup.has_pending_changes() {
                if let Err(err) = popup.send_configure() {
                    warn!("popup configure on commit: {err:?}");
                }
            }
        }

        if let Some(id) = self
            .surface_clients
            .get(&surface.id().protocol_id())
            .copied()
        {
            self.sync_client_meta(id);
        }
    }
}

impl ShmHandler for DwmState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl SeatHandler for DwmState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
}

impl OutputHandler for DwmState {}

impl XdgDecorationHandler for DwmState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(XdgDecoMode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: XdgDecoMode) {
        // Always server-side: compositor draws dwm borders, never client title bars.
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(XdgDecoMode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(XdgDecoMode::ServerSide);
        });
        toplevel.send_configure();
    }
}

impl KdeDecorationHandler for DwmState {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn new_decoration(
        &mut self,
        _surface: &WlSurface,
        decoration: &wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration,
    ) {
        decoration.mode(KdeDecoMode::Server);
    }

    fn request_mode(
        &mut self,
        _surface: &WlSurface,
        decoration: &wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration,
        _mode: WEnum<KdeDecoMode>,
    ) {
        decoration.mode(KdeDecoMode::Server);
    }
}

delegate_xdg_shell!(DwmState);
delegate_xdg_decoration!(DwmState);
delegate_kde_decoration!(DwmState);
delegate_compositor!(DwmState);
delegate_shm!(DwmState);
delegate_seat!(DwmState);
delegate_data_device!(DwmState);
delegate_primary_selection!(DwmState);
delegate_output!(DwmState);

#[derive(Default)]
pub(crate) struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: WlClientId) {}
    fn disconnected(&self, _client_id: WlClientId, _reason: DisconnectReason) {}
}

/// Headless protocol + core smoke test (no GPU / winit).
pub fn check_protocols() -> Result<String, Error> {
    let display: Display<DwmState> = Display::new()?;
    let dh = display.handle();
    let mut state = DwmState::new(&dh, Rect::new(0, 0, 1280, 720));

    for i in 0..3 {
        let c = DwmClient::new(ClientId(0), format!("check-{i}"));
        state.monitor.manage(c);
    }
    state.apply_command(Command::SetLayout(dwm_core::LayoutKind::Tile));
    let (geoms, _symbol) = state.monitor.arrange();
    if geoms.len() < 3 {
        return Err(Error::Other(format!(
            "expected >=3 geometries, got {}",
            geoms.len()
        )));
    }
    state.apply_command(Command::FocusStack { delta: 1 });
    state.apply_command(Command::SetLayout(dwm_core::LayoutKind::Monocle));
    state.apply_command(Command::ToggleGaps);

    Ok(format!(
        "dwm-wayland check ok: clients={} layout={:?} tagset={:?} symbol={}",
        state.monitor.clients.len(),
        state.monitor.layout,
        state.monitor.tagset(),
        state.layout_symbol,
    ))
}

pub fn run() -> Result<(), Error> {
    let mut display: Display<DwmState> = Display::new()?;
    let dh = display.handle();

    let socket_name = unique_socket_name();
    let listener = ListeningSocket::bind(&socket_name).map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            format!("bind {socket_name}: {e}"),
        ))
    })?;
    info!("listening on WAYLAND_DISPLAY={socket_name}");

    let (mut backend, mut winit_loop) = winit_backend::init_from_attributes::<GlesRenderer>(
        winit::window::Window::default_attributes()
            .with_title("dwm-wayland")
            .with_visible(true),
    )
    .map_err(|e| Error::Winit(format!("{e:?}")))?;

    let size = backend.window_size();
    let screen = Rect::new(0, 0, size.w, size.h);
    let mut state = DwmState::new(&dh, screen);
    let _keyboard = state
        .seat
        .add_keyboard(Default::default(), 200, 200)
        .map_err(|e| Error::Other(format!("keyboard: {e:?}")))?;

    std::env::set_var("WAYLAND_DISPLAY", &socket_name);

    let mut clients = Vec::new();
    let start_time = Instant::now();

    if std::env::var_os("DWM_WAYLAND_NO_AUTOSTART").is_none() {
        let term = state.config.terminal.clone();
        spawn_program(&term, &[]);
    }

    loop {
        if state.should_quit {
            info!("quit requested");
            return Ok(());
        }

        let status = winit_loop.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => {
                state.resize_output(size.w, size.h);
                state.monitor.set_screen(Rect::new(0, 0, size.w, size.h));
                state.rearrange();
            }
            WinitEvent::Input(event) => {
                state.process_input_event(event);
            }
            _ => {}
        });

        match status {
            PumpStatus::Continue => {}
            PumpStatus::Exit(_) => return Ok(()),
        }

        let size = backend.window_size();
        let damage = Rectangle::from_size(size);

        state.refresh_status();

        {
            let (renderer, mut framebuffer) = backend
                .bind()
                .map_err(|e| Error::Winit(format!("bind: {e:?}")))?;

            let popup_surfaces = state
                .render_gles_frame(renderer, &mut framebuffer, size, Transform::Flipped180)
                .map_err(Error::Winit)?;
            state.send_frames(start_time.elapsed().as_millis() as u32, &popup_surfaces);

            if let Some(stream) = listener.accept()? {
                info!("client connected");
                let client = display
                    .handle()
                    .insert_client(stream, Arc::new(ClientState::default()))
                    .map_err(|e| Error::Other(format!("insert_client: {e}")))?;
                clients.push(client);
            }

            display
                .dispatch_clients(&mut state)
                .map_err(|e| Error::Other(format!("dispatch: {e}")))?;
            display
                .flush_clients()
                .map_err(|e| Error::Other(format!("flush: {e}")))?;
        }

        backend
            .submit(Some(&[damage]))
            .map_err(|e| Error::Winit(format!("submit: {e:?}")))?;
    }
}

pub(crate) fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

fn unique_socket_name() -> String {
    let pid = std::process::id();
    format!("dwm-wayland-{pid}")
}
