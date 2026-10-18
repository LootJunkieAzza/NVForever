//! The stress tests' samples (docs/design/stress-tests.md; the runner is `tools/stress`).
//!
//! `NVF_PLAY_TEST_SAMPLE=<seconds>`: every that many real seconds one line
//! `stress sample: t <s> s, frames <n>, frame mean <m> ms, frame worst <w> ms, entities <n>,
//! refs drawn <n>, meshes <n>, images <n>, materials <n>, models cached <n>, textures
//! cached <n>, audio <n>, cells <n>,
//! bodies <fixed>/<kinematic>/<dynamic>, actors <n>, nan actors <n>, ragdolls <n>,
//! projectiles <n>, at <place>` with what the engine holds now and where the camera is
//! (the crash report's place text). At the exit, `stress frames: <n> frames, mean <m> ms,
//! 1% low <l> ms (<fps> fps), worst <w> ms` over the run after its start-up ([`frame_stats`]).
//!
//! Start-up (the frames until the start's cells are all in, `Streamer::settled`; the first
//! takes the app's build and the Startup systems, 2.5 s in the stress runs of 2026-10-18)
//! is left out of the samples' and the run's frame times and logged once on its own:
//! `stress start-up: <n> frames, <ms> ms, slowest <w> ms`. After it, a frame slower than
//! [`HITCH_MS`] logs `stress hitch: <ms> ms at t <s> s, <k> cells loaded in it (<ms> ms),
//! at <place>`, so a stall can be told from a cell load. The process's memory is the
//! runner's to read (it has the process).

use crate::limbs::ActorRig;
use bevy::audio::AudioSink;
use bevy::prelude::*;
use nvf_play::phys::PhysState;
use nvf_play::RefVisual;

pub struct StressSamplePlugin;

impl StressSamplePlugin {
    /// The sampling interval, when `NVF_PLAY_TEST_SAMPLE` asks for samples.
    pub fn interval() -> Option<f32> {
        std::env::var("NVF_PLAY_TEST_SAMPLE").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|s| *s > 0.0)
    }
}

impl Plugin for StressSamplePlugin {
    fn build(&self, app: &mut App) {
        let Some(every) = Self::interval() else { return };
        info!("test aid NVF_PLAY_TEST_SAMPLE: a stress sample every {every} s");
        app.insert_resource(Samples { every, ..default() }).add_systems(Last, (sample, report).chain());
    }
}

/// A frame slower than this (milliseconds) after start-up is logged as a hitch.
pub const HITCH_MS: f32 = 250.0;

#[derive(Resource, Default)]
struct Samples {
    every: f32,
    since: f32,
    /// The start's cells are in: frame times count from the next frame on.
    started: bool,
    /// Start-up's frame times (milliseconds).
    startup: Vec<f32>,
    /// The streamer's cells loaded and their load time so far, at the last frame.
    cells_loaded: usize,
    load_ms: f64,
    /// This window's frame times, and every frame's (milliseconds).
    window: Vec<f32>,
    all: Vec<f32>,
}

/// (mean, 1 % low, worst) of frame times in milliseconds: the 1 % low is the mean of the
/// slowest hundredth of the frames (one frame at least). Zeros with no frames.
pub fn frame_stats(ms: &[f32]) -> (f32, f32, f32) {
    if ms.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mean = ms.iter().sum::<f32>() / ms.len() as f32;
    let mut sorted = ms.to_vec();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let n = (ms.len() / 100).max(1);
    let low = sorted[..n].iter().sum::<f32>() / n as f32;
    (mean, low, sorted[0])
}

