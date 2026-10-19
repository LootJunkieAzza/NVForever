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
//! once each time combat counts an attack or a reload up (a round-by-round reload: the
//! opening clip, then the reload clip again for each round).
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
//! go round the neck with it. While an idle drives the first-person view (the opening's
//! wake-up) the body is not drawn: the arms rig plays the idle alone.
//!
//! **The shadow** [engineering choice; the original casts no sun shadows from the world]:
//! the third-person body casts the player's shadow in every view, head, arms and weapon
//! included, and the arms rig and its weapon cast none on the world. What of the body the
//! camera is not to see is drawn on [`SHADOW_LAYER`], which the sun casts from and no
//! camera sees, instead of being hidden (a hidden entity casts no shadow). The arms rig
//! is on [`ARMS_LAYER`], which the sun does not cast from. The unseen body stands where
//! the arms rig is and would shade it, so the arms rig takes its sun shadows from a light
//! of its own ([`ArmsShade`], `NifSurface::shade_light`) that casts from the world's
//! layer and the arms': buildings, what is seen of the body and the arms themselves shade
//! the arms, as before the unseen body cast anything.
//!
//! **Lying on a bed** ([`lying_view`]; owner requests 2026-10-10): in first person the
//! view is the lying body's, whether or not the body is drawn. It keeps its place in the
//! body's own frame while the body lies down and gets up, so the picture leans with the
//! head and ends on its side; the mouse turns the head in that frame, as far as a neck
//! does. In third person the camera's pivot is lifted clear of the bed.

use crate::actors::{part_uses, ActorWorld};
use crate::idles::{IdleSlot, PlayerIdles};
use crate::sky::{Sun, ARMS_LAYER, SHADOW_LAYER};
use crate::world::WorldRoot;
use bevy::camera::visibility::RenderLayers;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_actor::first_person::{self, is_first_person_shape, shape_shows, Show};
use fnv_actor::player_clips::{
    aim_clips, aim_variant, aim_weights, attack_clips, attack_stem, block_clips, blocks, direction_weights, equip_clips, holster_clips, idle_clips, jump_clips,
    limping_idle_clips, limping_move_clips, limping_turn_clips, move_clips, reload_clips, reload_open_clips, turn_clips, Aim, ClipKeys, Dir, HandEvent, Stance, Transition, WeaponHands, WeaponPhase, UNARMED,
};
use fnv_actor::weapon::AnimType;
use fnv_actor::Appearance;
use fnv_sim::FormId;
use nvf_assets::actor::{rigid_local, spawn_actor, ActorCtx, ActorParts, PartUse, PreparedSkeleton};
use nvf_assets::anim_blend::{Animator, ExtraNode, Mode, DEFAULT_FADE};
use nvf_assets::material_anim::StoredLook;
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::NifMaterial;
use nvf_play::combat::WeaponAnim;
use nvf_play::control::{BodyView, PlayerControl};
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player, MAX_PITCH};
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
/// The Pip-Boy coming up (`pipboy_arm.rs`).
const TAG_PIPBOY: u32 = 90;
/// The Pip-Boy clip fades in over its first 8 frames [observed: its `Blend: 8` key;
/// hypothesis: frames at 30 a second, as for idles] and out over its last 8 going down
/// [design].
const PIPBOY_BLEND: f32 = 8.0 / 30.0;

/// The field of view the first-person arms are drawn with while the Pip-Boy is up,
/// degrees, as the game's field-of-view setting counts them (across a 4:3 picture)
/// [documented: `Fallout_default.ini` `fPipboy1stPersonFOV=47.0`, and the owner's
/// `Fallout.ini` the same].
pub const PIPBOY_FOV: f32 = 47.0;

/// The field of view the first-person arms and weapon are drawn with otherwise, degrees
/// as [`PIPBOY_FOV`] [documented: `Fallout_default.ini` `fDefault1stPersonFOV=55`,
/// docs/design/layer3.md; hypothesis: that it is the view of the arms' own camera, so
/// they are drawn bigger than through the world's 75: our 9mm pistol was drawn smaller
/// than the game's, with less of its detail and a coarser mip (owner's `Weapon Quality`
/// screenshots, playtest list 5 item 18)].
pub const FIRST_PERSON_FOV: f32 = 55.0;

/// How much of the Pip-Boy clip shows at `t` seconds into it: up over its first
/// [`PIPBOY_BLEND`], down over its last (smoothstep).
pub fn pipboy_ease(t: f32) -> f32 {
    let e = (t.min(nvf_play::pipboy::CLIP_SECONDS - t) / PIPBOY_BLEND).clamp(0.0, 1.0);
    e * e * (3.0 - 2.0 * e)
}

/// How much bigger the arms are drawn so that they look as through a view of `arms_fov`
/// (the game's degrees across 4:3) inside the view's own vertical field of view
/// `view_vfov` (radians): the ratio of the two views' half-widths.
pub fn arms_magnification(view_vfov: f32, arms_fov: f32) -> f32 {
    let arms_vfov_half = ((arms_fov.to_radians() / 2.0).tan() * 3.0 / 4.0).atan();
    (view_vfov / 2.0).tan() / arms_vfov_half.tan()
}
/// Idles (`idles.rs`): one role per idle started, so an idle keeps its layer while the
/// next one fades in over it.
const TAG_SPECIAL: u32 = 1000;

/// Biped slots of headgear hidden in first person when the third-person body has to stand
/// in for the first-person rig: Head, Hair, Headband, Hat, Eye Glasses, Nose Ring,
/// Earrings, Mask, Mouth Object [documented: xEdit biped flag names, bits 0, 1, 9–14,
/// 16; engineering choice: which ones block the view].
const HEAD_SLOTS: u32 = 1 | 1 << 1 | 0x7E << 8 | 1 << 16;

/// Time in the air before the jump loop plays, and the shortest fall that plays a
/// landing, seconds [engineering choice: steps and slopes leave the ground briefly].
const AIRBORNE_LOOP: f32 = 0.15;
const AIRBORNE_LAND: f32 = 0.35;
/// The jump loop (falling) starts only with the ground farther below the feet than this,
/// or out of the controller's reach, game units: running down a slope or sliding over
/// rocks leaves the ground for a few steps without a fall [engineering choice: the
/// controller's step height, the drop it walks down without leaving the ground; the
/// original's rule is not documented (physics.md §6.4.2)].
const FALL_GAP: f32 = 24.0;
/// Turning on the spot faster than this (radians per second) plays a turn clip
/// [engineering choice].
const TURN_RATE: f32 = 1.0;
/// Without a first-person rig: share of the view's pitch the neck takes when nothing else
/// bends the body, so the eye clears the chest looking down [engineering choice].
const NECK_PITCH: f32 = 0.6;
/// Blend-in of an attack clip, seconds [engineering choice: a shot should show at once].
const ATTACK_FADE: f32 = 0.03;
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
/// Lying on a bed: how far the third-person camera's pivot is lifted above the head, in
/// game units, so the camera's way round the body is clear of the mattress and the bed's
/// frame [engineering choice].
const LYING_PIVOT_UP: f32 = 40.0;
/// Lying on a bed: how far below level the first-person view looks at most, radians
/// [engineering choice: 10 degrees; the head is on the pillow, and lower is the inside of
/// the mattress].
const LYING_DOWN: f32 = 10.0 * std::f32::consts::PI / 180.0;
/// An attack clip with a `Loop` key goes round its repeated part while the shots come no
/// further apart than this, seconds [engineering choice: the clips that have the key are
/// the `attackspin` ones, of weapons that fire many times a second; animation.md §9.8].
const LOOP_HOLD: f32 = 0.25;

/// The share of the sun's light the arms rig's shade light carries. The materials need
/// some to work the sun's part of the light out (`nif_material.wgsl`), and the world gets
/// that much more sun with it [engineering choice: a thousandth, a quarter of one step of
/// an 8-bit picture at full brightness].
const ARMS_SHADE_SHARE: f32 = 1.0 / 1024.0;

/// How far from the camera the [`ArmsShade`] light's cascade reaches (game units): the
/// arms rig and its weapon lie within about 100 units of the eye [engineering choice].
const ARMS_SHADE_REACH: f32 = 256.0;

/// The light the arms rig's sun shadows are read from (module docs, "The shadow"): a
/// child of the sun, so it points the same way, with the sun's colour and
/// [`ARMS_SHADE_SHARE`] of its strength. It casts from the world's layer and the arms',
/// and not from the sun's shadow-only layer. Its one shadow cascade reaches only
/// [`ARMS_SHADE_REACH`] from the camera, around the arms: a cascade takes every caster
/// between it and the sun, so what shades the arms still does, while the pass draws only
/// those (before 2026-10-12 it was the sun's first cascade, about 190 casters at
/// Goodsprings against about 30 now; the arms' shadows are finer for it).
#[derive(Component)]
struct ArmsShade;

