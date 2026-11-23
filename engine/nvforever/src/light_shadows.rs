//! Shadows of placed lights, the settings row ALC shadows (ALC: All Lights Cast; called Lamp
//! shadows until 2026-11-19: the INI key `light-shadows`, the switch `--light-shadows` and
//! the log lines `light shadows: …` keep that name, for saved settings and the smoke
//! scenarios; docs/design/graphics-enhanced.md §3.17). An addition of NVForever's own, not a fact
//! about the original game: there placed lights cast none, and no LIGH record of the
//! masters sets the Spot Shadow flag [observed: docs/handoff/esm-data-audit-2026-10-25.md,
//! item 6]. Off by default. Built as the sun's shadows are (shadow_rate.rs), [design,
//! #3074]: "look how sun shadows are calculated and do the same", "use the same
//! performance optimizations also for the lamp shadows".
//!
//! - **Which lights: by distance, as the sun's shadows.** Every drawn placed LIGH light
//!   ([`SessionLight`] with a point or spot light) whose light reaches within the sun's
//!   shadow distance of the camera (`sky::SkyState::shadow_distance` × its reach: the
//!   Shadow distance and Longer sun shadows rows) casts, nearest first, up to
//!   [`Tuning::lights`] (a safety cap for the Strip at night). Not by view: a light behind
//!   the camera keeps casting, so turning around draws nothing anew. A light already
//!   casting keeps its place against one [`KEEP`] times nearer.
//! - **Held maps, as the sun's held cascades.** A point light's cube, or a spot light's
//!   map, is drawn when it starts casting and then kept (`bevy::pbr::HoldLightShadowMaps`, a
//!   patched bevy_pbr: engine/vendor/README.md "held light shadow maps"). Between draws the
//!   light's maps are switched off for Bevy (`shadow_maps_enabled` false: no views, no
//!   culling, nothing queued, which is what a shadowed point light costs, about 1 ms a frame
//!   each on the CPU, measured), and the patched renderer still samples them. Each light
//!   keeps a slot while it casts, so others coming and going do not move it: a point light
//!   a cube of the cube texture, a spot light a layer of the sun's texture past its
//!   cascades. Each texture keeps room for its slots, grown in fours ([`grown_room`]; the
//!   spot lights' also for the other shadowed spot lights, the flashlight,
//!   [`MapTextureInputs`]). A texture the renderer makes anew (the room grew, the Shadow
//!   resolution row, the sun's cascades changed) holds no light's maps: its lights draw
//!   again as if new.
//! - **Redrawn where something changed.** A face is marked when, since it was drawn, a
//!   shadow-casting shape moved, appeared or was shown or hidden with its bounds in the
//!   face, an actor's skeleton moved there, or the light itself moved more than
//!   [`LIGHT_MOVE_EPS`] from where its maps were last marked or drawn in full (all its
//!   faces then); something removed
//!   (a taken item) marks the lights around the camera in full; every light is redrawn in
//!   full every [`Tuning::refresh`] frames anyway, each at its own time.
//! - **Staggered, as the sun's cascades.** A light with marked faces is switched on to
//!   draw them on every [`Tuning::near_every`] frame when its light reaches within [`NEAR`]
//!   of the camera, every [`Tuning::far_every`] frame otherwise (each light on frames of
//!   its own), and no more than [`Tuning::draws`] lights a frame, those that never drew
//!   first, then the nearest; the rest wait, and so a moving thing's lamp shadow can lag a
//!   frame or more. Of a light switched on, only the marked faces are drawn; the others
//!   keep their maps. A light that has never drawn casts nothing until it has.
//! - **Other lights** ([`ShadowSource`], [design, #3074]: all light sources cast). A
//!   grass fire's light (grass_fire_fx.rs) and a light a script attached to a node (JIP
//!   AttachLight, nif_mirror.rs) rank and count with the placed lights. Muzzle flashes and
//!   explosions' lights (shot_fx.rs) are transient ([`SourceKind::transient`]): chosen apart
//!   from the placed lights, nearest first, when their light reaches within
//!   [`TRANSIENT_REACH`] of the camera (and the sun's shadow distance) and into the camera's
//!   view this frame ([`in_view`]: a new light was not seen last frame), up to
//!   [`Tuning::transients`], with their own [`Tuning::transient_draws`] a frame; so a
//!   flash's cube is drawn in the frame it appears, and a firefight and the lamps do not
//!   starve each other. A transient draws its cube once and holds it for the rest of its
//!   short life (redrawn as recoil moved it, a flash drew 3.5 times on average). The effects' own shapes are `NotShadowCaster` (shot_fx.rs
//!   `spawn_effect`, `spawn_playing_itself`), and the gun a flash sits at is a small shape
//!   it sits against ([`skip_fixtures`]).
//! - **Spot lights** (few among the placed ones) are held as the point lights are: switched
//!   on only on the frames their map draws (marked and their turn come, never-drawn first),
//!   counting against [`Tuning::draws`] (a transient's against [`Tuning::transient_draws`]).
//! - **Fading** ([`FADE_SECS`]). A light's shadow fades in once its maps were first drawn.
//!   A light no longer chosen (out of range, swapped at the cap) is not dropped at once: it
//!   stays held, its slot kept and nothing drawn, while its shadow fades out, and does not
//!   count against [`Tuning::lights`] or [`Tuning::transients`]; then it is dropped
//!   ([`leaving_fade`]). A light despawned is dropped at once. Transients do not fade: a
//!   muzzle flash lasts about a tenth of a second. The fade goes to the renderer in
//!   `HoldLightShadowMaps::fade`, which scales the shadow term (`mix(1, shadow, fade)`).
//! - **What casts**: the sun's layers (`sky::SHADOW_LAYER`, `sky::BODY_LAYER` and the
//!   world's layer 0), so the player's body casts a lamp's shadow as it casts the sun's;
//!   the first-person arms do not.
//! - **Light fixtures cast none** ([`LightFixture`], [`skip_fixtures`]): the shapes of a
//!   model named as a lamp, and of a light's own model, are taken out of every casting
//!   light's maps, as is a small shape a light sits in or against. The game hangs its
//!   lights under their lamps, so a lamp's housing threw its shadow, larger than the lamp,
//!   onto the ceiling over it ([design, #3074]). The sun's shadows keep them.
//! - **Near plane** [`NEAR_Z`]: what lies nearer the light than that casts nothing from it,
//!   so the bulb, shade or flames a light sits in do not black out the room
//!   [engineering choice; `NVF_LIGHT_SHADOW_NEAR` overrides it for a run].
//! - **Maps**: the cubes' side follows the Shadow resolution row: 512 with 2048, 1024 with
//!   4096 (`PointLightShadowMap`); a spot light draws one layer of the sun's texture (its
//!   side, so each slot of the spot room costs a layer of 2048 or 4096 texels square).
//! - The diffuse light of the display-value sum takes the shadow
//!   (`lights::display_shader`), as the flashlight's always has. The flashlight is not one
//!   of these lights: it casts whenever it is on (flashlight.rs). Nor are the lights of the
//!   menu and preview scenes (creator_view.rs, vigor_view.rs, poker_view.rs) or the sky's.
//! - Test aid `NVF_PLAY_TEST_LIGHT_SHADOW_HOLD=0`: every casting light draws every face
//!   every frame, for comparing the picture and the cost.
//! - **Performance pass** (2026-11-18, [design, #3074]: "the quality of the shadow can be reduced
//!   with distance"; graphics-enhanced.md §3.17 "Performance pass"; none measured in game
//!   yet), each with an aid that turns it off:
//!   - Steps of detail by the camera's distance to a light's edge ([`STEPS`], hysteresis
//!     [`STEP_HYSTERESIS`]) and the small-caster cut-off ([`cut_off`],
//!     [`CUTOFF_TEXELS`]): a caster under a few texels of the light's map is taken out of
//!     its lists in [`skip_fixtures`] and its moves mark nothing on it; a light stepping
//!     finer after a cut draws again in full. `NVF_LIGHT_SHADOW_CUTOFF=0`.
//!   - A grid of shadow casters hands each drawing lamp its candidates for Bevy's culling
//!     (light_shadow_casters.rs). `NVF_LIGHT_SHADOW_CASTER_GRID=0`.
//!   - A light's shadow fades out over the last [`DISTANCE_FADE`] of the reach
//!     ([`distance_fade`]). `NVF_LIGHT_SHADOW_DISTANCE_FADE=0`.
//!   - Marked faces the camera cannot see stay held until they come into view
//!     ([`faces_in_view`]). `NVF_LIGHT_SHADOW_FACE_CULL=0`.
//!   - Moved shapes are marked through a [`LampGrid`], skipping lamps that draw in full
//!     anyway; lamps are marked changed for Bevy only when their settings change.
//!   - Log: besides the summary, every 10 s `light shadows: renderer over …` (the patched
//!     renderer's light views, events and `prepare_lights` times, and these systems'
//!     times, [`renderer_summary`], [`systems_summary`]) and `light shadows: performance
//!     pass over …` ([`pass_summary`]).
//! - **Static shadow cache** (2026-11-18, [design, #3074]: "Since most of the lights in New Vegas are
//!   static, could we not bake the shadows at least partially?"; graphics-enhanced.md §3.17
//!   "Static shadow cache"; not measured in game yet). Shadow casters are static (the
//!   placed world) or dynamic ([`ShadowDynamic`], [`classify_casters`]: skinned shapes,
//!   actors' and creatures' shapes, the player's body, and any shape once it moves after it
//!   marked the lamps, [`shape_change`]). Each cached point lamp keeps a second cube (the
//!   patched renderer's cache texture, same slots and size: engine/vendor/README.md "static
//!   shadow cache") drawn from the static casters alone. A face whose cache is out of date
//!   ([`LampState::static_dirty`]: a static shape appeared, vanished, was shown, hidden or
//!   moved in it, the light's first draw, a removal of static shapes, a refresh, a finer
//!   step) is drawn into the cache and copied into the sampled cube; a face marked only by
//!   dynamic casters is composited: the cache's face copied in and the dynamic casters drawn
//!   over it, the depth loaded ([`face_modes`]; the casters' lists split in
//!   [`skip_fixtures`], [`keeps_caster`]). A face drawn into the cache whose list held
//!   dynamic casters is composited next, before other lights ([`put_back`]); a dynamic
//!   caster leaving a face marks it (where it was and is, [`merged_sphere`]), and its
//!   composite with none left is the cache copied back. Transients, spot lights and lights
//!   that move keep the plain path ([`plain_path`]). The cache costs as much memory as the
//!   cubes ([`cube_texture_mb`], logged). Aid `NVF_LIGHT_SHADOW_STATIC_CACHE=0`: the plain
//!   path for all, as before. The 10-s performance line ends with its counts.
//! - **Pre-drawn under the loading screen** ([design, #3074], with the cache). While a
//!   loading screen is up (`nvf_play::loading::Loading::active`: a travel, a cover, the start)
//!   and the renderer has the cells around the camera (`Loading::render_busy` false; the
//!   destination's lights are spawned with its cells and chosen by distance to the camera,
//!   which is at the destination by then), every chosen light the camera sees draws all its
//!   marked faces at once, with no budget, no turns and no face culling ([`predrawing`],
//!   [`takes_budget`]). Once none of those has maps left to draw, the chosen lights the
//!   camera does not see draw too, [`PREDRAW_UNSEEN_PER_FRAME`] a frame, nearest first
//!   ([`shown_unseen`]): Bevy culls a point light's shadow casters only for lights a camera
//!   sees (vendored bevy_light `check_point_light_mesh_visibility`), so they are shown to it
//!   through the patched bevy_light's `NvfShadowLightsShown`, which finds their casters and
//!   marks them visible for the renderer (engine/vendor/README.md "shown shadow lights");
//!   their faces go into the static cache as any. The screen waits for it
//!   (`Loading::lamp_shadows_busy`, "ALC shadows" in its phases line) while lamps drew in the
//!   last frame or any chosen lamp still has marked faces, at most [`PREDRAW_CAP_SECS`]
//!   ([`predraw_busy`]). When the screen comes down: `light shadows: <n> lights pre-drawn
//!   under the loading screen (<f> faces, <s> into the static cache); <c> casting, <u> of
//!   them not drawn yet; <o> of those pre-drawn out of view`. Aid
//!   `NVF_LIGHT_SHADOW_PREDRAW=0` (none pre-drawn, in view or not).
//! - **Kept shadow phases** (2026-11-19, the patched bevy_pbr: engine/vendor/README.md "kept
//!   shadow phases"). A held light's shadow phases, pipeline keys and specialized pipelines
//!   outlive its views while it keeps its slot, so switching it on again queues and
//!   specializes only what changed since (Bevy's renderer threw them away with the views and
//!   did all of it again: a share of the 1.4 ms a switch-on cost); and a light re-extracted
//!   with the same place, range, near plane and layers keeps its views as they are. The
//!   renderer line's events `phases reused`, `kept phases wiped` and `views kept` count them;
//!   `kept phases resynced` counts kept phases back in view resynced with their light's whole
//!   visible list, `entities rebinned` entities queued while binned (none expected).
//!   Aid `NVF_LIGHT_SHADOW_KEEP_PHASES=0` (phases go with the views, as before).

use bevy::camera::primitives::{Aabb, CubemapFrusta, Frustum, Sphere, CUBE_MAP_FACES};
use bevy::camera::visibility::{CubemapVisibleEntities, RenderLayers, VisibilitySystems, VisibleMeshEntities};
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::light::{CascadeShadowConfig, DirectionalLightShadowMap, NotShadowCaster, NvfShadowLightsShown, PointLightShadowMap, SimulationLightSystems};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::HoldLightShadowMaps;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::transform::TransformSystems;

use crate::lights::{MovingLight, SessionLight};

/// `--light-shadows=<mode>`: the mode lamp shadows start in.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightShadowMode {
    Off,
    Performance,
    Quality,
}

/// The row's choices, in the order the page and the INI keep them (the Sun shadows row's).
pub const CHOICES: [&str; 3] = ["Off", "Performance", "Quality"];

/// The ALC shadows row's choice: an index into [`CHOICES`] (0: off).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightShadows {
    pub choice: usize,
}

impl LightShadows {
    pub fn of(mode: LightShadowMode) -> Self {
        Self { choice: mode as usize }
    }
}

/// How a mode spends its frame time [engineering choices, measured in
/// graphics-enhanced.md §3.17].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// At most this many lights cast at once, nearest first.
    pub lights: usize,
    /// At most this many lights (point or spot) switched on to draw in a frame.
    pub draws: usize,
    /// A marked light within [`NEAR`] of the camera draws on every this many frames.
    pub near_every: u64,
    /// A farther one on every this many frames.
    pub far_every: u64,
    /// Every light is drawn again in full every this many frames anyway.
    pub refresh: u64,
    /// At most this many transient lights (muzzle flashes, explosions: [`SourceKind`]) cast
    /// at once, nearest first; they do not count against [`Tuning::lights`].
    pub transients: usize,
    /// At most this many transient lights switched on to draw in a frame, besides
    /// [`Tuning::draws`], so a firefight does not starve the lamps nor the lamps it.
    pub transient_draws: usize,
}

/// The tuning of a choice; `None` for Off.
pub fn tuning(choice: usize) -> Option<Tuning> {
    match choice {
        1 => Some(Tuning { lights: 16, draws: 1, near_every: 2, far_every: 4, refresh: 1200, transients: 1, transient_draws: 1 }),
        2 => Some(Tuning { lights: 32, draws: 3, near_every: 1, far_every: 2, refresh: 600, transients: 2, transient_draws: 2 }),
        _ => None,
    }
}

/// Lights whose light reaches within this far of the camera count as near (game units,
/// about 21 m) [engineering choice: the room around the player].
pub const NEAR: f32 = 1500.0;

/// A light already casting keeps its place against one this many times nearer.
pub const KEEP: f32 = 1.25;

/// Shadow near plane of a placed light, game units (about 11 cm, as the flashlight's)
/// [engineering choice, 2026-11-18: 16 at first, to clear the bulbs and shades lights sit
/// in; since the fixtures are skipped (`LightFixture`, [`FIXTURE_MARGIN`]) 8 and 4 drew
/// the same pictures in the General Store and the Prospector Saloon, and a wall nearer
/// than 16 let the light through].
pub const NEAR_Z: f32 = 8.0;

/// Default depth and normal shadow biases for casting point and spot lights in game units
/// (`1 m = 70 units`, so `2.4` units ≈ 3.4 cm; Bevy's upstream default `0.08` is 1.1 mm
/// in game units and causes contour-band self-shadow acne on curved faces and necks).
pub const DEFAULT_DEPTH_BIAS: f32 = 2.4;
pub const DEFAULT_NORMAL_BIAS: f32 = 0.35;

/// A shape or a skeleton counts as moved for the lamps' maps once it is this far from where
/// it last marked them (game units, about 6 cm) [engineering choice, 2026-11-18: in the
/// Prospector Saloon the actors' idle skeletons and the head parts that follow them (eyes,
/// mouth, hat) marked about 116 times a frame, every lamp near an actor every frame].
pub const MOVE_EPS: f32 = 4.0;

/// Movement threshold (game units) for dynamic actor parts and skeletons so head turns and
/// nods in dialogue redraw nearby lamp shadow faces without waiting for the periodic refresh.
pub const DYNAMIC_MOVE_EPS: f32 = 1.25;

/// A casting light counts as moved once it is this far from where its maps were last
/// marked or drawn in full (game units, about 11 cm) [engineering choice, 2026-11-18: a
/// light an animation or script sways by a few units is not drawn again each frame].
/// Transients are not redrawn at all once drawn.
pub const LIGHT_MOVE_EPS: f32 = 8.0;

/// Transient lights cast only when their light reaches within this far of the camera
/// (game units, about 43 m), and within the sun's shadow distance [engineering choice: a
/// firefight around the player; farther flashes are small on screen and gone in a tenth of
/// a second].
pub const TRANSIENT_REACH: f32 = 3000.0;

/// A light's shadow fades in over this long once its maps were first drawn, and out over
/// this long once it stops casting (real seconds) [engineering choice: no popping when a
/// light comes into range, leaves it or is swapped at the cap; short enough that a light
/// coming into range shows its shadow at once]. Transients do not fade.
pub const FADE_SECS: f32 = 0.3;

/// On a light other than a placed one that casts lamp shadows (main world): what it is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowSource(pub SourceKind);

