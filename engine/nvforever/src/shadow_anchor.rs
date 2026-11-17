//! The sun's shadow cascades snapped to a texel grid anchored near the camera instead of
//! the grid through the world's origin, and the order of every change made to the
//! directional lights' cascades after Bevy builds them ([`cascade_edits`]). #2833
//! [engineering choice throughout]
//!
//! **Why.** Bevy builds each cascade (`bevy_light` `calculate_cascade`) with the centre of
//! its near plane, in the light's axes, at a whole number of the cascade's texels, so that
//! a camera moving under a still light finds every shadow edge at the same place in the
//! map's texels and the edges stand still. The grid those whole numbers make is fixed to
//! the light's axes through the world's origin. The sun light turns a little every frame
//! (sky.rs `aim_sun`, following the game clock: `SkyState::sun_light_hour`), and a
//! worldspace's places lie up to tens of thousands of units from its origin: a turn of
//! `dθ` slides the grid under the camera by `|camera| × dθ`. For a camera 70,000 units out
//! and a turn of 3.6e-5 rad a frame (a game day of 48 minutes at 60 frames a second) that
//! is about 2.5 units a frame, four texels of a cascade of 2048 texels over 1,280 units.
//! Every frame then samples every shadow edge at a new fraction of a texel, and the
//! snapping steadies nothing: the shadows shimmer while the player stands still. The
//! cascades held on the staggered schedule (shadow_rate.rs) keep the matrices they were
//! drawn with, so a held cascade does not shimmer, and the shimmer comes as a pulse on the
//! frames a farther cascade is drawn again: at half the frame rate for the middle cascade
//! on Quality, and for both cascades on Performance.
//!
//! **What.** [`anchor_cascades`] moves each cascade of the sun and of the first-person
//! arms' shade light (player_body.rs `ArmsShade`, which turns with the sun) across the
//! light by at most about a texel, toward the middle of the slice it covers, so that its
//! grid is fixed to the light's axes through an anchor point near the cascade's middle
//! ([`snap_to_anchor`]). A turn of the light then slides the grid at a point `P` by
//! `|P − anchor| × dθ`, a few hundred units times `dθ` instead of 70,000 times: in texels,
//! at most about 0.03 of one a frame at the cascade's middle and 0.07 at its edge (at
//! 3.6e-5 rad a frame, `P` up to 887 texels from the anchor and 1,024 more to the edge of
//! a 2048-texel map). The anchor stays put while the cascade's middle stays within
//! [`ANCHOR_REACH_TEXELS`] texels of it on every world axis (so up to about 887 texels
//! away in a straight line); past that it moves by whole texels in the light's axes, which
//! keeps the grid where it was in that frame ([`pick_anchor`]). A cascade of another texel
//! size takes a fresh anchor: the moon's single cascade by night, Performance's second
//! cascade (the first covers the same slice in both modes, has the same texel and keeps
//! its anchor), a view whose frustum changed. The anchors of cascades no longer built (a
//! light or view gone, fewer cascades, a light casting no shadows) are forgotten. Test aid `NVF_PLAY_TEST_SHADOW_ANCHOR=0` leaves the
//! cascades on Bevy's grid through the origin, for measuring against.
//!
//! **Measured** [observed 2026-11-16: Goodsprings' start view standing still, 1280 × 720,
//! vertical sync at 60 Hz, `--no-actors --no-grass`; the mean change of luma (of 255)
//! between consecutive frames over the ground and the house]:
//!
//! | Sun shadows, hour | Grid through the origin | Anchored |
//! |---|---|---|
//! | Quality, 09:00 | 0.02 / 0.10 alternating (0.10 on the frames the middle cascade is drawn; 0.10 every frame with `NVF_PLAY_TEST_SHADOW_STAGGER=0`) | 0.01, staggered or not |
//! | Quality, 09:00, the sun light held still | 0.02-0.03 | |
//! | Quality, 07:00 | 0.114 | 0.012 |
//! | Performance, 07:00 | 0.25 / 0.00 alternating (0.127) | 0.013 |
//!
//! Anchoring takes about 90 % of the frame-to-frame change away, to below what is left
//! with the light held still on the origin's grid.
//!
//! **Precision.** The shader still forms `clip_from_world × position` in 32-bit floats with
//! positions 70,000 units out, as it does with Bevy's own snapping. A float there is good to
//! 2^-7 ≈ 0.008 units (half that, rounded to nearest): the matrix's rotation entries and its
//! translation (the near-plane centre, rounded once to a float) each carry about that much,
//! and the sums in the shader a few times as much again, together a few hundredths of a
//! unit, a few hundredths of the nearest cascade's texel. That error changes from frame to
//! frame with the light's turn under either snapping; it is the floor, and well below what
//! a filtered shadow edge can show. The anchoring itself is worked out in 64-bit floats.
//! What it costs: a few matrices per cascade a frame (three cascades for the sun and one
//! for the arms' light per view).

