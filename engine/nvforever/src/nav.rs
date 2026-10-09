//! NPCs travelling on the navmesh (docs/design/navmesh.md §5): `--npc-travel
//! ACTOR=TARGET` plans a path over the load order's nav graph (`fnv-nav`) and walks the
//! actor along it, through doors into other spaces, playing the walk cycle while it
//! moves and its idle when it stands. `--follow-npc` puts the free camera behind the
//! first traveller (streaming and door travel follow it); `--show-navmesh` draws the
//! navmesh near the camera and the travellers' paths.
//!
//! Travellers are moved on the navmesh surface by `fnv_nav::Walker` (no collision with
//! the world); walking the same paths with the physics character controller is tested
//! headlessly by `fnvtool nav walk`. Behaviour (what an actor does on arrival, speeds) is
//! an engine choice until AI packages exist (Layer 6).

use crate::actors::{idle_clip, ActorWorld, Spawned};
use crate::world::{Streamer, WorldData, WorldRoot};
use bevy::prelude::*;
use fnv_nav::graph::{EdgeKind, NONE};
use fnv_nav::{NavGraph, NavRecords, PathParams, Place, Space, Steer, WalkEvent, Walker};
use fnv_sim::RefTable;
use nvf_assets::actor::{play_on_actor, ActorCtx, ActorParts};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;
use std::sync::Arc;

/// Walking speed of travellers, game units per second: the player's observed walking
/// speed [observed: physics.md §6.3]; NPC speeds are not captured [hypothesis].
pub const WALK_SPEED: f32 = 86.8;

/// One NPC on its way.
pub struct Traveller {
    pub reference: u32,
    pub base: u32,
    pub label: String,
    pub target_label: String,
    pub walker: Walker,
    /// Drawn: the actor's entities and skeleton, while its space is the one drawn.
    visual: Option<Spawned>,
    /// The clip playing: walking or standing.
    walking: Option<bool>,
    pub started: Option<f32>,
    pub arrived: Option<f32>,
    pub failed: Option<String>,
    next_report: f32,
}

/// Prepared walk clips and their stride speeds, by path (`None`: not found).
type WalkClips = HashMap<String, Option<(Arc<Clip>, Option<f32>)>>;

#[derive(Resource)]
pub struct NavState {
    pub graph: NavGraph,
    pub travellers: Vec<Traveller>,
    pub show: bool,
    pub follow: bool,
    /// Walk clips with their accumulated motion taken out, and the speed (units per
    /// second) the clip's stride covers at its own frequency, by path.
    clips: WalkClips,
    /// Smoothed chase-camera position, game space.
    cam: Option<Vec3>,
    /// The camera follows the first traveller into another space: travel asked for.
    travel_asked: bool,
    /// What building found, logged once the app's logger runs ([`log_startup`]).
    startup: Vec<(bool, String)>,
}

fn space_of_cell(world: &WorldData, cell: u32) -> Option<Space> {
    let c = world.index.cells.get(&cell)?;
    Some(match c.world {
        Some(w) => Space::World(w),
        None => Space::Interior(c.form_id),
    })
}

fn space_name(world: &WorldData, s: Space) -> String {
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
                    label,
                    target_label,
                    walker,
                    visual: None,
                    walking: None,
                    started: None,
                    arrived: None,
                    failed,
                    next_report: 5.0,
                });
            }
        }
        Ok(NavState { graph, travellers, show, follow, clips: HashMap::new(), cam: None, travel_asked: false, startup })
    }
}

/// The walk clip beside `skeleton` for the actor's sex, prepared to play in place
/// ([`in_place`]), cached by path in `clips`.
fn walk_clip(clips: &mut WalkClips, ctx: &mut ActorCtx, skeleton: &str, female: bool) -> Option<(Arc<Clip>, Option<f32>)> {
    let path = walk_clip_path(skeleton, female);
    if let Some(c) = clips.get(&path) {
        return c.clone();
    }
    let c = ctx.clip(&path).map(|c| {
        let (clip, speed) = in_place(&c);
        match speed {
            Some(v) => info!("nav: walk clip {path}: stride covers {v:.1} units/s at its own rate"),
            None => info!("nav: walk clip {path}: no accumulated motion; played at its own rate"),
        }
        (Arc::new(clip), speed)
    });
    if c.is_none() {
        warn!("nav: walk clip {path} not found");
    }
    clips.insert(path, c.clone());
    c
}

