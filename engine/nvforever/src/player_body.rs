//! The player's own body in `--play` (docs/design/player-body.md), as two rigs:
//!
//! - **Third person**: the player's NPC_ (FormID 7) assembled like a placed NPC
//!   (`actors.rs`) on `characters\_male\skeleton.nif`, wearing what the simulation has
//!   equipped, standing at the character's feet and facing the view's heading. Seen in
//!   third person and while the camera circles the player.
//! - **First person**: the game's separate arms rig, `characters\_1stperson\skeleton.nif`,
//!   with the arms and hands only (`fnv_actor::first_person`: hands' `…1st.nif`
//!   variants, the `Arms` shapes of the upper body and of upper-body armour) and the
//!   weapon's first-person model (WEAP WNAM). Its `Bip01 Looking` node turns with the
//!   view's pitch, so arms and weapon follow the view, and the camera sits at its
//!   `Camera1st` node (118 units up, 78 crouched) [hypothesis: the node names; observed:
//!   the nodes, and the first-person sneak clips lower `Bip01 NonAccum` by 40;
//!   animation.md §9.5].
//!
//! Both play layered KF clips (`nvf_assets::anim_blend`) from their own folder, chosen by
//! `fnv_actor::player_clips`: an idle, movement in four directions walking or running,
//! sneaking, swimming, jumping and landing, turning on the spot, and the weapon drawn,
//! held (aimed down the sights, or blocking) and holstered, with the weapon moving
//! between the hand and the holster bone at the clips' `Attach`/`Detach` keys. Attacks
//! and reloads (combat step C4, `nvf_play::combat`): the body tells combat when the weapon
//! is out and how long the attack and reload clips run (`WeaponAnim`), and plays the clip
//! once each time combat counts an attack or a reload up.
//!
//! The weapon model keeps its node tree under the `Weapon` node, and the clips move its
//! parts by name as they move bones: the slide, hammer and trigger of a pistol in its
//! attack clip, the magazine in its reload clip, a minigun's barrel in its spin clip
//! (animation.md §9.8).
//!
//! Runs in `nvf_play::player::BodySet`: after the character moved, before the camera is
//! placed. The eye (`Player::body_eye`) is the first-person rig's camera node. Without
//! actors (`--no-actors`) there is no body and the eye stays at the fixed eye height;
//! without a first-person skeleton the third-person body's head gives the eye and its
//! head is hidden in first person.
//!
//! **The body in first person** ([`PlayerBody::first_person_body`], `--first-person-body`
//! and the settings page; off as the original, which draws the arms rig alone): the
//! third-person body is drawn in first person too, without its head, standing so that
//! its eyes are under the camera, and upright whatever the pitch. While the arms rig's
//! arms are up the body's own arms and hands are hidden, and its weapon while it is in
//! the hand. On furniture the seated body is drawn without its head, the view turns no
//! further from the way the body faces than a neck does ([`seated_heading`]) and the eyes
//! go round the neck with it.

use crate::actors::{part_uses, ActorWorld};
use crate::world::WorldRoot;
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_actor::first_person::{self, is_first_person_shape, shape_shows, Show};
use fnv_actor::player_clips::{
    aim_clips, aim_variant, aim_weights, attack_clips, attack_stem, block_clips, blocks, direction_weights, equip_clips, holster_clips, idle_clips, jump_clips,
    move_clips, reload_clips, turn_clips, Aim, ClipKeys, Dir, HandEvent, Stance, Transition, WeaponHands, WeaponPhase, UNARMED,
};
use fnv_actor::weapon::AnimType;
use fnv_actor::Appearance;
use fnv_sim::FormId;
use nvf_assets::actor::{rigid_local, spawn_actor, ActorCtx, ActorParts, PartUse, PreparedSkeleton};
use nvf_assets::anim_blend::{Animator, ExtraNode, Mode, DEFAULT_FADE};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_play::combat::WeaponAnim;
use nvf_play::control::{BodyView, PlayerControl};
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player};
use nvf_play::sim::SimState;
use nvf_play::PlayWorld;
use std::collections::HashMap;
use std::sync::Arc;

/// Layer roles ([`Animator`] tags).
const TAG_IDLE: u32 = 0;
const TAG_MOVE: u32 = 10; // + 0 along, + 1 sideways
const TAG_TURN: u32 = 20;
const TAG_JUMP: u32 = 30;
const TAG_LAND: u32 = 31;
const TAG_AIM: u32 = 40; // + 0 level, + 1 up, + 2 down
const TAG_HOLSTER: u32 = 50;
const TAG_EQUIP: u32 = 60;
const TAG_ATTACK: u32 = 70; // + 0 level, + 1 up, + 2 down
const TAG_RELOAD: u32 = 75;
const TAG_FURNITURE: u32 = 80;

/// Biped slots of headgear hidden in first person when the third-person body has to stand
/// in for the first-person rig: Head, Hair, Headband, Hat, Eye Glasses, Nose Ring,
/// Earrings, Mask, Mouth Object [documented: xEdit biped flag names, bits 0, 1, 9–14,
/// 16; engineering choice: which ones block the view].
const HEAD_SLOTS: u32 = 1 | 1 << 1 | 0x7E << 8 | 1 << 16;

/// Time in the air before the jump loop plays, and the shortest fall that plays a
/// landing, seconds [engineering choice: steps and slopes leave the ground briefly].
const AIRBORNE_LOOP: f32 = 0.15;
const AIRBORNE_LAND: f32 = 0.35;
/// Turning on the spot faster than this (radians per second) plays a turn clip
/// [engineering choice].
const TURN_RATE: f32 = 1.0;
/// Without a first-person rig: share of the view's pitch the neck takes when nothing else
/// bends the body, so the eye clears the chest looking down [engineering choice].
const NECK_PITCH: f32 = 0.6;
/// Blend-in of an attack clip, seconds [engineering choice: a shot should show at once].
const ATTACK_FADE: f32 = 0.03;
/// An attack clip with a `Loop` key goes round its repeated part while the shots come no
/// further apart than this, seconds [engineering choice: the clips that have the key are
/// the `attackspin` ones, of weapons that fire many times a second; animation.md §9.8].
const LOOP_HOLD: f32 = 0.25;
/// The body in first person: how far behind the camera its eyes stand, in the skeleton's
/// units, so the open neck stays out of the picture looking down [engineering choice].
const OWN_BODY_BACK: f32 = 4.0;
/// The body in first person on furniture: how far the view turns to either side of the
/// way the body faces, radians [owner decision 2026-10-10: "limited to a real person's
/// head movement"; engineering choice: 80 degrees, about what a neck turns].
const NECK_TURN: f32 = 80.0 * std::f32::consts::PI / 180.0;
/// How fast a view turned further than that when the body sits down comes round to it,
/// radians per second [engineering choice].
const NECK_RETURN_RATE: f32 = 4.0;
/// And how far down it tilts, radians [engineering choice: 75 degrees, head and eyes;
/// steeper, the view is down the body's own collar].
const NECK_DOWN: f32 = 75.0 * std::f32::consts::PI / 180.0;

