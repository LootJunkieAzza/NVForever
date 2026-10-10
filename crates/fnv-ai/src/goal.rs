//! Where a package sends its actor: the package's location (PLDT) turned into a place
//! on the navmesh. Which package types go to their location, and what each location
//! kind means: docs/spec-notes/packages.md §3 [documented: GECK wiki, Category:Packages
//! "Standard Location" and the page of each type]; what is not carried out yet:
//! docs/design/actor-behaviour.md §5.

use fnv_game::game::Game;
use fnv_game::package::{furniture, LocationKind, PackageDef, PackageType};
use fnv_nav::graph::NONE;
use fnv_nav::{NavGraph, Place, Space};
use fnv_sim::{FormId, RefTable};

/// What an actor has to do to be where its package wants it.
#[derive(Debug, Clone, PartialEq)]
pub enum Goal {
    /// Nothing to walk to: the package acts where the actor is ("near current
    /// location", or no location).
    Here,
    /// Be within `radius` of a place.
    Near { place: Place, radius: f32 },
    /// Be anywhere inside an interior cell ("in cell": the radius is not used); `at` is
    /// a point of the cell's navmesh to walk towards.
    Inside { space: Space, at: Place },
    /// Walk a patrol route: the location's reference and the references linked from it.
    Route(Route),
    /// The package type or the location kind is not carried out yet: the actor stays
    /// where it is.
    NotCarriedOut(&'static str),
    /// The location names something that is not in the world: the package cannot be
    /// carried out at all.
    Missing(&'static str),
}

/// A patrol route.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The patrol points in order (the reference, where it is).
    pub points: Vec<(FormId, Place)>,
    /// The chain of linked references comes back to this point after the last one.
    pub loops_to: Option<usize>,
    /// PKPT "repeatable".
    pub repeatable: bool,
}

/// The navmesh space of a cell.
pub fn space_of_cell(refs: &RefTable, cell: FormId) -> Space {
    match refs.world_of(cell) {
        Some(w) => Space::World(w.0),
        None => Space::Interior(cell.0),
    }
}

/// The cell of a place: the interior, or the exterior cell under the position.
pub fn cell_of_place(refs: &RefTable, place: &Place) -> Option<FormId> {
    match place.space {
        Space::Interior(c) => Some(FormId(c)),
        Space::World(w) => refs.world_grid.get(&FormId(w))?.get(&fnv_world::cell_of(place.pos[0], place.pos[1])).copied(),
    }
}

/// Where a reference is now (moved references included).
pub fn place_of_ref(game: &Game, r: FormId) -> Option<Place> {
    let refs = game.world.placed;
    let (cell, pos) = match game.world.peek(r) {
        Some(s) => (s.cell, s.pos),
        None => {
            let p = refs.get(r)?;
            (p.cell, p.pos)
        }
    };
    Some(Place { space: space_of_cell(refs, cell?), pos })
}

/// Where a reference was placed in the editor.
pub fn editor_place(refs: &RefTable, r: FormId) -> Option<Place> {
    let p = refs.get(r)?;
    Some(Place { space: space_of_cell(refs, p.cell?), pos: p.pos })
}

/// A point of a cell's navmesh to walk towards: the centre of the triangle nearest the
/// middle of the cell's largest walkable area [our choice].
pub fn cell_anchor(nav: &NavGraph, refs: &RefTable, cell: FormId) -> Option<Place> {
    let space = space_of_cell(refs, cell);
    let tris: Vec<u32> = nav
        .meshes
        .iter()
        .filter(|m| m.enabled && m.cell == Some(cell.0))
        .flat_map(|m| m.first_tri..m.first_tri + m.tri_count)
        .filter(|&t| nav.area[t as usize] != NONE)
        .collect();
    // The area with the most triangles in this cell.
    let mut counts: std::collections::BTreeMap<u32, usize> = Default::default();
    for &t in &tris {
        *counts.entry(nav.area[t as usize]).or_default() += 1;
    }
    let (&area, _) = counts.iter().max_by_key(|(a, n)| (**n, std::cmp::Reverse(**a)))?;
    let of_area: Vec<u32> = tris.into_iter().filter(|&t| nav.area[t as usize] == area).collect();
    let n = of_area.len() as f32;
    let mean = of_area.iter().fold([0.0f32; 3], |m, &t| {
        let c = nav.centre(t);
        [m[0] + c[0] / n, m[1] + c[1] / n, m[2] + c[2] / n]
    });
    let d2 = |t: &u32| {
        let c = nav.centre(*t);
        (c[0] - mean[0]).powi(2) + (c[1] - mean[1]).powi(2) + (c[2] - mean[2]).powi(2)
    };
    let best = of_area.iter().min_by(|a, b| d2(a).total_cmp(&d2(b)))?;
    Some(Place { space, pos: nav.centre(*best) })
}

/// The chain of linked references (XLKR) from `start`, as patrol points.
fn route_from(game: &Game, start: FormId, repeatable: bool) -> Option<Route> {
    let refs = game.world.placed;
    let mut ids = vec![start];
    let mut loops_to = None;
    let mut at = start;
    while let Some(next) = refs.get(at).and_then(|r| r.linked) {
        if let Some(i) = ids.iter().position(|x| *x == next) {
            loops_to = Some(i);
            break;
        }
        if ids.len() >= 256 {
            break;
        }
        ids.push(next);
        at = next;
    }
    let mut points = Vec::with_capacity(ids.len());
    for id in ids {
        match place_of_ref(game, id) {
            Some(p) => points.push((id, p)),
            // A point that is nowhere ends the route there.
            None => {
                loops_to = None;
                break;
            }
        }
    }
    (!points.is_empty()).then_some(Route { points, loops_to, repeatable })
}

/// The goal of `actor` under package `def`.
///
/// Carried out: Travel, Sandbox, Sleep, Eat, Guard and Wander go to their location and
/// stay within its radius [documented: the page of each type]; a Patrol located near a
/// reference, or near the actor's linked reference, walks the chain of references linked
/// from it [documented: Patrol Package; observed: GSVictorRef, packages.md §3]. A Patrol
/// with another location goes to that location [hypothesis].
pub fn resolve(game: &Game, nav: &NavGraph, actor: FormId, def: &PackageDef) -> Goal {
    let refs = game.world.placed;
    match def.ty {
        PackageType::Travel | PackageType::Sandbox | PackageType::Sleep | PackageType::Eat | PackageType::Guard | PackageType::Wander | PackageType::Patrol => {}
        PackageType::Follow | PackageType::Accompany | PackageType::Escort => return Goal::NotCarriedOut("a package that follows or leads its target"),
        PackageType::Dialogue => return Goal::NotCarriedOut("a dialogue package"),
        PackageType::UseWeapon => return Goal::NotCarriedOut("a use-weapon package"),
        PackageType::Find | PackageType::UseItemAt => return Goal::NotCarriedOut("a package that searches for or uses an object"),
        PackageType::Ambush | PackageType::FleeNotCombat => return Goal::NotCarriedOut("an ambush or flee package"),
        PackageType::Other(_) => return Goal::NotCarriedOut("an unknown package type"),
    }
    let Some(loc) = def.location else { return Goal::Here };
    let radius = loc.radius.max(0) as f32;
    let linked = refs.get(actor).and_then(|r| r.linked);
    if def.ty == PackageType::Patrol {
        let start = match loc.kind {
            LocationKind::NearReference(r) => r,
            LocationKind::NearLinked => linked,
            _ => None,
        };
        if let Some(route) = start.and_then(|s| route_from(game, s, def.patrol_repeatable.unwrap_or(false))) {
            return Goal::Route(route);
        }
    }
    let near = |place: Option<Place>, why: &'static str| match place {
        Some(place) => Goal::Near { place, radius },
        None => Goal::Missing(why),
    };
    match loc.kind {
        LocationKind::NearReference(Some(r)) if r == FormId::PLAYER_REF => near(place_of_ref(game, r), "the location is the player, who is nowhere yet"),
        LocationKind::NearReference(Some(r)) => near(place_of_ref(game, r), "the location's reference is in no cell"),
        LocationKind::NearReference(None) => Goal::Missing("the location's reference is not in the load order"),
        LocationKind::InCell(Some(c)) => match (cell_anchor(nav, refs, c), refs.world_of(c)) {
            (Some(at), None) => Goal::Inside { space: at.space, at },
            // An exterior cell: be near the middle of its navmesh [our choice].
            (Some(at), Some(_)) => Goal::Near { place: at, radius: 2048.0 },
            (None, _) => Goal::Missing("the location's cell has no navmesh"),
        },
        LocationKind::InCell(None) => Goal::Missing("the location's cell is not in the load order"),
        LocationKind::NearCurrent => Goal::Here,
        LocationKind::NearEditor => near(editor_place(refs, actor), "the actor has no editor location"),
        LocationKind::NearLinked => near(linked.and_then(|l| place_of_ref(game, l)), "the actor has no linked reference"),
        LocationKind::ObjectId(_) | LocationKind::ObjectType(_) | LocationKind::AtPackageLocation | LocationKind::Other(_) => {
            Goal::NotCarriedOut("a location given by an object")
        }
    }
}

