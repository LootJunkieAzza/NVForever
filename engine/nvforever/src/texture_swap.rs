//! Textures scripts swap on references' shapes (`SwapTexture`, playtest list 5 item 38;
//! `fnv_game::functions::misc::swap_texture`, docs/design/script-stubs.md §4): the
//! drawn shapes of the reference whose model shape has the name the call gave get a
//! copy of their material with the new diffuse texture. Done again when a reference is
//! drawn anew (a cell loaded again), as the swaps are kept per reference.
//!
//! The shape is found by its material: the drawn entities of a reference (`RefVisual`)
//! carry the material of the model's shape they draw [design: no shape names are kept
//! on drawn entities of placed objects; a shape whose material was copied (animated,
//! emittance) is not found]. The shapes of actors' parts (a Securitron's screen,
//! `SetSecuritronExpression`) carry their name (`nvf_assets::actor::ShapeName`) and are
//! found by it.
//!
//! A mesh drawn see-through wears a copy of its material: SetActorAlpha's
//! (`crate::effect_shaders`, `Faded`) or a fade's (`nvf_play::fade::Fading`). The swap
//! goes to the mesh's own material, the one kept aside, so the end of the alpha or the
//! fade does not bring the old texture back (the psych test's card fades in as Doc
//! Mitchell's line swaps its texture, smoke test `rorschach-cards`): `Faded` takes the
//! swapped material as its own and SetActorAlpha copies it again; a fade's copy is
//! dropped and the fade's next step copies the swapped material.

use bevy::prelude::*;
use nvf_assets::actor::ShapeName;
use nvf_assets::model::ModelCtx;
use nvf_assets::NifMaterial;
use nvf_play::fade::Fading;
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use nvf_play::RefVisual;
use std::collections::HashMap;

use crate::world::WorldData;

pub struct TextureSwapPlugin;

impl Plugin for TextureSwapPlugin {
    fn build(&self, app: &mut App) {
        // Before the fades step, which then copies a swapped material the same frame.
        app.init_resource::<Swapped>().add_systems(Update, apply.before(nvf_play::PlaySet::Sync));
    }
}

/// The materials made for swaps: (original material, texture) → copy, and each copy's
/// original.
#[derive(Resource, Default)]
pub(crate) struct Swapped {
    made: HashMap<(AssetId<NifMaterial>, String), Handle<NifMaterial>>,
    original: HashMap<AssetId<NifMaterial>, AssetId<NifMaterial>>,
    generation: u64,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn apply(
    mut commands: Commands,
    script: Option<Res<ScriptState>>,
    sim: Option<Res<SimState>>,
    world: Option<Res<WorldData>>,
    mut models: ModelCtx,
    mut swapped: ResMut<Swapped>,
    mut drawn: Query<(Entity, &RefVisual, &MeshMaterial3d<NifMaterial>, Option<&ShapeName>, Option<&mut crate::effect_shaders::Faded>, Option<&Fading>)>,
    added: Query<(), Added<RefVisual>>,
) {
    let (Some(script), Some(sim), Some(world)) = (script, sim, world) else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    let swaps = &rt.game.texture_swaps;
    if swaps.by_ref.is_empty() || (swaps.generation == swapped.generation && added.is_empty()) {
        return;
    }
    swapped.generation = swaps.generation;
    for (&r, list) in &swaps.by_ref {
        let model = sim
            .base_of(r.0)
            .and_then(|b| fnv_world::placed::base_object(world.lo, world.schema, b.0))
            .and_then(|b| b.model)
            .map(|m| fnv_actor::resolve::mesh_path(&m))
            .and_then(|path| models.model(&path));
        for (shape, texture) in list {
            let targets: Vec<AssetId<NifMaterial>> = model
                .iter()
                .flat_map(|m| m.shapes.iter())
                .filter(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(shape)))
                .map(|s| s.material.id())
                .collect();
            for (e, rv, mat, name, mut faded, fading) in &mut drawn {
                if rv.0 != r.0 {
                    continue;
                }
                // A mesh drawn see-through wears a copy: the swap goes to its own material
                // (module notes).
                let own = match (&faded, fading) {
                    (_, Some(f)) => f.original().id(),
                    (Some(f), None) => f.original.id(),
                    (None, None) => mat.id(),
                };
                let original = swapped.original.get(&own).copied().unwrap_or(own);
                if !targets.contains(&original) && name.is_none_or(|n| n.0 != *shape) {
                    continue;
                }
                let key = (original, texture.clone());
                let copy = match swapped.made.get(&key) {
                    Some(h) => h.clone(),
                    None => {
                        let a = &mut models;
                        let Some(image) = a.textures.get(texture, true, &a.data, &mut a.images, a.formats.as_deref()) else {
                            warn!("SwapTexture: {texture} not found (reference {:08X}, shape {shape})", r.0);
                            continue;
                        };
                        let Some(mut m) = models.materials.get(original).cloned() else { continue };
                        m.base.base_color_texture = Some(image);
                        let h = models.materials.add(m);
                        swapped.original.insert(h.id(), original);
                        swapped.made.insert(key, h.clone());
                        info!("SwapTexture: reference {:08X} shape {shape}: {texture}", r.0);
                        h
                    }
                };
                match (faded.as_mut(), fading) {
                    (_, Some(f)) if f.original().id() != copy.id() => {
                        models.materials.remove(f.copy());
                        commands.entity(e).insert(MeshMaterial3d(copy)).remove::<Fading>();
                    }
                    (_, Some(_)) => {}
                    (Some(f), None) => {
                        if f.original.id() != copy.id() {
                            f.original = copy;
                        }
                    }
                    (None, None) if copy.id() != mat.id() => {
                        commands.entity(e).insert(MeshMaterial3d(copy));
                    }
                    (None, None) => {}
                }
            }
        }
    }
}
