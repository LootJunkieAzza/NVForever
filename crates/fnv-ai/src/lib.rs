//! Carrying AI packages out (Layer 6): the step between the package an actor chose
//! (`fnv_game::ai`) and the actor being where that package wants it, over the navmesh
//! (`fnv_nav`). Headless; no Bevy. Design: docs/design/actor-behaviour.md §5; facts and
//! confidence tags: docs/spec-notes/packages.md.
//!
//! - [`goal`]: where a package sends its actor
//! - [`carry`]: the actors the AI moves: reachability, the walk, arrival
//! - [`sandbox`]: what a sandboxing actor does in its area

pub mod carry;
pub mod goal;
pub mod sandbox;

pub use carry::{Activity, Agent, Ai, AiConfig, AiEvent, Logged, Mover};
pub use goal::{Goal, Route};
pub use sandbox::{Allowed, Doing, Sandbox};
