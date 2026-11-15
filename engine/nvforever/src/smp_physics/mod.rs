//! NV SMP physics, native (item 215; docs/spec-notes/smp-physics.md): cloth, coats and
//! hair rigged with physics bones move with the actor, collide with its body and the
//! ground, and blow in the weather's wind.
//!
//! - Items with SMP data (the `HDT Skinned Mesh Physics Object` XML and the `SMP Bones`
//!   list: the Vanilla NV SMP pack) get the bones the skeleton lacks, made at their bind
//!   pose on their parents [documented: NV SMP Physics README], and the system their XML
//!   describes ([`xml`]).
//! - Hair models and Qwib's poncho, which come with none, are rigged by the generator
//!   ([`gen`]).
//! - Every frame, after the animation and the transform propagation and before the
//!   skinned bounds and the draw [documented: README, "right after New Vegas has
//!   animated every actor and before it draws"], each system steps ([`solver`]) on its
//!   actor's animated pose and its simulated bones are written back.
//! - The settings are `NVSMPPhysics.ini`'s ([`ini`]); the F7 menu is not built yet.
//!
//! Co-op: every machine simulates the actors it draws, a guest its host's player and
//! NPCs as replicated; nothing of the physics is sent (docs/spec-notes/smp-physics.md §7).

pub mod gen;
pub mod ini;
pub mod solver;
pub mod xml;

use bevy::camera::visibility::{DynamicSkinnedMeshBounds, VisibilitySystems};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use nvf_assets::actor::{PartShape, PreparedSkeleton, SmpCandidate};
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::Arc;

use ini::SmpSettings;
use solver::{Bone, Capsule, Env, Link, LinkKind, Rig, State};

/// Gravity in game units/s²: 9.81 m/s² at 70 units per metre (fnv-phys `PhysParams`).
const GRAVITY: f32 = 9.81 * 70.0;
/// A frame where the actor's root moves farther than this has teleported: its systems
/// start over at rest [design].
const TELEPORT: f32 = 150.0;
/// Hair below this height over `Bip01 Head`'s bind place swings (game units) [tuned by
/// eye on CKR's styles: about the ears; the crown and fringe keep their shape].
const HAIR_TOP: f32 = 5.0;
/// The share of a poncho's height, from its top, that stays on the body's weights
/// [tuned by eye on Qwib's poncho: the shoulders hold, the lower half swings].
const DRAPE_HELD: f32 = 0.45;
/// The lightest mass a spring's stiffness is divided by [design: keeps a near-massless
/// bone's spring from becoming rigid].
const MIN_MASS: f32 = 0.01;
/// Collision radius of an item's bone that no collision shape names, rig units [design].
const DEFAULT_RADIUS: f32 = 1.0;

/// The body's capsules on a human skeleton: (bone, bone, radius) [design: radii fitted
/// by eye to the vanilla body at bind; each system shrinks them where its own bones rest
/// closer, see [`fit_capsules`]].
const BODY: &[(&str, &str, f32)] = &[
    ("bip01 pelvis", "bip01 spine1", 9.0),
    ("bip01 spine1", "bip01 spine2", 9.5),
    ("bip01 spine2", "bip01 neck1", 8.0),
    ("bip01 neck1", "bip01 head", 3.6),
    ("bip01 l thigh", "bip01 l calf", 5.6),
    ("bip01 r thigh", "bip01 r calf", 5.6),
    ("bip01 l calf", "bip01 l foot", 3.8),
    ("bip01 r calf", "bip01 r foot", 3.8),
    ("bip01 l upperarm", "bip01 l forearm", 3.4),
    ("bip01 r upperarm", "bip01 r forearm", 3.4),
    ("bip01 l forearm", "bip01 l hand", 2.6),
    ("bip01 r forearm", "bip01 r hand", 2.6),
];
/// The skull: a sphere on `Bip01 Head` this far above it at bind (root space z), radius
/// [design: the vanilla head at bind].
const SKULL_UP: f32 = 7.0;
const SKULL_R: f32 = 8.0;
/// The leg capsules (the thighs' and calves') are scaled by `nv-virtual-legs`' `scale`.
const LEG_CAPSULES: std::ops::Range<usize> = 4..8;

