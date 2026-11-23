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
//! dropped and the fade's next step copies the swapped material. With both on, the one
//! that started second may keep the other's see-through copy as its original; the swap
//! matches and copies the mesh's own material beneath both
//! (`crate::effect_shaders::persistent_material`, #2981).

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
        app.init_resource::<Swapped>().add_systems(Update, apply.before(nvf_play::PlaySet::Sync)).add_systems(Last, release_swapped);
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

impl Swapped {
    /// The swapped copies held, for the stress census.
    pub(crate) fn share(&self, materials: &Assets<NifMaterial>) -> nvf_assets::release::CacheShare {
        let mut s = nvf_assets::release::CacheShare::default();
        for h in self.made.values() {
            s.material(materials, h, 1);
        }
        s
    }
}

/// Every [`SWEEP_SECS`](nvf_assets::release::SWEEP_SECS), drop the swapped copies no mesh
/// wears (its reference unloaded or was swapped again), with their originals' entries, so
/// the textures go with their models (issue #904). A reference drawn again is swapped
/// again from the script's list.
fn release_swapped(time: Res<Time<Real>>, mut swapped: ResMut<Swapped>, mut since: Local<f32>) {
    *since += time.delta_secs();
    if *since < nvf_assets::release::SWEEP_SECS {
        return;
    }
    *since = 0.0;
    let s = &mut *swapped;
    if nvf_assets::release::retain_held(&mut s.made, 1) > 0 {
        let kept: std::collections::HashSet<AssetId<NifMaterial>> = s.made.values().map(Handle::id).collect();
        s.original.retain(|copy, _| kept.contains(copy));
    }
}

/// Whether a model's shape name is the shape a swap names: the call's name comes with its
/// leading `#`s dropped and in lower case (`fnv_game::functions::misc::texture_swap_names`),
/// the model's keeps them. The end slides swap `"##gs01:0"` on `gs01.nif`, whose shape is
/// `##GS01:0` [observed: fnvtool nif decode of meshes\interface\endgame\gs01.nif]; compared
/// with its `##` kept the swap never matched and the model's own first slide stayed up
/// (issue #2226).
fn same_shape(model_name: &str, swap: &str) -> bool {
    model_name.trim_start_matches('#').eq_ignore_ascii_case(swap)
}

pub(crate) fn apply(script: Option<Res<ScriptState>>, sim: Option<Res<SimState>>, world: Option<Res<WorldData>>, mut drawing: Swapping) {
    let (Some(script), Some(sim), Some(world)) = (script, sim, world) else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    drawing.sync(&rt.game.texture_swaps, &sim, &world);
}

/// [design, #2557] Production swap application, shared with synthetic ECS tests.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Swapping<'w, 's> {
    commands: Commands<'w, 's>,
    models: ModelCtx<'w>,
    swapped: ResMut<'w, Swapped>,
    #[allow(clippy::type_complexity)]
    drawn: Query<'w, 's, (Entity, &'static RefVisual, Option<&'static MeshMaterial3d<NifMaterial>>, Option<&'static crate::mesh_terrain_blend::BlendShape>, Option<&'static ShapeName>, Option<&'static mut crate::effect_shaders::Faded>, Option<&'static Fading>, Option<&'static nvf_assets::nif_material::MaterialOverride>)>,
    added: Query<'w, 's, (), NewSwapTargets>,
}

/// What makes the retained swaps apply again: a reference's mesh drawn anew, as on main,
/// and, for the mesh terrain blend's shapes only (test aid; no entity has `BlendShape`
/// without it), their marker or a restored ordinary material arriving [design, #2661].
type BlendShapeRef = (With<crate::mesh_terrain_blend::BlendShape>, With<RefVisual>);
type NewSwapTargets = Or<(Added<RefVisual>, (Added<MeshMaterial3d<NifMaterial>>, BlendShapeRef), (Added<crate::mesh_terrain_blend::BlendShape>, With<RefVisual>))>;

