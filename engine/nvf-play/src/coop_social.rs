//! The co-op interaction wheel [design]: looking at another player shows an Interact
//! prompt, as Talk shows on an NPC; E opens a wheel styled as the weapon wheel's with
//! **Duel** and **Trade**. Choosing one asks the other player with a Y/N prompt that
//! names the sender ([`invite`]).
//!
//! - **Duel** ([`duel`]): pistols only, and only with room (outdoors, an open line through
//!   the challenger; lack of room is the one reason shown when both are missing). Once
//!   accepted both are told to equip a pistol if they have none, then to stand a few steps
//!   apart with their backs turned; a hidden countdown (2–7 s) holds both players still,
//!   cameras too; an orange sweep (VATS mode's, `crate::vats_mode::look_at`) is the signal
//!   to draw. The first shot to land wins (each game judges its own shot by its aim against
//!   the other's body; the challenger's game says who was first). No damage is done: the
//!   host's hits on the other's stand-in are dropped while it lasts (nvforever `coop.rs`,
//!   [`Social::spares`]); the loser is knocked down and thrown a few feet back.
//! - **Trade** ([`trade`]): browse the other's inventory and ask for items; the owner
//!   prices each item asked for (a price for one); the asker buys for caps. On the barter
//!   menu's own screen, keys and mouse ([`barter`]).
//!
//! The messages ride the players' actions ([`wire`], `crate::coop_tables`); nvforever
//! `coop.rs` fills [`SocialMates`] with where the others are each frame. The loser's own
//! game knocks it down and throws it (`Social::fall`); the other players see it lying
//! through its ordinary state (`nvforever::coop`), not thrown the same: the
//! throw's velocity is told ([`wire::Msg::Thrown`]) and logged, and nothing more.
//!
//! An invitation is answered with Y or N (Enter and Esc too; on a controller Y and X),
//! as the pause menu's questions are.
//!
//! Log lines: `co-op social: …` (invitations, wheel picks, duel phases, trades).
//!
//! Test aid `NVF_PLAY_TEST_SOCIAL=<seconds>:<step>[,...]`: `face` turns the view to the
//! nearest other player's chest at once; `drawface` does so only once a duel's signal to
//! draw has come, turning over [`AID_TURN_SECONDS`] (a player spinning round to shoot;
//! for recorded demos); `drawshoot` turns so too, then after [`AID_SHOT_REACTION`]
//! shoots (the trigger pulled [`AID_SHOT_SECONDS`] of every [`AID_SHOT_EVERY`]) while the
//! draw lasts; `turnback` turns the back to the nearest other player over
//! [`AID_TURNBACK_SECONDS`], only while a duel is getting into position with the two far
//! enough apart (a duel's stance, for recorded demos: give it again and again);
//! `pick=duel|trade` points the open wheel at a slice.

pub mod barter;
pub mod draw;
pub mod duel;
pub mod invite;
pub mod trade;
pub mod wheel;
pub mod wire;

use fnv_util::words::{tr, trf};
use crate::bindings::{Control, ControlInput};
use crate::coop_tables::{game, Tables};
use crate::player::{MoveIntent, Player};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use duel::{Duel, Out};
use fnv_sim::FormId;
use invite::{Invites, Settled};
use rand_core::{RngCore, SeedableRng};
use trade::{Sale, Trade};
use wheel::Wheel;
use wire::{Kind, Msg};

/// How close another player must be for the Interact prompt, game units (an NPC's
/// activation reach is `activate::pick::REACH`; a little more, as the other player moves).
pub const INTERACT_REACH: f32 = 220.0;
/// The body the crosshair is tested against: a vertical capsule from the feet, game units.
const BODY_RADIUS: f32 = 30.0;
const BODY_HEIGHT: f32 = 128.0;
/// Real seconds a banner line stays.
const BANNER_SECONDS: f32 = 4.0;
/// The real seconds a trade waits for the answers to buys in flight after it closes.
const TRADE_LINGER: f32 = 6.0;
/// A guest that has lost the host this long is taken as the one who disconnected: its duel
/// is called off [design: long enough to ride out a short drop].
const DROPPED_SECONDS: f32 = 10.0;
/// The orange sweep: how long it crosses and how long its grade fades (real seconds).
pub const SWEEP_SECONDS: f32 = crate::vats_mode::ENTER_SWIPE;
pub const SWEEP_FADE: f32 = 0.9;

/// The other players (piece 2's `crate::coop_mates`, filled by nvforever's `coop.rs`).
pub use crate::coop_mates::{Mate, SocialMates};

/// The other player `id`, if it is in the session.
pub fn mate(mates: &SocialMates, id: u8) -> Option<&Mate> {
    mates.mates.iter().find(|m| m.id == id)
}

/// A trade going on: its book, and the barter screen it is shown on ([`barter`]).
#[derive(Debug)]
pub struct TradeUi {
    pub book: Trade,
    pub name: String,
    /// The other player's puppet here (the screen's merchant).
    pub puppet: u32,
    /// The barter screen has been opened.
    pub opened: bool,
    /// What just happened, for the line under the lists, and when (real seconds).
    pub event: Option<(String, f32)>,
    /// Closed at: waiting for the answers to buys in flight.
    pub closing: Option<f32>,
    last_stock: f32,
}

/// The interaction wheel's state: invitations, the wheel, the session going on.
#[derive(Resource, Default, Debug)]
pub struct Social {
    pub invites: Invites,
    pub wheel: Option<Wheel>,
    pub duel: Option<Duel>,
    pub trade: Option<TradeUi>,
    /// A line shown under the crosshair, until (real seconds).
    pub banner: Option<(String, f32)>,
    /// When the duel's sweep began (real seconds).
    pub sweep_at: Option<f32>,
    /// This player lost a duel and is to be knocked down and thrown (done by `duel_step`).
    fall: bool,
    /// This player lost a duel and is thrown with this velocity (game units a second):
    /// the knockdown's camera follows the body (`nvforever::player_knockdown`) and takes
    /// this.
    pub thrown: Option<[f32; 3]>,
    /// Attacks begun by this player as last seen (`combat::WeaponAnim::attacks`).
    attacks: Option<u32>,
    /// Caps to give back (buys the other could not honour), and sales to carry out
    /// (item, count, the caps for them all, checked: `trade::cost`).
    refund: u32,
    sales: Vec<(u32, u16, u32)>,
    /// A guest: since when its link to the host has been lost (real seconds).
    host_lost_since: Option<f32>,
    rng: Option<rand_chacha::ChaCha8Rng>,
    /// The wheel was open this frame: its Activate press is not also the prompt's
    /// ([`target`]).
    wheel_had_input: bool,
    /// The wheel closed and the attack and aim buttons are still down: they stay the
    /// wheel's until let go (the click that asked does not also fire).
    wheel_release: bool,
    stats: Stats,
}

#[derive(Debug, Default)]
struct Stats {
    wheels: u32,
    invites: u32,
    accepted: u32,
    duels: u32,
    trades: u32,
    sold: u32,
    bought: u32,
}

impl Social {
    /// The wheel or the trade's screen has the mouse and the keys.
    pub fn holds_input(&self) -> bool {
        self.wheel.is_some() || self.trade.as_ref().is_some_and(|t| t.closing.is_none())
    }

