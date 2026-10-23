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
//!   docs/spec-notes/nif.md §6, "Multiply blend");
//! - an additive shape (light beams, glows, flames) adds about what it would add to
//!   display values: its alpha is raised to a power ([`add_alpha`], nif.md §6
//!   "Additive blend");
//! - an unlit alpha-blended shape (smoke, mist, dust, particles) puts about what it would
//!   put on display values: its colour is scaled by √alpha ([`blend_colour_scale`],
//!   nif.md §6 "Alpha blend of unlit shapes").
//!
//! And one change in the pipeline: shapes with a decal shader flag are drawn with a depth
//! offset towards the camera ([`decal_bias`], docs/spec-notes/nif.md §6, "Decal flags").
//!
//! Opaque, alpha-tested and alpha-blended shapes with a black emissive colour, no falloff
//! and no decal flag render exactly as with StandardMaterial.
//!
//! Soft intersections: an unlit effect shape that adds or blends (light beams, glows,
//! smoke and dust sheets, particles; [`soft_intersections`]) fades out over
//! [`SOFT_DISTANCE`] towards the opaque surface behind it (the view's depth prepass), so
//! a beam's panels end softly where they pass into a road or a wall instead of along a
//! hard line, the way they blend in the game's frames [engineering choice: the game's
//! own rule is not known; docs/spec-notes/nif.md §6 "Soft intersections"]. Where the
//! shape lies along that surface (its facing within about 25° of the surface's, both
//! taken from screen derivatives) it is not faded, so light pools and grime cards laid on
//! a floor or wall stay. One rule for every such shape, not a setting per model.
//!
//! Soft edges: where the view is multisampled, the cut-out edges of alpha-tested shapes
//! (plants, fences, hair) are anti-aliased ([`soft_edges`]), as the game's transparency
//! multisampling does [observed: the owner's `FalloutPrefs.ini` has `iMultiSample=8` and
//! `bTransparencyMultisampling=1`; docs/spec-notes/nif.md §6 "Alpha"].
//!
//! Crossfades: meshes with a crossfading `VisibilityRange` (the streamed cells' fade in
//! and out, nvforever `fade.rs`) are dithered in every pass. Bevy's depth-only prepass
//! skips the dither, so the pixels the main pass discards would still hold depth there and
//! hide what lies behind them; [`fade_prepass`] gives that prepass a fragment that dithers.
//!
//! Vertex alpha: the main pass tests material alpha × texture alpha × vertex alpha, while
//! Bevy's depth-only prepass leaves the vertex alpha out. NIF shapes get a prepass
//! fragment that tests what the main pass tests ([`nif_prepass`], `nif_prepass.wgsl`), for
//! the same reason as the dither.
//!
//! Blend and test together: a blended shape can carry the threshold of its alpha test as
//! a cut ([`NifSurface::alpha_cut`]): fragments below it are dropped, the rest blend
//! (docs/spec-notes/nif.md §6 "Alpha", "Vertex alpha and the test").
//!
//! Refraction: a shape with the Refraction shader flag (`nif_spawn::refracts`: explosions'
//! heat-haze shells, Mister Handy's jet) is not drawn as a surface; it shows what lies
//! behind it, shifted on the screen by its normal (normal map applied) in view space times
//! its refraction strength ([`NifSurface::refraction`], which the model's
//! `BSRefractionStrengthController` animates) [hypothesis, docs/spec-notes/
//! explosion-effects.md §7]. It is drawn after the opaque pass from Bevy's copy of it (the
//! view transmission texture, as the water is), writes no depth, so the flames and smoke
//! inside a fireball's shell still show, and draws nothing into the depth-only passes
//! (shadows). Particles and other blended shapes are drawn later and are not bent.
//!
//! Fire refraction: a refracting shape with the Fire Refraction flag as well
//! (`nif_spawn::fire_period`) moves its bend: the normal map is read a second time,
//! scrolled up its texture once every [`NifSurface::fire_period`] seconds, and that
//! tilt is added to the shift [hypothesis, docs/spec-notes/refraction.md §3]. The
//! Stealth Boy's field (nvforever `stealth_field.rs`) uses the same to make an actor's
//! body shimmer. A refracting shape keeps the outline of its alpha cut
//! ([`NifSurface::alpha_cut`]).
//!
//! Holes: a shape can be eaten away by a holes texture (an effect shader's dissolve,
//! nvforever `effect_shaders.rs`, docs/spec-notes/effect-shaders.md §7): where the
//! texture's alpha (0–255, read at the shape's own texture coordinates) is at or above
//! [`NifSurface::holes`]`.x` nothing is drawn, in every pass, and the band of `.y` units
//! below it is drawn in [`NifSurface::holes_edge`]; `.x` negative (every other shape): no
//! holes. A shape with holes is drawn alpha-tested, so the depth-only passes run the
//! same test (`nif_prepass.wgsl`).
//!
//! Shade light: a shape can take the sun's shadows from another light's shadow map
//! instead of the sun's own ([`NifSurface::shade_light`]). That light is a second
//! directional light with shadows, pointing as the sun does, so its shadow map holds the
//! sun's shadows of the things on its own render layers. It carries a small share of the
//! sun's light (any share; the shader reads it from the two lights' colours), which is
//! what lets the shader tell the sun's part of a fragment's light from the rest using
//! Bevy's lighting alone (`with_shade_light` in the shader). For the player's
//! first-person arms, which the unseen body that casts the player's shadow stands in the
//! middle of [engineering choice, nvforever `player_body.rs`]. Every other shape is lit
//! by Bevy's lighting alone.
//!
//! FaceGen tint: an NPC's face and skin shapes carry the `.egt` colour offset of its
//! texture coefficients ([`NifSurface::facegen_offset`], made by nvforever `facegen.rs`)
//! and the shader adds it to the diffuse texture as the CPU rule does
//! (`fnv_actor::facegen::add_offset`, docs/spec-notes/actors.md §4): the offset image,
//! upsampled bilinearly over the texture and flipped vertically (its first row is the
//! texture's bottom row), is added to the texel's stored sRGB value (0–255) and held to
//! 0–255, then the sum goes back to linear [hypothesis, as the CPU rule]. It differs from
//! adding to the stored bytes only in that the texture is filtered first: no rounding to
//! whole units, and the hold to 0–255 comes after filtering, not per texel
//! [engineering choice: a small per-NPC offset image in place of a full-size texture per
//! NPC, docs/design/streaming-hitches.md §3]. Every other shape has no offset and the
//! shader is unchanged (the `FACEGEN_TINT` shader def).

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline, MaterialPlugin, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, CompareFunction, DepthBiasState, FragmentState,RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::{ShaderDefVal, ShaderRef};

