//! Owned dual-rig controls for #2387. No installed assets or native captures.
mod common;
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nif_blocks::{ModelView, SceneEdit, SceneNode};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const P: FormId = FormId::PLAYER_REF;
const NPC: FormId = FormId(0x501);
const CELL: u32 = 0x900;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let npcs = [record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        record(b"NPC_", 0, 0x500, &[sub(b"EDID", z("OwnedActor"))])].concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("OwnedRoom")), sub(b"DATA", vec![1])]);
    let actor = record(b"ACHR", 0, NPC.0, &[sub(b"NAME", 0x500u32.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])]);
    let cell = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &actor))].concat();
    let bytes = [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedModels.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}

fn tree(root: &str, x: f32) -> Vec<SceneNode> {
    let node = |name: &str, parent, x| SceneNode { name: name.into(), parent,
        translation: [x, 0.0, 0.0], rotation: [x, 0.0, 0.0], scale: x,
        flags: 0, material: None, info: None };
    vec![node(root, None, 0.0), node("SharedBone", Some(0), x)]
}

fn game(f: &Fixture) -> Game<'_> {
    let mut g = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g.scene.set_tree_for(P, ModelView::ThirdPerson, tree("ThirdRoot", 30.0));
    g.scene.set_tree_for(P, ModelView::FirstPerson, tree("FirstRoot", 10.0));
    g.scene.use_separate_player_models();
    g.scene.set_tree(NPC, tree("ActorRoot", 70.0));
    g
}

fn call(g: &mut Game, r: FormId, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(g, &RunCtx::default(), name, Some(r), args).unwrap().unwrap()
}
fn text(s: &str) -> XVal { XVal::Str(s.into()) }
fn num(n: f64) -> XVal { XVal::Num(n) }
fn array(g: &mut Game, r: FormId, name: &str, args: &[XVal]) -> Vec<XVal> {
    match call(g, r, name, args) {
        XVal::Array(0) => Vec::new(),
        XVal::Array(id) => g.ext.array(id).unwrap().entries.iter().map(|(_, v)| v.clone()).collect(),
        v => panic!("{name}: {v:?}"),
    }
}
fn scale(g: &mut Game, r: FormId, selector: f64) -> XVal {
    call(g, r, "getnifblockscale", &[text("SharedBone"), num(selector)])
}

#[test]
fn explicit_player_models_read_separate_names_and_transforms() {
    let f = fixture(); let mut g = game(&f);
    for (selector, root, x) in [(1.0, "ThirdRoot", 30.0), (2.0, "FirstRoot", 10.0)] {
        assert_eq!(array(&mut g, P, "getnifblockparentnodes", &[text("SharedBone"), num(selector)]), [text(root)]);
        assert_eq!(array(&mut g, P, "getchildblocks", &[text(root), num(selector)]), [text("SharedBone")]);
        assert_eq!(array(&mut g, P, "getnifblocktranslation", &[text("SharedBone"), num(0.0), num(selector)]), [num(x), num(0.0), num(0.0)]);
        assert_eq!(array(&mut g, P, "getnifblockrotation", &[text("SharedBone"), num(0.0), num(selector)]), [num(x), num(0.0), num(0.0)]);
        assert_eq!(call(&mut g, P, "getnifblocktranslation", &[text("SharedBone"), text("X"), num(0.0), num(selector)]), num(x));
        assert_eq!(array(&mut g, P, "getnifblocktranslationalt", &[text("SharedBone"), num(0.0), num(selector)]), [num(1.0), num(x), num(0.0), num(0.0)]);
        assert_eq!(scale(&mut g, P, selector), num(x));
    }
}

#[test]
fn omitted_selection_keeps_the_old_third_person_default_and_explicit_edits_keep_their_model() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(call(&mut g, P, "getnifblockscale", &[text("SharedBone")]), num(30.0));
    assert_eq!(scale(&mut g, P, 0.0), num(30.0));
    call(&mut g, P, "setnifblockscale", &[text("SharedBone"), num(11.0), num(2.0)]);
    assert_eq!(scale(&mut g, P, 2.0), num(11.0));
    call(&mut g, P, "setnifblockscale", &[text("SharedBone"), num(33.0), num(0.0)]);
    let edits: Vec<_> = g.scene.take_edits().into_iter().map(SceneEdit::into_model).collect();
    assert_eq!(edits, [
        (ModelView::FirstPerson, SceneEdit::Scale { r: P, node: "SharedBone".into(), v: 11.0 }),
        (ModelView::ThirdPerson, SceneEdit::Scale { r: P, node: "SharedBone".into(), v: 33.0 }),
    ]);
}

