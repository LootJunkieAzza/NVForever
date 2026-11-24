//! The game's difficulty (docs/spec-notes/difficulty.md): five levels that scale the
//! damage the player deals and takes and the XP for a kill, and NVForever's Custom level,
//! which sets each of the three itself.
//!
//! - Levels 0 Very Easy … 4 Very Hard [documented: GECK wiki GetGameDifficulty]; the
//!   engine keeps the chosen one in [`Game::difficulty`] (it is a setting of the
//!   program, not of the save: spec §1).
//! - Damage: a hit by the player on another actor × `fDiffMultHPByPC<level>`, a hit on
//!   the player × `fDiffMultHPToPC<level>` ([`Game::damage_mult`]) [hypothesis: the
//!   settings' roles from their names; no master carries them, so the defaults here
//!   are used, [`BY_PC`], [`TO_PC`], hypotheses too].
//! - Kill XP × `fDiffMultXP<level>` ([`Game::kill_xp_mult`]) [documented: GECK wiki
//!   Experience Settings; observed: 1.0 at every level in FalloutNV.esm].
//! - Custom ([`Difficulty::Custom`]) takes the three multipliers from
//!   [`Game::difficulty_custom`] instead of the game settings, each from [`CUSTOM_MIN`] to
//!   [`CUSTOM_MAX`]; scripts see it as Normal (spec §5) [design].
//! - A script's `SetGameDifficulty` also leaves its level in [`Game::difficulty_request`],
//!   so the engine, which keeps the chosen level, takes it rather than putting its own back
//!   (spec §4) [design].

use crate::combat::setting;
use crate::game::Game;
use fnv_sim::FormId;
use serde::Serialize;

/// A difficulty level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum Difficulty {
    VeryEasy,
    Easy,
    #[default]
    Normal,
    Hard,
    VeryHard,
    /// NVForever's own: the multipliers are the player's ([`CustomDifficulty`]) [design].
    Custom,
}

/// The original's five levels, by code.
pub const LEVELS: [Difficulty; 5] = [Difficulty::VeryEasy, Difficulty::Easy, Difficulty::Normal, Difficulty::Hard, Difficulty::VeryHard];

/// The settings page's choices: the five levels, then Custom.
pub const CHOICES: [Difficulty; 6] = [Difficulty::VeryEasy, Difficulty::Easy, Difficulty::Normal, Difficulty::Hard, Difficulty::VeryHard, Difficulty::Custom];

/// `fDiffMultHPByPC<level>` when the load order lacks it, by code [hypothesis: spec §3.6].
pub const BY_PC: [f64; 5] = [2.0, 1.5, 1.0, 0.75, 0.5];
/// `fDiffMultHPToPC<level>` when the load order lacks it, by code [hypothesis: spec §3.6].
pub const TO_PC: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// `fDiffMultXP<level>` when the load order lacks it, by code [documented: GECK wiki
/// Experience Settings defaults].
pub const XP: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];

/// The least a Custom multiplier may be (50%) [design].
pub const CUSTOM_MIN: f64 = 0.5;
/// The most a Custom multiplier may be (300%) [design].
pub const CUSTOM_MAX: f64 = 3.0;

/// The Custom level's multipliers (1 = Normal's) [design].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CustomDifficulty {
    /// The damage the player deals (in place of `fDiffMultHPByPC`).
    pub dealt: f64,
    /// The damage the player takes (in place of `fDiffMultHPToPC`).
    pub taken: f64,
    /// The XP for a kill (in place of `fDiffMultXP`).
    pub kill_xp: f64,
}

impl Default for CustomDifficulty {
    /// 100% each: Custom starts as Normal.
    fn default() -> Self {
        CustomDifficulty { dealt: 1.0, taken: 1.0, kill_xp: 1.0 }
    }
}

impl CustomDifficulty {
    /// Each multiplier brought into [`CUSTOM_MIN`]..=[`CUSTOM_MAX`] (one that is not a
    /// number: 1).
    pub fn clamped(self) -> CustomDifficulty {
        let c = |v: f64| if v.is_finite() { v.clamp(CUSTOM_MIN, CUSTOM_MAX) } else { 1.0 };
        CustomDifficulty { dealt: c(self.dealt), taken: c(self.taken), kill_xp: c(self.kill_xp) }
    }
}

impl Difficulty {
    /// The level of a code 0–4 (GetGameDifficulty's numbering).
    pub fn from_code(code: i64) -> Option<Difficulty> {
        usize::try_from(code).ok().and_then(|i| LEVELS.get(i)).copied()
    }

    /// GetGameDifficulty's number; Custom answers 2, Normal's, as scripts know only the
    /// five levels [design].
    pub fn code(self) -> usize {
        match self {
            Difficulty::Custom => 2,
            d => d as usize,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::VeryEasy => "Very Easy",
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
            Difficulty::VeryHard => "Very Hard",
            Difficulty::Custom => "Custom",
        }
    }

