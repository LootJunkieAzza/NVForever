//! The models of VATS camera shots (CAMS `MODL`, `meshes\vatscameras\…`): one root node
//! moved by its own `NiTransformController` over an `NiCamera` child [observed:
//! `fnvtool nif decode` of `simplefronthitl03.nif` and `snipercamhit01.nif`: a root
//! `NiNode` named as the file, a controller of about 17 s whose Euler keys move it by a
//! few degrees (a hand-held sway) and a few translation keys, and an `NiCamera` with a
//! frustum of ±0.767 at a near plane of 1, 75° across]. Read for the kill cameras
//! (`nvf_play::kill_cam`, docs/design/vats-mode.md §19).

use crate::model::ModelCtx;
use bevy::math::{Affine3A, Mat3, Quat, Vec3};
use fnv_nif::anim::{transform_track, TransformTrack};
use fnv_nif::scene::{scene_objects, Decoded, Transform as NifTransform};
use fnv_nif::NifFile;
use std::sync::Arc;

/// A camera shot's model.
#[derive(Debug, Clone)]
pub struct CameraModel {
    /// The root node's stored place (game units, the model's own space).
    pub root: NifTransform,
    /// The root's track and the controller's start and stop times, when it has one.
    pub track: Option<(Arc<TransformTrack>, f32, f32)>,
    /// The camera's transform under the root (its chain of nodes below the root).
    pub camera: Affine3A,
    /// The camera's horizontal field of view, degrees (2 atan of the frustum's right edge
    /// over its near plane), when it has a frustum.
    pub fov_x: Option<f32>,
}

impl CameraModel {
    /// The root's place `t` seconds into its animation, held at the ends: the track's
    /// translation and rotation where it has them, the stored ones elsewhere.
    pub fn root_at(&self, t: f32) -> (Vec3, Quat) {
        let stored = Mat3::from_cols_array(&self.root.rotation).transpose(); // stored row-major
        let mut at = Vec3::from_array(self.root.translation);
        let mut turn = Quat::from_mat3(&stored).normalize();
        if let Some((track, start, stop)) = &self.track {
            let t = (start + t.max(0.0)).min(stop.max(*start));
            if let Some(p) = track.translation.sample(t) {
                at = Vec3::from_array(p);
            }
            if let Some([w, x, y, z]) = track.rotation_at(t) {
                turn = Quat::from_xyzw(x, y, z, w).normalize();
            }
        }
        (at, turn)
    }

    /// How long the root's animation runs, seconds.
    pub fn length(&self) -> f32 {
        self.track.as_ref().map_or(0.0, |(_, a, b)| (b - a).max(0.0))
    }
}

/// Read a camera shot's model from its parts (`None` without an `NiCamera`).
pub fn read_camera_model(file: &NifFile, dec: &Decoded) -> Option<CameraModel> {
    let objects = scene_objects(file, dec);
    let cam = file.blocks.iter().position(|b| b.type_name == "NiCamera")?;
    let by_block = |b: usize| objects.iter().find(|o| o.block == b);
    let parent_of = |b: usize| objects.iter().find(|o| o.children.contains(&b)).map(|o| o.block);
    // The camera's chain up to the root (the node with no parent).
    let mut chain = vec![cam];
    let mut cur = cam;
    while let Some(p) = parent_of(cur).filter(|_| chain.len() < 64) {
        chain.push(p);
        cur = p;
    }
    let root = *chain.last()?;
    let camera_block = dec.blocks.get(cam)?.as_ref().ok()?;
    let local_of = |b: usize| -> Option<NifTransform> {
        if b == cam {
            NifTransform::from_struct(camera_block)
        } else {
            by_block(b).map(|o| o.transform)
        }
    };
    let affine = |t: &NifTransform| Affine3A::from_mat3_translation(Mat3::from_cols_array(&t.rotation).transpose() * t.scale, Vec3::from_array(t.translation));
    let mut camera = Affine3A::IDENTITY;
    for &b in chain.iter().rev().skip(1) {
        camera *= affine(&local_of(b)?);
    }
    let root_t = if root == cam { NifTransform::IDENTITY } else { local_of(root)? };
    // The root's own controller.
    let track = file.blocks.iter().enumerate().find_map(|(i, b)| {
        if b.type_name != "NiTransformController" {
            return None;
        }
        let c = dec.blocks.get(i)?.as_ref().ok()?;
        if c.block_ref("target") != Some(root as i32) {
            return None;
        }
        let interp = c.block_ref("interpolator").filter(|&r| r >= 0)?;
        let kind = file.blocks.get(interp as usize)?.type_name.as_str();
        let t = transform_track(dec, interp, kind)?;
        Some((Arc::new(t), c.float("start_time").unwrap_or(0.0), c.float("stop_time").unwrap_or(0.0)))
    });
    let fov_x = match (camera_block.float("frustum_right"), camera_block.float("frustum_near")) {
        (Some(r), Some(n)) if r > 0.0 && n > 0.0 => Some(2.0 * (r / n).atan().to_degrees()),
        (Some(r), _) if r > 0.0 => Some(2.0 * r.atan().to_degrees()),
        _ => None,
    };
    Some(CameraModel { root: root_t, track, camera, fov_x })
}

impl ModelCtx<'_> {
    /// The camera shot model at `path` (under `meshes\`), read on first use and
    /// remembered; `None` when it is missing or holds no camera.
    pub fn camera_model(&mut self, path: &str) -> Option<Arc<CameraModel>> {
        let key = fnv_vfs::normalize(path);
        if let Some(m) = self.cache.camera_models.get(&key) {
            return m.clone();
        }
        let m = (|| {
            let bytes = self.data.read(&key)?;
            let schema = self.data.schema.clone();
            let file = NifFile::parse(&bytes, Some(&schema)).ok()?;
            let dec = Decoded::new(&file, &bytes, &schema);
            read_camera_model(&file, &dec).map(Arc::new)
        })();
        self.cache.camera_models.insert(key, m.clone());
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::anim::{Channel, Key, KeyKind, Keys};

    fn model(track: Option<TransformTrack>) -> CameraModel {
        CameraModel {
            root: NifTransform { translation: [-59.0, -226.0, 18.0], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 },
            track: track.map(|t| (Arc::new(t), 0.0, 2.0)),
            camera: Affine3A::IDENTITY,
            fov_x: Some(75.0),
        }
    }

    #[test]
    fn the_root_follows_its_track_and_holds_at_the_ends() {
        assert_eq!(model(None).root_at(5.0).0, Vec3::new(-59.0, -226.0, 18.0));
        assert_eq!(model(None).length(), 0.0);
        let keys = Keys { kind: KeyKind::Linear, keys: vec![Key { time: 0.0, value: [0.0, -200.0, 20.0], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }, Key { time: 2.0, value: [0.0, -100.0, 20.0], forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] }] };
        let t = TransformTrack { translation: Channel::Keys(keys), rotation: Channel::Static(None), euler: None, scale: Channel::Static(None) };
        let m = model(Some(t));
        assert_eq!(m.length(), 2.0);
        assert!((m.root_at(1.0).0 - Vec3::new(0.0, -150.0, 20.0)).length() < 1e-3);
        assert!((m.root_at(9.0).0 - Vec3::new(0.0, -100.0, 20.0)).length() < 1e-3, "held at the end");
        assert!(m.root_at(1.0).1.angle_between(Quat::IDENTITY) < 1e-4, "no rotation keys: the stored turn");
    }
}
