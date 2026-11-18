//! Sway and bob of the first-person arms and weapon (the owner's request, 2026-10-20: the
//! arms and weapon "stay rigid throughout movement"; reworked for the movement-inertia
//! request of 2026-11-13: "the gun … moves with the direction using a ramp curve - slow
//! start, ramps up to a faster movement then smoothly slows").
//!
//! The game's first-person movement clips do not move the arms: the `mt` clips of
//! `characters\_1stperson\locomotion` drive `Bip01` (the root motion, 102.3 units over
//! `mtforward.kf`'s 1.2 s) and leave `Bip01 NonAccum` still, the `1hp` ones add
//! `Camera1st`, `Bip01 Translate` and `Bip01 Rotate` tracks without keys that move them,
//! and the folder has no `2hr`, `2ha` or `2hh` movement clips at all [observed:
//! `fnvtool nif decode` of `mtforward.kf` and `1hpforward.kf`, the folder's file list].
//! The camera node and the arms both hang from `Bip01 Looking`, so whatever moved that
//! chain would move the view with the arms. The executable holds the settings
//! `fFirstPersonHandFollowMult`, `fFirstPersonHandChaseSeconds` and
//! `fFirstPersonHandChaseSecondsAttack` [observed: their names in `FalloutNV.exe`'s
//! strings; not in `FalloutNV.esm` or `Fallout_default.ini`], which reads as the hands
//! chasing the view after a while [hypothesis: their meaning from the names; their
//! values are unknown]. So the sway is the engine's own, and this one is NVForever's
//! [design; every constant below an engineering choice, tuned by eye].
//!
//! The weapon hangs from the view on critically damped springs (`nvf_play::ease::Spring`,
//! solved exactly per frame, so the same at any frame rate): it starts to follow a change
//! slowly, speeds up and settles without overshoot, and its own motion never jumps.
//!
//! - **Turn lag**: the head turns first and the arms follow. The lag is driven by how fast
//!   the view turns, not by how far it has turned (as Source's `CalcViewModelLag` and the
//!   spring setups of Overwatch-style first-person animation do; research of 2026-11-13):
//!   a turn asks for a lag of rate × [`CHASE_SECONDS`] that bends smoothly towards
//!   [`MAX_LAG_YAW`] / [`MAX_LAG_PITCH`] (a tanh, never a hard stop), and the arms follow
//!   that on one slow critically damped spring ([`FOLLOW_SECONDS`]), the same out and
//!   back, which also smooths the mouse's frame-to-frame unevenness. So a fast turn builds the lag up gently instead of throwing the
//!   arms to the limit, the lag never exceeds the limit, and the moment the view stops
//!   the arms start back, without waiting, and come to rest on the view without swinging
//!   past it. A sideways lag tilts the weapon a little. The view itself turns exactly as
//!   the mouse says. [design: the owner's playtests of 2026-11-13: the gun "reaches max
//!   sway quickly and then slowly returns", "there is a visible delay … the gun should
//!   never stop and wait", "limit the range … but slow it down deliberately so there is
//!   visible weight".]
//! - **Movement lag**: the weapon is held as by a sprung hand, pushed by the body's
//!   acceleration: starting, stopping, strafing and turning while running leave it a
//!   little behind (slowly: [`MOVE_OMEGA`]), and it comes back while the pace holds;
//!   speeding up ahead also drops it a little and tips the muzzle down, slowing down tips
//!   it up. It leans into a strafe on its own spring.
//! - **Aiming and blocking**: raising the gun to the sights or to block, and lowering it,
//!   it trails its way a little and settles into place (a spring pushed by the raise's
//!   own acceleration; the true iron sights' share where they run, else the aim or block
//!   clip asked for, smoothed). The clips themselves are not changed.
//! - **Jumps and landings**: the same hand, pushed by the body's vertical acceleration: a
//!   take-off dips the weapon, it rides a little high through the fall, and a landing
//!   dips it again, more the harder the landing; the muzzle tips with the dip.
//! - **Walking bob**: two dips a stride, one at each footfall, and one sideways swing
//!   a stride, at the phase of the movement clips the arms rig plays (their footfalls,
//!   `m:L` and `m:R`, fall about half a cycle apart: 0.40 s and 1.03 s of
//!   `mtforward.kf`'s 1.2 s [observed: audio.md]). It grows with the ground speed up to
//!   a run's, less sneaking, little down the sights, none in the air.
//! - **Breathing**: standing still the weapon drifts in a slow, small figure of eight.
//! - Down the sights and sneaking the sway is [`SIGHTS_SWAY`] of itself.
//! - **Weapon weight** ([`heft`], its own switch, off by default): a heavier weapon's
//!   springs are slower and its sway larger, up to half as much again.
//!
//! The pose turns and moves `Bip01 Translate` (the node above the arms, under `Bip01
//! Looking`) about the eye, in the view's own frame, so the camera node (`Camera1st`,
//! beside it under `Bip01 Looking`) and the view stay where they were.

use bevy::math::{Quat, Vec2, Vec3};
use nvf_play::ease::{carried_push, omega_for, smoothstep, Pushed, Spring};

