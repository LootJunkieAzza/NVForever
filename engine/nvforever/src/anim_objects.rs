//! Animated objects (docs/spec-notes/idles.md §10): the props actors hold while an idle
//! plays, such as Chet's broom while he sweeps (`LooseSweeping`, `aoSweepingBroom`) or a
//! cigarette while smoking. `fnv_actor::idle::anim_objects` reads the ANIO records; each
//! names the IDLE it goes with and a model whose `Prn` extra data names the bone it hangs
//! from. While an actor's idle slot plays an IDLE that has animated objects (`idles.rs`,
//! [`ActorIdle`]), their models are drawn under those bones of its skeleton, rigid, as
//! a rigid body part is (`nvf_assets::actor::rigid_local`); when the idle ends or another
//! starts they are taken away [hypothesis for the game's rule, idles.md §10].
//!
//! Log: `anim object: <actor> holds <model> on <bone> (<idle>)` per object drawn, and at
//! the end `anim objects summary: …`.
//!
//! Not done: the player's idles (none of the player's known idles has an object), an
//! object's own animation (`aobroom.nif` carries a transform controller on its root,
//! not played), and objects whose model has no `Prn` (drawn at `Bip01 Head`, the rigid
//! parts' default).

use crate::idles::ActorIdle;
use crate::limbs::ActorRig;
use crate::world::WorldData;
use bevy::prelude::*;
use nvf_assets::actor::{rigid_local, ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use std::collections::HashMap;

/// The animated objects by the IDLE they go with: (editor ID, model) of each.
#[derive(Resource, Default)]
pub struct AnimObjects {
    by_idle: HashMap<u32, Vec<(String, String)>>,
    stats: Stats,
    startup: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    /// Objects drawn on actors, idles that had objects, objects whose model did not load
    /// or whose bone the skeleton lacks.
    drawn: u32,
    idles: u32,
    failed: u32,
}

impl AnimObjects {
    pub fn build(world: &WorldData) -> AnimObjects {
        let mut issues = fnv_sim::extract::Issues::default();
        let all = fnv_actor::idle::anim_objects(world.lo, world.schema, &mut issues);
        let mut by_idle: HashMap<u32, Vec<(String, String)>> = HashMap::new();
        let mut usable = 0;
        for a in &all {
            if let (Some(idle), Some(model)) = (a.idle, a.model.clone()) {
                by_idle.entry(idle.0).or_default().push((a.edid.clone().unwrap_or_else(|| a.id.to_string()), model));
                usable += 1;
            }
        }
        let startup = format!("anim objects: {} records, {usable} with an idle and a model, for {} idles", all.len(), by_idle.len());
        AnimObjects { by_idle, stats: Stats::default(), startup: Some(startup) }
    }
}

/// The animated objects drawn on an actor now (on its root): for which idle (its slot
/// serial), and their entities.
#[derive(Component)]
pub struct HeldObjects {
    serial: u32,
    entities: Vec<Entity>,
}

/// Read the records once the world data is there.
pub fn read(mut a: ResMut<AnimObjects>, world: Res<WorldData>) {
    *a = AnimObjects::build(&world);
    if let Some(line) = a.startup.take() {
        info!("{line}");
    }
}

/// Draw the objects of each actor's idle while it plays; take them away after.
pub fn hold(
    mut commands: Commands,
    mut objects: ResMut<AnimObjects>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    actors: Query<(Entity, &ActorRig, Option<&ActorIdle>, Option<&HeldObjects>)>,
) {
    let objects = &mut *objects;
    let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
    for (e, rig, idle, held) in &actors {
        // The idle playing (also while it fades out at its end), not a script's group.
        let now = idle.and_then(|i| i.0.cur.as_ref()).filter(|p| p.group.is_none()).map(|p| (p.idle, p.serial));
        let want = now.filter(|(idle, _)| objects.by_idle.contains_key(idle));
        if held.map(|h| h.serial) == want.map(|w| w.1) {
            continue;
        }
        if let Some(h) = held {
            for &x in &h.entities {
                commands.entity(x).despawn();
            }
            commands.entity(e).remove::<HeldObjects>();
        }
        let Some((idle, serial)) = want else { continue };
        objects.stats.idles += 1;
        let mut entities = Vec::new();
        for (edid, model) in objects.by_idle[&idle].clone() {
            let Some(part) = ctx.part(&model) else {
                objects.stats.failed += 1;
                warn!("anim object: {edid} ({model}) does not load");
                continue;
            };
            let bone_name = part.attach_bone().to_string();
            let Some(&b) = rig.skel.by_name.get(&bone_name) else {
                objects.stats.failed += 1;
                warn!("anim object: {:08X} has no {bone_name} for {edid}", rig.reference);
                continue;
            };
            for s in part.shapes.iter().filter(|s| !s.cap) {
                let local = rigid_local(&rig.skel, b, &part.root, &s.transform);
                let x = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(local)))).id();
                commands.entity(rig.nodes[b]).add_child(x);
                entities.push(x);
            }
            objects.stats.drawn += 1;
            info!("anim object: {:08X} holds {model} on {bone_name} ({edid}, idle {idle:08X})", rig.reference);
        }
        commands.entity(e).insert(HeldObjects { serial, entities });
    }
}

pub fn report(mut exit: MessageReader<AppExit>, objects: Res<AnimObjects>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = objects.stats;
    info!("anim objects summary: {} objects drawn for {} idles played, {} could not be drawn", s.drawn, s.idles, s.failed);
}
