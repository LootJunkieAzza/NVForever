//! Production activation ray -> registered source -> injected tile -> draw pieces.
use super::*;
use crate::script::ScriptState;
use crate::tiles::draw::{Look, Select, pieces};
use fnv_script::vm::XVal;
use fnv_ui::MemSource;

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
        "physical-prompt-compile: {:?}",
        compiled.diags
    );
    g.run_query(&source, Default::default())
        .unwrap_or_else(|e| panic!("physical-prompt-source: {body}: {e}"))
}

fn prompt_scene() -> App {
    let mut a = scene(&[(CHEST_REF, 60.0)]);
    let mut script = ScriptState::over_no_plugins();
    let g = &mut script.rt.as_mut().unwrap().game;
    g.cfg.strict = true;
    let files = MemSource::default()
        .with("menus/prompt.xml", r#"<menu name="HUDMainMenu"><_calls>0</_calls><_iconAlpha>0</_iconAlpha><_icon></_icon><rect name="Prompt"><visible>1</visible><width>300</width><height>50</height><text name="Title"><visible>1</visible><font>1</font><string>Supply cache</string><width>200</width><height>20</height></text></rect></menu>"#)
        .with("menus/owned/icon.xml", r#"<image name="Icon"><visible>1</visible><width>30</width><height>30</height><alpha><copy src="io()" trait="_iconAlpha"/></alpha><filename><copy src="io()" trait="_icon"/></filename></image>"#);
    g.ui_tree.load_menu(&files, "menus/prompt.xml").unwrap();
    g.ui_files = Some(Box::new(files));
    a.insert_resource(script);
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (InjectUIXML \"HUDMainMenu/Prompt\" \"data/menus/owned/icon.xml\")"
        ),
        XVal::Num(1.0)
    );
    query(
        &mut a,
        "SetEventHandler \"OnCrosshairOn\" (Begin Function {ref looked}
ModUIFloat \"HUDMainMenu/_calls\" 1
if GetCrosshairRef == looked
SetUIStringAlt \"HUDMainMenu/_icon\" \"textures/owned/current-target.dds\"
SetUIFloat \"HUDMainMenu/_iconAlpha\" 255
endif
End)",
    );
    a
}

#[test]
fn actual_target_ray_refreshes_prompt_image_and_parent_hiding_stays_effective() {
    let mut a = prompt_scene();
    a.update();
    assert_eq!(target(&a), Some((CHEST_REF, Source::Collision)));
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairRef == 0x302)"),
        XVal::Num(1.0),
        "real target getter is a baseline positive control"
    );
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(1.0),
        "physical-prompt: collision acquisition must run the registered image callback"
    );
    {
        let mut script = a.world_mut().resource_mut::<ScriptState>();
        let ui = &mut script.rt.as_mut().unwrap().game.ui_tree;
        ui.update();
        let menu = ui.find_tile("HUDMainMenu").unwrap();
        let measure = |_: f64, text: &str, _: Option<f64>| (text.len() as f64 * 8.0, 20.0);
        let (drawn, _) = pieces(
            ui,
            menu,
            &measure,
            Select::Whole {
                kept: &[],
                strip: None,
            },
            false,
        );
        let icon = ui.find_tile("HUDMainMenu/Prompt/Icon").unwrap();
        assert!(drawn.iter().any(|p| p.tile == icon && p.colour[3] == 1.0 && matches!(&p.look, Look::Picture { name, .. } if name == "textures/owned/current-target.dds")), "real draw pieces must contain the callback's current image: {drawn:?}");
        assert!(
            drawn
                .iter()
                .any(|p| matches!(&p.look, Look::Text { string, .. } if string == "Supply cache"))
        );
    }
    a.update();
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(1.0)
    );
    query(&mut a, "SetUIFloat \"HUDMainMenu/Prompt/visible\" 0");
    {
        let mut script = a.world_mut().resource_mut::<ScriptState>();
        let ui = &mut script.rt.as_mut().unwrap().game.ui_tree;
        ui.update();
        let menu = ui.find_tile("HUDMainMenu").unwrap();
        let icon = ui.find_tile("HUDMainMenu/Prompt/Icon").unwrap();
        assert!(
            !pieces(
                ui,
                menu,
                &|_, _, _| (8.0, 20.0),
                Select::Whole {
                    kept: &[],
                    strip: None
                },
                false
            )
            .0
            .iter()
            .any(|p| p.tile == icon),
            "callback must not defeat genuine parent hiding"
        );
    }
    a.world_mut()
        .resource_mut::<PhysState>()
        .world
        .remove_ref(CHEST_REF);
    a.update();
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairRef == 0)"),
        XVal::Num(1.0)
    );
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(1.0)
    );
    a.world_mut().resource_mut::<PhysState>().world.add_ref(
        &RefPose {
            form_id: DOOR_REF,
            position: [80.0, 0.0, 100.0],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        },
        &[boxed(10.0)],
    );
    a.update();
    assert_eq!(target(&a), Some((DOOR_REF, Source::Collision)));
    assert_eq!(
        query(
            &mut a,
            "SetFunctionValue (GetUIFloat \"HUDMainMenu/_calls\")"
        ),
        XVal::Num(2.0)
    );
    assert_eq!(
        query(&mut a, "SetFunctionValue (GetCrosshairRef == 0x304)"),
        XVal::Num(1.0)
    );
}

