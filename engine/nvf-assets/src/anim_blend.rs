//! Several KF clips at once on one actor skeleton, combined bone by bone by track
//! priority (docs/spec-notes/animation.md §9.4). Used for an actor under direct control
//! (the player): an idle under everything, movement clips over it, a weapon stance over
//! the arms, draw and holster clips over all of them.
//!
//! Rule [hypothesis: nif.xml names the blend interpolator's fields "High Priority",
//! "Next High Priority", "High Weights Sum" and "Next High Weights Sum", which reads as
//! priority-first blending; the details are not documented]: for each bone, the tracks of
//! the playing clips are taken from the highest priority down. Tracks of equal priority
//! share by their clips' weights; a priority group takes at most the weight still free
//! (starting at 1), and lower groups get what is left; the bind pose fills any rest.
//! Clips fade in and out by moving their weight, so a change of clip at the same
//! priority cross-fades.
//!
//! The actor's root node is its placement and is never posed; the clips' accumulation
//! root is held at the identity: its motion is the actor's movement, which the
//! character controller provides (actors.md §7.2 for idles).
//!
//! No allocation per frame once the clips are bound: bindings are made when a clip
//! starts, and the per-frame work reuses the animator's buffers.

use crate::actor::{clip_target, PreparedSkeleton};
use crate::anim_play::{cycle_time, Clip};
use bevy::math::Affine3A;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::sync::Arc;

/// Fade-out time and fade-in time for clips without a `Blend` key, seconds
/// [engineering choice].
pub const DEFAULT_FADE: f32 = 0.2;

/// How a layer's time runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// By the clip's own cycle type, at `rate`.
    Loop,
    /// Once from the start, holding the last pose.
    Once,
    /// On the animator's shared phase (movement clips of different lengths stay in step).
    Synced,
}

/// One clip playing on the skeleton.
#[derive(Debug, Clone)]
pub struct Layer {
    pub clip: Arc<Clip>,
    /// The caller's name for the role the clip plays (two roles may play one clip).
    pub tag: u32,
    pub mode: Mode,
    /// (skeleton node, track index, priority) for every track that drives a node.
    bound: Vec<(u16, u16, i32)>,
    /// Clip time since the clip's start (Loop and Once): seconds × rate × frequency.
    pub time: f32,
    pub weight: f32,
    pub target: f32,
    /// Seconds to fade in (from weight 0 to 1); fading out takes [`DEFAULT_FADE`].
    pub fade_in: f32,
    /// Playback speed multiplier (Loop and Once).
    pub rate: f32,
    seen: bool,
}

impl Layer {
    /// The time within the clip the layer samples, given the animator's phase.
    pub fn sample_time(&self, phase: f32) -> f32 {
        let c = &self.clip;
        match self.mode {
            Mode::Synced => c.start + phase.rem_euclid(1.0) * c.length(),
            Mode::Once => cycle_time(c.start + self.time, c.start, c.stop, 2),
            Mode::Loop => cycle_time(c.start + self.time, c.start, c.stop, c.cycle_type),
        }
    }

    /// Whether a Once layer has reached the clip's end.
    pub fn finished(&self) -> bool {
        self.time >= self.clip.length()
    }
}

#[derive(Debug, Clone, Copy)]
struct Contribution {
    bone: u16,
    priority: i32,
    weight: f32,
    layer: u16,
    track: u16,
}

/// Layered clips on a spawned actor's skeleton ([`crate::actor::spawn_actor`]).
#[derive(Component)]
pub struct Animator {
    pub skel: Arc<PreparedSkeleton>,
    /// The skeleton's node entities (`nodes[0]` is the actor's root).
    pub nodes: Vec<Entity>,
    pub layers: Vec<Layer>,
    /// Shared phase (0–1) of the [`Mode::Synced`] layers.
    pub phase: f32,
    /// The pose of the last [`Animator::pose`]: local transforms in NIF space.
    pub locals: Vec<Transform>,
    /// Nodes held at the identity (the accumulation root).
    held: Vec<usize>,
    scratch: Vec<Contribution>,
}

impl Animator {
    /// An animator on a spawned skeleton; `accum_root` (normally `Bip01`) is held at the
    /// identity.
    pub fn new(skel: Arc<PreparedSkeleton>, nodes: Vec<Entity>, accum_root: Option<&str>) -> Animator {
        let held = accum_root.and_then(|r| clip_target(&skel, r)).into_iter().collect();
        let locals = skel.locals.clone();
        Animator { skel, nodes, layers: Vec::new(), phase: 0.0, locals, held, scratch: Vec::new() }
    }

