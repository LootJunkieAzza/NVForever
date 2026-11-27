//! Lighting cold campfires on a synthetic plugin (no game data): a campfire is an
//! activator whose script names the recipe category `CampfireRecipes`; a cold one is lit
//! once (a fire and a light placed at it) and then burns; one with a fire placed beside it
//! is not lit; an activator without the category is no campfire; with the fire's or the
//! light's form missing nothing is placed; with the switch off nothing is placed.
//! docs/design/campfires.md, `fnv_game::campfire`.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const CATEGORY: u32 = 0x810;
const CAMP_SCRIPT: u32 = 0x820;
const OTHER_SCRIPT: u32 = 0x821;
const CAMPFIRE: u32 = 0x830;
const LEVER: u32 = 0x831;
const FIRE: u32 = 0x840;
const LIGHT: u32 = 0x841;
const PLACED_FIRE: u32 = 0x842;
const BARREL: u32 = 0x843;
const LAMP: u32 = 0x844;
/// A cold campfire, with a barrel beside it and a fire too far off to be its own.
const COLD: FormId = FormId(0x501);
/// A campfire with a fire placed at it.
const BURNING: FormId = FormId(0x502);
/// A campfire with a light placed at it.
const LAMPLIT: FormId = FormId(0x505);
/// An activator whose script names no recipe category.
const LEVER_REF: FormId = FormId(0x504);
const ON_ACTIVATE: u16 = 2;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn refr(fid: u32, base: u32, pos: [f32; 3]) -> Vec<u8> {
    record(b"REFR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", f32s(&[pos[0], pos[1], pos[2], 0.0, 0.0, 0.5]))])
}

fn scpt(fid: u32, edid: &str, refs: &[u32]) -> Vec<u8> {
    let body = script(&[(ON_ACTIVATE, None, Vec::new())]);
    record(b"SCPT", 0, fid, &[vec![sub(b"EDID", z(edid))], unit(0, &body, &[], refs)].concat())
}

fn base(sig: &[u8; 4], fid: u32, edid: &str, model: &str, script: Option<u32>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"MODL", z(model))];
    if let Some(s) = script {
        subs.push(sub(b"SCRI", s.to_le_bytes().to_vec()));
    }
    record(sig, 0, fid, &subs)
}

/// The plugin; `with_forms` false leaves out the fire and the light placed at a cold
/// campfire.
fn plugin_bytes(with_forms: bool, with_fire: bool) -> Vec<u8> {
    let category = record(b"RCCT", 0, CATEGORY, &[sub(b"EDID", z("CampfireRecipes")), sub(b"FULL", z("Campfire")), sub(b"DATA", vec![0])]);
    let scripts = [scpt(CAMP_SCRIPT, "CraftingCampfireRecipesScript", &[CATEGORY]), scpt(OTHER_SCRIPT, "LeverScript", &[])].concat();
    let actis = [
        base(b"ACTI", CAMPFIRE, "CampfireCrafting01", "Clutter\\Camps\\Campfire01.NIF", Some(CAMP_SCRIPT)),
        base(b"ACTI", LEVER, "TestLever", "Clutter\\Lever01.NIF", Some(OTHER_SCRIPT)),
    ]
    .concat();
    let mut statics = vec![
        base(b"MSTT", PLACED_FIRE, "FXFireMed01", "Effects\\Ambient\\FXFireMed01.NIF", None),
        base(b"MSTT", BARREL, "Barrel01", "Clutter\\Barrel01.NIF", None),
    ];
    let mut lights = vec![base(b"LIGH", LAMP, "LampLight", "", None)];
    if with_forms {
        if with_fire {
            statics.push(base(b"MSTT", FIRE, "FXFireSmall01", "Effects\\Ambient\\FXFireSmall01.NIF", None));
        }
        lights.push(base(b"LIGH", LIGHT, "FireBrightOrange", "", None));
    }
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [
        refr(COLD.0, CAMPFIRE, [0.0, 0.0, 0.0]),
        refr(0x506, BARREL, [40.0, 0.0, 0.0]),
        refr(0x507, PLACED_FIRE, [0.0, 400.0, 0.0]),
        refr(BURNING.0, CAMPFIRE, [1000.0, 0.0, 0.0]),
        refr(0x503, PLACED_FIRE, [1050.0, 20.0, 5.0]),
        refr(LAMPLIT.0, CAMPFIRE, [3000.0, 0.0, 0.0]),
        refr(0x508, LAMP, [3000.0, 100.0, 60.0]),
        refr(LEVER_REF.0, LEVER, [2000.0, 0.0, 0.0]),
    ]
    .concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"SCPT", &scripts),
        top(b"ACTI", &actis),
        top(b"LIGH", &lights.concat()),
        top(b"MSTT", &statics.concat()),
        top(b"RCCT", &category),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture(with_forms: bool) -> Fixture {
    fixture_of(with_forms, with_forms)
}

