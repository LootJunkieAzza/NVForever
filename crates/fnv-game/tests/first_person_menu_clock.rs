//! Authored source-compiler fixture for the Pip-Boy's first-person animation clock.
//! File availability is an interface seam; no animation decoding is claimed here.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{menus, Game, GameConfig};
use fnv_game::nvse_knvse::SequenceInfo;
use fnv_game::GameData;
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, World};
use std::sync::Arc;

const ROOM: u32 = 0x801;
const NPC: u32 = 0x802;
const FIRST: &str = "qa\\pipboy-clock\\first.kf";

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let actors = [
        record(
            b"NPC_",
            0,
            FormId::PLAYER_BASE.0,
            &[sub(b"EDID", z("Player"))],
        ),
        record(b"NPC_", 0, 0x800, &[sub(b"EDID", z("ClockNPC"))]),
    ]
    .concat();
    let room = record(
        b"CELL",
        0,
        ROOM,
        &[sub(b"EDID", z("ClockRoom")), sub(b"DATA", vec![1])],
    );
    let actor = record(
        b"ACHR",
        0,
        NPC,
        &[
            sub(b"EDID", z("ClockNPCRef")),
            sub(b"NAME", 0x800u32.to_le_bytes().to_vec()),
            sub(b"DATA", vec![0; 24]),
        ],
    );
    let children = group(ROOM.to_le_bytes(), 6, &group(ROOM.to_le_bytes(), 9, &actor));
    let bytes = [
        tes4(),
        top(b"NPC_", &actors),
        top(
            b"CELL",
            &group([0; 4], 2, &group([0; 4], 3, &[room, children].concat())),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "Clock.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let schema =
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let table = FunctionTable::load(&path).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, table, &json, &refs);
    Fixture { data, forms, refs }
}

struct Availability(fnv_game::mod_files::Roots);
impl Availability {
    fn install() -> Self {
        let mut saved = None;
        fnv_game::mod_files::update_roots(|roots| {
            saved = Some(roots.clone());
            roots.lookup = Some(Arc::new(|path| {
                u8::from(
                    path.to_ascii_lowercase()
                        .starts_with("meshes\\qa\\pipboy-clock\\"),
                )
            }));
        });
        Self(saved.unwrap())
    }
}
impl Drop for Availability {
    fn drop(&mut self) {
        fnv_game::mod_files::set_roots(self.0.clone());
    }
}

fn q(game: &mut Game<'_>, source: &str) -> XVal {
    game.run_query(
        &format!("Begin Function {{}}\n{source}\nEnd"),
        Default::default(),
    )
    .unwrap()
}

fn number(game: &mut Game<'_>, source: &str) -> f64 {
    let XVal::Num(value) = q(game, &format!("SetFunctionValue ({source})")) else {
        panic!("numeric query")
    };
    value
}

fn close(actual: f64, expected: f64, boundary: &str) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "{boundary}: got {actual}, expected {expected}"
    );
}

