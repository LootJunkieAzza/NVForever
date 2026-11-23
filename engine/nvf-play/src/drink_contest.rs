//! The drinking contest's screen (owner request 2026-11-11): the contest itself is
//! `fnv_game::drink_contest` (the circles, drinks, pushes, Endurance, NPC drinkers, all
//! deterministic from a seed); this is what a player sees and does, in the casino games'
//! HUD [design; the original game has no drinking contest].
//!
//! - **Starting** ([`DrinkStart`]): sent by drink-b's stake page (`drink_coop`), alone or
//!   in co-op on every machine with the same seed and drinkers, once the bottle prop
//!   (drink-c) is activated. The screen opens at once ([`DrinkScreen`]). The world goes on
//!   under it (the drinkers' idles, a pass-out's fall: drink-c); the player's moving,
//!   looking, fighting, Pip-Boy and sneaking are held while it is open, as a script's
//!   DisablePlayerControls would ([`LOCK_KEY`]). [`npc_drinker`] and [`player_drinker`]
//!   make the drinkers, Endurance from their records.
//! - **Ticks**: the contest runs whole ticks at `TICK_HZ` from aid time, catching up when
//!   frames are slow. **Co-op** ([`DrinkLink`], filled by drink-b): every screen runs the
//!   whole contest from the seed; the host's ([`DrinkLink::authority`]) decides who passes
//!   out and who wins, the guests' copies are followers (`Contest::follower`) that take
//!   the host's places ([`SlotSnap`]), pass-outs and winner. The other players' steering
//!   comes from [`DrinkLink::steer`]; this player's goes out in [`DrinkLink::my_steer`].
//! - **Steering**: the mouse (moving it [`MOUSE_FULL`] interface units a second is full
//!   speed), the left stick, or WASD / the arrow keys; the strongest wins. Tab gets up
//!   (passing out at once: a forfeit).
//! - **The HUD**: the target ring and the small circle (centred low in the view, over the
//!   soft backdrop), the next drink's ring faintly in its last [`WARN`] seconds, one
//!   prompt under them ("Keep it centered", "Get back in!" while outside, the countdown);
//!   the roster in the score panel: each drinker's name and drinks (owner, 2026-11-12).
//! - **The end**: for a win the highlight (`flourish::Highlight::DrinkContest`, the
//!   Endurance picture) first, then the leaderboard (last one standing first, then in
//!   reverse order of passing out); [`RESULT_HOLD`] after it the screen closes and
//!   [`DrinkEnded`] says who won, on every screen (the passed out too). The winner's body
//!   dances (drink-c, on [`DrinkEvent`]'s winner; the owner 2026-11-11: something silly,
//!   not the cheer).
//! - **Events** ([`DrinkEvent`]): every drink, push, pass-out and the winner, for the
//!   animations (drink-c) and the payout (drink-b; this screen moves no caps).
//!
//! Log lines start `drink contest:`.

use crate::casino_hud::{CasinoFaces, Place, ScoreRow};
use crate::pause::AidTime;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use fnv_game::drink_contest::{self as dc, Contest, Drinker, Event, Who, Why};
use fnv_sim::FormId;

/// Seconds the result shows before the screen closes [design, as arm wrestling].
pub const RESULT_HOLD: f32 = 3.0;
/// Interface units a second of mouse movement that steer at full speed [design].
pub const MOUSE_FULL: f32 = 420.0;
/// Seconds before a drink that its ring shows [design].
pub const WARN: f32 = 2.0;
/// Most ticks run in one frame (a long stall is not caught up all at once).
const MAX_CATCH_UP: u32 = 10;
/// A local slot further than this from the host's place is put where the host has it;
/// nearer, this game's own steering stands [design, drink-b's proposal].
pub const SNAP_HARD: f32 = 0.15;

/// The game's control groups held while the contest is open (`Game::controls_locked`
/// reads every entry of this map): movement, Pip-Boy, fighting, POV (left free),
/// looking, rollover, sneaking.
pub const LOCK_KEY: &str = "nvforever:drink_contest";
const LOCKS: [bool; 7] = [true, true, true, false, true, true, true];

/// Start a contest (see the module notes). This player's slot: [`DrinkLink::local`] in
/// co-op, else the first player among `drinkers`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct DrinkStart {
    /// The bottle's reference (the contest's key).
    pub bottle: u32,
    pub drinkers: Vec<Drinker>,
    pub seed: u64,
    pub stake: u32,
}

/// Something that happened in the contest at `bottle`, at contest tick `tick`.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DrinkEvent {
    pub bottle: u32,
    pub tick: u32,
    pub event: Event,
}

/// The screen closed: who won (`None` a draw or nobody), whether it was this player,
/// and the pot.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrinkEnded {
    pub bottle: u32,
    pub winner: Option<Who>,
    pub won: bool,
    pub pot: u32,
}

/// One slot as the host last saw it (co-op).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SlotSnap {
    pub pos: [f32; 2],
    pub drift: [f32; 2],
    pub radius: f32,
    /// The tick it passed out.
    pub out: Option<u32>,
    /// Why, when `out` is set (the host's reason, so every screen names the same one).
    pub why: Why,
    /// Drinks taken: the host's count, which the end's snapshot settles on every screen.
    pub step: u32,
}

