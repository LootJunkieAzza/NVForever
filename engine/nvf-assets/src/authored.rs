//! Authored PBR material sets as materials use them (#2981, docs/spec-notes/pbr-materials.md).
//!
//! - The run's switch ([`enabled`]): the settings' Authored materials row, read once at the
//!   start ([`set_enabled`], nvforever `main.rs`) and fixed for the run, so cached models
//!   never mix the two states. Off, nothing here asks for the index (`GameData::pbr_index`)
//!   or reads a file under `textures\pbr\`.
//! - [`find`]: a shape's set by its resolved diffuse key, the members loaded by the exact
//!   paths the index names, the scalar file read once per mount ([`scalars_cached`],
//!   `GameData::pbr_scalars`).
//! - [`apply`] / [`restore`]: a set on a material and back, from the legacy look every
//!   eligible shape's material keeps ([`LegacyLook`]), the set's `_albedo` drawn in place of
//!   the game's diffuse where it has one; [`retexture`] is the one rule for a lit copy whose
//!   diffuse is replaced later.
//!
//! [design] The rules are NVForever's own (the format is ours); none describes the original.

use crate::data::GameData;
use crate::texture::TextureUsage;
use bevy::prelude::*;
use fnv_pbr::{Found, Member, PbrIndex, Scalars, SetState};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

/// The run's state of the Authored materials setting (module notes).
static RUN: OnceLock<bool> = OnceLock::new();

/// Fix the run's state of the setting, before the first material is made. Called once at the
/// start; a later call that disagrees is refused with a warning (the state is the run's).
pub fn set_enabled(on: bool) {
    if RUN.set(on).is_err() && enabled() != on {
        warn!("authored materials: already {} for this run; the setting takes effect at the next start", if enabled() { "on" } else { "off" });
    }
}

/// Whether authored sets shape materials in this run: as [`set_enabled`] fixed it, else on
/// (the default; the viewer never sets it).
pub fn enabled() -> bool {
    *RUN.get_or_init(|| true)
}

/// An authored set found for a shape's diffuse, its maps loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredSet {
    /// The set's stem (`textures\pbr\...`, without suffix).
    pub stem: String,
    /// `_orm`: occlusion red, roughness green, metallic blue (linear).
    pub orm: Handle<Image>,
    /// `_n` in the two-channel role, when the set has one and it loaded.
    pub normal: Option<Handle<Image>>,
    /// `_albedo` in the colour role (sRGB, as the game's diffuse), when the set has one and it
    /// loaded: the base colour texture in place of the game's diffuse ([`apply`]).
    pub albedo: Option<Handle<Image>>,
    /// The scalar file's values (all `None` without one).
    pub scalars: Scalars,
    /// The base colour gain of the set's metals ([`set_metal_gain`]: the run's override, the
    /// scalar file's `metal_gain`, or its default), carried to the shader by
    /// `NifSurface::metal_gain` ([`apply`]).
    pub metal_gain: f32,
}

/// The files [`find`] and the decoders already warned about (one warning per file).
#[derive(Default)]
pub struct Memo {
    warned: Mutex<HashSet<String>>,
}

/// The scalar files of one mount by path, each read once ([`scalars_cached`]); kept beside the
/// mount's index (`GameData`), so a new mount starts empty.
pub type ScalarCache = Mutex<HashMap<String, Arc<OnceLock<Scalars>>>>;

/// The scalars of the file at `path`, from `cache`: the first caller reads (`read`), parses
/// and logs its warnings while any other caller for the same path waits on that path's cell,
/// so a file is read and warned about once; other paths are not held up.
pub(crate) fn scalars_cached(cache: &ScalarCache, path: &str, read: impl FnOnce() -> Option<Vec<u8>>) -> Scalars {
    let cell = cache.lock().expect("authored scalars").entry(path.to_string()).or_default().clone();
    *cell.get_or_init(|| match read() {
        Some(bytes) => {
            let file = fnv_pbr::parse_scalars(&bytes);
            for w in &file.warnings {
                warn!("authored materials: {path}: {w}");
            }
            file.scalars
        }
        None => {
            warn!("authored materials: {path} could not be read; the defaults apply");
            Scalars::default()
        }
    })
}

