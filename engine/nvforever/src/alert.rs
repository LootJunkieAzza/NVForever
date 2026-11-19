//! How actors in a combat state look (docs/design/alert.md §5): the state itself is the
//! game's (`fnv_game::awareness`, fed by `detection.rs`).
//!
//! - **Weapon**: an actor in the alerted animation state (a combat state, or SetAlert)
//!   draws its weapon; back in Normal it holsters it again [documented: Alert is "weapon
//!   out", GECK wiki Category:Detection]. Only actors drawn with a weapon in hand or
//!   holster (`--actor-weapons`, holstered by default) have one to draw. Its current
//!   package has its say too [documented: GECK wiki Packages, Flags tab]: "Weapon
//!   Drawn" ("Actor will have weapon out during package") draws it, and "Weapons
//!   Unequipped" ("will not show any visible weapon while running this package, even if
//!   he has one equipped") holsters it and hides it, unless the actor is in a combat
//!   state [hypothesis: a fight comes first] (owner's playtest list 5 item 34: Sunny ran
//!   to the geckos without her weapon drawn; VCG02SunnyTravelToWell1 has Weapon Drawn).
//!   An actor fleeing a fight holsters it until it fights again [owner 2026-10-17;
//!   docs/design/combat-ai.md §12.2.1]. Out of a fight, an actor about to play an idle
//!   that is not made for a drawn weapon (a talking gesture, a response's or a package's
//!   idle: `Game::idle_fits_weapon_out`) puts it away first and keeps it away while the
//!   idle plays (`idles.rs`) [design: owner 2026-11-11, playtest 119 item 155]. The same
//!   goes for furniture (`furniture.rs` plays its clips, not `idles.rs`): out of a fight,
//!   an actor walking up to furniture, on it or getting off has its weapon away, whatever
//!   its package's Weapon Drawn flag says [design, the same rule; owner's ship17 shot
//!   2026-11-13: a caravan guard, VCaravanMerc01Post188REF, whose VCaravanMojaveToFork6am
//!   drew her machete, sat on the floor with it in hand].
//! - **Warning** (`Game::warning_to`, not alerted): the actor draws its weapon and holds
//!   it while it says its warnings (GuardTrespass) [owner decision 2026-10-25: the
//!   original's warning guard drew and holstered his gun every 2.5 s (Gomorrah's
//!   cash-room guard, capture `warnings` cashroom_near, idles.md §14.3), which looks like
//!   an unintended quirk; draw and hold instead].
//! - **Head**: an Alert or Lost actor looks at where its latest alarm is (what it
//!   noticed, heard or last saw), at chest height; one in Combat looks at its target.
//!   The look is a [`LookOverride`] this module puts on and takes off again, never over
//!   one put there by something else (a conversation, a script's Look).
//! - **Body**: actors the AI moves (`--npc-packages`) turn and search there
//!   (`fnv_ai::alert`); the others stand where they are and only turn their heads.

use crate::actors::ActorWorld;
use crate::head_look::{LookAt, LookOverride};
use crate::limbs::ActorRig;
use crate::weapons::{WeaponHold, WeaponMode};
use bevy::prelude::*;
use fnv_game::awareness::State;
use fnv_game::package::flags;
use fnv_sim::FormId;
use nvf_play::player::EYE_HEIGHT;
use nvf_play::script::ScriptState;

pub struct AlertPlugin;

impl Plugin for AlertPlugin {
    fn build(&self, app: &mut App) {
        // [design, #2512] Flush package hiding before weapon-part visibility is composed.
        app.add_systems(Update, show.after(nvf_play::PlaySet::Scripts).before(crate::weapons::reparent));
    }
}

/// On an actor's root while this module turns its head.
#[derive(Component)]
struct AlertLook(LookAt);

/// [design: show] On an actor's root while its package hides its weapon (Weapons Unequipped).
#[derive(Component)]
pub(crate) struct WeaponHidden;

/// The chest, as a fraction of the eye height (as `detection.rs` measures).
const CHEST: f32 = 0.75;

type Actors<'w> = (Entity, &'w ActorRig, Option<&'w mut WeaponHold>, Option<&'w LookOverride>, Option<&'w AlertLook>, Has<WeaponHidden>, (Has<crate::furniture::SitOn>, Has<crate::furniture::OnFurniture>));

