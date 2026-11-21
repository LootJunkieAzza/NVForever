//! Light and noise gauges while sneaking (owner request 2026-10-24, after the stealth
//! games that show how lit and how loud the player is) [design: not the original's; the
//! original shows only the sneak word]. Off by default; the settings page's "Gameplay"
//! switch `sneak-gauges` turns them on, kept in the INI like every switch.
//!
//! Two segmented bars under the sneak word, top middle, while the player sneaks and the
//! HUD is up:
//!
//! - **Light**: the light level at the player, 0–100, as detection measures it
//!   (nvforever `detection.rs`: ambient, the sun unless shaded, the lights that reach
//!   the player, the Pip-Boy light), before fSneakLightMult.
//! - **Noise**: the sound the player makes as detection counts it before the distance
//!   falloff (`fnv_game::detection::Settings::noise`: footsteps by body armour weight,
//!   × 1.5 running unless Silent Running, shots by their sound level), shown against
//!   [`NOISE_FULL`]. A shot fills it at once and it falls back over [`NOISE_FALL`]
//!   seconds, so a moment's sound can be seen.
//!
//! - **Cover** (#2688, [`crate::sound_cover`]): the loudest sound of the world heard at
//!   the player, in the noise's units. The noise bar's segment at that level wears a
//!   brighter, larger square ([`COVER_OUTLINE`]): noise up to it is not heard. It jumps up
//!   and falls back as the noise does; no square without cover (or with the
//!   `sound-cover` switch off).
//!
//! The values are written by the detection pass (four times a second); the bars ease
//! towards them. They show what the player gives off, not whether anyone notices: the
//! sneak word says that.

use crate::hud::{HudSet, HudView};
use crate::interface::{amber, u};
use crate::settings::{add_setting, Setting};
use crate::PlaySet;
use bevy::prelude::*;

/// The noise that fills the bar: a walk in light armour is about a third, a run
/// about a half, a Normal shot (50 × 2 × 1.6 = 160) over full [design].
pub const NOISE_FULL: f32 = 100.0;
/// Seconds the noise bar takes to fall from full to empty [design].
pub const NOISE_FALL: f32 = 1.5;
/// The cover mark: its line's width and how far it stands out of the segment, in
/// interface units rounded to whole pixels ([`SneakGauges::mark_pixels`]), so the square is
/// even on every side; together under [`GAP`] at any height, so it never reaches the
/// next segment [design; #2688 owner: the first square, 2 + 2 units in a 3-unit gap,
/// overlapped its neighbour and was uneven].
const COVER_OUTLINE: f32 = 2.0;
const COVER_OFFSET: f32 = 1.0;
/// The gap between segments, in interface units [design].
const GAP: f32 = 5.0;
/// Seconds the light bar takes to follow a change across the whole bar [design].
const LIGHT_FOLLOW: f32 = 0.5;
/// Segments of each bar [design: as the stealth games' bars].
const SEGMENTS: usize = 10;
/// The piece: width, a bar's height, the row's height; its top, under the sneak word.
const WIDTH: f32 = 220.0;
const BAR: f32 = 10.0;
const ROW: f32 = 22.0;
const TOP: f32 = 64.0;
const LABEL: f32 = 60.0;

/// What the gauges show, written by the detection pass.
#[derive(Resource, Debug, Default, Clone)]
pub struct SneakGauges {
    /// The setting.
    pub on: bool,
    /// The light level at the player, 0–100.
    pub light: f32,
    /// The player's noise, in the sound term's units (0 standing still).
    pub noise: f32,
    /// The sound cover at the player, in the same units (0: none; [`crate::sound_cover`]).
    pub cover: f32,
    /// What the bars show now, 0–1 each, and where the cover mark stands.
    shown: [f32; 2],
    cover_shown: f32,
    /// Seconds shown, of them with the cover mark, and the most light and noise seen, for
    /// the exit line.
    secs: f32,
    cover_secs: f32,
    most: [f32; 2],
}

impl SneakGauges {
    /// Where the bars go next from `shown` towards the values, `dt` seconds on: light
    /// eases both ways; noise jumps up and falls at [`NOISE_FALL`].
    pub fn step(shown: [f32; 2], light: f32, noise: f32, dt: f32) -> [f32; 2] {
        let l = (light / 100.0).clamp(0.0, 1.0);
        let n = (noise / NOISE_FULL).clamp(0.0, 1.0);
        let follow = dt / LIGHT_FOLLOW;
        let light = shown[0] + (l - shown[0]).clamp(-follow, follow);
        let noise = if n >= shown[1] { n } else { (shown[1] - dt / NOISE_FALL).max(n) };
        [light, noise]
    }

