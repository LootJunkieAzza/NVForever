//! Lip sync in the game (dialogue step D6, docs/spec-notes/lipsync.md): while an NPC
//! says a voiced response, its face moves from the response's `.lip` file, in time with
//! the sound.
//!
//! - **What moves.** The `.lip` gives a weight per channel, 30 times a second
//!   (`fnv_voice::lip`). A channel's name is the label of a morph in the FaceGen `.tri`
//!   file beside a head part's model (`fnv_actor::tri`): the head, the mouth, the lower
//!   teeth, the tongue, beards and eyebrows each carry the morphs that concern them. A
//!   vertex is drawn at: the model's vertex, shaped by the actor's FaceGen `.egm`
//!   (facegen.rs), plus Σ weight × the morph's displacement [hypothesis: the morphs add
//!   up linearly on top of the face shape; lipsync.md §4.3].
//! - **Which channels.** The sixteen mouth shapes, the blinks, the brows and the squints
//!   ([`DRIVEN`]). The look and head-turn channels are read but not applied: their unit
//!   is not established, and they come as a pair (the eyes turn against the head), so
//!   one without the other would be wrong (lipsync.md §6).
//! - **When.** `nvf_play::voice` says which response each speaker is saying and when its
//!   sound started (also with `--no-voice`); the frame for the moment `t` after that is
//!   `t × 30 − start frame`, the two frames around it mixed. The frames a file has
//!   before the sound starts are not shown [hypothesis]. When the line ends or is cut
//!   the face eases back to rest over [`EASE_SECONDS`] [engine choice], then the actor
//!   gets its own mesh back.
//! - **How.** While an actor speaks, each of its moving shapes is drawn from a mesh of
//!   its own whose positions are rewritten each frame (normals are kept as they are).
//!   Actors that do not speak cost nothing. Speakers without a rig — creatures, robots,
//!   talking activators, the unseen — are skipped, with a log line.
//!
//! One log line per response: `lip: animating <path> ...` or `lip: skipped <path>: <why>`.
//! `--no-lip` turns it off.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_actor::records::FaceGen;
use fnv_actor::tri::{label_aliases, Tri};
use fnv_actor::Appearance;
use fnv_sim::FormId;
use fnv_voice::lip::{channel, Lip, CHANNELS, CHANNEL_NAMES, PHONEMES};
use nvf_assets::actor::{PartUse, SpawnedActor};
use nvf_assets::GameData;
use nvf_play::voice::VoiceState;
use std::collections::HashMap;
use std::sync::Arc;

use crate::actors::ActorWorld;
use crate::facegen::sibling;
use crate::limbs::ActorRig;

/// The channels applied to the face: every mouth shape, then these modifiers.
pub const DRIVEN_MODIFIERS: [&str; 10] = [
    "BlinkLeft",
    "BlinkRight",
    "BrowDownLeft",
    "BrowDownRight",
    "BrowInLeft",
    "BrowInRight",
    "BrowUpLeft",
    "BrowUpRight",
    "SquintLeft",
    "SquintRight",
];

/// Whether channel `c` of a `.lip` is applied.
pub fn driven(c: usize) -> bool {
    c < PHONEMES || CHANNEL_NAMES.get(c).is_some_and(|n| DRIVEN_MODIFIERS.contains(n))
}

/// Seconds over which a face returns to rest when its line ends or is cut.
pub const EASE_SECONDS: f64 = 0.15;

/// A response let go this long before its sound's end counts as cut (for the log).
pub const CUT_SECONDS: f64 = 0.25;

/// A weight below this moves nothing worth a mesh update.
const SMALL: f32 = 1e-4;

/// A shape of an actor that a `.tri` can move.
#[derive(Clone)]
pub struct LipShape {
    pub entity: Entity,
    /// The part's model (archive path); the `.tri` and `.egm` sit beside it.
    pub model: String,
    /// The model's mesh, unshaped.
    pub cpu: Arc<Mesh>,
    /// The mesh the shape is drawn with when the actor is silent.
    pub rest: Handle<Mesh>,
}

/// On an actor's root: its shapes that have a `.tri` beside their model, and the
/// FaceGen coefficients its face was shaped with (`None`: the race's base face).
#[derive(Component, Clone)]
pub struct LipRig {
    pub shapes: Vec<LipShape>,
    pub facegen: Option<FaceGen>,
}

