//! Plays KF controller sequences by sampling `fnv_nif::anim` tracks every frame and
//! writing the bones' local transforms. Done here rather than with Bevy animation clips
//! so that the original interpolation rules (B-splines, TBC keys, cycle types) stay in
//! the engine-independent format crate (docs/design/layer2.md §2.7).
//!
//! The viewer plays one sequence at a time ([`AnimState`]); world objects carry their
//! own [`Playing`] component, advanced by [`advance_instances`].

use bevy::prelude::*;
use fnv_nif::anim::{BoolTrack, Sequence, TrackData, TransformTrack, ValueTrack};
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
    /// Each track's priority (nif.xml ControlledBlock "Priority"; 0 when absent), in
    /// `tracks` order.
    pub priorities: Vec<i32>,
    /// Tracks without a target name or with an interpolator that is not a transform.
    pub skipped: usize,
    /// The sequence's accumulation root (nif.xml NiControllerSequence "Accum Root Name").
    pub accum_root: Option<String>,
    /// Text keys (time, text).
    pub text_keys: Vec<(f32, String)>,
    /// Visibility tracks (`NiVisController`): node name, priority, shown over time
    /// (docs/spec-notes/animation.md §9.7).
    pub visibility: Vec<(String, i32, BoolTrack)>,
    /// Tracks on a shape's material: its glow and texture offset (`crate::material_anim`,
    /// docs/spec-notes/activators.md §2).
    pub materials: Vec<MaterialTrack>,
}

/// What a material track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialTarget {
    /// `NiMaterialColorController` with controller ID `SELF_ILLUM`: the emissive colour
    /// [documented: nif.xml MaterialColor TC_SELF_ILLUM; observed controller ID].
    SelfIllum,
    /// `BSMaterialEmittanceMultController`: the emissive multiplier [hypothesis from the
    /// name; observed on blinking lights and slot machines].
    EmissiveMult,
    /// `NiTextureTransformController` `0-0-TT_TRANSLATE_U` (0) or `_V` (1): the base
    /// texture's offset [documented: nif.xml TransformMember; observed controller IDs].
    UvOffset(usize),
    /// `NiAlphaController` on the shape's `NiMaterialProperty`: the material's alpha
    /// [observed: the controlled blocks name property type NiMaterialProperty and the
    /// controller targets that property (`windowem01.nif`); hypothesis: the value
    /// replaces the stored alpha]. The neon signs and lit windows fade with it
    /// (activators.md §6).
    Alpha,
}

/// One track of a sequence on a shape's material, by shape name.
#[derive(Debug, Clone)]
pub struct MaterialTrack {
    pub shape: String,
    pub target: MaterialTarget,
    pub values: Arc<ValueTrack>,
}

impl MaterialTrack {
    /// The material track of a sequence's track, if it drives a material member this
    /// engine draws: emissive colour, emissive multiplier, base texture offset, alpha.
    pub fn of(t: &fnv_nif::anim::Track) -> Option<MaterialTrack> {
        let shape = t.node_name.clone()?;
        let values = t.values.clone()?;
        let id = t.controller_id.as_deref().unwrap_or("");
        let target = match (t.controller_type.as_deref()?, &values) {
            ("NiMaterialColorController", ValueTrack::Point3(_)) if id == "SELF_ILLUM" => MaterialTarget::SelfIllum,
            ("BSMaterialEmittanceMultController", ValueTrack::Float(_)) => MaterialTarget::EmissiveMult,
            ("NiAlphaController", ValueTrack::Float(_)) => MaterialTarget::Alpha,
            ("NiTextureTransformController", ValueTrack::Float(_)) => match id {
                "0-0-TT_TRANSLATE_U" => MaterialTarget::UvOffset(0),
                "0-0-TT_TRANSLATE_V" => MaterialTarget::UvOffset(1),
                _ => return None,
            },
            _ => return None,
        };
        Some(MaterialTrack { shape, target, values: Arc::new(values) })
    }
}

