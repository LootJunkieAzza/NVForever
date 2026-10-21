//! The player's grab: picking up a loose object or a corpse, carrying it in front of the
//! view, and dropping or throwing it (docs/spec-notes/grab.md).
//!
//! - What can be held ([`Candidate`], found every frame by [`grab`]): under the
//!   crosshair, within the activation reach (`activate::pick::REACH`), a reference whose
//!   collision is one dynamic body (loose clutter placed in the world, an item dropped)
//!   or the ragdoll of a corpse.
//! - The Grab control (Z, the game's own binding; the right stick's press on a pad)
//!   held picks it up and keeps it while held [hypothesis: the settings are the
//!   "ZKey" ones, `fZKey*`, grab.md §2]. Activate (E) held for [`ACTIVATE_HOLD`] on it
//!   picks it up too, and keeps it until E is let go; a shorter press is the activation
//!   as before, made when E comes up (`activate::activate` hands such presses here)
//!   [hypothesis: tap to take, hold to grab, grab.md §2; the tutorial's brahmin skull
//!   has an empty OnActivate block so that a press does nothing].
//! - Held, its grip point is wanted where the crosshair ray was when it was picked up, at
//!   the same distance from the eye ([`hold_distance`]), and physics moves it there
//!   (`fnv_phys::grab`). Use (attack) throws it along the view ([`throw_speed`]) and does
//!   not fire. It is let go when the key comes up, when it catches on something more
//!   than [`MAX_GAP`] from where it is wanted, when play stops (a menu) or the body is
//!   gone (its cell unloaded).
//! - Weight: a body heavier than `fGrabMaxWeightWalking` is not picked up, and while one
//!   heavier than `fGrabMaxWeightRunning` is held the player walks [hypothesis: the
//!   meaning of the two settings, read from their names; neither is in the shipped
//!   plugins, so their values here are guesses, grab.md §3]. For a corpse the weight is
//!   the held body's, not the whole ragdoll's [hypothesis].
//! - Scripts: `fnv_game` is told what is held (`Game::set_player_grab`), which runs
//!   OnGrab and OnRelease blocks and answers IsPlayerGrabbedRef.
//! - Where it comes to rest after a drop or a throw is the physics write-back's
//!   (`crate::phys`): placed clutter keeps its new place, a dropped item too.
//!
//! Test aid: `NVF_PLAY_TEST_GRAB=<seconds>:<z|e>:<hold seconds>:<turn degrees>[:throw][:at=<FormID>]`
//! (several, comma-separated, on the world aids' clock): at that time the Grab key (`z`)
//! or Activate (`e`) is held for that long on what is under the crosshair (with `at=`,
//! after the view is turned to that reference's body) while the view turns right by the
//! degrees, then let go (or, with `throw`, Use pressed first).

use crate::activate::pick::REACH;
use crate::activate::{Target, Targeted};
use crate::bindings::{Control, ControlInput};
use crate::control::{Crosshair, PlayerControl};
use crate::phys::PhysState;
use crate::player::{MoveIntent, Player};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_phys::{layers, Filter, GripOn, HoldParams, Owner};
use fnv_sim::FormId;

/// How long Activate must be held on something that can be held to pick it up rather
/// than activate it, seconds [hypothesis].
pub const ACTIVATE_HOLD: f32 = 0.4;

/// The nearest a held object is kept to the eye, game units [hypothesis].
pub const MIN_HOLD: f32 = 50.0;

/// A held object that is more than this from where it is wanted (caught behind a wall or
/// under something) is let go, game units [hypothesis: the role of the original's
/// `fZKeyMaxContactDistance`, whose value is not known].
pub const MAX_GAP: f32 = 90.0;

/// A throw gives the held body this much momentum, kg × game units a second, at most
/// [`THROW_MAX_SPEED`] [hypothesis].
pub const THROW_IMPULSE: f32 = 2500.0;
pub const THROW_MAX_SPEED: f32 = 600.0;

/// The heaviest bodies picked up and carried at a run, kg, when the plugins do not set
/// `fGrabMaxWeightWalking` / `fGrabMaxWeightRunning` [hypothesis: the shipped plugins
/// set neither; values chosen so that clutter and a corpse's limbs and trunk can be
/// held and a car cannot].
pub const MAX_WEIGHT_WALKING: f32 = 100.0;
pub const MAX_WEIGHT_RUNNING: f32 = 25.0;

