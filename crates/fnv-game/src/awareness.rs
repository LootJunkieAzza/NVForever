//! Detection states: what an actor does with what it perceives (docs/design/alert.md,
//! facts in docs/spec-notes/actor-behaviour.md §2.2–§2.3).
//!
//! Every actor is in one of four states [documented: GECK wiki Category:Detection]:
//! **Normal**, **Alert** (it has noticed something of an actor it would attack, but has
//! not detected it: it draws its weapon and searches), **Combat** (it detects an actor
//! it would attack) and **Lost** (it was fighting, and every target has gone undetected:
//! it searches like an alert actor). Alert, Combat and Lost are the combat states.
//!
//! What moves an actor between them:
//! - the detection values the engine measures (`crate::detection`), passed in through
//!   [`Game::observe_threat`] for each pair: a hostile above fSneakNoticedMin (−20) and
//!   at or below 0 (noticed: heard moving, glimpsed far away or in the dark) puts an
//!   **alarm** on the actor's alert list at the hostile's position; a hostile above 0
//!   (detected) goes on its **target list**;
//! - detection events (CreateDetectionEvent, impacts and explosions the engine raises,
//!   [`Game::queue_detection_event`]) that the actor hears ([`Game::hear_event`]), and
//!   being hurt by a hostile ([`Game::hurt_by`]): an alarm at where it happened;
//! - [`Game::settle_awareness`], after each round of measurements: alarms and targets
//!   time out, and the state follows the documented transitions.
//!
//! Only actors the detector would attack count ([`Game::would_attack`]): what others do
//! is never a threat.
//!
//! The timers' lengths are not documented and no game setting in FalloutNV.esm names
//! them; [`Timers`] holds NVForever's choices. Combat itself (attacking, cover, fleeing)
//! is not built: an actor in Combat stands facing its target.

use crate::detection::{Band, Detection};
use crate::functions::actor::{actor_value, faction_relation, factions_of};
use crate::functions::{none, yes, Args, Ctx};
use crate::game::Game;
use crate::package::flags;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use std::collections::BTreeMap;

const AGGRESSION: u16 = crate::actor_values::AGGRESSION;

/// An actor's detection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum State {
    #[default]
    Normal,
    Alert,
    Combat,
    Lost,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Normal => "Normal",
            State::Alert => "Alert",
            State::Combat => "Combat",
            State::Lost => "Lost",
        }
    }

    /// Alert, Combat and Lost are combat states [documented: Category:Detection].
    pub fn in_combat(self) -> bool {
        self != State::Normal
    }

    /// The actor searches: Alert and Lost [documented: "Alert and Lost actors search …
    /// if their package allows"].
    pub fn searching(self) -> bool {
        matches!(self, State::Alert | State::Lost)
    }

    /// The Detection topic said on the change from `self` to `to`, named
    /// `<From>To<To>` [documented: GECK wiki Detection Tab]; `None` for changes that have
    /// no topic.
    pub fn topic_to(self, to: State) -> Option<&'static str> {
        Some(match (self, to) {
            (State::Normal, State::Alert) => "NormalToAlert",
            (State::Normal, State::Combat) => "NormalToCombat",
            (State::Alert, State::Combat) => "AlertToCombat",
            (State::Alert, State::Normal) => "AlertToNormal",
            (State::Combat, State::Normal) => "CombatToNormal",
            (State::Combat, State::Lost) => "CombatToLost",
            (State::Lost, State::Combat) => "LostToCombat",
            (State::Lost, State::Normal) => "LostToNormal",
            _ => return None,
        })
    }

    /// The topic said now and then while in this state [documented: Detection Tab,
    /// AlertIdle "while … searching for a target they have never detected", LostIdle for
    /// one they have lost].
    pub fn idle_topic(self) -> Option<&'static str> {
        match self {
            State::Alert => Some("AlertIdle"),
            State::Lost => Some("LostIdle"),
            _ => None,
        }
    }
}

/// CreateDetectionEvent's event types [documented: GECK wiki CreateDetectionEvent].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventType {
    Bullet,
    Beam,
    Explosion,
    Generic,
}

impl EventType {
    pub fn from_code(c: i64) -> EventType {
        match c {
            0 => EventType::Bullet,
            1 => EventType::Beam,
            2 => EventType::Explosion,
            _ => EventType::Generic,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EventType::Bullet => "bullet",
            EventType::Beam => "beam",
            EventType::Explosion => "explosion",
            EventType::Generic => "generic",
        }
    }
}

/// Why an alarm is on an actor's alert list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// A hostile it noticed (above −20, not detected).
    Noticed,
    /// A detection event it heard.
    Event(EventType),
    /// A hostile hurt it.
    Hurt,
    /// A target it was fighting went undetected: where it was last known to be.
    LostTarget,
    /// An ally it saw fighting is fighting this one (assistance, `crate::provocation`).
    Ally,
}

impl Cause {
    pub fn describe(self) -> String {
        match self {
            Cause::Noticed => "noticed".into(),
            Cause::Event(t) => format!("heard a {} event", t.name()),
            Cause::Hurt => "was hurt".into(),
            Cause::LostTarget => "lost sight".into(),
            Cause::Ally => "saw an ally fight".into(),
        }
    }
}

