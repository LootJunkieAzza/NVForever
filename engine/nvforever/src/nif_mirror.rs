//! The drawn nodes for JIP's NIF block functions (fnv_game::nif_blocks;
//! docs/design/nif-blocks.md): each frame the scripts' node changes are applied to the
//! entities of the reference they name, and the node tree of every reference a script
//! asked about is told back. A reference's root is its actor rig (each player model
//! separately, `PlayerBody::script_rig`), told as the skeleton's root node
//! (`Scene Root` [observed: the original's head parents end there, #2132]), else the
//! topmost entity drawing it. A shape's NiMaterialProperty values (JIP's Get/SetMaterialPropertyValue)
//! are read from the reference's model file, once per model, with the scripts' changes
//! over them; a change draws the shape with its own copy of the material. The vertices of
//! the shapes a script asked for (SUP's NifBlockGetVerticles) are told back each frame.
//! A created item drawn as flat shapes (`nvf_play::dropped`, a weapon on the ground) has
//! no node entities: its tree is told from its model file (names, parents, local
//! transforms), with the scripts' set transforms over it (#2132).

use crate::limbs::ActorRig;
use bevy::prelude::*;
use bevy::camera::primitives::Aabb;
use fnv_game::nif_blocks::{ModelView, NodeInfo, SceneEdit, SceneNode};
use fnv_sim::FormId;
use nvf_assets::{GameData, NifHidden, NifMaterial};
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;

pub struct NifMirrorPlugin;

impl Plugin for NifMirrorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, mirror.before(TransformSystems::Propagate));
    }
}

/// What the mirror keeps between frames: the references it logged, the held transforms,
/// the models' material values, the scripts' material values and the shapes drawn with
/// their own material since.
#[derive(Default)]
struct Memory {
    logged: std::collections::HashSet<u32>,
    held: HashMap<(u32, ModelView, String), Held>,
    tables: HashMap<String, HashMap<String, [f32; 9]>>,
    materials: HashMap<(u32, ModelView, String), [f32; 9]>,
    owned: HashMap<Entity, Handle<NifMaterial>>,
    /// Each model's shapes' NiAlphaProperty and NiStencilProperty values, by shape name.
    props: HashMap<String, HashMap<String, Props>>,
    /// Each model's node tree as its file holds it, for items drawn as flat shapes.
    files: HashMap<String, Vec<FileNode>>,
}

/// A node of a model file: its name, its parent's index, its local transform.
#[derive(Clone)]
struct FileNode {
    name: String,
    parent: Option<usize>,
    translation: Vec3,
    rotation: Quat,
    scale: f32,
}

/// A model file's nodes and shapes (under meshes), depth first from its root, children in
/// stored order, as `walk` tells drawn ones.
fn file_tree(data: &GameData, model: &str) -> Vec<FileNode> {
    let mut out = Vec::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    let objects = fnv_nif::scene::scene_objects(&file, &dec);
    let by_block: HashMap<usize, &fnv_nif::scene::SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let mut stack: Vec<(usize, Option<usize>)> = objects.iter().filter(|o| o.parent.is_none()).map(|o| (o.block, None)).rev().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some((b, parent)) = stack.pop() {
        let Some(o) = by_block.get(&b).filter(|_| seen.insert(b)) else { continue };
        let t = &o.transform;
        // The stored rows taken as columns: the angles the original reads of a file's node
        // [observed: GetNifBlockRotation of the 10mm pistol's ProjectileNode, 90 0 90, is
        // this matrix's; the transpose nvf_assets::nif_spawn draws with gives its inverse].
        let m = Mat3::from_cols_array(&t.rotation);
        let at = out.len();
        out.push(FileNode { name: o.name.clone().unwrap_or_default(), parent, translation: Vec3::from_array(t.translation), rotation: Quat::from_mat3(&m), scale: t.scale });
        stack.extend(o.children.iter().rev().map(|&c| (c, Some(at))));
    }
    out
}

/// A shape's alpha (flags, threshold) and stencil (flags, reference, mask) properties.
type Props = (Option<[f32; 2]>, Option<[f32; 3]>);

