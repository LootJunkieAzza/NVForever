//! nvforever — the NVForever game binary. At milestone M3 it is a free-camera explorer:
//! terrain and objects of a worldspace from the user's own game data, streamed around the
//! camera (docs/design/layer3.md). Read-only: plugins and archives are only read.
//!
//! Keys: W/A/S/D move, Q/E down/up, Shift faster, right mouse drag look, wheel speed,
//! F12 screenshot, Esc quit.

mod fly;
mod lod;
mod sky;
mod terrain;
mod world;

use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use clap::Parser;
use fly::FlyCam;
use fnv_world::land::{load_land, LandParts, SIDE};
use nvf_assets::{ArchivePlugin, GameData, ModelCache, TextureCache};
use std::path::PathBuf;
use terrain::{TerrainCache, TerrainSettings};
use world::{game_to_bevy, Streamer, WorldData};

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

/// NVForever world explorer (terrain and objects from your own game data).
#[derive(Parser, Clone, Resource)]
#[command(name = "nvforever", version)]
struct Args {
    /// The game's Data folder (read-only). Defaults to $NVFOREVER_DATA.
    #[arg(long, env = "NVFOREVER_DATA")]
    data: PathBuf,
    /// Plugins in load order (default: the base game and all DLC masters).
    #[arg(long, num_args = 1.., default_values_t = MASTERS.map(String::from))]
    plugins: Vec<String>,
    /// Record schema directory.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv"))]
    esm_schema: PathBuf,
    /// NIF schema directory.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/nif"))]
    nif_schema: PathBuf,
    /// Worldspace Editor ID or FormID.
    #[arg(long, default_value = "WastelandNV")]
    world: String,
    /// Start in this exterior cell: Editor ID (e.g. Goodsprings) or "x,y".
    #[arg(long, default_value = "Goodsprings", allow_hyphen_values = true)]
    cell: String,
    /// Start at this game position "x,y,z" instead of above the cell's centre.
    #[arg(long, allow_hyphen_values = true)]
    pos: Option<String>,
    /// Compass heading in degrees (0 north, 90 east).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    heading: f32,
    /// Pitch in degrees (negative looks down).
    #[arg(long, default_value_t = -10.0, allow_hyphen_values = true)]
    pitch: f32,
    /// Cells loaded around the camera's cell in each direction.
    #[arg(long, default_value_t = 2)]
    radius: i32,
    /// Time of day in hours (0-24).
    #[arg(long, default_value_t = 12.0)]
    hour: f32,
    /// Weather Editor ID or load-order FormID (default: the climate's most likely).
    #[arg(long)]
    weather: Option<String>,
    /// Override the weather's fog distances: "near,far" in game units.
    #[arg(long)]
    fog: Option<String>,
    /// Distant LOD (terrain chunks, object blocks) within this many 4-cell chunks.
    #[arg(long, default_value_t = 8)]
    lod_radius: i32,
    /// Do not draw distant LOD.
    #[arg(long)]
    no_lod: bool,
    /// World units per terrain texture repeat [hypothesis; docs/design/layer3.md].
    #[arg(long, default_value_t = 4096.0 / 6.0)]
    terrain_tile: f32,
    /// Do not draw models whose path contains any of these texts (diagnostics).
    #[arg(long, value_delimiter = ',')]
    hide: Vec<String>,
    /// Render, save a PNG here once the cells around the start are loaded, and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Window size "WxH" (also the screenshot size).
    #[arg(long, default_value = "1280x800")]
    size: String,
}

#[derive(Component)]
struct Overlay;

fn parse_vec<const N: usize>(s: &str) -> Option<[f32; N]> {
    let v: Vec<f32> = s.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    v.try_into().ok()
}