pub struct SmpPhysicsPlugin;

impl Plugin for SmpPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SmpCache>().add_systems(
            PostUpdate,
            (rig_candidates, simulate).chain().after(TransformSystems::Propagate).before(VisibilitySystems::CalculateBounds),
        );
    }
}

/// A generated rig's mesh, its bind poses and its chains, shared by shapes of one mesh.
type GeneratedMesh = (Handle<Mesh>, Handle<SkinnedMeshInverseBindposes>, Arc<gen::Generated>);

/// The settings, the physics XMLs read and the meshes rigged.
#[derive(Resource, Default)]
pub struct SmpCache {
    settings: Option<SmpSettings>,
    systems: HashMap<String, Option<Arc<xml::SystemDef>>>,
    /// Rigged meshes by the mesh they were made from, so a hair model's two layers share.
    meshes: HashMap<AssetId<Mesh>, GeneratedMesh>,
    /// Shapes whose rigging was logged (once each).
    logged: std::collections::HashSet<String>,
    time: f32,
}

impl SmpCache {
    pub fn settings(&self) -> SmpSettings {
        self.settings.unwrap_or_default()
    }
}

/// On a drawn shape that was looked at and has nothing to simulate.
#[derive(Component)]
pub struct SmpNone;

/// A capsule on the body: two skeleton nodes, a point in each one's frame, the radius
/// (rig units).
#[derive(Debug, Clone, Copy)]
struct BodyCapsule {
    a: usize,
    a_off: Vec3,
    b: usize,
    b_off: Vec3,
    r: f32,
}

/// A simulated shape.
#[derive(Component)]
pub struct SmpRig {
    /// The skeleton's root node.
    root: Entity,
    /// Per rig bone, its entity (a skeleton node, or a bone made for the item).
    entities: Vec<Entity>,
    /// The rig bones made for the item, written back every frame.
    made: Vec<usize>,
    rig: Rig,
    state: State,
    /// The skeleton nodes the body capsules sit on.
    nodes: Arc<Vec<Entity>>,
    capsules: Vec<BodyCapsule>,
    last_root: Option<(Vec3, Vec3)>,
    pub label: String,
    /// How often the system blew up and was started over.
    pub blowups: u32,
}

fn lower_index(skel: &PreparedSkeleton) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, n) in skel.names.iter().enumerate() {
        if let Some(n) = n {
            m.entry(n.to_ascii_lowercase()).or_insert(i);
        }
    }
    m
}

fn mesh_positions(mesh: &Mesh) -> Option<Vec<Vec3>> {
    match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(v) => Some(v.iter().map(|p| Vec3::from_array(*p)).collect()),
        _ => None,
    }
}

fn mesh_weights(mesh: &Mesh) -> Option<Vec<Vec<(u16, f32)>>> {
    let (VertexAttributeValues::Uint16x4(j), VertexAttributeValues::Float32x4(w)) = (mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)?, mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)?) else {
        return None;
    };
    Some(j.iter().zip(w).map(|(j, w)| (0..4).filter(|&k| w[k] > 0.0).map(|k| (j[k], w[k])).collect()).collect())
}

/// The body capsules of a skeleton in its rig space, shrunk so that `rest` (the
/// simulated bones' rest places with their radii, rig space) start clear of them, never
/// below 35% of their size; `legs` scales the leg capsules.
fn fit_capsules(skel: &PreparedSkeleton, rest: &[(Vec3, f32)], legs: f32) -> Vec<BodyCapsule> {
    let by = lower_index(skel);
    let mut out = Vec::new();
    let place = |i: usize, root: Vec3| skel.bind_world[i].inverse().transform_point3(root);
    let mut add = |a: usize, pa: Vec3, b: usize, pb: Vec3, r: f32| {
        let c = Capsule { a: pa, b: pb, r };
        let clear = rest.iter().map(|&(p, pr)| (p - c.closest(p)).length() - pr).fold(f32::MAX, f32::min);
        let r = r.min(clear * 0.98).max(r * 0.35);
        out.push(BodyCapsule { a, a_off: place(a, pa), b, b_off: place(b, pb), r });
    };
    for (k, (a, b, r)) in BODY.iter().enumerate() {
        let (Some(&ia), Some(&ib)) = (by.get(*a), by.get(*b)) else { continue };
        let r = if LEG_CAPSULES.contains(&k) { r * legs } else { *r };
        add(ia, skel.bind_world[ia].translation.into(), ib, skel.bind_world[ib].translation.into(), r);
    }
    if let Some(&h) = by.get("bip01 head") {
        let c = Vec3::from(skel.bind_world[h].translation) + Vec3::Z * SKULL_UP;
        add(h, c, h, c, SKULL_R);
    }
    out
}