impl Memo {
    /// Log `message` the first time `path` is warned about.
    pub(crate) fn warn_once(&self, path: &str, message: impl FnOnce() -> String) {
        if self.warned.lock().expect("authored warnings").insert(path.to_string()) {
            warn!("{}", message());
        }
    }
}

/// The authored set (state [`SetState::Authored`]) for the resolved diffuse `key`: the one
/// active set the index finds (`PbrIndex::lookup`, with its first-person rule), if it is
/// authored. A height-only set changes no material in this stage, so it means "no authored
/// set" for the shape, also when it is a first-person twin's own set: the twin's set wins the
/// lookup and the plain texture's set is not used behind it [design: the first-person rule of
/// docs/spec-notes/pbr-materials.md "Layout"; stage 3 of #2981 draws no height maps].
pub fn authored_found(index: &PbrIndex, key: &str) -> Option<Found> {
    index.lookup(key).filter(|f| f.state() == SetState::Authored)
}

/// The authored set for the resolved diffuse `key` with its maps loaded by `load` (path as the
/// index names it, exactly: never resolved again, so a `\xbox\` member stays the one listed;
/// role), or `None`: the setting off (`enabled`; nothing is asked of the index then), no
/// authored set, or an `_orm` that does not load (one warning per file; the shape keeps the
/// game's material). An `_n` that does not load leaves the game's normal map in use, an
/// `_albedo` that does not load the game's diffuse (and the metal gain's default for it).
pub(crate) fn find(data: &GameData, enabled: bool, key: &str, memo: &Memo, mut load: impl FnMut(&str, TextureUsage) -> Option<Handle<Image>>) -> Option<AuthoredSet> {
    if !enabled {
        return None;
    }
    let found = authored_found(data.pbr_index(), key)?;
    let orm_path = found.path(Member::Orm)?;
    let Some(orm) = load(&orm_path, TextureUsage::Linear) else {
        Stats::add(&stats().orm_failed);
        memo.warn_once(&orm_path, || format!("authored materials: {orm_path} could not be loaded; surfaces drawn with {key} keep the game's material"));
        return None;
    };
    let normal = found.path(Member::Normal).and_then(|path| {
        let h = load(&path, TextureUsage::NormalTwoChannel);
        if h.is_none() {
            memo.warn_once(&path, || format!("authored materials: {path} could not be loaded; the game's normal map is used"));
        }
        h
    });
    let albedo = found.path(Member::Albedo).and_then(|path| {
        let h = load(&path, TextureUsage::Color);
        if h.is_none() {
            memo.warn_once(&path, || format!("authored materials: {path} could not be loaded; the game's diffuse stays the base colour"));
        }
        h
    });
    let scalars = found.path(Member::Scalars).map(|path| data.pbr_scalars(&path)).unwrap_or_default();
    let metal_gain = set_metal_gain(&scalars, albedo.is_some(), metal_gain_override());
    Some(AuthoredSet { stem: found.stem, orm, normal, albedo, scalars, metal_gain })
}

/// The value of `NifSurface::emissive_rule.w` that marks a material shaded from an authored
/// set (0 on every other material; the lane is otherwise unused, and survives the mesh
/// terrain blend's conversion).
pub const AUTHORED_MARK: f32 = 1.0;

/// What a lit shape's material is without a set (its "legacy look"), kept on the material
/// (`NifSurface::legacy`) for every shape that may take a set, so a later change of its
/// diffuse (a script's texture swap, the blackjack cards, the Vigor Tester's digits) can put
/// the new diffuse's set on it or go back to the game's values ([`retexture`]). Changed
/// after the material is made only by a script's glossiness or specular colour edit
/// ([`set_legacy_roughness`], [`set_legacy_specular_tint`]);
/// copies carry it along.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct LegacyLook {
    /// The game's diffuse: the base colour texture as the material was built, or as a later
    /// swap put it ([`retexture`]); a set's `_albedo` replaces it while the set is on, and
    /// [`restore`] gives it back.
    pub diffuse: Option<Handle<Image>>,
    /// The game's normal map in use (after the tangent check).
    pub normal: Option<Handle<Image>>,
    /// The specular mask (the normal map, when its alpha is one: `nif_spawn::specular`).
    pub specular: Option<Handle<Image>>,
    pub roughness: f32,
    pub reflectance: f32,
    pub metallic: f32,
    /// The drawn mesh has tangents, so a set's `_n` can be applied to it even where the game
    /// gives no normal map.
    pub tangents: bool,
    /// The specular tint (Bevy multiplies the reflectance by it): white as built, a script's
    /// specular colour once one is set ([`set_legacy_specular_tint`]).
    pub specular_tint: Color,
    /// A specular colour a script set itself (the colour was the value it changed), which a
    /// set put on later keeps drawing in place of white ([`apply`]); `None` as built, and when
    /// only the stored colour was written back with another value [design].
    pub scripted_tint: Option<Color>,
}

