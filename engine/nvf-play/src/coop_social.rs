//! The co-op interaction wheel (the fork; the owner's spec of 2026-11-04): looking at
//! another player shows an Interact prompt, as Talk shows on an NPC; E opens a wheel
//! styled as the weapon wheel's with **Duel**, **Trade** and **Play Caravan**. Choosing
//! one asks the other player with a Y/N prompt that names the sender ([`invite`]).
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
//! - **Trade** ([`trade`]): Fallout 76's player vending: browse the other's inventory and
//!   ask for items; the owner prices each item asked for (a price for one); the asker buys
//!   for caps. A panel over the world, which runs on ([`draw`]).
//! - **Play Caravan**: the caravan menu against the other player (`crate::caravan`'s
//!   [`crate::caravan::PvpWire`]).
//!
//! The messages ride the co-op wire's table actions ([`wire`]; no protocol change);
//! nvforever `coop.rs` fills [`SocialMates`] with where the others are each frame and
//! makes the loser's puppet fall on the winner's screen ([`Social::falls`]).
//!
//! Log lines: `co-op social: …` (invitations, wheel picks, duel phases, trades).

pub mod draw;
pub mod duel;
pub mod invite;
pub mod trade;
pub mod wheel;
pub mod wire;

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
use wire::{CaravanMove, Kind, Msg};

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
/// The orange sweep: how long it crosses and how long its grade fades (real seconds).
pub const SWEEP_SECONDS: f32 = crate::vats_mode::ENTER_SWIPE;
pub const SWEEP_FADE: f32 = 0.9;

/// Another player as this game sees it (filled by nvforever `coop.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct Mate {
    pub id: u8,
    pub name: String,
    /// Its puppet's reference here (its actor's on the host).
    pub reference: u32,
    /// In this player's cell or worldspace.
    pub here: bool,
    pub feet: [f32; 3],
    pub yaw: f32,
    pub down: bool,
    pub lost: bool,
}

/// The co-op session as the wheel needs it.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct SocialMates {
    pub me: Option<u8>,
    /// This player is indoors.
    pub interior: bool,
    pub mates: Vec<Mate>,
}

impl SocialMates {
    pub fn get(&self, id: u8) -> Option<&Mate> {
        self.mates.iter().find(|m| m.id == id)
    }
}

/// A trade open on screen.
#[derive(Debug)]
pub struct TradeUi {
    pub book: Trade,
    pub name: String,
    /// 0 their items, 1 this player's asks, 2 what they asked of this player.
    pub panel: usize,
    pub rows: [usize; 3],
    /// How many of the picked item of theirs to ask for.
    pub count: u16,
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
    /// A caravan game: the other player and the session's nonce.
    pub caravan: Option<(u8, u16)>,
    /// A line shown under the crosshair, until (real seconds).
    pub banner: Option<(String, f32)>,
    /// When the duel's sweep began (real seconds).
    pub sweep_at: Option<f32>,
    /// Players whose puppets fall here (they lost a duel to this player).
    pub falls: Vec<u8>,
    /// Attacks begun by this player as last seen (`combat::WeaponAnim::attacks`).
    attacks: Option<u32>,
    /// Caps to give back (buys the other could not honour), and sales to carry out
    /// (item, count, price for one).
    refund: u32,
    sales: Vec<(u32, u16, u32)>,
    rng: Option<rand_chacha::ChaCha8Rng>,
    /// The wheel was open this frame: its E press is not also the prompt's ([`target`]).
    wheel_had_input: bool,
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
    caravans: u32,
}

