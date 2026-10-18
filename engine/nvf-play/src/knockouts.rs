//! Unconscious actors in play (docs/spec-notes/unconscious.md §4): one log line per
//! knockout and per getting up (`fnv_game::unconscious`), a count at exit, and the test
//! aid `NVF_PLAY_TEST_HIT`. The corner message of an essential knockout goes the script
//! effects' way (`crate::script_events::unconscious_line`); the bodies falling and getting
//! up are the renderer's (nvforever `knockouts.rs`).
//!
//! `NVF_PLAY_TEST_HIT=<seconds>:<reference>:<damage>[:<weapon>][,…]`: at that time the
//! player hits the reference (an Editor ID, a hex FormID) for that much health damage,
//! with that weapon (its OnHit block runs, as the boxing gloves' does) [test aid].

use crate::activate::HudMessage;
use crate::combat::{apply_hit, ActorHit, CombatStats, HitFeedback};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::PlayState;
use bevy::prelude::*;
use fnv_game::combat::HitInfo;
use fnv_game::unconscious::{Cause, Happened};
use fnv_sim::FormId;

pub struct KnockoutsPlugin;

impl Plugin for KnockoutsPlugin {
    fn build(&self, app: &mut App) {
        let hits = match std::env::var("NVF_PLAY_TEST_HIT") {
            Ok(v) => parse_hits(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_HIT: {e}");
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        app.init_resource::<KnockoutStats>()
            .insert_resource(TestHits(hits))
            .add_systems(Update, (test_hits.run_if(in_state(PlayState::Playing)), report_news).chain().after(crate::PlaySet::Activation))
            .add_systems(Last, report);
    }
}

/// Knockouts seen in this run.
#[derive(Resource, Default, Debug)]
pub struct KnockoutStats {
    pub essential: u32,
    pub fatigue: u32,
    pub got_up: u32,
}

/// One `NVF_PLAY_TEST_HIT` item.
#[derive(Debug, Clone, PartialEq)]
pub struct TestHit {
    pub at: f32,
    pub target: String,
    pub damage: f32,
    pub weapon: Option<String>,
}

/// The aid's hits still to come, soonest first.
#[derive(Resource, Default)]
struct TestHits(Vec<TestHit>);

/// Parse `<seconds>:<reference>:<damage>[:<weapon>]` items separated by commas.
pub fn parse_hits(v: &str) -> Result<Vec<TestHit>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let parts: Vec<&str> = item.split(':').map(str::trim).collect();
            let (t, target, damage, weapon) = match parts.as_slice() {
                [t, r, d] => (t, r, d, None),
                [t, r, d, w] => (t, r, d, Some(w.to_string())),
                _ => return Err(format!("{item:?}: expected <seconds>:<reference>:<damage>[:<weapon>]")),
            };
            let at = t.parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            let damage = damage.parse::<f32>().map_err(|_| format!("{item:?}: bad damage {damage:?}"))?;
            Ok(TestHit { at, target: target.to_string(), damage, weapon })
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn test_hits(
    time: crate::pause::AidTime,
    mut hits: ResMut<TestHits>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut stats: ResMut<CombatStats>,
    mut feedback: ResMut<HitFeedback>,
    mut hud: Option<ResMut<HudMessage>>,
    mut messages: MessageWriter<ActorHit>,
) {
    let now = time.elapsed_secs();
    while hits.0.first().is_some_and(|h| h.at <= now) {
        // A hit due before the scripts run waits for them (a slow start-up dropped it).
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
        let h = hits.0.remove(0);
        let found = with_world(rt, &mut sim, |rt| {
            let g = &rt.game;
            let target = fnv_game::runner::resolve(g, &h.target)?;
            let weapon = h.weapon.as_deref().map(|w| fnv_game::runner::resolve(g, w)).transpose()?;
            Ok::<_, String>((target, weapon))
        });
        let (target, weapon) = match found {
            Ok(x) => x,
            Err(e) => {
                warn!("test aid: hit {}: {e}", h.target);
                continue;
            }
        };
        let info = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon, location: 0, critical: false, limb: None };
        let r = apply_hit(script.as_deref_mut(), &mut sim, target, h.damage, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &[], false);
        match r {
            Some(r) => info!("test aid: the player hits {target} for {:.0}{}: health {:.0}/{:.0}", r.dealt, weapon.map_or(String::new(), |w| format!(" with {w}")), r.health, r.max_health),
            None => warn!("test aid: hit {target}: no scripts"),
        }
    }
}

/// Log the knockouts and the getting up since last frame.
fn report_news(mut script: Option<ResMut<ScriptState>>, sim: Res<SimState>, mut stats: ResMut<KnockoutStats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    for n in rt.game.take_knockout_news() {
        let name = sim.ref_name(Some(rt.game.data), n.actor.0).map_or_else(|| n.actor.to_string(), str::to_string);
        match n.what {
            Happened::Up => {
                stats.got_up += 1;
                info!("knockout: {name} {} gets up ({} knockout over)", n.actor, n.cause.name());
            }
            Happened::Held => info!("knockout: {name} {} stays down: a fight goes on near it", n.actor),
            Happened::Down => {
                match n.cause {
                    Cause::Essential => stats.essential += 1,
                    Cause::Fatigue => stats.fatigue += 1,
                }
                info!("knockout: {name} {} knocked out ({})", n.actor, n.cause.name());
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<KnockoutStats>) {
    if exit.read().last().is_none() || stats.essential + stats.fatigue == 0 {
        return;
    }
    info!("knockouts: {} essential, {} by fatigue, {} got up", stats.essential, stats.fatigue, stats.got_up);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_parse_in_time_order() {
        let h = parse_hits("6:GSSunnySmilesREF:30:WeapNVBoxingGloves, 4:0x00104E87:500").unwrap();
        assert_eq!(h[0], TestHit { at: 4.0, target: "0x00104E87".into(), damage: 500.0, weapon: None });
        assert_eq!(h[1].weapon.as_deref(), Some("WeapNVBoxingGloves"));
        assert!(parse_hits("4:x").is_err());
        assert!(parse_hits("a:x:1").is_err());
    }
}
