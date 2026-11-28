use super::{parse_static_gates, StaticGate};
use crate::data::GameData;
use crate::game::{Game, GameConfig};
use fnv_esm::{LoadOrder, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

fn empty_game() -> (GameData, FormStore, RefTable) {
    let plugins: &[fnv_esm::Plugin] = &[];
    let lo = LoadOrder::new(plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalog: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalog, &refs);
    (data, forms, refs)
}

#[test]
fn parse_static_gates_extracts_sound_conjuncts_only() {
    let iv = r#"AuxVarGetFlt "*_IVDialogTimer" == 0 && GetIsSex Female && ((HasKeyWord (this.GBO) "IV_ArrogantLook") || (this.GetActorLeveledList != 0 && HasKeyWord (this.GetLeveledActorBase) "IV_ArrogantLook")) && (HasKeyword (GetEqObj 2) "TNW") == 0"#;
    let gates = parse_static_gates(iv);
    assert!(gates.contains(&StaticGate::SexFemale(true)), "{gates:?}");
    assert!(gates.contains(&StaticGate::ActorOrLeveledKeyword("IV_ArrogantLook".into())), "{gates:?}");
    assert!(gates.contains(&StaticGate::Slot2Keyword("TNW".into(), false)), "{gates:?}");

    let tnw = r#"(HasKeyword (GetEqObj 2) "TNW3" == 1)"#;
    assert_eq!(parse_static_gates(tnw), vec![StaticGate::Slot2Keyword("TNW3".into(), true)]);

    let polled = r#"IsInCombat == 0 && IsWeaponOut == 1 && (HasKeyword (GetEqObj 2) "TNW") == 0"#;
    assert_eq!(
        parse_static_gates(polled),
        vec![StaticGate::WeaponOut(true), StaticGate::Slot2Keyword("TNW".into(), false)]
    );

    let teammate = r#"GetPlayerTeammate == 1 && Player.IsSneaking == 1"#;
    assert_eq!(parse_static_gates(teammate), vec![StaticGate::PlayerTeammate(true)]);

    // Top-level disjunction must return empty so we never false-reject.
    assert!(parse_static_gates("GetIsSex Female || IsWeaponOut == 1").is_empty());
}

#[test]
fn has_keyword_fast_path_handles_empty_direct_and_case_insensitive() {
    let (data, forms, refs) = empty_game();
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    let form = FormId(0x0000_7001);
    assert!(!g.has_keyword(form, "MyTag"));
    assert!(!g.has_keyword(form, ""));

    g.jip2.keywords.entry(form).or_default().insert("mytag".into());
    assert!(g.has_keyword(form, "MyTag"));
    assert!(g.has_keyword(form, "MYTAG"));
    assert!(!g.has_keyword(FormId(0x0000_7002), "MyTag"));
}

#[test]
fn static_gate_weapon_out_matches_is_weapon_out_for_fallback_dead_and_unarmed_crea() {
    use super::FolderEntry;
    use std::collections::HashMap;

    let (mut data, mut forms, refs) = empty_game();
    let npc_base = FormId(0x0000_7100);
    let crea_base = FormId(0x0000_7101);
    let weap_form = FormId(0x0000_7102);
    let cell = FormId(0x0000_7103);
    data.sigs.insert(npc_base, *b"NPC_");
    data.sigs.insert(crea_base, *b"CREA");
    forms.sigs.insert(npc_base, *b"NPC_");
    forms.sigs.insert(crea_base, *b"CREA");

    let mut world = World::new(&forms, &refs, SimConfig::default());
    let npc = world.place(npc_base, Some(cell), [0.0; 3], [0.0; 3], 1).unwrap();
    let crea = world.place(crea_base, Some(cell), [0.0; 3], [0.0; 3], 1).unwrap();

    let mut g = Game::new(&data, world, GameConfig::default());
    let entry = FolderEntry {
        folder: "test".into(),
        ..Default::default()
    };
    let gates = vec![vec![StaticGate::WeaponOut(true)]];
    let mut compiled = vec![None];
    let mut cache = HashMap::new();
    let mut slot2 = None;

    // Living NPC with weapon_out=false but wants_weapon_out=true (via hud.forced_out) -> fallback is true.
    g.extra.entry(npc).or_default().weapon_out = false;
    g.hud.forced_out.insert(npc);
    assert!(g.weapon_is_out(npc));
    assert!(g.folder_entry_holds(0, &entry, &gates, &mut compiled, &mut cache, npc, false, Some(npc_base), None, Some(weap_form), &mut slot2));

    // Dead NPC even with weapon_out=true -> IsWeaponOut is false.
    g.extra.entry(npc).or_default().weapon_out = true;
    g.extra.entry(npc).or_default().dead = true;
    assert!(!g.folder_entry_holds(0, &entry, &gates, &mut compiled, &mut cache, npc, false, Some(npc_base), None, Some(weap_form), &mut slot2));

    // Unarmed CREA with weapon_out=true -> IsWeaponOut is false; armed CREA -> true.
    g.extra.entry(crea).or_default().weapon_out = true;
    g.hud.forced_out.insert(crea);
    assert!(!g.folder_entry_holds(0, &entry, &gates, &mut compiled, &mut cache, crea, false, Some(crea_base), None, None, &mut slot2));
    assert!(g.folder_entry_holds(0, &entry, &gates, &mut compiled, &mut cache, crea, false, Some(crea_base), None, Some(weap_form), &mut slot2));

    // PlayerTeammate aligns with get_player_teammate (`ex.teammate`).
    let tm_gates = vec![vec![StaticGate::PlayerTeammate(true)]];
    assert!(!g.folder_entry_holds(0, &entry, &tm_gates, &mut compiled, &mut cache, npc, false, Some(npc_base), None, None, &mut slot2));
    g.extra.entry(npc).or_default().teammate = true;
    assert!(g.folder_entry_holds(0, &entry, &tm_gates, &mut compiled, &mut cache, npc, false, Some(npc_base), None, None, &mut slot2));
}
