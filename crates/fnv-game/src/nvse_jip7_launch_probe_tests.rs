//! Authored lease controls; no installed records or native observations.
use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const PROJ: FormId = FormId(0x900);
const OTHER: FormId = FormId(0x901);
struct Fixture { data: GameData, forms: FormStore, refs: RefTable }
fn fixture() -> Fixture {
    let mut d = vec![0; 84];
    d[2..4].copy_from_slice(&1u16.to_le_bytes());
    d[8..12].copy_from_slice(&1000f32.to_le_bytes());
    d[52..56].copy_from_slice(&10f32.to_le_bytes());
    let bytes = [tes4(), top(b"PROJ", &record(b"PROJ", 0, PROJ.0, &[sub(b"EDID", z("LeaseProjectile")), sub(b"DATA", d)])),
        top(b"STAT", &record(b"STAT", 0, OTHER.0, &[sub(b"EDID", z("LeaseNeighbor"))]))].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedLaunchLease.esm".into()).unwrap()];
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
fn key(path: &str) -> (FormId, String) { (PROJ, format!("DATA/{path}")) }
fn edit(g: &mut Game<'_>) {
    g.run_query("Begin Function {}\nSetProjectileTraitNumeric LeaseProjectile 2 500\nSetProjectileTraitNumeric LeaseProjectile 9 7\nEnd\n", Default::default()).unwrap();
}

#[test]
fn absent_entries_are_removed_after_real_setters_without_touching_neighbor_fields() {
    let f = fixture(); let mut g = game(&f);
    g.field_overrides.insert((OTHER, "DATA/speed".into()), XVal::Num(33.0));
    g.field_overrides.insert(key("gravity"), XVal::Num(42.0));
    let mut lease = g.preserve_projectile_launch_fields(PROJ).unwrap();
    assert_eq!(lease.effective_values(), [1000.0, 10.0]);
    assert!(!g.field_overrides.contains_key(&key("speed")));
    edit(&mut g); assert_eq!(g.field_overrides[&key("speed")], XVal::Num(500.0));
    lease.restore(&mut g).unwrap();
    assert!(!g.field_overrides.contains_key(&key("speed")));
    assert!(!g.field_overrides.contains_key(&key("impact_force")));
    assert_eq!(g.field_overrides[&key("gravity")], XVal::Num(42.0));
    assert_eq!(g.field_overrides[&(OTHER, "DATA/speed".into())], XVal::Num(33.0));
    assert_eq!([g.num_field(PROJ, "DATA", "speed"), g.num_field(PROJ, "DATA", "impact_force")], [1000.0, 10.0]);
}

#[test]
fn present_entries_restore_exact_values_and_presence_including_negative_zero() {
    let f = fixture(); let mut g = game(&f);
    g.field_overrides.insert(key("speed"), XVal::Num(250.5));
    g.field_overrides.insert(key("impact_force"), XVal::Num(-0.0));
    let mut lease = g.preserve_projectile_launch_fields(PROJ).unwrap();
    edit(&mut g); lease.restore(&mut g).unwrap();
    assert_eq!(g.field_overrides[&key("speed")], XVal::Num(250.5));
    assert!(matches!(g.field_overrides.get(&key("impact_force")), Some(XVal::Num(n)) if n.to_bits()==(-0.0f64).to_bits()));
}

#[test]
fn malformed_prior_values_are_preserved_verbatim_while_launch_keeps_typed_fallback() {
    let f = fixture(); let mut g = game(&f);
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    g.field_overrides.insert(key("speed"), XVal::Str("authored prior entry".into()));
    g.field_overrides.insert(key("impact_force"), XVal::Num(nan));
    let mut lease = g.preserve_projectile_launch_fields(PROJ).unwrap();
    assert_eq!(lease.effective_values()[0], 0.0);
    assert_eq!(g.projectile_launch_traits(PROJ).unwrap().speed, None);
    assert_eq!(g.projectile_launch_traits(PROJ).unwrap().impact_force, None);
    edit(&mut g); lease.restore(&mut g).unwrap();
    assert_eq!(g.field_overrides[&key("speed")], XVal::Str("authored prior entry".into()));
    assert!(matches!(g.field_overrides.get(&key("impact_force")), Some(XVal::Num(n)) if n.to_bits()==nan.to_bits()));
}

#[test]
fn wrong_owner_rejection_preserves_token_and_both_games_until_actual_owner_restores() {
    let f = fixture(); let mut a = game(&f); let mut b = game(&f);
    let mut lease = a.preserve_projectile_launch_fields(PROJ).unwrap(); edit(&mut a); edit(&mut b);
    assert!(!lease.belongs_to(&b)); assert!(lease.restore(&mut b).is_err());
    assert_eq!(b.field_overrides[&key("speed")], XVal::Num(500.0));
    assert_eq!(a.field_overrides[&key("speed")], XVal::Num(500.0));
    lease.restore(&mut a).unwrap(); assert!(!a.field_overrides.contains_key(&key("speed")));
    assert_eq!(b.field_overrides[&key("speed")], XVal::Num(500.0));
}

#[test]
fn moving_game_and_token_retains_owner_while_repeat_restore_cannot_erase_newer_edits() {
    let f = fixture(); let mut a = game(&f);
    let lease = a.preserve_projectile_launch_fields(PROJ).unwrap(); edit(&mut a);
    let (mut moved, mut token) = (Box::new(a), Box::new(lease));
    assert!(token.belongs_to(&moved)); token.restore(&mut moved).unwrap();
    edit(&mut moved); assert!(token.restore(&mut moved).is_err());
    assert_eq!(moved.field_overrides[&key("speed")], XVal::Num(500.0));
}

#[test]
fn dropping_owner_cannot_rebind_its_live_token_to_a_new_game() {
    let f = fixture(); let mut old = game(&f);
    let mut token = old.preserve_projectile_launch_fields(PROJ).unwrap(); edit(&mut old); drop(old);
    let mut replacement = game(&f);
    assert!(!token.belongs_to(&replacement)); assert!(token.restore(&mut replacement).is_err());
    assert!(!replacement.field_overrides.contains_key(&key("speed")));
    drop(token); assert!(!replacement.field_overrides.contains_key(&key("speed")));
}

#[test]
fn dropping_unused_token_has_no_hidden_field_write_and_nonprojectiles_are_refused() {
    let f = fixture(); let mut g = game(&f);
    let token = g.preserve_projectile_launch_fields(PROJ).unwrap(); edit(&mut g); drop(token);
    assert_eq!(g.field_overrides[&key("speed")], XVal::Num(500.0));
    assert!(g.preserve_projectile_launch_fields(OTHER).is_none());
    assert!(g.preserve_projectile_launch_fields(FormId(0)).is_none());
}
