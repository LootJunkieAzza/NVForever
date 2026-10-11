//! Actor hit volumes in play (docs/design/combat.md §5.2, step C3): every placed actor
//! (ACHR, ACRE) that is drawn gets a capsule in the physics world (`fnv_phys::actors`),
//! so shots, melee rays and the crosshair's collision ray can hit it.
//!
//! The capsule is sized and placed from the actor's render bounds each frame [design]:
//! the union of the world-space bounding boxes of the actor's drawn parts; feet at the
//! bottom centre, height the box's height, radius the smaller horizontal half-extent
//! (clamped to [`MIN_RADIUS`]..[`MAX_RADIUS`]). An actor that is hidden, removed by the
//! simulation or no longer drawn loses its volume. Dead actors (the script runtime's dead
//! state, combat C5) are on the DEAD_BIP layer; once a dead actor has a ragdoll in the
//! physics world, the ragdoll's bodies are what rays hit and it has no volume here.
//!
//! **Body parts** (step C8): when the renderer publishes an actor's posed body parts
//! ([`ActorParts`]: for each part of its BPTD, the segments along the part's bones), the
//! actor gets one capsule per segment instead (`PhysWorld::set_actor_parts`), so a hit
//! says which part it struck. A capsule's radius is a share of the actor's size (the
//! largest side of its bounds) by kind of limb ([`limb_radius`], [design]). Each part's
//! chain of segments is pulled in by that radius at its two ends, so the capsule's caps
//! end at the part's end joints instead of reaching into the next part (the torso's top
//! cap would otherwise cover the lower head) [design]. The torso's lower end (the pelvis)
//! stays: pulled in, it left the hips between the thighs uncovered [observed: the
//! Goodsprings Settler, 2026-10-09].
//!
//! The same bounds and posed parts say where each living actor stands in the player's
//! way (`crate::blocking`); hit volumes themselves do not stop anyone walking.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use fnv_combat::data::Limb;
use fnv_phys::{HitVolume, PartCapsule};
use fnv_sim::FormId;
use std::collections::{BTreeMap, HashMap};

use crate::blocking::{self, ActorBlocks};
use crate::phys::{PhysState, PhysSystems};
use crate::sim::SimState;
use crate::{space, PlayWorld, RefVisual};

/// Radius limits, game units [design: a thin limb pose or a T-pose bind box should not
/// make a needle or a wall].
pub const MIN_RADIUS: f32 = 10.0;
pub const MAX_RADIUS: f32 = 150.0;

/// Volumes in the world now, and the most at once (for the exit report).
#[derive(Resource, Default)]
pub struct HitVolumes {
    pub count: usize,
    pub most: usize,
    /// The living actors drawn now and where each one's feet are (game space), the
    /// player not among them.
    pub living: Vec<(u32, [f32; 3])>,
}

/// One segment of a body part, game space (step C8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartSegment {
    /// The part's index in the actor's body part data.
    pub part: u8,
    pub limb: Limb,
    pub a: [f32; 3],
    pub b: [f32; 3],
}

/// The part number of the weapon an actor holds drawn: one segment along the weapon,
/// after the body parts (no BPTD part has this index). A hit on it wears the weapon, not
/// the actor (`crate::combat`, docs/spec-notes/combat-rules.md §17).
pub const WEAPON_PART: u8 = 254;

/// An actor's body parts as posed last frame.
#[derive(Debug, Clone)]
pub struct PosedParts {
    /// The body part data (BPTD) the parts index.
    pub data: FormId,
    pub segments: Vec<PartSegment>,
    /// The weapon in the actor's hand, when it has a [`WEAPON_PART`] segment.
    pub weapon: Option<FormId>,
}

/// The posed body parts of drawn actors by reference FormID, published by the renderer
/// from the animated skeletons (step C8).
#[derive(Resource, Default)]
pub struct ActorParts {
    pub by_ref: HashMap<u32, PosedParts>,
}

impl ActorParts {
    /// The body part data an actor's posed parts index.
    pub fn data_of(&self, form_id: u32) -> Option<FormId> {
        self.by_ref.get(&form_id).map(|p| p.data)
    }
}