/// `locomotion\male\mtforward.kf` (or `female\`) in the skeleton's folder [observed:
/// `characters\_male\locomotion\male\mtforward.kf` and `...\female\mtforward.kf` in
/// `Fallout - Meshes.bsa`; that the game picks them by the actor's sex is [hypothesis]].
pub fn walk_clip_path(skeleton: &str, female: bool) -> String {
    let idle = idle_clip(skeleton);
    let dir = idle.strip_suffix("mtidle.kf").unwrap_or(&idle);
    format!("{dir}{}\\mtforward.kf", if female { "female" } else { "male" })
}

/// A copy of `clip` whose accumulation root does not travel: the drift of its
/// translation from the clip's start to its end is taken out, keeping the bob and sway.
/// Returns the speed the drift covers at the clip's own frequency (`None` when the root
/// has no translation keys) [engine choice: the walker moves the actor; the clip only
/// animates it in place].
pub fn in_place(clip: &Clip) -> (Clip, Option<f32>) {
    let mut out = clip.clone();
    let span = clip.stop - clip.start;
    let Some(root) = clip.accum_root.as_deref() else { return (out, None) };
    let Some((_, track)) = out.tracks.iter_mut().find(|(n, _)| n == root) else { return (out, None) };
    let fnv_nif::anim::Channel::Keys(keys) = &track.translation else { return (out, None) };
    let (Some(a), Some(b)) = (track.translation.sample(clip.start), track.translation.sample(clip.stop)) else { return (out, None) };
    if span <= 0.0 || keys.keys.len() < 2 {
        return (out, None);
    }
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let mut keys = keys.clone();
    for k in &mut keys.keys {
        let s = (k.time - clip.start) / span;
        for (v, d) in k.value.iter_mut().zip(d) {
            *v -= d * s;
        }
    }
    let mut t = (**track).clone();
    t.translation = fnv_nif::anim::Channel::Keys(keys);
    *track = Arc::new(t);
    let speed = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / span * clip.frequency;
    (out, (speed > 1.0).then_some(speed))
}

/// Game space to the Bevy space the camera lives in (the world root's rotation).
fn to_bevy(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, -p.y)
}

/// The space drawn now.
fn drawn_space(world: &WorldData, streamer: &Streamer) -> Space {
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
    mut transforms: Query<&mut Transform>,
    mut travel: MessageWriter<nvf_play::Travel>,
) {
    let Ok(root) = root.single() else { return };
    let dt = time.delta_secs().min(0.1);
    let now = time.elapsed_secs();
    let drawn = drawn_space(&world, &streamer);
    let nav = &mut *nav;
    // Following: draw the space the first traveller is in (it may start indoors, or
    // have gone through a door).
    if let Some(first) = nav.travellers.first().filter(|_| nav.follow) {
        let space = first.walker.space;
        if space == drawn {
            nav.travel_asked = false;
        } else if !nav.travel_asked {
            let (w, interior) = match space {
                Space::World(w) => (Some(w), None),
                Space::Interior(c) => (None, Some(c)),
            };
            travel.write(nvf_play::Travel { world: w, interior, feet: first.walker.feet, heading_deg: first.walker.heading.to_degrees() });
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
            tr.walker.update(&nav.graph, dt)
        };
        let tr = &mut nav.travellers[i];
        let elapsed = now - tr.started.unwrap_or(now);
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
        let moving = tr.walker.follower.is_some() && ev == WalkEvent::Moved;
        // The visual: drawn while the traveller is in the space drawn.
        let visible = tr.walker.space == drawn;
        if !visible {
            if let Some(v) = tr.visual.take() {
                commands.entity(v.actor.root).despawn();
            }
            continue;
        }
        let Some(aw) = actor_world.as_deref() else { continue };
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        if tr.visual.is_none() {
            let rt = bevy::math::Affine3A::from_rotation_translation(Quat::from_rotation_z(-tr.walker.heading), Vec3::from_array(tr.walker.feet));
            let mut stats = Default::default();
            tr.visual = crate::actors::spawn(&mut commands, root, rt, tr.reference, tr.base, aw, &mut ctx, &mut stats);
            tr.walking = None;
            if tr.visual.is_none() {
                warn!("nav: {} could not be drawn", tr.label);
                tr.failed = Some("not drawn".into());
                continue;
            }
        }
        let v = tr.visual.as_ref().expect("spawned");
        if let Ok(mut t) = transforms.get_mut(v.actor.root) {
            t.translation = Vec3::from_array(tr.walker.feet);
            t.rotation = Quat::from_rotation_z(-tr.walker.heading);
        }
        if tr.walking != Some(moving) {
            tr.walking = Some(moving);
            let (skeleton, female) = (v.appearance.skeleton.clone(), v.appearance.female);
            let clip = if moving {
                let speed = tr.walker.speed;
                walk_clip(&mut nav.clips, &mut ctx, &skeleton, female).map(|(c, natural)| {
                    let mut c = (*c).clone();
                    if let Some(n) = natural {
                        c.frequency *= speed / n;
                    }
                    (Arc::new(c), walk_clip_path(&skeleton, female))
                })
            } else {
                let p = idle_clip(&skeleton);
                ctx.clip(&p).map(|c| (c, p))
            };
            let tr = &nav.travellers[i];
            let v = tr.visual.as_ref().expect("spawned");
            if let Some((clip, path)) = clip {
                play_on_actor(&mut commands, &v.actor, &v.skeleton, &clip, &path, 0.0, false);
            }
        }
    }
}

