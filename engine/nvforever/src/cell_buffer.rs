//! The cell buffer: which cells the player left recently enough to be "still buffered in
//! memory", for the AI's process levels (`fnv_ai::process`, docs/design/actor-behaviour.md
//! §5.9).
//!
//! - Actors in exterior cells "not currently loaded, but which have recently been
//!   visited by the player and are still buffered in memory" are at the original's
//!   Mid-High level (updated about every 18 s); actors in cells "neither currently loaded
//!   nor buffered" at Low (about every 120 s); entering a casino puts the Strip's exterior
//!   actors at Mid-High [documented: GECK wiki (JIP) GetActorsByProcessingLevel].
//! - The buffers' sizes are INI settings: `uExterior Cell Buffer=36`, `uInterior Cell
//!   Buffer=3` with `uGridsToLoad=5` [observed: the owner's install, Fallout_default.ini
//!   and Fallout.ini].
//! - What a buffer holds [hypothesis]: the cells drawn most recently, the drawn ones
//!   included (36 = the 5 × 5 loaded cells and the 11 left last; the interior buffer is
//!   the interior the player is in and the two before). An interior does not touch the
//!   exterior buffer (the Strip example) nor the other way round.
//! - Actors in "interior cells that have not been visited, but are accessible through
//!   teleport doors within player's loaded cell grid" are at Mid-Low, and "once actor's
//!   level goes up to Mid-Low, it never seems to go back to Low" (Chet in Goodsprings)
//!   [documented: same page]. Its rate is "unknown at this time" [documented:
//!   GetActorProcessingLevel]: here such an interior counts as buffered, every 18 s
//!   [design: the rate those actors had before the Low level existed]. The interiors are
//!   the destinations of the load doors placed in the exterior cells drawn this session.

use bevy::prelude::*;
use fnv_nav::Space;
use std::collections::HashMap;

use crate::world::{Streamer, WorldData};
use fnv_sim::{FormId, RefTable};
use std::collections::HashSet;

/// Exterior cells kept [observed: `uExterior Cell Buffer`, Fallout_default.ini].
pub const EXTERIOR_CELLS: usize = 36;
/// Interior cells kept [observed: `uInterior Cell Buffer`, Fallout_default.ini].
pub const INTERIOR_CELLS: usize = 3;

/// The cells drawn most recently (module notes).
#[derive(Debug, Default, Clone)]
pub struct CellBuffer {
    /// Exterior cells (worldspace, grid square) and interior cells, with the frame each
    /// was last drawn in.
    exterior: HashMap<(u32, (i32, i32)), u64>,
    interior: HashMap<u32, u64>,
    frame: u64,
    /// Interiors behind the load doors of the exterior cells drawn so far (Mid-Low),
    /// and the exterior cells whose doors were looked at.
    behind_doors: HashSet<u32>,
    doors_seen: HashSet<(u32, (i32, i32))>,
}

impl CellBuffer {
    /// The cells drawn in this frame: an interior, or the exterior grid squares of
    /// worldspace `world`.
    pub fn remember(&mut self, world: u32, interior: Option<u32>, exterior: impl Iterator<Item = (i32, i32)>) {
        self.frame += 1;
        match interior {
            Some(i) => {
                self.interior.insert(i, self.frame);
            }
            None => {
                for c in exterior {
                    self.exterior.insert((world, c), self.frame);
                }
            }
        }
        keep_newest(&mut self.exterior, EXTERIOR_CELLS);
        keep_newest(&mut self.interior, INTERIOR_CELLS);
    }

    /// Note the interiors the load doors of the drawn exterior cells of `world` lead to
    /// (each cell looked at once).
    pub fn note_doors(&mut self, refs: &RefTable, world: u32, exterior: impl Iterator<Item = (i32, i32)>) {
        let Some(grid) = refs.world_grid.get(&FormId(world)) else { return };
        for c in exterior {
            if !self.doors_seen.insert((world, c)) {
                continue;
            }
            let Some(cell) = grid.get(&c) else { continue };
            for r in refs.by_cell.get(cell).into_iter().flatten() {
                let to = refs.get(*r).and_then(|p| p.teleport.as_ref()).and_then(|t| refs.teleport_cell(t));
                if let Some(to) = to.filter(|&t| refs.world_of(t).is_none()) {
                    self.behind_doors.insert(to.0);
                }
            }
        }
    }

