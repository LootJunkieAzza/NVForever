//! Part P (docs/design/layer4-play.md §4): the player — input, character movement, the
//! first-person camera.

use crate::{space, PlayWorld, PlayerCamera};
use bevy::prelude::*;

/// Eye height above the feet, game units [hypothesis: placeholder for a standing human at
/// about 70 units per metre; not captured].
pub const EYE_HEIGHT: f32 = 112.0;

/// The player's pose in game space.
#[derive(Resource, Debug, Clone, Copy)]
pub struct Player {
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    /// Pitch, radians (negative looks down).
    pub pitch: f32,
    pub eye_height: f32,
    /// Feet position, game space.
    pub feet: [f32; 3],
}

impl Player {
    /// Eye position, game space.
    pub fn eye(&self) -> [f32; 3] {
        [self.feet[0], self.feet[1], self.feet[2] + self.eye_height]
    }

    /// Unit view direction, game space.
    pub fn look_dir(&self) -> [f32; 3] {
        space::look_dir(self.heading, self.pitch)
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_player).add_systems(Update, place_camera.in_set(crate::PlaySet::Physics));
    }
}

fn init_player(mut commands: Commands, d: Res<PlayWorld>) {
    let [x, y, z] = d.eye;
    commands.insert_resource(Player {
        heading: d.heading_deg.to_radians(),
        pitch: d.pitch_deg.to_radians(),
        eye_height: EYE_HEIGHT,
        feet: [x, y, z - EYE_HEIGHT],
    });
}

/// The camera at the player's eye, looking along the view.
fn place_camera(player: Option<Res<Player>>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let (Some(p), Ok(mut t)) = (player, cams.single_mut()) else { return };
    t.translation = space::to_bevy(p.eye());
    t.rotation = space::look_rotation(p.heading, p.pitch);
}
