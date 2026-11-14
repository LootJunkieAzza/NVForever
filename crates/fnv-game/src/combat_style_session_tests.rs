//! Candidate-only internal safety controls. Injected malformed alias graphs are
//! not a claim that an installed mod produced them or a new public deletion API.
#[path = "../tests/common/mod.rs"]
mod common;
use common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use std::borrow::Cow;

const STYLE: FormId = FormId(0xA10);
const ALIAS: FormId = FormId(0xFE00_1234);

pub(crate) struct Fixture { pub data: GameData, pub forms: FormStore, pub refs: RefTable }

pub(crate) fn fixture() -> Fixture {
    let bytes = [tes4(), top(b"CSTY", &[record(b"CSTY", 0, STYLE.0, &[sub(b"EDID", z("OwnedStyle"))]),
        record(b"CSTY", 0, 0x3D, &[sub(b"EDID", z("DefaultCombatstyle"))])].concat()),
        top(b"STAT", &record(b"STAT", 0, 0xD10, &[sub(b"EDID", z("OwnedSaveNeighbor"))]))].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedAliasSafety.esm".into()).unwrap()];
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

pub(crate) fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

#[test]
fn authored_views_borrow_and_edited_views_are_independent_owned_values() {
    let f = fixture(); let mut g = game(&f);
    assert!(matches!(g.combat_style_form(STYLE), Some(Cow::Borrowed(_))));
    g.run_query("Begin Function { }\nSetCombatStyleTraitNumeric OwnedStyle 42 2\nEnd\n", Default::default()).unwrap();
    let held = g.combat_style_form(STYLE).unwrap();
    assert!(matches!(&held, Cow::Owned(_)));
    g.run_query("Begin Function { }\nSetCombatStyleTraitNumeric OwnedStyle 42 9\nEnd\n", Default::default()).unwrap();
    assert_eq!(held.rushing_attack_distance_mult, 2.0);
    assert_eq!(g.combat_style_form(STYLE).unwrap().rushing_attack_distance_mult, 9.0);
}

#[test]
fn cyclic_and_unresolved_aliases_are_rejected_by_real_trait_dispatch() {
    let f = fixture(); let mut g = game(&f);
    let other = FormId(ALIAS.0 + 1);
    for destination in [ALIAS, other] {
        g.clones.clear();
        g.clones.insert(ALIAS, destination);
        if destination == other { g.clones.insert(other, ALIAS); }
        assert!(g.combat_style_form(ALIAS).is_none());
        let args = [XVal::Ref(ALIAS.0), XVal::Num(42.0), XVal::Num(7.0)];
        assert_eq!(crate::nvse_jip4::call(&mut g, "setcombatstyletraitnumeric", None, &args).unwrap().unwrap(), XVal::Num(0.0));
        assert_eq!(crate::nvse_jip4::call(&mut g, "getcombatstyletraitnumeric", None, &args[..2]).unwrap().unwrap(), XVal::Num(0.0));
        assert!(!g.combat_style_edits.typed.contains_key(&ALIAS));
    }
    g.clones.clear();
    g.clones.insert(ALIAS, FormId(0x123456));
    assert!(g.combat_style_form(ALIAS).is_none());
    g.snapshot_combat_style(ALIAS, other);
    assert!(!g.combat_style_edits.typed.contains_key(&other));
}

#[test]
fn a_removed_alias_is_not_kept_valid_by_a_stale_typed_copy_or_explicit_assignment() {
    let f = fixture(); let mut g = game(&f);
    g.snapshot_combat_style(STYLE, ALIAS);
    g.clones.insert(ALIAS, STYLE);
    g.extra.entry(FormId::PLAYER_REF).or_default().combat_style = Some(ALIAS);
    assert_eq!(g.combat_style_id(FormId::PLAYER_REF), Some(ALIAS));
    g.clones.remove(&ALIAS); // Test-only simulated stale reference, no public API claim.
    assert!(g.combat_style_form(ALIAS).is_none());
    assert_eq!(g.combat_style_id(FormId::PLAYER_REF), None);
    assert_eq!(*g.combat_style(FormId::PLAYER_REF).unwrap(), f.data.combat_styles[&FormId(0x3D)]);
}

fn restored_roundtrip<'a>(g: &Game<'a>) -> Game<'a> {
    let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
    let snapshot: crate::game::GameSnapshot = serde_json::from_slice(&bytes).unwrap();
    let world = World::restore(g.world.forms, g.world.placed, &snapshot.sim).unwrap();
    Game::restore(g.data, world, g.cfg, &snapshot).unwrap()
}

