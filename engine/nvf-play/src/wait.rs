//! The Wait menu, and the same menu as a Sleep menu while the player lies on a bed
//! (docs/spec-notes/wait-menu.md).
//!
//! The Wait control (`Control::Rest`, T by default) opens it while playing. The player
//! picks 1 to 24 hours on the slider and accepts; game time then passes one hour at a
//! time ([`HOUR_SECONDS`] apart) on the one play clock, the simulation's calendar
//! (`fnv_sim::World::advance_hours`: the GameHour, GameDaysPassed and date globals), so
//! the sky, the lighting, the AI packages' clock and scripts read each new hour. The menu
//! counts the hours down meanwhile and closes after the last one; Cancel stops the wait
//! with the hours passed so far.
//!
//! **Sleeping fades the screen out** [owner request 2026-10-11]: once a sleep's hours are
//! accepted the world behind the menu goes to black over [`FADE_SECONDS`] ([`SleepFade`],
//! which whoever draws the world applies), the menu staying on top with its count; when
//! the menu closes the world comes back as fast. A wait does not fade.
//!
//! It is a `PlayState::Menu` like the container and inventory menus: the player, the
//! world and the running game clock stand still, the cursor is free, and the picture
//! follows the menu pause switch (`crate::pause`). To scripts it is menu 1012
//! ([`crate::script::MENU_SLEEP_WAIT`]). It stays shut while a script has the Pip-Boy
//! group of the player's controls off (docs/spec-notes/controls.md §5).
//!
//! Keys: Left / Right (or A / D) and the mouse wheel move the slider one hour, Page Up /
//! Page Down six, Home and End go to its ends, E or Enter accepts, X, Esc or the Wait
//! control cancels; the slider and the two buttons take the mouse (wait-menu.md §6 says
//! which of these are the original's). [`model`] is the menu as plain data.
//!
//! **Refusals** (wait-menu.md §9): instead of opening, a line on the screen says why the
//! player cannot wait or sleep now ([`refusal`]): a script forbade waiting, the player is
//! swimming, hostile actors are near, the bed is someone else's. A script that opens the
//! menu without asking for the check gets it regardless.
//!
//! **Scripts** see a sleep or a wait while its hours pass (`fnv_game::rest`:
//! `IsPCSleeping`, `IsTimePassing`), can open the menu (`ShowSleepWaitMenu`), make the
//! player sleep (`SetPCSleepHours`) and stop a sleep (`WakeUpPC`); [`script_requests`]
//! takes what they asked. A bed's own script is told of the sleep
//! (`Runtime::bed_activated_for_sleep`): that is what gives Well Rested.
//!
//! Test aid `NVF_PLAY_TEST_WAIT=<seconds>:<what>[,…]`: at each time `open` presses the
//! Wait control, `hours=<n>` sets the slider, `accept` and `cancel` press the buttons.
//! For the mouse: `point=<x>:<y>` puts the pointer there (parts of the window's width
//! and height, 0 to 1), `press` and `release` are its left button; they go the way the
//! real pointer does (Bevy's UI finds what is under it).
//! At exit, when the menu was opened at all: "wait summary: …".

pub mod model;

use crate::bindings::{Control, ControlInput};
use crate::control::{group, ControlLocks, LockStats};
use crate::furniture::PlayerSeat;
use crate::menu::ActiveMenu;
use crate::pause::{AidTime, Paused};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use fnv_game::rest::RestRequest;
use fnv_sim::FormId;
use model::{Command, Model, Refusal, RefusalWords, Words, JUMP, MAX_HOURS};

/// Real seconds between two hours passing [engineering choice: the original's pace is
/// not captured; a full day takes twelve seconds].
pub const HOUR_SECONDS: f32 = 0.5;

/// Seconds the screen takes to go black when a sleep is accepted, and to come back
/// after it [design].
pub const FADE_SECONDS: f32 = 1.0;

/// How far the world is faded to black by sleeping: 0 not at all, 1 black.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct SleepFade(pub f32);

/// The fade `dt` seconds on: towards black while a sleep's hours pass, back otherwise.
pub fn fade_step(fade: f32, sleeping: bool, dt: f32) -> f32 {
    let step = dt.max(0.0) / FADE_SECONDS;
    if sleeping { (fade + step).min(1.0) } else { (fade - step).max(0.0) }
}

/// The open Wait or Sleep menu, if any.
#[derive(Resource, Default)]
pub struct WaitMenu(pub Option<Model>);

impl WaitMenu {
    pub fn is_open(&self) -> bool {
        self.0.is_some()
    }
}

