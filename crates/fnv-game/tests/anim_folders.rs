//! kNVSE's AnimGroupOverride folders (#1461, docs/spec-notes/knvse-anim-folders.md): a
//! made-up Data folder with a weapon's override, a conditioned one and one for every
//! actor, read through a synthetic plugin (no game data). Its own test binary: the
//! Data roots are global to the process.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const WEAPON: u32 = 0x800;
const OTHER_GUN: u32 = 0x801;
const FLAG: u32 = 0x802;
const ROOM: u32 = 0x803;
const MOD_ONLY_GUN: u32 = 0x804;
const DIRECT_GUN: u32 = 0x805;
const CHANCES_GUN: u32 = 0x806;
const VANILLA_GUN: u32 = 0x807;

fn plugin_bytes() -> Vec<u8> {
    let npc_data = [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", npc_data)]);
    let weap_data = [&10i32.to_le_bytes()[..], &100i32.to_le_bytes(), &0.5f32.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]].concat();
    let weaps = [
        record(b"WEAP", 0, WEAPON, &[sub(b"EDID", z("TestGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, OTHER_GUN, &[sub(b"EDID", z("OtherGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, MOD_ONLY_GUN, &[sub(b"EDID", z("ModOnlyGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, DIRECT_GUN, &[sub(b"EDID", z("DirectGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, CHANCES_GUN, &[sub(b"EDID", z("ChancesGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, VANILLA_GUN, &[sub(b"EDID", z("VanillaGun")), sub(b"DATA", weap_data)]),
    ]
    .concat();
    let glob = record(b"GLOB", 0, FLAG, &[sub(b"EDID", z("TestFlag")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let room = record(b"CELL", 0, ROOM, &[sub(b"EDID", z("TestRoom")), sub(b"DATA", vec![1])]);
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"NPC_", &player),
        top(b"WEAP", &weaps),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &room))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Folders.esm".into()).unwrap()];
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

/// The Data folder: `meshes\AnimGroupOverride` with the JSON files and their folders,
/// spelled in mixed case as mods spell them, plus an empty, a broken and a JSON file
/// naming a folder that is not there (all left out). Written once for the process: the
/// tests run side by side, and one rewriting a JSON file while another read the folder
/// made that file look empty or broken to the reader (#3172).
fn files() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(write_files);
}

fn write_files() {
    use fnv_game::mod_files::{set_roots, Roots};
    let dir = std::env::temp_dir().join(format!("nvf-anim-folders-{}", std::process::id()));
    let ago = dir.join("Meshes/AnimGroupOverride");
    for f in [
        "Gun/_male/2hrAttack3.kf",
        "Gun/_male/Mod2/2hrReloadA.kf",
        "Gun/_1stPerson/2hrAttack3.kf",
        "Gun/_1stPerson/Mod1/2hrReloadA.kf",
        "Gun/_1stPerson/Mod2/2hrReloadA.kf",
        "Gun/_1stPerson/Mod3/2hrEquip.kf",
        "Gun/_1stPerson/Mod12/2hrReloadA.kf",
        "Gun/_1stPerson/Mod123/2hrReloadA.kf",
        "Gun/_1stPerson/chances.txt",
        "Gun/Flag/_1stperson/2hrAttack3.kf",
        "Gun/_male.mohidden/mtidle.kf",
        "ModOnly/_1stPerson/Mod2/2hrReloadB.kf",
        "DirectPack/2haJamF.kf",
        "DirectPack/ReloadA/2hrReloadA.kf",
        "DirectPack/Mod1/2hrReloadA.kf",
        "DirectPack/SubCond/2haJamF_sub.kf",
        "ChancesPack/_1stPerson/2haJamF.kf",
        "ChancesPack/_1stPerson/2haJamF_a.kf",
        "ChancesPack/_1stPerson/2haJamF_b.kf",
        "ChancesPack/_1stPerson/2hrAttack3.kf",
        "ChancesPack/_1stPerson/Mod1/2hrReloadA.kf",
        "ChancesPack/_1stPerson/Mod1/2hrReloadA_a.kf",
        "AuthorNotesPack/_1stPerson/2haJamF.kf",
        "AuthorNotesPack/_1stPerson/2haJamF_a.kf",
        "Everyone/_Male/Locomotion/mtidle.kf",
    ] {
        let p = ago.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"kf").unwrap();
    }
    std::fs::write(
        ago.join("ChancesPack/_1stPerson/chances.txt"),
        "; weighted variants\n2haJamF.kf = 75\n2haJamF_a.kf: 25%\n2haJamF_b.kf = 0\n# malformed and invalid lines ignored\nbad_neg.kf = -10\nbad_nan.kf = NaN\nnot a valid line\n",
    )
    .unwrap();
    std::fs::write(
        ago.join("ChancesPack/_1stPerson/Mod1/chances.txt"),
        "2hrReloadA.kf = 0\n2hrReloadA_a.kf = 100\n",
    )
    .unwrap();
    std::fs::write(
        ago.join("AuthorNotesPack/_1stPerson/chances.txt"),
        "Short: 20%\nMed: 30%\nLong: 50%\n",
    )
    .unwrap();
    std::fs::write(
        ago.join("TestGun.json"),
        r#"[
            {"mod": "Folders.esm", "form": "800", "folder": "Gun"},
            {"mod": "Folders.esm", "form": ["801", "800"], "folder": "Gun\\Flag", "condition": "TestFlag == 1", "priority": 1},
            {"mod": "NotLoaded.esp", "form": "800", "folder": "Gun\\Flag"},
            {"mod": "Folders.esm", "form": "800", "folder": "Missing"},
            {"mod": "Folders.esm", "folder": "Gun"},
            {"mod": "Folders.esm", "form": "804", "folder": "ModOnly"},
            {"mod": "Folders.esm", "form": "805", "folder": "DirectPack"},
            {"mod": "Folders.esm", "form": "805", "folder": "DirectPack\\SubCond", "condition": "TestFlag == 1", "priority": 1},
            {"mod": "Folders.esm", "form": "806", "folder": "ChancesPack"},
            {"mod": "Folders.esm", "form": "806", "folder": "AuthorNotesPack", "condition": "TestFlag == 1", "priority": 1}
        ]"#,
    )
    .unwrap();
    std::fs::write(ago.join("everyone.json"), r#"{"folder": "Everyone", "condition": "this == Player"}"#).unwrap();
    std::fs::write(ago.join("Empty.json"), "").unwrap();
    std::fs::write(ago.join("Broken.json"), "[{").unwrap();
    set_roots(Roots { read: vec![dir], ..Roots::default() });
}

fn q(g: &mut Game, body: &str) {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}"));
}

#[test]
fn folder_overrides_follow_the_weapon_and_the_condition() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    // Valid entries kept in file-name order without regard to case: everyone's, the gun's
    // and its conditioned sub-set, plus the mod-only, direct-KF and chances packs.
    let folders: Vec<&str> = g.anim_folders.entries.iter().map(|e| e.folder.as_str()).collect();
    assert_eq!(
        folders,
        [
            "Everyone",
            "Gun",
            "Gun\\Flag",
            "ModOnly",
            "DirectPack",
            "DirectPack\\SubCond",
            "ChancesPack",
            "AuthorNotesPack",
        ]
    );
    // The Mod* files, hidden `.mohidden` folder and text file are not in the base list.
    assert_eq!(g.anim_folders.entries[1].first, ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]);
    assert_eq!(g.anim_folders.entries[1].third, ["AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf"]);
    // Without the gun: only everyone's entry (its condition holds on the player).
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, false), ["AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf"]);
    assert!(g.anim_overrides(FormId::PLAYER_REF, true).is_empty());
    // With the gun: its files, both views.
    q(&mut g, "player.AddItem TestGun 1\nplayer.EquipItem TestGun");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true), ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]);
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, false), ["AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf", "AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf"]);
    // The conditioned folder only while its condition holds, after the plain one (it
    // wins the group).
    q(&mut g, "set TestFlag to 1");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true), ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf", "AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf"]);
    q(&mut g, "set TestFlag to 0");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true).len(), 1);
    // A script's path comes after the folders' (it wins).
    q(&mut g, "player.SetActorAnimationPath 1 1 \"AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf\"");
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true).last().map(String::as_str), Some("AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf"));
    // CopyAnimationsToForm takes the folders' entries along, conditions and all.
    q(&mut g, "CopyAnimationsToForm TestGun OtherGun");
    assert!(g.anim_folders.entries.iter().filter(|e| e.forms.contains(&FormId(WEAPON))).all(|e| e.forms.contains(&FormId(OTHER_GUN))));
}

