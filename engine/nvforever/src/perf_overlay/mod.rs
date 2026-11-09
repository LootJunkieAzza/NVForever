//! The performance overlay (issue #1868, grown from the frame rate counter of #1087 and
//! #1704): a small panel at the top right of the screen with a row per reading, each row a
//! switch of the settings page (`settings.rs`, kept in `NVForever.ini`):
//!
//! | Row  | Reads | From |
//! |------|-------|------|
//! | FPS  | frames a second and the mean frame time of the last half second, the 1 % low of the last 10 s | the time between frames ([`readings::Frames`], `stress_sample::frame_stats`) |
//! | CPU  | the machine's use, and the game's own as cores kept busy | Windows' performance counters; the process's processor time ([`usage`]) |
//! | GPU  | the busiest engine's use | Windows' performance counters ([`usage`]) |
//! | VRAM | this process's GPU memory against the budget | `memory_budget.rs`'s check every 2 s (`MemoryBudget::vram_use_mb`) |
//! | RAM  | the game's working set, the machine's RAM in use | the process's memory counters ([`usage`]); `memory_budget.rs`'s check |
//!
//! The settings page's Frame rate counter switch and `--fps` show it (off by default);
//! the rows are its own
//! switches. A row with nothing to read (no GPU budget on an integrated GPU; off Windows
//! the counters' rows) is left out, and nothing is read while the overlay is hidden. Not
//! in the original [engineering choice, issue #1087].
//!
//! The times are wall-clock (`Time<Real>`), so a paused game or a slowed clock still
//! reads the frame rate the player sees. The text changes twice a second so it can be
//! read ([`readings::WINDOW`]).
//!
//! The log says the first readings of the first showing (`fps counter: first reading …`,
//! `first usage reading …`, `first process reading …`) and each turn on or off
//! (`fps counter: on`), as the smoke scenarios read them.
//!
//! - [`readings`]: the numbers and the rows' text (pure, tested).
//! - [`usage`]: the sampler thread for what Windows gives.
//! - [`draw`]: the panel.
//!
//! A later row (the frame graph, the frame split, the loaded counts) is a switch in
//! [`readings::Rows`], a row of [`readings::rows`] and, if it draws other than a line of
//! text, a part of [`draw`].

mod draw;
mod readings;
mod usage;

use std::sync::Arc;

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

use crate::memory_budget::MemoryBudget;
pub use readings::Rows;
use readings::{Frames, Readings};

/// Whether the overlay is shown, and its rows.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FpsCounter {
    pub show: bool,
    pub rows: Rows,
}

/// A showing's readings, the sampler, what was logged, and whether the panel is to be
/// set again.
#[derive(Resource, Default)]
struct Overlay {
    frames: Frames,
    readings: Readings,
    /// The sampler, started the first time the overlay shows.
    usage: Option<Arc<usage::Shared>>,
    logged: bool,
    usage_logged: bool,
    process_logged: bool,
    dirty: bool,
    /// The switches changed: the panel's width starts again from its rows.
    fresh: bool,
}

pub struct PerfOverlayPlugin(pub bool);

impl Plugin for PerfOverlayPlugin {
    fn build(&self, app: &mut App) {
        // In Update: the panel is drawn at the UI scale, fixed once the INI is read
        // (`nvf_play::interface::fix_ui_scale`, PostStartup).
        app.insert_resource(FpsCounter { show: self.0, ..default() }).init_resource::<Overlay>().add_systems(Update, (switch, sample, draw).chain());
    }
}

/// A change of the switches: a new showing starts from the waiting text, not the last
/// showing's reading; a hidden overlay stops the sampler.
fn switch(counter: Res<FpsCounter>, mut overlay: ResMut<Overlay>, mut was: Local<Option<bool>>) {
    if !counter.is_changed() {
        return;
    }
    if was.is_some_and(|w| w != counter.show) {
        info!("fps counter: {}", if counter.show { "on" } else { "off" });
    }
    if *was != Some(counter.show) {
        overlay.frames.clear();
        overlay.readings = Readings::default();
        if let Some(u) = &overlay.usage {
            u.set_on(counter.show);
        }
    }
    *was = Some(counter.show);
    overlay.dirty = true;
    overlay.fresh = true;
}

