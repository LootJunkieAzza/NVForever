//! Impact decals (docs/spec-notes/impact-decals.md): the mark a shot leaves where it
//! lands, a bullet hole in a wall, a scorch on the ground, blood on a body. Every
//! [`ImpactFx`] whose IPCT has a texture set and decal data (the IPCT's own `DODT`, else
//! the texture set's: `fnv_combat::fx::FxForms::decal`) leaves one.
//!
//! - **Drawn as a forward decal** (Bevy's `ForwardDecal`): a square lying on the surface
//!   at the point, facing along the surface's normal, that shows on whatever the depth
//!   prepass holds within the decal data's depth of its plane, fading towards that depth
//!   [hypothesis: "Depth: How far from the point of impact the decal renders",
//!   documented: GECK wiki ImpactData]. Only the main camera draws it ([`DECAL_LAYER`]):
//!   it needs the camera's depth prepass.
//! - **Its look**: one quarter of the texture set's base image, picked at random (the
//!   decal textures hold four marks in a 2 × 2 grid) [hypothesis, impact-decals.md §2];
//!   times the decal colour; the texture set's normal map; specular exponent the decal's
//!   shininess, none with "No Specular Map". Turned about the normal at random [design].
//! - **Its size**: a square, its side between the decal data's min and max width
//!   (`DecalData::side`) [design, impact-decals.md §2].
//! - **On an actor** it lies on the drawn body, not where the shot met the actor's hit
//!   volume: the shot's line is cast against the actor's drawn shapes as they are posed
//!   now (skinned on the CPU, [`ray_shape`]), and the decal goes where the line first
//!   meets one, facing along that triangle's normal [design: the owner's playtest list 7
//!   item 27, the decals floated off the bodies and turned wrong, as they lay on the hit
//!   capsules]. A line that meets no drawn shape leaves no decal. It hangs from the
//!   actor's skeleton bone nearest that point, so it moves with the body
//!   (Fallout_default.ini `bDecalsOnSkinnedGeometry=1`); with no skeleton drawn (the player
//!   in first person) there is none. Fewer than the game's [design: the owner, "tone
//!   down"]: at most [`MAX_PER_ACTOR`] alive on one actor, one each [`ACTOR_GAP`]
//!   seconds, no wider than [`ACTOR_MAX_SIDE`] and thin ([`ACTOR_DEPTH`]) so a flat mark
//!   does not show past a limb's curve. Elsewhere it hangs from the drawn cell it lies
//!   in, none outside the drawn cells.
//! - **Not kept** (the owner, 2026-10-20: "make sure the scorch marks and bullet holes
//!   arent persistent i dont want to jam up the engine remembering all these"): decals
//!   are only drawn entities, never saved nor remembered by a cell; they go with their
//!   cell when it unloads (or their actor), and at most [`MAX_DECALS`] are alive at once,
//!   the oldest going first.
//! - **How many and how long** (impact-decals.md §3): at most [`MAX_DECALS_PER_FRAME`]
//!   new ones a frame; each stays [`DECAL_LIFETIME`] seconds, fading out over the last
//!   [`DECAL_FADE`]; none within the IPCT's placement radius of a live decal of the same
//!   texture set [hypothesis: "Placement Radius: Affects the distance between decals"].
//! - Not built: the angle threshold, parallax, alpha testing (every decal is blended),
//!   the height range, decals on water.

use bevy::camera::visibility::RenderLayers;
use bevy::math::{Affine2, Affine3A};
use bevy::pbr::decal::{ForwardDecal, ForwardDecalMaterial, ForwardDecalMaterialExt};
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use nvf_assets::model::ModelCtx;
use nvf_play::combat::{Combat, ImpactFx};
use nvf_play::sfx::Sfx;
use bevy::mesh::skinning::SkinnedMesh;
use nvf_assets::actor::{skin_weights, LimbShape};
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::limbs::ActorRig;
use crate::world::{CellEntity, Streamer, WorldRoot};

/// The render layer of decals: drawn by the main camera only, the one with a depth
/// prepass (main.rs `setup`).
pub const DECAL_LAYER: usize = 12;