/// SetActorAnimationPath takes a folder (its KF files) and a condition function: the
/// path applies only while the condition holds [documented: SetActorAnimationPath].
#[test]
fn script_paths_take_folders_and_conditions() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    let gun = |v: Vec<String>| v.into_iter().map(|p| p.replace(char::from(92), "/")).filter(|p| p.contains("Gun")).collect::<Vec<_>>();
    // A folder: its KF files, not the text file or the subfolder's.
    q(&mut g, "player.SetActorAnimationPath 1 1 \"AnimGroupOverride/Gun/_1stPerson\"");
    assert_eq!(gun(g.anim_overrides(FormId::PLAYER_REF, true)), ["AnimGroupOverride/Gun/_1stPerson/2hrAttack3.kf"]);
    // A conditional folder: out while TestFlag is 0, in once it is 1.
    q(&mut g, "player.SetActorAnimationPath 0 1 \"AnimGroupOverride/Gun/_male\" 1 ({} => TestFlag == 1)");
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, false)).is_empty(), "the plain list leaves conditional paths out");
    assert!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)).is_empty());
    q(&mut g, "set TestFlag to 1");
    assert_eq!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    // Disabled: gone.
    q(&mut g, "player.SetActorAnimationPath 0 0 \"AnimGroupOverride/Gun/_male\"");
    assert!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)).is_empty());
}