/// The co-op hook (drink-b fills it; the default, off, is one player alone).
#[derive(Resource, Debug, Clone, Default)]
pub struct DrinkLink {
    pub on: bool,
    /// This game decides (the host).
    pub authority: bool,
    /// The slot this machine's keys steer.
    pub local: Option<usize>,
    /// Per slot: steer it with this (the other players' latest).
    pub steer: Vec<Option<[f32; 2]>>,
    /// The host's state at its tick; the screen takes it.
    pub snap: Option<(u32, Vec<SlotSnap>)>,
    /// The host settled it (followers): the screen shows the winner.
    pub winner: Option<Option<usize>>,
    /// Slots whose player left or dropped: the deciding screen passes them out.
    pub drop_slot: Vec<usize>,
    /// This player's steering, written by the screen every frame.
    pub my_steer: [f32; 2],
    /// This player gets up: set by the screen on Tab (a follower's link sends it to the
    /// host); set by a test aid, the screen takes it as Tab.
    pub leave: bool,
    /// Test aid: the screen steers the local slot back to the centre, this well (0..1).
    pub aid_skill: Option<f32>,
}

/// Where the screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The countdown and the drinking.
    Play,
    /// Settled: the highlight, then the leaderboard.
    Result,
}

/// The contest on this screen.
#[derive(Debug, Clone)]
pub struct Drink {
    pub bottle: u32,
    pub contest: Contest,
    /// This player's slot (none: watching).
    pub me: Option<usize>,
    pub page: Page,
    /// Aid seconds the contest started (its tick 0).
    pub started: f32,
    /// Aid seconds it was settled.
    pub ended: Option<f32>,
    /// Events still to be shown.
    pub inbox: Vec<Event>,
    /// This player's steering now (each axis -1..1, y up).
    pub steer: [f32; 2],
    /// This player asked to get up (once).
    pub want_leave: bool,
    /// Aid seconds this player's last drink came (the ring's flash).
    pub drank_at: Option<f32>,
}

impl Drink {
    pub fn new(s: &DrinkStart, link: &DrinkLink, now: f32) -> Drink {
        let mut contest = Contest::start(s.drinkers.clone(), s.seed, s.stake);
        contest.follower = link.on && !link.authority;
        let me = if link.on { link.local } else { contest.drinkers.iter().position(|d| matches!(d.who, Who::Player(_))) };
        Drink { bottle: s.bottle, contest, me, page: Page::Play, started: now, ended: None, inbox: Vec::new(), steer: [0.0; 2], want_leave: false, drank_at: None }
    }

    /// Whether this player won, once settled.
    pub fn won(&self) -> Option<bool> {
        self.contest.winner.map(|w| w.is_some() && w == self.me)
    }

    fn name(&self, slot: usize) -> String {
        if Some(slot) == self.me {
            "You".into()
        } else {
            self.contest.drinkers.get(slot).map_or_else(String::new, |d| d.name.clone())
        }
    }
}

#[derive(Resource, Default)]
pub struct DrinkScreen(pub Option<Drink>);

impl DrinkScreen {
    /// This player's slot in the contest shown.
    pub fn me_slot(&self) -> Option<usize> {
        self.0.as_ref().and_then(|d| d.me)
    }
}

/// The leaderboard's order: the winner, then the rest by when they passed out, latest
/// first.
/// The drinks a slot finished: a drinker who passes out off centre does so on the drink
/// that shrank the target, and that drink is not counted (parity, 2026-11-12: "Count only
/// finished drinks"), so every screen shows the same count for the slot.
pub fn finished_drinks(s: &dc::Slot) -> u32 {
    match s.out {
        Some((_, Why::OffCentre)) => s.step.saturating_sub(1),
        _ => s.step,
    }
}

pub fn standings(c: &Contest) -> Vec<usize> {
    let mut order: Vec<usize> = (0..c.slots.len()).collect();
    let winner = c.winner.flatten();
    order.sort_by_key(|&i| (Some(i) != winner, std::cmp::Reverse(c.slots[i].out.map_or(u32::MAX, |o| o.0)), i));
    order
}

/// Seconds after the end before the leaderboard: the highlight plays out first after a
/// win [owner 2026-11-11, as arm wrestling].
pub fn board_after(won: bool) -> f32 {
    if won {
        crate::flourish::LIFE
    } else {
        0.0
    }
}

/// What keeps an actor near the table out of a drinking contest. Only people drink
/// (owner, 2026-11-12: "it lets cheyenne drink wtf"): no creature (its base, through its
/// template, is a CREA record [documented: the Tes4Mod NPC_/CREA record pages]: dogs,
/// robots, eyebots), no child (its race's Child flag, RACE DATA [documented: catalogue
/// summary of IsChild]), and nobody dead, out cold (knocked out or set unconscious) or in
/// a combat state (Alert, Combat or Lost [documented: GECK wiki Category:Detection]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fit {
    pub creature: bool,
    pub child: bool,
    pub dead: bool,
    pub out_cold: bool,
    pub fighting: bool,
}

impl Fit {
    pub fn may_drink(self) -> bool {
        !(self.creature || self.child || self.dead || self.out_cold || self.fighting)
    }
}

/// `r`'s [`Fit`] from the game: an actor with no actor record counts as no person.
pub fn fit_of(g: &fnv_game::Game, r: FormId) -> Fit {
    Fit {
        creature: fnv_game::nvse_nx::is_creature(g, r) != Some(false),
        child: g.actor_race(r).is_some_and(|race| g.data.child_races.contains(&race)),
        dead: g.is_dead(r),
        out_cold: g.is_out_cold(r),
        fighting: g.awareness.state(r).in_combat(),
    }
}

