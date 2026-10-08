//! nvf-play — Layer 4 in the game: the player (physics-driven first-person controller),
//! activation through the crosshair, and container and inventory menus, on top of the
//! headless `fnv-sim` (references, inventories, activation, clock) and `fnv-phys`
//! (collision, character controller). Design and part ownership:
//! docs/design/layer4-play.md.
//!
//! `nvforever --play` adds [`PlayPlugin`]; without it the binary stays the M3 explorer.

pub mod activate;
pub mod menu;
pub mod phys;
pub mod player;
pub mod sim;
pub mod space;

use bevy::prelude::*;
use fnv_esm::{LoadOrder, Schema};
use fnv_world::WorldIndex;

/// What the game hands the plugin: the plugins as loaded (shared with render streaming,
/// hence `'static`), the start pose and test aids.
#[derive(Clone, Copy)]
pub struct PlayData {
    pub lo: &'static LoadOrder<'static>,
    pub schema: &'static Schema,
    pub index: &'static WorldIndex,
    /// The worldspace being played (FormID).
    pub world: u32,
    /// Start inside this interior cell instead of the worldspace.
    pub interior: Option<u32>,
    /// Start eye position, game space.
    pub eye: [f32; 3],
    /// Compass heading in degrees (0 north, 90 east).
    pub heading_deg: f32,
    /// Pitch in degrees (negative looks down).
    pub pitch_deg: f32,
    /// Test aid: walk forward at this speed (game units per second); 0 = off.
    pub auto_walk: f32,
}

/// [`PlayData`] as a resource, for systems that read the plugins.
#[derive(Resource, Clone, Copy, Deref)]
pub struct PlayWorld(pub PlayData);

/// Whether the player is in the world or in a menu (no movement or looking in menus).
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PlayState {
    #[default]
    Playing,
    Menu,
}

/// Order of the play systems within `Update`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaySet {
    /// Read input (keys, mouse).
    Input,
    /// Stream physics cells, step the world, move the player and the camera.
    Physics,
    /// Crosshair ray, prompt, activation.
    Activation,
    /// Menus.
    Ui,
    /// Bring render entities in line with the simulation.
    Sync,
}

/// Marks the `Camera3d` that is the player's eyes.
#[derive(Component)]
pub struct PlayerCamera;

/// Marks a render entity spawned for a placed reference (its FormID), so the game can
/// hide what the simulation removed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefVisual(pub u32);

/// Move the player to another place: through a door into an interior (`Some(cell)`) or
/// into the worldspace (`None`). Render streaming and physics both follow it.
#[derive(Message, Clone, Copy, Debug)]
pub struct Travel {
    pub interior: Option<u32>,
    /// Destination feet position, game space.
    pub feet: [f32; 3],
    /// Compass heading in degrees on arrival.
    pub heading_deg: f32,
}

/// Open the container menu on this container reference.
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenContainer {
    pub container: u32,
}

/// The simulation changed whether a reference is present (taken, dropped, enabled,
/// disabled): visuals and collision must follow.
#[derive(Message, Clone, Copy, Debug)]
pub struct RefChanged {
    pub form_id: u32,
}

pub struct PlayPlugin(pub PlayData);

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let d = self.0;
        app.insert_resource(PlayWorld(d))
            .insert_resource(sim::SimState::new(d.lo, d.schema, d.index))
            .init_state::<PlayState>()
            .add_message::<Travel>()
            .add_message::<OpenContainer>()
            .add_message::<RefChanged>()
            .configure_sets(Update, (PlaySet::Input, PlaySet::Physics, PlaySet::Activation, PlaySet::Ui, PlaySet::Sync).chain())
            .add_plugins((sim::SimPlugin, phys::PhysPlugin, player::PlayerPlugin, activate::ActivatePlugin, menu::MenuPlugin));
    }
}