/// SetWeaponAnimationPath's files apply to whoever holds the weapon, a folder giving its
/// KF files [documented: SetWeaponAnimationPath; hypothesis: the holder].
#[test]
fn weapon_paths_follow_the_weapon_in_hand() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    let gun = |v: Vec<String>| v.into_iter().map(|p| p.replace(char::from(92), "/")).filter(|p| p.contains("Gun/_male")).collect::<Vec<_>>();
    q(&mut g, "SetWeaponAnimationPath OtherGun 0 1 \"AnimGroupOverride/Gun/_male\"");
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, false)).is_empty(), "not held");
    q(&mut g, "player.AddItem OtherGun 1\nplayer.EquipItem OtherGun");
    assert_eq!(gun(g.anim_overrides(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    assert_eq!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, true)).is_empty(), "the third-person path only");
}

/// PlayAnimationPath's sequences finish by themselves; ActivateAnim's stay until a script
/// takes them back [documented: ActivateAnim, "The animation will overlay any other
/// animations playing on the actor"; DeactivateAnim deactivates it]. Both
/// are requests in `AnimPaths::sequences`, which the renderer layers on the actor.
#[test]
fn activate_anim_sequences_stay_until_taken_back() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    q(&mut g, "player.PlayAnimationPath \"AnimGroupOverride/Gun/_male/2hrAttack3.kf\"
player.ActivateAnim \"AnimGroupOverride/Gun/_1stPerson/2hrAttack3.kf\"");
    let played = g.anim_paths.played.get(&FormId::PLAYER_REF).cloned().unwrap_or_default();
    assert_eq!(played.len(), 2);
    let mut kept: Vec<(String, bool)> = g.anim_paths.sequences.iter()
        .filter(|((a, _, _), _)| *a == FormId::PLAYER_REF)
        .map(|((_, _, p), s)| (p.replace(char::from(92), "/"), s.auto_finish))
        .collect();
    kept.sort();
    assert_eq!(kept, [("animgroupoverride/gun/_1stperson/2hrattack3.kf".to_string(), false), ("animgroupoverride/gun/_male/2hrattack3.kf".to_string(), true)]);
}

