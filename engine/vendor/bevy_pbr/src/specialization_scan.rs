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
//!
//! Gaps: the events and the removals are messages, which expire after two buffer updates;
//! `AssetChanged` keeps a tick per asset and never loses one. A system that did not run
//! for long enough (a run condition, a schedule not run) finds messages it never read
//! gone ([`asset_events_missed`], [`removals_missed`]). It then rebuilds its index from
//! the world and treats every holder as changed for that run: more entities than upstream
//! may be flagged in that frame (each is specialized again, with the same result), none
//! fewer.
//!
//! Only Bevy's `Disabled` is let into the indexes (`Allow<Disabled>`): an entity hidden by
//! another disabling component (`World::register_disabling_component`; NVForever registers
//! none) whose handle changes while hidden keeps a stale entry until its component changes
//! again. The entries are checked against the component before an entity is flagged, so a
//! stale one never flags a wrong entity; it can only miss one.

use bevy_asset::{Asset, AssetEvent, AssetId};
use bevy_ecs::{
    component::Component,
    entity::{Entity, EntityHashMap, EntityHashSet},
    lifecycle::RemovedComponents,
    message::{MessageCursor, Messages},
};
use bevy_platform::collections::{hash_map::Entry, HashMap, HashSet};

/// Above this many slots, a cleared set is shrunk, so a burst (a cell load names thousands
/// of new assets) does not leave every later frame clearing a large table.
const KEEP_CAPACITY: usize = 1024;

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

    /// Forgets every entity (before a rebuild).
    pub fn clear(&mut self) {
        self.by_asset.clear();
        self.by_entity.clear();
    }

    /// The entities recorded as holding `id`.
    pub fn holders(&self, id: &AssetId<A>) -> impl Iterator<Item = Entity> + '_ {
        self.by_asset.get(id).into_iter().flatten().copied()
    }

    /// Every entity recorded.
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.by_entity.keys().copied()
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

/// Whether messages this cursor has not read have already expired.
pub fn asset_events_missed<A: Asset>(
    cursor: &MessageCursor<AssetEvent<A>>,
    events: &Messages<AssetEvent<A>>,
) -> bool {
    cursor.missed_messages(events) > 0
}

/// Whether removals this reader has not read have already expired.
pub fn removals_missed<T: Component>(removed: &RemovedComponents<T>) -> bool {
    removed
        .messages()
        .is_some_and(|messages| removed.reader().missed_messages(messages) > 0)
}

/// Empties a set, shrinking it when a burst left it large.
pub fn clear_set<T: core::hash::Hash + Eq, S: core::hash::BuildHasher>(set: &mut HashSet<T, S>) {
    set.clear();
    if set.capacity() > KEEP_CAPACITY {
        set.shrink_to(KEEP_CAPACITY / 4);
    }
}

/// Reads the asset events `cursor` has not seen and leaves in `out` the ids that
/// `AssetChanged` would call changed since the system's last run.
///
/// [design, bevy_asset 0.20.1 `assets.rs` `Assets::asset_events`] `AssetChanged` looks ids
/// up in `AssetChanges<A>`, which that system fills from the same queue it writes these
/// events from, in the same order: `Added` and `Modified` store the id with the run's tick,
/// `Removed` and `Unused` drop it. An id counts as changed when its stored tick is newer
/// than the reader's last run, which holds exactly for the ids stored by the runs since
/// then and not dropped after. `LoadedWithDependencies` never goes through that queue (the
/// asset server writes it straight to the messages, `server/mod.rs` `register_asset`), so
/// it is not a change here either. A system that reads these events after
/// `AssetEventSystems` sees the events of exactly those runs, unless some expired before
/// it read them: callers check [`asset_events_missed`] first.
pub fn read_changed_assets<A: Asset>(
    cursor: &mut MessageCursor<AssetEvent<A>>,
    events: &Messages<AssetEvent<A>>,
    out: &mut HashSet<AssetId<A>>,
) {
    clear_set(out);
    for event in cursor.read(events) {
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
