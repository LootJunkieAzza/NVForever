//! Part P (docs/design/layer4-play.md §4): the player — input, character movement, the
//! first-person camera.
//!
//! Each frame: keys and mouse become a [`MoveIntent`] and the look angles (Input set);
//! the character is placed on the ground once its cell is loaded, then moved in fixed
//! steps through the `Stepper` (Physics set); the camera sits at the eye, interpolated
//! between the last two steps; the simulation's player reference follows the feet.
//! Nothing moves in `PlayState::Menu` (physics time stops). Movement values are
//! [hypothesis] (docs/spec-notes/physics.md §6).

use crate::phys::cells::Space;
use crate::phys::PhysState;
use crate::sim::SimState;
use crate::{space, PlaySet, PlayState, PlayWorld, PlayerCamera};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use fnv_phys::{CharacterInput, CharacterStep, Owner, Stroke};
use fnv_sim::FormId;
use fnv_world::cell_of;
use std::collections::BTreeSet;

/// Eye height above the feet, game units [hypothesis: placeholder for a standing human at
/// about 70 units per metre; not captured].
pub const EYE_HEIGHT: f32 = 112.0;

/// FalloutNV.esm's `fMoveBaseSpeed` and `fMoveRunMult` [observed: GMST records, physics.md
/// §6], used when the loaded plugins lack them.
pub const DEFAULT_MOVE_BASE_SPEED: f32 = 77.0;
pub const DEFAULT_MOVE_RUN_MULT: f32 = 4.0;

/// Mouse look, radians per mouse count [engineering choice, not a game value].
pub const LOOK_RADIANS_PER_COUNT: f32 = 0.0025;
/// Steepest look up or down [engineering choice].
pub const MAX_PITCH: f32 = 89.0 * std::f32::consts::PI / 180.0;

/// The player's pose in game space.
#[derive(Resource, Debug, Clone, Copy)]
pub struct Player {
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    /// Pitch, radians (negative looks down).
    pub pitch: f32,
    pub eye_height: f32,
    /// Feet position, game space (interpolated between fixed steps: where the camera
    /// shows the player; `PhysState::player` holds the stepped position).
    pub feet: [f32; 3],
}

impl Player {
    /// Eye position, game space.
    pub fn eye(&self) -> [f32; 3] {
        [self.feet[0], self.feet[1], self.feet[2] + self.eye_height]
    }

    /// Unit view direction, game space.
    pub fn look_dir(&self) -> [f32; 3] {
        space::look_dir(self.heading, self.pitch)
    }
}

/// Movement speeds, game units per second.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MoveSpeeds {
    pub walk: f32,
    pub run: f32,
}

impl MoveSpeeds {
    /// The player's speeds measured in game with no one at the controls: running (Always
    /// Run on, forward held) 344 units/s, walking (the Run key held as well) 86.8 units/s,
    /// at full speed from the first loop [observed: capture mv_trial, 2026-10-08, a level-1
    /// character in Doc Mitchell's save, Speed Mult 100; physics.md §6.3]. They are 11–13%
    /// above `fMoveBaseSpeed` (77) and `× fMoveRunMult` (308); the game's formula is not
    /// known [hypothesis: animation, armour, weapon and encumbrance may all enter it].
    pub const OBSERVED: MoveSpeeds = MoveSpeeds { walk: 86.8, run: 344.0 };

    /// Walking at `fMoveBaseSpeed`, running at `fMoveBaseSpeed × fMoveRunMult`
    /// [hypothesis: meanings guessed from the setting names; the game's formula (Speed
    /// Mult, encumbrance, sneaking, weapon drawn) is not known — physics.md §6]. Missing
    /// settings take FalloutNV.esm's values.
    pub fn from_settings(base: Option<f32>, run_mult: Option<f32>) -> MoveSpeeds {
        let base = base.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(DEFAULT_MOVE_BASE_SPEED);
        let mult = run_mult.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(DEFAULT_MOVE_RUN_MULT);
        MoveSpeeds { walk: base, run: base * mult }
    }
}

