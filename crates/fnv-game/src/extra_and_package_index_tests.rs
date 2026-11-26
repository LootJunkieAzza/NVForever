use crate::data::GameData;
use crate::game::{Game, GameConfig};
use fnv_esm::{LoadOrder, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

fn empty_game() -> (GameData, FormStore, RefTable) {
    let plugins: &[fnv_esm::Plugin] = &[];
    let lo = LoadOrder::new(plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalog: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalog, &refs);
    (data, forms, refs)
}

#[test]
fn extra_index_tracks_sparse_flags_across_cold_build_and_live_mutations() {
    let (data, forms, refs) = empty_game();
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    let a = FormId(0x0000_6001);
    let b = FormId(0x0000_6002);
    let c = FormId(0x0000_6003);

    // Pre-populate before lazy index initialization.
    g.extra.entry(a).or_default().teammate = true;
    g.extra.entry(a).or_default().marked_for_delete = true;
    g.extra.entry(b).or_default().burning = Some(5.0);

    assert_eq!(g.teammates_indexed(), vec![a]);
    assert_eq!(g.marked_for_delete_refs(), vec![a]);
    assert!(g.extra_index.initialized);
    assert!(g.extra_index.burning.contains(&b));

    // Live mutations after initialization stay synchronized.
    g.extra.entry(c).or_default().teammate = true;
    g.note_teammate(c, true);
    assert_eq!(g.teammates_indexed(), vec![a, c]);

    g.extra.entry(a).or_default().teammate = false;
    g.note_teammate(a, false);
    assert_eq!(g.teammates_indexed(), vec![c]);

    g.extra.entry(c).or_default().marked_for_delete = true;
    g.note_marked_for_delete(c);
    assert_eq!(g.marked_for_delete_refs(), vec![a, c]);

    g.note_burning(b, false);
    g.extra.entry(b).or_default().burning = None;
    assert!(g.extra_index.burning.is_empty());

    // Knockouts & concussions stay fast when sparse sets are empty.
    assert!(g.tick_knockouts(0.016).is_empty());
    g.tick_concussions();
    assert!(g.destruction_tick(0.016).is_empty());
}

#[test]
fn package_schedule_wakes_immediately_on_enable_ai_on_resurrect_and_dynamic_spawn() {
    use crate::data::ActorDef;
    use crate::package::PackageDef;

    let (mut data, mut forms, refs) = empty_game();
    let cell = FormId(0x0000_8001);
    let pkg = FormId(0x0000_8002);
    let npc_base = FormId(0x0000_8003);

    data.package_defs.insert(pkg, PackageDef::default());
    data.actors.insert(
        npc_base,
        ActorDef {
            packages: vec![pkg],
            ..Default::default()
        },
    );
    forms.sigs.insert(npc_base, *b"NPC_");
    data.sigs.insert(npc_base, *b"NPC_");

    let cfg = GameConfig {
        package_interval: Some(10.0),
        ..Default::default()
    };
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), cfg);
    g.loaded_cells.insert(cell);

    // Initialize package_schedule on an empty loaded cell so `package_scan_due` jumps to `now + 10.0`.
    g.time = 1.0;
    g.tick_packages();
    assert!(g.package_schedule.initialized);
    assert!(g.package_scan_due >= 11.0);

    // 1. Dynamically spawn an actor into the loaded cell via create_at -> must wake immediately,
    // not stall 10 seconds until `now = 11.0`!
    let dyn_actor = g.create_at(npc_base, Some(cell), [0.0; 3], [0.0; 3]).unwrap();
    g.time = 1.1;
    g.tick_packages();
    assert_eq!(
        g.extra.get(&dyn_actor).and_then(|e| e.current_package),
        Some(pkg),
        "dynamic actor spawned into loaded cell must pick package immediately"
    );

    // 2. Disable actor and clear package; let tick push it to `now + 10.0`, then Enable -> wakes immediately.
    let disabled_actor = g.create_at(npc_base, Some(cell), [10.0, 0.0, 0.0], [0.0; 3]).unwrap();
    g.world.set_enabled(disabled_actor, false).unwrap();
    g.time = 2.0;
    g.tick_packages();
    assert_eq!(g.extra.get(&disabled_actor).and_then(|e| e.current_package), None);
    // Advance only 0.5s (well short of the 10s interval) and Enable via script path.
    g.time = 2.5;
    let _ = crate::functions::refs::enable_ref(&mut g, disabled_actor, false).unwrap();
    g.tick_packages();
    assert_eq!(
        g.extra.get(&disabled_actor).and_then(|e| e.current_package),
        Some(pkg),
        "re-enabled actor must pick package immediately"
    );

    // 3. Dead actor resurrected mid-interval wakes immediately.
    let dead_actor = g.create_at(npc_base, Some(cell), [20.0, 0.0, 0.0], [0.0; 3]).unwrap();
    g.extra.entry(dead_actor).or_default().dead = true;
    g.time = 3.0;
    g.tick_packages();
    assert_eq!(g.extra.get(&dead_actor).and_then(|e| e.current_package), None);
    g.time = 3.5;
    g.resurrect(dead_actor, false);
    g.tick_packages();
    assert_eq!(
        g.extra.get(&dead_actor).and_then(|e| e.current_package),
        Some(pkg),
        "resurrected actor must pick package immediately"
    );

    // 4. Actor with SetActorsAI 0 -> SetActorsAI 1 mid-interval wakes immediately.
    let paused_actor = g.create_at(npc_base, Some(cell), [30.0, 0.0, 0.0], [0.0; 3]).unwrap();
    g.set_actor_ai_off(paused_actor, true);
    g.time = 4.0;
    g.tick_packages();
    assert_eq!(g.extra.get(&paused_actor).and_then(|e| e.current_package), None);
    g.time = 4.5;
    g.set_actor_ai_off(paused_actor, false);
    g.tick_packages();
    assert_eq!(
        g.extra.get(&paused_actor).and_then(|e| e.current_package),
        Some(pkg),
        "unpaused AI actor must pick package immediately"
    );
}
