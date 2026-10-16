//! The renderer's switches on the pause menu's settings page (`nvf_play::settings`, drawn
//! by `nvf_play::pause`; `--play` only): the on/off choices of the command line that can
//! change while the game runs. Each reads and writes the state its module uses, so a
//! switch starts as the command line left it; nothing is saved.
//!
//! Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
//! `--no-actors`, FaceGen, weapons, `--no-lodgen`), the numbers (distances, `--msaa`,
//! `--fov`, volumes) and the test aids.

use crate::graphics::Graphics;
use crate::grass::GrassConfig;
use crate::head_look::HeadLookSettings;
use crate::lips::LipState;
use crate::lod::LodState;
use crate::nav::NavState;
use crate::player_body::PlayerBody;
use crate::sky::SkyState;
use crate::world::Streamer;
use crate::{Args, Overlay};
use bevy::prelude::*;
use nvf_play::pause::GROUP_MENUS;
use nvf_play::settings::{add_setting, Setting};

// The pause menu shows the groups as the original's settings categories (Gameplay,
// Display, …), one short word each (nvf_play::pause).
const GROUP_WORLD: &str = "World";
const GROUP_PICTURE: &str = "Display";
const GROUP_DEVELOPER: &str = "Debug";
const GROUP_GRAPHICS: &str = "Graphics";

/// List the renderer's switches: those whose module is part of this run.
pub fn register(app: &mut App) {
    let has_lips = app.world().contains_resource::<LipState>();
    let has_heads = app.world().contains_resource::<HeadLookSettings>();
    let has_nav = app.world().contains_resource::<NavState>();
    let has_body = app.world().contains_resource::<PlayerBody>();
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
    if has_body {
        add_setting(
            app,
            Setting {
                key: "first-person-body",
                group: GROUP_PICTURE,
                label: "Body in first person",
                note: "On: in first person you see your own body and legs when you look down. Off: only the arms and the weapon are drawn, as in the original. (--first-person-body)",
                get: |w| w.resource::<PlayerBody>().first_person_body,
                set: |w, on| w.resource_mut::<PlayerBody>().first_person_body = on,
            },
        );
    }
    add_setting(
        app,
        Setting {
            key: "sun-shadows",
            group: GROUP_PICTURE,
            label: "Sun shadows",
            note: "On: the sun casts shadows out to --shadow-distance. The original casts none from the world. (--sun-shadows)",
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
    register_graphics(app);
    add_setting(
        app,
        Setting {
            key: "overlay",
            group: GROUP_DEVELOPER,
            label: "Text overlay",
            note: "On: the position, loading counts and key help are written over the picture. Off by default in the game. (--overlay, --no-overlay)",
            get: |w| !w.resource::<Args>().no_overlay,
            set: set_overlay,
        },
    );
    add_setting(
        app,
        Setting {
            key: "hit-markers",
            group: GROUP_DEVELOPER,
            label: "Shot markers",
            note: "On: a cross is drawn at each impact and a line from the muzzle along each hitscan shot. (--hit-markers)",
            get: |w| w.resource::<nvf_play::combat::HitMarks>().show,
            set: |w, on| w.resource_mut::<nvf_play::combat::HitMarks>().show = on,
        },
    );
    add_setting(
        app,
        Setting {
            key: "god-mode",
            group: GROUP_DEVELOPER,
            label: "God mode",
            note: "On: the player takes no damage from hits and cannot be killed by them. (--god)",
            get: |_| nvf_play::combat::god_mode(),
            set: |_, on| nvf_play::combat::set_god_mode(on),
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

/// The engine-level improvements of `graphics.rs` (`--graphics enhanced` starts with all
/// of them on, `vanilla` with none).
fn register_graphics(app: &mut App) {
    type Switch = (&'static str, &'static str, &'static str, fn(&World) -> bool, fn(&mut World, bool));
    let switches: [Switch; 8] = [
        (
            "ambient-occlusion",
            "Ambient occlusion",
            "On: creases, corners and the ground under things are darkened by how enclosed they are (screen-space, from the depth). Not in the original. (--graphics)",
            |w| w.resource::<Graphics>().ambient_occlusion,
            |w, on| w.resource_mut::<Graphics>().ambient_occlusion = on,
        ),
        (
            "fine-shadows",
            "Finer, longer sun shadows",
            "On: the sun's shadow maps have twice the detail and reach half as far again as --shadow-distance. Costs frame rate and video memory. (--graphics)",
            |w| w.resource::<Graphics>().fine_shadows,
            |w, on| w.resource_mut::<Graphics>().fine_shadows = on,
        ),
        (
            "bloom",
            "Bloom",
            "On: light brighter than a sunlit white wall (the sun, lamps, windows) glows into its surroundings, on top of the game's own image space. (--graphics)",
            |w| w.resource::<Graphics>().bloom,
            |w, on| w.resource_mut::<Graphics>().bloom = on,
        ),
        (
            "sharpen",
            "Sharpening",
            "On: the final picture is sharpened where it has little contrast (contrast-adaptive sharpening). (--graphics)",
            |w| w.resource::<Graphics>().sharpen,
            |w, on| w.resource_mut::<Graphics>().sharpen = on,
        ),
        (
            "anisotropic",
            "Anisotropic filtering 16x",
            "On: textures seen at a slant stay sharp further (16x instead of 8x). For textures loaded from now on (the cells loaded next). (--graphics)",
            |w| w.resource::<Graphics>().anisotropic,
            |w, on| {
                w.resource_mut::<Graphics>().anisotropic = on;
                nvf_assets::texture::set_anisotropy(if on { 16 } else { 8 });
            },
        ),
        (
            "taa",
            "Temporal anti-aliasing",
            "On: edges are smoothed over the frames instead of by multisampling (which is switched off while this is on); plants' cut-out edges too. Off: --msaa again. (--graphics)",
            |w| w.resource::<Graphics>().taa,
            |w, on| w.resource_mut::<Graphics>().taa = on,
        ),
        (
            "light-shafts",
            "Sun shafts",
            "On: sunlight streams past what stands in front of the sun, stronger the hazier the weather's fog. Not in the original. (--graphics)",
            |w| w.resource::<Graphics>().light_shafts,
            |w, on| w.resource_mut::<Graphics>().light_shafts = on,
        ),
        (
            "water-reflections",
            "Screen-space water reflections",
            "On: water the mirrored reflection does not reach (another height, interiors) reflects what is on the screen instead of only the sky. (--graphics)",
            |w| w.resource::<Graphics>().water_reflections,
            |w, on| w.resource_mut::<Graphics>().water_reflections = on,
        ),
    ];
    for (key, label, note, get, set) in switches {
        add_setting(app, Setting { key, group: GROUP_GRAPHICS, label, note, get, set });
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
