//! The particle shaders of effect shaders (EFSH, docs/spec-notes/effect-shaders.md §6):
//! the flames of a burning actor (`Flames01`, the flamer's "Burning"), the smoke over it
//! (`Smoke01`), played on a reference by a magic effect or a script (`effect_shaders`).
//!
//! - **Births**: while the shader plays, particles are born at its birth ratio for the
//!   time since it started (ramp up, full, ramp down, then the persistent ratio while the
//!   persistent part shows) × [`BIRTHS_PER_RATIO`] a second [design: the GECK wiki calls
//!   the ratio "the amount of particles that spawn" without a unit].
//! - **Where**: on the reference's drawn meshes: at one of the bones a skinned mesh is
//!   bound to (so on the body as it moves), [`SURFACE_OFFSET`] out along the particle's
//!   normal, or for an unskinned mesh somewhere in its bounds [design: the game's
//!   "emit from the target object" surface points are not documented]; none from a
//!   hidden mesh (the player's body in first person). The normal is a random direction
//!   ("mostly random", GECK wiki).
//! - **Motion and look** [documented: the GECK wiki EffectShader page; hypothesis where
//!   `fnv_world::effect_shader::ParticleShader` says]: speed along the normal ± its range
//!   with its acceleration, plus the world velocity and acceleration (game axes); life ±
//!   its range; scale from 0 through the two keys (the scale as the particle's width in
//!   game units [hypothesis]); colour keys over the life from and back to the texture's
//!   own colour, the alpha held at the nearest key's (`ParticleShader::tint`); rotation ± range and spin ± range; one frame of the texture's U × V
//!   grid each, at random [hypothesis]. Drawn as squares facing the camera, unlit, the
//!   tint taken as display values as `particles.rs` does, blended by the destination
//!   blend (One: added).
//! - **End**: no births once the play ends; the particles alive live out their time.
//! - **Built only where a camera may see it** (issue #1417), as `particles.rs`'s systems
//!   are (its module notes, "Built only where a camera may see it" and "When"): births and
//!   motion stay in [`draw`] in `Update`, which ends by taking the world camera's basis
//!   (`particles::WorldCamera`; it no longer stops while a scope's camera is up) where it
//!   took it before; [`build`] makes the squares in `PostUpdate`
//!   (`particles::ParticleDrawSet`) only for a burst whose live particles' box
//!   ([`burst_bounds`]) some active camera drawing its layers may see this frame, and
//!   hides the others' render entities meanwhile.
//!
//! Log: `effect particles: <EDID> on <who> (<texture>, …)` when a play starts drawing,
//! and at exit `effect particles summary: …`.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::Indices;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use fnv_world::effect_shader::{blend, ParticleShader};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::nif_material;
use nvf_assets::particles::particle_mesh;
use nvf_play::script::ScriptState;
use std::collections::HashMap;

use crate::effect_shaders::{Shaders, Source};
use crate::world::WorldRoot;

/// Particles a second per unit of birth ratio [design: fitted by eye between
/// `Flames01`'s flames (persistent 15, living 0.35 s) covering a burning human and
/// `Smoke01`'s smoke (15, living 2.3 s and growing to 100 units) not hiding it].
pub const BIRTHS_PER_RATIO: f32 = 3.0;

/// Game units a particle starts out from the bone it is born at, along its normal: a
/// bone lies inside the body [design].
pub const SURFACE_OFFSET: f32 = 10.0;

/// Particles alive at once for one play, at most [design].
pub const MAX_ALIVE: usize = 600;

struct Particle {
    pos: Vec3,
    vel: Vec3,
    normal: Vec3,
    age: f32,
    life: f32,
    angle: f32,
    spin: f32,
    frame: u32,
}

/// The particles of one play.
struct Burst {
    def: ParticleShader,
    edid: String,
    particles: Vec<Particle>,
    mesh: Handle<Mesh>,
    entity: Entity,
    owed: f32,
    /// The play still goes on (births); once it ends the burst goes when empty.
    playing: bool,
}

