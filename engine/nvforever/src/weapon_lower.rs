//! The first-person weapon lowered against a wall or a friendly actor close ahead (the
//! owner's request, 2026-10-26, after the way a later game of the series does it).
//!
//! The original keeps the weapon up whatever is in front of it, so a gun pokes through a
//! wall or a companion's head [observed: the owner's play]. The game's files have no
//! lowered first-person pose to play (the `_1stperson` clips hold the weapon up: their
//! idles, aims, attacks and reloads [observed: the folder's file list]), so the lowering
//! is a turn of the arms rig like the sway's (`view_sway.rs`) [design; every constant
//! below an engineering choice, tuned by eye]:
//!
//! - **What lowers it**: two short rays ahead of the view, one from the eye and one from
//!   about where the weapon is held. A surface within [`WALL_START`] eases the weapon
//!   down, all the way at [`WALL_FULL`]; a living actor that would not attack the player
//!   (`fnv_game::rest::hostile_to_player`) within [`FRIENDLY_REACH`] lowers it all the
//!   way. A hostile one never does: the gun stays on it.
//! - **What brings it up**: aiming, the trigger, an attack or a reload bring it up at
//!   once ([`RAISE_FAST_SECONDS`]) and keep it up for [`HOLD_UP_SECONDS`] after (a
//!   reload for its clip's length), so a
//!   shot is never fired from a lowered gun for long and automatic fire does not dip
//!   between rounds. Shots leave from the view as before (`nvf_play::combat`), so a
//!   lowered gun changes nothing in combat.
//! - **The pose**: the weapon pitches down, turns in across the body and cants, about the
//!   eye, so the muzzle drops out of the wall and nothing rises into the view; less for
//!   rifles, heavy weapons and launchers ([`weight`]).
//!
//! Two taps of Ready Item lower it by hand, and two more raise it (`nvf_play::control`);
//! aiming or the trigger raise it too.
//!
//! Fists are never lowered, nor is the weapon under the Pip-Boy, an idle or in third
//! person.

use bevy::math::{Quat, Vec3};
use bevy::prelude::*;
use fnv_phys::{layers, Filter, LayerSet, Owner};
use fnv_sim::FormId;
use nvf_play::phys::PhysState;
use nvf_play::player::Player;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;

use crate::view_sway::SwayPose;

/// A surface this far ahead of the eye (units) starts lowering the weapon, and this
/// close lowers it all the way.
pub const WALL_START: f32 = 40.0;
pub const WALL_FULL: f32 = 22.0;
/// A friendly actor this far ahead lowers the weapon all the way.
pub const FRIENDLY_REACH: f32 = 72.0;
/// The second ray starts here from the eye, in the view's frame (x right, y ahead, z up):
/// about where a gun is held.
const HELD_AT: Vec3 = Vec3::new(5.0, 0.0, -8.0);
/// Seconds to lower, to raise, and to raise for a shot (time constants).
const LOWER_SECONDS: f32 = 0.16;
const RAISE_SECONDS: f32 = 0.12;
pub const RAISE_FAST_SECONDS: f32 = 0.04;
/// Seconds the weapon stays up after the trigger, an attack or a reload.
pub const HOLD_UP_SECONDS: f32 = 0.6;
/// Seconds between two checks of whether the same actor is friendly.
const FRIENDLY_RECHECK: f32 = 0.5;
/// The lowered pose: pitched down, turned in (left), canted (top to the left), radians,
/// about the eye. Turned about a point ahead (the hands), the arms between it and the
/// eye rise by their distance from it times the turn's sine, so the sleeves swung up
/// across the view (the owner's report, 2026-10-26); about the eye nothing near the
/// camera rises, the hands go down and back a little and the muzzle down.
const PITCH: f32 = -9.0 * std::f32::consts::PI / 180.0;
const TURN_IN: f32 = 10.0 * std::f32::consts::PI / 180.0;
const CANT: f32 = -5.0 * std::f32::consts::PI / 180.0;

/// What is ahead of the view this frame (`sense`, read by `player_body.rs`).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Ahead {
    /// The nearest surface ahead within [`WALL_START`] (units), not an actor.
    pub wall: Option<f32>,
    /// A living actor ahead within [`FRIENDLY_REACH`] that would not attack the player.
    pub friendly: bool,
    /// The last actor checked: its id, whether friendly, seconds until it is checked again.
    checked: Option<(u32, bool, f32)>,
}

/// What the lowering reads each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct LowerInput {
    pub dt: f32,
    pub wall: Option<f32>,
    pub friendly: bool,
    /// The player lowered it by hand (Ready Item tapped twice).
    pub by_hand: bool,
    /// Seconds from now the weapon must stay up: [`HOLD_UP_SECONDS`] while aiming, with
    /// the trigger held or an attack started, a reload's length when one starts; 0 else.
    pub up_for: f32,
    /// Whether the weapon may be lowered at all: a weapon drawn, in first person, not
    /// under the Pip-Boy or an idle, the switch on.
    pub on: bool,
}

