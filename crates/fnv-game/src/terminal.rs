//! Terminals as the game sees them (docs/spec-notes/hacking.md §1, §6): whether a
//! terminal opens straight away, must be hacked, refuses, or is locked for good; which
//! of its menu items are listed; and what choosing one does (its result script, its
//! note, its sub menu). The screens are the engine's; the hacking game itself is
//! [`crate::hacking`].

use crate::data::{terminal_flag, TerminalDef};
use crate::functions::actor::actor_value_capped;
use crate::game::{Game, RunCtx};
use crate::hacking::{self, Difficulty, Refusal, Settings};
use crate::trace::EventKind;
use fnv_sim::FormId;
use std::collections::BTreeMap;

/// What became of a terminal reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Hacked: it opens straight away from now on [documented: GECK wiki Lock, "if
    /// player successfully hacked terminal … will still have terminal access"].
    Hacked,
    /// Four wrong words: it can never be hacked again [documented: GECK wiki Lock,
    /// "if player failed to hack terminal … will still be locked out"; the owner,
    /// 2026-10-11].
    LockedOut,
}

/// Every terminal reference's [`Access`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Terminals {
    pub access: BTreeMap<FormId, Access>,
    /// ForceTerminalBack was called by the result script running now
    /// ([`choose`] hands it on as [`Chosen::back`]).
    pub back: bool,
}

/// What activating a terminal does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opening {
    /// Its menu: unlocked, hacked before, or the player has its password note.
    Menu,
    /// The hacking game with this many words.
    Hack { difficulty: Difficulty, words: u32 },
    Refused(Refusal),
    LockedOut,
}

/// The player's Science skill (0 to 100).
pub fn science(g: &mut Game) -> f32 {
    actor_value_capped(g, FormId::PLAYER_REF, crate::actor_values::SCIENCE) as f32
}

/// Whether the player holds the terminal's password note: among their notes, or carried
/// as an item.
pub fn has_password(g: &mut Game, t: &TerminalDef) -> bool {
    let Some(note) = t.password_note else { return false };
    g.player.notes.contains(&note) || g.world.item_count(FormId::PLAYER_REF, note).is_ok_and(|n| n > 0)
}

/// What activating the terminal reference `reference` of base `base` does, for a player
/// of Science skill `sci` ([`science`]).
pub fn opening(g: &mut Game, reference: FormId, base: FormId, sci: f32) -> Option<Opening> {
    let t = g.data.terminals.get(&base)?;
    let unlocked = t.has(terminal_flag::UNLOCKED);
    // [hypothesis, hacking.md §6] The password note opens a terminal without hacking,
    // even one locked out (GECK wiki Lock: the note keeps access); it is checked first.
    if unlocked || g.terminals.access.get(&reference) == Some(&Access::Hacked) || has_password(g, t) {
        return Some(Opening::Menu);
    }
    if g.terminals.access.get(&reference) == Some(&Access::LockedOut) {
        return Some(Opening::LockedOut);
    }
    // The Leveled flag is not applied: how the level would change with the player's
    // level is not documented [hacking.md §6].
    let difficulty = Difficulty::from_code(t.difficulty);
    let s = Settings::default();
    Some(match hacking::may_hack(sci, difficulty, &s) {
        Err(r) => Opening::Refused(r),
        Ok(()) => Opening::Hack { difficulty, words: hacking::word_count(sci, difficulty, &s) },
    })
}

/// The hacking game ended.
pub fn hacked(g: &mut Game, reference: FormId, success: bool) {
    let a = if success { Access::Hacked } else { Access::LockedOut };
    g.terminals.access.insert(reference, a);
}

/// The items of terminal `base` listed now, by index: those whose conditions hold, run
/// on the terminal reference with the player as target [hypothesis, hacking.md §6].
pub fn items(g: &mut Game, reference: FormId, base: FormId) -> Vec<usize> {
    let Some(t) = g.data.terminals.get(&base) else { return Vec::new() };
    let mut out = Vec::new();
    for (i, item) in t.items.iter().enumerate() {
        let label = format!("terminal 0x{:08X} item {i}", base.0);
        if g.conditions_pass(&item.conditions, Some(reference), Some(FormId::PLAYER_REF), &label) {
            out.push(i);
        }
    }
    out
}

/// What choosing an item showed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chosen {
    /// RNAM, shown under the menu.
    pub result: String,
    /// The terminal to show next as a sub menu.
    pub submenu: Option<FormId>,
    /// INAM, the note shown in the screen.
    pub note: Option<FormId>,
    /// The item's result script called ForceTerminalBack: the terminal goes back one
    /// level, or closes from its top level [documented: GECK wiki ForceTerminalBack].
    pub back: bool,
}

/// The player chooses item `item` of terminal `base` at reference `reference`: its result
/// script runs on the reference [hypothesis, hacking.md §6], an Add Note item gives its
/// note to the player [documented: xEdit flag name].
pub fn choose(g: &mut Game, reference: FormId, base: FormId, item: usize) -> Option<Chosen> {
    let data = g.data;
    let it = data.terminals.get(&base)?.items.get(item)?;
    g.terminals.back = false;
    if let Some(u) = it.unit {
        g.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(reference) });
        g.run_result(u, RunCtx { self_ref: Some(reference), action_ref: Some(FormId::PLAYER_REF), quest: None });
    }
    if it.flags & 1 != 0 {
        if let Some(n) = it.note {
            g.player.notes.insert(n);
        }
    }
    let back = std::mem::take(&mut g.terminals.back);
    Some(Chosen { result: it.result.clone(), submenu: it.submenu, note: it.note, back })
}

/// The server line of the header: "-Server 1-" to "-Server 10-" [documented: xEdit
/// TERM ServerType names]; the game's own text for each is a setting
/// (`sTerminalServerText<N>`), of which only the tenth is in the plugins.
pub fn server_text(server: u8) -> String {
    format!("-Server {}-", server as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_count_from_one() {
        assert_eq!(server_text(0), "-Server 1-");
        assert_eq!(server_text(5), "-Server 6-");
        assert_eq!(server_text(9), "-Server 10-");
    }
}