/// Open the menu. The default is the player's own request (the Wait control, lying down
/// on a bed, the test aid).
#[derive(Message, Clone, Copy, Debug, Default, PartialEq)]
pub struct OpenWait {
    /// A script's choice of the Sleep or the Wait menu (`ShowSleepWaitMenu`); `None`:
    /// the Sleep menu when the player lies on a bed.
    pub sleep: Option<bool>,
    /// A script opens it without the check of the conditions: no refusal applies.
    pub unchecked: bool,
    /// These hours start passing at once (`SetPCSleepHours`).
    pub hours: Option<u32>,
}

/// Outdoors and indoors, how near a hostile actor refuses a wait or a sleep, game units.
/// The original reads the game settings `fHostileActorExteriorDistance` and
/// `fHostileActorInteriorDistance` [documented: GECK wiki IsHostilesNearby]; neither has
/// a record in FalloutNV.esm and no source read gives their values, so these two numbers
/// are NVForever's [engineering choice] until someone reads them off the original
/// (`GetGS` in its console). A plugin that sets the settings is followed.
pub const HOSTILE_DISTANCE: [f32; 2] = [2048.0, 1024.0];

/// An accepted hour has passed: the calendar is one hour on (whoever moves the world's
/// people moves them on, `nvforever`'s residents).
#[derive(Message, Clone, Copy, Debug)]
pub struct HourPassed {
    pub sleep: bool,
}

/// A command for the open menu (keys, mouse, test aid).
#[derive(Message, Clone, Copy, Debug)]
pub struct WaitInput(pub Command);

/// For the run summary.
#[derive(Resource, Default, Debug)]
struct WaitStats {
    opened: u32,
    as_sleep: u32,
    hours: u32,
    refused: u32,
    /// Requests answered with a refusal line, by [`Refusal`].
    turned_down: [u32; Refusal::COUNT],
    /// Sleeps a bed's own script was told of.
    beds_told: u32,
}

/// `NVF_PLAY_TEST_WAIT`: what to do when.
#[derive(Resource, Default)]
struct WaitAid(Vec<(f32, AidStep)>);

#[derive(Debug, Clone, Copy, PartialEq)]
enum AidStep {
    Open,
    Input(Command),
    Point(f32, f32),
    Press,
    Release,
}

impl AidStep {
    fn of_the_pointer(&self) -> bool {
        matches!(self, AidStep::Point(..) | AidStep::Press | AidStep::Release)
    }
}

/// The steps of `aid` due at `now` that are (or are not) the pointer's, in time order.
fn due_steps(aid: &mut WaitAid, now: f32, pointer: bool) -> Vec<AidStep> {
    let mine = |(t, s): &(f32, AidStep)| *t <= now && s.of_the_pointer() == pointer;
    let mut due: Vec<(f32, AidStep)> = aid.0.iter().filter(|e| mine(e)).copied().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.0.retain(|e| !mine(e));
    due.into_iter().map(|(_, s)| s).collect()
}

fn parse_aid(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, what)| {
            let step = match what {
                "open" => AidStep::Open,
                "accept" => AidStep::Input(Command::Accept),
                "cancel" => AidStep::Input(Command::Cancel),
                "press" => AidStep::Press,
                "release" => AidStep::Release,
                _ => {
                    let hours = what.strip_prefix("hours=").and_then(|n| n.trim().parse::<u32>().ok());
                    let point = what.strip_prefix("point=").and_then(|p| p.split_once(':')).and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                    match (hours, point) {
                        (Some(n), _) => AidStep::Input(Command::Hours(n)),
                        (_, Some((x, y))) => AidStep::Point(x, y),
                        _ => return Err(format!("{what:?}: expected open, hours=<n>, accept, cancel, point=<x>:<y>, press or release")),
                    }
                }
            };
            Ok((t, step))
        })
        .collect()
}

pub struct WaitPlugin;