/// fade_prepass.wgsl.
const FADE_PREPASS_SHADER: Handle<Shader> = uuid_handle!("6f0c9b51-2a7e-4f43-9d0e-5b8c1e7a4d21");

/// nif_prepass.wgsl.
const NIF_PREPASS_SHADER: Handle<Shader> = uuid_handle!("b1d6a2f4-7c35-4e0a-8f29-3a5d9c0e6b17");

/// Make a depth-only prepass pipeline of a crossfading mesh dither as the main pass does
/// (module docs). Call from `MaterialExtension::specialize` of materials whose meshes
/// can fade; other pipelines are left alone.
pub fn fade_prepass(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::VISIBILITY_RANGE_DITHER) {
        return;
    }
    depth_prepass_fragment(descriptor, FADE_PREPASS_SHADER);
}

/// The depth-only prepass fragment of NIF shapes (module docs, "Vertex alpha"): the
/// dither of [`fade_prepass`] and the main pass's alpha test, vertex alpha and
/// [`NifSurface::alpha_cut`] included. Pipelines that neither dither nor discard keep
/// having no fragment stage.
fn nif_prepass(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.intersects(MeshPipelineKey::VISIBILITY_RANGE_DITHER | MeshPipelineKey::MAY_DISCARD) {
        return;
    }
    depth_prepass_fragment(descriptor, NIF_PREPASS_SHADER);
}

