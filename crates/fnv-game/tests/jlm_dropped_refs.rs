//! Independent source/World consumers; producer association is explicitly seeded.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, GameEnv, GameSnapshot, RunCtx},
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;

const CELL: u32 = 0xE00;
const NPC: u32 = 0xE10;
const CREATURE: u32 = 0xE11;
const NPC_LIST: u32 = 0xE12;
const CREATURE_LIST: u32 = 0xE13;
const HOLDER: u32 = 0xE20;
const BEAST: u32 = 0xE21;
const LEVELED_NPC: u32 = 0xE22;
const LEVELED_CREATURE: u32 = 0xE23;
const BOX: u32 = 0xE24;
const CONTAINER: u32 = 0xE25;
const CARBINE: u32 = 0xE30;
const BLADE: u32 = 0xE31;
const COMPONENT: u32 = 0xE32;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let actor = |sig, id, name: &str| {
        record(
            sig,
            0,
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"FULL", z(name)),
                sub(b"ACBS", vec![0; 24]),
            ],
        )
    };
    let list = |sig, id, name: &str, member: u32| {
        record(
            sig,
            0,
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"LVLD", vec![0]),
                sub(b"LVLF", vec![0]),
                sub(
                    b"LVLO",
                    [
                        1i16.to_le_bytes().to_vec(),
                        vec![0; 2],
                        member.to_le_bytes().to_vec(),
                        1i16.to_le_bytes().to_vec(),
                        vec![0; 2],
                    ]
                    .concat(),
                ),
            ],
        )
    };
    let placed = |sig, id, name: &str, base: u32| {
        record(
            sig,
            0,
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"NAME", base.to_le_bytes().to_vec()),
                sub(b"DATA", vec![0; 24]),
            ],
        )
    };
    let refs = [
        placed(b"ACHR", HOLDER, "OwnedHolder", NPC),
        placed(b"ACRE", BEAST, "OwnedBeast", CREATURE),
        placed(b"ACHR", LEVELED_NPC, "OwnedLeveledNpc", NPC_LIST),
        placed(
            b"ACRE",
            LEVELED_CREATURE,
            "OwnedLeveledCreature",
            CREATURE_LIST,
        ),
        placed(b"REFR", BOX, "OwnedBox", CONTAINER),
    ]
    .concat();
    let cell = [
        record(
            b"CELL",
            0,
            CELL,
            &[sub(b"EDID", z("OwnedDropCell")), sub(b"DATA", vec![1])],
        ),
        group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs)),
    ]
    .concat();
    let weapon = |id, name: &str| {
        record(
            b"WEAP",
            0,
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"FULL", z(name)),
                sub(b"DATA", vec![0; 15]),
            ],
        )
    };
    let bytes = [
        tes4(),
        top(b"NPC_", &actor(b"NPC_", NPC, "OwnedHolderBase")),
        top(b"CREA", &actor(b"CREA", CREATURE, "OwnedCreatureBase")),
        top(b"LVLN", &list(b"LVLN", NPC_LIST, "OwnedNpcList", NPC)),
        top(
            b"LVLC",
            &list(b"LVLC", CREATURE_LIST, "OwnedCreatureList", CREATURE),
        ),
        top(
            b"CONT",
            &record(
                b"CONT",
                0,
                CONTAINER,
                &[sub(b"EDID", z("OwnedContainerBase"))],
            ),
        ),
        top(
            b"WEAP",
            &[
                weapon(CARBINE, "Signal carbine"),
                weapon(BLADE, "Spare blade"),
            ]
            .concat(),
        ),
        top(
            b"MISC",
            &record(
                b"MISC",
                0,
                COMPONENT,
                &[
                    sub(b"EDID", z("OwnedComponent")),
                    sub(b"FULL", z("Circuit pack")),
                    sub(b"DATA", [1u32.to_le_bytes(), 1f32.to_le_bytes()].concat()),
                ],
            ),
        ),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedDroppedRefs.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&functions).unwrap(),
        &json,
        &refs,
    );
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}
fn query(g: &mut Game, declarations: &str, body: &str, ctx: RunCtx) -> XVal {
    g.run_query(
        &format!("{declarations}\nBegin Function {{}}\n{body}\nEnd\n"),
        ctx,
    )
    .unwrap_or_else(|e| panic!("{body}: {e}"))
}
fn refs_call(g: &mut Game, call: &str, ctx: RunCtx) -> Vec<FormId> {
    // Array ownership ends when run_query hands over/sweeps its result. Inspect the
    // array inside the live source and return only an owned scalar representation.
    let result = query(
        g,
        "array_var ownedRefs\narray_var ownedIter\nstring_var ownedIds",
        &format!(
            r#"
          let ownedRefs := {call}
          let ownedIds := ""
          if ownedRefs != Ar_Null
            foreach ownedIter <- ownedRefs
              let ownedIds += (sv_Construct "%i " ownedIter["value"])
            loop
          endif
          SetFunctionValue ownedIds
        "#
        ),
        ctx,
    );
    let XVal::Str(ids) = result else {
        panic!("owned source reference serialization: {result:?}")
    };
    ids.split_whitespace()
        .map(|id| FormId(u32::from_str_radix(id, 16).expect("source returned a FormID")))
        .collect()
}
fn refs(g: &mut Game, holder: &str) -> Vec<FormId> {
    refs_call(g, &format!("{holder}.GetDroppedRefs"), RunCtx::default())
}
/// Exercise real World placement/lifecycle; seed the existing producer's one association.
/// These controls intentionally do not claim death/VATS drop-producer coverage.
fn associate(g: &mut Game, holder: u32, item: u32) -> FormId {
    let r = g
        .world
        .place(
            FormId(item),
            Some(FormId(CELL)),
            [13.0, 27.0, 9.0],
            [0.0; 3],
            1,
        )
        .unwrap();
    g.world.get(r).unwrap().dropped_by = Some(FormId(holder));
    r
}

