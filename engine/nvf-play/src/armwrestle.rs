//! Arm wrestling at the Prospector Saloon (queue item 199, part A): a bout against the
//! Goodsprings Settler at a table NVForever places in the saloon, in the casino games'
//! HUD. The bout itself (the meter, taps, Strength) is `fnv_game::armwrestle`; the table,
//! its stools and the opponent's coming and going are `fnv_game::armwrestle_table`
//! (armwrestle-c) [design: owner 2026-11-10, item 199; the original game has no arm
//! wrestling].
//!
//! - **The table**: `Table02` with a `Stool01L` either side at the east end of the back
//!   hall, made by the game when the saloon loads (`armwrestle_table::ensure`); found here
//!   ([`ArmTable`]) through `armwrestle_table::find`.
//! - **Sitting down** on either stool (the furniture's own activation) opens the bout's
//!   card ([`Page::Offer`]). The other stool is the opponent's.
//! - **The opponent** (part A: one player alone): the Goodsprings Settler
//!   (`armwrestle_table::OPPONENT`). The game calls him over when a player sits at the
//!   table with nobody across and he is about; he walks there and sits through his AI. The
//!   card waits for him to sit ([`Foe`]); his Strength is his own. When he is not about
//!   (asleep, gone, in a fight) the card says nobody is free, and the player gets up.
//! - **The bout** ([`Page::Bout`]): a countdown, then the one tap button ([`TAP`]); the
//!   meter's lean is shown under one centred prompt. The opponent taps as
//!   `fnv_game::armwrestle::NpcTapper`.
//! - **The end** ([`Page::Result`]): for a win the Strength highlight
//!   (`flourish::Highlight::ArmWrestle`) first, then ([`board_after`]) the leaderboard;
//!   [`RESULT_HOLD`] after it the player gets
//!   up (against another player both at the winner's time, so they rise together), the game lets the Settler go back to his package, and the end view (nvforever
//!   `match_end`, item 166) shows the winner cheering.
//! - **Co-op** (part B, `crate::armwrestle_coop`): the match is the deciding game's (the
//!   host's). This screen sits the player down (`ArmSeat::at`, its side the stool's),
//!   sends its taps (`ArmSeat::taps`), asks for a bout against the opponent once he sits
//!   (`ArmSeat::want_npc`) and draws the bout it is sent (`ArmSeat::bout`) instead of
//!   running its own. Two players on the two stools wrestle each other.
//!
//! Log lines start `arm wrestle:`.

use crate::armwrestle_coop::{ArmBodies, ArmSeat, Wrestler, TABLE_KEY};
use crate::bindings::{mode_key, Control};
use crate::casino_hud::CasinoFaces;
use crate::coop_tables::Tables;
use crate::furniture::PlayerSeat;
use crate::pause::AidTime;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_game::armwrestle::{Bout, NpcTapper, Phase};
use fnv_game::armwrestle_table as at;
use fnv_sim::FormId;

/// The saloon (FalloutNV.esm 0x00106185) [observed].
pub const CELL: &str = at::SALOON_EDID;
/// The tap button [design: one button for everyone; jumping is off while seated].
/// The tap: Start / Next's binding (Space, or A on a controller).
pub const TAP: Control = Control::GameConfirm;
/// Seconds the result shows before the player gets up [design].
pub const RESULT_HOLD: f32 = 2.6;

/// Seconds after the bout's end before the leaderboard shows: after a win the YOU WIN
/// highlight plays out first (`flourish::LIFE`) [design: owner 2026-11-11, "leaderboard
/// should play after the you win anmimation"]; a loss has no highlight.
pub fn board_after(won: bool) -> f32 {
    if won {
        crate::flourish::LIFE
    } else {
        0.0
    }
}
/// Frames between looks for the table while in the saloon without it.
const LOOK_EVERY: u32 = 30;

/// The table and its stools, once found.
#[derive(Resource, Default, Debug, Clone)]
pub struct ArmTable {
    pub table: Option<FormId>,
    pub stools: [Option<FormId>; 2],
}

impl ArmTable {
    /// Which of the stools `r` is (0 or 1).
    pub fn stool_index(&self, r: u32) -> Option<usize> {
        self.stools.iter().position(|s| s.is_some_and(|s| s.0 == r))
    }
}

