//! DRM / libseat / libinput session backend.
//!
//! Opens a seat session, finds a primary GPU, modesets the first connected
//! connector via GBM, and runs a calloop-driven Wayland + libinput loop.
//! Adapted/simplified from smithay's anvil `udev` backend (no multi-GPU lease,
//! tracy, or debug overlays).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, Mode, PostAction, RegistrationToken};
use calloop::generic::Generic;
use dwm_core::Rect;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::allocator::Fourcc;
use smithay::backend::drm::{
    DrmDevice, DrmDeviceFd, DrmDeviceNotifier, DrmEvent, DrmNode, GbmBufferedSurface, NodeType,
};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Bind, ImportMemWl};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{all_gpus, primary_gpu, UdevBackend, UdevEvent};
use smithay::output::{Mode as WlMode, Output, PhysicalProperties, Scale};
use smithay::reexports::drm::control::{connector, crtc, ModeTypeFlags};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{DeviceFd, Rectangle, Transform};
use smithay::wayland::socket::ListeningSocketSource;
use smithay_drm_extras::display_info;
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};
use tracing::{error, info, warn};

use crate::state::{ClientState, DwmState, Error};

const SUPPORTED_FORMATS: &[Fourcc] = &[
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
];

type RenderSurface = GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>;

struct GpuBackend {
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    drm_scanner: DrmScanner,
    #[allow(dead_code)] // kept so the DRM notifier source stays registered for the device lifetime
    registration_token: RegistrationToken,
    surfaces: HashMap<crtc::Handle, OutputSurface>,
}

struct OutputSurface {
    gbm_surface: RenderSurface,
}

struct UdevLoopState {
    dwm: DwmState,
    dh: DisplayHandle,
    session: LibSeatSession,
    primary_gpu: DrmNode,
    backend: Option<GpuBackend>,
    renderer: GlesRenderer,
    start_time: Instant,
    pending_render: bool,
    /// Primary connector CRTC currently driving the compositor output.
    primary_crtc: Option<crtc::Handle>,
}

#[cfg(feature = "xwayland")]
crate::forward_xwayland_to_dwm!(UdevLoopState);