/// An entry of an actor's alert list: something of `owner` happened at `at`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alarm {
    pub owner: FormId,
    /// Game space.
    pub at: [f32; 3],
    /// The detection value it was noticed with (the event's "size").
    pub size: f64,
    pub cause: Cause,
    /// `Game::time` it was raised, last refreshed, and runs out.
    pub raised: f64,
    pub refreshed: f64,
    pub expires: f64,
}

/// An entry of an actor's target list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    /// The band and value of the latest measurement.
    pub band: Band,
    pub value: f64,
    /// Where it was last measured (game space).
    pub at: [f32; 3],
    /// When it was last detected (`Game::time`); `None`: never (put on the list by an
    /// event it made).
    pub detected: Option<f64>,
    /// Since when it has been hidden (at or below −20): the timer that takes it off the
    /// list.
    pub hidden_since: Option<f64>,
}

/// The timers of the detection states, seconds of `Game::time` [engineering choice: the
/// documentation names the timers but not their lengths, and FalloutNV.esm has no game
/// setting for them].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timers {
    /// An alarm runs out this long after it was last raised or refreshed.
    pub alarm: f64,
    /// A target hidden this long leaves the target list.
    pub forget_target: f64,
    /// In Combat, no target detected for this long: Lost.
    pub lost: f64,
    /// AlertIdle and LostIdle are said every this many seconds, a random time between the
    /// two [documented: Detection Tab, fCombatDetectionDialogueIdleMin/MaxElapsedTime;
    /// the defaults are FalloutNV.esm's values, observed 2026-10-11].
    pub idle_bark: (f64, f64),
    /// After an actor says a Detection line of one kind ([`bark_kind`]), neither it nor
    /// a member of its group ([`Game::bark_mates`]) within [`BARK_GROUP_RADIUS`] says one
    /// of the same kind for this many seconds [owner request 2026-10-11: "if an enemy in
    /// a group says a line … all the others don't shout the same line type"; design:
    /// the length is the longer idle-bark wait, fCombatDetectionDialogueIdleMaxElapsedTime,
    /// 15 s in FalloutNV.esm].
    pub bark_cooldown: f64,
}

impl Default for Timers {
    fn default() -> Timers {
        Timers { alarm: 20.0, forget_target: 10.0, lost: 3.0, idle_bark: (5.0, 15.0), bark_cooldown: 15.0 }
    }
}

impl Timers {
    /// The defaults, with the bark timer from the load order's game settings.
    pub fn read(g: &Game) -> Timers {
        let d = Timers::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        let idle_bark = (s("fCombatDetectionDialogueIdleMinElapsedTime", d.idle_bark.0), s("fCombatDetectionDialogueIdleMaxElapsedTime", d.idle_bark.1));
        Timers { idle_bark, bark_cooldown: idle_bark.1.max(idle_bark.0), ..d }
    }
}

/// Members of a group within this many game units of a Detection line's speaker do not
/// say a line of the same kind while its cooldown lasts [design: one exterior cell's
/// width, about the spread of a camp such as the Fiends' around Cook-Cook].
pub const BARK_GROUP_RADIUS: f32 = 4096.0;

/// The kind of a Detection topic, for the group cooldown: the three topics an actor says
/// on starting to fight (NormalToCombat, AlertToCombat, LostToCombat) are one kind, as
/// are the three on giving up (AlertToNormal, CombatToNormal, LostToNormal) and the two
/// idles (AlertIdle, LostIdle); NormalToAlert and CombatToLost are kinds of their own
/// [design: lines of one kind say the same thing].
pub fn bark_kind(topic: &'static str) -> &'static str {
    match topic {
        "NormalToCombat" | "AlertToCombat" | "LostToCombat" => "to combat",
        "AlertToNormal" | "CombatToNormal" | "LostToNormal" => "to normal",
        "AlertIdle" | "LostIdle" => "idle",
        t => t,
    }
}

/// Who of `said` (the lines said within the cooldown) holds back a line of `kind` by an
/// actor standing at `at`: the first that said a line of that kind within
/// [`BARK_GROUP_RADIUS`] of it and is of its group (`mates`, the actor itself included).
pub fn held_back_by(said: &[Said], kind: &str, at: Option<[f32; 3]>, mates: impl Fn(FormId) -> bool) -> Option<FormId> {
    let near = |s: &Said| at.is_none_or(|a| (a[0] - s.at[0]).hypot(a[1] - s.at[1]) <= BARK_GROUP_RADIUS);
    said.iter().find(|s| s.kind == kind && near(s) && mates(s.speaker)).map(|s| s.speaker)
}

/// A Detection line said: when, by whom, where, of what kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Said {
    pub kind: &'static str,
    pub speaker: FormId,
    pub at: [f32; 3],
    pub time: f64,
}

/// One actor's detection state, alert list and target list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Awareness {
    pub state: State,
    /// `Game::time` the state began.
    pub since: f64,
    pub alarms: Vec<Alarm>,
    pub targets: BTreeMap<FormId, Target>,
    /// When the next AlertIdle or LostIdle is due.
    pub next_idle_bark: Option<f64>,
}

/// A change of state, for the engine's log and the tools.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub actor: FormId,
    pub from: State,
    pub to: State,
    pub time: f64,
    /// Whom and what it is about: the target detected, the alarm's owner and cause.
    pub about: Option<FormId>,
    pub cause: Option<Cause>,
    pub at: Option<[f32; 3]>,
    pub value: Option<f64>,
}