/// Where a bout is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Seated: waiting for the opponent, then start a bout or leave.
    Offer,
    /// Counting down or pulling.
    Bout,
    /// Settled: the result shows, then the player gets up.
    Result,
}

/// The opponent, as the card sees him.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foe {
    /// Sent to the other stool (or about to be), not yet on it.
    Coming,
    /// On the other stool.
    Seated,
    /// Not sent there and not about: nobody to wrestle.
    Away,
}

/// What the card shows of the opponent: `called` is the stool he was sent to (if any),
/// `seated` whether he sits on it, `available` whether he could be called.
pub fn foe(other: FormId, called: Option<FormId>, seated: bool, available: bool) -> Foe {
    match called {
        Some(s) if s == other && seated => Foe::Seated,
        Some(s) if s == other => Foe::Coming,
        // Not called yet (the game calls him once the player sits): coming while he is about.
        None if available => Foe::Coming,
        _ => Foe::Away,
    }
}

/// The bout at the table, while this player is seated at it.
#[derive(Debug, Clone)]
pub struct Arm {
    /// This player's stool and the opponent's.
    pub stool: FormId,
    pub other: FormId,
    pub page: Page,
    pub bout: Option<Bout>,
    /// The NPC's tapping (one player alone).
    pub npc: Option<NpcTapper>,
    /// The opponent's reference once he sits, his name and Strength.
    pub foe: Option<FormId>,
    pub foe_name: String,
    pub foe_strength: f32,
    /// The opponent is another player (its co-op id): both get up together, and its
    /// puppet reacts in the end view.
    pub foe_player: Option<u8>,
    pub foe_state: Foe,
    /// When he sat down (aid seconds).
    pub foe_seated_at: Option<f32>,
    /// This player's side (0 alone: whoever sat first).
    pub side: u8,
    pub player_strength: f32,
    /// Getting up was asked (the bout over or left).
    pub leaving: bool,
    pub left_at: Option<f32>,
    /// When the card opened (aid seconds).
    pub opened: f32,
}

impl Arm {
    /// The winner's side, once settled.
    pub fn winner(&self) -> Option<u8> {
        self.bout.as_ref().and_then(|b| b.winner)
    }

    /// Whether this player won, once settled.
    pub fn won(&self) -> Option<bool> {
        self.winner().map(|w| w == self.side)
    }
}

#[derive(Resource, Default)]
pub struct ArmScreen(pub Option<Arm>);

/// The end of a bout for the end view (nvforever `match_end`): the table, whether this
/// player won and the opponent's reference; written once the player is up.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmEnded {
    pub table: u32,
    pub won: bool,
    /// The NPC opponent's reference.
    pub opponent: Option<u32>,
    /// The other player's co-op id, when it was a player.
    pub peer: Option<u8>,
}

#[derive(Component)]
struct ArmRoot;

pub struct ArmWrestlePlugin;

impl Plugin for ArmWrestlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArmTable>()
            .init_resource::<ArmScreen>()
            .add_message::<ArmEnded>()
            .add_systems(Update, (look, open).chain().after(PlaySet::Activation).before(PlaySet::Ui))
            .add_systems(Update, (watch_foe, keys, step, draw).chain().in_set(PlaySet::Ui))
            .add_systems(Update, (leave, match_camera).chain().after(PlaySet::Ui));
    }
}