/// How many decals are alive at most, how many are placed a frame at most, how long one
/// stays and how long it takes to fade out, seconds [design: the owner asked for a small
/// fixed cap and a limited life, 2026-10-20; the game's `Fallout_default.ini` has
/// `[Decals] uMaxDecalCount=100`, `[Display] iMaxDecalsPerFrame=10` and
/// `fDecalLifetime=10.0000` (observed; their meanings read from the names): the cap is
/// kept below the game's, the frame cap and the life are its].
pub const MAX_DECALS: usize = 64;
pub const MAX_DECALS_PER_FRAME: usize = 10;
pub const DECAL_LIFETIME: f32 = 10.0;
pub const DECAL_FADE: f32 = 1.0;

/// The thinnest depth a decal shows within, units [design: a decal whose depth is 1
/// (`BloodSpoutRobotData`) would show nowhere on a bumpy surface].
const MIN_DEPTH: f32 = 4.0;

/// On actors (module notes) [design]: the most decals alive on one actor, the least time
/// between two on it (seconds), the deepest a decal shows within (units), the widest one
/// is (units), and how far back along the shot and past the hit the line is cast against
/// the drawn body (units).
pub const MAX_PER_ACTOR: usize = 3;
pub const ACTOR_GAP: f32 = 0.3;
pub const ACTOR_DEPTH: f32 = 4.0;
pub const ACTOR_MAX_SIDE: f32 = 16.0;
const ACTOR_REACH: f32 = 48.0;
/// Hits that met no drawn shape logged one by one, at most.
const MISSES_LOGGED: u32 = 8;

#[derive(Component)]
struct Decal {
    age: f32,
    material: Handle<ForwardDecalMaterial<StandardMaterial>>,
}

/// A live decal: its entity, texture set and where it was put (game space).
struct Live {
    entity: Entity,
    texture_set: FormId,
    point: Vec3,
    /// The actor it lies on.
    actor: Option<u32>,
}

#[derive(Resource)]
struct Decals {
    live: VecDeque<Live>,
    rng: SplitMix,
    /// IPCTs whose first decal was reported.
    reported: HashSet<FormId>,
    placed: u32,
    on_actors: u32,
    near_another: u32,
    /// On an actor flagged No Blood Decal (docs/spec-notes/actor-flags.md §2).
    no_decal: u32,
    over_frame_cap: u32,
    no_body: u32,
    /// Outside the drawn cells.
    no_cell: u32,
    over_cap: u32,
    faded: u32,
    /// Gone with their cell or actor before their time.
    unloaded: u32,
    /// The most alive at once.
    peak: usize,
    /// Seconds since start, and when each actor last had one.
    clock: f32,
    last_on: HashMap<u32, f32>,
    /// Hits on actors whose line met no drawn shape, and those left out by the actor
    /// limits.
    off_body: u32,
    actor_limit: u32,
}

impl Default for Decals {
    fn default() -> Self {
        Decals {
            live: VecDeque::new(),
            rng: SplitMix(0xDEC0),
            reported: HashSet::new(),
            placed: 0,
            on_actors: 0,
            near_another: 0,
            no_decal: 0,
            over_frame_cap: 0,
            no_body: 0,
            no_cell: 0,
            over_cap: 0,
            faded: 0,
            unloaded: 0,
            peak: 0,
            clock: 0.0,
            last_on: HashMap::new(),
            off_body: 0,
            actor_limit: 0,
        }
    }
}

pub struct DecalsPlugin;

impl Plugin for DecalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Decals>()
            .add_systems(Update, (place, fade).chain().after(crate::shot_fx::ShotFxSet).run_if(resource_exists::<Combat>))
            .add_systems(Last, report);
    }
}

/// The texture path as the texture cache takes it.
fn texture_path(t: &str) -> String {
    let path = t.replace('/', "\\").to_ascii_lowercase();
    if path.starts_with("textures\\") {
        path
    } else {
        format!("textures\\{path}")
    }
}

/// The UV transform showing quarter `k` (0..4) of a 2 × 2 grid.
pub fn quarter(k: u32) -> Affine2 {
    Affine2::from_scale_angle_translation(Vec2::splat(0.5), 0.0, Vec2::new((k % 2) as f32 * 0.5, (k / 2 % 2) as f32 * 0.5))
}

/// An actor's drawn shape, for the line cast onto the body.
type DrawnShape = (&'static RefVisual, &'static LimbShape, &'static Mesh3d, Option<&'static SkinnedMesh>, &'static GlobalTransform, &'static InheritedVisibility);