/// The lag a slow, steady turn asks for: turn rate × this (seconds); × the weapon's weight.
pub const CHASE_SECONDS: f32 = 0.15;
/// The most lag a turn asks for: sideways and up or down, radians; × the weapon's weight
/// (12° and 6° in the second build, 5° and 3.5° in the first).
pub const MAX_LAG_YAW: f32 = 6.0 * std::f32::consts::PI / 180.0;
pub const MAX_LAG_PITCH: f32 = 4.0 * std::f32::consts::PI / 180.0;
/// The arms reach 95% of the lag asked for in this long, out and back alike: one
/// critically damped spring with one rate, so its motion never changes character
/// mid-way [design: the owner's fifth playtest, 2026-11-14: "it looks like there are
/// transitions between different spring values … quick ramps in speed and a delay";
/// before, a smoothing of the rate with separate build and release times fed a spring
/// with separate out and back times]. × the weapon's weight.
/// One spring since the ninth playtest (two of 0.3 s in a row before).
pub const FOLLOW_SECONDS: f32 = 0.4;
/// The turn lag's own weight on top of the weapon's [design: the owner's ninth
/// playtest, 2026-11-14: "increase the default weight just for mouse movement reactions to
/// 1.2x the current values"].
pub const TURN_WEIGHT: f32 = 1.2;
/// The turn's rate is averaged over about this long (95 %) before the limit bends it: long
/// enough to even out a mouse's uneven frames, short enough that nothing of a fast turn is
/// held back once the view stops [design: the ninth playtest, "there is still a delay
/// between the mouse stop and the hands returning": the first of two 0.3 s springs held
/// up to 14 times the limit, which the limit hid while it drained, about 0.2 s].
const TURN_AVERAGE_SECONDS: f32 = 0.06;
/// A turn slower than this (radians/s, about 6°/s, after the averaging) asks for less lag
/// than its rate would, and a still hand's twitches on the mouse almost none: the lag is
/// × `r² / (r² + KNEE²)` of the averaged rate `r` [design: the owner's tenth playtest,
/// 2026-11-14: "the idle weapon/hand animation micro jitters a lot, when I aim down sights it
/// jitters more visibly"; a resting hand's mouse counts set the arms swimming against the view
/// (about 0.9 px at 2160 lines, a simulation of 25 counts a second), 0.02 px with the knee].
const TURN_KNEE: f32 = 0.1;
/// Turn rates beyond this (radians/s, about 690°/s) count as this.
const MAX_TURN_RATE: f32 = 12.0;
/// A change of the view's angles bigger than this in one frame (radians) is a jump (a
/// teleport, an idle letting go), not a turn.
const VIEW_JUMP: f32 = 1.0;
/// Weapon tilt per radian of sideways lag.
const LAG_ROLL: f32 = 0.6;
/// The ground speed (units per second) at which the bob is full: about a run
/// [hypothesis: the player's walk is 85 units/s, `mtforward.kf`'s root motion].
const FULL_BOB_SPEED: f32 = 300.0;
/// The full bob: the dip, the sideways swing (units, at the eye's distance) and the
/// tilt with the swing (radians).
const BOB_DIP: f32 = 0.9;
const BOB_SWAY: f32 = 0.6;
const BOB_ROLL: f32 = 1.2 * std::f32::consts::PI / 180.0;
/// The bob comes and goes with the speed on a spring settling in this long (seconds, 95%;
/// × the weapon's weight; a plain 0.2 s lag until the seventh playtest: "when walking and
/// stopping make the gun return to the default position smoother").
const BOB_SETTLE_SECONDS: f32 = 0.5;
/// The first footfall's place in the movement cycle (0.40 s of 1.2 s).
pub const FOOTFALL_PHASE: f32 = 0.33;
/// Movement lag: the hand's spring rate (1/s; 10 until the owner's fourth playtest, "when
/// moving side to side … the gun slides to the sides too quick"), the most offset it
/// shows for a sudden change of velocity, units per unit/s, and the most offset at all.
const MOVE_OMEGA: f32 = 3.8;
/// The body's velocities are smoothed over this long (seconds, 95%) before their changes
/// push the hand (`nvf_play::ease::Pushed`), so a sudden change never jolts it [design: the
/// sixth playtest's trace: a take-off threw the gun 3.5 units within two frames].
const PUSH_SMOOTH_SECONDS: f32 = 0.12;
const MOVE_PEAK_PER_SPEED: f32 = 0.005;
const MAX_MOVE_LAG: f32 = 1.2;
/// Going forward or back: the hand's lag along the view also drops it this much (units
/// down per unit behind) and tips the muzzle (radians per unit), so speeding up and
/// slowing down show from the eye [design: the owner, "add a bit of inertia to front and
/// back movement"].
const AHEAD_DROP: f32 = 0.5;
const AHEAD_TIP: f32 = 1.2 * std::f32::consts::PI / 180.0;
/// Jumps and landings: the spring rate (1/s; 12 before the fourth playtest, now softer),
/// the most dip for a sudden change of vertical velocity (units per unit/s: about 1.2
/// units for a standing jump's 291 units/s), the most offset at all, and the muzzle's tip
/// per unit of dip (radians).
const LIFT_OMEGA: f32 = 4.0;
/// (0.004 and 2 until the fifth playtest, "jumping is not noticeable".)
const LIFT_PEAK_PER_SPEED: f32 = 0.008;
/// (3.5 until the eighth playtest's "fall-scaled landing": hard landings were held to
/// about a hop's.)
const MAX_LIFT: f32 = 6.0;
/// While the jump's or the landing's dip of the gun is deeper than the first of these
/// (units) the stride's bob fades, gone from the second, and comes back as the gun rises
/// again: the landing is the footfall (`head_bob.rs`, the same for the view).
const LIFT_HUSH_FROM: f32 = 0.2;
const LIFT_HUSH_FULL: f32 = 0.8;
const LIFT_TIP: f32 = 1.0 * std::f32::consts::PI / 180.0;
/// The gun with the view's pitch: looking straight down it comes up by `PITCH_DOWN_RAISE`
/// and back towards the eye by `PITCH_DOWN_BACK` (units), its muzzle a little up; looking
/// straight up it drops by `PITCH_UP_DROP` and goes out by `PITCH_UP_OUT`, the muzzle a
/// little down; with the sine of the pitch between; none down the sights [design: the
/// owner's eighth playtest, 2026-11-14: "pitch offset for the gun"; held arms are not
/// bolted to the eyes].
const PITCH_DOWN_RAISE: f32 = 1.4;
const PITCH_DOWN_BACK: f32 = 1.8;
const PITCH_UP_DROP: f32 = 1.0;
const PITCH_UP_OUT: f32 = 0.6;
const PITCH_DOWN_TIP: f32 = 2.5 * std::f32::consts::PI / 180.0;
const PITCH_UP_TIP: f32 = 1.5 * std::f32::consts::PI / 180.0;
/// Walking ahead the muzzle tips down by this (radians) and the gun drops by this (units),
/// full from [`AHEAD_FULL_SPEED`] (units/s, a brisk walk) and none down the sights, on
/// springs settling in [`AHEAD_SECONDS`] [design: the owner's ninth playtest, 2026-11-14:
/// "make the gun point to the ground more when walking forward, just a bit"].
const AHEAD_WALK_TIP: f32 = 3.0 * std::f32::consts::PI / 180.0;
const AHEAD_WALK_DROP: f32 = 0.5;
const AHEAD_FULL_SPEED: f32 = 150.0;
/// (Each of the two springs takes 0.6 of it, so the pair arrives in about that time.)
const AHEAD_SECONDS: f32 = 0.6;
/// Off the ground the muzzle tips down by this (radians), turning about the grip
/// ([`GRIP_PIVOT`]) so the gun's back end stays where it is; full while in the air (or
/// rising faster than [`JUMP_TIP_RISE`] units/s, from the take-off), on two springs in a
/// row each settling in [`JUMP_TIP_SECONDS`] (× the weight), none down the sights [design:
/// the owner's tenth playtest, 2026-11-14: "when jumping the muzzle of the gun should point
/// more towards the ground, but only the muzzle if possible"].
const JUMP_TIP: f32 = 6.0 * std::f32::consts::PI / 180.0;
const JUMP_TIP_RISE: f32 = 100.0;
const JUMP_TIP_SECONDS: f32 = 0.15;
/// Where the right hand holds a gun in the view's frame (x right, y ahead, z up; units from
/// the eye) [observed: the motion trace's `Bip01 R Hand` against `Camera1st`, varmint rifle
/// held, about (10, 15.7, -14.6)].
const GRIP_PIVOT: Vec3 = Vec3::new(10.0, 15.7, -14.6);
/// A reload brings the gun down and turns it over to work on it: the most drop (units),
/// muzzle tip down and roll (radians), and the move in towards the middle (units), on two
/// springs in a row each settling in [`RELOAD_SECONDS`] (× the weight); the reload clip
/// plays on top [design: the owner's ninth playtest, 2026-11-14: "reloads - do not see any
/// difference"; the eighth build only smoothed the clip's start and end].
const RELOAD_DROP: f32 = 1.6;
const RELOAD_TIP: f32 = 5.0 * std::f32::consts::PI / 180.0;
const RELOAD_ROLL: f32 = 9.0 * std::f32::consts::PI / 180.0;
const RELOAD_IN: f32 = 0.8;
const RELOAD_SECONDS: f32 = 0.3;
/// Tilt into a strafe at a run's sideways speed, radians, reached through a spring
/// settling in this long (seconds; × the weapon's weight).
const STRAFE_ROLL: f32 = 1.5 * std::f32::consts::PI / 180.0;
const STRAFE_ROLL_SECONDS: f32 = 0.6;
/// Raising the gun to aim or block and lowering it again: how far it travels (units, about
/// a hand's height), the raise smoothed by a spring settling in this long (seconds; the
/// clips' own change is a cross-fade of about 0.2 s), the lag spring's rate (1/s), the most lag for a sudden change of the
/// gun's speed (units per unit/s) and the most shown. The gun trails a little on the
/// way and settles into place [design: the owner, "add inertia to aiming and melee
/// blocking and return to their normal position"; the clips are not changed].
const AIM_TRAVEL: f32 = 8.0;
/// How much of the aim's raise pushes the lag: none since the sixth playtest (the aim's
/// eased cross-over with the true iron sights carries its weight; the push added a second
/// movement).
const AIM_SHARE: f32 = 0.0;
const AIM_RAMP_SECONDS: f32 = 0.25;
const AIM_OMEGA: f32 = 8.0;
const AIM_PEAK_PER_SPEED: f32 = 0.04;
const MAX_AIM_LAG: f32 = 1.5;
/// Drawing and putting away a weapon moves it this far (units) through the same lag, so a
/// switch trails and settles too [design: the owner, 2026-11-14, "implement weapon
/// switching"; the clips are not changed].
const DRAW_TRAVEL: f32 = 6.0;
/// Recoil on the gun alone (the view's own kick is the game's, `nvf_play::combat`): each
/// shot of strength 1 sets springs moving that peak at these (units back, radians up,
/// radians aside at most, at random), then settle; their rate (1/s, × the inertia
/// weight); the most shown; the share down the sights [design: the owner, 2026-11-14,
/// "can we improve recoil using our inertia formulas"].
const RECOIL_BACK: f32 = 1.2;
const RECOIL_UP: f32 = 1.6 * std::f32::consts::PI / 180.0;
const RECOIL_SIDE: f32 = 0.5 * std::f32::consts::PI / 180.0;
/// (16 until the ninth playtest: "make the end of the recoil smoother - slower ending".)
const RECOIL_OMEGA: f32 = 11.0;
const MAX_RECOIL_BACK: f32 = 3.0;
const MAX_RECOIL_UP: f32 = 5.0 * std::f32::consts::PI / 180.0;
const SIGHTS_RECOIL: f32 = 0.6;
/// A blow to the player jolts the gun: its offset's springs are set moving this fast
/// (units/s) per unit of punch (`SwayInput::punch`).
const PUNCH_SPEED: f32 = 30.0;
/// Out of breath (`SwayInput::exertion`, 0–1): the breathing gets up to this many times
/// faster and this many times larger [design: the owner, 2026-11-14, "out of breath"].
pub const EXERTED_PACE: f32 = 2.2;
pub const EXERTED_SIZE: f32 = 2.2;
/// Breathing: one figure of eight in this many seconds, its width (units; half as high),
/// below this ground speed (units/s), fading in and out over this long.
const BREATH_SECONDS: f32 = 4.0;
const BREATH_WIDTH: f32 = 0.18;
const BREATH_BELOW_SPEED: f32 = 20.0;
const BREATH_FADE_SECONDS: f32 = 0.8;
/// Shares down the sights and sneaking.
pub const SIGHTS_SWAY: f32 = 0.35;
const SIGHTS_BOB: f32 = 0.2;
const SNEAK_BOB: f32 = 0.6;
const SNEAK_BREATH: f32 = 0.35;
/// Seconds to fade the whole effect in or out (Pip-Boy, idles, the switch).
const FADE_SECONDS: f32 = 0.15;
/// Seconds the shares down the sights and sneaking take to come and go (times the weight):
/// switched at once they stepped the hand the frame the stance changed [observed: the
/// motion trace of 2026-11-14, 0.37 units at an aim's end].
pub const STANCE_SHARE_SECONDS: f32 = 0.5;
/// Weapon weight: no more sway up to the first weight (pounds, the WEAP record's), half
/// as much again from the second: pistols as before, rifles a little heavier, the
/// miniguns and launchers clearly so [design: the owner, 2026-11-13].
pub const LIGHT_WEAPON: f32 = 3.0;
pub const HEAVY_WEAPON: f32 = 20.0;
pub const MOST_HEFT: f32 = 1.5;
/// `e`, for a spring's peak: a sudden change `Δ` of what drives it peaks at `Δ·k/(ω e)`.
const E: f32 = std::f32::consts::E;