/// In the saloon: the table and stools the game made, found (looked for again while not
/// there, and when the ones found are gone, as after loading a save).
fn look(mut sim: ResMut<SimState>, mut script: Option<ResMut<ScriptState>>, mut table: ResMut<ArmTable>, mut frames: Local<u32>, mut was: Local<Option<FormId>>, mut bodies: ResMut<ArmBodies>) {
    let Some(cell) = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell) else { return };
    let here = |r: Option<FormId>| r.is_some_and(|r| sim.world.peek(r).is_some_and(|s| !s.deleted()));
    let fresh = *was != Some(cell);
    *was = Some(cell);
    if !fresh && here(table.table) && table.stools.iter().all(|s| here(*s)) {
        return;
    }
    *frames = frames.wrapping_add(1);
    if !fresh && !frames.is_multiple_of(LOOK_EVERY) {
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    if !rt.game.data.name(cell).eq_ignore_ascii_case(CELL) {
        if table.table.is_some() {
            *table = ArmTable::default();
        }
        return;
    }
    let found = with_world(rt, &mut sim, |rt| at::find(&rt.game));
    let t = found.map_or_else(ArmTable::default, |t| ArmTable { table: Some(t.table), stools: t.stools.map(Some) });
    if t.table != table.table {
        info!(
            "arm wrestle: the table {}: table {:?}, stools {:?}",
            if t.table.is_some() { "found" } else { "not there" },
            t.table.map(|r| format!("{:08X}", r.0)),
            t.stools.map(|s| s.map(|r| format!("{:08X}", r.0)))
        );
    }
    bodies.stools = t.stools.map(|s| s.map(|r| r.0));
    bodies.table = t.table.map(|_| TABLE_KEY);
    *table = t;
}

/// The compass heading (0 north, a quarter turn east: `crate::player::Player::heading`)
/// from `from` towards `to`. Sat down, the view looks across the table at the other side
/// [owner (via the lead, 2026-11-11): the guest's view looked along the table's edge,
/// north, its opponent out of view; laptop ship2 run].
pub fn facing(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[0] - from[0]).atan2(to[1] - from[1])
}

/// Seated on one of the stools: the card opens.
#[allow(clippy::too_many_arguments)]
fn open(
    table: Res<ArmTable>,
    seat: Res<PlayerSeat>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut screen: ResMut<ArmScreen>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut was: Local<Option<u32>>,
    time: AidTime,
    mut arm: ResMut<ArmSeat>,
    mut player: ResMut<crate::player::Player>,
) {
    let on = seat.on.filter(|_| seat.hold.is_some());
    // Sitting down during a menu is taken once play resumes: the preorder packs' message
    // boxes opened in the very frame the player sat, and the card never opened (smoke
    // armwrestle-solo, armwrestle-c's run 2026-11-12) [design].
    if *state.get() != PlayState::Playing {
        return;
    }
    let fresh = on != *was;
    *was = on;
    let Some(stool) = on.filter(|_| fresh) else { return };
    let Some(i) = table.stool_index(stool) else { return };
    if screen.0.is_some() {
        return;
    }
    let Some(other) = table.stools[1 - i] else { return };
    let player_strength = script
        .as_mut()
        .and_then(|s| s.rt.as_mut())
        .map(|rt| with_world(rt, &mut sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::STRENGTH).unwrap_or(5.0)))
        .unwrap_or(5.0) as f32;
    info!("arm wrestle: seated on stool {i} ({stool:08X}), Strength {player_strength}");
    // The view turns across the table, at the other side's hands; the pitch stays.
    // From the seat itself: while getting on the body is still at the entry, to the side.
    let at = |r: FormId| sim.world.peek(r).map(|x| x.pos).or_else(|| sim.world.placed.get(r).map(|p| p.pos));
    if let (Some(from), Some(to)) = (at(FormId(stool)), table.table.and_then(at)) {
        player.heading = facing(from, to);
        info!("arm wrestle: the view turns to the table, heading {:.0} deg", player.heading.to_degrees());
    }
    screen.0 = Some(Arm {
        stool: FormId(stool),
        other,
        page: Page::Offer,
        bout: None,
        npc: None,
        foe: None,
        foe_name: String::new(),
        foe_strength: 5.0,
        foe_player: None,
        foe_state: Foe::Coming,
        foe_seated_at: None,
        // In co-op the stool's side: the match is the table's, not this player's.
        side: if arm.coop { i as u8 } else { 0 },
        player_strength,
        leaving: false,
        left_at: None,
        opened: time.elapsed_secs(),
    });
    if arm.coop {
        arm.at = Some((TABLE_KEY, i as u8, player_strength.round().clamp(0.0, 255.0) as u8));
        arm.asked_npc = false;
    }
    next.set(PlayState::Menu);
}

