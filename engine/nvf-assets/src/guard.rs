//! Loading a broken file must not stop the game. Every step that turns a file from the
//! game data or a mod (a model, a texture, an animation) into something drawn runs under
//! [`guarded`]: a panic in it (a bug of ours met by a file nobody tested) becomes an
//! error for that one file, which is then treated like a missing file.
//!
//! The crash hook (`nvforever::crash`) asks [`in_guard`] and writes no crash report for a
//! panic caught here; it logs a warning instead.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    /// How many [`guarded`] calls this thread is inside.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Whether this thread is running a [`guarded`] step now (for the panic hook).
pub fn in_guard() -> bool {
    DEPTH.with(|d| d.get() > 0)
}

/// Run `f`; a panic in it becomes `Err("panicked: <message>")`. `f` must leave nothing
/// half-changed that matters when it stops part way: what it built is dropped, and what
/// it put in shared stores stays unused.
pub fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    DEPTH.with(|d| d.set(d.get() + 1));
    let r = catch_unwind(AssertUnwindSafe(f));
    DEPTH.with(|d| d.set(d.get() - 1));
    r.unwrap_or_else(|p| Err(format!("panicked: {}", message(p.as_ref()))))
}

/// Record that a file could not be used (`what`: "model", "texture", ...): logged as it
/// happens and kept in `list` (the caches remember each failed file, so each is logged
/// once).
pub fn failed(list: &mut Vec<String>, what: &str, text: String) {
    bevy::log::warn!("{what} not loaded: {text}");
    list.push(text);
}

/// The text of a panic's payload.
pub fn message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "(not text)".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_becomes_an_error() {
        let r: Result<u32, String> = guarded(|| {
            assert!(in_guard());
            let v: Vec<u32> = std::hint::black_box(Vec::new());
            Ok(v[3])
        });
        assert!(r.unwrap_err().starts_with("panicked: index out of bounds"));
        assert!(!in_guard());
        assert_eq!(guarded(|| Ok::<_, String>(5)), Ok(5));
        assert_eq!(guarded(|| Err::<u32, _>("no".to_string())), Err("no".into()));
    }
}