/// Whether a shape may take an authored set, by what is known when its material is made:
/// lit, not refracting, not hair or skin (FaceGen), drawn neither additive nor multiplied
/// (the final alpha mode), with texture coordinates, and not excluded by its caller
/// (`no_authored`: particles, object LOD) [design, docs/spec-notes/pbr-materials.md "What a
/// set changes"]. The key is checked by [`key_eligible`].
pub fn shape_eligible(unlit: bool, refracts: bool, hair: bool, skin: bool, alpha_mode: AlphaMode, has_uv: bool, no_authored: bool) -> bool {
    !unlit && !refracts && !hair && !skin && has_uv && !no_authored && !matches!(alpha_mode, AlphaMode::Add | AlphaMode::Multiply)
}

/// Whether a resolved diffuse key may take a set: not the Pip-Boy's or the interface's
/// textures [design].
pub fn key_eligible(key: &str) -> bool {
    !key.starts_with("textures\\pipboy3000\\") && !key.starts_with("textures\\interface\\")
}

/// The metallic value an authored material is built with: 0 (the ORM's blue channel then has
/// no effect: no reflection source, docs/spec-notes/pbr-materials.md "Metallic"), or 1 in the
/// metallic channel view ([`PbrView::Metallic`]), so the material's sample of the blue
/// channel is the per-texel value the view shows, under the metallic debug switch
/// ([`metallic_debug`]), so the blue channel takes effect, and with reflections on
/// (`reflection_cube::enabled`), where nif_material.wesl holds the sampled value at 0 in every
/// view without a reflection cube, so only a view with one shades a metal as a metal.
pub fn authored_metallic() -> f32 {
    if view() == PbrView::Metallic || metallic_debug() || crate::reflection_cube::enabled() { 1.0 } else { 0.0 }
}

/// The environment variable of the metallic debug switch ([`metallic_debug`]).
pub const METALLIC_ENV: &str = "NVF_PBR_METALLIC";

/// The metallic debug switch (a calibration aid, off by default; `NVF_PBR_METALLIC=1`, read
/// once at the start): authored materials are built with metallic 1, so the ORM's blue
/// channel takes effect, and nif_material.wesl's removal of Bevy's ambient highlight keeps
/// the metal's share of it on authored materials ((1 - m) of it is removed, m the sampled
/// metallic), so a metal shows the ambient light in its colour instead of drawing black where
/// no lamp's highlight falls (`NVF_PBR_METALLIC` shader def) [design, DESIGN option (b)].
/// Every other material is drawn as without it. With reflections on
/// (`reflection_cube::enabled`) the "keep the metal share" rule is retired: the shader def is
/// not set and the full ambient highlight is removed, also at metallic 1, since the cube
/// replaces it (nif_material.rs, reflections slice 1).
pub fn metallic_debug() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var(METALLIC_ENV).is_ok_and(|v| v.trim() == "1");
        if on {
            info!("authored materials: metallic debug switch ({METALLIC_ENV}=1): authored materials are metallic by their ORM's blue channel and keep a metal's ambient highlight");
        }
        on
    })
}

/// The environment variable of the metal gain override ([`metal_gain_override`]).
pub const METAL_GAIN_ENV: &str = "NVF_PBR_METAL_GAIN";

/// The value of [`METAL_GAIN_ENV`] as a gain: `Ok(None)` for no value (or an empty one),
/// `Ok(Some(g))` for a number in the scalar file's range (`fnv_pbr::METAL_GAIN_RANGE`, 1 to
/// 40), `Err` with the reason otherwise.
pub fn parse_metal_gain(text: Option<&str>) -> Result<Option<f32>, String> {
    let range = fnv_pbr::METAL_GAIN_RANGE;
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    match t.parse::<f32>() {
        Ok(g) if range.contains(&g) => Ok(Some(g)),
        Ok(g) => Err(format!("{METAL_GAIN_ENV}={g} is outside {} to {}", range.start(), range.end())),
        Err(_) => Err(format!("{METAL_GAIN_ENV}={t} is not a number")),
    }
}

