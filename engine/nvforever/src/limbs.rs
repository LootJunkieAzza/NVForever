//! Body parts posed from the animated skeleton (docs/design/combat.md §5.9, step C8):
//! each drawn actor's body part data (BPTD) is matched to its skeleton once, and every
//! frame the parts' bone chains are published to `nvf_play::hitvol::ActorParts` in game
//! space, where they become one hit capsule per bone segment.
//!
//! - **Which BPTD** (`CombatForms::body_parts_for`): a creature's own (CREA `PNAM`), else
//!   the one whose model is the actor's skeleton (`DefaultBodyPartData` for
//!   `characters\_male\skeleton.nif`) [hypothesis, combat.md §5.9].
//! - **A part's bones** [design]: from the part's node down to its end. When the part's
//!   own bones (its subtree, without other parts' subtrees) lead on to other parts, the
//!   end is the farthest of those connecting bones (the torso runs from `Bip01` up to the
//!   neck); otherwise the farthest `Bip01` bone below it (an arm to the fingers, the head
//!   to the top of the head). The chain does not start on the skeleton's root bone or its
//!   `NonAccum` node: `Bip01`, the torso's node in `DefaultBodyPartData`, stands at the
//!   feet [observed: posed segments, 2026-10-09], so the torso starts at the next joint
//!   (the pelvis). Joints closer than [`MIN_SEGMENT`] to the last one kept are merged.

use bevy::prelude::*;
use fnv_combat::data::Limb;
use fnv_sim::FormId;
use nvf_assets::actor::PreparedSkeleton;
use nvf_play::combat::Combat;
use nvf_play::hitvol::{ActorParts, PartSegment, PosedParts};
use std::sync::Arc;

/// Joints closer than this to the previous kept joint are merged, game units [design].
pub const MIN_SEGMENT: f32 = 4.0;

/// An actor's skeleton as spawned, until its parts are matched ([`LimbRig`]).
#[derive(Component, Clone)]
pub struct ActorRig {
    pub reference: u32,
    pub base: u32,
    pub skeleton: String,
    pub skel: Arc<PreparedSkeleton>,
    /// The skeleton's node entities, in `skel` order.
    pub nodes: Vec<Entity>,
}

/// An actor's body parts on its node entities.
#[derive(Component, Clone, Debug)]
pub struct LimbRig {
    pub data: FormId,
    /// (part index, kind, the chain's node entities from the part's node to its end).
    pub chains: Vec<(u8, Limb, Vec<Entity>)>,
}

pub struct LimbsPlugin;

impl Plugin for LimbsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, rig_limbs.run_if(resource_exists::<Combat>)).add_systems(
            PostUpdate,
            publish_limbs.after(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<ActorParts>),
        );
    }
}

/// The node chains of a skeleton's body parts (see the module notes): for each part
/// whose node the skeleton has, its index and the nodes from its node to its end. `bind`
/// holds the nodes' bind-pose positions.
pub fn part_chains(names: &[Option<String>], parents: &[Option<usize>], bind: &[Vec3], part_nodes: &[Option<&str>]) -> Vec<(usize, Vec<usize>)> {
    let find = |n: &str| names.iter().position(|x| x.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(n)));
    let nodes: Vec<Option<usize>> = part_nodes.iter().map(|n| n.and_then(find)).collect();
    let is_part = |i: usize| nodes.contains(&Some(i));
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    for (i, p) in parents.iter().enumerate() {
        if let Some(p) = p {
            children[*p].push(i);
        }
    }
    let is_ancestor = |a: usize, mut b: usize| {
        while let Some(p) = parents[b] {
            if p == a {
                return true;
            }
            b = p;
        }
        false
    };
    let mut out = Vec::new();
    for (k, n) in nodes.iter().enumerate() {
        let Some(n) = *n else { continue };
        // The part's own bones: its subtree without other parts' subtrees.
        let mut own = Vec::new();
        let mut stack = vec![n];
        while let Some(i) = stack.pop() {
            own.push(i);
            stack.extend(children[i].iter().copied().filter(|&c| !is_part(c)));
        }
        let others: Vec<usize> = nodes.iter().flatten().copied().filter(|&o| o != n && is_ancestor(n, o)).collect();
        let connecting: Vec<usize> = own.iter().copied().filter(|&i| i != n && others.iter().any(|&o| is_ancestor(i, o))).collect();
        let candidates: Vec<usize> = if connecting.is_empty() {
            own.iter().copied().filter(|&i| i != n && names[i].as_deref().is_some_and(|s| s.to_ascii_lowercase().starts_with("bip01"))).collect()
        } else {
            connecting
        };
        let end = candidates.into_iter().max_by(|&a, &b| bind[a].distance(bind[n]).total_cmp(&bind[b].distance(bind[n])));
        let mut path = vec![];
        if let Some(mut i) = end {
            while i != n {
                path.push(i);
                i = parents[i].expect("below the part's node");
            }
        }
        path.push(n);
        path.reverse();
        // Not from the root bone or the accumulation node (they stand at the feet).
        let root_like = |i: usize| {
            parents[i].is_none_or(|p| parents[p].is_none()) || names[i].as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("nonaccum"))
        };
        while path.len() > 1 && root_like(path[0]) {
            path.remove(0);
        }
        let start = path[0];
        // Merge joints too close to the last one kept (the end always stays).
        let mut chain = vec![start];
        for (j, &i) in path.iter().enumerate().skip(1) {
            let last = *chain.last().expect("start");
            if bind[i].distance(bind[last]) >= MIN_SEGMENT || (j + 1 == path.len() && chain.len() == 1) {
                chain.push(i);
            }
        }
        out.push((k, chain));
    }
    out
}