impl Social {
    /// The wheel or the trade panel has the mouse and the keys.
    pub fn holds_input(&self) -> bool {
        self.wheel.is_some() || self.trade.as_ref().is_some_and(|t| t.closing.is_none())
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
        self.duel.is_some() || self.trade.is_some() || self.caravan.is_some()
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

/// The caravan game's seed, from the session's nonce: both games deal the same.
pub fn caravan_seed(nonce: u16) -> u64 {
    (nonce as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xC0A7_CA2A
}

pub struct CoopSocialPlugin;

impl Plugin for CoopSocialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Social>()
            .init_resource::<SocialMates>()
            .add_systems(Update, social_aid.in_set(PlaySet::Input).before(input))
            .add_systems(Update, input.in_set(PlaySet::Input).after(crate::weapon_wheel::read).before(crate::control::read_controls))
            .add_systems(Update, hold.after(crate::control::enforce_locks).after(PlaySet::Input).before(PlaySet::Physics))
            .add_systems(Update, target.in_set(PlaySet::Activation).after(crate::activate::find_target).before(crate::activate::activate))
            .add_systems(Update, (talk, duel_step, trade_step, caravan_bridge).chain().in_set(PlaySet::Ui))
            .add_systems(Last, report);
        draw::build(app);
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
    (mut tables, mut sim, mates): (ResMut<Tables>, ResMut<SimState>, Res<SocialMates>),
) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let playing = *state.get() == PlayState::Playing;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    let keys = &input.keys;
    // The invitation on screen.
    if playing && s.wheel.is_none() && s.invites.shown().is_some() {
        let yes = keys.just_pressed(KeyCode::KeyY);
        if yes || keys.just_pressed(KeyCode::KeyN) {
            let shown = *s.invites.shown().unwrap();
            info!("co-op social: {} the invitation of player {} ({})", if yes { "accepted" } else { "declined" }, shown.with + 1, shown.kind.label());
            if let Some(Settled::Accepted { with, kind, nonce, inviter }) = s.invites.answer(yes, &mut out) {
                start(s, with, kind, nonce, inviter, &mates, now, &mut out);
            }
        }
    }
    // The wheel.
    let k = windows.single().map_or(1.0, |w| 960.0 / w.height().max(1.0));
    s.wheel_had_input = s.wheel.is_some();
    let stepped = s.wheel.as_mut().map(|w| {
        let moved = if playing { motion.delta * k } else { Vec2::ZERO };
        let choose = playing && (input.mouse.just_pressed(MouseButton::Left) || input.just_pressed(Control::Activate));
        let cancel = !playing || input.mouse.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Escape) || mates.get(w.with).is_none_or(|m| !m.here || m.lost);
        (w.step(moved, None, choose, cancel), w.with, w.name.clone())
    });
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
                    s.say(format!("Waiting for {name} to answer…"), now);
                }
                Some(Err(why)) => s.say(format!("Still {why}"), now),
                None => {}
            }
        }
        _ => {}
    }
    // The trade panel.
    let said = s.trade.as_mut().filter(|t| t.closing.is_none() && playing).and_then(|t| trade_keys(t, keys, &mut sim, &mut out));
    if let Some(line) = said {
        s.say(line, now);
    }
    if let Some(t) = s.trade.as_ref().filter(|t| t.closing.is_none() && (keys.just_pressed(KeyCode::Escape) || !playing)) {
        let (nonce, with) = (t.book.nonce, t.book.with);
        info!("co-op social: the trade with player {} closed", with + 1);
        out.push((with, Msg::Quit { nonce }));
        close_trade(s, now);
    }
    if s.holds_input() || s.frozen() {
        control.wheel = true;
        (control.trigger, control.aiming) = (false, false);
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

/// A step of `NVF_PLAY_TEST_SOCIAL` (module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
enum AidStep {
    Face,
    Pick(Kind),
}

fn aid_steps(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, rest)| {
            let step = match rest {
                "face" => AidStep::Face,
                "pick=duel" => AidStep::Pick(Kind::Duel),
                "pick=trade" => AidStep::Pick(Kind::Trade),
                "pick=caravan" => AidStep::Pick(Kind::Caravan),
                _ => return Err(format!("{rest:?}: expected face or pick=duel|trade|caravan")),
            };
            Ok((t, step))
        })
        .collect()
}

