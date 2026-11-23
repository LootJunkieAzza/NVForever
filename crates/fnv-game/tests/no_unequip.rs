//! Authored tests for JIP LN `GetNoUnequip` paired with `SetNoUnequip`, `EquipItem`
//! `NoUnequipFlag`, `UnequipItem`, and `World::equip_lock` persistence.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig, GameSnapshot, RunCtx},
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{world::SimError, FormId, FormStore, RefTable, SimConfig, World};

const CELL_ID: FormId = FormId(0x200);
const NPC_BASE: FormId = FormId(0x800);
const PISTOL: FormId = FormId(0x801);
const RIFLE: FormId = FormId(0x802);
const NPC_REF: FormId = FormId(0x300);

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let player = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let npc_base = record(b"NPC_", 0, NPC_BASE.0, &[sub(b"EDID", z("TestCompanionBase"))]);
    let weap = |id: FormId, edid: &str| {
        let data = [&0i32.to_le_bytes()[..], &100i32.to_le_bytes(), &2.0f32.to_le_bytes(), &15i16.to_le_bytes(), &[1u8]].concat();
        record(b"WEAP", 0, id.0, &[sub(b"EDID", z(edid)), sub(b"DATA", data), sub(b"DNAM", vec![0; 204])])
    };
    let placed_npc = record(
        b"ACHR",
        0,
        NPC_REF.0,
        &[sub(b"EDID", z("TestCompanionRef")), sub(b"NAME", NPC_BASE.0.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])],
    );
    let cell = record(b"CELL", 0, CELL_ID.0, &[sub(b"EDID", z("NoUnequipCell"))]);
    let children = group(CELL_ID.0.to_le_bytes(), 6, &group(CELL_ID.0.to_le_bytes(), 9, &placed_npc));
    let bytes = [
        tes4(),
        top(b"NPC_", &[player, npc_base].concat()),
        top(b"WEAP", &[weap(PISTOL, "TestPistol"), weap(RIFLE, "TestRifle")].concat()),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "NoUnequipTest.esm".into()).unwrap()];
    let order = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&order, &schema);
    let refs = RefTable::build(&order, &schema);
    let data = GameData::build(&order, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn eval(g: &mut Game<'_>, body: &str) -> XVal {
    let before = g.events.len();
    let v = g
        .run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default())
        .unwrap_or_else(|e| panic!("{body}: {e}"));
    assert!(
        !g.events[before..].iter().any(|e| matches!(
            e.kind,
            EventKind::Stub { .. }
                | EventKind::Unhandled { .. }
                | EventKind::CallFailed { .. }
                | EventKind::ScriptError { .. }
        )),
        "{body}: {:?}",
        &g.events[before..]
    );
    v
}

