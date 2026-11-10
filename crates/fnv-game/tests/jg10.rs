//! JohnnyGuitar NVSE batch 10 (#1007; docs/spec-notes/ext-jg10.md) on a synthetic
//! plugin: navmesh obstacles, triangles and paths, weapon scopes, FaceGen values, body
//! part highlights, the land texture underfoot and landscape remaps, the barber menu's
//! lists, the screen splatter's textures and the requests kept for the engine.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nav_ask::NavAsk;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const NPC: u32 = 0x910;
const WEAP: u32 = 0x911;
const FLST: u32 = 0x912;
const HAIR_A: u32 = 0x913;
const HAIR_B: u32 = 0x914;
const ALCH: u32 = 0x915;

fn edid(sig: &[u8; 4], fid: u32, name: &str) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"EDID", z(name))])
}

fn plugin_bytes() -> Vec<u8> {
    let fggs: Vec<u8> = (0..50).flat_map(|i| (i as f32 * 0.5).to_le_bytes()).collect();
    let npc = record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestFace")), sub(b"FGGS", fggs)]);
    let weap = record(b"WEAP", 0, WEAP, &[sub(b"EDID", z("TestScoped")), sub(b"MOD3", z("interface\\hud\\testscope.nif"))]);
    let flst = record(b"FLST", 0, FLST, &[sub(b"EDID", z("TestHairList")), sub(b"LNAM", HAIR_A.to_le_bytes().to_vec())]);
    [
        tes4(),
        top(b"NPC_", &[edid(b"NPC_", 0x7, "Player"), npc].concat()),
        top(b"WEAP", &weap),
        top(b"FLST", &flst),
        top(b"HAIR", &[edid(b"HAIR", HAIR_A, "TestHairA"), edid(b"HAIR", HAIR_B, "TestHairB")].concat()),
        top(b"ALCH", &edid(b"ALCH", ALCH, "TestPoison")),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Jg10.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        v => panic!("{expr}: {v:?}"),
    }
}

/// A navmesh of one triangle whose centre is (10, 20, 30).
struct OneTriangle;
impl NavAsk for OneTriangle {
    fn path(&self, _from: [f32; 3], _to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        None
    }
    fn on_navmesh(&self, _at: [f32; 3], _near: Option<u32>) -> bool {
        false
    }
    fn cover(&self, _at: [f32; 3], _radius: f32) -> Vec<([f32; 3], u32)> {
        Vec::new()
    }
    fn triangle(&self, at: [f32; 3], under: bool, _skip: bool, _z: Option<f32>) -> Option<[f32; 3]> {
        (!under || (at[0] - 10.0).abs() < 5.0).then_some([10.0, 20.0, 30.0])
    }
}

#[test]
fn navmeshes_and_paths() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: AddNavmeshObstacle, RemoveNavmeshobstacle on the calling reference].
    assert_eq!(n(&mut g, "PlayerREF.AddNavmeshObstacle"), 1.0);
    assert!(g.script_nav_obstacles().contains(&FormId::PLAYER_REF));
    assert_eq!(n(&mut g, "PlayerREF.RemoveNavmeshobstacle"), 1.0);
    assert!(g.script_nav_obstacles().is_empty());
    // No answerer (headless): an empty array [hypothesis].
    assert_eq!(n(&mut g, "Ar_Size (GetNearestNavMeshTriangle 0 0 0 1)"), 0.0);
    // [documented: x, y, z and the distance].
    let near = fnv_game::nav_ask::with_nav(&OneTriangle, || n(&mut g, "Ar_Size (GetNearestNavMeshTriangle 10 20 0 1 100)"));
    assert_eq!(near, 4.0);
    let d = fnv_game::nav_ask::with_nav(&OneTriangle, || n(&mut g, "(GetNearestNavMeshTriangle 10 20 0 1)[3]"));
    assert_eq!(d, 30.0);
    let outside = fnv_game::nav_ask::with_nav(&OneTriangle, || n(&mut g, "Ar_Size (GetPointInNavMesh 500 20 0 1)"));
    assert_eq!(outside, 0.0);
    // [documented: PathToPoint, PathToRef; hypothesis: 1 when asked].
    assert_eq!(n(&mut g, "PlayerREF.PathToPoint 1 2 3 50"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.PathToRef TestFace"), 0.0, "a base form is no place");
    let paths = g.take_path_requests();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].to, fnv_game::nvse_jg10::PathTarget::Point([1.0, 2.0, 3.0]));
    assert_eq!(paths[0].radius, 50.0);
}

