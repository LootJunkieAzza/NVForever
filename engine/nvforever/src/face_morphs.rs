//! Face morph weights that come from something other than a `.lip` file: the combat face
//! (combat_face.rs, docs/spec-notes/combat-face.md), and later moods and scripted faces.
//! Each source sets the `.tri` labels it owns on the actor's root; lips.rs draws them,
//! added to the lip sync's own weights (lipsync.md §4.3), so there is one place that
//! moves a face.

use bevy::prelude::*;
use std::collections::BTreeMap;

/// A weight below this counts as none.
pub const SMALL: f32 = 1e-4;

/// On an actor's root: weight per `.tri` morph label (`CombatAnger` …), 0 to 1. A label
/// at 0 is removed, so an empty map means a face at rest.
#[derive(Component, Default, Clone, Debug, PartialEq)]
pub struct FaceMorphs(pub BTreeMap<&'static str, f32>);

impl FaceMorphs {
    pub fn get(&self, label: &str) -> f32 {
        self.0.get(label).copied().unwrap_or(0.0)
    }

    pub fn set(&mut self, label: &'static str, weight: f32) {
        if weight.abs() < SMALL {
            self.0.remove(label);
        } else {
            self.0.insert(label, weight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weight_of_nothing_is_removed() {
        let mut f = FaceMorphs::default();
        f.set("CombatAnger", 0.5);
        assert_eq!(f.get("CombatAnger"), 0.5);
        f.set("CombatAnger", 0.0);
        assert!(f.0.is_empty());
        assert_eq!(f.get("CombatAnger"), 0.0);
    }
}
