//! The renderer's switches on the pause menu's settings page (`nvf_play::settings`, drawn
//! by `nvf_play::pause`; `--play` only): the on/off choices of the command line that can
//! change while the game runs. Each reads and writes the state its module uses, so a
//! switch starts as the command line left it, then as `NVForever.ini` stored it (kept by
//! `nvf_play::settings` like every switch of the page), except that in a test run a
//! switch given on the command line keeps its state (`given_switches`, #1382).
//!
//! Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
//! `--no-actors`, FaceGen, weapons, `--no-lodgen`), most numbers (`--radius`; the volumes
//! are the Sound tab, `nvf_play::audio_out::register_settings`)
//! and the test aids. The draw distances are sliders here, each a part of the
//! command line's distance (`draw_distance.rs`); the field of view, the frame-rate cap and the
//! anti-aliasing and shadow-filter choices are sliders whose steps are the choices.

use crate::draw_distance::{self, DrawClass, DrawDistances};
use crate::fly::FlyCam;
use crate::frame_pace::{self, DisplaySync, FramePace};
use crate::graphics::{self, Graphics};
use crate::grass::GrassConfig;
use crate::foot_ik::FootIkSettings;
use crate::head_look::HeadLookSettings;
use crate::lips::LipState;
use crate::lod::LodState;
use crate::nav::NavState;
use crate::perf_overlay::{FpsCounter, Low};
use crate::player_body::PlayerBody;
use crate::sky::SkyState;
use crate::window_mode;
use crate::world::Streamer;
use crate::{Args, Overlay};
use bevy::prelude::*;
use nvf_play::pause::GROUP_MENUS;
use nvf_play::settings::{add_choice_list, add_setting, choice_index, choice_part, ChoiceList, Setting, Slider};

// The pause menu shows the groups as the original's settings categories (Gameplay,
// Display, …), one short word each (nvf_play::pause).
const GROUP_WORLD: &str = "World";
const GROUP_PICTURE: &str = "Display";
const GROUP_DEVELOPER: &str = "Debug";
const GROUP_GRAPHICS: &str = "Graphics";

/// The settings switches this run's command line gave (`nvf_play::settings::GivenSwitches`):
/// the `--name` of each argument, without the `--size` a run taking over a load is given
/// (the window's size then, handoff.rs). They win over the INI in a test run: a timed,
/// measured or pictured run, whose results must be those of its switches (#1382).
pub(crate) fn given_switches(args: &Args, line: &[String]) -> nvf_play::settings::GivenSwitches {
    let handoff = line.iter().any(|a| a == "--handoff" || a.starts_with("--handoff="));
    let given = line
        .iter()
        .filter(|a| a.starts_with("--"))
        .map(|a| a.split('=').next().unwrap_or(a).to_string())
        .filter(|a| !(handoff && a == "--size"))
        .collect();
    let win = args.run_seconds.is_some() || args.log_frame_times.is_some() || args.screenshot.is_some() || args.timelapse.is_some() || args.frame_json.is_some();
    nvf_play::settings::GivenSwitches { given, win }
}

