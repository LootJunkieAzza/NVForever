//! Opt-in raw physical-field observation for the authored AttachModel smoke.
//!
//! This child of `nif_mirror` uses the baseline recipient lookup, never attachment
//! producer state or script/getter acknowledgments. It does not decide whether
//! geometry/material values match a fixture: the scenario judges the raw rows.
use super::{root_of, Rigs, Visuals};
use bevy::{asset::AssetId, ecs::system::SystemState, mesh::VertexAttributeValues, prelude::*};
use fnv_game::nif_blocks::ModelView;
use fnv_sim::FormId;
use nvf_assets::{model::ModelCtx, NifMaterial};
use nvf_play::pause::AidTime;

const OPT_IN: &str = "NVF_PLAY_TEST_ATTACH_MODEL_PROBE";
const FIXTURE_PATH: &str = "NVF_PLAY_TEST_ATTACH_MODEL_PROBE_PATH";
const LABEL: &str = "OwnedAttachShape_nvfa";
// Observation bounds only; they do not alter attachment/scene behavior.
const ANCESTOR_LIMIT: usize = 256;
const DEADLINE: f32 = 18.;

/// Absent opt-in installs no resource or per-frame system.
pub(super) fn install(app: &mut App) {
    if !std::env::var(OPT_IN).is_ok_and(|v| v == "1") {
        return;
    }
    app.insert_resource(Probe {
        preflight_path: std::env::var(FIXTURE_PATH).ok(),
        ..default()
    })
    .add_systems(
        PostUpdate,
        (preflight, observe).chain().after(TransformSystems::Propagate),
    );
}

#[derive(Resource, Default)]
struct Probe {
    preflight_path: Option<String>,
    preflight_done: bool,
    preflight_waiting: bool,
    finished: bool,
    root: Option<Entity>,
    captured: Vec<Capture>,
    retained: bool,
}


/// Real baseline ModelCtx admission of the single caller-owned fixture, before
/// extraction can remove CPU vertices. This deliberately uses the ordinary
/// model/cache variant, never the attachment producer or its private state.
/// Exclusive access avoids aliasing ModelCtx's mutable Assets with observe's
/// read-only Assets parameters; the two systems are explicitly chained.
fn preflight(world: &mut World) {
    let path = {
        let state = world.resource::<Probe>();
        if state.preflight_done {
            return;
        }
        state.preflight_path.clone()
    };
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        warn!("nif attach probe: preflight unavailable reason=missing_fixture_path");
        world.resource_mut::<Probe>().preflight_done = true;
        return;
    };
    // Do not invent startup resources or panic while the normal app is loading.
    // CompressedImageFormatSupport is optional in the existing ModelCtx itself.
    let ready = world.contains_resource::<nvf_assets::GameData>()
        && world.contains_resource::<nvf_assets::ModelCache>()
        && world.contains_resource::<Assets<Mesh>>()
        && world.contains_resource::<Assets<NifMaterial>>()
        && world.contains_resource::<Assets<Image>>()
        && world.contains_resource::<nvf_assets::TextureCache>()
        && world.contains_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>();
    if !ready {
        let mut state = world.resource_mut::<Probe>();
        if !state.preflight_waiting {
            info!("nif attach probe: preflight waiting reason=model_resources_not_available");
            state.preflight_waiting = true;
        }
        return;
    }
    world.resource_mut::<Probe>().preflight_done = true;
    let mut params = SystemState::<ModelCtx>::new(world);
    {
        let Ok(mut models) = params.get_mut(world) else {
            warn!("nif attach probe: preflight unavailable reason=model_params_not_available");
            return;
        };
        let Some(model) = models.model(&path) else {
            warn!("nif attach probe: preflight unavailable reason=model_not_prepared");
            return;
        };
        info!(
            "nif attach probe: preflight prepared shapes={} triangles={} rig={} bounds={:?}",
            model.shapes.len(), model.triangles, u8::from(model.rig.is_some()), model.bounds,
        );
        for (index, shape) in model.shapes.iter().enumerate() {
            let mesh_id = shape.mesh.id();
            let material_id = shape.material.id();
            let Some(mesh) = models.meshes.get(mesh_id) else {
                info!("nif attach probe: preflight unavailable index={index} reason=missing_mesh_asset");
                continue;
            };
            let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                info!("nif attach probe: preflight unavailable index={index} reason=missing_cpu_positions");
                continue;
            };
            let Some(material) = models.materials.get(material_id) else {
                info!("nif attach probe: preflight unavailable index={index} reason=missing_material_asset");
                continue;
            };
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            let mut finite = true;
            for &position in positions {
                let p = Vec3::from_array(position);
                finite &= p.is_finite();
                low = low.min(p);
                high = high.max(p);
            }
            let roughness = material.base.perceptual_roughness;
            info!(
                "nif attach probe: preflight shape index={index} mesh={mesh_id:?} material={material_id:?} vertices={} local_min={:?} local_max={:?} positions_finite={} roughness={:.9} roughness_bits={} roughness_finite={}",
                positions.len(), low.to_array(), high.to_array(), u8::from(finite),
                roughness, roughness.to_bits(), u8::from(roughness.is_finite()),
            );
        }
        // model and its owning handles die here; no extra strong handle, CPU copy,
        // source bytes or prepared model is stored in the diagnostic state.
    }
    params.apply(world);
}