/// Give a depth-only prepass pipeline (the shadow passes included) `shader` as its
/// fragment stage; other pipelines are left alone.
fn depth_prepass_fragment(descriptor: &mut RenderPipelineDescriptor, shader: Handle<Shader>) {
    let defs = &descriptor.vertex.shader_defs;
    let has = |d: &str| defs.contains(&ShaderDefVal::from(d));
    // Prepasses with colour targets (or unclipped depth) run Bevy's full prepass
    // fragment, which already dithers (and tests without the vertex alpha: the
    // renderers use no such prepass).
    if !has("PREPASS_PIPELINE") || has("PREPASS_FRAGMENT") {
        return;
    }
    let (shader_defs, targets) = match descriptor.fragment.take() {
        Some(f) => (f.shader_defs, f.targets),
        None => (defs.clone(), Vec::new()),
    };
    descriptor.fragment = Some(FragmentState { shader, shader_defs, targets, ..default() });
}

/// Anti-alias the cut-out edges of an alpha-tested material in the main pass of a
/// multisampled view: the fragment shader gets `SOFT_EDGES` and must write, as its alpha,
/// the share of the pixel's samples it covers (alpha to coverage). Call from
/// `MaterialExtension::specialize`; pipelines of other materials and passes, and views
/// without multisampling, are left alone (the alpha is tested per pixel there).
///
/// The share the shaders write is 1 at and above the material's cutoff and falls to 0
/// within one pixel outside it: `(alpha - cutoff) / fwidth(alpha) + 1`. The depth prepass
/// tests at the cutoff, so every pixel it has written depth for is fully covered and
/// nothing behind the shape is wrongly hidden [chosen: Bevy's own alpha-to-coverage mode
/// centres the edge on the cutoff and drops the prepass's alpha test].
pub fn soft_edges(descriptor: &mut RenderPipelineDescriptor, mesh_key: MeshPipelineKey) {
    if !mesh_key.contains(MeshPipelineKey::MAY_DISCARD) || descriptor.multisample.count <= 1 {
        return;
    }
    if descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
        return;
    }
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push("SOFT_EDGES".into());
        descriptor.multisample.alpha_to_coverage_enabled = true;
    }
}

/// The extension part: the falloff parameters, the alpha cut, the decal switch and the
/// fragment shader.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(NifSurfaceKey)]
pub struct NifSurface {
    /// Start angle, stop angle (cosines), start opacity, stop opacity; constant opacity 1
    /// when the shape has no falloff ([`Falloff::NONE`]).
    #[uniform(100)]
    pub falloff: Vec4,
    /// Fragments whose alpha is below this are not drawn: the alpha test of a shape that
    /// is drawn blended (`nif_spawn::blend_cut`); 0 on every other shape, which drops
    /// nothing.
    #[uniform(100)]
    pub alpha_cut: f32,
    /// 1 for an additive shape whose source factor is ONE (NiAlphaProperty ONE / ONE): its
    /// colour is added whole, whatever its alpha [documented: nif.xml AlphaFlags, source
    /// blend mode 0 ONE]; the view-angle falloff still fades it [hypothesis]. 0 for every
    /// other shape (an additive SRC_ALPHA / ONE shape adds colour × [`add_alpha`]).
    #[uniform(100)]
    pub add_whole: f32,
    /// The screen shift of a refracting shape per unit of its view-space normal (module
    /// notes, "Refraction"), in screen UV; 0 for every other shape.
    #[uniform(100)]
    pub refraction: f32,
    /// Seconds for the normal map of a refracting shape to scroll once up its texture,
    /// negative for down (module notes, "Fire refraction"); 0: it does not move.
    #[uniform(100)]
    pub fire_period: f32,
    /// The shape refracts (module notes, "Refraction").
    pub refract: bool,
    /// The shape lies on another surface (a decal shader flag is set): drawn with
    /// [`decal_bias`].
    pub decal: bool,
    /// An alpha-tested shape whose edges stay hard ([`soft_edges`] is not applied).
    pub hard_edges: bool,
    /// The shape's sun shadows are those of the shade light (module docs, "Shade light"),
    /// where the view has one.
    pub shade_light: bool,
    /// Holes (module notes, "Holes"): x the value in alpha units (0–255) at and above
    /// which the holes texture's alpha leaves a hole, negative for none; y the width of
    /// the edge band below it; z, w unused.
    #[uniform(100)]
    pub holes: Vec4,
    /// The colour of the holes' edge band (linear, alpha unused).
    #[uniform(100)]
    pub holes_edge: Vec4,
    /// Soft intersections (module notes, "Soft intersections"): x the distance in game
    /// units over which the shape fades out towards the opaque surface behind it, 0 for
    /// none; y, z, w unused.
    #[uniform(100)]
    pub soft: Vec4,
    /// The holes texture; `None` with no holes.
    #[texture(101)]
    #[sampler(102)]
    pub holes_texture: Option<Handle<Image>>,
    /// FaceGen tint (module notes, "FaceGen tint"): the colour offset in colour units
    /// (0–255 scale), one texel per `.egt` image texel, in the file's row order (first row
    /// at the texture's bottom); read with `textureLoad`, so it is not filtered. `None` on
    /// every shape but an NPC's face and skin.
    #[texture(103, sample_type = "float", filterable = false)]
    pub facegen_offset: Option<Handle<Image>>,
}