#[allow(clippy::too_many_arguments)]
fn sample(
    real: Res<Time<Real>>,
    mut s: ResMut<Samples>,
    entities: Query<Entity>,
    refs: Query<(), With<RefVisual>>,
    actors: Query<&ActorRig>,
    globals: Query<&GlobalTransform>,
    audio: Query<(), With<AudioSink>>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
    cached: (Option<Res<nvf_assets::ModelCache>>, Option<Res<nvf_assets::TextureCache>>),
    phys: Option<Res<PhysState>>,
    combat: Option<Res<nvf_play::combat::Combat>>,
    streamer: Option<Res<crate::world::Streamer>>,
) {
    let dt = real.delta_secs();
    let ms = dt * 1000.0;
    s.since += dt;
    let (cells_loaded, load_ms) = streamer.as_deref().map_or((0, 0.0), |st| (st.stats.cells_loaded, st.stats.load_ms));
    let (new_cells, new_ms) = (cells_loaded.saturating_sub(s.cells_loaded), load_ms - s.load_ms);
    (s.cells_loaded, s.load_ms) = (cells_loaded, load_ms);
    if !s.started {
        s.startup.push(ms);
        // A start that never settles still ends start-up after a minute.
        if streamer.as_deref().is_none_or(|st| st.settled) || s.startup.iter().sum::<f32>() > 60_000.0 {
            s.started = true;
            let total: f32 = s.startup.iter().sum();
            let slowest = s.startup.iter().copied().fold(0.0, f32::max);
            info!("stress start-up: {} frames, {total:.0} ms, slowest {slowest:.2} ms", s.startup.len());
        }
    } else {
        s.window.push(ms);
        s.all.push(ms);
        if ms > HITCH_MS {
            info!("stress hitch: {ms:.0} ms at t {:.1} s, {new_cells} cells loaded in it ({new_ms:.0} ms), at {}", real.elapsed_secs(), crate::crash::place_text());
        }
    }
    if s.since < s.every {
        return;
    }
    s.since = 0.0;
    let (mean, _, worst) = frame_stats(&s.window);
    let frames = s.window.len();
    s.window.clear();
    let nan = actors.iter().filter(|r| r.nodes.first().and_then(|&e| globals.get(e).ok()).is_some_and(|g| !g.translation().is_finite())).count();
    let (cells, bodies, ragdolls) = phys.as_deref().map_or((0, "0/0/0".to_string(), 0), |p| {
        let st = p.world.stats();
        (p.cells.len(), format!("{}/{}/{}", st.bodies_fixed, st.bodies_kinematic, st.bodies_dynamic), p.world.ragdolls().count())
    });
    info!(
        "stress sample: t {:.1} s, frames {frames}, frame mean {mean:.2} ms, frame worst {worst:.2} ms, entities {}, refs drawn {}, meshes {}, images {}, materials {}, models cached {}, textures cached {}, audio {}, cells {cells}, bodies {bodies}, actors {}, nan actors {nan}, ragdolls {ragdolls}, projectiles {}, at {}",
        real.elapsed_secs(),
        entities.iter().count(),
        refs.iter().count(),
        meshes.len(),
        images.len(),
        materials.len(),
        cached.0.as_deref().map_or(0, nvf_assets::ModelCache::held),
        cached.1.as_deref().map_or(0, nvf_assets::TextureCache::len),
        audio.iter().count(),
        actors.iter().count(),
        combat.as_deref().map_or(0, |c| c.in_flight()),
        crate::crash::place_text(),
    );
}

fn report(mut exit: MessageReader<AppExit>, s: Res<Samples>) {
    if exit.read().next().is_none() {
        return;
    }
    let (mean, low, worst) = frame_stats(&s.all);
    let fps = if low > 0.0 { 1000.0 / low } else { 0.0 };
    info!("stress frames: {} frames, mean {mean:.2} ms, 1% low {low:.2} ms ({fps:.1} fps), worst {worst:.2} ms", s.all.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_stats_take_the_slowest_hundredth_as_the_low() {
        assert_eq!(frame_stats(&[]), (0.0, 0.0, 0.0));
        // 198 frames of 10 ms and two of 50 ms: the slowest 1 % (2 frames) are the 50s.
        let mut ms = vec![10.0; 198];
        ms.extend([50.0, 50.0]);
        let (mean, low, worst) = frame_stats(&ms);
        assert!((mean - 10.4).abs() < 1e-4, "{mean}");
        assert_eq!((low, worst), (50.0, 50.0));
        // Fewer than 100 frames: the worst one.
        assert_eq!(frame_stats(&[5.0, 20.0, 8.0]).1, 20.0);
    }
}