/// Each named shape's NiAlphaProperty and NiStencilProperty values in a model file (under
/// meshes), for JohnnyGuitar's Get…PropertyValue [documented: nif.xml's fields;
/// hypothesis: the functions' item order is the fields'].
fn property_table(data: &GameData, model: &str) -> HashMap<String, Props> {
    let mut out: HashMap<String, Props> = HashMap::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    for i in 0..file.blocks.len() {
        let Some(s) = dec.get(i as i32) else { continue };
        let Some(name) = s.text("name").filter(|n| !n.is_empty()) else { continue };
        for &r in s.refs("properties").unwrap_or_default() {
            let (Some(b), Some(p)) = (file.blocks.get(r as usize), dec.get(r)) else { continue };
            let int = |k: &str| p.int(k).map(|v| v as f32);
            let e = out.entry(name.to_string()).or_default();
            match b.type_name.as_str() {
                "NiAlphaProperty" => e.0 = Some([int("flags").unwrap_or(0.0), int("threshold").unwrap_or(0.0)]),
                "NiStencilProperty" => {
                    let reference = int("stencil_ref").or_else(|| int("stencil_reference")).unwrap_or(0.0);
                    let mask = int("stencil_mask").unwrap_or(0.0);
                    e.1 = Some([int("flags").unwrap_or(0.0), reference, mask]);
                }
                _ => {}
            }
        }
    }
    out
}

/// The world-space bounding sphere of `node` and what lies under it: of its shapes'
/// boxes, or the node's own place (radius 0) without any.
fn node_bound(at: usize, tree: &[(Entity, Option<usize>)], globals: &Query<&GlobalTransform>, boxes: &Query<&Aabb>) -> Option<[f32; 4]> {
    let mut spheres: Vec<(Vec3, f32)> = Vec::new();
    for (k, (e, _)) in tree.iter().enumerate() {
        // k under `at` (or `at` itself).
        let mut up = Some(k);
        let mut under = false;
        while let Some(i) = up {
            if i == at {
                under = true;
                break;
            }
            up = tree[i].1;
        }
        if !under {
            continue;
        }
        if let (Ok(b), Ok(g)) = (boxes.get(*e), globals.get(*e)) {
            let c = g.transform_point(Vec3::from(b.center));
            let s = g.compute_transform().scale.abs().max_element();
            spheres.push((c, Vec3::from(b.half_extents).length() * s));
        }
    }
    let own = globals.get(tree.get(at)?.0).ok()?.translation();
    let (centre, radius) = if spheres.is_empty() {
        (own, 0.0)
    } else {
        let (lo, hi) = spheres.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), (c, r)| (lo.min(*c - *r), hi.max(*c + *r)));
        let mid = (lo + hi) / 2.0;
        (mid, spheres.iter().map(|(c, r)| c.distance(mid) + r).fold(0.0, f32::max))
    };
    let g = nvf_play::space::to_game(centre);
    Some([g[0], g[1], g[2], radius])
}