// AssetId is deliberately non-owning: this observer must not extend asset life.
struct Capture {
    entity: Entity,
    mesh: AssetId<Mesh>,
    material: AssetId<NifMaterial>,
    vertices: usize,
    low: [f32; 3],
    high: [f32; 3],
    roughness: f32,
    local_translation: [f32; 3],
    local_rotation: [f32; 4],
    local_scale: [f32; 3],
    world_translation: [f32; 3],
    ancestors: Vec<Entity>,
}

type NamedData = (
    Entity,
    &'static Name,
    Option<&'static Mesh3d>,
    Option<&'static MeshMaterial3d<NifMaterial>>,
    Option<&'static Transform>,
    Option<&'static GlobalTransform>,
);

/// Includes both the child and root; a cycle or unrelated tree is not ancestry.
fn ancestry<T: Copy + Eq>(
    start: T,
    root: T,
    mut parent: impl FnMut(T) -> Option<T>,
) -> Result<Vec<T>, &'static str> {
    let mut path = Vec::new();
    let mut at = start;
    loop {
        if path.len() == ANCESTOR_LIMIT {
            return Err("ancestor_limit");
        }
        if path.contains(&at) {
            return Err("ancestor_cycle");
        }
        path.push(at);
        if at == root {
            return Ok(path);
        }
        at = parent(at).ok_or("outside_recipient")?;
    }
}

fn sample(
    entity: Entity,
    root: Entity,
    named: &Query<NamedData>,
    parents: &Query<&ChildOf>,
    meshes: &Assets<Mesh>,
    materials: &Assets<NifMaterial>,
) -> Result<Capture, &'static str> {
    let (_, _, mesh, material, local, global) = named.get(entity).map_err(|_| "entity_gone")?;
    let mesh = mesh.ok_or("missing_mesh_component")?;
    let material = material.ok_or("missing_material_component")?;
    let local = local.ok_or("missing_local_transform")?;
    let global = global.ok_or("missing_global_transform")?;
    let ancestors = ancestry(entity, root, |e| parents.get(e).ok().map(|p| p.parent()))?;
    let asset = meshes.get(mesh.0.id()).ok_or("missing_mesh_asset")?;
    // RENDER_WORLD meshes lose CPU GPU-data at extraction. This system runs in
    // the first post-propagation main-world phase, not a later timer callback.
    let Some(VertexAttributeValues::Float32x3(positions)) = asset.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return Err("missing_cpu_positions");
    };
    if positions.is_empty() {
        return Err("empty_positions");
    }
    let (mut low, mut high) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &position in positions {
        let p = Vec3::from_array(position);
        if !p.is_finite() {
            return Err("nonfinite_positions");
        }
        low = low.min(p);
        high = high.max(p);
    }
    let roughness = materials.get(material.0.id()).ok_or("missing_material_asset")?.base.perceptual_roughness;
    if !roughness.is_finite() {
        return Err("nonfinite_material");
    }
    Ok(Capture {
        entity,
        mesh: mesh.0.id(),
        material: material.0.id(),
        vertices: positions.len(),
        low: low.to_array(),
        high: high.to_array(),
        roughness,
        local_translation: local.translation.to_array(),
        local_rotation: local.rotation.to_array(),
        local_scale: local.scale.to_array(),
        world_translation: global.translation().to_array(),
        ancestors,
    })
}

fn terminal(state: &mut Probe, now: f32, reason: &str) {
    warn!("nif attach probe: terminal time={now:.6} reason={reason}");
    state.finished = true;
    state.captured.clear();
}

