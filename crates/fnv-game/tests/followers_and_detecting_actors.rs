//! Authored tests for JIP LN `GetFollowers` and `GetDetectingActors` (`nvse_gaps`).

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    detection::{Band, Detection},
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const CELL_ID: FormId = FormId(0x200);
const NPC_BASE: FormId = FormId(0x800);
const CONT_BASE: FormId = FormId(0x801);

const PKG_FOLLOW_PLAYER: FormId = FormId(0x810);
const PKG_ACCOMPANY_LINKED: FormId = FormId(0x811);
const PKG_GUARD: FormId = FormId(0x812);
const PKG_FOLLOW_LEADER: FormId = FormId(0x813);

const FOLLOWER_A: FormId = FormId(0x301);
const FOLLOWER_B: FormId = FormId(0x302);
const LEADER_NPC: FormId = FormId(0x303);
const WATCHER_A: FormId = FormId(0x304);
const WATCHER_B: FormId = FormId(0x305);
const CRATE_REF: FormId = FormId(0x306);

fn pack_record(id: FormId, edid: &str, ty: u8, target: Option<(i32, u32)>) -> Vec<u8> {
    // PKDT: general_flags (u32), type (u8), unused (u8), fallout_flags (u16), type_flags (u32) = 12 bytes.
    let mut pkdt = vec![0u8; 12];
    pkdt[4] = ty;
    // PSDT: month (i8), day (i8), date (u8), time (i8), duration (i32) = 8 bytes (-1 = any).
    let psdt = [0xFFu8, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0].to_vec();
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"PKDT", pkdt), sub(b"PSDT", psdt)];
    if let Some((kind, form)) = target {
        // PTDT: type (i32), target (u32), count_or_distance (i32), acquire_skill (f32) = 16 bytes.
        let mut ptdt = Vec::with_capacity(16);
        ptdt.extend_from_slice(&kind.to_le_bytes());
        ptdt.extend_from_slice(&form.to_le_bytes());
        ptdt.extend_from_slice(&0i32.to_le_bytes());
        ptdt.extend_from_slice(&0f32.to_le_bytes());
        subs.push(sub(b"PTDT", ptdt));
    }
    record(b"PACK", 0, id.0, &subs)
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let npc_base = record(b"NPC_", 0, NPC_BASE.0, &[sub(b"EDID", z("TestActorBase"))]);
    let cont_base = record(b"CONT", 0, CONT_BASE.0, &[sub(b"EDID", z("TestCrateBase"))]);

    // PackageType codes: Follow = 1, Accompany = 7, Guard = 14.
    // PTDT target kind: 0 = Specific Reference, 3 = Linked Reference.
    let packs = [
        pack_record(PKG_FOLLOW_PLAYER, "PkgFollowPlayer", 1, Some((0, FormId::PLAYER_REF.0))),
        pack_record(PKG_ACCOMPANY_LINKED, "PkgAccompanyLinked", 7, Some((3, 0))),
        pack_record(PKG_GUARD, "PkgGuard", 14, None),
        pack_record(PKG_FOLLOW_LEADER, "PkgFollowLeader", 1, Some((0, LEADER_NPC.0))),
    ]
    .concat();

    let placed_actor = |id: FormId, edid: &str| {
        record(
            b"ACHR",
            0,
            id.0,
            &[sub(b"EDID", z(edid)), sub(b"NAME", NPC_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
        )
    };
    let crate_ref = record(
        b"REFR",
        0,
        CRATE_REF.0,
        &[sub(b"EDID", z("TestCrateRef")), sub(b"NAME", CONT_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell = record(b"CELL", 0, CELL_ID.0, &[sub(b"EDID", z("FollowersDetectCell"))]);
    let children = group(
        CELL_ID.0.to_le_bytes(),
        6,
        &group(
            CELL_ID.0.to_le_bytes(),
            9,
            &[
                placed_actor(FOLLOWER_A, "FollowerARef"),
                placed_actor(FOLLOWER_B, "FollowerBRef"),
                placed_actor(LEADER_NPC, "LeaderNpcRef"),
                placed_actor(WATCHER_A, "WatcherARef"),
                placed_actor(WATCHER_B, "WatcherBRef"),
                crate_ref,
            ]
            .concat(),
        ),
    );
    let bytes = [
        tes4(),
        top(b"NPC_", &[player, npc_base].concat()),
        top(b"CONT", &cont_base),
        top(b"PACK", &packs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "FollowersDetectTest.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    g.player_entered(Some(CELL_ID));
    g
}

fn eval(g: &mut Game<'_>, body: &str) -> XVal {
    let before = g.events.len();
    let v = g.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"));
    assert!(
        !g.events[before..].iter().any(|e| matches!(
            e.kind,
            EventKind::Stub { .. }
                | EventKind::Unhandled { .. }
                | EventKind::CallFailed { .. }
                | EventKind::ScriptError { .. }
        )),
        "{body}: {:?}",
        &g.events[before..]
    );
    v
}

#[test]
fn get_followers_returns_live_follow_and_accompany_actors_and_ar_null_when_empty() {
    let f = fixture();
    let mut g = game(&f);

    // Empty / no-receiver cases return ar_Null (ar_Size == -1, matching gaps7).
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));

    // FollowerA follows player via Follow; FollowerB accompanies player via LinkedReference;
    // WatcherA runs a non-follow Guard package; WatcherB follows LeaderNpc.
    eval(
        &mut g,
        "FollowerARef.AddScriptPackage PkgFollowPlayer\n\
         FollowerBRef.SetLinkedReference player\n\
         FollowerBRef.AddScriptPackage PkgAccompanyLinked\n\
         WatcherARef.AddScriptPackage PkgGuard\n\
         WatcherBRef.AddScriptPackage PkgFollowLeader",
    );

    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(2.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (a[0])"), XVal::Ref(FOLLOWER_A.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (a[1])"), XVal::Ref(FOLLOWER_B.0));

    // Non-player leader sees only its own followers.
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetFollowers\nSetFunctionValue (a[0])"), XVal::Ref(WATCHER_B.0));

    // Disabling FollowerA excludes it; killing FollowerB excludes it -> back to ar_Null.
    eval(&mut g, "FollowerARef.Disable");
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (a[0])"), XVal::Ref(FOLLOWER_B.0));

    eval(&mut g, "FollowerBRef.Kill");
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));

    // Removing WatcherB's Follow script package clears LeaderNpc's followers back to ar_Null.
    eval(&mut g, "WatcherBRef.RemoveScriptPackage");
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetFollowers\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));
}

