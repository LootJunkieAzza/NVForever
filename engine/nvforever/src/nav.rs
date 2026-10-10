//! NPCs travelling on the navmesh (docs/design/navmesh.md §5): `--npc-travel
//! ACTOR=TARGET` plans a path over the load order's nav graph (`fnv-nav`) and walks the
//! actor along it, through doors into other spaces; its locomotion (`locomotion.rs`)
//! picks and blends the walk, turn and idle clips from the velocity and turn rate set
//! here. `--follow-npc` puts the free camera behind the
//! first traveller (streaming and door travel follow it); `--show-navmesh` draws the
//! navmesh near the camera and the travellers' paths.
//!
//! Travellers are moved on the navmesh surface by `fnv_nav::Walker` (no collision with
//! the world); walking the same paths with the physics character controller is tested
//! headlessly by `fnvtool nav walk`. At a door that opens in place, a traveller stops,
//! opens it (`nvf_play::doors`, which the drawn door and its collision follow) and waits
//! until its "Open" sequence has ended (docs/design/navmesh.md §6.1); a door that is not
//! drawn (its cell not loaded) has nothing to wait for. A trip here is a fixed A to B;
//! actors that live by their AI packages are `residents.rs`, which shares the drawing
//! (`show_actor`) and the graph.

use crate::actors::ActorWorld;
use crate::locomotion::Locomotion;
use crate::world::{Streamer, WorldData, WorldRoot};
use bevy::prelude::*;
use fnv_nav::graph::{EdgeKind, NONE};
use fnv_nav::{NavGraph, NavRecords, PathParams, Place, Space, Steer, WalkEvent, Walker};
use fnv_sim::RefTable;
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_play::RefVisual;

/// Walking speed of travellers, game units per second: the player's observed walking
/// speed [observed: physics.md §6.3]; NPC speeds are not captured [hypothesis].
pub const WALK_SPEED: f32 = 86.8;

/// One NPC on its way.
pub struct Traveller {
    pub reference: u32,
    pub base: u32,
    /// The reference's scale (XSCL).
    pub scale: f32,
    pub label: String,
    pub target_label: String,
    pub walker: Walker,
    /// Drawn while its space is the one drawn: spawned (`true` once asked), and the
    /// actor's root entity once found.
    spawned: bool,
    root: Option<Entity>,
    pub started: Option<f32>,
    pub arrived: Option<f32>,
    pub failed: Option<String>,
    next_report: f32,
    /// The door it is waiting at and since when (elapsed seconds).
    waiting: Option<(u32, f32)>,
    /// Doors it opened and how long it waited at each (seconds).
    pub doors_opened: Vec<(u32, f32)>,
}

/// The actor `--follow-npc` follows: the first traveller, else the first resident
/// (`residents.rs`).
#[derive(Clone, Copy)]
pub struct Lead {
    pub space: Space,
    pub feet: [f32; 3],
    pub heading: f32,
}

#[derive(Resource)]
pub struct NavState {
    pub graph: NavGraph,
    pub travellers: Vec<Traveller>,
    pub show: bool,
    pub follow: bool,
    pub lead: Option<Lead>,
    /// Smoothed chase-camera position, game space.
    cam: Option<Vec3>,
    /// The camera follows the first traveller into another space: travel asked for.
    travel_asked: bool,
    /// What building found, logged once the app's logger runs ([`log_startup`]).
    startup: Vec<(bool, String)>,
}

pub(crate) fn space_of_cell(world: &WorldData, cell: u32) -> Option<Space> {
    let c = world.index.cells.get(&cell)?;
    Some(match c.world {
        Some(w) => Space::World(w),
        None => Space::Interior(c.form_id),
    })
}

pub(crate) fn space_name(world: &WorldData, s: Space) -> String {
    match s {
        Space::World(w) => world.index.worldspaces.get(&w).and_then(|w| w.edid.clone()).unwrap_or_else(|| format!("{w:08X}")),
        Space::Interior(c) => world.index.cells.get(&c).and_then(|c| c.edid.clone()).unwrap_or_else(|| format!("{c:08X}")),
    }
}