    /// Start of a frame's requests: every layer not asked for again by [`Self::end`] fades
    /// out.
    pub fn begin(&mut self) {
        for l in &mut self.layers {
            l.seen = false;
        }
    }

    /// Keep `clip` playing in role `tag` at `weight` (starting it, faded in, if it is not
    /// playing in that role). Returns the layer's index.
    pub fn want(&mut self, clip: &Arc<Clip>, tag: u32, weight: f32, mode: Mode, fade_in: f32) -> usize {
        if let Some(i) = self.find(clip, tag) {
            let l = &mut self.layers[i];
            (l.target, l.seen, l.mode, l.fade_in) = (weight, true, mode, fade_in);
            return i;
        }
        self.push(clip, tag, weight, mode, fade_in)
    }

    /// Play `clip` in role `tag` from its start, replacing any layer of that role.
    pub fn restart(&mut self, clip: &Arc<Clip>, tag: u32, weight: f32, mode: Mode, fade_in: f32) -> usize {
        self.layers.retain(|l| l.tag != tag);
        self.push(clip, tag, weight, mode, fade_in)
    }

    /// Drop every layer of role `tag` at once (no fade).
    pub fn cut(&mut self, tag: u32) {
        self.layers.retain(|l| l.tag != tag);
    }

    fn find(&self, clip: &Arc<Clip>, tag: u32) -> Option<usize> {
        self.layers.iter().position(|l| l.tag == tag && Arc::ptr_eq(&l.clip, clip))
    }

    fn push(&mut self, clip: &Arc<Clip>, tag: u32, weight: f32, mode: Mode, fade_in: f32) -> usize {
        let skel = &self.skel;
        let bound = clip
            .tracks
            .iter()
            .zip(&clip.priorities)
            .enumerate()
            .filter_map(|(ti, ((name, _), &p))| {
                let b = clip_target(skel, name)?;
                (!self.held.contains(&b)).then_some((b as u16, ti as u16, p))
            })
            .collect();
        let start = if fade_in <= 0.0 { weight } else { 0.0 };
        self.layers.push(Layer {
            clip: clip.clone(),
            tag,
            mode,
            bound,
            time: 0.0,
            weight: start,
            target: weight,
            fade_in,
            rate: 1.0,
            seen: true,
        });
        self.layers.len() - 1
    }

    /// End of a frame's requests (see [`Self::begin`]).
    pub fn end(&mut self) {
        for l in &mut self.layers {
            if !l.seen {
                l.target = 0.0;
            }
        }
    }

    /// The layer playing `clip` in role `tag`.
    pub fn layer(&self, clip: &Arc<Clip>, tag: u32) -> Option<&Layer> {
        self.find(clip, tag).map(|i| &self.layers[i])
    }

    /// Advance weights and times by `dt` seconds; synced layers move their shared phase
    /// at `sync_rate` times their own speed (weighted by their weights). Layers faded
    /// out are dropped.
    pub fn advance(&mut self, dt: f32, sync_rate: f32) {
        let (mut len_sum, mut w_sum) = (0.0, 0.0);
        for l in &mut self.layers {
            let step = if l.target > l.weight {
                if l.fade_in <= 0.0 { f32::INFINITY } else { dt / l.fade_in }
            } else {
                dt / DEFAULT_FADE
            };
            l.weight = if l.target > l.weight { (l.weight + step).min(l.target) } else { (l.weight - step).max(l.target) };
            match l.mode {
                Mode::Synced => {
                    len_sum += l.weight * l.clip.length() / l.clip.frequency;
                    w_sum += l.weight;
                }
                _ => l.time += dt * l.rate * l.clip.frequency,
            }
        }
        self.layers.retain(|l| l.weight > 0.0 || l.target > 0.0);
        if w_sum > 1e-6 && len_sum > 1e-6 {
            self.phase = (self.phase + dt * sync_rate * w_sum / len_sum).rem_euclid(1.0);
        }
    }