#[test]
fn get_detecting_actors_tracks_detected_band_filters_dead_disabled_and_non_actors() {
    let f = fixture();
    let mut g = game(&f);

    // No detectors, no receiver, or non-actor target => ar_Null (ar_Size == -1).
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := TestCrateRef.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));

    let det = |band, value| Detection { value, band, seen: band == Band::Detected, clear: band == Band::Detected, ..Default::default() };

    // WatcherB and WatcherA detect player; FollowerA only Noticed; FollowerB Hidden; self-pair ignored.
    g.detection.record(WATCHER_B, FormId::PLAYER_REF, det(Band::Detected, 45.0), g.time);
    g.detection.record(WATCHER_A, FormId::PLAYER_REF, det(Band::Detected, 80.0), g.time);
    g.detection.record(FOLLOWER_A, FormId::PLAYER_REF, det(Band::Noticed, -10.0), g.time);
    g.detection.record(FOLLOWER_B, FormId::PLAYER_REF, det(Band::Hidden, -80.0), g.time);
    g.detection.record(FormId::PLAYER_REF, FormId::PLAYER_REF, det(Band::Detected, 100.0), g.time);
    // Player and WatcherA also detect LeaderNpc (separate target).
    g.detection.record(FormId::PLAYER_REF, LEADER_NPC, det(Band::Detected, 60.0), g.time);
    g.detection.record(WATCHER_A, LEADER_NPC, det(Band::Detected, 50.0), g.time);
    // Spurious pair against non-actor crate is ignored.
    g.detection.record(WATCHER_A, CRATE_REF, det(Band::Detected, 50.0), g.time);

    // Deterministic FormID order: WatcherA (0x304) then WatcherB (0x305).
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(2.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetDetectingActors\nSetFunctionValue (a[0])"), XVal::Ref(WATCHER_A.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetDetectingActors\nSetFunctionValue (a[1])"), XVal::Ref(WATCHER_B.0));

    // NPC target sees both player (0x14) and WatcherA (0x304), matching GetDetectedActors symmetry.
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(2.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetDetectingActors\nSetFunctionValue (a[0])"), XVal::Ref(FormId::PLAYER_REF.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetDetectingActors\nSetFunctionValue (a[1])"), XVal::Ref(WATCHER_A.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := TestCrateRef.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));

    // Killing WatcherA and disabling WatcherB removes both from player's detecting actors -> ar_Null.
    eval(&mut g, "WatcherARef.Kill\nWatcherBRef.Disable");
    assert_eq!(eval(&mut g, "array_var a\nlet a := player.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(-1.0));
    // LeaderNpc is still detected by the living player alone.
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetDetectingActors\nSetFunctionValue (ar_Size a)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "array_var a\nlet a := LeaderNpcRef.GetDetectingActors\nSetFunctionValue (a[0])"), XVal::Ref(FormId::PLAYER_REF.0));
}
