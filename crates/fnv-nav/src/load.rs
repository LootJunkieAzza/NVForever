//! Every NAVM and NAVI record of a load order, the last definition of each navmesh
//! winning (as for every other record; docs/spec-notes/navmesh.md §2).

use crate::record::{read_navi, read_navm, NavInfoMap, NavMeshRecord};
use fnv_esm::raw::{group_type, visit_records};
use fnv_esm::{GroupHeader, LoadOrder, Record};
use rayon::prelude::*;
use std::collections::BTreeMap;

/// The navmesh records of a load order.
#[derive(Debug, Default)]
pub struct NavRecords {
    /// Winning definition of every navmesh, by load-order FormID. Deleted winners are
    /// left out (and counted).
    pub navmeshes: BTreeMap<u32, NavMeshRecord>,
    /// The cell whose children group stores each navmesh's winning definition.
    pub stored_in: BTreeMap<u32, u32>,
    /// NAVI records, in load order.
    pub info_maps: Vec<NavInfoMap>,
    /// How many NAVM definitions each plugin holds (index = load-order index).
    pub definitions: Vec<usize>,
    /// Winning definitions with the Deleted flag.
    pub deleted: usize,
    /// Records that could not be read (FormID, message).
    pub errors: Vec<(u32, String)>,
}

impl NavRecords {
    /// Read every NAVM and NAVI record of the load order (in parallel; deterministic).
    pub fn read(lo: &LoadOrder) -> NavRecords {
        // (plugin, record, the cell group it is in)
        let mut found: Vec<(usize, &Record, Option<u32>)> = Vec::new();
        let mut navi: Vec<(usize, &Record)> = Vec::new();
        for (pi, p) in lo.plugins.iter().enumerate() {
            visit_records(&p.nodes[1..], &mut |r, stack: &[&GroupHeader]| match &r.header.sig {
                b"NAVM" => {
                    let cell = stack
                        .iter()
                        .rev()
                        .find(|g| {
                            matches!(
                                g.group_type,
                                group_type::CELL_PERSISTENT_CHILDREN
                                    | group_type::CELL_TEMPORARY_CHILDREN
                                    | group_type::CELL_VISIBLE_DISTANT_CHILDREN
                            )
                        })
                        .and_then(|g| lo.to_global(pi, g.label_u32()));
                    found.push((pi, r, cell));
                }
                b"NAVI" => navi.push((pi, r)),
                _ => {}
            });
        }
        let mut out = NavRecords { definitions: vec![0; lo.plugins.len()], ..Default::default() };
        for &(pi, _, _) in &found {
            out.definitions[pi] += 1;
        }
        // The winner of each FormID: the last definition in load order.
        let mut winner: BTreeMap<u32, usize> = BTreeMap::new();
        for (i, &(pi, r, _)) in found.iter().enumerate() {
            if let Some(g) = lo.to_global(pi, r.header.form_id) {
                winner.insert(g, i);
            }
        }
        let read: Vec<(u32, Option<u32>, Result<NavMeshRecord, String>)> = winner
            .par_iter()
            .map(|(&g, &i)| {
                let (pi, r, cell) = found[i];
                (g, cell, read_navm(lo, pi, r))
            })
            .collect();
        for (g, cell, res) in read {
            match res {
                Ok(m) if m.deleted() => out.deleted += 1,
                Ok(m) => {
                    if let Some(c) = cell {
                        out.stored_in.insert(g, c);
                    }
                    out.navmeshes.insert(g, m);
                }
                Err(e) => out.errors.push((g, e)),
            }
        }
        for (pi, r) in navi {
            match read_navi(lo, pi, r) {
                Ok(m) => out.info_maps.push(m),
                Err(e) => out.errors.push((lo.to_global(pi, r.header.form_id).unwrap_or(0), e)),
            }
        }
        out
    }
}
