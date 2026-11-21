//! One-shot filtering changes for currently cache-owned DDS images.
use super::{set_anisotropy, TextureCache, ANISOTROPY};
use bevy::{
    image::{ImageFilterMode, ImageSampler},
    prelude::*,
};
use std::{collections::HashSet, sync::atomic::Ordering};

fn change(image: &mut Image, level: u16) -> bool {
    let ImageSampler::Descriptor(s) = &mut image.sampler else {
        return false;
    };
    // Anisotropy above 1 requires linear sampling. Never rewrite a foreign nearest
    // sampler's filters to make it eligible, or invent a descriptor for Default.
    if s.mag_filter != ImageFilterMode::Linear
        || s.min_filter != ImageFilterMode::Linear
        || s.mipmap_filter != ImageFilterMode::Linear
        || s.anisotropy_clamp == level
    {
        return false;
    }
    s.anisotropy_clamp = level;
    true
}

/// Workers can finish decoding before the last filtering request. Select the
/// current policy at both actual publication paths, preserving their own handles.
pub(super) fn publish(image: &mut Image) {
    change(image, ANISOTROPY.load(Ordering::Relaxed));
}

impl TextureCache {
    /// Set the existing future-DDS policy and update eligible images currently held
    /// by this cache/worker pool. Other samplers/fields and unrelated images stay
    /// untouched. This is a command, not a held overlay or a restoration authority.
    pub fn apply_anisotropy(&mut self, images: &mut Assets<Image>, level: u16) -> usize {
        let level = level.clamp(1, 16);
        set_anisotropy(level);
        let mut ids: HashSet<_> = self
            .map
            .values()
            .chain(self.clamped.values())
            .flatten()
            .map(|(h, _)| h.id())
            .collect();
        {
            let workers = self.workers.entries.lock().expect("worker textures");
            ids.extend(workers.values().map(|w| w.handle.id()));
        }
        let mut changed = 0;
        for id in ids {
            if let Some(mut image) = images.get_mut(id) {
                changed += usize::from(change(&mut image, level));
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