/// `NVF_PLAY_TEST_SOCIAL` (module docs).
fn social_aid(
    time: crate::pause::AidTime,
    mut steps: Local<Option<Vec<(f32, AidStep)>>>,
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
    let due: Vec<AidStep> = steps.iter().filter(|(t, _)| *t <= now).map(|&(_, s)| s).collect();
    steps.retain(|(t, _)| *t > now);
    for step in due {
        match step {
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

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// The trade panel's keys: Left/Right the panel, Up/Down the row, +/- the count or the
/// price (Shift: by 10), Enter asks, buys or sends the price, X declines.
fn trade_keys(t: &mut TradeUi, keys: &ButtonInput<KeyCode>, sim: &mut SimState, out: &mut Vec<(u8, Msg)>) -> Option<String> {
    let with = t.book.with;
    let lens = [t.book.stock.len(), t.book.asks.len(), t.book.offers.len()];
    if keys.just_pressed(KeyCode::ArrowRight) {
        t.panel = (t.panel + 1) % 3;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        t.panel = (t.panel + 2) % 3;
    }
    let p = t.panel;
    if keys.just_pressed(KeyCode::ArrowDown) && lens[p] > 0 {
        t.rows[p] = (t.rows[p] + 1) % lens[p];
        t.count = 1;
    }
    if keys.just_pressed(KeyCode::ArrowUp) && lens[p] > 0 {
        t.rows[p] = (t.rows[p] + lens[p] - 1) % lens[p];
        t.count = 1;
    }
    for (i, n) in lens.iter().enumerate() {
        t.rows[i] = t.rows[i].min(n.saturating_sub(1));
    }
    let step: i64 = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { 10 } else { 1 };
    let plus = keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) || keys.just_pressed(KeyCode::PageUp);
    let minus = keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) || keys.just_pressed(KeyCode::PageDown);
    let by = if plus { step } else if minus { -step } else { 0 };
    let enter = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let row = t.rows[p];
    match p {
        0 => {
            if let Some(&(item, have)) = t.book.stock.get(row) {
                t.count = (t.count as i64 + by).clamp(1, have.max(1) as i64) as u16;
                if enter {
                    match t.book.request(item, t.count) {
                        Ok(m) => {
                            info!("co-op social: trade: asked player {} for {} × {}", with + 1, t.count, FormId(item));
                            out.push((with, m));
                        }
                        Err(why) => return Some(format!("Cannot ask: {why}")),
                    }
                }
            }
        }
        1 if enter => {
            let caps = caps_of(sim);
            match t.book.buy(row, caps) {
                Ok((take, m)) => {
                    let _ = sim.world.remove_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, take as i32);
                    info!("co-op social: trade: buying from player {} for {take} caps (held until they answer)", with + 1);
                    out.push((with, m));
                }
                Err(why) => {
                    info!("co-op social: trade: cannot buy: {why}");
                    return Some(format!("Cannot buy: {why}"));
                }
            }
        }
        2 => {
            if by != 0 {
                t.book.nudge(row, by);
            }
            if enter {
                if let Some(m) = t.book.price(row) {
                    info!("co-op social: trade: price sent to player {}: {m:?}", with + 1);
                    out.push((with, m));
                }
            }
            if keys.just_pressed(KeyCode::KeyX) && row < t.book.offers.len() {
                if let Some(m) = t.book.decline(row) {
                    out.push((with, m));
                }
            }
        }
        _ => {}
    }
    None
}

fn caps_of(sim: &SimState) -> u32 {
    sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(fnv_game::caravan::CAPS).max(0) as u32)
}