    /// Where the cover mark goes next, as the noise bar does: up at once, down over
    /// [`NOISE_FALL`].
    pub fn step_cover(shown: f32, cover: f32, dt: f32) -> f32 {
        let c = (cover / NOISE_FULL).clamp(0.0, 1.0);
        if c >= shown { c } else { (shown - dt / NOISE_FALL).max(c) }
    }

    /// The noise bar's segment the cover mark is on: the last one the bar lights at that
    /// level (the noise up to it is covered); `None` below the first.
    pub fn cover_segment(shown: f32) -> Option<usize> {
        (0..SEGMENTS).rev().find(|&k| (k as f32 + 0.5) / SEGMENTS as f32 <= shown)
    }

    /// The cover mark's line width and offset in whole physical pixels for a window
    /// `height` physical pixels high at UI scale `scale`: the line at least one pixel,
    /// and the two together leaving at least a pixel of the gap clear (on very small
    /// windows the offset goes first).
    pub fn mark_pixels(height: f32, scale: f32) -> (f32, f32) {
        let unit = height * scale / 960.0;
        let room = ((GAP * unit) - 1.0).floor().max(1.0);
        let width = (COVER_OUTLINE * unit).round().clamp(1.0, room);
        let offset = (COVER_OFFSET * unit).round().max(1.0).min(room - width);
        (width, offset)
    }
}

pub struct SneakGaugesPlugin;

impl Plugin for SneakGaugesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SneakGauges>()
            .add_systems(Update, (spawn, draw).chain().after(HudSet).in_set(PlaySet::Ui))
            .add_systems(Last, report);
        add_setting(
            app,
            Setting {
                key: "sneak-gauges",
                group: crate::pause::GROUP_MENUS,
                label: "Light and noise gauges while sneaking",
                note: "On: two bars under the sneak word show how lit you are and how much noise you make, as detection counts them. Not in the original.",
                switches: "",
                get: |w| w.resource::<SneakGauges>().on,
                set: |w, on| w.resource_mut::<SneakGauges>().on = on,
                slider: None,
            },
        );
    }
}

#[derive(Component)]
struct GaugeRoot;

/// Segment `k` of bar `bar` (0 light, 1 noise).
#[derive(Component)]
struct Segment {
    bar: usize,
    k: usize,
}

fn spawn(mut commands: Commands, root: Query<(), With<GaugeRoot>>) {
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    if !root.is_empty() {
        return;
    }
    let node = Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        margin: UiRect::left(u(-WIDTH / 2.0)),
        top: u(TOP),
        width: u(WIDTH),
        height: u(ROW * 2.0),
        flex_direction: FlexDirection::Column,
        ..default()
    };
    commands.spawn((GaugeRoot, node, Visibility::Hidden, GlobalZIndex(5))).with_children(|p| {
        for (bar, name) in ["LIGHT", "NOISE"].into_iter().enumerate() {
            p.spawn(Node { height: u(ROW), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                row.spawn((Node { width: u(LABEL), ..default() }, Text::new(name), TextFont { font_size: crate::interface::font_units(13.0), ..default() }, TextColor(amber(255.0))));
                row.spawn(Node { width: u(WIDTH - LABEL), height: u(BAR), column_gap: u(GAP), ..default() }).with_children(|segs| {
                    for k in 0..SEGMENTS {
                        segs.spawn((
                            Segment { bar, k },
                            Node { flex_grow: 1.0, height: Val::Percent(100.0), ..default() },
                            BackgroundColor(amber(60.0)),
                            Outline { width: Val::Px(1.0), offset: Val::Px(1.0), color: Color::NONE },
                        ));
                    }
                });
            });
        }
    });
}

