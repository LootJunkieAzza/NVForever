//! nvforever — the NVForever game binary. At milestone M3 it is a free-camera explorer:
//! terrain and objects of a worldspace from the user's own game data, streamed around the
//! camera (docs/design/layer3.md). Read-only: plugins and archives are only read.
//!
//! Keys: W/A/S/D move, Q/E down/up, Shift faster, right mouse drag look, wheel speed,
//! F12 screenshot, Esc quit. With `--play` the camera is the player instead (Layer 4,
//! `nvf-play`: walking with collision, activation, menus; docs/design/layer4-play.md),
//! and Esc in a menu closes the menu.

mod actors;
mod emittance;
mod fade;
mod facegen;
mod fly;
mod lights;
mod lod;
mod locomotion;
mod weapons;
mod lodgen;
mod reflection;
mod ripples;
mod imagespace;
mod sky;
mod terrain;
mod tree;
mod treelod;
mod underwater;
mod water;
mod world;
mod worldspace;

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
    /// Cells loaded around the camera's cell in each direction (2: the 5 x 5 cells of
    /// the game's uGridsToLoad=5, its default and the owner's INI value).
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
    /// Distant LOD (terrain chunks, object blocks) out to this distance in game units
    /// (the game's fBlockLoadDistance; default the owner's INI value) [hypothesis: lod.rs].
    #[arg(long, default_value_t = 125_000.0)]
    lod_distance: f32,
    /// Finest terrain LOD (level 4) within this distance (fBlockLoadDistanceLow).
    #[arg(long, default_value_t = 50_000.0)]
    lod_near_distance: f32,
    /// Coarser terrain LOD splits while the camera is nearer than this x its side
    /// (fSplitDistanceMult).
    #[arg(long, default_value_t = 1.5)]
    lod_split: f32,
    /// Distant trees drawn as billboards out to this distance (fTreeLoadDistance); 0: none.
    #[arg(long, default_value_t = 40_000.0)]
    tree_distance: f32,
    /// Do not draw distant LOD.
    #[arg(long)]
    no_lod: bool,
    /// Seconds a streamed cell takes to fade in or out against the distant LOD; 0 swaps
    /// at once (fade.rs).
    #[arg(long, default_value_t = 1.0)]
    cell_fade: f32,
    /// Do not generate distant terrain from the landscape records; draw only the game's
    /// LOD chunks (docs/design/generated-lod.md).
    #[arg(long)]
    no_lodgen: bool,
    /// Generated terrain replaces the game's level-4 LOD within this many 4-cell chunks.
    #[arg(long, default_value_t = 4)]
    lodgen_radius: i32,
    /// Generated terrain: diffuse texels per chunk side (power of two; normals get half).
    #[arg(long, default_value_t = 1024)]
    lodgen_res: usize,
    /// Generated terrain: mesh vertex step in landscape grid vertices (1, 2, 4, 8, 16 or 32).
    #[arg(long, default_value_t = 2)]
    lodgen_step: usize,
    /// Game days passed, for the moon phase (default: the GameDaysPassed global).
    #[arg(long)]
    day: Option<f32>,
    /// Do not apply the image-space grade (the worldspace's image space and the weather's
    /// adapters), for comparisons.
    #[arg(long)]
    no_grade: bool,
    /// Sun shadows reach this far from the camera (game units); 0 turns them off. On by
    /// default (owner decision 2026-10-09), although the original game casts no sun shadows
    /// from the world (docs/spec-notes/world.md §6, "Sun path and shadows").
    #[arg(long, default_value_t = 16_000.0)]
    shadow_distance: f32,
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
    /// The game's field-of-view setting in degrees (fDefaultFOV, default 75): the horizontal
    /// angle at 4:3; other aspects keep its vertical angle, so 16:9 is wider (75 → 91.3°
    /// horizontal) [hypothesis; docs/spec-notes/camera.md].
    #[arg(long, default_value_t = 75.0)]
    fov: f32,
    /// Added to the z of --pos (e.g. the eye height above a captured player position).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    eye_height: f32,
    /// With --screenshot: also write the lab's frame document (fnv.frame/1) here.
    #[arg(long, requires = "screenshot")]
    frame_json: Option<PathBuf>,
    /// Multiplier on every light's brightness (LIGH colour × fade; 1 = as stored).
    #[arg(long, default_value_t = 1.0)]
    light_scale: f32,
    /// Test aid: fly forward at this speed (game units per second).
    #[arg(long, default_value_t = 0.0)]
    auto_fly: f32,
    /// Test aid: game time passes at this many game hours per real second (0: it stands
    /// still). Logs a "time-lapse" sample of the sun and light colours at every whole hour.
    #[arg(long, default_value_t = 0.0)]
    time_scale: f32,
    /// Test aid with --time-scale: also save a screenshot in this folder at every whole hour.
    #[arg(long, requires = "time_scale")]
    timelapse: Option<PathBuf>,
    /// Test aid with --timelapse: game minutes between saved frames (60: one per hour; a
    /// few minutes give frames for a video).
    #[arg(long, default_value_t = 60.0, requires = "timelapse")]
    timelapse_step: f32,
    /// Test aid: exit with success after this many seconds (smoke tests).
    #[arg(long)]
    run_seconds: Option<f32>,
    /// Do not draw the text overlay (implied by --frame-json).
    #[arg(long)]
    no_overlay: bool,
    /// Play: the camera is the player, walking with collision, activating with E
    /// (Layer 4, nvf-play) instead of the free-fly camera.
    #[arg(long)]
    play: bool,
    /// Do not draw NPCs (workstream 5, docs/design/static-actors.md).
    #[arg(long)]
    no_actors: bool,
    /// Draw NPCs with the race's base face (no FaceGen morphs or face textures).
    #[arg(long)]
    no_facegen: bool,
    /// How NPCs are posed: the skeleton's idle clip in a loop (the default), or the
    /// frozen bind pose (docs/spec-notes/actors.md §7).
    #[arg(long, value_enum, default_value_t = actors::ActorPose::Idle)]
    actor_pose: actors::ActorPose,
    /// Test aid with --actor-pose idle: hold every NPC at this clip time (seconds); with
    /// --actor-walk-test, at this time of the walk script (run from its start, repeatable).
    #[arg(long)]
    actor_pose_time: Option<f32>,
    /// Test aid: NPCs walk a fixed script along their forward axis at this speed (units
    /// per second): out, stop, turn, back, and a backward leg (docs/spec-notes/actors.md
    /// §8.4). Needs --actor-pose idle.
    #[arg(long)]
    actor_walk_test: Option<f32>,
    /// How NPCs' weapons are shown: holstered (the default), drawn, or not at all
    /// (docs/spec-notes/actors.md §9). Needs --actor-pose idle.
    #[arg(long, value_enum, default_value_t = weapons::WeaponMode::Holstered)]
    actor_weapons: weapons::WeaponMode,
    /// Test aid: armed NPCs draw and holster their weapon, each state held this many
    /// seconds (docs/spec-notes/actors.md §9.4).
    #[arg(long)]
    actor_weapon_test: Option<f32>,
    /// With --actor-walk-test: only NPCs within this distance of the start position
    /// (game units; 0 = all).
    #[arg(long, default_value_t = 0.0, requires = "actor_walk_test")]
    actor_walk_test_radius: f32,
    /// Test aid with --play: walk forward at this speed (game units per second).
    #[arg(long, default_value_t = 0.0, requires = "play")]
    auto_walk: f32,
}

