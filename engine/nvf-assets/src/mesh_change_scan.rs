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
//! last run, and marks the holders of the modified ids. Bevy's system is taken out of
//! `PostUpdate` once, at `PostStartup` (`Schedule::remove_systems_in_set`, which keeps the
//! orderings that name it), so bevy_mesh need not be patched; the replacement runs at the
//! same point: after `AssetEventSystems`, before every system ordered after Bevy's.
//! If the removal fails, Bevy's system stays and the replacement never runs.

use bevy::asset::{AssetEvent, AssetEventSystems, AssetId};
use bevy::ecs::entity_disabling::Disabled;
use bevy::ecs::query::Allow;
use bevy::ecs::schedule::{ScheduleCleanupPolicy, ScheduleError};
use bevy::mesh::{mark_3d_meshes_as_changed_if_their_assets_changed, Mesh, Mesh3d};
use bevy::pbr::specialization_scan::AssetHolders;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

/// Replaces Bevy's mesh change mark with [`mark_3d_meshes_changed_by_index`].
pub struct MeshChangeScanPlugin;

impl Plugin for MeshChangeScanPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, replace_bevy_mesh_mark).add_systems(
            PostUpdate,
            mark_3d_meshes_changed_by_index
                .run_if(resource_exists::<BevyMeshMarkReplaced>)
                .after(AssetEventSystems)
                .before(mark_3d_meshes_as_changed_if_their_assets_changed),
        );
    }
}

/// Present once Bevy's system is out of `PostUpdate`; the replacement runs only then.
#[derive(Resource)]
pub struct BevyMeshMarkReplaced;

/// Takes Bevy's system out of `PostUpdate`. `RemoveSystemsOnly` keeps its system set, so
/// the systems ordered after it (the material systems, bevy_pbr `MaterialPlugin`) stay
/// ordered after the replacement, which is ordered before that set.
fn replace_bevy_mesh_mark(world: &mut World) {
    let removed = world.try_schedule_scope(PostUpdate, |world, schedule| {
        schedule.remove_systems_in_set(
            mark_3d_meshes_as_changed_if_their_assets_changed,
            world,
            ScheduleCleanupPolicy::RemoveSystemsOnly,
        )
    });
    match removed {
        Ok(Ok(n)) if n > 0 => {
            world.insert_resource(BevyMeshMarkReplaced);
        }
        // No `MeshPlugin` (an app without meshes): nothing to replace.
        Ok(Err(ScheduleError::SetNotFound)) | Ok(Ok(_)) => {
            debug!("mesh change scan: Bevy's mesh change mark is not in PostUpdate; nothing replaced");
        }
        Ok(Err(e)) => warn!("mesh change scan: Bevy's mesh change mark kept ({e:?})"),
        Err(e) => warn!("mesh change scan: Bevy's mesh change mark kept ({e:?})"),
    }
}

/// The state [`mark_3d_meshes_changed_by_index`] keeps between runs.
#[derive(Default)]
pub struct MeshMarkScan {
    holders: AssetHolders<Mesh>,
    modified: HashSet<AssetId<Mesh>>,
}

/// Marks every `Mesh3d` whose mesh was modified since the last run as changed: the
/// entities Bevy's `mark_3d_meshes_as_changed_if_their_assets_changed` marks, found
/// through the index instead of a walk over every `Mesh3d`.
#[allow(clippy::type_complexity)]
pub fn mark_3d_meshes_changed_by_index(
    mut meshes: ParamSet<(
        Query<&mut Mesh3d>,
        Query<(Entity, &Mesh3d), (Changed<Mesh3d>, Allow<Disabled>)>,
        Query<&Mesh3d, Allow<Disabled>>,
    )>,
    mut removed: RemovedComponents<Mesh3d>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut scan: Local<MeshMarkScan>,
) {
    let scan = &mut *scan;

    // Bring the index up to date. Disabled entities are kept in it (`Allow<Disabled>`), so
    // a handle swapped while disabled is not missed once they are enabled again.
    for entity in removed.read() {
        let id = meshes.p2().get(entity).ok().map(|mesh| mesh.id());
        scan.holders.set(entity, id);
    }
    for (entity, mesh) in &meshes.p1() {
        scan.holders.set(entity, Some(mesh.id()));
    }

    // Bevy's system takes every `Modified` id read, whatever followed it.
    scan.modified.clear();
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
    use bevy::pbr::{
        check_entities_needing_specialization, mark_meshes_as_changed_if_their_materials_changed,
        EntitiesNeedingSpecialization, MeshMaterial3d, StandardMaterial,
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

    /// The material systems run in either order (Bevy leaves it open), after the mesh mark
    /// as `MaterialPlugin` orders them, and the record after both.
    fn test_app(upstream: bool, mark_first: bool) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
            .init_asset::<Mat>()
            .init_resource::<EntitiesNeedingSpecialization<Mat>>()
            .init_resource::<Frames>();
        macro_rules! under_test {
            ($mark:expr, $check:expr) => {
                if mark_first {
                    app.add_systems(PostUpdate, ($mark, $check, record).chain().after(AssetEventSystems).after(mark_3d_meshes_as_changed_if_their_assets_changed));
                } else {
                    app.add_systems(PostUpdate, ($check, $mark, record).chain().after(AssetEventSystems).after(mark_3d_meshes_as_changed_if_their_assets_changed));
                }
            };
        }
        if upstream {
            under_test!(upstream_mark, upstream_check);
        } else {
            app.add_plugins(MeshChangeScanPlugin);
            under_test!(mark_meshes_as_changed_if_their_materials_changed::<Mat>, check_entities_needing_specialization::<Mat>);
        }
        app
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
            let (mut upstream, mut indexed) = (test_app(true, mark_first), test_app(false, mark_first));
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
                match n {
                    2 => {
                        assert_eq!(expected.specialize, sorted(vec![e[0], e[1]]));
                        assert_eq!(expected.mesh_changed, sorted(vec![e[0], e[1], e[4]]));
                    }
                    3 => assert_eq!(expected.specialize, sorted(vec![e[2], e[6]])),
                    5 | 16 | 25 => assert_eq!(expected, Frame::default(), "frame {n}"),
                    17 => assert_eq!(expected.specialize, sorted(vec![e[7], e[8]])),
                    20 => assert_eq!(expected.specialize.len(), 50 + 2),
                    28 => assert_eq!(expected.specialize, sorted(vec![e[2], e[3]])),
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
}
