//! The particle systems of a NIF as data (docs/spec-notes/particles.md): what each
//! `NiParticleSystem` emits and how its particles change, read from its modifier and
//! controller blocks [documented: nif.xml block and field names]. What the fields do is
//! mostly a reading of their names [hypothesis], set out in the notes; the simulation
//! is the caller's (`nvforever::particles`).

use crate::model::ModelCtx;
use crate::nif_material::NifMaterial;
use crate::nif_spawn::{shape_material, shape_props};
use bevy::asset::RenderAssetUsages;
use bevy::math::Affine3A;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use fnv_nif::scene::{scene_objects, Decoded, SceneObject, Transform as NifTransform};
use fnv_nif::value::Struct;
use fnv_nif::NifFile;
use std::collections::HashMap;
use std::sync::Arc;

/// A `NiPSys…Emitter`: where particles are born and what they start with.
#[derive(Debug, Clone)]
pub struct Emitter {
    /// The emitter object's place in the file (the root's transform left out).
    pub frame: Affine3A,
    /// The volume particles start in, about the emitter object: a box's width, height
    /// and depth (x, y, z); a sphere's or cylinder's radius in x and y and a cylinder's
    /// height in z.
    pub size: Vec3,
    pub round: bool,
    pub speed: f32,
    pub speed_variation: f32,
    /// Radians from the emitter object's +z, and about it.
    pub declination: f32,
    pub declination_variation: f32,
    pub planar_angle: f32,
    pub planar_angle_variation: f32,
    pub color: [f32; 4],
    pub radius: f32,
    pub radius_variation: f32,
    pub life: f32,
    pub life_variation: f32,
}

/// `BSPSysSimpleColorModifier`: three colours over a particle's life, and its fades.
#[derive(Debug, Clone)]
pub struct SimpleColor {
    pub fade_in: f32,
    pub fade_out: f32,
    pub color1_end: f32,
    pub color2_start: f32,
    pub color2_end: f32,
    pub color3_start: f32,
    pub colors: [[f32; 4]; 3],
}

impl SimpleColor {
    /// The colour at `t`, the share of its life a particle has lived [hypothesis: the
    /// first colour until `color1_end`, a blend to the second by `color2_start`, the
    /// second until `color2_end`, a blend to the third by `color3_start`; alpha rises
    /// from nothing over `fade_in` and falls to nothing from `fade_out` to the end].
    /// With no room for the last blend (`color3_start` not after `color2_end`) the
    /// second colour stays [hypothesis: `mpsdirtdust01.nif` has all four percentages 0
    /// and only its second colour is not blank, so that is the colour meant].
    pub fn at(&self, t: f32) -> [f32; 4] {
        let mix = |a: [f32; 4], b: [f32; 4], k: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k.clamp(0.0, 1.0));
        let share = |from: f32, to: f32| (t - from) / (to - from);
        let [c1, c2, c3] = self.colors;
        let mut c: [f32; 4] = if t < self.color2_start && self.color2_start > self.color1_end {
            if t <= self.color1_end { c1 } else { mix(c1, c2, share(self.color1_end, self.color2_start)) }
        } else if t <= self.color2_end || self.color3_start <= self.color2_end {
            c2
        } else {
            mix(c2, c3, share(self.color2_end, self.color3_start))
        };
        if self.fade_in > 0.0 && t < self.fade_in {
            c[3] *= t / self.fade_in;
        }
        if self.fade_out < 1.0 && t > self.fade_out {
            c[3] *= ((1.0 - t) / (1.0 - self.fade_out)).clamp(0.0, 1.0);
        }
        c
    }
}

/// `NiPSysRotationModifier`: radians and radians per second.
#[derive(Debug, Clone, Default)]
pub struct Rotation {
    pub speed: f32,
    pub speed_variation: f32,
    pub angle: f32,
    pub angle_variation: f32,
    pub random_sign: bool,
}

/// `NiPSysGravityModifier`: a pull along an axis (force type 0) or towards a point.
#[derive(Debug, Clone)]
pub struct Gravity {
    /// The gravity object's axis in the file's frame (unit), and its place.
    pub direction: Vec3,
    pub origin: Vec3,
    pub strength: f32,
    pub spherical: bool,
    /// When it acts ([`ModifierActive`]).
    pub active: ModifierActive,
}

