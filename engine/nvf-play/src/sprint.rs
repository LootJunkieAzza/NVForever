//! Sprint: NVF Features › Advanced Movement's Sprint switch [design, #3000].
//! Holding the Sprint control (a key the player binds, none by default; `crate::stance`, the shared contract)
//! runs faster for Action Points; off ([`MovementInertia::sprint_on`] false) the player
//! moves exactly as in the original.
//!
//! - **Speed** ([`sprint_velocity`]): a full sprint is [`SPRINT_SPEED`] × the speed the
//!   player would have running, with every pace `crate::player::step` puts on a run
//!   (crippled legs, `RunPace`'s two-handed weapon, `PerkSpeed`'s Modify Run Speed perks,
//!   `ReloadPace`, `StatSpeed`'s Speed Mult and armour, the tweaks' wading and strafe
//!   paces), so whatever speeds or slows a run speeds or slows a sprint the same. Always
//!   from the run: walking (Always Run off) a sprint runs too [design, #3000]. The velocity goes from the gait's (walk or run) to the sprint's by
//!   [`StanceMotion::sprint_share`], so it comes and goes smoothly, a walk's change of gait
//!   with it, also with Movement Inertia off; with it on the ground inertia's own spring
//!   (`crate::inertia`) carries it as any change of the keys' speed. From
//!   [`RUN_GAIT_FROM`] of a sprint the body plays the run clips and glides as a run.
//! - **Direction**: forward only, and less the more the keys go aside
//!   ([`direction_share`]): straight ahead a full sprint, ahead and aside (W+D) about half
//!   of one, straight aside or any way back none [design, #3000 left it open; the request: "no
//!   sprint backwards; strafing reduces or blocks it"].
//! - **Action Points** ([`SPRINT_AP_PER_SECOND`]): a fixed amount a second, not a share of
//!   the most, so a character with more AP sprints longer (100 AP: ten seconds; 60 AP: six).
//!   Spent as the game's other AP costs are (`fnv_game::functions::add_actor_value_damage`
//!   on the player's Action Points, as the tweaks' run cost and VATS do), never more than
//!   the player has, so AP never goes under 0 (issue #2601), and AP's own perks and effects
//!   (more most AP from Agility, chems and armour; the Action Point Regen perks) act on it.
//!   While sprinting the AP does not come back (`crate::vats_mode`'s regeneration waits
//!   for [`StanceMotion::sprinting`]). At 0 the sprint ends and does not start again
//!   until the control is let go and AP has come back. None is spent in god mode.
//! - **What stops it** ([`Blocked`]): the control let go, aiming or blocking, the trigger
//!   (an attack), a reload (`crate::combat::Combat::player_reloading`) [design, #3000], sneaking or crouching, over-encumbered (`crate::player::OVERENCUMBERED`), swimming or deeper in water than
//!   [`WATER_DEPTH`], the keys not going ahead, a script's movement lock (it strikes
//!   `PlayerControl::sprint_held`), no AP. A jump does not: the sprint's speed carries
//!   into it as the air control takes it. Each ends the sprint eased, never at once
//!   ([`StanceMotion::sprint_share`] on one spring of [`SHARE_SECONDS`] both ways); held
//!   on, the sprint comes back when the stop is gone (but after running out of AP).
//! - **Out of breath** ([`StanceMotion::winded`]): stopping a sprint with AP under
//!   [`WINDED_BELOW`] of the most leaves the player winded, more the lower the AP
//!   ([`winded_strength`]): it comes on over [`WINDED_ONSET_SECONDS`] and fades over
//!   [`WINDED_FADE_SECONDS`], slowly at the end ([`Winded`]). nvforever's Camera Motion
//!   (`head_bob.rs`, its existing out-of-breath breathing) shows it, and the gun's breathing
//!   under Weapon Inertia; both off, nothing shows.
//!
//! - **Heartbeat** ([`Sprint::heart`], played by `crate::heartbeat`): while sprinting, and
//!   after a sprint while the AP comes back, the game's low-health heartbeat loops: the slower
//!   one with AP under [`HEARTBEAT_SLOW_BELOW`] of the most, the faster one under
//!   [`HEARTBEAT_FAST_BELOW`] [design, #3000; the request: "slower heartbeat from 40-20%,
//!   the faster one from the last 20%"]. AP spent by VATS alone, with no sprint, plays none.
//!   Sprint's switch (and the Global Toggle) turns it off.
//!
//! Test aids: `NVF_SPRINT_AP_PER_SECOND=<n>` and `NVF_SPRINT_SPEED=<x>` replace
//! [`SPRINT_AP_PER_SECOND`] and [`SPRINT_SPEED`] for the run ([`SprintTuning::from_env`]),
//! for trying values in game without a build.
//!
//! [`MovementInertia::sprint_on`]: crate::inertia::MovementInertia::sprint_on

use crate::control::PlayerControl;
use crate::ease::{omega_for, settle, Spring};
use crate::inertia::MovementInertia;
use crate::player::MoveIntent;
use crate::stance::StanceMotion;
use bevy::prelude::*;

