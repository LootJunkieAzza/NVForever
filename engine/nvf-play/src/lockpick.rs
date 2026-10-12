//! Lockpicking (docs/spec-notes/lockpicking.md): activating a locked door or container
//! whose key the player does not carry opens the lockpicking menu, when the player's
//! Lockpick skill is high enough and a bobby pin is in the inventory
//! (`fnv_game::lockpick::opening`); else a line says why not. In the menu the mouse
//! turns the bobby pin over a half circle (left 0°, up 90°, right 180°) and the movement
//! keys put torque on the lock with the screwdriver (`fnv_game::lockpick::Picking`): the
//! nearer the pin is to the lock's sweet spot the further the lock turns; in the sweet
//! spot it turns all the way and opens. Torque against a lock that has stopped wears the
//! pin; a worn-out pin breaks and is taken from the inventory, and the next one goes in.
//! Force Lock opens the lock at once or breaks it for good. When the lock opens it is
//! unlocked and the menu closes; the player activates the door or container again to use
//! it [hypothesis, §4].
//!
//! It is a `PlayState::Menu`: the world and the game clock stand still. To scripts it is
//! menu 1014 [documented: GECK wiki GetMenuTargetRef]. What is on the screen besides the
//! lock (`screen`) follows `menus\lockpick_menu.xml`; the lock itself is the game's model,
//! drawn by the engine (nvforever `lockpick_view.rs`) into [`LockView`].
//!
//! Keys [documented: the help message `HelpLockpicking` 0x0000017C and the menu file's
//! `_PCButton_F` / `_PCButton_E`]: the mouse turns the pin (the Left and Right arrows too
//! [design]); Forward, Back, Strafe Left and Strafe Right (W, S, A, D) apply torque while
//! held; F forces the lock; E leaves (Esc and Tab too [design]). The buttons can be
//! clicked.
//!
//! A picked lock is a crime when the door or container is owned: a [`LockOpened`]
//! message names the owner and a line is logged ("crime hook"); `crate::crime` judges it
//! (`fnv_game::Game::player_opened_lock`).
//!
//! Test aid `NVF_PLAY_TEST_LOCKPICK=<seconds>:<step>[,…]`: `open:<reference hex>` as if
//! the player had activated that locked reference, `skill:<n>` makes the player's Lockpick
//! that for locks, `pin:<degrees>` puts the pin there, `spot` in the middle of the sweet
//! spot, `off:<degrees>` that far beyond the sweet spot's edge, `torque:<seconds>` holds
//! torque that long, `force` presses Force Lock (`force:<roll>`: with that roll, 0 to
//! 100), `exit` leaves. At exit, when the menu was asked for: "lockpick summary: …".

pub mod screen;

use crate::bindings::{Control, ControlInput};
use crate::pause::{AidTime, Paused};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
use crate::{PlaySet, PlayState};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use fnv_game::lockpick::{self as rules, Opening, Phase, Picking, Refusal, Settings, PIN_RANGE};
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// The menu code scripts see [documented: GECK wiki GetMenuTargetRef, 1014].
pub const MENU_LOCKPICK: u16 = fnv_game::game::menus::LOCKPICK;

/// Degrees the pin turns for a unit of mouse movement [design: 600 units across the half
/// circle].
pub const MOUSE_DEGREES: f32 = 0.3;
/// Degrees a second the arrow keys turn the pin [design].
pub const KEY_DEGREES: f32 = 90.0;
/// After the lock opened, broke, or the last pin broke: seconds until the menu closes
/// [design: long enough to see the lock turned and read the line].
pub const END_SECONDS: f32 = 1.5;
/// The pin's movement sound plays once for this many degrees of travel [design].
pub const MOVE_SOUND_DEGREES: f32 = 8.0;

/// Ask for the lockpicking menu on a locked reference (activation, or the test aid).
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenLockpick {
    pub reference: u32,
}

/// A lock was picked or forced open: the crime hook (`crate::crime` reads it). `owner`:
/// the door's or container's owner, or its cell's (`fnv_game::lockpick::owner`).
#[derive(Message, Clone, Copy, Debug)]
pub struct LockOpened {
    pub reference: u32,
    pub owner: Option<u32>,
    pub forced: bool,
}