/// The metal gain override (a tuning aid, off by default; `NVF_PBR_METAL_GAIN=<gain>`, 1 to
/// 40, read once at the start, NifMaterialPlugin): every set's metal gain
/// ([`AuthoredSet::metal_gain`]) is this value for the run, whatever its scalar file or
/// `_albedo` says, so a gain can be judged by picture [design]. A value that is no number or
/// out of range is warned about once and the sets keep their own gains. Not a
/// `NVF_PLAY_TEST_*` aid: it does not make a run unattended.
pub fn metal_gain_override() -> Option<f32> {
    static GAIN: OnceLock<Option<f32>> = OnceLock::new();
    *GAIN.get_or_init(|| match parse_metal_gain(std::env::var(METAL_GAIN_ENV).ok().as_deref()) {
        Ok(Some(g)) => {
            info!("authored materials: metal gain override ({METAL_GAIN_ENV}={g}): every authored set's metal gain is {g} for this run");
            Some(g)
        }
        Ok(None) => None,
        Err(reason) => {
            warn!("authored materials: {reason}; each set keeps its own metal gain");
            None
        }
    })
}

/// A set's metal gain: `over` (the run's override, [`metal_gain_override`]) when given, else
/// the scalar file's `metal_gain`, else its default by whether an `_albedo` is drawn
/// (`albedo`: 1, real reflectances) or the game's diffuse (12) [design].
pub fn set_metal_gain(scalars: &Scalars, albedo: bool, over: Option<f32>) -> f32 {
    over.unwrap_or_else(|| scalars.metal_gain_or_default(albedo))
}

/// The base colour an authored metal is lit with in a view with a reflection cube: the CPU
/// mirror of nif_material.wesl's `nvf_metal_base` (tests compare the two by their rules):
/// mix(base, lifted, metallic), lifted = base x min(gain, 1 / brightest channel), never below
/// base. One gain on all three channels, limited so the brightest channel reaches 1, keeps
/// the hue (the experiment before it clipped each channel at 1, which shifted bright texels
/// towards white) [design].
pub fn lifted_base(base: Vec3, gain: f32, metallic: f32) -> Vec3 {
    let brightest = base.x.max(base.y).max(base.z.max(1e-4));
    let lifted = base * gain.min(1.0 / brightest).max(1.0);
    base.lerp(lifted, metallic)
}

/// The environment variable that picks a channel view ([`PbrView`]).
pub const VIEW_ENV: &str = "NVF_PBR_VIEW";

/// A debug view of the material inputs of NIF surfaces (a calibration aid for authored sets,
/// off by default): the lit colour of forward-drawn lit NIF surfaces is replaced by one
/// input, read just after the standard material's sampling (nif_material.wesl,
/// "NVF_PBR_VIEW"). Refracting shapes return before it; additive draws add it by their alpha
/// and multiplied ones stay neutral, so effects fade out of the view. Picked once at
/// the start by `NVF_PBR_VIEW` = `roughness` | `metallic` | `occlusion` | `normal` |
/// `authored` | `reflection` [design]. Not a `NVF_PLAY_TEST_*` aid: it does not make a run unattended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PbrView {
    Off,
    /// Perceptual roughness after the scale (the material's value × the ORM's green).
    Roughness,
    /// The ORM's blue channel (authored materials are built with metallic 1 in this view).
    Metallic,
    /// The ORM's red channel, sampled on its own (no screen-space occlusion); 1 without one.
    Occlusion,
    /// The world-space normal as 0.5 + 0.5 N.
    Normal,
    /// Authored shapes tinted orange, every other shape grey, by its base colour's brightness.
    Authored,
    /// What an authored surface reflects: the view's reflection cube at level 0 along the
    /// surface's mirror direction, as a white perfect mirror writes it (texel x intensity x
    /// exposure, tone-mapped as the rest of the picture), so its scale can be judged against
    /// the sky in the same frame. Every other shape grey as in [`PbrView::Authored`];
    /// authored shapes orange where the view has no cube or reflections are off. Only the
    /// opted-in NIF fragment (`reflection_cube::REFLECT_DEF`) reads the cube.
    Reflection,
}

