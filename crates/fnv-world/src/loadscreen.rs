//! Loading screens (LSCR): the picture and text shown while the game loads, and the
//! places a screen belongs to. Facts and tags: docs/spec-notes/loading-screens.md.

use crate::fields::{formid_of, global, num, winner, Fields};
use fnv_esm::{LoadOrder, Schema};
use std::collections::BTreeSet;

/// Record flag "deleted" [documented: xEdit record header flags].
const DELETED: u32 = 0x20;

/// Where a screen belongs (LSCR LNAM) [documented: fopdoc FalloutNV/Records/LSCR].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenLocation {
    /// "Direct": a cell or a worldspace (FormID).
    Direct(u32),
    /// "Indirect": a grid square of a worldspace.
    Grid { world: u32, x: i32, y: i32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadScreen {
    pub form_id: u32,
    pub edid: Option<String>,
    /// ICON as stored: a path under `textures\`.
    pub texture: Option<String>,
    /// DESC: the text shown with the picture.
    pub text: String,
    /// Empty: a screen for anywhere.
    pub locations: Vec<ScreenLocation>,
}

/// Where the player is being taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Destination {
    /// The interior cell, when going indoors.
    pub interior: Option<u32>,
    /// Outdoors: the worldspace and the grid square arrived in.
    pub exterior: Option<(u32, (i32, i32))>,
}

/// A grid location covers the squares within this many steps of its own: the squares
/// loaded around a player standing on it [hypothesis: the game's own rule is not known;
/// loading-screens.md §3].
pub const GRID_REACH: i32 = 2;

impl LoadScreen {
    /// Whether one of the screen's locations is the destination.
    pub fn belongs_to(&self, d: &Destination) -> bool {
        self.locations.iter().any(|l| match *l {
            ScreenLocation::Direct(id) => d.interior == Some(id) || d.exterior.is_some_and(|(w, _)| w == id),
            ScreenLocation::Grid { world, x, y } => {
                d.exterior.is_some_and(|(w, g)| w == world && (g.0 - x).abs() <= GRID_REACH && (g.1 - y).abs() <= GRID_REACH)
            }
        })
    }
}

/// Every loading screen of the load order (the winning definition of each), by FormID.
pub fn load_screens(lo: &LoadOrder, schema: &Schema) -> Vec<LoadScreen> {
    let mut ids = BTreeSet::new();
    for (pi, records) in lo.records.iter().enumerate() {
        for r in records.iter().filter(|r| &r.header.sig == b"LSCR") {
            ids.extend(lo.to_global(pi, r.header.form_id));
        }
    }
    ids.into_iter().filter_map(|id| load_screen(lo, schema, id)).collect()
}

fn load_screen(lo: &LoadOrder, schema: &Schema, form_id: u32) -> Option<LoadScreen> {
    let (pi, r) = winner(lo, form_id)?;
    if &r.header.sig != b"LSCR" || r.header.flags & DELETED != 0 {
        return None;
    }
    let f = Fields::new(r, pi, schema).ok()?;
    let locations = f
        .subs
        .iter()
        .filter(|s| &s.sig == b"LNAM")
        .filter_map(|s| {
            let v = f.decode(s)?;
            let id = |k: &str| v.get(k).and_then(formid_of).and_then(|l| global(lo, pi, l));
            match (id("direct"), id("indirect_world")) {
                (Some(direct), _) => Some(ScreenLocation::Direct(direct)),
                (None, Some(world)) => {
                    Some(ScreenLocation::Grid { world, x: num(&v, "indirect_grid_x")? as i32, y: num(&v, "indirect_grid_y")? as i32 })
                }
                (None, None) => None,
            }
        })
        .collect();
    Some(LoadScreen {
        form_id,
        edid: f.string(b"EDID"),
        texture: f.string(b"ICON").filter(|t| !t.is_empty()),
        text: f.string(b"DESC").unwrap_or_default(),
        locations,
    })
}

