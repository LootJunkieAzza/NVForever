//! Where the player may fast travel from (docs/spec-notes/fast-travel.md §2): the rule the
//! Pip-Boy's World Map (`nvf_play::pipboy::data::travel`) and the scripts' check
//! (`Game::can_fast_travel`) share.

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// CELL `DATA` bit 2, "Invert Fast Travel Behavior" [documented: GECK wiki GetCellFlag and
/// SetCellFlag; fopdoc CELL]: it turns round what the cell's kind does by itself.
pub const CELL_INVERT_TRAVEL: u32 = 1 << 2;
/// CELL `DATA` bit 7, "Behave Like Exterior" [documented: GECK wiki GetCellFlag; xEdit].
pub const CELL_LIKE_EXTERIOR: u32 = 1 << 7;
/// WRLD `DATA` bit 1, "Can't Fast-Travel" [documented: GECK wiki GetWorldspaceFlag; xEdit].
pub const WORLD_NO_TRAVEL: u32 = 1 << 1;

/// Whether a place forbids fast travel, by its cell's and its worldspace's `DATA` flags
/// (`world` `None` indoors): an interior does [documented: GECK wiki EnableFastTravel,
/// "inside an interior cell, they will still be not permitted"], an exterior does not;
/// [`CELL_INVERT_TRAVEL`] turns that round, and [`WORLD_NO_TRAVEL`] forbids the whole
/// worldspace. An interior that behaves like an exterior counts as one [hypothesis].
pub fn place_forbids(cell: u32, world: Option<u32>) -> bool {
    let indoors = world.is_none() && cell & CELL_LIKE_EXTERIOR == 0;
    indoors != (cell & CELL_INVERT_TRAVEL != 0) || world.is_some_and(|w| w & WORLD_NO_TRAVEL != 0)
}

impl Game<'_> {
    /// [`place_forbids`] for `cell` (`None`: no record there) of worldspace `world` (`None`:
    /// an interior), as the scripts left the flags (SetCellFlag, SetWorldspaceFlag).
    pub fn place_forbids_travel(&self, cell: Option<FormId>, world: Option<FormId>) -> bool {
        let flags = |f: FormId| match crate::nvse::record_value(self, f, "DATA", "") {
            Some(XVal::Num(x)) => x as u32,
            _ => 0,
        };
        place_forbids(cell.map_or(0, flags), world.map(flags))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interior_forbids_travel_and_the_cell_flag_inverts_it() {
        assert!(place_forbids(1, None));
        assert!(!place_forbids(1 | CELL_INVERT_TRAVEL, None));
        assert!(!place_forbids(1 | CELL_LIKE_EXTERIOR, None), "a fake exterior is as one");
        assert!(place_forbids(1 | CELL_LIKE_EXTERIOR | CELL_INVERT_TRAVEL, None));
        assert!(!place_forbids(0, Some(0)));
        assert!(place_forbids(CELL_INVERT_TRAVEL, Some(0)));
        assert!(place_forbids(0, Some(WORLD_NO_TRAVEL)));
        assert!(place_forbids(CELL_INVERT_TRAVEL, Some(WORLD_NO_TRAVEL)), "the worldspace's flag holds");
    }
}
