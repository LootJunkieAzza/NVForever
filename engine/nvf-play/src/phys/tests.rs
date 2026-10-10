//! Streaming through the loader, over cells made up in memory (no game data): the world
//! must come out the same whatever order the loader finishes cells in, the same as
//! loading every cell on the main thread, and the same on every run.

use super::*;
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::{CharacterInput, PreparedRef, PreparedTerrain, RefPose};
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use fnv_world::land::SIDE;
use fnv_world::WorldIndex;
use prepare::{Entry, RefShapes};

fn sim() -> SimState {
    let forms: &'static FormStore = Box::leak(Box::default());
    let refs: &'static RefTable = Box::leak(Box::default());
    SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }
}

fn play_data() -> PlayData {
    let lo: &'static fnv_esm::LoadOrder<'static> = Box::leak(Box::new(fnv_esm::LoadOrder::new(&[]).expect("empty load order")));
    let schema: &'static fnv_esm::Schema = Box::leak(Box::default());
    let index: &'static WorldIndex = Box::leak(Box::default());
    PlayData { lo, schema, index, world: 0x3C, interior: None, eye: [0.0; 3], heading_deg: 0.0, pitch_deg: 0.0, auto_walk: 0.0, start_at_cell_spot: false }
}

fn object(half: f32, motion: Motion) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer: fnv_phys::layers::STATIC,
        mass: 20.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents: [half; 3] },
        shell: 0.0,
    }
}

/// A made-up exterior cell: gently sloping terrain, a row of static boxes and a dynamic
/// crate dropped from above (so that stepping depends on when and in what order bodies
/// arrived), with FormIDs derived from the grid square.
fn fake_cell(job: &Job) -> Prepared {
    let PhysCell::Exterior(gx, gy) = job.cell else { panic!("exterior cells only") };
    let params = PhysParams::default();
    let (x0, y0) = (gx as f32 * CELL_SIZE, gy as f32 * CELL_SIZE);
    // A plane rising to the north-east, continuous across cells.
    let heights: Vec<f32> =
        (0..SIDE * SIDE).map(|i| 0.01 * (x0 + (i % SIDE) as f32 * 128.0) + 0.005 * (y0 + (i / SIDE) as f32 * 128.0)).collect();
    let base = 0x0100_0000 + (((gx + 100) as u32) << 12) + (((gy + 100) as u32) << 4);
    let mut refs = Vec::new();
    for i in 0..6u32 {
        let dynamic = i == 5;
        let position = [x0 + 300.0 + i as f32 * 600.0, y0 + 1500.0 + (i % 2) as f32 * 900.0, if dynamic { 400.0 } else { 60.0 }];
        let pl = Placement {
            form_id: base + i,
            sig: *b"REFR",
            flags: 0,
            base: None,
            position,
            rotation: [0.0, 0.0, i as f32 * 0.4],
            scale: 1.0,
            enable_parent: None,
            open_by_default: false,
            teleport: false,
            emittance: None,
        };
        // As `prepare::cell` does: at the simulation's pose when it holds another.
        let pose = job.pose_of(pl.form_id);
        let pl = prepare::at_pose(pl, pose);
        let obj = object(if dynamic { 15.0 } else { 50.0 }, if dynamic { Motion::Dynamic } else { Motion::Static });
        let prepared = PreparedRef::new(&params, &RefPose::from_placement(&pl), &[obj]);
        let dynamic_local = dynamic.then_some(Iso::IDENTITY);
        refs.push(Entry { form_id: pl.form_id, placement: Some(pl), shapes: Some(RefShapes { tallies: vec!["fake"], water: Vec::new(), prepared: Some(prepared), dynamic_local, door: None }) });
    }
    // A pond: a reference with only a water volume (placed water, physics.md §6.6).
    let pond = Placement {
        form_id: base + 6,
        sig: *b"REFR",
        flags: 0,
        base: None,
        position: [x0 + 2000.0, y0 + 3500.0, 0.0],
        rotation: [0.0; 3],
        scale: 1.0,
        enable_parent: None,
        open_by_default: false,
        teleport: false,
        emittance: None,
    };
    let water = vec![CollisionObject { layer: fnv_phys::layers::WATER, ..object(100.0, Motion::Static) }];
    refs.push(Entry { form_id: pond.form_id, placement: Some(pond), shapes: Some(RefShapes { tallies: vec!["water_volume"], water, prepared: None, dynamic_local: None, door: None }) });
    let terrain = Some(PreparedTerrain::new(&params, (gx, gy), &heights));
    Prepared { cell: job.cell, heights: Some(heights), terrain, refs, times: LoadTimes::default() }
}

fn state(manual: bool) -> PhysState {
    let loader = if manual { loader::Loader::manual() } else { loader::Loader::default() };
    PhysState { fake: Some(fake_cell), loader, ..Default::default() }
}

fn centre(gx: i32, gy: i32) -> [f32; 3] {
    [(gx as f32 + 0.5) * CELL_SIZE, (gy as f32 + 0.5) * CELL_SIZE, 50.0]
}

