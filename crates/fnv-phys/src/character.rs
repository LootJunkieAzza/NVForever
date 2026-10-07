//! Kinematic capsule character controller (player and NPC movement), over rapier's
//! `KinematicCharacterController` with Z up.
//!
//! The character is a vertical capsule standing on its feet point. Each fixed step the
//! caller gives a horizontal velocity and whether jump is pressed; the controller
//! applies gravity to its own vertical speed, sweeps the capsule through the world
//! (sliding along walls, climbing steps up to `step_height`, refusing slopes steeper than
//! `max_slope_deg`, snapping down to the ground on descents) and moves its kinematic body
//! so dynamic bodies collide with it. Every value of [`CharacterParams`] is an unknown of
//! the original game [hypothesis]; physics.md §6 lists what to capture.

use crate::convert::{arr3, vec3};
use crate::world::{Key, Owner, PhysWorld};
use rapier3d::control::{CharacterAutostep, CharacterCollision, CharacterLength, KinematicCharacterController};
use rapier3d::math::{Pose, Vector};
use rapier3d::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CharacterParams {
    /// Capsule radius, game units [hypothesis].
    pub radius: f32,
    /// Total capsule height, feet to top of head, game units [hypothesis].
    pub height: f32,
    /// Highest ledge climbed without jumping [hypothesis].
    pub step_height: f32,
    /// Steepest walkable slope, degrees from horizontal; steeper slopes are slid down
    /// [hypothesis].
    pub max_slope_deg: f32,
    /// Apex height of a standing jump above the take-off point [hypothesis].
    pub jump_height: f32,
    /// Gap kept between the capsule and the world (numerical skin; not a game value).
    pub skin: f32,
    /// Fastest fall speed, units/s; `None` for no limit [hypothesis].
    pub max_fall_speed: Option<f32>,
    /// Mass used to push dynamic bodies; 0 disables pushing [hypothesis].
    pub push_mass: f32,
}

impl Default for CharacterParams {
    /// Placeholders sized for a human at 70 units per metre (physics.md §6); none is a
    /// measured value.
    fn default() -> Self {
        CharacterParams {
            radius: 18.0,
            height: 128.0,
            step_height: 24.0,
            max_slope_deg: 45.0,
            jump_height: 64.0,
            skin: 0.5,
            max_fall_speed: None,
            push_mass: 80.0,
        }
    }
}

/// Per-step input.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CharacterInput {
    /// Desired horizontal velocity (x, y), units/s.
    pub velocity: [f32; 2],
    pub jump: bool,
}

/// What one step did.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CharacterStep {
    /// Feet position after the step.
    pub feet: [f32; 3],
    pub grounded: bool,
    pub sliding: bool,
    /// Downward speed at touchdown, when this step landed (fall damage input).
    pub landed_speed: Option<f32>,
    /// Entities touched during the step, sorted.
    pub touched: Vec<Owner>,
}

#[derive(Debug, Clone)]
pub struct Character {
    pub id: u32,
    pub params: CharacterParams,
    body: RigidBodyHandle,
    shape: SharedShape,
    /// Feet position (bottom of the capsule).
    pub feet: [f32; 3],
    /// Vertical speed, units/s (positive up).
    pub vertical_speed: f32,
    pub grounded: bool,
    /// Reference colliders on these layers do not block the character; default
    /// [`crate::layers::character_ignores`] [hypothesis].
    pub ignore_layers: crate::layers::LayerSet,
}

impl Character {
    /// This character's key, for query filters (e.g. to keep its own activation ray from
    /// hitting its capsule).
    pub fn key(&self) -> Key {
        Key::Character(self.id)
    }

    /// Capsule centre in rapier's units (`k` per game unit).
    fn center(&self, k: f32) -> Vector {
        (vec3(self.feet) + Vector::Z * (self.params.height / 2.0)) * k
    }

    fn controller(&self, k: f32) -> KinematicCharacterController {
        let p = &self.params;
        KinematicCharacterController {
            up: Vector::Z,
            offset: CharacterLength::Absolute(p.skin * k),
            slide: true,
            autostep: (p.step_height > 0.0).then_some(CharacterAutostep {
                max_height: CharacterLength::Absolute(p.step_height * k),
                min_width: CharacterLength::Absolute(p.radius * k),
                include_dynamic_bodies: false,
            }),
            max_slope_climb_angle: p.max_slope_deg.to_radians(),
            min_slope_slide_angle: p.max_slope_deg.to_radians(),
            snap_to_ground: (p.step_height > 0.0).then_some(CharacterLength::Absolute(p.step_height * k)),
            ..Default::default()
        }
    }
}

