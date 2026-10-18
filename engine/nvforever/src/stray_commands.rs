//! Commands sent to an entity that is gone.
//!
//! A system keeps an entity's handle, and the entity goes with its cell, an imposter
//! region or an actor's body. When a command for that handle is applied, Bevy's default
//! error handler panics and the game ends. The stress tour crashed this way at WastelandNV
//! cell -1,28 (2026-10-18) without naming the system: the backtrace had none, and the
//! command's name needs Bevy's `debug` feature, which the engine's Cargo.toml now turns
//! on.
//!
//! The fallback error handler ([`handler`]) logs such an error as a warning, naming the
//! command or system that failed. It counts them, and at the exit `stray entity commands:
//! N` names the first few. Any other error goes to Bevy's default ([`match_severity`]),
//! which panics as before. A stray command does nothing: the entity it was for is already
//! gone. A fix still belongs where the handle is kept (`try_insert`, or forgetting the
//! handle when its entity goes); these lines say where.

use bevy::ecs::error::{match_severity, BevyError, ErrorContext};
use bevy::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Stray commands so far, and the first few distinct ones (`<kind> <name>: <error>`).
static COUNT: AtomicUsize = AtomicUsize::new(0);
static FIRST: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Distinct stray commands kept for the exit line, and warnings logged at most.
const KEEP: usize = 8;
const WARN_MAX: usize = 50;

/// Whether an error is a command applied to an entity that no longer exists (or never
/// did): Bevy's "Entity despawned" / "not spawned" messages.
pub fn is_stray_entity(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("entity despawned") || m.contains("is invalid; its index now has generation") || m.contains("does not exist") && m.contains("entity")
}

/// The app's fallback error handler: stray entity commands are logged, the rest go to
/// Bevy's default handling.
pub fn handler(error: BevyError, ctx: ErrorContext) {
    let message = error.to_string();
    if !is_stray_entity(&message) {
        match_severity(error, ctx);
        return;
    }
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let first_line = message.lines().next().unwrap_or("").to_string();
    let what = format!("{} {}: {first_line}", ctx.kind(), ctx.name());
    if let Ok(mut f) = FIRST.lock() {
        // The same command again with another entity is the same fault.
        let key = format!("{} {}", ctx.kind(), ctx.name());
        if f.len() < KEEP && !f.iter().any(|w| w.starts_with(&key)) {
            f.push(what.clone());
        }
    }
    if n <= WARN_MAX {
        warn!("stray entity command #{n} (ignored; the entity is gone): {what}{}", crate::crash::place_text_suffix());
    }
}

/// How many stray commands were ignored so far.
pub fn count() -> usize {
    COUNT.load(Ordering::Relaxed)
}

/// At the exit: how many, and which.
pub fn report(mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_none() {
        return;
    }
    let n = count();
    if n == 0 {
        return;
    }
    let first = FIRST.lock().map(|f| f.join(" | ")).unwrap_or_default();
    info!("stray entity commands: {n} ignored; first of each: {first}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_despawned_entity_is_a_stray_command_and_other_errors_are_not() {
        // The tour's crash, 2026-10-18 (bevy_ecs 0.19).
        assert!(is_stray_entity("Entity despawned: The entity with ID 4742v5 is invalid; its index now has generation 6."));
        assert!(is_stray_entity("The entity with ID 12v1 does not exist"));
        assert!(!is_stray_entity("Resource does not exist: Assets<Mesh>"));
        assert!(!is_stray_entity("index out of bounds: the len is 3 but the index is 7"));
    }
}
