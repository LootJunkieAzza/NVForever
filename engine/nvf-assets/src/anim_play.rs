//! Plays KF controller sequences by sampling `fnv_nif::anim` tracks every frame and
//! writing the bones' local transforms. Done here rather than with Bevy animation clips
//! so that the original interpolation rules (B-splines, TBC keys, cycle types) stay in
//! the engine-independent format crate (docs/design/layer2.md §2.7).
//!
//! The viewer plays one sequence at a time ([`AnimState`]); world objects carry their
//! own [`Playing`] component, advanced by [`advance_instances`].

use bevy::prelude::*;
use fnv_nif::anim::{Sequence, TrackData, TransformTrack};
use std::collections::HashMap;
use std::sync::Arc;

/// A sequence's transform tracks by target node name, shareable between instances.
#[derive(Debug, Clone)]
pub struct Clip {
    pub name: String,
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub cycle_type: i64,
    pub tracks: Vec<(String, Arc<TransformTrack>)>,
    /// Each track's controlled-block priority (nif.xml ControlledBlock "Priority"; 0 when
    /// absent), in `tracks` order.
    pub priorities: Vec<i32>,
    /// Tracks without a target name or with an interpolator that is not a transform.
    pub skipped: usize,
    /// The sequence's accumulation root (nif.xml NiControllerSequence "Accum Root Name").
    pub accum_root: Option<String>,
    /// Text keys (time, text).
    pub text_keys: Vec<(f32, String)>,
}

impl Clip {
    pub fn from_sequence(seq: &Sequence) -> Clip {
        let mut tracks = Vec::new();
        let mut priorities = Vec::new();
        let mut skipped = 0;
        for t in &seq.tracks {
            match (&t.node_name, &t.data) {
                (Some(n), TrackData::Transform(tt)) => {
                    tracks.push((n.clone(), Arc::new(tt.clone())));
                    priorities.push(t.priority.unwrap_or(0) as i32);
                }
                _ => skipped += 1,
            }
        }
        Clip {
            name: seq.name.clone().unwrap_or_default(),
            start: seq.start_time,
            stop: seq.stop_time,
            frequency: if seq.frequency > 0.0 { seq.frequency } else { 1.0 },
            cycle_type: seq.cycle_type,
            tracks,
            priorities,
            skipped,
            accum_root: seq.accum_root_name.clone().filter(|n| !n.is_empty()),
            text_keys: seq.text_keys.clone(),
        }
    }

    pub fn length(&self) -> f32 {
        (self.stop - self.start).max(0.0)
    }

    /// The speed the clip's accumulation root moves at over one cycle (units per second
    /// at frequency 1), from its track's translation at the start and stop times: the
    /// ground speed the clip was made for [observed: `locomotion\male\mtforward.kf`
    /// moves `Bip01` 102.3 units in 1.2 s, docs/spec-notes/animation.md §8.2]. `None`
    /// when the root has no track or the clip no length.
    pub fn root_speed(&self) -> Option<f32> {
        let root = self.accum_root.as_deref()?;
        let (_, t) = self.tracks.iter().find(|(n, _)| n == root)?;
        let (a, b) = (t.translation.sample(self.start)?, t.translation.sample(self.stop)?);
        let len = self.length();
        (len > 0.0).then(|| Vec2::new(b[0] - a[0], b[1] - a[1]).length() / len)
    }

    /// The accumulation root when no track drives it. Such a clip carries the root's
    /// whole bind offset on the node below it (`Bip01 NonAccum` under `Bip01` in
    /// `mtidle.kf` [observed, docs/spec-notes/actors.md §7]), so the root itself must be
    /// held at the identity while it plays [hypothesis: the accumulated motion of a clip
    /// without a root track is none].
    pub fn undriven_accum_root(&self) -> Option<&str> {
        self.accum_root.as_deref().filter(|r| !self.tracks.iter().any(|(n, _)| n == r))
    }
}

/// A clip playing on a set of entities.
#[derive(Component)]
pub struct Playing {
    pub name: String,
    pub source: String,
    pub start: f32,
    pub stop: f32,
    pub frequency: f32,
    pub cycle_type: i64,
    pub time: f32,
    pub tracks: Vec<(Entity, Arc<TransformTrack>)>,
    /// Tracks whose target node was not found.
    pub unmatched: usize,
}

impl Playing {
    /// Start `clip` from its start time, binding tracks to entities by node name.
    pub fn new(source: &str, clip: &Clip, node: impl Fn(&str) -> Option<Entity>) -> Playing {
        let mut tracks = Vec::new();
        let mut unmatched = 0;
        for (n, tt) in &clip.tracks {
            match node(n) {
                Some(e) => tracks.push((e, tt.clone())),
                None => unmatched += 1,
            }
        }
        Playing {
            name: clip.name.clone(),
            source: source.to_string(),
            start: clip.start,
            stop: clip.stop,
            frequency: clip.frequency,
            cycle_type: clip.cycle_type,
            time: clip.start,
            tracks,
            unmatched,
        }
    }

    /// The time within the sequence after the cycle rule.
    pub fn sequence_time(&self) -> f32 {
        cycle_time(self.time, self.start, self.stop, self.cycle_type)
    }