#[test]
fn compiled_hover_consumer_combines_inventory_refs_with_associated_world_weapons() {
    let f = fixture();
    let mut g = game(&f);
    g.world
        .add_item(FormId(HOLDER), FormId(COMPONENT), 3)
        .unwrap();
    let first = associate(&mut g, HOLDER, CARBINE);
    let second = associate(&mut g, HOLDER, BLADE);
    associate(&mut g, BEAST, CARBINE);
    associate(&mut g, HOLDER, COMPONENT); // Not a dropped weapon.
    let files = MemSource::default().with(
        "owned-hover.xml",
        r#"<menu name="HUDMainMenu"><text name="OwnedRow"><string>pending</string></text></menu>"#,
    );
    g.ui_tree.load_menu(&files, "owned-hover.xml").unwrap();
    let original_source = format!("array_var carried\narray_var dropped\narray_var each\nref entry\nstring_var label\nBegin Function {{}}\n{}\nEnd\n", r#"
      let carried := OwnedHolder.GetAllItemRefs 0 1 0 0
      let dropped := OwnedHolder.GetDroppedRefs
      if dropped != Ar_Null
        Ar_InsertRange carried 0 dropped
      endif
      let label := ""
      foreach each <- carried
        let entry := *each
        let label += $entry + ";"
      loop
      SetUIStringAlt "HUDMainMenu/OwnedRow/string" $label
      SetFunctionValue (Ar_Size carried)
    "#);
    {
        let rejected = fnv_script::compile::compile_runtime(
            &original_source,
            &g.data.table,
            &GameEnv { g: &g },
        );
        let first = rejected
            .errors()
            .next()
            .expect("the original helper must retain its actual declaration refusal");
        assert_eq!(
            (first.kind, first.detail.as_str()),
            ("variable_named_like_a_function", "label"),
            "original-hover-compile: retain the precise compiler diagnostic"
        );
    }
    let renamed_source = original_source.replace("label", "ownedLabelText");
    {
        let rejected = fnv_script::compile::compile_runtime(
            &renamed_source,
            &g.data.table,
            &GameEnv { g: &g },
        );
        assert!(
            rejected.errors().any(|d| d.kind == "mismatched_quotes"),
            "renamed-hover-compile: retain the literal-semicolon refusal: {:?}",
            rejected.diags
        );
    }
    // A literal semicolon starts a source comment, including inside quotes. Keep
    // the exact rendered delimiter through the existing character conversion.
    let corrected_source = renamed_source.replace("+ \";\"", "+ (AsciiToChar 59)");
    {
        let compiled = fnv_script::compile::compile_runtime(
            &corrected_source,
            &g.data.table,
            &GameEnv { g: &g },
        );
        assert!(
            compiled.errors().next().is_none(),
            "corrected-hover-compile: {:?}",
            compiled.diags
        );
    }
    let count = g
        .run_query(&corrected_source, RunCtx::default())
        .unwrap_or_else(|e| panic!("corrected hover consumer: {e}"));
    assert_eq!(
        count,
        XVal::Num(3.0),
        "jlm-dropped: real hover merge must include both associated world weapons"
    );
    assert_eq!(
        g.ui_tree
            .get_string("HUDMainMenu/OwnedRow/string")
            .as_deref(),
        Some("Signal carbine;Spare blade;Circuit pack;")
    );
    assert_eq!(refs(&mut g, "OwnedHolder"), [first, second]);
    for id in [first, second] {
        assert!(
            g.temp_item(id).is_none(),
            "world refs must not be inventory handles"
        );
    }
    assert_eq!(
        g.world.item_count(FormId(HOLDER), FormId(CARBINE)).unwrap(),
        0
    );
    assert_eq!(
        g.world
            .item_count(FormId(HOLDER), FormId(COMPONENT))
            .unwrap(),
        3
    );
}

#[test]
fn ordinary_and_selected_leveled_npc_creature_receivers_keep_their_associations() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.data.actors.contains_key(&FormId(NPC_LIST)));
    assert!(!g.data.actors.contains_key(&FormId(CREATURE_LIST)));
    for (id, name) in [
        (HOLDER, "OwnedHolder"),
        (BEAST, "OwnedBeast"),
        (LEVELED_NPC, "OwnedLeveledNpc"),
        (LEVELED_CREATURE, "OwnedLeveledCreature"),
    ] {
        let expected = associate(&mut g, id, CARBINE);
        assert_eq!(
            refs(&mut g, name),
            [expected],
            "selected actor qualification: {name}"
        );
        assert_eq!(
            refs(&mut g, name),
            [expected],
            "queries do not consume/drop or duplicate a reference"
        );
    }
}