#[derive(Resource)]
pub struct EffectParticles {
    bursts: HashMap<Source, Burst>,
    /// Shaders whose particle texture is missing, reported once.
    missing: Vec<u32>,
    rng: SplitMix,
    born: u64,
    plays: u32,
    most_alive: usize,
    /// The main camera's right, up and backward in game space taken this frame by
    /// [`draw`] for [`build`]; `None`: nothing is drawn.
    basis: Option<[Vec3; 3]>,
}

impl Default for EffectParticles {
    fn default() -> Self {
        EffectParticles { bursts: HashMap::new(), missing: Vec::new(), rng: SplitMix(0x5eed_f1a3), born: 0, plays: 0, most_alive: 0, basis: None }
    }
}

impl EffectParticles {
    /// The bursts' meshes (the profile aid's `profile change sources:` line).
    pub fn mesh_ids(&self) -> impl Iterator<Item = AssetId<Mesh>> + '_ {
        self.bursts.values().map(|b| b.mesh.id())
    }
}

/// A number in −1..1.
fn spread(rng: &mut SplitMix) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// A random unit vector.
fn random_dir(rng: &mut SplitMix) -> Vec3 {
    let z = spread(rng);
    let a = rng.next_f32() * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * a.cos(), r * a.sin(), z)
}

fn alpha_mode(dest_blend: u32) -> AlphaMode {
    match dest_blend {
        blend::SOURCE_INVERTED_ALPHA => AlphaMode::Blend,
        blend::DEST_COLOR => AlphaMode::Multiply,
        _ => AlphaMode::Add,
    }
}

/// A particle's tint as display values: converted to linear, the alpha kept
/// (`particles.rs`, `display_colour`).
fn display_colour(c: [f32; 4]) -> [f32; 4] {
    let l = Color::srgb(c[0].max(0.0), c[1].max(0.0), c[2].max(0.0)).to_linear();
    [l.red, l.green, l.blue, c[3]]
}

