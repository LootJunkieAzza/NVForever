use super::*;

#[test]
fn non_actor_created_index_tracks_non_actors_and_ignores_dynamic_actors() {
    let mut forms = FormStore::default();
    let item_base = FormId(0x0001_0001);
    let npc_base = FormId(0x0001_0002);
    forms.sigs.insert(item_base, *b"MISC");
    forms.sigs.insert(npc_base, *b"NPC_");
    let placed = RefTable::default();
    let mut w = World::new(&forms, &placed, SimConfig::default());

    let epoch1 = w.created_epoch();
    let gen0 = w.non_actor_created_gen();
    assert_eq!(w.non_actor_created().count(), 0);

    // Placing an NPC dynamic ref must not enter non_actor_created or bump non_actor_created_gen.
    let npc_ref = w.place(npc_base, None, [0.0; 3], [0.0; 3], 1).unwrap();
    assert_eq!(w.non_actor_created_gen(), gen0);
    assert_eq!(w.non_actor_created().count(), 0);
    // Mutating the dynamic NPC via get() also leaves non_actor_created_gen unchanged.
    w.get(npc_ref).unwrap().pos[0] = 5.0;
    assert_eq!(w.non_actor_created_gen(), gen0);

    // Placing a non-actor dynamic ref enters non_actor_created and bumps non_actor_created_gen.
    let drop_ref = w.place(item_base, None, [1.0, 2.0, 3.0], [0.0; 3], 1).unwrap();
    let gen1 = w.non_actor_created_gen();
    assert!(gen1 > gen0);
    assert_eq!(w.non_actor_created().map(|(id, _)| id).collect::<Vec<_>>(), vec![drop_ref]);

    // Mutating the non-actor dynamic ref via get() bumps non_actor_created_gen.
    w.get(drop_ref).unwrap().pos[0] = 42.0;
    let gen2 = w.non_actor_created_gen();
    assert!(gen2 > gen1);

    // Snapshot & restore rebuilds non_actor_created (excluding actors) and assigns a fresh created_epoch.
    let snap = w.snapshot();
    let w2 = World::restore(&forms, &placed, &snap).unwrap();
    assert_ne!(w2.created_epoch(), epoch1);
    assert_eq!(w2.non_actor_created().map(|(id, _)| id).collect::<Vec<_>>(), vec![drop_ref]);
}