/// Weapon-mod subfolders (`Mod1`–`Mod3`, `Mod12`, `Mod123`) match the equipped weapon's
/// active mod bitmask (`GetEquippedWeaponModFlags` bits 1/2/4), fall back cleanly to the
/// base folder when unattached, and keep packs that contain only `Mod*` files.
#[test]
fn weapon_mod_subfolders_follow_equipped_mod_flags_and_fallback() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();

    // Base TestGun with no mods attached: only base files, no Mod* overrides.
    q(&mut g, "player.AddItem TestGun 1\nplayer.EquipItem TestGun");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]
    );
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, false),
        [
            "AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf",
            "AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf",
        ]
    );

    // Mod 1 attached (bit 1): Mod1 first-person file appended after base.
    q(&mut g, "player.SetEquippedWeaponModFlags 1");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod1\\2hrReloadA.kf",
        ]
    );
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, false),
        [
            "AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf",
            "AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf",
        ]
    );

    // Mod 2 attached (bit 2): Mod2 applies in both first and third person.
    q(&mut g, "player.SetEquippedWeaponModFlags 2");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod2\\2hrReloadA.kf",
        ]
    );
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, false),
        [
            "AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf",
            "AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf",
            "AnimGroupOverride\\Gun\\_male\\Mod2\\2hrReloadA.kf",
        ]
    );

    // Mod 1 + Mod 2 attached (mask 3): Mod1 and Mod2 come before combined Mod12 so Mod12 wins.
    q(&mut g, "player.SetEquippedWeaponModFlags 3");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod1\\2hrReloadA.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod2\\2hrReloadA.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod12\\2hrReloadA.kf",
        ]
    );

    // All three mods attached (mask 7): Mod123 comes last (highest specificity).
    q(&mut g, "player.SetEquippedWeaponModFlags 7");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod1\\2hrReloadA.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod2\\2hrReloadA.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod3\\2hrEquip.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod12\\2hrReloadA.kf",
            "AnimGroupOverride\\Gun\\_1stPerson\\Mod123\\2hrReloadA.kf",
        ]
    );

    // Detaching all mods restores base-only overrides.
    q(&mut g, "player.SetEquippedWeaponModFlags 0");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]
    );

    // A pack with ONLY `_1stPerson\Mod2\*.kf` is loaded (not dropped as empty) and
    // activates only while Mod 2 is attached.
    q(&mut g, "player.AddItem ModOnlyGun 1\nplayer.EquipItem ModOnlyGun");
    g.refresh_anim_folders();
    assert!(g.anim_overrides(FormId::PLAYER_REF, true).is_empty(), "ModOnly inactive without Mod2");
    q(&mut g, "player.SetEquippedWeaponModFlags 2");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        ["AnimGroupOverride\\ModOnly\\_1stPerson\\Mod2\\2hrReloadB.kf"]
    );
}

/// Folders with direct `.kf` files (root or named anim-group subfolders without `_male`/
/// `_1stPerson`) resolve cleanly for both views, honour `Mod*` subfolders inside them,
/// and never pull in nested child sets referenced by another JSON entry.
#[test]
fn direct_kf_subfolders_resolve_and_isolate_nested_sets() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();

    q(&mut g, "player.AddItem DirectGun 1\nplayer.EquipItem DirectGun");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\DirectPack\\2haJamF.kf",
            "AnimGroupOverride\\DirectPack\\ReloadA\\2hrReloadA.kf",
        ]
    );
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, false),
        [
            "AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf",
            "AnimGroupOverride\\DirectPack\\2haJamF.kf",
            "AnimGroupOverride\\DirectPack\\ReloadA\\2hrReloadA.kf",
        ]
    );

    // Mod1 directly inside a direct-KF pack applies when Mod 1 is attached.
    q(&mut g, "player.SetEquippedWeaponModFlags 1");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\DirectPack\\2haJamF.kf",
            "AnimGroupOverride\\DirectPack\\ReloadA\\2hrReloadA.kf",
            "AnimGroupOverride\\DirectPack\\Mod1\\2hrReloadA.kf",
        ]
    );

    // Conditional direct-KF child set (`DirectPack\SubCond`) activates only when TestFlag == 1.
    q(&mut g, "set TestFlag to 1");
    g.refresh_anim_folders();
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, true),
        [
            "AnimGroupOverride\\DirectPack\\2haJamF.kf",
            "AnimGroupOverride\\DirectPack\\ReloadA\\2hrReloadA.kf",
            "AnimGroupOverride\\DirectPack\\Mod1\\2hrReloadA.kf",
            "AnimGroupOverride\\DirectPack\\SubCond\\2haJamF_sub.kf",
        ]
    );
}