/// `--follow-npc`: the camera behind the first traveller, a little above, looking at it.
pub fn follow_camera(time: Res<Time>, mut nav: ResMut<NavState>, mut cams: Query<(&mut Transform, &mut crate::fly::FlyCam)>) {
    if !nav.follow {
        return;
    }
    let Some(tr) = nav.travellers.first() else { return };
    let Ok((mut t, mut fly)) = cams.single_mut() else { return };
    let feet = Vec3::from_array(tr.walker.feet);
    let (s, c) = tr.walker.heading.sin_cos();
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
                (None, Some(s)) => format!("{}: arrived after {s:.1}s", t.label),
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
    fn in_place_takes_out_the_stride_and_keeps_the_bob() {
        use fnv_nif::anim::{Channel, Key, KeyKind, Keys, TransformTrack};
        // The root moves 120 units along y over a 1.2 s cycle and bobs 2 units mid-way.
        let key = |time: f32, y: f32, z: f32| Key { time, value: [0.0, y, z], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] };
        let track = TransformTrack {
            translation: Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, 0.0, 50.0), key(0.6, 60.0, 52.0), key(1.2, 120.0, 50.0)] }),
            rotation: Channel::Static(None),
            euler: None,
            scale: Channel::Static(None),
        };
        let clip = Clip {
            name: "Forward".into(),
            start: 0.0,
            stop: 1.2,
            frequency: 1.0,
            cycle_type: 0,
            tracks: vec![("Bip01".into(), Arc::new(track))],
            skipped: 0,
            accum_root: Some("Bip01".into()),
        };
        let (c, speed) = in_place(&clip);
        assert!((speed.unwrap() - 100.0).abs() < 1e-3);
        let t = &c.tracks[0].1.translation;
        assert_eq!(t.sample(0.0), Some([0.0, 0.0, 50.0]));
        assert_eq!(t.sample(1.2), Some([0.0, 0.0, 50.0]));
        assert_eq!(t.sample(0.6), Some([0.0, 0.0, 52.0]));
        // Without an accumulation root there is nothing to take out.
        let (_, none) = in_place(&Clip { accum_root: None, ..clip });
        assert_eq!(none, None);
    }

    #[test]
    fn walk_clip_sits_in_the_sex_folder_beside_the_idle() {
        assert_eq!(walk_clip_path(r"Characters\_Male\Skeleton.NIF", false), r"characters\_male\locomotion\male\mtforward.kf");
        assert_eq!(walk_clip_path(r"Characters\_Male\Skeleton.NIF", true), r"characters\_male\locomotion\female\mtforward.kf");
    }
}
