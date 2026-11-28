//! Provides component types for lighting a bevy scene. This includes the usual
//! directional, point, and spot lights, as well as light probes, atmosphere,
//! other volumetrics, and shadow configuration.

extern crate alloc;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_asset::{AssetApp, AssetEventSystems};
use bevy_camera::{
    primitives::{Aabb, CascadesFrusta, CubemapFrusta, Frustum, Sphere},
    visibility::{
        CascadesVisibleEntities, CubemapVisibleEntities, InheritedVisibility, NoCpuCulling,
        NoFrustumCulling, RenderLayers, ViewVisibility, VisibilityRange, VisibilitySystems,
        VisibleEntities, VisibleEntityRanges, VisibleMeshEntities,
    },
    Camera, Camera3d, CameraUpdateSystems, RenderTarget, ShadowLodOrigin,
};
use bevy_ecs::{entity::EntityHashSet, prelude::*, system::QueryLens};
#[cfg(feature = "bevy_gizmos")]
use bevy_gizmos::frustum::FrustumGizmoSystems;
use bevy_log::warn_once;
use bevy_math::Vec3A;
use bevy_mesh::Mesh3d;
use bevy_reflect::prelude::*;
use bevy_transform::{components::GlobalTransform, TransformSystems};
use bevy_utils::Parallel;
use core::{any::TypeId, mem, ops::DerefMut};
use smallvec::{smallvec, SmallVec};

pub mod cluster;
use cluster::assign::assign_objects_to_clusters;
pub use cluster::ClusteredDecal;
mod ambient_light;
pub use ambient_light::{AmbientLight, GlobalAmbientLight};
use bevy_camera::visibility::SetViewVisibility;

mod probe;
pub use probe::{
    automatically_add_parallax_correction_components, AtmosphereEnvironmentMapLight,
    EnvironmentMapLight, GeneratedEnvironmentMapLight, IrradianceVolume, LightProbe,
    ParallaxCorrection, Skybox,
};
pub mod atmosphere;
pub use atmosphere::Atmosphere;
mod volumetric;
pub use volumetric::{FogVolume, VolumetricFog, VolumetricLight};
pub mod cascade;
use cascade::build_directional_light_cascades;
pub use cascade::{CascadeShadowConfig, CascadeShadowConfigBuilder, Cascades};
mod point_light;
pub use point_light::{
    update_point_light_frusta, PointLight, PointLightShadowMap, PointLightTexture,
};
mod spot_light;
pub use spot_light::{
    orthonormalize, spot_light_clip_from_view, spot_light_world_from_view,
    update_spot_light_frusta, SpotLight, SpotLightTexture,
};
mod directional_light;
pub use directional_light::{
    update_directional_light_frusta, DirectionalLight, DirectionalLightShadowMap,
    DirectionalLightTexture, SunDisk,
};
mod rect_light;
pub use rect_light::RectLight;
/// Provides gizmo drawing for visualizing light positions.
#[cfg(feature = "bevy_gizmos")]
pub mod gizmos;

/// The light prelude.
///
/// This includes the most common types in this crate, re-exported for your convenience.
pub mod prelude {
    #[doc(hidden)]
    pub use crate::{
        light_consts, AmbientLight, DirectionalLight, EnvironmentMapLight,
        GeneratedEnvironmentMapLight, GlobalAmbientLight, LightProbe, PointLight, RectLight,
        SpotLight,
    };

    #[doc(hidden)]
    #[cfg(feature = "bevy_gizmos")]
    pub use crate::gizmos::{LightGizmoColor, LightGizmoConfigGroup, ShowLightGizmo};
}

use crate::{
    atmosphere::{extract_chromatic_phase_textures, ScatteringMedium},
    cluster::{add_light_probe_and_decal_aabbs, ClusterVisibilityClass, Clusters},
    directional_light::validate_shadow_map_size,
    point_light::update_point_light_bounding_spheres,
    spot_light::update_spot_light_bounding_spheres,
};

/// Constants for operating with the light units: lumens, and lux.
pub mod light_consts {
    /// Approximations for converting the wattage of lamps to lumens.
    ///
    /// The **lumen** (symbol: **lm**) is the unit of [luminous flux], a measure
    /// of the total quantity of [visible light] emitted by a source per unit of
    /// time, in the [International System of Units] (SI).
    ///
    /// For more information, see [wikipedia](https://en.wikipedia.org/wiki/Lumen_(unit))
    ///
    /// [luminous flux]: https://en.wikipedia.org/wiki/Luminous_flux
    /// [visible light]: https://en.wikipedia.org/wiki/Visible_light
    /// [International System of Units]: https://en.wikipedia.org/wiki/International_System_of_Units
    pub mod lumens {
        /// The conversion factor used to determine how many lumens a typical LED light of a given wattage produces.
        pub const LUMENS_PER_LED_WATTS: f32 = 90.0;
        /// The conversion factor used to determine how many lumens a typical incandescent light of a given wattage produces.
        pub const LUMENS_PER_INCANDESCENT_WATTS: f32 = 13.8;
        /// The conversion factor used to determine how many lumens a typical halogen light of a given wattage produces.
        pub const LUMENS_PER_HALOGEN_WATTS: f32 = 19.8;
        /// 1,000,000 lumens is a very large "cinema light" capable of registering brightly at Bevy's
        /// default [`bevy_camera::Exposure::BLENDER`] exposure level. For "indoor lighting" with a lower exposure,
        /// this would be way too bright.
        pub const VERY_LARGE_CINEMA_LIGHT: f32 = 1_000_000.0;
    }

    /// Predefined for lux values in several locations.
    ///
    /// The **lux** (symbol: **lx**) is the unit of [illuminance], or [luminous flux] per unit area,
    /// in the [International System of Units] (SI). It is equal to one lumen per square meter.
    ///
    /// For more information, see [wikipedia](https://en.wikipedia.org/wiki/Lux)
    ///
    /// [illuminance]: https://en.wikipedia.org/wiki/Illuminance
    /// [luminous flux]: https://en.wikipedia.org/wiki/Luminous_flux
    /// [International System of Units]: https://en.wikipedia.org/wiki/International_System_of_Units
    pub mod lux {
        /// The amount of light (lux) in a moonless, overcast night sky. (starlight)
        pub const MOONLESS_NIGHT: f32 = 0.0001;
        /// The amount of light (lux) during a full moon on a clear night.
        pub const FULL_MOON_NIGHT: f32 = 0.05;
        /// The amount of light (lux) during the dark limit of civil twilight under a clear sky.
        pub const CIVIL_TWILIGHT: f32 = 3.4;
        /// The amount of light (lux) in family living room lights.
        pub const LIVING_ROOM: f32 = 50.;
        /// The amount of light (lux) in an office building's hallway/toilet lighting.
        pub const HALLWAY: f32 = 80.;
        /// The amount of light (lux) in very dark overcast day
        pub const DARK_OVERCAST_DAY: f32 = 100.;
        /// The amount of light (lux) in an office.
        pub const OFFICE: f32 = 320.;
        /// The amount of light (lux) during sunrise or sunset on a clear day.
        pub const CLEAR_SUNRISE: f32 = 400.;
        /// The amount of light (lux) on an overcast day; typical TV studio lighting
        pub const OVERCAST_DAY: f32 = 1000.;
        /// The amount of light (lux) from ambient daylight (not direct sunlight).
        /// This is the default for [`DirectionalLight`](crate::DirectionalLight)s in Bevy.
        pub const AMBIENT_DAYLIGHT: f32 = 10_000.;
        /// The amount of light (lux) in full daylight (not direct sun).
        pub const FULL_DAYLIGHT: f32 = 20_000.;
        /// The amount of light (lux) in direct sunlight.
        pub const DIRECT_SUNLIGHT: f32 = 100_000.;
        /// The amount of light (lux) of raw sunlight, not filtered by the atmosphere.
        pub const RAW_SUNLIGHT: f32 = 130_000.;
    }
}

