//! The sun shadows' modes: the settings page's Sun shadows row, Off, Performance or Quality
//! ([`SUN_SHADOW_CHOICES`]; `--sun-shadows[=quality|performance]`). Performance trades some
//! of the shadows' precision for frame time. [engineering choices throughout; the original
//! casts no sun shadows from the world at all, docs/spec-notes/world.md §6]
//!
//! Performance:
//! - **The sun's shadow maps are drawn every other frame** ([`hold_cascades`]). On the
//!   frames between, the lights keep the cascades of the frame that drew them, so the maps
//!   and the matrices they are read with still belong together, and the renderer is told
//!   to leave the maps alone (`bevy::pbr::HoldDirectionalShadowMaps`, a patched bevy_pbr:
//!   engine/vendor/README.md). The shadow of a moving thing then moves at half the frame
//!   rate, and on a fast turn the edge of the view can lie outside a held cascade for one
//!   frame (unshadowed).
//! - **The sun's shadows are split over [`PERFORMANCE_CASCADES`] cascades** instead of
//!   `sky::DEFAULT_SHADOW_CASCADES` ([`set_performance`]): one shadow view less, and shadows
//!   near the camera a little coarser.
//! - **The first-person arms have no shade light of their own** (player_body.rs
//!   `shade_light`): another shadow view less; the arms take the sun's own shadows.
//! - **A tree casts a shadow only within [`TREE_SHADOW_NEAR`] of the camera**
//!   ([`far_trees_cast_no_shadow`]): a SpeedTree tree is thousands of leaf cards and a
//!   finely divided trunk in every cascade it falls in.
//!
//! Quality (the default) draws the sun's cascades on a staggered schedule
//! ([`STAGGERED_SCHEDULE`], #2063): the nearest every frame, the middle one every other
//! frame, the farthest every fourth, each held cascade kept with the matrices of the frame
//! that drew it, as above. A moving thing's shadow in the far cascades moves at a half or
//! a quarter of the frame rate, and on a fast turn the edge of the view can lie outside a
//! held cascade for up to three frames. Test aid `NVF_PLAY_TEST_SHADOW_STAGGER=0` draws
//! every cascade every frame.
//!
//! What it costs and saves: docs/design/frame-rate-audit.md §9 and §10.

use crate::fly::FlyCam;
use crate::sky::SkyState;
use bevy::ecs::entity::EntityHashMap;
use bevy::light::{Cascades, DirectionalLight, DirectionalLightShadowMap, NotShadowCaster, SimulationLightSystems};
use bevy::pbr::HoldDirectionalShadowMaps;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResourcePlugin;

/// `--sun-shadows=<mode>`: the mode the sun's shadows start in.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SunShadowMode {
    Performance,
    Quality,
}

/// Sun shadows in Performance mode (the Sun shadows row's middle choice).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PerformanceShadows(pub bool);

/// The Sun shadows row's choices, in the order the page and the INI keep them: an INI
/// written while the row was an on/off switch (`1`/`0`) reads back as Quality or Off.
pub const SUN_SHADOW_CHOICES: [&str; 3] = ["Off", "Performance", "Quality"];

/// The Sun shadows row's choice now (an index into [`SUN_SHADOW_CHOICES`]).
pub fn sun_shadow_choice(world: &World) -> usize {
    if !world.resource::<SkyState>().shadows {
        0
    } else if world.resource::<PerformanceShadows>().0 {
        1
    } else {
        2
    }
}

/// Set the Sun shadows row's choice: Off turns the sun's shadows off and leaves the mode
/// as it was; Performance and Quality set the mode, then turn the shadows on.
pub fn set_sun_shadow_choice(world: &mut World, choice: usize) {
    if choice > 0 {
        let performance = choice == 1;
        if world.resource::<PerformanceShadows>().0 != performance {
            set_performance(world, performance);
        }
    }
    if world.resource::<SkyState>().shadows != (choice > 0) {
        crate::sky::set_shadows(world, choice > 0);
    }
}

/// The sun's cascades in Performance mode.
pub const PERFORMANCE_CASCADES: usize = 2;

/// With the switch on a tree casts a shadow within this distance of the camera (game
/// units: about 43 m), and again once it is back within [`TREE_SHADOW_BACK`] of it.
pub const TREE_SHADOW_NEAR: f32 = 3000.0;
pub const TREE_SHADOW_BACK: f32 = 2700.0;