#[test]
fn selected_edits_are_visible_only_in_their_own_player_model() {
    let f = fixture(); let mut g = game(&f);
    for (name, args) in [
        ("setnifblocktranslation", vec![text("SharedBone"), num(4.0), num(5.0), num(6.0), num(2.0)]),
        ("setnifblockrotation", vec![text("SharedBone"), num(7.0), num(8.0), num(9.0), num(0.0), num(2.0)]),
        ("setnifblockflag", vec![text("SharedBone"), num(0.0), num(1.0), num(2.0)]),
    ] { call(&mut g, P, name, &args); }
    assert_eq!(array(&mut g, P, "getnifblocktranslation", &[text("SharedBone"), num(0.0), num(2.0)]), [num(4.0), num(5.0), num(6.0)]);
    assert_eq!(array(&mut g, P, "getnifblockrotation", &[text("SharedBone"), num(0.0), num(2.0)]), [num(7.0), num(8.0), num(9.0)]);
    assert_eq!(call(&mut g, P, "getnifblockflag", &[text("SharedBone"), num(0.0), num(2.0)]), num(1.0));
    assert_eq!(call(&mut g, P, "getnifblockflag", &[text("SharedBone"), num(0.0), num(1.0)]), num(0.0));
    assert_eq!(array(&mut g, P, "getnifblocktranslation", &[text("SharedBone"), num(0.0), num(1.0)]), [num(30.0), num(0.0), num(0.0)]);
    call(&mut g, P, "renamenifblock", &[text("SharedBone"), text("FirstOnly"), num(2.0)]);
    assert_eq!(array(&mut g, P, "getchildblocks", &[text("FirstRoot"), num(2.0)]), [text("FirstOnly")]);
    assert_eq!(array(&mut g, P, "getchildblocks", &[text("ThirdRoot"), num(1.0)]), [text("SharedBone")]);
    call(&mut g, P, "removenifblock", &[text("FirstOnly"), num(2.0)]);
    assert!(array(&mut g, P, "getchildblocks", &[text("FirstRoot"), num(2.0)]).is_empty());
    assert_eq!(array(&mut g, P, "getchildblocks", &[text(""), num(2.0)]), [text("FirstRoot")], "the selected root still exists; this is not missing-tree ar_Null");
    assert_eq!(scale(&mut g, P, 1.0), num(30.0));
    assert!(g.scene.take_edits().into_iter().all(|e| e.into_model().0 == ModelView::FirstPerson));
}

#[test]
fn a_missing_selected_rig_never_falls_back_and_ordinary_actors_ignore_player_selector() {
    let f = fixture(); let mut g = game(&f);
    g.scene.forget_model(P, ModelView::FirstPerson);
    assert_eq!(scale(&mut g, P, 2.0), num(0.0));
    assert_eq!(scale(&mut g, P, 0.0), num(30.0));
    assert_eq!(scale(&mut g, P, 1.0), num(30.0));
    assert_eq!(scale(&mut g, P, 0.0), num(30.0));
    for selector in [0.0, 1.0, 2.0] { assert_eq!(scale(&mut g, NPC, selector), num(70.0)); }
    call(&mut g, NPC, "setnifblockscale", &[text("SharedBone"), num(71.0), num(2.0)]);
    assert_eq!(g.scene.take_edits(), [SceneEdit::Scale { r: NPC, node: "SharedBone".into(), v: 71.0 }]);
    assert_eq!(scale(&mut g, P, 1.0), num(30.0));
}