/// The pose `p` made `share` as large across the view (x and z, and its turn), keeping
/// its depth: the arms are drawn `k` times as big across the view for the viewmodel field
/// of view (`player_body::place_lens`), which would make every sway, recoil and dip `k`
/// times as big on the screen too. The sway is tuned at the original's 55°; `share` (the
/// tuned field of view's `k` over the chosen one's) keeps it the size it was tuned to at
/// any viewmodel field of view [design: the viewmodel field of view slider, 2026-11-14].
///
/// The turn is made `share` as large and the grip's way across the view too
/// ([`GRIP_PIVOT`]: the offset is what puts the grip there), so a turn about the grip (the
/// jump's muzzle tip) stays about the grip: made smaller by its scaled offset alone, the
/// grip slid in depth by the turn's depth part (review, 2026-11-15).
pub fn rescale(p: SwayPose, share: f32) -> SwayPose {
    if (share - 1.0).abs() < 1e-6 {
        return p;
    }
    let across = |v: Vec3| Vec3::new(v.x * share, v.y, v.z * share);
    let rotation = Quat::from_scaled_axis(p.rotation.to_scaled_axis() * share);
    let grip = p.rotation * GRIP_PIVOT + p.offset - GRIP_PIVOT;
    SwayPose { rotation, offset: GRIP_PIVOT + across(grip) - rotation * GRIP_PIVOT }
}

/// How much slower and larger a weapon of `weight` pounds sways: 1 up to
/// [`LIGHT_WEAPON`], [`MOST_HEFT`] from [`HEAVY_WEAPON`], smoothly between.
pub fn heft(weight: f32) -> f32 {
    1.0 + (MOST_HEFT - 1.0) * smoothstep(LIGHT_WEAPON, HEAVY_WEAPON, weight)
}

/// What the sway reads each frame.
#[derive(Debug, Clone, Copy)]
pub struct SwayInput {
    pub dt: f32,
    /// The view's compass heading (clockwise) and pitch (up), radians.
    pub heading: f32,
    pub pitch: f32,
    /// Ground velocity in the view's frame: x to the right, y ahead (units/s).
    pub velocity: Vec2,
    /// The body's vertical velocity off the ground (units/s, up); 0 on it and swimming.
    pub vertical: f32,
    /// Where the movement clips are in their cycle (0–1).
    pub phase: f32,
    pub sights: bool,
    pub sneaking: bool,
    pub airborne: bool,
    /// The weapon's weight as sway ([`heft`]; 1 for none).
    pub heft: f32,
    /// The gun breathes with the player (the "Weapon breathing" switch).
    pub breathing: bool,
    /// A hard landing pushes harder than its speed alone (the "Landing weight" switch,
    /// `nvf_play::inertia::hard_landing`).
    pub hard_landings: bool,
    /// How far the gun is raised to aim or block (0–1): the true iron sights' eased share
    /// while they run, else 1 while aiming or blocking and 0 otherwise.
    pub raised: f32,
    /// 1 while a weapon is out in the hand, 0 while it is put away or being switched.
    pub drawn: f32,
    /// A shot this frame: its recoil strength (0: none; about 1 for a rifle).
    pub kick: f32,
    /// A blow taken this frame: its punch (view frame: x right, y ahead, z up; 0: none).
    pub punch: Vec3,
    /// How out of breath the player is (0–1; nvforever `head_bob::HeadBob::exertion`).
    pub exertion: f32,
    /// A reload (or a jam) is playing (0 or 1).
    pub reloading: f32,
    /// Whether the arms may sway at all this frame.
    pub on: bool,
}

impl Default for SwayInput {
    fn default() -> Self {
        SwayInput { dt: 0.0, heading: 0.0, pitch: 0.0, velocity: Vec2::ZERO, vertical: 0.0, phase: 0.0, sights: false, sneaking: false, airborne: false, heft: 1.0, breathing: true, hard_landings: true, raised: 0.0, drawn: 0.0, kick: 0.0, punch: Vec3::ZERO, exertion: 0.0, reloading: 0.0, on: false }
    }
}

/// The pose to put on the arms: a turn and a move in the view's frame (x right, y ahead,
/// z up) about the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwayPose {
    pub rotation: Quat,
    pub offset: Vec3,
}

impl SwayPose {
    pub const NONE: SwayPose = SwayPose { rotation: Quat::IDENTITY, offset: Vec3::ZERO };
}

/// The sway's state across frames.
#[derive(Debug, Clone, Default)]
pub struct ViewSway {
    last: Option<(f32, f32)>,
    /// The arms' way relative to the view's (arms − view, radians) on their two springs.
    turn_pre: [Spring; 2],
    turn: [Spring; 2],
    /// How far the arms are behind the view.
    lag: Vec2,
    /// The hand's offset from the body's acceleration (view frame, x right, y ahead) and
    /// from its vertical acceleration (up), and the velocities they were last pushed from.
    hand: [Pushed; 2],
    lift: Pushed,
   last_vertical: f32,
    /// The strafe's tilt on its spring (radians).
    lean: Spring,
    /// Walking ahead's tip of the muzzle, and a reload's lean, each on two springs in a row
    /// (0–1 of the full).
    ahead: [Spring; 2],
    reload: [Spring; 2],
    /// Off the ground's muzzle tip (0–1 of the full), on two springs in a row.
    jump_tip: [Spring; 2],
    /// How far the gun is raised (smoothed), its speed last frame (units/s), and its lag.
    raise: Spring,
    draw: Spring,
    raise_speed: f32,
    aim_lag: Spring,
    /// Recoil: back (units), up and aside (radians); shots fired, for the aside's sign.
    recoil: [Spring; 3],
    shots: u32,
    /// The most recoil back shown since the start (for the exit summary).
    pub peak_recoil: f32,
    /// How much of the bob shows (follows the speed on a spring).
    bob: Spring,
    /// How much of the bob's lowering (its middle) shows: follows the speed in the air too.
    bob_carry: Spring,
    /// Breathing: where in the figure of eight (0–1) and how much of it shows.
    breath_phase: f32,
    breath: f32,
    /// How far down the sights and how far sneaking the shares are (0-1, on springs).
    sighted: Spring,
    crouched: Spring,
    /// How much of everything shows.
    gain: f32,
    /// The most turn lag (sideways, up or down), bob dip and landing dip shown since the
    /// start (for the exit summary).
    pub peak_lag: Vec2,
    pub peak_dip: f32,
    pub peak_lift_dip: f32,
}

