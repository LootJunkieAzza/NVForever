//! Unconscious actors (docs/spec-notes/unconscious.md §2): essential actors at zero
//! health and any actor whose Fatigue runs out are knocked out instead of dying, and get
//! up again.
//!
//! - A knockout ([`KnockOut`], `RefExtra::knocked_out`) has a cause and the game time it
//!   began. It is not SetUnconscious's flag (`RefExtra::unconscious`), which only
//!   scripts set and GetUnconscious reads [documented: GECK wiki GetUnconscious].
//! - Essential at zero health ([`Game::knock_out`] from `Game::hit`, the effects'
//!   health damage and KillActor): it keeps 1 health and does not die [documented:
//!   KillActor notes; design: what stays undone].
//! - Fatigue at zero ([`Game::fatigue_changed`], after any fatigue damage) [documented:
//!   GECK wiki Stats Tab - Creatures, "unconscious at zero"].
//! - Each frame ([`Game::tick_knockouts`]) fatigue returns at [`FATIGUE_RETURN`] a
//!   second, and the knocked out get up: fatigue knockouts after [`FATIGUE_DOWN`]
//!   seconds, fight or not, with their Fatigue whole; essential ones once no fight goes
//!   on within [`FIGHT_RADIUS`] of them (and at least [`ESSENTIAL_DOWN`] seconds), with
//!   health back to [`ESSENTIAL_UP_HEALTH`] of the maximum [owner rules 2026-10-17;
//!   design: the numbers, no setting in the data].
//! - **Getting up** is a stage of the knockout ([`KnockOut::rising`]): the actor still
//!   counts as knocked out — it does not move, fight or talk — until the engine has played
//!   its get-up clip ([`Game::finish_getting_up`]), or [`GETUP_MAX`] seconds for an
//!   actor nobody draws [owner rule 2026-10-17: NPCs do not move until their get-up
//!   animation is over].
//! - Each knockout and each getting up is an event ([`EventKind::KnockedOut`],
//!   [`EventKind::GotUp`]) for the engine (corner message, animations).

use crate::actor_values::FATIGUE;
use crate::combat::HEALTH;
use crate::functions::actor;
use crate::game::Game;
use crate::trace::EventKind;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};

/// Fatigue points that come back each second of game time [design].
pub const FATIGUE_RETURN: f64 = 10.0;
/// Seconds of game time an essential actor stays down at least [design: "a few
/// seconds", documented: GECK wiki KillActor notes]; then until no fight goes on near it
/// [owner rule 2026-10-17].
pub const ESSENTIAL_DOWN: f64 = 10.0;
/// Game units around a knocked-out essential actor within which an actor in combat keeps
/// it down [design].
pub const FIGHT_RADIUS: f32 = 4096.0;
/// Seconds of game time a fatigue knockout lasts, fight or not [owner rule 2026-10-17;
/// design: a test value, the data has no setting for it].
pub const FATIGUE_DOWN: f64 = 30.0;
/// Seconds of game time the getting up lasts at most when the engine does not end it
/// sooner (an actor that is not drawn) [design: the `_male` get-up clips last 2 s].
pub const GETUP_MAX: f64 = 3.0;
/// The share of its maximum health an essential actor gets up with, at least [design].
pub const ESSENTIAL_UP_HEALTH: f64 = 0.5;

/// Why an actor is knocked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// An essential actor brought to zero health.
    Essential,
    /// Its Fatigue ran out.
    Fatigue,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Essential => "essential",
            Cause::Fatigue => "fatigue",
        }
    }
}

/// What happened to a knocked-out actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Happened {
    /// It was knocked out.
    Down,
    /// Its time to get up came while a fight went on near it: it stays down (once a
    /// knockout).
    Held,
    /// It got up.
    Up,
}

/// A knockout or a getting up, for the engine ([`Game::take_knockout_news`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct News {
    pub actor: FormId,
    pub cause: Cause,
    pub what: Happened,
}