/// The rig of a freshly spawned actor (`actors::spawn`): `uses[i]` is part
/// `a.parts[part_index[i]]`, and `spawned.drawn` lists the shapes `spawn_actor` drew in
/// the order it walks them (caps and hidden shapes left out). `None` when no part has a
/// `.tri`, or when that order cannot be followed. `facegen` is what shaped the face
/// (`Appearance::shape_facegen`; `None` with FaceGen off).
pub fn rig(a: &Appearance, part_index: &[usize], uses: &[PartUse], spawned: &SpawnedActor, data: &GameData, facegen: Option<FaceGen>) -> Option<LipRig> {
    let mut shapes = Vec::new();
    let mut drawn = spawned.drawn.iter().zip(&spawned.part_of);
    for (ui, u) in uses.iter().enumerate() {
        let model = &a.parts.get(*part_index.get(ui)?)?.model;
        let has_tri = data.vfs.exists(&sibling(model, "tri"));
        for (i, s) in u.part.shapes.iter().enumerate() {
            if s.cap || s.name.as_ref().is_some_and(|n| u.hidden_shapes.iter().any(|h| h.eq_ignore_ascii_case(n))) {
                continue;
            }
            let (&entity, &pi) = drawn.next()?;
            if pi != ui {
                return None;
            }
            if has_tri {
                let rest = u.meshes.get(i).cloned().flatten().unwrap_or_else(|| s.mesh.clone());
                shapes.push(LipShape { entity, model: model.clone(), cpu: s.cpu.clone(), rest });
            }
        }
    }
    (!shapes.is_empty()).then(|| LipRig { shapes, facegen })
}

/// One morph of a shape at full weight: the vertices it moves and by how much.
pub type Displacement = Vec<(u32, [f32; 3])>;

/// The morphs of a shape that the `.lip` channels drive: (channel, displacement), for
/// the driven channels the `.tri` has a morph for (by label, [`label_aliases`]) that
/// moves at least one vertex. `shaped`: the `.tri`'s V + K positions with the actor's
/// FaceGen shape applied, for the statistical morphs (`Tri::displacements`).
pub fn shape_morphs(tri: &Tri, shaped: Option<&[[f32; 3]]>) -> Vec<(usize, Displacement)> {
    (0..CHANNELS)
        .filter(|&c| driven(c))
        .filter_map(|c| {
            let d = label_aliases(CHANNEL_NAMES[c]).into_iter().find_map(|label| tri.displacements(label, shaped))?;
            (!d.is_empty()).then_some((c, d))
        })
        .collect()
}

/// `base` with every morph added at its channel's weight.
pub fn morphed(base: &[[f32; 3]], morphs: &[(usize, Displacement)], weights: &[f32; CHANNELS]) -> Vec<[f32; 3]> {
    let mut out = base.to_vec();
    for (c, d) in morphs {
        let w = weights[*c];
        if w.abs() < SMALL {
            continue;
        }
        for &(v, delta) in d {
            if let Some(p) = out.get_mut(v as usize) {
                for k in 0..3 {
                    p[k] += w * delta[k];
                }
            }
        }
    }
    out
}

/// The weights to show `t` seconds after the sound started: the file's frames around
/// that moment, nothing before the sound starts or after the last frame, scaled by
/// `ease` (1 while the line is said).
pub fn weights_at(lip: &Lip, t: f64, ease: f32) -> [f32; CHANNELS] {
    let mut w = if t < 0.0 { None } else { lip.sample(t) }.unwrap_or([0.0; CHANNELS]);
    for (c, x) in w.iter_mut().enumerate() {
        *x = if driven(c) { *x * ease } else { 0.0 };
    }
    w
}

/// A shape being moved.
struct Moving {
    entity: Entity,
    rest: Handle<Mesh>,
    /// The mesh it is drawn from while its actor speaks.
    mesh: Handle<Mesh>,
    base: Vec<[f32; 3]>,
    morphs: Vec<(usize, Displacement)>,
}