fn draw(
    time: Res<Time>,
    view: Res<HudView>,
    mut gauges: ResMut<SneakGauges>,
    mut root: Query<&mut Visibility, With<GaugeRoot>>,
    mut segments: Query<(&Segment, &mut BackgroundColor, &mut Outline)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let show = gauges.on && view.shown && view.sneak.is_some();
    let Ok(mut vis) = root.single_mut() else { return };
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
        if show {
            info!("sneak gauges shown: light {:.0}, noise {:.0}", gauges.light, gauges.noise);
        }
    }
    if !show {
        return;
    }
    let dt = time.delta_secs();
    let g = &mut *gauges;
    g.shown = SneakGauges::step(g.shown, g.light, g.noise, dt);
    let was = SneakGauges::cover_segment(g.cover_shown);
    g.cover_shown = SneakGauges::step_cover(g.cover_shown, g.cover, dt);
    let mark = SneakGauges::cover_segment(g.cover_shown);
    if mark != was {
        debug!("sneak gauges: cover mark {} (cover {:.0}, noise {:.0})", mark.map_or("off".to_string(), |k| format!("on segment {}", k + 1)), g.cover, g.noise);
    }
    if mark.is_some() {
        g.cover_secs += dt;
    }
    g.secs += dt;
    g.most = [g.most[0].max(g.light), g.most[1].max(g.noise)];
    // Val::Px is in logical pixels: whole physical pixels divided by the scale factor.
    let (width, offset) = windows.single().map_or((Val::Px(2.0), Val::Px(1.0)), |w| {
        let (wd, off) = SneakGauges::mark_pixels(w.physical_height() as f32, crate::interface::unit_scale());
        let f = w.scale_factor();
        (Val::Px(wd / f), Val::Px(off / f))
    });
    for (s, mut c, mut o) in &mut segments {
        let lit = (s.k as f32 + 0.5) / SEGMENTS as f32 <= g.shown[s.bar];
        c.0 = amber(if lit { 255.0 } else { 60.0 });
        let marked = s.bar == 1 && mark == Some(s.k);
        let want = if marked { cover_colour() } else { Color::NONE };
        if o.color != want || (marked && (o.width != width || o.offset != offset)) {
            o.color = want;
            o.width = width;
            o.offset = offset;
        }
    }
}

/// The cover mark's colour: the HUD colour at full, half way to white, so it stands out
/// from a lit segment in any HUD colour [design].
fn cover_colour() -> Color {
    let c = amber(255.0).to_srgba();
    Color::srgb((c.red + 1.0) / 2.0, (c.green + 1.0) / 2.0, (c.blue + 1.0) / 2.0)
}

fn report(mut exit: MessageReader<AppExit>, gauges: Res<SneakGauges>) {
    if exit.read().next().is_none() {
        return;
    }
    info!(
        "sneak gauges summary: on {}, shown {:.1} s, most light {:.0}, most noise {:.0}, cover mark {:.1} s",
        gauges.on, gauges.secs, gauges.most[0], gauges.most[1], gauges.cover_secs
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_jumps_up_and_falls_slowly_light_eases() {
        let up = SneakGauges::step([0.0, 0.0], 100.0, 160.0, 0.1);
        assert!((up[0] - 0.2).abs() < 1e-5, "light moves 0.1 / 0.5 of the bar");
        assert_eq!(up[1], 1.0, "a shot fills the noise bar at once");
        let down = SneakGauges::step([1.0, 1.0], 100.0, 0.0, 0.15);
        assert!((down[1] - 0.9).abs() < 1e-5, "and it falls a tenth in 0.15 s");
        assert_eq!(down[0], 1.0);
        let walk = SneakGauges::step([0.0, 0.0], 0.0, 31.2, 0.1);
        assert!((walk[1] - 0.312).abs() < 1e-5);
    }

    #[test]
    fn the_cover_mark_stands_on_the_last_segment_the_cover_reaches() {
        assert_eq!(SneakGauges::cover_segment(0.0), None);
        assert_eq!(SneakGauges::cover_segment(0.04), None);
        assert_eq!(SneakGauges::cover_segment(0.05), Some(0));
        assert_eq!(SneakGauges::cover_segment(0.31), Some(2), "a 31 cover marks the third segment, the last a walk's 31 lights");
        assert_eq!(SneakGauges::cover_segment(0.35), Some(3));
        assert_eq!(SneakGauges::cover_segment(1.0), Some(SEGMENTS - 1));
        let up = SneakGauges::step_cover(0.0, 40.0, 0.1);
        assert!((up - 0.4).abs() < 1e-5, "up at once");
        let down = SneakGauges::step_cover(0.4, 0.0, 0.15);
        assert!((down - 0.3).abs() < 1e-5, "down a tenth in 0.15 s");
        assert_eq!(SneakGauges::step_cover(0.5, 200.0, 0.1), 1.0);
    }

    #[test]
    fn the_cover_mark_is_whole_pixels_and_stays_inside_the_gap() {
        assert_eq!(SneakGauges::mark_pixels(900.0, 1.0), (2.0, 1.0));
        assert_eq!(SneakGauges::mark_pixels(2160.0, 1.0), (5.0, 2.0));
        for h in [480.0, 600.0, 720.0, 900.0, 1080.0, 1440.0, 2160.0, 4320.0] {
            for scale in [0.5, 0.75, 1.0, 1.25, 1.5] {
                let (w, o) = SneakGauges::mark_pixels(h, scale);
                let gap = GAP * h * scale / 960.0;
                assert!(w >= 1.0 && o >= 0.0 && w.fract() == 0.0 && o.fract() == 0.0);
                if gap >= 2.0 {
                    assert!(w + o <= gap - 1.0, "{h} px at {scale}: mark {w}+{o} px in a {gap:.1} px gap");
                }
            }
        }
    }
}
