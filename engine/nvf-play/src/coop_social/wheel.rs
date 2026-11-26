//! The interaction wheel: two slices (Duel on the left, Trade on the right), picked as
//! the weapon wheel's are (`crate::weapon_wheel`): the pointer's movement since it opened
//! (as the weapon wheel moves its own pointer), past [`crate::weapon_wheel::DEAD_ZONE`]; 1
//! and 2 pick by key.
//! A slice with a reason it cannot be chosen shows the reason and does nothing.

use super::wire::Kind;
use bevy::math::Vec2;

/// The number of slices.
pub const SLICES: usize = Kind::ALL.len();
/// Each slice's span, degrees.
pub const SPAN: f32 = 360.0 / SLICES as f32;
/// The first slice's middle, degrees clockwise from straight up: the left [design].
const FIRST: f32 = 270.0;

/// A slice's middle, degrees clockwise from straight up.
pub fn angle_of(i: usize) -> f32 {
    (FIRST + i as f32 * SPAN).rem_euclid(360.0)
}

/// The slice in direction `p` from the centre (y down).
pub fn slice_at(p: Vec2) -> usize {
    let a = p.x.atan2(-p.y).to_degrees().rem_euclid(360.0);
    (((a - FIRST + SPAN / 2.0).rem_euclid(360.0) / SPAN).floor() as usize) % SLICES
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    /// The player the wheel is for.
    pub with: u8,
    pub name: String,
    pub picked: usize,
    pointer: Vec2,
    /// Why each slice cannot be chosen, if it cannot.
    pub refusals: [Option<String>; SLICES],
}

/// What a frame of input did.
/// A controller's right stick picks once pushed this far (the weapon wheel's).
const STICK_DEAD: f32 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Nothing,
    /// The pick moved.
    Moved,
    Chose(Kind),
    /// A slice that cannot be chosen was: why.
    Refused(String),
    Cancelled,
}

impl Wheel {
    pub fn new(with: u8, name: String) -> Wheel {
        Wheel { with, name, picked: 0, pointer: Vec2::ZERO, refusals: Default::default() }
    }

    /// One frame: the pointer's movement, a stick (y down; `None`: none), a number key
    /// (0-based), choose (click, Activate), cancel (Esc, right click, Menu Mode).
    pub fn step(&mut self, moved: Vec2, stick: Option<Vec2>, key: Option<usize>, choose: bool, cancel: bool) -> Step {
        if cancel {
            return Step::Cancelled;
        }
        let before = self.picked;
        self.pointer = match stick {
            Some(s) if s.length() >= STICK_DEAD => s.normalize() * crate::weapon_wheel::REACH,
            _ => (self.pointer + moved).clamp_length_max(crate::weapon_wheel::REACH),
        };
        if self.pointer.length() >= crate::weapon_wheel::DEAD_ZONE {
            self.picked = slice_at(self.pointer);
        }
        if let Some(k) = key.filter(|k| *k < SLICES) {
            self.picked = k;
            return self.choose();
        }
        if choose {
            return self.choose();
        }
        if self.picked != before {
            Step::Moved
        } else {
            Step::Nothing
        }
    }

    fn choose(&self) -> Step {
        match &self.refusals[self.picked] {
            Some(why) => Step::Refused(why.clone()),
            None => Step::Chose(Kind::ALL[self.picked]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon_wheel::point;

    #[test]
    fn slices_by_direction() {
        for i in 0..SLICES {
            assert_eq!(slice_at(point(angle_of(i), 100.0)), i);
            assert_eq!(slice_at(point(angle_of(i) + SPAN / 2.0 - 5.0, 100.0)), i);
            assert_eq!(slice_at(point(angle_of(i) - SPAN / 2.0 + 5.0, 100.0)), i);
        }
        assert_eq!(Kind::ALL[0], Kind::Duel);
        assert_eq!(slice_at(Vec2::new(-50.0, 0.0)), 0, "the left is Duel");
        assert_eq!(slice_at(Vec2::new(50.0, 0.0)), 1, "the right is Trade");
    }

    #[test]
    fn picking_and_refusals() {
        let mut w = Wheel::new(1, "B".into());
        assert_eq!(w.step(Vec2::new(5.0, 0.0), None, None, false, false), Step::Nothing, "inside the dead zone");
        assert_eq!(w.step(point(angle_of(1), 150.0), None, None, false, false), Step::Moved);
        assert_eq!(w.picked, 1);
        assert_eq!(w.step(Vec2::ZERO, None, None, true, false), Step::Chose(Kind::Trade));
        w.refusals[0] = Some("Not enough room to duel here".into());
        assert_eq!(w.step(Vec2::ZERO, None, Some(0), false, false), Step::Refused("Not enough room to duel here".into()));
        assert_eq!(w.step(Vec2::ZERO, None, Some(1), false, false), Step::Chose(Kind::Trade));
        assert_eq!(w.step(Vec2::ZERO, None, Some(2), false, false), Step::Nothing, "no third slice");
        assert_eq!(w.step(Vec2::ZERO, None, None, false, true), Step::Cancelled);
    }
}