#[test]
fn weapons_and_scopes() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: DisableMuzzleFlashLights' modes].
    assert!(g.muzzle_light_on(FormId::PLAYER_REF));
    q(&mut g, "DisableMuzzleFlashLights 2");
    assert!(!g.muzzle_light_on(FormId::PLAYER_REF));
    assert!(g.muzzle_light_on(FormId(0x1234)));
    q(&mut g, "DisableMuzzleFlashLights 3");
    assert!(g.muzzle_light_on(FormId::PLAYER_REF) && !g.muzzle_light_on(FormId(0x1234)));
    q(&mut g, "DisableMuzzleFlashLights 9");
    assert!(!g.muzzle_light_on(FormId(0x1234)), "a mode outside 0-3 changes nothing");
    q(&mut g, "DisableMuzzleFlashLights 0");
    assert!(g.muzzle_light_on(FormId(0x1234)));
    // [documented: the weapon's own scope when a weapon is given].
    q(&mut g, "SetWeaponScopeUIModel \"ignored.nif\" TestScoped");
    assert_eq!(g.script_scope(), (Some("interface\\hud\\testscope.nif"), None));
    q(&mut g, "ToggleWeaponScopeUIModel 0");
    assert_eq!(g.script_scope().1, Some(false));
    q(&mut g, "ClearWeaponScopeUIModel");
    assert_eq!(g.script_scope(), (Some(""), Some(false)));
    // [documented: RemoveScopeModelPath, "isSuccess"].
    assert_eq!(n(&mut g, "RemoveScopeModelPath TestScoped"), 1.0);
    assert_eq!(n(&mut g, "RemoveScopeModelPath TestScoped"), 0.0, "nothing left to remove");
    // A poison only on a weapon (the player has none equipped here).
    q(&mut g, "PlayerREF.ApplyWeaponPoison TestPoison");
    assert_eq!(g.weapon_poison(FormId::PLAYER_REF), None);
    q(&mut g, "PlayerREF.EjectCasing \"ShellNode\"");
    let c = g.take_casing_requests();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].node.as_deref(), c[0].model.as_deref()), (Some("ShellNode"), None));
}

#[test]
fn faces_highlights_land_and_menus() {
    let f = fixture();
    let mut g = game(&f);
    // [documented: the arguments; hypothesis: type 0 is FGGS].
    assert_eq!(n(&mut g, "FaceGenGetNthProperty TestFace 0 4"), 2.0);
    assert_eq!(n(&mut g, "FaceGenGetNthProperty TestFace 0 50"), 0.0);
    assert_eq!(n(&mut g, "FaceGenGetNthProperty TestFace 7 0"), 0.0);
    assert_eq!(n(&mut g, "FaceGenSetNthProperty TestFace 0 4 1.25"), 1.0);
    assert_eq!(n(&mut g, "FaceGenGetNthProperty TestFace 0 4"), 1.25);
    assert_eq!(n(&mut g, "FaceGenSetNthProperty TestFace 1 30 1"), 0.0, "FGGA holds 30");
    q(&mut g, "PlayerREF.FaceGenRefreshAppearance");
    assert_eq!(g.take_appearance_changes().len(), 1);
    // [documented: one actor at a time; no part is everything but the weapon].
    q(&mut g, "PlayerREF.HighlightBodyPartAlt");
    assert_eq!(g.body_part_highlight(), Some((FormId::PLAYER_REF, -1)));
    q(&mut g, "PlayerREF.HighlightBodyPartAlt 1");
    assert_eq!(g.body_part_highlight(), Some((FormId::PLAYER_REF, 1)));
    q(&mut g, "PlayerREF.HighlightBodyPartAlt 16");
    assert_eq!(g.body_part_highlight(), Some((FormId::PLAYER_REF, 1)), "no part 16");
    q(&mut g, "PlayerREF.DeactivateAllHighlightsAlt");
    assert_eq!(g.body_part_highlight(), None);
    // The land texture from the engine's cast; none without one.
    assert_eq!(n(&mut g, "PlayerREF.GetLandTextureUnderFeet"), 0.0);
    let caster = |a: [f32; 3], _b: [f32; 3], k: fnv_game::ray::CastKind| {
        (k == fnv_game::ray::CastKind::LandTexture).then_some(fnv_game::ray::Hit { distance: 64.0, point: [a[0], a[1], 0.0], form: Some(0xABC), material: None })
    };
    q(&mut g, "player.SetPos X 0
player.SetPos Y 0
player.SetPos Z 0");
    assert_eq!(fnv_game::ray::with_caster(&caster, || n(&mut g, "PlayerREF.GetLandTextureUnderFeet")), f64::from(0xABC));
    // No land under it any more (indoors): the last one found [observed: an LTEX indoors].
    assert_eq!(n(&mut g, "PlayerREF.GetLandTextureUnderFeet"), f64::from(0xABC));
    // [documented: RemapLand's 0/1; hypothesis: a source cell that is not there fails].
    assert_eq!(n(&mut g, "RemapLand 4096 TestFace 0 0"), 0.0);
    assert_eq!(n(&mut g, "RemapLand 4096"), 0.0, "nothing to put back");
    // [documented: ShowBarberMenuEx 1, beards blacklisted and hairs whitelisted].
    q(&mut g, "ShowBarberMenuEx 1 TestHairList");
    assert_eq!(g.open_menu_code(), Some(fnv_game::game::menus::RACE));
    assert!(g.barber_allows(FormId(HAIR_A), false));
    assert!(!g.barber_allows(FormId(HAIR_B), false));
    assert!(g.barber_allows(FormId(HAIR_B), true), "a beard not on the blacklist");
    // [documented: TriggerScreenSplatterEx's textures; hypothesis: 1 when splats come].
    assert_eq!(n(&mut g, "TriggerScreenSplatterEx 12 5 1 1 \"a.dds\" \"c.dds\" \"f.dds\" 1"), 1.0);
    assert_eq!(g.ui.splatter.triggers.len(), 1);
    assert_eq!(g.splatter_textures().map(|t| (t.alpha.as_str(), t.fade)), Some(("a.dds", true)));
    assert_eq!(n(&mut g, "TriggerScreenSplatterEx 0 5 1 1 \"a.dds\" \"c.dds\" \"f.dds\" 0"), 0.0);
    q(&mut g, "UpdateCrosshairPrompt\nUpdateRepairMenu\nUpdateRepairMenu");
    assert_eq!(g.ui_refreshes(), (1, 2));
}