#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    mut landed: MessageReader<ImpactFx>,
    sfx: Option<Res<Sfx>>,
    mut models: ModelCtx,
    mut materials: ResMut<Assets<ForwardDecalMaterial<StandardMaterial>>>,
    mut st: ResMut<Decals>,
    root: Query<(Entity, &GlobalTransform), With<WorldRoot>>,
    (rigs, nodes): (Query<&ActorRig>, Query<&GlobalTransform>),
    (streamer, cells): (Option<Res<Streamer>>, Query<&Transform, With<CellEntity>>),
    shapes: Query<DrawnShape>,
) {
    let (Some(sfx), Ok((world_root, root_place))) = (sfx, root.single()) else {
        landed.clear();
        return;
    };
    let st = &mut *st;
    let mut this_frame = 0;
    for hit in landed.read() {
        if hit.no_decal {
            st.no_decal += 1;
            continue;
        }
        let Some(impact) = sfx.forms.impacts.get(&hit.impact) else { continue };
        let Some((set, data)) = sfx.forms.decal(impact) else { continue };
        let Some(set_id) = impact.texture_set else { continue };
        let mut point = Vec3::from_array(hit.point);
        let mut normal_game = Vec3::from_array(hit.normal);
        if let Some(actor) = hit.actor {
            let live_on = st.live.iter().filter(|l| l.actor == Some(actor)).count();
            if live_on >= MAX_PER_ACTOR || st.last_on.get(&actor).is_some_and(|t| st.clock - t < ACTOR_GAP) {
                st.actor_limit += 1;
                continue;
            }
            // Onto the drawn body, along the shot (module notes).
            let place = root_place.affine();
            let to_game = place.inverse();
            let shot = place.transform_vector3(Vec3::from_array(hit.dir)).normalize_or_zero();
            let dir = if shot == Vec3::ZERO { -place.transform_vector3(normal_game).normalize_or(Vec3::NEG_Z) } else { shot };
            let from = place.transform_point3(point) - dir * ACTOR_REACH;
            let mut best: Option<(f32, Vec3)> = None;
            let (mut tried, mut nearest) = (0u32, f32::INFINITY);
            let at = place.transform_point3(point);
            let (meshes, binds) = (&models.meshes, &models.bindposes);
            for (r, shape, mesh3d, skin, g, seen) in &shapes {
                if r.0 != actor || !seen.get() {
                    continue;
                }
                // Only a mesh still held in the main world can be read (one extracted to
                // the render world panics on access): else the shape's own CPU copy.
                let drawn = meshes.get(&mesh3d.0).filter(|m| m.asset_usage.contains(bevy::asset::RenderAssetUsages::MAIN_WORLD));
                // A shape drawn without triangles (an exploded part's) is not there.
                if drawn.is_some_and(|m| m.indices().is_some_and(|i| i.is_empty())) {
                    continue;
                }
                let mats: Option<Vec<Mat4>> = match skin {
                    Some(sk) => {
                        let Some(inv) = binds.get(&sk.inverse_bindposes) else { continue };
                        sk.joints.iter().zip(inv.iter()).map(|(&j, ib)| nodes.get(j).ok().map(|jg| jg.to_matrix() * *ib)).collect()
                    }
                    None => Some(vec![g.to_matrix()]),
                };
                let Some(mats) = mats else { continue };
                let mesh = drawn.filter(|m| m.attribute(Mesh::ATTRIBUTE_POSITION).is_some()).unwrap_or(&shape.cpu);
                tried += 1;
                if st.off_body < MISSES_LOGGED {
                    nearest = nearest.min(nearest_vertex(mesh, skin.is_some(), &mats, at).unwrap_or(f32::INFINITY));
                }
                if let Some(h) = ray_shape(mesh, skin.is_some(), &mats, from, dir, 2.0 * ACTOR_REACH) {
                    if best.is_none_or(|b| h.0 < b.0) {
                        best = Some(h);
                    }
                }
            }
            match best {
                Some((t, n)) => {
                    point = to_game.transform_point3(from + dir * t);
                    normal_game = to_game.transform_vector3(n).normalize_or(normal_game);
                }
                // No drawn shape at all (the player in first person): no body, as before.
                None if tried == 0 => {
                    st.no_body += 1;
                    continue;
                }
                None => {
                    if st.off_body < MISSES_LOGGED {
                        info!("decals: a hit on actor {actor:08X} meets none of its {tried} drawn shapes along the shot; the nearest posed vertex is {nearest:.1} from the hit point");
                    }
                    st.off_body += 1;
                    continue;
                }
            }
        }
        if impact.placement_radius > 0.0 && st.live.iter().any(|l| l.texture_set == set_id && l.point.distance(point) < impact.placement_radius) {
            st.near_another += 1;
            continue;
        }
        if this_frame >= MAX_DECALS_PER_FRAME {
            st.over_frame_cap += 1;
            continue;
        }
        // On an actor: its bone nearest the point.
        let bone = match hit.actor {
            None => None,
            Some(actor) => {
                let near = rigs.iter().filter(|r| r.reference == actor).flat_map(|r| {
                    r.nodes.iter().enumerate().filter(|(i, _)| r.skel.names.get(*i).and_then(|n| n.as_deref()).is_some_and(|n| n.to_ascii_lowercase().starts_with("bip01"))).map(|(_, &e)| e)
                });
                let at = root_place.transform_point(point);
                let nearest = near.filter_map(|e| Some((e, nodes.get(e).ok()?))).min_by(|a, b| a.1.translation().distance_squared(at).total_cmp(&b.1.translation().distance_squared(at)));
                match nearest {
                    Some((e, g)) => Some((e, *g)),
                    None => {
                        st.no_body += 1;
                        continue;
                    }
                }
            }
        };
        // Elsewhere: the drawn cell it lies in (exterior by its grid square), which takes
        // it away when it unloads.
        let cell = match (&bone, streamer.as_deref()) {
            (Some(_), _) => None,
            (None, Some(s)) => {
                let key = if s.interior.is_some() { (i32::MIN, i32::MIN) } else { fnv_world::cell_of(point.x, point.y) };
                match s.loaded.get(&key).and_then(|&e| Some((e, cells.get(e).ok()?.translation))) {
                    Some(c) => Some(c),
                    None => {
                        st.no_cell += 1;
                        continue;
                    }
                }
            }
            (None, None) => None,
        };
        let Some(diffuse) = set.diffuse.as_deref() else { continue };
        let a = &mut models;
        let Some(base) = a.textures.get(&texture_path(diffuse), true, &a.data, &mut a.images, a.formats.as_deref()) else {
            if st.reported.insert(hit.impact) {
                warn!("decals: {}: texture {diffuse} not found", impact.edid.as_deref().unwrap_or("?"));
            }
            continue;
        };
        let normal = set.normal.as_deref().and_then(|n| a.textures.get(&texture_path(n), false, &a.data, &mut a.images, a.formats.as_deref()));
        // Specular as a NIF shape's: masked by the normal map's alpha unless it has none
        // (nvf_assets::nif_spawn::specular); none with "No Specular Map".
        let unmasked = normal.as_ref().and_then(|h| a.images.get(h)).is_some_and(nvf_assets::nif_spawn::lacks_alpha);
        let (reflectance, masked) = match (&normal, set.no_specular) {
            (Some(_), false) if unmasked => (nvf_assets::nif_spawn::UNMASKED_SPECULAR, false),
            (Some(_), false) => (2.0, true),
            _ => (0.0, false),
        };
        let [r, g, b] = data.color;
        let material = materials.add(ForwardDecalMaterial {
            base: StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                base_color_texture: Some(base),
                specular_texture: normal.clone().filter(|_| masked),
                normal_map_texture: normal,
                // As the NIF shapes' normal maps (nif_spawn::shape_material).
                flip_normal_map_y: true,
                perceptual_roughness: nvf_assets::nif_spawn::gloss_roughness(data.shininess),
                metallic: 0.0,
                reflectance,
                uv_transform: quarter((st.rng.next_f32() * 4.0) as u32),
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: ForwardDecalMaterialExt { depth_fade_factor: if hit.actor.is_some() { data.depth.clamp(1.0, ACTOR_DEPTH) } else { data.depth.max(MIN_DEPTH) } },
        });
        let side = data.side(st.rng.next_f32());
        let side = if hit.actor.is_some() { side.min(ACTOR_MAX_SIDE) } else { side };
        let normal_dir = normal_game.normalize_or(Vec3::Z);
        let spin = st.rng.next_f32() * std::f32::consts::TAU;
        // The decal's quad lies in its xz plane, facing +y.
        let game_place = Transform { translation: point, rotation: Quat::from_rotation_arc(Vec3::Y, normal_dir) * Quat::from_rotation_y(spin), scale: Vec3::new(side, 1.0, side) };
        let (parent, place) = match bone {
            None => match cell {
                Some((e, origin)) => (e, Transform { translation: point - origin, ..game_place }),
                None => (world_root, game_place),
            },
            Some((e, g)) => {
                let world: Affine3A = root_place.affine() * game_place.compute_affine();
                (e, Transform::from_matrix(Mat4::from(g.affine().inverse() * world)))
            }
        };
        let entity = commands
            .spawn((ForwardDecal, MeshMaterial3d(material.clone()), place, Visibility::default(), RenderLayers::layer(DECAL_LAYER), Decal { age: 0.0, material }, ChildOf(parent)))
            .id();
        if st.reported.insert(hit.impact) {
            info!(
                "decals: {} leaves {} ({}) on {}: side {side:.0} of {:.0}-{:.0}, depth {:.0}",
                impact.edid.as_deref().unwrap_or("?"),
                set.edid.as_deref().unwrap_or("?"),
                diffuse,
                hit.actor.map_or("the world".to_string(), |a| format!("actor {a:08X}")),
                data.min_width.min(data.max_width),
                data.min_width.max(data.max_width),
                data.depth
            );
        }
        this_frame += 1;
        st.placed += 1;
        if bone.is_some() {
            st.on_actors += 1;
        }
        if let Some(a) = hit.actor {
            st.last_on.insert(a, st.clock);
        }
        st.live.push_back(Live { entity, texture_set: set_id, point, actor: hit.actor.filter(|_| bone.is_some()) });
        while st.live.len() > MAX_DECALS {
            if let Some(old) = st.live.pop_front() {
                commands.entity(old.entity).try_despawn();
                st.over_cap += 1;
            }
        }
        st.peak = st.peak.max(st.live.len());
    }
}