/// The lock's picture, drawn by the engine (nvforever `lockpick_view.rs`) while the menu
/// is open; `screen` shows it behind the menu's text.
#[derive(Resource, Clone, Debug)]
pub struct LockView {
    pub image: Handle<Image>,
}

/// The open lockpicking menu, if any.
#[derive(Resource, Default)]
pub struct LockpickScreen(pub Option<Session>);

pub struct Session {
    pub reference: FormId,
    pub name: String,
    pub game: Picking,
    /// The status line (a pin broke, the lock opened …) [documented: the menu file's
    /// `LPM_StatusText`, "message on pick break / success"; the words are NVForever's].
    pub status: Option<String>,
    /// Aid seconds when the menu opened (the lock's model comes in from then) and when
    /// the current pin went in.
    pub opened: f32,
    pub pin_in: f32,
    /// Aid seconds when the game ended (opened, broken, out of pins).
    pub ended: Option<f32>,
    /// Degrees the pin moved since its last movement sound.
    moved: f32,
    /// The test aid holds torque until then (aid seconds).
    aid_torque_until: Option<f32>,
}

impl Session {
    pub fn force_label(&self) -> String {
        // [observed: "Force Lock [15%]", the owner's screenshots as noted 2026-10-11]
        format!("Force Lock [{:.0}%]", self.game.force_chance())
    }
}

/// A command for the open menu.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum LockpickInput {
    /// Turn the pin by degrees (the mouse, the arrows).
    Turn(f32),
    /// Put the pin at an angle (test aid).
    Pin(f32),
    /// Test aid: the pin in the middle of the sweet spot, or this many degrees beyond
    /// its edge.
    Spot,
    Off(f32),
    /// Test aid: hold torque this many seconds.
    Torque(f32),
    /// Force Lock, with the roll given or a random one.
    Force(Option<f32>),
    Exit,
}

#[derive(Resource, Default, Debug)]
struct LockpickStats {
    asked: u32,
    opened: u32,
    refused: u32,
    picked: u32,
    forced: u32,
    broken: u32,
    pins_broken: u32,
    out_of_pins: u32,
    tries: u32,
    left: u32,
}

/// `NVF_PLAY_TEST_LOCKPICK`.
#[derive(Resource, Default)]
struct LockpickAid {
    steps: Vec<(f32, AidStep)>,
    skill: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AidStep {
    Open(u32),
    Skill(f32),
    Input(LockpickInput),
}

fn parse_aid(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, what)| {
            let (op, arg) = what.split_once(':').unwrap_or((what, ""));
            let num = |name: &str| arg.trim().parse::<f32>().map_err(|_| format!("{what:?}: expected {op}:<{name}>"));
            let step = match op {
                "open" => AidStep::Open(u32::from_str_radix(arg.trim().trim_start_matches("0x"), 16).map_err(|_| format!("{what:?}: expected open:<reference hex>"))?),
                "skill" => AidStep::Skill(num("n")?),
                "pin" => AidStep::Input(LockpickInput::Pin(num("degrees")?)),
                "spot" => AidStep::Input(LockpickInput::Spot),
                "off" => AidStep::Input(LockpickInput::Off(num("degrees")?)),
                "torque" => AidStep::Input(LockpickInput::Torque(num("seconds")?)),
                "force" if arg.is_empty() => AidStep::Input(LockpickInput::Force(None)),
                "force" => AidStep::Input(LockpickInput::Force(Some(num("roll")?))),
                "exit" => AidStep::Input(LockpickInput::Exit),
                _ => return Err(format!("{what:?}: expected open:, skill:, pin:, spot, off:, torque:, force or exit")),
            };
            Ok((t, step))
        })
        .collect()
}

pub struct LockpickPlugin;

