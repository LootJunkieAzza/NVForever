//! The HUD of the original: health and compass at the bottom left, action points,
//! weapon condition and ammo at the bottom right (docs/spec-notes/hud.md).
//!
//! The pieces, their sizes and their textures are the game's own interface files'
//! (§2, §3); where they stand is left to the game's code by those files and is set here
//! (§4). Lengths are in the interface's units, 960 to the height of the view.
//!
//! - [`gather`] reads what the HUD shows from the game: the player's health and action
//!   points (the script runtime's actor values), the heading, the equipped weapon's
//!   condition and ammo (`combat`).
//! - [`Places`] sets the pieces out (pure, tested without the game's files).
//! - The pieces are spawned once; after that only the bars' lengths, the compass strip's
//!   place and the ammo's digits change.
//! - Without the game's textures the bars are plain blocks and the brackets plain lines.
//! - The enemy health bar (bottom middle, `crate::hud_target` says whose) and the sneak
//!   word (top middle, while sneaking) are parts of the same HUD (§8, §9).
//! - A bar's ticks are one image of the tick picture repeated ([`crate::repeat`]),
//!   stretched over the bar and seen through a window as wide as the lit ticks, so they
//!   stand evenly at any view size (T12d, hud.md §14.9) [design].

use crate::combat::{Combat, HudWeapon};
use crate::control::ControlLocks;
use crate::hud_target::HudTarget;
use crate::player::Player;
use crate::reticle::crime_colour;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::tiles::vanilla::{DrawnByTiles, EnemyPiece};
use crate::interface::{amber, at, bc_supported, face, plain_font, set_line, texture, u, Face};
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_font::Font;
use fnv_game::actor_values::ACTION_POINTS;
use fnv_game::awareness::State as Awake;
use fnv_game::detection::Band;
use nvf_assets::GameData;

// ---- the interface files' numbers [documented: hud.md §2, §3] -------------------------

/// The two corner pieces (`HitPoints`, `ActionPoints`): width and height.
pub(crate) const HP_PANEL: Vec2 = Vec2::new(369.0, 127.0);
const AP_PANEL: Vec2 = Vec2::new(386.0, 127.0);
/// The bracket pictures' room; the pictures are drawn texel for unit.
const BRACKET: Vec2 = Vec2::new(512.0, 256.0);
/// One tick of a bar: the room it takes and its height. (The file's own length for a full
/// bar, 300, is about the 296 measured, §4.)
const TICK: Vec2 = Vec2::new(8.0, 20.0);
/// The tick picture's block: 6 texels wide, then 2 clear [observed: `hud_tick_mark.dds`,
/// its opacity rising from top to bottom].
const TICK_BLOCK: f32 = 6.0;
/// The health and action points ticks' brightness [hypothesis: `brightness.xml`'s
/// "meter and compass in HUD" 175, as the enemy bar's meter reads (hud.md §14.8); the
/// original's ticks are about 0.7 of its lines' colour at their full rows, hud.md
/// §14.9].
pub(crate) const METER_BRIGHTNESS: f32 = 175.0;
/// The tick picture, 8 x 32 texels [observed: the texture, hud.md §3].
const TICK_PICTURE: &str = "textures\\interface\\hud\\hud_tick_mark.dds";
/// The compass window in the health piece: place and size.
const COMPASS_AT: Vec2 = Vec2::new(20.0, 65.0);
pub const COMPASS: Vec2 = Vec2::new(345.0, 64.0);
/// The heading's x in compass-window units [observed: hud.md §14.5].
pub const COMPASS_HEADING: f32 = 148.0;
/// The whole strip, north to north: 1024 texels for 360 degrees [observed: the texture].
pub const STRIP: Vec2 = Vec2::new(1024.0, 64.0);
/// The solid bar's ground (weapon condition): length and height [documented: the file
/// puts it at x 90 of a frame the code sets; where it stands is measured, below].
const CND_BAR: Vec2 = Vec2::new(60.0, 20.0);
/// The condition fill's full width, starting at the ground's x [observed: hud.md §14,
/// §14.3].
const CND_FILL_WIDTH: f32 = 59.0;
/// The condition's arrows, `CNDArrows`: a 16 x 16 picture, placed by the code
/// [documented: hud_main_menu.xml]; black arrows at the picture's top and bottom edges
/// pointing in, their middle 3.5 texels from its left [observed: the texture].
const CND_ARROWS: &str = "textures\\interface\\shared\\arrow\\repair_meter_pips.dds";
const ARROWS_MIDDLE: f32 = 3.5 / 16.0;
/// Where the arrows stand on the condition bar: below this condition a weapon may jam
/// [owner's playtest list 5, item 73: "the 75% threshold on condition … when jamming
/// occurs"; observed: the arrows at about three quarters on the owner's screenshot].
pub const JAM_CONDITION: f32 = 0.75;
/// The arrows' picture is drawn this much taller than the bar at the top and the bottom
/// and cut to the bar: the arrows' bases are the picture's first and last rows
/// [observed: the texture], and drawn exactly as high as the bar the filtered edge rows
/// left a strip of the bar showing above and below them (owner's playtest 2026-10-16).
const ARROWS_BLEED: f32 = 1.0;

/// The enemy's health piece, `EnemyHealth`, and its bracket picture's place and size in
/// it (`glow_enemy_health_seperator.dds`) [documented: hud_main_menu.xml,
/// hudtemplates.xml].
const ENEMY: Vec2 = Vec2::new(350.0, 52.0);
const ENEMY_BRACKET_AT: Vec2 = Vec2::new(-7.0, 3.0);
/// [documented: hudtemplates.xml, template_enemy_health_bracket; observed: private
/// jobs hud_enemy2, e5_back; hud_enemy3, g2_gecko_half]. Previously 512 wide [design].
const ENEMY_BRACKET: Vec2 = Vec2::new(500.0, 64.0);
/// The sneak piece, `SneakMeter`: width and height [documented: hud_main_menu.xml].
const SNEAK: Vec2 = Vec2::new(356.0, 34.0);

// ---- what the bracket pictures show [observed: the textures, §3] ----------------------

/// The upper edge of the brackets' long line.
const LINE_Y: f32 = 66.0;
/// The left bracket's upright and the right bracket's two (the end and the divider).
const LEFT_UPRIGHT: f32 = 17.0;
const RIGHT_UPRIGHT: f32 = 369.0;
const DIVIDER: f32 = 207.0;
/// The small corner marks on the long line: where a full bar ends on the left piece and
/// starts on the right one.
const LEFT_MARK: f32 = 320.0;
const RIGHT_MARK: f32 = 70.0;
/// The enemy bracket's long line, taken as `LINE_Y` is on the corner pieces (2 rows into
/// the solid line, rows 21-23; `LINE_Y` is 2 into rows 64-66), and the inner edges of its
/// two corner marks, between which its bar stands [observed: the texture]. At 19 (the glow
/// above the line) the line stood 4 units under the corner pieces' and its ticks 7 pixels
/// over it at 1080 lines, where the original's line is level with the corner pieces'
/// upper solid line and its ticks stand on it [observed: `Target healthbar` screenshot;
/// owner's playtest 2026-10-16].
const ENEMY_LINE_Y: f32 = 23.0;
/// The enemy bar's ticks stand right on its solid line (they end at row 21), not
/// `METER_GAP` over it as the player's bars do [observed: `Target healthbar` screenshot;
/// at `METER_GAP` a 3-pixel strip showed at 1080 lines, owner's playtest 2026-10-16].
const ENEMY_METER_GAP: f32 = 2.0;
/// Texture columns scaled to the measured 500-wide bracket [design; observed: private
/// job hud_enemy3, g2_gecko_half]. Previously unscaled (56, 307).
const ENEMY_MARKS: (f32, f32) = (56.0 * 500.0 / 512.0, 307.0 * 500.0 / 512.0);

