//! The game's difficulty (docs/spec-notes/difficulty.md): five levels that scale the
//! damage the player deals and takes and the XP for a kill.
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
}

/// Every level, by code.
pub const LEVELS: [Difficulty; 5] = [Difficulty::VeryEasy, Difficulty::Easy, Difficulty::Normal, Difficulty::Hard, Difficulty::VeryHard];

/// `fDiffMultHPByPC<level>` when the load order lacks it, by code [hypothesis: spec §3.6].
pub const BY_PC: [f64; 5] = [2.0, 1.5, 1.0, 0.75, 0.5];
/// `fDiffMultHPToPC<level>` when the load order lacks it, by code [hypothesis: spec §3.6].
pub const TO_PC: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];
/// `fDiffMultXP<level>` when the load order lacks it, by code [documented: GECK wiki
/// Experience Settings defaults].
pub const XP: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];

impl Difficulty {
    /// The level of a code 0–4 (GetGameDifficulty's numbering).
    pub fn from_code(code: i64) -> Option<Difficulty> {
        usize::try_from(code).ok().and_then(|i| LEVELS.get(i)).copied()
    }

    /// GetGameDifficulty's number.
    pub fn code(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        ["Very Easy", "Easy", "Normal", "Hard", "Very Hard"][self.code()]
    }

    /// The game settings' suffix [documented: `fDiffMultXPVE` … `fDiffMultXPVH`].
    pub fn suffix(self) -> &'static str {
        ["VE", "E", "N", "H", "VH"][self.code()]
    }
}

impl Game<'_> {
    /// `fDiffMultHPByPC<level>`: the damage the player deals is multiplied by it.
    pub fn damage_by_player_mult(&self) -> f64 {
        let d = self.difficulty;
        setting(self, &format!("fDiffMultHPByPC{}", d.suffix()), BY_PC[d.code()])
    }

    /// `fDiffMultHPToPC<level>`: the damage the player takes is multiplied by it.
    pub fn damage_to_player_mult(&self) -> f64 {
        let d = self.difficulty;
        setting(self, &format!("fDiffMultHPToPC{}", d.suffix()), TO_PC[d.code()])
    }

    /// `fDiffMultXP<level>`: kill XP is multiplied by it.
    pub fn kill_xp_mult(&self) -> f64 {
        let d = self.difficulty;
        setting(self, &format!("fDiffMultXP{}", d.suffix()), XP[d.code()])
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
        format!(
            "difficulty: {} ({}): damage by the player x{:.2}, to the player x{:.2}, kill XP x{:.2}",
            d.name(),
            d.code(),
            self.damage_by_player_mult(),
            self.damage_to_player_mult(),
            self.kill_xp_mult()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_by_code_and_suffix() {
        assert_eq!(Difficulty::from_code(0), Some(Difficulty::VeryEasy));
        assert_eq!(Difficulty::from_code(4), Some(Difficulty::VeryHard));
        assert_eq!(Difficulty::from_code(5), None);
        assert_eq!(Difficulty::from_code(-1), None);
        assert_eq!(Difficulty::default().code(), 2);
        assert_eq!(Difficulty::VeryHard.suffix(), "VH");
        assert_eq!(Difficulty::Easy.name(), "Easy");
    }
}
