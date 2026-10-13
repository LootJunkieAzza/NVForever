//! A debug probe for shadows cast where nothing stands (playtest list 4 item 5, the
//! first-person shadow flicker): `NVF_PLAY_DEBUG_SHADOWS=1` (off by default). Every
//! [`EVERY`] frames it checks what the sun casts from:
//!
//! - **Stretched skins**: a skinned shape drawn (in a view or a shadow cascade) whose
//!   joints lie further apart than [`SPREAD`] game units: a skin bound to bones of two
//!   places stretches between them.
//! - **Big casters near the camera**: skinned shapes in the sun's first two cascades of
//!   the main camera whose bounds are wider than [`SPREAD`].
//! - **Skin slots** (render world): a skinned shape whose instance data on the GPU names
//!   another slot of the joint buffer than the one its skin has now (Bevy places a skin
//!   anew when it is shown again, and a shape not re-read keeps the old slot: it is then
//!   drawn with whatever skin has that slot since, round that skin's bones).
//! - **Lost joints**: a skin whose joint entities are gone or stand at the world's
//!   origin. Bevy writes a missing joint as the identity, so the vertices bound to it are
//!   drawn at the origin (game 0, 0, 0) and the shape stretches from where it stands to
//!   there. [hypothesis: the owner's three screenshots all look east from Goodsprings,
//!   towards the origin, and the wedge narrows that way.]
//! - **Moved meshes** (render world): an instance drawn from where its mesh's vertices
//!   or indices were before the mesh was changed and placed anew (garbage triangles).
//! - **The player's body**: the widest joint spread of the third-person body, which in
//!   first person only casts the shadow (`player_body.rs`), in each summary.
//!
//! Each finding is logged once per shape (`shadow probe: …`), with a summary every five
//! seconds: `shadow probe: <n> skins; stretched … player body joints within <units>`.

use crate::sky::Sun;
use crate::world::bevy_to_game;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{CascadesVisibleEntities, RenderLayers};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::{MeshInputUniform, MeshUniform, RenderMeshInstances, SkinUniforms};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::batching::gpu_preprocessing::BatchedInstanceBuffers;
use bevy::render::sync_world::MainEntity;
use bevy::render::{Render, RenderApp, RenderSystems};
use std::sync::{Arc, Mutex};

/// Frames between checks.
const EVERY: u32 = 10;
/// Joints or bounds further apart than this (game units) are flagged: a body is about
/// 130 units tall, the largest creatures some 800.
const SPREAD: f32 = 1500.0;

/// Whether the probe is on (`NVF_PLAY_DEBUG_SHADOWS`).
pub fn enabled() -> bool {
    nvf_play::dialogue::debug_switch(std::env::var("NVF_PLAY_DEBUG_SHADOWS").ok().as_deref())
}

/// Skin slot findings from the render world: (shape, slot named by its instance data,
/// slot of its skin now).
#[derive(Resource, Clone, Default)]
struct SlotFindings(Arc<Mutex<Found>>);

/// What the render world found since the main world last looked.
#[derive(Default)]
struct Found {
    /// Skinned instances checked in the last check.
    checked: u32,
    slots: Vec<Finding>,
    moved: Vec<Finding>,
}

/// A shape and two numbers: the slot or vertex it is drawn with, and its skin's or mesh's.
type Finding = (Entity, u32, u32);

/// Names, parents, layers and places, to say where a shape is.
type Names<'w, 's> = Query<'w, 's, (Option<&'static Name>, Option<&'static ChildOf>, Option<&'static RenderLayers>, Option<&'static GlobalTransform>)>;

#[derive(Resource, Default)]
struct Probe {
    frame: u32,
    reported: HashSet<Entity>,
    since_summary: f32,
    stretched: u32,
    big: u32,
    slots: u32,
    moved: u32,
    lost_joints: u32,
    checked_slots: u32,
    /// Whether a skinned shape belongs to the player's third-person body.
    player_skin: HashMap<Entity, bool>,
    /// The widest joint spread of the player's third-person body since the last summary.
    player_spread: f32,
}