/// What the player could hold now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub on: GripOn,
    /// The crosshair ray's hit on it, game units.
    pub at: [f32; 3],
    pub distance: f32,
}

/// Which control holds the grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    GrabKey,
    Activate,
    /// The test aid.
    Aid,
}

/// What the player holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub on: GripOn,
    pub name: String,
    pub by: By,
    pub mass: f32,
    /// Distance from the eye the grip point is kept at.
    pub distance: f32,
    /// Where it was picked up (the grip point then).
    pub from: [f32; 3],
    /// Where its body was then (its centre: the trunk of a corpse).
    pub body_from: Option<[f32; 3]>,
    /// Seconds held.
    pub seconds: f32,
    /// Heavier than the running limit: the player walks.
    pub walks: bool,
    carry_logged: bool,
}

/// A press of Activate on something that can be held, not yet a tap or a hold.
#[derive(Debug, Clone, PartialEq)]
pub struct Tap {
    /// What the press would activate (`None`: nothing, an object without activation).
    pub target: Option<Targeted>,
    pub candidate: Candidate,
    pub held: f32,
}

/// After a drop: watched until it comes to rest, for the log.
#[derive(Debug, Clone, PartialEq)]
struct Falling {
    on: GripOn,
    name: String,
    /// Where its body was when it was picked up.
    from: Option<[f32; 3]>,
    seconds: f32,
}

/// The grab's state.
#[derive(Resource, Debug, Default)]
pub struct GrabState {
    pub candidate: Option<Candidate>,
    pub held: Option<Held>,
    /// A press of Activate on the candidate (set by `activate::activate`, or here for an
    /// object without activation).
    pub tap: Option<Tap>,
    falling: Option<Falling>,
    weights: Option<(f32, f32)>,
    aid: Vec<AidGrab>,
    aid_now: Option<AidGrab>,
}

impl GrabState {
    /// A press of Activate on a target held off: activated when E comes up (`pressed`
    /// false) before [`ACTIVATE_HOLD`]; `None` while it may still become a grab. A press
    /// with nothing to activate (an object without activation, the test aid's) is the
    /// grab's own to end.
    pub fn released_tap(&mut self, pressed: bool) -> Option<Targeted> {
        if pressed || !self.tap.as_ref().is_some_and(|t| t.target.is_some()) {
            return None;
        }
        self.tap.take().and_then(|t| t.target)
    }

    /// Whether a press of Activate on `form_id` is to wait for a tap or a hold.
    pub fn holdable(&self, form_id: u32) -> bool {
        self.held.is_none() && self.candidate.is_some_and(|c| c.on.form_id() == form_id)
    }
}

/// One timed use of the test aid.
#[derive(Debug, Clone, Copy, PartialEq)]
struct AidGrab {
    at: f32,
    activate: bool,
    hold: f32,
    turn_deg: f32,
    throw: bool,
    /// First turn the view to this reference's body (its centre of mass).
    aim: Option<u32>,
    /// Seconds since it started.
    t: f32,
}

fn parse_aid(v: &str) -> Vec<AidGrab> {
    v.split(',')
        .filter_map(|item| {
            let f: Vec<&str> = item.trim().split(':').collect();
            if f.len() < 4 {
                warn!("NVF_PLAY_TEST_GRAB: '{item}' is not <seconds>:<z|e>:<hold seconds>:<turn degrees>[:throw]");
                return None;
            }
            Some(AidGrab {
                at: f[0].parse().ok()?,
                activate: f[1].eq_ignore_ascii_case("e"),
                hold: f[2].parse().ok()?,
                turn_deg: f[3].parse().ok()?,
                throw: f[4..].iter().any(|s| s.eq_ignore_ascii_case("throw")),
                aim: f[4..].iter().find_map(|s| u32::from_str_radix(s.strip_prefix("at=")?.trim_start_matches("0x"), 16).ok()),
                t: 0.0,
            })
        })
        .collect()
}

pub struct GrabPlugin;

