//! NVForever change: a process may ignore this machine's keyboard and mouse altogether
//! (split screen's player 2, whose controller is relayed to it by player 1's game; nvforever
//! `split.rs`). While [`set_ignored`] is on, the event loop drops the keyboard, mouse,
//! touch, gesture and text-input window events and the raw mouse motion before they become
//! Bevy messages, so nothing in the app reads them, whichever window has the focus. Input
//! an app writes as messages itself (test aids) is untouched.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use winit::event::WindowEvent;

static IGNORED: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Turns ignoring this machine's keyboard and mouse on or off.
pub fn set_ignored(on: bool) {
    IGNORED.store(on, Ordering::Relaxed);
}

/// Whether this machine's keyboard and mouse are ignored.
pub fn ignored() -> bool {
    IGNORED.load(Ordering::Relaxed)
}

/// How many keyboard and mouse events were dropped so far.
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Whether `event` is keyboard, mouse, touch or text input.
pub fn is_hardware_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::Ime(_)
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch(_)
            | WindowEvent::TouchpadPressure { .. }
            | WindowEvent::PinchGesture { .. }
            | WindowEvent::PanGesture { .. }
            | WindowEvent::RotationGesture { .. }
            | WindowEvent::DoubleTapGesture { .. }
    )
}

/// True (and counted) when `event` is to be dropped.
pub(crate) fn drops(event: &WindowEvent) -> bool {
    let drop = ignored() && is_hardware_input(event);
    if drop {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    drop
}

/// True (and counted) when raw mouse motion is to be dropped.
pub(crate) fn drops_motion() -> bool {
    let drop = ignored();
    if drop {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    drop
}
