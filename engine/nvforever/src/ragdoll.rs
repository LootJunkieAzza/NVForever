//! Ragdolls of dead actors (docs/spec-notes/ragdoll.md §4): when an actor dies, the
//! Havok bodies and constraints of its skeleton (`fnv_collide::ragdoll`) become jointed
//! rigid bodies in the physics world (`fnv_phys::ragdoll`) at the pose the skeleton is
//! in, and from then on the bodies move the bones: the corpse drops and comes to rest.
//!
//! - `deaths.rs` marks a dead actor whose skeleton has a ragdoll with [`Corpse`].
//! - [`start`] puts the ragdoll in the world once the physics cell under the actor is
//!   loaded (until then the actor shows its death pose as before), and passes on the
//!   shoves of hits (`nvf_play::combat::ActorPush`), the killing one included.
//! - [`pose`] writes the bodies' poses to the bones after each physics step. Bones
//!   without a body (fingers, clavicles, twist bones) keep the pose they died in under
//!   their parent.
//! - Where a corpse came to rest is remembered by reference ([`Ragdolls::rest`]): a
//!   corpse whose ground unloads, or that is drawn again later, is put back there at rest
//!   instead of falling again [design: the original keeps corpse poses in its saves].
//! - A corpse that is hidden (a script disabled or deleted its reference) leaves the
//!   physics world.
//! - A corpse that gets under the terrain is lifted back above it (a guard, as for the
//!   player).
//! - The player's own body drops the same way when the player dies (`death_cam.rs`): its
//!   third-person rig carries a [`PlayerRig`] in place of an [`ActorRig`], so that the
//!   systems of placed actors (head tracking, hit volumes, outfits) leave it alone.

use crate::limbs::{ActorRig, LimbRig};
use crate::locomotion::Dying;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_collide::ragdoll::Ragdoll;
use fnv_collide::Iso;
use fnv_combat::data::Limb;
use fnv_nif::scene::Decoded;
use fnv_nif::NifFile;
use fnv_phys::{BodyState, RagdollStart};
use nvf_assets::data::GameData;
use nvf_play::combat::ActorPush;
use nvf_play::phys::PhysState;
use nvf_play::space;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A corpse this far under the terrain has fallen through it, game units; it is put back
/// this far above it, at most [`MAX_LIFTS`] times.
/// A ragdoll whose bodies all move slower than [`CALM_SPEED`] (game units per second)
/// and turn slower than [`CALM_SPIN`] (radians per second) for [`CALM_TIME`] seconds is
/// set at rest (`PhysWorld::set_ragdoll_at_rest`: held where it lies until a hit or a
/// blast): joints resting against their limits can keep a corpse creeping below what the
/// eye sees but above what the physics engine calls rest.
pub const CALM_SPEED: f32 = 12.0;
pub const CALM_SPIN: f32 = 0.6;
pub const CALM_TIME: f32 = 1.5;

pub const FELL_THROUGH: f32 = 150.0;
pub const LIFT_ABOVE: f32 = 20.0;
pub const MAX_LIFTS: u8 = 3;

/// The ragdoll data of skeletons, where corpses rest, and counts for the exit report.
#[derive(Resource, Default)]
pub struct Ragdolls {
    /// By skeleton path; `None` for a skeleton without a usable ragdoll.
    cache: HashMap<String, Option<Arc<Ragdoll>>>,
    /// Bone poses (game space, in the ragdoll's body order) of corpses by reference.
    pub rest: HashMap<u32, Vec<Option<Iso>>>,
    pub started: usize,
    pub restored: usize,
    pub rested: usize,
    pub lifted: usize,
    pub pushes: usize,
    /// Corpses given the pose their reference stores (`XRGD`, `deaths.rs`), and those
    /// whose stored pose did not fit their skeleton.
    pub authored: usize,
    pub authored_skipped: usize,
}

