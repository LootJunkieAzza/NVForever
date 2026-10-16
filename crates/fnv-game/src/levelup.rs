//! Levelling up (docs/spec-notes/levelup.md): the rules of the level-up menu (1027,
//! `menus\levelup_menu.xml`) as plain data the engine draws, and what its Done writes.
//!
//! - [`advance`] is `AdvancePCLevel` (`AdvLevel`): the player's level goes up by one and
//!   the menu opens. [`open`] opens it for the level the player has now, for whatever
//!   raises the level by experience.
//! - The menu has two pages [documented: levelup_menu.xml `_CurrentPage`, `_EndPage`]:
//!   the skills ([`SkillSheet`]), then, at a level that brings a perk, the perks
//!   ([`PerkSheet`]).
//! - Each sheet starts from the player as the game holds them and changes nothing until
//!   its `apply`.

use crate::actor_values::{BIG_GUNS, INTELLIGENCE, SKILLS};
use crate::chargen;
use crate::combat::setting;
use crate::functions::actor;
use crate::game::{menus, Game};
use fnv_sim::FormId;
use std::collections::BTreeSet;

/// The highest a skill can be raised to [documented: levelup_menu.xml, the skill row's
/// `_MaxValue` 100].
pub const SKILL_MAX: i32 = 100;

/// `iLevelsPerPerk` when the masters do not set it [documented: GECK wiki, "iLevelsPerPerk
/// is 2 (default)"; observed: not among FalloutNV.esm's GMST].
pub const DEFAULT_LEVELS_PER_PERK: i32 = 2;

/// Perks chosen at a perk level [hypothesis: one; no screenshot of the perk page].
pub const PERKS_PER_LEVEL: usize = 1;

/// The skill points a level brings at a permanent Intelligence:
/// ⌊min(Intelligence, 9) × 0.5 + 10⌋, and one more when the level and Intelligence are
/// both even [documented: GECK wiki "Skill Rate", New Vegas; observed: 12 points at
/// level 3 on the owner's screenshot, which Intelligence 4 or 5 gives].
pub fn skill_points(intelligence: i32, level: i32) -> i32 {
    let base = (intelligence.min(9) as f64 * 0.5 + 10.0).floor() as i32;
    base + i32::from(level % 2 == 0 && intelligence % 2 == 0)
}

/// The skill points the player gets for reaching `level`.
pub fn points_for(g: &mut Game, level: i32) -> i32 {
    let int = actor::actor_value_base(g, FormId::PLAYER_REF, INTELLIGENCE).round() as i32;
    skill_points(int, level)
}

/// Whether reaching `level` brings a perk: every `iLevelsPerPerk` levels [documented:
/// GECK wiki, iLevelsPerPerk; hypothesis: counted from level 0, so levels 2, 4, 6 …].
pub fn perk_due(g: &Game, level: i32) -> bool {
    let every = setting(g, "iLevelsPerPerk", DEFAULT_LEVELS_PER_PERK as f64) as i32;
    every > 0 && level % every == 0
}

/// Open the level-up menu for the player's level now.
pub fn open(g: &mut Game) {
    g.open_menu(menus::LEVEL_UP);
}

/// `AdvancePCLevel`: the player's level goes up by one and the level-up menu opens
/// [documented: GECK wiki AdvancePCLevel]. Experience is left as it is [hypothesis: the
/// page does not say]. The new level.
pub fn advance(g: &mut Game) -> i32 {
    let level = g.player_level() as i32 + 1;
    g.player.level = Some(level);
    open(g);
    level
}

/// The skills in the menu's order: every skill but Big Guns, by name [observed: the
/// owner's screenshots of the level-up menu list the thirteen of the tag-skill menu in
/// the same order].
pub fn menu_skills(g: &Game) -> Vec<u16> {
    let mut skills: Vec<u16> = SKILLS.filter(|&c| c != BIG_GUNS).collect();
    skills.sort_by_key(|&c| chargen::skill_name(g, c).to_lowercase());
    skills
}

