//! Where a drawn mesh came from: its model's file and its shape's block there, so that a
//! worker can read it a second time (docs/design/rtgi-collect.md). Nothing in the game asks
//! yet.

use super::ModelCache;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// The file and the shape a mesh of the model cache was made from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshSource {
    /// The model's path in the VFS.
    pub path: String,
    /// The shape's block in that file.
    pub block: u32,
    /// The shape's name, empty when it has none.
    pub name: String,
}

impl ModelCache {
    /// Where these meshes came from, those that a model held here has among its shapes or its
    /// rig's. `None` for one that two of them claim from different places, which no loader here
    /// makes: it is not known.
    pub fn sources(&self, wanted: &HashSet<AssetId<Mesh>>) -> HashMap<AssetId<Mesh>, Option<MeshSource>> {
        let mut out: HashMap<AssetId<Mesh>, Option<MeshSource>> = HashMap::new();
        for (key, model) in &self.map {
            let Some(model) = model else { continue };
            // The marks of the variants follow the path (`model_key`).
            let path = key.split_once('|').map_or(key.as_str(), |(path, _)| path);
            let shapes = model.shapes.iter().map(|s| (s.mesh.id(), s.block, s.name.as_deref().unwrap_or_default()));
            let rig = model.rig.iter().flat_map(|r| &r.shapes).map(|s| (s.mesh.id(), s.block, s.look.0.as_str()));
            for (id, block, name) in shapes.chain(rig).filter(|(id, ..)| wanted.contains(id)) {
                let same = |s: &MeshSource| s.path == path && s.block == block && s.name == name;
                match out.get_mut(&id) {
                    Some(held) if held.as_ref().is_some_and(same) => {}
                    Some(held) => *held = None,
                    None => {
                        out.insert(id, Some(MeshSource { path: path.into(), block, name: name.into() }));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PreparedModel, PreparedShape};
    use std::sync::Arc;

    fn model(mesh: &Handle<Mesh>, block: u32) -> Option<Arc<PreparedModel>> {
        let shape = PreparedShape {
            mesh: mesh.clone(),
            material: Handle::default(),
            transform: default(),
            name: Some("Wall:0".into()),
            ancestors: Vec::new(),
            node: None,
            own: default(),
            cpu: None,
            segments: Vec::new(),
            external_emittance: false,
            look: crate::material_anim::Look::stored([0.0; 3], 1.0, 1.0),
            missing_diffuse: None,
            block,
        };
        Some(Arc::new(PreparedModel { shapes: vec![shape], ..default() }))
    }

    /// The path ends at the key's first mark, whatever follows. An id from two places is not
    /// known, in whichever order the map gives them and however often; one that is not asked
    /// for is left out.
    #[test]
    fn a_source_is_the_path_before_the_marks_and_one_place_only() {
        let mut meshes = Assets::<Mesh>::default();
        let mut made = || meshes.add(Mesh::from(Cuboid::default()));
        let (plain, marked, twice, torn, other) = (made(), made(), made(), made(), made());
        let mut cache = ModelCache::default();
        let a = r"meshes\a.nif";
        let mut put = |key: String, m: &Handle<Mesh>, block| cache.map.insert(key, model(m, block));
        put(a.into(), &plain, 3);
        put(format!("{a}|1stperson|cpu|swap0123456789abcdef"), &marked, 3);
        put(format!("{a}|attachment"), &twice, 4);
        put(format!("{a}|actor-idle-00012345"), &twice, 4);
        put(r"meshes\b.nif".into(), &torn, 4);
        put(r"meshes\b.nif|cpu".into(), &torn, 5);
        put(r"meshes\c.nif".into(), &torn, 4);
        put(r"meshes\d.nif".into(), &other, 1);
        cache.map.insert(r"meshes\gone.nif".into(), None);
        let wanted = HashSet::from([plain.id(), marked.id(), twice.id(), torn.id(), made().id()]);
        let at = |block| Some(MeshSource { path: a.into(), block, name: "Wall:0".into() });
        let expected = HashMap::from([(plain.id(), at(3)), (marked.id(), at(3)), (twice.id(), at(4)), (torn.id(), None)]);
        assert_eq!(cache.sources(&wanted), expected);
    }
}