/// What the player asks for this frame.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveIntent {
    /// +1 forward (W), −1 back (S).
    pub forward: f32,
    /// +1 right (D), −1 left (A).
    pub right: f32,
    /// Walk instead of run (Shift held) [hypothesis: running is the game's default].
    pub walk: bool,
    /// Jump (Space) on the next fixed step; kept until a step uses it.
    pub jump: bool,
}

/// Speed backwards as a fraction of the forward speed [observed: 272.8 against 344 units/s
/// running, capture mv_values, physics.md §6.3]. Sideways and diagonal speeds were not
/// measured (the strafe runs hit a wall); they take the forward speed [hypothesis].
pub const BACK_SPEED_FRACTION: f32 = 272.8 / 344.0;

/// Horizontal velocity (game x, y) for an intent at a compass heading: diagonals not
/// faster; straight backwards at [`BACK_SPEED_FRACTION`] of the speed, scaling with the
/// backward share of a diagonal [hypothesis for diagonals].
pub fn velocity(intent: &MoveIntent, heading: f32, speeds: MoveSpeeds) -> [f32; 2] {
    let (s, c) = heading.sin_cos();
    // Forward is (sin h, cos h); right is forward turned 90° clockwise, (cos h, −sin h).
    let v = [s * intent.forward + c * intent.right, c * intent.forward - s * intent.right];
    let len = v[0].hypot(v[1]);
    if len < 1e-6 {
        return [0.0, 0.0];
    }
    let mut speed = if intent.walk { speeds.walk } else { speeds.run };
    if intent.forward < 0.0 {
        let back_share = -intent.forward / intent.forward.hypot(intent.right);
        speed *= 1.0 - back_share * (1.0 - BACK_SPEED_FRACTION);
    }
    [v[0] / len * speed, v[1] / len * speed]
}

/// Swimming speeds, units/s, before the stroke cycle (`fnv_phys::swim`): run (Always Run)
/// 339, walk (Shift) 86, the same forwards, backwards and sideways [observed: sw_trial at
/// Lake Mead; physics.md §6.6].
pub const SWIM_SPEEDS: MoveSpeeds = MoveSpeeds { walk: fnv_phys::swim::SWIM_WALK_SPEED, run: fnv_phys::swim::SWIM_RUN_SPEED };

/// Velocity (game x, y, z) while swimming: forward and back along the view, pitch
/// included (the game swims along the view [observed]), sideways level; backwards at the
/// full speed and diagonals not faster [observed for straight and forward-right runs;
/// other diagonals hypothesis].
pub fn swim_velocity(intent: &MoveIntent, heading: f32, pitch: f32, speeds: MoveSpeeds) -> [f32; 3] {
    let (s, c) = heading.sin_cos();
    let (ps, pc) = pitch.sin_cos();
    let f = intent.forward;
    let r = intent.right;
    let v = [s * pc * f + c * r, c * pc * f - s * r, ps * f];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 {
        return [0.0, 0.0, 0.0];
    }
    let speed = if intent.walk { speeds.walk } else { speeds.run };
    [v[0] / len * speed, v[1] / len * speed, v[2] / len * speed]
}

/// New look angles after a mouse motion (counts; +x right, +y down).
pub fn look(heading: f32, pitch: f32, delta: Vec2) -> (f32, f32) {
    let h = (heading + delta.x * LOOK_RADIANS_PER_COUNT).rem_euclid(std::f32::consts::TAU);
    let p = (pitch - delta.y * LOOK_RADIANS_PER_COUNT).clamp(-MAX_PITCH, MAX_PITCH);
    (h, p)
}

/// Where `a` is `t` of the way to `b`.
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Once-a-second status (debug log).
#[derive(Resource, Default)]
struct Status {
    elapsed: f32,
    steps: u32,
    grounded_steps: u32,
    swimming_steps: u32,
    moved: f32,
    commanded: f32,
    touched: BTreeSet<u32>,
    touched_terrain: bool,
    last: Option<CharacterStep>,
    waiting_frames: u32,
}

