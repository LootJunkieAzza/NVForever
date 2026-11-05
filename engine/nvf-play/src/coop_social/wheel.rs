//! The interaction wheel: three slices (Duel at the top, Trade lower right, Play Caravan
//! lower left), picked as the weapon wheel's are (`crate::weapon_wheel`): the pointer's
//! movement since it opened, past [`crate::weapon_wheel::DEAD_ZONE`]; 1 to 3 pick by key.
//! A slice with a reason it cannot be chosen shows the reason and does nothing.

use super::wire::Kind;
use bevy::math::Vec2;

/// A slice's middle, degrees clockwise from straight up.
pub fn angle_of(i: usize) -> f32 {
    i as f32 * 120.0
}

/// The slice in direction `p` from the centre (y down).
pub fn slice_at(p: Vec2) -> usize {
    let a = p.x.atan2(-p.y).to_degrees().rem_euclid(360.0);
    (((a + 60.0) / 120.0).floor() as usize) % 3
}

#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    /// The player the wheel is for.
    pub with: u8,
    pub name: String,
    pub picked: usize,
    pointer: Vec2,
    /// Why each slice cannot be chosen, if it cannot.
    pub refusals: [Option<String>; 3],
}

/// What a frame of input did.
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
        Wheel { with, name, picked: 0, pointer: Vec2::ZERO, refusals: [None, None, None] }
    }

    /// One frame: the pointer's movement, a number key (0-based), choose (click, E),
    /// cancel (Esc, right click).
    pub fn step(&mut self, moved: Vec2, key: Option<usize>, choose: bool, cancel: bool) -> Step {
        if cancel {
            return Step::Cancelled;
        }
        let before = self.picked;
        self.pointer += moved;
        if self.pointer.length() > crate::weapon_wheel::REACH {
            self.pointer = self.pointer.normalize() * crate::weapon_wheel::REACH;
        }
        if self.pointer.length() >= crate::weapon_wheel::DEAD_ZONE {
            self.picked = slice_at(self.pointer);
        }
        if let Some(k) = key.filter(|k| *k < 3) {
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
        for i in 0..3 {
            assert_eq!(slice_at(point(angle_of(i), 100.0)), i);
            assert_eq!(slice_at(point(angle_of(i) + 55.0, 100.0)), i);
            assert_eq!(slice_at(point(angle_of(i) - 55.0, 100.0)), i);
        }
        assert_eq!(Kind::ALL[0], Kind::Duel);
    }

    #[test]
    fn picking_and_refusals() {
        let mut w = Wheel::new(1, "B".into());
        assert_eq!(w.step(Vec2::new(5.0, 0.0), None, false, false), Step::Nothing, "inside the dead zone");
        assert_eq!(w.step(point(120.0, 150.0), None, false, false), Step::Moved);
        assert_eq!(w.picked, 1);
        assert_eq!(w.step(Vec2::ZERO, None, true, false), Step::Chose(Kind::Trade));
        w.refusals[0] = Some("Not enough room to duel here".into());
        assert_eq!(w.step(Vec2::ZERO, Some(0), false, false), Step::Refused("Not enough room to duel here".into()));
        assert_eq!(w.step(Vec2::ZERO, Some(2), false, false), Step::Chose(Kind::Caravan));
        assert_eq!(w.step(Vec2::ZERO, None, false, true), Step::Cancelled);
    }
}
