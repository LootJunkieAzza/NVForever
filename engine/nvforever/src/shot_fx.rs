//! What is seen of the player's shots (docs/spec-notes/combat-rules.md §15): the PROJ's
//! muzzle flash at the weapon's muzzle and the PROJ's model on every projectile in
//! flight. Combat (`nvf_play::combat`) says what was fired ([`ShotFired`]) and where
//! each projectile is drawn ([`ShotVisuals`]); the models and the light come from here.
//!
//! - **Muzzle flash.** A PROJ with the Muzzle Flash flag has a flash model (`NAM1`), a
//!   duration and a light (LIGH) [documented: xEdit PROJ, GECK wiki Projectile]. The
//!   model is spawned under the weapon model's `ProjectileNode` in the rig in view
//!   (`PlayerBody::muzzle_entity`), so it sits at the muzzle, points the way the node
//!   does and moves with the gun [hypothesis: that the game attaches it there, from the
//!   node's name, its place and its y axis lying along the barrel as the flash models'
//!   length does; animation.md §9.9]. It stays for the PROJ's duration and no longer
//!   (0.04 s for the bullet PROJs), but always for the frame it is made in, so a flash
//!   shorter than a frame is still seen once; the light with it. Another actor's flash
//!   goes under its drawn weapon's muzzle marker (`weapons::Muzzle`, the same node of
//!   its weapon model, at rest) the same way; without one (a weapon model with no
//!   `ProjectileNode`) it stands where the shot left, its +y turned along the shot
//!   [design]. Creatures fire no guns yet (`npc_combat`: their attacks are blows).
//! - **Projectiles.** The PROJ's model is drawn at each [`ShotVisuals`] entry with its
//!   +y along the flight [observed: the models are built along +y; hypothesis: that it
//!   is their forward axis]. A beam's model is stretched along y from the muzzle to its
//!   end by the place of its `BeamEnd` node [hypothesis: the node's meaning, from its
//!   name and its place at the far end of the beam's geometry].
//! - **Billboards.** A `NiBillboardNode` of these models turns to the camera each
//!   frame: about its own y axis for the "rotate about up" modes (1, 5, 9), freely for
//!   the others [documented: nif.xml BillboardMode names; hypothesis: that the node's +z
//!   is the side shown and +y the "up" it turns about].
//! - **Add-on nodes.** A `BSValueNode` of these models becomes an emitter of its ADDN's
//!   particle effect for as long as the model is there (`crate::particles`): the sparks
//!   and smoke of a flash, a missile's trail, the flamer's flame.
//! - **Impacts.** A shot that lands ([`ImpactFx`]) shows its IPCT's model at the point
//!   for the IPCT's effect duration, its +z along the IPCT's orientation: the surface's
//!   normal, the projectile's direction or its reflection [documented: the IPCT fields;
//!   hypothesis: that +z is the axis turned, from the impact models' add-on nodes
//!   standing along +z].
//! - **Shell casings.** [`CasingFx`]: the WEAP's casing model, falling and bouncing; a
//!   bounce from at least [`CASING_KNOCK`] units/s plays the casing's PHY sound
//!   ([`casing_sound`]) where it lands, up to [`CASING_SOUNDS`] times a casing [design].
//! - **Explosions.** [`ExplosionFx`]: the EXPL's model at the point, upright, emitting
//!   its own particle systems (flash, sparks, smoke, shrapnel: `crate::particles::
//!   ModelEmitter`) for as long as its sequence runs (else [`EXPLOSION_SECS`]), and the
//!   EXPL's light (LIGH) fading out over [`EXPLOSION_LIGHT_SECS`] [design: the LIGH's
//!   time is -1, no duration of its own]. The dirt thrown up is the impact of the EXPL's
//!   data set on the ground below (`nvf_play::combat`), shown as any impact. Seen from
//!   within the EXPL's image-space radius, its image space modifier plays on the screen
//!   (`crate::imagespace::ScreenModifiers::play`).
//! - **The models' own animation** (docs/spec-notes/explosion-effects.md): a flash,
//!   impact, explosion or projectile model plays its effect clip from when it appears
//!   (`nvf_assets::model::Rig::effect`): its `SpecialIdle_AreaEffect` sequence and the
//!   controllers no sequence drives, so fireballs and dust splashes grow, move, fade and
//!   show and hide as their files say ([`spawn_effect`]), and the energy beams scroll
//!   their textures along and across themselves (combat-rules.md §15).
//! - **Beam fade.** A beam is drawn at [`ShotVisual::strength`]: the alpha of every
//!   shape of its model (and with it all it adds, the beams being additive) falls from
//!   full to nothing over the PROJ's Fade Duration (`nvf_play::combat::beam_strength`),
//!   unless the model fades itself with an alpha controller (the green laser's, the
//!   pulse gun's) [hypothesis, combat-rules.md §15]. A beam model with no controllers
//!   at all (`DLC03ScatterBeam`'s) has no rig and is not faded.
//! - Not built: impact decals, the models' refraction (heat haze) controllers.