/// Game units over which a soft-intersecting shape fades out towards the surface behind
/// it (module notes, "Soft intersections") [engineering choice, judged on the Strip's
/// lamp beams against the owner's game frame].
pub const SOFT_DISTANCE: f32 = 160.0;

/// Whether a shape of this alpha mode fades out where it meets the surface behind it
/// (module notes, "Soft intersections"): unlit, added or blended (not multiplied: those
/// darken what they lie on), not a decal and not refracting.
pub fn soft_intersections(alpha_mode: AlphaMode, unlit: bool, decal: bool, refracts: bool) -> bool {
    unlit && !decal && !refracts && matches!(alpha_mode, AlphaMode::Add | AlphaMode::Blend | AlphaMode::Premultiplied)
}

/// [`NifSurface::holes`] of a shape with no holes.
pub const NO_HOLES: Vec4 = Vec4::new(-1.0, 0.0, 0.0, 0.0);

impl Default for NifSurface {
    fn default() -> Self {
        NifSurface {
            falloff: Falloff::NONE.params,
            alpha_cut: 0.0,
            add_whole: 0.0,
            refraction: 0.0,
            fire_period: 0.0,
            refract: false,
            decal: false,
            hard_edges: false,
            shade_light: false,
            holes: NO_HOLES,
            holes_edge: Vec4::ONE,
            soft: Vec4::ZERO,
            holes_texture: None,
            facegen_offset: None,
        }
    }
}

/// The part of [`NifSurface`] that selects the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NifSurfaceKey {
    pub decal: bool,
    pub refract: bool,
    pub hard_edges: bool,
    pub shade_light: bool,
    /// The shape has a FaceGen offset ([`NifSurface::facegen_offset`]).
    pub facegen: bool,
}

impl From<&NifSurface> for NifSurfaceKey {
    fn from(surface: &NifSurface) -> Self {
        NifSurfaceKey {
            decal: surface.decal,
            refract: surface.refract,
            hard_edges: surface.hard_edges,
            shade_light: surface.shade_light,
            facegen: surface.facegen_offset.is_some(),
        }
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
        nif_prepass(descriptor, key.mesh_key);
        if !key.bind_group_data.hard_edges {
            soft_edges(descriptor, key.mesh_key);
        }
        if key.bind_group_data.shade_light && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("SHADE_LIGHT".into());
            }
        }
        if key.bind_group_data.facegen && !descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("FACEGEN_TINT".into());
            }
        }
        if key.bind_group_data.refract {
            refraction_pipeline(descriptor);
        }
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

