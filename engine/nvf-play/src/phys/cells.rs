//! Which physics cells to keep loaded around the player (pure logic).
//!
//! Exterior: the 3×3 grid squares around the player's square, nearest first; a loaded
//! square is dropped only once it is more than one square beyond that (hysteresis), so
//! walking along a cell border does not load and unload the same row each frame.
//! Interior: the one cell. The radius is an engineering choice (physics only needs what
//! the player can touch), not the game's uGridsToLoad.

use fnv_world::cell_of;

/// Exterior squares loaded around the player's square in each direction.
pub const RADIUS: i32 = 1;
/// Extra squares a loaded cell may lag behind before it is dropped.
pub const HYSTERESIS: i32 = 1;

/// A loaded unit of the physics world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PhysCell {
    /// An exterior grid square of the worldspace being played (loaded even when the
    /// worldspace has no cell there, so it is not retried every frame).
    Exterior(i32, i32),
    /// An interior cell (FormID).
    Interior(u32),
}

/// Where the player is: the worldspace or an interior cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Space {
    #[default]
    Exterior,
    Interior(u32),
}

impl Space {
    /// The cell the player stands in at `feet`.
    pub fn cell_at(self, feet: [f32; 3]) -> PhysCell {
        match self {
            Space::Exterior => {
                let (x, y) = cell_of(feet[0], feet[1]);
                PhysCell::Exterior(x, y)
            }
            Space::Interior(id) => PhysCell::Interior(id),
        }
    }
}

fn ring(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The exterior squares to have loaded around `centre`, nearest first (the centre first;
/// ties in a fixed order).
pub fn wanted(centre: (i32, i32)) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> =
        (-RADIUS..=RADIUS).flat_map(|dy| (-RADIUS..=RADIUS).map(move |dx| (centre.0 + dx, centre.1 + dy))).collect();
    v.sort_by_key(|&c| (ring(c, centre), (c.0 - centre.0).abs() + (c.1 - centre.1).abs(), c.1, c.0));
    v
}

/// What to unload and load (in order) for the player at `feet` in `space`, given what is
/// loaded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub unload: Vec<PhysCell>,
    pub load: Vec<PhysCell>,
}

pub fn plan(space: Space, loaded: &[PhysCell], feet: [f32; 3]) -> Plan {
    match space {
        Space::Interior(id) => Plan {
            unload: loaded.iter().copied().filter(|&c| c != PhysCell::Interior(id)).collect(),
            load: if loaded.contains(&PhysCell::Interior(id)) { vec![] } else { vec![PhysCell::Interior(id)] },
        },
        Space::Exterior => {
            let centre = cell_of(feet[0], feet[1]);
            let unload = loaded
                .iter()
                .copied()
                .filter(|&c| match c {
                    PhysCell::Exterior(x, y) => ring((x, y), centre) > RADIUS + HYSTERESIS,
                    PhysCell::Interior(_) => true,
                })
                .collect();
            let load = wanted(centre)
                .into_iter()
                .map(|(x, y)| PhysCell::Exterior(x, y))
                .filter(|c| !loaded.contains(c))
                .collect();
            Plan { unload, load }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::CELL_SIZE;

    fn at(gx: i32, gy: i32) -> [f32; 3] {
        [(gx as f32 + 0.5) * CELL_SIZE, (gy as f32 + 0.5) * CELL_SIZE, 0.0]
    }

    #[test]
    fn three_by_three_centre_first() {
        let w = wanted((-18, 0));
        assert_eq!(w.len(), 9);
        assert_eq!(w[0], (-18, 0));
        // The four edge neighbours before the corners.
        assert!(w[1..5].iter().all(|c| (c.0 + 18).abs() + c.1.abs() == 1));
        assert!(w[5..].iter().all(|c| (c.0 + 18).abs() == 1 && c.1.abs() == 1));
    }

    #[test]
    fn hysteresis_keeps_the_trailing_row() {
        let mut loaded: Vec<PhysCell> = wanted((0, 0)).into_iter().map(|(x, y)| PhysCell::Exterior(x, y)).collect();
        // One square east: three new cells, nothing dropped.
        let p = plan(Space::Exterior, &loaded, at(1, 0));
        assert!(p.unload.is_empty());
        assert_eq!(p.load, vec![PhysCell::Exterior(2, 0), PhysCell::Exterior(2, -1), PhysCell::Exterior(2, 1)]);
        loaded.extend(p.load);
        // Back west: nothing to do either way.
        assert_eq!(plan(Space::Exterior, &loaded, at(0, 0)), Plan::default());
        // Two squares east of the start: the west column (x = -1) is three away and goes.
        let p = plan(Space::Exterior, &loaded, at(2, 0));
        let mut gone = p.unload.clone();
        gone.sort();
        assert_eq!(gone, vec![PhysCell::Exterior(-1, -1), PhysCell::Exterior(-1, 0), PhysCell::Exterior(-1, 1)]);
        assert_eq!(p.load.len(), 3);
    }

    #[test]
    fn negative_coordinates_use_floor() {
        // Just west of x = 0 is square -1.
        assert_eq!(Space::Exterior.cell_at([-0.5, 10.0, 0.0]), PhysCell::Exterior(-1, 0));
    }

    #[test]
    fn interiors_hold_one_cell() {
        let loaded = vec![PhysCell::Exterior(0, 0), PhysCell::Exterior(1, 0)];
        let p = plan(Space::Interior(7), &loaded, [0.0; 3]);
        assert_eq!(p.unload, loaded);
        assert_eq!(p.load, vec![PhysCell::Interior(7)]);
        assert_eq!(plan(Space::Interior(7), &[PhysCell::Interior(7)], [0.0; 3]), Plan::default());
        // Back outside: the interior goes, the 3×3 comes.
        let p = plan(Space::Exterior, &[PhysCell::Interior(7)], at(3, 3));
        assert_eq!(p.unload, vec![PhysCell::Interior(7)]);
        assert_eq!(p.load[0], PhysCell::Exterior(3, 3));
        assert_eq!(p.load.len(), 9);
    }
}