/// The lights besides the placed LIGH lights that cast lamp shadows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A weapon's muzzle flash (shot_fx.rs): transient, moves with the gun.
    MuzzleFlash,
    /// An explosion's fading light (shot_fx.rs): transient, still.
    Explosion,
    /// A burning grass patch's light (grass_fire_fx.rs): ranked with the placed lights.
    GrassFire,
    /// A light a script attached to a node (JIP AttachLight, nif_mirror.rs): ranked with the
    /// placed lights, moves with its node.
    Attached,
}

impl SourceKind {
    /// Lights that live a moment and are chosen apart from the placed ones
    /// ([`Tuning::transients`], [`TRANSIENT_REACH`]).
    pub fn transient(self) -> bool {
        matches!(self, Self::MuzzleFlash | Self::Explosion)
    }
}

/// On a shape of a light fixture: a model named as a lamp ([`is_fixture_model`]) or a
/// light's own model (a LIGH with a model). It casts no lamp shadows; the sun's it still
/// casts. A playtest (2026-11-18, [design, #3074]): a hanging box lamp threw its housing's shadow, larger than the
/// lamp, onto the ceiling above it. The game hangs its lights below their fixtures: in the
/// Goodsprings General Store the light under the box lamp `OffRmLightHanging01` sits 86
/// units under the lamp's reference; over the masters' interiors 3,909 of the 7,543 LIGH
/// references without a model of their own have a lamp model's reference within 160 units,
/// half of them within 89 [observed 2026-11-18: `fnvtool world refs --interiors`].
#[derive(Component)]
pub struct LightFixture;

/// The words in a model's file name that make it a light fixture [engineering choice from
/// the census in [`LightFixture`]: `offrmlight01`, `lightceilingvaultr02on`,
/// `shackhanginglight02`, `lamplightlamp`, `neoclassicalsconce`, …].
const FIXTURE_WORDS: [&str; 7] = ["light", "lamp", "lantern", "chandelier", "sconce", "bulb", "candle"];

/// Whether a model (a path under `meshes\`) is a light fixture: its file name holds one of
/// [`FIXTURE_WORDS`].
pub fn is_fixture_model(path: &str) -> bool {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase();
    FIXTURE_WORDS.iter().any(|w| file.contains(w))
}

/// A shape not named as a fixture is one for a light that lies within this far of its box
/// (game units, about 23 cm): a bulb or shade of another name the light sits
/// in [engineering choice].
pub const FIXTURE_MARGIN: f32 = 16.0;

/// Such a shape's bounding sphere is no larger than this (game units, about 1.8 m), so a
/// wall, floor, ceiling or room piece next to a light is never one [engineering choice: the
/// interior kits' pieces are 256 units and more].
pub const FIXTURE_RADIUS: f32 = 128.0;

/// The faces of a point light's cube (bit `i` for face `i`).
const ALL_FACES: u8 = 0b11_1111;

/// On a placed light while it casts shadows.
#[derive(Component)]
pub struct CastingLight;

/// On a shadow-casting shape that moves (the static shadow cache, [`static_cache_on`]): it
/// is left out of the lamps' static caches and drawn over them. Put by [`classify_casters`]
/// on a skinned shape, on every shape of an actor or creature (under an `ActorRig` or a
/// `PlayerRig`) and of the player's body (`sky::SHADOW_LAYER`, `sky::BODY_LAYER`), and by
/// [`shadow_lights`] on any other shape once it moves after it marked the lamps (clutter
/// physics pushed, a door or activator that turns, what a script moves): [`shape_change`].
#[derive(Component)]
pub struct ShadowDynamic;

/// `NVF_LIGHT_SHADOW_STATIC_CACHE=0` (a measuring aid): no static shadow cache; every face
/// drawn is drawn with every caster, as before 2026-11-18.
fn static_cache_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_STATIC_CACHE").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_STATIC_CACHE=0, no static shadow cache: every caster drawn into every face");
        }
        !off
    })
}

/// What a shadow-casting shape's change does to the lamps' maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeChange {
    /// A static shape appeared, vanished, or was shown or hidden (a cell loading, Primm's
    /// scripted mass enable): the static caches of the faces it is in are drawn again.
    Static,
    /// A dynamic shape moved, appeared, vanished, or was shown or hidden: the faces it was and
    /// is in are composited again; no cache is drawn.
    Dynamic,
    /// A static shape moved after it marked the lamps: from now on it is dynamic, and the
    /// caches of the faces it was and is in are drawn again without it.
    Promoted,
}

/// What a shape's change is: `dynamic` it already is ([`ShadowDynamic`]); `shown_or_hidden`
/// its visibility changed (or it is new); `known` it marked the lamps before, from where it
/// is said to have moved.
pub fn shape_change(dynamic: bool, shown_or_hidden: bool, known: bool) -> ShapeChange {
    if dynamic {
        ShapeChange::Dynamic
    } else if shown_or_hidden || !known {
        ShapeChange::Static
    } else {
        ShapeChange::Promoted
    }
}

/// The sphere holding both spheres: a moved caster marks where it was and where it is.
pub fn merged_sphere(a: &Sphere, b: &Sphere) -> Sphere {
    let (ca, cb) = (Vec3::from(a.center), Vec3::from(b.center));
    let d = ca.distance(cb);
    if d + b.radius <= a.radius {
        return *a;
    }
    if d + a.radius <= b.radius {
        return *b;
    }
    let radius = (d + a.radius + b.radius) * 0.5;
    // From a's far side towards b, `radius` along.
    let centre = ca + (cb - ca) * ((radius - a.radius) / d.max(1.0e-6));
    Sphere { center: centre.into(), radius }
}

/// How a light's faces `draw` this frame are drawn: those whose static cache must be drawn
/// again (`static_dirty`) into the cache, then copied into the sampled map; the rest
/// composited (the cache copied in, the dynamic casters drawn over). `(to_static,
/// composite)`; both 0 for a light on the plain path (`cached` false).
pub fn face_modes(draw: u8, static_dirty: u8, cached: bool) -> (u8, u8) {
    if !cached {
        return (0, 0);
    }
    (draw & static_dirty, draw & !static_dirty)
}

/// Which casters a face's list keeps: on the plain path all; drawn into the static cache
/// the static ones; composited the dynamic ones.
pub fn keeps_caster(to_static: bool, composite: bool, dynamic: bool) -> bool {
    match (to_static, composite) {
        (true, _) => !dynamic,
        (false, true) => dynamic,
        _ => true,
    }
}

/// The faces drawn into the static cache this frame (`to_static`) whose lists held dynamic
/// casters (`dynamic_in`): the post-draw copy left them out of the sampled map, so they are
/// composited again, first.
pub fn put_back(to_static: u8, dynamic_in: u8) -> u8 {
    to_static & dynamic_in
}

/// `NVF_LIGHT_SHADOW_PREDRAW=0` (a measuring aid): no pre-draw under loading screens; the
/// lamps draw on their budget and turns there as in play, and the screen does not wait for
/// them.
fn predraw_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_PREDRAW").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_PREDRAW=0, no pre-draw under loading screens");
        }
        !off
    })
}

/// The loading screen waits for the pre-draw this long at most, real seconds from when the
/// renderer had its cells [engineering choice: a lamp's maps draw in a frame or two once its
/// light is seen; a shape that kept marking them must not hold the screen for long].
pub const PREDRAW_CAP_SECS: f32 = 3.0;

/// Whether the lamps pre-draw this frame: a mode on, the aid not set, a loading screen up
/// and the renderer done with its cells (before that the lamps would draw again for every
/// cell that comes in).
pub fn predrawing(mode_on: bool, aid_on: bool, screen_up: bool, render_busy: bool) -> bool {
    mode_on && aid_on && screen_up && !render_busy
}

/// Whether the loading screen still waits for the pre-draw: lamps drew this frame (`drew`:
/// their composites may follow) or seen lamps still have marked faces (`waiting`), within
/// [`PREDRAW_CAP_SECS`] of its start (`secs`).
pub fn predraw_busy(drew: bool, waiting: bool, secs: f32) -> bool {
    (drew || waiting) && secs < PREDRAW_CAP_SECS
}

/// Under a loading screen, at most this many lamps the camera does not see are shown to
/// Bevy's culling a frame to draw their maps (nearest first) [engineering choice: a frame of
/// the pre-draw stays short, so the screen's own picture keeps moving; 32 lamps in four
/// frames].
pub const PREDRAW_UNSEEN_PER_FRAME: usize = 8;

/// Which lamps the camera does not see are shown this frame of a pre-draw: none while a lamp
/// in view still has maps to draw (`seen_waiting`: those go first), else the `cap` nearest of
/// `unseen` (`(id, gap)`, the camera's distance to its light's edge) that have maps to draw.
pub fn shown_unseen<T: Copy>(unseen: &mut [(T, f32)], seen_waiting: bool, cap: usize) -> Vec<T> {
    if seen_waiting {
        return Vec::new();
    }
    unseen.sort_by(|a, b| a.1.total_cmp(&b.1));
    unseen.iter().take(cap).map(|u| u.0).collect()
}

/// Whether a light with maps to draw may draw: always without a limit (`unlimited`: the
/// hold aid, a pre-draw), else while the frame's `budget` lasts, which it takes one from.
pub fn takes_budget(budget: &mut usize, unlimited: bool) -> bool {
    if unlimited {
        return true;
    }
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    true
}

/// Whether a light's maps take the plain path: the cache off, every map drawn every frame
/// (the hold aid), a transient (drawn once), a spot light (one map, few among the lamps:
/// [engineering choice, left on the old path]), or a light that moved (its cache would be
/// drawn again each time).
pub fn plain_path(cache_on: bool, hold_all: bool, transient: bool, spot: bool, moved: bool) -> bool {
    !cache_on || hold_all || transient || spot || moved
}

/// What a casting light's maps need (main world).
#[derive(Debug, Clone, Copy)]
struct LampState {
    /// Faces changed since they were last drawn (bit 0 for a spot light).
    dirty: u8,
    /// It has not drawn yet (its maps hold nothing of it).
    fresh: bool,
    /// Frame its maps were last drawn in full.
    refreshed: u64,
    /// A number of its own, for its turn in the staggered schedule.
    phase: u64,
    /// Where the light was when its maps were last marked or drawn in full.
    at: Vec3,
    /// A muzzle flash or an explosion ([`SourceKind::transient`]).
    transient: bool,
    /// A spot light (its map is a layer of the sun's texture), not a point light.
    spot: bool,
    /// How much of its shadow it casts, 0..1 ([`FADE_SECS`]).
    fade: f32,
    /// No longer chosen: held, not drawn, while its fade falls to 0.
    leaving: bool,
    /// Its step of detail by distance ([`STEPS`]), and its maps' texels across a unit at a
    /// unit's distance ([`map_scale`]): together the size under which a caster is left out
    /// of its maps ([`cut_off`]).
    step: u8,
    map_scale: f32,
    /// A caster was left out of its maps since the light last stepped to finer detail: a
    /// step finer draws its maps again so that caster comes back.
    cut_any: bool,
    /// Its shadow's strength by distance, 0..1 ([`distance_fade`]); with [`Self::fade`] it
    /// makes what the renderer gets.
    far_fade: f32,
    /// Marked faces held last frame because the camera could not see them
    /// ([`faces_in_view`]): one coming into view draws before other lights' faces.
    deferred: u8,
    /// The static shadow cache ([`static_cache_on`]): faces whose cache (the static casters'
    /// map) must be drawn again; always within [`Self::dirty`].
    static_dirty: u8,
    /// Faces drawn into the cache whose lists held dynamic casters: the sampled face lacks
    /// them until it is composited, which goes before other lights' turns.
    pending: u8,
    /// Its maps take the plain path, every caster drawn into the sampled map: a transient, a
    /// spot light, a light that moved, or the cache off.
    plain: bool,
    /// This frame's faces drawn into the cache, and composited from it ([`face_modes`]).
    to_static: u8,
    composite: u8,
}

/// The casting lights' slots and states.
#[derive(Resource, Default)]
struct LampShadowState {
    lamps: EntityHashMap<LampState>,
    /// Stable slots of the casting point lights in the cube texture.
    slots: EntityHashMap<u32>,
    /// The cube texture's room, in lights; it only grows while a mode stays on.
    capacity: u32,
    /// Stable slots of the casting spot lights past the sun's cascades in its texture, and
    /// the room kept there for them; it only grows while a mode stays on.
    spot_slots: EntityHashMap<u32>,
    spot_capacity: u32,
    /// What the cube texture (its side, its room) and the sun's texture (its side, the spot
    /// room, the cascades) were made for last frame: when either changes the renderer takes
    /// a new texture, which holds no light's maps.
    point_maps_key: Option<(usize, u32, bool)>,
    spot_maps_key: Option<(usize, u32, usize)>,
    frame: u64,
    /// Lights switched on and faces drawn since the last summary, and frames counted.
    switched: u64,
    drawn: u64,
    /// The same for the transient lights, kept apart: those that started casting, were
    /// switched on, and their faces drawn, since the last summary.
    transients_cast: u64,
    transients_switched: u64,
    transients_drawn: u64,
    /// What marked faces since the last summary: shapes, skeletons, removals, the lights
    /// themselves (moved or their full redraw due); and a few of the shapes' names.
    causes: [u64; 4],
    names: Vec<String>,
    /// Where each shape (its bounds' centre and orientation probe) and each skeleton (by
    /// its first joint; its joints' box) was when it last marked the lamps, and the frame:
    /// moves under [`MOVE_EPS`] add up until they reach it.
    shapes_at: EntityHashMap<(Vec3, Vec3, u64)>,
    skeletons_at: EntityHashMap<(Vec3, Vec3, u64)>,
    frames: u64,
    since: f32,
    /// The performance pass's counts since the last summary ([`Pass`]).
    pass: Pass,
    /// What the lamps drew under the loading screen that is up ([`predrawing`]).
    predraw: Predraw,
    /// The point lights' cube side before a mode set its own, given back when the row is
    /// switched off.
    cube_before: Option<usize>,
}

/// The lamps' pre-draw under one loading screen.
#[derive(Debug, Default)]
struct Predraw {
    /// A loading screen was up last frame.
    screen: bool,
    /// Real seconds (`Time<Real>` elapsed) when the pre-draw began under this screen.
    began: Option<f32>,
    /// Lights drawn, their faces, and of those the faces drawn into the static cache.
    lights: EntityHashSet,
    faces: u64,
    static_faces: u64,
    /// Lights drawn while the camera did not see them (shown to Bevy's culling,
    /// [`shown_unseen`]).
    unseen: EntityHashSet,
}

/// What the performance pass's techniques did since the last summary (its own 10-s line).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Pass {
    /// Casters left out of a drawing light's lists by the small-caster cut-off ([`cut_off`]).
    pub cut_casters: u64,
    /// Marks of a moved shape or skeleton on a light it is under the cut-off for, skipped.
    pub cut_marks: u64,
    /// Full redraws for a light stepping to finer detail with casters cut before.
    pub step_redraws: u64,
    /// Casting lights at each step, summed over the frames.
    pub steps: [u64; 3],
    /// The caster grid's counts (light_shadow_casters.rs `take_grid_counts`): lamps served,
    /// candidates over them, meshes filed, those of them listed for every light.
    pub grid: [u64; 4],
    /// Casting lights whose shadow was faded by distance, summed over the frames.
    pub distance_faded: u64,
    /// Marked faces held on their light's turn because the camera could not see them.
    pub faces_out_of_view: u64,
    /// The static shadow cache: faces drawn into it (each then copied into the sampled map),
    /// faces composited from it (copied in, the dynamic casters drawn over), and of the faces
    /// drawn into it those whose dynamic casters were put back by a composite after.
    pub static_faces: u64,
    pub composited: u64,
    pub put_back: u64,
    /// Shapes that moved after they were in the static caches and became dynamic.
    pub promoted: u64,
    /// Lights pre-drawn under loading screens that came down since the last summary.
    pub predrawn: u64,
}

/// A light's shadow fades out over this last fraction of the reach it is chosen within (the
/// sun's shadow distance), instead of ending at it [engineering choice: a lamp leaving the
/// reach or coming into it no longer pops even before its time fade, [`FADE_SECS`]; Godot's
/// lights fade their shadows out by distance the same way (from a research pass)].
pub const DISTANCE_FADE: f32 = 0.2;

/// A light's shadow strength by distance: 1 until the last [`DISTANCE_FADE`] of `reach`,
/// then falling in a straight line to 0 at it; `gap` is the camera's distance to its
/// light's edge.
pub fn distance_fade(gap: f32, reach: f32) -> f32 {
    if reach <= 0.0 {
        return 1.0;
    }
    ((reach - gap) / (DISTANCE_FADE * reach)).clamp(0.0, 1.0)
}

/// `NVF_LIGHT_SHADOW_DISTANCE_FADE=0` (a measuring aid): no fade by distance, a light's
/// shadow ends at the reach as before 2026-11-18.
fn distance_fade_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_DISTANCE_FADE").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_DISTANCE_FADE=0, no fade by distance");
        }
        !off
    })
}

/// The system that chooses the casting lights and switches their maps on to draw
/// (`shadow_lights`), for ordering against it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChooseLamps;

/// How many frames the ALC shadows' systems still run after the row is switched off (or
/// from the start with it off): enough for [`shadow_lights`] to switch every light's maps
/// off and give the renderer an inactive hold, and for the soft edges, contact shadows and
/// caster candidates to be given back, whatever order they run in.
pub const WIND_DOWN_FRAMES: u8 = 3;

/// Frames since [`shadow_lights`] last ran with the row on (saturating at
/// [`WIND_DOWN_FRAMES`]); see [`live`].
#[derive(Resource, Default, Debug)]
pub struct LightShadowsIdle {
    frames: u8,
}

/// Whether the ALC shadows' systems run this frame: with the row on, and for
/// [`WIND_DOWN_FRAMES`] frames after it is off. With the row Off none of them runs (their
/// run condition only), and the renderer's hold is inactive, upstream's paths
/// (bevy_pbr `HoldLightShadowMaps::active`).
pub fn live(settings: Res<LightShadows>, idle: Res<LightShadowsIdle>) -> bool {
    runs(settings.choice, idle.frames)
}

/// [`live`] for a row choice and the idle frames.
pub fn runs(choice: usize, idle_frames: u8) -> bool {
    choice != 0 || idle_frames < WIND_DOWN_FRAMES
}

pub struct LightShadowsPlugin(pub LightShadowMode);

