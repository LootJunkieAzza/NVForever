use crate::data::{GameData, PerkDef, PerkEffect, PerkEntry};
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
fn perk_ability_spells_index_grants_keeps_and_removes_stale_abilities() {
    let (mut data, forms, refs) = empty_game();
    let perk = FormId(0x0000_5001);
    let perk_spell = FormId(0x0000_5002);
    let regular_spell = FormId(0x0000_5003);
    data.spell_types.insert(perk_spell, crate::abilities::SPELL_ABILITY);
    data.spell_types.insert(regular_spell, crate::abilities::SPELL_ABILITY);
    data.perks.insert(
        perk,
        PerkDef {
            name: "TestPerk".into(),
            description: String::new(),
            icon: None,
            is_trait: false,
            min_level: 0,
            ranks: 1,
            playable: true,
            hidden: false,
            conditions: Some(Vec::new()),
            entries: vec![PerkEntry {
                rank: 0,
                priority: 0,
                ordinal: 0,
                effect: PerkEffect::Ability(perk_spell),
            }],
        },
    );
    data.perk_ability_spells.insert(perk_spell);

    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    g.extra.entry(FormId::PLAYER_REF).or_default().spells.push(regular_spell);
    g.player.perks.insert(perk, 1);

    // Granting the perk adds perk_spell while preserving unrelated regular_spell.
    g.sync_loaded_actor_effects();
    let spells = &g.extra[&FormId::PLAYER_REF].spells;
    assert!(spells.contains(&perk_spell));
    assert!(spells.contains(&regular_spell));

    // Steady-state second sync is a clean no-op.
    g.sync_loaded_actor_effects();
    assert!(g.extra[&FormId::PLAYER_REF].spells.contains(&perk_spell));

    // Removing the perk removes only the indexed stale perk_spell, keeping regular_spell.
    g.player.perks.remove(&perk);
    g.sync_loaded_actor_effects();
    let spells = &g.extra[&FormId::PLAYER_REF].spells;
    assert!(!spells.contains(&perk_spell));
    assert!(spells.contains(&regular_spell));
}

#[test]
fn give_abilities_reverts_race_abilities_when_override_is_cleared() {
    let (mut data, mut forms, refs) = empty_game();
    let base_race = FormId(0x0000_6001);
    let alt_race = FormId(0x0000_6002);
    let base_spell = FormId(0x0000_6011);
    let alt_spell = FormId(0x0000_6012);
    let npc_base = FormId(0x0000_6020);
    let npc_ref = FormId(0x0000_6021);
    let cell = FormId(0x0000_6030);

    data.spell_types.insert(base_spell, crate::abilities::SPELL_ABILITY);
    data.spell_types.insert(alt_spell, crate::abilities::SPELL_ABILITY);
    data.race_spells.insert(base_race, vec![base_spell]);
    data.race_spells.insert(alt_race, vec![alt_spell]);
    data.actors.insert(
        npc_base,
        crate::data::ActorDef {
            race: Some(base_race),
            ..Default::default()
        },
    );
    forms.sigs.insert(npc_base, *b"NPC_");

    let mut world = World::new(&forms, &refs, SimConfig::default());
    let dynamic_ref = world.place(npc_base, Some(cell), [0.0; 3], [0.0; 3], 1).unwrap();
    let _ = npc_ref;

    let mut g = Game::new(&data, world, GameConfig::default());
    g.loaded_cells.insert(cell);

    // Initial grant gives base_race ability and marks ability_race_matches_base.
    g.give_abilities(dynamic_ref);
    assert!(g.extra[&dynamic_ref].spells.contains(&base_spell));
    assert!(!g.extra[&dynamic_ref].spells.contains(&alt_spell));
    assert_eq!(g.extra[&dynamic_ref].ability_race, Some(base_race));
    assert!(g.extra[&dynamic_ref].ability_race_matches_base);

    // Override race -> swaps to alt_race ability and clears ability_race_matches_base.
    g.extra.entry(dynamic_ref).or_default().race = Some(alt_race);
    g.give_abilities(dynamic_ref);
    assert!(!g.extra[&dynamic_ref].spells.contains(&base_spell));
    assert!(g.extra[&dynamic_ref].spells.contains(&alt_spell));
    assert_eq!(g.extra[&dynamic_ref].ability_race, Some(alt_race));
    assert!(!g.extra[&dynamic_ref].ability_race_matches_base);

    // Clear override -> must NOT early-return; reverts to base_race ability!
    g.extra.entry(dynamic_ref).or_default().race = None;
    g.give_abilities(dynamic_ref);
    assert!(g.extra[&dynamic_ref].spells.contains(&base_spell));
    assert!(!g.extra[&dynamic_ref].spells.contains(&alt_spell));
    assert_eq!(g.extra[&dynamic_ref].ability_race, Some(base_race));
    assert!(g.extra[&dynamic_ref].ability_race_matches_base);
}
