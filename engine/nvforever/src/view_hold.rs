//! Photo mode's held view in co-op (docs/design/photo-mode.md, "Solo and co-op"): the
//! shaders' clock stands still for this player while the world's clock goes on.
//!
//! In co-op nothing pauses (the virtual clock drives the simulation and the network), so
//! the drawn picture is held instead: `nvf_play::photo_mode` holds the drawn places and
//! poses, the systems that only move the picture wait (`nvf_play::photo_mode::view_live`),
//! and here the time the shaders read (`globals.time`: water, grass and tree wind, fire)
//! is the time photo mode opened at, with no time passing, until it closes [design: owner
//! request 2026-11-14, "a true frozen snapshot you fly around in"].

use bevy::prelude::*;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use nvf_play::photo_mode::PhotoMode;
use std::time::Duration;

pub struct ViewHoldPlugin;

impl Plugin for ViewHoldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldShaderTime>().add_systems(Last, track);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(ExtractSchedule, extract_held).add_systems(Render, apply_held.in_set(RenderSystems::PrepareAssets));
        }
    }
}

/// The clock the shaders are shown while the view is held: the time photo mode opened at,
/// no time passing.
#[derive(Resource, Default, Clone, Copy)]
struct HeldShaderTime(Option<Time>);

/// The held time, in the render world.
#[derive(Resource, Default, Clone, Copy)]
struct RenderHeldTime(Option<Time>);

/// The time to hold, from the frame the view is held to the frame it goes live.
fn track(photo: Option<Res<PhotoMode>>, time: Res<Time>, mut held: ResMut<HeldShaderTime>) {
    let on = photo.is_some_and(|p| p.holds_view());
    if on == held.0.is_some() {
        return;
    }
    held.0 = on.then(|| still(*time));
    if on {
        info!("view hold: the shaders' clock stands at {:.2} s", time.elapsed_secs());
    } else {
        info!("view hold: the shaders' clock runs again");
    }
}

/// `time` with no time passing: the same moment, a zero step.
fn still(mut time: Time) -> Time {
    time.advance_by(Duration::ZERO);
    time
}

fn extract_held(mut commands: Commands, held: Extract<Res<HeldShaderTime>>) {
    commands.insert_resource(RenderHeldTime(held.0));
}

/// Before the globals buffer is written (`RenderSystems::PrepareResources`): the extracted
/// clock is replaced by the held one.
fn apply_held(held: Option<Res<RenderHeldTime>>, mut time: ResMut<Time>) {
    if let Some(t) = held.and_then(|h| h.0) {
        *time = t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_clock_keeps_its_moment_and_passes_no_time() {
        let mut t = Time::<()>::default();
        t.advance_by(Duration::from_millis(1500));
        t.advance_by(Duration::from_millis(16));
        let s = still(t);
        assert_eq!(s.elapsed(), t.elapsed());
        assert_eq!(s.delta(), Duration::ZERO);
    }
}