impl Plugin for LightShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightShadows::of(self.0))
            .init_resource::<LampShadowState>()
            .init_resource::<LightShadowsIdle>()
            .init_resource::<MapTextureInputs>()
            .init_resource::<HoldLightShadowMaps>()
            .init_resource::<NvfShadowLightsShown>()
            .add_plugins(ExtractResourcePlugin::<HoldLightShadowMaps>::default())
            .add_systems(PostUpdate, watch_map_textures.before(shadow_lights).run_if(live))
            .add_systems(PostUpdate, classify_casters.before(shadow_lights).run_if(live))
            .add_systems(
                PostUpdate,
                // Before Bevy resets this frame's view visibility, so the lights' and the
                // shapes' `ViewVisibility` still say what the last frame saw.
                // After the camera's frustum is updated: a transient draws in its first frame
                // when its light reaches into this frame's view.
                shadow_lights
                    .in_set(ChooseLamps)
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::UpdateFrusta)
                    .before(VisibilitySystems::VisibilityPropagate)
                    .before(SimulationLightSystems::AssignLightsToClusters)
                    .before(SimulationLightSystems::UpdateLightFrusta)
                    .run_if(live),
            )
            .add_systems(PostUpdate, skip_fixtures.after(SimulationLightSystems::CheckLightVisibility).run_if(live))
            .add_plugins(crate::light_shadow_casters::LightShadowCastersPlugin);
    }
}

/// What else decides the size of the sun's shadow texture the spot lights' maps are in
/// (main world): the other shadowed spot lights (the flashlight), which take slots of their
/// own past the lamps' (bevy_pbr's patched `prepare_lights`), and the sun's cascades, which
/// come before them.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
struct MapTextureInputs {
    other_spots: u32,
    cascades: usize,
}

/// A directional light draws at most this many cascades (Bevy's `MAX_CASCADES_PER_LIGHT`)
/// [documented: engine/vendor/bevy_pbr/src/render/light.rs].
const MAX_CASCADES: usize = 4;

/// Count what [`MapTextureInputs`] holds. Separate from [`shadow_lights`], whose query
/// holds the lamps' `SpotLight`s mutably.
fn watch_map_textures(
    mut inputs: ResMut<MapTextureInputs>,
    spots: Query<&SpotLight, (Without<SessionLight>, Without<ShadowSource>)>,
    suns: Query<(&DirectionalLight, &CascadeShadowConfig)>,
) {
    let now = MapTextureInputs {
        other_spots: spots.iter().filter(|s| s.shadow_maps_enabled).count() as u32,
        cascades: suns.iter().filter(|(d, _)| d.shadow_maps_enabled).map(|(_, c)| c.bounds.len().min(MAX_CASCADES)).sum(),
    };
    if *inputs != now {
        *inputs = now;
    }
}

/// A light's fade after `dt` real seconds, rising (it casts) or falling (it is leaving),
/// over [`FADE_SECS`]; within 0..1.
pub fn fade_step(fade: f32, rising: bool, dt: f32) -> f32 {
    let step = dt / FADE_SECS;
    (if rising { fade + step } else { fade - step }).clamp(0.0, 1.0)
}

/// A light no longer chosen: its fade after `dt` while it is held fading out, `None` once it
/// is dropped (at once when it despawned, `alive` false; never drew; or is a transient,
/// which does not fade).
pub fn leaving_fade(fade: f32, drawn: bool, transient: bool, alive: bool, dt: f32) -> Option<f32> {
    if !alive || !drawn || transient {
        return None;
    }
    let fade = fade_step(fade, false, dt);
    (fade > 0.0).then_some(fade)
}

/// The room a texture keeps for `slots` and `others` lights besides them: `room`, grown in
/// fours until it holds the highest slot and the others.
pub fn grown_room(room: u32, slots: impl Iterator<Item = u32>, others: u32) -> u32 {
    let needed = slots.map(|s| s + 1).max().unwrap_or(0) + others;
    room.max(needed.div_ceil(4) * 4)
}

/// `e`'s slot in `slots`: the one it has, else the lowest free one.
pub fn assign_slot(slots: &mut EntityHashMap<u32>, e: Entity) -> u32 {
    if let Some(&slot) = slots.get(&e) {
        return slot;
    }
    let slot = free_slot(slots.values().copied());
    slots.insert(e, slot);
    slot
}

/// The order of a light among the candidates: the distance from the camera to the edge of
/// its light (0 inside it), then to the light; a casting light's divided by [`KEEP`].
pub fn rank(distance: f32, range: f32, casting: bool) -> (f32, f32) {
    let k = if casting { KEEP } else { 1.0 };
    ((distance - range).max(0.0) / k, distance / k)
}

/// The lights that cast: the `cap` first of `(id, rank)` with a rank's edge distance
/// within `reach`, nearest first; ties keep the order given.
pub fn pick<T: Copy>(candidates: &mut [(T, (f32, f32))], reach: f32, cap: usize) -> Vec<T> {
    candidates.sort_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.1 .1.total_cmp(&b.1 .1)));
    candidates.iter().filter(|c| c.1 .0 <= reach).take(cap).map(|c| c.0).collect()
}

/// How near a transient light's edge must come to the camera to cast: [`TRANSIENT_REACH`],
/// or the sun's shadow distance when that is shorter.
pub fn transient_reach(shadow_reach: f32) -> f32 {
    shadow_reach.min(TRANSIENT_REACH)
}

/// Whether a light's sphere at `at` with `range` reaches into the camera's view (the far
/// plane left out, as Bevy's culling of lights does); without a frustum, yes. A light whose
/// sphere does is one Bevy's culling finds visible this frame (its box holds the sphere), so
/// its maps are drawn.
pub fn in_view(frustum: Option<&Frustum>, at: Vec3, range: f32) -> bool {
    frustum.is_none_or(|f| f.intersects_sphere(&Sphere { center: at.into(), radius: range }, false))
}

/// Whether a light now at `now` moved more than [`LIGHT_MOVE_EPS`] from `drawn_at`, where
/// its maps were last marked or drawn in full.
pub fn light_moved(drawn_at: Vec3, now: Vec3) -> bool {
    drawn_at.distance_squared(now) > LIGHT_MOVE_EPS * LIGHT_MOVE_EPS
}

/// The renderer's side of the 10-s summary, from the patched bevy_pbr's counts over its
/// frames since the last one (engine/vendor/README.md "light view counts"): the point and
/// spot light shadow views alive and run a frame (the flashlight's spot view among them),
/// those a hidden light took with it, what `prepare_lights` did, and the wall time of
/// `extract_lights`, `prepare_lights` and its stretches, mean and longest a frame.
pub fn renderer_summary(c: &bevy::pbr::NvfLightViewCounts) -> String {
    let frames = c.frames.max(1) as f64;
    let ms = |ns: u64| ns as f64 / 1.0e6;
    let events: Vec<String> = bevy::pbr::NVF_EVENTS.iter().zip(c.events).map(|(name, n)| format!("{name} {n}")).collect();
    let times: Vec<String> =
        bevy::pbr::NVF_SECTIONS.iter().enumerate().map(|(i, name)| format!("{name} {:.3}/{:.2}", ms(c.ns[i]) / frames, ms(c.max_ns[i]))).collect();
    format!(
        "light shadows: renderer over {} frames: light views a frame {:.2} alive, {:.2} run, {} despawned with their light hidden; {}; ms a frame mean/max: {}",
        c.frames,
        c.views as f64 / frames,
        c.run as f64 / frames,
        c.hidden,
        events.join(", "),
        times.join(", ")
    )
}

/// The lamp shadows' own systems whose wall time the 10-s summary gives (a measuring aid),
/// in the order of [`Timed`]'s index.
pub const SYSTEMS: [&str; 4] = ["shadow_lights", "skip_fixtures", "soften", "contact"];

static SYSTEM_NS: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];
static SYSTEM_MAX_NS: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

/// Times a run of one of [`SYSTEMS`] (by index) until it is dropped.
pub struct Timed(usize, std::time::Instant);

impl Timed {
    pub fn start(system: usize) -> Self {
        Self(system, std::time::Instant::now())
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering::Relaxed;
        let ns = self.1.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        SYSTEM_NS[self.0].fetch_add(ns, Relaxed);
        SYSTEM_MAX_NS[self.0].fetch_max(ns, Relaxed);
    }
}

/// The [`SYSTEMS`]' times since the last call (summed, longest; nanoseconds), and start
/// timing anew.
fn take_system_times() -> ([u64; 4], [u64; 4]) {
    use std::sync::atomic::Ordering::Relaxed;
    (std::array::from_fn(|i| SYSTEM_NS[i].swap(0, Relaxed)), std::array::from_fn(|i| SYSTEM_MAX_NS[i].swap(0, Relaxed)))
}

/// The [`SYSTEMS`]' times over `frames` game frames, mean and longest a frame (ms).
pub fn systems_summary(ns: &[u64; 4], max: &[u64; 4], frames: u64) -> String {
    let frames = frames.max(1) as f64;
    let parts: Vec<String> = SYSTEMS.iter().enumerate().map(|(i, name)| format!("{name} {:.3}/{:.2}", ns[i] as f64 / 1.0e6 / frames, max[i] as f64 / 1.0e6)).collect();
    format!("game systems ms a frame mean/max: {}", parts.join(", "))
}

/// The performance pass's line of the 10-s summary: what each technique did, a frame over
/// `frames` frames, and which are switched off by their test aids.
pub fn pass_summary(p: &Pass, frames: u64) -> String {
    let per = |n: u64| n as f64 / frames.max(1) as f64;
    let on = |b: bool| if b { "on" } else { "OFF" };
    let [served, candidates, filed, everywhere] = p.grid;
    format!(
        "light shadows: performance pass over {frames} frames: lights at steps near/mid/far {:.1}/{:.1}/{:.1} a frame; small-caster cut-off ({}) casters cut {:.1} a frame, marks skipped {:.2} a frame, redraws for a finer step {}; caster grid ({}) {:.2} lamps served a frame, {:.0} candidates each, {filed} meshes filed ({everywhere} for every light); distance fade ({}) {:.1} lights fading a frame; face culling ({}) {:.2} marked faces held out of view a frame; static cache ({}) {:.2} static redraws (faces) a frame, {:.2} dynamic recomposites a frame, {:.2} layer copies a frame, {} faces put back after a static redraw, {} shapes became dynamic; loading-screen pre-draw ({}) {} lights",
        per(p.steps[0]),
        per(p.steps[1]),
        per(p.steps[2]),
        on(cutoff_on()),
        per(p.cut_casters),
        per(p.cut_marks),
        p.step_redraws,
        on(crate::light_shadow_casters::grid_on()),
        per(served),
        candidates as f64 / served.max(1) as f64,
        on(distance_fade_on()),
        per(p.distance_faded),
        on(face_cull_on()),
        per(p.faces_out_of_view),
        on(static_cache_on()),
        per(p.static_faces),
        per(p.composited),
        // Each face drawn into the cache is copied out after, each composite copied in first.
        per(p.static_faces + p.composited),
        p.put_back,
        p.promoted,
        on(predraw_on()),
        p.predrawn
    )
}

/// The lowest slot not in `used`.
pub fn free_slot(used: impl Iterator<Item = u32>) -> u32 {
    let mut taken: Vec<u32> = used.collect();
    taken.sort_unstable();
    let mut slot = 0;
    for t in taken {
        if t == slot {
            slot += 1;
        } else if t > slot {
            break;
        }
    }
    slot
}

/// Whether a light with these turns draws on `frame`.
pub fn its_turn(frame: u64, phase: u64, every: u64) -> bool {
    (frame + phase).is_multiple_of(every.max(1))
}

/// Which of a point light's faces (or a spot light's map, bit 0) a sphere reaches, leaving
/// out (not testing) the faces in `skip`.
fn faces_reached(sphere: &Sphere, cube: Option<&CubemapFrusta>, spot: Option<&Frustum>, skip: u8) -> u8 {
    if let Some(f) = spot {
        return u8::from(skip & 1 == 0 && f.intersects_sphere(sphere, true));
    }
    let Some(cube) = cube else { return ALL_FACES & !skip };
    cube.frusta.iter().enumerate().fold(0, |m, (i, f)| if skip & (1 << i) == 0 && f.intersects_sphere(sphere, true) { m | 1 << i } else { m })
}

/// What [`mark_sphere`] found: whether the caster reached any lamp's sphere, whether it
/// marked (or lies in) any face, and on how many lamps it was under the cut-off and marked
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marked {
    pub reached: bool,
    pub marked: bool,
    pub cut: u32,
}

/// Mark the faces of the `candidates` (indices into `lamps`, with `frusta` alongside) a
/// caster's sphere reaches. A lamp that draws in full anyway (it never drew, or every face
/// is marked) is not tested face by face, nor is a face already marked. With `cut` (the
/// small-caster cut-off on), a caster under a lamp's cut-off where it is now and where it
/// last marked (`old`; none for one shown, hidden or new) marks nothing on that lamp: it is
/// not in its maps. With `static_mark` (a static shape's change, [`ShapeChange`]) the faces'
/// static caches are marked too ([`LampState::static_dirty`]).
#[allow(clippy::too_many_arguments)]
fn mark_sphere(
    sphere: &Sphere,
    old: Option<&Sphere>,
    cut: bool,
    static_mark: bool,
    candidates: impl Iterator<Item = usize>,
    lamps: &[Lamp],
    frusta: &[(Option<CubemapFrusta>, Option<Frustum>)],
    states: &mut EntityHashMap<LampState>,
) -> Marked {
    let (mut reached, mut marked, mut cuts) = (false, false, 0);
    for i in candidates {
        let l = &lamps[i];
        if !reaches(l.at, l.range, sphere) {
            continue;
        }
        reached = true;
        let below = |s: &Sphere| cut_off(s.radius, Vec3::from(s.center).distance(l.at), l.map_scale, l.step);
        if cut && below(sphere) && old.is_none_or(below) {
            cuts += 1;
            continue;
        }
        let Some(s) = states.get_mut(&l.entity) else { continue };
        let (cube, spot) = (frusta[i].0.as_ref(), frusta[i].1.as_ref());
        if l.spot {
            // One map: its cone is tested.
            let faces = faces_reached(sphere, cube, spot, 0);
            marked |= faces != 0;
            s.dirty |= faces;
            if static_mark {
                s.static_dirty |= faces;
            }
            continue;
        }
        // A sphere that reaches a point light's lies in one of its faces at least.
        marked = true;
        // Faces already marked as this mark would are not tested again (the static marks
        // within the faces marked: `static_dirty` lies within `dirty`).
        let have = if static_mark { s.static_dirty } else { s.dirty } & ALL_FACES;
        if !s.fresh && have != ALL_FACES {
            let faces = faces_reached(sphere, cube, spot, have);
            s.dirty |= faces;
            if static_mark {
                s.static_dirty |= faces;
            }
        }
    }
    Marked { reached, marked, cut: cuts }
}

/// The camera's distance to a light's edge (game units) where its maps step down in
/// detail: within the first, step 0 (the room around the player, [`NEAR`]); past it step 1;
/// past the second, step 2 [engineering choice: the next room or street, and beyond].
pub const STEPS: [f32; 2] = [NEAR, 2.0 * NEAR];

/// A light moves to a coarser step once it is this fraction beyond the step's distance, and
/// back to a finer one this fraction within it [engineering choice: walking back and forth
/// over a step's distance does not switch the step, nor draw the maps again, each time].
pub const STEP_HYSTERESIS: f32 = 0.1;

/// Per step, the size (across, in texels of the light's map) under which a caster is left
/// out of a light's map ([`cut_off`]) [engineering choice: at step 0 only what covers less
/// than a texel, which the map cannot show anyway; a farther light is seen from farther
/// away, so a texel of its map covers fewer pixels on screen. Unreal's
/// r.Shadow.RadiusThreshold culls small casters by their projected size too (from a
/// research pass, not checked against its code)].
pub const CUTOFF_TEXELS: [f32; 3] = [1.0, 2.0, 4.0];

/// A light's step of detail for `gap` (the camera's distance to its light's edge), starting
/// from `current`, with hysteresis `h` (a fraction of each step's distance; 0: none).
pub fn stepped(current: u8, gap: f32, h: f32) -> u8 {
    let coarser = STEPS.iter().filter(|b| gap > **b * (1.0 + h)).count() as u8;
    let finer = STEPS.iter().filter(|b| gap > **b * (1.0 - h)).count() as u8;
    current.clamp(coarser, finer)
}

/// A map's texels across one unit at one unit's distance: its side over the tangent of half
/// its field of view (1 for a cube face).
pub fn map_scale(side: f32, half_fov_tan: f32) -> f32 {
    side / half_fov_tan.max(1.0e-3)
}

/// Whether a caster's sphere (`radius`, `distance` from the light) covers less than its
/// step's [`CUTOFF_TEXELS`] across in a map of `scale` ([`map_scale`]).
pub fn cut_off(radius: f32, distance: f32, scale: f32, step: u8) -> bool {
    let texels = radius * scale / distance.max(1.0);
    texels < CUTOFF_TEXELS[usize::from(step.min(2))]
}

/// `NVF_LIGHT_SHADOW_CUTOFF=0` (a measuring aid): no small-caster cut-off; every caster
/// Bevy finds is drawn and marks the maps, as before 2026-11-18.
fn cutoff_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_CUTOFF").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_CUTOFF=0, no small-caster cut-off");
        }
        !off
    })
}

/// A cube face counts as in the camera's view while its pyramid comes within this far of
/// the view's frustum (game units) [engineering choice: a face at the edge of the screen is
/// drawn before it shows much, so a turn of the camera rarely finds it stale].
pub const FACE_VIEW_MARGIN: f32 = 64.0;

/// Which faces of a point light's cube at `light` with `range` can show in a camera's view
/// (bit `i` for face `i` in Bevy's cube face order, `CUBE_MAP_FACES`). A face is out when
/// its pyramid (the light's point and the four corners of its square at the range) lies
/// wholly outside one of the view's side or near planes, moved out by `margin`; the far
/// plane is left out, as Bevy's culling of lights does. Conservative: a face it keeps may
/// still be out of view, a face it drops is not in it.
pub fn faces_in_view(view: &Frustum, light: Vec3, range: f32, margin: f32) -> u8 {
    let planes = &view.half_spaces[..=bevy::shape::ViewFrustum::NEAR_PLANE_IDX];
    let mut faces = 0;
    for (i, face) in CUBE_MAP_FACES.iter().enumerate() {
        let t = face.target;
        // Two axes square to the face's.
        let a = Vec3::new(t.y.abs(), t.z.abs(), t.x.abs());
        let b = t.cross(a);
        let points = [light, light + range * (t + a + b), light + range * (t + a - b), light + range * (t - a + b), light + range * (t - a - b)];
        let out = planes.iter().any(|h| points.iter().all(|p| h.normal_d().dot(p.extend(1.0)) + margin <= 0.0));
        if !out {
            faces |= 1 << i;
        }
    }
    faces
}