/// Two consecutive polls with nothing changed keep the exact same `chances.txt` weighted
/// pick and leave `Game::rng` completely untouched; switching weapons or changing the
/// matching entry set re-rolls from `AnimFolders`'s separate RNG while still leaving
/// `Game::rng` untouched.
#[test]
fn chances_txt_pick_is_stable_across_unchanged_polls_and_leaves_game_rng_untouched() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { seed: 2026, ..GameConfig::default() });
    g.player_entered(Some(FormId(ROOM)));
    g.tick();

    q(&mut g, "player.AddItem ChancesGun 1\nplayer.AddItem VanillaGun 1\nplayer.EquipItem ChancesGun");
    let rng_before = g.rng.clone();

    // First poll selects a weighted variant; a second (and further) poll with nothing
    // changed keeps the exact same pick and never touches Game::rng.
    g.refresh_anim_folders();
    let first_poll = g.anim_overrides(FormId::PLAYER_REF, true);
    assert_eq!(first_poll.len(), 2, "expected 1 unweighted + 1 weighted pick, got {first_poll:?}");
    assert_eq!(first_poll[0], "AnimGroupOverride\\ChancesPack\\_1stPerson\\2hrAttack3.kf");
    assert_eq!(g.rng, rng_before, "chances.txt draw must not touch Game::rng");

    for _ in 0..32 {
        g.refresh_anim_folders();
        assert_eq!(
            g.anim_overrides(FormId::PLAYER_REF, true),
            first_poll,
            "unchanged poll must keep the same chances.txt pick"
        );
    }
    assert_eq!(g.rng, rng_before, "repeated polls must leave Game::rng untouched");

    // Changing the equipped weapon and switching back invalidates the cached pick so the
    // pool re-rolls from the separate RNG (both positive-weight variants eventually appear),
    // while still leaving Game::rng untouched.
    let mut saw_base = false;
    let mut saw_a = false;
    for _ in 0..64 {
        q(&mut g, "player.EquipItem VanillaGun");
        g.refresh_anim_folders();
        q(&mut g, "player.EquipItem ChancesGun");
        g.refresh_anim_folders();
        let picked = g.anim_overrides(FormId::PLAYER_REF, true);
        if picked[1].ends_with("2haJamF.kf") {
            saw_base = true;
        } else if picked[1].ends_with("2haJamF_a.kf") {
            saw_a = true;
        }
        if saw_base && saw_a {
            break;
        }
    }
    assert!(saw_base && saw_a, "changing weapons should re-roll from the separate RNG");
    assert_eq!(g.rng, rng_before, "re-rolling on weapon change must still leave Game::rng untouched");

    // Changing the matching entry set (via TestFlag condition) also re-rolls cleanly
    // without touching Game::rng.
    let mut saw_entry_change_other = false;
    let before_flag = g.anim_overrides(FormId::PLAYER_REF, true)[1].clone();
    for i in 0..32 {
        let flag = if i % 2 == 0 { 1 } else { 0 };
        q(&mut g, &format!("set TestFlag to {flag}"));
        g.refresh_anim_folders();
        let after = g.anim_overrides(FormId::PLAYER_REF, true);
        if after[1] != before_flag {
            saw_entry_change_other = true;
            break;
        }
    }
    assert!(saw_entry_change_other, "changing the matching entry set should re-roll the pool");
    assert_eq!(g.rng, rng_before, "re-rolling on entry-set change must still leave Game::rng untouched");
}

