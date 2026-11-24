//! A lost GPU device, or another GPU error the game cannot draw past (owner's 5-player
//! co-op stress test, 2026-11-05: the AMD driver dropped the device and the game then
//! panicked in Bevy on a buffer map).
//!
//! What happens: Bevy's renderer reports the error to [`crate::crash`]'s render error
//! handler, which calls [`lost`] and stops rendering (the vendored bevy_render no longer
//! panics on the buffer maps a lost device fails, engine/vendor/README.md). Then
//! [`after_loss`] asks for an autosave (slot `autosave`, which waits for a moment saving
//! is allowed, as every autosave does; the thumbnail never arrives, so the save goes
//! without one after `nvf_play::save::SHOT_WAIT_FRAMES`), waits for it at most
//! [`SAVE_WAIT`], tells the player in a message box (not in unattended runs) and closes
//! the game with an error exit. No in-place recovery: Bevy 0.20 makes a new device, but
//! the meshes, textures and pipelines the game made on the old one would stay invalid
//! (`RenderErrorPolicy::Recover` warns of strobing); starting again from the autosave is
//! the reliable way back.
//!
//! Log: `render: the GPU device was lost …; autosaving, then the game closes`,
//! `render: GPU lost: autosaved to <path>; the game closes` (or `… not saved: <why>`).
//!
//! Also here: the adapter the renderer chose is logged (`render: adapter <name> …`), with
//! a warning when it is an integrated GPU (the PC may also have a discrete one; Bevy asks
//! for the high-performance adapter). Test aid `NVF_PLAY_TEST_GPU_LOST=<seconds>`: that
//! long after the camera's place is known, the render device is destroyed, which wgpu
//! reports as a lost device, so the whole path above runs.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Set by the test aid: wgpu's `destroy` reports the loss only once the device's queue is
/// empty, which Bevy does not wait for, so the errors that follow count as a lost device.
static SIMULATED: AtomicBool = AtomicBool::new(false);
/// Set once the game closes after a loss (the panic hook, crash.rs).
static CLOSING: AtomicBool = AtomicBool::new(false);

/// Whether the GPU device was lost in this run (reported by wgpu, or the test aid's).
pub fn device_lost() -> bool {
    bevy::render::error_handler::device_was_lost() || SIMULATED.load(Ordering::SeqCst)
}

/// Whether the game is closing after a lost device: a panic now comes from releasing the
/// lost device's objects (wgpu-hal asserts on a swapchain the dead device still holds),
/// after the save is on disk; crash.rs logs it and ends the process without a report.
pub fn closing() -> bool {
    CLOSING.load(Ordering::SeqCst)
}

/// How long the game waits for the autosave before it closes without one.
pub const SAVE_WAIT: Duration = Duration::from_secs(15);

/// The renderer is gone for this run; the game saves and closes.
#[derive(Resource)]
pub struct RenderLost {
    pub why: String,
    at: Instant,
    asked: Option<MessageCursor<nvf_play::save::SaveDone>>,
    saved: Option<Result<PathBuf, String>>,
    closing: bool,
}

/// The renderer stopped for good (`why`): start the autosave and close. Called by the
/// render error handler, every frame while rendering is stopped; only the first call
/// counts.
pub fn lost(main: &mut World, why: String) {
    if main.contains_resource::<RenderLost>() {
        return;
    }
    let why = one_line(&why);
    error!("render: {why}; autosaving, then the game closes{}", crate::crash::place_text_suffix());
    main.insert_resource(RenderLost { why, at: Instant::now(), asked: None, saved: None, closing: false });
}

/// wgpu's error text on one line (it spans several: "Validation Error", "Caused by: …").
pub fn one_line(text: &str) -> String {
    text.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" / ")
}

/// What the player is told when the game closes.
pub fn player_text(why: &str, saved: &Result<PathBuf, String>) -> String {
    let saved = match saved {
        Ok(_) => "Your game was saved to the autosave slot. Start NVForever again and load the autosave to carry on.".to_string(),
        Err(e) => format!("The game could not be saved ({e}); your last save is still there. Start NVForever again and load it to carry on."),
    };
    format!("The graphics card stopped working, so NVForever has to close.\n\n({why})\n\n{saved}")
}

