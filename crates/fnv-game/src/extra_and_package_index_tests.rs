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

#[test]
fn respawned_actor_clears_extra_index_and_immediately_reschedules_package() {
    use crate::game::RunCtx;
    use fnv_esm::raw::{write_subrecords, Subrecord};
    use fnv_esm::Plugin;

    const CELL_HOME: u32 = 0x200;
    const CELL_OTHER: u32 = 0x202;
    const PKG_WANDER: u32 = 0x600;
    const NPC_BASE: u32 = 0x800;
    const GUARD_REF: u32 = 0x301;
    const RESPAWN_FLAG: u32 = 0x8;
    const WANDER: u8 = 5;

    fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
        Subrecord { sig: *sig, data, extended: false }
    }
    fn z(s: &str) -> Vec<u8> {
        [s.as_bytes(), &[0]].concat()
    }
    fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
        let payload = write_subrecords(subs);
        [
            &sig[..],
            &(payload.len() as u32).to_le_bytes(),
            &0u32.to_le_bytes(),
            &fid.to_le_bytes(),
            &[0; 4],
            &15u16.to_le_bytes(),
            &[0; 2],
            &payload,
        ]
        .concat()
    }
    fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
        [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
    }
    fn tes4() -> Vec<u8> {
        let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
        let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
        [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
    }
    fn glob(fid: u32, edid: &str, v: f32) -> Vec<u8> {
        record(b"GLOB", fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
    }
    fn achr(fid: u32, edid: &str, base: u32, pos: [f32; 3]) -> Vec<u8> {
        let mut data = Vec::with_capacity(24);
        for v in pos {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&[0u8; 12]);
        record(b"ACHR", fid, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
    }
    fn cell_block(cid: u32, edid: &str, children: &[u8]) -> Vec<u8> {
        let c = record(b"CELL", cid, &[sub(b"EDID", z(edid)), sub(b"DATA", vec![1])]);
        if children.is_empty() {
            c
        } else {
            [c, group(cid.to_le_bytes(), 6, &group(cid.to_le_bytes(), 9, children))].concat()
        }
    }

    let globs = [
        glob(0x37, "GameDaysPassed", 1.0),
        glob(0x38, "GameHour", 12.0),
        glob(0x3A, "GameDay", 19.0),
        glob(0x3B, "GameMonth", 9.0),
        glob(0x3C, "GameYear", 2281.0),
        glob(0x39, "TimeScale", 30.0),
    ]
    .concat();
    let pack = record(
        b"PACK",
        PKG_WANDER,
        &[
            sub(b"EDID", z("TestWanderPkg")),
            sub(b"PKDT", [&0u32.to_le_bytes()[..], &[WANDER, 0, 0, 0], &[0; 4]].concat()),
            sub(b"PLDT", [2i32.to_le_bytes(), 0u32.to_le_bytes(), 256i32.to_le_bytes()].concat()),
        ],
    );
    let mut acbs = RESPAWN_FLAG.to_le_bytes().to_vec();
    acbs.extend([0u8; 18]);
    acbs.extend(0u16.to_le_bytes());
    let npc = record(
        b"NPC_",
        NPC_BASE,
        &[
            sub(b"EDID", z("TestGuardNpc")),
            sub(b"ACBS", acbs),
            sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat()),
            sub(b"PKID", PKG_WANDER.to_le_bytes().to_vec()),
        ],
    );
    let cells = [
        cell_block(CELL_HOME, "HomeCell", &achr(GUARD_REF, "GuardRef", NPC_BASE, [100.0, 0.0, 0.0])),
        cell_block(CELL_OTHER, "OtherCell", &[]),
    ]
    .concat();
    let bytes = [
        tes4(),
        group(*b"GLOB", 0, &globs),
        group(*b"PACK", 0, &pack),
        group(*b"NPC_", 0, &npc),
        group(*b"CELL", 0, &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cells))),
    ]
    .concat();

    let plugins = vec![Plugin::parse(&bytes, "RespawnPkg.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalog: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalog, &refs);
    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, SimConfig::default()),
        GameConfig { package_interval: Some(10.0), ..GameConfig::default() },
    );
    let guard = FormId(GUARD_REF);

    // Enter CELL_HOME: GUARD picks its wander package and initializes PackageSchedule + ExtraIndex.
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL_HOME));
    g.player_entered(Some(FormId(CELL_HOME)));
    g.time += 0.1;
    g.tick();
    assert_eq!(g.extra.get(&guard).and_then(|e| e.current_package), Some(FormId(PKG_WANDER)));

    // Initialize ExtraIndex, kill GUARD first (teammates go unconscious instead of dying
    // in non-hardcore), populate ExtraIndex (teammates + burning), and tick past its next
    // due time so PackageSchedule reschedules the dead actor for `now + 10.0`.
    g.ensure_extra_index();
    g.run_query("Begin Function {}\nGuardRef.Kill\nGuardRef.SetPlayerTeammate 1\nEnd\n", RunCtx::default()).unwrap();
    g.extra.entry(guard).or_default().burning = Some(30.0);
    g.note_burning(guard, true);
    assert!(g.is_dead(guard));
    assert!(g.extra_index.teammates.contains(&guard));
    assert!(g.extra_index.burning.contains(&guard));
    g.time += 10.5;
    g.tick();
    assert!(g.is_dead(guard));

    // Leave CELL_HOME for 72 hours, then return so cell_reset calls recreate(guard).
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL_OTHER));
    g.player_entered(Some(FormId(CELL_OTHER)));
    g.world.advance_hours(72.0).unwrap();
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(CELL_HOME));
    g.player_entered(Some(FormId(CELL_HOME)));

    let resets = g.take_cell_resets();
    assert_eq!(resets.len(), 1);
    assert!(resets[0].respawned.contains(&guard));
    assert!(!g.is_dead(guard));
    assert_eq!(g.extra.get(&guard).and_then(|e| e.current_package), None);
    assert!(
        !g.extra_index.teammates.contains(&guard) && !g.extra_index.burning.contains(&guard),
        "recreate() must clear stale ExtraIndex entries when removing RefExtra"
    );

    // On the very next short tick (well before the stale 10 s dead-actor interval),
    // the respawned actor must immediately choose its package.
    g.time += 0.05;
    g.tick();
    assert_eq!(
        g.extra.get(&guard).and_then(|e| e.current_package),
        Some(FormId(PKG_WANDER)),
        "respawned actor must immediately pick a fresh package after recreate()"
    );
}