impl Plugin for WaitPlugin {
    fn build(&self, app: &mut App) {
        let aid = match std::env::var("NVF_PLAY_TEST_WAIT").map(|v| parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_WAIT: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<WaitMenu>()
            .init_resource::<SleepFade>()
            .add_systems(Update, fade.after(PlaySet::Ui))
            .init_resource::<WaitStats>()
            .insert_resource(WaitAid(aid))
            .add_message::<OpenWait>()
            .add_message::<WaitInput>()
            .add_message::<HourPassed>()
            .add_systems(Startup, read_words)
            .add_systems(PreUpdate, aid_pointer.after(bevy::input::InputSystems).before(bevy::ui::UiSystems::Focus))
            // Before the scripts, as the running clock is (`crate::sim`): a frame's
            // scripts all see the hour that passed in it.
            .add_systems(Update, pass_hours.run_if(in_state(PlayState::Menu)).before(PlaySet::Scripts))
            .add_systems(
                Update,
                (
                    rest_opens.run_if(in_state(PlayState::Playing)),
                    run_aid,
                    script_requests,
                    open,
                    (keys, mouse, apply, draw).chain().run_if(in_state(PlayState::Menu)),
                )
                    .chain()
                    .after(crate::menu::open_menus)
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close)
            .add_systems(Last, report);
    }
}

/// The menu's wording, from the game settings of the plugins in use.
fn read_words(sim: Res<SimState>, mut commands: Commands) {
    let words = Words::from_settings(|name| match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(fnv_sim::settings::Setting::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    });
    commands.insert_resource(words);
    let refusals = RefusalWords::from_settings(|name| match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(fnv_sim::settings::Setting::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    });
    commands.insert_resource(refusals);
}

fn rest_opens(input: ControlInput, mut out: MessageWriter<OpenWait>) {
    if input.just_pressed(Control::Rest) {
        out.write(OpenWait::default());
    }
}

/// What scripts asked of the menu since the last frame (`fnv_game::rest`).
fn script_requests(mut script: Option<ResMut<ScriptState>>, mut menu: ResMut<WaitMenu>, mut open: MessageWriter<OpenWait>, mut input: MessageWriter<WaitInput>) {
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    for r in rt.game.rest.take_requests() {
        info!("wait menu: a script asks: {r:?}");
        match (r, menu.0.as_mut()) {
            (RestRequest::Menu { sleep, check }, _) => {
                open.write(OpenWait { sleep: Some(sleep), unchecked: !check, hours: None });
            }
            // With the menu open the hours start in it; a sleep already passing takes
            // the new count.
            (RestRequest::Sleep { hours }, Some(m)) => {
                m.sleep_for(hours);
                match m.passing {
                    Some(p) => rt.game.rest.begin(true, p.asked),
                    None => rt.game.rest.end(),
                }
            }
            (RestRequest::Sleep { hours }, _) => {
                open.write(OpenWait { sleep: Some(true), unchecked: true, hours: Some(hours) });
            }
            (RestRequest::Wake, Some(m)) if m.sleep => {
                input.write(WaitInput(Command::Cancel));
            }
            (RestRequest::Wake, _) => {}
        }
    }
}

/// Why the player cannot wait or sleep now, if anything forbids it (wait-menu.md §9).
/// The original's refusals are known by the names of their texts only; the order they
/// are tried in here is NVForever's [engineering choice].
#[allow(clippy::too_many_arguments)]
fn refusal(
    sleep: bool,
    bed: Option<u32>,
    rt: Option<&mut crate::script::Runtime>,
    sim: &mut SimState,
    swimming: bool,
    feet: Option<[f32; 3]>,
    indoors: bool,
    actors: &[(u32, [f32; 3])],
    distance: f32,
) -> Option<(Refusal, String)> {
    // A script forbade waiting (`EnableFastTravel`'s canWait): the same flag refuses
    // sleeping, and its text is the "cannot wait in this cell" one [documented: GECK wiki
    // SetPCHasSleepWaitOverride, GetPCHasSleepWaitOverride].
    if rt.as_ref().is_some_and(|rt| rt.game.player.fast_travel.wait_disabled) {
        return Some((Refusal::Forbidden, "a script has turned waiting off (EnableFastTravel)".into()));
    }
    // [hypothesis: "under water" is the swimming state; the text's name speaks of
    // waiting only, and nobody lies on a bed swimming.]
    if swimming {
        return Some((Refusal::UnderWater, "the player is swimming".into()));
    }
    let rt = rt?;
    crate::script::with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        if let Some(feet) = feet {
            let near = |p: &[f32; 3]| (0..3).map(|k| (p[k] - feet[k]).powi(2)).sum::<f32>() <= distance * distance;
            let close: Vec<FormId> = actors.iter().filter(|(_, p)| near(p)).map(|(id, _)| FormId(*id)).collect();
            let hostile = close.iter().copied().find(|&a| fnv_game::rest::hostile_to_player(g, a));
            info!("wait menu: {} living actors drawn, {} within {distance:.0} units, {}", actors.len(), close.len(), if hostile.is_some() { "one of them hostile" } else { "none hostile" });
            if let Some(a) = hostile {
                let place = if indoors { "indoors" } else { "outdoors" };
                return Some((Refusal::HostilesNear, format!("{} is hostile and within {distance:.0} units ({place})", g.data.name(a))));
            }
        }
        let owner = bed.filter(|_| sleep).and_then(|b| fnv_game::rest::bed_owned_by_another(g, FormId(b)))?;
        Some((Refusal::OwnedBed, format!("the bed belongs to {}", g.data.name(owner))))
    })
}

