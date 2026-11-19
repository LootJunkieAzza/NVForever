//! Integer-subset faction modifier controls through real source and World consumers.
use crate::common::*;
use crate::game::{GameConfig, GameSnapshot, RunCtx};
use crate::{Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const A: FormId = FormId(0x830);
const B: FormId = FormId(0x831);
const PROOF: FormId = FormId(0x840);

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn relation(to: FormId, modifier: i32, combat: u32) -> fnv_esm::raw::Subrecord {
    sub(b"XNAM", [to.0.to_le_bytes(), modifier.to_le_bytes(), combat.to_le_bytes()].concat())
}

fn fixture(winning_modifier: Option<i32>) -> Fixture {
    let actor = |id: u32, edid: &str, faction: FormId| {
        record(
            b"NPC_",
            0,
            id,
            &[sub(b"EDID", z(edid)), sub(b"SNAM", [faction.0.to_le_bytes().to_vec(), vec![0; 4]].concat())],
        )
    };
    let placed = |id: u32, base: u32, edid: &str| {
        record(
            b"ACHR",
            0,
            id,
            &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
        )
    };
    let refs = [placed(0x301, 0x801, "MemberARef"), placed(0x302, 0x802, "MemberBRef")].concat();
    let cell = [
        record(b"CELL", 0, 0x200, &[sub(b"DATA", vec![1])]),
        group(0x200u32.to_le_bytes(), 6, &group(0x200u32.to_le_bytes(), 8, &refs)),
    ]
    .concat();
    let bytes = [
        tes4(),
        top(
            b"FACT",
            &[
                record(b"FACT", 0, A.0, &[sub(b"EDID", z("FirstFaction")), relation(B, -12, 1)]),
                record(b"FACT", 0, B.0, &[sub(b"EDID", z("SecondFaction")), relation(A, 7, 2)]),
            ]
            .concat(),
        ),
        top(
            b"NPC_",
            &[
                record(b"NPC_", 0, FormId::PLAYER_BASE.0, &[sub(b"EDID", z("Player"))]),
                actor(0x801, "MemberA", A),
                actor(0x802, "MemberB", B),
            ]
            .concat(),
        ),
        top(
            b"MISC",
            &record(
                b"MISC",
                0,
                PROOF.0,
                &[
                    sub(b"EDID", z("ProofItem")),
                    sub(b"FULL", z("Authored proof item")),
                    sub(b"DATA", [1i32.to_le_bytes(), 0f32.to_le_bytes()].concat()),
                ],
            ),
        ),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat();
    let mut plugins = vec![Plugin::parse(&bytes, "ModifierBase.esm".into()).unwrap()];
    if let Some(modifier) = winning_modifier {
        let header = record(
            b"TES4",
            0,
            0,
            &[
                sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x800u32.to_le_bytes()].concat()),
                sub(b"MAST", z("ModifierBase.esm")),
                sub(b"DATA", vec![0; 8]),
            ],
        );
        let patch = [
            header,
            top(b"FACT", &record(b"FACT", 0, A.0, &[sub(b"EDID", z("FirstFaction")), relation(B, modifier, 3)])),
        ]
        .concat();
        plugins.push(Plugin::parse(&patch, "ModifierPatch.esp".into()).unwrap());
    }
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
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

fn run(g: &mut Game<'_>, body: &str, receiver: Option<FormId>) -> Result<XVal, String> {
    g.run_query(
        &format!("ref empty\nBegin Function {{}}\n{body}\nEnd\n"),
        RunCtx {
            self_ref: receiver,
            ..Default::default()
        },
    )
}

fn success(g: &mut Game<'_>, body: &str) -> XVal {
    run(g, body, Some(FormId::PLAYER_REF)).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn value(g: &mut Game<'_>, from: &str, to: &str) -> XVal {
    success(g, &format!("SetFunctionValue (GetFactionReaction {from} {to})"))
}

#[test]
fn integer_modifier_source_drives_a_real_inventory_branch_and_preserves_direction() {
    let f = fixture(None);
    let mut g = game(&f);
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(-12.0));
    assert_eq!(value(&mut g, "SecondFaction", "FirstFaction"), XVal::Num(7.0));
    assert_eq!(value(&mut g, "FirstFaction", "FirstFaction"), XVal::Num(0.0));
    success(
        &mut g,
        "if eval (GetFactionReaction FirstFaction SecondFaction) == -7\nAddItem ProofItem 2\nendif",
    );
    assert_eq!(
        g.world.item_count(FormId::PLAYER_REF, PROOF).unwrap(),
        0,
        "the branch is not an unconditional reward"
    );
    success(
        &mut g,
        "ModFactionReaction FirstFaction SecondFaction 5\nif eval (GetFactionReaction FirstFaction SecondFaction) == -7\nAddItem ProofItem 2\nendif",
    );
    assert_eq!(g.world.item_count(FormId::PLAYER_REF, PROOF).unwrap(), 2);
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(-7.0));
    assert_eq!(value(&mut g, "SecondFaction", "FirstFaction"), XVal::Num(7.0));
}