/// A place: a placed reference (Editor ID or hex FormID), or "x,y,z" in `space`.
fn resolve(world: &WorldData, refs: &RefTable, spec: &str, space: Space) -> Result<(Place, Option<u32>, String), String> {
    let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
    if let [x, y, z] = parts[..] {
        if let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) {
            return Ok((Place { space, pos: [x, y, z] }, None, format!("({x:.0}, {y:.0}, {z:.0})")));
        }
    }
    let id = match refs.by_edid(spec) {
        Some(id) => id,
        None => spec.trim_start_matches("0x").parse::<fnv_sim::FormId>().map_err(|_| format!("{spec}: not x,y,z, a reference Editor ID or a FormID"))?,
    };
    let r = refs.get(id).ok_or_else(|| format!("{spec}: no placed reference {id}"))?;
    let cell = r.cell.ok_or_else(|| format!("{spec}: no cell"))?;
    let space = space_of_cell(world, cell.0).ok_or_else(|| format!("{spec}: cell {cell} not indexed"))?;
    Ok((Place { space, pos: r.pos }, Some(id.0), r.edid.clone().unwrap_or_else(|| format!("{id}"))))
}

impl NavState {
    /// Build the nav graph and plan each `ACTOR=TARGET` trip.
    pub fn build(world: &WorldData, trips: &[String], speed: f32, show: bool, follow: bool) -> Result<NavState, String> {
        let mut startup: Vec<(bool, String)> = Vec::new();
        let t0 = std::time::Instant::now();
        let records = NavRecords::read(world.lo);
        let graph = NavGraph::build(world.lo, world.schema, world.index, &records);
        let s = &graph.stats;
        startup.push((true, format!(
            "nav: graph of {} navmeshes, {} triangles, {} portal and {} ledge links ({} unresolved), {} doors ({} unresolved), built in {:.2}s",
            s.navmeshes,
            s.triangles,
            s.portal_edges,
            s.ledge_edges,
            s.unresolved_links,
            s.doors,
            s.doors_unresolved,
            t0.elapsed().as_secs_f64()
        )));
        let params = PathParams::default();
        let mut travellers = Vec::new();
        if !trips.is_empty() {
            let refs = RefTable::from_index(world.lo, world.schema, world.index);
            let here = Space::World(world.world);
            for trip in trips {
                let (who, to) = trip.split_once('=').ok_or_else(|| format!("--npc-travel {trip}: expected ACTOR=TARGET"))?;
                let (from, Some(reference), label) = resolve(world, &refs, who.trim(), here)? else {
                    return Err(format!("--npc-travel {trip}: {who} is not a placed reference"));
                };
                let base = refs.get(fnv_sim::FormId(reference)).map(|r| r.base.0).unwrap_or(0);
                let (target, _, target_label) = resolve(world, &refs, to.trim(), from.space)?;
                let heading = refs.get(fnv_sim::FormId(reference)).map_or(0.0, |r| r.rot[2]);
                let mut walker = Walker::new(from.space, from.pos, heading, speed);
                let t = std::time::Instant::now();
                let mut failed = None;
                match walker.go_to(&graph, target, &params) {
                    Ok(p) => {
                        startup.push((true, format!(
                            "nav: {label} ({reference:08X}) -> {target_label}: path of {:.0} units in {} legs ({}), {} triangles searched in {:.1} ms; start snapped {:.0}, goal snapped {:.0}",
                            p.length,
                            p.legs.len(),
                            p.legs.iter().map(|l| space_name(world, l.space)).collect::<Vec<_>>().join(" -> "),
                            p.expanded,
                            t.elapsed().as_secs_f64() * 1e3,
                            p.start_snap,
                            p.goal_snap
                        )));
                        // Start on the navmesh (where the path begins).
                        if let Some(p0) = p.legs.first().and_then(|l| l.points.first()) {
                            walker.feet = *p0;
                        }
                    }
                    Err(e) => {
                        startup.push((false, format!("nav: {label} ({reference:08X}) -> {target_label}: {e}")));
                        failed = Some(e.to_string());
                    }
                }
                // Arrive next to an actor rather than inside it.
                if let Some(f) = walker.follower.as_mut() {
                    let target_actor = matches!(to.trim(), s if refs.by_edid(s).and_then(|id| refs.get(id)).is_some_and(|r| matches!(&r.sig, b"ACHR" | b"ACRE")));
                    f.goal_radius = if target_actor { 72.0 } else { 8.0 };
                }
                travellers.push(Traveller {
                    reference,
                    base,
                    scale: refs.get(fnv_sim::FormId(reference)).and_then(|r| r.scale).unwrap_or(1.0),
                    label,
                    target_label,
                    walker,
                    spawned: false,
                    root: None,
                    started: None,
                    arrived: None,
                    failed,
                    next_report: 5.0, waiting: None, doors_opened: Vec::new(),
                });
            }
        }
        Ok(NavState { graph, travellers, show, follow, lead: None, cam: None, travel_asked: false, startup })
    }
}