impl Awareness {
    /// A measurement of a hostile `target`, at `at`.
    pub fn observe(&mut self, target: FormId, det: &Detection, at: [f32; 3], now: f64, t: &Timers) {
        match det.band {
            Band::Detected => {
                let e = self.targets.entry(target).or_insert(Target { band: det.band, value: det.value, at, detected: None, hidden_since: None });
                *e = Target { band: det.band, value: det.value, at, detected: Some(now), hidden_since: None };
            }
            Band::Noticed => {
                if let Some(e) = self.targets.get_mut(&target) {
                    e.band = det.band;
                    e.value = det.value;
                    e.hidden_since = None;
                }
                // Noticed and not detected: something to search for. An alarm of the
                // same hostile is moved to where it is now [documented: an Alert actor
                // re-centres its search on an enemy that stays within the alert band].
                self.alarm(target, at, det.value, Cause::Noticed, now, t);
            }
            Band::Hidden => {
                if let Some(e) = self.targets.get_mut(&target) {
                    e.band = det.band;
                    e.value = det.value;
                    e.hidden_since.get_or_insert(now);
                }
            }
        }
    }

    /// Put an alarm on the alert list, or refresh the one of the same owner and cause.
    /// An alert actor that hears an event adds its owner to its target list (not
    /// detected) [documented: Category:Detection].
    pub fn alarm(&mut self, owner: FormId, at: [f32; 3], size: f64, cause: Cause, now: f64, t: &Timers) {
        let expires = now + t.alarm;
        match self.alarms.iter_mut().find(|a| a.owner == owner && a.cause == cause) {
            Some(a) => {
                a.at = at;
                a.size = size;
                a.refreshed = now;
                a.expires = expires;
            }
            None => self.alarms.push(Alarm { owner, at, size, cause, raised: now, refreshed: now, expires }),
        }
        if self.state.searching() && matches!(cause, Cause::Event(_)) {
            self.targets.entry(owner).or_insert(Target { band: Band::Hidden, value: size, at, detected: None, hidden_since: Some(now) });
        }
    }

    /// Forget an actor (dead, gone, no longer hostile).
    pub fn forget(&mut self, who: FormId) {
        self.targets.remove(&who);
        self.alarms.retain(|a| a.owner != who);
    }

    /// The target detected now with the highest value: who it fights.
    pub fn combat_target(&self) -> Option<(FormId, &Target)> {
        self.targets.iter().filter(|(_, t)| t.band == Band::Detected).max_by(|a, b| a.1.value.total_cmp(&b.1.value)).map(|(id, t)| (*id, t))
    }

    /// The alarm a searching actor goes to: the latest raised or refreshed.
    pub fn search_alarm(&self) -> Option<&Alarm> {
        self.alarms.iter().max_by(|a, b| a.refreshed.total_cmp(&b.refreshed))
    }

    /// Time alarms and targets out and follow the transitions [documented:
    /// Category:Detection; the timers' lengths: [`Timers`]]. `may_alert`: the actor may
    /// become Alert (its package does not forbid it). Returns the change, if any.
    pub fn settle(&mut self, actor: FormId, now: f64, t: &Timers, may_alert: bool) -> Option<Change> {
        self.alarms.retain(|a| a.expires > now);
        self.targets.retain(|_, x| !x.hidden_since.is_some_and(|h| now - h >= t.forget_target));
        let detected = self.combat_target().map(|(id, x)| (id, x.value, x.at));
        let latest = self.search_alarm().copied();
        let from = self.state;
        let (to, about, cause, at, value) = match (from, detected, latest) {
            // A hostile detected: Combat, from any state.
            (State::Normal | State::Alert | State::Lost, Some((id, v, at)), _) => (State::Combat, Some(id), None, Some(at), Some(v)),
            (State::Normal, None, Some(a)) if may_alert => (State::Alert, Some(a.owner), Some(a.cause), Some(a.at), Some(a.size)),
            (State::Alert, None, None) => (State::Normal, None, None, None, None),
            (State::Combat, None, _) => {
                if self.targets.is_empty() && latest.is_none() {
                    (State::Normal, None, None, None, None)
                } else if self.targets.values().all(|x| x.detected.is_none_or(|d| now - d >= t.lost)) {
                    // Every target undetected long enough: search where each was last
                    // known to be.
                    let lost: Vec<(FormId, Target)> = self.targets.iter().map(|(id, x)| (*id, *x)).collect();
                    for (id, x) in &lost {
                        self.alarm(*id, x.at, x.value, Cause::LostTarget, now, t);
                    }
                    let a = self.search_alarm().copied();
                    (State::Lost, a.map(|a| a.owner), a.map(|a| a.cause), a.map(|a| a.at), a.map(|a| a.size))
                } else {
                    return None;
                }
            }
            // Every target hidden (or gone) and the alert list empty.
            (State::Lost, None, None) if self.targets.values().all(|x| x.band == Band::Hidden) => (State::Normal, None, None, None, None),
            _ => return None,
        };
        if to == from {
            return None;
        }
        self.state = to;
        self.since = now;
        self.next_idle_bark = None;
        if to == State::Normal {
            self.targets.clear();
            self.alarms.clear();
        }
        Some(Change { actor, from, to, time: now, about, cause, at, value })
    }
}

/// A detection event waiting to be heard: the engine measures who hears it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionEvent {
    pub owner: FormId,
    pub at: [f32; 3],
    /// Its sound level, in place of the action sound in the formula.
    pub level: f64,
    pub kind: EventType,
}

