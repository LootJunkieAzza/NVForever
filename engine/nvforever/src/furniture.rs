//! Furniture in use (docs/spec-notes/furniture.md): actors and the player get onto
//! chairs, stools, benches and beds, stay there, and get off again.
//!
//! `fnv_actor::furniture` decides everything that does not need the engine: the entry
//! point (a marker of the furniture's model that the base's marker flags enable), the
//! clips (the ones the game's IDLE tree names for the marker, the actor's sex and the
//! phase), and where the actor's root stands in each phase (`UseFrames`). Here the clips
//! are played and the roots moved:
//!
//! - An actor with a [`SitOn`] walks the last steps to the entry point, plays the
//!   getting-on clip, then the loop on the furniture; [`GetUp`] makes it play the
//!   getting-off clip, after which it is back at the entry point with its back to the
//!   furniture and both components are gone. `--npc-sit ACTOR=FURNITURE[:SECONDS]`
//!   sends a travelling NPC (`nav.rs`) to a piece of furniture and sits it down.
//! - The player uses furniture by activating it (`nvf_play::furniture`) and gets up by
//!   pressing a movement control or activating it again. Its third-person body plays the
//!   clips ([`BodyOverride`], read by `player_body.rs`); in first person the view is that
//!   body's eye and the body is not drawn [design]. Beds are used the same way: the
//!   player lies down as a sleeping actor does [owner request 2026-10-10]. No game time
//!   passes and there is no sleep menu (not built).
//!
//! While a clip plays, `GetSitting` / `GetSleeping` and the other furniture functions of
//! the script runtime answer for the user (`nvf_play::furniture::set_use`).
//!
//! Not done: work spots without getting-on clips (marker 22), fidgets and eating while
//! seated (state 3 of the IDLE tree), checking the entry point against the navmesh, and
//! sandboxing actors choosing furniture themselves (residents use the furniture their
//! package names, a bed for Sleep and a seat for Eat packages: `residents.rs`).

use crate::actors::ActorWorld;
use crate::limbs::ActorRig;
use crate::locomotion::Locomotion;
use crate::nav::NavState;
use crate::world::WorldData;
use bevy::prelude::*;
use fnv_actor::furniture::{self as furn, state, BodyAt, ClipEnds, EntryPoint, Frame, Furniture, Marker, Phase, Use, UseEvent, UseFrames, UseIdles, User};
use fnv_actor::idle::Idles;
use fnv_actor::locomotion::yaw_of;
use fnv_sim::FormId;
use nvf_assets::actor::{bind_clip, ActorCtx, ActorParts};
use nvf_assets::anim_play::{BoundClip, Clip};
use nvf_assets::data::GameData;
use nvf_assets::model::ModelCtx;
use nvf_play::furniture::{PlayerSeat, UseFurniture};
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;
use std::sync::Arc;

/// Speed of the last steps to the entry point, game units per second (the travellers'
/// walking speed, `nav::WALK_SPEED`).
const APPROACH_SPEED: f32 = crate::nav::WALK_SPEED;
/// The longest way an actor is moved to an entry point without a path, game units
/// [design: an actor sent to furniture arrives next to it].
const APPROACH_MAX: f32 = 400.0;

/// An actor is to use this furniture reference (put on the actor's root entity).
#[derive(Component, Debug, Clone, Copy)]
pub struct SitOn {
    pub furniture: u32,
    /// Get up by itself after this many seconds on it.
    pub stay: Option<f32>,
}

/// The actor is to get off the furniture it is on.
#[derive(Component, Debug, Clone, Copy)]
pub struct GetUp;

/// Who uses the furniture, as the IDLE conditions need it.
#[derive(Debug, Clone, Copy)]
struct Who {
    reference: u32,
    base: u32,
    female: bool,
}

/// What a use needs, decided once.
struct Plan {
    furniture: u32,
    base: u32,
    entry: EntryPoint,
    get_on: Arc<Clip>,
    /// The loop on the furniture, when the IDLE tree names one.
    stay: Option<Arc<Clip>>,
    frames: UseFrames,
    /// How far the getting-on clip moves the root (`Bip01`), in the actor's space.
    travel: Vec3,
    /// The body's turn in a standing actor (the getting-on clip's first).
    standing_yaw: f32,
}

