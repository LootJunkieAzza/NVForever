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

use crate::combat::{Combat, HudWeapon};
use crate::control::ControlLocks;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::ui::{amber, at, bc_supported, face, plain_font, set_line, texture, u, Face};
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_font::Font;
use fnv_game::actor_values::ACTION_POINTS;
use nvf_assets::GameData;

// ---- the interface files' numbers [documented: hud.md §2, §3] -------------------------

/// The two corner pieces (`HitPoints`, `ActionPoints`): width and height.
const HP_PANEL: Vec2 = Vec2::new(369.0, 127.0);
const AP_PANEL: Vec2 = Vec2::new(386.0, 127.0);
/// The bracket pictures' room; the pictures are drawn texel for unit.
const BRACKET: Vec2 = Vec2::new(512.0, 256.0);
/// One tick of a bar. (The file's own length for a full bar, 300, is not used: §4.)
const TICK: Vec2 = Vec2::new(8.0, 20.0);
/// The compass window in the health piece: place and size.
const COMPASS_AT: Vec2 = Vec2::new(20.0, 65.0);
const COMPASS: Vec2 = Vec2::new(345.0, 64.0);
/// The whole strip, north to north: 1024 texels for 360 degrees [observed: the texture].
const STRIP: Vec2 = Vec2::new(1024.0, 64.0);
/// The solid bar's ground (weapon condition): place across, length and height.
const CND_X: f32 = 90.0;
const CND_BAR: Vec2 = Vec2::new(60.0, 20.0);

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

// ---- set here; the files leave these to the game's code [hypothesis, §4] --------------

/// From the view's side and bottom edges to the pieces.
const MARGIN_X: f32 = 40.0;
const MARGIN_Y: f32 = 30.0;
/// The bars stand this far above the long line.
const METER_GAP: f32 = 4.0;
/// Room between a label and what it labels, and between an upright and a label.
const LABEL_GAP: f32 = 6.0;
/// A full bar's length: from the label to the bracket's corner mark, in whole ticks.
const METER_WIDTH: f32 = 256.0;
/// The line the texts of the lower row stand on, and where the upper row's stand.
const LOWER_BASE: f32 = 106.0;
const UPPER_BASE: f32 = LINE_Y - METER_GAP - 2.0;
/// The ground of the condition bar is this much of the bar's own brightness
/// [hypothesis: as `meter.xml`'s ground, a quarter].
const GROUND: f32 = 0.25;

/// The labels [hypothesis: the game takes them from its settings (`sStats…`); the
/// owner's screenshot shows these words].
const HP: &str = "HP";
const AP: &str = "AP";
const CND: &str = "CND";

/// The font of every text on the HUD: font 7 [documented: `hudtemplates.xml`;
/// Fallout_default.ini `sFontFile_7`].
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

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
}

impl Default for HudView {
    fn default() -> Self {
        HudView { shown: false, health: 1.0, action_points: 1.0, heading: 0.0, weapon: HudWeapon::default() }
    }
}

// ---- layout --------------------------------------------------------------------------

/// The length of a bar at `value` (0 to 1) [hypothesis: a tick that does not fit whole
/// is cut].
pub fn meter_length(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0) * METER_WIDTH
    } else {
        0.0
    }
}

/// How many ticks a full bar takes, the cut one included.
fn tick_count() -> usize {
    (METER_WIDTH / TICK.x).ceil() as usize
}

/// Where the doubled compass strip starts in its window (0 or less), so that `heading`
/// (degrees) is in the window's middle (§4).
pub fn strip_left(heading: f32) -> f32 {
    let middle = heading.rem_euclid(360.0) / 360.0 * STRIP.x;
    -(middle - COMPASS.x / 2.0).rem_euclid(STRIP.x)
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
    /// The "CND" label's start, the condition bar's top, and where the ammo's text ends.
    pub cnd_label: f32,
    pub cnd_y: f32,
    pub ammo_right: f32,
}

impl Places {
    pub fn new(font: &Font) -> Places {
        // [hypothesis, §4] A full bar reaches the bracket's corner mark; its label
        // stands at its outer end, between the bar and the bracket's upright.
        let hp_meter = LEFT_MARK - METER_WIDTH;
        let ap_meter = RIGHT_MARK;
        Places {
            hp_label: hp_meter - LABEL_GAP - font.width(HP),
            hp_meter,
            ap_meter,
            ap_label: ap_meter + METER_WIDTH + LABEL_GAP,
            meter_y: LINE_Y - METER_GAP - TICK.y,
            cnd_label: CND_X - LABEL_GAP - font.width(CND),
            cnd_y: LOWER_BASE - CND_BAR.y,
            ammo_right: RIGHT_UPRIGHT - 2.0 * LABEL_GAP,
        }
    }
}

// ---- reading the game ------------------------------------------------------------------

