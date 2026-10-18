//! A script's `Look` and `StopLook` (docs/design/script-stubs.md §3): the game keeps
//! which reference each actor's head is held on (`fnv_game::Game::script_look`); this
//! module puts a [`LookOverride`] on that actor and takes it off at `StopLook`.
//!
//! - **Over every other look.** Look "overrides all normal head-tracking behavior"
//!   [documented: GECK wiki Look]: the override replaces one another module put there (a
//!   conversation, a line said, an alert); those modules leave a look they did not put
//!   alone, and put theirs back once this one is gone.
//! - **The target.** The player's eyes, another actor's head while it is drawn, else the
//!   reference's position in the simulation (a marker), at its height plus the player's
//!   eye height [design: a marker on the floor is looked at as a person standing there
//!   would be]. A target that is not in the world (disabled, not loaded) leaves the head
//!   to the proximity choice: "until … the target leaves the loaded area" [documented].
//! - **The body.** With Look's `rotateBody` 1 "Actor will rotate his entire body to
//!   Target ref" [documented: GECK wiki Look]. An actor the AI runs (every actor with
//!   `--play`) is turned by `fnv_ai` at its walker's turn rate, over the package's own
//!   facing (`Game::body_looks`, `Ai::face_looks`; log `ai [..] <actor>: turns its body
//!   to … for a script's Look`, `faces …`). Any other actor standing in its idle is turned on the spot here with the turn of a
//!   conversation (`dialogue_look::BodyTurn`: the same speed, turn clips and the 30°
//!   from which a body turns at all), and turned again whenever the target is that far
//!   off its front, until StopLook; the body keeps its last heading afterwards
//!   [hypothesis: the wiki says only that the body rotates; the observer's captures hold
//!   no Look call to measure]. Not turned (the head still is): a dead actor, a creature,
//!   one in furniture, a `--npc-travel` traveller, one under the walk test, a walking
//!   one (`speech_look::standing_body_stays`), and the actor of a conversation with the
//!   player (`dialogue_look.rs` turns it).
//!
//! Log: `script look: <actor> looks at <target>` when a look starts, `… stops looking`
//! when it ends; `script look: <actor> turns its body to <target> …` per body turn, `…
//! the body stays here: <why>` once per look; at exit `script look summary: N looks
//! started, M stopped; B body turns, A bodies left to the AI, S stayed`.

use crate::dialogue_look::{heading_deg, yaw_pitch, BodyHeld, BodyTurn, Looks};
use crate::head_look::{LookAt, LookOverride};
use crate::limbs::ActorRig;
use crate::locomotion::{Dying, Locomotion, WalkTest};
use crate::nav::NavState;
use crate::ragdoll::Corpse;
use crate::residents::Residents;
use crate::speech_look::{standing_body_stays, BY_AI};
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::combat::is_dead;
use nvf_play::pause::{MenuPause, Paused};
use nvf_play::player::{Player, EYE_HEIGHT};
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use nvf_play::{space, PlaySet};
use std::collections::{HashMap, HashSet};

pub struct ScriptLookPlugin;

impl Plugin for ScriptLookPlugin {
    fn build(&self, app: &mut App) {
        // The bodies turn after the frame's animation and the play systems, as a line's
        // turn does (speech_look.rs); head tracking runs after them (head_look.rs).
        app.init_resource::<ScriptLooks>()
            .add_systems(Update, (show.after(PlaySet::Scripts), turn_bodies.after(PlaySet::Scripts).after(crate::locomotion::drive).after(PlaySet::Ui).after(crate::speech_look::face)))
            .add_systems(Last, report);
    }
}

/// The looks this module has put on, by actor reference: the entity and the target.
#[derive(Resource, Default)]
pub(crate) struct ScriptLooks {
    on: HashMap<u32, (Entity, LookAt)>,
    started: u64,
    stopped: u64,
    /// Bodies turning on the spot for a Look, by reference.
    bodies: HashMap<u32, (Entity, BodyTurn)>,
    /// The (actor, target) looks whose body was said to stay or be the AI's (said once).
    told: HashSet<(u32, u32)>,
    body_turns: u64,
    by_ai: u64,
    stayed: u64,
}

