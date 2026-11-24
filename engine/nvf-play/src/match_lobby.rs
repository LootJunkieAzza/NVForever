//! Gathering a co-op match at a table (items 126, 128) [owner 2026-11-08]:
//!
//! - **The modes.** A table's Challenge page lists its game's modes. Each is greyed until
//!   enough players are near the table (this player counted): darts 1v1, 1v1v1, 2v2,
//!   1v1v1v1; pool 1v1 8-ball, 3-player cutthroat, 2v2 8-ball, 4-player cutthroat; fillet
//!   a race of up to 4; blackjack and poker up to 4 (their shared tables); caravan stays
//!   1v1 ([`modes`]).
//! - **The invitation.** Choosing a mode asks the players near the table, as a duel
//!   invitation does: "Peter would like to play darts (1v1v1)", Y or N ([`prompt`]).
//!   The first who say yes fill the seats; the others are told no. The ask travels in the
//!   owner's table view ([`LobbyView`], a trailing part of the game's view), and the
//!   answer as the game's own action, so no new co-op message kind is needed [design].
//! - **Coming over.** Those who said yes walk to the table and use it. Everyone already
//!   there sees "Waiting for all players..." in the style of the revive screen: a
//!   picture and two lines ([`WaitingScreen`]).
//! - **The start.** The match starts by itself once every seat's player is at the table
//!   ([`Gather::ready`]). Nobody answering in [`ASK_SECONDS`], or too few saying yes,
//!   calls it off.
//!
//! The words are NVForever's own (LEGAL.md).

use crate::coop_tables::{game, Tables, R, W};
use crate::interface::{amber, at, face, plain_font, Face, Pictures};
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

/// Seconds an invitation waits for its answer, and the players who said yes to come to
/// the table [design].
pub const ASK_SECONDS: f32 = 25.0;
pub const COME_SECONDS: f32 = 60.0;

/// A way to play a table's game with other players.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    /// Its number on the wire and on the Challenge page.
    pub code: u8,
    pub label: &'static str,
    /// Players in it, this one counted.
    pub players: u8,
    /// Two teams of two (the shooters take turns within a team).
    pub teams: bool,
}

const fn m(code: u8, label: &'static str, players: u8, teams: bool) -> Mode {
    Mode { code, label, players, teams }
}

pub const DARTS_MODES: [Mode; 4] = [m(1, "1v1", 2, false), m(2, "1v1v1", 3, false), m(3, "2v2", 4, true), m(4, "1v1v1v1", 4, false)];
pub const POOL_MODES: [Mode; 4] = [m(1, "1v1 8-ball", 2, false), m(2, "3-player cutthroat", 3, false), m(3, "2v2 8-ball", 4, true), m(4, "4-player cutthroat", 4, false)];
pub const FILLET_MODES: [Mode; 3] = [m(1, "Race of 2", 2, false), m(2, "Race of 3", 3, false), m(3, "Race of 4", 4, false)];

/// The modes of a game's tables (none for a game not gathered this way).
pub fn modes(g: u8) -> &'static [Mode] {
    match g {
        game::DARTS => &DARTS_MODES,
        game::POOL => &POOL_MODES,
        game::FILLET => &FILLET_MODES,
        _ => &[],
    }
}

pub fn mode(g: u8, code: u8) -> Option<Mode> {
    modes(g).iter().copied().find(|m| m.code == code)
}

/// Whether a mode can be chosen with this many players at hand (this one counted).
pub fn available(m: Mode, at_hand: usize) -> bool {
    at_hand >= usize::from(m.players)
}

/// The game's name in an invitation.
pub fn game_word(g: u8) -> &'static str {
    match g {
        game::DARTS => "Darts",
        game::POOL => "Pool",
        game::FILLET => "Five Finger Fillet",
        _ => "a game",
    }
}

/// The invitation's line.
pub fn invite_line(name: &str, g: u8, m: Mode) -> String {
    format!("{name} would like to play {} ({})", game_word(g), m.label)
}

/// Where an asked player is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seat {
    Asked,
    Yes,
    Here,
    No,
}