    /// Whether the cell at `feet` in `space` is in the buffer (or an interior behind the
    /// doors of the cells drawn so far, module notes).
    pub fn holds(&self, space: Space, feet: [f32; 3]) -> bool {
        match space {
            Space::Interior(c) => self.interior.contains_key(&c) || self.behind_doors.contains(&c),
            Space::World(w) => self.exterior.contains_key(&(w, fnv_world::cell_of(feet[0], feet[1]))),
        }
    }

    #[cfg(test)]
    fn len(&self) -> (usize, usize) {
        (self.exterior.len(), self.interior.len())
    }
}

/// Drop the oldest entries beyond `n` (among cells left in the same frame, any).
fn keep_newest<K: Copy + Eq + std::hash::Hash>(m: &mut HashMap<K, u64>, n: usize) {
    if m.len() <= n {
        return;
    }
    let mut by_age: Vec<(u64, K)> = m.iter().map(|(k, f)| (*f, *k)).collect();
    by_age.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
    for (_, k) in &by_age[n..] {
        m.remove(k);
    }
}

/// Note the cells drawn this frame (after `world::stream`), and the interiors behind
/// their doors (the placed references: the played game's, else the residents' own).
pub fn remember(mut streamer: ResMut<Streamer>, world: Res<WorldData>, sim: Option<Res<nvf_play::sim::SimState>>, residents: Option<ResMut<crate::residents::Residents>>) {
    let refs: Option<&'static RefTable> = match sim {
        Some(s) => Some(s.refs),
        None => residents.and_then(|mut r| r.own_game().map(|g| g.world.placed)),
    };
    let s = &mut *streamer;
    // The loading screen's placeholder key is no cell.
    let drawn = || s.loaded.keys().copied().filter(|&(x, _)| x != i32::MIN);
    if let (Some(refs), None) = (refs, s.interior) {
        s.buffer.note_doors(refs, world.world, drawn());
    }
    let drawn: Vec<(i32, i32)> = drawn().collect();
    s.buffer.remember(world.world, s.interior, drawn.into_iter());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(x0: i32, y0: i32) -> impl Iterator<Item = (i32, i32)> {
        (-2..=2).flat_map(move |dx| (-2..=2).map(move |dy| (x0 + dx, y0 + dy)))
    }

    fn feet(c: (i32, i32)) -> [f32; 3] {
        [c.0 as f32 * 4096.0 + 100.0, c.1 as f32 * 4096.0 + 100.0, 0.0]
    }

    #[test]
    fn cells_left_stay_until_newer_ones_push_them_out() {
        let mut b = CellBuffer::default();
        b.remember(7, None, grid(0, 0));
        // One step north: the southern row (y = -2) is no longer drawn, but buffered.
        b.remember(7, None, grid(0, 1));
        assert!(b.holds(Space::World(7), feet((0, -2))));
        assert_eq!(b.len().0, 30);
        // More steps: 25 drawn and 11 left, the oldest go.
        b.remember(7, None, grid(0, 2));
        assert_eq!(b.len().0, 35);
        b.remember(7, None, grid(0, 3));
        assert_eq!(b.len().0, 36);
        b.remember(7, None, grid(0, 4));
        assert!(!b.holds(Space::World(7), feet((0, -2))), "pushed out");
        assert!(b.holds(Space::World(7), feet((0, 0))), "a row left later is still in");
        assert!(!b.holds(Space::World(8), feet((0, 3))), "another worldspace");
    }

    #[test]
    fn an_interior_keeps_the_exterior_buffer_and_three_interiors_are_kept() {
        let mut b = CellBuffer::default();
        b.remember(7, None, grid(0, 0));
        for i in 1..=4 {
            b.remember(7, Some(100 + i), std::iter::empty());
        }
        assert!(b.holds(Space::World(7), feet((2, 2))), "the exterior stays buffered");
        assert!(!b.holds(Space::Interior(101), [0.0; 3]));
        assert!((102..=104).all(|i| b.holds(Space::Interior(i), [0.0; 3])));
    }
}
