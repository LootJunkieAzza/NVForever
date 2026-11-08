//! Bevy's `mark_3d_meshes_as_changed_if_their_assets_changed` replaced by a system that
//! marks the same entities while visiting only the holders of the modified meshes.
//!
//! [design, bevy_mesh 0.20.1 `components.rs`] Bevy's system marks a `Mesh3d` changed when
//! its mesh asset was modified (`AssetEvent::Modified` since the system's last run), so
//! the mesh extraction re-reads it. When any mesh was modified, it walks every `Mesh3d` in
//! the world and reads each handle. A live particle mesh is modified every frame, so it
//! walked all of them (3-5k in a town) every frame.
//!
//! [design] The replacement keeps an index of mesh id -> entities
//! (`bevy::pbr::specialization_scan::AssetHolders`, the same index the patched
//! `check_entities_needing_specialization` uses, engine/vendor/README.md), brought up to
//! date at the start of each run from the `Mesh3d` components changed or removed since the
//! last run, and marks the holders of the modified ids. It reads the events with a
//! `MessageReader` as Bevy's system does, so events that expired unread are lost to both
//! alike; removals that expired unread make it rebuild the index from the world (they
//! would otherwise leave entries for entities that no longer hold the mesh).
//!
//! Bevy's system is taken out of `PostUpdate` once, at `PostStartup`
//! (`Schedule::remove_systems_in_set`, which keeps the orderings that name its system set),
//! so bevy_mesh need not be patched. The replacement runs in [`MeshChangeScanSystems`],
//! after `AssetEventSystems`, before Bevy's (now empty) set, so before every system
//! ordered after Bevy's; the orderings other systems had before Bevy's system, and the
//! ones it had itself, are copied onto that set ([`replace_bevy_mesh_mark`]). If Bevy's
//! system has a run condition or a parent set, or an ordering that cannot be copied, it is
//! kept, with a warning, and the replacement never runs.
//!
//! Only Bevy's `Disabled` is let into the index (`Allow<Disabled>`); NVForever registers
//! no other disabling component (see bevy_pbr `specialization_scan.rs`).

use bevy::asset::{AssetEvent, AssetEventSystems, AssetId};
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::ecs::schedule::graph::Direction;
use bevy::ecs::schedule::{InternedSystemSet, NodeId, Schedule, ScheduleCleanupPolicy, SystemSet};
use bevy::mesh::{mark_3d_meshes_as_changed_if_their_assets_changed, Mesh, Mesh3d};
use bevy::pbr::specialization_scan::{clear_set, removals_missed, AssetHolders};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

/// Replaces Bevy's mesh change mark with [`mark_3d_meshes_changed_by_index`].
pub struct MeshChangeScanPlugin;

impl Plugin for MeshChangeScanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshMarkScan>()
            .configure_sets(
                PostUpdate,
                MeshChangeScanSystems
                    .after(AssetEventSystems)
                    .before(mark_3d_meshes_as_changed_if_their_assets_changed),
            )
            .add_systems(PostStartup, replace_bevy_mesh_mark)
            .add_systems(
                PostUpdate,
                mark_3d_meshes_changed_by_index
                    .run_if(resource_exists::<BevyMeshMarkReplaced>)
                    .in_set(MeshChangeScanSystems),
            );
    }
}

/// Where [`mark_3d_meshes_changed_by_index`] runs.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeshChangeScanSystems;

/// Present once Bevy's system is out of `PostUpdate`; the replacement runs only then.
#[derive(Resource)]
pub struct BevyMeshMarkReplaced;

/// Takes Bevy's system out of `PostUpdate`, after copying the orderings that involve it
/// onto [`MeshChangeScanSystems`].
fn replace_bevy_mesh_mark(world: &mut World) {
    let replaced = world.try_schedule_scope(PostUpdate, |world, schedule| replace_in(schedule, world));
    match replaced {
        Ok(Ok(true)) => {
            world.insert_resource(BevyMeshMarkReplaced);
        }
        // No `MeshPlugin` (an app without meshes): nothing to replace.
        Ok(Ok(false)) => debug!("mesh change scan: Bevy's mesh change mark is not in PostUpdate; nothing replaced"),
        Ok(Err(why)) => warn!("mesh change scan: Bevy's mesh change mark kept: {why}"),
        Err(e) => warn!("mesh change scan: Bevy's mesh change mark kept ({e:?})"),
    }
}