    /// The wheel closed this frame (Esc, a click, a key): that press is the wheel's, so
    /// the pause menu does not also open on it (`crate::pause`).
    pub fn closed_now(&self) -> bool {
        self.wheel_had_input && self.wheel.is_none()
    }

    /// The duel's countdown: this player is held still, its view too.
    pub fn frozen(&self) -> bool {
        self.duel.as_ref().is_some_and(Duel::frozen)
    }

    /// Hits on player `id` do nothing now: it is this player's duel opponent.
    pub fn spares(&self, id: u8) -> bool {
        self.duel.as_ref().is_some_and(|d| d.with == id && d.harmless())
    }

    /// Busy with another player: invitations are declined by themselves.
    pub fn busy(&self) -> bool {
        self.duel.is_some() || self.trade.is_some()
    }

    /// The image-space look of the duel's signal, as VATS mode's ([`crate::vats_mode::VatsMode::look`]).
    pub fn sweep_look(&self, now: f32) -> [f32; 4] {
        sweep_look(self.sweep_at.map(|t| now - t))
    }

    fn say(&mut self, line: impl Into<String>, now: f32) {
        let line = line.into();
        info!("co-op social: on screen: {line:?}");
        self.banner = Some((line, now + BANNER_SECONDS));
    }

    fn rng(&mut self) -> &mut rand_chacha::ChaCha8Rng {
        self.rng.get_or_insert_with(|| {
            let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64);
            rand_chacha::ChaCha8Rng::seed_from_u64(seed)
        })
    }
}

/// Test aid `NVF_PLAY_TEST_SWEEP_SHOTS=<folder>[:<seconds>,...]`: screenshots of the duel's
/// signal to draw, each so many real seconds after it (default: 0.05, 0.15, 0.25, 0.35,
/// 0.6, 0.9, 1.5), saved as `sweep-<seconds>.png` in the folder, so a run catches the
/// flash whenever the random countdown ends.
fn sweep_shots(social: Res<Social>, real: Res<Time<Real>>, mut commands: Commands, mut seen: Local<Option<f32>>, mut due: Local<Vec<(f32, std::path::PathBuf)>>) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Some(spec) = std::env::var_os("NVF_PLAY_TEST_SWEEP_SHOTS") else { return };
    let spec = spec.to_string_lossy().into_owned();
    let now = real.elapsed_secs();
    if social.sweep_at.is_some() && social.sweep_at != *seen {
        *seen = social.sweep_at;
        // A drive letter's colon is part of the folder: the times follow the last colon.
        let (dir, times) = match spec.rsplit_once(':').filter(|(_, t)| t.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')) {
            Some((d, t)) if !t.is_empty() => (d.to_string(), t.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>()),
            _ => (spec.clone(), vec![0.05, 0.15, 0.25, 0.35, 0.6, 0.9, 1.5]),
        };
        let at = social.sweep_at.unwrap_or(now);
        for t in times {
            due.push((at + t, std::path::Path::new(&dir).join(format!("sweep-{t:.2}.png"))));
        }
        info!("test aid: sweep shots: the signal to draw at {at:.2}, {} shots to {dir}", due.len());
    }
    let (fire, keep): (Vec<_>, Vec<_>) = due.drain(..).partition(|(t, _)| *t <= now);
    *due = keep;
    for (_, path) in fire {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        info!("test aid: sweep shot {} (look {:?})", path.display(), social.sweep_look(now));
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// The sweep `since` real seconds after the signal: the swipe across, then the amber
/// grade fading (VATS mode's look: edge, band, wash, grade).
pub fn sweep_look(since: Option<f32>) -> [f32; 4] {
    match since {
        Some(t) if (0.0..SWEEP_SECONDS).contains(&t) => crate::vats_mode::look_at(true, Some(t), None),
        Some(t) if (SWEEP_SECONDS..SWEEP_SECONDS + SWEEP_FADE).contains(&t) => {
            let f = (t - SWEEP_SECONDS) / SWEEP_FADE;
            [2.0, 0.0, crate::vats_mode::WASH_START * (1.0 - f), 1.0 - f]
        }
        _ => [0.0; 4],
    }
}

/// Where along the view ray (`eye`, `dir` unit) it passes through a standing body at
/// `feet`, if it does within `reach`.
pub fn ray_meets_body(eye: [f32; 3], dir: [f32; 3], feet: [f32; 3], reach: f32) -> Option<f32> {
    let flat = dir[0] * dir[0] + dir[1] * dir[1];
    let (fx, fy) = (feet[0] - eye[0], feet[1] - eye[1]);
    // Straight down or up: the body's axis is met only from right above it.
    let t = if flat < 1e-6 { fx.hypot(fy).le(&BODY_RADIUS).then_some(0.0)? } else { (fx * dir[0] + fy * dir[1]) / flat };
    let t = t.max(0.0);
    let (px, py, pz) = (eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t);
    let off = (px - feet[0]).hypot(py - feet[1]);
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
    let along = t * len;
    (off <= BODY_RADIUS && pz >= feet[2] - 4.0 && pz <= feet[2] + BODY_HEIGHT && along <= reach).then_some(along)
}

/// The co-op rows of the settings page (Gameplay > Co-op): greyed, their values kept,
/// while the Co-op switch is off (`crate::lobby::CoopSwitch`) [design: what only a co-op
/// game uses].
pub const COOP_ROWS: [&str; 4] = ["name-tags", "name-tag-near", "name-tag-far", "kill-in-conversation"];

/// Grey [`COOP_ROWS`] while the Co-op switch is off.
pub fn grey_outside_coop(app: &mut App) {
    for key in COOP_ROWS {
        crate::settings::set_disabled_status(app, key, |w| (!w.get_resource::<crate::lobby::CoopSwitch>().is_some_and(|s| s.on)).then_some("Needs the Co-op switch on."));
    }
}

pub struct CoopSocialPlugin;

impl Plugin for CoopSocialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Social>()
            .init_resource::<Tables>()
            .init_resource::<SocialMates>()
            .init_resource::<AidShoot>()
            .add_systems(Update, social_aid.in_set(PlaySet::Input).before(input))
            .add_systems(Update, sweep_shots)
            .add_systems(Update, social_aid_trigger.in_set(PlaySet::Input).after(crate::control::read_controls))
            .add_systems(Update, input.in_set(PlaySet::Input).after(crate::weapon_wheel::WheelRead).after(crate::flashlight::menu_key).before(crate::control::read_controls))
            .add_systems(Update, hold.after(crate::control::enforce_locks).after(PlaySet::Input).before(PlaySet::Physics))
            .add_systems(Update, target.in_set(PlaySet::Activation).after(crate::activate::find_target).before(crate::activate::activate))
            .add_systems(Update, (talk, duel_step, trade_step).chain().in_set(PlaySet::Ui))
            .add_systems(Update, trade_screen.in_set(PlaySet::Ui).after(crate::menu::MenuSystems).after(trade_step))
            .add_systems(Last, report);
        draw::build(app);
    }

    /// The rows are added by their plugins' `build` (`crate::world_marks`,
    /// `crate::coop_talk`); greyed once they are all there.
    fn finish(&self, app: &mut App) {
        grey_outside_coop(app);
    }
}

