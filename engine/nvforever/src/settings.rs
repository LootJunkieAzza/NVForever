//! The renderer's switches on the pause menu's settings page (`nvf_play::settings`, drawn
//! by `nvf_play::pause`; `--play` only): the on/off choices of the command line that can
//! change while the game runs. Each reads and writes the state its module uses, so a
//! switch starts as the command line left it; nothing is saved.
//!
//! Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
//! `--no-actors`, FaceGen, weapons, `--no-lodgen`), the numbers (distances, `--msaa`,
//! `--fov`, volumes) and the test aids.

use crate::grass::GrassConfig;
use crate::head_look::HeadLookSettings;
use crate::lips::LipState;
use crate::lod::LodState;
use crate::nav::NavState;
use crate::sky::SkyState;
use crate::world::Streamer;
use crate::{Args, Overlay};
use bevy::prelude::*;
use nvf_play::pause::GROUP_MENUS;
use nvf_play::settings::{add_setting, Setting};

const GROUP_WORLD: &str = "World";
const GROUP_PICTURE: &str = "Picture";
const GROUP_DEVELOPER: &str = "Developer";

/// List the renderer's switches: those whose module is part of this run.
pub fn register(app: &mut App) {
    let has_lips = app.world().contains_resource::<LipState>();
    let has_heads = app.world().contains_resource::<HeadLookSettings>();
    let has_nav = app.world().contains_resource::<NavState>();
    if has_lips {
        add_setting(
            app,
            Setting {
                key: "lip-sync",
                group: GROUP_MENUS,
                label: "Lip sync",
                note: "On: a speaker's face moves with the line. Off: a face that is moving eases back to rest. (--no-lip)",
                get: |w| w.resource::<LipState>().enabled,
                set: |w, on| w.resource_mut::<LipState>().enabled = on,
            },
        );
    }
    if has_heads {
        add_setting(
            app,
            Setting {
                key: "head-tracking",
                group: GROUP_WORLD,
                label: "Head tracking",
                note: "On: actors turn their heads to the player and to others near them. Off: heads ease back to their animation. (--no-head-tracking)",
                get: |w| w.resource::<HeadLookSettings>().enabled,
                set: |w, on| w.resource_mut::<HeadLookSettings>().enabled = on,
            },
        );
    }
    add_setting(
        app,
        Setting {
            key: "sky-clock",
            group: GROUP_WORLD,
            label: "Sky follows the game clock",
            note: "On: the sky shows the game's time of day as it passes. Off: the sky keeps its hour while the game's clock runs on ([ and ] still set it). (--still-sky)",
            get: |w| !w.resource::<Args>().still_sky,
            set: |w, on| w.resource_mut::<Args>().still_sky = !on,
        },
    );
    add_setting(
        app,
        Setting {
            key: "sun-shadows",
            group: GROUP_PICTURE,
            label: "Sun shadows",
            note: "On: the sun casts shadows out to --shadow-distance. The original casts none from the world. (--shadow-distance 0)",
            get: |w| w.resource::<SkyState>().shadows,
            set: crate::sky::set_shadows,
        },
    );
    add_setting(
        app,
        Setting {
            key: "grass",
            group: GROUP_PICTURE,
            label: "Grass",
            note: "On: grass grows on the landscape; the cells around you get theirs over the next frames. Off: it is hidden and none grows on new cells. (--no-grass)",
            get: |w| w.resource::<GrassConfig>().enabled,
            set: |w, on| w.resource_mut::<GrassConfig>().enabled = on,
        },
    );
    add_setting(
        app,
        Setting {
            key: "distant-lod",
            group: GROUP_PICTURE,
            label: "Distant terrain and objects",
            note: "On: distant land, buildings and trees are drawn beyond the loaded cells (outdoors). Off: the world ends at the loaded cells. (--no-lod)",
            get: |w| !w.resource::<Args>().no_lod,
            set: set_lod,
        },
    );
    add_setting(
        app,
        Setting {
            key: "grade",
            group: GROUP_PICTURE,
            label: "Colour grade",
            note: "On: the picture is graded by the worldspace's, the weather's and the cell's image space. Off: the picture as lit, for comparisons. (--no-grade)",
            get: |w| w.resource::<SkyState>().grade,
            set: |w, on| w.resource_mut::<SkyState>().grade = on,
        },
    );
    add_setting(
        app,
        Setting {
            key: "overlay",
            group: GROUP_DEVELOPER,
            label: "Text overlay",
            note: "On: the position, loading counts and key help are written over the picture. (--no-overlay)",
            get: |w| !w.resource::<Args>().no_overlay,
            set: set_overlay,
        },
    );
    if has_nav {
        add_setting(
            app,
            Setting {
                key: "navmesh",
                group: GROUP_DEVELOPER,
                label: "Draw the navmesh",
                note: "On: the navmesh near the camera and the walking NPCs' paths are drawn. (--show-navmesh)",
                get: |w| w.resource::<NavState>().show,
                set: |w, on| w.resource_mut::<NavState>().show = on,
            },
        );
    }
}

/// Distant LOD on or off now; indoors there is none either way, and the door out goes
/// by `Args::no_lod` (`world::travel`).
fn set_lod(world: &mut World, on: bool) {
    world.resource_mut::<Args>().no_lod = !on;
    let outdoors = world.resource::<Streamer>().interior.is_none();
    world.resource_scope(|world, mut lod: Mut<LodState>| {
        lod.set_enabled(&mut world.commands(), on && outdoors);
    });
    world.flush();
}

fn set_overlay(world: &mut World, on: bool) {
    world.resource_mut::<Args>().no_overlay = !on;
    let mut overlays = world.query_filtered::<&mut Visibility, With<Overlay>>();
    for mut v in overlays.iter_mut(world) {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
}
