//! What mods that move the player's model nodes by script lean on (B42 True Leaning,
//! ISControl): JohnnyGuitar's RenderUpdate handlers read the frame's time, InsertNode's
//! nodes go into models drawn after the call, and GetReticleRange tells nothing under the
//! crosshair apart. Authored scene trees and scripts; no installed data.
use crate::common::model_has_block::*;
use fnv_game::nif_blocks::{ModelView, SceneEdit, SceneNode};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn nodes(names: &[&str]) -> Vec<SceneNode> {
    names.iter().enumerate().map(|(i, n)| SceneNode { name: (*n).into(), parent: i.checked_sub(1), translation: [0.0; 3], rotation: [0.0; 3], scale: 1.0, flags: 0, material: None, info: None }).collect()
}

fn inserts(edits: &[SceneEdit]) -> usize {
    edits.iter().filter(|e| matches!((*e).clone().into_model().1, SceneEdit::Insert { .. })).count()
}

#[test]
fn render_update_handlers_read_the_time_since_the_last_frame() {
    with_game(|g| {
        g.cfg.frame_seconds = 1.0 / 64.0;
        query(g, "SetJohnnyOnRenderUpdateEventHandler 1 (begin function {}\nPlayerREF.AuxVarSetFlt \"*_owned_seconds\" (PlayerREF.AuxVarGetFlt \"*_owned_seconds\" + GetSecondsPassed)\nend) 0");
        for _ in 0..8 {
            g.tick();
        }
        let XVal::Num(seconds) = query(g, "SetFunctionValue (PlayerREF.AuxVarGetFlt \"*_owned_seconds\")") else { panic!("a number") };
        // The first dispatch has no frame before it.
        assert!((seconds - 7.0 / 64.0).abs() < 1e-9, "{seconds}");
    });
}

#[test]
fn insert_node_goes_into_a_model_drawn_after_the_call_once() {
    with_game(|g| {
        assert_eq!(query(g, "SetFunctionValue (InsertNode OwnedRefA 3 \"OwnedRoot|^Added\")"), XVal::Num(0.0), "no model drawn yet");
        g.scene.set_tree(FIRST, tree("OwnedRoot"));
        g.tick();
        assert_eq!(has(g, "OwnedRefA", "Added"), XVal::Num(1.0));
        assert_eq!(inserts(&g.scene.take_edits()), 1);
        // The engine tells the tree again before it shows the node: no second copy.
        g.scene.set_tree(FIRST, tree("OwnedRoot"));
        g.tick();
        assert_eq!(inserts(&g.scene.take_edits()), 0);
        assert_eq!(has(g, "OwnedRefB", "Added"), XVal::Num(0.0), "another reference keeps its model");
        // The host shows it, then tells a model drawn anew without it (no identity to tell
        // the two apart): it goes in again.
        g.scene.set_tree(FIRST, nodes(&["Added", "OwnedRoot"]));
        g.tick();
        assert_eq!(inserts(&g.scene.take_edits()), 0);
        g.scene.set_tree(FIRST, tree("OwnedRoot"));
        g.tick();
        assert_eq!(inserts(&g.scene.take_edits()), 1);
        assert_eq!(has(g, "OwnedRefA", "Added"), XVal::Num(1.0));
    });
}

#[test]
fn insert_node_on_the_player_goes_into_each_model_with_the_target() {
    with_game(|g| {
        g.scene.use_separate_player_models();
        g.scene.set_tree_for(FormId::PLAYER_REF, ModelView::FirstPerson, nodes(&["Scene Root", "Bip01 Looking", "Camera1st"]));
        g.scene.set_tree_for(FormId::PLAYER_REF, ModelView::ThirdPerson, nodes(&["Scene Root", "Bip01 Head"]));
        assert_eq!(query(g, "SetFunctionValue (InsertNode PlayerREF 3 \"Camera1st|^OwnedCam\")"), XVal::Num(1.0));
        assert_eq!(query(g, "SetFunctionValue (InsertNode PlayerREF 3 \"Bip01 Head|^OwnedHead\")"), XVal::Num(1.0));
        let first = g.scene.node_for_model(FormId::PLAYER_REF, ModelView::FirstPerson, "OwnedCam");
        assert!(first.is_some_and(|n| n.parent == Some(1)), "between Camera1st and its parent");
        assert!(g.scene.node_for_model(FormId::PLAYER_REF, ModelView::ThirdPerson, "OwnedCam").is_none(), "no Camera1st there");
        assert!(g.scene.node_for_model(FormId::PLAYER_REF, ModelView::ThirdPerson, "OwnedHead").is_some());
        assert_eq!(inserts(&g.scene.take_edits()), 2);
    });
}

#[test]
fn reticle_range_is_minus_one_with_nothing_under_the_crosshair() {
    with_game(|g| {
        assert_eq!(query(g, "SetFunctionValue (GetReticleRange)"), XVal::Num(-1.0));
    });
}

/// `OwnedRefA` drawn as `model` (an identity, or none) with InsertNode's `Added` shown.
fn with_shown_node(g: &mut fnv_game::game::Game<'_>, model: Option<u64>) {
    tell(g, model, &["OwnedRoot"]);
    query(g, "InsertNode OwnedRefA 3 \"OwnedRoot|Added\"");
    tell(g, model, &["OwnedRoot", "Added"]);
    g.tick();
    g.scene.take_edits();
}

