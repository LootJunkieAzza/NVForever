//! The collision view (docs/design/console.md, docs/spec-notes/physics.md §9): the
//! physics world's collision shapes drawn as lines in the world, coloured by what they
//! belong to, within a radius of the camera. For developers and testers, to see what
//! physics sees.
//!
//! - **Switch**: the console's `tcg` (ToggleCollisionGeometry), the original's command
//!   for "a color-coded view of the collision geometry" [documented: Fallout Wiki,
//!   "Fallout: New Vegas console commands", `tcg`; GECK wiki ToggleCollisionGeometry, seen
//!   in search summaries]; `tcg <radius>` switches it on with that radius. The original
//!   colours by the layer a modeller gave the shape; ours colour by what the shape is in
//!   our physics [design]. At start: `NVF_COLLISION_VIEW` (`1` the default radius, a
//!   number above 1 that radius in game units).
//! - **What**: every collider whose outline passes within the radius
//!   (`fnv_phys::debug_lines`: terrain, static, keyframed, dynamic awake and asleep,
//!   characters, hit volumes, ragdolls, gore, trigger-layer volumes), and the script
//!   trigger volumes (`crate::script_trigger`, which physics does not hold), drawn in the
//!   trigger colour. Poses are the last physics step's. A character capsule holding the
//!   camera (the player's own, in first person) is left out [design].
//! - **Cost**: off, one run condition a frame and a disabled gizmo group. On, the outlines
//!   are drawn as gizmo lines, at most [`MAX_SEGMENTS`]. Terrain and static outlines are a
//!   retained gizmo, gathered and rebuilt only when the camera moved ([`REGATHER_MOVE`]), a
//!   collider came or went, or after [`STILL_REFRESH`]; the rest are gathered after each
//!   physics step and drawn each frame.
//!
//! Log: `collision view: on|off …` when it switches, then `collision view: <n> segments
//! within <r> units …` with the count of each kind, at the first draw and every
//! [`LOG_EVERY`] seconds while on, with the mean time a gather took.

use crate::{space, PlayerCamera};
use bevy::prelude::*;
use fnv_phys::debug_lines::{Category, CollisionLines};

/// The environment variable that switches the view on at start.
pub const ENV: &str = "NVF_COLLISION_VIEW";
/// Radius around the camera, game units, when none is given [design: a town block].
pub const DEFAULT_RADIUS: f32 = 3000.0;
/// Most segments drawn a frame [design: keeps a dense interior from swamping the frame].
pub const MAX_SEGMENTS: usize = 200_000;
/// How far the camera moves before the outlines are gathered again, game units [design].
pub const REGATHER_MOVE: f32 = 32.0;
/// Seconds between the counts in the log while on.
pub const LOG_EVERY: f32 = 10.0;

/// The colour of each category, in `Category::ALL`'s order [design].
pub const COLOURS: [[f32; 3]; Category::COUNT] = [
    [0.65, 0.5, 0.25], // terrain: brown
    [0.75, 0.75, 0.75], // static: light grey
    [0.2, 0.85, 0.95], // keyframed: cyan
    [0.25, 1.0, 0.25], // dynamic awake: bright green
    [0.1, 0.45, 0.2],  // dynamic asleep: dark green
    [1.0, 1.0, 0.25],  // characters: yellow
    [1.0, 0.55, 0.1],  // hit volumes: orange
    [1.0, 0.3, 1.0],   // ragdolls: magenta
    [0.9, 0.1, 0.1],   // gore: red
    [0.45, 0.5, 1.0],  // triggers: blue
];

/// Seconds after which the still outlines (terrain, static) are gathered again even
/// though nothing said they changed [design: a reference replaced by another with as many
/// colliders keeps the old outline at most this long].
pub const STILL_REFRESH: f32 = 2.0;

/// One set of outlines and when it was gathered.
#[derive(Default)]
struct Part {
    lines: CollisionLines,
    /// Camera position, radius, physics step, collider count and real time of the gather.
    at: Option<([f32; 3], f32, u64, usize, f32)>,
    /// Gathers and their time since the last log line.
    gathers: u32,
    secs: f64,
}