use crate::player_body::ArmsShade;
use crate::sky::Sun;
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::light::cascade::Cascade;
use bevy::light::{Cascades, SimulationLightSystems};
use bevy::math::{DMat3, DVec2, DVec3};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// How far (in the cascade's texels, on each world axis) the cascade's middle may move from
/// its anchor before the anchor follows: 512 texels, about 320 units in the nearest cascade
/// and a quarter of a 2048-texel map [engineering choice: rare re-anchoring, while the
/// grid's slide at the cascade's edge stays a small fraction of a texel a frame].
pub const ANCHOR_REACH_TEXELS: f64 = 512.0;

/// Whether the cascades are anchored: always, unless test aid `NVF_PLAY_TEST_SHADOW_ANCHOR=0`
/// leaves them on Bevy's grid through the world's origin, for measuring against. Read once.
pub(crate) fn anchored() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PLAY_TEST_SHADOW_ANCHOR").map_or(true, |v| v != "0"))
}

/// A cascade's anchor: the world point its texel grid is fixed to, and the texel size it
/// was picked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub at: DVec3,
    pub texel: f32,
}

/// The anchor of every cascade built this frame, by light, view and cascade index (nearest
/// first).
#[derive(Resource, Default, Debug)]
pub(crate) struct CascadeAnchors(pub HashMap<(Entity, Entity, usize), Anchor>);

/// The light's rotation (light axes to world) and the near-plane centre in world space.
fn rotation_and_origin(c: &Cascade) -> (DMat3, DVec3) {
    (Mat3::from_mat4(c.world_from_cascade).as_dmat3(), c.world_from_cascade.w_axis.truncate().as_dvec3())
}

/// The world position of the middle of the cascade's box: the near-plane centre moved half
/// the box's depth away from the light (Bevy's projection keeps `1 / depth` in its z scale).
pub fn cascade_centre(c: &Cascade) -> DVec3 {
    let (world_from_light, origin) = rotation_and_origin(c);
    let r = f64::from(c.clip_from_cascade.z_axis.z);
    let depth = if r > 0.0 { 1.0 / r } else { 0.0 };
    origin - world_from_light * DVec3::new(0.0, 0.0, 0.5 * depth)
}

/// The anchor for a cascade whose middle is at `centre` this frame, given last frame's.
///
/// A new one (none before, or another texel size) is `centre` rounded on each world axis to
/// [`ANCHOR_REACH_TEXELS`] texels. The last one is kept while `centre` is within that many
/// texels of it on every world axis. Past that the anchor moves to about `centre`, by whole
/// texels across the light (`light_from_world`'s x and y) and freely along it: in this
/// frame's light axes the grid through the new anchor is the grid through the old one, so
/// re-anchoring moves no shadow edge. (Rounding the new anchor in world axes instead would
/// shift every edge of the cascade by some fraction of a texel at once, a visible jump even
/// with the light still.)
pub fn pick_anchor(centre: DVec3, texel: f32, light_from_world: DMat3, prev: Option<Anchor>) -> Anchor {
    let t = f64::from(texel);
    let reach = ANCHOR_REACH_TEXELS * t;
    match prev {
        Some(p) if p.texel == texel => {
            let off = centre - p.at;
            if off.abs().max_element() <= reach {
                return p;
            }
            let l = light_from_world * off;
            let step = DVec3::new((l.x / t).round() * t, (l.y / t).round() * t, l.z);
            Anchor { at: p.at + light_from_world.transpose() * step, texel }
        }
        _ => Anchor { at: (centre / reach).round() * reach, texel },
    }
}