    /// Blend the layers into [`Self::locals`].
    pub fn pose(&mut self) {
        let skel = &self.skel;
        self.scratch.clear();
        for (li, l) in self.layers.iter().enumerate() {
            if l.weight <= 0.0 {
                continue;
            }
            for &(bone, track, priority) in &l.bound {
                self.scratch.push(Contribution { bone, priority, weight: l.weight, layer: li as u16, track });
            }
        }
        self.scratch.sort_unstable_by_key(|c| (c.bone, Reverse(c.priority)));
        // Bones without tracks: the bind pose; held nodes: the identity.
        self.locals.clone_from(&skel.locals);
        for &h in &self.held {
            self.locals[h] = Transform::IDENTITY;
        }
        let phase = self.phase;
        let mut i = 0;
        while i < self.scratch.len() {
            let bone = self.scratch[i].bone as usize;
            let bind = skel.locals[bone];
            let (mut t, mut q, mut s) = (Vec3::ZERO, Vec4::ZERO, 0.0f32);
            let mut first: Option<Vec4> = None;
            let mut free = 1.0f32;
            while i < self.scratch.len() && self.scratch[i].bone as usize == bone {
                // One priority group.
                let p = self.scratch[i].priority;
                let g0 = i;
                let mut sum = 0.0;
                while i < self.scratch.len() && self.scratch[i].bone as usize == bone && self.scratch[i].priority == p {
                    sum += self.scratch[i].weight;
                    i += 1;
                }
                if free <= 1e-6 || sum <= 0.0 {
                    continue;
                }
                let take = sum.min(free);
                free -= take;
                for c in &self.scratch[g0..i] {
                    let f = c.weight / sum * take;
                    let l = &self.layers[c.layer as usize];
                    let track = &l.clip.tracks[c.track as usize].1;
                    let time = l.sample_time(phase);
                    t += f * track.translation.sample(time).map_or(bind.translation, Vec3::from_array);
                    let r = track.rotation_at(time).map_or(bind.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w));
                    let mut v = Vec4::from(r);
                    let reference = *first.get_or_insert(v);
                    if v.dot(reference) < 0.0 {
                        v = -v;
                    }
                    q += f * v;
                    s += f * track.scale.sample(time).map_or(bind.scale.x, |[x]| x);
                }
            }
            if free > 1e-6 {
                t += free * bind.translation;
                let mut v = Vec4::from(bind.rotation);
                if first.is_some_and(|r| v.dot(r) < 0.0) {
                    v = -v;
                }
                q += free * v;
                s += free * bind.scale.x;
            }
            let rotation = if q.length_squared() > 1e-12 { Quat::from_vec4(q).normalize() } else { bind.rotation };
            self.locals[bone] = Transform { translation: t, rotation, scale: Vec3::splat(s) };
        }
    }

    /// Write the pose to the skeleton's node entities (the root excluded).
    pub fn apply(&self, transforms: &mut Query<&mut Transform>) {
        for (i, e) in self.nodes.iter().enumerate().skip(1) {
            if let Ok(mut tr) = transforms.get_mut(*e) {
                if *tr != self.locals[i] {
                    *tr = self.locals[i];
                }
            }
        }
    }

    /// Node `i`'s transform relative to the actor's root in the current pose (along the
    /// skeleton's own hierarchy).
    pub fn model_space(&self, i: usize) -> Affine3A {
        let mut m = Affine3A::IDENTITY;
        let mut cur = Some(i);
        let mut depth = 0;
        while let Some(n) = cur.filter(|&n| n > 0) {
            let l = self.locals[n];
            m = Affine3A::from_scale_rotation_translation(l.scale, l.rotation, l.translation) * m;
            cur = self.skel.parents[n];
            depth += 1;
            if depth > 256 {
                break;
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, TransformTrack};
    use std::collections::HashMap;

    /// Root, Bip01 (accumulation root), Bone A, Bone B.
    fn skeleton() -> Arc<PreparedSkeleton> {
        let names = ["Scene Root", "Bip01", "A", "B"];
        let locals = vec![Transform::IDENTITY, Transform::from_xyz(0.0, 0.0, 60.0), Transform::from_xyz(1.0, 0.0, 0.0), Transform::from_xyz(0.0, 2.0, 0.0)];
        Arc::new(PreparedSkeleton {
            names: names.iter().map(|n| Some(n.to_string())).collect(),
            parents: vec![None, Some(0), Some(1), Some(2)],
            locals,
            bind_world: vec![Affine3A::IDENTITY; 4],
            by_name: names.iter().enumerate().map(|(i, n)| (n.to_string(), i)).collect::<HashMap<_, _>>(),
        })
    }

    /// A clip moving each named node to a fixed translation, at a priority.
    fn clip(tracks: &[(&str, [f32; 3], i32)], len: f32) -> Arc<Clip> {
        Arc::new(Clip {
            name: "c".into(),
            start: 0.0,
            stop: len,
            frequency: 1.0,
            cycle_type: 0,
            tracks: tracks
                .iter()
                .map(|(n, t, _)| {
                    let tt = TransformTrack { translation: Channel::Static(Some(*t)), rotation: Channel::Static(None), euler: None, scale: Channel::Static(None) };
                    (n.to_string(), Arc::new(tt))
                })
                .collect(),
            priorities: tracks.iter().map(|t| t.2).collect(),
            skipped: 0,
            accum_root: Some("Bip01".into()),
            text_keys: Vec::new(),
        })
    }

    fn animator() -> Animator {
        let nodes = (1..=4).map(|i| Entity::from_raw_u32(i).unwrap()).collect();
        Animator::new(skeleton(), nodes, Some("Bip01"))
    }

    #[test]
    fn higher_priority_wins_and_lower_fills_the_rest() {
        let mut a = animator();
        let base = clip(&[("A", [10.0, 0.0, 0.0], 10), ("B", [0.0, 10.0, 0.0], 10), ("Bip01", [5.0, 5.0, 5.0], 10)], 1.0);
        let top = clip(&[("A", [20.0, 0.0, 0.0], 30)], 1.0);
        a.want(&base, 0, 1.0, Mode::Loop, 0.0);
        a.want(&top, 1, 1.0, Mode::Loop, 0.0);
        a.pose();
        assert_eq!(a.locals[2].translation, Vec3::new(20.0, 0.0, 0.0), "the higher priority takes the bone");
        assert_eq!(a.locals[3].translation, Vec3::new(0.0, 10.0, 0.0), "bones it does not drive keep the base");
        assert_eq!(a.locals[1], Transform::IDENTITY, "the accumulation root is held");
        // Half weight on top: the base fills the other half.
        a.layers[1].weight = 0.5;
        a.pose();
        assert!((a.locals[2].translation.x - 15.0).abs() < 1e-5);
        // Nothing on a bone at all: its bind pose.
        a.layers.clear();
        a.pose();
        assert_eq!(a.locals[2].translation, Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn equal_priorities_share_by_weight_and_fade() {
        let mut a = animator();
        let x = clip(&[("A", [10.0, 0.0, 0.0], 30)], 1.0);
        let y = clip(&[("A", [0.0, 10.0, 0.0], 30)], 1.0);
        a.begin();
        a.want(&x, 0, 1.0, Mode::Loop, 0.0);
        a.end();
        a.advance(0.1, 1.0);
        // Switch to y with a 0.2 s fade in; x fades out over DEFAULT_FADE.
        a.begin();
        a.want(&y, 0, 1.0, Mode::Loop, 0.2);
        a.end();
        a.advance(0.1, 1.0);
        a.pose();
        let t = a.locals[2].translation;
        assert!((t.x - 5.0).abs() < 1e-4 && (t.y - 5.0).abs() < 1e-4, "{t}");
        a.begin();
        a.want(&y, 0, 1.0, Mode::Loop, 0.2);
        a.end();
        a.advance(0.2, 1.0);
        assert_eq!(a.layers.len(), 1, "a faded-out layer is dropped");
        a.pose();
        assert_eq!(a.locals[2].translation, Vec3::new(0.0, 10.0, 0.0));
    }

    #[test]
    fn synced_layers_share_a_phase() {
        let mut a = animator();
        let long = clip(&[("A", [1.0, 0.0, 0.0], 30)], 2.0);
        let short = clip(&[("B", [1.0, 0.0, 0.0], 30)], 1.0);
        a.want(&long, 0, 0.5, Mode::Synced, 0.0);
        a.want(&short, 1, 0.5, Mode::Synced, 0.0);
        // Weighted length 1.5 s: a quarter of it moves the phase by a quarter.
        a.advance(0.375, 1.0);
        assert!((a.phase - 0.25).abs() < 1e-5, "{}", a.phase);
        assert!((a.layers[0].sample_time(a.phase) - 0.5).abs() < 1e-5);
        assert!((a.layers[1].sample_time(a.phase) - 0.25).abs() < 1e-5);
        // Twice the rate, twice the phase step.
        a.advance(0.375, 2.0);
        assert!((a.phase - 0.75).abs() < 1e-5, "{}", a.phase);
    }

    #[test]
    fn once_holds_its_last_pose_and_restart_rewinds() {
        let mut a = animator();
        let c = clip(&[("A", [1.0, 0.0, 0.0], 55)], 0.4);
        let i = a.restart(&c, 7, 1.0, Mode::Once, 0.0);
        a.advance(0.5, 1.0);
        assert!(a.layers[i].finished());
        assert_eq!(a.layers[i].sample_time(0.0), 0.4);
        let i = a.restart(&c, 7, 1.0, Mode::Once, 0.0);
        assert_eq!((a.layers.len(), a.layers[i].time), (1, 0.0));
        a.cut(7);
        assert!(a.layers.is_empty());
    }

    #[test]
    fn model_space_follows_the_pose() {
        let mut a = animator();
        a.pose();
        // Bip01 held at the identity: B is at A (1, 0, 0) + (0, 2, 0).
        let m = a.model_space(3);
        assert!((m.translation - Vec3A::new(1.0, 2.0, 0.0)).length() < 1e-5);
    }
}