// ---- set here; the files leave these to the game's code [hypothesis, §4] --------------

/// From the view's side and bottom edges to the pieces.
pub(crate) const MARGIN_X: f32 = 40.0;
pub(crate) const MARGIN_Y: f32 = 30.0;
/// The bars stand this far above the long line.
const METER_GAP: f32 = 4.0;
/// Room between a label and what it labels.
const LABEL_GAP: f32 = 6.0;
/// The line the texts of the lower row stand on.
const LOWER_BASE: f32 = 106.0;
/// The ground of the condition bar is this much of the bar's own brightness
/// [hypothesis: as `meter.xml`'s ground, a quarter].
const GROUND: f32 = 0.25;

// ---- measured on the owner's 1920 x 1080 screenshot of the original [observed, §4] -----

/// A full bar: 37 ticks, from just inside the bracket's upright to its corner mark (on
/// the left; mirrored on the right).
const PIPS: usize = 37;
const METER_WIDTH: f32 = PIPS as f32 * TICK.x;
/// The bars stand this far left of where they were set from the bracket alone (the
/// corner mark on the left piece, the upright on the right): the first tick starts at
/// the upright's inside edge, the last ends clear of the mark and of the upright
/// [observed: 3 pixels at 1080 lines against the `Target healthbar` screenshot, owner's
/// playtest 2026-10-16]. The action points bar 1 less: its ticks stood 2 pixels left of
/// the original's at 1080 lines, the last one clear of the upright where the original's
/// meets it [observed: the same screenshot, tick edges 1520 … 1844 against 1518 … 1842,
/// 2026-10-16].
const METER_SHIFT: f32 = 3.0;
const AP_METER_SHIFT: f32 = 2.0;
/// "HP" and "AP" stand above their bars near the uprights: the line they stand on, where
/// "HP" starts and where "AP" ends [observed: as above, "HP" 11 and "AP" 8 pixels left of
/// the earlier 33 and 362].
pub(crate) const UPPER_BASE: f32 = 33.0;
pub(crate) const HP_LABEL_X: f32 = 23.0;
const AP_LABEL_END: f32 = 355.0;
/// The condition bar ends this far before the divider, and its bottom stands this far
/// above the line "CND" stands on; "CND" ends this far before the bar [observed: on the
/// `Target healthbar` screenshot the bar starts about 8 pixels after the "D" and is about
/// as high as the letters; owner's playtest 2026-10-16, after 15 / 0 / 6 left 2 pixels
/// between "D" and the bar and the bar lower than the letters]. The bar ends 17 before
/// the divider [observed: hud_weapon capture 2026-10-28, hud.md §14: the 60 x 20 ground's
/// GetUIFloatInherited x 1410 with the divider at 1487, and on its screenshot the bar at
/// 1058-1102 of 1280 pixels; 10, from the screenshot of 2026-10-16, stood 7 too far right,
/// the label with it].
const CND_BEFORE_DIVIDER: f32 = 17.0;
const CND_RAISE: f32 = 2.0;
const CND_LABEL_GAP: f32 = 11.0;
/// The ammo type's abbreviation starts here, just right of the divider, whatever the
/// counter's length [observed: the owner's screenshot 2026-10-25 of the original marks
/// its room at about x 1680-1765 of 1920 at 1080 lines, the divider at 1672: 214-290
/// units; owner 2026-10-25: anchored to the divider, not against the counter].
const AMMO_LABEL_X: f32 = 214.0;
/// The enemy bar's pips: as many as fit between the bracket's corner marks.
const ENEMY_PIPS: usize = 31;
/// The enemy's name: the line it stands on in its piece, centred.
const ENEMY_NAME_BASE: f32 = 53.0;
/// Living Anatomy's "HP:… DT:…" line stands on this y of the enemy's piece, a line under
/// the name [observed: the owner's screenshot 2026-10-24 of the original, the two lines
/// about 20 of 528 rows apart: 36 interface units].
const ENEMY_STATS_BASE: f32 = ENEMY_NAME_BASE + 36.0;
/// The sneak word: the line it stands on, from the view's top, centred.
const SNEAK_BASE: f32 = 51.0;

/// The labels [hypothesis: the game takes them from its settings (`sStats…`); the
/// owner's screenshot shows these words].
pub(crate) const HP: &str = "HP";
const AP: &str = "AP";
const CND: &str = "CND";

/// The font of every text on the HUD: font 7 [documented: `hudtemplates.xml`;
/// Fallout_default.ini `sFontFile_7`].
pub(crate) const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

const HUD_TEXTURES: &str = "textures\\interface\\hud\\";

/// What the HUD shows.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct HudView {
    /// The HUD is on the screen: the player is in the world, and no script holds the
    /// player's movement (§5).
    pub shown: bool,
    /// Health and action points, 0 to 1 of their full values.
    pub health: f32,
    pub action_points: f32,
    /// Compass heading in degrees (0 north, 90 east).
    pub heading: f32,
    pub weapon: HudWeapon,
    /// The sneak word, while the player sneaks (§9).
    pub sneak: Option<Sneak>,
}

impl Default for HudView {
    fn default() -> Self {
        HudView { shown: false, health: 1.0, action_points: 1.0, heading: 0.0, weapon: HudWeapon::default(), sneak: None }
    }
}

/// The sneak word at the top middle while the player sneaks (§9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sneak {
    Hidden,
    Detected,
    Caution,
    Danger,
}

impl Sneak {
    /// [observed: the owner's four screenshots of the original.]
    pub fn word(self) -> &'static str {
        match self {
            Sneak::Hidden => "[HIDDEN]",
            Sneak::Detected => "[DETECTED]",
            Sneak::Caution => "[CAUTION]",
            Sneak::Danger => "[DANGER]",
        }
    }

    /// The two warnings are in the HUD's alternative colour, the others in its own
    /// [observed: the screenshots].
    pub fn warning(self) -> bool {
        matches!(self, Sneak::Caution | Sneak::Danger)
    }

    /// Which word: someone fights the player; else someone hostile searches for the
    /// player; else someone detects the player; else hidden [hypothesis: the words'
    /// meaning as players describe them; the original's rule is not documented].
    pub fn of(fighting: bool, searching: bool, detected: bool) -> Sneak {
        if fighting {
            Sneak::Danger
        } else if searching {
            Sneak::Caution
        } else if detected {
            Sneak::Detected
        } else {
            Sneak::Hidden
        }
    }
}