/// Sets up all the light visibility and clustering infrastructure needed for rendering lights.
#[derive(Default)]
pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GlobalAmbientLight>()
            .init_resource::<DirectionalLightShadowMap>()
            .init_resource::<PointLightShadowMap>()
            .init_asset::<ScatteringMedium>()
            .register_required_components::<Camera3d, Clusters>()
            .configure_sets(
                PostUpdate,
                SimulationLightSystems::CheckLightVisibility
                    .ambiguous_with(SimulationLightSystems::CheckLightVisibility),
            )
            .configure_sets(
                PostUpdate,
                SimulationLightSystems::AssignLightsToClusters
                    .before(bevy_app::TransformGizmoRenderStep),
            )
            .add_systems(Update, automatically_add_parallax_correction_components)
            .add_systems(
                PostUpdate,
                (
                    validate_shadow_map_size.before(build_directional_light_cascades),
                    assign_objects_to_clusters
                        .in_set(SimulationLightSystems::AssignLightsToClusters)
                        .after(TransformSystems::Propagate)
                        .after(VisibilitySystems::CheckVisibility)
                        .after(CameraUpdateSystems),
                    update_directional_light_frusta
                        .in_set(SimulationLightSystems::UpdateLightFrusta)
                        // This must run after CheckVisibility because it relies on `ViewVisibility`
                        .after(VisibilitySystems::CheckVisibility)
                        .after(TransformSystems::Propagate)
                        .after(SimulationLightSystems::UpdateDirectionalLightCascades)
                        // We assume that no entity will be both a directional light and a spot light,
                        // so these systems will run independently of one another.
                        // FIXME: Add an archetype invariant for this https://github.com/bevyengine/bevy/issues/1481.
                        .ambiguous_with(update_spot_light_frusta),
                    update_point_light_frusta
                        .in_set(SimulationLightSystems::UpdateLightFrusta)
                        .after(TransformSystems::Propagate)
                        .after(SimulationLightSystems::AssignLightsToClusters),
                    #[cfg(feature = "bevy_gizmos")]
                    update_spot_light_frusta
                        .in_set(SimulationLightSystems::UpdateLightFrusta)
                        .before(FrustumGizmoSystems)
                        .after(TransformSystems::Propagate)
                        .after(SimulationLightSystems::AssignLightsToClusters),
                    #[cfg(not(feature = "bevy_gizmos"))]
                    update_spot_light_frusta
                        .in_set(SimulationLightSystems::UpdateLightFrusta)
                        .after(TransformSystems::Propagate)
                        .after(SimulationLightSystems::AssignLightsToClusters),
                    (
                        check_dir_light_mesh_visibility,
                        check_point_light_mesh_visibility,
                    )
                        .in_set(SimulationLightSystems::CheckLightVisibility)
                        .after(VisibilitySystems::CalculateBounds)
                        .after(TransformSystems::Propagate)
                        .after(SimulationLightSystems::UpdateLightFrusta)
                        // Lights can "see" entities and mark them as visible. This is done to
                        // correctly render shadows for entities that are not in view of a camera,
                        // but must be renderable to cast shadows. Because of this, we need to check
                        // entity visibility and mark as visible before they can be hidden.
                        .after(VisibilitySystems::CheckVisibility)
                        .before(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                    (
                        update_point_light_bounding_spheres.after(TransformSystems::Propagate),
                        update_spot_light_bounding_spheres.after(TransformSystems::Propagate),
                        add_light_probe_and_decal_aabbs,
                    )
                        .in_set(SimulationLightSystems::UpdateBounds)
                        .before(VisibilitySystems::UpdateFrusta),
                    build_directional_light_cascades
                        .in_set(SimulationLightSystems::UpdateDirectionalLightCascades)
                        .after(TransformSystems::Propagate)
                        .before(bevy_app::TransformGizmoRenderStep)
                        .after(CameraUpdateSystems),
                    extract_chromatic_phase_textures.after(AssetEventSystems),
                ),
            );

        #[cfg(feature = "bevy_gizmos")]
        app.add_plugins(gizmos::LightGizmoPlugin);
    }
}

/// A convenient alias for `Or<(With<PointLight>, With<SpotLight>,
/// With<DirectionalLight>, With<RectLight>)>`, for use with [`bevy_camera::visibility::VisibleEntities`].
pub type WithLight = Or<(
    With<PointLight>,
    With<SpotLight>,
    With<DirectionalLight>,
    With<RectLight>,
)>;

/// Add this component to make a [`Mesh3d`] not cast shadows.
#[derive(Debug, Component, Reflect, Default, Clone, PartialEq)]
#[reflect(Component, Default, Debug, Clone, PartialEq)]
pub struct NotShadowCaster;
/// Add this component to make a [`Mesh3d`] not receive shadows.
///
/// **Note:** If you're using diffuse transmission, setting [`NotShadowReceiver`] will
/// cause both “regular” shadows as well as diffusely transmitted shadows to be disabled,
/// even when [`TransmittedShadowReceiver`] is being used.
#[derive(Debug, Component, Reflect, Default, Clone)]
#[reflect(Component, Default, Debug)]
pub struct NotShadowReceiver;
/// Add this component to make a [`Mesh3d`] using a PBR material with `StandardMaterial::diffuse_transmission > 0.0`
/// receive shadows on its diffuse transmission lobe. (i.e. its “backside”)
///
/// Not enabled by default, as it requires carefully setting up `StandardMaterial::thickness`
/// (and potentially even baking a thickness texture!) to match the geometry of the mesh, in order to avoid self-shadow artifacts.
///
/// **Note:** Using [`NotShadowReceiver`] overrides this component.
#[derive(Debug, Component, Reflect, Default, Clone)]
#[reflect(Component, Default, Debug)]
pub struct TransmittedShadowReceiver;

