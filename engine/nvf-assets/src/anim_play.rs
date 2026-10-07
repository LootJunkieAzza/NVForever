//! Plays KF controller sequences by sampling `fnv_nif::anim` tracks every frame and
//! writing the bones' local transforms. Done here rather than with Bevy animation clips
//! so that the original interpolation rules (B-splines, TBC keys, cycle types) stay in
//! the engine-independent format crate (docs/design/layer2.md §2.7).

use bevy::prelude::*;
use fnv_nif::anim::{Sequence, TrackData, TransformTrack};
use std::collections::HashMap;

pub struct Playing {
    pub name: String,
    pub source: String,
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub cycle_type: i64,
    pub time: f32,
    pub tracks: Vec<(Entity, TransformTrack)>,
    pub unmatched: usize,
}

/// Every sequence that can be played on the current mesh, and the one playing.
#[derive(Resource, Default)]
pub struct AnimState {
    pub available: Vec<(String, Sequence)>,
    pub current: Option<usize>,
    pub playing: Option<Playing>,
    pub paused: bool,
}

impl AnimState {
    pub fn start(&mut self, index: usize, nodes: &HashMap<String, Entity>) {
        let Some((source, seq)) = self.available.get(index) else { return };
        let mut tracks = Vec::new();
        let mut unmatched = 0;
        for t in &seq.tracks {
            let TrackData::Transform(tt) = &t.data else { continue };
            match t.node_name.as_ref().and_then(|n| nodes.get(n)) {
                Some(e) => tracks.push((*e, tt.clone())),
                None => unmatched += 1,
            }
        }
        self.current = Some(index);
        self.playing = Some(Playing {
            name: seq.name.clone().unwrap_or_default(),
            source: source.clone(),
            start: seq.start_time,
            stop: seq.stop_time,
            frequency: if seq.frequency > 0.0 { seq.frequency } else { 1.0 },
            cycle_type: seq.cycle_type,
            time: seq.start_time,
            tracks,
            unmatched,
        });
    }
}

/// Map a running time onto the sequence according to nif.xml CycleType
/// (0 loop, 1 reverse = ping-pong, 2 clamp) [documented: nif.xml CycleType].
pub fn cycle_time(t: f32, start: f32, stop: f32, cycle: i64) -> f32 {
    let len = stop - start;
    if len <= 0.0 {
        return start;
    }
    let x = t - start;
    match cycle {
        2 => start + x.clamp(0.0, len),
        1 => {
            let m = x.rem_euclid(2.0 * len);
            start + if m > len { 2.0 * len - m } else { m }
        }
        _ => start + x.rem_euclid(len),
    }
}

pub fn advance(time: Res<Time>, mut state: ResMut<AnimState>, mut transforms: Query<&mut Transform>) {
    let paused = state.paused;
    let Some(p) = state.playing.as_mut() else { return };
    if !paused {
        p.time += time.delta_secs() * p.frequency;
    }
    let t = cycle_time(p.time, p.start, p.stop, p.cycle_type);
    for (e, track) in &p.tracks {
        let Ok(mut tr) = transforms.get_mut(*e) else { continue };
        if let Some(v) = track.translation.sample(t) {
            tr.translation = Vec3::from_array(v);
        }
        if let Some([w, x, y, z]) = track.rotation_at(t) {
            tr.rotation = Quat::from_xyzw(x, y, z, w).normalize();
        }
        if let Some([s]) = track.scale.sample(t) {
            tr.scale = Vec3::splat(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cycle_time;

    #[test]
    fn cycles() {
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 0), 0.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 1), 1.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 2), 2.0);
        assert_eq!(cycle_time(1.0, 1.0, 1.0, 0), 1.0);
    }
}