/// A full sprint is this many times the run speed the player has now (after perks, Speed
/// Mult, armour and the rest) [design, #3000; the request: "default 1.45x run"].
pub const SPRINT_SPEED: f32 = 1.45;
/// Action Points a second of sprinting costs, a fixed amount whatever the most AP is
/// [design, #3000; the request: "10 AP per second"].
pub const SPRINT_AP_PER_SECOND: f32 = 10.0;
/// [`StanceMotion::sprint_share`]'s spring: 95% of the way to a full sprint, or back to
/// none, in this many seconds, the same both ways (one spring constant, so a change of
/// mind part way never hitches) [design].
pub const SHARE_SECONDS: f32 = 0.5;
/// From this share of a sprint on, a sprint begun from a walk counts as a run: the run clips
/// play (nvforever `player_body`) and the ground inertia glides as after a run [design].
pub const RUN_GAIT_FROM: f32 = 0.5;
/// The keys' share ahead (forward over the length of forward and aside, 1 straight ahead,
/// 0.71 diagonally) under which there is no sprint; from it to straight ahead the sprint
/// grows ([`direction_share`]) [design: half a sprint diagonally].
pub const LEAST_AHEAD: f32 = 0.4;
/// Feet deeper in water than this (units, about 43 cm, over the knees) stop a sprint, as
/// swimming does [design].
pub const WATER_DEPTH: f32 = 30.0;
/// Stopping a sprint with AP under this share of the most leaves the player out of breath
/// [design, #3000; the request: "below 20% of max AP"].
pub const WINDED_BELOW: f32 = 0.2;
/// How winded a stop just under [`WINDED_BELOW`] leaves the player (0–1); at 0 AP fully
/// (1), straight between: so the least of it is still clearly seen [design, #3000: judged by eye].
pub const WINDED_LEAST: f32 = 0.35;
/// Out of breath comes on through a spring settling (95%) in this many seconds [design].
pub const WINDED_ONSET_SECONDS: f32 = 0.6;
/// Out of breath fades over this many seconds along the settling curve, slowly at the end
/// [design, #3000; the request: "fades out over a few seconds"].
pub const WINDED_FADE_SECONDS: f32 = 6.0;

/// After a sprint, AP under this share of the most plays the slower heartbeat, and under
/// [`HEARTBEAT_FAST_BELOW`] the faster one [design, #3000].
pub const HEARTBEAT_SLOW_BELOW: f32 = 0.4;
pub const HEARTBEAT_FAST_BELOW: f32 = 0.2;

/// Which heartbeat a sprint asks for ([`Sprint::heart`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeartPace {
    #[default]
    Off,
    /// The slower beat (`crate::heartbeat::SOUND`, about 84 a minute).
    Slow,
    /// The faster beat (`crate::heartbeat::FAST_SOUND`, about 124 a minute).
    Fast,
}

impl HeartPace {
    pub fn text(self) -> &'static str {
        match self {
            HeartPace::Off => "off",
            HeartPace::Slow => "slow",
            HeartPace::Fast => "fast",
        }
    }
}

/// The heartbeat for AP `ap` after a sprint: none from [`HEARTBEAT_SLOW_BELOW`] of the most
/// up, the slower one under it, the faster one under [`HEARTBEAT_FAST_BELOW`].
pub fn heart_pace(ap: Ap) -> HeartPace {
    let share = ap.share();
    if ap.max <= 0.0 || share >= HEARTBEAT_SLOW_BELOW {
        HeartPace::Off
    } else if share >= HEARTBEAT_FAST_BELOW {
        HeartPace::Slow
    } else {
        HeartPace::Fast
    }
}

/// The sprint's values for this run: the constants, or the test aids' (module notes).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct SprintTuning {
    pub speed: f32,
    pub ap_per_second: f32,
}

impl Default for SprintTuning {
    fn default() -> Self {
        SprintTuning { speed: SPRINT_SPEED, ap_per_second: SPRINT_AP_PER_SECOND }
    }
}

impl SprintTuning {
    /// The constants, or `NVF_SPRINT_SPEED` and `NVF_SPRINT_AP_PER_SECOND` where set.
    pub fn from_env() -> SprintTuning {
        let read = |name: &str, lo: f32, hi: f32| std::env::var(name).ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(lo, hi));
        let t = SprintTuning { speed: read("NVF_SPRINT_SPEED", 1.0, 3.0).unwrap_or(SPRINT_SPEED), ap_per_second: read("NVF_SPRINT_AP_PER_SECOND", 0.0, 1000.0).unwrap_or(SPRINT_AP_PER_SECOND) };
        if t != SprintTuning::default() {
            info!("sprint: test aid: {:.2} x the run, {:.1} AP a second", t.speed, t.ap_per_second);
        }
        t
    }
}

/// The player's Action Points now and their most (the current value's parts: base,
/// modifiers and temporary changes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ap {
    pub now: f32,
    pub max: f32,
}