/// The pipeline of a refracting shape (module notes, "Refraction"): in the main pass the
/// shader gets `REFRACT` and no depth is written; a depth-only pass (the shadow maps; Bevy
/// leaves such materials out of the view's prepass) passes no fragment.
fn refraction_pipeline(descriptor: &mut RenderPipelineDescriptor) {
    let prepass = descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE"));
    if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
        depth_stencil.depth_write_enabled = Some(false);
        if prepass {
            depth_stencil.depth_compare = Some(CompareFunction::Never);
        }
    }
    if !prepass {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push("REFRACT".into());
        }
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

/// Power on the alpha of additive shapes ([`add_alpha`]; nif_material.wgsl has the same
/// number) [hypothesis, fitted: docs/spec-notes/nif.md §6 "Additive blend"].
pub const ADD_ALPHA_POWER: f32 = 1.5;

/// The alpha an additive shape (NiAlphaProperty destination factor ONE, drawn as
/// `AlphaMode::Add`) is drawn with, for its alpha `a` (CPU mirror of nif_material.wgsl).
///
/// The game adds colour × alpha to the frame's display values [hypothesis,
/// docs/spec-notes/nif.md §6 "Additive blend"]; Bevy adds linear colour × alpha to linear
/// light, which on a dark wall shows far brighter (0.1 added to black shows as 0.35). How
/// much linear light a display-value sum needs depends on the frame behind, which a fixed
/// blend cannot read: alpha^2.2 is exact on black and leaves faint shapes nearly invisible
/// on a lit wall, alpha^1 is the old look. [`ADD_ALPHA_POWER`] is the power that brought
/// the Prospector Saloon's window rays nearest to a display-value sum of the same rays.
pub fn add_alpha(a: f32) -> f32 {
    a.clamp(0.0, 1.0).powf(ADD_ALPHA_POWER)
}

/// The factor on the colour of an unlit alpha-blended shape (smoke, mist, dust, every
/// particle system; NiAlphaProperty SRC_ALPHA / INV_SRC_ALPHA) of alpha `a` (CPU mirror
/// of nif_material.wgsl).
///
/// The game blends colour × alpha + behind × (1 − alpha) in display values
/// [hypothesis, docs/spec-notes/nif.md §6 "Alpha blend of unlit shapes"]; Bevy does it
/// in linear light, where light smoke over a dark room shows about twice as bright
/// (owner's Bison Steve lobby screenshots, 2026-10-23). Scaled by √alpha, the colour
/// term becomes colour × alpha^[`ADD_ALPHA_POWER`], fitted against the display-value
/// blend; the share of what lies behind is kept.
pub fn blend_colour_scale(a: f32) -> f32 {
    a.clamp(0.0, 1.0).sqrt()
}

pub struct NifMaterialPlugin;

impl Plugin for NifMaterialPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "nif_material.wgsl");
        load_internal_asset!(app, FADE_PREPASS_SHADER, "fade_prepass.wgsl", Shader::from_wgsl);
        load_internal_asset!(app, NIF_PREPASS_SHADER, "nif_prepass.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<NifMaterial>::default()).add_systems(PostUpdate, see_through_casts_no_shadow);
    }
}

/// Whether a material is see-through: blended, added or multiplied onto what lies behind
/// it (glass, light rays, glows, smoke), and not an alpha-tested cut-out with soft edges
/// (`NifSurface::alpha_cut`), which is solid where it is drawn.
pub fn see_through(m: &NifMaterial) -> bool {
    matches!(m.base.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Multiply | AlphaMode::Premultiplied) && m.extension.alpha_cut <= 0.0
}

/// Marks a mesh [`see_through_casts_no_shadow`] took the shadow from.
#[derive(Component)]
pub struct SeeThroughNoShadow;

