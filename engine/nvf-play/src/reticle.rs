//! The crosshair and what is written under it, drawn as the original draws them
//! (docs/spec-notes/reticle.md).
//!
//! - The crosshair is the game's `glow_crosshair.dds`, two arrows pointing in; with
//!   something under it the player can use, `glow_crosshair_filled.dds`, the same arrows
//!   in a box [documented: the HUD's template names the two; observed: the owner's
//!   screenshots, §2, §3].
//! - Under it, the key and the action ("E) Take"), the target's name, and for an item a
//!   line across with its weight and value either side of a stem (§3). [`layout`] sets
//!   them out (pure, tested) from the measurements of §3, in the HUD's font 7.
//! - **Spread**: while a weapon with a firing cycle is drawn the plain crosshair's two
//!   arrows stand apart by the cone a shot would leave in now (`Combat::crosshair_cone`,
//!   bloom included), each arrow's tip on the cone's edge [hypothesis: the HUD file's
//!   `_SpreadPercentage` trait says the code sets a spread; owner's playtest list 5,
//!   item 15]; they move there at `_SpreadChangeSpeed` [documented: 0.125, in
//!   hud_main_menu.xml; hypothesis: a share of the way per frame at 60 frames a second].
//! - **Hostile**: over an actor hostile to the player the crosshair is red, the crime
//!   colour (`crate::hud_target::AimHostile`) [owner's playtest list 5, item 58;
//!   documented: GECK wiki IsCrimeOrEnemy, "the red color of the crosshair"].
//! - Hidden outside play. A script's DisablePlayerControls hides the crosshair with the
//!   movement flag and the texts with the rollover text flag [documented: GECK wiki
//!   DisablePlayerControls, MovementFlag and RolloverTextFlag; controls.md §5].
//! - Without the game's files: a small square, and the texts in Bevy's font.

use crate::activate::outcome::{self, Action};
use crate::activate::{Target, Targeted};
use crate::control::ControlLocks;
use crate::interface::{amber, at, bc_supported, face, plain_font, set_text, Face, Picture, Pictures, Set};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::{Font, Line};
use fnv_sim::forms::BaseData;
use nvf_assets::GameData;

// ---- the interface files' [documented: menus\prefabs\hudtemplates.xml, §2] -------------

const PLAIN: &str = "interface\\hud\\glow_crosshair.dds";
const FILLED: &str = "interface\\hud\\glow_crosshair_filled.dds";
/// The crosshair's picture, 64 by 64, its middle on the view's middle [observed: the
/// arrows' middle is the picture's, and on the screenshots the view's].
const RETICLE: f32 = 64.0;
/// The arrows' tips stand this far either side of the middle in the picture [observed:
/// `glow_crosshair.dds`, the left arrow's tip at texel 30, the right's at 34].
const TIP: f32 = 2.0;
/// [documented: hud_main_menu.xml `_SpreadChangeSpeed`].
const SPREAD_SPEED: f32 = 0.125;
/// The arrows go no further apart than this from the middle [design].
const MAX_SPREAD: f32 = 240.0;
/// Font 7 [documented: the prompt's text box with the glow set and no font of its own;
/// Fallout_default.ini `sFontFile_7`].
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
/// The item's line across, with its stem [hypothesis: the HUD's `info_seperator`
/// template, the only picture of its shape; §3].
const DIVIDER: &str = "interface\\hud\\glow_hud_bottom_info_seperator_divider.dds";
/// The picture's size, drawn texel for unit [observed: its double line is 9 texels and
/// 7 pixels of the 720-high screenshot], and where its stem's middle is in it [observed].
const DIVIDER_SIZE: Vec2 = Vec2::new(512.0, 128.0);
const STEM_IN_PICTURE: f32 = 160.5;

// ---- measured on the owner's screenshots [observed, §3] --------------------------------
// In the interface's units (960 to the view's height) from the view's middle.

