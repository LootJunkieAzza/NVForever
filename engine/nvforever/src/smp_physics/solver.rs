//! The solver (docs/spec-notes/smp-physics.md §3): every simulated bone is a point
//! moved by gravity, wind and its springs, held to its neighbours by the system's
//! constraints, solved by position a set number of times per step (position-based
//! dynamics), then pushed out of the body's capsules and the ground. A bone's turn comes
//! from where its first child is: the rest direction to that child turned onto the
//! current one. The bones not simulated follow the animation: the skeleton's own nodes
//! as given, the item's anchor bones at their rest place on their parent.
//!
//! [design] This is not FSMP's rigid-body solver (Bullet) and not a port of it; the
//! XML's values map onto it as §3 of the spec note says, tuned by eye.

use bevy::math::{Affine3A, Quat, Vec3};

/// A spring's pull: acceleration per unit of offset per unit of stiffness, 1/s². The
/// stiffness given to a link is the XML's divided by the moving bone's mass (as a
/// rigid body's spring accelerates a light body more) [tuned by eye: a coat row of
/// stiffness 2 and mass 0.02 (400/s²) holds its drape and swings back in about a
/// quarter of a second; gravity bends a level link of it by under 2 units].
pub const SPRING_SCALE: f32 = 4.0;
/// The most a spring moves a bone toward its target in one step, as a share of the way
/// (keeps a stiff spring at a low frame rate from overshooting) [design].
const MAX_SPRING_SHARE: f32 = 0.5;
/// A bone farther than this many rest lengths from its parent has gone wrong: the
/// system starts over at rest [design].
const BLOWN: f32 = 6.0;
/// How far past its rest length a joint may be left at the end of a substep.
const UNSTRETCHED: f32 = 1.02;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bone {
    /// Index of its parent in the rig (parents come first); `None` for a driven bone.
    pub parent: Option<usize>,
    /// Its rest transform on its parent (the rig's own space: the actor's skeleton root).
    pub rest_local: Affine3A,
    /// Its world transform is given every frame (a skeleton node).
    pub driven: bool,
    /// Simulated (mass above 0); else it follows its parent at rest (an anchor).
    pub dynamic: bool,
    pub inv_mass: f32,
    /// Bullet's linear damping: the speed keeps (1 - damping) of itself each second.
    pub damping: f32,
    /// How much of the wind it takes.
    pub wind: f32,
    /// Its collision radius, rig units.
    pub radius: f32,
    pub collide_body: bool,
    pub collide_ground: bool,
}