#[test]
fn get_no_unequip_tracks_set_no_unequip_equip_flag_unequip_and_saves() {
    let f = fixture();
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });

    eval(&mut g, "player.AddItem TestPistol 1\nplayer.AddItem TestRifle 1\nTestCompanionRef.AddItem TestPistol 1");

    // Non-inventory refs and omitted receiver return 0.
    assert_eq!(eval(&mut g, "SetFunctionValue (GetNoUnequip)"), XVal::Num(0.0));
    assert_eq!(eval(&mut g, "SetFunctionValue (player.GetNoUnequip)"), XVal::Num(0.0));

    // Unequipped inventory ref reads 0, and SetNoUnequip 1 on an unequipped item does not lock it.
    let inv_check = "array_var refs\nref r\nlet refs := player.GetInvRefsForItem TestPistol\nlet r := refs[0]\nSetFunctionValue (r.GetNoUnequip)";
    assert_eq!(eval(&mut g, inv_check), XVal::Num(0.0));
    eval(&mut g, "array_var refs\nref r\nlet refs := player.GetInvRefsForItem TestPistol\nlet r := refs[0]\nr.SetNoUnequip 1");
    assert_eq!(eval(&mut g, inv_check), XVal::Num(0.0));

    // Equipped without NoUnequipFlag reads 0 (matches data2 observation).
    eval(&mut g, "player.EquipItem TestPistol 0");
    assert_eq!(eval(&mut g, inv_check), XVal::Num(0.0));

    // SetNoUnequip 1 on the equipped inventory ref makes GetNoUnequip read 1 and blocks unequip.
    eval(&mut g, "ref r\nlet r := player.GetEquippedItemRefForItem TestPistol\nr.SetNoUnequip 1");
    assert_eq!(eval(&mut g, "ref r\nlet r := player.GetEquippedItemRefForItem TestPistol\nSetFunctionValue (r.GetNoUnequip)"), XVal::Num(1.0));
    assert_eq!(g.world.unequip(FormId::PLAYER_REF, PISTOL), Err(SimError::EquipLocked { item: PISTOL, on: true }));

    // Save/restore preserves the NoUnequip lock and GetNoUnequip readback.
    let snap: GameSnapshot = serde_json::from_str(&serde_json::to_string(&g.checked_snapshot().unwrap()).unwrap()).unwrap();
    let world = World::restore(&f.forms, &f.refs, &snap.sim).unwrap();
    let mut restored = Game::restore(&f.data, world, g.cfg, &snap).unwrap();
    assert_eq!(eval(&mut restored, "ref r\nlet r := player.GetEquippedItemRefForItem TestPistol\nSetFunctionValue (r.GetNoUnequip)"), XVal::Num(1.0));

    // SetNoUnequip 0 clears the lock: GetNoUnequip reads 0 and unequip now succeeds.
    eval(&mut restored, "ref r\nlet r := player.GetEquippedItemRefForItem TestPistol\nr.SetNoUnequip 0");
    assert_eq!(eval(&mut restored, "ref r\nlet r := player.GetEquippedItemRefForItem TestPistol\nSetFunctionValue (r.GetNoUnequip)"), XVal::Num(0.0));
    assert!(restored.world.unequip(FormId::PLAYER_REF, PISTOL).unwrap());

    // EquipItem with NoUnequipFlag = 1 sets NoUnequip directly; UnequipItem (with NoEquip = 1) clears it.
    eval(&mut restored, "player.EquipItem TestRifle 1");
    assert_eq!(eval(&mut restored, "ref r\nlet r := player.GetEquippedItemRefForItem TestRifle\nSetFunctionValue (r.GetNoUnequip)"), XVal::Num(1.0));
    eval(&mut restored, "player.UnequipItem TestRifle 1");
    assert_eq!(
        eval(&mut restored, "array_var refs\nref r\nlet refs := player.GetInvRefsForItem TestRifle\nlet r := refs[0]\nSetFunctionValue (r.GetNoUnequip)"),
        XVal::Num(0.0),
        "UnequipItem NoEquip lock must not read as NoUnequip"
    );

    // Unoutfitted NPC carrying an unequipped weapon (IsEquipped == 0, before EquipItem):
    // SetNoUnequip 1 must NOT lock the unequipped pistol and GetNoUnequip must stay 0.
    let npc_inv_check = "array_var refs\nref r\nlet refs := TestCompanionRef.GetInvRefsForItem TestPistol\nlet r := refs[0]\nSetFunctionValue (r.GetNoUnequip)";
    assert_eq!(
        eval(&mut restored, "array_var refs\nref r\nlet refs := TestCompanionRef.GetInvRefsForItem TestPistol\nlet r := refs[0]\nSetFunctionValue (r.IsEquipped)"),
        XVal::Num(0.0)
    );
    eval(&mut restored, "array_var refs\nref r\nlet refs := TestCompanionRef.GetInvRefsForItem TestPistol\nlet r := refs[0]\nr.SetNoUnequip 1");
    assert_eq!(restored.world.equip_lock(NPC_REF, PISTOL), None, "unequipped item on unoutfitted NPC must not acquire NoUnequip lock");
    assert_eq!(eval(&mut restored, npc_inv_check), XVal::Num(0.0));

    // NPC actors (even before cell outfitting) keep independent NoUnequip state once equipped.
    eval(&mut restored, "TestCompanionRef.EquipItem TestPistol 1");
    assert_eq!(
        eval(&mut restored, "ref r\nlet r := TestCompanionRef.GetEquippedItemRefForItem TestPistol\nSetFunctionValue (r.GetNoUnequip)"),
        XVal::Num(1.0)
    );
    eval(&mut restored, "ref r\nlet r := TestCompanionRef.GetEquippedItemRefForItem TestPistol\nr.SetNoUnequip 0");
    assert_eq!(
        eval(&mut restored, "ref r\nlet r := TestCompanionRef.GetEquippedItemRefForItem TestPistol\nSetFunctionValue (r.GetNoUnequip)"),
        XVal::Num(0.0)
    );
    assert_eq!(eval(&mut restored, inv_check), XVal::Num(0.0));
}
