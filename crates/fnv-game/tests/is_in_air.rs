//! Independently authored tests for live `Motion::in_air` wiring across JIP LN
//! `IsInAir`, ShowOff `IsJumping`, and ShowOff `GetPCCanFastTravel`.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    functions::senses::Motion,
    game::{Game, GameConfig, RunCtx},
    trace::EventKind,
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const NPC_BASE: u32 = 0x900;
const NPC_REF: u32 = 0x901;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let leading = [
        tes4(),
        top(
            b"NPC_",
            &[
                record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
                record(b"NPC_", 0, NPC_BASE, &[sub(b"EDID", z("AirTestNpcBase"))]),
            ]
            .concat(),
        ),
        top(
            b"CELL",
            &[
                record(b"CELL", 0, 0x950, &[sub(b"EDID", z("AirTestCell"))]),
                group(
                    0x950u32.to_le_bytes(),
                    6,
                    &group(
                        0x950u32.to_le_bytes(),
                        8,
                        &record(
                            b"ACHR",
                            0,
                            NPC_REF,
                            &[sub(b"EDID", z("AirTestNpc")), sub(b"NAME", NPC_BASE.to_le_bytes().to_vec())],
                        ),
                    ),
                ),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = [Plugin::parse(&leading, "OwnedLeading.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}

fn eval(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

fn eval_on(g: &mut Game, self_ref: FormId, body: &str) -> XVal {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd"),
        RunCtx { self_ref: Some(self_ref), ..RunCtx::default() },
    )
    .unwrap()
}

fn try_eval(g: &mut Game, self_ref: Option<FormId>, body: &str) -> Result<XVal, String> {
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd"),
        RunCtx { self_ref, ..RunCtx::default() },
    )
}

#[test]
fn is_in_air_is_jumping_and_fast_travel_track_live_motion_state() {
    let f = fixture();
    let mut g = game(&f);
    let npc = FormId(NPC_REF);

    // Defaults to 0 for both player and NPC when grounded / not yet measured.
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval_on(&mut g, FormId::PLAYER_REF, "SetFunctionValue (IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval_on(&mut g, npc, "SetFunctionValue (IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsJumping)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsJumping)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetPCCanFastTravel)"), XVal::Num(1.0));

    // Unrelated Motion flags (moving, running, sneaking, turning, swimming) do not trigger IsInAir.
    g.set_motion(
        FormId::PLAYER_REF,
        Motion { moving: true, running: true, sneaking: true, turning: 2, run_mode: true, swimming: true, in_air: false },
    );
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsJumping)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetPCCanFastTravel)"), XVal::Num(1.0));

    // Player airborne: player.IsInAir, implicit-self IsInAir on player, and player.IsJumping read 1;
    // GetPCCanFastTravel blocks; NPC remains 0.
    g.set_motion(FormId::PLAYER_REF, Motion { in_air: true, ..Motion::default() });
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsInAir)"), XVal::Num(1.0));
    assert_eq!(eval_on(&mut g, FormId::PLAYER_REF, "SetFunctionValue (IsInAir)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsJumping)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetPCCanFastTravel)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsJumping)"), XVal::Num(0.0));

    // Player lands while NPC is airborne: each actor keeps independent state and player fast travel works.
    g.set_motion(FormId::PLAYER_REF, Motion::default());
    g.set_motion(npc, Motion { in_air: true, ..Motion::default() });
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsInAir)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (player.IsJumping)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (GetPCCanFastTravel)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsInAir)"), XVal::Num(1.0));
    assert_eq!(eval_on(&mut g, npc, "SetFunctionValue (IsInAir)"), XVal::Num(1.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsJumping)"), XVal::Num(1.0));

    // Forgetting NPC motion returns it to 0.
    g.forget_motion(npc);
    assert_eq!(eval(&mut g, "SetFunctionValue (AirTestNpc.IsInAir)"), XVal::Num(0.0));

    // Clean execution on valid calls: no CallFailed diagnostics emitted.
    assert!(
        !g.events.iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })),
        "IsInAir / IsJumping / GetPCCanFastTravel must not emit CallFailed"
    );

    // Bare IsInAir without self_ref defaults to player (matching IsInWater), while an explicit
    // empty receiver (even when player is airborne and self_ref is player) is refused by the VM.
    g.set_motion(FormId::PLAYER_REF, Motion { in_air: true, ..Motion::default() });
    assert_eq!(try_eval(&mut g, None, "SetFunctionValue (IsInAir)"), Ok(XVal::Num(1.0)));
    assert_eq!(
        try_eval(&mut g, Some(FormId::PLAYER_REF), "ref emptyRef\nSetFunctionValue (emptyRef.IsInAir)"),
        Err("isinair called on no reference".into())
    );

    // Independent Game instances keep no leaked state.
    let mut fresh = game(&f);
    assert_eq!(eval(&mut fresh, "SetFunctionValue (player.IsInAir)"), XVal::Num(0.0));
}
