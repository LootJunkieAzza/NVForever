//! Several KF clips at once on one actor skeleton, combined bone by bone by track
//! priority (docs/spec-notes/animation.md §9.4). Used for an actor under direct control
//! (the player): an idle under everything, movement clips over it, a weapon stance over
//! the arms, draw and holster clips over all of them.
//!
//! Rule [hypothesis: nif.xml names the blend interpolator's fields "High Priority",
//! "Next High Priority", "High Weights Sum" and "Next High Weights Sum", which reads as
//! priority-first blending; the details are not documented]: for each bone, the tracks of
//! the playing clips are taken from the highest priority down. Tracks of equal priority
//! share by their clips' weights, whatever those add up to: a weight says how a clip
//! shares a bone with the others at its priority, not how much of the bone the priority
//! takes. How much it takes is the clips' fade (their ease, 0 to 1): a priority group
//! takes its clips' eases added up, at most what is still free (starting at 1); lower
//! groups get what is left and the bind pose fills any rest. So a clip that is alone at
//! its priority on a bone has all of that bone once faded in, even at a small weight
//! [observed: the rifles' `aimup` and `aimdown` clips have no arm tracks, so the arms
//! must stay with `aim` while its weight drops; animation.md §9.6]. Clips fade in and out
//! by their ease, so a change of clip at the same priority cross-fades.
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
    /// (node, track index, priority) for every track that drives a node: a skeleton node,
    /// or an extra node counted on from the skeleton's last ([`Animator::set_extra`]).
    bound: Vec<(u16, u16, i32)>,
    /// Clip time since the clip's start (Loop and Once): seconds × rate × frequency.
    pub time: f32,
    /// The clip's share among the tracks of its priority on a bone; follows `target`
    /// while the layer is asked for and stays as it was while the layer fades out.
    pub weight: f32,
    pub target: f32,
    /// The fade, 0 to 1: how much of a bone the layer's priority takes from those below.
    pub ease: f32,
    /// Seconds to fade in (ease 0 to 1); fading out takes [`DEFAULT_FADE`].
    pub fade_in: f32,
    /// Playback speed multiplier (Loop and Once).
    pub rate: f32,
    seen: bool,
    /// Not asked for at the last [`Animator::end`]: fading out.
    leaving: bool,
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
    /// The layer's weight × ease: its share within its priority group.
    weight: f32,
    ease: f32,
    layer: u16,
    track: u16,
}

/// A node outside the skeleton that clips drive by name: a part of the weapon model
/// the actor holds (`##Slide`, `##Clip`; docs/spec-notes/animation.md §9.8).
#[derive(Debug, Clone)]
pub struct ExtraNode {
    pub name: String,
    pub entity: Entity,
    /// Its transform in the model: what it takes where no clip drives it.
    pub bind: Transform,
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
    /// The pose of the last [`Animator::pose`]: local transforms in NIF space, the
    /// skeleton's nodes first, then the extra nodes.
    pub locals: Vec<Transform>,
    /// Nodes outside the skeleton ([`Self::set_extra`]).
    extra: Vec<ExtraNode>,
    /// Nodes held at the identity (the accumulation root).
    held: Vec<usize>,
    /// Nodes with a rest pose of their own in place of the bind pose ([`Self::set_rest`]).
    rest: Vec<(usize, Transform)>,
    scratch: Vec<Contribution>,
}

impl Animator {
    /// An animator on a spawned skeleton; `accum_root` (normally `Bip01`) is held at the
    /// identity.
    pub fn new(skel: Arc<PreparedSkeleton>, nodes: Vec<Entity>, accum_root: Option<&str>) -> Animator {
        let held = accum_root.and_then(|r| clip_target(&skel, r)).into_iter().collect();
        let locals = skel.locals.clone();
        Animator { skel, nodes, layers: Vec::new(), phase: 0.0, locals, extra: Vec::new(), held, rest: Vec::new(), scratch: Vec::new() }
    }