/// Read what the HUD shows. Health and action points come from the script runtime's
/// actor values; until it runs they read full.
fn gather(
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    combat: Option<Res<Combat>>,
    locks: Option<Res<ControlLocks>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut view: ResMut<HudView>,
) {
    // [documented: GECK wiki DisablePlayerControls, the movement flag: "HUD is partially
    // disabled: Action Points, Health, Crosshair".]
    let held = locks.is_some_and(|l| l.movement);
    let mut now = HudView { shown: *state.get() == PlayState::Playing && !held, ..view.clone() };
    if let Some(p) = &player {
        now.heading = p.heading.to_degrees();
    }
    if let Some(c) = &combat {
        now.weapon = c.hud_weapon(&sim);
    }
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        (now.health, now.action_points) = with_world(rt, &mut sim, |rt| {
            let max = rt.game.max_health(FormId::PLAYER_REF);
            let health = fnv_game::functions::actor_health(&mut rt.game, FormId::PLAYER_REF).unwrap_or(max);
            // Base, permanent modifier, damage: the bar is what is left of the first two.
            let ap = match fnv_game::functions::actor_value_parts(&mut rt.game, FormId::PLAYER_REF, ACTION_POINTS) {
                Ok([base, modifier, damage]) if base + modifier > 0.0 => (base + modifier - damage) / (base + modifier),
                _ => 1.0,
            };
            (if max > 0.0 { (health / max) as f32 } else { 0.0 }, ap as f32)
        });
    }
    view.set_if_neq(now);
}

// ---- drawing ---------------------------------------------------------------------------

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudView>().add_systems(Update, (gather, draw).chain().in_set(PlaySet::Ui));
    }
}

/// What the HUD is drawn with.
struct Art {
    font: Face,
    left_bracket: Option<Handle<Image>>,
    right_bracket: Option<Handle<Image>>,
    tick: Option<Handle<Image>>,
    strip: Option<Handle<Image>>,
}

