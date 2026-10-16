//! The renderer's switches on the pause menu's settings page (`nvf_play::settings`, drawn
//! by `nvf_play::pause`; `--play` only): the on/off choices of the command line that can
//! change while the game runs. Each reads and writes the state its module uses, so a
//! switch starts as the command line left it, then as `NVForever.ini` stored it (kept by
//! `nvf_play::settings` like every switch of the page).
//!
//! Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
//! `--no-actors`, FaceGen, weapons, `--no-lodgen`), most numbers (`--msaa`, `--fov`,
//! volumes) and the test aids. The draw distances are sliders here, each a part of the
//! command line's distance (`draw_distance.rs`).

use crate::draw_distance::{self, DrawClass, DrawDistances};
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
use nvf_play::settings::{add_setting, Setting, Slider};

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
                slider: None,
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
                slider: None,
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
            slider: None,
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
                slider: None,
            },
        );
    }
    add_setting(
        app,
        Setting {
            key: "borderless",
            group: GROUP_PICTURE,
            label: "Borderless full screen",
            note: "On: the game fills the monitor it is on, without a frame. Off: a window of --size. (--window borderless)",
            get: |w| w.resource::<Args>().window == crate::window_mode::WindowChoice::Borderless,
            set: crate::window_mode::set_borderless,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "sun-shadows",
            group: GROUP_PICTURE,
            label: "Sun shadows",
            note: "On: the sun casts shadows out to --shadow-distance. The original casts none from the world. (--sun-shadows)",
            get: |w| w.resource::<SkyState>().shadows,
            set: crate::sky::set_shadows,
            slider: None,
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
            slider: None,
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
            slider: None,
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
            slider: None,
        },
    );
    register_distances(app);
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
            slider: None,
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
            slider: None,
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
            slider: None,
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
                slider: None,
            },
        );
    }
}

/// The draw-distance sliders (`draw_distance.rs`): each a part of its full distance, the
/// full one drawing as without them.
fn register_distances(app: &mut App) {
    if !app.world().contains_resource::<DrawDistances>() {
        return;
    }
    let slider = |get: fn(&World) -> f32, set: fn(&mut World, f32)| Slider { get, set, min: draw_distance::MIN, step: draw_distance::STEP };
    let sliders = [
        Setting::slider(
            "object-distance",
            GROUP_PICTURE,
            "Object distance",
            "How far from you objects (buildings, rocks, furniture, clutter) of the loaded cells are drawn, as a part of the farthest those cells reach. Lower draws fewer and runs faster.",
            slider(|w| w.resource::<DrawDistances>().objects, |w, v| draw_distance::set_part(w, DrawClass::Object, v)),
        ),
        Setting::slider(
            "actor-distance",
            GROUP_PICTURE,
            "Actor distance",
            "How far from you people and creatures are drawn, as a part of the farthest the loaded cells reach.",
            slider(|w| w.resource::<DrawDistances>().actors, |w, v| draw_distance::set_part(w, DrawClass::Actor, v)),
        ),
        Setting::slider(
            "item-distance",
            GROUP_PICTURE,
            "Item distance",
            "How far from you items lying in the world (weapons, ammo, aid, notes, caps, clutter you can take) are drawn, as a part of the farthest the loaded cells reach.",
            slider(|w| w.resource::<DrawDistances>().items, |w, v| draw_distance::set_part(w, DrawClass::Item, v)),
        ),
        Setting::slider(
            "shadow-distance",
            GROUP_PICTURE,
            "Shadow distance",
            "How far from you the sun casts shadows (with Sun shadows on), as a part of --shadow-distance.",
            slider(|w| w.resource::<DrawDistances>().shadows, draw_distance::set_shadows),
        ),
        Setting::slider(
            "grass-distance",
            GROUP_PICTURE,
            "Grass distance",
            "How far from you grass is drawn, as a part of --grass-distance and its fade range.",
            slider(|w| w.resource::<DrawDistances>().grass, draw_distance::set_grass),
        ),
    ];
    for s in sliders {
        add_setting(app, s);
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
        add_setting(app, Setting { key, group: GROUP_GRAPHICS, label, note, get, set, slider: None });
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