/// Look at new candidates: rig those with something to simulate, mark the others.
#[allow(clippy::type_complexity)]
fn rig_candidates(
    mut commands: Commands,
    data: Option<Res<GameData>>,
    mut cache: ResMut<SmpCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut binds: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    new: Query<(Entity, &SmpCandidate, &Mesh3d, Option<&SkinnedMesh>), (Without<SmpRig>, Without<SmpNone>)>,
) {
    if new.is_empty() {
        return;
    }
    if cache.settings.is_none() {
        let text = data.as_ref().and_then(|d| d.read(ini::INI_PATH)).map(|b| String::from_utf8_lossy(&b).into_owned());
        let s = text.as_deref().map(SmpSettings::parse).unwrap_or_default();
        info!("smp: settings {} ({s:?})", if text.is_some() { ini::INI_PATH } else { "default (no NVSMPPhysics.ini in the load order)" });
        cache.settings = Some(s);
    }
    for (e, cand, mesh3d, skinned) in &new {
        let shape = &cand.part.shapes[cand.shape];
        let label = format!("{} shape {:?}", cand.part.path, shape.name.as_deref().unwrap_or(""));
        let made = if let Some(smp) = &shape.smp {
            let sys = smp.xml.as_ref().and_then(|x| {
                let path = fnv_vfs::normalize(x);
                cache
                    .systems
                    .entry(path.clone())
                    .or_insert_with(|| {
                        let def = data.as_ref().and_then(|d| d.read(&path)).and_then(|b| xml::system(&String::from_utf8_lossy(&b)));
                        match &def {
                            Some(d) => info!("smp: {path}: {} bones ({} simulated), {} constraints, {} shapes{}", d.bones.len(), d.bones.iter().filter(|b| b.props.mass > 0.0).count(), d.constraints.len(), d.shapes.len(), if d.unknown.is_empty() { String::new() } else { format!("; not handled: {:?}", d.unknown) }),
                            None => warn!("smp: {path}: physics XML missing or unreadable"),
                        }
                        def.map(Arc::new)
                    })
                    .clone()
            });
            sys.and_then(|sys| rig_from_xml(&mut commands, e, cand, shape, &sys, skinned, label.clone()))
        } else {
            let source = meshes.get(&mesh3d.0).cloned();
            source.and_then(|m| rig_generated(&mut commands, &mut cache, &mut meshes, &mut binds, e, cand, shape, &mesh3d.0, &m, label.clone()))
        };
        match made {
            Some(rig) => {
                if cache.logged.insert(label.clone()) {
                    info!("smp: {label}: rigged, {} bones ({} simulated), {} links", rig.rig.bones.len(), rig.rig.dynamic_count(), rig.rig.links.len());
                }
                commands.entity(e).insert(rig);
            }
            None => {
                commands.entity(e).insert(SmpNone);
            }
        }
    }
}

