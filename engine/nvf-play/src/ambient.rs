//! Placed sounds and the cells' background loops (docs/spec-notes/audio.md §5, §6).
//!
//! - **Sound markers** are references whose base is a SOUN. One whose sound has the
//!   Loop flag plays round and round while the player is within the sound's maximum
//!   distance of it, at the loudness [`crate::sfx`] gives a sound at that distance,
//!   followed as the player moves. One whose sound has Play At Random plays a file of
//!   its folder now and then: once a second, when it is not already sounding, it is
//!   rolled against the record's Random Chance [hypothesis: the chance is per second].
//!   Either only between the sound's start and stop hours, and only while the reference
//!   is enabled.
//! - **The background loop** of the cell the player is in: its acoustic space (ASPC)
//!   names a looping sound for dawn, afternoon, dusk and night
//!   (`AcousticSpace::loop_at`); it plays at the listener and gives way to the next
//!   over [`BED_FADE`] seconds [design].
//! - Loops are decoded as they play (WAV or Ogg Vorbis, [`LayeredMusic`] with one file),
//!   so a long one costs no more memory than its file.
//! - The Esc pause menu and a loading screen hold the loops; other menus do not
//!   [design].
//! - Not built: sound markers that neither loop nor play at random (played by scripts
//!   or once), Random Location, Start at Random Position, Mute When Submerged, panning,
//!   an acoustic space's reverb environment, walla and region sounds (REGN `RDSD`).
//!
//! Switched off with the sound effects (`nvforever --no-sound`). An `ambient summary`
//! line at the end.

use crate::music::{cell_at, game_hour, AudioPlaces, LayeredMusic, Space};
use crate::pause::Paused;
use crate::player::Player;
use crate::sfx::{distance_gain, one_shot, resolve, static_gain, PcmSound, Sfx, SfxSettings};
use crate::sim::SimState;
use crate::{Location, PlayState, PlayWorld};
use bevy::audio::AudioSinkPlayback;
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_world::audio;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Seconds one background loop takes to give way to the next.
pub const BED_FADE: f32 = 2.0;

/// A loop stops once the player is this many times its sound's maximum distance away
/// (a little more than where it starts, so standing on the edge does not restart it).
const LEAVE: f32 = 1.05;

/// Whether a sound limited to hours sounds at `hour`: from its start up to its stop,
/// over midnight when the stop is the earlier; equal hours mean all day [observed:
/// 1,150 of the 1,178 placed sounds store 0 and 0].
pub fn in_hours(start: f32, stop: f32, hour: f32) -> bool {
    if (start - stop).abs() < 1e-3 {
        true
    } else if start < stop {
        hour >= start && hour < stop
    } else {
        hour >= start || hour < stop
    }
}

/// A loop being played: its entity and the level it is mixed at.
struct Playing {
    entity: Entity,
    level: Arc<[AtomicU32; 3]>,
}

impl Playing {
    fn set(&self, gain: f32) {
        self.level[0].store(gain.to_bits(), Ordering::Relaxed);
    }
}

/// A background loop.
struct Bed {
    sound: u32,
    /// `None`: its file could not be played.
    playing: Option<Playing>,
    /// How far it is faded in, 0–1, and its gain when fully in.
    level: f32,
    gain: f32,
}

#[derive(Component)]
struct AmbientSound;