impl Plugin for LockpickPlugin {
    fn build(&self, app: &mut App) {
        let steps = match std::env::var("NVF_PLAY_TEST_LOCKPICK").map(|v| parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_LOCKPICK: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<LockpickScreen>()
            .init_resource::<LockpickStats>()
            .insert_resource(LockpickAid { steps, skill: None })
            .add_message::<OpenLockpick>()
            .add_message::<LockpickInput>()
            .add_message::<LockOpened>()
            .add_systems(
                Update,
                (run_aid, open, (keys, buttons, apply, step).chain().run_if(in_state(PlayState::Menu)), screen::draw)
                    .chain()
                    .after(crate::menu::open_menus)
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close)
            .add_systems(Last, report);
    }
}

fn run_aid(time: AidTime, mut aid: ResMut<LockpickAid>, mut open: MessageWriter<OpenLockpick>, mut input: MessageWriter<LockpickInput>) {
    if aid.steps.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let mut due: Vec<(f32, AidStep)> = aid.steps.iter().filter(|(t, _)| *t <= now).copied().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.steps.retain(|(t, _)| *t > now);
    for (_, step) in due {
        info!("test aid NVF_PLAY_TEST_LOCKPICK: {step:?}");
        match step {
            AidStep::Open(r) => {
                open.write(OpenLockpick { reference: r });
            }
            AidStep::Skill(s) => aid.skill = Some(s),
            AidStep::Input(i) => {
                input.write(i);
            }
        }
    }
}

/// A seed: different every time [design: where the sweet spot lies is not known to be
/// kept per lock, §3].
fn seed(reference: FormId, now: f32) -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    t ^ ((reference.0 as u64) << 24) ^ now.to_bits() as u64
}

/// 0 to 1 from the generator.
fn unit(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

/// The line shown when the menu does not open [hypothesis: the original's words are its
/// own settings, not in the plugins; these are NVForever's].
pub fn refusal_line(r: Refusal) -> String {
    match r {
        Refusal::RequiresKey => "This lock requires a key.".into(),
        Refusal::Broken => "This lock is broken. It can only be opened with a key.".into(),
        Refusal::Skill { needed } => format!("You need a Lockpick skill of {needed:.0} to pick this lock."),
        Refusal::NoPins => "You have no bobby pins.".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn open(
    mut requests: MessageReader<OpenLockpick>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut screen: ResMut<LockpickScreen>,
    paused: Res<Paused>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    aid: Res<LockpickAid>,
    time: AidTime,
    (mut stats, mut hud, mut sounds): (ResMut<LockpickStats>, ResMut<crate::activate::HudMessage>, MessageWriter<UiSound>),
) {
    let Some(asked) = requests.read().last().copied() else { return };
    stats.asked += 1;
    if *state.get() != PlayState::Playing || screen.0.is_some() || paused.0 {
        info!("lockpick: request for {:08X} ignored, the player is not in the world", asked.reference);
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else {
        warn!("lockpick: scripts are not running; lockpicking needs the game's rules");
        return;
    };
    let reference = FormId(asked.reference);
    let name = sim.base_of(asked.reference).and_then(|b| sim.name(b)).map_or_else(|| reference.to_string(), str::to_string);
    let now = time.elapsed_secs();
    let (opening, skill) = with_world(rt, &mut sim, |rt| {
        let skill = aid.skill.unwrap_or_else(|| rules::skill(&mut rt.game));
        (rules::opening(&mut rt.game, reference, skill), skill)
    });
    match opening {
        Opening::NotLocked => info!("lockpick: {reference} {name} is not locked"),
        Opening::Refused(r) => {
            stats.refused += 1;
            let line = refusal_line(r);
            info!("lockpick: {reference} {name} refused (Lockpick {skill:.0}): on screen: {line:?}");
            hud.show(line);
            sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::Refused });
        }
        Opening::Pick { level, skill, pins } => {
            stats.opened += 1;
            let mut rng = ChaCha8Rng::seed_from_u64(seed(reference, now));
            let game = Picking::new(level, skill, pins, unit(&mut rng), Settings::default());
            info!(
                "lockpick: open {reference} {name}: {} lock (Lockpick {skill:.0}, {pins} bobby pins): sweet spot {:.1}° ± {:.1}°, Force Lock {:.0}%",
                level.name(),
                game.sweet,
                game.half,
                game.force_chance()
            );
            rt.game.open_menu(MENU_LOCKPICK);
            screen.0 = Some(Session { reference, name, game, status: None, opened: now, pin_in: now, ended: None, moved: 0.0, aid_torque_until: None });
            sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::Opened });
            next.set(PlayState::Menu);
        }
    }
}