/// Add this component to a [`Camera3d`]
/// to control how to anti-alias shadow edges.
///
/// The different modes use different approaches to
/// [Percentage Closer Filtering](https://developer.nvidia.com/gpugems/gpugems/part-ii-lighting-and-shadows/chapter-11-shadow-map-antialiasing).
#[derive(Debug, Component, Reflect, Clone, Copy, PartialEq, Eq, Default)]
#[reflect(Component, Default, Debug, PartialEq, Clone)]
pub enum ShadowFilteringMethod {
    /// Hardware 2x2.
    ///
    /// Fast but poor quality.
    Hardware2x2,
    /// Approximates a fixed Gaussian blur, good when TAA isn't in use.
    ///
    /// Good quality, good performance.
    ///
    /// For directional and spot lights, this uses a [method by Ignacio Castaño
    /// for *The Witness*] using 9 samples and smart filtering to achieve the same
    /// as a regular 5x5 filter kernel.
    ///
    /// [method by Ignacio Castaño for *The Witness*]: https://web.archive.org/web/20230210095515/http://the-witness.net/news/2013/09/shadow-mapping-summary-part-1/
    #[default]
    Gaussian,
    /// A randomized filter that varies over time, good when TAA is in use.
    ///
    /// Good quality when used with `TemporalAntiAliasing`
    /// and good performance.
    ///
    /// For directional and spot lights, this uses a [method by Jorge Jimenez for
    /// *Call of Duty: Advanced Warfare*] using 8 samples in spiral pattern,
    /// randomly-rotated by interleaved gradient noise with spatial variation.
    ///
    /// [method by Jorge Jimenez for *Call of Duty: Advanced Warfare*]: https://www.iryoku.com/next-generation-post-processing-in-call-of-duty-advanced-warfare/
    Temporal,
}

/// System sets used to run light-related systems.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum SimulationLightSystems {
    /// The set that adds AABBs and bounding spheres to clustered objects.
    UpdateBounds,
    /// After this set, all lights have been clustered.
    AssignLightsToClusters,
    /// After this set, all directional light cascades are up to date.
    UpdateDirectionalLightCascades,
    /// After this set, the frusta of shadow-casting point lights, spot lights, and directional lights are up to date.
    UpdateLightFrusta,
    /// System order ambiguities between systems in this set are ignored:
    /// the order of systems within this set is irrelevant, as the various visibility-checking systems
    /// assumes that their operations are irreversible during the frame.
    CheckLightVisibility,
}

/// NVForever small casters (graphics pass 2026-11-23): a sun shadow caster whose bounding
/// radius is below this many texels of a cascade's map is left out of that cascade. With
/// the Smooth filter's 3x3 kernel such a shadow is a faint smudge of a few texels at most.
/// `NVF_SMALL_CASTER_TEXELS=<n>` (0 turns it off); default 1.
pub fn nvf_small_caster_texels() -> f32 {
    static TEXELS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *TEXELS.get_or_init(|| std::env::var("NVF_SMALL_CASTER_TEXELS").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32).max(0.0))
}

/// Updates the visibility for [`DirectionalLight`]s so that shadow map
/// rendering can work.
///
/// This only processes entities without [`NoCpuCulling`]. Entities with
/// NVForever: when secondary cameras (water mirror and/or B42 Optics extra cameras) share
/// the main camera's directional shadow maps this frame, `share_mirror_casters` overwrites
/// their caster lists right after `check_dir_light_mesh_visibility`, and held cascades
/// (`HoldDirectionalShadowMaps`) keep their previous frame's caster lists intact instead of
/// re-culling 10k+ meshes [design].
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvfSkipDirLightShadowView {
    /// Views whose directional shadow caster lists are overwritten by `share_mirror_casters`.
    pub views: [Option<Entity>; 4],
    /// Bitmask of held directional shadow cascades whose previous caster lists stay intact.
    pub held_mask: u32,
}

impl NvfSkipDirLightShadowView {
    /// Returns true if `view` skips directional light shadow culling this frame.
    pub fn skips(&self, view: Entity) -> bool {
        self.views.contains(&Some(view))
    }

    /// Returns true if any view is currently registered to skip culling.
    pub fn any_views(&self) -> bool {
        self.views.iter().any(Option::is_some)
    }

    /// Clears all registered skip views.
    pub fn clear_views(&mut self) {
        self.views = [None; 4];
    }

    /// Registers `view` to skip directional light shadow culling (up to 4 views).
    pub fn push_view(&mut self, view: Entity) {
        if self.skips(view) {
            return;
        }
        if let Some(slot) = self.views.iter_mut().find(|s| s.is_none()) {
            *slot = Some(view);
        }
    }
}

pub use bevy_camera::visibility::NvfSpatialMeshGrid;

impl NvfDirLightSliceHull {
    /// Returns true if the 2D light-space box `(cx ± rx, cy ± ry)` intersects this hull.
    #[inline]
    pub fn intersects_box(&self, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
        let centre = bevy_math::Vec2::new(cx, cy);
        let reach = bevy_math::Vec2::new(rx + self.margin, ry + self.margin);
        self.edges
            .iter()
            .all(|&(n, n_abs, limit)| n.dot(centre) - n_abs.dot(reach) <= limit)
    }
}

/// NVForever: per-cascade 2D light-space convex hull of the camera frustum slice
/// (`sky::prepare_sun_slice_hulls`). Checking it inside `check_dir_light_mesh_visibility`
/// before pushing to `defer_visible_entities_queue` prevents thousands of shadow casters
/// outside the slice hull from being marked `ViewVisibility::VISIBLE` on drawn frames and
/// then flipped back to `HIDDEN` on held cascade frames (avoiding GPU mesh instance churn)
/// [design].
#[derive(Clone, Debug)]
pub struct NvfDirLightSliceHull {
    /// Light-space X basis axis in world coordinates.
    pub lx: Vec3A,
    /// Light-space Y basis axis in world coordinates.
    pub ly: Vec3A,
    /// Conservative margin in world units around the slice outline for shadow filtering.
    pub margin: f32,
    /// Precomputed convex-hull edges `(outward_normal, abs_normal, support_limit)`.
    pub edges: Vec<(bevy_math::Vec2, bevy_math::Vec2, f32)>,
}

/// Resource storing per-`(light, view)` cascade slice hulls for [`check_dir_light_mesh_visibility`].
#[derive(Resource, Default, Clone, Debug)]
pub struct NvfDirLightSliceHulls {
    /// `(light_entity, view_entity, per_cascade_hulls)` entries for the current frame.
    pub entries: Vec<(Entity, Entity, Vec<Option<NvfDirLightSliceHull>>)>,
}

