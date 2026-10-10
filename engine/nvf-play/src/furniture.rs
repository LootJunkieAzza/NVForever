//! The player on furniture (docs/spec-notes/furniture.md). Activating a piece of
//! furniture asks for it ([`UseFurniture`]); whoever draws the player's body (nvforever's
//! `furniture.rs`) plays the clips and holds the player meanwhile ([`PlayerSeat`]):
//! movement, jumping and the character's collision are off, and pressing a movement
//! control asks to get up [design: how the original game ends sitting is not captured;
//! moving is what its players know].

use bevy::prelude::*;

/// The player activated a piece of furniture (its default activation).
#[derive(Message, Clone, Copy, Debug)]
pub struct UseFurniture {
    pub reference: u32,
}

/// Furniture holding the player.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct PlayerSeat {
    /// The feet (game space) while furniture holds the player.
    pub hold: Option<[f32; 3]>,
    /// A movement control was pressed while held; cleared by whoever acts on it.
    pub leave: bool,
}

/// Tell the script runtime which furniture `actor` uses, so GetSitting and the other
/// furniture functions answer for it: `using` is the furniture reference, the marker ID
/// the actor entered by, and the phase value (`fnv_actor::furniture::state`); `None`
/// when the actor is off it.
pub fn set_use(script: &mut crate::script::ScriptState, actor: u32, using: Option<(u32, u8, u8)>) {
    use fnv_game::game::FurnitureUse;
    use fnv_sim::FormId;
    let using = using.map(|(reference, marker_id, state)| FurnitureUse { reference: FormId(reference), marker_id, state });
    // No runtime (scripts off or still loading): nothing to tell.
    if let Some(rt) = script.rt.as_mut() {
        rt.game.set_furniture_use(FormId(actor), using);
    }
}
