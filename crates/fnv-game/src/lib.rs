//! Layer 5 game rules over the Layer 4 world (docs/design/layer5.md §3.1): script
//! instances and their variables, event dispatch, the quest model, condition (`CTDA`)
//! evaluation, the base-game function implementations, and the headless script runner
//! with its trace (`fnv.script.trace/1`). The interpreter itself is
//! `fnv_script::vm`; this crate is the `Host` it runs against.
//!
//! - [`data`]: compiled scripts, quests, dialogue/package results, messages, form lists
//! - [`quest`]: quest definitions and run-time state
//! - [`cond`]: the conditions of `CTDA` records
//! - [`package`]: AI packages as typed data (docs/spec-notes/packages.md)
//! - [`ai`]: which package an actor runs and when it chooses again
//! - [`game`]: the game state, instances, dispatch, SetStage, activation, snapshots
//! - [`functions`]: function implementations by opcode
//! - [`trace`]: trace events
//! - [`dialogue`]: conversations between the player and an actor (Layer 6, step D2)
//! - [`idles`]: the idle animations scripts, packages and dialogue lines ask for
//! - [`intro`]: the intro chain's units (coverage)
//! - [`runner`]: `fnv.script.actions/1` → `fnv.script.trace/1`
//! - [`voice`]: the hook that gives a dialogue response the length of its voice file
//!
//! Behaviour notes with confidence tags: docs/spec-notes/scripts.md §4 and later.

pub mod actor_values;
pub mod ai;
pub mod combat;
pub mod cond;
pub mod data;
pub mod dialogue;
pub mod functions;
pub mod game;
pub mod idles;
pub mod intro;
pub mod package;
pub mod quest;
pub mod runner;
pub mod templates;
pub mod trace;
pub mod voice;

pub use data::GameData;
pub use game::{Game, GameConfig, InstKey};
