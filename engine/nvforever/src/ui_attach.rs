//! AttachUIComponent's and AttachUIXML's rects over their model nodes (issue #1835;
//! docs/spec-notes/menu-xml.md §9, "Attached components").
//!
//! [documented: AttachUIComponent] The component goes into one parent rect under
//! `HUDMainMenu` and follows a named node of a loaded model, the name case-sensitive.
//! `fnv_game::ui_more` keeps each attachment (reference, node, rect) and
//! [`fnv_game::ui_more::place_attachments`] writes the rect's place; this finds the node and
//! its point on screen each frame.
//!
//! - The node: an actor's skeleton node by its name as the skeleton spells it
//!   ([`ActorRig`]'s `skel.by_name`), else a named entity (`Name`, the NIF block's name as
//!   `nvf_assets::nif_spawn` gives it) under one of the reference's drawn entities
//!   ([`RefVisual`]): a held weapon's nodes, an activator's or a dropped item's model.
//!   A static's baked shapes keep no nodes, so a name there finds nothing [design].
//! - The camera: the player's eyes ([`MainCamera`], the `PlayerCamera` marker on main
//!   today), its projection after this frame's camera update. The view is taken to fill
//!   the window, as the HUD's interface units do (`nvf_play::tiles::screen_size`) [design].
//! - Units: interface units, 960 to the window's height, `screen()`'s space (draw.rs).
//!
//! What the page leaves open, decided here [design; the page says nothing of them]:
//! - Behind the camera, nearer than its near plane or past its far plane: hidden.
//! - Off screen: hidden as soon as the node's point leaves the view. A rect whose node is
//!   on screen draws whole, cut only by the window's edge (no clipping of its own).
//! - No occlusion test: a rect shows through walls and other models.
//! - No scaling with distance: the rect keeps its authored size.
//! - The reference not drawn (no render entity, or its model hidden: a disabled or
//!   culled reference, `InheritedVisibility`), the node not loaded, no active camera:
//!   hidden. A reference out of the loaded cells loses its attachment altogether
//!   (`fnv_game::ui_more::prune_attachments`).
//!
//! Timing: in `PostUpdate`, after the transforms (this frame's animated pose) and the
//! camera are worked out; the tile drawing (`nvf_play::tiles::draw`, in `Update`) shows the
//! new place on the next frame, a frame behind the model [design].
//!
//! Log: one line when an attachment is first placed and whenever it comes on or goes off
//! screen, `ui: attached <rect> on <reference> node "<node>" shown at (x, y)` or
//! `… hidden (<why>)`.

use crate::limbs::ActorRig;
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;
use nvf_play::{PlayerCamera, RefVisual};

/// The camera the attached rects are projected through: the player's main view. One alias,
/// so a later marker for the main view camera (#1753) is switched in here alone [design].
pub type MainCamera = With<PlayerCamera>;

/// Why an attached rect is hidden this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hidden {
    /// No active main camera, or a window of no size.
    NoView,
    /// The reference has no render entity, or its model is hidden.
    NotDrawn,
    /// No node of that name in the reference's drawn model.
    NoNode,
    /// The node's point is behind the camera.
    Behind,
    /// In front, but nearer than the near plane or past the far plane.
    OutOfDepth,
    /// In front, but outside the view's sides.
    OffScreen,
}

impl Hidden {
    fn words(self) -> &'static str {
        match self {
            Hidden::NoView => "no camera",
            Hidden::NotDrawn => "not drawn",
            Hidden::NoNode => "no such node",
            Hidden::Behind => "behind the camera",
            Hidden::OutOfDepth => "outside the camera's depth",
            Hidden::OffScreen => "off the screen",
        }
    }
}