impl Part {
    fn gather(&mut self, world: &fnv_phys::PhysWorld, key: ([f32; 3], f32, u64, usize, f32), max: usize, kinds: &[Category]) {
        let t = std::time::Instant::now();
        // The player's own capsule round a first-person camera would fence the view in.
        self.lines.hide_character_at_center = true;
        self.lines.gather_only(world, key.0, key.1, max, kinds);
        self.secs += t.elapsed().as_secs_f64();
        self.gathers += 1;
        self.at = Some(key);
    }

    /// Mean gather time since the last log line, milliseconds; and resets the count.
    fn mean_ms(&mut self) -> (u32, f64) {
        let r = (self.gathers, if self.gathers > 0 { self.secs * 1000.0 / self.gathers as f64 } else { 0.0 });
        self.gathers = 0;
        self.secs = 0.0;
        r
    }
}

/// Whether the view is on, its radius, and what it gathered.
#[derive(Resource)]
pub struct CollisionView {
    pub on: bool,
    pub radius: f32,
    /// The outlines that never move (terrain, static) and the rest; kept only while on.
    parts: Option<Box<[Part; 2]>>,
    /// Real time of the last log line; `None` until the first draw after switching on.
    logged: Option<f32>,
    /// The still outlines as a retained gizmo (drawn without being sent again each frame),
    /// while on.
    still_gizmo: Option<(Entity, Handle<GizmoAsset>)>,
}

impl Default for CollisionView {
    fn default() -> Self {
        CollisionView { on: false, radius: DEFAULT_RADIUS, parts: None, logged: None, still_gizmo: None }
    }
}

impl CollisionView {
    /// Switch it on (with `radius`, else the one it had) or off.
    pub fn set(&mut self, on: bool, radius: Option<f32>) {
        if let Some(r) = radius.filter(|r| r.is_finite() && *r > 0.0) {
            self.radius = r;
        }
        self.on = on;
        self.logged = None;
        self.parts = None;
    }

    /// Its state in words, for the console and the log.
    pub fn state(&self) -> String {
        if self.on {
            format!("Collision geometry on (within {} units of the camera)", self.radius)
        } else {
            "Collision geometry off".to_string()
        }
    }
}

/// The radius `NVF_COLLISION_VIEW` asks for: `1` (or `on`) the default, a number above 1
/// that radius; anything else (`0`, unset) none.
pub fn env_radius(v: Option<&str>) -> Option<f32> {
    let v = v?.trim();
    if v.eq_ignore_ascii_case("on") {
        return Some(DEFAULT_RADIUS);
    }
    match v.parse::<f32>() {
        Ok(r) if r.is_finite() && r > 1.0 => Some(r),
        Ok(1.0) => Some(DEFAULT_RADIUS),
        _ => None,
    }
}

/// The lines of the collision view, in front of their own surfaces by a hair so terrain
/// lines do not flicker in the ground they outline.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CollisionGizmos;

pub struct CollisionViewPlugin;

impl Plugin for CollisionViewPlugin {
    fn build(&self, app: &mut App) {
        let mut view = CollisionView::default();
        if let Some(r) = env_radius(std::env::var(ENV).ok().as_deref()) {
            view.set(true, Some(r));
            info!("collision view: on ({ENV}, radius {r} units)");
        }
        app.insert_resource(view)
            .init_gizmo_group::<CollisionGizmos>()
            .add_systems(Startup, configure)
            .add_systems(PostUpdate, (switched.run_if(resource_changed::<CollisionView>), draw.run_if(view_on)).chain().after(bevy::transform::TransformSystems::Propagate));
    }
}

fn view_on(v: Res<CollisionView>) -> bool {
    v.on
}

/// How much nearer the camera the lines are drawn than the surfaces they outline (Bevy's
/// gizmo depth bias, -1 to 1) [design: enough to keep terrain lines out of the ground].
const DEPTH_BIAS: f32 = -0.0005;
/// Line width, pixels [design].
const LINE_WIDTH: f32 = 1.5;

