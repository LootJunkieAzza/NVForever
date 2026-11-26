//! Co-op for NVForever: a host and up to five guests, each playing their own game and
//! seeing the others' players. This crate is the protocol layer only: joining and hosting
//! a lobby, and every player's state reaching the others. Nothing of the world is shared
//! here yet.
//!
//! `net` is the session (the messages and who talks to whom), `rel` the reliable ordered
//! stream inside it, `repl` the kinds of item that stream carries, `guard` the host's
//! checks on everything a guest sends, and `seam` what carries the packets (any
//! transport that implements [`seam::Transport`]) and who may join. No real transport is in
//! this part: sockets, discovery and lobby codes come with the internet transport on top.

pub mod guard;
pub mod net;
pub mod rel;
pub mod repl;
pub mod seam;