/// A world point (Bevy space) through a camera's `clip_from_world` to interface units from
/// the view's top left, on a screen `screen` interface units wide and high
/// (`nvf_play::tiles::screen_size`: 960 high). The rules above: behind the camera, outside
/// its depth or outside the view's sides is hidden [design]. The view's edges count as on
/// screen.
pub fn interface_point(clip_from_world: Mat4, point: Vec3, screen: (f64, f64)) -> Result<(f64, f64), Hidden> {
    let (width, height) = screen;
    if !(width > 0.0 && height > 0.0) {
        return Err(Hidden::NoView);
    }
    let clip = clip_from_world * point.extend(1.0);
    if !clip.is_finite() || clip.w <= f32::EPSILON {
        return Err(Hidden::Behind);
    }
    let ndc = clip.truncate() / clip.w;
    // Bevy's depth runs 1 at the near plane to 0 at the far one (or infinity).
    if !(0.0..=1.0).contains(&ndc.z) {
        return Err(Hidden::OutOfDepth);
    }
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
        return Err(Hidden::OffScreen);
    }
    Ok(((f64::from(ndc.x) + 1.0) * 0.5 * width, (1.0 - f64::from(ndc.y)) * 0.5 * height))
}

/// What [`node_point`] reads of the drawn world.
type Rigs<'w, 's> = Query<'w, 's, (&'static ActorRig, Option<&'static InheritedVisibility>)>;
type Drawn<'w, 's> = Query<'w, 's, (Entity, &'static RefVisual, Option<&'static InheritedVisibility>), Without<ActorRig>>;

/// The named node of `reference`'s drawn model in world space (the module's rules).
fn node_point(reference: u32, node: &str, rigs: &Rigs, drawn: &Drawn, children: &Query<&Children>, names: &Query<&Name>, globals: &Query<&GlobalTransform>) -> Result<Vec3, Hidden> {
    let shown = |v: Option<&InheritedVisibility>| v.is_none_or(|v| v.get());
    let mut any = false;
    if let Some((rig, vis)) = rigs.iter().find(|(rig, _)| rig.reference == reference) {
        if !shown(vis) {
            return Err(Hidden::NotDrawn);
        }
        any = true;
        if let Some(&e) = rig.skel.by_name.get(node).and_then(|&i| rig.nodes.get(i)) {
            return globals.get(e).map(GlobalTransform::translation).map_err(|_| Hidden::NoNode);
        }
    }
    for (e, _, vis) in drawn.iter().filter(|(_, r, _)| r.0 == reference) {
        any = true;
        if !shown(vis) {
            continue;
        }
        for d in std::iter::once(e).chain(children.iter_descendants(e)) {
            if names.get(d).is_ok_and(|n| n.as_str() == node) {
                if let Ok(t) = globals.get(d) {
                    return Ok(t.translation());
                }
            }
        }
    }
    Err(if any { Hidden::NoNode } else { Hidden::NotDrawn })
}

/// Places every attached rect (nothing to do, and nothing read, without one).
#[allow(clippy::too_many_arguments)]
fn place(
    script: Option<ResMut<ScriptState>>,
    cameras: Query<(&Camera, &GlobalTransform), MainCamera>,
    windows: Query<&Window, With<PrimaryWindow>>,
    rigs: Rigs,
    drawn: Drawn,
    children: Query<&Children>,
    names: Query<&Name>,
    globals: Query<&GlobalTransform>,
) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_extra.attached.is_empty() {
        return;
    }
    let view = cameras.iter().find(|(c, _)| c.is_active).map(|(c, t)| c.clip_from_view() * t.to_matrix().inverse());
    let screen = windows.single().ok().map(|w| nvf_play::tiles::screen_size(w.width(), w.height()));
    let mut why: Vec<(FormId, String, Hidden)> = Vec::new();
    let changed = fnv_game::ui_more::place_attachments(&mut rt.game, |reference, node| {
        let at = match (view, screen) {
            (Some(m), Some(s)) => node_point(reference.0, node, &rigs, &drawn, &children, &names, &globals).and_then(|p| interface_point(m, p, s)),
            _ => Err(Hidden::NoView),
        };
        at.map_err(|h| why.push((reference, node.to_string(), h))).ok()
    });
    for c in changed {
        let head = format!("ui: attached {} on {:08X} node {:?}", c.rect, c.reference.0, c.node);
        match c.at {
            Some((x, y)) => info!("{head} shown at ({x:.0}, {y:.0})"),
            None => {
                let reason = why.iter().find(|w| w.0 == c.reference && w.1 == c.node).map_or("hidden", |w| w.2.words());
                info!("{head} hidden ({reason})");
            }
        }
    }
}

pub struct UiAttachPlugin;