/// [`NoCpuCulling`] receive no view-specific processing in the main world.
pub fn check_dir_light_mesh_visibility(
    mut commands: Commands,
    mut directional_lights: Query<
        (
            Entity,
            &DirectionalLight,
            &CascadesFrusta,
            &mut CascadesVisibleEntities,
            Option<&RenderLayers>,
            &ViewVisibility,
            // NVForever small casters: each cascade's texel size.
            Option<&Cascades>,
        ),
        Without<SpotLight>,
    >,
    visible_entity_query: Query<
        (
            Entity,
            &InheritedVisibility,
            Option<&RenderLayers>,
            Option<&Aabb>,
            Option<&GlobalTransform>,
            Has<VisibilityRange>,
            Has<NoFrustumCulling>,
        ),
        (
            Without<NotShadowCaster>,
            Without<DirectionalLight>,
            Without<NoCpuCulling>,
            With<Mesh3d>,
        ),
    >,
    visible_entity_ranges: Option<Res<VisibleEntityRanges>>,
    skip_shadow_view: Option<Res<NvfSkipDirLightShadowView>>,
    slice_hulls: Option<Res<NvfDirLightSliceHulls>>,
    spatial_grid: Option<Res<NvfSpatialMeshGrid>>,
    mut defer_visible_entities_queue: Local<Parallel<Vec<Entity>>>,
    mut view_visible_entities_queue: Local<Parallel<Vec<Vec<Entity>>>>,
) {
    let visible_entity_ranges = visible_entity_ranges.as_deref();
    let skip_shadow_view = skip_shadow_view.as_deref();
    let held_mask = skip_shadow_view.map_or(0, |s| s.held_mask);
    let slice_hulls = slice_hulls.as_deref();
    let spatial_grid = spatial_grid.as_deref().filter(|g| g.ready);

    let small_texels = nvf_small_caster_texels();
    for (
        light_entity,
        directional_light,
        frusta,
        mut visible_entities,
        maybe_view_mask,
        light_view_visibility,
        cascades,
    ) in &mut directional_lights
    {
        let mut had_matching_view = Vec::with_capacity(visible_entities.entities.len());
        let mut views_to_remove = Vec::new();
        for (view, cascade_view_entities) in &mut visible_entities.entities {
            match frusta.frusta.get(view) {
                Some(view_frusta) => {
                    if cascade_view_entities.len() == view_frusta.len() {
                        had_matching_view.push(*view);
                    }
                    cascade_view_entities.resize(view_frusta.len(), Default::default());
                }
                None => views_to_remove.push(*view),
            };
        }
        for (view, frusta) in &frusta.frusta {
            visible_entities
                .entities
                .entry(*view)
                .or_insert_with(|| vec![VisibleMeshEntities::default(); frusta.len()]);
        }

        for v in views_to_remove {
            visible_entities.entities.remove(&v);
        }

        // NOTE: If shadow mapping is disabled for the light then it must have no visible entities
        if !directional_light.shadow_maps_enabled || !light_view_visibility.get() {
            visible_entities.entities.clear();
            continue;
        }

        let view_mask = maybe_view_mask.unwrap_or_default();
        let view_has_default_layer = view_mask.intersects(&RenderLayers::default());

        for (view, view_frusta) in &frusta.frusta {
            if skip_shadow_view.is_some_and(|s| s.skips(*view)) {
                continue;
            }
            let view_held_mask = if held_mask != 0 && had_matching_view.contains(view) {
                held_mask
            } else {
                0
            };
            let any_active = (0..view_frusta.len()).any(|c| c >= 32 || (view_held_mask & (1u32 << c)) == 0);
            if any_active {
                // NVForever small casters: per cascade, squared bounding radius below which a
                // caster is left out of its map (0: none), avoiding 4 sqrts per mesh [design].
                let mut min_radius_sq = [0.0f32; 8];
                // NVForever fast ortho X/Y pre-reject: light-space orthographic rows 0 and 1
                // from `Cascade::clip_from_world` so far meshes reject before full 6-plane
                // OBB frustum intersection. When all active cascades share the same light-space
                // X/Y axes (the standard directional-light case), project each mesh once onto
                // the shared `(lx, ly)` axes, pre-reject against the union of all active
                // cascades, and test each cascade with 2 scalar compares [design].
                let mut ortho_xy: [Option<(Vec3A, f32, Vec3A, f32)>; 8] = [None; 8];
                let mut shared_axes: Option<(Vec3A, Vec3A)> = None;
                let mut shared_ok = true;
                let mut shared_bounds: [Option<(f32, f32, f32, f32)>; 8] = [None; 8];
                let mut union_min_x = f32::INFINITY;
                let mut union_max_x = f32::NEG_INFINITY;
                let mut union_min_y = f32::INFINITY;
                let mut union_max_y = f32::NEG_INFINITY;
                let mut min_any_radius_sq = f32::INFINITY;
                if let Some(cs) = cascades.and_then(|c| c.cascades.get(view)) {
                    for (idx, c) in cs.iter().enumerate().take(8) {
                        let r = small_texels * c.texel_size;
                        min_radius_sq[idx] = r * r;
                        let is_active = idx < view_frusta.len()
                            && (idx >= 32 || (view_held_mask & (1u32 << idx)) == 0);
                        if is_active {
                            min_any_radius_sq = min_any_radius_sq.min(min_radius_sq[idx]);
                        }
                        let m = c.clip_from_world;
                        let r3 = m.row(3);
                        if (r3 - bevy_math::Vec4::W).length_squared() < 1e-6 {
                            let r0 = m.row(0);
                            let r1 = m.row(1);
                            let r0_xyz = Vec3A::new(r0.x, r0.y, r0.z);
                            let r1_xyz = Vec3A::new(r1.x, r1.y, r1.z);
                            let len0 = r0_xyz.length();
                            let len1 = r1_xyz.length();
                            if len0 > 0.0 && len1 > 0.0 {
                                ortho_xy[idx] = Some((r0_xyz, r0.w, r1_xyz, r1.w));
                                if is_active && shared_ok {
                                    let dir0 = r0_xyz / len0;
                                    let dir1 = r1_xyz / len1;
                                    let (lx, ly) = *shared_axes.get_or_insert((dir0, dir1));
                                    if (dir0 - lx).length_squared() < 1e-6
                                        && (dir1 - ly).length_squared() < 1e-6
                                    {
                                        // `cx = len0 * (lx.dot(world_c)) + r0.w`; `|cx| <= 1.0001 + len0 * rx_ls`
                                        let inv0 = 1.0 / len0;
                                        let inv1 = 1.0 / len1;
                                        let c_min_x = (-1.0001 - r0.w) * inv0;
                                        let c_max_x = (1.0001 - r0.w) * inv0;
                                        let c_min_y = (-1.0001 - r1.w) * inv1;
                                        let c_max_y = (1.0001 - r1.w) * inv1;
                                        shared_bounds[idx] = Some((c_min_x, c_max_x, c_min_y, c_max_y));
                                        union_min_x = union_min_x.min(c_min_x);
                                        union_max_x = union_max_x.max(c_max_x);
                                        union_min_y = union_min_y.min(c_min_y);
                                        union_max_y = union_max_y.max(c_max_y);
                                    } else {
                                        shared_ok = false;
                                    }
                                }
                            } else if is_active {
                                shared_ok = false;
                            }
                        } else if is_active {
                            shared_ok = false;
                        }
                    }
                    // Only enable union reject if every active cascade has shared bounds.
                    for idx in 0..view_frusta.len() {
                        if idx >= 32 || (view_held_mask & (1u32 << idx)) == 0 {
                            if idx >= 8 || shared_bounds[idx].is_none() {
                                shared_ok = false;
                                break;
                            }
                        }
                    }
                } else {
                    shared_ok = false;
                }
                if !min_any_radius_sq.is_finite() {
                    min_any_radius_sq = 0.0;
                }
                let shared_union = if shared_ok {
                    shared_axes.map(|(lx, ly)| {
                        (lx, ly, union_min_x, union_max_x, union_min_y, union_max_y)
                    })
                } else {
                    None
                };
                let view_slice_hulls: Option<&[Option<NvfDirLightSliceHull>]> = slice_hulls
                    .and_then(|h| {
                        h.entries
                            .iter()
                            .find(|(l, v, _)| *l == light_entity && *v == *view)
                            .map(|(_, _, list)| list.as_slice())
                    });
                // Check once per cascade whether its slice hull shares `(lx, ly)` with
                // `shared_union` so we can reuse `(cx, rx, cy, ry)` with 0 extra dot
                // products and pre-reject entire spatial grid cells before entity tests [design].
                let mut aligned_hulls: [Option<&NvfDirLightSliceHull>; 8] = [None; 8];
                let mut all_active_aligned_hulls = shared_union.is_some();
                if let Some((lx, ly, ..)) = shared_union {
                    for cascade in 0..view_frusta.len() {
                        if cascade < 32 && (view_held_mask & (1u32 << cascade)) != 0 {
                            continue;
                        }
                        if cascade < 8
                            && let Some(Some(hull)) =
                                view_slice_hulls.and_then(|list| list.get(cascade))
                            && (hull.lx - lx).length_squared() < 1e-5
                            && (hull.ly - ly).length_squared() < 1e-5
                        {
                            aligned_hulls[cascade] = Some(hull);
                        } else {
                            all_active_aligned_hulls = false;
                        }
                    }
                }

                // Shared per-entity culling logic used by both the spatial-grid fast path
                // and the fallback parallel world scan.
                let cull_entity = |defer_local: &mut Vec<Entity>,
                                   cascade_queues: &mut [Vec<Entity>],
                                   (
                    entity,
                    inherited_visibility,
                    maybe_entity_mask,
                    maybe_aabb,
                    maybe_transform,
                    has_visibility_range,
                    has_no_frustum_culling,
                ): (
                    Entity,
                    &InheritedVisibility,
                    Option<&RenderLayers>,
                    Option<&Aabb>,
                    Option<&GlobalTransform>,
                    bool,
                    bool,
                )| {
                    if !inherited_visibility.get() {
                        return;
                    }

                    let layer_ok = match maybe_entity_mask {
                        None => view_has_default_layer,
                        Some(entity_mask) => view_mask.intersects(entity_mask),
                    };
                    if !layer_ok {
                        return;
                    }

                    // Check visibility ranges.
                    if has_visibility_range
                        && visible_entity_ranges.is_some_and(|visible_entity_ranges| {
                            !visible_entity_ranges.entity_is_in_range_of_view(entity, *view)
                        })
                    {
                        return;
                    }

                    if let (Some(aabb), Some(transform)) = (maybe_aabb, maybe_transform) {
                        let mut visible = false;
                        // NVForever small casters: squared world bounding radius, hoisted affine.
                        let affine = transform.affine();
                        let ax = affine.matrix3.x_axis;
                        let ay = affine.matrix3.y_axis;
                        let az = affine.matrix3.z_axis;
                        let scale_sq = ax
                            .length_squared()
                            .max(ay.length_squared())
                            .max(az.length_squared());
                        let radius_sq = aabb.half_extents.length_squared() * scale_sq;
                        if !has_no_frustum_culling && radius_sq < min_any_radius_sq {
                            return;
                        }
                        let world_c = affine.transform_point3a(aabb.center);
                        let h = aabb.half_extents;
                        // Project once onto shared light-space X/Y axes and pre-reject
                        // against the union of all active cascades before entering the loop.
                        let shared_xy = if !has_no_frustum_culling {
                            if let Some((lx, ly, umin_x, umax_x, umin_y, umax_y)) = shared_union {
                                let cx = lx.dot(world_c);
                                let rx = lx.dot(ax).abs() * h.x
                                    + lx.dot(ay).abs() * h.y
                                    + lx.dot(az).abs() * h.z;
                                if cx + rx < umin_x || cx - rx > umax_x {
                                    return;
                                }
                                let cy = ly.dot(world_c);
                                let ry = ly.dot(ax).abs() * h.x
                                    + ly.dot(ay).abs() * h.y
                                    + ly.dot(az).abs() * h.z;
                                if cy + ry < umin_y || cy - ry > umax_y {
                                    return;
                                }
                                Some((cx, rx, cy, ry))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        for (cascade, (frustum, frustum_visible_entities)) in view_frusta
                            .iter()
                            .zip(cascade_queues.iter_mut())
                            .enumerate()
                        {
                            if cascade < 32 && (view_held_mask & (1u32 << cascade)) != 0 {
                                continue;
                            }
                            // NVForever small casters: under about a texel of this cascade's
                            // map, a caster's shadow is filtered away; it is left out.
                            if !has_no_frustum_culling
                                && radius_sq < min_radius_sq.get(cascade).copied().unwrap_or(0.0)
                            {
                                continue;
                            }
                            if let Some((cx, rx, cy, ry)) = shared_xy {
                                if let Some(Some((c_min_x, c_max_x, c_min_y, c_max_y))) =
                                    shared_bounds.get(cascade)
                                {
                                    if cx + rx < *c_min_x
                                        || cx - rx > *c_max_x
                                        || cy + ry < *c_min_y
                                        || cy - ry > *c_max_y
                                    {
                                        continue;
                                    }
                                }
                                // Aligned slice hull test BEFORE 6-plane OBB math (0 extra dots).
                                if let Some(Some(hull)) = aligned_hulls.get(cascade) {
                                    if !hull.intersects_box(cx, cy, rx, ry) {
                                        continue;
                                    }
                                }
                            } else if !has_no_frustum_culling
                                && let Some(Some((r0_xyz, r0_w, r1_xyz, r1_w))) =
                                    ortho_xy.get(cascade)
                            {
                                let cx = r0_xyz.dot(world_c) + *r0_w;
                                let rx = r0_xyz.dot(ax).abs() * h.x
                                    + r0_xyz.dot(ay).abs() * h.y
                                    + r0_xyz.dot(az).abs() * h.z;
                                if cx.abs() > 1.0001 + rx {
                                    continue;
                                }
                                let cy = r1_xyz.dot(world_c) + *r1_w;
                                let ry = r1_xyz.dot(ax).abs() * h.x
                                    + r1_xyz.dot(ay).abs() * h.y
                                    + r1_xyz.dot(az).abs() * h.z;
                                if cy.abs() > 1.0001 + ry {
                                    continue;
                                }
                            }
                            // Disable near-plane culling, as a shadow caster could lie before the near plane.
                            if !has_no_frustum_culling
                                && !frustum.intersects_obb(aabb, &affine, false, true)
                            {
                                continue;
                            }
                            // Fallback slice hull check only when not already checked via `aligned_hulls`.
                            let already_checked_hull = shared_xy.is_some()
                                && aligned_hulls.get(cascade).is_some_and(Option::is_some);
                            if !already_checked_hull
                                && let Some(Some(hull)) =
                                    view_slice_hulls.and_then(|list| list.get(cascade))
                            {
                                let cx = hull.lx.dot(world_c);
                                let rx = hull.lx.dot(ax).abs() * h.x
                                    + hull.lx.dot(ay).abs() * h.y
                                    + hull.lx.dot(az).abs() * h.z;
                                let cy = hull.ly.dot(world_c);
                                let ry = hull.ly.dot(ax).abs() * h.x
                                    + hull.ly.dot(ay).abs() * h.y
                                    + hull.ly.dot(az).abs() * h.z;
                                if !hull.intersects_box(cx, cy, rx, ry) {
                                    continue;
                                }
                            }
                            visible = true;

                            frustum_visible_entities.push(entity);
                        }
                        if visible {
                            defer_local.push(entity);
                        }
                    } else {
                        defer_local.push(entity);
                        for (cascade, frustum_visible_entities) in
                            cascade_queues.iter_mut().enumerate()
                        {
                            if cascade < 32 && (view_held_mask & (1u32 << cascade)) != 0 {
                                continue;
                            }
                            frustum_visible_entities.push(entity);
                        }
                    }
                };

                // Resize any per-thread buffer left at a stale state from a prior frame.
                for thread_queue in view_visible_entities_queue.iter_mut() {
                    thread_queue.resize(view_frusta.len(), Vec::default());
                }

                // Two-level spatial-grid front-end [design]: reject entire 1024-unit cells
                // outside the active cascade union bounds & aligned slice hulls before
                // touching per-entity ECS components.
                if let (Some(grid), Some((lx, ly, umin_x, umax_x, umin_y, umax_y))) =
                    (spatial_grid, shared_union)
                {
                    let he = grid.cell_half_extent;
                    let cell_rx = (lx.x.abs() + lx.y.abs() + lx.z.abs()) * he;
                    let cell_ry = (ly.x.abs() + ly.y.abs() + ly.z.abs()) * he;
                    let mut defer_local = defer_visible_entities_queue.borrow_local_mut();
                    let mut cascade_queues = view_visible_entities_queue.borrow_local_mut();
                    cascade_queues.resize(view_frusta.len(), Vec::default());

                    for item in visible_entity_query.iter_many(&grid.everywhere).matched() {
                        cull_entity(&mut defer_local, &mut cascade_queues, item);
                    }
                    for (_, cell_center, entities) in &grid.cells {
                        let cell_cx = lx.dot(*cell_center);
                        if cell_cx + cell_rx < umin_x || cell_cx - cell_rx > umax_x {
                            continue;
                        }
                        let cell_cy = ly.dot(*cell_center);
                        if cell_cy + cell_ry < umin_y || cell_cy - cell_ry > umax_y {
                            continue;
                        }
                        if all_active_aligned_hulls {
                            let any_hull = aligned_hulls
                                .iter()
                                .flatten()
                                .any(|hull| hull.intersects_box(cell_cx, cell_cy, cell_rx, cell_ry));
                            if !any_hull {
                                continue;
                            }
                        }
                        for item in visible_entity_query.iter_many(entities).matched() {
                            cull_entity(&mut defer_local, &mut cascade_queues, item);
                        }
                    }
                } else {
                    visible_entity_query.par_iter().for_each_init(
                        || {
                            let mut entities = view_visible_entities_queue.borrow_local_mut();
                            entities.resize(view_frusta.len(), Vec::default());
                            (defer_visible_entities_queue.borrow_local_mut(), entities)
                        },
                        |(defer_visible_entities_local_queue, view_visible_entities_local_queue),
                         item| {
                            cull_entity(
                                defer_visible_entities_local_queue,
                                view_visible_entities_local_queue,
                                item,
                            );
                        },
                    );
                }
            }
            // collect entities from parallel queue (keeping held cascades' lists intact)
            let mut held_defer = defer_visible_entities_queue.borrow_local_mut();
            for (view_dest_index, view_dest) in visible_entities
                .entities
                .get_mut(view)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                if view_dest_index < 32 && (view_held_mask & (1u32 << view_dest_index)) != 0 {
                    held_defer.extend_from_slice(&view_dest.entities);
                    continue;
                }
                view_dest.entities.clear();
                for thread_entity_queue in view_visible_entities_queue.iter_mut() {
                    view_dest
                        .entities
                        .append(&mut thread_entity_queue[view_dest_index]);
                }
                view_dest.shrink();
                view_dest.entities.sort_unstable();
            }
        }
    }

    // Defer marking view visibility so this system can run in parallel with check_point_light_mesh_visibility
    // TODO: use resource to avoid unnecessary memory alloc
    let mut defer_queue = mem::take(defer_visible_entities_queue.deref_mut());
    commands.queue(move |world: &mut World| {
        let mut query = world.query::<&mut ViewVisibility>();
        for entities in defer_queue.iter_mut() {
            let mut iter = query.iter_many_mut(world, entities.iter()).matched();
            while let Some(mut view_visibility) = iter.fetch_next() {
                view_visibility.set_visible();
            }
        }
    });
}

/// Updates the visibility for [`PointLight`]s and [`SpotLight`]s so that shadow
/// map rendering can work.
///
/// This only processes entities without [`NoCpuCulling`]. Entities with
/// [`NoCpuCulling`] receive no view-specific processing in the main world.
pub fn check_point_light_mesh_visibility(
    visible_point_lights: Query<&VisibleEntities>,
    mut point_lights: Query<(
        &PointLight,
        &GlobalTransform,
        &CubemapFrusta,
        &mut CubemapVisibleEntities,
        Option<&RenderLayers>,
    )>,
    mut spot_lights: Query<(
        &SpotLight,
        &GlobalTransform,
        &Frustum,
        &mut VisibleMeshEntities,
        Option<&RenderLayers>,
    )>,
    mut visible_entity_query: Query<
        (
            Entity,
            &InheritedVisibility,
            &mut ViewVisibility,
            Option<&RenderLayers>,
            Option<&Aabb>,
            Option<&GlobalTransform>,
            Has<VisibilityRange>,
            Has<NoFrustumCulling>,
        ),
        (
            Without<NotShadowCaster>,
            Without<DirectionalLight>,
            Without<NoCpuCulling>,
            With<Mesh3d>,
        ),
    >,
    mut camera_query: Query<(Entity, &RenderTarget), With<Camera>>,
    mut shadow_lod_origin_query: Query<Entity, With<ShadowLodOrigin>>,
    mut point_and_spot_light_query: Query<Entity, Or<(With<PointLight>, With<SpotLight>)>>,
    visible_entity_ranges: Option<Res<VisibleEntityRanges>>,
    mut cubemap_visible_entities_queue: Local<Parallel<[Vec<Entity>; 6]>>,
    mut spot_visible_entities_queue: Local<Parallel<Vec<Entity>>>,
    mut checked_lights: Local<EntityHashSet>,
    // NVForever shadow caster candidates.
    nvf_candidates: Option<Res<NvfShadowCasterCandidates>>,
    // NVForever shown shadow lights: the lights shown, and their visibility.
    nvf_shown: Option<Res<NvfShadowLightsShown>>,
    mut nvf_light_visibility: Query<
        &mut ViewVisibility,
        (Or<(With<PointLight>, With<SpotLight>)>, Without<Mesh3d>),
    >,
) {
    checked_lights.clear();

    let shadow_lod_origin = get_shadow_lod_origin(
        camera_query.transmute_lens_filtered(),
        shadow_lod_origin_query.transmute_lens_filtered(),
        point_and_spot_light_query.transmute_lens_filtered(),
    );

    let visible_entity_ranges = visible_entity_ranges.as_deref();
    // NVForever shown shadow lights: after the lights the cameras see, those shown (made
    // visible here, so their shadow maps are extracted and drawn too).
    let nvf_shown = nvf_shown.as_deref().map(|shown| &shown.lights);
    let nvf_lights = visible_point_lights
        .iter()
        .flat_map(|visible_lights| visible_lights.get(TypeId::of::<ClusterVisibilityClass>()).iter().copied())
        .chain(nvf_shown.into_iter().flat_map(|shown| shown.iter().copied()));
    {
        for light_entity in nvf_lights {
            if !checked_lights.insert(light_entity) {
                continue;
            }
            if nvf_shown.is_some_and(|shown| shown.contains(&light_entity)) {
                if let Ok(mut view_visibility) = nvf_light_visibility.get_mut(light_entity) {
                    view_visibility.set_visible();
                }
            }

            // Point lights
            if let Ok((
                point_light,
                transform,
                cubemap_frusta,
                mut cubemap_visible_entities,
                maybe_view_mask,
            )) = point_lights.get_mut(light_entity)
            {
                // NOTE: If shadow mapping is disabled for the light then it must have no visible entities
                if !point_light.shadow_maps_enabled {
                    continue;
                }

                let view_mask = maybe_view_mask.unwrap_or_default();
        let view_has_default_layer = view_mask.intersects(&RenderLayers::default());
                let light_sphere = Sphere {
                    center: Vec3A::from(transform.translation()),
                    radius: point_light.range,
                };

                // NVForever shadow caster candidates: only the light's candidates, with the
                // same tests, instead of every shadow caster in the world.
                if let Some(candidates) = nvf_candidates.as_ref().and_then(|c| c.lights.get(&light_entity)) {
                    let mut faces: [Vec<Entity>; 6] = Default::default();
                    let mut iter = visible_entity_query.iter_many_mut(candidates.iter().copied());
                    while let Some(item) = iter.fetch_next() {
                        // An entity the query does not match (not a caster) is skipped.
                        let Ok((entity, inherited, mut view_visibility, mask, aabb, transform, has_range, no_culling)) = item else {
                            continue;
                        };
                        let caster = NvfCaster { entity, inherited, mask, aabb, transform, has_range, no_culling };
                        let reached = nvf_caster_faces(&caster, view_mask, view_has_default_layer, &light_sphere, &cubemap_frusta.frusta, visible_entity_ranges, shadow_lod_origin);
                        if reached != 0 {
                            view_visibility.set_visible();
                            for (i, face) in faces.iter_mut().enumerate() {
                                if reached & (1 << i) != 0 {
                                    face.push(entity);
                                }
                            }
                        }
                    }
                    for (dest, face) in cubemap_visible_entities.iter_mut().zip(faces) {
                        dest.entities.clear();
                        dest.entities.extend(face);
                        dest.shrink();
                        dest.entities.sort_unstable();
                        dest.entities.dedup();
                    }
                    continue;
                }

                visible_entity_query.par_iter_mut().for_each_init(
                    || cubemap_visible_entities_queue.borrow_local_mut(),
                    |cubemap_visible_entities_local_queue,
                     (
                        entity,
                        inherited_visibility,
                        mut view_visibility,
                        maybe_entity_mask,
                        maybe_aabb,
                        maybe_transform,
                        has_visibility_range,
                        has_no_frustum_culling,
                    )| {
                        if !inherited_visibility.get() {
                            return;
                        }
                        let layer_ok = match maybe_entity_mask {
                            None => view_has_default_layer,
                            Some(entity_mask) => view_mask.intersects(entity_mask),
                        };
                        if !layer_ok {
                            return;
                        }
                        if has_visibility_range
                            && visible_entity_ranges.is_some_and(|visible_entity_ranges| {
                                shadow_lod_origin.is_none_or(|shadow_lod_origin| {
                                    !visible_entity_ranges
                                        .entity_is_in_range_of_view(entity, shadow_lod_origin)
                                })
                            })
                        {
                            return;
                        }

                        // If we have an aabb and transform, do frustum culling
                        if let (Some(aabb), Some(transform)) = (maybe_aabb, maybe_transform) {
                            let model_to_world = transform.affine();
                            // Do a cheap sphere vs obb test to prune out most meshes outside the sphere of the light
                            if !has_no_frustum_culling
                                && !light_sphere.intersects_obb(aabb, &model_to_world)
                            {
                                return;
                            }

                            for (frustum, visible_entities) in cubemap_frusta
                                .iter()
                                .zip(cubemap_visible_entities_local_queue.iter_mut())
                            {
                                if has_no_frustum_culling
                                    || frustum.intersects_obb(aabb, &model_to_world, true, true)
                                {
                                    view_visibility.set_visible();
                                    visible_entities.push(entity);
                                }
                            }
                        } else {
                            view_visibility.set_visible();
                            for visible_entities in cubemap_visible_entities_local_queue.iter_mut()
                            {
                                visible_entities.push(entity);
                            }
                        }
                    },
                );

                // Collect the entities from each parallel queue.
                for (view_dest_index, view_dest) in cubemap_visible_entities.iter_mut().enumerate()
                {
                    view_dest.entities.clear();
                    for thread_entity_queue in cubemap_visible_entities_queue.iter_mut() {
                        view_dest
                            .entities
                            .append(&mut thread_entity_queue[view_dest_index]);
                    }
                    view_dest.shrink();
                    view_dest.entities.sort_unstable();
                }
            }

            // Spot lights
            if let Ok((point_light, transform, frustum, mut visible_entities, maybe_view_mask)) =
                spot_lights.get_mut(light_entity)
            {
                // NOTE: If shadow mapping is disabled for the light then it must have no visible entities
                if !point_light.shadow_maps_enabled {
                    continue;
                }

                let view_mask = maybe_view_mask.unwrap_or_default();
        let view_has_default_layer = view_mask.intersects(&RenderLayers::default());
                let light_sphere = Sphere {
                    center: Vec3A::from(transform.translation()),
                    radius: point_light.range,
                };

                // NVForever shadow caster candidates: as for the point lights, one frustum.
                if let Some(candidates) = nvf_candidates.as_ref().and_then(|c| c.lights.get(&light_entity)) {
                    let mut seen: Vec<Entity> = Vec::new();
                    let mut iter = visible_entity_query.iter_many_mut(candidates.iter().copied());
                    while let Some(item) = iter.fetch_next() {
                        // An entity the query does not match (not a caster) is skipped.
                        let Ok((entity, inherited, mut view_visibility, mask, aabb, transform, has_range, no_culling)) = item else {
                            continue;
                        };
                        let caster = NvfCaster { entity, inherited, mask, aabb, transform, has_range, no_culling };
                        if nvf_caster_faces(&caster, view_mask, view_has_default_layer, &light_sphere, core::slice::from_ref(&*frustum), visible_entity_ranges, shadow_lod_origin) != 0 {
                            view_visibility.set_visible();
                            seen.push(entity);
                        }
                    }
                    visible_entities.entities.clear();
                    visible_entities.entities.extend(seen);
                    visible_entities.shrink();
                    visible_entities.entities.sort_unstable();
                    visible_entities.entities.dedup();
                    continue;
                }

                visible_entity_query.par_iter_mut().for_each_init(
                    || spot_visible_entities_queue.borrow_local_mut(),
                    |spot_visible_entities_local_queue,
                     (
                        entity,
                        inherited_visibility,
                        mut view_visibility,
                        maybe_entity_mask,
                        maybe_aabb,
                        maybe_transform,
                        has_visibility_range,
                        has_no_frustum_culling,
                    )| {
                        if !inherited_visibility.get() {
                            return;
                        }

                        let layer_ok = match maybe_entity_mask {
                            None => view_has_default_layer,
                            Some(entity_mask) => view_mask.intersects(entity_mask),
                        };
                        if !layer_ok {
                            return;
                        }
                        // Check visibility ranges.
                        if has_visibility_range
                            && visible_entity_ranges.is_some_and(|visible_entity_ranges| {
                                shadow_lod_origin.is_none_or(|shadow_lod_origin| {
                                    !visible_entity_ranges
                                        .entity_is_in_range_of_view(entity, shadow_lod_origin)
                                })
                            })
                        {
                            return;
                        }

                        if let (Some(aabb), Some(transform)) = (maybe_aabb, maybe_transform) {
                            let model_to_world = transform.affine();
                            // Do a cheap sphere vs obb test to prune out most meshes outside the sphere of the light
                            if !has_no_frustum_culling
                                && !light_sphere.intersects_obb(aabb, &model_to_world)
                            {
                                return;
                            }

                            if has_no_frustum_culling
                                || frustum.intersects_obb(aabb, &model_to_world, true, true)
                            {
                                view_visibility.set_visible();
                                spot_visible_entities_local_queue.push(entity);
                            }
                        } else {
                            view_visibility.set_visible();
                            spot_visible_entities_local_queue.push(entity);
                        }
                    },
                );

                visible_entities.entities.clear();
                for thread_entity_queue in spot_visible_entities_queue.iter_mut() {
                    visible_entities.entities.append(thread_entity_queue);
                }
                visible_entities.shrink();
                visible_entities.entities.sort_unstable();
            }
        }
    }
}

/// NVForever shadow caster candidates (2026-11-18, engine/vendor/README.md): for a point
/// or spot light listed here, [`check_point_light_mesh_visibility`] tests only the entities
/// given for it (with the same tests, one thread) instead of every shadow caster in the
/// world, which it does in parallel for each shadowed light. Whoever fills it must list
/// every caster whose bounds may reach the light's sphere (a superset is fine; entities that
/// are not casters are skipped). Bevy inserts none; without it, or for a light not listed,
/// nothing changes.
#[derive(Resource, Default, Debug, Clone)]
pub struct NvfShadowCasterCandidates {
    /// The entities to test, by light.
    pub lights: bevy_ecs::entity::EntityHashMap<Vec<Entity>>,
}

/// NVForever shown shadow lights (2026-11-19, engine/vendor/README.md): the point and spot
/// lights [`check_point_light_mesh_visibility`] treats as seen though no camera sees them: it
/// finds their shadow casters after the cameras' lights and marks the lights themselves
/// visible (`ViewVisibility`), so `bevy_pbr` extracts them and draws their shadow maps. Such
/// a light lights nothing on screen (it lies in no cluster of a view). The game shows its
/// lamps this way under a loading screen, to draw their maps before play. Bevy inserts none;
/// without it, or empty, nothing changes.
#[derive(Resource, Default, Debug, Clone)]
pub struct NvfShadowLightsShown {
    /// The lights shown this frame.
    pub lights: EntityHashSet,
}

/// NVForever shadow caster candidates: a caster as the visibility query gives it.
struct NvfCaster<'a> {
    entity: Entity,
    inherited: &'a InheritedVisibility,
    mask: Option<&'a RenderLayers>,
    aabb: Option<&'a Aabb>,
    transform: Option<&'a GlobalTransform>,
    has_range: bool,
    no_culling: bool,
}