// Co-op puppets (coop.rs) draw and holster as their player does.
fn show(mut commands: Commands, script: Option<Res<ScriptState>>, world: Option<Res<ActorWorld>>, idles: Option<Res<crate::idles::Idles>>, mut actors: Query<Actors, (Without<crate::coop::Puppet>, Without<crate::coop::Remote>)>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let drawn_by_default = world.is_some_and(|w| w.weapon_mode == WeaponMode::Drawn);
    for (e, rig, hold, forced, ours, hidden, (sit_on, on_furniture)) in &mut actors {
        let id = FormId(rig.reference);
        if let Some(mut hold) = hold {
            let fighting = game.awareness.state(id).in_combat();
            let unequipped = !fighting && game.package_has(id, flags::WEAPONS_UNEQUIPPED);
            let fleeing = fighting && game.awareness.tactics.flights.contains_key(&id);
            let away = put_away(fighting, idles.as_deref().is_some_and(|i| i.holstering.contains_key(&id.0)), sit_on || on_furniture);
            let want = away.is_none() && (game.wants_weapon_out(id) || (drawn_by_default && !unequipped && !fleeing));
            if hold.test.is_none() && hold.drawn != want {
                hold.drawn = want;
                let why = if let Some(why) = away {
                    why
                } else if fleeing {
                    " (it flees)"
                } else if game.active_weapon_use(id).is_some() {
                    " (a weapon use)"
                } else if !fighting && want != game.draws_weapon(id) {
                    " (its package's flags)"
                } else if !fighting && want && game.warning_to(id).is_some() {
                    " (it warns the player)"
                } else {
                    ""
                };
                info!("alert: {} {id} {} its weapon{why}", game.data.name(id), if want { "draws" } else { "holsters" });
            }
            // Hidden while holstered under Weapons Unequipped; a creature's body weapon is
            // part of its body (its `Weapon` node carries body parts).
            let hide = unequipped && !hold.body && hold.anim.stage == fnv_actor::weapon::Stage::Holstered;
            if hide != hidden {
                // [design, #2512] reparent combines this marker with clip visibility
                // for model shapes, including parts mounted on separate back bones.
                if let Some(&part) = rig.nodes.get(hold.stance.weapon_node) {
                    commands.entity(part).try_insert(if hide { Visibility::Hidden } else { Visibility::Inherited });
                }
                if hide {
                    commands.entity(e).insert(WeaponHidden);
                } else {
                    commands.entity(e).remove::<WeaponHidden>();
                }
            }
        }
        let aware = game.awareness.get(id);
        let look = aware.and_then(|a| match a.state {
            State::Combat => a.combat_target().map(|(t, _)| if t == FormId::PLAYER_REF { LookAt::Player } else { LookAt::Actor(t.0) }),
            State::Alert | State::Lost => a.search_alarm().map(|x| LookAt::Point([x.at[0], x.at[1], x.at[2] + EYE_HEIGHT * CHEST])),
            State::Normal => None,
        });
        // Our look is on unless something else has put its own in its place since.
        let active = ours.is_some_and(|o| forced.is_some_and(|f| f.0 == o.0));
        match look {
            Some(at) if active && ours.is_some_and(|o| o.0 == at) => {}
            Some(at) if active || forced.is_none() => {
                commands.entity(e).insert((LookOverride(at), AlertLook(at)));
            }
            // Another's look comes first.
            Some(_) => {
                if ours.is_some() {
                    commands.entity(e).remove::<AlertLook>();
                }
            }
            None if active => {
                commands.entity(e).remove::<(LookOverride, AlertLook)>();
            }
            None if ours.is_some() => {
                commands.entity(e).remove::<AlertLook>();
            }
            None => {}
        }
    }
}

/// Why an actor out of a fight has its weapon away whatever its state and package say
/// (module notes): an idle not played with a weapon out (`idles.rs` holds it in
/// `Idles::holstering`), or furniture it walks up to, is on or gets off. `None` in a
/// fight, or with neither.
fn put_away(fighting: bool, for_idle: bool, furniture: bool) -> Option<&'static str> {
    if fighting {
        None
    } else if for_idle {
        Some(" (for an idle played without it)")
    } else if furniture {
        Some(" (it uses furniture)")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furniture_puts_the_weapon_away_out_of_a_fight() {
        // VCaravanMerc01Post188REF: Weapon Drawn by her package, sitting on the floor.
        assert_eq!(put_away(false, false, true), Some(" (it uses furniture)"));
        assert_eq!(put_away(false, true, true), Some(" (for an idle played without it)"));
        assert_eq!(put_away(false, true, false), Some(" (for an idle played without it)"));
        // A fight comes first; with neither, the state and package decide.
        assert_eq!(put_away(true, true, true), None);
        assert_eq!(put_away(false, false, false), None);
    }
}
