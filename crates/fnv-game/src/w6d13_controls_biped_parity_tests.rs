use super::{call as input_call, ScriptInput};
use crate::common::*;
use crate::{game::GameConfig, Game, GameData};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

fn build_fixture(bytes: &[u8], name: &str) -> (GameData, FormStore, RefTable) {
    let plugins = [Plugin::parse(bytes, name.into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    (data, forms, refs)
}

fn minimal_fixture(name: &str) -> (GameData, FormStore, RefTable) {
    build_fixture(&tes4(), name)
}

fn new_game<'a>(data: &'a GameData, forms: &'a FormStore, refs: &'a RefTable) -> Game<'a> {
    Game::new(data, World::new(forms, refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn qn_body(g: &mut Game<'_>, body: &str) -> f64 {
    let src = format!("Begin Function {{}}\n{body}\nEnd\n");
    match g.run_query(&src, Default::default()).unwrap() {
        XVal::Num(n) => n,
        other => panic!("expected Num, got {other:?}"),
    }
}

fn qn_expr(g: &mut Game<'_>, expr: &str) -> f64 {
    qn_body(g, &format!("SetFunctionValue ({expr})"))
}

#[test]
fn disableplayercontrols_zero_and_short_args_default_to_first_three_groups() {
    let (data, forms, refs) = minimal_fixture("W6D13Dpc.esm");
    let mut g = new_game(&data, &forms, &refs);

    qn_body(&mut g, "DisablePlayerControls\nSetFunctionValue 0");
    assert_eq!(g.player.controls_disabled, [true, true, true, false, false, false, false]);

    qn_body(&mut g, "EnablePlayerControls\nDisablePlayerControls 1 1\nSetFunctionValue 0");
    assert_eq!(g.player.controls_disabled, [true, true, true, false, false, false, false]);

    qn_body(&mut g, "EnablePlayerControls\nDisablePlayerControls 0 0 0 1 0 0 0\nSetFunctionValue 0");
    assert_eq!(g.player.controls_disabled, [false, false, false, true, false, false, false]);

    qn_body(&mut g, "DisablePlayerControls 1 1 1 1 1 1 1\nEnablePlayerControls 1 1 1 1 1 1\nSetFunctionValue (GetPlayerControlsDisabled)");
    assert_eq!(g.player.controls_disabled, [false; 7]);

    // Preserve co-op guest guard from main: guest scripts do not lock local controls.
    g.coop_guest = true;
    qn_body(&mut g, "DisablePlayerControls\nSetFunctionValue 0");
    assert_eq!(g.player.controls_disabled, [false; 7]);
}

#[test]
fn gamepad_deadzone_getters_default_to_024_and_027_and_roundtrip_setters() {
    let (data, forms, refs) = minimal_fixture("W6D13Dz.esm");
    let mut g = new_game(&data, &forms, &refs);
    let call = |g: &mut Game<'_>, n: &str, a: &[XVal]| match crate::nvse_jip2::call(g, &Default::default(), n, None, a).unwrap().unwrap() {
        XVal::Num(v) => v,
        other => panic!("expected Num, got {other:?}"),
    };
    assert!((call(&mut g, "getdeadzonels", &[]) - 0.24).abs() < 1e-6);
    assert!((call(&mut g, "getdeadzoners", &[]) - 0.27).abs() < 1e-6);
    call(&mut g, "setdeadzonels", &[XVal::Num(0.35)]);
    call(&mut g, "setdeadzoners", &[XVal::Num(0.40)]);
    assert!((call(&mut g, "getdeadzonels", &[]) - 0.35).abs() < 1e-6);
    assert!((call(&mut g, "getdeadzoners", &[]) - 0.40).abs() < 1e-6);
}

#[test]
fn getcontrol_returns_unbound_for_non_keyboard_mouse_type_2() {
    let mut input = ScriptInput::default();
    assert_eq!(input_call(&mut input, "getcontrol", &[XVal::Num(27.0), XVal::Num(0.0)]).unwrap(), XVal::Num(44.0));
    assert_eq!(input_call(&mut input, "getcontrol", &[XVal::Num(27.0), XVal::Num(1.0)]).unwrap(), XVal::Num(-1.0));
    assert_eq!(input_call(&mut input, "getcontrol", &[XVal::Num(27.0), XVal::Num(2.0)]).unwrap(), XVal::Num(-1.0));
    assert_eq!(input_call(&mut input, "getcontrol", &[XVal::Num(4.0), XVal::Num(1.0)]).unwrap(), XVal::Num(256.0));
}

#[test]
fn iscontrol_returns_bitmask_three_when_key_is_both_bound_and_custom() {
    let (data, forms, refs) = minimal_fixture("W6D13IsCtrl.esm");
    let mut g = new_game(&data, &forms, &refs);

    let isc = |g: &mut Game<'_>, k: f64| crate::nvse_actor::call(g, "iscontrol", None, &[XVal::Num(k)]).unwrap().unwrap();
    // Unbound key 88: 0 -> SetIsControl 1 -> 2 -> SetIsControl 0 -> 0.
    assert_eq!(isc(&mut g, 88.0), XVal::Num(0.0));
    g.other.custom_controls.insert(88);
    assert_eq!(isc(&mut g, 88.0), XVal::Num(2.0));
    g.other.custom_controls.remove(&88);
    assert_eq!(isc(&mut g, 88.0), XVal::Num(0.0));
    // Bound key 17 (W): 1 -> SetIsControl 1 -> 3 -> SetIsControl 0 -> 1.
    assert_eq!(isc(&mut g, 17.0), XVal::Num(1.0));
    g.other.custom_controls.insert(17);
    assert_eq!(isc(&mut g, 17.0), XVal::Num(3.0));
    g.other.custom_controls.remove(&17);
    assert_eq!(isc(&mut g, 17.0), XVal::Num(1.0));
    assert_eq!(isc(&mut g, -1.0), XVal::Num(0.0));
}

#[test]
fn iscontroldisabled_reflects_disabled_bound_key_and_rejects_oob() {
    let mut input = ScriptInput::default();
    assert_eq!(input_call(&mut input, "iscontroldisabled", &[XVal::Num(0.0)]).unwrap(), XVal::Num(0.0));
    input_call(&mut input, "disablekey", &[XVal::Num(17.0)]).unwrap();
    assert_eq!(input_call(&mut input, "iskeydisabled", &[XVal::Num(17.0)]).unwrap(), XVal::Num(1.0));
    assert_eq!(input_call(&mut input, "iscontroldisabled", &[XVal::Num(0.0)]).unwrap(), XVal::Num(1.0));
    input_call(&mut input, "enablekey", &[XVal::Num(17.0)]).unwrap();
    assert_eq!(input_call(&mut input, "iscontroldisabled", &[XVal::Num(0.0)]).unwrap(), XVal::Num(0.0));
    assert_eq!(input_call(&mut input, "iscontroldisabled", &[XVal::Num(-1.0)]).unwrap(), XVal::Num(0.0));
    assert_eq!(input_call(&mut input, "iscontroldisabled", &[XVal::Num(28.0)]).unwrap(), XVal::Num(0.0));
}

#[test]
fn setalwaysrun_updates_player_isrunning_only_when_second_arg_nonzero() {
    let (data, forms, refs) = minimal_fixture("W6D13AlwaysRun.esm");
    let mut g = new_game(&data, &forms, &refs);
    g.gaps.always_run = true;
    let mut m = g.motion(FormId::PLAYER_REF);
    m.run_mode = true;
    g.set_motion(FormId::PLAYER_REF, m);

    assert_eq!(qn_expr(&mut g, "Player.IsRunning"), 1.0);
    assert_eq!(qn_expr(&mut g, "SetAlwaysRun 0 0"), 1.0);
    assert_eq!(qn_expr(&mut g, "GetAlwaysRun"), 0.0);
    assert_eq!(qn_expr(&mut g, "Player.IsRunning"), 1.0, "updateRunningFlag=0 leaves IsRunning unchanged");
    assert_eq!(qn_expr(&mut g, "SetAlwaysRun 0 1"), 1.0);
    assert_eq!(qn_expr(&mut g, "Player.IsRunning"), 0.0, "updateRunningFlag=1 updates IsRunning immediately");
    assert_eq!(qn_expr(&mut g, "SetAlwaysRun 1 1"), 1.0);
    assert_eq!(qn_expr(&mut g, "Player.IsRunning"), 1.0);
}

#[test]
fn setautomove_updates_getautomove_in_the_same_frame() {
    let (data, forms, refs) = minimal_fixture("W6D13AutoMove.esm");
    let mut g = new_game(&data, &forms, &refs);
    assert_eq!(qn_expr(&mut g, "GetAutoMove"), 0.0);
    assert_eq!(qn_expr(&mut g, "SetAutoMove 1"), 1.0);
    assert_eq!(qn_expr(&mut g, "GetAutoMove"), 1.0);
    assert_eq!(qn_expr(&mut g, "SetAutoMove 0"), 1.0);
    assert_eq!(qn_expr(&mut g, "GetAutoMove"), 0.0);
}

#[test]
fn setidlelooptimes_wraps_negative_to_u8_and_clamps_high_and_inverted() {
    let bytes = [
        tes4(),
        top(b"IDLE", &record(b"IDLE", 0, 0xB01, &[
            sub(b"EDID", z("W6D13Idle")),
            sub(b"DATA", vec![0, 1, 2, 0, 0, 0]),
        ])),
    ].concat();
    let (data, forms, refs) = build_fixture(&bytes, "W6D13Idle.esm");
    let mut g = new_game(&data, &forms, &refs);
    qn_expr(&mut g, "SetIdleLoopTimes W6D13Idle -1 -1");
    assert_eq!((qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 0"), qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 1")), (255.0, 255.0));
    qn_expr(&mut g, "SetIdleLoopTimes W6D13Idle 5 2");
    assert_eq!((qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 0"), qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 1")), (5.0, 5.0));
    qn_expr(&mut g, "SetIdleLoopTimes W6D13Idle 300 2");
    assert_eq!((qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 0"), qn_expr(&mut g, "GetIdleLoopTimes W6D13Idle 1")), (255.0, 255.0));
}

#[test]
fn getbipedmodelpath_code_4_reads_male_icon_path() {
    let bytes = [
        tes4(),
        top(b"ARMO", &record(b"ARMO", 0, 0xD01, &[
            sub(b"EDID", z("W6D13Arm")),
            sub(b"MODL", z(r"Armor\Male.nif")),
            sub(b"MOD3", z(r"Armor\Female.nif")),
            sub(b"MOD2", z(r"Armor\MaleGnd.nif")),
            sub(b"MOD4", z(r"Armor\FemaleGnd.nif")),
            sub(b"ICON", z(r"Interface\Icons\MaleIcon.dds")),
        ])),
    ].concat();
    let (data, forms, refs) = build_fixture(&bytes, "W6D13BipMod.esm");
    let mut g = new_game(&data, &forms, &refs);
    let got = |g: &mut Game<'_>, c: f64| crate::nvse::call(g, &Default::default(), "getbipedmodelpath", None, &[XVal::Num(c), XVal::Ref(0xD01)]).unwrap().unwrap();
    assert_eq!(got(&mut g, 0.0), XVal::Str(r"Armor\Male.nif".into()));
    assert_eq!(got(&mut g, 4.0), XVal::Str(r"Interface\Icons\MaleIcon.dds".into()));
    assert_eq!(got(&mut g, 5.0), XVal::Str(String::new()));
}

#[test]
fn togglebipedslotvisibility_rejects_out_of_range_slots() {
    let (data, forms, refs) = minimal_fixture("W6D13BipVis.esm");
    let mut g = new_game(&data, &forms, &refs);
    let call = |g: &mut Game<'_>, a: &[XVal]| crate::nvse_gaps::call(g, &Default::default(), "togglebipedslotvisibility", None, a).unwrap().unwrap();
    assert_eq!(call(&mut g, &[XVal::Num(2.0)]), XVal::Num(1.0));
    assert_eq!(call(&mut g, &[XVal::Num(2.0), XVal::Num(0.0)]), XVal::Num(0.0));
    assert_eq!(call(&mut g, &[XVal::Num(2.0), XVal::Num(1.0)]), XVal::Num(1.0));
    assert_eq!(call(&mut g, &[XVal::Num(-1.0)]), XVal::Num(0.0));
    assert_eq!(call(&mut g, &[XVal::Num(25.0)]), XVal::Num(0.0));
    assert_eq!(call(&mut g, &[XVal::Num(-1.0), XVal::Num(0.0)]), XVal::Num(0.0));
    assert_eq!(call(&mut g, &[XVal::Num(0.0)]), XVal::Num(1.0), "negative slot did not mutate slot 0");
}
