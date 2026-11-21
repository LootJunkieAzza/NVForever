//! AttachUIComponent's and AttachUIXML's rects over their model nodes (issue #1835;
//! docs/spec-notes/menu-xml.md §9, "Attached components").
//!
//! [documented: AttachUIComponent] The component goes into one parent rect under
//! `HUDMainMenu` and follows a named node of a loaded model, the name case-sensitive.
//! `fnv_game::ui_more` keeps each attachment (reference, node, rect) and
//! [`fnv_game::ui_more::place_attachments`] writes the rect's place; this finds the node and
//! its point on screen each frame.
//!
//! - The node, by its name exactly as the model spells it: an actor's skeleton node
//!   ([`ActorRig`]'s `skel.by_name`), or a node of a reference's own animated model
//!   ([`RefNodes`]: doors, containers, activators and scripted models that world.rs spawns
//!   with their node hierarchy, `nvf_play::script_draw`'s enabled references). Each
//!   reference's rigs and node maps are indexed by reference ([`NodeOwners`]) as they are
//!   inserted and dropped (despawned, removed, or replaced when the model is spawned again),
//!   so a lookup is two map reads and never walks the world. A static's baked shapes keep
//!   no nodes, nor do an actor's worn and held models: a name there finds nothing [design].
//! - The camera: the player's eyes ([`MainCamera`], the `PlayerCamera` marker on main
//!   today), its projection after this frame's camera update. The view is taken to fill
//!   the window [design].
//! - Units: the HUD's. A window pixel is `window height / 960 × UI scale` of the HUD's
//!   interface units (`nvf_play::tiles::draw`, `hud`, drawn at `scale × interface::ui_scale()`;
//!   the UI scale is NVForever's setting, #1631), so the window's interface units
//!   (`nvf_play::tiles::screen_size`, 960 high) are divided by the UI scale.
//!
//! What the page leaves open, decided here [design; the page says nothing of them]:
//! - Behind the camera, nearer than its near plane, or farther than its far distance
//!   (`Projection::far`; Bevy's perspective matrix has no far plane, the distance only
//!   bounds its culling, so it is checked here on the camera-space depth): hidden.
//! - Off screen: hidden as soon as the node's point leaves the view. A rect whose node is
//!   on screen draws whole, cut only by the window's edge (no clipping of its own).
//! - No occlusion test: a rect shows through walls and other models.
//! - No scaling with distance: the rect keeps its authored size.
//! - The reference with no drawn model that names nodes, its model hidden
//!   (`InheritedVisibility`: a disabled reference; not frustum culling, which the depth and
//!   side rules stand for), the node not in it, no active camera: hidden. A reference out
//!   of the loaded cells loses its attachment altogether
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
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;
use nvf_play::{PlayerCamera, RefNodes};
use std::collections::HashMap;

/// The camera the attached rects are projected through: the player's main view. One alias,
/// so a later marker for the main view camera (#1753) is switched in here alone [design].
pub type MainCamera = With<PlayerCamera>;

/// Why an attached rect is hidden this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hidden {
    /// No active main camera, or a window of no size.
    NoView,
    /// The reference has no drawn model that names nodes, or its model is hidden.
    NotDrawn,
    /// No node of that name in the reference's drawn model.
    NoNode,
    /// The node's point is behind the camera.
    Behind,
    /// In front, but nearer than the near plane or farther than the far distance.
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

/// The main camera as the projection reads it.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub clip_from_view: Mat4,
    pub view_from_world: Mat4,
    /// The farthest camera-space depth shown (`Projection::far`).
    pub far: f32,
}