fn main() -> AppExit {
    let args = Args::parse();
    let fail = |e: String| {
        eprintln!("error: {e}");
        AppExit::error()
    };
    let game = match GameData::open(&args.data, &args.nif_schema) {
        Ok(g) => g,
        Err(e) => return fail(e),
    };
    let t0 = std::time::Instant::now();
    let world = match WorldData::open(&args.data, &args.esm_schema, &args.plugins, &args.world) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    info!("world index built in {:.2}s", t0.elapsed().as_secs_f64());

    // Start position (game space).
    let start = match args.pos.as_deref() {
        Some(p) => match parse_vec::<3>(p) {
            Some(v) => Vec3::from_array(v),
            None => return fail(format!("--pos {p}: expected x,y,z")),
        },
        None => {
            let grid = match parse_vec::<2>(&args.cell) {
                Some([x, y]) => (x as i32, y as i32),
                None => match world.index.cell_by_name(&args.cell).and_then(|c| c.grid) {
                    Some(g) => g,
                    None => return fail(format!("no exterior cell {}", args.cell)),
                },
            };
            let ground = world
                .grid_cell(grid)
                .and_then(|c| c.land)
                .and_then(|l| load_land(&world.lo, world.schema, l, LandParts::HEIGHTS))
                .and_then(|l| l.heights)
                .map_or(0.0, |h| h[(SIDE / 2) * SIDE + SIDE / 2]);
            let s = fnv_world::CELL_SIZE;
            Vec3::new((grid.0 as f32 + 0.5) * s, (grid.1 as f32 + 0.5) * s, ground + 400.0)
        }
    };
    let mut sky_state = match sky::SkyState::new(&world, args.weather.as_deref(), args.hour) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    if let Some(f) = args.fog.as_deref() {
        match parse_vec::<2>(f) {
            Some([n, fa]) => sky_state.fog_override = Some((n, fa)),
            None => return fail(format!("--fog {f}: expected near,far")),
        }
    }
    let Some([w, h]) = args.size.split_once('x').and_then(|(w, h)| Some([w.parse::<u32>().ok()?, h.parse::<u32>().ok()?])) else {
        return fail(format!("--size {}: expected WxH", args.size));
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin {
                default_sampler: ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                },
            })
            .set(WindowPlugin {
                primary_window: Some(Window { title: "NVForever".into(), resolution: (w, h).into(), ..default() }),
                ..default()
            }),
    )
    .add_plugins(ArchivePlugin(game))
    .insert_resource(ClearColor(Color::srgb(0.62, 0.72, 0.85)))
    .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
    .insert_resource(sky_state)
    .insert_resource(lod::LodState::new(&world, args.lod_radius, !args.no_lod))
    .insert_resource(TerrainSettings { tile: args.terrain_tile })
    .init_resource::<TerrainCache>()
    .insert_resource(Streamer {
        radius: args.radius,
        loaded: Default::default(),
        per_frame: 1,
        hide: args.hide.iter().map(|h| h.to_ascii_lowercase()).collect(),
        stats: Default::default(),
    })
    .insert_resource(world)
    .insert_resource(StartPose { position: start, heading: args.heading, pitch: args.pitch })
    .insert_resource(args.clone())
    .add_systems(Startup, (world::setup_root, setup, sky::setup_sky))
    .add_systems(Update, (fly::fly, sky::sky_keys, sky::update_sky, world::stream, lod::update_lod, overlay, keys).chain());
    if args.screenshot.is_some() {
        app.add_systems(Update, screenshot_and_exit);
    }
    app.run()
}

#[derive(Resource)]
struct StartPose {
    position: Vec3,
    heading: f32,
    pitch: f32,
}

fn setup(mut commands: Commands, start: Res<StartPose>) {
    let pos = game_to_bevy().transform_point(start.position);
    let (yaw, pitch) = (-start.heading.to_radians(), start.pitch.to_radians());
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { far: 400_000.0, near: 5.0, ..default() }),
        Transform::from_translation(pos).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
        FlyCam { yaw, pitch, speed: 800.0 },
        sky::initial_fog(),
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        Node { position_type: PositionType::Absolute, top: Val::Px(8.0), left: Val::Px(8.0), ..default() },
        Overlay,
    ));
}