/// On the card: whether the opponent is coming, seated or away.
fn watch_foe(time: AidTime, mut screen: ResMut<ArmScreen>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>) {
    let Some(a) = screen.0.as_mut().filter(|a| a.page == Page::Offer && !a.leaving) else { return };
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    let other = a.other;
    let (state, strength) = with_world(rt, &mut sim, |rt| {
        let g = &mut rt.game;
        let state = foe(other, at::called_to(g, at::OPPONENT), at::opponent_seated(g, at::OPPONENT), at::opponent_available(g).is_some());
        let strength = (state == Foe::Seated).then(|| at::opponent_strength(g, at::OPPONENT));
        (state, strength)
    });
    if a.foe_name.is_empty() {
        a.foe_name = sim.ref_name(Some(&rt.game), at::OPPONENT.0).map(str::to_string).unwrap_or_else(|| "The Settler".into());
    }
    if state == a.foe_state && (state != Foe::Seated || a.foe.is_some()) {
        return;
    }
    info!("arm wrestle: opponent {} ({:08X}) {state:?}", a.foe_name, at::OPPONENT.0);
    a.foe_state = state;
    if let Some(s) = strength {
        a.foe = Some(at::OPPONENT);
        a.foe_strength = s;
        a.foe_seated_at = Some(time.elapsed_secs());
    } else {
        a.foe = None;
        a.foe_seated_at = None;
    }
}

fn keys(input: crate::bindings::ModeInput, time: AidTime, mut screen: ResMut<ArmScreen>, mut arm: ResMut<ArmSeat>, mut aid_taps: Local<u32>) {
    let Some(a) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let test = test_aid();
    let test_taps = test.map(|t| t.0);
    if arm.coop {
        // The deciding game runs the bout: this screen sends what the player does. With a
        // player on the other stool the bout starts by itself; with the opponent seated
        // there, the start key asks for it.
        match a.page {
            Page::Offer | Page::Bout if input.just_pressed(Control::GameLeave) => a.leaving = true,
            Page::Offer => {
                let seated = a.foe_seated_at.filter(|_| a.foe.is_some());
                let start = input.just_pressed(TAP) || input.key_just_pressed(KeyCode::Enter) || seated.is_some_and(|t| test.is_some_and(|aid| now - t >= aid.1));
                if seated.is_some() && start && !arm.asked_npc {
                    info!("arm wrestle: asks for the bout against {} (co-op)", a.foe_name);
                    arm.want_npc = true;
                    arm.asked_npc = true;
                }
            }
            Page::Bout => {
                if input.just_pressed(TAP) {
                    arm.taps += 1;
                }
                // The test aid taps steadily at its rate from the pull.
                match (test_taps, a.bout.as_ref().filter(|b| b.phase == Phase::Pull)) {
                    (Some(rate), Some(b)) => {
                        let due = ((now - b.started - fnv_game::armwrestle::COUNTDOWN).max(0.0) * rate) as u32;
                        while *aid_taps < due {
                            *aid_taps += 1;
                            arm.taps += 1;
                        }
                    }
                    _ => *aid_taps = 0,
                }
            }
            Page::Result => {}
        }
        return;
    }
    match a.page {
        Page::Offer => {
            if input.just_pressed(Control::GameLeave) {
                a.leaving = true;
                return;
            }
            // Only with him on the other stool.
            let Some(seated_at) = a.foe_seated_at.filter(|_| a.foe.is_some()) else { return };
            let start = input.just_pressed(TAP) || input.key_just_pressed(KeyCode::Enter) || test.is_some_and(|t| now - seated_at >= t.1);
            if start {
                let seed = crate::minigames::seed(a.other.0, now);
                info!("arm wrestle: {} (Strength {}) against the player (Strength {}); the bout begins", a.foe_name, a.foe_strength, a.player_strength);
                a.bout = Some(Bout::new([a.player_strength, a.foe_strength], now));
                a.npc = Some(NpcTapper::new(a.foe_strength, now + fnv_game::armwrestle::COUNTDOWN, seed));
                a.page = Page::Bout;
            }
        }
        Page::Bout => {
            let Some(b) = a.bout.as_mut() else { return };
            if input.just_pressed(Control::GameLeave) {
                // Getting up mid-bout gives it up.
                info!("arm wrestle: left mid-bout; the opponent wins");
                b.winner = Some(1 - a.side);
                b.phase = Phase::Done;
                b.ended = Some(now);
                return;
            }
            if input.just_pressed(TAP) {
                b.tap(a.side, now);
            }
            if let Some(rate) = test_taps {
                // The test aid taps steadily at its rate.
                let pull = b.started + fnv_game::armwrestle::COUNTDOWN;
                let due = ((now - pull).max(0.0) * rate) as u32;
                while b.phase == Phase::Pull && b.taps[a.side as usize] < due {
                    b.tap(a.side, now);
                }
            }
        }
        Page::Result => {}
    }
}

