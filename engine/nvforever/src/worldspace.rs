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
//! Each worldspace's own records are used. A child worldspace (WRLD `WNAM`) may take data
//! from its parent by its `PNAM` flags [documented: xEdit names, schemas/fnv/world.json];
//! every child worldspace in the ten masters sets only "Use Map Data" and has its own
//! climate, water, image space and land [observed: `fnvtool world list`, `esm decode`],
//! so taking the parent's is not implemented beyond what the start already does.

use crate::lod::{GenState, LodState};
use crate::sky::{self, CloudLayer, MoonDisc, SkyDome, SkyState, Stars, Sun};
use crate::water::DistantWater;
use crate::world::WorldData;
use crate::Args;
use bevy::prelude::*;
use nvf_assets::GameData;

/// The sky's entities, spawned by `sky::setup_sky`.
type SkyEntity = Or<(With<SkyDome>, With<CloudLayer>, With<Sun>, With<Stars>, With<MoonDisc>)>;

/// What a new sky keeps of the one it replaces: the settings, the clock, the scripts'
/// wind and moons.
fn carry_sky(from: &SkyState, s: &mut SkyState) {
    s.shadows = from.shadows;
    s.shadow_distance = from.shadow_distance;
    s.shadow_reach = from.shadow_reach;
    s.shadow_cascades = from.shadow_cascades;
    s.grade = from.grade;
    s.time_scale = from.time_scale;
    s.clock_running = from.clock_running;
    s.days_passed = from.days_passed;
    s.clock_seen = from.clock_seen;
    s.fog_override = from.fog_override;
    s.script_wind = from.script_wind;
    s.moon_textures = from.moon_textures.clone();
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
                carry_sky(&sky, &mut s);
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
