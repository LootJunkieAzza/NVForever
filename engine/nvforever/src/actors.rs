//! Placed NPCs drawn as one assembled entity each (workstream 5,
//! docs/design/static-actors.md): `fnv-actor` decides the parts, `nvf_assets::actor`
//! assembles them on one skeleton, playing its locomotion clips with `--actor-pose idle`
//! (the default; docs/spec-notes/actors.md §7–8, `locomotion.rs`) or frozen in the bind
//! pose; FaceGen shapes the faces (`facegen.rs`). Placed creatures (part D, actors.md
//! §10) are assembled the same way from their skeleton and NIFZ models and play their
//! skeleton's idle.
//!
//! References not enabled at the start of a game (initially disabled, or behind a
//! disabled enable parent) are spawned hidden ([`SpawnedDisabled`]), so a script's
//! Enable can show them through `RefVisual` sync (docs/design/layer5-play.md §4).

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_actor::locomotion::{Blend, Body, Move};
use fnv_actor::{ActorForms, Appearance, CreatureAppearance, Resolver};
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore};
use nvf_assets::actor::{blend_player, spawn_actor, ActorCtx, PartUse, PreparedSkeleton};
use nvf_play::RefVisual;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::facegen::FaceGenCache;
use crate::limbs::ActorRig;
use crate::locomotion::{simulate, walk_test_at, walk_test_period, ClipSet, Locomotion, WalkTest};
use crate::weapons::{Stance, WeaponHold, WeaponMode};
use fnv_actor::weapon::AnimType;

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
pub fn clip_phase(reference: u32) -> f32 {
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
    /// Skin tints and face shapes add the race's FaceGen values to the actor's own
    /// (`--no-race-facegen` turns it off; actors.md §4.2, §4.3).
    pub race_facegen: bool,
    /// Test aid (`--show-disabled-actors`): draw actors that are not enabled at the start
    /// instead of spawning them hidden.
    pub show_disabled: bool,
    pub pose: ActorPose,
    /// With [`ActorPose::Idle`]: hold every actor at this time instead of playing
    /// (screenshots): the idle's clip time, or with the walk test the script time.
    pub pose_time: Option<f32>,
    pub walk_test: Option<WalkTestArgs>,
    /// How carried weapons are shown (`--actor-weapons`), and the draw/holster test aid's
    /// period (`--actor-weapon-test`).
    pub weapon_mode: WeaponMode,
    pub weapon_test: Option<f32>,
    /// Locomotion clip sets by (skeleton, body); `None` when no idle was found.
    sets: Mutex<ClipSets>,
    /// Weapon stances by (skeleton, prefix); `None` when they could not be loaded.
    stances: Mutex<Stances>,
}

type ClipSets = HashMap<(String, Body), Option<Arc<ClipSet>>>;
type Stances = HashMap<(String, String), Option<Arc<Stance>>>;

impl ActorWorld {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> ActorWorld {
        ActorWorld {
            actors: ActorForms::build(lo, schema),
            forms: FormStore::build(lo, schema),
            facegen_enabled: true,
            facegen: Mutex::new(FaceGenCache::default()),
            race_facegen: true,
            show_disabled: false,
            pose: ActorPose::Bind,
            pose_time: None,
            walk_test: None,
            weapon_mode: WeaponMode::None,
            weapon_test: None,
            sets: Mutex::new(HashMap::new()),
            stances: Mutex::new(HashMap::new()),
        }
    }

    pub fn resolve(&self, base: u32, reference: u32) -> Result<Appearance, String> {
        self.resolve_wearing(base, reference, None)
    }

    /// [`Self::resolve`] wearing the given armour instead of the chosen one
    /// (`Resolver::resolve_wearing`; the player's body).
    pub fn resolve_wearing(&self, base: u32, reference: u32, wearing: Option<&[FormId]>) -> Result<Appearance, String> {
        Resolver { actors: &self.actors, forms: &self.forms }.resolve_wearing(FormId(base), FormId(reference), wearing).map_err(|e| e.to_string())
    }