/// What the mirror reads beside the nodes: their places in the world and boxes (bounds),
/// and the particle systems.
type Extra<'w, 's> = (Query<'w, 's, &'static GlobalTransform>, Query<'w, 's, &'static Aabb>, Option<ResMut<'w, crate::particles::Particles>>);

/// Each named shape's NiMaterialProperty values in a model file (under meshes), in
/// GetMaterialPropertyValue's order: specular R G B, emissive R G B, glossiness, alpha,
/// emissive mult [documented: the page's value IDs; the fields: nif.xml
/// NiMaterialProperty]. The emissive mult is 1 in files without it.
fn material_table(data: &GameData, model: &str) -> HashMap<String, [f32; 9]> {
    let mut out = HashMap::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    for i in 0..file.blocks.len() {
        let Some(s) = dec.get(i as i32) else { continue };
        let Some(name) = s.text("name").filter(|n| !n.is_empty()) else { continue };
        for &r in s.refs("properties").unwrap_or_default() {
            if !file.blocks.get(r as usize).is_some_and(|b| b.type_name == "NiMaterialProperty") {
                continue;
            }
            let Some(m) = dec.get(r) else { continue };
            let c = |f: &str| m.floats(f).filter(|v| v.len() >= 3).map_or([0.0; 3], |v| [v[0], v[1], v[2]]);
            let (sp, em) = (c("specular_color"), c("emissive_color"));
            let v = [sp[0], sp[1], sp[2], em[0], em[1], em[2], m.float("glossiness").unwrap_or(0.0), m.float("alpha").unwrap_or(1.0), m.float("emissive_mult").unwrap_or(1.0)];
            out.entry(name.to_string()).or_insert(v);
        }
    }
    out
}

/// A shape's material drawn with these values: the specular tint, the emitted light
/// (colour times mult), the roughness of the glossiness, the alpha [hypothesis: the
/// spawn's own display tuning of the emitted light is not redone here].
fn set_material(m: &mut NifMaterial, v: &[f32; 9]) {
    m.base.specular_tint = Color::linear_rgb(v[0], v[1], v[2]);
    m.base.emissive = LinearRgba::rgb(v[3] * v[8], v[4] * v[8], v[5] * v[8]);
    m.base.perceptual_roughness = nvf_assets::nif_spawn::gloss_roughness(v[6]);
    m.base.base_color.set_alpha(v[7].clamp(0.0, 1.0));
}

/// A node transform a script set, held over the animation.
#[derive(Clone, Copy, Default)]
struct Held {
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<f32>,
}

type Visuals<'w, 's> = Query<'w, 's, (Entity, &'static RefVisual, Option<&'static ChildOf>), crate::lights::NotLightSource>;

/// Distinguish an explicit NIF rename from factory entity labels on actor roots.
/// The marker dies with the entity; it is not a FormId/name override for a new rig.
#[derive(Component)]
struct RenamedNifNode;

type NodeQuery<'w, 's> = Query<'w, 's, (Option<&'static Name>, &'static mut Transform, Option<&'static Children>, Has<NifHidden>, Has<RenamedNifNode>)>;

/// The actors' rigs and both built player models.
type Rigs<'w, 's> = (Query<'w, 's, (Entity, &'static ActorRig)>, Option<Res<'w, crate::player_body::PlayerBody>>);

fn concrete_view(r: u32, view: ModelView) -> ModelView {
    if r != FormId::PLAYER_REF.0 { ModelView::Reference }
    else if view == ModelView::Reference { ModelView::ThirdPerson }
    else { view }
}

fn selected_player_rig<T>(view: ModelView, third: impl FnOnce() -> Option<T>, first: impl FnOnce() -> Option<T>) -> Option<T> {
    if view == ModelView::FirstPerson { first() } else { third() }
}

/// A reference's rig: its root entity and the rig.
fn rig_of(r: u32, view: ModelView, rigs: &Rigs) -> Option<(Entity, ActorRig)> {
    let (actors, player) = rigs;
    if r == FormId::PLAYER_REF.0 {
        // Player callers resolve the current-view sentinel before selecting a rig.
        if view == ModelView::Reference { return None; }
        return player.as_deref().and_then(|p| selected_player_rig(view, || p.script_rig(false), || p.script_rig(true)));
    }
    actors.iter().find(|(_, a)| a.reference == r).map(|(e, a)| (e, a.clone()))
}

/// The root entity of a reference's drawing.
fn root_of(r: u32, view: ModelView, rigs: &Rigs, visuals: &Visuals) -> Option<Entity> {
    if let Some((e, _)) = rig_of(r, view, rigs) {
        return Some(e);
    }
    if r == FormId::PLAYER_REF.0 { return None; }
    visuals.iter().filter(|(_, v, _)| v.0 == r).find(|(_, _, p)| p.is_none_or(|p| visuals.get(p.parent()).map_or(true, |(_, v, _)| v.0 != r))).map(|(e, _, _)| e)
}

/// The node entities under `root`, depth first, with their parent's index; an actor's
/// root first (its skeleton's root node), the rest under it.
fn walk(root: Entity, nodes: &NodeQuery, rig: &RigNames) -> Vec<(Entity, Option<usize>)> {
    let mut out = Vec::new();
    let mut stack: Vec<(Entity, Option<usize>)> = Vec::new();
    let top = if rig.root == Some(root) {
        out.push((root, None));
        Some(0)
    } else {
        None
    };
    if let Ok((_, _, Some(ch), _, _)) = nodes.get(root) {
        stack.extend(ch.iter().rev().map(|c| (c, top)));
    }
    while let Some((e, parent)) = stack.pop() {
        let at = out.len();
        out.push((e, parent));
        if let Ok((_, _, Some(ch), _, _)) = nodes.get(e) {
            stack.extend(ch.iter().rev().map(|c| (c, Some(at))));
        }
    }
    out
}

/// An actor's skeleton node names, by entity (its node entities carry no `Name`), and its
/// root entity (named by the skeleton, not by the `Name` the spawn gives it).
#[derive(Default)]
struct RigNames {
    root: Option<Entity>,
    names: HashMap<Entity, String>,
}

fn rig_names(r: u32, view: ModelView, rigs: &Rigs) -> RigNames {
    rig_of(r, view, rigs)
        .map(|(root, a)| RigNames { root: Some(root), names: a.nodes.iter().zip(&a.skel.names).filter_map(|(e, n)| n.clone().map(|n| (*e, n))).collect() })
        .unwrap_or_default()
}

/// A node's name: an actor root's from its skeleton, else its `Name`, else the rig's.
fn name_of(e: Entity, nodes: &NodeQuery, rig: &RigNames) -> String {
    let node = nodes.get(e).ok();
    if let Some(name) = node.as_ref().filter(|n| n.4).and_then(|n| n.0) {
        return name.as_str().to_string();
    }
    if rig.root == Some(e) {
        if let Some(n) = rig.names.get(&e) {
            return n.clone();
        }
    }
    node.and_then(|n| n.0.map(|n| n.as_str().to_string())).or_else(|| rig.names.get(&e).cloned()).unwrap_or_default()
}

/// The node of that name under `root`: exactly, else without regard to case.
fn find(root: Entity, name: &str, nodes: &NodeQuery, rig: &RigNames) -> Option<Entity> {
    let all = walk(root, nodes, rig);
    all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig) == name).or_else(|| all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig).eq_ignore_ascii_case(name)))
}