/// The rig of a shape with SMP data: its skin's joints (skeleton nodes, or bones made
/// for it at their bind pose on their `SMP Bones` parent) and its XML's system.
fn rig_from_xml(commands: &mut Commands, e: Entity, cand: &SmpCandidate, shape: &PartShape, sys: &xml::SystemDef, skinned: Option<&SkinnedMesh>, label: String) -> Option<SmpRig> {
    let (Some((bind_handle, names)), Some(ibp)) = (&shape.skin, &shape.inverse_binds) else { return None };
    skinned?;
    let by = lower_index(&cand.skel);
    let parent_of: HashMap<String, String> = cand.part.shapes[cand.shape].smp.as_ref().map(|s| s.bones.iter().map(|(n, p)| (n.to_ascii_lowercase(), p.to_ascii_lowercase())).collect()).unwrap_or_default();
    let slot_of: HashMap<String, usize> = names.iter().enumerate().map(|(i, n)| (n.to_ascii_lowercase(), i)).collect();
    // Rig bones: every skin slot, then the skeleton parents the made bones need.
    enum Src {
        Node(usize),
        Made(usize),
    }
    let mut src: Vec<Src> = names.iter().enumerate().map(|(i, n)| by.get(&n.to_ascii_lowercase()).map_or(Src::Made(i), |&j| Src::Node(j))).collect();
    if !src.iter().any(|s| matches!(s, Src::Made(_))) {
        return None;
    }
    let rest_root: Vec<Affine3A> = ibp.iter().map(|m| Affine3A::from_mat4(m.inverse())).collect();
    // Each made bone's parent: a slot, or a skeleton node (added), or the root.
    let mut parent: Vec<Option<usize>> = vec![None; src.len()];
    let mut extra_nodes: HashMap<usize, usize> = HashMap::new();
    for i in 0..names.len() {
        if !matches!(src[i], Src::Made(_)) {
            continue;
        }
        let p = parent_of.get(&names[i].to_ascii_lowercase()).cloned().unwrap_or_default();
        parent[i] = Some(match (slot_of.get(&p), by.get(&p)) {
            (Some(&s), _) if s != i => s,
            (_, Some(&node)) => *extra_nodes.entry(node).or_insert_with(|| {
                src.push(Src::Node(node));
                src.len() - 1
            }),
            _ => *extra_nodes.entry(0).or_insert_with(|| {
                src.push(Src::Node(0));
                src.len() - 1
            }),
        });
    }
    parent.resize(src.len(), None);
    // A skin slot rests where its inverse bind pose says; an added skeleton parent at its
    // bind place.
    let rest_of = |k: usize, src: &[Src]| match src[k] {
        _ if k < rest_root.len() => rest_root[k],
        Src::Node(j) => cand.skel.bind_world[j],
        Src::Made(i) => rest_root[i],
    };
    // Order: parents first.
    let n = src.len();
    let mut order: Vec<usize> = (0..n).filter(|&k| matches!(src[k], Src::Node(_))).collect();
    let mut placed = vec![false; n];
    for &k in &order {
        placed[k] = true;
    }
    while order.len() < n {
        let before = order.len();
        for k in 0..n {
            if !placed[k] && parent[k].is_none_or(|p| placed[p]) {
                placed[k] = true;
                order.push(k);
            }
        }
        if order.len() == before {
            warn!("smp: {label}: SMP Bones parents loop; not simulated");
            return None;
        }
    }
    let index: Vec<usize> = {
        let mut v = vec![0; n];
        for (ri, &k) in order.iter().enumerate() {
            v[k] = ri;
        }
        v
    };
    let rests: Vec<Affine3A> = order.iter().map(|&k| rest_of(k, &src)).collect();
    let mut entities = Vec::with_capacity(n);
    let mut made = Vec::new();
    let mut bones = Vec::with_capacity(n);
    let shape_radius = |name: &str| sys.shapes.iter().find(|s| s.bones.iter().any(|b| b == name));
    for (ri, &k) in order.iter().enumerate() {
        match src[k] {
            Src::Node(j) => {
                entities.push(cand.nodes[j]);
                bones.push(Bone::driven());
            }
            Src::Made(i) => {
                let name = names[i].to_ascii_lowercase();
                let p = index[parent[k].unwrap_or(0)];
                let rest_local = rests[p].inverse() * rests[ri];
                let def = sys.bone(&name);
                let props = def.map(|d| d.props).unwrap_or_default();
                let sh = shape_radius(&name);
                let dynamic = props.mass > 0.0;
                bones.push(Bone {
                    parent: Some(p),
                    rest_local,
                    driven: false,
                    dynamic,
                    inv_mass: if dynamic { 1.0 / props.mass } else { 0.0 },
                    damping: props.linear_damping,
                    wind: props.wind_factor,
                    radius: sh.map_or(DEFAULT_RADIUS, |s| s.margin),
                    collide_body: sh.is_none_or(|s| !s.no_collide.iter().any(|t| t == "body")),
                    collide_ground: sh.is_none_or(|s| !s.no_collide.iter().any(|t| t == "ground")),
                });
                let be = commands.spawn((Transform::from_matrix(Mat4::from(rests[ri])), Visibility::Inherited, Name::new(names[i].clone()))).id();
                commands.entity(e).add_child(be);
                entities.push(be);
                made.push(ri);
            }
        }
    }
    let rig_index = |name: &str| slot_of.get(name).map(|&s| index[s]);
    let mut links = Vec::new();
    for c in &sys.constraints {
        let (Some(a), Some(b)) = (rig_index(&c.a), rig_index(&c.b)) else { continue };
        // Stiffness per unit of the moving bones' mass (solver::SPRING_SCALE).
        let mass = |i: usize| if bones[i].dynamic { 1.0 / bones[i].inv_mass } else { 0.0 };
        let kind = if c.props.is_pinned() {
            let off = (rests[b].inverse() * rests[a]).translation.into();
            LinkKind::Joint { off, swing: c.props.swing().clamp(0.02, 3.0), k: c.props.angular_stiffness.element_sum() / 3.0 / mass(a).max(MIN_MASS) }
        } else {
            let m = if bones[a].dynamic && bones[b].dynamic { (mass(a) + mass(b)) / 2.0 } else { mass(a).max(mass(b)) };
            LinkKind::Spring { rest: 0.0, slack: c.props.slack(), k: c.props.linear_stiffness.element_sum() / 3.0 / m.max(MIN_MASS) }
        };
        links.push(Link { a, b, kind });
    }
    let rig = Rig::new(bones, links, &rests);
    if rig.dynamic_count() == 0 {
        return None;
    }
    let rest_pts: Vec<(Vec3, f32)> = rig.bones.iter().zip(&rests).filter(|(b, _)| b.dynamic).map(|(b, r)| (Vec3::from(r.translation), b.radius)).collect();
    let capsules = fit_capsules(&cand.skel, &rest_pts, sys.virtual_legs.unwrap_or(1.0));
    // The skin now binds to every slot.
    let joints: Vec<Entity> = (0..names.len()).map(|s| entities[index[s]]).collect();
    commands.entity(e).insert((Transform::IDENTITY, SkinnedMesh { inverse_bindposes: bind_handle.clone(), joints }, DynamicSkinnedMeshBounds));
    Some(SmpRig { root: cand.nodes[0], entities, made, rig, state: State::default(), nodes: cand.nodes.clone(), capsules, last_root: None, label, blowups: 0 })
}

