//! Co-op prototype for a private fork of NVForever: a host and up to three guests, each
//! playing their own game and seeing the others as NPCs. `net` is the link, `code` the
//! lobby codes, `reach` how the host's router is asked to let friends in; the game side
//! is `engine/nvforever/src/coop.rs`.

pub mod code;
pub mod net;
pub mod reach;