use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_nif::anim::BoolTrack;
use nvf_assets::anim_play::{MaterialTarget, Playing};
use nvf_assets::material_anim::RigMaterials;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::combat::{CasingFx, Combat, ExplosionFx, ImpactFx, ShotFired, ShotVisual, ShotVisuals};
use nvf_play::phys::PhysState;
use fnv_sim::FormId;
use nvf_play::sfx::{PlaySound, Sfx};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::player_body::PlayerBody;
use crate::weapons::{HeldWeapon, Muzzle};
use crate::world::{WorldData, WorldRoot};

/// A `NiBillboardNode`: its billboard mode and its rotation in the file.
#[derive(Component)]
struct Billboard {
    mode: u16,
    bind: Quat,
}

/// A shell casing in the air or on the ground: its velocity and spin (game space) and
/// how long it has left.
#[derive(Component)]
struct Casing {
    velocity: Vec3,
    spin: Vec3,
    left: f32,
    resting: bool,
    /// Its knock (a PHYCasing SOUN) and how many times it has sounded.
    sound: Option<FormId>,
    knocks: u8,
}

/// The speed at which a casing's bounce is heard, units/s, and how many bounces of one
/// casing are [design].
pub const CASING_KNOCK: f32 = 60.0;
pub const CASING_SOUNDS: u8 = 3;

/// The PHY sound (SOUN editor ID) of a shell casing by its model's name [hypothesis: by
/// the names; FalloutNV.esm has `PHYCasingPistol`, `PHYCasingRifle` and
/// `PHYCasingShotgun` (files `fx\phy\weapon\casing_<kind>\`) and no record links them to
/// a weapon or a casing model, audio.md §14].
pub fn casing_sound(model: &str) -> &'static str {
    let m = model.to_ascii_lowercase();
    if m.contains("shotgun") {
        "PHYCasingShotgun"
    } else if ["45cal", "357", "44magnum", "10mm", "9mm", "pistol", "eccell"].iter().any(|k| m.contains(k)) {
        "PHYCasingPistol"
    } else {
        "PHYCasingRifle"
    }
}

/// The share of its speed a casing keeps when it bounces, and the speed below which it
/// stays down, units per second [design].
const CASING_BOUNCE: f32 = 0.35;
const CASING_REST: f32 = 25.0;

/// A muzzle flash or an impact effect and how long it has left.
#[derive(Component)]
struct Flash {
    left: f32,
}

/// The nodes an effect model's clip shows and hides (`NiVisController` tracks), on the
/// model's root with its [`Playing`].
#[derive(Component)]
struct EffectVisibility(Vec<(Entity, BoolTrack)>);

/// How long an explosion's model stays and emits when it has no sequence, and how long
/// its light takes to fade, seconds [design: about the grenade explosion's sequence,
/// 5.03 s, and its flash's emitter, on for 0.33 s]. A model with a sequence stays for its
/// length [hypothesis: the game removes the effect when its sequence has run].
pub const EXPLOSION_SECS: f32 = 5.0;
pub const EXPLOSION_LIGHT_SECS: f32 = 0.6;

/// An explosion's light: its full intensity and how long it has been fading.
#[derive(Component)]
struct FadingLight {
    full: f32,
    age: f32,
}

/// The systems that put shot effects in the world; particles are emitted after them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShotFxSet;

#[derive(Resource, Default)]
struct ShotFx {
    /// The model drawn for each projectile in flight (`ShotVisual::id`); `None`: it has
    /// none to draw. With the y its model's `BeamEnd` node is at, if it has one, and how
    /// far the model reaches behind its origin.
    flying: HashMap<u64, Option<(Entity, Option<f32>, f32)>>,
    /// Model paths that were reported.
    reported: HashSet<String>,
    flashes: u32,
    lights: u32,
    /// Other actors' flashes: under their weapon's muzzle marker, and where the shot
    /// left (no marker).
    actor_flashes_muzzle: u32,
    actor_flashes_at_shot: u32,
    /// Weapons drawn on other actors whose flash and projectile models were loaded.
    actor_weapons_loaded: HashSet<fnv_sim::FormId>,
    projectiles: u32,
    no_model: u32,
    casings: u32,
    casing_knocks: u32,
    impacts: u32,
    explosions: u32,
    /// Effect models spawned playing their own clip, and of those, explosions whose
    /// image space modifier went on the screen.
    animated: u32,
    screen_effects: u32,
    /// Beams that played a clip of their own, and those drawn fading.
    beams_animated: u32,
    beams_faded: u32,
}