#[derive(Component)]
struct Overlay;

fn parse_vec<const N: usize>(s: &str) -> Option<[f32; N]> {
    let v: Vec<f32> = s.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    v.try_into().ok()
}

/// The distant LOD distances from the command line (lod.rs).
fn lod_settings(args: &Args) -> lod::LodSettings {
    lod::LodSettings {
        far: args.lod_distance.max(0.0),
        near: args.lod_near_distance.max(0.0),
        split: args.lod_split.max(0.0),
        trees: args.tree_distance.max(0.0),
    }
}

/// The generated-terrain job: parameters, cache folder (NVForever's home, never the game's
/// folder) and shared data (docs/design/generated-lod.md).
fn lodgen_job(args: &Args, world: &WorldData, game: &GameData) -> Result<lodgen::GenJob, String> {
    let (res, step) = (args.lodgen_res, args.lodgen_step);
    if !(16..=4096).contains(&res) || !res.is_power_of_two() {
        return Err(format!("--lodgen-res {res}: expected a power of two from 16 to 4096"));
    }
    if step == 0 || 32 % step != 0 {
        return Err(format!("--lodgen-step {step}: expected 1, 2, 4, 8, 16 or 32"));
    }
    let params = lodgen::GenParams { res, step, tile: args.terrain_tile };
    let world_name = world.index.worldspaces[&world.world].edid.clone().unwrap_or_default();
    let cache = fnv_install::EngineHome::default_root().map(|root| {
        let (key, manifest) = lodgen::cache_key(&params, &world_name, &args.data, &args.plugins);
        let dir = fnv_install::EngineHome::new(root).cache().join("lod").join(world_name.to_ascii_lowercase()).join(key);
        info!("generated LOD cache: {}", dir.display());
        lodgen::GenCache { dir, manifest, game: fnv_install::GameInstall::open(&args.data).ok() }
    });
    if cache.is_none() {
        warn!("no NVForever home folder: generated LOD is not cached");
    }
    Ok(lodgen::GenJob {
        lo: world.lo,
        schema: world.schema,
        index: world.index,
        world: world.world,
        data: game.clone(),
        params,
        cache,
        mips: Default::default(),
    })
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

    // Interior mode when --cell names an interior cell.
    let interior = world.index.cell_by_name(&args.cell).filter(|c| c.world.is_none()).map(|c| c.form_id);
    // Start position (game space).
    let start = match args.pos.as_deref() {
        Some(p) => match parse_vec::<3>(p) {
            Some(v) => Vec3::from_array(v) + Vec3::Z * args.eye_height,
            None => return fail(format!("--pos {p}: expected x,y,z")),
        },
        None if interior.is_some() => {
            // The middle of the cell's references, a little above.
            let ps: Vec<Vec3> = world.index.cells[&interior.unwrap()]
                .refs
                .iter()
                .filter_map(|&r| fnv_world::placed::placement(world.lo, world.schema, r))
                .map(|p| Vec3::from_array(p.position))
                .collect();
            let n = ps.len().max(1) as f32;
            ps.iter().copied().sum::<Vec3>() / n + Vec3::Z * 150.0
        }
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
                .and_then(|l| load_land(world.lo, world.schema, l, LandParts::HEIGHTS))
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
    sky_state.shadow_distance = args.shadow_distance.max(0.0);
    if args.no_grade {
        sky_state.image_space = None;
    }
    sky_state.time_scale = args.time_scale.max(0.0);
    sky_state.days_passed = args.day.or_else(|| sky::find_global(&world, "GameDaysPassed")).unwrap_or(0.0).floor();
    if let Some(dir) = &args.timelapse {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return fail(format!("--timelapse {}: {e}", dir.display()));
        }
    }
    if let Some(id) = interior {
        sky_state.interior = fnv_world::lighting::cell_lighting(world.lo, world.schema, id);
        sky_state.interior_grade = sky::SkyState::cell_grade(world.lo, world.schema, id);
    }
    if let Some(f) = args.fog.as_deref() {
        match parse_vec::<2>(f) {
            Some([n, fa]) => sky_state.fog_override = Some((n, fa)),
            None => return fail(format!("--fog {f}: expected near,far")),
        }
    }
    let Some([w, h]) = args.size.split_once('x').and_then(|(w, h)| Some([w.parse::<u32>().ok()?, h.parse::<u32>().ok()?])) else {
        return fail(format!("--size {}: expected WxH", args.size));
    };

    if let Some(dir) = args.screenshot.as_deref().and_then(|p| p.parent()).filter(|d| !d.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return fail(format!("{}: {e}", dir.display()));
        }
    }
    let play = args.play.then_some(nvf_play::PlayData {
        lo: world.lo,
        schema: world.schema,
        index: world.index,
        world: world.world,
        interior,
        eye: start.to_array(),
        heading_deg: args.heading,
        pitch_deg: args.pitch,
        auto_walk: args.auto_walk,
    });
    // Also for an interior start: LOD stays off inside and is switched on at the door out
    // (world::travel), with generated terrain for the start worldspace.
    let lodgen = if args.no_lod || args.no_lodgen {
        None
    } else {
        match lodgen_job(&args, &world, &game) {
            Ok(job) => Some(lod::GenState::new(job, args.lodgen_radius.max(0))),
            Err(e) => return fail(e),
        }
    };
    let actor_world = (!args.no_actors).then(|| {
        let t0 = std::time::Instant::now();
        let mut a = actors::ActorWorld::build(world.lo, world.schema);
        a.facegen_enabled = !args.no_facegen;
        a.pose = args.actor_pose;
        a.pose_time = args.actor_pose_time;
        a.walk_test = args.actor_walk_test.map(|speed| actors::WalkTestArgs { speed, radius: args.actor_walk_test_radius, center: start });
        a.weapon_mode = args.actor_weapons;
        a.weapon_test = args.actor_weapon_test;
        if a.walk_test.is_some() && a.pose == actors::ActorPose::Bind {
            warn!("--actor-walk-test needs --actor-pose idle; actors stay in the bind pose");
        }
        info!("actor records loaded in {:.2}s", t0.elapsed().as_secs_f64());
        a
    });
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
                // Scale factor 1 so screenshots are exactly --size pixels on any display.
                primary_window: Some(Window {
                    title: "NVForever".into(),
                    resolution: bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins((ArchivePlugin(game), lights::LightsPlugin, emittance::EmittancePlugin, water::WaterPlugin, tree::TreePlugin, reflection::ReflectionPlugin, ripples::RipplesPlugin))
    .add_plugins((imagespace::ImageSpacePlugin, sky::SkyPlugin))
    .insert_resource(ClearColor(Color::srgb(0.62, 0.72, 0.85)))
    .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
    .insert_resource(sky_state)
    .insert_resource(fly::AutoFly(args.auto_fly))
    .insert_resource(lod::LodState::new(&world, lod_settings(&args), !args.no_lod && interior.is_none(), lodgen))
    .insert_resource(TerrainSettings { tile: args.terrain_tile })
    .init_resource::<TerrainCache>()
    .insert_resource(Streamer {
        radius: args.radius,
        loaded: Default::default(),
        fades: Default::default(),
        fade_secs: args.cell_fade.max(0.0),
        settled: false,
        per_frame: 1,
        hide: args.hide.iter().map(|h| h.to_ascii_lowercase()).collect(),
        interior,
        light_scale: args.light_scale,
        stats: Default::default(),
    })
    .insert_resource(world)
    .init_resource::<nvf_assets::actor::ActorParts>()
    .insert_resource(StartPose {
        position: start,
        heading: args.heading,
        pitch: args.pitch,
        fov: args.fov,
        overlay: !args.no_overlay && args.frame_json.is_none(),
        play: play.is_some(),
    })
    .insert_resource(args.clone())
    .add_systems(Startup, (world::setup_root, setup, sky::setup_sky))
    .add_systems(Update, (fly::fly.run_if(move || play.is_none()), sky::sky_keys, sky::update_sky, sky::update_clouds, sky::update_night_sky, world::stream, fade::fade_cells, world::face_billboards, lod::update_lod, treelod::update_distant_trees, overlay, keys).chain())
    .init_resource::<AnimCost>()
    .add_systems(Update, advance_animations)
    .init_resource::<locomotion::ActorAnimCost>()
    .add_systems(Update, (locomotion::walk_test, locomotion::drive, weapons::reparent).chain())
    .add_systems(Update, timelapse.after(sky::update_sky));
    if let Some(data) = play {
        // Travel reloads cells before streaming runs again; the play systems run in
        // nvf-play's own sets.
        // Streaming follows the camera, which the player systems move: stream after them.
        app.add_plugins(nvf_play::PlayPlugin(data))
            .add_systems(Update, (worldspace::switch, world::travel).chain().before(world::stream))
            .configure_sets(Update, nvf_play::PlaySet::Physics.before(world::stream));
    }
    if let Some(a) = actor_world {
        app.insert_resource(a);
    }
    if args.screenshot.is_some() {
        app.add_systems(Update, screenshot_and_exit);
    }
    if let Some(secs) = args.run_seconds {
        #[allow(clippy::too_many_arguments)]
        let summary = move |time: Res<Time>,
                            streamer: Res<Streamer>,
                            lod: Res<lod::LodState>,
                            actors: Option<Res<actors::ActorWorld>>,
                            anim: Res<AnimCost>,
                            actor_anim: Res<locomotion::ActorAnimCost>,
                            mut commands: Commands,
                            mut t: Local<(f32, u32)>| {
            t.0 += time.delta_secs();
            t.1 += 1;
            if t.0 >= secs {
                let (baked, cached) = lod.gen.as_ref().map_or((0, 0), |g| (g.baked, g.cached));
                info!(
                    "run finished after {:.1}s: {} cells loaded in total, {} loaded now, {} frames ({:.1} per second), generated LOD chunks {} baked, {} from the cache, actors {:?}",
                    t.0, streamer.stats.cells_loaded, streamer.loaded.len(), t.1, t.1 as f32 / t.0, baked, cached, streamer.stats.actors
                );
                info!("distant LOD at the end: {}", lod.describe());
                info!(
                    "animation: up to {} clips playing at once, {:.3} ms per frame on average, {:.2} us per clip and frame",
                    anim.peak,
                    anim.total.as_secs_f64() * 1e3 / f64::from(anim.frames.max(1)),
                    anim.total.as_secs_f64() * 1e6 / anim.clip_frames.max(1) as f64
                );
                let c = &*actor_anim;
                if c.actor_frames == 0 {
                    info!("actor animation: no actors posed");
                } else {
                    info!(
                    "actor animation: up to {} actors posed at once, {:.3} ms per frame on average, {:.2} us per actor and frame, {:.2} clips per actor and frame ({:.1}% of actor frames with more than one clip), {:.1} tracks per actor and frame",
                    c.peak,
                    c.total.as_secs_f64() * 1e3 / f64::from(c.frames.max(1)),
                    c.total.as_secs_f64() * 1e6 / c.actor_frames.max(1) as f64,
                    c.layer_frames as f64 / c.actor_frames.max(1) as f64,
                    100.0 * c.blend_frames as f64 / c.actor_frames.max(1) as f64,
                    c.tracks as f64 / c.actor_frames as f64
                    );
                }
                if let Some(a) = actors {
                    let fg = a.facegen.lock().expect("facegen cache");
                    info!("FaceGen: {} shapes morphed, {} faces textured, {} EGMs not matching their shape", fg.morphed_shapes, fg.faces_textured, fg.mismatched);
                }
                commands.write_message(AppExit::Success);
            }
        };
        app.add_systems(Update, summary);
    }
    app.run()
}

#[derive(Resource)]
struct StartPose {
    position: Vec3,
    heading: f32,
    pitch: f32,
    /// The game's field-of-view setting in degrees (see `vertical_fov`).
    fov: f32,
    overlay: bool,
    /// The camera is the player (`--play`).
    play: bool,
}

/// Vertical field of view (radians) for the game's FOV setting (degrees): the setting is the
/// horizontal angle at a 4:3 aspect and the vertical angle stays fixed at every other aspect
/// ("Hor+"), so Bevy's vertical `fov` needs no update on resize [hypothesis; matched at 16:9
/// on four reference frames, docs/spec-notes/camera.md].
fn vertical_fov(setting_deg: f32) -> f32 {
    2.0 * ((setting_deg.to_radians() / 2.0).tan() * 3.0 / 4.0).atan()
}

fn setup(mut commands: Commands, start: Res<StartPose>) {
    let pos = game_to_bevy().transform_point(start.position);
    let (yaw, pitch) = (-start.heading.to_radians(), start.pitch.to_radians());
    let cam = commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: vertical_fov(start.fov),
            far: 400_000.0,
            near: 5.0,
            ..default()
        }),
        Transform::from_translation(pos).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
        FlyCam { yaw, pitch, speed: 800.0 },
        imagespace::ImageSpaceGrade::default(),
        sky::initial_fog(),
        // Water reads the depth of what lies behind it (water.rs).
        bevy::core_pipeline::prepass::DepthPrepass,
        // The world and the water (which the reflection camera leaves out).
        bevy::camera::visibility::RenderLayers::from_layers(&[0, reflection::WATER_LAYER]),
    )).id();
    if start.play {
        commands.entity(cam).insert(nvf_play::PlayerCamera);
    }
    if !start.overlay {
        return;
    }
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
        "pos {:.0}, {:.0}, {:.0}  cell {},{}\ncells {} ({} loads, {:.0} ms total)  refs drawn {} (animated {})  disabled {}  not drawn {}  speedtree {}  no model {}  shapes {}\nmodels {} (failed {})  textures failed {}  terrain layers {}  quadrants without base {}  water {}  {}  lights {}\n{}\nWASD/QE move  Shift fast  right-drag look  wheel speed  [ ] hour  F12 screenshot  Esc quit",
        g.x, g.y, g.z, c.0, c.1,
        streamer.loaded.len(), s.cells_loaded, s.load_ms,
        s.refs_drawn, s.refs_animated, s.refs_skipped_disabled, s.refs_skipped_type, s.refs_speedtree, s.refs_no_model, s.shapes,
        models.len(), models.failures.len(), textures.failures.len(), terrain.layers, terrain.quadrants_without_base,
        s.water_planes, lod.describe(), s.lights, sky.describe(),
    );
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    play_state: Option<Res<State<nvf_play::PlayState>>>,
    mut commands: Commands,
    mut n: Local<u32>,
) {
    // With --play, Esc in a menu closes the menu (nvf-play) instead of quitting.
    let in_menu = play_state.is_some_and(|s| *s.get() == nvf_play::PlayState::Menu);
    if input.just_pressed(KeyCode::Escape) && !in_menu {
        commands.write_message(AppExit::Success);
    }
    if input.just_pressed(KeyCode::F12) {
        *n += 1;
        let path = format!("nvforever-{:03}.png", *n);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// The lab's frame document: where the image is and how it was made.
fn write_frame_json(out: &std::path::Path, image: &std::path::Path, args: &Args, sky: &sky::SkyState) -> std::io::Result<()> {
    let abs = std::path::absolute(image)?;
    let doc = serde_json::json!({
        "schema": "fnv.frame/1",
        "image": abs.to_string_lossy(),
        "size": args.size,
        "camera": {"pos": args.pos, "eye_height": args.eye_height, "heading": args.heading, "pitch": args.pitch, "fov": args.fov},
        "world": args.world,
        "hour": sky.hour,
        "weather": sky.weather.edid,
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_vec_pretty(&doc)?)
}

/// With --time-scale: the clock starts once the cells around the start are loaded; then
/// one log line per whole game hour, and with --timelapse a screenshot every
/// --timelapse-step game minutes.
#[allow(clippy::too_many_arguments)]
fn timelapse(
    mut last: Local<Option<i32>>,
    mut seen: Local<u32>,
    mut last_slot: Local<Option<i64>>,
    mut frames: Local<u32>,
    mut sky: ResMut<sky::SkyState>,
    streamer: Res<Streamer>,
    args: Res<Args>,
    mut commands: Commands,
) {
    if sky.time_scale <= 0.0 || sky.interior.is_some() {
        return;
    }
    if !sky.clock_running {
        let side = (2 * streamer.radius + 1) as usize;
        if streamer.loaded.len() >= side * side {
            sky.clock_running = true;
            info!("time-lapse starts: {}", sky.sample());
        }
        return;
    }
    if let Some(dir) = &args.timelapse {
        // Slots of --timelapse-step minutes from midnight; the hour wraps at 24, so a change
        // of slot (not an increase) marks a new frame. Waits for the first slot boundary.
        let step = args.timelapse_step.max(0.5) as f64;
        let slot = (sky.hour as f64 * 60.0 / step).floor() as i64;
        if *last_slot != Some(slot) {
            if last_slot.is_some() {
                *frames += 1;
                let minutes = (slot as f64 * step).round() as i64;
                let name = format!("{:04}_{:02}{:02}.png", *frames, minutes / 60 % 24, minutes % 60);
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join(name)));
            }
            *last_slot = Some(slot);
        }
    }
    let hour = sky.hour.floor() as i32;
    if *last == Some(hour) {
        return;
    }
    let first = last.is_none();
    *last = Some(hour);
    if first {
        return; // Wait for the first whole hour.
    }
    *seen += 1;
    info!("time-lapse {}  ({} hours passed)", sky.sample(), *seen);
}

/// Time spent advancing single clips (animated models; actors are timed by
/// `locomotion::ActorAnimCost`), for the `--run-seconds` summary.
#[derive(Resource, Default)]
struct AnimCost {
    total: std::time::Duration,
    frames: u32,
    /// Clips advanced, summed over frames, and the most at once.
    clip_frames: u64,
    peak: usize,
}

fn advance_animations(
    time: Res<Time>,
    playing: Query<&mut nvf_assets::anim_play::Playing>,
    transforms: Query<&mut Transform>,
    mut cost: ResMut<AnimCost>,
) {
    let t0 = std::time::Instant::now();
    let n = playing.iter().len();
    nvf_assets::anim_play::advance_instances(time, playing, transforms);
    cost.total += t0.elapsed();
    cost.frames += 1;
    cost.clip_frames += n as u64;
    cost.peak = cost.peak.max(n);
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
    sky: Res<sky::SkyState>,
    lod: Res<lod::LodState>,
    mut commands: Commands,
) {
    *frames += 1;
    *elapsed += time.delta_secs();
    let side = (2 * streamer.radius + 1) as usize;
    let want = if streamer.interior.is_some() { 1 } else { side * side };
    if ready_at.is_none() && streamer.loaded.len() >= want && lod.gen_pending() == 0 {
        *ready_at = Some(*elapsed);
        let s = &streamer.stats;
        info!(
            "cells loaded in {:.2}s ({:.0} ms loading): refs drawn {} (animated {}), disabled {}, not drawn {} {:?}, no model {}, shapes {}",
            *elapsed, s.load_ms, s.refs_drawn, s.refs_animated, s.refs_skipped_disabled, s.refs_skipped_type, s.skipped_types, s.refs_no_model, s.shapes
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
            if let Some(fj) = &args.frame_json {
                if let Err(e) = write_frame_json(fj, path, &args, &sky) {
                    error!("frame json {}: {e}", fj.display());
                }
            }
        }
        *frames = 10_000;
        *ready_at = Some(*elapsed);
    } else if *frames >= 10_000 && *elapsed >= r + 3.0 {
        commands.write_message(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::vertical_fov;

    #[test]
    fn fov_setting_is_horizontal_at_4_3() {
        let v = vertical_fov(75.0);
        let horizontal = |aspect: f32| 2.0 * ((v / 2.0).tan() * aspect).atan().to_degrees();
        assert!((horizontal(4.0 / 3.0) - 75.0).abs() < 1e-3);
        assert!((v.to_degrees() - 59.84).abs() < 0.01);
        // The 16:9 angle the reference frames matched (docs/spec-notes/camera.md).
        assert!((horizontal(16.0 / 9.0) - 91.31).abs() < 0.01);
    }
}