/// A copy of material `m` whose sun shadows are the [`ArmsShade`] light's (`m` itself is
/// shared with other actors and weapons).
fn arms_material(materials: &mut Assets<NifMaterial>, m: &Handle<NifMaterial>) -> Handle<NifMaterial> {
    let Some(mut copy) = materials.get(m).cloned() else { return m.clone() };
    copy.extension.shade_light = true;
    materials.add(copy)
}

/// Keep the [`ArmsShade`] light: there while the arms rig is drawn and the sun casts
/// shadows, so that nothing pays for it otherwise.
#[allow(clippy::type_complexity)]
fn shade_light(
    mut commands: Commands,
    state: Res<PlayerBody>,
    sun: Query<(Entity, &DirectionalLight), (With<Sun>, Without<ArmsShade>)>,
    mut shade: Query<(&mut DirectionalLight, &mut Visibility), With<ArmsShade>>,
) {
    // A sun casting no shadows has no shade light either.
    let Ok((sun, sun_light)) = sun.single() else { return };
    let arms_drawn = state.body.as_ref().and_then(|b| b.first.as_ref()).is_some_and(|f| f.shown == Some(true));
    let on = arms_drawn && sun_light.shadow_maps_enabled;
    let illuminance = sun_light.illuminance * ARMS_SHADE_SHARE;
    if let Ok((mut light, mut v)) = shade.single_mut() {
        if v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden }) {
            debug!("arms shade light {}", if on { "on" } else { "off" });
        }
        if on && (light.color != sun_light.color || light.illuminance != illuminance) {
            light.color = sun_light.color;
            light.illuminance = illuminance;
        }
    } else if on {
        let cascades = CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 5.0,
            first_cascade_far_bound: ARMS_SHADE_REACH,
            maximum_distance: ARMS_SHADE_REACH,
            overlap_proportion: 0.0,
        }
        .build();
        commands.spawn((
            Name::new("Arms shade light"),
            ArmsShade,
            DirectionalLight { color: sun_light.color, illuminance, shadow_maps_enabled: true, ..default() },
            cascades,
            RenderLayers::from_layers(&[0, ARMS_LAYER]),
            Transform::IDENTITY,
            ChildOf(sun),
        ));
    }
}

