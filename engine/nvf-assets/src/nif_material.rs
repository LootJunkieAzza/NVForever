//! The material of NIF shapes: Bevy's StandardMaterial with changes in the fragment shader
//! (`nif_material.wgsl`):
//! - the emissive light is multiplied by the surface colour (diffuse texture × vertex
//!   colour), as NIF emissive light is modulated by the texture [hypothesis,
//!   docs/spec-notes/nif.md §6 "Emissive light"];
//! - the view-angle falloff of BSShaderNoLightingProperty scales the alpha, so light
//!   beams, glows, force fields and dust sheets fade where they turn edge-on to the camera
//!   (docs/spec-notes/nif.md §6, "View-angle falloff");
//! - the colour of a multiply-blended shape (tinted glass, shadow overlays) is a factor on
//!   what lies behind it: its lit colour and emissive light are summed in display values
//!   and held to 0..1, without fog or tonemapping ([`multiply_factor`],
//!   docs/spec-notes/nif.md §6, "Multiply blend").
//!
//! And one change in the pipeline: shapes with a decal shader flag are drawn with a depth
//! offset towards the camera ([`decal_bias`], docs/spec-notes/nif.md §6, "Decal flags").
//!
//! Opaque, alpha-tested and alpha-blended shapes with a black emissive colour, no falloff
//! and no decal flag render exactly as with StandardMaterial.
//!
//! Crossfades: meshes with a crossfading `VisibilityRange` (the streamed cells' fade in
//! and out, nvforever `fade.rs`) are dithered in every pass. Bevy's depth-only prepass
//! skips the dither, so the pixels the main pass discards would still hold depth there and
//! hide what lies behind them; [`fade_prepass`] gives that prepass a fragment that dithers.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline, MaterialPlugin, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, DepthBiasState, FragmentState,RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::{ShaderDefVal, ShaderRef};

/// fade_prepass.wgsl.
const FADE_PREPASS_SHADER: Handle<Shader> = uuid_handle!("6f0c9b51-2a7e-4f43-9d0e-5b8c1e7a4d21");

/// Make a depth-only prepass pipeline of a crossfading mesh dither as the main pass does
/// (module docs). Call from `MaterialExtension::specialize` of materials whose meshes
/// can fade; other pipelines are left alone.
pub fn fade_prepass(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::VISIBILITY_RANGE_DITHER) {
        return;
    }
    let defs = &descriptor.vertex.shader_defs;
    let has = |d: &str| defs.contains(&ShaderDefVal::from(d));
    // Prepasses with colour targets (or unclipped depth) run Bevy's full prepass
    // fragment, which already dithers.
    if !has("PREPASS_PIPELINE") || has("PREPASS_FRAGMENT") {
        return;
    }
    let (shader_defs, targets) = match descriptor.fragment.take() {
        Some(f) => (f.shader_defs, f.targets),
        None => (defs.clone(), Vec::new()),
    };
    descriptor.fragment = Some(FragmentState { shader: FADE_PREPASS_SHADER, shader_defs, targets, ..default() });
}

/// The extension part: the falloff parameters, the decal switch and the fragment shader.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(NifSurfaceKey)]
pub struct NifSurface {
    /// Start angle, stop angle (cosines), start opacity, stop opacity; constant opacity 1
    /// when the shape has no falloff ([`Falloff::NONE`]).
    #[uniform(100)]
    pub falloff: Vec4,
    /// The shape lies on another surface (a decal shader flag is set): drawn with
    /// [`decal_bias`].
    pub decal: bool,
}

impl Default for NifSurface {
    fn default() -> Self {
        NifSurface { falloff: Falloff::NONE.params, decal: false }
    }
}

/// The part of [`NifSurface`] that selects the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NifSurfaceKey {
    pub decal: bool,
}

impl From<&NifSurface> for NifSurfaceKey {
    fn from(surface: &NifSurface) -> Self {
        NifSurfaceKey { decal: surface.decal }
    }
}

impl MaterialExtension for NifSurface {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wgsl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "embedded://nvf_assets/nif_material.wgsl".into()
    }

    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        fade_prepass(descriptor, key.mesh_key);
        // StandardMaterial has set the rasteriser's bias from the material's depth bias.
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias.constant = raster_bias(depth_stencil.bias.constant);
            if key.bind_group_data.decal {
                depth_stencil.bias = decal_bias(depth_stencil.bias);
            }
        }
        Ok(())
    }
}

/// Sort-only part of the depth bias of blended meshes that are drawn before every other
/// blended mesh of the world ([`sort_first`]). Bevy sorts blended meshes by view distance
/// plus the material's depth bias, ascending, and draws them in that order
/// [documented: bevy_core_pipeline 0.19 Transparent3d]; the sky uses −1e6 (nvforever
/// `sky.rs`), so these come after the sky and before the rest.
pub const SORT_FIRST_BIAS: f32 = -5.0e5;