impl Plugin for GrabPlugin {
    fn build(&self, app: &mut App) {
        let aid = std::env::var("NVF_PLAY_TEST_GRAB").map(|v| parse_aid(&v)).unwrap_or_default();
        app.insert_resource(GrabState { aid, ..default() })
            .add_systems(Update, holding_controls.after(PlaySet::Input).before(PlaySet::Physics))
            .add_systems(Update, grab.in_set(PlaySet::Activation).after(crate::activate::find_target).before(crate::activate::activate));
    }
}

/// The distance the grip point is kept from the eye: where it was picked up, no nearer
/// than [`MIN_HOLD`] [hypothesis].
pub fn hold_distance(hit: f32) -> f32 {
    hit.clamp(MIN_HOLD, REACH)
}

/// The speed a throw gives a body of `mass` kg.
pub fn throw_speed(mass: f32) -> f32 {
    (THROW_IMPULSE / mass.max(0.1)).min(THROW_MAX_SPEED)
}

/// While something is held: Use throws instead of firing, and a heavy load walks.
fn holding_controls(state: Res<GrabState>, mut control: ResMut<PlayerControl>, mut intent: ResMut<MoveIntent>) {
    let Some(h) = state.held.as_ref() else { return };
    control.trigger = false;
    if h.walks {
        intent.walk = true;
    }
}

/// The grab settings: (walking, running) weight limits.
fn weights(sim: &SimState) -> (f32, f32) {
    let f = |n: &str, d: f32| sim.world.setting_f32(n).filter(|v| *v > 0.0).unwrap_or(d);
    (f("fGrabMaxWeightWalking", MAX_WEIGHT_WALKING), f("fGrabMaxWeightRunning", MAX_WEIGHT_RUNNING))
}

/// What is under the crosshair that could be held; else what the ray found, for the log.
fn find_candidate(phys: &mut PhysState, sim: &SimState, eye: [f32; 3], dir: [f32; 3]) -> Result<Candidate, String> {
    let mut filter = phys.player.as_ref().map_or_else(Filter::default, |c| Filter::excluding(c.key()));
    filter.exclude_layers.insert(layers::TRIGGER);
    let hit = phys.ray(eye, dir, REACH, filter).ok_or("nothing within reach")?;
    let on = match hit.owner {
        Owner::Ref { form_id, .. } => GripOn::Ref(form_id),
        Owner::Actor { form_id, layer, .. } if layer == layers::DEAD_BIP && phys.world.has_ragdoll(form_id) => GripOn::Ragdoll(form_id),
        Owner::Actor { form_id, .. } => return Err(format!("the living actor {form_id:08X} at {:.0}", hit.distance)),
        o => return Err(format!("{o:?} at {:.0}", hit.distance)),
    };
    let id = on.form_id();
    if !sim.present(id) {
        return Err(format!("{id:08X}, not in the world"));
    }
    if phys.world.grip_mass(on, hit.point).is_none() {
        return Err(format!("{id:08X} at {:.0}, no loose body", hit.distance));
    }
    Ok(Candidate { on, at: hit.point, distance: hit.distance })
}

/// A reference's name for the log.
fn name_of(sim: &SimState, script: Option<&crate::script::ScriptState>, form_id: u32) -> String {
    let game = script.and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    format!("{} ({form_id:08X})", sim.ref_name(game, form_id).unwrap_or("unnamed"))
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

fn fmt3(p: [f32; 3]) -> String {
    format!("({:.1}, {:.1}, {:.1})", p[0], p[1], p[2])
}

/// Tell the scripts what is held; log the blocks that ran.
fn tell_scripts(script: Option<&mut crate::script::ScriptState>, sim: &mut SimState, now: Option<u32>) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    let (released, grabbed) = crate::script::with_world(rt, sim, |rt| rt.game.set_player_grab(now.map(FormId)));
    if released {
        info!("grab: the released object's OnRelease ran");
    }
    if grabbed {
        info!("grab: the grabbed object's OnGrab ran");
    }
}

/// The player's view and what holds the controls: the crosshair, the activation target,
/// furniture, VATS mode.
type Around<'w> = (Res<'w, PlayerControl>, Res<'w, Target>, Option<Res<'w, crate::furniture::PlayerSeat>>, Option<Res<'w, crate::vats_mode::VatsMode>>);