fn show(
    mut commands: Commands,
    script: Option<Res<ScriptState>>,
    sim: Option<Res<SimState>>,
    actors: Query<(Entity, &ActorRig, Option<&LookOverride>)>,
    mut looks: ResMut<ScriptLooks>,
) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    if game.senses.looks.is_empty() && looks.on.is_empty() {
        return;
    }
    let drawn: HashSet<u32> = actors.iter().map(|(_, r, _)| r.reference).collect();
    let mut seen = HashSet::new();
    for (e, rig, forced) in &actors {
        let id = FormId(rig.reference);
        seen.insert(rig.reference);
        let want = game.script_look(id).and_then(|l| {
            let t = l.target;
            if t == FormId::PLAYER_REF {
                Some(LookAt::Player)
            } else if drawn.contains(&t.0) {
                Some(LookAt::Actor(t.0))
            } else {
                let s = sim.as_deref()?;
                if !s.present(t.0) {
                    return None;
                }
                let pos = s.world.peek(t).map(|r| r.pos).or_else(|| s.refs.get(t).map(|p| p.pos))?;
                Some(LookAt::Point([pos[0], pos[1], pos[2] + EYE_HEIGHT]))
            }
        });
        let ours = looks.on.get(&rig.reference).filter(|(oe, _)| *oe == e).map(|(_, at)| *at);
        match want {
            Some(at) => {
                if forced.map(|f| f.0) != Some(at) || ours != Some(at) {
                    commands.entity(e).insert(LookOverride(at));
                    if ours.is_none() {
                        looks.started += 1;
                        info!("script look: {} looks at {}", game.data.name(id), game.data.name(game.script_look(id).map_or(id, |l| l.target)));
                    }
                    looks.on.insert(rig.reference, (e, at));
                }
            }
            None => {
                if let Some(at) = ours {
                    if forced.is_some_and(|f| f.0 == at) {
                        commands.entity(e).remove::<LookOverride>();
                    }
                    looks.on.remove(&rig.reference);
                    looks.stopped += 1;
                    info!("script look: {} stops looking", game.data.name(id));
                }
            }
        }
    }
    // Actors no longer drawn keep nothing of ours.
    looks.on.retain(|r, _| seen.contains(r));
}

/// What [`turn_bodies`] reads of an actor.
type Actor = (Entity, &'static ActorRig, &'static ChildOf, Has<Dying>, Has<Corpse>, Has<WalkTest>, Has<BodyHeld>);

