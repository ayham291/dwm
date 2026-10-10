//! Smithay compositor state wired to `dwm-core`.

use std::collections::HashMap;
use std::os::unix::io::OwnedFd;
use std::process::Command as ProcCommand;
use std::sync::Arc;
use std::time::Instant;

use dwm_core::{Client as DwmClient, ClientId, Command, Config, Monitor, Rect};
use smithay::backend::input::{InputEvent, KeyState, KeyboardKeyEvent};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::{draw_render_elements, on_commit_buffer_handler};
use smithay::backend::renderer::{Color32F, Frame, Renderer};
use smithay::backend::winit::{self as winit_backend, WinitEvent};
use smithay::input::keyboard::FilterResult;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::Display;
use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Scale, Subpixel};
use smithay::utils::{Logical, Point, Rectangle, Serial, Size, Transform};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::compositor::{
    with_surface_tree_downward, CompositorClientState, CompositorHandler, CompositorState,
    SurfaceAttributes, TraversalAction,
};
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_shell,
};
use tracing::{info, warn};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::backend::{ClientData, ClientId as WlClientId, DisconnectReason};
use wayland_server::protocol::wl_buffer;
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{Client, DisplayHandle, ListeningSocket, Resource};
use winit::platform::pump_events::PumpStatus;

use crate::bindings::{self, Cmd};

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

struct DwmState {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    _output_manager_state: OutputManagerState,
    seat: Seat<Self>,
    output: Output,

    monitor: Monitor,
    config: Config,
    bindings: Vec<bindings::Binding>,

    /// Map dwm ClientId → xdg toplevel.
    toplevels: HashMap<ClientId, ToplevelSurface>,
    /// Map wl_surface id → ClientId for commit lookups.
    surface_clients: HashMap<u32, ClientId>,

    should_quit: bool,
    layout_symbol: String,
}

impl DwmState {
    fn new(dh: &DisplayHandle, screen: Rect) -> Self {
        let config = Config::default();
        let mut seat_state = SeatState::new();
        let seat = seat_state.new_wl_seat(dh, "dwm");

        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
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

        Self {
            compositor_state: CompositorState::new::<Self>(dh),
            xdg_shell_state: XdgShellState::new::<Self>(dh),
            shm_state: ShmState::new::<Self>(dh, vec![]),
            seat_state,
            data_device_state: DataDeviceState::new::<Self>(dh),
            _output_manager_state: output_manager_state,
            seat,
            output,
            monitor: Monitor::from_config(&config, screen),
            config,
            bindings: bindings::default_bindings(),
            toplevels: HashMap::new(),
            surface_clients: HashMap::new(),
            should_quit: false,
            layout_symbol: "[]=".into(),
        }
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

    fn apply_command(&mut self, cmd: Command) {
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
        self.push_configures(&frame.geometries);
        if let Some(focus) = frame.focused {
            self.focus_client(focus);
        }
    }

    fn push_configures(&mut self, geometries: &[(ClientId, Rect)]) {
        for (id, rect) in geometries {
            let Some(toplevel) = self.toplevels.get(id) else {
                continue;
            };
            if rect.w <= 0 || rect.h <= 0 {
                toplevel.with_pending_state(|state| {
                    state.states.unset(xdg_toplevel::State::Activated);
                });
                toplevel.send_configure();
                continue;
            }
            let size = Size::<i32, Logical>::from((rect.w, rect.h));
            let activated = self.monitor.sel == Some(*id);
            toplevel.with_pending_state(|state| {
                state.size = Some(size);
                if activated {
                    state.states.set(xdg_toplevel::State::Activated);
                } else {
                    state.states.unset(xdg_toplevel::State::Activated);
                }
            });
            toplevel.send_configure();
        }
    }

    fn focus_client(&mut self, id: ClientId) {
        if let Some(toplevel) = self.toplevels.get(&id) {
            let surface = toplevel.wl_surface().clone();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surface), Serial::from(0));
            }
        }
    }

    fn handle_key(
        &mut self,
        keysym: smithay::input::keyboard::Keysym,
        mods: &smithay::input::keyboard::ModifiersState,
    ) {
        let Some(cmd) = bindings::lookup(&self.bindings, mods, keysym) else {
            return;
        };
        match cmd {
            Cmd::Core(c) => self.apply_command(c),
            Cmd::SpawnTerminal => spawn_program(&self.config.terminal, &[]),
            Cmd::SpawnDmenu => spawn_program("dmenu_run", &[]),
        }
    }

}