impl Swapping<'_, '_> {
    pub(crate) fn sync(&mut self, swaps: &fnv_game::functions::misc::TextureSwaps, sim: &SimState, world: &WorldData) {
        let Self { commands, models, swapped, drawn, added } = self;
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
                    .filter(|s| s.name.as_deref().is_some_and(|n| same_shape(n, shape)))
                    .map(|s| s.material.id())
                    .collect();
                for (e, rv, mat, blend, name, mut faded, fading, drawing) in &mut *drawn {
                    let Some(mat) = nvf_assets::nif_surface_blend::source_material(mat, blend) else { continue };
                    if rv.0 != r.0 {
                        continue;
                    }
                    // A mesh drawn see-through wears a copy: the swap goes to its own material
                    // (module notes), never to a copy one fade made of the other's (#2981).
                    // [design, #2557] Texture copies must not inherit the cloak either.
                    let own = crate::effect_shaders::persistent_material(mat, drawing, faded.as_deref(), fading, |id| models.materials.contains(id)).id();
                    let original = swapped.original.get(&own).copied().unwrap_or(own);
                    if !targets.contains(&original) && name.is_none_or(|n| !same_shape(&n.0, shape)) {
                        continue;
                    }
                    let key = (original, texture.clone());
                    let copy = match swapped.made.get(&key) {
                        Some(h) => h.clone(),
                        None => {
                            let a = &mut *models;
                            let Some(image) = a.textures.get(texture, true, &a.data, &mut a.images, a.formats.as_deref()) else {
                                warn!("SwapTexture: {texture} not found (reference {:08X}, shape {shape})", r.0);
                                continue;
                            };
                            let Some(mut m) = models.materials.get(original).cloned() else { continue };
                            m.base.base_color_texture = Some(image);
                            // The new diffuse's authored set, or the legacy look (#2981).
                            let a = &mut *models;
                            nvf_assets::authored::retexture(&mut m, texture, &mut a.textures, &a.data, &mut a.images, a.formats.as_deref());
                            let h = models.materials.add(m);
                            swapped.original.insert(h.id(), original);
                            swapped.made.insert(key, h.clone());
                            info!("SwapTexture: reference {:08X} shape {shape}: {texture}", r.0);
                            h
                        }
                    };
                    match (faded.as_mut(), fading) {
                        (also_faded, Some(f)) if f.original().id() != copy.id() => {
                            // With the actor's alpha fade on too, its stored original takes the
                            // swap as well, or releasing it would bring back the material worn
                            // before (#2981: an old authored set, mark or gloss).
                            if let Some(fd) = also_faded {
                                fd.retarget(&copy);
                            }
                            models.materials.remove(f.copy());
                            // The entity may go with its cell before this is applied (#2757).
                            commands.entity(e).try_insert(MeshMaterial3d(copy)).try_remove::<Fading>();
                        }
                        (also_faded, Some(_)) => {
                            if let Some(fd) = also_faded {
                                fd.retarget(&copy);
                            }
                        }
                        (Some(f), None) => {
                            f.retarget(&copy);
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
}

#[cfg(test)]
mod tests {
    use super::same_shape;
    use bevy::prelude::*;
    use fnv_sim::FormId;
    use nvf_assets::NifMaterial;

    #[derive(Resource, Default)]
    struct Swaps(fnv_game::functions::misc::TextureSwaps);

    #[derive(Resource, Default)]
    struct Alphas(std::collections::BTreeMap<FormId, f32>);

    const TEXTURE: &str = "textures\\synthetic-fade-swap.dds";
    /// An unknown reference is present, so a fade in runs for it (nvf_play fade.rs `start`).
    const REFERENCE: u32 = 0xFF00_0001;

    /// The production systems in the fixture's order (the swap before the fades' step, the
    /// actor's alpha after it; the game declares no order between the last two), a synthetic
    /// texture and one named shape wearing its own material.
    fn overlap_app() -> (App, impl Sized, Entity, Handle<NifMaterial>) {
        let (mut app, _files) = crate::outfits::tests::asset_fixture();
        let forms: &'static fnv_sim::FormStore = Box::leak(Box::default());
        let refs: &'static fnv_sim::RefTable = Box::leak(Box::default());
        app.insert_resource(nvf_play::sim::SimState { forms, refs, world: fnv_sim::World::new(forms, refs, Default::default()) })
            .insert_resource(crate::outfits::tests::empty_world_data())
            .init_resource::<Time<Virtual>>()
            .init_resource::<super::Swapped>()
            .init_resource::<Swaps>()
            .init_resource::<crate::effect_shaders::Shaders>()
            .init_resource::<Alphas>()
            .add_plugins(nvf_play::fade::FadePlugin)
            .add_systems(Update, (
                (|swaps: Res<Swaps>, sim: Res<nvf_play::sim::SimState>, world: Res<crate::world::WorldData>, mut drawing: super::Swapping| drawing.sync(&swaps.0, &sim, &world)).before(nvf_play::PlaySet::Sync),
                (|alpha: Res<Alphas>, mut drawing: crate::effect_shaders::AlphaDrawing| { drawing.sync(&alpha.0); }).after(nvf_play::PlaySet::Sync),
            ));
        // [documented: engine/nvf-assets/src/texture.rs tests::dds] A synthetic
        // one-block DDS offered to the normal texture loader, no game files.
        let texture = TEXTURE;
        let mut dds = vec![0u8; 136];
        dds[..4].copy_from_slice(b"DDS ");
        for (at, value) in [(4, 124u32), (8, 0x1007), (12, 4), (16, 4), (28, 1), (76, 32), (80, 4), (108, 0x1000)] {
            dds[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        dds[84..88].copy_from_slice(b"DXT1");
        app.world_mut().resource_mut::<nvf_assets::TextureCache>().offer(texture.into(), dds, 0.0);
        // An unknown reference is present, so a fade in runs for it (nvf_play fade.rs `start`).
        let reference = REFERENCE;
        let own = app.world_mut().resource_mut::<Assets<NifMaterial>>().add(nvf_assets::nif_material::nif_material(StandardMaterial::default()));
        let e = app.world_mut().spawn((MeshMaterial3d(own.clone()), nvf_play::RefVisual(reference), Visibility::Inherited, nvf_assets::actor::ShapeName("testshape".into()))).id();
        app.update();
        (app, _files, e, own)
    }

    /// #2981, review of stage 3: a mesh under SetActorAlpha and a fade at once, the swap made
    /// while both run, through the production systems (`Swapping::sync`, nvf_play's fade
    /// `step` and `AlphaDrawing::sync`), the fade stepped before the actor's alpha in this
    /// fixture (`overlap_app`). In both start orders the swap copies the model's
    /// material, the handle an unnamed shape is matched by (`targets` in `Swapping::sync`),
    /// never a see-through copy, and the swapped material has no fade's alpha baked in.
    ///
    /// How the two see-through copies hand the mesh back is not this change's subject and
    /// does not hold in every order, with or without a swap: `HELD` lists the orders that end
    /// on the mesh's own (or swapped) material, opaque, with both markers gone, as the test
    /// finds them, so a change to either system shows here.
    #[test]
    fn a_swap_under_both_fades_copies_the_mesh_own_material_in_either_start_order() {
        // Not held, both without a swap: the alpha started first and released first (the fade's
        // end puts the alpha's copy back), and the fade started first and ended first (the
        // alpha's release puts back the fade copy the fade removed).
        // Both are known bugs, older than #2981 and not this change's subject: the intended
        // result is the mesh given back whole in all eight orders. When either starts to
        // hold, add it here.
        const HELD: &[&str] = &[
            "swap false, alpha first true, alpha released first false",
            "swap false, alpha first false, alpha released first true",
            "swap true, alpha first true, alpha released first true",
            "swap true, alpha first true, alpha released first false",
            "swap true, alpha first false, alpha released first true",
            "swap true, alpha first false, alpha released first false",
        ];
        let mut held = Vec::new();
        for swap in [false, true] {
            for alpha_first in [true, false] {
                for alpha_released_first in [true, false] {
                    let case = format!("swap {swap}, alpha first {alpha_first}, alpha released first {alpha_released_first}");
                    let (mut app, _files, e, own) = overlap_app();
                    let (reference, texture) = (REFERENCE, TEXTURE);
                    let (own_mode, own_depth) = {
                        let m = app.world().resource::<Assets<NifMaterial>>().get(&own).unwrap().clone();
                        (m.base.alpha_mode, m.extension.depth_write)
                    };
                    let start_alpha = |app: &mut App| {
                        app.world_mut().resource_mut::<Alphas>().0.insert(FormId(reference), 0.5);
                        app.update();
                    };
                    let start_fade = |app: &mut App| {
                        app.world_mut().write_message(nvf_play::fade::RefFade { form_id: reference, fade_in: true });
                        app.update();
                    };
                    if alpha_first {
                        start_alpha(&mut app);
                        start_fade(&mut app);
                    } else {
                        start_fade(&mut app);
                        start_alpha(&mut app);
                    }
                    assert!(app.world().get::<crate::effect_shaders::Faded>(e).is_some(), "{case}: alpha on");
                    let fading = app.world().get::<nvf_play::fade::Fading>(e).unwrap_or_else(|| panic!("{case}: fade on")).original().clone();
                    if alpha_first {
                        assert_ne!(fading, own, "{case}: the fade copied the alpha's see-through copy (the case the swap must look beneath)");
                    } else {
                        assert_eq!(fading, own, "{case}");
                    }
                    let expected = if swap {
                        {
                            let mut swaps = app.world_mut().resource_mut::<Swaps>();
                            swaps.0.by_ref.insert(FormId(reference), vec![("testshape".into(), texture.into())]);
                            swaps.0.generation += 1;
                        }
                        app.update();
                        let swapped = app.world().resource::<super::Swapped>();
                        assert_eq!(swapped.made.len(), 1, "{case}");
                        let copy = swapped.made.values().next().unwrap().clone();
                        assert_eq!(swapped.original.get(&copy.id()), Some(&own.id()), "{case}: the swap copied the model's material");
                        let materials = app.world().resource::<Assets<NifMaterial>>();
                        let m = materials.get(&copy).unwrap();
                        assert!(m.base.base_color_texture.is_some(), "{case}: the new diffuse");
                        assert!(materials.get(&own).unwrap().base.base_color_texture.is_none(), "{case}");
                        assert_eq!((m.base.base_color.alpha(), m.base.alpha_mode, m.extension.depth_write), (1.0, own_mode, own_depth), "{case}: no fade's alpha baked into the swapped material");
                        copy
                    } else {
                        own.clone()
                    };
                    let release_alpha = |app: &mut App| {
                        app.world_mut().resource_mut::<Alphas>().0.clear();
                        app.update();
                    };
                    let end_fade = |app: &mut App| {
                        app.world_mut().resource_mut::<Time<Virtual>>().advance_by(std::time::Duration::from_secs_f32(3.0));
                        app.update();
                    };
                    if alpha_released_first {
                        release_alpha(&mut app);
                        end_fade(&mut app);
                    } else {
                        end_fade(&mut app);
                        release_alpha(&mut app);
                    }
                    app.update();
                    app.update();
                    let worn = app.world().get::<MeshMaterial3d<NifMaterial>>(e).unwrap().0.clone();
                    let opaque = app.world().resource::<Assets<NifMaterial>>().get(&worn).is_some_and(|m| (m.base.base_color.alpha(), m.base.alpha_mode, m.extension.depth_write) == (1.0, own_mode, own_depth));
                    let unmarked = app.world().get::<crate::effect_shaders::Faded>(e).is_none() && app.world().get::<nvf_play::fade::Fading>(e).is_none();
                    if worn == expected && opaque && unmarked {
                        held.push(case);
                    }
                }
            }
        }
        assert_eq!(held, HELD, "the orders that give the mesh back whole");
    }

    /// #2981, second review: the actor's alpha spans two fades. The first fade ends and
    /// removes its copy, which the alpha still names as its original; a swap during the
    /// second fade must copy the mesh's own material, not that removed copy.
    #[test]
    fn a_swap_during_a_second_fade_under_one_actor_alpha_copies_the_mesh_own_material() {
        let (mut app, _files, e, own) = overlap_app();
        let fade = |app: &mut App| {
            app.world_mut().write_message(nvf_play::fade::RefFade { form_id: REFERENCE, fade_in: true });
            app.update();
        };
        fade(&mut app);
        app.world_mut().resource_mut::<Alphas>().0.insert(FormId(REFERENCE), 0.5);
        app.update();
        app.world_mut().resource_mut::<Time<Virtual>>().advance_by(std::time::Duration::from_secs_f32(3.0));
        app.update();
        app.update();
        assert!(app.world().get::<nvf_play::fade::Fading>(e).is_none(), "the first fade is over");
        let stale = app.world().get::<crate::effect_shaders::Faded>(e).expect("alpha still on").original.clone();
        assert!(app.world().resource::<Assets<NifMaterial>>().get(&stale).is_none(), "the alpha's stored original is the first fade's removed copy");
        fade(&mut app);
        assert!(app.world().get::<nvf_play::fade::Fading>(e).is_some(), "the second fade runs");
        {
            let mut swaps = app.world_mut().resource_mut::<Swaps>();
            swaps.0.by_ref.insert(FormId(REFERENCE), vec![("testshape".into(), TEXTURE.into())]);
            swaps.0.generation += 1;
        }
        app.update();
        let swapped = app.world().resource::<super::Swapped>();
        assert_eq!(swapped.made.len(), 1, "the swap was made");
        let copy = swapped.made.values().next().unwrap().clone();
        assert_eq!(swapped.original.get(&copy.id()), Some(&own.id()), "the swap copied the model's material");
        let m = app.world().resource::<Assets<NifMaterial>>().get(&copy).unwrap();
        assert!(m.base.base_color_texture.is_some(), "the new diffuse");
        assert_eq!(m.base.base_color.alpha(), 1.0, "no fade's alpha baked in");
    }

    #[test]
    fn a_swap_finds_shapes_named_with_or_without_hashes() {
        // The end slides' shape keeps its `##` in the model; the call's name has none.
        assert!(same_shape("##GS01:0", "gs01:0"));
        assert!(same_shape("PipBoyCard01:0", "pipboycard01:0"));
        assert!(!same_shape("##GS01:1", "gs01:0"));
        assert!(!same_shape("GS01", "gs01:0"));
    }
}