/// Every actor's [`Awareness`], the events not yet heard and the changes not yet told.
/// Not part of the saved state: the engine measures again.
#[derive(Debug, Clone, Default)]
pub struct Awarenesses {
    pub actors: BTreeMap<FormId, Awareness>,
    pub events: Vec<DetectionEvent>,
    /// State changes since the engine last took them.
    pub changes: Vec<Change>,
    /// Detection topic lines said: the change's or idle topic, the speaker and the line.
    pub barks: Vec<(&'static str, FormId, Option<FormId>)>,
    /// Detection lines said within the last cooldown, for the group cooldown.
    pub said: Vec<Said>,
    /// Detection lines not said for the group cooldown: the topic, the actor held back
    /// and the one that had said a line of that kind.
    pub held_back: Vec<(&'static str, FormId, FormId)>,
    pub timers: Option<Timers>,
    /// How close each actor wants to be to its target, and how far it may be before it
    /// has to close in again (its weapon's range, its reach), game units: the engine's
    /// combat AI says, the AI that moves actors closes in (`fnv_ai::alert`). Not set: it
    /// holds its ground.
    pub engage: BTreeMap<FormId, (f32, f32)>,
    /// Actors aiming down their sights now (the engine's combat AI says): the AI that
    /// moves actors has them walk, not run (`fnv_ai::alert`).
    pub aiming: std::collections::BTreeSet<FormId>,
    /// (actor, target) pairs StartCombat made hostile whatever the records say, until
    /// StopCombat.
    pub forced: std::collections::BTreeSet<(FormId, FormId)>,
    rng: u64,
}

impl Awarenesses {
    pub fn state(&self, actor: FormId) -> State {
        self.actors.get(&actor).map_or(State::Normal, |a| a.state)
    }

    pub fn get(&self, actor: FormId) -> Option<&Awareness> {
        self.actors.get(&actor)
    }

    /// Whether any actor in a combat state has `who` on its target list or alert list.
    pub fn hunted(&self, who: FormId) -> bool {
        self.actors.values().any(|a| a.state.in_combat() && (a.targets.contains_key(&who) || a.alarms.iter().any(|x| x.owner == who)))
    }

    /// Take the changes since the last call.
    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }

    /// A number in [0, 1) from the module's own sequence (the bark timer), so that the
    /// game's random numbers stay as they were.
    fn unit(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Where a reference is now, game space.
fn position(g: &Game, r: FormId) -> Option<[f32; 3]> {
    g.world.peek(r).map(|s| s.pos).or_else(|| g.world.placed.get(r).map(|p| p.pos))
}

impl Game<'_> {
    fn awareness_timers(&mut self) -> Timers {
        match self.awareness.timers {
            Some(t) => t,
            None => {
                let t = Timers::read(self);
                self.awareness.timers = Some(t);
                t
            }
        }
    }

    /// Whether `actor` would attack `other` on detecting it: what its records say
    /// [documented: GECK wiki GetFactionCombatReaction: "Enemy actors are attacked on sight
    /// by Aggressive and Very Aggressive actors", "Neutrals will be attacked by Very
    /// Aggressive actors", allies and friends are not], the factions combined as
    /// `GetFactionRelation` does; a Frenzied actor attacks anyone [hypothesis]. Neither may
    /// be dead, disabled, unconscious or restrained, and `actor` not the player's
    /// teammate when `other` is the player. Actors with a faction in common, or of the
    /// same base form, never attack each other [engineering choice: without it Very
    /// Aggressive creatures of one kind, neutral to each other, would fight].
    pub fn would_attack(&mut self, actor: FormId, other: FormId) -> bool {
        if actor == other || actor == FormId::PLAYER_REF || !self.world.is_enabled(actor).unwrap_or(false) {
            return false;
        }
        let out = |g: &Game, r: FormId| g.extra.get(&r).is_some_and(|e| e.dead || e.unconscious || e.restrained);
        if out(self, actor) || out(self, other) || self.is_dead(other) {
            return false;
        }
        if self.awareness.forced.contains(&(actor, other)) || self.provocations.holds(actor, other) {
            return true;
        }
        if other == FormId::PLAYER_REF {
            if self.extra.get(&actor).is_some_and(|e| e.teammate) {
                return false;
            }
        } else {
            if !self.world.is_enabled(other).unwrap_or(false) {
                return false;
            }
            let base = |g: &Game, r: FormId| g.world.placed.get(r).map(|p| p.base);
            if base(self, actor).is_some() && base(self, actor) == base(self, other) {
                return false;
            }
            let mine = factions_of(self, actor);
            if !mine.is_empty() && factions_of(self, other).iter().any(|f| mine.contains(f)) {
                return false;
            }
        }
        let relation = faction_relation(self, actor, other);
        let aggression = actor_value(self, actor, AGGRESSION).unwrap_or(0.0) as i64;
        match aggression {
            a if a >= 3 => true,
            2 => relation != 2 && relation != 3,
            1 => relation == 1,
            _ => false,
        }
    }

    /// Whether `actor`'s current package keeps it out of Alert: the package flag "No
    /// Combat Alert" [hypothesis: what the flag's name says; no source describes it].
    fn no_combat_alert(&mut self, actor: FormId) -> bool {
        let data = self.data;
        self.current_package(actor).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| d.has(flags::NO_COMBAT_ALERT))
    }

    /// Whether `actor`'s current package goes on while the actor is in a combat state
    /// (Alert, Combat, Lost): the package flag "Continue During Combat" [hypothesis: what
    /// the flag's name says, no source describes it; 399 of FalloutNV.esm's 4,163
    /// packages set it, among them sniper posts, "stand in place" and "flee the scene"
    /// packages, observed 2026-10-11]. Such an actor keeps carrying its package out
    /// instead of searching or holding its ground (`fnv_ai`); its state, weapon and barks
    /// are as for any other actor.
    pub fn continues_during_combat(&mut self, actor: FormId) -> bool {
        let data = self.data;
        self.current_package(actor).and_then(|p| data.package_defs.get(&p)).is_some_and(|d| d.has(flags::CONTINUE_DURING_COMBAT))
    }

    /// The engine measured `target` from `detector` at `at` (game space): when the
    /// detector would attack the target, the measurement feeds its detection state.
    /// What others do is no threat; a target it no longer would attack is forgotten.
    pub fn observe_threat(&mut self, detector: FormId, target: FormId, det: &Detection, at: [f32; 3]) {
        if !self.would_attack(detector, target) {
            if let Some(a) = self.awareness.actors.get_mut(&detector) {
                a.forget(target);
            }
            return;
        }
        let t = self.awareness_timers();
        let now = self.time;
        // Nothing to keep for a hidden hostile the actor knows nothing of.
        if det.band == Band::Hidden && !self.awareness.actors.contains_key(&detector) {
            return;
        }
        self.awareness.actors.entry(detector).or_default().observe(target, det, at, now, &t);
    }

    /// Raise a detection event for the engine to measure (CreateDetectionEvent, an
    /// impact, an explosion).
    pub fn queue_detection_event(&mut self, ev: DetectionEvent) {
        self.awareness.events.push(ev);
    }

    /// Take the events raised since the last call.
    pub fn take_detection_events(&mut self) -> Vec<DetectionEvent> {
        std::mem::take(&mut self.awareness.events)
    }

    /// `detector` measured `ev` at `det`: one that would attack the event's owner, is not
    /// in Combat and detects it (above −20) is alarmed [documented: Category:Detection,
    /// "Actors ignore the event if they would not act aggressively toward its owner, or if
    /// they are already in Combat"]. Returns whether it was.
    pub fn hear_event(&mut self, detector: FormId, ev: &DetectionEvent, det: &Detection) -> bool {
        if det.band == Band::Hidden || self.awareness.state(detector) == State::Combat || !self.would_attack(detector, ev.owner) {
            return false;
        }
        let t = self.awareness_timers();
        let now = self.time;
        self.awareness.actors.entry(detector).or_default().alarm(ev.owner, ev.at, det.value, Cause::Event(ev.kind), now, &t);
        true
    }

    /// `actor` was hurt by `by`, who stands at `at`: an alarm at the attacker's position
    /// when the actor would attack it [documented: Normal → Alert when a hostile hurts the
    /// detector, an event at the hostile's position]. Returns whether it was alarmed.
    pub fn hurt_by(&mut self, actor: FormId, by: FormId, at: [f32; 3]) -> bool {
        if !self.would_attack(actor, by) {
            return false;
        }
        let t = self.awareness_timers();
        let now = self.time;
        self.awareness.actors.entry(actor).or_default().alarm(by, at, 0.0, Cause::Hurt, now, &t);
        true
    }

    /// After a round of measurements: every actor's alarms and targets time out and its
    /// state changes as documented; the changes are kept for the engine
    /// ([`Awarenesses::take_changes`]) and the Detection topics are said
    /// ([`Game::detection_bark`]). Actors that are gone or dead are dropped.
    pub fn settle_awareness(&mut self) {
        let t = self.awareness_timers();
        let now = self.time;
        let ids: Vec<FormId> = self.awareness.actors.keys().copied().collect();
        for id in ids {
            if !self.present(id) || self.is_dead(id) || self.extra.get(&id).is_some_and(|e| e.dead) {
                self.awareness.actors.remove(&id);
                self.provocations.calm(id);
                continue;
            }
            // Targets and alarm owners that died or went are forgotten.
            let gone: Vec<FormId> = {
                let a = &self.awareness.actors[&id];
                a.targets.keys().copied().chain(a.alarms.iter().map(|x| x.owner)).filter(|&o| o != FormId::PLAYER_REF && (!self.present(o) || self.is_dead(o))).collect()
            };
            let may_alert = !self.no_combat_alert(id);
            let a = self.awareness.actors.get_mut(&id).expect("listed");
            for o in gone {
                a.forget(o);
            }
            let change = a.settle(id, now, &t, may_alert);
            let idle_due = a.state.idle_topic().is_some() && a.next_idle_bark.is_some_and(|due| now >= due);
            let schedule = a.state.idle_topic().is_some() && (a.next_idle_bark.is_none() || idle_due);
            if schedule {
                let wait = t.idle_bark.0 + (t.idle_bark.1 - t.idle_bark.0).max(0.0) * self.awareness.unit();
                self.awareness.actors.get_mut(&id).expect("listed").next_idle_bark = Some(now + wait);
            }
            let a = &self.awareness.actors[&id];
            let (state, about) = (a.state, a.search_alarm().map(|x| x.owner).or(a.combat_target().map(|x| x.0)));
            if let Some(c) = change {
                // Back in Normal: the fights it took up because it was hit are over.
                if c.to == State::Normal {
                    self.provocations.calm(id);
                }
                self.awareness.changes.push(c);
                if let Some(topic) = c.from.topic_to(c.to) {
                    self.detection_bark(id, c.about, topic);
                }
            } else if idle_due {
                if let Some(topic) = state.idle_topic() {
                    self.detection_bark(id, about, topic);
                }
            }
            if state == State::Normal && self.awareness.actors.get(&id).is_some_and(|a| a.alarms.is_empty() && a.targets.is_empty()) {
                self.awareness.actors.remove(&id);
            }
        }
    }

    /// `actor` says its line of the Detection topic `topic` about `about` (the
    /// conditions' target), when the load order has the topic, the actor is not already
    /// speaking and no conversation or menu holds the game [documented: Detection Tab],
    /// and neither it nor a member of its group near it has said a line of the same kind
    /// within the cooldown ([`Timers::bark_cooldown`], [`Game::bark_mates`]).
    pub fn detection_bark(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) {
        if self.menu.is_some() || self.dialogue.is_some() || self.is_talking(actor) {
            return;
        }
        let data = self.data;
        let Some(t) = data.by_edid(topic).filter(|t| data.topic_defs.contains_key(t)) else { return };
        let cooldown = self.awareness_timers().bark_cooldown;
        let now = self.time;
        let kind = bark_kind(topic);
        let at = position(self, actor);
        self.awareness.said.retain(|s| now - s.time < cooldown);
        let said = self.awareness.said.clone();
        if let Some(by) = held_back_by(&said, kind, at, |other| self.bark_mates(actor, other)) {
            self.awareness.held_back.push((topic, actor, by));
            return;
        }
        let line = self.say(actor, about, t, false);
        if line.is_some() {
            self.awareness.said.push(Said { kind, speaker: actor, at: at.unwrap_or_default(), time: now });
        }
        self.awareness.barks.push((topic, actor, line));
    }

    /// Whether two actors are of one group for the Detection lines' cooldown: the same
    /// actor, the same base form, or a faction in common [design: the rule by which
    /// [`Game::would_attack`] keeps them from fighting each other; the game's combat
    /// groups are not built].
    pub fn bark_mates(&self, a: FormId, b: FormId) -> bool {
        if a == b {
            return true;
        }
        let base = |r: FormId| self.world.placed.get(r).map(|p| p.base);
        if base(a).is_some() && base(a) == base(b) {
            return true;
        }
        let mine = factions_of(self, a);
        !mine.is_empty() && factions_of(self, b).iter().any(|f| mine.contains(f))
    }
}

impl Game<'_> {
    /// `actor.StartCombat target`: the actor fights `target` from now on, whatever the
    /// records say of hostility, until StopCombat [documented: catalogue notes, "Forces
    /// the calling actor to start combat with a target actor"]. It counts as detecting the
    /// target at once (Combat); one it cannot see it loses and searches for, as any
    /// other (`Awareness::settle`) [engineering choice]. The target's combat group (the
    /// catalogue's second case) is not built: there are no combat groups yet.
    pub fn start_combat(&mut self, actor: FormId, target: FormId) {
        if actor == FormId::PLAYER_REF || actor == target || self.is_dead(actor) || self.is_dead(target) {
            return;
        }
        self.awareness.forced.insert((actor, target));
        let t = self.awareness_timers();
        let now = self.time;
        let at = position(self, target).unwrap_or_default();
        let a = self.awareness.actors.entry(actor).or_default();
        a.targets.insert(target, Target { band: Band::Detected, value: 100.0, at, detected: Some(now), hidden_since: None });
        if let Some(c) = a.settle(actor, now, &t, true) {
            self.awareness.changes.push(c);
            if let Some(topic) = c.from.topic_to(c.to) {
                self.detection_bark(actor, Some(target), topic);
            }
        }
    }