/// The `n` nearest actors of `near` (each with its place and [`Fit`]) that may drink,
/// nearest first on the floor (distance across, not up), never the player.
pub fn nearest_drinkers(near: Vec<(FormId, [f32; 3], Fit)>, me: [f32; 3], n: usize) -> Vec<FormId> {
    let d2 = |p: &[f32; 3]| (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
    let mut ok: Vec<_> = near.into_iter().filter(|(r, _, f)| *r != FormId::PLAYER_REF && f.may_drink()).collect();
    ok.sort_by(|a, b| d2(&a.1).total_cmp(&d2(&b.1)));
    ok.into_iter().take(n).map(|(r, _, _)| r).collect()
}

/// `drinkers` without the NPCs whose [`Fit`] (`fit`, by reference id) keeps them out.
pub fn keep_people(drinkers: Vec<Drinker>, fit: impl Fn(u32) -> Fit) -> Vec<Drinker> {
    drinkers
        .into_iter()
        .filter(|d| match d.who {
            Who::Npc(r) => {
                let f = fit(r);
                if !f.may_drink() {
                    info!("drink contest: {} ({r:08X}) may not drink ({f:?}): left out", d.name);
                }
                f.may_drink()
            }
            _ => true,
        })
        .collect()
}

/// The actors near the player in the loaded cells, each with its [`Fit`].
pub fn near_with_fit(g: &fnv_game::Game) -> Vec<(FormId, [f32; 3], Fit)> {
    crate::compass::actors_near(g, &Default::default()).into_iter().map(|(r, p)| (r, p, fit_of(g, r))).collect()
}

/// An NPC drinker: its name and Endurance from its record.
pub fn npc_drinker(rt: &mut crate::script::Runtime, sim: &mut SimState, r: FormId) -> Drinker {
    let endurance = with_world(rt, sim, |rt| fnv_game::functions::actor_value(&mut rt.game, r, fnv_game::actor_values::ENDURANCE).unwrap_or(5.0));
    let name = sim.ref_name(Some(&rt.game), r.0).map(str::to_string).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{:08X}", r.0));
    Drinker { who: Who::Npc(r.0), name, endurance: endurance.round().clamp(1.0, 10.0) as u8 }
}

/// This player as a drinker (`id` the co-op id, 0 alone).
pub fn player_drinker(rt: &mut crate::script::Runtime, sim: &mut SimState, id: u8, name: &str) -> Drinker {
    let endurance = with_world(rt, sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::ENDURANCE).unwrap_or(5.0));
    Drinker { who: Who::Player(id), name: name.into(), endurance: endurance.round().clamp(1.0, 10.0) as u8 }
}

#[derive(Component)]
struct DrinkRoot;

pub struct DrinkContestPlugin;

impl Plugin for DrinkContestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DrinkScreen>()
            .init_resource::<DrinkLink>()
            .add_message::<DrinkStart>()
            .add_message::<DrinkEvent>()
            .add_message::<DrinkEnded>()
            .add_systems(Update, (test_aid, open).chain().after(PlaySet::Activation).before(PlaySet::Ui))
            .add_systems(Update, (steer, step, show_events, draw, close).chain().in_set(PlaySet::Ui))
            .add_systems(Update, test_shots.after(PlaySet::Ui));
    }
}

fn open(mut starts: MessageReader<DrinkStart>, mut screen: ResMut<DrinkScreen>, time: AidTime, link: Res<DrinkLink>, mut script: Option<ResMut<ScriptState>>) {
    for s in starts.read() {
        if screen.0.is_some() {
            warn!("drink contest: a contest is already on; the start at {:08X} is not taken", s.bottle);
            continue;
        }
        // The last gate: an NPC drinker who may not drink ([`Fit`]) is left out, whatever
        // started the contest. Alone only: a co-op contest's list is the deciding game's,
        // the same slots on every screen, and holds no NPCs.
        let mut s = s.clone();
        if !link.on {
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                let g = &rt.game;
                s.drinkers = keep_people(std::mem::take(&mut s.drinkers), |r| fit_of(g, FormId(r)));
            }
        }
        let s = &s;
        let d = Drink::new(s, &link, time.elapsed_secs());
        let who: Vec<String> = d.contest.drinkers.iter().map(|x| format!("{} (Endurance {})", x.name, x.endurance)).collect();
        let role = match (link.on, link.authority) {
            (false, _) => "",
            (true, true) => " (co-op, deciding)",
            (true, false) => " (co-op, following)",
        };
        info!("drink contest: starts at {:08X}, seed {}, stake {}, pot {}: {}; this player slot {:?}{role}", s.bottle, s.seed, s.stake, d.contest.pot(), who.join(", "), d.me);
        screen.0 = Some(d);
        if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
            rt.game.other.alt_locks.insert(LOCK_KEY.to_string(), LOCKS);
        }
    }
}