/// Ask for the autosave, wait for it, then tell the player and close (module notes).
pub fn after_loss(world: &mut World) {
    let Some(lost) = world.get_resource::<RenderLost>() else { return };
    if lost.closing {
        return;
    }
    if lost.asked.is_none() && lost.saved.is_none() {
        let can_save = world.contains_resource::<Messages<nvf_play::save::SaveRequest>>() && world.contains_resource::<Messages<nvf_play::save::SaveDone>>();
        if can_save {
            let cursor = world.resource::<Messages<nvf_play::save::SaveDone>>().get_cursor_current();
            world.write_message(nvf_play::save::SaveRequest { name: fnv_save::AUTO_NAME.into(), kind: fnv_save::SaveKind::Auto });
            world.resource_mut::<RenderLost>().asked = Some(cursor);
        } else {
            world.resource_mut::<RenderLost>().saved = Some(Err("no game is running".into()));
        }
    }
    // The autosave's result, once it is on disk.
    if world.resource::<RenderLost>().saved.is_none() {
        let done = world.resource_scope(|world, mut lost: Mut<RenderLost>| {
            let messages = world.resource::<Messages<nvf_play::save::SaveDone>>();
            let cursor = lost.asked.as_mut()?;
            cursor.read(messages).map(|d| d.result.clone().map(|_| d.path.clone())).last()
        });
        let timed_out = world.resource::<RenderLost>().at.elapsed() > SAVE_WAIT;
        match done {
            Some(r) => world.resource_mut::<RenderLost>().saved = Some(r),
            None if timed_out => world.resource_mut::<RenderLost>().saved = Some(Err(format!("saving was not possible within {} s", SAVE_WAIT.as_secs()))),
            None => return,
        }
    }
    let mut lost = world.resource_mut::<RenderLost>();
    lost.closing = true;
    let saved = lost.saved.clone().unwrap_or_else(|| Err("not asked".into()));
    let why = lost.why.clone();
    match &saved {
        Ok(p) => error!("render: GPU lost: autosaved to {}; the game closes", p.display()),
        Err(e) => error!("render: GPU lost: not saved: {e}; the game closes"),
    }
    crate::crash::tell_player(&player_text(&why, &saved));
    CLOSING.store(true, Ordering::SeqCst);
    world.write_message(AppExit::error());
}

/// Log the adapter the renderer uses, again whenever it changes.
pub fn log_adapter(info: Option<Res<RenderAdapterInfo>>) {
    let Some(info) = info else { return };
    if !info.is_changed() {
        return;
    }
    info!("render: adapter {} ({:?}, {:?}, {} {})", info.name, info.device_type, info.backend, info.driver, info.driver_info);
    if info.device_type == wgpu::DeviceType::IntegratedGpu {
        warn!("render: the renderer is on an integrated GPU; if the PC has a discrete one, choose it for nvforever.exe in Windows' graphics settings (or set WGPU_ADAPTER_NAME)");
    }
}

/// Test aid `NVF_PLAY_TEST_GPU_LOST=<seconds>` (module notes).
pub fn test_lose_device(device: Option<Res<RenderDevice>>, time: Res<Time>, mut since: Local<Option<f32>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(secs) = std::env::var("NVF_PLAY_TEST_GPU_LOST").ok().and_then(|v| v.trim().parse::<f32>().ok()) else {
        *done = true;
        return;
    };
    let Some(device) = device else { return };
    if crate::crash::place_text().is_empty() {
        return;
    }
    let start = *since.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - start < secs.max(0.0) {
        return;
    }
    *done = true;
    warn!("render: test aid NVF_PLAY_TEST_GPU_LOST: destroying the render device");
    SIMULATED.store(true, Ordering::SeqCst);
    device.wgpu_device().destroy();
}

pub struct GpuLostPlugin;

impl Plugin for GpuLostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (log_adapter, test_lose_device)).add_systems(Last, after_loss.before(crate::crash::report_exit));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wgpu_error_text_goes_on_one_line() {
        assert_eq!(one_line("Validation Error\n\nCaused by:\n  Buffer is invalid\n"),"Validation Error / Caused by: / Buffer is invalid");
    }

    #[test]
    fn the_player_is_told_whether_the_game_was_saved() {
        let ok = player_text("lost", &Ok(PathBuf::from("autosave.nvfsave")));
        assert!(ok.contains("saved to the autosave slot") && ok.contains("(lost)"));
        let no = player_text("lost", &Err("in a conversation".into()));
        assert!(no.contains("could not be saved (in a conversation)") && no.contains("last save is still there"));
    }

    #[test]
    fn a_loss_asks_for_one_autosave_then_closes_when_it_is_written() {
        let mut app = App::new();
        app.add_message::<nvf_play::save::SaveRequest>().add_message::<nvf_play::save::SaveDone>().add_message::<AppExit>().add_systems(Update, after_loss);
        lost(app.world_mut(), "test".into());
        lost(app.world_mut(), "second".into());
        app.update();
        let asked: Vec<_> = app.world_mut().resource_mut::<Messages<nvf_play::save::SaveRequest>>().drain().collect();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].kind, fnv_save::SaveKind::Auto);
        assert_eq!(app.world().resource::<RenderLost>().why, "test");
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());
        app.world_mut().write_message(nvf_play::save::SaveDone { path: "autosave.nvfsave".into(), result: Ok(()) });
        app.update();
        assert!(app.world().resource::<RenderLost>().closing);
        assert!(app.world().resource::<Messages<nvf_play::save::SaveRequest>>().is_empty());
        assert_eq!(app.world_mut().resource_mut::<Messages<AppExit>>().drain().count(), 1);
    }

    #[test]
    fn without_a_game_the_loss_closes_at_once() {
        let mut app = App::new();
        app.add_message::<AppExit>().add_systems(Update, after_loss);
        lost(app.world_mut(), "test".into());
        app.update();
        assert!(app.world().resource::<RenderLost>().closing);
        assert_eq!(app.world_mut().resource_mut::<Messages<AppExit>>().drain().count(), 1);
    }
}