fn run_aid(time: AidTime, mut aid: ResMut<WaitAid>, mut open: MessageWriter<OpenWait>, mut input: MessageWriter<WaitInput>) {
    if aid.0.is_empty() {
        return;
    }
    for step in due_steps(&mut aid, time.elapsed_secs(), false) {
        info!("test aid NVF_PLAY_TEST_WAIT: {step:?}");
        match step {
            AidStep::Open => {
                open.write(OpenWait::default());
            }
            AidStep::Input(c) => {
                input.write(WaitInput(c));
            }
            _ => {}
        }
    }
}

/// The aid's pointer: where the window has its cursor and what its left button does,
/// after the real input was read and before the UI looks for what is under the cursor.
fn aid_pointer(
    time: AidTime,
    mut aid: ResMut<WaitAid>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
) {
    if aid.0.is_empty() {
        return;
    }
    for step in due_steps(&mut aid, time.elapsed_secs(), true) {
        info!("test aid NVF_PLAY_TEST_WAIT: {step:?}");
        match step {
            AidStep::Point(x, y) => {
                if let Ok(mut w) = windows.single_mut() {
                    let at = Vec2::new(x * w.width(), y * w.height());
                    w.set_cursor_position(Some(at));
                }
            }
            AidStep::Press => buttons.press(MouseButton::Left),
            AidStep::Release => buttons.release(MouseButton::Left),
            _ => {}
        }
    }
}

/// Open the menu on request while playing: as a Sleep menu when the player lies on a
/// bed. It stays shut while a script has turned the Pip-Boy off [documented: GECK wiki
/// DisablePlayerControls, "the Pip-Boy cannot be brought up, nor the Wait menu";
/// controls.md §5].
#[allow(clippy::too_many_arguments)]
fn open(
    mut requests: MessageReader<OpenWait>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut menu: ResMut<WaitMenu>,
    active: Res<ActiveMenu>,
    paused: Res<Paused>,
    seat: Res<PlayerSeat>,
    mut sim: ResMut<SimState>,
    (words, refusals): (Res<Words>, Res<RefusalWords>),
    mut stats: ResMut<WaitStats>,
    mut script: Option<ResMut<ScriptState>>,
    locks: Option<Res<ControlLocks>>,
    lock_stats: Option<ResMut<LockStats>>,
    (phys, location, actors, mut corner): (Res<crate::phys::PhysState>, Res<crate::Location>, Res<crate::hitvol::HitVolumes>, Option<ResMut<crate::corner_messages::CornerMessages>>),
) {
    // One menu a frame: the last request stands.
    let Some(asked) = requests.read().last().copied() else { return };
    let by_script = asked.sleep.is_some();
    if !by_script && locks.is_some_and(|l| l.pipboy) {
        info!("wait menu: stays shut, a script has disabled the player's controls (Pip-Boy)");
        stats.refused += 1;
        if let Some(mut s) = lock_stats {
            s.held[group::PIPBOY] += 1;
        }
        return;
    }
    let in_script_menu = script.as_ref().is_some_and(|s| s.rt.as_ref().is_some_and(|rt| rt.game.in_menu()));
    if *state.get() != PlayState::Playing || menu.is_open() || active.0.is_some() || paused.0 || in_script_menu {
        info!("wait menu: request ignored, the player is not in the world (a menu, a conversation or a loading screen is up)");
        return;
    }
    let sleep = asked.sleep.unwrap_or(seat.bed);
    let bed = seat.on.filter(|_| seat.bed);
    if !asked.unchecked {
        let player = phys.player.as_ref();
        let indoors = location.interior.is_some();
        let distance = ["fHostileActorExteriorDistance", "fHostileActorInteriorDistance"].map(|name| match sim.forms.settings.get(&name.to_ascii_lowercase()) {
            Some(fnv_sim::settings::Setting::Float(v)) => Some(*v),
            _ => None,
        })[usize::from(indoors)]
        .unwrap_or(HOSTILE_DISTANCE[usize::from(indoors)]);
        let rt = script.as_mut().and_then(|s| s.rt.as_mut());
        let no = refusal(sleep, bed, rt, &mut sim, player.is_some_and(|c| c.swimming), player.map(|c| c.feet), indoors, &actors.living, distance);
        if let Some((why, detail)) = no {
            let line = refusals.line(why, sleep);
            info!("wait menu: refused ({}): {detail}; on screen: {line:?}", if sleep { "sleep" } else { "wait" });
            // A top-left corner message (owner, list 5 item 44).
            if let Some(c) = corner.as_deref_mut() {
                c.push(line.into());
            }
            stats.turned_down[why as usize] += 1;
            return;
        }
    }
    let mut m = Model::open(sleep);
    stats.opened += 1;
    stats.as_sleep += u32::from(m.sleep);
    match sim.world.calendar() {
        Ok(c) => info!(
            "wait menu: open as {} at {} (GameHour {:.3}, GameDaysPassed {:.3})",
            if m.sleep { "sleep (the player lies on a bed)" } else { "wait" },
            words.date_line(&c),
            c.hour,
            c.days_passed
        ),
        Err(e) => warn!("wait menu: no calendar: {e}"),
    }
    // Scripts run MenuMode 1012 from now on.
    if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
        rt.game.open_menu(crate::script::MENU_SLEEP_WAIT);
        // The bed's own script learns that the player sleeps on it.
        if let Some(bed) = bed.filter(|_| sleep) {
            if crate::script::with_world(rt, &mut sim, |rt| rt.bed_activated_for_sleep(FormId(bed))) {
                info!("wait menu: the bed {bed:08X} has a script: its OnActivate block ran for this sleep");
                stats.beds_told += 1;
            }
        }
        if let Some(hours) = asked.hours {
            m.sleep_for(hours);
            if let Some(p) = m.passing {
                info!("wait menu: sleeping {} hour(s) (a script's SetPCSleepHours)", p.asked);
                rt.game.rest.begin(m.sleep, p.asked);
            }
        }
    }
    menu.0 = Some(m);
    next.set(PlayState::Menu);
}