impl Clip {
    pub fn from_sequence(seq: &Sequence) -> Clip {
        let mut tracks = Vec::new();
        let mut priorities = Vec::new();
        let mut skipped = 0;
        let mut visibility = Vec::new();
        let mut materials = Vec::new();
        for t in &seq.tracks {
            materials.extend(MaterialTrack::of(t));
            if let (Some(n), Some(b), Some("NiVisController")) = (&t.node_name, &t.bools, t.controller_type.as_deref()) {
                visibility.push((n.clone(), t.priority.unwrap_or(0) as i32, b.clone()));
            }
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
            visibility,
            materials,
        }
    }

    pub fn length(&self) -> f32 {
        (self.stop - self.start).max(0.0)
    }

    /// The speed the clip's accumulation root moves at over one cycle (units per second
    /// at frequency 1), from its track's translation at the start and stop times: the
    /// ground speed the clip was made for [observed: `locomotion\male\mtforward.kf`
    /// moves `Bip01` 102.3 units in 1.2 s, docs/spec-notes/animation.md §9.2]. `None`
    /// when the root has no track or the clip no length.
    pub fn root_speed(&self) -> Option<f32> {
        let t = self.accum_track()?;
        let (a, b) = (t.translation.sample(self.start)?, t.translation.sample(self.stop)?);
        let len = self.length();
        (len > 0.0).then(|| Vec2::new(b[0] - a[0], b[1] - a[1]).length() / len)
    }

