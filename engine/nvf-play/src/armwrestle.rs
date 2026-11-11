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
//! - **The end** ([`Page::Result`]): the leaderboard and, for a win, the Strength
//!   highlight (`flourish::Highlight::ArmWrestle`); after [`RESULT_HOLD`] the player gets
//!   up, the game lets the Settler go back to his package, and the end view (nvforever
//!   `match_end`, item 166) shows the winner cheering.
//! - **Co-op** (part B, `crate::armwrestle_coop`): the match is the deciding game's (the
//!   host's). This screen sits the player down (`ArmSeat::at`, its side the stool's),
//!   sends its taps (`ArmSeat::taps`), asks for a bout against the opponent once he sits
//!   (`ArmSeat::want_npc`) and draws the bout it is sent (`ArmSeat::bout`) instead of
//!   running its own. Two players on the two stools wrestle each other.
//!
//! Log lines start `arm wrestle:`.

use crate::armwrestle_coop::{ArmBodies, ArmSeat, Wrestler, TABLE_KEY};
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
pub const TAP: KeyCode = KeyCode::Space;
pub const TAP_NAME: &str = "Space";
/// Seconds the result shows before the player gets up [design].
pub const RESULT_HOLD: f32 = 2.6;
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
    pub opponent: Option<u32>,
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
            .add_systems(Update, leave.after(PlaySet::Ui));
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
) {
    let on = seat.on.filter(|_| seat.hold.is_some());
    let fresh = on != *was;
    *was = on;
    let Some(stool) = on.filter(|_| fresh) else { return };
    let Some(i) = table.stool_index(stool) else { return };
    if screen.0.is_some() || *state.get() != PlayState::Playing {
        return;
    }
    let Some(other) = table.stools[1 - i] else { return };
    let player_strength = script
        .as_mut()
        .and_then(|s| s.rt.as_mut())
        .map(|rt| with_world(rt, &mut sim, |rt| fnv_game::functions::actor_value(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::STRENGTH).unwrap_or(5.0)))
        .unwrap_or(5.0) as f32;
    info!("arm wrestle: seated on stool {i} ({stool:08X}), Strength {player_strength}");
    screen.0 = Some(Arm {
        stool: FormId(stool),
        other,
        page: Page::Offer,
        bout: None,
        npc: None,
        foe: None,
        foe_name: String::new(),
        foe_strength: 5.0,
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

fn keys(input: Res<ButtonInput<KeyCode>>, time: AidTime, mut screen: ResMut<ArmScreen>, mut arm: ResMut<ArmSeat>, mut aid_taps: Local<u32>) {
    let Some(a) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    let test = test_aid();
    let test_taps = test.map(|t| t.0);
    if arm.coop {
        // The deciding game runs the bout: this screen sends what the player does. With a
        // player on the other stool the bout starts by itself; with the opponent seated
        // there, the start key asks for it.
        match a.page {
            Page::Offer | Page::Bout if input.just_pressed(KeyCode::Tab) => a.leaving = true,
            Page::Offer => {
                let seated = a.foe_seated_at.filter(|_| a.foe.is_some());
                let start = input.just_pressed(TAP) || input.just_pressed(KeyCode::Enter) || seated.is_some_and(|t| test.is_some_and(|aid| now - t >= aid.1));
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
            if input.just_pressed(KeyCode::Tab) {
                a.leaving = true;
                return;
            }
            // Only with him on the other stool.
            let Some(seated_at) = a.foe_seated_at.filter(|_| a.foe.is_some()) else { return };
            let start = input.just_pressed(TAP) || input.just_pressed(KeyCode::Enter) || test.is_some_and(|t| now - seated_at >= t.1);
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
            if input.just_pressed(KeyCode::Tab) {
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
    if a.page == Page::Result && !a.leaving && b.ended.is_some_and(|e| now - e >= RESULT_HOLD) {
        a.leaving = true;
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
            ended.write(ArmEnded { table: table.table.map_or(a.stool.0, |t| t.0), won, opponent: a.foe.map(|r| r.0) });
        }
        info!("arm wrestle: up; closed");
        screen.0 = None;
    }
}

/// The meter's track, interface units.
const METER_W: f32 = 420.0;
const METER_H: f32 = 14.0;
/// The tap prompt's place: the view's middle, a little low [design].
const PROMPT_TOP: f32 = 560.0;

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
            use crate::match_lobby::{menu_card, row};
            let who = if a.foe_name.is_empty() { "The Settler" } else { a.foe_name.as_str() };
            let (status, first) = match a.foe_state {
                Foe::Seated => (format!("{who} is ready."), row(TAP_NAME, "Start the bout")),
                Foe::Coming => (format!("{who} is coming over..."), row(TAP_NAME, "Wait")),
                Foe::Away => ("Nobody is free to wrestle right now.".to_string(), row(TAP_NAME, "Wait")),
            };
            let rows = [first, row("Tab", "Get up")];
            menu_card(p, f, "Arm Wrestling", &status, &rows, &format!("Tap {TAP_NAME} as fast as you can to force the arm down. Strength helps a little."));
        }
        Page::Bout | Page::Result => {
            let Some(b) = a.bout.as_ref() else { return };
            let me = a.side as usize;
            let rows = [
                crate::casino_hud::ScoreRow { name: "You".into(), score: format!("{}", b.taps[me]), note: format!("STR {}", a.player_strength as i32), up: true },
                crate::casino_hud::ScoreRow { name: a.foe_name.clone(), score: format!("{}", b.taps[1 - me]), note: format!("STR {}", b.strength[1 - me] as i32), up: false },
            ];
            crate::casino_hud::score_panel(p, f, &rows, Some("Taps"));
            if a.page == Page::Result {
                let won = a.won() == Some(true);
                let winner = if won { "You".to_string() } else { a.foe_name.clone() };
                let loser = if won { a.foe_name.clone() } else { "You".to_string() };
                let places = [crate::casino_hud::Place { name: winner.clone(), detail: "Arm down".into() }, crate::casino_hud::Place { name: loser, detail: String::new() }];
                crate::casino_hud::leaderboard(p, f, &winner, &places);
            } else {
                let left = b.countdown_left(now);
                let prompt = if left > 0.0 { format!("{}", left.ceil() as i32) } else { format!("TAP {}", TAP_NAME.to_uppercase()) };
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