/// Seconds between two looks at the trees.
const TREES_EVERY: f32 = 0.5;

/// The sun's cascade count Performance replaced when it was chosen (`--graphics-cycle
/// cascades=` or the default), given back with Quality.
#[derive(Resource, Default)]
struct CascadesBeforePerformance(Option<usize>);

/// Performance mode on or off now (the Sun shadows row): the cascade count changes with it.
/// Only the count is set here; graphics.rs `sun_shadow_quality` puts the cascades on the
/// sun as the hour wants them (the moon's single cascade by night).
pub fn set_performance(world: &mut World, on: bool) {
    let was = std::mem::replace(&mut world.resource_mut::<PerformanceShadows>().0, on);
    let current = world.resource::<SkyState>().shadow_cascades;
    let mut before = world.get_resource_or_init::<CascadesBeforePerformance>();
    let cascades = if on {
        if !was {
            before.0 = Some(current);
        }
        PERFORMANCE_CASCADES
    } else if was {
        before.0.take().unwrap_or(crate::sky::DEFAULT_SHADOW_CASCADES)
    } else {
        current
    };
    world.resource_mut::<SkyState>().shadow_cascades = cascades;
    let how = if on {
        "performance (maps drawn every other frame, trees cast near the camera only, no arms shade light)"
    } else if staggered() {
        "cascades on a staggered schedule (the nearest every frame, the farther ones less often)"
    } else {
        "drawn every frame"
    };
    info!("sun shadows: {how}, {cascades} cascades");
}

/// A part of a SpeedTree tree (tree.rs `spawn`).
#[derive(Component)]
pub struct TreePart;

/// Marks the tree parts whose `NotShadowCaster` [`far_trees_cast_no_shadow`] put there.
#[derive(Component)]
pub struct FarTreeNoShadow;

/// Whether a tree part `distance` from the camera casts a shadow with the switch on,
/// given whether it casts now.
pub fn tree_casts(distance: f32, casting_now: bool) -> bool {
    distance <= if casting_now { TREE_SHADOW_NEAR } else { TREE_SHADOW_BACK }
}

/// With the switch on, trees far from the camera cast no shadow (module notes); with it
/// off every tree casts again.
fn far_trees_cast_no_shadow(
    mut commands: Commands,
    on: Res<PerformanceShadows>,
    time: Res<Time<Real>>,
    cams: Query<&GlobalTransform, With<FlyCam>>,
    trees: Query<(Entity, &GlobalTransform, Has<FarTreeNoShadow>), With<TreePart>>,
    ours: Query<Entity, With<FarTreeNoShadow>>,
    mut since: Local<f32>,
) {
    if !on.0 {
        for e in &ours {
            commands.entity(e).try_remove::<(NotShadowCaster, FarTreeNoShadow)>();
        }
        return;
    }
    *since += time.delta_secs();
    if *since < TREES_EVERY && !on.is_changed() {
        return;
    }
    *since = 0.0;
    let Some(cam) = cams.iter().next().map(|t| t.translation()) else { return };
    for (e, at, held) in &trees {
        let casts = tree_casts(at.translation().distance(cam), !held);
        if casts && held {
            commands.entity(e).try_remove::<(NotShadowCaster, FarTreeNoShadow)>();
        } else if !casts && !held {
            commands.entity(e).try_insert((NotShadowCaster, FarTreeNoShadow));
        }
    }
}

/// How often each cascade's map is drawn, nearest cascade first: `(n, phase)` draws it on
/// the frames where (frame + phase) is a multiple of n; a cascade past the list is drawn
/// every frame. With the switch on, both cascades every other frame, together.
pub const PERFORMANCE_SCHEDULE: [(u32, u32); 2] = [(2, 0), (2, 0)];
/// With the switch off: the nearest cascade every frame, the middle one every other frame
/// and the farthest every fourth, on different frames, so at most two are drawn in one
/// frame (1.75 of 3 on average). The farthest holds most of the casters (about 1,150 of
/// 1,400 at Goodsprings, 1,570 of 2,140 at Novac), and its shadows are the coarsest, so a
/// moving thing's shadow there moving at a quarter of the frame rate shows least.
pub const STAGGERED_SCHEDULE: [(u32, u32); 3] = [(1, 0), (2, 1), (4, 2)];