/// The work of [`replace_bevy_mesh_mark`] on the `PostUpdate` schedule: `Ok(false)` when
/// Bevy's system is not there, `Err` (nothing changed) when it cannot be replaced safely.
///
/// [design, bevy_ecs 0.20.1 `schedule.rs`] `remove_systems_in_set` with
/// `RemoveSystemsOnly` keeps the system's type set and its edges, so systems ordered after
/// it stay ordered after [`MeshChangeScanSystems`] (which is before the set). What it does
/// not keep for us is an ordering into Bevy's system from elsewhere (`x.before(it)`, or
/// `it.after(x)`), which would leave `x` unordered with the replacement, nor one out of
/// the system itself (`it.before(y)`); those are copied onto [`MeshChangeScanSystems`].
fn replace_in(schedule: &mut Schedule, world: &mut World) -> Result<bool, String> {
    schedule.initialize(world).map_err(|e| format!("{e:?}"))?;
    let original: InternedSystemSet = IntoSystemSet::into_system_set(mark_3d_meshes_as_changed_if_their_assets_changed).intern();
    let ours: InternedSystemSet = MeshChangeScanSystems.intern();
    let graph = schedule.graph();
    let Some(original_key) = graph.system_sets.get_key(original) else {
        return Ok(false);
    };
    let members: Vec<_> = graph.systems_in_set(original).map_err(|e| format!("{e:?}"))?.iter().copied().collect();
    if members.is_empty() {
        return Ok(false);
    }

    // The set a dependency node stands for, to order ours against: `None` for the sets
    // ours is already ordered against (itself, Bevy's set, `AssetEventSystems`). A system is
    // named by its type set, which is only safe when that set holds that one system. Any
    // other named set cannot be named again from here (the graph hands out `&dyn
    // SystemSet`, not the interned set), so it makes the replacement give up.
    let known: Vec<NodeId> = [ours, original, AssetEventSystems.intern()]
        .into_iter()
        .filter_map(|set| graph.system_sets.get_key(set).map(NodeId::Set))
        .collect();
    let set_of = |node: NodeId| -> Result<Option<InternedSystemSet>, String> {
        if known.contains(&node) || in_our_set(graph, node, ours) {
            return Ok(None);
        }
        match node {
            NodeId::Set(key) => Err(format!("it is ordered against the set {:?}", graph.system_sets.get(key))),
            NodeId::System(key) => {
                let system = graph.systems.get(key).ok_or_else(|| format!("unknown system {key:?}"))?.system();
                let type_set = system.default_system_sets().into_iter().find(|set| set.system_type().is_some()).ok_or_else(|| format!("{} has no type set", system.name()))?;
                let instances = graph.systems_in_set(type_set).map_err(|e| format!("{e:?}"))?.len();
                if instances == 1 {
                    Ok(Some(type_set))
                } else {
                    Err(format!("{} is ordered against it and has {instances} instances", system.name()))
                }
            }
        }
    };

    let mut after: Vec<InternedSystemSet> = Vec::new();
    let mut before: Vec<InternedSystemSet> = Vec::new();
    for pred in graph.dependency().graph().neighbors_directed(NodeId::Set(original_key), Direction::Incoming) {
        after.extend(set_of(pred)?);
    }
    for &key in &members {
        let node = NodeId::System(key);
        if graph.systems.has_conditions(key) {
            return Err("it has a run condition".into());
        }
        if let Some(parent) = graph.hierarchy().graph().neighbors_directed(node, Direction::Incoming).find(|&parent| parent != NodeId::Set(original_key)) {
            return Err(format!("it is in a set ({parent:?})"));
        }
        for pred in graph.dependency().graph().neighbors_directed(node, Direction::Incoming) {
            after.extend(set_of(pred)?);
        }
        for succ in graph.dependency().graph().neighbors_directed(node, Direction::Outgoing) {
            before.extend(set_of(succ)?);
        }
    }

    for set in after {
        schedule.configure_sets(MeshChangeScanSystems.after(set));
    }
    for set in before {
        schedule.configure_sets(MeshChangeScanSystems.before(set));
    }
    let removed = schedule
        .remove_systems_in_set(original, world, ScheduleCleanupPolicy::RemoveSystemsOnly)
        .map_err(|e| format!("{e:?}"))?;
    Ok(removed > 0)
}

/// Whether a dependency node is [`MeshChangeScanSystems`] or a system in it.
fn in_our_set(graph: &bevy::ecs::schedule::ScheduleGraph, node: NodeId, ours: InternedSystemSet) -> bool {
    let Some(ours_key) = graph.system_sets.get_key(ours) else {
        return false;
    };
    node == NodeId::Set(ours_key) || graph.hierarchy().graph().neighbors_directed(node, Direction::Incoming).any(|parent| parent == NodeId::Set(ours_key))
}

/// The state [`mark_3d_meshes_changed_by_index`] keeps between runs (a resource, so tests
/// can look at it).
#[derive(Resource, Default)]
pub struct MeshMarkScan {
    holders: AssetHolders<Mesh>,
    modified: HashSet<AssetId<Mesh>>,
}

impl MeshMarkScan {
    /// Every entity in the index.
    pub fn indexed(&self) -> impl Iterator<Item = Entity> + '_ {
        self.holders.entities()
    }
}