impl PbrView {
    pub const ALL: [PbrView; 6] = [PbrView::Roughness, PbrView::Metallic, PbrView::Occlusion, PbrView::Normal, PbrView::Authored, PbrView::Reflection];

    pub fn name(self) -> &'static str {
        match self {
            PbrView::Off => "off",
            PbrView::Roughness => "roughness",
            PbrView::Metallic => "metallic",
            PbrView::Occlusion => "occlusion",
            PbrView::Normal => "normal",
            PbrView::Authored => "authored",
            PbrView::Reflection => "reflection",
        }
    }

    /// The view named `text` (any case; empty or `off`: none); `None` for another word.
    pub fn parse(text: &str) -> Option<PbrView> {
        let t = text.trim().to_ascii_lowercase();
        if t.is_empty() || t == "off" {
            return Some(PbrView::Off);
        }
        PbrView::ALL.into_iter().find(|v| v.name() == t)
    }

    /// The fragment shader's defs for the view: `NVF_PBR_VIEW` and the mode's own.
    pub fn shader_defs(self) -> &'static [&'static str] {
        match self {
            PbrView::Off => &[],
            PbrView::Roughness => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_ROUGHNESS"],
            PbrView::Metallic => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_METALLIC"],
            PbrView::Occlusion => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_OCCLUSION"],
            PbrView::Normal => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_NORMAL"],
            PbrView::Authored => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_AUTHORED"],
            PbrView::Reflection => &["NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"],
        }
    }
}

/// The run's channel view, read from [`VIEW_ENV`] the first time (the NIF material plugin asks
/// at the start) and fixed for the run.
pub fn view() -> PbrView {
    static VIEW: OnceLock<PbrView> = OnceLock::new();
    *VIEW.get_or_init(|| {
        let Some(text) = std::env::var_os(VIEW_ENV) else { return PbrView::Off };
        let text = text.to_string_lossy();
        match PbrView::parse(&text) {
            Some(PbrView::Off) => PbrView::Off,
            Some(v) => {
                info!("authored materials: channel view {} ({VIEW_ENV}): NIF surfaces show that input instead of their lit colour", v.name());
                v
            }
            None => {
                warn!("authored materials: {VIEW_ENV}={text} is none of roughness, metallic, occlusion, normal, authored, reflection; no channel view");
                PbrView::Off
            }
        }
    })
}

/// Put `set` on `material` from its legacy look: the ORM as metallic-roughness and occlusion
/// map (one image in both slots), no specular mask, the scalars' roughness scale and
/// reflectance (their defaults without a file), [`authored_metallic`], the set's `_n` when it
/// has one and the drawn mesh has tangents (else the game's normal map, its alpha no longer a
/// mask), the mark ([`AUTHORED_MARK`]) and the set's metal gain (`NifSurface::metal_gain`);
/// the set's `_albedo` as the base colour texture when it has one (`NifSurface::
/// authored_albedo` records it), else the game's diffuse ([`LegacyLook::diffuse`]) where
/// another set's `_albedo` was drawn. Everything else (the base colour factor, alpha, glow,
/// decal, the extension's other values) stays. Whether the set's `_n` was applied.
pub fn apply(material: &mut crate::NifMaterial, set: &AuthoredSet, legacy: &LegacyLook) -> bool {
    drop_albedo(material, legacy);
    if let Some(albedo) = &set.albedo {
        material.base.base_color_texture = Some(albedo.clone());
        material.extension.authored_albedo = Some(albedo.clone());
    }
    let b = &mut material.base;
    b.metallic_roughness_texture = Some(set.orm.clone());
    b.occlusion_texture = Some(set.orm.clone());
    b.specular_texture = None;
    b.perceptual_roughness = set.scalars.roughness_scale_or_default();
    b.reflectance = set.scalars.reflectance_or_default();
    b.metallic = authored_metallic();
    // A set's highlight strength is its reflectance alone, never scaled by the game's stored
    // specular colour (#2981); a colour a script set itself stays drawn, as it is on the
    // material the script edited, across a later set (a texture swap's).
    b.specular_tint = legacy.scripted_tint.unwrap_or(Color::WHITE);
    let normal = set.normal.clone().filter(|_| legacy.tangents);
    if normal.is_none() && set.normal.is_some() {
        Stats::add(&stats().normal_without_tangents);
        no_tangents_once(&set.stem);
    }
    let applied = normal.is_some();
    b.normal_map_texture = normal.or_else(|| legacy.normal.clone());
    material.extension.emissive_rule.w = AUTHORED_MARK;
    material.extension.metal_gain = set.metal_gain;
    applied
}

