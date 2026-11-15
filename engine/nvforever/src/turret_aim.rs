//! Turrets keep their base where it was placed and turn their head (issue #2561).
//!
//! - **The skeletons** [documented: `meshes\creatures\sentryturret\skeleton.nif`, the
//!   floor turrets, and `meshes\creatures\minisentryturret\skeleton.nif`, the ceiling
//!   ones, read with `fnvtool nif decode`]: `Scene Root` → `Bip01` → `Bip01 NonAccum` →
//!   `Bip01 Base` → `Bip01 Yaw` → (`Bip01 Piston` →) `Bip01 Pitch` → `ProjectileNode`. The
//!   `Bip01 Yaw` body hangs from the base by a hinge (`bhkHingeConstraint`) with a
//!   `bhkOrientHingedBodyAction`, the `Bip01 Pitch` body by a limited hinge. So the
//!   skeleton is built to turn at `Bip01 Yaw`, the base standing still [hypothesis: that
//!   the original turns that bone to the target; the owner's screenshots of 2026-11-14
//!   show ours turning base and all, which the original does not].
//! - **Before**: a rooted actor's turn to its target (`fnv_ai::Ai::rooted_turn`, #1537)
//!   is its heading, and the heading is the rotation of the drawn actor's root, so the
//!   whole model turned.
//! - **Now**: the heading stays what the simulation aims, detects and fires by. In the
//!   drawing, every actor whose skeleton has a `Bip01 Yaw` bone has the turn away from
//!   its placed heading taken off at its top bone (`Bip01`) and put back at `Bip01 Yaw`:
//!   the base and all under it but the yaw bone stand as placed, the yaw bone and what
//!   hangs from it (the gun, the muzzle `ProjectileNode`) face where the actor aims. Done
//!   after `locomotion::drive` has posed the skeleton, on top of the animated pose, as
//!   head tracking does (`crate::head_look`).
//! - Not built: the pitch (`Bip01 Pitch` up and down to the target's height); the gun
//!   stays level as before. A dead turret (its ragdoll has the bones) is left alone.
//! - Who: by the bone, not by a list. In the base game also `nvmrhouse`, `nvfestus` and
//!   `nvsporeplant` have a `Bip01 Yaw` (docs/spec-notes/nif-node-audit.md row 10); they
//!   turn the same way if their heading ever leaves the placed one (Mr. House's screen
//!   never turns, #2150).
//!
//! Log: `turret aim: <skeleton>: the base stays as placed, Bip01 Yaw turns` once per
//! skeleton; and once per actor, when it has stood turned more than [`SAY_TURN`] from its
//! placed heading for [`SAY_FRAMES`] frames, `turret aim: <reference>: turned <n>° from
//! where it was placed: its base <b>°, its Bip01 Yaw <y>°`, the two bones' turns measured
//! on the drawn skeleton (their world rotations now against those they had unturned).

use crate::limbs::ActorRig;
use crate::ragdoll::Corpse;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::sim::SimState;
use std::collections::HashSet;

/// The bone the turn is taken off at (the skeleton's first bone) and the one it is put
/// back at [documented: the skeletons, module notes].
const TOP_BONE: &str = "Bip01";
const YAW_BONE: &str = "Bip01 Yaw";

/// A drawn actor whose head turns on its base (on the actor's root).
#[derive(Component, Debug)]
pub struct YawBase {
    top: Entity,
    yaw: Entity,
    /// The root's rotation as the reference was placed.
    placed: Quat,
    /// What was written over each bone last frame: the animation's rotation then, and
    /// the rotation written (`crate::head_look::animated_rotation`).
    written: [Option<(Quat, Quat)>; 2],
    reference: u32,
    /// The two bones' world rotations while the actor stood unturned (the last such
    /// frame), for the log's measure.
    unturned: Option<[Quat; 2]>,
    /// Frames stood turned past [`SAY_TURN`]; `None` once the log line is written.
    turned_frames: Option<u32>,
}

/// The turn from the placed heading, and the frames it is held, before the log says how
/// the bones stand (module notes).
const SAY_TURN: f32 = 0.5;
const SAY_FRAMES: u32 = 20;

/// The angle between two rotations, degrees.
fn degrees_between(a: Quat, b: Quat) -> f32 {
    a.angle_between(b).to_degrees()
}

/// The rotations to put before the top bone's and the yaw bone's own, so that with the
/// actor's root turned by `turn` from where it was placed (a rotation in the frame the
/// root stands in) the top bone stands as placed and the yaw bone is turned by `turn`.
/// `above_top` and `above_yaw`: the rotation from that frame down to each bone's parent,
/// the root's included, as the animation left them.
pub fn base_and_yaw(turn: Quat, above_top: Quat, above_yaw: Quat) -> (Quat, Quat) {
    (above_top.inverse() * turn.inverse() * above_top, above_yaw.inverse() * turn * above_yaw)
}

/// The rotation from the frame `root` stands in down to `bone`'s parent: the local
/// rotations from `root` (included) to the parent of `bone`, in order. `None` when
/// `bone` does not hang under `root`.
fn rotation_above(bone: Entity, root: Entity, parents: &Query<&ChildOf>, locals: &Query<&mut Transform>) -> Option<Quat> {
    let mut chain = Vec::new();
    let mut at = bone;
    // A skeleton is a few bones deep; the bound stops a loop in a broken hierarchy.
    for _ in 0..64 {
        at = parents.get(at).ok()?.parent();
        chain.push(locals.get(at).ok()?.rotation);
        if at == root {
            return Some(chain.into_iter().rev().fold(Quat::IDENTITY, |acc, r| acc * r));
        }
    }
    None
}