#[test]
fn setter_then_mod_then_setter_use_the_same_saved_integer_value() {
    let f = fixture(None);
    let mut g = game(&f);
    success(&mut g, "ModFactionReaction FirstFaction FirstFaction 0");
    assert!(g.forms.faction_modifiers.is_empty(), "zero creates no saved override");
    assert_eq!(run(&mut g, "ModFactionReaction FirstFaction SecondFaction 2", None).unwrap(), XVal::Num(0.0));
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(-10.0));
    success(
        &mut g,
        "SetFactionReaction FirstFaction SecondFaction 23\nModFactionReaction FirstFaction SecondFaction -3",
    );
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(20.0));
    success(
        &mut g,
        "ModFactionReaction FirstFaction SecondFaction 0\nSetFactionReaction FirstFaction SecondFaction -8\nModFactionReaction FirstFaction SecondFaction 2",
    );
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(-6.0));
    assert_eq!(g.forms.faction_modifiers[&A][&B], -6);
    assert_eq!(value(&mut g, "SecondFaction", "FirstFaction"), XVal::Num(7.0));
}

#[test]
fn disposition_modifiers_do_not_replace_combat_relations_or_actor_disposition_policy() {
    let f = fixture(None);
    let mut g = game(&f);
    let before = success(&mut g, "SetFunctionValue (MemberARef.GetDisposition PlayerRef)");
    assert_eq!(crate::functions::actor::faction_relation(&g, FormId(0x301), FormId(0x302)), 1);
    success(&mut g, "ModFactionReaction FirstFaction SecondFaction 30");
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(18.0));
    assert_eq!(crate::functions::actor::reaction(&g, A, B), 1);
    assert_eq!(crate::functions::actor::faction_relation(&g, FormId(0x301), FormId(0x302)), 1);
    assert_eq!(success(&mut g, "SetFunctionValue (MemberARef.GetDisposition PlayerRef)"), before);
    success(&mut g, "SetAlly FirstFaction SecondFaction");
    assert_eq!(crate::functions::actor::reaction(&g, A, B), 2);
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(18.0));
    success(
        &mut g,
        "MemberARef.SetDisposition PlayerRef 22\nModFactionReaction FirstFaction SecondFaction -50",
    );
    assert_eq!(success(&mut g, "SetFunctionValue (MemberARef.GetDisposition PlayerRef)"), XVal::Num(22.0));
}

#[test]
fn invalid_factions_and_explicit_empty_receiver_stop_source_without_mutation() {
    let f = fixture(None);
    let mut g = game(&f);
    success(&mut g, "SetFactionReaction FirstFaction SecondFaction 10");
    g.world.item_count(FormId::PLAYER_REF, PROOF).unwrap();
    let (saved, world) = (g.forms.faction_modifiers.clone(), g.world.snapshot());
    for call in ["ModFactionReaction ProofItem SecondFaction 1", "ModFactionReaction FirstFaction empty 1"] {
        let err = run(&mut g, &format!("{call}\nAddItem ProofItem 50"), Some(FormId::PLAYER_REF)).unwrap_err();
        assert!(err.contains("requires two faction forms"), "{err}");
        assert_eq!(g.forms.faction_modifiers, saved);
        assert_eq!(g.world.snapshot(), world);
    }
    for call in [
        "empty.ModFactionReaction FirstFaction SecondFaction 1",
        "SetFunctionValue (empty.ModFactionReaction FirstFaction SecondFaction 1)",
    ] {
        assert_eq!(
            run(
                &mut g,
                &format!("{call}\nModFactionReaction FirstFaction SecondFaction 50"),
                Some(FormId::PLAYER_REF)
            )
            .unwrap_err(),
            "modfactionreaction called on no reference"
        );
        assert_eq!(g.forms.faction_modifiers, saved);
    }
    assert_eq!(
        value(&mut g, "FirstFaction", "ProofItem"),
        XVal::Num(0.0),
        "retained getter invalid-form mapping"
    );
}