/// A clip choice, for the clip caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClipKey {
    Idle(Stance),
    Move(Stance, bool, bool, Dir, Option<&'static str>),
    Aim(Stance, &'static str, Aim, bool),
    Block(Stance, &'static str),
    Holster(Stance, &'static str),
    Equip(Stance, &'static str, bool),
    Jump(Stance, Option<&'static str>, Option<Dir>, bool),
    Turn(Stance, Option<&'static str>, bool),
    /// Weapon prefix, attack clip stem (`attackright`), down the sights.
    Attack(Stance, &'static str, &'static str, bool),
    /// Weapon prefix, WEAP reload animation (`ReloadL`).
    Reload(Stance, &'static str, &'static str),
}

impl ClipKey {
    fn candidates(self) -> Vec<String> {
        match self {
            ClipKey::Idle(s) => idle_clips(s),
            ClipKey::Move(s, female, fast, d, w) => move_clips(s, female, fast, d, w),
            ClipKey::Aim(s, p, a, sights) => aim_clips(s, p, a, sights),
            ClipKey::Block(s, p) => block_clips(s, p),
            ClipKey::Holster(s, p) => holster_clips(s, p),
            ClipKey::Equip(s, p, e) => equip_clips(s, p, e),
            ClipKey::Jump(s, w, d, land) => jump_clips(s, w, d, land),
            ClipKey::Turn(s, w, left) => turn_clips(s, w, left),
            ClipKey::Attack(s, p, a, sights) => attack_clips(s, p, a, sights),
            ClipKey::Reload(s, p, r) => reload_clips(s, p, r),
        }
    }
}

/// A loaded clip with what the body reads from it.
struct ClipInfo {
    path: String,
    clip: Arc<Clip>,
    keys: ClipKeys,
    /// Ground speed of its accumulation root (movement clips).
    speed: Option<f32>,
    /// Its track on the `Weapon` node.
    weapon_track: Option<usize>,
}

impl ClipInfo {
    fn new(path: String, clip: Arc<Clip>, speed: Option<f32>) -> ClipInfo {
        let keys = ClipKeys::parse(&clip.text_keys);
        let weapon_track = clip.tracks.iter().position(|(n, _)| n.eq_ignore_ascii_case("Weapon"));
        ClipInfo { path, clip, keys, speed, weapon_track }
    }

    fn fade(&self) -> f32 {
        self.keys.blend_seconds(DEFAULT_FADE)
    }

    /// Where the clip leaves the `Weapon` node at its end (`bind`: the node's bind pose,
    /// for the parts the track does not key).
    fn weapon_end(&self, bind: Transform) -> Option<Transform> {
        let (_, t) = &self.clip.tracks[self.weapon_track?];
        let time = self.clip.stop;
        Some(Transform {
            translation: t.translation.sample(time).map_or(bind.translation, Vec3::from_array),
            rotation: t.rotation_at(time).map_or(bind.rotation, |[w, x, y, z]| Quat::from_xyzw(x, y, z, w)),
            scale: Vec3::splat(t.scale.sample(time).map_or(bind.scale.x, |[s]| s)),
        })
    }
}

/// Which view a rig is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Third,
    First,
}

/// One shape of the weapon model.
struct WeaponPart {
    entity: Entity,
    /// The model nodes above it whose names begin `##`, each with whether the clips
    /// last showed it: clips show and hide such nodes (the minigun's gun while it is
    /// holstered) [observed: animation.md §9.7].
    tags: Vec<(String, bool)>,
    /// Whether it hangs from a bone of its own (the minigun's backpack) rather than
    /// from the `Weapon` node.
    on_bone: bool,
    /// The visibility last written to the entity.
    shown: Option<bool>,
}

/// One skeleton with its parts, clips and weapon.
struct Rig {
    view: View,
    root: Entity,
    /// The skeleton's folder (`meshes\characters\_male\`).
    folder: String,
    animator: Animator,
    clips: HashMap<ClipKey, Option<Arc<ClipInfo>>>,
    /// The up and down forms of clips, by path (failures too).
    variants: HashMap<String, Option<Arc<ClipInfo>>>,
    /// Third person: the head bone and the eye in its space.
    head: Option<usize>,
    eye_local: Vec3,
    /// First person: the node that turns with the pitch, and the camera node.
    looking: Option<usize>,
    camera: Option<usize>,
    weapon_node: Option<usize>,
    /// The skeleton node the weapon node should hang from, and the one it was last
    /// attached to.
    weapon_parent: Option<usize>,
    attached: Option<usize>,
    weapon_model: Option<Entity>,
    /// The weapon model's shapes.
    weapon_parts: Vec<WeaponPart>,
    /// Third person: drawn entities to hide when it stands in for the first-person rig.
    head_entities: Vec<Entity>,
    /// Third person: the drawn entities the first-person rig has its own of (arms, hands,
    /// gloves, the Pip-Boy), to hide while that rig's arms are up in front of the body.
    arm_entities: Vec<Entity>,
    /// Whether the rig is shown, and whether its head, its arms and its weapon are (last
    /// applied).
    shown: Option<bool>,
    head_shown: Option<bool>,
    arms_shown: Option<bool>,
    weapon_shown: Option<bool>,
    /// The furniture clip it plays (`BodyClip::serial`), while it plays one.
    furniture: Option<u32>,
    /// Seconds since the last attack started, and the repeated part of its clip, if it
    /// has one: from and to, seconds into the clip.
    since_attack: f32,
    attack_loop: Option<(f32, f32)>,
}

/// Where a layer's time is after going round the repeated part of its clip, `start` to
/// `end` seconds into it: unchanged before `end`.
fn loop_time(time: f32, start: f32, end: f32) -> f32 {
    if time < end || start >= end {
        return time;
    }
    start + (time - end).rem_euclid(end - start)
}

impl Rig {
    /// The first existing file of a clip choice in this rig's folder (cached, failures
    /// too).
    fn clip(&mut self, ctx: &mut ActorCtx, key: ClipKey) -> Option<Arc<ClipInfo>> {
        if let Some(c) = self.clips.get(&key) {
            return c.clone();
        }
        let found = key.candidates().into_iter().map(|c| format!("{}{c}", self.folder)).find(|p| ctx.models.data.vfs.exists(p)).and_then(|path| {
            let clip = ctx.clip(&path)?;
            Some(Arc::new(ClipInfo::new(path, clip.clone(), clip.root_speed())))
        });
        match &found {
            Some(c) => debug!("player {:?} rig: {key:?} plays {} ({:.3} s, shot at {:?}, repeats to {:?})", self.view, c.path, c.clip.length(), c.keys.hit, c.keys.loop_end),
            None => debug!("player {:?} rig: {key:?}: none of {:?} in {}", self.view, key.candidates(), self.folder),
        }
        self.clips.insert(key, found.clone());
        found
    }

    /// The up or down form of clip `c` (`2hrattackleftup.kf` beside `2hrattackleft.kf`),
    /// if its folder has one (cached, failures too).
    fn variant(&mut self, ctx: &mut ActorCtx, c: &ClipInfo, aim: Aim) -> Option<Arc<ClipInfo>> {
        let path = aim_variant(&c.path, aim);
        if let Some(v) = self.variants.get(&path) {
            return v.clone();
        }
        let found = if ctx.models.data.vfs.exists(&path) { ctx.clip(&path) } else { None }.map(|clip| Arc::new(ClipInfo::new(path.clone(), clip, None)));
        debug!("player {:?} rig: {aim:?} form of {}: {}", self.view, c.path, if found.is_some() { &path } else { "none" });
        self.variants.insert(path, found.clone());
        found
    }

    /// The draw and holster timings of weapon `prefix` for this rig.
    fn transitions(&mut self, ctx: &mut ActorCtx, stance: Stance, prefix: &'static str) -> (Transition, Transition) {
        let draw = self.clip(ctx, ClipKey::Equip(stance, prefix, true));
        let away = self.clip(ctx, ClipKey::Equip(stance, prefix, false));
        (
            draw.map_or(Transition::new(0.0, None), |c| Transition::new(c.clip.length(), c.keys.attach)),
            away.map_or(Transition::new(0.0, None), |c| Transition::new(c.clip.length(), c.keys.detach)),
        )
    }
}

struct Body {
    worn: Vec<FormId>,
    female: bool,
    scale: f32,
    third: Rig,
    first: Option<Rig>,
}

/// State kept across body rebuilds (an outfit change rebuilds the body).
struct Motion {
    weapon: Option<FormId>,
    prefix: &'static str,
    hands: WeaponHands,
    events: Vec<HandEvent>,
    speed: f32,
    last_feet: Option<[f32; 3]>,
    airborne: f32,
    last_heading: Option<f32>,
    turn_rate: f32,
    /// Once-a-second status (debug log).
    log_elapsed: f32,
    /// On furniture with the body in first person: the view has come within the neck's
    /// reach of the way the body faces, and is held there.
    seat_settled: bool,
    /// The same: the way the body faced last frame, to turn the view with it.
    seat_facing: Option<f32>,
    /// The attack, reload and spin-up counts last acted on (`WeaponAnim`).
    attacks_seen: u32,
    reloads_seen: u32,
    spin_ups_seen: u32,
    /// Interned clip names (attack stems, reload names) for the clip keys.
    names: HashMap<String, &'static str>,
}

impl Motion {
    fn intern(&mut self, s: String) -> &'static str {
        if let Some(v) = self.names.get(&s) {
            return v;
        }
        let v: &'static str = Box::leak(s.clone().into_boxed_str());
        self.names.insert(s, v);
        v
    }
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            weapon: None,
            prefix: UNARMED,
            hands: WeaponHands::default(),
            events: Vec::new(),
            speed: 0.0,
            last_feet: None,
            airborne: 0.0,
            last_heading: None,
            turn_rate: 0.0,
            log_elapsed: 0.0,
            seat_settled: false,
            seat_facing: None,
            attacks_seen: 0,
            reloads_seen: 0,
            spin_ups_seen: 0,
            names: HashMap::new(),
        }
    }
}

#[derive(Resource, Default)]
pub struct PlayerBody {
    /// Draw the body in first person too (`--first-person-body`, the settings page).
    pub first_person_body: bool,
    body: Option<Body>,
    motion: Motion,
    /// Why the body could not be built (reported once).
    failed: Option<String>,
}

impl PlayerBody {
    /// Whether the body drawn is a woman's; `None` without a body.
    pub fn female(&self) -> Option<bool> {
        self.body.as_ref().map(|b| b.female)
    }
}

pub struct PlayerBodyPlugin {
    /// `--first-person-body`.
    pub first_person_body: bool,
}

impl Plugin for PlayerBodyPlugin {
    fn build(&self, app: &mut App) {
        // Furniture (`furniture.rs`) says what the body plays before it is posed.
        app.insert_resource(PlayerBody { first_person_body: self.first_person_body, ..default() })
            .init_resource::<crate::furniture::BodyOverride>()
            .init_resource::<crate::furniture::PlayerUse>()
            .add_systems(Update, (crate::furniture::drive_player, ensure_body, drive_body).chain().in_set(nvf_play::player::BodySet));
    }
}

/// The player's equipped armour and weapon in the simulation.
fn equipped(sim: &SimState, world: &ActorWorld) -> (Vec<FormId>, Option<FormId>) {
    let Some(inv) = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()) else { return (Vec::new(), None) };
    let eq = inv.equipped();
    let worn = eq.iter().copied().filter(|id| world.actors.armors.contains_key(id)).collect();
    let weapon = eq.iter().copied().find(|id| world.actors.weapons.contains_key(id));
    (worn, weapon)
}

/// The centre of the eye parts in the head bone's space, from their meshes [observed:
/// the race's eye parts hang from `Bip01 Head` by `Prn`, actors.md §1.2].
fn eye_centre(skel: &PreparedSkeleton, head: usize, parts: &[&nvf_assets::actor::PreparedPart]) -> Option<Vec3> {
    let mut sum = Vec3::ZERO;
    let mut n = 0.0;
    for part in parts {
        let Some(&b) = skel.by_name.get(part.attach_bone()) else { continue };
        for s in &part.shapes {
            let Some(VertexAttributeValues::Float32x3(pos)) = s.cpu.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
            if pos.is_empty() {
                continue;
            }
            let c = pos.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from_array(*p)) / pos.len() as f32;
            // Into the attachment bone's space, then the head's (normally the same bone).
            let local = rigid_local(skel, b, &part.root, &s.transform).transform_point3(c);
            let world = skel.bind_world[b].transform_point3(local);
            sum += skel.bind_world[head].inverse().transform_point3(world);
            n += 1.0;
        }
    }
    (n > 0.0).then(|| sum / n)
}