impl Ap {
    /// The share of the most left (0 with no most).
    pub fn share(&self) -> f32 {
        if self.max > 0.0 {
            (self.now / self.max).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The speed factor for a sprint share (0–1): 1 none, `speed` a full sprint.
pub fn speed_factor(share: f32, speed: f32) -> f32 {
    1.0 + (speed - 1.0) * share.clamp(0.0, 1.0)
}

/// The velocity at a sprint `share` (0–1): from the gait's own (`gait`, walking or running,
/// every pace on it) to `speed` × the run's (`run`, the same keys running with every pace),
/// straight between by the share (which is eased). Running, it is the run × [`speed_factor`].
pub fn sprint_velocity(gait: [f32; 2], run: [f32; 2], share: f32, speed: f32) -> [f32; 2] {
    let s = share.clamp(0.0, 1.0);
    [gait[0] + s * (run[0] * speed - gait[0]), gait[1] + s * (run[1] * speed - gait[1])]
}

/// How much of a sprint the keys allow (0–1): `forward` and `right` as in
/// [`MoveIntent`]. 1 straight ahead, none from [`LEAST_AHEAD`] down (aside, back).
pub fn direction_share(forward: f32, right: f32) -> f32 {
    let len = forward.hypot(right);
    if forward <= 0.0 || len < 1e-4 {
        return 0.0;
    }
    ((forward / len - LEAST_AHEAD) / (1.0 - LEAST_AHEAD)).clamp(0.0, 1.0)
}

/// How winded a stop at `ap` leaves the player: 0 at [`WINDED_BELOW`] of the most and over,
/// from [`WINDED_LEAST`] just under it to 1 at none.
pub fn winded_strength(ap: Ap) -> f32 {
    let share = ap.share();
    if ap.max <= 0.0 || share >= WINDED_BELOW {
        return 0.0;
    }
    WINDED_LEAST + (1.0 - WINDED_LEAST) * (1.0 - share / WINDED_BELOW)
}

/// Out of breath: a strength set at a stop ([`Winded::start`]), which the shown value
/// follows on a spring and which fades along the settling curve.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Winded {
    /// The strength at the last stop that winded the player.
    peak: f32,
    /// Seconds since then.
    since: f32,
    /// The shown value on its spring.
    out: Spring,
}

impl Winded {
    /// What the shown value heads for now: the strength fading over
    /// [`WINDED_FADE_SECONDS`] (the settling curve mirrored: briskly after a soft start,
    /// slowly at the end).
    pub fn target(&self) -> f32 {
        if self.peak <= 0.0 {
            return 0.0;
        }
        self.peak * (1.0 - settle(self.since / WINDED_FADE_SECONDS))
    }

    /// A stop that winds the player by `strength`: the fade starts again from it, or from
    /// what is still left if that is more.
    pub fn start(&mut self, strength: f32) {
        self.peak = strength.clamp(0.0, 1.0).max(self.target());
        self.since = 0.0;
    }

    /// `dt` seconds on; the value shown (0–1).
    pub fn step(&mut self, dt: f32) -> f32 {
        let dt = dt.max(0.0);
        self.since += dt;
        let target = self.target();
        self.out.step(target, omega_for(WINDED_ONSET_SECONDS), dt);
        if target <= 0.0 && self.out.snap(0.0, 1e-4, 1e-3) {
            self.peak = 0.0;
        }
        self.value()
    }

    pub fn value(&self) -> f32 {
        self.out.value.clamp(0.0, 1.0)
    }
}

/// Why there is no sprint now (the log names it at a sprint's end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    SwitchedOff,
    KeyLetGo,
    Aiming,
    Attacking,
    Reloading,
    Sneaking,
    Encumbered,
    InWater,
    Toddler,
    NotAhead,
    OutOfAp,
}

impl Blocked {
    pub fn text(self) -> &'static str {
        match self {
            Blocked::SwitchedOff => "Sprint switched off",
            Blocked::KeyLetGo => "the control let go",
            Blocked::Aiming => "aiming",
            Blocked::Attacking => "attacking",
            Blocked::Reloading => "reloading",
            Blocked::Sneaking => "sneaking",
            Blocked::Encumbered => "over-encumbered",
            Blocked::InWater => "in water",
            Blocked::Toddler => "toddler walk",
            Blocked::NotAhead => "not moving ahead",
            Blocked::OutOfAp => "out of AP",
        }
    }
}

/// What one frame of the sprint reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SprintInput {
    pub dt: f32,
    /// [`MovementInertia::sprint_on`].
    pub on: bool,
    /// `PlayerControl::sprint_held`.
    pub held: bool,
    /// The keys (after the locks and the walk rules), as [`MoveIntent`].
    pub forward: f32,
    pub right: f32,
    pub sneaking: bool,
    pub aiming: bool,
    pub attacking: bool,
    /// A reload (or a jam's clearing) plays or was just asked for.
    pub reloading: bool,
    pub encumbered: bool,
    pub in_water: bool,
    /// `SetPCToddler 1` is active: toddlers walk rather than sprint.
    pub toddler: bool,
    /// The player's AP; `None`: unlimited (god mode, no script runtime), nothing spent.
    pub ap: Option<Ap>,
    pub ap_per_second: f32,
}

impl Default for SprintInput {
    fn default() -> Self {
        SprintInput { dt: 0.0, on: true, held: false, forward: 0.0, right: 0.0, sneaking: false, aiming: false, attacking: false, reloading: false, encumbered: false, in_water: false, toddler: false, ap: None, ap_per_second: SPRINT_AP_PER_SECOND }
    }
}

/// What one frame of the sprint did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SprintStep {
    /// AP to spend this frame (never more than the player has).
    pub spend: f32,
    /// The sprint started this frame.
    pub started: bool,
    /// The sprint ended this frame, and why.
    pub ended: Option<Blocked>,
    /// The stop left the player winded this much.
    pub winded: Option<f32>,
}

