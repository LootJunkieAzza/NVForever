//! Placed NPCs drawn as one assembled entity each (workstream 5,
//! docs/design/static-actors.md): `fnv-actor` decides the parts, `nvf_assets::actor`
//! assembles them on one skeleton, playing its locomotion clips with `--actor-pose idle`
//! (the default; docs/spec-notes/actors.md §7–8, `locomotion.rs`) or frozen in the bind
//! pose; FaceGen shapes the faces (`facegen.rs`).

use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::locomotion::{Blend, Body, Move};
use fnv_actor::{ActorForms, Appearance, Resolver};
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore};
use nvf_assets::actor::{blend_player, spawn_actor, ActorCtx, PartUse};
use nvf_play::RefVisual;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::facegen::FaceGenCache;
use crate::locomotion::{simulate, walk_test_at, walk_test_period, ClipSet, Locomotion, WalkTest};

/// How placed actors are posed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ActorPose {
    /// The skeleton's bind pose, frozen (`--actor-pose bind`).
    Bind,
    /// The skeleton's locomotion clips (`locomotion.rs`): the idle in a loop while an
    /// actor stands; the default since 2026-10-09 (owner decision,
    /// docs/design/static-actors.md).
    #[default]
    Idle,
}

/// `--actor-walk-test`: speed (units/s) and how far from the start position (game units;
/// 0 = any distance) actors take part.
#[derive(Debug, Clone, Copy)]
pub struct WalkTestArgs {
    pub speed: f32,
    pub radius: f32,
    pub center: Vec3,
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
    /// With [`ActorPose::Idle`]: hold every actor at this time instead of playing
    /// (screenshots): the idle's clip time, or with the walk test the script time.
    pub pose_time: Option<f32>,
    pub walk_test: Option<WalkTestArgs>,
    /// Locomotion clip sets by (skeleton, body); `None` when no idle was found.
    sets: Mutex<HashMap<(String, Body), Option<Arc<ClipSet>>>>,
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
            walk_test: None,
            sets: Mutex::new(HashMap::new()),
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
    /// Actors playing clips, those whose idle could not be loaded, and those under the
    /// walk test.
    pub animated: usize,
    pub clip_missing: usize,
    pub walking: usize,
}

/// Spawn the actor placed by `reference` under `parent` with the reference transform
/// `rt` (parent space); `at` is its position in game space. Returns whether anything was
/// drawn.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    commands: &mut Commands,
    parent: Entity,
    rt: Affine3A,
    at: Vec3,
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
    if world.pose == ActorPose::Idle {
        let body = if a.female { Body::Female } else { Body::Male };
        let key = (fnv_vfs::normalize(&a.skeleton), body);
        let set = world
            .sets
            .lock()
            .expect("clip sets")
            .entry(key)
            .or_insert_with(|| {
                let (set, line) = ClipSet::load(ctx, &a.skeleton, &skel, body);
                info!("actor locomotion: {line}");
                set.clips.contains_key(&Move::Idle).then(|| Arc::new(set))
            })
            .clone();
        match set {
            Some(set) => {
                let mut lo = Locomotion::new(set.clone(), clip_phase(reference));
                let walk = world.walk_test.filter(|w| w.radius <= 0.0 || at.distance(w.center) <= w.radius);
                let mut player = blend_player(&spawned, &skel);
                let mut ec = commands.entity(spawned.root);
                if let Some(w) = walk {
                    let base = Transform::from_matrix(Mat4::from(rt));
                    let mut wt = WalkTest { base, speed: w.speed, t: clip_phase(reference) * walk_test_period(w.speed) };
                    let tr = match world.pose_time {
                        Some(t) => {
                            // A repeatable pose: the script and blend run from the start to `t`.
                            wt.t = 0.0;
                            lo.hold = true;
                            simulate(&mut wt, &mut lo, t)
                        }
                        None => wt.transform(&walk_test_at(wt.t, w.speed)),
                    };
                    ec.insert((tr, wt));
                    stats.walking += 1;
                } else if let Some(t) = world.pose_time {
                    let len = set.info(Move::Idle).map_or(1.0, |c| c.duration).max(f32::EPSILON);
                    lo.blend = Blend::new(Move::Idle, t / len);
                    lo.hold = true;
                }
                lo.layers(&mut player);
                ec.insert((lo, player));
                stats.animated += 1;
            }
            None => stats.clip_missing += 1,
        }
    }
    stats.unbound_shapes += spawned.unbound;
    stats.drawn += 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_spread_within_the_clip() {
        let p: Vec<f32> = (0..64u32).map(|r| clip_phase(0x0010_4C0C + r)).collect();
        assert!(p.iter().all(|x| (0.0..1.0).contains(x)));
        assert!(p.iter().any(|&x| x < 0.25) && p.iter().any(|&x| x > 0.75));
    }
}
