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
//! - Not built: turbulence, spawn-on-death, the bomb modifier, strip and mesh particle
//!   systems, particle systems of models placed in cells (fires, steam).

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::mesh::Indices;
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use nvf_assets::model::ModelCtx;
use nvf_assets::particles::{particle_mesh, ParticleSystemDef, PreparedParticles};
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
                pp.systems.iter().map(|s| format!("{} {}/s, at most {}", s.name, s.rate, s.max)).collect::<Vec<_>>().join("; "),
                if pp.unread.is_empty() { String::new() } else { format!("; not read: {}", pp.unread.join(", ")) }
            );
            pp.systems
                .iter()
                .map(|def| {
                    let mesh = models.meshes.add(particle_mesh());
                    let entity = commands
                        .spawn((Mesh3d(mesh.clone()), MeshMaterial3d(def.material.clone()), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, ChildOf(root)))
                        .id();
                    Live { def: def.clone(), particles: Vec::new(), mesh, entity, served: 0, owed: 0.0 }
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
    mut emitters: Query<(Entity, &mut AddonEmitter)>,
    mut model_emitters: Query<(Entity, &mut ModelEmitter)>,
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
    for (e, mut em) in &mut emitters {
        sources.push((e, Source::Addon(em.index), em.age));
        em.age += dt;
    }
    for (e, mut em) in &mut model_emitters {
        sources.push((e, Source::Model(em.path.clone()), em.age));
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
            *owed += d.rate * on;
            let frame = node * d.emitter.frame;
            let (_, turn, origin) = frame.to_scale_rotation_translation();
            while *owed >= 1.0 {
                *owed -= 1.0;
                if live.particles.len() >= d.max {
                    continue;
                }
                let m = &d.emitter;
                // Somewhere in the emitter's volume.
                let offset = if m.round {
                    let a = rng.next_f32() * std::f32::consts::TAU;
                    let r = m.size.x * rng.next_f32().sqrt();
                    Vec3::new(a.cos() * r, a.sin() * r, m.size.z * spread(&mut rng) * 0.5)
                } else {
                    m.size * Vec3::new(spread(&mut rng), spread(&mut rng), spread(&mut rng)) * 0.5
                };
                // Thrown within the cone: so far from the emitter's +z, any way round it.
                let decl = m.declination + m.declination_variation * spread(&mut rng);
                let planar = m.planar_angle + m.planar_angle_variation * spread(&mut rng);
                let dir = Vec3::new(decl.sin() * planar.cos(), decl.sin() * planar.sin(), decl.cos());
                let speed = m.speed + m.speed_variation * spread(&mut rng) * 0.5;
                let rot = d.rotation.clone().unwrap_or_default();
                let mut spin = rot.speed + rot.speed_variation * spread(&mut rng);
                if rot.random_sign && rng.next_f32() < 0.5 {
                    spin = -spin;
                }
                live.particles.push(Particle {
                    pos: origin + turn * offset,
                    vel: turn * dir * speed,
                    born_at: age,
                    age: 0.0,
                    life: (m.life + m.life_variation * spread(&mut rng) * 0.5).max(0.02),
                    radius: (m.radius + m.radius_variation * spread(&mut rng)).max(0.0),
                    angle: rot.angle + rot.angle_variation * spread(&mut rng),
                    spin,
                    tex: if d.subtextures.is_empty() { 0 } else { ((rng.next_f32() * d.subtextures.len() as f32) as usize).min(d.subtextures.len() - 1) },
                });
                born += 1;
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
                let pull = if g.spherical { (g.origin - q.pos).normalize_or_zero() } else { g.direction };
                q.vel += pull * g.strength * dt;
            }
            for drag in d.drag.iter().filter(|g| g.active.on(at, d.period, d.loops)) {
                // The share of the speed along the axis kept after `dt`.
                let keep = (1.0 - drag.percentage.clamp(0.0, 1.0)).powf(dt * DRAG_STEPS_PER_SECOND);
                let along = q.vel.dot(drag.axis);
                q.vel -= drag.axis * along * (1.0 - keep);
            }
            q.pos += q.vel * dt;
            q.angle += q.spin * dt;
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
        for (k, q) in live.particles.iter().enumerate() {
            let t = (q.age / q.life).clamp(0.0, 1.0);
            let half = q.radius * d.scale(q.age, q.life);
            let (s, c) = q.angle.sin_cos();
            let (x, y) = ((right * c + up * s) * half, (up * c - right * s) * half);
            for corner in [-x - y, x - y, x + y, -x + y] {
                positions.push((q.pos + corner).to_array());
            }
            let [u, w, v, h] = d.subtextures.get(q.tex).copied().unwrap_or([0.0, 1.0, 0.0, 1.0]);
            uvs.extend([[u, v + h], [u + w, v + h], [u + w, v], [u, v]]);
            let tint = d.color.as_ref().map_or([1.0; 4], |c| c.at(t));
            let e = d.emitter.color;
            colors.extend([display_colour([tint[0] * e[0], tint[1] * e[1], tint[2] * e[2], tint[3] * e[3]]); 4]);
            let b = (k * 4) as u32;
            indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![back.to_array(); n * 4]);
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