impl Status {
    fn record(&mut self, s: &CharacterStep, from: [f32; 3], vel: [f32; 2], dt: f32) {
        self.steps += 1;
        self.grounded_steps += s.grounded as u32;
        self.swimming_steps += s.swimming as u32;
        self.moved += (s.feet[0] - from[0]).hypot(s.feet[1] - from[1]);
        self.commanded += vel[0].hypot(vel[1]) * dt;
        for o in &s.touched {
            match o {
                Owner::Ref { form_id, .. } => {
                    self.touched.insert(*form_id);
                }
                Owner::Terrain { .. } => self.touched_terrain = true,
                Owner::Character { .. } => {}
            }
        }
        self.last = Some(s.clone());
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveIntent>()
            .init_resource::<Status>()
            .add_systems(Startup, init_player)
            .add_systems(Update, (grab_cursor, read_input, mouse_look).chain().in_set(PlaySet::Input))
            .add_systems(
                Update,
                (place_initial, step, crate::phys::settle_created, sync_sim, log_status, place_camera)
                    .chain()
                    .in_set(PlaySet::Physics)
                    .after(crate::phys::PhysSystems),
            );
    }
}

fn init_player(mut commands: Commands, d: Res<PlayWorld>, sim: Res<SimState>) {
    let [x, y, z] = d.eye;
    commands.insert_resource(Player {
        heading: d.heading_deg.to_radians(),
        pitch: d.pitch_deg.to_radians(),
        eye_height: EYE_HEIGHT,
        feet: [x, y, z - EYE_HEIGHT],
    });
    let settings = MoveSpeeds::from_settings(sim.forms.setting_f32("fMoveBaseSpeed"), sim.forms.setting_f32("fMoveRunMult"));
    let speeds = MoveSpeeds::OBSERVED;
    info!(
        "player speeds: walk {:.1}, run {:.0} units/s (measured in game; fMoveBaseSpeed and fMoveRunMult give {:.0} and {:.0})",
        speeds.walk, speeds.run, settings.walk, settings.run
    );
    commands.insert_resource(speeds);
}

/// The cursor is grabbed and hidden while playing in a focused window, released in menus.
/// Not with `--auto-walk` (a test aid: the window must not hold the desktop's mouse).
fn grab_cursor(
    state: Res<State<PlayState>>,
    d: Res<PlayWorld>,
    unattended: Option<Res<crate::test_aid::Unattended>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else { return };
    let driven = d.auto_walk > 0.0 || unattended.is_some_and(|u| u.0);
    let grab = *state.get() == PlayState::Playing && !driven && window.focused;
    // Windows cannot lock the cursor in place; confining it still gives raw motion.
    let mode = match (grab, cfg!(target_os = "windows")) {
        (false, _) => CursorGrabMode::None,
        (true, true) => CursorGrabMode::Confined,
        (true, false) => CursorGrabMode::Locked,
    };
    if cursor.grab_mode != mode || cursor.visible == grab {
        cursor.grab_mode = mode;
        cursor.visible = !grab;
    }
}

fn read_input(keys: Res<ButtonInput<KeyCode>>, state: Res<State<PlayState>>, mut intent: ResMut<MoveIntent>) {
    if *state.get() != PlayState::Playing {
        *intent = MoveIntent::default();
        return;
    }
    let axis = |plus: KeyCode, minus: KeyCode| keys.pressed(plus) as i32 as f32 - keys.pressed(minus) as i32 as f32;
    intent.forward = axis(KeyCode::KeyW, KeyCode::KeyS);
    intent.right = axis(KeyCode::KeyD, KeyCode::KeyA);
    intent.walk = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::Space) {
        intent.jump = true;
    }
}

/// Mouse look while the cursor is grabbed.
fn mouse_look(
    motion: Res<AccumulatedMouseMotion>,
    state: Res<State<PlayState>>,
    cursor: Query<&CursorOptions, With<PrimaryWindow>>,
    mut player: ResMut<Player>,
) {
    let grabbed = cursor.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None);
    if !grabbed || *state.get() != PlayState::Playing || motion.delta == Vec2::ZERO {
        return;
    }
    (player.heading, player.pitch) = look(player.heading, player.pitch, motion.delta);
}