// ---- the skills page ---------------------------------------------------------------------

/// The skills page: the points to spend and where they went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSheet {
    pub skills: Vec<u16>,
    /// Each skill's value when the menu opened: the current value, effects included
    /// [documented: the row's `_BaseValue` plus `_ExtraValue`, "values added … from
    /// perks, chems, and other enhancers"].
    pub start: Vec<i32>,
    /// The points spent on each skill.
    pub spent: Vec<i32>,
    /// What one point adds to each skill: `iSkillPointsTagSkillMult` for a tagged skill,
    /// else 1 [documented: GECK wiki Skill Settings, default 1; observed: not in the
    /// masters].
    pub per_point: Vec<i32>,
    /// The points the level brings.
    pub points: i32,
}

impl SkillSheet {
    pub fn new(skills: Vec<u16>, start: Vec<i32>, per_point: Vec<i32>, points: i32) -> SkillSheet {
        let spent = vec![0; skills.len()];
        SkillSheet { skills, start, spent, per_point, points }
    }

    /// The sheet for `level`, from the player as the game holds them.
    pub fn of_player(g: &mut Game, level: i32) -> SkillSheet {
        let skills = menu_skills(g);
        let tag_mult = setting(g, "iSkillPointsTagSkillMult", 1.0).max(1.0) as i32;
        let start = skills.iter().map(|&s| actor::actor_value(g, FormId::PLAYER_REF, s).unwrap_or(0.0).round() as i32).collect();
        let per_point = skills.iter().map(|s| if g.player.tag_skills.contains(s) { tag_mult } else { 1 }).collect();
        SkillSheet::new(skills, start, per_point, points_for(g, level))
    }

    pub fn remaining(&self) -> i32 {
        self.points - self.spent.iter().sum::<i32>()
    }

    /// Row `i`'s value as the menu shows it, never past [`SKILL_MAX`] (points that would
    /// push it over are kept but add nothing [documented: the row's `_OverflowValue`]).
    pub fn value(&self, i: usize) -> i32 {
        let raised = self.start[i] + self.spent[i] * self.per_point[i];
        raised.min(SKILL_MAX.max(self.start[i]))
    }

    /// Whether row `i`'s "+" is shown: points are left and the skill is under the most
    /// [documented: LUM_Template_RightArrow's `visible`].
    pub fn can_raise(&self, i: usize) -> bool {
        i < self.skills.len() && self.remaining() > 0 && self.value(i) < SKILL_MAX
    }

    /// Whether row `i`'s "−" is shown: points were spent on it in this menu (a skill
    /// never goes under its value at the start) [documented: LUM_Template_LeftArrow's
    /// `visible` and its comment].
    pub fn can_lower(&self, i: usize) -> bool {
        self.spent.get(i).is_some_and(|&n| n > 0)
    }

    pub fn raise(&mut self, i: usize) -> bool {
        let ok = self.can_raise(i);
        if ok {
            self.spent[i] += 1;
        }
        ok
    }

    pub fn lower(&mut self, i: usize) -> bool {
        let ok = self.can_lower(i);
        if ok {
            self.spent[i] -= 1;
        }
        ok
    }

    /// Reset: every point back [documented: LUM_ResetButton, "Resets all distributed
    /// points"].
    pub fn reset(&mut self) {
        self.spent.iter_mut().for_each(|n| *n = 0);
    }

    /// Whether Continue / Done may be pressed: every point spent [documented:
    /// LUM_ContinueButton's `target`, the points assigned equal to the most], or no skill
    /// left to raise [design: else a player with every skill at 100 could not leave].
    pub fn complete(&self) -> bool {
        self.remaining() <= 0 || !(0..self.skills.len()).any(|i| self.value(i) < SKILL_MAX)
    }