/// Run the DRM/libseat session compositor until quit or a fatal backend error.
pub fn run() -> Result<(), Error> {
    info!("udev backend: starting DRM/libseat/libinput session");

    let mut event_loop: EventLoop<'static, UdevLoopState> =
        EventLoop::try_new().map_err(|e| Error::Other(format!("event loop: {e}")))?;
    let display: Display<DwmState> = Display::new()?;
    let dh = display.handle();

    let (mut session, notifier) = LibSeatSession::new().map_err(|err| {
        warn!("udev backend: LibSeatSession::new failed: {err:?}");
        Error::Other(format!("libseat session: {err:?}"))
    })?;
    let seat_name = session.seat();
    info!("udev backend: session seat={seat_name}");

    let primary_gpu = find_primary_gpu(&seat_name)?;
    info!("udev backend: primary GPU {primary_gpu}");

    // Wayland listening socket (calloop source).
    let socket = ListeningSocketSource::with_name(&unique_socket_name())
        .or_else(|_| ListeningSocketSource::new_auto())
        .map_err(|e| Error::Other(format!("wayland socket: {e:?}")))?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    info!("listening on WAYLAND_DISPLAY={socket_name}");
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);

    event_loop
        .handle()
        .insert_source(socket, |client_stream, _, state| {
            if let Err(err) = state
                .dh
                .insert_client(client_stream, Arc::new(ClientState::default()))
            {
                warn!("failed to insert wayland client: {err}");
            } else {
                info!("client connected");
            }
        })
        .map_err(|e| Error::Other(format!("socket source: {e}")))?;

    // Display dispatch source.
    event_loop
        .handle()
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                // SAFETY: display is owned by the event source for the loop lifetime.
                unsafe {
                    if let Err(err) = display.get_mut().dispatch_clients(&mut state.dwm) {
                        error!("dispatch_clients: {err}");
                    }
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| Error::Other(format!("display source: {e}")))?;

    // Open primary DRM device before building compositor state (need connector size).
    let udev_backend = UdevBackend::new(&seat_name)
        .map_err(|e| Error::Other(format!("udev backend: {e}")))?;

    let primary_node = primary_gpu
        .node_with_type(NodeType::Primary)
        .and_then(|n| n.ok());
    let primary_entry = udev_backend.device_list().find(|(device_id, _)| {
        primary_node
            .map(|n| *device_id == n.dev_id())
            .unwrap_or(false)
            || *device_id == primary_gpu.dev_id()
    });

    let (device_id, path) = primary_entry.ok_or_else(|| {
        Error::Other(
            "no DRM device found for primary GPU (is /dev/dri available?)".into(),
        )
    })?;
    let node = DrmNode::from_dev_id(device_id)
        .map_err(|e| Error::Other(format!("drm node: {e}")))?;
    let path = path.to_path_buf();

    let (drm, gbm, drm_notifier) = open_drm_device(&mut session, &path)?;
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }
        .map_err(|e| Error::Other(format!("EGLDisplay: {e:?}")))?;
    let context =
        EGLContext::new(&egl).map_err(|e| Error::Other(format!("EGLContext: {e:?}")))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }
        .map_err(|e| Error::Other(format!("GlesRenderer: {e:?}")))?;

    let drm_token = event_loop
        .handle()
        .insert_source(drm_notifier, move |event, _metadata, state| match event {
            DrmEvent::VBlank(crtc) => {
                if let Some(backend) = state.backend.as_mut() {
                    if let Some(surface) = backend.surfaces.get_mut(&crtc) {
                        if let Err(err) = surface.gbm_surface.frame_submitted() {
                            warn!("frame_submitted: {err:?}");
                        }
                    }
                }
                state.pending_render = true;
            }
            DrmEvent::Error(err) => {
                error!("DRM error: {err:?}");
            }
        })
        .map_err(|e| Error::Other(format!("drm notifier: {e}")))?;

    let mut gpu = GpuBackend {
        drm,
        gbm,
        drm_scanner: DrmScanner::new(),
        registration_token: drm_token,
        surfaces: HashMap::new(),
    };

    // Scan connectors and modeset the first usable one.
    let (output, screen, primary_crtc) = setup_first_connector(&dh, &mut gpu, &mut renderer)?;

    let mut dwm = DwmState::new_with_output(&dh, seat_name.clone(), screen, output);
    let _keyboard = dwm
        .seat
        .add_keyboard(Default::default(), 200, 200)
        .map_err(|e| Error::Other(format!("keyboard: {e:?}")))?;
    dwm.update_shm_formats(renderer.shm_formats());

    let mut loop_state = UdevLoopState {
        dwm,
        dh: dh.clone(),
        session,
        primary_gpu: node,
        backend: Some(gpu),
        renderer,
        start_time: Instant::now(),
        pending_render: true,
        primary_crtc: Some(primary_crtc),
    };

    // Libinput
    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        loop_state.session.clone().into(),
    );
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|e| Error::Other(format!("libinput udev_assign_seat: {e:?}")))?;
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, state| {
            state.dwm.process_input_event(event);
            state.pending_render = true;
        })
        .map_err(|e| Error::Other(format!("libinput source: {e}")))?;

    // Session pause / resume
    event_loop
        .handle()
        .insert_source(notifier, move |event, &mut (), state| match event {
            SessionEvent::PauseSession => {
                info!("session paused");
                libinput_context.suspend();
                if let Some(backend) = state.backend.as_mut() {
                    backend.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                info!("session resumed");
                if let Err(err) = libinput_context.resume() {
                    error!("libinput resume failed: {err:?}");
                }
                if let Some(backend) = state.backend.as_mut() {
                    if let Err(err) = backend.drm.activate(false) {
                        error!("drm activate failed: {err:?}");
                    }
                }
                state.pending_render = true;
            }
        })
        .map_err(|e| Error::Other(format!("session notifier: {e}")))?;

    // Hotplug
    for (device_id, path) in udev_backend.device_list() {
        // Primary already opened; ignore others for this simplified backend.
        let _ = (device_id, path);
    }
    event_loop
        .handle()
        .insert_source(udev_backend, move |event, _, state| {
            handle_udev_event(event, state);
        })
        .map_err(|e| Error::Other(format!("udev source: {e}")))?;

    // Render timer (~60 Hz kick; actual scanout waits on VBlank).
    event_loop
        .handle()
        .insert_source(Timer::from_duration(Duration::from_millis(16)), |_, _, state| {
            state.pending_render = true;
            TimeoutAction::ToDuration(Duration::from_millis(16))
        })
        .map_err(|e| Error::Other(format!("render timer: {e}")))?;

    #[cfg(feature = "xwayland")]
    {
        let handle = event_loop.handle();
        crate::xwayland::spawn_xwayland(handle, &dh, |state, wm, display_number| {
            crate::xwayland::on_ready_store_on_dwm(&mut state.dwm, wm, display_number);
        })
        .map_err(Error::Other)?;
    }

    if std::env::var_os("DWM_WAYLAND_NO_AUTOSTART").is_none() {
        let term = loop_state.dwm.config.terminal.clone();
        crate::state::spawn_program(&term, &[]);
    }

    info!("udev backend: entering event loop");
    let signal = event_loop.get_signal();
    event_loop
        .run(None, &mut loop_state, |state| {
            if state.dwm.should_quit() {
                signal.stop();
                return;
            }
            if state.pending_render {
                if let Err(err) = render_frame(state) {
                    warn!("render: {err}");
                }
                state.pending_render = false;
            }
            state.dwm.refresh_status();
            let _ = state.dh.flush_clients();
        })
        .map_err(|e| Error::Other(format!("event loop run: {e}")))?;

    info!("udev backend: exiting");
    Ok(())
}