/// A point in front of the head when the eyes cannot be measured: 9 units forward and 7
/// up from the head bone at the bind pose [engineering choice].
fn default_eye(skel: &PreparedSkeleton, head: usize) -> Vec3 {
    let bind = skel.bind_world[head];
    let world = bind.translation + Vec3A::new(0.0, 9.0, 7.0);
    bind.inverse().transform_point3(world.into())
}

#[allow(clippy::too_many_arguments)]
fn ensure_body(
    mut commands: Commands,
    world: Option<Res<ActorWorld>>,
    sim: Res<SimState>,
    mut state: ResMut<PlayerBody>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
) {
    let (Some(world), Ok(world_root)) = (world, root.single()) else { return };
    if state.failed.is_some() {
        return;
    }
    let (worn, weapon) = equipped(&sim, &world);
    let state = &mut *state;
    let mut rebuilt = false;
    if state.body.as_ref().is_none_or(|b| b.worn != worn) {
        if let Some(old) = state.body.take() {
            commands.entity(old.third.root).despawn();
            if let Some(f) = old.first {
                commands.entity(f.root).despawn();
            }
        }
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        match build(&mut commands, world_root, &world, &mut ctx, &worn) {
            Ok(b) => {
                info!(
                    "player body: third-person rig {} nodes (eye {:.1?} in the head's space), first-person rig {}, wearing {:?}",
                    b.third.animator.nodes.len(),
                    b.third.eye_local,
                    b.first.as_ref().map_or("missing".to_string(), |f| format!("{} nodes from {}", f.animator.nodes.len(), f.folder)),
                    worn.iter().map(|w| w.to_string()).collect::<Vec<_>>()
                );
                state.body = Some(b);
                rebuilt = true;
            }
            Err(e) => {
                warn!("player body: {e}; playing without one");
                state.failed = Some(e);
                return;
            }
        }
    }
    let Some(body) = state.body.as_mut() else { return };
    let m = &mut state.motion;
    if !rebuilt && m.weapon == weapon {
        return;
    }
    m.weapon = weapon;
    let w = weapon.and_then(|id| world.actors.weapons.get(&id));
    m.prefix = w.and_then(|w| w.anim_type.as_deref()).and_then(AnimType::from_name).map_or(UNARMED, AnimType::prefix);
    info!("player weapon: {}, animations {}", weapon.map_or("none (fists)".to_string(), |w| w.to_string()), m.prefix);
    for rig in std::iter::once(&mut body.third).chain(body.first.as_mut()) {
        if let Some(e) = rig.weapon_model.take() {
            commands.entity(e).despawn();
        }
        rig.animator.set_extra(Vec::new());
        for p in rig.weapon_parts.drain(..).filter(|p| p.on_bone) {
            commands.entity(p.entity).despawn();
        }
        let model = match rig.view {
            View::Third => w.and_then(|w| w.model.as_deref()),
            View::First => w.and_then(|w| w.first_person_model.as_deref().or(w.model.as_deref())),
        };
        let (Some(path), Some(node)) = (model.map(fnv_actor::resolve::mesh_path), rig.weapon_node) else { continue };
        match models.model(&path) {
            Some(pm) => {
                let holder = commands.spawn((Name::new(format!("Player weapon {path}")), Transform::IDENTITY, Visibility::Inherited)).id();
                commands.entity(rig.animator.nodes[node]).add_child(holder);
                // A node of the model whose `UPB` string names a bone hangs its shapes
                // from that bone instead [observed: the minigun's `Backpack` node says
                // `Bip01 Spine2`; hypothesis: that is its meaning; animation.md §9.7].
                let bones = models.node_strings(&path, "UPB");
                let skel = rig.animator.skel.clone();
                let in_hand = m.hands.in_hand;
                // The model's nodes keep their tree and their names, so the clips can
                // move its parts [observed: the clips' tracks on `##Slide`, `##Clip`,
                // `##MinigunBarrel` name nodes of the weapon models; animation.md §9.8].
                let local = |a: &Affine3A| Transform::from_matrix(Mat4::from(*a));
                let part_nodes: Vec<Entity> = pm.nodes.iter().map(|n| commands.spawn((local(&n.local), Visibility::Inherited)).id()).collect();
                for (n, e) in pm.nodes.iter().zip(&part_nodes) {
                    commands.entity(n.parent.map_or(holder, |p| part_nodes[p])).add_child(*e);
                }
                let mut extra: Vec<ExtraNode> =
                    pm.nodes.iter().zip(&part_nodes).filter_map(|(n, e)| Some(ExtraNode { name: n.name.clone()?, entity: *e, bind: local(&n.local) })).collect();
                for s in &pm.shapes {
                    let bone = s.ancestors.iter().find_map(|a| skel.by_name.get(bones.get(a)?.trim()).copied());
                    if bone.is_some() && rig.view == View::First {
                        // On the body, behind the first-person view.
                        continue;
                    }
                    let tags: Vec<(String, bool)> =
                        s.ancestors.iter().chain(s.name.as_ref()).filter(|n| n.starts_with("##")).map(|n| (n.clone(), true)).collect();
                    let shown = tags.is_empty() || in_hand;
                    let visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
                    // On a bone of its own the shape keeps its place in the whole model;
                    // else it sits under its node, which the clips may move.
                    let (parent, place) = match bone {
                        Some(b) => (rig.animator.nodes[b], local(&s.transform)),
                        None => (s.node.map_or(holder, |n| part_nodes[n]), local(&s.own)),
                    };
                    let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), place, visibility)).id();
                    commands.entity(parent).add_child(e);
                    if let (None, Some(name)) = (bone, &s.name) {
                        extra.push(ExtraNode { name: name.clone(), entity: e, bind: place });
                    }
                    rig.weapon_parts.push(WeaponPart { entity: e, tags, on_bone: bone.is_some(), shown: Some(shown) });
                }
                rig.weapon_model = Some(holder);
                rig.weapon_shown = None;
                info!("player weapon model ({:?} person): {path}, {} nodes, {} named parts for the clips", rig.view, pm.nodes.len(), extra.len());
                rig.animator.set_extra(extra);
            }
            None => warn!("player weapon {path}: model not loaded"),
        }
    }
}

