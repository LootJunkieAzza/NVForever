//! Clean-room end-to-end qualification tests for Stewie's Tweaks version gates
//! #3179 (941 -> 945), #3183 (945 -> 960), and #3184 (960 -> 970).
//!
//! Independently authored synthetic records, XML fixtures, and NVSE scripts only
//! (no game data, no copied mod script text, and no version bump: `GetPluginVersion
//! "lStewieAl's Tweaks"` remains pinned at `941`).

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    casino::{self, Closed},
    cond::{Cmp, Condition, Operand, RunOn},
    data::topic_type,
    game::{menus, Game, GameConfig, RunCtx},
    tweaks::Tweaks,
    ui_input,
    ui_loading::{self, ALWAYS_LOADED},
    ui_search_bar, GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::{
    input::{no_measure, Input},
    MemSource,
};
use serde_json::Value as Json;
use std::path::Path;

const CELL_ID: u32 = 0x200;
const MANAGER_BASE: u32 = 0x910;
const MANAGER_REF: u32 = 0x310;
const CASINO_ID: u32 = 0x920;
const CHIP_ID: u32 = 0x921;
const QUEST_ID: u32 = 0x930;
const GREETING_TOPIC: u32 = 0x931;
const REWARD_TOPIC: u32 = 0x932;
const GREETING_INFO: u32 = 0x933;
// 8-digit all-decimal bare hex FormIDs (zero A-F digits) to exercise the script
// compiler's bare hex FormID admission on extender form parameters (#3179).
const DECIMAL_INFO_HIT: u32 = 0x0016_3183;
const DECIMAL_INFO_MISS: u32 = 0x0016_3184;

