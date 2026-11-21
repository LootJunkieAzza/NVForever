//! CompileScript needs a process-owned Data reader; keep it outside parallel fixtures.
mod common;
use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{Game, GameConfig, GameData};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use std::sync::Arc;

struct Files;
impl Drop for Files {
    fn drop(&mut self) {
        fnv_game::mod_files::set_roots(Default::default());
    }
}

fn query(g: &mut Game, body: &str) -> XVal {
    let source = format!("Begin Function {{}}\n{body}\nEnd\n");
    let compiled = fnv_script::compile::compile_runtime(
        &source,
        &g.data.table,
        &fnv_game::game::GameEnv { g: &*g },
    );
    assert!(
        compiled.errors().next().is_none(),
        "crosshair-source-compile: {:?}; {body}",
        compiled.diags
    );
    g.run_query(&source, Default::default())
        .unwrap_or_else(|e| panic!("crosshair-source: {body}: {e}"))
}

#[test]
fn compiled_prompt_callback_refreshes_injected_image_once_per_target_with_filters_and_current_getter()
 {
    let callback = br#"ref looked
Begin Function {looked}
ModUIFloat "HUDMainMenu/_calls" 1
ModUIFloat "HUDMainMenu/_sequence" 1
SetUIFloat "HUDMainMenu/_sourceOrder" (GetUIFloat "HUDMainMenu/_sequence")
SetUIFloat "HUDMainMenu/_fresh" (GetCrosshairRef == looked)
if looked == 3616
SetUIStringAlt "HUDMainMenu/_icon" "textures/owned/container.dds"
else
SetUIStringAlt "HUDMainMenu/_icon" "textures/owned/door.dds"
endif
SetUIFloat "HUDMainMenu/_iconAlpha" 255
SetUIFloat "HUDMainMenu/_targetLiteral" 3616
End
"#;
    let reader: fnv_game::mod_files::Reader = Arc::new(move |p| {
        p.eq_ignore_ascii_case("nvse\\user_defined_functions\\owned\\prompt.gek")
            .then(|| callback.to_vec())
    });
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots {
        reader: Some(reader),
        ..Default::default()
    });
    let _files = Files;
    let chest = record(
        b"CONT",
        0,
        0xE10,
        &[
            sub(b"EDID", z("OwnedChest")),
            sub(b"FULL", z("Supply cache")),
        ],
    );
    let door = record(
        b"DOOR",
        0,
        0xE11,
        &[
            sub(b"EDID", z("OwnedDoor")),
            sub(b"FULL", z("Workshop door")),
        ],
    );
    let placed = |id, base: u32, editor_id: &str| {
        record(
            b"REFR",
            0,
            id,
            &[
                sub(b"EDID", z(editor_id)),
                sub(b"NAME", base.to_le_bytes().to_vec()),
                sub(b"DATA", vec![0; 24]),
            ],
        )
    };
    let cell = [
        record(b"CELL", 0, 0xE00, &[sub(b"DATA", vec![1])]),
        group(
            0xE00u32.to_le_bytes(),
            6,
            &group(
                0xE00u32.to_le_bytes(),
                9,
                &[placed(0xE20, 0xE10, "OwnedChestRef"), placed(0xE21, 0xE11, "OwnedDoorRef")].concat(),
            ),
        ),
    ]
    .concat();
    let list = record(
        b"FLST",
        0,
        0xE30,
        &[sub(b"EDID", z("OwnedDoorList")), sub(b"LNAM", 0xE11u32.to_le_bytes().to_vec())],
    );
    let bytes = [
        tes4(),
        top(b"CONT", &chest),
        top(b"DOOR", &door),
        top(b"FLST", &list),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedPrompt.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let table_path = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&table_path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&table_path).unwrap(),
        &json,
        &refs,
    );
    let mut g = Game::new(
        &data,
        World::new(&forms, &refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    );
    let before = g.events.len();
    g.set_crosshair_ref(Some(FormId(0xE20)));
    g.set_crosshair_ref(None);
    assert_eq!(
        g.events.len(),
        before,
        "unused transition publishes the getter without a callback trace"
    );
    let compiled_callback = fnv_script::compile::compile_mod_runtime(
        std::str::from_utf8(callback).unwrap(),
        &g.data.table,
        &fnv_game::game::GameEnv { g: &g },
    );
    assert!(
        compiled_callback.errors().next().is_none(),
        "crosshair-file-compile: {:?}",
        compiled_callback.diags
    );
    let source = MemSource::default()
        .with("menus/owned.xml", r#"<menu name="HUDMainMenu"><_calls>0</_calls><_sequence>0</_sequence><_sourceOrder>0</_sourceOrder><_johnnyOrder>0</_johnnyOrder><_johnnyOld>0</_johnnyOld><_fresh>0</_fresh><_typed>0</_typed><_wrongType>0</_wrongType><_doorType>0</_doorType><_byForm>0</_byForm><_byList>0</_byList><_byNumber>0</_byNumber><_byNumberList>0</_byNumberList><_iconAlpha>0</_iconAlpha><_icon></_icon><_targetLiteral>0</_targetLiteral><rect name="Prompt"><visible>1</visible><text name="Title"><string>Supply cache</string></text></rect></menu>"#)
        .with("menus/owned/icon.xml", r#"<image name="Icon"><visible>1</visible><width>30</width><height>30</height><alpha><copy src="io()" trait="_iconAlpha"/></alpha><filename><copy src="io()" trait="_icon"/></filename></image>"#);
    g.ui_tree.load_menu(&source, "menus/owned.xml").unwrap();
    g.ui_files = Some(Box::new(source));
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (InjectUIXML \"HUDMainMenu/Prompt\" \"data/menus/owned/icon.xml\")"
        ),
        XVal::Num(1.0)
    );
    query(&mut g, "SetEventHandler \"OnCrosshairOn\" (CompileScript \"Owned/Prompt.gek\")
SetEventHandler \"OnCrosshairOn:27\" ({ref looked} => ModUIFloat \"HUDMainMenu/_typed\" 1)
SetEventHandler \"OnCrosshairOn:24\" ({ref looked} => ModUIFloat \"HUDMainMenu/_wrongType\" 1)
SetEventHandler \"OnCrosshairOn:28\" ({ref looked} => ModUIFloat \"HUDMainMenu/_doorType\" 1)
SetEventHandler \"OnCrosshairOn\" ({ref looked} => ModUIFloat \"HUDMainMenu/_byForm\" 1) \"first\"::OwnedChestRef
SetEventHandler \"OnCrosshairOn\" ({ref looked} => ModUIFloat \"HUDMainMenu/_byList\" 1) \"first\"::OwnedDoorList
SetEventHandler \"OnCrosshairOn\" ({ref looked} => ModUIFloat \"HUDMainMenu/_byNumber\" 1) \"first\"::0xE20
SetEventHandler \"OnCrosshairOn\" ({ref looked} => ModUIFloat \"HUDMainMenu/_byNumberList\" 1) \"first\"::0xE30
SetJohnnyOnCrosshairEventHandler 1 (Begin Function {ref looked}
ModUIFloat \"HUDMainMenu/_sequence\" 1
SetUIFloat \"HUDMainMenu/_johnnyOrder\" (GetUIFloat \"HUDMainMenu/_sequence\")
SetUIFloat \"HUDMainMenu/_johnnyOld\" (GetCrosshairRef != looked)
End) 0");
    let registered_source = "array_var registered\narray_var handlerEntry\nBegin Function {}\nregistered = GetEventHandlers \"OnCrosshairOn\" 0 1\nhandlerEntry = registered[0]\nSetFunctionValue handlerEntry[0]\nEnd";
    let registered_compiled = fnv_script::compile::compile_runtime(
        registered_source,
        &g.data.table,
        &fnv_game::game::GameEnv { g: &g },
    );
    assert!(
        registered_compiled.errors().next().is_none(),
        "crosshair-registration-inspection-compile: {:?}",
        registered_compiled.diags
    );
    let callable = g
        .run_query(registered_source, Default::default())
        .unwrap_or_else(|e| {
            panic!(
                "crosshair-registration-inspection: {e}; trace: {:?}",
                g.events
            )
        });
    let callable_id = match &callable {
        XVal::Ref(id) => Some(*id),
        XVal::Num(n)
            if n.is_finite() && n.fract() == 0.0 && *n >= 0.0 && *n <= f64::from(u32::MAX) =>
        {
            Some(*n as u32)
        }
        _ => None,
    };
    assert!(
        callable_id.is_some_and(|id| (fnv_game::game::COMPILED_BASE
            ..fnv_game::game::TEMP_REF_BASE)
            .contains(&id)),
        "crosshair-original-registration: {callable:?}; trace: {:?}",
        g.events
    );
    // Inspect the original registrations while their returned arrays are live.
    // Numeric hex tokens are numbers; a form filter must actually contain a Form.
    for (row, expected) in [
        (1, XVal::Ref(0xE20)),
        (2, XVal::Ref(0xE30)),
        (3, XVal::Num(f64::from(0xE20u32))),
        (4, XVal::Num(f64::from(0xE30u32))),
    ] {
        let inspect = format!("array_var registered\narray_var handlerEntry\narray_var filterMap\nBegin Function {{}}\nregistered = GetEventHandlers \"OnCrosshairOn\" 0 1\nhandlerEntry = registered[{row}]\nfilterMap = handlerEntry[1]\nSetFunctionValue filterMap[\"first\"]\nEnd");
        let compiled = fnv_script::compile::compile_runtime(&inspect, &g.data.table, &fnv_game::game::GameEnv { g: &g });
        assert!(compiled.errors().next().is_none(), "crosshair-filter-inspection-compile: {:?}", compiled.diags);
        assert_eq!(
            g.run_query(&inspect, Default::default()).unwrap(), expected,
            "crosshair-filter-admission: registered row {row} must retain its actual Form/Number kind and exact value"
        );
    }
    g.set_crosshair_ref(Some(FormId(0xE20)));
    g.ui_tree.update();
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCrosshairRef == 0xE20)"),
        XVal::Num(1.0),
        "existing target getter is a baseline positive control"
    );
    assert_eq!(
        query(&mut g, "ref typedTarget\nlet typedTarget := GetCrosshairRef\nSetFunctionValue (typedTarget.GetType)"),
        XVal::Num(27.0),
        "crosshair-container-type: actual source GetType must identify the container before typed delivery is checked"
    );
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_johnnyOrder"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_johnnyOld"), Some(1.0));
    assert_eq!(
        g.ui_tree.get_float("HUDMainMenu/_calls"),
        Some(1.0),
        "crosshair-prompt: real registered source must run on first acquired target; trace: {:?}",
        g.events
    );
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_fresh"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_sourceOrder"), Some(2.0));
    assert_eq!(
        g.ui_tree.get_float("HUDMainMenu/_targetLiteral"),
        Some(f64::from(0xE20u32)),
        "crosshair-source-literal: callback decimal must equal the actual target FormID"
    );
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_typed"), Some(1.0),
        "crosshair-typed-container: registered type27 must run for the actual CONT; trace: {:?}", g.events);
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_wrongType"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byForm"), Some(1.0), "crosshair-form-filter: the admitted Ref(E20) filter must receive its actual target");
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byList"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byNumber"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byNumberList"), Some(0.0));
    assert_eq!(
        g.ui_tree
            .get_string("HUDMainMenu/Prompt/Icon/filename")
            .as_deref(),
        Some("textures/owned/container.dds")
    );
    assert_eq!(
        g.ui_tree.get_float("HUDMainMenu/Prompt/Icon/alpha"),
        Some(255.0)
    );
    for _ in 0..3 {
        g.set_crosshair_ref(Some(FormId(0xE20)));
    }
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(1.0)
    );
    g.set_crosshair_ref(Some(FormId(0xE21)));
    g.ui_tree.update();
    assert_eq!(
        g.ui_tree
            .get_string("HUDMainMenu/Prompt/Icon/filename")
            .as_deref(),
        Some("textures/owned/door.dds")
    );
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byForm"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byList"), Some(1.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byNumber"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_byNumberList"), Some(0.0));
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_typed"), Some(1.0),
        "crosshair-typed-door-neighbor: DOOR must not increment the CONT listener");
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_doorType"), Some(1.0),
        "crosshair-typed-door: registered type28 must run for the actual DOOR; trace: {:?}", g.events);
    assert_eq!(g.ui_tree.get_float("HUDMainMenu/_wrongType"), Some(0.0));
    assert_eq!(
        query(&mut g, "ref typedTarget\nlet typedTarget := GetCrosshairRef\nSetFunctionValue (typedTarget.GetType)"),
        XVal::Num(28.0),
        "crosshair-door-type: typed event matches actual source GetType"
    );
    g.set_crosshair_ref(None);
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCrosshairRef == 0)"),
        XVal::Num(1.0)
    );
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(2.0)
    );
    g.set_crosshair_ref(Some(FormId(0xE20)));
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(3.0)
    );
    query(
        &mut g,
        "RemoveEventHandler \"OnCrosshairOn\" (CompileScript \"Owned/Prompt.gek\")",
    );
    g.set_crosshair_ref(Some(FormId(0xE21)));
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(3.0),
        "removal must stop the original callback without suppressing target publication"
    );
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCrosshairRef == 0xE21)"),
        XVal::Num(1.0)
    );
}