/// See-through shapes cast no shadows, from the sun, the flashlight or any other light
/// (owner, playtest 2026-10-11: "anything see-through should not be generating a
/// shadow"; the Prospector Saloon's window rays and lit lanterns threw the flashlight's
/// shadows). Their depth-only shadow pass drew every fragment with alpha above 0.05
/// (`nif_prepass.wgsl`), so a light ray stood in the light like a wall. Checked whenever
/// a mesh gets its material (docs/spec-notes/nif.md §6 "Shadows of see-through shapes").
#[allow(clippy::type_complexity)]
fn see_through_casts_no_shadow(
    mut commands: Commands,
    materials: Res<Assets<NifMaterial>>,
    changed: Query<(Entity, &MeshMaterial3d<NifMaterial>, Has<bevy::light::NotShadowCaster>, Has<SeeThroughNoShadow>), Changed<MeshMaterial3d<NifMaterial>>>,
) {
    for (e, m, no_shadow, ours) in &changed {
        let Some(m) = materials.get(&m.0) else { continue };
        match (see_through(m), no_shadow, ours) {
            (true, false, _) => {
                commands.entity(e).try_insert((bevy::light::NotShadowCaster, SeeThroughNoShadow));
            }
            (false, true, true) => {
                commands.entity(e).try_remove::<(bevy::light::NotShadowCaster, SeeThroughNoShadow)>();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_through_shapes_are_the_blended_ones_without_a_cut() {
        let with = |alpha_mode: AlphaMode, cut: f32| {
            let mut m = nif_material(StandardMaterial { alpha_mode, ..default() });
            m.extension.alpha_cut = cut;
            see_through(&m)
        };
        assert!(with(AlphaMode::Blend, 0.0));
        assert!(with(AlphaMode::Add, 0.0));
        assert!(with(AlphaMode::Multiply, 0.0));
        assert!(!with(AlphaMode::Blend, 0.5));
        assert!(!with(AlphaMode::Mask(0.5), 0.0));
        assert!(!with(AlphaMode::Opaque, 0.0));
    }

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
        // Only the decal, refraction, hard-edge, shade-light and FaceGen switches select the pipeline; the falloff and the
        // alpha cut are uniforms.
        let mut surface = NifSurface::default();
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: false, refract: false, hard_edges: false, shade_light: false, facegen: false });
        assert_eq!(surface.alpha_cut, 0.0);
        surface.falloff = Vec4::new(0.9, 0.2, 1.0, 0.0);
        surface.alpha_cut = 0.5;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey::from(&NifSurface::default()));
        surface.decal = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: false, shade_light: false, facegen: false });
        surface.hard_edges = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: false, facegen: false });
        surface.shade_light = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: true, facegen: false });
        // The refraction strength is a uniform; whether the shape refracts is the pipeline's.
        surface.refraction = 0.1;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: false, hard_edges: true, shade_light: true, facegen: false });
        surface.refract = true;
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: true, hard_edges: true, shade_light: true, facegen: false });
        // A FaceGen offset selects the tinting shader.
        surface.facegen_offset = Some(Handle::default());
        assert_eq!(NifSurfaceKey::from(&surface), NifSurfaceKey { decal: true, refract: true, hard_edges: true, shade_light: true, facegen: true });
    }

    /// CPU mirror of the shaders' edge formula (nif_material.wgsl, tree.wgsl).
    fn edge(alpha: f32, cutoff: f32, width: f32) -> f32 {
        ((alpha - cutoff) / width.max(0.0001) + 1.0).clamp(0.0, 1.0)
    }

    #[test]
    fn soft_intersections_for_unlit_added_and_blended_shapes_only() {
        // Beams and glows (added), smoke and particles (blended), premultiplied.
        for mode in [AlphaMode::Add, AlphaMode::Blend, AlphaMode::Premultiplied] {
            assert!(soft_intersections(mode, true, false, false));
        }
        // Not lit shapes (glass), multiply cards, solid or cut-out shapes, decals or
        // refracting shells.
        assert!(!soft_intersections(AlphaMode::Blend, false, false, false));
        assert!(!soft_intersections(AlphaMode::Multiply, true, false, false));
        assert!(!soft_intersections(AlphaMode::Opaque, true, false, false));
        assert!(!soft_intersections(AlphaMode::Mask(0.5), true, false, false));
        assert!(!soft_intersections(AlphaMode::Add, true, true, false));
        assert!(!soft_intersections(AlphaMode::Add, true, false, true));
    }

    #[test]
    fn soft_edge_covers_what_the_alpha_test_keeps() {
        // `width` is how much the alpha changes across one pixel.
        for (cutoff, width) in [(0.5, 0.25), (0.36, 0.05), (128.5 / 255.0, 1.0)] {
            // At and above the cutoff (where the depth prepass writes depth): all samples.
            assert_eq!(edge(cutoff, cutoff, width), 1.0);
            assert_eq!(edge(1.0, cutoff, width), 1.0);
            // One pixel's change below the cutoff: nothing; half way: half the samples.
            assert_eq!(edge(cutoff - width, cutoff, width), 0.0);
            assert!((edge(cutoff - width / 2.0, cutoff, width) - 0.5).abs() < 1e-5);
        }
        // Constant alpha (no change across the pixel): the plain alpha test.
        assert_eq!(edge(0.49, 0.5, 0.0), 0.0);
        assert_eq!(edge(0.5, 0.5, 0.0), 1.0);
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

    #[test]
    fn additive_blend_alpha() {
        let display = |v: f32| v.powf(1.0 / MULTIPLY_GAMMA);
        let linear = |v: f32| v.powf(MULTIPLY_GAMMA);
        // What a white shape of alpha `a` adds to a display value `behind`.
        let adds = |behind: f32, a: f32| display(linear(behind) + add_alpha(a)) - behind;
        // The Saloon's window rays (indfxlightrays01.nif): a white texture, vertex alpha
        // 0.1 at most, so the game adds 0.1 at most. With the alpha as stored a ray added
        // 0.35 on black and 0.15 on a wall of 0.3; now 0.21 and 0.05, between that and
        // alpha^2.2 (0.1 on black, but 0.01 on the wall).
        assert!((adds(0.0, 0.1) - 0.208).abs() < 0.001);
        assert!((adds(0.3, 0.1) - 0.055).abs() < 0.001);
        assert!((display(0.1) - 0.351).abs() < 0.001);
        // Full and no alpha are unchanged.
        assert_eq!(add_alpha(1.0), 1.0);
        assert_eq!(add_alpha(0.0), 0.0);
        assert_eq!(add_alpha(2.0), 1.0);
    }

    #[test]
    fn unlit_blend_colour() {
        let display = |v: f32| v.powf(1.0 / MULTIPLY_GAMMA);
        let linear = |v: f32| v.powf(MULTIPLY_GAMMA);
        // Bevy's blend of a colour (display) with the factor on it, over `behind`.
        let blend = |colour: f32, a: f32, behind: f32| display(linear(colour) * blend_colour_scale(a) * a + linear(behind) * (1.0 - a));
        // Grey 0.5 smoke at alpha 0.3 over black: 0.15 in display values, 0.29 as
        // blended before; now 0.22.
        assert!((blend(0.5, 0.3, 0.0) - 0.220).abs() < 0.002, "{}", blend(0.5, 0.3, 0.0));
        assert!((display(linear(0.5) * 0.3) - 0.290).abs() < 0.002);
        // The colour term is colour × alpha^1.5, the add_alpha power.
        assert!((blend_colour_scale(0.3) * 0.3 - add_alpha(0.3)).abs() < 1e-6);
        // Over a backdrop of its own colour it stays near it (display 0.5).
        assert!((blend(0.5, 0.3, 0.5) - 0.5).abs() < 0.04);
        // Full and no alpha are unchanged.
        assert_eq!(blend_colour_scale(1.0), 1.0);
        assert_eq!(blend_colour_scale(0.0), 0.0);
        assert_eq!(blend_colour_scale(2.0), 1.0);
    }
}
