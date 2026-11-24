//! A large mod list's start-up (the owner's Nuclear Sunset run, 2026-11-11). A keybind mod's duplicate-control check (the shape of Keybinder's SetKeybinds and
//! SetControl user functions, written here from what they do, no mod file copied): every
//! value of an INI section read with GetINISection is looked up with Ar_Find in an
//! auxiliary-variable list and appended when absent; "0." (disabled) counts as -1, which
//! may repeat. A config with no repeated key must not be reported. Synthetic plugin and
//! made-up files (no game data).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let glob = record(b"GLOB", 0, 0x800, &[sub(b"EDID", z("KBDupes")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let bytes = [tes4(), top(b"GLOB", &glob)].concat();
    let plugins = vec![Plugin::parse(&bytes, "KB.esm".into()).unwrap()];
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

const INI: &str = "[Vanilla]\r\n;; 0 disables\r\n;; Forward\r\n0 = 17.\r\n1 = 31.\r\n2 = 30.\r\n3 = 32.\r\n4 = 256.\r\n5 = 33.\r\n6 = 257.\r\n7 = 19.\r\n8 = 29.\r\n9 = 0.\r\n10 = 58.\r\n16 = 0.\r\n17 = 0.\r\n18 = 21.\r\n25 = 201.\r\n\r\n[Tweaks]\r\niOpenMapKey = 50\r\n";

const SET_CONTROL: &str = "string_var svControl\r\nstring_var svKey\r\n\r\nbegin function { svKey, svControl }\r\n\
    if eval (ToNumber (svControl)) >= 256\r\n\
        SetControl (ToNumber (svKey)) -1\r\n\
        SetAltControl (ToNumber (svKey)) ((ToNumber (svControl)) - 256)\r\n\
    else\r\n\
        if eval (ToNumber (svControl)) == 0\r\n\
            svControl = \"-1\"\r\n\
        endif\r\n\
        SetControl (ToNumber (svKey)) (ToNumber (svControl))\r\n\
    endif\r\n\
    if eval (Ar_Find (svControl) (PlayerREF.AuxVarGetAsArr \"*mappedControls\") == Ar_BadNumericIndex)\r\n\
        PlayerREF.AuxVarSetStr \"*mappedControls\" (svControl) -1\r\n\
    else\r\n\
        if eval (ToNumber (svControl)) != -1\r\n\
            PlayerREF.AuxVarSetFlt \"*KeybinderDuplicateControls\" 1\r\n\
        endif\r\n\
    endif\r\n\
    Sv_Destruct svKey, svControl\r\n\
end\r\n";

/// The calling side, also a runtime UDF as in the mod (its private auxiliary variables are
/// the same as SetControl's): the duplicate flag * 1000 + how many controls it mapped.
const SET_KEYBINDS: &str = "array_var aControls\r\narray_var aControl\r\narray_var aSeen\r\nstring_var svControl\r\n\r\nbegin function { }\r\n\
    PlayerREF.AuxVarErase \"*KeybinderDuplicateControls\"\r\n\
    PlayerREF.AuxVarErase \"*mappedControls\"\r\n\
    aControls = GetINISection \"Vanilla\" \"keybinder/Keybinder.ini\" 0\r\n\
    ForEach aControl <- aControls\r\n\
        svControl = (*aControl)\r\n\
        call (CompileScript \"keybinder/SetControl.gek\") (aControl[\"key\"]) (svControl)\r\n\
    Loop\r\n\
    aSeen = PlayerREF.AuxVarGetAsArr \"*mappedControls\"\r\n\
    SetFunctionValue (PlayerREF.AuxVarGetFlt \"*KeybinderDuplicateControls\") * 1000 + (Ar_Size aSeen)\r\n\
end\r\n";

fn files() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let dir = std::env::temp_dir().join(format!("nvf-mod-list-startup-{}", std::process::id()));
    let d = dir.clone();
    ONCE.call_once(move || write_files(d));
    dir
}

fn write_files(dir: std::path::PathBuf) {
    std::fs::create_dir_all(dir.join("Config/keybinder")).unwrap();
    std::fs::create_dir_all(dir.join("NVSE/user_defined_functions/keybinder")).unwrap();
    std::fs::write(dir.join("Config/keybinder/Keybinder.ini"), INI).unwrap();
    std::fs::write(dir.join("NVSE/user_defined_functions/keybinder/SetControl.gek"), SET_CONTROL).unwrap();
    std::fs::write(dir.join("NVSE/user_defined_functions/keybinder/SetKeybinds.gek"), SET_KEYBINDS).unwrap();
    std::fs::write(dir.join("NVSE/user_defined_functions/keybinder/Twice.gek"), "int i
begin function { i }
    SetFunctionValue i * 2
end
").unwrap();
    // JIP's INI as a mod list sets it, and Stewie's Tweaks' INI with one override file.
    std::fs::create_dir_all(dir.join("NVSE/Plugins/Tweaks/INIs")).unwrap();
    std::fs::write(dir.join("NVSE/Plugins/jip_nvse.ini"), "[GamePatches]\r\nuNPCPerks=1\r\n").unwrap();
    std::fs::write(dir.join("NVSE/Plugins/nvse_stewie_tweaks.ini"), "[Hotkeys]\r\niOpenRadioKey = 0\r\niOpenMapKey = 50\r\n").unwrap();
    std::fs::write(dir.join("NVSE/Plugins/Tweaks/INIs/zzzzKeybinder.ini"), "[Hotkeys]\r\niOpenRadioKey = 25\r\n").unwrap();
    let d = dir.clone();
    let reader: fnv_game::mod_files::Reader = std::sync::Arc::new(move |p: &str| std::fs::read(d.join(p.replace('\\', "/"))).ok());
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots { read: vec![dir.clone()], config: None, write: None, lookup: None, reader: Some(reader), archives: Vec::new() });
}

