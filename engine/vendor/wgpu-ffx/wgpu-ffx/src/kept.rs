//! A view's kept bind groups (`pass.rs`, `fg_pass.rs`).
//!
//! NVForever addition (2026-11-11). A kept bind group holds clones of the textures the caller
//! handed in. When the caller stops handing one in (Bevy makes new main textures while the
//! view lives on, for one), nothing asks for that bind group again, and until the list's cap
//! was reached it kept the texture alive: a display-sized texture or two for as long as the
//! view lasted. So each entry remembers the frame it was last used in, and one not used for
//! [`UNUSED_FRAMES`] frames is dropped.

/// How many frames an entry may go unused. More than the sets the passes alternate between
/// (the two parities, the caller's textures used in turn), so a steady state drops nothing.
/// A frame is one call of the view's dispatch (or prepare, which frame interpolation has too).
pub(crate) const UNUSED_FRAMES: u64 = 16;

pub(crate) struct Kept<K, V> {
    frame: u64,
    /// The key, the value and the frame it was last used in.
    entries: Vec<(K, V, u64)>,
}

impl<K, V> Default for Kept<K, V> {
    fn default() -> Self {
        Self { frame: 0, entries: Vec::new() }
    }
}

impl<K, V> Kept<K, V> {
    /// A frame begins: entries not used in the last [`UNUSED_FRAMES`] frames are dropped.
    pub fn next_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.entries.retain(|(_, _, used)| frame - *used <= UNUSED_FRAMES);
    }

    /// Where the entry `is` accepts lies, marked as used this frame.
    pub fn find(&mut self, is: impl Fn(&K) -> bool) -> Option<usize> {
        let at = self.entries.iter().position(|(key, ..)| is(key))?;
        self.entries[at].2 = self.frame;
        Some(at)
    }

    /// Adds an entry and returns where it lies. At `cap` entries the list starts again, so a
    /// caller that hands over new textures every frame does not grow it without end.
    pub fn push(&mut self, cap: usize, key: K, value: V) -> usize {
        if self.entries.len() >= cap {
            self.entries.clear();
        }
        self.entries.push((key, value, self.frame));
        self.entries.len() - 1
    }

    pub fn value(&self, at: usize) -> &V {
        &self.entries[at].1
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of a caller: every key of `keys` is asked for, and made if it is not kept.
    fn frame(kept: &mut Kept<u32, ()>, keys: &[u32]) -> usize {
        kept.next_frame();
        let mut made = 0;
        for key in keys {
            if kept.find(|k| k == key).is_none() {
                kept.push(usize::MAX, *key, ());
                made += 1;
            }
        }
        made
    }

    #[test]
    fn sets_used_in_turn_are_kept() {
        // Two parities and three textures in turn: six sets, each used every sixth frame.
        let mut kept = Kept::default();
        let mut made = 0;
        for n in 0..600u32 {
            made += frame(&mut kept, &[n % 6]);
        }
        assert_eq!(made, 6, "made once each, never again");
        assert_eq!(kept.len(), 6);
    }

    #[test]
    fn an_entry_no_longer_asked_for_is_dropped() {
        let mut kept = Kept::default();
        // The caller's first textures (key 1), then new ones (key 2) for good.
        frame(&mut kept, &[0, 1]);
        for n in 0..UNUSED_FRAMES {
            frame(&mut kept, &[0, 2]);
            assert!(kept.find(|k| *k == 0).is_some());
            assert_eq!(kept.len(), 3, "still kept {} frames after its last use", n + 1);
        }
        frame(&mut kept, &[0, 2]);
        assert_eq!(kept.len(), 2, "the old textures' entry is gone");
        assert!(kept.find(|k| *k == 0).is_some() && kept.find(|k| *k == 2).is_some());
    }

    #[test]
    fn an_entry_used_again_in_time_stays() {
        let mut kept = Kept::default();
        frame(&mut kept, &[7]);
        for _ in 0..10 {
            for _ in 0..UNUSED_FRAMES - 1 {
                frame(&mut kept, &[]);
            }
            assert_eq!(frame(&mut kept, &[7]), 0, "found, not made again");
        }
    }

    #[test]
    fn the_cap_still_bounds_new_textures_every_frame() {
        let mut kept = Kept::default();
        for n in 0..100u32 {
            kept.next_frame();
            for pass in 0..10 {
                kept.push(128, n * 10 + pass, ());
                assert!(kept.len() <= 128);
            }
        }
    }

    #[test]
    fn where_an_entry_lies_holds_until_the_next_frame() {
        let mut kept = Kept::default();
        kept.next_frame();
        let at = kept.push(8, 1u32, "one");
        let other = kept.push(8, 2u32, "two");
        assert_eq!((*kept.value(at), *kept.value(other)), ("one", "two"));
        assert_eq!(kept.find(|k| *k == 2), Some(other));
    }
}