/// A body part capsule's radius as a share of the actor's size [design: about a human's
/// proportions at 128 units, a little generous for play].
pub fn limb_radius(limb: Limb, size: f32) -> f32 {
    let share = match limb {
        Limb::Head => 0.065,
        Limb::Torso => 0.10,
        Limb::Arm => 0.042,
        Limb::Leg => 0.058,
        Limb::Other => 0.05,
    };
    (share * size).max(2.0)
}

/// The capsules of an actor's posed parts, `size` being its largest side (see the module
/// notes): consecutive segments of one part form its chain, whose two ends are pulled in
/// by the radius (never past the next joint, or a lone segment's middle; not the torso's
/// lower end).
pub fn part_capsules(segments: &[PartSegment], size: f32) -> Vec<PartCapsule> {
    let mut out: Vec<PartCapsule> = Vec::with_capacity(segments.len());
    let mut i = 0;
    while i < segments.len() {
        let part = segments[i].part;
        let j = segments[i..].iter().position(|s| s.part != part).map_or(segments.len(), |n| i + n);
        let radius = limb_radius(segments[i].limb, size);
        for (k, s) in segments[i..j].iter().enumerate() {
            let (mut a, mut b) = (Vec3::from(s.a), Vec3::from(s.b));
            let d = b - a;
            let len = d.length();
            if len > 1e-3 {
                // A lone segment shrinks from both ends, at most to its middle; an end
                // segment of a longer chain only from its outer end, at most to its joint.
                let most = if j - i == 1 { len / 2.0 } else { len };
                let pull = |n: f32| d / len * n.min(most);
                if k == 0 && segments[i].limb != Limb::Torso {
                    a += pull(radius);
                }
                if i + k + 1 == j {
                    b -= pull(radius);
                }
            }
            out.push(PartCapsule { part, a: a.to_array(), b: b.to_array(), radius });
        }
        i = j;
    }
    out
}

pub struct HitVolumePlugin;

impl Plugin for HitVolumePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HitVolumes>()
            .init_resource::<ActorParts>()
            .init_resource::<ActorBlocks>()
            .add_systems(Update, sync_volumes.after(PhysSystems).in_set(crate::PlaySet::Physics))
            .add_systems(Last, report);
    }
}

/// The game-space box of a part: the eight corners of its local bounds through its
/// transform, then into game space.
fn game_box(aabb: &Aabb, gt: &GlobalTransform) -> ([f32; 3], [f32; 3]) {
    let t = gt.affine();
    let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for i in 0..8 {
        let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
        let p = space::to_game(t.transform_point3(c + h * s));
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// The capsule for an actor's game-space bounds.
pub fn volume_for(lo: [f32; 3], hi: [f32; 3], dead: bool) -> HitVolume {
    let half_x = (hi[0] - lo[0]) / 2.0;
    let half_y = (hi[1] - lo[1]) / 2.0;
    HitVolume {
        feet: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]],
        radius: half_x.min(half_y).clamp(MIN_RADIUS, MAX_RADIUS),
        height: (hi[2] - lo[2]).max(2.0 * MIN_RADIUS),
        dead,
    }
}

fn is_actor(play: &PlayWorld, form_id: u32) -> bool {
    play.index.refs.get(&form_id).is_some_and(|e| &e.sig == b"ACHR" || &e.sig == b"ACRE")
}