fn sample(counter: Res<FpsCounter>, time: Res<Time<Real>>, budget: Option<Res<MemoryBudget>>, mut overlay: ResMut<Overlay>) {
    if !counter.show {
        return;
    }
    let rows = counter.rows;
    let u = overlay.usage.get_or_insert_with(usage::start).clone();
    u.set_on(true);
    u.set_want(rows.cpu, rows.gpu, rows.cpu || rows.ram);
    overlay.frames.push(time.delta_secs());
    let Some(frame) = overlay.frames.take() else { return };
    let got = u.read();
    let vram_mb = budget.as_ref().and_then(|b| b.vram_use_mb);
    let ram_share = budget.as_ref().and_then(|b| b.ram_share);
    let vram_share = vram_mb.map(|(used, b)| used as f64 / b.max(1) as f64);
    let (vram_warn, ram_warn) = readings::pressed_rows(budget.as_ref().is_some_and(|b| b.pressed), vram_share, ram_share);
    let o = &mut *overlay;
    o.readings = Readings { frame: Some(frame), cpu: got.cpu, game_cores: got.cores, gpu: got.gpu, vram_mb, game_ram_mb: got.ram_mb, ram_share, vram_warn, ram_warn };
    o.dirty = true;
    if !o.logged {
        o.logged = true;
        info!("fps counter: first reading {:.0} fps, {:.1} ms, slowest {:.1} ms, 1% low {:.1} ms", frame.fps, frame.mean_ms, frame.worst_ms, frame.low_ms);
    }
    // Once both are read: the GPU's can come a sample after the CPU's (its counter array
    // grows between PDH's size and fill calls) or never (no GPU counters; the counters
    // line says which were found), and a line without it said once would never say it.
    if !o.usage_logged && got.cpu.is_some() && got.gpu.is_some() {
        o.usage_logged = true;
        info!("fps counter: first usage reading{}", usage_text(got.cpu, got.gpu));
    }
    if let (false, Some(cores), Some(ram_mb)) = (o.process_logged, got.cores, got.ram_mb) {
        o.process_logged = true;
        info!("fps counter: first process reading: the game keeps {cores:.2} cores busy, working set {ram_mb} MB");
    }
}

/// `  CPU 34%  GPU 71%`, each part only when it was read (the log's wording since #1704).
fn usage_text(cpu: Option<f32>, gpu: Option<f32>) -> String {
    let mut s = String::new();
    if let Some(c) = cpu {
        s += &format!("  CPU {c:.0}%");
    }
    if let Some(g) = gpu {
        s += &format!("  GPU {g:.0}%");
    }
    s
}

/// What the panel was last set with: its kit, its width this showing, the HUD colour.
#[derive(Default)]
struct Drawn {
    kit: Option<draw::Kit>,
    width: f32,
    rgb: [u8; 3],
}

/// Set the panel again when the readings or the switches changed, or the HUD colour did
/// (the settings page's colour rows: `nvf_play::ui_colour`).
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    counter: Res<FpsCounter>,
    mut overlay: ResMut<Overlay>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    panels: Query<Entity, With<draw::PanelRoot>>,
    mut drawn: Local<Drawn>,
) {
    let rgb = nvf_play::interface::hud_rgb();
    if !overlay.dirty && (!counter.show || rgb == drawn.rgb) {
        return;
    }
    overlay.dirty = false;
    drawn.rgb = rgb;
    if std::mem::take(&mut overlay.fresh) {
        drawn.width = 0.0;
    }
    for e in &panels {
        commands.entity(e).despawn();
    }
    if !counter.show {
        return;
    }
    // Font 7 once there is game data to read it from; Bevy's font until then.
    if drawn.kit.as_ref().is_none_or(|k| !k.had_data && data.is_some()) {
        drawn.kit = Some(draw::Kit::load(data.as_deref(), &mut images, formats.as_deref()));
    }
    let rows = readings::rows(&overlay.readings, &counter.rows);
    let Drawn { kit, width, .. } = &mut *drawn;
    if let Some(kit) = kit {
        draw::spawn(&mut commands, kit, &rows, width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_text_shows_what_was_read() {
        assert_eq!(usage_text(None, None), "");
        assert_eq!(usage_text(Some(34.4), Some(70.6)), "  CPU 34%  GPU 71%");
        assert_eq!(usage_text(None, Some(5.0)), "  GPU 5%");
    }
}