fn find_primary_gpu(seat: &str) -> Result<DrmNode, Error> {
    if let Ok(var) = std::env::var("DWM_WAYLAND_DRM_DEVICE") {
        return DrmNode::from_path(var).map_err(|e| Error::Other(format!("DRM device path: {e}")));
    }

    let from_primary = primary_gpu(seat)
        .map_err(|e| Error::Other(format!("primary_gpu: {e}")))?
        .and_then(|path| {
            DrmNode::from_path(path)
                .ok()?
                .node_with_type(NodeType::Render)?
                .ok()
        });

    if let Some(node) = from_primary {
        return Ok(node);
    }

    all_gpus(seat)
        .map_err(|e| Error::Other(format!("all_gpus: {e}")))?
        .into_iter()
        .find_map(|path| DrmNode::from_path(path).ok())
        .ok_or_else(|| Error::Other("no GPU found (is /dev/dri available?)".into()))
}

fn open_drm_device(
    session: &mut LibSeatSession,
    path: &Path,
) -> Result<(DrmDevice, GbmDevice<DrmDeviceFd>, DrmDeviceNotifier), Error> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| Error::Other(format!("session.open({}): {e:?}", path.display())))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier) = DrmDevice::new(fd.clone(), true)
        .map_err(|e| Error::Other(format!("DrmDevice::new: {e:?}")))?;
    let gbm = GbmDevice::new(fd).map_err(|e| Error::Other(format!("GbmDevice::new: {e}")))?;
    Ok((drm, gbm, notifier))
}

fn setup_first_connector(
    dh: &DisplayHandle,
    gpu: &mut GpuBackend,
    renderer: &mut GlesRenderer,
) -> Result<(Output, Rect, crtc::Handle), Error> {
    let scan = gpu
        .drm_scanner
        .scan_connectors(&gpu.drm)
        .map_err(|e| Error::Other(format!("scan_connectors: {e:?}")))?;

    for event in scan {
        if let DrmScanEvent::Connected {
            connector,
            crtc: Some(crtc),
        } = event
        {
            match create_output_surface(dh, gpu, renderer, connector, crtc) {
                Ok((output, screen)) => return Ok((output, screen, crtc)),
                Err(err) => {
                    warn!("skipping connector: {err}");
                }
            }
        }
    }

    Err(Error::Other(
        "no active DRM connector with a CRTC (attach a display?)".into(),
    ))
}