fn keys(keys: Res<ButtonInput<KeyCode>>, input: ControlInput, menu: Res<WaitMenu>, mut out: MessageWriter<WaitInput>) {
    use KeyCode::*;
    if !menu.is_open() {
        return;
    }
    let mut send = |c| {
        out.write(WaitInput(c));
    };
    if keys.any_just_pressed([KeyX, Escape]) || input.just_pressed(Control::Rest) {
        send(Command::Cancel);
        return;
    }
    if keys.any_just_pressed([KeyE, Enter, NumpadEnter]) {
        send(Command::Accept);
    }
    if keys.any_just_pressed([ArrowLeft, KeyA]) {
        send(Command::Step(-1));
    }
    if keys.any_just_pressed([ArrowRight, KeyD]) {
        send(Command::Step(1));
    }
    if keys.just_pressed(PageDown) {
        send(Command::Step(-JUMP));
    }
    if keys.just_pressed(PageUp) {
        send(Command::Step(JUMP));
    }
    if keys.just_pressed(Home) {
        send(Command::Hours(1));
    }
    if keys.just_pressed(End) {
        send(Command::Hours(MAX_HOURS));
    }
}

fn mouse(
    menu: Res<WaitMenu>,
    track: Query<(&Interaction, &RelativeCursorPosition), With<SliderTrack>>,
    buttons: Query<(&Interaction, &WaitButton), Changed<Interaction>>,
    mut wheel: MessageReader<MouseWheel>,
    mut lines: Local<f32>,
    mut out: MessageWriter<WaitInput>,
) {
    let Some(m) = menu.0.as_ref() else { return };
    // A press on the slider, and the drag after it, put the marker under the pointer.
    for (i, at) in &track {
        if let (Interaction::Pressed, Some(p)) = (i, at.normalized) {
            let hours = model::hours_at(p.x + 0.5);
            if hours != m.hours {
                out.write(WaitInput(Command::Hours(hours)));
            }
        }
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            out.write(WaitInput(b.0));
        }
    }
    for w in wheel.read() {
        *lines += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 20.0,
        };
    }
    // Wheel up moves the marker right [documented: the interface file's slider].
    let whole = lines.trunc();
    if whole != 0.0 {
        out.write(WaitInput(Command::Step(whole as i32)));
        *lines -= whole;
    }
}

fn apply(mut input: MessageReader<WaitInput>, mut menu: ResMut<WaitMenu>, mut next: ResMut<NextState<PlayState>>, mut script: Option<ResMut<ScriptState>>) {
    for &WaitInput(cmd) in input.read() {
        let Some(m) = menu.0.as_mut() else { return };
        let before = m.clone();
        let close = m.apply(cmd);
        match (cmd, &m.passing, close) {
            (Command::Accept, Some(p), _) if before.passing.is_none() => {
                info!("wait menu: {} {} hour(s)", if m.sleep { "sleeping" } else { "waiting" }, p.asked);
                // To scripts the player sleeps (or waits) from now until the menu closes.
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    rt.game.rest.begin(m.sleep, p.asked);
                }
            }
            (Command::Cancel, _, true) => match &before.passing {
                Some(p) => info!("wait menu: stopped after {} of {} hours", p.asked - p.left, p.asked),
                None => info!("wait menu: cancelled"),
            },
            _ => {}
        }
        if close {
            next.set(PlayState::Playing);
            return;
        }
    }
}

