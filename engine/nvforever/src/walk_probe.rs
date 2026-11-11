//! The walk probe (#2124): how fast chosen actors' bodies move on their walks, with and
//! without a weapon drawn, for the smoke checks of the people's walking speeds
//! (docs/spec-notes/actors.md §8.10, People; `fnv_ai::carry::drawn_walk_share`).
//!
//! `NVF_PLAY_TEST_WALK_PROBE=<ref>[,<ref>…]` (editor IDs or `0x` FormIDs, as the script
//! test aid names them): every frame each one's body position is read (its rig's
//! transform). A window of [`WINDOW`] seconds in which the actor was on a walk the whole
//! time (the AI's `Activity::Travelling` at a walk: its walker's speed at most half its
//! calm run), had its weapon out, or away, the whole time (`Game::weapon_is_out`) and was
//! not in combat gives one sample: the horizontal distance its body moved over the
//! window's length, as the AI observer in the original game measured (one-second windows
//! in which the actor moved and kept its running and weapon state, actors.md §8.10). A
//! window that a run, a fight or the end of the walk breaks is left out and counted.
//!
//! At the exit, per actor and weapon state: `walk probe: <ref> drawn speed=<median> (<n>
//! windows; <class> armour, Speed Mult <m>, weapon <weapon> type <t>, the AI's walk <w>)`
//! (`holstered` the same; the armour class, Speed Mult, weapon and its WEAP animation type
//! and the AI's walker speed as in the last window), or `walk probe: <ref> drawn: no
//! windows`; then `walk probe: <ref> windows left out: <c> in combat, <r> running, <o> off
//! a walk`.

use crate::limbs::ActorRig;
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_play::script::ScriptState;
use std::collections::BTreeMap;

/// A sample's length, seconds [observed: the AI observer's windows, actors.md §8.10].
const WINDOW: f32 = 1.0;

pub struct WalkProbePlugin;

impl Plugin for WalkProbePlugin {
    fn build(&self, app: &mut App) {
        let names = names(&std::env::var("NVF_PLAY_TEST_WALK_PROBE").unwrap_or_default());
        if names.is_empty() {
            return;
        }
        app.insert_resource(Probe { names, actors: BTreeMap::new() }).add_systems(Update, sample.after(crate::residents::drive)).add_systems(Last, report);
    }
}

/// The actors named in the variable's value.
fn names(v: &str) -> Vec<String> {
    v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

#[derive(Default)]
struct Phase {
    speeds: Vec<f32>,
    /// What the last window saw: armour, Speed Mult, weapon, the AI's walk.
    about: String,
}

#[derive(Default)]
struct Track {
    label: String,
    /// The window under way: where the body was (Bevy x and z: game x and -y), when, and
    /// whether the weapon was out.
    window: Option<(Vec2, f32, bool)>,
    drawn: Phase,
    holstered: Phase,
    combat: u32,
    running: u32,
    off: u32,
}

#[derive(Resource)]
struct Probe {
    /// Names not resolved yet (the game not running yet).
    names: Vec<String>,
    actors: BTreeMap<u32, Track>,
}

fn sample(time: Res<Time>, mut probe: ResMut<Probe>, mut script: Option<ResMut<ScriptState>>, res: Option<Res<Residents>>, rigs: Query<(&ActorRig, &GlobalTransform)>) {
    let (Some(res), Some(rt)) = (res, script.as_deref_mut().and_then(|s| s.rt.as_mut())) else { return };
    let game = &mut rt.game;
    if !probe.names.is_empty() {
        for name in std::mem::take(&mut probe.names) {
            match fnv_game::runner::resolve(game, &name) {
                Ok(id) => {
                    info!("walk probe: measuring {name} ({id})");
                    probe.actors.insert(id.0, Track { label: name, ..Default::default() });
                }
                Err(e) => warn!("walk probe: {name} is not a reference: {e}"),
            }
        }
    }
    let now = time.elapsed_secs();
    for (rig, at) in &rigs {
        let Some(t) = probe.actors.get_mut(&rig.reference) else { continue };
        let id = FormId(rig.reference);
        let p = at.translation();
        let flat = Vec2::new(p.x, p.z);
        let (travelling, walker, run) = res.walk_of(id).unwrap_or((false, 0.0, 0.0));
        let broken = if game.awareness.state(id).in_combat() {
            Some(&mut t.combat)
        } else if !travelling {
            Some(&mut t.off)
        } else if walker > run * 0.5 {
            Some(&mut t.running)
        } else {
            None
        };
        if let Some(count) = broken {
            if t.window.take().is_some() {
                *count += 1;
            }
            continue;
        }
        let out = game.weapon_is_out(id);
        match t.window {
            Some((from, since, was)) if was == out => {
                let dt = now - since;
                if dt < WINDOW {
                    continue;
                }
                let class = fnv_game::combat::worn_armor_class(&game.world, id);
                let mult = crate::creature_pace::speed_mult(game, FormId(rig.base), id);
                let weapon = game.held_weapon(id).map(|(w, _)| w).or_else(|| game.extra.get(&id).and_then(|e| e.held_weapon));
                let kind = weapon.and_then(|w| game.data.weapon_anim_types.get(&w).copied());
                let phase = if out { &mut t.drawn } else { &mut t.holstered };
                phase.speeds.push((flat - from).length() / dt);
                phase.about = format!(
                    "{class:?} armour, Speed Mult {mult:.0}, weapon {} type {}, the AI's walk {walker:.1}",
                    weapon.map_or("none".to_string(), |w| game.data.name(w).to_string()),
                    kind.map_or("none".to_string(), |k| k.to_string())
                );
                t.window = Some((flat, now, out));
            }
            // A new window, also when the weapon was drawn or put away in the last.
            _ => t.window = Some((flat, now, out)),
        }
    }
}

/// The middle value (the mean of the two middle ones for an even count).
fn median(v: &[f32]) -> f32 {
    let mut v = v.to_vec();
    v.sort_by(f32::total_cmp);
    let n = v.len();
    if n.is_multiple_of(2) { (v[n / 2 - 1] + v[n / 2]) / 2.0 } else { v[n / 2] }
}

fn report(mut exit: MessageReader<AppExit>, probe: Res<Probe>) {
    if exit.read().next().is_none() {
        return;
    }
    for t in probe.actors.values() {
        for (state, phase) in [("drawn", &t.drawn), ("holstered", &t.holstered)] {
            if phase.speeds.is_empty() {
                info!("walk probe: {} {state}: no windows", t.label);
            } else {
                info!("walk probe: {} {state} speed={:.1} ({} windows; {})", t.label, median(&phase.speeds), phase.speeds.len(), phase.about);
            }
        }
        info!("walk probe: {} windows left out: {} in combat, {} running, {} off a walk", t.label, t.combat, t.running, t.off);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_are_split_at_commas() {
        assert_eq!(names(" GoodspringsSettler01Ref, 0x0010C0F4 ,,"), vec!["GoodspringsSettler01Ref".to_string(), "0x0010C0F4".to_string()]);
        assert!(names("").is_empty());
    }

    #[test]
    fn the_median_is_the_middle_value() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }
}