/// The sprint's state across frames.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Sprint {
    /// [`StanceMotion::sprint_share`] on its spring.
    share: Spring,
    /// Sprinting is the target state ([`StanceMotion::sprinting`]).
    pub sprinting: bool,
    /// The AP ran out: no sprint until the control is let go.
    spent_out: bool,
    winded: Winded,
    /// This sprint's seconds and AP spent (for the log at its end).
    pub seconds: f32,
    pub spent: f32,
    /// Sprints started since the start (the exit summary).
    pub count: u32,
    /// A sprint ran and the AP has not come back over [`HEARTBEAT_SLOW_BELOW`] since.
    from_sprint: bool,
    /// The heartbeat the sprint asks for now.
    heart: HeartPace,
}

impl Sprint {
    /// What stops a sprint now, if anything.
    fn blocked(&self, i: &SprintInput) -> Option<Blocked> {
        Some(if !i.on {
            Blocked::SwitchedOff
        } else if !i.held {
            Blocked::KeyLetGo
        } else if i.aiming {
            Blocked::Aiming
        } else if i.attacking {
            Blocked::Attacking
        } else if i.reloading {
            Blocked::Reloading
        } else if i.sneaking {
            Blocked::Sneaking
        } else if i.encumbered {
            Blocked::Encumbered
        } else if i.in_water {
            Blocked::InWater
        } else if i.toddler {
            Blocked::Toddler
        } else if direction_share(i.forward, i.right) <= 0.0 {
            Blocked::NotAhead
        } else if self.spent_out || i.ap.is_some_and(|a| a.now <= 0.0) {
            Blocked::OutOfAp
        } else {
            return None;
        })
    }

    /// One frame: whether the player sprints, the AP to spend, the share and out of breath.
    pub fn update(&mut self, i: SprintInput) -> SprintStep {
        let dt = i.dt.max(0.0);
        if !i.held {
            self.spent_out = false;
        }
        let mut step = SprintStep::default();
        let blocked = self.blocked(&i);
        let mut sprinting = blocked.is_none();
        let mut ended = blocked;
        let mut ap_after = i.ap;
        if sprinting {
            if let Some(ap) = i.ap {
                step.spend = (i.ap_per_second.max(0.0) * dt).min(ap.now.max(0.0));
                let mut left = ap.now - step.spend;
                if left <= 1e-4 && i.ap_per_second > 0.0 {
                    // Spent this frame's last (a rounding's crumb with it): the sprint ends now
                    // and waits for a new press.
                    step.spend = ap.now.max(0.0);
                    left = 0.0;
                    self.spent_out = true;
                    sprinting = false;
                    ended = Some(Blocked::OutOfAp);
                }
                ap_after = Some(Ap { now: left, max: ap.max });
            }
        }
        if sprinting && !self.sprinting {
            step.started = true;
            self.seconds = 0.0;
            self.spent = 0.0;
            self.count += 1;
        }
        if sprinting || step.spend > 0.0 {
            self.seconds += dt;
            self.spent += step.spend;
        }
        if self.sprinting && !sprinting {
            step.ended = ended;
            // Stopping low on AP leaves the player out of breath.
            let strength = ap_after.map_or(0.0, winded_strength);
            if strength > 0.0 {
                self.winded.start(strength);
                step.winded = Some(strength);
            }
        }
        self.sprinting = sprinting;
        // The heartbeat: while sprinting and after it until the AP is back over the slower
        // beat's share; none for AP spent without a sprint, or with the switch off.
        self.from_sprint |= sprinting;
        let pace = if i.on { ap_after.map_or(HeartPace::Off, heart_pace) } else { HeartPace::Off };
        if !sprinting && pace == HeartPace::Off {
            self.from_sprint = false;
        }
        self.heart = if self.from_sprint { pace } else { HeartPace::Off };
        let target = if sprinting { direction_share(i.forward, i.right) } else { 0.0 };
        self.share.step(target, omega_for(SHARE_SECONDS), dt);
        self.share.snap(target, 1e-4, 1e-3);
        self.winded.step(dt);
        step
    }

    /// The sprint's share now (0–1).
    pub fn share(&self) -> f32 {
        self.share.value.clamp(0.0, 1.0)
    }

    /// How out of breath now (0–1).
    pub fn winded(&self) -> f32 {
        self.winded.value()
    }

    /// The heartbeat the sprint asks for now (`crate::heartbeat` plays it).
    pub fn heart(&self) -> HeartPace {
        self.heart
    }
}

/// The player's AP, through the script runtime (`None` without one).
fn player_ap(script: Option<&mut crate::script::ScriptState>, sim: Option<&mut crate::sim::SimState>) -> Option<Ap> {
    use fnv_game::actor_values::ACTION_POINTS;
    let (rt, sim) = (script?.rt.as_mut()?, sim?);
    crate::script::with_world(rt, sim, |rt| {
        let p = fnv_sim::FormId::PLAYER_REF;
        let [base, modifier, damage] = fnv_game::functions::actor_value_parts(&mut rt.game, p, ACTION_POINTS).ok()?;
        let max = base + modifier + fnv_game::functions::actor_value_temporary(&rt.game, p, ACTION_POINTS);
        Some(Ap { now: (max - damage) as f32, max: max as f32 })
    })
}