/// This player's tradeable stock: its inventory but caps, equipped items and quest items.
fn my_stock(sim: &SimState) -> Vec<(u32, u16)> {
    let Some(inv) = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
    let equipped = inv.equipped();
    let items = inv.iter().filter(|(f, _)| !sim.forms.item(*f).is_some_and(|i| i.quest_item)).map(|(f, n)| (f.0, n, equipped.iter().filter(|e| **e == f).count() as i32)).collect::<Vec<_>>();
    trade::tradeable(items, fnv_game::caravan::CAPS.0)
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
    let name = mates.get(with).map_or_else(|| format!("Player {}", with + 1), |m| m.name.clone());
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
            s.trade = Some(TradeUi { book, name, panel: 0, rows: [0; 3], count: 1, closing: None, last_stock: f32::MIN });
        }
        Kind::Caravan => {
            s.stats.caravans += 1;
            s.caravan = Some((with, nonce));
            s.say(format!("A hand of caravan with {name}"), now);
        }
    }
}

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
    (mut social, combat): (ResMut<Social>, Option<Res<crate::combat::Combat>>),
) {
    let Some(player) = player else { return };
    if mates.me.is_none() || social.holds_input() {
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
    let t = crate::activate::Targeted { form_id: m.reference, name: m.name.clone(), action: crate::activate::outcome::Action::Player, distance: d, source: crate::activate::pick::Source::Collision, crime: false, empty: false };
    if found.0.as_ref() != Some(&t) {
        found.0 = Some(t);
    }
    if !input.just_pressed(Control::Activate) || social.frozen() || social.wheel_had_input {
        return;
    }
    let now = real.elapsed_secs();
    if social.busy() {
        social.say("Finish what you are doing first", now);
        return;
    }
    let mut w = Wheel::new(m.id, m.name.clone());
    let room = phys.as_deref_mut().is_some_and(|p| {
        let mut f = p.player.as_ref().map_or_else(fnv_phys::Filter::default, |c| fnv_phys::Filter::excluding(c.key()));
        f.exclude_layers.insert(fnv_phys::layers::TRIGGER);
        let from = [player.feet[0], player.feet[1], player.feet[2] + 60.0];
        let clear: [f32; 8] = std::array::from_fn(|i| {
            let a = (i as f32 * 45.0).to_radians();
            let d = [a.sin(), a.cos(), 0.0];
            p.world.queries().ray(from, d, duel::ROOM_CLEAR, &f).filter(|h| !matches!(h.owner, fnv_phys::Owner::Actor { .. })).map_or(duel::ROOM_CLEAR, |h| h.distance)
        });
        duel::room_for_duel(mates.interior, &clear)
    });
    let pistol = pistol_in_hand(combat.as_deref());
    w.refusals[0] = duel::duel_refusal(room, pistol).map(str::to_string);
    info!("co-op social: wheel open on {} (player {}); duel: room {room}, pistol {pistol}", m.name, m.id + 1);
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
fn talk(real: Res<Time<Real>>, mut tables: ResMut<Tables>, mut social: ResMut<Social>, mates: Res<SocialMates>, mut wire_cv: ResMut<crate::caravan::PvpWire>, sim: Res<SimState>, mut caravan_gone: Local<Option<u16>>) {
    let now = real.elapsed_secs();
    let s = &mut *social;
    let mut out: Vec<(u8, Msg)> = Vec::new();
    let Some(me) = tables.me.filter(|_| tables.on) else {
        // Out of co-op: everything ends.
        if s.duel.is_some() || s.trade.is_some() || s.caravan.is_some() || s.wheel.is_some() || s.invites.out.is_some() || !s.invites.asked.is_empty() {
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
                    let line = format!("{} declined {}", name_of(&mates, with), invitation_word(kind));
                    s.say(line, now);
                }
                _ => {}
            },
            Msg::Quit { nonce } if s.invites.asked.iter().any(|i| i.nonce == nonce && i.with == from) => {
                s.invites.withdrawn(from, nonce);
            }
            m @ (Msg::Quit { .. } | Msg::DuelReady { .. } | Msg::DuelFreeze { .. } | Msg::DuelHit { .. } | Msg::DuelOver { .. }) if s.duel.as_ref().is_some_and(|d| d.nonce == nonce && d.with == from) => {
                let d = s.duel.as_mut().unwrap();
                let outs = d.heard(&m, now);
                duel_outs(s, outs, &mates, now, &mut out);
            }
            Msg::Quit { nonce } if s.trade.as_ref().is_some_and(|t| t.book.nonce == nonce && t.book.with == from) => {
                let line = format!("{} left the trade", name_of(&mates, from));
                s.say(line, now);
                close_trade(s, now);
            }
            Msg::Quit { nonce } if s.caravan.is_some_and(|c| c == (from, nonce)) => wire_cv.inbox.push(crate::caravan::PvpIn::Forfeit),
            Msg::CaravanDeck { cards, .. } if s.caravan.is_some_and(|c| c == (from, nonce)) => wire_cv.inbox.push(crate::caravan::PvpIn::Deck(cards.into_iter().map(FormId).collect())),
            Msg::CaravanMove { mv, .. } if s.caravan.is_some_and(|c| c == (from, nonce)) => match mv.flipped() {
                Some(m) => wire_cv.inbox.push(crate::caravan::PvpIn::Move(m)),
                None => warn!("co-op social: player {} sent a caravan move this game cannot read", from + 1),
            },
            m if s.trade.as_ref().is_some_and(|t| t.book.nonce == nonce && t.book.with == from) => trade_heard(s, m, &sim, now, &mut out),
            _ => debug!("co-op social: a message of a session that is over: {m:?}"),
        }
    }
    for st in s.invites.step(now, &mut out) {
        match st {
            Settled::Lapsed { with, kind, mine: true } => {
                let line = format!("{} did not answer {}", name_of(&mates, with), invitation_word(kind));
                s.say(line, now);
            }
            Settled::Lapsed { with, .. } => info!("co-op social: player {}'s invitation lapsed", with + 1),
            _ => {}
        }
    }
    // A player gone (left, lost): its sessions end.
    let gone = |id: u8| mates.get(id).is_none_or(|m| m.lost);
    for i in s.invites.asked.iter().map(|i| i.with).chain(s.invites.out.map(|o| o.with)).filter(|&i| gone(i)).collect::<Vec<_>>() {
        s.invites.gone(i);
    }
    if let Some(d) = s.duel.as_ref().filter(|d| gone(d.with) && !d.done()) {
        info!("co-op social: duel with player {}: they went away", d.with + 1);
        s.duel = None;
        s.say("The duel is called off", now);
    }
    if s.trade.as_ref().is_some_and(|t| gone(t.book.with) && t.closing.is_none()) {
        close_trade(s, now);
    }
    // Once: the menu closes on its own frame later.
    if let Some((with, nonce)) = s.caravan.filter(|&(w, n)| gone(w) && *caravan_gone != Some(n)) {
        *caravan_gone = Some(nonce);
        info!("co-op social: caravan with player {}: they went away", with + 1);
        wire_cv.inbox.push(crate::caravan::PvpIn::Forfeit);
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

fn name_of(mates: &SocialMates, id: u8) -> String {
    mates.get(id).map_or_else(|| format!("Player {}", id + 1), |m| m.name.clone())
}

fn invitation_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Duel => "your challenge",
        Kind::Trade => "your offer to trade",
        Kind::Caravan => "your game of caravan",
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
            Out::Thrown => s.falls.push(u8::MAX),
            Out::TheyFall => {
                info!("co-op social: duel: {} falls", name_of(mates, with));
                s.falls.push(with);
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
        let them = mates.get(with).cloned();
        let name = them.as_ref().map_or_else(|| format!("Player {}", with + 1), |m| m.name.clone());
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
    if let Some(i) = s.falls.iter().position(|&f| f == u8::MAX) {
        s.falls.remove(i);
        let winner = s.duel.as_ref().and_then(|d| mates.get(d.with)).map_or(player.feet, |m| m.feet);
        let shove = duel::throw(player.feet, winner);
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            let down = crate::script::with_world(rt, &mut sim, |rt| rt.game.knock_down(FormId::PLAYER_REF, None, shove));
            info!("co-op social: duel: this player is thrown back ({shove:?}): {}", if down { "knocked down" } else { "could not be knocked down" });
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
    let have = |item: u32| sim.world.peek(FormId::PLAYER_REF).and_then(|p| p.inventory.as_ref()).map_or(0, |i| i.count(FormId(item)).clamp(0, u16::MAX as i32) as u16);
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
            say = Some(format!("{} asks for {count} × {}: set your price", t.name, item_name(sim, item)));
        }
        Msg::TradePrice { item, each, .. } => {
            t.book.price_heard(item, each);
            say = Some(if each == 0 { format!("{} will not sell {}", t.name, item_name(sim, item)) } else { format!("{} asks {each} caps each for {}", t.name, item_name(sim, item)) });
        }
        // The other buys from this player: the items go, the caps come (`trade_step`; the
        // buyer gets the items when told).
        Msg::TradeBuy { item, count, each, .. } => {
            let (sale, reply) = t.book.buy_heard(item, count, each, have(item));
            out.push((with, reply));
            if let Sale::Sell { item, count, each } = sale {
                info!("co-op social: trade: sold {count} × {} to player {} for {} caps", FormId(item), with + 1, each * count as u32);
                say = Some(format!("Sold {count} × {} for {} caps", item_name(sim, item), each * count as u32));
                s.stats.sold += 1;
                s.sales.push((item, count, each));
            } else {
                info!("co-op social: trade: refused player {}'s buy of {count} × {} at {each}", with + 1, FormId(item));
            }
        }
        Msg::TradeSold { item, count, each, .. } => {
            if t.book.sold(item, count, each) {
                info!("co-op social: trade: bought {count} × {} from player {}", FormId(item), with + 1);
                say = Some(format!("Bought {count} × {} for {} caps", item_name(sim, item), each * count as u32));
                s.stats.bought += 1;
            }
        }
        Msg::TradeRefused { item, count, each, .. } => {
            if let Some(back) = t.book.refused(item, count, each) {
                info!("co-op social: trade: player {} could not sell {}: {back} caps back", with + 1, FormId(item));
                say = Some(format!("{} could not sell that: {back} caps given back", t.name));
                s.refund = s.refund.saturating_add(back);
            }
        }
        _ => {}
    }
    if let Some(line) = say {
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
        let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, s.refund as i32);
        s.refund = 0;
    }
    // Sold: the items go and the caps come.
    for (item, count, each) in std::mem::take(&mut s.sales) {
        let _ = sim.world.remove_item(FormId::PLAYER_REF, FormId(item), count as i32);
        let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, (each * count as u32) as i32);
    }
    let Some(t) = s.trade.as_mut() else { return };
    // Bought: the items come, once (the ask is done with).
    for a in t.book.asks.iter_mut().filter(|a| a.state == trade::AskState::Bought) {
        let _ = sim.world.add_item(FormId::PLAYER_REF, FormId(a.item), a.count as i32);
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
                let _ = sim.world.add_item(FormId::PLAYER_REF, fnv_game::caravan::CAPS, held as i32);
            }
            s.trade = None;
        }
    }
    for (to, m) in out {
        tables.send(to as u32, game::SOCIAL, m.encode());
    }
}