type HeldModels = HashMap<(u32, ModelView, String), Held>;

fn remember_transform(view: ModelView, edit: &SceneEdit, held: &mut HeldModels) -> bool {
    match edit {
        SceneEdit::Translation { r, node, v } => {
            held.entry((r.0, view, node.clone())).or_default().translation = Some(Vec3::from(*v));
        }
        SceneEdit::Rotation { r, node, v } => {
            let [x, y, z] = v.map(f32::to_radians);
            held.entry((r.0, view, node.clone())).or_default().rotation = Some(Quat::from_euler(EulerRot::ZYX, z, y, x));
        }
        SceneEdit::Scale { r, node, v } => {
            held.entry((r.0, view, node.clone())).or_default().scale = Some(*v);
        }
        _ => return false,
    }
    true
}

fn apply_held(root: Entity, node: &str, held: Held, nodes: &mut NodeQuery, rig: &RigNames) {
    if let Some(mut t) = find(root, node, nodes, rig).and_then(|e| nodes.get_mut(e).ok()) {
        if let Some(v) = held.translation { t.1.translation = v; }
        if let Some(q) = held.rotation { t.1.rotation = q; }
        if let Some(s) = held.scale { t.1.scale = Vec3::splat(s); }
    }
}

/// The material side of [`mirror`]: the data files, the shapes' materials, the assets.
type Materials<'w, 's> = (Option<Res<'w, GameData>>, Query<'w, 's, &'static MeshMaterial3d<NifMaterial>>, Option<ResMut<'w, Assets<NifMaterial>>>);

/// The roots of created items (`nvf_play::dropped`) and the entities with children: an
/// item whose shapes hang straight from its root is drawn flat.
type Dropped<'w, 's> = (Query<'w, 's, (&'static nvf_play::dropped::DroppedRoot, Option<&'static nvf_play::dropped::DroppedModel>, Option<&'static Children>)>, Query<'w, 's, (), With<Children>>);

/// The model of `r` when it is a created item drawn as flat shapes (no node entities).
fn drawn_flat(r: u32, (roots, nested): &Dropped) -> Option<String> {
    roots.iter().find(|(d, _, ch)| d.0 == r && ch.is_some_and(|c| c.iter().all(|e| !nested.contains(e)))).map(|(_, m, _)| {
        // The drop's path is under meshes already; the tables here take it without.
        let m = m.map(|m| m.0.as_str()).unwrap_or_default();
        m.get(..7).filter(|p| p.eq_ignore_ascii_case("meshes\\")).map_or(m, |_| &m[7..]).to_string()
    })
}

/// The shapes' meshes, for SUP's NifBlockGetVerticles.
type Shapes<'w, 's> = (Query<'w, 's, &'static Mesh3d>, Option<Res<'w, Assets<Mesh>>>);

#[allow(clippy::too_many_arguments)]
fn mirror(
    mut script: Option<ResMut<ScriptState>>,
    rigs: Rigs,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    nodes: NodeQuery,
    commands: Commands,
    mut mem: Local<Memory>,
    materials: Materials,
    shapes: Shapes,
    extra: Extra,
    dropped: Dropped,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    mirror_game(&mut rt.game, rigs, visuals, parents, nodes, commands, &mut mem, materials, shapes, extra, dropped);
}

/// The real mirror producer, separated from ScriptState acquisition so authored
/// CPU controls can exercise the PlayerBody-to-script-to-ECS pipeline without a
/// game-data loader or renderer. This is also the production mirror's implementation.
#[allow(clippy::too_many_arguments)]
fn mirror_game(
    game: &mut fnv_game::game::Game<'_>,
    rigs: Rigs,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    mut nodes: NodeQuery,
    mut commands: Commands,
    mem: &mut Memory,
    (data, drawn, mut assets): Materials,
    (shapes, meshes): Shapes,
    (globals, boxes, mut particles): Extra,
    dropped: Dropped,
) {
    let Memory { logged, held, tables, materials, owned, props, files } = mem;
    game.scene.use_separate_player_models();
    let edits = game.scene.take_edits();
    let mut edits_rest = Vec::new();
    for edit in edits {
        let (view, edit) = edit.into_model();
        let r = match &edit {
            SceneEdit::Translation { r, .. } | SceneEdit::Rotation { r, .. } | SceneEdit::Scale { r, .. }
            | SceneEdit::Flags { r, .. } | SceneEdit::ChildFlags { r, .. } | SceneEdit::Rename { r, .. }
            | SceneEdit::Remove { r, .. } | SceneEdit::Insert { r, .. } | SceneEdit::Attach { r, .. }
            | SceneEdit::Material { r, .. } | SceneEdit::Particle { r, .. } => *r,
            SceneEdit::ForModel { .. } => continue,
        };
        let view = concrete_view(r.0, view);
        // A transform a script sets holds over the animation, frame after frame
        // [observed: set-then-read stays exact on an animated bone]; kept for a model
        // drawn flat too, whose tree is its file's.
        if !remember_transform(view, &edit, held) { edits_rest.push((view, edit)); }
    }
    for (view, edit) in edits_rest {
        let r = match &edit {
            SceneEdit::Translation { r, .. } | SceneEdit::Rotation { r, .. } | SceneEdit::Scale { r, .. } | SceneEdit::Flags { r, .. } | SceneEdit::ChildFlags { r, .. } | SceneEdit::Rename { r, .. } | SceneEdit::Remove { r, .. } | SceneEdit::Insert { r, .. } | SceneEdit::Attach { r, .. } | SceneEdit::Material { r, .. } | SceneEdit::Particle { r, .. } => *r,
            SceneEdit::ForModel { .. } => continue,
        };
        let Some(root) = root_of(r.0, view, &rigs, &visuals) else { continue };
        let rig = rig_names(r.0, view, &rigs);
        match edit {
            SceneEdit::ForModel { .. } => {}
            SceneEdit::Translation { .. } | SceneEdit::Rotation { .. } | SceneEdit::Scale { .. } => {}
            SceneEdit::Flags { node, flags, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    if flags & 1 != 0 {
                        commands.entity(e).insert((NifHidden, Visibility::Hidden));
                    } else {
                        commands.entity(e).remove::<NifHidden>().insert(Visibility::Inherited);
                    }
                }
            }
            // A child by its place under the node, in the order `walk` told the mirror
            // (a switch's children, #2017).
            SceneEdit::ChildFlags { node, child, flags, .. } => {
                let kid = find(root, &node, &nodes, &rig).and_then(|p| nodes.get(p).ok().and_then(|(_, _, ch, _, _)| ch.and_then(|c| c.iter().nth(child))));
                if let Some(e) = kid {
                    if flags & 1 != 0 {
                        commands.entity(e).insert((NifHidden, Visibility::Hidden));
                    } else {
                        commands.entity(e).remove::<NifHidden>().insert(Visibility::Inherited);
                    }
                }
            }
            SceneEdit::Rename { node, to, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).insert((Name::new(to.clone()), RenamedNifNode));
                    if let Some(h) = held.remove(&(r.0, view, node)) {
                        held.insert((r.0, view, to), h);
                    }
                }
            }
            SceneEdit::Remove { node, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).despawn();
                }
                held.remove(&(r.0, view, node));
            }
            SceneEdit::Insert { target, name, as_parent, .. } => {
                let Some(t) = find(root, &target, &nodes, &rig) else { continue };
                let new = commands.spawn((Name::new(name), Transform::IDENTITY, Visibility::Inherited)).id();
                if as_parent {
                    let above = parents.get(t).map(|p| p.parent()).unwrap_or(root);
                    commands.entity(above).add_child(new);
                    commands.entity(new).add_child(t);
                } else {
                    commands.entity(t).add_child(new);
                }
            }
            // The model is not drawn yet [hypothesis: slice 2 spawns it through
            // nvf_assets::SpawnCtx]; the mirror holds the request only.
            SceneEdit::Attach { .. } => {}
            // A placed model's particle system of that name (or emitting from a node of
            // that name), changed while it lives (JohnnyGuitar's SetNiPSysModifierValue).
            SceneEdit::Particle { node, id, v, .. } => {
                if let Some(p) = particles.as_deref_mut() {
                    for d in crate::particles::placed_systems(p, r.0) {
                        if d.name.eq_ignore_ascii_case(&node) || d.emitter.object.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&node)) {
                            crate::particles::set_script_value(d, id, v);
                        }
                    }
                }
            }
            // The mirror has the value already; the shape is drawn with its own copy of
            // the material from now on [hypothesis: the original's change is this
            // reference's; the page's note puts an inventory reference's on the base].
            SceneEdit::Material { node, id, v, .. } => {
                // The player's base MODL is not an owned first-person shape source.
                // Refuse this unqualified path instead of borrowing third-person data.
                if view == ModelView::FirstPerson { continue; }
                let Some(d) = data.as_deref() else { continue };
                let model = fnv_game::nif_blocks::model_of(game, r).unwrap_or_default();
                let table = tables.entry(model.clone()).or_insert_with(|| material_table(d, &model));
                let base = node.split(':').next().unwrap_or(&node).to_string();
                let Some(&start) = table.get(&node).or_else(|| table.get(&base)) else { continue };
                let vals = materials.entry((r.0, view, node.clone())).or_insert(start);
                vals[id] = v;
                let vals = *vals;
                let (Some(e), Some(assets)) = (find(root, &node, &nodes, &rig), assets.as_deref_mut()) else { continue };
                let Ok(handle) = drawn.get(e) else { continue };
                match owned.get(&e).filter(|h| *h == &handle.0).cloned() {
                    Some(h) => {
                        if let Some(mut m) = assets.get_mut(&h) {
                            set_material(&mut m, &vals);
                        }
                    }
                    None => {
                        let Some(mut m) = assets.get(&handle.0).cloned() else { continue };
                        set_material(&mut m, &vals);
                        let h = assets.add(m);
                        owned.insert(e, h.clone());
                        commands.entity(e).insert(MeshMaterial3d(h));
                    }
                }
            }
        }
    }
    // The held transforms, over this frame's pose.
    let keys: Vec<(u32, ModelView, String)> = held.keys().cloned().collect();
    for (r, view, node) in keys {
        let Some(root) = root_of(r, view, &rigs, &visuals) else { continue };
        let rig = rig_names(r, view, &rigs);
        let h = held[&(r, view, node.clone())];
        apply_held(root, &node, h, &mut nodes, &rig);
    }
    // The trees of the references scripts asked about.
    // Once scripts use the NIF functions, the player's tree is told too, watched or not:
    // the original has the player's 3D at once, so a script's first read finds it
    // [observed: player.GetChildBlocks in the nif-blocks set, #2132].
    let mut watched: Vec<FormId> = game.scene.watched().collect();
    // So are created items drawn flat, whose 3D the original has a frame after
    // PlaceAtMe (the nif-blocks set's first read of the pistol finds it).
    if !watched.is_empty() {
        for r in std::iter::once(FormId::PLAYER_REF).chain(dropped.0.iter().map(|(d, _, _)| FormId(d.0))) {
            if !watched.contains(&r) {
                watched.push(r);
            }
        }
    }
    for r in watched {
        if let Some(model) = drawn_flat(r.0, &dropped) {
            let Some(d) = data.as_deref() else { continue };
            let file = files.entry(model.clone()).or_insert_with(|| file_tree(d, &model));
            let table = tables.entry(model.clone()).or_insert_with(|| material_table(d, &model));
            let mut seen: HashMap<String, u32> = HashMap::new();
            let tree: Vec<SceneNode> = file
                .iter()
                .map(|f| {
                    let n = seen.entry(f.name.to_ascii_lowercase()).or_insert(0);
                    let shown = if *n == 0 { f.name.clone() } else { format!("{}:{}", f.name, *n - 1) };
                    *n += 1;
                    let h = held.get(&(r.0, ModelView::Reference, shown.clone())).copied().unwrap_or_default();
                    let (z, y, x) = h.rotation.unwrap_or(f.rotation).to_euler(EulerRot::ZYX);
                    let material = materials.get(&(r.0, ModelView::Reference, shown.clone())).or_else(|| table.get(&f.name)).copied();
                    SceneNode {
                        name: shown,
                        parent: f.parent,
                        translation: h.translation.unwrap_or(f.translation).to_array(),
                        rotation: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                        scale: h.scale.unwrap_or(f.scale),
                        flags: 0,
                        material,
                        info: None,
                    }
                })
                .collect();
            if logged.insert(r.0 ^ 0x4000_0000) {
                info!("nif mirror: {:08X} told from its model file {model:?} (drawn flat), {} nodes, first {:?}", r.0, tree.len(), tree.iter().take(4).map(|n| n.name.as_str()).collect::<Vec<_>>());
            }
            game.scene.set_tree(r, tree);
            continue;
        }
        let views: &[ModelView] = if r == FormId::PLAYER_REF {
            &[ModelView::ThirdPerson, ModelView::FirstPerson]
        } else { &[ModelView::Reference] };
        for &view in views {
            let Some(root) = root_of(r.0, view, &rigs, &visuals) else {
                if logged.insert(r.0) {
                    info!("nif mirror: {:08X} watched, not drawn", r.0);
                }
                game.scene.forget_model(r, view);
                continue;
            };
            let rig = rig_names(r.0, view, &rigs);
            let mut seen: HashMap<String, u32> = HashMap::new();
            // Per-view cache keys do not establish per-view file ownership. Until
            // the first-person model's actual shape source is supplied, metadata
            // from MODL/placed systems of the reference is unavailable for that rig.
            let model = if view == ModelView::FirstPerson { None }
                else { fnv_game::nif_blocks::model_of(game, r) };
            let table = match (data.as_deref(), model.as_deref()) {
                (Some(d), Some(model)) => tables.entry(model.to_string()).or_insert_with(|| material_table(d, model)).clone(),
                _ => HashMap::new(),
            };
            let props = match (data.as_deref(), model.as_deref()) {
                (Some(d), Some(model)) => props.entry(model.to_string()).or_insert_with(|| property_table(d, model)).clone(),
                _ => HashMap::new(),
            };
            let systems: Vec<(String, Option<String>, [f32; 21])> = if view == ModelView::FirstPerson { Vec::new() } else { particles
                .as_deref_mut()
                .map(|p| crate::particles::placed_systems(p, r.0).into_iter().map(|d| (d.name.clone(), d.emitter.object.clone(), crate::particles::script_values(d))).collect())
                .unwrap_or_default() };
            let walked = walk(root, &nodes, &rig);
            let tree: Vec<SceneNode> = walked
                .iter()
                .enumerate()
                .map(|(k, &(e, parent))| (k, e, parent))
                .filter_map(|(k, e, parent)| {
                    let base = name_of(e, &nodes, &rig);
                    let (_, t, _, hidden, _) = nodes.get(e).ok()?;
                    // A name seen before gets ":0", ":1" … [observed: "Object", "Object:0"].
                    let n = seen.entry(base.to_ascii_lowercase()).or_insert(0);
                    let shown = if *n == 0 { base.clone() } else { format!("{base}:{}", *n - 1) };
                    *n += 1;
                    let (z, y, x) = t.rotation.to_euler(EulerRot::ZYX);
                    let material = if view == ModelView::FirstPerson { None }
                        else { materials.get(&(r.0, view, shown.clone())).or_else(|| table.get(&base)).copied() };
                    let (alpha, stencil) = props.get(&base).copied().unwrap_or_default();
                    let emitter = systems.iter().find(|(n, o, _)| n.eq_ignore_ascii_case(&base) || o.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&base))).map(|s| s.2);
                    let info = NodeInfo { bound: node_bound(k, &walked, &globals, &boxes), emitter, alpha, stencil };
                    Some(SceneNode {
                        name: shown,
                        parent,
                        translation: t.translation.to_array(),
                        rotation: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                        scale: t.scale.x,
                        flags: u32::from(hidden),
                        material,
                        info: Some(Box::new(info)),
                    })
                })
                .collect();
            if logged.insert(r.0 | 0x8000_0000) {
                info!("nif mirror: {:08X} told, {} nodes, first {:?}", r.0, tree.len(), tree.iter().take(4).map(|n| n.name.as_str()).collect::<Vec<_>>());
            }
            game.scene.set_tree_for(r, view, tree);
        }
    }
    // The vertices of the shapes scripts asked about (SUP's NifBlockGetVerticles), in the
    // shape's own space as its mesh holds them; none for a mesh kept on the GPU only
    // [hypothesis: a skinned shape's are its bind pose].
    let wanted: Vec<(FormId, String)> = game.scene.wanted_vertices().cloned().collect();
    for (r, node) in wanted {
        let view = concrete_view(r.0, ModelView::Reference);
        let verts = root_of(r.0, view, &rigs, &visuals)
            .and_then(|root| find(root, &node, &nodes, &rig_names(r.0, view, &rigs)))
            .and_then(|e| shapes.get(e).ok())
            .and_then(|m| meshes.as_deref()?.get(&m.0))
            // `try_`: a mesh extracted to the render world gives none, where `attribute` panics (#2202).
            .and_then(|mesh| match mesh.try_attribute_option(Mesh::ATTRIBUTE_POSITION).ok()?? {
                bevy::mesh::VertexAttributeValues::Float32x3(v) => Some(v.clone()),
                _ => None,
            });
        game.scene.set_vertices(r, &node, verts);
    }
}