/// The rig the generator makes for a shape with no SMP data (hair models, Qwib's
/// poncho), or `None` when it has nothing long enough to swing.
#[allow(clippy::too_many_arguments)]
fn rig_generated(
    commands: &mut Commands,
    cache: &mut SmpCache,
    meshes: &mut Assets<Mesh>,
    binds: &mut Assets<SkinnedMeshInverseBindposes>,
    e: Entity,
    cand: &SmpCandidate,
    shape: &PartShape,
    mesh_handle: &Handle<Mesh>,
    mesh: &Mesh,
    label: String,
) -> Option<SmpRig> {
    let by = lower_index(&cand.skel);
    let points_local = mesh_positions(mesh)?;
    // (shape's own joints as skeleton nodes, anchor slot, points in root space, own
    // weights, looseness, top, center, anchor bind, params)
    let (own_nodes, anchor_slot, points, own, loose, top, center, anchor_bind, params, own_ibp) = if let Some((b, local)) = cand.rigid {
        // A hair model on its bone: one joint, the bone.
        if !shape.is_hair() {
            return None;
        }
        let bind = cand.skel.bind_world[b];
        let to_root = bind * local;
        let pts: Vec<Vec3> = points_local.iter().map(|p| to_root.transform_point3(*p)).collect();
        let top = bind.translation.z + HAIR_TOP;
        let cap: Vec<Vec3> = pts.iter().copied().filter(|p| p.z > top).collect();
        let center = if cap.is_empty() { Vec2::new(bind.translation.x, bind.translation.y) } else { cap.iter().map(|p| Vec2::new(p.x, p.y)).sum::<Vec2>() / cap.len() as f32 };
        let own = vec![vec![(0u16, 1.0)]; pts.len()];
        let loose = vec![1.0; pts.len()];
        (vec![b], 0usize, pts, own, loose, top, center, bind, gen::HAIR, vec![Mat4::from(local)])
    } else {
        // A skinned accessory: only the poncho [design: the bandana, bags and hats
        // keep their shape].
        if !cand.part.path.starts_with("meshes\\qwib\\poncho\\") {
            return None;
        }
        let (_, names) = shape.skin.as_ref()?;
        let ibp = shape.inverse_binds.as_ref()?;
        let nodes: Option<Vec<usize>> = names.iter().map(|n| by.get(&n.to_ascii_lowercase()).copied()).collect();
        let mut nodes = nodes?;
        let mut own_ibp: Vec<Mat4> = ibp.to_vec();
        let spine = *by.get("bip01 spine2")?;
        let anchor_slot = match nodes.iter().position(|&n| n == spine) {
            Some(s) => s,
            None => {
                nodes.push(spine);
                own_ibp.push(Mat4::from(cand.skel.bind_world[spine].inverse()));
                nodes.len() - 1
            }
        };
        let own = mesh_weights(mesh)?;
        let arms: Vec<bool> = names.iter().map(|n| {
            let n = n.to_ascii_lowercase();
            ["arm", "clavicle", "hand", "finger", "neck", "head"].iter().any(|k| n.contains(k))
        }).collect();
        let loose: Vec<f32> = own.iter().map(|w| 1.0 - (w.iter().filter(|(j, _)| arms.get(*j as usize).copied().unwrap_or(false)).map(|(_, w)| w).sum::<f32>() * 2.5).clamp(0.0, 1.0)).collect();
        let pts = points_local;
        let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        let top = hi - DRAPE_HELD * (hi - lo);
        let bind = cand.skel.bind_world[spine];
        (nodes, anchor_slot, pts, own, loose, top, Vec2::new(bind.translation.x, bind.translation.y), bind, gen::DRAPE, own_ibp)
    };
    let own_joints = own_nodes.len();
    let generated = match cache.meshes.get(&mesh_handle.id()) {
        Some((_, _, g)) => g.clone(),
        None => {
            let g = Arc::new(gen::generate(gen::Shape { points: &points, own: &own, own_joints, looseness: &loose, center, top, anchor: anchor_bind }, params)?);
            let mut m = mesh.clone();
            m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(g.weights.iter().map(|w| [w[0].0, w[1].0, w[2].0, w[3].0]).collect()));
            m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, VertexAttributeValues::Float32x4(g.weights.iter().map(|w| [w[0].1, w[1].1, w[2].1, w[3].1]).collect()));
            let m = m.with_generated_skinned_mesh_bounds().ok()?;
            // Generated bone i: the vertex in root space (the rigid shape's bone bind
            // times its place there, or the skinned mesh as it is) into the bone.
            let to_root = match cand.rigid {
                Some((b, local)) => Mat4::from(cand.skel.bind_world[b] * local),
                None => Mat4::IDENTITY,
            };
            let mut ibps = own_ibp.clone();
            ibps.extend(g.bones.iter().map(|b| Mat4::from(b.rest.inverse()) * to_root));
            let entry = (meshes.add(m), binds.add(SkinnedMeshInverseBindposes::from(ibps)), g.clone());
            cache.meshes.insert(mesh_handle.id(), entry);
            g
        }
    };
    let (new_mesh, new_binds, _) = cache.meshes.get(&mesh_handle.id())?.clone();
    // Rig bones: the shape's own joints (driven), then the generated ones.
    let mut bones: Vec<Bone> = own_nodes.iter().map(|_| Bone::driven()).collect();
    let mut rests: Vec<Affine3A> = own_nodes.iter().map(|&j| cand.skel.bind_world[j]).collect();
    let mut entities: Vec<Entity> = own_nodes.iter().map(|&j| cand.nodes[j]).collect();
    let mut made = Vec::new();
    for gb in &generated.bones {
        let p = gb.parent.map_or(anchor_slot, |q| own_joints + q);
        let rest_local = rests[p].inverse() * gb.rest;
        bones.push(Bone {
            parent: Some(p),
            rest_local,
            driven: false,
            dynamic: gb.dynamic,
            inv_mass: if gb.dynamic { 1.0 / params.mass } else { 0.0 },
            damping: params.damping,
            wind: params.wind,
            radius: params.radius,
            collide_body: true,
            collide_ground: true,
        });
        rests.push(gb.rest);
        let be = commands.spawn((Transform::from_matrix(Mat4::from(gb.rest)), Visibility::Inherited, Name::new("SMP generated"))).id();
        commands.entity(e).add_child(be);
        made.push(entities.len());
        entities.push(be);
    }
    let mut links: Vec<Link> = generated
        .joints
        .iter()
        .map(|&(c, p)| {
            let (a, b) = (own_joints + c, own_joints + p);
            Link { a, b, kind: LinkKind::Joint { off: (rests[b].inverse() * rests[a]).translation.into(), swing: params.swing, k: params.stiffness / params.mass.max(MIN_MASS) } }
        })
        .collect();
    links.extend(generated.ties.iter().map(|&(a, b)| Link { a: own_joints + a, b: own_joints + b, kind: LinkKind::Spring { rest: 0.0, slack: params.tie_slack, k: params.tie_stiffness / params.mass.max(MIN_MASS) } }));
    let rig = Rig::new(bones, links, &rests);
    let rest_pts: Vec<(Vec3, f32)> = rig.bones.iter().zip(&rests).filter(|(b, _)| b.dynamic).map(|(b, r)| (Vec3::from(r.translation), b.radius)).collect();
    let capsules = fit_capsules(&cand.skel, &rest_pts, 1.0);
    commands.entity(cand.nodes[0]).add_child(e);
    commands.entity(e).insert((Mesh3d(new_mesh), Transform::IDENTITY, SkinnedMesh { inverse_bindposes: new_binds, joints: entities.clone() }, DynamicSkinnedMeshBounds));
    Some(SmpRig { root: cand.nodes[0], entities, made, rig, state: State::default(), nodes: cand.nodes.clone(), capsules, last_root: None, label, blowups: 0 })
}

