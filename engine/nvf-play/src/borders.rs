//! World borders in play (`fnv_game::borders`): outdoors, a player who steps past the
//! border regions of their worldspace is put back where they last stood inside and told
//! the border's warning in the corner [hypothesis: the original stops the player at the
//! edge; that the warning is a corner message is ours]. Under the player's World
//! Borders: Warning Only tweak (`bDisableRegionBordersKeepMessage`) the warning is given
//! and the player goes on [design: docs/design/tweaks.md]. The console's ToggleCollision
//! passes borders [design].
//!
//! The warning comes again no sooner than [`WARN_AGAIN`] after the last [design].
//! Log lines: `borders: the player is past the border of worldspace <id> (<x>, <y>), put
//! back` (or `, let go (tweak)`).

use crate::corner_messages::CornerMessages;
use crate::phys::PhysState;
use crate::player::{NoClip, Player};
use crate::script::ScriptState;
use crate::{Location, PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

pub struct BordersPlugin;

impl Plugin for BordersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, keep_inside.after(PlaySet::Physics).before(PlaySet::Activation).run_if(in_state(PlayState::Playing)));
    }
}

/// Seconds before the border warns again [design].
const WARN_AGAIN: f32 = 3.0;

/// Where the player last stood inside, and when the border last warned.
#[derive(Default)]
struct Watch {
    inside: Option<(u32, [f32; 3])>,
    warned: Option<f32>,
}

#[allow(clippy::too_many_arguments)]
fn keep_inside(
    time: Res<Time<Real>>,
    script: Option<Res<ScriptState>>,
    location: Option<Res<Location>>,
    (mut player, phys, noclip): (ResMut<Player>, Option<ResMut<PhysState>>, Option<Res<NoClip>>),
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    mut corner: Option<ResMut<CornerMessages>>,
    mut watch: Local<Watch>,
) {
    let (Some(rt), Some(loc)) = (script.as_deref().and_then(|s| s.rt.as_ref()), location.as_deref()) else { return };
    if loc.interior.is_some() || noclip.is_some_and(|n| n.0) {
        watch.inside = None;
        return;
    }
    let feet = player.feet;
    if !rt.game.past_border(FormId(loc.world), feet[0], feet[1]) {
        watch.inside = Some((loc.world, feet));
        return;
    }
    let message_only = tweaks.as_deref().is_some_and(|t| t.region_borders_message_only);
    let now = time.elapsed_secs();
    if watch.warned.is_none_or(|t| now - t >= WARN_AGAIN) {
        watch.warned = Some(now);
        info!("borders: the player is past the border of worldspace {:08X} ({:.0}, {:.0}), {}", loc.world, feet[0], feet[1], if message_only { "let go (tweak)" } else { "put back" });
        if let Some(c) = corner.as_deref_mut() {
            c.push(rt.game.border_message().into());
        }
    }
    if message_only {
        return;
    }
    // Back to where the player last stood inside this worldspace; arrived past the
    // border (a door, a script's move), the player stays where the game put them.
    let Some((_, back)) = watch.inside.filter(|(w, _)| *w == loc.world) else { return };
    if let Some(mut p) = phys {
        p.place_player(back, 60.0, 1000.0);
        player.feet = p.player.as_ref().map_or(back, |c| c.feet);
    } else {
        player.feet = back;
    }
}