    /// Replace the nodes outside the skeleton that clips drive by name (none: an empty
    /// list). A name the skeleton has stays the skeleton's, and the first of equal names
    /// counts. The clips playing are bound again, so a weapon that arrives half way
    /// through a clip moves with it.
    pub fn set_extra(&mut self, extra: Vec<ExtraNode>) {
        self.extra = extra;
        // The last pose's extra nodes were the old list's.
        self.locals.truncate(self.skel.locals.len());
        for i in 0..self.layers.len() {
            let bound = self.bind(&self.layers[i].clip);
            self.layers[i].bound = bound;
        }
    }

    /// The node a track named `name` drives: a skeleton node (not the root), else an
    /// extra node, numbered after the skeleton's.
    fn target(&self, name: &str) -> Option<usize> {
        if self.skel.by_name.contains_key(name) {
            return clip_target(&self.skel, name);
        }
        self.extra.iter().position(|x| x.name == name).map(|k| self.skel.locals.len() + k)
    }

    /// `clip`'s tracks bound to nodes: (node, track index, priority).
    fn bind(&self, clip: &Clip) -> Vec<(u16, u16, i32)> {
        clip.tracks
            .iter()
            .zip(&clip.priorities)
            .enumerate()
            .filter_map(|(ti, ((name, _), &p))| {
                let b = self.target(name)?;
                (!self.held.contains(&b)).then_some((b as u16, ti as u16, p))
            })
            .collect()
    }

