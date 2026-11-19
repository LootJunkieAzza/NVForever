//! References whose bounds reach into exterior grid squares other than the one whose cell
//! lists them (#2795).
//!
//! Physics loads the squares around the player and, in each, the references its cell
//! record lists. A reference placed in one square can stretch over several:
//! HeliosPowerPlant (REFR 0x000B0A0E, placed in square 5,-1) is the front plaza of HELIOS
//! One down to y -8801, inside square 5,-3 [observed: `fnvtool phys cell` rays, #2795], so
//! a player in 5,-3 stood on the terrain under the plaza. This index lists, for each
//! square, the references placed elsewhere whose bounds cover part of it, so a square
//! loads them too [design, #2795].
//!
//! A reference's bounds are its base record's OBND box (min, max corners in the model's
//! space [documented: xEdit wbDefinitionsCommon.pas wbObjectBounds,
//! schemas/fnv/common.json]) scaled, turned by the reference's full rotation
//! ([`crate::transform::ref_rotation`]) and moved to its position; the squares are those
//! the box's horizontal extent covers. That the OBND box covers the model's collision is
//! [hypothesis]: HeliosPowerPlant's does (OBND y -4750..1142 around its origin, its
//! collision y -2913..-8801 placed [observed: #2795]); a model whose collision reaches
//! past its OBND is still missed past the bounds.
//!
//! Only references whose box reaches beyond the square that lists them get entries, and
//! only placed REFRs of the worldspace's exterior cells: persistent references load by
//! the square they stand in, as before, and actors have their own bodies.

use crate::fields::{num, winner, Fields};
use crate::placed::placement;
use crate::transform::ref_rotation;
use crate::{cell_of, WorldIndex};
use fnv_esm::{LoadOrder, Schema};
use std::collections::HashMap;

/// An OBND box: (min, max) corners in the model's space.
pub type Bounds = ([f32; 3], [f32; 3]);

/// The inclusive range of grid squares (min corner square, max corner square).
pub type Squares = ((i32, i32), (i32, i32));

/// A base record's OBND, when it has one.
pub fn base_bounds(lo: &LoadOrder, schema: &Schema, base: u32) -> Option<Bounds> {
    let (pi, r) = winner(lo, base)?;
    let f = Fields::new(r, pi, schema).ok()?;
    let b = f.get(b"OBND")?;
    let corner = |k: &str| -> Option<[f32; 3]> {
        let c = b.get(k)?;
        Some([num(c, "x")? as f32, num(c, "y")? as f32, num(c, "z")? as f32])
    };
    Some((corner("min")?, corner("max")?))
}

/// The grid squares the horizontal extent of `bounds` covers once placed at `position`,
/// turned by `rotation` (stored Euler angles, radians) and scaled by `scale`.
pub fn squares_reached(position: [f32; 3], rotation: [f32; 3], scale: f32, bounds: Bounds) -> Squares {
    let m = ref_rotation(rotation);
    let (lo, hi) = bounds;
    let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for i in 0..8 {
        let c = [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
        for (k, (mn, mx)) in min.iter_mut().zip(max.iter_mut()).enumerate() {
            let v = position[k] + scale * (m[k * 3] * c[0] + m[k * 3 + 1] * c[1] + m[k * 3 + 2] * c[2]);
            *mn = mn.min(v);
            *mx = mx.max(v);
        }
    }
    (cell_of(min[0], min[1]), cell_of(max[0], max[1]))
}

/// For each exterior grid square of one worldspace, the references placed in another
/// square whose bounds reach into it, by FormID.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReachIndex {
    by_square: HashMap<(i32, i32), Vec<u32>>,
    /// References reaching further than [`LONG_REACH`] squares from their own: (FormID,
    /// own square, the squares covered), checked by containment rather than listed under
    /// each square, so a huge box does not fill the map.
    long: Vec<(u32, (i32, i32), Squares)>,
    /// References with entries (each reaches beyond its own square).
    pub refs: usize,
}

/// A reference reaching further than this many squares from its own is kept as a
/// rectangle (`ReachIndex::long`) and logged (a WARN naming it and its reach): the largest
/// buildings of FalloutNV.esm reach two or three [observed: #2795's table], so such a box
/// more likely comes from a broken record. It loads with every square it covers all the
/// same [design, #2795].
const LONG_REACH: i64 = 32;

/// One placed reference as [`ReachIndex::build`] reads it: FormID, the square whose cell
/// lists it, its base, position, rotation and scale.
struct Placed {
    form_id: u32,
    own: (i32, i32),
    base: u32,
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
}