/// The keys and the mouse.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    time: AidTime,
    screen: Res<LockpickScreen>,
    over_button: Query<&Interaction, With<screen::LockButton>>,
    mut out: MessageWriter<LockpickInput>,
) {
    if screen.0.is_none() {
        return;
    }
    if keys.any_just_pressed([KeyCode::KeyE, KeyCode::Escape, KeyCode::Tab]) {
        out.write(LockpickInput::Exit);
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        out.write(LockpickInput::Force(None));
    }
    let dt = time.delta_secs_f64() as f32;
    let mut turn = 0.0;
    if keys.pressed(KeyCode::ArrowLeft) {
        turn -= KEY_DEGREES * dt;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        turn += KEY_DEGREES * dt;
    }
    // The pointer on a button moves no pin [design: the original hides the pointer over
    // the lock only, `LPM_DetectArea`].
    let on_button = over_button.iter().any(|i| *i != Interaction::None);
    if let Some(m) = motion.filter(|_| !on_button) {
        turn += m.delta.x * MOUSE_DEGREES;
    }
    if turn != 0.0 {
        out.write(LockpickInput::Turn(turn));
    }
}

/// Whether torque is held: any of the four movement controls [documented: the help
/// message].
fn torque_held(input: &ControlInput) -> bool {
    [Control::Forward, Control::Back, Control::SlideLeft, Control::SlideRight].into_iter().any(|c| input.pressed(c))
}

fn buttons(screen: Res<LockpickScreen>, pressed: Query<(&Interaction, &screen::LockButton), Changed<Interaction>>, mut out: MessageWriter<LockpickInput>) {
    if screen.0.is_none() {
        return;
    }
    for (i, b) in &pressed {
        if *i == Interaction::Pressed {
            out.write(b.0);
        }
    }
}

/// Close the menu: back to the world.
fn leave(next: &mut NextState<PlayState>, why: &str) {
    info!("lockpick: closed ({why})");
    next.set(PlayState::Playing);
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut input: MessageReader<LockpickInput>,
    mut screen: ResMut<LockpickScreen>,
    mut next: ResMut<NextState<PlayState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    time: AidTime,
    mut stats: ResMut<LockpickStats>,
    mut rng: Local<Option<ChaCha8Rng>>,
    (mut sounds, mut opened): (MessageWriter<UiSound>, MessageWriter<LockOpened>),
) {
    let now = time.elapsed_secs();
    for &cmd in input.read() {
        let Some(s) = screen.0.as_mut() else { return };
        let g = &mut s.game;
        match cmd {
            LockpickInput::Exit => {
                if s.ended.is_none() {
                    stats.left += 1;
                }
                leave(&mut next, if s.ended.is_some() { "after the game" } else { "the player left" });
                return;
            }
            _ if s.ended.is_some() => {}
            LockpickInput::Turn(d) => {
                let at = g.pin;
                if g.set_pin(at + d) {
                    s.moved += (g.pin - at).abs();
                }
            }
            LockpickInput::Pin(a) => {
                g.set_pin(a);
                info!("lockpick: pin at {:.1}° (sweet spot {:.1}° ± {:.1}°): the lock turns {:.0}°", g.pin, g.sweet, g.half, g.max_turn());
            }
            LockpickInput::Spot => {
                g.set_pin(g.sweet);
                info!("lockpick: pin at {:.1}°, in the sweet spot: the lock turns {:.0}°", g.pin, g.max_turn());
            }
            LockpickInput::Off(d) => {
                // To the side with room for it.
                let right = g.sweet + g.half + d;
                let a = if right <= PIN_RANGE { right } else { g.sweet - g.half - d };
                g.set_pin(a);
                info!("lockpick: pin at {:.1}°, {d:.1}° off the sweet spot ({:.1}° ± {:.1}°): the lock turns {:.0}°", g.pin, g.sweet, g.half, g.max_turn());
            }
            LockpickInput::Torque(secs) => s.aid_torque_until = Some(now + secs),
            LockpickInput::Force(roll) => {
                if g.phase != Phase::Picking {
                    continue;
                }
                let reference = s.reference;
                let roll = roll.unwrap_or_else(|| 100.0 * unit(rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(seed(reference, now)))));
                let chance = g.force_chance();
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
                match g.force(roll) {
                    Some(true) => {
                        stats.forced += 1;
                        let level = g.level;
                        let owner = with_world(rt, &mut sim, |rt| rules::opened(&mut rt.game, reference, level, false));
                        info!("lockpick: Force Lock ({chance:.0}%, roll {roll:.1}): {reference} forced open");
                        crime_hook(&mut opened, reference, owner, true);
                        s.status = Some("Lock forced open".into());
                        s.ended = Some(now);
                        sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::Unlocked });
                    }
                    Some(false) => {
                        stats.broken += 1;
                        with_world(rt, &mut sim, |rt| rules::broke(&mut rt.game, reference));
                        info!("lockpick: Force Lock ({chance:.0}%, roll {roll:.1}): {reference} broken for good, the pin with it ({} left)", g.pins);
                        s.status = Some("The lock is broken".into());
                        s.ended = Some(now);
                        sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::ForceFailed });
                    }
                    None => {}
                }
            }
        }
    }
}

