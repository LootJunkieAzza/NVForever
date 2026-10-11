//! Imposters: references drawn while the player stands in a region that lists them,
//! at any distance, instead of with their cell (docs/spec-notes/world.md §2).
//!
//! A reference with the XSRF Imposter flag "is displayed always when the player is in
//! the region"; it "must be added to the Imposter list for the region" [documented: GECK
//! wiki "Reference", owner's export]. The list is the region's RDID, after an RDAT of
//! type 8 [documented: xEdit wbRegionDataEntries type 8 "imposter"; observed: RDID only
//! there, a list of REFR FormIDs]. Outside every region that lists it the reference is
//! not drawn [hypothesis: the Strip's and South Vegas's imposters stand on the full
//! buildings they stand in for, e.g. Bldg03Imposter02 0015EFBC on the BLD11 pieces
//! south-east of the Poseidon gas station, and the region there (StripImposterClose)
//! lists only the near versions of the Strip's light glows; drawing them hid the ruins
//! behind flat blocks, owner's screenshot 2026-10-11].

use crate::fields::{formid_of, global, num, Fields};
use crate::placed::ref_flags;
use fnv_esm::{LoadOrder, Schema};
use std::collections::BTreeSet;

/// RDAT data type of an imposter list [documented: xEdit wbRegionDataEntries].
const IMPOSTER_DATA: u64 = 8;

/// A region with an imposter list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImposterRegion {
    pub form_id: u32,
    pub edid: Option<String>,
    /// WNAM: the worldspace its areas lie in.
    pub world: Option<u32>,
    /// RPLI/RPLD: each area a polygon of worldspace x, y points.
    pub areas: Vec<Vec<[f32; 2]>>,
    /// RDID: the references it shows.
    pub refs: Vec<u32>,
}

impl ImposterRegion {
    /// Whether a worldspace point lies inside one of the areas (even-odd rule)
    /// [hypothesis: an area is the plain polygon of its points; the edge fall-off (RPLI)
    /// is ignored, as for the regions' other uses].
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.areas.iter().any(|pts| inside(pts, x, y))
    }
}

fn inside(pts: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let [xi, yi] = pts[i];
        let [xj, yj] = pts[(i + n - 1) % n];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
    }
    inside
}

/// Every region of the load order with an imposter list.
#[derive(Debug, Clone, Default)]
pub struct ImposterRegions {
    pub regions: Vec<ImposterRegion>,
}

impl ImposterRegions {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> ImposterRegions {
        let mut regions = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let pi = d.plugin as usize;
            let r = lo.records[pi][d.ordinal as usize];
            if &r.header.sig != b"REGN" || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let mut rg = ImposterRegion { form_id: g, edid: f.string(b"EDID"), world: f.formid(b"WNAM", lo), ..Default::default() };
            let mut data_type = None;
            for s in &f.subs {
                match &s.sig {
                    b"RPLI" => rg.areas.push(Vec::new()),
                    b"RPLD" => {
                        let pts: Vec<[f32; 2]> = f
                            .decode(s)
                            .as_ref()
                            .and_then(|v| v.as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(|p| Some([num(p, "x")? as f32, num(p, "y")? as f32]))
                            .collect();
                        match rg.areas.last_mut() {
                            Some(a) if a.is_empty() => *a = pts,
                            _ => rg.areas.push(pts),
                        }
                    }
                    b"RDAT" => data_type = f.decode(s).and_then(|v| v.get("type")?.as_u64()),
                    b"RDID" if data_type == Some(IMPOSTER_DATA) => {
                        let list = f.decode(s);
                        let ids = list.as_ref().and_then(|v| v.as_array()).into_iter().flatten();
                        rg.refs.extend(ids.filter_map(|v| global(lo, pi, formid_of(v)?)));
                    }
                    _ => {}
                }
            }
            rg.areas.retain(|a| a.len() >= 3);
            if !rg.refs.is_empty() {
                regions.push(rg);
            }
        }
        regions.sort_by_key(|r| r.form_id);
        ImposterRegions { regions }
    }

    /// The references listed by the regions of worldspace `world` that contain the point
    /// (x, y): the imposters to draw for a player standing there.
    pub fn shown(&self, world: u32, x: f32, y: f32) -> BTreeSet<u32> {
        self.regions.iter().filter(|r| r.world == Some(world) && r.contains(x, y)).flat_map(|r| r.refs.iter().copied()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_by_area() {
        let square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let far = vec![[100.0, 0.0], [110.0, 0.0], [105.0, 10.0]];
        let rs = ImposterRegions {
            regions: vec![
                ImposterRegion { form_id: 1, world: Some(7), areas: vec![square.clone(), far], refs: vec![10, 11], ..Default::default() },
                ImposterRegion { form_id: 2, world: Some(7), areas: vec![square.clone()], refs: vec![11, 12], ..Default::default() },
                ImposterRegion { form_id: 3, world: Some(8), areas: vec![square], refs: vec![13], ..Default::default() },
            ],
        };
        assert_eq!(rs.shown(7, 5.0, 5.0), BTreeSet::from([10, 11, 12]));
        assert_eq!(rs.shown(7, 105.0, 2.0), BTreeSet::from([10, 11]));
        assert_eq!(rs.shown(7, 50.0, 5.0), BTreeSet::new());
        assert_eq!(rs.shown(8, 5.0, 5.0), BTreeSet::from([13]));
        // The triangle's slanted edges: outside next to them.
        assert_eq!(rs.shown(7, 101.0, 9.0), BTreeSet::new());
    }
}