/// This player's steering: mouse, stick or keys, the strongest.
#[allow(clippy::too_many_arguments)]
fn steer(
    input: Res<ButtonInput<KeyCode>>,
    (mode, ask): (crate::bindings::ModeInput, Option<ResMut<crate::bindings::AskTake>>),
    motion: Res<AccumulatedMouseMotion>,
    pads: crate::pad::ActionPads,
    real: Res<Time<Real>>,
    windows: Query<&Window>,
    state: Res<State<PlayState>>,
    mut screen: ResMut<DrinkScreen>,
    mut link: ResMut<DrinkLink>,
    mut smooth: Local<Vec2>,
) {
    let Some(d) = screen.0.as_mut() else { return };
    let playing = *state.get() == PlayState::Playing;
    // Leave's key and button are the contest's while it runs (B is the Pip-Boy's in play).
    if let Some(mut a) = ask.filter(|_| playing && d.page == Page::Play) {
        a.take(&[crate::bindings::Control::GameLeave]);
    }
    let tab = playing && mode.just_pressed(crate::bindings::Control::GameLeave);
    if (tab || link.leave) && d.page == Page::Play && !d.want_leave && d.me.is_some_and(|i| d.contest.slots[i].out.is_none()) {
        d.want_leave = true;
        link.leave = true;
    }
    let dt = real.delta_secs().max(1e-3);
    // The mouse in interface units (the view is 960 high), y up; nothing while a menu is
    // over the game (the pause menu's cursor).
    let scale = windows.iter().next().map_or(1.0, |w| 960.0 / w.height().max(1.0));
    let m = if playing { Vec2::new(motion.delta.x, -motion.delta.y) * scale / (dt * MOUSE_FULL) } else { Vec2::ZERO };
    // A little smoothing: a mouse's counts come unevenly from frame to frame.
    *smooth = smooth.lerp(m.clamp_length_max(1.0), (dt * 20.0).min(1.0));
    let stick = if playing { crate::pad::stick(&pads, true) } else { Vec2::ZERO };
    let key = |a: KeyCode, b: KeyCode| if playing && (input.pressed(a) || input.pressed(b)) { 1.0 } else { 0.0 };
    let keys = Vec2::new(key(KeyCode::KeyD, KeyCode::ArrowRight) - key(KeyCode::KeyA, KeyCode::ArrowLeft), key(KeyCode::KeyW, KeyCode::ArrowUp) - key(KeyCode::KeyS, KeyCode::ArrowDown)).clamp_length_max(1.0);
    let mut v = [*smooth, stick, keys].into_iter().max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec2::ZERO);
    let mine = d.me.and_then(|i| d.contest.slots.get(i)).map(|s| Vec2::new(s.pos[0], s.pos[1]));
    if let (Some(k), Some(p)) = (link.aid_skill, mine) {
        v = (-p * 4.0 * k).clamp_length_max(1.0);
    } else if let Some(a) = aid() {
        v = match (a.mode, mine) {
            (AidMode::Bot, Some(p)) => (-p * 4.0).clamp_length_max(1.0),
            (AidMode::Drift, _) => Vec2::new(0.3, 0.0),
            _ => v,
        };
    }
    d.steer = [v.x, v.y];
    link.my_steer = d.steer;
}

/// The contest's ticks.
fn step(time: AidTime, mut screen: ResMut<DrinkScreen>, mut link: ResMut<DrinkLink>) {
    let Some(d) = screen.0.as_mut() else { return };
    let deciding = !link.on || link.authority;
    if d.want_leave && deciding && d.me.is_some_and(|i| d.contest.slots[i].out.is_none()) {
        link.leave = false;
        if let Some(me) = d.me {
            info!("drink contest: this player gets up; passes out (forfeit)");
            let ev = d.contest.leave(me);
            d.inbox.extend(ev);
        }
    }
    if link.on {
        if deciding {
            for slot in std::mem::take(&mut link.drop_slot) {
                info!("drink contest: slot {slot} left the contest; passes out");
                let ev = d.contest.leave(slot);
                d.inbox.extend(ev);
            }
        } else {
            link.drop_slot.clear();
            follow(d, &mut link);
        }
    }
    let due = ((time.elapsed_secs() - d.started).max(0.0) * dc::TICK_HZ as f32) as u32;
    let mut steer = vec![[0.0; 2]; d.contest.slots.len()];
    for (i, s) in steer.iter_mut().enumerate() {
        if Some(i) == d.me {
            *s = d.steer;
        } else if let Some(Some(v)) = link.steer.get(i) {
            *s = *v;
        }
    }
    let mut n = 0;
    while d.contest.winner.is_none() && d.contest.tick < due && n < MAX_CATCH_UP {
        let ev = d.contest.tick(&steer);
        d.inbox.extend(ev);
        n += 1;
    }
    // Behind by more than the catch-up: the clock moves on, the contest does not jump.
    if d.contest.tick + MAX_CATCH_UP < due {
        d.started += (due - d.contest.tick - MAX_CATCH_UP) as f32 * dc::DT;
    }
}

/// A follower's copy takes the host's places, pass-outs and winner.
fn follow(d: &mut Drink, link: &mut DrinkLink) {
    // The end's snapshot comes with the winner: a follower's own clock may have poured a
    // drink after the host's last tick (the view's lag), so the host's counts stand.
    let settled = link.winner.is_some();
    if let Some((_, snaps)) = link.snap.take() {
        for (i, (s, n)) in d.contest.slots.iter_mut().zip(&snaps).enumerate() {
            let off = (s.pos[0] - n.pos[0]).hypot(s.pos[1] - n.pos[1]);
            if Some(i) != d.me || off > SNAP_HARD {
                s.pos = n.pos;
                s.drift = n.drift;
            }
            s.radius = n.radius;
            if settled {
                s.step = n.step;
            }
        }
        for (i, n) in snaps.iter().enumerate() {
            if let Some(e) = n.out.and_then(|t| d.contest.mark_out(i, t, n.why)) {
                d.inbox.push(e);
            }
        }
    }
    if let Some(e) = link.winner.and_then(|w| d.contest.settle_as(w)) {
        d.inbox.push(e);
    }
}