#[test]
fn disabled_deleted_and_checked_restore_states_follow_existing_world_lifecycle() {
    let f = fixture();
    let mut g = game(&f);
    let kept = associate(&mut g, HOLDER, CARBINE);
    let disabled = associate(&mut g, HOLDER, BLADE);
    let deleted = associate(&mut g, HOLDER, CARBINE);
    g.world.set_enabled(disabled, false).unwrap();
    g.world.set_deleted(deleted, true).unwrap();
    assert_eq!(
        refs(&mut g, "OwnedHolder"),
        [kept],
        "existing association filters disabled/deleted world refs"
    );
    let snap = g.checked_snapshot().unwrap();
    let back: GameSnapshot = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut restored = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(refs(&mut restored, "OwnedHolder"), [kept]);
    restored.world.set_enabled(disabled, true).unwrap();
    assert_eq!(refs(&mut restored, "OwnedHolder"), [kept, disabled]);
    restored.world.set_deleted(kept, true).unwrap();
    assert_eq!(refs(&mut restored, "OwnedHolder"), [disabled]);
    assert!(
        refs(&mut game(&f), "OwnedHolder").is_empty(),
        "a fresh world inherits no old association"
    );
}

#[test]
fn source_receiver_admission_distinguishes_calling_actor_from_explicit_invalid_subject() {
    let f = fixture();
    let mut g = game(&f);
    let expected = associate(&mut g, HOLDER, CARBINE);
    associate(&mut g, BOX, CARBINE); // Even an erroneous nonactor association must not qualify.
    let ctx = RunCtx {
        self_ref: Some(FormId(HOLDER)),
        ..Default::default()
    };
    let files = MemSource::default().with(
        "owned-receiver.xml",
        r#"<menu name="HUDMainMenu"><rect name="OwnedReceiver"><_after>0</_after></rect></menu>"#,
    );
    g.ui_tree.load_menu(&files, "owned-receiver.xml").unwrap();
    for input in ["let explicit := 0", "let explicit := -1"] {
        assert!(g.ui_tree.set_float("HUDMainMenu/OwnedReceiver/_after", 0.0));
        let before = serde_json::to_value(g.world.snapshot()).unwrap();
        let source = format!("ref explicit\nBegin Function {{}}\n{input}\nSetFunctionValue (explicit.GetDroppedRefs)\nSetUIFloatAlt \"HUDMainMenu/OwnedReceiver/_after\" 1\nEnd\n");
        let error = g
            .run_query(&source, ctx)
            .expect_err("explicit invalid receiver must stop at the existing VM boundary");
        assert!(
            error.contains("getdroppedrefs called on no reference"),
            "exact invalid receiver refusal: {error}"
        );
        assert_eq!(
            g.ui_tree.get_float("HUDMainMenu/OwnedReceiver/_after"),
            Some(0.0),
            "the VM must not silently continue"
        );
        assert_eq!(
            serde_json::to_value(g.world.snapshot()).unwrap(),
            before,
            "invalid query must not mutate the world"
        );
    }
    for body in [
        "let explicit := OwnedHolderBase\nSetFunctionValue (explicit.GetDroppedRefs)",
        "let explicit := OwnedBox\nSetFunctionValue (explicit.GetDroppedRefs)",
    ] {
        assert_eq!(
            query(&mut g, "ref explicit", body, ctx),
            XVal::Num(0.0),
            "a valid nonactor/base receiver must not become the calling actor: {body}"
        );
    }
    assert_eq!(
        query(
            &mut g,
            "",
            "SetFunctionValue (GetDroppedRefs)",
            RunCtx::default()
        ),
        XVal::Num(0.0)
    );
    assert_eq!(
        refs_call(&mut g, "GetDroppedRefs", ctx),
        [expected],
        "calling actor must expose its associated world reference"
    );
}
