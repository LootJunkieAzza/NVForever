//! Gamebryo NIF / KF files as used by Fallout: New Vegas (Layer 2).
//!
//! - [`header`]: container header, block table, footer (byte-exact round trip);
//! - [`schema`]: block layouts as data (`schemas/nif/*.json`, format `fnv.nif.schema/1`);
//! - [`codec`]: the schema interpreter (decode and re-encode);
//! - [`file`]: a whole file, block splitting, per-block status.
//!
//! Format notes with confidence tags: `docs/spec-notes/nif.md`.

pub mod codec;
pub mod expr;
pub mod file;
pub mod header;
pub mod schema;
pub mod value;

pub use file::{BlockStatus, NifFile, RawBlock};
pub use header::{Header, NifError};
pub use schema::Schema;
pub use value::{Struct, Value};