/// The sneak word from the game's detection states (`fnv_game::awareness`) and the
/// latest detection values (`Game::detection`) [design: shared by drawing and tree].
pub(crate) fn sneak_now(g: &fnv_game::Game) -> Sneak {
    let me = FormId::PLAYER_REF;
    let (mut fighting, mut searching) = (false, false);
    for (id, a) in &g.awareness.actors {
        if g.is_dead(*id) {
            continue;
        }
        match a.state {
            Awake::Combat => fighting |= a.combat_target().is_some_and(|(t, _)| t == me),
            Awake::Alert | Awake::Lost => searching |= a.targets.contains_key(&me) || a.alarms.iter().any(|x| x.owner == me),
            Awake::Normal => {}
        }
    }
    let detected = g.detection.highest(me).is_some_and(|(_, d)| d.band == Band::Detected);
    Sneak::of(fighting, searching, detected)
}

// ---- layout --------------------------------------------------------------------------

/// How many of a bar's `count` ticks are lit at `value` (0 to 1): rounded down for the
/// health and action points bars [observed: hud.md §14.3]. None at 0 or below, or for
/// non-finite values [design: bounded shares].
pub fn lit(value: f32, count: usize) -> usize {
    if value.is_finite() {
        ((value.clamp(0.0, 1.0) * count as f32).floor() as usize).min(count)
    } else {
        0
    }
}

/// The ticks lit of a bar that empties from both ends towards its middle (the enemy's
/// [observed: owner's playtest list 5, item 32]): from and to (exclusive), counted from
/// the left. An odd tick left over goes from the right end first [design: centred ticks].
/// The count rounds down [hypothesis: as the other bars].
pub fn lit_middle(value: f32, count: usize) -> (usize, usize) {
    let n = lit(value, count);
    let from = (count - n) / 2;
    (from, from + n)
}

/// The condition fill's width in whole interface units [observed: hud.md §14.3].
fn condition_width(value: f32) -> f32 {
    (value.clamp(0.0, 1.0) * CND_FILL_WIDTH).round()
}

/// How far a row's strip of ticks stands right of the row's start: its ticks fill from
/// the bar's outer end, so a full row runs the whole room but the clear strip after the
/// last tick (health), before the first (action points), or half of it either side (the
/// enemy's, in the middle) (§4).
fn strip_shift(bar: Bar, period: f32) -> f32 {
    let clear = period * (TICK.x - TICK_BLOCK) / TICK.x;
    match bar {
        Bar::Health => 0.0,
        Bar::ActionPoints => clear,
        Bar::Enemy => clear / 2.0,
    }
}

/// The window through which a row's lit ticks `from..to` (of `count` over `width` units)
/// show: its left and width, and the strip's left in it, in units from the row's start.
/// Its edges stand a texel into the clear gaps before tick `from` and after tick `to - 1`,
/// so a pixel's rounding of the window never cuts a tick [design].
fn lit_window(bar: Bar, width: f32, count: usize, from: usize, to: usize) -> (f32, f32, f32) {
    if count == 0 || to <= from {
        return (0.0, 0.0, 0.0);
    }
    let period = width / count as f32;
    let shift = strip_shift(bar, period);
    let edge = |i: usize| shift + i as f32 * period - period / TICK.x;
    let left = if from == 0 { 0.0 } else { edge(from) };
    let right = edge(to).max(left);
    (left, right - left, shift - left)
}

/// The lit ticks of a bar, from and to (exclusive), counted from its left.
fn lit_range(bar: Bar, view: &HudView, enemy: f32) -> (usize, usize) {
    match bar {
        Bar::Health => (0, lit(view.health, PIPS)),
        Bar::ActionPoints => (PIPS - lit(view.action_points, PIPS), PIPS),
        Bar::Enemy => lit_middle(enemy, ENEMY_PIPS),
    }
}

/// Where the doubled compass strip starts in its window (0 or less), so that `heading`
/// (degrees) stands at [`COMPASS_HEADING`] [observed: hud.md §14.5].
pub fn strip_left(heading: f32) -> f32 {
    let at = heading.rem_euclid(360.0) / 360.0 * STRIP.x;
    -(at - COMPASS_HEADING).rem_euclid(STRIP.x)
}

/// The ammo counter's text: rounds in the weapon / rounds carried besides.
pub fn ammo_text(ammo: Option<(u32, u32)>) -> String {
    match ammo {
        Some((loaded, carried)) => format!("{loaded}/{carried}"),
        None => String::new(),
    }
}

/// Where the pieces stand in their corner piece, from its top left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct Places {
    /// The "HP" label's start, and the health bar's left end.
    pub hp_label: f32,
    pub hp_meter: f32,
    /// The action points bar's left end (it empties from the left), and the "AP" label's
    /// start.
    pub ap_meter: f32,
    pub ap_label: f32,
    /// The bars' top.
    pub meter_y: f32,
    /// The "CND" label's start, the condition bar's start and top (its bottom on the
    /// label's line), the arrows' left, and where the ammo's text ends.
    pub cnd_label: f32,
    pub cnd_x: f32,
    pub cnd_y: f32,
    pub arrows_x: f32,
    pub ammo_right: f32,
}

impl Places {
    pub fn new(font: &Font) -> Places {
        // [observed, §4] A full bar runs from the upright's inside edge to just short of
        // the bracket's corner mark; its label stands above it, near the upright.
        let hp_meter = LEFT_MARK - METER_WIDTH - METER_SHIFT;
        let ap_meter = RIGHT_MARK - AP_METER_SHIFT;
        let cnd_x = DIVIDER - CND_BEFORE_DIVIDER - CND_BAR.x;
        Places {
            hp_label: HP_LABEL_X,
            hp_meter,
            ap_meter,
            ap_label: AP_LABEL_END - font.width(AP),
            meter_y: LINE_Y - METER_GAP - TICK.y,
            cnd_label: cnd_x - CND_LABEL_GAP - font.width(CND),
            cnd_x,
            cnd_y: LOWER_BASE - CND_RAISE - CND_BAR.y,
            arrows_x: cnd_x + JAM_CONDITION * CND_BAR.x - ARROWS_MIDDLE * (CND_BAR.y + 2.0 * ARROWS_BLEED),
            ammo_right: RIGHT_UPRIGHT - 2.0 * LABEL_GAP,
        }
    }
}

/// Where this HUD sets its corner labels, for the tile tree's code-made copies while the HUD
/// is drawn from tiles (`crate::tiles::vanilla::write_corners`; the captures give none of
/// their places, hud.md §14.6) [design]: "HP"'s start, the ends of "AP", "CND" and the
/// ammo counter, and the ammo abbreviation's start (`AmmoTypeLabel`, whose place the
/// captures read only with no abbreviation shown), each with the line it stands on, from
/// its corner piece's top left (the right piece's as drawn here, [`MARGIN_X`] in from the
/// view's edge).
pub(crate) fn label_places(font: &Font) -> [(f32, f32); 5] {
    let p = Places::new(font);
    [(p.hp_label, UPPER_BASE), (AP_LABEL_END, UPPER_BASE), (p.cnd_x - CND_LABEL_GAP, LOWER_BASE), (p.ammo_right, LOWER_BASE), (AMMO_LABEL_X, LOWER_BASE)]
}