    pub fn resolve_creature(&self, base: u32, reference: u32) -> Result<CreatureAppearance, String> {
        Resolver { actors: &self.actors, forms: &self.forms }.resolve_creature(FormId(base), FormId(reference)).map_err(|e| e.to_string())
    }

    /// The locomotion clip set of a skeleton and body (loaded once), if it has an idle.
    fn clip_set(&self, ctx: &mut ActorCtx, skeleton: &str, skel: &PreparedSkeleton, body: Body) -> Option<Arc<ClipSet>> {
        let key = (fnv_vfs::normalize(skeleton), body);
        self.sets
            .lock()
            .expect("clip sets")
            .entry(key)
            .or_insert_with(|| {
                let (set, line) = ClipSet::load(ctx, skeleton, skel, body);
                info!("actor locomotion: {line}");
                set.clips.contains_key(&Move::Idle).then(|| Arc::new(set))
            })
            .clone()
    }
}

/// Marks the root of an actor, or the light source of a light, spawned hidden because its
/// reference is not enabled at the start of a game (world.rs `WorldData::enabled`).
/// `RefVisual` sync shows it when a script enables it; if the simulation already has it
/// enabled when its cell loads (enabled while the cell was away), it is shown at once
/// (component hook, so no system needs registering).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(on_add = show_if_present)]
pub struct SpawnedDisabled(pub u32);

fn show_if_present(mut world: DeferredWorld, ctx: HookContext) {
    let Some(r) = world.get::<SpawnedDisabled>(ctx.entity).map(|s| s.0) else { return };
    let present = world.get_resource::<nvf_play::sim::SimState>().is_some_and(|s| s.present(r));
    if present {
        if let Some(mut v) = world.get_mut::<Visibility>(ctx.entity) {
            *v = Visibility::Inherited;
        }
    }
}

/// Hide `root` (spawned for `reference`) until a script enables it ([`SpawnedDisabled`]).
pub fn spawn_hidden(commands: &mut Commands, root: Entity, reference: u32) {
    commands.entity(root).insert((Visibility::Hidden, SpawnedDisabled(reference)));
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
    /// NPC bodies whose skin texture was tinted with the race's body `.egt`.
    pub skins_tinted: usize,
    /// NPCs whose hand skin texture was tinted with the hand `.egt`.
    pub hands_tinted: usize,
    /// Placed creatures drawn, unresolved, playing their idle, without an idle.
    pub creatures: usize,
    pub creatures_unresolved: usize,
    pub creatures_animated: usize,
    pub creature_clip_missing: usize,
    /// Actors and creatures spawned hidden (not enabled at the start).
    pub spawned_hidden: usize,
    /// Actors holding a weapon, and those whose weapon model or stance clips are missing.
    pub armed: usize,
    pub weapon_missing: usize,
}

/// A race skin texture and the `.egt` that tints it; `image` is made on first use.
struct SkinTint {
    hand: bool,
    texture: String,
    egt: String,
    image: Option<Option<Handle<Image>>>,
}