impl Plugin for UiAttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            place
                .after(bevy::transform::TransformSystems::Propagate)
                .after(bevy::camera::CameraUpdateSystems)
                .after(VisibilitySystems::VisibilityPropagate),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1280 x 720 in interface units: 960 high, 960 x 16/9 wide.
    const SCREEN: (f64, f64) = (960.0 * 1280.0 / 720.0, 960.0);

    /// A camera at `at` looking down -Z, 90 degrees high, 16:9, near plane 0.1, through
    /// Bevy's own perspective (infinite, reversed depth), as the player's camera has it.
    fn camera(at: Vec3) -> Mat4 {
        let proj = Projection::Perspective(PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: 16.0 / 9.0, near: 0.1, ..default() });
        proj.get_clip_from_view() * Mat4::from_translation(at).inverse()
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn a_point_ahead_lands_at_its_place_in_interface_units() {
        let m = camera(Vec3::ZERO);
        let centre = interface_point(m, Vec3::new(0.0, 0.0, -10.0), SCREEN).unwrap();
        assert!(near(centre, (SCREEN.0 / 2.0, 480.0)), "{centre:?}");
        // Half way up the view (tan 45 = 1: the view is 20 high at 10 ahead).
        let up = interface_point(m, Vec3::new(0.0, 5.0, -10.0), SCREEN).unwrap();
        assert!(near(up, (SCREEN.0 / 2.0, 240.0)), "{up:?}");
        // Right is right, down is down; farther away the same direction is the same place.
        let p = interface_point(m, Vec3::new(8.0, -2.5, -10.0), SCREEN).unwrap();
        let q = interface_point(m, Vec3::new(80.0, -25.0, -100.0), SCREEN).unwrap();
        assert!(p.0 > SCREEN.0 / 2.0 && p.1 > 480.0);
        assert!(near(p, q), "{p:?} {q:?}");
    }

    #[test]
    fn the_camera_moving_moves_the_point() {
        let m = camera(Vec3::new(100.0, 0.0, 0.0));
        let centre = interface_point(m, Vec3::new(100.0, 0.0, -10.0), SCREEN).unwrap();
        assert!(near(centre, (SCREEN.0 / 2.0, 480.0)), "{centre:?}");
        // A wider window keeps the height and widens the units.
        let wide = (960.0 * 2.0, 960.0);
        let c = interface_point(m, Vec3::new(100.0, 0.0, -10.0), wide).unwrap();
        assert!(near(c, (960.0, 480.0)), "{c:?}");
    }

    #[test]
    fn behind_too_near_and_off_screen_are_hidden() {
        let m = camera(Vec3::ZERO);
        assert_eq!(interface_point(m, Vec3::new(0.0, 0.0, 10.0), SCREEN), Err(Hidden::Behind));
        assert_eq!(interface_point(m, Vec3::new(0.0, 0.0, 0.0), SCREEN), Err(Hidden::Behind), "on the camera");
        assert_eq!(interface_point(m, Vec3::new(0.0, 0.0, -0.05), SCREEN), Err(Hidden::OutOfDepth), "nearer than the near plane");
        // The view is 35.6 wide and 20 high at 10 ahead.
        assert!(interface_point(m, Vec3::new(17.0, 0.0, -10.0), SCREEN).is_ok(), "just inside the side");
        assert_eq!(interface_point(m, Vec3::new(18.5, 0.0, -10.0), SCREEN), Err(Hidden::OffScreen));
        assert_eq!(interface_point(m, Vec3::new(0.0, -10.5, -10.0), SCREEN), Err(Hidden::OffScreen));
        // The infinite projection has no far plane: far ahead is still shown.
        assert!(interface_point(m, Vec3::new(0.0, 0.0, -1.0e6), SCREEN).is_ok());
    }

    #[test]
    fn no_screen_or_a_broken_matrix_hides() {
        let m = camera(Vec3::ZERO);
        assert_eq!(interface_point(m, Vec3::new(0.0, 0.0, -10.0), (0.0, 0.0)), Err(Hidden::NoView));
        assert_eq!(interface_point(Mat4::NAN, Vec3::new(0.0, 0.0, -10.0), SCREEN), Err(Hidden::Behind));
    }
}