fn crime_hook(out: &mut MessageWriter<LockOpened>, reference: FormId, owner: Option<FormId>, forced: bool) {
    match owner {
        Some(o) => info!("lockpick: crime hook: {reference} is owned by {o} ({}; judged by `crate::crime`)", if forced { "forced" } else { "picked" }),
        None => info!("lockpick: crime hook: {reference} has no owner"),
    }
    out.write(LockOpened { reference: reference.0, owner: owner.map(|o| o.0), forced });
}

/// The game's frame: torque, the lock turning, wear, breaks, the end.
#[allow(clippy::too_many_arguments)]
fn step(
    input: ControlInput,
    time: AidTime,
    mut screen: ResMut<LockpickScreen>,
    mut next: ResMut<NextState<PlayState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    mut stats: ResMut<LockpickStats>,
    (mut sounds, mut opened): (MessageWriter<UiSound>, MessageWriter<LockOpened>),
) {
    let Some(s) = screen.0.as_mut() else { return };
    let now = time.elapsed_secs();
    if let Some(end) = s.ended {
        if now - end >= END_SECONDS {
            leave(&mut next, "the game is over");
        }
        // The lock springs back and a broken pin's wait runs on, without torque.
        s.game.step(time.delta_secs_f64() as f32, false);
        return;
    }
    let mut ui = |event| {
        sounds.write(UiSound { menu: UiMenu::Lockpick, event });
    };
    if s.moved >= MOVE_SOUND_DEGREES {
        s.moved = 0.0;
        ui(UiEvent::PinMoved);
    }
    let aid = s.aid_torque_until.is_some_and(|t| now < t);
    if s.aid_torque_until.is_some() && !aid {
        s.aid_torque_until = None;
    }
    let torque = torque_held(&input) || aid;
    let strained = s.game.stopped;
    let reference = s.reference;
    let events = s.game.step(time.delta_secs_f64() as f32, torque);
    for e in events {
        match e {
            rules::Event::Turning => {
                stats.tries += 1;
                info!("lockpick: torque on, pin at {:.1}° (the lock can turn {:.0}°)", s.game.pin, s.game.max_turn());
                ui(UiEvent::Turning);
            }
            rules::Event::Stopped { at } => {
                info!("lockpick: the lock stopped at {at:.0}°: the pin is under strain");
                ui(UiEvent::Stopped);
                ui(UiEvent::Strain { on: true });
            }
            rules::Event::PinBroke { left } => {
                stats.pins_broken += 1;
                if let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) {
                    with_world(rt, &mut sim, |rt| rules::pin_broke(&mut rt.game));
                }
                info!("lockpick: the bobby pin broke, {left} left");
                ui(UiEvent::Strain { on: false });
                ui(UiEvent::PinBroke);
                if left == 0 {
                    stats.out_of_pins += 1;
                    s.status = Some("You have no bobby pins left".into());
                    s.ended = Some(now);
                } else {
                    s.status = Some("Your bobby pin broke".into());
                }
            }
            rules::Event::NewPin => {
                info!("lockpick: a new bobby pin is in");
                s.pin_in = now;
                s.status = None;
            }
            rules::Event::Opened => {
                stats.picked += 1;
                let level = s.game.level;
                let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { continue };
                let xp_before = rt.game.player.xp;
                let owner = with_world(rt, &mut sim, |rt| rules::opened(&mut rt.game, reference, level, true));
                info!("lockpick: the lock turned all the way: {reference} picked open ({} lock, {:.0} XP)", level.name(), rt.game.player.xp - xp_before);
                crime_hook(&mut opened, reference, owner, false);
                if strained {
                    ui(UiEvent::Strain { on: false });
                }
                ui(UiEvent::Unlocked);
                s.status = Some("Lock picked".into());
                s.ended = Some(now);
            }
        }
    }
    // Torque let go against the stop: the strain is off.
    if strained && !s.game.stopped && s.game.phase == Phase::Picking {
        ui(UiEvent::Strain { on: false });
    }
}