/// `NVF_LIGHT_SHADOW_FACE_CULL=0` (a measuring aid): every marked face draws on its turn,
/// in view or not, as before 2026-11-18.
fn face_cull_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_FACE_CULL").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_FACE_CULL=0, faces out of view drawn too");
        }
        !off
    })
}

/// The largest scale along a transform's axes (no decomposition into a rotation).
fn max_scale(t: &GlobalTransform) -> f32 {
    let m = t.affine().matrix3;
    m.x_axis.length().max(m.y_axis.length()).max(m.z_axis.length())
}

/// Shapes looked up through a [`LampGrid`] once more than this many moved in a frame (fewer
/// are tested against every lamp) [engineering choice: the grid costs about as much to
/// build as a few dozen shapes tested against 34 lamps].
pub const GRID_AFTER: usize = 32;

/// A caster sphere up to this radius (game units) is looked up in one cell of a
/// [`LampGrid`]; a larger one is tested against every lamp [engineering choice: nearly every
/// shape is smaller, a building is not].
pub const GRID_MARGIN: f32 = 512.0;

/// A broad phase over the casting lights' spheres for marking: a grid whose cells are as
/// large as the longest reach plus [`GRID_MARGIN`], each lamp listed in every cell its sphere
/// grown by the margin overlaps (at most 27). A caster sphere within the margin reaches
/// only lamps listed in the cell its centre lies in.
pub struct LampGrid {
    cell: f32,
    cells: bevy::platform::collections::HashMap<IVec3, Vec<u16>>,
    all: Vec<u16>,
}

impl LampGrid {
    /// The grid of lamps (centre, range), indexed in the order given.
    pub fn new(lamps: &[(Vec3, f32)]) -> Self {
        let cell = lamps.iter().map(|l| l.1).fold(0.0, f32::max) + GRID_MARGIN;
        let mut cells: bevy::platform::collections::HashMap<IVec3, Vec<u16>> = Default::default();
        for (i, (at, range)) in lamps.iter().enumerate() {
            let r = range + GRID_MARGIN;
            let (lo, hi) = (((*at - r) / cell).floor().as_ivec3(), ((*at + r) / cell).floor().as_ivec3());
            for x in lo.x..=hi.x {
                for y in lo.y..=hi.y {
                    for z in lo.z..=hi.z {
                        cells.entry(IVec3::new(x, y, z)).or_default().push(i as u16);
                    }
                }
            }
        }
        Self { cell, cells, all: (0..lamps.len() as u16).collect() }
    }

    /// The lamps a sphere at `centre` with `radius` may reach (a superset).
    pub fn candidates(&self, centre: Vec3, radius: f32) -> &[u16] {
        if radius > GRID_MARGIN {
            return &self.all;
        }
        self.cells.get(&(centre / self.cell).floor().as_ivec3()).map_or(&[], |v| v.as_slice())
    }
}

/// Whether the light's sphere at `light` with `range` and a caster's sphere meet.
fn reaches(light: Vec3, range: f32, sphere: &Sphere) -> bool {
    Vec3::from(sphere.center).distance_squared(light) <= (range + sphere.radius) * (range + sphere.radius)
}

/// How far `light` lies outside the box `aabb` placed by `to_world` (game units, 0 inside),
/// when the box is small enough to be a fixture ([`FIXTURE_RADIUS`]); `None` for a larger one.
pub fn fixture_gap(light: Vec3, to_world: &bevy::math::Affine3A, aabb: &Aabb) -> Option<f32> {
    let m = to_world.matrix3;
    let scale = Vec3::new(m.x_axis.length(), m.y_axis.length(), m.z_axis.length());
    let half = Vec3::from(aabb.half_extents) * scale;
    let radius = half.length();
    if radius > FIXTURE_RADIUS {
        return None;
    }
    let centre = Vec3::from(to_world.transform_point3a(aabb.center));
    // Far from its sphere: no need for the box.
    let far = centre.distance(light) - radius;
    if far > FIXTURE_MARGIN {
        return Some(far);
    }
    let local = Vec3::from(to_world.inverse().transform_point3a(light.into()) - aabb.center);
    let outside = (local.abs() * scale - half).max(Vec3::ZERO);
    Some(outside.length())
}

/// `NVF_LIGHT_SHADOW_FIXTURES=1` (a test aid): log, once for each light when it starts
/// casting, the shapes taken as its fixture and the small ones near it that are not.
fn log_fixtures() -> bool {
    static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LOG.get_or_init(|| std::env::var("NVF_LIGHT_SHADOW_FIXTURES").is_ok_and(|v| v.trim() == "1"))
}

/// `NVF_LIGHT_SHADOW_REFRESH=<frames>` (a measuring aid): every light is drawn again in
/// full this often instead of [`Tuning::refresh`], to measure what switching lights on costs.
fn refresh_override() -> Option<u64> {
    static REFRESH: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *REFRESH.get_or_init(|| {
        let v = std::env::var("NVF_LIGHT_SHADOW_REFRESH").ok()?.trim().parse::<u64>().ok().filter(|v| *v > 0)?;
        info!("light shadows: NVF_LIGHT_SHADOW_REFRESH={v}");
        Some(v)
    })
}

/// The near plane in use: [`NEAR_Z`], or `NVF_LIGHT_SHADOW_NEAR` (game units) for a run.
fn near_z() -> f32 {
    static NEAR_PLANE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *NEAR_PLANE.get_or_init(|| match std::env::var("NVF_LIGHT_SHADOW_NEAR").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| *v > 0.0) {
        Some(v) => {
            info!("light shadows: NVF_LIGHT_SHADOW_NEAR={v}");
            v
        }
        None => NEAR_Z,
    })
}

/// `NVF_LIGHT_SHADOW_BIAS=<depth>,<normal>` (a test aid): overrides the game-unit shadow
/// biases ([`DEFAULT_DEPTH_BIAS`], [`DEFAULT_NORMAL_BIAS`]) of the casting lights.
fn biases() -> (f32, f32) {
    static BIAS: std::sync::OnceLock<(f32, f32)> = std::sync::OnceLock::new();
    *BIAS.get_or_init(|| {
        let parsed = std::env::var("NVF_LIGHT_SHADOW_BIAS").ok().and_then(|v| {
            let (d, n) = v.split_once(',')?;
            Some((d.trim().parse().ok()?, n.trim().parse().ok()?))
        });
        match parsed {
            Some(b) => {
                info!("light shadows: NVF_LIGHT_SHADOW_BIAS depth {} normal {}", b.0, b.1);
                b
            }
            None => (DEFAULT_DEPTH_BIAS, DEFAULT_NORMAL_BIAS),
        }
    })
}

/// `NVF_PLAY_TEST_LIGHT_SHADOW_HOLD=0`: nothing held, every casting light draws in full
/// every frame.
fn holding() -> bool {
    static HOLD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HOLD.get_or_init(|| {
        let off = std::env::var("NVF_PLAY_TEST_LIGHT_SHADOW_HOLD").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: test aid NVF_PLAY_TEST_LIGHT_SHADOW_HOLD=0, every map drawn every frame");
        }
        !off
    })
}

/// The layers a casting light draws its shadow casters from (the sun's, sky.rs).
fn casting_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, crate::sky::SHADOW_LAYER, crate::sky::BODY_LAYER])
}

/// The cube side for the Shadow resolution row's side.
pub fn cube_side(directional: usize) -> usize {
    if directional >= 4096 { 1024 } else { 512 }
}

/// Megabytes (10^6 bytes) of a cube texture of `cubes` cubes of `side` texels square, 32-bit
/// depth (`CORE_3D_SHADOW_MAP_FORMAT`, Depth32Float): the static shadow cache costs as much
/// again as the sampled cubes.
pub fn cube_texture_mb(side: usize, cubes: u32) -> f64 {
    (side * side * 4 * 6) as f64 * f64::from(cubes) / 1.0e6
}

type Placed<'a> = (
    Entity,
    &'a GlobalTransform,
    &'a InheritedVisibility,
    &'a ViewVisibility,
    Option<&'a mut PointLight>,
    Option<&'a mut SpotLight>,
    Option<&'a MovingLight>,
    Option<&'a CubemapFrusta>,
    Option<&'a Frustum>,
    Has<CastingLight>,
    Option<&'a ShadowSource>,
);

/// The lights that may cast: the placed ones and the [`ShadowSource`]s.
type Casters = Or<(With<SessionLight>, With<ShadowSource>)>;

/// Shapes whose place or visibility changed (skinned ones go by their skeleton).
type MovedFilter = (With<Mesh3d>, Without<NotShadowCaster>, Without<SkinnedMesh>, Or<(Changed<GlobalTransform>, Changed<InheritedVisibility>)>);

type Skinned<'a> = (Entity, &'a SkinnedMesh, &'a GlobalTransform, &'a InheritedVisibility, Option<&'a RenderLayers>);

type MovedCasters<'a> = (
    Entity,
    &'a GlobalTransform,
    &'a Aabb,
    Ref<'a, InheritedVisibility>,
    Option<&'a RenderLayers>,
    Option<&'a Name>,
    Option<&'a ChildOf>,
    Has<ShadowDynamic>,
);

/// Shapes new this frame, or whose layers changed, not yet dynamic ([`classify_casters`]).
type NewShapes<'a> = (Entity, Has<SkinnedMesh>, Option<&'a RenderLayers>);
type NewShapesFilter = (With<Mesh3d>, Without<ShadowDynamic>, Or<(Added<Mesh3d>, Changed<RenderLayers>)>);

/// An actor's or creature's rig root, and the player's ragdoll rig.
type Rigs = Or<(With<crate::limbs::ActorRig>, With<crate::ragdoll::PlayerRig>)>;
type NewRigs = Or<(Added<crate::limbs::ActorRig>, Added<crate::ragdoll::PlayerRig>)>;

/// The layers only the player's body is on (sky.rs): its shapes are dynamic.
fn body_layers() -> RenderLayers {
    RenderLayers::from_layers(&[crate::sky::SHADOW_LAYER, crate::sky::BODY_LAYER])
}

/// Mark the shapes that move as [`ShadowDynamic`] from the start: skinned shapes, the
/// player's body's (its layers), and every shape under an actor's or creature's rig (its
/// weapons, hat, eyes: they follow the skeleton), found from a new shape up its ancestors
/// and from a new rig down its descendants. Any other shape becomes dynamic in
/// [`shadow_lights`] once it moves ([`shape_change`]). It runs only while the row is on
/// ([`live`]), so when the row is switched on every shape and rig not yet dynamic is looked
/// at once (the shapes spawned while it was off).
#[allow(clippy::too_many_arguments)]
fn classify_casters(
    mut commands: Commands,
    settings: Res<LightShadows>,
    new: Query<NewShapes, NewShapesFilter>,
    every: Query<NewShapes, (With<Mesh3d>, Without<ShadowDynamic>)>,
    rigs: Query<Entity, NewRigs>,
    rigged: Query<(), Rigs>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    shapes: Query<(), (With<Mesh3d>, Without<ShadowDynamic>)>,
) {
    if settings.choice == 0 {
        return;
    }
    // Switched on (or the game started with it on): every shape, not only the new ones.
    let all = settings.is_changed();
    let body = body_layers();
    let mut marked = EntityHashSet::default();
    let mut look = |(e, skinned, layers): (Entity, bool, Option<&RenderLayers>)| {
        if skinned || layers.is_some_and(|l| l.intersects(&body)) || parents.iter_ancestors(e).any(|a| rigged.contains(a)) {
            marked.insert(e);
        }
    };
    if all {
        every.iter().for_each(&mut look);
    } else {
        new.iter().for_each(&mut look);
    }
    // A new rig's shapes (the whole pass above already found every shape under a rig).
    if !all {
        for rig in &rigs {
            marked.extend(children.iter_descendants(rig).filter(|d| shapes.contains(*d)));
        }
    }
    for e in marked {
        commands.entity(e).try_insert(ShadowDynamic);
    }
}

/// A light this frame: what [`shadow_lights`] works out for it.
struct Lamp {
    entity: Entity,
    at: Vec3,
    range: f32,
    spot: bool,
    seen: bool,
    gap: f32,
    transient: bool,
    /// Its step of detail and map scale ([`LampState::step`]).
    step: u8,
    map_scale: f32,
}

/// What [`shadow_lights`] hands on: the renderer's hold, the loading screen's wait, the
/// lights shown to Bevy's culling and the idle frames ([`live`]); and the textures' inputs.
type Outputs<'w> = (
    ResMut<'w, HoldLightShadowMaps>,
    Res<'w, MapTextureInputs>,
    Option<ResMut<'w, nvf_play::loading::Loading>>,
    ResMut<'w, NvfShadowLightsShown>,
    ResMut<'w, LightShadowsIdle>,
);

