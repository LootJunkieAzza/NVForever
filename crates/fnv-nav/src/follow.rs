//! Following a path: [`Follower`] says where to head from the actor's current feet
//! (whatever moves the actor: the physics character controller, or [`Walker`], which
//! keeps the actor on the navmesh surface without physics).
//!
//! Arrival radii, turn rates and the stuck rule are engine choices
//! (docs/design/navmesh.md §4), not observed game values.

use crate::graph::{DoorEdge, NavGraph, Space, NONE};
use crate::path::{Path, PathError, PathParams, Place};
use serde::Serialize;

/// What to do next.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Steer {
    /// Head for `target`; `dir` is the horizontal unit direction, `to_target` the
    /// horizontal distance to it, `remaining` the horizontal length left on the path
    /// (to its end, or to the door that ends this leg).
    Walk { target: [f32; 3], dir: [f32; 2], to_target: f32, remaining: f32 },
    /// At a door: move the actor to `arrival` (in the next leg's space), then call
    /// [`Follower::door_used`].
    Door(DoorEdge),
    /// At the stop in front of a door that opens in place (`at`): stand, open the door,
    /// and once it is fully open (its "Open" sequence has ended) call
    /// [`Follower::door_opened`]. Until then the follower holds here. A door already
    /// open is passed at once by calling `door_opened` straight away.
    OpenDoor { door: u32, at: [f32; 3] },
    /// At the end of the path.
    Arrived,
}

/// Where along a path an actor is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Follower {
    pub path: Path,
    pub leg: usize,
    /// The point of the leg being walked to.
    pub next: usize,
    /// A corner counts as reached within this horizontal distance.
    pub arrive_radius: f32,
    /// The end of the path counts as reached within this horizontal distance.
    pub goal_radius: f32,
    /// A door that ends a leg is used from within this horizontal distance of the leg's
    /// end (in front of the door, where the frame may keep a body from the exact point).
    pub door_radius: f32,
    /// The next stop of the leg's `door_stops` (doors that open in place).
    pub stop: usize,
    /// Held at that stop ([`Steer::OpenDoor`] given) until [`Follower::door_opened`]:
    /// a door pushing the actor about while it opens does not send it walking again.
    pub holding: bool,
}

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Follower {
    pub fn new(path: Path) -> Follower {
        Follower { path, leg: 0, next: 1, arrive_radius: 16.0, goal_radius: 1.0, door_radius: 48.0, stop: 0, holding: false }
    }

    /// The space of the leg being walked.
    pub fn space(&self) -> Option<Space> {
        self.path.legs.get(self.leg).map(|l| l.space)
    }

    /// Advance past reached corners and say where to head from `feet`.
    pub fn steer(&mut self, feet: [f32; 3]) -> Steer {
        let Some(leg) = self.path.legs.get(self.leg) else { return Steer::Arrived };
        let pts = &leg.points;
        let pending = leg.door_stops.get(self.stop).copied();
        // A door stop reached, or at or behind the corner being walked to, holds the
        // follower there.
        if let Some(s) = pending.filter(|s| self.holding || s.point < self.next) {
            self.holding = true;
            return Steer::OpenDoor { door: s.door, at: pts[s.point] };
        }
        while self.next < pts.len() {
            let q = pts[self.next];
            let d = hdist(feet, q);
            let last = self.next + 1 == pts.len();
            let stop = pending.filter(|s| s.point == self.next);
            let goal = last && leg.door.is_none() && self.leg + 1 == self.path.legs.len();
            let radius = if stop.is_some() {
                self.arrive_radius
            } else if goal {
                self.goal_radius
            } else if last && leg.door.is_some() {
                self.door_radius
            } else {
                self.arrive_radius
            };
            // Passed a corner (beyond it along the segment that ends there) while close.
            let passed = !last && d < 4.0 * self.arrive_radius && {
                let p = pts[self.next - 1];
                let (sx, sy) = (q[0] - p[0], q[1] - p[1]);
                (feet[0] - q[0]) * sx + (feet[1] - q[1]) * sy >= 0.0
            };
            if d <= radius || passed {
                if let Some(s) = stop {
                    self.holding = true;
                    return Steer::OpenDoor { door: s.door, at: q };
                }
                self.next += 1;
                continue;
            }
            break;
        }
        if self.next >= pts.len() {
            return match leg.door {
                Some(d) => Steer::Door(d),
                None => Steer::Arrived,
            };
        }
        let target = pts[self.next];
        let to_target = hdist(feet, target);
        let dir = if to_target > 1e-4 { [(target[0] - feet[0]) / to_target, (target[1] - feet[1]) / to_target] } else { [0.0, 0.0] };
        let remaining = to_target + pts[self.next..].windows(2).map(|w| hdist(w[0], w[1])).sum::<f32>();
        Steer::Walk { target, dir, to_target, remaining }
    }

    /// The door that ended the current leg was used: go on with the next leg.
    pub fn door_used(&mut self) {
        self.leg += 1;
        self.next = 1;
        self.stop = 0;
        self.holding = false;
    }

    /// The door of the current [`Steer::OpenDoor`] is open: walk on through it.
    pub fn door_opened(&mut self) {
        let Some(s) = self.path.legs.get(self.leg).and_then(|l| l.door_stops.get(self.stop)).copied() else { return };
        self.stop += 1;
        self.holding = false;
        self.next = self.next.max(s.point + 1);
    }

    /// The path's destination.
    pub fn destination(&self) -> Option<Place> {
        let l = self.path.legs.last()?;
        Some(Place { space: l.space, pos: *l.points.last()? })
    }
}

