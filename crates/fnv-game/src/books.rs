//! Skill books (docs/spec-notes/skill-books.md): an actor reads a book whose BOOK DATA
//! "Skill" names a skill (`fnv_sim::forms::Item::teaches`), and the skill's permanent
//! value rises by the book's points, which the perks' Adjust Book Skill Points entry
//! point (11, Comprehension) changes; the player's reading counts in Books Read.
//!
//! Taking the book out of the inventory is the caller's: the inventory menu's use and
//! EquipItem (`fnv_sim::World::equip_at` uses a skill book up).

use crate::functions::actor as fa;
use crate::game::Game;
use crate::trace::EventKind;
use fnv_sim::FormId;

/// The points a book gives before the perks when the masters carry no setting
/// [hypothesis: the masters' `fBookPerkBonus` is 3.0, skill-books.md §3].
pub const DEFAULT_BOOK_POINTS: f64 = 3.0;

/// The skills' top: no book raises a skill's permanent value past it (`levelup::SKILL_MAX`)
/// [hypothesis, skill-books.md §3].
const SKILL_MAX: f64 = crate::levelup::SKILL_MAX as f64;

/// What reading one book did.
#[derive(Debug, Clone, PartialEq)]
pub struct BookRead {
    pub book: FormId,
    /// The skill's actor value code.
    pub skill: u16,
    /// The points the book was worth after the perks (before the skills' top).
    pub worth: f64,
    /// The points the skill's permanent value rose by.
    pub points: f64,
    /// The skill's current value before and after.
    pub before: f64,
    pub after: f64,
    /// Perks whose entry point 11 changed the points.
    pub perks: Vec<FormId>,
}

/// The points a book raises a skill whose permanent value is `permanent` by, given the
/// points it is worth: never past the top, never below 0, whole points [hypothesis].
pub fn points_given(worth: f64, permanent: f64) -> f64 {
    worth.trunc().min(SKILL_MAX - permanent).max(0.0)
}

impl Game<'_> {
    /// The skill `book` teaches, if it is a skill book.
    pub fn book_skill(&self, book: FormId) -> Option<u16> {
        self.world.forms.item(book).and_then(|it| it.teaches)
    }

    /// `actor` reads the skill book `book` (already taken from its inventory, or never
    /// held): the skill's permanent modifier rises (ModActorValue's pool, as the GECK
    /// wiki's "Permanent Pool" names skill books for), and the player's Books Read
    /// counts one. `None` when the book teaches no skill.
    pub fn read_book(&mut self, actor: FormId, book: FormId) -> Option<BookRead> {
        let skill = self.book_skill(book)?;
        let base = self.world.setting_f32("fBookPerkBonus").map_or(DEFAULT_BOOK_POINTS, f64::from);
        let (worth, by) = self.entry_point(actor, crate::perks::ADJUST_BOOK_SKILL_POINTS, base);
        let perks: Vec<FormId> = by.iter().map(|a| a.perk).collect();
        let before = fa::actor_value(self, actor, skill).unwrap_or(0.0);
        let permanent = fa::permanent_value(self, actor, skill).unwrap_or(0.0);
        let points = points_given(worth, permanent);
        if points != 0.0 {
            let _ = fa::add_modifier(self, actor, skill, points);
        }
        let after = fa::actor_value(self, actor, skill).unwrap_or(before);
        if actor == FormId::PLAYER_REF {
            self.count_stat(crate::misc_stats::BOOKS_READ, 1.0);
        }
        self.emit(EventKind::BookRead { actor, book, skill, points, value: after, perks: perks.clone() });
        Some(BookRead { book, skill, worth, points, before, after, perks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_stop_at_the_top() {
        assert_eq!(points_given(3.0, 15.0), 3.0);
        assert_eq!(points_given(4.0, 15.0), 4.0, "Comprehension's Add 1");
        assert_eq!(points_given(4.0, 98.0), 2.0);
        assert_eq!(points_given(3.0, 100.0), 0.0);
        assert_eq!(points_given(3.0, 120.0), 0.0, "never takes away");
        assert_eq!(points_given(3.9, 10.0), 3.0, "whole points");
    }
}