/// `NVF_PLAY_TEST_ARMWRESTLE=<taps a second>[:<seconds>]`: start a bout that long after
/// the opponent sits (0 by default) and tap at that rate. A test aid for the smoke
/// scenario; not a game behaviour.
fn test_aid() -> Option<(f32, f32)> {
    let v = std::env::var("NVF_PLAY_TEST_ARMWRESTLE").ok()?;
    let mut it = v.trim().split(':');
    let rate = it.next()?.trim().parse().ok()?;
    let after = it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
    Some((rate, after))
}

fn step(time: AidTime, mut screen: ResMut<ArmScreen>, mut flourish: MessageWriter<crate::flourish::Flourish>, arm: Res<ArmSeat>, tables: Option<Res<Tables>>, sim: Res<SimState>) {
    let Some(a) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    if arm.coop {
        // The match the deciding game sends, while this player wrestles in it.
        let me = tables.as_deref().and_then(|t| t.me).unwrap_or(crate::armwrestle_coop::HOST);
        if let (Some((_, b, who)), Some(side)) = (arm.bout.as_ref(), arm.my_side(me)) {
            a.side = side;
            a.npc = None;
            a.foe_name = match who[1 - side as usize] {
                Wrestler::Player(p) => tables.as_deref().map_or_else(|| format!("Player {p}"), |t| t.name(p)),
                Wrestler::Npc(r) => sim.ref_name(None, r).map(str::to_string).filter(|n| !n.is_empty()).unwrap_or_else(|| a.foe_name.clone()),
            };
            a.foe_strength = b.strength[1 - side as usize];
            a.foe_player = match who[1 - side as usize] {
                Wrestler::Player(p) => Some(p),
                Wrestler::Npc(_) => None,
            };
            if a.page == Page::Offer && b.phase != Phase::Done {
                info!("arm wrestle: the bout begins against {} (co-op, side {side})", a.foe_name);
                a.page = Page::Bout;
            }
            if a.page != Page::Offer {
                a.bout = Some(b.clone());
            }
        }
    }
    let Some(b) = a.bout.as_mut() else { return };
    if !arm.coop {
        b.step(now);
        if let Some(npc) = a.npc.as_mut() {
            npc.drive(b, 1 - a.side, now);
        }
    }
    if a.page == Page::Bout && b.phase == Phase::Done {
        a.page = Page::Result;
        let won = b.winner == Some(a.side);
        info!("arm wrestle: {} after {:.1}s, taps {:?}, meter {:.2}", if won { "won" } else { "lost" }, b.ended.unwrap_or(now) - b.started - fnv_game::armwrestle::COUNTDOWN, b.taps, b.meter);
        if won {
            flourish.write(crate::flourish::Flourish { kind: crate::flourish::Highlight::ArmWrestle, text: "YOU WIN".into() });
        }
    }
    // Against another player both get up on the winner's schedule, the highlight's
    // length on: the loser's screen has no highlight, and alone it got up 2.4 s before the
    // winner, its puppet walking off while the winner still sat [the lead, ship14 laptop
    // run: the guest up 2.6 s after the loss, the host 5.0 s].
    let board = board_after(a.foe_player.is_some() || b.winner == Some(a.side));
    if a.page == Page::Result && !a.leaving && b.ended.is_some_and(|e| now - e >= board + RESULT_HOLD) {
        a.leaving = true;
    }
}

/// The match view's pitch: down onto the clasp, the opponent's face in the upper part of
/// the screen over the shoulder (run9: -39 degrees framed it so) [design].
pub const MATCH_PITCH: f32 = -0.61;