/// What [`Unstuck`] advises.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Advice {
    /// Head this way (the path's direction, or a detour round an obstacle).
    Head([f32; 2]),
    /// Detours did not help: plan the path again from here.
    Replan,
    /// Replanning did not help either.
    GiveUp,
}

/// Getting an actor moved by physics past obstacles the navmesh does not know about
/// (low rocks the navmesh covers but the character controller cannot climb, clutter,
/// other actors). When the actor has got less than a fifth of its speed closer to the end
/// of its path (the path's remaining length) over the last second, it detours: turned 70° one way for 0.6 s, then the other way, then 110° each
/// way; if still stuck, the path is planned again; after `max_replans`, it gives up.
/// An engine choice (docs/design/navmesh.md §4), not the original game's behaviour.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unstuck {
    pub speed: f32,
    pub max_replans: usize,
    /// The path's remaining length over the last second, one sample per update with its
    /// time step.
    #[serde(skip)]
    history: std::collections::VecDeque<(f32, f32)>,
    /// The detour being made: index into [`DETOURS`] and time left.
    detour: Option<(usize, f32)>,
    /// Detours tried since the actor got stuck.
    tried: usize,
    /// The remaining length where the actor got stuck: detours count as having helped
    /// only once the actor is half a second's walk closer than this.
    stuck_at: Option<f32>,
    pub detours: usize,
    pub replans: usize,
}

/// Detour turns (radians, positive = to the left / counter-clockwise seen from above).
const DETOURS: [f32; 4] = [1.22, -1.22, 1.92, -1.92];
const DETOUR_SECONDS: f32 = 0.6;

fn rotate(d: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
}

impl Unstuck {
    pub fn new(speed: f32) -> Unstuck {
        Unstuck { speed, max_replans: 5, history: Default::default(), detour: None, tried: 0, stuck_at: None, detours: 0, replans: 0 }
    }

    /// Forget the progress history (after a door, a teleport or a new path).
    pub fn reset(&mut self) {
        self.history.clear();
        self.detour = None;
        self.stuck_at = None;
    }

    /// Record the path's remaining length after a step of `dt` seconds and say which way
    /// to head next, given the path's direction `want`. Moving without getting closer
    /// along the path (sliding round an obstacle, a detour that does not help) is not
    /// progress.
    pub fn advise(&mut self, remaining: f32, want: [f32; 2], dt: f32) -> Advice {
        self.history.push_back((remaining, dt));
        let mut span: f32 = self.history.iter().map(|h| h.1).sum();
        while span - self.history.front().map_or(0.0, |h| h.1) >= 1.0 {
            span -= self.history.pop_front().expect("non-empty").1;
        }
        if let Some((i, left)) = self.detour {
            let left = left - dt;
            if left > 0.0 {
                self.detour = Some((i, left));
                return Advice::Head(rotate(want, DETOURS[i]));
            }
            self.detour = None;
            self.history.clear();
            self.history.push_back((remaining, dt));
            return Advice::Head(want);
        }
        let first = self.history.front().expect("just pushed").0;
        let progress = first - remaining;
        if span < 1.0 {
            return Advice::Head(want);
        }
        if progress >= 0.2 * self.speed {
            if self.stuck_at.is_none_or(|m| remaining < m - 0.5 * self.speed) {
                self.tried = 0;
                self.stuck_at = None;
            }
            return Advice::Head(want);
        }
        // Stuck.
        self.history.clear();
        self.stuck_at = Some(self.stuck_at.map_or(remaining, |m| m.min(remaining)));
        if self.tried < DETOURS.len() {
            let i = self.tried;
            self.tried += 1;
            self.detours += 1;
            self.detour = Some((i, DETOUR_SECONDS));
            return Advice::Head(rotate(want, DETOURS[i]));
        }
        self.tried = 0;
        if self.replans >= self.max_replans {
            return Advice::GiveUp;
        }
        self.replans += 1;
        Advice::Replan
    }
}