impl Bone {
    pub fn driven() -> Bone {
        Bone { parent: None, rest_local: Affine3A::IDENTITY, driven: true, dynamic: false, inv_mass: 0.0, damping: 0.0, wind: 0.0, radius: 0.0, collide_body: false, collide_ground: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LinkKind {
    /// `a` held at its rest distance from `b`, its direction within `swing` (radians)
    /// of its rest direction in `b`'s frame (`off`: `a`'s rest place there), pulled back
    /// toward it with stiffness `k`.
    Joint { off: Vec3, swing: f32, k: f32 },
    /// `a` and `b` kept within `slack` of their rest distance `rest`, pulled toward it
    /// with stiffness `k`.
    Spring { rest: f32, slack: f32, k: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub kind: LinkKind,
}

/// A body capsule in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

impl Capsule {
    /// The closest point of its axis to `p`.
    pub fn closest(&self, p: Vec3) -> Vec3 {
        let ab = self.b - self.a;
        let l2 = ab.length_squared();
        if l2 < 1e-8 {
            return self.a;
        }
        self.a + ab * ((p - self.a).dot(ab) / l2).clamp(0.0, 1.0)
    }
}

/// A rig: its bones (parents first) and links.
#[derive(Debug, Clone, Default)]
pub struct Rig {
    pub bones: Vec<Bone>,
    pub links: Vec<Link>,
    /// Per bone, its first child among the bones (what its turn follows).
    child: Vec<Option<usize>>,
    /// Per link, the rest distance between its bones (rig units).
    rest_len: Vec<f32>,
    /// The joint links whose first bone is simulated, nearest the root first: the last
    /// pass of each substep takes the stretch out of them in this order.
    unstretch: Vec<usize>,
}

impl Rig {
    /// A rig of `bones` and `links`, with `rest` the bones' rest world transforms (rig
    /// space) for the links' rest lengths. Links naming a bone out of range, or between
    /// two bones that are not simulated, are left out.
    pub fn new(bones: Vec<Bone>, links: Vec<Link>, rest: &[Affine3A]) -> Rig {
        let n = bones.len();
        let mut child = vec![None; n];
        for (i, b) in bones.iter().enumerate() {
            if let Some(p) = b.parent {
                if p < i && child[p].is_none() {
                    child[p] = Some(i);
                }
            }
        }
        let links: Vec<Link> = links.into_iter().filter(|l| l.a < n && l.b < n && l.a != l.b && (bones[l.a].dynamic || bones[l.b].dynamic)).collect();
        let rest_len = links.iter().map(|l| (rest[l.a].translation - rest[l.b].translation).length()).collect();
        let mut depth = vec![0usize; n];
        for (i, b) in bones.iter().enumerate() {
            if let Some(p) = b.parent.filter(|&p| p < i) {
                depth[i] = depth[p] + 1;
            }
        }
        let mut unstretch: Vec<usize> = (0..links.len()).filter(|&li| matches!(links[li].kind, LinkKind::Joint { .. }) && bones[links[li].a].dynamic).collect();
        unstretch.sort_by_key(|&li| depth[links[li].a]);
        Rig { bones, links, child, rest_len, unstretch }
    }

    pub fn dynamic_count(&self) -> usize {
        self.bones.iter().filter(|b| b.dynamic).count()
    }
}

/// The world a step runs in.
#[derive(Debug, Clone, Copy)]
pub struct Env<'a> {
    /// World-space acceleration of gravity.
    pub gravity: Vec3,
    /// World-space acceleration of the wind on a bone of wind factor 1.
    pub wind: Vec3,
    pub capsules: &'a [Capsule],
    /// Per bone and capsule (`bone * capsules.len() + capsule`), the capsule's radius for
    /// that bone in rig units, when it is less than the capsule's own: a bone resting
    /// close to the body is held no closer than it rests. Empty: every bone gets `r`.
    pub bone_r: &'a [f32],
    /// The ground's height (world y), if the bones rest on it.
    pub ground: Option<f32>,
    /// World units per rig unit (the actor's scale).
    pub scale: f32,
    pub iterations: u32,
    /// Most steps per second, and most steps per frame.
    pub min_fps: f32,
    pub max_substeps: u32,
}

/// A rig's simulated state.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub pos: Vec<Vec3>,
    vel: Vec<Vec3>,
    rot: Vec<Quat>,
    /// Every bone's world transform after the last step (what is drawn).
    pub world: Vec<Affine3A>,
    prev_driven: Vec<Affine3A>,
    pub live: bool,
}

fn lerp_affine(a: &Affine3A, b: &Affine3A, t: f32) -> Affine3A {
    let (sa, ra, ta) = a.to_scale_rotation_translation();
    let (sb, rb, tb) = b.to_scale_rotation_translation();
    Affine3A::from_scale_rotation_translation(sa.lerp(sb, t), ra.slerp(rb, t), ta.lerp(tb, t))
}

fn rotation(a: &Affine3A) -> Quat {
    a.to_scale_rotation_translation().1
}

/// How many steps a frame of `dt` seconds takes, and each one's length: at least one,
/// as many as `min_fps` asks for, at most `max_substeps` (the rest of a long frame is
/// dropped, not caught up) [documented: the INI's iMinFps and iMaxSubSteps comments].
pub fn substeps(dt: f32, min_fps: f32, max_substeps: u32) -> (u32, f32) {
    if dt <= 0.0 || !dt.is_finite() {
        return (0, 0.0);
    }
    let h = 1.0 / min_fps.max(1.0);
    let n = (dt / h).ceil().max(1.0) as u32;
    if n > max_substeps.max(1) {
        (max_substeps.max(1), h)
    } else {
        (n, dt / n as f32)
    }
}

impl State {
    /// Start over: every bone at its rest place on the pose `driven` gives.
    pub fn reset(&mut self, rig: &Rig, driven: &[Affine3A]) {
        let n = rig.bones.len();
        self.world = vec![Affine3A::IDENTITY; n];
        for (i, b) in rig.bones.iter().enumerate() {
            self.world[i] = match b.parent {
                _ if b.driven => driven.get(i).copied().unwrap_or(Affine3A::IDENTITY),
                Some(p) => self.world[p] * b.rest_local,
                None => b.rest_local,
            };
        }
        self.pos = self.world.iter().map(|w| Vec3::from(w.translation)).collect();
        self.vel = vec![Vec3::ZERO; n];
        self.rot = self.world.iter().map(rotation).collect();
        self.prev_driven = driven.to_vec();
        self.live = true;
    }

    /// Where a link's `b` frame says its `a` belongs: `b`'s own turn when it is not
    /// simulated, else the turn it would have unswung (its parent's turn and its rest).
    fn reference(&self, rig: &Rig, b: usize) -> Quat {
        let bone = &rig.bones[b];
        match bone.parent {
            Some(p) if bone.dynamic => self.rot[p] * rotation(&bone.rest_local),
            _ => self.rot[b],
        }
    }

    fn update_turns(&mut self, rig: &Rig) {
        for (i, b) in rig.bones.iter().enumerate() {
            if !b.dynamic {
                self.rot[i] = rotation(&self.world[i]);
                continue;
            }
            let Some(p) = b.parent else { continue };
            let unswung = self.rot[p] * rotation(&b.rest_local);
            self.rot[i] = match rig.child[i] {
                Some(c) => {
                    let rest_dir = (unswung * Vec3::from(rig.bones[c].rest_local.translation)).normalize_or_zero();
                    let now = (self.pos[c] - self.pos[i]).normalize_or_zero();
                    if rest_dir == Vec3::ZERO || now == Vec3::ZERO {
                        unswung
                    } else {
                        (Quat::from_rotation_arc(rest_dir, now) * unswung).normalize()
                    }
                }
                None => unswung,
            };
        }
    }

    /// Place the bones that are not simulated on `driven` (this step's pose).
    fn place_kinematic(&mut self, rig: &Rig, driven: &[Affine3A]) {
        for (i, b) in rig.bones.iter().enumerate() {
            if b.dynamic {
                continue;
            }
            self.world[i] = match b.parent {
                _ if b.driven => driven[i],
                Some(p) => {
                    // An anchor on a simulated bone follows its turn and place.
                    let parent = if rig.bones[p].dynamic { Affine3A::from_rotation_translation(self.rot[p], self.pos[p]) * Affine3A::from_scale(Vec3::splat(self.scale_of(p))) } else { self.world[p] };
                    parent * b.rest_local
                }
                None => b.rest_local,
            };
            self.pos[i] = self.world[i].translation.into();
        }
    }

    fn scale_of(&self, i: usize) -> f32 {
        self.world[i].matrix3.x_axis.length()
    }

    /// Advance the rig by a frame of `dt` seconds to the pose `driven` (world transforms
    /// of the driven bones, by bone index; others ignored). True when the system blew up
    /// and was started over at rest.
    pub fn step(&mut self, rig: &Rig, driven: &[Affine3A], env: &Env, dt: f32) -> bool {
        if !self.live || self.pos.len() != rig.bones.len() || self.prev_driven.len() != driven.len() {
            self.reset(rig, driven);
            return false;
        }
        let (n, h) = substeps(dt, env.min_fps, env.max_substeps);
        if n == 0 {
            return false;
        }
        let spring = |k: f32| (k * SPRING_SCALE * h * h).clamp(0.0, MAX_SPRING_SHARE);
        let prev = std::mem::take(&mut self.prev_driven);
        let mut pose = driven.to_vec();
        let mut start = vec![Vec3::ZERO; rig.bones.len()];
        for s in 0..n {
            let t = (s + 1) as f32 / n as f32;
            for (i, b) in rig.bones.iter().enumerate() {
                if b.driven {
                    pose[i] = lerp_affine(&prev[i], &driven[i], t);
                }
            }
            self.place_kinematic(rig, &pose);
            self.update_turns(rig);
            // Gravity, wind and damping.
            for (i, b) in rig.bones.iter().enumerate() {
                if !b.dynamic {
                    continue;
                }
                let keep = (1.0 - b.damping).clamp(0.0, 1.0).powf(h);
                self.vel[i] = self.vel[i] * keep + (env.gravity + env.wind * b.wind) * h;
                start[i] = self.pos[i];
                self.pos[i] += self.vel[i] * h;
            }
            // Springs toward the rest shape.
            for (li, l) in rig.links.iter().enumerate() {
                match l.kind {
                    LinkKind::Joint { off, k, .. } if k > 0.0 && rig.bones[l.a].dynamic => {
                        let target = self.pos[l.b] + self.reference(rig, l.b) * off * env.scale;
                        let p = self.pos[l.a];
                        self.pos[l.a] = p + (target - p) * spring(k);
                    }
                    LinkKind::Spring { k, .. } if k > 0.0 => {
                        let d = self.pos[l.a] - self.pos[l.b];
                        let len = d.length();
                        if len > 1e-6 {
                            let err = (len - rig.rest_len[li] * env.scale) * spring(k);
                            self.share(rig, l.a, l.b, d / len * err);
                        }
                    }
                    _ => {}
                }
            }
            for _ in 0..env.iterations.max(1) {
                for (li, l) in rig.links.iter().enumerate() {
                    self.solve(rig, li, l, env.scale);
                }
                self.collide(rig, env);
                self.update_turns(rig);
            }
            // A few iterations leave a long chain stretched after a hard jerk of its root:
            // take what is left out of the joints from the root outward.
            for &li in &rig.unstretch {
                let l = &rig.links[li];
                let d = self.pos[l.a] - self.pos[l.b];
                let len = d.length();
                let rest = rig.rest_len[li] * env.scale;
                if len > rest * UNSTRETCHED {
                    self.pos[l.a] = self.pos[l.b] + d / len * rest * UNSTRETCHED;
                }
            }
            self.update_turns(rig);
            for (i, b) in rig.bones.iter().enumerate() {
                if b.dynamic {
                    self.vel[i] = (self.pos[i] - start[i]) / h;
                }
            }
        }
        self.prev_driven = driven.to_vec();
        // A blown-up system starts over at rest.
        let blown = rig.bones.iter().enumerate().any(|(i, b)| {
            b.dynamic
                && (!self.pos[i].is_finite()
                    || b.parent.is_some_and(|p| {
                        let rest = Vec3::from(b.rest_local.translation).length().max(1.0) * env.scale;
                        (self.pos[i] - self.pos[p]).length() > rest * BLOWN
                    }))
        });
        if blown {
            self.reset(rig, driven);
            return true;
        }
        for (i, b) in rig.bones.iter().enumerate() {
            if b.dynamic {
                let s = Vec3::splat(self.world.get(b.parent.unwrap_or(i)).map_or(1.0, |w| w.matrix3.x_axis.length()) * b.rest_local.matrix3.x_axis.length());
                self.world[i] = Affine3A::from_scale_rotation_translation(s, self.rot[i], self.pos[i]);
            }
        }
        false
    }

    /// Close a gap `d` (to take off `a - b`): `a` moves by `-d` and `b` by `+d`, each by
    /// its share of the two inverse masses (a bone not simulated does not move).
    fn share(&mut self, rig: &Rig, a: usize, b: usize, d: Vec3) {
        let wa = if rig.bones[a].dynamic { rig.bones[a].inv_mass } else { 0.0 };
        let wb = if rig.bones[b].dynamic { rig.bones[b].inv_mass } else { 0.0 };
        let w = wa + wb;
        if w <= 0.0 {
            return;
        }
        self.pos[a] -= d * (wa / w);
        self.pos[b] += d * (wb / w);
    }

    fn solve(&mut self, rig: &Rig, li: usize, l: &Link, scale: f32) {
        let d = self.pos[l.a] - self.pos[l.b];
        let len = d.length();
        if len < 1e-6 {
            return;
        }
        match l.kind {
            LinkKind::Joint { off, swing, .. } => {
                let rest = rig.rest_len[li] * scale;
                let mut dir = d / len;
                let want = (self.reference(rig, l.b) * off).normalize_or_zero();
                if want != Vec3::ZERO {
                    let angle = dir.dot(want).clamp(-1.0, 1.0).acos();
                    if angle > swing {
                        let axis = dir.cross(want);
                        let axis = if axis.length_squared() > 1e-10 { axis.normalize() } else { want.any_orthonormal_vector() };
                        dir = Quat::from_axis_angle(axis, angle - swing) * dir;
                    }
                }
                let target = dir * rest;
                self.share(rig, l.a, l.b, d - target);
            }
            LinkKind::Spring { slack, .. } => {
                let rest = rig.rest_len[li] * scale;
                let (lo, hi) = ((rest - slack * scale).max(0.0), rest + slack * scale);
                let want = len.clamp(lo, hi);
                if want != len {
                    self.share(rig, l.a, l.b, d / len * (len - want));
                }
            }
        }
    }

    fn collide(&mut self, rig: &Rig, env: &Env) {
        for (i, b) in rig.bones.iter().enumerate() {
            if !b.dynamic {
                continue;
            }
            let r = b.radius * env.scale;
            if b.collide_body {
                let n = env.capsules.len();
                for (ci, c) in env.capsules.iter().enumerate() {
                    let cr = env.bone_r.get(i * n + ci).map_or(c.r, |&br| c.r.min(br * env.scale));
                    if cr <= 0.0 {
                        continue;
                    }
                    let at = c.closest(self.pos[i]);
                    let d = self.pos[i] - at;
                    let len = d.length();
                    let min = cr + r;
                    if len < min {
                        let out = if len > 1e-6 { d / len } else { Vec3::Y };
                        self.pos[i] = at + out * min;
                    }
                }
            }
            if let Some(g) = env.ground.filter(|_| b.collide_ground) {
                if self.pos[i].y < g + r {
                    self.pos[i].y = g + r;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain of `n` simulated links hanging from a driven root along +x, `len` apart.
    fn chain(n: usize, len: f32, k: f32) -> (Rig, Vec<Affine3A>) {
        let mut bones = vec![Bone::driven()];
        let mut links = Vec::new();
        let mut rest = vec![Affine3A::IDENTITY];
        for i in 1..=n {
            let local = Affine3A::from_translation(Vec3::X * len);
            bones.push(Bone { parent: Some(i - 1), rest_local: local, driven: false, dynamic: true, inv_mass: 10.0, damping: 0.5, wind: 1.0, radius: 0.5, collide_body: true, collide_ground: true });
            rest.push(rest[i - 1] * local);
            links.push(Link { a: i, b: i - 1, kind: LinkKind::Joint { off: Vec3::X * len, swing: 3.0, k } });
        }
        (Rig::new(bones, links, &rest), rest)
    }

    fn env(capsules: &[Capsule]) -> Env<'_> {
        Env { gravity: Vec3::new(0.0, -686.7, 0.0), wind: Vec3::ZERO, capsules, bone_r: &[], ground: None, scale: 1.0, iterations: 10, min_fps: 100.0, max_substeps: 4 }
    }

    #[test]
    fn substeps_follow_min_fps_and_drop_the_rest_of_a_long_frame() {
        assert_eq!(substeps(1.0 / 60.0, 100.0, 4), (2, 1.0 / 120.0));
        assert_eq!(substeps(1.0 / 200.0, 100.0, 4), (1, 1.0 / 200.0));
        assert_eq!(substeps(0.5, 100.0, 4), (4, 0.01));
        assert_eq!(substeps(0.0, 100.0, 4).0, 0);
    }

    #[test]
    fn a_loose_chain_falls_to_hang_below_its_root_without_stretching() {
        let (rig, _) = chain(4, 5.0, 0.0);
        let driven = vec![Affine3A::IDENTITY; 5];
        let mut s = State::default();
        let e = env(&[]);
        for _ in 0..600 {
            s.step(&rig, &driven, &e, 1.0 / 60.0);
        }
        for i in 1..5 {
            let d = (s.pos[i] - s.pos[i - 1]).length();
            assert!((d - 5.0).abs() < 0.05, "link {i} is {d} long");
        }
        assert!(s.pos[4].y < -19.0, "the end hangs down: {:?}", s.pos[4]);
        assert!(s.pos[4].x.abs() < 1.0);
    }

    #[test]
    fn a_stiff_chain_holds_its_rest_shape_against_gravity_better_than_a_loose_one() {
        let driven = vec![Affine3A::IDENTITY; 5];
        let e = env(&[]);
        let sag = |k: f32| {
            let (rig, _) = chain(4, 5.0, k);
            let mut s = State::default();
            for _ in 0..600 {
                s.step(&rig, &driven, &e, 1.0 / 60.0);
            }
            -s.pos[4].y
        };
        assert!(sag(200.0) < sag(0.0) - 5.0);
    }

    #[test]
    fn a_cone_limit_holds_the_swing() {
        let (mut rig, rest) = chain(1, 5.0, 0.0);
        rig.links[0].kind = LinkKind::Joint { off: Vec3::X * 5.0, swing: 0.3, k: 0.0 };
        let rig = Rig::new(rig.bones, rig.links, &rest);
        let driven = vec![Affine3A::IDENTITY; 2];
        let mut s = State::default();
        for _ in 0..300 {
            s.step(&rig, &driven, &env(&[]), 1.0 / 60.0);
        }
        let angle = s.pos[1].normalize().angle_between(Vec3::X);
        assert!(angle <= 0.31, "swung {angle}");
    }

    #[test]
    fn bones_stay_out_of_capsules_and_off_the_ground() {
        let (rig, _) = chain(4, 5.0, 0.0);
        let driven = vec![Affine3A::IDENTITY; 5];
        let caps = [Capsule { a: Vec3::new(10.0, -30.0, 0.0), b: Vec3::new(10.0, 0.0, 0.0), r: 3.0 }];
        let mut e = env(&caps);
        e.ground = Some(-12.0);
        let mut s = State::default();
        for _ in 0..600 {
            s.step(&rig, &driven, &e, 1.0 / 60.0);
        }
        for i in 1..5 {
            assert!(s.pos[i].y >= -12.0 + 0.5 - 1e-3);
            assert!((s.pos[i] - caps[0].closest(s.pos[i])).length() >= 3.5 - 0.05, "bone {i} in the capsule: {:?}", s.pos[i]);
        }
    }

    /// A bone resting close to a capsule is held at its rest distance; the others get the
    /// whole capsule (the thigh through a coat's front panel, 2026-11-15, came from one
    /// close bone shrinking the capsule for all).
    #[test]
    fn a_close_resting_bone_shrinks_the_capsule_for_itself_only() {
        let (rig, _) = chain(4, 5.0, 0.0);
        let driven = vec![Affine3A::IDENTITY; 5];
        // A thigh across the chain's hanging line, 3 below the root; bone 1 rests 1 from it.
        let caps = [Capsule { a: Vec3::new(0.0, -8.0, -20.0), b: Vec3::new(0.0, -8.0, 20.0), r: 6.0 }];
        let bone_r = [6.0, 0.5, 6.0, 6.0, 6.0];
        let mut e = env(&caps);
        e.bone_r = &bone_r;
        let mut s = State::default();
        for _ in 0..600 {
            s.step(&rig, &driven, &e, 1.0 / 60.0);
        }
        for i in 2..5 {
            assert!((s.pos[i] - caps[0].closest(s.pos[i])).length() >= 6.5 - 0.05, "bone {i} in the capsule: {:?}", s.pos[i]);
        }
        assert!((s.pos[1] - caps[0].closest(s.pos[1])).length() >= 1.0 - 0.05);
    }

    #[test]
    fn a_running_turning_root_never_stretches_or_explodes_the_chain() {
        let (rig, _) = chain(5, 4.0, 4.0);
        let mut s = State::default();
        let e = env(&[]);
        for f in 0..2000 {
            let t = f as f32 / 60.0;
            let root = Affine3A::from_rotation_translation(Quat::from_rotation_y(t * 6.0), Vec3::new(t * 400.0, (t * 9.0).sin() * 8.0, 0.0));
            let driven = vec![root; 6];
            s.step(&rig, &driven, &e, if f % 7 == 0 { 1.0 / 20.0 } else { 1.0 / 144.0 });
            for i in 1..6 {
                assert!(s.pos[i].is_finite());
                let d = (s.pos[i] - s.pos[i - 1]).length();
                assert!(d < 4.0 * 1.1, "frame {f}: link {i} is {d} long");
            }
        }
    }

    #[test]
    fn anchors_follow_their_parent_at_rest() {
        let local = Affine3A::from_translation(Vec3::Y * 2.0);
        let anchor = Bone { parent: Some(0), rest_local: local, driven: false, dynamic: false, ..Bone::driven() };
        let rig = Rig::new(vec![Bone::driven(), anchor], Vec::new(), &[Affine3A::IDENTITY, local]);
        let mut s = State::default();
        let root = Affine3A::from_translation(Vec3::new(5.0, 0.0, 0.0));
        s.step(&rig, &[root, Affine3A::IDENTITY], &env(&[]), 1.0 / 60.0);
        s.step(&rig, &[root, Affine3A::IDENTITY], &env(&[]), 1.0 / 60.0);
        assert!((s.pos[1] - Vec3::new(5.0, 2.0, 0.0)).length() < 1e-4);
    }
}