// All Transform/parent/mesh queries are read-only. In particular this does not
// borrow the mirror's mutable NodeQuery (whose baseline tuple is different).
fn observe(
    mut state: ResMut<Probe>,
    (rigs, visuals): (Rigs, Visuals),
    named: Query<NamedData>,
    (parents, globals, entities): (Query<&ChildOf>, Query<&GlobalTransform>, Query<Entity>),
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<NifMaterial>>,
    time: AidTime,
) {
    if state.finished {
        return;
    }
    let Some(now) = time.world_secs() else { return };
    let selected = root_of(FormId::PLAYER_REF.0, ModelView::ThirdPerson, &rigs, &visuals);
    if let Some(root) = state.root {
        let host_live = entities.get(root).is_ok() && globals.get(root).is_ok();
        let same_root = selected == Some(root);
        let gone = state.captured.iter().filter(|c| entities.get(c.entity).is_err()).count();
        let named_count = named.iter().filter(|(_, n, ..)| n.as_str() == LABEL).count();
        if !host_live || !same_root {
            info!(
                "nif attach probe: recipient time={now:.6} captured_root={} selected_root={:?} host_live={} selected_same={} captured_gone={gone}",
                root.to_bits(), selected.map(Entity::to_bits), u8::from(host_live), u8::from(same_root),
            );
            terminal(&mut state, now, "recipient_changed");
            return;
        }
        if gone != 0 {
            let ids: Vec<_> = state.captured.iter().map(|c| c.entity.to_bits()).collect();
            info!(
                "nif attach probe: removed time={now:.6} root={} captured_entities={ids:?} captured_count={} gone={gone} matching_named={named_count} host_live={} selected_same={}",
                root.to_bits(), state.captured.len(), u8::from(host_live), u8::from(same_root),
            );
            // This row reports facts, not mode2 causality. The scenario must put
            // its removal-submitted console marker before it and judge counts.
            state.finished = true;
            state.captured.clear();
            return;
        }
        if !state.retained {
            let same_handles = state.captured.iter().all(|c| {
                named.get(c.entity).is_ok_and(|(_, _, mesh, material, _, _)| {
                    mesh.is_some_and(|m| m.0.id() == c.mesh)
                        && material.is_some_and(|m| m.0.id() == c.material)
                })
            });
            let same_ancestry = state.captured.iter().all(|c| {
                ancestry(c.entity, root, |e| parents.get(e).ok().map(|p| p.parent()))
                    .is_ok_and(|path| path == c.ancestors)
            });
            info!(
                "nif attach probe: retained time={now:.6} root={} captured_count={} matching_named={named_count} same_handles={} same_ancestry={}",
                root.to_bits(), state.captured.len(), u8::from(same_handles), u8::from(same_ancestry),
            );
            state.retained = true;
        }
        if now > DEADLINE {
            terminal(&mut state, now, "retirement_not_observed");
        }
        return;
    }
    if now > DEADLINE {
        terminal(&mut state, now, "physical_fields_not_observed");
        return;
    }
    let Some(root) = selected.filter(|r| globals.get(*r).is_ok()) else { return };
    let mut candidates = Vec::with_capacity(2);
    let mut total = 0;
    for (entity, name, ..) in &named {
        if name.as_str() == LABEL {
            total += 1;
            if candidates.len() < 2 {
                candidates.push(entity);
            }
        }
    }
    if total == 0 {
        return;
    }
    info!("nif attach probe: candidates time={now:.6} root={} matching_named={total}", root.to_bits());
    if total != 2 {
        terminal(&mut state, now, "authored_field_count");
        return;
    }
    candidates.sort_by_key(|e| e.to_bits());
    let mut captured = Vec::with_capacity(2);
    for entity in candidates {
        let field = match sample(entity, root, &named, &parents, &meshes, &materials) {
            Ok(field) => field,
            Err(reason) => {
                info!("nif attach probe: unavailable time={now:.6} root={} entity={} reason={reason}", root.to_bits(), entity.to_bits());
                terminal(&mut state, now, reason);
                return;
            }
        };
        captured.push(field);
    }
    for c in &captured {
        let ancestors: Vec<_> = c.ancestors.iter().map(|e| e.to_bits()).collect();
        info!(
            "nif attach probe: present time={now:.6} root={} entity={} mesh={:?} material={:?} vertices={} local_min={:?} local_max={:?} roughness={:.9} roughness_bits={} local_translation={:?} local_rotation={:?} local_scale={:?} world_translation={:?} ancestors={ancestors:?}",
            root.to_bits(), c.entity.to_bits(), c.mesh, c.material, c.vertices,
            c.low, c.high, c.roughness, c.roughness.to_bits(), c.local_translation,
            c.local_rotation, c.local_scale, c.world_translation,
        );
    }
    let distinct_meshes = captured[0].mesh != captured[1].mesh;
    let distinct_entities = captured[0].entity != captured[1].entity;
    info!(
        "nif attach probe: captured time={now:.6} root={} count={} distinct_entities={} distinct_meshes={}",
        root.to_bits(), captured.len(), u8::from(distinct_entities), u8::from(distinct_meshes),
    );
    state.root = Some(root);
    state.captured = captured;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestry_accepts_exact_limit_but_refuses_one_beyond() {
        let previous = |n: usize| n.checked_sub(1);
        assert_eq!(ancestry(ANCESTOR_LIMIT - 1, 0, previous).unwrap().len(), ANCESTOR_LIMIT);
        assert_eq!(ancestry(ANCESTOR_LIMIT, 0, previous), Err("ancestor_limit"));
    }

    #[test]
    fn a_cycle_and_an_unrelated_tree_do_not_become_recipient_ancestry() {
        assert_eq!(ancestry(1, 0, |n| Some(if n == 1 { 2 } else { 1 })), Err("ancestor_cycle"));
        assert_eq!(ancestry(2_usize, 0, |n| (n == 2).then_some(1)), Err("outside_recipient"));
    }
}