/// Choose the lights that cast, mark what changed in their maps, choose which draw this
/// frame, and set them up for Bevy and the patched renderer.
#[allow(clippy::too_many_arguments)]
fn shadow_lights(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<LightShadows>,
    sky: Option<Res<crate::sky::SkyState>>,
    directional: Option<Res<DirectionalLightShadowMap>>,
    mut cube: ResMut<PointLightShadowMap>,
    mut state: ResMut<LampShadowState>,
    (mut out, inputs, mut loading, mut shown, mut idle): Outputs,
    camera: Query<(&GlobalTransform, Option<&Frustum>), crate::fly::WorldCamera>,
    mut lights: Query<Placed, Casters>,
    moved: Query<MovedCasters, MovedFilter>,
    skinned: Query<Skinned, Without<NotShadowCaster>>,
    names: Query<&Name>,
    joints: Query<Ref<GlobalTransform>>,
    (mut removed, mut removed_dynamic): (RemovedComponents<Mesh3d>, RemovedComponents<ShadowDynamic>),
    mut logged: Local<Option<(usize, usize, usize)>>,
) {
    let _timed = Timed::start(0);
    let tune = tuning(settings.choice).map(|t| Tuning { refresh: refresh_override().unwrap_or(t.refresh), ..t });
    let reach = sky.as_ref().map_or(0.0, |s| s.shadow_distance * s.shadow_reach.max(1.0));
    let (at, view) = camera.iter().next().map_or((None, None), |(c, f)| (Some(c.translation()), f));
    let state = &mut *state;
    state.frame += 1;
    let frame = state.frame;
    let hold_all = !holding();
    // The static shadow cache: point lamps' faces drawn from the static casters into a cache
    // and the dynamic casters drawn over a copy of it.
    let cache_on = static_cache_on() && tune.is_some();
    // Under a loading screen, once the renderer has its cells: every seen lamp draws its
    // marked faces at once, in or out of view, with no budget, so the shadows are whole when
    // the screen comes down.
    let screen_up = loading.as_ref().is_some_and(|l| l.active());
    let predraw = predrawing(tune.is_some(), predraw_on(), screen_up, loading.as_ref().is_some_and(|l| l.render_busy));
    if predraw && state.predraw.began.is_none() {
        state.predraw.began = Some(time.elapsed_secs());
    }

    // 1. Which lights cast: the placed ones (with grass fires and attached lights) up to
    // their cap; apart from them, the transients the camera sees, up to theirs within a
    // shorter reach.
    // Sets: every light in the world is looked up in them a few times a frame.
    let (chosen, transients): (EntityHashSet, EntityHashSet) = match (tune, at) {
        (Some(t), Some(at)) if reach > 0.0 => {
            let mut placed: Vec<(Entity, (f32, f32))> = Vec::new();
            let mut quick: Vec<(Entity, (f32, f32))> = Vec::new();
            for (e, place, vis, _, point, spot, moving, _, _, casting, source) in &lights {
                if !vis.get() {
                    continue;
                }
                let (range, intensity) = match (&point, &spot) {
                    (Some(p), _) => (p.range, p.intensity),
                    (None, Some(s)) => (s.range, s.intensity),
                    _ => continue,
                };
                if range <= 0.0 || moving.map_or(intensity, |m| m.full) <= 0.0 {
                    continue;
                }
                let r = rank(place.translation().distance(at), range, casting);
                if source.is_some_and(|s| s.0.transient()) {
                    if in_view(view, place.translation(), range) {
                        quick.push((e, r));
                    }
                } else {
                    placed.push((e, r));
                }
            }
            let transients = pick(&mut quick, transient_reach(reach), t.transients);
            let mut chosen = pick(&mut placed, reach, t.lights);
            chosen.extend_from_slice(&transients);
            (chosen.into_iter().collect(), transients.into_iter().collect())
        }
        _ => (EntityHashSet::default(), EntityHashSet::default()),
    };
    if tune.is_some() {
        let side = cube_side(directional.as_ref().map_or(2048, |d| d.size));
        state.cube_before.get_or_insert(cube.size);
        if cube.size != side {
            cube.size = side;
        }
    } else if let Some(side) = state.cube_before.take() {
        // Off: Bevy's cube side as it was.
        if cube.size != side {
            cube.size = side;
        }
    }
    // Their states and slots. A light no longer chosen is held, its slot kept and nothing
    // drawn, while its shadow fades out; then it loses them, as does a light despawned.
    let mut lamps: Vec<Lamp> = Vec::with_capacity(chosen.len());
    let mut alive = EntityHashSet::default();
    for (e, t, _, seen, point, spot, ..) in &lights {
        if point.is_none() && spot.is_none() {
            continue;
        }
        if !chosen.contains(&e) {
            if state.lamps.contains_key(&e) {
                alive.insert(e);
            }
            continue;
        }
        let is_point = point.is_some();
        // A spot light's map spans its cone at the sun's side; a cube face 90 degrees at the
        // cube's side.
        let scale = match &spot {
            Some(s) if !is_point => map_scale(directional.as_ref().map_or(2048, |d| d.size) as f32, s.outer_angle.tan()),
            _ => map_scale(cube.size as f32, 1.0),
        };
        let range = point.map(|p| p.range).or(spot.map(|s| s.range)).unwrap_or(0.0);
        let transient = transients.contains(&e);
        if transient && !state.lamps.contains_key(&e) {
            state.transients_cast += 1;
        }
        let gap = at.map_or(0.0, |a| (t.translation().distance(a) - range).max(0.0));
        let entry = state.lamps.entry(e).or_insert(LampState {
            dirty: 0,
            fresh: true,
            refreshed: 0,
            phase: e.index_u32() as u64,
            at: t.translation(),
            transient,
            spot: !is_point,
            fade: 0.0,
            leaving: false,
            step: stepped(0, gap, 0.0),
            map_scale: scale,
            cut_any: false,
            far_fade: 1.0,
            deferred: 0,
            static_dirty: ALL_FACES,
            pending: 0,
            plain: plain_path(cache_on, hold_all, transient, !is_point, false),
            to_static: 0,
            composite: 0,
        });
        // Its shadow fades out over the last stretch of the reach (not a transient's: it is
        // chosen within a shorter reach and lasts a moment).
        entry.far_fade = if transient || !distance_fade_on() { 1.0 } else { distance_fade(gap, reach) };
        if entry.far_fade < 1.0 {
            state.pass.distance_faded += 1;
        }
        if entry.fresh {
            // Its full redraws fall at a time of its own.
            entry.refreshed = frame.saturating_sub(entry.phase % tune.map_or(1, |t| t.refresh).max(1));
        }
        if entry.leaving {
            // Chosen again while fading out: what changed meanwhile was not marked.
            entry.leaving = false;
            entry.dirty = ALL_FACES;
            entry.static_dirty = ALL_FACES;
        }
        // Its step of detail, with hysteresis; a step finer draws its maps again in full when
        // a caster was cut from them, so it comes back.
        let step = stepped(entry.step, gap, STEP_HYSTERESIS);
        if step < entry.step && entry.cut_any && cutoff_on() {
            entry.dirty = ALL_FACES;
            entry.static_dirty = ALL_FACES;
            entry.cut_any = false;
            state.pass.step_redraws += 1;
        }
        (entry.step, entry.map_scale) = (step, scale);
        state.pass.steps[usize::from(step.min(2))] += 1;
        assign_slot(if is_point { &mut state.slots } else { &mut state.spot_slots }, e);
        // A transient was chosen only if the camera sees it this frame (a new light's
        // `ViewVisibility` is still false).
        lamps.push(Lamp { entity: e, at: t.translation(), range, spot: !is_point, seen: transient || seen.get(), gap, transient, step, map_scale: scale });
    }
    // The fades: rising once a light's maps were first drawn (a transient's full at once),
    // falling while it leaves (with a mode on; turned off, every light is dropped at once).
    let dt = time.delta_secs();
    state.lamps.retain(|e, s| {
        if chosen.contains(e) {
            s.fade = if s.transient {
                1.0
            } else if s.fresh {
                0.0
            } else {
                fade_step(s.fade, true, dt)
            };
            return true;
        }
        match leaving_fade(s.fade, !s.fresh, s.transient, tune.is_some() && alive.contains(e), dt) {
            Some(fade) => {
                s.fade = fade;
                s.leaving = true;
                true
            }
            None => false,
        }
    });
    state.slots.retain(|e, _| state.lamps.get(e).is_some_and(|s| !s.spot));
    state.spot_slots.retain(|e, _| state.lamps.get(e).is_some_and(|s| s.spot));
    state.capacity = match tune {
        Some(_) => grown_room(state.capacity, state.slots.values().copied(), 0),
        None => 0,
    };
    // The spot lights' room also holds the other shadowed spot lights (the flashlight), so
    // switching one on or off does not change the texture; none while no lamp spot casts.
    state.spot_capacity = match tune {
        Some(_) if !state.spot_slots.is_empty() || state.spot_capacity > 0 => grown_room(state.spot_capacity, state.spot_slots.values().copied(), inputs.other_spots),
        _ => 0,
    };
    // A texture made anew holds no light's maps: its lights draw again as if new, and those
    // leaving are dropped.
    // The static cache gives the cube texture another use (copied into): another texture.
    let point_key = (cube.size, state.capacity, cache_on);
    let spot_key = (directional.as_ref().map_or(2048, |d| d.size), state.spot_capacity, inputs.cascades);
    let new_point = state.point_maps_key.is_some_and(|k| k != point_key);
    let new_spot = state.spot_maps_key.is_some_and(|k| k != spot_key);
    (state.point_maps_key, state.spot_maps_key) = match tune {
        Some(_) => (Some(point_key), Some(spot_key)),
        None => (None, None),
    };
    if new_point || new_spot {
        state.lamps.retain(|_, s| {
            let made_anew = if s.spot { new_spot } else { new_point };
            if !made_anew {
                return true;
            }
            if s.leaving {
                return false;
            }
            (s.fresh, s.dirty, s.static_dirty, s.pending) = (true, ALL_FACES, ALL_FACES, 0);
            if !s.transient {
                s.fade = 0.0;
            }
            true
        });
        state.slots.retain(|e, _| state.lamps.contains_key(e));
        state.spot_slots.retain(|e, _| state.lamps.contains_key(e));
    }

    // 2. What changed in their maps since last frame. Nothing to mark without a lamp. Per
    // shape the lamps are found through a grid once more than a few shapes moved
    // ([`LampGrid`]), and a lamp that draws in full anyway (fresh or every face marked) and
    // faces already marked are not tested again: a reference toggled or faded in town
    // changes thousands of shapes in a frame.
    let layers = casting_layers();
    if !lamps.is_empty() {
        let frusta: Vec<(Option<CubemapFrusta>, Option<Frustum>)> = lamps
            .iter()
            .map(|l| match lights.get(l.entity) {
                Ok((_, _, _, _, _, spot, _, cube, frustum, ..)) => (cube.cloned(), if spot.is_some() { frustum.copied() } else { None }),
                Err(_) => (None, None),
            })
            .collect();
        let spheres: Vec<(Vec3, f32)> = lamps.iter().map(|l| (l.at, l.range)).collect();
        let cut = cutoff_on();
        let mut grid: Option<LampGrid> = None;
        let mut shapes_seen = 0usize;
        for (e, t, aabb, vis, rl, name, parent, dynamic) in &moved {
            if rl.is_some_and(|l| !l.intersects(&layers)) {
                continue;
            }
            // Shown, hidden or new at once; moved once it went past its movement threshold
            // from its last mark. Dynamic actor parts also track orientation via an off-centre
            // probe so rotating a head in dialogue redraws nearby lamps immediately.
            let centre = t.transform_point(Vec3::from(aabb.center));
            let probe = if dynamic {
                t.transform_point(Vec3::from(aabb.center) + Vec3::from(aabb.half_extents) * 0.5)
            } else {
                centre
            };
            let eps = if dynamic { DYNAMIC_MOVE_EPS } else { MOVE_EPS };
            let last = state.shapes_at.get(&e).copied();
            let moved_enough = last.is_none_or(|(_, p, _)| p.distance(probe) > eps);
            if !vis.is_changed() && !moved_enough {
                continue;
            }
            let sphere = Sphere { center: centre.into(), radius: Vec3::from(aabb.half_extents).length() * max_scale(t) };
            // Where it last marked, for the cut-off (a shape shown, hidden or new: none).
            let old = (!vis.is_changed()).then(|| last.map(|(c, _, _)| Sphere { center: c.into(), radius: sphere.radius })).flatten();
            // With the static cache: a static shape shown, hidden or new marks the caches
            // where it is; one that moved becomes dynamic and marks the caches where it was
            // and is; a dynamic one marks only the composites, where it was and is.
            let change = shape_change(dynamic, vis.is_changed(), last.is_some());
            let static_mark = cache_on && change != ShapeChange::Dynamic;
            if cache_on && change == ShapeChange::Promoted {
                commands.entity(e).try_insert(ShadowDynamic);
                state.pass.promoted += 1;
            }
            let sphere = match &old {
                Some(o) if cache_on => merged_sphere(&sphere, o),
                _ => sphere,
            };
            shapes_seen += 1;
            if shapes_seen > GRID_AFTER && grid.is_none() {
                grid = Some(LampGrid::new(&spheres));
            }
            let at = Vec3::from(sphere.center);
            let m = match &grid {
                Some(g) => mark_sphere(&sphere, old.as_ref(), cut, static_mark, g.candidates(at, sphere.radius).iter().map(|&i| i as usize), &lamps, &frusta, &mut state.lamps),
                None => mark_sphere(&sphere, old.as_ref(), cut, static_mark, 0..lamps.len(), &lamps, &frusta, &mut state.lamps),
            };
            state.pass.cut_marks += u64::from(m.cut);
            // Where it was is kept only for shapes near a lamp.
            if m.reached {
                state.shapes_at.insert(e, (centre, probe, frame));
            }
            if m.marked {
                state.causes[0] += 1;
                if state.names.len() < 6 {
                    let parent = parent.and_then(|c| names.get(c.parent()).ok());
                    let n = format!("{}/{}", parent.map_or("-", |n| n.as_str()), name.map_or("-", |n| n.as_str()));
                    if !state.names.contains(&n) {
                        state.names.push(n);
                    }
                }
            }
        }
        // An actor's skinned shapes share its skeleton: each skeleton (by its first joint) is
        // gone over and marks once a frame, not once for each of its shapes.
        let mut skeletons_seen = EntityHashSet::default();
        for (_, skin, t, seen, rl) in &skinned {
            if !seen.get() || rl.is_some_and(|l| !l.intersects(&layers)) {
                continue;
            }
            let root = t.translation();
            if !lamps.iter().any(|l| l.at.distance_squared(root) <= (l.range + 512.0) * (l.range + 512.0)) {
                continue;
            }
            let Some(&e) = skin.joints.first() else { continue };
            if !skeletons_seen.insert(e) {
                continue;
            }
            let (mut changed, mut lo, mut hi) = (false, Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for j in skin.joints.iter().filter_map(|&j| joints.get(j).ok()) {
                changed |= j.is_changed();
                let pos = j.translation();
                let tip = pos + j.forward().as_vec3() * 6.0;
                lo = lo.min(pos).min(tip);
                hi = hi.max(pos).max(tip);
            }
            if !changed || lo.x > hi.x {
                continue;
            }
            let last = state.skeletons_at.get(&e).copied();
            let moved_enough = last.is_none_or(|(l, h, _)| (l - lo).abs().max_element() > DYNAMIC_MOVE_EPS || (h - hi).abs().max_element() > DYNAMIC_MOVE_EPS);
            if !moved_enough {
                continue;
            }
            state.skeletons_at.insert(e, (lo, hi, frame));
            // The joints' box, grown by a limb's thickness [engineering choice].
            let box_sphere = |lo: Vec3, hi: Vec3| Sphere { center: ((lo + hi) * 0.5).into(), radius: (hi - lo).length() * 0.5 + 16.0 };
            let old = last.map(|(l, h, _)| box_sphere(l, h));
            // A skeleton is dynamic: with the static cache it marks the composites of the
            // faces it was and is in.
            let now = box_sphere(lo, hi);
            let sphere = match &old {
                Some(o) if cache_on => merged_sphere(&now, o),
                _ => now,
            };
            let m = mark_sphere(&sphere, old.as_ref(), cut, false, 0..lamps.len(), &lamps, &frusta, &mut state.lamps);
            state.pass.cut_marks += u64::from(m.cut);
            if m.marked {
                state.causes[1] += 1;
            }
        }
    }
    // Something removed: the lights around the camera, where the player takes, kills and
    // breaks things, are drawn again in full [engineering choice]. With the static cache, a
    // removal of dynamic shapes only (an actor's part, a moved item) redraws no cache.
    let gone_dynamic: EntityHashSet = removed_dynamic.read().collect();
    let (mut any_gone, mut static_gone) = (false, false);
    for e in removed.read() {
        any_gone = true;
        static_gone |= !gone_dynamic.contains(&e);
    }
    if any_gone {
        if let Some(a) = at {
            for l in lamps.iter().filter(|l| l.at.distance(a) <= l.range + 512.0) {
                if let Some(s) = state.lamps.get_mut(&l.entity) {
                    s.dirty = ALL_FACES;
                    if static_gone || !cache_on {
                        s.static_dirty = ALL_FACES;
                    }
                    state.causes[2] += 1;
                }
            }
        }
    }
    // The light moved past LIGHT_MOVE_EPS, or its turn for a full redraw came.
    for l in &lamps {
        if let (Some(s), Some(tune)) = (state.lamps.get_mut(&l.entity), tune) {
            let moved = light_moved(s.at, l.at);
            if !l.transient && (moved || frame.saturating_sub(s.refreshed) >= tune.refresh) {
                s.dirty = ALL_FACES;
                s.static_dirty = ALL_FACES;
                s.refreshed = frame;
                s.at = l.at;
                state.causes[3] += 1;
                // A light that moves would draw its cache again each time: the plain path.
                s.plain |= moved;
            }
        }
    }

    // Under a loading screen, the lamps the camera does not see draw too, once those it sees
    // are done: shown to Bevy's culling (vendored bevy_light `NvfShadowLightsShown`), which
    // finds their casters and marks them visible, so the renderer draws their maps; the
    // nearest few a frame. Their faces out of view go into the static cache as any.
    let to_draw = |s: &LampShadowState, e: &Entity| s.lamps.get(e).is_some_and(|s| s.fresh || s.dirty != 0);
    let shown_now = if predraw {
        let seen_waiting = lamps.iter().any(|l| !l.transient && l.seen && to_draw(state, &l.entity));
        let mut unseen: Vec<(Entity, f32)> = lamps.iter().filter(|l| !l.transient && !l.seen && to_draw(state, &l.entity)).map(|l| (l.entity, l.gap)).collect();
        shown_unseen(&mut unseen, seen_waiting, PREDRAW_UNSEEN_PER_FRAME)
    } else {
        Vec::new()
    };
    for l in lamps.iter_mut().filter(|l| shown_now.contains(&l.entity)) {
        l.seen = true;
    }
    if !shown.lights.is_empty() || !shown_now.is_empty() {
        shown.lights.clear();
        shown.lights.extend(shown_now.iter().copied());
    }

    // 3. Which lights draw this frame: those never drawn first, then the nearest marked ones
    // on their turn; at most `draws` lights (point or spot), and apart from them
    // `transient_draws` transient ones.
    lamps.sort_by(|a, b| b.seen.cmp(&a.seen).then(a.gap.total_cmp(&b.gap)));
    // The faces of each point light that can show in the camera's view (all for a spot
    // light, one never drawn, a transient, or with face culling off): a marked face out of
    // view stays marked, held, until it comes into view.
    let cull = face_cull_on() && !hold_all && !predraw;
    let in_view: Vec<u8> = lamps
        .iter()
        .map(|l| {
            let fresh = state.lamps.get(&l.entity).is_none_or(|s| s.fresh);
            match view {
                Some(v) if cull && !l.spot && !l.transient && !fresh => faces_in_view(v, l.at, l.range, FACE_VIEW_MARGIN),
                _ => ALL_FACES,
            }
        })
        .collect();
    // Never drawn first, then those whose dynamic casters are to be put back over a fresh
    // static cache, then those with marked faces that came into view, then the rest as
    // sorted.
    let mut order: Vec<usize> = (0..lamps.len()).collect();
    order.sort_by_key(|&i| {
        let s = state.lamps.get(&lamps[i].entity);
        (!s.is_some_and(|s| s.fresh), !s.is_some_and(|s| s.pending != 0), !s.is_some_and(|s| s.deferred & in_view[i] != 0))
    });
    let (mut draws, mut transient_draws) = tune.map_or((0, 0), |t| (t.draws, t.transient_draws));
    let mut drawing: EntityHashMap<u8> = EntityHashMap::default();
    out.held.clear();
    out.to_static.clear();
    out.on_static.clear();
    for i in order {
        let l = &lamps[i];
        let Some(s) = state.lamps.get_mut(&l.entity) else { continue };
        let all = if l.spot { 1 } else { ALL_FACES };
        s.dirty &= all;
        s.static_dirty &= s.dirty;
        s.pending &= s.dirty;
        (s.to_static, s.composite) = (0, 0);
        let Some(t) = tune else { continue };
        let every = if l.gap <= NEAR { t.near_every } else { t.far_every };
        // A transient (a muzzle flash, 0.1 s; an explosion's light, still while it fades)
        // draws once and is held for the rest of its short life: a flash moved by the gun's
        // recoil was drawn 3.5 times on average, measured in the Prospector Saloon gunfight
        // [engineering choice, 2026-11-18].
        let want = if hold_all || s.fresh {
            all
        } else if l.transient {
            0
        } else if predraw || s.pending != 0 || its_turn(frame, s.phase, every) {
            state.pass.faces_out_of_view += u64::from((s.dirty & !in_view[i]).count_ones());
            s.dirty & in_view[i]
        } else {
            0
        };
        s.deferred = s.dirty & !in_view[i];
        // Only a light the camera saw last frame (a transient: sees this frame) is drawn by
        // Bevy this frame.
        let budget = if l.transient { &mut transient_draws } else { &mut draws };
        let draw = if want != 0 && l.seen && takes_budget(budget, hold_all || predraw) { want } else { 0 };
        if draw != 0 {
            // With the static cache: faces whose cache is out of date drawn into it (then
            // copied out), the others composited from it.
            let (to_static, composite) = face_modes(draw, if s.fresh { all } else { s.static_dirty }, !s.plain);
            (s.to_static, s.composite) = (to_static, composite);
            if to_static != 0 {
                out.to_static.insert(l.entity, to_static);
            }
            if composite != 0 {
                out.on_static.insert(l.entity, composite);
            }
            state.pass.static_faces += u64::from(to_static.count_ones());
            state.pass.composited += u64::from(composite.count_ones());
            if predraw && !l.transient {
                state.predraw.lights.insert(l.entity);
                if shown_now.contains(&l.entity) {
                    state.predraw.unseen.insert(l.entity);
                }
                state.predraw.faces += u64::from(draw.count_ones());
                state.predraw.static_faces += u64::from(to_static.count_ones());
            }
            s.static_dirty &= !draw;
            s.pending &= !draw;
            s.dirty &= !draw;
            s.fresh = false;
            if draw == all {
                s.at = l.at;
            }
            if l.transient {
                state.transients_drawn += u64::from(draw.count_ones());
                state.transients_switched += u64::from(!l.spot);
            } else {
                state.drawn += u64::from(draw.count_ones());
                state.switched += u64::from(!l.spot);
            }
            drawing.insert(l.entity, draw);
        }
        // A light is switched on only to draw, a point light's other faces held.
        if draw != 0 {
            out.held.insert(l.entity, all & !draw);
        }
    }

    // The loading screen waits while the pre-draw draws: lamps drew this frame (their
    // composites may follow) or lamps, seen or not, still have marked faces; at most
    // PREDRAW_CAP_SECS. When the screen comes down, what was pre-drawn is logged.
    let busy = predraw && {
        let drew = lamps.iter().any(|l| !l.transient && drawing.contains_key(&l.entity));
        let waiting = lamps.iter().any(|l| !l.transient && state.lamps.get(&l.entity).is_some_and(|s| s.fresh || s.dirty != 0));
        predraw_busy(drew, waiting, state.predraw.began.map_or(0.0, |b| time.elapsed_secs() - b))
    };
    if let Some(loading) = loading.as_mut() {
        if loading.lamp_shadows_busy != busy {
            loading.lamp_shadows_busy = busy;
        }
    }
    if state.predraw.screen && !screen_up {
        let p = std::mem::take(&mut state.predraw);
        if p.began.is_some() {
            let casting: Vec<&LampState> = state.lamps.values().filter(|s| !s.transient && !s.leaving).collect();
            info!(
                "light shadows: {} lights pre-drawn under the loading screen ({} faces, {} into the static cache); {} casting, {} of them not drawn yet; {} of those pre-drawn out of view",
                p.lights.len(),
                p.faces,
                p.static_faces,
                casting.len(),
                casting.iter().filter(|s| s.fresh).count(),
                p.unseen.len()
            );
            state.pass.predrawn += p.lights.len() as u64;
        }
    }
    state.predraw.screen = screen_up;

    // 4. Set the lights up: shadows switched on to draw, the casters' layers, the near plane
    // and biases.
    let (near, bias) = (near_z(), biases());
    let (mut points, mut spots) = (0, 0);
    for (e, _, _, _, point, spot, _, _, _, casting, _) in &mut lights {
        let cast = chosen.contains(&e);
        // The counts logged leave the transients out, or each muzzle flash wrote a line.
        let counted = cast && !transients.contains(&e);
        if let Some(mut p) = point {
            points += usize::from(counted);
            let on = cast && drawing.contains_key(&e);
            if p.shadow_maps_enabled != on {
                p.shadow_maps_enabled = on;
            }
            if cast && p.shadow_map_near_z != near {
                p.shadow_map_near_z = near;
            }
            if cast && (p.shadow_depth_bias, p.shadow_normal_bias) != bias {
                (p.shadow_depth_bias, p.shadow_normal_bias) = bias;
            }
        }
        if let Some(mut s) = spot {
            spots += usize::from(counted);
            let on = cast && drawing.contains_key(&e);
            if s.shadow_maps_enabled != on {
                s.shadow_maps_enabled = on;
            }
            if cast && s.shadow_map_near_z != near {
                s.shadow_map_near_z = near;
            }
            if cast && (s.shadow_depth_bias, s.shadow_normal_bias) != bias {
                (s.shadow_depth_bias, s.shadow_normal_bias) = bias;
            }
        }
        match (cast, casting) {
            (true, false) => {
                commands.entity(e).insert((CastingLight, casting_layers()));
            }
            (false, true) => {
                commands.entity(e).remove::<(CastingLight, RenderLayers)>();
            }
            _ => {}
        }
    }
    if out.slots != state.slots {
        out.slots.clone_from(&state.slots);
    }
    if out.static_cache != cache_on {
        out.static_cache = cache_on;
    }
    if out.point_capacity != state.capacity {
        out.point_capacity = state.capacity;
    }
    if out.spot_slots != state.spot_slots {
        out.spot_slots.clone_from(&state.spot_slots);
    }
    if out.spot_capacity != state.spot_capacity {
        out.spot_capacity = state.spot_capacity;
    }
    // The fades below 1 (a light absent casts in full): in and out over time, times by
    // distance.
    out.fade.clear();
    out.fade.extend(state.lamps.iter().map(|(e, s)| (*e, s.fade * s.far_fade)).filter(|(_, f)| *f < 1.0));
    // The renderer holds the lamps only while a mode is on; off, its point and spot light
    // paths are upstream's (bevy_pbr `HoldLightShadowMaps::active`).
    if out.active != tune.is_some() {
        out.active = tune.is_some();
    }
    removed.clear();
    idle.frames = if tune.is_some() { 0 } else { idle.frames.saturating_add(1).min(WIND_DOWN_FRAMES) };

    let now = (settings.choice, points, spots);
    if *logged != Some(now) {
        *logged = Some(now);
        match tune {
            None => info!("light shadows: off"),
            Some(t) => info!(
                "light shadows: {} cast ({points} point, {spots} spot lights) within {reach:.0} units, {} at most; cubes {}x{}, room for {}; spot maps room for {}; transients (muzzle flashes, explosions) within {:.0} units, {} at most; static shadow cache {}",
                points + spots,
                t.lights,
                cube.size,
                cube.size,
                state.capacity,
                state.spot_capacity,
                transient_reach(reach),
                t.transients,
                if cache_on { format!("on ({:.0} MB)", cube_texture_mb(cube.size, state.capacity)) } else { "off".to_string() }
            ),
        }
    }
    // A summary of what was drawn, every 10 s of real time. With the mode off, the renderer's
    // counts and the systems' times are dropped, so the first summary after it is switched on
    // counts only its own frames.
    if tune.is_none() {
        bevy::pbr::nvf_take_light_view_counts();
        take_system_times();
    } else {
        state.frames += 1;
        state.since += time.delta_secs();
        if state.since >= 10.0 {
            let per = |n: u64| n as f32 / state.frames as f32;
            info!(
                "light shadows: {:.2} point lights switched on and {:.1} faces drawn a frame over {} frames ({} lights casting); marked a frame by shapes {:.2}, skeletons {:.2}, removals {:.2}, lights {:.2}; shapes such as {}; transients {} cast, {} switched on, {} faces drawn",
                per(state.switched),
                per(state.drawn),
                state.frames,
                state.lamps.values().filter(|l| !l.transient && !l.leaving).count(),
                per(state.causes[0]),
                per(state.causes[1]),
                per(state.causes[2]),
                per(state.causes[3]),
                if state.names.is_empty() { "none".to_string() } else { state.names.join(", ") },
                state.transients_cast,
                state.transients_switched,
                state.transients_drawn
            );
            let (ns, max) = take_system_times();
            info!("{}; {}", renderer_summary(&bevy::pbr::nvf_take_light_view_counts()), systems_summary(&ns, &max, state.frames));
            let (served, candidates, filed, everywhere) = crate::light_shadow_casters::take_grid_counts();
            state.pass.grid = [served, candidates, filed, everywhere];
            info!("{}", pass_summary(&state.pass, state.frames));
            state.pass = Pass::default();
            (state.switched, state.drawn, state.frames, state.since, state.causes) = (0, 0, 0, 0.0, [0; 4]);
            (state.transients_cast, state.transients_switched, state.transients_drawn) = (0, 0, 0);
            state.names.clear();
            // Forget shapes and skeletons that have not marked for a while.
            let old = frame.saturating_sub(4800);
            state.shapes_at.retain(|_, (_, _, f)| *f >= old);
            state.skeletons_at.retain(|_, (_, _, f)| *f >= old);
        }
    }
}

type CasterLists<'a> = (
    Entity,
    &'a GlobalTransform,
    Option<&'a mut CubemapVisibleEntities>,
    Option<&'a mut VisibleMeshEntities>,
    Option<&'a PointLight>,
    Option<&'a SpotLight>,
);