/// Match spawned actors' skeletons to their body part data, once.
fn rig_limbs(mut commands: Commands, combat: Res<Combat>, rigs: Query<(Entity, &ActorRig), Without<LimbRig>>) {
    for (e, rig) in &rigs {
        let mut limb = LimbRig { data: FormId(0), chains: Vec::new() };
        if let Some(data) = combat.forms.body_parts_for(FormId(rig.base), &rig.skeleton) {
            let parts = &combat.forms.body_parts[&data].parts;
            let names: Vec<Option<&str>> = parts.iter().map(|p| p.node.as_deref()).collect();
            let bind: Vec<Vec3> = rig.skel.bind_world.iter().map(|a| Vec3::from(a.translation)).collect();
            limb.data = data;
            for (k, chain) in part_chains(&rig.skel.names, &rig.skel.parents, &bind, &names) {
                let ents: Vec<Entity> = chain.iter().filter_map(|&i| rig.nodes.get(i).copied()).collect();
                limb.chains.push((k as u8, parts[k].limb(), ents));
            }
            debug!("actor {:08X}: body parts {data} on {} chains", rig.reference, limb.chains.len());
        }
        commands.entity(e).insert(limb);
    }
}

/// Publish every rigged actor's posed parts (game space) for the hit volumes.
fn publish_limbs(mut out: ResMut<ActorParts>, rigs: Query<(&ActorRig, &LimbRig, &InheritedVisibility)>, nodes: Query<&GlobalTransform>) {
    out.by_ref.clear();
    for (rig, limb, vis) in &rigs {
        if !vis.get() || limb.chains.is_empty() {
            continue;
        }
        let mut posed = PosedParts { data: limb.data, segments: Vec::new() };
        for (part, kind, chain) in &limb.chains {
            let pts: Vec<[f32; 3]> = chain.iter().filter_map(|&e| nodes.get(e).ok()).map(|g| nvf_play::space::to_game(g.translation())).collect();
            match pts.len() {
                0 => {}
                1 => posed.segments.push(PartSegment { part: *part, limb: *kind, a: pts[0], b: pts[0] }),
                _ => posed.segments.extend(pts.windows(2).map(|w| PartSegment { part: *part, limb: *kind, a: w[0], b: w[1] })),
            }
        }
        out.by_ref.insert(rig.reference, posed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small biped as the game's skeletons are laid out: the scene root, Bip01 and Bip01
    /// NonAccum at the feet, then Pelvis → (L Thigh → L Calf → L Foot), Spine → Neck →
    /// (Neck1 → Head → HeadNub), (L UpperArm → L Forearm → L Hand → Weapon).
    fn biped() -> (Vec<Option<String>>, Vec<Option<usize>>, Vec<Vec3>) {
        let nodes: [(&str, Option<usize>, [f32; 3]); 16] = [
            ("skeleton.nif", None, [0.0, 0.0, 0.0]),
            ("Bip01", Some(0), [0.0, 0.0, 0.0]),
            ("Bip01 NonAccum", Some(1), [0.0, 0.0, 0.0]),
            ("Bip01 Pelvis", Some(2), [0.0, 0.0, 70.0]),
            ("Bip01 L Thigh", Some(3), [-8.0, 0.0, 66.0]),
            ("Bip01 L Calf", Some(4), [-8.0, 0.0, 36.0]),
            ("Bip01 L Foot", Some(5), [-8.0, 0.0, 6.0]),
            ("Bip01 Spine", Some(3), [0.0, 0.0, 78.0]),
            ("Bip01 Neck", Some(7), [0.0, 0.0, 108.0]),
            ("Bip01 Neck1", Some(8), [0.0, 0.0, 112.0]),
            ("Bip01 Head", Some(9), [0.0, 0.0, 115.0]),
            ("Bip01 HeadNub", Some(10), [0.0, 0.0, 128.0]),
            ("Bip01 L UpperArm", Some(8), [-14.0, 0.0, 104.0]),
            ("Bip01 L Forearm", Some(12), [-14.0, 0.0, 80.0]),
            ("Bip01 L Hand", Some(13), [-14.0, 0.0, 58.0]),
            ("Weapon", Some(14), [-14.0, 0.0, 40.0]),
        ];
        (nodes.iter().map(|n| Some(n.0.to_string())).collect(), nodes.iter().map(|n| n.1).collect(), nodes.iter().map(|n| Vec3::from(n.2)).collect())
    }

    #[test]
    fn chains_follow_the_bones_between_parts() {
        let (names, parents, bind) = biped();
        let parts = [Some("Bip01"), Some("bip01 neck1"), Some("Bip01 L UpperArm"), Some("Bip01 L Thigh"), Some("Bip01 R Thigh")];
        let chains = part_chains(&names, &parents, &bind, &parts);
        // The torso runs from the pelvis (not Bip01 and NonAccum at the feet) up to the neck;
        // the head to its top (Head merged, too close to Neck1); the arm to the hand (Weapon
        // is no bone); the leg to the foot. The missing right thigh has no chain.
        assert_eq!(chains, vec![(0, vec![3, 7, 8]), (1, vec![9, 11]), (2, vec![12, 13, 14]), (3, vec![4, 5, 6])]);
    }

    #[test]
    fn a_lone_part_node_is_a_point() {
        let (names, parents, bind) = biped();
        let chains = part_chains(&names, &parents, &bind, &[Some("Bip01 L Foot")]);
        assert_eq!(chains, vec![(0, vec![6])]);
    }
}
