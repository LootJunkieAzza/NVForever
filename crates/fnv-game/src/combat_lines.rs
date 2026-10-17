//! Combat-tab dialogue: the lines (and effort sounds) actors say in a fight
//! (docs/spec-notes/combat-dialogue.md).
//!
//! - **Hit, Crippled, HealthHalf, HealthQuarter** ([`Game::hit_lines`], from
//!   [`Game::hit`], so every hit on the player, an NPC or a creature): a hit that cripples
//!   a limb says Crippled; one that takes health to half or a quarter says HealthHalf or
//!   HealthQuarter (once per crossing); otherwise, or when the actor has no such line, Hit
//!   when the damage is more than `fCombatSpeakHitThreshold` of its health and (an NPC
//!   or creature) a random number is below `fCombatSpeakHitChance`, or (the player)
//!   either [documented: Combat Tab names both settings; the "and" for NPCs is fitted:
//!   in the original's NPC battles about 1 % of 217 hits were followed by a line,
//!   observed 2026-10-17, docs/spec-notes/npc-behaviour-compare.md §2].
//! - **Death** ([`Game::death_line`], from [`Game::kill`]): always, cutting the actor's
//!   line; the line goes on although its speaker is dead (`Game::update_speech`). The
//!   nearest living member of its group within [`RESPONSE_RADIUS`] then says
//!   DeathResponse once the Death line is over [documented: Combat Tab, "If a valid actor
//!   is nearby"; design: who is valid and how near].
//! - **Attack, PowerAttack, FireExplosive** ([`Game::attack_line`], from the engine when
//!   the player or an NPC begins an attack): at most one per actor per random time between
//!   `fCombatDialogueAttackMinElapsedTime` and `…MaxElapsedTime` [documented: Combat Tab];
//!   an NPC or creature also only when a random number is below `fCombatSpeakAttackChance`
//!   [hypothesis fitted to the original: about 3 lines for some 175 attacks begun in its
//!   NPC battles, observed 2026-10-17; the player keeps the timer alone, our design].
//! - **WeaponBroke, NoAmmo** ([`Game::combat_line`], from the engine).
//!
//! A response with a sound of its own (creature grunts, robot buzzes) plays that sound
//! instead of the voice: `nvf-play`'s `voice.rs`.

use crate::game::Game;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// The actor of the group nearest to one that died says DeathResponse when within this
/// many game units of it [design: a room's width; the documentation says "nearby"].
pub const RESPONSE_RADIUS: f32 = 1536.0;

/// The settings of the Combat tab's timing, with FalloutNV.esm's values as defaults
/// [observed 2026-10-16, `fnvtool esm decode`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// `fCombatSpeakHitThreshold`: Hit is said when the damage is more than this share of
    /// the actor's health.
    pub hit_threshold: f64,
    /// `fCombatSpeakHitChance`: or when a random number in [0, 1) is below this.
    pub hit_chance: f64,
    /// `fCombatDialogueAttackMin/MaxElapsedTime`: seconds between two Attack or
    /// PowerAttack lines of one actor.
    pub attack_wait: (f64, f64),
    /// `fCombatSpeakAttackChance`: an NPC's chance, per attack begun, to say its line.
    pub attack_chance: f64,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { hit_threshold: 0.01, hit_chance: 0.01, attack_wait: (5.0, 30.0), attack_chance: 0.01 }
    }
}

impl Timing {
    pub fn read(g: &Game<'_>) -> Timing {
        let d = Timing::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        Timing {
            hit_threshold: s("fCombatSpeakHitThreshold", d.hit_threshold),
            hit_chance: s("fCombatSpeakHitChance", d.hit_chance),
            attack_wait: (s("fCombatDialogueAttackMinElapsedTime", d.attack_wait.0), s("fCombatDialogueAttackMaxElapsedTime", d.attack_wait.1)),
            attack_chance: s("fCombatSpeakAttackChance", d.attack_chance),
        }
    }
}

