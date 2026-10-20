//! Particle effects of add-on nodes (docs/spec-notes/particles.md): the sparks and smoke
//! of a muzzle flash, a missile's trail, the flamer's flame, the dust of an impact.
//!
//! - **Add-on nodes.** A model's `BSValueNode` named `AddOnNode<n>` stands for the ADDN
//!   record with that node index, whose model (`meshes\mps\…`) holds one or more
//!   `NiParticleSystem`s under a `BSMasterParticleSystem` [documented: xEdit ADDN;
//!   nif.xml]. Each add-on node in the world is an **emitter** of those systems for as
//!   long as it exists ([`AddonEmitter`]); the particles belong to the system, live in
//!   the world and outlast the node [hypothesis: the meaning of "master particle
//!   system", from the systems' `world_space` flag and from their particle counts,
//!   which equal birth rate × longest life × the emitter controller's `max_emitters`].
//! - **A system** is read by `nvf_assets::particles`; its particles are simulated here:
//!   born at the emitter's rate in its volume, thrown within its cone about the emitter
//!   object's +z, then aged, pulled by its gravity modifiers (in the world's axes),
//!   slowed by its drag modifiers, turned, sized by grow and fade, coloured over their
//!   life. A system holds at most its data block's vertex count of particles, and
//!   serves at most `max_emitters` emitters a frame.
//! - **Drawn** as one mesh per system, a square facing the camera for each particle,
//!   with the system's own texture, blending and vertex colours.
//! - **Model emitters** ([`ModelEmitter`]): a model's own particle systems, emitted from
//!   where its emitter objects are in the file (explosions, `shot_fx`).
//! - **Modifiers switched on and off** (`NiPSysModifierActiveCtlr`): a gravity or drag
//!   modifier acts on a particle while the emitter it came from is at a time its keys
//!   say on (`nvf_assets::particles::ModifierActive`).
//! - **Mesh emitters** start particles on their meshes (vertices, face or edge centres,
//!   anywhere on the faces or edges) thrown along the mesh's normal there; **strip
//!   systems** trail a strip of recorded points behind each particle, drawn as a ribbon;
//!   the **bomb** modifier pushes particles away from its object, **planar colliders**
//!   bounce or kill them; float controllers key emitter fields, gravity strength and
//!   birth rates over the emitter's time (particles.md §6). Bombs, colliders and
//!   spherical gravity sit where the emitter's node placed them when the particle was
//!   born (`Particle::frame`).
//! - **Placed models** emit their own systems and their add-on nodes' from a holder at
//!   the reference (`world::spawn_emitters`: Gomorrah's roof fires), not while hidden.
//! - Not built: turbulence, spawn-on-death.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::mesh::Indices;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use nvf_assets::model::ModelCtx;
use nvf_assets::particles::{particle_mesh, InitialVelocity, ParticleSystemDef, PreparedParticles};
use nvf_play::sfx::Sfx;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::world::WorldRoot;

/// The steps a second a drag modifier's percentage is taken per [hypothesis: fitted so
/// the flamer's flame, thrown at 2,700 units a second, ends near its projectile's range
/// of 640 units; docs/spec-notes/particles.md].
pub const DRAG_STEPS_PER_SECOND: f32 = 30.0;

/// An add-on node in the world: it emits the particles of ADDN `index` while it exists.
#[derive(Component)]
pub struct AddonEmitter {
    pub index: i32,
    age: f32,
}

impl AddonEmitter {
    pub fn new(index: i32) -> AddonEmitter {
        AddonEmitter { index, age: 0.0 }
    }
}

/// A model in the world that emits the particle systems inside its own file (an
/// explosion's flash, smoke and shrapnel) while it exists, each from its emitter object's
/// place in the file under this entity [hypothesis: as an add-on node emits its ADDN's;
/// the systems' rate and on/off keys come from the model's sequence when its emitter
/// controllers leave them to it, nvf_assets::particles].
#[derive(Component)]
pub struct ModelEmitter {
    pub path: String,
    age: f32,
}

impl ModelEmitter {
    pub fn new(path: impl Into<String>) -> ModelEmitter {
        ModelEmitter { path: path.into(), age: 0.0 }
    }
}

/// Whose systems: an ADDN's (by node index) or a model file's own (by path).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    Addon(i32),
    Model(String),
}