fn attach(mut commands: Commands, sim: Option<Res<SimState>>, new: Query<(Entity, &ActorRig, &Transform), Without<YawBase>>, mut said: Local<HashSet<String>>, mut none: Local<HashSet<Entity>>) {
    for (e, rig, t) in &new {
        if none.contains(&e) {
            continue;
        }
        let bone = |name: &str| rig.skel.by_name.get(name).and_then(|i| rig.nodes.get(*i)).copied();
        let (Some(top), Some(yaw)) = (bone(TOP_BONE), bone(YAW_BONE)) else {
            none.insert(e);
            continue;
        };
        // As placed: the reference's own rotation; one made at run time (PlaceAtMe)
        // stands as it was first drawn.
        let placed = sim.as_ref().and_then(|s| s.refs.get(FormId(rig.reference))).map_or(t.rotation, |p| Quat::from_rotation_z(-p.rot[2]));
        if said.insert(fnv_vfs::normalize(&rig.skeleton)) {
            info!("turret aim: {}: the base stays as placed, {YAW_BONE} turns", fnv_vfs::normalize(&rig.skeleton));
        }
        commands.entity(e).insert(YawBase { top, yaw, placed, written: [None; 2], reference: rig.reference, unturned: None, turned_frames: Some(0) });
    }
}

fn aim(mut actors: Query<(Entity, &mut YawBase), Without<Corpse>>, parents: Query<&ChildOf>, mut locals: Query<&mut Transform>, globals: Query<&GlobalTransform>) {
    for (root, mut y) in &mut actors {
        let Ok(root_rotation) = locals.get(root).map(|t| t.rotation) else { continue };
        let (top, yaw) = (y.top, y.yaw);
        // The bones as the animation has them: what was written last frame is taken off
        // first, so the rotations above the yaw bone are the animated ones.
        let mut animated = [Quat::IDENTITY; 2];
        for (i, bone) in [top, yaw].into_iter().enumerate() {
            let Ok(mut t) = locals.get_mut(bone) else { continue };
            animated[i] = crate::head_look::animated_rotation(t.rotation, y.written[i]);
            if t.rotation != animated[i] {
                t.rotation = animated[i];
            }
        }
        let turn = root_rotation * y.placed.inverse();
        // The log's measure, on the skeleton as drawn last frame.
        let drawn = globals.get(top).ok().zip(globals.get(yaw).ok()).map(|(a, b)| [a.rotation(), b.rotation()]);
        let turned = turn.angle_between(Quat::IDENTITY);
        if turned < 1e-3 {
            y.unturned = drawn.or(y.unturned);
        } else if let (Some(frames), Some(was), Some(now)) = (y.turned_frames, y.unturned, drawn) {
            if turned < SAY_TURN {
                y.turned_frames = Some(0);
            } else if frames + 1 >= SAY_FRAMES {
                info!(
                    "turret aim: {:08X}: turned {:.0}° from where it was placed: its base {:.0}°, its {YAW_BONE} {:.0}°",
                    y.reference,
                    turned.to_degrees(),
                    degrees_between(was[0], now[0]),
                    degrees_between(was[1], now[1])
                );
                y.turned_frames = None;
            } else {
                y.turned_frames = Some(frames + 1);
            }
        }
        if turn.abs_diff_eq(Quat::IDENTITY, 1e-5) {
            y.written = [None; 2];
            continue;
        }
        let (Some(above_top), Some(above_yaw)) = (rotation_above(top, root, &parents, &locals), rotation_above(yaw, root, &parents, &locals)) else { continue };
        let (at_top, at_yaw) = base_and_yaw(turn, above_top, above_yaw);
        for (i, (bone, before)) in [(top, at_top), (yaw, at_yaw)].into_iter().enumerate() {
            let Ok(mut t) = locals.get_mut(bone) else { continue };
            let w = (before * animated[i]).normalize();
            t.rotation = w;
            y.written[i] = Some((animated[i], w));
        }
    }
}

pub struct TurretAimPlugin;

impl Plugin for TurretAimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach, aim).chain().after(crate::locomotion::drive).after(crate::head_look::track));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Quat, b: Quat) -> bool {
        a.abs_diff_eq(b, 1e-5) || a.abs_diff_eq(-b, 1e-5)
    }

    /// Issue #2561: the root turned a quarter from where it was placed; the bones between
    /// the root and the yaw bone tilted as a skeleton's are. After the two rotations the
    /// top bone stands in the world as it did when placed, and the yaw bone is turned by
    /// the quarter from where it stood when placed.
    #[test]
    fn the_base_stays_and_the_yaw_bone_turns() {
        let placed = Quat::from_rotation_z(0.4);
        let turn = Quat::from_rotation_z(-FRAC_PI_2);
        let root = turn * placed;
        // Local rotations: the top bone, the base under it, the yaw bone.
        let (top, base, yaw) = (Quat::from_rotation_x(0.3), Quat::from_rotation_y(-1.1), Quat::from_rotation_x(0.7));
        let (at_top, at_yaw) = base_and_yaw(turn, root, root * top * base);
        let (top_now, yaw_now) = (at_top * top, at_yaw * yaw);
        // In the world: as placed, the top bone was placed * top.
        assert!(close(root * top_now, placed * top));
        // The base under it follows the top bone: as placed too.
        assert!(close(root * top_now * base, placed * top * base));
        // The yaw bone: turned by `turn` from where it stood when placed.
        assert!(close(root * top_now * base * yaw_now, turn * placed * top * base * yaw));
        // No turn: nothing to put before either bone.
        let (a, b) = base_and_yaw(Quat::IDENTITY, placed, placed * top * base);
        assert!(close(a, Quat::IDENTITY) && close(b, Quat::IDENTITY));
    }
}
