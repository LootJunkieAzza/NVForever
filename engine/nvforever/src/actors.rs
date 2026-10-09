//! Placed NPCs drawn as one assembled entity each (workstream 5,
//! docs/design/static-actors.md): `fnv-actor` decides the parts, `nvf_assets::actor`
//! assembles them on one skeleton, frozen in the bind pose by default or playing an idle
//! clip with `--actor-pose idle` (docs/spec-notes/actors.md §7); FaceGen shapes the faces
//! (`facegen.rs`).

use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::{ActorForms, Appearance, Resolver};
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore};
use nvf_assets::actor::{clip_binding, play_on_actor, spawn_actor, ActorCtx, PartUse, PreparedSkeleton, SpawnedActor};
use nvf_play::RefVisual;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::facegen::FaceGenCache;

/// How placed actors are posed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ActorPose {
    /// The skeleton's bind pose, frozen (`--actor-pose bind`).
    Bind,
    /// The skeleton's idle clip in a loop ([`idle_clip`]); the default since 2026-10-09
    /// (owner decision, docs/design/static-actors.md).
    #[default]
    Idle,
}

/// The idle clip played on a skeleton: `locomotion\mtidle.kf` in the skeleton's folder
/// [observed: `characters\_male\locomotion\mtidle.kf` beside
/// `characters\_male\skeleton.nif`; hypothesis: the folder rule for other skeletons].
pub fn idle_clip(skeleton: &str) -> String {
    let s = fnv_vfs::normalize(skeleton);
    let dir = s.rfind('\\').map_or("", |i| &s[..=i]);
    format!("{dir}locomotion\\mtidle.kf")
}

/// Where in its clip an actor starts (0–1): spread by reference FormID so neighbours do
/// not move in step [engine choice, not a game rule].
fn clip_phase(reference: u32) -> f32 {
    (reference.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32
}

/// The records actors are resolved from (built once at start).
#[derive(Resource)]
pub struct ActorWorld {
    pub actors: ActorForms,
    pub forms: FormStore,
    /// FaceGen on/off (`--no-facegen`), and its file cache.
    pub facegen_enabled: bool,
    pub facegen: Mutex<FaceGenCache>,
    pub pose: ActorPose,
    /// With [`ActorPose::Idle`]: hold every actor at this clip time (seconds from the
    /// clip's start) instead of playing (screenshots).
    pub pose_time: Option<f32>,
    /// (skeleton, clip) pairs whose track binding was reported.
    reported: Mutex<HashSet<(String, String)>>,
}

impl ActorWorld {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> ActorWorld {
        ActorWorld {
            actors: ActorForms::build(lo, schema),
            forms: FormStore::build(lo, schema),
            facegen_enabled: true,
            facegen: Mutex::new(FaceGenCache::default()),
            pose: ActorPose::Bind,
            pose_time: None,
            reported: Mutex::new(HashSet::new()),
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
    /// Actors playing a clip, and those whose clip could not be loaded.
    pub animated: usize,
    pub clip_missing: usize,
}

/// What [`spawn`] put together: the actor's entities, its skeleton and appearance (for
/// changing its clip later, `nav.rs`).
pub struct Spawned {
    pub actor: SpawnedActor,
    pub skeleton: Arc<PreparedSkeleton>,
    pub appearance: Appearance,
}

/// Spawn the actor placed by `reference` under `parent` with the reference transform
/// `rt` (parent space). `None` when nothing was drawn.
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
) -> Option<Spawned> {
    let a = match world.resolve(base, reference) {
        Ok(a) => a,
        Err(e) => {
            stats.unresolved += 1;
            debug!("actor {reference:08X}: {e}");
            return None;
        }
    };
    let Some(skel) = ctx.skeleton(&a.skeleton) else {
        stats.unresolved += 1;
        return None;
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
    if world.pose == ActorPose::Idle {
        let path = idle_clip(&a.skeleton);
        match ctx.clip(&path) {
            Some(clip) => {
                let pair = (fnv_vfs::normalize(&a.skeleton), path.clone());
                if world.reported.lock().expect("reported").insert(pair) {
                    let (bound, unbound) = clip_binding(&skel, &clip);
                    info!(
                        "actor pose: {path} ({:.2}s) on {} ({} nodes): {} transform tracks ({} others skipped), {} drive skeleton nodes, {} do not {:?}; accumulation root {:?} held at the identity: {}",
                        clip.stop - clip.start,
                        a.skeleton,
                        skel.names.len(),
                        clip.tracks.len(),
                        clip.skipped,
                        bound.len(),
                        unbound.len(),
                        unbound,
                        clip.accum_root,
                        clip.undriven_accum_root().is_some()
                    );
                }
                let (phase, hold) = match world.pose_time {
                    Some(t) => (t / (clip.stop - clip.start).max(f32::EPSILON), true),
                    None => (clip_phase(reference), false),
                };
                play_on_actor(commands, &spawned, &skel, &clip, &path, phase, hold);
                stats.animated += 1;
            }
            None => stats.clip_missing += 1,
        }
    }
    stats.unbound_shapes += spawned.unbound;
    stats.drawn += 1;
    Some(Spawned { actor: spawned, skeleton: skel, appearance: a })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_clip_beside_the_skeleton() {
        assert_eq!(idle_clip(r"Meshes\Characters\_Male\skeleton.nif"), r"meshes\characters\_male\locomotion\mtidle.kf");
        assert_eq!(idle_clip("skeleton.nif"), r"locomotion\mtidle.kf");
    }

    #[test]
    fn phases_spread_within_the_clip() {
        let p: Vec<f32> = (0..64u32).map(|r| clip_phase(0x0010_4C0C + r)).collect();
        assert!(p.iter().all(|x| (0.0..1.0).contains(x)));
        assert!(p.iter().any(|&x| x < 0.25) && p.iter().any(|&x| x > 0.75));
    }
}