struct Particle {
    /// Its emitter node's place in the world when it was born: where its file's bombs,
    /// colliders and gravity points are.
    frame: Affine3A,
    pos: Vec3,
    vel: Vec3,
    /// Its emitter's time when it was born: the emitter's time now is this plus `age`
    /// (for modifiers switched on and off on the emitter's time).
    born_at: f32,
    age: f32,
    life: f32,
    radius: f32,
    angle: f32,
    spin: f32,
    tex: usize,
    /// A strip system's recorded points, oldest first, and the time since the last.
    trail: Vec<Vec3>,
    trail_clock: f32,
}

struct Live {
    def: ParticleSystemDef,
    particles: Vec<Particle>,
    mesh: Handle<Mesh>,
    entity: Entity,
    /// Emitters served this frame.
    served: usize,
    /// The part of a particle owed from earlier frames. Kept by the system, not by the
    /// emitter: a muzzle flash lives one frame, in which its share of the birth rate is
    /// about one particle, and a count kept by each flash would round most of them away
    /// [design].
    owed: f32,
    /// Particles born into it so far (the exit report).
    born: u64,
}

#[derive(Resource)]
pub struct Particles {
    /// The live systems of each ADDN index or model file; `None`: none we can read.
    systems: HashMap<Source, Option<Vec<Live>>>,
    reported: HashSet<Source>,
    rng: SplitMix,
    pub born: u64,
    pub most_alive: usize,
}

impl Default for Particles {
    fn default() -> Self {
        Particles { systems: HashMap::new(), reported: HashSet::new(), rng: SplitMix(0x2545_f491), born: 0, most_alive: 0 }
    }
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Particles>().add_systems(Update, (emit, simulate, draw).chain().after(crate::shot_fx::ShotFxSet)).add_systems(Last, report);
    }
}

/// An entity's place in game space from this frame's transforms: the chain of local
/// transforms up to the world root.
pub fn game_affine(e: Entity, transforms: &Query<&Transform>, parents: &Query<&ChildOf>, root: Entity) -> Affine3A {
    let mut m = Affine3A::IDENTITY;
    let mut at = Some(e);
    let mut depth = 0;
    while let Some(p) = at.filter(|p| *p != root) {
        if let Ok(t) = transforms.get(p) {
            m = t.compute_affine() * m;
        }
        at = parents.get(p).ok().map(ChildOf::parent);
        depth += 1;
        if depth > 256 {
            break;
        }
    }
    m
}

fn systems_of<'a>(
    p: &'a mut Particles,
    source: &Source,
    sfx: Option<&Sfx>,
    models: &mut ModelCtx,
    commands: &mut Commands,
    root: Entity,
) -> Option<&'a mut Vec<Live>> {
    if !p.systems.contains_key(source) {
        let (path, what) = match source {
            Source::Addon(index) => {
                let addon = sfx.and_then(|s| s.forms.addons.get(index));
                (addon.and_then(|a| a.model.as_deref()).map(fnv_actor::resolve::mesh_path), format!("add-on node {index}"))
            }
            Source::Model(path) => (Some(path.clone()), "model".to_string()),
        };
        let prepared: Option<Arc<PreparedParticles>> = path.as_deref().and_then(|p| models.particles(p));
        let live = prepared.filter(|pp| !pp.systems.is_empty()).map(|pp| {
            info!(
                "particles: {what} {}: {} system(s) [{}]{}",
                path.as_deref().unwrap_or("?"),
                pp.systems.len(),
                pp.systems.iter().map(ParticleSystemDef::describe).collect::<Vec<_>>().join("; "),
                if pp.unread.is_empty() { String::new() } else { format!("; not read: {}", pp.unread.join(", ")) }
            );
            pp.systems
                .iter()
                .map(|def| {
                    let mesh = models.meshes.add(particle_mesh());
                    let entity = commands
                        .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(def.material.clone()), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, ChildOf(root)))
                        .id();
                    Live { def: def.clone(), particles: Vec::new(), mesh, entity, served: 0, owed: 0.0, born: 0 }
                })
                .collect()
        });
        // A model without systems of its own is the common case (bullet impacts).
        if live.is_none() && matches!(source, Source::Addon(_)) && p.reported.insert(source.clone()) {
            warn!("particles: {what}: no particle systems ({})", path.as_deref().unwrap_or("no ADDN record or model"));
        }
        p.systems.insert(source.clone(), live);
    }
    p.systems.get_mut(source).and_then(Option::as_mut)
}