/// The cascades (a bit each, nearest first) whose maps frame `frame` keeps from an earlier
/// frame under `schedule`, of `cascades` cascades.
pub fn held_mask(schedule: &[(u32, u32)], frame: u64, cascades: usize) -> u32 {
    (0..cascades.min(32)).fold(0, |mask, i| {
        let (n, phase) = schedule.get(i).copied().unwrap_or((1, 0));
        if n > 1 && !(frame + u64::from(phase)).is_multiple_of(u64::from(n)) { mask | 1 << i } else { mask }
    })
}

/// Whether the cascades kept from earlier frames can stand in for this frame's: every
/// light has kept cascades for the same views, as many of them.
fn same_layout(kept: &EntityHashMap<Cascades>, now: &[(Entity, &Cascades)]) -> bool {
    !now.is_empty()
        && now.iter().all(|(light, cascades)| {
            kept.get(light).is_some_and(|k| {
                k.cascades.len() == cascades.cascades.len()
                    && cascades.cascades.iter().all(|(view, c)| k.cascades.get(view).is_some_and(|kc| kc.len() == c.len()))
            })
        })
}

/// Whether the cascades are drawn on a staggered schedule with the switch off (test aid
/// `NVF_PLAY_TEST_SHADOW_STAGGER=0` draws every cascade every frame).
fn staggered() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PLAY_TEST_SHADOW_STAGGER").map_or(true, |v| v != "0"))
}

/// The cascades not drawn this frame ([`PERFORMANCE_SCHEDULE`], [`STAGGERED_SCHEDULE`]): give
/// the directional lights back those cascades as the frame that drew them had them, and
/// have the renderer keep their maps (module notes). Runs after Bevy has built this
/// frame's cascades and before it fits the lights' frusta to them.
fn hold_cascades(
    on: Res<PerformanceShadows>,
    map: Option<Res<DirectionalLightShadowMap>>,
    mut hold: ResMut<HoldDirectionalShadowMaps>,
    mut lights: Query<(Entity, &mut Cascades), With<DirectionalLight>>,
    mut kept: Local<EntityHashMap<Cascades>>,
    mut frame: Local<u64>,
    mut said: Local<Option<bool>>,
) {
    *frame = frame.wrapping_add(1);
    let schedule: &[(u32, u32)] = if on.0 {
        &PERFORMANCE_SCHEDULE
    } else if staggered() {
        &STAGGERED_SCHEDULE
    } else {
        &[]
    };
    let count = lights.iter().flat_map(|(_, c)| c.cascades.values().map(Vec::len)).max().unwrap_or(0);
    // A new map size makes new maps: nothing to keep.
    let same_maps = !map.is_some_and(|m| m.is_changed());
    let mut mask = held_mask(schedule, *frame, count);
    if mask != 0 && !(same_maps && same_layout(&kept, &lights.iter().collect::<Vec<_>>())) {
        mask = 0;
    }
    if mask == 0 {
        kept.clear();
        if !schedule.is_empty() {
            kept.extend(lights.iter().map(|(e, c)| (e, c.clone())));
        }
    } else {
        for (light, mut cascades) in &mut lights {
            let Some(k) = kept.get_mut(&light) else { continue };
            for (view, list) in cascades.cascades.iter_mut() {
                let Some(kl) = k.cascades.get_mut(view) else { continue };
                for (i, (c, kc)) in list.iter_mut().zip(kl.iter_mut()).enumerate() {
                    if mask & (1 << i) != 0 {
                        *c = kc.clone();
                    } else {
                        *kc = c.clone();
                    }
                }
            }
        }
    }
    // Once for each schedule that comes into use (the switch on, or the staggered one with
    // it off): the first frame that kept maps.
    if schedule.is_empty() {
        *said = None;
    } else if mask != 0 && *said != Some(on.0) {
        if on.0 {
            info!("sun shadows: a frame kept the shadow maps of the frame before ({} light(s))", kept.len());
        } else {
            info!("sun shadows: a frame kept its farther cascades' maps (staggered schedule, held cascades {mask:#05b}, {} light(s))", kept.len());
        }
        *said = Some(on.0);
    }
    if hold.0 != mask {
        hold.0 = mask;
    }
}