#[test]
fn fractional_nonfinite_and_out_of_range_deltas_refuse_without_rounding_or_mutation() {
    let f = fixture(None);
    let mut g = game(&f);
    success(&mut g, "SetFactionReaction FirstFaction SecondFaction 10");
    // Bare integer tokens use the VM's i32 route. Decimal expressions and exact
    // source readbacks prove that these controls pass the intended wider values.
    let upper = "(9223372036854775808.0)";
    let lower = "(-9223372036854775808.0)";
    let huge = "(1000000000000000000000000000000.0)";
    for (expression, expected) in [("0.5", 0.5), (huge, 1e30), (upper, -(i64::MIN as f64)), (lower, i64::MIN as f64)] {
        assert_eq!(success(&mut g, &format!("SetFunctionValue {expression}")), XVal::Num(expected), "source value: {expression}");
    }
    for (expression, nan) in [("(Pow -1 0.5)", true), ("(Pow 10 400)", false)] {
        let XVal::Num(value) = success(&mut g, &format!("SetFunctionValue {expression}")) else { panic!("non-numeric source value: {expression}") };
        assert!(if nan { value.is_nan() } else { value == f64::INFINITY }, "source value: {expression}: {value}");
    }
    for delta in ["0.5", huge, upper, "(Pow -1 0.5)", "(Pow 10 400)"] {
        let saved = g.forms.faction_modifiers.clone();
        let err = run(
            &mut g,
            &format!("ModFactionReaction FirstFaction SecondFaction {delta}\nSetFactionReaction FirstFaction SecondFaction 99"),
            None,
        )
        .unwrap_err();
        assert!(err.contains("integral i64 delta"), "{delta}: {err}");
        assert_eq!(g.forms.faction_modifiers, saved);
    }
    success(&mut g, &format!("ModFactionReaction FirstFaction FirstFaction {lower}"));
    assert_eq!(g.forms.faction_modifiers[&A][&A], i64::MIN, "the exact lower input bound is representable");
    // Actual stored representation's limits, not a native command limit.
    for (at, delta) in [(i64::MAX, 1), (i64::MIN, -1)] {
        g.forms.faction_modifiers.entry(A).or_default().insert(B, at);
        let err = run(&mut g, &format!("ModFactionReaction FirstFaction SecondFaction {delta}"), None).unwrap_err();
        assert!(err.contains("integer storage range"), "{err}");
        assert_eq!(g.forms.faction_modifiers[&A][&B], at);
    }
}

#[test]
fn integer_edits_round_trip_actual_json_restore_and_new_sessions_read_records_again() {
    let f = fixture(None);
    let mut g = game(&f);
    success(&mut g, "SetFactionReaction FirstFaction SecondFaction 10\nModFactionReaction FirstFaction SecondFaction -7\nModFactionReaction SecondFaction FirstFaction -2\nAddItem ProofItem 2");
    let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
    let saved: GameSnapshot = serde_json::from_slice(&bytes).unwrap();
    let world = World::restore(&f.forms, &f.refs, &saved.sim).unwrap();
    let mut back = Game::restore(&f.data, world, g.cfg, &saved).unwrap();
    assert_eq!(value(&mut back, "FirstFaction", "SecondFaction"), XVal::Num(3.0));
    assert_eq!(value(&mut back, "SecondFaction", "FirstFaction"), XVal::Num(5.0));
    assert_eq!(back.world.item_count(FormId::PLAYER_REF, PROOF).unwrap(), 2);
    success(&mut back, "ModFactionReaction FirstFaction SecondFaction 2");
    assert_eq!(value(&mut back, "FirstFaction", "SecondFaction"), XVal::Num(5.0));
    let mut fresh = game(&f);
    assert_eq!(value(&mut fresh, "FirstFaction", "SecondFaction"), XVal::Num(-12.0));
    assert_eq!(value(&mut fresh, "SecondFaction", "FirstFaction"), XVal::Num(7.0));
}

#[test]
fn winning_decoded_record_supplies_modifier_and_combat_fields_independently() {
    let f = fixture(Some(42));
    let mut g = game(&f);
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(42.0));
    assert_eq!(crate::functions::actor::reaction(&g, A, B), 3);
    success(&mut g, "ModFactionReaction FirstFaction SecondFaction -2");
    assert_eq!(value(&mut g, "FirstFaction", "SecondFaction"), XVal::Num(40.0));
    assert_eq!(crate::functions::actor::reaction(&g, A, B), 3);
    assert_eq!(value(&mut g, "SecondFaction", "FirstFaction"), XVal::Num(7.0));
}