/// The bones' and the actors' transforms; not the cameras, which `simulate` reads too.
type Placed<'w, 's> = Query<'w, 's, (&'static mut Transform, &'static mut GlobalTransform), (Without<SmpRig>, Without<Camera3d>)>;

/// Step every system on its actor's pose and write its bones back.
fn simulate(
    time: Res<Time>,
    mut cache: ResMut<SmpCache>,
    sky: Option<Res<crate::sky::SkyState>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut rigs: Query<&mut SmpRig>,
    mut transforms: Placed<'_, '_>,
    mut reported: Local<bool>,
) {
    let dt = time.delta_secs();
    cache.time += dt;
    let settings = cache.settings();
    let eye = cameras.iter().filter(|(c, _)| c.is_active).map(|(_, g)| g.translation()).next().unwrap_or(Vec3::ZERO);
    // Collision for the nearest actors within range.
    let mut by_root: Vec<(Entity, f32)> = Vec::new();
    for rig in &rigs {
        if let Ok((_, g)) = transforms.get(rig.root) {
            let d = g.translation().distance(eye);
            if !by_root.iter().any(|(e, _)| *e == rig.root) {
                by_root.push((rig.root, d));
            }
        }
    }
    by_root.sort_by(|a, b| a.1.total_cmp(&b.1));
    let colliding: Vec<Entity> = by_root.iter().filter(|(_, d)| *d <= settings.collision_distance).take(settings.max_actors).map(|(e, _)| *e).collect();
    let wind = match (&sky, settings.wind) {
        (Some(sky), true) => {
            let w = crate::tree::wind_vector(sky);
            let t = cache.time;
            let gust = 0.65 + 0.35 * (t * 1.3).sin() * (t * 0.37 + 1.0).sin();
            w.truncate() * w.w * settings.wind_strength / 9.8 * GRAVITY * gust
        }
        _ => Vec3::ZERO,
    };
    let gravity = Vec3::NEG_Y * GRAVITY * settings.gravity_scale;
    let mut steps = 0usize;
    for mut rig in &mut rigs {
        let rig = &mut *rig;
        let Ok((_, root_g)) = transforms.get(rig.root) else { continue };
        let root = root_g.affine();
        let scale = root.matrix3.x_axis.length();
        let driven: Vec<Affine3A> = rig.entities.iter().zip(&rig.rig.bones).map(|(e, b)| if b.driven { transforms.get(*e).map(|(_, g)| g.affine()).unwrap_or(root) } else { Affine3A::IDENTITY }).collect();
        // Teleports and fast turns start over.
        let pos = Vec3::from(root.translation);
        // The root's side axis, flattened: it turns with the actor whichever way its
        // forward axis points (the side axis is x in both the game and Bevy frames).
        let fwd = Vec3::from(root.matrix3.x_axis).with_y(0.0).normalize_or_zero();
        if let Some((p0, f0)) = rig.last_root {
            let turn = if dt > 0.0 { f0.angle_between(fwd).to_degrees() / dt } else { 0.0 };
            if pos.distance(p0) > TELEPORT || (f0 != Vec3::ZERO && fwd != Vec3::ZERO && turn > settings.reset_angle) {
                rig.state.live = false;
            }
        }
        rig.last_root = Some((pos, fwd));
        if !settings.enabled {
            rig.state.reset(&rig.rig, &driven);
        } else {
            let collide = settings.collisions && colliding.contains(&rig.root);
            let capsules: Vec<Capsule> = if collide {
                rig.capsules
                    .iter()
                    .filter_map(|c| {
                        let a = transforms.get(rig.nodes[c.a]).ok()?.1.transform_point(c.a_off);
                        let b = transforms.get(rig.nodes[c.b]).ok()?.1.transform_point(c.b_off);
                        Some(Capsule { a, b, r: c.r * scale })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let env = Env { gravity, wind, capsules: &capsules, ground: (collide && settings.ground).then_some(root.translation.y), scale, iterations: settings.iterations, min_fps: settings.min_fps, max_substeps: settings.max_substeps };
            if rig.state.step(&rig.rig, &driven, &env, dt) {
                rig.blowups += 1;
                // The first and then every hundredth: a mesh that keeps blowing up is a bug.
                if rig.blowups % 100 == 1 {
                    warn!("smp: {}: blew up, started over at rest ({} times)", rig.label, rig.blowups);
                }
            }
            steps += 1;
        }
        let inv_root = root.inverse();
        for &i in &rig.made {
            let Some(w) = rig.state.world.get(i).copied() else { continue };
            if let Ok((mut t, mut g)) = transforms.get_mut(rig.entities[i]) {
                *t = Transform::from_matrix(Mat4::from(inv_root * w));
                *g = GlobalTransform::from(w);
            }
        }
    }
    if steps > 0 && !*reported {
        *reported = true;
        info!("smp: simulating {steps} system(s), {} actor(s) colliding", colliding.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bevy checks the systems' queries for conflicting access only when they first run:
    /// run them once on an empty world (a camera and an empty rig included).
    #[test]
    fn the_systems_run_without_conflicting_queries() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<SmpCache>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<SkinnedMeshInverseBindposes>>();
        world.spawn((Camera3d::default(), Camera::default(), Transform::IDENTITY, GlobalTransform::IDENTITY));
        let mut schedule = Schedule::default();
        schedule.add_systems((rig_candidates, simulate).chain());
        schedule.run(&mut world);
        schedule.run(&mut world);
    }
}