fn build(commands: &mut Commands, parent: Entity, world: &ActorWorld, ctx: &mut ActorCtx, worn: &[FormId]) -> Result<Body, String> {
    let a = world.resolve_wearing(FormId::PLAYER_BASE.0, FormId::PLAYER_REF.0, Some(worn))?;
    let scale = a.race_height as f32;
    let third = build_rig(commands, parent, world, ctx, &a, View::Third, None)?;
    let vfs = ctx.models.data.vfs.clone();
    let (fa, shows) = first_person::appearance(&world.actors, &a, |p| vfs.exists(p));
    let first = match build_rig(commands, parent, world, ctx, &fa, View::First, Some(&shows)) {
        Ok(r) => Some(r),
        Err(e) => {
            warn!("player body: no first-person rig ({e}); the third-person body's head gives the view");
            None
        }
    };
    Ok(Body { worn: worn.to_vec(), female: a.female, scale, third, first })
}

/// Spawn one rig of appearance `a` under `parent` (its transform set each frame).
/// `shows` (first person): per part of `a`, how it shows.
fn build_rig(commands: &mut Commands, parent: Entity, world: &ActorWorld, ctx: &mut ActorCtx, a: &Appearance, view: View, shows: Option<&[Show]>) -> Result<Rig, String> {
    let skel = ctx.skeleton(&a.skeleton).ok_or_else(|| format!("skeleton {} not loaded", a.skeleton))?;
    let mut stats = crate::actors::ActorStats::default();
    let mut uses = part_uses(a, world, ctx, &mut stats);
    if stats.parts_missing > 0 {
        warn!("player {view:?} rig: {} parts not loaded", stats.parts_missing);
    }
    if let Some(shows) = shows {
        // Upper-body models show their `Arms` shapes and the Pip-Boy sleeve only.
        for (i, u) in &mut uses {
            if shows[*i] == Show::Arms {
                let hide = u.part.shapes.iter().filter_map(|s| s.name.clone().filter(|n| !is_first_person_shape(n)));
                u.hidden_shapes.extend(hide);
            }
        }
    }
    for (i, u) in &uses {
        let p = &a.parts[*i];
        let drawn: Vec<&str> = u.part.shapes.iter().filter_map(|s| s.name.as_deref()).filter(|n| !u.hidden_shapes.iter().any(|h| h == n)).collect();
        debug!("player {view:?} rig part {} {} {}: shapes {drawn:?}", p.role, p.form, p.model);
    }
    let head = skel.by_name.get("Bip01 Head").copied();
    let eye_local = match view {
        View::Third => {
            let eyes: Vec<_> = uses.iter().filter(|(i, _)| a.parts[*i].role.ends_with("_eye")).map(|(_, u)| &*u.part).collect();
            head.and_then(|h| eye_centre(&skel, h, &eyes).or_else(|| Some(default_eye(&skel, h)))).unwrap_or(Vec3::new(0.0, 0.0, 120.0))
        }
        View::First => Vec3::ZERO,
    };
    let head_part = |i: usize| {
        let p = &a.parts[i];
        p.role.starts_with("head") || p.role == "hair" || world.actors.armors.get(&p.form).is_some_and(|ar| ar.slots & HEAD_SLOTS != 0)
    };
    let hide: Vec<bool> = uses.iter().map(|(i, _)| head_part(*i)).collect();
    // Third person: per part, how the first-person rig shows it.
    let arms: Vec<Option<Show>> = uses.iter().map(|(i, _)| (view == View::Third).then(|| first_person::show(&world.actors, &a.parts[*i])).flatten()).collect();
    let uses: Vec<PartUse> = uses.into_iter().map(|(_, u)| u).collect();
    let spawned = spawn_actor(commands, parent, Transform::from_scale(Vec3::splat(a.race_height as f32)), &skel, &uses);
    commands.entity(spawned.root).insert(Name::new(format!("Player body ({view:?} person)")));
    let head_entities = spawned.drawn.iter().zip(&spawned.part_of).filter(|(_, &p)| hide[p]).map(|(e, _)| *e).collect();
    let arm_entities = (spawned.drawn.iter().zip(&spawned.part_of).zip(&spawned.shape_of))
        .filter(|((_, &p), &s)| shape_shows(arms[p], uses[p].part.shapes[s].name.as_deref()))
        .map(|((e, _), _)| *e)
        .collect();
    let folder = a.skeleton.rfind('\\').map_or(String::new(), |i| a.skeleton[..=i].to_string());
    let node = |n: &str| skel.by_name.get(n).copied();
    let weapon_node = node("Weapon");
    let weapon_parent = weapon_node.and_then(|w| skel.parents[w]);
    let (looking, camera) = match view {
        View::First => (node("Bip01 Looking"), node("Camera1st")),
        View::Third => (None, None),
    };
    if view == View::First && camera.is_none() {
        return Err(format!("{} has no Camera1st node", a.skeleton));
    }
    let animator = Animator::new(skel.clone(), spawned.nodes, Some("Bip01"));
    Ok(Rig {
        view,
        root: spawned.root,
        folder,
        animator,
        clips: HashMap::new(),
        variants: HashMap::new(),
        head,
        eye_local,
        looking,
        camera,
        weapon_node,
        weapon_parent,
        attached: weapon_parent,
        weapon_model: None,
        weapon_parts: Vec::new(),
        head_entities,
        arm_entities,
        shown: None,
        head_shown: None,
        arms_shown: None,
        weapon_shown: None,
        furniture: None,
        since_attack: f32::INFINITY,
        attack_loop: None,
    })
}

