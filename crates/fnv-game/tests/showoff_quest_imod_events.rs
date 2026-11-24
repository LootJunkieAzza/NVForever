//! Integration tests for ShowOff objective & weapon-unholster events
//! (`ShowOff:OnDisplayObjective`, `ShowOff:OnCompleteObjective`,
//! `ShowOff:OnWeaponUnholster`) and built-in xNVSE image-space modifier events
//! (`OnApplyIMOD`, `OnRemoveIMOD`).

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    game::{Game, GameConfig},
    nvse_hud::WEAPON_FRAMES,
    trace::EventKind,
    GameData,
};
use fnv_script::{vm::XVal, FunctionTable};
use fnv_sim::{FormId, FormStore, RefTable, World};

const FISTS: u32 = 0x1F4;
const NPC_GUARD: u32 = 0x810;
const CREA_GHOUL: u32 = 0x811;
const WEAP_PISTOL: u32 = 0x820;
const WEAP_RIFLE: u32 = 0x821;
const QUEST_A: u32 = 0x830;
const QUEST_B: u32 = 0x831;
const IMAD_FADE: u32 = 0x840;
const IMAD_HOLD: u32 = 0x841;
const FLST_IMADS: u32 = 0x850;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn quest_record(id: u32, edid: &str, objectives: &[i32]) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"DATA", [vec![0u8, 0, 0, 0], 5.0f32.to_le_bytes().to_vec()].concat()),
    ];
    for &idx in objectives {
        subs.push(sub(b"QOBJ", idx.to_le_bytes().to_vec()));
        subs.push(sub(b"NNAM", z(&format!("Objective {idx}"))));
    }
    record(b"QUST", 0, id, &subs)
}

fn imad_record(id: u32, edid: &str, animatable: u32, duration: f32) -> Vec<u8> {
    let dnam = [u32s(&[animatable]), duration.to_le_bytes().to_vec(), vec![0; 236]].concat();
    record(b"IMAD", 0, id, &[sub(b"EDID", z(edid)), sub(b"DNAM", dnam)])
}

fn fixture(with_fists: bool) -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(
        b"NPC_",
        &[
            record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
            record(b"NPC_", 0, NPC_GUARD, &[sub(b"EDID", z("OwnedGuardNpc"))]),
        ]
        .concat(),
    ));
    let crea_data = [vec![4u8, 0, 0, 0], vec![0; 6], vec![0; 7]].concat();
    bytes.extend(top(
        b"CREA",
        &record(b"CREA", 0, CREA_GHOUL, &[sub(b"EDID", z("OwnedGhoulCrea")), sub(b"DATA", crea_data)]),
    ));
    let mut weaps = vec![
        record(b"WEAP", 0, WEAP_PISTOL, &[sub(b"EDID", z("OwnedPistol")), sub(b"DATA", vec![0; 15])]),
        record(b"WEAP", 0, WEAP_RIFLE, &[sub(b"EDID", z("OwnedRifle")), sub(b"DATA", vec![0; 15])]),
    ];
    if with_fists {
        weaps.push(record(b"WEAP", 0, FISTS, &[sub(b"EDID", z("Fists")), sub(b"DATA", vec![0; 15])]));
    }
    bytes.extend(top(b"WEAP", &weaps.concat()));
    bytes.extend(top(
        b"QUST",
        &[quest_record(QUEST_A, "OwnedQuestA", &[10, 20, 30]), quest_record(QUEST_B, "OwnedQuestB", &[10, 20])].concat(),
    ));
    bytes.extend(top(
        b"IMAD",
        &[imad_record(IMAD_FADE, "OwnedFadeImad", 1, 0.5), imad_record(IMAD_HOLD, "OwnedHoldImad", 0, 1.0)].concat(),
    ));
    bytes.extend(top(
        b"FLST",
        &record(
            b"FLST",
            0,
            FLST_IMADS,
            &[sub(b"EDID", z("OwnedImadList")), sub(b"LNAM", IMAD_FADE.to_le_bytes().to_vec())],
        ),
    ));
    let plugins = vec![Plugin::parse(&bytes, "OwnedShowOffEvents.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, Default::default()),
        GameConfig { strict: true, ..Default::default() },
    )
}

fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("ref ownRef\narray_var aTmp\nBegin Function {{}}\n{body}\nEnd"), Default::default()).unwrap()
}

fn num(g: &mut Game, key: &str) -> f64 {
    let XVal::Num(n) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("expected number for {key}")
    };
    n
}

fn form(g: &mut Game, key: &str) -> FormId {
    let XVal::Ref(id) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("expected ref for {key}")
    };
    FormId(id)
}

fn text(g: &mut Game, key: &str) -> String {
    let XVal::Str(s) = query(g, &format!("SetFunctionValue (GetModLocalData \"{key}\")")) else {
        panic!("expected string for {key}")
    };
    s
}

fn no_errors(g: &Game) {
    assert!(
        !g.events.iter().any(|e| matches!(e.kind, EventKind::ScriptError { .. })),
        "{:?}",
        g.events
    );
}

#[test]
fn objective_events_dispatch_on_display_and_complete_with_filters_and_controls() {
    let f = fixture(true);
    // Vanilla-unchanged control: no handlers registered.
    {
        let mut g = game(&f);
        query(&mut g, "SetObjectiveDisplayed OwnedQuestA 10 1\nSetObjectiveCompleted OwnedQuestA 10 1\nCompleteAllObjectives OwnedQuestA");
        assert_eq!(query(&mut g, "SetFunctionValue (ar_Size (GetEventHandlers \"\"))"), XVal::Num(0.0));
        assert_eq!(query(&mut g, "SetFunctionValue (GetObjectiveDisplayed OwnedQuestA 10)"), XVal::Num(1.0));
        assert_eq!(query(&mut g, "SetFunctionValue (GetObjectiveCompleted OwnedQuestA 30)"), XVal::Num(1.0));
        no_errors(&g);
    }

    let mut g = game(&f);
    query(
        &mut g,
        r#"SetEventHandler "ShowOff:OnDisplayObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "disp.calls" ((GetModLocalData "disp.calls") + 1)
SetModLocalData "disp.quest" rQuest
SetModLocalData "disp.obj" iObj
SetModLocalData "disp.getter" (GetObjectiveDisplayed rQuest iObj)
SetModLocalData "disp.ev" (GetCurrentEventName)
End)
SetEventHandler "ShowOff:OnDisplayObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "disp.a10" ((GetModLocalData "disp.a10") + 1)
End) "first"::OwnedQuestA "second"::10
SetEventHandlerAlt "ShowOff:OnDisplayObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "disp.a20" ((GetModLocalData "disp.a20") + 1)
End) 1::OwnedQuestA 2::20
SetEventHandler "ShowOff:OnCompleteObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "comp.calls" ((GetModLocalData "comp.calls") + 1)
SetModLocalData "comp.quest" rQuest
SetModLocalData "comp.obj" iObj
SetModLocalData "comp.sum" ((GetModLocalData "comp.sum") + iObj)
SetModLocalData "comp.getter" (GetObjectiveCompleted rQuest iObj)
SetModLocalData "comp.ev" (GetCurrentEventName)
End)
SetEventHandler "ShowOff:OnCompleteObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "comp.a10" ((GetModLocalData "comp.a10") + 1)
End) "first"::OwnedQuestA "second"::10
SetEventHandlerAlt "ShowOff:OnCompleteObjective" (Begin Function {ref rQuest, int iObj}
SetModLocalData "comp.a20" ((GetModLocalData "comp.a20") + 1)
End) 1::OwnedQuestA 2::20"#,
    );

    // Display objective 10: fires OnDisplayObjective once, updates getter first, matches first/second filter.
    query(&mut g, "SetObjectiveDisplayed OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "disp.calls"), 1.0);
    assert_eq!(form(&mut g, "disp.quest"), FormId(QUEST_A));
    assert_eq!(num(&mut g, "disp.obj"), 10.0);
    assert_eq!(num(&mut g, "disp.getter"), 1.0);
    assert_eq!(text(&mut g, "disp.ev"), "ShowOff:OnDisplayObjective");
    assert_eq!(num(&mut g, "disp.a10"), 1.0);
    assert_eq!(num(&mut g, "disp.a20"), 0.0);
    assert_eq!(num(&mut g, "comp.calls"), 0.0);

    // Repeating display on an already-displayed objective must not fire again.
    query(&mut g, "SetObjectiveDisplayed OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "disp.calls"), 1.0);

    // Hiding an objective must not fire OnDisplayObjective.
    query(&mut g, "SetObjectiveDisplayed OwnedQuestA 10 0");
    assert_eq!(num(&mut g, "disp.calls"), 1.0);

    // Completing objective 10 fires OnCompleteObjective once, updates getter first, matches filters.
    query(&mut g, "SetObjectiveCompleted OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "comp.calls"), 1.0);
    assert_eq!(form(&mut g, "comp.quest"), FormId(QUEST_A));
    assert_eq!(num(&mut g, "comp.obj"), 10.0);
    assert_eq!(num(&mut g, "comp.getter"), 1.0);
    assert_eq!(text(&mut g, "comp.ev"), "ShowOff:OnCompleteObjective");
    assert_eq!(num(&mut g, "comp.a10"), 1.0);
    assert_eq!(num(&mut g, "comp.a20"), 0.0);

    // Repeating completion or un-completing must not fire OnCompleteObjective.
    query(&mut g, "SetObjectiveCompleted OwnedQuestA 10 1\nSetObjectiveCompleted OwnedQuestA 10 0");
    assert_eq!(num(&mut g, "comp.calls"), 1.0);

    // Re-displaying a hidden, completed objective clears completed and fires OnDisplayObjective only.
    query(&mut g, "SetObjectiveCompleted OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "comp.calls"), 2.0);
    query(&mut g, "SetObjectiveDisplayed OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "disp.calls"), 2.0);
    assert_eq!(num(&mut g, "comp.calls"), 2.0);
    assert_eq!(query(&mut g, "SetFunctionValue (GetObjectiveCompleted OwnedQuestA 10)"), XVal::Num(0.0));

    // Undefined objective index must not fire either event.
    query(&mut g, "SetObjectiveDisplayed OwnedQuestA 99 1\nSetObjectiveCompleted OwnedQuestA 99 1");
    assert_eq!(num(&mut g, "disp.calls"), 2.0);
    assert_eq!(num(&mut g, "comp.calls"), 2.0);

    // Completed quest refuses SetObjectiveDisplayed/SetObjectiveCompleted (#2958): neither event fires.
    query(&mut g, "CompleteQuest OwnedQuestB\nSetObjectiveDisplayed OwnedQuestB 10 1\nSetObjectiveCompleted OwnedQuestB 10 1");
    assert_eq!(num(&mut g, "disp.calls"), 2.0);
    assert_eq!(num(&mut g, "comp.calls"), 2.0);

    // CompleteAllObjectives fires OnCompleteObjective only for newly completed objectives (10, 20, 30).
    // Mark 10 completed first so CompleteAllObjectives completes only 20 and 30 in ascending order.
    query(&mut g, "SetObjectiveCompleted OwnedQuestA 10 1");
    assert_eq!(num(&mut g, "comp.calls"), 3.0);
    query(&mut g, "SetModLocalData \"comp.sum\" 0\nCompleteAllObjectives OwnedQuestA");
    assert_eq!(num(&mut g, "comp.calls"), 5.0);
    assert_eq!(num(&mut g, "comp.sum"), 50.0, "objectives 20 and 30 completed once each");
    assert_eq!(num(&mut g, "comp.obj"), 30.0, "deterministic ascending BTreeMap order");
    assert_eq!(num(&mut g, "comp.a20"), 1.0);
    no_errors(&g);
}

#[test]
fn imod_events_dispatch_on_apply_remove_and_expiry_with_filters_and_controls() {
    let f = fixture(true);
    // Vanilla-unchanged control + built-in KNOWN registration check.
    {
        let mut g = game(&f);
        assert_eq!(query(&mut g, "SetFunctionValue (ar_Size (GetEventHandlers \"OnApplyIMOD\"))"), XVal::Num(0.0));
        assert_eq!(query(&mut g, "SetFunctionValue (ar_Size (GetEventHandlers \"OnRemoveIMOD\"))"), XVal::Num(0.0));
        query(&mut g, "ApplyImageSpaceModifier OwnedHoldImad\nRemoveImageSpaceModifier OwnedHoldImad");
        assert!(g.image_modifiers.is_empty());
        no_errors(&g);
    }

    let mut g = game(&f);
    query(
        &mut g,
        r#"SetEventHandler "OnApplyIMOD" (Begin Function {ref rImad}
SetModLocalData "imod.apply.calls" ((GetModLocalData "imod.apply.calls") + 1)
SetModLocalData "imod.apply.last" rImad
SetModLocalData "imod.apply.active" (IsImageSpaceActive rImad)
SetModLocalData "imod.apply.ev" (GetCurrentEventName)
End)
SetEventHandler "OnApplyIMOD" (Begin Function {ref rImad}
SetModLocalData "imod.apply.fade_first" ((GetModLocalData "imod.apply.fade_first") + 1)
End) "first"::OwnedFadeImad
SetEventHandlerAlt "OnApplyIMOD" (Begin Function {ref rImad}
SetModLocalData "imod.apply.list" ((GetModLocalData "imod.apply.list") + 1)
End) 1::OwnedImadList
SetEventHandler "OnRemoveIMOD" (Begin Function {ref rImad}
SetModLocalData "imod.remove.calls" ((GetModLocalData "imod.remove.calls") + 1)
SetModLocalData "imod.remove.last" rImad
SetModLocalData "imod.remove.active" (IsImageSpaceActive rImad)
SetModLocalData "imod.remove.ev" (GetCurrentEventName)
End)
SetEventHandlerAlt "OnRemoveIMOD" (Begin Function {ref rImad}
SetModLocalData "imod.remove.hold" ((GetModLocalData "imod.remove.hold") + 1)
End) 1::OwnedHoldImad"#,
    );

    // Apply non-animatable HoldImad: fires OnApplyIMOD with IsImageSpaceActive == 1; Fade/List filters do not match.
    query(&mut g, "ApplyImageSpaceModifier OwnedHoldImad");
    assert_eq!(num(&mut g, "imod.apply.calls"), 1.0);
    assert_eq!(form(&mut g, "imod.apply.last"), FormId(IMAD_HOLD));
    assert_eq!(num(&mut g, "imod.apply.active"), 1.0);
    assert_eq!(text(&mut g, "imod.apply.ev"), "OnApplyIMOD");
    assert_eq!(num(&mut g, "imod.apply.fade_first"), 0.0);
    assert_eq!(num(&mut g, "imod.apply.list"), 0.0);

    // Re-applying an already active IMOD restarts it and fires OnApplyIMOD again.
    query(&mut g, "ApplyImageSpaceModifier OwnedHoldImad");
    assert_eq!(num(&mut g, "imod.apply.calls"), 2.0);

    // Snapshot restore with an active IMOD must not spuriously fire OnApplyIMOD.
    let snap = g.snapshot();
    let restored = Game::restore(&f.data, World::new(&f.forms, &f.refs, Default::default()), GameConfig { strict: true, ..Default::default() }, &snap).unwrap();
    assert!(restored.image_modifier_active(FormId(IMAD_HOLD)));
    assert!(restored.events.is_empty());

    // Remove HoldImad: fires OnRemoveIMOD once with IsImageSpaceActive == 0.
    query(&mut g, "RemoveImageSpaceModifier OwnedHoldImad");
    assert_eq!(num(&mut g, "imod.remove.calls"), 1.0);
    assert_eq!(form(&mut g, "imod.remove.last"), FormId(IMAD_HOLD));
    assert_eq!(num(&mut g, "imod.remove.active"), 0.0);
    assert_eq!(text(&mut g, "imod.remove.ev"), "OnRemoveIMOD");
    assert_eq!(num(&mut g, "imod.remove.hold"), 1.0);

    // Removing an inactive IMOD must not fire OnRemoveIMOD.
    query(&mut g, "RemoveImageSpaceModifier OwnedHoldImad\nRemoveImageSpaceModifier OwnedFadeImad");
    assert_eq!(num(&mut g, "imod.remove.calls"), 1.0);

    // Apply animatable FadeImad (0.5s): matches "first"::OwnedFadeImad and 1::OwnedImadList.
    query(&mut g, "ApplyImageSpaceModifier OwnedFadeImad");
    assert_eq!(num(&mut g, "imod.apply.calls"), 3.0);
    assert_eq!(num(&mut g, "imod.apply.fade_first"), 1.0);
    assert_eq!(num(&mut g, "imod.apply.list"), 1.0);
    assert_eq!(num(&mut g, "imod.remove.calls"), 1.0);

    // Halfway through duration: still active, OnRemoveIMOD has not fired.
    g.frames(15);
    assert_eq!(num(&mut g, "imod.remove.calls"), 1.0);

    // Past duration (0.5s = 30 frames at 60Hz): natural expiry removes FadeImad and dispatches OnRemoveIMOD.
    g.frames(16);
    assert_eq!(num(&mut g, "imod.remove.calls"), 2.0);
    assert_eq!(form(&mut g, "imod.remove.last"), FormId(IMAD_FADE));
    assert_eq!(num(&mut g, "imod.remove.active"), 0.0);
    assert_eq!(num(&mut g, "imod.remove.hold"), 1.0, "1::OwnedHoldImad filter must ignore FadeImad");
    no_errors(&g);

    // Invalid non-IMAD form fails ApplyImageSpaceModifier and must not fire OnApplyIMOD.
    let before = num(&mut g, "imod.apply.calls");
    let _ = g.run_query("Begin Function {}\nApplyImageSpaceModifier OwnedPistol\nEnd", Default::default());
    assert_eq!(num(&mut g, "imod.apply.calls"), before);
    // Keep `restored` live through the end.
    assert_eq!(restored.image_modifiers.len(), 1);
}

#[test]
fn weapon_unholster_event_dispatches_on_player_npc_and_scripted_draw_with_filters() {
    let f = fixture(true);
    // Vanilla-unchanged control: no handlers registered.
    {
        let mut g = game(&f);
        g.world.add_item(FormId::PLAYER_REF, FormId(WEAP_PISTOL), 1).unwrap();
        query(&mut g, "player.EquipItem OwnedPistol");
        g.set_weapon_out(FormId::PLAYER_REF, true);
        g.set_weapon_out(FormId::PLAYER_REF, false);
        assert_eq!(query(&mut g, "SetFunctionValue (ar_Size (GetEventHandlers \"\"))"), XVal::Num(0.0));
        no_errors(&g);
    }

    let mut g = game(&f);
    g.world.add_item(FormId::PLAYER_REF, FormId(WEAP_PISTOL), 1).unwrap();
    query(
        &mut g,
        r#"player.EquipItem OwnedPistol
SetModLocalData "npc.ref" (player.PlaceAtMe OwnedGuardNpc)
SetModLocalData "crea.ref" (player.PlaceAtMe OwnedGhoulCrea)
SetEventHandler "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "unh.calls" ((GetModLocalData "unh.calls") + 1)
SetModLocalData "unh.actor" GetSelfAlt
SetModLocalData "unh.weapon" rWeapon
SetModLocalData "unh.is_out" (GetSelfAlt.IsWeaponOut)
SetModLocalData "unh.ev" (GetCurrentEventName)
End)
SetEventHandler "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "unh.pistol" ((GetModLocalData "unh.pistol") + 1)
End) "first"::OwnedPistol
SetEventHandlerAlt "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "unh.player_only" ((GetModLocalData "unh.player_only") + 1)
End) 0::player
SetEventHandlerAlt "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "unh.guard_rifle" ((GetModLocalData "unh.guard_rifle") + 1)
End) 0::OwnedGuardNpc 1::OwnedRifle"#,
    );

    let npc = form(&mut g, "npc.ref");
    let crea = form(&mut g, "crea.ref");

    // Repeated holstered reports do not fire.
    g.set_weapon_out(FormId::PLAYER_REF, false);
    assert_eq!(num(&mut g, "unh.calls"), 0.0);

    // Player draws equipped pistol: fires once with actor=player, weapon=OwnedPistol, IsWeaponOut=1.
    g.set_weapon_out(FormId::PLAYER_REF, true);
    assert_eq!(num(&mut g, "unh.calls"), 1.0);
    assert_eq!(form(&mut g, "unh.actor"), FormId::PLAYER_REF);
    assert_eq!(form(&mut g, "unh.weapon"), FormId(WEAP_PISTOL));
    assert_eq!(num(&mut g, "unh.is_out"), 1.0);
    assert_eq!(text(&mut g, "unh.ev"), "ShowOff:OnWeaponUnholster");
    assert_eq!(num(&mut g, "unh.pistol"), 1.0);
    assert_eq!(num(&mut g, "unh.player_only"), 1.0);
    assert_eq!(num(&mut g, "unh.guard_rifle"), 0.0);

    // Repeated drawn frames must not spam the event.
    for _ in 0..5 {
        g.set_weapon_out(FormId::PLAYER_REF, true);
    }
    assert_eq!(num(&mut g, "unh.calls"), 1.0);

    // Holstering must not fire OnWeaponUnholster.
    g.set_weapon_out(FormId::PLAYER_REF, false);
    assert_eq!(num(&mut g, "unh.calls"), 1.0);

    // NPC with engine-reported held_weapon (not equipped in inventory): matches base filter 0::OwnedGuardNpc + 1::OwnedRifle.
    g.set_held_weapon(npc, Some(FormId(WEAP_RIFLE)));
    g.set_weapon_out(npc, true);
    assert_eq!(num(&mut g, "unh.calls"), 2.0);
    assert_eq!(form(&mut g, "unh.actor"), npc);
    assert_eq!(form(&mut g, "unh.weapon"), FormId(WEAP_RIFLE));
    assert_eq!(num(&mut g, "unh.pistol"), 1.0);
    assert_eq!(num(&mut g, "unh.player_only"), 1.0);
    assert_eq!(num(&mut g, "unh.guard_rifle"), 1.0);

    // Unarmed creature with Fists (0x1F4) in game data: passes Fists and actor=crea.
    g.set_weapon_out(crea, true);
    assert_eq!(num(&mut g, "unh.calls"), 3.0);
    assert_eq!(form(&mut g, "unh.actor"), crea);
    assert_eq!(form(&mut g, "unh.weapon"), FormId(FISTS));

    // Scripted SetWeaponOut 1 fires via hud_tick after WEAPON_FRAMES, and subsequent engine set_weapon_out(true) does not double-fire.
    query(&mut g, "player.SetWeaponOut 1");
    g.frames(WEAPON_FRAMES - 1);
    assert_eq!(num(&mut g, "unh.calls"), 3.0, "must wait until WEAPON_FRAMES elapse");
    g.frames(1);
    assert_eq!(num(&mut g, "unh.calls"), 4.0);
    assert_eq!(form(&mut g, "unh.actor"), FormId::PLAYER_REF);
    assert_eq!(form(&mut g, "unh.weapon"), FormId(WEAP_PISTOL));
    g.set_weapon_out(FormId::PLAYER_REF, true);
    assert_eq!(num(&mut g, "unh.calls"), 4.0, "already drawn: no duplicate fire");

    // Snapshot restore with weapon_out == true does not spuriously fire OnWeaponUnholster.
    let snap = g.snapshot();
    let mut restored = Game::restore(&f.data, World::new(&f.forms, &f.refs, Default::default()), GameConfig { strict: true, ..Default::default() }, &snap).unwrap();
    assert!(restored.extra.get(&FormId::PLAYER_REF).is_some_and(|e| e.weapon_out));
    query(
        &mut restored,
        r#"SetEventHandler "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "restored.unh" ((GetModLocalData "restored.unh") + 1)
End)"#,
    );
    // First engine frame after load transitioning true -> false must not fire unholster.
    restored.set_weapon_out(FormId::PLAYER_REF, false);
    assert!(!restored.extra.get(&FormId::PLAYER_REF).is_some_and(|e| e.weapon_out));
    assert_eq!(num(&mut restored, "restored.unh"), 0.0);
    no_errors(&restored);
    no_errors(&g);
}

#[test]
fn weapon_unholster_unarmed_without_fists_record_passes_null_form() {
    let f = fixture(false);
    let mut g = game(&f);
    query(
        &mut g,
        r#"SetEventHandler "ShowOff:OnWeaponUnholster" (Begin Function {ref rWeapon}
SetModLocalData "unh.calls" ((GetModLocalData "unh.calls") + 1)
SetModLocalData "unh.weapon" rWeapon
End)"#,
    );
    g.set_weapon_out(FormId::PLAYER_REF, true);
    assert_eq!(num(&mut g, "unh.calls"), 1.0);
    assert_eq!(form(&mut g, "unh.weapon"), FormId(0));
    no_errors(&g);
}
