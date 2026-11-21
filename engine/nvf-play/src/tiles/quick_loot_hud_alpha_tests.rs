//! Synthetic hover text consumers of engine-created HUD copies; no installed assets.
use super::layout::{self, Kind};
use crate::script::ScriptState;
use fnv_game::game::RunCtx;
use fnv_game::ui_hud::{self, Shown, Texts};
use fnv_script::vm::XVal;
use fnv_ui::{MemSource, Value};

fn state(alpha: &str) -> ScriptState {
    let mut state = ScriptState::over_no_plugins();
    let game = &mut state.rt.as_mut().unwrap().game;
    let files = MemSource::default().with(
        "owned-hover.xml",
        &format!(
            r#"
      <menu name="HUDMainMenu"><alpha>255</alpha>
        <rect name="ActionPoints"/>
        <template name="template_justify_right_text">
          <text name="justify_right_text"><font>2</font>{alpha}</text>
        </template>
        <rect name="Hover"><_textAlpha>0</_textAlpha>
          <text name="Title"><string>Supply locker</string><font>2</font>
            <alpha><copy src="parent()" trait="_textAlpha"/></alpha></text>
          <text name="Row"><string>Spare component (3)</string><font>2</font>
            <y>37</y><alpha><copy src="parent()" trait="_textAlpha"/></alpha></text>
          <rect name="Backdrop"><alpha>149</alpha><width>383</width><height>91</height></rect>
        </rect>
      </menu>"#
        ),
    );
    game.ui_tree.load_menu(&files, "owned-hover.xml").unwrap();
    ui_hud::build(&mut game.ui_tree);
    game.ui_tree.update();
    state
}

fn write(state: &mut ScriptState, shown: bool) {
    let game = &mut state.rt.as_mut().unwrap().game;
    ui_hud::write(
        &mut game.ui_tree,
        Shown {
            root: shown,
            corners: shown,
            ..Default::default()
        },
        &Texts {
            radiation_word: String::new(),
            radiation_value: String::new(),
            xp_label: String::new(),
            xp_level_up: String::new(),
            needs: std::array::from_fn(|_| String::new()),
            sneak: None,
            prompt_key: None,
            prompt_verb: None,
            frame: ui_hud::Frame {
                ammo: "17/51".into(),
                ..Default::default()
            },
        },
    );
    game.ui_tree.update();
}

fn forward(state: &mut ScriptState) -> f64 {
    let game = &mut state.rt.as_mut().unwrap().game;
    let result = game.run_query(r#"float observed
      Begin Function {}
        let observed := GetUIFloatAlt "HUDMainMenu/ActionPoints/justify_right_text:1/alpha"
        if observed == -999
          let observed := GetUIFloatAlt "HUDMainMenu/OtherWrap/ActionPoints/justify_right_text:1/alpha"
        endif
        SetUIFloatAlt "HUDMainMenu/Hover/_textAlpha" observed
        SetFunctionValue observed
      End"#, RunCtx::default()).unwrap();
    game.ui_tree.update();
    let XVal::Num(value) = result else {
        panic!("numeric alpha query: {result:?}")
    };
    value
}

fn piece(state: &ScriptState, name: &str) -> layout::DrawItem {
    let ui = &state.rt.as_ref().unwrap().game.ui_tree;
    let root = ui.find_tile("HUDMainMenu").unwrap();
    let tile = ui.find_tile(&format!("HUDMainMenu/Hover/{name}")).unwrap();
    layout::layout(
        ui,
        root,
        &|_, text, _| (text.len() as f64 * 11.0, 29.0),
        false,
    )
    .into_iter()
    .find(|item| item.tile == tile)
    .unwrap()
}

#[test]
fn omitted_ammo_alpha_reaches_compiled_hover_forwarding_and_real_text_pieces() {
    let mut state = state("");
    write(&mut state, true);
    {
        let ui = &state.rt.as_ref().unwrap().game.ui_tree;
        let root = ui.find_tile("HUDMainMenu").unwrap();
        let ammo = ui
            .find_tile("HUDMainMenu/ActionPoints/justify_right_text:1")
            .unwrap();
        let pieces = layout::layout(
            ui,
            root,
            &|_, text, _| (text.len() as f64 * 11.0, 29.0),
            false,
        );
        let ammo = pieces.iter().find(|item| item.tile == ammo).unwrap();
        assert_eq!(
            ammo.alpha, 255.0,
            "ordinary ammo rendering already uses the omitted-alpha fallback"
        );
        assert!(matches!(&ammo.kind, Kind::Text(text) if text.string == "17/51"));
    }
    let value = forward(&mut state);
    for name in ["Title", "Row"] {
        let item = piece(&state, name);
        assert!(matches!(&item.kind, Kind::Text(text) if !text.string.is_empty()));
        assert!(item.visible && item.rect.height > 0.0);
        assert_eq!(
            item.alpha, 255.0,
            "hover-alpha: populated text must survive the actual copy/query/forward/layout chain"
        );
    }
    assert_eq!(value, 255.0);
    assert_eq!(
        piece(&state, "Backdrop").alpha,
        149.0,
        "independent backdrop never proves text visibility"
    );
}

#[test]
fn explicit_template_alpha_and_later_foreign_hiding_remain_authoritative() {
    for (initial, expected) in [
        ("<alpha>0</alpha>", 0.0),
        ("<alpha><copy>83</copy></alpha>", 83.0),
    ] {
        let mut state = state(initial);
        write(&mut state, true);
        assert_eq!(forward(&mut state), expected);
        assert_eq!(piece(&state, "Row").alpha, expected);
        let game = &mut state.rt.as_mut().unwrap().game;
        assert!(game
            .ui_tree
            .set_float("HUDMainMenu/ActionPoints/justify_right_text:1/alpha", 0.0));
        write(&mut state, true);
        assert_eq!(
            forward(&mut state),
            0.0,
            "HUD publication must preserve a later script hide"
        );
        assert_eq!(piece(&state, "Row").alpha, 0.0);
    }
}

#[test]
fn local_alpha_does_not_bypass_parent_opacity_visibility_or_missing_paths() {
    let mut state = state("<alpha>191</alpha>");
    write(&mut state, true);
    {
        let game = &mut state.rt.as_mut().unwrap().game;
        let root = game.ui_tree.find_tile("HUDMainMenu").unwrap();
        game.ui_tree.set_trait(root, "alpha", Value::Num(85.0));
        let result = game
            .run_query(
                r#"Begin Function {}
          SetFunctionValue (GetUIFloatAlt "HUDMainMenu/Missing/alpha")
        End"#,
                RunCtx::default(),
            )
            .unwrap();
        assert_eq!(result, XVal::Num(-999.0));
    }
    assert_eq!(
        forward(&mut state),
        191.0,
        "the API returns local alpha, not inherited opacity"
    );
    assert!((piece(&state, "Row").alpha - 191.0 / 3.0).abs() < 1e-9);
    write(&mut state, false);
    assert_eq!(forward(&mut state), 191.0);
    assert!(
        !piece(&state, "Row").visible,
        "parent hiding remains effective"
    );
}