impl Seat {
    fn code(self) -> u8 {
        match self {
            Seat::Asked => 0,
            Seat::Yes => 1,
            Seat::Here => 2,
            Seat::No => 3,
        }
    }

    fn of(c: u8) -> Option<Seat> {
        [Seat::Asked, Seat::Yes, Seat::Here, Seat::No].into_iter().find(|s| s.code() == c)
    }
}

/// A match being gathered, as the owner's table view tells the others.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LobbyView {
    pub mode: u8,
    pub seats: Vec<(u8, Seat)>,
}

impl LobbyView {
    pub fn write(&self, w: &mut W) {
        w.u8(self.mode).u8(self.seats.len().min(8) as u8);
        for (p, s) in self.seats.iter().take(8) {
            w.u8(*p).u8(s.code());
        }
    }

    pub fn read(r: &mut R) -> Option<LobbyView> {
        let mode = r.u8()?;
        let n = r.u8()? as usize;
        let mut seats = Vec::new();
        for _ in 0..n.min(8) {
            seats.push((r.u8()?, Seat::of(r.u8()?)?));
        }
        Some(LobbyView { mode, seats })
    }

    pub fn seat(&self, p: u8) -> Option<Seat> {
        self.seats.iter().find(|s| s.0 == p).map(|s| s.1)
    }

    /// The waiting screen's second line: who is still to come.
    pub fn still_to_come(&self, name: &dyn Fn(u8) -> String) -> String {
        let waiting: Vec<String> = self.seats.iter().filter(|s| matches!(s.1, Seat::Asked | Seat::Yes)).map(|s| name(s.0)).collect();
        if waiting.is_empty() {
            "Starting".into()
        } else {
            format!("Still to come: {}", waiting.join(", "))
        }
    }
}

/// The owner's side of a match being gathered.
#[derive(Debug, Clone, PartialEq)]
pub struct Gather {
    pub mode: Mode,
    pub seats: Vec<(u8, Seat)>,
    /// When the players were asked (aid seconds), and when the seats were filled.
    pub since: f32,
    pub filled: Option<f32>,
}

/// Why a gathering was called off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    TooFew,
    NotCome,
}

impl Gather {
    pub fn new(mode: Mode, asked: &[u8], now: f32) -> Gather {
        Gather { mode, seats: asked.iter().map(|p| (*p, Seat::Asked)).collect(), since: now, filled: None }
    }

    /// The other players the match needs.
    fn needed(&self) -> usize {
        usize::from(self.mode.players) - 1
    }

    fn count(&self, f: impl Fn(Seat) -> bool) -> usize {
        self.seats.iter().filter(|s| f(s.1)).count()
    }

    /// An answer from `p`. A yes after the seats are full is a no.
    pub fn answer(&mut self, p: u8, yes: bool, now: f32) {
        let full = self.count(|s| matches!(s, Seat::Yes | Seat::Here)) >= self.needed();
        if let Some(s) = self.seats.iter_mut().find(|s| s.0 == p && s.1 == Seat::Asked) {
            s.1 = if yes && !full { Seat::Yes } else { Seat::No };
        }
        if self.count(|s| matches!(s, Seat::Yes | Seat::Here)) >= self.needed() {
            self.filled.get_or_insert(now);
            // The seats are full: those not yet answered are told no.
            for s in &mut self.seats {
                if s.1 == Seat::Asked {
                    s.1 = Seat::No;
                }
            }
        }
    }

    /// `p` came to the table (it opened it).
    pub fn arrive(&mut self, p: u8) {
        if let Some(s) = self.seats.iter_mut().find(|s| s.0 == p && s.1 == Seat::Yes) {
            s.1 = Seat::Here;
        }
    }

    /// `p` left (or walked away from the table): its seat is a no.
    pub fn leave(&mut self, p: u8) {
        if let Some(s) = self.seats.iter_mut().find(|s| s.0 == p) {
            s.1 = Seat::No;
            self.filled = None;
        }
    }

    /// Called off: too few can still say yes, or the asked are out of time.
    pub fn off(&self, now: f32) -> Option<Off> {
        let can = self.count(|s| s != Seat::No);
        if can < self.needed() || (self.filled.is_none() && now - self.since > ASK_SECONDS) {
            return Some(Off::TooFew);
        }
        self.filled.filter(|t| now - t > COME_SECONDS).map(|_| Off::NotCome)
    }