/// `--npc-sit`: a traveller to sit down when it arrives.
struct Order {
    /// Index of its trip in `NavState::travellers`.
    traveller: usize,
    furniture: u32,
    stay: Option<f32>,
    asked: bool,
}

/// Counts for the run summary.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub sat: u32,
    pub stood: u32,
    pub failed: u32,
    pub player_sat: u32,
    pub player_stood: u32,
}

/// The furniture records and who is on what.
#[derive(Resource)]
pub struct Furnishings {
    bases: HashMap<u32, Furniture>,
    tree: UseIdles,
    /// Markers by model path.
    markers: HashMap<String, Arc<Vec<Marker>>>,
    orders: Vec<Order>,
    /// Per furniture reference: the marker index and the reference of each user.
    users: HashMap<u32, Vec<(usize, u32)>>,
    pub stats: Stats,
    startup: Vec<String>,
}

fn clip_body(clip: &Clip, t: f32) -> BodyAt {
    let track = |n: &str| clip.tracks.iter().find(|(name, _)| name == n).map(|(_, t)| t);
    let yaw = |n: &str| track(n).and_then(|tt| tt.rotation_at(t)).map_or(0.0, yaw_of);
    BodyAt { bip01: track("Bip01").and_then(|tt| tt.translation.sample(t)).unwrap_or([0.0; 3]), yaw: yaw("Bip01") + yaw("Bip01 NonAccum") }
}

fn clip_ends(clip: &Clip) -> ClipEnds {
    ClipEnds { start: clip_body(clip, clip.start), stop: clip_body(clip, clip.stop) }
}

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

impl Furnishings {
    /// Read the furniture records and the furniture part of the IDLE tree, and turn the
    /// `--npc-sit ACTOR=FURNITURE[:SECONDS]` requests into trips for `NavState` (returned;
    /// they follow the `first_trip` trips already asked for).
    pub fn build(world: &WorldData, sit: &[String], first_trip: usize) -> Result<(Furnishings, Vec<String>), String> {
        let t0 = std::time::Instant::now();
        let mut issues = fnv_sim::extract::Issues::default();
        let bases: HashMap<u32, Furniture> = furn::furniture(world.lo, world.schema, &mut issues).into_iter().map(|f| (f.id.0, f)).collect();
        let tree = UseIdles::build(&Idles::build(world.lo, world.schema), world.lo);
        let mut startup =
            vec![format!("furniture: {} bases, {} idles for using them, read in {:.2}s", bases.len(), tree.len(), t0.elapsed().as_secs_f64())];
        let (mut orders, mut trips) = (Vec::new(), Vec::new());
        for spec in sit {
            let bad = || format!("--npc-sit {spec}: expected ACTOR=FURNITURE[:SECONDS], the furniture as a reference FormID");
            let (who, rest) = spec.split_once('=').ok_or_else(bad)?;
            let (target, stay) = match rest.split_once(':') {
                Some((t, s)) => (t, Some(s.trim().parse::<f32>().map_err(|_| bad())?)),
                None => (rest, None),
            };
            let furniture = hex(target).ok_or_else(bad)?;
            let placed = fnv_world::placed::placement(world.lo, world.schema, furniture).ok_or_else(|| format!("--npc-sit {spec}: no placed reference {furniture:08X}"))?;
            let base = placed.base.filter(|b| bases.contains_key(b)).ok_or_else(|| format!("--npc-sit {spec}: {furniture:08X} is not furniture"))?;
            startup.push(format!("furniture: {} is to use {furniture:08X} ({})", who.trim(), bases[&base].edid.as_deref().unwrap_or("?")));
            orders.push(Order { traveller: first_trip + trips.len(), furniture, stay, asked: false });
            trips.push(format!("{}={furniture:08X}", who.trim()));
        }
        Ok((Furnishings { bases, tree, markers: HashMap::new(), orders, users: HashMap::new(), stats: Stats::default(), startup }, trips))
    }

