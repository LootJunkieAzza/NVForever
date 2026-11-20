//! Deliberately authored river surface currents [design].
//!
//! WATR wind is not a river's downstream direction. These zones are selected once
//! when a water plane is created, using its own position (never the camera). They
//! are restricted to a worldspace editor ID and water elevation; no load-order
//! FormIDs, nearest-world fallback, or extrapolation across a whole worldspace.

use bevy::prelude::Vec2;
use fnv_world::cell_of;
#[cfg(test)]
use fnv_world::CELL_SIZE;

/// The current's speed in game units per second, about 1.6 m/s [design, by eye]. At 40
/// the river drifted slower than Lake Mead's wind moves its still water (NVCleanWater's
/// first layer: 65 units a second, world.md §6) and did not read as a current.
pub const FLOW_SPEED: f32 = 110.0;

/// Finite compass key: signed components are clamped to -1, 0 or 1, giving
/// exactly nine cache entries per water type, including still water [design].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FlowDirection { x: i8, y: i8 }

impl FlowDirection {
    pub const STILL: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i8, y: i8) -> Self {
        Self { x: x.signum(), y: y.signum() }
    }

    /// Surface advection in game x/y units per second [design]. Cardinal and
    /// diagonal zones have the same speed. This does not move actors or physics.
    pub fn velocity(self) -> Vec2 {
        Vec2::new(f32::from(self.x), f32::from(self.y)).normalize_or_zero() * FLOW_SPEED
    }
}

/// An inclusive rectangle of authored cells. Coverage is restricted to the
/// plane's elevation, so Lake Mead and Zion's higher pools cannot inherit it.
#[derive(Clone, Copy, Debug)]
struct FlowZone {
    worldspace: &'static str,
    surface_height: f32,
    x: (i32, i32),
    y: (i32, i32),
    direction: FlowDirection,
}

impl FlowZone {
    fn contains(self, worldspace: &str, cell: (i32, i32), surface_height: f32) -> bool {
        worldspace.eq_ignore_ascii_case(self.worldspace)
            && (surface_height - self.surface_height).abs() <= 1.0
            && (self.x.0..=self.x.1).contains(&cell.0)
            && (self.y.0..=self.y.1).contains(&cell.1)
    }
}

const SOUTH: FlowDirection = FlowDirection::new(0, -1);
const SOUTH_WEST: FlowDirection = FlowDirection::new(-1, -1);
const SOUTH_EAST: FlowDirection = FlowDirection::new(1, -1);

const fn colorado(x: (i32, i32), y: (i32, i32), direction: FlowDirection) -> FlowZone {
    FlowZone { worldspace: "WastelandNV", surface_height: -2300.0, x, y, direction }
}

const fn zion(x: (i32, i32), y: (i32, i32), direction: FlowDirection) -> FlowZone {
    FlowZone { worldspace: "NVDLC02ZionCanyon", surface_height: 5000.0, x, y, direction }
}

// Where the rivers are [observed, 2026-11-18: a private script of this change's author
// over their own FalloutNV.esm and HonestHearts.esm, read-only; its output is not in the
// repository]:
// - the surface heights are the worldspaces' default water heights (WRLD DNAM's second
//   float): WastelandNV -2300, NVDLC02ZionCanyon 5000 (docs/spec-notes/world.md records both);
// - the cells: for each exterior cell with the "has water" flag, the LAND heights (VHGT)
//   along the row a quarter of the way up the cell, and the stretches of it lying more
//   than 12 units under the cell's water height. On the Colorado those stretches run
//   between cells x 17 and 24 from row y 6 down to -31 (north of row 6 the water is Lake
//   Mead's, at 2600); in Zion the western stream runs from cell (1, 8) to (-3, -9).
// The rectangles below cover the cells those stretches cross, inclusive, and between two
// surveyed rows the continuous main channel only: no side branch is inferred.
// [design] The directions and the speed are choices of this project's: no record names
// a flow. They are coherent reaches, coarser than the shoreline's turns. Only drawn
// water moves.
const AUTHORED_ZONES: &[FlowZone] = &[
    // Colorado, below the dam: south, southwest around the northern bend, then
    // south through Cottonwood and southeast toward the southern boundary.
    colorado((17, 19), (2, 6), SOUTH),
    colorado((17, 20), (0, 1), SOUTH_WEST),
    colorado((17, 19), (-10, -1), SOUTH),
    colorado((17, 20), (-13, -11), SOUTH),
    colorado((17, 20), (-18, -14), SOUTH_EAST),
    colorado((18, 21), (-20, -19), SOUTH),
    colorado((16, 22), (-25, -21), SOUTH),
    colorado((19, 23), (-28, -26), SOUTH_EAST),
    colorado((21, 24), (-31, -29), SOUTH_EAST),
    // Zion's western/main stream. Higher side pools and unverified branches
    // intentionally have no zones. Negative coordinates use floor, not truncation.
    zion((1, 1), (7, 8), SOUTH),
    zion((1, 2), (6, 6), SOUTH_WEST),
    zion((1, 1), (4, 5), SOUTH),
    zion((1, 2), (3, 3), SOUTH_WEST),
    zion((1, 1), (2, 2), SOUTH_WEST),
    zion((0, 1), (1, 1), SOUTH_WEST),
    zion((-1, -1), (0, 0), SOUTH_WEST),
    zion((-2, -1), (-3, -1), SOUTH_WEST),
    zion((-3, -2), (-4, -4), SOUTH),
    zion((-2, -1), (-5, -5), SOUTH_WEST),
    zion((-2, -2), (-6, -6), SOUTH_WEST),
    zion((-3, -2), (-8, -7), SOUTH_WEST),
    zion((-3, -3), (-9, -9), SOUTH_WEST),
];