#[derive(Resource, Default)]
pub struct Ambient {
    space: Option<Space>,
    /// Looping markers that sound, by reference.
    loops: HashMap<u32, Playing>,
    /// Random markers: the real time until which each still sounds.
    busy_until: HashMap<u32, f64>,
    next_roll: f64,
    /// CELL `XCAS` by cell.
    spaces: HashMap<u32, Option<u32>>,
    /// Loop files by archive path; `None`: not there.
    files: HashMap<String, Option<Arc<[u8]>>>,
    bed: Option<Bed>,
    beds_leaving: Vec<Bed>,
    pub loops_started: u32,
    pub random_played: u32,
    pub beds_started: u32,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Start a SOUN going round, silent; the record's static gain with it.
fn start_loop(commands: &mut Commands, amb: &mut Ambient, sfx: &mut Sfx, vfs: &fnv_vfs::Vfs, assets: &mut Assets<LayeredMusic>, sound: u32) -> Option<(Playing, f32)> {
    let (s, name, path) = resolve(sfx, vfs, FormId(sound))?;
    let bytes = amb
        .files
        .entry(path.clone())
        .or_insert_with(|| match vfs.read(&path) {
            Some(Ok(b)) => Some(b.into()),
            Some(Err(e)) => {
                warn!("ambient: {name}: {path}: {e}");
                None
            }
            None => {
                warn!("ambient: {name}: {path}: not in the archives");
                None
            }
        })
        .clone()?;
    let Some((music, level)) = LayeredMusic::single(bytes, true) else {
        warn!("ambient: {name}: {path}: not decoded");
        amb.files.insert(path, None);
        return None;
    };
    debug!("ambient: {name} {path} loops");
    let speed = (1.0 + s.frequency_adjust_percent / 100.0).clamp(0.25, 4.0);
    let entity = commands.spawn((AudioPlayer::<LayeredMusic>(assets.add(music)), PlaybackSettings::DESPAWN.with_speed(speed), AmbientSound)).id();
    Some((Playing { entity, level }, static_gain(&s)))
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<SfxSettings>,
    mut amb: ResMut<Ambient>,
    places: Option<Res<AudioPlaces>>,
    sfx: Option<ResMut<Sfx>>,
    d: Res<PlayWorld>,
    loc: Res<Location>,
    sim: Res<SimState>,
    state: Res<State<PlayState>>,
    paused: Res<Paused>,
    player: Option<Res<Player>>,
    data: Option<Res<nvf_assets::GameData>>,
    mut pcm: ResMut<Assets<PcmSound>>,
    mut streams: ResMut<Assets<LayeredMusic>>,
    sinks: Query<&AudioSink, With<AmbientSound>>,
) {
    let (Some(places), Some(mut sfx), Some(player), Some(data)) = (places, sfx, player, data) else { return };
    let (amb, sfx) = (&mut *amb, &mut *sfx);
    let held = paused.0 || *state.get() == PlayState::Loading;
    let now = time.elapsed_secs_f64();
    let eye = player.eye();
    let hour = game_hour(&sim);
    let volume = settings.volume.max(0.0);
    let space = Space::of(&loc);
    if amb.space != Some(space) {
        // Another worldspace or interior: nothing of the last one is heard here.
        for (_, p) in amb.loops.drain() {
            commands.entity(p.entity).try_despawn();
        }
        amb.busy_until.clear();
        amb.space = Some(space);
    }
    let roll = !held && now >= amb.next_roll;
    if roll {
        amb.next_roll = now + 1.0;
    }
    let markers = if settings.play { places.sounds.get(&space).map_or(&[][..], Vec::as_slice) } else { &[] };
    let mut sounding: Vec<u32> = Vec::new();
    for m in markers {
        let Some(s) = sfx.forms.sounds.get(&FormId(m.sound)) else { continue };
        let dist = distance(m.position, eye);
        let playing = amb.loops.contains_key(&m.ref_id);
        if s.max_distance <= 0.0 || dist >= s.max_distance * if playing { LEAVE } else { 1.0 } {
            continue;
        }
        if !in_hours(s.start_hour, s.stop_hour, hour) || !sim.world.is_enabled(FormId(m.ref_id)).unwrap_or(!m.initially_disabled) {
            continue;
        }
        if s.has("Loop") {
            let gain = static_gain(s) * distance_gain(s, dist) * volume;
            sounding.push(m.ref_id);
            if let Some(p) = amb.loops.get(&m.ref_id) {
                p.set(gain);
            } else if !held {
                let Some((p, _)) = start_loop(&mut commands, amb, sfx, &data.vfs, &mut streams, m.sound) else { continue };
                p.set(gain);
                amb.loops.insert(m.ref_id, p);
                amb.loops_started += 1;
            }
        } else if s.has("Play At Random") && roll && amb.busy_until.get(&m.ref_id).is_none_or(|&t| now >= t) {
            let chance = s.random_chance_percent / 100.0;
            if sfx.rng.next_f32() < chance {
                if let Some(seconds) = one_shot(&mut commands, sfx, &data.vfs, &mut pcm, &settings, FormId(m.sound), Some(m.position), Some(eye)) {
                    amb.busy_until.insert(m.ref_id, now + seconds as f64);
                    amb.random_played += 1;
                }
            }
        }
    }
    amb.loops.retain(|r, p| {
        let keep = sounding.contains(r);
        if !keep {
            commands.entity(p.entity).try_despawn();
        }
        keep
    });

    // The background loop of the cell.
    let wanted = settings.play.then(|| cell_at(&d, &loc, eye)).flatten().and_then(|c| {
        let space = (*amb.spaces.entry(c).or_insert_with(|| audio::cell_audio(d.lo, d.schema, c).acoustic_space))?;
        places.forms.spaces.get(&space)?.loop_at(hour)
    });
    if amb.bed.as_ref().map(|b| b.sound) != wanted && !held {
        if let Some(old) = amb.bed.take() {
            amb.beds_leaving.push(old);
        }
        if let Some(sound) = wanted {
            let started = start_loop(&mut commands, amb, sfx, &data.vfs, &mut streams, sound);
            amb.beds_started += started.is_some() as u32;
            // A sound without a file is remembered too, so it is not asked for again.
            let gain = started.as_ref().map_or(0.0, |s| s.1);
            amb.bed = Some(Bed { sound, playing: started.map(|s| s.0), level: 0.0, gain });
        }
    }
    let step = time.delta_secs() / BED_FADE;
    if let Some(b) = amb.bed.as_mut() {
        b.level = (b.level + step).min(1.0);
    }
    for b in amb.beds_leaving.iter_mut() {
        b.level -= step;
    }
    for b in amb.bed.iter().chain(amb.beds_leaving.iter()) {
        if let Some(p) = &b.playing {
            p.set(b.level.max(0.0) * b.gain * volume);
        }
    }
    amb.beds_leaving.retain(|b| {
        if let (true, Some(p)) = (b.level <= 0.0, &b.playing) {
            commands.entity(p.entity).try_despawn();
        }
        b.level > 0.0
    });

    for sink in &sinks {
        match (held, sink.is_paused()) {
            (true, false) => sink.pause(),
            (false, true) => sink.play(),
            _ => {}
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, amb: Res<Ambient>) {
    if exit.read().last().is_some() {
        info!("ambient summary: {} placed loops started, {} random sounds played, {} background loops started", amb.loops_started, amb.random_played, amb.beds_started);
    }
}

pub struct AmbientPlugin;

impl Plugin for AmbientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ambient>().add_systems(Update, play.after(crate::PlaySet::Activation)).add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_of_a_sound() {
        // No hours: all day.
        assert!(in_hours(0.0, 0.0, 3.0) && in_hours(20.06, 20.06, 12.0));
        // Night sounds, over midnight.
        assert!(in_hours(21.0, 6.0, 23.0) && in_hours(21.0, 6.0, 2.0));
        assert!(!in_hours(21.0, 6.0, 6.0) && !in_hours(21.0, 6.0, 12.0));
        // Day sounds.
        assert!(in_hours(8.25, 17.34, 12.0) && !in_hours(8.25, 17.34, 18.0) && !in_hours(8.25, 17.34, 8.0));
    }
}
