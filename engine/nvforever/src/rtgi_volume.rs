//! The bounce light of a probe volume on an interior's NIF surfaces (#3009,
//! docs/design/rtgi-volume.md) [design], an engineering choice, not the original game's. What is
//! here is the shader that reads a volume; **nothing makes or binds one**: there is no setting
//! and no texture, so no view has the bindings and no pipeline reads them.
//!
//! The parts, built the way the ambient occlusion's are (ssao_lighting.rs): bevy_pbr's view
//! component `NvfRtgiVolumeBindings` (vendored, engine/vendor/README.md), the NIF surfaces that
//! read it (nvf_assets nif_material.rs `reads_rtgi_volume`), rtgi_volume.wesl, and the text hook
//! that lights.rs patches into `pbr_functions` ([`hook`]).

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::prelude::*;
use bevy::shader::Shader;
use std::sync::atomic::{AtomicU8, Ordering};

pub mod hook;
pub(crate) use hook::hook_patched;

/// rtgi_volume.wesl, imported by the patched `apply_pbr_lighting` and by nvf_assets
/// nif_material.wesl as `nvforever::rtgi_volume`.
const SHADER: Handle<Shader> = uuid_handle!("3c0a7e14-91d2-4d86-b2f5-07c6ea51d9b4");

/// Whether lights.rs has put the hook into `pbr_functions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookState {
    /// Bevy's `pbr_functions` has not loaded yet.
    Pending,
    Installed,
    /// The source was not as expected: no view may get a volume.
    Failed,
}

static HOOK_STATE: AtomicU8 = AtomicU8::new(HookState::Pending as u8);

/// Record what became of the hook (lights.rs `patch_display_sum`).
pub(crate) fn set_hook_state(state: HookState) {
    HOOK_STATE.store(state as u8, Ordering::Relaxed);
}

/// What became of the hook; a volume may be bound only once it is in place (nothing binds one
/// yet, so only the tests ask).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn hook_state() -> HookState {
    match HOOK_STATE.load(Ordering::Relaxed) {
        0 => HookState::Pending,
        1 => HookState::Installed,
        _ => HookState::Failed,
    }
}

pub struct RtgiVolumePlugin;

impl Plugin for RtgiVolumePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "rtgi_volume.wesl", |s, _| Shader::from_wesl(s, "embedded://nvforever/rtgi_volume.wesl"));
    }
}

#[cfg(test)]
mod tests;
