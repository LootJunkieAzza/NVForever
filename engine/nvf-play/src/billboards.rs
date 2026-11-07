//! `NiBillboardNode`s of the models drawn in the world (placed references, render
//! streaming in `nvforever::world`, and created ones, [`crate::dropped`]): each turns to
//! the camera every frame, after the clips have posed the nodes.
//!
//! - **Rotate about up** (modes 1, 5, 9 [documented: nif.xml BillboardMode names]): the
//!   node turns about its own +z so that its -y faces the camera [observed:
//!   `effects\fxfiremeshsmall.nif` (the fire Burning Campfire Redone places), node
//!   `FireBall09`, mode 5: its two flame cards stand along x and z, 0.1-5 units deep in
//!   y, with normals averaging (0, -0.93, ±0.1); hypothesis: that the side the normals
//!   face is the one the original shows the camera, and +z the "up" it turns about].
//! - **Other modes**: its -y points at the camera [hypothesis, as above].
//!
//! Without the turn the flame cards stand still, and their shader's view-angle falloff
//! (`BSShaderNoLightingProperty`, falloff from 35° to 85° off their normal) fades them
//! out seen from the side: a campfire's fire was nearly gone from most places
//! (owner's playtest 2026-11-07).
//!
//! The effect models of shots (`nvforever::shot_fx`) turn their own billboards.

use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

/// A billboard node of a model drawn in the world: its mode and its rotation in the file.
#[derive(Component, Debug, Clone, Copy)]
pub struct Billboard {
    pub mode: u16,
    pub bind: Quat,
}

/// The turn (in the node's own frame as the file has it) that shows the node's -y to a
/// camera in direction `d`.
pub fn turn(mode: u16, d: Vec3) -> Quat {
    match mode {
        1 | 5 | 9 => Quat::from_rotation_z(d.x.atan2(-d.y)),
        _ => Quat::from_rotation_arc(Vec3::NEG_Y, d.normalize_or(Vec3::NEG_Y)),
    }
}

fn face_camera(
    camera: Query<Entity, (With<Camera3d>, With<crate::PlayerCamera>)>,
    boards: Query<(Entity, &Billboard, &ChildOf)>,
    parents: Query<&ChildOf>,
    mut transforms: Query<&mut Transform>,
) {
    if boards.is_empty() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    // An entity's place from this frame's transforms (the propagated ones are a frame
    // old, and missing for what was spawned this frame).
    let world = |transforms: &Query<&mut Transform>, e: Entity| {
        let mut m = Affine3A::IDENTITY;
        let mut at = Some(e);
        while let Some(p) = at {
            if let Ok(t) = transforms.get(p) {
                m = t.compute_affine() * m;
            }
            at = parents.get(p).ok().map(ChildOf::parent);
        }
        m
    };
    let eye = Vec3::from(world(&transforms, cam).translation);
    for (e, b, parent) in &boards {
        let m = world(&transforms, parent.parent());
        let Ok(mut t) = transforms.get_mut(e) else { continue };
        let d = b.bind.inverse() * (m.inverse().transform_point3(eye) - t.translation);
        let rotation = b.bind * turn(b.mode, d);
        if t.rotation != rotation {
            t.rotation = rotation;
        }
    }
}

pub struct BillboardPlugin;

impl Plugin for BillboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, face_camera.before(TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_up_shows_minus_y_to_the_camera() {
        for d in [Vec3::NEG_Y, Vec3::X, Vec3::new(-3.0, 4.0, 2.0), Vec3::Y] {
            let shown = turn(5, d) * Vec3::NEG_Y;
            let flat = Vec3::new(d.x, d.y, 0.0).normalize();
            assert!(shown.abs_diff_eq(flat, 1e-5), "{d}: {shown}");
            assert!((turn(5, d) * Vec3::Z).abs_diff_eq(Vec3::Z, 1e-5), "about +z only");
        }
        let d = Vec3::new(1.0, 2.0, 3.0);
        assert!((turn(0, d) * Vec3::NEG_Y).abs_diff_eq(d.normalize(), 1e-5));
    }
}