fn capsule(p: &CharacterParams, k: f32) -> SharedShape {
    let half = (p.height / 2.0 - p.radius).max(0.0) * k;
    SharedShape::capsule(Vector::Z * -half, Vector::Z * half, p.radius * k)
}

impl PhysWorld {
    /// Add a character standing with its feet at `feet`.
    pub fn add_character(&mut self, id: u32, params: CharacterParams, feet: [f32; 3]) -> Character {
        let shape = capsule(&params, self.k);
        let owner = Owner::Character { id };
        let mut c = Character {
            id,
            params,
            body: RigidBodyHandle::invalid(),
            shape,
            feet,
            vertical_speed: 0.0,
            grounded: false,
            ignore_layers: crate::layers::character_ignores(),
        };
        let body =
            self.insert_body(RigidBodyBuilder::kinematic_position_based().translation(c.center(self.k)).user_data(owner.encode()));
        self.w.insert_collider(ColliderBuilder::new(c.shape.clone()).user_data(owner.encode()), Some(body));
        c.body = body;
        self.mark_dirty();
        c
    }

    pub fn remove_character(&mut self, c: Character) {
        self.remove_body(c.body);
    }

    /// Move a character teleport-style (no sweep), e.g. on a door transition.
    pub fn place_character(&mut self, c: &mut Character, feet: [f32; 3]) {
        c.feet = feet;
        c.vertical_speed = 0.0;
        c.grounded = false;
        let center = c.center(self.k);
        self.w.bodies[c.body].set_translation(center, true);
        self.mark_dirty();
    }

    /// One fixed step of character movement (call once per [`PhysWorld::step`], before
    /// it, with the same `dt`).
    pub fn move_character(&mut self, c: &mut Character, input: CharacterInput) -> CharacterStep {
        self.update_queries();
        let dt = self.params.dt;
        let g = self.params.gravity;
        if input.jump && c.grounded {
            c.vertical_speed = (2.0 * g * c.params.jump_height.max(0.0)).sqrt();
        }
        c.vertical_speed -= g * dt;
        if let Some(max) = c.params.max_fall_speed {
            c.vertical_speed = c.vertical_speed.max(-max);
        }
        let k = self.k;
        let desired = Vector::new(input.velocity[0] * dt, input.velocity[1] * dt, c.vertical_speed * dt);
        let kcc = c.controller(k);
        let pos = Pose::from_translation(c.center(k));
        let mut collisions: Vec<CharacterCollision> = Vec::new();
        let ignore = c.ignore_layers;
        let solid = move |_: ColliderHandle, co: &Collider| match Owner::decode(co.user_data) {
            Some(Owner::Ref { layer, .. }) => !ignore.contains(layer),
            _ => true,
        };
        let filter = QueryFilter::default().exclude_sensors().exclude_rigid_body(c.body).predicate(&solid);
        let moved = {
            let q = self.w.query_pipeline_with_filter(filter);
            kcc.move_shape(dt, &q, c.shape.as_ref(), &pos, desired * k, |hit| collisions.push(hit))
        };
        if c.params.push_mass > 0.0 && !collisions.is_empty() {
            let w = &mut self.w;
            let mut q = w.broad_phase.as_query_pipeline_mut(
                w.narrow_phase.query_dispatcher(),
                &mut w.bodies,
                &mut w.colliders,
                filter,
            );
            kcc.solve_character_collision_impulses(dt, &mut q, c.shape.as_ref(), c.params.push_mass, &collisions);
        }
        let falling_speed = -c.vertical_speed;
        let was_grounded = c.grounded;
        c.grounded = moved.grounded;
        let mut landed_speed = None;
        if moved.grounded && c.vertical_speed <= 0.0 {
            if !was_grounded {
                landed_speed = Some(falling_speed.max(0.0));
            }
            c.vertical_speed = 0.0;
        } else if c.vertical_speed > 0.0 && moved.translation.z / k < desired.z - 1e-3 {
            // Head hit a ceiling.
            c.vertical_speed = 0.0;
        }
        c.feet = arr3(vec3(c.feet) + moved.translation / k);
        self.w.bodies[c.body].set_next_kinematic_translation(c.center(k));
        let mut touched: Vec<Owner> =
            collisions.iter().filter_map(|h| Owner::decode(self.w.colliders.get(h.handle)?.user_data)).collect();
        touched.sort();
        touched.dedup();
        CharacterStep { feet: c.feet, grounded: c.grounded, sliding: moved.is_sliding_down_slope, landed_speed, touched }
    }

}
