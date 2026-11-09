//! What scripts read and change of a reference's bodies (JIP LN's collision object
//! functions, `fnv_game::nvse_jip13`): each body by the node its collision object hangs
//! on, its velocities, layer and surface and mass properties.

use crate::world::{Owner, PhysWorld};
use rapier3d::prelude::*;

/// One body of a reference as a script sees it, in game units and seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptBody {
    /// The node its collision object hangs on (`CollisionObject::node`).
    pub node: Option<String>,
    pub layer: u8,
    pub dynamic: bool,
    /// Game units a second, world axes.
    pub linvel: [f32; 3],
    /// Radians a second, world axes.
    pub angvel: [f32; 3],
    /// Row-major 3×3, body to world.
    pub rotation: [f32; 9],
    /// Kilograms; 0 for a body that does not move.
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// The principal moments of inertia, kg·m².
    pub inertia: [f32; 3],
}

/// A property [`PhysWorld::set_script_property`] changes, in JIP's order (0-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyProperty {
    Friction,
    Restitution,
    LinearDamping,
    AngularDamping,
    InertiaX,
    InertiaY,
    InertiaZ,
    Mass,
}

impl BodyProperty {
    pub fn from_index(i: i32) -> Option<BodyProperty> {
        use BodyProperty::*;
        [Friction, Restitution, LinearDamping, AngularDamping, InertiaX, InertiaY, InertiaZ, Mass].get(usize::try_from(i).ok()?).copied()
    }
}

fn arr(v: Vector) -> [f32; 3] {
    [v.x, v.y, v.z]
}

impl PhysWorld {
    /// A reference's bodies whose node is `node` (case ignored), or all of them for "*"
    /// [documented: SetCollisionObjLayerType, "an asterisk"]: their places in the
    /// reference's list.
    pub fn script_bodies_at(&self, form_id: u32, node: &str) -> Vec<usize> {
        let nodes = self.ref_nodes.get(&form_id);
        let n = self.refs.get(&form_id).map_or(0, Vec::len);
        (0..n).filter(|&i| node == "*" || nodes.and_then(|v| v.get(i)).and_then(|o| o.as_deref()).is_some_and(|m| m.eq_ignore_ascii_case(node))).collect()
    }

    /// The reference's bodies as scripts see them, in its list's order.
    pub fn script_bodies(&self, form_id: u32) -> Vec<ScriptBody> {
        let k = self.k;
        let Some(bodies) = self.refs.get(&form_id) else { return Vec::new() };
        bodies
            .iter()
            .enumerate()
            .filter_map(|(i, &h)| {
                let b = self.w.bodies.get(h)?;
                let layer = match Owner::decode(b.user_data) {
                    Some(Owner::Ref { layer, .. }) => layer,
                    _ => 0,
                };
                let c = b.colliders().first().and_then(|&c| self.w.colliders.get(c));
                let p = b.position();
                let rotation = rapier3d::math::Matrix::from_quat(p.rotation).transpose().to_cols_array();
                let props = b.mass_properties().local_mprops;
                let dynamic = b.is_dynamic();
                Some(ScriptBody {
                    node: self.ref_nodes.get(&form_id).and_then(|v| v.get(i)).cloned().flatten(),
                    layer,
                    dynamic,
                    linvel: arr(b.linvel() / k),
                    angvel: arr(b.angvel()),
                    rotation,
                    mass: if dynamic { b.mass() } else { 0.0 },
                    friction: c.map_or(0.0, |c| c.friction()),
                    restitution: c.map_or(0.0, |c| c.restitution()),
                    linear_damping: b.linear_damping(),
                    angular_damping: b.angular_damping(),
                    inertia: if dynamic { arr(props.principal_inertia()) } else { [0.0; 3] },
                })
            })
            .collect()
    }