const G_OPEN_COUNT: u32 = 0x940;
const G_OPEN_SUM: u32 = 0x941;
const G_KEY_COUNT: u32 = 0x942;
const G_KEY_SUM: u32 = 0x943;
const G_BTN_COUNT: u32 = 0x944;
const G_BTN_SUM: u32 = 0x945;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn glob_rec(id: u32, edid: &str) -> Vec<u8> {
    record(b"GLOB", 0, id, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", f32b(0.0))])
}

fn udf_rec(id: u32, edid: &str, src: &str) -> Vec<u8> {
    // Include a `string_var` local so `Auto` source routing compiles SCTX directly.
    let src = src.replacen("\nBegin Function", "\nstring_var sMarker\nBegin Function", 1);
    let mut fields = vec![sub(b"EDID", z(edid))];
    fields.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    fields.push(sub(b"SCTX", src.into_bytes()));
    record(b"SCPT", 0, id, &fields)
}

fn info_rec(id: u32, quest: u32, ty: u8, text: &str, speaker: Option<u32>, choices: &[u32]) -> Vec<u8> {
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    let mut s = vec![
        sub(b"EDID", z(&format!("QualInfo{id:08X}"))),
        sub(b"DATA", vec![ty, 0, 0, 0]),
        sub(b"QSTI", u32b(quest)),
        sub(b"TRDT", trdt),
        sub(b"NAM1", z(text)),
        sub(b"NAM2", z("")),
        sub(b"NAM3", z("")),
    ];
    s.extend(choices.iter().map(|t| sub(b"TCLT", u32b(*t))));
    s.extend(unit(0, &[], &[], &[]));
    s.push(sub(b"NEXT", vec![]));
    s.extend(unit(0, &[], &[], &[]));
    if let Some(a) = speaker {
        s.push(sub(b"ANAM", u32b(a)));
    }
    record(b"INFO", 0, id, &s)
}

fn topic_rec(id: u32, edid: &str, name: &str, ty: u8, lines: Vec<Vec<u8>>) -> Vec<u8> {
    let dial = record(
        b"DIAL",
        0,
        id,
        &[sub(b"EDID", z(edid)), sub(b"FULL", z(name)), sub(b"PNAM", f32b(50.0)), sub(b"DATA", vec![ty, 0])],
    );
    let children: Vec<u8> = lines.into_iter().flatten().collect();
    [dial, group(id.to_le_bytes(), 7, &children)].concat()
}

fn fixture() -> Fixture {
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, MANAGER_BASE, &[sub(b"EDID", z("OwnedFloorManager")), sub(b"FULL", z("Floor Manager"))]),
    ]
    .concat();
    let chip = record(b"CHIP", 0, CHIP_ID, &[sub(b"EDID", z("OwnedChip"))]);
    let csno_data = [f32b(0.5), f32b(1.5), u32s(&[1; 7]), u32s(&[4, 1000, CHIP_ID, QUEST_ID, 0])].concat();
    let csno = record(b"CSNO", 0, CASINO_ID, &[sub(b"EDID", z("OwnedCasino")), sub(b"FULL", z("Owned Casino")), sub(b"DATA", csno_data)]);
    let qust = record(b"QUST", 0, QUEST_ID, &[sub(b"EDID", z("OwnedCasinoQuest")), sub(b"DATA", [vec![1, 50, 0, 0], f32b(0.0)].concat())]);
    let dials = [
        topic_rec(
            GREETING_TOPIC,
            "GREETING",
            "GREETING",
            topic_type::TOPIC,
            vec![info_rec(GREETING_INFO, QUEST_ID, topic_type::TOPIC, "Welcome.", Some(MANAGER_BASE), &[REWARD_TOPIC])],
        ),
        topic_rec(
            REWARD_TOPIC,
            "OwnedRewardTopic",
            "Claim comp",
            topic_type::TOPIC,
            vec![
                info_rec(DECIMAL_INFO_HIT, QUEST_ID, topic_type::TOPIC, "Enjoy your comp.", Some(MANAGER_BASE), &[]),
                info_rec(DECIMAL_INFO_MISS, QUEST_ID, topic_type::TOPIC, "Unreached fallback.", Some(MANAGER_BASE), &[]),
            ],
        ),
    ]
    .concat();
    let globs = [
        glob_rec(G_OPEN_COUNT, "QualOpenCount"),
        glob_rec(G_OPEN_SUM, "QualOpenSum"),
        glob_rec(G_KEY_COUNT, "QualKeyCount"),
        glob_rec(G_KEY_SUM, "QualKeySum"),
        glob_rec(G_BTN_COUNT, "QualBtnCount"),
        glob_rec(G_BTN_SUM, "QualBtnSum"),
    ]
    .concat();
    let scripts = [
        udf_rec(
            0x950,
            "QualOnMenuOpenUDF",
            "scn QualOnMenuOpenUDF\nint iMenu\nBegin Function {iMenu}\n  set QualOpenCount to QualOpenCount + 1\n  set QualOpenSum to QualOpenSum + iMenu\nEnd\n",
        ),
        udf_rec(
            0x951,
            "QualOnKeyUDF",
            "scn QualOnKeyUDF\nint iKey\nBegin Function {iKey}\n  set QualKeyCount to QualKeyCount + 1\n  set QualKeySum to QualKeySum + iKey\nEnd\n",
        ),
        udf_rec(
            0x952,
            "QualOnBtnUDF",
            "scn QualOnBtnUDF\nint iBtn\nBegin Function {iBtn}\n  set QualBtnCount to QualBtnCount + 1\n  set QualBtnSum to QualBtnSum + iBtn\nEnd\n",
        ),
    ]
    .concat();
    let achr = record(b"ACHR", 0, MANAGER_REF, &[sub(b"EDID", z("OwnedManagerREF")), sub(b"NAME", u32b(MANAGER_BASE))]);
    let cell_rec = record(b"CELL", 0, CELL_ID, &[sub(b"EDID", z("OwnedCell")), sub(b"DATA", vec![1])]);
    let cell_children = group(CELL_ID.to_le_bytes(), 6, &group(CELL_ID.to_le_bytes(), 9, &achr));
    let cells = group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell_rec, cell_children].concat()));

    let bytes = [
        tes4(),
        top(b"GLOB", &globs),
        top(b"SCPT", &scripts),
        top(b"NPC_", &npcs),
        top(b"CHIP", &chip),
        top(b"CSNO", &csno),
        top(b"QUST", &qust),
        top(b"DIAL", &dials),
        top(b"CELL", &cells),
    ]
    .concat();

    let plugins = [Plugin::parse(&bytes, "StewieQual.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let funcs = root.join("script/functions.json");
    let json: Json = serde_json::from_str(&std::fs::read_to_string(&funcs).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&funcs).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn new_game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig { strict: true, frame_seconds: 0.02, ..Default::default() },
    );
    g.new_game();
    g.player_entered(Some(FormId(CELL_ID)));
    g
}

