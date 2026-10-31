//! Encounter zones on screen (`fnv_game::zones`, docs/spec-notes/encounter-zones.md §5):
//! the drawn actors read their leveled lists at the level the game gives each reference,
//! so the actor drawn is the one the game's rules (factions, packages, carried items)
//! speak of.
//!
//! The game's level of every placed actor is copied into `ActorWorld::levels` whenever it
//! can have changed: a zone was entered, references were fixed at a level (a cell
//! loaded), or the player's level changed. A reference whose level is not fixed yet gets
//! the level its cell's load will fix it at, so an actor drawn just before the game hears
//! of its cell is drawn as it will stay.

use crate::actors::ActorWorld;
use bevy::prelude::*;
use nvf_play::script::ScriptState;
use std::collections::HashMap;

pub struct ZonesPlugin;

impl Plugin for ZonesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_levels.after(nvf_play::PlaySet::Activation));
    }
}

/// What the copy was made from: fixed references, zones entered, the player's level.
type Key = (usize, usize, u16);

fn sync_levels(script: Option<Res<ScriptState>>, sim: Option<Res<nvf_play::sim::SimState>>, actor_world: Option<Res<ActorWorld>>, mut last: Local<Option<Key>>) {
    let Some(script) = script else { return };
    let Some(rt) = script.rt.as_ref() else { return };
    let (Some(aw), Some(sim)) = (actor_world, sim) else { return };
    let g = &rt.game;
    // The world played is the simulation's: outside a game turn `g.world` is an empty
    // stand-in (`nvf_play::script::with_world`) with no fixed levels.
    let w = &sim.world;
    let player = g.player.level.map_or(w.config().player_level, |l| l.clamp(1, i32::from(u16::MAX)) as u16);
    let key = (w.fixed_levels().len(), w.zone_levels().len(), player);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let display = fnv_sim::zones::LevelAt::plain(fnv_actor::resolve::DISPLAY_LEVEL);
    let levels: HashMap<u32, fnv_sim::zones::LevelAt> = w
        .placed
        .refs
        .iter()
        .filter(|(_, p)| matches!(&p.sig, b"ACHR" | b"ACRE"))
        .map(|(&r, _)| (r.0, sim.leveled_at(g, r)))
        .filter(|(_, l)| *l != display)
        .collect();
    debug!("zones: {} actor reference(s) read leveled lists above the display level", levels.len());
    if let Ok(mut l) = aw.levels.lock() {
        *l = levels;
    };
}
