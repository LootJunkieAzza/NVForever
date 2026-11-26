//! Parameterless CTDA GetFriendHit (function 288) vs explicit script calls (#2641).
//! Independently authored records and tests; no game data or process-global roots.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    cond,
    functions::Ctx,
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{
    vm::{Value, XVal},
    FunctionTable,
};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const FRIEND_HIT_FN: u16 = 288;
const FRIEND_HIT_OPCODE: u16 = 0x1120;
const SUBJECT: FormId = FormId(0x0000_0900);
const ACTOR_A: FormId = FormId(0x0000_0901);
const ACTOR_B: FormId = FormId(0x0000_0902);

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let npc = |id, name: &str| record(b"NPC_", 0, id, &[sub(b"EDID", z(name))]);
    let placed = |id, base: u32, name: &str| {
        record(
            b"ACHR",
            0,
            id,
            &[
                sub(b"EDID", z(name)),
                sub(b"NAME", base.to_le_bytes().to_vec()),
                sub(b"DATA", vec![0; 24]),
            ],
        )
    };
    let room = record(b"CELL", 0, 0x200, &[sub(b"EDID", z("OwnFriendHitRoom"))]);
    let actors = [
        placed(SUBJECT.0, 0x800, "OwnSubjectRef"),
        placed(ACTOR_A.0, 0x801, "OwnActorARef"),
        placed(ACTOR_B.0, 0x802, "OwnActorBRef"),
    ]
    .concat();
    let children = group(
        0x200u32.to_le_bytes(),
        6,
        &group(0x200u32.to_le_bytes(), 9, &actors),
    );
    let bytes = [
        tes4(),
        top(
            b"NPC_",
            &[
                npc(7, "Player"),
                npc(0x800, "OwnSubjectBase"),
                npc(0x801, "OwnActorABase"),
                npc(0x802, "OwnActorBBase"),
            ]
            .concat(),
        ),
        top(
            b"CELL",
            &group(
                0i32.to_le_bytes(),
                2,
                &group(0i32.to_le_bytes(), 3, &[room, children].concat()),
            ),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&bytes, "OwnedFriendHit.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
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

fn ctda_friend_hit(g: &Game, cmp_type: u8, rhs: f32) -> cond::Condition {
    cond::parse(
        &serde_json::json!({
            "type": cmp_type,
            "function": FRIEND_HIT_FN,
            "comparison_value_raw": rhs.to_bits(),
            "parameter1_raw": 0,
            "parameter2_raw": 0,
            "run_on": 0,
            "reference": 0,
        }),
        &g.data.param_types,
        &|raw| (raw != 0).then_some(raw),
    )
    .unwrap()
}

fn call_failed_since(g: &Game, before: usize) -> Vec<String> {
    g.events[before..]
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::CallFailed { opcode, error, .. } => Some(format!("{opcode}: {error}")),
            _ => None,
        })
        .collect()
}

#[test]
fn parameterless_get_friend_hit_ctda_reads_condition_target_without_call_failed() {
    let f = fixture();
    let mut g = game(&f);

    // 0x00 = Eq, 0x40 = Gt (the comparison used on FalloutNV.esm INFO CTDAs).
    let eq_zero = ctda_friend_hit(&g, 0x00, 0.0);
    let gt_zero = ctda_friend_hit(&g, 0x40, 0.0);
    assert_eq!(eq_zero.opcode(), FRIEND_HIT_OPCODE);
    assert!(
        eq_zero.params.is_empty(),
        "CTDA function 288 must parse with an empty parameter vector"
    );

    // Baseline: 0 hits for both targets, no CallFailed hidden behind == 0.
    let before = g.events.len();
    let res = g.eval_conditions(
        std::slice::from_ref(&eq_zero),
        Some(SUBJECT),
        Some(ACTOR_A),
        "friend_hit eq0",
    );
    let failed = call_failed_since(&g, before);
    assert!(failed.is_empty(), "unexpected CallFailed: {failed:?}");
    assert_eq!(res[0].value, 0.0);
    assert!(res[0].passed);

    // Record distinct active friendly hits from two different actors.
    g.time = 5.0;
    g.provocations
        .friend_hits
        .insert((SUBJECT, ACTOR_A), vec![4.5]);
    g.provocations
        .friend_hits
        .insert((SUBJECT, ACTOR_B), vec![3.0, 4.0]);

    // Target ACTOR_A sees 1 hit; Target ACTOR_B sees 2 hits; absent target sees 0.
    for (target, expected_val, expected_gt) in [
        (Some(ACTOR_A), 1.0, true),
        (Some(ACTOR_B), 2.0, true),
        (None, 0.0, false),
    ] {
        let before = g.events.len();
        let res = g.eval_conditions(
            std::slice::from_ref(&gt_zero),
            Some(SUBJECT),
            target,
            "friend_hit gt0",
        );
        let failed = call_failed_since(&g, before);
        assert!(
            failed.is_empty(),
            "target {target:?} produced CallFailed: {failed:?}"
        );
        assert_eq!(res[0].value, expected_val);
        assert_eq!(res[0].passed, expected_gt);
        assert_eq!(
            g.conditions_pass(
                std::slice::from_ref(&gt_zero),
                Some(SUBJECT),
                target,
                "friend_hit pass",
            ),
            expected_gt
        );
    }

    // Explicit script calls still read their explicit actor argument cleanly.
    let before = g.events.len();
    let a_hits = g
        .run_query(
            "Begin Function {}\nSetFunctionValue (OwnSubjectRef.GetFriendHit OwnActorARef)\nEnd",
            RunCtx::default(),
        )
        .unwrap();
    let b_hits = g
        .run_query(
            "Begin Function {}\nSetFunctionValue (OwnSubjectRef.GetFriendHit OwnActorBRef)\nEnd",
            RunCtx::default(),
        )
        .unwrap();
    let failed = call_failed_since(&g, before);
    assert!(
        failed.is_empty(),
        "explicit script controls produced CallFailed: {failed:?}"
    );
    assert_eq!(a_hits, XVal::Num(1.0));
    assert_eq!(b_hits, XVal::Num(2.0));

    // Separate scope: an omitted-argument script call (in_condition: false) still fails.
    let before = g.events.len();
    let script_ctx = Ctx {
        unit: "script_omitted_arg".into(),
        opcode: FRIEND_HIT_OPCODE,
        on: Some(SUBJECT),
        self_ref: Some(SUBJECT),
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    let omitted = g.call_function(&script_ctx, &[]);
    let failed = call_failed_since(&g, before);
    assert_eq!(
        omitted,
        Value::ZERO,
        "failed script call returns Value::ZERO while recording CallFailed"
    );
    assert_eq!(
        failed.len(),
        1,
        "omitted-argument script call must record CallFailed: {failed:?}"
    );
    assert!(
        failed[0].contains("parameter 1 is not a form"),
        "expected missing-form diagnostic, got {failed:?}"
    );

    // Expiry control: once past fFriendHitTimer (10 s), CTDA drops back to 0 without error.
    g.time = 20.0;
    let before = g.events.len();
    let expired = g.eval_conditions(
        std::slice::from_ref(&gt_zero),
        Some(SUBJECT),
        Some(ACTOR_B),
        "friend_hit expired",
    );
    let failed = call_failed_since(&g, before);
    assert!(failed.is_empty(), "expired check produced CallFailed: {failed:?}");
    assert_eq!(expired[0].value, 0.0);
    assert!(!expired[0].passed);
    assert_eq!(
        Value::Num(expired[0].value),
        Value::ZERO,
        "expired hits read zero"
    );
}
