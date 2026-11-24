//! Door teleports left stale by the load order (docs/spec-notes/doors.md §6).
//!
//! A load door's XTEL names its destination door and stores where whoever goes through
//! arrives (the teleport marker's position and rotation) [documented: xEdit's XTEL
//! definition; the GECK writes both doors' XTELs when two doors are linked]. The marker is
//! stored on the door one leaves from, so a mod that moves the destination door has to
//! override the door one leaves from as well. A plugin that overrides that door without
//! having the mod that moved the destination door among its masters (or a mod that moves
//! the destination door and leaves the XTEL alone) wins with a marker by the place the
//! door stood before: the player arrives far from the door [observed: the owner's load
//! order, 2026-11-05: HOTM.esm moves the Strip's gate to Freeside (0x0013BB0D) about
//! 3,400 units up the Strip and keeps the vanilla marker of FreesideGateToStripREF
//! (0x00116425); Simple Open Freeside, loaded after it with FalloutNV.esm its only master,
//! overrides that door with the vanilla marker too, and the player arrives between the
//! Silver Palace and the House of Jack Pots]. The original engine uses the winning XTEL
//! as stored [hypothesis: nothing seen to the contrary; such load orders need a patch
//! plugin there].
//!
//! NVForever mends it [design: owner's overnight queue 2026-11-05, "you should arrive just
//! in front of the gate you came through"]: the plugin whose XTEL wins saw the destination
//! door as the latest version of it among that plugin and its masters. When the winning
//! version of the door differs from that one, the marker moves with the door: it keeps
//! its place and heading relative to the door as that plugin saw it, turned by the
//! door's change of z rotation (a z rotation turns clockwise seen from above,
//! docs/spec-notes/world.md §4 [observed]).
//!
//! A destination door that was tipped over (its x or y rotation changed) was not moved
//! for anyone to come out of: a mod knocked it out of the way to open the passage, and the
//! marker stays as stored [observed: the owner's load order, 2026-11-07: Simple Open
//! Freeside tilts Freeside's three gates by 0.5 to 0.65 rad on x and y, turns them and
//! sinks them about 290 units into the ground (VFSEastGateREF 0x00116423 to
//! (9402, -7264, 742)) and leaves the markers by the gates; moved with the door, the east
//! gate's marker went to the far side of the wall and under the street, and the player
//! fell and was put outside the gate (overnight queue item 37)]. [design: doors are placed
//! upright, so a tilt is the sign of a door taken out of the way.]

use crate::fields::{vec3, Fields};
use crate::index::global_of;
use fnv_esm::{LoadOrder, Record, Schema};
use std::collections::BTreeSet;

/// One plugin's XTEL: destination door (global FormID), marker position and rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xtel {
    pub door: u32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// A marker moved with its destination door.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mended {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// The load-order index of the plugin whose version of the destination door the
    /// XTEL's plugin saw.
    pub seen_in: usize,
    /// How far the destination door moved from there, game units.
    pub moved_by: f32,
}

/// A destination door that moved less than this, and turned less than [`STILL_TURN`],
/// counts as where it was [our choice: float noise between plugins].
pub const STILL_MOVE: f32 = 1.0;
/// Radians (about 0.06°).
pub const STILL_TURN: f32 = 0.001;

/// A destination door whose x or y rotation changed by more than this (radians, about
/// 6°) was tipped over to open the way, not moved: its marker is used as stored (module
/// notes) [our choice: well under the 0.47 rad the smallest observed tilt has].
pub const TIPPED: f32 = 0.1;

/// The change from angle `a` to `b`, radians, in (-π, π].
fn angle_change(a: f32, b: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (b - a).rem_euclid(tau);
    if d > std::f32::consts::PI { d - tau } else { d }
}

/// The XTEL of `r` (in plugin `pi`), if it has a complete one.
pub fn xtel(lo: &LoadOrder, schema: &Schema, pi: usize, r: &Record) -> Option<Xtel> {
    let f = Fields::new(r, pi, schema).ok()?;
    let x = f.get(b"XTEL")?;
    let door = global_of(lo, pi, x.get("door")?)?;
    let pr = x.get("pos_rot")?;
    Some(Xtel { door, pos: vec3(pr.get("position")?)?, rot: vec3(pr.get("rotation_radians")?)? })
}

/// The position and rotation of reference `r` (in plugin `pi`).
fn placement(schema: &Schema, pi: usize, r: &Record) -> Option<([f32; 3], [f32; 3])> {
    let f = Fields::new(r, pi, schema).ok()?;
    let d = f.get(b"DATA")?;
    Some((vec3(d.get("position")?)?, vec3(d.get("rotation_radians")?)?))
}

/// The latest version of reference `id` among plugin `pi` and its masters, and the plugin.
fn seen_by(lo: &LoadOrder, schema: &Schema, pi: usize, id: u32) -> Option<(usize, [f32; 3], [f32; 3])> {
    let view: BTreeSet<usize> = lo.master_map(pi).iter().map(|&m| m as usize).chain([pi]).collect();
    let d = lo.index.get(&id)?.iter().rev().find(|d| view.contains(&(d.plugin as usize)))?;
    let p = d.plugin as usize;
    let (pos, rot) = placement(schema, p, lo.records[p][d.ordinal as usize])?;
    Some((p, pos, rot))
}

/// The marker to use instead of `stored`, the XTEL of plugin `pi`, when the destination
/// door's winning version is not the one that plugin saw (module notes); `None`: use it
/// as stored.
pub fn mend(lo: &LoadOrder, schema: &Schema, pi: usize, stored: &Xtel) -> Option<Mended> {
    let (wp, wr) = {
        let (p, r) = crate::fields::winner(lo, stored.door)?;
        placement(schema, p, r)?
    };
    let (seen_in, sp, sr) = seen_by(lo, schema, pi, stored.door)?;
    let moved_by = ((wp[0] - sp[0]).powi(2) + (wp[1] - sp[1]).powi(2) + (wp[2] - sp[2]).powi(2)).sqrt();
    let turn = wr[2] - sr[2];
    if moved_by < STILL_MOVE && turn.abs() < STILL_TURN {
        return None;
    }
    if (0..2).any(|i| angle_change(sr[i], wr[i]).abs() > TIPPED) {
        return None;
    }
    // The marker's offset from the door as seen, turned clockwise by `turn`.
    let (dx, dy, dz) = (stored.pos[0] - sp[0], stored.pos[1] - sp[1], stored.pos[2] - sp[2]);
    let (s, c) = turn.sin_cos();
    let pos = [wp[0] + dx * c + dy * s, wp[1] - dx * s + dy * c, wp[2] + dz];
    let rot = [stored.rot[0], stored.rot[1], stored.rot[2] + turn];
    Some(Mended { pos, rot, seen_in, moved_by })
}

/// The XTEL of door reference `door_ref` (global FormID) as NVForever uses it: the
/// winning one, its marker moved with its destination door when that door moved after
/// the XTEL's plugin saw it (module notes).
pub fn door_xtel(lo: &LoadOrder, schema: &Schema, door_ref: u32) -> Option<Xtel> {
    let (pi, r) = crate::fields::winner(lo, door_ref)?;
    let mut x = xtel(lo, schema, pi, r)?;
    if let Some(m) = mend(lo, schema, pi, &x) {
        (x.pos, x.rot) = (m.pos, m.rot);
    }
    Some(x)
}
