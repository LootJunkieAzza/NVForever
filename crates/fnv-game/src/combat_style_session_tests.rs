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

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let bytes = [tes4(), top(b"CSTY", &[record(b"CSTY", 0, STYLE.0, &[sub(b"EDID", z("OwnedStyle"))]),
        record(b"CSTY", 0, 0x3D, &[sub(b"EDID", z("DefaultCombatstyle"))])].concat())].concat();
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

fn game(f: &Fixture) -> Game<'_> {
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
