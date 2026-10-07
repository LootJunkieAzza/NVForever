//! nvf-viewer — Layer 2 asset viewer for NVForever.
//!
//! Renders any mesh from the user's own game data with its textures and plays KF
//! animations (embedded sequences, or a `.kf` given with `--anim`). Read-only: game files
//! are read through `fnv-vfs`; screenshots go to the path given or the current folder.
//!
//! Keys: N / P next / previous mesh (PageDown / PageUp: ±10), A next animation, Space
//! pause, H show hidden objects, F frame, F12 screenshot, Esc quit. Mouse: left drag
//! orbit, right drag pan, wheel zoom.

mod camera;

use nvf_assets::anim_play::{self, AnimState};
use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use camera::Orbit;
use clap::Parser;
use nvf_assets::{texture, ArchivePlugin, GameData};
use nvf_assets::{NifHidden, NifRoot, SpawnCtx, SpawnReport};
use std::path::PathBuf;

/// NVForever asset viewer (meshes, textures, animations from your own game data).
#[derive(Parser, Clone, Resource)]
#[command(name = "nvf-viewer", version)]
struct Args {
    /// The game's Data folder (read-only). Defaults to $NVFOREVER_DATA.
    #[arg(long, env = "NVFOREVER_DATA")]
    data: PathBuf,
    /// NIF schema directory.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/nif"))]
    schema: PathBuf,
    /// Mesh to open first (path inside the archives, e.g. meshes\clutter\x.nif).
    #[arg(long)]
    mesh: Option<String>,
    /// Also offer the sequences of this .kf file (e.g. an idle for a skeleton mesh).
    #[arg(long)]
    anim: Option<String>,
    /// Skeleton for skinned meshes. Default: `skeleton.nif` in the mesh's folder, or the
    /// nearest parent folder that has one (e.g. characters\_male for body parts).
    #[arg(long)]
    skeleton: Option<String>,
    /// Only browse meshes whose path contains this text.
    #[arg(long)]
    filter: Option<String>,
    /// Render the first mesh, save a PNG here after a few frames, and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Seconds to wait (and play animation) before a --screenshot is taken (at least 3).
    #[arg(long, default_value_t = 3.0)]
    screenshot_at: f32,
}

#[derive(Resource)]
struct Browser {
    meshes: Vec<String>,
    index: usize,
    load_requested: bool,
    report: Option<SpawnReport>,
    error: Option<String>,
    show_hidden: bool,
}

#[derive(Component)]
struct Overlay;

