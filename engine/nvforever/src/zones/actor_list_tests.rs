//! Authored source-to-model controls; no game data or rendered frames.
use super::ActorListSync;
use fnv_actor::{records::ActorForms, resolve::Resolver};
use fnv_esm::{raw::{write_subrecords, Subrecord}, LoadOrder, Plugin, Schema};
use fnv_game::{game::GameConfig, Game, GameData};
use fnv_sim::{zones::LevelAt, FormId, FormStore, RefTable, World};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn record(sig: &[u8; 4], id: u32, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [&sig[..], &(body.len() as u32).to_le_bytes(), &[0; 4], &id.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &body].concat()
}

fn top(sig: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], &body].concat()
}

fn fixture() -> Vec<u8> {
    let named = |name: &str| sub(b"EDID", [name.as_bytes(), &[0]].concat());
    let actor = |sig, id, name| record(sig, id, &[named(name)]);
    let list = |sig, id, name, item: u32| record(sig, id, &[
        named(name), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]),
        sub(b"LVLO", [1u16.to_le_bytes().to_vec(), vec![0; 2], item.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), vec![0; 2]].concat()),
    ]);
    [record(b"TES4", 0, &[sub(b"HEDR", [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat())]),
        top(b"NPC_", [actor(b"NPC_", 0x801, "InitialNPC"), actor(b"NPC_", 0x802, "AddedNPC")].concat()),
        top(b"CREA", [actor(b"CREA", 0x811, "InitialCreature"), actor(b"CREA", 0x812, "AddedCreature")].concat()),
        top(b"LVLN", [list(b"LVLN", 0x821, "NPCTiers", 0x801), list(b"LVLN", 0x822, "NestedNPC", 0x802)].concat()),
        top(b"LVLC", [list(b"LVLC", 0x831, "CreatureTiers", 0x811), list(b"LVLC", 0x832, "NestedCreature", 0x812)].concat()),
    ].concat()
}

fn source(g: &mut Game<'_>, text: &str) {
    g.run_query(&format!("Begin Function {{}}\n{text}\nEnd\n"), Default::default()).unwrap();
}

fn assert_pick(g: &Game<'_>, actors: &ActorForms, list: u32, level: u16, expected: Option<u32>) {
    let at = LevelAt::plain(level);
    let reference = FormId(0xA01);
    let rendered = Resolver { actors, forms: g.world.forms }.first_pick_at(FormId(list), reference, at);
    let game = g.runtime_template_source(FormId(list), reference, at, |_| false).map(|(id, _)| id);
    assert_eq!(game, expected.map(FormId));
    assert_eq!(rendered, game);
}

#[test]
fn source_actor_lists_reach_the_actual_model_resolver_across_session_and_save_transitions() {
    let bytes = fixture();
    let plugins = [Plugin::parse(&bytes, "ActorModels.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, fnv_script::FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    let new_game = || Game::new(&data, World::new(&forms, &refs, Default::default()), GameConfig { strict: true, ..Default::default() });
    let mut g = new_game();
    let mut actors = ActorForms::build(&lo, &schema);
    let mut sync = ActorListSync::default();
    for (list, initial) in [(0x821, 0x801), (0x831, 0x811)] {
        assert_pick(&g, &actors, list, 5, Some(initial));
    }

    source(&mut g, "AddNPCToLeveledList NPCTiers NestedNPC 5\nAddCreatureToLeveledList CreatureTiers NestedCreature 5\n");
    sync.accept(&g, &mut actors);
    for (list, initial, added) in [(0x821, 0x801, 0x802), (0x831, 0x811, 0x812)] {
        assert_pick(&g, &actors, list, 4, Some(initial));
        assert_pick(&g, &actors, list, 5, Some(added));
    }
    source(&mut g, "SetChanceNone NPCTiers 100\nSetChanceNone CreatureTiers 100\n");
    sync.accept(&g, &mut actors);
    for list in [0x821, 0x831] { assert_pick(&g, &actors, list, 5, None); }

    let saved = g.checked_snapshot().unwrap();
    let world = World::restore(&forms, &refs, &saved.sim).unwrap();
    let restored = Game::restore(&data, world, g.cfg, &saved).unwrap();
    sync.accept(&restored, &mut actors);
    for (list, added) in [(0x821, 0x802), (0x831, 0x812)] { assert_pick(&restored, &actors, list, 5, Some(added)); }
    // Reuse the same renderer and sync state when loading a game without additions.
    let plain = new_game();
    sync.accept(&plain, &mut actors);
    for (list, initial) in [(0x821, 0x801), (0x831, 0x811)] { assert_pick(&plain, &actors, list, 5, Some(initial)); }
}