/// A response being animated on one speaker.
struct Speaking {
    path: String,
    started_at: f64,
    /// Real time at which the sound is over.
    ends_at: f64,
    /// Real seconds the sound has been held so far (`nvf_play::voice::Spoken::held_secs`):
    /// the mouth's time is `now - started_at - held_secs`.
    held_secs: f64,
    lip: Arc<Lip>,
    shapes: Vec<Moving>,
    /// Real time at which the voice stopped saying the response (its end, or a cut).
    released_at: Option<f64>,
    last: [f32; CHANNELS],
}

#[derive(Resource)]
pub struct LipState {
    pub enabled: bool,
    tris: HashMap<String, Option<Arc<Tri>>>,
    speaking: HashMap<FormId, Speaking>,
    /// (speaker, path, start) of the response last looked at per speaker, animated or
    /// skipped, so that each is handled and logged once.
    seen: HashMap<FormId, (String, f64)>,
    /// Responses animated, skipped (no `.lip`, no face to move), and animated ones that
    /// were cut short.
    pub animated: u64,
    pub skipped: u64,
    pub cut: u64,
}

impl LipState {
    fn tri(&mut self, data: &GameData, path: &str) -> Option<Arc<Tri>> {
        self.tris
            .entry(path.to_string())
            .or_insert_with(|| {
                let bytes = data.vfs.exists(path).then(|| data.read(path)).flatten()?;
                match Tri::parse(&bytes) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        warn!("lip: {path}: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// The positions of a mesh.
fn positions(mesh: &Mesh) -> Option<Vec<[f32; 3]>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(p) => Some(p.clone()),
        _ => None,
    }
}

/// Set up the moving shapes of an actor: per shape with a `.tri` of its vertex count, the
/// rest positions (the model's, shaped by the actor's `.egm` as facegen.rs shapes them)
/// and the morphs. Returns the shapes and the number of morphs found.
fn prepare(st: &mut LipState, rig: &LipRig, world: &ActorWorld, data: &GameData, meshes: &mut Assets<Mesh>) -> (Vec<Moving>, usize) {
    let mut out = Vec::new();
    let mut count = 0;
    for s in &rig.shapes {
        let Some(tri) = st.tri(data, &sibling(&s.model, "tri")) else { continue };
        let Some(mut base) = positions(&s.cpu) else { continue };
        if base.len() != tri.vertices {
            continue;
        }
        // The file's own positions (mesh vertices, then the statistical morphs'
        // targets), shaped like the mesh, so a blink's targets follow the face.
        let mut shaped = tri.positions.clone();
        if let Some(fg) = &rig.facegen {
            let egm = world.facegen.lock().expect("facegen cache").egm(data, &sibling(&s.model, "egm"));
            if let Some(egm) = egm.filter(|e| base.len() <= e.vertices) {
                egm.apply(&mut base, &fg.fggs, &fg.fgga);
                egm.apply(&mut shaped, &fg.fggs, &fg.fgga);
            }
        }
        let morphs = shape_morphs(&tri, Some(&shaped));
        if morphs.is_empty() {
            continue;
        }
        let mut mesh = (*s.cpu).clone();
        mesh.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, base.clone());
        count += morphs.len();
        out.push(Moving { entity: s.entity, rest: s.rest.clone(), mesh: meshes.add(mesh), base, morphs });
    }
    (out, count)
}

/// Give the shapes their own meshes back.
fn restore(commands: &mut Commands, meshes: &mut Assets<Mesh>, shapes: &[Moving]) {
    for m in shapes {
        if let Ok(mut e) = commands.get_entity(m.entity) {
            e.try_insert(Mesh3d(m.rest.clone()));
        }
        meshes.remove(&m.mesh);
    }
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    time: Res<Time<Real>>,
    voice: Res<VoiceState>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut state: ResMut<LipState>,
    world: Option<Res<ActorWorld>>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    rigs: Query<(&ActorRig, &LipRig)>,
) {
    let st = &mut *state;
    let Some(world) = world else { return };
    if !st.enabled {
        // Switched on again, a response still being said is taken up where it is.
        st.seen.clear();
        if st.speaking.is_empty() {
            return;
        }
    }
    let now = time.elapsed_secs_f64();
    // With each response, how long its sound has been held so far (a menu that holds the
    // line, the pause menu): the mouth holds and goes on with the sound.
    // Switched off while a face moves (the settings page): no response counts as said, so
    // the faces ease out and are let go.
    let spoken: Vec<(FormId, String, f64, f64, f64)> = if st.enabled {
        voice.spoken().filter_map(|s| Some((s.speaker, s.path?.to_string(), s.started_at, s.ends_at, s.held_secs))).collect()
    } else {
        Vec::new()
    };

    // Responses that started: animate them, or say why not.
    for (speaker, path, started_at, ends_at, _) in &spoken {
        if st.seen.get(speaker).is_some_and(|(p, t)| p == path && t == started_at) {
            continue;
        }
        st.seen.insert(*speaker, (path.clone(), *started_at));
        if let Some(old) = st.speaking.remove(speaker) {
            restore(&mut commands, &mut meshes, &old.shapes);
        }
        let lip_path = fnv_voice::lip::lip_path(path);
        let who = script.as_ref().and_then(|s| s.rt.as_ref()).map_or_else(|| format!("{:08X}", speaker.0), |rt| rt.game.data.name(*speaker));
        let lip = voice.bank.as_ref().and_then(|b| b.read(&lip_path)).map(|bytes| Lip::parse(&bytes));
        let lip = match lip {
            Some(Ok(l)) => Arc::new(l),
            Some(Err(e)) => {
                st.skipped += 1;
                info!("lip: skipped {lip_path}: {e}");
                continue;
            }
            None => {
                st.skipped += 1;
                info!("lip: skipped {lip_path}: no such file (speaker {who})");
                continue;
            }
        };
        let Some((_, rig)) = rigs.iter().find(|(a, _)| a.reference == speaker.0) else {
            st.skipped += 1;
            info!("lip: skipped {lip_path}: speaker {who} has no face rig here (a creature, a robot, an activator, or not drawn)");
            continue;
        };
        let (shapes, morphs) = prepare(st, rig, &world, &data, &mut meshes);
        if shapes.is_empty() {
            st.skipped += 1;
            info!("lip: skipped {lip_path}: speaker {who} has no shape with morphs");
            continue;
        }
        for m in &shapes {
            if let Ok(mut e) = commands.get_entity(m.entity) {
                e.try_insert(Mesh3d(m.mesh.clone()));
            }
        }
        st.animated += 1;
        let tracks = lip.active_channels();
        info!(
            "lip: animating {lip_path} ({} tracks, {} applied; {} frames, {:.2} s from {:.2} s) speaker {who}: {} shapes, {morphs} morphs",
            tracks.len(),
            tracks.iter().filter(|&&c| driven(c)).count(),
            lip.frames.len(),
            lip.frames.len() as f64 / fnv_voice::lip::FRAMES_PER_SECOND,
            lip.start_seconds(),
            shapes.len(),
        );
        st.speaking.insert(
            *speaker,
            Speaking { path: path.clone(), started_at: *started_at, ends_at: *ends_at, held_secs: 0.0, lip, shapes, released_at: None, last: [0.0; CHANNELS] },
        );
    }

    // Move the faces; ease out and let go of those whose response is over.
    let mut done = Vec::new();
    for (speaker, sp) in st.speaking.iter_mut() {
        let now_said = spoken.iter().find(|(s, p, t, ..)| s == speaker && *p == sp.path && *t == sp.started_at);
        if let Some((.., ends_at, held_secs)) = now_said {
            (sp.ends_at, sp.held_secs) = (*ends_at, *held_secs);
        }
        let said = now_said.is_some();
        if !said && sp.released_at.is_none() {
            sp.released_at = Some(now);
            if st.enabled && now < sp.ends_at - CUT_SECONDS {
                st.cut += 1;
                info!("lip: cut {} {:.2} s into its {:.2} s (the response was skipped or its speaker interrupted)", sp.path, now - sp.started_at - sp.held_secs, sp.ends_at - sp.started_at - sp.held_secs);
            }
        }
        let ease = match sp.released_at {
            Some(r) => (1.0 - (now - r) / EASE_SECONDS).clamp(0.0, 1.0) as f32,
            None => 1.0,
        };
        if sp.released_at.is_some() && ease <= 0.0 {
            done.push(*speaker);
            continue;
        }
        let w = weights_at(&sp.lip, now - sp.started_at - sp.held_secs, ease);
        if w.iter().zip(&sp.last).all(|(a, b)| (a - b).abs() < SMALL) {
            continue;
        }
        sp.last = w;
        for m in &sp.shapes {
            if let Some(mut mesh) = meshes.get_mut(&m.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, morphed(&m.base, &m.morphs, &w));
            }
        }
    }
    for speaker in done {
        if let Some(sp) = st.speaking.remove(&speaker) {
            restore(&mut commands, &mut meshes, &sp.shapes);
        }
    }
}

/// Test aid `NVF_LIP_TEST_HOLD=<channel name>=<weight>[,...]`: every actor with a rig
/// holds these morph weights from the start (screenshots of single mouth shapes; no
/// line needs to be said). Not a game behaviour.
#[derive(Resource, Default)]
struct Hold {
    weights: Option<[f32; CHANNELS]>,
    held: HashMap<Entity, Vec<Moving>>,
}

fn parse_hold(v: &str) -> Result<[f32; CHANNELS], String> {
    let mut w = [0.0; CHANNELS];
    for item in v.split(',').filter(|s| !s.trim().is_empty()) {
        let (name, value) = item.split_once('=').ok_or(format!("{item}: expected <channel>=<weight>"))?;
        let c = channel(name.trim()).ok_or(format!("{name}: no such channel"))?;
        w[c] = value.trim().parse().map_err(|_| format!("{value}: not a number"))?;
    }
    Ok(w)
}

fn hold(
    mut commands: Commands,
    mut hold: ResMut<Hold>,
    mut state: ResMut<LipState>,
    world: Option<Res<ActorWorld>>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    rigs: Query<(Entity, &LipRig), Added<LipRig>>,
) {
    let (Some(w), Some(world)) = (hold.weights, world) else { return };
    for (root, rig) in &rigs {
        let (shapes, morphs) = prepare(&mut state, rig, &world, &data, &mut meshes);
        for m in &shapes {
            if let Some(mut mesh) = meshes.get_mut(&m.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, morphed(&m.base, &m.morphs, &w));
            }
            if let Ok(mut e) = commands.get_entity(m.entity) {
                e.try_insert(Mesh3d(m.mesh.clone()));
            }
        }
        info!("lip test: holding the given weights on {} shapes ({morphs} morphs) of an actor", shapes.len());
        hold.held.insert(root, shapes);
    }
}

