//! The simulation as a resource: `fnv_sim::World` over the loaded plugins, its game clock
//! running in real time while the player is in the world.

use crate::PlayState;
use bevy::prelude::*;
use fnv_esm::{LoadOrder, Schema};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_world::WorldIndex;

#[derive(Resource)]
pub struct SimState {
    pub forms: &'static FormStore,
    pub refs: &'static RefTable,
    pub world: World<'static>,
}

impl SimState {
    /// Build the simulation's tables from the plugins (they live for the whole run).
    pub fn new(lo: &LoadOrder, schema: &Schema, index: &WorldIndex) -> SimState {
        let forms: &'static FormStore = Box::leak(Box::new(FormStore::build(lo, schema)));
        let refs: &'static RefTable = Box::leak(Box::new(RefTable::from_index(lo, schema, index)));
        SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) }
    }

    /// The base form of a reference as the simulation sees it now.
    pub fn base_of(&self, form_id: u32) -> Option<FormId> {
        let id = FormId(form_id);
        self.world.peek(id).map(|s| s.base).or_else(|| self.refs.get(id).map(|p| p.base))
    }

    /// Display name of a base form (FULL), else its Editor ID.
    pub fn name(&self, base: FormId) -> Option<&str> {
        let b = self.forms.get(base)?;
        b.name.as_deref().or(b.edid.as_deref())
    }

    /// Whether a placed reference is in the world now (not taken or deleted, enabled).
    pub fn present(&self, form_id: u32) -> bool {
        let id = FormId(form_id);
        !self.world.peek(id).is_some_and(|s| s.deleted) && self.world.is_enabled(id).unwrap_or(true)
    }
}

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, advance_clock.run_if(in_state(PlayState::Playing)));
    }
}

/// Game time runs at TimeScale while the player is in the world [hypothesis: whether it
/// also runs in menus is not captured for the containers menu; the clock kept running in
/// the console and Wait menus, docs/spec-notes/simulation.md §5b].
fn advance_clock(time: Res<Time>, mut sim: ResMut<SimState>) {
    if let Err(e) = sim.world.advance_real_seconds(time.delta_secs_f64()) {
        warn_once!("game clock: {e}");
    }
}