pub struct ShotFxPlugin;

impl Plugin for ShotFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShotFx>()
            .add_systems(Update, (preload, flash, draw_flying, casings, impacts, explosions, face_camera, effect_visibility).chain().in_set(ShotFxSet).after(nvf_play::PlaySet::Activation).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

fn local(a: &Affine3A) -> Transform {
    Transform::from_matrix(Mat4::from(*a))
}

/// Spawn `pm` under `parent` at `place` as its node tree, so that its billboards can
/// turn; returns the entity above it.
fn spawn_model(commands: &mut Commands, parent: Entity, pm: &PreparedModel, place: Transform) -> Entity {
    let holder = commands.spawn((place, Visibility::Inherited, ChildOf(parent))).id();
    let nodes: Vec<Entity> = pm
        .nodes
        .iter()
        .map(|n| {
            let t = local(&n.local);
            let mut e = commands.spawn((t, Visibility::Inherited));
            if let Some(mode) = n.billboard {
                e.insert(Billboard { mode, bind: t.rotation });
            }
            if let Some(index) = n.addon {
                e.insert(crate::particles::AddonEmitter::new(index));
            }
            e.id()
        })
        .collect();
    for (n, e) in pm.nodes.iter().zip(&nodes) {
        commands.entity(n.parent.map_or(holder, |p| nodes[p])).add_child(*e);
    }
    for s in &pm.shapes {
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), local(&s.own), Visibility::Inherited, NotShadowCaster)).id();
        commands.entity(s.node.map_or(holder, |n| nodes[n])).add_child(e);
    }
    holder
}

/// Spawn an effect model under `parent` at `place`: its rig playing its effect clip when
/// something in the file moves (`Rig::effect`), else its node tree ([`spawn_model`]).
/// Its billboards turn and its add-on nodes emit either way. Returns the entity above it
/// and the length of its clip in seconds (0: none). `beam`: a beam, which fades with its
/// [`ShotVisual::strength`] ([`Fade`]).
fn spawn_effect(commands: &mut Commands, fx: &mut ShotFx, parent: Entity, pm: &PreparedModel, place: Transform, source: &str, beam: bool) -> (Entity, f32) {
    let Some((rig, clip)) = pm.rig.as_ref().and_then(|r| Some((r, r.effect.as_ref()?))) else {
        return (spawn_model(commands, parent, pm, place), 0.0);
    };
    // Under a holder at the place: a track of the clip may drive the file's root node.
    let holder = commands.spawn((place, Visibility::Inherited, ChildOf(parent))).id();
    let (root, drawn, by_name) = rig.spawn_playing(commands, holder, Transform::IDENTITY, source, 0.0, Some(clip));
    for e in &drawn {
        commands.entity(*e).insert(NotShadowCaster);
    }
    for n in &pm.nodes {
        let Some(&e) = n.name.as_ref().and_then(|k| by_name.get(k)) else { continue };
        if let Some(mode) = n.billboard {
            commands.entity(e).insert(Billboard { mode, bind: local(&n.local).rotation });
        }
        if let Some(index) = n.addon {
            commands.entity(e).insert(crate::particles::AddonEmitter::new(index));
        }
    }
    let shown: Vec<(Entity, BoolTrack)> = clip.visibility.iter().filter_map(|(n, _, t)| Some((*by_name.get(n)?, t.clone()))).collect();
    if !shown.is_empty() {
        commands.entity(root).insert(EffectVisibility(shown));
    }
    if beam {
        // Every shape gets a material of its own to fade, not only those the clip drives
        // (the laser beam's white core is not).
        let fades_itself = clip.materials.iter().any(|t| t.target == MaterialTarget::Alpha);
        if !fades_itself {
            let slots = rig.shapes.iter().zip(&drawn).map(|(s, e)| s.slot(*e)).collect();
            commands.entity(root).insert(RigMaterials { slots, idle: Arc::new(clip.materials.clone()), dark: false, fade: 1.0 });
        }
        commands.entity(holder).insert(Fade { root, own: fades_itself });
        fx.beams_animated += 1;
    }
    fx.animated += 1;
    (holder, clip.length() / clip.frequency.max(1e-6))
}