pub struct LipsPlugin {
    pub enabled: bool,
}

impl Plugin for LipsPlugin {
    fn build(&self, app: &mut App) {
        let weights = std::env::var("NVF_LIP_TEST_HOLD").ok().and_then(|v| match parse_hold(&v) {
            Ok(w) => Some(w),
            Err(e) => {
                warn!("NVF_LIP_TEST_HOLD: {e}");
                None
            }
        });
        let holding = weights.is_some();
        app.insert_resource(LipState {
            // A held face and a spoken one would fight over the same shapes.
            enabled: self.enabled && !holding,
            tris: HashMap::new(),
            speaking: HashMap::new(),
            seen: HashMap::new(),
            animated: 0,
            skipped: 0,
            cut: 0,
        })
        .insert_resource(Hold { weights, held: HashMap::new() })
        .add_systems(Update, (drive.after(nvf_play::PlaySet::Ui), hold.run_if(move || holding)));
        if !self.enabled {
            info!("lip sync is off (--no-lip)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lip(start: i32, frames: Vec<[f32; CHANNELS]>) -> Lip {
        Lip { version: 1, flag: 1, start_frame: start, frames }
    }

    fn frame(set: &[(usize, f32)]) -> [f32; CHANNELS] {
        let mut f = [0.0; CHANNELS];
        for &(c, v) in set {
            f[c] = v;
        }
        f
    }

    #[test]
    fn mouth_blink_and_brow_channels_are_driven_look_and_head_are_not() {
        let names: Vec<&str> = (0..CHANNELS).filter(|&c| driven(c)).map(|c| CHANNEL_NAMES[c]).collect();
        assert_eq!(names.len(), 26);
        assert!(names.contains(&"Aah") && names.contains(&"W") && names.contains(&"BlinkRight") && names.contains(&"BrowUpLeft"));
        for n in ["LookDown", "LookLeft", "LookRight", "LookUp", "HeadPitch", "HeadRoll", "HeadYaw"] {
            assert!(!names.contains(&n), "{n}");
        }
    }

    #[test]
    fn weights_follow_the_sounds_clock() {
        // Two frames before the sound, then Aah opening; a head turn rides along.
        let l = lip(-2, vec![frame(&[(0, 0.9)]), frame(&[(0, 0.8)]), frame(&[(0, 0.2), (30, 0.1)]), frame(&[(0, 0.6), (16, 1.0)])]);
        // At the sound's start: frame 2; the head channel is not applied.
        let w = weights_at(&l, 0.0, 1.0);
        assert!((w[0] - 0.2).abs() < 1e-6);
        assert_eq!(w[30], 0.0);
        // Half a frame later: between frames 2 and 3.
        let w = weights_at(&l, 0.5 / 30.0, 1.0);
        assert!((w[0] - 0.4).abs() < 1e-5 && (w[16] - 0.5).abs() < 1e-5);
        // Past the last frame, and before the sound: rest.
        assert_eq!(weights_at(&l, 1.0, 1.0), [0.0; CHANNELS]);
        assert_eq!(weights_at(&l, -0.01, 1.0), [0.0; CHANNELS]);
        // Easing out scales everything.
        assert!((weights_at(&l, 1.0 / 30.0, 0.5)[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn morphs_add_on_top_of_the_shaped_face() {
        let base = vec![[1.0, 1.0, 1.0], [2.0, 2.0, 2.0], [3.0, 3.0, 3.0]];
        let morphs = vec![(0usize, vec![(1u32, [0.0, 0.0, -2.0])]), (2, vec![(1, [1.0, 0.0, 0.0]), (2, [0.0, 4.0, 0.0])]), (16, vec![(9, [1.0; 3])])];
        let w = frame(&[(0, 0.5), (2, 0.25), (16, 1.0)]);
        let p = morphed(&base, &morphs, &w);
        assert_eq!(p, vec![[1.0, 1.0, 1.0], [2.25, 2.0, 1.0], [3.0, 4.0, 3.0]]);
        // At rest nothing moves.
        assert_eq!(morphed(&base, &morphs, &[0.0; CHANNELS]), base);
    }

    #[test]
    fn a_shapes_morphs_come_from_its_tri_by_label() {
        // A three-vertex "mouth" with Aah, an empty BMP, an expression, a blink and a look.
        let mut b = b"FRTRI003".to_vec();
        for x in [3i32, 0, 0, 0, 0, 0, 0, 3, 2, 2] {
            b.extend(x.to_le_bytes());
        }
        b.extend([0u8; 16]);
        for p in [[0.0f32; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, -0.5], [0.0, 1.0, 0.5]] {
            for c in p {
                b.extend(c.to_le_bytes());
            }
        }
        let string = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as i32 + 1).to_le_bytes());
            b.extend(s.as_bytes());
            b.push(0);
        };
        for (label, deltas) in [("Aah", [[0i16, 0, 0], [0, 0, -4], [0, 0, 0]]), ("BMP", [[0; 3]; 3]), ("Happy", [[1; 3]; 3])] {
            string(&mut b, label);
            b.extend(0.5f32.to_le_bytes());
            for d in deltas {
                for c in d {
                    b.extend(c.to_le_bytes());
                }
            }
        }
        for (label, index) in [("BlinkLeft", 1u32), ("LookUp", 2)] {
            string(&mut b, label);
            b.extend(1i32.to_le_bytes());
            b.extend(index.to_le_bytes());
        }
        let tri = Tri::parse(&b).unwrap();
        let morphs = shape_morphs(&tri, None);
        // Aah and the blink; BMP moves nothing, Happy is no channel, LookUp is not driven.
        assert_eq!(morphs.len(), 2);
        assert_eq!(morphs[0], (0, vec![(1, [0.0, 0.0, -2.0])]));
        assert_eq!(morphs[1], (16, vec![(1, [0.0, 0.0, -0.5])]));
    }

    #[test]
    fn hold_aid_parses() {
        let w = parse_hold("BigAah=1, blinkleft=0.5").unwrap();
        assert_eq!((w[1], w[16]), (1.0, 0.5));
        assert!(parse_hold("Nose=1").is_err());
        assert!(parse_hold("Aah").is_err());
    }
}
