//! The attack cycle of one wielded weapon (docs/design/combat.md §5.5): trigger, fire
//! rate, magazine, reload, regenerating ammo. Headless; the engine feeds input and clip
//! timing and acts on the events.
//!
//! Rate of fire [documented: GECK wiki Weapons]: automatic weapons fire `fire_rate` shots
//! per second while the trigger is held; a semi-automatic weapon's rate follows its attack
//! clip and attack multiplier, so the interval is the attack clip's length ÷ (attack
//! multiplier × animation multiplier) when the engine knows the clip, else
//! 1 ÷ "Attack Shots/Sec". The shot leaves at the clip's `Hit` key [documented: GECK wiki
//! NiTextKeyExtraData], `fire_delay` after the trigger.
//!
//! Spin-up [documented: GECK wiki NiTextKeyExtraData, `Fire` "works like `Hit` and also
//! starts the internal loop of a clamped sequence; `Loop` jumps back to it"; owner
//! decision 2026-10-10]: an automatic weapon whose attack clip has that loop (a minigun's
//! `AttackSpin`) fires its first shot at the clip's `Fire` key, `spin_up` after the
//! trigger, then at its rate for as long as the trigger stays held. Letting go during
//! the spin-up fires nothing; every new pull, and firing on after a reload, spins up
//! again [design].

use serde::Serialize;

use crate::data::WeaponStats;
use crate::tuning::CombatTuning;

/// Clip timing the engine reads from the weapon's attack and reload clips. All optional.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ClipTiming {
    /// Length of the attack clip, seconds.
    pub attack_len: Option<f32>,
    /// Time of the attack clip's `Hit` key, seconds.
    pub hit_time: Option<f32>,
    /// Length of the reload clip, seconds.
    pub reload_len: Option<f32>,
    /// The attack clip has an internal loop (`Fire` and `Loop` keys): seconds from its
    /// start to the `Fire` key.
    pub spin_up: Option<f32>,
}

/// The intervals the cycle runs on.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Timing {
    /// Seconds between shots.
    pub interval: f32,
    /// Seconds from the trigger to the shot.
    pub fire_delay: f32,
    pub reload: f32,
    /// Seconds from a new pull of the trigger to the first shot of an automatic weapon
    /// that has to spin up; 0 for the others.
    pub spin_up: f32,
}