/// On a beam's holder: the root of its rig, whose materials fade with the beam's
/// strength, unless `own`: its model fades itself (an alpha track of its clip).
#[derive(Component)]
struct Fade {
    root: Entity,
    own: bool,
}

/// Draw a beam at its strength (see the module notes); the beams that faded are counted
/// once each.
fn fade_beam(fx: &mut ShotFx, m: &mut RigMaterials, v: &ShotVisual) {
    if m.fade >= 1.0 && v.strength < 1.0 {
        fx.beams_faded += 1;
    }
    if m.fade != v.strength {
        m.fade = v.strength;
    }
}

/// Show and hide the nodes of effect models as their clips' visibility tracks say.
fn effect_visibility(effects: Query<(&Playing, &EffectVisibility)>, mut visibility: Query<&mut Visibility>) {
    for (p, ev) in &effects {
        let t = p.sequence_time();
        for (e, track) in &ev.0 {
            let want = match track.at(t) {
                Some(true) => Visibility::Inherited,
                Some(false) => Visibility::Hidden,
                None => continue,
            };
            if let Ok(mut v) = visibility.get_mut(*e) {
                if *v != want {
                    *v = want;
                }
            }
        }
    }
}

fn load(models: &mut ModelCtx, fx: &mut ShotFx, what: &str, model: &str) -> Option<Arc<PreparedModel>> {
    let path = fnv_actor::resolve::mesh_path(model);
    let pm = models.model(&path);
    if fx.reported.insert(path.clone()) {
        match &pm {
            Some(pm) => info!(
                "shot fx: {what} {path}: {} shapes, {} nodes, {} billboards{}",
                pm.shapes.len(),
                pm.nodes.len(),
                pm.nodes.iter().filter(|n| n.billboard.is_some()).count(),
                match pm.rig.as_ref().and_then(|r| r.effect.as_ref()) {
                    Some(c) => format!(
                        "; plays {} ({:.2} s: {} node track(s), {} visibility, {} material)",
                        c.name,
                        c.length(),
                        c.tracks.len(),
                        c.visibility.len(),
                        c.materials.len()
                    ),
                    None => String::new(),
                }
            ),
            None => warn!("shot fx: {what} {path}: model not loaded"),
        }
    }
    pm
}

/// Load the flash and projectile models of the weapon in use before its first shot: a
/// model first asked for in the frame it is drawn in can miss that frame, which is the
/// whole of a 0.04 s flash. The same for each weapon drawn on another actor, once per
/// weapon: its default ammo's.
fn preload(combat: Res<Combat>, mut models: ModelCtx, mut fx: ResMut<ShotFx>, mut last: Local<Option<fnv_sim::FormId>>, drawn: Query<&HeldWeapon, Added<HeldWeapon>>) {
    let fx = &mut *fx;
    let mut models_of = |fx: &mut ShotFx, id: fnv_sim::FormId| {
        let Some(p) = combat.forms.projectiles.get(&id) else { return };
        if let Some(m) = p.muzzle_flash_model.as_deref().filter(|_| p.has("Muzzle Flash")) {
            load(&mut models, fx, "muzzle flash", m);
        }
        if let Some(m) = p.model.as_deref() {
            load(&mut models, fx, "projectile", m);
        }
    };
    for h in &drawn {
        if fx.actor_weapons_loaded.insert(h.weapon) {
            if let Some(p) = combat.forms.chain(h.weapon, None).and_then(|c| c.projectile) {
                models_of(fx, p);
            }
        }
    }
    let now = combat.projectile();
    if *last == now {
        return;
    }
    *last = now;
    if let Some(p) = now {
        models_of(fx, p);
    }
}