/// A world point (Bevy space) seen by `view` to the HUD's interface units from the view's
/// top left: `screen` is the window in interface units (`nvf_play::tiles::screen_size`,
/// 960 high), `ui_scale` the HUD's UI scale. The rules above: behind the camera, outside its
/// depth or outside the view's sides is hidden [design]. The view's edges count as on screen.
pub fn interface_point(view: &View, point: Vec3, screen: (f64, f64), ui_scale: f64) -> Result<(f64, f64), Hidden> {
    let (width, height) = screen;
    if !(width > 0.0 && height > 0.0 && ui_scale > 0.0 && ui_scale.is_finite()) {
        return Err(Hidden::NoView);
    }
    let seen = view.view_from_world.transform_point3(point);
    // Looking down -Z: the depth ahead.
    let depth = -seen.z;
    if !depth.is_finite() || depth <= 0.0 {
        return Err(Hidden::Behind);
    }
    if depth > view.far {
        return Err(Hidden::OutOfDepth);
    }
    let clip = view.clip_from_view * seen.extend(1.0);
    if !clip.is_finite() || clip.w <= f32::EPSILON {
        return Err(Hidden::Behind);
    }
    let ndc = clip.truncate() / clip.w;
    // Bevy's depth runs 1 at the near plane down to 0 (at infinity, or a finite far plane).
    if !(0.0..=1.0).contains(&ndc.z) {
        return Err(Hidden::OutOfDepth);
    }
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
        return Err(Hidden::OffScreen);
    }
    let x = (f64::from(ndc.x) + 1.0) * 0.5 * width;
    let y = (1.0 - f64::from(ndc.y)) * 0.5 * height;
    Ok((x / ui_scale, y / ui_scale))
}

/// Each reference's model roots that name nodes: actors' rigs ([`ActorRig`]) and animated
/// models' node maps ([`RefNodes`]), kept by the observers [`owner_added`] and
/// [`owner_dropped`] as the components are inserted and dropped.
#[derive(Resource, Default, Debug)]
pub struct NodeOwners(HashMap<u32, Vec<Entity>>);

impl NodeOwners {
    fn add(&mut self, reference: u32, root: Entity) {
        let roots = self.0.entry(reference).or_default();
        if !roots.contains(&root) {
            roots.push(root);
        }
    }

    fn drop_root(&mut self, reference: u32, root: Entity) {
        if let Some(roots) = self.0.get_mut(&reference) {
            roots.retain(|&e| e != root);
            if roots.is_empty() {
                self.0.remove(&reference);
            }
        }
    }

    /// The reference's roots, in the order they came.
    pub fn of(&self, reference: u32) -> &[Entity] {
        self.0.get(&reference).map_or(&[], Vec::as_slice)
    }
}

/// An actor's rig or a model's node map inserted: indexed under its reference.
fn owner_added(ev: On<Insert<(ActorRig, RefNodes)>>, rigs: Query<&ActorRig>, models: Query<&RefNodes>, mut owners: ResMut<NodeOwners>) {
    let e = ev.event().entity;
    if let Ok(rig) = rigs.get(e) {
        owners.add(rig.reference, e);
    }
    if let Ok(m) = models.get(e) {
        owners.add(m.reference, e);
    }
}

/// One dropped (removed, replaced or despawned; the component is still there to read): out
/// of the index. A replacement is indexed again by [`owner_added`] right after.
fn owner_dropped(ev: On<Discard<(ActorRig, RefNodes)>>, rigs: Query<&ActorRig>, models: Query<&RefNodes>, mut owners: ResMut<NodeOwners>) {
    let e = ev.event().entity;
    if let Ok(rig) = rigs.get(e) {
        owners.drop_root(rig.reference, e);
    }
    if let Ok(m) = models.get(e) {
        owners.drop_root(m.reference, e);
    }
}

/// What a node lookup reads.
#[derive(SystemParam)]
pub struct Nodes<'w, 's> {
    owners: Res<'w, NodeOwners>,
    rigs: Query<'w, 's, &'static ActorRig>,
    models: Query<'w, 's, &'static RefNodes>,
    shown: Query<'w, 's, &'static InheritedVisibility>,
    globals: Query<'w, 's, &'static GlobalTransform>,
}