    fn markers(&mut self, data: &GameData, model: &str) -> Arc<Vec<Marker>> {
        if let Some(m) = self.markers.get(model) {
            return m.clone();
        }
        let read = || {
            let bytes = data.read(model)?;
            let file = fnv_nif::file::NifFile::parse(&bytes, Some(&data.schema)).ok()?;
            let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
            Some(furn::markers(&file, &dec))
        };
        let m = Arc::new(read().unwrap_or_default());
        self.markers.insert(model.to_string(), m.clone());
        m
    }

    fn user(plan_entry: &EntryPoint, furniture: u32, base: u32, who: Who, state: u8) -> User {
        User {
            state,
            marker_id: plan_entry.marker_id,
            furniture_ref: FormId(furniture),
            furniture_base: FormId(base),
            actor_base: FormId(who.base),
            // The third-person body plays the clips for the player too (module notes).
            first_person: false,
            female: who.female,
            child: false,
            alerted: false,
            attacked: false,
            random_percent: 99,
        }
    }

    /// Decide how `who`, standing at `from` (game space), uses `furniture`.
    fn plan(&mut self, world: &WorldData, ctx: &mut ActorCtx, furniture: u32, from: [f32; 3], who: Who) -> Result<Plan, String> {
        let placed = fnv_world::placed::placement(world.lo, world.schema, furniture).ok_or("no such reference")?;
        let base = placed.base.ok_or("no base")?;
        let record = self.bases.get(&base).ok_or("not furniture")?.clone();
        let model = record.model.clone().ok_or("the furniture has no model")?;
        let markers = self.markers(&ctx.models.data, &model);
        if markers.is_empty() {
            return Err(format!("{model} has no furniture markers"));
        }
        let points = furn::entry_points(&markers, placed.position, placed.rotation[2], placed.scale);
        // One user per reference, except where every marker is a seat of its own
        // (benches, stools around a table) [design, furniture.md §5].
        let seats = points.len() > 1 && points.iter().all(|p| matches!(p.marker_id, 14 | 15));
        let taken = self.users.get(&furniture).cloned().unwrap_or_default();
        if !seats && !taken.is_empty() {
            return Err("in use".into());
        }
        let entry = furn::nearest_entry(&points, from, |p| record.marker_enabled(p.index) && !taken.iter().any(|(i, _)| *i == p.index))
            .ok_or("no free entry point")?;
        let pick = |tree: &UseIdles, s: u8| tree.pick(&Self::user(&entry, furniture, base, who, s));
        let on = pick(&self.tree, state::GETTING_ON).ok_or_else(|| format!("no clip for getting onto marker {}", entry.marker_id))?;
        let get_on = ctx.clip(&on.clip).ok_or_else(|| format!("{} does not load", on.clip))?;
        let stay = pick(&self.tree, state::LOOP).and_then(|p| ctx.clip(&p.clip));
        let ends = clip_ends(&get_on);
        let standing_yaw = ends.start.yaw;
        let loop_yaw = stay.as_ref().map_or(standing_yaw, |c| clip_body(c, c.start).yaw);
        let frames = UseFrames::new(&entry, &ends, loop_yaw);
        let travel = Vec3::from_array(ends.stop.bip01) - Vec3::from_array(ends.start.bip01);
        Ok(Plan { furniture, base, entry, get_on, stay, frames, travel: Vec3::new(travel.x, travel.y, 0.0), standing_yaw })
    }

    /// The clip that gets `who` off again.
    fn get_off(&self, ctx: &mut ActorCtx, plan: &Plan, who: Who) -> Option<Arc<Clip>> {
        let pick = self.tree.pick(&Self::user(&plan.entry, plan.furniture, plan.base, who, state::GETTING_OFF))?;
        ctx.clip(&pick.clip)
    }

    fn take(&mut self, plan: &Plan, user: u32) {
        self.users.entry(plan.furniture).or_default().push((plan.entry.index, user));
    }