/// A number in −1..1.
fn spread(rng: &mut SplitMix) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// Births: each add-on node in the world emits into the systems of its ADDN, each
/// model emitter into its file's own.
#[allow(clippy::too_many_arguments)]
fn emit(
    mut commands: Commands,
    time: Res<Time>,
    mut p: ResMut<Particles>,
    sfx: Option<Res<Sfx>>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    mut emitters: Query<(Entity, &mut AddonEmitter, Option<&InheritedVisibility>)>,
    mut model_emitters: Query<(Entity, &mut ModelEmitter, Option<&InheritedVisibility>)>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
) {
    let Ok(root) = root.single() else { return };
    let dt = time.delta_secs();
    let p = &mut *p;
    for live in p.systems.values_mut().flatten().flatten() {
        live.served = 0;
    }
    let mut sources = Vec::new();
    // A hidden emitter (a disabled placed fire, `world::spawn_emitters`) keeps its time
    // but makes nothing. Not in its first frame, before its visibility was worked out:
    // a muzzle flash lives one frame [design].
    let hidden = |age: f32, v: Option<&InheritedVisibility>| age > 0.0 && v.is_some_and(|v| !v.get());
    for (e, mut em, v) in &mut emitters {
        if !hidden(em.age, v) {
            sources.push((e, Source::Addon(em.index), em.age));
        }
        em.age += dt;
    }
    for (e, mut em, v) in &mut model_emitters {
        if !hidden(em.age, v) {
            sources.push((e, Source::Model(em.path.clone()), em.age));
        }
        em.age += dt;
    }
    for (e, source, age) in sources {
        let mut rng = SplitMix(p.rng.0);
        let Some(systems) = systems_of(p, &source, sfx.as_deref(), &mut models, &mut commands, root) else { continue };
        let node = game_affine(e, &transforms, &parents, root);
        let mut born = 0;
        for live in systems.iter_mut() {
            let owed = &mut live.owed;
            let d = &live.def;
            let on = d.on_time(age, age + dt);
            if on <= 0.0 || (d.max_emitters > 0 && live.served >= d.max_emitters) {
                continue;
            }
            live.served += 1;
            // A keyed rate at the middle of the frame (particles.md §6.6).
            *owed += d.rate_at(age + dt * 0.5) * on;
            let frame = node * d.emitter.frame;
            let (_, turn, origin) = frame.to_scale_rotation_translation();
            let vals = d.emitter.values(age);
            while *owed >= 1.0 {
                *owed -= 1.0;
                if live.particles.len() >= d.max {
                    continue;
                }
                let m = &d.emitter;
                // Where it starts and the axis of its cone: on a mesh, along the normal
                // there (or at random, or a set axis); else somewhere in the emitter's
                // volume, about its +z.
                let (start, axis) = match &m.mesh {
                    Some(me) => {
                        let mesh = &me.meshes[((rng.next_f32() * me.meshes.len() as f32) as usize).min(me.meshes.len() - 1)];
                        let Some((at, normal)) = mesh.pick(me.from, &mut || rng.next_f32()) else { continue };
                        let mf = node * mesh.frame;
                        let along = match me.velocity {
                            InitialVelocity::Normals => mf.transform_vector3(normal),
                            InitialVelocity::Random => Vec3::new(spread(&mut rng), spread(&mut rng), spread(&mut rng)),
                            InitialVelocity::Direction(a) => mf.transform_vector3(a),
                        };
                        (mf.transform_point3(at), Quat::from_rotation_arc(Vec3::Z, along.normalize_or(Vec3::Z)))
                    }
                    None => {
                        let offset = if m.round {
                            let a = rng.next_f32() * std::f32::consts::TAU;
                            let r = m.size.x * rng.next_f32().sqrt();
                            Vec3::new(a.cos() * r, a.sin() * r, m.size.z * spread(&mut rng) * 0.5)
                        } else {
                            m.size * Vec3::new(spread(&mut rng), spread(&mut rng), spread(&mut rng)) * 0.5
                        };
                        (origin + turn * offset, turn)
                    }
                };
                // Thrown within the cone: so far from the axis, any way round it.
                let decl = vals.declination + vals.declination_variation * spread(&mut rng);
                let planar = vals.planar_angle + vals.planar_angle_variation * spread(&mut rng);
                let dir = Vec3::new(decl.sin() * planar.cos(), decl.sin() * planar.sin(), decl.cos());
                let speed = vals.speed + vals.speed_variation * spread(&mut rng) * 0.5;
                let rot = d.rotation.clone().unwrap_or_default();
                let mut spin = rot.speed + rot.speed_variation * spread(&mut rng);
                if rot.random_sign && rng.next_f32() < 0.5 {
                    spin = -spin;
                }
                live.particles.push(Particle {
                    frame: node,
                    pos: start,
                    vel: axis * dir * speed,
                    born_at: age,
                    age: 0.0,
                    life: (vals.life + vals.life_variation * spread(&mut rng) * 0.5).max(0.02),
                    radius: (vals.radius + vals.radius_variation * spread(&mut rng)).max(0.0),
                    angle: rot.angle + rot.angle_variation * spread(&mut rng),
                    spin,
                    tex: if d.subtextures.is_empty() { 0 } else { ((rng.next_f32() * d.subtextures.len() as f32) as usize).min(d.subtextures.len() - 1) },
                    // A strip starts where its particle is born.
                    trail: if d.strip.is_some() { vec![start] } else { Vec::new() },
                    trail_clock: 0.0,
                });
                born += 1;
                live.born += 1;
            }
        }
        p.rng = rng;
        p.born += born;
    }
}

