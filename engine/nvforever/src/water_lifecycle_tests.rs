//! Authored water records through the real plane producer and camera fog plugin.
use super::*;
use crate::authored_test_records::{record, sub, tes4, top, z};
use crate::fly::FlyCam;
use crate::world::{game_to_bevy, ReadAhead, StreamStats};
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::pbr::DistanceFog;
use fnv_esm::{LoadOrder, Plugin as EsmPlugin, Schema};
use fnv_world::index::{Cell, Worldspace};
use nvf_assets::{GameData, ModelCache, NifMaterial, TextureCache};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const WATER: u32 = 0x800;

struct EmptyData(PathBuf);
impl Drop for EmptyData {
    fn drop(&mut self) {
        std::fs::remove_dir(&self.0).unwrap();
    }
}

fn setup() -> (App, Entity, EmptyData) {
    // WaterVisual's documented record fields, as decoded by schemas/fnv/sky.json.
    // No image files or installed records are needed by this authored water type.
    let mut visual = vec![0; 196];
    visual[44..48].copy_from_slice(&[20, 170, 210, 0]);
    visual[144..148].copy_from_slice(&7_f32.to_le_bytes());
    visual[148..152].copy_from_slice(&321_f32.to_le_bytes());
    let bytes = [
        tes4(),
        top(
            b"WATR",
            &record(
                b"WATR",
                0,
                WATER,
                &[
                    sub(b"EDID", z("AuthoredLifecycleWater")),
                    sub(b"DNAM", visual),
                ],
            ),
        ),
    ]
    .concat();
    let plugins = Box::leak(
        vec![EsmPlugin::parse(&bytes, "AuthoredLifecycle.esm".into()).unwrap()].into_boxed_slice(),
    );
    let lo = Box::leak(Box::new(LoadOrder::new(plugins).unwrap()));
    let schemas = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Box::leak(Box::new(Schema::load_dir(schemas.join("fnv")).unwrap()));
    let decoded = fnv_world::sky::water(lo, schema, WATER).unwrap();
    assert_eq!(decoded.deep, [20, 170, 210]);
    assert_eq!(decoded.under_fog, [7.0, 321.0, 0.0]);
    let mut index = fnv_world::WorldIndex::default();
    let mut ws = Worldspace {
        form_id: 1,
        water: Some(WATER),
        ..Default::default()
    };
    for (n, grid) in [(-1, 0), (0, 0), (1, 0), (4, 0)].into_iter().enumerate() {
        let id = 0x900 + n as u32;
        ws.grid.insert(grid, id);
        index.cells.insert(
            id,
            Cell {
                form_id: id,
                world: Some(1),
                grid: Some(grid),
                data_flags: 2,
                water_height: Some(100.0),
                water_type: Some(WATER),
                ..Default::default()
            },
        );
    }
    index.worldspaces.insert(1, ws);
    let world = WorldData {
        lo,
        schema,
        index: Box::leak(Box::new(index)),
        world: 1,
        persistent_by_grid: HashMap::new(),
        imposters: Box::leak(Box::new(fnv_world::imposter::ImposterRegions::build(
            lo, schema,
        ))),
        weather_regions: &[],
        dropped_plugins: Vec::new(),
    };
    let mut lod = LodState::new(&world, crate::lod::LodSettings::default(), true, None);
    lod.radius = 0;
    let streamer = Streamer {
        radius: 1,
        loaded: HashMap::new(),
        fades: HashMap::new(),
        fade_secs: 0.0,
        settled: true,
        arrival: None,
        per_frame: 1,
        spawning: HashMap::new(),
        dying: Vec::new(),
        ahead: ReadAhead::default(),
        hide: Vec::new(),
        interior: None,
        light_scale: 1.0,
        managed: HashSet::new(),
        manage_actors: false,
        stats: StreamStats::default(),
        buffer: Default::default(),
        centre: None,
        waiting: Vec::new(),
        model_overrides: HashMap::new(),
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = EmptyData(std::env::temp_dir().join(format!(
        "nvf-water-lifecycle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir(&dir.0).unwrap();
    let data = GameData::open_for_plugins(&dir.0, &[] as &[&str], &schemas.join("nif")).unwrap();
    let mut a = App::new();
    a.add_plugins(TransformPlugin)
        .insert_resource(world)
        .insert_resource(lod)
        .insert_resource(streamer)
        .insert_resource(data)
        .init_resource::<DistantWater>()
        .init_resource::<WaterCache>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<NifMaterial>>()
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<WaterMaterial>>()
        .init_resource::<Assets<ShaderBuffer>>()
        .init_resource::<Assets<SkinnedMeshInverseBindposes>>()
        .init_resource::<ModelCache>()
        .init_resource::<TextureCache>();
    a.world_mut().spawn((WorldRoot, game_to_bevy()));
    let camera = a
        .world_mut()
        .spawn((
            Camera3d::default(),
            FlyCam {
                yaw: 0.0,
                pitch: 0.0,
                speed: 0.0,
            },
            Transform::from_xyz(1024.0, 200.0, -1024.0),
            DistanceFog::default(),
        ))
        .id();
    (a, camera, dir)
}

fn local_plane(a: &mut App, grid: (i32, i32)) -> Entity {
    let cache = a.world().resource::<WaterCache>();
    let mesh = cache.plane.clone().unwrap();
    let mat = cache.materials[&(WATER, true, false, FlowDirection::STILL)].clone();
    let e = a
        .world_mut()
        .spawn((
            Name::new("Authored loaded cell water"),
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_xyz(
                grid.0 as f32 * CELL_SIZE + CELL_SIZE / 2.0,
                grid.1 as f32 * CELL_SIZE + CELL_SIZE / 2.0,
                100.0,
            ),
        ))
        .id();
    let root = {
        let w = a.world_mut();
        let mut roots = w.query_filtered::<Entity, With<WorldRoot>>();
        roots.single(w).unwrap()
    };
    a.world_mut().entity_mut(root).add_child(e);
    a.world_mut()
        .resource_mut::<Streamer>()
        .loaded
        .insert(grid, e);
    e
}

/// Advance `distant_water` until `main`'s budgeted surface reads (#2956) finish
/// populating the initial or newly expanded LOD square (`built_for.is_some()`).
fn settle_distant_water(a: &mut App) {
    for _ in 0..32 {
        a.update();
        if a.world().resource::<DistantWater>().built_for.is_some() {
            return;
        }
    }
    panic!("distant_water did not finish budgeted surface reads within 32 frames");
}

#[test]
fn equal_count_stream_replacement_removes_coplanar_distant_owner_and_retains_neighbors() {
    let (mut a, _, _dir) = setup();
    a.add_systems(Update, distant_water);
    settle_distant_water(&mut a);
    let local0 = local_plane(&mut a, (0, 0));
    a.update();
    let (old1, neighbor, material) = {
        let dw = a.world().resource::<DistantWater>();
        assert!(!dw.planes.contains_key(&(0, 0)));
        let neighbor = dw.planes[&(-1, 0)];
        (
            dw.planes[&(1, 0)],
            neighbor,
            a.world()
                .get::<MeshMaterial3d<WaterMaterial>>(neighbor)
                .unwrap()
                .0
                .clone(),
        )
    };
    a.world_mut()
        .resource_mut::<Streamer>()
        .loaded
        .remove(&(0, 0));
    a.world_mut().despawn(local0);
    let local1 = local_plane(&mut a, (1, 0));
    a.update();
    let dw = a.world().resource::<DistantWater>();
    assert!(
        !dw.planes.contains_key(&(1, 0)),
        "loaded replacement must evict its distant water even when loaded count is unchanged"
    );
    assert!(
        a.world().get_entity(old1).is_err(),
        "the old coplanar mesh must actually be despawned"
    );
    assert!(a.world().get::<Mesh3d>(local1).is_some());
    assert!(
        dw.planes.contains_key(&(0, 0)),
        "the unloaded water cell must regain distant coverage"
    );
    assert_eq!(dw.planes[&(-1, 0)], neighbor);
    assert_eq!(
        a.world()
            .get::<MeshMaterial3d<WaterMaterial>>(neighbor)
            .unwrap()
            .0,
        material
    );
    let before: HashMap<_, _> = dw.planes.clone();
    for _ in 0..8 {
        a.update();
    }
    assert_eq!(a.world().resource::<DistantWater>().planes, before);
    assert_eq!(a.world().resource::<Assets<WaterMaterial>>().len(), 1);
    assert_eq!(a.world().resource::<Assets<Mesh>>().len(), 1);
}

#[test]
fn lod_and_interior_changes_reconcile_real_planes_without_camera_or_count_changes() {
    let (mut a, _, _dir) = setup();
    a.add_systems(Update, distant_water);
    settle_distant_water(&mut a);
    let old: Vec<_> = a
        .world()
        .resource::<DistantWater>()
        .planes
        .values()
        .copied()
        .collect();
    assert_eq!(old.len(), 3);
    assert!(!a
        .world()
        .resource::<DistantWater>()
        .planes
        .contains_key(&(4, 0)));
    a.world_mut().resource_mut::<LodState>().radius = 1;
    settle_distant_water(&mut a);
    assert!(
        a.world()
            .resource::<DistantWater>()
            .planes
            .contains_key(&(4, 0)),
        "changed LOD radius must update actual water coverage"
    );
    a.world_mut().resource_mut::<LodState>().enabled = false;
    a.update();
    assert!(
        a.world().resource::<DistantWater>().planes.is_empty(),
        "LOD off must remove old water meshes at the unchanged camera"
    );
    assert!(old.iter().all(|e| a.world().get_entity(*e).is_err()));
    a.world_mut().resource_mut::<LodState>().enabled = true;
    a.update();
    assert_eq!(a.world().resource::<DistantWater>().planes.len(), 4);
    a.world_mut().resource_mut::<Streamer>().interior = Some(0x999);
    a.update();
    assert!(a.world().resource::<DistantWater>().planes.is_empty());
    a.world_mut().resource_mut::<Streamer>().interior = None;
    a.update();
    assert_eq!(a.world().resource::<DistantWater>().planes.len(), 4);
}

fn fog_setup() -> (App, Entity, EmptyData) {
    let (mut a, camera, dir) = setup();
    let mut sky = crate::sky::authored_session_sky();
    sky.fog_override = Some((17.0, 4500.0));
    a.insert_resource(sky)
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 1.0,
            ..Default::default()
        })
        .add_plugins(crate::underwater::UnderwaterFogPlugin);
    (a, camera, dir)
}

fn assert_falloff(actual: &DistanceFog, expected: &DistanceFog) {
    assert_eq!(actual.color, expected.color);
    // FogFalloff's variants need not implement PartialEq; compare their complete public representation.
    assert_eq!(
        format!("{:?}", actual.falloff),
        format!("{:?}", expected.falloff)
    );
}

#[test]
fn current_camera_enter_exit_and_reentry_restore_current_weather_without_refresh_delay() {
    let (mut a, camera, _dir) = fog_setup();
    a.update();
    a.world_mut()
        .get_mut::<Transform>(camera)
        .unwrap()
        .translation
        .y = 50.0;
    a.update();
    let under = a.world().get::<DistanceFog>(camera).unwrap();
    assert!(
        matches!(
            under.falloff,
            bevy::pbr::FogFalloff::Linear {
                start: 7.0,
                end: 321.0
            }
        ),
        "current submerged Transform must select water fog in this frame"
    );
    assert!(under.color.to_linear().blue > under.color.to_linear().red);
    a.world_mut().resource_mut::<SkyState>().fog_override = Some((91.0, 12345.0));
    let sky = a.world().resource::<SkyState>();
    let rgb = sky.color(fnv_world::sky::ColorType::Fog);
    let expected = DistanceFog {
        color: Color::linear_rgba(rgb[0], rgb[1], rgb[2], 1.0),
        falloff: crate::fog::falloff(91.0, 12345.0, sky.fog_power(), true, sky.emissive_scale()),
        ..Default::default()
    };
    a.world_mut()
        .get_mut::<Transform>(camera)
        .unwrap()
        .translation
        .y = 200.0;
    a.update();
    assert_falloff(a.world().get::<DistanceFog>(camera).unwrap(), &expected);
    for _ in 0..4 {
        a.update();
    }
    assert_falloff(a.world().get::<DistanceFog>(camera).unwrap(), &expected);
    a.world_mut()
        .get_mut::<Transform>(camera)
        .unwrap()
        .translation
        .y = 50.0;
    a.update();
    assert!(matches!(
        a.world().get::<DistanceFog>(camera).unwrap().falloff,
        bevy::pbr::FogFalloff::Linear {
            start: 7.0,
            end: 321.0
        }
    ));
}

#[test]
fn leaving_water_into_an_interior_restores_current_lighting_and_preference() {
    let (mut a, camera, _dir) = fog_setup();
    a.world_mut()
        .get_mut::<Transform>(camera)
        .unwrap()
        .translation
        .y = 50.0;
    a.update();
    let light = fnv_world::lighting::CellLighting {
        fog_color: [190, 17, 61],
        fog_near: 27.0,
        fog_far: 1900.0,
        fog_power: 1.7,
        ..Default::default()
    };
    a.world_mut().resource_mut::<SkyState>().interior = Some(light);
    a.world_mut().resource_mut::<Streamer>().interior = Some(0x999);
    let tweaks = nvf_play::tweaks::TweakSettings {
        no_interior_fog: true,
        ..Default::default()
    };
    a.insert_resource(tweaks);
    let rgb = light.fog_color.map(srgb_to_linear);
    let expected = DistanceFog {
        color: Color::linear_rgb(rgb[0], rgb[1], rgb[2]),
        falloff: crate::fog::falloff(27.0, 1_000_000.0, 1.7, false, 1.0),
        ..Default::default()
    };
    a.update();
    assert_falloff(a.world().get::<DistanceFog>(camera).unwrap(), &expected);
    assert!(
        a.world()
            .get::<DistanceFog>(camera)
            .unwrap()
            .color
            .to_linear()
            .red
            > a.world()
                .get::<DistanceFog>(camera)
                .unwrap()
                .color
                .to_linear()
                .blue
    );
}