/// `performance`: start in Performance mode (`--sun-shadows=performance`; the program sets
/// `SkyState`'s cascade count to match).
pub struct ShadowRatePlugin {
    pub performance: bool,
}

impl Plugin for ShadowRatePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PerformanceShadows(self.performance))
            .insert_resource(CascadesBeforePerformance(self.performance.then_some(crate::sky::DEFAULT_SHADOW_CASCADES)))
            .init_resource::<HoldDirectionalShadowMaps>()
            .add_plugins(ExtractResourcePlugin::<HoldDirectionalShadowMaps>::default())
            .add_systems(
                PostUpdate,
                hold_cascades.after(SimulationLightSystems::UpdateDirectionalLightCascades).before(SimulationLightSystems::UpdateLightFrusta),
            )
            .add_systems(Update, far_trees_cast_no_shadow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::light::cascade::Cascade;

    fn cascades(view: Entity, n: usize) -> Cascades {
        let mut c = Cascades::default();
        c.cascades.insert(view, vec![Cascade::default(); n]);
        c
    }

    #[test]
    fn maps_are_held_only_for_the_same_lights_and_cascades() {
        let (sun, arms, view) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let (three, two) = (cascades(view, 3), cascades(view, 2));
        let mut kept = EntityHashMap::default();
        kept.insert(sun, three.clone());
        assert!(same_layout(&kept, &[(sun, &three)]));
        // Never without lights.
        assert!(!same_layout(&kept, &[]));
        // A light that was not there (the arms' shade light switching on), a different
        // number of cascades, or another view: this frame draws.
        assert!(!same_layout(&kept, &[(sun, &three), (arms, &three)]));
        assert!(!same_layout(&kept, &[(sun, &two)]));
        assert!(!same_layout(&kept, &[(sun, &cascades(arms, 3))]));
    }

    /// The switch gives back the cascade count it replaced (#2182 review: it reset a
    /// `--graphics-cycle cascades=` value to the default), and only sets the count.
    #[test]
    fn the_switch_gives_back_the_cascades_it_replaced() {
        let mut world = World::new();
        world.init_resource::<PerformanceShadows>();
        let mut sky = SkyState::default();
        sky.shadow_cascades = 4;
        world.insert_resource(sky);
        set_performance(&mut world, true);
        assert_eq!(world.resource::<SkyState>().shadow_cascades, PERFORMANCE_CASCADES);
        set_performance(&mut world, true);
        set_performance(&mut world, false);
        assert_eq!(world.resource::<SkyState>().shadow_cascades, 4);
        // Off when it already is: the count stays.
        set_performance(&mut world, false);
        assert_eq!(world.resource::<SkyState>().shadow_cascades, 4);
    }

    #[test]
    fn the_schedules_hold_the_cascades_they_should() {
        // Performance: both cascades every other frame, together.
        let performance: Vec<u32> = (0..4).map(|f| held_mask(&PERFORMANCE_SCHEDULE, f, 2)).collect();
        assert_eq!(performance, [0, 0b11, 0, 0b11]);
        // Staggered: the nearest every frame, the middle every other, the farthest every
        // fourth, never both of the far two drawn in one frame.
        let staggered: Vec<u32> = (0..4).map(|f| held_mask(&STAGGERED_SCHEDULE, f, 3)).collect();
        assert_eq!(staggered, [0b110, 0b100, 0b010, 0b100]);
        // More cascades than the schedule: those are drawn every frame.
        assert_eq!(held_mask(&PERFORMANCE_SCHEDULE, 1, 3), 0b011);
        assert_eq!(held_mask(&[], 1, 3), 0);
    }

    #[test]
    fn a_far_tree_stops_casting_and_starts_again_nearer() {
        assert!(tree_casts(TREE_SHADOW_NEAR, true) && !tree_casts(TREE_SHADOW_NEAR + 1.0, true));
        // Between the two distances it stays as it is.
        let between = (TREE_SHADOW_NEAR + TREE_SHADOW_BACK) / 2.0;
        assert!(tree_casts(between, true) && !tree_casts(between, false));
        assert!(tree_casts(TREE_SHADOW_BACK, false));
    }
}
