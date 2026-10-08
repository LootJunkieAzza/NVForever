//! World data for Layer 3: worldspaces, cells, placed references and landscape,
//! resolved across a load order. No engine dependencies; the renderer consumes this.
//!
//! - [`fields`]: schema-decoded access to one record's subrecords
//! - [`index`]: which cells belong to which worldspace and which references to which cell
//! - [`placed`]: a placed reference (REFR/ACHR/ACRE/...) and the base object it places
//! - [`land`]: landscape heights, normals, vertex colours and texture layers
//! - [`lighting`]: interior cell lighting and light sources
//! - [`sky`]: climate, weather and water records; time-of-day colours, sun and fog
//! - [`transform`]: reference rotation convention
//!
//! Format facts and confidence tags: `docs/spec-notes/world.md`.

pub mod fields;
pub mod imagespace;
pub mod index;
pub mod land;
pub mod lighting;
pub mod placed;
pub mod sky;
pub mod transform;

pub use index::{Cell, RefEntry, WorldIndex, Worldspace};
pub use land::Land;
pub use placed::{BaseObject, Placement};

/// Exterior cell edge length in world units [documented: UESP; observed: every exterior
/// reference lies within 4096 units of its cell's grid square, see world.md].
pub const CELL_SIZE: f32 = 4096.0;

/// The exterior grid cell containing a world position.
pub fn cell_of(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL_SIZE).floor() as i32, (y / CELL_SIZE).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cells() {
        assert_eq!(cell_of(0.0, 0.0), (0, 0));
        assert_eq!(cell_of(4095.9, 4096.0), (0, 1));
        assert_eq!(cell_of(-0.1, -4096.0), (-1, -1));
        assert_eq!(cell_of(-4096.1, 10.0), (-2, 0));
    }
}