// ---- reading the game ------------------------------------------------------------------

/// Read what the HUD shows. Health and action points come from the script runtime's
/// actor values; until it runs they read full.
#[allow(clippy::too_many_arguments)]
fn gather(
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    combat: Option<Res<Combat>>,
    locks: Option<Res<ControlLocks>>,
    control: Option<Res<crate::control::PlayerControl>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut view: ResMut<HudView>,
    (vats, wheel): (Option<Res<crate::vats_mode::VatsMode>>, Option<Res<crate::weapon_wheel::WeaponWheel>>),
    driven: Option<Res<crate::player::ViewDriven>>,
) {
    // [documented: GECK wiki DisablePlayerControls, the movement flag: "HUD is partially
    // disabled: Action Points, Health, Crosshair".]
    let held = locks.is_some_and(|l| l.movement);
    // Nor over the character creator (its camera is up), whatever state the opening
    // leaves the game in [owner 2026-10-17: "hide the UI so no health bar or AP or
    // dialogue"].
    let creating = control.as_ref().is_some_and(|c| c.portrait.is_some());
    // Nor while an idle holds the camera (the opening's wake-up, `ViewDriven`), whatever
    // the scripts' locks: only subtitles then [owner 2026-10-24]; nor while the weapon
    // wheel is open (docs/design/weapon-wheel.md §7) [design].
    let posed = driven.is_some_and(|d| d.0);
    let wheel_open = wheel.is_some_and(|w| w.is_open());
    let mut now = HudView { shown: *state.get() == PlayState::Playing && !held && !creating && !posed && !wheel_open, ..view.clone() };
    if let Some(p) = &player {
        // Where the camera looks, not where the character faces (third person).
        now.heading = control.as_deref().map_or(p.heading, |c| c.view_heading(p.heading)).to_degrees();
    }
    if let Some(c) = &combat {
        now.weapon = c.hud_weapon(&sim);
    }
    let sneaking = control.is_some_and(|c| c.sneak);
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        now.sneak = sneaking.then(|| sneak_now(&rt.game));
        (now.health, now.action_points) = with_world(rt, &mut sim, |rt| {
            // An aid item's temporary modifier (Buffout's Health, Jet's Action Points) raises
            // the maximum with the value [hypothesis: the bar stays a share of the whole].
            let temporary = |g: &fnv_game::Game, code| fnv_game::functions::actor_value_temporary(g, FormId::PLAYER_REF, code);
            let max = rt.game.max_health(FormId::PLAYER_REF) + temporary(&rt.game, fnv_game::actor_values::HEALTH);
            let health = fnv_game::functions::actor_health(&mut rt.game, FormId::PLAYER_REF).unwrap_or(max);
            let health_share = if max > 0.0 { (health / max) as f32 } else { 0.0 };
            // Log tick changes even under the console [design: smoke hud-bars-partial].
            let n = lit(health_share, PIPS);
            if n != lit(view.health, PIPS) {
                info!("hud: bars health {n} of 37 ({health:.0} of {max:.0})");
            }
            // Base, permanent and temporary modifiers, damage: the bar is what is left of
            // the first three.
            let ap = match fnv_game::functions::actor_value_parts(&mut rt.game, FormId::PLAYER_REF, ACTION_POINTS) {
                Ok([base, modifier, damage]) => {
                    let whole = base + modifier + temporary(&rt.game, ACTION_POINTS);
                    // VATS mode's marks hold their AP: the bar shows what is left after
                    // them (docs/design/vats-mode.md §8) [design].
                    let reserved = vats.as_ref().map_or(0.0, |v| v.reserved() as f64);
                    let share = if whole > 0.0 {
                        ((whole - damage - reserved) / whole).max(0.0)
                    } else {
                        1.0
                    };
                    let n = lit(share as f32, PIPS);
                    if n != lit(view.action_points, PIPS) {
                        let current = (whole - damage - reserved).max(0.0);
                        info!("hud: bars action points {n} of 37 ({current:.0} of {whole:.0})");
                    }
                    share
                }
                _ => 1.0,
            };
            (health_share, ap as f32)
        });
    } else {
        now.sneak = sneaking.then_some(Sneak::Hidden);
    }
    if now.sneak != view.sneak && now.shown {
        match now.sneak {
            Some(s) => info!("hud: sneak {}", s.word()),
            None => info!("hud: sneak off"),
        }
    }
    if now.weapon.abbrev != view.weapon.abbrev && now.shown {
        info!("hud: ammo label {}", now.weapon.abbrev.as_deref().unwrap_or("(none)"));
    }
    view.set_if_neq(now);
}

// ---- drawing ---------------------------------------------------------------------------

/// The HUD's systems: what it shows is read, then drawn.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HudSet;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudView>()
            .add_plugins((crate::hud_target::HudTargetPlugin, crate::hud_limbs::HudLimbsPlugin, crate::hud_needs::HudNeedsPlugin, crate::hud_breath::HudBreathPlugin))
            .add_systems(Update, (gather, draw, pips, texts, enemy_colours).chain().in_set(HudSet).in_set(PlaySet::Ui));
    }
}

/// What the HUD is drawn with.
struct Art {
    font: Face,
    left_bracket: Option<Handle<Image>>,
    right_bracket: Option<Handle<Image>>,
    enemy_bracket: Option<Handle<Image>>,
    strip: Option<Handle<Image>>,
    arrows: Option<Handle<Image>>,
    /// The tick picture repeated: the corner bars' [`PIPS`] and the enemy's [`ENEMY_PIPS`]
    /// ticks, its top [`TICK`] rows ([`crate::repeat`]).
    ticks: Option<Handle<Image>>,
    enemy_ticks: Option<Handle<Image>>,
}

