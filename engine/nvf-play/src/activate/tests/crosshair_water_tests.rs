//! Actual activation collision -> current-target water -> source/image consumer.
use super::*;
use crate::script::ScriptState;
use crate::tiles::draw::{Look, Select, pieces};
use fnv_esm::raw::{Subrecord, write_subrecords};
use fnv_script::vm::XVal;
use fnv_ui::MemSource;

fn rec(sig: [u8; 4], id: u32, fields: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let body = write_subrecords(
        &fields
            .iter()
            .map(|(sig, data)| Subrecord {
                sig: *sig,
                data: data.clone(),
                extended: false,
            })
            .collect::<Vec<_>>(),
    );
    [
        sig.to_vec(),
        (body.len() as u32).to_le_bytes().to_vec(),
        vec![0; 4],
        id.to_le_bytes().to_vec(),
        vec![0; 4],
        15u16.to_le_bytes().to_vec(),
        vec![0; 2],
        body,
    ]
    .concat()
}
fn group(label: [u8; 4], kind: i32, content: &[u8]) -> Vec<u8> {
    [
        b"GRUP".to_vec(),
        ((24 + content.len()) as u32).to_le_bytes().to_vec(),
        label.to_vec(),
        kind.to_le_bytes().to_vec(),
        vec![0; 8],
        content.to_vec(),
    ]
    .concat()
}
fn query(a: &mut App, body: &str) -> XVal {
    let mut script = a.world_mut().resource_mut::<ScriptState>();
    let g = &mut script.rt.as_mut().unwrap().game;
    let source = format!("Begin Function {{}}\n{body}\nEnd\n");
    let compiled = fnv_script::compile::compile_runtime(
        &source,
        &g.data.table,
        &fnv_game::game::GameEnv { g: &*g },
    );
    assert!(
        compiled.errors().next().is_none(),
        "physical-water-compile: {:?}",
        compiled.diags
    );
    g.run_query(&source, Default::default())
        .unwrap_or_else(|e| panic!("physical-water-source: {body}: {e}"))
}

#[test]
fn actual_collision_target_supplies_water_to_callback_and_drawn_image_then_clears_on_eviction() {
    let header = rec(
        *b"TES4",
        0,
        &[(
            *b"HEDR",
            [
                1.34f32.to_le_bytes(),
                0u32.to_le_bytes(),
                0x1000u32.to_le_bytes(),
            ]
            .concat(),
        )],
    );
    let water = rec(
        *b"WATR",
        0xF01,
        &[(*b"EDID", b"OwnedTargetWater\0".to_vec())],
    );
    let base = rec(
        *b"ACTI",
        LEVER,
        &[
            (*b"EDID", b"OwnedWaterLever\0".to_vec()),
            (*b"WNAM", 0xF01u32.to_le_bytes().to_vec()),
        ],
    );
    let reference = rec(
        *b"REFR",
        LEVER_REF,
        &[
            (*b"NAME", LEVER.to_le_bytes().to_vec()),
            (*b"DATA", vec![0; 24]),
        ],
    );
    let cell = [
        rec(*b"CELL", CELL_EXT, &[(*b"DATA", vec![1])]),
        group(
            CELL_EXT.to_le_bytes(),
            6,
            &group(CELL_EXT.to_le_bytes(), 9, &reference),
        ),
    ]
    .concat();
    let bytes = [
        header,
        group(*b"WATR", 0, &water),
        group(*b"ACTI", 0, &base),
        group(
            *b"CELL",
            0,
            &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)),
        ),
    ]
    .concat();
    let plugins = [fnv_esm::Plugin::parse(&bytes, "OwnedPhysicalWater.esm".into()).unwrap()];
    let mut script = ScriptState::over_plugins(&plugins);
    let g = &mut script.rt.as_mut().unwrap().game;
    g.cfg.strict = true;
    let files = MemSource::default().with("menus/water.xml", r#"<menu name="HUDMainMenu"><_image></_image><_alpha>0</_alpha><image name="Icon"><visible>1</visible><width>30</width><height>30</height><filename><copy src="io()" trait="_image"/></filename><alpha><copy src="io()" trait="_alpha"/></alpha></image><text name="Title"><visible>1</visible><font>1</font><width>200</width><height>20</height><string>Water source</string></text></menu>"#);
    g.ui_tree.load_menu(&files, "menus/water.xml").unwrap();
    let mut a = scene(&[(LEVER_REF, 60.0)]);
    a.insert_resource(script);
    a.update();
    assert_eq!(target(&a), Some((LEVER_REF, Source::Collision)));
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairRef == 0x308)"),
        XVal::Num(1.0)
    );
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetActivatorWaterType 0x160)"),
        XVal::Ref(0xF01)
    );
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairWater)"),
        XVal::Ref(0xF01),
        "physical-water: actual activation target must return its object water"
    );
    let callback = "if GetCrosshairWater\nSetUIStringAlt \"HUDMainMenu/_image\" \"textures/owned/current-water.dds\"\nSetUIFloat \"HUDMainMenu/_alpha\" 255\nelse\nSetUIFloat \"HUDMainMenu/_alpha\" 0\nendif";
    query(&mut a, callback);
    {
        let mut script = a.world_mut().resource_mut::<ScriptState>();
        let ui = &mut script.rt.as_mut().unwrap().game.ui_tree;
        ui.update();
        let menu = ui.find_tile("HUDMainMenu").unwrap();
        let (drawn, _) = pieces(
            ui,
            menu,
            &|_: f64, text: &str, _: Option<f64>| (text.len() as f64 * 8.0, 20.0),
            Select::Whole {
                kept: &[],
                strip: None,
            },
            false,
        );
        assert!(drawn.iter().any(|p| p.colour[3] == 1.0 && matches!(&p.look, Look::Picture { name, .. } if name == "textures/owned/current-water.dds")));
        assert!(
            drawn
                .iter()
                .any(|p| matches!(&p.look, Look::Text { string, .. } if string == "Water source"))
        );
    }
    a.world_mut()
        .resource_mut::<PhysState>()
        .world
        .remove_ref(LEVER_REF);
    a.update();
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairRef == 0)"),
        XVal::Num(1.0)
    );
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairWater)"),
        XVal::Ref(0)
    );
    query(&mut a, callback);
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_alpha\")"
        ),
        XVal::Num(0.0)
    );
}