/// Bit-identical worlds: every body's state in handle order, and what is loaded.
fn fingerprint(p: &PhysState) -> (u64, Vec<PhysCell>, fnv_phys::Stats) {
    (p.world.state_digest(), p.cells.keys().copied().collect(), p.world.stats())
}

#[test]
fn cells_go_in_in_request_order_whatever_order_they_finish() {
    let (d, sim) = (play_data(), sim());
    let ctx = LoadCtx { d: &d, sim: &sim, data: None };
    let run = |reverse: bool| {
        let mut p = state(true);
        p.stream(&ctx, centre(0, 0), CELLS_PER_FRAME);
        // The cell underfoot went in at once; its eight neighbours wait for the loader.
        assert_eq!(p.cells.keys().copied().collect::<Vec<_>>(), vec![PhysCell::Exterior(0, 0)]);
        assert_eq!(p.loader.pending().count(), 8);
        p.loader.run_manual(reverse);
        for _ in 0..8 {
            p.stream(&ctx, centre(0, 0), CELLS_PER_FRAME);
        }
        assert_eq!(p.cells.len(), 9);
        for _ in 0..30 {
            p.world.step();
        }
        fingerprint(&p)
    };
    let forward = run(false);
    assert_eq!(run(true), forward);
    // The same as loading every cell on the main thread, nearest first.
    let mut sync = state(true);
    for (x, y) in cells::wanted((0, 0)) {
        sync.load(&ctx, PhysCell::Exterior(x, y));
    }
    for _ in 0..30 {
        sync.world.step();
    }
    assert_eq!(fingerprint(&sync), forward);
    // The check can fail: in another order the same cells give another world.
    let mut other = state(true);
    for (x, y) in cells::wanted((0, 0)).into_iter().rev() {
        other.load(&ctx, PhysCell::Exterior(x, y));
    }
    for _ in 0..30 {
        other.world.step();
    }
    assert_ne!(fingerprint(&other), forward);
}

/// Walk east across three cells, streaming as the game does and waiting for the loader
/// thread each frame.
fn walk() -> ((u64, Vec<PhysCell>, fnv_phys::Stats), [f32; 3]) {
    let (d, sim) = (play_data(), sim());
    let ctx = LoadCtx { d: &d, sim: &sim, data: None };
    let mut p = state(false);
    let start = centre(0, 0);
    p.stream(&ctx, start, CELLS_PER_FRAME);
    p.finish_loading(&ctx);
    p.place_player(start, 200.0, 1000.0).expect("ground under the start");
    for _ in 0..900 {
        let feet = p.player.as_ref().unwrap().feet;
        p.stream(&ctx, feet, CELLS_PER_FRAME);
        p.finish_loading(&ctx);
        assert!(p.ready_at(feet));
        let c = p.player.as_mut().unwrap();
        p.world.move_character(c, CharacterInput { velocity: [700.0, 0.0], ..Default::default() });
        p.world.step();
    }
    let feet = p.player.as_ref().unwrap().feet;
    assert!(feet[0] > 2.0 * CELL_SIZE, "walked into the third cell: {feet:?}");
    assert_eq!(p.stats.loaded_inline, 0);
    (fingerprint(&p), feet)
}

#[test]
fn two_runs_are_identical() {
    let a = walk();
    let b = walk();
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.map(f32::to_bits), b.1.map(f32::to_bits));
}

#[test]
fn the_cell_underfoot_never_waits_for_the_loader() {
    let (d, sim) = (play_data(), sim());
    let ctx = LoadCtx { d: &d, sim: &sim, data: None };
    // A loader that never finishes anything.
    let mut p = state(true);
    p.stream(&ctx, centre(0, 0), CELLS_PER_FRAME);
    p.place_player(centre(0, 0), 200.0, 1000.0).expect("ground");
    assert_eq!(p.stats.loaded_inline, 0, "the start is not a hitch");
    // The player steps into the next cell before the loader delivered it.
    let next = centre(1, 0);
    assert!(!p.ready_at(next));
    p.stream(&ctx, next, CELLS_PER_FRAME);
    assert!(p.ready_at(next));
    assert_eq!(p.stats.loaded_inline, 1);
    // Its request was dropped: the loader's late result is not inserted twice.
    assert!(!p.loader.pending().any(|c| c == PhysCell::Exterior(1, 0)));
    p.loader.run_manual(false);
    let bodies = p.world.stats().refs;
    for _ in 0..20 {
        p.stream(&ctx, next, CELLS_PER_FRAME);
    }
    assert_eq!(p.cells.len(), 12);
    assert_eq!(p.world.stats().refs, bodies + 10 * 6, "ten more cells of six references");
}