    fn release(&mut self, furniture: u32, user: u32) {
        if let Some(u) = self.users.get_mut(&furniture) {
            u.retain(|(_, who)| *who != user);
            if u.is_empty() {
                self.users.remove(&furniture);
            }
        }
    }
}

pub fn log_startup(mut f: ResMut<Furnishings>) {
    for line in std::mem::take(&mut f.startup) {
        info!("{line}");
    }
}

fn rotation(heading: f32) -> Quat {
    Quat::from_rotation_z(-heading)
}

/// Bevy space (the world root's rotation, `nav.rs`) back to game space.
fn to_game(p: Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y)
}

/// The compass heading a root transform faces.
fn heading_of(t: &Transform) -> f32 {
    let f = t.rotation * Vec3::Y;
    f.x.atan2(f.y)
}

/// The shorter way round from heading `a` to `b`, at share `k`.
fn lerp_heading(a: f32, b: f32, k: f32) -> f32 {
    let d = (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a + d * k
}

fn tell_scripts(script: &mut Option<ResMut<ScriptState>>, user: u32, using: Option<(&Plan, u8)>) {
    if let Some(s) = script.as_deref_mut() {
        nvf_play::furniture::set_use(s, user, using.map(|(p, state)| (p.furniture, p.entry.marker_id, state)));
    }
}

// ---- actors -----------------------------------------------------------------------

enum Stage {
    /// The last steps to the entry point.
    Approach { from: Frame, t: f32, len: f32 },
    Using(Use),
}

/// An actor walking up to, on, or getting off furniture.
#[derive(Component)]
pub struct OnFurniture {
    plan: Plan,
    who: Who,
    stage: Stage,
    /// What the skeleton plays now and where in it (0–1); read by `locomotion::drive`.
    /// `None` while the actor still walks.
    pub pose: Option<(Arc<BoundClip>, f32)>,
    get_on: Arc<BoundClip>,
    stay: Option<Arc<BoundClip>>,
    get_off: Option<(Arc<BoundClip>, ClipEnds)>,
    /// The actor's parent's place in game space (its own transform is relative to it).
    origin: Vec3,
    stay_left: Option<f32>,
}

/// Drawn actors not yet told to sit.
type Unseated = (With<ActorRig>, Without<SitOn>);

/// `--npc-sit`: once a traveller has arrived and is drawn, it is told to sit.
pub fn seat_orders(mut commands: Commands, mut f: ResMut<Furnishings>, nav: Option<Res<NavState>>, actors: Query<(Entity, &RefVisual), Unseated>) {
    let Some(nav) = nav else { return };
    for o in f.orders.iter_mut().filter(|o| !o.asked) {
        let Some(tr) = nav.travellers.get(o.traveller) else { continue };
        if tr.failed.is_some() {
            o.asked = true;
            warn!("furniture: {} did not get to {:08X}", tr.label, o.furniture);
            continue;
        }
        if tr.arrived.is_none() {
            continue;
        }
        if let Some((e, _)) = actors.iter().find(|(_, v)| v.0 == tr.reference) {
            commands.entity(e).insert(SitOn { furniture: o.furniture, stay: o.stay });
            o.asked = true;
        }
    }
}

/// What [`drive_actors`] reads and writes of an actor on furniture.
type Seated = (Entity, &'static ActorRig, &'static mut Transform, &'static mut OnFurniture, Option<&'static mut Locomotion>, Has<GetUp>);

/// Start, advance and end the actors' uses; place their roots.
#[allow(clippy::too_many_arguments)]
pub fn drive_actors(
    mut commands: Commands,
    time: Res<Time>,
    mut f: ResMut<Furnishings>,
    world: Res<WorldData>,
    actor_world: Option<Res<ActorWorld>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    starting: Query<(Entity, &ActorRig, &Transform, &GlobalTransform, &SitOn), Without<OnFurniture>>,
    mut using: Query<Seated>,
    mut script: Option<ResMut<ScriptState>>,
    mut nav: Option<ResMut<NavState>>,
) {
    let dt = time.delta_secs().min(0.1);
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, t, global, sit) in &starting {
        let at = to_game(global.translation());
        let female = actor_world.as_deref().and_then(|w| w.actors.npcs.get(&FormId(rig.base))).is_some_and(|n| n.female);
        let who = Who { reference: rig.reference, base: rig.base, female };
        let plan = f.plan(&world, &mut ctx, sit.furniture, at.to_array(), who).and_then(|p| {
            let d = Vec3::from_array(p.entry.pos).distance(at);
            if d > APPROACH_MAX { Err(format!("{d:.0} units from its entry point")) } else { Ok((p, d)) }
        });
        match plan {
            Ok((plan, d)) => {
                info!(
                    "furniture: {:08X} goes to {:08X} by marker {} (ID {}), {d:.0} units away; clips {} / {}",
                    rig.reference,
                    plan.furniture,
                    plan.entry.index,
                    plan.entry.marker_id,
                    plan.get_on.name,
                    plan.stay.as_ref().map_or("none", |c| c.name.as_str())
                );
                f.take(&plan, rig.reference);
                let get_on = Arc::new(bind_clip(&rig.skel, plan.get_on.clone()));
                let stay = plan.stay.clone().map(|c| Arc::new(bind_clip(&rig.skel, c)));
                let from = Frame { pos: at.to_array(), heading: heading_of(t) };
                // Its body is the furniture's now: a conversation turns only its head.
                commands.entity(e).insert(crate::dialogue_look::BodyHeld).insert(OnFurniture {
                    plan,
                    who,
                    stage: Stage::Approach { from, t: 0.0, len: (d / APPROACH_SPEED).max(0.15) },
                    pose: None,
                    get_on,
                    stay,
                    get_off: None,
                    origin: at - t.translation,
                    stay_left: sit.stay,
                });
            }
            Err(why) => {
                warn!("furniture: {:08X} cannot use {:08X}: {why}", rig.reference, sit.furniture);
                f.stats.failed += 1;
                commands.entity(e).remove::<(SitOn, GetUp)>();
            }
        }
    }

    for (e, rig, mut t, mut on, mut lo, get_up) in &mut using {
        let on = &mut *on;
        let scale = t.scale.x.max(1e-3);
        let mut place = |frame: Frame| {
            t.translation = Vec3::from_array(frame.pos) - on.origin;
            t.rotation = rotation(frame.heading);
        };
        let clip_frame = on.plan.frames.clip;
        match &mut on.stage {
            Stage::Approach { from, t: at, len } => {
                *at += dt;
                let k = (*at / *len).min(1.0);
                let to = Frame { pos: on.plan.entry.pos, heading: on.plan.entry.heading };
                let pos = Vec3::from_array(from.pos).lerp(Vec3::from_array(to.pos), k);
                place(Frame { pos: pos.to_array(), heading: lerp_heading(from.heading, to.heading, k) });
                if let Some(lo) = lo.as_deref_mut() {
                    // Walking: the distance over the time, in the actor's own scale.
                    let speed = Vec3::from_array(from.pos).distance(Vec3::from_array(to.pos)) / *len;
                    lo.velocity = [0.0, if k < 1.0 { speed / scale } else { 0.0 }];
                    lo.turn = 0.0;
                }
                if k >= 1.0 {
                    let len = on.get_on.clip.length();
                    on.stage = Stage::Using(Use::begin(FormId(on.plan.furniture), FormId(on.plan.base), on.plan.entry, len));
                    on.pose = Some((on.get_on.clone(), 0.0));
                    place(clip_frame);
                    tell_scripts(&mut script, on.who.reference, Some((&on.plan, state::GETTING_ON)));
                }
            }
            Stage::Using(usage) => {
                if get_up {
                    usage.leave_wanted = true;
                }
                let ev = usage.advance(dt);
                if ev == Some(UseEvent::GotOn) {
                    f.stats.sat += 1;
                    info!("furniture: {:08X} is on {:08X}", on.who.reference, on.plan.furniture);
                    tell_scripts(&mut script, on.who.reference, Some((&on.plan, state::ON)));
                }
                if usage.phase == Phase::On {
                    if let Some(left) = on.stay_left.as_mut() {
                        *left -= dt;
                        if *left <= 0.0 {
                            usage.leave_wanted = true;
                            on.stay_left = None;
                        }
                    }
                    if usage.leave_wanted && usage.leave() {
                        let clip = f.get_off(&mut ctx, &on.plan, on.who);
                        let bound = clip.map(|c| (clip_ends(&c), Arc::new(bind_clip(&rig.skel, c))));
                        usage.begin_leaving(bound.as_ref().map_or(0.0, |(_, b)| b.clip.length()));
                        on.get_off = bound.map(|(ends, b)| (b, ends));
                        tell_scripts(&mut script, on.who.reference, Some((&on.plan, state::GETTING_OFF)));
                    }
                }
                let phase = if usage.clip_len > 0.0 { (usage.t / usage.clip_len).min(0.999) } else { 0.999 };
                match usage.phase {
                    Phase::GettingOn => {
                        on.pose = Some((on.get_on.clone(), phase));
                        place(Frame { pos: clip_frame.point((on.plan.travel * phase).to_array()), heading: clip_frame.heading });
                    }
                    Phase::On => match &on.stay {
                        Some(stay) => {
                            let len = stay.clip.length().max(1e-3);
                            on.pose = Some((stay.clone(), (usage.t / len).fract()));
                            place(on.plan.frames.on);
                        }
                        // No loop for this furniture: the getting-on clip's last pose.
                        None => {
                            on.pose = Some((on.get_on.clone(), 0.999));
                            place(Frame { pos: clip_frame.point(on.plan.travel.to_array()), heading: clip_frame.heading });
                        }
                    },
                    Phase::GettingOff if ev != Some(UseEvent::GotOff) => {
                        if let Some((clip, ends)) = &on.get_off {
                            let travel = Vec3::from_array(ends.stop.bip01) - Vec3::from_array(ends.start.bip01);
                            on.pose = Some((clip.clone(), phase));
                            place(Frame { pos: clip_frame.point([travel.x * phase, travel.y * phase, 0.0]), heading: clip_frame.heading });
                        }
                    }
                    Phase::GettingOff => {
                        // Back on its feet: at the entry point, turned as the clip left it.
                        let end = match &on.get_off {
                            Some((_, ends)) => on.plan.frames.after(ends, on.plan.standing_yaw),
                            None => Frame { pos: on.plan.entry.pos, heading: on.plan.entry.heading },
                        };
                        place(end);
                        if let Some(tr) = nav.as_deref_mut().and_then(|n| n.travellers.iter_mut().find(|tr| tr.reference == on.who.reference)) {
                            tr.walker.feet = end.pos;
                            tr.walker.heading = end.heading;
                        }
                        if let Some(lo) = lo.as_deref_mut() {
                            lo.velocity = [0.0; 2];
                            lo.turn = 0.0;
                        }
                        f.stats.stood += 1;
                        f.release(on.plan.furniture, on.who.reference);
                        info!("furniture: {:08X} got off {:08X}", on.who.reference, on.plan.furniture);
                        tell_scripts(&mut script, on.who.reference, None);
                        commands.entity(e).remove::<(OnFurniture, SitOn, GetUp, crate::dialogue_look::BodyHeld)>();
                    }
                }
            }
        }
    }
}

