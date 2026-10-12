//! Navigation for NVForever: the game's navmeshes (NAVM/NAVI records) read from a load
//! order, assembled into one nav graph per load order (cells joined by their edge links,
//! spaces joined by doors), paths over it, and a path follower for actors. Headless; no
//! Bevy. The renderer and the tools consume this.
//!
//! - [`record`]: NAVM and NAVI records from their subrecord bytes
//! - [`load`]: every navmesh record of a load order, last definition winning
//! - [`graph`]: the nav graph: triangles joined across cells and doors, point lookup
//! - [`path`]: A* over the graph and the funnel algorithm
//! - [`doors`]: doors that open in place (no XTEL) standing on the navmesh
//! - [`follow`]: following a path; a walker that keeps actors on the navmesh surface
//!
//! Format facts and confidence tags: `docs/spec-notes/navmesh.md`; design:
//! `docs/design/navmesh.md`. Units and axes are the game's (game units, Z up).

pub mod doors;
pub mod follow;
pub mod graph;
pub mod load;
pub mod path;
pub mod record;

pub use follow::{Follower, Steer, WalkEvent, Walker};
pub use doors::InPlaceDoor;
pub use graph::{CoverEdge, NavGraph, Space};
pub use load::NavRecords;
pub use path::{DoorStop, Leg, Path, PathError, PathParams, Place};
pub use record::{DoorLink, EdgeLink, NavInfoMap, NavMeshRecord, Triangle};

#[cfg(test)]
mod tests;