fn spawn_program(bin: &str, args: &[&str]) {
    match ProcCommand::new(bin).args(args).spawn() {
        Ok(_) => info!("spawned {bin}"),
        Err(e) => warn!("failed to spawn {bin}: {e}"),
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

        let (geoms, symbol) = self.monitor.arrange();
        self.layout_symbol = symbol;
        self.push_configures(&geoms);
        self.focus_client(id);
        info!("new toplevel client={id:?} layout={}", self.layout_symbol);
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
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
}

impl SelectionHandler for DwmState {
    type SelectionUserData = ();
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
        let _ = surface;
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

delegate_xdg_shell!(DwmState);
delegate_compositor!(DwmState);
delegate_shm!(DwmState);
delegate_seat!(DwmState);
delegate_data_device!(DwmState);
delegate_output!(DwmState);

#[derive(Default)]
struct ClientState {
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

    let (mut backend, mut winit_loop) = winit_backend::init::<GlesRenderer>()
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
                let (geoms, symbol) = state.monitor.arrange();
                state.layout_symbol = symbol;
                state.push_configures(&geoms);
            }
            WinitEvent::Input(event) => {
                if let InputEvent::Keyboard { event } = event {
                    let keyboard = state.seat.get_keyboard().unwrap();
                    let pressed = event.state() == KeyState::Pressed;
                    keyboard.input::<(), _>(
                        &mut state,
                        event.key_code(),
                        event.state(),
                        Serial::from(0),
                        0,
                        |state, modifiers, handle| {
                            if pressed {
                                // Prefer raw keysyms so Shift doesn't turn `u` into `U`.
                                let keysym = handle
                                    .raw_syms()
                                    .first()
                                    .copied()
                                    .unwrap_or_else(|| handle.modified_sym());
                                state.handle_key(keysym, modifiers);
                            }
                            FilterResult::Forward
                        },
                    );
                }
            }
            _ => {}
        });

        match status {
            PumpStatus::Continue => {}
            PumpStatus::Exit(_) => return Ok(()),
        }

        let size = backend.window_size();
        let damage = Rectangle::from_size(size);

        {
            let (renderer, mut framebuffer) = backend
                .bind()
                .map_err(|e| Error::Winit(format!("bind: {e:?}")))?;

            let mut elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
            let tagset = state.monitor.tagset();
            for client in state.monitor.visible_clients() {
                let Some(toplevel) = state.toplevels.get(&client.id) else {
                    continue;
                };
                if !client.is_visible(tagset) {
                    continue;
                }
                let loc: Point<i32, Logical> = (client.geom.x, client.geom.y).into();
                elements.extend(render_elements_from_surface_tree(
                    renderer,
                    toplevel.wl_surface(),
                    loc.to_physical_precise_round(1.0),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                ));
            }

            let mut frame = renderer
                .render(&mut framebuffer, size, Transform::Flipped180)
                .map_err(|e| Error::Winit(format!("render: {e:?}")))?;
            frame
                .clear(Color32F::new(0.12, 0.12, 0.12, 1.0), &[damage])
                .map_err(|e| Error::Winit(format!("clear: {e:?}")))?;
            draw_render_elements(&mut frame, 1.0, &elements, &[damage])
                .map_err(|e| Error::Winit(format!("draw: {e:?}")))?;
            let _ = frame
                .finish()
                .map_err(|e| Error::Winit(format!("finish: {e:?}")))?;

            for surface in state.xdg_shell_state.toplevel_surfaces() {
                send_frames_surface_tree(
                    surface.wl_surface(),
                    start_time.elapsed().as_millis() as u32,
                );
            }

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

fn send_frames_surface_tree(surface: &WlSurface, time: u32) {
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
