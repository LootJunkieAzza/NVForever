//! View-angle falloff of BSShaderNoLightingProperty: light beams, glows, force fields and
//! dust sheets fade where their surface turns edge-on to the camera (docs/spec-notes/nif.md
//! §6, "View-angle falloff"). Shapes that use it get a [`FalloffMaterial`] (the standard
//! material plus falloff.wgsl) instead of a plain [`StandardMaterial`]; [`ShapeMaterial`]
//! holds either.

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub type FalloffMaterial = ExtendedMaterial<StandardMaterial, Falloff>;

/// Falloff parameters as stored: start angle and stop angle (cosines), start opacity and
/// stop opacity [documented: nif.xml BSShaderNoLightingProperty].
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Copy, PartialEq)]
pub struct Falloff {
    #[uniform(100)]
    pub params: Vec4,
}

impl MaterialExtension for Falloff {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/falloff.wgsl".into()
    }
}

impl Falloff {
    /// The falloff to draw for the stored fields, or `None` when it changes nothing or
    /// is degenerate. A start angle equal to the stop angle gives no range to interpolate
    /// over; 166 shapes store all four fields as 0 (e.g. the vertex-coloured quads of
    /// `offpitwall01.nif`), which read literally would hide them entirely [hypothesis: the
    /// game skips the falloff for an empty range]. Constant opacity 1 is no falloff.
    pub fn from_fields(start_angle: f32, stop_angle: f32, start_opacity: f32, stop_opacity: f32) -> Option<Falloff> {
        let p = Vec4::new(start_angle, stop_angle, start_opacity, stop_opacity);
        if !p.is_finite() || (start_angle - stop_angle).abs() < 1e-4 || (start_opacity == 1.0 && stop_opacity == 1.0) {
            return None;
        }
        Some(Falloff { params: p })
    }

    /// Opacity at the cosine `c` of the view angle: linear in the cosine between the
    /// start and stop angles, held beyond them (CPU mirror of falloff.wgsl).
    pub fn opacity(&self, c: f32) -> f32 {
        let p = self.params;
        let t = ((c - p.y) / (p.x - p.y)).clamp(0.0, 1.0);
        p.w + (p.z - p.w) * t
    }
}

/// A shape's material: standard, or standard with view-angle falloff.
#[derive(Debug, Clone)]
pub enum ShapeMaterial {
    Standard(Handle<StandardMaterial>),
    Falloff(Handle<FalloffMaterial>),
}

impl ShapeMaterial {
    /// Add the material component to an entity.
    pub fn insert(&self, e: &mut EntityCommands) {
        match self {
            ShapeMaterial::Standard(h) => e.insert(MeshMaterial3d(h.clone())),
            ShapeMaterial::Falloff(h) => e.insert(MeshMaterial3d(h.clone())),
        };
    }

    /// The standard material, when there is no falloff.
    pub fn standard(&self) -> Option<&Handle<StandardMaterial>> {
        match self {
            ShapeMaterial::Standard(h) => Some(h),
            ShapeMaterial::Falloff(_) => None,
        }
    }
}

pub struct FalloffPlugin;

impl Plugin for FalloffPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "falloff.wgsl");
        app.add_plugins(MaterialPlugin::<FalloffMaterial>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_fields() {
        // fxlightbeambrtw02.nif: opaque facing the camera (within 10°), gone past 80°.
        let beam = Falloff::from_fields(0.985, 0.174, 1.0, 0.0).unwrap();
        assert_eq!(beam.opacity(1.0), 1.0);
        assert_eq!(beam.opacity(0.0), 0.0);
        assert!((beam.opacity((0.985 + 0.174) / 2.0) - 0.5).abs() < 1e-5);
        // Reversed ranges fade in towards the edges (rim glows).
        let rim = Falloff::from_fields(0.866, 0.174, 0.0, 1.0).unwrap();
        assert_eq!(rim.opacity(1.0), 0.0);
        assert_eq!(rim.opacity(0.1), 1.0);
        // The nif.xml defaults are a real falloff: opacity equals the cosine.
        assert!((Falloff::from_fields(1.0, 0.0, 1.0, 0.0).unwrap().opacity(0.3) - 0.3).abs() < 1e-6);
        // Empty ranges and constant full opacity draw without falloff.
        assert_eq!(Falloff::from_fields(0.0, 0.0, 0.0, 0.0), None);
        assert_eq!(Falloff::from_fields(1.0, 0.0, 1.0, 1.0), None);
        assert_eq!(Falloff::from_fields(f32::NAN, 0.0, 1.0, 0.0), None);
    }
}