/// Age the decals: fade them out over their last [`DECAL_FADE`] seconds, then remove
/// them (their material goes with its last handle). A decal whose cell unloaded or whose
/// actor went is forgotten.
fn fade(
    mut commands: Commands,
    time: Res<Time>,
    mut st: ResMut<Decals>,
    mut decals: Query<(Entity, &mut Decal)>,
    mut materials: ResMut<Assets<ForwardDecalMaterial<StandardMaterial>>>,
) {
    let dt = time.delta_secs();
    st.clock += dt;
    let clock = st.clock;
    st.last_on.retain(|_, t| clock - *t < ACTOR_GAP);
    let mut gone = Vec::new();
    for (e, mut d) in &mut decals {
        d.age += dt;
        let left = DECAL_LIFETIME - d.age;
        if left <= 0.0 {
            commands.entity(e).try_despawn();
            gone.push(e);
        } else if left < DECAL_FADE {
            if let Some(mut m) = materials.get_mut(&d.material) {
                m.base.base_color.set_alpha(left / DECAL_FADE);
            }
        }
    }
    st.faded += gone.len() as u32;
    let alive: HashSet<Entity> = decals.iter().map(|(e, _)| e).collect();
    let before = st.live.len();
    st.live.retain(|l| alive.contains(&l.entity));
    st.unloaded += (before - st.live.len()) as u32;
    st.live.retain(|l| !gone.contains(&l.entity));
}

