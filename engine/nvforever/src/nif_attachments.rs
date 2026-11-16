//! Per-reference AttachModel producer. See docs/spec-notes/nif-attachments.md.
use crate::nif_mirror::{
    bound_entity, concrete_view, find, rig_names, root_of, NodeQuery, Rigs, Visuals,
};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fnv_game::nif_blocks::{AttachmentChange, ModelView, SceneTarget};
use nvf_assets::{
    model::{ModelCtx, ModelObject, PreparedModel},
    NifHidden,
};
use nvf_play::script::ScriptState;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(crate) const ANCESTOR_LIMIT: usize = 256;

#[derive(Component, Clone)]
pub(crate) struct AttachedNode {
    pub source: ModelObject,
    pub material: Option<[f32; 9]>,
}
#[derive(Component)]
pub(crate) struct Member(u64);

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    reference: u32,
    view: ModelView,
    target: String,
    model: String,
}
struct Intent {
    change: AttachmentChange,
    root: Option<Entity>,
    parent: Option<Entity>,
    blocked: bool,
    deferred: Option<Entity>,
    failed: bool,
    produced: bool,
    stale: bool,
}
struct Instance {
    id: u64,
    root: Entity,
    parent: Entity,
    entities: Vec<Entity>,
}
#[derive(Resource, Default)]
pub(crate) struct State {
    session: Option<Arc<()>>,
    intents: HashMap<Key, Intent>,
    order: Vec<Key>,
    instances: HashMap<Key, Instance>,
    next: u64,
    removed: HashSet<Entity>,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<State>().add_observer(retired);
}
// An ordinary observer can be installed after Transform was registered. No global
// component hook, Added-tick heuristic, or poll-only lifetime detection is used.
fn retired(event: On<Remove<Transform>>, children: Query<&Children>, mut state: ResMut<State>) {
    let mut pending = vec![event.entity];
    while let Some(entity) = pending.pop() {
        if !state.removed.insert(entity) {
            continue;
        }
        if let Ok(children) = children.get(entity) {
            pending.extend(children.iter());
        }
    }
}

pub(crate) type Owned<'w, 's> = Query<'w, 's, (&'static Member, Option<&'static Children>)>;

fn clean(instances: &[Instance], commands: &mut Commands, owned: &Owned) {
    let ours: HashSet<_> = instances
        .iter()
        .flat_map(|instance| {
            instance
                .entities
                .iter()
                .copied()
                .filter(move |&e| owned.get(e).is_ok_and(|(m, _)| m.0 == instance.id))
        })
        .collect();
    // Detach every foreign child before the first recursive despawn. Doing this
    // per instance would let a retiring outer instance erase a nested one's guest.
    for &e in &ours {
        if let Ok((_, Some(children))) = owned.get(e) {
            for child in children.iter().filter(|c| !ours.contains(c)) {
                commands.entity(child).try_remove::<ChildOf>();
            }
        }
    }
    for instance in instances {
        for &e in instance.entities.iter().rev().filter(|e| ours.contains(e)) {
            commands.entity(e).try_despawn();
        }
    }
}

fn intact(
    root: Entity,
    mut child: Entity,
    parents: &Query<&ChildOf>,
    removed: &HashSet<Entity>,
) -> bool {
    for _ in 0..ANCESTOR_LIMIT {
        if removed.contains(&child) {
            return false;
        }
        if child == root {
            return true;
        }
        let Ok(parent) = parents.get(child) else {
            return false;
        };
        child = parent.parent();
    }
    false
}

