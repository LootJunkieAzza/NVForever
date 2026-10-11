//! nvf-play — Layer 4 in the game: the player (physics-driven first-person controller),
//! activation through the crosshair, and container and inventory menus, on top of the
//! headless `fnv-sim` (references, inventories, activation, clock) and `fnv-phys`
//! (collision, character controller). Design and part ownership:
//! docs/design/layer4-play.md.
//!
//! `nvforever --play` adds [`PlayPlugin`]; without it the binary stays the M3 explorer.
//!
//! Layer 5 (workstream 5D, docs/design/layer5-play.md): [`script`] runs the `fnv-game`
//! runtime (quest and object scripts, events, messages) next to the simulation.
//! [`dialogue`] is the screen for its conversations (docs/design/dialogue-play.md);
//! [`dialogue_look`] turns the player's view to the actor they talk to.

pub mod activate;
pub mod bindings;
pub mod blocking;
pub mod combat;
pub mod control;
pub mod dialogue;
pub mod dialogue_look;
pub mod doors;
pub mod dropped;
pub mod furniture;
pub mod hitvol;
pub mod loading;
pub mod menu;
pub mod moved;
pub mod pause;
pub mod phys;
pub mod player;
pub mod script;
pub mod script_draw;
pub mod script_events;
pub mod script_hud;
pub mod script_trigger;
pub mod settings;
pub mod sim;
pub mod space;
mod test_aid;
pub mod sfx;
pub mod voice;
pub mod wait;

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
    /// The worldspace the game starts in (FormID). Where the player is later is
    /// [`Location`]: door travel can change the worldspace.
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
    /// No start position was asked for: an interior start goes where the console's `coc`
    /// puts the player (`player::safe::cell_spot`), facing that way.
    pub start_at_cell_spot: bool,
    /// Actors live by their AI packages (nvforever's residents): the game runs the
    /// packages' begin, end and change scripts and idles from the start, the player's
    /// script packages included (`fnv_game::ai`).
    pub packages: bool,
}

/// The controls in `--play`, for nvforever's help line.
pub const HELP: &str =
    "WASD move  Shift walk  Caps Lock always run  Ctrl sneak  Q auto move  Space jump  mouse look  E use  Tab inventory  T wait  Esc pause  [ ] hour  F12 screenshot";

/// [`PlayData`] as a resource, for systems that read the plugins.
#[derive(Resource, Clone, Copy, Deref)]
pub struct PlayWorld(pub PlayData);

/// Whether the player is in the world, in a menu (no movement or looking in menus), in
/// a conversation or behind a loading screen.
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PlayState {
    #[default]
    Playing,
    Menu,
    /// Talking to an actor ([`dialogue`]): a pausing menu like `Menu` (the player, the
    /// world and the game clock stand still, the cursor is free) with the dialogue
    /// screen's own keys; the actor's lines go on in real time.
    Dialogue,
    /// Behind a loading screen ([`loading`]): nothing happens in the world until the
    /// destination is loaded.
    Loading,
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
    /// Layer 5 scripts: events, the frame's tick, their effects (`script`).
    Scripts,
    /// Menus.
    Ui,
    /// Bring render entities in line with the simulation.
    Sync,
}

/// Marks the `Camera3d` that is the player's eyes.
#[derive(Component)]
pub struct PlayerCamera;

/// Marks a render entity spawned for a reference (its FormID): by nvforever for placed
/// references, by [`dropped`] for references the simulation created (dropped items), so
/// the game can hide what the simulation removed and the crosshair can find items by
/// their render bounds.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefVisual(pub u32);

/// Put by nvforever on the entity of every render cell it streams in (its transform is
/// the cell's origin in game space, under the world root). [`dropped`] spawns the
/// references the simulation created in that cell as its children, so they come and go
/// with render streaming without nvforever knowing them.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderCell {
    /// An exterior grid square of a worldspace (FormID).
    Exterior { world: u32, x: i32, y: i32 },
    /// An interior cell (FormID).
    Interior(u32),
}

/// Where the player is now: the worldspace, and the interior cell when inside one. Set
/// from [`PlayData`] at the start; physics changes it when it applies a [`Travel`]
/// (PlaySet::Physics), so later sets of the same frame see the destination.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    /// The worldspace (FormID): the one being played, or, inside an interior, the one
    /// the player last walked in.
    pub world: u32,
    pub interior: Option<u32>,
}