#[test]
fn an_unassigned_style_clone_in_an_extender_array_saves_without_rewriting_its_carrier() {
    use fnv_script::vm::{ArrayKind, XKey};
    let f = fixture(); let mut g = game(&f);
    let alias = g.run_query("Begin Function { }\nSetFunctionValue (TempCloneForm OwnedStyle 0)\nEnd\n", Default::default()).unwrap();
    assert!(matches!(&alias, XVal::Ref(id) if *id != 0), "the actual source must create a style alias");
    let array = g.ext.new_array(ArrayKind::List, vec![(XKey::Num(0.0), alias.clone())]);
    let carrier = XVal::Array(array);
    g.ext.retain(&carrier);
    g.ext.sweep();
    assert_eq!(g.ext.array(array).unwrap().get(&XKey::Num(0.0)), Some(&alias));
    assert!(g.extra.values().all(|extra| extra.combat_style.is_none()));
    let back = restored_roundtrip(&g);
    assert_eq!(back.ext.array(array).unwrap().get(&XKey::Num(0.0)), Some(&alias), "saved carriers are not filtered or remapped");
    let XVal::Ref(id) = alias else { unreachable!() };
    assert!(back.combat_style_form(FormId(id)).is_none(), "temporary copies are session-only");
    assert!(g.combat_style_form(FormId(id)).is_some(), "saving does not reset the running session");
    g.ext.release(&carrier);
    assert!(g.ext.array(array).is_none());
    assert!(g.checked_snapshot().is_ok(), "an unused tracked clone must not disable saving");
}

#[test]
fn a_style_alias_carried_by_a_deleted_created_reference_is_preserved_in_the_save() {
    let f = fixture(); let mut g = game(&f);
    let value = g.run_query("Begin Function { }\nSetFunctionValue (TempCloneForm OwnedStyle 0)\nEnd\n", Default::default()).unwrap();
    let XVal::Ref(alias) = value else { panic!("a real style alias is required") };
    let reference = g.world.place(FormId(0xD10), None, [0.0; 3], [0.0; 3], 1).unwrap();
    g.extra.entry(reference).or_default().combat_style = Some(FormId(alias));
    g.world.set_deleted(reference, true).unwrap();
    let snapshot = g.checked_snapshot().unwrap();
    assert_eq!(snapshot.sim.deleted_created.get(&reference), Some(&FormId(0xD10)));
    assert!(!snapshot.sim.refs.contains_key(&reference));
    assert_eq!(snapshot.extra[&reference].combat_style, Some(FormId(alias)));
    let back = restored_roundtrip(&g);
    assert_eq!(back.extra[&reference].combat_style, Some(FormId(alias)));
    assert_eq!(back.world.snapshot().deleted_created.get(&reference), Some(&FormId(0xD10)));
    assert!(back.combat_style_form(FormId(alias)).is_none());
}

#[test]
fn retained_copy_metadata_or_aliases_do_not_turn_the_save_into_a_refusal() {
    let f = fixture();
    for remove_relation in [false, true] {
        let mut g = game(&f);
        g.snapshot_combat_style(STYLE, ALIAS);
        g.clones.insert(ALIAS, STYLE);
        if remove_relation { g.clones.remove(&ALIAS); }
        else { g.combat_style_edits.raw_clones.clear(); }
        let back = restored_roundtrip(&g);
        assert!(back.combat_style_edits.typed.is_empty());
        assert!(back.combat_style_edits.raw_clones.is_empty());
        assert!(back.clones.is_empty());
    }
}

#[test]
fn checked_style_snapshots_warn_without_clearing_session_state() {
    use std::sync::{Arc, Mutex};
    use tracing::{field::{Field, Visit}, Event, Subscriber};
    use tracing_subscriber::{layer::{Context, Layer}, prelude::*};
    #[derive(Clone, Default)]
    struct Warnings(Arc<Mutex<Vec<String>>>);
    struct Message(String);
    impl Visit for Message {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" { self.0 = format!("{value:?}"); }
        }
    }
    impl<S: Subscriber> Layer<S> for Warnings {
        fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
            let mut text = Message(String::new());
            event.record(&mut text);
            if *event.metadata().level() == tracing::Level::WARN && text.0.contains("combat-style") {
                self.0.lock().unwrap().push(text.0);
            }
        }
    }
    let f = fixture(); let mut g = game(&f);
    let warnings = Warnings::default();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(warnings.clone()), || {
        g.checked_snapshot().unwrap();
        assert!(warnings.0.lock().unwrap().is_empty(), "a clean session needs no warning");
        g.run_query("Begin Function { }\nSetCombatStyleTraitNumeric OwnedStyle 42 7\nEnd\n", Default::default()).unwrap();
        g.checked_snapshot().unwrap();
    });
    let messages = warnings.0.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("session-only") && messages[0].contains("loading resets"));
    drop(messages);
    assert_eq!(g.combat_style_form(STYLE).unwrap().rushing_attack_distance_mult, 7.0);
    assert_eq!(restored_roundtrip(&g).combat_style_form(STYLE).unwrap().rushing_attack_distance_mult, 0.0);
}