/// A knockout in progress.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KnockOut {
    pub cause: Cause,
    /// Game time ([`Game::time`]) it began.
    pub at: f64,
    /// The side the body lies on, as the engine saw it come to rest (IsFacingUp); `None`
    /// until then.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub facing_up: Option<bool>,
    /// A fight near it has held it down past its time (told once).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub held: bool,
    /// Game time it began to get up: from then until [`Game::finish_getting_up`] it plays
    /// its get-up clip and still does not move.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rising: Option<f64>,
}

impl KnockOut {
    /// Lying down (not yet getting up).
    pub fn lying(&self) -> bool {
        self.rising.is_none()
    }
}

impl crate::game::RefExtra {
    /// Knocked out, or set unconscious by a script.
    pub fn out_cold(&self) -> bool {
        self.unconscious || self.knocked_out.is_some()
    }
}

impl Game<'_> {
    /// The actor's knockout, if it is knocked out.
    pub fn knockout(&self, r: FormId) -> Option<KnockOut> {
        self.extra.get(&r).and_then(|e| e.knocked_out)
    }

    /// Knocked out, or set unconscious by a script: it does not fight, talk or move.
    pub fn is_out_cold(&self, r: FormId) -> bool {
        self.extra.get(&r).is_some_and(|e| e.out_cold())
    }

    /// Knock `r` out for `cause` (see the module notes). An essential knockout replaces
    /// a fatigue one (its time starts again); otherwise one already knocked out stays
    /// as it is. Returns whether it was knocked out now.
    pub fn knock_out(&mut self, r: FormId, cause: Cause, by: Option<FormId>) -> bool {
        if r == FormId::PLAYER_REF || self.is_dead(r) {
            return false;
        }
        match self.knockout(r) {
            // Getting up from a fatigue knockout and beaten to zero: down again.
            Some(k) if k.lying() && (k.cause == Cause::Essential || cause == Cause::Fatigue) => return false,
            Some(k) if !k.lying() && cause == Cause::Fatigue => return false,
            _ => {}
        }
        let now = self.time;
        self.extra.entry(r).or_default().knocked_out = Some(KnockOut { cause, at: now, facing_up: None, held: false, rising: None });
        self.emit(EventKind::KnockedOut { actor: r, cause: cause.name() });
        self.knockout_news.push(News { actor: r, cause, what: Happened::Down });
        self.combat_line(r, by, "KOed");
        true
    }

    /// The actor's Fatigue was lowered: at 0 or less it is knocked out (cause fatigue).
    pub fn fatigue_changed(&mut self, r: FormId) {
        if self.knockout(r).is_some() || r == FormId::PLAYER_REF || self.is_dead(r) {
            return;
        }
        // Only actors have a Fatigue to run out of.
        if self.stats_def(r).is_none() {
            return;
        }
        if actor::actor_value(self, r, FATIGUE).is_ok_and(|v| v <= 0.0) {
            let by = self.extra.get(&r).and_then(|e| e.last_hit).and_then(|h| h.attacker);
            self.knock_out(r, Cause::Fatigue, by);
        }
    }

    /// The knockouts and getting up since the last call.
    pub fn take_knockout_news(&mut self) -> Vec<News> {
        std::mem::take(&mut self.knockout_news)
    }

    /// The engine saw the knocked-out body come to rest on its back (`up`) or front.
    pub fn set_knocked_side(&mut self, r: FormId, up: bool) {
        if let Some(k) = self.extra.get_mut(&r).and_then(|e| e.knocked_out.as_mut()) {
            k.facing_up = Some(up);
        }
    }

    /// The engine finished playing the actor's get-up clip: the knockout is over and it
    /// may move again.
    pub fn finish_getting_up(&mut self, r: FormId) {
        if self.knockout(r).is_some_and(|k| !k.lying()) {
            self.end_knockout(r);
        }
    }

    /// The knockout ends at once (ResurrectActor): no event, nothing restored.
    pub fn end_knockout(&mut self, r: FormId) {
        if let Some(e) = self.extra.get_mut(&r) {
            e.knocked_out = None;
        }
    }

    /// Whether an actor in combat (not dead, not knocked out itself) stands within
    /// [`FIGHT_RADIUS`] of `r`, in the same interior or worldspace. The player fights
    /// when someone fights it, so its foes count.
    pub fn fight_near(&mut self, r: FormId) -> bool {
        let fighters: Vec<FormId> = self.awareness.actors.iter().filter(|(id, a)| **id != r && a.state.in_combat()).map(|(id, _)| *id).collect();
        let Some((cell, at)) = self.world.get(r).ok().map(|s| (s.cell, s.pos)) else { return false };
        let space = |g: &Game, c: Option<FormId>| c.map(|c| g.world.placed.world_of(c).unwrap_or(c));
        let mine = space(self, cell);
        let able: Vec<FormId> = fighters.into_iter().filter(|&f| !self.is_dead(f) && self.knockout(f).is_none()).collect();
        able.into_iter().any(|f| {
            let Some((c, p)) = self.world.get(f).ok().map(|s| (s.cell, s.pos)) else { return false };
            mine.is_some() && space(self, c) == mine && (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f32>().sqrt() <= FIGHT_RADIUS
        })
    }

    /// One frame of `dt` game seconds: fatigue returns, and the knocked out whose time
    /// has come begin to get up (see the module notes). Returns those who began to.
    pub fn tick_knockouts(&mut self, dt: f64) -> Vec<FormId> {
        let tired: Vec<FormId> = self.extra.iter().filter(|(_, e)| e.av_damage.contains_key(&FATIGUE)).map(|(r, _)| *r).collect();
        for r in tired {
            actor::restore(self, r, FATIGUE, FATIGUE_RETURN * dt);
        }
        let now = self.time;
        let down: Vec<(FormId, KnockOut)> = self.extra.iter().filter_map(|(r, e)| e.knocked_out.map(|k| (*r, k))).collect();
        let mut up = Vec::new();
        for (r, k) in down {
            if self.is_dead(r) {
                // Killed while down: it stays dead.
                self.end_knockout(r);
                continue;
            }
            if let Some(since) = k.rising {
                // Nobody ended its getting up (not drawn): over after its time.
                if now - since + 1e-9 >= GETUP_MAX {
                    self.end_knockout(r);
                }
                continue;
            }
            let rises = match k.cause {
                Cause::Fatigue => now - k.at + 1e-9 >= FATIGUE_DOWN,
                Cause::Essential => now - k.at + 1e-9 >= ESSENTIAL_DOWN,
            };
            if !rises {
                continue;
            }
            if k.cause == Cause::Essential && self.fight_near(r) {
                if !k.held {
                    if let Some(k) = self.extra.get_mut(&r).and_then(|e| e.knocked_out.as_mut()) {
                        k.held = true;
                    }
                    self.knockout_news.push(News { actor: r, cause: k.cause, what: Happened::Held });
                }
                continue;
            }
            actor::restore(self, r, FATIGUE, f64::MAX);
            if k.cause == Cause::Essential {
                let max = actor::actor_value_base(self, r, HEALTH);
                let health = actor::actor_value(self, r, HEALTH).unwrap_or(max);
                let want = max * ESSENTIAL_UP_HEALTH;
                if health < want {
                    actor::restore(self, r, HEALTH, want - health);
                }
            }
            if let Some(k) = self.extra.get_mut(&r).and_then(|e| e.knocked_out.as_mut()) {
                k.rising = Some(now);
            }
            self.emit(EventKind::GotUp { actor: r, cause: k.cause.name() });
            self.knockout_news.push(News { actor: r, cause: k.cause, what: Happened::Up });
            up.push(r);
        }
        up
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causes_have_names() {
        assert_eq!(Cause::Essential.name(), "essential");
        assert_eq!(Cause::Fatigue.name(), "fatigue");
    }
}