#[allow(clippy::too_many_arguments)]
fn overlay(
    streamer: Res<Streamer>,
    models: Res<ModelCache>,
    terrain: Res<TerrainCache>,
    textures: Res<TextureCache>,
    sky: Res<sky::SkyState>,
    lod: Res<lod::LodState>,
    cam: Query<&GlobalTransform, With<FlyCam>>,
    mut text: Query<&mut Text, With<Overlay>>,
) {
    let (Ok(cam), Ok(mut t)) = (cam.single(), text.single_mut()) else { return };
    let g = world::bevy_to_game(cam.translation());
    let c = fnv_world::cell_of(g.x, g.y);
    let s = &streamer.stats;
    t.0 = format!(
        "pos {:.0}, {:.0}, {:.0}  cell {},{}\ncells {} ({} loads, {:.0} ms total)  refs drawn {}  disabled {}  not drawn {}  speedtree {}  no model {}  shapes {}\nmodels {} (failed {})  textures failed {}  terrain layers {}  quadrants without base {}  water {}  LOD chunks {} blocks {}\n{}\nWASD/QE move  Shift fast  right-drag look  wheel speed  [ ] hour  F12 screenshot  Esc quit",
        g.x, g.y, g.z, c.0, c.1,
        streamer.loaded.len(), s.cells_loaded, s.load_ms,
        s.refs_drawn, s.refs_skipped_disabled, s.refs_skipped_type, s.refs_speedtree, s.refs_no_model, s.shapes,
        models.len(), models.failures.len(), textures.failures.len(), terrain.layers, terrain.quadrants_without_base,
        s.water_planes, lod.chunks, lod.blocks, sky.describe(),
    );
}

fn keys(input: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut n: Local<u32>) {
    if input.just_pressed(KeyCode::Escape) {
        commands.write_message(AppExit::Success);
    }
    if input.just_pressed(KeyCode::F12) {
        *n += 1;
        let path = format!("nvforever-{:03}.png", *n);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// --screenshot: wait for the cells around the start and for pipelines and textures, save,
/// exit.
#[allow(clippy::too_many_arguments)]
fn screenshot_and_exit(
    mut frames: Local<u32>,
    mut elapsed: Local<f32>,
    mut ready_at: Local<Option<f32>>,
    time: Res<Time>,
    args: Res<Args>,
    streamer: Res<Streamer>,
    models: Res<ModelCache>,
    textures: Res<TextureCache>,
    mut commands: Commands,
) {
    *frames += 1;
    *elapsed += time.delta_secs();
    let side = (2 * streamer.radius + 1) as usize;
    if ready_at.is_none() && streamer.loaded.len() >= side * side {
        *ready_at = Some(*elapsed);
        let s = &streamer.stats;
        info!(
            "cells loaded in {:.2}s ({:.0} ms loading): refs drawn {}, disabled {}, not drawn {} {:?}, no model {}, shapes {}",
            *elapsed, s.load_ms, s.refs_drawn, s.refs_skipped_disabled, s.refs_skipped_type, s.skipped_types, s.refs_no_model, s.shapes
        );
    }
    let Some(r) = *ready_at else { return };
    // Pipelines compile during the first seconds; give them time after the last load.
    if *frames < 10_000 && *elapsed >= r + 4.0 {
        if let Some(path) = &args.screenshot {
            for f in models.failures.iter().take(20) {
                warn!("model: {f}");
            }
            for f in textures.failures.iter().take(20) {
                warn!("texture: {f}");
            }
            info!("models failed {}, textures failed {}", models.failures.len(), textures.failures.len());
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
        }
        *frames = 10_000;
        *ready_at = Some(*elapsed);
    } else if *frames >= 10_000 && *elapsed >= r + 3.0 {
        commands.write_message(AppExit::Success);
    }
}
