//! Physics for Layer 4: a rapier3d world built from terrain (LAND heights, via
//! `fnv-world`) and placed references' collision objects (`fnv_collide` shapes), with
//! ray and shape queries, a kinematic capsule character controller, dynamic bodies for
//! clutter and deterministic fixed-step stepping. Headless; no Bevy.
//!
//! - [`convert`]: `fnv_collide::Shape` trees to rapier shapes
//! - [`terrain`]: one heightfield per exterior cell
//! - [`world`]: the world, bodies, stepping, queries
//! - [`character`]: the character controller
//! - [`air`]: horizontal motion in the air and after landings
//! - [`arrival`]: where an actor comes to rest after a teleport (shared by the tools and
//!   the game)
//! - [`layers`]: Havok collision layer numbers and names
//! - [`primitive`]: collision primitives placed on references (CollisionMarker boxes)
//! - [`placed`]: the collision objects of a placed reference (shared by the tools and the
//!   game)
//! - [`water`]: water areas (cell water surfaces) the character swims in
//! - [`swim`]: swimming speeds (the stroke cycle)
//!
//! Units and axes are the game's: lengths in game units, Z up, rotations row-major 3×3
//! rotating column vectors (as in `fnv_collide::shape` and `fnv_world::transform`).
//! Design: docs/design/layer4.md §2.8 and §7 (workstream 2); facts, hypotheses and the
//! CLI contract: docs/spec-notes/physics.md.

pub mod actors;
pub mod air;
pub mod arrival;
pub mod character;
pub mod convert;
pub mod layers;
pub mod placed;
pub mod primitive;
pub mod swim;
pub mod terrain;
pub mod water;
pub mod world;

pub use actors::HitVolume;
pub use arrival::Arrival;
pub use character::{Character, CharacterInput, CharacterParams, CharacterStep};
pub use convert::ConvertError;
pub use layers::LayerSet;
pub use placed::{ref_collision, ModelCollision, RefCollision};
pub use swim::Stroke;
pub use water::{PlacedWater, WaterArea};
pub use world::{
    AddReport, BodyState, Filter, Key, Owner, PhysParams, PhysWorld, PreparedRef, PreparedTerrain, Queries, RayHit, RefPose, Stats, Stepper,
    SweepHit, TERRAIN_SHELL,
};
