//! Headless integration tests for `bevy_pbr`'s reverse material/mesh asset indices (#4719).
use bevy::asset::AssetPlugin;
use bevy::pbr::{EntitiesNeedingSpecialization, MaterialPlugin, MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use bevy::mesh::PrimitiveTopology;
use bevy::asset::RenderAssetUsages;

#[derive(Resource, Default)]
struct ChangedMeshes(Vec<Entity>);

fn record_changed_meshes(q: Query<Entity, Changed<Mesh3d>>, mut out: ResMut<ChangedMeshes>) {
    out.0.clear();
    out.0.extend(q.iter());
}

fn dummy_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
}

#[test]
fn material_and_mesh_asset_edits_target_only_bound_entities() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MaterialPlugin::<StandardMaterial>::default(),
    ));
    app.init_asset::<Mesh>();
    app.init_resource::<ChangedMeshes>();
    app.add_systems(Last, record_changed_meshes);

    let mesh_a = app.world_mut().resource_mut::<Assets<Mesh>>().add(dummy_mesh());
    let mesh_b = app.world_mut().resource_mut::<Assets<Mesh>>().add(dummy_mesh());
    let mat_a = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());
    let mat_b = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());

    let e1 = app.world_mut().spawn((Mesh3d(mesh_a.clone()), MeshMaterial3d(mat_a.clone()))).id();
    let e2 = app.world_mut().spawn((Mesh3d(mesh_b.clone()), MeshMaterial3d(mat_b.clone()))).id();

    // Frame 1: initial population marks both entities.
    app.update();
    let spec = app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>();
    assert!(spec.changed.contains(&e1));
    assert!(spec.changed.contains(&e2));

    // Settle one frame (the two PostUpdate systems are unordered relative to each other, so
    // `mark_meshes_as_changed_if_their_materials_changed` may mark `Mesh3d` after
    // `check_entities_needing_specialization` on frame 1), then verify steady state is quiet.
    app.update();
    app.update();
    let spec = app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>();
    assert!(spec.changed.is_empty(), "steady state should not re-specialize");
    assert!(spec.removed.is_empty());
    assert!(app.world().resource::<ChangedMeshes>().0.is_empty());

    // Mutating mat_a marks ONLY e1 (both Mesh3d change and specialization).
    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&mat_a)
        .unwrap()
        .perceptual_roughness = 0.25;
    app.update();
    let spec = app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>();
    assert_eq!(spec.changed, vec![e1]);
    assert_eq!(app.world().resource::<ChangedMeshes>().0, vec![e1]);

    // Settle any carry-over Mesh3d tick from the unordered material-change mark above.
    app.update();
    app.update();
    assert!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed.is_empty());

    // Mutating mesh_b marks ONLY e2 for specialization.
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&mesh_b)
        .unwrap()
        .insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]]);
    app.update();
    let spec = app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>();
    assert_eq!(spec.changed, vec![e2]);

    // Rebind e1 from mat_a to mat_b, settle, then mutate mat_a -> e1 must NOT trigger!
    app.world_mut().entity_mut(e1).insert(MeshMaterial3d(mat_b.clone()));
    app.update();
    assert_eq!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed, vec![e1]);
    app.update();
    app.update();

    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&mat_a)
        .unwrap()
        .perceptual_roughness = 0.75;
    app.update();
    assert!(
        app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed.is_empty(),
        "rebound entity must be evicted from old material bucket"
    );
    assert!(app.world().resource::<ChangedMeshes>().0.is_empty());

    // Mutating mat_b now marks BOTH e1 and e2!
    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&mat_b)
        .unwrap()
        .perceptual_roughness = 0.6;
    app.update();
    let spec = app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>();
    assert_eq!(spec.changed.len(), 2);
    assert!(spec.changed.contains(&e1));
    assert!(spec.changed.contains(&e2));
    assert_eq!(app.world().resource::<ChangedMeshes>().0.len(), 2);

    // Removing MeshMaterial3d from e2 reports e2 removed and evicts it from mat_b.
    app.world_mut().entity_mut(e2).remove::<MeshMaterial3d<StandardMaterial>>();
    app.update();
    assert_eq!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().removed, vec![e2]);
    app.update();
    app.update();

    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&mat_b)
        .unwrap()
        .perceptual_roughness = 0.9;
    app.update();
    assert_eq!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed, vec![e1]);
}

#[test]
fn late_added_mesh3d_is_indexed_for_subsequent_material_edits() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MaterialPlugin::<StandardMaterial>::default(),
    ));
    app.init_asset::<Mesh>();
    app.init_resource::<ChangedMeshes>();
    app.add_systems(Last, record_changed_meshes);

    let mesh_a = app.world_mut().resource_mut::<Assets<Mesh>>().add(dummy_mesh());
    let mat_a = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());

    // Spawn with MeshMaterial3d only first.
    let e = app.world_mut().spawn(MeshMaterial3d(mat_a.clone())).id();
    app.update();

    // Add Mesh3d on a later frame without touching MeshMaterial3d.
    app.world_mut().entity_mut(e).insert(Mesh3d(mesh_a));
    app.update();
    assert_eq!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed, vec![e]);

    // Steady frame.
    app.update();
    assert!(app.world().resource::<ChangedMeshes>().0.is_empty());

    // Mutate mat_a -> late-attached Mesh3d must be marked changed!
    app.world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&mat_a)
        .unwrap()
        .perceptual_roughness = 0.4;
    app.update();
    assert_eq!(app.world().resource::<ChangedMeshes>().0, vec![e]);
    assert_eq!(app.world().resource::<EntitiesNeedingSpecialization<StandardMaterial>>().changed, vec![e]);
}