/// While the accepted hours pass: one hour onto the game's calendar every
/// [`HOUR_SECONDS`], a log line for each, and the menu closes after the last. The time
/// is the aids' clock, which goes on while a menu holds the picture (`crate::pause`).
///
/// Each hour also passes for the player's body (`fnv_game::Game::pass_hours`): active
/// effects advance by the seconds of play an hour stands for, Hardcore's needs rise, a
/// sleep lowers sleep deprivation (docs/spec-notes/aid-items.md §6, §11); a line says
/// what changed.
#[allow(clippy::too_many_arguments)]
fn pass_hours(
    time: AidTime,
    mut menu: ResMut<WaitMenu>,
    mut sim: ResMut<SimState>,
    words: Res<Words>,
    mut stats: ResMut<WaitStats>,
    mut next: ResMut<NextState<PlayState>>,
    mut passed: MessageWriter<HourPassed>,
    mut script: Option<ResMut<ScriptState>>,
) {
    let Some(m) = menu.0.as_mut() else { return };
    if m.passing.is_none() || !m.tick(time.delta_secs_f64() as f32, HOUR_SECONDS) {
        return;
    }
    let p = m.passing.expect("an hour passed");
    match sim.world.advance_hours(1.0) {
        Ok(c) => {
            stats.hours += 1;
            passed.write(HourPassed { sleep: m.sleep });
            if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                let (sleep, hour) = (m.sleep, p.asked - p.left);
                crate::script::with_world(rt, &mut sim, |rt| crate::aid::hour_passed(rt, sleep, hour));
            }
            info!(
                "{}: hour {} of {} passed: {} (GameHour {:.3}, GameDaysPassed {:.3})",
                if m.sleep { "sleep" } else { "wait" },
                p.asked - p.left,
                p.asked,
                words.date_line(&c),
                c.hour,
                c.days_passed
            );
        }
        Err(e) => {
            warn!("wait menu: the game clock cannot advance: {e}");
            next.set(PlayState::Playing);
            return;
        }
    }
    if p.left == 0 {
        info!("wait menu: {} hour(s) passed; closed", p.asked);
        next.set(PlayState::Playing);
    }
}

fn fade(time: AidTime, menu: Res<WaitMenu>, mut fade: ResMut<SleepFade>) {
    let sleeping = menu.0.as_ref().is_some_and(|m| m.sleep && m.passing.is_some());
    let next = fade_step(fade.0, sleeping, time.delta_secs_f64() as f32);
    if next != fade.0 {
        fade.0 = next;
    }
}

fn close(mut commands: Commands, mut menu: ResMut<WaitMenu>, roots: Query<Entity, With<WaitRoot>>, script: Option<ResMut<ScriptState>>) {
    if menu.0.take().is_some() {
        if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
            if rt.game.menu == Some(crate::script::MENU_SLEEP_WAIT) {
                rt.game.close_menu();
            }
            rt.game.rest.end();
        }
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<WaitStats>) {
    let turned_down: u32 = stats.turned_down.iter().sum();
    if exit.read().next().is_none() || (stats.opened == 0 && stats.refused == 0 && turned_down == 0) {
        return;
    }
    let why: Vec<String> = Refusal::ALL.iter().filter(|r| stats.turned_down[**r as usize] > 0).map(|r| format!("{} {}", stats.turned_down[*r as usize], r.label())).collect();
    info!(
        "wait summary: the menu was opened {} times ({} as sleep), {} hours passed, {} requests held by scripts, {} refused{}, {} sleeps told to a bed's script",
        stats.opened,
        stats.as_sleep,
        stats.hours,
        stats.refused,
        turned_down,
        if why.is_empty() { String::new() } else { format!(" ({})", why.join(", ")) },
        stats.beds_told
    );
}

// ---- drawing -----------------------------------------------------------------------
//
// The layout is measured on the owner's 1280 x 720 screenshot of the original menu
// [observed; docs/spec-notes/wait-menu.md §2] and drawn in parts of the view's height,
// so it keeps its place and size on any window. The font is Bevy's, not the game's.

#[derive(Component)]
struct WaitRoot;

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct HoursText;

#[derive(Component)]
struct DateText;

/// The triangle over the slider.
#[derive(Component)]
struct Marker;

/// The slider's clickable length: from its first end mark to its last.
#[derive(Component)]
struct SliderTrack;

#[derive(Component, Clone, Copy)]
struct WaitButton(Command);

/// The colour of the slider's line in the screenshot [observed: (179, 140, 65)].
const AMBER: Color = Color::srgb(0.70, 0.55, 0.255);
/// How opaque a text is: the three lines and the button under the pointer; a button the
/// pointer is not on [documented: the interface file's buttons are at half opacity
/// unless targeted; observed: "Wait E)" at 0.59 of "Cancel X)" in the screenshot]; the
/// accept button while the hours pass [design]. Bevy mixes in linear light, where 0.3
/// reads as that 0.59.
const BRIGHT: f32 = 1.0;
const NORMAL: f32 = 0.3;
const FAINT: f32 = 0.12;
/// The box darkens what is behind it [observed: the sky inside the box is at a quarter
/// of its brightness outside, which in linear light is a twentieth].
const SHADE: Color = Color::srgba(0.0, 0.0, 0.0, 0.93);

