//! Actors' jumps over the navmesh's ledge links (docs/spec-notes/actor-jumps.md): a path
//! that crosses a ledge link takes off at the top edge and lands on the triangle the link
//! names ([`crate::path::LedgeHop`]); between the two the body flies a ballistic arc
//! ([`Flight`]) under the gravity the player's character falls by.
//!
//! An actor moved by the physics character controller gets the same take-off as a jump
//! request (the controller carries the arc); a [`crate::Walker`] (no physics) flies the
//! arc computed here, landing exactly on the path's landing point.

use serde::Serialize;

/// Gravity, game units per second squared: the character controller's (686.7, 9.81 m/s²
/// at 70 units a metre) [observed: 683.8 and 685.5 measured on the player,
/// docs/spec-notes/physics.md §6.3].
pub const GRAVITY: f32 = 686.7;

/// Apex of a standing jump above the take-off, game units [observed: 61.6 on the player,
/// physics.md §6.3]. NPCs are taken to jump as high [hypothesis: not captured].
pub const JUMP_HEIGHT: f32 = 61.6;

/// The highest ledge-up link a path takes, game units of rise from the middle of the
/// link's edge to the landing triangle's centre: what a jump reaches
/// ([`JUMP_HEIGHT`]) [design: the navmesh's ledge-up links rise 304 units at the median
/// (navmesh.md §3), out of any jump's reach, so they read as the way back of a drop;
/// actor-jumps.md §3].
pub const LEDGE_RISE_MAX: f32 = JUMP_HEIGHT;

/// A jump up a ledge clears the landing by this much at its apex, game units [design].
pub const CLEAR: f32 = 8.0;

/// A ballistic arc from `from` to `to`: up at `vz` (units per second) at take-off,
/// across at a steady horizontal speed, landing on `to` after `duration` seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Flight {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub vz: f32,
    pub duration: f32,
    /// A climb (a ledge-up link) rather than a drop.
    pub up: bool,
}

impl Flight {
    /// The arc over a ledge from `from` to `to`. Up a ledge the actor jumps (`vz` for an
    /// apex of [`JUMP_HEIGHT`], or of the rise and [`CLEAR`] when that is higher); down
    /// one it steps off (`vz` 0) [hypothesis: the GECK wiki says NPCs "jump down" a drop;
    /// how they leave the edge is not documented, actor-jumps.md §3].
    pub fn over_ledge(from: [f32; 3], to: [f32; 3], up: bool) -> Flight {
        let rise = to[2] - from[2];
        let vz = if up { (2.0 * GRAVITY * JUMP_HEIGHT.max(rise + CLEAR)).sqrt() } else { 0.0 };
        Flight::with(from, to, vz, up)
    }

    /// The arc from `from` to `to` leaving at vertical speed `vz`: its time in the air
    /// is when it comes down to `to`'s height (at the apex, when it never rises that
    /// high).
    pub fn with(from: [f32; 3], to: [f32; 3], vz: f32, up: bool) -> Flight {
        let drop = from[2] - to[2];
        let disc = vz * vz + 2.0 * GRAVITY * drop;
        let duration = if disc >= 0.0 { (vz + disc.sqrt()) / GRAVITY } else { vz.max(0.0) / GRAVITY };
        Flight { from, to, vz, duration: duration.max(1e-3), up }
    }

    /// Horizontal speed over the arc, units per second.
    pub fn speed(&self) -> f32 {
        (self.to[0] - self.from[0]).hypot(self.to[1] - self.from[1]) / self.duration
    }

    /// Where the body is `t` seconds after take-off (`to` from `duration` on).
    pub fn at(&self, t: f32) -> [f32; 3] {
        if t >= self.duration {
            return self.to;
        }
        let t = t.max(0.0);
        let s = t / self.duration;
        [
            self.from[0] + (self.to[0] - self.from[0]) * s,
            self.from[1] + (self.to[1] - self.from[1]) * s,
            self.from[2] + self.vz * t - 0.5 * GRAVITY * t * t,
        ]
    }

    /// Highest point of the arc above the take-off, game units.
    pub fn apex(&self) -> f32 {
        let t = (self.vz / GRAVITY).clamp(0.0, self.duration);
        self.at(t)[2] - self.from[2]
    }
}

/// Whether an actor walking into something should jump it, from two probes straight ahead
/// along its way: `low` the distance to what a probe at knee height (above the step it
/// climbs) meets, `high` what a probe just above a jump's reach meets, `None` for nothing
/// within `reach`. It jumps when something blocks it low and nothing high [design:
/// the original's rule for jumping obstacles is not documented, actor-jumps.md §4].
pub fn obstacle_jumpable(low: Option<f32>, high: Option<f32>, reach: f32) -> bool {
    low.is_some_and(|d| d <= reach) && high.is_none_or(|d| d > reach)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_steps_off_and_lands_on_the_point() {
        // The median ledge-down link: 304 down, 168 across (navmesh.md §3).
        let f = Flight::over_ledge([0.0, 0.0, 304.0], [168.0, 0.0, 0.0], false);
        assert_eq!(f.vz, 0.0);
        // 304 = g t² / 2: t = 0.941 s.
        assert!((f.duration - (2.0f32 * 304.0 / GRAVITY).sqrt()).abs() < 1e-4, "{}", f.duration);
        assert!((f.speed() - 168.0 / f.duration).abs() < 1e-3);
        let end = f.at(f.duration - 1e-4);
        assert!((end[0] - 168.0).abs() < 0.1 && end[2].abs() < 0.2, "{end:?}");
        assert_eq!(f.at(5.0), [168.0, 0.0, 0.0]);
        assert!(f.apex().abs() < 1e-4);
        // Halfway through the time it has fallen a quarter of the way.
        let mid = f.at(f.duration / 2.0);
        assert!((mid[2] - 304.0 * 0.75).abs() < 0.1, "{mid:?}");
    }

    #[test]
    fn a_climb_jumps_as_high_as_the_player_or_higher() {
        // A low ledge: the standing jump's apex, 61.6, as the player's.
        let f = Flight::over_ledge([0.0, 0.0, 0.0], [60.0, 0.0, 30.0], true);
        assert!((f.apex() - JUMP_HEIGHT).abs() < 0.01, "{}", f.apex());
        // The player's standing jump: 290.6 units/s at take-off, 0.845 s in the air
        // (physics.md §6.3) — the same arc landing where it left.
        let flat = Flight::over_ledge([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], true);
        assert!((flat.vz - 290.6).abs() < 1.0 && (flat.duration - 0.845).abs() < 0.01, "{flat:?}");
        // Higher than a jump: it rises to clear it.
        let f = Flight::over_ledge([0.0, 0.0, 0.0], [60.0, 0.0, 80.0], true);
        assert!((f.apex() - 88.0).abs() < 0.01, "{}", f.apex());
        let end = f.at(f.duration);
        assert_eq!(end, [60.0, 0.0, 80.0]);
        // It lands on the way down: after the apex.
        assert!(f.duration > f.vz / GRAVITY);
    }

    #[test]
    fn obstacles_low_and_clear_above_are_jumped() {
        assert!(obstacle_jumpable(Some(20.0), None, 40.0));
        assert!(obstacle_jumpable(Some(20.0), Some(60.0), 40.0));
        // A wall: blocked high too.
        assert!(!obstacle_jumpable(Some(20.0), Some(25.0), 40.0));
        // Nothing near.
        assert!(!obstacle_jumpable(None, None, 40.0));
        assert!(!obstacle_jumpable(Some(80.0), None, 40.0));
    }
}