/// Births and motion of every play's particles, then the camera basis [`build`] draws
/// them with this frame.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    mut commands: Commands,
    time: Res<Time>,
    shaders: Res<Shaders>,
    script: Option<Res<ScriptState>>,
    mut st: ResMut<EffectParticles>,
    mut models: ModelCtx,
    root: Query<(Entity, &GlobalTransform), With<WorldRoot>>,
    skins: Query<&SkinnedMesh>,
    globals: Query<&GlobalTransform>,
    bounds: Query<&Aabb>,
    shown: Query<&InheritedVisibility>,
    layers: Query<&bevy::camera::visibility::RenderLayers>,
    camera: Query<&Transform, crate::particles::WorldCamera>,
) {
    st.basis = None;
    let Ok((root, root_at)) = root.single() else { return };
    let st = &mut *st;
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    let to_game = root_at.affine().inverse();

    // New plays with a particle shader get a burst; ended ones stop being born into.
    for b in st.bursts.values_mut() {
        b.playing = false;
    }
    for (key, a) in &shaders.active {
        let Some(def) = a.shader.particles.as_ref() else { continue };
        if let Some(b) = st.bursts.get_mut(key) {
            b.playing = true;
            continue;
        }
        let path = a.shader.particle_texture.as_deref().map(|t| {
            let p = t.replace('/', "\\").to_ascii_lowercase();
            if p.starts_with("textures\\") {
                p
            } else {
                format!("textures\\{p}")
            }
        });
        let m = &mut models;
        let texture = path.as_deref().and_then(|p| m.textures.get(p, true, &m.data, &mut m.images, m.formats.as_deref()));
        if texture.is_none() {
            if !st.missing.contains(&a.shader.form_id) {
                st.missing.push(a.shader.form_id);
                warn!("effect particles: {}: particle texture {} not found", a.shader.edid, path.as_deref().unwrap_or("?"));
            }
            continue;
        }
        let material = models.materials.add(nif_material(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: texture,
            unlit: true,
            alpha_mode: alpha_mode(def.dest_blend),
            double_sided: true,
            cull_mode: None,
            ..default()
        }));
        let mesh = models.meshes.add(particle_mesh());
        let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, ChildOf(root))).id();
        st.plays += 1;
        let who = if a.on == FormId::PLAYER_REF.0 { "the player".to_string() } else { script.as_deref().and_then(|s| s.rt.as_ref()).map_or_else(|| format!("{:08X}", a.on), |rt| rt.game.data.name(FormId(a.on)).to_string()) };
        info!(
            "effect particles: {} on {who} ({}, {:.0}/s at full, {:.0}/s persistent, life {:.2} s, frames {}x{})",
            a.shader.edid,
            path.as_deref().unwrap_or("?"),
            def.full_ratio * BIRTHS_PER_RATIO,
            def.persistent_ratio * BIRTHS_PER_RATIO,
            def.lifetime,
            def.frames[0],
            def.frames[1]
        );
        st.bursts.insert(*key, Burst { def: def.clone(), edid: a.shader.edid.clone(), particles: Vec::new(), mesh, entity, owed: 0.0, playing: true });
    }

    // Births on the covered meshes.
    let rng = &mut st.rng;
    let mut born = 0;
    for (key, b) in st.bursts.iter_mut().filter(|(_, b)| b.playing) {
        let Some(a) = shaders.active.get(key) else { continue };
        let d = &b.def;
        b.owed += d.birth_ratio(now - a.since, a.persist) * BIRTHS_PER_RATIO * dt;
        // Births past the free slots are dropped (as the loop below would skip them);
        // capping here keeps a broken shader's huge or infinite ratio from looping
        // without end.
        let free = MAX_ALIVE.saturating_sub(b.particles.len()) as f32;
        b.owed = if b.owed.is_finite() { b.owed.min(free + b.owed.fract()) } else { 0.0 };
        if b.owed < 1.0 {
            continue;
        }
        // Where particles can be born: the bones of the skinned meshes, the bounds of the others.
        let mut bones: Vec<Entity> = Vec::new();
        let mut boxes: Vec<(Entity, Aabb)> = Vec::new();
        for &e in a.covered.keys() {
            // [design: player_body::show_rigs] Hidden meshes still give off none;
            // the player's shadow-only meshes also have no births in the camera.
            if shown.get(e).is_ok_and(|v| !v.get()) || (a.on == FormId::PLAYER_REF.0 && !crate::effect_shaders::player_mesh_seen(layers.get(e).ok())) {
                continue;
            }
            match skins.get(e) {
                Ok(s) => bones.extend(s.joints.iter().copied()),
                Err(_) => {
                    if let Ok(bx) = bounds.get(e) {
                        boxes.push((e, *bx));
                    }
                }
            }
        }
        bones.sort();
        bones.dedup();
        let places = bones.len() + boxes.len();
        if places == 0 {
            b.owed = 0.0;
            continue;
        }
        while b.owed >= 1.0 {
            b.owed -= 1.0;
            if b.particles.len() >= MAX_ALIVE {
                continue;
            }
            let pick = ((rng.next_f32() * places as f32) as usize).min(places - 1);
            let normal = random_dir(rng);
            let at = if pick < bones.len() {
                let Ok(g) = globals.get(bones[pick]) else { continue };
                to_game.transform_point3(g.translation()) + normal * SURFACE_OFFSET
            } else {
                let (e, bx) = &boxes[pick - bones.len()];
                let Ok(g) = globals.get(*e) else { continue };
                let local = Vec3::from(bx.center) + Vec3::from(bx.half_extents) * Vec3::new(spread(rng), spread(rng), spread(rng));
                to_game.transform_point3(g.transform_point(local))
            };
            let speed = d.speed_normal + d.speed_normal_range * spread(rng);
            b.particles.push(Particle {
                pos: at,
                vel: normal * speed + Vec3::from(d.velocity),
                normal,
                age: 0.0,
                life: (d.lifetime + d.lifetime_range * spread(rng)).max(0.02),
                angle: (d.rotation + d.rotation_range * spread(rng)).to_radians(),
                spin: (d.spin + d.spin_range * spread(rng)).to_radians(),
                frame: (rng.next_f32() * (d.frames[0] * d.frames[1]) as f32) as u32,
            });
            born += 1;
        }
    }
    st.born += born;

    // Motion; bursts whose play ended and whose particles are gone go.
    let mut alive = 0;
    for b in st.bursts.values_mut() {
        let d = &b.def;
        let accel = Vec3::from(d.accel);
        for q in &mut b.particles {
            q.age += dt;
            q.vel += (accel + q.normal * d.accel_normal) * dt;
            q.pos += q.vel * dt;
            q.angle += q.spin * dt;
        }
        b.particles.retain(|q| q.age < q.life);
        alive += b.particles.len();
    }
    st.most_alive = st.most_alive.max(alive);
    let done: Vec<Source> = st.bursts.iter().filter(|(_, b)| !b.playing && b.particles.is_empty()).map(|(k, _)| *k).collect();
    for k in done {
        if let Some(b) = st.bursts.remove(&k) {
            commands.entity(b.entity).try_despawn();
            models.meshes.remove(&b.mesh);
            debug!("effect particles: {} done", b.edid);
        }
    }

    // The basis the squares face this frame, taken where the draw took it before it
    // moved to `build`.
    let Ok(cam) = camera.single() else { return };
    st.basis = Some(crate::particles::camera_basis(cam.rotation, &to_game));
}