/// `NiPSysDragModifier`: the share of the velocity along `axis` lost per step.
#[derive(Debug, Clone)]
pub struct Drag {
    pub axis: Vec3,
    pub percentage: f32,
    /// When it acts ([`ModifierActive`]).
    pub active: ModifierActive,
}

/// A modifier switched on and off over time by a `NiPSysModifierActiveCtlr` (its own
/// bool keys, or those of the model's sequence when the controller carries a blend
/// interpolator) [documented: nif.xml block names; observed: `explosiongrenadefrag.nif`'s
/// smoke drags are on from 0 s and off at 0.33 s of its `SpecialIdle_AreaEffect`].
/// Reading [hypothesis]: the keys are on the emitter's time, as the emitter's own on/off
/// keys are, and a particle feels the modifier while the emitter it came from is at a
/// time the keys say on; empty keys: always on.
#[derive(Debug, Clone, Default)]
pub struct ModifierActive {
    pub keys: Vec<(f32, bool)>,
}

impl ModifierActive {
    /// Whether the modifier acts at the emitter's time `t` (`period` > 0 with `loops`:
    /// the keys repeat).
    pub fn on(&self, t: f32, period: f32, loops: bool) -> bool {
        let t = if loops && period > 0.0 { t.rem_euclid(period) } else { t };
        self.keys.iter().rev().find(|(k, _)| *k <= t + 1e-4).or(self.keys.first()).is_none_or(|(_, on)| *on)
    }
}

/// One `NiParticleSystem`.
#[derive(Debug, Clone)]
pub struct ParticleSystemDef {
    pub name: String,
    /// The most particles alive at once (the data block's vertex count).
    pub max: usize,
    pub material: Handle<NifMaterial>,
    pub emitter: Emitter,
    /// Particles born per second while the emitter is on (the emitter controller's
    /// float interpolator).
    pub rate: f32,
    /// The emitter controller's on/off keys (time, on) and its stop time, after which
    /// they repeat when the controller cycles.
    pub active: Vec<(f32, bool)>,
    pub period: f32,
    pub loops: bool,
    /// The most emitters served at once (`BSPSysMultiTargetEmitterCtlr`); 0: no limit.
    pub max_emitters: usize,
    pub grow_time: f32,
    pub fade_time: f32,
    pub base_scale: f32,
    pub color: Option<SimpleColor>,
    pub rotation: Option<Rotation>,
    pub gravity: Vec<Gravity>,
    pub drag: Vec<Drag>,
    /// Parts of the texture a particle can show: (u, width, v, height).
    pub subtextures: Vec<[f32; 4]>,
}

impl ParticleSystemDef {
    /// Whether the emitter is on `age` seconds after it appeared.
    pub fn emitting(&self, age: f32) -> bool {
        let t = if self.loops && self.period > 0.0 { age % self.period } else { age };
        self.active.iter().rev().find(|(time, _)| *time <= t + 1e-4).or(self.active.first()).is_none_or(|(_, on)| *on)
    }

    /// How many of the seconds from `from` to `to` after the emitter appeared it is on:
    /// births are the rate times this, so an emitter on for a moment (an explosion's
    /// burst: 400,000 a second for 0.00001 s) gives its few particles whatever the frame
    /// length.
    pub fn on_time(&self, from: f32, to: f32) -> f32 {
        if self.active.is_empty() {
            return (to - from).max(0.0);
        }
        let state = |t: f32| self.active.iter().rev().find(|(k, _)| *k <= t).or(self.active.first()).is_none_or(|(_, on)| *on);
        // The span split at the keys (and, looping, at the period's ends).
        let looping = self.loops && self.period > 0.0;
        let mut on = 0.0;
        let mut t = from;
        let mut steps = 0;
        while t < to && steps < 4096 {
            steps += 1;
            let (base, local) = if looping { let n = (t / self.period).floor(); (n * self.period, t - n * self.period) } else { (0.0, t) };
            let mut next = self.active.iter().map(|(k, _)| *k).filter(|k| *k > local).fold(f32::INFINITY, f32::min);
            if looping {
                next = next.min(self.period);
            }
            let end = (base + next).min(to);
            if state(local) {
                on += end - t;
            }
            if end <= t {
                break;
            }
            t = end;
        }
        on
    }

