//! SetMaxAniso source intent reaches the existing DDS image sampler consumer.
use crate::{script::ScriptState, PlaySet};
use bevy::prelude::*;
use nvf_assets::texture::TextureCache;

pub(crate) struct ScriptTextureFilterPlugin;
impl Plugin for ScriptTextureFilterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply.after(PlaySet::Scripts).before(PlaySet::Ui));
    }
}

fn apply(
    script: Option<ResMut<ScriptState>>,
    cache: Option<ResMut<TextureCache>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let (Some(mut script), Some(mut cache), Some(mut images)) = (script, cache, images) else {
        return;
    };
    let Some(rt) = script.rt.as_mut() else { return };
    if let Some(level) = rt.game.take_texture_filter_level() {
        cache.apply_anisotropy(&mut images, level);
    }
}

#[cfg(test)]
mod tests;
