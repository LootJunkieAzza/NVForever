//! NVForever specialization scan: indexes that let `check_entities_needing_specialization`
//! and `mark_meshes_as_changed_if_their_materials_changed` (material.rs) visit only the
//! entities that hold a changed asset, instead of every entity with the components.
//!
//! Upstream filters those systems with `AssetChanged<Mesh3d>` and
//! `AssetChanged<MeshMaterial3d<M>>`. That filter is not archetypal: when any asset of the
//! type changed since the system last ran, it reads every matching entity's handle and
//! looks the id up in a map of every live asset (bevy_asset 0.20.1 `asset_changed.rs`). A
//! live particle mesh changes every frame, so the systems walked every mesh entity every
//! frame (engine/vendor/README.md has the measurement).
//!
//! The replacement reads the asset events the system has not seen yet, which name the
//! changed ids, and finds their holders in an index of asset id -> entities kept per
//! system. The index is brought up to date at the start of each run from the entities
//! whose components changed or were removed since the last run, so it always holds the
//! handles as they are when the system runs.

use bevy_asset::{Asset, AssetEvent, AssetId};
use bevy_ecs::{
    entity::{Entity, EntityHashMap, EntityHashSet},
    message::MessageReader,
};
use bevy_platform::collections::{hash_map::Entry, HashMap, HashSet};

/// Which entities hold which asset, in one direction and the other.
pub struct AssetHolders<A: Asset> {
    by_asset: HashMap<AssetId<A>, EntityHashSet>,
    by_entity: EntityHashMap<AssetId<A>>,
}

impl<A: Asset> Default for AssetHolders<A> {
    fn default() -> Self {
        Self {
            by_asset: HashMap::default(),
            by_entity: EntityHashMap::default(),
        }
    }
}

impl<A: Asset> AssetHolders<A> {
    /// Records that `entity` now holds `id` (`None`: no asset of this type, or no entity).
    pub fn set(&mut self, entity: Entity, id: Option<AssetId<A>>) {
        let old = match id {
            Some(id) => self.by_entity.insert(entity, id),
            None => self.by_entity.remove(&entity),
        };
        if old == id {
            return;
        }
        if let Some(old) = old
            && let Entry::Occupied(mut holders) = self.by_asset.entry(old)
        {
            holders.get_mut().remove(&entity);
            if holders.get().is_empty() {
                holders.remove();
            }
        }
        if let Some(id) = id {
            self.by_asset.entry(id).or_default().insert(entity);
        }
    }

    /// The entities recorded as holding `id`.
    pub fn holders(&self, id: &AssetId<A>) -> impl Iterator<Item = Entity> + '_ {
        self.by_asset.get(id).into_iter().flatten().copied()
    }

    /// How many entities are recorded.
    pub fn len(&self) -> usize {
        self.by_entity.len()
    }

    /// Whether no entity is recorded.
    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }
}

/// Reads the asset events this system has not seen and leaves in `out` the ids that
/// `AssetChanged` would call changed since the system's last run.
///
/// [design, bevy_asset 0.20.1 `assets.rs` `Assets::asset_events`] `AssetChanged` looks ids
/// up in `AssetChanges<A>`, which that system fills from the same queue it writes these
/// events from, in the same order: `Added` and `Modified` store the id with the run's tick,
/// `Removed` and `Unused` drop it. An id counts as changed when its stored tick is newer
/// than the reader's last run, which holds exactly for the ids stored by the runs since
/// then and not dropped after. `LoadedWithDependencies` never goes through that queue (the
/// asset server writes it straight to the messages, `server/mod.rs` `register_asset`), so
/// it is not a change here either. A system reading these events every frame after
/// `AssetEventSystems` sees the events of exactly those runs.
pub fn read_changed_assets<A: Asset>(
    events: &mut MessageReader<AssetEvent<A>>,
    out: &mut HashSet<AssetId<A>>,
) {
    out.clear();
    for event in events.read() {
        match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => {
                out.insert(*id);
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                out.remove(id);
            }
            AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
}