/// How long a match waits for the player to be on the seat before it aims anyway
/// (seconds) [design].
const AIM_WAIT: f32 = 4.0;

/// Whether the player is on `stool` (GetSitting 3 there), not still getting on: aimed
/// while getting on, from the side entry, the camera's line was off the table (run9,
/// run10) [design].
pub fn seated_on(using: Option<fnv_game::game::FurnitureUse>, stool: FormId) -> bool {
    using.is_some_and(|u| u.reference == stool && u.state == fnv_actor::furniture::state::ON)
}

/// The match's view: seated at the table the camera goes to third person, over the
/// shoulder, where the player's own body wrestles (its arms posed to the clasp, nvforever
/// `armwrestle_arm`; the first-person arms are not), and turns once to look across the table
/// down onto the clasped hands, the opponent above them; the mouse looks around after. Up again,
/// the view it had comes back. While the match runs nvforever's `armwrestle_view` holds the
/// camera close over the shoulder [owner (via the lead, 2026-11-11): in first person the
/// table filled the screen, the opponent cut off at the chest, the player's own arm not
/// there; laptop ship5 run].
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn match_camera(
    time: AidTime,
    screen: Res<ArmScreen>,
    table: Res<ArmTable>,
    sim: Res<SimState>,
    mut control: ResMut<crate::control::PlayerControl>,
    mut player: ResMut<crate::player::Player>,
    mut st: Local<Option<(FormId, crate::control::Pov, Option<f32>)>>,
    script: Option<Res<ScriptState>>,
) {
    use crate::control::Pov;
    let now = time.elapsed_secs();
    let seated = screen.0.as_ref().filter(|a| a.left_at.is_none()).map(|a| a.stool);
    match (seated, st.as_mut()) {
        (Some(stool), None) => {
            info!("arm wrestle: the match's view: third person (was {:?})", control.pov);
            *st = Some((stool, control.pov, Some(now)));
            control.pov = Pov::Third;
        }
        (Some(stool), Some((_, _, since))) => {
            let Some(started) = *since else { return };
            let at = |r: FormId| sim.world.peek(r).map(|x| x.pos).or_else(|| sim.world.placed.get(r).map(|p| p.pos));
            let using = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.furniture_use(FormId::PLAYER_REF));
            if !seated_on(using, stool) && now - started < AIM_WAIT {
                return;
            }
            // Straight across the table from the seat, looking down onto the clasp: the
            // camera over the shoulder behind (its pivot is not the seated body's, run11).
            if let (Some(seat), Some(t)) = (at(stool), table.table.and_then(at)) {
                player.heading = facing(seat, t);
                player.pitch = MATCH_PITCH;
                info!("arm wrestle: the view looks across the table, heading {:.0} pitch {:.0} deg", player.heading.to_degrees(), player.pitch.to_degrees());
            }
            *since = None;
        }
        (None, Some((_, was, _))) => {
            if *was != Pov::Third && control.pov == Pov::Third {
                control.pov = *was;
            }
            info!("arm wrestle: the match's view ends ({:?} again)", control.pov);
            *st = None;
        }
        (None, None) => {}
    }
}

/// Getting up: the screen closes and the player stands (the furniture). The game lets the
/// opponent go once nobody sits across from him (`armwrestle_table::tick`).
#[allow(clippy::too_many_arguments)]
fn leave(
    time: AidTime,
    table: Res<ArmTable>,
    mut screen: ResMut<ArmScreen>,
    mut seat: ResMut<PlayerSeat>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut ended: MessageWriter<ArmEnded>,
    mut arm: ResMut<ArmSeat>,
) {
    let now = time.elapsed_secs();
    // On one of the table's stools and not getting up: the keys are this game's, Space
    // its tap and only Tab gets up (`PlayerSeat::game`; the card may not be open yet,
    // waiting for a message box to close). Play runs under the card every other frame
    // [observed: desktop run 2026-11-12, script.rs hold_player sets Playing back each
    // frame, the arm screen not being one of its tables; the world going on is what lets
    // the Settler walk over], and there Space was Jump and stood the player up.
    let staying = screen.0.as_ref().is_none_or(|a| !a.leaving && a.left_at.is_none());
    seat.game = staying && seat.hold.is_some() && seat.on.is_some_and(|s| table.stool_index(s).is_some());
    let Some(a) = screen.0.as_mut() else { return };
    // Up some other way (the stool left by moving once the menu let go): as leaving.
    if (a.leaving || seat.hold.is_none()) && a.left_at.is_none() {
        a.leaving = true;
        a.left_at = Some(now);
        seat.leave = seat.hold.is_some();
        if *state.get() == PlayState::Menu {
            next.set(PlayState::Playing);
        }
        info!("arm wrestle: getting up");
        arm.at = None;
        arm.asked_npc = false;
    }
    // Up: the screen is done; a settled bout gets its end view.
    if a.left_at.is_some() && seat.hold.is_none() {
        if let Some(won) = a.won() {
            ended.write(ArmEnded { table: table.table.map_or(a.stool.0, |t| t.0), won, opponent: a.foe.map(|r| r.0), peer: a.foe_player });
        }
        info!("arm wrestle: up; closed");
        screen.0 = None;
    }
}