/// The box, in the screenshot's pixels.
const BOX: Vec2 = Vec2::new(643.0, 300.0);
/// The slider: its marks start here and are this far apart; marks 1 to 24 are the hours,
/// 0 and 25 the end marks.
const MARK_X0: f32 = 80.0;
const MARK_STEP: f32 = 19.4;
/// The top of the slider's line.
const LINE_Y: f32 = 140.0;
/// The marker over the slider: its width and height; its point ends this far above the
/// line.
const MARKER: Vec2 = Vec2::new(27.0, 13.5);
const MARKER_GAP: f32 = 17.0;

/// A length of the screenshot (720 pixels high) as a part of the view's height.
fn u(px: f32) -> Val {
    Val::Vh(px / 7.2)
}

fn shade(part: f32) -> Color {
    AMBER.with_alpha(part)
}

fn font(px: f32) -> TextFont {
    TextFont { font_size: FontSize::Vh(px / 7.2), ..default() }
}

fn at(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(left), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A line of text across the box, centred on `y`.
fn line(y: f32) -> Node {
    Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..at(0.0, y - 15.0, BOX.x, 30.0) }
}

fn spawn_menu(commands: &mut Commands) {
    let screen = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((WaitRoot, screen, GlobalZIndex(105))).with_children(|root| {
        root.spawn((Node { width: u(BOX.x), height: u(BOX.y), ..default() }, BackgroundColor(SHADE))).with_children(|b| {
            // The outline: a line along the top and the bottom, and at each corner a
            // short side that fades out.
            for y in [0.0, BOX.y - 1.5] {
                b.spawn((at(0.0, y, BOX.x, 1.5), BackgroundColor(AMBER)));
            }
            for x in [0.0, BOX.x - 1.5] {
                for (i, part) in [0.85, 0.55, 0.25].into_iter().enumerate() {
                    let step = 10.0 * i as f32;
                    b.spawn((at(x, step, 1.5, 10.0), BackgroundColor(shade(part))));
                    b.spawn((at(x, BOX.y - 10.0 - step, 1.5, 10.0), BackgroundColor(shade(part))));
                }
            }
            b.spawn((line(61.0), children![(TitleText, Text::new(""), font(21.0), TextColor(shade(BRIGHT)))]));
            // The slider: the line, the hour marks on it and the two end marks.
            b.spawn((at(65.0, LINE_Y, 516.0, 3.0), BackgroundColor(AMBER)));
            for k in 1..=MAX_HOURS {
                b.spawn((at(MARK_X0 + MARK_STEP * k as f32 - 1.0, LINE_Y - 6.0, 2.0, 6.0), BackgroundColor(AMBER)));
            }
            let last = MARK_X0 + MARK_STEP * (MAX_HOURS + 1) as f32;
            for (x, foot) in [(MARK_X0, -7.0), (last, 2.5)] {
                b.spawn((at(x - 1.25, LINE_Y - 12.0, 2.5, 12.0), BackgroundColor(AMBER)));
                b.spawn((at(x + foot - 1.25, LINE_Y - 4.0, 7.0, 4.0), BackgroundColor(AMBER)));
            }
            // The marker: the lower half of a square standing on its corner.
            let side = MARKER.x / std::f32::consts::SQRT_2;
            b.spawn((Marker, Node { overflow: Overflow::clip(), ..at(0.0, LINE_Y - MARKER_GAP - MARKER.y, MARKER.x, MARKER.y) })).with_children(|m| {
                m.spawn((
                    at((MARKER.x - side) / 2.0, -side / 2.0, side, side),
                    BackgroundColor(AMBER),
                    UiTransform::from_rotation(Rot2::degrees(45.0)),
                ));
            });
            // What takes the mouse: the slider's length, from above the marker to under
            // the line.
            b.spawn((SliderTrack, Button, RelativeCursorPosition::default(), at(MARK_X0, LINE_Y - 34.0, MARK_STEP * (MAX_HOURS + 1) as f32, 46.0)));
            b.spawn((line(168.0), children![(HoursText, Text::new(""), font(24.0), TextColor(shade(BRIGHT)))]));
            b.spawn((line(243.0), children![(DateText, Text::new(""), font(20.0), TextColor(shade(BRIGHT)))]));
            // The two buttons, their right ends a little outside the box's.
            for (y, cmd) in [(205.0, Command::Accept), (243.0, Command::Cancel)] {
                let node = Node { position_type: PositionType::Absolute, right: u(-5.0), top: u(y - 15.0), height: u(30.0), align_items: AlignItems::Center, ..default() };
                b.spawn((Button, WaitButton(cmd), node, children![(Text::new(""), font(21.0), TextColor(shade(NORMAL)))]));
            }
        });
    });
}

