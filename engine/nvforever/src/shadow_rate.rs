//! Lighter sun shadows: a settings switch (off by default) that trades some of the sun
//! shadows' precision for frame time. [engineering choices throughout; the original casts
//! no sun shadows from the world at all, docs/spec-notes/world.md §6]
//!
//! With the switch on:
//! - **The sun's shadow maps are drawn every other frame** ([`hold_cascades`]). On the
//!   frames between, the lights keep the cascades of the frame that drew them, so the maps
//!   and the matrices they are read with still belong together, and the renderer is told
//!   to leave the maps alone (`bevy::pbr::HoldDirectionalShadowMaps`, a patched bevy_pbr:
//!   engine/vendor/README.md). The shadow of a moving thing then moves at half the frame
//!   rate, and on a fast turn the edge of the view can lie outside a held cascade for one
//!   frame (unshadowed).
//! - **The sun's shadows are split over [`LIGHTER_CASCADES`] cascades** instead of
//!   `sky::DEFAULT_SHADOW_CASCADES` ([`set_lighter`]): one shadow view less, and shadows
//!   near the camera a little coarser.
//! - **The first-person arms have no shade light of their own** (player_body.rs
//!   `shade_light`): another shadow view less; the arms take the sun's own shadows.
//! - **A tree casts a shadow only within [`TREE_SHADOW_NEAR`] of the camera**
//!   ([`far_trees_cast_no_shadow`]): a SpeedTree tree is thousands of leaf cards and a
//!   finely divided trunk in every cascade it falls in.
//!
//! What it costs and saves: docs/design/frame-rate-audit.md §9.

use crate::fly::FlyCam;
use crate::sky::{SkyState, Sun};
use bevy::ecs::entity::EntityHashMap;
use bevy::light::{Cascades, DirectionalLight, DirectionalLightShadowMap, NotShadowCaster, SimulationLightSystems};
use bevy::pbr::HoldDirectionalShadowMaps;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResourcePlugin;

/// The switch (settings key `lighter-shadows`).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct LighterShadows(pub bool);

/// The sun's cascades with the switch on.
pub const LIGHTER_CASCADES: usize = 2;

/// With the switch on a tree casts a shadow within this distance of the camera (game
/// units: about 43 m), and again once it is back within [`TREE_SHADOW_BACK`] of it.
pub const TREE_SHADOW_NEAR: f32 = 3000.0;
pub const TREE_SHADOW_BACK: f32 = 2700.0;

/// Seconds between two looks at the trees.
const TREES_EVERY: f32 = 0.5;

/// Turn the switch on or off now (the settings page): the cascade count changes with it.
pub fn set_lighter(world: &mut World, on: bool) {
    world.resource_mut::<LighterShadows>().0 = on;
    let (shadows, distance, reach, cascades) = {
        let mut sky = world.resource_mut::<SkyState>();
        sky.shadow_cascades = if on { LIGHTER_CASCADES } else { crate::sky::DEFAULT_SHADOW_CASCADES };
        (sky.shadows, sky.shadow_distance, sky.shadow_reach, sky.shadow_cascades)
    };
    info!("sun shadows: {}, {cascades} cascades", if on { "lighter (maps drawn every other frame, trees cast near the camera only, no arms shade light)" } else { "drawn every frame" });
    if shadows {
        let suns: Vec<Entity> = world.query_filtered::<Entity, With<Sun>>().iter(world).collect();
        for sun in suns {
            world.entity_mut(sun).insert(crate::sky::shadow_cascades(distance, reach, cascades));
        }
    }
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
    on: Res<LighterShadows>,
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

/// Whether a frame may reuse the maps of the frame before: that frame drew them, and every
/// light has the cascades it drew them with, for the same views and as many of them.
fn can_hold(drew_last: bool, kept: &EntityHashMap<Cascades>, now: &[(Entity, &Cascades)]) -> bool {
    drew_last
        && !now.is_empty()
        && now.iter().all(|(light, cascades)| {
            kept.get(light).is_some_and(|k| {
                k.cascades.len() == cascades.cascades.len()
                    && cascades.cascades.iter().all(|(view, c)| k.cascades.get(view).is_some_and(|kc| kc.len() == c.len()))
            })
        })
}

/// Every other frame, give the directional lights back the cascades of the frame before
/// and have the renderer keep its shadow maps (module notes). Runs after Bevy has built
/// this frame's cascades and before it fits the lights' frusta to them.
fn hold_cascades(
    on: Res<LighterShadows>,
    map: Option<Res<DirectionalLightShadowMap>>,
    mut hold: ResMut<HoldDirectionalShadowMaps>,
    mut lights: Query<(Entity, &mut Cascades), With<DirectionalLight>>,
    mut kept: Local<EntityHashMap<Cascades>>,
    mut drew_last: Local<bool>,
    mut said: Local<bool>,
) {
    // A new map size makes new maps: nothing to keep.
    let same_maps = !map.is_some_and(|m| m.is_changed());
    let holds = on.0 && same_maps && {
        let now: Vec<(Entity, &Cascades)> = lights.iter().collect();
        can_hold(*drew_last, &kept, &now)
    };
    if holds {
        for (light, mut cascades) in &mut lights {
            if let Some(k) = kept.get(&light) {
                *cascades = k.clone();
            }
        }
    } else {
        kept.clear();
        if on.0 {
            kept.extend(lights.iter().map(|(e, c)| (e, c.clone())));
        }
    }
    *drew_last = !holds;
    // Once each time the switch goes on: the first frame that kept its maps.
    if holds && !*said {
        info!("sun shadows: a frame kept the shadow maps of the frame before ({} light(s))", kept.len());
    }
    *said = on.0 && (*said || holds);
    if hold.0 != holds {
        hold.0 = holds;
    }
}

pub struct ShadowRatePlugin;

impl Plugin for ShadowRatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LighterShadows>()
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
    fn maps_are_held_only_after_a_frame_that_drew_them_for_the_same_lights_and_cascades() {
        let (sun, arms, view) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let (three, two) = (cascades(view, 3), cascades(view, 2));
        let mut kept = EntityHashMap::default();
        kept.insert(sun, three.clone());
        // The frame before drew, and the sun's cascades are those it drew with.
        assert!(can_hold(true, &kept, &[(sun, &three)]));
        // Not two frames running, and never without lights.
        assert!(!can_hold(false, &kept, &[(sun, &three)]));
        assert!(!can_hold(true, &kept, &[]));
        // A light that was not there (the arms' shade light switching on), a different
        // number of cascades, or another view: this frame draws.
        assert!(!can_hold(true, &kept, &[(sun, &three), (arms, &three)]));
        assert!(!can_hold(true, &kept, &[(sun, &two)]));
        assert!(!can_hold(true, &kept, &[(sun, &cascades(arms, 3))]));
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