/// Whether `e` hangs under the player's third-person body (`player_body.rs` names it).
fn under_player_body(e: Entity, names: &Names) -> bool {
    let mut at = Some(e);
    for _ in 0..40 {
        let Some(x) = at else { break };
        let Ok((name, parent, ..)) = names.get(x) else { break };
        if name.is_some_and(|n| n.as_str().starts_with("Player body (Third")) {
            return true;
        }
        at = parent.map(|p| p.parent());
    }
    false
}

pub struct ShadowProbePlugin;

impl Plugin for ShadowProbePlugin {
    fn build(&self, app: &mut App) {
        let findings = SlotFindings::default();
        app.insert_resource(findings.clone()).init_resource::<Probe>().add_systems(Last, probe);
        info!("shadow probe: on (NVF_PLAY_DEBUG_SHADOWS)");
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app.insert_resource(findings).add_systems(Render, check_slots.in_set(RenderSystems::PrepareResourcesFlush).after(RenderSystems::PrepareMeshes));
    }
}

/// Where an entity is: the names of it and its ancestors, its layers, its place.
fn describe(e: Entity, names: &Names) -> String {
    let mut chain = Vec::new();
    let mut at = Some(e);
    let mut steps = 0;
    while let (Some(x), true) = (at, steps < 40) {
        let Ok((name, parent, ..)) = names.get(x) else { break };
        if let Some(n) = name {
            chain.push(n.as_str().to_string());
        }
        at = parent.map(|p| p.parent());
        steps += 1;
    }
    let (layers, place) = names.get(e).map_or((None, None), |(_, _, l, g)| (l.cloned(), g.map(|g| bevy_to_game(g.translation()))));
    format!("{e:?} [{}] layers {:?} at {:?}", chain.join(" < "), layers.map(|l| l.iter().collect::<Vec<_>>()), place.map(|p| p.round()))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn probe(
    time: Res<Time>,
    mut state: ResMut<Probe>,
    findings: Res<SlotFindings>,
    skins: Query<(Entity, &SkinnedMesh, &ViewVisibility, Option<&Aabb>, &GlobalTransform)>,
    globals: Query<&GlobalTransform>,
    names: Names,
    sun: Query<&CascadesVisibleEntities, With<Sun>>,
    camera: Query<(Entity, &GlobalTransform), With<nvf_play::PlayerCamera>>,
) {
    let state = &mut *state;
    state.frame = state.frame.wrapping_add(1);
    state.since_summary += time.delta_secs();
    if state.frame.is_multiple_of(EVERY) {
        let cam = camera.single().ok();
        // Stretched skins.
        for (e, skin, seen, _, _) in &skins {
            if !seen.get() {
                continue;
            }
            let points: Vec<Vec3> = skin.joints.iter().filter_map(|j| globals.get(*j).ok()).map(|g| g.translation()).collect();
            // A joint gone, or still at the world's origin, is drawn as the identity:
            // the vertices bound to it go to the origin (game 0, 0, 0).
            let missing = skin.joints.len() - points.len();
            let at_origin = points.iter().filter(|p| p.length() < 1.0).count();
            if missing + at_origin > 0 {
                state.lost_joints += 1;
                if state.reported.insert(e) {
                    warn!("shadow probe: skin of {} has {missing} joints gone and {at_origin} at the world's origin (of {})", describe(e, &names), skin.joints.len());
                }
            }
            let Some(first) = points.first().copied() else { continue };
            let spread = points.iter().map(|p| p.distance(first)).fold(0.0, f32::max);
            let player = *state.player_skin.entry(e).or_insert_with(|| under_player_body(e, &names));
            if player {
                state.player_spread = state.player_spread.max(spread);
            }
            if spread > SPREAD {
                state.stretched += 1;
                if state.reported.insert(e) {
                    warn!("shadow probe: skin of {} has joints {spread:.0} units apart ({} joints)", describe(e, &names), points.len());
                }
            }
        }
        // Big skinned casters in the sun's near cascades of the main camera.
        if let (Ok(cascades), Some((cam_e, cam_at))) = (sun.single(), cam) {
            if let Some(list) = cascades.entities.get(&cam_e) {
                for (ci, c) in list.iter().take(2).enumerate() {
                    for &e in &c.entities {
                        let Ok((_, _, _, Some(aabb), g)) = skins.get(e) else { continue };
                        let size = (g.affine().matrix3 * Vec3::from(aabb.half_extents) * 2.0).abs().max_element();
                        if size > SPREAD {
                            state.big += 1;
                            if state.reported.insert(e) {
                                let d = g.transform_point(Vec3::from(aabb.center)).distance(cam_at.translation());
                                warn!("shadow probe: cascade {ci} casts from skinned {} with bounds {size:.0} wide, centre {d:.0} from the camera", describe(e, &names));
                            }
                        }
                    }
                }
            }
        }
    }
    // Skin slots, from the render world.
    let (taken, moved): (Vec<Finding>, Vec<Finding>) = {
        let Ok(mut f) = findings.0.lock() else { return };
        state.checked_slots = state.checked_slots.max(f.checked);
        (std::mem::take(&mut f.slots), std::mem::take(&mut f.moved))
    };
    for (e, had, now) in moved {
        state.moved += 1;
        if state.reported.insert(e) {
            let skinned = skins.get(e).is_ok();
            warn!("shadow probe: {} (skinned {skinned}) is drawn from vertex {had}, its mesh is now at vertex {now}", describe(e, &names));
        }
    }
    for (e, had, now) in taken {
        state.slots += 1;
        if state.reported.insert(e) {
            warn!("shadow probe: {} is drawn with skin slot {had}, its skin has slot {now} (u32::MAX: none)", describe(e, &names));
        }
    }
    if state.since_summary >= 5.0 {
        state.since_summary = 0.0;
        info!(
            "shadow probe: {} skins; stretched {} big near casters {} wrong skin slots {} (of {} skinned instances checked); drawn from a mesh's old place {}; joints gone or at the origin {}; player body joints within {:.0}",
            skins.iter().count(),
            state.stretched,
            state.big,
            state.slots,
            state.checked_slots,
            state.moved,
            state.lost_joints,
            state.player_spread
        );
        state.player_spread = 0.0;
    }
}

/// Render world: compare each skinned instance's skin slot on the GPU with its skin's.
fn check_slots(
    mut frame: Local<u32>,
    instances: Res<RenderMeshInstances>,
    buffers: Option<Res<BatchedInstanceBuffers<MeshUniform, MeshInputUniform>>>,
    skins: Res<SkinUniforms>,
    allocator: Res<bevy::render::mesh::allocator::MeshAllocator>,
    findings: Res<SlotFindings>,
) {
    *frame = frame.wrapping_add(1);
    if !frame.is_multiple_of(EVERY) {
        return;
    }
    let (RenderMeshInstances::GpuBuilding(gpu), Some(buffers)) = (&*instances, buffers) else { return };
    let mut checked = 0;
    let mut wrong = Vec::new();
    let mut stale = Vec::new();
    for (main, inst) in gpu.iter() {
        let Some(input) = buffers.current_input_buffer.get(inst.gpu_specific.current_uniform_index()) else { continue };
        // Where the mesh's vertices and indices are now, against where the instance
        // draws them from.
        let id = inst.shared.mesh_asset_id();
        if let Some(v) = allocator.mesh_vertex_slice(&id) {
            let i = allocator.mesh_index_slice(&id);
            let moved = input.first_vertex_index != v.range.start
                || i.as_ref().is_some_and(|i| input.first_index_index != i.range.start || input.index_count != i.range.end - i.range.start);
            if moved {
                stale.push((MainEntity::id(main), input.first_vertex_index, v.range.start));
            }
        }
        let want = skins.skin_index(*main).unwrap_or(u32::MAX);
        if input.current_skin_index == u32::MAX && want == u32::MAX {
            continue;
        }
        checked += 1;
        if input.current_skin_index != want {
            wrong.push((MainEntity::id(main), input.current_skin_index, want));
        }
    }
    if let Ok(mut f) = findings.0.lock() {
        f.checked = checked;
        f.slots.extend(wrong);
        f.moved.extend(stale);
    }
}