/// The events: logged, sent on, and the screen's own reactions (the ring's flash, the
/// result, the highlight).
fn show_events(time: AidTime, mut screen: ResMut<DrinkScreen>, mut out: MessageWriter<DrinkEvent>, mut flourish: MessageWriter<crate::flourish::Flourish>) {
    let Some(d) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let tick = d.contest.tick;
    for e in std::mem::take(&mut d.inbox) {
        out.write(DrinkEvent { bottle: d.bottle, tick, event: e });
        match e {
            Event::Shrink { slot, step, radius } => {
                info!("drink contest: {} takes drink {step}, target {radius:.2} (tick {tick})", d.name(slot));
                if Some(slot) == d.me {
                    d.drank_at = Some(now);
                }
            }
            Event::Push { .. } => {}
            Event::PassedOut { slot, why } => {
                let why = match why {
                    Why::Outside => "outside too long",
                    Why::OffCentre => "off centre at a drink",
                    Why::Left => "got up",
                };
                info!("drink contest: {} passes out ({why}) at {:.1}s, drinks {}", d.name(slot), tick as f32 * dc::DT, d.contest.slots.get(slot).map_or(0, finished_drinks));
            }
            Event::Winner { slot, pot } => {
                let won = slot.is_some() && slot == d.me;
                let who = |s: usize| format!("slot {s} ({}) wins{}", d.contest.drinkers.get(s).map_or("", |x| x.name.as_str()), if won { ", this player" } else { "" });
                info!("drink contest: {} after {:.1}s, pot {pot}", slot.map_or_else(|| "a draw".to_string(), who), tick as f32 * dc::DT);
                d.page = Page::Result;
                d.ended = Some(now);
                if won {
                    flourish.write(crate::flourish::Flourish { kind: crate::flourish::Highlight::DrinkContest, text: "YOU WIN".into() });
                }
            }
        }
    }
}

/// The result has shown: the screen closes and says who won, on every screen.
fn close(time: AidTime, mut screen: ResMut<DrinkScreen>, mut script: Option<ResMut<ScriptState>>, mut ended: MessageWriter<DrinkEnded>) {
    let Some(d) = screen.0.as_ref() else { return };
    let Some(at) = d.ended else { return };
    let won = d.won() == Some(true);
    if time.elapsed_secs() - at < board_after(won) + RESULT_HOLD {
        return;
    }
    let winner = d.contest.winner.flatten().and_then(|s| d.contest.drinkers.get(s)).map(|x| x.who);
    ended.write(DrinkEnded { bottle: d.bottle, winner, won, pot: d.contest.pot() });
    info!("drink contest: closed");
    screen.0 = None;
    if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
        rt.game.other.alt_locks.remove(LOCK_KEY);
    }
}

/// The circle's place and size, interface units: its centre from the view's top, and
/// the first target's radius on screen. Low in the view, so the drinkers at the bar stay
/// in sight above it, and above the HUD's bars (their tops at about 800) [design].
const CIRCLE_TOP: f32 = 560.0;
const SCALE: f32 = 130.0;
const PROMPT_BASE: f32 = 735.0;

fn draw(mut commands: Commands, screen: Res<DrinkScreen>, time: AidTime, casino: Option<Res<CasinoFaces>>, old: Query<Entity, With<DrinkRoot>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let Some(d) = screen.0.as_ref() else { return };
    let now = time.elapsed_secs();
    let plain;
    let f = match casino.as_deref() {
        Some(f) => f,
        None => {
            plain = CasinoFaces::plain();
            &plain
        }
    };
    let c = &d.contest;
    let root = commands.spawn((DrinkRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(25), Pickable::IGNORE)).id();
    commands.entity(root).with_children(|p| {
        let rows: Vec<ScoreRow> = (0..c.slots.len())
            .map(|i| {
                // Names and drinks only (owner, 2026-11-12: "The hud in the left should just
                // say names and number of drinks").
                let s = &c.slots[i];
                let n = finished_drinks(s);
                ScoreRow { name: d.name(i), score: format!("{n} {}", if n == 1 { "drink" } else { "drinks" }), note: String::new(), up: Some(i) == d.me && s.out.is_none() }
            })
            .collect();
        // The leaderboard says it all once it shows; the roster under it would cross it
        // on a narrow view (1280x800 run, drink_board.png).
        let board = d.page == Page::Result && d.ended.is_some_and(|e| now - e >= board_after(d.won() == Some(true)));
        if !board {
            crate::casino_hud::score_panel(p, f, &rows, None);
        }
        match d.page {
            Page::Play => {
                let Some(me) = d.me.and_then(|i| c.slots.get(i)) else { return };
                let prompt = if c.tick < dc::COUNTDOWN {
                    format!("{}", c.countdown_left().ceil() as i32)
                } else if me.out.is_some() {
                    "You passed out".into()
                } else if me.outside > 0 {
                    "Get back in!".into()
                } else {
                    "Keep it centered".into()
                };
                let warn = me.out.is_none() && c.tick >= dc::COUNTDOWN && (me.drink_in(c.tick) as f32 * dc::DT) < WARN;
                let flash = d.drank_at.is_some_and(|t| now - t < 0.4);
                circles(p, f, me, warn, flash, &prompt, now);
                crate::casino_hud::help_line(p, f, &format!("Mouse, left stick or WASD: steer    {}: give up", crate::bindings::mode_key(crate::bindings::Control::GameLeave)));
            }
            Page::Result => {
                let won = d.won() == Some(true);
                if !d.ended.is_some_and(|e| now - e >= board_after(won)) {
                    return;
                }
                let places: Vec<Place> = standings(c)
                    .iter()
                    .map(|&i| {
                        let s = &c.slots[i];
                        let detail = match s.out {
                            None => format!("{} drinks", finished_drinks(s)),
                            Some((t, _)) => format!("out at {:.0}s", t as f32 * dc::DT),
                        };
                        Place { name: d.name(i), detail }
                    })
                    .collect();
                let title = c.winner.flatten().map_or_else(|| "Nobody".to_string(), |w| d.name(w));
                crate::casino_hud::leaderboard(p, f, &title, &places);
            }
        }
    });
}