fn close(mut screen: ResMut<LockpickScreen>, script: Option<ResMut<ScriptState>>, mut sounds: MessageWriter<UiSound>) {
    let Some(s) = screen.0.take() else { return };
    if s.game.stopped {
        sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::Strain { on: false } });
    }
    sounds.write(UiSound { menu: UiMenu::Lockpick, event: UiEvent::Closed });
    if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
        if rt.game.menu == Some(MENU_LOCKPICK) {
            rt.game.close_menu();
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<LockpickStats>) {
    if exit.read().next().is_none() || stats.asked == 0 {
        return;
    }
    let s = &*stats;
    info!(
        "lockpick summary: {} asked, {} opened ({} refused); {} picked, {} forced, {} broken by forcing; {} torque tries, {} pins broken, {} times out of pins; {} left before the end",
        s.asked, s.opened, s.refused, s.picked, s.forced, s.broken, s.tries, s.pins_broken, s.out_of_pins, s.left
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aid_reads_its_steps() {
        let s = parse_aid("1:skill:30,2:open:0010B2D4,3:off:20,4:torque:2.5,5:spot,6:pin:45,7:force,8:force:99,9:exit").unwrap();
        assert_eq!(s[0], (1.0, AidStep::Skill(30.0)));
        assert_eq!(s[1], (2.0, AidStep::Open(0x0010B2D4)));
        assert_eq!(s[2], (3.0, AidStep::Input(LockpickInput::Off(20.0))));
        assert_eq!(s[3], (4.0, AidStep::Input(LockpickInput::Torque(2.5))));
        assert_eq!(s[4], (5.0, AidStep::Input(LockpickInput::Spot)));
        assert_eq!(s[5], (6.0, AidStep::Input(LockpickInput::Pin(45.0))));
        assert_eq!(s[6], (7.0, AidStep::Input(LockpickInput::Force(None))));
        assert_eq!(s[7], (8.0, AidStep::Input(LockpickInput::Force(Some(99.0)))));
        assert_eq!(s[8], (9.0, AidStep::Input(LockpickInput::Exit)));
        assert!(parse_aid("1:pick").is_err());
        assert!(parse_aid("1:torque").is_err());
    }

    #[test]
    fn refusals_say_why() {
        assert_eq!(refusal_line(Refusal::Skill { needed: 50.0 }), "You need a Lockpick skill of 50 to pick this lock.");
        assert!(refusal_line(Refusal::NoPins).contains("bobby pins"));
        assert!(refusal_line(Refusal::Broken).contains("key"));
    }

    #[test]
    fn the_random_spot_is_in_range() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        for _ in 0..1000 {
            let u = unit(&mut rng);
            assert!((0.0..1.0).contains(&u));
        }
    }
}
