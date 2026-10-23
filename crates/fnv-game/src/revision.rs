//! The character revision offered when the player first leaves Goodsprings (quest VCG04
//! "Player Character Revision", its activators' script VCG04ActivatorScript): it keeps
//! the player's progress (docs/spec-notes/chargen.md §9).
//!
//! The original's script [observed: FalloutNV.esm SCPT 001164A4, `fnvtool script
//! decompile`]: on "revise" it calls `player.SetLevel 1`, `ResetXP` and `RemovePerk` on
//! the ten traits and eighteen perks of the early levels, shows the race menu, then sets
//! each S.P.E.C.I.A.L. to 5 with `SetSPECIALPoints 40`, each skill to 0 with
//! `SetActorValue`, opens the tag-skill menu (`SetTagSkills 3`) and the trait menu; on
//! "done" it gives the experience back (`RewardXP` of the XP the player had), so the
//! player levels up again and spends the points and perks anew.
//!
//! Here the revision keeps what the player has [design: the owner's wish, playtest list
//! 7 item 1: "rebuild character should retain our currently selected perks and our
//! skill levels ... minus our 3 tag skills that are reapplied"]. From that script only:
//!
//! - `SetLevel`, `ResetXP` and the closing `RewardXP` on the player do nothing (level and
//!   experience stay, so no level-up comes again);
//! - `RemovePerk` takes traits only (the trait menu that follows offers them again); the
//!   other perks stay;
//! - `SetActorValue` of a skill on the player does nothing: a player's skill is its
//!   formula over the (new) S.P.E.C.I.A.L., the tag bonus of the (new) tag skills and
//!   the points spent at level-ups (`crate::actor_values`), so the points stay and the
//!   rest follows the new choices.
//!
//! The S.P.E.C.I.A.L. reset, the race, tag-skill and trait menus run as the script says.
//! Each kept call is logged `revision: keeps ...`.

use crate::functions::Ctx;
use crate::game::Game;
use fnv_sim::FormId;

/// The unit of VCG04ActivatorScript [observed: FalloutNV.esm 001164A4, the script of
/// the activators VCG04 places at Goodsprings' edges].
pub const REVISION_UNIT: &str = "FalloutNV.esm|SCPT 0x001164A4";

/// The call is the revision script's.
pub fn by_revision(ctx: &Ctx) -> bool {
    ctx.unit.eq_ignore_ascii_case(REVISION_UNIT)
}

/// The revision script's call on the player that is kept from resetting (`what` names
/// it in the log): true when the caller is to do nothing.
pub fn keeps(ctx: &Ctx, subject: FormId, what: impl FnOnce() -> String) -> bool {
    let kept = subject == FormId::PLAYER_REF && by_revision(ctx);
    if kept {
        tracing::info!("revision: keeps {}", what());
    }
    kept
}

/// Whether the revision keeps `perk` (anything but a trait); logged when the player has
/// it.
pub fn keeps_perk(g: &Game, ctx: &Ctx, subject: FormId, perk: FormId) -> bool {
    let is_trait = g.data.perks.get(&perk).is_some_and(|p| p.is_trait);
    let kept = !is_trait && subject == FormId::PLAYER_REF && by_revision(ctx);
    if kept && g.player.perks.contains_key(&perk) {
        tracing::info!("revision: keeps the perk {}", g.data.name(perk));
    }
    kept
}
