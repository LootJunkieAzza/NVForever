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
//!
//! Round by round (docs/spec-notes/combat-rules.md §4): a weapon whose reload animation
//! has an opening clip (`1hpreloadxstart.kf` beside `1hpreloadx.kf`: the .357 revolver,
//! the lever-actions, the hunting shotgun) opens once, then loads one round per pass of
//! the reload clip up to its `Hit` key and closes with the rest of that clip
//! [hypothesis: the `Hit` key "also ends looping reloads", documented: GECK wiki
//! NiTextKeyExtraData; observed: the sounds named after it are the `…ReloadEnd`, click
//! and cock sounds]. A fresh pull of the trigger stops it after the round in hand and
//! the weapon closes; the close plays every time [owner decision 2026-10-11]. The pull
//! that stops the reload only closes the weapon: it fires nothing [owner decision
//! 2026-10-11, playtest: "pressing attack mid reload to close cylinder fires a shot"].
//! A later pull while the reload is stopped or closing is kept for [`SHOT_GRACE`]: if
//! the weapon is closed within that time of the pull, the shot follows; else it is
//! dropped and a new pull is needed.

use serde::Serialize;

use crate::data::WeaponStats;
use crate::tuning::CombatTuning;

/// How long a pull made while a round-by-round reload is stopped or closing waits for
/// the weapon to close, seconds [design: the owner's "grace period", 2026-10-11; chosen].
pub const SHOT_GRACE: f32 = 0.25;

/// Clip timing the engine reads from the weapon's attack and reload clips. All optional.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ClipTiming {
    /// Length of the attack clip, seconds.
    pub attack_len: Option<f32>,
    /// Time of the attack clip's `Hit` key, seconds.
    pub hit_time: Option<f32>,
    /// Length of the reload clip, seconds.
    pub reload_len: Option<f32>,
    /// Time of the reload clip's `Hit` key, seconds.
    pub reload_hit: Option<f32>,
    /// Length of the reload animation's opening clip (`…start.kf`), when it has one.
    pub reload_open_len: Option<f32>,
    /// The attack clip has an internal loop (`Fire` and `Loop` keys): seconds from its
    /// start to the `Fire` key.
    pub spin_up: Option<f32>,
    /// The attack clip's `a:` key: its time and the attack it names next (melee combos,
    /// [`crate::melee`]).
    pub next_attack: Option<(f32, crate::melee::NextAttack)>,
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
    /// A reload that loads round by round; `None`: the whole magazine in one go.
    pub rounds: Option<RoundReload>,
}

/// The parts of a round-by-round reload, seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RoundReload {
    /// Opening the weapon: the opening clip.
    pub open: f32,
    /// One round: the reload clip up to its `Hit` key.
    pub round: f32,
    /// Closing it: the rest of the reload clip.
    pub close: f32,
}

