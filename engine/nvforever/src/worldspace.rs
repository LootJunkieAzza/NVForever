//! Door travel into another worldspace (`--play`; docs/design/layer4-play.md §3): a
//! `nvf_play::Travel` whose `world` differs from the one drawn switches what is drawn to
//! that worldspace, rebuilt with the same setup code the start uses:
//! - `WorldData` points at the new worldspace (its grid cells and persistent references);
//!   `world::travel`, which runs next, drops the loaded cells and render streaming loads
//!   the destination's cells around the camera.
//! - Sky and weather: `SkyState::new` from the new worldspace's climate (or its parent's,
//!   as at the start) with the clock and the command-line choices carried over, and the
//!   sky's entities spawned again by `sky::setup_sky` (stars, moon and sun depend on the
//!   climate). Its image space comes with it (`SkyState::image_space`).
//! - Distant LOD: a fresh `LodState` (its paths are named after the worldspace), with
//!   generated distant terrain for the new worldspace unless switched off.
//! - Water: distant water planes and their per-cell surfaces are dropped (they hold the
//!   old worldspace's default water height and water type).
//!
//! A child worldspace (WRLD `WNAM`) takes data from its parent by its `PNAM` flags
//! [documented: xEdit names, schemas/fnv/world.json]. The world index follows them for
//! land, default heights, water type and climate (`fnv_world::WorldIndex::take_parent_data`,
//! #2345; world.md §2.1), the distant LOD's files are the parent's with Use LOD Data
//! (`WorldIndex::lod_world`), and the image space falls back to the parent's. Every child
//! in the ten New Vegas masters sets only "Use Map Data"; Tale of Two Wastelands' Megaton,
//! Oasis, Paradise Falls and the DC roof worlds set more.

use crate::lod::{GenState, LodState};
use crate::sky::{self, CloudLayer, MoonDisc, SkyDome, SkyState, Stars, Sun};
use crate::water::DistantWater;
use crate::world::WorldData;
use crate::Args;
use bevy::prelude::*;
use nvf_assets::GameData;

/// The sky's entities, spawned by `sky::setup_sky`.
type SkyEntity = Or<(With<SkyDome>, With<CloudLayer>, With<Sun>, With<Stars>, With<MoonDisc>)>;

/// [design: sky.rs::SkyState::carry_weather_from] A new sky keeps settings, clock,
/// script wind, moons and weather overrides from the one it replaces.
fn carry_sky(from: &SkyState, s: &mut SkyState) {
    s.carry_weather_from(from);
    s.shadows = from.shadows;
    s.shadow_distance = from.shadow_distance;
    s.shadow_reach = from.shadow_reach;
    s.shadow_layout = from.shadow_layout;
    s.grade = from.grade;
    s.time_scale = from.time_scale;
    s.clock_running = from.clock_running;
    s.days_passed = from.days_passed;
    s.clock_seen = from.clock_seen;
    s.fog_override = from.fog_override;
    s.weather_fog_overrides = from.weather_fog_overrides.clone();
    s.weather_visual_overrides = from.weather_visual_overrides.clone();
    s.script_wind = from.script_wind;
    s.moon_textures = from.moon_textures.clone();
}

/// Seed current gates before restoring sampled placement and selecting weather
/// [design, #2531; `SkyState::carry_region_placement`]. Worldspace travel uses
/// `carry_sky` instead.
pub(crate) fn carry_climate_sky(from: &SkyState, s: &mut SkyState, global_value: impl Fn(u32) -> Option<f32>) {
    for id in s.gate_globals() {
        if let Some(value) = global_value(id) {
            s.set_live_global(id, value);
        }
    }
    carry_sky(from, s);
    if from.world_image_space_id.is_some() && from.world_image_space_id == s.world_image_space_id {
        s.image_space = from.image_space;
    }
    s.carry_region_placement(from);
}