/// The compass heading a root transform faces (its rotation about z, clockwise).
fn tr_heading(t: &Transform) -> f32 {
    let f = t.rotation * Vec3::Y;
    f.x.atan2(f.y)
}

/// Game space to the Bevy space the camera lives in (the world root's rotation).
fn to_bevy(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, -p.y)
}

/// The space drawn now.
pub(crate) fn drawn_space(world: &WorldData, streamer: &Streamer) -> Space {
    match streamer.interior {
        Some(c) => Space::Interior(c),
        None => Space::World(world.world),
    }
}

/// Log what building the graph and planning the trips found (built before the app, and
/// so before its logger).
pub fn log_startup(mut nav: ResMut<NavState>) {
    for (ok, line) in std::mem::take(&mut nav.startup) {
        if ok {
            info!("{line}");
        } else {
            warn!("{line}");
        }
    }
}

/// Move the travellers, keep their visuals in step, report progress.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    time: Res<Time>,
    mut nav: ResMut<NavState>,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    actor_world: Option<Res<ActorWorld>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    roots: Query<(Entity, &RefVisual, &ChildOf)>,
    mut actors: Query<(&mut Transform, Option<&mut Locomotion>)>,
    mut travel: MessageWriter<nvf_play::Travel>,
    mut doors: ResMut<nvf_play::doors::Doors>,
    held: Option<Res<nvf_play::dialogue::WorldHeld>>,
) {
    let Ok(root) = root.single() else { return };
    // A conversation with the player holds the travellers where they are (they blend to
    // their idle: no velocity below); docs/design/dialogue.md, Pausing.
    let held = held.is_some_and(|h| h.0);
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let drawn = drawn_space(&world, &streamer);
    let nav = &mut *nav;
    if let Some(first) = nav.travellers.first() {
        nav.lead = Some(Lead { space: first.walker.space, feet: first.walker.feet, heading: first.walker.heading });
    }
    // Following: draw the space the lead is in (it may start indoors, or have gone
    // through a door).
    if let Some(lead) = nav.lead.filter(|_| nav.follow) {
        if lead.space == drawn {
            nav.travel_asked = false;
        } else if !nav.travel_asked {
            let (w, interior) = match lead.space {
                Space::World(w) => (Some(w), None),
                Space::Interior(c) => (None, Some(c)),
            };
            travel.write(nvf_play::Travel { world: w, interior, feet: lead.feet, heading_deg: lead.heading.to_degrees() });
            nav.travel_asked = true;
            nav.cam = None;
        }
    }
    for i in 0..nav.travellers.len() {
        if nav.travellers[i].failed.is_some() {
            continue;
        }
        let ev = {
            let tr = &mut nav.travellers[i];
            // Start walking once the actor could be seen (its cells are loading).
            if tr.started.is_none() {
                if !streamer.settled && tr.walker.space == drawn {
                    continue;
                }
                tr.started = Some(now);
            }
            // A door known here (drawn, with its animation) is opened and waited for.
            let mut open = |door: u32| {
                if doors.get(door).is_none() {
                    return true;
                }
                doors.set_open(door, true);
                doors.is_open(door) == Some(true)
            };
            if held {
                WalkEvent::Idle
            } else {
                tr.walker.update_with(&nav.graph, dt, &mut open)
            }
        };
        let tr = &mut nav.travellers[i];
        let elapsed = now - tr.started.unwrap_or(now);
        match (ev, tr.waiting) {
            (WalkEvent::WaitingForDoor(d), w) if w.is_none_or(|w| w.0 != d) => {
                info!("nav: {} opens door {d:08X} and waits", tr.label);
                tr.waiting = Some((d, elapsed));
            }
            (WalkEvent::WaitingForDoor(_), _) => {}
            (_, Some((d, since))) => {
                info!("nav: {} walks through door {d:08X} after waiting {:.2}s", tr.label, elapsed - since);
                tr.doors_opened.push((d, elapsed - since));
                tr.waiting = None;
            }
            _ => {}
        }
        match ev {
            WalkEvent::UsedDoor(d) => {
                info!("nav: {} used door {:08X} into {} after {elapsed:.1}s", tr.label, d.door, space_name(&world, tr.walker.space));
            }
            WalkEvent::Arrived => {
                tr.arrived = Some(elapsed);
                info!(
                    "nav: {} arrived at {} after {elapsed:.1}s, walked {:.0} units, at ({:.0}, {:.0}, {:.0}) in {}",
                    tr.label,
                    tr.target_label,
                    tr.walker.travelled,
                    tr.walker.feet[0],
                    tr.walker.feet[1],
                    tr.walker.feet[2],
                    space_name(&world, tr.walker.space)
                );
            }
            WalkEvent::Moved if elapsed >= tr.next_report => {
                tr.next_report += 5.0;
                let left = tr.walker.follower.as_mut().map_or(0.0, |f| match f.steer(tr.walker.feet) {
                    Steer::Walk { remaining, .. } => remaining,
                    _ => 0.0,
                });
                info!(
                    "nav: {} at ({:.0}, {:.0}, {:.0}) in {}, {:.0} units walked, {left:.0} left on this leg",
                    tr.label,
                    tr.walker.feet[0],
                    tr.walker.feet[1],
                    tr.walker.feet[2],
                    space_name(&world, tr.walker.space),
                    tr.walker.travelled
                );
            }
            _ => {}
        }
        // The visual: drawn while the traveller is in the space drawn.
        let visible = tr.walker.space == drawn;
        let moving = ev == WalkEvent::Moved;
        let stage = (&mut tr.spawned, &mut tr.root);
        if let Err(e) = show_actor(&mut commands, root, visible, actor_world.as_deref(), &mut parts, &mut models, &roots, &mut actors, Who { reference: tr.reference, base: tr.base, creature: false, scale: tr.scale }, stage, &tr.walker, moving, dt) {
            warn!("nav: {} could not be drawn", tr.label);
            tr.failed = Some(e.into());
        }
    }
}