/// Put the legacy look back on `material` (the inverse of [`apply`]): the game's diffuse too,
/// where the set's `_albedo` is still the base colour texture.
pub fn restore(material: &mut crate::NifMaterial, legacy: &LegacyLook) {
    drop_albedo(material, legacy);
    let b = &mut material.base;
    b.metallic_roughness_texture = None;
    b.occlusion_texture = None;
    b.specular_texture = legacy.specular.clone();
    b.normal_map_texture = legacy.normal.clone();
    b.perceptual_roughness = legacy.roughness;
    b.reflectance = legacy.reflectance;
    b.metallic = legacy.metallic;
    b.specular_tint = legacy.specular_tint;
    material.extension.emissive_rule.w = 0.0;
    material.extension.metal_gain = 0.0;
}

/// Take a set's `_albedo` off `material` (`NifSurface::authored_albedo`): the game's diffuse
/// back as the base colour texture where the `_albedo` is still the one drawn; a texture a
/// caller put there since (a picture, a swap's new diffuse) stays.
fn drop_albedo(material: &mut crate::NifMaterial, legacy: &LegacyLook) {
    if let Some(albedo) = material.extension.authored_albedo.take() {
        if material.base.base_color_texture.as_ref() == Some(&albedo) {
            material.base.base_color_texture = legacy.diffuse.clone();
        }
    }
}

/// A script's change of a shape's glossiness (nvforever `nif_mirror.rs`, the material
/// property mirror) as perceptual roughness: the legacy look takes it, so the game's value is
/// the one restored later, and the drawn roughness takes it only while no set is on the
/// material; a set's roughness (its scale on the ORM's green) stays in force meanwhile
/// [design].
pub fn set_legacy_roughness(material: &mut crate::NifMaterial, roughness: f32) {
    if let Some(legacy) = material.extension.legacy.as_mut() {
        legacy.roughness = roughness;
    }
    if !is_authored(material) {
        material.base.perceptual_roughness = roughness;
    }
}

/// A script's specular colour (nvforever `nif_mirror.rs`): the legacy look takes it, so it is
/// the one restored later; the drawn tint takes it when no set is on the material, or when
/// the colour is what the script changed (`changed`). The mirror writes every value back
/// after any one change, so an edit of another property leaves a set's highlight (Bevy draws
/// `specular_tint x reflectance`) as it is [design]. A colour the script changed is kept as
/// its override (`LegacyLook::scripted_tint`), so a set applied later ([`apply`], through a
/// swap's [`retexture`]) keeps drawing it instead of going back to white.
pub fn set_legacy_specular_tint(material: &mut crate::NifMaterial, tint: Color, changed: bool) {
    if let Some(legacy) = material.extension.legacy.as_mut() {
        legacy.specular_tint = tint;
        if changed {
            legacy.scripted_tint = Some(tint);
        }
    }
    if changed || !is_authored(material) {
        material.base.specular_tint = tint;
    }
}

/// Whether any shape of `data`'s mount can take an authored set: the run's setting on
/// (`enabled`) and at least one authored set in the mount's index (built here if it was not).
/// Callers that make shapes legacy for good once their role is known (nvforever `actors.rs`)
/// skip that work when it is false: no set can reach such a shape in this mount, through a
/// material build or a later swap alike. A new mount (`GameData::set_vfs`) would need those
/// materials made again; no production path mounts again (only tests call `set_vfs`).
pub fn mount_has_sets(data: &GameData) -> bool {
    mount_has_sets_when(data, enabled())
}

fn mount_has_sets_when(data: &GameData, on: bool) -> bool {
    on && data.pbr_index().counts().authored > 0
}

