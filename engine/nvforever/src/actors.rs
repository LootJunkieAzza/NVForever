//! Placed NPCs drawn as one assembled entity each (workstream 5,
//! docs/design/static-actors.md): `fnv-actor` decides the parts, `nvf_assets::actor`
//! assembles them on one skeleton, frozen in the bind pose; FaceGen shapes the faces
//! (`facegen.rs`).

use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::{ActorForms, Appearance, Resolver};
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore};
use nvf_assets::actor::{spawn_actor, ActorCtx, PartUse};
use nvf_play::RefVisual;
use std::sync::Mutex;

use crate::facegen::FaceGenCache;

/// The records actors are resolved from (built once at start).
#[derive(Resource)]
pub struct ActorWorld {
    pub actors: ActorForms,
    pub forms: FormStore,
    /// FaceGen on/off (`--no-facegen`), and its file cache.
    pub facegen_enabled: bool,
    pub facegen: Mutex<FaceGenCache>,
}

impl ActorWorld {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> ActorWorld {
        ActorWorld {
            actors: ActorForms::build(lo, schema),
            forms: FormStore::build(lo, schema),
            facegen_enabled: true,
            facegen: Mutex::new(FaceGenCache::default()),
        }
    }

    pub fn resolve(&self, base: u32, reference: u32) -> Result<Appearance, String> {
        Resolver { actors: &self.actors, forms: &self.forms }.resolve(FormId(base), FormId(reference)).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ActorStats {
    pub drawn: usize,
    pub unresolved: usize,
    pub parts_missing: usize,
    /// Skinned shapes whose bones the skeleton lacks (drawn unskinned).
    pub unbound_shapes: usize,
}

/// Spawn the actor placed by `reference` under `parent` with the reference transform
/// `rt` (parent space). Returns whether anything was drawn.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    commands: &mut Commands,
    parent: Entity,
    rt: Affine3A,
    reference: u32,
    base: u32,
    world: &ActorWorld,
    ctx: &mut ActorCtx,
    stats: &mut ActorStats,
) -> bool {
    let a = match world.resolve(base, reference) {
        Ok(a) => a,
        Err(e) => {
            stats.unresolved += 1;
            debug!("actor {reference:08X}: {e}");
            return false;
        }
    };
    let Some(skel) = ctx.skeleton(&a.skeleton) else {
        stats.unresolved += 1;
        return false;
    };
    let mut uses = Vec::with_capacity(a.parts.len());
    for p in &a.parts {
        let Some(part) = ctx.part(&p.model) else {
            stats.parts_missing += 1;
            continue;
        };
        let mut materials: Vec<_> = part.shapes.iter().map(|s| s.material.clone()).collect();
        if let Some(t) = &p.texture {
            for (i, s) in part.shapes.iter().enumerate() {
                if !s.cap {
                    materials[i] = ctx.material_with_texture(&p.model, &part, i, t);
                }
            }
        }
        let mut meshes = vec![None; part.shapes.len()];
        if let Some(fg) = a.facegen.as_ref().filter(|_| world.facegen_enabled) {
            let mut cache = world.facegen.lock().expect("facegen cache");
            meshes = cache.morph(ctx, &part, &p.model, fg);
            if p.role == "head:head" {
                let base = p.texture.clone().or_else(|| part.shapes.first().and_then(|s| s.base_texture()).map(str::to_string));
                if let Some(img) = base.and_then(|t| cache.face_texture(ctx, &p.model, &t, fg)) {
                    for (i, s) in part.shapes.iter().enumerate() {
                        if s.cap {
                            continue;
                        }
                        if let Some(mut m) = ctx.models.materials.get(&materials[i]).cloned() {
                            m.base.base_color_texture = Some(img.clone());
                            materials[i] = ctx.models.materials.add(m);
                        }
                    }
                }
            }
        }
        // Hair colour: the texture times HCLR, doubled, in gamma space [hypothesis,
        // actors.md §3: the hair textures are close to grey; a plain multiply leaves
        // Sunny Smiles' hair dark brown where the owner's screenshot shows auburn].
        if p.role == "hair" {
            if let Some([r, g, b]) = a.hair_color {
                let tint = Color::srgb(2.0 * f32::from(r) / 255.0, 2.0 * f32::from(g) / 255.0, 2.0 * f32::from(b) / 255.0).to_linear();
                for (i, s) in part.shapes.iter().enumerate() {
                    if s.cap {
                        continue;
                    }
                    if let Some(mut m) = ctx.models.materials.get(&materials[i]).cloned() {
                        let base = m.base.base_color.to_linear();
                        m.base.base_color =
                            Color::LinearRgba(LinearRgba::new(base.red * tint.red, base.green * tint.green, base.blue * tint.blue, base.alpha));
                        materials[i] = ctx.models.materials.add(m);
                    }
                }
            }
        }
        // The hair model holds a `NoHat` and a `Hat` shape (actors.md §2.4).
        let hidden_shapes = if p.role == "hair" { vec![if a.hat { "NoHat" } else { "Hat" }.to_string()] } else { Vec::new() };
        uses.push(PartUse { meshes, part, materials, hidden_shapes });
    }
    // The race's height scales the actor [hypothesis, actors.md §3].
    let rt = rt * Affine3A::from_scale(Vec3::splat(a.race_height as f32));
    let spawned = spawn_actor(commands, parent, Transform::from_matrix(Mat4::from(rt)), &skel, &uses);
    commands.entity(spawned.root).insert((Name::new(format!("Actor {reference:08X} {}", a.edid.as_deref().unwrap_or(""))), RefVisual(reference)));
    for e in &spawned.drawn {
        commands.entity(*e).insert(RefVisual(reference));
    }
    stats.unbound_shapes += spawned.unbound;
    stats.drawn += 1;
    true
}