#[test]
fn actual_target_ray_dispatches_on_crosshair_off_on_switch_and_leave() {
    let mut a = scene(&[(CHEST_REF, 60.0)]);
    let mut script = ScriptState::over_no_plugins();
    let g = &mut script.rt.as_mut().unwrap().game;
    g.cfg.strict = true;
    let files = MemSource::default().with(
        "menus/prompt_off.xml",
        r#"<menu name="HUDMainMenu"><_seq>0</_seq><_onCalls>0</_onCalls><_offCalls>0</_offCalls><_onOrder>0</_onOrder><_offOrder>0</_offOrder><_offZero>0</_offZero></menu>"#,
    );
    g.ui_tree.load_menu(&files, "menus/prompt_off.xml").unwrap();
    g.ui_files = Some(Box::new(files));
    a.insert_resource(script);
    query(
        &mut a,
        r#"SetEventHandler "OnCrosshairOn" (Begin Function {ref looked}
ModUIFloat "HUDMainMenu/_seq" 1
ModUIFloat "HUDMainMenu/_onCalls" 1
SetUIFloat "HUDMainMenu/_onOrder" (GetUIFloat "HUDMainMenu/_seq")
End)
SetEventHandler "OnCrosshairOff" (Begin Function {ref left}
ModUIFloat "HUDMainMenu/_seq" 1
ModUIFloat "HUDMainMenu/_offCalls" 1
SetUIFloat "HUDMainMenu/_offOrder" (GetUIFloat "HUDMainMenu/_seq")
SetUIFloat "HUDMainMenu/_offZero" (GetCrosshairRef == 0)
SetModLocalData "ray.off_ref" left
End)"#,
    );

    // Frame 1: acquire CHEST_REF (On only, no Off).
    a.update();
    assert_eq!(target(&a), Some((CHEST_REF, Source::Collision)));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_onCalls\")"), XVal::Num(1.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offCalls\")"), XVal::Num(0.0));

    // Frame 2: steady target fires nothing.
    a.update();
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_onCalls\")"), XVal::Num(1.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offCalls\")"), XVal::Num(0.0));

    // Frame 3: swap CHEST_REF -> DOOR_REF in one frame: OnCrosshairOff(CHEST_REF) before OnCrosshairOn(DOOR_REF).
    {
        let mut phys = a.world_mut().resource_mut::<PhysState>();
        phys.world.remove_ref(CHEST_REF);
        phys.world.add_ref(
            &RefPose {
                form_id: DOOR_REF,
                position: [80.0, 0.0, 100.0],
                rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                scale: 1.0,
            },
            &[boxed(10.0)],
        );
    }
    a.update();
    assert_eq!(target(&a), Some((DOOR_REF, Source::Collision)));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offCalls\")"), XVal::Num(1.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_onCalls\")"), XVal::Num(2.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offOrder\")"), XVal::Num(2.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_onOrder\")"), XVal::Num(3.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetModLocalData \"ray.off_ref\")"), XVal::Ref(CHEST_REF));

    // Frame 4: remove DOOR_REF: OnCrosshairOff(DOOR_REF) with GetCrosshairRef == 0.
    a.world_mut().resource_mut::<PhysState>().world.remove_ref(DOOR_REF);
    a.update();
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offCalls\")"), XVal::Num(2.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_onCalls\")"), XVal::Num(2.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offZero\")"), XVal::Num(1.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetModLocalData \"ray.off_ref\")"), XVal::Ref(DOOR_REF));

    // Frame 5: empty steady frame does not re-fire.
    a.update();
    assert_eq!(query(&mut a, "SetFunctionValue (GetUIFloat \"HUDMainMenu/_offCalls\")"), XVal::Num(2.0));
}