/// A ring of radius `r` interface units centred at (x, y) from the parent's corner.
fn ring(p: &mut ChildSpawnerCommands, x: f32, y: f32, r: f32, width: f32, color: Color) {
    use crate::interface::u;
    p.spawn((
        Node { position_type: PositionType::Absolute, left: u(x - r), top: u(y - r), width: u(2.0 * r), height: u(2.0 * r), border: UiRect::all(u(width)), border_radius: BorderRadius::MAX, ..default() },
        BorderColor::all(color),
    ));
}

fn disc(p: &mut ChildSpawnerCommands, x: f32, y: f32, r: f32, color: Color) {
    use crate::interface::u;
    p.spawn((Node { position_type: PositionType::Absolute, left: u(x - r), top: u(y - r), width: u(2.0 * r), height: u(2.0 * r), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(color)));
}

/// The target, the small circle, the next drink's ring, and the prompt under them.
fn circles(p: &mut ChildSpawnerCommands, f: &CasinoFaces, s: &dc::Slot, warn: bool, flash: bool, prompt: &str, now: f32) {
    use crate::interface::{amber, u};
    let half = SCALE * 1.15;
    let pw = crate::casino_hud::width(f, 2, prompt);
    let box_w = (half * 2.0).max(pw + 40.0);
    let box_h = PROMPT_BASE - (CIRCLE_TOP - half) + 20.0;
    p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(-box_w / 2.0)), top: u(CIRCLE_TOP - half), width: u(box_w), height: u(box_h), ..default() }).with_children(|b| {
        crate::casino_hud::glass(b, 0.0, 0.0, box_w, box_h);
        let (cx, cy) = (box_w / 2.0, half);
        // The first target's size, faintly: how far it has shrunk.
        ring(b, cx, cy, SCALE, 1.0, amber(255.0).with_alpha(0.18));
        let blink = s.outside > 0 && (now * 6.0).fract() < 0.5;
        let target = if flash || blink { amber(255.0) } else { amber(220.0) };
        ring(b, cx, cy, s.radius * SCALE, if flash { 4.0 } else { 3.0 }, target);
        if warn {
            let next = dc::radius(s.step + 1);
            if next < s.radius {
                ring(b, cx, cy, next * SCALE, 1.5, amber(255.0).with_alpha(0.45));
            }
        }
        // A small cross at the centre to aim for.
        b.spawn((Node { position_type: PositionType::Absolute, left: u(cx - 6.0), top: u(cy - 0.5), width: u(12.0), height: u(1.0), ..default() }, BackgroundColor(amber(255.0).with_alpha(0.5))));
        b.spawn((Node { position_type: PositionType::Absolute, left: u(cx - 0.5), top: u(cy - 6.0), width: u(1.0), height: u(12.0), ..default() }, BackgroundColor(amber(255.0).with_alpha(0.5))));
        let (bx, by) = (cx + s.pos[0] * SCALE, cy - s.pos[1] * SCALE);
        let alpha = if s.out.is_some() { 0.3 } else { 0.85 };
        disc(b, bx, by, dc::BALL * SCALE, amber(255.0).with_alpha(alpha));
        crate::casino_hud::line(b, f, 2, prompt, (box_w - pw) / 2.0, PROMPT_BASE - (CIRCLE_TOP - half), amber(255.0));
    });
}

// ---- the test aid ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AidMode {
    /// The player steers back to the centre at once (wins against most NPCs).
    Bot,
    /// The player drifts off to the right (passes out).
    Drift,
    /// The player's own input.
    Hands,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Aid {
    at: f32,
    seed: u64,
    npcs: usize,
    mode: AidMode,
}

/// `NVF_PLAY_TEST_DRINK=<seconds>:<seed>[:<NPCs>[:bot|drift|hands]]`: that long after the
/// start, a contest against the nearest NPCs (2 by default) of the loaded cells, this
/// player steered by the aid (`bot` by default). A test aid for the smoke scenario; not a
/// game behaviour.
fn aid() -> Option<Aid> {
    static AID: std::sync::OnceLock<Option<Aid>> = std::sync::OnceLock::new();
    *AID.get_or_init(|| std::env::var("NVF_PLAY_TEST_DRINK").ok().and_then(|v| parse_aid(&v)))
}