    /// `actor.StopCombat`: the actor leaves its combat state [documented: catalogue
    /// notes, "Ends the calling actor's combat state so it stops fighting"]; what
    /// StartCombat forced on it is undone. It may start again on what it perceives next.
    pub fn stop_combat(&mut self, actor: FormId) {
        self.awareness.forced.retain(|(a, _)| *a != actor);
        self.provocations.calm(actor);
        if let Some(a) = self.awareness.actors.remove(&actor) {
            if a.state != State::Normal {
                self.awareness.changes.push(Change { actor, from: a.state, to: State::Normal, time: self.time, about: None, cause: None, at: None, value: None });
            }
        }
    }
}

/// `actor.StartCombat target` ([`Game::start_combat`]).
pub fn start_combat(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.start_combat(r, a.form(0)?);
    none()
}

/// `actor.StopCombat` ([`Game::stop_combat`]).
pub fn stop_combat(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.stop_combat(r);
    none()
}

/// `actor.StopCombatAlarmOnActor`: every actor fighting or hunting the calling actor
/// stops ([`Game::stop_combat`]) [documented: GECK wiki StopCombatAlarmOnActor, "Ends the
/// fighting flag for all enemies targeting the specified actor"].
pub fn stop_combat_alarm_on_actor(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let hunters: Vec<FormId> = g.awareness.actors.iter().filter(|(_, a)| a.state.in_combat() && (a.targets.contains_key(&r) || a.alarms.iter().any(|x| x.owner == r))).map(|(id, _)| *id).collect();
    for h in hunters {
        g.stop_combat(h);
    }
    none()
}