/// `chances.txt` weighted random selection follows exact weights when re-rolled across
/// weapon equips, excludes weight-0 variants, preserves unweighted sibling `.kf` files,
/// works inside `Mod*` subfolders, leaves author-note `chances.txt` files undisturbed,
/// and never consumes `Game::rng`.
#[test]
fn chances_txt_weighted_selection_distribution_and_zero_exclusion() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { seed: 2026, ..GameConfig::default() });
    g.player_entered(Some(FormId(ROOM)));
    g.tick();

    q(&mut g, "player.AddItem ChancesGun 1\nplayer.AddItem VanillaGun 1\nplayer.EquipItem ChancesGun");
    let rng_before = g.rng.clone();
    let mut base_hits = 0usize;
    let mut a_hits = 0usize;
    let mut b_hits = 0usize;
    let trials = 2_000usize;
    for _ in 0..trials {
        // Switching away and back changes the weapon context so each trial draws once from
        // the dedicated `AnimFolders` RNG.
        q(&mut g, "player.EquipItem VanillaGun");
        g.refresh_anim_folders();
        q(&mut g, "player.EquipItem ChancesGun");
        g.refresh_anim_folders();
        let list = g.anim_overrides(FormId::PLAYER_REF, true);
        // Unweighted sibling `2hrAttack3.kf` is always present alongside the 1 weighted pick.
        assert_eq!(list.len(), 2, "expected 1 unweighted + 1 weighted pick, got {list:?}");
        assert_eq!(list[0], "AnimGroupOverride\\ChancesPack\\_1stPerson\\2hrAttack3.kf");
        match list[1].as_str() {
            "AnimGroupOverride\\ChancesPack\\_1stPerson\\2haJamF.kf" => base_hits += 1,
            "AnimGroupOverride\\ChancesPack\\_1stPerson\\2haJamF_a.kf" => a_hits += 1,
            "AnimGroupOverride\\ChancesPack\\_1stPerson\\2haJamF_b.kf" => b_hits += 1,
            other => panic!("unexpected weighted pick: {other}"),
        }
    }
    assert_eq!(b_hits, 0, "weight-0 variant must never be selected");
    let base_share = base_hits as f64 / trials as f64;
    let a_share = a_hits as f64 / trials as f64;
    assert!((base_share - 0.75).abs() < 0.03, "base_share={base_share}");
    assert!((a_share - 0.25).abs() < 0.03, "a_share={a_share}");
    assert_eq!(g.rng, rng_before, "2,000 weighted draws must leave Game::rng untouched");

    // Deterministic reproducibility: a fresh Game with the same seed draws the same sequence.
    let draw_sequence = || {
        let world = World::new(&f.forms, &f.refs, SimConfig::default());
        let mut game = Game::new(&f.data, world, GameConfig { seed: 2026, ..GameConfig::default() });
        game.player_entered(Some(FormId(ROOM)));
        game.tick();
        q(&mut game, "player.AddItem ChancesGun 1\nplayer.AddItem VanillaGun 1");
        (0..16)
            .map(|_| {
                q(&mut game, "player.EquipItem VanillaGun");
                game.refresh_anim_folders();
                q(&mut game, "player.EquipItem ChancesGun");
                game.refresh_anim_folders();
                game.anim_overrides(FormId::PLAYER_REF, true)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(draw_sequence(), draw_sequence(), "weighted selection must be deterministic for identical GameConfig::seed");

    // Mod1 subfolder with its own `chances.txt` (`2hrReloadA.kf = 0`, `2hrReloadA_a.kf = 100`).
    q(&mut g, "player.SetEquippedWeaponModFlags 1");
    g.refresh_anim_folders();
    let with_mod1 = g.anim_overrides(FormId::PLAYER_REF, true);
    assert!(with_mod1.contains(&"AnimGroupOverride\\ChancesPack\\_1stPerson\\Mod1\\2hrReloadA_a.kf".to_string()));
    assert!(!with_mod1.contains(&"AnimGroupOverride\\ChancesPack\\_1stPerson\\Mod1\\2hrReloadA.kf".to_string()));

    // Author-note `chances.txt` (`Short: 20%`, etc. — no matching `.kf` filenames) leaves
    // all `.kf` files in that folder intact.
    q(&mut g, "set TestFlag to 1");
    g.refresh_anim_folders();
    let with_notes = g.anim_overrides(FormId::PLAYER_REF, true);
    assert!(with_notes.contains(&"AnimGroupOverride\\AuthorNotesPack\\_1stPerson\\2haJamF.kf".to_string()));
    assert!(with_notes.contains(&"AnimGroupOverride\\AuthorNotesPack\\_1stPerson\\2haJamF_a.kf".to_string()));
    assert_eq!(g.rng, rng_before, "Mod1 and author-note chances.txt checks must leave Game::rng untouched");
}

/// Vanilla weapons with no matching `AnimGroupOverride` JSON entries remain completely
/// unchanged (empty first-person overrides, no weapon-mod folder leakage, untouched RNG).
#[test]
fn vanilla_weapon_without_overrides_is_unchanged() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { seed: 77, ..GameConfig::default() });
    g.player_entered(Some(FormId(ROOM)));
    g.tick();

    q(&mut g, "player.AddItem VanillaGun 1\nplayer.EquipItem VanillaGun\nplayer.SetEquippedWeaponModFlags 7");
    let rng_before = g.rng.clone();
    g.refresh_anim_folders();
    assert!(g.anim_overrides(FormId::PLAYER_REF, true).is_empty());
    assert_eq!(
        g.anim_overrides(FormId::PLAYER_REF, false),
        ["AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf"]
    );
    // An actor with no matching entries at all receives empty overrides and does not
    // consume Game::rng.
    let other_actor = FormId(0x9999);
    g.refresh_anim_folders_for(other_actor);
    assert!(g.anim_overrides(other_actor, false).is_empty());
    assert!(g.anim_overrides(other_actor, true).is_empty());
    assert_eq!(g.rng, rng_before, "refresh without weighted pools must not consume Game::rng");
}