#[cfg(test)]
#[path = "nif_mirror/player_models_tests.rs"]
mod player_models_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn a_light_source_is_not_a_references_root() {
        const R: u32 = 0x301;
        fn root(world: &mut World) -> Option<Entity> {
            world.run_system_once(|rigs: Rigs, visuals: Visuals| root_of(R, ModelView::Reference, &rigs, &visuals)).unwrap()
        }
        // Without a parent, so that each entity passes the test on its parent and only the
        // filter on light sources can leave it out.
        let mut world = World::new();
        world.spawn((PointLight::default(), RefVisual(R)));
        world.spawn((SpotLight::default(), RefVisual(R)));
        assert_eq!(root(&mut world), None, "its light sources alone");
        let drawn = world.spawn((Transform::IDENTITY, RefVisual(R))).id();
        assert_eq!(root(&mut world), Some(drawn), "not a light source beside it");
    }

    #[test]
    fn explicitly_selected_player_root_does_not_fall_back_to_another_rig() {
        let mut world = World::new();
        let third = world.spawn(Transform::IDENTITY).id();
        let first = world.spawn(Transform::IDENTITY).id();
        assert_eq!(selected_player_rig(ModelView::ThirdPerson, || Some(third), || Some(first)), Some(third));
        assert_eq!(selected_player_rig(ModelView::FirstPerson, || Some(third), || Some(first)), Some(first));
        assert_eq!(selected_player_rig(ModelView::FirstPerson, || Some(third), || None::<Entity>), None);
        assert_eq!(selected_player_rig(ModelView::ThirdPerson, || None::<Entity>, || Some(first)), None);
    }

    #[test]
    fn legacy_default_is_third_person_and_explicit_selection_stays_concrete() {
        assert_eq!(concrete_view(FormId::PLAYER_REF.0, ModelView::Reference), ModelView::ThirdPerson);
        assert_eq!(concrete_view(FormId::PLAYER_REF.0, ModelView::FirstPerson), ModelView::FirstPerson);
        assert_eq!(concrete_view(0x301, ModelView::FirstPerson), ModelView::Reference);
    }

    #[test]
    fn tagged_held_edits_use_each_rigs_names_and_transform_entities() {
        let mut world = World::new();
        let third = world.spawn(Transform::IDENTITY).id();
        let first = world.spawn(Transform::IDENTITY).id();
        // Skeleton entities have no Name. Both rigs reuse the same authored bone
        // name; choosing the right rig's skeleton map matters as much as its root.
        let third_bone = world.spawn((Transform::from_xyz(30.0, 0.0, 0.0), ChildOf(third))).id();
        let first_bone = world.spawn((Transform::from_xyz(10.0, 0.0, 0.0), ChildOf(first))).id();
        let third_names = RigNames { root: Some(third), names: [(third, "ThirdRoot".into()), (third_bone, "SharedBone".into())].into() };
        let first_names = RigNames { root: Some(first), names: [(first, "FirstRoot".into()), (first_bone, "SharedBone".into())].into() };
        let mut held = HeldModels::new();
        for (view, edit) in [
            (ModelView::FirstPerson, SceneEdit::Translation { r: FormId::PLAYER_REF, node: "SharedBone".into(), v: [1.0, 2.0, 3.0] }),
            (ModelView::FirstPerson, SceneEdit::Scale { r: FormId::PLAYER_REF, node: "SharedBone".into(), v: 2.0 }),
            (ModelView::ThirdPerson, SceneEdit::Scale { r: FormId::PLAYER_REF, node: "SharedBone".into(), v: 3.0 }),
        ] {
            let (view, edit) = SceneEdit::ForModel { view, edit: Box::new(edit) }.into_model();
            assert!(remember_transform(view, &edit, &mut held));
        }
        assert_eq!(held.len(), 2, "one held entry per concrete player model");
        world.run_system_once(move |mut nodes: NodeQuery| {
            for view in [ModelView::ThirdPerson, ModelView::FirstPerson] {
                let (root, names) = selected_player_rig(view, || Some((third, &third_names)), || Some((first, &first_names))).unwrap();
                assert_eq!(find(root, "SharedBone", &nodes, names), Some(if view == ModelView::FirstPerson { first_bone } else { third_bone }));
                apply_held(root, "SharedBone", held[&(FormId::PLAYER_REF.0, view, "SharedBone".into())], &mut nodes, names);
            }
        }).unwrap();
        assert_eq!(world.get::<Transform>(first_bone).unwrap().translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(world.get::<Transform>(third_bone).unwrap().translation, Vec3::new(30.0, 0.0, 0.0));
        assert_eq!(world.get::<Transform>(first_bone).unwrap().scale, Vec3::splat(2.0));
        assert_eq!(world.get::<Transform>(third_bone).unwrap().scale, Vec3::splat(3.0));
    }
}