/// Snapping a cascade already on an anchor's grid again leaves it there, against float
/// error up to this share of a texel (see [`snap_to_anchor`]): 1/8, against the 32-bit
/// near-plane centre's few thousandths of a unit 70,000 units out, a few hundredths of the
/// finest cascade's texel [engineering choice].
const SNAP_TIE: f64 = 0.125;

/// `c` moved across the light so that its near-plane centre lies a whole number of texels
/// from `anchor` in the light's x and y (depth unchanged): to the grid point nearest the
/// middle of the slice as far as the cascade tells it. Bevy floors the near-plane centre
/// to whole texels, so the slice's middle lies within a texel above it on each axis; the
/// best guess is Bevy's centre plus half a texel, and the grid point nearest that is the
/// first at or above Bevy's centre, taken from [`SNAP_TIE`] of a texel below it so that a
/// centre on the grid already (or a float's error off it) stays. The centre then moves at
/// most `1 − SNAP_TIE` texels up or `SNAP_TIE` down from Bevy's, and lies within
/// `1 + SNAP_TIE` texels of the slice's middle (Bevy's own within one), where rounding
/// Bevy's floored centre to the nearest grid point would leave it up to 1.5 texels off.
/// Its matrices are formed as `bevy_light` `calculate_cascade` forms them: `cascade_from_world`
/// straight from the transposed rotation and the negated centre (not by inverting), and
/// `clip_from_world = clip_from_cascade × cascade_from_world`. The projection and texel
/// size stay.
pub fn snap_to_anchor(c: &Cascade, anchor: DVec3) -> Cascade {
    let (world_from_light, origin) = rotation_and_origin(c);
    let light_from_world = world_from_light.transpose();
    let t = f64::from(c.texel_size);
    if t <= 0.0 {
        return c.clone();
    }
    // Bevy's near-plane centre in the light's axes (its x and y a whole number of texels
    // from the world's origin, its z the near plane), and the anchor's.
    let n = light_from_world * origin;
    let a = (light_from_world * anchor).truncate();
    let xy: DVec2 = a + ((n.truncate() - a) / t - DVec2::splat(SNAP_TIE)).ceil() * t;
    let centre = xy.extend(n.z);
    let light_axes = Mat3::from_mat4(c.world_from_cascade).transpose();
    let cascade_from_world = Mat4::from_cols(light_axes.x_axis.extend(0.0), light_axes.y_axis.extend(0.0), light_axes.z_axis.extend(0.0), (-centre).as_vec3().extend(1.0));
    let world_from_cascade = Mat4::from_cols(c.world_from_cascade.x_axis, c.world_from_cascade.y_axis, c.world_from_cascade.z_axis, (world_from_light * centre).as_vec3().extend(1.0));
    Cascade { world_from_cascade, clip_from_cascade: c.clip_from_cascade, clip_from_world: c.clip_from_cascade * cascade_from_world, texel_size: c.texel_size }
}

/// Every cascade of every view of the sun and the arms' shade light snapped to its anchor
/// (module notes). A light casting no shadows is left alone: Bevy leaves its cascades as
/// they last were, and its anchors go, so it starts afresh when it casts again. Logs once:
/// the first frame it anchors a cascade, or, with the test aid, that it does not.
#[allow(clippy::type_complexity)]
pub(crate) fn anchor_cascades(
    mut lights: Query<(Entity, &DirectionalLight, &mut Cascades), Or<(With<Sun>, With<ArmsShade>)>>,
    mut anchors: ResMut<CascadeAnchors>,
    mut said: Local<bool>,
) {
    if !anchored() {
        if !*said {
            info!("sun shadows: cascades left on the grid through the world's origin (test aid NVF_PLAY_TEST_SHADOW_ANCHOR=0)");
            *said = true;
        }
        return;
    }
    let anchors = &mut anchors.0;
    let mut seen = Vec::with_capacity(anchors.len());
    for (light, directional, mut cascades) in &mut lights {
        if !directional.shadow_maps_enabled {
            continue;
        }
        for (view, list) in cascades.cascades.iter_mut() {
            for (i, c) in list.iter_mut().enumerate() {
                let key = (light, *view, i);
                let light_from_world = Mat3::from_mat4(c.world_from_cascade).transpose().as_dmat3();
                let anchor = pick_anchor(cascade_centre(c), c.texel_size, light_from_world, anchors.get(&key).copied());
                anchors.insert(key, anchor);
                seen.push(key);
                *c = snap_to_anchor(c, anchor.at);
            }
        }
    }
    if !*said && !seen.is_empty() {
        info!("sun shadows: cascades snapped to a grid anchored near the camera");
        *said = true;
    }
    // Forget the anchors of cascades not built this frame (every key seen is in the map,
    // so equal counts mean the same keys).
    if seen.len() != anchors.len() {
        anchors.retain(|k, _| seen.contains(k));
    }
}