/// The meter's track, interface units.
const METER_W: f32 = 420.0;
const METER_H: f32 = 14.0;
/// The tap prompt's place: the view's lower third, under the clasped hands the match camera
/// centres (nvforever `armwrestle_view`), above the HUD's bars (their tops at about 800 of
/// the 960 units) [design: the lead, 2026-11-11, run15: at 560 the prompt and bar sat on the
/// clasp].
const PROMPT_TOP: f32 = 680.0;

fn draw(mut commands: Commands, screen: Res<ArmScreen>, time: AidTime, casino: Option<Res<CasinoFaces>>, old: Query<Entity, With<ArmRoot>>, mut arm: ResMut<ArmSeat>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let hud = screen.0.as_ref().is_some_and(|a| a.page != Page::Offer);
    if arm.hud != hud {
        arm.hud = hud;
    }
    let Some(a) = screen.0.as_ref() else { return };
    let now = time.elapsed_secs();
    let plain;
    let f = match casino.as_deref() {
        Some(f) => f,
        None => {
            plain = CasinoFaces::plain();
            &plain
        }
    };
    let root = commands.spawn((ArmRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(25), Pickable::IGNORE)).id();
    commands.entity(root).with_children(|p| match a.page {
        Page::Offer => {
            use crate::match_lobby::{key_row, menu_card};
            let who = if a.foe_name.is_empty() { "The Settler" } else { a.foe_name.as_str() };
            let (status, first) = match a.foe_state {
                Foe::Seated => (format!("{who} is ready."), key_row(TAP, "Start the bout")),
                Foe::Coming => (format!("{who} is coming over..."), key_row(TAP, "Wait")),
                Foe::Away => ("Nobody is free to wrestle right now.".to_string(), key_row(TAP, "Wait")),
            };
            let rows = [first, key_row(Control::GameLeave, "Get up")];
            menu_card(p, f, "Arm Wrestling", &status, &rows, &format!("Tap {} as fast as you can to force the arm down. Strength helps a little.", mode_key(TAP)));
        }
        Page::Bout | Page::Result => {
            let Some(b) = a.bout.as_ref() else { return };
            let me = a.side as usize;
            let rows = [
                crate::casino_hud::ScoreRow { name: "You".into(), score: format!("{}", b.taps[me]), note: format!("STR {}", a.player_strength as i32), up: true },
                crate::casino_hud::ScoreRow { name: a.foe_name.clone(), score: format!("{}", b.taps[1 - me]), note: format!("STR {}", b.strength[1 - me] as i32), up: false },
            ];
            let won = a.won() == Some(true);
            let board = a.page == Page::Result && b.ended.is_some_and(|e| now - e >= board_after(won));
            // The taps until the leaderboard, which takes their place (parity's review of
            // the night shots, 2026-11-12: both up at once) [design].
            if !board {
                crate::casino_hud::score_panel(p, f, &rows, Some("Taps"));
            }
            if a.page == Page::Result {
                // The YOU WIN highlight alone first, then the leaderboard.
                if !board {
                    return;
                }
                let winner = if won { "You".to_string() } else { a.foe_name.clone() };
                let loser = if won { a.foe_name.clone() } else { "You".to_string() };
                let places = [crate::casino_hud::Place { name: winner.clone(), detail: "Arm down".into() }, crate::casino_hud::Place { name: loser, detail: String::new() }];
                crate::casino_hud::leaderboard(p, f, &winner, &places);
            } else {
                let left = b.countdown_left(now);
                let prompt = if left > 0.0 { format!("{}", left.ceil() as i32) } else { format!("TAP {}", mode_key(TAP).to_uppercase()) };
                prompt_and_meter(p, f, &prompt, b.lean(a.side));
            }
        }
    });
}