fn load_art(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Art {
    let mut repeats = crate::repeat::Repeats::default();
    let mut bake = |count: usize, images: &mut Assets<Image>| data.and_then(|d| repeats.baked(TICK_PICTURE, None, ((count as f32 * TICK.x) as u32, TICK.y as u32), d, images));
    let (ticks, enemy_ticks) = (bake(PIPS, images), bake(ENEMY_PIPS, images));
    let mut read = |path: &str| data.and_then(|d| texture(path, d, images, bc)).map(|t| t.0);
    let hud = |name: &str| format!("{HUD_TEXTURES}{name}.dds");
    let (left_bracket, right_bracket) = (read(&hud("glow_hud_left_seperatorglow")), read(&hud("glow_hud_right_seperatorglow")));
    let enemy_bracket = read(&hud("glow_enemy_health_seperator"));
    let strip = read(&hud("glow_hud_comp_direction_strip"));
    let arrows = read(CND_ARROWS);
    Art { font: face(FONT, plain_font(36.0, 15.0), data, images, bc), left_bracket, right_bracket, enemy_bracket, strip, arrows, ticks, enemy_ticks }
}

#[derive(Component)]
pub(crate) struct HudRoot;

#[derive(Component, Clone, Copy, PartialEq, Debug)]
enum Bar {
    Health,
    ActionPoints,
    Enemy,
}

/// The weapon condition's bar.
#[derive(Component)]
struct ConditionBar;

/// The window over a bar's row of ticks as wide as its lit ticks ([`lit_window`]):
/// which bar, and the row's width (interface units) and tick count.
#[derive(Component, Clone, Copy)]
struct PipWindow {
    bar: Bar,
    /// The row's start in its piece.
    x: f32,
    width: f32,
    count: usize,
}

/// The strip of a bar's ticks in its window: the repeated picture, or plain blocks.
#[derive(Component)]
struct PipStrip;

/// The doubled compass strip in its window.
#[derive(Component)]
struct Strip;

/// The marks over the compass (`crate::compass` fills it): the window's place and size,
/// clipped at its sides only, as the marks reach above and below the strip.
#[derive(Component)]
pub struct CompassMarks;

/// The lower row of the right piece: the condition's label and bar.
#[derive(Component)]
struct ConditionRow;

/// Holds the ammo counter's letters.
#[derive(Component)]
struct AmmoText;

/// The node that holds the enemy name's letters.
#[derive(Component)]
struct EnemyName;
/// Holds Living Anatomy's line under the name (`crate::hud_target::TargetHealth::anatomy_line`).
#[derive(Component)]
struct EnemyStats;

/// Holds the sneak word's letters.
#[derive(Component)]
struct SneakText;

/// A bar's row of `count` ticks at `x`, `y` (`width` long): one image of the repeated
/// picture (`ticks`), stretched over the row, in a window [`pips`] sets to the lit ticks;
/// without the picture, plain blocks [design: T12d, every tick `width / count` from the
/// next at any size].
#[allow(clippy::too_many_arguments)]
fn spawn_row(p: &mut ChildSpawnerCommands, bar: Bar, x: f32, y: f32, width: f32, count: usize, color: Color, ticks: &Option<Handle<Image>>) {
    let (left, open, strip) = lit_window(bar, width, count, 0, count);
    let mut window = at(x + left, y, open, TICK.y);
    window.overflow = Overflow::clip();
    p.spawn((PipWindow { bar, x, width, count }, window)).with_children(|w| {
        let node = at(strip, 0.0, width, TICK.y);
        match ticks {
            Some(image) => {
                w.spawn((PipStrip, node, ImageNode { color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }));
            }
            None => {
                w.spawn((PipStrip, node)).with_children(|s| {
                    let period = width / count as f32;
                    for i in 0..count {
                        s.spawn((at(i as f32 * period, 0.0, period * TICK_BLOCK / TICK.x, TICK.y), BackgroundColor(color)));
                    }
                });
            }
        }
    });
}

/// A bracket: the game's picture, or plain lines where its lines are.
fn spawn_bracket(p: &mut ChildSpawnerCommands, image: &Option<Handle<Image>>, uprights: &[f32], color: Color) {
    match image {
        Some(image) => {
            p.spawn((at(0.0, 0.0, BRACKET.x, BRACKET.y), ImageNode { color, ..ImageNode::new(image.clone()) }));
        }
        None => {
            p.spawn((at(LEFT_UPRIGHT, LINE_Y, RIGHT_UPRIGHT - LEFT_UPRIGHT, 2.0), BackgroundColor(color)));
            for &x in uprights {
                p.spawn((at(x, if x == DIVIDER { LINE_Y } else { 10.0 }, 2.0, if x == DIVIDER { 55.0 } else { 115.0 }), BackgroundColor(color)));
            }
        }
    }
}

fn spawn_hud(commands: &mut Commands, art: &Art) {
    let places = Places::new(&art.font.font);
    let color = amber(255.0);
    let font = &art.font.font;
    let label = |p: &mut ChildSpawnerCommands, text: &str, x: f32, base: f32| {
        set_line(p, &art.font, &font.lines(text, None).remove(0), x, base, color);
    };
    let screen = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands.spawn((HudRoot, screen, Visibility::Hidden, Pickable::IGNORE)).with_children(|root| {
        // Health and compass, bottom left.
        let piece = Node { position_type: PositionType::Absolute, left: u(MARGIN_X), bottom: u(MARGIN_Y), width: u(HP_PANEL.x), height: u(HP_PANEL.y), ..default() };
        root.spawn((piece, DrawnByTiles)).with_children(|p| {
            spawn_bracket(p, &art.left_bracket, &[LEFT_UPRIGHT], color);
            label(p, HP, places.hp_label, UPPER_BASE);
            spawn_row(p, Bar::Health, places.hp_meter, places.meter_y, METER_WIDTH, PIPS, amber(METER_BRIGHTNESS), &art.ticks);
            info!("compass: heading at x {x} of {w}", x = COMPASS_HEADING, w = COMPASS.x);
            let mut window = at(COMPASS_AT.x, COMPASS_AT.y, COMPASS.x, COMPASS.y);
            window.overflow = Overflow::clip();
            p.spawn(window).with_children(|w| {
                w.spawn((Strip, at(0.0, 0.0, 2.0 * STRIP.x, STRIP.y))).with_children(|s| {
                    if let Some(image) = &art.strip {
                        for i in 0..2 {
                            s.spawn((at(i as f32 * STRIP.x, 0.0, STRIP.x, STRIP.y), ImageNode { color, ..ImageNode::new(image.clone()) }));
                        }
                    } else {
                        // The eight directions' letters, 45 degrees apart.
                        for i in 0..16 {
                            let name = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][i % 8];
                            let line = font.lines(name, None).remove(0);
                            let x = i as f32 * STRIP.x / 8.0 - line.width / 2.0;
                            set_line(s, &art.font, &line, x, 44.0, color);
                        }
                    }
                });
            });
            let mut marks = at(COMPASS_AT.x, COMPASS_AT.y, COMPASS.x, COMPASS.y);
            marks.overflow = Overflow::clip_x();
            p.spawn((CompassMarks, marks));
        });
        // Action points, condition and ammo, bottom right.
        let piece = Node { position_type: PositionType::Absolute, right: u(MARGIN_X), bottom: u(MARGIN_Y), width: u(AP_PANEL.x), height: u(AP_PANEL.y), ..default() };
        root.spawn((piece, DrawnByTiles)).with_children(|p| {
            spawn_bracket(p, &art.right_bracket, &[RIGHT_UPRIGHT, DIVIDER], color);
            spawn_row(p, Bar::ActionPoints, places.ap_meter, places.meter_y, METER_WIDTH, PIPS, amber(METER_BRIGHTNESS), &art.ticks);
            label(p, AP, places.ap_label, UPPER_BASE);
            p.spawn((ConditionRow, at(0.0, 0.0, DIVIDER, AP_PANEL.y), Visibility::Inherited)).with_children(|row| {
                label(row, CND, places.cnd_label, LOWER_BASE);
                row.spawn((at(places.cnd_x, places.cnd_y, CND_BAR.x, CND_BAR.y), BackgroundColor(color.with_alpha(GROUND))));
                row.spawn((ConditionBar, at(places.cnd_x, places.cnd_y, CND_FILL_WIDTH, CND_BAR.y), BackgroundColor(color)));
                // The jamming threshold's two arrows, over the bar (§4).
                match &art.arrows {
                    Some(image) => {
                        let mut clip = at(places.cnd_x, places.cnd_y, CND_BAR.x, CND_BAR.y);
                        clip.overflow = Overflow::clip();
                        let side = CND_BAR.y + 2.0 * ARROWS_BLEED;
                        row.spawn(clip).with_children(|c| {
                            c.spawn((at(places.arrows_x - places.cnd_x, -ARROWS_BLEED, side, side), ImageNode::new(image.clone())));
                        });
                    }
                    None => {
                        let x = places.cnd_x + JAM_CONDITION * CND_BAR.x - 2.0;
                        row.spawn((at(x, places.cnd_y, 4.0, 5.0), BackgroundColor(Color::BLACK)));
                        row.spawn((at(x, places.cnd_y + CND_BAR.y - 5.0, 4.0, 5.0), BackgroundColor(Color::BLACK)));
                    }
                }
            });
            p.spawn((AmmoText, at(0.0, 0.0, AP_PANEL.x, AP_PANEL.y)));
        });
        // The enemy's health, bottom middle: its long line level with the corner pieces'
        // (§8).
        let red = crime_colour();
        let enemy_top = HP_PANEL.y - LINE_Y + ENEMY_LINE_Y + ENEMY_BRACKET_AT.y - ENEMY.y;
        let piece = Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            margin: UiRect::left(u(-ENEMY.x / 2.0)),
            bottom: u(MARGIN_Y + enemy_top),
            width: u(ENEMY.x),
            height: u(ENEMY.y),
            ..default()
        };
        root.spawn((EnemyPiece, piece, Visibility::Hidden)).with_children(|p| {
            let (bx, by) = (ENEMY_BRACKET_AT.x, ENEMY_BRACKET_AT.y);
            match &art.enemy_bracket {
                Some(image) => {
                    p.spawn((at(bx, by, ENEMY_BRACKET.x, ENEMY_BRACKET.y), ImageNode { color: red, ..ImageNode::new(image.clone()) }));
                }
                None => {
                    p.spawn((at(bx + ENEMY_MARKS.0 - 20.0, by + ENEMY_LINE_Y, ENEMY_MARKS.1 - ENEMY_MARKS.0 + 40.0, 2.0), BackgroundColor(red)));
                }
            }
            let (from, to) = (bx + ENEMY_MARKS.0, bx + ENEMY_MARKS.1);
            spawn_row(p, Bar::Enemy, from, by + ENEMY_LINE_Y - ENEMY_METER_GAP - TICK.y, to - from, ENEMY_PIPS, red, &art.enemy_ticks);
            p.spawn((EnemyName, at(0.0, 0.0, ENEMY.x, ENEMY.y)));
            p.spawn((EnemyStats, at(0.0, 0.0, ENEMY.x, ENEMY.y)));
        });
        // The sneak word, top middle.
        let piece = Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(-SNEAK.x / 2.0)), top: u(0.0), width: u(SNEAK.x), height: u(SNEAK.y + SNEAK_BASE), ..default() };
        root.spawn((SneakText, piece, DrawnByTiles));
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    target: Res<HudTarget>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut art: Local<Option<Art>>,
    mut root: Query<&mut Visibility, (With<HudRoot>, Without<ConditionRow>, Without<EnemyPiece>)>,
    mut condition: Query<&mut Visibility, (With<ConditionRow>, Without<HudRoot>, Without<EnemyPiece>)>,
    mut enemy: Query<&mut Visibility, (With<EnemyPiece>, Without<HudRoot>, Without<ConditionRow>)>,
    mut bar: Query<&mut Node, (With<ConditionBar>, Without<Strip>)>,
    mut strip: Query<&mut Node, (With<Strip>, Without<ConditionBar>)>,
) {
    if art.is_none() {
        // The first frame the HUD is wanted: read the game's pictures and set it up.
        if view.shown {
            let loaded = load_art(data.as_deref(), &mut images, bc_supported(formats.as_deref()));
            spawn_hud(&mut commands, &loaded);
            *art = Some(loaded);
        }
        return;
    }
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(view.shown));
    }
    if !view.shown {
        return;
    }
    for mut v in &mut condition {
        v.set_if_neq(show(view.weapon.condition.is_some()));
    }
    for mut v in &mut enemy {
        v.set_if_neq(show(target.0.is_some()));
    }
    let width = u(condition_width(view.weapon.condition.unwrap_or(0.0)));
    for mut node in &mut bar {
        if node.width != width {
            node.width = width;
        }
    }
    let left = u(strip_left(view.heading));
    for mut node in &mut strip {
        if node.left != left {
            node.left = left;
        }
    }
}

