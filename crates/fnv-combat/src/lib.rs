//! Combat (Layer 6, docs/design/combat.md). Headless: no engine, no physics world.
//!
//! - [`data`] (step C1): the combat content of the masters: weapons, ammo and its
//!   effects, projectiles, explosions, body parts, the effect forms weapons apply, and
//!   the chain a shot follows (weapon → ammo → projectile → explosion). Field meanings
//!   and sources: docs/spec-notes/combat-data.md.
//! - Step C2, the rules: [`tuning`] (game settings with documented defaults, and our own
//!   [`tuning::CombatTuning`]), [`damage`] (damage, criticals, armour, knockdown,
//!   knockback), [`spread`] (spread cone, bloom, recoil), [`attack`] (fire rate, magazine,
//!   reload), [`projectile`] (flight, melee swings, blasts against a [`projectile::CombatWorld`]
//!   the caller provides), [`shot`] (a trigger pull to projectiles). The rules are
//!   described in docs/spec-notes/combat-rules.md.

pub mod attack;
pub mod damage;
pub mod data;
pub mod projectile;
pub mod shot;
pub mod spread;
pub mod tuning;
pub mod vec;

pub use data::{CombatForms, ShotChain};
pub use tuning::{CombatTuning, Rules};
