//! Fidgets of people standing with their weapon away (`FidgetKind::WeaponAway`, #2026)
//! on a plugin built in memory (no game data): an idle tree of the people's shape, its
//! branches gated by the weapon away, a faction for each branch and random percentages
//! that always or never pass. The rules and their sources: docs/spec-notes/idles.md §11.4.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::idles::FidgetKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x900u32.to_le_bytes()].concat();
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 4], &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

/// Comparisons of a CTDA's type byte (its top three bits).
const EQ: u8 = 0x00;
const NE: u8 = 0x20;
const LT: u8 = 0x80;

/// A condition `function(param) <cmp> value` on the subject (the 28-byte CTDA form).
fn ctda(cmp: u8, function: u16, param: u32, value: f32) -> Subrecord {
    let mut d = vec![cmp, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(param.to_le_bytes());
    d.extend([0u8; 12]);
    sub(b"CTDA", d)
}

/// Condition function indices.
const GET_IS_ID: u16 = 72;
const GET_IN_FACTION: u16 = 71;
const GET_RANDOM_PERCENT: u16 = 77;
const IS_WEAPON_OUT: u16 = 101;
const GET_CURRENT_AI_PROCEDURE: u16 = 143;

/// An IDLE below `parent` (0: a root), after `previous` in its chain: an inner node
/// (naming the people's folder) or a leaf (naming a file).
fn idle(fid: u32, edid: &str, file: bool, parent: u32, previous: u32, conds: Vec<Subrecord>) -> Vec<u8> {
    let model = if file { "Characters\\_Male\\IdleAnims\\TestLeaf.kf" } else { "Characters\\_Male\\IdleAnims" };
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"MODL", z(model))];
    subs.extend(conds);
    subs.extend([sub(b"ANAM", [parent.to_le_bytes(), previous.to_le_bytes()].concat()), sub(b"DATA", vec![7, 0, 0, 0, 0, 0, 0, 0])]);
    record(b"IDLE", fid, &subs)
}

const F_SMOKER: u32 = 0x901;
const F_GENERAL: u32 = 0x902;

/// The tree. A root wanting the weapon away (`!= 1`) and the eat procedure, with a
/// drinking leaf; then a root over an NPC-only node with a weapon-out branch and a
/// weapon-away one (`== 0`): a leaf whose random percentage never passes, a smoking leaf
/// for the smokers' faction, a general node for the general faction with two leaves.
const R_EAT: u32 = 0xA01;
const L_DRINK: u32 = 0xA02;
const R_GENERAL: u32 = 0xA10;
const N_NPC: u32 = 0xA11;
const N_WEAPON: u32 = 0xA12;
const L_WEAPON: u32 = 0xA13;
const N_AWAY: u32 = 0xA14;
const L_NEVER: u32 = 0xA15;
const L_SMOKE: u32 = 0xA16;
const N_FIDGETS: u32 = 0xA17;
const L_NAILS: u32 = 0xA18;
const L_SCRATCH: u32 = 0xA19;

const P_EAT: u32 = 0xB01;

/// Actor bases and their placed references (base + 0x100): in both factions, the
/// smokers' only, the general one only, in neither.
const BOTH: u32 = 0xC01;
const SMOKER: u32 = 0xC02;
const GENERAL: u32 = 0xC03;
const NEITHER: u32 = 0xC04;
const CELL: u32 = 0xE01;

fn plugin_bytes() -> Vec<u8> {
    let always = || ctda(LT, GET_RANDOM_PERCENT, 0, 100.0);
    let idles = [
        idle(R_EAT, "TestEatRoot", false, 0, 0, vec![ctda(NE, IS_WEAPON_OUT, 0, 1.0), ctda(EQ, GET_CURRENT_AI_PROCEDURE, 0, 10.0)]),
        idle(L_DRINK, "TestDrink", true, R_EAT, 0, vec![]),
        idle(R_GENERAL, "TestGeneralRoot", false, 0, R_EAT, vec![]),
        idle(N_NPC, "TestNpcOnly", false, R_GENERAL, 0, vec![ctda(EQ, GET_IS_ID, 0x7, 0.0)]),
        idle(N_WEAPON, "TestWeaponOut", false, N_NPC, 0, vec![ctda(EQ, IS_WEAPON_OUT, 0, 1.0)]),
        idle(L_WEAPON, "TestWeaponLeaf", true, N_WEAPON, 0, vec![]),
        idle(N_AWAY, "TestWeaponAway", false, N_NPC, N_WEAPON, vec![ctda(EQ, IS_WEAPON_OUT, 0, 0.0)]),
        idle(L_NEVER, "TestNever", true, N_AWAY, 0, vec![ctda(LT, GET_RANDOM_PERCENT, 0, 0.0)]),
        idle(L_SMOKE, "TestSmoke", true, N_AWAY, L_NEVER, vec![ctda(EQ, GET_IN_FACTION, F_SMOKER, 1.0), always()]),
        idle(N_FIDGETS, "TestFidgets", false, N_AWAY, L_SMOKE, vec![ctda(EQ, GET_IN_FACTION, F_GENERAL, 1.0), always()]),
        idle(L_NAILS, "TestNails", true, N_FIDGETS, 0, vec![always()]),
        idle(L_SCRATCH, "TestScratch", true, N_FIDGETS, L_NAILS, vec![]),
    ]
    .concat();
    let factions = [F_SMOKER, F_GENERAL].map(|f| record(b"FACT", f, &[sub(b"EDID", z(&format!("TestFaction{f:X}"))), sub(b"DATA", vec![0, 0, 0, 0])])).concat();
    // An Eat package (PKDT type 3), at the editor location.
    let eat = record(
        b"PACK",
        P_EAT,
        &[
            sub(b"EDID", z("TestEat")),
            sub(b"PKDT", [&0u32.to_le_bytes()[..], &[3, 0], &[0; 6]].concat()),
            sub(b"PLDT", [0i32.to_le_bytes(), 0u32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
            sub(b"PSDT", [&[0xFF, 0xFF, 0, 0xFF][..], &0u32.to_le_bytes()].concat()),
        ],
    );
    let snam = |f: u32| sub(b"SNAM", [f.to_le_bytes().to_vec(), vec![0, 0, 0, 0]].concat());
    let npcs = [
        record(b"NPC_", 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", BOTH, &[sub(b"EDID", z("TestBoth")), snam(F_SMOKER), snam(F_GENERAL)]),
        record(b"NPC_", SMOKER, &[sub(b"EDID", z("TestSmoker")), snam(F_SMOKER)]),
        record(b"NPC_", GENERAL, &[sub(b"EDID", z("TestGeneral")), snam(F_GENERAL)]),
        record(b"NPC_", NEITHER, &[sub(b"EDID", z("TestNeither"))]),
    ]
    .concat();
    let placed = [BOTH, SMOKER, GENERAL, NEITHER].map(|b| record(b"ACHR", b + 0x100, &[sub(b"NAME", b.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])).concat();
    let cell = [record(b"CELL", CELL, &[sub(b"EDID", z("TestCell"))]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &placed))].concat();
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"NPC_", &npcs),
        top(b"IDLE", &idles),
        top(b"PACK", &eat),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Holstered.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, package_interval: Some(10.0), ..GameConfig::default() })
}

