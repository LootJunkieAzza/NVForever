//! The material of NIF shapes: Bevy's StandardMaterial with one change in the fragment
//! shader (`nif_material.wgsl`): the emissive light is multiplied by the surface colour
//! (diffuse texture × vertex colour), as NIF emissive light is modulated by the texture
//! [hypothesis, docs/spec-notes/nif.md §6 "Emissive light"]. Shapes with a black emissive
//! colour render exactly as with StandardMaterial.

use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

/// The extension part: no data of its own, only the fragment shader.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct NifSurface {}

impl MaterialExtension for NifSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wgsl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wgsl".into()
    }
}

pub type NifMaterial = ExtendedMaterial<StandardMaterial, NifSurface>;

/// A NIF material from its StandardMaterial part.
pub fn nif_material(base: StandardMaterial) -> NifMaterial {
    NifMaterial { base, extension: NifSurface {} }
}

pub struct NifMaterialPlugin;

impl Plugin for NifMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "nif_material.wgsl");
        app.add_plugins(MaterialPlugin::<NifMaterial>::default());
    }
}