    /// The transform track of the accumulation root, if one drives it.
    pub fn accum_track(&self) -> Option<&TransformTrack> {
        let r = self.accum_root.as_deref()?;
        self.tracks.iter().find(|(n, _)| n == r).map(|(_, t)| t.as_ref())
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
    /// Play in place: the index in `tracks` of the accumulation root's track and the
    /// root's translation over one cycle, removed in proportion to the cycle phase (the
    /// rule of [`BlendPlayer`], docs/spec-notes/actors.md §8.2). Set by
    /// [`Playing::in_place`]; world objects leave it unset.
    pub root_motion: Option<(usize, Vec3)>,
    /// Held where it is: [`advance_instances`] does not move it on (a radio switched
    /// off, docs/spec-notes/activators.md §3).
    pub held: bool,
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
            root_motion: None,
            held: false,
        }
    }

    /// Remove the travel of `clip`'s accumulation root, if a track drives it (walk and
    /// run clips), so the clip plays in place; `node` is the binding given to [`Self::new`].
    pub fn in_place(mut self, clip: &Clip, node: impl Fn(&str) -> Option<Entity>) -> Playing {
        let root = clip.accum_root.as_deref().and_then(&node);
        let track = clip.accum_track();
        if let (Some(root), Some(track)) = (root, track) {
            let at = |t: f32| track.translation.sample(t).map_or(Vec3::ZERO, Vec3::from_array);
            if let Some(k) = self.tracks.iter().position(|(e, _)| *e == root) {
                self.root_motion = Some((k, at(clip.stop) - at(clip.start)));
            }
        }
        self
    }

    /// Start at `phase` (0–1) of the cycle instead of the clip's start.
    pub fn at_phase(mut self, phase: f32) -> Playing {
        self.time = self.start + phase.clamp(0.0, 1.0) * (self.stop - self.start);
        self
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
        let phase = if self.stop > self.start { (t - self.start) / (self.stop - self.start) } else { 0.0 };
        for (k, (e, track)) in self.tracks.iter().enumerate() {
            let Ok(mut tr) = transforms.get_mut(*e) else { continue };
            if let Some(v) = track.translation.sample(t) {
                tr.translation = Vec3::from_array(v);
                if let Some((_, per_cycle)) = self.root_motion.filter(|r| r.0 == k) {
                    tr.translation -= per_cycle * phase;
                }
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

/// A clip's tracks bound to a skeleton's node indices, shared by every actor of that
/// skeleton, with the accumulation-root rule of docs/spec-notes/actors.md §8.2.
#[derive(Debug)]
pub struct BoundClip {
    pub clip: Arc<Clip>,
    /// (node index, track, priority).
    pub tracks: Vec<(usize, Arc<TransformTrack>, i32)>,
    /// The accumulation root's node index, if the skeleton has it.
    pub root: Option<usize>,
    /// The index in `tracks` of the root's track, and the root's translation over one
    /// cycle, which sampling removes so the clip plays in place.
    pub root_track: Option<(usize, Vec3)>,
}

impl BoundClip {
    /// Bind `clip`'s tracks to node indices by name (`node`).
    pub fn bind(clip: Arc<Clip>, node: impl Fn(&str) -> Option<usize>) -> BoundClip {
        let tracks: Vec<(usize, Arc<TransformTrack>, i32)> = clip
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(k, (n, t))| Some((node(n)?, t.clone(), clip.priorities.get(k).copied().unwrap_or(0))))
            .collect();
        let root = clip.accum_root.as_deref().and_then(&node);
        let root_track = root.and_then(|r| tracks.iter().position(|(i, _, _)| *i == r)).map(|k| {
            let tt = &tracks[k].1;
            let at = |t: f32| tt.translation.sample(t).map_or(Vec3::ZERO, Vec3::from_array);
            (k, at(clip.stop) - at(clip.start))
        });
        BoundClip { clip, tracks, root, root_track }
    }

    /// Sequence time at `phase` (0–1 of the cycle).
    pub fn time_at(&self, phase: f32) -> f32 {
        self.clip.start + phase * (self.clip.stop - self.clip.start)
    }
}

/// One weighted clip of a [`BlendPlayer`].
#[derive(Debug, Clone)]
pub struct Layer {
    pub clip: Arc<BoundClip>,
    /// Where in the cycle (0–1).
    pub phase: f32,
    pub weight: f32,
}

/// How many clips a [`BlendPlayer`] blends at once.
pub const MAX_LAYERS: usize = 6;

/// The priority of the identity a clip gives an accumulation root it does not drive: below
/// every track, so any clip that drives the root wins.
const UNDRIVEN_ROOT_PRIORITY: i32 = i32::MIN;

/// One clip's sample of one node.
#[derive(Debug, Clone, Copy)]
struct Contrib {
    t: Vec3,
    r: Quat,
    s: f32,
    w: f32,
    p: i32,
}

const CONTRIB_NONE: Contrib = Contrib { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w: 0.0, p: 0 };

/// A running weighted average of transforms (rotations nlerped along the shorter arc).
#[derive(Debug, Clone, Copy)]
struct Mix {
    t: Vec3,
    r: Quat,
    s: f32,
    w: f32,
}

impl Mix {
    const EMPTY: Mix = Mix { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w: 0.0 };

    fn add(&mut self, t: Vec3, r: Quat, s: f32, w: f32) {
        if w <= 0.0 {
            return;
        }
        if self.w == 0.0 {
            *self = Mix { t, r, s, w };
            return;
        }
        let f = w / (self.w + w);
        let r = if self.r.dot(r) < 0.0 { -r } else { r };
        self.t = self.t.lerp(t, f);
        self.r = Quat::from_vec4(Vec4::from(self.r).lerp(Vec4::from(r), f));
        self.s += (s - self.s) * f;
        self.w += w;
    }
}

/// Blend one node's samples by priority (docs/spec-notes/actors.md §9.4): from the highest
/// priority down, each priority group takes its summed weight, at most what is left of 1,
/// shared by weight within the group; the rest goes to `bind`.
fn blend_by_priority(c: &mut [Contrib], bind: Transform) -> Transform {
    // Highest priority first (insertion sort: at most MAX_LAYERS entries).
    for i in 1..c.len() {
        let mut j = i;
        while j > 0 && c[j - 1].p < c[j].p {
            c.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut mix = Mix::EMPTY;
    let mut left = 1.0f32;
    let mut k = 0;
    while k < c.len() && left > 1e-6 {
        let end = (k..c.len()).find(|&e| c[e].p != c[k].p).unwrap_or(c.len());
        let group: f32 = c[k..end].iter().map(|x| x.w).sum();
        if group > 0.0 {
            let share = group.min(left);
            for x in &c[k..end] {
                mix.add(x.t, x.r, x.s, x.w / group * share);
            }
            left -= share;
        }
        k = end;
    }
    if left > 1e-3 {
        mix.add(bind.translation, bind.rotation, bind.scale.x, left);
    }
    Transform { translation: mix.t, rotation: mix.r.normalize(), scale: Vec3::splat(mix.s) }
}

/// Poses a skeleton from up to [`MAX_LAYERS`] weighted clips (docs/spec-notes/actors.md
/// §8.3 and §9.4): per node, the clips that drive it are blended by track priority and
/// weight (translations and scales lerped, rotations nlerped along the shorter arc), with
/// any weight left over going to the node's bind transform; a node no clip drives any
/// more returns to its bind transform. The accumulation root plays in place: a clip that
/// drives it has its per-cycle travel removed, and one that does not holds it at the
/// identity (below every priority). Buffers are allocated once, so posing allocates
/// nothing.
#[derive(Component, Debug)]
pub struct BlendPlayer {
    /// The skeleton's node entities, by node index.
    pub nodes: Vec<Entity>,
    /// The nodes' local bind transforms.
    pub bind: Arc<Vec<Transform>>,
    pub layers: [Option<Layer>; MAX_LAYERS],
    contribs: Vec<([Contrib; MAX_LAYERS], u8)>,
    touched: Vec<usize>,
    /// Nodes posed in the previous call, and a per-node flag for this call's.
    posed: Vec<usize>,
    driven: Vec<bool>,
}

impl BlendPlayer {
    pub fn new(nodes: Vec<Entity>, bind: Arc<Vec<Transform>>) -> BlendPlayer {
        let n = nodes.len();
        BlendPlayer {
            nodes,
            bind,
            layers: Default::default(),
            contribs: vec![([CONTRIB_NONE; MAX_LAYERS], 0); n],
            touched: Vec::with_capacity(n),
            posed: Vec::with_capacity(n),
            driven: vec![false; n],
        }
    }

    fn add(&mut self, i: usize, c: Contrib) {
        let Some((slots, n)) = self.contribs.get_mut(i) else { return };
        if *n == 0 {
            self.touched.push(i);
        }
        if (*n as usize) < MAX_LAYERS {
            slots[*n as usize] = c;
            *n += 1;
        }
    }

    /// The blended local transforms (node index, transform) of every node a layer drives,
    /// and the bind transforms of the nodes the layers stopped driving; returns the number
    /// of tracks sampled. `out` is cleared first.
    pub fn sample(&mut self, out: &mut Vec<(usize, Transform)>) -> usize {
        out.clear();
        let mut sampled = 0;
        let layers = std::mem::take(&mut self.layers);
        for layer in layers.iter().flatten().filter(|l| l.weight > 0.0) {
            let (bc, w) = (&layer.clip, layer.weight);
            let time = cycle_time(bc.time_at(layer.phase), bc.clip.start, bc.clip.stop, bc.clip.cycle_type);
            let phase = if bc.clip.stop > bc.clip.start { (time - bc.clip.start) / (bc.clip.stop - bc.clip.start) } else { 0.0 };
            for (j, (i, track, p)) in bc.tracks.iter().enumerate() {
                let b = self.bind.get(*i).copied().unwrap_or_default();
                let mut t = track.translation.sample(time).map_or(b.translation, Vec3::from_array);
                if let Some((rj, per_cycle)) = bc.root_track {
                    if rj == j {
                        t -= per_cycle * phase;
                    }
                }
                let r = track.rotation_at(time).map_or(b.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w).normalize());
                let s = track.scale.sample(time).map_or(b.scale.x, |[s]| s);
                self.add(*i, Contrib { t, r, s, w, p: *p });
                sampled += 1;
            }
            if let (Some(root), None) = (bc.root, bc.root_track) {
                self.add(root, Contrib { t: Vec3::ZERO, r: Quat::IDENTITY, s: 1.0, w, p: UNDRIVEN_ROOT_PRIORITY });
            }
        }
        self.layers = layers;
        for &i in &self.touched {
            let (slots, n) = &mut self.contribs[i];
            let bind = self.bind.get(i).copied().unwrap_or_default();
            out.push((i, blend_by_priority(&mut slots[..*n as usize], bind)));
            *n = 0;
            self.driven[i] = true;
        }
        // Nodes posed last time that no clip drives now go back to their bind transform.
        for &i in &self.posed {
            if !self.driven[i] {
                out.push((i, self.bind.get(i).copied().unwrap_or_default()));
            }
        }
        self.posed.clear();
        for &i in &self.touched {
            self.driven[i] = false;
            self.posed.push(i);
        }
        self.touched.clear();
        sampled
    }

    /// Pose the nodes: [`Self::sample`] written to their transforms. `scratch` is reused
    /// between calls.
    pub fn pose(&mut self, scratch: &mut Vec<(usize, Transform)>, transforms: &mut Query<&mut Transform>) -> usize {
        let n = self.sample(scratch);
        for (i, tr) in scratch.iter() {
            if let Some(Ok(mut t)) = self.nodes.get(*i).map(|e| transforms.get_mut(*e)) {
                *t = *tr;
            }
        }
        n
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
        // Walk and run clips play in place, as on actors (docs/spec-notes/actors.md §8.2).
        let playing = Playing::new(source, &clip, |n| nodes.get(n).copied()).in_place(&clip, |n| nodes.get(n).copied());
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
        let held = p.held;
        p.step(dt, held, &mut transforms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys, Track};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    fn seq(tracks: &[&str]) -> Sequence {
        let track = |n: &&str| Track {
            node_name: Some(n.to_string()),
            controller_type: None,
            priority: None,
            bools: None,
            controller_id: None,
            values: None,
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

    fn track(t: Channel<3>, r: Channel<4>) -> Arc<TransformTrack> {
        Arc::new(TransformTrack { translation: t, rotation: r, euler: None, scale: Channel::Static(None) })
    }

    fn clip(len: f32, tracks: Vec<(&str, Arc<TransformTrack>)>) -> Arc<Clip> {
        prio_clip(len, tracks.into_iter().map(|(n, t)| (n, t, 30)).collect())
    }

    fn prio_clip(len: f32, tracks: Vec<(&str, Arc<TransformTrack>, i32)>) -> Arc<Clip> {
        Arc::new(Clip {
            name: String::new(),
            start: 0.0,
            stop: len,
            frequency: 1.0,
            cycle_type: 0,
            priorities: tracks.iter().map(|t| t.2).collect(),
            tracks: tracks.into_iter().map(|(n, t, _)| (n.to_string(), t)).collect(),
            skipped: 0,
            accum_root: Some("Bip01".into()),
            text_keys: Vec::new(),
            visibility: Vec::new(),
            materials: Vec::new(),
        })
    }

    /// Nodes 1 `Bip01`, 2 `Bip01 NonAccum`, 3 `Other`; node 0 is the actor's root.
    fn player() -> (BlendPlayer, impl Fn(&str) -> Option<usize>) {
        let nodes = (1..=4).map(|i| Entity::from_raw_u32(i).unwrap()).collect();
        let bind = vec![Transform::IDENTITY, Transform::from_xyz(0.0, 0.657, 67.771), Transform::IDENTITY, Transform::from_xyz(5.0, 0.0, 0.0)];
        let ix = |n: &str| ["Bip01", "Bip01 NonAccum", "Other"].iter().position(|x| *x == n).map(|i| i + 1);
        (BlendPlayer::new(nodes, Arc::new(bind)), ix)
    }

    fn walk_and_idle(ix: &impl Fn(&str) -> Option<usize>) -> (Arc<BoundClip>, Arc<BoundClip>) {
        let travel = Keys {
            kind: KeyKind::Linear,
            keys: [(0.0, [0.0, 0.0, 0.0]), (0.5, [0.0, 52.0, 0.0]), (1.0, [0.0, 100.0, 0.0])]
                .map(|(time, value)| Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] })
                .to_vec(),
        };
        let walk = clip(
            1.0,
            vec![
                ("Bip01", track(Channel::Keys(travel), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))),
                ("Bip01 NonAccum", track(Channel::Static(Some([0.0, 0.0, 65.0])), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))),
                ("Other", track(Channel::Static(Some([9.0, 0.0, 0.0])), Channel::Static(None))),
            ],
        );
        let (s, c) = (FRAC_PI_4.sin(), FRAC_PI_4.cos());
        let idle = clip(4.0, vec![("Bip01 NonAccum", track(Channel::Static(Some([0.0, 0.0, 61.0])), Channel::Static(Some([c, 0.0, 0.0, s])))), ("Missing", track(Channel::Static(None), Channel::Static(None)))]);
        (Arc::new(BoundClip::bind(walk, ix)), Arc::new(BoundClip::bind(idle, ix)))
    }

    #[test]
    fn bound_clip_root_motion() {
        let (_, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        assert_eq!(walk.tracks.len(), 3);
        assert_eq!((walk.root, walk.root_track), (Some(1), Some((0, Vec3::new(0.0, 100.0, 0.0)))));
        // A track whose node the skeleton lacks is dropped; the idle drives no root.
        assert_eq!((idle.tracks.len(), idle.root, idle.root_track), (1, Some(1), None));
        assert_eq!(walk.time_at(0.25), 0.25);
    }

    #[test]
    fn single_clip_player_plays_walks_in_place() {
        // nvf-viewer's player: the walk clip's Bip01 travel is removed, its sway kept.
        let (_, ix) = player();
        let (walk, _) = walk_and_idle(&ix);
        let mut app = App::new();
        let root = app.world_mut().spawn(Transform::IDENTITY).id();
        let other = app.world_mut().spawn(Transform::IDENTITY).id();
        let node = |n: &str| match n {
            "Bip01" => Some(root),
            "Other" => Some(other),
            _ => None,
        };
        let playing = Playing::new("walk", &walk.clip, node).in_place(&walk.clip, node).at_phase(0.5);
        assert_eq!(playing.root_motion, Some((0, Vec3::new(0.0, 100.0, 0.0))));
        app.world_mut().spawn(playing);
        app.add_systems(Update, |mut q: Query<&mut Playing>, mut t: Query<&mut Transform>| {
            for mut p in &mut q {
                p.step(0.0, true, &mut t);
            }
        });
        app.update();
        // Half way: 52 units travelled, 50 removed.
        let y = app.world().get::<Transform>(root).unwrap().translation.y;
        assert!((y - 2.0).abs() < 1e-4, "{y}");
        assert_eq!(app.world().get::<Transform>(other).unwrap().translation.x, 9.0);
        // Without in_place the travel stays (world objects).
        let p = Playing::new("walk", &walk.clip, node);
        assert_eq!(p.root_motion, None);
    }

    #[test]
    fn one_clip_plays_in_place() {
        let (mut p, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        let mut out = Vec::new();
        p.layers[0] = Some(Layer { clip: walk, phase: 0.5, weight: 1.0 });
        assert_eq!(p.sample(&mut out), 3);
        let get = |out: &Vec<(usize, Transform)>, i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1;
        // The travel is removed: 52 at half the cycle less half of 100.
        assert!((get(&out, 1).translation - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-4);
        assert_eq!(get(&out, 2).translation, Vec3::new(0.0, 0.0, 65.0));
        // A channel with no value keeps the bind transform's.
        assert_eq!(get(&out, 3).translation, Vec3::new(9.0, 0.0, 0.0));
        assert_eq!(get(&out, 3).rotation, Quat::IDENTITY);
        // An idle without a root track holds the root at the identity.
        p.layers[0] = Some(Layer { clip: idle, phase: 0.0, weight: 1.0 });
        p.sample(&mut out);
        assert_eq!(get(&out, 1), Transform::IDENTITY);
        // `Other`, which only the walk drove, is back at its bind transform.
        assert_eq!(out.len(), 3);
        assert_eq!(get(&out, 3), Transform::from_xyz(5.0, 0.0, 0.0));
    }

    #[test]
    fn two_clips_blend_by_weight() {
        let (mut p, ix) = player();
        let (walk, idle) = walk_and_idle(&ix);
        let mut out = Vec::new();
        p.layers = [Some(Layer { clip: idle, phase: 0.0, weight: 0.75 }), Some(Layer { clip: walk, phase: 0.0, weight: 0.25 }), None, None, None, None];
        p.sample(&mut out);
        let get = |i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1;
        assert_eq!(get(1), Transform::IDENTITY);
        let na = get(2);
        assert!((na.translation.z - (0.75 * 61.0 + 0.25 * 65.0)).abs() < 1e-4);
        // 90° in the idle, 0° in the walk: three quarters of the way, 67.5° (nlerp is
        // close to slerp here).
        let (axis, angle) = na.rotation.to_axis_angle();
        assert!(axis.z > 0.99 && (angle.to_degrees() - 67.5).abs() < 2.0, "{}", angle.to_degrees());
        // Only the walk drives `Other`: a quarter of its pose, the rest the bind pose.
        assert!((get(3).translation.x - (0.75 * 5.0 + 0.25 * 9.0)).abs() < 1e-4);
        // Weights are not kept between frames.
        p.layers[1] = None;
        p.layers[0].as_mut().unwrap().weight = 1.0;
        p.sample(&mut out);
        assert_eq!(out.len(), 3);
        assert!((out.iter().find(|(j, _)| *j == 2).unwrap().1.translation.z - 61.0).abs() < 1e-5);
        assert_eq!(out.iter().find(|(j, _)| *j == 3).unwrap().1, Transform::from_xyz(5.0, 0.0, 0.0));
    }

    fn pose(t: [f32; 3]) -> Arc<TransformTrack> {
        track(Channel::Static(Some(t)), Channel::Static(Some([1.0, 0.0, 0.0, 0.0])))
    }

    #[test]
    fn priority_decides_per_node() {
        let (mut p, ix) = player();
        // Legs (`Other`) at 30 in a walk, 26 in an equip clip; the arm (`Bip01 NonAccum`
        // here) at 55 in the equip clip only.
        let walk = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([1.0, 0.0, 0.0]), 30)]), &ix));
        let equip = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([2.0, 0.0, 0.0]), 26), ("Bip01 NonAccum", pose([0.0, 0.0, 7.0]), 55)]), &ix));
        let idle = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([3.0, 0.0, 0.0]), 10), ("Bip01 NonAccum", pose([0.0, 0.0, 1.0]), 10)]), &ix));
        let mut out = Vec::new();
        let get = |out: &Vec<(usize, Transform)>, i: usize| out.iter().find(|(j, _)| *j == i).unwrap().1.translation;
        let layer = |c: &Arc<BoundClip>, weight: f32| Some(Layer { clip: c.clone(), phase: 0.0, weight });
        // Walking legs (30) beat the equip clip's (26); its upper body (55) wins.
        p.layers = [layer(&walk, 1.0), layer(&equip, 1.0), None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 3).x, 1.0);
        assert_eq!(get(&out, 2).z, 7.0);
        // Standing, the equip clip's legs (26) beat the idle's (10).
        p.layers = [layer(&idle, 1.0), layer(&equip, 1.0), None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 3).x, 2.0);
        // A higher priority fading in (0.25) leaves the rest to the lower one.
        p.layers = [layer(&idle, 1.0), layer(&equip, 0.25), None, None, None, None];
        p.sample(&mut out);
        assert!((get(&out, 2).z - (0.25 * 7.0 + 0.75 * 1.0)).abs() < 1e-5);
        // Equal priorities share by weight even when they add up to more than 1.
        let walk2 = Arc::new(BoundClip::bind(prio_clip(1.0, vec![("Other", pose([5.0, 0.0, 0.0]), 30)]), &ix));
        p.layers = [layer(&walk, 1.0), layer(&walk2, 1.0), None, None, None, None];
        p.sample(&mut out);
        assert!((get(&out, 3).x - 3.0).abs() < 1e-5);
        // A node no clip drives any more goes back to its bind transform (5, 0, 0).
        p.layers = [layer(&walk, 1.0), None, None, None, None, None];
        p.sample(&mut out);
        p.layers = [layer(&equip, 1.0), None, None, None, None, None];
        p.sample(&mut out);
        p.layers = [layer(&walk, 1.0), None, None, None, None, None];
        p.sample(&mut out);
        assert_eq!(get(&out, 2), Vec3::ZERO);
    }

    #[test]
    fn cycles() {
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 0), 0.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 1), 1.5);
        assert_eq!(cycle_time(2.5, 0.0, 2.0, 2), 2.0);
        assert_eq!(cycle_time(1.0, 1.0, 1.0, 0), 1.0);
    }
}