/// The caravan menu's messages to the other player, and the session's end.
fn caravan_bridge(mut social: ResMut<Social>, mut tables: ResMut<Tables>, mut wire: ResMut<crate::caravan::PvpWire>, mates: Res<SocialMates>, mut opened: Local<bool>) {
    let s = &mut *social;
    let Some((with, nonce)) = s.caravan else {
        *opened = false;
        wire.out.clear();
        return;
    };
    // Accepted: the menu opens (the inviter deals).
    if !*opened && wire.open.is_none() && wire.playing.is_none() {
        *opened = true;
        let inviter = s.asked_by_me(nonce, mates.me);
        let m = mates.get(with);
        wire.open = Some(crate::caravan::PvpOpen { mate: with, name: name_of(&mates, with), reference: FormId(m.map_or(0, |m| m.reference)), leads: inviter, seed: caravan_seed(nonce) });
        return;
    }
    for o in std::mem::take(&mut wire.out) {
        let m = match o {
            crate::caravan::PvpOut::Deck(d) => Msg::CaravanDeck { nonce, cards: d.into_iter().map(|f| f.0).collect() },
            crate::caravan::PvpOut::Move(m) => Msg::CaravanMove { nonce, mv: CaravanMove::of(m) },
            crate::caravan::PvpOut::Forfeit => Msg::Quit { nonce },
        };
        tables.send(with as u32, game::SOCIAL, m.encode());
    }
    // The menu closed (or never opened): the session is over.
    if *opened && wire.open.is_none() && wire.playing.is_none() {
        info!("co-op social: caravan with player {} is over", with + 1);
        s.caravan = None;
        *opened = false;
    }
}

impl Social {
    /// Whether this player asked for session `nonce`: its nonces carry its id.
    fn asked_by_me(&self, nonce: u16, me: Option<u8>) -> bool {
        me.is_some_and(|me| (nonce >> 12) as u8 == (me & 0xF))
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
        "co-op social summary: {} wheels opened, {} invitations sent, {} accepted; {} duels, {} trades ({} sold, {} bought), {} caravan games",
        s.wheels, s.invites, s.accepted, s.duels, s.trades, s.sold, s.bought, s.caravans
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_social_aid_reads_its_steps() {
        assert_eq!(aid_steps("3:face, 4.5:pick=trade").unwrap(), vec![(3.0, AidStep::Face), (4.5, AidStep::Pick(Kind::Trade))]);
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

    #[test]
    fn both_games_deal_one_caravan() {
        assert_eq!(caravan_seed(5), caravan_seed(5));
        assert_ne!(caravan_seed(5), caravan_seed(6));
        let s = Social::default();
        assert!(s.asked_by_me(2 << 12 | 9, Some(2)));
        assert!(!s.asked_by_me(2 << 12 | 9, Some(1)));
    }
}