// Compute teardown to a fixed point before publication: a second owned instance
// attached below a retiring one must not survive as an orphan for one frame.
fn retire_instances(
    state: &mut State,
    retiring: &mut Vec<Instance>,
    owned: &Owned,
    rigs: &Rigs,
    visuals: &Visuals,
    nodes: &NodeQuery,
    parents: &Query<&ChildOf>,
) {
    loop {
        let gone: Vec<_> = state
            .instances
            .iter()
            .filter_map(|(key, instance)| {
                let alive = root_of(key.reference, key.view, rigs, visuals) == Some(instance.root)
                    && nodes.get(instance.root).is_ok()
                    && nodes.get(instance.parent).is_ok()
                    && intact(instance.root, instance.parent, parents, &state.removed)
                    && instance.entities.iter().all(|e| {
                        nodes.get(*e).is_ok()
                            && owned.get(*e).is_ok_and(|(m, _)| m.0 == instance.id)
                            && intact(instance.parent, *e, parents, &state.removed)
                    });
                (!alive).then(|| key.clone())
            })
            .collect();
        if gone.is_empty() {
            break;
        }
        for key in gone {
            let instance = state
                .instances
                .remove(&key)
                .expect("examined owned instance");
            state.removed.extend(instance.entities.iter().copied());
            if let Some(intent) = state.intents.get_mut(&key) {
                intent.root = Some(instance.root);
                intent.parent = None;
                intent.blocked = true;
            }
            retiring.push(instance);
        }
    }
}

/// The optional `*suffix*` prefix is a label transform, never an object identity.
fn model_path(value: &str) -> Option<(String, String)> {
    let (suffix, path) = if let Some(rest) = value.strip_prefix('*') {
        let (suffix, path) = rest.split_once('*')?;
        (suffix.to_string(), path)
    } else {
        (String::new(), value)
    };
    if path.is_empty()
        || path.contains(':')
        || path.starts_with(['/', '\\'])
        || path.split(['/', '\\']).any(|p| p == "..")
    {
        return None;
    }
    let path = fnv_vfs::normalize(path);
    if !path.ends_with(".nif") {
        return None;
    }
    Some((suffix, format!("meshes/{path}")))
}

/// Validate the entire correspondence before even the first entity is reserved.
pub(crate) fn valid(model: &PreparedModel) -> bool {
    let objects = &model.objects;
    if objects.is_empty() {
        return false;
    }
    let mut blocks = HashSet::new();
    let mut shapes = HashSet::new();
    let mut rig_nodes = HashSet::new();
    let mut depths = Vec::with_capacity(objects.len());
    for (i, o) in objects.iter().enumerate() {
        if !blocks.insert(o.block)
            || o.parent.is_some_and(|p| p >= i)
            || !o.local.translation.is_finite()
            || !o.local.rotation.is_finite()
            || !o.local.scale.is_finite()
        {
            return false;
        }
        let depth = o.parent.map_or(1, |p| depths[p] + 1);
        if depth >= ANCESTOR_LIMIT {
            return false;
        }
        depths.push(depth);
        if let Some(shape) = o.shape {
            if shape >= model.shapes.len()
                || !shapes.insert(shape)
                || model.shapes[shape].name != o.name
            {
                return false;
            }
        }
        if let Some(rig) = &model.rig {
            let Some(n) = o
                .rig_node
                .filter(|n| *n < rig.nodes.len() && rig_nodes.insert(*n))
            else {
                return false;
            };
            if rig.nodes[n].parent != o.parent.and_then(|p| objects[p].rig_node)
                || rig.nodes[n].name != o.name
            {
                return false;
            }
        }
    }
    if shapes.len() != model.shapes.len() {
        return false;
    }
    model.rig.as_ref().is_none_or(|rig| {
        let mut drawn = HashSet::new();
        rig.nodes.len() == objects.len()
            && objects.iter().filter(|o| o.parent.is_none()).count() == 1
            && objects[0].rig_node == Some(0)
            && rig.shapes.iter().all(|s| {
                s.node < rig.nodes.len()
                    && drawn.insert(s.node)
                    && s.skin
                        .as_ref()
                        .is_none_or(|(_, joints)| joints.iter().all(|&j| j < rig.nodes.len()))
            })
    })
}

