//! The player's own body in `--play` (docs/design/player-body.md): the player's NPC_
//! (FormID 7) assembled like a placed NPC (`actors.rs`), wearing what the simulation has
//! equipped, standing at the character's feet and facing the view's heading, animated
//! from the controls with layered KF clips (`nvf_assets::anim_blend`) chosen by
//! `fnv_actor::locomotion`: an idle, movement in four directions walking or running,
//! sneaking, swimming, jumping and landing, turning on the spot, and the weapon drawn,
//! held and holstered. The camera's eye is the head's (between the eyes), so it crouches,
//! bobs and leans with the body; in first person the head and headgear are hidden.
//!
//! Runs in `nvf_play::player::BodySet`: after the character moved, before the camera is
//! placed. Without actors (`--no-actors`) there is no body and the eye stays at the
//! fixed eye height.

use crate::actors::{part_uses, ActorWorld};
use crate::world::WorldRoot;
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use fnv_actor::locomotion::{
    aim_clips, aim_weights, direction_weights, equip_clips, holster_clips, idle_clips, jump_clips, move_clips, turn_clips, weapon_prefix, Aim,
    ClipKeys, Dir, HandEvent, Stance, Transition, WeaponHands, WeaponPhase, UNARMED,
};
use fnv_sim::FormId;
use nvf_assets::actor::{rigid_local, spawn_actor, ActorCtx, ActorParts, PreparedSkeleton};
use nvf_assets::anim_blend::{Animator, Mode, DEFAULT_FADE};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::ModelCtx;
use nvf_play::control::{BodyView, PlayerControl, Pov};
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

/// Biped slots of headgear hidden in first person: Head, Hair, Headband, Hat, Eye
/// Glasses, Nose Ring, Earrings, Mask, Mouth Object [documented: xEdit biped flag names,
/// bits 0, 1, 9–14, 16; engineering choice: which ones block the first-person view].
const HEAD_SLOTS: u32 = 1 | 1 << 1 | 0x7E << 8 | 1 << 16;

/// Time in the air before the jump loop plays, and the shortest fall that plays a
/// landing, seconds [engineering choice: steps and slopes leave the ground briefly].
const AIRBORNE_LOOP: f32 = 0.15;
const AIRBORNE_LAND: f32 = 0.35;
/// Turning on the spot faster than this (radians per second) plays a turn clip
/// [engineering choice].
const TURN_RATE: f32 = 1.0;
/// Share of the view's pitch the neck takes when nothing else bends the body: the eye
/// swings forward and down about the head bone when looking down, so the view clears
/// the chest [engineering choice: the clips do not move the head with the view; with a
/// weapon out the aim-up and aim-down clips bend the body instead].
const NECK_PITCH: f32 = 0.6;

