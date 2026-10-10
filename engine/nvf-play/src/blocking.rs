//! Actors in the player's way (docs/spec-notes/physics.md §6.8): a living, drawn actor
//! blocks the player where its body is now: on its feet, walking, on a stool or in bed.
//!
//! An actor has no collider for this [design]. Its hit volumes are for shots
//! (`crate::hitvol`; the character controller passes through them), and the bodies of its
//! skeleton model are the actor's own, not the cell's (`fnv_phys::placed`: added at the
//! placement they stayed where the actor was placed). Each frame `crate::hitvol` works
//! out a [`Block`] per actor from what is drawn, and the player's wanted velocity is
//! clipped against the blocks before the character controller moves it ([`clip`]): the
//! player stops at an actor and slides round it, and one caught inside (an actor walked
//! or sat down into the player) is let out and moved out. Nothing here can wedge the
//! character, and rays never meet a block.
//!
//! - **Where** [design]: with posed body parts (`crate::hitvol::ActorParts`), the line on
//!   the ground under the torso's bones, pelvis to neck: a point under a standing or
//!   sitting person, the length of the body under one lying down or a four-legged
//!   creature. Without them, the middle of the actor's render bounds.
//! - **How wide** [design]: [`RADIUS_SHARE`] of the bounds' height round that line (22
//!   units for a person 128 tall, the player's own radius), within [`MIN_RADIUS`] and
//!   [`MAX_RADIUS`]. The bounds' width is not used: it changes as the actor turns.
//! - **Who is left out**: the dead, and actors the character controller walks
//!   (`PhysState::npcs`), whose characters block the player already.
//!
//! Not done: actors do not block each other, and the player does not block an actor
//! that is not walked by the character controller.

use crate::hitvol::PartSegment;
use bevy::prelude::*;
use fnv_combat::data::Limb;

/// A block's radius as a share of the actor's height [design].
pub const RADIUS_SHARE: f32 = 0.17;
/// Radius limits, game units [design].
pub const MIN_RADIUS: f32 = 10.0;
pub const MAX_RADIUS: f32 = 60.0;
/// The speed at which a player inside a block is moved out of it, game units per second
/// [design: about a walk].
pub const PUSH_OUT: f32 = 80.0;

/// Where an actor is in the player's way: `radius` round the line `a`–`b` on the ground,
/// from `bottom` to `top`. Game units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub actor: u32,
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub radius: f32,
    pub bottom: f32,
    pub top: f32,
}

/// The actors in the player's way this frame (`crate::hitvol` fills it).
#[derive(Resource, Default)]
pub struct ActorBlocks {
    pub list: Vec<Block>,
    /// Fixed steps in which a block changed the player's velocity (for the exit report).
    pub steps_blocked: u64,
}

/// The ends of an actor's torso: the first and last joint of its first torso part.
pub fn torso(segments: &[PartSegment]) -> Option<([f32; 3], [f32; 3])> {
    let first = segments.iter().find(|s| s.limb == Limb::Torso)?;
    let last = segments.iter().rfind(|s| s.limb == Limb::Torso && s.part == first.part)?;
    Some((first.a, last.b))
}

/// The block of an actor with the game-space bounds `lo`–`hi` and, when it has posed
/// parts, the ends of its torso (see the module notes).
pub fn block_for(actor: u32, lo: [f32; 3], hi: [f32; 3], torso: Option<([f32; 3], [f32; 3])>) -> Block {
    let middle = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let (a, b) = torso.map_or((middle, middle), |(a, b)| ([a[0], a[1]], [b[0], b[1]]));
    Block { actor, a, b, radius: (RADIUS_SHARE * (hi[2] - lo[2])).clamp(MIN_RADIUS, MAX_RADIUS), bottom: lo[2], top: hi[2] }
}

impl Block {
    /// The point of the block's line nearest to `p`.
    fn nearest(&self, p: [f32; 2]) -> [f32; 2] {
        let d = [self.b[0] - self.a[0], self.b[1] - self.a[1]];
        let len2 = d[0] * d[0] + d[1] * d[1];
        let t = if len2 > 1e-6 { (((p[0] - self.a[0]) * d[0] + (p[1] - self.a[1]) * d[1]) / len2).clamp(0.0, 1.0) } else { 0.0 };
        [self.a[0] + d[0] * t, self.a[1] + d[1] * t]
    }

