//! The grenade indicator (docs/spec-notes/hud.md §15): a mark on the HUD for each live grenade
//! whose blast would reach the player.
//!
//! - **The picture** [documented: `menus\prefabs\HUDTemplates.xml`
//!   `template_explosive_indicator`, placed in `menus\main\hud_main_menu.xml` under
//!   `Explosive_positioning_rect`]: `Interface\HUD\explosive_indicator.dds` in the HUD's
//!   alternative colour (`&hudalt;`), a box of 151 × 187 interface units whose top left is
//!   75 left of and 93 above the positioning rect, the picture at zoom 120 in it, cut to
//!   the box (the tile renderer's reading of `zoom`, `crate::tiles::draw::Fit::Zoom`). The
//!   picture is the top of a ring with a point at its top [observed: the texture].
//! - **Which grenades** [hypothesis: the same rule as the actors' getting away from them,
//!   docs/design/combat-ai.md §17.7]: a live grenade (`crate::combat::Combat::live_grenades`:
//!   lobbed, with an explosion, in flight or at rest) within its explosion's radius plus
//!   [`REACH_MARGIN`] of the player's feet; not the player's own within its first
//!   [`OWN_GRACE`] seconds (it leaves the hand).
//! - **Where** [hypothesis: the game's code moves the positioning rect; observer check 108]:
//!   with the grenade in view, the rect is on the grenade's place on the screen and the
//!   picture upright (the ring over it, its point up). Out of view or behind the camera,
//!   the rect is on the view's edge, [`EDGE_INSET`] in, the way the grenade lies from the
//!   view's middle, the picture turned so its point faces it.
//! - At most [`MOST_SHOWN`] at once, the nearest first [design].
//! - **The player's tweaks** [design: docs/design/tweaks.md]: Hide Grenade Indicator draws
//!   none; Hide It for Harmless Blasts leaves out grenades whose explosion does no damage;
//!   Exploding Destructible Indicator adds objects burning toward a stage that explodes
//!   (`fnv_game::Game::burning_explosives`), by the same reach.
//!
//! A `hud: grenade indicator` line when the count shown changes.

use crate::combat::Combat;
use crate::hud::{HudSet, HudView};
use crate::interface::{bc_supported, texture, u};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{space, PlayerCamera, PlaySet};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::GameData;

/// The picture [documented: the template's `filename`, under `textures\`].
pub const TEXTURE: &str = "textures\\interface\\hud\\explosive_indicator.dds";
/// The template's box and its top left from the positioning rect, interface units
/// [documented: `width`, `height`, `x`, `y`].
pub const BOX: Vec2 = Vec2::new(151.0, 187.0);
pub const BOX_AT: Vec2 = Vec2::new(-75.0, -93.0);
/// The template's `zoom` [documented].
pub const ZOOM: f32 = 120.0;
/// How far past the explosion's radius a grenade still shows, units [hypothesis: as
/// combat-ai.md §17.7's 64].
pub const REACH_MARGIN: f32 = 64.0;
/// Seconds the player's own grenade is not shown after it leaves [design: as §17.7].
pub const OWN_GRACE: f32 = 0.5;
/// How far in from the view's edge a mark of something out of view stands, logical pixels
/// [design: the picture's half width at a 1080-line view].
pub const EDGE_INSET: f32 = 90.0;
/// Marks drawn at most [design].
pub const MOST_SHOWN: usize = 8;

/// Where a mark goes on the view (`size`, logical pixels) and its turn (radians,
/// clockwise on the screen, 0 its point up). `on_screen` is the thing's place on the view
/// when it is in front of the camera; `side` the way it lies from the camera's line of
/// sight, x right and y down the screen (its camera-space x and minus its y).
pub fn mark_place(on_screen: Option<Vec2>, side: Vec2, size: Vec2) -> (Vec2, f32) {
    if let Some(p) = on_screen.filter(|p| p.x >= 0.0 && p.y >= 0.0 && p.x <= size.x && p.y <= size.y) {
        return (p, 0.0);
    }
    let mid = size / 2.0;
    let dir = if side.length_squared() > 1e-8 { side.normalize() } else { Vec2::Y };
    let half = (mid - Vec2::splat(EDGE_INSET)).max(Vec2::ONE);
    // Out to the inset box's edge along `dir`.
    let reach = (half.x / dir.x.abs().max(1e-6)).min(half.y / dir.y.abs().max(1e-6));
    (mid + dir * reach, dir.x.atan2(-dir.y))
}

