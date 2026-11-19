//! Source VM queries against authored published scene trees; no installed data.
use crate::common::model_has_block::*;
use fnv_game::game::{Game, GameSnapshot};
use fnv_game::nif_blocks::ModelView;
use fnv_script::vm::XVal;
use fnv_sim::{FormId, World};

#[test]
fn published_reference_names_are_exact_and_instance_local() {
    with_game(|g| {
        g.scene.set_tree(FIRST, tree("OwnedAttachment"));
        g.scene.set_tree(SECOND, tree("OtherAttachment"));
        g.scene.set_tree_for(FormId::PLAYER_REF, ModelView::FirstPerson, tree("FirstOnly"));
        g.scene.set_tree_for(FormId::PLAYER_REF, ModelView::ThirdPerson, tree("ThirdOnly"));
        g.scene.use_separate_player_models();
        assert_eq!(has(g, "OwnedRefA", "OwnedAttachment"), XVal::Num(1.0));
        assert_eq!(has(g, "OwnedRefB", "OwnedAttachment"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedRefA", "ownedattachment"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedRefA", "Missing"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedRefA", ""), XVal::Num(0.0));
        assert_eq!(has(g, "PlayerREF", "ThirdOnly"), XVal::Num(1.0));
        assert_eq!(has(g, "PlayerREF", "FirstOnly"), XVal::Num(0.0), "retain the mirror's existing default model");
        g.scene.set_tree(FIRST, tree(""));
        assert_eq!(has(g, "OwnedRefA", ""), XVal::Num(0.0), "retain the file helper's empty-name result even with an unnamed scene node");
        assert!(g.scene.take_edits().is_empty(), "queries do not queue scene edits");
    });
}

#[test]
fn source_vm_sees_insert_rename_and_remove_without_a_new_publication() {
    with_game(|g| {
        g.scene.set_tree(FIRST, tree("OwnedRoot"));
        assert_eq!(query(g, "InsertNode OwnedRefA 3 \"OwnedRoot|Added\"\nSetFunctionValue (ModelHasBlock OwnedRefA \"Added\")"), XVal::Num(1.0));
        assert_eq!(query(g, "OwnedRefA.RenameNifBlock \"Added\" \"Renamed\"\nSetFunctionValue (ModelHasBlock OwnedRefA \"Renamed\")"), XVal::Num(1.0));
        assert_eq!(has(g, "OwnedRefA", "Added"), XVal::Num(0.0));
        assert_eq!(query(g, "OwnedRefA.RemoveNifBlock \"Renamed\"\nSetFunctionValue (ModelHasBlock OwnedRefA \"Renamed\")"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedRefA", "OwnedRoot"), XVal::Num(1.0));
        assert_eq!(g.scene.take_edits().len(), 3, "only the three explicit mutations queue edits");
    });
}

#[test]
fn empty_and_invalid_objects_do_not_become_the_running_self() {
    with_game(|g| {
        g.scene.set_tree(FormId::PLAYER_REF, tree("SelfOnly"));
        g.scene.set_tree(FormId(0), tree("SelfOnly"));
        g.scene.set_tree(FormId(0xDEAD), tree("SelfOnly"));
        assert_eq!(has(g, "PlayerREF", "SelfOnly"), XVal::Num(1.0));
        assert_eq!(has(g, "empty", "SelfOnly"), XVal::Num(0.0));
        assert_eq!(has(g, "0", "SelfOnly"), XVal::Num(0.0));
        assert_eq!(has(g, "57005", "SelfOnly"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedModel", "SelfOnly"), XVal::Num(0.0));
        assert!(g.scene.take_edits().is_empty());
    });
}

#[test]
fn host_forget_and_save_restore_drop_session_model_publications() {
    with_game(|g| {
        g.scene.set_tree(FIRST, tree("SessionOnly"));
        assert_eq!(has(g, "OwnedRefA", "SessionOnly"), XVal::Num(1.0));
        g.scene.forget(FIRST);
        assert_eq!(has(g, "OwnedRefA", "SessionOnly"), XVal::Num(0.0), "unpublished model after unload");
        g.scene.set_tree(FIRST, tree("Rebuilt"));
        assert_eq!(has(g, "OwnedRefA", "Rebuilt"), XVal::Num(1.0));
        g.world.set_deleted(FIRST, true).unwrap();
        g.scene.forget(FIRST);
        assert_eq!(has(g, "OwnedRefA", "Rebuilt"), XVal::Num(0.0), "host removes a deleted reference's model");
        g.scene.set_tree(SECOND, tree("SavedSession"));
        assert_eq!(has(g, "OwnedRefB", "SavedSession"), XVal::Num(1.0));
        let bytes = serde_json::to_vec(&g.checked_snapshot().unwrap()).unwrap();
        let snapshot: GameSnapshot = serde_json::from_slice(&bytes).unwrap();
        let world = World::restore(g.world.forms, g.world.placed, &snapshot.sim).unwrap();
        let mut back = Game::restore(g.data, world, g.cfg, &snapshot).unwrap();
        assert_eq!(has(&mut back, "OwnedRefB", "SavedSession"), XVal::Num(0.0));
        assert_eq!(has(g, "OwnedRefB", "SavedSession"), XVal::Num(1.0), "saving does not mutate the running mirror");
        back.scene.set_tree(SECOND, tree("AfterLoad"));
        assert_eq!(has(&mut back, "OwnedRefB", "AfterLoad"), XVal::Num(1.0));
    });
}