/// Build the public call layout independently of the handler's selector table.
/// In particular, an intervening optional argument must occupy its catalogue slot.
fn documented_setter_args(name: &str, method: Option<f64>, selector: Option<f64>) -> Vec<XVal> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let catalogue: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let row = catalogue["functions"].as_array().unwrap().iter()
        .find(|row| row["name"] == name && row["page"] == name).unwrap();
    let params = row["params"].as_array().unwrap();
    let slot = |name: &str| params.iter().position(|p| p["name"] == name).unwrap();
    let last = if selector.is_some() { slot("pcModel") }
        else if method.is_some() { slot("method") }
        else { params.iter().rposition(|p| p["optional"] != true).unwrap() };
    params[..=last].iter().map(|param| match param["name"].as_str().unwrap() {
        "blockName" => text("SharedBone"),
        "posX" | "rotX" => num(7.0),
        "posY" | "rotY" => num(8.0),
        "posZ" | "rotZ" => num(9.0),
        "method" => num(method.unwrap_or(0.0)),
        "pcModel" => num(selector.unwrap()),
        other => panic!("unhandled documented parameter {name}.{other}"),
    }).collect()
}

#[test]
fn public_catalogue_layout_does_not_confuse_rotation_method_with_model_selection() {
    let f = fixture();
    let mut cases = Vec::new();
    for selector in [None, Some(1.0), Some(2.0)] {
        cases.push(("SetNifBlockTranslation", None, selector));
        for method in [0.0, 1.0, 2.0] {
            cases.push(("SetNifBlockRotation", Some(method), selector));
        }
    }
    cases.push(("SetNifBlockRotation", None, None));
    for (name, method, selector) in cases {
        let mut g = game(&f);
        let args = documented_setter_args(name, method, selector);
        call(&mut g, P, &name.to_ascii_lowercase(), &args);
        let view = if selector == Some(2.0) { ModelView::FirstPerson } else { ModelView::ThirdPerson };
        let selected = if view == ModelView::FirstPerson { 2.0 } else { 1.0 };
        let other = if selected == 2.0 { 1.0 } else { 2.0 };
        let getter = if name == "SetNifBlockTranslation" { "getnifblocktranslation" } else { "getnifblockrotation" };
        let baseline = if other == 1.0 { 30.0 } else { 10.0 };
        assert_eq!(array(&mut g, P, getter, &[text("SharedBone"), num(0.0), num(other)]),
            [num(baseline), num(0.0), num(0.0)], "{name} method={method:?} selector={selector:?}: other model changed");
        let changed = array(&mut g, P, getter, &[text("SharedBone"), num(0.0), num(selected)]);
        let before = if selected == 1.0 { 30.0 } else { 10.0 };
        assert_ne!(changed, [num(before), num(0.0), num(0.0)], "{name} method={method:?} selector={selector:?}: selected model unchanged");
        let edits = g.scene.take_edits();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits.into_iter().next().unwrap().into_model().0, view,
            "{name} method={method:?} selector={selector:?}: queue lost model identity");
    }
}

#[test]
fn source_vm_rotation_setter_and_alt_output_variables_keep_method_and_selector_separate() {
    let f = fixture();
    for method in [0, 1, 2] {
        for selector in [None, Some(1), Some(2)] {
            let mut g = game(&f);
            let suffix = selector.map_or(String::new(), |s| format!(" {s}"));
            let source = format!("float fX\nfloat fY\nfloat fZ\nfloat fOn\nBegin Function {{ }}\nPlayerREF.SetNifBlockRotation \"SharedBone\" 7 8 9 {method}{suffix}\nlet fOn := PlayerREF.GetNifBlockRotationAlt \"SharedBone\" fX fY fZ 0{suffix}\nSetFunctionValue (fOn * 1000 + fX + 10 * fY + 100 * fZ)\nEnd\n");
            let result = g.run_query(&source, RunCtx::default()).unwrap_or_else(|e| panic!("{source}: {e}"));
            let XVal::Num(result) = result else { panic!("expected output-variable checksum") };
            // This checks the existing local-set/read arithmetic and all three VM
            // outputs. It is not a claim about native method 1/2 rotation arithmetic.
            assert!((result - 1987.0).abs() < 0.01, "method={method} selector={selector:?}: {result}");
            let selected = if selector == Some(2) { ModelView::FirstPerson } else { ModelView::ThirdPerson };
            let other = if selected == ModelView::FirstPerson { 1.0 } else { 2.0 };
            let baseline = if other == 1.0 { 30.0 } else { 10.0 };
            assert_eq!(array(&mut g, P, "getnifblockrotation", &[text("SharedBone"), num(0.0), num(other)]), [num(baseline), num(0.0), num(0.0)]);
            let edits = g.scene.take_edits();
            assert_eq!(edits.len(), 1);
            assert_eq!(edits.into_iter().next().unwrap().into_model().0, selected);
        }
    }
}