impl Timing {
    pub fn of(w: &WeaponStats, clips: &ClipTiming, t: &CombatTuning) -> Timing {
        let speed = (w.attack_mult.max(0.01)) * (w.anim_mult.max(0.01));
        let interval = if w.automatic && w.fire_rate > 0.0 {
            1.0 / w.fire_rate
        } else if let Some(len) = clips.attack_len {
            len / speed
        } else if w.attack_shots_per_sec > 0.0 {
            1.0 / w.attack_shots_per_sec
        } else {
            1.0
        };
        let fire_delay = if w.automatic { 0.0 } else { clips.hit_time.map_or(0.0, |h| h / speed).min(interval) };
        // The animation multiplier does not speed up reloads [documented: GECK wiki
        // Weapons, Animation section].
        let reload = clips.reload_len.unwrap_or(t.fallback_reload);
        // In clip seconds: the body plays the attack clip at its own speed, and the
        // shot should leave when the clip reaches its key.
        let spin_up = if w.automatic { clips.spin_up.unwrap_or(0.0).max(0.0) } else { 0.0 };
        Timing { interval, fire_delay, reload, spin_up }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Phase {
    Ready,
    /// The trigger was pulled on a weapon that has to spin up; the first shot leaves
    /// when `left` reaches 0 if the trigger is still held.
    SpinUp { left: f32 },
    /// The trigger was pulled; the shot leaves when `left` reaches 0.
    WindUp { left: f32 },
    /// Between shots.
    Cooldown { left: f32 },
    Reloading { left: f32 },
}

/// One tick's input.
#[derive(Debug, Clone, Copy, Default)]
pub struct Input {
    /// Trigger held this tick.
    pub trigger: bool,
    /// Reload requested this tick.
    pub reload: bool,
    pub dt: f32,
    /// Rounds of the loaded ammo carried outside the magazine.
    pub carried: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Event {
    /// The weapon started to spin up: the first shot follows in `Timing::spin_up`.
    SpinUp,
    /// A shot left the weapon: `rounds` taken from the magazine.
    Fired { rounds: u32 },
    /// Trigger on an empty magazine with nothing to reload.
    DryFire,
    ReloadStarted,
    /// Reload finished: `taken` rounds move from the inventory to the magazine.
    Reloaded { taken: u32 },
    /// A regenerating weapon gained a round.
    Regenerated,
}

/// The cycle of one wielded weapon.
#[derive(Debug, Clone, Serialize)]
pub struct Cycle {
    pub phase: Phase,
    pub magazine: u32,
    pub clip_size: u32,
    pub ammo_use: u32,
    pub automatic: bool,
    /// Uses no ammo item (melee, unarmed; a thrown weapon spends itself, which the
    /// caller handles).
    pub no_ammo: bool,
    /// Seconds per regenerated round; regenerating weapons cannot be reloaded by hand
    /// [documented: GECK wiki Weapons, Regen Rate].
    pub regen: Option<f32>,
    regen_acc: f32,
    /// The trigger was held last tick (semi-automatic weapons need a fresh pull).
    held: bool,
    /// Spun up: shots have left since the trigger was last pulled.
    spun: bool,
    pub timing: Timing,
}

impl Cycle {
    pub fn new(w: &WeaponStats, timing: Timing, no_ammo: bool, magazine: u32) -> Cycle {
        let clip_size = w.clip_size.max(1);
        Cycle {
            phase: Phase::Ready,
            magazine: magazine.min(clip_size),
            clip_size,
            ammo_use: w.ammo_use.max(1),
            automatic: w.automatic,
            no_ammo,
            regen: (w.regen_rate > 0.0).then_some(w.regen_rate),
            regen_acc: 0.0,
            held: false,
            spun: false,
            timing,
        }
    }

    fn has_round(&self) -> bool {
        self.no_ammo || self.magazine >= self.ammo_use
    }

    fn can_reload(&self, carried: u32) -> bool {
        !self.no_ammo && self.regen.is_none() && self.magazine < self.clip_size && carried > 0
    }

    pub fn step(&mut self, t: &CombatTuning, i: Input) -> Vec<Event> {
        let mut out = Vec::new();
        let fresh = i.trigger && !self.held;
        let pulled = fresh || (i.trigger && self.automatic);
        self.held = i.trigger;
        if !i.trigger {
            self.spun = false;
        }
        if let Some(r) = self.regen {
            if self.magazine < self.clip_size {
                self.regen_acc += i.dt;
                while self.regen_acc >= r && self.magazine < self.clip_size {
                    self.regen_acc -= r;
                    self.magazine += 1;
                    out.push(Event::Regenerated);
                }
            } else {
                self.regen_acc = 0.0;
            }
        }
        let mut dt = i.dt;
        // Up to a few phase changes per tick, so short intervals at long ticks still fire.
        for _ in 0..8 {
            match self.phase {
                Phase::Ready => {
                    if i.reload && self.can_reload(i.carried) {
                        self.phase = Phase::Reloading { left: self.timing.reload };
                        self.spun = false;
                        out.push(Event::ReloadStarted);
                    } else if pulled {
                        if self.has_round() {
                            if self.timing.spin_up > 0.0 && !self.spun {
                                self.phase = Phase::SpinUp { left: self.timing.spin_up };
                                out.push(Event::SpinUp);
                            } else {
                                self.phase = Phase::WindUp { left: self.timing.fire_delay };
                            }
                        } else if t.auto_reload && self.can_reload(i.carried) {
                            self.phase = Phase::Reloading { left: self.timing.reload };
                            self.spun = false;
                            out.push(Event::ReloadStarted);
                        } else {
                            // One click per pull, also for automatic weapons.
                            if fresh {
                                out.push(Event::DryFire);
                            }
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Phase::SpinUp { left } => {
                    if !i.trigger {
                        // Let go before the first shot: none leaves.
                        self.phase = Phase::Ready;
                        break;
                    }
                    if left > dt {
                        self.phase = Phase::SpinUp { left: left - dt };
                        break;
                    }
                    dt -= left;
                    self.spun = true;
                    self.phase = Phase::WindUp { left: self.timing.fire_delay };
                }
                Phase::WindUp { left } => {
                    if left > dt {
                        self.phase = Phase::WindUp { left: left - dt };
                        break;
                    }
                    dt -= left;
                    let rounds = if self.no_ammo { 0 } else { self.ammo_use };
                    self.magazine -= rounds;
                    out.push(Event::Fired { rounds });
                    self.phase = Phase::Cooldown { left: (self.timing.interval - self.timing.fire_delay).max(0.0) };
                }
                Phase::Cooldown { left } => {
                    if left > dt {
                        self.phase = Phase::Cooldown { left: left - dt };
                        break;
                    }
                    dt -= left;
                    self.phase = Phase::Ready;
                    // An automatic weapon keeps firing while held; a semi-automatic one
                    // waits for the next pull.
                    if !(i.trigger && self.automatic) {
                        break;
                    }
                }
                Phase::Reloading { left } => {
                    if left > dt {
                        self.phase = Phase::Reloading { left: left - dt };
                        break;
                    }
                    let taken = (self.clip_size - self.magazine).min(i.carried);
                    self.magazine += taken;
                    out.push(Event::Reloaded { taken });
                    self.phase = Phase::Ready;
                    break;
                }
            }
        }
        out
    }
}