/// The material depth bias of a blended mesh that lies on an opaque surface and is drawn
/// before every other blended mesh, with `bias` as its bias in the depth buffer (terrain
/// texture layers). Blended meshes write no depth: one drawn before such a layer would be
/// painted over by it wherever the layer sorts as the nearer of the two.
pub fn sort_first(bias: f32) -> f32 {
    SORT_FIRST_BIAS + bias
}

/// The rasteriser's depth bias for a material depth bias: Bevy uses one number for the
/// draw order and for the depth buffer, so the sort-only part of [`sort_first`] is taken
/// out again.
pub fn raster_bias(constant: i32) -> i32 {
    if constant <= (SORT_FIRST_BIAS / 2.0) as i32 {
        constant - SORT_FIRST_BIAS as i32
    } else {
        constant
    }
}

/// Constant part of [`decal_bias`], in steps of the depth buffer at the shape's depth
/// (2⁻²³ of the depth value for the 32-bit float buffer). With depth = near / distance
/// that is at most 64 · 2⁻²³ ≈ 7.6e-6 of the distance to the camera: 0.008 units at
/// 1,000 units, far less than the 0.05 to 1 unit the decals are lifted off their surface.
pub const DECAL_BIAS: i32 = 64;

/// Slope part of [`decal_bias`]: one pixel's change of depth across the triangle, which
/// covers the difference between a decal and a surface triangulated differently.
pub const DECAL_SLOPE_BIAS: f32 = 1.0;

/// The rasteriser's depth bias for a shape with a decal shader flag, added to `bias`: a
/// small offset towards the camera (depth is reversed, so a positive bias is nearer), so
/// that the decal wins the depth test against the surface it lies on whichever is drawn
/// first [hypothesis, docs/spec-notes/nif.md §6 "Decal flags"]. Only the depth buffer is
/// concerned: the draw order of blended decals stays with their distance.
pub fn decal_bias(bias: DepthBiasState) -> DepthBiasState {
    DepthBiasState { constant: bias.constant + DECAL_BIAS, slope_scale: bias.slope_scale + DECAL_SLOPE_BIAS, clamp: bias.clamp }
}

pub type NifMaterial = ExtendedMaterial<StandardMaterial, NifSurface>;

/// A NIF material from its StandardMaterial part.
pub fn nif_material(base: StandardMaterial) -> NifMaterial {
    NifMaterial { base, extension: NifSurface::default() }
}

/// View-angle falloff as stored in BSShaderNoLightingProperty: start angle and stop
/// angle (cosines), start opacity and stop opacity [documented: nif.xml].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Falloff {
    pub params: Vec4,
}

impl Falloff {
    /// No falloff: opacity 1 at every angle.
    pub const NONE: Falloff = Falloff { params: Vec4::new(1.0, 0.0, 1.0, 1.0) };

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
    /// start and stop angles, held beyond them (CPU mirror of nif_material.wgsl).
    pub fn opacity(&self, c: f32) -> f32 {
        let p = self.params;
        let t = ((c - p.y) / (p.x - p.y)).clamp(0.0, 1.0);
        p.w + (p.z - p.w) * t
    }
}

/// Display gamma of [`multiply_factor`] and [`multiply_emissive`]: a plain power law, so a
/// product of display values is the product of the linear values.
pub const MULTIPLY_GAMMA: f32 = 2.2;

/// The factor a multiply-blended shape (NiAlphaProperty source factor ZERO, destination
/// factor SRC_COLOR: result = destination × source colour [documented: nif.xml
/// AlphaFlags]) puts on what lies behind it, for the shape's lit colour and emissive
/// light, both linear (CPU mirror of nif_material.wgsl).
///
/// The game's shader output is taken as texture × (light + emissive colour × emissive
/// multiplier) in display values, held to 0..1 when written to the frame [hypothesis,
/// docs/spec-notes/nif.md §6 "Multiply blend"]. So both parts are summed as display
/// values and the sum is limited: a multiply can only darken.
pub fn multiply_factor(lit: Vec3, emissive: Vec3) -> Vec3 {
    let display = lit.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA) + emissive.max(Vec3::ZERO).powf(1.0 / MULTIPLY_GAMMA);
    display.clamp(Vec3::ZERO, Vec3::ONE).powf(MULTIPLY_GAMMA)
}

