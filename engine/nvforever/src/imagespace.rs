//! The image-space grade as a full-screen pass after tonemapping: the worldspace's base
//! image space with the weather's adapters for the time of day (fnv_world::imagespace,
//! docs/spec-notes/world.md §6).
//!
//! Formula [hypothesis: parameter names from the records; order and the use of display
//! (sRGB) values follow the classic image-space pass, to compare with screenshots]:
//! saturation around the luminance, then a tint towards luminance × tint colour by the
//! tint amount, then brightness as a factor, then contrast around the average luminance.

use crate::sky::SkyState;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::Core3dSystems;
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

/// Grade parameters on the main camera.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
pub struct ImageSpaceGrade {
    /// Tint colour (rgb) and amount (a).
    pub tint: Vec4,
    pub saturation: f32,
    pub contrast: f32,
    pub contrast_avg_lum: f32,
    pub brightness: f32,
}

impl Default for ImageSpaceGrade {
    fn default() -> Self {
        ImageSpaceGrade { tint: Vec4::new(1.0, 1.0, 1.0, 0.0), saturation: 1.0, contrast: 1.0, contrast_avg_lum: 0.5, brightness: 1.0 }
    }
}

impl FullscreenMaterial for ImageSpaceGrade {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvforever/imagespace.wgsl".into()
    }

    /// After tonemapping: the game grades the final display colours [hypothesis].
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system.in_set(Core3dSystems::PostProcess).after(tonemapping)
    }
}

pub struct ImageSpacePlugin;

impl Plugin for ImageSpacePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "imagespace.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<ImageSpaceGrade>::default())
            .add_systems(Update, update_grade.after(crate::sky::update_sky));
    }
}

/// Copy the sky's grade onto every graded camera.
fn update_grade(sky: Res<SkyState>, mut grades: Query<&mut ImageSpaceGrade>) {
    let c = sky.cinematic();
    let g = ImageSpaceGrade {
        tint: Vec4::new(c.tint[0], c.tint[1], c.tint[2], c.tint_amount),
        saturation: c.saturation,
        contrast: c.contrast,
        contrast_avg_lum: c.contrast_avg_lum,
        brightness: c.brightness,
    };
    for mut grade in &mut grades {
        *grade = g;
    }
}
