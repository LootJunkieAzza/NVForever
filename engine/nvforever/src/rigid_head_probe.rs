//! [design, #2529] Read-only smoke aid: `NVF_PLAY_TEST_RIGID_HEAD=base_hex|shape_name`.
//! For an identity-root, bone-local rigid head mesh, compare its drawn frame and bounds
//! centre with the full head transform after propagation. No game records are changed.

use bevy::math::Mat3;
use bevy::mesh::{skinning::SkinnedMesh, VertexAttributeValues};
use bevy::prelude::*;
use nvf_assets::actor::{LimbShape, ShapeName, DEFAULT_RIGID_BONE};
use nvf_play::script::ScriptState;

use crate::{limbs::ActorRig, outfits::Dressed};

#[derive(Resource)]
struct Probe {
    base: u32,
    shape: String,
    initial_rotation: Option<Quat>,
    next_report: f64,
    phase: &'static str,
    talked: bool,
    samples: u64,
    max_error: f32,
    max_frame_error: f32,
    max_turn: f32,
}

pub fn install(app: &mut App) {
    let Ok(value) = std::env::var("NVF_PLAY_TEST_RIGID_HEAD") else { return };
    let parsed = value.split_once('|').and_then(|(base, shape)| {
        let base = u32::from_str_radix(base.trim().trim_start_matches("0x"), 16).ok()?;
        (!shape.trim().is_empty()).then(|| (base, shape.trim().to_ascii_lowercase()))
    });
    let Some((base, shape)) = parsed else {
        warn!("rigid head check: expected NVF_PLAY_TEST_RIGID_HEAD=base_hex|shape_name");
        return;
    };
    app.insert_resource(Probe {
        base, shape, initial_rotation: None, next_report: 0.0, phase: "", talked: false,
        samples: 0, max_error: 0.0, max_frame_error: 0.0, max_turn: 0.0,
    }).add_systems(PostUpdate, sample.after(bevy::transform::TransformSystems::Propagate))
        .add_systems(Last, report);
}

fn sample(
    mut probe: ResMut<Probe>,
    time: Res<Time<Real>>,
    script: Option<Res<ScriptState>>,
    actors: Query<(&ActorRig, &Dressed)>,
    nodes: Query<(&Transform, &GlobalTransform)>,
    shapes: Query<(&ShapeName, &LimbShape, &ChildOf, &GlobalTransform), Without<SkinnedMesh>>,
) {
    for (rig, dressed) in &actors {
        if rig.base != probe.base { continue; }
        let Some(&index) = rig.skel.by_name.get(DEFAULT_RIGID_BONE) else { continue };
        let head_entity = rig.nodes[index];
        let Ok((head_local, head)) = nodes.get(head_entity) else { continue };
        for &entity in dressed.parts.iter().flat_map(|(_, entities)| entities) {
            let Ok((name, limb, parent, mesh)) = shapes.get(entity) else { continue };
            if name.0 != probe.shape { continue; }
            let Some(VertexAttributeValues::Float32x3(vertices)) = limb.cpu.attribute(Mesh::ATTRIBUTE_POSITION) else { continue };
            if vertices.is_empty() { continue; }
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for v in vertices {
                min = min.min(Vec3::from_array(*v));
                max = max.max(Vec3::from_array(*v));
            }
            let centre = (min + max) * 0.5;
            let world = mesh.transform_point(centre);
            let relative_frame = head.affine().inverse() * mesh.affine();
            let relative = relative_frame.transform_point3(centre);
            let error = relative.distance(centre);
            let frame_error = (Mat3::from(relative_frame.matrix3) - Mat3::IDENTITY)
                .to_cols_array().into_iter().map(f32::abs).fold(0.0, f32::max);
            let initial = *probe.initial_rotation.get_or_insert(head_local.rotation);
            let turn = initial.angle_between(head_local.rotation).to_degrees();
            probe.max_turn = probe.max_turn.max(turn);
            probe.max_error = probe.max_error.max(error);
            probe.max_frame_error = probe.max_frame_error.max(frame_error);
            probe.samples += 1;
            let talking = script.as_ref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.dialogue.as_ref())
                .is_some_and(|c| c.actor.0 == rig.reference);
            probe.talked |= talking;
            let phase = if talking { "talking" } else if probe.talked { "after" } else { "idle" };
            if time.elapsed_secs_f64() >= probe.next_report || probe.phase != phase {
                let h = head.translation();
                info!("rigid head check: actor={:08X} phase={phase} parent_head={} world=({:.3},{:.3},{:.3}) head=({:.3},{:.3},{:.3}) relative=({:.3},{:.3},{:.3}) error={error:.5} frame_error={frame_error:.5} turn_deg={:.2}",
                    rig.reference, parent.parent() == head_entity, world.x, world.y, world.z,
                    h.x, h.y, h.z, relative.x, relative.y, relative.z, probe.max_turn);
                probe.next_report = time.elapsed_secs_f64() + 1.0;
                probe.phase = phase;
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, probe: Res<Probe>) {
    if exit.read().next().is_some() {
        info!("rigid head check summary: samples={} max_error={:.5} max_frame_error={:.5} turn_deg={:.2}",
            probe.samples, probe.max_error, probe.max_frame_error, probe.max_turn);
    }
}