/// NVForever shadow caster candidates: which of `frusta` (bit `i` for frustum `i`) a caster
/// lies in for a light of `view_mask` and `light_sphere`: the tests of the parallel pass
/// above (visibility, render layers, visibility ranges, the sphere, the frusta).
fn nvf_caster_faces(
    c: &NvfCaster,
    view_mask: &RenderLayers,
    view_has_default_layer: bool,
    light_sphere: &Sphere,
    frusta: &[Frustum],
    visible_entity_ranges: Option<&VisibleEntityRanges>,
    shadow_lod_origin: Option<Entity>,
) -> u8 {
    let layer_ok = match c.mask {
        None => view_has_default_layer,
        Some(mask) => view_mask.intersects(mask),
    };
    if !c.inherited.get() || !layer_ok {
        return 0;
    }
    if c.has_range
        && visible_entity_ranges.is_some_and(|ranges| {
            shadow_lod_origin.is_none_or(|origin| !ranges.entity_is_in_range_of_view(c.entity, origin))
        })
    {
        return 0;
    }
    let all = ((1u32 << frusta.len()) - 1) as u8;
    let (Some(aabb), Some(transform)) = (c.aabb, c.transform) else {
        return all;
    };
    let model_to_world = transform.affine();
    if c.no_culling {
        return all;
    }
    if !light_sphere.intersects_obb(aabb, &model_to_world) {
        return 0;
    }
    frusta.iter().enumerate().fold(0, |m, (i, f)| if f.intersects_obb(aabb, &model_to_world, true, true) { m | 1 << i } else { m })
}