/// Put the character on the ground under the start position once its cell is loaded: a
/// ray down from the start eye, else from far above (a start under the terrain).
fn place_initial(mut phys: ResMut<PhysState>, mut player: ResMut<Player>) {
    if phys.player.is_some() || !phys.ready_at(player.feet) {
        return;
    }
    let start = player.feet;
    let mut hit = phys.place_player(start, player.eye_height, 100_000.0);
    if hit.is_none() {
        hit = phys.place_player(start, 100_000.0, 100_000.0);
    }
    // A start above water deeper than the swimming depth: the character starts where it
    // was asked to and falls in (into the swimming state), as the game's player does after
    // a SetPos there [observed: physics.md §6.6], instead of standing on the bed.
    let swim_depth = phys.player.as_ref().map_or(0.0, |c| c.params.swim_depth);
    let deep = phys.world.water_surface_at(start[0], start[1]).filter(|&w| {
        let bed = hit.as_ref().map_or(f32::NEG_INFINITY, |h| h.point[2]);
        bed < w - swim_depth && start[2] > bed
    });
    if let Some(w) = deep {
        let p = &mut *phys;
        let c = p.player.as_mut().expect("placed");
        p.world.place_character(c, start);
        p.prev_feet = start;
        info!("player starts above water (surface z {w:.1}): falling in");
    }
    let feet = phys.player.as_ref().expect("placed").feet;
    player.feet = feet;
    info!(
        "player placed: feet ({:.0}, {:.0}, {:.1}), ground {}, terrain z {}",
        feet[0],
        feet[1],
        feet[2],
        hit.map_or("none below the start".to_string(), |h| format!("z {:.1} ({:?})", h.point[2], h.owner)),
        phys.terrain_z(feet[0], feet[1]).map_or("-".to_string(), |z| format!("{z:.1}")),
    );
}

/// Below the terrain by this much the character has fallen through [engineering choice]:
/// it is put back on the ground (logged as a warning).
const FELL_THROUGH: f32 = 1000.0;

/// Fixed steps of the character for the frame's time; the camera's feet between the last
/// two.
#[allow(clippy::too_many_arguments)]
fn step(
    mut stroke_time: Local<f32>,
    time: Res<Time>,
    state: Res<State<PlayState>>,
    d: Res<PlayWorld>,
    speeds: Res<MoveSpeeds>,
    mut intent: ResMut<MoveIntent>,
    mut phys: ResMut<PhysState>,
    mut player: ResMut<Player>,
    mut status: ResMut<Status>,
) {
    if *state.get() != PlayState::Playing {
        phys.stepper = default();
        return;
    }
    let Some(feet) = phys.player.as_ref().map(|c| c.feet) else { return };
    if !phys.ready_at(feet) {
        // Walked into a cell that is not loaded yet: wait for it.
        phys.stepper = default();
        status.waiting_frames += 1;
        return;
    }
    if let Some(t) = phys.terrain_z(feet[0], feet[1]).filter(|t| feet[2] < t - FELL_THROUGH) {
        warn!("player fell through at ({:.0}, {:.0}, {:.0}), terrain z {t:.0}: back on the ground", feet[0], feet[1], feet[2]);
        phys.place_player([feet[0], feet[1], t], 100_000.0, 100_000.0);
    }
    let p = &mut *phys;
    let dt = p.world.params.dt;
    let n = p.stepper.advance(&p.world.params, time.delta_secs());
    let c = p.player.as_mut().expect("placed");
    for i in 0..n {
        // Swimming or not is the character's state after the last step.
        let [vx, vy, vz] = if d.auto_walk > 0.0 {
            let (s, c) = player.heading.sin_cos();
            [s * d.auto_walk, c * d.auto_walk, 0.0]
        } else if c.swimming {
            // Running strokes follow the stroke cycle from when the swimmer started to
            // move [observed]; walking stays at its start speed [hypothesis].
            let v = swim_velocity(&intent, player.heading, player.pitch, SWIM_SPEEDS);
            match Stroke::of(intent.forward, intent.right) {
                Some(stroke) => {
                    let f = if intent.walk { 1.0 } else { stroke.factor(*stroke_time + dt / 2.0) };
                    *stroke_time += dt;
                    v.map(|x| x * f)
                }
                None => {
                    *stroke_time = 0.0;
                    v
                }
            }
        } else {
            *stroke_time = 0.0;
            let [x, y] = velocity(&intent, player.heading, *speeds);
            [x, y, 0.0]
        };
        p.prev_feet = c.feet;
        let s = p.world.move_character(c, CharacterInput { velocity: [vx, vy], jump: intent.jump && i == 0, vertical: vz });
        p.world.step();
        status.record(&s, p.prev_feet, [vx, vy], dt);
    }
    if n > 0 {
        intent.jump = false;
    }
    let alpha = (p.stepper.accumulated / dt).clamp(0.0, 1.0);
    player.feet = lerp3(p.prev_feet, c.feet, alpha);
}

