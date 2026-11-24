//! Poker tables in the world (docs/design/poker.md): the original has no poker, so
//! NVForever turns some of the Strip casinos' blackjack tables into poker tables
//! [design: the owner's ask, 2026-11-04; tables added on open floor were tried and left
//! out, as no place found looked like the casinos' own].
//!
//! - A swapped table ([`SWAPPED`]) is a placed blackjack table: its script still asks for
//!   blackjack (ShowBlackJackMenuParams), and the engine opens poker at its casino instead
//!   (`crate::blackjack`'s `scripts_open`); it goes by "Poker Table" ([`NAME`]), and the
//!   engine draws a poker table's top over its blackjack felt (nvforever `poker_view.rs`).
//! - Poker plays for caps at every table; the casino gives the cards and the felt.

use crate::poker::Casino;
use crate::sim::SimState;
use fnv_sim::FormId;

/// What a poker table is called at the crosshair.
pub const NAME: &str = "Poker Table";

/// Blackjack tables that are poker tables, and their casino [observed: FalloutNV.esm, the
/// casinos' blackjack activators: the Ultra-Luxe has six, Gomorrah four, the Tops three].
pub const SWAPPED: [(u32, Casino); 4] = [
    // The Ultra-Luxe: two of its six, the pair by the north wall.
    (0x0017_1653, Casino::UltraLuxe),
    (0x0011_887B, Casino::UltraLuxe),
    // Gomorrah: one of its four.
    (0x0017_11A6, Casino::Gomorrah),
    // The Tops: one of its three.
    (0x0015_1E68, Casino::Tops),
];

/// The poker casino of a table reference, if it is a poker table.
pub fn casino_of(_sim: &SimState, table: FormId) -> Option<Casino> {
    SWAPPED.iter().find(|(r, _)| *r == table.0).map(|(_, c)| *c)
}

pub fn is_poker_table(sim: &SimState, table: FormId) -> bool {
    casino_of(sim, table).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_casino_keeps_blackjack_tables_of_its_own() {
        for (c, of) in [(Casino::UltraLuxe, 6), (Casino::Gomorrah, 4), (Casino::Tops, 3)] {
            let n = SWAPPED.iter().filter(|(_, k)| *k == c).count();
            assert!(n >= 1 && n * 2 < of + 1, "{c:?}: {n} of {of}");
        }
    }
}
