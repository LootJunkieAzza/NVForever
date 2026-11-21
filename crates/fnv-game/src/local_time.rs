//! Shared system-zone selection for the host-time script functions.

use jiff::tz::TimeZone;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn system_zone() -> TimeZone {
    static WARNED: AtomicBool = AtomicBool::new(false);
    match TimeZone::try_system() {
        Ok(zone) => zone,
        Err(error) => {
            if !WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(%error, "Host timezone unavailable; host-time script functions are using UTC");
            }
            TimeZone::unknown()
        }
    }
}
