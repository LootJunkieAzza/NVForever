//! Face morphs other systems ask for (docs/spec-notes/lipsync.md §5.4): a weight per
//! `.tri` morph label on an actor's root (the entity with `ActorRig` and
//! `lips::LipRig`), shown by `lips.rs` on top of the line's lip sync and emotion. The
//! combat face (`combat_face.rs`) sets `CombatAnger` here.

use bevy::prelude::*;
use std::collections::BTreeMap;

/// Weights 0 to 1 by `.tri` morph label. Labels `lips.rs` knows: the expressions
/// (`Anger` … `Pained`) and the moods (`MoodNeutral` … `CombatAnger`, `lips::MOODS`).
#[derive(Component, Default, Clone, Debug, PartialEq)]
pub struct FaceMorphs(pub BTreeMap<&'static str, f32>);

impl FaceMorphs {
    /// The weight of a label (0 when not set).
    pub fn get(&self, label: &str) -> f32 {
        self.0.get(label).copied().unwrap_or(0.0)
    }

    /// Set a label's weight; a weight of about 0 removes it.
    pub fn set(&mut self, label: &'static str, w: f32) {
        if w.abs() < 1e-4 {
            self.0.remove(label);
        } else {
            self.0.insert(label, w);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_remove() {
        let mut m = FaceMorphs::default();
        assert!(m.is_empty());
        m.set("CombatAnger", 0.5);
        assert_eq!(m.get("CombatAnger"), 0.5);
        m.set("CombatAnger", 0.0);
        assert!(m.is_empty() && m.get("CombatAnger") == 0.0);
    }
}