/// `actor.IsInCombat`: 1 in a combat state (Alert, Combat, Lost) [documented: GECK wiki
/// Category:Detection, the three are combat states]; the player is in combat while an
/// actor in a combat state has it on its target or alert list [documented: GECK wiki
/// IsInCombat, "will return 1 if enemies are actively searching for the player"].
pub fn is_in_combat(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if r == FormId::PLAYER_REF {
        return yes(g.awareness.hunted(r));
    }
    yes(g.awareness.state(r).in_combat())
}

/// `actor.GetCombatTarget`: the actor it fights, the detected target with the highest
/// value [engineering choice: no source says how the original picks among several]; 0
/// outside Combat.
pub fn get_combat_target(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let a = g.awareness.get(r).filter(|a| a.state == State::Combat);
    Ok(a.and_then(|a| a.combat_target()).map(|(t, _)| Value::Ref(t.0)).unwrap_or(Value::NONE))
}

/// `location.CreateDetectionEvent owner level [type]`: a detection event at the calling
/// reference [documented: GECK wiki CreateDetectionEvent; the type 0 bullet, 1 beam, 2
/// explosion, 3 generic; generic when not given: hypothesis]. Who hears it is the
/// engine's to measure.
pub fn create_detection_event(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let at_ref = ctx.subject()?;
    let owner = a.form(0)?;
    let level = a.num_or(1, 0.0).max(0.0);
    let kind = EventType::from_code(a.num_or(2, 3.0) as i64);
    let at = if at_ref == FormId::PLAYER_REF { g.player_pos() } else { position(g, at_ref) };
    if let Some(at) = at {
        g.queue_detection_event(DetectionEvent { owner, at, level, kind });
    }
    none()
}