// ---- the player -------------------------------------------------------------------

/// A clip the player's third-person body plays instead of its own, with the body's
/// place (`player_body.rs`).
#[derive(Clone)]
pub struct BodyClip {
    pub clip: Arc<Clip>,
    pub looped: bool,
    /// Changes when the clip does: the body starts it from its beginning.
    pub serial: u32,
    pub pos: [f32; 3],
    pub heading: f32,
    /// The furniture is a bed: the body lies down, and the first-person view lies down
    /// with it (`player_body.rs`).
    pub lying: bool,
}

#[derive(Resource, Default)]
pub struct BodyOverride(pub Option<BodyClip>);

struct PlayerUsing {
    plan: Plan,
    usage: Use,
    get_off: Option<(Arc<Clip>, ClipEnds)>,
}

/// The player's use of furniture.
#[derive(Resource, Default)]
pub struct PlayerUse {
    using: Option<PlayerUsing>,
    serial: u32,
}

/// Where a getting-on or getting-off clip has the root at `t` seconds into it: the
/// player's body holds `Bip01` still (`nvf_assets::anim_blend`), so the root follows its
/// track.
fn root_at(frame: &Frame, clip: &Clip, t: f32) -> [f32; 3] {
    let p = clip_body(clip, clip.start + t.clamp(0.0, clip.length())).bip01;
    frame.point([p[0], p[1], 0.0])
}