    /// Set the linear velocity (game units a second) and the angular one (radians a
    /// second), world axes, of the reference's `index`-th body if it moves; whether it does.
    pub fn set_script_velocity(&mut self, form_id: u32, index: usize, linvel: Option<[f32; 3]>, angvel: Option<[f32; 3]>) -> bool {
        let k = self.k;
        let Some(&h) = self.refs.get(&form_id).and_then(|v| v.get(index)) else { return false };
        let Some(b) = self.w.bodies.get_mut(h).filter(|b| b.is_dynamic()) else { return false };
        if let Some(v) = linvel {
            b.set_linvel(Vector::new(v[0], v[1], v[2]) * k, true);
        }
        if let Some(w) = angvel {
            b.set_angvel(Vector::new(w[0], w[1], w[2]), true);
        }
        self.struck.push(h);
        true
    }

    /// Set a surface or mass property of the reference's `index`-th body; whether it was
    /// set. Friction and restitution are its colliders'; damping and mass its own (a
    /// moving body's); the inertia is not set.
    pub fn set_script_property(&mut self, form_id: u32, index: usize, prop: BodyProperty, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        let Some(&h) = self.refs.get(&form_id).and_then(|v| v.get(index)) else { return false };
        let Some(b) = self.w.bodies.get(h) else { return false };
        let colliders: Vec<ColliderHandle> = b.colliders().to_vec();
        match prop {
            BodyProperty::Friction | BodyProperty::Restitution => {
                for c in colliders {
                    if let Some(c) = self.w.colliders.get_mut(c) {
                        if prop == BodyProperty::Friction {
                            c.set_friction(value.max(0.0));
                        } else {
                            c.set_restitution(value.max(0.0));
                        }
                    }
                }
            }
            BodyProperty::Mass => self.set_mass(h, value),
            _ => {
                let Some(b) = self.w.bodies.get_mut(h).filter(|b| b.is_dynamic()) else { return false };
                match prop {
                    BodyProperty::LinearDamping => b.set_linear_damping(value.max(0.0)),
                    BodyProperty::AngularDamping => b.set_angular_damping(value.max(0.0)),
                    // [decision: the moments of inertia are rapier's, from the shapes and
                    // the mass; a script's are not taken]
                    _ => return false,
                }
            }
        }
        self.struck.push(h);
        true
    }

    /// Change the layer of the reference's `index`-th body: what the queries and the
    /// layer filters see of it from now on; whether it has that body.
    pub fn set_script_layer(&mut self, form_id: u32, index: usize, layer: u8) -> bool {
        let Some(&h) = self.refs.get(&form_id).and_then(|v| v.get(index)) else { return false };
        let Some(b) = self.w.bodies.get_mut(h) else { return false };
        let Some(Owner::Ref { form_id: f, object, .. }) = Owner::decode(b.user_data) else { return false };
        let owner = Owner::Ref { form_id: f, object, layer };
        b.user_data = owner.encode();
        let colliders: Vec<ColliderHandle> = b.colliders().to_vec();
        for (i, c) in colliders.into_iter().enumerate() {
            if let Some(c) = self.w.colliders.get_mut(c) {
                c.user_data = owner.encode_segment(i as u16);
            }
        }
        self.mark_dirty_bodies();
        true
    }

    /// Switch the collision of all the reference's bodies off or back on (JIP's
    /// ToggleObjectCollision); how many colliders changed.
    pub fn set_script_collision(&mut self, form_id: u32, on: bool) -> usize {
        let Some(bodies) = self.refs.get(&form_id).cloned() else { return 0 };
        let mut n = 0;
        for h in bodies {
            let Some(b) = self.w.bodies.get(h) else { continue };
            for c in b.colliders().to_vec() {
                if let Some(c) = self.w.colliders.get_mut(c) {
                    if c.is_enabled() != on {
                        c.set_enabled(on);
                        n += 1;
                    }
                }
            }
        }
        if n > 0 {
            self.mark_dirty_bodies();
        }
        n
    }

    /// The world's gravity, game units a second squared (each axis).
    pub fn script_gravity(&self) -> [f32; 3] {
        arr(self.w.gravity / self.k)
    }

    /// Set the world's gravity for the moving bodies (game units a second squared); the
    /// characters keep their own.
    pub fn set_script_gravity(&mut self, g: [f32; 3]) {
        if g.iter().all(|x| x.is_finite()) {
            self.w.gravity = Vector::new(g[0], g[1], g[2]) * self.k;
        }
    }
}