/// Marks every `Mesh3d` whose mesh was modified since the last run as changed: the
/// entities Bevy's `mark_3d_meshes_as_changed_if_their_assets_changed` marks, found
/// through the index instead of a walk over every `Mesh3d`.
#[allow(clippy::type_complexity)]
pub fn mark_3d_meshes_changed_by_index(
    mut meshes: ParamSet<(
        Query<&mut Mesh3d>,
        Query<(Entity, &Mesh3d), (Changed<Mesh3d>, Allow<Disabled>)>,
        Query<(Entity, &Mesh3d), Allow<Disabled>>,
    )>,
    mut removed: RemovedComponents<Mesh3d>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut scan: ResMut<MeshMarkScan>,
) {
    let scan = &mut *scan;

    // Bring the index up to date. Disabled entities are kept in it (`Allow<Disabled>`), so
    // a handle swapped while disabled is not missed once they are enabled again. Removals
    // that expired unread (the system did not run for a while): rebuild it.
    if removals_missed(&removed) {
        removed.clear();
        scan.holders.clear();
        for (entity, mesh) in &meshes.p2() {
            scan.holders.set(entity, Some(mesh.id()));
        }
    } else {
        for entity in removed.read() {
            let id = meshes.p2().get(entity).ok().map(|(_, mesh)| mesh.id());
            scan.holders.set(entity, id);
        }
        for (entity, mesh) in &meshes.p1() {
            scan.holders.set(entity, Some(mesh.id()));
        }
    }

    // Bevy's system takes every `Modified` id read, whatever followed it.
    clear_set(&mut scan.modified);
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            scan.modified.insert(*id);
        }
    }
    if scan.modified.is_empty() {
        return;
    }

    // `Query<&mut Mesh3d>` applies the default query filters as Bevy's query does, and the
    // id is checked again against the component itself.
    let mut marked = meshes.p0();
    for id in &scan.modified {
        for entity in scan.holders.holders(id) {
            if let Ok(mut mesh) = marked.get_mut(entity) {
                if mesh.id() == *id {
                    mesh.set_changed();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Equivalence of the indexed change checks with Bevy's filters, frame by frame: the
    //! patched `check_entities_needing_specialization` and
    //! `mark_meshes_as_changed_if_their_materials_changed` (engine/vendor/bevy_pbr) and
    //! [`mark_3d_meshes_changed_by_index`] in one app, upstream's filters (copied below) and
    //! Bevy's own mesh mark in another, both driven by the same edits.
    use super::*;
    use bevy::asset::prelude::AssetChanged;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{MeshPlugin, PrimitiveTopology};
    use bevy::ecs::message::Messages;
    use bevy::pbr::{
        check_entities_needing_specialization, mark_meshes_as_changed_if_their_materials_changed,
        EntitiesNeedingSpecialization, MaterialMarkScan, MeshMaterial3d, SpecializationScan, StandardMaterial,
    };

    type Mat = StandardMaterial;

    /// bevy_pbr 0.20.1 `material.rs` `mark_meshes_as_changed_if_their_materials_changed`, as
    /// upstream has it (sequential instead of `par_iter_mut`; the same entities).
    #[allow(clippy::type_complexity)]
    fn upstream_mark(mut meshes: Query<&mut Mesh3d, Or<(Changed<MeshMaterial3d<Mat>>, AssetChanged<MeshMaterial3d<Mat>>)>>) {
        meshes.iter_mut().for_each(|mut mesh| mesh.set_changed());
    }

    /// bevy_pbr 0.20.1 `material.rs` `check_entities_needing_specialization`, as upstream
    /// has it (sequential instead of `par_iter`; the same entities).
    #[allow(clippy::type_complexity)]
    fn upstream_check(
        needs_specialization: Query<
            Entity,
            (Or<(Changed<Mesh3d>, AssetChanged<Mesh3d>, Changed<MeshMaterial3d<Mat>>, AssetChanged<MeshMaterial3d<Mat>>)>, With<MeshMaterial3d<Mat>>),
        >,
        mut needs: ResMut<EntitiesNeedingSpecialization<Mat>>,
        mut removed_meshes: RemovedComponents<Mesh3d>,
        mut removed_materials: RemovedComponents<MeshMaterial3d<Mat>>,
    ) {
        needs.changed.clear();
        needs.removed.clear();
        needs.changed.extend(needs_specialization.iter());
        let removed: Vec<Entity> = removed_meshes.read().chain(removed_materials.read()).collect();
        needs.removed.extend(removed);
    }

    /// What one frame flagged: the entities to specialize again, those to drop, and the
    /// `Mesh3d` components changed (by the edits or by the marks) since the last frame.
    #[derive(Debug, PartialEq, Eq, Clone, Default)]
    struct Frame {
        specialize: Vec<Entity>,
        unspecialize: Vec<Entity>,
        mesh_changed: Vec<Entity>,
    }

    #[derive(Resource, Default)]
    struct Frames(Vec<Frame>);

    fn sorted(mut v: Vec<Entity>) -> Vec<Entity> {
        v.sort();
        v
    }

    fn record(needs: Res<EntitiesNeedingSpecialization<Mat>>, changed: Query<Entity, Changed<Mesh3d>>, mut frames: ResMut<Frames>) {
        frames.0.push(Frame {
            specialize: sorted(needs.changed.clone()),
            unspecialize: sorted(needs.removed.clone()),
            mesh_changed: sorted(changed.iter().collect()),
        });
    }

    /// Whether the systems under test run this frame (the gap tests turn it off).
    #[derive(Resource)]
    struct Running(bool);

    fn running(running: Res<Running>) -> bool {
        running.0
    }

    /// Bevy's mesh mark, as a system of the test's own, so the gap tests can hold it back
    /// in the reference app (Bevy's system itself takes no run condition).
    fn upstream_mesh_mark(meshes: Query<&mut Mesh3d>, events: MessageReader<AssetEvent<Mesh>>) {
        mark_3d_meshes_as_changed_if_their_assets_changed(meshes, events);
    }

    fn remove_bevy_mesh_mark(world: &mut World) {
        world
            .schedule_scope(PostUpdate, |world, schedule| {
                schedule.remove_systems_in_set(mark_3d_meshes_as_changed_if_their_assets_changed, world, ScheduleCleanupPolicy::RemoveSystemsOnly)
            })
            .unwrap();
    }

    /// The material systems run in either order (Bevy leaves it open), after the mesh mark
    /// as `MaterialPlugin` orders them, and the record after both. `gated`: all of them,
    /// and the mesh mark, run only while [`Running`] says so (the patched app's mesh mark
    /// while `BevyMeshMarkReplaced` is present: [`set_running`]).
    fn test_app(upstream: bool, mark_first: bool, gated: bool) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
            .init_asset::<Mat>()
            .init_resource::<EntitiesNeedingSpecialization<Mat>>()
            .init_resource::<SpecializationScan<Mat>>()
            .init_resource::<MaterialMarkScan<Mat>>()
            .init_resource::<Frames>()
            .insert_resource(Running(true));
        macro_rules! under_test {
            ($mark:expr, $check:expr) => {
                if mark_first {
                    app.add_systems(PostUpdate, ($mark, $check, record).chain().run_if(running).after(AssetEventSystems).after(mark_3d_meshes_as_changed_if_their_assets_changed));
                } else {
                    app.add_systems(PostUpdate, ($check, $mark, record).chain().run_if(running).after(AssetEventSystems).after(mark_3d_meshes_as_changed_if_their_assets_changed));
                }
            };
        }
        if upstream {
            if gated {
                app.add_systems(PostStartup, remove_bevy_mesh_mark).add_systems(
                    PostUpdate,
                    upstream_mesh_mark.run_if(running).after(AssetEventSystems).before(mark_3d_meshes_as_changed_if_their_assets_changed),
                );
            }
            under_test!(upstream_mark, upstream_check);
        } else {
            app.add_plugins(MeshChangeScanPlugin);
            under_test!(mark_meshes_as_changed_if_their_materials_changed::<Mat>, check_entities_needing_specialization::<Mat>);
        }
        app
    }

    fn set_running(app: &mut App, upstream: bool, on: bool) {
        app.world_mut().resource_mut::<Running>().0 = on;
        if !upstream {
            if on {
                app.world_mut().insert_resource(BevyMeshMarkReplaced);
            } else {
                app.world_mut().remove_resource::<BevyMeshMarkReplaced>();
            }
        }
    }

    #[derive(Default)]
    struct Fixture {
        meshes: Vec<Handle<Mesh>>,
        materials: Vec<Handle<Mat>>,
        /// e0..e9, see step 0 and step 22.
        e: Vec<Entity>,
        bulk: Vec<Entity>,
        /// Every entity the edits spawned, in order: an entity's place here is what the two
        /// apps are compared by, as their `Entity` values need not match: resources are
        /// entities in Bevy 0.20 (`bevy_ecs` `resource.rs` `IS_RESOURCE`), and the reference
        /// app makes two the other does not (`AssetChanges<Mesh>`, `AssetChanges<Mat>`, at
        /// its first `PostUpdate`) while the other makes one of its own
        /// (`BevyMeshMarkReplaced`), so later spawns get other indices.
        spawned: Vec<Entity>,
    }

    impl Fixture {
        /// The places in `spawned` of `entities`, sorted. Panics on an entity the edits did
        /// not spawn: no system under test may flag one.
        fn labels(&self, entities: &[Entity]) -> Vec<usize> {
            let mut labels: Vec<usize> = entities
                .iter()
                .map(|entity| self.spawned.iter().position(|e| e == entity).unwrap_or_else(|| panic!("{entity} was not spawned by the edits")))
                .collect();
            labels.sort();
            labels
        }

        fn label_frame(&self, frame: &Frame) -> [Vec<usize>; 3] {
            [self.labels(&frame.specialize), self.labels(&frame.unspecialize), self.labels(&frame.mesh_changed)]
        }
    }

    fn new_mesh() -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
    }

    fn touch_mesh(world: &mut World, mesh: &Handle<Mesh>) {
        let _ = world.resource_mut::<Assets<Mesh>>().get_mut(mesh).unwrap().into_inner();
    }

    fn touch_material(world: &mut World, material: &Handle<Mat>) {
        let _ = world.resource_mut::<Assets<Mat>>().get_mut(material).unwrap().into_inner();
    }

    fn spawn(world: &mut World, spawned: &mut Vec<Entity>, mesh: Option<&Handle<Mesh>>, material: Option<&Handle<Mat>>) -> Entity {
        let mut entity = world.spawn_empty();
        if let Some(mesh) = mesh {
            entity.insert(Mesh3d(mesh.clone()));
        }
        if let Some(material) = material {
            entity.insert(MeshMaterial3d(material.clone()));
        }
        spawned.push(entity.id());
        entity.id()
    }

    /// The edits made before frame `n`.
    fn step(world: &mut World, f: &mut Fixture, n: usize) {
        let (m, s) = (f.meshes.clone(), f.materials.clone());
        let e = f.e.clone();
        match n {
            0 => {
                f.meshes = (0..6).map(|_| world.resource_mut::<Assets<Mesh>>().add(new_mesh())).collect();
                f.materials = (0..4).map(|_| world.resource_mut::<Assets<Mat>>().add(Mat::default())).collect();
                let (m, s) = (f.meshes.clone(), f.materials.clone());
                f.e = vec![
                    spawn(world, &mut f.spawned, Some(&m[0]), Some(&s[0])), // e0 shares m0 and s0 with e1
                    spawn(world, &mut f.spawned, Some(&m[0]), Some(&s[0])), // e1
                    spawn(world, &mut f.spawned, Some(&m[1]), Some(&s[1])), // e2
                    spawn(world, &mut f.spawned, Some(&m[2]), Some(&s[0])), // e3
                    spawn(world, &mut f.spawned, Some(&m[0]), None),        // e4: a mesh without a material
                    spawn(world, &mut f.spawned, None, Some(&s[2])),        // e5: a material without a mesh
                    spawn(world, &mut f.spawned, Some(&m[3]), Some(&s[1])), // e6
                    spawn(world, &mut f.spawned, Some(&m[4]), Some(&s[3])), // e7
                    spawn(world, &mut f.spawned, Some(&m[4]), Some(&s[3])), // e8
                ];
                f.bulk = (0..50).map(|_| spawn(world, &mut f.spawned, Some(&m[5]), Some(&s[3]))).collect();
            }
            2 => touch_mesh(world, &m[0]),
            3 => touch_material(world, &s[1]),
            // A handle replaced, and its new mesh modified in the same frame.
            4 => {
                world.entity_mut(e[2]).insert(Mesh3d(m[2].clone()));
                touch_mesh(world, &m[2]);
            }
            // The old mesh of e2: nobody holds it any more.
            5 => touch_mesh(world, &m[1]),
            // A handle changed in place (no hook sees this).
            6 => world.get_mut::<MeshMaterial3d<Mat>>(e[3]).unwrap().0 = s[1].clone(),
            7 => {
                touch_material(world, &s[1]);
                touch_material(world, &s[0]);
            }
            8 => {
                world.entity_mut(e[5]).insert(Mesh3d(m[3].clone()));
            }
            9 => touch_mesh(world, &m[3]),
            10 => {
                world.entity_mut(e[0]).remove::<Mesh3d>();
                touch_mesh(world, &m[0]);
            }
            11 => touch_mesh(world, &m[0]),
            12 => {
                world.despawn(e[1]);
                touch_mesh(world, &m[0]);
            }
            13 => {
                world.entity_mut(e[6]).remove::<MeshMaterial3d<Mat>>();
                touch_mesh(world, &m[3]);
            }
            14 => {
                world.entity_mut(e[7]).insert(Disabled);
                touch_mesh(world, &m[4]);
            }
            // Its handle swapped while it is disabled.
            15 => world.get_mut::<Mesh3d>(e[7]).unwrap().0 = m[1].clone(),
            16 => {
                world.entity_mut(e[7]).remove::<Disabled>();
            }
            17 => {
                touch_mesh(world, &m[1]);
                touch_mesh(world, &m[4]);
            }
            // Modified, then removed, in one frame.
            18 => {
                touch_mesh(world, &m[4]);
                world.resource_mut::<Assets<Mesh>>().remove(&m[4]);
            }
            // Back under the same id.
            19 => world.resource_mut::<Assets<Mesh>>().insert(m[4].id(), new_mesh()).unwrap(),
            20 => touch_material(world, &s[3]),
            21 => {
                world.entity_mut(f.bulk[0]).insert(MeshMaterial3d(s[0].clone()));
                for entity in f.bulk.drain(40..) {
                    world.despawn(entity);
                }
                for _ in 0..10 {
                    let entity = spawn(world, &mut f.spawned, Some(&m[5]), Some(&s[3]));
                    f.bulk.push(entity);
                }
            }
            // A mesh only e9 holds a handle to.
            22 => {
                touch_mesh(world, &m[5]);
                let lone = world.resource_mut::<Assets<Mesh>>().add(new_mesh());
                let e9 = spawn(world, &mut f.spawned, Some(&lone), Some(&s[2]));
                drop(lone); // e9 holds the only handle now
                f.e.push(e9);
            }
            // Its last handle dropped: `Unused` and `Removed` on the next frame.
            23 => {
                world.despawn(e[9]);
                touch_mesh(world, &m[3]);
            }
            24 => {
                touch_material(world, &s[0]);
                touch_mesh(world, &m[0]);
                world.get_mut::<Mesh3d>(e[4]).unwrap().set_changed();
            }
            26 => {
                world.entity_mut(e[2]).insert(MeshMaterial3d(s[2].clone()));
                touch_material(world, &s[1]);
                touch_material(world, &s[2]);
            }
            // Removed and added again in one frame.
            27 => {
                world.entity_mut(e[8]).remove::<Mesh3d>();
                world.entity_mut(e[8]).insert(Mesh3d(m[0].clone()));
                touch_mesh(world, &m[0]);
            }
            // Marked changed on purpose, handles kept (as the engine's respecialize, draw
            // distance, door fade and player body code do).
            28 => {
                world.get_mut::<MeshMaterial3d<Mat>>(e[3]).unwrap().set_changed();
                world.get_mut::<Mesh3d>(e[2]).unwrap().set_changed();
            }
            _ => {}
        }
    }

    fn post_update_system_names(app: &App) -> Vec<String> {
        let schedule = app.world().resource::<Schedules>().get(PostUpdate).unwrap();
        schedule.systems().unwrap().map(|(_, system)| system.name().to_string()).collect()
    }

    #[test]
    fn indexed_change_checks_flag_what_bevys_filters_flag() {
        for mark_first in [true, false] {
            let (mut upstream, mut indexed) = (test_app(true, mark_first, false), test_app(false, mark_first, false));
            let (mut fu, mut fi) = (Fixture::default(), Fixture::default());
            let mut compared = 0;
            for n in 0..30 {
                step(upstream.world_mut(), &mut fu, n);
                step(indexed.world_mut(), &mut fi, n);
                // The same edits: as many entities spawned, the same ones named and alive.
                assert_eq!(fu.spawned.len(), fi.spawned.len(), "frame {n}: the two apps spawn as many entities");
                assert_eq!(fu.labels(&fu.e), fi.labels(&fi.e), "frame {n}");
                assert_eq!(fu.labels(&fu.bulk), fi.labels(&fi.bulk), "frame {n}");
                let alive = |app: &App, f: &Fixture| f.spawned.iter().map(|&e| app.world().get_entity(e).is_ok()).collect::<Vec<_>>();
                assert_eq!(alive(&upstream, &fu), alive(&indexed, &fi), "frame {n}: the same entities alive");
                upstream.update();
                indexed.update();
                let expected = upstream.world().resource::<Frames>().0.last().cloned().unwrap();
                let got = indexed.world().resource::<Frames>().0.last().cloned().unwrap();
                assert_eq!(fi.label_frame(&got), fu.label_frame(&expected), "frame {n} (material mark first: {mark_first})");
                compared += 1;

                // The reference itself, on a few frames, so the comparison is not vacuous.
                let e = &fu.e;
                let at = format!("reference app, frame {n} (material mark first: {mark_first})");
                match n {
                    2 => {
                        assert_eq!(expected.specialize, sorted(vec![e[0], e[1]]), "{at}");
                        assert_eq!(expected.mesh_changed, sorted(vec![e[0], e[1], e[4]]), "{at}");
                    }
                    3 => assert_eq!(expected.specialize, sorted(vec![e[2], e[6]]), "{at}"),
                    5 | 16 => assert_eq!(expected, Frame::default(), "{at}"),
                    // Frame 24 modified s0. With the check first, the material mark sets the
                    // `Mesh3d` of s0's holders changed after the check ran, so the check sees
                    // that a frame later: bulk[0] (e0, the other holder, has no `Mesh3d`).
                    // With the mark first, the check saw it in frame 24 already.
                    25 => {
                        let carried = if mark_first { vec![] } else { vec![fu.bulk[0]] };
                        assert_eq!(expected, Frame { specialize: carried, ..Frame::default() }, "{at}");
                    }
                    17 => assert_eq!(expected.specialize, sorted(vec![e[7], e[8]]), "{at}"),
                    20 => assert_eq!(expected.specialize.len(), 50 + 2, "{at}"),
                    28 => assert_eq!(expected.specialize, sorted(vec![e[2], e[3]]), "{at}"),
                    _ => {}
                }
            }
            assert_eq!(compared, 30, "every frame compared");
            assert!(post_update_system_names(&upstream).iter().any(|n| n.contains("mark_3d_meshes_as_changed_if_their_assets_changed")));
            let names = post_update_system_names(&indexed);
            assert!(!names.iter().any(|n| n.contains("mark_3d_meshes_as_changed_if_their_assets_changed")), "Bevy's mesh mark is out");
            assert!(names.iter().any(|n| n.contains("mark_3d_meshes_changed_by_index")));
        }
    }

    #[test]
    fn asset_holders_follow_handle_changes() {
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let x = AssetId::<Mesh>::Uuid { uuid: bevy::asset::uuid::Uuid::from_u128(3) };
        let y = AssetId::<Mesh>::Uuid { uuid: bevy::asset::uuid::Uuid::from_u128(7) };
        let mut holders = AssetHolders::<Mesh>::default();
        holders.set(a, Some(x));
        holders.set(b, Some(x));
        holders.set(a, Some(y));
        assert_eq!(holders.holders(&x).collect::<Vec<_>>(), vec![b]);
        assert_eq!(holders.holders(&y).collect::<Vec<_>>(), vec![a]);
        holders.set(b, None);
        assert_eq!(holders.holders(&x).count(), 0);
        assert_eq!(holders.len(), 1);
    }

    /// Two buffer updates, as the engine's message update does once a frame: events written
    /// before this are gone for a reader that has not read them.
    fn expire_asset_messages(world: &mut World) {
        for _ in 0..2 {
            world.resource_mut::<Messages<AssetEvent<Mesh>>>().update();
            world.resource_mut::<Messages<AssetEvent<Mat>>>().update();
        }
    }

    /// The edits of the gap tests, made before frame `n`.
    fn gap_step(world: &mut World, f: &mut Fixture, n: usize) {
        let (m, s) = (f.meshes.clone(), f.materials.clone());
        let e = f.e.clone();
        match n {
            0 => {
                f.meshes = (0..4).map(|_| world.resource_mut::<Assets<Mesh>>().add(new_mesh())).collect();
                f.materials = (0..4).map(|_| world.resource_mut::<Assets<Mat>>().add(Mat::default())).collect();
                let (m, s) = (f.meshes.clone(), f.materials.clone());
                f.e = vec![
                    spawn(world, &mut f.spawned, Some(&m[0]), Some(&s[0])), // e0
                    spawn(world, &mut f.spawned, Some(&m[0]), Some(&s[0])), // e1
                    spawn(world, &mut f.spawned, Some(&m[1]), Some(&s[1])), // e2: left alone; s1 changes in the gap
                    spawn(world, &mut f.spawned, Some(&m[2]), Some(&s[2])), // e3
                    spawn(world, &mut f.spawned, Some(&m[1]), None),        // e4
                ];
                f.bulk = (0..20).map(|_| spawn(world, &mut f.spawned, Some(&m[3]), Some(&s[3]))).collect();
            }
            1 => touch_mesh(world, &m[3]),
            2 => {
                touch_material(world, &s[1]);
                touch_mesh(world, &m[2]);
            }
            3 => {
                world.despawn(e[1]);
                for entity in f.bulk.drain(..5) {
                    world.despawn(entity);
                }
                expire_asset_messages(world);
            }
            4 => {
                touch_mesh(world, &m[0]);
                world.entity_mut(e[0]).insert(Mesh3d(m[2].clone()));
                expire_asset_messages(world);
            }
            6 => touch_material(world, &s[1]),
            7 => touch_mesh(world, &m[2]),
            8 => touch_mesh(world, &m[3]),
            9 => {
                world.entity_mut(e[2]).insert(MeshMaterial3d(s[2].clone()));
                touch_material(world, &s[2]);
            }
            _ => {}
        }
    }

    fn is_subset(small: &[usize], big: &[usize]) -> bool {
        small.iter().all(|label| big.contains(label))
    }

    /// Systems held back for some frames while assets change and entities despawn, then
    /// run again: mid-run (frames 2-5 skipped) and from the start (frames 0-4 skipped).
    /// Bevy's filters keep a tick per asset, so they still flag what changed in the gap; the
    /// patched systems find the messages naming it expired and flag every holder instead.
    /// So in the frame they run again, and the next one (a material mark run after the
    /// check carries its marks into it), the patched app may flag more than the reference,
    /// never less; in every other frame both flag the same. The removals expired too, and
    /// the indexes must not keep the despawned entities.
    #[test]
    fn indexed_change_checks_after_a_gap_flag_at_least_what_bevys_filters_flag() {
        // From the start, the patched app's mesh mark still runs once in frame 0 (its
        // `PostStartup` inserts `BevyMeshMarkReplaced` after `set_running` took it away);
        // frame 0 modifies no mesh, so that run marks nothing.
        for (gap, resume) in [(2..=5, 6), (0..=4, 5)] {
            for mark_first in [true, false] {
                let (mut upstream, mut indexed) = (test_app(true, mark_first, true), test_app(false, mark_first, true));
                let (mut fu, mut fi) = (Fixture::default(), Fixture::default());
                let mut compared = 0;
                for n in 0..12 {
                    let on = !gap.contains(&n);
                    set_running(&mut upstream, true, on);
                    set_running(&mut indexed, false, on);
                    gap_step(upstream.world_mut(), &mut fu, n);
                    gap_step(indexed.world_mut(), &mut fi, n);
                    assert_eq!(fu.spawned.len(), fi.spawned.len());
                    assert_eq!(fu.labels(&fu.e), fi.labels(&fi.e));
                    assert_eq!(fu.labels(&fu.bulk), fi.labels(&fi.bulk));
                    upstream.update();
                    indexed.update();
                    let (fu_frames, fi_frames) = (&upstream.world().resource::<Frames>().0, &indexed.world().resource::<Frames>().0);
                    assert_eq!(fu_frames.len(), fi_frames.len(), "frame {n}: both record in the same frames");
                    if !on {
                        continue;
                    }
                    let at = format!("frame {n} (gap {gap:?}, material mark first: {mark_first})");
                    let [want_spec, want_unspec, want_mesh] = fu.label_frame(fu_frames.last().unwrap());
                    let [got_spec, got_unspec, got_mesh] = fi.label_frame(fi_frames.last().unwrap());
                    assert_eq!(got_unspec, want_unspec, "{at}");
                    if n == resume || n == resume + 1 {
                        assert!(is_subset(&want_spec, &got_spec), "{at}: {want_spec:?} not within {got_spec:?}");
                        assert!(is_subset(&want_mesh, &got_mesh), "{at}: {want_mesh:?} not within {got_mesh:?}");
                    } else {
                        assert_eq!((&got_spec, &got_mesh), (&want_spec, &want_mesh), "{at}");
                    }
                    if n == resume && *gap.start() > 0 {
                        // The reference still flags e2, whose material changed in the gap,
                        // although the event naming it expired; the patched app flags every
                        // holder, so more (the gap path ran).
                        assert!(want_spec.contains(&fu.labels(&[fu.e[2]])[0]), "{at}");
                        assert!(got_spec.len() > want_spec.len(), "{at}: the patched app saw no gap");
                    }
                    compared += 1;
                }
                assert_eq!(compared, 12 - gap.clone().count());

                // No index keeps an entity that is gone or no longer holds the component.
                let world = indexed.world();
                let check = |name: &str, entities: Vec<Entity>, holds: &dyn Fn(Entity) -> bool| {
                    for entity in entities {
                        assert!(world.get_entity(entity).is_ok() && holds(entity), "{name} keeps {entity} (gap {gap:?}, mark first {mark_first})");
                    }
                };
                let has_material = |entity: Entity| world.get::<MeshMaterial3d<Mat>>(entity).is_some();
                let has_mesh = |entity: Entity| world.get::<Mesh3d>(entity).is_some();
                check("SpecializationScan", world.resource::<SpecializationScan<Mat>>().indexed().collect(), &has_material);
                check("MaterialMarkScan", world.resource::<MaterialMarkScan<Mat>>().indexed().collect(), &has_material);
                check("MeshMarkScan", world.resource::<MeshMarkScan>().indexed().collect(), &has_mesh);
            }
        }
    }

    fn before_mesh_mark() {}
    fn after_mesh_mark() {}

    /// Orderings other systems had against Bevy's mesh mark hold for the replacement: one
    /// ordered before Bevy's system runs before [`MeshChangeScanSystems`], and one after it
    /// still runs after.
    #[test]
    fn orderings_against_bevys_mesh_mark_carry_over() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin, MeshChangeScanPlugin)).add_systems(
            PostUpdate,
            (
                before_mesh_mark.before(mark_3d_meshes_as_changed_if_their_assets_changed),
                after_mesh_mark.after(mark_3d_meshes_as_changed_if_their_assets_changed),
            ),
        );
        app.update();
        app.update();
        assert!(app.world().contains_resource::<BevyMeshMarkReplaced>());
        let schedule = app.world().resource::<Schedules>().get(PostUpdate).unwrap();
        let graph = schedule.graph();
        let key = |set: InternedSystemSet| NodeId::Set(graph.system_sets.get_key(set).unwrap());
        let ours = key(MeshChangeScanSystems.intern());
        let before = key(IntoSystemSet::into_system_set(before_mesh_mark).intern());
        let original = key(IntoSystemSet::into_system_set(mark_3d_meshes_as_changed_if_their_assets_changed).intern());
        let dependency = graph.dependency().graph();
        assert!(dependency.contains_edge(before, ours), "the system ordered before Bevy's mesh mark is ordered before the replacement");
        assert!(dependency.contains_edge(ours, original), "the replacement is ordered before Bevy's (now empty) set, so before what follows it");
        let names = post_update_system_names(&app);
        let position = |name: &str| names.iter().position(|n| n.contains(name)).unwrap_or_else(|| panic!("{name} not in PostUpdate"));
        assert!(position("before_mesh_mark") < position("mark_3d_meshes_changed_by_index"));
        assert!(position("mark_3d_meshes_changed_by_index") < position("after_mesh_mark"));
    }
}
