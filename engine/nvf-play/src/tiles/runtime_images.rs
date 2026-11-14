//! Pictures made at run time for `image` tiles: a texture NVForever's code makes (the Local
//! Map's outline picture, `crate::pipboy::data::outline`) drawn where a menu file has an
//! image whose picture the original's code sets [design: docs/design/ui-tile-tree.md §6.1,
//! docs/spec-notes/menu-xml.md §3.1.5].
//!
//! - **The name**: a tile's `filename` that starts with [`PREFIX`] (`nvf:`, any case) names an
//!   entry of [`RuntimeImages`] instead of a game file. No game path starts so: the menu
//!   files name pictures under `textures\` by relative paths, and in the Windows paths the
//!   archives and the Data folder hold a colon only follows a drive letter [design]. The
//!   name is resolved before the atlas indexes and the files are looked in, and never enters
//!   the file-name cache (`crate::interface::Pictures`).
//! - **Repainting**: each [`RuntimeImages::set`] (and [`RuntimeImages::touch`]) is a new
//!   revision. The renderer stamps a runtime picture's piece with its entry's revision and
//!   size before comparing the frame's pieces with the last ones ([`mark`]), so a new handle,
//!   the same handle's texels changed, an entry that comes late, goes, or changes size
//!   repaints the piece although its name stays the same.
//! - **Not read from a runtime picture** [design]: the `tile` repeats are not baked into one
//!   image (`crate::repeat` reads files): Bevy's own tiles draw them, as for a picture whose
//!   texels cannot be read; SetUITexOffset and SetUIAlphaMap (`super::texmod`, which read the
//!   texels from files) are not applied; each said once in the log.
//! - A name with no entry draws nothing and says so once.
//!
//! The way code uses it: `let name = images.set("local_map", handle, size);` then the tile's
//! `filename` written to `name` (`nvf:local_map`); set the entry before writing the name, or
//! the first frame says the entry is missing.

use super::draw::{Look, Piece, Source};
use crate::interface::SizedPicture;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// The `filename` prefix of a runtime picture [design: see the module notes].
pub const PREFIX: &str = "nvf:";

/// Whether a tile's `filename` names a runtime picture ([`PREFIX`], any case, spaces around
/// ignored as the renderer trims them).
pub fn is_runtime(filename: &str) -> bool {
    filename.trim().get(..PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(PREFIX))
}

/// The `filename` a tile is given for the runtime picture `name` (with or without the
/// prefix).
pub fn filename(name: &str) -> String {
    let name = name.trim();
    if is_runtime(name) { name.to_string() } else { format!("{PREFIX}{name}") }
}

/// The key an entry is kept by: the whole filename, in small letters.
fn key(name: &str) -> String {
    filename(name).to_ascii_lowercase()
}

/// One runtime picture: its texture, its size in texels and its revision.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeImage {
    pub image: Handle<Image>,
    pub size: Vec2,
    pub revision: u64,
}

/// What a runtime picture's piece is compared by from frame to frame ([`mark`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub revision: u64,
    pub size: Vec2,
}

/// The runtime pictures by name, set by the code that makes them.
#[derive(Resource, Default, Debug)]
pub struct RuntimeImages {
    entries: HashMap<String, RuntimeImage>,
    /// The last revision given, over every name, so an entry removed and set again never
    /// gets a revision it had.
    revision: u64,
}

impl RuntimeImages {
    fn next(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Give `name` (with or without [`PREFIX`]) the texture `image` of `size` texels (the
    /// image's own size: a `tile` picture's repeats are as large as Bevy finds the texture),
    /// and return the `filename` to write into the tile. Each call is a new revision: the piece
    /// repaints even when the handle is the same one.
    pub fn set(&mut self, name: &str, image: Handle<Image>, size: Vec2) -> String {
        let revision = self.next();
        self.entries.insert(key(name), RuntimeImage { image, size, revision });
        filename(name)
    }

    /// The texels of `name`'s texture changed in place (the same handle): a new revision.
    /// False when there is no such entry.
    pub fn touch(&mut self, name: &str) -> bool {
        let revision = self.next();
        match self.entries.get_mut(&key(name)) {
            Some(entry) => {
                entry.revision = revision;
                true
            }
            None => false,
        }
    }

    /// Take `name`'s entry away: its tiles draw nothing. False when there was none.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(&key(name)).is_some()
    }

    /// The entry a tile's `filename` names.
    pub fn get(&self, filename: &str) -> Option<&RuntimeImage> {
        self.entries.get(&key(filename))
    }

    /// What a piece of `filename` is compared by: none without an entry.
    pub fn stamp(&self, filename: &str) -> Option<Stamp> {
        self.get(filename).map(|e| Stamp { revision: e.revision, size: e.size })
    }
}

/// What a drawing has said once about its runtime pictures (one [`super::draw`] drawing's).
#[derive(Default, Debug)]
pub struct Told {
    missing: HashSet<String>,
    repeats: HashSet<String>,
    texmod: HashSet<String>,
}

impl Told {
    /// Whether the missing entry `name` was said.
    pub fn missing(&self, name: &str) -> bool {
        self.missing.contains(&key(name))
    }