    /// A particle's size as a share of its radius, `age` seconds into a life of `life`
    /// [hypothesis: from the base scale to 1 over the grow time, and back to the base
    /// scale over the fade time before it dies].
    pub fn scale(&self, age: f32, life: f32) -> f32 {
        let mut s = 1.0f32;
        if self.grow_time > 0.0 && age < self.grow_time {
            s = s.min(self.base_scale + (1.0 - self.base_scale) * (age / self.grow_time));
        }
        if self.fade_time > 0.0 && life - age < self.fade_time {
            s = s.min(self.base_scale + (1.0 - self.base_scale) * ((life - age) / self.fade_time).max(0.0));
        }
        s
    }
}

/// The particle systems of one file.
#[derive(Debug, Clone, Default)]
pub struct PreparedParticles {
    pub systems: Vec<ParticleSystemDef>,
    /// Block types met among modifiers and controllers that are not read.
    pub unread: Vec<String>,
}

/// An emitter's birth rate and on/off keys as a sequence of the file drives them.
struct SequenceEmitter {
    rate: f32,
    active: Vec<(f32, bool)>,
    period: f32,
    loops: bool,
}

/// The `NiPSysEmitterCtlr` tracks of the first sequence that drives the particle system
/// named `system`: interpolator ID `BirthRate` (a float interpolator's value) and
/// `EmitterActive` (bool keys) [observed: the controlled blocks of
/// `explosiongrenadefrag.nif`'s `SpecialIdle_AreaEffect`, whose emitter controllers
/// carry blend interpolators]; the sequence's stop time is the period, and it repeats
/// when its cycle type is 0 [documented: nif.xml NiControllerSequence, CycleType].
fn sequence_emitter(file: &NifFile, dec: &Decoded, system: &str) -> Option<SequenceEmitter> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "NiControllerSequence" {
            continue;
        }
        let Ok(seq) = &dec.blocks[i] else { continue };
        let mut found = SequenceEmitter { rate: -1.0, active: Vec::new(), period: seq.float("stop_time").unwrap_or(0.0) - seq.float("start_time").unwrap_or(0.0), loops: seq.int("cycle_type").unwrap_or(0) == 0 };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() != Some(system) || text(cb, "controller_type").as_deref() != Some("NiPSysEmitterCtlr") {
                continue;
            }
            let interp = cb.block_ref("interpolator").unwrap_or(-1);
            match text(cb, "interpolator_id").as_deref() {
                Some("BirthRate") => {
                    let v = dec.get(interp).and_then(|s| s.float("value")).unwrap_or(0.0);
                    if v.is_finite() && v > -1e30 {
                        found.rate = v.max(0.0);
                    }
                }
                Some("EmitterActive") => {
                    found.active = fnv_nif::anim::bool_track(dec, interp).map(|t| t.keys).unwrap_or_default();
                }
                _ => {}
            }
        }
        if found.rate >= 0.0 {
            return Some(found);
        }
    }
    None
}

/// The bool keys a sequence of the file gives the `NiPSysModifierActiveCtlr` of the
/// modifier named `modifier` of the particle system named `system` (controlled blocks
/// whose controller ID is the modifier's name [observed: `explosiongrenadefrag.nif`]).
fn sequence_modifier_active(file: &NifFile, dec: &Decoded, system: &str, modifier: &str) -> Option<Vec<(f32, bool)>> {
    let text = |s: &Struct, k: &str| s.text(k).map(str::to_string);
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "NiControllerSequence" {
            continue;
        }
        let Ok(seq) = &dec.blocks[i] else { continue };
        for cb in seq.list("controlled_blocks").unwrap_or_default() {
            let fnv_nif::value::Value::Struct(cb) = cb else { continue };
            if text(cb, "node_name").as_deref() == Some(system)
                && text(cb, "controller_type").as_deref() == Some("NiPSysModifierActiveCtlr")
                && text(cb, "controller_id").as_deref() == Some(modifier)
            {
                let keys = fnv_nif::anim::bool_track(dec, cb.block_ref("interpolator").unwrap_or(-1)).map(|t| t.keys).unwrap_or_default();
                return Some(keys);
            }
        }
    }
    None
}