/// `x` towards `max` and never past it, by a curve that keeps rising (`x / (1 + |x| / max)`:
/// half of `max` at `max`, nine tenths at nine times): unlike [`soft_clamp`] it has no flat
/// top for a large `x` to sit on.
fn gentle_limit(x: f32, max: f32) -> f32 {
    x / (1.0 + x.abs() / max)
}

/// `x` towards `max` and never past it, smoothly.
fn soft_clamp(x: f32, max: f32) -> f32 {
    max * (x / max).tanh()
}

/// `a - b` as the shorter way round, radians.
fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a - b + PI).rem_euclid(TAU) - PI
}

/// A spring rate `omega` with the gain that makes a sudden change `Δ` of the velocity
/// pushing it peak at `peak_per_speed × Δ`: its target for an acceleration `accel`.
fn pushed_target(accel: f32, omega: f32, peak_per_speed: f32) -> f32 {
    -peak_per_speed * omega * E * accel / (omega * omega)
}

impl ViewSway {
    pub fn update(&mut self, i: SwayInput) -> SwayPose {
        let dt = i.dt.max(0.0);
        let follow = |t: f32| if dt <= 0.0 { 0.0 } else { 1.0 - (-dt / t).exp() };
        let heft = i.heft.max(1.0);
        // The springs' times and the sizes grow with the weapon's weight.
        let slow = heft;
        self.gain += (f32::from(u8::from(i.on)) - self.gain) * follow(FADE_SECONDS);

        // Turn lag: from how fast the view turns this frame. A jump of the view (a
        // teleport, an idle letting go) is not a turn: the arms go with it.
        let mut rate = Vec2::ZERO;
        if let (Some((h, p)), true) = (self.last, dt > 0.0) {
            let d = Vec2::new(angle_diff(i.heading, h), i.pitch - p);
            if d.length() < VIEW_JUMP {
                rate = (d / dt).clamp(Vec2::splat(-MAX_TURN_RATE), Vec2::splat(MAX_TURN_RATE));
            }
        }
        self.last = Some((i.heading, i.pitch));
        let limit = Vec2::new(MAX_LAG_YAW, MAX_LAG_PITCH) * heft;
        let chase = CHASE_SECONDS * heft;
        // Two springs in a row: a lone spring's speed is smooth but its acceleration jumps
        // whenever what it follows does (a mouse's every uneven frame); two in a row smooth
        // that too (Daniel Holden's "double spring", theorangeduck.com "Spring-It-On";
        // research of 2026-11-14). The first is short and only averages; the second is the
        // arms' weight.
        let average = omega_for(TURN_AVERAGE_SECONDS);
        let omega = omega_for(FOLLOW_SECONDS * TURN_WEIGHT * slow);
        for k in 0..2 {
            // The lag this frame's turn asks for, averaged, then bent towards the limit by a
            // curve that never flattens (`gentle_limit`): a faster turn always asks a little
            // more, and a slower one at once a little less. The arms follow it (bounded by
            // what they follow, so never past the limit, and back towards the view from the
            // moment the turn eases). (Bending each frame's rate before averaging lost most
            // of an uneven turn: the sixth playtest's trace.)
            self.turn_pre[k].step(-rate[k] * chase, average, dt);
            let asked = self.turn_pre[k].value;
            let r = asked.abs() / chase.max(1e-6);
            let want = gentle_limit(asked * r * r / (r * r + TURN_KNEE * TURN_KNEE), limit[k]);
            self.turn[k].step(want, omega, dt);
        }
        self.lag = Vec2::new(self.turn[0].value, self.turn[1].value);

        // The sprung hand, pushed by the body's acceleration across the ground and up. The
        // velocities go first through a short spring (`nvf_play::ease::Pushed`), so a
        // sudden change (a take-off, a landing) pushes the hand smoothly, without a jolt.
        let pre = omega_for(PUSH_SMOOTH_SECONDS * slow);
        let move_omega = MOVE_OMEGA / slow;
        let move_peak = MOVE_PEAK_PER_SPEED * heft;
        for (s, v) in self.hand.iter_mut().zip([i.velocity.x, i.velocity.y]) {
            s.step(v, pre, move_omega, move_peak, dt);
        }
        // Up and down only a take-off and a landing push: a free fall moves the body and the
        // hand alike (`nvf_play::ease::carried_push`).
        let push = carried_push(self.last_vertical, i.vertical);
        let push = if i.hard_landings { nvf_play::inertia::hard_landing(push) } else { push };
        self.last_vertical = i.vertical;
        let lift_omega = LIFT_OMEGA / slow;
        self.lift.step_delta(push, pre, lift_omega, LIFT_PEAK_PER_SPEED * heft, dt);
        // The strafe's tilt follows the sideways speed on its own spring.
        let lean_target = STRAFE_ROLL * (i.velocity.x / FULL_BOB_SPEED).clamp(-1.0, 1.0);
        self.lean.step(lean_target, omega_for(STRAFE_ROLL_SECONDS * slow), dt);
        // Raising the gun to aim or block, lowering it, drawing and putting it away: it
        // trails its way a little.
        let (before, drawn_before) = (self.raise.value, self.draw.value);
        let ramp = omega_for(AIM_RAMP_SECONDS * slow);
        self.raise.step(i.raised.clamp(0.0, 1.0), ramp, dt);
        self.draw.step(i.drawn.clamp(0.0, 1.0), ramp, dt);
        // Aiming's own share is left out (AIM_SHARE): its trail and settle made a second
        // movement after the gun reached the eye (the sixth playtest's trace); the aim's
        // eased cross-over is its inertia now.
        let raise_speed = if dt > 0.0 { ((self.raise.value - before) * AIM_TRAVEL * AIM_SHARE + (self.draw.value - drawn_before) * DRAW_TRAVEL) / dt } else { 0.0 };
        let raise_accel = if dt > 0.0 { (raise_speed - self.raise_speed) / dt } else { 0.0 };
        self.raise_speed = raise_speed;
        let aim_omega = AIM_OMEGA / slow;
        self.aim_lag.step(pushed_target(raise_accel, aim_omega, AIM_PEAK_PER_SPEED * heft), aim_omega, dt);
        // A blow taken jolts the hand: its springs set moving, then they settle.
        self.hand[0].out.rate += i.punch.x * PUNCH_SPEED;
        self.hand[1].out.rate += i.punch.y * PUNCH_SPEED;
        self.lift.out.rate += i.punch.z * PUNCH_SPEED;
        // Recoil: each shot sets the recoil springs moving (from rest a spring set moving
        // at v peaks at v / (ω e)), back, up and a little aside, then they settle.
        let recoil_omega = RECOIL_OMEGA / slow;
        if i.kick > 0.0 {
            self.shots = self.shots.wrapping_add(1);
            let side = ((self.shots as f32 * 12.9898).sin() * 43_758.547).fract() * 2.0 - 1.0;
            let v = recoil_omega * E * i.kick * heft;
            self.recoil[0].rate -= RECOIL_BACK * v;
            self.recoil[1].rate += RECOIL_UP * v;
            self.recoil[2].rate += RECOIL_SIDE * side * v;
        }
        for s in &mut self.recoil {
            s.step(0.0, recoil_omega, dt);
        }

        // The bob follows the speed; breathing shows standing still, and out of breath
        // always, faster and larger.
        let speed = i.velocity.length();
        // In the air the stride's bob fades and its middle stays (`head_bob.rs`, the same
        // for the view): the gun lands where it was carried.
        let running = (speed / FULL_BOB_SPEED).min(1.0);
        let bob_target = if i.airborne { 0.0 } else { running };
        self.bob.step(bob_target, omega_for(BOB_SETTLE_SECONDS * slow), dt);
        self.bob_carry.step(running, omega_for(BOB_SETTLE_SECONDS * slow), dt);
        let exertion = i.exertion.clamp(0.0, 1.0);
        let still = if !i.airborne && speed < BREATH_BELOW_SPEED { 1.0 } else { 0.0f32 };
        let breath_goal = if i.breathing { still.max(exertion) } else { 0.0 };
        self.breath += (breath_goal - self.breath) * follow(BREATH_FADE_SECONDS);
        let pace = 1.0 + (EXERTED_PACE - 1.0) * exertion;
        self.breath_phase = (self.breath_phase + dt * pace / (BREATH_SECONDS * heft)).fract();

        let g = self.gain;
        // The calmer shares down the sights and sneaking come and go on springs.
        let stance = omega_for(STANCE_SHARE_SECONDS * slow);
        self.sighted.step(f32::from(u8::from(i.sights)), stance, dt);
        self.crouched.step(f32::from(u8::from(i.sneaking)), stance, dt);
        let (a, n) = (self.sighted.value.clamp(0.0, 1.0), self.crouched.value.clamp(0.0, 1.0));
        let mix = |share: f32, by: f32| 1.0 + (share - 1.0) * by;
        let sway_share = g * mix(SIGHTS_SWAY, a);
        let bob_calm = g * mix(SIGHTS_BOB, a) * mix(SNEAK_BOB, n);
        let hush = 1.0 - smoothstep(LIFT_HUSH_FROM, LIFT_HUSH_FULL, -soft_clamp(self.lift.out.value, MAX_LIFT * heft));
        let (bob_share, bob_carried) = (bob_calm * self.bob.value * hush, bob_calm * self.bob_carry.value);
        let breath_calm = mix(SNEAK_BREATH, n);
        let breath_share = g * self.breath * heft * (1.0 + (EXERTED_SIZE - 1.0) * exertion) * (breath_calm + (SIGHTS_SWAY - breath_calm) * a);
        let recoil_share = g * mix(SIGHTS_RECOIL, a);
        let back = -soft_clamp(-self.recoil[0].value, MAX_RECOIL_BACK) * recoil_share;
        let kick_up = soft_clamp(self.recoil[1].value, MAX_RECOIL_UP) * recoil_share;
        let kick_side = soft_clamp(self.recoil[2].value, MAX_RECOIL_UP) * recoil_share;
        self.peak_recoil = self.peak_recoil.max(-back);
        let lag = self.lag * sway_share;
        let lift = soft_clamp(self.lift.out.value, MAX_LIFT * heft) * sway_share;
        // The raise's lag is the act of aiming itself: not calmed down the sights.
        let aim = soft_clamp(self.aim_lag.value, MAX_AIM_LAG * heft) * g;
        let hand = Vec2::new(self.hand[0].out.value, self.hand[1].out.value);
        let most = MAX_MOVE_LAG * heft;
        let hand = if hand.length() > 1e-6 { hand * (soft_clamp(hand.length(), most) / hand.length()) } else { Vec2::ZERO };
        let hand = hand * sway_share;
        // Heading grows clockwise, a turn about +z anticlockwise: arms behind a turn to
        // the right point left of it. A dip, a lag behind and a trailing raise tip the
        // muzzle down.
        // Walking ahead tips the muzzle down a little; a reload leans the gun over.
        let ahead_w = omega_for(AHEAD_SECONDS * 0.6 * slow);
        let ahead_goal = (i.velocity.y / AHEAD_FULL_SPEED).clamp(0.0, 1.0);
        self.ahead[0].step(ahead_goal, ahead_w, dt);
        let first = self.ahead[0].value;
        self.ahead[1].step(first, ahead_w, dt);
        let reload_w = omega_for(RELOAD_SECONDS * slow);
        self.reload[0].step(i.reloading.clamp(0.0, 1.0), reload_w, dt);
        let first = self.reload[0].value;
        self.reload[1].step(first, reload_w, dt);
        let reload = self.reload[1].value * g;
        let jump_w = omega_for(JUMP_TIP_SECONDS * slow);
        let off_ground = i.airborne || i.vertical > JUMP_TIP_RISE;
        self.jump_tip[0].step(f32::from(u8::from(off_ground)), jump_w, dt);
        let first = self.jump_tip[0].value;
        self.jump_tip[1].step(first, jump_w, dt);
        let jump_tip = Quat::from_rotation_x(-JUMP_TIP * self.jump_tip[1].value * g * (1.0 - a));
        // Off the ground the jump's tip takes the walking tip's place, at its own pace each
        // way: running ahead the muzzle goes from the one to the other in one move. The walk's
        // springs follow the speed through the air; with them let go in the air they came
        // back over 0.6 s after a running landing, a second, slower dip of the gun after the
        // jump tip's return (the trace `movement-inertia-trace-run-jump`).
        let walk = self.ahead[1].value * g * (1.0 - a) * (1.0 - self.jump_tip[1].value.clamp(0.0, 1.0));
        // The view's pitch (up positive): down raises the gun and draws it in, up lowers it.
        let s = i.pitch.sin();
        let (down, up) = ((-s).max(0.0) * g * (1.0 - a), s.max(0.0) * g * (1.0 - a));
        let pitch_offset = Vec3::new(0.0, -PITCH_DOWN_BACK * down + PITCH_UP_OUT * up, PITCH_DOWN_RAISE * down - PITCH_UP_DROP * up);
        let tip = lag.y + (lift + aim) * LIFT_TIP + hand.y * AHEAD_TIP + kick_up + PITCH_DOWN_TIP * down - PITCH_UP_TIP * up - AHEAD_WALK_TIP * walk - RELOAD_TIP * reload;
        let turn = Quat::from_rotation_z(-lag.x + kick_side) * Quat::from_rotation_x(tip);
        let a = std::f32::consts::TAU * (i.phase - FOOTFALL_PHASE);
        // Down at each footfall (twice a cycle), across once a cycle.
        let dip = -BOB_DIP * 0.5 * (bob_carried + (2.0 * a).cos() * bob_share);
        let swing = a.sin();
        let roll = lag.x * LAG_ROLL + BOB_ROLL * swing * bob_share + self.lean.value * sway_share + RELOAD_ROLL * reload;
        let b = std::f32::consts::TAU * self.breath_phase;
        let breath = Vec2::new(b.sin(), 0.5 * (2.0 * b).sin()) * BREATH_WIDTH * breath_share;
        let offset = Vec3::new(BOB_SWAY * swing * bob_share + hand.x + breath.x, hand.y + back, dip + lift + aim + breath.y + hand.y.min(0.0) * AHEAD_DROP) + pitch_offset + Vec3::new(-RELOAD_IN * reload, 0.0, -AHEAD_WALK_DROP * walk - RELOAD_DROP * reload);
        self.peak_lag = self.peak_lag.max(lag.abs());
        self.peak_dip = self.peak_dip.max(-dip);
        self.peak_lift_dip = self.peak_lift_dip.max(-lift);
        // The jump's tip turns about the grip: the turn about the eye and a move that puts
        // the grip back.
        let base = turn * Quat::from_rotation_y(roll);
        SwayPose { rotation: base * jump_tip, offset: offset + base * (GRIP_PIVOT - jump_tip * GRIP_PIVOT) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(heading: f32, velocity: Vec2, phase: f32) -> SwayInput {
        SwayInput { dt: 1.0 / 60.0, heading, velocity, phase, on: true, ..SwayInput::default() }
    }

    fn settle(s: &mut ViewSway, i: SwayInput, frames: usize) -> SwayPose {
        let mut p = SwayPose::NONE;
        for _ in 0..frames {
            p = s.update(i);
        }
        p
    }

    #[test]
    fn looking_down_raises_the_gun_and_up_lowers_it_but_not_down_the_sights() {
        let at = |pitch: f32, sights: bool| settle(&mut ViewSway::default(), SwayInput { pitch, sights, ..input(0.0, Vec2::ZERO, 0.0) }, 600).offset;
        let (level, down, up) = (at(0.0, false), at(-1.2, false), at(1.2, false));
        assert!(down.z > level.z + 1.0 && down.y < level.y - 1.0, "down: {down} against {level}");
        assert!(up.z < level.z - 0.5 && up.y > level.y, "up: {up} against {level}");
        let sighted = at(-1.2, true) - at(0.0, true);
        assert!(sighted.length() < 0.05, "down the sights the gun stays on the eye: {sighted}");
    }

    #[test]
    fn walking_ahead_tips_the_muzzle_down_and_a_reload_leans_the_gun() {
        let pose = |v: Vec2, reloading: f32| settle(&mut ViewSway::default(), SwayInput { reloading, ..input(0.0, v, 0.0) }, 600);
        let ahead = |p: SwayPose| (p.rotation * Vec3::Y).z;
        assert!(ahead(pose(Vec2::new(0.0, 150.0), 0.0)) < ahead(pose(Vec2::ZERO, 0.0)) - 0.03, "the muzzle points lower walking ahead");
        assert!((ahead(pose(Vec2::new(0.0, -150.0), 0.0)) - ahead(pose(Vec2::ZERO, 0.0))).abs() < 0.01, "not backing up");
        let (still, reloading) = (pose(Vec2::ZERO, 0.0), pose(Vec2::ZERO, 1.0));
        assert!(reloading.offset.z < still.offset.z - 1.0, "down for the reload");
        assert!(reloading.rotation.angle_between(still.rotation) > 0.1, "turned over");
    }

    #[test]
    fn in_the_air_the_muzzle_tips_down_about_the_grip() {
        // No take-off push: the body is already in the air at a steady fall.
        let pose = |airborne: bool| settle(&mut ViewSway::default(), SwayInput { airborne, ..input(0.0, Vec2::ZERO, 0.0) }, 600);
        let (ground, air) = (pose(false), pose(true));
        let ahead = |p: SwayPose| (p.rotation * Vec3::Y).z;
        assert!(ahead(air) < ahead(ground) - 0.08, "the muzzle points lower in the air: {} against {}", ahead(air), ahead(ground));
        let grip = |p: SwayPose| p.rotation * GRIP_PIVOT + p.offset;
        assert!(grip(air).distance(grip(ground)) < 0.05, "the grip stays put: {} against {}", grip(air), grip(ground));
        let muzzle = |p: SwayPose| p.rotation * (GRIP_PIVOT + Vec3::Y * 30.0) + p.offset;
        assert!(muzzle(air).z < muzzle(ground).z - 2.5, "the muzzle goes down: {} against {}", muzzle(air), muzzle(ground));
        // Down the sights none.
        let sighted = |airborne: bool| settle(&mut ViewSway::default(), SwayInput { airborne, sights: true, ..input(0.0, Vec2::ZERO, 0.0) }, 600);
        assert!((ahead(sighted(true)) - ahead(sighted(false))).abs() < 1e-3);
    }

    #[test]
    fn standing_still_only_breathes() {
        let mut s = ViewSway::default();
        let mut most = 0.0f32;
        for _ in 0..600 {
            let p = s.update(input(1.0, Vec2::ZERO, 0.0));
            assert!(p.rotation.angle_between(Quat::IDENTITY) < 1e-4, "{p:?}");
            assert!(p.offset.y.abs() < 1e-4);
            most = most.max(p.offset.x.abs());
        }
        assert!(most > 0.8 * BREATH_WIDTH && most <= BREATH_WIDTH + 1e-4, "a small drift: {most}");
    }

    #[test]
    fn breathing_stops_while_moving_and_calms_down_the_sights() {
        let still = |sights: bool, v: Vec2| {
            let mut s = ViewSway::default();
            (0..600).map(|_| s.update(SwayInput { sights, ..input(0.0, v, 0.0) }).offset.x.abs()).skip(300).fold(0.0f32, f32::max)
        };
        let free = still(false, Vec2::ZERO);
        let aimed = still(true, Vec2::ZERO);
        assert!((aimed / free - SIGHTS_SWAY).abs() < 0.02, "{aimed} of {free}");
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::new(0.0, 300.0), FOOTFALL_PHASE), 600);
        assert!(s.breath < 1e-3, "{}", s.breath);
    }