#[test]
fn unique_keys_with_repeated_disabled_ones_are_not_reported() {
    let _dir = files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    let src = "Begin Function { }\nSetFunctionValue (call (CompileScript \"keybinder/SetKeybinds.gek\"))\nEnd\n";
    let r = g.run_query(src, RunCtx::default()).unwrap();
    // 13 distinct values (11 keys, the two mouse buttons; -1 once); no duplicate flag.
    assert_eq!(r, XVal::Num(13.0), "{:?}", g.console_out);
}

#[test]
fn each_queued_message_box_ex_alt_runs_its_own_callback() {
    // Two boxes queued at once, each with its callback; closing the first (its button 0)
    // runs the first's alone, not the last one shown [documented: MessageBoxExAlt, the
    // UDF gets the pressed button; observed: a start-up's queue ran the last box's
    // "Restart Game" callback when the first closed].
    let _dir = files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, modal_message_boxes: true, ..GameConfig::default() });
    let q = "Begin Function { }\n\
        MessageBoxExAlt ({int b} => PlayerREF.AuxVarSetFlt \"*first\" (b + 1)) \"First|Ok|Other\"\n\
        MessageBoxExAlt ({int b} => PlayerREF.AuxVarSetFlt \"*second\" (b + 1)) \"Second|Quit\"\n\
        End\n";
    g.run_query(q, RunCtx::default()).unwrap();
    let read = |g: &mut Game, v: &str| g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue (PlayerREF.AuxVarGetFlt \"{v}\")\nEnd\n"), RunCtx::default()).unwrap();
    g.press_button(1).unwrap();
    g.tick();
    assert_eq!((read(&mut g, "*first"), read(&mut g, "*second")), (XVal::Num(2.0), XVal::Num(0.0)));
    // The second box is in front now; closed without a pick, its button 0.
    g.close_menu();
    g.tick();
    assert_eq!(read(&mut g, "*second"), XVal::Num(1.0));
}

#[test]
fn inis_named_from_config_with_dot_dot_and_tweaks_overrides_are_read() {
    // Titans of the New West reads JIP's INI as `..\nvse\plugins\JIP_NVSE.ini` from
    // Config and, reading 0, asks for a restart on every start; Keybinder compares its keys
    // with StewVal, which the Tweaks\INIs files override.
    let _dir = files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    let r = g.run_query("Begin Function { }\nSetFunctionValue (GetINIFloat \"GamePatches:uNPCPerks\" \"..\\nvse\\plugins\\JIP_NVSE.ini\")\nEnd\n", RunCtx::default()).unwrap();
    assert_eq!(r, XVal::Num(1.0));
    let r = g.run_query("Begin Function { }\nSetFunctionValue (StewVal \"iOpenRadioKey:Hotkeys\") * 100 + (StewVal \"iOpenMapKey:Hotkeys\")\nEnd\n", RunCtx::default()).unwrap();
    assert_eq!(r, XVal::Num(2550.0));
}

#[test]
fn a_compiled_script_is_a_valid_form() {
    // A mod checks CompileScript's result with IsFormValid and reports "a problem
    // compiling" when it is 0 (Scaled Recurring Challenges' start-up script does).
    let _dir = files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    let src = "Begin Function { }
ref r
r = CompileScript \"keybinder/Twice.gek\"
if eval !(IsFormValid r)
    SetFunctionValue -1
    return
endif
SetFunctionValue (call r 21)
End
";
    assert_eq!(g.run_query(src, RunCtx::default()).unwrap(), XVal::Num(42.0));
}

#[test]
fn the_players_first_person_model_is_asked_with_pc_model_2() {
    // B42 Weapon Inertia checks `Player.GetNifScale "NeckController" 2`, a node only
    // the first-person skeleton has, and reports a missing skeleton when it is not 1.
    let _dir = files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    let node = |name: &str, parent: Option<usize>| fnv_game::nif_blocks::SceneNode {
        name: name.into(),
        parent,
        translation: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
        flags: 0,
        material: None,
        info: None,
        world: [0.0; 3],
    };
    g.scene.set_tree_for(fnv_sim::FormId::PLAYER_REF, fnv_game::nif_blocks::ModelView::FirstPerson, vec![node("Bip01", None), node("NeckController", Some(0))]);
    let q = |g: &mut Game, s: &str| g.run_query(&format!("Begin Function {{ }}
SetFunctionValue {s}
End
"), RunCtx::default()).unwrap();
    assert_eq!(q(&mut g, "(PlayerREF.GetNifScale \"NeckController\" 2)"), XVal::Num(1.0));
    // The third-person model (no tree told here) has no such node.
    assert_eq!(q(&mut g, "(PlayerREF.GetNifScale \"NeckController\" 1)"), XVal::Num(0.0));
}
