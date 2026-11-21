//! Real compiled callback/image controls with a process-owned CompileScript reader.
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
        "crosshair-water-compile: {:?}; {body}",
        compiled.diags
    );
    g.run_query(&source, Default::default())
        .unwrap_or_else(|e| panic!("crosshair-water-source: {body}: {e}"))
}

#[test]
fn current_object_water_drives_file_callback_image_and_effective_accessor_without_world_mutation() {
    let callback = br#"ref water
Begin Function {}
water = GetCrosshairWater
if water
SetUIStringAlt "HUDMainMenu/_image" "textures/owned/water-source.dds"
else
SetUIStringAlt "HUDMainMenu/_image" "textures/owned/ordinary-object.dds"
endif
SetUIFloat "HUDMainMenu/_alpha" 255
SetFunctionValue water
End
"#;
    let reader: fnv_game::mod_files::Reader = Arc::new(move |p| {
        p.eq_ignore_ascii_case("nvse\\user_defined_functions\\owned\\water.gek")
            .then(|| callback.to_vec())
    });
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots {
        reader: Some(reader),
        ..Default::default()
    });
    let _files = Files;
    let activator = |id, water: Option<u32>| {
        let mut fields = vec![
            sub(b"EDID", z(&format!("OwnedSource{id:X}"))),
            sub(b"FULL", z("Water source")),
        ];
        if let Some(w) = water {
            fields.push(sub(b"WNAM", w.to_le_bytes().to_vec()));
        }
        record(b"ACTI", 0, id, &fields)
    };
    let placed = |id, base: u32| {
        record(
            b"REFR",
            0,
            id,
            &[
                sub(b"NAME", base.to_le_bytes().to_vec()),
                sub(b"DATA", vec![0; 24]),
            ],
        )
    };
    let cell = [
        record(b"CELL", 0, 0xF00, &[sub(b"DATA", vec![1])]),
        group(
            0xF00u32.to_le_bytes(),
            6,
            &group(
                0xF00u32.to_le_bytes(),
                9,
                &[
                    placed(0xF20, 0xF10),
                    placed(0xF21, 0xF11),
                    placed(0xF22, 0xF12),
                ]
                .concat(),
            ),
        ),
    ]
    .concat();
    let water = |id| {
        record(
            b"WATR",
            0,
            id,
            &[sub(b"EDID", z(&format!("OwnedWater{id:X}")))],
        )
    };
    let nonwater = record(
        b"CONT",
        0,
        0xF12,
        &[
            sub(b"EDID", z("OwnedCache")),
            sub(b"FULL", z("Supply cache")),
        ],
    );
    let bytes = [
        tes4(),
        top(
            b"ACTI",
            &[activator(0xF10, Some(0xF01)), activator(0xF11, None)].concat(),
        ),
        top(b"CONT", &nonwater),
        top(b"WATR", &[water(0xF01), water(0xF02)].concat()),
        top(
            b"CELL",
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedWaterSource.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&path).unwrap(),
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
    let compiled_callback = fnv_script::compile::compile_runtime(
        std::str::from_utf8(callback).unwrap(),
        &g.data.table,
        &fnv_game::game::GameEnv { g: &g },
    );
    assert!(
        compiled_callback.errors().next().is_none(),
        "crosshair-water-file-compile: {:?}",
        compiled_callback.diags
    );
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue ((CompileScript \"Owned/Water.gek\") != 0)"
        ),
        XVal::Num(1.0),
        "file-backed callback must actually compile before behavioral assertions"
    );
    let ui = MemSource::default()
        .with("menus/water.xml", r#"<menu name="HUDMainMenu"><_image></_image><_alpha>0</_alpha><rect name="Prompt"><visible>1</visible><text name="Title"><string>Water source</string></text></rect></menu>"#)
        .with("menus/owned/image.xml", r#"<image name="Icon"><visible>1</visible><width>30</width><height>30</height><filename><copy src="io()" trait="_image"/></filename><alpha><copy src="io()" trait="_alpha"/></alpha></image>"#);
    g.ui_tree.load_menu(&ui, "menus/water.xml").unwrap();
    g.ui_files = Some(Box::new(ui));
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (InjectUIXML \"HUDMainMenu/Prompt\" \"data/menus/owned/image.xml\")"
        ),
        XVal::Num(1.0)
    );
    for target in [None, Some(0xBAD), Some(0xF22)] {
        g.senses.crosshair_ref = target.map(FormId);
        assert_eq!(
            query(&mut g, "SetFunctionValue (GetCrosshairWater)"),
            XVal::Ref(0),
            "existing absent/invalid/nonwater result is a baseline positive control"
        );
    }
    g.senses.crosshair_ref = Some(FormId(0xF20));
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetActivatorWaterType 0xF10)"),
        XVal::Ref(0xF01),
        "existing ACTI accessor is a baseline positive control"
    );
    let before = serde_json::to_value(g.world.snapshot()).unwrap();
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCrosshairWater)"),
        XVal::Ref(0xF01),
        "crosshair-water: current target must supply its actual WNAM form"
    );
    assert_eq!(serde_json::to_value(g.world.snapshot()).unwrap(), before);
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (Call (CompileScript \"Owned/Water.gek\"))"
        ),
        XVal::Ref(0xF01)
    );
    g.ui_tree.update();
    assert_eq!(
        g.ui_tree
            .get_string("HUDMainMenu/Prompt/Icon/filename")
            .as_deref(),
        Some("textures/owned/water-source.dds")
    );
    assert_eq!(
        g.ui_tree.get_float("HUDMainMenu/Prompt/Icon/alpha"),
        Some(255.0)
    );
    query(&mut g, "SetActivatorWaterType 0xF02 0xF10");
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetActivatorWaterType 0xF10)"),
        XVal::Ref(0xF02)
    );
    assert_eq!(
        query(&mut g, "SetFunctionValue (GetCrosshairWater)"),
        XVal::Ref(0xF02),
        "same effective record authority as the accepted accessor"
    );
    for target in [Some(0xF21), Some(0xF22), Some(0xBAD), None] {
        g.senses.crosshair_ref = target.map(FormId);
        let before = serde_json::to_value(g.world.snapshot()).unwrap();
        assert_eq!(
            query(
                &mut g,
                "SetFunctionValue (Call (CompileScript \"Owned/Water.gek\"))"
            ),
            XVal::Ref(0)
        );
        assert_eq!(serde_json::to_value(g.world.snapshot()).unwrap(), before);
        g.ui_tree.update();
        assert_eq!(
            g.ui_tree
                .get_string("HUDMainMenu/Prompt/Icon/filename")
                .as_deref(),
            Some("textures/owned/ordinary-object.dds")
        );
    }
    g.senses.crosshair_ref = Some(FormId(0xF20));
    assert_eq!(
        query(
            &mut g,
            "SetFunctionValue (Call (CompileScript \"Owned/Water.gek\"))"
        ),
        XVal::Ref(0xF02)
    );
    g.ui_tree.update();
    assert_eq!(
        g.ui_tree
            .get_string("HUDMainMenu/Prompt/Icon/filename")
            .as_deref(),
        Some("textures/owned/water-source.dds")
    );
}