/// Combat lines' state. Not part of the saved state.
#[derive(Debug, Clone, Default)]
pub struct CombatLines {
    /// Per actor: `Game::time` before which it says no Attack or PowerAttack line.
    pub next_attack: BTreeMap<FormId, f64>,
    /// Per actor: the lowest health mark it has said a line for (2 half, 4 quarter).
    pub marks: BTreeMap<FormId, u8>,
    /// DeathResponse lines due: (when, who, about the dead).
    pub responses: Vec<(f64, FormId, FormId)>,
    /// Lines tried since the engine last took them: topic, speaker, the line said (`None`:
    /// the actor has none).
    pub said: Vec<(&'static str, FormId, Option<FormId>)>,
    pub timing: Option<Timing>,
    rng: u64,
}

impl CombatLines {
    /// A number in [0, 1) from the module's own sequence, so that the game's random
    /// numbers stay as they were.
    fn unit(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The health mark a hit crossed, from `before` to `after` of `max`: 4 for a quarter, 2
/// for half, 0 none; `said` is the lowest mark already said (a mark is said once until
/// health rises above it again).
pub fn mark_crossed(before: f64, after: f64, max: f64, said: u8) -> u8 {
    if max <= 0.0 || after <= 0.0 {
        return 0;
    }
    let at = |m: u8| max / f64::from(m);
    [4u8, 2].into_iter().find(|&m| after <= at(m) && before > at(m) && said < m).unwrap_or(0)
}

/// The lowest mark an actor at `health` of `max` is still below (to forget marks it rose
/// above).
pub fn mark_below(health: f64, max: f64) -> u8 {
    if max <= 0.0 {
        0
    } else if health <= max / 4.0 {
        4
    } else if health <= max / 2.0 {
        2
    } else {
        0
    }
}

/// Whether a hit of `dealt` damage on an actor of `max` health says Hit, with `roll` a
/// random number in [0, 1): an NPC or creature (`npc`) needs both the threshold and the
/// chance, the player either (module notes).
pub fn says_hit(dealt: f64, max: f64, roll: f64, t: &Timing, npc: bool) -> bool {
    let over = max > 0.0 && dealt / max > t.hit_threshold;
    let chance = roll < t.hit_chance;
    if npc {
        over && chance
    } else {
        over || chance
    }
}

impl Game<'_> {
    fn combat_timing(&mut self) -> Timing {
        if let Some(t) = self.combat_lines.timing {
            return t;
        }
        let t = Timing::read(self);
        self.combat_lines.timing = Some(t);
        t
    }

    /// `actor` says its line of the Combat topic `topic` about `about` (the conditions'
    /// target), when the load order has the topic, the actor is alive and not already
    /// speaking, and no conversation or menu holds the game (as Detection lines,
    /// `Game::detection_bark`). Returns the line said.
    pub fn combat_line(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) -> Option<FormId> {
        if self.menu.is_some() || self.dialogue.is_some() || self.is_talking(actor) || self.is_dead(actor) {
            return None;
        }
        self.say_combat(actor, about, topic)
    }

    fn say_combat(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) -> Option<FormId> {
        let data = self.data;
        let t = data.by_edid(topic).filter(|t| data.topic_defs.contains_key(t))?;
        let line = self.say(actor, about, t, false);
        self.combat_lines.said.push((topic, actor, line));
        line
    }

    /// `actor` begins an attack on `target`: a power attack, the throw of a grenade or
    /// the shot of an explosive, or another. It says PowerAttack, FireExplosive or Attack
    /// once its attack timer has run out, which starts again on every try (said or not:
    /// an actor with no line does not look for one on every shot) [documented timer:
    /// Combat Tab; design: FireExplosive shares it].
    pub fn attack_line(&mut self, actor: FormId, target: Option<FormId>, power: bool, explosive: bool) -> Option<FormId> {
        let now = self.time;
        if self.combat_lines.next_attack.get(&actor).is_some_and(|&t| now < t) {
            return None;
        }
        if self.menu.is_some() || self.dialogue.is_some() || self.is_talking(actor) || self.is_dead(actor) {
            return None;
        }
        let t = self.combat_timing();
        if actor != FormId::PLAYER_REF && self.combat_lines.unit() >= t.attack_chance {
            return None;
        }
        let wait = t.attack_wait.0 + (t.attack_wait.1 - t.attack_wait.0).max(0.0) * self.combat_lines.unit();
        self.combat_lines.next_attack.insert(actor, now + wait);
        let topic = if power {
            "PowerAttack"
        } else if explosive {
            "FireExplosive"
        } else {
            "Attack"
        };
        self.say_combat(actor, target, topic).or_else(|| if topic == "Attack" { None } else { self.say_combat(actor, target, "Attack") })
    }

    /// The lines of a hit that took `target` from `before` to `after` health of `max`
    /// (called by [`Game::hit`]; see the module notes).
    pub(crate) fn hit_lines(&mut self, target: FormId, attacker: Option<FormId>, before: f64, after: f64, max: f64, crippled: bool) {
        if self.is_dead(target) {
            return;
        }
        let below = mark_below(after, max);
        let said = self.combat_lines.marks.get(&target).copied().unwrap_or(0).min(below);
        let mark = mark_crossed(before, after, max, said);
        if below == 0 {
            self.combat_lines.marks.remove(&target);
        } else {
            self.combat_lines.marks.insert(target, said.max(mark));
        }
        if crippled && self.combat_line(target, attacker, "Crippled").is_some() {
            return;
        }
        let health_topic = match mark {
            4 => Some("HealthQuarter"),
            2 => Some("HealthHalf"),
            _ => None,
        };
        if health_topic.is_some_and(|t| self.combat_line(target, attacker, t).is_some()) {
            return;
        }
        let t = self.combat_timing();
        let roll = self.combat_lines.unit();
        if says_hit(before - after, max, roll, &t, target != FormId::PLAYER_REF) {
            self.combat_line(target, attacker, "Hit");
        }
    }

    /// `actor` died (called by [`Game::kill`]): it says Death, cutting what it was
    /// saying, and the nearest living member of its group near it will say DeathResponse
    /// when that line is over.
    pub(crate) fn death_line(&mut self, actor: FormId, killer: Option<FormId>) {
        self.combat_lines.next_attack.remove(&actor);
        self.combat_lines.marks.remove(&actor);
        if self.loading || self.menu.is_some() || self.dialogue.is_some() {
            return;
        }
        self.cut_line(actor);
        if self.say_combat(actor, killer, "Death").is_none() {
            return;
        }
        let Some(at) = crate::awareness::position(self, actor) else { return };
        let ids: Vec<FormId> = self.awareness.actors.keys().copied().filter(|&o| o != actor && o != FormId::PLAYER_REF).collect();
        let near = ids
            .into_iter()
            .filter(|&o| !self.is_dead(o) && self.bark_mates(actor, o))
            .filter_map(|o| {
                let p = crate::awareness::position(self, o)?;
                let d = (p[0] - at[0]).hypot(p[1] - at[1]).hypot(p[2] - at[2]);
                (d <= RESPONSE_RADIUS).then_some((d, o))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, o)) = near {
            let seconds = self.speech.iter().find(|s| s.speaker == actor).map_or(0.0, |s| s.remaining);
            self.combat_lines.responses.push((self.time + seconds, o, actor));
        }
    }

    /// The DeathResponse lines that are due (called each frame from `Game::update_speech`).
    pub(crate) fn combat_lines_tick(&mut self) {
        let now = self.time;
        if self.combat_lines.responses.iter().all(|r| r.0 > now) {
            return;
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.combat_lines.responses).into_iter().partition(|r| r.0 <= now);
        self.combat_lines.responses = later;
        for (_, who, dead) in due {
            if self.present(who) {
                self.combat_line(who, Some(dead), "DeathResponse");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_marks_are_said_once_per_crossing() {
        // 100 → 60: no mark; 60 → 45: half; 45 → 20: quarter; again below: nothing.
        assert_eq!(mark_crossed(100.0, 60.0, 100.0, 0), 0);
        assert_eq!(mark_crossed(60.0, 45.0, 100.0, 0), 2);
        assert_eq!(mark_crossed(45.0, 20.0, 100.0, 2), 4);
        assert_eq!(mark_crossed(20.0, 10.0, 100.0, 4), 0);
        // One hit past both marks says the quarter.
        assert_eq!(mark_crossed(90.0, 10.0, 100.0, 0), 4);
        // Death says no mark.
        assert_eq!(mark_crossed(30.0, 0.0, 100.0, 0), 0);
        // Healed above half, then hit below again: said again.
        assert_eq!(mark_below(70.0, 100.0), 0);
        assert_eq!(mark_crossed(70.0, 40.0, 100.0, 0), 2);
    }

    #[test]
    fn hit_is_said_above_the_threshold_or_by_chance() {
        let t = Timing::default();
        // The player: either.
        assert!(says_hit(2.0, 100.0, 0.5, &t, false));
        assert!(!says_hit(0.5, 100.0, 0.5, &t, false));
        assert!(says_hit(0.5, 100.0, 0.005, &t, false));
        // An NPC: both.
        assert!(!says_hit(2.0, 100.0, 0.5, &t, true));
        assert!(!says_hit(0.5, 100.0, 0.005, &t, true));
        assert!(says_hit(2.0, 100.0, 0.005, &t, true));
    }
}