/// Keys and the mouse while the wheel or the trade panel is open, and the Y/N of an
/// invitation; the game's own controls stay off meanwhile (as the weapon wheel's,
/// `PlayerControl::wheel`), and while the duel's countdown holds the player.
#[allow(clippy::too_many_arguments)]
fn input(
    input: ControlInput,
    motion: Res<AccumulatedMouseMotion>,
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut social: ResMut<Social>,
    mut control: ResMut<crate::control::PlayerControl>,
    (mut tables, mates, mut menu_key): (ResMut<Tables>, Res<SocialMates>, Option<ResMut<crate::flashlight::MenuKeyHold>>),
) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let playing = *state.get() == PlayState::Playing;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    let keys = &input.keys;
    // The invitation on screen.
    if playing && s.wheel.is_none() && s.invites.shown().is_some() {
        if let Some(yes) = answer(&input) {
            let shown = *s.invites.shown().unwrap();
            info!("co-op social: {} the invitation of player {} ({})", if yes { "accepted" } else { "declined" }, shown.with + 1, shown.kind.label());
            if let Some(Settled::Accepted { with, kind, nonce, inviter }) = s.invites.answer(yes, &mut out) {
                start(s, with, kind, nonce, inviter, &mates, now, &mut out);
            }
        }
    }
    // The wheel.
    // The pointer's travel in units of the wheel as drawn, at the UI scale (as the weapon wheel's).
    let k = windows.single().map_or(1.0, |w| 960.0 / w.height().max(1.0)) / crate::interface::unit_scale();
    s.wheel_had_input = s.wheel.is_some();
    let stepped = s.wheel.as_mut().map(|w| {
        let moved = if playing { motion.delta * k } else { Vec2::ZERO };
        // On a controller the right stick points, as on the weapon wheel (y down).
        let stick = (playing && input.bindings.device == crate::bindings::InputDevice::Controller).then(|| {
            let s = crate::pad::stick(&input.pads, false);
            Vec2::new(s.x, -s.y)
        });
        let choose = playing && (input.mouse.just_pressed(MouseButton::Left) || input.just_pressed(Control::Activate));
        // Menu Mode (Tab, or B on a controller) backs out, as Esc and a right click do.
        let back = input.just_pressed(Control::MenuMode);
        let cancel = !playing || back || input.mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) || mate(&mates, w.with).is_none_or(|m| !m.here || m.lost);
        (w.step(moved, stick, None, choose, cancel), w.with, w.name.clone())
    });
    // The Menu Mode press that closed the wheel is the wheel's: used up, so its release
    // opens no Pip-Boy and its hold switches no flashlight (`crate::flashlight`), as VATS
    // mode does with it [design: B and Tab back out of the wheel without opening the Pip-Boy].
    if s.wheel_had_input && input.just_pressed(Control::MenuMode) {
        if let Some(hold) = menu_key.as_deref_mut() {
            hold.consume();
        }
    }
    match stepped {
        Some((wheel::Step::Cancelled, ..)) => {
            info!("co-op social: wheel closed: nothing chosen");
            s.wheel = None;
        }
        Some((wheel::Step::Refused(why), ..)) => s.say(why, now),
        Some((wheel::Step::Chose(kind), with, name)) => {
            s.wheel = None;
            match mates.me.map(|me| s.invites.send(me, with, kind, now, &mut out)) {
                Some(Ok(_)) => {
                    s.stats.invites += 1;
                    info!("co-op social: wheel: {} asked of {name} (player {})", kind.label(), with + 1);
                    s.say(trf("Waiting for {name} to answer…", &[("name", &name)]), now);
                }
                Some(Err(why)) => s.say(trf("Still {why}", &[("why", &why)]), now),
                None => {}
            }
        }
        _ => {}
    }
    // Closed by a click or a key: that press is not the game's, nor is the button held on.
    if s.closed_now() {
        s.wheel_release = true;
    }
    if s.wheel_release && !input.pressed(Control::Use) && !input.pressed(Control::Block) && !input.mouse.pressed(MouseButton::Left) && !input.mouse.pressed(MouseButton::Right) {
        s.wheel_release = false;
    }
    if s.holds_input() || s.frozen() || s.wheel_release {
        control.wheel = true;
        (control.trigger, control.aiming) = (false, false);
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

/// The answer to the question on screen this frame: Y or Enter yes, N or Esc no (as the
/// pause menu's questions, `crate::pause`); on a controller Y yes and X no [design].
fn answer(input: &ControlInput) -> Option<bool> {
    let pad = |b: GamepadButton| input.pads.iter().any(|p| p.just_pressed(b));
    if input.keys.any_just_pressed([KeyCode::KeyY, KeyCode::Enter, KeyCode::NumpadEnter]) || pad(GamepadButton::North) {
        Some(true)
    } else if input.keys.any_just_pressed([KeyCode::KeyN, KeyCode::Escape]) || pad(GamepadButton::West) {
        Some(false)
    } else {
        None
    }
}

/// A step of `NVF_PLAY_TEST_SOCIAL` (module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
enum AidStep {
    Face,
    /// Face the other player, only once the duel's signal came, turning smoothly.
    DrawFace,
    /// As [`AidStep::DrawFace`], then shoot while the draw lasts.
    DrawShoot,
    /// Turn the back to the other player, smoothly.
    TurnBack,
    Pick(Kind),
}

/// How long the test aid's `drawface` turn takes (seconds).
const AID_TURN_SECONDS: f32 = 0.3;
/// How long the test aid's `turnback` turn takes (seconds).
const AID_TURNBACK_SECONDS: f32 = 0.8;
/// The test aid's `drawshoot`: the trigger held this long of every [`AID_SHOT_EVERY`]
/// seconds (a semi-automatic pistol fires once a pull).
const AID_SHOT_SECONDS: f32 = 0.12;
const AID_SHOT_EVERY: f32 = 0.35;
/// The test aid's `drawshoot` waits this long once turned before its first shot (a
/// player's reaction; the throw then shows after the signal's sweep).
const AID_SHOT_REACTION: f32 = 0.6;

/// A test aid turn under way: since (aid seconds), how long, from and to (heading,
/// pitch), and whether `drawshoot` shoots once it ends.
type AidTurn = (f32, f32, [f32; 2], [f32; 2], bool);

/// When the test aid's `drawshoot` began shooting (aid seconds), while the draw lasts.
#[derive(Resource, Default)]
struct AidShoot(Option<f32>);

fn aid_steps(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, rest)| {
            let step = match rest {
                "face" => AidStep::Face,
                "drawface" => AidStep::DrawFace,
                "drawshoot" => AidStep::DrawShoot,
                "turnback" => AidStep::TurnBack,
                "pick=duel" => AidStep::Pick(Kind::Duel),
                "pick=trade" => AidStep::Pick(Kind::Trade),
                _ => return Err(format!("{rest:?}: expected face, drawface, drawshoot, turnback or pick=duel|trade")),
            };
            Ok((t, step))
        })
        .collect()
}