#[test]
fn pipboy_menu_advances_only_player_first_person_samples_and_lifecycle() {
    // The aggregate runs in parallel; only the exact child owns process-wide Roots.
    const SELF_TEST: &str = "first_person_menu_clock::pipboy_menu_advances_only_player_first_person_samples_and_lifecycle";
    const CHILD: &str = "NVF_QA_PIPBOY_MENU_CLOCK_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([SELF_TEST, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .expect("start the isolated exact menu-clock fixture");
        assert!(
            output.status.success(),
            "isolated menu-clock fixture failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.iter().any(|arg| arg == "--exact") && args.iter().any(|arg| arg == SELF_TEST),
        "Roots seam requires this exact fixture alone in its process"
    );
    let _availability = Availability::install();
    let fixture = fixture();
    let mut game = Game::new(
        &fixture.data,
        World::new(&fixture.forms, &fixture.refs, Default::default()),
        GameConfig {
            frame_seconds: 0.125,
            ..Default::default()
        },
    );
    game.player_entered(Some(FormId(ROOM)));
    game.senses.first_person = true;
    let global = number(&mut game, "PlayerREF.GetGlobalAnimTime 1");
    let third_global = number(&mut game, "PlayerREF.GetGlobalAnimTime 0");
    let npc_global = number(&mut game, "ClockNPCRef.GetGlobalAnimTime 0");
    for source in [
        "PlayerREF.ActivateAnim \"qa\\pipboy-clock\\first.kf\" 1 1 1 1 0.125",
        "PlayerREF.ActivateAnim \"qa\\pipboy-clock\\third.kf\" 0 1 1 1 0.125",
        "ClockNPCRef.ActivateAnim \"qa\\pipboy-clock\\npc.kf\" 0 1 1 1 0.125",
    ] {
        close(number(&mut game, source), 1.0, "source activation");
    }
    let world = game.time;
    game.menu = Some(menus::INVENTORY);
    for _ in 0..4 {
        game.tick();
    }
    close(game.time, world, "inventory keeps game time held");
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.5,
        "first-person menu sample advances",
    );
    close(
        number(
            &mut game,
            "PlayerREF.GetAnimCurrentTime \"qa\\pipboy-clock\\first.kf\"",
        ),
        0.5,
        "getter shares renderer sample",
    );
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 1"),
        global + 0.5,
        "first global clock",
    );
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 0"),
        third_global,
        "third global held",
    );
    close(
        number(&mut game, "ClockNPCRef.GetGlobalAnimTime 0"),
        npc_global,
        "NPC global held",
    );
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, "qa\\pipboy-clock\\third.kf", false),
        0.0,
        "third sample held",
    );
    close(
        game.script_sequence_time_for(FormId(NPC), "qa\\pipboy-clock\\npc.kf", false),
        0.0,
        "NPC sample held",
    );

    q(&mut game, "PlayerREF.SetGlobalAnimTime 20 1");
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 1"),
        20.0,
        "global setter immediate",
    );
    game.tick();
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 1"),
        20.125,
        "global setter continues on menu clock",
    );
    let before_seek = game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true);
    q(
        &mut game,
        "PlayerREF.SetAnimCurrentTime \"qa\\pipboy-clock\\first.kf\" 0.25",
    );
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        before_seek,
        "seek stays deferred",
    );
    game.tick();
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.375,
        "seek then one menu frame",
    );

    let frozen_global = number(&mut game, "PlayerREF.GetGlobalAnimTime 1");
    for _ in 0..2 {
        game.tick();
        let advanced = number(&mut game, "PlayerREF.GetGlobalAnimTime 1");
        q(
            &mut game,
            &format!("PlayerREF.SetGlobalAnimTime {} 1", advanced - 0.125),
        );
        close(
            number(&mut game, "PlayerREF.GetGlobalAnimTime 1"),
            frozen_global,
            "explicit per-frame clock freeze",
        );
        close(
            game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
            0.375,
            "explicit clock freeze reaches renderer sample",
        );
    }

    game.menu = Some(menus::MESSAGE);
    game.tick();
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.375,
        "unrelated menu keeps existing freeze",
    );
    game.menu = Some(menus::MAP);
    game.loading = true;
    game.tick();
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.375,
        "loading does not advance device time",
    );
    game.loading = false;
    close(
        number(
            &mut game,
            "PlayerREF.DeactivateAnim \"qa\\pipboy-clock\\first.kf\" 0.25",
        ),
        1.0,
        "deactivation",
    );
    for _ in 0..4 {
        game.tick();
    }
    let key = (FormId::PLAYER_REF, true, FIRST.to_string());
    assert!(
        !game.anim_paths.sequences.contains_key(&key),
        "menu fade request retires on its own clock"
    );
    close(
        number(
            &mut game,
            "PlayerREF.ActivateAnim \"qa\\pipboy-clock\\first.kf\" 1 1 0 1 0.125",
        ),
        1.0,
        "resume",
    );
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.375,
        "resume retains paused sample",
    );

    let auto = "qa\\pipboy-clock\\auto.kf";
    close(
        number(
            &mut game,
            "PlayerREF.PlayAnimationPath \"qa\\pipboy-clock\\auto.kf\" 1",
        ),
        1.0,
        "automatic sequence",
    );
    game.set_sequence_info(
        auto,
        SequenceInfo {
            name: "Authored menu opening".into(),
            accum_root: "".into(),
            frequency: 1.0,
            begin: 0.0,
            end: 0.25,
            cycle: 2,
            text_keys: vec![],
        },
    );
    for _ in 0..6 {
        game.tick();
    }
    assert!(
        !game
            .anim_paths
            .sequences
            .contains_key(&(FormId::PLAYER_REF, true, auto.into())),
        "clamped menu animation completes"
    );

    let before = game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true);
    game.menu = None;
    game.tick();
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        before + 0.125,
        "closing menu resumes without a jump",
    );
    close(game.time, world + 0.125, "world resumes once");
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, "qa\\pipboy-clock\\third.kf", false),
        0.125,
        "third view resumes",
    );
    close(
        game.script_sequence_time_for(FormId(NPC), "qa\\pipboy-clock\\npc.kf", false),
        0.125,
        "NPC resumes",
    );
    let mut fresh = Game::new(
        &fixture.data,
        World::new(&fixture.forms, &fixture.refs, Default::default()),
        GameConfig {
            frame_seconds: 0.125,
            ..Default::default()
        },
    );
    close(
        number(&mut fresh, "PlayerREF.GetGlobalAnimTime 1"),
        0.125,
        "new runtime does not inherit menu time",
    );
}

