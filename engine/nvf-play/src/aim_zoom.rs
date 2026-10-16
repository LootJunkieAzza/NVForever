//! The view zooms in while the player aims down the sights.
//!
//! - **What zooms**: Block held (`PlayerControl::aiming`) with a gun drawn
//!   (`WeaponAnim::ready`) whose WEAP DNAM Sight FOV is above 0 narrows the player
//!   camera's field of view to that Sight FOV [documented: GECK wiki Weapons, Sight FOV is
//!   the field of view down the sights, 0 "essentially disables iron sights";
//!   docs/spec-notes/combat-data.md §1.1]. The zoom is the weapon's own: a lower Sight FOV
//!   zooms in further (the owner's playtest, 2026-10-16: the Varmint Rifle zooms in more
//!   than the Sturdy Caravan Shotgun).
//! - **Units** [hypothesis]: the Sight FOV is read like the FOV setting, the horizontal
//!   angle of a 4:3 view with the vertical angle kept at other aspects
//!   ([`vertical_fov`], docs/spec-notes/camera.md §3).
//! - **Timing** [design]: the field of view moves between the two over [`ZOOM_SECONDS`],
//!   smoothed at both ends; letting go of Block zooms back out at the same pace.
//! - The free camera, menus and melee weapons (which block) do not zoom; both points of
//!   view do. Whatever else sets the field of view (the console's SetCameraFOV,
//!   chargen) sets the unzoomed angle: a change this module did not make is taken as
//!   the new base.

use bevy::prelude::*;

use crate::combat::{Combat, WeaponAnim};
use crate::control::PlayerControl;
use crate::free_cam::{vertical_fov, FreeCam};
use crate::{PlayState, PlayerCamera, PlaySet};

/// Seconds from the unzoomed view to the full zoom and back [design: not captured].
pub const ZOOM_SECONDS: f32 = 0.25;

/// How far the view is zoomed in, and the angles it moves between.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct AimZoom {
    /// 0 unzoomed, 1 the weapon's Sight FOV.
    pub blend: f32,
    /// The weapon's Sight FOV as a vertical angle (radians), held while zooming out.
    pub sight: Option<f32>,
    /// The unzoomed vertical field of view (radians).
    base: Option<f32>,
    /// The vertical field of view this module last set.
    applied: Option<f32>,
    /// The full zoom has been reported (smoke tests read the line).
    told: bool,
}

/// The field of view (vertical, radians) for `base`, the weapon's `sight` and `blend`
/// (0–1), eased in and out.
pub fn zoomed_fov(base: f32, sight: f32, blend: f32) -> f32 {
    let b = blend.clamp(0.0, 1.0);
    let s = b * b * (3.0 - 2.0 * b);
    base + (sight - base) * s
}

pub struct AimZoomPlugin;

impl Plugin for AimZoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimZoom>().add_systems(Update, zoom.in_set(PlaySet::Sync));
    }
}

#[allow(clippy::too_many_arguments)]
fn zoom(
    time: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    control: Res<PlayerControl>,
    anim: Res<WeaponAnim>,
    combat: Option<Res<Combat>>,
    free: Option<Res<FreeCam>>,
    mut z: ResMut<AimZoom>,
    mut cams: Query<&mut Projection, With<PlayerCamera>>,
) {
    let Ok(mut proj) = cams.single_mut() else { return };
    // Read first: a write every frame would mark the projection changed.
    let Projection::Perspective(p) = &*proj else { return };
    // Someone else set the field of view: that is the unzoomed angle now.
    if z.applied != Some(p.fov) {
        z.base = Some(p.fov);
        z.applied = None;
    }
    let playing = *state.get() == PlayState::Playing && !free.is_some_and(|f| f.on);
    let sight = combat.as_deref().and_then(Combat::sight_fov).filter(|_| playing && control.aiming && control.weapon_out && anim.ready);
    if let Some((_, deg)) = sight {
        z.sight = Some(vertical_fov(deg));
    }
    let step = time.delta_secs().min(0.25) / ZOOM_SECONDS;
    let was = z.blend;
    z.blend = if sight.is_some() { (z.blend + step).min(1.0) } else { (z.blend - step).max(0.0) };
    if was == 0.0 && z.blend == 0.0 {
        return;
    }
    let (Some(base), Some(target)) = (z.base, z.sight) else { return };
    let fov = zoomed_fov(base, target, z.blend);
    if let Projection::Perspective(p) = &mut *proj {
        p.fov = fov;
    }
    z.applied = Some(fov);
    if z.blend == 1.0 && !z.told {
        z.told = true;
        if let Some((w, deg)) = sight {
            let edid = combat.as_deref().and_then(|c| c.forms.weapons.get(&w)).and_then(|ws| ws.edid.as_deref()).unwrap_or("?");
            info!("aim zoom: {edid} ({w}) sight FOV {deg:.1}° (vertical {:.1}°, unzoomed {:.1}°)", target.to_degrees(), base.to_degrees());
        }
    } else if z.blend == 0.0 {
        z.told = false;
        z.sight = None;
        info!("aim zoom: back to the unzoomed view (vertical {:.1}°)", base.to_degrees());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zoom_eases_from_the_base_to_the_sights() {
        let (base, sight) = (vertical_fov(75.0), vertical_fov(55.0));
        assert_eq!(zoomed_fov(base, sight, 0.0), base);
        assert!((zoomed_fov(base, sight, 1.0) - sight).abs() < 1e-6);
        assert!((zoomed_fov(base, sight, 0.5) - (base + sight) / 2.0).abs() < 1e-6);
        // Slow at the ends: the first tenth covers less than a tenth of the way.
        assert!(base - zoomed_fov(base, sight, 0.1) < (base - sight) * 0.1);
    }

    #[test]
    fn a_lower_sight_fov_zooms_further() {
        assert!(vertical_fov(40.0) < vertical_fov(60.0));
    }
}