/// The furniture a package gives an actor that has arrived at `at`, if any:
/// - the package's location, when that reference is furniture [observed: under Travel
///   packages to a chair EasyPeteRef and a settler were sitting, GetSitting 3, in 7,395
///   samples; packages.md §4];
/// - for a Sleep package, the nearest bed of the actor's cell within the location's
///   radius, or anywhere in the cell for an "in cell" location [documented: GECK wiki,
///   Sleep Package];
/// - for an Eat package, the nearest seat within 256 units [documented: "sitting down
///   if furniture is available", Eat Package; the distance is our choice].
///
/// Furniture given to another actor (`taken`: furniture to its user) is left alone.
/// Sandbox and Wander packages choose for themselves (`crate::sandbox`).
pub fn package_furniture(game: &Game, taken: &std::collections::BTreeMap<FormId, FormId>, actor: FormId, def: &PackageDef, at: Place) -> Option<FormId> {
    let refs = game.world.placed;
    let flags = |r: FormId| refs.get(r).and_then(|p| game.data.furniture.get(&p.base)).copied();
    let free = |r: &FormId| taken.get(r).is_none_or(|a| *a == actor);
    let loc = def.location?;
    if let LocationKind::NearReference(Some(r)) = loc.kind {
        if flags(r).is_some() {
            return free(&r).then_some(r);
        }
    }
    let (want, reach) = match (def.ty, loc.kind) {
        (PackageType::Sleep, LocationKind::InCell(_)) => (furniture::SLEEP, f32::INFINITY),
        (PackageType::Sleep, _) => (furniture::SLEEP, (loc.radius as f32).max(256.0)),
        (PackageType::Eat, _) => (furniture::SIT, 256.0),
        _ => return None,
    };
    let cell = cell_of_place(refs, &at)?;
    refs.by_cell
        .get(&cell)?
        .iter()
        .filter(|r| free(r) && flags(**r).is_some_and(|f| f & want != 0))
        .filter_map(|r| {
            let p = refs.get(*r)?;
            let d = (p.pos[0] - at.pos[0]).hypot(p.pos[1] - at.pos[1]);
            (d <= reach && (p.pos[2] - at.pos[2]).abs() < 200.0).then_some((d, *r))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}
