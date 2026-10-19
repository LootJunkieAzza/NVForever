//! Projectiles, melee swings and blasts against a world the caller provides
//! (docs/design/combat.md §5.1, §5.4, §5.8). The engine implements [`CombatWorld`] with
//! `fnv-phys` queries and actor hit volumes; tests use small stand-ins.
//!
//! Projectile behaviour by PROJ type and flags [documented: GECK wiki Projectile, owner
//! export 2026-10-09]: a missile flies straight (with its gravity), a lobber is thrown and
//! bounces, a beam hits at once whatever its speed, a flame is a missile that actors do
//! not stop; hitscan finds the hit at once and the damage arrives after distance ÷ speed;
//! the explosion goes off on impact, or with Alt. Trigger on a timer or proximity, or on
//! detonation for Detonates. A mine (`crate::mine::is_mine`) does not trigger itself in
//! flight: once it comes to rest it is [`ProjEvent::Settled`] and the engine places it
//! as a mine in the world (`nvf_play::mines`), armed at once [owner 2026-10-19: "your own
//! mines arm immediately after leaving the player hand"]. How gravity scales is a hypothesis: the PROJ value
//! multiplies the world's gravity (9.81 m/s² at 70 units per metre, `fnv-phys`).

use serde::Serialize;

use crate::data::{ExplosionStats, ProjectileStats};
use crate::damage::explosion_damage;
use crate::tuning::CombatTuning;
use crate::vec::{self, V3};
use fnv_sim::FormId;

/// An actor's handle in the caller's world.
pub type ActorId = u64;

/// A ray hit.
#[derive(Debug, Clone, Serialize)]
pub struct WorldHit {
    pub point: V3,
    pub normal: V3,
    pub distance: f32,
    /// The actor hit, if the ray hit an actor's hit volume.
    pub actor: Option<ActorId>,
    /// The body part hit (its index in the actor's body part data), when the actor has
    /// part volumes (step C8).
    pub part: Option<u8>,
    /// The placed reference hit (its FormID), for destructible objects (step C7).
    pub object: Option<u64>,
    /// The collider is on the small-transparent layer (chain-link fences, …).
    pub small_transparent: bool,
    /// Havok material code, for impact effects.
    pub material: Option<u32>,
}