/// The screen for a trip to `to`. Screens that belong to the destination and screens for
/// anywhere are both allowed [documented: GECK wiki SetLocationSpecificLoadScreensOnly,
/// which exists to leave only the former]; when the destination has its own, one of them
/// is taken every other time [design: the game's odds are not known]. `roll` is any
/// number, e.g. random: the same roll gives the same screen.
pub fn choose<'a>(screens: &'a [LoadScreen], to: &Destination, roll: u64) -> Option<&'a LoadScreen> {
    let own: Vec<&LoadScreen> = screens.iter().filter(|s| s.belongs_to(to)).collect();
    let anywhere: Vec<&LoadScreen> = screens.iter().filter(|s| s.locations.is_empty()).collect();
    let pool = match (own.is_empty(), anywhere.is_empty()) {
        (true, true) => return None,
        (false, true) => own,
        (true, false) => anywhere,
        (false, false) => {
            if roll & 1 == 0 {
                own
            } else {
                anywhere
            }
        }
    };
    Some(pool[((roll >> 1) % pool.len() as u64) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOJAVE: u32 = 0xDA726;
    const SALOON: u32 = 0x106FE5;

    fn screen(form_id: u32, locations: Vec<ScreenLocation>) -> LoadScreen {
        LoadScreen { form_id, edid: None, texture: None, text: String::new(), locations }
    }

    fn outdoors(x: i32, y: i32) -> Destination {
        Destination { interior: None, exterior: Some((MOJAVE, (x, y))) }
    }

    #[test]
    fn locations() {
        let town = screen(1, vec![ScreenLocation::Grid { world: MOJAVE, x: -17, y: -2 }]);
        assert!(town.belongs_to(&outdoors(-17, -2)));
        assert!(town.belongs_to(&outdoors(-18, 0)));
        assert!(!town.belongs_to(&outdoors(-17, 1)));
        assert!(!town.belongs_to(&Destination { interior: None, exterior: Some((0x100, (-17, -2))) }));
        assert!(!town.belongs_to(&Destination { interior: Some(SALOON), exterior: None }));

        let bar = screen(2, vec![ScreenLocation::Direct(0x200), ScreenLocation::Direct(SALOON)]);
        assert!(bar.belongs_to(&Destination { interior: Some(SALOON), exterior: None }));
        assert!(!bar.belongs_to(&outdoors(0, 0)));
        // A worldspace named directly covers all of it.
        assert!(screen(3, vec![ScreenLocation::Direct(MOJAVE)]).belongs_to(&outdoors(40, 40)));
        assert!(!screen(4, vec![]).belongs_to(&outdoors(0, 0)));
    }

    #[test]
    fn choice() {
        let screens = vec![
            screen(1, vec![]),
            screen(2, vec![ScreenLocation::Direct(SALOON)]),
            screen(3, vec![]),
            screen(4, vec![ScreenLocation::Direct(0x999)]),
        ];
        let saloon = Destination { interior: Some(SALOON), exterior: None };
        let picked = |to: &Destination| -> BTreeSet<u32> { (0..64).filter_map(|r| choose(&screens, to, r)).map(|s| s.form_id).collect() };
        // The saloon's own screen and the ones for anywhere; never another place's.
        assert_eq!(picked(&saloon), BTreeSet::from([1, 2, 3]));
        assert_eq!(picked(&outdoors(0, 0)), BTreeSet::from([1, 3]));
        // Its own screen every other time.
        assert_eq!((0..64).filter(|&r| choose(&screens, &saloon, r).unwrap().form_id == 2).count(), 32);
        assert_eq!(choose(&screens, &saloon, 5), choose(&screens, &saloon, 5));
        assert_eq!(choose(&[], &saloon, 0), None);
        // Only screens of other places: nothing fits.
        assert_eq!(choose(&screens[3..], &saloon, 0), None);
    }
}