impl Ragdolls {
    /// The ragdoll of the skeleton at `skeleton` (read once): at least two bodies and a
    /// joint.
    fn data(&mut self, data: &GameData, skeleton: &str) -> Option<Arc<Ragdoll>> {
        let key = fnv_vfs::normalize(skeleton);
        self.cache
            .entry(key.clone())
            .or_insert_with(|| {
                let bytes = data.read(&key)?;
                let file = NifFile::parse(&bytes, Some(&data.schema)).ok()?;
                let dec = Decoded::new(&file, &bytes, &data.schema);
                let r = fnv_collide::ragdoll::ragdoll(&file, &dec);
                let mass: f32 = r.bodies.iter().map(|b| b.object.mass).sum();
                info!("ragdoll: {key}: {} bodies ({mass:.0} mass), {} joints{}", r.bodies.len(), r.joints.len(), if r.problems.is_empty() { String::new() } else { format!(", problems: {}", r.problems.join("; ")) });
                (r.bodies.len() >= 2 && !r.joints.is_empty()).then(|| Arc::new(r))
            })
            .clone()
    }

    /// The [`Corpse`] of an actor, when its skeleton has a ragdoll and a bone for each
    /// of its bodies.
    pub fn corpse(&mut self, data: &GameData, rig: &ActorRig) -> Option<Corpse> {
        let rag = self.data(data, &rig.skeleton)?;
        let bones: Option<Vec<usize>> = rag.bodies.iter().map(|b| b.object.node.as_ref().and_then(|n| rig.skel.by_name.get(n)).copied().filter(|&i| i > 0)).collect();
        let bones = bones?;
        let mut body_of = vec![None; rig.skel.parents.len()];
        for (b, &n) in bones.iter().enumerate() {
            body_of[n] = Some(b);
        }
        Some(Corpse { data: rag, bones, body_of, live: false, settled: false, lifts: 0, calm: 0.0 })
    }
}

/// The player's third-person rig while it lies as a ragdoll (`death_cam.rs`): the rig of
/// reference [`fnv_sim::FormId::PLAYER_REF`], on the rig's root.
#[derive(Component, Clone)]
pub struct PlayerRig(pub ActorRig);

/// The rig of a corpse: a placed actor's or the player's.
type AnyRig = AnyOf<(&'static ActorRig, &'static PlayerRig)>;

fn rig_of<'a>((actor, player): (Option<&'a ActorRig>, Option<&'a PlayerRig>)) -> &'a ActorRig {
    actor.or(player.map(|p| &p.0)).expect("AnyOf holds one of them")
}

/// A dead actor with a ragdoll (on the actor's root, next to its [`ActorRig`]).
#[derive(Component)]
pub struct Corpse {
    pub data: Arc<Ragdoll>,
    /// Per ragdoll body, its bone's index in the skeleton.
    bones: Vec<usize>,
    /// Per skeleton node, the ragdoll body on it.
    body_of: Vec<Option<usize>>,
    /// The ragdoll is in the physics world.
    pub live: bool,
    /// The bones hold the pose the ragdoll went to sleep in.
    settled: bool,
    lifts: u8,
    /// Seconds it has been nearly still.
    calm: f32,
}

pub struct RagdollPlugin;

impl Plugin for RagdollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ragdolls>()
            .add_systems(Update, pose.after(nvf_play::PlaySet::Physics).run_if(resource_exists::<PhysState>))
            .add_systems(PostUpdate, start.after(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
    }
}

/// A bone's world transform (Bevy space, any uniform scale) as a game-space pose.
fn game_iso(a: &Affine3A) -> Iso {
    let col = |c: bevy::math::Vec3A| Vec3::from(space::to_game(Vec3::from(c))).normalize_or_zero();
    let (x, y, z) = (col(a.matrix3.x_axis), col(a.matrix3.y_axis), col(a.matrix3.z_axis));
    Iso { translation: space::to_game(Vec3::from(a.translation)), rotation: [x.x, y.x, z.x, x.y, y.y, z.y, x.z, y.z, z.z] }
}

