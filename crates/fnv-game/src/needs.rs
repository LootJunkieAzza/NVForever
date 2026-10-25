//! Time passing for the player's body: Hardcore's needs (dehydration, hunger, sleep
//! deprivation) rising with time, the stages of rads and of the needs (their actor
//! effects), and the hours of a wait or a sleep advancing active effects and needs
//! (docs/spec-notes/aid-items.md §6, §11).
//!
//! - **Needs rise** in Hardcore mode only, by one point every `rate` seconds of game-mode
//!   time, `rate` being `fHCStarvationRate` (25 in FalloutNV.esm) for hunger and
//!   `fHCSleepDeprivationRate` (50) for sleep deprivation [observed: the two settings
//!   and their values; hypothesis: the unit. The game's help says dehydration rises
//!   fastest, starvation slower and sleep deprivation slowest ("HelpHardcoreNeeds",
//!   documented in the masters), so the larger number is the slower need: a number of
//!   seconds per point]. Dehydration's rate is in no master [observed]: the code reads
//!   `fHCDehydrationRate` if a plugin has it [hypothesis: the name, by analogy] and else
//!   takes [`DEHYDRATION_RATE`], 10 s [documented: the owner's Hardcore table,
//!   2026-10-23, "~12/h (1 every 10s)"; the same table gives hunger "1 every 25s" and
//!   sleep "1 every 50s", which the two settings agree with].
//! - **Waiting and sleeping** "will also affect your needs" [documented: the help
//!   message]: an hour waited counts as the seconds of play in which an hour passes at
//!   the TimeScale (3600 / 30 = 120 s) for the effects' timers and the needs
//!   [hypothesis]; an hour slept also lowers sleep deprivation by
//!   [`SLEEP_RECOVERY_PER_HOUR`] instead of raising it [documented: the owner's Hardcore
//!   table, 2026-10-23, "~ -60/h Sleep"].
//! - **Stages**: RADS, DEHY, HUNG and SLPD records give from which value each stage holds
//!   and its actor effect (200 Minor … 1000 Fatal). The player has the effect of the
//!   highest stage reached, added as AddSpell adds it, and loses it when the value drops
//!   below [hypothesis]. Rads' stages hold in every mode, the needs' only in Hardcore
//!   [hypothesis: radiation sickness is no Hardcore feature; the needs' stage effects
//!   are taken away when Hardcore is turned off].
//! - **Sleep heals crippled limbs** outside Hardcore: an hour slept takes away all the
//!   damage of the player's limb conditions [observed: FalloutNV.esm MESG
//!   `HelpHealingLimbs`, "to heal a crippled limb, you have three choices: sleep in a
//!   bed, find a doctor, or use a Stimpak"; "In Hardcore Mode, you may only heal crippled
//!   limbs by using Doctor's Bags or by visiting a doctor"; hypothesis: the first hour
//!   heals them all] (docs/spec-notes/crippled-limbs.md §6).
//! - Only the player has needs here [design].

use crate::aid::{DEHYDRATION, HUNGER, RAD_LEVEL, SLEEP_DEPRIVATION};
use crate::combat::setting;
use crate::functions::actor as fa;
use crate::game::Game;
use crate::trace::EventKind;
use fnv_sim::FormId;

/// Seconds of play per point of dehydration when no plugin sets the rate [documented:
/// the owner's Hardcore table, 2026-10-23, "1 every 10s", about 12 an hour; no master
/// has the setting. Before, a guess of 12.5]. The fastest need, as the help message says.
pub const DEHYDRATION_RATE: f64 = 10.0;

/// Points of sleep deprivation an hour of sleep takes away [documented: the owner's
/// Hardcore table, 2026-10-23, "~ -60/h Sleep"; nothing in the masters says. Before, a
/// guess of 100].
pub const SLEEP_RECOVERY_PER_HOUR: f64 = 60.0;

/// The needs, their rate settings and the rates used without them.
pub const NEEDS: [(u16, &str, f64); 3] = [
    (DEHYDRATION, "fHCDehydrationRate", DEHYDRATION_RATE),
    (HUNGER, "fHCStarvationRate", 25.0),
    (SLEEP_DEPRIVATION, "fHCSleepDeprivationRate", 50.0),
];

/// The TimeScale used when the world has none (game seconds per real second).
const DEFAULT_TIME_SCALE: f64 = 30.0;

/// What passing hours did ([`Game::pass_hours`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Passed {
    /// The seconds of play the hours stood for.
    pub seconds: f64,
    /// The needs before and after (Hardcore only).
    pub needs: Vec<(u16, f64, f64)>,
    /// Effects on the player that ended meanwhile.
    pub effects_ended: usize,
    /// Crippled limbs the sleep healed (condition codes).
    pub limbs_healed: Vec<u16>,
}

