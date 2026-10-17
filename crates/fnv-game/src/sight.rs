//! Who measures whom, and what a detector can see (docs/design/detection.md §3, §7): the
//! geometry the engine's detection pass (`nvforever` `detection.rs`) feeds the formula of
//! [`crate::detection`], kept here so it can be tested without a world.
//!
//! - **Field of view**: ±95° about where the observer looks, horizontally, the same for
//!   every actor and for the player, whose observer is the camera [documented: GECK wiki
//!   GetLineOfSight notes, "The field of view for LOS is +/- 95 degrees … (non-player
//!   actor, player, or camera)"; "When called on the player … it returns true if the
//!   *camera* can see the object"].
//! - **Pairs**: every two actors of the loaded area within detection reach of each other
//!   are measured, whether the player is near or not ([`pairs_within`]) [hypothesis: the
//!   original's High process ("processed every frame", GECK wiki (JIP)
//!   GetActorProcessingLevel) runs detection for all of its actors; capture
//!   requests/detection-1.toml batch det2 checks it]. NVForever used to measure only
//!   actors within reach of the player, so fights between NPCs out of that reach never
//!   started (docs/design/script-function-coverage.md §5 item 1).
//! - **Sight over the land** ([`over_land`]): where the collision world is not loaded (it
//!   is only around the player), a line of sight is blocked by the landscape alone:
//!   buildings, rocks and other objects out there do not hide anyone [engineering choice:
//!   the cheapest blocker that still keeps hills between two camps].

/// Half the field of view, degrees [documented: GECK wiki GetLineOfSight].
pub const HALF_FIELD_OF_VIEW: f32 = 95.0;

/// Spacing of the landscape samples along a line of sight, game units: about two of the
/// heightmap's 128-unit squares per three samples [engineering choice].
pub const LAND_STEP: f32 = 96.0;

/// Whether `to` is within the field of view of an observer at `from` looking along
/// `forward` (a horizontal unit vector, game space).
pub fn in_view(from: [f32; 3], forward: [f32; 2], to: [f32; 3]) -> bool {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1.0 {
        return true;
    }
    let cos = (d[0] * forward[0] + d[1] * forward[1]) / len;
    cos >= HALF_FIELD_OF_VIEW.to_radians().cos()
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Every two of `feet` (indices, the lower first) no farther apart than `reach`: the pairs
/// to measure, each in both directions. Nobody is special: the player is one of the
/// points, and pairs far from it are measured too.
pub fn pairs_within(feet: &[[f32; 3]], reach: f32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for i in 0..feet.len() {
        for j in i + 1..feet.len() {
            if distance(feet[i], feet[j]) <= reach {
                out.push((i, j));
            }
        }
    }
    out
}

/// Whether the landscape leaves the line from `from` to `to` clear: sampled every
/// [`LAND_STEP`] units strictly between the ends, it is blocked where the land stands
/// above the line. `height` gives the land's height at (x, y), `None` where there is no
/// land (or it is not known): nothing blocks there.
pub fn over_land(from: [f32; 3], to: [f32; 3], mut height: impl FnMut(f32, f32) -> Option<f32>) -> bool {
    let flat = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
    let n = (flat / LAND_STEP).floor() as usize;
    for k in 1..=n {
        let t = k as f32 * LAND_STEP / flat;
        if t >= 1.0 {
            break;
        }
        let p = [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t, from[2] + (to[2] - from[2]) * t];
        if height(p[0], p[1]).is_some_and(|h| h > p[2]) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_of_view_is_95_degrees_either_side() {
        let at = [0.0, 0.0, 0.0];
        let north = [0.0, 1.0];
        assert!(in_view(at, north, [0.0, 100.0, 0.0]));
        assert!(in_view(at, north, [100.0, 0.0, 0.0]));
        // 94° and 96° off straight ahead.
        let off = |deg: f32| [100.0 * deg.to_radians().sin(), 100.0 * deg.to_radians().cos(), 0.0];
        assert!(in_view(at, north, off(94.0)));
        assert!(!in_view(at, north, off(96.0)));
        assert!(!in_view(at, north, [0.0, -100.0, 0.0]));
    }

    #[test]
    fn pairs_far_from_the_player_are_measured() {
        // The player at the origin; two actors 8,000 units away and 300 apart; a third
        // 6,000 units from them all.
        let feet = [[0.0, 0.0, 0.0], [8000.0, 0.0, 0.0], [8000.0, 300.0, 0.0], [8000.0, -6000.0, 0.0]];
        assert_eq!(pairs_within(&feet, 5000.0), vec![(1, 2)]);
        // Within reach of everyone: every pair, once.
        let near = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]];
        assert_eq!(pairs_within(&near, 5000.0), vec![(0, 1), (0, 2), (1, 2)]);
        // The reach itself counts.
        assert_eq!(pairs_within(&[[0.0; 3], [2500.0, 0.0, 0.0]], 2500.0), vec![(0, 1)]);
    }

    #[test]
    fn a_hill_between_blocks_sight_and_flat_land_does_not() {
        let from = [0.0, 0.0, 120.0];
        let to = [2000.0, 0.0, 90.0];
        assert!(over_land(from, to, |_, _| Some(0.0)));
        // A ridge 300 high halfway.
        let ridge = |x: f32, _: f32| Some(if (900.0..1100.0).contains(&x) { 300.0 } else { 0.0 });
        assert!(!over_land(from, to, ridge));
        // The same ridge with the observer on a tower above it.
        assert!(over_land([0.0, 0.0, 1000.0], [2000.0, 0.0, 400.0], ridge));
        // Unknown land never blocks.
        assert!(over_land(from, to, |_, _| None));
        // The ends themselves are not sampled: an observer standing in a dip still sees.
        assert!(over_land([0.0, 0.0, 10.0], [50.0, 0.0, 10.0], |_, _| Some(500.0)));
    }
}