fn main() -> AppExit {
    let args = Args::parse();
    let game = match GameData::open(&args.data, &args.schema) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            return AppExit::error();
        }
    };
    let filter = args.filter.as_deref().map(fnv_vfs::normalize);
    let meshes = game.vfs.paths(|p| {
        p.starts_with("meshes\\") && p.ends_with(".nif") && filter.as_ref().is_none_or(|f| p.contains(f.as_str()))
    });
    if meshes.is_empty() {
        eprintln!("error: no meshes match");
        return AppExit::error();
    }
    let index = args
        .mesh
        .as_deref()
        .map(fnv_vfs::normalize)
        .and_then(|m| meshes.iter().position(|p| *p == m))
        .unwrap_or(0);
    let headless_shot = args.screenshot.is_some();

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
                primary_window: Some(Window {
                    title: "NVForever viewer".into(),
                    resolution: (1280, 800).into(),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(ArchivePlugin(game))
    .insert_resource(args)
    .insert_resource(ClearColor(Color::srgb(0.18, 0.19, 0.21)))
    .insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() })
    .insert_resource(Browser { meshes, index, load_requested: true, report: None, error: None, show_hidden: false })
    .init_resource::<AnimState>()
    .add_systems(Startup, setup)
    .add_systems(Update, (keys, load_mesh, anim_play::advance, camera::orbit_input, overlay).chain());
    if headless_shot {
        app.add_systems(Update, screenshot_and_exit);
    }
    app.run()
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { far: 200_000.0, near: 1.0, ..default() }),
        Transform::from_xyz(0.0, 100.0, 300.0),
        Orbit { target: Vec3::ZERO, distance: 300.0, yaw: 0.6, pitch: -0.35 },
    ));
    commands.spawn((
        DirectionalLight { illuminance: 6000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 0.8, -0.9, 0.0)),
    ));
    commands.spawn((
        Text::new(""),
        Node { position_type: PositionType::Absolute, top: Val::Px(8.0), left: Val::Px(8.0), ..default() },
        Overlay,
    ));
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    mut browser: ResMut<Browser>,
    mut anim: ResMut<AnimState>,
    mut hidden: Query<&mut Visibility, With<NifHidden>>,
    mut cams: Query<&mut Orbit>,
    mut commands: Commands,
    mut counter: Local<u32>,
) {
    let n = browser.meshes.len();
    let step = |d: isize, b: &mut Browser| {
        b.index = (b.index as isize + d).rem_euclid(n as isize) as usize;
        b.load_requested = true;
    };
    if input.just_pressed(KeyCode::KeyN) {
        step(1, &mut browser);
    }
    if input.just_pressed(KeyCode::KeyP) {
        step(-1, &mut browser);
    }
    if input.just_pressed(KeyCode::PageDown) {
        step(10, &mut browser);
    }
    if input.just_pressed(KeyCode::PageUp) {
        step(-10, &mut browser);
    }
    if input.just_pressed(KeyCode::Space) {
        anim.paused = !anim.paused;
    }
    if input.just_pressed(KeyCode::KeyA) && !anim.available.is_empty() {
        let next = anim.current.map_or(0, |c| (c + 1) % anim.available.len());
        if let Some(nodes) = browser.report.as_ref().map(|r| r.nodes.clone()) {
            anim.start(next, &nodes);
        }
    }
    if input.just_pressed(KeyCode::KeyH) {
        browser.show_hidden = !browser.show_hidden;
        for mut v in &mut hidden {
            *v = if browser.show_hidden { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    if input.just_pressed(KeyCode::KeyF) {
        if let Some((lo, hi)) = browser.report.as_ref().and_then(|r| r.bounds) {
            for mut o in &mut cams {
                o.frame(lo, hi);
            }
        }
    }
    if input.just_pressed(KeyCode::F12) {
        let path = format!("nvf-screenshot-{}.png", *counter);
        *counter += 1;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if input.just_pressed(KeyCode::Escape) {
        commands.write_message(AppExit::Success);
    }
}

#[allow(clippy::too_many_arguments)]
fn load_mesh(
    mut browser: ResMut<Browser>,
    mut ctx: SpawnCtx,
    roots: Query<Entity, With<NifRoot>>,
    mut anim: ResMut<AnimState>,
    args: Res<Args>,
    mut cams: Query<&mut Orbit>,
) {
    if !browser.load_requested {
        return;
    }
    browser.load_requested = false;
    for r in &roots {
        ctx.commands.entity(r).despawn();
    }
    *anim = AnimState::default();
    let path = browser.meshes[browser.index].clone();
    let skeleton = args.skeleton.as_deref().map(fnv_vfs::normalize).or_else(|| find_skeleton(&ctx.data, &path));
    match ctx.spawn_nif(&path, skeleton.as_deref().filter(|s| *s != path)) {
        Ok(report) => {
            if let Some((lo, hi)) = report.bounds {
                for mut o in &mut cams {
                    o.frame(lo, hi);
                }
            }
            // Sequences embedded in the mesh, then those of the --anim file.
            let schema = ctx.data.schema.clone();
            let mut sources = vec![path.clone()];
            sources.extend(args.anim.clone());
            for src in sources {
                let Some(bytes) = ctx.data.read(&src) else { continue };
                let Ok(f) = fnv_nif::NifFile::parse(&bytes, Some(&schema)) else { continue };
                let dec = fnv_nif::scene::Decoded::new(&f, &bytes, &schema);
                for s in fnv_nif::anim::sequences(&f, &dec) {
                    anim.available.push((src.clone(), s));
                }
            }
            if !anim.available.is_empty() {
                anim.start(0, &report.nodes);
            }
            browser.report = Some(report);
            browser.error = None;
        }
        Err(e) => {
            browser.report = None;
            browser.error = Some(e);
        }
    }
}

/// The viewer's skeleton guess for a mesh: `skeleton.nif` in its folder or the nearest
/// parent folder below `meshes\` [hypothesis: in the game, the creature or NPC record
/// names the skeleton; this only serves browsing]. Only used when the mesh is skinned.
fn find_skeleton(data: &GameData, mesh: &str) -> Option<String> {
    let bytes = data.read(mesh)?;
    if !bytes.windows(14).any(|w| w == b"NiSkinInstance") && !bytes.windows(23).any(|w| w == b"BSDismemberSkinInstance") {
        return None;
    }
    let mut dir = mesh.rsplit_once('\\')?.0.to_string();
    loop {
        let candidate = format!("{dir}\\skeleton.nif");
        if data.vfs.exists(&candidate) {
            return Some(candidate);
        }
        if dir.ends_with("characters") || !dir.contains('\\') {
            break;
        }
        dir = dir.rsplit_once('\\')?.0.to_string();
    }
    let male = r"meshes\characters\_male\skeleton.nif";
    (mesh.starts_with(r"meshes\characters\") && data.vfs.exists(male)).then(|| male.to_string())
}

fn overlay(browser: Res<Browser>, anim: Res<AnimState>, textures: Res<texture::TextureCache>, mut text: Query<&mut Text, With<Overlay>>) {
    let Ok(mut t) = text.single_mut() else { return };
    let mut s = format!("[{}/{}] {}\n", browser.index + 1, browser.meshes.len(), browser.meshes[browser.index]);
    if let Some(r) = &browser.report {
        s += &format!(
            "blocks {}  shapes {}  skinned {}  triangles {}  missing textures {}\n",
            r.blocks,
            r.shapes,
            r.skinned,
            r.triangles,
            r.textures_missing.len()
        );
    }
    if let Some(e) = &browser.error {
        s += &format!("error: {e}\n");
    }
    match &anim.playing {
        Some(p) => {
            s += &format!(
                "animation {}/{}: {} ({})  t={:.2} [{:.2}..{:.2}]  tracks {} (unmatched {}){}\n",
                anim.current.map_or(0, |c| c + 1),
                anim.available.len(),
                p.name,
                p.source,
                anim_play::cycle_time(p.time, p.start, p.stop, p.cycle_type),
                p.start,
                p.stop,
                p.tracks.len(),
                p.unmatched,
                if anim.paused { "  paused" } else { "" }
            );
        }
        None => s += "no animation\n",
    }
    if !textures.failures.is_empty() {
        s += &format!("texture load failures so far: {}\n", textures.failures.len());
    }
    s += "N/P next/prev  PgDn/PgUp +/-10  A animation  Space pause  H hidden  F frame  F12 screenshot  Esc quit";
    t.0 = s;
}

/// --screenshot: give textures time to upload (and the animation time to run), save, exit.
fn screenshot_and_exit(
    mut frames: Local<u32>,
    mut elapsed: Local<f32>,
    time: Res<Time>,
    args: Res<Args>,
    browser: Res<Browser>,
    textures: Res<texture::TextureCache>,
    mut commands: Commands,
) {
    *frames += 1;
    *elapsed += time.delta_secs();
    // Pipelines compile during the first second or two; nothing is drawn before that.
    let wait_done = *frames >= 20 && *elapsed >= args.screenshot_at.max(3.0);
    if wait_done && *frames < 10_000 {
        if let Some(path) = &args.screenshot {
            if let Some(e) = &browser.error {
                eprintln!("error: {e}");
            }
            for f in &textures.failures {
                warn!("texture: {f}");
            }
            info!("taking screenshot {} after {} frames, {:.2}s", path.display(), *frames, *elapsed);
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
        }
        *frames = 10_000;
    } else if *frames >= 10_000 && *elapsed >= args.screenshot_at.max(3.0) + 3.0 {
        commands.write_message(AppExit::Success);
    }
}