/// What combat asks of the world.
pub trait CombatWorld {
    /// The first hit on the segment `from` → `to`, ignoring the listed actors.
    fn ray(&self, from: V3, to: V3, ignore: &[ActorId]) -> Option<WorldHit>;
    /// Actors whose centre lies within `radius` of `point`, with that centre.
    fn actors_near(&self, point: V3, radius: f32) -> Vec<(ActorId, V3)>;
    /// Points on an actor's body to aim a blow at, the likeliest first (the centres of
    /// its body part capsules, widest first). None by default: the centre is aimed at.
    fn aim_points(&self, _actor: ActorId) -> Vec<V3> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Motion {
    Missile,
    Lobber,
    Beam,
    Flame,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ProjEvent {
    /// A hit to apply after `delay` seconds (hitscan: distance ÷ speed).
    Hit { actor: Option<ActorId>, part: Option<u8>, object: Option<u64>, point: V3, normal: V3, material: Option<u32>, delay: f32 },
    /// The projectile's explosion goes off here.
    Explode { explosion: FormId, point: V3 },
    /// Out of range or out of time without a hit.
    Expired { point: V3 },
    /// A mine came to rest here, on a surface with this normal: it leaves flight.
    Settled { point: V3, normal: V3 },
}

/// A projectile in flight.
#[derive(Debug, Clone, Serialize)]
pub struct Projectile {
    pub pos: V3,
    pub vel: V3,
    pub shooter: Option<ActorId>,
    pub motion: Motion,
    pub hitscan: bool,
    pub speed: f32,
    /// Downward acceleration, units/s².
    pub gravity: f32,
    pub range: f32,
    pub travelled: f32,
    pub age: f32,
    pub explosion: Option<FormId>,
    pub alt_trigger: bool,
    pub timer: f32,
    pub proximity: f32,
    pub detonates: bool,
    pub pass_small_transparent: bool,
    pub bounce: f32,
    pub landed: bool,
    pub alive: bool,
    /// A mine: it settles instead of triggering itself.
    pub mine: bool,
    /// Actors a flame has already hit.
    hit_actors: Vec<ActorId>,
}

/// Lobbers stop bouncing below this speed, units/s [design].
const REST_SPEED: f32 = 40.0;
/// How far past a surface a projectile restarts after passing through, units.
const NUDGE: f32 = 0.5;
/// Lifetime of a projectile with no other end, seconds [design].
const MAX_AGE: f32 = 60.0;

impl Projectile {
    /// A projectile leaving `origin` along `dir`. `explosion` is the chain's (the PROJ's
    /// explosion when its Explosion flag is set); `world_gravity` is units/s².
    pub fn spawn(p: &ProjectileStats, explosion: Option<FormId>, origin: V3, dir: V3, shooter: Option<ActorId>, world_gravity: f32, t: &CombatTuning) -> Projectile {
        let motion = match p.kind.as_deref() {
            Some("Lobber") => Motion::Lobber,
            Some("Beam") | Some("Continuous Beam") => Motion::Beam,
            Some("Flame") => Motion::Flame,
            _ => Motion::Missile,
        };
        let dir = vec::normalize(dir);
        Projectile {
            pos: origin,
            vel: vec::scale(dir, p.speed),
            shooter,
            motion,
            hitscan: p.has("Hitscan"),
            speed: p.speed,
            gravity: p.gravity * world_gravity,
            range: if p.range > 0.0 { p.range } else { f32::INFINITY },
            travelled: 0.0,
            age: 0.0,
            explosion,
            alt_trigger: p.has("Alt. Trigger"),
            timer: p.alt_trigger_timer,
            proximity: p.alt_trigger_proximity,
            detonates: p.has("Detonates"),
            pass_small_transparent: p.has("Pass Through Small Transparent"),
            // The PROJ's Bouncy Mult multiplies the lobbers' bounce [hypothesis: from its
            // name; taken as the share of speed kept, it was 1.0 for the frag grenade and
            // the frag mine and they bounced at full speed until their fuse ran out or for
            // ever, so a thrown mine never came to rest].
            bounce: t.default_bounce * if p.bouncy_mult > 0.0 { p.bouncy_mult } else { 1.0 },
            landed: false,
            alive: true,
            mine: crate::mine::is_mine(p),
            hit_actors: Vec::new(),
        }
    }

    fn ignore(&self) -> Vec<ActorId> {
        self.shooter.iter().copied().chain(self.hit_actors.iter().copied()).collect()
    }

    /// The first hit on a segment, passing through small-transparent colliders when the
    /// projectile may.
    fn trace(&self, world: &dyn CombatWorld, from: V3, to: V3) -> Option<WorldHit> {
        let ignore = self.ignore();
        let total = vec::len(vec::sub(to, from));
        let dir = vec::normalize(vec::sub(to, from));
        let mut start = from;
        let mut done = 0.0;
        for _ in 0..8 {
            let mut h = world.ray(start, to, &ignore)?;
            h.distance += done;
            if h.small_transparent && self.pass_small_transparent && h.actor.is_none() && h.distance + NUDGE < total {
                done = h.distance + NUDGE;
                start = vec::add(from, vec::scale(dir, done));
                continue;
            }
            return Some(h);
        }
        None
    }

    /// The explosion if it goes off on impact: not with Alt. Trigger (timer, proximity)
    /// nor Detonates (it waits for `detonate`) [documented: GECK wiki Projectile].
    fn impact_explosion(&self) -> Option<FormId> {
        self.explosion.filter(|_| !self.alt_trigger && !self.detonates)
    }

    /// Advance by `dt` seconds.
    pub fn step(&mut self, world: &dyn CombatWorld, dt: f32) -> Vec<ProjEvent> {
        let mut out = Vec::new();
        if !self.alive {
            return out;
        }
        let first = self.age == 0.0;
        self.age += dt;
        // Instant traces: beams and hitscan projectiles resolve on their first step.
        if first && (self.hitscan || self.motion == Motion::Beam) && self.motion != Motion::Lobber {
            let dir = vec::normalize(self.vel);
            let reach = if self.range.is_finite() { self.range } else { 100_000.0 };
            let end = vec::add(self.pos, vec::scale(dir, reach));
            self.alive = false;
            match self.trace(world, self.pos, end) {
                Some(h) => {
                    let delay = if self.motion == Motion::Beam || self.speed <= 0.0 { 0.0 } else { h.distance / self.speed };
                    out.push(ProjEvent::Hit { actor: h.actor, part: h.part, object: h.object, point: h.point, normal: h.normal, material: h.material, delay });
                    if let Some(e) = self.impact_explosion() {
                        out.push(ProjEvent::Explode { explosion: e, point: h.point });
                    }
                }
                None => out.push(ProjEvent::Expired { point: end }),
            }
            return out;
        }
        // Triggers that do not need movement.
        if let Some(e) = self.explosion.filter(|_| self.alt_trigger && !self.mine) {
            let armed = self.age >= self.timer;
            if self.proximity > 0.0 {
                if armed && world.actors_near(self.pos, self.proximity).iter().any(|(a, _)| Some(*a) != self.shooter) {
                    self.alive = false;
                    out.push(ProjEvent::Explode { explosion: e, point: self.pos });
                    return out;
                }
            } else if self.timer > 0.0 && armed && !self.detonates {
                self.alive = false;
                out.push(ProjEvent::Explode { explosion: e, point: self.pos });
                return out;
            }
        }
        if self.age > MAX_AGE && !self.detonates && self.proximity <= 0.0 {
            self.alive = false;
            out.push(ProjEvent::Expired { point: self.pos });
            return out;
        }
        if self.landed {
            return out;
        }
        // Swept flight.
        let vel = [self.vel[0], self.vel[1], self.vel[2] - self.gravity * dt];
        let avg = vec::scale(vec::add(self.vel, vel), 0.5);
        let mut step = vec::scale(avg, dt);
        let mut len = vec::len(step);
        let left = self.range - self.travelled;
        let mut ends = false;
        if self.motion != Motion::Lobber && len >= left {
            step = vec::scale(step, left / len.max(1e-6));
            len = left;
            ends = true;
        }
        let to = vec::add(self.pos, step);
        self.vel = vel;
        let mut from = self.pos;
        while let Some(h) = self.trace(world, from, to) {
            match (self.motion, h.actor) {
                // A flame goes on through actors, hitting each once.
                (Motion::Flame, Some(a)) => {
                    self.hit_actors.push(a);
                    out.push(ProjEvent::Hit { actor: Some(a), part: h.part, object: None, point: h.point, normal: h.normal, material: h.material, delay: 0.0 });
                    from = h.point;
                    continue;
                }
                (Motion::Lobber, _) if self.alt_trigger || self.detonates => {
                    // Bounce off whatever it met; settle when slow.
                    let n = h.normal;
                    let v = self.vel;
                    let reflected = vec::sub(v, vec::scale(n, 2.0 * vec::dot(v, n)));
                    self.vel = vec::scale(reflected, self.bounce);
                    self.pos = vec::add(h.point, vec::scale(n, NUDGE));
                    self.travelled += h.distance;
                    if vec::len(self.vel) < REST_SPEED {
                        self.vel = [0.0; 3];
                        self.landed = true;
                        if self.mine {
                            self.alive = false;
                            out.push(ProjEvent::Settled { point: self.pos, normal: n });
                        }
                    }
                    return out;
                }
                _ => {
                    self.alive = false;
                    self.travelled += h.distance;
                    self.pos = h.point;
                    out.push(ProjEvent::Hit { actor: h.actor, part: h.part, object: h.object, point: h.point, normal: h.normal, material: h.material, delay: 0.0 });
                    if let Some(e) = self.impact_explosion() {
                        out.push(ProjEvent::Explode { explosion: e, point: h.point });
                    }
                    return out;
                }
            }
        }
        self.pos = to;
        self.travelled += len;
        if ends {
            self.alive = false;
            out.push(ProjEvent::Expired { point: to });
        }
        out
    }

    /// Remote detonation (C-4): explode now if this projectile detonates.
    pub fn detonate(&mut self) -> Option<ProjEvent> {
        if !self.alive || !self.detonates {
            return None;
        }
        self.alive = false;
        self.explosion.map(|e| ProjEvent::Explode { explosion: e, point: self.pos })
    }
}

/// The actors a melee swing touches: a fan of rays across the swing in front of the
/// attacker, each as long as the reach [design: rays and arc from CombatTuning]. Each
/// actor appears once, with the nearest hit.
pub fn swing(world: &dyn CombatWorld, t: &CombatTuning, eye: V3, forward: V3, up: V3, reach: f32, attacker: ActorId) -> Vec<(ActorId, WorldHit)> {
    let n = t.melee_rays.max(1);
    let mut out: Vec<(ActorId, WorldHit)> = Vec::new();
    let axis = vec::normalize(up);
    for k in 0..n {
        let f = if n == 1 { 0.0 } else { k as f32 / (n - 1) as f32 * 2.0 - 1.0 };
        let dir = vec::rotate(vec::normalize(forward), axis, (f * t.melee_arc_deg).to_radians());
        let Some(h) = world.ray(eye, vec::add(eye, vec::scale(dir, reach)), &[attacker]) else { continue };
        let Some(a) = h.actor else { continue };
        match out.iter_mut().find(|(x, _)| *x == a) {
            Some(e) if e.1.distance > h.distance => e.1 = h,
            Some(_) => {}
            None => out.push((a, h)),
        }
    }
    out
}

/// How far past the reach an actor's centre may be and still be struck by
/// [`swing_reaching`] (the half-width of a body), game units [design].
pub const SWING_BODY_MARGIN: f32 = 24.0;
/// The heights [`swing_reaching`] reaches: from this far below the attacker's feet to this
/// far above its eyes, game units [design].
pub const SWING_BELOW_FEET: f32 = 32.0;
pub const SWING_ABOVE_EYES: f32 = 48.0;

/// The actors a melee swing touches, low or high: [`swing`]'s fan of rays from the eye,
/// and besides it every actor whose centre is within the reach across the ground from the
/// attacker's body (plus [`SWING_BODY_MARGIN`]), inside the swing's arc either side of
/// where it faces, between its feet and above its eyes (`feet_z`, [`SWING_BELOW_FEET`],
/// [`SWING_ABOVE_EYES`]), and that a ray from the eye to that centre meets first
/// [design; playtest 2026-10-11: a machete never hit a gecko, whose body is below the
/// fan of rays at eye height unless the view points steeply down, and the reach
/// (fCombatDistance × Reach, 150 units) is used up by the drop from the eye]. The reach
/// is measured across the ground, so a short creature at the player's feet is as near
/// as a man in front of them.
pub fn swing_reaching(world: &dyn CombatWorld, t: &CombatTuning, eye: V3, feet_z: f32, forward: V3, reach: f32, attacker: ActorId) -> Vec<(ActorId, WorldHit)> {
    let mut out = swing(world, t, eye, forward, [0.0, 0.0, 1.0], reach, attacker);
    let flat = [forward[0], forward[1], 0.0];
    if vec::len(flat) < 1e-4 {
        return out;
    }
    let flat = vec::normalize(flat);
    let height = (eye[2] - feet_z).max(0.0);
    let middle = [eye[0], eye[1], feet_z + height / 2.0];
    let mut near = world.actors_near(middle, reach + SWING_BODY_MARGIN + height);
    near.sort_by(|a, b| vec::len(vec::sub(a.1, eye)).total_cmp(&vec::len(vec::sub(b.1, eye))));
    for (a, c) in near {
        if a == attacker || out.iter().any(|(x, _)| *x == a) {
            continue;
        }
        let across = [c[0] - eye[0], c[1] - eye[1], 0.0];
        let d = vec::len(across);
        if d > reach + SWING_BODY_MARGIN || c[2] < feet_z - SWING_BELOW_FEET || c[2] > eye[2] + SWING_ABOVE_EYES {
            continue;
        }
        if d > 1e-3 && vec::dot(vec::scale(across, 1.0 / d), flat).clamp(-1.0, 1.0).acos().to_degrees() > t.melee_arc_deg {
            continue;
        }
        // Its body, then its centre: the centre of a creature's bounds (a radscorpion's,
        // its tail raised) can lie in the air over its body, where the ray meets nothing
        // (the owner's playtest list 5 item 25) [design].
        let hit = world.aim_points(a).into_iter().chain([c]).find_map(|p| {
            let to = vec::sub(p, eye);
            let len = vec::len(to);
            if len < 1e-3 {
                return None;
            }
            let end = vec::add(eye, vec::scale(to, (len + SWING_BODY_MARGIN) / len));
            world.ray(eye, end, &[attacker]).filter(|h| h.actor == Some(a))
        });
        if let Some(h) = hit {
            out.push((a, h));
        }
    }
    out
}

/// One actor caught in a blast.
#[derive(Debug, Clone, Serialize)]
pub struct BlastHit {
    pub actor: ActorId,
    pub distance: f32,
    /// Damage before armour.
    pub damage: f32,
    /// Push direction (unit) and the EXPL force.
    pub push: V3,
    pub force: f32,
}

/// The actors an explosion at `center` reaches: within its radius, and in line of sight
/// of the centre unless the EXPL ignores it [documented: GECK wiki Explosion]; damage
/// falls off with distance [design].
pub fn blast(world: &dyn CombatWorld, t: &CombatTuning, e: &ExplosionStats, center: V3) -> Vec<BlastHit> {
    let los = !e.flags.iter().any(|f| f == "Ignore LOS Check");
    let mut out = Vec::new();
    for (a, at) in world.actors_near(center, e.radius) {
        let d = vec::len(vec::sub(at, center));
        if los {
            if let Some(h) = world.ray(center, at, &[]) {
                if h.actor != Some(a) && h.distance + NUDGE < d {
                    continue;
                }
            }
        }
        let damage = explosion_damage(t, e.damage, e.radius, d);
        if damage <= 0.0 {
            continue;
        }
        let push = if d > 1e-3 { vec::normalize(vec::sub(at, center)) } else { [0.0, 0.0, 1.0] };
        out.push(BlastHit { actor: a, distance: d, damage, push, force: e.force });
    }
    out
}