#[allow(clippy::too_many_arguments)]
fn sync_volumes(
    play: Res<PlayWorld>,
    sim: Res<SimState>,
    script: Option<Res<crate::script::ScriptState>>,
    posed: Res<ActorParts>,
    parts: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    mut phys: ResMut<PhysState>,
    mut vols: ResMut<HitVolumes>,
    mut blocks: ResMut<ActorBlocks>,
) {
    let mut bounds: BTreeMap<u32, ([f32; 3], [f32; 3])> = BTreeMap::new();
    for (r, aabb, gt, vis) in &parts {
        if !vis.get() || !is_actor(&play, r.0) {
            continue;
        }
        let (lo, hi) = game_box(aabb, gt);
        let e = bounds.entry(r.0).or_insert((lo, hi));
        for k in 0..3 {
            e.0[k] = e.0[k].min(lo[k]);
            e.1[k] = e.1[k].max(hi[k]);
        }
    }
    // A corpse with a ragdoll is hit on its ragdoll bodies (`fnv_phys::ragdoll`).
    bounds.retain(|&id, _| sim.present(id) && !phys.world.has_ragdoll(id));
    let stale: Vec<u32> = phys.world.actor_volumes().filter(|id| !bounds.contains_key(id)).collect();
    for id in stale {
        phys.world.remove_actor_volume(id);
    }
    blocks.list.clear();
    vols.living.clear();
    for (&id, &(lo, hi)) in &bounds {
        let dead = crate::combat::is_dead(script.as_deref(), id);
        let v = volume_for(lo, hi, dead);
        if !dead && id != FormId::PLAYER_REF.0 {
            vols.living.push((id, v.feet));
        }
        let posed = posed.by_ref.get(&id).filter(|p| !p.segments.is_empty());
        // In the player's way where its body is now, unless the character controller
        // walks it (its block is where its character is, `crate::player`).
        if !dead && id != FormId::PLAYER_REF.0 && !phys.npcs.contains_key(&id) {
            let block = blocking::block_for(id, lo, hi, posed.and_then(|p| blocking::torso(&p.segments)));
            // One that stands where it may be pushed is where its owner has it, with
            // the pushes still to come.
            let block = blocks.standing_now(id).map_or(block, |at| block.standing_at(at));
            blocks.list.push(block);
        }
        match posed {
            Some(p) => {
                let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f32, f32::max);
                let caps = part_capsules(&p.segments, size);
                if phys.world.actor_part_count(id) != caps.len() {
                    debug!("actor {id:08X}: {} part capsules (size {size:.0}): {caps:?}", caps.len());
                }
                phys.world.set_actor_parts(id, v, &caps);
            }
            None => phys.world.set_actor_volume(id, v),
        }
    }
    if bounds.len() != vols.count {
        debug!("actor hit volumes: {}", bounds.len());
    }
    vols.count = bounds.len();
    vols.most = vols.most.max(vols.count);
}