/// The linear emissive light of a multiply-blended shape for a NiMaterialProperty emissive
/// colour × multiplier `value`: `value` raised to the display gamma. The shader multiplies
/// it by the linear texture colour, and [`multiply_factor`] takes the product back to
/// display values, which gives texture (display) × `value`, the game's term [hypothesis].
pub fn multiply_emissive(value: f32) -> f32 {
    value.max(0.0).powf(MULTIPLY_GAMMA)
}

pub struct NifMaterialPlugin;

impl Plugin for NifMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "nif_material.wgsl");
        load_internal_asset!(app, FADE_PREPASS_SHADER, "fade_prepass.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<NifMaterial>::default());
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
        // The default parameters keep full opacity at every angle.
        for c in [0.0, 0.5, 1.0] {
            assert_eq!(Falloff::NONE.opacity(c), 1.0);
        }
    }

    #[test]
    fn sort_first_bias() {
        // Terrain layers: drawn before a blended mesh thousands of units farther away
        // (Bevy sorts by distance + bias, ascending), in rank order among themselves, and
        // with their plain bias in the depth buffer.
        for rank in 0..8 {
            let bias = 2.0 * (rank as f32 + 1.0);
            let m = sort_first(bias);
            assert!(m + 400_000.0 < 0.0);
            assert!(m > -1.0e6 + 100.0, "after the sky");
            assert!(sort_first(bias + 2.0) > m);
            assert_eq!(raster_bias(m as i32), bias as i32);
        }
        // Other materials keep their bias (LOD meshes use −50).
        for b in [0, 2, 16, -50, -1000] {
            assert_eq!(raster_bias(b), b);
        }
    }

    #[test]
    fn decal_depth_bias() {
        // A decal is nearer in the depth buffer than a plain shape (reversed depth:
        // positive is nearer), by a constant and by the slope of its triangle.
        let plain = DepthBiasState::default();
        let decal = decal_bias(plain);
        assert!(decal.constant > plain.constant && decal.slope_scale > plain.slope_scale);
        assert_eq!(decal.clamp, plain.clamp);
        // The constant part stays far below the 0.05 units the nearest decals are lifted
        // off their surface: its share of the distance to the camera, at 5,000 units.
        let share = DECAL_BIAS as f32 / (1u32 << 23) as f32;
        assert!(share * 5000.0 < 0.05);
        // It adds to a bias the material already has, and is not a sort-first bias.
        let lod = decal_bias(DepthBiasState { constant: -50, ..default() });
        assert_eq!(lod.constant, DECAL_BIAS - 50);
        assert_eq!(raster_bias(decal.constant), decal.constant);
        // Only the decal switch selects the pipeline; the falloff is a uniform.
        let mut surface = NifSurface::default();
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: false });
        surface.falloff = Vec4::new(0.9, 0.2, 1.0, 0.0);
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey::from(&NifSurface::default()));
        surface.decal = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true });
    }

    #[test]
    fn multiply_blend_factor() {
        let display = |v: Vec3| v.powf(1.0 / MULTIPLY_GAMMA);
        let close = |a: Vec3, b: [f32; 3]| (a - Vec3::from_array(b)).abs().max_element() < 0.005;
        // Rex's skull cap (rex_skullcap.nif `##SkullCap:0`): a flat texture of sRGB
        // (49, 87, 117), emissive colour 0.19 × multiplier 12. In the dark it leaves what
        // lies behind at texture × 2.28 in display values, blue held at 1: a light blue
        // glass, not black.
        let texture = Vec3::new(49.0, 87.0, 117.0) / 255.0;
        let emissive = texture.powf(MULTIPLY_GAMMA) * multiply_emissive(0.190_283_18 * 12.0);
        assert!(close(display(multiply_factor(Vec3::ZERO, emissive)), [0.439, 0.779, 1.0]));
        // Light on the cap adds in display values and cannot pass 1.
        let lit = Vec3::splat(0.25f32.powf(MULTIPLY_GAMMA));
        assert!(close(display(multiply_factor(lit, emissive)), [0.689, 1.0, 1.0]));
        // Without emissive light the factor is the shape's colour: an unlit shadow overlay
        // darkens by its vertex colour, white changes nothing and black hides.
        assert!(close(multiply_factor(Vec3::splat(0.5), Vec3::ZERO), [0.5; 3]));
        assert!(close(multiply_factor(Vec3::ONE, Vec3::ZERO), [1.0; 3]));
        assert!(close(multiply_factor(Vec3::splat(4.0), Vec3::ZERO), [1.0; 3]));
        assert!(close(multiply_factor(Vec3::ZERO, Vec3::ZERO), [0.0; 3]));
        // An emissive value of 1 is 1 in both spaces; larger ones grow by the gamma.
        assert_eq!(multiply_emissive(1.0), 1.0);
        assert!((multiply_emissive(2.0) - 2f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(multiply_emissive(-1.0), 0.0);
    }
}