impl ReachIndex {
    /// From (reference, the square whose cell lists it, the squares its bounds cover):
    /// the reference is listed under every covered square but its own, whether a cell
    /// record is there or not (physics loads such squares too). A reference whose bounds
    /// stay in its own square gives no entry. One reaching further than [`LONG_REACH`]
    /// squares from its own is kept as a rectangle and logged as a warning.
    pub fn from_entries(entries: impl IntoIterator<Item = (u32, (i32, i32), Squares)>) -> ReachIndex {
        let mut ix = ReachIndex::default();
        for (r, own, reach) in entries {
            let ((x0, y0), (x1, y1)) = reach;
            let far = |a: i32, b: i32| (i64::from(a) - i64::from(b)).abs();
            if far(own.0, x0).max(far(x1, own.0)).max(far(own.1, y0)).max(far(y1, own.1)) > LONG_REACH {
                tracing::warn!(
                    "reach: 0x{r:08X} (placed in square {},{}) has bounds over squares {x0},{y0} to {x1},{y1}, more than {LONG_REACH} squares from its own (a broken record?); it loads with every one of them",
                    own.0,
                    own.1,
                );
                ix.long.push((r, own, reach));
                ix.refs += 1;
                continue;
            }
            let mut any = false;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if (x, y) != own {
                        ix.by_square.entry((x, y)).or_default().push(r);
                        any = true;
                    }
                }
            }
            ix.refs += any as usize;
        }
        for v in ix.by_square.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        ix
    }

    /// The index of worldspace `world`: every placed REFR of its exterior cell records
    /// (not its persistent cell's), with its base's OBND. Reads each reference once, on
    /// as many threads as the machine has (up to 8).
    pub fn build(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, world: u32) -> ReachIndex {
        let Some(ws) = ix.worldspaces.get(&world) else { return ReachIndex::default() };
        let cells: Vec<((i32, i32), &[u32])> =
            ws.grid.iter().filter_map(|(&g, id)| ix.cells.get(id).map(|c| (g, c.refs.as_slice()))).collect();
        let placed_refs = |part: &[((i32, i32), &[u32])]| -> Vec<Placed> {
            let mut out = Vec::new();
            for &(own, refs) in part {
                for &r in refs {
                    if !ix.refs.get(&r).is_some_and(|e| &e.sig == b"REFR" && !e.persistent) {
                        continue;
                    }
                    let Some(pl) = placement(lo, schema, r).filter(|p| !p.deleted()) else { continue };
                    let Some(base) = pl.base else { continue };
                    out.push(Placed { form_id: r, own, base, position: pl.position, rotation: pl.rotation, scale: pl.scale });
                }
            }
            out
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8);
        let chunk = cells.len().div_ceil(threads).max(1);
        let placed: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = cells.chunks(chunk).map(|part| s.spawn(move || placed_refs(part))).collect();
            handles.into_iter().flat_map(|h| h.join().expect("reach index thread")).collect()
        });
        let mut bases: Vec<u32> = placed.iter().map(|p| p.base).collect();
        bases.sort_unstable();
        bases.dedup();
        let bchunk = bases.len().div_ceil(threads).max(1);
        let bounds: HashMap<u32, Bounds> = std::thread::scope(|s| {
            let handles: Vec<_> = bases
                .chunks(bchunk)
                .map(|part| s.spawn(move || part.iter().filter_map(|&b| base_bounds(lo, schema, b).map(|v| (b, v))).collect::<Vec<_>>()))
                .collect();
            handles.into_iter().flat_map(|h| h.join().expect("reach index thread")).collect()
        });
        let entries = placed.into_iter().filter_map(|p| {
            let b = bounds.get(&p.base)?;
            Some((p.form_id, p.own, squares_reached(p.position, p.rotation, p.scale, *b)))
        });
        ReachIndex::from_entries(entries)
    }

    /// The references placed elsewhere whose bounds reach into `square`, by FormID.
    pub fn reaching(&self, square: (i32, i32)) -> Vec<u32> {
        let mut v: Vec<u32> = self.by_square.get(&square).cloned().unwrap_or_default();
        let (x, y) = square;
        v.extend(
            self.long
                .iter()
                .filter(|(_, own, ((x0, y0), (x1, y1)))| *own != square && (*x0..=*x1).contains(&x) && (*y0..=*y1).contains(&y))
                .map(|e| e.0),
        );
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Entries over every square (a reference counts once per square it reaches into),
    /// and one per reference kept as a rectangle.
    pub fn entries(&self) -> usize {
        self.by_square.values().map(Vec::len).sum::<usize>() + self.long.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CELL_SIZE;

    /// The centre of grid square `g`.
    fn square_centre(g: (i32, i32)) -> [f32; 2] {
        [(g.0 as f32 + 0.5) * CELL_SIZE, (g.1 as f32 + 0.5) * CELL_SIZE]
    }

    #[test]
    fn a_box_two_squares_long_covers_three_squares() {
        // Placed in the middle of square (5, -1), reaching 9,000 units south (OBND y
        // -9000..500): it covers 5,-3 to 5,-1.
        let c = square_centre((5, -1));
        let b = ([-200.0, -9000.0, 0.0], [200.0, 500.0, 100.0]);
        assert_eq!(squares_reached([c[0], c[1], 0.0], [0.0; 3], 1.0, b), ((5, -3), (5, -1)));
        // Turned a quarter about z it lies east-west instead.
        let ((x0, y0), (x1, y1)) = squares_reached([c[0], c[1], 0.0], [0.0, 0.0, std::f32::consts::FRAC_PI_2], 1.0, b);
        assert_eq!((y0, y1), (-1, -1));
        assert_eq!(x1 - x0, 2, "three squares east-west");
        // Half the size: one square south only.
        assert_eq!(squares_reached([c[0], c[1], 0.0], [0.0; 3], 0.5, b), ((5, -2), (5, -1)));
    }

    #[test]
    fn a_box_in_its_own_square_gives_no_entry() {
        let c = square_centre((0, 0));
        let s = squares_reached([c[0], c[1], 0.0], [0.0, 0.0, 0.7], 1.0, ([-100.0; 3], [100.0; 3]));
        assert_eq!(s, ((0, 0), (0, 0)));
        let ix = ReachIndex::from_entries([(7, (0, 0), s)]);
        assert_eq!(ix.refs, 0);
        assert_eq!(ix.entries(), 0);
    }

    #[test]
    fn the_players_square_lists_a_reference_placed_two_squares_away_that_reaches_it() {
        // The player stands in (5, -3). The plant is placed in (5, -1) and reaches down to
        // (5, -3); a rock placed in (5, -1) reaches only (5, -2); a box in (7, -3) stays home.
        let ix = ReachIndex::from_entries([
            (0x0B0A0E, (5, -1), ((4, -3), (5, -1))),
            (0x000100, (5, -1), ((5, -2), (5, -1))),
            (0x000200, (7, -3), ((7, -3), (7, -3))),
        ]);
        assert_eq!(ix.reaching((5, -3)), vec![0x0B0A0E]);
        assert_eq!(ix.reaching((4, -3)), vec![0x0B0A0E]);
        assert_eq!(ix.reaching((5, -2)), vec![0x000100, 0x0B0A0E]);
        assert!(ix.reaching((5, -1)).is_empty(), "never listed under its own square");
        assert!(ix.reaching((7, -3)).is_empty());
        assert_eq!(ix.refs, 2);
    }

    #[test]
    fn a_square_past_the_last_cell_record_lists_a_reference_reaching_it() {
        // A structure in the easternmost cell (40, 0) reaching (42, 0), where no cell
        // record is: physics loads (42, 0) all the same, and it lists the structure.
        let ix = ReachIndex::from_entries([(2, (40, 0), ((40, 0), (42, 0)))]);
        assert_eq!(ix.reaching((41, 0)), vec![2]);
        assert_eq!(ix.reaching((42, 0)), vec![2]);
        assert!(ix.reaching((43, 0)).is_empty());
    }

    #[test]
    fn a_huge_box_is_kept_as_a_rectangle() {
        // Reaching 1,000 squares either way: no square entries, found by containment in
        // every square it covers but its own.
        let ix = ReachIndex::from_entries([(1, (0, 0), ((-1000, -1000), (1000, 1000))), (3, (5, 5), ((5, 5), (6, 5)))]);
        assert_eq!(ix.refs, 2);
        assert_eq!(ix.entries(), 1 + 1, "one square entry for the small box, one rectangle");
        assert_eq!(ix.reaching((900, -900)), vec![1]);
        assert_eq!(ix.reaching((6, 5)), vec![1, 3], "both, once each, by FormID");
        assert!(ix.reaching((0, 0)).is_empty(), "never under its own square");
        assert!(ix.reaching((1001, 0)).is_empty());
        // A box from a record with no usable numbers (the corners at the ends of i32).
        let ix = ReachIndex::from_entries([(4, (0, 0), ((i32::MIN, i32::MIN), (i32::MAX, i32::MAX)))]);
        assert_eq!(ix.reaching((i32::MAX, i32::MIN)), vec![4]);
    }

    #[test]
    fn an_empty_world_index_builds_an_empty_reach_index() {
        let lo = LoadOrder::new(&[]).expect("empty load order");
        let ix = ReachIndex::build(&lo, &Schema::default(), &WorldIndex::default(), 0x3C);
        assert_eq!(ix, ReachIndex::default());
    }
}