/// Whether `material` is shaded from an authored set.
pub fn is_authored(material: &crate::NifMaterial) -> bool {
    material.extension.emissive_rule.w == AUTHORED_MARK
}

/// Make `material` the game's own for good: its legacy look put back and its baseline
/// dropped, so no later swap gives it a set. For shapes found to be hair or a FaceGen tint
/// target only after their material was made (nvforever `actors.rs`, `facegen.rs`). Whether
/// anything changed.
pub fn make_legacy(material: &mut crate::NifMaterial) -> bool {
    let Some(legacy) = material.extension.legacy.take() else { return false };
    if is_authored(material) {
        restore(material, &legacy);
    }
    Stats::add(&stats().made_legacy);
    true
}

/// Clear every authored map from an unlit copy (the Pip-Boy's glare and menus copies,
/// nvforever `pipboy_arm.rs`): the ORM in both slots, the depth map (Bevy applies parallax
/// before its unlit test), the mark, the metal gain, a set's `_albedo` (the game's diffuse back
/// where it is still the texture drawn; a picture the caller put there stays) and the legacy
/// look, so nothing of a set reaches it whatever its source wore [design, a second guard
/// beside the `textures\pipboy3000\` exclusion].
pub fn clear_for_unlit_copy(material: &mut crate::NifMaterial) {
    if let Some(legacy) = material.extension.legacy.clone() {
        drop_albedo(material, &legacy);
    }
    material.extension.authored_albedo = None;
    material.base.metallic_roughness_texture = None;
    material.base.occlusion_texture = None;
    material.base.depth_map = None;
    material.extension.emissive_rule.w = 0.0;
    material.extension.metal_gain = 0.0;
    material.extension.legacy = None;
}

/// What [`retexture`] did to a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retextured {
    /// The material takes no set (no legacy look kept: not eligible, or made legacy).
    Untouched,
    /// The new diffuse's set is on it.
    Authored,
    /// No set for the new diffuse: the legacy look is on it (as it was, or put back).
    Legacy,
}

/// The one rule for a lit copy of a shape's material whose diffuse was just replaced by
/// `texture` (a NIF texture path; script `SwapTexture`, the blackjack cards, the Vigor
/// Tester's buttons and digits): from the material's legacy look ([`LegacyLook`], carried by
/// the copy), the new diffuse's set is applied (its scalars' defaults, the game's normal map
/// where it has no `_n` or the drawn mesh has no tangents, its `_albedo` in place of the new
/// diffuse where it has one), or, without one, the legacy look is put back if a set was on
/// it. Always from the legacy look, never from what the copy wore: a copy of a material that
/// wore another diffuse's set takes none of it. The caller must put the replacement diffuse
/// on the copy before this call. It becomes the legacy look's diffuse ([`LegacyLook::diffuse`])
/// even when it happens to be the previous set's `_albedo` image. Only the set's fields change;
/// tint, animation, fade and every other value stay.
pub fn retexture(
    material: &mut crate::NifMaterial,
    texture: &str,
    textures: &mut crate::TextureCache,
    data: &GameData,
    images: &mut Assets<Image>,
    formats: Option<&bevy::image::CompressedImageFormatSupport>,
) -> Retextured {
    use crate::nif_spawn::TextureSource;
    let Some(mut legacy) = material.extension.legacy.clone() else { return Retextured::Untouched };
    let shown = material.base.base_color_texture.clone();
    if shown != legacy.diffuse {
        legacy.diffuse = shown;
        material.extension.legacy = Some(legacy.clone());
    }
    let mut source = crate::nif_spawn::StoreTextures { textures, data, images, formats };
    let key = source.resolve(texture);
    match key_eligible(&key).then(|| source.authored(&key)).flatten() {
        Some(set) => {
            apply(material, &set, &legacy);
            Stats::add(&stats().swaps_authored);
            Retextured::Authored
        }
        None => {
            if is_authored(material) {
                restore(material, &legacy);
            }
            Stats::add(&stats().swaps_legacy);
            Retextured::Legacy
        }
    }
}