// Embedded animation uses the existing indexed Rig producer. Do not add a second
// GroupRig for the host FormId: that would steal PlayGroup/sequence/collision-pose
// ownership from the host (object_anims::drive and publish_poses).
pub(crate) fn spawn_model(
    commands: &mut Commands,
    model: &PreparedModel,
    parent: Entity,
    path: &str,
    suffix: &str,
    layer: RenderLayers,
    id: u64,
) -> Vec<Entity> {
    let entities: Vec<_> = if let Some(rig) = &model.rig {
        let (_, _, _, indexed) = rig.spawn_indexed(
            commands,
            parent,
            model.objects[0].local,
            path,
            0.,
            rig.idle.as_ref(),
        );
        model
            .objects
            .iter()
            .map(|o| indexed[o.rig_node.expect("validated rig correspondence")])
            .collect()
    } else {
        let entities: Vec<_> = model
            .objects
            .iter()
            .map(|o| commands.spawn((o.local, Visibility::Inherited)).id())
            .collect();
        for (i, object) in model.objects.iter().enumerate() {
            commands
                .entity(entities[i])
                .insert(ChildOf(object.parent.map_or(parent, |p| entities[p])));
            if let Some(s) = object.shape.map(|s| &model.shapes[s]) {
                commands
                    .entity(entities[i])
                    .insert((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone())));
            }
        }
        entities
    };
    for (object, &entity) in model.objects.iter().zip(&entities) {
        commands.entity(entity).insert((
            Name::new(format!("{}{suffix}", object.name.as_deref().unwrap_or(""))),
            Member(id),
            AttachedNode {
                source: object.clone(),
                material: object.material,
            },
            layer.clone(),
        ));
        if object.flags & 1 != 0 {
            commands
                .entity(entity)
                .insert((NifHidden, Visibility::Hidden));
        }
    }
    entities
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn produce(
    mut script: Option<ResMut<ScriptState>>,
    mut state: ResMut<State>,
    commands: Commands,
    models: Option<ModelCtx>,
    rigs: Rigs,
    visuals: Visuals,
    nodes: NodeQuery,
    parents: Query<&ChildOf>,
    layers: Query<&RenderLayers>,
    owned: Owned,
) {
    let game = script
        .as_deref_mut()
        .and_then(|s| s.rt.as_mut())
        .map(|rt| &mut rt.game);
    produce_game(
        game, &mut state, commands, models, rigs, visuals, nodes, parents, layers, owned,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn produce_game(
    game: Option<&mut fnv_game::Game<'_>>,
    state: &mut State,
    mut commands: Commands,
    mut models: Option<ModelCtx>,
    rigs: Rigs,
    visuals: Visuals,
    nodes: NodeQuery,
    parents: Query<&ChildOf>,
    layers: Query<&RenderLayers>,
    owned: Owned,
) {
    let mut retiring = Vec::new();
    let Some(game) = game else {
        retiring.extend(state.instances.drain().map(|(_, instance)| instance));
        state.intents.clear();
        state.order.clear();
        state.instances.clear();
        state.session = None;
        state.removed.clear();
        clean(&retiring, &mut commands, &owned);
        return;
    };
    let session = game.scene.attachment_session();
    if state
        .session
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &session))
    {
        retiring.extend(state.instances.drain().map(|(_, instance)| instance));
        state.intents.clear();
        state.order.clear();
        state.instances.clear();
        state.session = Some(session);
    }
    retire_instances(
        state,
        &mut retiring,
        &owned,
        &rigs,
        &visuals,
        &nodes,
        &parents,
    );
    let mut removed_instance = false;
    for mut change in game.scene.take_attachment_changes() {
        change.view = concrete_view(change.reference.0, change.view);
        let key = Key {
            reference: change.reference.0,
            view: change.view,
            target: change.target.clone(),
            model: change.model.clone(),
        };
        state.order.retain(|old| old != &key);
        if change.mode & 1 == 0 {
            state.intents.remove(&key);
            if change.mode == 2 {
                if let Some(instance) = state.instances.remove(&key) {
                    state.removed.extend(instance.entities.iter().copied());
                    removed_instance = true;
                    retiring.push(instance);
                }
            }
        } else {
            let captured = match change.captured {
                SceneTarget::Node(i) => Some(i.model),
                SceneTarget::Model(i) => Some(i),
                _ => None,
            };
            let deferred = (change.mode == 1)
                .then(|| {
                    root_of(key.reference, key.view, &rigs, &visuals)
                        .filter(|root| captured == Some(root.to_bits()))
                })
                .flatten();
            if change.mode == 1 {
                change.captured = SceneTarget::Legacy;
            }
            let produced = state.instances.contains_key(&key);
            state.order.push(key.clone());
            state.intents.insert(
                key,
                Intent {
                    change,
                    root: deferred,
                    parent: None,
                    blocked: false,
                    deferred,
                    failed: false,
                    produced,
                    stale: false,
                },
            );
        }
    }
    if removed_instance {
        retire_instances(
            state,
            &mut retiring,
            &owned,
            &rigs,
            &visuals,
            &nodes,
            &parents,
        );
    }
    clean(&retiring, &mut commands, &owned);
    // The registered order determines duplicate-name publication consistently.
    // Keep it with the intents instead of allocating/sorting every frame.
    for key in &state.order {
        let intent = state.intents.get_mut(key).expect("ordered live intent");
        if intent.stale || state.instances.contains_key(key) {
            continue;
        }
        let Some(root) =
            root_of(key.reference, key.view, &rigs, &visuals).filter(|e| nodes.get(*e).is_ok())
        else {
            continue;
        };
        if intent.deferred == Some(root) {
            continue;
        }
        if intent.root.is_some_and(|old| old != root) {
            // Replay belongs to an instance already produced in its captured model,
            // or to explicit mode 1's next-model request. An unresolved cold/captured
            // parent cannot gain authority by waiting through replacement models.
            if !intent.produced && intent.deferred != intent.root {
                intent.stale = true;
                continue;
            }
            intent.deferred = None;
            intent.change.captured = SceneTarget::Legacy;
            intent.parent = None;
            intent.blocked = false;
            intent.failed = false;
        }
        if intent.root.is_none() {
            let expected = match intent.change.captured {
                SceneTarget::Node(i) => Some(i.model),
                SceneTarget::Model(i) => Some(i),
                _ => None,
            };
            intent.stale |= expected.is_some_and(|model| model != root.to_bits())
                || intent.change.captured == SceneTarget::Missing;
        }
        intent.root = Some(root);
        if state.removed.contains(&root) {
            intent.blocked = true;
        }
        if intent.stale || intent.blocked || intent.failed {
            continue;
        }
        let names = rig_names(key.reference, key.view, &rigs);
        if intent.parent.is_none() {
            intent.parent = match intent.change.captured {
                SceneTarget::Node(identity) => bound_entity(root, identity, &nodes, &names),
                _ if key.target.is_empty() => Some(root),
                _ => find(root, &key.target, &nodes, &names),
            };
            if matches!(intent.change.captured, SceneTarget::Node(_)) && intent.parent.is_none() {
                intent.blocked = true;
            }
        }
        let Some(parent) = intent.parent else {
            continue;
        };
        if state.removed.contains(&parent)
            || nodes.get(parent).is_err()
            || !intact(root, parent, &parents, &state.removed)
        {
            intent.blocked = true;
            continue;
        }
        let Some((suffix, path)) = model_path(&key.model) else {
            intent.failed = true;
            continue;
        };
        // Mirror-only hosts may have no asset loader yet. Capture the recipient and
        // process removals above even then; only model preparation waits for assets.
        let Some(models) = models.as_mut() else {
            continue;
        };
        let Some(model) = models
            .model_for_attachment(&path)
            .filter(|model| valid(model))
        else {
            warn!(
                "attachment {:08X}: model {path:?} could not supply a complete graph",
                key.reference
            );
            intent.failed = true;
            continue;
        };
        let mut layer = RenderLayers::default();
        let mut at = parent;
        for _ in 0..ANCESTOR_LIMIT {
            if let Ok(found) = layers.get(at) {
                layer = found.clone();
                break;
            }
            let Ok(up) = parents.get(at) else { break };
            at = up.parent();
        }
        if key.reference == fnv_sim::FormId::PLAYER_REF.0 {
            layer = RenderLayers::layer(if key.view == ModelView::FirstPerson {
                crate::sky::ARMS_LAYER
            } else {
                crate::sky::BODY_LAYER
            });
        }
        let Some(id) = state.next.checked_add(1) else {
            continue;
        };
        state.next = id;
        let entities = spawn_model(&mut commands, &model, parent, &path, &suffix, layer, id);
        intent.produced = true;
        state.instances.insert(
            key.clone(),
            Instance {
                id,
                root,
                parent,
                entities,
            },
        );
    }
    state.removed.clear();
}