fn report(mut exit: MessageReader<AppExit>, st: Res<Decals>) {
    let exit_seen = exit.read().last().is_some();
    if exit_seen && st.placed + st.near_another + st.no_body + st.no_cell > 0 {
        info!(
            "decals summary: {} placed ({} on actors), at most {} alive (cap {MAX_DECALS}), {} still alive; {} within the placement radius of another, {} over the {MAX_DECALS_PER_FRAME} a frame, {} on actors without a skeleton drawn, {} outside the drawn cells; {} removed at the cap, {} faded out after {DECAL_LIFETIME} s, {} gone with their cell or actor",
            st.placed, st.on_actors, st.peak, st.live.len(), st.near_another, st.over_frame_cap, st.no_body, st.no_cell, st.over_cap, st.faded, st.unloaded
        );
    }
    if exit_seen && st.off_body + st.actor_limit > 0 {
        info!(
            "decals: on actors, {} hits whose line met no drawn shape left none, {} were left out by the actor limits ({MAX_PER_ACTOR} alive, one each {ACTOR_GAP} s)",
            st.off_body, st.actor_limit
        );
    }
    if exit_seen && st.no_decal > 0 {
        info!("decals: {} hits on actors flagged No Blood Decal left none", st.no_decal);
    }
}

/// Where the line from `from` along unit `dir` first meets a drawn shape within `reach`
/// (Bevy world space): the distance along it and the met triangle's normal, turned
/// against the line. A skinned shape is posed by `mats` (each joint's world matrix times
/// its inverse bind pose), one without skin by `mats[0]` (its world matrix).
pub fn ray_shape(mesh: &Mesh, skinned: bool, mats: &[Mat4], from: Vec3, dir: Vec3, reach: f32) -> Option<(f32, Vec3)> {
    let posed = posed(mesh, skinned, mats)?;
    let mut best: Option<(f32, Vec3)> = None;
    let mut tri = |a: usize, b: usize, c: usize| {
        let (Some(&a), Some(&b), Some(&c)) = (posed.get(a), posed.get(b), posed.get(c)) else { return };
        if let Some(t) = ray_triangle(from, dir, a, b, c).filter(|t| *t >= 0.0 && *t <= reach) {
            if best.is_none_or(|x| t < x.0) {
                let n = (b - a).cross(c - a).normalize_or_zero();
                best = Some((t, if n.dot(dir) > 0.0 { -n } else { n }));
            }
        }
    };
    match mesh.indices() {
        Some(ix) => {
            let ix: Vec<usize> = ix.iter().collect();
            for &[a, b, c] in ix.as_chunks::<3>().0 {
                tri(a, b, c);
            }
        }
        None => {
            for i in (0..posed.len().saturating_sub(2)).step_by(3) {
                tri(i, i + 1, i + 2);
            }
        }
    }
    best
}