/// The action's middle and the name's, across: not the same, on both screenshots.
const ACTION_X: f32 = -3.0;
const NAME_X: f32 = 1.0;
/// The action's line, and the name's under it.
const ACTION_BASE: f32 = 326.5;
const LINE_PITCH: f32 = 40.0;
/// From the key's end to the action's start.
const KEY_GAP: f32 = 11.0;
/// The divider picture's top under the name's line; its stem's middle, across.
const DIVIDER_BELOW: f32 = 8.5;
const STEM_X: f32 = -4.0;
/// The item's figures: their line under the divider picture's top; from the stem, where
/// "WG" starts and the weight ends (left of it) and where "VAL" starts and the value ends
/// (right of it).
const FIGURES_BELOW: f32 = 50.5;
const WG_X: f32 = -145.0;
const WEIGHT_END: f32 = -21.0;
const VAL_X: f32 = 9.5;
const VALUE_END: f32 = 135.0;

/// The key that activates [hypothesis: the key's name and ")", as the screenshots show for
/// E, the Activate control's default (bindings.rs)].
const KEY: &str = "E)";
const WG: &str = "WG";
const VAL: &str = "VAL";

/// What is written under the crosshair.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prompt {
    pub action: String,
    pub name: String,
    /// An item's weight and value, as written.
    pub figures: Option<[String; 2]>,
    /// Drawn in the crime colour: someone else owns the target (`crate::crime`).
    pub crime: bool,
}

impl Prompt {
    pub fn of(t: &Targeted, sim: &SimState) -> Prompt {
        let (mut action, name) = outcome::prompt(&t.name, &t.action);
        // An owned item is stolen, not taken [hypothesis: the game's word; owner's
        // playtest list 2 of 2026-10-12].
        if t.crime && matches!(t.action, Action::Take { .. }) {
            action = "Steal".into();
        }
        let item = match t.action {
            Action::Take { .. } => sim.base_of(t.form_id).and_then(|b| sim.forms.get(b)),
            _ => None,
        };
        let figures = item.and_then(|b| match &b.data {
            BaseData::Item(i) => Some([weight_text(i.weight), value_text(i.value)]),
            _ => None,
        });
        Prompt { action, name, figures, crime: t.crime }
    }
}

/// An item's weight as written: "--" for none [observed: a bottle cap's, §3]; else
/// without the zeros after the point [hypothesis, §5].
pub fn weight_text(w: Option<f32>) -> String {
    match w.filter(|w| *w > 0.0) {
        Some(w) => {
            let s = format!("{w:.2}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        None => "--".into(),
    }
}

/// An item's value as written; "--" for none [hypothesis: as the barter menu's price of
/// an item worth nothing, §5].
pub fn value_text(v: Option<i32>) -> String {
    match v.filter(|v| *v > 0) {
        Some(v) => v.to_string(),
        None => "--".into(),
    }
}

/// The texts set out from the view's middle, and the divider picture's top left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub texts: Vec<Set>,
    pub divider: Option<Vec2>,
}

pub fn layout(p: &Prompt, font: &Font) -> Layout {
    let line = |s: &str| font.lines(s, None).into_iter().next().unwrap_or_default();
    let set = |line: Line, x: f32, base: f32| Set { line, x, base };
    let action = line(&p.action);
    let key = line(KEY);
    let name = line(&p.name);
    let action_x = ACTION_X - action.width / 2.0;
    let name_base = ACTION_BASE + LINE_PITCH;
    // No action (an actor shown by name only, `Action::Name`): the name alone, on its
    // own line [hypothesis: the line stays where it is with an action].
    let mut texts = if p.action.is_empty() {
        vec![set(name.clone(), NAME_X - name.width / 2.0, name_base)]
    } else {
        vec![
            set(key.clone(), action_x - KEY_GAP - key.width, ACTION_BASE),
            set(action, action_x, ACTION_BASE),
            set(name.clone(), NAME_X - name.width / 2.0, name_base),
        ]
    };
    let divider = p.figures.as_ref().map(|[weight, value]| {
        let top = name_base + DIVIDER_BELOW;
        let base = top + FIGURES_BELOW;
        let (weight, value) = (line(weight), line(value));
        texts.push(set(line(WG), STEM_X + WG_X, base));
        texts.push(set(weight.clone(), STEM_X + WEIGHT_END - weight.width, base));
        texts.push(set(line(VAL), STEM_X + VAL_X, base));
        texts.push(set(value.clone(), STEM_X + VALUE_END - value.width, base));
        Vec2::new(STEM_X - STEM_IN_PICTURE, top)
    });
    Layout { texts, divider }
}

/// The crime colour: the prompt over something another owns, and the crosshair with it
/// [documented: GECK wiki IsCrimeOrEnemy, the "red color of the crosshair or the
/// activation prompt"; HudBarSetSystemColor, the system colour "Crime" of the prompt].
/// Its value: the HUD's alternative colour, `iSystemColorHUDAlt*` 255, 67, 42 in
/// Fallout_default.ini [observed; hypothesis: that it is the crime colour].
pub fn crime_colour() -> Color {
    Color::srgb_u8(255, 67, 42)
}

// ---- drawing ----------------------------------------------------------------------------

pub struct ReticlePlugin;

impl Plugin for ReticlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw.in_set(PlaySet::Ui));
    }
}

