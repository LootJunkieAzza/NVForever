//! Where a drawn texture came from: its file, so that a worker can read it a second time
//! (docs/design/rtgi-collect.md). Nothing in the game asks yet.

use super::{split_cube, TextureCache, TextureUsage};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// The file a texture of the cache was made from, and as what.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageSource {
    /// Its path in the VFS, resolved (`resolved_path`).
    pub path: String,
    pub usage: TextureUsage,
    /// Whether it is the copy with clamped edges.
    pub clamped: bool,
}

impl TextureCache {
    /// Where these images came from, those that are held here or that a worker decoded. `None`
    /// for a cube map, and for one that two keys claim, which no loader here makes: it is not
    /// known as one flat texture of one file.
    pub fn sources(&self, wanted: &HashSet<AssetId<Image>>) -> HashMap<AssetId<Image>, Option<ImageSource>> {
        let mut out: HashMap<AssetId<Image>, Option<ImageSource>> = HashMap::new();
        let mut add = |(path, usage): &(String, TextureUsage), id: AssetId<Image>, clamped: bool| {
            if !wanted.contains(&id) {
                return;
            }
            let (path, cube) = split_cube(path);
            let source = (!cube).then(|| ImageSource { path: path.into(), usage: *usage, clamped });
            let held = out.entry(id).or_insert_with(|| source.clone());
            if *held != source {
                *held = None;
            }
        };
        for (clamped, map) in [(false, &self.map), (true, &self.clamped)] {
            for (key, (handle, _)) in map.iter().filter_map(|(k, h)| Some((k, h.as_ref()?))) {
                add(key, handle.id(), clamped);
            }
        }
        for (key, w) in self.workers.entries.lock().expect("worker textures").iter() {
            add(key, w.handle.id(), false);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::bc5_tests::OwnedData;
    use super::super::{worker_texture, TextureSnapshot};
    use super::*;

    /// The three places an image is held at: the cache's list, its clamped copies and the
    /// workers' entries, before and after they are taken in. A cube map is refused, and so is
    /// an id of two keys.
    #[test]
    fn a_source_is_found_wherever_the_image_is_held() {
        let owned = OwnedData::new();
        let (mut cache, mut images) = (TextureCache::default(), Assets::<Image>::default());
        let plain = cache.get("old.dds", true, &owned.data, &mut images, None).unwrap();
        let clamped = cache.get_clamped("old.dds", true, &owned.data, &mut images, None).unwrap();
        let worker = worker_texture("signed.dds", TextureUsage::Normal, &owned.data, &TextureSnapshot::new(), &cache.workers, &images.get_handle_provider(), true).unwrap().0;
        // The file holds no cube map, so the key's entry is made by hand.
        let cube = images.add(Image::default());
        cache.map.insert((super::super::cube_path(r"textures\old.dds"), TextureUsage::Color), Some((cube.clone(), false)));
        let (torn, other) = (images.add(Image::default()), images.add(Image::default()));
        for usage in [TextureUsage::Color, TextureUsage::Linear] {
            cache.map.insert((r"textures\torn.dds".into(), usage), Some((torn.clone(), false)));
        }
        cache.map.insert((r"textures\other.dds".into(), TextureUsage::Color), Some((other, false)));
        cache.map.insert((r"textures\gone.dds".into(), TextureUsage::Color), None);
        let wanted = HashSet::from([&plain, &clamped, &worker, &cube, &torn].map(|h| h.id()));
        let at = |path: &str, usage, clamped| Some(ImageSource { path: format!(r"textures\{path}"), usage, clamped });
        let expected = HashMap::from([
            (plain.id(), at("old.dds", TextureUsage::Color, false)),
            (clamped.id(), at("old.dds", TextureUsage::Color, true)),
            (worker.id(), at("signed.dds", TextureUsage::Normal, false)),
            (cube.id(), None),
            (torn.id(), None),
        ]);
        assert_eq!(cache.sources(&wanted), expected);
        cache.take_workers(&mut images, 1.0);
        assert_eq!(cache.sources(&wanted), expected, "held in the list and by the workers");
    }
}
