//! Where a package sends its actor: the package's location (PLDT) turned into a place
//! on the navmesh. Which package types go to their location, and what each location
//! kind means: docs/spec-notes/packages.md §3 [documented: GECK wiki, Category:Packages
//! "Standard Location" and the page of each type]; what is not carried out yet:
//! docs/design/actor-behaviour.md §5. A Dialogue package has no place to stay at but
//! someone to talk to ([`Talk`], docs/design/actor-behaviour.md §5.6).

use fnv_game::game::Game;
use fnv_game::package::{furniture, Location, LocationKind, PackageDef, PackageType, TargetKind};
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
    /// Talk to someone: a Dialogue package.
    Talk(Talk),
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

/// What a Dialogue package has its actor do [documented: GECK wiki, Dialogue Package]:
/// go to the wait location, wait there for the target, go up to it and talk.
#[derive(Debug, Clone, PartialEq)]
pub struct Talk {
    /// Whom the actor talks to (the package's target).
    pub target: FormId,
    /// "Activate Distance": how near the target the actor has to be to start talking
    /// (the target's count / distance); [`STANDARD_TALK_DISTANCE`] when the record has 0.
    pub distance: f32,
    /// The "Start/Wait Location" (location 1) as a place to stay at: [`Goal::Near`],
    /// [`Goal::Inside`], or [`Goal::Here`] when the package has none.
    pub wait: Box<Goal>,
    /// The wait location's radius.
    pub wait_radius: f32,
    /// The "Trigger Location" (location 2).
    pub trigger: Trigger,
    /// The topic the conversation starts with, or the one line of a Say To package;
    /// none: the greeting.
    pub topic: Option<FormId>,
    /// "Say To": one line said to the target, no conversation.
    pub say_to: bool,
    /// PKDD "No Headtracking": speaker and target are not made to look at each other.
    pub no_headtracking: bool,
}

/// The activate distance of a Dialogue package that gives none [documented: GECK wiki,
/// Dialogue Package: "The standard distance is 150, as defined in the
/// iActivatePickLength game setting"; hypothesis: that a stored 0 means the standard
/// distance; 54 of FalloutNV.esm's 293 Dialogue packages aimed at the player store 0].
pub const STANDARD_TALK_DISTANCE: f32 = 150.0;

/// Where the target of a Dialogue package has to be for the actor to set off towards it
/// [documented: GECK wiki, Dialogue Package, Trigger Location].
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// The package has no trigger location: the actor goes up to the target at once.
    None,
    /// Within `radius` of a reference, or inside the reference's own volume when it
    /// carries one. [observed: 32 of the 33 trigger locations of FalloutNV.esm that name
    /// a reference with radius 0 name a box primitive, an activator placed for it such
    /// as SunnySmilesTrigger; of the 29 with a radius, 5 do; packages.md §2.
    /// hypothesis: that the game tests the volume; the documentation only speaks of
    /// the radius.]
    NearReference { reference: FormId, radius: f32 },
    /// Within `radius` of a fixed place (the actor's editor location).
    Near { place: Place, radius: f32 },
    /// Within `radius` of where the actor is now.
    NearActor { radius: f32 },
    /// In this cell.
    InCell(FormId),
    /// A trigger location that names nothing in the world, or is of a kind not
    /// understood: the target is never in it.
    Never,
}

/// How much of an actor is tested against a trigger location's volume: its feet, its
/// middle and its head, this far apart [our choice; the game tests the actor's
/// collision object against trigger volumes, docs/spec-notes/level-design.md §11.2].
const BODY_STEP: f32 = 64.0;

/// Whether a point is inside the volume a reference carries (REFR XPRM), where the
/// reference is now.
fn in_volume(game: &Game, reference: FormId, p: [f32; 3]) -> bool {
    let Some(placed) = game.world.placed.get(reference) else { return false };
    let Some(prim) = placed.primitive else { return false };
    let (pos, rot) = match game.world.peek(reference) {
        Some(s) => (s.pos, s.rot),
        None => (placed.pos, placed.rot),
    };
    // world = pos + R·local·scale, R row-major (`fnv_world::transform::ref_rotation`).
    let r = fnv_world::transform::ref_rotation(rot);
    let scale = placed.scale.filter(|s| *s > 0.0).unwrap_or(1.0);
    let d = [p[0] - pos[0], p[1] - pos[1], p[2] - pos[2]];
    let local = [0, 1, 2].map(|i| (r[i] * d[0] + r[3 + i] * d[1] + r[6 + i] * d[2]) / scale);
    if prim.sphere {
        local.iter().map(|v| v * v).sum::<f32>() <= prim.bounds[0] * prim.bounds[0]
    } else {
        (0..3).all(|i| local[i].abs() <= prim.bounds[i])
    }
}