/// Turn the bodies of the actors a Look with `rotateBody` holds on a target (module
/// notes), and stop a turn whose Look is over.
#[allow(clippy::too_many_arguments)]
pub(crate) fn turn_bodies(
    time: Res<Time>,
    (mode, paused): (Res<MenuPause>, Res<Paused>),
    script: Option<Res<ScriptState>>,
    sim: Option<Res<SimState>>,
    player: Option<Res<Player>>,
    nav: Option<Res<NavState>>,
    residents: Option<Res<Residents>>,
    talk: Option<Res<Looks>>,
    mut looks: ResMut<ScriptLooks>,
    actors: Query<Actor>,
    mut locos: Query<&mut Locomotion>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let wanted: Vec<(FormId, FormId)> = game.map(|g| g.body_looks()).unwrap_or_default();
    let looks = &mut *looks;
    if wanted.is_empty() && looks.bodies.is_empty() {
        looks.told.clear();
        return;
    }
    let in_conversation = game.and_then(|g| g.dialogue_actor()).map(|a| a.0);
    let by_ref: HashMap<u32, Entity> = actors.iter().map(|a| (a.1.reference, a.0)).collect();
    let name = |r: FormId| if r == FormId::PLAYER_REF { "the player".to_string() } else { game.map_or_else(|| format!("{:08X}", r.0), |g| g.data.name(r)) };
    // Where the target stands (Bevy space): the player's feet, a drawn actor, else the
    // reference's place in the simulation (a marker); none when it is not in the world.
    let place_of = |to: FormId| -> Option<Vec3> {
        if to == FormId::PLAYER_REF {
            return player.as_ref().map(|p| space::to_bevy(p.feet));
        }
        if let Some(g) = by_ref.get(&to.0).and_then(|&e| globals.get(e).ok()) {
            return Some(g.translation());
        }
        let s = sim.as_deref()?;
        if !s.present(to.0) {
            return None;
        }
        s.world.peek(to).map(|r| r.pos).or_else(|| s.refs.get(to).map(|p| p.pos)).map(space::to_bevy)
    };

    // A turn whose Look is over (StopLook, or no longer turning the body) stops where it is.
    looks.told.retain(|&(a, t)| wanted.contains(&(FormId(a), FormId(t))));
    let over: Vec<u32> = looks.bodies.keys().copied().filter(|&r| !wanted.iter().any(|(a, _)| a.0 == r) || Some(r) == in_conversation).collect();
    for r in over {
        let (entity, _) = looks.bodies.remove(&r).expect("listed");
        if let Ok(mut lo) = locos.get_mut(entity) {
            lo.turn = 0.0;
        }
    }

    // A body facing further than a turn's threshold from its target starts a turn.
    for &(who, to) in &wanted {
        let r = who.0;
        if looks.bodies.contains_key(&r) || Some(r) == in_conversation || talk.as_ref().is_some_and(|t| t.turns_head(r)) {
            continue;
        }
        let Some(&entity) = by_ref.get(&r) else { continue };
        let Ok((_, _, child_of, dying, corpse, walk_test, held)) = actors.get(entity) else { continue };
        let npc = sim.as_deref().is_some_and(|s| s.refs.get(who).is_some_and(|p| &p.sig == b"ACHR"));
        let dead = dying || corpse || is_dead(script.as_deref(), r);
        let traveller = nav.as_ref().is_some_and(|n| n.travellers.iter().any(|t| t.reference == r));
        let by_ai = residents.as_ref().is_some_and(|res| res.runs(r));
        if let Some(why) = standing_body_stays(npc, dead, held, walk_test, traveller, by_ai, locos.get(entity).ok()) {
            if looks.told.insert((r, to.0)) {
                if why == BY_AI {
                    looks.by_ai += 1;
                } else {
                    looks.stayed += 1;
                }
                info!("script look: {} looks at {} with its body; the body stays here: {why}", name(who), name(to));
            }
            continue;
        }
        let Some(target) = place_of(to) else { continue };
        let Ok(root) = transforms.get(entity).copied() else { continue };
        let Ok(parent) = globals.get(child_of.parent()).map(|g| g.affine()) else { continue };
        let off = yaw_pitch((parent * root.compute_affine()).inverse().transform_point3(target)).0;
        if let Some(b) = BodyTurn::plan(root.rotation, off) {
            info!(
                "script look: {} turns its body to {}: {:+.0}° off its front, the body turns {:+.0}° in {:.2} s from heading {:.0}°",
                name(who),
                name(to),
                off.to_degrees(),
                b.total.to_degrees(),
                b.seconds,
                heading_deg(root.rotation)
            );
            looks.body_turns += 1;
            looks.bodies.insert(r, (entity, b));
        }
    }

    // The bodies turning, on the virtual clock. With `--menu-pause original` the picture
    // stands still while a menu is up: a turn is completed at once (speech_look.rs).
    let dt = time.delta_secs();
    let instant = *mode == MenuPause::Original && !paused.0 && game.is_some_and(|g| g.menu.is_some() || g.dialogue.is_some());
    let mut done = Vec::new();
    for (&r, (entity, b)) in looks.bodies.iter_mut() {
        let gone = actors.get(*entity).map_or(true, |a| a.3 || a.4) || is_dead(script.as_deref(), r);
        let finished = if gone {
            true
        } else {
            let (rot, rate) = b.step(dt, instant);
            if let Ok(mut t) = transforms.get_mut(*entity) {
                t.rotation = rot;
            }
            if let Ok(mut lo) = locos.get_mut(*entity) {
                lo.turn = if b.done() { 0.0 } else { rate };
            }
            b.done()
        };
        if gone {
            if let Ok(mut lo) = locos.get_mut(*entity) {
                lo.turn = 0.0;
            }
        }
        if finished {
            done.push(r);
        }
    }
    for r in done {
        looks.bodies.remove(&r);
    }
}

fn report(mut exit: MessageReader<AppExit>, looks: Res<ScriptLooks>) {
    if exit.read().next().is_some() {
        info!(
            "script look summary: {} looks started, {} stopped; {} body turns, {} bodies left to the AI, {} stayed",
            looks.started, looks.stopped, looks.body_turns, looks.by_ai, looks.stayed
        );
    }
}