const FOLDER: &str = "characters\\_male";

/// The weapon-away fidget of the placed reference of `base`, nothing held back.
fn away(g: &mut Game, base: u32) -> Option<u32> {
    g.fidget(FormId(base + 0x100), FOLDER, FidgetKind::WeaponAway, &|_| false).map(|i| i.0)
}

#[test]
fn the_factions_decide_who_fidgets_with_the_weapon_away() {
    let f = fixture();
    let mut g = game(&f);
    // The never-passing leaf is passed over; the smokers' leaf comes before the general
    // node, so an actor in both smokes.
    assert_eq!(away(&mut g, BOTH), Some(L_SMOKE));
    assert_eq!(away(&mut g, SMOKER), Some(L_SMOKE));
    assert_eq!(away(&mut g, GENERAL), Some(L_NAILS), "the general node's first leaf");
    assert_eq!(away(&mut g, NEITHER), None, "in neither faction: the branch's children all fail");
    // The player is not walked for these (and the tree's NPC-only node would keep it out).
    assert_eq!(g.fidget(FormId::PLAYER_REF, FOLDER, FidgetKind::WeaponAway, &|_| false), None);
    // Another folder: nothing.
    assert_eq!(g.fidget(FormId(BOTH + 0x100), "creatures\\dog", FidgetKind::WeaponAway, &|_| false), None);
}

#[test]
fn a_held_back_leaf_gives_way_to_the_next_branch() {
    let f = fixture();
    let mut g = game(&f);
    let smoke_held = |i: FormId| i == FormId(L_SMOKE);
    assert_eq!(g.fidget(FormId(BOTH + 0x100), FOLDER, FidgetKind::WeaponAway, &smoke_held).map(|i| i.0), Some(L_NAILS));
    assert_eq!(g.fidget(FormId(SMOKER + 0x100), FOLDER, FidgetKind::WeaponAway, &smoke_held), None);
    let nails_held = |i: FormId| i == FormId(L_NAILS);
    assert_eq!(g.fidget(FormId(GENERAL + 0x100), FOLDER, FidgetKind::WeaponAway, &nails_held).map(|i| i.0), Some(L_SCRATCH));
}

#[test]
fn the_weapon_out_takes_the_other_branch() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(BOTH + 0x100);
    g.set_weapon_out(actor, true);
    // The weapon-out branch's leaf is the walk's pick: no weapon-away fidget.
    assert_eq!(g.fidget(actor, FOLDER, FidgetKind::WeaponAway, &|_| false), None);
    assert_eq!(g.fidget(actor, FOLDER, FidgetKind::WeaponOut, &|_| false), Some(FormId(L_WEAPON)));
    g.set_weapon_out(actor, false);
    assert_eq!(g.fidget(actor, FOLDER, FidgetKind::WeaponOut, &|_| false), None, "the weapon away: no drawn-weapon fidget");
    assert_eq!(g.fidget(actor, FOLDER, FidgetKind::WeaponAway, &|_| false), Some(FormId(L_SMOKE)));
}

#[test]
fn an_actor_eating_drinks_whatever_its_factions() {
    let f = fixture();
    let mut g = game(&f);
    let actor = FormId(NEITHER + 0x100);
    assert_eq!(away(&mut g, NEITHER), None);
    let ctx = Ctx { unit: "test".into(), opcode: 0x1097, on: Some(actor), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[ArgValue::Value(Value::Ref(P_EAT))]);
    assert_eq!(g.extra[&actor].current_package, Some(FormId(P_EAT)));
    // The eat root comes first in the chain and wants the eat procedure (10).
    assert_eq!(away(&mut g, NEITHER), Some(L_DRINK));
    // With the weapon out the eat root's `IsWeaponOut != 1` fails.
    g.set_weapon_out(actor, true);
    assert_eq!(away(&mut g, NEITHER), None);
}