/// Draw the menu when it opens; after that only what changes: the texts, the marker's
/// place, the buttons' shade.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    menu: Res<WaitMenu>,
    sim: Res<SimState>,
    words: Res<Words>,
    roots: Query<Entity, With<WaitRoot>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<TitleText>>,
        Query<&mut Text, With<HoursText>>,
        Query<&mut Text, With<DateText>>,
        Query<(&mut Text, &mut TextColor, &ChildOf)>,
    )>,
    mut marker: Query<&mut Node, With<Marker>>,
    buttons: Query<(&WaitButton, &Interaction)>,
) {
    let Some(m) = menu.0.as_ref() else { return };
    if roots.is_empty() {
        spawn_menu(&mut commands);
        // Filled in from the next frame on, when the texts exist.
        return;
    }
    let Ok(cal) = sim.world.calendar() else { return };
    let view = m.view(&words, &cal);
    let set = |t: &mut Text, s: &str| {
        if t.0 != s {
            t.0 = s.to_string();
        }
    };
    for mut t in &mut texts.p0() {
        set(&mut t, &view.title);
    }
    for mut t in &mut texts.p1() {
        set(&mut t, &view.hours);
    }
    for mut t in &mut texts.p2() {
        set(&mut t, &view.date);
    }
    for mut node in &mut marker {
        let left = u(MARK_X0 + MARK_STEP * view.marker as f32 - MARKER.x / 2.0);
        if node.left != left {
            node.left = left;
        }
    }
    for (mut t, mut color, parent) in &mut texts.p3() {
        let Ok((b, interaction)) = buttons.get(parent.parent()) else { continue };
        let (label, live) = match b.0 {
            Command::Accept => (&view.accept, view.can_accept),
            _ => (&view.cancel, true),
        };
        set(&mut t, label);
        let part = match (live, interaction) {
            (false, _) => FAINT,
            (true, Interaction::None) => NORMAL,
            (true, _) => BRIGHT,
        };
        if color.0 != shade(part) {
            color.0 = shade(part);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aid_parses() {
        assert_eq!(
            parse_aid("16:open, 17:hours=22,19.5:accept,30:cancel").unwrap(),
            vec![
                (16.0, AidStep::Open),
                (17.0, AidStep::Input(Command::Hours(22))),
                (19.5, AidStep::Input(Command::Accept)),
                (30.0, AidStep::Input(Command::Cancel)),
            ]
        );
        assert_eq!(
            parse_aid("17:point=0.45:0.47,17.5:press,18:release").unwrap(),
            vec![(17.0, AidStep::Point(0.45, 0.47)), (17.5, AidStep::Press), (18.0, AidStep::Release)]
        );
        // The pointer's steps and the others are taken apart, each in time order.
        let mut aid = WaitAid(parse_aid("3:accept,1:press,2:open,0.5:point=0:0,9:release").unwrap());
        assert_eq!(due_steps(&mut aid, 5.0, true), vec![AidStep::Point(0.0, 0.0), AidStep::Press]);
        assert_eq!(due_steps(&mut aid, 5.0, false), vec![AidStep::Open, AidStep::Input(Command::Accept)]);
        assert_eq!(aid.0, vec![(9.0, AidStep::Release)]);
        assert!(parse_aid("16:point=1").is_err());
        assert!(parse_aid("16").is_err());
        assert!(parse_aid("16:sleep").is_err());
        assert!(parse_aid("16:hours=x").is_err());
        assert!(parse_aid("").unwrap().is_empty());
    }

    #[test]
    fn a_sleep_fades_the_screen_out_and_back() {
        let mut f = 0.0;
        // A wait, or a sleep not yet accepted: nothing.
        f = fade_step(f, false, 0.5);
        assert_eq!(f, 0.0);
        f = fade_step(f, true, FADE_SECONDS / 2.0);
        assert!((f - 0.5).abs() < 1e-6);
        f = fade_step(f, true, 10.0);
        assert_eq!(f, 1.0);
        f = fade_step(f, false, FADE_SECONDS / 4.0);
        assert!((f - 0.75).abs() < 1e-6);
        assert_eq!(fade_step(f, false, 10.0), 0.0);
    }

    #[test]
    fn the_marks_fit_the_line() {
        // The end marks stand inside the line's ends.
        let last = MARK_X0 + MARK_STEP * (MAX_HOURS + 1) as f32;
        assert!(MARK_X0 > 65.0 && last < 65.0 + 516.0);
    }
}