/// A game-space pose with a uniform scale as a world transform (Bevy space).
fn bevy_affine(translation: [f32; 3], r: &[f32; 9], scale: f32) -> Affine3A {
    let col = |j: usize| space::to_bevy([r[j], r[3 + j], r[6 + j]]) * scale;
    Affine3A::from_cols(col(0).into(), col(1).into(), col(2).into(), space::to_bevy(translation).into())
}

fn iso_of(b: &BodyState) -> Iso {
    Iso { translation: b.translation, rotation: b.rotation }
}

/// The body part each ragdoll body reports when hit: the part whose bone chain holds the
/// body's bone or the nearest bone above it, else the torso's, else none.
fn body_parts(rig: &ActorRig, corpse: &Corpse, limbs: Option<&LimbRig>) -> Vec<u8> {
    let Some(limbs) = limbs else { return Vec::new() };
    let part_of = |node: usize| limbs.chains.iter().find(|(_, _, chain)| chain.contains(&rig.nodes[node])).map(|c| c.0);
    let torso = limbs.chains.iter().find(|c| c.1 == Limb::Torso).map(|c| c.0);
    corpse
        .bones
        .iter()
        .map(|&b| {
            let mut i = Some(b);
            while let Some(n) = i {
                if let Some(p) = part_of(n) {
                    return p;
                }
                i = rig.skel.parents.get(n).copied().flatten();
            }
            torso.unwrap_or(fnv_phys::actors::WHOLE)
        })
        .collect()
}

/// What [`start`] reads of a corpse.
type Waiting = (AnyRig, &'static mut Corpse, Option<&'static mut Dying>, Option<&'static LimbRig>, &'static InheritedVisibility);

/// Put waiting corpses' ragdolls in the world where the ground is loaded, and pass on
/// the frame's shoves.
fn start(
    mut phys: ResMut<PhysState>,
    mut ragdolls: ResMut<Ragdolls>,
    mut pushes: MessageReader<ActorPush>,
    mut corpses: Query<Waiting>,
    globals: Query<&GlobalTransform>,
) {
    for (rig, mut c, dying, limbs, vis) in &mut corpses {
        let rig = rig_of(rig);
        if c.live || !vis.get() || c.lifts > MAX_LIFTS {
            continue;
        }
        let Ok(root) = globals.get(rig.nodes[0]) else { continue };
        let scale = root.affine().matrix3.x_axis.length();
        let stored = ragdolls.rest.get(&rig.reference).filter(|s| s.len() == c.bones.len());
        let nodes: Option<Vec<Iso>> = match stored {
            Some(s) => s.iter().copied().collect(),
            None => c.bones.iter().map(|&b| globals.get(rig.nodes[b]).ok().map(|g| game_iso(&g.affine()))).collect(),
        };
        let Some(nodes) = nodes.filter(|n| !n.is_empty()) else { continue };
        if !phys.ready_at(nodes[0].translation) {
            continue;
        }
        let parts = body_parts(rig, &c, limbs);
        let asleep = stored.is_some();
        let r = phys.world.add_ragdoll(&RagdollStart { form_id: rig.reference, ragdoll: &c.data, scale, nodes: &nodes, velocity: [0.0; 3], parts: &parts, asleep });
        debug!("ragdoll: actor {:08X}: {} bodies, {} joints, {} self-collision pairs{}{}", rig.reference, r.bodies, r.joints, r.self_pairs, if asleep { ", put back at rest" } else { "" }, if r.errors.is_empty() { String::new() } else { format!(", {} shape errors", r.errors.len()) });
        if r.bodies == 0 {
            phys.world.remove_ragdoll(rig.reference);
            c.lifts = MAX_LIFTS + 1;
            continue;
        }
        c.live = true;
        c.settled = false;
        c.calm = 0.0;
        if asleep {
            ragdolls.restored += 1;
        } else {
            ragdolls.started += 1;
        }
        // The ragdoll has the pose from here on.
        if let Some(mut d) = dying {
            d.clip = None;
        }
    }
    for p in pushes.read() {
        if phys.world.push_ragdoll(p.form_id, p.point, p.dv) {
            ragdolls.pushes += 1;
        }
    }
}