    /// Whether `name`'s unbaked repeats were said.
    pub fn repeats(&self, name: &str) -> bool {
        self.repeats.contains(&key(name))
    }

    /// Whether `name`'s dropped texture offset or alpha map was said.
    pub fn texmod(&self, name: &str) -> bool {
        self.texmod.contains(&key(name))
    }
}

/// Stamp each runtime picture's piece with its entry's revision and size (none without one),
/// so a change repaints it, and drop a texture offset or alpha map from it, saying so once
/// ([`super::texmod`] reads files). Run before the pieces are compared with the last frame's.
pub fn mark(pieces: &mut [Piece], images: Option<&RuntimeImages>, told: &mut Told) {
    for p in pieces {
        let Look::Picture { name, source: Source::Runtime(stamp), .. } = &mut p.look else { continue };
        *stamp = images.and_then(|i| i.stamp(name));
        if p.tex.take().is_some() && told.texmod.insert(key(name)) {
            warn!("tiles: the runtime picture {name} takes no texture offset or alpha map (they are read from files); drawn without");
        }
    }
}

/// The picture of a runtime piece named `name`: its texture, no atlas cut, its size. None
/// for a name with no entry, said once.
pub fn picture(images: Option<&RuntimeImages>, name: &str, told: &mut Told) -> Option<SizedPicture> {
    match images.and_then(|i| i.get(name)) {
        Some(entry) => Some((entry.image.clone(), None, entry.size)),
        None => {
            if told.missing.insert(key(name)) {
                warn!("tiles: no runtime picture {name} is set; its tile draws nothing until one is");
            }
            None
        }
    }
}

/// Say once that a runtime picture's `tile` repeats are drawn by Bevy's tiles, not baked.
pub fn unbaked(name: &str, told: &mut Told) {
    if told.repeats.insert(key(name)) {
        info!("tiles: the runtime picture {name} is repeated by Bevy's tiles (only files' repeats are baked into one image)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_names_a_runtime_picture_in_any_case() {
        assert!(is_runtime("nvf:local_map") && is_runtime(" NVF:Local_Map ") && !is_runtime("Interface\\nvf.dds") && !is_runtime("nv"));
        assert_eq!(filename("local_map"), "nvf:local_map");
        assert_eq!(filename(" nvf:local_map "), "nvf:local_map");
    }

    #[test]
    fn every_set_is_a_new_revision_and_a_removed_name_never_gets_an_old_one() {
        let mut images = RuntimeImages::default();
        let (a, b) = (Handle::<Image>::default(), Handle::<Image>::default());
        assert_eq!(images.set("map", a.clone(), Vec2::new(64.0, 32.0)), "nvf:map");
        let first = images.stamp("NVF:Map").unwrap();
        images.set("nvf:map", b, Vec2::new(64.0, 32.0));
        let second = images.stamp("nvf:map").unwrap();
        assert!(second.revision > first.revision);
        assert!(images.touch("map"));
        assert!(images.stamp("nvf:map").unwrap().revision > second.revision);
        assert!(images.remove("map") && images.stamp("nvf:map").is_none() && !images.remove("map") && !images.touch("map"));
        images.set("map", a, Vec2::new(64.0, 32.0));
        assert!(images.stamp("nvf:map").unwrap().revision > second.revision + 1);
    }

    #[test]
    fn a_missing_entry_draws_nothing_and_is_told_once() {
        let mut told = Told::default();
        assert!(picture(None, "nvf:gone", &mut told).is_none());
        assert!(told.missing("nvf:gone"));
        let images = RuntimeImages::default();
        assert!(picture(Some(&images), "NVF:Gone", &mut told).is_none());
        assert_eq!(told.missing.len(), 1, "said once, whatever the case");
        let mut images = images;
        let h = Handle::<Image>::default();
        images.set("gone", h.clone(), Vec2::new(8.0, 4.0));
        assert_eq!(picture(Some(&images), "nvf:gone", &mut told), Some((h, None, Vec2::new(8.0, 4.0))));
    }
}