#[test]
fn placed_water_comes_and_goes_with_its_cell() {
    let (d, sim) = (play_data(), sim());
    let ctx = LoadCtx { d: &d, sim: &sim, data: None };
    // Only the cell underfoot goes in: the neighbours wait for a loader that never runs.
    let mut p = state(true);
    let pond = [2000.0, 3500.0, 50.0];
    assert_eq!(p.world.water_surface_for(pond), None);
    p.stream(&ctx, centre(0, 0), CELLS_PER_FRAME);
    assert_eq!(p.world.water_surface_for(pond), Some(100.0), "the box's top");
    assert_eq!(p.stats.counts.get("water_volume_refs"), Some(&1));
    p.unload(PhysCell::Exterior(0, 0));
    assert_eq!(p.world.water_surface_for(pond), None);
}

#[test]
fn cells_left_behind_are_cancelled_and_reset_forgets_requests() {
    let (d, sim) = (play_data(), sim());
    let ctx = LoadCtx { d: &d, sim: &sim, data: None };
    let mut p = state(true);
    p.stream(&ctx, centre(0, 0), CELLS_PER_FRAME);
    // Far away before anything arrived: the old requests go, new ones replace them.
    p.stream(&ctx, centre(10, 0), CELLS_PER_FRAME);
    assert!(p.loader.pending().all(|c| matches!(c, PhysCell::Exterior(x, _) if x >= 9)));
    assert_eq!(p.cells.keys().copied().collect::<Vec<_>>(), vec![PhysCell::Exterior(10, 0)]);
    p.loader.run_manual(false);
    for _ in 0..20 {
        p.stream(&ctx, centre(10, 0), CELLS_PER_FRAME);
    }
    assert_eq!(p.cells.len(), 9);
    assert!(p.cells.keys().all(|c| matches!(c, PhysCell::Exterior(x, _) if (9..=11).contains(x))));
    // Door travel: everything requested is forgotten with the cells.
    p.stream(&ctx, centre(20, 0), CELLS_PER_FRAME);
    assert!(p.loader.pending().count() > 0);
    p.reset(Space::Interior(7));
    assert_eq!(p.loader.pending().count(), 0);
    assert!(p.cells.is_empty());
}

/// The dynamic crate of cell (0, 0) as a placed reference the simulation knows.
fn sim_with_crate() -> (SimState, u32, [f32; 3]) {
    let id = 0x0100_0000 + (100 << 12) + (100 << 4) + 5;
    let pos = [300.0 + 5.0 * 600.0, 1500.0 + 900.0, 400.0];
    let mut table = RefTable::default();
    table.refs.insert(
        FormId(id),
        fnv_sim::refs::PlacedRef {
            id: FormId(id),
            sig: *b"REFR",
            edid: None,
            base: FormId(0x0500_0000),
            cell: None,
            stored_in: None,
            plugin: 0,
            flags: 0,
            in_persistent_group: false,
            pos,
            rot: [0.0, 0.0, 2.0],
            scale: None,
            count: None,
            health: None,
            owner: None,
            lock: None,
            teleport: None,
            enable_parent: None,
            linked: None,
            patrol_idle: None,
            merchant_container: None,
            open_by_default: false,
        },
    );
    let forms: &'static FormStore = Box::leak(Box::default());
    let refs: &'static RefTable = Box::leak(Box::new(table));
    (SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }, id, pos)
}

#[test]
fn placed_clutter_moved_by_physics_is_written_back_and_reloads_there() {
    let d = play_data();
    let (mut sim, id, placed) = sim_with_crate();
    let mut p = state(true);
    p.load(&LoadCtx { d: &d, sim: &sim, data: None }, PhysCell::Exterior(0, 0));
    assert!(p.placed_dynamic.get(&id).is_some_and(|c| !c.moved), "tracked, not moved yet");
    assert!(sim.moved_pose(id).is_none());
    let mut written = 0;
    for _ in 0..240 {
        p.world.step();
        written += p.settle_placed(&mut sim.world).len();
    }
    assert!(p.placed_dynamic[&id].moved && written > 0);
    let (pos, rot) = sim.moved_pose(id).expect("written back");
    assert!(pos[2] < placed[2] - 200.0, "fell onto the terrain: {pos:?}");
    // At rest: nothing more to write.
    assert!(p.settle_placed(&mut sim.world).is_empty());
    // Unloaded and loaded again: built at the simulation's pose, not the placement's.
    p.unload(PhysCell::Exterior(0, 0));
    assert!(!p.placed_dynamic.contains_key(&id));
    // (The made-up cells have no records, so the job names the crate itself.)
    let cell = PhysCell::Exterior(0, 0);
    let job = with_poses(Job { cell, refs: vec![id], land: None, poses: Vec::new() }, &sim);
    p.insert(&LoadCtx { d: &d, sim: &sim, data: None }, fake_cell(&job));
    let b = p.world.ref_bodies(id)[0];
    for i in 0..3 {
        assert!((b.translation[i] - pos[i]).abs() < 1e-2, "{b:?} against {pos:?}");
    }
    assert!(fnv_world::transform::max_diff(&b.rotation, &fnv_world::transform::ref_rotation(rot)) < 1e-4);
    assert!(!p.placed_dynamic[&id].moved, "a fresh start from where it rests");
}