/// The climate the scripts want (ForceClimate, SetCurrentClimate; `fnv_game::nvse_jip12`)
/// and their moon textures (SetMoonTexture): the sky is made again with them, keeping its
/// weather when the climate changes [hypothesis: the original goes on to the new
/// climate's weathers from there]. Outdoors only; the worldspace's own climate comes back
/// when no script wants one.
#[allow(clippy::too_many_arguments)]
pub fn script_sky(
    mut commands: Commands,
    script: Option<Res<nvf_play::script::ScriptState>>,
    sim: Res<nvf_play::sim::SimState>,
    world: Res<WorldData>,
    mut sky: ResMut<SkyState>,
    sky_entities: Query<Entity, SkyEntity>,
    args: Res<Args>,
    mut failed: Local<Option<u32>>,
) {
    let Some(g) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let moons = g.moon_textures();
    let wanted = g.script_climate().map(|c| c.0);
    let own = || {
        let ws = world.index.worldspaces.get(&world.world)?;
        ws.climate.or_else(|| ws.parent.and_then(|p| world.index.worldspaces.get(&p)).and_then(|p| p.climate))
    };
    let climate = if sky.interior.is_some() { None } else { wanted.or_else(own) };
    let mut remake = false;
    if let Some(c) = climate.filter(|&c| c != sky.climate.form_id && *failed != Some(c)) {
        let weather = sky.weather.edid.clone();
        match SkyState::with_climate(&world, c, args.weather.as_deref(), weather.as_deref(), sky.hour, sky.rng) {
            Ok(mut s) => {
                // Preserve live SetWorldspaceImagespace / exterior SetCellImageSpace across
                // same-worldspace climate rebuilds before carry_climate_sky runs.
                let active_space = sky
                    .world_image_space_id
                    .filter(|&id| Some(id) != s.world_image_space_id)
                    .or_else(|| crate::world::effective_worldspace_image_space_id(&world, Some(g)));
                if active_space != s.world_image_space_id {
                    s.world_image_space_id = active_space;
                    s.image_space = active_space.and_then(|id| crate::world::effective_image_space(&world, Some(g), id));
                }
                carry_climate_sky(&sky, &mut s, |id| sim.world.global(fnv_sim::FormId(id)).ok());
                info!("sky: climate {:08X} -> {c:08X}{}: {}", sky.climate.form_id, if wanted.is_some() { " (a script's)" } else { "" }, s.describe());
                *sky = s;
                remake = true;
            }
            Err(e) => {
                warn!("sky: climate {c:08X}: {e}; keeping {:08X}", sky.climate.form_id);
                *failed = Some(c);
            }
        }
    }
    if sky.moon_textures != moons {
        info!("sky: moon textures set by a script: {:?}", moons.iter().enumerate().filter_map(|(k, t)| Some((k, t.as_deref()?))).collect::<Vec<_>>());
        sky.moon_textures = moons;
        remake = true;
    }
    if remake {
        for e in &sky_entities {
            commands.entity(e).despawn();
        }
        commands.run_system_cached(sky::setup_sky);
    }
}

/// Switch to the travel's worldspace when it is another one (runs before `world::travel`).
#[allow(clippy::too_many_arguments)]
pub fn switch(
    mut commands: Commands,
    mut msgs: MessageReader<nvf_play::Travel>,
    mut world: ResMut<WorldData>,
    mut sky: ResMut<SkyState>,
    mut lod: ResMut<LodState>,
    mut water: ResMut<DistantWater>,
    sky_entities: Query<Entity, SkyEntity>,
    game: Res<GameData>,
    args: Res<Args>,
) {
    let drawn = world.world;
    let Some(to) = msgs.read().last().and_then(|t| t.world).filter(|&w| w != drawn) else { return };
    let Some(ws) = world.index.worldspaces.get(&to) else {
        warn!("travel into worldspace {to:08X}: not in the world index; staying in {:08X}", world.world);
        return;
    };
    let name = ws.edid.clone().unwrap_or_default();
    info!("render: worldspace {:08X} -> {to:08X} ({name})", world.world);
    world.set_world(to);

    match SkyState::new(&world, args.weather.as_deref(), None, sky.hour, sky.rng) {
        Ok(mut s) => {
            carry_sky(&sky, &mut s);
            info!("render: {name}: {}", s.describe());
            *sky = s;
            for e in &sky_entities {
                commands.entity(e).despawn();
            }
            commands.run_system_cached(sky::setup_sky);
        }
        Err(e) => warn!("render: sky of {name}: {e}; keeping the previous worldspace's sky"),
    }

    lod.set_enabled(&mut commands, false);
    // Also with --no-lod: the settings page can switch distant LOD on later.
    let gen = if args.no_lodgen {
        None
    } else {
        match crate::lodgen_job(&args, &world, &game) {
            Ok(job) => Some(GenState::new(job, args.lodgen_radius.max(0))),
            Err(e) => {
                warn!("render: generated LOD for {name}: {e}");
                None
            }
        }
    };
    *lod = LodState::new(&world, crate::lod_settings(&args), !args.no_lod, gen).with_origin(crate::lod::grid_origin(&game, &world));

    water.clear(&mut commands);
}
