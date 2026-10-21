//! Unconscious actors (docs/spec-notes/unconscious.md §2): essential actors at zero
//! health and any actor whose Fatigue runs out are knocked out instead of dying, and get
//! up again; live actors knocked down (PushActorAway, explosions) lie down for a moment
//! the same way (§5).
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
//! - **Knockdowns** ([`Game::push_actor_away`], [`Game::knock_down`]) are knockouts
//!   with their own causes ([`Cause::Pushed`], [`Cause::KnockedDown`]) that last
//!   [`KNOCKDOWN_DOWN`] seconds; a push gives the body a velocity the engine takes
//!   ([`Game::take_shove`]).
//! - SetUnconscious holds a lying actor down until it is cleared (§5).
//! - The player is knocked down like anyone (§5.4) but never knocked out; enemies may
//!   still attack it while it lies (`Game::would_attack`).
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
/// Seconds of game time an essential actor stays down at least [observed: RingoRef, made
/// essential and knocked out by KillActor with no fight about, lay 10.0 s and then got up
/// (GetAnimAction 13 for 2.2 s), AI observer campaign `functions_round` 2026-10-19,
/// docs/spec-notes/unconscious.md §2.10]; then until no fight goes on near it [owner rule
/// 2026-10-17].
pub const ESSENTIAL_DOWN: f64 = 10.0;
/// Game units around a knocked-out essential actor within which an actor in combat keeps
/// it down [design].
pub const FIGHT_RADIUS: f32 = 4096.0;
/// Seconds of game time a fatigue knockout lasts, fight or not [owner rule 2026-10-17;
/// design: a test value, the data has no setting for it].
pub const FATIGUE_DOWN: f64 = 30.0;
/// Seconds of game time the getting up lasts at most when the engine does not end it
/// sooner (an actor that is not drawn) [observed: GetAnimAction 13, the get-up, lasted
/// 2.41-2.53 s after RingoRef's pushes, AI observer campaign `knockdown_look` 2026-10-19,
/// docs/spec-notes/unconscious.md §5.5].
pub const GETUP_MAX: f64 = 2.5;
/// The share of its maximum health an essential actor gets up with, at least [design].
pub const ESSENTIAL_UP_HEALTH: f64 = 0.5;
/// Seconds of game time a knockdown (PushActorAway, an explosion) keeps an actor down,
/// from the push to the start of its get-up [observed: 2.38 s after four of five pushes
/// of RingoRef at forces 10 and 120, whatever the distance, as the player's
/// `PLAYER_KNOCKDOWN_DOWN`; 5.40 s after the fifth (force 40, the body against a wall),
/// campaign `knockdown_look` 2026-10-19, unconscious.md §5.5; explosions not measured].
pub const KNOCKDOWN_DOWN: f64 = 2.4;
/// Seconds the player lies knocked down before getting up: from the push until the
/// character is stood up for the get-up clip [observed: capture `kd_player_knockdown`
/// 2026-10-18, PushActorAway force 10, about 8 samples of 0.30 s in both views,
/// docs/spec-notes/unconscious.md §5.4].
pub const PLAYER_KNOCKDOWN_DOWN: f64 = 2.4;
/// Game units a second of the body's start velocity per point of PushActorAway's force
/// (the wiki calls it a "Havok force") [observed: RingoRef's reference flew 330-420
/// units/s at force 10 and 1,000-1,300 at force 40, campaign `knockdown_look`
/// 2026-10-19, unconscious.md §5.5].
pub const PUSH_SPEED: f32 = 30.0;
/// The upward speed a push gives the body, game units a second, whatever the force (no
/// more than the push's own speed) [observed: the reference rose 42 units at force 10 and
/// 31 at force 40, 220-240 units/s at the start under the game's gravity; same campaign].
pub const PUSH_RISE: f32 = 220.0;
/// The upward share of an explosion's knockdown velocity [design].
pub const PUSH_LIFT: f32 = 0.25;