/// Drawing: a square facing the camera per particle, built only for a burst some camera
/// may see this frame (module notes); the others' render entities are hidden.
pub fn build(
    mut st: ResMut<EffectParticles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visibility: Query<&mut Visibility>,
    views: crate::particles::ParticleViews,
    mut counts: Option<ResMut<crate::profile::ParticleCull>>,
) {
    // Taken: a frame whose `draw` did not get this far draws nothing, as before.
    let Some(basis) = st.basis.take() else { return };
    let frame = views.frame();
    for b in st.bursts.values() {
        let alive = !b.particles.is_empty();
        let seen = alive && frame.may_see(burst_bounds(b), b.entity, &views);
        // The burst's entity may have gone with the world root (a load); nothing to show then.
        if let Ok(mut v) = visibility.get_mut(b.entity) {
            let want = if seen { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        if !alive {
            continue;
        }
        if let Some(c) = counts.as_deref_mut() {
            c.count(crate::profile::ParticleKind::Effect, seen);
        }
        if !seen {
            // Its mesh keeps its last squares, hidden, until a camera may see it again.
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&b.mesh) else { continue };
        let g = squares(b, basis);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, g.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, g.colors);
        mesh.insert_indices(Indices::U32(g.indices));
    }
}

/// A burst's squares: the mesh attributes and indices.
struct Squares {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// A square facing the camera (`basis`: its right, up and backward in game space) per
/// particle of `b`.
fn squares(b: &Burst, [right, up, back]: [Vec3; 3]) -> Squares {
    let d = &b.def;
    let n = b.particles.len();
    let (mut positions, mut uvs, mut colors, mut indices) = (Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 6));
    let [fu, fv] = d.frames;
    let (w, h) = (1.0 / fu as f32, 1.0 / fv as f32);
    for (k, q) in b.particles.iter().enumerate() {
        let f = (q.age / q.life).clamp(0.0, 1.0);
        let half = 0.5 * d.scale(f).max(0.0);
        let (s, c) = q.angle.sin_cos();
        let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
        for corner in [-x - y, x - y, x + y, -x + y] {
            positions.push((q.pos + corner).to_array());
        }
        let (u, v) = ((q.frame % fu) as f32 * w, (q.frame / fu % fv) as f32 * h);
        uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
        colors.extend([display_colour(d.tint(f)); 4]);
        let i = (k * 4) as u32;
        indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    Squares { positions, normals: vec![back.to_array(); n * 4], uvs, colors, indices }
}

/// A game-space box holding every corner [`squares`] makes of the burst, whatever the
/// camera's basis: within half the width × √2 of each particle (the camera's right and
/// up are orthonormal, as for `particles.rs`'s `particle_bounds`). `None` without a
/// particle or with a value not finite (then drawn as before).
fn burst_bounds(b: &Burst) -> Option<Aabb> {
    let d = &b.def;
    crate::particles::sphere_bounds(b.particles.iter().map(|q| {
        let half = 0.5 * d.scale((q.age / q.life).clamp(0.0, 1.0)).max(0.0);
        (q.pos, half * std::f32::consts::SQRT_2)
    }))
}

pub fn report(mut exit: MessageReader<AppExit>, st: Res<EffectParticles>) {
    if exit.read().last().is_some() && st.plays > 0 {
        info!("effect particles summary: {} plays drew particles, {} born, at most {} alive at once", st.plays, st.born, st.most_alive);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_directions_are_unit_and_spread() {
        let mut rng = SplitMix(1);
        let mut sum = Vec3::ZERO;
        for _ in 0..2000 {
            let d = random_dir(&mut rng);
            assert!((d.length() - 1.0).abs() < 1e-4);
            sum += d;
        }
        assert!(sum.length() / 2000.0 < 0.1, "{sum:?}");
    }

    /// A burst of `n` particles ahead of a camera with no turn (game +y is Bevy -z), growing
    /// to 40 units, turned and framed differently.
    fn test_burst(n: usize, mesh: Handle<Mesh>, entity: Entity) -> Burst {
        let def = ParticleShader { scale_keys: [(1.0, 0.0), (40.0, 1.0)], lifetime: 2.0, frames: [2, 2], ..default() };
        let particles = (0..n)
            .map(|i| Particle {
                pos: Vec3::new(i as f32 * 15.0 - 30.0, 500.0 + i as f32, 20.0),
                vel: Vec3::ZERO,
                normal: Vec3::Z,
                age: 0.1 + i as f32 * 0.3,
                life: 2.0,
                angle: i as f32 * 0.7,
                spin: 0.0,
                frame: i as u32,
            })
            .collect();
        Burst { def, edid: "TestShader".into(), particles, mesh, entity, owed: 0.0, playing: true }
    }

    /// The burst's box holds every corner of its squares whatever way the camera faces.
    #[test]
    fn burst_bounds_hold_every_corner_for_any_camera() {
        let to_game = crate::world::game_to_bevy().compute_affine().inverse();
        let b = test_burst(6, Handle::default(), Entity::PLACEHOLDER);
        let bx = burst_bounds(&b).expect("bounds");
        let (lo, hi) = (Vec3::from(bx.min()), Vec3::from(bx.max()));
        for k in 0..48 {
            let rot = Quat::from_euler(EulerRot::YXZ, k as f32 * 0.61, (k as f32 * 0.37).sin() * 1.5, k as f32 * 0.11);
            let g = squares(&b, crate::particles::camera_basis(rot, &to_game));
            assert_eq!(g.positions.len(), 24);
            for p in &g.positions {
                let p = Vec3::from_array(*p);
                assert!(p.cmpge(lo).all() && p.cmple(hi).all(), "{p} outside {lo}..{hi}");
            }
        }
        assert!(burst_bounds(&test_burst(0, Handle::default(), Entity::PLACEHOLDER)).is_none());
    }

    /// A burst in view is built and shown; out of view hidden with its mesh left alone;
    /// back in view rebuilt and shown in that same frame.
    #[test]
    fn a_burst_out_of_view_is_hidden_and_not_rebuilt() {
        use bevy::camera::primitives::Frustum;
        use bevy::ecs::system::RunSystemOnce;
        let projection = Projection::Perspective(PerspectiveProjection { fov: 75f32.to_radians(), aspect_ratio: 16.0 / 9.0, near: 5.0, ..default() });
        let placed = |rotation: Quat| {
            let t = Transform::from_rotation(rotation);
            let g = GlobalTransform::from(t);
            let f: Frustum = projection.compute_frustum(&g);
            (t, g, f)
        };
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let root_t = crate::world::game_to_bevy();
        let to_game = root_t.compute_affine().inverse();
        let camera = world.spawn((Camera3d::default(), Camera::default(), projection.clone(), placed(Quat::IDENTITY))).id();
        let mesh = world.resource_mut::<Assets<Mesh>>().add(particle_mesh());
        let entity = world.spawn((Visibility::Hidden, Transform::IDENTITY, GlobalTransform::from(root_t))).id();
        let mut st = EffectParticles::default();
        st.bursts.insert(Source::Script(1), test_burst(4, mesh.clone(), entity));
        world.insert_resource(st);
        let positions = |w: &World| w.resource::<Assets<Mesh>>().get(&mesh).unwrap().attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap().to_vec();
        let shown = |w: &World| *w.get::<Visibility>(entity).unwrap() == Visibility::Inherited;
        // One frame: the basis `draw` would take, a little motion, then the build.
        let frame = |w: &mut World, rotation: Quat| {
            let mut st = w.resource_mut::<EffectParticles>();
            for b in st.bursts.values_mut() {
                for q in &mut b.particles {
                    q.pos.z += 1.0;
                }
            }
            st.basis = Some(crate::particles::camera_basis(rotation, &to_game));
            w.run_system_once(build).unwrap();
        };
        let expected = |w: &World, rotation: Quat| squares(w.resource::<EffectParticles>().bursts.values().next().unwrap(), crate::particles::camera_basis(rotation, &to_game)).positions;
        frame(&mut world, Quat::IDENTITY);
        assert!(shown(&world));
        assert_eq!(positions(&world), expected(&world, Quat::IDENTITY), "in view: the squares as before, bit for bit");
        let before = positions(&world);
        let away = Quat::from_rotation_y(std::f32::consts::PI);
        world.entity_mut(camera).insert(placed(away));
        frame(&mut world, away);
        assert!(!shown(&world), "out of view: hidden");
        assert_eq!(positions(&world), before, "out of view: not rebuilt");
        world.entity_mut(camera).insert(placed(Quat::IDENTITY));
        frame(&mut world, Quat::IDENTITY);
        assert!(shown(&world), "back in view: shown at once");
        assert_eq!(positions(&world), expected(&world, Quat::IDENTITY), "back in view: this frame's squares");
        // A frame whose `draw` took no basis (no single main camera): left as it was.
        let now = positions(&world);
        world.run_system_once(build).unwrap();
        assert_eq!(positions(&world), now);
    }

    /// Culling changes nothing of the bursts' births, motion and deaths: a play giving
    /// birth all the time on a covered mesh, its particles living 0.1-0.5 s, run with the
    /// world camera facing it and facing away; the particles, the random state and the
    /// counts are the same, and only the one facing it is drawn.
    #[test]
    fn effect_simulation_is_the_same_with_the_burst_culled_or_drawn() {
        use bevy::camera::primitives::Frustum;
        use bevy::ecs::system::RunSystemOnce;
        use fnv_world::effect_shader::{AlphaCycle, EffectShader};
        let run = |facing: Quat| {
            let mut world = World::new();
            crate::particles::model_ctx_resources(&mut world);
            world.insert_resource(Time::<()>::default());
            let root_t = crate::world::game_to_bevy();
            let root = world.spawn((WorldRoot, root_t, GlobalTransform::from(root_t))).id();
            let projection = Projection::Perspective(PerspectiveProjection { fov: 75f32.to_radians(), aspect_ratio: 16.0 / 9.0, near: 5.0, ..default() });
            let t = Transform::from_rotation(facing);
            let g = GlobalTransform::from(t);
            let f: Frustum = projection.compute_frustum(&g);
            world.spawn((Camera3d::default(), Camera::default(), projection.clone(), t, g, f, crate::fly::FlyCam { yaw: 0.0, pitch: 0.0, speed: 0.0 }));
            // The covered mesh 500 units ahead of a camera with no turn (Bevy -z), a box
            // for the births.
            let covered = world.spawn((GlobalTransform::from_translation(Vec3::new(0.0, 0.0, -500.0)), Aabb::from_min_max(Vec3::splat(-20.0), Vec3::splat(20.0)))).id();
            let mesh = world.resource_mut::<Assets<Mesh>>().add(particle_mesh());
            let entity = world.spawn((Visibility::Hidden, Transform::IDENTITY, GlobalTransform::from(root_t), ChildOf(root))).id();
            let def = ParticleShader {
                full_ratio: 25.0,
                persistent_ratio: 25.0,
                full_time: 1000.0,
                lifetime: 0.3,
                lifetime_range: 0.2,
                speed_normal: 30.0,
                speed_normal_range: 10.0,
                accel: [0.0, 0.0, 15.0],
                scale_keys: [(5.0, 0.2), (30.0, 1.0)],
                rotation_range: 90.0,
                spin: 20.0,
                spin_range: 10.0,
                frames: [2, 2],
                ..default()
            };
            let shader = EffectShader {
                form_id: 1,
                edid: "TestShader".into(),
                flags: 0,
                fill_texture: None,
                particle_texture: Some("fx\\test.dds".into()),
                source_blend: 0,
                blend_op: 0,
                z_test: 0,
                dest_blend: 0,
                fill_color: [0; 3],
                fill: AlphaCycle::default(),
                fill_speed: [0.0; 2],
                edge_color: [0; 3],
                edge: AlphaCycle::default(),
                edge_fall_off: 0.0,
                particles: Some(def.clone()),
                holes: None,
                addons: None,
            };
            let key = Source::Script(1);
            let mut shaders = Shaders::default();
            shaders.active.insert(key, crate::effect_shaders::Active::for_test(0x0001_2345, shader, 0.0, &[covered]));
            world.insert_resource(shaders);
            let mut st = EffectParticles::default();
            // The burst made already (its texture is not looked up again).
            st.bursts.insert(key, Burst { def, edid: "TestShader".into(), particles: Vec::new(), mesh, entity, owed: 0.0, playing: true });
            world.insert_resource(st);
            let mut most = 0;
            for k in 0..150 {
                let dt = 0.004 + (k % 7) as f32 * 0.003;
                world.resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(dt));
                world.run_system_once(draw).unwrap();
                world.run_system_once(build).unwrap();
                most = most.max(world.resource::<EffectParticles>().bursts.values().next().map_or(0, |b| b.particles.len()));
            }
            let st = world.resource::<EffectParticles>();
            let b = st.bursts.values().next().expect("the burst lives on while it plays");
            let particles: Vec<_> = b.particles.iter().map(|q| (q.pos.to_array().map(f32::to_bits), q.vel.to_array().map(f32::to_bits), q.age.to_bits(), q.life.to_bits(), q.angle.to_bits(), q.spin.to_bits(), q.frame)).collect();
            let shown = *world.get::<Visibility>(entity).unwrap() == Visibility::Inherited;
            ((particles, st.rng.0, st.born, st.most_alive, b.owed.to_bits()), shown, most)
        };
        let (seen, seen_shown, most) = run(Quat::IDENTITY);
        let (culled, culled_shown, _) = run(Quat::from_rotation_y(std::f32::consts::PI));
        assert!(seen_shown && !culled_shown, "drawn facing it, culled facing away");
        assert_eq!(seen, culled);
        // Births and deaths both happened: far more born than ever alive at once.
        let born = seen.2;
        assert!(born > 40 && (most as u64) < born, "born {born}, most alive {most}");
        assert!(!seen.0.is_empty());
    }
}
