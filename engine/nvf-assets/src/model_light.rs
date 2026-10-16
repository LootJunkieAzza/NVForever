//! The light a model holds: the colour of the first `NiPointLight` or `NiSpotLight` in a
//! LIGH's model (docs/spec-notes/world.md §8 "Black LIGH colours").

use crate::model::ModelCtx;
use fnv_nif::scene::Decoded;
use fnv_nif::value::Struct;
use fnv_nif::NifFile;

/// The diffuse colour times the dimmer of the first point or spot light in a decoded
/// file, linear 0–1; `None` when it has none or the colour is black.
pub fn first_light_colour<'a>(blocks: impl IntoIterator<Item = (&'a str, &'a Struct)>) -> Option<[f32; 3]> {
    blocks.into_iter().filter(|(t, _)| matches!(*t, "NiPointLight" | "NiSpotLight")).find_map(|(_, b)| {
        let c = b.floats("diffuse_color").filter(|c| c.len() >= 3)?;
        let dimmer = b.float("dimmer").unwrap_or(1.0).max(0.0);
        let c = [c[0] * dimmer, c[1] * dimmer, c[2] * dimmer];
        (c.iter().any(|&v| v > 0.0)).then_some(c)
    })
}

impl ModelCtx<'_> {
    /// The colour of the light inside the model at `path` ([`first_light_colour`]),
    /// read on first use and remembered.
    pub fn model_light_colour(&mut self, path: &str) -> Option<[f32; 3]> {
        let key = fnv_vfs::normalize(path);
        if let Some(c) = self.cache.light_colours.get(&key) {
            return *c;
        }
        let c = (|| {
            let bytes = self.data.read(&key)?;
            let schema = self.data.schema.clone();
            let file = NifFile::parse(&bytes, Some(&schema)).ok()?;
            let dec = Decoded::new(&file, &bytes, &schema);
            first_light_colour(file.blocks.iter().zip(dec.blocks.iter()).filter_map(|(b, d)| Some((b.type_name.as_str(), d.as_ref().ok()?))))
        })();
        self.cache.light_colours.insert(key, c);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_nif::value::Value;
    use std::sync::Arc;

    fn light(c: [f32; 3], dimmer: f32) -> Struct {
        Struct { fields: vec![(Arc::from("dimmer"), Value::Float(dimmer)), (Arc::from("diffuse_color"), Value::Floats(c.to_vec()))] }
    }

    #[test]
    fn the_first_lit_light_gives_the_colour() {
        let ambient = light([1.0, 1.0, 1.0], 1.0);
        let black = light([0.0; 3], 1.0);
        let warm = light([0.78, 0.58, 0.16], 0.5);
        let c = first_light_colour([("NiAmbientLight", &ambient), ("NiPointLight", &black), ("NiPointLight", &warm)]).unwrap();
        assert!((c[0] - 0.39).abs() < 1e-6 && (c[2] - 0.08).abs() < 1e-6, "{c:?}");
        assert_eq!(first_light_colour([("NiAmbientLight", &ambient)]), None);
    }
}