fn affine(t: &NifTransform) -> Affine3A {
    let r = &t.rotation;
    let m = Mat3::from_cols(Vec3::new(r[0], r[3], r[6]), Vec3::new(r[1], r[4], r[7]), Vec3::new(r[2], r[5], r[8]));
    Affine3A::from_mat3_translation(m * t.scale, Vec3::from_array(t.translation))
}

fn rgba(s: &Struct, key: &str) -> [f32; 4] {
    s.floats(key).filter(|c| c.len() >= 4).map_or([1.0; 4], |c| [c[0], c[1], c[2], c[3]])
}

/// A mesh with the attributes a particle mesh has, for the material to be built against.
pub fn particle_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
    m.insert_indices(bevy::mesh::Indices::U32(vec![0, 1, 2]));
    m
}

impl ModelCtx<'_> {
    /// The particle systems of the file at `path` (read on first use; a failure is
    /// remembered).
    pub fn particles(&mut self, path: &str) -> Option<Arc<PreparedParticles>> {
        let key = fnv_vfs::normalize(path);
        if let Some(p) = self.cache.particles.get(&key) {
            return p.clone();
        }
        let p = match self.prepare_particles(&key) {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                self.cache.failures.push(format!("{key}: {e}"));
                None
            }
        };
        self.cache.particles.insert(key, p.clone());
        p
    }

    fn prepare_particles(&mut self, path: &str) -> Result<PreparedParticles, String> {
        let bytes = self.data.read(path).ok_or("not in any archive")?;
        let schema = self.data.schema.clone();
        let file = NifFile::parse(&bytes, Some(&schema)).map_err(|e| e.to_string())?;
        let dec = Decoded::new(&file, &bytes, &schema);
        let objects = scene_objects(&file, &dec);
        let by_block: HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
        // A node's place in the file, the root's own transform left out.
        let place = |block: i32| -> Affine3A {
            let mut m = Affine3A::IDENTITY;
            let mut at = usize::try_from(block).ok().and_then(|b| by_block.get(&b));
            let mut depth = 0;
            while let Some(o) = at.filter(|o| o.parent.is_some()) {
                m = affine(&o.transform) * m;
                at = o.parent.and_then(|p| by_block.get(&p));
                depth += 1;
                if depth > 256 {
                    break;
                }
            }
            m
        };
        let type_of = |r: i32| usize::try_from(r).ok().and_then(|i| file.blocks.get(i)).map(|b| b.type_name.as_str());
        let mut out = PreparedParticles::default();
        for (i, b) in file.blocks.iter().enumerate() {
            if b.type_name != "NiParticleSystem" {
                continue;
            }
            let Ok(ps) = &dec.blocks[i] else { continue };
            // Its properties, with those of the nodes above it.
            let parent = objects.iter().find(|o| o.is_node && o.children.contains(&i)).map(|o| o.block);
            let fake = SceneObject { block: i, type_name: b.type_name.clone(), name: None, parent, transform: NifTransform::IDENTITY, is_node: false, children: Vec::new() };
            let props = shape_props(&file, &dec, &fake, &by_block);
            let mut template = particle_mesh();
            let (material, _missing) = shape_material(&props, &mut template, &mut self.textures, &self.data, &mut self.images, self.formats.as_deref());
            let material = self.materials.add(material);
            let data = ps.block_ref("data").and_then(|r| dec.get(r));
            let mut def = ParticleSystemDef {
                name: ps.text("name").unwrap_or_default().to_string(),
                max: data.and_then(|d| d.int("bs_max_vertices")).unwrap_or(0).max(0) as usize,
                material,
                emitter: Emitter {
                    frame: Affine3A::IDENTITY,
                    size: Vec3::ZERO,
                    round: false,
                    speed: 0.0,
                    speed_variation: 0.0,
                    declination: 0.0,
                    declination_variation: 0.0,
                    planar_angle: 0.0,
                    planar_angle_variation: 0.0,
                    color: [1.0; 4],
                    radius: 1.0,
                    radius_variation: 0.0,
                    life: 1.0,
                    life_variation: 0.0,
                },
                rate: 0.0,
                active: Vec::new(),
                period: 0.0,
                loops: true,
                max_emitters: 0,
                grow_time: 0.0,
                fade_time: 0.0,
                base_scale: 1.0,
                color: None,
                rotation: None,
                gravity: Vec::new(),
                drag: Vec::new(),
                subtextures: data
                    .and_then(|d| d.floats("subtexture_offsets"))
                    .map(|f| f.as_chunks::<4>().0.to_vec())
                    .unwrap_or_default(),
            };
            // The modifiers' on/off keys: a `NiPSysModifierActiveCtlr` in the system's
            // controller chain names its modifier; a blend interpolator leaves the keys to
            // the model's sequence.
            let system_name = def.name.clone();
            let mut switched: HashMap<String, Vec<(f32, bool)>> = HashMap::new();
            {
                let mut c = ps.block_ref("controller").unwrap_or(-1);
                let mut steps = 0;
                while let (Some(ty), Some(ctl)) = (type_of(c), dec.get(c)) {
                    if ty == "NiPSysModifierActiveCtlr" {
                        if let Some(name) = ctl.text("modifier_name").map(str::to_string) {
                            let own = ctl.block_ref("interpolator").and_then(|r| fnv_nif::anim::bool_track(&dec, r)).map(|t| t.keys).unwrap_or_default();
                            let keys = if own.is_empty() { sequence_modifier_active(&file, &dec, &system_name, &name).unwrap_or_default() } else { own };
                            switched.insert(name, keys);
                        }
                    }
                    c = ctl.block_ref("next_controller").unwrap_or(-1);
                    steps += 1;
                    if steps > 64 {
                        break;
                    }
                }
            }
            let modifier_keys = |m: &Struct| m.text("name").and_then(|n| switched.get(n)).cloned().unwrap_or_default();
            let mut has_emitter = false;
            for &r in ps.refs("modifiers").unwrap_or_default() {
                let (Some(ty), Some(m)) = (type_of(r), dec.get(r)) else { continue };
                let f = |key: &str| m.float(key).unwrap_or(0.0);
                if m.get("active").is_some_and(|v| matches!(v, fnv_nif::value::Value::Bool(false))) {
                    continue;
                }
                match ty {
                    "NiPSysBoxEmitter" | "NiPSysSphereEmitter" | "NiPSysCylinderEmitter" => {
                        has_emitter = true;
                        let round = ty != "NiPSysBoxEmitter";
                        def.emitter = Emitter {
                            frame: place(m.block_ref("emitter_object").unwrap_or(-1)),
                            size: if round { Vec3::new(f("radius"), f("radius"), f("height")) } else { Vec3::new(f("width"), f("height"), f("depth")) },
                            round,
                            speed: f("speed"),
                            speed_variation: f("speed_variation"),
                            declination: f("declination"),
                            declination_variation: f("declination_variation"),
                            planar_angle: f("planar_angle"),
                            planar_angle_variation: f("planar_angle_variation"),
                            color: rgba(m, "initial_color"),
                            radius: f("initial_radius"),
                            radius_variation: f("radius_variation"),
                            life: f("life_span"),
                            life_variation: f("life_span_variation"),
                        };
                    }
                    "NiPSysGrowFadeModifier" => {
                        def.grow_time = f("grow_time");
                        def.fade_time = f("fade_time");
                        def.base_scale = f("base_scale");
                    }
                    "BSPSysSimpleColorModifier" => {
                        let c = m.floats("colors").unwrap_or_default();
                        if c.len() >= 12 {
                            def.color = Some(SimpleColor {
                                fade_in: f("fade_in_percent"),
                                fade_out: f("fade_out_percent"),
                                // The schema's names for the four percentages, in file order.
                                color1_end: f("color1_end_percent"),
                                color2_start: f("color1_start_percent"),
                                color2_end: f("color2_end_percent"),
                                color3_start: f("color2_start_percent"),
                                colors: std::array::from_fn(|k| [c[k * 4], c[k * 4 + 1], c[k * 4 + 2], c[k * 4 + 3]]),
                            });
                        }
                    }
                    "NiPSysRotationModifier" => {
                        def.rotation = Some(Rotation {
                            speed: f("rotation_speed"),
                            speed_variation: f("rotation_speed_variation"),
                            angle: f("rotation_angle"),
                            angle_variation: f("rotation_angle_variation"),
                            random_sign: matches!(m.get("random_rot_speed_sign"), Some(fnv_nif::value::Value::Bool(true))),
                        });
                    }
                    "NiPSysGravityModifier" => {
                        let frame = place(m.block_ref("gravity_object").unwrap_or(-1));
                        let axis = m.floats("gravity_axis").filter(|a| a.len() >= 3).map_or(Vec3::Z, |a| Vec3::new(a[0], a[1], a[2]));
                        def.gravity.push(Gravity {
                            direction: frame.transform_vector3(axis).normalize_or_zero(),
                            origin: frame.translation.into(),
                            strength: f("strength"),
                            spherical: m.int("force_type").unwrap_or(0) == 1,
                            active: ModifierActive { keys: modifier_keys(m) },
                        });
                    }
                    "NiPSysDragModifier" => {
                        let frame = place(m.block_ref("drag_object").unwrap_or(-1));
                        let axis = m.floats("drag_axis").filter(|a| a.len() >= 3).map_or(Vec3::X, |a| Vec3::new(a[0], a[1], a[2]));
                        def.drag.push(Drag { axis: frame.transform_vector3(axis).normalize_or_zero(), percentage: f("percentage"), active: ModifierActive { keys: modifier_keys(m) } });
                    }
                    // Ageing, moving and bounds are what any simulation does; a spawn
                    // modifier only acts when the age-death modifier spawns on death.
                    "NiPSysAgeDeathModifier" | "NiPSysPositionModifier" | "NiPSysBoundUpdateModifier" | "NiPSysSpawnModifier" => {}
                    other => {
                        if !out.unread.iter().any(|u| u == other) {
                            out.unread.push(other.to_string());
                        }
                    }
                }
            }
            // The controllers: the emitter's birth rate and when it is on.
            let mut c = ps.block_ref("controller").unwrap_or(-1);
            let mut steps = 0;
            while let (Some(ty), Some(ctl)) = (type_of(c), dec.get(c)) {
                match ty {
                    "BSPSysMultiTargetEmitterCtlr" | "NiPSysEmitterCtlr" => {
                        let rate = ctl.block_ref("interpolator").and_then(|r| dec.get(r)).and_then(|s| s.float("value")).unwrap_or(0.0);
                        // A value at the bottom of the float range stands for "none".
                        def.rate = if rate.is_finite() && rate > -1e30 { rate.max(0.0) } else { 0.0 };
                        def.active = ctl.block_ref("visibility_interpolator").and_then(|r| fnv_nif::anim::bool_track(&dec, r)).map(|t| t.keys).unwrap_or_default();
                        def.period = ctl.float("stop_time").unwrap_or(0.0);
                        // TimeController flags bits 1–2: the cycle type, 0 loops.
                        def.loops = (ctl.int("flags").unwrap_or(0) >> 1) & 3 == 0;
                        def.max_emitters = ctl.int("max_emitters").unwrap_or(0).max(0) as usize;
                    }
                    // Read with the modifiers above.
                    "NiPSysUpdateCtlr" | "NiPSysModifierActiveCtlr" => {}
                    other => {
                        if !out.unread.iter().any(|u| u == other) {
                            out.unread.push(other.to_string());
                        }
                    }
                }
                c = ctl.block_ref("next_controller").unwrap_or(-1);
                steps += 1;
                if steps > 64 {
                    break;
                }
            }
            // An emitter controller of a model's own sequence leaves its rate and on/off
            // keys to the sequence (its interpolators are blend interpolators).
            if def.rate <= 0.0 || def.active.is_empty() {
                if let Some(se) = sequence_emitter(&file, &dec, &def.name) {
                    if def.rate <= 0.0 {
                        def.rate = se.rate;
                    }
                    if def.active.is_empty() {
                        def.active = se.active;
                    }
                    def.period = se.period;
                    def.loops = se.loops;
                }
            }
            if has_emitter && def.max > 0 {
                out.systems.push(def);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> ParticleSystemDef {
        ParticleSystemDef {
            name: String::new(),
            max: 8,
            material: Handle::default(),
            emitter: Emitter {
                frame: Affine3A::IDENTITY,
                size: Vec3::ZERO,
                round: false,
                speed: 0.0,
                speed_variation: 0.0,
                declination: 0.0,
                declination_variation: 0.0,
                planar_angle: 0.0,
                planar_angle_variation: 0.0,
                color: [1.0; 4],
                radius: 1.0,
                radius_variation: 0.0,
                life: 1.0,
                life_variation: 0.0,
            },
            rate: 10.0,
            active: vec![(0.0, true), (0.5, false)],
            period: 1.0,
            loops: true,
            max_emitters: 2,
            grow_time: 0.5,
            fade_time: 0.25,
            base_scale: 0.2,
            color: None,
            rotation: None,
            gravity: Vec::new(),
            drag: Vec::new(),
            subtextures: Vec::new(),
        }
    }

    #[test]
    fn emitter_keys_repeat_and_sizes_grow_and_fade() {
        let d = def();
        assert!(d.emitting(0.0) && d.emitting(0.4) && !d.emitting(0.6) && d.emitting(1.2) && !d.emitting(1.7));
        let once = ParticleSystemDef { loops: false, ..def() };
        assert!(once.emitting(0.2) && !once.emitting(1.2));
        // Grows from the base scale to full size, then back down before it dies.
        assert!((d.scale(0.0, 2.0) - 0.2).abs() < 1e-6);
        assert!((d.scale(0.25, 2.0) - 0.6).abs() < 1e-6);
        assert_eq!(d.scale(1.0, 2.0), 1.0);
        assert!((d.scale(1.875, 2.0) - 0.6).abs() < 1e-6);
        assert!((d.scale(2.0, 2.0) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn a_switched_modifier_acts_while_its_keys_say_on() {
        // The frag grenade's smoke drags: on from 0, off at 0.33 s, clamped (5.03 s).
        let drag = ModifierActive { keys: vec![(0.0, true), (0.333, false), (5.033, false)] };
        assert!(drag.on(0.0, 5.033, false) && drag.on(0.3, 5.033, false));
        assert!(!drag.on(0.4, 5.033, false) && !drag.on(9.0, 5.033, false));
        // Looping keys repeat after the period.
        assert!(drag.on(5.1, 5.033, true));
        // No keys: always on.
        assert!(ModifierActive::default().on(3.0, 0.0, false));
    }

    #[test]
    fn a_burst_gives_its_particles_whatever_the_frame() {
        // On for 0.00001 s at the start, once (an explosion's burst).
        let burst = ParticleSystemDef { active: vec![(0.0, true), (1e-5, false), (5.0, false)], period: 5.0, loops: false, ..def() };
        assert!((burst.on_time(0.0, 0.016) - 1e-5).abs() < 1e-9);
        assert!((burst.on_time(0.0, 0.1) - 1e-5).abs() < 1e-9);
        assert_eq!(burst.on_time(0.016, 0.032), 0.0);
        // The looping keys of `def`: on, off at 0.5 of a period of 1.
        let d = def();
        assert!((d.on_time(0.25, 1.25) - 0.5).abs() < 1e-5);
        assert!((d.on_time(0.0, 2.0) - 1.0).abs() < 1e-5);
        let always = ParticleSystemDef { active: Vec::new(), ..def() };
        assert!((always.on_time(3.0, 3.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn simple_colour_over_a_life() {
        let c = SimpleColor { fade_in: 0.1, fade_out: 0.8, color1_end: 0.2, color2_start: 0.4, color2_end: 0.6, color3_start: 1.0, colors: [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]] };
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert_eq!(c.at(0.15), [1.0, 0.0, 0.0, 1.0]);
        assert!(near(c.at(0.3), [0.5, 0.5, 0.0, 1.0]), "{:?}", c.at(0.3));
        assert_eq!(c.at(0.5), [0.0, 1.0, 0.0, 1.0]);
        // Half way from the second colour to the third, and half faded out.
        let late = c.at(0.9);
        assert!((late[1] - 0.25).abs() < 1e-5 && (late[2] - 0.75).abs() < 1e-5 && (late[3] - 0.5).abs() < 1e-5);
        assert!((c.at(0.05)[3] - 0.5).abs() < 1e-6);
        // All four percentages 0 (the dirt dust): the second colour throughout.
        let dust = SimpleColor { fade_in: 0.0, fade_out: 1.0, color1_end: 0.0, color2_start: 0.0, color2_end: 0.0, color3_start: 0.0, ..c };
        assert_eq!(dust.at(0.0), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(dust.at(0.7), [0.0, 1.0, 0.0, 1.0]);
    }
}
