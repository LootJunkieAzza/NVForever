//! Carrying AI packages out (Layer 6): the step between the package an actor chose
//! (`fnv_game::ai`) and the actor being where that package wants it, over the navmesh
//! (`fnv_nav`). Headless; no Bevy. Design: docs/design/actor-behaviour.md §5; facts and
//! confidence tags: docs/spec-notes/packages.md.
//!
//! - [`goal`]: where a package sends its actor
//! - [`carry`]: the actors the AI moves: reachability, the walk, arrival
//! - [`sandbox`]: what a sandboxing actor does in its area
//! - [`social`]: actors who talk on their own: hellos to the player, random
//!   conversations

pub mod carry;
pub mod goal;
pub mod sandbox;
pub mod social;

pub use carry::{Activity, Agent, Ai, AiConfig, AiEvent, Logged, Mover, Talking};
pub use goal::{Goal, Route, Talk, Trigger};
pub use sandbox::{Allowed, Doing, Sandbox};
pub use social::Chat;
