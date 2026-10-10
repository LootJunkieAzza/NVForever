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
//!   shorter than a frame is still seen once; the light with it.
//! - **Projectiles.** The PROJ's model is drawn at each [`ShotVisuals`] entry with its
//!   +y along the flight [observed: the models are built along +y; hypothesis: that it
//!   is their forward axis]. A beam's model is stretched along y from the muzzle to its
//!   end by the place of its `BeamEnd` node [hypothesis: the node's meaning, from its
//!   name and its place at the far end of the beam's geometry].
//! - **Billboards.** A `NiBillboardNode` of these models turns to the camera each
//!   frame: about its own y axis for the "rotate about up" modes (1, 5, 9), freely for
//!   the others [documented: nif.xml BillboardMode names; hypothesis: that the node's +z
//!   is the side shown and +y the "up" it turns about].
//! - Not built: the add-on nodes of these models (`BSValueNode`: particle effects), the
//!   controllers inside the flash models, sounds, impact effects, NPCs' shots.

use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::prelude::*;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::combat::{Combat, ShotFired, ShotVisuals};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::player_body::PlayerBody;
use crate::world::{WorldData, WorldRoot};

/// A `NiBillboardNode`: its billboard mode and its rotation in the file.
#[derive(Component)]
struct Billboard {
    mode: u16,
    bind: Quat,
}

/// A muzzle flash and how long it has left.
#[derive(Component)]
struct Flash {
    left: f32,
}

#[derive(Resource, Default)]
struct ShotFx {
    /// The model drawn for each projectile in flight (`ShotVisual::id`); `None`: it has
    /// none to draw. With the y its model's `BeamEnd` node is at, if it has one.
    flying: HashMap<u64, Option<(Entity, Option<f32>)>>,
    /// Model paths that were reported.
    reported: HashSet<String>,
    flashes: u32,
    lights: u32,
    projectiles: u32,
    no_model: u32,
}

pub struct ShotFxPlugin;

impl Plugin for ShotFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShotFx>()
            .add_systems(Update, (preload, flash, draw_flying, face_camera).chain().after(nvf_play::PlaySet::Activation).run_if(resource_exists::<Combat>))
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

fn load(models: &mut ModelCtx, fx: &mut ShotFx, what: &str, model: &str) -> Option<Arc<PreparedModel>> {
    let path = fnv_actor::resolve::mesh_path(model);
    let pm = models.model(&path);
    if fx.reported.insert(path.clone()) {
        match &pm {
            Some(pm) => info!(
                "shot fx: {what} {path}: {} shapes, {} nodes, {} billboards",
                pm.shapes.len(),
                pm.nodes.len(),
                pm.nodes.iter().filter(|n| n.billboard.is_some()).count()
            ),
            None => warn!("shot fx: {what} {path}: model not loaded"),
        }
    }
    pm
}

/// Load the flash and projectile models of the weapon in use before its first shot: a
/// model first asked for in the frame it is drawn in can miss that frame, which is the
/// whole of a 0.04 s flash.
fn preload(combat: Res<Combat>, mut models: ModelCtx, mut fx: ResMut<ShotFx>, mut last: Local<Option<fnv_sim::FormId>>) {
    let now = combat.projectile();
    if *last == now {
        return;
    }
    *last = now;
    let Some(p) = now.and_then(|id| combat.forms.projectiles.get(&id)) else { return };
    if let Some(m) = p.muzzle_flash_model.as_deref().filter(|_| p.has("Muzzle Flash")) {
        load(&mut models, &mut fx, "muzzle flash", m);
    }
    if let Some(m) = p.model.as_deref() {
        load(&mut models, &mut fx, "projectile", m);
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
        // No muzzle to put it at (no body, no `ProjectileNode`): no flash.
        let Some(muzzle) = body.as_deref().and_then(PlayerBody::muzzle_entity) else { continue };
        let Some(pm) = load(&mut models, &mut fx, "muzzle flash", model) else { continue };
        let holder = spawn_model(&mut commands, muzzle, &pm, Transform::IDENTITY);
        // Made after this frame's flashes were aged: it is drawn this frame whatever
        // its duration, and goes in the first frame its time is up.
        commands.entity(holder).insert(Flash { left: p.muzzle_flash_duration });
        fx.flashes += 1;
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
fn draw_flying(
    mut commands: Commands,
    visuals: Res<ShotVisuals>,
    combat: Res<Combat>,
    mut fx: ResMut<ShotFx>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok(world_root) = root.single() else { return };
    let fx = &mut *fx;
    for v in &visuals.flying {
        let dir = Vec3::from_array(v.dir);
        let known = fx.flying.get(&v.id).copied();
        let beam_end = known.flatten().and_then(|k| k.1);
        let place = |beam_end: Option<f32>| Transform {
            translation: Vec3::from_array(v.pos),
            rotation: Quat::from_rotation_arc(Vec3::Y, dir.normalize_or(Vec3::Y)),
            scale: match (v.beam, beam_end) {
                (Some(len), Some(end)) if end > 0.0 => Vec3::new(1.0, len / end, 1.0),
                _ => Vec3::ONE,
            },
        };
        match known {
            Some(Some((e, _))) => {
                if let Ok(mut t) = transforms.get_mut(e) {
                    *t = place(beam_end);
                }
            }
            Some(None) => {}
            None => {
                let p = combat.forms.projectiles.get(&v.projectile);
                let pm = p.and_then(|p| p.model.as_deref()).and_then(|m| load(&mut models, fx, "projectile", m));
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
                    (spawn_model(&mut commands, world_root, &pm, place(end)), end)
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
        if let (false, Some((e, _))) = (keep, drawn) {
            commands.entity(*e).try_despawn();
        }
        keep
    });
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
            "shot fx: {} muzzle flashes ({} with a light), {} projectile models drawn, {} projectiles without a model",
            fx.flashes, fx.lights, fx.projectiles, fx.no_model
        );
    }
}