/// Spend `amount` of the player's AP, as the game's AP costs are spent.
fn spend_ap(script: Option<&mut crate::script::ScriptState>, sim: Option<&mut crate::sim::SimState>, amount: f32) {
    use fnv_game::actor_values::ACTION_POINTS;
    let (Some(rt), Some(sim)) = (script.and_then(|s| s.rt.as_mut()), sim) else { return };
    crate::script::with_world(rt, sim, |rt| {
        let p = fnv_sim::FormId::PLAYER_REF;
        // Never more than there is: AP never goes under 0 (issue #2601).
        let now = fnv_game::functions::actor_value(&mut rt.game, p, ACTION_POINTS).unwrap_or(0.0);
        let amount = f64::from(amount).min(now.max(0.0));
        if amount > 0.0 {
            fnv_game::functions::add_actor_value_damage(&mut rt.game, p, ACTION_POINTS, amount);
        }
    });
}

/// The frame's sprint (after the keys and the walk rules, before the physics steps):
/// [`StanceMotion::sprinting`], [`StanceMotion::sprint_share`] and
/// [`StanceMotion::winded`], and the AP spent.
#[allow(clippy::too_many_arguments)]
pub fn update(
    time: Res<Time>,
    inertia: Option<Res<MovementInertia>>,
    control: Res<PlayerControl>,
    intent: Res<MoveIntent>,
    (phys, combat): (Option<Res<crate::phys::PhysState>>, Option<Res<crate::combat::Combat>>),
    toddler: Option<Res<crate::player::PlayerToddler>>,
    (mut sprint, mut stance, tuning): (ResMut<Sprint>, ResMut<StanceMotion>, Res<SprintTuning>),
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut sim: Option<ResMut<crate::sim::SimState>>,
) {
    let on = inertia.as_deref().is_some_and(MovementInertia::sprint_on);
    // At rest with the switch off and nothing left to ease out: nothing to read.
    if !on && !sprint.sprinting && sprint.share() == 0.0 && sprint.winded() == 0.0 && sprint.heart() == HeartPace::Off {
        if (stance.sprinting, stance.sprint_share, stance.winded) != (false, 0.0, 0.0) {
            (stance.sprinting, stance.sprint_share, stance.winded) = (false, 0.0, 0.0);
        }
        return;
    }
    let god = crate::combat::god_mode();
    // Read while the control is held, a sprint runs (its end measures out of breath) or its
    // heartbeat may still beat.
    let ap = if god || !(control.sprint_held || sprint.sprinting || sprint.from_sprint) { None } else { player_ap(script.as_deref_mut(), sim.as_deref_mut()) };
    let in_water = phys.as_deref().and_then(|p| p.player.as_ref().map(|c| (p, c))).is_some_and(|(p, c)| c.swimming || p.world.water_surface_at(c.feet[0], c.feet[1]).is_some_and(|w| w - c.feet[2] > WATER_DEPTH));
    let step = sprint.update(SprintInput {
        dt: time.delta_secs(),
        on,
        held: control.sprint_held,
        forward: intent.forward,
        right: intent.right,
        sneaking: control.sneak || intent.sneak,
        aiming: control.aiming && control.weapon_out,
        attacking: control.trigger,
        reloading: control.reload || combat.as_deref().is_some_and(crate::combat::Combat::player_reloading),
        encumbered: crate::player::OVERENCUMBERED.load(std::sync::atomic::Ordering::Relaxed),
        in_water,
        toddler: toddler.as_deref().is_some_and(|t| t.active),
        ap,
        ap_per_second: tuning.ap_per_second,
    });
    if step.spend > 0.0 {
        spend_ap(script.as_deref_mut(), sim.as_deref_mut(), step.spend);
    }
    let ap_text = |a: Option<Ap>| a.map_or("AP unlimited".to_string(), |a| format!("{:.0} of {:.0} AP", a.now.max(0.0), a.max));
    if step.started {
        info!("sprint: started, {} ({:.2} x the run, {:.1} AP a second)", ap_text(ap), tuning.speed, tuning.ap_per_second);
    }
    if let Some(why) = step.ended {
        let left = ap.map(|a| Ap { now: a.now - step.spend, max: a.max });
        info!("sprint: ended ({}) after {:.1} s, {:.1} AP spent, {} left", why.text(), sprint.seconds, sprint.spent, ap_text(left));
    }
    if let Some(w) = step.winded {
        info!("sprint: out of breath, strength {w:.2}");
    }
    stance.sprinting = sprint.sprinting;
    stance.sprint_share = sprint.share();
    stance.winded = sprint.winded();
}

/// The sprint and its values, between the frame's walk rules and the physics steps.
pub struct SprintPlugin;