/// The one centred prompt, and the meter under it: the marker right of the middle is
/// this player winning.
fn prompt_and_meter(p: &mut ChildSpawnerCommands, f: &CasinoFaces, prompt: &str, lean: f32) {
    use crate::interface::{amber, u};
    let w = crate::casino_hud::width(f, 2, prompt);
    let box_w = METER_W.max(w) + 60.0;
    p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(-box_w / 2.0)), top: u(PROMPT_TOP), width: u(box_w), height: u(110.0), ..default() }).with_children(|b| {
        crate::casino_hud::glass(b, 0.0, 0.0, box_w, 104.0);
        crate::casino_hud::line(b, f, 2, prompt, (box_w - w) / 2.0, 46.0, amber(255.0));
        let x0 = (box_w - METER_W) / 2.0;
        let y = 70.0;
        b.spawn((Node { position_type: PositionType::Absolute, left: u(x0), top: u(y), width: u(METER_W), height: u(METER_H), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)), Outline { width: u(1.0), offset: u(0.0), color: amber(200.0) }));
        // This player's share of the track, from the left.
        let share = ((lean + 1.0) / 2.0).clamp(0.0, 1.0);
        b.spawn((Node { position_type: PositionType::Absolute, left: u(x0), top: u(y), width: u(METER_W * share), height: u(METER_H), ..default() }, BackgroundColor(amber(200.0).with_alpha(0.75))));
        b.spawn((Node { position_type: PositionType::Absolute, left: u(x0 + METER_W / 2.0 - 1.0), top: u(y - 4.0), width: u(2.0), height: u(METER_H + 8.0), ..default() }, BackgroundColor(amber(255.0))));
    });
}

#[cfg(test)]
mod facing_tests {
    use super::facing;
    use std::f32::consts::FRAC_PI_2;

    /// The camera aims once the player is on the seat, not while getting on.
    #[test]
    fn the_view_waits_for_the_seat() {
        use fnv_game::game::FurnitureUse;
        use fnv_sim::FormId;
        let stool = FormId(0xFF7F_A001);
        let using = |reference, state| Some(FurnitureUse { reference, marker_id: 12, state });
        assert!(!super::seated_on(None, stool));
        assert!(!super::seated_on(using(stool, 2), stool), "getting on");
        assert!(!super::seated_on(using(FormId(0xFF7F_A002), 3), stool), "the other seat");
        assert!(super::seated_on(using(stool, 3), stool));
    }

    /// From the east seat the view faces west across the table, from the west seat east;
    /// a table to the north is straight ahead.
    #[test]
    fn seated_view_faces_the_table() {
        let table = [350.0, 300.0, 3456.0];
        assert!((facing([392.0, 300.0, 3482.0], table) + FRAC_PI_2).abs() < 1e-5);
        assert!((facing([308.0, 300.0, 3482.0], table) - FRAC_PI_2).abs() < 1e-5);
        assert!(facing([350.0, 250.0, 3482.0], table).abs() < 1e-5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_waits_for_the_opponent_on_the_other_stool() {
        let (other, elsewhere) = (FormId(0xFF00_0002), FormId(0x0010_6192));
        assert_eq!(foe(other, Some(other), true, true), Foe::Seated);
        assert_eq!(foe(other, Some(other), false, true), Foe::Coming);
        // Not called yet: coming while he is about, else nobody.
        assert_eq!(foe(other, None, false, true), Foe::Coming);
        assert_eq!(foe(other, None, false, false), Foe::Away);
        // Sent to another seat is not ours.
        assert_eq!(foe(other, Some(elsewhere), true, true), Foe::Away);
    }
}
