//! Part P (docs/design/layer4-play.md §4): the physics world around the player — terrain
//! and references' collision streamed by cell, stepped at a fixed rate.

use bevy::prelude::*;
use fnv_phys::{Character, PhysParams, PhysWorld, Stepper};

/// The physics world. Ray queries for activation go through `world.queries()` (call
/// `world.update_queries()` first if bodies changed since the last step).
#[derive(Resource)]
pub struct PhysState {
    pub world: PhysWorld,
    pub stepper: Stepper,
    /// The player's character, once placed.
    pub player: Option<Character>,
}

impl Default for PhysState {
    fn default() -> Self {
        PhysState { world: PhysWorld::new(PhysParams::default()), stepper: Stepper::default(), player: None }
    }
}

pub struct PhysPlugin;

impl Plugin for PhysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PhysState>();
    }
}
