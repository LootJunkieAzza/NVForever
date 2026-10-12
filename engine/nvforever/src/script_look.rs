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
//! - **Not yet.** Look's `rotateBody` (the whole body turns) is kept by the game but not
//!   acted on.
//!
//! Log: `script look: <actor> looks at <target>` when a look starts, `… stops looking`
//! when it ends, and at exit `script look summary: N looks started, M stopped`.

use crate::head_look::{LookAt, LookOverride};
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::player::EYE_HEIGHT;
use nvf_play::script::ScriptState;
use nvf_play::sim::SimState;
use std::collections::{HashMap, HashSet};

pub struct ScriptLookPlugin;

impl Plugin for ScriptLookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptLooks>().add_systems(Update, show.after(nvf_play::PlaySet::Scripts)).add_systems(Last, report);
    }
}

/// The looks this module has put on, by actor reference: the entity and the target.
#[derive(Resource, Default)]
struct ScriptLooks {
    on: HashMap<u32, (Entity, LookAt)>,
    started: u64,
    stopped: u64,
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

fn report(mut exit: MessageReader<AppExit>, looks: Res<ScriptLooks>) {
    if exit.read().next().is_some() {
        info!("script look summary: {} looks started, {} stopped", looks.started, looks.stopped);
    }
}