impl Game<'_> {
    /// Whether Hardcore mode is on (SetHardcore, else the configuration).
    pub fn hardcore(&self) -> bool {
        self.player.hardcore.unwrap_or(self.world.config().hardcore)
    }

    /// Turn Hardcore mode on or off (SetHardcore): the needs, and the weight of
    /// ammunition, which the simulation counts by its own switch
    /// (`fnv_sim::World::set_hardcore`) [observed: FalloutNV.esm's Hardcore help text,
    /// "ammunition has weight"]. Turning it off marks the game as not always Hardcore
    /// (IsAlwaysHardcore).
    pub fn set_hardcore(&mut self, on: bool) {
        self.player.hardcore = Some(on);
        self.player.hardcore_was_off |= !on;
        self.world.set_hardcore(on);
    }

    /// Seconds of play per point of a need.
    pub fn need_rate(&self, code: u16) -> f64 {
        NEEDS.iter().find(|n| n.0 == code).map_or(f64::INFINITY, |&(_, name, default)| setting(self, name, default)).max(1e-3)
    }

    /// Raise (or, negative, lower) a need of the player, never below 0.
    pub(crate) fn shift_need(&mut self, code: u16, by: f64) {
        let p = FormId::PLAYER_REF;
        let now = fa::actor_value(self, p, code).unwrap_or(0.0);
        let to = (now + by).max(0.0);
        if to != now {
            let _ = fa::shift_base(self, p, code, to - now);
        }
    }

    /// One frame of game-mode time for the player's needs and stages.
    pub(crate) fn tick_needs(&mut self, dt: f64) {
        if self.hardcore() && dt > 0.0 {
            for (code, _, _) in NEEDS {
                let rate = self.need_rate(code);
                self.shift_need(code, dt / rate);
            }
        }
        self.update_need_stages();
    }

    /// The game's TimeScale (game seconds per second of play).
    pub fn time_scale(&self) -> f64 {
        self.world.time_scale().filter(|s| *s > 0.0).unwrap_or(DEFAULT_TIME_SCALE)
    }

    /// `hours` of a wait or a sleep have passed on the clock (the caller moved the
    /// calendar): active effects advance by the seconds of play those hours stand for,
    /// needs rise (Hardcore), a sleep lowers sleep deprivation, and stages follow.
    pub fn pass_hours(&mut self, hours: f64, sleeping: bool) -> Passed {
        let seconds = hours.max(0.0) * 3600.0 / self.time_scale();
        let p = FormId::PLAYER_REF;
        let started: Vec<u32> = self.effects.iter().filter(|e| e.target == p).map(|e| e.id).collect();
        self.advance_effects(seconds);
        // Explosions' radiation zones fade over the hours waited too (radiation.md §2.1).
        self.age_radiation_zones(seconds);
        let effects_ended = started.iter().filter(|id| !self.effects.iter().any(|e| e.id == **id)).count();
        let mut needs = Vec::new();
        if self.hardcore() {
            for (code, _, _) in NEEDS {
                let from = fa::actor_value(self, p, code).unwrap_or(0.0);
                let by = if sleeping && code == SLEEP_DEPRIVATION { -SLEEP_RECOVERY_PER_HOUR * hours } else { seconds / self.need_rate(code) };
                self.shift_need(code, by);
                needs.push((code, from, fa::actor_value(self, p, code).unwrap_or(0.0)));
            }
        }
        self.update_need_stages();
        let limbs_healed = if sleeping && hours > 0.0 && !self.hardcore() { self.heal_limbs(p) } else { Vec::new() };
        self.emit(EventKind::TimePassed { hours, seconds, sleeping });
        Passed { seconds, needs, effects_ended, limbs_healed }
    }

    /// Take away the damage of every limb condition of `r`; the crippled ones healed.
    pub fn heal_limbs(&mut self, r: FormId) -> Vec<u16> {
        let crippled: Vec<u16> = crate::combat::LIMB_CONDITIONS.filter(|&c| self.is_crippled(r, c)).collect();
        if let Some(e) = self.extra.get_mut(&r) {
            for code in crate::combat::LIMB_CONDITIONS {
                e.av_damage.remove(&code);
            }
        }
        crippled
    }

    /// The player gets the effect of the stage each need (and rads) has reached, and
    /// loses that of a stage left.
    pub(crate) fn update_need_stages(&mut self) {
        let data = self.data;
        if data.need_stages.is_empty() {
            return;
        }
        let p = FormId::PLAYER_REF;
        let hardcore = self.hardcore();
        for code in [RAD_LEVEL, DEHYDRATION, HUNGER, SLEEP_DEPRIVATION] {
            let Some(stages) = data.need_stages.get(&code) else { continue };
            let held = self.player.need_stages.get(&code).copied();
            let value = fa::actor_value(self, p, code).unwrap_or(0.0);
            let want = if code == RAD_LEVEL || hardcore { stages.iter().rev().find(|s| value >= s.threshold as f64) } else { None };
            if want.map(|s| s.stage) == held {
                continue;
            }
            if let Some(old) = held.and_then(|h| stages.iter().find(|s| s.stage == h)) {
                if let Some(x) = self.extra.get_mut(&p) {
                    x.spells.retain(|&s| s != old.spell);
                }
                self.dispel(p, None, |e| e.source == old.spell);
            }
            match want {
                Some(s) => {
                    self.player.need_stages.insert(code, s.stage);
                    self.add_spell(p, s.spell);
                }
                None => {
                    self.player.need_stages.remove(&code);
                }
            }
            self.emit(EventKind::NeedStage { actor: p, value: code, level: value, stage: want.map(|s| s.stage), spell: want.map(|s| s.spell) });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_masters_rates_make_dehydration_fastest_and_sleep_slowest() {
        let rate = |c: u16| NEEDS.iter().find(|n| n.0 == c).unwrap().2;
        assert!(rate(DEHYDRATION) < rate(HUNGER) && rate(HUNGER) < rate(SLEEP_DEPRIVATION));
        // An hour waited at TimeScale 30 is 120 s of play: 4.8 points of hunger.
        assert!((120.0 / rate(HUNGER) - 4.8).abs() < 1e-9);
    }
}