/// Muzzle flashes: one for each shot fired, gone after their time.
#[allow(clippy::too_many_arguments)]
fn flash(
    mut commands: Commands,
    time: Res<Time>,
    mut fired: MessageReader<ShotFired>,
    combat: Res<Combat>,
    body: Option<Res<PlayerBody>>,
    world: Option<Res<WorldData>>,
    mut models: ModelCtx,
    mut fx: ResMut<ShotFx>,
    mut flashes: Query<(Entity, &mut Flash)>,
    root: Query<Entity, With<WorldRoot>>,
    muzzles: Query<(Entity, &Muzzle, &GlobalTransform)>,
    mut told: Local<HashSet<fnv_sim::FormId>>,
) {
    let dt = time.delta_secs();
    for (e, mut f) in &mut flashes {
        f.left -= dt;
        if f.left <= 0.0 {
            commands.entity(e).try_despawn();
        }
    }
    for shot in fired.read() {
        let Some(p) = combat.forms.projectiles.get(&shot.projectile) else { continue };
        let (true, Some(model)) = (p.has("Muzzle Flash"), p.muzzle_flash_model.as_deref()) else { continue };
        // The player's at the held weapon's muzzle (no body, no `ProjectileNode`: no
        // flash); another actor's at its weapon's muzzle marker, else where its shot left
        // (game space, under the world root), +y along the shot (module notes).
        let player = shot.shooter == fnv_sim::FormId::PLAYER_REF;
        let marker = (!player).then(|| muzzles.iter().find(|(_, m, _)| m.reference == shot.shooter.0)).flatten();
        let place = if player {
            body.as_deref().and_then(PlayerBody::muzzle_entity).map(|m| (m, Transform::IDENTITY))
        } else if let Some((m, _, _)) = marker {
            Some((m, Transform::IDENTITY))
        } else {
            let along = |d: [f32; 3]| Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(d).normalize_or(Vec3::Y));
            shot.at.zip(root.single().ok()).map(|(at, r)| (r, Transform::from_translation(Vec3::from_array(at)).with_rotation(shot.dir.map_or(Quat::IDENTITY, along))))
        };
        let Some((parent, place)) = place else { continue };
        let Some(pm) = load(&mut models, &mut fx, "muzzle flash", model) else { continue };
        let (holder, _) = spawn_effect(&mut commands, &mut fx, parent, &pm, place, model, false);
        // Made after this frame's flashes were aged: it is drawn this frame whatever
        // its duration, and goes in the first frame its time is up.
        commands.entity(holder).insert(Flash { left: p.muzzle_flash_duration });
        fx.flashes += 1;
        match (player, marker) {
            (true, _) => {}
            (false, Some(_)) => fx.actor_flashes_muzzle += 1,
            (false, None) => fx.actor_flashes_at_shot += 1,
        }
        // Once an actor: how near the flash is to where the shot left and how far its
        // length (the marker's +y, game space as the world root's turn alone) is turned
        // from the shot (the marker as last frame left it).
        if let (Some((_, _, g)), Some(at), Some(dir), true) = (marker, shot.at, shot.dir, told.insert(shot.shooter)) {
            let off = crate::world::bevy_to_game(g.translation()).distance(Vec3::from_array(at));
            let angle = crate::world::bevy_to_game(g.affine().matrix3.y_axis.into()).normalize_or_zero().angle_between(Vec3::from_array(dir).normalize_or(Vec3::Y)).to_degrees();
            info!("shot fx: {} muzzle flash at its weapon's muzzle, {off:.1} units from where the shot left, its length {angle:.0} degrees off the shot", shot.shooter);
        }
        // The flash's light: the LIGH's colour, brightness and radius.
        let light = p.muzzle_flash_light.zip(world.as_deref()).and_then(|(id, w)| fnv_world::lighting::light_data(w.lo, w.schema, id.0));
        if let Some(l) = light.filter(|l| l.radius > 0.0) {
            let lin = l.color.map(fnv_world::sky::srgb_to_linear);
            let color = Color::LinearRgba(LinearRgba::rgb(lin[0], lin[1], lin[2]));
            let intensity = crate::lights::lumens(l.fade.max(0.0), crate::sky::light_unit());
            commands.entity(holder).with_child((PointLight { color, intensity, range: l.radius, shadow_maps_enabled: false, ..default() }, Transform::IDENTITY));
            fx.lights += 1;
        }
    }
}