/// List the renderer's switches: those whose module is part of this run.
pub fn register(app: &mut App) {
    let given = given_switches(app.world().resource::<Args>(), &crate::command_line());
    app.insert_resource(given);
    let has_lips = app.world().contains_resource::<LipState>();
    let has_heads = app.world().contains_resource::<HeadLookSettings>();
    let has_feet = app.world().contains_resource::<FootIkSettings>();
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
    if has_feet {
        add_setting(
            app,
            Setting {
                key: "foot-ik",
                group: GROUP_WORLD,
                label: "Feet follow the ground",
                note: "On: on slopes and steps actors bend their knees and set each foot on the ground under it. Off: the legs play the animation as it is. (--no-foot-ik)",
                get: |w| w.resource::<FootIkSettings>().enabled,
                set: |w, on| w.resource_mut::<FootIkSettings>().enabled = on,
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
    // The resolution first, then full screen [owner 2026-10-18].
    add_choice_list(
        app,
        Setting {
            key: "resolution",
            group: GROUP_PICTURE,
            label: "Resolution",
            note: "The window's size, or in full screen the monitor's (its own size: borderless). A new one goes back after 20 seconds unless you keep it. (--size)",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: window_mode::resolution_names,
            chosen: window_mode::resolution_chosen,
            pick: window_mode::resolution_pick,
            save: window_mode::resolution_save,
            load: window_mode::resolution_load,
            ask_keep: true,
        },
    );
    add_setting(
        app,
        Setting {
            key: "borderless",
            group: GROUP_PICTURE,
            label: "Full screen",
            note: "On: the game fills the monitor it is on, without a frame, at the resolution chosen above for full screen. Off: a window of the resolution chosen for a window. (--window borderless)",
            get: |w| w.resource::<Args>().window == window_mode::WindowChoice::Borderless,
            set: window_mode::set_borderless,
            slider: None,
        },
    );
    add_choice_list(
        app,
        Setting {
            key: "monitor",
            group: GROUP_PICTURE,
            label: "Monitor",
            note: "The display the game opens on in full screen (the first display until you pick one). (--window borderless)",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: window_mode::monitor_names,
            chosen: window_mode::monitor_chosen,
            pick: window_mode::monitor_pick,
            save: window_mode::monitor_save,
            load: window_mode::monitor_load,
            ask_keep: false,
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
        add_setting(
            app,
            Setting {
                key: "weapon-sway",
                group: GROUP_PICTURE,
                label: "Weapon sway",
                note: "On: in first person the arms and weapon lag behind a turn and bob as you walk and run. Off: they stay still in the view. (--no-weapon-sway)",
                get: |w| w.resource::<PlayerBody>().weapon_sway,
                set: |w, on| w.resource_mut::<PlayerBody>().weapon_sway = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "head-bob",
                group: GROUP_PICTURE,
                label: "Head bob",
                note: "On: in first person the view dips with each step as you walk and run. Off: it stays level. (--no-head-bob)",
                get: |w| w.resource::<PlayerBody>().head_bob,
                set: |w, on| w.resource_mut::<PlayerBody>().head_bob = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "weapon-lower",
                group: GROUP_PICTURE,
                label: "Lower weapon near walls",
                note: "On: in first person a drawn weapon is lowered when a wall or a friendly person is right in front of you, and comes straight back up when you aim or fire. Off: it stays up and can poke through them, as in the original. Lower Weapon (H) lowers or raises it by hand either way. (--no-weapon-lower)",
                get: |w| w.resource::<PlayerBody>().weapon_lower,
                set: |w, on| w.resource_mut::<PlayerBody>().weapon_lower = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "true-iron-sights",
                group: GROUP_MENUS,
                label: "True iron sights",
                note: "On: aiming a gun lines its sights up with your eye. The crosshair is hidden while you aim down them. Off: the gun is held low with the crosshair, as in the original. Guns with no sights to look down (laser rifle, minigun) aim as the original either way. (--no-true-iron-sights)",
                get: |w| w.resource::<PlayerBody>().true_iron_sights,
                set: |w, on| w.resource_mut::<PlayerBody>().true_iron_sights = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "scope-3d",
                group: GROUP_MENUS,
                label: "3D scopes (test)",
                note: "On: aiming a scoped gun keeps the gun on screen and shows the zoomed view in its scope's lens, with the reticle; the view around it zooms only a little. Off: the full-screen scope picture with its black border, as in the original. Guns whose scope lens is not known yet keep the picture. (--scope-3d)",
                get: |w| w.resource::<nvf_play::aim_zoom::Scope3d>().on,
                set: |w, on| w.resource_mut::<nvf_play::aim_zoom::Scope3d>().on = on,
                slider: None,
            },
        );
    }
    add_setting(
        app,
        Setting {
            key: "sun-shadows",
            group: GROUP_PICTURE,
            label: "Sun shadows",
            note: "On: the sun casts shadows out to --shadow-distance. The original casts none from the world. On by default in the game. (--sun-shadows, --no-sun-shadows)",
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
    register_display(app);
    register_distances(app);
    register_graphics(app);
    // Renderer edits are latched by the shared settings setters, including Defaults.
    // Draw-distance streaming has no resume readiness contract yet (#1820) [design].
    register_pipeline_waits(app);
    // The Sound tab after the picture's, as the original lists Audio after Display.
    nvf_play::audio_out::register_settings(app);
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

/// The Frame rate counter switch's note; the switch it names (`--fps`) decides with the
/// INI whether the frame split takes GPU timestamps from the start
/// (`perf_overlay::split::timestamps_wanted`), as `nvf_play::settings::apply_stored` reads it.
pub(crate) const FPS_COUNTER_NOTE: &str = "On: a small panel of performance readings at the top right of the screen, the Counter rows below choosing what it shows; with only its frame rate row on it is a frame rate counter. Not in the original. (--fps)";

/// The frame split row's note (no command-line switch: the INI's state always applies).
pub(crate) const FPS_SPLIT_NOTE: &str = "What a frame took on the main thread (the game's own work), on the render thread (preparing the picture, including the wait for the window's next image, shown in brackets: with vertical sync that wait is the display's time, not work) and on the graphics card (the render passes it times, from GPU timestamps), each the mean over half a second. A frame held up by the processor has a long main time, or a long render time beside a short wait; the GPU time is what tells a frame held up by the graphics card. The GPU part needs this row and the counter on when the game starts (it takes effect at the next start), and a graphics card that offers timestamps; without them it is left out. Unless a measuring aid also uses them, the timestamps are taken only while the row shows.";

/// The field of view's least and greatest degrees on the settings page, in steps of 5.
const FOV_MIN: f32 = 50.0;
const FOV_MAX: f32 = 110.0;
const FOV_CHOICES: usize = 13;

/// The field of view (`--fov`), the frame rate counter (the performance overlay,
/// `perf_overlay/`: its switch and a switch per row) and how fast frames are
/// shown (`frame_pace.rs`; not while `--log-frame-times` measures, which wants vsync off).
fn register_display(app: &mut App) {
    add_setting(
        app,
        Setting::slider(
            "field-of-view",
            GROUP_PICTURE,
            "Field of view",
            "The game's field of view in degrees across a 4:3 picture (75 as the original's default); wider screens see more at the sides. (--fov)",
            Slider::choice(FOV_CHOICES, |w| fov_part(w.resource::<Args>().fov), set_fov, |v| format!("{:.0}", fov_degrees(v))),
        ),
    );
    // The HUD's size (#1631), kept by nvf-play's interface and fixed for the run at start.
    add_setting(app, nvf_play::interface::ui_scale_row(GROUP_PICTURE));
    add_setting(
        app,
        Setting {
            key: "fps-counter",
            group: GROUP_PICTURE,
            label: "Frame rate counter",
            note: FPS_COUNTER_NOTE,
            get: |w| w.resource::<FpsCounter>().show,
            set: |w, on| w.resource_mut::<FpsCounter>().show = on,
            slider: None,
        },
    );
    register_counter_rows(app);
    if app.world().resource::<Args>().log_frame_times.is_some() {
        return;
    }
    add_choice_list(
        app,
        Setting {
            key: "display-sync",
            group: GROUP_PICTURE,
            label: "Display sync",
            note: "Vertical sync: each frame waits for the display's refresh (no tearing). Variable refresh: for a G-Sync or FreeSync screen, vertical sync with the frame rate held a few frames under the screen's refresh rate, so frames are shown as soon as they are drawn without tearing. Off: frames are shown as soon as they are drawn (may tear).",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| DisplaySync::ALL.iter().map(|d| d.name().to_string()).collect(),
            chosen: |w| w.resource::<FramePace>().sync.name().to_string(),
            pick: set_display_sync,
            save: |w| w.resource::<FramePace>().sync.stored().to_string(),
            load: set_display_sync,
            ask_keep: false,
        },
    );
    app.add_systems(PostStartup, read_old_vsync.after(nvf_play::settings::apply_stored));
    let caps = frame_pace::CAPS.len();
    add_setting(
        app,
        Setting::slider(
            "frame-cap",
            GROUP_PICTURE,
            "Frame rate limit",
            "At most this many frames a second (Off: as many as the computer draws, or the display's refresh with vertical sync; with variable refresh the lower of this and its own limit). A lower limit keeps the computer cooler and quieter.",
            Slider::choice(
                caps,
                |w| {
                    let cap = w.resource::<FramePace>().cap;
                    choice_part(frame_pace::CAPS.iter().position(|&c| c == cap).unwrap_or(0), frame_pace::CAPS.len())
                },
                |w, v| w.resource_mut::<FramePace>().cap = frame_pace::CAPS[choice_index(v, frame_pace::CAPS.len())],
                |v| match frame_pace::CAPS[choice_index(v, frame_pace::CAPS.len())] {
                    0 => "Off".to_string(),
                    c => c.to_string(),
                },
            ),
        ),
    );
}

/// The frame rate counter's rows, listed under its switch (issue #1868; the
/// defaults are the issue's, `perf_overlay::Rows`). Each note says what the row reads and
/// where from.
fn register_counter_rows(app: &mut App) {
    let row = |key: &'static str, label: &'static str, note: &'static str, get: fn(&World) -> bool, set: fn(&mut World, bool)| Setting { key, group: GROUP_PICTURE, label, note, get, set, slider: None };
    add_setting(
        app,
        row(
            "fps-counter-fps",
            "Counter: frame rate",
            "The frames a second and the mean frame time of the last half second, and the 1% low (the mean of the slowest hundredth of the frames of the last 10 seconds) or the slowest frame of the last 10 seconds, as the row below chooses. Measured from the time between frames.",
            |w| w.resource::<FpsCounter>().rows.fps,
            |w, on| w.resource_mut::<FpsCounter>().rows.fps = on,
        ),
    );
    add_choice_list(
        app,
        Setting {
            key: "fps-counter-low",
            group: GROUP_PICTURE,
            label: "Counter: last figure",
            note: "The frame rate row's last figure, over the last 10 seconds: 1% low, the mean of the slowest hundredth of the frames; Slowest frame, the single slowest.",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| Low::ALL.iter().map(|l| l.name().to_string()).collect(),
            chosen: |w| w.resource::<FpsCounter>().rows.low.name().to_string(),
            pick: set_counter_low,
            save: |w| w.resource::<FpsCounter>().rows.low.stored().to_string(),
            load: set_counter_low,
            ask_keep: false,
        },
    );
    let rows = [
        row(
            "fps-counter-graph",
            "Counter: frame graph",
            "The frame times of the last 5 seconds as a line under the frame rate, the newest at the right, with a faint line at a 60 Hz frame (16.7 ms); its top is the slowest frame drawn, or two 60 Hz frames when every frame was faster, so a spike reaches the top. Measured from the time between frames, drawn again twice a second.",
            |w| w.resource::<FpsCounter>().rows.graph,
            |w, on| w.resource_mut::<FpsCounter>().rows.graph = on,
        ),
        row(
            "fps-counter-split",
            "Counter: frame split",
            FPS_SPLIT_NOTE,
            |w| w.resource::<FpsCounter>().rows.split,
            |w, on| w.resource_mut::<FpsCounter>().rows.split = on,
        ),
        row(
            "fps-counter-cpu",
            "Counter: CPU use",
            "The whole machine's processor use, as Task Manager shows it, and the game's own as cores kept busy: 1.0 is all of one core's time, so a thread held at its limit reads at least 1.0 however many cores the machine has. From Windows' performance counters and the game's processor time, read once a second while the counter shows. Windows only.",
            |w| w.resource::<FpsCounter>().rows.cpu,
            |w, on| w.resource_mut::<FpsCounter>().rows.cpu = on,
        ),
        row(
            "fps-counter-gpu",
            "Counter: GPU use",
            "The use of the graphics card's busiest engine by every program, as Task Manager shows it. From Windows' performance counters, read once a second while the counter shows. Windows only.",
            |w| w.resource::<FpsCounter>().rows.gpu,
            |w, on| w.resource_mut::<FpsCounter>().rows.gpu = on,
        ),
        row(
            "fps-counter-vram",
            "Counter: GPU memory",
            "The graphics memory the game holds against the budget the driver gives it (Vulkan's memory budget, read every 2 seconds). It is the driver's figure: it counts the blocks kept empty for reuse, so it is what the game holds, not the size of what is drawn. Left out on a graphics chip that shares the machine's RAM, which has no budget of its own. Marked ! while the game keeps fewer unused models and textures because it is near the budget.",
            |w| w.resource::<FpsCounter>().rows.vram,
            |w, on| w.resource_mut::<FpsCounter>().rows.vram = on,
        ),
        row(
            "fps-counter-ram",
            "Counter: RAM",
            "The game's memory in RAM (its working set, from Windows once a second while the counter shows) and the share of the machine's RAM in use (read every 2 seconds). Marked ! while the game keeps fewer unused models and textures because the machine has under a tenth of its RAM free. Windows only.",
            |w| w.resource::<FpsCounter>().rows.ram,
            |w, on| w.resource_mut::<FpsCounter>().rows.ram = on,
        ),
        row(
            "fps-counter-loaded",
            "Counter: loaded",
            "What the game holds now: the cells loaded in the physics world, the placed objects with a model, the actors with a skeleton and every entity of the engine, counted twice a second while the row shows (the stress tests' counts).",
            |w| w.resource::<FpsCounter>().rows.loaded,
            |w, on| w.resource_mut::<FpsCounter>().rows.loaded = on,
        ),
    ];
    for r in rows {
        add_setting(app, r);
    }
}

/// The frame rate row's last figure of this name (or INI text, `Low::parse`); false: not
/// one.
fn set_counter_low(world: &mut World, name: &str) -> bool {
    let Some(low) = Low::parse(name) else { return false };
    world.resource_mut::<FpsCounter>().rows.low = low;
    true
}

/// `[Settings] <key>` of the INI this run reads (`SettingsFile::for_run`), for what has to
/// be known when the app is built, before the INI is applied (`apply_stored`, PostStartup):
/// the CPU placement (`cpu_placement.rs`), the frame split's GPU timestamps
/// (`perf_overlay::split`). None without the INI or the key.
pub(crate) fn stored_at_start(key: &str) -> Option<String> {
    // Unattended: a test aid drives this run (as nvf_play's test_aid::Unattended).
    let unattended = std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run));
    let path = nvf_play::settings::SettingsFile::for_run(unattended).0?;
    let text = std::fs::read_to_string(path).ok()?;
    fnv_install::ini_get(&text, nvf_play::settings::SECTION, key)
}

/// The display sync of this name (or INI text, `DisplaySync::parse`); false: not one.
fn set_display_sync(world: &mut World, name: &str) -> bool {
    let Some(d) = DisplaySync::parse(name) else { return false };
    world.resource_mut::<FramePace>().sync = d;
    true
}

/// An INI written before the display-sync row had the `vsync` switch: read it when there is
/// no `display-sync` (after `nvf_play::settings::apply_stored`, so it counts as stored).
fn read_old_vsync(world: &mut World) {
    if world.resource::<Args>().log_frame_times.is_some() {
        return;
    }
    let Some(path) = world.get_resource::<nvf_play::settings::SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let section = nvf_play::settings::SECTION;
    if fnv_install::ini_get(&text, section, "display-sync").is_some() {
        return;
    }
    let Some(old) = fnv_install::ini_get(&text, section, "vsync") else { return };
    match DisplaySync::parse(&old) {
        Some(d) => {
            world.resource_mut::<FramePace>().sync = d;
            info!("settings: display-sync {} from the old vsync={} in {}", d.name(), old.trim(), path.display());
        }
        None => warn!("settings: [{section}] vsync={old} in {} is not 1 or 0; left as it is", path.display()),
    }
}

/// The field of view's degrees for a slider part, whole.
fn fov_degrees(v: f32) -> f32 {
    (FOV_MIN + v.clamp(0.0, 1.0) * (FOV_MAX - FOV_MIN)).round()
}

/// The slider part for `deg` degrees, as `Slider::fit` keeps parts.
fn fov_part(deg: f32) -> f32 {
    ((deg - FOV_MIN) / (FOV_MAX - FOV_MIN) * 1000.0).round().clamp(0.0, 1000.0) / 1000.0
}

/// A new field of view: kept in `Args` and put on the camera.
fn set_fov(world: &mut World, v: f32) {
    let deg = fov_degrees(v);
    world.resource_mut::<Args>().fov = deg;
    let mut cams = world.query_filtered::<&mut Projection, With<FlyCam>>();
    for mut p in cams.iter_mut(world) {
        if let Projection::Perspective(p) = &mut *p {
            p.fov = nvf_play::free_cam::vertical_fov(deg);
        }
    }
}

/// The draw-distance sliders (`draw_distance.rs`): each a part of its full distance, the
/// full one drawing as without them.
fn register_distances(app: &mut App) {
    if !app.world().contains_resource::<DrawDistances>() {
        return;
    }
    let slider = |get: fn(&World) -> f32, set: fn(&mut World, f32)| Slider { get, set, min: draw_distance::MIN, step: draw_distance::STEP, shown: None };
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
    add_choice_list(
        app,
        Setting {
            key: "fsr-upscaling",
            group: GROUP_GRAPHICS,
            label: "FSR 3.1 upscaling",
            note: "Reconstruct the world at display resolution; Native AA keeps full resolution. Disables MSAA, FXAA, SMAA and TAA while active. Menus stay sharp. Requires Vulkan; unsupported devices keep native rendering. Sharpening controls FSR sharpening. Frame generation is the row below. (--fsr)",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| graphics::FSR_CHOICES.iter().map(|(name, _)| (*name).to_string()).collect(),
            chosen: fsr_chosen,
            pick: pick_fsr,
            save: fsr_chosen,
            load: load_fsr,
            ask_keep: false,
        },
    );
    let choices = [
        Setting::slider(
            "anti-aliasing",
            GROUP_GRAPHICS,
            "Anti-aliasing",
            "How edges are smoothed: multisampling Off, 2x, 4x or 8x (--msaa; the cut-out edges of plants too), or the picture smoothed afterwards: FXAA (cheapest, softest), SMAA (sharper, a little dearer) or TAA (over the frames; plants too, soft while moving; --graphics-only fxaa, smaa or taa). Multisampling is the dearest, more at each step: at 1920x1080 on an RTX 4070, 2x added about 0.8 ms a frame, 4x about 1.3 ms and 8x about 2.3 ms (Goodsprings and Novac at noon); FXAA, SMAA and TAA cost less than 4x. Where the processor sets the pace (the Strip at night) the choice costs next to nothing. While FSR 3.1 upscaling is on it does the anti-aliasing itself, so this row reads Off (FSR active); your choice comes back when FSR is Off.",
            Slider::choice(graphics::AA_CHOICES.len(), |w| choice_part(graphics::aa_choice(w), graphics::AA_CHOICES.len()), |w, v| graphics::set_aa_choice(w, choice_index(v, graphics::AA_CHOICES.len())), |v| {
                graphics::AA_CHOICES[choice_index(v, graphics::AA_CHOICES.len())].0.to_string()
            }),
        ),
        Setting::slider(
            "shadow-filter",
            GROUP_GRAPHICS,
            "Shadow filtering",
            "How the edges of the sun's shadows are softened: Fast (hardware 2x2, harder edges, cheapest), Smooth (as vanilla) or Temporal (soft but grainy unless Anti-aliasing is TAA).",
            Slider::choice(
                graphics::SHADOW_FILTERS.len(),
                |w| {
                    let f = w.resource::<Graphics>().shadow_filter;
                    choice_part(graphics::SHADOW_FILTERS.iter().position(|&(_, m)| m == f).unwrap_or(1), graphics::SHADOW_FILTERS.len())
                },
                |w, v| w.resource_mut::<Graphics>().shadow_filter = graphics::SHADOW_FILTERS[choice_index(v, graphics::SHADOW_FILTERS.len())].1,
                |v| graphics::SHADOW_FILTERS[choice_index(v, graphics::SHADOW_FILTERS.len())].0.to_string(),
            ),
        ),
    ];
    for s in choices {
        add_setting(app, s);
    }
    nvf_play::settings::set_disabled_status(app, "anti-aliasing", |w| graphics::fsr_active(w).then_some("Off (FSR active)"));
    add_setting(
        app,
        Setting {
            key: "frame-generation",
            group: GROUP_GRAPHICS,
            label: "Frame generation",
            note: "FSR 3.1 frame generation: a frame made between each two real frames, so motion looks twice as smooth; what you do reaches the screen a little later. Needs FSR 3.1 upscaling (any mode) and vertical sync or variable refresh; real frames are held to half the refresh rate, and latency reduction turns on with it. The HUD and menus are not generated; menus, loading screens and camera cuts show real frames only. Best from 60 real frames a second. Not in the original. (--frame-generation)",
            get: |w| w.resource::<crate::frame_gen::FrameGeneration>().on,
            set: |w, on| w.resource_mut::<crate::frame_gen::FrameGeneration>().on = on,
            slider: None,
        },
    );
    nvf_play::settings::set_disabled_status(app, "frame-generation", crate::frame_gen::unavailable_reason);
    add_setting(
        app,
        Setting {
            key: "latency-reduction",
            group: GROUP_GRAPHICS,
            label: "Latency reduction",
            note: "AMD Anti-Lag: the graphics driver keeps the game from preparing frames ahead of the graphics card. It is meant to make what you do reach the screen sooner; that has not been measured on an AMD card yet. AMD graphics cards with Anti-Lag in their Vulkan driver (Windows: Adrenalin 24.9.1 or later; Linux: Mesa 26.3 or later). Not in the original. (--latency-reduction)",
            get: |w| w.resource::<crate::anti_lag::LatencyReduction>().on,
            set: |w, on| w.resource_mut::<crate::anti_lag::LatencyReduction>().on = on,
            slider: None,
        },
    );
    nvf_play::settings::set_disabled_status(app, "latency-reduction", |w| (!crate::anti_lag::available(w)).then_some("Not on this GPU"));
    let switches: [Switch; 11] = [
        (
            "ambient-occlusion",
            "Ambient occlusion",
            "On: creases, corners and the ground under things are darkened by how enclosed they are (screen-space, from the depth). Not in the original. (--graphics)",
            |w| w.resource::<Graphics>().ambient_occlusion,
            |w, on| w.resource_mut::<Graphics>().ambient_occlusion = on,
        ),
        (
            "fine-shadows",
            "Longer sun shadows",
            "On: the sun's shadows reach half as far again as --shadow-distance. Costs a little frame rate. (--graphics)",
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
            "glow",
            "Light glow (HDR)",
            "On: neon, lamps, glowing beams and fire glow softly into their surroundings, as the original's HDR glow does; how much follows the place's image space (the Strip's neon at night most). Bloom, when on, adds to it. (--graphics-only no-glow turns it off)",
            |w| w.resource::<Graphics>().glow,
            |w, on| w.resource_mut::<Graphics>().glow = on,
        ),
        (
            "eye-adaptation",
            "Eye adaptation",
            "On: dark views brighten after a moment, as the eye gets used to the dark, by the place's image space. Needs the light glow. (--graphics-only no-eye-adaptation turns it off)",
            |w| w.resource::<Graphics>().eye_adaptation,
            |w, on| w.resource_mut::<Graphics>().eye_adaptation = on,
        ),
        (
            "neon-glow",
            "Neon glow",
            "On: where the place's image space asks for a strong glow (the Strip at night) neon tubes and signs burn with a wide halo, as in the original. Needs the light glow. (--graphics-only no-neon-glow turns it off)",
            |w| w.resource::<Graphics>().neon_glow,
            |w, on| w.resource_mut::<Graphics>().neon_glow = on,
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
        (
            "water-mirror",
            "Mirrored water reflections",
            "On: water mirrors the world above it, as in the original; the scene is drawn a second time for it while water is on screen. Off: water reflects the sky, and what is on the screen with screen-space reflections. (--graphics-only no-water-mirror turns it off)",
            |w| w.resource::<Graphics>().water_mirror,
            |w, on| w.resource_mut::<Graphics>().water_mirror = on,
        ),
    ];
    for (key, label, note, get, set) in switches {
        add_setting(app, Setting { key, group: GROUP_GRAPHICS, label, note, get, set, slider: None });
    }
}

fn register_pipeline_waits(app: &mut App) {
    for key in [
        "anti-aliasing", "shadow-filter", "fsr-upscaling", "ambient-occlusion",
        "fine-shadows", "bloom", "glow", "eye-adaptation", "neon-glow", "sharpen",
        "light-shafts", "water-reflections", "water-mirror", "sun-shadows", "grade",
    ] {
        nvf_play::settings::wait_for_pipelines(app, key);
    }
}

fn fsr_chosen(world: &World) -> String {
    let quality = world.resource::<Graphics>().fsr;
    graphics::FSR_CHOICES.iter().find(|(_, mode)| mode.quality() == quality).map_or("Off", |(name, _)| *name).to_string()
}

fn pick_fsr(world: &mut World, name: &str) -> bool {
    let Some((_, mode)) = graphics::FSR_CHOICES.iter().find(|(label, _)| label.eq_ignore_ascii_case(name.trim())) else { return false };
    world.resource_mut::<Graphics>().fsr = mode.quality();
    true
}

fn load_fsr(world: &mut World, value: &str) -> bool {
    if pick_fsr(world, value) {
        return true;
    }
    // Preserve settings written by the early local slider build; new files store
    // the actual choice name, independent of the number/order of future options.
    let Some(part) = value.trim().parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=1.0).contains(v)) else { return false };
    world.resource_mut::<Graphics>().fsr = graphics::FSR_CHOICES[choice_index(part, graphics::FSR_CHOICES.len())].1.quality();
    true
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

#[cfg(test)]
mod fsr_settings_tests {
    use super::*;

    #[test]
    fn renderer_registration_covers_aa_ao_fsr_and_defaults_without_latching_no_ops() {
        use nvf_play::{graphics_resume::GraphicsResume, pause::Paused, settings, PlayState};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Off)).init_resource::<GraphicsResume>()
            .insert_resource(Paused(true)).insert_resource(State::new(PlayState::Menu));
        register_graphics(&mut app);
        register_pipeline_waits(&mut app);
        let rows = app.world().resource::<settings::Settings>();
        let aa = rows.index("anti-aliasing").unwrap();
        let ao = rows.index("ambient-occlusion").unwrap();
        let fsr = rows.index("fsr-upscaling").unwrap();
        let world = app.world_mut();
        settings::set_slider(world, aa, 0.0);
        settings::restore(world, ao, 0.0);
        assert!(!settings::pick(world, fsr, "Off"));
        assert!(world.resource_mut::<GraphicsResume>().request(0.0));
        settings::set_slider(world, aa, 1.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::restore(world, aa, 0.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::flip(world, ao);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::pick(world, fsr, "Quality");
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::restore(world, fsr, 0.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0), "Defaults also restores named choices");
    }

    #[test]
    fn native_aa_row_is_off_and_locked_for_every_supported_fsr_mode_then_restores_the_saved_choice() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Sample8))
            .insert_resource(crate::fsr::FsrSupport(true));
        register_graphics(&mut app);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("anti-aliasing").unwrap();
        for (_, mode) in graphics::FSR_CHOICES.into_iter().filter(|(_, m)| m.quality().is_some()) {
            app.world_mut().resource_mut::<Graphics>().fsr = mode.quality();
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), Some("Off (FSR active)"));
            let saved = settings.list()[row].value(app.world());
            assert_eq!(saved, choice_part(3, graphics::AA_CHOICES.len()));
            assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_none());
            assert!(nvf_play::settings::step_slider(app.world_mut(), row, 1).is_none());
            assert_eq!(graphics::aa_choice(app.world()), 3, "disabled UI must preserve saved 8x");
            app.world_mut().resource_mut::<Graphics>().fsr = None;
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), None);
            assert_eq!(settings.list()[row].slider.unwrap().text(saved), "8x");
        }
        app.world_mut().resource_mut::<crate::fsr::FsrSupport>().0 = false;
        app.world_mut().resource_mut::<Graphics>().fsr = Some(crate::fsr::FsrQuality::NativeAa);
        assert_eq!(app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), row), None, "unsupported FSR leaves native AA editable");
        assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_some());
        assert_eq!(graphics::aa_choice(app.world()), 0);
    }

    #[test]
    fn fsr_is_a_named_dropdown_and_round_trips_every_mode() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        let row = settings.index("fsr-upscaling").unwrap();
        assert!(settings.list()[row].slider.is_none());
        let dropdown = settings.choices(row).expect("FSR must use the native dropdown widget");
        assert_eq!((dropdown.names)(app.world()).len(), 6);
        for (name, mode) in graphics::FSR_CHOICES {
            assert!((dropdown.pick)(app.world_mut(), name));
            assert_eq!(app.world().resource::<Graphics>().fsr, mode.quality());
            let saved = (dropdown.save)(app.world());
            app.world_mut().resource_mut::<Graphics>().fsr = None;
            assert!((dropdown.load)(app.world_mut(), &saved));
            assert_eq!((dropdown.chosen)(app.world()), name);
        }
        assert!(!pick_fsr(app.world_mut(), "unknown"));
        assert!(!load_fsr(app.world_mut(), "NaN"));
        assert!(!load_fsr(app.world_mut(), "2"));
        assert!(load_fsr(app.world_mut(), "0.40"));
        assert_eq!(fsr_chosen(app.world()), "Quality");
    }
}