/// The player's use of furniture: start it on activation, advance it, end it.
#[allow(clippy::too_many_arguments)]
pub fn drive_player(
    time: Res<Time>,
    f: Option<ResMut<Furnishings>>,
    world: Res<WorldData>,
    mut asked: MessageReader<UseFurniture>,
    mut seat: ResMut<PlayerSeat>,
    mut state_: ResMut<PlayerUse>,
    mut body: ResMut<BodyOverride>,
    player: Res<nvf_play::player::Player>,
    mut control: ResMut<nvf_play::control::PlayerControl>,
    mut phys: ResMut<nvf_play::phys::PhysState>,
    body_state: Res<crate::player_body::PlayerBody>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut script: Option<ResMut<ScriptState>>,
    mut hud: ResMut<nvf_play::activate::HudMessage>,
) {
    let Some(mut f) = f else { return };
    let dt = time.delta_secs().min(0.1);
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let who = Who { reference: FormId::PLAYER_REF.0, base: FormId::PLAYER_BASE.0, female: body_state.female().unwrap_or(false) };
    let st = &mut *state_;
    for UseFurniture { reference } in asked.read().copied() {
        match &mut st.using {
            // Activating the furniture one is on: get up.
            Some(u) if u.plan.furniture == reference => seat.leave = true,
            Some(_) => {}
            None => match f.plan(&world, &mut ctx, reference, player.feet, who) {
                // Beds as seats: the player lies down as a sleeping actor does [owner
                // request 2026-10-10; the original's sleep menu is not built].
                Ok(plan) => {
                    info!(
                        "furniture: the player {} {reference:08X} by marker {} (ID {}); clips {} / {}",
                        if furn::is_bed(plan.entry.marker_id) { "lies down on" } else { "uses" },
                        plan.entry.index,
                        plan.entry.marker_id,
                        plan.get_on.name,
                        plan.stay.as_ref().map_or("none", |c| c.name.as_str())
                    );
                    f.take(&plan, who.reference);
                    // Sitting puts the weapon away [design].
                    control.weapon_out = false;
                    let usage = Use::begin(FormId(plan.furniture), FormId(plan.base), plan.entry, plan.get_on.length());
                    tell_scripts(&mut script, who.reference, Some((&plan, state::GETTING_ON)));
                    st.serial += 1;
                    st.using = Some(PlayerUsing { plan, usage, get_off: None });
                    seat.leave = false;
                }
                Err(why) => {
                    info!("furniture: the player cannot use {reference:08X}: {why}");
                    hud.show(format!("You cannot use this ({why})."));
                }
            },
        }
    }
    let Some(u) = st.using.as_mut() else {
        seat.leave = false;
        return;
    };
    if std::mem::take(&mut seat.leave) {
        u.usage.leave_wanted = true;
    }
    let ev = u.usage.advance(dt);
    if ev == Some(UseEvent::GotOn) {
        f.stats.player_sat += 1;
        st.serial += 1;
        info!("furniture: the player is on {:08X}", u.plan.furniture);
        tell_scripts(&mut script, who.reference, Some((&u.plan, state::ON)));
    }
    if u.usage.phase == Phase::On && u.usage.leave_wanted && u.usage.leave() {
        let clip = f.get_off(&mut ctx, &u.plan, who);
        u.usage.begin_leaving(clip.as_ref().map_or(0.0, |c| c.length()));
        u.get_off = clip.map(|c| (c.clone(), clip_ends(&c)));
        st.serial += 1;
        tell_scripts(&mut script, who.reference, Some((&u.plan, state::GETTING_OFF)));
    }
    let frames = u.plan.frames;
    let lying = u.usage.is_bed();
    let shown = match u.usage.phase {
        Phase::GettingOn => Some(BodyClip {
            clip: u.plan.get_on.clone(),
            looped: false,
            serial: st.serial,
            pos: root_at(&frames.clip, &u.plan.get_on, u.usage.t),
            heading: frames.clip.heading,
            lying,
        }),
        Phase::On => Some(match &u.plan.stay {
            Some(stay) => BodyClip { clip: stay.clone(), looped: true, serial: st.serial, pos: frames.on.pos, heading: frames.on.heading, lying },
            None => BodyClip {
                clip: u.plan.get_on.clone(),
                looped: false,
                serial: st.serial,
                pos: root_at(&frames.clip, &u.plan.get_on, f32::MAX),
                heading: frames.clip.heading,
                lying,
            },
        }),
        Phase::GettingOff if ev != Some(UseEvent::GotOff) => u.get_off.as_ref().map(|(c, _)| BodyClip {
            clip: c.clone(),
            looped: false,
            serial: st.serial,
            pos: root_at(&frames.clip, c, u.usage.t),
            heading: frames.clip.heading,
            lying,
        }),
        Phase::GettingOff => None,
    };
    match shown {
        Some(b) => {
            seat.hold = Some(b.pos);
            body.0 = Some(b);
        }
        None => {
            // Off: the character stands at the entry point again.
            let end = match &u.get_off {
                Some((_, ends)) => frames.after(ends, u.plan.standing_yaw),
                None => Frame { pos: u.plan.entry.pos, heading: u.plan.entry.heading },
            };
            phys.place_player(end.pos, 60.0, 200.0);
            seat.hold = None;
            body.0 = None;
            f.stats.player_stood += 1;
            f.release(u.plan.furniture, who.reference);
            info!("furniture: the player got off {:08X}", u.plan.furniture);
            tell_scripts(&mut script, who.reference, None);
            st.using = None;
        }
    }
}

