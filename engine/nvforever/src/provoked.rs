//! Fights that start because someone was hit, and how they end (docs/design/combat-ai.md
//! §10): the rules are the game's (`fnv_game::provocation`, fed by `detection.rs` with
//! every hit); this module tells the game when the player yields and logs what happened.
//!
//! - **Yield**: the player putting the weapon away (`PlayerControl::weapon_out` going
//!   off) is a yield [documented: GECK wiki Yielding]; `Game::player_yields` decides who
//!   accepts it.
//! - **Log**: `provocation:` lines for each assault, friendly hit let pass, hit ignored,
//!   ally joining, yield accepted or refused, and a `provocation summary` at exit.

use bevy::prelude::*;
use fnv_game::provocation::Note;
use fnv_sim::FormId;
use nvf_play::control::PlayerControl;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;

pub struct ProvokedPlugin;

impl Plugin for ProvokedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stats>().add_systems(Update, (yields, log_notes).chain().after(nvf_play::PlaySet::Scripts)).add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct Stats {
    assaulted: usize,
    tolerated: usize,
    ignored: usize,
    assisted: usize,
    yields: usize,
    forgiven: usize,
    refused: usize,
}

/// The player put the weapon away: a yield.
fn yields(control: Option<Res<PlayerControl>>, mut script: Option<ResMut<ScriptState>>, sim: Option<ResMut<SimState>>, mut was_out: Local<bool>, mut stats: ResMut<Stats>) {
    let Some(control) = control else { return };
    let out = control.weapon_out;
    let put_away = *was_out && !out;
    *was_out = out;
    if !put_away {
        return;
    }
    let (Some(rt), Some(mut sim)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim) else { return };
    let fighting = rt.game.provocations.grudges.keys().any(|(_, o)| *o == FormId::PLAYER_REF);
    if !fighting {
        return;
    }
    stats.yields += 1;
    let accepted = with_world(rt, &mut sim, |rt| rt.game.player_yields());
    info!("provocation: the player puts the weapon away: a yield, accepted by {accepted}");
}

fn log_notes(mut script: Option<ResMut<ScriptState>>, mut stats: ResMut<Stats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let notes = rt.game.provocations.take_notes();
    if notes.is_empty() {
        return;
    }
    let g = &rt.game;
    let name = |id: FormId| if id == FormId::PLAYER_REF { "the player".to_string() } else { format!("{} {id}", g.data.name(id)) };
    let relation = |r: i64| match r {
        fnv_game::provocation::ENEMY => "enemy",
        fnv_game::provocation::ALLY => "ally",
        fnv_game::provocation::FRIEND => "friend",
        _ => "neutral",
    };
    for n in notes {
        match n {
            Note::Assaulted { actor, by, relation: r, hits } => {
                stats.assaulted += 1;
                info!("provocation: {} ({}) is attacked by {} and fights back (hits counted {hits})", name(actor), relation(r), name(by));
            }
            Note::Tolerated { actor, by, relation: r, hits, allowed, in_combat } => {
                stats.tolerated += 1;
                let allowed = if allowed >= fnv_game::provocation::INFINITE_HITS { "any number".to_string() } else { format!("{allowed:.0}") };
                info!("provocation: {} ({}, {}) lets a hit by {} pass ({hits} counted, {allowed} allowed)", name(actor), relation(r), if in_combat { "in combat" } else { "out of combat" }, name(by));
            }
            Note::Ignored { actor, by, why } => {
                stats.ignored += 1;
                info!("provocation: {} ignores a hit by {}: {why}", name(actor), name(by));
            }
            Note::Assists { helper, victim, attacker } => {
                stats.assisted += 1;
                info!("provocation: {} helps {} against {}", name(helper), name(victim), name(attacker));
            }
            Note::Forgives { actor, after } => {
                stats.forgiven += 1;
                info!("provocation: {} accepts the yield ({after:.1} s into the fight) and stops fighting", name(actor));
            }
            Note::Refuses { actor, why, after } => {
                stats.refused += 1;
                info!("provocation: {} ignores the yield ({after:.1} s into the fight): {why}", name(actor));
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<Stats>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &*stats;
    info!(
        "provocation summary: {} actors attacked turned on their attacker, {} friendly hits let pass, {} hits ignored, {} allies joined a fight; {} yields: {} accepted, {} ignored",
        s.assaulted, s.tolerated, s.ignored, s.assisted, s.yields, s.forgiven, s.refused
    );
}
