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
//! - **On an actor** it hangs from the actor's skeleton bone nearest the point, so it
//!   moves with the body (Fallout_default.ini `bDecalsOnSkinnedGeometry=1`); with no
//!   skeleton drawn (the player in first person) there is none. Elsewhere it hangs from
//!   the drawn cell it lies in, none outside the drawn cells.
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
use std::collections::{HashSet, VecDeque};

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
}

impl Default for Decals {
    fn default() -> Self {
        Decals { live: VecDeque::new(), rng: SplitMix(0xDEC0), reported: HashSet::new(), placed: 0, on_actors: 0, near_another: 0, over_frame_cap: 0, no_body: 0, no_cell: 0, over_cap: 0, faded: 0, unloaded: 0, peak: 0 }
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
) {
    let (Some(sfx), Ok((world_root, root_place))) = (sfx, root.single()) else {
        landed.clear();
        return;
    };
    let st = &mut *st;
    let mut this_frame = 0;
    for hit in landed.read() {
        let Some(impact) = sfx.forms.impacts.get(&hit.impact) else { continue };
        let Some((set, data)) = sfx.forms.decal(impact) else { continue };
        let Some(set_id) = impact.texture_set else { continue };
        let point = Vec3::from_array(hit.point);
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
            extension: ForwardDecalMaterialExt { depth_fade_factor: data.depth.max(MIN_DEPTH) },
        });
        let side = data.side(st.rng.next_f32());
        let normal_dir = Vec3::from_array(hit.normal).normalize_or(Vec3::Z);
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
        st.live.push_back(Live { entity, texture_set: set_id, point });
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
    if exit.read().last().is_some() && st.placed + st.near_another + st.no_body + st.no_cell > 0 {
        info!(
            "decals summary: {} placed ({} on actors), at most {} alive (cap {MAX_DECALS}), {} still alive; {} within the placement radius of another, {} over the {MAX_DECALS_PER_FRAME} a frame, {} on actors without a skeleton drawn, {} outside the drawn cells; {} removed at the cap, {} faded out after {DECAL_LIFETIME} s, {} gone with their cell or actor",
            st.placed, st.on_actors, st.peak, st.live.len(), st.near_another, st.over_frame_cap, st.no_body, st.no_cell, st.over_cap, st.faded, st.unloaded
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarters_cover_the_four_cells() {
        let corners: Vec<Vec2> = (0..4).map(|k| quarter(k).transform_point2(Vec2::ZERO)).collect();
        assert_eq!(corners, vec![Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.0), Vec2::new(0.0, 0.5), Vec2::new(0.5, 0.5)]);
        assert_eq!(quarter(3).transform_point2(Vec2::ONE), Vec2::ONE);
    }
}