impl RoundReload {
    /// From the clips: an opening clip, and a reload clip with a `Hit` key inside it.
    pub fn of(clips: &ClipTiming) -> Option<RoundReload> {
        let (open, hit, len) = (clips.reload_open_len?, clips.reload_hit?, clips.reload_len?);
        (open >= 0.0 && hit > 0.0 && hit < len).then_some(RoundReload { open, round: hit, close: len - hit })
    }
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
        Timing { interval, fire_delay, reload, spin_up, rounds: RoundReload::of(clips) }
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
    /// Round by round: opening the weapon. `stop`: the trigger was pulled, so only the
    /// first round goes in.
    ReloadOpen { left: f32, stop: bool },
    /// Round by round: one round goes in when `left` reaches 0; `taken` so far.
    ReloadRound { left: f32, stop: bool, taken: u32 },
    /// Round by round: closing the weapon.
    ReloadClose { left: f32, taken: u32 },
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
    /// A reload started: the reload clip, or the opening clip of a round-by-round one.
    ReloadStarted,
    /// Round by round: the pass that loads one round starts (the reload clip from its
    /// start).
    ReloadRound,
    /// Round by round: a round went into the magazine.
    RoundLoaded,
    /// Round by round: the weapon closes (the reload clip plays on past `Hit`);
    /// `stopped`: by the trigger.
    ReloadClosing { stopped: bool },
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
    /// Seconds since a pull made while a round-by-round reload was stopped or closing:
    /// the shot it asks for, kept for [`SHOT_GRACE`].
    queued: Option<f32>,
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
            queued: None,
            timing,
        }
    }

    fn has_round(&self) -> bool {
        self.no_ammo || self.magazine >= self.ammo_use
    }

    fn can_reload(&self, carried: u32) -> bool {
        !self.no_ammo && self.regen.is_none() && self.magazine < self.clip_size && carried > 0
    }

    /// The whole magazine, or round by round when the weapon's clips say so.
    fn start_reload(&mut self) -> Phase {
        self.spun = false;
        match self.timing.rounds {
            Some(r) => Phase::ReloadOpen { left: r.open, stop: false },
            None => Phase::Reloading { left: self.timing.reload },
        }
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
        // A queued shot ages; past the grace it is dropped.
        self.queued = self.queued.map(|q| q + i.dt).filter(|q| *q <= SHOT_GRACE);
        // A fresh pull stops a round-by-round reload, unless that pull started it; once
        // it is stopped, a fresh pull is queued.
        let mut stop_pull = fresh;
        let stopped = matches!(self.phase, Phase::ReloadOpen { stop: true, .. } | Phase::ReloadRound { stop: true, .. } | Phase::ReloadClose { .. });
        if fresh && stopped {
            self.queued = Some(0.0);
            stop_pull = false;
        }
        // Rounds loaded this tick, not yet out of `i.carried`.
        let mut loaded = 0;
        // Up to a few phase changes per tick, so short intervals at long ticks still fire.
        for _ in 0..8 {
            match self.phase {
                Phase::Ready => {
                    if i.reload && self.can_reload(i.carried) {
                        self.phase = self.start_reload();
                        stop_pull = false;
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
                            self.phase = self.start_reload();
                            stop_pull = false;
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
                Phase::ReloadOpen { left, stop } => {
                    let stop = stop || std::mem::take(&mut stop_pull);
                    if left > dt {
                        self.phase = Phase::ReloadOpen { left: left - dt, stop };
                        break;
                    }
                    dt -= left;
                    // The first round always goes in, whatever stops the reload [design].
                    let round = self.timing.rounds.map_or(0.0, |r| r.round);
                    self.phase = Phase::ReloadRound { left: round, stop, taken: 0 };
                    out.push(Event::ReloadRound);
                }
                Phase::ReloadRound { left, stop, taken } => {
                    let stop = stop || std::mem::take(&mut stop_pull);
                    if left > dt {
                        self.phase = Phase::ReloadRound { left: left - dt, stop, taken };
                        break;
                    }
                    dt -= left;
                    let r = self.timing.rounds.unwrap_or(RoundReload { open: 0.0, round: 0.0, close: 0.0 });
                    // The round in hand goes in, if one is still carried.
                    let taken = if i.carried > loaded {
                        self.magazine += 1;
                        loaded += 1;
                        out.push(Event::RoundLoaded);
                        taken + 1
                    } else {
                        taken
                    };
                    if stop || self.magazine >= self.clip_size || i.carried <= loaded {
                        self.phase = Phase::ReloadClose { left: r.close, taken };
                        out.push(Event::ReloadClosing { stopped: stop });
                    } else {
                        self.phase = Phase::ReloadRound { left: r.round, stop: false, taken };
                        out.push(Event::ReloadRound);
                    }
                }
                Phase::ReloadClose { left, taken } => {
                    if left > dt {
                        self.phase = Phase::ReloadClose { left: left - dt, taken };
                        break;
                    }
                    dt -= left;
                    out.push(Event::Reloaded { taken });
                    self.phase = Phase::Ready;
                    // Closed: a shot asked for within the grace follows, without a new
                    // pull; the pull that stopped the reload fires nothing (module docs).
                    if self.queued.take().is_some() && self.has_round() {
                        self.phase = Phase::WindUp { left: self.timing.fire_delay };
                    } else {
                        break;
                    }
                }
            }
        }
        out
    }
}