    /// How far a character of `radius` at `p` is from touching the block (negative
    /// inside it), heights aside.
    pub fn room(&self, p: [f32; 2], radius: f32) -> f32 {
        let c = self.nearest(p);
        (p[0] - c[0]).hypot(p[1] - c[1]) - (self.radius + radius)
    }
}

/// The horizontal velocity a character with its feet at `feet`, of `radius` and `height`,
/// may have for a step of `dt` seconds when it wants `velocity`: towards a block no
/// closer than touching it, sliding along it; inside one, out of it at [`PUSH_OUT`] at
/// least (or as fast as takes it out in this step).
pub fn clip(velocity: [f32; 2], feet: [f32; 3], radius: f32, height: f32, dt: f32, blocks: &[Block]) -> [f32; 2] {
    let mut v = velocity;
    // Twice: sliding along one block can lead into the next.
    for _ in 0..2 {
        for b in blocks {
            if feet[2] >= b.top || feet[2] + height <= b.bottom {
                continue;
            }
            let c = b.nearest([feet[0], feet[1]]);
            let d = [feet[0] - c[0], feet[1] - c[1]];
            let dist = d[0].hypot(d[1]);
            let room = dist - (b.radius + radius);
            let speed = v[0].hypot(v[1]);
            if room > speed * dt {
                continue;
            }
            // Away from the block; standing on its line, the way the character goes.
            let n = if dist > 1e-3 {
                [d[0] / dist, d[1] / dist]
            } else if speed > 1e-3 {
                [v[0] / speed, v[1] / speed]
            } else {
                [1.0, 0.0]
            };
            let towards = -(v[0] * n[0] + v[1] * n[1]);
            let most = if room < 0.0 { -(-room / dt).min(PUSH_OUT) } else { room / dt };
            if towards > most {
                v[0] += n[0] * (towards - most);
                v[1] += n[1] * (towards - most);
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn person(at: [f32; 2]) -> Block {
        block_for(7, [at[0] - 60.0, at[1] - 15.0, 0.0], [at[0] + 60.0, at[1] + 15.0, 128.0], Some(([at[0], at[1], 70.0], [at[0], at[1], 110.0])))
    }

    /// Walk a character of radius 22 from `from` with the wanted velocity for `steps`.
    fn walk(mut feet: [f32; 3], want: [f32; 2], steps: usize, blocks: &[Block]) -> [f32; 3] {
        for _ in 0..steps {
            let v = clip(want, feet, 22.0, 128.0, DT, blocks);
            feet[0] += v[0] * DT;
            feet[1] += v[1] * DT;
        }
        feet
    }

    #[test]
    fn a_person_is_as_wide_as_the_player_however_it_is_turned() {
        let b = person([0.0, 0.0]);
        assert!((b.radius - 21.76).abs() < 0.01);
        // Turned a quarter: the bounds are wide the other way, the block is the same.
        let turned = block_for(7, [-15.0, -60.0, 0.0], [15.0, 60.0, 128.0], Some(([0.0, 0.0, 70.0], [0.0, 0.0, 110.0])));
        assert_eq!((b.radius, b.a, b.b), (turned.radius, turned.a, turned.b));
        // Small and huge creatures keep to the limits.
        assert_eq!(block_for(1, [0.0; 3], [10.0, 10.0, 20.0], None).radius, MIN_RADIUS);
        assert_eq!(block_for(1, [0.0; 3], [10.0, 10.0, 900.0], None).radius, MAX_RADIUS);
    }

    #[test]
    fn the_block_is_under_the_torso_not_the_bounds() {
        // Sat on a stool 40 units from where the bounds' middle is.
        let b = block_for(7, [-60.0, -15.0, 0.0], [60.0, 15.0, 128.0], Some(([40.0, 2.0, 50.0], [42.0, 2.0, 90.0])));
        assert_eq!((b.a, b.b), ([40.0, 2.0], [42.0, 2.0]));
        // No posed parts: the bounds' middle.
        let b = block_for(7, [-60.0, -15.0, 0.0], [60.0, 15.0, 128.0], None);
        assert_eq!((b.a, b.b), ([0.0, 0.0], [0.0, 0.0]));
    }

    #[test]
    fn the_torso_is_the_first_torso_part() {
        let seg = |part, limb, a: [f32; 3], b: [f32; 3]| PartSegment { part, limb, a, b };
        let segs = [
            seg(0, Limb::Head, [0.0, 0.0, 112.0], [0.0, 0.0, 128.0]),
            seg(1, Limb::Torso, [1.0, 0.0, 70.0], [2.0, 0.0, 90.0]),
            seg(1, Limb::Torso, [2.0, 0.0, 90.0], [3.0, 0.0, 110.0]),
            seg(4, Limb::Torso, [9.0, 9.0, 9.0], [9.0, 9.0, 9.0]),
        ];
        assert_eq!(torso(&segs), Some(([1.0, 0.0, 70.0], [3.0, 0.0, 110.0])));
        assert_eq!(torso(&segs[..1]), None);
    }

    #[test]
    fn walking_at_a_person_stops_where_they_touch() {
        let blocks = [person([100.0, 0.0])];
        let reach = blocks[0].radius + 22.0;
        let end = walk([0.0, 0.0, 0.0], [300.0, 0.0], 120, &blocks);
        assert!((end[0] - (100.0 - reach)).abs() < 0.01 && end[1] == 0.0, "{end:?}");
        // Where the person is not, nothing stops the walk.
        let free = walk([0.0, 200.0, 0.0], [300.0, 0.0], 60, &blocks);
        assert!((free[0] - 300.0).abs() < 0.01);
    }

    #[test]
    fn walking_past_slides_round() {
        let blocks = [person([100.0, 0.0])];
        let reach = blocks[0].radius + 22.0;
        // A little to the side of the middle: round the person and on.
        let end = walk([0.0, 10.0, 0.0], [300.0, 0.0], 240, &blocks);
        assert!(end[0] > 150.0 && end[1] >= reach - 0.01, "{end:?}");
        // Never closer than touching on the way.
        let mut feet = [0.0, 10.0, 0.0];
        for _ in 0..240 {
            feet = walk(feet, [300.0, 0.0], 1, &blocks);
            assert!((feet[0] - 100.0).hypot(feet[1]) >= reach - 0.01, "{feet:?}");
        }
    }

    #[test]
    fn above_and_below_is_free() {
        let blocks = [person([100.0, 0.0])];
        assert_eq!(clip([300.0, 0.0], [60.0, 0.0, 128.0], 22.0, 128.0, DT, &blocks), [300.0, 0.0]);
        assert_eq!(clip([300.0, 0.0], [60.0, 0.0, -128.0], 22.0, 128.0, DT, &blocks), [300.0, 0.0]);
    }

    #[test]
    fn caught_inside_gets_out() {
        let blocks = [person([100.0, 0.0])];
        let reach = blocks[0].radius + 22.0;
        // Standing still 10 units from the person: moved out, away from it, and no further.
        let v = clip([0.0, 0.0], [90.0, 0.0, 0.0], 22.0, 128.0, DT, &blocks);
        assert_eq!(v, [-PUSH_OUT, 0.0]);
        let end = walk([90.0, 0.0, 0.0], [0.0, 0.0], 120, &blocks);
        assert!((end[0] - (100.0 - reach)).abs() < 0.01, "{end:?}");
        // Walking out is as fast as wanted; walking further in is not possible.
        assert_eq!(clip([-300.0, 0.0], [90.0, 0.0, 0.0], 22.0, 128.0, DT, &blocks), [-300.0, 0.0]);
        assert!(clip([300.0, 0.0], [90.0, 0.0, 0.0], 22.0, 128.0, DT, &blocks)[0] <= -PUSH_OUT + 0.01);
        // On the very spot: out the way the character goes.
        let v = clip([0.0, 50.0], [100.0, 0.0, 0.0], 22.0, 128.0, DT, &blocks);
        assert!(v[0].abs() < 1e-4 && v[1] >= PUSH_OUT - 0.01, "{v:?}");
    }

    #[test]
    fn a_lying_body_blocks_along_its_length() {
        // A torso lying along y from 0 to 60.
        let b = block_for(7, [-20.0, -10.0, 0.0], [20.0, 70.0, 128.0], Some(([0.0, 0.0, 30.0], [0.0, 60.0, 30.0])));
        let reach = b.radius + 22.0;
        for y in [0.0, 30.0, 60.0] {
            let end = walk([-200.0, y, 0.0], [300.0, 0.0], 120, &[b]);
            assert!((end[0] + reach).abs() < 0.01, "{y}: {end:?}");
        }
        // Past its end the way is free.
        let end = walk([-200.0, 60.0 + reach + 1.0, 0.0], [300.0, 0.0], 120, &[b]);
        assert!(end[0] > 300.0, "{end:?}");
    }
}