/// The projectiles in flight: each one's PROJ model where combat says it is drawn.
#[allow(clippy::too_many_arguments)]
fn draw_flying(
    mut commands: Commands,
    visuals: Res<ShotVisuals>,
    combat: Res<Combat>,
    mut fx: ResMut<ShotFx>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    mut transforms: Query<&mut Transform>,
    fading: Query<&Fade>,
    mut materials: Query<&mut RigMaterials>,
) {
    let Ok(world_root) = root.single() else { return };
    let fx = &mut *fx;
    for v in &visuals.flying {
        let dir = Vec3::from_array(v.dir);
        let known = fx.flying.get(&v.id).copied();
        let beam_end = known.flatten().and_then(|k| k.1);
        // A model is not shown until it has flown its own length behind it: a tracer's
        // streak (220 units behind the bullet) would otherwise run back through the gun
        // and the shooter in its first frame [design].
        let out = |tail: f32| if v.beam.is_some() || v.travelled >= tail { Visibility::Inherited } else { Visibility::Hidden };
        let place = |beam_end: Option<f32>| Transform {
            translation: Vec3::from_array(v.pos),
            rotation: Quat::from_rotation_arc(Vec3::Y, dir.normalize_or(Vec3::Y)),
            scale: match (v.beam, beam_end) {
                (Some(len), Some(end)) if end > 0.0 => Vec3::new(1.0, len / end, 1.0),
                _ => Vec3::ONE,
            },
        };
        match known {
            Some(Some((e, _, tail))) => {
                if let Ok(mut t) = transforms.get_mut(e) {
                    *t = place(beam_end);
                }
                commands.entity(e).try_insert(out(tail));
                if let Some(mut m) = fading.get(e).ok().filter(|f| !f.own).and_then(|f| materials.get_mut(f.root).ok()) {
                    fade_beam(fx, &mut m, v);
                }
            }
            Some(None) => {}
            None => {
                let p = combat.forms.projectiles.get(&v.projectile);
                let model = p.and_then(|p| p.model.as_deref());
                let pm = model.and_then(|m| load(&mut models, fx, "projectile", m));
                let spawned = pm.map(|pm| {
                    let end = pm.nodes.iter().find(|n| n.name.as_deref().is_some_and(|x| x.eq_ignore_ascii_case("BeamEnd"))).map(|n| n.local.translation.y);
                    debug!(
                        "shot fx: {} drawn at ({:.1}, {:.1}, {:.1}){}",
                        p.and_then(|p| p.edid.as_deref()).unwrap_or("?"),
                        v.pos[0],
                        v.pos[1],
                        v.pos[2],
                        v.beam.map_or(String::new(), |l| format!(", a beam {l:.0} units long"))
                    );
                    let tail = pm.bounds.map_or(0.0, |(lo, _)| (-lo.y).max(0.0));
                    let (e, _) = spawn_effect(&mut commands, fx, world_root, &pm, place(end), model.unwrap_or_default(), v.beam.is_some());
                    commands.entity(e).insert(out(tail));
                    (e, end, tail)
                });
                match spawned {
                    Some(_) => fx.projectiles += 1,
                    None => fx.no_model += 1,
                }
                fx.flying.insert(v.id, spawned);
            }
        }
    }
    fx.flying.retain(|id, drawn| {
        let keep = visuals.flying.iter().any(|v| v.id == *id);
        if let (false, Some((e, _, _))) = (keep, drawn) {
            commands.entity(*e).try_despawn();
        }
        keep
    });
}

/// Shell casings: the WEAP's casing model thrown from the `ShellCasingNode`, falling
/// under the world's gravity, bouncing off what it meets and gone after its time.
#[allow(clippy::too_many_arguments)]
fn casings(
    mut commands: Commands,
    time: Res<Time>,
    mut thrown: MessageReader<CasingFx>,
    sfx: Option<Res<Sfx>>,
    mut phys: ResMut<PhysState>,
    mut models: ModelCtx,
    mut fx: ResMut<ShotFx>,
    root: Query<Entity, With<WorldRoot>>,
    mut live: Query<(Entity, &mut Casing, &mut Transform)>,
    mut knocks: MessageWriter<PlaySound>,
) {
    let dt = time.delta_secs();
    let gravity = phys.world.params.gravity;
    for (e, mut c, mut t) in &mut live {
        c.left -= dt;
        if c.left <= 0.0 {
            commands.entity(e).try_despawn();
            continue;
        }
        if c.resting {
            continue;
        }
        c.velocity.z -= gravity * dt;
        let step = c.velocity * dt;
        let len = step.length();
        let hit = (len > 1e-4).then(|| phys.ray(t.translation.to_array(), step.to_array(), len, Default::default())).flatten();
        match hit {
            Some(h) => {
                let n = Vec3::from_array(h.normal);
                t.translation = Vec3::from_array(h.point) + n * 0.5;
                if let (Some(sound), true) = (c.sound, c.velocity.length() >= CASING_KNOCK && c.knocks < CASING_SOUNDS) {
                    c.knocks += 1;
                    fx.casing_knocks += 1;
                    knocks.write(PlaySound { sound, at: Some(h.point) });
                }
                c.velocity = (c.velocity - 2.0 * c.velocity.dot(n) * n) * CASING_BOUNCE;
                c.spin *= CASING_BOUNCE;
                c.resting = c.velocity.length() < CASING_REST;
            }
            None => t.translation += step,
        }
        if !c.resting {
            t.rotation = Quat::from_scaled_axis(c.spin * dt) * t.rotation;
        }
    }
    let (Some(sfx), Ok(world_root)) = (sfx, root.single()) else {
        thrown.clear();
        return;
    };
    for c in thrown.read() {
        let Some(model) = sfx.forms.weapons.get(&c.weapon).and_then(|w| w.shell_casing_model.as_deref()) else { continue };
        let Some(pm) = load(&mut models, &mut fx, "shell casing", model) else { continue };
        let velocity = Vec3::from_array(c.velocity);
        // It tumbles about an axis across its flight [design].
        let spin = velocity.cross(Vec3::Z).normalize_or(Vec3::X) * 12.0;
        let holder = spawn_model(&mut commands, world_root, &pm, Transform::from_translation(Vec3::from_array(c.point)));
        let sound = sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref() == Some(casing_sound(model))).map(|(&id, _)| id);
        commands.entity(holder).insert(Casing { velocity, spin, left: c.life.max(0.5), resting: false, sound, knocks: 0 });
        fx.casings += 1;
    }
}