/// Write the ragdolls' bodies to their bones, and take out of the world the ragdolls
/// whose ground or whose actor is gone.
pub(crate) fn pose(
    time: Res<Time>,
    mut phys: ResMut<PhysState>,
    mut ragdolls: ResMut<Ragdolls>,
    mut corpses: Query<(AnyRig, &mut Corpse, &InheritedVisibility)>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let mut seen = HashSet::new();
    for (rig, mut c, vis) in &mut corpses {
        let rig = rig_of(rig);
        if !c.live {
            continue;
        }
        let id = rig.reference;
        if !vis.get() {
            // Disabled or deleted by a script (a raven cleans itself up): no body is left.
            phys.world.remove_ragdoll(id);
            ragdolls.rest.remove(&id);
            c.live = false;
            continue;
        }
        let Some(nodes) = phys.world.ragdoll_nodes(id) else {
            c.live = false;
            continue;
        };
        let Some(first) = nodes.iter().flatten().next().map(|b| b.translation) else { continue };
        let rest = || nodes.iter().map(|b| b.as_ref().map(iso_of)).collect::<Vec<_>>();
        if !phys.ready_at(first) {
            // The ground under it is gone: keep the pose for when it is back.
            ragdolls.rest.insert(id, rest());
            phys.world.remove_ragdoll(id);
            c.live = false;
            continue;
        }
        let lowest = nodes.iter().flatten().map(|b| b.translation[2]).fold(f32::INFINITY, f32::min);
        if let Some(t) = phys.terrain_z(first[0], first[1]).filter(|t| lowest < t - FELL_THROUGH) {
            warn!("ragdoll: actor {id:08X} fell through at ({:.0}, {:.0}, {lowest:.0}), terrain z {t:.0}: back above the ground", first[0], first[1]);
            // From where it last lay when that is known (its pose now may be torn), else
            // from its pose now; lifted so that its lowest bone is above the terrain.
            let from = ragdolls.rest.remove(&id).filter(|r| r.len() == nodes.len()).unwrap_or_else(rest);
            let low = from.iter().flatten().map(|i| i.translation[2]).fold(f32::INFINITY, f32::min);
            let at = from.iter().flatten().next().map_or(first, |i| i.translation);
            let up = phys.terrain_z(at[0], at[1]).unwrap_or(t) + LIFT_ABOVE - low;
            let lifted: Vec<Option<Iso>> = from.into_iter().map(|i| i.map(|mut i| { i.translation[2] += up; i })).collect();
            phys.world.remove_ragdoll(id);
            ragdolls.lifted += 1;
            c.lifts += 1;
            c.live = false;
            if c.lifts > MAX_LIFTS {
                continue;
            }
            // Start again from the lifted pose (awake): `start` reads the bones.
            let states: Vec<Option<BodyState>> = nodes.iter().zip(&lifted).map(|(b, l)| b.zip(*l).map(|(b, l)| BodyState { translation: l.translation, rotation: l.rotation, ..b })).collect();
            write_bones(rig, &c, &states, &globals, &mut transforms);
            continue;
        }
        seen.insert(id);
        let calm = phys.world.ragdoll_speed(id).is_some_and(|(v, w)| v < CALM_SPEED && w < CALM_SPIN);
        c.calm = if calm { c.calm + time.delta_secs() } else { 0.0 };
        if c.calm > CALM_TIME || phys.world.ragdoll_asleep(id) {
            phys.world.set_ragdoll_at_rest(id);
        }
        let asleep = phys.world.ragdoll_asleep(id);
        if asleep && c.settled {
            continue;
        }
        write_bones(rig, &c, &nodes, &globals, &mut transforms);
        if asleep {
            ragdolls.rested += 1;
            ragdolls.rest.insert(id, rest());
            debug!("ragdoll: actor {id:08X} at rest at ({:.0}, {:.0}, {:.0})", first[0], first[1], first[2]);
        }
        c.settled = asleep;
    }
    // Ragdolls whose actor is no longer drawn: remember where they lie.
    let gone: Vec<u32> = phys.world.ragdolls().filter(|id| !seen.contains(id)).collect();
    for id in gone {
        if let Some(nodes) = phys.world.ragdoll_nodes(id) {
            ragdolls.rest.insert(id, nodes.iter().map(|b| b.as_ref().map(iso_of)).collect());
        }
        phys.world.remove_ragdoll(id);
    }
}