fn create_output_surface(
    dh: &DisplayHandle,
    gpu: &mut GpuBackend,
    renderer: &mut GlesRenderer,
    connector: connector::Info,
    crtc: crtc::Handle,
) -> Result<(Output, Rect), Error> {
    let output_name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );
    info!("modesetting connector {output_name} on {crtc:?}");

    let mode_id = connector
        .modes()
        .iter()
        .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .unwrap_or(0);
    let drm_mode = *connector
        .modes()
        .get(mode_id)
        .ok_or_else(|| Error::Other(format!("connector {output_name} has no modes")))?;
    let wl_mode = WlMode::from(drm_mode);

    let display_info = display_info::for_connector(&gpu.drm, connector.handle());
    let make = display_info
        .as_ref()
        .and_then(|info| info.make())
        .unwrap_or_else(|| "Unknown".into());
    let model = display_info
        .as_ref()
        .and_then(|info| info.model())
        .unwrap_or_else(|| "Unknown".into());

    let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
    let output = Output::new(
        output_name.clone(),
        PhysicalProperties {
            size: (phys_w as i32, phys_h as i32).into(),
            subpixel: connector.subpixel().into(),
            make,
            model,
        },
    );
    let _global = output.create_global::<DwmState>(dh);
    output.set_preferred(wl_mode);
    output.change_current_state(
        Some(wl_mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output.add_mode(wl_mode);

    let drm_surface = gpu
        .drm
        .create_surface(crtc, drm_mode, &[connector.handle()])
        .map_err(|e| Error::Other(format!("create_surface({output_name}): {e:?}")))?;

    let allocator = GbmAllocator::new(
        gpu.gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let render_formats = renderer.egl_context().dmabuf_render_formats().clone();
    let gbm_surface = RenderSurface::new(
        drm_surface,
        allocator,
        SUPPORTED_FORMATS,
        render_formats.iter().copied(),
    )
    .map_err(|e| Error::Other(format!("GbmBufferedSurface({output_name}): {e:?}")))?;

    gpu.surfaces.insert(
        crtc,
        OutputSurface {
            gbm_surface,
        },
    );

    let screen = Rect::new(0, 0, wl_mode.size.w, wl_mode.size.h);
    Ok((output, screen))
}

fn handle_udev_event(event: UdevEvent, state: &mut UdevLoopState) {
    match event {
        UdevEvent::Added { device_id, path } => {
            info!("udev Added device_id={device_id} path={}", path.display());
            // Single-GPU backend: ignore secondary GPUs for now.
            let _ = PathBuf::from(path);
        }
        UdevEvent::Changed { device_id } => {
            let Ok(node) = DrmNode::from_dev_id(device_id) else {
                return;
            };
            let is_primary = state.primary_gpu == node
                || state
                    .primary_gpu
                    .node_with_type(NodeType::Primary)
                    .and_then(|n| n.ok())
                    .is_some_and(|n| n == node);
            if !is_primary {
                return;
            }

            let events: Vec<DrmScanEvent> = {
                let Some(backend) = state.backend.as_mut() else {
                    return;
                };
                match backend.drm_scanner.scan_connectors(&backend.drm) {
                    Ok(s) => s.into_iter().collect(),
                    Err(err) => {
                        warn!("rescan connectors: {err:?}");
                        return;
                    }
                }
            };

            for ev in events {
                match ev {
                    DrmScanEvent::Connected {
                        connector,
                        crtc: Some(crtc),
                    } => {
                        if state.primary_crtc.is_some() {
                            info!(
                                "additional connector {}-{} ignored (single-output session)",
                                connector.interface().as_str(),
                                connector.interface_id()
                            );
                            continue;
                        }
                        let Some(backend) = state.backend.as_mut() else {
                            return;
                        };
                        match create_output_surface(
                            &state.dh,
                            backend,
                            &mut state.renderer,
                            connector,
                            crtc,
                        ) {
                            Ok((_output, screen)) => {
                                state.dwm.monitor.set_screen(screen);
                                state.dwm.rearrange();
                                state.primary_crtc = Some(crtc);
                                state.pending_render = true;
                            }
                            Err(err) => warn!("hotplug connector failed: {err}"),
                        }
                    }
                    DrmScanEvent::Disconnected {
                        connector,
                        crtc: Some(crtc),
                    } => {
                        info!(
                            "connector {}-{} disconnected",
                            connector.interface().as_str(),
                            connector.interface_id()
                        );
                        if let Some(backend) = state.backend.as_mut() {
                            backend.surfaces.remove(&crtc);
                        }
                        if state.primary_crtc == Some(crtc) {
                            warn!("primary output lost");
                            state.primary_crtc = None;
                        }
                    }
                    _ => {}
                }
            }
        }
        UdevEvent::Removed { device_id } => {
            warn!("udev Removed device_id={device_id}");
            if let Ok(node) = DrmNode::from_dev_id(device_id) {
                if node == state.primary_gpu {
                    let _ = state.backend.take();
                    state.primary_crtc = None;
                }
            }
        }
    }
}

fn render_frame(state: &mut UdevLoopState) -> Result<(), String> {
    let crtc = state
        .primary_crtc
        .ok_or_else(|| "no primary CRTC".to_string())?;
    let UdevLoopState {
        dwm,
        backend,
        renderer,
        start_time,
        ..
    } = state;
    let backend = backend
        .as_mut()
        .ok_or_else(|| "DRM backend gone".to_string())?;
    let surface = backend
        .surfaces
        .get_mut(&crtc)
        .ok_or_else(|| "no GbmBufferedSurface for primary CRTC".to_string())?;

    let (mut dmabuf, _age) = surface
        .gbm_surface
        .next_buffer()
        .map_err(|e| format!("next_buffer: {e:?}"))?;

    let mut framebuffer = renderer
        .bind(&mut dmabuf)
        .map_err(|e| format!("bind dmabuf: {e:?}"))?;

    let size = (dwm.monitor.screen.w, dwm.monitor.screen.h).into();
    let popup_surfaces =
        dwm.render_gles_frame(renderer, &mut framebuffer, size, Transform::Normal)?;
    drop(framebuffer);

    let damage = Rectangle::from_size(size);
    surface
        .gbm_surface
        .queue_buffer(None, Some(vec![damage]), ())
        .map_err(|e| format!("queue_buffer: {e:?}"))?;

    dwm.send_frames(start_time.elapsed().as_millis() as u32, &popup_surfaces);
    Ok(())
}

fn unique_socket_name() -> String {
    let pid = std::process::id();
    format!("dwm-wayland-{pid}")
}
