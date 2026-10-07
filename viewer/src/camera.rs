//! Orbit camera: left drag rotates, right drag pans, wheel zooms, F frames the mesh.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

#[derive(Component)]
pub struct Orbit {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Orbit {
    pub fn frame(&mut self, lo: Vec3, hi: Vec3) {
        self.target = (lo + hi) * 0.5;
        let radius = ((hi - lo).length() * 0.5).max(1.0);
        self.distance = radius * 3.0;
    }
}

pub fn orbit_input(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cams: Query<(&mut Orbit, &mut Transform)>,
) {
    for (mut o, mut t) in &mut cams {
        let d = motion.delta;
        if buttons.pressed(MouseButton::Left) {
            o.yaw -= d.x * 0.005;
            o.pitch = (o.pitch - d.y * 0.005).clamp(-1.55, 1.55);
        }
        if buttons.pressed(MouseButton::Right) {
            let right = t.right();
            let up = t.up();
            let k = o.distance * 0.0015;
            o.target += (-right * d.x + up * d.y) * k;
        }
        if scroll.delta.y != 0.0 {
            o.distance = (o.distance * (1.0 - scroll.delta.y * 0.1)).max(0.5);
        }
        let rot = Quat::from_euler(EulerRot::YXZ, o.yaw, o.pitch, 0.0);
        t.translation = o.target + rot * Vec3::new(0.0, 0.0, o.distance);
        t.look_at(o.target, Vec3::Y);
    }
}
