//! DRM / libseat / udev session backend bootstrap.
//!
//! Full multi-GPU DRM + libinput loop is not wired yet; this module verifies that
//! a [`LibSeatSession`] can be acquired so default-feature builds compile and the
//! session entry point is exercised.

use smithay::backend::session::libseat::LibSeatSession;
use tracing::{info, warn};

use crate::state::Error;

/// Attempt to open a libseat session. Succeeds when seat access works; logs and
/// returns when the DRM render loop is not yet implemented.
pub fn run() -> Result<(), Error> {
    info!("udev backend: attempting LibSeatSession::new()");
    match LibSeatSession::new() {
        Ok((_session, _notifier)) => {
            info!("udev backend: LibSeatSession acquired successfully");
            warn!(
                "udev backend: DRM/libinput event loop not yet implemented — \
                 exiting after successful seat open"
            );
            Ok(())
        }
        Err(err) => {
            warn!("udev backend: LibSeatSession::new failed: {err:?}");
            Err(Error::Other(format!("libseat session: {err:?}")))
        }
    }
}