impl Trigger {
    /// Whether the target, standing at `target`, is in the trigger location of an
    /// actor standing at `feet`. Without a trigger location this always holds.
    pub fn holds(&self, game: &Game, feet: Place, target: Place) -> bool {
        let near = |place: Place, radius: f32| place.space == target.space && radius > 0.0 && dist(place.pos, target.pos) <= radius;
        match self {
            Trigger::None => true,
            Trigger::NearReference { reference, radius } => {
                let Some(place) = place_of_ref(game, *reference) else { return false };
                let body = |i: usize| [target.pos[0], target.pos[1], target.pos[2] + BODY_STEP * i as f32];
                near(place, *radius) || place.space == target.space && (0..3).any(|i| in_volume(game, *reference, body(i)))
            }
            Trigger::Near { place, radius } => near(*place, *radius),
            Trigger::NearActor { radius } => near(feet, *radius),
            Trigger::InCell(cell) => cell_of_place(game.world.placed, &target) == Some(*cell),
            Trigger::Never => false,
        }
    }
}

/// Distance between two points of one space.
pub fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
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
        PackageType::Dialogue => return talk(game, nav, actor, def),
        PackageType::UseWeapon => return Goal::NotCarriedOut("a use-weapon package"),
        PackageType::Find | PackageType::UseItemAt => return Goal::NotCarriedOut("a package that searches for or uses an object"),
        PackageType::Ambush | PackageType::FleeNotCombat => return Goal::NotCarriedOut("an ambush or flee package"),
        PackageType::Other(_) => return Goal::NotCarriedOut("an unknown package type"),
    }
    let Some(loc) = def.location else { return Goal::Here };
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
    stay_at(game, nav, actor, &loc)
}

/// A location as a place to stay at ([`Goal::Here`], [`Goal::Near`], [`Goal::Inside`]),
/// or why it is none.
fn stay_at(game: &Game, nav: &NavGraph, actor: FormId, loc: &Location) -> Goal {
    let refs = game.world.placed;
    let radius = loc.radius.max(0) as f32;
    let linked = refs.get(actor).and_then(|r| r.linked);
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

/// The goal of a Dialogue package [documented: GECK wiki, Dialogue Package].
///
/// Carried out: a conversation with the player (the actor "will force-greet the
/// player") and a Say To line to anyone. A conversation between two actors is not
/// (there are no actor-to-actor conversations yet).
fn talk(game: &Game, nav: &NavGraph, actor: FormId, def: &PackageDef) -> Goal {
    let refs = game.world.placed;
    let linked = refs.get(actor).and_then(|r| r.linked);
    let target = match def.target.map(|t| t.kind) {
        Some(TargetKind::Reference(Some(r))) => r,
        Some(TargetKind::LinkedReference) => match linked {
            Some(l) => l,
            None => return Goal::Missing("the actor has no linked reference to talk to"),
        },
        Some(TargetKind::Reference(None)) => return Goal::Missing("the reference to talk to is not in the load order"),
        _ => return Goal::NotCarriedOut("a dialogue package without a reference to talk to"),
    };
    let data = def.dialogue.unwrap_or_default();
    if target != FormId::PLAYER_REF && !data.say_to {
        return Goal::NotCarriedOut("a conversation between two actors");
    }
    if data.say_to && data.topic.is_none() {
        return Goal::NotCarriedOut("a Say To package without a topic");
    }
    // A wait location that cannot be gone to leaves the actor where it is [our choice].
    let wait = match def.location.map(|l| stay_at(game, nav, actor, &l)) {
        Some(g @ (Goal::Near { .. } | Goal::Inside { .. })) => g,
        _ => Goal::Here,
    };
    let trigger = match def.location2 {
        None => Trigger::None,
        Some(l) => {
            let radius = l.radius.max(0) as f32;
            match l.kind {
                LocationKind::NearReference(Some(reference)) => Trigger::NearReference { reference, radius },
                LocationKind::NearEditor => editor_place(refs, actor).map_or(Trigger::Never, |place| Trigger::Near { place, radius }),
                LocationKind::NearLinked => linked.map_or(Trigger::Never, |reference| Trigger::NearReference { reference, radius }),
                LocationKind::NearCurrent => Trigger::NearActor { radius },
                LocationKind::InCell(Some(c)) => Trigger::InCell(c),
                _ => Trigger::Never,
            }
        }
    };
    let distance = def.target.map_or(0, |t| t.count_distance);
    Goal::Talk(Talk {
        target,
        distance: if distance > 0 { distance as f32 } else { STANDARD_TALK_DISTANCE },
        wait: Box::new(wait),
        wait_radius: def.location.map_or(0.0, |l| l.radius.max(0) as f32),
        trigger,
        topic: data.topic,
        say_to: data.say_to,
        no_headtracking: data.no_headtracking,
    })
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
