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
use crate::world::{Key, Owner, PhysWorld, TERRAIN_SHELL};
use rapier3d::control::{CharacterAutostep, CharacterCollision, CharacterLength, KinematicCharacterController};
use rapier3d::math::{Pose, Vector};
use rapier3d::parry::query::ShapeCastOptions;
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

impl CharacterParams {
    /// Gap the capsule keeps from a support whose contact shell is `shell` (game units):
    /// the shell, never less than the numerical `skin`. The one rule for where a
    /// character rests, used by the walking controller and by the arrival after a
    /// teleport (physics.md §6.2) [observed: rest heights above terrain and statics; that
    /// the excess is a shell is [hypothesis]].
    pub fn rest_gap(&self, shell: f32) -> f32 {
        if shell.is_finite() {
            shell.max(self.skin)
        } else {
            self.skin
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
    /// Gap kept from the support this step ([`CharacterParams::rest_gap`]).
    pub gap: f32,
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
    /// Gap kept from the support (the controller's offset), from the last support found
    /// ([`CharacterParams::rest_gap`]).
    pub gap: f32,
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
            offset: CharacterLength::Absolute(self.gap.max(p.skin) * k),
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
            gap: params.skin,
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
    ///
    /// The capsule keeps the contact shell of what it stands on between itself and the
    /// bare surface ([`CharacterParams::rest_gap`], the rule the arrival uses, physics.md
    /// §6.2): before moving, a short downward sweep finds the support; its gap becomes the
    /// controller's offset for the step, and a capsule closer to a walkable support than
    /// that gap (e.g. having walked from a static floor onto terrain) is lifted to it.
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
        let ignore = c.ignore_layers;
        let solid = move |_: ColliderHandle, co: &Collider| match Owner::decode(co.user_data) {
            Some(Owner::Ref { layer, .. }) => !ignore.contains(layer),
            _ => true,
        };
        let filter = QueryFilter::default().exclude_sensors().exclude_rigid_body(c.body).predicate(&solid);
        let walkable = c.params.max_slope_deg.to_radians().cos();

        // The support and the gap to keep from it.
        let reach = c.params.step_height + 2.0 * TERRAIN_SHELL + (-desired.z).max(0.0);
        let support = {
            let q = self.w.query_pipeline_with_filter(filter);
            let opts = ShapeCastOptions {
                max_time_of_impact: reach * k,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            };
            q.cast_shape(&Pose::from_translation(c.center(k)), -Vector::Z, c.shape.as_ref(), opts)
                .map(|(h, hit)| (h, hit.time_of_impact / k, hit.normal1))
        };
        if let Some((h, toi, n)) = support {
            c.gap = c.params.rest_gap(self.shell_of(h));
            let want = c.gap / n.z.max(walkable);
            if n.z >= walkable && c.vertical_speed <= 0.0 && toi < want - 1e-3 {
                let mut lift = want - toi;
                let q = self.w.query_pipeline_with_filter(filter);
                let opts = ShapeCastOptions {
                    max_time_of_impact: lift * k,
                    target_distance: c.params.skin * k,
                    stop_at_penetration: false,
                    compute_impact_geometry_on_penetration: true,
                };
                if let Some((_, up)) = q.cast_shape(&Pose::from_translation(c.center(k)), Vector::Z, c.shape.as_ref(), opts) {
                    lift = lift.min(up.time_of_impact / k);
                }
                c.feet[2] += lift;
            }
        }

        let kcc = c.controller(k);
        let pos = Pose::from_translation(c.center(k));
        let mut collisions: Vec<CharacterCollision> = Vec::new();
        let mut moved = {
            let q = self.w.query_pipeline_with_filter(filter);
            kcc.move_shape(dt, &q, c.shape.as_ref(), &pos, desired * k, |hit| collisions.push(hit))
        };
        let mut touched_handles: Vec<ColliderHandle> = collisions.iter().map(|h| h.handle).collect();
        let horizontal = [desired.x, desired.y];
        // rapier 0.36 drops the whole remaining horizontal motion when its sweep meets an
        // exactly level surface and the slide's tangent comes out a hair below horizontal
        // (the slope tangent direction is degenerate for a vertical normal): about one step
        // in sixty of a walk across flat ground went nowhere [observed: tests]. Continue
        // such a step with the horizontal motion it lost.
        for _ in 0..2 {
            let want = Vector::new(horizontal[0], horizontal[1], 0.0) * k;
            let missing = want - Vector::new(moved.translation.x, moved.translation.y, 0.0);
            let level_only = !collisions.is_empty() && collisions.iter().all(|h| h.hit.normal1.z > LEVEL);
            if !level_only || want.length() < 1e-9 || missing.dot(want) <= 1e-3 * want.length_squared() {
                break;
            }
            collisions.clear();
            let from = Pose::from_translation(pos.translation + moved.translation);
            let q = self.w.query_pipeline_with_filter(filter);
            let more = kcc.move_shape(dt, &q, c.shape.as_ref(), &from, missing, |hit| collisions.push(hit));
            touched_handles.extend(collisions.iter().map(|h| h.handle));
            moved.translation += more.translation;
            moved.grounded = more.grounded;
            moved.is_sliding_down_slope = more.is_sliding_down_slope;
        }
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
            touched_handles.iter().filter_map(|&h| Owner::decode(self.w.colliders.get(h)?.user_data)).collect();
        touched.sort();
        touched.dedup();
        CharacterStep { feet: c.feet, grounded: c.grounded, sliding: moved.is_sliding_down_slope, landed_speed, gap: c.gap, touched }
    }
}

/// Normal z above which a hit counts as an exactly level surface (see `move_character`).
const LEVEL: f32 = 0.99999;