/// What the crosshair and the prompt are drawn with, read the first time play starts.
struct Kit {
    face: Face,
    plain: Option<Picture>,
    filled: Option<Picture>,
    divider: Option<Picture>,
}

impl Kit {
    fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        let mut pictures = Pictures::load(data);
        let mut picture = |path: &str| pictures.picture(path, data, images, bc);
        let (plain, filled, divider) = (picture(PLAIN), picture(FILLED), picture(DIVIDER));
        Kit { face: face(FONT, plain_font(36.0, 17.0), data, images, bc), plain, filled, divider }
    }
}

#[derive(Component)]
struct Root;

/// One of the two crosshairs: the filled one is shown while something can be used. The
/// plain one is drawn in two halves (`side` −1 left, 1 right) that the spread moves apart.
#[derive(Component)]
struct Cross {
    filled: bool,
    side: f32,
}

/// How far an arrow's tip stands from the view's middle for a cone of `cone` degrees
/// (half-angle) in a view `fov` radians high, in the interface's units (960 to the view's
/// height): where the cone's edge meets the screen.
pub fn spread_units(cone: f32, fov: f32) -> f32 {
    let half = (fov / 2.0).tan();
    if half <= 0.0 {
        return 0.0;
    }
    (cone.to_radians().tan() / half * 480.0).clamp(0.0, MAX_SPREAD)
}

/// The half of a picture: its left or right half.
fn half_rect(rect: Option<Rect>, side: f32) -> Rect {
    let r = rect.unwrap_or(Rect::new(0.0, 0.0, RETICLE, RETICLE));
    let mid = (r.min.x + r.max.x) / 2.0;
    if side < 0.0 {
        Rect::new(r.min.x, r.min.y, mid, r.max.y)
    } else {
        Rect::new(mid, r.min.y, r.max.x, r.max.y)
    }
}

/// The prompt's texts and divider, set again when what they say changes.
#[derive(Component)]
struct Texts;

fn picture_node((image, rect): Picture, color: Color) -> ImageNode {
    ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }
}

fn spawn(commands: &mut Commands, kit: &Kit) {
    let middle = Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(50.0),
        width: Val::Px(0.0),
        height: Val::Px(0.0),
        ..default()
    };
    let color = amber(255.0);
    commands.spawn((Root, middle, Visibility::Hidden, Pickable::IGNORE)).with_children(|r| {
        let half = RETICLE / 2.0;
        match kit.plain.clone() {
            Some((image, rect)) => {
                for side in [-1.0, 1.0] {
                    let x = if side < 0.0 { -half } else { 0.0 };
                    r.spawn((Cross { filled: false, side }, at(x, -half, half, RETICLE), picture_node((image.clone(), Some(half_rect(rect, side))), color), Visibility::Hidden));
                }
            }
            None => {
                for side in [-1.0, 1.0] {
                    r.spawn((Cross { filled: false, side }, at(if side < 0.0 { -5.0 } else { 2.0 }, -1.5, 3.0, 3.0), BackgroundColor(color), Visibility::Hidden));
                }
            }
        }
        match kit.filled.clone() {
            Some(p) => r.spawn((Cross { filled: true, side: 0.0 }, at(-half, -half, RETICLE, RETICLE), picture_node(p, color), Visibility::Hidden)),
            None => r.spawn((Cross { filled: true, side: 0.0 }, at(-3.0, -3.0, 6.0, 6.0), BackgroundColor(color), Visibility::Hidden)),
        };
        r.spawn((Texts, Node { position_type: PositionType::Absolute, ..default() }));
    });
}

