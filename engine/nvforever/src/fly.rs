//! Free-fly camera: W/A/S/D move, Q/E down/up, Shift faster, right mouse button drag to
//! look, mouse wheel changes speed.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

/// Test aid: move the camera forward (horizontally) at this many units per second.
#[derive(Resource, Default)]
pub struct AutoFly(pub f32);

/// The main world camera, the player's view (docs/design/layer4-play.md §2, "The main
/// view"): put once on the camera `setup` spawns (main.rs) and never moved, with or without
/// `--play` (`nvf_play::PlayerCamera` joins it on the same entity with `--play`). Systems
/// that mean "where the player looks from" (streaming, level of detail, grass, water,
/// particles, actors' heads and feet) find the camera by [`WorldCamera`], not as the only
/// `Camera3d`: menus and scopes spawn 3D cameras of their own [design: issue #1753].
#[derive(Component)]
pub struct FlyCam {
    pub yaw: f32,
    pub pitch: f32,
    /// World units per second.
    pub speed: f32,
}

/// Query filter for the main world camera (the player's view): the [`FlyCam`] camera, never
/// the water reflection's, a scope lens's or a menu view's `Camera3d` beside it. The
/// `Without<ReflectionCamera>` keeps queries that also write the reflection camera
/// disjoint from it (`reflection::update_reflection`) [design: issue #1753].
pub type WorldCamera = (With<Camera3d>, With<FlyCam>, Without<crate::reflection::ReflectionCamera>);

pub fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    auto: Res<AutoFly>,
    mut cams: Query<(&mut Transform, &mut FlyCam)>,
) {
    let Ok((mut t, mut c)) = cams.single_mut() else { return };
    if buttons.pressed(MouseButton::Right) {
        c.yaw -= motion.delta.x * 0.003;
        c.pitch = (c.pitch - motion.delta.y * 0.003).clamp(-1.55, 1.55);
    }
    if scroll.delta.y != 0.0 {
        c.speed = (c.speed * 1.2f32.powf(scroll.delta.y)).clamp(50.0, 50_000.0);
    }
    t.rotation = Quat::from_euler(EulerRot::YXZ, c.yaw, c.pitch, 0.0);
    let mut dir = Vec3::ZERO;
    let (fwd, right) = (t.forward().as_vec3(), t.right().as_vec3());
    if keys.pressed(KeyCode::KeyW) {
        dir += fwd;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= fwd;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= right;
    }
    if keys.pressed(KeyCode::KeyE) {
        dir += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) {
        dir -= Vec3::Y;
    }
    let boost = if keys.pressed(KeyCode::ShiftLeft) { 5.0 } else { 1.0 };
    t.translation += dir.normalize_or_zero() * c.speed * boost * time.delta_secs();
    if auto.0 != 0.0 {
        let f = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
        t.translation += f * auto.0 * time.delta_secs();
    }
}