/// What a [`Walker`] did in an update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum WalkEvent {
    /// No path, or standing at its end.
    Idle,
    Moved,
    /// Used a door; the walker is now at its arrival, in another space or place.
    UsedDoor(DoorEdge),
    /// Reached the end of the path in this update.
    Arrived,
    /// Standing in front of a door that opens in place, waiting for it to open.
    WaitingForDoor(u32),
}

/// An actor moved along a path on the navmesh surface, without physics (the actor's
/// height is the navmesh's under it). For actors with no collision world around them;
/// the engine moves actors with collision through the character controller instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Walker {
    pub space: Space,
    pub feet: [f32; 3],
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    /// The triangle under the feet, or [`NONE`].
    pub tri: u32,
    pub speed: f32,
    /// Fastest turn, radians per second.
    pub turn_rate: f32,
    pub follower: Option<Follower>,
    /// Horizontal distance moved so far.
    pub travelled: f32,
}

/// Compass heading of a horizontal direction (0 north = +y, π/2 east = +x).
pub fn heading_of(dir: [f32; 2]) -> f32 {
    dir[0].atan2(dir[1])
}

/// Turn `from` towards `to` (radians) by at most `max`, the short way round.
pub fn turn_towards(from: f32, to: f32, max: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (to - from + PI).rem_euclid(TAU) - PI;
    let r = from + d.clamp(-max, max);
    (r + PI).rem_euclid(TAU) - PI
}

impl Walker {
    pub fn new(space: Space, feet: [f32; 3], heading: f32, speed: f32) -> Walker {
        Walker { space, feet, heading, tri: NONE, speed, turn_rate: 4.0 * std::f32::consts::PI, follower: None, travelled: 0.0 }
    }

    /// Plan a path from the walker's feet to `to` and start following it.
    pub fn go_to(&mut self, nav: &NavGraph, to: Place, params: &PathParams) -> Result<&Path, PathError> {
        let path = nav.find_path(Place { space: self.space, pos: self.feet }, to, params)?;
        self.follower = Some(Follower::new(path));
        Ok(&self.follower.as_ref().expect("just set").path)
    }

    /// Move for `dt` seconds, passing doors that open in place as if they were open.
    pub fn update(&mut self, nav: &NavGraph, dt: f32) -> WalkEvent {
        self.update_with(nav, dt, &mut |_| true)
    }

    /// Move for `dt` seconds. At a door that opens in place, `open(door)` is asked every
    /// update until it says the door is fully open: it should start the door opening (if
    /// it is not already) and answer whether its "Open" sequence has ended; the walker
    /// stands meanwhile.
    pub fn update_with(&mut self, nav: &NavGraph, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        let Some(mut f) = self.follower.take() else { return WalkEvent::Idle };
        let ev = self.advance(nav, &mut f, dt, open);
        // Keep the path unless it ended (an idle update with the path not finished keeps it).
        let keep = match ev {
            WalkEvent::Arrived => false,
            WalkEvent::Idle => f.steer(self.feet) != Steer::Arrived,
            WalkEvent::Moved | WalkEvent::UsedDoor(_) | WalkEvent::WaitingForDoor(_) => true,
        };
        if keep {
            self.follower = Some(f);
        }
        ev
    }