/// Feet, cell and heading last written to the simulation.
type SimPose = ([f32; 3], Option<u32>, f32);

/// The simulation's player reference stands where the character does.
fn sync_sim(
    phys: Res<PhysState>,
    player: Res<Player>,
    d: Res<PlayWorld>,
    mut sim: ResMut<SimState>,
    mut last: Local<Option<SimPose>>,
) {
    let Some(c) = &phys.player else { return };
    let cell = match phys.space {
        Space::Interior(id) => Some(id),
        Space::Exterior => {
            let g = cell_of(c.feet[0], c.feet[1]);
            d.index.worldspaces.get(&d.world).and_then(|w| w.grid.get(&g)).copied()
        }
    };
    let now = (c.feet, cell, player.heading);
    if *last == Some(now) {
        return;
    }
    *last = Some(now);
    match sim.world.get(FormId::PLAYER_REF) {
        Ok(s) => {
            s.pos = c.feet;
            s.cell = cell.map(FormId);
            // The reference's z rotation is its compass heading in radians [hypothesis:
            // as door teleport markers' rotations are read for Travel].
            s.rot[2] = player.heading;
        }
        Err(e) => warn_once!("simulation player reference: {e}"),
    }
}

fn log_status(time: Res<Time>, phys: Res<PhysState>, mut status: ResMut<Status>) {
    status.elapsed += time.delta_secs();
    if status.elapsed < 1.0 {
        return;
    }
    let s = std::mem::take(&mut *status);
    let Some(c) = &phys.player else { return };
    let f = c.feet;
    let terrain = phys.terrain_z(f[0], f[1]);
    let water = phys.world.water_surface_at(f[0], f[1]);
    let touched: Vec<String> = s.touched.iter().map(|id| format!("{id:08X}")).collect();
    debug!(
        "player: feet ({:.0}, {:.0}, {:.1}) terrain {} water {} swimming {}/{} grounded {}/{} sliding {} moved {:.0} of {:.0} units/s commanded, touched{} [{}]; physics: {} cells, {} refs with collision, {} steps{}",
        f[0],
        f[1],
        f[2],
        terrain.map_or("-".to_string(), |t| format!("z {t:.1} (feet {:+.1})", f[2] - t)),
        water.map_or("-".to_string(), |w| format!("z {w:.1} (feet {:+.1})", f[2] - w)),
        s.swimming_steps,
        s.steps,
        s.grounded_steps,
        s.steps,
        s.last.as_ref().is_some_and(|l| l.sliding),
        s.moved / s.elapsed,
        s.commanded / s.elapsed,
        if s.touched_terrain { " terrain" } else { "" },
        touched.join(" "),
        phys.cells.len(),
        phys.world.stats().refs,
        phys.world.steps(),
        if s.waiting_frames > 0 { format!(", waited {} frames for its cell", s.waiting_frames) } else { String::new() },
    );
}

