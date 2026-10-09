//! Combat (Layer 6, docs/design/combat.md). Headless: no engine, no physics world.
//!
//! - [`data`] (step C1): the combat content of the masters: weapons, ammo and its
//!   effects, projectiles, explosions, body parts, the effect forms weapons apply, and
//!   the chain a shot follows (weapon → ammo → projectile → explosion). Field meanings
//!   and sources: docs/spec-notes/combat-data.md.
//!
//! The damage rules, attack cycle and projectile stepping (steps C2 onwards) build on
//! this module.

pub mod data;

pub use data::{CombatForms, ShotChain};