/// Move the player to another place: through a door into an interior (`interior:
/// Some(cell)`) or out into a worldspace (`interior: None`; `world` names it, and may
/// differ from the one being played). Render streaming and physics both follow it.
/// Activation and scripts ask for it with a [`loading::TravelRequest`]; it goes out once
/// the loading screen is up.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Travel {
    /// The destination worldspace (FormID) for travel outdoors; `None` into an interior,
    /// or outdoors meaning the current worldspace.
    pub world: Option<u32>,
    pub interior: Option<u32>,
    /// Destination feet position, game space.
    pub feet: [f32; 3],
    /// Compass heading in degrees on arrival.
    pub heading_deg: f32,
}

impl Travel {
    /// Where the player is after this travel, leaving `from`. Into an interior the
    /// worldspace stays the one left.
    pub fn destination(&self, from: Location) -> Location {
        match self.interior {
            Some(_) => Location { world: from.world, interior: self.interior },
            None => Location { world: self.world.unwrap_or(from.world), interior: None },
        }
    }
}

/// Open the container menu on this container reference.
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenContainer {
    pub container: u32,
}

/// The simulation changed whether a reference is present (taken, dropped, enabled,
/// disabled): visuals and collision must follow. Sent for placed references and for
/// references the simulation creates (a drop from the inventory sends one per new
/// reference; taking a dropped item back sends one for it).
#[derive(Message, Clone, Copy, Debug)]
pub struct RefChanged {
    pub form_id: u32,
}

/// The player used up an item from the inventory (an aid item): the script runtime
/// applies its effects, including effect scripts (`fnv_game::Game::apply_magic_item`).
/// The menu has already removed the item from the simulation.
#[derive(Message, Clone, Copy, Debug)]
pub struct ItemUsed {
    pub item: u32,
}

pub struct PlayPlugin(pub PlayData);

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let d = self.0;
        app.insert_resource(PlayWorld(d))
            .insert_resource(Location { world: d.world, interior: d.interior })
            .insert_resource(sim::SimState::new(d.lo, d.schema, d.index))
            .init_state::<PlayState>()
            .add_message::<Travel>()
            .add_message::<OpenContainer>()
            .add_message::<RefChanged>()
            .add_message::<ItemUsed>()
            .add_message::<furniture::UseFurniture>()
            .init_resource::<furniture::PlayerSeat>()
            .configure_sets(Update, (PlaySet::Input, PlaySet::Physics, PlaySet::Activation, PlaySet::Scripts, PlaySet::Ui, PlaySet::Sync).chain())
            .add_plugins((sim::SimPlugin, phys::PhysPlugin, player::PlayerPlugin, activate::ActivatePlugin, menu::MenuPlugin, dropped::DroppedPlugin, moved::MovedPlugin, pause::PausePlugin, wait::WaitPlugin))
            .add_plugins((script::ScriptPlugin, hitvol::HitVolumePlugin, combat::CombatPlugin, sfx::SfxPlugin, voice::VoicePlugin, loading::LoadingPlugin, dialogue_look::DialogueLookPlugin));
        test_aid::setup(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOJAVE: u32 = 0xDA726;
    const FREESIDE: u32 = 0x12D94D;
    const HOUSE: u32 = 0x100;

    fn travel(world: Option<u32>, interior: Option<u32>) -> Travel {
        Travel { world, interior, feet: [0.0; 3], heading_deg: 0.0 }
    }

    #[test]
    fn travel_changes_the_location() {
        let outside = Location { world: MOJAVE, interior: None };
        // Into an interior: the worldspace is kept for the way back out.
        let inside = travel(None, Some(HOUSE)).destination(outside);
        assert_eq!(inside, Location { world: MOJAVE, interior: Some(HOUSE) });
        // Out into the same worldspace, named or not.
        assert_eq!(travel(Some(MOJAVE), None).destination(inside), outside);
        assert_eq!(travel(None, None).destination(inside), outside);
        // Through a gate into another worldspace, and back.
        let freeside = travel(Some(FREESIDE), None).destination(outside);
        assert_eq!(freeside, Location { world: FREESIDE, interior: None });
        assert_eq!(travel(Some(MOJAVE), None).destination(freeside), outside);
        // From an interior out into another worldspace than the one it was entered from.
        assert_eq!(travel(Some(FREESIDE), None).destination(inside), freeside);
    }
}