/// A shape's vertices as posed (see [`ray_shape`]), Bevy world space.
fn posed(mesh: &Mesh, skinned: bool, mats: &[Mat4]) -> Option<Vec<Vec3>> {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let weights = if skinned { Some(skin_weights(mesh)?) } else { None };
    let posed: Vec<Vec3> = pos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let p = Vec3::from_array(*p);
            match &weights {
                Some((j, w)) => {
                    let (j, w) = (j[i], w[i]);
                    let mut out = Vec3::ZERO;
                    for c in 0..4 {
                        if w[c] > 0.0 {
                            if let Some(m) = mats.get(j[c] as usize) {
                                out += m.transform_point3(p) * w[c];
                            }
                        }
                    }
                    out
                }
                None => mats.first().map_or(p, |m| m.transform_point3(p)),
            }
        })
        .collect();
    Some(posed)
}

/// How far the nearest posed vertex of a shape lies from `p` (the log of a missed body).
pub fn nearest_vertex(mesh: &Mesh, skinned: bool, mats: &[Mat4], p: Vec3) -> Option<f32> {
    posed(mesh, skinned, mats)?.iter().map(|v| v.distance(p)).min_by(f32::total_cmp)
}

/// The distance along the line from `o` along `d` to triangle `a b c`, either side
/// (Moller-Trumbore).
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(e2.dot(q) * inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_meets_the_posed_shape() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
        // A square at z = 0 in its own space, skinned to one joint that lifts it to z = 10.
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-5.0f32, -5.0, 0.0], [5.0, -5.0, 0.0], [5.0, 5.0, 0.0], [-5.0, 5.0, 0.0]]);
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(vec![[0; 4]; 4]));
        m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.0f32, 0.0, 0.0, 0.0]; 4]);
        m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
        let mats = [Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0))];
        let (t, n) = ray_shape(&m, true, &mats, Vec3::new(1.0, 1.0, 50.0), Vec3::NEG_Z, 100.0).expect("met");
        assert!((t - 40.0).abs() < 1e-4);
        assert!((n - Vec3::Z).length() < 1e-4, "{n}");
        // Beside it: nothing.
        assert!(ray_shape(&m, true, &mats, Vec3::new(9.0, 0.0, 50.0), Vec3::NEG_Z, 100.0).is_none());
        // Out of reach.
        assert!(ray_shape(&m, true, &mats, Vec3::new(0.0, 0.0, 50.0), Vec3::NEG_Z, 30.0).is_none());
    }

    #[test]
    fn quarters_cover_the_four_cells() {
        let corners: Vec<Vec2> = (0..4).map(|k| quarter(k).transform_point2(Vec2::ZERO)).collect();
        assert_eq!(corners, vec![Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.0), Vec2::new(0.0, 0.5), Vec2::new(0.5, 0.5)]);
        assert_eq!(quarter(3).transform_point2(Vec2::ONE), Vec2::ONE);
    }
}