/// The parts of a resolved actor ready to assemble, each with its index in `a.parts`:
/// models loaded, textures, FaceGen, body skin and hair colour applied, the hair's hat
/// variant chosen. Parts whose model cannot be loaded are left out (counted in `stats`).
/// Placed NPCs and the player's body (`player_body.rs`) build their parts here.
pub fn part_uses(a: &Appearance, world: &ActorWorld, ctx: &mut ActorCtx, stats: &mut ActorStats) -> Vec<(usize, PartUse)> {
    let mut uses = Vec::with_capacity(a.parts.len());
    // The skin textures, each tinted once per actor: the race's upper body texture with
    // the body `.egt`, its hand texture with the hand `.egt` (the left one for both
    // hands) [hypothesis, actors.md §4.1]. Face, body and hands take the actor's texture
    // coefficients plus its race's [hypothesis, actors.md §4.2].
    let fgts = a.skin_fgts(world.race_facegen).filter(|_| world.facegen_enabled);
    let fgts = fgts.as_deref();
    // The shape of the face and of what is fitted to it (hair, brows, eyes, hats): the
    // actor's geometry coefficients plus its race's [hypothesis, actors.md §4.3].
    let shape = a.shape_facegen(world.race_facegen).filter(|_| world.facegen_enabled);
    let mut skins: Vec<SkinTint> = Vec::new();
    if let Some(body_egt) = a.body_egt.as_deref().filter(|_| fgts.is_some()) {
        let hand_egt = fnv_actor::resolve::hand_egt(body_egt, true);
        for (hand, texture, egt) in [(false, &a.body_texture, Some(body_egt.to_string())), (true, &a.hand_texture, hand_egt)] {
            if let (Some(texture), Some(egt)) = (texture.as_deref().map(fnv_vfs::texture_path), egt) {
                skins.push(SkinTint { hand, texture, egt, image: None });
            }
        }
    }
    let pipboy = a.parts.iter().any(|p| world.actors.armors.get(&p.form).is_some_and(|ar| ar.slots & fnv_actor::resolve::slot::PIPBOY != 0));
    for (pi, p) in a.parts.iter().enumerate() {
        let Some(part) = ctx.part(&p.model) else {
            stats.parts_missing += 1;
            continue;
        };
        let mut materials: Vec<_> = part.shapes.iter().map(|s| s.material.clone()).collect();
        // The part's texture goes on its skin shapes when it has any: the race bodies
        // also hold the underwear, which keeps its own texture (actors.md §3.1).
        let takes_texture = part.texture_targets();
        if let Some(t) = &p.texture {
            for (i, _) in takes_texture.iter().enumerate().filter(|(_, &takes)| takes) {
                materials[i] = ctx.material_with_texture(&p.model, &part, i, t);
            }
        }
        // A worn model's skin shapes (skin showing inside an outfit or a glove) take the
        // race's skin texture in place of the one they name, so a ghoul's or a raider's
        // arms are not human skin [hypothesis, actors.md §3.1].
        let mut worn_skin: Vec<Option<&str>> = vec![None; part.shapes.len()];
        if p.role == "armor" {
            for (i, s) in part.shapes.iter().enumerate().filter(|(_, s)| !s.cap && s.is_skin()) {
                let Some(t) = a.worn_skin_texture(s.base_texture()) else { continue };
                if s.base_texture().map(fnv_vfs::texture_path) != Some(fnv_vfs::texture_path(t)) {
                    materials[i] = ctx.material_with_texture(&p.model, &part, i, t);
                }
                worn_skin[i] = Some(t);
            }
        }
        let mut meshes = vec![None; part.shapes.len()];
        if let Some(shape) = &shape {
            let mut cache = world.facegen.lock().expect("facegen cache");
            meshes = cache.morph(ctx, &part, &p.model, shape);
            if p.role == "head:head" {
                let base = p.texture.clone().or_else(|| part.shapes.first().and_then(|s| s.base_texture()).map(str::to_string));
                if let Some(img) = base.zip(fgts).and_then(|(t, fgts)| cache.face_texture(ctx, &p.model, &t, fgts)) {
                    for (i, _) in takes_texture.iter().enumerate().filter(|(_, &takes)| takes) {
                        if let Some(mut m) = ctx.models.materials.get(&materials[i]).cloned() {
                            m.base.base_color_texture = Some(img.clone());
                            materials[i] = ctx.models.materials.add(m);
                        }
                    }
                }
            }
        }
        // Skin: every shape whose diffuse is one of the race's skin textures (the race
        // body and hands, and skin inside outfits and gloves) gets the tinted copy
        // [hypothesis, actors.md §4.1].
        if let Some(fg) = fgts {
            for (i, s) in part.shapes.iter().enumerate() {
                let given = worn_skin[i].or(p.texture.as_deref().filter(|_| takes_texture[i]));
                let tex = if s.cap { None } else { given.or_else(|| s.base_texture()) };
                let Some(tex) = tex.map(fnv_vfs::texture_path) else { continue };
                let Some(skin) = skins.iter_mut().find(|k| k.texture == tex) else { continue };
                let img = skin
                    .image
                    .get_or_insert_with(|| {
                        let mut cache = world.facegen.lock().expect("facegen cache");
                        let img = cache.skin_texture(ctx, &skin.egt, &skin.texture, fg);
                        *(if skin.hand { &mut stats.hands_tinted } else { &mut stats.skins_tinted }) += usize::from(img.is_some());
                        img
                    })
                    .clone();
                if let Some(img) = img {
                    if let Some(mut m) = ctx.models.materials.get(&materials[i]).cloned() {
                        m.base.base_color_texture = Some(img);
                        materials[i] = ctx.models.materials.add(m);
                    }
                }
            }
        }
        // Hair colour: the texture times HCLR, doubled, in gamma space [hypothesis,
        // actors.md §3: the hair textures are close to grey; a plain multiply leaves
        // Sunny Smiles' hair dark brown where the owner's screenshot shows auburn]. The
        // hair model's shapes, and the head parts' hair-shader shapes (beards), which
        // were drawn untinted, near white (playtest list 5 item 26,
        // actors.md §3).
        if p.role == "hair" || p.role == "head_part" {
            if let Some([r, g, b]) = a.hair_color {
                let tint = Color::srgb(2.0 * f32::from(r) / 255.0, 2.0 * f32::from(g) / 255.0, 2.0 * f32::from(b) / 255.0).to_linear();
                for (i, s) in part.shapes.iter().enumerate() {
                    if s.cap || (p.role == "head_part" && !s.is_hair()) {
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
        let mut hidden_shapes = if p.role == "hair" { vec![if a.hat { "NoHat" } else { "Hat" }.to_string()] } else { Vec::new() };
        // The Pip-Boy light's glow shows only with the light on, which is not built, and
        // outfits hold a left sleeve for wearing a Pip-Boy and one for not (actors.md §1.2).
        hidden_shapes.extend(part.shapes.iter().filter_map(|s| s.name.clone().filter(|n| fnv_actor::mesh::hidden_for_pipboy(n, pipboy))));
        uses.push((pi, PartUse { meshes, part, materials, hidden_shapes, by_ancestor: false }));
    }
    uses
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
    hidden: bool,
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
    let (part_index, uses): (Vec<usize>, Vec<PartUse>) = part_uses(&a, world, ctx, stats).into_iter().unzip();
    // The race's height scales the actor [hypothesis, actors.md §3].
    let rt = rt * Affine3A::from_scale(Vec3::splat(a.race_height as f32));
    let spawned = spawn_actor(commands, parent, Transform::from_matrix(Mat4::from(rt)), &skel, &uses);
    // The shapes a `.tri` can move when the actor speaks (lips.rs).
    if let Some(rig) = crate::lips::rig(&a, &part_index, &uses, &spawned, &ctx.models.data, a.shape_facegen(world.race_facegen).filter(|_| world.facegen_enabled)) {
        commands.entity(spawned.root).insert(rig);
    }
    commands.entity(spawned.root).insert((
        Name::new(format!("Actor {reference:08X} {}", a.edid.as_deref().unwrap_or(""))),
        RefVisual(reference),
        ActorRig { reference, base, skeleton: a.skeleton.clone(), skel: skel.clone(), nodes: spawned.nodes.clone() },
        // What it wears and holds follows its inventory (`outfits.rs`).
        crate::outfits::Dressed::new(world, &a, &part_index, &spawned),
    ));
    for e in &spawned.drawn {
        commands.entity(*e).insert(RefVisual(reference));
    }
    if hidden && !world.show_disabled {
        spawn_hidden(commands, spawned.root, reference);
        stats.spawned_hidden += 1;
    }
    if world.pose == ActorPose::Idle {
        let body = if a.female { Body::Female } else { Body::Male };
        match world.clip_set(ctx, &a.skeleton, &skel, body) {
            Some(set) => {
                let mut lo = Locomotion::new(set.clone(), clip_phase(reference));
                let walk = world.walk_test.filter(|w| w.radius <= 0.0 || at.distance(w.center) <= w.radius);
                let mut player = blend_player(&spawned, &skel);
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
                    commands.entity(spawned.root).insert((tr, wt));
                    stats.walking += 1;
                } else if let Some(t) = world.pose_time {
                    let len = set.info(Move::Idle).map_or(1.0, |c| c.duration).max(f32::EPSILON);
                    lo.blend = Blend::new(Move::Idle, t / len);
                    lo.hold = true;
                }
                lo.layers(&mut player);
                let weapon = arm(commands, &spawned, &skel, &a, reference, world, ctx, &mut player, stats);
                let mut ec = commands.entity(spawned.root);
                ec.insert((lo, player));
                if let Some(w) = weapon {
                    ec.insert(w);
                }
                stats.animated += 1;
            }
            None => stats.clip_missing += 1,
        }
    }
    stats.unbound_shapes += spawned.unbound;
    stats.drawn += 1;
    true
}

/// Spawn the creature placed by `reference` (ACRE) under `parent` (actors.md §10): its
/// skeleton with the NIFZ models, scaled by the reference's and the creature's base
/// scale, playing the skeleton's idle with `--actor-pose idle`. `rt` is the reference
/// transform (parent space). Returns whether anything was drawn.
#[allow(clippy::too_many_arguments)]
pub fn spawn_creature(
    commands: &mut Commands,
    parent: Entity,
    rt: Affine3A,
    reference: u32,
    base: u32,
    world: &ActorWorld,
    ctx: &mut ActorCtx,
    stats: &mut ActorStats,
    hidden: bool,
) -> bool {
    let c = match world.resolve_creature(base, reference) {
        Ok(c) => c,
        Err(e) => {
            stats.creatures_unresolved += 1;
            debug!("creature {reference:08X}: {e}");
            return false;
        }
    };
    let Some(skel) = ctx.skeleton(&c.skeleton) else {
        stats.creatures_unresolved += 1;
        return false;
    };
    let mut uses = Vec::with_capacity(c.parts.len());
    for p in &c.parts {
        let Some(part) = ctx.part(&p.model) else {
            stats.parts_missing += 1;
            continue;
        };
        let materials = part.shapes.iter().map(|s| s.material.clone()).collect();
        let meshes = vec![None; part.shapes.len()];
        uses.push(PartUse { meshes, part, materials, hidden_shapes: Vec::new(), by_ancestor: true });
    }
    // The creature's base scale (BNAM) on top of the reference's [hypothesis, §10.1].
    let rt = rt * Affine3A::from_scale(Vec3::splat(c.base_scale as f32));
    let spawned = spawn_actor(commands, parent, Transform::from_matrix(Mat4::from(rt)), &skel, &uses);
    commands.entity(spawned.root).insert((
        Name::new(format!("Creature {reference:08X} {}", c.edid.as_deref().unwrap_or(""))),
        RefVisual(reference),
        ActorRig { reference, base, skeleton: c.skeleton.clone(), skel: skel.clone(), nodes: spawned.nodes.clone() },
    ));
    for e in &spawned.drawn {
        commands.entity(*e).insert(RefVisual(reference));
    }
    if hidden && !world.show_disabled {
        spawn_hidden(commands, spawned.root, reference);
        stats.spawned_hidden += 1;
    }
    if world.pose == ActorPose::Idle {
        match world.clip_set(ctx, &c.skeleton, &skel, Body::Male) {
            Some(set) => {
                let mut lo = Locomotion::new(set.clone(), clip_phase(reference));
                if let Some(t) = world.pose_time {
                    let len = set.info(Move::Idle).map_or(1.0, |c| c.duration).max(f32::EPSILON);
                    lo.blend = Blend::new(Move::Idle, t / len);
                    lo.hold = true;
                }
                let mut player = blend_player(&spawned, &skel);
                lo.layers(&mut player);
                commands.entity(spawned.root).insert((lo, player));
                stats.creatures_animated += 1;
            }
            None => stats.creature_clip_missing += 1,
        }
    }
    stats.unbound_shapes += spawned.unbound;
    stats.creatures += 1;
    true
}

/// Give an actor its weapon (part H, actors.md §9): the model under the skeleton's
/// `Weapon` node and a [`WeaponHold`] with the stance's clips, or nothing when the actor
/// carries no drawable weapon or weapons are off.
#[allow(clippy::too_many_arguments)]
fn arm(
    commands: &mut Commands,
    spawned: &nvf_assets::actor::SpawnedActor,
    skel: &nvf_assets::actor::PreparedSkeleton,
    a: &Appearance,
    reference: u32,
    world: &ActorWorld,
    ctx: &mut ActorCtx,
    player: &mut nvf_assets::anim_play::BlendPlayer,
    stats: &mut ActorStats,
) -> Option<WeaponHold> {
    if world.weapon_mode == WeaponMode::None {
        return None;
    }
    let w = a.weapon.and_then(|id| world.actors.weapons.get(&id))?;
    let prefix = w.anim_type.as_deref().and_then(AnimType::from_name)?.prefix();
    let model = w.model.as_deref()?;
    let key = (fnv_vfs::normalize(&a.skeleton), prefix.to_string());
    let stance = world
        .stances
        .lock()
        .expect("stances")
        .entry(key)
        .or_insert_with(|| match Stance::load(ctx, &a.skeleton, skel, prefix) {
            Ok((s, line)) => {
                info!("actor weapons: {line}");
                Some(Arc::new(s))
            }
            Err(e) => {
                warn!("actor weapons: {prefix}: {e}");
                None
            }
        })
        .clone();
    let (Some(stance), Some(part)) = (stance, ctx.part(&fnv_actor::resolve::mesh_path(model))) else {
        stats.weapon_missing += 1;
        return None;
    };
    for e in crate::weapons::spawn_model(commands, spawned, stance.weapon_node, &part) {
        commands.entity(e).insert(RefVisual(reference));
        if let Some(weapon) = a.weapon {
            commands.entity(e).insert(crate::weapons::HeldWeapon { reference, weapon });
        }
    }
    // Where its shots leave (`npc_combat`): the model's `ProjectileNode`.
    if let Some(pm) = ctx.models.model(&fnv_actor::resolve::mesh_path(model)) {
        crate::weapons::spawn_muzzle(commands, spawned, stance.weapon_node, &pm, reference);
    }
    let skel_parent = skel.parents[stance.weapon_node].unwrap_or(0);
    let mut hold = WeaponHold::new(stance, world.weapon_mode == WeaponMode::Drawn, skel_parent);
    if let Some(period) = world.weapon_test {
        hold.test = Some((period, clip_phase(reference) * 2.0 * period));
    }
    if let Some(t) = world.pose_time {
        // A repeatable pose: the test aid runs from its start to `t`; otherwise the stage's
        // clip is held at `t`.
        match hold.test.as_mut() {
            Some(test) => {
                test.1 = 0.0;
                hold.simulate(t);
            }
            None => hold.anim.time = t,
        }
        hold.hold = true;
    }
    hold.layers(player);
    stats.armed += 1;
    Some(hold)
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
