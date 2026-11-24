//! Co-op for NVForever: a host and up to five guests, each playing their own game and seeing
//! the others (docs/design/coop-networking.md). `net` is the link, `transport` its sockets,
//! `noise` its encryption, `online` and `signal_io` the matchmaking service and the checks
//! that find a path between two games, `code` the address codes for LAN play, and `reach`
//! how a host's router is asked to let friends in; the game side is
//! `engine/nvforever/src/coop.rs`.

pub mod code;
pub mod net;
pub mod noise;
pub mod online;
pub mod portmap;
pub mod reach;
pub mod rel;
pub mod repl;
pub mod signal_io;
pub mod story;
pub mod transport;