/// Find what could be held, pick it up, carry it, let it go (see the module notes).
#[allow(clippy::too_many_arguments)]
fn grab(
    input: ControlInput,
    time: Res<Time>,
    clocks: (Option<Res<crate::pause::PlayClock>>, Option<Res<crate::pause::WorldAidClock>>),
    state: Res<State<PlayState>>,
    mut grab: ResMut<GrabState>,
    mut player: ResMut<Player>,
    phys: Option<ResMut<PhysState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<crate::script::ScriptState>>,
    (control, target, seat, vats): Around,
) {
    let Some(mut phys) = phys else { return };
    let g = &mut *grab;
    let dt = time.delta_secs();
    let playing = *state.get() == PlayState::Playing && seat.is_none_or(|s| s.hold.is_none()) && !vats.is_some_and(|v| v.holds_input());
    if g.weights.is_none() {
        let w = weights(&sim);
        info!("grab: picks up up to {:.0} kg, runs with up to {:.0} kg (fGrabMaxWeightWalking, fGrabMaxWeightRunning or the guesses)", w.0, w.1);
        g.weights = Some(w);
    }
    let (max_walk, max_run) = g.weights.unwrap_or((MAX_WEIGHT_WALKING, MAX_WEIGHT_RUNNING));

    // The test aid: start a timed use, turn the view while it lasts.
    let now = time.elapsed_secs() + clocks.0.as_ref().map_or(0.0, |c| c.stopped_secs());
    let world_now = clocks.1.as_ref().map_or(Some(now), |w| w.at(now));
    if g.aid_now.is_none() && playing {
        if let Some(i) = world_now.and_then(|n| g.aid.iter().position(|a| a.at <= n)) {
            let a = g.aid.remove(i);
            info!(
                "test aid NVF_PLAY_TEST_GRAB: {} held {:.1} s, the view turning {:.0}°{}",
                if a.activate { "Activate" } else { "Grab" },
                a.hold,
                a.turn_deg,
                if a.throw { ", then thrown" } else { "" }
            );
            g.aid_now = Some(a);
        }
    }
    let mut aid_release = false;
    let mut aid_throw = false;
    let mut aid_press: Option<bool> = None;
    if let Some(a) = g.aid_now.as_mut() {
        if a.t == 0.0 {
            aid_press = Some(a.activate);
            // Aimed at the body's centre, as a player puts the crosshair on it.
            if let Some(c) = a.aim.and_then(|id| phys.world.ref_center(id).or_else(|| phys.world.ragdoll_center(id))) {
                let e = player.eye();
                let d = [c[0] - e[0], c[1] - e[1], c[2] - e[2]];
                let len = dist(c, e).max(1e-3);
                player.heading = d[0].atan2(d[1]);
                player.pitch = (d[2] / len).clamp(-1.0, 1.0).asin();
                info!("test aid NVF_PLAY_TEST_GRAB: aimed at {:08X}, {len:.0} units away", a.aim.unwrap_or(0));
            }
        }
        // E must be held past the hold time before the turn starts.
        let lead = if a.activate { ACTIVATE_HOLD + 0.1 } else { 0.0 };
        let span = (a.hold - lead).max(0.01);
        let before = ((a.t - lead) / span).clamp(0.0, 1.0);
        a.t += dt;
        let after = ((a.t - lead) / span).clamp(0.0, 1.0);
        player.heading += (a.turn_deg * (after - before)).to_radians();
        if a.t >= a.hold {
            aid_release = true;
            aid_throw = a.throw;
            g.aid_now = None;
        }
    }

    let (eye, dir) = Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());

    // What could be held now.
    let found = if playing && g.held.is_none() { find_candidate(&mut phys, &sim, eye, dir) } else { Err("holding or not playing".into()) };
    g.candidate = found.as_ref().ok().copied();

    // A press of Activate on something holdable without an activation of its own (the
    // other presses are handed here by `activate::activate`), and the aid's press.
    let activate_down = input.pressed(Control::Activate) || g.aid_now.is_some_and(|a| a.activate);
    if let (Some(c), true) = (g.candidate, playing) {
        let pressed_e = (input.just_pressed(Control::Activate) && target.0.as_ref().is_none_or(|t| t.form_id != c.on.form_id())) || aid_press == Some(true);
        if pressed_e && g.tap.is_none() {
            let t = target.0.clone().filter(|t| t.form_id == c.on.form_id() && aid_press != Some(true));
            g.tap = Some(Tap { target: t, candidate: c, held: 0.0 });
        }
    }
    if aid_press == Some(true) && (g.candidate.is_none() || !playing) {
        info!("grab: nothing under the crosshair to pick up ({})", if playing { found.as_ref().err().map_or("", |e| e.as_str()) } else { "not playing" });
    }
    let mut start: Option<(Candidate, By)> = None;
    if let Some(tap) = g.tap.as_mut() {
        tap.held += dt;
        if !playing {
            g.tap = None;
        } else if activate_down && tap.held >= ACTIVATE_HOLD {
            let by = if g.aid_now.is_some() { By::Aid } else { By::Activate };
            start = Some((tap.candidate, by));
            g.tap = None;
        } else if !activate_down && tap.target.is_none() {
            g.tap = None;
        }
    }
    if g.held.is_none() && playing && (input.just_pressed(Control::Grab) || aid_press == Some(false)) {
        match g.candidate {
            Some(c) => start = Some((c, if aid_press.is_some() { By::Aid } else { By::GrabKey })),
            None => info!("grab: nothing under the crosshair to pick up ({})", found.as_ref().err().map_or("", |e| e.as_str())),
        }
    }
    if let Some((c, by)) = start {
        let id = c.on.form_id();
        let name = name_of(&sim, script.as_deref(), id);
        let mass = phys.world.grip_mass(c.on, c.at).unwrap_or(0.0);
        if mass > max_walk {
            info!("grab: {name} is too heavy to pick up ({mass:.1} kg, the limit {max_walk:.0})");
        } else if let Some(mass) = phys.world.grab(c.on, c.at, HoldParams::default()) {
            let walks = mass > max_run;
            let what = match c.on {
                GripOn::Ref(_) => "loose object",
                GripOn::Ragdoll(_) => "corpse",
            };
            info!(
                "grab: picked up {what} {name} by {:?}: {mass:.1} kg at {}, held {:.0} units from the eye{}",
                by,
                fmt3(c.at),
                hold_distance(c.distance),
                if walks { "; heavy, the player walks" } else { "" }
            );
            let body_from = body_at(&phys, c.on);
            g.held = Some(Held { on: c.on, name, by, mass, distance: hold_distance(c.distance), from: c.at, body_from, seconds: 0.0, walks, carry_logged: false });
            g.falling = None;
            tell_scripts(script.as_deref_mut(), &mut sim, Some(id));
        }
    }

    // Carry, or let go.
    let Some(h) = g.held.as_mut() else {
        watch_falling(g, &phys, dt);
        return;
    };
    h.seconds += dt;
    let want = [eye[0] + dir[0] * h.distance, eye[1] + dir[1] * h.distance, eye[2] + dir[2] * h.distance];
    phys.world.set_grip_target(want);
    let point = phys.world.grip_point();
    let key_down = match h.by {
        By::GrabKey => input.pressed(Control::Grab),
        By::Activate => input.pressed(Control::Activate),
        By::Aid => !aid_release,
    };
    let throw = (h.by != By::Aid && input.just_pressed(Control::Use)) || aid_throw;
    let gap = point.map(|p| dist(p, want));
    let why = if point.is_none() || phys.world.holding().is_none() {
        Some("it is gone")
    } else if !playing {
        Some("play stopped")
    } else if throw {
        Some("thrown")
    } else if !key_down {
        Some("let go")
    } else if gap.is_some_and(|d| d > MAX_GAP) {
        Some("it caught on something")
    } else {
        None
    };
    if let (false, true, Some(p)) = (h.carry_logged, h.seconds >= 0.5, point) {
        h.carry_logged = true;
        info!("grab: carrying {} at {}, {:.1} units from where it is held, {:.1} from where it was picked up", h.name, fmt3(p), gap.unwrap_or(0.0), dist(p, h.from));
    }
    let Some(why) = why else { return };
    let h = g.held.take().expect("held");
    let velocity = throw.then(|| {
        let s = throw_speed(h.mass);
        [dir[0] * s, dir[1] * s, dir[2] * s]
    });
    phys.world.release_grip(velocity);
    let at = point.unwrap_or(h.from);
    info!(
        "grab: {} {} after {:.1} s ({why}) at {}, {:.1} units from where it was picked up{}",
        if throw { "threw" } else { "dropped" },
        h.name,
        h.seconds,
        fmt3(at),
        dist(at, h.from),
        velocity.map_or(String::new(), |_| format!(", at {:.0} units/s", throw_speed(h.mass)))
    );
    g.falling = Some(Falling { on: h.on, name: h.name, from: h.body_from, seconds: 0.0 });
    tell_scripts(script.as_deref_mut(), &mut sim, None);
}

