//! Retired scene-clone provenance, not persistent temporary base forms.
//!
//! An inert created reference can leave a tombstone after its model is deleted.
//! Keep that identity reconstructible without making the temporary base valid again.
//! See docs/spec-notes/temporary-scene-bases.md for the bounded save policy.

use crate::game::{Game, GameSnapshot, RetiredSceneBase};
use crate::nvse_data::{CLONE_BASE, CLONE_LOCAL};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const FORMAT: &str = "fnv.game.state/2-retired-scenes";
// Existing runtime-compiled units start at FE200000. Local clones must also stay
// inside their existing index-zero allocation range. These are allocator limits,
// not original-game format constants.
pub(crate) const MAX_SEQUENCE: u32 = crate::game::COMPILED_BASE - CLONE_BASE - 1;

fn sequence(alias: FormId) -> Option<u32> {
    let id = alias.0;
    if (CLONE_LOCAL + 1..=0x00FF_FFFF).contains(&id) {
        Some(id - CLONE_LOCAL)
    } else if (CLONE_BASE + 1..crate::game::COMPILED_BASE).contains(&id) {
        Some(id - CLONE_BASE)
    } else {
        None
    }
}

impl GameSnapshot {
    pub(crate) fn check_retired_scene_bases(&self) -> Result<(), String> {
        if self.retired_scene_bases.is_empty() {
            if self.format == FORMAT {
                return Err(format!("unsupported snapshot format {:?}: retired scene provenance is absent", self.format));
            }
            if self.format != crate::game::SNAPSHOT_FORMAT || self.clone_sequence_floor.is_some() {
                return Err("retired scene save format requires provenance and an identity reservation".into());
            }
            return Ok(());
        }
        if self.format != FORMAT {
            return Err("retired scene provenance requires the versioned snapshot format".into());
        }
        let floor = self.clone_sequence_floor.filter(|n| *n > 0 && *n <= MAX_SEQUENCE)
            .ok_or("invalid retired scene clone sequence reservation")?;
        let bases: BTreeSet<_> = self.sim.deleted_created.values().copied().collect();
        for (&alias, descriptor) in &self.retired_scene_bases {
            let seq = sequence(alias).ok_or("invalid retired scene alias identity")?;
            if seq > floor || !bases.contains(&alias)
                || descriptor.original.0 == 0 || descriptor.original == alias
                || self.retired_scene_bases.contains_key(&descriptor.original)
                || self.sim.refs.contains_key(&alias) || self.sim.deleted_created.contains_key(&alias)
                || !matches!(&descriptor.kind, b"STAT" | b"MSTT" | b"PROJ")
                || self.sim.refs.values().any(|state| state.base == alias)
                || self.sim.deleted_created.iter().any(|(id, base)| *base == alias && (!id.is_dynamic() || self.sim.refs.contains_key(id) || id.0 >= self.sim.next_dynamic))
            {
                return Err("inconsistent retired scene provenance or live reference".into());
            }
        }
        Ok(())
    }
}

impl Game<'_> {
    pub(crate) fn retired_scene_snapshot(&self, sim: &fnv_sim::world::Snapshot) -> BTreeMap<FormId, RetiredSceneBase> {
        let bases: BTreeSet<_> = sim.deleted_created.values().copied().collect();
        let mut saved: BTreeMap<_, _> = self.retired_scene_bases.iter()
            .filter(|(alias, _)| bases.contains(*alias)).map(|(&alias, value)| (alias, value.clone())).collect();
        for alias in bases {
            if let Some(descriptor) = self.world.temporary_scene_base(alias) {
                saved.insert(alias, RetiredSceneBase {
                    original: descriptor.original,
                    kind: self.data.sig(descriptor.original).unwrap_or_default(),
                    model_override: descriptor.model_override.clone(),
                });
            }
        }
        saved
    }

    pub(crate) fn validate_retired_scene_bases(&self, snapshot: &GameSnapshot) -> Result<(), String> {
        for (&alias, descriptor) in &snapshot.retired_scene_bases {
            if self.data.sig(alias).is_some() || self.world.forms.sig(alias).is_some()
                || self.data.sig(descriptor.original) != Some(descriptor.kind)
                || self.world.forms.sig(descriptor.original) != Some(descriptor.kind)
                || self.data.script_of_base(descriptor.original).is_some()
                || self.base_script(descriptor.original).is_some()
                || self.clones.contains_key(&descriptor.original)
            {
                return Err(format!("retired scene base {alias} has a conflicting or unsupported original"));
            }
            if let Some(current) = self.world.temporary_scene_base(alias) {
                if self.clones.get(&alias) != Some(&descriptor.original)
                    || current.original != descriptor.original || current.model_override != descriptor.model_override
                {
                    return Err(format!("retired scene base {alias} has inconsistent runtime provenance"));
                }
            } else if self.clones.contains_key(&alias) {
                return Err(format!("retired scene base {alias} has no admitted scene descriptor"));
            }
        }
        Ok(())
    }

    /// Reserve past clone IDs without restoring the corresponding temporary forms.
    pub(crate) fn next_clone_identity(&mut self, local: bool) -> Result<FormId, String> {
        loop {
            let next = self.clone_seq.checked_add(1).filter(|n| *n <= MAX_SEQUENCE)
                .ok_or("temporary clone identity space exhausted")?;
            let id = if local {
                CLONE_LOCAL.checked_add(next).filter(|id| *id <= 0x00FF_FFFF)
            } else {
                CLONE_BASE.checked_add(next)
            }.ok_or("temporary clone identity space exhausted")?;
            self.clone_seq = next;
            let id = FormId(id);
            if self.data.sig(id).is_none() && self.world.forms.sig(id).is_none()
                && !self.clones.contains_key(&id) && !self.retired_scene_bases.contains_key(&id)
            {
                return Ok(id);
            }
        }
    }
}
