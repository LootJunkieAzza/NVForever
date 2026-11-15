//! Real compiled Game writes -> production lighting reader -> SkyState -> Projection.
//! The source-only baseline adds a neutral reader seam, not a new lighting policy.
use super::*;
#[path = "../../../crates/fnv-game/tests/common/mod.rs"]
mod lighting_build;
#[path = "../../../crates/fnv-game/tests/common/lighting_templates.rs"]
mod fixtures;
use fixtures::*;
use crate::world::{effective_cell_lighting, WorldData};
use fnv_script::vm::XVal;

fn world(f: &Fixture) -> WorldData {
    WorldData {
        lo: f.lo, schema: f.schema,
        index: Box::leak(Box::new(fnv_world::WorldIndex::build(f.lo, f.schema))),
        world: 0, persistent_by_grid: HashMap::new(),
        imposters: Box::leak(Box::new(fnv_world::imposter::ImposterRegions::build(f.lo, f.schema))),
        weather_regions: &[], dropped_plugins: Vec::new(),
    }
}

fn sky() -> SkyState {
    SkyState {
        climate: Climate::default(), weather: Weather::default(), hour: 12.0,
        fog_override: None, interior: None, interior_grade: None, shadows: false,
        shadow_distance: DEFAULT_SHADOW_DISTANCE, shadow_reach: 1.0,
        shadow_cascades: DEFAULT_SHADOW_CASCADES, grade: true, time_scale: 0.0,
        clock_running: false, days_passed: 0.0, keep_once: false, prev: None, fade: 1.0,
        choices: Vec::new(), image_space: None, adapters: HashMap::new(), changed: true,
        script: None, pinned: false, regions: Vec::new(), source: None, placed: false, region_cell: None,
        clock_seen: false, sun_hour: None, rolled_for: None, pick: 0, rolls: 0,
        rng: DEFAULT_WEATHER_SEED, hour_index: None, live_globals: HashMap::new(),
        resume: None, flash: [0.0; 3], script_wind: None,
        moon_textures: Default::default(), lightning_request: false,
    }
}

fn app() -> (App, Entity, Entity) {
    let mut a = App::new();
    a.insert_resource(sky()).add_systems(Update, interior_clip);
    let main = a.world_mut().spawn((Projection::Perspective(PerspectiveProjection::default()),
        FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 })).id();
    let other = a.world_mut().spawn(Projection::Perspective(PerspectiveProjection { far: 12345.0, ..Default::default() })).id();
    (a, main, other)
}

fn far(a: &App, camera: Entity) -> f32 {
    match a.world().get::<Projection>(camera).unwrap() {
        Projection::Perspective(p) => p.far,
        _ => panic!("the authored camera must remain perspective"),
    }
}

fn apply(a: &mut App, w: &WorldData, game: Option<&fnv_game::Game<'_>>, cell: fnv_sim::FormId) {
    let lighting = effective_cell_lighting(w, game, cell.0);
    a.world_mut().resource_mut::<SkyState>().set_interior(lighting);
    a.update();
}

#[test]
fn cold_and_no_runtime_paths_keep_the_existing_real_camera_consumer() {
    let f = fixture(); let w = world(&f); let mut g = game(&f);
    let (mut a, main, other) = app();
    apply(&mut a, &w, None, CELL);
    assert_eq!(far(&a, main), 10000.0);
    apply(&mut a, &w, Some(&g), NO_TEMPLATE);
    assert_eq!(far(&a, main), 3000.0);
    apply(&mut a, &w, None, EMPTY_CELL);
    assert_eq!(far(&a, main), OUTDOOR_FAR, "the existing engine's no-positive-clip policy, not a native observation");
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetLightingTemplateTraitNumeric OwnedLightB 15 30000");
    apply(&mut a, &w, None, CELL);
    assert_eq!(far(&a, main), 10000.0, "no runtime means the unchanged immutable reader");
    assert_eq!(far(&a, other), 12345.0, "an unrelated camera has no FlyCam marker");
}

#[test]
fn compiled_template_edits_reach_the_projection_at_cell_application_boundaries() {
    let f = fixture(); let w = world(&f); let mut g = game(&f);
    let (mut a, main, other) = app();
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 10000.0);
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetLightingTemplateTraitNumeric OwnedLightB 15 30000");
    assert_eq!(query(&mut g, "SetFunctionValue (GetLightingTemplateTraitNumeric OwnedLightB 15)"), XVal::Num(30000.0));
    a.update();
    assert_eq!(far(&a, main), 10000.0, "no implicit live-cell application is added to this fixture or policy");
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 30000.0, "the actual Projection must receive the compiled session edit at an application boundary");
    apply(&mut a, &w, Some(&g), OTHER_CELL);
    assert_eq!(far(&a, main), 10000.0, "the other cell still selects its original template");
    set(&mut g, "SetLightingTemplateTraitNumeric OwnedLightB 15 0");
    a.update();
    assert_eq!(far(&a, main), 10000.0);
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), OUTDOOR_FAR, "zero follows the already documented engine fallback policy");
    set(&mut g, "SetCellFlag OwnedLightCell 15 0");
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 1000.0, "the cell's own clip wins when inheritance is cleared");
    set(&mut g, "SetCellFlag OwnedLightCell 15 1\nSetCellLightingTemplate OwnedLightCell 0");
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 1000.0, "explicitly removing the template does not resurrect the original");
    assert_eq!(far(&a, other), 12345.0);
}

#[test]
fn a_restored_game_and_runtime_absence_do_not_reuse_old_session_lighting() {
    let f = fixture(); let w = world(&f); let mut g = game(&f);
    let (mut a, main, other) = app();
    set(&mut g, "SetCellLightingTemplate OwnedLightCell OwnedLightB\nSetLightingTemplateTraitNumeric OwnedLightB 15 30000");
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 30000.0);
    let loaded = restore(&f, &g.snapshot());
    apply(&mut a, &w, Some(&loaded), CELL);
    assert_eq!(far(&a, main), 10000.0, "a fresh Game::restore keeps the existing nonpersistent generic override policy");
    apply(&mut a, &w, Some(&g), CELL);
    assert_eq!(far(&a, main), 30000.0, "the prior Game's session remains separate");
    apply(&mut a, &w, None, CELL);
    assert_eq!(far(&a, main), 10000.0, "losing the runtime restores immutable cell lighting");
    assert_eq!(far(&a, other), 12345.0);
}

#[test]
fn cold_shifted_plugin_lighting_reaches_the_same_projection() {
    let f = shifted_fixture(); let w = world(&f); let g = game(&f);
    let (mut a, main, other) = app();
    apply(&mut a, &w, None, SHIFTED_CELL);
    assert_eq!(far(&a, main), 45000.0);
    apply(&mut a, &w, Some(&g), SHIFTED_CELL);
    assert_eq!(far(&a, main), 45000.0, "runtime availability must preserve the cold LoadOrder namespace mapping");
    assert_eq!(far(&a, other), 12345.0);
}