/// The camera at the player's eye, looking along the view.
fn place_camera(player: Option<Res<Player>>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let (Some(p), Ok(mut t)) = (player, cams.single_mut()) else { return };
    t.translation = space::to_bevy(p.eye());
    t.rotation = space::look_rotation(p.heading, p.pitch);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    const SPEEDS: MoveSpeeds = MoveSpeeds { walk: 77.0, run: 308.0 };

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn forward_follows_the_heading() {
        let fwd = MoveIntent { forward: 1.0, ..default() };
        assert!(close(velocity(&fwd, 0.0, SPEEDS), [0.0, 308.0])); // north
        assert!(close(velocity(&fwd, FRAC_PI_2, SPEEDS), [308.0, 0.0])); // east
        assert!(close(velocity(&fwd, PI, SPEEDS), [0.0, -308.0])); // south
        let back = MoveIntent { forward: -1.0, walk: true, ..default() };
        assert!(close(velocity(&back, 0.0, SPEEDS), [0.0, -77.0 * BACK_SPEED_FRACTION]));
    }

    #[test]
    fn strafing_is_to_the_right_of_the_view() {
        let right = MoveIntent { right: 1.0, ..default() };
        // Facing north, right is east; facing east, right is south.
        assert!(close(velocity(&right, 0.0, SPEEDS), [308.0, 0.0]));
        assert!(close(velocity(&right, FRAC_PI_2, SPEEDS), [0.0, -308.0]));
        let left = MoveIntent { right: -1.0, ..default() };
        assert!(close(velocity(&left, 0.0, SPEEDS), [-308.0, 0.0]));
    }

    #[test]
    fn diagonals_are_not_faster_and_nothing_is_still() {
        let d = MoveIntent { forward: 1.0, right: 1.0, ..default() };
        let v = velocity(&d, 0.3, SPEEDS);
        assert!((v[0].hypot(v[1]) - 308.0).abs() < 1e-3);
        assert_eq!(velocity(&MoveIntent::default(), 1.0, SPEEDS), [0.0, 0.0]);
    }

    #[test]
    fn speeds_from_settings() {
        assert_eq!(MoveSpeeds::from_settings(Some(77.0), Some(4.0)), SPEEDS);
        assert_eq!(MoveSpeeds::from_settings(None, None), SPEEDS);
        assert_eq!(MoveSpeeds::from_settings(Some(100.0), Some(-1.0)), MoveSpeeds { walk: 100.0, run: 400.0 });
    }

    #[test]
    fn looking_turns_right_and_clamps_pitch() {
        let (h, p) = look(0.0, 0.0, Vec2::new(100.0, 0.0));
        assert!(h > 0.0 && p == 0.0, "mouse right turns clockwise (towards east)");
        let (h, _) = look(0.0, 0.0, Vec2::new(-1.0, 0.0));
        assert!(h > 6.0 && h < std::f32::consts::TAU, "headings wrap into [0, 2π)");
        let (_, p) = look(0.0, 0.0, Vec2::new(0.0, 1.0e6));
        assert_eq!(p, -MAX_PITCH, "mouse down looks down, at most straight down");
    }

    #[test]
    fn swimming_follows_the_view_at_full_speed_backwards() {
        let fwd = MoveIntent { forward: 1.0, ..default() };
        let down30 = -30f32.to_radians();
        let v = swim_velocity(&fwd, FRAC_PI_2, down30, SWIM_SPEEDS);
        assert!((v[0] - 339.0 * 30f32.to_radians().cos()).abs() < 1e-2 && v[1].abs() < 1e-3, "{v:?}");
        assert!((v[2] + 169.5).abs() < 1e-2, "{v:?}");
        let back = MoveIntent { forward: -1.0, ..default() };
        let v = swim_velocity(&back, 0.0, 0.0, SWIM_SPEEDS);
        assert!((v[1] + 339.0).abs() < 1e-3, "{v:?}");
        let right = MoveIntent { right: 1.0, walk: true, ..default() };
        let v = swim_velocity(&right, 0.0, down30, SWIM_SPEEDS);
        assert!((v[0] - 86.0).abs() < 1e-3 && v[2] == 0.0, "{v:?}");
        assert_eq!(swim_velocity(&MoveIntent::default(), 1.0, 0.5, SWIM_SPEEDS), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn interpolation() {
        assert_eq!(lerp3([0.0; 3], [2.0, 4.0, 6.0], 0.5), [1.0, 2.0, 3.0]);
    }
}
