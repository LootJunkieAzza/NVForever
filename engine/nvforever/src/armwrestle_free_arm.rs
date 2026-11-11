//! The wrestlers' free (left) arms at the arm-wrestling table (queue item 199): while the
//! right hands are clasped (`armwrestle_arm`), the left hand lies flat on the table on the
//! wrestler's own side, a little to the left of the right elbow, fingers towards the
//! opponent, the elbow out to the left [design: the free hand braced on the table, as
//! arm wrestlers brace; the chair's sitting pose leaves it on the lap]. Posed over the
//! seated pose by the same weight as the right arm, after the upper body has leaned in.

use crate::darts_arm::{rot, solve, world_of};
use crate::limbs::ActorRig;
use bevy::prelude::*;

/// The free arm's bones [observed: the human skeleton's node names].
const UPPER: &str = "Bip01 L UpperArm";
const FORE: &str = "Bip01 L Forearm";
const HAND: &str = "Bip01 L Hand";
/// The left hand's fingers' first bones, to measure the hand's length and frame [observed:
/// node names].
const KNUCKLES: [&str; 4] = ["Bip01 L Finger1", "Bip01 L Finger2", "Bip01 L Finger3", "Bip01 L Finger4"];
/// The thumb's first bone [observed: node name].
const THUMB: &str = "Bip01 L Thumb1";
/// Where the palm's middle lies on the table, from the table's middle: back towards the
/// wrestler and out to the left, game units [design: beside the right elbow, which is
/// about 8 back from the middle, `armwrestle_arm::ELBOW_IN`].
const BACK: f32 = 9.0;
const OUT: f32 = 14.0;
/// The palm's middle from `top` (the elbows' height, 2 over the table's top,
/// `armwrestle_arm::ELBOW_UP`): half a hand's thickness over the top [design].
const PALM_UP: f32 = -0.8;
/// The fingers' turn inwards from straight towards the opponent, radians [design: about
/// 25 degrees, the hand angled in as it lies].
const TURN_IN: f32 = 0.45;
/// The palm's middle from the wrist, as a share of the knuckles' distance [design].
const PALM_MID: f32 = 0.6;

/// Lays wrestler `root`'s left hand flat on the table: `top` over the middle of the table's
/// top (Bevy, its y the elbows' height, 2 over the top), `toward` the level direction to the opponent, by `w`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn brace(root: Entity, rig: &ActorRig, top: Vec3, toward: Vec3, w: f32, parents: &Query<&ChildOf>, transforms: &mut Query<&mut Transform>, globals: &Query<&GlobalTransform>) -> Result<(), &'static str> {
    if w <= 0.0 {
        return Ok(());
    }
    let node = |name: &str| rig.skel.names.iter().position(|n| n.as_deref() == Some(name)).and_then(|i| rig.nodes.get(i).copied());
    let (Some(upper), Some(fore), Some(hand)) = (node(UPPER), node(FORE), node(HAND)) else { return Err("no left arm") };
    let above = parents.get(upper).map_err(|_| "the arm hangs from nothing")?.parent();
    let gp = world_of(above, root, parents, transforms, globals).ok_or("the arm is not under the body")?;
    let local = |e: Entity| transforms.get(e).copied().map_err(|_| "a bone has no transform");
    let (lu, lf, lh) = (local(upper)?, local(fore)?, local(hand)?);
    let gu = gp.mul_transform(lu);
    let gf = gu.mul_transform(lf);
    let gh = gf.mul_transform(lh);
    let (s, e0, h0) = (gu.translation(), gf.translation(), gh.translation());
    if s.distance(e0) < 1e-3 || e0.distance(h0) < 1e-3 {
        return Err("the arm has no length");
    }
    // The hand's own frame, in its local space: along (wrist to the knuckles' middle), and
    // the palm's normal: of the two normals to along and the thumb, the one the seated pose
    // turns down (the hand rests palm down on the lap [observed: the chair's sitting idle]).
    let at = |e: Entity| -> Option<Vec3> {
        (parents.get(e).ok()?.parent() == hand).then_some(())?;
        Some(transforms.get(e).ok()?.translation)
    };
    let knuckles: Vec<Vec3> = KNUCKLES.iter().filter_map(|n| node(n).and_then(at)).collect();
    let frame = if knuckles.len() >= 2 {
        let k = knuckles.iter().sum::<Vec3>() / knuckles.len() as f32;
        node(THUMB).and_then(at).and_then(|t| {
            let a = k.try_normalize()?;
            let t = (t - a * t.dot(a)).try_normalize()?;
            let n = t.cross(a).normalize();
            let hr = rot(&gh);
            let p = if (hr * n).y < 0.0 { n } else { -n };
            Some((a, p, k.length() * PALM_MID))
        })
    } else {
        None
    };
    let left = Vec3::Y.cross(toward).normalize_or(Vec3::X);
    let along_w = (toward * TURN_IN.cos() - left * TURN_IN.sin()).normalize();
    let palm_mid = Vec3::new(top.x, top.y + PALM_UP, top.z) - toward * BACK + left * OUT;
    let reach = frame.map_or(0.0, |f| f.2);
    let wrist = palm_mid - along_w * reach;
    // The elbow out to the left and a little back and down, as a braced arm bends.
    let pole = left - toward * 0.4 - Vec3::Y * 0.3;
    let (e, h) = solve(s, e0, h0, wrist, pole);
    // The upper arm and the forearm turned to reach there (as `armwrestle_arm::Arm::apply`).
    let rp = rot(&gp);
    let ru = rp * lu.rotation;
    let du = Quat::from_rotation_arc((e0 - s).normalize(), (e - s).normalize_or(Vec3::NEG_Y));
    let ru2 = du * ru;
    let lu2 = (rp.inverse() * ru2).normalize();
    let cf = du * (h0 - e0);
    let df = Quat::from_rotation_arc(cf.normalize(), (h - e).normalize_or(Vec3::Y));
    let lf2 = (ru2.inverse() * df * ru2 * lf.rotation).normalize();
    let (u2, f2) = (lu.rotation.slerp(lu2, w), lf.rotation.slerp(lf2, w));
    if let Ok(mut t) = transforms.get_mut(upper) {
        t.rotation = u2;
    }
    if let Ok(mut t) = transforms.get_mut(fore) {
        t.rotation = f2;
    }
    // The hand flat, palm down, fingers along `along_w`.
    if let Some((a, p, _)) = frame {
        let t = p.cross(a).normalize();
        let palm_w = Vec3::NEG_Y;
        let thumb_w = palm_w.cross(along_w).normalize();
        let world = Mat3::from_cols(along_w, thumb_w, palm_w) * Mat3::from_cols(a, t, p).transpose();
        let fore_w = rp * u2 * f2;
        let to = (fore_w.inverse() * Quat::from_mat3(&world)).normalize();
        if let Ok(mut tr) = transforms.get_mut(hand) {
            tr.rotation = lh.rotation.slerp(to, w);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_free_hand_lies_on_the_wrestlers_own_left() {
        // Facing -Z (Bevy forward), the left is -X.
        let toward = Vec3::NEG_Z;
        let left = Vec3::Y.cross(toward);
        assert!((left - Vec3::NEG_X).length() < 1e-6);
        // The fingers point mostly at the opponent, turned in (to the right).
        let along = (toward * TURN_IN.cos() - left * TURN_IN.sin()).normalize();
        assert!(along.dot(toward) > 0.85 && along.x > 0.0);
    }
}
