//! Eye adaptation (docs/spec-notes/world.md §6 "Eye adaptation"). The original's HDR
//! pass lets "the adapted scene lum chase the actual lum" toward the image space's Target
//! LUM [documented: GECK wiki ImageSpace, HDR tab]; how it maps one to the other is not
//! documented. Against 31 original frames (the Mojave by day, the lab's interiors, the
//! effects audit's night spots and Dead Money), pictures whose average is above about
//! −7 EV match ours without it, and darker ones are brightened by most of the way back
//! [observed, world.md §6]. The renderer meters the HDR view with Bevy's auto exposure and
//! brightens only: lift = [`LIFT_SLOPE`] × (reference − average log2 luminance), never
//! below 0 or above [`MAX_LIFT`], the reference being [`REFERENCE_EV`] + log2(Target LUM /
//! 1.4) [hypothesis, fitted]. It needs the HDR view, so it goes with the light glow.

use bevy::math::cubic_splines::LinearSpline;
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve, AutoExposurePlugin};
use bevy::prelude::*;
use fnv_world::imagespace::HDR_TARGET_LUM;

/// The average log2 luminance below which a picture is brightened, at Target LUM 1.4
/// (NVDefaultExterior) [hypothesis, fitted to the frames].
pub const REFERENCE_EV: f32 = -6.8;
/// EV of brightening per EV the average lies under the reference [hypothesis, fitted].
pub const LIFT_SLOPE: f32 = 0.85;
/// The most a picture is brightened, in EV [engineering choice: a black screen stays dark].
pub const MAX_LIFT: f32 = 5.0;
/// Target LUM of NVDefaultExterior, where [`REFERENCE_EV`] was fitted [observed].
const BASE_TARGET_LUM: f32 = 1.4;
/// The metered range of log2 luminance [engineering choice: the darkest frames average
/// about −12 EV, sunlit sand about 0].
const METER_LOW: f32 = -16.0;
const METER_HIGH: f32 = 6.0;

pub struct EyeAdaptPlugin;

impl Plugin for EyeAdaptPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(AutoExposurePlugin).init_resource::<EyeCurve>().add_systems(PostUpdate, follow_target_lum);
    }
}

/// The compensation curve every adapting camera shares, and the Target LUM it was made for.
#[derive(Resource, Default)]
struct EyeCurve {
    handle: Handle<AutoExposureCompensationCurve>,
    target: f32,
}

/// The reference average (log2 luminance) for an image space's Target LUM; a missing or
/// non-positive value counts as the base one.
pub fn reference_ev(target_lum: f32) -> f32 {
    let t = if target_lum > 0.0 { target_lum } else { BASE_TARGET_LUM };
    REFERENCE_EV + (t / BASE_TARGET_LUM).log2()
}

/// How many EV a picture of average log2 luminance `avg` is brightened.
pub fn lift(avg: f32, target_lum: f32) -> f32 {
    (LIFT_SLOPE * (reference_ev(target_lum) - avg)).clamp(0.0, MAX_LIFT)
}

/// Bevy's curve: for an average `x` the target exposure is `y − x`, so `y = x + lift(x)`.
/// Corners: the meter's ends, the reference, and where the lift reaches its cap.
fn curve(target_lum: f32) -> AutoExposureCompensationCurve {
    let r = reference_ev(target_lum);
    let cap = r - MAX_LIFT / LIFT_SLOPE;
    let mut xs = vec![METER_LOW, cap, r, METER_HIGH];
    xs.retain(|x| (METER_LOW..=METER_HIGH).contains(x));
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let points: Vec<Vec2> = xs.into_iter().map(|x| Vec2::new(x, x + lift(x, target_lum))).collect();
    AutoExposureCompensationCurve::from_curve(LinearSpline::new(points)).unwrap_or_default()
}

/// The camera component for an adapting view (inserted with the light glow, graphics.rs).
pub fn auto_exposure(handle: Handle<AutoExposureCompensationCurve>) -> AutoExposure {
    AutoExposure {
        range: METER_LOW..=METER_HIGH,
        // Brightening as quickly as darkening, both within a couple of seconds
        // [engineering choice; the image space's Eye Adapt Speed is not used].
        speed_brighten: 3.0,
        speed_darken: 3.0,
        compensation_curve: handle,
        ..default()
    }
}

/// Keep the curve on the image space's Target LUM, and give new HDR cameras the adaptation.
fn follow_target_lum(
    mut commands: Commands,
    sky: Option<Res<crate::sky::SkyState>>,
    mut eye: ResMut<EyeCurve>,
    mut curves: ResMut<Assets<AutoExposureCompensationCurve>>,
    cams: Query<(Entity, Option<&AutoExposure>), With<crate::post::ImageSpaceGlow>>,
    bare: Query<Entity, (With<AutoExposure>, Without<crate::post::ImageSpaceGlow>)>,
) {
    let target = sky.map_or(BASE_TARGET_LUM, |s| s.cinematic().hdr.v[HDR_TARGET_LUM]);
    if eye.handle == Handle::default() {
        eye.handle = curves.add(curve(target));
        eye.target = target;
        info!("eye adaptation: brightens pictures averaging under {:.1} EV (Target LUM {target:.2})", reference_ev(target));
    } else if (eye.target - target).abs() > 1e-3 {
        eye.target = target;
        let _ = curves.insert(&eye.handle, curve(target));
        info!("eye adaptation: Target LUM {target:.2}, brightens under {:.1} EV", reference_ev(target));
    }
    for (e, a) in &cams {
        if a.is_none() {
            commands.entity(e).insert(auto_exposure(eye.handle.clone()));
        }
    }
    // The light glow switched off: the view is no longer HDR.
    for e in &bare {
        commands.entity(e).remove::<AutoExposure>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifts_only_dark_pictures() {
        // A Mojave day (about −3 EV) is left alone; a Dead Money night (−9 to −12) is lifted.
        assert_eq!(lift(-3.0, 1.4), 0.0);
        assert!((lift(-9.12, 1.4) - 0.85 * 2.32).abs() < 1e-4);
        assert_eq!(lift(-20.0, 1.4), MAX_LIFT);
        // Puesta del Sol's adapter triples Target LUM: the reference rises by log2 3.
        assert!((reference_ev(4.2) - (REFERENCE_EV + 3f32.log2())).abs() < 1e-5);
        // An image space without HDR values counts as the base one.
        assert_eq!(reference_ev(0.0), REFERENCE_EV);
    }

    #[test]
    fn curve_builds() {
        let c = curve(1.4);
        let d = format!("{c:?}");
        assert!(d.contains("min_log_lum: -16.0"), "{d}");
    }
}