/// The lowering's state across frames.
#[derive(Debug, Clone, Default)]
pub struct WeaponLower {
    /// How far down the weapon is (0 up, 1 lowered).
    pub share: f32,
    /// Seconds the weapon stays up for.
    hold: f32,
    /// Whether it counts as lowered (past half way), for the log.
    pub down: bool,
    /// Times lowered since the start, and the most lowered (the exit summary).
    pub times: u32,
    pub peak: f32,
}

/// How far down a surface `d` units ahead puts the weapon.
pub fn wall_share(d: f32) -> f32 {
    ((WALL_START - d) / (WALL_START - WALL_FULL)).clamp(0.0, 1.0)
}

impl WeaponLower {
    pub fn update(&mut self, i: LowerInput) -> f32 {
        let dt = i.dt.max(0.0);
        let follow = |t: f32| if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / t).exp() };
        self.hold = (self.hold - dt).max(i.up_for).max(0.0);
        let target = if !i.on || self.hold > 0.0 {
            0.0
        } else if i.friendly || i.by_hand {
            1.0
        } else {
            i.wall.map_or(0.0, wall_share)
        };
        let t = if target > self.share {
            LOWER_SECONDS
        } else if self.hold > 0.0 {
            RAISE_FAST_SECONDS
        } else {
            RAISE_SECONDS
        };
        self.share += (target - self.share) * follow(t);
        if self.share < 1e-4 {
            self.share = 0.0;
        }
        self.peak = self.peak.max(self.share);
        let down = self.share > 0.5;
        if down && !self.down {
            self.times += 1;
        }
        self.down = down;
        self.share
    }
}

/// How much of the pose a weapon of animation prefix `prefix` takes: the heavy weapons
/// and launchers (`2hh`, `2hl`) are held at the foot of the view or on the shoulder and
/// would leave it or swing across it, the rifles (`2hr`, `2ha`) a little less [design, by
/// eye from the screenshots of 2026-10-27].
pub fn weight(prefix: &str) -> f32 {
    match prefix {
        "2hh" | "2hl" => 0.35,
        "2hr" | "2ha" => 0.8,
        _ => 1.0,
    }
}

/// The pose of the arms lowered by `share` (eased) of `weight` ([`weight`]), in the
/// view's frame about the eye.
pub fn pose(share: f32, weight: f32) -> SwayPose {
    if share <= 0.0 {
        return SwayPose::NONE;
    }
    let s = share * share * (3.0 - 2.0 * share) * weight;
    SwayPose { rotation: Quat::from_rotation_z(TURN_IN * s) * Quat::from_rotation_x(PITCH * s) * Quat::from_rotation_y(CANT * s), offset: Vec3::ZERO }
}

/// `b` after `a`: the pose `a` moved on by `b`, both about the eye.
pub fn then(a: SwayPose, b: SwayPose) -> SwayPose {
    if b == SwayPose::NONE {
        return a;
    }
    SwayPose { rotation: b.rotation * a.rotation, offset: b.rotation * a.offset + b.offset }
}