    /// Everyone is here: the other players, in the order they said yes.
    pub fn ready(&self) -> Option<Vec<u8>> {
        let here: Vec<u8> = self.seats.iter().filter(|s| s.1 == Seat::Here).map(|s| s.0).collect();
        (here.len() >= self.needed()).then(|| here.into_iter().take(self.needed()).collect())
    }

    pub fn view(&self) -> LobbyView {
        LobbyView { mode: self.mode.code, seats: self.seats.clone() }
    }
}

// ---- the invitation on the screen -----------------------------------------------------

/// The lobby in a near player's view of their table, by game.
pub fn lobby_of(g: u8, blob: &[u8]) -> Option<LobbyView> {
    match g {
        game::DARTS => crate::coop_tables::DartsView::decode(blob).and_then(|v| v.lobby),
        game::POOL => crate::pool::PoolView::decode(blob).and_then(|v| v.lobby),
        _ => None,
    }
}

/// The answer to an invitation, as the game's action.
pub fn answer_blob(g: u8, yes: bool) -> Option<Vec<u8>> {
    match g {
        game::DARTS => Some(crate::coop_tables::DartsAction::Answer(yes).encode()),
        game::POOL => Some(crate::pool::PoolAction::Answer(yes).encode()),
        _ => None,
    }
}

/// The invitation shown (or the reminder to come over), and the answers already sent.
#[derive(Resource, Default)]
pub struct Invitation {
    /// The owner, its table, the game, the mode, and what this player is to do.
    pub shown: Option<(u8, u32, u8, Mode, Seat)>,
    answered: Vec<(u8, u32)>,
}

/// The invitations in the near players' views: Y or N answers the first.
fn prompt(keys: crate::bindings::ModeInput, state: Res<State<PlayState>>, tables: Option<ResMut<Tables>>, mut inv: ResMut<Invitation>, ask: Option<ResMut<crate::bindings::AskTake>>) {
    let Some(mut t) = tables.filter(|t| t.on) else {
        inv.shown = None;
        return;
    };
    let Some(me) = t.me else { return };
    let found = t.near.iter().find_map(|&p| {
        let (table, g, blob) = t.view(p)?;
        let l = lobby_of(g, blob)?;
        let s = l.seat(me).filter(|s| matches!(s, Seat::Asked | Seat::Yes))?;
        Some((p, table, g, mode(g, l.mode)?, s))
    });
    // An answer sent: not shown again while the view still says asked.
    let found = found.filter(|f| !(f.4 == Seat::Asked && inv.answered.contains(&(f.0, f.1))));
    inv.answered.retain(|a| found.is_none_or(|f| f.0 != a.0 || f.1 != a.1) || t.view(a.0).is_some());
    if found != inv.shown {
        if let Some((p, _, g, m, s)) = found {
            info!("match lobby: {} ({s:?})", invite_line(&t.name(p), g, m));
        }
        inv.shown = found;
    }
    let Some((p, table, g, _, Seat::Asked)) = inv.shown else { return };
    if *state.get() != PlayState::Playing {
        return;
    }
    if let Some(mut a) = ask {
        a.ask();
    }
    let yes = keys.just_pressed(crate::bindings::Control::Accept);
    if yes || keys.just_pressed(crate::bindings::Control::Decline) {
        if let Some(b) = answer_blob(g, yes) {
            t.send(table, g, b);
            info!("match lobby: {} {}'s invitation", if yes { "accepted" } else { "declined" }, t.name(p));
        }
        inv.answered.push((p, table));
        inv.shown = None;
    }
}

#[derive(Component)]
struct InvitationRoot;