    #[test]
    fn turning_right_leaves_the_arms_to_the_left_and_they_catch_up() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        // A steady turn to the right at 0.3 rad/s: the lag it asks for, bent towards the
        // limit.
        let mut h = 0.0;
        let mut p = SwayPose::NONE;
        for _ in 0..180 {
            h += 0.3 / 60.0;
            p = s.update(input(h, Vec2::ZERO, 0.0));
        }
        let ahead = p.rotation * Vec3::Y;
        assert!(ahead.x < -0.01, "the weapon points left of the view: {ahead:?}");
        let want = -gentle_limit(0.3 * CHASE_SECONDS * 0.09 / (0.09 + TURN_KNEE * TURN_KNEE), MAX_LAG_YAW);
        assert!((s.turn[0].value - want).abs() < 1e-3, "{} against {want}", s.turn[0].value);
        let p = settle(&mut s, input(h, Vec2::ZERO, 0.0), 240);
        assert!(p.rotation.angle_between(Quat::IDENTITY) < 1e-3, "back on the view: {p:?}");
    }

    #[test]
    fn a_resting_hands_mouse_twitches_leave_the_arms_on_the_view() {
        // Single mouse counts (a thousandth of a radian × a sensitivity of 0.4) now and then,
        // back and forth, as a hand resting on the mouse sends them.
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        let (mut h, mut most) = (0.0f32, 0.0f32);
        for i in 0..600u32 {
            if i % 7 == 0 {
                h += if (i / 7) % 3 == 0 { -0.0004 } else { 0.0004 };
            }
            s.update(input(h, Vec2::ZERO, 0.0));
            most = most.max(s.turn[0].value.abs());
        }
        // A thousandth of a degree; without the knee about 0.02°.
        assert!(most < 2e-5, "the arms stay with the view: {most}");
    }

    #[test]
    fn the_lag_builds_up_gently_and_starts_back_as_soon_as_the_view_stops() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        // The view starts turning at once; the lag builds up, never in a jump.
        let mut h = 0.0;
        let mut lags = Vec::new();
        for _ in 0..30 {
            h += 0.5 / 60.0;
            s.update(input(h, Vec2::ZERO, 0.0));
            lags.push(-s.turn[0].value);
        }
        let full = gentle_limit(0.5 * CHASE_SECONDS, MAX_LAG_YAW);
        assert!(lags[0] < 0.05 * full, "a gentle start: {} of {full}", lags[0]);
        assert!(lags.windows(2).all(|w| w[1] >= w[0] - 1e-6), "then more: {lags:?}");
        // The view stops: the arms turn back within a few frames (no waiting at the
        // limit), never past the view, and rest on it.
        let mut after = Vec::new();
        for _ in 0..240 {
            s.update(input(h, Vec2::ZERO, 0.0));
            assert!(s.turn[0].value <= 1e-6, "past the view: {}", s.turn[0].value);
            after.push(-s.turn[0].value);
        }
        let peak = after.iter().enumerate().fold((0, 0.0f32), |m, (i, &v)| if v > m.1 { (i, v) } else { m }).0;
        assert!(peak <= 6, "back on its way within 0.1 s: the most lag at frame {peak}");
        assert!(after[peak..].windows(2).all(|w| w[1] <= w[0] + 1e-6), "then steadily back");
        assert!(after.last().unwrap().abs() < 1e-3);
    }

    #[test]
    fn the_turn_lag_is_the_same_at_any_frame_rate() {
        let lag_after = |hz: f32| {
            let mut s = ViewSway::default();
            let dt = 1.0 / hz;
            let mut h = 0.0;
            s.update(SwayInput { dt, ..input(h, Vec2::ZERO, 0.0) });
            for _ in 0..(hz / 4.0) as usize {
                h += 0.4 * dt;
                s.update(SwayInput { dt, ..input(h, Vec2::ZERO, 0.0) });
            }
            s.turn[0].value
        };
        let at60 = lag_after(60.0);
        for hz in [120.0, 144.0, 240.0] {
            assert!((lag_after(hz) - at60).abs() < 1e-3, "{hz}: {} against {at60}", lag_after(hz));
        }
    }

    #[test]
    fn a_fast_flick_stays_within_the_limit() {
        let mut s = ViewSway::default();
        let mut h = 0.0;
        s.update(input(h, Vec2::ZERO, 0.0));
        for _ in 0..60 {
            h += 0.3;
            s.update(input(h, Vec2::ZERO, 0.0));
            assert!(s.lag.x.abs() <= MAX_LAG_YAW + 1e-6, "{}", s.lag.x);
        }
        assert!(s.lag.x.abs() > MAX_LAG_YAW * 0.9, "{}", s.lag.x);
    }

    #[test]
    fn starting_to_run_leaves_the_weapon_behind_and_a_steady_pace_brings_it_back() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        // The body's speed ramps up over 0.12 s (`nvf_play::inertia`), then holds.
        let mut most = 0.0f32;
        for f in 0..60 {
            let v = 344.0 * (f as f32 / 7.0).min(1.0);
            let p = s.update(input(0.0, Vec2::new(0.0, v), 0.0));
            most = most.min(p.offset.y);
        }
        assert!((-MAX_MOVE_LAG..-0.3).contains(&most), "behind while speeding up: {most}");
        let p = settle(&mut s, input(0.0, Vec2::new(0.0, 344.0), 0.0), 120);
        assert!(p.offset.y.abs() < 1e-2, "back at a steady pace: {p:?}");
    }

    #[test]
    fn walking_dips_at_the_footfalls() {
        let mut s = ViewSway::default();
        let v = Vec2::new(0.0, 300.0);
        settle(&mut s, input(0.0, v, FOOTFALL_PHASE), 120);
        let at_footfall = s.update(input(0.0, v, FOOTFALL_PHASE)).offset.z;
        let between = s.update(input(0.0, v, FOOTFALL_PHASE + 0.25)).offset.z;
        let other = s.update(input(0.0, v, FOOTFALL_PHASE + 0.5)).offset.z;
        // Walking ahead also holds the gun a little lower (`AHEAD_WALK_DROP`): the bob is
        // measured from there.
        let (at_footfall, between, other) = (at_footfall + AHEAD_WALK_DROP, between + AHEAD_WALK_DROP, other + AHEAD_WALK_DROP);
        assert!(at_footfall < -0.5 * BOB_DIP && other < -0.5 * BOB_DIP, "{at_footfall} {other}");
        assert!(between > -0.05, "{between}");
    }

    /// A standing jump of the player (`fnv_phys`: 61.6 units high under 686.7 units/s²)
    /// and its landing, a frame at a time: the lowest and highest offset while rising,
    /// near the top, and just after landing.
    fn jump(land_speed: f32) -> (f32, f32, f32) {
        let g = 686.7;
        let up = (2.0f32 * g * 61.6).sqrt();
        let mut s = ViewSway::default();
        let dt = 1.0 / 60.0;
        let air = |vz: f32| SwayInput { vertical: vz, airborne: true, ..input(0.0, Vec2::ZERO, 0.0) };
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        let mut take_off = 0.0f32;
        let mut top = f32::MIN;
        let mut vz = up;
        while vz > -land_speed {
            let z = s.update(air(vz)).offset.z;
            if vz > up * 0.5 {
                take_off = take_off.min(z);
            }
            if vz.abs() < up * 0.2 {
                top = top.max(z);
            }
            vz -= g * dt;
        }
        let mut landing = 0.0f32;
        for _ in 0..30 {
            landing = landing.min(s.update(input(0.0, Vec2::ZERO, 0.0)).offset.z);
        }
        (take_off, top, landing)
    }

    #[test]
    fn a_jump_dips_at_the_take_off_comes_back_through_the_fall_and_dips_on_landing() {
        let (take_off, top, landing) = jump(291.0);
        assert!(take_off < -0.5, "dips at the take-off: {take_off}");
        // The fall itself pushes nothing: the gun is on its way back to rest, never above it.
        assert!(top > take_off && top < 0.2, "back towards rest near the top: {top} after {take_off}");
        assert!((-MAX_LIFT..-0.4).contains(&landing), "dips on landing: {landing}");
        let (_, _, hard) = jump(700.0);
        assert!(hard < landing - 0.2 && hard >= -MAX_LIFT, "a harder landing dips more: {hard} against {landing}");
    }

    #[test]
    fn the_sights_and_the_air_calm_it() {
        let v = Vec2::new(0.0, 300.0);
        let mut free = ViewSway::default();
        let mut sights = ViewSway::default();
        let mut air = ViewSway::default();
        let a = settle(&mut free, input(0.0, v, FOOTFALL_PHASE), 240).offset.z;
        let b = settle(&mut sights, SwayInput { sights: true, ..input(0.0, v, FOOTFALL_PHASE) }, 240).offset.z;
        // In the air the stride's bob is gone where the gun is held (the jump's tip turns it
        // about the grip); its middle, half the dip at a run, stays, so the gun lands where
        // it was carried.
        let c = settle(&mut air, SwayInput { airborne: true, ..input(0.0, v, FOOTFALL_PHASE) }, 240);
        let c = (c.rotation * GRIP_PIVOT + c.offset).z - GRIP_PIVOT.z;
        assert!(b > a * 0.5 && b < 0.0, "{a} {b}");
        assert!((c + 0.5 * BOB_DIP).abs() < 1e-3, "{c}");
    }

    #[test]
    fn a_running_landing_does_not_sink_the_gun_after_it() {
        // Running ahead, 0.6 s in the air and running on, at the stride's place where the
        // footfall dip is at its middle (its swing is nothing there) and with no push from
        // the take-off or the landing: 0.35 s after the landing the gun is where it stays.
        // With the bob's middle and the walking drop let go in the air, they came back over
        // half a second after it: the gun sank on after the jump's tip had gone.
        let v = Vec2::new(0.0, 344.0);
        let mid = FOOTFALL_PHASE + 0.125;
        let held = |p: SwayPose| (p.rotation * GRIP_PIVOT + p.offset).z - GRIP_PIVOT.z;
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, v, mid), 240);
        settle(&mut s, SwayInput { airborne: true, ..input(0.0, v, mid) }, 36);
        let soon = held(settle(&mut s, input(0.0, v, mid), 21));
        let later = held(settle(&mut s, input(0.0, v, mid), 90));
        assert!((soon - later).abs() < 0.05, "0.35 s after the landing {soon}, settled {later}");
    }

    #[test]
    fn a_heavy_weapon_lags_further_and_slower() {
        assert_eq!(heft(0.0), 1.0);
        assert_eq!(heft(LIGHT_WEAPON), 1.0);
        assert_eq!(heft(HEAVY_WEAPON), MOST_HEFT);
        assert_eq!(heft(40.0), MOST_HEFT);
        assert!(heft(11.5) > 1.2 && heft(11.5) < 1.3);
        // A slow turn, where the lag asked for is still rate × the chase time.
        let steady = |heft: f32| {
            let mut s = ViewSway::default();
            let mut h = 0.0;
            for _ in 0..240 {
                h += 0.05 / 60.0;
                s.update(SwayInput { heft, ..input(h, Vec2::ZERO, 0.0) });
            }
            s.turn[0].value
        };
        assert!((steady(MOST_HEFT) / steady(1.0) - MOST_HEFT).abs() < 0.02);
    }

    #[test]
    fn a_flick_moves_the_arms_smoothly_within_the_limit_and_they_follow_with_weight() {
        // The view turns 90° in 0.15 s, then holds: the head first, the arms after.
        let flick = |heft: f32| {
            let mut s = ViewSway::default();
            let i = |h: f32| SwayInput { heft, ..input(h, Vec2::ZERO, 0.0) };
            settle(&mut s, i(0.0), 30);
            let mut h = 0.0;
            for _ in 0..9 {
                h += std::f32::consts::FRAC_PI_2 / 9.0;
                s.update(i(h));
            }
            let mut most = s.lag.x.abs();
            let mut frames = 0;
            while frames < 10 || s.lag.x.abs() > 0.05 * MAX_LAG_YAW {
                s.update(i(h));
                most = most.max(s.lag.x.abs());
                frames += 1;
            }
            (most, frames as f32 / 60.0)
        };
        let (most, back) = flick(1.0);
        assert!(most > 0.3 * MAX_LAG_YAW && most <= MAX_LAG_YAW, "a visible lag within the limit: {}°", most.to_degrees());
        assert!(back > 0.2 && back < 1.0, "back on the view soon, not at once: {back} s");
        // It never swings past the view on the way back.
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 30);
        let mut h = 0.0;
        for _ in 0..6 {
            h += 0.6;
            s.update(input(h, Vec2::ZERO, 0.0));
        }
        for _ in 0..240 {
            s.update(input(h, Vec2::ZERO, 0.0));
            assert!(s.turn[0].value <= 1e-6, "past the view: {}", s.turn[0].value);
        }
        let (_, slow) = flick(MOST_HEFT);
        assert!(slow > 1.2 * back, "the heaviest weapon follows slower: {slow} against {back}");
    }

    #[test]
    fn a_strafe_moves_the_weapon_aside_slowly() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        // Strafing right at a run's speed, reached over half a second.
        let mut xs = Vec::new();
        for f in 0..120 {
            let v = 344.0 * (f as f32 / 30.0).min(1.0);
            xs.push(s.update(input(0.0, Vec2::new(v, 0.0), 0.0)).offset.x);
        }
        let (at, most) = xs.iter().enumerate().fold((0, 0.0f32), |m, (i, &x)| if x < m.1 { (i, x) } else { m });
        assert!(most < -0.3, "left behind to the left: {most}");
        assert!(at >= 24, "the slide builds up over a while, not at once: most at frame {at}");
        // Leaning into it on its own spring: not all at once either.
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 30);
        s.update(input(0.0, Vec2::new(300.0, 0.0), 0.0));
        assert!(s.lean.value < 0.1 * STRAFE_ROLL, "{}", s.lean.value);
        settle(&mut s, input(0.0, Vec2::new(300.0, 0.0), 0.0), 120);
        assert!((s.lean.value - STRAFE_ROLL).abs() < 1e-3);
    }

    #[test]
    fn speeding_up_ahead_drops_and_tips_the_weapon_and_a_steady_pace_brings_it_back() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        let mut low = 0.0f32;
        let mut tip = 0.0f32;
        for f in 0..30 {
            let p = s.update(input(0.0, Vec2::new(0.0, 344.0 * f as f32 / 30.0), 0.0));
            low = low.min(p.offset.z);
            tip = tip.min((p.rotation * Vec3::Y).z);
        }
        assert!(low < -0.15 && tip < -0.002, "lower and the muzzle down while speeding up: {low} {tip}");
    }

    #[test]
    fn raising_the_gun_to_aim_adds_no_movement_of_its_own() {
        // The aim's eased cross-over is its whole motion: the sway adds no trail and no
        // settle after it (the sixth playtest: "2 different movements").
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 600);
        let mut most = 0.0f32;
        for _ in 0..60 {
            let p = s.update(SwayInput { raised: 1.0, ..input(0.0, Vec2::ZERO, 0.0) });
            most = most.max(p.offset.z.abs());
        }
        assert!(most <= 0.5 * BREATH_WIDTH + 1e-3, "only the breathing moves it: {most}");
        const { assert!(AIM_SHARE == 0.0) };
    }

    #[test]
    fn the_turn_motion_has_no_sudden_change_of_pace() {
        // An uneven flick (the mouse's frames vary), then a hold: the arms' turn changes
        // its speed only a little from frame to frame, all the way.
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 30);
        let mut h = 0.0;
        let mut lags = Vec::new();
        for f in 0..90 {
            if f < 12 {
                h += [0.15, 0.05, 0.2, 0.1][f % 4];
            }
            s.update(input(h, Vec2::ZERO, 0.0));
            lags.push(s.lag.x);
        }
        // Smooth: the arms speed up and slow down a few times only (out, then back), never
        // in ripples following the uneven frames, and their speed never jumps.
        let speeds: Vec<f32> = lags.windows(2).map(|w| (w[1] - w[0]) * 60.0).collect();
        let accels: Vec<f32> = speeds.windows(2).map(|w| w[1] - w[0]).filter(|a| a.abs() > 1e-3).collect();
        let reversals = accels.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
        assert!(reversals <= 3, "ripples in the arms' turn: {reversals} reversals: {speeds:?}");
        let most_change = speeds.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(most_change < 0.4, "a jolt in the arms' turn: {most_change} rad/s in a frame");
    }

    #[test]
    fn a_shot_kicks_the_gun_back_and_up_and_it_settles_and_rapid_fire_plateaus() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        s.update(SwayInput { kick: 1.0, ..input(0.0, Vec2::ZERO, 0.0) });
        let mut back = 0.0f32;
        let mut up = 0.0f32;
        for _ in 0..30 {
            let p = s.update(input(0.0, Vec2::ZERO, 0.0));
            back = back.min(p.offset.y);
            up = up.max((p.rotation * Vec3::Y).z);
        }
        assert!((back + RECOIL_BACK).abs() < 0.25, "back about {RECOIL_BACK}: {back}");
        assert!(up > 0.6 * RECOIL_UP, "muzzle up: {up}");
        let p = settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 120);
        assert!(p.offset.y.abs() < 0.05, "settled: {p:?}");
        // Ten shots a second for two seconds: the kick builds to a plateau within its limit.
        let mut most = 0.0f32;
        for f in 0..120 {
            let p = s.update(SwayInput { kick: if f % 6 == 0 { 1.0 } else { 0.0 }, ..input(0.0, Vec2::ZERO, 0.0) });
            most = most.min(p.offset.y);
        }
        assert!(most > -MAX_RECOIL_BACK - BREATH_WIDTH && most < -RECOIL_BACK, "{most}");
    }

    #[test]
    fn a_blow_jolts_the_gun_and_drawing_trails() {
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        s.update(SwayInput { punch: Vec3::new(0.0, 0.0, -1.0), ..input(0.0, Vec2::ZERO, 0.0) });
        let mut low = 0.0f32;
        for _ in 0..30 {
            low = low.min(s.update(input(0.0, Vec2::ZERO, 0.0)).offset.z);
        }
        assert!(low < -0.8, "{low}");
        let mut s = ViewSway::default();
        settle(&mut s, input(0.0, Vec2::ZERO, 0.0), 60);
        let mut low = 0.0f32;
        for _ in 0..30 {
            low = low.min(s.update(SwayInput { drawn: 1.0, ..input(0.0, Vec2::ZERO, 0.0) }).offset.z);
        }
        assert!(low < -0.2, "trails low as it comes up: {low}");
    }

    #[test]
    fn a_rescaled_pose_keeps_its_depth_and_scales_across_and_its_turn() {
        let grip = |p: SwayPose| p.rotation * GRIP_PIVOT + p.offset;
        // A move alone: across made half, depth kept.
        let moved = SwayPose { rotation: Quat::IDENTITY, offset: Vec3::new(1.0, -2.0, 0.5) };
        assert_eq!(rescale(moved, 1.0), moved);
        assert!((rescale(moved, 0.5).offset - Vec3::new(0.5, -2.0, 0.25)).length() < 1e-5);
        // A turn and a move: the turn made half, the grip's way across half, its depth kept.
        let p = SwayPose { rotation: Quat::from_rotation_z(0.1) * Quat::from_rotation_x(0.04), offset: Vec3::new(1.0, -2.0, 0.5) };
        let half = rescale(p, 0.5);
        assert!((half.rotation.to_scaled_axis() - p.rotation.to_scaled_axis() * 0.5).length() < 1e-5);
        let (way, half_way) = (grip(p) - GRIP_PIVOT, grip(half) - GRIP_PIVOT);
        assert!((half_way - Vec3::new(way.x * 0.5, way.y, way.z * 0.5)).length() < 1e-4, "{half_way} against {way}");
        // A turn about the grip (the jump's muzzle tip) stays about the grip.
        let tip = Quat::from_rotation_x(-0.1);
        let about = SwayPose { rotation: tip, offset: GRIP_PIVOT - tip * GRIP_PIVOT };
        assert!((grip(rescale(about, 0.6)) - GRIP_PIVOT).length() < 1e-4);
        assert_eq!(rescale(SwayPose::NONE, 0.5), SwayPose::NONE);
    }

    #[test]
    fn switched_off_it_fades_out() {
        let mut s = ViewSway::default();
        let v = Vec2::new(0.0, 300.0);
        settle(&mut s, input(0.0, v, FOOTFALL_PHASE), 120);
        let p = settle(&mut s, SwayInput { on: false, ..input(0.0, v, FOOTFALL_PHASE) }, 120);
        assert!(p.offset.length() < 1e-3, "{p:?}");
    }
}