fn parse_aid(v: &str) -> Option<Aid> {
    let mut it = v.trim().split(':');
    let at = it.next()?.trim().parse().ok()?;
    let seed = it.next()?.trim().parse().ok()?;
    let npcs = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(2);
    let mode = match it.next().map(|s| s.trim().to_ascii_lowercase()) {
        Some(m) if m == "drift" => AidMode::Drift,
        Some(m) if m == "hands" => AidMode::Hands,
        _ => AidMode::Bot,
    };
    Some(Aid { at, seed, npcs, mode })
}

fn test_aid(time: AidTime, mut done: Local<bool>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut start: MessageWriter<DrinkStart>, state: Res<State<PlayState>>) {
    if *done || *state.get() != PlayState::Playing {
        return;
    }
    let Some(a) = aid() else {
        *done = true;
        return;
    };
    if time.world_secs().is_none_or(|t| t < a.at) {
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    *done = true;
    let me = sim.world.peek(FormId::PLAYER_REF).map_or([0.0; 3], |s| s.pos);
    let near = with_world(rt, &mut sim, |rt| near_with_fit(&rt.game));
    let mut drinkers = vec![player_drinker(rt, &mut sim, 0, "You")];
    for r in nearest_drinkers(near, me, a.npcs) {
        drinkers.push(npc_drinker(rt, &mut sim, r));
    }
    info!("drink contest: test aid starts a contest ({:?}) with {} NPCs", a.mode, drinkers.len() - 1);
    start.write(DrinkStart { bottle: 0, drinkers, seed: a.seed, stake: 25 });
}

/// `NVF_PLAY_TEST_DRINK_SHOTS=<folder>`: a screenshot shortly after each of this player's
/// drinks (`drink_shrink_<n>.png`), each pass-out (`drink_out_<slot>.png`), mid-highlight
/// (`drink_win.png`) and on the leaderboard (`drink_board.png`), and after the screen
/// closed (`drink_end.png`, the winner's dance). A test aid.
fn test_shots(time: AidTime, mut events: MessageReader<DrinkEvent>, mut ended: MessageReader<DrinkEnded>, screen: Res<DrinkScreen>, mut due: Local<Vec<(f32, String)>>, mut commands: Commands) {
    let Some(dir) = std::env::var_os("NVF_PLAY_TEST_DRINK_SHOTS").map(std::path::PathBuf::from) else {
        events.clear();
        ended.clear();
        return;
    };
    let now = time.elapsed_secs();
    let me = screen.me_slot();
    for e in events.read() {
        match e.event {
            Event::Shrink { slot, step, .. } if Some(slot) == me => due.push((now + 0.5, format!("drink_shrink_{step}.png"))),
            Event::PassedOut { slot, .. } => due.push((now + 0.3, format!("drink_out_{slot}.png"))),
            Event::Winner { slot, .. } => {
                due.push((now + 0.9, "drink_win.png".into()));
                let won = slot.is_some() && slot == me;
                due.push((now + board_after(won) + 0.8, "drink_board.png".into()));
            }
            _ => {}
        }
    }
    for _ in ended.read() {
        due.push((now + 1.5, "drink_end.png".into()));
    }
    let _ = std::fs::create_dir_all(&dir);
    due.retain(|(t, name)| {
        if *t > now {
            return true;
        }
        let path = dir.join(name);
        info!("drink contest: screenshot {}", path.display());
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drinkers() -> Vec<Drinker> {
        vec![
            Drinker { who: Who::Player(0), name: "You".into(), endurance: 6 },
            Drinker { who: Who::Npc(0x104F08), name: "Settler".into(), endurance: 5 },
            Drinker { who: Who::Npc(0x104C7E), name: "Trudy".into(), endurance: 4 },
        ]
    }

    #[test]
    fn the_aid_reads_its_parts() {
        assert_eq!(parse_aid("6:42"), Some(Aid { at: 6.0, seed: 42, npcs: 2, mode: AidMode::Bot }));
        assert_eq!(parse_aid("6:42:1:drift"), Some(Aid { at: 6.0, seed: 42, npcs: 1, mode: AidMode::Drift }));
        assert_eq!(parse_aid("x"), None);
    }

    #[test]
    fn the_board_puts_the_last_one_standing_first() {
        let start = DrinkStart { bottle: 1, drinkers: drinkers(), seed: 5, stake: 10 };
        let mut d = Drink::new(&start, &DrinkLink::default(), 0.0);
        assert_eq!(d.me, Some(0));
        d.contest.tick = 100;
        d.contest.leave(2);
        d.contest.tick = 300;
        d.contest.leave(1);
        assert_eq!(d.contest.winner, Some(Some(0)));
        assert_eq!(standings(&d.contest), [0, 1, 2]);
        assert_eq!(d.won(), Some(true));
        assert_eq!(d.contest.pot(), 30);
        assert_eq!(board_after(true), crate::flourish::LIFE);
    }

    /// A dog, a robot and a child stand nearest the table; only the people drink, and
    /// never the dead, the out cold or the fighting.
    #[test]
    fn only_people_are_picked_to_drink() {
        let at = |x: f32| [x, 0.0, 0.0];
        let person = Fit::default();
        let near = vec![
            (FormId(0x10), at(10.0), Fit { creature: true, ..person }),
            (FormId(0x11), at(20.0), Fit { child: true, ..person }),
            (FormId(0x12), at(30.0), Fit { dead: true, ..person }),
            (FormId(0x13), at(40.0), Fit { out_cold: true, ..person }),
            (FormId(0x14), at(50.0), Fit { fighting: true, ..person }),
            (FormId(0x15), at(15.0), Fit { creature: true, fighting: true, ..person }),
            (FormId(0x20), at(90.0), person),
            (FormId::PLAYER_REF, at(0.0), person),
            (FormId(0x21), at(60.0), person),
            (FormId(0x22), at(200.0), person),
        ];
        assert_eq!(nearest_drinkers(near.clone(), at(0.0), 2), [FormId(0x21), FormId(0x20)]);
        assert_eq!(nearest_drinkers(near, at(0.0), 9), [FormId(0x21), FormId(0x20), FormId(0x22)]);
        assert!(person.may_drink());
    }

    /// Whatever starts a contest, a creature or a child among its drinkers is left out.
    #[test]
    fn a_start_keeps_only_people() {
        let mut list = drinkers();
        list.push(Drinker { who: Who::Npc(0x7), name: "Dog".into(), endurance: 5 });
        let kept = keep_people(list, |r| Fit { creature: r == 0x7, child: r == 0x104C7E, ..Fit::default() });
        let names: Vec<&str> = kept.iter().map(|d| d.name.as_str()).collect();
        assert!(!names.contains(&"Dog") && !names.contains(&"Trudy"), "{names:?}");
        assert!(names.contains(&"Settler"), "{names:?}");
        assert_eq!(kept.len(), drinkers().len() - 1);
    }

    /// The drink a drinker passes out on is not counted; one who leaves or slips outside
    /// keeps every drink taken.
    #[test]
    fn the_drink_passed_out_on_does_not_count() {
        let start = DrinkStart { bottle: 1, drinkers: drinkers(), seed: 5, stake: 10 };
        let link = DrinkLink { on: true, authority: true, local: Some(0), ..default() };
        let mut d = Drink::new(&start, &link, 0.0);
        let s = &mut d.contest.slots[0];
        s.step = 7;
        assert_eq!(finished_drinks(s), 7);
        s.out = Some((1221, Why::OffCentre));
        assert_eq!(finished_drinks(s), 6);
        s.out = Some((1221, Why::Outside));
        assert_eq!(finished_drinks(s), 7);
        s.step = 0;
        s.out = Some((1, Why::OffCentre));
        assert_eq!(finished_drinks(s), 0);
    }

    /// A guest's screen follows the host: its link's slot, a follower's contest, the
    /// host's places past the hard limit, its pass-outs and winner.
    #[test]
    fn a_guest_follows_the_host() {
        let start = DrinkStart { bottle: 1, drinkers: drinkers(), seed: 5, stake: 10 };
        let mut link = DrinkLink { on: true, authority: false, local: Some(1), ..default() };
        let mut d = Drink::new(&start, &link, 0.0);
        assert_eq!(d.me, Some(1));
        assert!(d.contest.follower);
        let snap = |x: f32, out| SlotSnap { pos: [x, 0.0], drift: [0.0; 2], radius: 0.85, out, why: Why::Outside, step: 3 };
        // Its own slot: near the host's place it keeps its own; slot 0 takes the host's.
        d.contest.slots[1].pos = [0.1, 0.0];
        let mut gone = snap(0.0, Some(79));
        gone.why = Why::OffCentre;
        link.snap = Some((80, vec![snap(0.3, None), snap(0.2, None), gone]));
        follow(&mut d, &mut link);
        assert_eq!(d.contest.slots[0].pos, [0.3, 0.0]);
        assert_eq!(d.contest.slots[1].pos, [0.1, 0.0]);
        assert_eq!(d.inbox, [Event::PassedOut { slot: 2, why: Why::OffCentre }]);
        link.snap = Some((81, vec![snap(0.3, None), snap(0.6, None), snap(0.0, Some(79))]));
        follow(&mut d, &mut link);
        assert_eq!(d.contest.slots[1].pos, [0.6, 0.0], "past the hard limit");
        assert_eq!(d.inbox.len(), 1, "a pass-out once");
        link.winner = Some(Some(0));
        follow(&mut d, &mut link);
        assert_eq!(d.inbox.last(), Some(&Event::Winner { slot: Some(0), pot: 30 }));
        assert_eq!(d.won(), Some(false));
    }

    /// A guest's own clock poured a drink after the host's last tick: the end's snapshot,
    /// which comes with the winner, puts the host's count back on every slot.
    #[test]
    fn the_end_takes_the_hosts_drink_counts() {
        let start = DrinkStart { bottle: 1, drinkers: drinkers(), seed: 5, stake: 10 };
        let mut link = DrinkLink { on: true, authority: false, local: Some(1), ..default() };
        let mut d = Drink::new(&start, &link, 0.0);
        let snap = |step, out| SlotSnap { pos: [0.0; 2], drift: [0.0; 2], radius: 0.4, out, why: Why::Outside, step };
        d.contest.slots[1].step = 7;
        // While it runs the guest keeps its own count.
        link.snap = Some((1100, vec![snap(6, None), snap(6, None), snap(6, None)]));
        follow(&mut d, &mut link);
        assert_eq!(d.contest.slots[1].step, 7);
        // The end: the host's counts.
        link.snap = Some((1159, vec![snap(6, Some(1159)), snap(6, None), snap(5, Some(900))]));
        link.winner = Some(Some(1));
        follow(&mut d, &mut link);
        assert_eq!(d.contest.slots.iter().map(|s| s.step).collect::<Vec<_>>(), [6, 6, 5]);
        assert_eq!(finished_drinks(&d.contest.slots[1]), 6);
    }
}