    /// The names of the extra nodes `clip` has a transform track for (for reports).
    pub fn extra_driven<'a>(&'a self, clip: &Clip) -> Vec<&'a str> {
        let n = self.skel.locals.len();
        let mut out: Vec<&str> = clip.tracks.iter().filter_map(|(name, _)| self.target(name)?.checked_sub(n)).map(|k| self.extra[k].name.as_str()).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Give `node` a rest pose in place of its bind pose (`None`: the bind pose again):
    /// what it takes where no clip drives it, and what fills the part of it that the
    /// clips leave free.
    pub fn set_rest(&mut self, node: usize, pose: Option<Transform>) {
        self.rest.retain(|(n, _)| *n != node);
        if let Some(p) = pose {
            self.rest.push((node, p));
        }
    }

    /// Start of a frame's requests: every layer not asked for again by [`Self::end`] fades
    /// out.
    pub fn begin(&mut self) {
        for l in &mut self.layers {
            l.seen = false;
        }
    }

    /// Keep `clip` playing in role `tag` at `weight` (starting it, faded in, if it is not
    /// playing in that role). A weight of 0 keeps the clip on the bones it has alone at
    /// its priority. Returns the layer's index.
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
        let bound = self.bind(clip);
        let ease = if fade_in <= 0.0 { 1.0 } else { 0.0 };
        self.layers.push(Layer {
            clip: clip.clone(),
            tag,
            mode,
            bound,
            time: 0.0,
            weight,
            target: weight,
            ease,
            fade_in,
            rate: 1.0,
            seen: true,
            leaving: false,
        });
        self.layers.len() - 1
    }

    /// End of a frame's requests (see [`Self::begin`]): the layers not asked for fade out
    /// from here on, keeping their weights.
    pub fn end(&mut self) {
        for l in &mut self.layers {
            l.leaving = !l.seen;
        }
    }

    /// The layer playing `clip` in role `tag`.
    pub fn layer(&self, clip: &Arc<Clip>, tag: u32) -> Option<&Layer> {
        self.find(clip, tag).map(|i| &self.layers[i])
    }

    /// Whether the playing clips show or hide the node named `name` (a part of a weapon
    /// model, animation.md §9.7): the visibility track of the highest priority among
    /// them, the latest started of equals; `None` when no playing clip has one for it.
    pub fn visible(&self, name: &str) -> Option<bool> {
        let mut best: Option<(i32, bool)> = None;
        for l in self.layers.iter().filter(|l| l.ease > 0.0) {
            for (n, priority, track) in &l.clip.visibility {
                if n != name {
                    continue;
                }
                if let Some(v) = track.at(l.sample_time(self.phase)) {
                    if best.is_none_or(|(p, _)| *priority >= p) {
                        best = Some((*priority, v));
                    }
                }
            }
        }
        best.map(|(_, v)| v)
    }

    /// Where the last [`Self::pose`] put the extra node at `k` in the list given to
    /// [`Self::set_extra`] (its local transform); `None` before the first pose with it.
    pub fn extra_local(&self, k: usize) -> Option<Transform> {
        self.locals.get(self.skel.locals.len() + k).copied()
    }

    /// The extra nodes the last [`Self::pose`] left away from their place in their
    /// model, with how far: units moved and degrees turned (for reports).
    pub fn extra_moved(&self) -> impl Iterator<Item = (&str, f32, f32)> {
        let n = self.skel.locals.len();
        self.extra.iter().zip(self.locals.iter().skip(n)).filter(|(x, l)| x.bind != **l).map(|(x, l)| {
            (x.name.as_str(), x.bind.translation.distance(l.translation), x.bind.rotation.angle_between(l.rotation).to_degrees())
        })
    }

    /// Advance fades, weights and times by `dt` seconds; synced layers move their shared
    /// phase at `sync_rate` times their own speed (weighted by their shares). Layers
    /// faded out are dropped.
    pub fn advance(&mut self, dt: f32, sync_rate: f32) {
        let (mut len_sum, mut w_sum) = (0.0, 0.0);
        for l in &mut self.layers {
            if l.leaving {
                l.ease = (l.ease - dt / DEFAULT_FADE).max(0.0);
            } else {
                let up = if l.fade_in <= 0.0 { f32::INFINITY } else { dt / l.fade_in };
                l.ease = (l.ease + up).min(1.0);
                l.weight = if l.target > l.weight { (l.weight + up).min(l.target) } else { (l.weight - dt / DEFAULT_FADE).max(l.target) };
            }
            match l.mode {
                Mode::Synced => {
                    let w = l.weight * l.ease;
                    len_sum += w * l.clip.length() / l.clip.frequency;
                    w_sum += w;
                }
                _ => l.time += dt * l.rate * l.clip.frequency,
            }
        }
        self.layers.retain(|l| l.ease > 0.0 || !l.leaving);
        if w_sum > 1e-6 && len_sum > 1e-6 {
            self.phase = (self.phase + dt * sync_rate * w_sum / len_sum).rem_euclid(1.0);
        }
    }

    /// Blend the layers into [`Self::locals`].
    pub fn pose(&mut self) {
        let skel = &self.skel;
        self.scratch.clear();
        for (li, l) in self.layers.iter().enumerate() {
            if l.ease <= 0.0 {
                continue;
            }
            for &(bone, track, priority) in &l.bound {
                self.scratch.push(Contribution { bone, priority, weight: l.weight * l.ease, ease: l.ease, layer: li as u16, track });
            }
        }
        self.scratch.sort_unstable_by_key(|c| (c.bone, Reverse(c.priority)));
        // Bones without tracks: the bind pose, or their own rest pose; held nodes: the
        // identity; extra nodes: their place in their model.
        self.locals.clone_from(&skel.locals);
        self.locals.extend(self.extra.iter().map(|x| x.bind));
        for &h in &self.held {
            self.locals[h] = Transform::IDENTITY;
        }
        for &(n, pose) in &self.rest {
            self.locals[n] = pose;
        }
        let phase = self.phase;
        let mut i = 0;
        while i < self.scratch.len() {
            let bone = self.scratch[i].bone as usize;
            // The bone's rest pose (set above).
            let bind = self.locals[bone];
            let (mut t, mut q, mut s) = (Vec3::ZERO, Vec4::ZERO, 0.0f32);
            let mut first: Option<Vec4> = None;
            let mut free = 1.0f32;
            while i < self.scratch.len() && self.scratch[i].bone as usize == bone {
                // One priority group.
                let p = self.scratch[i].priority;
                let g0 = i;
                let (mut sum, mut ease) = (0.0, 0.0);
                while i < self.scratch.len() && self.scratch[i].bone as usize == bone && self.scratch[i].priority == p {
                    sum += self.scratch[i].weight;
                    ease += self.scratch[i].ease;
                    i += 1;
                }
                if free <= 1e-6 {
                    continue;
                }
                let take = ease.min(free);
                free -= take;
                for c in &self.scratch[g0..i] {
                    // Clips all at weight 0 share by their fades.
                    let f = if sum > 1e-6 { c.weight / sum * take } else { c.ease / ease * take };
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

    /// Write the pose to the skeleton's node entities (the root excluded) and to the
    /// extra nodes.
    pub fn apply(&self, transforms: &mut Query<&mut Transform>) {
        let extra = self.extra.iter().map(|x| &x.entity).zip(self.locals.iter().skip(self.skel.locals.len()));
        for (e, local) in self.nodes.iter().zip(&self.locals).skip(1).chain(extra) {
            if let Ok(mut tr) = transforms.get_mut(*e) {
                if *tr != *local {
                    *tr = *local;
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
            visibility: Vec::new(),
            materials: Vec::new(),
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
        // Half faded in on top: the base fills the other half.
        a.layers[1].ease = 0.5;
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

    /// A rifle's aim: `level` has the spine (A) and the arms (B), `up` the spine only,
    /// both over an idle that has both bones.
    #[test]
    fn a_bone_one_clip_has_alone_stays_with_it_at_any_weight() {
        let mut a = animator();
        let idle = clip(&[("A", [1.0, 1.0, 1.0], 10), ("B", [2.0, 2.0, 2.0], 10)], 1.0);
        let level = clip(&[("A", [10.0, 0.0, 0.0], 30), ("B", [0.0, 10.0, 0.0], 35)], 1.0);
        let up = clip(&[("A", [30.0, 0.0, 0.0], 30)], 1.0);
        for (wl, wu) in [(1.0, 0.0), (0.75, 0.25), (0.25, 0.75), (0.0, 1.0)] {
            a.begin();
            a.want(&idle, 0, 1.0, Mode::Loop, 0.0);
            a.want(&level, 1, wl, Mode::Loop, 0.0);
            a.want(&up, 2, wu, Mode::Loop, 0.0);
            a.end();
            // Long enough for the weights to reach their targets.
            a.advance(1.0, 1.0);
            a.pose();
            assert!((a.locals[2].translation.x - (10.0 * wl + 30.0 * wu)).abs() < 1e-4, "the spine shares by weight at {wl}/{wu}: {}", a.locals[2].translation);
            assert_eq!(a.locals[3].translation, Vec3::new(0.0, 10.0, 0.0), "the arms stay with the level clip at {wl}/{wu}");
        }
        // Let go of both: they fade out together and the idle comes through on both bones.
        a.begin();
        a.want(&idle, 0, 1.0, Mode::Loop, 0.0);
        a.end();
        a.advance(DEFAULT_FADE * 0.75, 1.0);
        a.pose();
        assert!((a.locals[3].translation - Vec3::new(1.5, 4.0, 1.5)).length() < 1e-4, "{}", a.locals[3].translation);
        a.advance(DEFAULT_FADE, 1.0);
        a.pose();
        assert_eq!(a.layers.len(), 1);
        assert_eq!((a.locals[2].translation, a.locals[3].translation), (Vec3::splat(1.0), Vec3::splat(2.0)));
    }

    #[test]
    fn a_weight_change_moves_the_share_without_a_jump() {
        let mut a = animator();
        let x = clip(&[("A", [10.0, 0.0, 0.0], 30)], 1.0);
        let y = clip(&[("A", [0.0, 10.0, 0.0], 30)], 1.0);
        let step = |a: &mut Animator, wx: f32, wy: Option<f32>, dt: f32| {
            a.begin();
            a.want(&x, 0, wx, Mode::Loop, DEFAULT_FADE);
            if let Some(wy) = wy {
                a.want(&y, 1, wy, Mode::Loop, DEFAULT_FADE);
            }
            a.end();
            a.advance(dt, 1.0);
            a.pose();
            a.locals[2].translation
        };
        assert_eq!(step(&mut a, 0.5, Some(0.5), 1.0), Vec3::new(5.0, 5.0, 0.0));
        // y is let go and x asked for in full: the next frame is still close to half and half.
        let t = step(&mut a, 1.0, None, 0.01);
        assert!((t.x - 5.0).abs() < 0.5 && (t.y - 5.0).abs() < 0.5, "{t}");
        let t = step(&mut a, 1.0, None, 1.0);
        assert_eq!(t, Vec3::new(10.0, 0.0, 0.0));
    }

    #[test]
    fn the_highest_priority_visibility_track_decides() {
        use fnv_nif::anim::BoolTrack;
        let with = |priority: i32, track: BoolTrack| {
            let mut c = (*clip(&[("A", [0.0; 3], 30)], 1.0)).clone();
            c.visibility = vec![("##Gun".to_string(), priority, track)];
            Arc::new(c)
        };
        let mut a = animator();
        assert_eq!(a.visible("##Gun"), None);
        // The holster clip hides the part; the draw clip, above it, shows it half way.
        let holster = with(0, BoolTrack { value: Some(false), keys: Vec::new() });
        let draw = with(55, BoolTrack { value: None, keys: vec![(0.0, false), (0.5, true)] });
        a.want(&holster, 0, 1.0, Mode::Once, 0.0);
        assert_eq!(a.visible("##Gun"), Some(false));
        a.want(&draw, 1, 1.0, Mode::Once, 0.0);
        a.advance(0.25, 1.0);
        assert_eq!(a.visible("##Gun"), Some(false));
        a.advance(0.5, 1.0);
        assert_eq!(a.visible("##Gun"), Some(true));
        assert_eq!(a.visible("##Other"), None);
    }

    #[test]
    fn a_rest_pose_stands_in_for_the_bind_pose() {
        let mut a = animator();
        let rest = Transform::from_xyz(0.0, 0.0, 7.0);
        a.set_rest(3, Some(rest));
        a.pose();
        assert_eq!(a.locals[3], rest, "no clip drives the node");
        // A clip on it, half faded in: the rest pose fills the other half.
        let c = clip(&[("B", [0.0, 0.0, 1.0], 30)], 1.0);
        a.want(&c, 0, 1.0, Mode::Loop, 0.0);
        a.layers[0].ease = 0.5;
        a.pose();
        assert!((a.locals[3].translation.z - 4.0).abs() < 1e-5, "{}", a.locals[3].translation);
        a.layers.clear();
        a.set_rest(3, None);
        a.pose();
        assert_eq!(a.locals[3].translation, Vec3::new(0.0, 2.0, 0.0), "the bind pose again");
    }

    /// A pistol's slide: a node of the weapon model, not of the skeleton.
    #[test]
    fn clips_drive_extra_nodes_by_name() {
        let mut a = animator();
        let bind = Transform::from_xyz(5.0, 0.0, 0.0);
        let part = |name: &str| ExtraNode { name: name.to_string(), entity: Entity::from_raw_u32(9).unwrap(), bind };
        let fire = clip(&[("A", [1.0, 0.0, 0.0], 30), ("##Slide", [2.0, 0.0, 0.0], 55), ("##Other", [7.0, 0.0, 0.0], 55)], 1.0);
        // Started before the weapon is there: nothing of it to drive yet.
        a.want(&fire, 0, 1.0, Mode::Once, 0.0);
        a.pose();
        assert_eq!(a.locals.len(), 4);
        // The weapon arrives: the playing clip takes its slide; a name the skeleton has
        // stays the skeleton's.
        a.set_extra(vec![part("##Slide"), part("A"), part("##Idle")]);
        assert_eq!(a.extra_driven(&fire), ["##Slide"]);
        a.pose();
        assert_eq!(a.locals.len(), 7);
        assert_eq!(a.locals[4].translation, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(a.locals[5], bind, "the skeleton's A is driven, not the weapon's");
        assert_eq!(a.locals[2].translation, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(a.locals[6], bind, "a part no clip drives keeps its place in the model");
        assert_eq!(a.extra_moved().collect::<Vec<_>>(), [("##Slide", 3.0, 0.0)]);
        // Half faded in: the rest is the part's place in the model.
        a.layers[0].ease = 0.5;
        a.pose();
        assert!((a.locals[4].translation.x - 3.5).abs() < 1e-5);
        // The weapon goes: the clip plays on without it.
        a.set_extra(Vec::new());
        a.pose();
        assert_eq!(a.locals.len(), 4);
        assert_eq!(a.locals[2].translation, Vec3::new(1.0, 0.0, 0.0));
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
