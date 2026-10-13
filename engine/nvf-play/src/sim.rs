//! The simulation as a resource: `fnv_sim::World` over the loaded plugins, its game clock
//! running in real time while the player is in the world.

use crate::{PlaySet, PlayState};
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

    /// The name a placed reference is shown by: for an actor, its "Base Data" template
    /// source's ([`fnv_game::GameData::display_base`]: an actor placed from a template
    /// actor with no name of its own shows the name of the actor its template, through a
    /// leveled list, leads to; owner's playtest list 4, item 20); otherwise, and without
    /// the game data, its base form's ([`SimState::name`]).
    pub fn ref_name(&self, data: Option<&fnv_game::GameData>, form_id: u32) -> Option<&str> {
        let base = self.base_of(form_id)?;
        let shown = data.map_or(base, |d| d.display_base(base, FormId(form_id)));
        self.name(shown).or_else(|| self.name(base))
    }

    /// A placed reference's pose in the simulation (position, Euler angles in radians)
    /// when it differs from its placement: moved by physics (pushed or knocked clutter,
    /// `crate::phys::PhysState::settle_placed`) or by a script. `None` for references the
    /// plugins do not place (the player, dropped items) and for those still where the
    /// plugins put them.
    pub fn moved_pose(&self, form_id: u32) -> Option<([f32; 3], [f32; 3])> {
        let id = FormId(form_id);
        let (s, p) = (self.world.peek(id)?, self.refs.get(id)?);
        (s.pos != p.pos || s.rot != p.rot).then_some((s.pos, s.rot))
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
        // Before the scripts, so a frame's scripts all see that frame's game time (one
        // order every frame; which comes first in the game is [hypothesis]: the capture of
        // docs/spec-notes/layer5-captures.md §9 only shows a quest script's own clock
        // change reaching object scripts in the same frame).
        app.add_systems(Update, advance_clock.run_if(in_state(PlayState::Playing)).before(PlaySet::Scripts));
    }
}

/// Game time runs at TimeScale while the player is in the world, not in the container
/// and inventory menus (`PlayState::Menu`) [observed for the container menu: the game
/// clock stopped there, docs/spec-notes/layer5-captures.md §8.3 e; it kept running in the
/// console and Wait menus, docs/spec-notes/simulation.md §5b]. It stops in a conversation
/// with the player as well (`PlayState::Dialogue`) [owner ruling 2026-10-10: a
/// conversation with the player pauses the game; `crate::dialogue`].
fn advance_clock(time: Res<Time>, mut sim: ResMut<SimState>) {
    if let Err(e) = sim.world.advance_real_seconds(time.delta_secs_f64()) {
        warn_once!("game clock: {e}");
    }
}
