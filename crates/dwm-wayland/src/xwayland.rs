//! XWayland integration stub (feature `xwayland`).
//!
//! TODO: Once the compositor runs under a calloop event loop (udev/session
//! backend), spawn [`smithay::xwayland::XWayland`], insert the client into the
//! display, and implement `XwmHandler` so X11 apps map as managed clients.
//!
//! The nested winit pump loop does not currently host calloop sources, so a
//! full spawn is deferred to keep `--no-default-features` and default builds green.

use tracing::info;

/// Log that the xwayland feature is compiled in.
pub fn announce() {
    info!(
        "xwayland: feature enabled (stub) — full XWayland spawn/XWM wiring TODO \
         (needs calloop-integrated backend)"
    );
}