/// Ageing and movement of every live particle.
fn simulate(time: Res<Time>, mut p: ResMut<Particles>) {
    let dt = time.delta_secs();
    let mut alive = 0;
    for live in p.systems.values_mut().flatten().flatten() {
        let d = &live.def;
        for q in &mut live.particles {
            q.age += dt;
            let at = q.born_at + q.age;
            for g in d.gravity.iter().filter(|g| g.active.on(at, d.period, d.loops)) {
                let pull = if g.spherical { (q.frame.transform_point3(g.origin) - q.pos).normalize_or_zero() } else { g.direction };
                q.vel += pull * g.strength_at(at) * dt;
            }
            for b in d.bombs.iter().filter(|b| b.active.on(at, d.period, d.loops)) {
                q.vel += b.push(&q.frame, q.pos) * dt;
            }
            for drag in d.drag.iter().filter(|g| g.active.on(at, d.period, d.loops)) {
                // The share of the speed along the axis kept after `dt`.
                let keep = (1.0 - drag.percentage.clamp(0.0, 1.0)).powf(dt * DRAG_STEPS_PER_SECOND);
                let along = q.vel.dot(drag.axis);
                q.vel -= drag.axis * along * (1.0 - keep);
            }
            let to = q.pos + q.vel * dt;
            match d.colliders.iter().find_map(|c| c.hit(&q.frame, q.pos, to, q.vel)) {
                None => q.pos = to,
                Some(None) => q.life = q.age,
                Some(Some((at, vel))) => {
                    q.pos = at;
                    q.vel = vel;
                }
            }
            q.angle += q.spin * dt;
            if let Some(st) = d.strip {
                q.trail_clock += dt;
                while q.trail_clock >= st.step {
                    q.trail_clock -= st.step;
                    q.trail.push(q.pos);
                }
                // The recorded points and the particle itself make at most `max_points`.
                let keep = st.max_points.saturating_sub(1).max(1);
                if q.trail.len() > keep {
                    q.trail.drain(..q.trail.len() - keep);
                }
            }
        }
        live.particles.retain(|q| q.age < q.life);
        alive += live.particles.len();
    }
    p.most_alive = p.most_alive.max(alive);
}