fn load_art(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Art {
    let mut hud = |name: &str| data.and_then(|d| texture(&format!("{HUD_TEXTURES}{name}.dds"), d, images, bc)).map(|t| t.0);
    let (left_bracket, right_bracket) = (hud("glow_hud_left_seperatorglow"), hud("glow_hud_right_seperatorglow"));
    let (tick, strip) = (hud("hud_tick_mark"), hud("glow_hud_comp_direction_strip"));
    Art { font: face(FONT, plain_font(36.0, 15.0), data, images, bc), left_bracket, right_bracket, tick, strip }
}

#[derive(Component)]
struct HudRoot;

#[derive(Component, Clone, Copy, PartialEq)]
enum Bar {
    Health,
    ActionPoints,
    Condition,
}

/// The doubled compass strip in its window.
#[derive(Component)]
struct Strip;

/// The lower row of the right piece: the condition's label and bar.
#[derive(Component)]
struct ConditionRow;

/// Holds the ammo counter's letters.
#[derive(Component)]
struct AmmoText;

/// A bar of ticks: a window `length` long over a row of ticks; `from_right` keeps the
/// window's right end in place (the action points bar empties from the left).
fn spawn_meter(p: &mut ChildSpawnerCommands, bar: Bar, x: f32, y: f32, from_right: bool, art: &Art, color: Color) {
    let mut window = at(x, y, METER_WIDTH, TICK.y);
    window.overflow = Overflow::clip();
    if from_right {
        (window.left, window.right) = (Val::Auto, u(AP_PANEL.x - x - METER_WIDTH));
    }
    p.spawn((bar, window)).with_children(|w| {
        for i in 0..tick_count() {
            let mut tick = at(i as f32 * TICK.x, 0.0, TICK.x, TICK.y);
            if from_right {
                (tick.left, tick.right) = (Val::Auto, u(i as f32 * TICK.x));
            }
            match &art.tick {
                // The picture is taller than a tick: its top part is the tick.
                Some(image) => w.spawn((tick, ImageNode { rect: Some(Rect::new(0.0, 0.0, TICK.x, TICK.y)), color, ..ImageNode::new(image.clone()) })),
                None => {
                    tick.width = u(TICK.x - 3.0);
                    w.spawn((tick, BackgroundColor(color)))
                }
            };
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
        root.spawn(piece).with_children(|p| {
            spawn_bracket(p, &art.left_bracket, &[LEFT_UPRIGHT], color);
            label(p, HP, places.hp_label, UPPER_BASE);
            spawn_meter(p, Bar::Health, places.hp_meter, places.meter_y, false, art, color);
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
        });
        // Action points, condition and ammo, bottom right.
        let piece = Node { position_type: PositionType::Absolute, right: u(MARGIN_X), bottom: u(MARGIN_Y), width: u(AP_PANEL.x), height: u(AP_PANEL.y), ..default() };
        root.spawn(piece).with_children(|p| {
            spawn_bracket(p, &art.right_bracket, &[RIGHT_UPRIGHT, DIVIDER], color);
            spawn_meter(p, Bar::ActionPoints, places.ap_meter, places.meter_y, true, art, color);
            label(p, AP, places.ap_label, UPPER_BASE);
            p.spawn((ConditionRow, at(0.0, 0.0, DIVIDER, AP_PANEL.y), Visibility::Inherited)).with_children(|row| {
                label(row, CND, places.cnd_label, LOWER_BASE);
                row.spawn((at(CND_X, places.cnd_y, CND_BAR.x, CND_BAR.y), BackgroundColor(color.with_alpha(GROUND))));
                row.spawn((Bar::Condition, at(CND_X, places.cnd_y, CND_BAR.x, CND_BAR.y), BackgroundColor(color)));
            });
            p.spawn((AmmoText, at(0.0, 0.0, AP_PANEL.x, AP_PANEL.y)));
        });
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut art: Local<Option<Art>>,
    mut ammo_shown: Local<Option<String>>,
    mut root: Query<&mut Visibility, (With<HudRoot>, Without<ConditionRow>)>,
    mut condition: Query<&mut Visibility, (With<ConditionRow>, Without<HudRoot>)>,
    mut bars: Query<(&Bar, &mut Node), Without<Strip>>,
    mut strip: Query<&mut Node, With<Strip>>,
    ammo: Query<Entity, With<AmmoText>>,
) {
    let Some(art) = art.as_ref() else {
        // The first frame the HUD is wanted: read the game's pictures and set it up.
        if view.shown {
            let loaded = load_art(data.as_deref(), &mut images, bc_supported(formats.as_deref()));
            spawn_hud(&mut commands, &loaded);
            *art = Some(loaded);
        }
        return;
    };
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
    for (bar, mut node) in &mut bars {
        let width = match bar {
            Bar::Health => u(meter_length(view.health)),
            Bar::ActionPoints => u(meter_length(view.action_points)),
            Bar::Condition => u(view.weapon.condition.unwrap_or(0.0).clamp(0.0, 1.0) * CND_BAR.x),
        };
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
    let text = ammo_text(view.weapon.ammo);
    if ammo_shown.as_deref() != Some(text.as_str()) {
        if let Ok(e) = ammo.single() {
            let places = Places::new(&art.font.font);
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                if let Some(line) = art.font.font.lines(&text, None).into_iter().next() {
                    set_line(p, &art.font, &line, places.ammo_right - line.width, LOWER_BASE, amber(255.0));
                }
            });
            *ammo_shown = Some(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Letters 15 apart, as the HUD's font sets them.
    fn font() -> Font {
        plain_font(36.0, 15.0)
    }

    #[test]
    fn a_bar_is_as_long_as_its_value() {
        assert_eq!(meter_length(1.0), 256.0);
        assert_eq!(meter_length(0.5), 128.0);
        assert_eq!(meter_length(0.0), 0.0);
        // Out of range, or not a number: an empty or a full bar, never more.
        assert_eq!(meter_length(-0.2), 0.0);
        assert_eq!(meter_length(1.7), 256.0);
        assert_eq!(meter_length(f32::NAN), 0.0);
        assert_eq!(tick_count(), 32);
    }

    #[test]
    fn the_compass_keeps_the_heading_in_the_middle() {
        // What is in the window's middle, in texels of one strip.
        let middle = |heading: f32| (COMPASS.x / 2.0 - strip_left(heading)).rem_euclid(STRIP.x);
        assert!((middle(0.0) - 0.0).abs() < 1e-3 || (middle(0.0) - 1024.0).abs() < 1e-3);
        assert!((middle(90.0) - 256.0).abs() < 1e-3);
        assert!((middle(225.0) - 640.0).abs() < 1e-3);
        assert!((middle(-90.0) - 768.0).abs() < 1e-3);
        assert!((middle(450.0) - 256.0).abs() < 1e-3);
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
    fn the_pieces_stand_inside_their_brackets() {
        let p = Places::new(&font());
        // A full bar ends at the corner mark on the left and starts at it on the right.
        assert_eq!(p.hp_meter + METER_WIDTH, LEFT_MARK);
        assert_eq!(p.ap_meter, RIGHT_MARK);
        // The bars stand on the long line, above it.
        assert_eq!(p.meter_y + TICK.y + METER_GAP, LINE_Y);
        // The labels stand between their bars and the uprights.
        assert!(p.hp_label > LEFT_UPRIGHT && p.hp_label + font().width(HP) < p.hp_meter);
        assert!(p.ap_label > p.ap_meter + METER_WIDTH && p.ap_label + font().width(AP) < RIGHT_UPRIGHT);
        // Condition's label, then its bar, left of the divider; the ammo right of it.
        assert_eq!(p.cnd_label + font().width(CND) + LABEL_GAP, CND_X);
        assert!(CND_X + CND_BAR.x < DIVIDER && p.ammo_right > DIVIDER && p.ammo_right < RIGHT_UPRIGHT);
        assert!(p.cnd_y > LINE_Y);
    }
}