/// The changes made to the directional lights' cascades after Bevy builds them
/// (`SimulationLightSystems::UpdateDirectionalLightCascades`, every frame, from scratch) and
/// before it fits the lights' frusta to them, in their one order:
///
/// 1. [`anchor_cascades`] first: it reads the depth Bevy gave each cascade
///    ([`cascade_centre`]), and what the hold keeps must already be on its anchored grid.
/// 2. `sky::reach_toward_sun`, which stretches the sun's cascades toward the sun, before
/// 3. `shadow_rate::hold_cascades`, which gives a cascade not drawn this frame back as the
///    frame that drew it had it. Either order of these two stretches a held cascade exactly
///    once: with the hold first it keeps and restores the unstretched cascade and the
///    stretch is applied to the copy, with the stretch first it keeps the stretched one and
///    nothing touches the copy after. The stretch first is chosen because the kept cascade
///    is then the very one its map was drawn with, whatever the stretch does on a later
///    frame; with the hold first a held map would be read through a later frame's stretch.
///    (Before this order was set the two ran in whichever order Bevy picked: by the same
///    argument the stretch was neither lost nor doubled.)
///
/// Nothing else writes the cascades in between.
pub(crate) fn cascade_edits() -> ScheduleConfigs<ScheduleSystem> {
    (anchor_cascades, crate::sky::reach_toward_sun, crate::shadow_rate::hold_cascades).chain()
}

/// Adds [`cascade_edits`] (with them [`anchor_cascades`], always; the test aid turns it
/// off from inside). `shadow_rate::ShadowRatePlugin` adds the resources the hold needs.
pub struct ShadowAnchorPlugin;

