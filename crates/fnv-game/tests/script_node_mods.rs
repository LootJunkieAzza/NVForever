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