impl Nodes<'_, '_> {
    /// The named node of `reference`'s drawn model in world space (the module's rules).
    pub fn point(&self, reference: u32, node: &str) -> Result<Vec3, Hidden> {
        let mut drawn = false;
        for &root in self.owners.of(reference) {
            if self.shown.get(root).is_ok_and(|v| !v.get()) {
                continue;
            }
            drawn = true;
            let rig = self.rigs.get(root).ok().and_then(|rig| rig.skel.by_name.get(node).and_then(|&i| rig.nodes.get(i)).copied());
            let found = rig.or_else(|| self.models.get(root).ok().and_then(|m| m.nodes.get(node).copied()));
            if let Some(Ok(t)) = found.map(|e| self.globals.get(e)) {
                return Ok(t.translation());
            }
        }
        Err(if drawn { Hidden::NoNode } else { Hidden::NotDrawn })
    }
}

/// Places every attached rect (nothing to do, and nothing read, without one).
fn place(script: Option<ResMut<ScriptState>>, cameras: Query<(&Camera, &Projection, &GlobalTransform), MainCamera>, windows: Query<&Window, With<PrimaryWindow>>, nodes: Nodes) {
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.ui_extra.attached.is_empty() {
        return;
    }
    let view = cameras.iter().find(|(c, ..)| c.is_active).map(|(c, p, t)| View { clip_from_view: c.clip_from_view(), view_from_world: t.to_matrix().inverse(), far: p.far() });
    let screen = windows.single().ok().map(|w| nvf_play::tiles::screen_size(w.width(), w.height()));
    let ui_scale = f64::from(nvf_play::interface::ui_scale());
    let mut why: Vec<(FormId, String, Hidden)> = Vec::new();
    let changed = fnv_game::ui_more::place_attachments(&mut rt.game, |reference, node| {
        let at = match (view, screen) {
            (Some(v), Some(s)) => nodes.point(reference.0, node).and_then(|p| interface_point(&v, p, s, ui_scale)),
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
        app.init_resource::<NodeOwners>().add_observer(owner_added).add_observer(owner_dropped).add_systems(
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
    use bevy::ecs::system::SystemState;
    use nvf_assets::actor::PreparedSkeleton;
    use std::sync::Arc;

    const WINDOW: (f32, f32) = (1280.0, 720.0);

    /// The window in interface units: 960 high, 960 x 16/9 wide.
    fn screen() -> (f64, f64) {
        nvf_play::tiles::screen_size(WINDOW.0, WINDOW.1)
    }

    /// A camera at `at` looking down -Z, 90 degrees high, 16:9, near plane 0.1, through
    /// Bevy's own perspective (infinite, reversed depth), as the player's camera has it.
    fn camera(at: Vec3, far: f32) -> View {
        let proj = Projection::Perspective(PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: WINDOW.0 / WINDOW.1, near: 0.1, far, ..default() });
        View { clip_from_view: proj.get_clip_from_view(), view_from_world: Mat4::from_translation(at).inverse(), far: proj.far() }
    }

    /// Where the HUD drawing puts interface units `at` in the window, in logical pixels: a
    /// unit is `height / 960 × ui_scale` pixels from the window's top left
    /// (`nvf_play::tiles::draw`, `hud`).
    fn rendered(at: (f64, f64), ui_scale: f64) -> (f64, f64) {
        let px = f64::from(WINDOW.1) / 960.0 * ui_scale;
        (at.0 * px, at.1 * px)
    }

    /// Where the 3D picture shows a point in front of `view`, in logical pixels.
    fn pictured(view: &View, point: Vec3) -> (f64, f64) {
        let clip = view.clip_from_view * view.view_from_world * point.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        ((f64::from(ndc.x) + 1.0) * 0.5 * f64::from(WINDOW.0), (1.0 - f64::from(ndc.y)) * 0.5 * f64::from(WINDOW.1))
    }

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn a_point_ahead_lands_at_its_place_in_interface_units() {
        let v = camera(Vec3::ZERO, 1000.0);
        let centre = interface_point(&v, Vec3::new(0.0, 0.0, -10.0), screen(), 1.0).unwrap();
        assert!(near(centre, (screen().0 / 2.0, 480.0)), "{centre:?}");
        // Half way up the view (tan 45 = 1: the view is 20 high at 10 ahead).
        let up = interface_point(&v, Vec3::new(0.0, 5.0, -10.0), screen(), 1.0).unwrap();
        assert!(near(up, (screen().0 / 2.0, 240.0)), "{up:?}");
        // Right is right, down is down; farther away the same direction is the same place.
        let p = interface_point(&v, Vec3::new(8.0, -2.5, -10.0), screen(), 1.0).unwrap();
        let q = interface_point(&v, Vec3::new(80.0, -25.0, -100.0), screen(), 1.0).unwrap();
        assert!(p.0 > screen().0 / 2.0 && p.1 > 480.0);
        assert!(near(p, q), "{p:?} {q:?}");
    }

    #[test]
    fn the_hud_draws_the_point_where_the_picture_shows_it_at_any_ui_scale() {
        let v = camera(Vec3::new(30.0, -4.0, 12.0), 1000.0);
        let points = [Vec3::new(30.0, -4.0, 2.0), Vec3::new(36.0, 0.5, -8.0), Vec3::new(14.0, -12.0, -20.0)];
        for ui_scale in [0.5, 0.75, 1.0, 1.25, 1.5] {
            for p in points {
                let at = interface_point(&v, p, screen(), ui_scale).unwrap();
                let (drawn, seen) = (rendered(at, ui_scale), pictured(&v, p));
                assert!((drawn.0 - seen.0).abs() < 1e-2 && (drawn.1 - seen.1).abs() < 1e-2, "scale {ui_scale}, {p}: drawn {drawn:?}, pictured {seen:?}");
            }
        }
        // At 50 % the HUD's view is 1920 units high, so the middle of the window is (1706.7, 960).
        let middle = interface_point(&v, Vec3::new(30.0, -4.0, 2.0), screen(), 0.5).unwrap();
        assert!(near(middle, (screen().0, 960.0)), "{middle:?}");
    }

    #[test]
    fn the_camera_moving_moves_the_point() {
        let v = camera(Vec3::new(100.0, 0.0, 0.0), 1000.0);
        let centre = interface_point(&v, Vec3::new(100.0, 0.0, -10.0), screen(), 1.0).unwrap();
        assert!(near(centre, (screen().0 / 2.0, 480.0)), "{centre:?}");
    }

    #[test]
    fn behind_too_near_too_far_and_off_screen_are_hidden() {
        let v = camera(Vec3::ZERO, 1000.0);
        assert_eq!(interface_point(&v, Vec3::new(0.0, 0.0, 10.0), screen(), 1.0), Err(Hidden::Behind));
        assert_eq!(interface_point(&v, Vec3::new(0.0, 0.0, 0.0), screen(), 1.0), Err(Hidden::Behind), "on the camera");
        assert_eq!(interface_point(&v, Vec3::new(0.0, 0.0, -0.05), screen(), 1.0), Err(Hidden::OutOfDepth), "nearer than the near plane");
        // The far distance holds though the matrix has no far plane.
        assert!(interface_point(&v, Vec3::new(0.0, 0.0, -999.0), screen(), 1.0).is_ok());
        assert_eq!(interface_point(&v, Vec3::new(0.0, 0.0, -1001.0), screen(), 1.0), Err(Hidden::OutOfDepth), "past the far distance");
        assert_eq!(interface_point(&camera(Vec3::ZERO, 5000.0), Vec3::new(0.0, 0.0, -1001.0), screen(), 1.0).map(|_| ()), Ok(()), "within a farther one");
        // The view is 35.6 wide and 20 high at 10 ahead.
        assert!(interface_point(&v, Vec3::new(17.0, 0.0, -10.0), screen(), 1.0).is_ok(), "just inside the side");
        assert_eq!(interface_point(&v, Vec3::new(18.5, 0.0, -10.0), screen(), 1.0), Err(Hidden::OffScreen));
        assert_eq!(interface_point(&v, Vec3::new(0.0, -10.5, -10.0), screen(), 1.0), Err(Hidden::OffScreen));
    }

    #[test]
    fn no_screen_no_scale_or_a_broken_matrix_hides() {
        let v = camera(Vec3::ZERO, 1000.0);
        let ahead = Vec3::new(0.0, 0.0, -10.0);
        assert_eq!(interface_point(&v, ahead, (0.0, 0.0), 1.0), Err(Hidden::NoView));
        assert_eq!(interface_point(&v, ahead, screen(), 0.0), Err(Hidden::NoView));
        let broken = View { view_from_world: Mat4::NAN, ..v };
        assert_eq!(interface_point(&broken, ahead, screen(), 1.0), Err(Hidden::Behind));
    }

    /// A world with the index's observers.
    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<NodeOwners>();
        w.add_observer(owner_added);
        w.add_observer(owner_dropped);
        w
    }

    fn at(w: &mut World, p: Vec3) -> Entity {
        w.spawn(GlobalTransform::from_translation(p)).id()
    }

    fn lookup(w: &mut World, reference: u32, node: &str) -> Result<Vec3, Hidden> {
        let mut state = SystemState::<Nodes>::new(w);
        state.get(w).expect("the lookup's resources").point(reference, node)
    }

    #[test]
    fn animated_models_name_their_nodes_by_their_exact_names() {
        let mut w = world();
        let lid = at(&mut w, Vec3::new(1.0, 2.0, 3.0));
        let root = w.spawn((RefNodes { reference: 7, nodes: HashMap::from([("Lid".to_string(), lid)]) }, InheritedVisibility::VISIBLE)).id();
        assert_eq!(lookup(&mut w, 7, "Lid"), Ok(Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(lookup(&mut w, 7, "lid"), Err(Hidden::NoNode), "case-sensitive");
        assert_eq!(lookup(&mut w, 8, "Lid"), Err(Hidden::NotDrawn), "another reference has no model");
        // Its model hidden (a disabled reference).
        w.entity_mut(root).insert(InheritedVisibility::HIDDEN);
        assert_eq!(lookup(&mut w, 7, "Lid"), Err(Hidden::NotDrawn));
        w.entity_mut(root).insert(InheritedVisibility::VISIBLE);
        // The model spawned again (a new root, the old one gone): the new nodes.
        let lid2 = at(&mut w, Vec3::new(4.0, 5.0, 6.0));
        w.spawn(RefNodes { reference: 7, nodes: HashMap::from([("Lid".to_string(), lid2)]) });
        w.despawn(root);
        assert_eq!(lookup(&mut w, 7, "Lid"), Ok(Vec3::new(4.0, 5.0, 6.0)));
        assert_eq!(w.resource::<NodeOwners>().of(7).len(), 1, "the despawned root left the index");
    }

    #[test]
    fn actors_name_their_skeleton_nodes_and_leave_the_index_when_despawned() {
        let mut w = world();
        let pelvis = at(&mut w, Vec3::ZERO);
        let head = at(&mut w, Vec3::new(0.0, 0.0, 120.0));
        let skel = PreparedSkeleton { by_name: HashMap::from([("Bip01".to_string(), 0), ("Bip01 Head".to_string(), 1)]), ..Default::default() };
        let rig = ActorRig { reference: 0x1234, base: 1, body: 1, skeleton: String::new(), skel: Arc::new(skel), nodes: vec![pelvis, head], base_scale: 1.0 };
        let actor = w.spawn(rig.clone()).id();
        assert_eq!(lookup(&mut w, 0x1234, "Bip01 Head"), Ok(Vec3::new(0.0, 0.0, 120.0)));
        assert_eq!(lookup(&mut w, 0x1234, "Bip01 head"), Err(Hidden::NoNode));
        // The rig put on again (the same entity): still once in the index.
        w.entity_mut(actor).insert(rig);
        assert_eq!(w.resource::<NodeOwners>().of(0x1234), &[actor]);
        w.despawn(actor);
        assert!(w.resource::<NodeOwners>().of(0x1234).is_empty());
        assert_eq!(lookup(&mut w, 0x1234, "Bip01 Head"), Err(Hidden::NotDrawn));
    }
}