/// What both rigs play this frame.
struct Frame {
    dt: f32,
    stance: Stance,
    dirs: Option<[(Dir, f32); 2]>,
    fast: bool,
    female: bool,
    /// The weapon prefix, and whether the weapon is out.
    prefix: &'static str,
    drawn: Option<&'static str>,
    hands: WeaponHands,
    airborne: f32,
    landing: bool,
    turn_rate: f32,
    pitch: f32,
    /// The third-person body is drawn in first person: it stays upright whatever the
    /// pitch, as the arms rig alone follows the view.
    upright: bool,
    /// Block held.
    aiming: bool,
    speed: f32,
    /// The attack and reload clips of the weapon, and whether each starts this frame.
    attack: Option<&'static str>,
    reload: Option<&'static str>,
    attack_now: bool,
    reload_now: bool,
    /// The weapon starts to spin up this frame: its attack clip plays from the start.
    spin_now: bool,
}

/// Pose `rig` for the frame, apply the weapon events, place it at `root`; returns the
/// eye (game space): the camera node in first person, between the eyes in third.
fn pose_rig(rig: &mut Rig, f: &Frame, events: &[HandEvent], root: Transform, ctx: &mut ActorCtx, transforms: &mut Query<&mut Transform>) -> Option<Vec3> {
    let draw = rig.clip(ctx, ClipKey::Equip(f.stance, f.prefix, true));
    let put_away = rig.clip(ctx, ClipKey::Equip(f.stance, f.prefix, false));
    let holster = rig.clip(ctx, ClipKey::Holster(f.stance, f.prefix));
    let skel = rig.animator.skel.clone();
    let bone = |name: &str| skel.by_name.get(name).copied();
    for ev in events {
        match ev {
            HandEvent::StartDraw => {
                if let Some(c) = &draw {
                    rig.animator.restart(&c.clip, TAG_EQUIP, 1.0, Mode::Once, c.fade());
                }
            }
            HandEvent::StartHolster => {
                if let Some(c) = &put_away {
                    rig.animator.restart(&c.clip, TAG_EQUIP, 1.0, Mode::Once, c.fade());
                }
            }
            HandEvent::ToHand => {
                rig.animator.cut(TAG_HOLSTER);
                rig.weapon_parent = draw.as_ref().and_then(|c| c.keys.prn.as_ref()).and_then(|(_, b)| bone(b)).or_else(|| bone("Bip01 R Hand"));
            }
            HandEvent::ToHolster | HandEvent::Drawn | HandEvent::Holstered => {}
        }
    }
    // In the hand, the `Weapon` node rests where the draw clip leaves it: the aim and
    // movement clips of some types (`2hh`, `1hp`) do not drive it, and the draw, attack
    // and reload clips that do agree on the place [observed: animation.md §9.7;
    // hypothesis: the game leaves the node where the last clip put it].
    if let Some(n) = rig.weapon_node {
        let rest = draw.as_ref().filter(|_| f.hands.in_hand).and_then(|c| c.weapon_end(skel.locals[n]));
        rig.animator.set_rest(n, rest);
    }
    if !f.hands.in_hand {
        // The holster clip's `prn:` names the holster bone [observed: `Bip01 Pelvis` for
        // pistols, `Bip01 Spine2` for rifles, `Bip01 Translate` in first person]; the
        // unequip clip's says the same.
        let from_keys = holster.as_ref().and_then(|c| c.keys.prn.as_ref()).or_else(|| put_away.as_ref().and_then(|c| c.keys.prn.as_ref()));
        if let Some(b) = from_keys.and_then(|(_, b)| bone(b)) {
            rig.weapon_parent = Some(b);
        }
    }

    // The frame's clips.
    let mut want: Vec<(Arc<ClipInfo>, u32, f32, Mode)> = Vec::with_capacity(8);
    if let Some(c) = rig.clip(ctx, ClipKey::Idle(f.stance)) {
        want.push((c, TAG_IDLE, 1.0, Mode::Loop));
    }
    let mut expected_speed = 0.0;
    let main_dir = f.dirs.map(|w| if w[0].1 >= w[1].1 { w[0].0 } else { w[1].0 });
    if f.airborne >= AIRBORNE_LOOP {
        if let Some(c) = rig.clip(ctx, ClipKey::Jump(f.stance, f.drawn, main_dir, false)) {
            want.push((c, TAG_JUMP, 1.0, Mode::Loop));
        }
    } else if let Some(w) = f.dirs {
        for (i, (dir, weight)) in w.into_iter().enumerate() {
            if weight <= 0.0 {
                continue;
            }
            if let Some(c) = rig.clip(ctx, ClipKey::Move(f.stance, f.female, f.fast, dir, f.drawn)) {
                expected_speed += weight * c.speed.unwrap_or(0.0);
                want.push((c, TAG_MOVE + i as u32, weight, Mode::Synced));
            }
        }
    } else if f.turn_rate.abs() > TURN_RATE {
        // Heading grows clockwise: a negative rate turns left.
        if let Some(c) = rig.clip(ctx, ClipKey::Turn(f.stance, f.drawn, f.turn_rate < 0.0)) {
            want.push((c, TAG_TURN, 1.0, Mode::Loop));
        }
    }
    if f.landing {
        if let Some(c) = rig.clip(ctx, ClipKey::Jump(f.stance, f.drawn, main_dir, true)) {
            rig.animator.restart(&c.clip, TAG_LAND, 1.0, Mode::Once, c.fade());
        }
    }
    // First person pitches the whole arms rig instead of bending the body.
    let aim_w = match rig.view {
        View::Third if !f.upright => aim_weights(f.pitch),
        _ => [(Aim::Level, 1.0), (Aim::Up, 0.0), (Aim::Down, 0.0)],
    };
    if f.hands.in_hand && f.hands.phase != WeaponPhase::Drawing {
        if f.aiming && blocks(f.prefix) {
            if let Some(c) = rig.clip(ctx, ClipKey::Block(f.stance, f.prefix)) {
                want.push((c, TAG_AIM, 1.0, Mode::Loop));
            }
        } else {
            for (i, (aim, w)) in aim_w.into_iter().enumerate() {
                // The level clip plays at any weight: the up and down clips of rifles
                // and two-handed melee weapons drive the spine only, so the arms are the
                // level clip's alone and both hands stay on the weapon [observed:
                // animation.md §9.6].
                if w <= 0.0 && aim != Aim::Level {
                    continue;
                }
                if let Some(c) = rig.clip(ctx, ClipKey::Aim(f.stance, f.prefix, aim, f.aiming)) {
                    want.push((c, TAG_AIM + i as u32, w, Mode::Loop));
                }
            }
        }
    }
    if !f.hands.in_hand {
        if let Some(c) = &holster {
            want.push((c.clone(), TAG_HOLSTER, 1.0, Mode::Once));
        }
    }
    if f.hands.in_hand && (f.attack_now || f.spin_now) {
        if let Some(c) = f.attack.and_then(|a| rig.clip(ctx, ClipKey::Attack(f.stance, f.prefix, a, f.aiming))) {
            // A clip with a `Loop` key plays on through the shots that follow while it
            // runs (a minigun's spin) instead of starting again at each, going round from
            // its `Fire` key to its `Loop` key [documented: GECK wiki NiTextKeyExtraData,
            // combat.md §4.2; observed: the barrel turns at one speed between the two,
            // animation.md §9.8]. A spin-up starts it from the beginning, so the first
            // shot, which combat holds back until then, meets the `Fire` key.
            rig.attack_loop = c.keys.hit.zip(c.keys.loop_end).filter(|(from, to)| from < to).map(|(from, to)| (from - c.clip.start, to - c.clip.start));
            let going = !f.spin_now && rig.attack_loop.is_some() && rig.animator.layers.iter().any(|l| l.tag == TAG_ATTACK && Arc::ptr_eq(&l.clip, &c.clip) && !l.finished());
            if !going {
                rig.animator.restart(&c.clip, TAG_ATTACK, aim_w[0].1, Mode::Once, ATTACK_FADE);
                debug!("player {:?} rig: {} moves the weapon's {:?}", rig.view, c.path, rig.animator.extra_driven(&c.clip));
                // The attack's own up and down forms keep the body bent with the pitch while
                // it plays, as the aim's do [observed: `2hrattackleftup.kf`, §9.6].
                for (i, (aim, w)) in aim_w.into_iter().enumerate().skip(1) {
                    let tag = TAG_ATTACK + i as u32;
                    rig.animator.cut(tag);
                    if let Some(v) = (rig.view == View::Third).then(|| rig.variant(ctx, &c, aim)).flatten() {
                        rig.animator.restart(&v.clip, tag, w, Mode::Once, ATTACK_FADE);
                    }
                }
            }
            // A spin-up is not a shot: the clip runs on to its `Fire` key by itself.
            if f.attack_now {
                rig.since_attack = 0.0;
            }
        }
    }
    if f.hands.in_hand && f.reload_now {
        if let Some(c) = f.reload.and_then(|r| rig.clip(ctx, ClipKey::Reload(f.stance, f.prefix, r))) {
            rig.animator.restart(&c.clip, TAG_RELOAD, 1.0, Mode::Once, c.fade());
            debug!("player {:?} rig: {} moves the weapon's {:?}", rig.view, c.path, rig.animator.extra_driven(&c.clip));
        }
    }
    if matches!(f.hands.phase, WeaponPhase::Drawing | WeaponPhase::Holstering) {
        let c = if f.hands.phase == WeaponPhase::Drawing { &draw } else { &put_away };
        if let Some(c) = c {
            want.push((c.clone(), TAG_EQUIP, 1.0, Mode::Once));
        }
    }
    let a = &mut rig.animator;
    a.begin();
    for (c, tag, w, mode) in &want {
        let fade = if *tag == TAG_HOLSTER { 0.0 } else { c.fade() };
        a.want(&c.clip, *tag, *w, *mode, fade);
    }
    // Clips that play once to their end once started: landing, attack, reload.
    for (tag, w) in [(TAG_LAND, 1.0), (TAG_ATTACK, aim_w[0].1), (TAG_ATTACK + 1, aim_w[1].1), (TAG_ATTACK + 2, aim_w[2].1), (TAG_RELOAD, 1.0)] {
        if let Some(c) = a.layers.iter().find(|l| l.tag == tag && !l.finished()).map(|l| l.clip.clone()) {
            a.want(&c, tag, w, Mode::Once, 0.0);
        }
    }
    a.end();
    // Movement clips play at the speed the character actually moves [hypothesis: the
    // game matches the clip's root motion to the ground speed].
    let rate = if expected_speed > 1.0 { (f.speed / expected_speed).clamp(0.25, 2.5) } else { 1.0 };
    a.advance(f.dt, rate);
    // While the shots keep coming, an attack clip with a repeated part goes round it;
    // after the last it plays out from where it is.
    rig.since_attack += f.dt;
    if let Some((from, to)) = rig.attack_loop.filter(|_| rig.since_attack <= LOOP_HOLD) {
        for l in a.layers.iter_mut().filter(|l| (TAG_ATTACK..TAG_ATTACK + 3).contains(&l.tag)) {
            l.time = loop_time(l.time, from, to);
        }
    }
    a.pose();
    if let Some(l) = rig.looking {
        // The arms rig turns about `Bip01 Looking` with the view's pitch (about the
        // actor's right axis, x) [hypothesis: the node's role, from its name and place].
        let bind = a.skel.locals[l];
        a.locals[l].rotation = Quat::from_rotation_x(f.pitch) * bind.rotation;
    }
    a.apply(transforms);
    if let Ok(mut t) = transforms.get_mut(rig.root) {
        *t = root;
    }
    let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
    match rig.view {
        View::First => rig.camera.map(|c| (root_m * a.model_space(c)).translation.into()),
        View::Third => rig.head.map(|h| (root_m * a.model_space(h)).transform_point3(rig.eye_local)),
    }
}

