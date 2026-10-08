//! Water volumes: where the character swims (physics.md §6.6).
//!
//! A water volume is everything below a flat surface: per loaded exterior cell, the grid
//! square of the cell (`Cell::water_surface` of `fnv-world`: CELL XCLW, else the
//! worldspace's default height, for cells flagged "has water"), or for an interior the
//! whole interior. Volumes have no collider: the character asks for the surface above its
//! feet. The original's water objects (Havok layer 11, placeable water) are not modelled
//! [hypothesis: the captures at Boulder Beach were all in cell water].

use crate::world::PhysWorld;
use fnv_world::{cell_of, CELL_SIZE};

/// Where a water surface applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaterArea {
    /// The exterior grid square (x, y), `CELL_SIZE` units wide.
    Exterior(i32, i32),
    /// The whole of the interior being simulated.
    Interior,
}

impl WaterArea {
    /// Whether the point (x, y) lies in this area.
    pub fn contains(self, x: f32, y: f32) -> bool {
        match self {
            WaterArea::Exterior(gx, gy) => cell_of(x, y) == (gx, gy),
            WaterArea::Interior => true,
        }
    }

    /// South-west corner and size of an exterior square, for callers that draw it.
    pub fn bounds(self) -> Option<([f32; 2], f32)> {
        match self {
            WaterArea::Exterior(gx, gy) => Some(([gx as f32 * CELL_SIZE, gy as f32 * CELL_SIZE], CELL_SIZE)),
            WaterArea::Interior => None,
        }
    }
}

impl PhysWorld {
    /// Set (or with `None` remove) the water surface height of an area.
    pub fn set_water(&mut self, area: WaterArea, surface: Option<f32>) {
        match surface.filter(|s| s.is_finite()) {
            Some(s) => {
                self.water.insert(area, s);
            }
            None => {
                self.water.remove(&area);
            }
        }
    }

    /// The water surface height above or below the point (x, y), if it lies in water: an
    /// interior's water, else the exterior square containing it.
    pub fn water_surface_at(&self, x: f32, y: f32) -> Option<f32> {
        if let Some(&s) = self.water.get(&WaterArea::Interior) {
            return Some(s);
        }
        let (gx, gy) = cell_of(x, y);
        self.water.get(&WaterArea::Exterior(gx, gy)).copied()
    }

    /// The water areas with their surface heights, in order.
    pub fn water(&self) -> impl Iterator<Item = (WaterArea, f32)> + '_ {
        self.water.iter().map(|(&a, &s)| (a, s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PhysParams;

    #[test]
    fn surfaces_follow_their_squares() {
        let mut w = PhysWorld::new(PhysParams::default());
        assert_eq!(w.water_surface_at(10.0, 10.0), None);
        w.set_water(WaterArea::Exterior(13, 9), Some(2600.0));
        assert_eq!(w.water_surface_at(13.0 * 4096.0 + 1.0, 9.0 * 4096.0 + 1.0), Some(2600.0));
        assert_eq!(w.water_surface_at(14.0 * 4096.0 + 1.0, 9.0 * 4096.0 + 1.0), None);
        assert_eq!(w.water_surface_at(13.0 * 4096.0 - 1.0, 9.0 * 4096.0 + 1.0), None);
        w.set_water(WaterArea::Exterior(13, 9), None);
        assert_eq!(w.water_surface_at(13.0 * 4096.0 + 1.0, 9.0 * 4096.0 + 1.0), None);
        w.set_water(WaterArea::Interior, Some(204.0));
        assert_eq!(w.water_surface_at(-1.0e5, 3.0), Some(204.0));
        assert_eq!(w.water().count(), 1);
    }
}