fn draw_invitation(mut commands: Commands, inv: Res<Invitation>, tables: Option<Res<Tables>>, roots: Query<Entity, With<InvitationRoot>>, mut last: Local<Option<String>>) {
    let line = inv.shown.map(|(p, _, g, m, s)| {
        let name = tables.as_deref().map_or_else(|| "A player".into(), |t| t.name(p));
        match s {
            Seat::Asked => format!("{}\n{}) Accept    {}) Decline", invite_line(&name, g, m), crate::bindings::mode_key(crate::bindings::Control::Accept), crate::bindings::mode_key(crate::bindings::Control::Decline)),
            _ => format!("{} ({}) with {name}: walk to the table and use it", game_word(g), m.label),
        }
    });
    if line == *last {
        return;
    }
    *last = line.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(line) = line else { return };
    commands
        .spawn((InvitationRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: Val::Percent(70.0), justify_content: JustifyContent::Center, ..default() }, GlobalZIndex(58), Pickable::IGNORE))
        .with_children(|p| {
            p.spawn((Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)))).with_children(|b| {
                b.spawn((Text::new(line), TextFont { font_size: FontSize::Px(24.0), ..default() }, TextColor(amber(255.0))));
            });
        });
}

// ---- "Waiting for all players..." ------------------------------------------------------

/// What the waiting screen shows: the game (for its picture) and who is still to come.
/// Written by the games while a match gathers at their table.
#[derive(Resource, Default, Clone, PartialEq)]
pub struct WaitingScreen(pub Option<(u8, String)>);

/// A Pip-Boy picture for each game's waiting screen [observed: the files are in
/// Fallout - Textures2.bsa; the Perks folder as the revive screen's picture; the choice
/// is design].
fn picture_of(g: u8) -> &'static str {
    match g {
        game::DARTS => r"Interface\Icons\PipboyImages\Perks\perk_center_of_mass.dds",
        game::POOL => r"Interface\Icons\PipboyImages\Perks\perk_finesse.dds",
        game::FILLET => r"Interface\Icons\PipboyImages\Perks\perk_ninja.dds",
        _ => r"Interface\Icons\PipboyImages\Perks\perk_fortune finder.dds",
    }
}

const ICON_SIZE: f32 = 128.0;

#[derive(Component)]
struct WaitingRoot;

struct Kit {
    large: Face,
    small: Face,
    pictures: Pictures,
}

