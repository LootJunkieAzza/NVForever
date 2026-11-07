//! A downed player's body on the others' screens (queue items 23 and 29, owner playtest
//! 2026-11-06).
//!
//! - **Where it lies.** While a player is down, its puppet's pelvis here is where it is
//!   revived from: [`Peer::body`], told to the crosshair's targeting through
//!   `nvf_play::coop_social::Mate::body`, so the "Revive" rollover is on the body where it
//!   slid or rolled to, not where the player stood when it went down.
//! - **Getting up.** Revived, its own game gets the player up from where its body lies
//!   with the knockdown's get-up clip (`player_knockdown.rs`). Here its puppet does the
//!   same from its own ragdoll's pose, as a knocked-out actor gets up (`knockouts.rs`):
//!   the ragdoll leaves the world, the get-up clip of the side it lies on plays once as a
//!   [`Dying`] clip from its first frame, laid so that the clip's pelvis and pelvis → head
//!   line are where the ragdoll's were, and when the clip is over the puppet follows its
//!   player again. It starts when the revived player's own body starts (its state stops
//!   saying `flag::LYING`), so both screens see it get up together. Without a body, a ragdoll pose or a clip, the puppet is drawn anew
//!   standing, as before. Log: `co-op: <name> gets up face up (<clip>, 2.3 s)`.

use super::{label, Coop, Puppet};
use crate::knockouts::{bone, face_up, pelvis_head};
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Locomotion};
use crate::ragdoll::{Corpse, Ragdolls};
use bevy::prelude::*;
use nvf_play::RefVisual;
use std::sync::Arc;

/// A revived player's puppet getting up ([`super::Peer::rising`]).
#[derive(Clone, Copy, Debug)]
pub(super) enum Rising {
    /// Revived at `since` (seconds of `Time`): its body still lies.
    Start { since: f32 },
    /// Its get-up clip starts this frame: the ragdoll's pelvis and head (world space).
    Align { pelvis: Vec3, head: Vec3, length: f32 },
    /// The clip plays until then (seconds of `Time`).
    Up { until: f32 },
}

/// A get-up that has not started this long after the revive is given up: the puppet is
/// drawn anew, standing (seconds) [design: the revived game's body lies about 2.5 s,
/// observed 2026-11-07].
const START_WAIT: f32 = 6.0;

type Body = (Entity, &'static RefVisual, &'static ActorRig, Option<&'static Locomotion>, &'static mut Transform, &'static GlobalTransform, Has<Corpse>);

/// Where each downed puppet's body lies, and the revived ones' get-ups (module notes).
#[allow(clippy::too_many_arguments)]
pub(super) fn bodies(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
    mut ragdolls: ResMut<Ragdolls>,
    mut puppets: Query<Body, With<Puppet>>,
    globals: Query<&GlobalTransform>,
    parents: Query<&ChildOf>,
) {
    let now = time.elapsed_secs();
    for (&id, p) in coop.peers.iter_mut() {
        let found = p.root.and_then(|e| puppets.get_mut(e).ok());
        if p.down {
            p.body = found.as_ref().and_then(|(_, _, rig, ..)| bone(rig, &globals, "Bip01 Pelvis")).map(nvf_play::space::to_game);
            continue;
        }
        let Some(rising) = p.rising else { continue };
        let name = label(id, &p.name);
        let Some((e, v, rig, lo, mut t, gt, corpse)) = found else {
            if matches!(rising, Rising::Start { since } if now - since > START_WAIT) {
                info!("co-op: {name}'s body is not here to get up: drawn standing");
                p.rising = None;
                p.redraw = true;
            }
            continue;
        };
        match rising {
            // The revived player's own body still lies (`flag::LYING`): so does this one.
            Rising::Start { since } if now - since < START_WAIT && p.samples.back().is_some_and(|(_, s)| s.has(nvf_coop::net::flag::LYING)) => {}
            Rising::Start { since } => {
                let lying = pelvis_head(rig, &globals);
                let side = usize::from(face_up(rig, &globals) == Some(false));
                let clip: Option<Arc<_>> = lo.and_then(|lo| lo.set.getup[side].clone().or_else(|| lo.set.getup[1 - side].clone()));
                let (Some(clip), Some((pelvis, head)), true) = (clip, lying, corpse) else {
                    if now - since > START_WAIT || !corpse {
                        info!("co-op: {name} stands up (no ragdoll or get-up clip): drawn standing");
                        p.rising = None;
                        p.redraw = true;
                    }
                    continue;
                };
                if let Some(ph) = phys.as_deref_mut() {
                    ph.world.remove_ragdoll(v.0);
                }
                ragdolls.rest.remove(&v.0);
                commands.entity(e).remove::<Corpse>();
                // Stood where its player is, then laid over the body ([`Rising::Align`]).
                if let Some(&(_, s)) = p.samples.back() {
                    t.translation = Vec3::from_array(s.feet);
                    t.rotation = Quat::from_rotation_z(-s.yaw);
                }
                let length = clip.clip.length();
                info!("co-op: {name} gets up face {} ({}, {length:.1} s)", if side == 0 { "up" } else { "down" }, clip.clip.name);
                commands.entity(e).insert(Dying { clip: Some(clip), t: 0.0 });
                p.rising = Some(Rising::Align { pelvis, head, length });
            }
            Rising::Align { pelvis, head, length } => {
                // The clip's first pose is on the bones: lay it over where the body lay
                // (`knockouts.rs`, the same turn and shift).
                if let Some((pl, hd)) = pelvis_head(rig, &globals) {
                    let flat = |v: Vec3| Vec2::new(v.x, v.z);
                    let (want, got) = (flat(head - pelvis), flat(hd - pl));
                    if want.length() > 1.0 && got.length() > 1.0 {
                        let angle = got.angle_to(want);
                        let turn = Quat::from_rotation_y(-angle);
                        let origin = gt.translation();
                        let moved = origin + turn * (pl - origin);
                        let shift = Vec3::new(pelvis.x - moved.x, 0.0, pelvis.z - moved.z);
                        let to_local = parents.get(e).ok().and_then(|c| globals.get(c.parent()).ok()).map_or(bevy::math::Affine3A::IDENTITY, |g| g.affine().inverse());
                        let axis = to_local.transform_vector3(Vec3::Y).normalize_or(Vec3::Z);
                        t.rotation = (Quat::from_axis_angle(axis, -angle) * t.rotation).normalize();
                        t.translation += to_local.transform_vector3(shift);
                        info!("co-op: {name} laid over its body: turned {:.0}°, moved {:.0} units", angle.to_degrees(), shift.length());
                    }
                }
                p.rising = Some(Rising::Up { until: now + length });
            }
            Rising::Up { until } => {
                if now >= until {
                    commands.entity(e).remove::<Dying>();
                    p.rising = None;
                    info!("co-op: {name} is up: follows its player again");
                }
            }
        }
    }
}