/// The furniture line of the run summary.
pub fn report(mut exit: MessageReader<AppExit>, f: Res<Furnishings>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = f.stats;
    info!(
        "furniture summary: {} of {} actors sat down, {} got up again, {} could not; the player sat down {} times and got up {}",
        s.sat,
        f.orders.len(),
        s.stood,
        s.failed,
        s.player_sat,
        s.player_stood
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    #[test]
    fn furniture_references_are_hex_form_ids() {
        assert_eq!(hex("0010634A"), Some(0x0010_634A));
        assert_eq!(hex(" 0x10634a "), Some(0x0010_634A));
        assert_eq!(hex("EasyPeteChairRef"), None);
    }

    #[test]
    fn headings_turn_the_short_way() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(lerp_heading(0.0, FRAC_PI_2, 0.5), FRAC_PI_2 / 2.0));
        // From just west of north to just east of it: through north, not round the south.
        assert!(close(lerp_heading(TAU - 0.2, 0.2, 0.5).rem_euclid(TAU), 0.0) || close(lerp_heading(TAU - 0.2, 0.2, 0.5), TAU));
        assert!(close(lerp_heading(0.3, 0.3 + PI - 0.01, 1.0), 0.3 + PI - 0.01));
    }

    #[test]
    fn a_root_transform_faces_its_heading() {
        for h in [0.0, 0.7, FRAC_PI_2, 2.5] {
            let t = Transform::from_rotation(rotation(h));
            assert!((heading_of(&t) - h).abs() < 1e-4, "{h}");
        }
        // Game space and the world root's space.
        assert_eq!(to_game(Vec3::new(1.0, 3.0, -2.0)), Vec3::new(1.0, 2.0, 3.0));
    }
}
