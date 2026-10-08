//! Where an actor comes to rest after a teleport (a door's XTEL destination): one rule for
//! `fnvtool sim run` and the game (`engine/nvf-play`), so the parity lab and the game
//! agree. Facts, tags and the captures behind them: docs/spec-notes/physics.md §6.2.
//!
//! The rule: find the support under the arrival point with a vertical ray from half a
//! capsule above it to four capsules below; the feet rest on the support's contact shell,
//! `feet z = hit z + shell` (the terrain's [`crate::TERRAIN_SHELL`], a model's
//! `CollisionObject::shell`), whether the point was below that height (the original
//! pushes the actor up) or above it (the actor falls). With no support in range the actor
//! stays at the point.
//! [observed: after eleven door teleports in and around Goodsprings the player ended
//! between 0.18 below and 0.35 above the shell height, on terrain and on statics alike,
//! whether the destination lay under the surface, on it or above it; which height in that
//! band depends on the arrival in a way not yet explained, so the shell itself is used].
//! A ray at the centre rather than the capsule's footprint, and the search range, are
//! engineering choices [hypothesis].

use crate::character::CharacterParams;
use crate::world::{Filter, PhysWorld, RayHit};

/// Where an actor put at a point comes to rest.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Arrival {
    /// The feet position after settling.
    pub feet: [f32; 3],
    /// The support found under the point (its `point` is on the bare surface, `shell` its
    /// contact shell), or `None` when the actor stays at the point.
    pub support: Option<RayHit>,
}

impl PhysWorld {
    /// Where a character with `params` put at `at` (feet) comes to rest. `filter` says what
    /// can support it: normally `Filter { exclude_layers: layers::character_ignores(), .. }`
    /// with the character's own key excluded.
    pub fn arrival(&mut self, at: [f32; 3], params: &CharacterParams, filter: &Filter) -> Arrival {
        let (up, down) = (params.height / 2.0, 4.0 * params.height);
        let support = self.queries().ray([at[0], at[1], at[2] + up], [0.0, 0.0, -1.0], up + down, filter);
        let feet = support.as_ref().map_or(at, |h| [at[0], at[1], h.point[2] + h.shell]);
        Arrival { feet, support }
    }
}
