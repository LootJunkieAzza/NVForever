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
use crate::upscaler::UpscalerChoice;
use crate::window_mode;
use crate::world::Streamer;
use crate::{Args, Overlay};
use bevy::prelude::*;
use nvf_play::pause::GROUP_MENUS;
use nvf_play::settings::{add_choice_list, add_setting, choice_index, choice_part, ChoiceList, Setting, Slider};

// The groups the rows register with, one short word each; which tab and heading the pause
// menu shows each row under is nvf_play::settings_layout's table, not the group (#1953).
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
    // The Quality and Enhanced effects rows over the rows above (#1966).
    crate::quality::register(app, GROUP_GRAPHICS);
    // Renderer edits are latched by the shared settings setters, including Defaults.
    // Draw-distance streaming has no resume readiness contract yet (#1820) [design].
    register_pipeline_waits(app);
    // The volumes after the picture's rows (shown on the Audio tab: nvf_play::settings_layout).
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
            "The frames a second and the mean frame time of the last half second, and the 1% low (the mean of the slowest hundredth of the frames of the last 10 seconds) or the slowest frame of the last 10 seconds, as the row below chooses. Measured from the time between frames. While frame generation shows generated frames, the frames shown a second come first and the rendered ones in brackets; the frame times are the rendered frames'.",
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

/// The upscaling row's key; it kept the name it had when FSR 3.1 was its only upscaler, so
/// the INIs and test aids written then still work.
pub(crate) const UPSCALING_KEY: &str = "fsr-upscaling";

/// The upscaling row's note. It names the command-line switches the row starts from, so a test
/// run that gives one keeps it over the INI (`nvf_play::settings::GivenSwitches`), at the start
/// ([`upscaler_after_ini`]) as at `PostStartup`.
pub(crate) const UPSCALING_NOTE: &str = "Reconstruct the world at display resolution with FSR 3.1 or NVIDIA DLSS; Native AA keeps full resolution. Disables MSAA, FXAA, SMAA and TAA while active. Menus stay sharp. Requires Vulkan; unsupported devices keep native rendering. DLSS needs an NVIDIA RTX graphics card and NVIDIA's DLSS files in the dlss folder beside nvforever.exe (NVForever ships none); choosing DLSS or leaving it takes effect at the next start (loading a save starts the game again). Sharpening controls FSR sharpening. Frame generation is the row below and needs FSR. (--upscaler, --upscaler-quality, --fsr)";