/// Whether a blast at `at` of `radius` reaches the player's feet at `player`.
pub fn reaches(at: [f32; 3], radius: f32, player: [f32; 3]) -> bool {
    let d = Vec3::from_array(at) - Vec3::from_array(player);
    d.length() <= radius + REACH_MARGIN
}

pub struct GrenadeIndicatorPlugin;

impl Plugin for GrenadeIndicatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw.after(HudSet).in_set(PlaySet::Ui));
    }
}

/// One mark: the positioning rect (turned) holding the picture.
#[derive(Component)]
struct Mark(usize);

/// The picture, or why there is none.
enum Read {
    Pool,
    Missing,
}

/// The places of the things to mark now, nearest first.
fn wanted(view: &HudView, combat: Option<&Combat>, sim: Option<&SimState>, script: Option<&ScriptState>, tweaks: Option<&crate::tweaks::TweakSettings>) -> Vec<[f32; 3]> {
    let (Some(c), Some(sim)) = (combat, sim) else { return Vec::new() };
    if !view.shown || tweaks.is_some_and(|t| t.no_grenade_indicator) {
        return Vec::new();
    }
    let Some(player) = sim.world.peek(FormId::PLAYER_REF).map(|p| p.pos) else { return Vec::new() };
    let skip_harmless = tweaks.is_some_and(|t| t.grenade_indicator_skip_harmless);
    let harmless = |x: FormId| skip_harmless && c.forms.explosions.get(&x).is_some_and(|e| e.damage <= 0.0);
    let mut out: Vec<[f32; 3]> = c
        .live_grenades()
        .into_iter()
        .filter(|g| !(g.thrower == FormId::PLAYER_REF && g.age < OWN_GRACE))
        .filter(|g| !harmless(g.explosion) && reaches(g.at, g.radius, player))
        .map(|g| g.at)
        .collect();
    if tweaks.is_some_and(|t| t.grenade_indicator_destructibles) {
        if let Some(rt) = script.and_then(|s| s.rt.as_ref()) {
            for (r, x) in rt.game.burning_explosives() {
                let Some(e) = c.forms.explosions.get(&x).filter(|_| !harmless(x)) else { continue };
                let Some(pos) = sim.world.peek(r).map(|s| s.pos).or_else(|| sim.refs.get(r).map(|p| p.pos)) else { continue };
                let at = [pos[0], pos[1], pos[2] + crate::combat::DESTRUCTION_BLAST_HEIGHT];
                if sim.present(r.0) && reaches(at, e.radius, player) {
                    out.push(at);
                }
            }
        }
    }
    let d = |p: &[f32; 3]| (Vec3::from_array(*p) - Vec3::from_array(player)).length();
    out.sort_by(|a, b| d(a).total_cmp(&d(b)));
    out.truncate(MOST_SHOWN);
    out
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    combat: Option<Res<Combat>>,
    sim: Option<Res<SimState>>,
    script: Option<Res<ScriptState>>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut marks: Query<(&Mark, &mut Node, &mut UiTransform, &mut Visibility)>,
    mut read: Local<Option<Read>>,
    mut last: Local<usize>,
) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    let places = wanted(&view, combat.as_deref(), sim.as_deref(), script.as_deref(), tweaks.as_deref());
    if read.is_none() {
        if places.is_empty() {
            return;
        }
        let Some(data) = data.as_deref() else { return };
        let Some((image, texels)) = texture(TEXTURE, data, &mut images, bc_supported(formats.as_deref())) else {
            warn!("hud: no grenade indicator: {TEXTURE} not read");
            *read = Some(Read::Missing);
            return;
        };
        // The picture at zoom 120 from the box's top left, cut to the box.
        let drawn = texels * ZOOM / 100.0;
        let shown = drawn.min(BOX);
        let rect = Rect::from_corners(Vec2::ZERO, shown * 100.0 / ZOOM);
        for i in 0..MOST_SHOWN {
            let picture = Node { position_type: PositionType::Absolute, left: u(BOX_AT.x), top: u(BOX_AT.y), width: u(shown.x), height: u(shown.y), ..default() };
            let colour = crate::reticle::crime_colour();
            commands
                .spawn((Mark(i), Node { position_type: PositionType::Absolute, width: Val::Px(0.0), height: Val::Px(0.0), ..default() }, UiTransform::default(), Visibility::Hidden, GlobalZIndex(3), Pickable::IGNORE))
                .with_children(|p| {
                    p.spawn((picture, ImageNode { rect: Some(rect), color: colour, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }, Pickable::IGNORE));
                });
        }
        info!("hud: grenade indicator read: {:.0} x {:.0} texels", texels.x, texels.y);
        *read = Some(Read::Pool);
        return;
    }
    let Some(Read::Pool) = read.as_ref() else { return };
    let camera = cameras.single().ok();
    let mut shown = 0;
    for (mark, mut node, mut turn, mut vis) in &mut marks {
        let place = places.get(mark.0).zip(camera).and_then(|(p, (cam, t))| {
            let size = cam.logical_viewport_size()?;
            let world = space::to_bevy(*p);
            let local = t.affine().inverse().transform_point3(world);
            let on_screen = (local.z < 0.0).then(|| cam.world_to_viewport(t, world).ok()).flatten();
            Some(mark_place(on_screen, Vec2::new(local.x, -local.y), size))
        });
        let Some((at, angle)) = place else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        shown += 1;
        node.left = Val::Px(at.x);
        node.top = Val::Px(at.y);
        turn.rotation = Rot2::radians(angle);
        vis.set_if_neq(Visibility::Inherited);
    }
    if shown != *last {
        info!("hud: grenade indicator: {shown} shown");
        *last = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_view_it_stands_on_the_grenade_upright() {
        let view = Vec2::new(1920.0, 1080.0);
        assert_eq!(mark_place(Some(Vec2::new(500.0, 300.0)), Vec2::new(-1.0, -1.0), view), (Vec2::new(500.0, 300.0), 0.0));
    }

    #[test]
    fn out_of_view_it_stands_on_the_edge_pointing_at_it() {
        let view = Vec2::new(1920.0, 1080.0);
        // Behind and to the right: the right edge, the point turned a quarter clockwise.
        let (at, angle) = mark_place(None, Vec2::new(1.0, 0.0), view);
        assert!((at - Vec2::new(1920.0 - EDGE_INSET, 540.0)).length() < 1e-3, "{at}");
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // Below the view: the bottom edge, the point down.
        let (at, angle) = mark_place(Some(Vec2::new(900.0, 2000.0)), Vec2::new(0.0, 1.0), view);
        assert!((at - Vec2::new(960.0, 1080.0 - EDGE_INSET)).length() < 1e-3, "{at}");
        assert!((angle.abs() - std::f32::consts::PI).abs() < 1e-5);
        // Up and left: on the inset box, the point between up and left.
        let (at, angle) = mark_place(None, Vec2::new(-1.0, -1.0), view);
        assert!((at.y - EDGE_INSET).abs() < 1e-3 && at.x > EDGE_INSET, "{at}");
        assert!((angle + std::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }

    #[test]
    fn a_blast_reaches_by_its_radius_and_a_margin() {
        assert!(reaches([300.0, 0.0, 0.0], 256.0, [0.0; 3]));
        assert!(!reaches([400.0, 0.0, 0.0], 256.0, [0.0; 3]));
    }
}