/// One mesh per system: a square facing the camera for each particle.
fn draw(
    p: Res<Particles>,
    mut meshes: ResMut<Assets<Mesh>>,
    camera: Query<&Transform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    root: Query<&Transform, With<WorldRoot>>,
    mut visibility: Query<&mut Visibility>,
) {
    let (Ok(cam), Ok(root)) = (camera.single(), root.single()) else { return };
    // The camera's right, up and backward in game space.
    let to_game = root.compute_affine().inverse();
    let right = to_game.transform_vector3(cam.rotation * Vec3::X).normalize_or(Vec3::X);
    let up = to_game.transform_vector3(cam.rotation * Vec3::Y).normalize_or(Vec3::Z);
    let back = to_game.transform_vector3(cam.rotation * Vec3::Z).normalize_or(Vec3::Y);
    for live in p.systems.values().flatten().flatten() {
        let shown = !live.particles.is_empty();
        if let Ok(mut v) = visibility.get_mut(live.entity) {
            let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        if !shown {
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&live.mesh) else { continue };
        let d = &live.def;
        let n = live.particles.len();
        let mut positions = Vec::with_capacity(n * 4);
        let mut uvs = Vec::with_capacity(n * 4);
        let mut colors = Vec::with_capacity(n * 4);
        let mut indices = Vec::with_capacity(n * 6);
        if d.strip.is_some() {
            // Ribbons through each particle's recorded points and its place, facing the
            // camera; the texture rectangle's width across, its height along, the oldest
            // point at its bottom (particles.md §6.2).
            for q in &live.particles {
                let points: Vec<Vec3> = q.trail.iter().copied().chain([q.pos]).collect();
                let count = points.len();
                if count < 2 {
                    continue;
                }
                let t = (q.age / q.life).clamp(0.0, 1.0);
                let half = q.radius * d.scale(q.age, q.life);
                let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
                let tint = d.tint(t);
                let e = d.emitter.color;
                let colour = display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3]]);
                let base = positions.len() as u32;
                for (i, p) in points.iter().enumerate() {
                    let tangent = points[(i + 1).min(count - 1)] - points[i.saturating_sub(1)];
                    let side = tangent.cross(back).normalize_or(right) * half;
                    positions.extend([(*p - side).to_array(), (*p + side).to_array()]);
                    let along = v + h * (1.0 - i as f32 / (count - 1) as f32);
                    uvs.extend([[u, along], [u + w, along]]);
                    colors.extend([colour; 2]);
                }
                for i in 0..(count as u32 - 1) {
                    let a = base + i * 2;
                    indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
                }
            }
        }
        for (k, q) in live.particles.iter().enumerate().filter(|_| d.strip.is_none()) {
            let t = (q.age / q.life).clamp(0.0, 1.0);
            let half = q.radius * d.scale(q.age, q.life);
            let (s, c) = q.angle.sin_cos();
            let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
            for corner in [-x - y, x - y, x + y, -x + y] {
                positions.push((q.pos + corner).to_array());
            }
            let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
            uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
            let tint = d.tint(t);
            let e = d.emitter.color;
            colors.extend([display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3]]); 4]);
            let b = (k * 4) as u32;
            indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![back.to_array(); positions.len()]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32(indices));
    }
}

/// A particle's colour (the colour modifier's times the emitter's) taken as display
/// values: converted to linear, the alpha kept. The particle systems are unlit
/// (BSShaderNoLightingProperty), and an unlit shape's texture × vertex colour is formed
/// in display values [hypothesis, as `nvf_assets::nif_spawn::unlit_vertex_colors` for
/// placed smoke, nif.md §6 "Vertex colours"]: multiplied in as linear values, the impact
/// dust's 0.27–0.43 greys drew as 0.57–0.69, a pale cloud on every wall hit (owner,
/// playtest list 5 item 48; particles.md §2 "Simple colour").
fn display_colour(c: [f32; 4]) -> [f32; 4] {
    let l = |v: f32| Color::srgb(v.max(0.0), 0.0, 0.0).to_linear().red;
    [l(c[0]), l(c[1]), l(c[2]), c[3]]
}

fn report(mut exit: MessageReader<AppExit>, p: Res<Particles>) {
    if exit.read().last().is_some() && p.born > 0 {
        info!("particles: {} born in {} add-on systems, at most {} alive at once", p.born, p.systems.values().flatten().count(), p.most_alive);
        // Each system that emitted, by its file.
        for (source, live) in &p.systems {
            let Some(live) = live else { continue };
            let file = match source {
                Source::Addon(i) => format!("add-on node {i}"),
                Source::Model(path) => path.clone(),
            };
            let born: Vec<String> = live.iter().filter(|l| l.born > 0).map(|l| format!("{} {}", l.def.name, l.born)).collect();
            if !born.is_empty() {
                info!("particles: born in {file}: {}", born.join(", "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_colours_are_display_values() {
        let c = display_colour([0.5, 1.0, 0.0, 0.3]);
        assert!((c[0] - 0.214).abs() < 1e-3 && (c[1] - 1.0).abs() < 1e-6 && c[2] == 0.0 && c[3] == 0.3, "{c:?}");
    }
}