/// The game's data has the fire and the light when `data_forms`; the world has the light
/// always and the fire only when `world_forms`.
fn fixture_of(data_forms: bool, world_forms: bool) -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(data_forms, true), "Campfires.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let world_plugins = vec![Plugin::parse(&plugin_bytes(true, world_forms), "Campfires.esm".into()).unwrap()];
    let world_lo = LoadOrder::new(&world_plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&world_lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g.light_campfires = true;
    g
}

fn created(g: &Game) -> Vec<FormId> {
    g.world.created().map(|(id, _)| id).collect()
}

#[test]
fn a_cold_campfire_is_lit_once_and_then_burns() {
    let f = fixture(true);
    let mut g = game(&f);
    assert!(g.is_campfire(COLD));
    assert!(!g.campfire_burns(COLD), "a barrel beside it and a fire 400 units off do not burn at it");
    let lit = g.light_campfire(COLD).expect("lit");
    assert_eq!(lit.len(), 2);
    let at = g.world.peek(COLD).map(|s| (s.cell, s.pos, s.rot)).unwrap();
    let bases: Vec<FormId> = lit.iter().map(|&r| g.world.peek(r).unwrap().base).collect();
    assert_eq!(bases, [FormId(FIRE), FormId(LIGHT)], "the fire, then its light");
    for &r in &lit {
        let s = g.world.peek(r).unwrap();
        assert_eq!((s.cell, s.pos, s.rot), at, "placed at the campfire as PlaceAtMe places");
        assert!(s.enabled() && !s.deleted());
    }
    assert!(g.campfire_burns(COLD), "the fire placed burns at it");
    assert_eq!(g.light_campfire(COLD), None, "a burning campfire is not lit again");
    assert_eq!(created(&g), [lit[1], lit[0]], "nothing more placed (the light was made first)");
}

#[test]
fn a_campfire_with_a_fire_or_a_light_beside_it_is_not_lit() {
    let f = fixture(true);
    let mut g = game(&f);
    for r in [BURNING, LAMPLIT] {
        assert!(g.is_campfire(r));
        assert!(g.campfire_burns(r), "{r}");
        assert_eq!(g.light_campfire(r), None, "{r}");
    }
    assert!(created(&g).is_empty());
}

#[test]
fn an_activator_without_the_category_is_no_campfire() {
    let f = fixture(true);
    let mut g = game(&f);
    assert!(!g.is_campfire(LEVER_REF));
    assert!(!g.is_campfire(FormId(0x506)), "a moveable static is no campfire");
    assert_eq!(g.light_campfire(LEVER_REF), None);
    assert!(created(&g).is_empty());
}

#[test]
fn without_the_fire_and_light_forms_nothing_is_placed() {
    let f = fixture(false);
    let mut g = game(&f);
    assert!(g.is_campfire(COLD) && !g.campfire_burns(COLD));
    assert_eq!(g.light_campfire(COLD), None);
    assert!(created(&g).is_empty());
}

#[test]
fn with_the_switch_off_nothing_is_placed() {
    let f = fixture(true);
    let mut g = game(&f);
    g.light_campfires = false;
    assert_eq!(g.light_campfire(COLD), None);
    assert!(created(&g).is_empty());
    g.light_campfires = true;
    assert!(g.light_campfire(COLD).is_some());
}

#[test]
fn a_fire_that_fails_to_place_leaves_no_light_and_the_campfire_cold() {
    // The data names both forms, the world can place the light but not the fire: the
    // light placed first is taken back, nothing is lit.
    let f = fixture_of(true, false);
    let mut g = game(&f);
    assert!(g.is_campfire(COLD) && !g.campfire_burns(COLD));
    assert_eq!(g.light_campfire(COLD), None);
    assert!(!g.campfire_burns(COLD), "no fire counted without its light");
    assert_eq!(g.world.created().count(), 1, "the light was placed first");
    assert!(g.world.created().all(|(_, s)| s.deleted()), "and taken back");
}