/// Classify the actual surface being created. Use the cell centre for cell water,
/// and the placed reference's actual game position for PWAT. Missing/unloaded
/// worldspaces, other water elevations and uncovered regions remain still.
pub fn surface_flow(worldspace: Option<&str>, game_xy: Vec2, surface_height: f32) -> FlowDirection {
    select_flow(AUTHORED_ZONES, worldspace, game_xy, surface_height)
}

/// First matching cell zone wins; finite coordinates are checked before casting.
fn select_flow(zones: &[FlowZone], worldspace: Option<&str>, xy: Vec2, surface_height: f32) -> FlowDirection {
    let Some(worldspace) = worldspace else { return FlowDirection::STILL };
    if !xy.is_finite() || !surface_height.is_finite() { return FlowDirection::STILL; }
    let cell = cell_of(xy.x, xy.y);
    zones.iter().find(|zone| zone.contains(worldspace, cell, surface_height))
        .map_or(FlowDirection::STILL, |zone| zone.direction)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_cell(world: &str, x: i32, y: i32, height: f32) -> FlowDirection {
        surface_flow(Some(world), Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL_SIZE, height)
    }

    #[test]
    fn colorado_corridor_and_cottonwood_probe_follow_authored_bends() {
        assert_eq!(at_cell("WastelandNV", 18, 5, -2300.0), SOUTH);
        assert_eq!(at_cell("WastelandNV", 19, 1, -2300.0), SOUTH_WEST);
        assert_eq!(at_cell("WastelandNV", 18, -16, -2300.0), SOUTH_EAST);
        assert_eq!(surface_flow(Some("WastelandNV"), Vec2::new(70250.0, -90750.0), -2300.0), SOUTH);
        assert_eq!(at_cell("WastelandNV", 16, -23, -2300.0), SOUTH);
        assert_eq!(at_cell("WastelandNV", 23, -31, -2300.0), SOUTH_EAST);
    }

    #[test]
    fn zion_narrow_surveyed_cells_remain_continuous_across_zero() {
        for (x, y) in [(1, 8), (1, 7), (1, 6), (1, 5), (1, 4), (1, 3), (1, 2), (0, 1), (-1, 0), (-1, -1), (-2, -2), (-2, -3), (-2, -4), (-1, -5), (-2, -6), (-2, -7), (-3, -8), (-3, -9)] {
            assert_ne!(at_cell("NVDLC02ZionCanyon", x, y, 5000.0), FlowDirection::STILL, "gap at ({x},{y})");
        }
        assert_eq!(surface_flow(Some("NVDLC02ZionCanyon"), Vec2::new(-0.5, 0.25) * CELL_SIZE, 5000.0), SOUTH_WEST);
    }

    #[test]
    fn lake_other_heights_worlds_and_near_misses_are_still() {
        for (world, x, y, height) in [
            ("WastelandNV", 18, 5, 2600.0), // Lake Mead elevation.
            ("WastelandNV", 18, 7, -2300.0), // Beyond surveyed northern reach.
            ("WastelandNV", 20, 5, -2300.0), // East of upper Colorado corridor.
            ("WastelandNV", 23, -23, -2300.0),
            ("WastelandNV", 23, -32, -2300.0),
            ("NVDLC02ZionCanyon", 1, 8, 6000.0), // Higher Zion pool.
            ("NVDLC02ZionCanyon", 2, 8, 5000.0), // Unverified side water.
            ("NVDLC02ZionCanyon", 0, 0, 5000.0),
            ("NVDLC02ZionCanyon", -3, -10, 5000.0),
            ("OtherWorld", 18, 5, -2300.0),
        ] {
            assert_eq!(at_cell(world, x, y, height), FlowDirection::STILL, "{world} ({x},{y}) height {height}");
        }
        assert_eq!(surface_flow(None, Vec2::new(70250.0, -90750.0), -2300.0), FlowDirection::STILL);
        assert_eq!(surface_flow(Some("WastelandNV"), Vec2::splat(f32::NAN), -2300.0), FlowDirection::STILL);
        assert_eq!(surface_flow(Some("WastelandNV"), Vec2::ZERO, f32::NAN), FlowDirection::STILL);
    }

    #[test]
    fn every_authored_row_has_valid_bounds_and_nonzero_flow() {
        for zone in AUTHORED_ZONES {
            assert!(zone.x.0 <= zone.x.1 && zone.y.0 <= zone.y.1);
            assert_ne!(zone.direction, FlowDirection::STILL);
            assert!(zone.surface_height.is_finite());
        }
    }

    #[test]
    fn compass_keys_are_canonical_and_have_equal_speed() {
        assert_eq!(FlowDirection::new(127, -128), FlowDirection::new(1, -1));
        assert_eq!(FlowDirection::new(0, 0), FlowDirection::STILL);
        assert_eq!(FlowDirection::STILL.velocity(), Vec2::ZERO);
        for x in -1..=1 {
            for y in -1..=1 {
                if x != 0 || y != 0 {
                    assert!((FlowDirection::new(x, y).velocity().length() - FLOW_SPEED).abs() < 0.001);
                }
            }
        }
    }
}