type Shapes<'a> = (&'a GlobalTransform, &'a Aabb, Has<LightFixture>, Option<&'a nvf_play::RefVisual>, Has<ShadowDynamic>, Has<SkinnedMesh>);

/// Take the light fixtures out of the shapes Bevy found for each casting light's maps this
/// frame: every shape marked [`LightFixture`], and a small shape the light sits in or
/// against ([`FIXTURE_MARGIN`], [`FIXTURE_RADIUS`]). Runs after Bevy's culling of the
/// lights' maps. Only the lights drawing this frame (maps switched on) are filtered: the
/// others' lists are last draw's, which no one reads, and touching them marked them changed,
/// so Bevy extracted every casting light again every frame. Also takes out the casters
/// under the light's small-caster cut-off ([`cut_off`]; `NVF_LIGHT_SHADOW_CUTOFF=0`: none).
fn skip_fixtures(
    mut lights: Query<CasterLists, With<CastingLight>>,
    shapes: Query<Shapes>,
    mut state: ResMut<LampShadowState>,
    mut logged: Local<bevy::ecs::entity::EntityHashSet>,
) {
    let _timed = Timed::start(1);
    let log = log_fixtures();
    let state = &mut *state;
    for (light, t, cube, spot, point_light, spot_light) in &mut lights {
        if !point_light.is_some_and(|p| p.shadow_maps_enabled) && !spot_light.is_some_and(|s| s.shadow_maps_enabled) {
            continue;
        }
        // The light's step and map scale, for the cut-off (none for a light not in the
        // game's states).
        let cut = state.lamps.get(&light).filter(|_| cutoff_on()).map(|s| (s.step, s.map_scale));
        // The static shadow cache: the faces drawn into it this frame (static casters only)
        // and those composited from it (dynamic casters only); the faces whose lists hold
        // dynamic casters.
        let (to_static, composite) = state.lamps.get(&light).map_or((0, 0), |s| (s.to_static, s.composite));
        let mut dynamic_in = 0u8;
        let mut cuts = 0u64;
        let at = t.translation();
        let mut lists: Vec<&mut Vec<Entity>> = Vec::with_capacity(6);
        if let Some(cube) = cube {
            lists.extend(cube.into_inner().iter_mut().map(|v| &mut v.entities));
        }
        if let Some(spot) = spot {
            lists.push(&mut spot.into_inner().entities);
        }
        // For the test aid: the shapes skipped, and the small ones kept near the light.
        let (mut skipped, mut kept): (Vec<Entity>, Vec<Entity>) = (Vec::new(), Vec::new());
        // Bit `face` for the cube's faces in order (a spot light's one list: bit 0).
        for (face, list) in lists.into_iter().enumerate() {
            let bit = 1u8 << face.min(7);
            let (into_cache, over_cache) = (to_static & bit != 0, composite & bit != 0);
            list.retain(|e| {
                let Ok((st, aabb, marked, _, dynamic, skinned)) = shapes.get(*e) else { return true };
                let gap = if marked { None } else { fixture_gap(at, &st.affine(), aabb) };
                let skip = marked || gap.is_some_and(|g| g <= FIXTURE_MARGIN);
                if log {
                    if skip && !skipped.contains(e) {
                        skipped.push(*e);
                    } else if !skip && gap.is_some_and(|g| g <= 4.0 * FIXTURE_MARGIN) && !kept.contains(e) {
                        kept.push(*e);
                    }
                }
                if skip {
                    return false;
                }
                let small = cut.is_some_and(|(step, scale)| {
                    let radius = Vec3::from(aabb.half_extents).length() * max_scale(st);
                    cut_off(radius, st.transform_point(Vec3::from(aabb.center)).distance(at), scale, step)
                });
                cuts += u64::from(small);
                if small {
                    return false;
                }
                let dynamic = dynamic || skinned;
                if dynamic {
                    dynamic_in |= bit;
                }
                keeps_caster(into_cache, over_cache, dynamic)
            });
        }
        if cuts > 0 {
            state.pass.cut_casters += cuts;
            if let Some(s) = state.lamps.get_mut(&light) {
                s.cut_any = true;
            }
        }
        // Faces drawn into the cache that held dynamic casters: composited again, first.
        let back = put_back(to_static, dynamic_in);
        if back != 0 {
            state.pass.put_back += u64::from(back.count_ones());
            if let Some(s) = state.lamps.get_mut(&light) {
                (s.dirty, s.pending) = (s.dirty | back, s.pending | back);
            }
        }
        if log && (!skipped.is_empty() || !kept.is_empty()) && logged.insert(light) {
            // Each by its reference, whether it is marked, its size and its gap to the light.
            let shown = |v: &[Entity]| {
                let all: Vec<String> = v
                    .iter()
                    .filter_map(|e| shapes.get(*e).ok())
                    .map(|(st, aabb, marked, r, ..)| {
                        let size = Vec3::from(aabb.half_extents) * 2.0 * st.compute_transform().scale.abs();
                        let gap = fixture_gap(at, &st.affine(), aabb).map_or("large".to_string(), |g| format!("{g:.0}"));
                        format!("{:08X}{} {:.0}x{:.0}x{:.0} gap {gap}", r.map_or(0, |r| r.0), if marked { " fixture" } else { "" }, size.x, size.y, size.z)
                    })
                    .collect();
                if all.is_empty() { "none".to_string() } else { all.join(", ") }
            };
            info!("light shadows: light {light} at {:.0} {:.0} {:.0} skips {}; small shapes near it kept: {}", at.x, -at.z, at.y, shown(&skipped), shown(&kept));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_systems_run_with_the_row_on_and_for_the_wind_down_after_it() {
        assert!(runs(1, WIND_DOWN_FRAMES));
        assert!(runs(2, WIND_DOWN_FRAMES));
        assert!(runs(0, 0));
        assert!(runs(0, WIND_DOWN_FRAMES - 1));
        assert!(!runs(0, WIND_DOWN_FRAMES));
    }

    /// The row Off from the start, switched on, switched off again: the systems stop after
    /// the wind-down, and the renderer's hold is active only while the row is on.
    #[test]
    fn with_the_row_off_nothing_runs_and_the_hold_is_inactive() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PointLightShadowMap>()
            .add_plugins(LightShadowsPlugin(LightShadowMode::Off));
        let side = app.world().resource::<PointLightShadowMap>().size;
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(app.world().resource::<LightShadowsIdle>().frames, WIND_DOWN_FRAMES);
        assert!(!app.world().resource::<HoldLightShadowMaps>().active);

        app.world_mut().insert_resource(LightShadows::of(LightShadowMode::Quality));
        app.update();
        assert_eq!(app.world().resource::<LightShadowsIdle>().frames, 0);
        assert!(app.world().resource::<HoldLightShadowMaps>().active);

        app.world_mut().insert_resource(LightShadows::of(LightShadowMode::Off));
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(app.world().resource::<LightShadowsIdle>().frames, WIND_DOWN_FRAMES);
        assert!(!app.world().resource::<HoldLightShadowMaps>().active);
        // Bevy's cube side given back.
        assert_eq!(app.world().resource::<PointLightShadowMap>().size, side);
    }

    #[test]
    fn the_choices_are_the_sun_shadows_rows_and_each_on_choice_is_tuned() {
        assert_eq!(CHOICES, crate::shadow_rate::SUN_SHADOW_CHOICES);
        assert_eq!(tuning(0), None);
        let (p, q) = (tuning(1).unwrap(), tuning(2).unwrap());
        assert!(q.lights > p.lights && q.draws > p.draws, "Quality draws more");
        assert!(q.near_every <= p.near_every && q.far_every <= p.far_every, "and as often or more");
        assert!(q.transients > p.transients && q.transient_draws >= p.transient_draws, "and more transients");
        assert!(p.transients >= 1 && p.transient_draws >= 1, "Performance casts a muzzle flash too");
        assert_eq!(LightShadows::of(LightShadowMode::Quality).choice, 2);
        assert_eq!(LightShadows::of(LightShadowMode::Off).choice, 0);
    }

    #[test]
    fn lights_are_picked_by_how_near_their_light_reaches_within_the_shadow_distance() {
        // Inside a light (edge distance 0) before one whose light ends 100 units away.
        let mut c = vec![(1, rank(500.0, 400.0, false)), (2, rank(300.0, 400.0, false)), (3, rank(5000.0, 400.0, false))];
        assert_eq!(pick(&mut c, 8000.0, 8), vec![2, 1, 3]);
        assert_eq!(pick(&mut c, 1000.0, 8), vec![2, 1], "beyond the shadow distance: none");
        assert_eq!(pick(&mut c, 8000.0, 1), vec![2], "the cap");
        // A casting light keeps its place against one KEEP times nearer.
        let mut c = vec![(1, rank(1200.0, 400.0, true)), (2, rank(1050.0, 400.0, false))];
        assert_eq!(pick(&mut c, 8000.0, 1), vec![1]);
        let mut c = vec![(1, rank(1200.0, 400.0, true)), (2, rank(700.0, 400.0, false))];
        assert_eq!(pick(&mut c, 8000.0, 1), vec![2]);
    }

    #[test]
    fn transients_are_muzzle_flashes_and_explosions_picked_near_by_their_own_cap() {
        assert!(SourceKind::MuzzleFlash.transient() && SourceKind::Explosion.transient());
        assert!(!SourceKind::GrassFire.transient() && !SourceKind::Attached.transient(), "ranked with the placed lights");
        // The reach: TRANSIENT_REACH, or the sun's shadow distance when shorter.
        assert_eq!(transient_reach(8000.0), TRANSIENT_REACH);
        assert_eq!(transient_reach(2000.0), 2000.0);
        let reach = transient_reach(8000.0);
        // Three flashes and an explosion (range 300 and 800): nearest edge first, the cap,
        // and none whose light ends beyond the reach.
        let mut c = vec![
            (1, rank(2000.0, 300.0, false)),
            (2, rank(600.0, 300.0, false)),
            (3, rank(3500.0, 800.0, false)),
            (4, rank(3400.0, 300.0, false)),
        ];
        assert_eq!(pick(&mut c, reach, 8), vec![2, 1, 3], "the explosion's light reaches 2,700 units: in; the far flash's 3,100: out");
        assert_eq!(pick(&mut c, reach, 2), vec![2, 1], "Quality");
        assert_eq!(pick(&mut c, reach, 1), vec![2], "Performance");
        assert_eq!(pick(&mut c, transient_reach(1500.0), 8), vec![2], "the sun's shadow distance shorter");
    }

    #[test]
    fn a_transient_casts_when_its_light_reaches_into_the_view() {
        // A camera at the origin looking down -z, 90 degrees wide, near plane 10.
        use bevy::camera::CameraProjection;
        let lens = bevy::camera::PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: 1.0, near: 10.0, ..default() };
        let clip = lens.get_clip_from_view();
        let view = Frustum(bevy::shape::ViewFrustum::from_clip_from_world(&clip));
        assert!(in_view(Some(&view), Vec3::new(0.0, 0.0, -1000.0), 100.0), "ahead");
        assert!(!in_view(Some(&view), Vec3::new(0.0, 0.0, 1000.0), 100.0), "behind");
        assert!(in_view(Some(&view), Vec3::new(0.0, 0.0, 1000.0), 1200.0), "behind, its light reaching past the camera");
        assert!(!in_view(Some(&view), Vec3::new(2000.0, 0.0, -100.0), 300.0), "off to the side");
        assert!(in_view(Some(&view), Vec3::new(0.0, 0.0, -1.0e6), 10.0), "no far plane");
        assert!(in_view(None, Vec3::new(0.0, 0.0, 1000.0), 1.0), "no frustum: the distance alone");
    }

    #[test]
    fn a_light_marks_its_maps_in_full_only_once_it_moved_past_the_threshold() {
        let drawn = Vec3::new(100.0, 50.0, 0.0);
        assert!(!light_moved(drawn, drawn));
        assert!(!light_moved(drawn, drawn + Vec3::X * (LIGHT_MOVE_EPS - 0.5)), "a gun's sway");
        assert!(!light_moved(drawn, drawn + Vec3::new(5.0, 5.0, 0.0)), "7.1 units, diagonal");
        assert!(light_moved(drawn, drawn + Vec3::new(6.0, 6.0, 0.0)), "8.5 units");
        assert!(light_moved(drawn, drawn - Vec3::Y * 40.0), "a running shooter's tenth of a second");
        // Small moves add up: measured from where it was drawn, not from last frame.
        let path: Vec<Vec3> = (1..=5).map(|i| drawn + Vec3::X * 3.0 * i as f32).collect();
        let first = path.iter().position(|p| light_moved(drawn, *p));
        assert_eq!(first, Some(2), "9 units after three steps of 3");
    }

    #[test]
    fn the_renderer_summary_gives_views_events_and_times_a_frame() {
        let mut c = bevy::pbr::NvfLightViewCounts { frames: 4, views: 30, run: 10, hidden: 6, ..default() };
        c.ns[1] = 8_000_000; // prepare_lights: 2 ms a frame
        c.max_ns[1] = 5_000_000;
        c.events[0] = 12;
        let s = renderer_summary(&c);
        assert!(s.starts_with("light shadows: renderer over 4 frames: light views a frame 7.50 alive, 2.50 run, 6 despawned with their light hidden;"), "{s}");
        assert!(s.contains("views spawned 12, views despawned 0"), "{s}");
        assert!(s.contains("extract_lights 0.000/0.00, prepare_lights 2.000/5.00, uniform writer"), "{s}");
        // The smoke scenarios read the static cache's three events together; the kept phases'
        // follow them.
        assert!(s.contains("static cache draws 0, static cache composites 0, static cache misses 0, phases reused 0, kept phases wiped 0, views kept 0, kept phases resynced 0, entities rebinned 0;"), "{s}");
        // No frames counted (nothing rendered since): no division by zero.
        assert!(renderer_summary(&default()).contains("over 0 frames: light views a frame 0.00 alive"));
        // The game's own systems, mean over the game frames and the longest.
        let s = systems_summary(&[3_000_000, 0, 0, 0], &[2_500_000, 0, 0, 0], 3);
        assert_eq!(s, "game systems ms a frame mean/max: shadow_lights 1.000/2.50, skip_fixtures 0.000/0.00, soften 0.000/0.00, contact 0.000/0.00");
    }

    #[test]
    fn slots_are_the_lowest_free_and_cubes_follow_the_shadow_resolution() {
        assert_eq!(free_slot([].into_iter()), 0);
        assert_eq!(free_slot([0, 1, 3].into_iter()), 2);
        assert_eq!(free_slot([3, 1, 0, 2].into_iter()), 4);
        assert_eq!(free_slot([1, 2].into_iter()), 0);
        assert_eq!(cube_side(2048), 512);
        assert_eq!(cube_side(4096), 1024);
    }

    #[test]
    fn a_shadow_fades_in_and_out_over_the_fade_time() {
        let half = FADE_SECS * 0.5;
        assert!((fade_step(0.0, true, half) - 0.5).abs() < 1e-5);
        assert_eq!(fade_step(0.9, true, half), 1.0, "no more than full");
        assert!((fade_step(1.0, false, half) - 0.5).abs() < 1e-5);
        assert_eq!(fade_step(0.2, false, half), 0.0, "no less than none");
        // At 60 frames a second it takes the fade time to come in full.
        let mut fade = 0.0;
        let frames = (0..100).position(|_| {
            fade = fade_step(fade, true, 1.0 / 60.0);
            fade >= 1.0
        });
        let expected = (FADE_SECS * 60.0).round() as usize;
        assert!(frames.is_some_and(|f| (f + 1).abs_diff(expected) <= 1), "{frames:?} frames, {expected} expected (a frame either way: rounding)");
    }

    #[test]
    fn a_light_no_longer_chosen_is_kept_while_its_shadow_fades_out() {
        let dt = 1.0 / 60.0;
        // A drawn lamp, alive: kept, fading, until its fade reaches 0.
        let mut fade = 1.0;
        let mut frames = 0usize;
        while let Some(f) = leaving_fade(fade, true, false, true, dt) {
            assert!(f < fade, "it falls");
            fade = f;
            frames += 1;
        }
        let expected = (FADE_SECS * 60.0).round() as usize;
        assert!((frames + 1).abs_diff(expected) <= 1, "dropped once at 0: {frames} frames kept, {expected} expected");
        // Half faded in when it left: half the time.
        let mut fade = 0.5;
        let mut half = 0;
        while let Some(f) = leaving_fade(fade, true, false, true, dt) {
            fade = f;
            half += 1;
        }
        assert!(half <= frames / 2 + 1, "{half} frames from half, {frames} from full");
        // Dropped at once: despawned, never drawn, a transient.
        assert_eq!(leaving_fade(1.0, true, false, false, dt), None, "despawned");
        assert_eq!(leaving_fade(0.0, false, false, true, dt), None, "never drew");
        assert_eq!(leaving_fade(1.0, true, true, true, dt), None, "a muzzle flash");
    }

    #[test]
    fn spot_lights_keep_stable_slots_and_their_room_grows_in_fours() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let mut slots: EntityHashMap<u32> = EntityHashMap::default();
        assert_eq!(assign_slot(&mut slots, e(10)), 0);
        assert_eq!(assign_slot(&mut slots, e(11)), 1);
        assert_eq!(assign_slot(&mut slots, e(12)), 2);
        assert_eq!(assign_slot(&mut slots, e(10)), 0, "kept");
        // One leaves: the others keep theirs, a new one takes the freed slot.
        slots.remove(&e(10));
        assert_eq!(assign_slot(&mut slots, e(13)), 0);
        assert_eq!((slots[&e(11)], slots[&e(12)]), (1, 2));
        // The room: in fours, never shrinking, with the other shadowed spot lights.
        assert_eq!(grown_room(0, std::iter::empty(), 0), 0);
        assert_eq!(grown_room(0, [0, 1, 2].into_iter(), 0), 4);
        assert_eq!(grown_room(0, [0, 1, 2].into_iter(), 1), 4, "and the flashlight");
        assert_eq!(grown_room(0, [0, 1, 2].into_iter(), 2), 8);
        assert_eq!(grown_room(8, [0].into_iter(), 0), 8, "never shrinks");
        assert_eq!(grown_room(4, [4].into_iter(), 0), 8);
    }

    #[test]
    fn turns_are_staggered_by_phase() {
        let every = 4;
        let frames: Vec<u64> = (0..12).filter(|&f| its_turn(f, 1, every)).collect();
        assert_eq!(frames, vec![3, 7, 11]);
        assert!((0..8).all(|f| its_turn(f, 5, 1)), "every frame");
        let a: Vec<bool> = (0..4).map(|f| its_turn(f, 0, 2)).collect();
        let b: Vec<bool> = (0..4).map(|f| its_turn(f, 1, 2)).collect();
        assert_ne!(a, b, "two lights on different frames");
    }

    #[test]
    fn a_sphere_dirties_the_faces_it_reaches() {
        let light = Vec3::ZERO;
        let sphere = |c: Vec3, r: f32| Sphere { center: c.into(), radius: r };
        assert!(reaches(light, 400.0, &sphere(Vec3::new(450.0, 0.0, 0.0), 60.0)));
        assert!(!reaches(light, 400.0, &sphere(Vec3::new(500.0, 0.0, 0.0), 60.0)));
        // Without frusta (a light that never drew) every face counts.
        assert_eq!(faces_reached(&sphere(Vec3::X * 100.0, 10.0), None, None, 0), ALL_FACES);
        assert_eq!(faces_reached(&sphere(Vec3::X * 100.0, 10.0), None, None, 0b11), ALL_FACES & !0b11, "faces skipped are not tested");
    }

    #[test]
    fn the_lamp_grid_finds_every_lamp_a_small_sphere_reaches() {
        let lamps = [(Vec3::ZERO, 400.0), (Vec3::new(5000.0, 0.0, 0.0), 800.0), (Vec3::new(-3000.0, 200.0, 2500.0), 300.0), (Vec3::new(900.0, 0.0, 0.0), 600.0)];
        let grid = LampGrid::new(&lamps);
        // Against the plain test over a lattice of spheres around them: never one missed.
        let mut checked = 0;
        for x in (-4000..=6500).step_by(250) {
            for z in (-1000..=3500).step_by(250) {
                for r in [1.0, 60.0, GRID_MARGIN] {
                    let c = Vec3::new(x as f32, 100.0, z as f32);
                    let s = Sphere { center: c.into(), radius: r };
                    let cands = grid.candidates(c, r);
                    for (i, (at, range)) in lamps.iter().enumerate() {
                        if reaches(*at, *range, &s) {
                            assert!(cands.contains(&(i as u16)), "lamp {i} missed at {c} r {r}");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 100, "{checked} reaches checked");
        // Far from every lamp: none; a sphere larger than the margin: all.
        assert!(grid.candidates(Vec3::new(20000.0, 0.0, 0.0), 10.0).is_empty());
        assert_eq!(grid.candidates(Vec3::ZERO, GRID_MARGIN + 1.0).len(), lamps.len());
        assert!(LampGrid::new(&[]).candidates(Vec3::ZERO, 1.0).is_empty(), "no lamps");
    }

    #[test]
    fn a_shape_marks_the_faces_it_reaches_and_a_lamp_drawing_in_full_is_left_alone() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let lamps = vec![test_lamp(1, Vec3::ZERO, 0), test_lamp(2, Vec3::new(2000.0, 0.0, 0.0), 0)];
        let frusta = vec![(None, None), (None, None)];
        let mut states: EntityHashMap<LampState> = EntityHashMap::default();
        states.insert(e(1), test_state(false, 0b1));
        states.insert(e(2), test_state(false, 0));
        let mark = |s: &Sphere, c: &[usize], states: &mut EntityHashMap<LampState>| mark_sphere(s, None, false, false, c.iter().copied(), &lamps, &frusta, states);
        let near = Sphere { center: Vec3::new(300.0, 0.0, 0.0).into(), radius: 20.0 };
        assert_eq!(mark(&near, &[0, 1], &mut states), Marked { reached: true, marked: true, cut: 0 });
        assert_eq!(states[&e(1)].dirty, ALL_FACES, "without frusta every face");
        assert_eq!(states[&e(2)].dirty, 0, "the far lamp is not reached");
        let far = Sphere { center: Vec3::new(9000.0, 0.0, 0.0).into(), radius: 20.0 };
        assert_eq!(mark(&far, &[0, 1], &mut states), Marked { reached: false, marked: false, cut: 0 });
        // A fresh lamp draws in full anyway: reached, nothing added.
        states.insert(e(2), test_state(true, 0));
        let by_two = Sphere { center: Vec3::new(2100.0, 0.0, 0.0).into(), radius: 20.0 };
        assert_eq!(mark(&by_two, &[1], &mut states), Marked { reached: true, marked: true, cut: 0 });
        assert_eq!(states[&e(2)].dirty, 0);
    }

    fn test_lamp(n: u32, at: Vec3, step: u8) -> Lamp {
        Lamp { entity: Entity::from_raw_u32(n).unwrap(), at, range: 500.0, spot: false, seen: true, gap: 0.0, transient: false, step, map_scale: map_scale(512.0, 1.0) }
    }

    fn test_state(fresh: bool, dirty: u8) -> LampState {
        LampState {
            dirty,
            fresh,
            refreshed: 0,
            phase: 0,
            at: Vec3::ZERO,
            transient: false,
            spot: false,
            fade: 1.0,
            leaving: false,
            step: 0,
            map_scale: map_scale(512.0, 1.0),
            cut_any: false,
            far_fade: 1.0,
            deferred: 0,
            static_dirty: 0,
            pending: 0,
            plain: false,
            to_static: 0,
            composite: 0,
        }
    }

    #[test]
    fn a_face_the_camera_cannot_see_is_held_and_the_others_drawn() {
        // A camera at the origin looking down -z, 90 degrees wide, near plane 10.
        use bevy::camera::CameraProjection;
        let lens = bevy::camera::PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: 1.0, near: 10.0, ..default() };
        let view = Frustum(bevy::shape::ViewFrustum::from_clip_from_world(&lens.get_clip_from_view()));
        // A lamp ahead: every face can show.
        assert_eq!(faces_in_view(&view, Vec3::new(0.0, 0.0, -1000.0), 400.0, 0.0), ALL_FACES);
        // A lamp 500 units behind whose light reaches past the camera: the face pointing
        // further back (+z, index 5) and the side faces lie behind the side planes; the face
        // towards the camera (-z, index 4) shows.
        let behind = faces_in_view(&view, Vec3::new(0.0, 0.0, 500.0), 800.0, 0.0);
        assert_eq!(behind & (1 << 5), 0, "+z, away from the view");
        assert_ne!(behind & (1 << 4), 0, "-z, into the view");
        assert_eq!(behind & 0b1111, 0, "the sides: {behind:06b}");
        // A margin keeps a face just out of view.
        let side = Vec3::new(1100.0, 0.0, -1000.0);
        let near_edge = faces_in_view(&view, side, 400.0, 0.0);
        assert_eq!(near_edge & 1, 0, "+x, out past the right plane");
        assert_ne!(faces_in_view(&view, side, 400.0, 1000.0) & 1, 0, "within a wide margin");
        // Each face's own pyramid: the light's point and four far corners.
        for (i, f) in CUBE_MAP_FACES.iter().enumerate() {
            assert!(f.target.length() > 0.99, "face {i}");
        }
    }

    #[test]
    fn a_shadow_fades_out_over_the_last_stretch_of_the_reach() {
        let reach = 8000.0;
        assert_eq!(distance_fade(0.0, reach), 1.0);
        assert_eq!(distance_fade(reach * (1.0 - DISTANCE_FADE), reach), 1.0, "full up to the stretch");
        assert!((distance_fade(reach * (1.0 - DISTANCE_FADE / 2.0), reach) - 0.5).abs() < 1e-5, "half way through it");
        assert_eq!(distance_fade(reach, reach), 0.0, "none at the reach");
        assert_eq!(distance_fade(reach + 100.0, reach), 0.0);
        assert_eq!(distance_fade(100.0, 0.0), 1.0, "no reach: no fade");
        // Falls steadily as the camera goes away.
        let fades: Vec<f32> = (0..=20).map(|i| distance_fade(reach * 0.75 + i as f32 * 100.0, reach)).collect();
        assert!(fades.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn a_light_steps_down_in_detail_with_distance_and_hysteresis() {
        assert_eq!(stepped(0, 0.0, 0.0), 0);
        assert_eq!(stepped(0, NEAR + 1.0, 0.0), 1);
        assert_eq!(stepped(0, 2.0 * NEAR + 1.0, 0.0), 2);
        // Within the hysteresis band it keeps its step, either way.
        let h = STEP_HYSTERESIS;
        assert_eq!(stepped(0, NEAR * (1.0 + h * 0.5), h), 0, "a little past the step's distance: still near");
        assert_eq!(stepped(0, NEAR * (1.0 + h * 1.5), h), 1);
        assert_eq!(stepped(1, NEAR * (1.0 - h * 0.5), h), 1, "a little within: still mid");
        assert_eq!(stepped(1, NEAR * (1.0 - h * 1.5), h), 0);
        assert_eq!(stepped(2, 0.0, h), 0, "came straight in");
        assert_eq!(stepped(0, 1.0e6, h), 2, "jumped far");
        // Walking back and forth over the distance by less than the band: one step change.
        let mut step = 0;
        let mut changes = 0;
        for i in 0..40 {
            let gap = NEAR * (1.0 + if i % 2 == 0 { 0.08 } else { -0.08 }) + if i == 0 { 400.0 } else { 0.0 };
            let s = stepped(step, gap, h);
            changes += usize::from(s != step);
            step = s;
        }
        assert_eq!(changes, 1, "only the first, far-out sample stepped it");
    }

    #[test]
    fn small_casters_are_cut_by_their_texels_in_the_map_more_for_far_lights() {
        let cube = map_scale(512.0, 1.0);
        // A bottle (radius 5) 300 units from the lamp covers 8.5 texels of a 512 face: kept.
        assert!(!cut_off(5.0, 300.0, cube, 0) && !cut_off(5.0, 300.0, cube, 2));
        // A coin (radius 0.5) at 400 units: 0.64 texels, cut at every step.
        assert!(cut_off(0.5, 400.0, cube, 0));
        // A can (radius 3) at 600 units: 2.56 texels: kept near, cut far.
        assert!(!cut_off(3.0, 600.0, cube, 0) && !cut_off(3.0, 600.0, cube, 1) && cut_off(3.0, 600.0, cube, 2));
        // A larger cube map (shadow resolution 4096) shows more.
        assert!(!cut_off(3.0, 600.0, map_scale(1024.0, 1.0), 2));
        // A spot light's 2048 map over a 45-degree half cone (tan 1) is a 2048 face.
        assert_eq!(map_scale(2048.0, 1.0), 2048.0);
        assert!(map_scale(2048.0, 0.5) > map_scale(2048.0, 1.0), "a narrower cone: more texels a unit");
        // A caster at the light itself is never cut.
        assert!(!cut_off(0.5, 0.0, cube, 2));
    }

    #[test]
    fn a_caster_under_the_cut_off_marks_nothing_unless_it_was_over_it() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let lamps = vec![test_lamp(1, Vec3::ZERO, 2)];
        let frusta = vec![(None, None)];
        let mut states: EntityHashMap<LampState> = EntityHashMap::default();
        states.insert(e(1), test_state(false, 0));
        let tiny = |x: f32| Sphere { center: Vec3::new(x, 0.0, 0.0).into(), radius: 1.0 };
        // 1 unit at 400: 1.3 texels, under step 2's 4: nothing marked, one cut counted.
        let m = mark_sphere(&tiny(400.0), Some(&tiny(390.0)), true, false, 0..1, &lamps, &frusta, &mut states);
        assert_eq!(m, Marked { reached: true, marked: false, cut: 1 });
        assert_eq!(states[&e(1)].dirty, 0);
        // Shown, hidden or new (no old place): the same.
        assert_eq!(mark_sphere(&tiny(400.0), None, true, false, 0..1, &lamps, &frusta, &mut states).cut, 1);
        // Moved away from where it was over the cut-off (50 units: 10 texels): it was in the
        // map, so it marks.
        let m = mark_sphere(&tiny(400.0), Some(&tiny(50.0)), true, false, 0..1, &lamps, &frusta, &mut states);
        assert_eq!((m.marked, m.cut), (true, 0));
        assert_eq!(states[&e(1)].dirty, ALL_FACES);
        // The aid off: it marks.
        states.insert(e(1), test_state(false, 0));
        assert_eq!(mark_sphere(&tiny(400.0), None, false, false, 0..1, &lamps, &frusta, &mut states).cut, 0);
        assert_eq!(states[&e(1)].dirty, ALL_FACES);
    }

    #[test]
    fn the_performance_pass_line_gives_each_technique_a_frame() {
        let p = Pass {
            cut_casters: 300,
            cut_marks: 20,
            step_redraws: 3,
            steps: [100, 50, 50],
            grid: [50, 25_000, 9000, 40],
            distance_faded: 250,
            faces_out_of_view: 30,
            static_faces: 12,
            composited: 40,
            put_back: 5,
            promoted: 2,
            predrawn: 7,
        };
        let s = pass_summary(&p, 100);
        assert!(s.starts_with("light shadows: performance pass over 100 frames: lights at steps near/mid/far 1.0/0.5/0.5 a frame;"), "{s}");
        assert!(s.contains("casters cut 3.0 a frame, marks skipped 0.20 a frame, redraws for a finer step 3"), "{s}");
        assert!(s.contains("0.50 lamps served a frame, 500 candidates each, 9000 meshes filed (40 for every light)"), "{s}");
        assert!(s.contains("distance fade (on) 2.5 lights fading a frame"), "{s}");
        assert!(s.contains("face culling (on) 0.30 marked faces held out of view a frame;"), "{s}");
        assert!(
            s.contains("static cache (on) 0.12 static redraws (faces) a frame, 0.40 dynamic recomposites a frame, 0.52 layer copies a frame, 5 faces put back after a static redraw, 2 shapes became dynamic;"),
            "{s}"
        );
        assert!(s.ends_with("loading-screen pre-draw (on) 7 lights"), "{s}");
        assert!(pass_summary(&Pass::default(), 0).contains("over 0 frames"), "no division by zero");
    }

    #[test]
    fn a_static_shape_that_moves_becomes_dynamic_and_a_dynamic_one_stays_so() {
        // Shown, hidden or new: the static caches where it is.
        assert_eq!(shape_change(false, true, true), ShapeChange::Static, "shown or hidden (Primm's mass enable)");
        assert_eq!(shape_change(false, false, false), ShapeChange::Static, "new: a cell loading");
        // Moved after it marked the lamps: promoted (clutter pushed, a door turning).
        assert_eq!(shape_change(false, false, true), ShapeChange::Promoted);
        // An actor's part, a skinned shape, one promoted before: composites only.
        for (shown, known) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(shape_change(true, shown, known), ShapeChange::Dynamic);
        }
    }

    #[test]
    fn a_moved_caster_marks_where_it_was_and_where_it_is() {
        let s = |x: f32, r: f32| Sphere { center: Vec3::new(x, 0.0, 0.0).into(), radius: r };
        let holds = |m: &Sphere, a: &Sphere| Vec3::from(m.center).distance(Vec3::from(a.center)) + a.radius <= m.radius + 1e-3;
        for (a, b) in [(s(0.0, 10.0), s(30.0, 10.0)), (s(0.0, 40.0), s(5.0, 2.0)), (s(100.0, 3.0), s(-50.0, 25.0)), (s(7.0, 5.0), s(7.0, 5.0))] {
            let m = merged_sphere(&a, &b);
            assert!(holds(&m, &a) && holds(&m, &b), "{m:?} from {a:?} and {b:?}");
            assert!(m.radius <= (Vec3::from(a.center).distance(Vec3::from(b.center)) + a.radius + b.radius) * 0.5 + 1e-3 || m.radius == a.radius.max(b.radius), "no larger than needed");
        }
        assert_eq!(merged_sphere(&s(0.0, 40.0), &s(5.0, 2.0)).radius, 40.0, "one inside the other: the outer");
        let m = merged_sphere(&s(0.0, 10.0), &s(30.0, 10.0));
        assert!((m.radius - 25.0).abs() < 1e-4 && (Vec3::from(m.center).x - 15.0).abs() < 1e-4, "{m:?}");
    }

    #[test]
    fn faces_with_an_old_cache_are_drawn_into_it_the_rest_composited() {
        // Faces 0-2 drawn this frame; 1 and 4 have an out-of-date cache.
        let (to_static, composite) = face_modes(0b0111, 0b1_0010, true);
        assert_eq!((to_static, composite), (0b0010, 0b0101));
        assert_eq!(face_modes(0b0111, 0b1_0010, false), (0, 0), "the plain path: neither");
        assert_eq!(face_modes(ALL_FACES, ALL_FACES, true), (ALL_FACES, 0), "a first draw: all into the cache");
        // Which casters each face's list keeps.
        assert!(keeps_caster(true, false, false) && !keeps_caster(true, false, true), "into the cache: the static ones");
        assert!(!keeps_caster(false, true, false) && keeps_caster(false, true, true), "over the cache: the dynamic ones");
        assert!(keeps_caster(false, false, false) && keeps_caster(false, false, true), "the plain path: all");
        // A face drawn into the cache whose list held an actor: composited again, first; a
        // face composited with no dynamic caster left is the cache copied back, done.
        assert_eq!(put_back(0b0011, 0b0110), 0b0010);
        assert_eq!(put_back(0, ALL_FACES), 0, "composites are not put back");
    }

    #[test]
    fn transients_spot_lights_moving_lights_and_the_aids_take_the_plain_path() {
        assert!(!plain_path(true, false, false, false, false), "a still point lamp is cached");
        assert!(plain_path(false, false, false, false, false), "NVF_LIGHT_SHADOW_STATIC_CACHE=0");
        assert!(plain_path(true, true, false, false, false), "NVF_PLAY_TEST_LIGHT_SHADOW_HOLD=0");
        assert!(plain_path(true, false, true, false, false), "a muzzle flash");
        assert!(plain_path(true, false, false, true, false), "a spot light");
        assert!(plain_path(true, false, false, false, true), "a light that moved");
    }

    #[test]
    fn a_static_mark_marks_the_cache_and_a_dynamic_one_the_composite_only() {
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let lamps = vec![test_lamp(1, Vec3::ZERO, 0)];
        let frusta = vec![(None, None)];
        let mut states: EntityHashMap<LampState> = EntityHashMap::default();
        states.insert(e(1), test_state(false, 0));
        let near = Sphere { center: Vec3::new(200.0, 0.0, 0.0).into(), radius: 20.0 };
        // An actor moved: the faces to composite, no cache.
        mark_sphere(&near, None, false, false, 0..1, &lamps, &frusta, &mut states);
        assert_eq!((states[&e(1)].dirty, states[&e(1)].static_dirty), (ALL_FACES, 0));
        // A static shape shown in the same faces: their caches too, though the faces were
        // marked already.
        mark_sphere(&near, None, false, true, 0..1, &lamps, &frusta, &mut states);
        assert_eq!((states[&e(1)].dirty, states[&e(1)].static_dirty), (ALL_FACES, ALL_FACES));
        // A spot light: its one map.
        let mut spot = test_lamp(2, Vec3::ZERO, 0);
        spot.spot = true;
        use bevy::camera::CameraProjection;
        let lens = bevy::camera::PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: 1.0, near: 8.0, ..default() };
        let cone = Frustum(bevy::shape::ViewFrustum::from_clip_from_world(&lens.get_clip_from_view()));
        states.insert(e(2), test_state(false, 0));
        mark_sphere(&Sphere { center: Vec3::new(0.0, 0.0, -100.0).into(), radius: 20.0 }, None, false, true, 0..1, &[spot], &[(None, Some(cone))], &mut states);
        assert_eq!((states[&e(2)].dirty, states[&e(2)].static_dirty), (1, 1));
    }

    #[test]
    fn under_a_loading_screen_the_lamps_draw_without_a_budget_once_the_cells_are_in() {
        assert!(predrawing(true, true, true, false));
        assert!(!predrawing(true, true, true, true), "cells still coming in: as in play");
        assert!(!predrawing(true, true, false, false), "no screen");
        assert!(!predrawing(false, true, true, false), "ALC shadows off");
        assert!(!predrawing(true, false, true, false), "NVF_LIGHT_SHADOW_PREDRAW=0");
        // The budget: one a light while it lasts; none taken without a limit.
        let mut budget = 2;
        assert!(takes_budget(&mut budget, false) && takes_budget(&mut budget, false));
        assert!(!takes_budget(&mut budget, false), "spent");
        assert_eq!(budget, 0);
        assert!((0..40).all(|_| takes_budget(&mut budget, true)), "the pre-draw: every light");
        assert_eq!(budget, 0, "and takes nothing");
    }

    #[test]
    fn the_screen_waits_for_the_pre_draw_until_nothing_is_left_or_the_cap() {
        assert!(predraw_busy(true, false, 0.5), "lamps drew this frame: their composites may follow");
        assert!(predraw_busy(false, true, 0.5), "lamps, seen or not, with marked faces");
        assert!(!predraw_busy(false, false, 0.5), "done");
        assert!(!predraw_busy(true, true, PREDRAW_CAP_SECS), "the cap");
        assert!(predraw_busy(true, true, PREDRAW_CAP_SECS - 0.01));
    }

    /// The kept shadow phases' re-bin path (vendored bevy_pbr): a lamp's kept phase is back
    /// after frames without a view, its light's visible list made anew (every visible entity
    /// "added"), some binned entities gone from it and a new one in it. Upstream's add panicked
    /// ("Binning main entity … when it was already binned", Novac at 22:00, 2026-11-19).
    #[test]
    fn a_kept_shadow_phase_back_in_view_is_resynced_and_never_bins_twice() {
        use bevy::pbr::{nvf_add_once, nvf_resync_kept_phase, Shadow, ShadowBatchSetKey, ShadowBinKey};
        use bevy::render::batching::gpu_preprocessing::GpuPreprocessingMode;
        use bevy::render::mesh::allocator::MeshSlabs;
        use bevy::render::render_phase::{BinnedRenderPhaseType, DrawFunctionId, InputUniformIndex, ViewBinnedRenderPhases};
        use bevy::render::render_resource::CachedRenderPipelineId;
        use bevy::render::sync_world::MainEntity;
        use bevy::render::view::{RenderVisibleEntitiesClass, RetainedViewEntity};

        let mut world = World::new();
        let [a, b, c, d] = std::array::from_fn(|_| MainEntity::from(world.spawn_empty().id()));
        let lamp = MainEntity::from(world.spawn_empty().id());
        let view = RetainedViewEntity::new(lamp, None, 0);
        let mut phases = ViewBinnedRenderPhases::<Shadow>::default();
        phases.prepare_for_new_frame(view, GpuPreprocessingMode::Culling);
        let phase = phases.get_mut(&view).unwrap();
        let key = || ShadowBatchSetKey {
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            material_bind_group_index: None,
            slabs: MeshSlabs::default(),
        };
        let bin = || ShadowBinKey { asset_id: AssetId::<Mesh>::default().untyped() };
        let add = |phase: &mut bevy::render::render_phase::BinnedRenderPhase<Shadow>, e: MainEntity, i: u32| {
            nvf_add_once(true, phase, key(), bin(), (Entity::PLACEHOLDER, e), InputUniformIndex(i), BinnedRenderPhaseType::MultidrawableMesh)
        };
        for (i, e) in [a, b, c].into_iter().enumerate() {
            assert!(!add(phase, e, i as u32), "binned once");
        }

        // Back in view: b and c still visible, a gone, d new; the list was made anew.
        let mut visible = RenderVisibleEntitiesClass::default();
        let mut now: Vec<(Entity, MainEntity)> = [b, c, d].iter().map(|e| (Entity::PLACEHOLDER, *e)).collect();
        now.sort_unstable_by_key(|(_, e)| *e);
        visible.update_cpu_culled_entities(&now);
        assert_eq!(visible.added_entities().len(), 3, "every visible entity reads as added");

        let mut to_queue = Vec::new();
        assert_eq!(nvf_resync_kept_phase(phase, &visible, &mut to_queue), 1, "a left its bin");
        assert_eq!(to_queue, vec![(Entity::PLACEHOLDER, d)], "only d is queued");
        assert!(!add(phase, d, 3));
        assert!(!phase.nvf_is_binned(a));
        assert!([b, c, d].iter().all(|e| phase.nvf_is_binned(*e)));

        // The guard: what the panic was (b queued while binned) is a re-bin.
        assert!(add(phase, b, 4), "b was binned already");
        assert!(phase.nvf_is_binned(b));
        assert_eq!(phase.nvf_retain_binned(|_| true), 0);
        assert_eq!(phase.nvf_retain_binned(|e| e != c), 1);
        assert!([b, d].iter().all(|e| phase.nvf_is_binned(*e)) && !phase.nvf_is_binned(c));
    }

    #[test]
    fn lamps_out_of_view_are_shown_nearest_first_once_those_in_view_are_done() {
        let mut unseen = vec![(1, 900.0), (2, 0.0), (3, 300.0), (4, 2500.0)];
        assert!(shown_unseen(&mut unseen, true, 8).is_empty(), "lamps in view still drawing go first");
        assert_eq!(shown_unseen(&mut unseen, false, 8), vec![2, 3, 1, 4]);
        assert_eq!(shown_unseen(&mut unseen, false, 2), vec![2, 3], "a few a frame");
        assert!(shown_unseen::<u32>(&mut [], false, 8).is_empty());
        // The cap spreads the most lamps a mode casts over a few frames, well within the
        // screen's wait (at 60 frames a second).
        let frames = tuning(2).unwrap().lights.div_ceil(PREDRAW_UNSEEN_PER_FRAME);
        assert!(frames <= 4 && (frames as f32 / 60.0) < PREDRAW_CAP_SECS);
    }

    #[test]
    fn the_cache_costs_as_much_again_as_the_cubes() {
        // 512 squared, 24 lamps: about 150 MB; 1024: four times that.
        assert!((cube_texture_mb(512, 24) - 150.99).abs() < 0.01);
        assert!((cube_texture_mb(1024, 32) - 805.31).abs() < 0.01);
        assert_eq!(cube_texture_mb(512, 0), 0.0);
    }

    #[test]
    fn the_largest_scale_is_read_off_the_axes() {
        let t = GlobalTransform::from(Transform::from_scale(Vec3::new(1.0, 3.0, 2.0)).with_rotation(Quat::from_rotation_y(0.7)));
        assert!((max_scale(&t) - 3.0).abs() < 1e-5);
        assert!((max_scale(&GlobalTransform::IDENTITY) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lamp_models_are_fixtures_by_their_file_name() {
        for m in [
            r"meshes\dungeons\office\lights\offrmlighthanging01.nif",
            r"meshes\architecture\megaton\interior\shackinteriors\shackhanginglight03.nif",
            r"meshes\clutter\lights\LampLightLamp.nif",
            r"meshes\architecture\strip\neoclassicalsconce.nif",
            "meshes/clutter/lantern01.nif",
        ] {
            assert!(is_fixture_model(m), "{m}");
        }
        for m in [r"meshes\dungeons\office\lights\..\offrmwall01.nif", r"meshes\clutter\office\rubberfloormatlrg01.nif", r"meshes\light\shelf01.nif"] {
            assert!(!is_fixture_model(m), "{m}: only the file name counts");
        }
    }

    #[test]
    fn a_small_shape_the_light_sits_in_or_against_is_its_fixture_a_wall_never() {
        use bevy::math::{Affine3A, Quat};
        let is_fixture = |light: Vec3, to_world: &Affine3A, aabb: &Aabb| fixture_gap(light, to_world, aabb).is_some_and(|g| g <= FIXTURE_MARGIN);
        // A hanging box lamp 100 x 40 x 20 units, its middle 30 units over the floor point.
        let lamp = Aabb::from_min_max(Vec3::new(-50.0, -20.0, -10.0), Vec3::new(50.0, 20.0, 10.0));
        let at = Affine3A::from_translation(Vec3::new(0.0, 0.0, 30.0));
        assert!(is_fixture(Vec3::new(0.0, 0.0, 30.0), &at, &lamp), "inside");
        assert!(is_fixture(Vec3::new(0.0, 0.0, 8.0), &at, &lamp), "12 units under it");
        assert!(!is_fixture(Vec3::new(0.0, 0.0, -10.0), &at, &lamp), "30 units under it");
        assert!(!is_fixture(Vec3::new(80.0, 0.0, 30.0), &at, &lamp), "30 units beside it");
        // Turned a quarter about z and scaled 2: its long side runs along y, 200 units.
        let turned = Affine3A::from_scale_rotation_translation(Vec3::splat(2.0), Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), Vec3::ZERO);
        let small = Aabb::from_min_max(Vec3::new(-25.0, -10.0, -5.0), Vec3::new(25.0, 10.0, 5.0));
        assert!(is_fixture(Vec3::new(0.0, 60.0, 0.0), &turned, &small), "along its long side");
        assert!(!is_fixture(Vec3::new(60.0, 0.0, 0.0), &turned, &small), "beyond its short side");
        // A wall piece 256 x 256 x 16 with the light 4 units off it: too large to be a fixture.
        let wall = Aabb::from_min_max(Vec3::new(-128.0, -8.0, -128.0), Vec3::new(128.0, 8.0, 128.0));
        assert_eq!(fixture_gap(Vec3::new(0.0, 12.0, 0.0), &Affine3A::IDENTITY, &wall), None);
        assert!(!is_fixture(Vec3::new(0.0, 12.0, 0.0), &Affine3A::IDENTITY, &wall));
    }
}