#[allow(clippy::too_many_arguments)]
fn draw_waiting(
    mut commands: Commands,
    screen: Res<WaitingScreen>,
    state: Res<State<PlayState>>,
    roots: Query<Entity, With<WaitingRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<(WaitingScreen, i32)>>,
    mut kit: Local<Option<Kit>>,
) {
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    // Only while a table's menu is open (a game that closed leaves none behind).
    let shown = if *state.get() == PlayState::Menu { screen.clone() } else { WaitingScreen(None) };
    let now = Some((shown.clone(), width.round() as i32));
    if now == *last {
        return;
    }
    *last = now;
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some((g, who)) = shown.0 else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let data = data.as_deref();
    let kit = kit.get_or_insert_with(|| {
        let large = face(crate::hud::FONT, plain_font(36.0, 15.0), data, &mut images, bc);
        let small = crate::interface::scaled_face(&large, 0.72);
        Kit { large, small, pictures: Pictures::load(data) }
    });
    let bright = amber(255.0);
    let (cx, top) = (width / 2.0, 960.0 / 2.0 - ICON_SIZE + 20.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands.spawn((WaitingRoot, full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)), GlobalZIndex(40))).with_children(|r| {
        if let Some((image, rect)) = kit.pictures.picture(picture_of(g), data, &mut images, bc) {
            r.spawn((at(cx - ICON_SIZE / 2.0, top, ICON_SIZE, ICON_SIZE), ImageNode { rect, color: bright, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
        }
        let y = top + ICON_SIZE + 10.0;
        crate::weapon_wheel::draw::text(r, &kit.large, "Waiting for all players...", cx, y, 0, bright);
        crate::weapon_wheel::draw::text(r, &kit.small, &who, cx, y + kit.large.font.size + 2.0, 0, bright.with_alpha(0.75));
    });
    info!("match lobby: waiting screen ({who})");
}

pub struct MatchLobbyPlugin;

impl Plugin for MatchLobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Invitation>().init_resource::<WaitingScreen>().add_systems(Update, (prompt, draw_invitation).chain().in_set(PlaySet::Ui));
        if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
            app.add_systems(Update, draw_waiting.in_set(PlaySet::Ui));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_are_greyed_until_enough_players_are_near() {
        let at_hand = 3;
        let open: Vec<&str> = DARTS_MODES.iter().filter(|m| available(**m, at_hand)).map(|m| m.label).collect();
        assert_eq!(open, ["1v1", "1v1v1"]);
        assert!(POOL_MODES.iter().all(|m| available(*m, 4)));
        assert!(modes(game::CARAVAN).is_empty());
    }

    #[test]
    fn the_first_yes_answers_fill_the_seats_and_the_match_starts_when_all_are_here() {
        let mut g = Gather::new(DARTS_MODES[1], &[1, 2, 3], 0.0);
        g.answer(2, true, 1.0);
        assert!(g.ready().is_none());
        g.answer(3, true, 2.0);
        // Full: player 1, not yet answered, is told no; a late yes stays a no.
        assert_eq!(g.view().seat(1), Some(Seat::No));
        g.answer(1, true, 3.0);
        assert_eq!(g.view().seat(1), Some(Seat::No));
        g.arrive(3);
        assert!(g.ready().is_none());
        g.arrive(2);
        assert_eq!(g.ready(), Some(vec![2, 3]));
        assert_eq!(g.off(4.0), None);
    }

    #[test]
    fn too_few_yes_answers_or_nobody_coming_call_it_off() {
        let mut g = Gather::new(DARTS_MODES[3], &[1, 2, 3], 0.0);
        g.answer(1, false, 1.0);
        assert_eq!(g.off(1.0), Some(Off::TooFew));
        let mut g = Gather::new(DARTS_MODES[0], &[5], 0.0);
        assert_eq!(g.off(ASK_SECONDS + 1.0), Some(Off::TooFew));
        g.answer(5, true, 1.0);
        assert_eq!(g.off(ASK_SECONDS + 1.0), None);
        assert_eq!(g.off(1.0 + COME_SECONDS + 1.0), Some(Off::NotCome));
    }

    #[test]
    fn the_lobby_goes_over_the_wire() {
        let l = LobbyView { mode: 3, seats: vec![(1, Seat::Asked), (2, Seat::Yes), (3, Seat::Here), (4, Seat::No)] };
        let mut w = W::default();
        l.write(&mut w);
        let mut r = R(&w.0);
        assert_eq!(LobbyView::read(&mut r), Some(l));
        assert!(r.done());
    }
}

// ---- The minigames' menu card (item 126, restyled) ---------------------------------------

/// One line of a minigame menu: the key, what it does, a note after it, and whether it can
/// be picked now (a greyed line says why not in its note).
#[derive(Debug, Clone, PartialEq)]
pub struct MenuRow {
    pub key: String,
    pub label: String,
    pub note: String,
    pub on: bool,
}

/// A line that can be picked.
pub fn row(key: &str, label: impl Into<String>) -> MenuRow {
    MenuRow { key: key.into(), label: label.into(), note: String::new(), on: true }
}

/// A line picked by a screen's control `c`, named as bound (`crate::bindings::mode_key`).
pub fn key_row(c: crate::bindings::Control, label: impl Into<String>) -> MenuRow {
    row(&crate::bindings::mode_key(c), label)
}

impl MenuRow {
    /// With a note after the label.
    pub fn note(mut self, note: impl Into<String>) -> MenuRow {
        self.note = note.into();
        self
    }

    /// Greyed when `on` is false.
    pub fn enabled(mut self, on: bool) -> MenuRow {
        self.on = on;
        self
    }
}

/// A minigame's menu page as one card on the left of the screen, the board or table in the
/// world to its right, in the HUD's look (`casino_hud::menu_card`). Darts, pool and fillet
/// draw every menu page with it, so they look alike [design: owner item 126, "restyle the
/// menus"; owner 2026-11-09, the HUD's font and colour].
pub fn menu_card(p: &mut ChildSpawnerCommands, f: &crate::casino_hud::CasinoFaces, title: &str, sub: &str, rows: &[MenuRow], foot: &str) {
    crate::casino_hud::menu_card(p, f, crate::casino_hud::CardText { title, sub, rows, foot });
}