/// A clip choice, for the clip caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClipKey {
    /// Posture; limping (a leg crippled, standing).
    Idle(Stance, bool),
    /// Posture, female, running, direction, the weapon drawn; limping.
    Move(Stance, bool, bool, Dir, Option<&'static str>, bool),
    Aim(Stance, &'static str, Aim, bool),
    Block(Stance, &'static str),
    Holster(Stance, &'static str),
    Equip(Stance, &'static str, bool),
    Jump(Stance, Option<&'static str>, Option<Dir>, bool),
    /// Posture, the weapon drawn, to the left; limping.
    Turn(Stance, Option<&'static str>, bool, bool),
    /// Weapon prefix, attack clip stem (`attackright`), down the sights.
    Attack(Stance, &'static str, &'static str, bool),
    /// Weapon prefix, WEAP reload animation (`ReloadL`).
    Reload(Stance, &'static str, &'static str),
    /// The opening clip of a round-by-round reload (`ReloadX` → `1hpreloadxstart.kf`).
    ReloadOpen(Stance, &'static str, &'static str),
    /// The Pip-Boy coming up (first person); a woman's.
    PipBoy(bool),
}

impl ClipKey {
    /// A crippled leg's choice (standing).
    fn limping(self) -> bool {
        matches!(self, ClipKey::Idle(Stance::Stand, true) | ClipKey::Move(Stance::Stand, .., true) | ClipKey::Turn(Stance::Stand, .., true))
    }

    fn candidates(self) -> Vec<String> {
        match self {
            ClipKey::Idle(Stance::Stand, true) => limping_idle_clips(),
            ClipKey::Idle(s, _) => idle_clips(s),
            ClipKey::Move(Stance::Stand, female, fast, d, w, true) => limping_move_clips(female, fast, d, w),
            ClipKey::Move(s, female, fast, d, w, _) => move_clips(s, female, fast, d, w),
            ClipKey::Aim(s, p, a, sights) => aim_clips(s, p, a, sights),
            ClipKey::Block(s, p) => block_clips(s, p),
            ClipKey::Holster(s, p) => holster_clips(s, p),
            ClipKey::Equip(s, p, e) => equip_clips(s, p, e),
            ClipKey::Jump(s, w, d, land) => jump_clips(s, w, d, land),
            ClipKey::Turn(Stance::Stand, w, left, true) => limping_turn_clips(w, left),
            ClipKey::Turn(s, w, left, _) => turn_clips(s, w, left),
            ClipKey::Attack(s, p, a, sights) => attack_clips(s, p, a, sights),
            ClipKey::Reload(s, p, r) => reload_clips(s, p, r),
            ClipKey::ReloadOpen(s, p, r) => reload_open_clips(s, p, r),
            // [observed: `_1stperson\locomotion\female\pipboyfemale.kf` beside the
            // male one]
            ClipKey::PipBoy(female) => {
                let own = if female { vec!["locomotion\\female\\pipboyfemale.kf".to_string()] } else { Vec::new() };
                own.into_iter().chain(["locomotion\\male\\pipboy.kf".to_string()]).collect()
            }
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
    /// First person: the nodes above the root that draw the rig bigger about the eye
    /// ([`place_lens`]).
    lens: Option<[Entity; 2]>,
    /// The skeleton's folder (`meshes\characters\_male\`), and its path.
    folder: String,
    skeleton: String,
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
    /// The way from the `Weapon` node down to the weapon model's `ProjectileNode`, when
    /// it has one: each node's place in the animator's extra nodes (a named node; the
    /// clips may move it) and its transform in the model [hypothesis: the node marks
    /// the muzzle; animation.md §9.9].
    muzzle: Option<Vec<(Option<usize>, Transform)>>,
    /// That node's entity (muzzle flashes are spawned under it; `shot_fx.rs`).
    muzzle_entity: Option<Entity>,
    /// The way down to the weapon model's `ShellCasingNode`, as `muzzle`.
    casing: Option<Vec<(Option<usize>, Transform)>>,
    /// Third person: the entities drawn for the body's parts.
    drawn: Vec<Drawn>,
    /// First person: whether the rig is shown (last applied).
    shown: Option<bool>,
    /// Third person: what of the rig the camera sees (last applied).
    seen: Option<Seen>,
    /// The furniture clip it plays (`BodyClip::serial`), while it plays one.
    furniture: Option<u32>,
    /// Seconds since the last attack started, and the repeated part of its clip, if it
    /// has one: from and to, seconds into the clip.
    since_attack: f32,
    attack_loop: Option<(f32, f32)>,
}

/// The node of a weapon model that shots are drawn from, by either of its names
/// (animation.md §9.9).
const MUZZLE_NODES: [&str; 2] = ["ProjectileNode", "##ProjectileNode"];

fn affine(t: Transform) -> Affine3A {
    Affine3A::from_scale_rotation_translation(t.scale, t.rotation, t.translation)
}

/// An entity drawn for a part of the third-person body.
struct Drawn {
    entity: Entity,
    /// Of the head (head, hair, headgear): not seen when the body stands in for the
    /// first-person rig.
    head: bool,
    /// One the first-person rig has its own of (arms, hands, gloves, the Pip-Boy): not
    /// seen while that rig's arms are up in front of the body.
    arm: bool,
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
    /// Where the weapon's muzzle is in the last pose with the rig at `root` (game
    /// space): under the node the `Weapon` node hangs from, down the weapon model to its
    /// `ProjectileNode`, each node where the clips have it.
    fn muzzle_at(&self, root: Transform) -> Option<Vec3> {
        Some(self.weapon_node_at(root, self.muzzle.as_ref()?)?.translation.into())
    }

    /// Where a node of the weapon model is in the last pose with the rig at `root`
    /// (game space), given the way down to it from the `Weapon` node.
    fn weapon_node_at(&self, root: Transform, path: &[(Option<usize>, Transform)]) -> Option<Affine3A> {
        let a = &self.animator;
        let mut m = affine(root) * a.model_space(self.weapon_parent?) * affine(a.locals[self.weapon_node?]);
        for (extra, bind) in path {
            m *= affine(extra.and_then(|k| a.extra_local(k)).unwrap_or(*bind));
        }
        Some(m)
    }

    /// Where the weapon's `ShellCasingNode` is and the way a casing leaves it: the
    /// node's own +z [hypothesis: animation.md §9.9].
    fn casing_at(&self, root: Transform) -> Option<(Vec3, Vec3)> {
        let m = self.weapon_node_at(root, self.casing.as_ref()?)?;
        Some((m.translation.into(), m.transform_vector3(Vec3::Z).normalize_or_zero()))
    }

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
            Some(c) if key.limping() => info!("player {:?} rig: limping {key:?} plays {}", self.view, c.path),
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
    /// The look chosen in the character creator the body was built with (`None`: the
    /// player record's).
    look: Option<fnv_actor::Look>,
    /// AgeRace's and SexChange's change the body was built with (`ActorWorld::change`,
    /// `crate::appearance`).
    change: fnv_actor::Change,
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
    /// Playing the jump loop: in the air long enough, with the ground far below
    /// ([`FALL_GAP`]); kept until the character is on the ground again.
    falling: bool,
    /// Falls (jump loops started) and landings played, for the summary at exit.
    falls: u32,
    landings: u32,
    last_heading: Option<f32>,
    turn_rate: f32,
    /// The view an idle gave the first-person camera last frame (`Player::body_view`).
    idle_view: Option<Quat>,
    /// Seconds until the line saying where the character stands after an idle let it
    /// go.
    after_idle: Option<f32>,
    /// Once-a-second status (debug log).
    log_elapsed: f32,
    /// On furniture with the body in first person: the view has come within the neck's
    /// reach of the way the body faces, and is held there.
    seat_settled: bool,
    /// The same: the way the body faced last frame, to turn the view with it.
    seat_facing: Option<f32>,
    /// Lying on a bed in first person: where the view looks in the body's own frame.
    lying: Option<LyingView>,
    /// The attack, reload, reload round and spin-up counts last acted on (`WeaponAnim`).
    attacks_seen: u32,
    reloads_seen: u32,
    reload_rounds_seen: u32,
    jams_seen: u32,
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
            falling: false,
            falls: 0,
            landings: 0,
            last_heading: None,
            turn_rate: 0.0,
            idle_view: None,
            after_idle: None,
            log_elapsed: 0.0,
            seat_settled: false,
            seat_facing: None,
            lying: None,
            attacks_seen: 0,
            reloads_seen: 0,
            reload_rounds_seen: 0,
            jams_seen: 0,
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
    /// The muzzle node of the weapon in hand in the rig in view, this frame.
    muzzle_entity: Option<Entity>,
    /// The third-person head's eye this frame, game space, while the view is from
    /// outside; `None` in first person or without a body.
    head_eye: Option<[f32; 3]>,
    motion: Motion,
    /// Why the body could not be built (reported once).
    failed: Option<String>,
}

impl PlayerBody {
    /// The landing clip after a jump or fall is playing (GetAnimAction 12, `anim_state`).
    pub fn landing(&self) -> bool {
        self.body.as_ref().is_some_and(|b| b.third.animator.layers.iter().any(|l| l.tag == TAG_LAND && !l.finished()))
    }

    /// The rigs built, root entity and animator (the body's, then the arms rig's): their
    /// clips drive the materials of their shapes too (`actor_materials.rs`).
    pub fn animators(&self) -> Vec<(Entity, &Animator)> {
        let Some(b) = self.body.as_ref() else { return Vec::new() };
        std::iter::once((b.third.root, &b.third.animator)).chain(b.first.as_ref().map(|f| (f.root, &f.animator))).collect()
    }

    /// Whether the first-person rig is drawn (`pipboy_arm.rs`).
    pub fn first_person_drawn(&self) -> bool {
        self.body.as_ref().and_then(|b| b.first.as_ref()).is_some_and(|f| f.shown == Some(true))
    }

    /// The root entities of the rigs built (the body's, then the arms rig's).
    pub fn roots(&self) -> Vec<Entity> {
        let Some(b) = self.body.as_ref() else { return Vec::new() };
        std::iter::once(b.third.root).chain(b.first.as_ref().map(|f| f.root)).collect()
    }

    /// Whether the body drawn is a woman's; `None` without a body.
    pub fn female(&self) -> Option<bool> {
        self.body.as_ref().map(|b| b.female)
    }

    /// The third-person head's eye this frame, game space, while the view is from
    /// outside (the flashlight follows it, `flashlight.rs`); `None` in first person or
    /// without a body.
    pub fn head_eye(&self) -> Option<[f32; 3]> {
        self.head_eye
    }

    /// The entity of the muzzle node (`ProjectileNode`) of the weapon in hand, in the
    /// rig in view; `None` without a body, a weapon in hand or such a node.
    pub fn muzzle_entity(&self) -> Option<Entity> {
        self.muzzle_entity
    }

    /// The third-person rig as an actor's rig of the player's reference, for its ragdoll
    /// (`death_cam.rs`): its root entity and the rig.
    pub fn third_rig(&self) -> Option<(Entity, crate::limbs::ActorRig)> {
        let r = &self.body.as_ref()?.third;
        let rig = crate::limbs::ActorRig {
            reference: FormId::PLAYER_REF.0,
            base: FormId::PLAYER_BASE.0,
            skeleton: r.skeleton.clone(),
            skel: r.animator.skel.clone(),
            nodes: r.animator.nodes.clone(),
        };
        Some((r.root, rig))
    }

    /// The clips heard (`anim_sounds.rs`): the animator of the rig in view (the
    /// first-person rig while it is shown, else the body), whether that is the
    /// first-person rig, and the armour worn.
    pub fn heard(&self) -> Option<(&Animator, bool, &[FormId])> {
        let b = self.body.as_ref()?;
        match b.first.as_ref().filter(|f| f.shown == Some(true)) {
            Some(f) => Some((&f.animator, true, &b.worn)),
            None => Some((&b.third.animator, false, &b.worn)),
        }
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
            .add_systems(Update, (crate::furniture::drive_player, ensure_body, drive_body, shade_light).chain().in_set(nvf_play::player::BodySet))
            .add_systems(Last, report);
    }
}

/// The body's falls at exit (smoke tests read it).
fn report(mut exit: MessageReader<AppExit>, body: Res<PlayerBody>) {
    if exit.read().next().is_some() {
        let m = &body.motion;
        info!("player body summary: {} falls (the jump loop), {} landings", m.falls, m.landings);
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
    (death, knock): (Res<crate::death_cam::DeathCam>, Res<crate::player_knockdown::PlayerKnockdown>),
    (script, real, mut last_look): (Option<Res<nvf_play::script::ScriptState>>, Res<Time<Real>>, Local<f32>),
) {
    let (Some(world), Ok(world_root)) = (world, root.single()) else { return };
    // The dead or knocked-down player's body lies as its ragdoll left it: it is not
    // rebuilt.
    if state.failed.is_some() || death.down() || knock.ragdolled() {
        return;
    }
    let (worn, weapon) = equipped(&sim, &world);
    // The look chosen in the character creator (docs/spec-notes/character-creator.md §5).
    let look = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.player.look.as_ref()).map(nvf_play::creator::actor_look);
    let state = &mut *state;
    let mut rebuilt = false;
    // A new look rebuilds the body, at most every [`LOOK_REBUILD`] seconds while a
    // slider is dragged [design].
    // AgeRace and SexChange before the creator gave a look (`crate::appearance`).
    let change = world.change(FormId::PLAYER_REF.0);
    let look_changed = state.body.as_ref().is_some_and(|b| b.look != look || b.change != change);
    let look_due = look_changed && real.elapsed_secs() - *last_look >= LOOK_REBUILD;
    if state.body.as_ref().is_none_or(|b| b.worn != worn) || look_due {
        if look_due {
            *last_look = real.elapsed_secs();
            debug!("player body: the look changed; rebuilt");
        }
        let old = state.body.take();
        if let Some(old) = &old {
            commands.entity(old.third.root).despawn();
            if let Some(f) = &old.first {
                commands.entity(f.lens.map_or(f.root, |[outer, _]| outer)).despawn();
            }
        }
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        match build(&mut commands, world_root, &world, &mut ctx, &worn, look.as_ref()) {
            Ok(mut b) => {
                // The new body goes on with the old one's clips and pose: no T-pose while
                // its clips would fade in [owner 2026-10-17: "a few frames where the model
                // is t posing"].
                if let Some(old) = &old {
                    b.third.animator.continue_from(&old.third.animator);
                    info!("player body: rebuilt; it goes on with the old body's {} clips and pose", b.third.animator.layers.len());
                    if let (Some(n), Some(o)) = (b.first.as_mut(), old.first.as_ref()) {
                        n.animator.continue_from(&o.animator);
                    }
                }
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
        rig.muzzle = None;
        rig.muzzle_entity = None;
        rig.casing = None;
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
                let mut extra: Vec<ExtraNode> = Vec::new();
                // Each node's place among the extra nodes, if it has a name.
                let extra_of: Vec<Option<usize>> = pm
                    .nodes
                    .iter()
                    .zip(&part_nodes)
                    .map(|(n, e)| {
                        let name = n.name.clone()?;
                        extra.push(ExtraNode { name, entity: *e, bind: local(&n.local) });
                        Some(extra.len() - 1)
                    })
                    .collect();
                let muzzle = nvf_assets::model::node_path(&pm.nodes, &MUZZLE_NODES);
                rig.muzzle_entity = muzzle.as_ref().and_then(|path| path.last()).map(|&i| part_nodes[i]);
                rig.muzzle = muzzle.map(|path| path.into_iter().map(|i| (extra_of[i], local(&pm.nodes[i].local))).collect());
                rig.casing = nvf_assets::model::node_path(&pm.nodes, &["ShellCasingNode", "##ShellCasingNode"]).map(|path| path.into_iter().map(|i| (extra_of[i], local(&pm.nodes[i].local))).collect());
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
                    // The shadow on the world is the third-person body's, weapon
                    // included: the arms rig's weapon casts none there, and takes none
                    // from that body.
                    let e = match rig.view {
                        View::Third => commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), place, visibility)).id(),
                        View::First => {
                            let material = arms_material(&mut models.materials, &s.material);
                            commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(material), place, visibility, RenderLayers::layer(ARMS_LAYER))).id()
                        }
                    };
                    commands.entity(parent).add_child(e);
                    // Named, so the clips can drive its glow (`##Bulbs:0` in `1hpaim.kf`;
                    // actor_materials.rs).
                    if let Some(name) = &s.name {
                        commands.entity(e).insert((nvf_assets::actor::ShapeName(name.to_ascii_lowercase()), StoredLook(s.look)));
                    }
                    if let (None, Some(name)) = (bone, &s.name) {
                        extra.push(ExtraNode { name: name.clone(), entity: e, bind: place });
                    }
                    rig.weapon_parts.push(WeaponPart { entity: e, tags, on_bone: bone.is_some(), shown: Some(shown) });
                }
                rig.weapon_model = Some(holder);
                rig.seen = None;
                info!(
                    "player weapon model ({:?} person): {path}, {} nodes, {} named parts for the clips, {}",
                    rig.view,
                    pm.nodes.len(),
                    extra.len(),
                    match &rig.muzzle {
                        Some(p) => format!("{} {} node(s) down: shots are drawn from it", MUZZLE_NODES[0], p.len()),
                        None => format!("no {}: shots are drawn from the eye", MUZZLE_NODES[0]),
                    }
                );
                rig.animator.set_extra(extra);
            }
            None => warn!("player weapon {path}: model not loaded"),
        }
    }
}

/// The least time between two body rebuilds for a changed look, seconds.
const LOOK_REBUILD: f32 = 0.1;

fn build(commands: &mut Commands, parent: Entity, world: &ActorWorld, ctx: &mut ActorCtx, worn: &[FormId], look: Option<&fnv_actor::Look>) -> Result<Body, String> {
    let a = world.resolve_as(FormId::PLAYER_BASE.0, FormId::PLAYER_REF.0, Some(worn), look)?;
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
    Ok(Body { worn: worn.to_vec(), look: look.cloned(), change: world.change(FormId::PLAYER_REF.0), female: a.female, scale, third, first })
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
    let mut uses: Vec<PartUse> = uses.into_iter().map(|(_, u)| u).collect();
    if view == View::First {
        // The arms take no shadow from the unseen body they are in the middle of.
        for u in &mut uses {
            u.materials = (u.part.shapes.iter().enumerate()).map(|(i, s)| arms_material(&mut ctx.models.materials, u.materials.get(i).unwrap_or(&s.material))).collect();
        }
    }
    // The first-person rig hangs under two nodes that can draw it bigger about the eye
    // ([`place_lens`]).
    let lens = (view == View::First).then(|| {
        let outer = commands.spawn((Name::new("Arms lens"), Transform::IDENTITY, Visibility::Inherited, ChildOf(parent))).id();
        let inner = commands.spawn((Transform::IDENTITY, Visibility::Inherited, ChildOf(outer))).id();
        [outer, inner]
    });
    let parent = lens.map_or(parent, |[_, inner]| inner);
    let spawned = spawn_actor(commands, parent, Transform::from_scale(Vec3::splat(a.race_height as f32)), &skel, &uses);
    commands.entity(spawned.root).insert(Name::new(format!("Player body ({view:?} person)")));
    let drawn = (spawned.drawn.iter().zip(&spawned.part_of).zip(&spawned.shape_of))
        .map(|((&entity, &p), &s)| Drawn { entity, head: hide[p], arm: shape_shows(arms[p], uses[p].part.shapes[s].name.as_deref()) })
        .collect();
    if view == View::First {
        // The arms rig casts no shadow on the world: the third-person body casts the
        // player's.
        for e in &spawned.drawn {
            commands.entity(*e).insert(RenderLayers::layer(ARMS_LAYER));
        }
        for ((&e, &p), &s) in spawned.drawn.iter().zip(&spawned.part_of).zip(&spawned.shape_of) {
            let shape = &uses[p].part.shapes[s];
            if let Some(part) = shape.name.as_deref().and_then(crate::pipboy_arm::part) {
                // The CPU copy: the pointer is followed onto the posed shape
                // (`pipboy_arm::follow_pointer`).
                commands.entity(e).insert((part, crate::pipboy_arm::PartMesh(shape.cpu.clone())));
            }
        }
    }
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
        lens,
        folder,
        skeleton: a.skeleton.clone(),
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
        muzzle: None,
        muzzle_entity: None,
        casing: None,
        drawn,
        shown: None,
        seen: None,
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
    /// A leg is crippled: standing, the body limps (`locomotion\hurt\` clips).
    limping: bool,
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
    /// The weapon's jam animation (`JamA`), and whether a shot jammed it since the last
    /// frame (combat, the owner's playtest list 5 item 9).
    jam: Option<&'static str>,
    jam_now: bool,
    attack_now: bool,
    reload_now: bool,
    /// A round-by-round reload starts on a round: the reload clip plays from its start.
    reload_round_now: bool,
    /// The weapon starts to spin up this frame: its attack clip plays from the start.
    spin_now: bool,
}

/// Where a rig stands in the frame and what it plays beside its own clips.
struct Posed<'a> {
    root: Transform,
    /// The pitch the first-person rig turns by.
    look_pitch: f32,
    /// The idles the rig plays (`idles.rs`).
    special: &'a IdleSlot<Arc<Clip>>,
    /// The Pip-Boy shows: seconds into its clip (first person).
    pipboy: Option<f32>,
}

/// Pose `rig` for the frame, apply the weapon events, place it at its root; returns the
/// eye (game space): the camera node in first person, between the eyes in third. With
/// it, in first person while an idle drives the camera node, the node's rotation (game
/// space): the view the animation gives.
///
/// Once an idle covers the rig (it has faded in, or fades in over the idle before it)
/// the rig's other clips are not played: a special idle takes the place of what the body
/// would do by itself [hypothesis: the clips of the opening share track priorities with
/// the standing idle, so both cannot play; idles.md §4].
fn pose_rig(rig: &mut Rig, f: &Frame, events: &[HandEvent], posed: Posed, ctx: &mut ActorCtx, transforms: &mut Query<&mut Transform>) -> Option<(Vec3, Option<Quat>)> {
    let Posed { root, look_pitch, special, pipboy } = posed;
    let pipboy = pipboy.filter(|_| rig.view == View::First).and_then(|t| Some((rig.clip(ctx, ClipKey::PipBoy(f.female))?, t)));
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
    if let Some(c) = rig.clip(ctx, ClipKey::Idle(f.stance, f.limping)) {
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
            if let Some(c) = rig.clip(ctx, ClipKey::Move(f.stance, f.female, f.fast, dir, f.drawn, f.limping)) {
                expected_speed += weight * c.speed.unwrap_or(0.0);
                want.push((c, TAG_MOVE + i as u32, weight, Mode::Synced));
            }
        }
    } else if f.turn_rate.abs() > TURN_RATE {
        // Heading grows clockwise: a negative rate turns left.
        if let Some(c) = rig.clip(ctx, ClipKey::Turn(f.stance, f.drawn, f.turn_rate < 0.0, f.limping)) {
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
    // A reload: its clip, or the opening clip of one that loads round by round; then the
    // reload clip from its start for each round, running on past its `Hit` key to close
    // the weapon after the last (`fnv_combat::attack`).
    if f.hands.in_hand && (f.reload_now || f.reload_round_now) {
        let open = f.reload_now && !f.reload_round_now;
        let key = |r| if open { ClipKey::ReloadOpen(f.stance, f.prefix, r) } else { ClipKey::Reload(f.stance, f.prefix, r) };
        let found = f.reload.and_then(|r| rig.clip(ctx, key(r)).or_else(|| rig.clip(ctx, ClipKey::Reload(f.stance, f.prefix, r))));
        if let Some(c) = found {
            // A round follows straight on from the opening clip or the last round: no
            // fade back through the held pose [hypothesis: the clips are made to chain,
            // the opening's end and the round's `Hit` close to its start pose].
            let fade = if f.reload_round_now { 0.0 } else { c.fade() };
            rig.animator.restart(&c.clip, TAG_RELOAD, 1.0, Mode::Once, fade);
            debug!("player {:?} rig: {} moves the weapon's {:?}", rig.view, c.path, rig.animator.extra_driven(&c.clip));
        }
    }
    // A jam: the weapon's jam clip (`1hpjama.kf`), played once on the reload layer as a
    // reload is [observed: the clips sit beside the reload clips in both rigs' folders].
    if f.hands.in_hand && f.jam_now {
        if let Some(c) = f.jam.and_then(|j| rig.clip(ctx, ClipKey::Reload(f.stance, f.prefix, j))) {
            rig.animator.restart(&c.clip, TAG_RELOAD, 1.0, Mode::Once, c.fade());
            debug!("player {:?} rig: jam clip {}", rig.view, c.path);
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
    let covered = special.covers();
    for (c, tag, w, mode) in want.iter().filter(|_| !covered) {
        let fade = if *tag == TAG_HOLSTER { 0.0 } else { c.fade() };
        a.want(&c.clip, *tag, *w, *mode, fade);
    }
    // Clips that play once to their end once started: landing, attack, reload.
    for (tag, w) in [(TAG_LAND, 1.0), (TAG_ATTACK, aim_w[0].1), (TAG_ATTACK + 1, aim_w[1].1), (TAG_ATTACK + 2, aim_w[2].1), (TAG_RELOAD, 1.0)] {
        if let Some(c) = a.layers.iter().find(|l| l.tag == tag && !l.finished()).map(|l| l.clip.clone()).filter(|_| !covered) {
            a.want(&c, tag, w, Mode::Once, 0.0);
        }
    }
    // The idles: each at the time its playback is at. One that has ended holds its last
    // pose until it is gone or the next idle takes over from it (a collection's next
    // idle starts a frame later), so the view it gives does not jump in between; the
    // rig's other clips are back under it meanwhile.
    let mut view_driven = false;
    for (p, w) in special.shown() {
        let newest = special.cur.as_ref().is_some_and(|c| c.serial == p.serial);
        let i = a.want(&p.clip, TAG_SPECIAL + p.serial % 1000, if newest { 1.0 } else { w }, Mode::Once, p.play.keys.blend_in);
        let l = &mut a.layers[i];
        (l.rate, l.time) = (0.0, p.play.time());
        if !newest {
            // The one before: its share shrinks as the new one fades in.
            l.weight = w;
        }
        view_driven |= rig.camera.is_some() && p.clip.tracks.iter().any(|(n, _)| n == crate::idles::CAMERA_NODE);
    }
    // The Pip-Boy: its clip at the time the Pip-Boy is at, over the rest (its tracks'
    // priority is above the others' [observed: 98]); it does not turn the view.
    if let Some((c, t)) = &pipboy {
        let i = a.want(&c.clip, TAG_PIPBOY, 1.0, Mode::Once, 0.0);
        let l = &mut a.layers[i];
        (l.rate, l.time) = (0.0, *t);
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
    if let Some((_, t)) = pipboy {
        for l in a.layers.iter_mut().filter(|l| l.tag == TAG_PIPBOY) {
            (l.weight, l.ease) = (1.0, pipboy_ease(t));
        }
    }
    a.pose();
    if let Some(l) = rig.looking {
        // The arms rig turns about `Bip01 Looking` with the view's pitch (about the
        // actor's right axis, x) [hypothesis: the node's role, from its name and place].
        let bind = a.skel.locals[l];
        a.locals[l].rotation = Quat::from_rotation_x(look_pitch) * bind.rotation;
    }
    a.apply(transforms);
    if let Ok(mut t) = transforms.get_mut(rig.root) {
        *t = root;
    }
    let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
    match rig.view {
        View::First => rig.camera.map(|c| {
            let m = root_m * a.model_space(c);
            (m.translation.into(), view_driven.then(|| m.to_scale_rotation_translation().1))
        }),
        View::Third => rig.head.map(|h| ((root_m * a.model_space(h)).transform_point3(rig.eye_local), None)),
    }
}

/// Draw the first-person rig `k` times as big across the view about the eye (along the
/// view it keeps its depth): what a view of a narrower field of view gives, without a
/// second camera [design]. The rig's root keeps the place [`pose_rig`] gave it, under
/// the lens nodes: the outer node stands at the eye looking along `forward`, the inner
/// one scales across that look.
fn place_lens(rig: &Rig, eye: Vec3, forward: Vec3, k: f32, transforms: &mut Query<&mut Transform>) {
    let Some([outer, inner]) = rig.lens else { return };
    let Ok(root) = transforms.get(rig.root).copied() else { return };
    let at = Transform { translation: eye, rotation: Quat::from_rotation_arc(Vec3::Y, forward.normalize_or(Vec3::Y)), scale: Vec3::ONE };
    let local = Transform::from_matrix(at.to_matrix().inverse() * root.to_matrix());
    for (e, t) in [(outer, at), (inner, Transform::from_scale(Vec3::new(k, 1.0, k))), (rig.root, local)] {
        if let Ok(mut m) = transforms.get_mut(e) {
            *m = t;
        }
    }
}

/// Pose the third-person rig with a furniture clip alone and place it at `root`
/// (`crate::furniture`); returns the eye. The clips of one use play in different frames,
/// so a new clip replaces the last at once instead of fading in.
fn pose_furniture(rig: &mut Rig, o: &crate::furniture::BodyClip, special: &IdleSlot<Arc<Clip>>, dt: f32, root: Transform, transforms: &mut Query<&mut Transform>) -> Option<Vec3> {
    let a = &mut rig.animator;
    let mode = if o.looped { Mode::Loop } else { Mode::Once };
    if rig.furniture != Some(o.serial) {
        a.layers.retain(|l| l.tag >= TAG_SPECIAL);
        a.restart(&o.clip, TAG_FURNITURE, 1.0, mode, 0.0);
        rig.furniture = Some(o.serial);
    }
    a.begin();
    a.want(&o.clip, TAG_FURNITURE, 1.0, mode, 0.0);
    // Over it the idle the body plays there (a fidget on furniture, `idles.rs`), as
    // `pose_rig` lays them.
    for (p, w) in special.shown() {
        let newest = special.cur.as_ref().is_some_and(|c| c.serial == p.serial);
        let i = a.want(&p.clip, TAG_SPECIAL + p.serial % 1000, if newest { 1.0 } else { w }, Mode::Once, p.play.keys.blend_in);
        let l = &mut a.layers[i];
        (l.rate, l.time) = (0.0, p.play.time());
        if !newest {
            l.weight = w;
        }
    }
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

/// How bone `b` of the posed rig is turned in game space since the bind pose, the rig's
/// root facing `heading`: it takes +X to the bone's right, +Y to the way it faces and +Z
/// to its up (the bind pose stands upright facing the rig's +Y).
fn bone_turn(rig: &Rig, b: usize, heading: f32) -> Quat {
    let a = &rig.animator;
    let turned = Mat3::from(a.model_space(b).matrix3 * a.skel.bind_world[b].matrix3.inverse());
    (Quat::from_rotation_z(-heading) * Quat::from_mat3(&turned)).normalize()
}

/// The way a view looks and the top of its picture, for a turn clockwise from +Y towards
/// +X and a tilt up from the XY plane: in game space for a compass heading and a pitch,
/// in a body's own frame ([`bone_turn`]) for its head's turn and tilt.
fn view_axes(turn: f32, tilt: f32) -> (Vec3, Vec3) {
    let (st, ct) = turn.sin_cos();
    let (sp, cp) = tilt.sin_cos();
    (Vec3::new(st * cp, ct * cp, sp), Vec3::new(-st * sp, -ct * sp, cp))
}

/// The compass heading and pitch of the level view nearest a view that looks along
/// `forward` with `up` the top of its picture (game space). Looking straight up or down
/// the top of the picture says which way the view is turned; `heading` is kept when
/// neither does.
fn view_angles(forward: Vec3, up: Vec3, heading: f32) -> (f32, f32) {
    let pitch = forward.z.clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH);
    let (sp, cp) = pitch.sin_cos();
    let flat = forward.truncate() * cp - up.truncate() * sp;
    let heading = if flat.length_squared() > 1e-6 { flat.x.atan2(flat.y).rem_euclid(std::f32::consts::TAU) } else { heading };
    (heading, pitch)
}

/// The first-person view of a body lying on a bed, in the body's own frame: the head's
/// turn to the body's right and its tilt towards the top of the body's head, radians.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LyingView {
    turn: f32,
    tilt: f32,
    /// The heading and pitch given to the player for it.
    wrote: (f32, f32),
}

/// One frame of the view of a body lying on a bed (or lying down on it, or getting up)
/// [engineering choice throughout; the original's player never lies down]:
///
/// - The view keeps its place in the body's frame (`frame`, [`bone_turn`] of the pelvis)
///   from frame to frame, so it lies back with the body and the picture leans with it:
///   on its back the body looks at the ceiling, the top of the picture towards the top
///   of its head; with the head turned on the pillow the room is on its side [owner
///   request 2026-10-10: the camera sideways when lying].
/// - The mouse (`look`: the heading and pitch asked for since the last frame) turns and
///   tilts the head in that frame. A heading and pitch set by anything else (the first
///   frame, a test aid) are taken as they are, in game space.
/// - The head turns as far as a seated one ([`NECK_TURN`], [`NECK_DOWN`];
///   [`seated_heading`] for a view that starts further round): beyond that the view is
///   in the body.
/// - The further the body has lain back, the less the view looks below level, down to
///   [`LYING_DOWN`]: the head is on the pillow and below it is the mattress. A body on
///   its side turns its head up to the ceiling but not down into the pillow.
///
/// Returns the view, the way it looks and the top of its picture (game space), and
/// whether it is within the neck's reach.
fn lying_view(last: Option<LyingView>, frame: Quat, heading: f32, pitch: f32, look: [f32; 2], settled: &mut bool, dt: f32) -> (LyingView, [Vec3; 2], bool) {
    use std::f32::consts::{PI, TAU};
    let near = |a: f32, b: f32| ((a - b + PI).rem_euclid(TAU) - PI).abs() < 1e-3;
    let (turn, tilt) = match last {
        Some(v) if near(heading, v.wrote.0 + look[0]) && near(pitch, (v.wrote.1 + look[1]).clamp(-MAX_PITCH, MAX_PITCH)) => (v.turn + look[0], v.tilt + look[1]),
        _ => {
            let d = frame.inverse() * view_axes(heading, pitch).0;
            (d.x.atan2(d.y), d.z.clamp(-1.0, 1.0).asin())
        }
    };
    let (turn, within) = seated_heading(turn, 0.0, settled, dt);
    let mut turn = (turn + PI).rem_euclid(TAU) - PI;
    let mut tilt = tilt.clamp(-NECK_DOWN, MAX_PITCH);
    let lain = (1.0 - (frame * Vec3::Z).z).clamp(0.0, 1.0);
    let lowest = -MAX_PITCH + (MAX_PITCH - LYING_DOWN) * lain;
    let ahead = frame * view_axes(turn, tilt).0;
    if ahead.z < lowest.sin() {
        // Up to the lowest it may look, the same way round.
        let d = frame.inverse() * (ahead.truncate().try_normalize().unwrap_or(Vec2::Y) * lowest.cos()).extend(lowest.sin());
        (turn, tilt) = (d.x.atan2(d.y), d.z.clamp(-1.0, 1.0).asin());
    }
    let (forward, up) = view_axes(turn, tilt);
    let (forward, up) = (frame * forward, frame * up);
    (LyingView { turn, tilt, wrote: view_angles(forward, up, heading) }, [forward, up], within)
}

/// What of the third-person rig the camera sees. All of it casts the shadow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    /// Any of it.
    body: bool,
    /// Its head, its arms and hands, the weapon on its `Weapon` node.
    head: bool,
    arms: bool,
    weapon: bool,
}

impl Seen {
    /// The render layer of a drawn entity: the world's when the camera sees it, else the
    /// one only the shadows are cast from.
    fn layer(on: bool) -> RenderLayers {
        RenderLayers::layer(if on { 0 } else { SHADOW_LAYER })
    }

    fn part(self, d: &Drawn) -> bool {
        self.body && (self.head || !d.head) && (self.arms || !d.arm)
    }

    /// A shape of the weapon model: the ones on a bone of their own (the minigun's
    /// backpack) are seen with the body.
    fn weapon_part(self, p: &WeaponPart) -> bool {
        self.body && (self.weapon || p.on_bone)
    }
}

/// Show the camera `seen` of the third-person rig, and the first-person rig or not.
fn show_rigs(body: &mut Body, commands: &mut Commands, visibility: &mut Query<&mut Visibility>, seen: Seen, first: bool) {
    let third = &mut body.third;
    if third.seen != Some(seen) {
        for d in &third.drawn {
            commands.entity(d.entity).insert(Seen::layer(seen.part(d)));
        }
        for p in &third.weapon_parts {
            commands.entity(p.entity).insert(Seen::layer(seen.weapon_part(p)));
        }
        third.seen = Some(seen);
        debug!("player body: the camera sees {seen:?} of the body");
    }
    if let Some(f) = body.first.as_mut().filter(|f| f.shown != Some(first)) {
        if let Ok(mut v) = visibility.get_mut(f.root) {
            *v = if first { Visibility::Inherited } else { Visibility::Hidden };
        }
        debug!("player body: first-person rig {}", if first { "shown" } else { "hidden" });
        f.shown = Some(first);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_body(
    mut commands: Commands,
    time: Res<Time>,
    d: Res<PlayWorld>,
    intent: Res<MoveIntent>,
    control: Res<PlayerControl>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut view: ResMut<BodyView>,
    mut state: ResMut<PlayerBody>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut anim: ResMut<WeaponAnim>,
    (furniture, mut idles, mut seat, pip, cams, first_fov, limbs, (death, knock)): (
        Res<crate::furniture::BodyOverride>,
        ResMut<PlayerIdles>,
        ResMut<nvf_play::furniture::PlayerSeat>,
        Res<nvf_play::pipboy::PipBoy>,
        Query<&Projection, With<nvf_play::PlayerCamera>>,
        Option<Res<nvf_play::free_cam::FirstPersonFov>>,
        Option<Res<nvf_play::combat::PlayerLimbs>>,
        (Res<crate::death_cam::DeathCam>, Res<crate::player_knockdown::PlayerKnockdown>),
    ),
    mut driven: ResMut<nvf_play::player::ViewDriven>,
) {
    let state = &mut *state;
    player.body_view = None;
    anim.drawing = false;
    anim.holstering = false;
    state.head_eye = None;
    let own_body = state.first_person_body;
    let seat_look = std::mem::take(&mut seat.look);
    let Some(body) = state.body.as_mut() else {
        player.body_eye = None;
        view.pivot = None;
        // No body to time the weapon: it is ready as soon as it is wanted out.
        anim.ready = control.weapon_out;
        anim.muzzle = None;
        anim.casing = None;
        state.muzzle_entity = None;
        return;
    };
    if death.down() || knock.ragdolled() {
        // Dead or knocked down: the clips stop and the ragdoll has the body
        // (`death_cam.rs`, `player_knockdown.rs`). The whole body is drawn, but for its
        // head while the view is inside it (a knockdown's view is outside).
        anim.ready = false;
        anim.muzzle = None;
        anim.casing = None;
        state.muzzle_entity = None;
        show_rigs(body, &mut commands, &mut visibility, Seen { body: true, head: death.third_person() || knock.ragdolled(), arms: true, weapon: true }, false);
        return;
    }
    let dt = time.delta_secs();
    let m = &mut state.motion;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };

    // What the character is doing.
    let (grounded, swimming, ground_gap) = phys.player.as_ref().map_or((true, false, None), |c| (c.grounded, c.swimming, c.ground_gap));
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
    let was_falling = m.falling;
    m.falling = m.airborne > 0.0 && (m.falling || (m.airborne >= AIRBORNE_LOOP && ground_gap.is_none_or(|g| g > FALL_GAP)));
    let landing = was_falling && airborne_before >= AIRBORNE_LAND && m.airborne == 0.0;
    m.falls += u32::from(m.falling && !was_falling);
    m.landings += u32::from(landing);
    if m.falling != was_falling {
        let gap = ground_gap.map_or("out of reach".to_string(), |g| format!("{g:.1} units"));
        debug!("player body: {} after {:.2}s in the air (ground {gap} below)", if m.falling { "falling" } else { "down again" }, airborne_before.max(m.airborne));
    }

    let third_view = control.third_person_view();
    if let Some(o) = furniture.0.as_ref() {
        // On furniture: the third-person body plays the furniture's clip where the
        // furniture has it. In first person the view is that body's eye and the body is
        // not drawn (seen from inside it gets in the way of looking round) [design],
        // unless the body is wanted in first person: then all but its head is, and the
        // view turns only as far as a neck does, so it never looks into the body.
        let root = Transform { translation: Vec3::from_array(o.pos), rotation: Quat::from_rotation_z(-o.heading), scale: Vec3::splat(body.scale) };
        let mut eye = pose_furniture(&mut body.third, o, &idles.third, dt, root, &mut transforms);
        let pelvis = body.third.animator.skel.by_name.get("Bip01 Pelvis").copied();
        // A body on a bed: the frame it lies in is its pelvis's.
        let lying = o.lying.then(|| pelvis.map_or(root.rotation, |b| bone_turn(&body.third, b, o.heading)));
        let own_body = if let (Some(frame), false) = (lying, third_view) {
            // Lying down: the view is the lying body's, drawn or not ([`lying_view`]).
            let third = &body.third;
            let (v, [forward, up], within) = lying_view(m.lying, frame, player.heading, player.pitch, seat_look, &mut m.seat_settled, dt);
            (player.heading, player.pitch) = v.wrote;
            m.lying = Some(v);
            m.seat_facing = None;
            // The camera leans with the head: its right, the way it looks and its top.
            player.body_view = Some(Quat::from_mat3(&Mat3::from_cols(forward.cross(up), forward, up)));
            // With the body drawn the eyes go round the neck with the view, about the
            // body's own length, as a head turned on a pillow: the open neck stays
            // behind the view.
            if let (true, Some(e), Some(h)) = (own_body, eye.as_mut(), third.head) {
                let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
                let neck = Vec3::from((root_m * third.animator.model_space(h)).translation);
                let faces = frame.inverse() * (bone_turn(third, h, o.heading) * Vec3::Y);
                *e = neck + Quat::from_axis_angle(frame * Vec3::Z, faces.x.atan2(faces.y) - v.turn) * (*e - neck);
            }
            own_body && within
        } else if own_body && !third_view {
            use std::f32::consts::{PI, TAU};
            let third = &body.third;
            m.lying = None;
            // The way the body faces is its pelvis's: the clips of getting on and off
            // turn it inside the rig. The view turns with it (owner report 2026-10-10:
            // sitting down, the camera "didn't move in line with the rest of the body"),
            // and within a neck's reach of it is the player's.
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
            m.lying = None;
            false
        };
        player.body_eye = eye.map(|e| e.to_array());
        state.head_eye = eye.filter(|_| third_view).map(|e| e.to_array());
        // Third person: the pivot is above the body; the further the body has lain
        // back, the higher, clear of the bed.
        let lift = lying.map_or(0.0, |frame| LYING_PIVOT_UP * (1.0 - (frame * Vec3::Z).z).clamp(0.0, 1.0));
        view.pivot = eye.map(|e| [o.pos[0], o.pos[1], e.z + 5.0 + lift]);
        anim.ready = false;
        anim.muzzle = None;
        anim.casing = None;
        state.muzzle_entity = None;
        show_rigs(body, &mut commands, &mut visibility, Seen { body: third_view || own_body, head: third_view, arms: true, weapon: true }, false);
        return;
    }
    if body.third.furniture.take().is_some() {
        body.third.animator.cut(TAG_FURNITURE);
    }
    m.seat_settled = false;
    m.seat_facing = None;
    m.lying = None;
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
    // A melee swing plays the clip combat chose (a combo's next group, a power attack's
    // direction; `fnv_combat::melee`), else the weapon's attack animation.
    let attack = match anim.swing.clone() {
        Some(stem) => m.intern(stem),
        None => m.intern(attack_stem(anim.attack.as_deref())),
    };
    let reload = anim.reload.clone().map(|r| m.intern(r));
    let attack_clip = timing_rig.clip(&mut ctx, ClipKey::Attack(stance, prefix, attack, control.aiming));
    let reload_clip = reload.and_then(|r| timing_rig.clip(&mut ctx, ClipKey::Reload(stance, prefix, r)));
    let reload_open = reload.and_then(|r| timing_rig.clip(&mut ctx, ClipKey::ReloadOpen(stance, prefix, r)));
    let jam = anim.jam.clone().map(|j| m.intern(j));
    let jam_len = jam.and_then(|j| timing_rig.clip(&mut ctx, ClipKey::Reload(stance, prefix, j))).map(|c| c.clip.length()).filter(|l| *l > 0.0);
    anim.timing = nvf_play::combat::clip_timing(
        attack_clip.as_ref().map(|c| (c.clip.length(), c.keys.hit, c.keys.hit.zip(c.keys.loop_end).is_some_and(|(fire, back)| fire < back))),
        reload_clip.as_ref().map(|c| (c.clip.length(), c.keys.hit)),
        reload_open.as_ref().map(|c| c.clip.length()),
        attack_clip.as_ref().and_then(|c| c.keys.next_attack.as_ref()).map(|(t, v)| (*t, v.as_str())),
        attack_clip.as_ref().and_then(|c| c.keys.attach),
    );
    anim.timing.jam_len = jam_len;
    anim.timed = Some(attack.to_string());
    anim.ready = m.hands.phase == WeaponPhase::Drawn && m.hands.in_hand;
    anim.drawing = m.hands.phase == WeaponPhase::Drawing;
    anim.holstering = m.hands.phase == WeaponPhase::Holstering;
    let attack_now = anim.attacks != m.attacks_seen;
    let reload_now = anim.reloads != m.reloads_seen;
    let reload_round_now = anim.reload_rounds != m.reload_rounds_seen;
    let spin_now = anim.spin_ups != m.spin_ups_seen;
    m.attacks_seen = anim.attacks;
    m.reloads_seen = anim.reloads;
    m.reload_rounds_seen = anim.reload_rounds;
    let jam_now = anim.jams != m.jams_seen;
    m.jams_seen = anim.jams;
    m.spin_ups_seen = anim.spin_ups;

    // While an idle turns the first-person view, its rig keeps the heading the idle
    // started at and the camera node alone places the view: mouse look and movement are
    // ignored (`nvf_play::player::ViewDriven`) [design: the owner's wish, playtest of
    // 2026-10-11; the opening's scripts leave looking on, so the original may let the
    // view turn; hypothesis that the body does not swing round with the view].
    if idles.first.is_empty() {
        if let Some(h) = idles.held.take() {
            seat.hold = None;
            // The character stands where it stood before the idles held it, clear of what
            // it collides with: let go where it was held (the opening's marker, against
            // the bed), it was pushed up onto the bed frame [design: owner report
            // 2026-10-16; idles.md §4]. The stand-up ends with the view over the marker,
            // so the view steps aside by that much (22 units in the opening).
            let hit = phys.place_player(h.free, 60.0, 200.0);
            let stood = phys.player.as_ref().map_or(h.free, |c| c.feet);
            info!(
                "player view: idle over; the character stands at ({:.0}, {:.0}, {:.0}), {:.0} units from where it was held{}",
                stood[0],
                stood[1],
                stood[2],
                (stood[0] - h.feet[0]).hypot(stood[1] - h.feet[1]),
                if hit.is_some() { "" } else { " (no ground under it)" }
            );
            player.feet = stood;
            m.after_idle = Some(1.0);
        }
    }
    // A second later: where the collision has left the character (a smoke check).
    if let Some(t) = m.after_idle.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            m.after_idle = None;
            info!("player view: a second after the idle the character stands at ({:.0}, {:.0}, {:.0})", feet[0], feet[1], feet[2]);
        }
    }
    if driven.0 != idles.held.is_some() {
        driven.0 = idles.held.is_some();
        info!("player view: {}", if driven.0 { "driven by an idle's camera node; mouse look and movement ignored" } else { "back to the player" });
        if own_body {
            info!("player body: {} in first person", if driven.0 { "not drawn while the idle drives the view" } else { "drawn again" });
        }
    }
    let frame = Frame {
        dt,
        stance,
        dirs: direction_weights(forward, right),
        fast,
        female: body.female,
        limping: limbs.as_deref().is_some_and(|l| l.limping()),
        prefix,
        drawn: (m.hands.phase == WeaponPhase::Drawn).then_some(prefix),
        hands: m.hands,
        airborne: if m.falling { m.airborne } else { 0.0 },
        landing,
        turn_rate: m.turn_rate,
        pitch: player.pitch,
        // Not while an idle drives the view (the opening's wake-up): the arms rig plays
        // the idle and the body would stand where the view lies [design: owner report
        // 2026-10-16, "the players own body standing over themselves"].
        upright: own_body && !third_view && body.first.is_some() && idles.held.is_none(),
        aiming: control.aiming,
        speed: m.speed,
        attack: Some(attack),
        reload,
        jam,
        jam_now,
        attack_now,
        reload_now,
        reload_round_now,
        spin_now,
    };
    // The body stands on the drawn ground under the feet (`nvf_play::phys::drawn_sink`);
    // its eye counts from the feet as before.
    let sink = Vec3::Z * phys.drawn_sink_of(FormId::PLAYER_REF.0);
    let root = Transform { translation: Vec3::from_array(feet) - sink, rotation: Quat::from_rotation_z(-player.heading), scale: Vec3::splat(body.scale) };
    let third = Posed { root, look_pitch: player.pitch, special: &idles.third, pipboy: None };
    let third_eye = pose_rig(&mut body.third, &frame, &events, third, &mut ctx, &mut transforms).map(|(e, _)| e + sink);
    let (held, pitch, at) = idles.held.map_or((player.heading, player.pitch, feet), |h| (h.heading, player.pitch - h.pitch, h.feet));
    if let Some(h) = idles.held {
        // The character stays where the scene plays (and collides with nothing there).
        seat.hold = Some(h.feet);
    }
    let pipboy = pip.shown().then_some(pip.clip_time);
    let first = Posed { root: Transform { translation: Vec3::from_array(at), rotation: Quat::from_rotation_z(-held), ..root }, look_pitch: pitch, special: &idles.first, pipboy };
    let first = body.first.as_mut().and_then(|f| pose_rig(f, &frame, &events, first, &mut ctx, &mut transforms));
    let first_eye = first.map(|(e, _)| e);
    // The arms and the weapon are drawn as through the first-person view, narrower than
    // the world's, and narrower still while the Pip-Boy shows.
    if let (Some(f), Some(eye)) = (body.first.as_ref(), first_eye) {
        let vfov = cams.single().ok().and_then(|p| match p {
            Projection::Perspective(p) => Some(p.fov),
            _ => None,
        });
        // The console's `fov` can ask for another first-person field of view.
        let first_fov = first_fov.and_then(|f| f.0).unwrap_or(FIRST_PERSON_FOV);
        let k = match (pip.shown(), vfov) {
            (true, Some(v)) => {
                let normal = arms_magnification(v, first_fov);
                normal + (arms_magnification(v, PIPBOY_FOV) - normal) * pipboy_ease(pip.clip_time)
            }
            (false, Some(v)) => arms_magnification(v, first_fov),
            _ => 1.0,
        };
        let forward = Vec3::from_array(nvf_play::space::look_dir(player.heading, player.pitch));
        place_lens(f, eye, forward, k, &mut transforms);
    }
    match first.and_then(|(_, v)| v) {
        Some(v) => {
            let v = Quat::from_rotation_z(held - player.heading) * v;
            player.body_view = Some(v);
            m.idle_view = Some(v);
        }
        // The idle is over: the view goes on from where it left it.
        None => {
            if let Some(v) = m.idle_view.take() {
                let fwd = v * Vec3::Y;
                player.heading = fwd.x.atan2(fwd.y);
                player.pitch = fwd.z.clamp(-1.0, 1.0).asin();
            }
        }
    }
    m.events = events;

    // The eye: the first-person camera node; without that rig, the head's eye, swung
    // forward about the neck by the pitch while no weapon bends the body.
    let eye = first_eye.or_else(|| {
        let mut e = third_eye?;
        if let (false, Some(h)) = (m.hands.in_hand, body.third.head) {
            let head = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation) * body.third.animator.model_space(h);
            let right = Vec3::new(player.heading.cos(), -player.heading.sin(), 0.0);
            let neck = Vec3::from(head.translation) + sink;
            e = neck + Quat::from_axis_angle(right, NECK_PITCH * player.pitch) * (e - neck);
        }
        Some(e)
    });
    player.body_eye = eye.map(|e| e.to_array());
    state.head_eye = third_eye.filter(|_| third_view).map(|e| e.to_array());
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
    // The muzzle of the weapon in hand, for combat (shots are drawn from it;
    // `WeaponAnim::muzzle`): in the first-person rig while that view is on (its weapon
    // is the one seen in hand, with or without the body below it), else the body's.
    let seen = match body.first.as_ref() {
        Some(f) if !third_view => f,
        _ => &body.third,
    };
    anim.muzzle = if m.hands.in_hand { seen.muzzle_at(root).map(|p| p.to_array()) } else { None };
    state.muzzle_entity = seen.muzzle_entity.filter(|_| m.hands.in_hand);
    anim.casing = if m.hands.in_hand { seen.casing_at(root).map(|(p, d)| (p.to_array(), d.to_array())) } else { None };
    let arms_up = m.hands.in_hand || m.hands.phase != WeaponPhase::Holstered;
    let seen = if frame.upright {
        Seen { body: true, head: false, arms: !arms_up, weapon: !m.hands.in_hand }
    } else {
        Seen { body: third_view || !has_first, head: third_view || has_first, arms: true, weapon: true }
    };
    show_rigs(body, &mut commands, &mut visibility, seen, !third_view);

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
    fn a_view_lies_down_with_its_body() {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
        // Standing, facing east: the view is the player's, level.
        let standing = Quat::from_rotation_z(-FRAC_PI_2);
        let mut settled = false;
        let (v, [forward, up], within) = lying_view(None, standing, FRAC_PI_2 + 0.3, -0.2, [0.0; 2], &mut settled, 0.016);
        assert!((v.turn - 0.3).abs() < 1e-4 && (v.tilt + 0.2).abs() < 1e-4 && within, "{v:?}");
        assert!(close(forward, view_axes(FRAC_PI_2 + 0.3, -0.2).0) && close(up, view_axes(FRAC_PI_2 + 0.3, -0.2).1));
        assert!((v.wrote.0 - (FRAC_PI_2 + 0.3)).abs() < 1e-4 && (v.wrote.1 + 0.2).abs() < 1e-4, "{v:?}");

        // The body lies back, its head to the west: a view straight ahead looks at the
        // ceiling, the top of the picture towards the top of the head. The level view
        // nearest to it looks east, the way the feet point, as far up as a view goes.
        let on_back = standing * Quat::from_rotation_x(FRAC_PI_2);
        assert!(close(on_back * Vec3::Y, Vec3::Z) && close(on_back * Vec3::Z, -Vec3::X));
        let ahead = LyingView { turn: 0.0, tilt: 0.0, wrote: (FRAC_PI_2, 0.0) };
        let (v, [forward, up], _) = lying_view(Some(ahead), on_back, FRAC_PI_2, 0.0, [0.0; 2], &mut settled, 0.016);
        assert!(close(forward, Vec3::Z) && close(up, -Vec3::X), "{forward} {up}");
        assert!((v.wrote.0 - FRAC_PI_2).abs() < 1e-3 && v.wrote.1 == MAX_PITCH, "{v:?}");
        // The mouse turns the head in the body's frame, also where a level view is
        // already as far up as it goes: to the body's right is south here.
        let (heading, pitch) = look_after(v, [0.5, 0.25]);
        let (v, [forward, _], _) = lying_view(Some(v), on_back, heading, pitch, [0.5, 0.25], &mut settled, 0.016);
        assert!((v.turn - 0.5).abs() < 1e-4 && (v.tilt - 0.25).abs() < 1e-4, "{v:?}");
        assert!(forward.y < -0.3 && forward.z > 0.5 && forward.x < -0.1, "{forward}");
        // No further than a neck turns, and not down into the body.
        let (heading, pitch) = look_after(v, [2.0, -3.0]);
        let (v, _, within) = lying_view(Some(v), on_back, heading, pitch, [2.0, -3.0], &mut settled, 0.016);
        assert!((v.turn - NECK_TURN).abs() < 1e-4 && v.tilt >= -NECK_DOWN - 1e-4 && within, "{v:?}");
        // A heading and pitch set from outside are taken in game space: north and a
        // little up is to the body's left.
        let (v, [forward, _], _) = lying_view(Some(v), on_back, 0.0, 0.3, [0.0; 2], &mut settled, 0.016);
        assert!(close(forward, view_axes(0.0, 0.3).0) && v.turn < -1.0, "{v:?} {forward}");

        // On its right side (its right is down): the head turns up to the ceiling but
        // not into the pillow.
        let on_side = on_back * Quat::from_rotation_z(-FRAC_PI_2);
        assert!(close(on_side * Vec3::X, -Vec3::Z), "{}", on_side * Vec3::X);
        let mut settled = true;
        for (turn, tilt) in [(1.2, 0.0), (0.8, -0.9), (0.6, 0.7), (-1.3, 0.2), (0.0, 0.0)] {
            let last = LyingView { turn: 0.0, tilt: 0.0, wrote: (0.0, 0.0) };
            let (v, [forward, up], _) = lying_view(Some(last), on_side, turn, tilt, [turn, tilt], &mut settled, 0.016);
            assert!(forward.z >= -LYING_DOWN.sin() - 1e-3, "{turn} {tilt}: {forward}");
            assert!((forward.length() - 1.0).abs() < 1e-3 && forward.dot(up).abs() < 1e-3);
            // Looking up is left alone.
            assert!(turn > 0.0 || ((v.turn - turn).abs() < 1e-4 && (v.tilt - tilt).abs() < 1e-4), "{turn} {tilt}: {v:?}");
            assert!((0.0..TAU).contains(&v.wrote.0) && v.wrote.1.abs() <= MAX_PITCH && v.turn.abs() <= PI);
        }
    }

    /// The heading and pitch the mouse leaves the player with after `look`.
    fn look_after(v: LyingView, look: [f32; 2]) -> (f32, f32) {
        ((v.wrote.0 + look[0]).rem_euclid(std::f32::consts::TAU), (v.wrote.1 + look[1]).clamp(-MAX_PITCH, MAX_PITCH))
    }

    #[test]
    fn a_level_view_is_found_for_a_leaning_one() {
        use std::f32::consts::FRAC_PI_2;
        // A level view is its own.
        for (h, p) in [(0.3, 0.2), (4.0, -1.0), (2.0, 1.3)] {
            let (forward, up) = view_axes(h, p);
            let (heading, pitch) = view_angles(forward, up, 0.0);
            assert!((heading - h).abs() < 1e-4 && (pitch - p).abs() < 1e-4, "{h} {p}");
        }
        // Straight up, the top of the picture to the west: the view is turned east.
        let (heading, pitch) = view_angles(Vec3::Z, -Vec3::X, 0.0);
        assert!((heading - FRAC_PI_2).abs() < 1e-3 && pitch == MAX_PITCH);
        // Level, the picture on its side: the heading is the view's own.
        let (heading, pitch) = view_angles(Vec3::X, Vec3::Y, 0.0);
        assert!((heading - FRAC_PI_2).abs() < 1e-4 && pitch.abs() < 1e-4);
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
