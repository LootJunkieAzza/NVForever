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
//!   docs/design/combat-ai.md §12.2.1].
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
        app.add_systems(Update, show.after(nvf_play::PlaySet::Scripts));
    }
}

/// On an actor's root while this module turns its head.
#[derive(Component)]
struct AlertLook(LookAt);

/// On an actor's root while its package hides its weapon (Weapons Unequipped).
#[derive(Component)]
struct WeaponHidden;

/// The chest, as a fraction of the eye height (as `detection.rs` measures).
const CHEST: f32 = 0.75;

type Actors<'w> = (Entity, &'w ActorRig, Option<&'w mut WeaponHold>, Option<&'w LookOverride>, Option<&'w AlertLook>, Has<WeaponHidden>);

fn show(mut commands: Commands, script: Option<Res<ScriptState>>, world: Option<Res<ActorWorld>>, mut actors: Query<Actors>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let drawn_by_default = world.is_some_and(|w| w.weapon_mode == WeaponMode::Drawn);
    for (e, rig, hold, forced, ours, hidden) in &mut actors {
        let id = FormId(rig.reference);
        if let Some(mut hold) = hold {
            let fighting = game.awareness.state(id).in_combat();
            let unequipped = !fighting && game.package_has(id, flags::WEAPONS_UNEQUIPPED);
            let fleeing = fighting && game.awareness.tactics.flights.contains_key(&id);
            let want = game.wants_weapon_out(id) || (drawn_by_default && !unequipped && !fleeing);
            if hold.test.is_none() && hold.drawn != want {
                hold.drawn = want;
                let why = if fleeing { " (it flees)" } else if game.active_weapon_use(id).is_some() { " (a weapon use)" } else if !fighting && want != game.is_alerted(id) { " (its package's flags)" } else { "" };
                info!("alert: {} {id} {} its weapon{why}", game.data.name(id), if want { "draws" } else { "holsters" });
            }
            // Hidden while holstered under Weapons Unequipped; a creature's body weapon is
            // part of its body (its `Weapon` node carries body parts).
            let hide = unequipped && !hold.body && hold.anim.stage == fnv_actor::weapon::Stage::Holstered;
            if hide != hidden {
                // The parts on the back (the flamer's tank) go with it.
                for &part in rig.nodes.get(hold.stance.weapon_node).into_iter().chain(&hold.back) {
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