fn spawn_texts(p: &mut ChildSpawnerCommands, kit: &Kit, prompt: &Prompt) {
    let color = if prompt.crime { crime_colour() } else { amber(255.0) };
    let l = layout(prompt, &kit.face.font);
    if let (Some(corner), Some(picture)) = (l.divider, kit.divider.clone()) {
        p.spawn((at(corner.x, corner.y, DIVIDER_SIZE.x, DIVIDER_SIZE.y), picture_node(picture, color)));
    }
    for set in &l.texts {
        set_text(p, &kit.face, set, Vec2::ZERO, color, ());
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    state: Res<State<PlayState>>,
    target: Res<Target>,
    sim: Res<SimState>,
    (combat, hostile, camera, time): (Option<Res<crate::combat::Combat>>, Option<Res<crate::hud_target::AimHostile>>, Query<&Projection, With<crate::PlayerCamera>>, Res<Time>),
    mut spread: Local<f32>,
    mut logged: Local<(bool, i32)>,
    locks: Option<Res<ControlLocks>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut kit: Local<Option<Kit>>,
    mut shown: Local<Option<Prompt>>,
    mut root: Query<&mut Visibility, With<Root>>,
    mut crosses: Query<(&Cross, &mut Visibility, &mut Node, Option<&mut ImageNode>), Without<Root>>,
    texts: Query<Entity, With<Texts>>,
) {
    let playing = *state.get() == PlayState::Playing;
    let Some(kit) = kit.as_ref() else {
        if playing {
            let loaded = Kit::load(data.as_deref(), &mut images, bc_supported(formats.as_deref()));
            spawn(&mut commands, &loaded);
            *kit = Some(loaded);
        }
        return;
    };
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(playing));
    }
    if !playing {
        return;
    }
    let locks = locks.as_deref().copied().unwrap_or_default();
    let aimed = target.0.is_some();
    let foe = hostile.is_some_and(|h| h.0);
    let colour = if foe || target.0.as_ref().is_some_and(|t| t.crime) { crime_colour() } else { amber(255.0) };
    // The arrows' tips on the cone's edge, moving there a share of the way each frame.
    let fov = camera.iter().find_map(|p| match p {
        Projection::Perspective(p) => Some(p.fov),
        _ => None,
    });
    let want = match (combat.as_ref().and_then(|c| c.crosshair_cone), fov) {
        (Some(cone), Some(fov)) => spread_units(cone, fov),
        _ => 0.0,
    };
    let share = (SPREAD_SPEED * time.delta_secs() * 60.0).min(1.0);
    *spread += (want - *spread) * share;
    if (want - *spread).abs() < 0.05 {
        *spread = want;
    }
    // One line when the colour changes or the spread settles somewhere new.
    let now = (foe, spread.round() as i32);
    if now.0 != logged.0 || (*spread == want && (now.1 - logged.1).abs() >= 2) {
        info!("reticle: {} spread {:.1} (cone {:.2} deg)", if foe { "hostile" } else { "plain" }, *spread, combat.as_ref().and_then(|c| c.crosshair_cone).unwrap_or(0.0));
        *logged = now;
    }
    let shift = (*spread - TIP).max(0.0);
    for (c, mut v, mut node, image) in &mut crosses {
        v.set_if_neq(show(!locks.movement && c.filled == aimed));
        if let Some(mut image) = image.filter(|i| i.color != colour) {
            image.color = colour;
        }
        if !c.filled {
            let half = RETICLE / 2.0;
            let x = if c.side < 0.0 { -half - shift } else { shift };
            let want_left = crate::interface::u(x);
            if node.left != want_left && kit.plain.is_some() {
                node.left = want_left;
            }
        }
    }
    let want = target.0.as_ref().filter(|_| !locks.rollover).map(|t| Prompt::of(t, &sim));
    if *shown != want {
        let Ok(e) = texts.single() else { return };
        commands.entity(e).despawn_related::<Children>().with_children(|p| {
            if let Some(prompt) = &want {
                spawn_texts(p, kit, prompt);
            }
        });
        *shown = want;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 17 wide: about the game's font 7.
    fn font() -> Font {
        plain_font(36.0, 17.0)
    }

    fn prompt(action: &str, name: &str, figures: Option<[&str; 2]>) -> Prompt {
        Prompt { action: action.into(), name: name.into(), figures: figures.map(|f| f.map(String::from)), crime: false }
    }

    #[test]
    fn the_action_and_the_name_are_centred_with_the_key_before_the_action() {
        let l = layout(&prompt("Open", "Gun Case", None), &font());
        assert_eq!(l.divider, None);
        let [key, action, name] = &l.texts[..] else { panic!("{:?}", l.texts) };
        // The key's end is the gap before the action, whatever the name's width.
        assert_eq!((key.x + key.line.width, key.base), (action.x - KEY_GAP, ACTION_BASE));
        assert_eq!(action.x + action.line.width / 2.0, ACTION_X);
        assert_eq!((name.x + name.line.width / 2.0, name.base), (NAME_X, ACTION_BASE + LINE_PITCH));
        let wider = layout(&prompt("Open", "A Much Longer Name", None), &font());
        assert_eq!(wider.texts[0], l.texts[0]);
    }

    #[test]
    fn an_items_figures_stand_either_side_of_the_stem() {
        let l = layout(&prompt("Take", "Bottle Cap", Some(["--", "1"])), &font());
        let corner = l.divider.unwrap();
        assert_eq!(corner.x + STEM_IN_PICTURE, STEM_X);
        assert_eq!(corner.y, ACTION_BASE + LINE_PITCH + DIVIDER_BELOW);
        let figures = &l.texts[3..];
        assert_eq!(figures.len(), 4);
        assert!(figures.iter().all(|s| s.base == corner.y + FIGURES_BELOW));
        let [wg, weight, val, value] = figures else { unreachable!() };
        assert_eq!(wg.x, STEM_X + WG_X);
        assert_eq!(weight.x + weight.line.width, STEM_X + WEIGHT_END);
        assert_eq!(val.x, STEM_X + VAL_X);
        assert_eq!(value.x + value.line.width, STEM_X + VALUE_END);
    }

    #[test]
    fn the_arrows_stand_on_the_cones_edge() {
        let fov = 2.0 * (0.5f32).atan();
        // tan(fov / 2) = 0.5: a cone of 45 degrees reaches twice the half height, capped.
        assert_eq!(spread_units(0.0, fov), 0.0);
        assert!((spread_units(1.0, fov) - 1f32.to_radians().tan() / 0.5 * 480.0).abs() < 1e-3);
        assert_eq!(spread_units(45.0, fov), MAX_SPREAD);
        assert_eq!(half_rect(None, -1.0), Rect::new(0.0, 0.0, 32.0, 64.0));
        assert_eq!(half_rect(Some(Rect::new(10.0, 0.0, 30.0, 20.0)), 1.0), Rect::new(20.0, 0.0, 30.0, 20.0));
    }

    #[test]
    fn weights_and_values_are_written_as_on_the_hud() {
        assert_eq!(weight_text(Some(0.0)), "--");
        assert_eq!(weight_text(None), "--");
        assert_eq!(weight_text(Some(2.0)), "2");
        assert_eq!(weight_text(Some(0.5)), "0.5");
        assert_eq!(weight_text(Some(0.25)), "0.25");
        assert_eq!(value_text(Some(1)), "1");
        assert_eq!(value_text(Some(0)), "--");
        assert_eq!(value_text(None), "--");
    }
}