/// The actor a walker moves: its reference, its base, whether the base is a creature,
/// and the reference's scale (XSCL).
#[derive(Clone, Copy)]
pub(crate) struct Who {
    pub reference: u32,
    pub base: u32,
    pub creature: bool,
    pub scale: f32,
}

/// Keep the visual of an actor that a walker moves in step with it: drawn while
/// `visible` (its space is the one drawn), spawned on demand (`spawned`, and `root` once
/// its entity is found), placed at the walker's feet and heading, its locomotion fed the
/// speed and turn rate. An error: the actor cannot be drawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show_actor(
    commands: &mut Commands,
    world_root: Entity,
    visible: bool,
    actor_world: Option<&ActorWorld>,
    parts: &mut ActorParts,
    models: &mut ModelCtx,
    roots: &Query<(Entity, &RefVisual, &ChildOf)>,
    actors: &mut Query<(&mut Transform, Option<&mut Locomotion>)>,
    who: Who,
    (spawned, root): (&mut bool, &mut Option<Entity>),
    walker: &Walker,
    moving: bool,
    dt: f32,
) -> Result<(), &'static str> {
    let Who { reference, base, creature, scale } = who;
    if !visible {
        if let Some(e) = root.take() {
            commands.entity(e).despawn();
        }
        *spawned = false;
        return Ok(());
    }
    let Some(aw) = actor_world else { return Ok(()) };
    if !*spawned {
        let at = Vec3::from_array(walker.feet);
        let rt = bevy::math::Affine3A::from_scale_rotation_translation(Vec3::splat(scale), Quat::from_rotation_z(-walker.heading), at);
        let mut ctx = ActorCtx { cache: parts, models };
        let mut stats = Default::default();
        let drawn = if creature {
            crate::actors::spawn_creature(commands, world_root, rt, reference, base, aw, &mut ctx, &mut stats, false)
        } else {
            crate::actors::spawn(commands, world_root, rt, at, reference, base, aw, &mut ctx, &mut stats, false)
        };
        if !drawn {
            return Err("not drawn");
        }
        *spawned = true;
    }
    // The actor's root: the entity of its reference placed directly under the world
    // root (spawned last frame; its parts sit below it).
    if root.is_none() {
        *root = roots.iter().find(|(_, v, p)| v.0 == reference && p.parent() == world_root).map(|(e, _, _)| e);
    }
    let Some(e) = *root else { return Ok(()) };
    let Ok((mut t, lo)) = actors.get_mut(e) else { return Ok(()) };
    let before = (t.translation, tr_heading(&t));
    t.translation = Vec3::from_array(walker.feet);
    t.rotation = Quat::from_rotation_z(-walker.heading);
    // Locomotion takes the velocity in the actor's axes and own scale (y forward)
    // and the turn rate (positive = left, counter-clockwise; the compass heading
    // turns the other way).
    if let Some(mut lo) = lo {
        let scale = t.scale.x.max(1e-3);
        let moved = before.0.truncate().distance(t.translation.truncate());
        let speed = if dt > 0.0 && moving { moved / dt } else { 0.0 };
        lo.velocity = [0.0, speed / scale];
        let d = (walker.heading - before.1 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        lo.turn = if dt > 0.0 { -d / dt } else { 0.0 };
    }
    Ok(())
}