/// `actor.SetAlert flag`: the alerted animation state (weapon drawn), kept with the
/// reference [documented: GECK wiki SetAlert, "purely an animation state", savebaked].
pub fn set_alert(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().alerted = a.num_or(0, 0.0) != 0.0;
    none()
}

/// `actor.GetIsAlerted`: 1 in the alerted animation state [documented: catalogue notes]:
/// set with SetAlert, or while the actor is in a combat state [hypothesis: Alert is
/// "weapon out", Category:Detection, which is the alerted animation state].
pub fn get_is_alerted(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.is_alerted(r))
}

impl Game<'_> {
    /// Whether `actor` has its weapon out for alertness: SetAlert, or a combat state
    /// ([`get_is_alerted`]).
    pub fn is_alerted(&self, actor: FormId) -> bool {
        self.extra.get(&actor).is_some_and(|e| e.alerted) || self.awareness.state(actor).in_combat()
    }

    fn player_pos(&self) -> Option<[f32; 3]> {
        position(self, FormId::PLAYER_REF)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: FormId = FormId(0x100);
    const FOE: FormId = FormId(0x200);
    const FOE2: FormId = FormId(0x300);

    fn det(value: f64) -> Detection {
        let band = if value > 0.0 {
            Band::Detected
        } else if value > -20.0 {
            Band::Noticed
        } else {
            Band::Hidden
        };
        Detection { value, band, ..Default::default() }
    }

    fn step(a: &mut Awareness, now: f64) -> Option<(State, State)> {
        a.settle(ME, now, &Timers::default(), true).map(|c| (c.from, c.to))
    }

    #[test]
    fn a_group_says_one_detection_line_of_a_kind_at_a_time() {
        let (a, b, c, stranger) = (FormId(0xA), FormId(0xB), FormId(0xC), FormId(0xD));
        let group = |x: FormId| x != stranger;
        // A fiend said its "to combat" line here 3 s ago.
        let said = [Said { kind: bark_kind("NormalToCombat"), speaker: a, at: [0.0, 0.0, 0.0], time: 10.0 }];
        // Its mates nearby do not say one of that kind (AlertToCombat and LostToCombat
        // are the same kind), nor does it itself.
        assert_eq!(held_back_by(&said, bark_kind("AlertToCombat"), Some([500.0, 0.0, 0.0]), group), Some(a));
        assert_eq!(held_back_by(&said, bark_kind("LostToCombat"), Some([0.0, 3000.0, 0.0]), group), Some(a));
        assert_eq!(held_back_by(&said, bark_kind("NormalToCombat"), Some([0.0, 0.0, 0.0]), |x| x == a), Some(a));
        // Another kind, an actor of another group, a mate far away: they speak.
        assert_eq!(held_back_by(&said, bark_kind("CombatToLost"), Some([500.0, 0.0, 0.0]), group), None);
        assert_eq!(held_back_by(&said, bark_kind("NormalToCombat"), Some([500.0, 0.0, 0.0]), |x| x == b || x == c), None);
        assert_eq!(held_back_by(&said, bark_kind("NormalToCombat"), Some([BARK_GROUP_RADIUS + 1.0, 0.0, 0.0]), group), None);
        assert_eq!((bark_kind("AlertIdle"), bark_kind("LostIdle"), bark_kind("NormalToAlert")), ("idle", "idle", "NormalToAlert"));
        // The cooldown is the longer idle-bark wait.
        assert_eq!(Timers::default().bark_cooldown, Timers::default().idle_bark.1);
    }

    #[test]
    fn a_noticed_hostile_alerts_and_the_alert_runs_out() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(-10.0), [100.0, 0.0, 0.0], 0.0, &t);
        assert_eq!(step(&mut a, 0.0), Some((State::Normal, State::Alert)));
        assert_eq!(a.search_alarm().map(|x| x.at), Some([100.0, 0.0, 0.0]));
        // Still noticed: the alarm moves with it and stays.
        a.observe(FOE, &det(-12.0), [150.0, 0.0, 0.0], 15.0, &t);
        assert_eq!(step(&mut a, 15.0), None);
        assert_eq!(a.search_alarm().map(|x| x.at), Some([150.0, 0.0, 0.0]));
        // Not noticed again: the alarm runs out, back to Normal.
        a.observe(FOE, &det(-40.0), [150.0, 0.0, 0.0], 20.0, &t);
        assert_eq!(step(&mut a, 34.9), None);
        assert_eq!(step(&mut a, 35.0), Some((State::Alert, State::Normal)));
        assert!(a.alarms.is_empty() && a.targets.is_empty());
    }

    #[test]
    fn a_hidden_hostile_does_nothing() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(-30.0), [0.0; 3], 0.0, &t);
        assert_eq!(step(&mut a, 0.0), None);
        assert_eq!(a.state, State::Normal);
    }

    #[test]
    fn detected_goes_to_combat_from_normal_and_from_alert() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(5.0), [0.0; 3], 0.0, &t);
        assert_eq!(step(&mut a, 0.0), Some((State::Normal, State::Combat)));
        let mut b = Awareness::default();
        b.observe(FOE, &det(-5.0), [0.0; 3], 0.0, &t);
        step(&mut b, 0.0);
        b.observe(FOE, &det(3.0), [0.0; 3], 1.0, &t);
        assert_eq!(step(&mut b, 1.0), Some((State::Alert, State::Combat)));
        assert_eq!(b.combat_target().map(|x| x.0), Some(FOE));
    }

    #[test]
    fn combat_loses_its_target_searches_and_gives_up() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(10.0), [10.0, 0.0, 0.0], 0.0, &t);
        step(&mut a, 0.0);
        // Undetected (noticed) for less than the lost timer: still Combat.
        a.observe(FOE, &det(-5.0), [20.0, 0.0, 0.0], 1.0, &t);
        assert_eq!(step(&mut a, 2.0), None);
        // Long enough: Lost, searching where it was last seen.
        assert_eq!(step(&mut a, 3.0), Some((State::Combat, State::Lost)));
        assert!(a.alarms.iter().any(|x| x.cause == Cause::LostTarget && x.owner == FOE));
        // Seen again: Combat.
        a.observe(FOE, &det(1.0), [30.0, 0.0, 0.0], 4.0, &t);
        assert_eq!(step(&mut a, 4.0), Some((State::Lost, State::Combat)));
        // Hidden: Lost after the lost timer, then the target is forgotten and the
        // alarms run out: Normal.
        a.observe(FOE, &det(-50.0), [30.0, 0.0, 0.0], 5.0, &t);
        assert_eq!(step(&mut a, 7.0), Some((State::Combat, State::Lost)));
        assert_eq!(step(&mut a, 15.0), None, "the target is still on the list");
        assert_eq!(step(&mut a, 27.0), Some((State::Lost, State::Normal)));
    }

    #[test]
    fn combat_ends_when_no_target_is_left() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(10.0), [0.0; 3], 0.0, &t);
        step(&mut a, 0.0);
        a.forget(FOE);
        assert_eq!(step(&mut a, 0.5), Some((State::Combat, State::Normal)));
    }

    #[test]
    fn an_event_heard_while_alert_puts_its_owner_on_the_target_list() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.alarm(FOE, [0.0; 3], -10.0, Cause::Event(EventType::Bullet), 0.0, &t);
        assert!(a.targets.is_empty(), "not yet alert");
        assert_eq!(step(&mut a, 0.0), Some((State::Normal, State::Alert)));
        a.alarm(FOE2, [5.0, 0.0, 0.0], -10.0, Cause::Event(EventType::Generic), 1.0, &t);
        assert!(a.targets.contains_key(&FOE2) && a.targets[&FOE2].detected.is_none());
        assert_eq!(a.search_alarm().map(|x| x.owner), Some(FOE2), "the latest alarm is searched");
    }

    #[test]
    fn no_combat_alert_keeps_the_actor_normal_but_not_out_of_combat() {
        let t = Timers::default();
        let mut a = Awareness::default();
        a.observe(FOE, &det(-10.0), [0.0; 3], 0.0, &t);
        assert_eq!(a.settle(ME, 0.0, &t, false), None);
        a.observe(FOE, &det(10.0), [0.0; 3], 1.0, &t);
        assert_eq!(a.settle(ME, 1.0, &t, false).map(|c| c.to), Some(State::Combat));
    }

    #[test]
    fn the_topics_are_named_from_and_to() {
        assert_eq!(State::Normal.topic_to(State::Alert), Some("NormalToAlert"));
        assert_eq!(State::Combat.topic_to(State::Lost), Some("CombatToLost"));
        assert_eq!(State::Alert.topic_to(State::Lost), None);
        assert_eq!(State::Lost.idle_topic(), Some("LostIdle"));
        assert!(State::Lost.in_combat() && !State::Normal.in_combat() && !State::Combat.searching());
    }
}