/// At exit: the counts, and a probe of each volume: a level ray from 150 units away
/// (from the side the player is on) at its aim point (the torso with body parts, else the
/// capsule's middle) should hit that actor first,
/// unless something stands in between.
fn report(mut exit: MessageReader<AppExit>, vols: Res<HitVolumes>, blocks: Res<ActorBlocks>, mut phys: ResMut<PhysState>) {
    if exit.read().last().is_none() {
        return;
    }
    let ids: Vec<u32> = phys.world.actor_volumes().collect();
    let player = phys.player.as_ref().map(|c| c.feet);
    let (mut hit, mut other, mut none) = (0, 0, 0);
    for id in &ids {
        let Some(c) = phys.world.actor_aim_point(*id) else { continue };
        let (dx, dy) = player.map_or((1.0, 0.0), |p| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 1e-3 { (dx / l, dy / l) } else { (1.0, 0.0) }
        });
        let from = [c[0] + dx * 150.0, c[1] + dy * 150.0, c[2]];
        match phys.ray(from, [-dx, -dy, 0.0], 300.0, fnv_phys::Filter::default()) {
            Some(h) if matches!(h.owner, fnv_phys::Owner::Actor { form_id, layer, .. } if form_id == *id && layer == fnv_phys::layers::BIPED) => hit += 1,
            Some(_) => other += 1,
            None => none += 1,
        }
    }
    let with_parts = ids.iter().filter(|&&id| phys.world.actor_part_count(id) > 0).count();
    info!(
        "actor hit volumes: {} at exit ({with_parts} with body parts), at most {} at once; probe rays: {hit} hit their actor, {other} hit something else first, {none} hit nothing",
        vols.count, vols.most
    );
    // The block nearest to the player and how far the two are from touching.
    let nearest = phys.player.as_ref().and_then(|c| {
        let room = |b: &blocking::Block| b.room([c.feet[0], c.feet[1]], c.params.radius);
        blocks.list.iter().min_by(|a, b| room(a).total_cmp(&room(b))).map(|b| {
            format!("; the nearest, {:08X} round ({:.0}, {:.0}) to ({:.0}, {:.0}), is {:.1} units from touching the player at ({:.0}, {:.0}, {:.0})", b.actor, b.a[0], b.a[1], b.b[0], b.b[1], room(b), c.feet[0], c.feet[1], c.feet[2])
        })
    });
    info!(
        "actors in the player's way: {} at exit; they changed the player's step {} times, a walking actor's {} times{}",
        blocks.list.len(),
        blocks.steps_blocked,
        blocks.steps_actor_blocked,
        nearest.unwrap_or_default()
    );
    let [player, actors, pushed] = blocks.steps_pushing;
    // Those that stand in the way and may be pushed, nearest to the player first.
    let from = phys.player.as_ref().map_or([0.0; 2], |c| [c.feet[0], c.feet[1]]);
    let mut standing: Vec<(f32, u32, [f32; 2])> =
        blocks.list.iter().filter(|b| b.pushable).map(|b| ((b.a[0] - from[0]).hypot(b.a[1] - from[1]), b.actor, b.a)).collect();
    standing.sort_by(|a, b| a.0.total_cmp(&b.0));
    let nearest: Vec<String> = standing.iter().take(6).map(|(d, id, at)| format!("{id:08X} at ({:.0}, {:.0}), {d:.0} units away", at[0], at[1])).collect();
    info!(
        "actors pushed aside: {} may be pushed at exit{}{}; the player pushed one in {player} steps, a walking actor another in {actors}, one the player in {pushed}",
        standing.len(),
        if nearest.is_empty() { "" } else { ": " },
        nearest.join("; ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_from_bounds() {
        // A person about 128 tall, 60 wide (arms) and 30 deep.
        let v = volume_for([-30.0, -15.0, 0.0], [30.0, 15.0, 128.0], false);
        assert_eq!(v.feet, [0.0, 0.0, 0.0]);
        assert_eq!((v.radius, v.height), (15.0, 128.0));
        // A flat thing gets the minimum radius and height.
        let v = volume_for([0.0, 0.0, 5.0], [2.0, 2.0, 6.0], true);
        assert_eq!((v.radius, v.height, v.dead), (MIN_RADIUS, 2.0 * MIN_RADIUS, true));
    }

    #[test]
    fn limb_radii_scale_with_size() {
        // A 128-unit human: a torso about 26 across, arms about 11.
        assert!((limb_radius(Limb::Torso, 128.0) - 12.8).abs() < 0.01);
        assert!(limb_radius(Limb::Arm, 128.0) < limb_radius(Limb::Leg, 128.0));
        assert_eq!(limb_radius(Limb::Head, 10.0), 2.0);
    }

    #[test]
    fn part_chains_end_at_their_joints() {
        let seg = |part, limb, a: [f32; 3], b: [f32; 3]| PartSegment { part, limb, a, b };
        // A torso of two segments from z 70 to 110 (radius 12.8 at size 128), a head of one
        // from 112 to 128 (radius 8.32).
        let segs = [seg(1, Limb::Torso, [0.0, 0.0, 70.0], [0.0, 0.0, 90.0]), seg(1, Limb::Torso, [0.0, 0.0, 90.0], [0.0, 0.0, 110.0]), seg(0, Limb::Head, [0.0, 0.0, 112.0], [0.0, 0.0, 128.0])];
        let caps = part_capsules(&segs, 128.0);
        assert_eq!(caps.len(), 3);
        // The torso's top end moves in by the radius; its lower end and the joint between
        // stay.
        assert!(caps[0].a[2] == 70.0 && caps[0].b[2] == 90.0);
        assert!(caps[1].a[2] == 90.0 && (caps[1].b[2] - 97.2).abs() < 1e-3);
        // The torso's top cap ends at 110; the head no longer starts inside it. A short
        // segment collapses to its middle.
        assert!((caps[1].b[2] + caps[1].radius - 110.0).abs() < 1e-3);
        assert_eq!((caps[2].a[2], caps[2].b[2]), (120.0, 120.0));
    }
}