fn tell(g: &mut fnv_game::game::Game<'_>, model: Option<u64>, names: &[&str]) {
    match model {
        Some(model) => g.scene.set_identified_model_for(FIRST, ModelView::Reference, model, nodes(names)),
        None => g.scene.set_tree(FIRST, nodes(names)),
    }
}

/// A frame passes: how many nodes it inserted.
fn frame(g: &mut fnv_game::game::Game<'_>) -> usize {
    g.tick();
    inserts(&g.scene.take_edits())
}

#[test]
fn a_removed_node_stays_removed_in_that_model() {
    for model in [None, Some(7)] {
        with_game(|g| {
            with_shown_node(g, model);
            query(g, "OwnedRefA.RemoveNifBlock \"added\"");
            assert_eq!(frame(g), 0, "{model:?}: the next frame");
            // The host applies the removal late: it tells the node once more, then without it.
            tell(g, model, &["OwnedRoot", "Added"]);
            assert_eq!(frame(g), 0, "{model:?}: still shown");
            tell(g, model, &["OwnedRoot"]);
            assert_eq!(frame(g), 0, "{model:?}: shown no more");
            assert_eq!(has(g, "OwnedRefA", "Added"), XVal::Num(0.0));
        });
    }
}

#[test]
fn a_node_removed_with_the_one_above_it_stays_removed() {
    with_game(|g| {
        tell(g, Some(7), &["OwnedRoot"]);
        query(g, "InsertNode OwnedRefA 3 \"OwnedRoot|^Above\"");
        tell(g, Some(7), &["Scene Root", "Holder", "Above"]);
        g.tick();
        g.scene.take_edits();
        query(g, "OwnedRefA.RemoveNifBlock \"Holder\"");
        // The target stands elsewhere in the model, so the entry could go in again.
        tell(g, Some(7), &["Scene Root", "OwnedRoot"]);
        assert_eq!(frame(g), 0);
    });
}

#[test]
fn a_renamed_node_is_not_inserted_again_under_its_old_name() {
    for model in [None, Some(7)] {
        with_game(|g| {
            with_shown_node(g, model);
            query(g, "OwnedRefA.RenameNifBlock \"Added\" \"Renamed\"");
            assert_eq!(frame(g), 0, "{model:?}: the next frame");
            tell(g, model, &["OwnedRoot", "Renamed"]);
            assert_eq!(frame(g), 0, "{model:?}: told renamed");
        });
    }
}

#[test]
fn a_removed_node_comes_back_with_a_new_model_or_a_new_insert_node() {
    with_game(|g| {
        with_shown_node(g, Some(7));
        query(g, "OwnedRefA.RemoveNifBlock \"Added\"");
        tell(g, Some(8), &["OwnedRoot"]);
        assert_eq!(frame(g), 1, "another model identity");
    });
    with_game(|g| {
        with_shown_node(g, None);
        query(g, "OwnedRefA.RemoveNifBlock \"Added\"");
        g.scene.forget(FIRST);
        tell(g, None, &["OwnedRoot"]);
        assert_eq!(frame(g), 1, "the 3D was gone");
    });
    with_game(|g| {
        with_shown_node(g, None);
        query(g, "OwnedRefA.RemoveNifBlock \"Added\"");
        g.scene.forget_model(FIRST, ModelView::Reference);
        tell(g, None, &["OwnedRoot"]);
        assert_eq!(frame(g), 1, "the model was gone");
    });
    for model in [None, Some(7)] {
        with_game(|g| {
            // Removed before the host ever showed it, and asked for again.
            tell(g, model, &["OwnedRoot"]);
            query(g, "InsertNode OwnedRefA 3 \"OwnedRoot|Added\"\nOwnedRefA.RemoveNifBlock \"Added\"");
            g.scene.take_edits();
            assert_eq!(frame(g), 0, "{model:?}: removed before it was shown");
            query(g, "InsertNode OwnedRefA 3 \"OwnedRoot|Added\"");
            assert_eq!(inserts(&g.scene.take_edits()), 1, "{model:?}: asked for again");
        });
    }
}

#[test]
fn a_removal_holds_in_the_player_model_it_was_made_in() {
    with_game(|g| {
        let player = FormId::PLAYER_REF;
        g.scene.use_separate_player_models();
        for (view, id) in [(ModelView::ThirdPerson, 3), (ModelView::FirstPerson, 1)] {
            g.scene.set_identified_model_for(player, view, id, nodes(&["OwnedRoot"]));
        }
        query(g, "InsertNode PlayerREF 3 \"OwnedRoot|Added\"");
        for (view, id) in [(ModelView::ThirdPerson, 3), (ModelView::FirstPerson, 1)] {
            g.scene.set_identified_model_for(player, view, id, nodes(&["OwnedRoot", "Added"]));
        }
        g.tick();
        g.scene.take_edits();
        query(g, "PlayerREF.RemoveNifBlock \"Added\" 2");
        assert_eq!(frame(g), 0);
        assert!(g.scene.node_for_model(player, ModelView::FirstPerson, "Added").is_none());
        assert!(g.scene.node_for_model(player, ModelView::ThirdPerson, "Added").is_some());
    });
}