impl Plugin for ShadowAnchorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CascadeAnchors>().add_systems(
            PostUpdate,
            cascade_edits().after(SimulationLightSystems::UpdateDirectionalLightCascades).before(SimulationLightSystems::UpdateLightFrusta),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadow_rate::PerformanceShadows;
    use bevy::pbr::HoldDirectionalShadowMaps;

    /// The size of the test cascades' maps, their texel (1,280 units over 2048 texels, a
    /// binary fraction as Bevy's are) and depth.
    const SIZE: f32 = 2048.0;
    const TEXEL: f32 = 0.625;
    const DEPTH: f32 = 2000.0;

    /// A cascade as `calculate_cascade` builds it for a light turned by `world_from_light`
    /// over a slice whose middle is at `middle` (world): the near-plane centre floored to
    /// whole texels in the light's x and y, the near plane half the depth toward the light.
    fn bevy_cascade(world_from_light: Mat4, middle: Vec3) -> Cascade {
        cascade_of(world_from_light, middle, TEXEL, DEPTH)
    }

    /// The same with another texel size and depth.
    fn cascade_of(world_from_light: Mat4, middle: Vec3, texel: f32, depth: f32) -> Cascade {
        let m = world_from_light.transpose().transform_point3(middle);
        let diameter = texel * SIZE;
        let n = Vec3::new((m.x / texel).floor() * texel, (m.y / texel).floor() * texel, m.z + 0.5 * depth);
        let t = world_from_light.transpose();
        let cascade_from_world = Mat4::from_cols(t.x_axis, t.y_axis, t.z_axis, (-n).extend(1.0));
        let world_from_cascade = Mat4::from_cols(world_from_light.x_axis, world_from_light.y_axis, world_from_light.z_axis, world_from_light * n.extend(1.0));
        let clip_from_cascade = Mat4::from_cols(Vec4::new(2.0 / diameter, 0.0, 0.0, 0.0), Vec4::new(0.0, 2.0 / diameter, 0.0, 0.0), Vec4::new(0.0, 0.0, 1.0 / depth, 0.0), Vec4::new(0.0, 0.0, 1.0, 1.0));
        Cascade { world_from_cascade, clip_from_cascade, clip_from_world: clip_from_cascade * cascade_from_world, texel_size: texel }
    }

    /// The near-plane centre in the light's axes.
    fn near_centre(c: &Cascade) -> DVec3 {
        let (r, o) = rotation_and_origin(c);
        r.transpose() * o
    }

    fn light_from_world(c: &Cascade) -> DMat3 {
        Mat3::from_mat4(c.world_from_cascade).transpose().as_dmat3()
    }

    /// `p` in the cascade's map, in texels from the map's middle, as the shader forms it:
    /// one 32-bit `clip_from_world`.
    fn texel_coords(c: &Cascade, p: Vec3) -> Vec2 {
        (c.clip_from_world * p.extend(1.0)).truncate().truncate() * (SIZE / 2.0)
    }

    /// A difference of texel phases, wrapped to -0.5..0.5.
    fn wrapped(d: f32) -> f32 {
        d - d.round()
    }

    /// A place far from the worldspace's origin.
    const FAR: Vec3 = Vec3::new(70_000.0, 120.0, -3_000.0);

    fn sun(turned: f32) -> Mat4 {
        Mat4::from_quat(Quat::from_axis_angle(Vec3::new(0.3, 1.0, 0.2).normalize(), turned) * Quat::from_rotation_x(-1.0))
    }

    #[test]
    fn snapping_moves_the_centre_under_half_a_texel_across_the_light_and_keeps_its_depth() {
        let c = bevy_cascade(sun(0.0), FAR + Vec3::new(37.0, 0.0, 81.0));
        let t = f64::from(TEXEL);
        for anchor in [FAR.as_dvec3(), FAR.as_dvec3() + DVec3::new(123.4, -5.6, 78.9), DVec3::ZERO] {
            let s = snap_to_anchor(&c, anchor);
            let (n, m) = (near_centre(&c), near_centre(&s));
            let d = m - n;
            // Up toward the middle by under a texel, or down by at most SNAP_TIE of one.
            for d in [d.x, d.y] {
                assert!((-SNAP_TIE * t - 0.02..=(1.0 - SNAP_TIE) * t + 0.02).contains(&d), "{d}");
            }
            assert!(d.z.abs() < 0.05, "depth kept: {d}");
            // Its x and y are whole texels from the anchor's.
            let a = light_from_world(&c) * anchor;
            let k = (m.truncate() - a.truncate()) / f64::from(TEXEL);
            assert!((k - k.round()).abs().max_element() < 0.05, "{k}");
            // Snapping again changes nothing (no walk by a texel a snap).
            let again = near_centre(&snap_to_anchor(&s, anchor));
            assert!((again - m).abs().max_element() < 0.02, "{again} {m}");
            assert_eq!((s.clip_from_cascade, s.texel_size), (c.clip_from_cascade, c.texel_size));
        }
    }

    #[test]
    fn the_view_projection_is_the_projection_times_the_inverse_placement() {
        let c = bevy_cascade(sun(0.3), FAR);
        let s = snap_to_anchor(&c, FAR.as_dvec3() + DVec3::new(-200.0, 40.0, 170.0));
        let want = s.clip_from_cascade.as_dmat4() * s.world_from_cascade.as_dmat4().inverse();
        let got = s.clip_from_world.as_dmat4();
        // Entries up to (2 / 1280) × 70,000 ≈ 110; a float's rounding there is ~1e-5.
        assert!(got.abs_diff_eq(want, 1e-4), "{got} vs {want}");
        // The rotation is the light's, unchanged.
        assert_eq!(Mat3::from_mat4(s.world_from_cascade), Mat3::from_mat4(c.world_from_cascade));
    }

    /// The point of the anchoring. A point `P` 70,000 units from the origin, the camera next
    /// to it, and the light turned by 3.6e-5 rad between two frames (a game day of 48 minutes
    /// at 60 frames a second). With Bevy's grid through the origin, `P` slides
    /// |P| × dθ ≈ 2.5 units ≈ 3.6 texels across the light here, and lands at another fraction
    /// of a texel (0.37 of one in y). With the grid through an anchor ~190 units from `P`
    /// it slides 190 × 3.6e-5 / 0.625 ≈ 0.01 texels; the 32-bit matrix adds a few hundredths.
    #[test]
    fn a_turning_light_keeps_the_texel_phase_with_the_anchor_and_scrambles_it_without() {
        let dtheta = 3.6e-5;
        let p = FAR;
        let camera = p + Vec3::new(20.0, 0.0, -30.0);
        let (c0, c1) = (bevy_cascade(sun(0.0), camera), bevy_cascade(sun(dtheta), camera));
        let a0 = pick_anchor(cascade_centre(&c0), TEXEL, light_from_world(&c0), None);
        let a1 = pick_anchor(cascade_centre(&c1), TEXEL, light_from_world(&c1), Some(a0));
        assert_eq!(a0, a1, "the anchor stays");
        assert!((a0.at - p.as_dvec3()).length() < 300.0, "{} from P", (a0.at - p.as_dvec3()).length());
        let (s0, s1) = (snap_to_anchor(&c0, a0.at), snap_to_anchor(&c1, a1.at));
        let anchored = texel_coords(&s1, p) - texel_coords(&s0, p);
        let origin = texel_coords(&c1, p) - texel_coords(&c0, p);
        let (anchored, origin) = (Vec2::new(wrapped(anchored.x), wrapped(anchored.y)), Vec2::new(wrapped(origin.x), wrapped(origin.y)));
        assert!(anchored.abs().max_element() < 0.08, "anchored phase change {anchored}");
        assert!(origin.abs().max_element() > 0.25, "origin-anchored phase change {origin}");
        // The slide behind it, worked in 64 bits: several texels with the origin's grid.
        let slide = (light_from_world(&c1) - light_from_world(&c0)) * p.as_dvec3() / f64::from(TEXEL);
        assert!(slide.truncate().length() > 2.0, "{slide}");
    }

    #[test]
    fn the_anchor_stays_within_its_reach_and_then_moves_by_whole_texels() {
        let r = Mat3::from_mat4(sun(0.2)).transpose().as_dmat3();
        let t = f64::from(TEXEL);
        let reach = ANCHOR_REACH_TEXELS * t;
        let first = pick_anchor(DVec3::new(70_010.0, 100.0, -2_990.0), TEXEL, r, None);
        // A new anchor: the middle rounded to the reach on each world axis.
        assert_eq!(first.at, (DVec3::new(70_010.0, 100.0, -2_990.0) / reach).round() * reach);
        // Kept while the middle is within the reach on every axis.
        let near = first.at + DVec3::new(0.99 * reach, -0.99 * reach, 0.5 * reach);
        assert_eq!(pick_anchor(near, TEXEL, r, Some(first)), first);
        // Past it on one axis: moved, to about the middle, by whole texels across the light.
        let past = first.at + DVec3::new(reach + 1.0, 0.0, 0.0);
        let moved = pick_anchor(past, TEXEL, r, Some(first));
        assert_ne!(moved, first);
        let step = r * (moved.at - first.at) / t;
        // (`r` is a 32-bit rotation widened, orthonormal to about 1e-7.)
        assert!((step.truncate() - step.truncate().round()).abs().max_element() < 1e-3, "{step}");
        assert!((r * (moved.at - past)).truncate().abs().max_element() <= 0.5 * t + 1e-3);
        // So a cascade snapped to either lies on the same grid: no edge moves.
        let c = bevy_cascade(sun(0.2), past.as_vec3());
        let (s_old, s_new) = (snap_to_anchor(&c, first.at), snap_to_anchor(&c, moved.at));
        let k = (near_centre(&s_new) - near_centre(&s_old)).truncate() / t;
        assert!((k - k.round()).abs().max_element() < 0.05, "{k}");
        // Another texel size: a new anchor.
        let other = pick_anchor(first.at, TEXEL * 2.0, r, Some(first));
        assert_eq!(other.texel, TEXEL * 2.0);
        assert_eq!(other.at, (first.at / (2.0 * reach)).round() * (2.0 * reach));
    }

    /// A world with what [`cascade_edits`] reads (Sun shadows on Performance, so the hold's
    /// schedule does not hang on a test aid), a schedule running them, one view.
    fn edits_world() -> (World, Schedule, Entity) {
        let mut world = World::new();
        world.insert_resource(PerformanceShadows(true));
        world.init_resource::<HoldDirectionalShadowMaps>();
        world.init_resource::<CascadeAnchors>();
        let mut schedule = Schedule::default();
        schedule.add_systems(cascade_edits());
        let view = world.spawn_empty().id();
        (world, schedule, view)
    }

    /// `cascades` for `view`, as Bevy builds them afresh every frame.
    fn built(view: Entity, cascades: Vec<Cascade>) -> Cascades {
        let mut c = Cascades::default();
        c.cascades.insert(view, cascades);
        c
    }

    fn sun_light(world: &mut World, on: bool) -> Entity {
        world.spawn((Sun, DirectionalLight { shadow_maps_enabled: on, ..default() })).id()
    }

    /// A cascade kept by the hold and given back on a frame that does not draw it carries
    /// the stretch toward the sun once, as the frame that drew it had it, and its anchoring.
    #[test]
    fn a_held_cascade_is_the_drawn_one_stretched_toward_the_sun_once() {
        let (mut world, mut schedule, view) = edits_world();
        let light = sun_light(&mut world, true);
        // Bevy's cascades of three frames, each a little deeper than the last so that a
        // given-back cascade can be told from this frame's.
        let frame = |k: f32| built(view, vec![cascade_of(sun(0.0), FAR, TEXEL, DEPTH + k), cascade_of(sun(0.0), FAR, 4.0 * TEXEL, 4.0 * DEPTH + k)]);
        let stretched_once = |k: f32, depth: f32| 1.0 / (depth + k + crate::sky::SUN_REACH_BACK);
        // Performance holds both cascades every other frame: the first two frames draw (the
        // first has nothing kept yet), the third is held.
        let mut drawn = Vec::new();
        for k in [1.0, 2.0, 3.0] {
            world.entity_mut(light).insert(frame(k));
            schedule.run(&mut world);
            let now = world.get::<Cascades>(light).unwrap().cascades[&view].clone();
            let held = world.resource::<HoldDirectionalShadowMaps>().0;
            if k < 3.0 {
                assert_eq!(held, 0, "frame {k} draws");
                for (c, depth) in now.iter().zip([DEPTH, 4.0 * DEPTH]) {
                    let r = c.clip_from_cascade.z_axis.z;
                    assert!((r - stretched_once(k, depth)).abs() < 1e-5 * r, "frame {k}: {r} against {}", stretched_once(k, depth));
                }
                drawn = now;
            } else {
                assert_eq!(held, 0b11, "frame 3 keeps both maps");
                for (c, d) in now.iter().zip(&drawn) {
                    // The cascade of frame 2, stretched once, exactly as it was drawn.
                    assert_eq!((c.clip_from_world, c.clip_from_cascade, c.world_from_cascade, c.texel_size), (d.clip_from_world, d.clip_from_cascade, d.world_from_cascade, d.texel_size));
                }
                let r = now[0].clip_from_cascade.z_axis.z;
                assert!((r - stretched_once(2.0, DEPTH)).abs() < 1e-5 * r, "{r}");
            }
        }
        // Each drawn cascade is on its anchor's grid (the anchoring ran before the hold kept it).
        let anchors = &world.resource::<CascadeAnchors>().0;
        for (i, c) in drawn.iter().enumerate() {
            let a = anchors[&(light, view, i)];
            let k = (near_centre(c).truncate() - (light_from_world(c) * a.at).truncate()) / f64::from(c.texel_size);
            assert!((k - k.round()).abs().max_element() < 0.05, "cascade {i}: {k}");
        }
    }

    /// The chain's order: the anchoring runs on the cascades Bevy built this frame, before
    /// the hold gives a kept one back. Between a drawn and a held frame Bevy's cascade moves
    /// past the anchor's reach: the anchor follows the cascade built (it re-anchors), and
    /// the held cascade is the drawn one, bit for bit. Were the anchoring after the hold, it
    /// would see the given-back cascade instead, at the old place, and keep the old anchor.
    #[test]
    fn the_anchors_follow_the_built_cascades_not_the_held_ones() {
        let (mut world, mut schedule, view) = edits_world();
        let light = sun_light(&mut world, true);
        let key = (light, view, 0);
        let one = |at: Vec3| built(view, vec![bevy_cascade(sun(0.0), at)]);
        // Performance, one cascade: frames 1 and 2 draw (the first has nothing kept), 3 holds.
        for _ in 0..2 {
            world.entity_mut(light).insert(one(FAR));
            schedule.run(&mut world);
        }
        assert_eq!(world.resource::<HoldDirectionalShadowMaps>().0, 0);
        let drawn = world.get::<Cascades>(light).unwrap().cascades[&view][0].clone();
        let before = world.resource::<CascadeAnchors>().0[&key];
        let moved = FAR + Vec3::new(3.0 * (ANCHOR_REACH_TEXELS * f64::from(TEXEL)) as f32, 0.0, 0.0);
        let fresh = bevy_cascade(sun(0.0), moved);
        let want = pick_anchor(cascade_centre(&fresh), TEXEL, light_from_world(&fresh), Some(before));
        assert_ne!(want, before, "the move re-anchors");
        world.entity_mut(light).insert(one(moved));
        schedule.run(&mut world);
        assert_eq!(world.resource::<HoldDirectionalShadowMaps>().0, 0b1, "frame 3 holds");
        let now = &world.get::<Cascades>(light).unwrap().cascades[&view][0];
        assert_eq!((now.clip_from_world, now.clip_from_cascade, now.world_from_cascade, now.texel_size), (drawn.clip_from_world, drawn.clip_from_cascade, drawn.world_from_cascade, drawn.texel_size));
        assert_eq!(world.resource::<CascadeAnchors>().0[&key], want, "anchored from the cascade built this frame");
    }

    /// The anchors follow the cascades: the moon's single cascade of another texel size
    /// takes a fresh anchor and the day's others go; a light casting no shadows is left
    /// alone and loses its anchors; a new sun (a worldspace switch) has its own.
    #[test]
    fn anchors_follow_the_cascades_built() {
        let (mut world, mut schedule, view) = edits_world();
        let day = sun_light(&mut world, true);
        let three = built(view, (0..3).map(|i| cascade_of(sun(0.0), FAR, TEXEL * (1 << i) as f32, DEPTH)).collect());
        world.entity_mut(day).insert(three);
        schedule.run(&mut world);
        let keys = |world: &World| {
            let mut k: Vec<_> = world.resource::<CascadeAnchors>().0.keys().copied().collect();
            k.sort_by_key(|&(l, v, i)| (l, v, i));
            k
        };
        assert_eq!(keys(&world), [(day, view, 0), (day, view, 1), (day, view, 2)]);
        // By night: one cascade over 3,000 units, a texel unlike the day's first.
        let moon_texel = 3.0 * TEXEL;
        world.entity_mut(day).insert(built(view, vec![cascade_of(sun(0.0), FAR, moon_texel, DEPTH)]));
        schedule.run(&mut world);
        assert_eq!(keys(&world), [(day, view, 0)]);
        let a = world.resource::<CascadeAnchors>().0[&(day, view, 0)];
        let reach = ANCHOR_REACH_TEXELS * f64::from(moon_texel);
        let centre = cascade_centre(&cascade_of(sun(0.0), FAR, moon_texel, DEPTH));
        assert_eq!(a, Anchor { at: (centre / reach).round() * reach, texel: moon_texel }, "a fresh anchor");
        // Faded at the horizon: no shadows, the cascades neither anchored nor given back by
        // the hold (shadow_rate.rs), no anchors.
        world.get_mut::<DirectionalLight>(day).unwrap().shadow_maps_enabled = false;
        let stale = cascade_of(sun(0.3), FAR, moon_texel, DEPTH);
        world.entity_mut(day).insert(built(view, vec![stale.clone()]));
        schedule.run(&mut world);
        assert!(keys(&world).is_empty());
        // (The stretch toward the sun still changes its depth; its place is untouched.)
        assert_eq!(world.get::<Cascades>(day).unwrap().cascades[&view][0].world_from_cascade, stale.world_from_cascade);
        // Another worldspace: the old sun gone, a new one with the day's cascades.
        world.despawn(day);
        let next = sun_light(&mut world, true);
        world.entity_mut(next).insert(built(view, vec![bevy_cascade(sun(0.0), FAR), bevy_cascade(sun(0.0), FAR)]));
        schedule.run(&mut world);
        assert_eq!(keys(&world), [(next, view, 0), (next, view, 1)]);
    }
}