    fn advance(&mut self, nav: &NavGraph, f: &mut Follower, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        let mut budget = self.speed * dt;
        let mut moved = false;
        // A step may pass several close corners.
        for _ in 0..8 {
            match f.steer(self.feet) {
                Steer::Arrived => return if moved { WalkEvent::Arrived } else { WalkEvent::Idle },
                Steer::OpenDoor { door, .. } => {
                    if open(door) {
                        f.door_opened();
                        continue;
                    }
                    return WalkEvent::WaitingForDoor(door);
                }
                Steer::Door(d) => {
                    f.door_used();
                    self.space = f.space().unwrap_or(self.space);
                    self.feet = d.arrival;
                    self.heading = d.arrival_heading;
                    self.tri = d.to;
                    return WalkEvent::UsedDoor(d);
                }
                Steer::Walk { target, dir, to_target, .. } => {
                    if budget <= 1e-4 {
                        break;
                    }
                    self.heading = turn_towards(self.heading, heading_of(dir), self.turn_rate * dt);
                    let step = budget.min(to_target);
                    let start = self.feet;
                    let mut p = [start[0] + dir[0] * step, start[1] + dir[1] * step, start[2]];
                    // Height: the navmesh under the new point, else along the segment.
                    let near = self.tri_near(nav, p);
                    p[2] = match near {
                        Some(t) => {
                            self.tri = t;
                            nav.height_on(t, p[0], p[1])
                        }
                        None => start[2] + (target[2] - start[2]) * (step / to_target.max(1e-4)),
                    };
                    self.feet = p;
                    self.travelled += step;
                    budget -= step;
                    moved = true;
                }
            }
        }
        if moved {
            WalkEvent::Moved
        } else {
            WalkEvent::Idle
        }
    }

    /// The triangle under `p`: the current one or one of its neighbours (two rings),
    /// else a search of the space.
    fn tri_near(&self, nav: &NavGraph, p: [f32; 3]) -> Option<u32> {
        if self.tri != NONE {
            let mut ring = vec![self.tri];
            for _ in 0..2 {
                if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                    return Some(t);
                }
                let mut next = Vec::new();
                for &t in &ring {
                    next.extend(nav.tris[t as usize].next.iter().copied().filter(|&n| n != NONE));
                }
                ring = next;
            }
            if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                return Some(t);
            }
        }
        nav.locate(self.space, p, 128.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Leg;

    fn path(points: Vec<[f32; 3]>) -> Path {
        Path { legs: vec![Leg { space: Space::Interior(1), points, corridor: vec![], door: None, door_stops: vec![] }], length: 0.0, expanded: 0, start_snap: 0.0, goal_snap: 0.0 }
    }

    #[test]
    fn follower_walks_corners_in_order() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        match f.steer([0.0, 0.0, 0.0]) {
            Steer::Walk { target, dir, remaining, .. } => {
                assert_eq!(target, [100.0, 0.0, 0.0]);
                assert_eq!(dir, [1.0, 0.0]);
                assert!((remaining - 200.0).abs() < 1e-3);
            }
            s => panic!("{s:?}"),
        }
        // Within the arrival radius of the corner: head for the end.
        match f.steer([95.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
        assert_eq!(f.steer([100.0, 99.0, 0.0]), Steer::Arrived);
    }

    #[test]
    fn passing_a_corner_counts() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        // Overshot the corner by 30 units, 30 units from it.
        match f.steer([130.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn unstuck_detours_then_replans_then_gives_up() {
        let mut u = Unstuck::new(100.0);
        u.max_replans = 1;
        let dt = 0.1;
        // Moving well: no advice beyond the path's direction.
        for i in 0..20 {
            assert_eq!(u.advise(1000.0 - i as f32 * 10.0, [1.0, 0.0], dt), Advice::Head([1.0, 0.0]));
        }
        // Blocked 800 from the end: after a second, the four detours in turn, each 0.6 s.
        let mut turns = Vec::new();
        let mut advice = Vec::new();
        for _ in 0..200 {
            let a = u.advise(800.0, [1.0, 0.0], dt);
            if let Advice::Head(d) = a {
                if d != [1.0, 0.0] && turns.last() != Some(&d) {
                    turns.push(d);
                }
            } else {
                advice.push(a);
            }
            if advice.contains(&Advice::GiveUp) {
                break;
            }
        }
        assert_eq!(turns.len(), 8, "{turns:?}"); // four detours, twice (before and after the replan)
        assert!(turns[0][1] > 0.9 && turns[1][1] < -0.9, "left first, then right: {turns:?}");
        assert_eq!(advice, vec![Advice::Replan, Advice::GiveUp]);
        assert_eq!((u.detours, u.replans), (8, 1));
    }

    #[test]
    fn turning_takes_the_short_way() {
        use std::f32::consts::PI;
        let h = turn_towards(0.9 * PI, -0.9 * PI, 0.1);
        assert!((h - (0.9 * PI + 0.1)).abs() < 1e-5 || (h - (0.9 * PI + 0.1 - 2.0 * PI)).abs() < 1e-5, "{h}");
        assert!((turn_towards(0.0, 0.05, 1.0) - 0.05).abs() < 1e-6);
        assert!((heading_of([1.0, 0.0]) - PI / 2.0).abs() < 1e-6);
        assert!(heading_of([0.0, 1.0]).abs() < 1e-6);
    }
}