    /// The game settings' suffix [documented: `fDiffMultXPVE` … `fDiffMultXPVH`]; Custom
    /// has none (its multipliers are the player's).
    pub fn suffix(self) -> Option<&'static str> {
        match self {
            Difficulty::Custom => None,
            d => Some(["VE", "E", "N", "H", "VH"][d.code()]),
        }
    }
}

impl Game<'_> {
    /// Level `d`'s three multipliers, whichever level is chosen: the damage the player deals
    /// (`fDiffMultHPByPC<level>`), takes (`fDiffMultHPToPC<level>`) and the kill XP
    /// (`fDiffMultXP<level>`), from the load order or the defaults here; Custom's are its own.
    /// For the settings page's note, which names every level's.
    pub fn difficulty_multipliers(&self, d: Difficulty) -> CustomDifficulty {
        match d.suffix() {
            Some(s) => CustomDifficulty {
                dealt: setting(self, &format!("fDiffMultHPByPC{s}"), BY_PC[d.code()]),
                taken: setting(self, &format!("fDiffMultHPToPC{s}"), TO_PC[d.code()]),
                kill_xp: setting(self, &format!("fDiffMultXP{s}"), XP[d.code()]),
            },
            None => self.difficulty_custom,
        }
    }

    /// `fDiffMultHPByPC<level>` (Custom: its own): the damage the player deals is
    /// multiplied by it.
    pub fn damage_by_player_mult(&self) -> f64 {
        self.difficulty_multipliers(self.difficulty).dealt
    }

    /// `fDiffMultHPToPC<level>` (Custom: its own): the damage the player takes is
    /// multiplied by it.
    pub fn damage_to_player_mult(&self) -> f64 {
        self.difficulty_multipliers(self.difficulty).taken
    }

    /// `fDiffMultXP<level>` (Custom: its own): kill XP is multiplied by it.
    pub fn kill_xp_mult(&self) -> f64 {
        self.difficulty_multipliers(self.difficulty).kill_xp
    }

    /// The multiplier of damage from `attacker` (none: an explosion without an owner, a
    /// fall) to `target`: taken by the player, else dealt by the player, else 1 (spec §3).
    pub fn damage_mult(&self, attacker: Option<FormId>, target: FormId) -> f64 {
        if target == FormId::PLAYER_REF {
            self.damage_to_player_mult()
        } else if attacker == Some(FormId::PLAYER_REF) {
            self.damage_by_player_mult()
        } else {
            1.0
        }
    }

    /// Set the level; returns the line the engine logs.
    pub fn set_difficulty(&mut self, d: Difficulty) -> String {
        self.difficulty = d;
        let level = match d {
            Difficulty::Custom => d.name().to_string(),
            _ => format!("{} ({})", d.name(), d.code()),
        };
        format!(
            "difficulty: {level}: damage by the player x{:.2}, to the player x{:.2}, kill XP x{:.2}",
            self.damage_by_player_mult(),
            self.damage_to_player_mult(),
            self.kill_xp_mult()
        )
    }

    /// Set the Custom level's multipliers (brought into range); they apply while the level
    /// is Custom.
    pub fn set_custom_difficulty(&mut self, c: CustomDifficulty) {
        self.difficulty_custom = c.clamped();
    }

    /// A script's `SetGameDifficulty`: the level at once, and a request the engine takes as
    /// the player's choice (module notes).
    pub fn script_set_difficulty(&mut self, d: Difficulty) {
        self.difficulty = d;
        self.difficulty_request = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_by_code_and_suffix() {
        assert_eq!(Difficulty::from_code(0), Some(Difficulty::VeryEasy));
        assert_eq!(Difficulty::from_code(4), Some(Difficulty::VeryHard));
        assert_eq!(Difficulty::from_code(5), None, "Custom has no code of its own");
        assert_eq!(Difficulty::from_code(-1), None);
        assert_eq!(Difficulty::default().code(), 2);
        assert_eq!(Difficulty::VeryHard.suffix(), Some("VH"));
        assert_eq!(Difficulty::Easy.name(), "Easy");
        assert_eq!((Difficulty::Custom.code(), Difficulty::Custom.suffix(), Difficulty::Custom.name()), (2, None, "Custom"));
        assert_eq!(CHOICES[..5], LEVELS);
    }

    #[test]
    fn custom_multipliers_stay_in_range() {
        let c = CustomDifficulty { dealt: 0.1, taken: 9.0, kill_xp: f64::NAN }.clamped();
        assert_eq!(c, CustomDifficulty { dealt: CUSTOM_MIN, taken: CUSTOM_MAX, kill_xp: 1.0 });
        assert_eq!(CustomDifficulty::default(), CustomDifficulty { dealt: 1.0, taken: 1.0, kill_xp: 1.0 });
    }
}