/// `--follow-npc`: the camera behind the first traveller, a little above, looking at it.
pub fn follow_camera(time: Res<Time>, mut nav: ResMut<NavState>, mut cams: Query<(&mut Transform, &mut crate::fly::FlyCam)>) {
    if !nav.follow {
        return;
    }
    let Some(lead) = nav.lead else { return };
    let Ok((mut t, mut fly)) = cams.single_mut() else { return };
    let feet = Vec3::from_array(lead.feet);
    let (s, c) = lead.heading.sin_cos();
    let want = feet - Vec3::new(s, c, 0.0) * 280.0 + Vec3::Z * 160.0;
    let k = 1.0 - (-4.0 * time.delta_secs()).exp();
    let cam = match nav.cam {
        Some(p) => p.lerp(want, k),
        None => want,
    };
    nav.cam = Some(cam);
    let look = feet + Vec3::Z * 90.0;
    *t = Transform::from_translation(to_bevy(cam)).looking_at(to_bevy(look), Vec3::Y);
    // Keep the free camera's angles in step, for when following ends.
    let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
    fly.yaw = yaw;
    fly.pitch = pitch;
}

/// `--show-navmesh`: the navmesh within 3,000 units of the camera (outline red, seams
/// cyan, ledges orange, preferred triangles green, water blue, door triangles yellow)
/// and each traveller's remaining path (magenta).
pub fn draw(
    nav: Res<NavState>,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut gizmos: Gizmos,
) {
    use fnv_nav::record::tri_flags;
    if !nav.show {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let space = drawn_space(&world, &streamer);
    let at = crate::world::bevy_to_game(cam.translation());
    let g = &nav.graph;
    let lift = Vec3::Z * 3.0;
    let p = |q: [f32; 3]| to_bevy(Vec3::from_array(q) + lift);
    for t in g.triangles_near(space, at.x, at.y, 3000.0) {
        let tri = &g.tris[t as usize];
        let c = g.corners(t);
        let fill = if tri.has(tri_flags::DOOR) {
            Some(Color::srgb(1.0, 0.9, 0.1))
        } else if tri.has(tri_flags::WATER) {
            Some(Color::srgb(0.2, 0.4, 1.0))
        } else if tri.has(tri_flags::PREFERRED) {
            Some(Color::srgb(0.2, 0.9, 0.3))
        } else {
            None
        };
        for e in 0..3 {
            let color = match tri.kind[e] {
                EdgeKind::Boundary => Color::srgb(1.0, 0.15, 0.1),
                EdgeKind::Portal => Color::srgb(0.1, 0.9, 1.0),
                EdgeKind::LedgeUp | EdgeKind::LedgeDown => Color::srgb(1.0, 0.55, 0.0),
                EdgeKind::Internal => fill.unwrap_or(Color::srgba(0.8, 0.8, 0.8, 0.35)),
            };
            // Shared internal edges are drawn once (from the lower triangle index).
            if tri.kind[e] == EdgeKind::Internal && tri.next[e] != NONE && tri.next[e] < t && fill.is_none() {
                continue;
            }
            gizmos.line(p(c[e]), p(c[(e + 1) % 3]), color);
        }
    }
    for tr in &nav.travellers {
        let Some(f) = &tr.walker.follower else { continue };
        let Some(leg) = f.path.legs.get(f.leg).filter(|l| l.space == space) else { continue };
        let mut prev = tr.walker.feet;
        for q in &leg.points[f.next.min(leg.points.len())..] {
            gizmos.line(p(prev), p(*q), Color::srgb(1.0, 0.2, 1.0));
            prev = *q;
        }
        let (s, c) = tr.walker.heading.sin_cos();
        let tip = [tr.walker.feet[0] + 40.0 * s, tr.walker.feet[1] + 40.0 * c, tr.walker.feet[2]];
        gizmos.line(p(tr.walker.feet), p(tip), Color::WHITE);
    }
}

/// The travellers' outcome when the app exits (smoke tests read it).
pub fn report(mut exit: MessageReader<AppExit>, nav: Res<NavState>) {
    if exit.read().next().is_none() {
        return;
    }
    let arrived = nav.travellers.iter().filter(|t| t.arrived.is_some()).count();
    info!(
        "nav summary: {arrived} of {} travellers arrived; {}",
        nav.travellers.len(),
        nav.travellers
            .iter()
            .map(|t| match (&t.failed, t.arrived) {
                (Some(e), _) => format!("{}: failed ({e})", t.label),
                (None, Some(s)) if t.doors_opened.is_empty() => format!("{}: arrived after {s:.1}s", t.label),
                (None, Some(s)) => format!(
                    "{}: arrived after {s:.1}s, opened {}",
                    t.label,
                    t.doors_opened.iter().map(|(d, w)| format!("door {d:08X} (waited {w:.2}s)")).collect::<Vec<_>>().join(", ")
                ),
                (None, None) => format!("{}: on the way, {:.0} units walked", t.label, t.walker.travelled),
            })
            .collect::<Vec<_>>()
            .join("; ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_of_a_root_rotation() {
        for h in [0.0f32, 0.5, 1.6, -2.0, 3.0] {
            let t = Transform::from_rotation(Quat::from_rotation_z(-h));
            assert!((tr_heading(&t) - h).abs() < 1e-5, "{h}");
        }
    }
}