fn run(g: &mut Game<'_>, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn num(g: &mut Game<'_>, expr: &str) -> f64 {
    match run(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(v) => v,
        XVal::Ref(r) => f64::from(r),
        other => panic!("{expr}: expected number/ref, got {other:?}"),
    }
}

fn glob(g: &Game<'_>, id: u32) -> f32 {
    g.world.global(FormId(id)).unwrap()
}

fn pipboy_and_trade_ui() -> MemSource {
    let mut src = MemSource::default();
    for (i, name) in ALWAYS_LOADED.iter().enumerate() {
        src = src.with(
            &format!("menus/added/{i}.xml"),
            &format!(r#"<menu name="{name}"><class>&{name};</class></menu>"#),
        );
    }
    src.with(
        "menus/barter_menu.xml",
        r#"<menu name="BarterMenu">
            <class>&BarterMenu;</class>
            <_wildHits>0</_wildHits>
            <_idx1Hits>0</_idx1Hits>
            <_lastMenu>0</_lastMenu>
            <_lastId>0</_lastId>
            <_lastName></_lastName>
            <rect name="BarterCategories">
                <x>0</x><y>0</y><width>320</width><height>40</height>
                <hotrect name="BC_Category">
                    <id>10</id><target>1</target>
                    <x>10</x><y>5</y><width>80</width><height>30</height>
                </hotrect>
                <hotrect name="BC_Category">
                    <id>11</id><target>1</target>
                    <x>110</x><y>5</y><width>80</width><height>30</height>
                </hotrect>
                <hotrect name="BC_Category">
                    <id>12</id><target>1</target>
                    <x>210</x><y>5</y><width>80</width><height>30</height>
                </hotrect>
            </rect>
        </menu>"#,
    )
    .with(
        "menus/container_menu.xml",
        r#"<menu name="ContainerMenu">
            <class>&ContainerMenu;</class>
            <_hits>0</_hits>
            <_menuId>0</_menuId>
            <hotrect name="CM_Tab">
                <id>20</id><target>1</target>
                <x>10</x><y>10</y><width>80</width><height>30</height>
            </hotrect>
        </menu>"#,
    )
}

#[test]
fn gate_945_casino_menu_close_decimal_info_dialog_topic_and_no_casino_bans() {
    let f = fixture();
    let mut g = new_game(&f);

    // Version remains pinned at 941 until original-game bNoCasinoBans capture clears #3179.
    assert_eq!(num(&mut g, r#"GetPluginVersion "lStewieAl's Tweaks""#), 941.0);

    // Register SetOnMenuCloseEventHandler for documented casino menu codes 1080/1081/1082
    // and SetOnDialogTopicEventHandler using an 8-digit ALL-DECIMAL bare hex FormID (`00163183`).
    assert_eq!(fnv_game::slots::MENU_CODE, 1080);
    assert_eq!(fnv_game::blackjack::MENU_CODE, 1081);
    assert_eq!(fnv_game::roulette::MENU_CODE, 1082);
    for code in [1080u16, 1081, 1082] {
        run(
            &mut g,
            &format!(
                "SetOnMenuCloseEventHandler (begin function {{int iMenu}}\n\
                 PlayerREF.AuxVarSetFlt \"*qual_close_sum\" ((PlayerREF.AuxVarGetFlt \"*qual_close_sum\") + iMenu)\n\
                 end) 1 {code}"
            ),
        );
    }
    run(
        &mut g,
        "SetOnDialogTopicEventHandler (begin function {ref rSpoken}\n\
           PlayerREF.AuxVarSetRef \"*qual_dt_speaker\" (GetSelf)\n\
           PlayerREF.AuxVarSetRef \"*qual_dt_info\" rSpoken\n\
         end) 1 00163183\n\
         SetOnDialogTopicEventHandler (begin function {ref rSpoken}\n\
           PlayerREF.AuxVarSetFlt \"*qual_dt_miss\" 1\n\
         end) 1 00163184",
    );

    // Push winnings to max (1000) and verify no_casino_bans toggle across both readers,
    // dialogue condition evaluation, and table opening policy without rewriting saved standing.
    casino::add_earnings(&mut g, FormId(CASINO_ID), 1000, 1000);
    let banned_cond = Condition {
        cmp: Cmp::Eq,
        or: false,
        run_on: RunOn::Subject,
        function: g.data.table.by_name("GetCasinoWinningStage").unwrap() - 0x1000,
        params: vec![fnv_script::vm::ArgValue::Value(fnv_script::vm::Value::Ref(CASINO_ID))],
        rhs: Operand::Value(4.0),
    };
    for enabled in [false, true, false] {
        g.tweaks.no_casino_bans = enabled;
        let expected = if enabled { 3.0 } else { 4.0 };
        assert_eq!(num(&mut g, "GetCasinoWinningsLevel OwnedCasino"), expected);
        assert_eq!(num(&mut g, "GetCasinoWinningStage OwnedCasino"), expected);
        assert_eq!(g.conditions_pass(std::slice::from_ref(&banned_cond), Some(FormId::PLAYER_REF), None, "casino ban"), !enabled);
        assert_eq!(casino::opening(&g, FormId(CASINO_ID), 10, 1000), if enabled { Ok(()) } else { Err(Closed::Banned) });
        assert_eq!(g.player.casino_levels[&FormId(CASINO_ID)], 4);
    }

    // Fire menu close for 1080/1081/1082 and confirm all three handlers run.
    for code in [1080u16, 1081, 1082] {
        g.open_menu(code);
        assert_eq!(g.close_menu(), Some(code));
    }
    assert_eq!(num(&mut g, r#"PlayerREF.AuxVarGetFlt "*qual_close_sum""#), f64::from(1080 + 1081 + 1082));

    // Fire live dialogue on the 8-digit all-decimal INFO FormID 0x00163183.
    g.start_dialogue(FormId(MANAGER_REF), None).unwrap();
    for _ in 0..20_000 {
        if g.dialogue.as_ref().is_none_or(|c| !c.choices.is_empty()) {
            break;
        }
        g.tick();
    }
    let idx = g
        .dialogue_choices()
        .iter()
        .position(|c| c.topic == FormId(REWARD_TOPIC))
        .expect("reward topic offered");
    let spoken = g.choose(idx).unwrap();
    assert_eq!(spoken, FormId(DECIMAL_INFO_HIT));
    assert_eq!(num(&mut g, r#"PlayerREF.AuxVarGetRef "*qual_dt_speaker""#), f64::from(MANAGER_REF));
    assert_eq!(num(&mut g, r#"PlayerREF.AuxVarGetRef "*qual_dt_info""#), f64::from(DECIMAL_INFO_HIT));
    assert_eq!(num(&mut g, r#"PlayerREF.AuxVarGetFlt "*qual_dt_miss""#), 0.0);
}

#[test]
fn gate_960_live_stewval_pipboy_mode_and_menu_search_bar_offsets() {
    let f = fixture();
    let mut g = new_game(&f);
    assert_eq!(num(&mut g, r#"GetPluginVersion "lStewieAl's Tweaks""#), 941.0);

    // Seed a conflicting INI file to prove live bound engine settings override stale INI
    // for mapped keys (including bArmorConditionLabel) while unrepresented keys
    // (bScaleCriticalDamage) honestly retain INI fallback.
    assert!(g.mod_files.write_bytes(
        "NVSE/Plugins/nvse_stewie_tweaks.ini",
        b"[Tweaks]\nbKeepPipboyLightOnCellChange=0\nbDisableStealthEffectInPipboy=0\nbIndividialItemStats=0\nbCritChanceIgnoresFireRate=0\nbScaleCriticalDamage=1\nbArmorConditionLabel=1\nbMenuSearch=1\n[Critical Hits]\nfCriticalDamageMult=2.5\n",
    ));
    let bound = Tweaks {
        single_item_figures: true,
        no_stealth_shimmer_in_pipboy: true,
        crit_ignores_fire_rate: true,
        crit_damage_scale: 1.75,
        menu_search: false,
        armor_condition: false,
        ..Default::default()
    };
    g.bind_stewie_stat_settings(&bound);

    for fn_name in ["StewVal", "StewieGetValue"] {
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bKeepPipboyLightOnCellChange:Tweaks""#)), 1.0);
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bDisableStealthEffectInPipboy:Tweaks""#)), 1.0);
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bIndividialItemStats:Tweaks""#)), 1.0);
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bCritChanceIgnoresFireRate:Tweaks""#)), 1.0);
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "fCriticalDamageMult:Critical Hits""#)), 1.75);
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bArmorConditionLabel:Tweaks""#)), 0.0);
        // Documented INI-fallback key (no separate boolean enable switch in NVForever):
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bScaleCriticalDamage:Tweaks""#)), 1.0);
    }

    // Toggling bound armor_condition=true reflects live through both aliases.
    g.bind_stewie_stat_settings(&Tweaks { armor_condition: true, ..bound.clone() });
    for fn_name in ["StewVal", "StewieGetValue"] {
        assert_eq!(num(&mut g, &format!(r#"{fn_name} "bArmorConditionLabel:Tweaks""#)), 1.0);
    }
    g.bind_stewie_stat_settings(&bound);

    // GetPipBoyMode live tracking: 0 closed -> 3 across Pip-Boy pages -> 0 closed / Barter.
    assert_eq!(num(&mut g, "GetPipBoyMode"), 0.0);
    g.open_menu(menus::INVENTORY);
    assert_eq!(num(&mut g, "GetPipBoyMode"), 3.0);
    assert!(g.set_pipboy_menu(menus::STATS));
    assert_eq!(num(&mut g, "GetPipBoyMode"), 3.0);
    assert!(g.set_pipboy_menu(menus::MAP));
    assert_eq!(num(&mut g, "GetPipBoyMode"), 3.0);
    assert!(g.engine_menu_closed(menus::INVENTORY));
    assert_eq!(num(&mut g, "GetPipBoyMode"), 0.0);
    g.open_menu(menus::BARTER);
    assert_eq!(num(&mut g, "GetPipBoyMode"), 0.0);
    g.close_menu();

    // bMenuSearch gating: bound menu_search=false suppresses IM/SM/MM_SearchBar even with INI=1.
    let src_off = pipboy_and_trade_ui();
    let paths_off: Vec<_> = src_off.files.keys().cloned().collect();
    ui_loading::load(&mut g, Box::new(src_off), paths_off);
    for path in ["InventoryMenu/IM_SearchBar", "StatsMenu/SM_SearchBar", "MapMenu/MM_SearchBar"] {
        assert!(g.ui_tree.find_tile(path).is_none(), "{path} must stay absent when bound menu_search=false");
    }

    // Enabling menu_search publishes IM/SM/MM_SearchBar and round-trips _OffsetX/_OffsetY.
    g.bind_stewie_stat_settings(&Tweaks { menu_search: true, ..bound });
    let src_on = pipboy_and_trade_ui();
    let paths_on: Vec<_> = src_on.files.keys().cloned().collect();
    ui_loading::load(&mut g, Box::new(src_on), paths_on);
    run(
        &mut g,
        r#"
        SetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetX" 18
        SetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetY" -6
        SetUIFloatAlt "StatsMenu\SM_SearchBar\_OffsetX" (GetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetX")
        SetUIFloatAlt "StatsMenu\SM_SearchBar\_OffsetY" (GetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetY")
        SetUIFloatAlt "MapMenu\MM_SearchBar\_OffsetX" (GetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetX")
        SetUIFloatAlt "MapMenu\MM_SearchBar\_OffsetY" (GetUIFloatAlt "InventoryMenu\IM_SearchBar\_OffsetY")
        "#,
    );
    for code in [menus::INVENTORY, menus::STATS, menus::MAP] {
        assert_eq!(ui_search_bar::read_offsets(&g.ui_tree, code), Some((18.0, -6.0)));
    }
}

#[test]
fn gate_970_barter_container_open_click_wildcards_and_key_button_events() {
    let f = fixture();
    let mut g = new_game(&f);
    assert_eq!(num(&mut g, r#"GetPluginVersion "lStewieAl's Tweaks""#), 941.0);

    let src = pipboy_and_trade_ui();
    let barter = g.ui_tree.load_menu(&src, "menus\\barter_menu.xml").unwrap();
    let container = g.ui_tree.load_menu(&src, "menus\\container_menu.xml").unwrap();
    g.ui_tree.update();

    // Register SetOnMenuOpenEventHandler (1053 BarterMenu, 1008 ContainerMenu),
    // SetOnMenuClickEventHandler (wildcard + indexed sibling + ContainerMenu#id),
    // and OnKeyDown (30/32/203/205) + OnButtonDown (4/8).
    run(
        &mut g,
        r#"
        SetOnMenuOpenEventHandler QualOnMenuOpenUDF 1 1053
        SetOnMenuOpenEventHandler QualOnMenuOpenUDF 1 1008
        SetOnMenuClickEventHandler (Begin Function {int iMenu, int iTile, string_var sName}
            SetUIFloat "BarterMenu/_lastMenu" iMenu
            SetUIFloat "BarterMenu/_lastId" iTile
            SetUIString "BarterMenu/_lastName" sName
            ModUIFloat "BarterMenu/_wildHits" 1
        End) 1 "BarterMenu/*/BC_Category"
        SetOnMenuClickEventHandler (Begin Function {int iMenu, int iTile, string_var sName}
            ModUIFloat "BarterMenu/_idx1Hits" 1
        End) 1 "BarterMenu/BarterCategories/BC_Category:1"
        SetOnMenuClickEventHandler (Begin Function {int iMenu, int iTile, string_var sName}
            ModUIFloat "ContainerMenu/_hits" 1
            SetUIFloat "ContainerMenu/_menuId" iMenu
        End) 1 "ContainerMenu#20"
        SetEventHandler "OnKeyDown:30" QualOnKeyUDF
        SetEventHandler "OnKeyDown:32" QualOnKeyUDF
        SetEventHandler "OnKeyDown:203" QualOnKeyUDF
        SetEventHandler "OnKeyDown:205" QualOnKeyUDF
        SetEventHandler "OnButtonDown:4" QualOnBtnUDF
        SetEventHandler "OnButtonDown:8" QualOnBtnUDF
        "#,
    );

    // Opening BarterMenu (1053) and ContainerMenu (1008) fires SetOnMenuOpenEventHandler.
    g.open_menu(menus::BARTER);
    assert_eq!(glob(&g, G_OPEN_COUNT), 1.0);
    assert_eq!(glob(&g, G_OPEN_SUM), 1053.0);
    g.close_menu();
    g.open_menu(1008);
    assert_eq!(glob(&g, G_OPEN_COUNT), 2.0);
    assert_eq!(glob(&g, G_OPEN_SUM), f64::from(1053 + 1008) as f32);
    g.close_menu();

    // Clicking BarterMenu sibling 0 hits wildcard only; sibling 1 hits wildcard + :1;
    // clicking ContainerMenu#20 hits ContainerMenu handler with menu ID 1008.
    ui_input::step(&mut g, &[barter], &Input { at: Some((40.0, 20.0)), press: true, ..Default::default() }, &no_measure);
    assert_eq!(g.ui_tree.get_float("BarterMenu/_lastMenu"), Some(1053.0));
    assert_eq!(g.ui_tree.get_float("BarterMenu/_lastId"), Some(10.0));
    assert_eq!(g.ui_tree.get_string("BarterMenu/_lastName").as_deref(), Some("bc_category:0"));
    assert_eq!(g.ui_tree.get_float("BarterMenu/_wildHits"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("BarterMenu/_idx1Hits"), Some(0.0));

    ui_input::step(&mut g, &[barter], &Input { at: Some((140.0, 20.0)), press: true, ..Default::default() }, &no_measure);
    assert_eq!(g.ui_tree.get_float("BarterMenu/_lastId"), Some(11.0));
    assert_eq!(g.ui_tree.get_string("BarterMenu/_lastName").as_deref(), Some("bc_category:1"));
    assert_eq!(g.ui_tree.get_float("BarterMenu/_wildHits"), Some(2.0));
    assert_eq!(g.ui_tree.get_float("BarterMenu/_idx1Hits"), Some(1.0));

    ui_input::step(&mut g, &[container], &Input { at: Some((40.0, 20.0)), press: true, ..Default::default() }, &no_measure);
    assert_eq!(g.ui_tree.get_float("ContainerMenu/_hits"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("ContainerMenu/_menuId"), Some(1008.0));

    // WASD (30/32), Arrow (203/205), and D-pad (4/8) edge dispatch fires once per press.
    for key in [30u16, 32, 203, 205] {
        g.input.held.insert(key);
        g.tick();
        g.tick(); // holding does not re-fire
        g.input.held.remove(&key);
        g.tick();
    }
    assert_eq!(glob(&g, G_KEY_COUNT), 4.0);
    assert_eq!(glob(&g, G_KEY_SUM), (30 + 32 + 203 + 205) as f32);

    for btn in [4u32, 8] {
        g.jip2.told.pad.buttons = btn;
        g.tick();
        g.tick(); // holding does not re-fire
        g.jip2.told.pad.buttons = 0;
        g.tick();
    }
    assert_eq!(glob(&g, G_BTN_COUNT), 2.0);
    assert_eq!(glob(&g, G_BTN_SUM), 12.0);
}