    /// What each skill gains, as (skill, amount), leaving out those that gain nothing.
    pub fn gains(&self) -> Vec<(u16, i32)> {
        (0..self.skills.len()).map(|i| (self.skills[i], self.value(i) - self.start[i])).filter(|&(_, d)| d != 0).collect()
    }

    /// The points go into the skills (`PlayerState::skill_points`).
    pub fn apply(&self, g: &mut Game) {
        self.shift(g, 1.0);
    }

    /// Take back what [`apply`](Self::apply) gave (Back from the perks page).
    pub fn undo(&self, g: &mut Game) {
        self.shift(g, -1.0);
    }

    fn shift(&self, g: &mut Game, sign: f64) {
        for (skill, d) in self.gains() {
            let e = g.player.skill_points.entry(skill).or_default();
            *e += sign * d as f64;
            if *e == 0.0 {
                g.player.skill_points.remove(&skill);
            }
        }
    }
}

/// The skills page's counter [observed: "ASSIGN 12 SKILL POINTS", "ASSIGN 7 SKILL POINTS",
/// nothing with every point spent; hypothesis: "ASSIGN 1 SKILL POINT"].
pub fn skill_counter(remaining: i32) -> String {
    match remaining {
        n if n <= 0 => String::new(),
        1 => "ASSIGN 1 SKILL POINT".into(),
        n => format!("ASSIGN {n} SKILL POINTS"),
    }
}

/// The menu's title [observed: "WELCOME TO LEVEL 3"].
pub fn title(level: i32) -> String {
    format!("WELCOME TO LEVEL {level}")
}

// ---- the perks page ----------------------------------------------------------------------

/// The perks page: the perks the player may take and the one taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerkSheet {
    pub perks: Vec<FormId>,
    pub chosen: BTreeSet<FormId>,
    pub count: usize,
}

impl PerkSheet {
    /// Every playable, not hidden perk that is not a trait, whose least level the player
    /// has reached, of which the player has a rank to come, and whose requirements the
    /// player meets, by name [documented: xEdit PERK DATA Trait, Min Level, Ranks,
    /// Playable, Hidden; hypothesis: the original lists only perks the player can take].
    pub fn of_player(g: &mut Game, level: i32) -> PerkSheet {
        let offered: Vec<FormId> = g
            .data
            .perks
            .iter()
            .filter(|(id, p)| !p.is_trait && p.playable && !p.hidden && p.min_level as i32 <= level && g.player.perks.get(id).copied().unwrap_or(0) < p.ranks.max(1))
            .map(|(&id, _)| id)
            .collect();
        let mut perks: Vec<FormId> = offered.into_iter().filter(|&p| chargen::requirements_met(g, p)).collect();
        perks.sort_by_key(|id| (g.data.perks[id].name.to_lowercase(), *id));
        PerkSheet { perks, chosen: BTreeSet::new(), count: PERKS_PER_LEVEL }
    }

    /// Take a perk while fewer than the count are taken, or give a taken one back.
    /// Whether anything changed.
    pub fn toggle(&mut self, p: FormId) -> bool {
        if self.chosen.remove(&p) {
            return true;
        }
        if self.chosen.len() < self.count && self.perks.contains(&p) {
            self.chosen.insert(p);
            return true;
        }
        false
    }

    pub fn left(&self) -> usize {
        self.count.saturating_sub(self.chosen.len())
    }

    /// Whether Done may be pressed: the perks taken, or none on offer [design].
    pub fn complete(&self) -> bool {
        self.left() == 0 || self.perks.is_empty()
    }

    /// Done: one more rank of each perk taken.
    pub fn apply(&self, g: &mut Game) {
        for p in &self.chosen {
            *g.player.perks.entry(*p).or_default() += 1;
        }
    }
}

/// The perks page's counter [hypothesis: no screenshot of it; worded as the skills
/// page's].
pub fn perk_counter(left: usize) -> String {
    match left {
        0 => String::new(),
        1 => "CHOOSE 1 PERK".into(),
        n => format!("CHOOSE {n} PERKS"),
    }
}