/// Impact effects: the IPCT's model where a shot landed, for the IPCT's duration.
fn impacts(
    mut commands: Commands,
    mut landed: MessageReader<ImpactFx>,
    sfx: Option<Res<Sfx>>,
    mut models: ModelCtx,
    mut fx: ResMut<ShotFx>,
    root: Query<Entity, With<WorldRoot>>,
) {
    let (Some(sfx), Ok(world_root)) = (sfx, root.single()) else {
        landed.clear();
        return;
    };
    for hit in landed.read() {
        let Some(i) = sfx.forms.impacts.get(&hit.impact) else { continue };
        let Some((model, pm)) = i.model.as_deref().and_then(|m| Some((m, load(&mut models, &mut fx, "impact", m)?))) else { continue };
        let (normal, dir) = (Vec3::from_array(hit.normal).normalize_or(Vec3::Z), Vec3::from_array(hit.dir).normalize_or(Vec3::NEG_Z));
        let along = match i.orientation.as_deref() {
            Some("Projectile Vector") => dir,
            Some("Projectile Reflection") => (dir - 2.0 * dir.dot(normal) * normal).normalize_or(normal),
            _ => normal,
        };
        let place = Transform { translation: Vec3::from_array(hit.point), rotation: Quat::from_rotation_arc(Vec3::Z, along), scale: Vec3::ONE };
        let (holder, _) = spawn_effect(&mut commands, &mut fx, world_root, &pm, place, model, false);
        // Its own particle systems, if any (the debris of an explosion's dirt impact).
        commands.entity(holder).insert((Flash { left: i.effect_duration }, crate::particles::ModelEmitter::new(fnv_actor::resolve::mesh_path(model))));
        fx.impacts += 1;
    }
}