/// Look ahead of the view for a wall or a friendly actor (before the body is posed).
pub fn sense(time: Res<Time>, player: Res<Player>, mut phys: ResMut<PhysState>, script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut ahead: ResMut<Ahead>) {
    let dt = time.delta_secs();
    let eye = Vec3::from_array(player.eye());
    let look = Vec3::from_array(player.look_dir());
    let (s, c) = player.heading.sin_cos();
    let right = Vec3::new(c, -s, 0.0);
    let up = right.cross(look).normalize_or_zero();
    let held = eye + right * HELD_AT.x + look * HELD_AT.y + up * HELD_AT.z;
    let reach = FRIENDLY_REACH.max(WALL_START);
    // Actors by their hit volumes (the movement capsules are not in the ray queries);
    // corpses are not looked at.
    let filter = || Filter { exclude_layers: fnv_phys::layers::character_ignores().union(LayerSet::from_layers(&[layers::DEAD_BIP])), ..Default::default() };
    let mut wall: Option<f32> = None;
    let mut actor: Option<u32> = None;
    for from in [eye, held] {
        // Past the player's own hit volumes.
        let own = |o: Owner| matches!(o, Owner::Actor { form_id, .. } if form_id == FormId::PLAYER_REF.0);
        let Some(hit) = phys.ray_all(from.to_array(), look.to_array(), reach, filter()).into_iter().find(|h| !own(h.owner)) else { continue };
        match hit.owner {
            Owner::Actor { form_id: id, .. } | Owner::Character { id } if id != 0 => {
                if hit.distance <= FRIENDLY_REACH {
                    actor = actor.or(Some(id));
                }
            }
            _ => {
                if hit.distance <= WALL_START {
                    wall = Some(wall.map_or(hit.distance, |w| w.min(hit.distance)));
                }
            }
        }
    }
    ahead.wall = wall;
    let checked = ahead.checked.map(|(id, f, t)| (id, f, t - dt));
    ahead.friendly = match actor {
        None => false,
        Some(id) => match checked {
            Some((was, f, t)) if was == id && t > 0.0 => {
                ahead.checked = Some((id, f, t));
                f
            }
            _ => {
                let rt = script.and_then(|s| s.into_inner().rt.as_mut());
                // No scripts running: nobody is known to be hostile, nobody counts as a friend.
                let f = rt.is_some_and(|rt| with_world(rt, &mut sim, |rt| !rt.game.is_dead(FormId(id)) && !fnv_game::rest::hostile_to_player(&mut rt.game, FormId(id))));
                ahead.checked = Some((id, f, FRIENDLY_RECHECK));
                f
            }
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(wall: Option<f32>, friendly: bool, busy: bool) -> LowerInput {
        LowerInput { dt: 1.0 / 60.0, wall, friendly, by_hand: false, up_for: if busy { HOLD_UP_SECONDS } else { 0.0 }, on: true }
    }

    fn settle(l: &mut WeaponLower, i: LowerInput, frames: usize) -> f32 {
        let mut s = 0.0;
        for _ in 0..frames {
            s = l.update(i);
        }
        s
    }

    #[test]
    fn a_wall_close_ahead_lowers_it_and_stepping_back_raises_it() {
        let mut l = WeaponLower::default();
        assert_eq!(settle(&mut l, input(None, false, false), 30), 0.0);
        assert!(settle(&mut l, input(Some(WALL_FULL - 2.0), false, false), 90) > 0.99);
        assert_eq!(l.times, 1);
        assert!(settle(&mut l, input(None, false, false), 90) < 1e-3);
    }

    #[test]
    fn a_wall_half_way_lowers_it_half_way() {
        let mut l = WeaponLower::default();
        let s = settle(&mut l, input(Some((WALL_START + WALL_FULL) / 2.0), false, false), 120);
        assert!((s - 0.5).abs() < 0.01, "{s}");
    }

    #[test]
    fn a_friend_lowers_it_fully() {
        let mut l = WeaponLower::default();
        assert!(settle(&mut l, input(None, true, false), 90) > 0.99);
    }

    #[test]
    fn aiming_or_firing_brings_it_straight_up_and_keeps_it_up_a_while() {
        let mut l = WeaponLower::default();
        settle(&mut l, input(Some(10.0), false, false), 90);
        // A tenth of a second of the trigger: up already.
        let s = settle(&mut l, input(Some(10.0), false, true), 6);
        assert!(s < 0.1, "{s}");
        // Let go: still up for the hold, then down again.
        let s = settle(&mut l, input(Some(10.0), false, false), (HOLD_UP_SECONDS * 60.0 * 0.8) as usize);
        assert!(s < 0.1, "{s}");
        assert!(settle(&mut l, input(Some(10.0), false, false), 90) > 0.99);
    }

    #[test]
    fn a_reload_keeps_it_up_for_its_length() {
        let mut l = WeaponLower::default();
        settle(&mut l, input(Some(10.0), false, false), 90);
        l.update(LowerInput { up_for: 2.0, ..input(Some(10.0), false, false) });
        assert!(settle(&mut l, input(Some(10.0), false, false), 100) < 0.01, "up through the reload");
        assert!(settle(&mut l, input(Some(10.0), false, false), 60) > 0.9, "down after it");
    }

    #[test]
    fn by_hand_it_lowers_with_nothing_ahead() {
        let mut l = WeaponLower::default();
        assert!(settle(&mut l, LowerInput { by_hand: true, ..input(None, false, false) }, 90) > 0.99);
    }

    #[test]
    fn switched_off_it_stays_up() {
        let mut l = WeaponLower::default();
        assert_eq!(settle(&mut l, LowerInput { on: false, ..input(Some(1.0), true, false) }, 90), 0.0);
    }

    #[test]
    fn the_pose_points_the_weapon_down_and_in() {
        let p = pose(1.0, 1.0);
        let ahead = p.rotation * Vec3::Y;
        assert!(ahead.z < -0.1 && ahead.x < -0.1, "down and to the left: {ahead:?}");
        assert_eq!(pose(0.0, 1.0), SwayPose::NONE);
    }

    #[test]
    fn nothing_of_the_arms_rises_towards_the_view() {
        // The arms and sleeves the view can see: from near the eye out past the hands,
        // below it. Only the cant lifts the far side a little.
        let p = pose(1.0, 1.0);
        for y in (5..=40).step_by(5) {
            for z in (-30..=0).step_by(3) {
                for x in [-12.0, 0.0, 12.0] {
                    let v = Vec3::new(x, y as f32, z as f32);
                    let moved = p.rotation * v + p.offset;
                    assert!(moved.z <= v.z + 2.5, "{v:?} rose to {moved:?}");
                }
            }
        }
    }

    #[test]
    fn then_composes_the_poses() {
        let a = SwayPose { rotation: Quat::from_rotation_x(0.1), offset: Vec3::new(1.0, 0.0, 0.0) };
        let b = pose(1.0, 1.0);
        let c = then(a, b);
        let p = Vec3::new(3.0, 20.0, -5.0);
        let by_steps = b.rotation * (a.rotation * p + a.offset) + b.offset;
        assert!((c.rotation * p + c.offset - by_steps).length() < 1e-4);
    }
}