/// The text beside a perk [hypothesis: as the trait menu's, "Req:", "Ranks:", an empty
/// line, the description; the rank the player would reach of the most].
pub fn perk_info(requirements: &str, rank: u8, ranks: u8, description: &str) -> String {
    format!("Req: {requirements}\nRanks: {rank}/{ranks}\n\n{}", description.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_per_level_follow_the_skill_rate() {
        // ⌊min(INT, 9) / 2 + 10⌋ ...
        assert_eq!(skill_points(1, 3), 10);
        assert_eq!(skill_points(5, 3), 12);
        assert_eq!(skill_points(9, 3), 14);
        assert_eq!(skill_points(10, 3), 14);
        // ... and one more when the level and Intelligence are both even.
        assert_eq!(skill_points(4, 4), 13);
        assert_eq!(skill_points(4, 3), 12);
        assert_eq!(skill_points(5, 4), 12);
        assert_eq!(skill_points(10, 2), 15);
    }

    /// The owner's screenshots: twelve points, all spent on Science (17 → 22 with seven
    /// left, then 29 with none).
    #[test]
    fn the_screenshot_sheet() {
        let mut s = SkillSheet::new(vec![32, 40], vec![17, 17], vec![1, 1], 12);
        assert_eq!(skill_counter(s.remaining()), "ASSIGN 12 SKILL POINTS");
        assert!(!s.can_lower(1) && s.can_raise(1) && !s.complete());
        for _ in 0..5 {
            assert!(s.raise(1));
        }
        assert_eq!((s.value(1), s.remaining()), (22, 7));
        assert_eq!(skill_counter(s.remaining()), "ASSIGN 7 SKILL POINTS");
        assert!(s.can_lower(1) && !s.can_lower(0));
        for _ in 0..7 {
            assert!(s.raise(1));
        }
        assert_eq!((s.value(1), s.remaining()), (29, 0));
        assert_eq!(skill_counter(0), "");
        // No "+" anywhere with nothing left; Done lights.
        assert!(!s.can_raise(0) && !s.raise(0) && s.complete());
        assert_eq!(s.gains(), vec![(40, 12)]);
        s.reset();
        assert_eq!((s.value(1), s.remaining()), (17, 12));
        assert_eq!(title(3), "WELCOME TO LEVEL 3");
    }

    #[test]
    fn a_skill_stops_at_100() {
        let mut s = SkillSheet::new(vec![32, 40], vec![98, 100], vec![2, 1], 10);
        assert!(!s.can_raise(1));
        assert!(s.raise(0));
        assert_eq!(s.value(0), 100);
        assert!(!s.raise(0), "100 is the top");
        // A point that pushed past the top gains nothing.
        let s = SkillSheet { spent: vec![2, 0], ..s };
        assert_eq!((s.value(0), s.gains()), (100, vec![(32, 2)]));
        // Every skill at the top: Done may be pressed with points left.
        let full = SkillSheet::new(vec![32], vec![100], vec![1], 10);
        assert!(full.complete());
    }

    #[test]
    fn one_perk_is_taken() {
        let mut p = PerkSheet { perks: vec![FormId(1), FormId(2)], chosen: BTreeSet::new(), count: PERKS_PER_LEVEL };
        assert!(!p.complete());
        assert_eq!(perk_counter(p.left()), "CHOOSE 1 PERK");
        assert!(p.toggle(FormId(1)));
        assert!(!p.toggle(FormId(2)), "a second perk is refused");
        assert!(p.complete());
        assert!(p.toggle(FormId(1)) && p.chosen.is_empty());
        assert!(!p.toggle(FormId(3)), "a perk not on offer");
        assert!(PerkSheet { perks: vec![], chosen: BTreeSet::new(), count: 1 }.complete());
        assert_eq!(perk_info("STR >= 5", 1, 3, " Hits hard. "), "Req: STR >= 5\nRanks: 1/3\n\nHits hard.");
    }
}