fn identified_tree(root: &str, model: u64, bone: u64) -> Vec<SceneNode> {
    use fnv_game::nif_blocks::{NodeInfo, SceneIdentity};
    let mut nodes = tree(root, 10.0);
    for (i, node) in nodes.iter_mut().enumerate() {
        node.info = Some(Box::new(NodeInfo { identity: Some(SceneIdentity {
            model, node: if i == 0 { model } else { bone },
        }), ..Default::default() }));
    }
    nodes
}

#[test]
fn identified_queue_captures_published_node_before_replacement_and_preserves_legacy_api() {
    use fnv_game::nif_blocks::{SceneIdentity, SceneTarget};
    let f = fixture();
    let mut g = game(&f);
    g.scene.set_identified_tree_for(P, ModelView::FirstPerson, identified_tree("FirstRoot", 100, 101));
    call(&mut g, P, "setnifblockscale", &[text("SharedBone"), num(2.0), num(2.0)]);
    // Replacing publication cannot retarget an already queued edit.
    g.scene.set_identified_tree_for(P, ModelView::FirstPerson, identified_tree("FirstRoot", 200, 201));
    let queued = g.scene.take_targeted_edits();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].target, SceneTarget::Node(SceneIdentity { model: 100, node: 101 }));
    call(&mut g, P, "setnifblockscale", &[text("AbsentBone"), num(9.0), num(2.0)]);
    assert_eq!(g.scene.take_targeted_edits()[0].target, SceneTarget::Missing);
    // A legacy, headless model still exposes exactly the old edit payload.
    call(&mut g, NPC, "setnifblockscale", &[text("SharedBone"), num(7.0)]);
    let legacy = g.scene.take_targeted_edits();
    assert_eq!(legacy[0].target, SceneTarget::Legacy);
    call(&mut g, NPC, "setnifblockscale", &[text("SharedBone"), num(8.0)]);
    assert_eq!(g.scene.take_edits(), [SceneEdit::Scale { r: NPC, node: "SharedBone".into(), v: 8.0 }]);
}

#[test]
fn identified_structural_queue_keeps_renamed_removed_and_indexed_child_identity() {
    use fnv_game::nif_blocks::{SceneIdentity, SceneTarget};
    let f = fixture();
    let mut g = game(&f);
    g.scene.set_identified_tree_for(P, ModelView::FirstPerson, identified_tree("FirstRoot", 300, 301));
    call(&mut g, P, "renamenifblock", &[text("SharedBone"), text("FirstRoot"), num(2.0)]);
    // New name collides with the root; capture must precede the mutation.
    assert_eq!(g.scene.take_targeted_edits()[0].target,
        SceneTarget::Node(SceneIdentity { model: 300, node: 301 }));
    g.scene.set_identified_tree_for(P, ModelView::FirstPerson, identified_tree("FirstRoot", 400, 401));
    call(&mut g, P, "removenifblock", &[text("SharedBone"), num(2.0)]);
    assert_eq!(g.scene.take_targeted_edits()[0].target,
        SceneTarget::Node(SceneIdentity { model: 400, node: 401 }));
    // Public producer queue captures the indexed child, never just the parent.
    g.scene.set_identified_tree_for(P, ModelView::FirstPerson, identified_tree("FirstRoot", 500, 501));
    g.scene.push_edit(SceneEdit::ForModel { view: ModelView::FirstPerson, edit: Box::new(
        SceneEdit::ChildFlags { r: P, node: "FirstRoot".into(), child: 0, flags: 1 }) });
    assert_eq!(g.scene.take_targeted_edits()[0].target,
        SceneTarget::Node(SceneIdentity { model: 500, node: 501 }));
}