/// Explosions: the EXPL's model playing its sequence and emitting its particle systems,
/// its light fading, and its image space modifier for an eye within its radius.
#[allow(clippy::too_many_arguments)]
fn explosions(
    mut commands: Commands,
    time: Res<Time>,
    mut boom: MessageReader<ExplosionFx>,
    (sfx, combat, mut screen): (Option<Res<Sfx>>, Res<Combat>, Option<ResMut<crate::imagespace::ScreenModifiers>>),
    world: Option<Res<WorldData>>,
    mut models: ModelCtx,
    mut fx: ResMut<ShotFx>,
    root: Query<(Entity, &Transform), With<WorldRoot>>,
    camera: Query<&GlobalTransform, With<nvf_play::PlayerCamera>>,
    mut lights: Query<(Entity, &mut FadingLight, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    for (e, mut f, mut l) in &mut lights {
        f.age += dt;
        let k = 1.0 - f.age / EXPLOSION_LIGHT_SECS;
        if k <= 0.0 {
            commands.entity(e).try_despawn();
        } else {
            l.intensity = f.full * k;
        }
    }
    let (Some(sfx), Ok((world_root, root_place))) = (sfx, root.single()) else {
        boom.clear();
        return;
    };
    // The eye in game space.
    let eye = camera.single().ok().map(|c| root_place.compute_affine().inverse().transform_point3(c.translation()));
    for b in boom.read() {
        let Some(x) = sfx.forms.explosions.get(&b.explosion) else { continue };
        let place = Transform::from_translation(Vec3::from_array(b.point));
        let (holder, length) = match x.model.as_deref().and_then(|m| load(&mut models, &mut fx, "explosion", m).map(|pm| (m, pm))) {
            Some((m, pm)) => {
                let (h, length) = spawn_effect(&mut commands, &mut fx, world_root, &pm, place, m, false);
                commands.entity(h).insert(crate::particles::ModelEmitter::new(fnv_actor::resolve::mesh_path(m)));
                (h, length)
            }
            None => (commands.spawn((place, Visibility::Inherited, ChildOf(world_root))).id(), 0.0),
        };
        commands.entity(holder).insert(Flash { left: if length > 0.0 { length } else { EXPLOSION_SECS } });
        fx.explosions += 1;
        // Its image space modifier, seen from within its image-space radius ("IS Radius:
        // Radius of the area that will see the ImageSpace effect" [documented: GECK wiki
        // Explosion]), at full strength [hypothesis: the IMAD's keys have no distance in
        // them; nothing says the effect weakens towards the edge].
        let stats = combat.forms.explosions.get(&b.explosion);
        if let (Some(s), Some(eye), Some(screen)) = (stats, eye, screen.as_deref_mut()) {
            if let Some(imod) = s.image_space_modifier.filter(|_| eye.distance(Vec3::from_array(b.point)) <= s.is_radius) {
                screen.play(imod.0, time.elapsed_secs());
                fx.screen_effects += 1;
            }
        }
        let light = x.light.zip(world.as_deref()).and_then(|(id, w)| fnv_world::lighting::light_data(w.lo, w.schema, id.0));
        if let Some(l) = light.filter(|l| l.radius > 0.0) {
            let lin = l.color.map(fnv_world::sky::srgb_to_linear);
            let color = Color::LinearRgba(LinearRgba::rgb(lin[0], lin[1], lin[2]));
            let intensity = crate::lights::lumens(l.fade.max(0.0), crate::sky::light_unit());
            // Above the point, so the ground it went off on is lit [design].
            commands.spawn((
                PointLight { color, intensity, range: l.radius, shadow_maps_enabled: false, ..default() },
                FadingLight { full: intensity, age: 0.0 },
                Transform::from_translation(Vec3::from_array(b.point) + Vec3::Z * 32.0),
                ChildOf(world_root),
            ));
        }
    }
}

/// Turn the billboards of the effect models to the camera (see the module notes).
fn face_camera(
    camera: Query<Entity, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    boards: Query<(Entity, &Billboard, &ChildOf)>,
    parents: Query<&ChildOf>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok(cam) = camera.single() else { return };
    // An entity's place in the world from this frame's transforms: the propagated ones
    // are a frame old (the camera has moved since) and missing for what was spawned
    // this frame.
    let world = |transforms: &Query<&mut Transform>, e: Entity| {
        let mut m = Affine3A::IDENTITY;
        let mut at = Some(e);
        while let Some(p) = at {
            if let Ok(t) = transforms.get(p) {
                m = t.compute_affine() * m;
            }
            at = parents.get(p).ok().map(ChildOf::parent);
        }
        m
    };
    let eye = Vec3::from(world(&transforms, cam).translation);
    for (e, b, parent) in &boards {
        let m = world(&transforms, parent.parent());
        let Ok(mut t) = transforms.get_mut(e) else { continue };
        // The way to the camera in the node's own frame as the file has it.
        let d = b.bind.inverse() * (m.inverse().transform_point3(eye) - t.translation);
        let turn = match b.mode {
            1 | 5 | 9 => Quat::from_rotation_y(d.x.atan2(d.z)),
            _ => Quat::from_rotation_arc(Vec3::Z, d.normalize_or(Vec3::Z)),
        };
        let rotation = b.bind * turn;
        if t.rotation != rotation {
            t.rotation = rotation;
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, fx: Res<ShotFx>) {
    if exit.read().last().is_some() && fx.flashes + fx.projectiles + fx.no_model > 0 {
        info!(
            "shot fx: {} muzzle flashes ({} with a light), {} projectile models drawn, {} projectiles without a model, {} shell casings, {} impact effects, {} explosions, {} casing knocks heard",
            fx.flashes, fx.lights, fx.projectiles, fx.no_model, fx.casings, fx.impacts, fx.explosions, fx.casing_knocks
        );
        info!("shot fx: {} effect models played their own animation, {} explosion screen effects", fx.animated, fx.screen_effects);
        if fx.actor_flashes_muzzle + fx.actor_flashes_at_shot > 0 {
            info!("shot fx: other actors' muzzle flashes: {} at their weapon's muzzle, {} where the shot left (no muzzle node)", fx.actor_flashes_muzzle, fx.actor_flashes_at_shot);
        }
        if fx.beams_animated > 0 {
            info!("shot fx: {} beams played their own animation, {} faded out over their fade duration", fx.beams_animated, fx.beams_faded);
        }
    }
}
