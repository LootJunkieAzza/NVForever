//! JIP's physics functions on the engine's side (`fnv_game::nvse_jip13`): each frame the
//! references the scripts asked about are told as the physics has them (their bodies,
//! contacts, whether the player could grab them, an actor's ground), and what the
//! scripts asked is done (velocities, layers, properties, collision on and off, the
//! gravity, shoves along the ground).

use crate::combat::Knockback;
use crate::phys::PhysState;
use crate::script::ScriptState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_game::nvse_jip13::{PhysBody, PhysRequest, PhysView};
use fnv_phys::script::BodyProperty;
use fnv_phys::world::{Filter, Owner};
use fnv_sim::FormId;

/// How long a shove pushes, seconds: SetVel's (`crate::console::SET_VEL_SECS`)
/// [hypothesis: a push without a fall slows as a SetVel push does].
const SHOVE_SECS: f32 = crate::console::SET_VEL_SECS;

/// The most a grabbed body may weigh, kilograms: `fGrabMaxWeightWalking`'s default
/// (`crate::grab`) [hypothesis: IsGrabbable asks of the walking limit].
const GRAB_MAX_MASS: f32 = 100.0;

/// How far below an actor's feet its ground is looked for, game units.
const GROUND_REACH: f32 = 24.0;

pub struct JipPhysicsPlugin;

impl Plugin for JipPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply.after(PlaySet::Scripts).before(PlaySet::Ui));
    }
}

fn apply(script: Option<ResMut<ScriptState>>, phys: Option<ResMut<PhysState>>, mut knocks: MessageWriter<Knockback>, mut gravity: Local<Option<[f32; 3]>>) {
    let (Some(mut script), Some(mut phys)) = (script, phys) else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let g = &mut rt.game;
    let p = &mut *phys;
    for req in g.take_phys_requests() {
        match req {
            PhysRequest::Velocity { r, body, linvel, angvel } => {
                p.world.set_script_velocity(r.0, body, linvel, angvel);
            }
            PhysRequest::Property { r, body, property, value } => {
                if let Some(prop) = BodyProperty::from_index(property) {
                    p.world.set_script_property(r.0, body, prop, value);
                }
            }
            PhysRequest::Layer { r, body, layer } => {
                p.world.set_script_layer(r.0, body, layer);
            }
            PhysRequest::Collision { r, on } => {
                let n = p.world.set_script_collision(r.0, on);
                info!("jip physics: {r} collision {} ({n} colliders)", if on { "on" } else { "off" });
            }
            PhysRequest::Gravity(v) => {
                let own = *gravity.get_or_insert_with(|| p.world.script_gravity());
                let to = v.unwrap_or(own);
                p.world.set_script_gravity(to);
                info!("jip physics: the moving bodies' gravity {to:?}");
            }
            PhysRequest::Shove { r, dir, speed } => {
                if speed != 0.0 {
                    let s = speed.signum();
                    knocks.write(Knockback { form_id: r.0, dir: [dir[0] * s, dir[1] * s], speed: speed.abs(), secs: SHOVE_SECS, attacker: 0, blow: 0, power: false });
                }
            }
        }
    }
    let watched: Vec<FormId> = g.phys_watched().collect();
    if watched.is_empty() {
        return;
    }
    p.world.update_queries();
    for r in watched {
        let view = view_of(p, r);
        g.set_phys_view(r, view);
    }
}

/// A reference's physics as the scripts see it; `None` when it has none.
fn view_of(p: &mut PhysState, r: FormId) -> Option<PhysView> {
    let ch = if r == FormId::PLAYER_REF { p.player.as_ref() } else { p.npcs.get(&r.0).map(|n| &n.ch) };
    let ground = ch.and_then(|c| {
        if !c.grounded {
            return None;
        }
        let from = [c.feet[0], c.feet[1], c.feet[2] + 2.0];
        let h = p.world.queries().ray(from, [0.0, 0.0, -1.0], GROUND_REACH, &Filter::excluding(c.key()));
        Some(match h {
            Some(h) => {
                let on = match h.owner {
                    Owner::Ref { form_id, .. } | Owner::Actor { form_id, .. } => Some(FormId(form_id)),
                    _ => None,
                };
                (on, h.material)
            }
            None => (None, None),
        })
    });
    let bodies: Vec<PhysBody> = p
        .world
        .script_bodies(r.0)
        .into_iter()
        .map(|b| PhysBody {
            node: b.node,
            layer: b.layer,
            dynamic: b.dynamic,
            linvel: b.linvel,
            angvel: b.angvel,
            rotation: b.rotation,
            mass: b.mass,
            friction: b.friction,
            restitution: b.restitution,
            linear_damping: b.linear_damping,
            angular_damping: b.angular_damping,
            inertia: b.inertia,
        })
        .collect();
    if bodies.is_empty() && ch.is_none() {
        return None;
    }
    let mut contacts: Vec<FormId> = p
        .world
        .ref_contacts(r.0)
        .into_iter()
        .filter(|(_, _, d)| *d <= 0.5)
        .filter_map(|(_, o, _)| match o? {
            Owner::Ref { form_id, .. } | Owner::Actor { form_id, .. } if form_id != r.0 => Some(FormId(form_id)),
            _ => None,
        })
        .collect();
    contacts.sort();
    contacts.dedup();
    let grabbable = ch.is_none() && bodies.iter().any(|b| b.dynamic) && p.world.ref_mass(r.0).is_some_and(|m| m <= GRAB_MAX_MASS);
    Some(PhysView { bodies, contacts, grabbable, ground, phantom: false })
}