/// Pose the third-person rig with a furniture clip alone and place it at `root`
/// (`crate::furniture`); returns the eye. The clips of one use play in different frames,
/// so a new clip replaces the last at once instead of fading in.
fn pose_furniture(rig: &mut Rig, o: &crate::furniture::BodyClip, dt: f32, root: Transform, transforms: &mut Query<&mut Transform>) -> Option<Vec3> {
    let a = &mut rig.animator;
    let mode = if o.looped { Mode::Loop } else { Mode::Once };
    if rig.furniture != Some(o.serial) {
        a.layers.clear();
        a.restart(&o.clip, TAG_FURNITURE, 1.0, mode, 0.0);
        rig.furniture = Some(o.serial);
    }
    a.begin();
    a.want(&o.clip, TAG_FURNITURE, 1.0, mode, 0.0);
    a.end();
    a.advance(dt, 1.0);
    a.pose();
    a.apply(transforms);
    if let Ok(mut t) = transforms.get_mut(rig.root) {
        *t = root;
    }
    let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
    rig.head.map(|h| (root_m * a.model_space(h)).transform_point3(rig.eye_local))
}

/// The view's heading on furniture with the body in first person: within [`NECK_TURN`] of
/// `facing` (the body's heading) it is the player's; the mouse cannot take it further
/// once it has been there (`settled`); a view further round when the body sits down
/// comes to the nearer limit at [`NECK_RETURN_RATE`]. Also whether it is within reach.
fn seated_heading(heading: f32, facing: f32, settled: &mut bool, dt: f32) -> (f32, bool) {
    use std::f32::consts::{PI, TAU};
    let turned = (heading - facing + PI).rem_euclid(TAU) - PI;
    let over = turned.abs() - NECK_TURN;
    if over <= 0.0 {
        *settled = true;
        return (heading, true);
    }
    let back = if *settled { over } else { over.min(NECK_RETURN_RATE * dt) };
    let within = back >= over;
    *settled |= within;
    ((facing + turned.signum() * (turned.abs() - back)).rem_euclid(TAU), within)
}

/// The compass heading bone `b` of the posed rig faces when the rig's root faces
/// `heading`: the root's, turned by the bone's turn about the vertical since the bind
/// pose (the clips of getting on furniture turn the body inside the rig).
fn bone_heading(rig: &Rig, b: usize, heading: f32) -> f32 {
    let a = &rig.animator;
    let forward = a.model_space(b).matrix3 * a.skel.bind_world[b].matrix3.inverse() * Vec3A::Y;
    heading + forward.x.atan2(forward.y)
}

/// What of the body is drawn.
#[derive(Clone, Copy)]
struct Shown {
    third: bool,
    first: bool,
    /// Of the third-person rig: its head, its arms and hands, the weapon on its `Weapon`
    /// node.
    head: bool,
    arms: bool,
    weapon: bool,
}

