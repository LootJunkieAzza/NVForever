//! Imposters: references with the XSRF Imposter flag are drawn while the camera stands
//! in a region whose imposter list names them, at any distance, and not otherwise; cells
//! never draw them (`world::load_cell`). The rule and its sources: `fnv_world::imposter`,
//! docs/spec-notes/world.md §2.
//!
//! WastelandNV's are the Strip's walls, hotels and light glows and the South Vegas
//! buildings, as flat stand-ins seen from far vantage points (StripImposter,
//! StripImposterBlackMountain, StripImposterJacobstown); near Vegas
//! (StripImposterClose) only near versions of the glows are listed, and the full
//! buildings in the loaded cells show [observed].
//!
//! The camera's position stands for the player's [engineering choice: in play mode the
//! camera is the player's eye; the free-fly camera has no player].

use crate::emittance::EmittanceState;
use crate::world::{bevy_to_game, ref_affine, spawn_model, Streamer, WorldData, WorldRoot};
use bevy::prelude::*;
use fnv_world::placed::{base_object, placement};
use nvf_assets::model::ModelCtx;
use std::collections::{BTreeSet, HashMap};

/// The imposters drawn now.
#[derive(Resource, Default)]
pub struct Imposters {
    /// The worldspace and the regions (indices into `WorldData::imposters`) the camera
    /// was in when the drawn set was chosen; `None` before the first choice.
    chosen: Option<(u32, Vec<usize>)>,
    /// Reference → the entity holding its model (a child of the world root).
    drawn: HashMap<u32, Entity>,
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    world: Res<WorldData>,
    mut streamer: ResMut<Streamer>,
    mut state: ResMut<Imposters>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut models: ModelCtx,
    mut emittance: ResMut<EmittanceState>,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let regions: Vec<usize> = if streamer.interior.is_some() {
        Vec::new()
    } else {
        let rs = &world.imposters.regions;
        (0..rs.len()).filter(|&i| rs[i].world == Some(world.world) && rs[i].contains(g.x, g.y)).collect()
    };
    let key = (world.world, regions);
    if state.chosen.as_ref() == Some(&key) {
        return;
    }
    let rs = &world.imposters.regions;
    let listed: BTreeSet<u32> = key.1.iter().flat_map(|&i| rs[i].refs.iter().copied()).collect();
    // Only flagged references (the GECK draws a listed one only with the flag
    // [documented: GECK wiki "Reference"]), enabled at the start; a script's Enable or
    // Disable later is `RefVisual` sync's (nvf-play).
    let want: Vec<(u32, fnv_world::Placement)> = listed
        .iter()
        .filter_map(|&r| Some((r, placement(world.lo, world.schema, r)?)))
        .filter(|(_, p)| p.imposter && world.enabled(p, 0))
        .collect();
    let gone: Vec<u32> = state.drawn.keys().filter(|r| !listed.contains(r)).copied().collect();
    for r in &gone {
        if let Some(e) = state.drawn.remove(r) {
            commands.entity(e).despawn();
        }
    }
    let s = &mut *streamer;
    let mut added = 0;
    for (r, p) in &want {
        if state.drawn.contains_key(r) {
            continue;
        }
        let Some(base) = p.base.and_then(|b| base_object(world.lo, world.schema, b)) else { continue };
        let holder = commands.spawn((Name::new(format!("Imposter {r:08X}")), Transform::IDENTITY, Visibility::default())).id();
        commands.entity(root).add_child(holder);
        spawn_model(&mut commands, holder, &world, p, *r, &base, ref_affine(p), &mut models, &mut emittance, &s.hide, &mut s.stats);
        state.drawn.insert(*r, holder);
        added += 1;
    }
    let names: Vec<&str> = key.1.iter().map(|&i| rs[i].edid.as_deref().unwrap_or("?")).collect();
    info!(
        "imposters: {} drawn ({} added, {} removed) for regions [{}]",
        state.drawn.len(),
        added,
        gone.len(),
        names.join(", ")
    );
    state.chosen = Some(key);
}