/// `NVF_PLAY_TEST_SOCIAL` (module docs).
fn social_aid(
    time: crate::pause::AidTime,
    mut steps: Local<Option<Vec<(f32, AidStep)>>>,
    (mut turn, mut shoot): (Local<Option<AidTurn>>, ResMut<AidShoot>),
    mut social: ResMut<Social>,
    mates: Res<SocialMates>,
    player: Option<ResMut<Player>>,
    control: Res<crate::control::PlayerControl>,
) {
    let steps = steps.get_or_insert_with(|| match std::env::var("NVF_PLAY_TEST_SOCIAL") {
        Ok(v) => aid_steps(&v).unwrap_or_else(|e| {
            warn!("test aid: NVF_PLAY_TEST_SOCIAL: {e}");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    });
    if steps.is_empty() {
        return;
    }
    let (Some(now), Some(mut player)) = (time.world_secs(), player) else { return };
    // A `drawface` or `turnback` turn under way: heading along the shorter way round,
    // pitch straight.
    if let Some((since, secs, from, to, then_shoot)) = *turn {
        let k = ((now - since) / secs).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let dh = (to[0] - from[0] + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        (player.heading, player.pitch) = (from[0] + dh * k, from[1] + (to[1] - from[1]) * k);
        if k >= 1.0 {
            *turn = None;
            if then_shoot && shoot.0.is_none() {
                info!("test aid: social drawshoot: shooting");
                shoot.0 = Some(now + AID_SHOT_REACTION);
            }
        }
    }
    let due: Vec<AidStep> = steps.iter().filter(|(t, _)| *t <= now).map(|&(_, s)| s).collect();
    steps.retain(|(t, _)| *t > now);
    for step in due {
        match step {
            AidStep::DrawFace | AidStep::DrawShoot => {
                let drawing = social.duel.as_ref().is_some_and(Duel::drawing);
                let near = mates.mates.iter().filter(|m| m.here && !m.lost).min_by(|a, b| dist2(a.feet, player.feet).total_cmp(&dist2(b.feet, player.feet)));
                if let (true, None, Some(m)) = (drawing, *turn, near) {
                    let chest = [m.feet[0], m.feet[1], m.feet[2] + BODY_HEIGHT * 0.6];
                    let (h, p) = crate::control::Crosshair::aim_at(control.crosshair.as_ref(), player.eye(), chest);
                    if (h - player.heading).abs() > 1e-3 || (p - player.pitch).abs() > 1e-3 {
                        info!("test aid: social drawface: turning to {} over {AID_TURN_SECONDS} s", m.name);
                        *turn = Some((now, AID_TURN_SECONDS, [player.heading, player.pitch], [h, p], step == AidStep::DrawShoot));
                    } else if step == AidStep::DrawShoot && shoot.0.is_none() {
                        shoot.0 = Some(now + AID_SHOT_REACTION);
                    }
                }
            }
            AidStep::TurnBack => {
                let near = mates.mates.iter().filter(|m| m.here && !m.lost).min_by(|a, b| dist2(a.feet, player.feet).total_cmp(&dist2(b.feet, player.feet)));
                // Only while a duel is getting into position, the two far enough apart, and
                // not turned away already: a step given again and again turns once, in time.
                let positioning = social.duel.as_ref().is_some_and(|d| d.phase == duel::Phase::Positioning);
                let apart = |m: &&Mate| dist2(m.feet, player.feet) >= duel::APART.0 * duel::APART.0;
                let Some(m) = near.filter(|m| positioning && turn.is_none() && apart(m)) else { continue };
                let away = (player.feet[0] - m.feet[0]).atan2(player.feet[1] - m.feet[1]).rem_euclid(std::f32::consts::TAU);
                let off = (away - player.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                if off.abs() < 0.2 {
                    continue;
                }
                info!("test aid: social turnback: from {} over {AID_TURNBACK_SECONDS} s, heading {:.0}", m.name, away.to_degrees());
                *turn = Some((now, AID_TURNBACK_SECONDS, [player.heading, player.pitch], [away, player.pitch], false));
            }
            AidStep::Face => {
                let near = mates.mates.iter().filter(|m| m.here && !m.lost).min_by(|a, b| dist2(a.feet, player.feet).total_cmp(&dist2(b.feet, player.feet)));
                let Some(m) = near else {
                    info!("test aid: social face: nobody here");
                    continue;
                };
                let chest = [m.feet[0], m.feet[1], m.feet[2] + BODY_HEIGHT * 0.6];
                (player.heading, player.pitch) = crate::control::Crosshair::aim_at(control.crosshair.as_ref(), player.eye(), chest);
                info!("test aid: social face {} at ({:.0}, {:.0}, {:.0}): heading {:.1}", m.name, m.feet[0], m.feet[1], m.feet[2], player.heading.to_degrees().rem_euclid(360.0));
            }
            AidStep::Pick(kind) => match social.wheel.as_mut() {
                Some(w) => {
                    w.picked = Kind::ALL.iter().position(|&k| k == kind).unwrap_or(0);
                    info!("test aid: social pick {}", kind.label());
                }
                None => info!("test aid: social pick {}: no wheel open", kind.label()),
            },
        }
    }
}

/// The test aid's `drawshoot` pulls the trigger while the draw lasts (module docs).
fn social_aid_trigger(time: crate::pause::AidTime, mut shoot: ResMut<AidShoot>, social: Res<Social>, mut control: ResMut<crate::control::PlayerControl>) {
    let Some(since) = shoot.0 else { return };
    if !social.duel.as_ref().is_some_and(Duel::drawing) {
        info!("test aid: social drawshoot: the draw is over, no more shots");
        shoot.0 = None;
        return;
    }
    let Some(now) = time.world_secs() else { return };
    control.trigger = control.weapon_out && now >= since && (now - since).rem_euclid(AID_SHOT_EVERY) < AID_SHOT_SECONDS;
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn caps_of(sim: &SimState) -> u32 {
    sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(fnv_game::caravan::CAPS).max(0) as u32)
}

/// This player's tradeable stock: its inventory but caps, equipped items and quest items;
/// then its caps (the other's screen shows them as this player's caps figure).
fn my_stock(sim: &SimState) -> Vec<(u32, u16)> {
    let Some(inv) = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
    let equipped = inv.equipped();
    let items = inv.iter().filter(|(f, _)| !sim.forms.item(*f).is_some_and(|i| i.quest_item)).map(|(f, n)| (f.0, n, equipped.iter().filter(|e| **e == f).count() as i32)).collect::<Vec<_>>();
    let mut out = trade::tradeable(items, fnv_game::caravan::CAPS.0);
    out.push((fnv_game::caravan::CAPS.0, caps_of(sim).min(u16::MAX as u32) as u16));
    out
}

fn close_trade(s: &mut Social, now: f32) {
    if let Some(t) = s.trade.as_mut() {
        t.closing.get_or_insert(now);
    }
}

/// An accepted invitation: its session starts here.
#[allow(clippy::too_many_arguments)]
fn start(s: &mut Social, with: u8, kind: Kind, nonce: u16, inviter: bool, mates: &SocialMates, now: f32, out: &mut Vec<(u8, Msg)>) {
    s.stats.accepted += 1;
    let name = name_of(mates, with);
    let me = mates.me.unwrap_or(0);
    info!("co-op social: {} with {name} (player {}) starts ({})", kind.label(), with + 1, if inviter { "this player asked" } else { "they asked" });
    match kind {
        Kind::Duel => {
            s.stats.duels += 1;
            s.duel = Some(Duel::new(me, with, nonce, inviter, now));
            s.attacks = None;
        }
        Kind::Trade => {
            s.stats.trades += 1;
            let book = Trade::new(with, nonce);
            out.push((with, book.browse()));
            let puppet = mate(mates, with).map_or(0, |m| m.reference);
            s.trade = Some(TradeUi { book, name, puppet, opened: false, event: None, closing: None, last_stock: f32::MIN });
        }
    }
}

/// The social state and combat `target` reads.
type TargetState<'w> = (ResMut<'w, Social>, Option<Res<'w, crate::combat::Combat>>);

/// The Interact prompt on another player in reach, and E on it opens the wheel.
#[allow(clippy::too_many_arguments)]
fn target(
    input: ControlInput,
    real: Res<Time<Real>>,
    player: Option<Res<Player>>,
    mut phys: Option<ResMut<crate::phys::PhysState>>,
    control: Res<crate::control::PlayerControl>,
    mates: Res<SocialMates>,
    mut found: ResMut<crate::activate::Target>,
    (mut social, combat): TargetState,
    vats: Option<Res<crate::vats_mode::VatsMode>>,
) {
    let Some(player) = player else { return };
    // VATS has Activate for itself (Accept plays the queued attacks), as for the world's
    // activation (`crate::activate`): no Talk prompt and no wheel on the other player
    // then [observed: buckallobill, in game, NVForever co-op: the host's Accept also
    // opened the wheel on the guest while the attacks played].
    if mates.me.is_none() || social.holds_input() || vats.is_some_and(|v| v.holds_input()) {
        return;
    }
    let (eye, dir) = crate::control::Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());
    let wall = phys.as_deref_mut().and_then(|p| {
        let mut f = p.player.as_ref().map_or_else(fnv_phys::Filter::default, |c| fnv_phys::Filter::excluding(c.key()));
        f.exclude_layers.insert(fnv_phys::layers::TRIGGER);
        p.world.queries().ray(eye, dir, INTERACT_REACH, &f).filter(|h| !matches!(h.owner, fnv_phys::Owner::Actor { .. })).map(|h| h.distance)
    });
    let near = mates
        .mates
        .iter()
        .filter(|m| m.here && !m.lost && !m.down)
        .filter_map(|m| ray_meets_body(eye, dir, m.feet, INTERACT_REACH).filter(|d| wall.is_none_or(|w| *d <= w + 8.0)).map(|d| (d, m)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    // An invitation waiting for Y or N, or a duel under way: the message box stands where
    // the prompt would (`draw`, mockup 1B), and E there would only open the wheel over it.
    let near = near.filter(|_| social.invites.shown().is_none() && social.duel.is_none());
    let Some((d, m)) = near else {
        // A stand-in actor the world's targeting found (the host): not Talk.
        if found.0.as_ref().is_some_and(|t| mates.mates.iter().any(|m| m.reference == t.form_id)) {
            found.0 = None;
        }
        return;
    };
    // Something nearer than the other player keeps the prompt.
    if found.0.as_ref().is_some_and(|t| t.distance + 8.0 < d && t.form_id != m.reference) {
        return;
    }
    let t = crate::activate::Targeted { form_id: m.reference, name: m.name.clone(), action: crate::activate::outcome::Action::Player, distance: d, source: crate::activate::pick::Source::Collision, crime: false, empty: false, through: None };
    if found.0.as_ref() != Some(&t) {
        found.0 = Some(t);
    }
    if !input.just_pressed(Control::Activate) || social.frozen() || social.wheel_had_input {
        return;
    }
    let now = real.elapsed_secs();
    if social.busy() {
        social.say(tr("Finish what you are doing first"), now);
        return;
    }
    let mut w = Wheel::new(m.id, m.name.clone());
    let mut clear_seen = [0.0f32; 8];
    let room = phys.as_deref_mut().is_some_and(|p| {
        let mut f = p.player.as_ref().map_or_else(fnv_phys::Filter::default, |c| fnv_phys::Filter::excluding(c.key()));
        f.exclude_layers.insert(fnv_phys::layers::TRIGGER);
        let from = [player.feet[0], player.feet[1], player.feet[2] + 60.0];
        let clear: [f32; 8] = std::array::from_fn(|i| {
            let a = (i as f32 * 45.0).to_radians();
            let d = [a.sin(), a.cos(), 0.0];
            // People are no walls: actors' hit volumes and characters' capsules (the other
            // player's puppet among them) do not count.
            let hit = p.world.queries().ray(from, d, duel::ROOM_CLEAR, &f).filter(|h| !matches!(h.owner, fnv_phys::Owner::Actor { .. } | fnv_phys::Owner::Character { .. }));
            if let Some(h) = hit.as_ref().filter(|h| h.distance < duel::ROOM_CLEAR) {
                info!("co-op social: room ray {}°: {:?} at {:.0}", i * 45, h.owner, h.distance);
            }
            hit.map_or(duel::ROOM_CLEAR, |h| h.distance)
        });
        clear_seen = clear;
        duel::room_for_duel(mates.interior, &clear)
    });
    let pistol = pistol_in_hand(combat.as_deref());
    w.refusals[0] = duel::duel_refusal(room, pistol).map(str::to_string);
    info!("co-op social: wheel open on {} (player {}); duel: room {room} (clear N..NW {:?}, indoors {}), pistol {pistol}", m.name, m.id + 1, clear_seen.map(|d| d.round() as i32), mates.interior);
    social.stats.wheels += 1;
    social.wheel = Some(w);
}

/// The player has a pistol equipped.
fn pistol_in_hand(combat: Option<&crate::combat::Combat>) -> bool {
    combat.is_some_and(|c| c.player_weapon().filter(|w| Some(*w) != c.fists_id()).and_then(|w| c.player_stats(w)).is_some_and(|s| duel::is_pistol(s.anim_type.as_deref())))
}

/// The duel's countdown holds the player still; the trade panel too (its keys are the
/// panel's).
fn hold(social: Res<Social>, mut intent: ResMut<MoveIntent>, mut control: ResMut<crate::control::PlayerControl>) {
    if !social.frozen() && !social.trade.as_ref().is_some_and(|t| t.closing.is_none()) {
        return;
    }
    let (mut i, mut c) = (*intent, *control);
    (i.forward, i.right, i.jump) = (0.0, 0.0, false);
    (c.trigger, c.aiming, c.reload, c.ammo_swap, c.ready_held, c.auto_move) = (false, false, false, false, None, false);
    intent.set_if_neq(i);
    control.set_if_neq(c);
}

/// The messages heard, the invitations' timeouts, the sessions of players gone.
fn talk(real: Res<Time<Real>>, mut tables: ResMut<Tables>, mut social: ResMut<Social>, mates: Res<SocialMates>, sim: Res<SimState>) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    let Some(me) = tables.me.filter(|_| tables.on) else {
        // Out of co-op: everything ends.
        if s.duel.is_some() || s.trade.is_some() || s.wheel.is_some() || s.invites.out.is_some() || !s.invites.asked.is_empty() {
            info!("co-op social: the session ended: everything with the others is off");
            let stats = std::mem::take(&mut s.stats);
            *s = Social { stats, ..Default::default() };
        }
        return;
    };
    let actions = tables.take_actions(game::SOCIAL);
    for (from, to, blob) in actions {
        if to != me as u32 {
            continue;
        }
        let Some(m) = Msg::decode(&blob) else {
            warn!("co-op social: player {} sent a message this game cannot read", from + 1);
            continue;
        };
        let nonce = m.nonce();
        match m {
            Msg::Invite { kind, nonce } => {
                let busy = s.busy();
                let shown_before = s.invites.asked.len();
                if let Some(Settled::Accepted { with, kind, nonce, inviter }) = s.invites.heard(me, from, kind, nonce, now, busy, &mut out) {
                    start(s, with, kind, nonce, inviter, &mates, now, &mut out);
                } else if s.invites.asked.len() > shown_before {
                    info!("co-op social: player {} asks: {}", from + 1, invite::invite_line(&name_of(&mates, from), kind));
                } else if busy {
                    info!("co-op social: player {}'s invitation declined: this player is busy", from + 1);
                }
            }
            Msg::Answer { nonce, yes } => match s.invites.answered(from, nonce, yes) {
                Some(Settled::Accepted { with, kind, nonce, inviter }) => start(s, with, kind, nonce, inviter, &mates, now, &mut out),
                Some(Settled::Declined { with, kind }) => {
                    let line = trf("{name} declined {what}", &[("name", &name_of(&mates, with)), ("what", &invitation_word(kind))]);
                    s.say(line, now);
                }
                _ => {}
            },
            Msg::Quit { nonce } if s.invites.asked.iter().any(|i| i.nonce == nonce && i.with == from) => {
                s.invites.withdrawn(from, nonce);
            }
            Msg::Thrown { velocity, .. } => {
                let velocity = velocity.map(|v| v as f32);
                info!("co-op social: player {} was thrown at ({:.0}, {:.0}, {:.0}) units/s in its own game", from + 1, velocity[0], velocity[1], velocity[2]);
            }
            m @ (Msg::Quit { .. } | Msg::DuelReady { .. } | Msg::DuelFreeze { .. } | Msg::DuelHit { .. } | Msg::DuelOver { .. }) if s.duel.as_ref().is_some_and(|d| d.nonce == nonce && d.with == from) => {
                let d = s.duel.as_mut().unwrap();
                let outs = d.heard(&m, now);
                duel_outs(s, outs, &mates, now, &mut out);
            }
            Msg::Quit { nonce } if s.trade.as_ref().is_some_and(|t| t.book.nonce == nonce && t.book.with == from) => {
                let line = trf("{name} left the trade", &[("name", &name_of(&mates, from))]);
                s.say(line, now);
                close_trade(s, now);
            }
            m if s.trade.as_ref().is_some_and(|t| t.book.nonce == nonce && t.book.with == from) => trade_heard(s, m, &sim, now, &mut out),
            _ => debug!("co-op social: a message of a session that is over: {m:?}"),
        }
    }
    for st in s.invites.step(now, &mut out) {
        match st {
            Settled::Lapsed { with, kind, mine: true } => {
                let line = trf("{name} did not answer {what}", &[("name", &name_of(&mates, with)), ("what", &invitation_word(kind))]);
                s.say(line, now);
            }
            Settled::Lapsed { with, .. } => info!("co-op social: player {}'s invitation lapsed", with + 1),
            _ => {}
        }
    }
    // A player gone (left, lost): its sessions end.
    let gone = |id: u8| mate(&mates, id).is_none_or(|m| m.lost);
    // A duel ends when either player goes. A link lost looks the same from both ends, so
    // the session decides: the host stays, and a guest that has lost the host for
    // DROPPED_SECONDS (or has no session left) is the one who disconnected.
    let host_lost = mates.me.is_none() || (mates.me != Some(0) && mate(&mates, 0).is_some_and(|m| m.lost));
    s.host_lost_since = if host_lost { s.host_lost_since.or(Some(now)) } else { None };
    let dropped = mates.me.is_none() || s.host_lost_since.is_some_and(|t| now - t >= DROPPED_SECONDS);
    let left = |id: u8| mates.me.is_some() && mate(&mates, id).is_none();
    for i in s.invites.asked.iter().map(|i| i.with).chain(s.invites.out.map(|o| o.with)).filter(|&i| gone(i)).collect::<Vec<_>>() {
        s.invites.gone(i);
    }
    if let Some(d) = s.duel.as_mut().filter(|d| (dropped || left(d.with)) && !d.done()) {
        let outs = if dropped {
            info!("co-op social: duel with player {}: this player's link was lost", d.with + 1);
            d.this_dropped()
        } else {
            info!("co-op social: duel with player {}: they left", d.with + 1);
            d.other_gone()
        };
        duel_outs(s, outs, &mates, now, &mut out);
        s.duel = None;
    }
    if s.trade.as_ref().is_some_and(|t| gone(t.book.with) && t.closing.is_none()) {
        close_trade(s, now);
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

fn name_of(mates: &SocialMates, id: u8) -> String {
    mate(mates, id).map_or_else(|| trf("Player {n}", &[("n", &(id + 1))]), |m| m.name.clone())
}

fn invitation_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Duel => tr("your challenge"),
        Kind::Trade => tr("your offer to trade"),
    }
}

/// What the duel asked for.
fn duel_outs(s: &mut Social, outs: Vec<Out>, mates: &SocialMates, now: f32, out: &mut Vec<(u8, Msg)>) {
    let Some(with) = s.duel.as_ref().map(|d| d.with) else { return };
    for o in outs {
        match o {
            Out::Send(m) => out.push((with, m)),
            Out::Say(line) => s.say(line, now),
            Out::Sweep => {
                info!("co-op social: duel: the signal to draw");
                s.sweep_at = Some(now);
            }
            Out::Thrown => s.fall = true,
            Out::TheyFall => {
                info!("co-op social: duel: {} falls (it is knocked down in its own game)", name_of(mates, with));
            }
        }
    }
}

/// What [`duel_step`] reads and changes beyond the co-op state.
type DuelWorld<'w> = (Option<Res<'w, crate::combat::Combat>>, Option<Res<'w, crate::combat::WeaponAnim>>, ResMut<'w, SimState>, Option<ResMut<'w, crate::script::ScriptState>>);

/// The duel, each frame: the stance, the shots, the loser thrown.
#[allow(clippy::too_many_arguments)]
fn duel_step(
    real: Res<Time<Real>>,
    mut social: ResMut<Social>,
    mates: Res<SocialMates>,
    mut tables: ResMut<Tables>,
    player: Option<Res<Player>>,
    control: Res<crate::control::PlayerControl>,
    mut phys: Option<ResMut<crate::phys::PhysState>>,
    (combat, anim, mut sim, mut script): DuelWorld,
) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    let Some(player) = player else { return };
    if let Some(d) = s.duel.as_ref() {
        let with = d.with;
        let them = mate(&mates, with).cloned();
        let name = name_of(&mates, with);
        let pistol = pistol_in_hand(combat.as_deref());
        let stance = them.as_ref().map_or(duel::Stance::TooFar, |m| duel::stance(pistol, player.feet, player.heading, m.feet));
        let random = s.rng().next_u32();
        let d = s.duel.as_mut().unwrap();
        let outs = d.step(now, stance, &name, random);
        duel_outs(s, outs, &mates, now, &mut out);
        // Its shots: each new attack during the draw, along the aim, against the other's body.
        let attacks = anim.as_deref().map(|a| a.attacks);
        let fired = matches!((s.attacks, attacks), (Some(a), Some(b)) if b != a);
        s.attacks = attacks;
        if fired && s.duel.as_ref().is_some_and(Duel::drawing) {
            let (eye, dir) = crate::control::Crosshair::ray(control.crosshair.as_ref(), player.eye(), player.look_dir());
            let hit = them.as_ref().and_then(|m| ray_meets_body(eye, dir, m.feet, 20_000.0)).filter(|&d| {
                let wall = phys.as_deref_mut().and_then(|p| {
                    let mut f = p.player.as_ref().map_or_else(fnv_phys::Filter::default, |c| fnv_phys::Filter::excluding(c.key()));
                    f.exclude_layers.insert(fnv_phys::layers::TRIGGER);
                    p.world.queries().ray(eye, dir, d, &f).filter(|h| !matches!(h.owner, fnv_phys::Owner::Actor { .. })).map(|h| h.distance)
                });
                wall.is_none_or(|w| w + 8.0 >= d)
            });
            info!("co-op social: duel: a shot {}", hit.map_or("missed".to_string(), |d| format!("hit {name} at {d:.0}")));
            if hit.is_some() {
                let outs = s.duel.as_mut().unwrap().hit(now);
                duel_outs(s, outs, &mates, now, &mut out);
            }
        }
        if s.duel.as_ref().is_some_and(Duel::done) {
            info!("co-op social: duel with player {} is over", with + 1);
            s.duel = None;
        }
    }
    // This player lost: knocked down, thrown away from the winner.
    if std::mem::take(&mut s.fall) {
        let winner = s.duel.as_ref().and_then(|d| mate(&mates, d.with)).map_or(player.feet, |m| m.feet);
        let shove = duel::throw(player.feet, winner);
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            let down = crate::script::with_world(rt, &mut sim, |rt| rt.game.knock_down(FormId::PLAYER_REF, None, shove));
            s.thrown = down.then_some(shove);
            info!("co-op social: duel: this player is thrown back ({shove:?}): {}", if down { "knocked down" } else { "could not be knocked down" });
            // Every player's copy of this body is thrown the same.
            if down {
                let nonce = s.duel.as_ref().map_or(0, |d| d.nonce);
                for m in mates.mates.iter().filter(|m| Some(m.id) != mates.me) {
                    out.push((m.id, Msg::Thrown { nonce, velocity: shove.map(|v| v.round() as i32) }));
                }
            }
        }
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

/// A trade message heard.
fn trade_heard(s: &mut Social, m: Msg, sim: &SimState, now: f32, out: &mut Vec<(u8, Msg)>) {
    let Some(t) = s.trade.as_mut() else { return };
    let with = t.book.with;
    // What this player has up for trade (not what is worn, nor quest items: `my_stock`).
    let stock = my_stock(sim);
    let have = |item: u32| stock.iter().find(|(f, _)| *f == item).map_or(0, |&(_, n)| n);
    let mut say: Option<String> = None;
    match m {
        Msg::TradeBrowse { .. } => {
            let mine = my_stock(sim);
            out.extend(t.book.stock_msgs(&mine, true).into_iter().map(|m| (with, m)));
            t.last_stock = now;
        }
        Msg::TradeStock { page, pages, items, .. } => t.book.stock_heard(page, pages, items),
        Msg::TradeRequest { item, count, .. } => {
            let value = sim.forms.item(FormId(item)).and_then(|i| i.value).unwrap_or(1).max(1) as u32;
            t.book.request_heard(item, count, have(item), value);
            say = Some(trf("{name} asks for {count} x {item}: set your price", &[("name", &t.name), ("count", &count), ("item", &item_name(sim, item))]));
        }
        Msg::TradePrice { item, each, .. } => {
            t.book.price_heard(item, each);
            say = Some(if each == 0 {
                trf("{name} will not sell {item}", &[("name", &t.name), ("item", &item_name(sim, item))])
            } else {
                trf("{name} asks {each} caps each for {item}", &[("name", &t.name), ("each", &each), ("item", &item_name(sim, item))])
            });
        }
        // The other buys from this player: the items go, the caps come (`trade_step`; the
        // buyer gets the items when told).
        Msg::TradeBuy { item, count, each, .. } => {
            let (sale, reply) = t.book.buy_heard(item, count, each, have(item));
            out.push((with, reply));
            if let Sale::Sell { item, count, caps, .. } = sale {
                info!("co-op social: trade: sold {count} × {} to player {} for {caps} caps", FormId(item), with + 1);
                say = Some(trf("Sold {count} x {item} for {caps} caps", &[("count", &count), ("item", &item_name(sim, item)), ("caps", &caps)]));
                s.stats.sold += 1;
                s.sales.push((item, count, caps));
            } else {
                info!("co-op social: trade: refused player {}'s buy of {count} × {} at {each}", with + 1, FormId(item));
            }
        }
        Msg::TradeSold { item, count, each, .. } => {
            if let (true, Some(caps)) = (t.book.sold(item, count, each), trade::cost(each, count)) {
                info!("co-op social: trade: bought {count} × {} from player {}", FormId(item), with + 1);
                say = Some(trf("Bought {count} x {item} for {caps} caps", &[("count", &count), ("item", &item_name(sim, item)), ("caps", &caps)]));
                s.stats.bought += 1;
            }
        }
        Msg::TradeRefused { item, count, each, .. } => {
            if let Some(back) = t.book.refused(item, count, each) {
                info!("co-op social: trade: player {} could not sell {}: {back} caps back", with + 1, FormId(item));
                say = Some(trf("{name} could not sell that: {back} caps given back", &[("name", &t.name), ("back", &back)]));
                s.refund = s.refund.saturating_add(back);
            }
        }
        _ => {}
    }
    if let Some(line) = say {
        if let Some(t) = s.trade.as_mut() {
            t.event = Some((line.clone(), now));
        }
        s.say(line, now);
    }
}

fn item_name(sim: &SimState, item: u32) -> String {
    sim.name(FormId(item)).map_or_else(|| FormId(item).to_string(), str::to_string)
}

/// The trade's inventory moves (what the messages decided), its stock sent again when it
/// changes, and its end once the buys in flight are answered.
fn trade_step(real: Res<Time<Real>>, mut social: ResMut<Social>, mut tables: ResMut<Tables>, mut sim: ResMut<SimState>) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    // Caps given back.
    if s.refund > 0 {
        let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, s.refund.min(i32::MAX as u32) as i32);
        s.refund = 0;
    }
    // Sold: the items go and the caps come.
    for (item, count, caps) in std::mem::take(&mut s.sales) {
        let _ = sim.world.remove_item(FormId::PLAYER_REF, FormId(item), count as i32);
        let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, caps.min(i32::MAX as u32) as i32);
    }
    let Some(t) = s.trade.as_mut() else { return };
    // Bought: the items come, once (the ask is done with).
    for a in t.book.asks.iter_mut().filter(|a| a.state == trade::AskState::Bought) {
        // Only an item this game knows (`sim.forms.item`): a made-up one is not added.
        if sim.forms.item(FormId(a.item)).is_some() {
            let _ = sim.world.add_item(FormId::PLAYER_REF, FormId(a.item), a.count as i32);
        }
        a.count = 0;
    }
    t.book.asks.retain(|a| a.count > 0);
    if t.closing.is_none() && now - t.last_stock >= 1.0 {
        t.last_stock = now;
        let mine = my_stock(&sim);
        let with = t.book.with;
        out.extend(t.book.stock_msgs(&mine, false).into_iter().map(|m| (with, m)));
    }
    if let Some(since) = t.closing {
        let held = t.book.held();
        if held == 0 || now - since >= TRADE_LINGER {
            if held > 0 {
                info!("co-op social: trade: {held} caps of unanswered buys given back");
                let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, held.min(i32::MAX as u32) as i32);
            }
            s.trade = None;
        }
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

/// The trade's barter screen ([`barter`]): opened once the trade starts and no other menu
/// is up, its commands done on the book, its lists filled from it; shut with the trade.
fn trade_screen(
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    mut social: ResMut<Social>,
    mut menu: (ResMut<crate::menu::ActiveMenu>, ResMut<crate::menu::FreshMenu>),
    mut input: MessageReader<crate::menu::MenuInput>,
    mut tables: ResMut<Tables>,
    mut sim: ResMut<SimState>,
) {
    let now = real.elapsed_secs();
    let (active, fresh) = (&mut menu.0, &mut menu.1);
    let s = &mut *social;
    let ours = active.0.as_ref().is_some_and(|m| m.coop);
    let Some(t) = s.trade.as_mut().filter(|t| t.closing.is_none()) else {
        input.clear();
        if ours {
            info!("co-op social: the trade's barter screen closes");
            active.0 = None;
        }
        return;
    };
    let with = t.book.with;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    if !t.opened {
        input.clear();
        if active.0.is_none() && *state.get() == PlayState::Playing {
            info!("co-op social: trade with player {}: the barter screen opens", with + 1);
            active.0 = Some(barter::open(&t.name, t.puppet));
            fresh.0 = true;
            t.opened = true;
        }
    } else if !ours {
        // Shut under it (a conversation, a loading screen): the trade ends.
        input.clear();
        info!("co-op social: trade with player {}: the barter screen was shut, the trade ends", with + 1);
        out.push((with, Msg::Quit { nonce: t.book.nonce }));
        close_trade(s, now);
    } else {
        let mut close = false;
        for &crate::menu::MenuInput(cmd) in input.read() {
            let caps = caps_of(&sim);
            let Some(m) = active.bypass_change_detection().0.as_mut() else { break };
            let fx = barter::command(m, &mut t.book, cmd, caps);
            active.set_changed();
            for e in fx {
                match e {
                    barter::Effect::Send(msg) => {
                        info!("co-op social: trade: {cmd:?}: to player {}: {msg:?}", with + 1);
                        out.push((with, msg));
                    }
                    barter::Effect::TakeCaps(n) => {
                        let _ = sim.world.remove_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, n as i32);
                        info!("co-op social: trade: buying from player {} for {n} caps (held until they answer)", with + 1);
                    }
                    barter::Effect::Say(line) => {
                        info!("co-op social: trade: {cmd:?}: {line}");
                        t.event = Some((line, now));
                    }
                    barter::Effect::Close => close = true,
                }
            }
        }
        if close {
            info!("co-op social: the trade with player {} closed", with + 1);
            out.push((with, Msg::Quit { nonce: t.book.nonce }));
            active.0 = None;
            close_trade(s, now);
        } else {
            let mine = my_stock(&sim);
            if let Some(m) = active.bypass_change_detection().0.as_mut() {
                if barter::fill(m, &t.book, &t.name, &mine, sim.forms, t.event.as_ref(), now) {
                    active.set_changed();
                }
            }
        }
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

fn report(mut exit: MessageReader<AppExit>, social: Res<Social>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &social.stats;
    if s.wheels + s.invites + s.accepted == 0 {
        return;
    }
    info!(
        "co-op social summary: {} wheels opened, {} invitations sent, {} accepted; {} duels, {} trades ({} sold, {} bought)",
        s.wheels, s.invites, s.accepted, s.duels, s.trades, s.sold, s.bought
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_footer_names_the_device_in_use() {
        use crate::bindings::{Bindings, InputDevice};
        let mut b = Bindings::default();
        b.device = InputDevice::KeyboardMouse;
        let keys = draw::footer(Some(&b));
        assert!(keys.starts_with("Mouse: pick") && keys.contains("Esc or Tab: cancel"), "{keys}");
        b.device = InputDevice::Controller;
        let pad = draw::footer(Some(&b));
        assert!(pad.starts_with("RS: pick") && pad.ends_with(": back") && !pad.contains("Mouse"), "{pad}");
    }

    #[test]
    fn the_social_aid_reads_its_steps() {
        assert_eq!(aid_steps("3:face, 4.5:pick=trade, 6:drawface, 7:turnback, 8:drawshoot").unwrap(), vec![(3.0, AidStep::Face), (4.5, AidStep::Pick(Kind::Trade)), (6.0, AidStep::DrawFace), (7.0, AidStep::TurnBack), (8.0, AidStep::DrawShoot)]);
        assert!(aid_steps("3:wave").is_err());
    }

    #[test]
    fn the_crosshair_meets_a_body() {
        let eye = [0.0, 0.0, 120.0];
        // Looking north at a player 150 units away.
        assert!(ray_meets_body(eye, [0.0, 1.0, 0.0], [0.0, 150.0, 0.0], INTERACT_REACH).is_some_and(|d| (d - 150.0).abs() < 1.0));
        assert_eq!(ray_meets_body(eye, [0.0, 1.0, 0.0], [0.0, 400.0, 0.0], INTERACT_REACH), None, "out of reach");
        assert_eq!(ray_meets_body(eye, [0.0, 1.0, 0.0], [100.0, 150.0, 0.0], INTERACT_REACH), None, "beside the line");
        assert_eq!(ray_meets_body(eye, [0.0, -1.0, 0.0], [0.0, 150.0, 0.0], INTERACT_REACH), None, "behind");
        // Looking down over its head.
        let up = [0.0, std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2];
        assert_eq!(ray_meets_body(eye, up, [0.0, 150.0, 0.0], INTERACT_REACH), None, "over its head");
    }

    #[test]
    fn the_sweep_crosses_then_fades() {
        assert_eq!(sweep_look(None), [0.0; 4]);
        let start = sweep_look(Some(0.0));
        assert!(start[0] < 0.01 && start[3] == 1.0);
        let mid = sweep_look(Some(SWEEP_SECONDS / 2.0));
        assert!(mid[0] > start[0] && mid[0] < 2.0);
        let fading = sweep_look(Some(SWEEP_SECONDS + SWEEP_FADE / 2.0));
        assert!((fading[3] - 0.5).abs() < 1e-3);
        assert_eq!(sweep_look(Some(SWEEP_SECONDS + SWEEP_FADE)), [0.0; 4]);
    }
}