/// Show or hide the rigs and the third-person rig's head, arms and weapon.
fn show_rigs(body: &mut Body, visibility: &mut Query<&mut Visibility>, s: Shown) {
    let mut show = |entities: &[Entity], shown: &mut Option<bool>, on: bool| {
        if *shown != Some(on) {
            for e in entities {
                if let Ok(mut v) = visibility.get_mut(*e) {
                    *v = if on { Visibility::Inherited } else { Visibility::Hidden };
                }
            }
            *shown = Some(on);
        }
    };
    let third = &mut body.third;
    show(&[third.root], &mut third.shown, s.third);
    show(&third.head_entities, &mut third.head_shown, s.head);
    show(&third.arm_entities, &mut third.arms_shown, s.arms);
    show(third.weapon_model.as_slice(), &mut third.weapon_shown, s.weapon);
    if let Some(f) = body.first.as_mut() {
        show(&[f.root], &mut f.shown, s.first);
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_body(
    mut commands: Commands,
    time: Res<Time>,
    d: Res<PlayWorld>,
    intent: Res<MoveIntent>,
    control: Res<PlayerControl>,
    phys: Res<PhysState>,
    mut player: ResMut<Player>,
    mut view: ResMut<BodyView>,
    mut state: ResMut<PlayerBody>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut anim: ResMut<WeaponAnim>,
    furniture: Res<crate::furniture::BodyOverride>,
) {
    let state = &mut *state;
    let own_body = state.first_person_body;
    let Some(body) = state.body.as_mut() else {
        player.body_eye = None;
        view.pivot = None;
        // No body to time the weapon: it is ready as soon as it is wanted out.
        anim.ready = control.weapon_out;
        return;
    };
    let dt = time.delta_secs();
    let m = &mut state.motion;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };

    // What the character is doing.
    let (grounded, swimming) = phys.player.as_ref().map_or((true, false), |c| (c.grounded, c.swimming));
    let stance = if swimming {
        Stance::Swim
    } else if control.sneak {
        Stance::Sneak
    } else {
        Stance::Stand
    };
    let (forward, right, fast) = if d.auto_walk > 0.0 { (1.0, 0.0, true) } else { (intent.forward, intent.right, !intent.walk) };
    let feet = player.feet;
    if dt > 0.0 {
        let moved = m.last_feet.map_or(0.0, |l| (feet[0] - l[0]).hypot(feet[1] - l[1]) / dt);
        m.speed += (moved - m.speed) * (dt / 0.15).min(1.0);
        if let Some(h) = m.last_heading {
            let dh = (player.heading - h + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            m.turn_rate += (dh / dt - m.turn_rate) * (dt / 0.1).min(1.0);
        }
    }
    m.last_feet = Some(feet);
    m.last_heading = Some(player.heading);
    let airborne_before = m.airborne;
    m.airborne = if grounded || swimming { 0.0 } else { m.airborne + dt };
    let landing = airborne_before >= AIRBORNE_LAND && m.airborne == 0.0;

    let third_view = control.third_person_view();
    if let Some(o) = furniture.0.as_ref() {
        // On furniture: the third-person body plays the furniture's clip where the
        // furniture has it. In first person the view is that body's eye and the body is
        // not drawn (seen from inside it gets in the way of looking round) [design],
        // unless the body is wanted in first person: then all but its head is, and the
        // view turns only as far as a neck does, so it never looks into the body.
        let root = Transform { translation: Vec3::from_array(o.pos), rotation: Quat::from_rotation_z(-o.heading), scale: Vec3::splat(body.scale) };
        let mut eye = pose_furniture(&mut body.third, o, dt, root, &mut transforms);
        let own_body = if own_body && !third_view {
            use std::f32::consts::{PI, TAU};
            let third = &body.third;
            // The way the body faces is its pelvis's: the clips of getting on and off
            // turn it inside the rig. The view turns with it (owner report 2026-10-10:
            // sitting down, the camera "didn't move in line with the rest of the body"),
            // and within a neck's reach of it is the player's.
            let pelvis = third.animator.skel.by_name.get("Bip01 Pelvis").copied();
            let facing = pelvis.map_or(o.heading, |b| bone_heading(third, b, o.heading)).rem_euclid(TAU);
            if let Some(last) = m.seat_facing.replace(facing) {
                player.heading = (player.heading + (facing - last + PI).rem_euclid(TAU) - PI).rem_euclid(TAU);
            }
            let (heading, within) = seated_heading(player.heading, facing, &mut m.seat_settled, dt);
            player.heading = heading;
            player.pitch = player.pitch.max(-NECK_DOWN);
            // The eyes go round the neck with the view, as a turned head's do: the open
            // neck stays behind the view whichever way it looks.
            if let (Some(e), Some(h)) = (eye.as_mut(), third.head) {
                let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
                let neck = Vec3::from((root_m * third.animator.model_space(h)).translation);
                *e = neck + Quat::from_rotation_z(bone_heading(third, h, o.heading) - heading) * (*e - neck);
            }
            within
        } else {
            m.seat_settled = false;
            m.seat_facing = None;
            false
        };
        player.body_eye = eye.map(|e| e.to_array());
        view.pivot = eye.map(|e| [o.pos[0], o.pos[1], e.z + 5.0]);
        anim.ready = false;
        show_rigs(body, &mut visibility, Shown { third: third_view || own_body, first: false, head: third_view, arms: true, weapon: true });
        return;
    }
    if body.third.furniture.take().is_some() {
        body.third.animator.cut(TAG_FURNITURE);
    }
    m.seat_settled = false;
    m.seat_facing = None;
    // The weapon: draw and holster, timed by the clips of the rig in view.
    let prefix = m.prefix;
    let timing_rig = match body.first.as_mut() {
        Some(f) if !third_view => f,
        _ => &mut body.third,
    };
    let (draw_t, away_t) = timing_rig.transitions(&mut ctx, stance, prefix);
    let want_out = control.weapon_out && stance != Stance::Swim;
    let mut events = std::mem::take(&mut m.events);
    m.hands.update(want_out, dt, draw_t, away_t, &mut events);

    // Attacks and reloads (combat): the clips of the weapon, their timing in the rig in
    // view, and whether combat started one since the last frame.
    let attack = if anim.power { m.intern("attackpower".to_string()) } else { m.intern(attack_stem(anim.attack.as_deref())) };
    let reload = anim.reload.clone().map(|r| m.intern(r));
    let attack_clip = timing_rig.clip(&mut ctx, ClipKey::Attack(stance, prefix, attack, control.aiming));
    let reload_clip = reload.and_then(|r| timing_rig.clip(&mut ctx, ClipKey::Reload(stance, prefix, r)));
    anim.timing = nvf_play::combat::clip_timing(
        attack_clip.as_ref().map(|c| (c.clip.length(), c.keys.hit, c.keys.hit.zip(c.keys.loop_end).is_some_and(|(fire, back)| fire < back))),
        reload_clip.as_ref().map(|c| c.clip.length()),
    );
    anim.ready = m.hands.phase == WeaponPhase::Drawn && m.hands.in_hand;
    let attack_now = anim.attacks != m.attacks_seen;
    let reload_now = anim.reloads != m.reloads_seen;
    let spin_now = anim.spin_ups != m.spin_ups_seen;
    m.attacks_seen = anim.attacks;
    m.reloads_seen = anim.reloads;
    m.spin_ups_seen = anim.spin_ups;

    let frame = Frame {
        dt,
        stance,
        dirs: direction_weights(forward, right),
        fast,
        female: body.female,
        prefix,
        drawn: (m.hands.phase == WeaponPhase::Drawn).then_some(prefix),
        hands: m.hands,
        airborne: m.airborne,
        landing,
        turn_rate: m.turn_rate,
        pitch: player.pitch,
        upright: own_body && !third_view && body.first.is_some(),
        aiming: control.aiming,
        speed: m.speed,
        attack: Some(attack),
        reload,
        attack_now,
        reload_now,
        spin_now,
    };
    let root = Transform { translation: Vec3::from_array(feet), rotation: Quat::from_rotation_z(-player.heading), scale: Vec3::splat(body.scale) };
    let third_eye = pose_rig(&mut body.third, &frame, &events, root, &mut ctx, &mut transforms);
    let first_eye = match body.first.as_mut() {
        Some(f) => pose_rig(f, &frame, &events, root, &mut ctx, &mut transforms),
        None => None,
    };
    m.events = events;

    // The eye: the first-person camera node; without that rig, the head's eye, swung
    // forward about the neck by the pitch while no weapon bends the body.
    let eye = first_eye.or_else(|| {
        let mut e = third_eye?;
        if let (false, Some(h)) = (m.hands.in_hand, body.third.head) {
            let head = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation) * body.third.animator.model_space(h);
            let right = Vec3::new(player.heading.cos(), -player.heading.sin(), 0.0);
            let neck = Vec3::from(head.translation);
            e = neck + Quat::from_axis_angle(right, NECK_PITCH * player.pitch) * (e - neck);
        }
        Some(e)
    });
    player.body_eye = eye.map(|e| e.to_array());
    view.pivot = third_eye.or(eye).map(|e| [feet[0], feet[1], e.z + 5.0]);

    // The body in first person stands where its eyes are under the camera (and a little
    // behind), feet on the ground: the view is the arms rig's, the body is moved to it.
    if let (true, Some(cam), Some(head)) = (frame.upright, first_eye, third_eye) {
        let back = root.rotation * Vec3::new(0.0, OWN_BODY_BACK * body.scale, 0.0);
        if let Ok(mut t) = transforms.get_mut(body.third.root) {
            t.translation += Vec3::new(cam.x - head.x - back.x, cam.y - head.y - back.y, 0.0);
        }
    }

    // The weapon nodes' parents, and the weapon parts the clips show or hide (a part
    // no playing clip speaks for stays as it was).
    for rig in std::iter::once(&mut body.third).chain(body.first.as_mut()) {
        for p in &mut rig.weapon_parts {
            for (tag, on) in &mut p.tags {
                if let Some(v) = rig.animator.visible(tag) {
                    *on = v;
                }
            }
            let shown = p.tags.iter().all(|(_, on)| *on);
            if p.shown != Some(shown) {
                if let Ok(mut v) = visibility.get_mut(p.entity) {
                    *v = if shown { Visibility::Inherited } else { Visibility::Hidden };
                    p.shown = Some(shown);
                }
            }
        }
        if rig.attached != rig.weapon_parent {
            if let (Some(w), Some(p)) = (rig.weapon_node, rig.weapon_parent) {
                commands.entity(rig.animator.nodes[w]).insert(ChildOf(rig.animator.nodes[p]));
                debug!("player {:?} rig weapon: now on {:?}", rig.view, rig.animator.skel.names[p]);
                rig.attached = Some(p);
            }
        }
    }
    // Which rig is seen; without a first-person rig the third-person head is hidden
    // from inside. With the body in first person, the arms rig's arms and weapon stand
    // in for the body's own while they are up (weapon out, drawing or putting away).
    let has_first = body.first.is_some();
    let arms_up = m.hands.in_hand || m.hands.phase != WeaponPhase::Holstered;
    let shown = if frame.upright {
        Shown { third: true, first: true, head: false, arms: !arms_up, weapon: !m.hands.in_hand }
    } else {
        Shown { third: third_view || !has_first, first: !third_view, head: third_view || has_first, arms: true, weapon: true }
    };
    show_rigs(body, &mut visibility, shown);

    m.log_elapsed += dt;
    if m.log_elapsed >= 1.0 {
        m.log_elapsed = 0.0;
        let layers = |r: &Rig| {
            let playing = r.animator.layers.iter().filter(|l| l.ease > 0.01).map(|l| format!("{}:{}×{:.2}", l.tag, l.clip.name, l.weight)).collect::<Vec<_>>().join(" ");
            // The weapon parts the clips hold away from their place in the model.
            let parts: Vec<String> = r.animator.extra_moved().filter(|p| p.1 > 0.01 || p.2 > 0.1).map(|(n, d, turn)| format!("{n} {d:.1} units {turn:.0}°")).collect();
            if parts.is_empty() {
                playing
            } else {
                format!("{playing}; weapon parts {}", parts.join(", "))
            }
        };
        debug!(
            "player body: {stance:?} speed {:.0} weapon {:?} {} in hand {} airborne {:.2}s view {} eye z {:.1} (feet {:.1}); third [{}]; first [{}]",
            m.speed,
            m.hands.phase,
            m.prefix,
            m.hands.in_hand,
            m.airborne,
            if third_view { "third" } else { "first" },
            player.body_eye.map_or(f32::NAN, |e| e[2]),
            feet[2],
            layers(&body.third),
            body.first.as_ref().map_or(String::new(), layers),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_slots() {
        // Head, Hair, Headband, Hat, Eye Glasses, Nose Ring, Earrings, Mask, Mouth Object.
        for b in [0, 1, 9, 10, 11, 12, 13, 14, 16] {
            assert!(HEAD_SLOTS & 1 << b != 0, "bit {b}");
        }
        // Upper body, hands, weapon, Pip-Boy, backpack, necklace, choker.
        for b in [2, 3, 4, 5, 6, 7, 8, 15] {
            assert!(HEAD_SLOTS & 1 << b == 0, "bit {b}");
        }
    }

    #[test]
    fn a_seated_view_turns_as_far_as_a_neck() {
        let deg = |d: f32| d.to_radians().rem_euclid(std::f32::consts::TAU);
        let close = |a: f32, b: f32| ((a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs() < 1e-4;
        // Within reach the view is the player's, either side, across north too.
        let mut settled = false;
        for h in [350.0, 10.0, 80.0, 290.0] {
            assert_eq!(seated_heading(deg(h), deg(10.0), &mut settled, 0.016), (deg(h), true), "{h}");
        }
        assert!(settled);
        // Once there, the mouse takes it no further than the limit.
        let (h, within) = seated_heading(deg(150.0), deg(10.0), &mut settled, 0.016);
        assert!(close(h, deg(90.0)) && within, "{h}");
        let (h, within) = seated_heading(deg(200.0), deg(10.0), &mut settled, 0.016);
        assert!(close(h, deg(290.0)) && within, "{h}");
        // Sitting down facing the chair: the view comes round at the return rate, and
        // is out of reach until it has.
        let mut settled = false;
        let (h, within) = seated_heading(deg(170.0), deg(10.0), &mut settled, 0.1);
        assert!(close(h, deg(170.0) - 0.1 * NECK_RETURN_RATE) && !within && !settled, "{h}");
        let mut h = deg(170.0);
        let mut frames = 0;
        while !seated_heading(h, deg(10.0), &mut settled, 0.016).1 {
            h = seated_heading(h, deg(10.0), &mut settled, 0.016).0;
            frames += 1;
            assert!(frames < 100);
        }
        assert!(settled && frames > 10);
    }

    #[test]
    fn default_eye_is_in_front_of_and_above_the_head() {
        let skel = PreparedSkeleton { bind_world: vec![Affine3A::from_translation(Vec3::new(0.0, 0.0, 110.0))], ..Default::default() };
        assert_eq!(default_eye(&skel, 0), Vec3::new(0.0, 9.0, 7.0));
    }

    #[test]
    fn an_attack_clip_goes_round_its_repeated_part() {
        // 2hhattackspin.kf: repeated from `Fire` at 0.967 s to `Loop` at 1.633 s.
        let (start, end) = (0.967, 1.633);
        assert_eq!(loop_time(0.5, start, end), 0.5);
        assert_eq!(loop_time(1.6, start, end), 1.6);
        assert!((loop_time(1.633, start, end) - start).abs() < 1e-5);
        assert!((loop_time(1.733, start, end) - (start + 0.1)).abs() < 1e-5);
        // A long frame, or a shot during the way out: back inside the repeated part.
        let t = loop_time(2.4, start, end);
        assert!((start..end).contains(&t), "{t}");
        // No repeated part to go round.
        assert_eq!(loop_time(3.0, 2.5, 2.5), 3.0);
    }
}