/// Set the local transforms of the bones that have a body so that they stand where their
/// bodies are. Nodes are in parent-first order; a node without a body keeps its local
/// transform under its parent.
fn write_bones(rig: &ActorRig, c: &Corpse, bodies: &[Option<BodyState>], globals: &Query<&GlobalTransform>, transforms: &mut Query<&mut Transform>) {
    let Ok(root) = globals.get(rig.nodes[0]).map(|g| g.affine()) else { return };
    let scale = root.matrix3.x_axis.length();
    let n = rig.nodes.len().min(rig.skel.parents.len());
    let mut global: Vec<Affine3A> = Vec::with_capacity(n);
    global.push(root);
    for i in 1..n {
        let parent = global[rig.skel.parents[i].filter(|&p| p < i).unwrap_or(0)];
        let body = c.body_of[i].and_then(|b| bodies.get(b).copied().flatten());
        let g = match body {
            Some(b) => {
                // The bone's own scale in the bind pose stays on it.
                let own = rig.skel.bind_world.get(i).map_or(1.0, |w| w.matrix3.x_axis.length());
                let g = bevy_affine(b.translation, &b.rotation, scale * own);
                if let Ok(mut t) = transforms.get_mut(rig.nodes[i]) {
                    let (_, rotation, translation) = (parent.inverse() * g).to_scale_rotation_translation();
                    if rotation.is_finite() && translation.is_finite() {
                        t.translation = translation;
                        t.rotation = rotation.normalize();
                    }
                }
                g
            }
            None => parent * transforms.get(rig.nodes[i]).map_or(Affine3A::IDENTITY, |t| t.compute_affine()),
        };
        global.push(g);
    }
}

fn report(mut exit: MessageReader<AppExit>, ragdolls: Res<Ragdolls>, phys: Option<Res<PhysState>>) {
    if exit.read().last().is_none() || ragdolls.started + ragdolls.restored == 0 {
        return;
    }
    let (live, asleep) = phys.map_or((0, 0), |p| (p.world.ragdolls().count(), p.world.ragdolls().filter(|&id| p.world.ragdoll_asleep(id)).count()));
    info!(
        "ragdolls: {} started, {} put back at rest, {} came to rest, {} shoved by hits, {} lifted back above the terrain; {live} in the world at exit ({asleep} at rest); {} laid in the pose their reference stores ({} stored poses did not fit)",
        ragdolls.started, ragdolls.restored, ragdolls.rested, ragdolls.pushes, ragdolls.lifted, ragdolls.authored, ragdolls.authored_skipped
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poses_survive_the_trip_to_bevy_space_and_back() {
        // 90° about z and 30° about x, at a point, on an actor scaled 1.1.
        let (s, c) = 30f32.to_radians().sin_cos();
        let r = [0.0, -c, s, 1.0, 0.0, 0.0, 0.0, s, c];
        let t = [120.0, -40.0, 66.0];
        let a = bevy_affine(t, &r, 1.1);
        // Game z is Bevy y; the scale is on the axes.
        assert!((a.translation.y - 66.0).abs() < 1e-4 && (a.matrix3.x_axis.length() - 1.1).abs() < 1e-5);
        let back = game_iso(&a);
        assert!(back.translation.iter().zip(&t).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", back.translation);
        assert!(back.rotation.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-5), "{:?}", back.rotation);
    }
}