impl Plugin for SprintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sprint>()
            .init_resource::<StanceMotion>()
            .insert_resource(SprintTuning::from_env())
            .add_systems(Update, update.after(crate::control::detach_camera).after(crate::PlaySet::Input).before(crate::PlaySet::Physics));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(s: &mut Sprint, i: SprintInput, seconds: f32) -> Vec<SprintStep> {
        (0..(seconds / DT).round() as usize).map(|_| s.update(SprintInput { dt: DT, ..i })).collect()
    }

    fn held(ap: Option<Ap>) -> SprintInput {
        SprintInput { dt: DT, held: true, forward: 1.0, ap, ..SprintInput::default() }
    }

    /// The AP the drain leaves after `seconds` held from `ap`, spending as the system does.
    fn drain(max: f32, seconds: f32) -> (f32, f32) {
        let mut s = Sprint::default();
        let mut ap = Ap { now: max, max };
        let mut sprinted = 0.0;
        for _ in 0..(seconds / DT).round() as usize {
            let step = s.update(held(Some(ap)));
            ap.now -= step.spend;
            assert!(ap.now >= 0.0, "AP never under 0: {}", ap.now);
            if s.sprinting {
                sprinted += DT;
            }
        }
        (ap.now, sprinted)
    }

    #[test]
    fn sprint_costs_a_fixed_ap_a_second_so_more_ap_sprints_longer() {
        // Two seconds cost twenty AP whatever the most.
        for max in [60.0, 100.0, 200.0] {
            let (left, _) = drain(max, 2.0);
            assert!((max - left - 2.0 * SPRINT_AP_PER_SECOND).abs() < 0.05, "{max}: {left}");
        }
        // Held until it runs out: as long as the AP lasts at 10 a second.
        for (max, secs) in [(60.0, 6.0), (100.0, 10.0), (150.0, 15.0)] {
            let (left, sprinted) = drain(max, secs + 3.0);
            assert_eq!(left, 0.0, "{max}: spent to the last point");
            assert!((sprinted - secs).abs() < 2.0 * DT, "{max} AP sprints {sprinted} s, not {secs}");
        }
    }

    #[test]
    fn at_no_ap_the_sprint_ends_and_waits_for_a_new_press() {
        let mut s = Sprint::default();
        let empty = Some(Ap { now: 0.0, max: 80.0 });
        let steps = run(&mut s, held(empty), 1.0);
        assert!(steps.iter().all(|st| !st.started && st.spend == 0.0), "cannot start at 0");
        // It ran out while held; the AP comes back a little: still none until let go.
        let mut s = Sprint::default();
        let mut ap = Ap { now: 1.0, max: 80.0 };
        let mut ended = None;
        for _ in 0..30 {
            let st = s.update(held(Some(ap)));
            ap.now -= st.spend;
            ended = ended.or(st.ended);
        }
        assert!(!s.sprinting && ap.now == 0.0 && ended == Some(Blocked::OutOfAp));
        let back = Some(Ap { now: 5.0, max: 80.0 });
        assert!(run(&mut s, held(back), 0.5).iter().all(|st| !st.started));
        s.update(SprintInput { held: false, ..held(back) });
        assert!(run(&mut s, held(back), 0.1)[0].started, "a new press sprints again");
    }

    #[test]
    fn what_stops_a_sprint_stops_it_and_the_switch_off_is_the_original_speed() {
        let ap = Some(Ap { now: 80.0, max: 80.0 });
        let stops = [
            (SprintInput { on: false, ..held(ap) }, Blocked::SwitchedOff),
            (SprintInput { held: false, ..held(ap) }, Blocked::KeyLetGo),
            (SprintInput { aiming: true, ..held(ap) }, Blocked::Aiming),
            (SprintInput { attacking: true, ..held(ap) }, Blocked::Attacking),
            (SprintInput { reloading: true, ..held(ap) }, Blocked::Reloading),
            (SprintInput { sneaking: true, ..held(ap) }, Blocked::Sneaking),
            (SprintInput { encumbered: true, ..held(ap) }, Blocked::Encumbered),
            (SprintInput { in_water: true, ..held(ap) }, Blocked::InWater),
            (SprintInput { toddler: true, ..held(ap) }, Blocked::Toddler),
            (SprintInput { forward: -1.0, ..held(ap) }, Blocked::NotAhead),
            (SprintInput { forward: 0.0, right: 1.0, ..held(ap) }, Blocked::NotAhead),
        ];
        for (i, why) in stops {
            let mut s = Sprint::default();
            run(&mut s, held(ap), 1.0);
            assert!(s.sprinting && s.share() > 0.99);
            let steps = run(&mut s, i, 2.0);
            assert_eq!(steps[0].ended, Some(why));
            assert!(!s.sprinting && s.share() == 0.0, "{why:?}: eased out to none");
            assert!(steps.iter().all(|st| st.spend == 0.0), "{why:?}: nothing spent");
            assert_eq!(speed_factor(s.share(), SPRINT_SPEED), 1.0, "{why:?}: the original speed");
        }
        // Never switched on: the speed is the original's from the first frame.
        let mut s = Sprint::default();
        for st in run(&mut s, SprintInput { on: false, ..held(ap) }, 1.0) {
            assert!(!st.started && st.spend == 0.0 && s.share() == 0.0);
        }
        // A jump keeps it: nothing in the input says airborne.
        assert_eq!(speed_factor(1.0, SPRINT_SPEED), SPRINT_SPEED);
    }

    #[test]
    fn the_share_eases_in_and_out_monotonically_and_ends_at_rest() {
        let mut s = Sprint::default();
        let ap = Some(Ap { now: 500.0, max: 500.0 });
        let up: Vec<f32> = (0..90).map(|_| { s.update(held(ap)); s.share() }).collect();
        assert!(up.windows(2).all(|w| w[1] >= w[0]), "up only");
        // A soft start: a plain exponential of the same rate moves 0.15 in the first frame.
        assert!(up[0] < 0.02, "a soft start: {}", up[0]);
        let at = (SHARE_SECONDS / DT).round() as usize - 1;
        assert!((up[at] - 0.95).abs() < 0.02, "95% in {SHARE_SECONDS} s: {}", up[at]);
        assert_eq!(*up.last().unwrap(), 1.0);
        // The last frames move it almost not at all: no stop with speed left.
        assert!(up[at..].windows(2).all(|w| w[1] - w[0] < 0.01));
        let down: Vec<f32> = (0..90).map(|_| { s.update(SprintInput { held: false, ..held(ap) }); s.share() }).collect();
        assert!(down.windows(2).all(|w| w[1] <= w[0]), "down only");
        assert!(1.0 - down[0] < 0.02, "a soft start down too");
        assert_eq!(*down.last().unwrap(), 0.0);
        assert!(down[at..].windows(2).all(|w| w[0] - w[1] < 0.01), "slow to its end");
        // A change of mind part way: no jump of the share.
        let mut s = Sprint::default();
        run(&mut s, held(ap), 0.2);
        let before = s.share();
        s.update(SprintInput { held: false, ..held(ap) });
        assert!((s.share() - before).abs() < 0.05, "{before} then {}", s.share());
        // Diagonally about half a sprint, aside none.
        assert!((direction_share(1.0, 1.0) - 0.51).abs() < 0.02);
        assert_eq!(direction_share(0.0, 1.0), 0.0);
        assert_eq!(direction_share(1.0, 0.0), 1.0);
    }

    #[test]
    fn stopping_low_on_ap_winds_the_player_more_the_lower_and_it_fades() {
        let at = |share: f32| winded_strength(Ap { now: 100.0 * share, max: 100.0 });
        assert_eq!(at(WINDED_BELOW), 0.0);
        assert_eq!(at(0.5), 0.0);
        assert!((at(WINDED_BELOW - 0.001) - WINDED_LEAST).abs() < 0.01);
        assert_eq!(at(0.0), 1.0);
        assert!(at(0.05) > at(0.15));
        // A stop at 25% AP: not winded; at 10%: winded, coming on and fading.
        let stop_at = |ap: f32| {
            let mut s = Sprint::default();
            let a = Some(Ap { now: ap, max: 100.0 });
            run(&mut s, held(a), 0.5);
            let st = s.update(SprintInput { held: false, ..held(a) });
            (s, st)
        };
        let (mut s, st) = stop_at(25.0);
        assert_eq!(st.winded, None);
        run(&mut s, SprintInput::default(), 1.0);
        assert_eq!(s.winded(), 0.0);
        let (mut s, st) = stop_at(10.0);
        let strength = st.winded.expect("winded");
        assert!(strength > WINDED_LEAST && strength < 1.0, "{strength}");
        let trace: Vec<f32> = (0..((WINDED_FADE_SECONDS + 3.0) / DT) as usize)
            .map(|_| {
                s.update(SprintInput { dt: DT, ..SprintInput::default() });
                s.winded()
            })
            .collect();
        let (peak_at, peak) = trace.iter().enumerate().fold((0, 0.0f32), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        assert!(peak > 0.6 * strength && peak <= strength, "it comes on near its strength: {peak} of {strength}");
        assert!(trace[0] < 0.05, "and does not jump on: {}", trace[0]);
        assert!(trace[..peak_at].windows(2).all(|w| w[1] >= w[0]) && trace[peak_at..].windows(2).all(|w| w[1] <= w[0] + 1e-6), "up, then down");
        // Fading: smooth, slow at the end, gone after the fade.
        // At most the onset spring's top speed (ω / e of the strength a second, about 0.03 a
        // frame here): no frame jumps.
        let most = omega_for(WINDED_ONSET_SECONDS) / std::f32::consts::E * strength * DT;
        assert!(trace.windows(2).all(|w| (w[1] - w[0]).abs() <= most * 1.05), "no frame jumps");
        let end = (WINDED_FADE_SECONDS / DT) as usize;
        assert!(trace[end - 30] < 0.1 && trace[end - 30] - trace[end - 1] < 0.03, "slow at the end: {} {}", trace[end - 30], trace[end - 1]);
        assert_eq!(*trace.last().unwrap(), 0.0);
        // Out of AP altogether: fully winded.
        let mut s = Sprint::default();
        let mut ap = Ap { now: 3.0, max: 100.0 };
        let mut strength = None;
        for _ in 0..60 {
            let st = s.update(held(Some(ap)));
            ap.now -= st.spend;
            strength = strength.or(st.winded);
        }
        assert!(strength.is_some_and(|w| (w - 1.0).abs() < 1e-5), "{strength:?}");
    }

    #[test]
    fn a_sprint_is_from_the_run_walking_or_running_and_eases_the_change_of_gait() {
        let (walk, run) = ([0.0, 86.8], [0.0, 344.0]);
        // Running: the run times the speed factor, as before.
        for share in [0.0, 0.3, 1.0] {
            let v = sprint_velocity(run, run, share, SPRINT_SPEED);
            assert!((v[1] - 344.0 * speed_factor(share, SPRINT_SPEED)).abs() < 1e-3);
        }
        // Walking (Always Run off): a full sprint is the run's sprint, not the walk's.
        assert!((sprint_velocity(walk, run, 1.0, SPRINT_SPEED)[1] - 344.0 * SPRINT_SPEED).abs() < 1e-3);
        assert_eq!(sprint_velocity(walk, run, 0.0, SPRINT_SPEED), walk);
        // The change of gait follows the eased share: no step, up only, then down only.
        let ap = Some(Ap { now: 500.0, max: 500.0 });
        let mut s = Sprint::default();
        let mut trace = Vec::new();
        for f in 0..180 {
            s.update(SprintInput { held: f < 90, ..held(ap) });
            trace.push(sprint_velocity(walk, run, s.share(), SPRINT_SPEED)[1]);
        }
        assert!(trace[..90].windows(2).all(|w| w[1] >= w[0]) && trace[90..].windows(2).all(|w| w[1] <= w[0]));
        // At most the share spring's top rate (ω / e a second) of the 412 units/s between: a
        // twentieth of it a frame, never the whole change at once.
        let gap = 344.0 * SPRINT_SPEED - 86.8;
        assert!(trace.windows(2).all(|w| (w[1] - w[0]).abs() < 0.065 * gap), "no step of the speed in a frame");
        assert!((trace[89] - 344.0 * SPRINT_SPEED).abs() < 0.5 && (trace[179] - 86.8).abs() < 0.5);
    }

    #[test]
    fn a_reload_ends_the_sprint_and_held_on_it_comes_back_after() {
        let ap = Some(Ap { now: 500.0, max: 500.0 });
        let mut s = Sprint::default();
        run(&mut s, held(ap), 1.0);
        let during = run(&mut s, SprintInput { reloading: true, ..held(ap) }, 1.5);
        assert_eq!(during[0].ended, Some(Blocked::Reloading));
        assert!(during.iter().all(|st| st.spend == 0.0) && s.share() == 0.0, "eased out, nothing spent");
        let after = run(&mut s, held(ap), 1.0);
        assert!(after[0].started && s.share() > 0.99, "Shift held: the sprint comes back after the reload");
    }

    #[test]
    fn the_heartbeat_follows_the_ap_after_a_sprint_and_never_vats_alone() {
        let at = |share: f32| heart_pace(Ap { now: 100.0 * share, max: 100.0 });
        assert_eq!(at(1.0), HeartPace::Off);
        assert_eq!(at(HEARTBEAT_SLOW_BELOW), HeartPace::Off);
        assert_eq!(at(0.39), HeartPace::Slow);
        assert_eq!(at(HEARTBEAT_FAST_BELOW), HeartPace::Slow);
        assert_eq!(at(0.19), HeartPace::Fast);
        assert_eq!(at(0.0), HeartPace::Fast);
        // AP low from VATS alone, no sprint: no heartbeat, whatever the AP.
        let mut s = Sprint::default();
        run(&mut s, SprintInput { ap: Some(Ap { now: 5.0, max: 100.0 }), ..SprintInput::default() }, 1.0);
        assert_eq!(s.heart(), HeartPace::Off);
        // Sprinting down through the thresholds: off, slow, fast; then recovering after it:
        // fast, slow, off, and off for good once back over 40%.
        let mut s = Sprint::default();
        let mut ap = Ap { now: 50.0, max: 100.0 };
        let mut seen = Vec::new();
        for _ in 0..(4.0 / DT) as usize {
            let st = s.update(held(Some(ap)));
            ap.now -= st.spend;
            if seen.last() != Some(&s.heart()) {
                seen.push(s.heart());
            }
        }
        assert_eq!(seen, [HeartPace::Off, HeartPace::Slow, HeartPace::Fast]);
        let mut seen = Vec::new();
        for f in 0..600 {
            // The AP comes back at 10 a second, Shift let go.
            ap.now = (ap.now + 10.0 * DT).min(100.0);
            s.update(SprintInput { held: false, ..held(Some(ap)) });
            if seen.last() != Some(&s.heart()) {
                seen.push(s.heart());
            }
            if f == 599 {
                assert!(ap.now > 50.0);
            }
        }
        assert_eq!(seen, [HeartPace::Fast, HeartPace::Slow, HeartPace::Off]);
        // A VATS spend after the recovery: still none.
        s.update(SprintInput { held: false, ..held(Some(Ap { now: 10.0, max: 100.0 })) });
        assert_eq!(s.heart(), HeartPace::Off);
        // The switch off ends it at once.
        let mut s = Sprint::default();
        run(&mut s, held(Some(Ap { now: 30.0, max: 100.0 })), 0.2);
        assert_eq!(s.heart(), HeartPace::Slow);
        s.update(SprintInput { on: false, ..held(Some(Ap { now: 30.0, max: 100.0 })) });
        assert_eq!(s.heart(), HeartPace::Off);
    }

    #[test]
    fn a_sprint_spends_only_while_it_runs_and_none_without_the_ap_known() {
        let mut s = Sprint::default();
        let steps = run(&mut s, held(None), 1.0);
        assert!(s.sprinting && steps.iter().all(|st| st.spend == 0.0), "god mode or no runtime: free, unlimited");
        assert_eq!(s.winded(), 0.0);
    }
}