fn configure(mut store: ResMut<GizmoConfigStore>, view: Res<CollisionView>) {
    let (config, _) = store.config_mut::<CollisionGizmos>();
    config.depth_bias = DEPTH_BIAS;
    config.line.width = LINE_WIDTH;
    config.enabled = view.on;
}

/// The gizmo group follows the switch (a disabled group costs nothing to draw), and the
/// retained still outlines go when it is off.
fn switched(mut store: ResMut<GizmoConfigStore>, mut view: ResMut<CollisionView>, mut commands: Commands, mut assets: ResMut<Assets<GizmoAsset>>) {
    let (config, _) = store.config_mut::<CollisionGizmos>();
    if config.enabled != view.on {
        config.enabled = view.on;
        info!("collision view: {}", if view.on { "on" } else { "off" });
    }
    if !view.on {
        if let Some((e, h)) = view.bypass_change_detection().still_gizmo.take() {
            commands.entity(e).despawn();
            assets.remove(&h);
        }
    }
}

fn colour(c: Category) -> Color {
    let [r, g, b] = COLOURS[c.index()];
    Color::srgb(r, g, b)
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// The corners of a box trigger volume in game space: `pos + R·(scale·local)`.
fn box_corners(t: &crate::script_events::TriggerShape, h: [f32; 3]) -> [[f32; 3]; 8] {
    let s = if t.scale > 0.0 { t.scale } else { 1.0 };
    let r = &t.rot;
    std::array::from_fn(|i| {
        let l = [if i & 1 == 0 { -h[0] } else { h[0] }, if i & 2 == 0 { -h[1] } else { h[1] }, if i & 4 == 0 { -h[2] } else { h[2] }].map(|v| v * s);
        std::array::from_fn(|k| t.pos[k] + r[k * 3] * l[0] + r[k * 3 + 1] * l[1] + r[k * 3 + 2] * l[2])
    })
}

/// The 12 edges of a box by corner index (bit 0 x, bit 1 y, bit 2 z).
const BOX_EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

#[allow(clippy::too_many_arguments)]
fn draw(
    mut view: ResMut<CollisionView>,
    phys: Res<crate::phys::PhysState>,
    script: Option<Res<crate::script::ScriptState>>,
    cameras: Query<&GlobalTransform, With<PlayerCamera>>,
    time: Res<Time<Real>>,
    mut gizmos: Gizmos<CollisionGizmos>,
    (mut commands, mut assets): (Commands, ResMut<Assets<GizmoAsset>>),
) {
    let Ok(cam) = cameras.single() else { return };
    let center = space::to_game(cam.translation());
    let view = &mut *view;
    let radius = view.radius;
    let world = &phys.world;
    let now = time.elapsed_secs();
    let key = (center, radius, world.steps(), world.collider_count(), now);
    let moved = |at: [f32; 3]| dist2(at, center) > REGATHER_MOVE * REGATHER_MOVE;
    let parts = view.parts.get_or_insert_with(Default::default);
    let [still, moving] = &mut **parts;
    // Terrain and static outlines: again when the camera moved, the radius changed, a
    // collider came or went, or after STILL_REFRESH.
    if still.at.is_none_or(|(at, r, _, n, t)| moved(at) || r != radius || n != key.3 || now - t >= STILL_REFRESH) {
        let kinds: Vec<Category> = Category::ALL.into_iter().filter(|c| c.is_still()).collect();
        still.gather(world, key, MAX_SEGMENTS / 2, &kinds);
        let mut asset = GizmoAsset::default();
        for s in &still.lines.segments {
            asset.line(space::to_bevy(s.a), space::to_bevy(s.b), colour(s.category));
        }
        match &view.still_gizmo {
            Some((_, h)) => {
                let _ = assets.insert(h, asset);
            }
            None => {
                let h = assets.add(asset);
                let line_config = GizmoLineConfig { width: LINE_WIDTH, ..default() };
                let e = commands.spawn(Gizmo { handle: h.clone(), line_config, depth_bias: DEPTH_BIAS }).id();
                view.still_gizmo = Some((e, h));
            }
        }
    }
    // The rest: again after each physics step, or when the camera moved.
    if moving.at.is_none_or(|(at, r, s, _, _)| moved(at) || r != radius || s != key.2) {
        let kinds: Vec<Category> = Category::ALL.into_iter().filter(|c| !c.is_still()).collect();
        moving.gather(world, key, MAX_SEGMENTS / 2, &kinds);
    }
    for s in &moving.lines.segments {
        gizmos.line(space::to_bevy(s.a), space::to_bevy(s.b), colour(s.category));
    }
    // The script trigger volumes, which physics does not hold.
    let mut volumes = 0;
    if let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) {
        let c = colour(Category::Trigger);
        for t in rt.triggers.volumes() {
            use crate::script_events::TriggerKind;
            let s = if t.scale > 0.0 { t.scale } else { 1.0 };
            let reach = match t.kind {
                TriggerKind::Box { half_extents: h } => (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt() * s,
                TriggerKind::Sphere { radius } => radius * s,
            };
            if dist2(t.pos, center) > (radius + reach) * (radius + reach) {
                continue;
            }
            volumes += 1;
            match t.kind {
                TriggerKind::Box { half_extents: h } => {
                    let p = box_corners(t, h);
                    for (a, b) in BOX_EDGES {
                        gizmos.line(space::to_bevy(p[a]), space::to_bevy(p[b]), c);
                    }
                }
                TriggerKind::Sphere { radius: r } => {
                    gizmos.sphere(Isometry3d::from_translation(space::to_bevy(t.pos)), r * s, c);
                }
            }
        }
    }
    if view.logged.is_none_or(|t| now - t >= LOG_EVERY) {
        let (a, b) = (still.lines.counts(), moving.lines.counts());
        let kinds: Vec<String> = Category::ALL.iter().map(|c| format!("{} {}", c.name(), a[c.index()] + b[c.index()])).collect();
        let ((sg, sms), (mg, mms)) = (still.mean_ms(), moving.mean_ms());
        info!(
            "collision view: {} segments within {} units of ({:.0}, {:.0}, {:.0}): {}; {} colliders, {} dropped, {} script trigger volumes; still {} gathers {:.2} ms each, moving {} gathers {:.2} ms each",
            still.lines.segments.len() + moving.lines.segments.len(),
            radius,
            center[0],
            center[1],
            center[2],
            kinds.join(", "),
            still.lines.colliders + moving.lines.colliders,
            still.lines.dropped + moving.lines.dropped,
            volumes,
            sg,
            sms,
            mg,
            mms
        );
        view.logged = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_events::{TriggerKind, TriggerShape};

    #[test]
    fn env_values() {
        assert_eq!(env_radius(None), None);
        assert_eq!(env_radius(Some("0")), None);
        assert_eq!(env_radius(Some("1")), Some(DEFAULT_RADIUS));
        assert_eq!(env_radius(Some("on")), Some(DEFAULT_RADIUS));
        assert_eq!(env_radius(Some(" 1500 ")), Some(1500.0));
        assert_eq!(env_radius(Some("far")), None);
        assert_eq!(env_radius(Some("-5")), None);
    }

    #[test]
    fn switching_keeps_the_radius_and_drops_the_lines() {
        let mut v = CollisionView::default();
        v.set(true, Some(800.0));
        assert!(v.on && v.radius == 800.0);
        v.parts = Some(Default::default());
        v.set(false, None);
        assert!(!v.on && v.parts.is_none() && v.radius == 800.0);
        assert_eq!(v.state(), "Collision geometry off");
        v.set(true, Some(f32::NAN));
        assert_eq!(v.radius, 800.0);
    }

    #[test]
    fn box_corners_turn_and_scale_with_the_reference() {
        // Turned 90° about Z: local +X is world +Y.
        let t = TriggerShape { pos: [100.0, 0.0, 0.0], rot: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], scale: 2.0, kind: TriggerKind::Box { half_extents: [10.0, 1.0, 1.0] } };
        let p = box_corners(&t, [10.0, 1.0, 1.0]);
        // Corner 1: local (+10, -1, -1) × 2 → world (100 + 2, 20, -2).
        assert_eq!(p[1], [102.0, 20.0, -2.0]);
        assert_eq!(p[6], [98.0, -20.0, 2.0]);
    }
}