    /// Advance by `dt` seconds (unless `paused`) and pose the targets.
    pub fn step(&mut self, dt: f32, paused: bool, transforms: &mut Query<&mut Transform>) {
        if !paused {
            self.time += dt * self.frequency;
        }
        let t = self.sequence_time();
        for (e, track) in &self.tracks {
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
}

/// Every sequence that can be played on the current mesh, and the one playing.
#[derive(Resource, Default)]
pub struct AnimState {
    pub available: Vec<(String, Sequence)>,
    pub current: Option<usize>,
    pub playing: Option<Playing>,
    pub paused: bool,
    /// The accumulation root the current sequence holds at the identity
    /// ([`Clip::undriven_accum_root`]), with its bind transform to put back.
    pub held: Option<(Entity, Transform)>,
    /// Transforms [`advance`] writes before posing (holding or releasing that root).
    pending: Vec<(Entity, Transform)>,
}

impl AnimState {
    /// Play sequence `index` on the nodes. `bind` gives the nodes' local bind transforms,
    /// for the accumulation root a previous sequence held.
    pub fn start(&mut self, index: usize, nodes: &HashMap<String, Entity>, bind: &HashMap<Entity, Transform>) {
        let Some((source, seq)) = self.available.get(index) else { return };
        let clip = Clip::from_sequence(seq);
        let playing = Playing::new(source, &clip, |n| nodes.get(n).copied());
        let hold = clip.undriven_accum_root().and_then(|r| nodes.get(r)).copied();
        self.current = Some(index);
        self.playing = Some(playing);
        self.release();
        if let Some(e) = hold {
            self.pending.push((e, Transform::IDENTITY));
            self.held = Some((e, bind.get(&e).copied().unwrap_or_default()));
        }
    }

    /// Stop playing; a held accumulation root goes back to its bind transform.
    pub fn stop(&mut self) {
        self.current = None;
        self.playing = None;
        self.release();
    }

    fn release(&mut self) {
        if let Some(back) = self.held.take() {
            self.pending.push(back);
        }
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
    for (e, t) in std::mem::take(&mut state.pending) {
        if let Ok(mut tr) = transforms.get_mut(e) {
            *tr = t;
        }
    }
    let paused = state.paused;
    if let Some(p) = state.playing.as_mut() {
        p.step(time.delta_secs(), paused, &mut transforms);
    }
}

/// Advance every [`Playing`] component (world objects with their own animation).
pub fn advance_instances(time: Res<Time>, mut playing: Query<&mut Playing>, mut transforms: Query<&mut Transform>) {
    let dt = time.delta_secs();
    for mut p in &mut playing {
        p.step(dt, false, &mut transforms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Track};
    use std::f32::consts::FRAC_PI_2;

    fn seq(tracks: &[&str]) -> Sequence {
        let track = |n: &&str| Track {
            node_name: Some(n.to_string()),
            controller_type: None,
            priority: None,
            data: TrackData::Transform(TransformTrack {
                translation: Channel::Static(Some([0.0, -0.296, 66.33])),
                rotation: Channel::Static(None),
                euler: None,
                scale: Channel::Static(None),
            }),
        };
        Sequence {
            block: 0,
            name: None,
            start_time: 0.0,
            stop_time: 1.0,
            frequency: 1.0,
            cycle_type: 0,
            accum_root_name: Some("Bip01".into()),
            text_keys: Vec::new(),
            tracks: tracks.iter().map(track).collect(),
        }
    }

    #[test]
    fn undriven_accum_root_is_held_then_restored() {
        let (root, non_accum) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let nodes = HashMap::from([("Bip01".to_string(), root), ("Bip01 NonAccum".to_string(), non_accum)]);
        let bind_root = Transform::from_xyz(0.0, 0.657, 67.771).with_rotation(Quat::from_rotation_z(FRAC_PI_2));
        let bind = HashMap::from([(root, bind_root), (non_accum, Transform::IDENTITY)]);
        let mut state = AnimState {
            available: vec![
                ("idle".into(), seq(&["Bip01 NonAccum"])),
                ("walk".into(), seq(&["Bip01", "Bip01 NonAccum"])),
            ],
            ..Default::default()
        };
        // A clip without a Bip01 track holds Bip01 at the identity.
        state.start(0, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, Transform::IDENTITY)]);
        // Switching to the same clip keeps holding it.
        state.start(0, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, bind_root), (root, Transform::IDENTITY)]);
        // A clip that drives Bip01 gets it back at its bind pose first.
        state.start(1, &nodes, &bind);
        assert_eq!(std::mem::take(&mut state.pending), [(root, bind_root)]);
        assert_eq!(state.held, None);
        // Stopping releases a held root.
        state.start(0, &nodes, &bind);
        state.pending.clear();
        state.stop();
        assert_eq!(state.pending, [(root, bind_root)]);
        assert!(state.playing.is_none() && state.current.is_none());
    }

    #[test]
    fn cycles() {
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 0), 0.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 1), 1.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 2), 2.0);
        assert_eq!(cycle_time(1.0, 1.0, 1.0, 0), 1.0);
    }
}