#[test]
fn cold_third_person_global_does_not_freeze_first_person_menu_sequence() {
    const SELF_TEST: &str = "first_person_menu_clock::cold_third_person_global_does_not_freeze_first_person_menu_sequence";
    const CHILD: &str = "NVF_QA_PIPBOY_COLD_MENU_CLOCK_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([SELF_TEST, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .expect("start the isolated cold menu-clock fixture");
        assert!(
            output.status.success(),
            "isolated cold menu-clock fixture failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let args: Vec<_> = std::env::args().collect();
    assert!(args.iter().any(|arg| arg == "--exact") && args.iter().any(|arg| arg == SELF_TEST));
    let _availability = Availability::install();
    let fixture = fixture();
    let mut game = Game::new(
        &fixture.data,
        World::new(&fixture.forms, &fixture.refs, Default::default()),
        GameConfig {
            frame_seconds: 0.125,
            ..Default::default()
        },
    );
    game.player_entered(Some(FormId(ROOM)));
    game.senses.first_person = true;
    // Neither view's getter nor any animation has warmed the player's origins.
    q(&mut game, "PlayerREF.SetGlobalAnimTime 20 0");
    q(&mut game, "ClockNPCRef.SetGlobalAnimTime 7 0");
    assert!(
        !game.knvse.clock_start.contains_key(&FormId::PLAYER_REF),
        "the player must enter the activation cold"
    );
    let explicit = game.knvse.global.clone();
    close(
        number(
            &mut game,
            "PlayerREF.ActivateAnim \"qa\\pipboy-clock\\first.kf\" 1 1 1 1 0.125",
        ),
        1.0,
        "cold first-person activation",
    );
    close(
        number(
            &mut game,
            "PlayerREF.ActivateAnim \"qa\\pipboy-clock\\third.kf\" 0 1 1 1 0.125",
        ),
        1.0,
        "third-person activation control",
    );
    close(
        number(
            &mut game,
            "ClockNPCRef.ActivateAnim \"qa\\pipboy-clock\\npc.kf\" 0 1 1 1 0.125",
        ),
        1.0,
        "NPC activation control",
    );
    let held = game.time;
    game.menu = Some(menus::INVENTORY);
    for _ in 0..4 {
        game.tick();
    }
    // Read the renderer's sample before any first-person getter can repair state.
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.5,
        "cold first-person menu sample advances without a first-person getter",
    );
    close(game.time, held, "cold menu keeps world time held");
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, "qa\\pipboy-clock\\third.kf", false),
        0.0,
        "explicit third-view sample stays held",
    );
    close(
        game.script_sequence_time_for(FormId(NPC), "qa\\pipboy-clock\\npc.kf", false),
        0.0,
        "explicit NPC sample stays held",
    );
    close(
        number(
            &mut game,
            "PlayerREF.GetAnimCurrentTime \"qa\\pipboy-clock\\first.kf\"",
        ),
        0.5,
        "cold getter agrees with renderer sample",
    );
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 1"),
        0.625,
        "first getter preserves admitted menu time",
    );
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 0"),
        20.0,
        "explicit third-view value is preserved",
    );
    close(
        number(&mut game, "ClockNPCRef.GetGlobalAnimTime 0"),
        7.0,
        "explicit NPC value is preserved",
    );
    assert_eq!(
        game.knvse.global, explicit,
        "origin admission must not rewrite explicit values or set times"
    );
    game.menu = None;
    game.tick();
    close(
        game.script_sequence_time_for(FormId::PLAYER_REF, FIRST, true),
        0.625,
        "world resume neither resets nor replays admitted menu time",
    );
    close(
        number(&mut game, "PlayerREF.GetGlobalAnimTime 0"),
        20.125,
        "third-view clock resumes on world time",
    );
    close(
        number(&mut game, "ClockNPCRef.GetGlobalAnimTime 0"),
        7.125,
        "NPC clock resumes on world time",
    );
}