/// The engine-level improvements of `graphics.rs` (`--graphics enhanced` starts with all
/// of them on, `vanilla` with none).
fn register_graphics(app: &mut App) {
    type Switch = (&'static str, &'static str, &'static str, fn(&World) -> bool, fn(&mut World, bool));
    // One row for every temporal upscaler (#2198): off, FSR 3.1 at each quality under the
    // names it always had, then DLSS at each quality (`graphics::UPSCALER_CHOICES`).
    add_choice_list(
        app,
        Setting {
            key: UPSCALING_KEY,
            group: GROUP_GRAPHICS,
            label: "Upscaling",
            note: UPSCALING_NOTE,
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| graphics::UPSCALER_CHOICES.iter().map(|(name, _)| (*name).to_string()).collect(),
            chosen: upscaler_chosen,
            pick: pick_upscaler,
            save: upscaler_chosen,
            load: load_upscaler,
            ask_keep: false,
        },
    );
    nvf_play::settings::set_choice_labels(app, UPSCALING_KEY, upscaler_label);
    nvf_play::settings::set_choice_unavailable(app, UPSCALING_KEY, upscaler_unavailable);
    let choices = [
        Setting::slider(
            "anti-aliasing",
            GROUP_GRAPHICS,
            "Anti-aliasing",
            "How edges are smoothed: multisampling Off, 2x, 4x or 8x (--msaa; the cut-out edges of plants too), or the picture smoothed afterwards: FXAA (cheapest, softest), SMAA (sharper, a little dearer) or TAA (over the frames; plants too, soft while moving; --graphics-only fxaa, smaa or taa). Multisampling is the dearest, more at each step: at 1920x1080 on an RTX 4070, 2x added about 0.8 ms a frame, 4x about 1.3 ms and 8x about 2.3 ms (Goodsprings and Novac at noon); FXAA, SMAA and TAA cost less than 4x. Where the processor sets the pace (the Strip at night) the choice costs next to nothing. While Upscaling (FSR 3.1 or DLSS) is on it does the anti-aliasing itself, so this row reads Off (upscaling active); your choice comes back when Upscaling is Off.",
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
    nvf_play::settings::set_disabled_status(app, "anti-aliasing", |w| crate::upscaler::temporal_active(w).then_some("Off (upscaling active)"));
    add_setting(
        app,
        Setting {
            key: "frame-generation",
            group: GROUP_GRAPHICS,
            label: "Frame generation",
            note: "FSR 3.1 frame generation: a frame made between each two real frames, so motion looks twice as smooth; what you do reaches the screen a little later. Needs FSR 3.1 upscaling (any mode; not with DLSS) and vertical sync or variable refresh; real frames are held to just under half the refresh rate, and latency reduction turns on with it. The HUD and menus are not generated; menus, loading screens and camera cuts show real frames only. Best from 60 real frames a second. Not in the original. (--frame-generation)",
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

/// The requested upscaler's name in the row (`graphics::UPSCALER_CHOICES`).
fn upscaler_chosen(world: &World) -> String {
    let requested = world.resource::<Graphics>().upscaler;
    graphics::UPSCALER_CHOICES.iter().find(|(_, choice)| *choice == requested).map_or("Off", |(name, _)| *name).to_string()
}

/// The choice of this name (any case).
fn upscaler_named(name: &str) -> Option<UpscalerChoice> {
    graphics::UPSCALER_CHOICES.iter().find(|(label, _)| label.eq_ignore_ascii_case(name.trim())).map(|(_, choice)| *choice)
}

fn pick_upscaler(world: &mut World, name: &str) -> bool {
    let Some(choice) = upscaler_named(name) else { return false };
    world.resource_mut::<Graphics>().upscaler = choice;
    true
}

/// The row's INI text read as the row reads it: a choice's name, or a number written by the
/// early local slider build of the FSR row (a part of its six FSR choices, which is all that
/// build had; new files store the choice's name). `None`: not one of the row's states. The
/// start reads the INI with it too ([`upscaler_after_ini`]), so the two cannot disagree.
pub(crate) fn parse_upscaler(value: &str) -> Option<UpscalerChoice> {
    if let Some(choice) = upscaler_named(value) {
        return Some(choice);
    }
    let part = value.trim().parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=1.0).contains(v))?;
    Some(UpscalerChoice::from_fsr(graphics::FSR_CHOICES[choice_index(part, graphics::FSR_CHOICES.len())].1.quality()))
}

/// Loads the INI's text, also a DLSS choice DLSS cannot meet this run: the request is kept
/// and native draws with the reason (`upscaler::resolve`).
fn load_upscaler(world: &mut World, value: &str) -> bool {
    let Some(choice) = parse_upscaler(value) else { return false };
    world.resource_mut::<Graphics>().upscaler = choice;
    true
}

/// The words the row shows for a choice: `Off`, `FSR 3.1: Quality`, `DLSS: Quality`, the
/// quality in the language shown; the requested one with "(from the next start)" while a
/// restart is pending for it (`upscaler::UpscalerStatus::restart_pending`).
fn upscaler_label(world: &World, name: &str) -> String {
    use fnv_util::words::{tr_str, trf};
    let Some(choice) = upscaler_named(name) else { return name.to_string() };
    let words = match choice {
        UpscalerChoice::Off => tr_str("Off").to_string(),
        UpscalerChoice::Fsr(q) => trf("FSR 3.1: {quality}", &[("quality", &tr_str(q.name()))]),
        UpscalerChoice::Dlss(q) => trf("DLSS: {quality}", &[("quality", &tr_str(q.name()))]),
    };
    let pending = world.get_resource::<crate::upscaler::UpscalerStatus>().is_some_and(|s| s.restart_pending());
    if pending && world.get_resource::<Graphics>().is_some_and(|g| g.upscaler == choice) {
        trf("{choice} (from the next start)", &[("choice", &words)])
    } else {
        words
    }
}

/// Why a DLSS choice cannot be taken this run: DLSS unavailable for a reason a restart with
/// DLSS chosen would meet again (`upscaler::Unavailable::words`). Off and FSR are always in
/// reach (FSR on a device without it draws natively, as before). A stored DLSS choice still
/// loads from the INI.
fn upscaler_unavailable(world: &World, name: &str) -> Option<String> {
    if !upscaler_named(name)?.is_dlss() {
        return None;
    }
    let why = match world.get_resource::<crate::upscaler::UpscalerStatus>() {
        Some(status) => status.dlss_unavailable(),
        None => match crate::upscaler::dlss_state_now() {
            crate::upscaler::DlssState::Unavailable(why) => Some(why),
            _ => None,
        },
    };
    why.map(|why| why.words().to_string())
}

/// The upscaler this run starts with, decided before the renderer exists (Streamline is only
/// loaded for DLSS; `main`): the command line's choice, then the INI's text by
/// `nvf_play::settings::apply_stored`'s precedence (`after_ini`), read with the row's own rule
/// ([`parse_upscaler`]). `given`: the switches this run was given, `None` when it has no
/// settings page (not `--play`: the INI is not applied, the command line decides).
pub(crate) fn upscaler_after_ini(command_line: UpscalerChoice, stored: Option<&str>, given: Option<&nvf_play::settings::GivenSwitches>) -> UpscalerChoice {
    let Some(given) = given else { return command_line };
    nvf_play::settings::after_ini(command_line, stored.and_then(parse_upscaler), given, UPSCALING_NOTE)
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
            app.world_mut().resource_mut::<Graphics>().set_fsr(mode.quality());
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), Some("Off (upscaling active)"));
            let saved = settings.list()[row].value(app.world());
            assert_eq!(saved, choice_part(3, graphics::AA_CHOICES.len()));
            assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_none());
            assert!(nvf_play::settings::step_slider(app.world_mut(), row, 1).is_none());
            assert_eq!(graphics::aa_choice(app.world()), 3, "disabled UI must preserve saved 8x");
            app.world_mut().resource_mut::<Graphics>().set_fsr(None);
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), None);
            assert_eq!(settings.list()[row].slider.unwrap().text(saved), "8x");
        }
        app.world_mut().resource_mut::<crate::fsr::FsrSupport>().0 = false;
        app.world_mut().resource_mut::<Graphics>().set_fsr(Some(crate::fsr::FsrQuality::NativeAa));
        assert_eq!(app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), row), None, "unsupported FSR leaves native AA editable");
        assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_some());
        assert_eq!(graphics::aa_choice(app.world()), 0);
    }

    #[test]
    fn native_aa_row_is_locked_while_dlss_draws_and_follows_the_effective_upscaler() {
        use crate::upscaler::{DlssState, Session, UpscaleQuality, UpscalerStatus};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Sample4))
            .insert_resource(crate::fsr::FsrSupport(true));
        register_graphics(&mut app);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("anti-aliasing").unwrap();
        let ready = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
        let mut status = UpscalerStatus::new(ready);
        status.update(UpscalerChoice::Dlss(UpscaleQuality::Quality));
        app.insert_resource(status);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(UpscaleQuality::Quality);
        let locked = |app: &App| app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), row);
        assert_eq!(locked(&app), Some("Off (upscaling active)"), "DLSS draws");
        assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_none());
        // Off asked for: native at once (Streamline loaded), the row editable again.
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Off;
        app.world_mut().resource_mut::<UpscalerStatus>().update(UpscalerChoice::Off);
        assert_eq!(locked(&app), None);
        assert_eq!(graphics::aa_choice(app.world()), 2, "the saved 4x is back");
        // DLSS asked for in a session that started without Streamline (the files there now):
        // what drew, native, stays until the restart, so the row stays editable.
        let stock = Session { streamline_loaded: false, dlss: DlssState::NotLoaded, fsr: None };
        let mut status = UpscalerStatus::new(stock);
        status.update(UpscalerChoice::Off);
        status.update(UpscalerChoice::Dlss(UpscaleQuality::Quality));
        assert!(status.restart_pending());
        app.insert_resource(status);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(UpscaleQuality::Quality);
        assert_eq!(locked(&app), None);
    }

    #[test]
    fn upscaling_is_one_named_dropdown_and_round_trips_every_choice() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        let row = settings.index("fsr-upscaling").unwrap();
        assert!(settings.list()[row].slider.is_none());
        assert_eq!(settings.list()[row].label, "Upscaling");
        let dropdown = settings.choices(row).expect("the upscaler must use the native dropdown widget");
        let names = (dropdown.names)(app.world());
        assert_eq!(names.len(), 11);
        // The six FSR names first and unchanged, so old INIs and test aids still work.
        let fsr: Vec<&str> = graphics::FSR_CHOICES.iter().map(|(n, _)| *n).collect();
        assert_eq!(&names[..6], fsr.as_slice());
        for (name, choice) in graphics::UPSCALER_CHOICES {
            assert!((dropdown.pick)(app.world_mut(), name));
            assert_eq!(app.world().resource::<Graphics>().upscaler, choice);
            let saved = (dropdown.save)(app.world());
            assert_eq!(saved, name);
            app.world_mut().resource_mut::<Graphics>().set_fsr(None);
            assert!((dropdown.load)(app.world_mut(), &saved));
            assert_eq!((dropdown.chosen)(app.world()), name);
        }
        for (name, mode) in graphics::FSR_CHOICES {
            assert!((dropdown.pick)(app.world_mut(), name));
            assert_eq!(app.world().resource::<Graphics>().fsr(), mode.quality(), "FSR's names still mean FSR");
        }
        assert!((dropdown.pick)(app.world_mut(), "dlss balanced"), "any case");
        assert_eq!(app.world().resource::<Graphics>().upscaler, UpscalerChoice::Dlss(crate::upscaler::UpscaleQuality::Balanced));
        assert!(!pick_upscaler(app.world_mut(), "unknown"));
        assert!(!load_upscaler(app.world_mut(), "NaN"));
        assert!(!load_upscaler(app.world_mut(), "2"));
        // The early slider build's numbers: a part of the six FSR choices.
        assert!(load_upscaler(app.world_mut(), "0.40"));
        assert_eq!(upscaler_chosen(app.world()), "Quality");
        assert_eq!(parse_upscaler("0"), Some(UpscalerChoice::Off));
        assert_eq!(parse_upscaler("1"), Some(UpscalerChoice::Fsr(crate::upscaler::UpscaleQuality::UltraPerformance)), "never DLSS");
        assert_eq!(parse_upscaler(" DLSS Native AA "), Some(UpscalerChoice::Dlss(crate::upscaler::UpscaleQuality::NativeAa)));
    }

    #[test]
    fn choices_are_shown_by_upscaler_and_quality() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        let row = settings.index("fsr-upscaling").unwrap();
        let label = |name: &str| settings.choice_label(app.world(), row, name);
        assert_eq!(label("Off"), "Off");
        assert_eq!(label("Quality"), "FSR 3.1: Quality");
        assert_eq!(label("Ultra Performance"), "FSR 3.1: Ultra Performance");
        assert_eq!(label("DLSS Native AA"), "DLSS: Native AA");
        assert_eq!(label("DLSS Balanced"), "DLSS: Balanced");
    }

    #[test]
    fn dlss_choices_are_out_of_reach_with_the_reason_but_a_stored_one_loads() {
        use crate::upscaler::{DlssState, Session, Unavailable, UpscaleQuality, UpscalerStatus};
        for why in [Unavailable::FilesMissing, Unavailable::NotWindows, Unavailable::NotVulkan, Unavailable::GpuNotSupported, Unavailable::UpdaterInFolder] {
            let mut app = App::new();
            app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
            register_graphics(&mut app);
            let session = Session { streamline_loaded: false, dlss: DlssState::Unavailable(why), fsr: None };
            app.insert_resource(UpscalerStatus::new(session));
            let row = app.world().resource::<nvf_play::settings::Settings>().index("fsr-upscaling").unwrap();
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            for q in UpscaleQuality::ALL {
                let name = format!("DLSS {}", q.name());
                assert_eq!(settings.choice_unavailable(app.world(), row, &name).as_deref(), Some(why.words()), "{why:?} {name}");
            }
            for (name, _) in graphics::FSR_CHOICES {
                assert_eq!(settings.choice_unavailable(app.world(), row, name), None, "Off and FSR stay in reach");
            }
            assert!(!nvf_play::settings::pick(app.world_mut(), row, "DLSS Quality"), "{why:?}: not taken from the page or a test aid");
            assert_eq!(app.world().resource::<Graphics>().upscaler, UpscalerChoice::Off);
            assert!(nvf_play::settings::pick(app.world_mut(), row, "Balanced"));
            // The INI's text still loads: the request survives the run.
            let load = app.world().resource::<nvf_play::settings::Settings>().choices(row).unwrap().load;
            assert!(load(app.world_mut(), "DLSS Performance"));
            assert_eq!(app.world().resource::<Graphics>().upscaler, UpscalerChoice::Dlss(UpscaleQuality::Performance));
        }
        // Ready, or not loaded in a session that may load it at the next start: in reach.
        for dlss in [DlssState::Ready, DlssState::NotLoaded] {
            let mut app = App::new();
            app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
            register_graphics(&mut app);
            app.insert_resource(UpscalerStatus::new(Session { streamline_loaded: dlss == DlssState::Ready, dlss, fsr: None }));
            let row = app.world().resource::<nvf_play::settings::Settings>().index("fsr-upscaling").unwrap();
            assert_eq!(app.world().resource::<nvf_play::settings::Settings>().choice_unavailable(app.world(), row, "DLSS Quality"), None, "{dlss:?}");
        }
    }

    /// DLSS given up on in a DLSS session (`dlss::session_state` `EvaluateFailed`, read by
    /// `upscaler::session_now`): the DLSS entries are out of reach with the reason, the stored
    /// DLSS request stays the request, and the camera draws natively.
    #[test]
    fn dlss_that_stopped_working_says_why_in_the_dropdown() {
        use crate::upscaler::{DlssState, Session, Unavailable, UpscaleQuality, UpscalerStatus};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let requested = UpscalerChoice::Dlss(UpscaleQuality::Quality);
        app.world_mut().resource_mut::<Graphics>().upscaler = requested;
        let ready = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
        let mut status = UpscalerStatus::new(ready);
        status.update(requested);
        status.session.dlss = DlssState::Unavailable(Unavailable::EvaluateFailed);
        status.update(requested);
        app.insert_resource(status);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("fsr-upscaling").unwrap();
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        for q in UpscaleQuality::ALL {
            let name = format!("DLSS {}", q.name());
            assert_eq!(settings.choice_unavailable(app.world(), row, &name).as_deref(), Some(Unavailable::EvaluateFailed.words()), "{name}");
        }
        for (name, _) in graphics::FSR_CHOICES {
            assert_eq!(settings.choice_unavailable(app.world(), row, name), None, "Off and FSR stay in reach");
        }
        assert_eq!(upscaler_chosen(app.world()), "DLSS Quality", "the request is kept");
        assert_eq!(app.world().resource::<UpscalerStatus>().effective(), UpscalerChoice::Off, "native, not FSR");
    }

    #[test]
    fn dlss_without_streamline_shows_the_restart_and_with_it_every_switch_is_live() {
        use crate::upscaler::{DlssState, Session, UpscaleQuality, UpscalerStatus};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("fsr-upscaling").unwrap();
        let stock = Session { streamline_loaded: false, dlss: DlssState::NotLoaded, fsr: None };
        app.insert_resource(UpscalerStatus::new(stock));
        // What follow_request does each frame, without the process-wide loader state.
        let pick = |app: &mut App, name: &str| {
            assert!(nvf_play::settings::pick(app.world_mut(), row, name), "{name}");
            let requested = app.world().resource::<Graphics>().upscaler;
            app.world_mut().resource_mut::<UpscalerStatus>().update(requested);
            let status = *app.world().resource::<UpscalerStatus>();
            let shown = app.world().resource::<nvf_play::settings::Settings>().choice_label(app.world(), row, name);
            (status.effective(), status.restart_pending(), shown)
        };
        assert_eq!(pick(&mut app, "Quality"), (UpscalerChoice::Fsr(UpscaleQuality::Quality), false, "FSR 3.1: Quality".to_string()), "FSR at once");
        assert_eq!(
            pick(&mut app, "DLSS Balanced"),
            (UpscalerChoice::Fsr(UpscaleQuality::Quality), true, "DLSS: Balanced (from the next start)".to_string()),
            "entering DLSS: saved, FSR keeps drawing"
        );
        assert_eq!(upscaler_chosen(app.world()), "DLSS Balanced", "the request is what the INI keeps");
        assert_eq!(
            app.world().resource::<nvf_play::settings::Settings>().choice_label(app.world(), row, "Quality"),
            "FSR 3.1: Quality",
            "only the requested choice says so"
        );
        assert_eq!(pick(&mut app, "Off"), (UpscalerChoice::Off, false, "Off".to_string()), "off at once");
        assert_eq!(pick(&mut app, "Performance"), (UpscalerChoice::Fsr(UpscaleQuality::Performance), false, "FSR 3.1: Performance".to_string()));

        // Streamline loaded at start: entering and leaving DLSS at once, nothing pending.
        let ready = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
        let mut status = UpscalerStatus::new(ready);
        status.update(app.world().resource::<Graphics>().upscaler);
        app.insert_resource(status);
        assert_eq!(pick(&mut app, "DLSS Ultra Performance"), (UpscalerChoice::Dlss(UpscaleQuality::UltraPerformance), false, "DLSS: Ultra Performance".to_string()));
        assert_eq!(pick(&mut app, "Balanced"), (UpscalerChoice::Fsr(UpscaleQuality::Balanced), false, "FSR 3.1: Balanced".to_string()), "leaving DLSS at once");
        assert_eq!(pick(&mut app, "Off"), (UpscalerChoice::Off, false, "Off".to_string()));
        assert_eq!(pick(&mut app, "DLSS Quality"), (UpscalerChoice::Dlss(UpscaleQuality::Quality), false, "DLSS: Quality".to_string()), "entering DLSS at once");
    }

    /// The start's choice (before the renderer, `main`) and `apply_stored`'s at `PostStartup`
    /// agree for every precedence case.
    #[test]
    fn the_start_and_the_settings_read_the_upscaler_alike() {
        use crate::upscaler::UpscaleQuality;
        use nvf_play::settings::{apply_stored, GivenSwitches, SettingsFile};
        let dir = std::env::temp_dir().join(format!("nvf-upscaler-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        let fsr_q = UpscalerChoice::Fsr(UpscaleQuality::Quality);
        let dlss_b = UpscalerChoice::Dlss(UpscaleQuality::Balanced);
        // The INI's text, the command line, the switches given, whether they win, the result.
        type Case<'a> = (Option<&'a str>, UpscalerChoice, &'a [&'a str], bool, UpscalerChoice);
        let cases: [Case; 10] = [
            (Some("DLSS Balanced"), fsr_q, &["--fsr"], false, dlss_b),
            (Some("DLSS Balanced"), fsr_q, &["--fsr"], true, fsr_q),
            (Some("DLSS Balanced"), fsr_q, &["--upscaler"], true, fsr_q),
            (Some("DLSS Balanced"), fsr_q, &["--upscaler-quality"], true, fsr_q),
            (Some("DLSS Balanced"), UpscalerChoice::Off, &["--msaa"], true, dlss_b),
            (Some("Quality"), dlss_b, &[], false, fsr_q),
            (Some("0.40"), UpscalerChoice::Off, &[], false, fsr_q),
            (Some("2"), dlss_b, &[], false, dlss_b),
            (Some("nonsense"), fsr_q, &[], false, fsr_q),
            (None, dlss_b, &[], false, dlss_b),
        ];
        for (stored, command_line, given, win, want) in cases {
            let text = stored.map_or_else(|| "[Settings]\nbloom=0\n".to_string(), |v| format!("[Settings]\nfsr-upscaling={v}\n"));
            std::fs::write(&path, &text).unwrap();
            let given = GivenSwitches { given: given.iter().map(|g| g.to_string()).collect(), win };
            let early = upscaler_after_ini(command_line, stored_in(&text).as_deref(), Some(&given));
            let mut app = App::new();
            let mut graphics = Graphics::preset(graphics::Preset::Vanilla);
            graphics.upscaler = command_line;
            app.insert_resource(graphics)
                .insert_resource(graphics::MsaaSetting(Msaa::Off))
                .insert_resource(crate::frame_gen::FrameGeneration { on: false })
                .insert_resource(crate::anti_lag::LatencyReduction { on: false, by_frame_generation: false })
                .insert_resource(SettingsFile(Some(path.clone())))
                .insert_resource(given.clone());
            register_graphics(&mut app);
            app.add_systems(PostStartup, apply_stored);
            app.update();
            let late = app.world().resource::<Graphics>().upscaler;
            assert_eq!(early, want, "{stored:?} {command_line:?} {given:?}");
            assert_eq!(late, early, "{stored:?} {command_line:?} {given:?}: the start and the settings disagree");
        }
        // No settings page (not --play): the INI is not applied, the command line decides.
        assert_eq!(upscaler_after_ini(fsr_q, Some("DLSS Balanced"), None), fsr_q);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `[Settings] fsr-upscaling` of an INI's text, as `stored_at_start` reads it.
    fn stored_in(text: &str) -> Option<String> {
        fnv_install::ini_get(text, nvf_play::settings::SECTION, UPSCALING_KEY)
    }
}