/// Determines the LOD origin for spot and point light shadow maps.
///
/// The selection priority is, from highest to lowest:
///
/// 1. An entity explicitly marked with the [`ShadowLodOrigin`] component.
///
/// 2. A camera that renders to a window.
///
/// 3. Any camera.
pub fn get_shadow_lod_origin(
    mut camera_query: QueryLens<(Entity, &RenderTarget), With<Camera>>,
    mut shadow_lod_origin_query: QueryLens<Entity, With<ShadowLodOrigin>>,
    mut lights_query: QueryLens<Entity, Or<(With<PointLight>, With<SpotLight>)>>,
) -> Option<Entity> {
    let (camera_query, shadow_lod_origin_query) =
        (camera_query.query(), shadow_lod_origin_query.query());

    let mut entities: SmallVec<[Entity; 4]> = smallvec![];
    entities.extend(shadow_lod_origin_query.iter());
    if let Some(lod_origin) = entities.iter().min() {
        return Some(*lod_origin);
    }

    entities.extend(
        camera_query
            .iter()
            .filter_map(|(main_entity, render_target)| match *render_target {
                RenderTarget::Window(_) => Some(main_entity),
                _ => None,
            }),
    );
    if let Some(lod_origin) = entities.iter().min() {
        return Some(*lod_origin);
    };

    entities.extend(camera_query.iter().map(|(main_entity, _)| main_entity));
    if let Some(lod_origin) = entities.iter().min() {
        if !lights_query.query().is_empty() {
            warn_once!(
                "Point lights and/or spot lights are present, but no entity has \
                 `ShadowLodOrigin`, and no camera that renders to the window has been found. \
                 Consider using the `ShadowLodOrigin` component to set a LOD origin."
            );
        }

        return Some(*lod_origin);
    };

    None
}