/// Where a held thing's body is: a loose object's body, a corpse's trunk.
fn body_at(phys: &PhysState, on: GripOn) -> Option<[f32; 3]> {
    match on {
        GripOn::Ref(id) => phys.world.ref_bodies(id).first().map(|b| b.translation),
        GripOn::Ragdoll(id) => phys.world.ragdoll_center(id),
    }
}

/// Log where a dropped object comes to rest (at most 10 s after the drop).
fn watch_falling(g: &mut GrabState, phys: &PhysState, dt: f32) {
    let Some(f) = g.falling.as_mut() else { return };
    f.seconds += dt;
    let id = f.on.form_id();
    let rest = match f.on {
        GripOn::Ref(_) => phys.world.ref_bodies(id).first().is_none_or(|b| b.sleeping),
        GripOn::Ragdoll(_) => phys.world.ragdoll_asleep(id),
    };
    let at = body_at(phys, f.on);
    if rest || f.seconds > 10.0 {
        match at {
            Some(p) if rest => info!(
                "grab: {} came to rest at {}, {:.1} units from where it was picked up, {:.1} s after it was let go",
                f.name,
                fmt3(p),
                f.from.map_or(0.0, |q| dist(p, q)),
                f.seconds
            ),
            _ => info!("grab: {} not at rest {:.1} s after it was let go", f.name, f.seconds),
        }
        g.falling = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aid_reads_its_items() {
        let a = parse_aid("6:e:3:90:at=00104C10, 12:z:2:-45:throw,bad");
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].aim, a[1].aim), (Some(0x00104C10), None));
        assert!(a[0].activate && !a[0].throw && a[0].hold == 3.0 && a[0].turn_deg == 90.0);
        assert!(!a[1].activate && a[1].throw && a[1].turn_deg == -45.0 && a[1].at == 12.0);
    }

    #[test]
    fn hold_distance_and_throw_speed() {
        assert_eq!(hold_distance(20.0), MIN_HOLD);
        assert_eq!(hold_distance(80.0), 80.0);
        assert_eq!(hold_distance(400.0), REACH);
        assert_eq!(throw_speed(5.0), 500.0);
        assert_eq!(throw_speed(0.5), THROW_MAX_SPEED);
        assert_eq!(throw_speed(50.0), 50.0);
    }

    #[test]
    fn a_tap_waits_for_the_key_to_come_up() {
        let c = Candidate { on: GripOn::Ref(7), at: [0.0; 3], distance: 60.0 };
        let mut g = GrabState { candidate: Some(c), ..default() };
        assert!(g.holdable(7) && !g.holdable(8));
        // Nothing to activate: the grab ends it, not the activation.
        g.tap = Some(Tap { target: None, candidate: c, held: 0.1 });
        assert_eq!(g.released_tap(false), None);
        assert!(g.tap.is_some());
        let t = Targeted { form_id: 7, name: "Skull".into(), action: crate::activate::outcome::Action::Name, distance: 60.0, source: crate::activate::pick::Source::Bounds, crime: false };
        g.tap = Some(Tap { target: Some(t.clone()), candidate: c, held: 0.1 });
        assert_eq!(g.released_tap(true), None);
        assert!(g.tap.is_some());
        assert_eq!(g.released_tap(false), Some(t));
        assert!(g.tap.is_none());
    }
}