/// Open each bar's window over its lit ticks ([`lit_window`]), the strip held in place.
#[allow(clippy::type_complexity)]
fn pips(
    view: Res<HudView>,
    target: Res<HudTarget>,
    mut windows: Query<(&PipWindow, &mut Node, &Children), Without<PipStrip>>,
    mut strips: Query<&mut Node, (With<PipStrip>, Without<PipWindow>)>,
) {
    if !view.shown {
        return;
    }
    let enemy = target.0.as_ref().map_or(0.0, |t| t.health);
    for (w, mut node, children) in &mut windows {
        let (from, to) = lit_range(w.bar, &view, enemy);
        let (left, open, strip) = lit_window(w.bar, w.width, w.count, from, to);
        let (left, open, strip) = (u(w.x + left), u(open), u(strip));
        if node.left != left {
            node.left = left;
        }
        if node.width != open {
            node.width = open;
        }
        for &c in children {
            if let Ok(mut s) = strips.get_mut(c) {
                if s.left != strip {
                    s.left = strip;
                }
            }
        }
    }
}

/// The texts that change: the ammo counter, the enemy's name, the sneak word.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn texts(
    mut commands: Commands,
    view: Res<HudView>,
    target: Res<HudTarget>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut face_: Local<Option<Face>>,
    mut shown: Local<(Option<String>, Option<String>, Option<Sneak>, Option<String>)>,
    ammo: Query<Entity, With<AmmoText>>,
    name: Query<Entity, With<EnemyName>>,
    stats: Query<Entity, With<EnemyStats>>,
    sneak: Query<Entity, With<SneakText>>,
) {
    if !view.shown || ammo.is_empty() {
        return;
    }
    let face_ = face_.get_or_insert_with(|| face(FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
    let font = &face_.font;
    let text = ammo_text(view.weapon.ammo);
    let abbrev = view.weapon.abbrev.as_deref().unwrap_or("");
    // Both texts make the key: swapping ammo of the same count changes only the label.
    let key = format!("{abbrev}|{text}");
    if shown.0.as_deref() != Some(key.as_str()) {
        if let Ok(e) = ammo.single() {
            let places = Places::new(font);
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(line) = font.lines(&text, None).into_iter().next() {
                    set_line(p, face_, &line, places.ammo_right - line.width, LOWER_BASE, amber(255.0));
                    if let Some(label) = font.lines(abbrev, None).into_iter().next().filter(|_| !abbrev.is_empty()) {
                        set_line(p, face_, &label, AMMO_LABEL_X, LOWER_BASE, amber(255.0));
                    }
                }
            });
            shown.0 = Some(key);
        }
    }
    let enemy = target.0.as_ref().map(|t| t.name.clone());
    if shown.1 != enemy {
        if let Ok(e) = name.single() {
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(line) = enemy.as_deref().and_then(|n| font.lines(n, None).into_iter().next()) {
                    set_line(p, face_, &line, (ENEMY.x - line.width) / 2.0, ENEMY_NAME_BASE, crime_colour());
                }
            });
            shown.1 = enemy;
        }
    }
    // Living Anatomy: "HP:… DT:…" centred under the name, in its colour.
    let anatomy = target.0.as_ref().and_then(|t| t.anatomy_line());
    if shown.3 != anatomy {
        if let Ok(e) = stats.single() {
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(line) = anatomy.as_deref().and_then(|n| font.lines(n, None).into_iter().next()) {
                    set_line(p, face_, &line, (ENEMY.x - line.width) / 2.0, ENEMY_STATS_BASE, crime_colour());
                }
            });
            shown.3 = anatomy;
        }
    }
    if shown.2 != view.sneak {
        if let Ok(e) = sneak.single() {
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(s) = view.sneak {
                    if let Some(line) = font.lines(s.word(), None).into_iter().next() {
                        let colour = if s.warning() { crime_colour() } else { amber(255.0) };
                        set_line(p, face_, &line, (SNEAK.x - line.width) / 2.0, SNEAK_BASE, colour);
                    }
                }
            });
            shown.2 = view.sneak;
        }
    }
}