/// A clip choice, for the clip cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ClipKey {
    Idle(Stance),
    Move(Stance, bool, bool, Dir, Option<&'static str>),
    Aim(Stance, &'static str, Aim, bool),
    Holster(Stance, &'static str),
    Equip(Stance, &'static str, bool),
    Jump(Stance, Option<&'static str>, Option<Dir>, bool),
    Turn(Stance, Option<&'static str>, bool),
}

impl ClipKey {
    fn candidates(self) -> Vec<String> {
        match self {
            ClipKey::Idle(s) => idle_clips(s),
            ClipKey::Move(s, female, fast, d, w) => move_clips(s, female, fast, d, w),
            ClipKey::Aim(s, p, a, sights) => aim_clips(s, p, a, sights),
            ClipKey::Holster(s, p) => holster_clips(s, p),
            ClipKey::Equip(s, p, e) => equip_clips(s, p, e),
            ClipKey::Jump(s, w, d, land) => jump_clips(s, w, d, land),
            ClipKey::Turn(s, w, left) => turn_clips(s, w, left),
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
}

impl ClipInfo {
    fn fade(&self) -> f32 {
        self.keys.blend_seconds(DEFAULT_FADE)
    }
}

struct Body {
    root: Entity,
    worn: Vec<FormId>,
    /// The skeleton's folder (`meshes\characters\_male\`).
    folder: String,
    female: bool,
    scale: f32,
    animator: Animator,
    head: Option<usize>,
    /// The eye in the head bone's space.
    eye_local: Vec3,
    weapon_node: Option<usize>,
    /// Drawn entities hidden in first person.
    head_entities: Vec<Entity>,
    hidden: Option<bool>,
    /// The skeleton node the weapon node should hang from, and the one it was last
    /// attached to.
    weapon_parent: Option<usize>,
    attached: Option<usize>,
}

/// State kept across body respawns (an outfit change rebuilds the body).
struct Motion {
    weapon: Option<FormId>,
    prefix: &'static str,
    weapon_model: Option<Entity>,
    hands: WeaponHands,
    events: Vec<HandEvent>,
    speed: f32,
    last_feet: Option<[f32; 3]>,
    airborne: f32,
    last_heading: Option<f32>,
    turn_rate: f32,
    landing: bool,
    /// Once-a-second status (debug log).
    log_elapsed: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            weapon: None,
            prefix: UNARMED,
            weapon_model: None,
            hands: WeaponHands::default(),
            events: Vec::new(),
            speed: 0.0,
            last_feet: None,
            airborne: 0.0,
            last_heading: None,
            turn_rate: 0.0,
            landing: false,
            log_elapsed: 0.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct PlayerBody {
    body: Option<Body>,
    motion: Motion,
    clips: HashMap<ClipKey, Option<Arc<ClipInfo>>>,
    /// Why the body could not be built (reported once).
    failed: Option<String>,
}

pub struct PlayerBodyPlugin;

impl Plugin for PlayerBodyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerBody>().add_systems(Update, (ensure_body, drive_body).chain().in_set(nvf_play::player::BodySet));
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
    if state.body.as_ref().is_none_or(|b| b.worn != worn) {
        if let Some(old) = state.body.take() {
            commands.entity(old.root).despawn();
            // The weapon model hung under the old skeleton.
            state.motion.weapon_model = None;
            state.motion.weapon = None;
        }
        let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
        match build(&mut commands, world_root, &world, &mut ctx, &worn) {
            Ok(b) => {
                info!(
                    "player body: {} skeleton nodes from {} (eye {:.1?} in the head's space), wearing {:?}",
                    b.animator.nodes.len(),
                    b.folder,
                    b.eye_local,
                    worn.iter().map(|w| w.to_string()).collect::<Vec<_>>()
                );
                // A weapon in the hand stays there; otherwise it goes to its holster.
                state.body = Some(b);
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
    if m.weapon != weapon || (m.weapon.is_some() && m.weapon_model.is_none()) {
        if let Some(e) = m.weapon_model.take() {
            commands.entity(e).despawn();
        }
        m.weapon = weapon;
        let w = weapon.and_then(|id| world.actors.weapons.get(&id));
        m.prefix = w.and_then(|w| w.animation_type).and_then(weapon_prefix).unwrap_or(UNARMED);
        let model = w.and_then(|w| w.model.as_deref()).map(fnv_actor::resolve::mesh_path);
        if let (Some(path), Some(node)) = (model, body.weapon_node) {
            match models.model(&path) {
                Some(pm) => {
                    let holder = commands.spawn((Name::new(format!("Player weapon {path}")), Transform::IDENTITY, Visibility::Inherited)).id();
                    commands.entity(body.animator.nodes[node]).add_child(holder);
                    for s in &pm.shapes {
                        let e = commands
                            .spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.transform))))
                            .id();
                        commands.entity(holder).add_child(e);
                    }
                    m.weapon_model = Some(holder);
                    info!("player weapon: {} ({path}), animations {}", weapon.map_or("-".to_string(), |w| w.to_string()), m.prefix);
                }
                None => warn!("player weapon {path}: model not loaded"),
            }
        } else {
            info!("player weapon: {}, animations {}", weapon.map_or("none (fists)".to_string(), |w| w.to_string()), m.prefix);
        }
    }
}

fn build(commands: &mut Commands, parent: Entity, world: &ActorWorld, ctx: &mut ActorCtx, worn: &[FormId]) -> Result<Body, String> {
    let a = world.resolve_wearing(FormId::PLAYER_BASE.0, FormId::PLAYER_REF.0, Some(worn))?;
    let skel = ctx.skeleton(&a.skeleton).ok_or_else(|| format!("skeleton {} not loaded", a.skeleton))?;
    let mut missing = 0;
    let uses = part_uses(&a, world, ctx, &mut missing);
    for (i, u) in &uses {
        let p = &a.parts[*i];
        debug!("player body part {} {} {}: {} shapes", p.role, p.form, p.model, u.part.shapes.len());
    }
    if missing > 0 {
        warn!("player body: {missing} parts not loaded");
    }
    let head = skel.by_name.get("Bip01 Head").copied();
    let eyes: Vec<_> = uses.iter().filter(|(i, _)| a.parts[*i].role.ends_with("_eye")).map(|(_, u)| &*u.part).collect();
    let eye_local = head.and_then(|h| eye_centre(&skel, h, &eyes).or_else(|| Some(default_eye(&skel, h)))).unwrap_or(Vec3::new(0.0, 0.0, 120.0));
    let head_part = |i: usize| {
        let p = &a.parts[i];
        p.role.starts_with("head") || p.role == "hair" || world.actors.armors.get(&p.form).is_some_and(|ar| ar.slots & HEAD_SLOTS != 0)
    };
    let hide: Vec<bool> = uses.iter().map(|(i, _)| head_part(*i)).collect();
    let uses: Vec<_> = uses.into_iter().map(|(_, u)| u).collect();
    let scale = a.race_height as f32;
    let spawned = spawn_actor(commands, parent, Transform::from_scale(Vec3::splat(scale)), &skel, &uses);
    commands.entity(spawned.root).insert(Name::new(format!("Player body {}", a.edid.as_deref().unwrap_or(""))));
    let head_entities = spawned.drawn.iter().zip(&spawned.part_of).filter(|(_, &p)| hide[p]).map(|(e, _)| *e).collect();
    let folder = {
        let s = &a.skeleton;
        s.rfind('\\').map_or(String::new(), |i| s[..=i].to_string())
    };
    let weapon_node = skel.by_name.get("Weapon").copied();
    let weapon_parent = weapon_node.and_then(|w| skel.parents[w]);
    let animator = Animator::new(skel.clone(), spawned.nodes, Some("Bip01"));
    Ok(Body {
        root: spawned.root,
        worn: worn.to_vec(),
        folder,
        female: a.female,
        scale,
        animator,
        head,
        eye_local,
        weapon_node,
        head_entities,
        hidden: None,
        weapon_parent,
        attached: weapon_parent,
    })
}

/// The first existing file of a clip choice, loaded (cached, failures too).
fn clip(cache: &mut HashMap<ClipKey, Option<Arc<ClipInfo>>>, ctx: &mut ActorCtx, folder: &str, key: ClipKey) -> Option<Arc<ClipInfo>> {
    if let Some(c) = cache.get(&key) {
        return c.clone();
    }
    let found = key.candidates().into_iter().map(|c| format!("{folder}{c}")).find(|p| ctx.models.data.vfs.exists(p)).and_then(|path| {
        let clip = ctx.clip(&path)?;
        let keys = ClipKeys::parse(&clip.text_keys);
        Some(Arc::new(ClipInfo { speed: clip.root_speed(), path, clip, keys }))
    });
    match &found {
        Some(c) => debug!("player body: {key:?} plays {}", c.path),
        None => debug!("player body: {key:?}: none of {:?}", key.candidates()),
    }
    cache.insert(key, found.clone());
    found
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
) {
    let state = &mut *state;
    let Some(body) = state.body.as_mut() else {
        player.body_eye = None;
        view.pivot = None;
        return;
    };
    let dt = time.delta_secs();
    let m = &mut state.motion;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    let folder = &body.folder;
    let mut get = |key| clip(&mut state.clips, &mut ctx, folder, key);

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
    if airborne_before >= AIRBORNE_LAND && m.airborne == 0.0 {
        m.landing = true;
    }

    // The weapon: draw and holster.
    let prefix = m.prefix;
    let draw = get(ClipKey::Equip(stance, prefix, true));
    let put_away = get(ClipKey::Equip(stance, prefix, false));
    let holster = get(ClipKey::Holster(stance, prefix));
    let draw_t = draw.as_ref().map_or(Transition::new(0.0, None), |c| Transition::new(c.clip.length(), c.keys.attach));
    let away_t = put_away.as_ref().map_or(Transition::new(0.0, None), |c| Transition::new(c.clip.length(), c.keys.detach));
    let want_out = control.weapon_out && stance != Stance::Swim;
    let mut events = std::mem::take(&mut m.events);
    m.hands.update(want_out, dt, draw_t, away_t, &mut events);
    let a = &mut body.animator;
    let skel = a.skel.clone();
    let bone = |name: &str| skel.by_name.get(name).copied();
    for ev in &events {
        match ev {
            HandEvent::StartDraw => {
                if let Some(c) = &draw {
                    a.restart(&c.clip, TAG_EQUIP, 1.0, Mode::Once, c.fade());
                }
            }
            HandEvent::StartHolster => {
                if let Some(c) = &put_away {
                    a.restart(&c.clip, TAG_EQUIP, 1.0, Mode::Once, c.fade());
                }
            }
            HandEvent::ToHand => {
                a.cut(TAG_HOLSTER);
                body.weapon_parent = draw.as_ref().and_then(|c| c.keys.prn.as_ref()).and_then(|(_, b)| bone(b)).or_else(|| bone("Bip01 R Hand"));
            }
            HandEvent::ToHolster => {
                body.weapon_parent = None; // set from the holster clip below
            }
            HandEvent::Drawn | HandEvent::Holstered => {}
        }
    }
    m.events = events;
    if !m.hands.in_hand {
        // The holster clip's `prn:` names the holster bone [observed: `Bip01 Pelvis` for
        // pistols, `Bip01 Spine2` for rifles]; the unequip clip's says the same.
        let from_keys = holster.as_ref().and_then(|c| c.keys.prn.as_ref()).or_else(|| put_away.as_ref().and_then(|c| c.keys.prn.as_ref()));
        if let Some(b) = from_keys.and_then(|(_, b)| bone(b)) {
            body.weapon_parent = Some(b);
        }
    }

    // The frame's clips.
    let drawn = (m.hands.phase == WeaponPhase::Drawn).then_some(prefix);
    a.begin();
    if let Some(c) = get(ClipKey::Idle(stance)) {
        a.want(&c.clip, TAG_IDLE, 1.0, Mode::Loop, c.fade());
    }
    let dirs = direction_weights(forward, right);
    let mut expected_speed = 0.0;
    if m.airborne >= AIRBORNE_LOOP {
        let dir = dirs.map(|w| if w[0].1 >= w[1].1 { w[0].0 } else { w[1].0 });
        if let Some(c) = get(ClipKey::Jump(stance, drawn, dir, false)) {
            a.want(&c.clip, TAG_JUMP, 1.0, Mode::Loop, c.fade());
        }
    } else if let Some(w) = dirs {
        for (i, (dir, weight)) in w.into_iter().enumerate() {
            if weight <= 0.0 {
                continue;
            }
            if let Some(c) = get(ClipKey::Move(stance, body.female, fast, dir, drawn)) {
                a.want(&c.clip, TAG_MOVE + i as u32, weight, Mode::Synced, c.fade());
                expected_speed += weight * c.speed.unwrap_or(0.0);
            }
        }
    } else if m.turn_rate.abs() > TURN_RATE {
        // Heading grows clockwise: a negative rate turns left.
        if let Some(c) = get(ClipKey::Turn(stance, drawn, m.turn_rate < 0.0)) {
            a.want(&c.clip, TAG_TURN, 1.0, Mode::Loop, c.fade());
        }
    }
    if m.landing {
        m.landing = false;
        let dir = dirs.map(|w| if w[0].1 >= w[1].1 { w[0].0 } else { w[1].0 });
        if let Some(c) = get(ClipKey::Jump(stance, drawn, dir, true)) {
            a.restart(&c.clip, TAG_LAND, 1.0, Mode::Once, c.fade());
        }
    }
    if let Some(c) = a.layers.iter().find(|l| l.tag == TAG_LAND && !l.finished()).map(|l| l.clip.clone()) {
        a.want(&c, TAG_LAND, 1.0, Mode::Once, 0.0);
    }
    if m.hands.in_hand && m.hands.phase != WeaponPhase::Drawing {
        for (i, (aim, w)) in aim_weights(player.pitch).into_iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            if let Some(c) = get(ClipKey::Aim(stance, prefix, aim, control.aiming)) {
                a.want(&c.clip, TAG_AIM + i as u32, w, Mode::Loop, c.fade());
            }
        }
    }
    if !m.hands.in_hand {
        if let Some(c) = &holster {
            a.want(&c.clip, TAG_HOLSTER, 1.0, Mode::Once, 0.0);
        }
    }
    if matches!(m.hands.phase, WeaponPhase::Drawing | WeaponPhase::Holstering) {
        let c = if m.hands.phase == WeaponPhase::Drawing { &draw } else { &put_away };
        if let Some(c) = c {
            a.want(&c.clip, TAG_EQUIP, 1.0, Mode::Once, c.fade());
        }
    }
    a.end();
    // Movement clips play at the speed the character actually moves [hypothesis: the
    // game matches the clip's root motion to the ground speed].
    let rate = if expected_speed > 1.0 { (m.speed / expected_speed).clamp(0.25, 2.5) } else { 1.0 };
    a.advance(dt, rate);
    a.pose();
    a.apply(&mut transforms);

    // Place the body and give the eye.
    let root = Transform { translation: Vec3::from_array(feet), rotation: Quat::from_rotation_z(-player.heading), scale: Vec3::splat(body.scale) };
    if let Ok(mut t) = transforms.get_mut(body.root) {
        *t = root;
    }
    let root_m = Affine3A::from_scale_rotation_translation(root.scale, root.rotation, root.translation);
    if let Some(h) = body.head {
        let head = root_m * a.model_space(h);
        let mut eye = head.transform_point3(body.eye_local);
        if !m.hands.in_hand {
            let right = Vec3::new(player.heading.cos(), -player.heading.sin(), 0.0);
            let neck = Vec3::from(head.translation);
            eye = neck + Quat::from_axis_angle(right, NECK_PITCH * player.pitch) * (eye - neck);
        }
        player.body_eye = Some(eye.to_array());
        view.pivot = Some([feet[0], feet[1], eye.z + 5.0]);
    }
    // The weapon node's parent.
    if body.attached != body.weapon_parent {
        if let (Some(w), Some(p)) = (body.weapon_node, body.weapon_parent) {
            commands.entity(a.nodes[w]).insert(ChildOf(a.nodes[p]));
            debug!("player weapon: now on {:?}", skel.names[p]);
            body.attached = Some(p);
        }
    }
    // Hide the head from inside it.
    let hide = control.pov == Pov::First;
    if body.hidden != Some(hide) {
        for e in &body.head_entities {
            if let Ok(mut v) = visibility.get_mut(*e) {
                *v = if hide { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
        body.hidden = Some(hide);
    }
    m.log_elapsed += dt;
    if m.log_elapsed >= 1.0 {
        m.log_elapsed = 0.0;
        let names: Vec<String> = a
            .layers
            .iter()
            .filter(|l| l.weight > 0.01)
            .map(|l| format!("{}:{}×{:.2}", l.tag, l.clip.name, l.weight))
            .collect();
        debug!(
            "player body: {stance:?} speed {:.0} (clip rate {rate:.2}) weapon {:?} {} in hand {} airborne {:.2}s eye z {:.1} (feet {:.1}) layers [{}]",
            m.speed,
            m.hands.phase,
            m.prefix,
            m.hands.in_hand,
            m.airborne,
            player.body_eye.map_or(f32::NAN, |e| e[2]),
            feet[2],
            names.join(" ")
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
    fn default_eye_is_in_front_of_and_above_the_head() {
        let skel = PreparedSkeleton { bind_world: vec![Affine3A::from_translation(Vec3::new(0.0, 0.0, 110.0))], ..Default::default() };
        assert_eq!(default_eye(&skel, 0), Vec3::new(0.0, 9.0, 7.0));
    }
}