/// Why an actor is knocked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// An essential actor brought to zero health.
    Essential,
    /// Its Fatigue ran out.
    Fatigue,
    /// Knocked down by PushActorAway.
    Pushed,
    /// Knocked down by an explosion.
    KnockedDown,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Essential => "essential",
            Cause::Fatigue => "fatigue",
            Cause::Pushed => "pushed",
            Cause::KnockedDown => "knocked_down",
        }
    }

    /// A knockdown: the actor is not out cold, only down for a moment.
    pub fn knockdown(self) -> bool {
        matches!(self, Cause::Pushed | Cause::KnockedDown)
    }

    /// Which cause wins when a second one comes while the actor lies: essential over
    /// fatigue over a knockdown.
    fn rank(self) -> u8 {
        match self {
            Cause::Essential => 2,
            Cause::Fatigue => 1,
            Cause::Pushed | Cause::KnockedDown => 0,
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
    /// PushActorAway laid it down: GetKnockedState reads 0 until it is up, also when it
    /// is knocked out meanwhile, and the player can open its inventory [documented: GECK
    /// wiki GetKnockedState and PushActorAway notes].
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub pushed: bool,
    /// The velocity (game units a second, game space) its body is to be thrown with,
    /// until the engine takes it ([`Game::take_shove`]).
    #[serde(skip)]
    pub shove: Option<[f32; 3]>,
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

    /// Knock `r` out for `cause` (see the module notes). A stronger cause replaces the
    /// one it lies with (essential over fatigue over a knockdown; its time starts again);
    /// otherwise one already lying stays as it is. Returns whether it went down now.
    pub fn knock_out(&mut self, r: FormId, cause: Cause, by: Option<FormId>) -> bool {
        self.knock(r, cause, by, None)
    }

    fn knock(&mut self, r: FormId, cause: Cause, by: Option<FormId>, shove: Option<[f32; 3]>) -> bool {
        // The player is knocked down but never knocked out (§2.1, §5.4).
        if (r == FormId::PLAYER_REF && !cause.knockdown()) || self.is_dead(r) {
            return false;
        }
        let old = self.knockout(r);
        let replaces = match old {
            None => true,
            Some(k) if k.lying() => cause.rank() > k.cause.rank(),
            // Getting up and knocked again: down again, unless only tired again.
            Some(_) => cause != Cause::Fatigue,
        };
        let pushed = cause == Cause::Pushed || old.is_some_and(|k| k.pushed && k.lying());
        if !replaces {
            // Still lying: a push throws the body again and marks it pushed.
            if let Some(k) = self.extra.get_mut(&r).and_then(|e| e.knocked_out.as_mut()).filter(|k| k.lying()) {
                k.pushed |= pushed;
                k.shove = shove.or(k.shove);
            }
            return false;
        }
        let now = self.time;
        let facing_up = old.filter(|k| k.lying()).and_then(|k| k.facing_up);
        self.extra.entry(r).or_default().knocked_out = Some(KnockOut { cause, at: now, facing_up, held: false, rising: None, pushed, shove });
        self.emit(EventKind::KnockedOut { actor: r, cause: cause.name() });
        self.knockout_news.push(News { actor: r, cause, what: Happened::Down });
        if !cause.knockdown() {
            self.combat_line(r, by, "KOed");
        } else if by == Some(FormId::PLAYER_REF) && r != FormId::PLAYER_REF {
            self.challenge_knockdown(r);
        }
        true
    }

    /// Whether `r` is an actor that can be knocked down: alive, an NPC or creature
    /// without the "No Knockdowns" flag (its own record's, never a template's) and not an
    /// "Immobile" creature [documented: GECK wiki PushActorAway notes, Category:NPC
    /// Template Data]; the player too, unless on furniture or in a conversation [design:
    /// unconscious.md §5.4; six shipped PushActorAway calls throw the player, §5.1].
    pub fn can_knock_down(&self, r: FormId) -> bool {
        if self.is_dead(r) {
            return false;
        }
        if r == FormId::PLAYER_REF {
            return self.furniture_use(r).is_none() && self.dialogue.is_none();
        }
        self.def_following(r, |_| false).is_some_and(|d| !d.no_knockdowns && !d.immobile)
    }

    /// `source.PushActorAway target force`: `target` is knocked down ([`Cause::Pushed`])
    /// and its body thrown away from `source` (towards it for a negative force), straight
    /// up without a source [documented: GECK wiki PushActorAway]; the speed is
    /// [`PUSH_SPEED`] per point of force, with [`PUSH_RISE`] upwards [observed]. Returns
    /// whether it was pushed (refused as [`Game::can_knock_down`] says).
    pub fn push_actor_away(&mut self, source: Option<FormId>, target: FormId, force: f64) -> bool {
        if !self.can_knock_down(target) {
            return false;
        }
        let speed = force as f32 * PUSH_SPEED;
        let to = self.world.get(target).ok().map(|s| s.pos);
        let from = source.filter(|&s| s != target).and_then(|s| self.world.get(s).ok().map(|s| s.pos));
        let away = from.zip(to).and_then(|(from, to)| {
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            let len = (dx * dx + dy * dy).sqrt();
            (len > 1e-3).then(|| [dx / len, dy / len])
        });
        let shove = match away {
            Some([x, y]) => [x * speed, y * speed, PUSH_RISE.min(speed.abs())],
            None => [0.0, 0.0, speed.abs()],
        };
        self.knock(target, Cause::Pushed, source, Some(shove));
        true
    }

    /// An explosion knocked `r` down ([`Cause::KnockedDown`], GetKnockedState 2), its
    /// body thrown with `shove` (game units a second). Returns whether it went down.
    pub fn knock_down(&mut self, r: FormId, by: Option<FormId>, shove: [f32; 3]) -> bool {
        self.can_knock_down(r) && self.knock(r, Cause::KnockedDown, by, Some(shove))
    }

    /// The velocity a knockdown gave `r`'s body, once: the engine throws the ragdoll with
    /// it.
    pub fn take_shove(&mut self, r: FormId) -> Option<[f32; 3]> {
        self.extra.get_mut(&r).and_then(|e| e.knocked_out.as_mut()).and_then(|k| k.shove.take())
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
                Cause::Pushed | Cause::KnockedDown => now - k.at + 1e-9 >= if r == FormId::PLAYER_REF { PLAYER_KNOCKDOWN_DOWN } else { KNOCKDOWN_DOWN },
            };
            // SetUnconscious holds a lying actor down until it is cleared [hypothesis:
            // the thugs that fake death, SCPT 0x00118A77, push their actor down, set it
            // unconscious and clear it only when hit].
            if !rises || self.extra.get(&r).is_some_and(|e| e.unconscious) {
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
            if !k.cause.knockdown() {
                actor::restore(self, r, FATIGUE, f64::MAX);
            }
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
            // "Said when an actor wakes from unconsciousness" [documented: GECK wiki
            // Combat Tab, Wake; docs/spec-notes/combat-dialogue.md §5]; not after a
            // knockdown, which is no unconsciousness [design].
            if !k.cause.knockdown() {
                self.combat_line(r, None, "Wake");
            }
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
        assert_eq!(Cause::Pushed.name(), "pushed");
        assert!(Cause::KnockedDown.knockdown() && !Cause::Essential.knockdown());
        assert!(Cause::Essential.rank() > Cause::Fatigue.rank() && Cause::Fatigue.rank() > Cause::Pushed.rank());
    }
}