/// Every part follows the shown actor's hostility, including existing letters/ticks
/// when only hostility changes [observed: private job hud_enemy3, n2_away (main),
/// g2_gecko_half (alt)]. Previously always red [design]. Traverse after spawning so
/// new and resized rows, names, anatomy and fallback blocks share the colour [design].
#[allow(clippy::type_complexity)]
fn enemy_colours(
    target: Res<HudTarget>,
    enemy: Query<Entity, With<EnemyPiece>>,
    children: Query<&Children>,
    mut ink: Query<(Option<&mut ImageNode>, Option<&mut TextColor>, Option<&mut BackgroundColor>)>,
) {
    let colour = if target.0.as_ref().is_some_and(|t| t.hostile) { crime_colour() } else { amber(255.0) };
    for root in &enemy {
        for entity in children.iter_descendants(root) {
            if let Ok((image, text, fill)) = ink.get_mut(entity) {
                if let Some(mut image) = image {
                    if image.color != colour {
                        image.color = colour;
                    }
                }
                if let Some(mut text) = text {
                    text.set_if_neq(TextColor(colour));
                }
                if let Some(mut fill) = fill {
                    fill.set_if_neq(BackgroundColor(colour));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enemy_ink_tracks_hostility_without_rebuilding_any_children() {
        use crate::hud_target::TargetHealth;
        let mut app = App::new();
        app.insert_resource(HudTarget(Some(TargetHealth { form_id: 1, name: "Target".into(), health: 0.5, hostile: false, anatomy: Some((50.0, 2.0)) })))
            .add_systems(Update, enemy_colours);
        let bracket = app.world_mut().spawn(ImageNode::default()).id();
        let tick = app.world_mut().spawn(BackgroundColor(Color::WHITE)).id();
        let name = app.world_mut().spawn(TextColor(Color::WHITE)).id();
        let group = app.world_mut().spawn_empty().add_children(&[tick, name]).id();
        app.world_mut().spawn(EnemyPiece).add_children(&[bracket, group]);
        for hostile in [false, true, false] {
            app.world_mut().resource_mut::<HudTarget>().0.as_mut().unwrap().hostile = hostile;
            app.update();
            let colour = if hostile { crime_colour() } else { amber(255.0) };
            assert_eq!(app.world().get::<ImageNode>(bracket).unwrap().color, colour);
            assert_eq!(app.world().get::<BackgroundColor>(tick).unwrap().0, colour);
            assert_eq!(app.world().get::<TextColor>(name).unwrap().0, colour);
        }
    }

    /// Letters 15 apart, as the HUD's font sets them.
    fn font() -> Font {
        plain_font(36.0, 15.0)
    }

    #[test]
    fn a_bar_rounds_its_lit_ticks_down() {
        assert_eq!(lit(1.0, PIPS), 37);
        assert_eq!(lit(0.5, PIPS), 18);
        assert_eq!(lit(0.01, PIPS), 0);
        assert_eq!(lit(0.0, PIPS), 0);
        // Out of range or non-finite: bounded ticks [design: lit].
        assert_eq!(lit(-0.2, PIPS), 0);
        assert_eq!(lit(1.7, PIPS), 37);
        assert_eq!(lit(f32::NAN, PIPS), 0);
        assert_eq!(lit(f32::INFINITY, PIPS), 0);
        assert_eq!(lit(f32::NEG_INFINITY, PIPS), 0);
        assert_eq!(lit(1.0, 0), 0);
    }

    #[test]
    fn the_corners_match_the_observed_tick_counts() {
        // [observed: hud.md §14.3] Health, then action points.
        for (current, max, ticks) in [
            (200.0, 200.0, 37), (100.0, 200.0, 18), (20.0, 200.0, 3),
            (43.68, 80.0, 20), (52.15, 80.0, 24), (69.7, 80.0, 32), (80.0, 80.0, 37),
        ] {
            assert_eq!(lit(current / max, PIPS), ticks, "{current} of {max}");
        }
    }

    #[test]
    fn the_condition_fill_matches_the_observed_widths() {
        // [observed: hud.md §14.3] Whole units inside the 60-wide ground.
        for (condition, width) in [(0.999, 59.0), (0.5, 30.0), (0.4987, 29.0), (0.1, 6.0)] {
            assert_eq!(condition_width(condition), width, "{condition}");
        }
    }

    #[test]
    fn the_enemy_bar_empties_from_both_ends() {
        assert_eq!(lit_middle(1.0, 31), (0, 31));
        assert_eq!(lit_middle(0.0, 31), (15, 15));
        // Half: 15 lit, 16 gone, 8 from each end [hypothesis: as the other bars].
        assert_eq!(lit_middle(0.5, 31), (8, 23));
        for i in 0..=100 {
            let (from, to) = lit_middle(i as f32 / 100.0, 31);
            assert!(to <= 31 && (31 - to) - from <= 1, "{i}: {from}..{to}");
        }
    }

    /// T12d: a bar's ticks are one stretched image, so at every window size each tick
    /// stands `8 x scale` pixels from the next (within half a pixel of the true period, as
    /// the GPU samples it), and the lit window's edges fall in the clear gaps.
    #[test]
    fn ticks_stand_evenly_at_any_window_size() {
        for (w, h) in [(1280.0f32, 800.0f32), (1920.0, 1080.0), (1280.0, 720.0)] {
            let scale = h / 960.0;
            for (bar, width, count) in [(Bar::Health, METER_WIDTH, PIPS), (Bar::ActionPoints, METER_WIDTH, PIPS), (Bar::Enemy, ENEMY_MARKS.1 - ENEMY_MARKS.0, ENEMY_PIPS)] {
                let period = width / count as f32;
                // The strip's left in the window plus the window's left: the row's start
                // plus the shift, whatever is lit.
                for (from, to) in [(0, count), (0, 3), (count - 5, count), (4, 9)] {
                    let (left, open, strip) = lit_window(bar, width, count, from, to);
                    assert!((left + strip - strip_shift(bar, period)).abs() < 1e-4, "{bar:?} {from}..{to}");
                    // The window's edges a texel into the clear gaps (texels 6 and 7 of
                    // a period), so whole-pixel rounding (half a pixel) cuts no tick.
                    for edge in [left, left + open] {
                        let texel = (edge - strip_shift(bar, period)) / period * TICK.x;
                        let inside = texel.rem_euclid(TICK.x);
                        assert!((from == 0 && edge == left) || (inside - 7.0).abs() < 1e-3, "{bar:?} {from}..{to}: edge at texel {inside}");
                    }
                }
                // The ticks' left edges in pixels, as one image stretched over the row:
                // every period the true one.
                let row_px = 40.0 * scale;
                let edges: Vec<f32> = (0..count).map(|i| row_px + (strip_shift(bar, period) + i as f32 * period) * scale).collect();
                for pair in edges.windows(2) {
                    assert!((pair[1] - pair[0] - period * scale).abs() < 0.5, "{w}x{h} {bar:?}: {pair:?}");
                }
            }
            // Half a pixel is less than the texel either side of the window's edge.
            assert!(0.5 / scale < METER_WIDTH / PIPS as f32 / TICK.x, "{w}x{h}");
            // At 1280 x 800 the true period is 6.67 pixels, not alternately 6 and 7.
            if h == 800.0 {
                assert!((METER_WIDTH / PIPS as f32 * scale - 6.667).abs() < 1e-3);
            }
        }
        // Nothing lit: a shut window.
        assert_eq!(lit_window(Bar::Health, METER_WIDTH, PIPS, 0, 0), (0.0, 0.0, 0.0));
    }

    /// T12d: the bars light from their outer ends, the enemy's from the middle.
    #[test]
    fn the_lit_ranges_follow_the_bars() {
        let view = HudView { shown: true, health: 0.5, action_points: 0.25, ..default() };
        assert_eq!(lit_range(Bar::Health, &view, 0.0), (0, 18));
        assert_eq!(lit_range(Bar::ActionPoints, &view, 0.0), (PIPS - 9, PIPS));
        assert_eq!(lit_range(Bar::Enemy, &view, 0.5), lit_middle(0.5, ENEMY_PIPS));
    }

    #[test]
    fn the_sneak_word_is_the_worst_news() {
        assert_eq!(Sneak::of(false, false, false), Sneak::Hidden);
        assert_eq!(Sneak::of(false, false, true), Sneak::Detected);
        assert_eq!(Sneak::of(false, true, true), Sneak::Caution);
        assert_eq!(Sneak::of(true, true, true), Sneak::Danger);
        assert!(Sneak::Danger.warning() && Sneak::Caution.warning() && !Sneak::Detected.warning());
        assert_eq!(Sneak::Hidden.word(), "[HIDDEN]");
    }

    #[test]
    fn the_compass_keeps_the_heading_at_the_triangle() {
        // Read the strip at x 148 instead of 172.5; its scale stays 1024 per turn
        // [observed: hud.md §14.5].
        let at = |heading: f32| (COMPASS_HEADING - strip_left(heading)).rem_euclid(STRIP.x);
        assert!((at(0.0) - 0.0).abs() < 1e-3 || (at(0.0) - 1024.0).abs() < 1e-3);
        assert!((at(90.0) - 256.0).abs() < 1e-3);
        assert!((at(225.0) - 640.0).abs() < 1e-3);
        assert!((at(-90.0) - 768.0).abs() < 1e-3);
        assert!((at(450.0) - 256.0).abs() < 1e-3);
        // The doubled strip always covers the window.
        for h in 0..720 {
            let left = strip_left(h as f32 * 0.5);
            assert!(left <= 0.0 && left + 2.0 * STRIP.x >= COMPASS.x, "heading {h}: {left}");
        }
        // Turning right moves the strip left.
        assert!(strip_left(100.0) < strip_left(90.0));
    }

    #[test]
    fn ammo_is_loaded_then_carried() {
        assert_eq!(ammo_text(Some((0, 0))), "0/0");
        assert_eq!(ammo_text(Some((6, 42))), "6/42");
        assert_eq!(ammo_text(None), "");
    }

    #[test]
    fn the_ammo_label_stands_right_of_the_divider() {
        const { assert!(AMMO_LABEL_X > DIVIDER && AMMO_LABEL_X < DIVIDER + 2.0 * LABEL_GAP) };
    }

    #[test]
    fn the_pieces_stand_inside_their_brackets() {
        let p = Places::new(&font());
        // A full bar ends at the corner mark on the left and starts at it on the right,
        // and runs to just inside the upright.
        assert_eq!(p.hp_meter + METER_WIDTH, LEFT_MARK - METER_SHIFT);
        assert_eq!(p.ap_meter, RIGHT_MARK - AP_METER_SHIFT);
        assert!(p.hp_meter > LEFT_UPRIGHT && p.hp_meter - LEFT_UPRIGHT < 10.0);
        assert!(p.ap_meter + METER_WIDTH < RIGHT_UPRIGHT && RIGHT_UPRIGHT - (p.ap_meter + METER_WIDTH) < 10.0);
        // The bars stand on the long line, above it; the labels above the bars.
        assert_eq!(p.meter_y + TICK.y + METER_GAP, LINE_Y);
        assert!(UPPER_BASE < p.meter_y);
        assert!(p.hp_label > LEFT_UPRIGHT && p.ap_label + font().width(AP) < RIGHT_UPRIGHT);
        // Condition's label, then its bar next to the divider; the ammo right of it.
        assert_eq!(p.cnd_label + font().width(CND) + CND_LABEL_GAP, p.cnd_x);
        assert_eq!(p.cnd_x + CND_BAR.x + CND_BEFORE_DIVIDER, DIVIDER);
        assert!(p.ammo_right > DIVIDER && p.ammo_right < RIGHT_UPRIGHT);
        // The bar's bottom is on the label's line.
        assert_eq!(p.cnd_y + CND_BAR.y + CND_RAISE, LOWER_BASE);
        assert!(p.cnd_y > LINE_Y);
        // The arrows' middle at three quarters of the bar.
        assert!((p.arrows_x + ARROWS_MIDDLE * (CND_BAR.y + 2.0 * ARROWS_BLEED) - (p.cnd_x + 0.75 * CND_BAR.x)).abs() < 1e-3);
        // The enemy's ticks fit between its bracket's marks.
        // Keep the owner's 31 ticks; the row packs them into the scaled marks [design].
        let room = ENEMY_MARKS.1 - ENEMY_MARKS.0;
        let period = room / ENEMY_PIPS as f32;
        let last_end = strip_shift(Bar::Enemy, period) + (ENEMY_PIPS - 1) as f32 * period + period * TICK_BLOCK / TICK.x;
        assert!(last_end < room && room - last_end < 1.5, "{last_end} of {room}");
    }
}