/// What authored sets did this run, for the exit summary ([`report`]): counts of events, not
/// a census of visible shapes (a material rebuilt or discarded counts again). Relaxed
/// atomics; a set of stems only where a set is applied.
#[derive(Default)]
pub struct Stats {
    /// Material builds that applied a set (`nif_spawn::shape_material`).
    pub built: AtomicU64,
    /// Shapes the run's setting would serve that may take no set (unlit, refracting, hair,
    /// skin, additive or multiplied, no UVs, particles, landscape LOD).
    pub not_eligible: AtomicU64,
    /// Shapes whose diffuse is under `textures\pipboy3000\` or `textures\interface\`.
    pub excluded_texture: AtomicU64,
    /// Sets whose `_orm` did not load, asked for by a material build or by a swap's
    /// [`retexture`] alike ([`find`]; the shape stays the game's).
    pub orm_failed: AtomicU64,
    /// Sets put on a material without their `_n`, the drawn mesh having no tangents.
    pub normal_without_tangents: AtomicU64,
    /// Copies made the game's own after their material was built (a role found later:
    /// [`make_legacy`] calls that dropped a legacy look).
    pub made_legacy: AtomicU64,
    /// [`retexture`] calls on materials with a legacy look that put a set on, and that left or
    /// put back the legacy look (cached swaps do not call it again).
    pub swaps_authored: AtomicU64,
    pub swaps_legacy: AtomicU64,
    stems: Mutex<HashSet<String>>,
}

impl Stats {
    fn add(counter: &AtomicU64) {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn get(counter: &AtomicU64) -> u64 {
        counter.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The summary line (without its prefix).
    pub fn line(&self) -> String {
        format!(
            "{} material builds applied a set ({} distinct set stems at build); builds without one: {} not eligible, {} on excluded textures; {} ORM load failures at build or swap; {} applied without their _n (no tangents); {} role copies made legacy; {} retexture calls ({} to a set, {} to the legacy look)",
            Self::get(&self.built),
            self.stems.lock().expect("authored stats").len(),
            Self::get(&self.not_eligible),
            Self::get(&self.excluded_texture),
            Self::get(&self.orm_failed),
            Self::get(&self.normal_without_tangents),
            Self::get(&self.made_legacy),
            Self::get(&self.swaps_authored) + Self::get(&self.swaps_legacy),
            Self::get(&self.swaps_authored),
            Self::get(&self.swaps_legacy),
        )
    }
}

/// The run's counts ([`Stats`]).
pub fn stats() -> &'static Stats {
    static STATS: OnceLock<Stats> = OnceLock::new();
    STATS.get_or_init(Stats::default)
}

/// A shape the setting would serve skipped by [`shape_eligible`] (`texture`: by
/// [`key_eligible`] instead).
pub(crate) fn note_skipped(texture: bool) {
    Stats::add(if texture { &stats().excluded_texture } else { &stats().not_eligible });
}

/// A material built with `set`.
pub(crate) fn note_built(set: &AuthoredSet) {
    Stats::add(&stats().built);
    let mut stems = stats().stems.lock().expect("authored stats");
    if !stems.contains(&set.stem) {
        stems.insert(set.stem.clone());
    }
}

/// The exit summary (Last, on the app's exit, as the other systems' summary lines): one line,
/// `authored materials summary: ...`, or that the setting was off.
pub fn report(mut exit: MessageReader<AppExit>) {
    if exit.read().last().is_none() {
        return;
    }
    if enabled() {
        info!("authored materials summary: {}", stats().line());
    } else {
        info!("authored materials summary: off (setting)");
    }
}

/// Log once per set that its `_n` was not applied to a shape without tangents.
fn no_tangents_once(stem: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    if SEEN.get_or_init(Default::default).lock().expect("authored tangents").insert(stem.to_string()) {
        info!("authored materials: {stem}_n.dds not applied where the drawn mesh has no tangents (the game's normal state is kept)");
    }
}

/// The warning for a linear member whose DX10 header names an `_SRGB` format (once per file:
/// the decode of a (path, role) happens once).
pub(crate) fn srgb_warning(memo: &Memo, path: &str, usage: TextureUsage, srgb_code: bool) {
    if srgb_code && usage != TextureUsage::Color && path.starts_with(fnv_pbr::PBR_DIR) {
        memo.warn_once(path, || format!("authored materials: {path} names an sRGB format in its DX10 header; it is read as linear data, as its role is"));
    }
}

#[cfg(test)]
pub(crate) mod tests;
