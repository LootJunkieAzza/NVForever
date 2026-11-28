//! O(changed) replacement for Bevy's `mark_3d_meshes_as_changed_if_their_assets_changed`
//! [design].
//!
//! Upstream Bevy scans every `Mesh3d` entity in the world whenever ANY single `Mesh` asset
//! emits `AssetEvent::Modified` (e.g. a dynamic particle, ribbon, grass, or morph mesh).
//! With 40,000+ static world meshes loaded, that full-world parallel scan consumed 3.05 s
//! of CPU in ETW profiling. Mirroring `MaterialMeshIndex<M>` in vendored `bevy_pbr`, this
//! plugin maintains an incremental `AssetId<Mesh> -> Vec<Entity>` reverse index and marks
//! only the entities referencing modified mesh assets, while gating the upstream system
//! with `run_if(|| false)` so every `.before`/`.after`/`.ambiguous_with` ordering edge stays
//! intact.

use bevy::asset::{AssetEvent, AssetEventSystems, AssetId};
use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::system::ParamSet;
use bevy::mesh::{
    mark_2d_meshes_as_changed_if_their_assets_changed,
    mark_3d_meshes_as_changed_if_their_assets_changed, skinning::SkinnedMeshInverseBindposes, Mesh,
    Mesh3d,
};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Drop-in replacement for [`bevy::mesh::MeshPlugin`] [design].
///
/// Registers the exact same mesh assets and 2D mesh sync, replaces the O(N) 3D mesh
/// scan with [`mark_3d_meshes_as_changed_indexed`], and keeps
/// `mark_3d_meshes_as_changed_if_their_assets_changed` in the schedule as a disabled
/// (`run_if(|| false)`) ordering anchor so every external `.before`/`.after` edge
/// (`mesh_terrain_blend`, `bevy_pbr::material`) remains valid.
pub struct MeshAssetSyncPlugin;

impl Plugin for MeshAssetSyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Mesh>()
            .init_asset::<SkinnedMeshInverseBindposes>()
            .register_asset_reflect::<Mesh>()
            .add_systems(
                PostUpdate,
                (
                    mark_2d_meshes_as_changed_if_their_assets_changed.after(AssetEventSystems),
                    mark_3d_meshes_as_changed_indexed
                        .after(AssetEventSystems)
                        .before(mark_3d_meshes_as_changed_if_their_assets_changed),
                    mark_3d_meshes_as_changed_if_their_assets_changed
                        .after(AssetEventSystems)
                        .run_if(|| false),
                ),
            );
    }
}

#[derive(Default)]
struct MeshAssetIndex {
    initialized: bool,
    entity_mesh: EntityHashMap<AssetId<Mesh>>,
    by_mesh: HashMap<AssetId<Mesh>, Vec<Entity>>,
}

impl MeshAssetIndex {
    fn remove_from_mesh(&mut self, mesh_id: AssetId<Mesh>, entity: Entity) {
        if let Some(list) = self.by_mesh.get_mut(&mesh_id) {
            if let Some(pos) = list.iter().position(|&e| e == entity) {
                list.swap_remove(pos);
            }
            if list.is_empty() {
                self.by_mesh.remove(&mesh_id);
            }
        }
    }

    fn upsert(&mut self, entity: Entity, mesh_id: AssetId<Mesh>) {
        if let Some(&old) = self.entity_mesh.get(&entity) {
            if old == mesh_id {
                return;
            }
            self.remove_from_mesh(old, entity);
        }
        self.entity_mesh.insert(entity, mesh_id);
        self.by_mesh.entry(mesh_id).or_default().push(entity);
    }

    fn remove(&mut self, entity: Entity) {
        if let Some(old) = self.entity_mesh.remove(&entity) {
            self.remove_from_mesh(old, entity);
        }
    }
}

#[allow(clippy::type_complexity)]
fn mark_3d_meshes_as_changed_indexed(
    mut queries: ParamSet<(
        Query<(Entity, &Mesh3d), Changed<Mesh3d>>,
        Query<(Entity, &Mesh3d)>,
        Query<&mut Mesh3d>,
    )>,
    mut removed: RemovedComponents<Mesh3d>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut index: Local<MeshAssetIndex>,
    mut modified: Local<Vec<AssetId<Mesh>>>,
    mut stale: Local<Vec<Entity>>,
) {
    if !index.initialized {
        index.initialized = true;
        for _ in removed.read() {}
        for (entity, mesh) in &queries.p1() {
            index.upsert(entity, mesh.0.id());
        }
    } else {
        for entity in removed.read() {
            index.remove(entity);
        }
        for (entity, mesh) in &queries.p0() {
            index.upsert(entity, mesh.0.id());
        }
    }

    modified.clear();
    for event in mesh_events.read() {
        if let AssetEvent::Modified { id } = event {
            if !modified.contains(id) {
                modified.push(*id);
            }
        }
    }
    if modified.is_empty() {
        return;
    }

    let mut all_meshes = queries.p2();
    stale.clear();
    for &mesh_id in modified.iter() {
        if let Some(entities) = index.by_mesh.get(&mesh_id) {
            for &entity in entities {
                if let Ok(mut mesh_3d) = all_meshes.get_mut(entity) {
                    mesh_3d.set_changed();
                } else {
                    stale.push(entity);
                }
            }
        }
    }
    for entity in stale.drain(..) {
        index.remove(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::mesh::PrimitiveTopology;

    #[test]
    fn marks_only_entities_referencing_modified_mesh_asset() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshAssetSyncPlugin));

        let (h_a, h_b) = {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Mesh::new(PrimitiveTopology::TriangleList, default())),
                meshes.add(Mesh::new(PrimitiveTopology::TriangleList, default())),
            )
        };
        let e_a = app.world_mut().spawn(Mesh3d(h_a.clone())).id();
        let e_b = app.world_mut().spawn(Mesh3d(h_b.clone())).id();

        // Prime initial ticks & index.
        app.update();
        app.update();

        let tick_before_a = app.world().entity(e_a).get_ref::<Mesh3d>().unwrap().last_changed();
        let tick_before_b = app.world().entity(e_b).get_ref::<Mesh3d>().unwrap().last_changed();

        // Mutate only mesh A (deref_mut triggers Bevy 0.20 AssetMutChangeNotifier).
        {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            let mut m = meshes.get_mut(&h_a).unwrap();
            let _ = &mut *m;
        }
        app.update();

        let tick_after_a = app.world().entity(e_a).get_ref::<Mesh3d>().unwrap().last_changed();
        let tick_after_b = app.world().entity(e_b).get_ref::<Mesh3d>().unwrap().last_changed();
        assert_ne!(tick_before_a, tick_after_a, "modified mesh entity must be marked changed");
        assert_eq!(tick_before_b, tick_after_b, "unmodified mesh entity must remain unchanged");

        // Swap entity B onto mesh A and despawn entity A; mutating mesh A marks only B.
        app.world_mut().entity_mut(e_b).insert(Mesh3d(h_a.clone()));
        app.world_mut().despawn(e_a);
        app.update();
        let tick_b_before_swap_mod = app.world().entity(e_b).get_ref::<Mesh3d>().unwrap().last_changed();
        {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            let mut m = meshes.get_mut(&h_a).unwrap();
            let _ = &mut *m;
        }
        app.update();
        let tick_b_after_swap_mod = app.world().entity(e_b).get_ref::<Mesh3d>().unwrap().last_changed();
        assert_ne!(tick_b_before_swap_mod, tick_b_after_swap_mod, "rebound entity must be marked changed");
    }
}
