//! HDR calibration (docs/design/hdr-output.md "Calibration"): the HDR row's Calibrate...
//! opens two full-screen pictures over the settings page, as Download DLSS...'s dialog does
//! (`nvf_play::pause::FormOver`: the page under it takes no key or click), as most games
//! calibrate HDR [design: PR #3348 review]:
//!
//! 1. **Peak brightness**: a white field at the peak being set, with the NVForever logo on it
//!    in white at 10,000 nits. Raise the peak until the logo just disappears: the field is
//!    then as bright as the screen goes, and light brighter than that is never asked for.
//! 2. **Brightness**: a black field with the logo in dark grey (3 % of paper white). Raise
//!    the brightness until the logo can just be seen. It sets paper white, how bright the
//!    picture's white and the HUD look.
//!
//! Left and Right (the D-pad) move the value, Enter goes on (Done on the second), Backspace
//! goes back, Esc puts both values back as they were; the buttons do the same with a click.
//! The pictures are drawn by the interface in scRGB, which the window's blit scales by the
//! HUD's white (vendored bevy_core_pipeline `blit`): a value of v shows at v x paper white.
//! Without an HDR surface the row says so in the corner and nothing opens.
//!
//! Test aid `NVF_PLAY_TEST_HDR_CALIBRATE=<seconds>:<step>` (step 1 or 2): opens that picture at
//! that time with or without an HDR surface (for screenshots of the layout).

use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use nvf_assets::GameData;
use nvf_play::interface::{amber, bc_supported, set_line, u, Face};
use nvf_play::pause::FormOver;
use nvf_play::start_menu::Faces;
use nvf_play::ui_pointer::{Button as UiButton, PointerState};
use std::sync::atomic::Ordering::Relaxed;

/// Asked for by the HDR row's Calibrate... (hdr.rs).
#[derive(Resource)]
pub struct Asked;

/// The logo's brightness on the peak picture, nits: above any screen's peak [design].
const LOGO_PEAK_NITS: f32 = 10_000.0;
/// The logo's brightness on the black picture, a part of paper white [design].
const LOGO_DARK: f32 = 0.03;
/// One press's step, nits: the peak and the brightness [design].
const PEAK_STEP: u32 = 50;
const PAPER_STEP: u32 = 10;
/// The logo's box, interface units (the view is 960 high) [design].
const LOGO_WIDTH: f32 = 560.0;
const LOGO_HEIGHT: f32 = 180.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Step {
    #[default]
    Closed,
    Peak,
    Paper,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Down,
    Up,
    Back,
    Next,
    Cancel,
}

#[derive(Component)]
struct CalibrationRoot;

#[derive(Resource, Default)]
struct Calibration {
    step: Step,
    faces: Option<Faces>,
    /// Peak and paper white when it opened: Esc puts them back.
    before: (u32, u32),
    /// The peak being set (the screen's own until changed).
    peak: u32,
    dirty: bool,
    /// Closed this frame: [`FormOver`] is cleared next frame, so the key that closed it does
    /// nothing in the menu under it.
    closing: bool,
}

impl Calibration {
    fn act(&mut self, act: Act) {
        match (self.step, act) {
            (Step::Closed, _) => return,
            (Step::Peak, Act::Down) => self.peak = self.peak.saturating_sub(PEAK_STEP).max(crate::hdr::PEAK_MIN),
            (Step::Peak, Act::Up) => self.peak = (self.peak + PEAK_STEP).min(crate::hdr::PEAK_MAX),
            (Step::Paper, Act::Down) => crate::hdr::set_paper_nits(crate::hdr::paper_nits().saturating_sub(PAPER_STEP).max(crate::hdr::PAPER_MIN)),
            (Step::Paper, Act::Up) => crate::hdr::set_paper_nits(crate::hdr::paper_nits() + PAPER_STEP),
            (Step::Peak, Act::Next) => self.step = Step::Paper,
            (Step::Paper, Act::Back) => self.step = Step::Peak,
            (Step::Peak, Act::Back) => {}
            (Step::Paper, Act::Next) => {
                info!("hdr: calibrated: peak {} nits, brightness (paper white) {} nits", self.peak, crate::hdr::paper_nits());
                self.close();
            }
            (_, Act::Cancel) => {
                crate::hdr::set_peak_nits(self.before.0);
                crate::hdr::set_paper_nits(self.before.1);
                info!("hdr: calibration cancelled, values put back");
                self.close();
                return;
            }
        }
        if self.step == Step::Peak || matches!(act, Act::Next) {
            crate::hdr::set_peak_nits(self.peak);
        }
        self.dirty = true;
    }

    fn close(&mut self) {
        self.step = Step::Closed;
        self.closing = true;
        self.dirty = true;
    }
}

pub fn add(app: &mut App) {
    app.init_resource::<Calibration>()
        .add_systems(PreUpdate, clear_form_over.run_if(resource_exists::<FormOver>))
        .add_systems(Update, (test_aid, open, keys, clicks, draw).chain().run_if(resource_exists::<FormOver>));
}

fn clear_form_over(mut c: ResMut<Calibration>, mut over: ResMut<FormOver>) {
    if c.closing {
        c.closing = false;
        over.set_if_neq(FormOver(false));
    }
}

/// The test aid's plan: when and which step, read once.
fn test_aid(mut commands: Commands, real: Res<Time<Real>>, mut plan: Local<Option<Option<(f64, Step)>>>, mut c: ResMut<Calibration>) {
    let plan = plan.get_or_insert_with(|| {
        let v = std::env::var("NVF_PLAY_TEST_HDR_CALIBRATE").ok()?;
        let (at, step) = v.split_once(':')?;
        Some((at.trim().parse().ok()?, if step.trim() == "2" { Step::Paper } else { Step::Peak }))
    });
    if let Some((at, step)) = *plan {
        if real.elapsed_secs_f64() >= at {
            *plan = None;
            commands.insert_resource(Asked);
            c.step = Step::Closed;
            c.closing = false;
            // Opened below with the screen check skipped, then moved to the asked step.
            c.dirty = true;
            commands.insert_resource(AidStep(step));
        }
    }
}

/// The test aid's step, taken by [`open`].
#[derive(Resource)]
struct AidStep(Step);

/// Calibrate... was picked: the peak picture opens, if the window shows HDR.
#[allow(clippy::too_many_arguments)]
fn open(
    mut commands: Commands,
    asked: Option<Res<Asked>>,
    aid: Option<Res<AidStep>>,
    mut c: ResMut<Calibration>,
    mut over: ResMut<FormOver>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    messages: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
) {
    if asked.is_none() {
        return;
    }
    commands.remove_resource::<Asked>();
    commands.remove_resource::<AidStep>();
    if c.step != Step::Closed {
        return;
    }
    let hdr = bevy::render::view::window::NVF_HDR_SURFACE_ACTIVE.load(Relaxed);
    if !hdr && aid.is_none() {
        let text = "HDR calibration needs HDR output on, on a screen that shows HDR (Windows HDR on)".to_string();
        info!("hdr: {text}");
        if let Some(mut m) = messages {
            m.push(nvf_play::script_events::Note { text, icon: None, seconds: Some(4.0) });
        }
        return;
    }
    if c.faces.is_none() {
        c.faces = Some(Faces::load(data.as_deref(), &mut images, bc_supported(formats.as_deref())));
    }
    c.before = (crate::hdr::peak_setting(), crate::hdr::paper_nits());
    c.peak = crate::hdr::peak_nits();
    c.step = aid.map_or(Step::Peak, |a| a.0);
    c.dirty = true;
    info!("hdr: calibration opened (peak {} nits, brightness {} nits)", c.peak, crate::hdr::paper_nits());
    over.set_if_neq(FormOver(true));
}

/// The keys pressed this frame: typed (repeats too), and pressed in the key state without
/// being typed (a controller's D-pad and buttons, `nvf_play::pad::menu_keys`), counted once.
fn presses(typed: &[KeyboardInput], held: &ButtonInput<KeyCode>) -> Vec<Act> {
    let mut out: Vec<Act> = typed
        .iter()
        .filter(|k| k.state.is_pressed())
        .filter_map(|k| match k.logical_key {
            Key::ArrowLeft => Some(Act::Down),
            Key::ArrowRight => Some(Act::Up),
            Key::Enter => Some(Act::Next),
            Key::Backspace => Some(Act::Back),
            Key::Escape => Some(Act::Cancel),
            _ => None,
        })
        .collect();
    for (code, act) in [(KeyCode::ArrowLeft, Act::Down), (KeyCode::ArrowRight, Act::Up), (KeyCode::Enter, Act::Next), (KeyCode::Backspace, Act::Back), (KeyCode::Escape, Act::Cancel)] {
        let typed_now = typed.iter().any(|k| k.state.is_pressed() && !k.repeat && k.key_code == code);
        if held.just_pressed(code) && !typed_now {
            out.push(act);
        }
    }
    out
}

fn keys(mut typed: MessageReader<KeyboardInput>, held: Res<ButtonInput<KeyCode>>, mut c: ResMut<Calibration>) {
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();
    if c.step == Step::Closed {
        return;
    }
    for act in presses(&events, &held) {
        c.act(act);
        if c.step == Step::Closed {
            return;
        }
    }
}

fn clicks(pressed: Query<(&PointerState, &Act), Changed<PointerState>>, mut c: ResMut<Calibration>) {
    if let Some(act) = pressed.iter().find(|(s, _)| **s == PointerState::Pressed).map(|(_, a)| *a) {
        c.act(act);
    }
}

/// A colour of `nits` on the window, as the interface draws it (scRGB scaled by the HUD's
/// white, which is paper white).
fn at_nits(nits: f32) -> Color {
    let v = nits / crate::hdr::paper_nits().max(1) as f32;
    Color::LinearRgba(LinearRgba::rgb(v, v, v))
}

fn text(p: &mut ChildSpawnerCommands, face: &Face, words: &str, color: Color) {
    let line = face.font.lines(words, None).into_iter().next().unwrap_or_default();
    let h = face.font.size;
    p.spawn(Node { width: u(line.width), height: u(h), flex_shrink: 0.0, ..default() }).with_children(|t| set_line(t, face, &line, 0.0, h * 0.8, color));
}

fn button(p: &mut ChildSpawnerCommands, face: &Face, label: &str, act: Act) {
    let line = face.font.lines(label, None).into_iter().next().unwrap_or_default();
    let h = face.font.size;
    p.spawn((
        Node { padding: UiRect::axes(u(14.0), u(4.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
        BorderColor::all(amber(255.0)),
        BackgroundColor(amber(40.0)),
        UiButton,
        act,
    ))
    .with_children(|b| {
        b.spawn(Node { width: u(line.width), height: u(h), ..default() }).with_children(|t| set_line(t, face, &line, 0.0, h * 0.8, amber(255.0)));
    });
}

fn draw(mut commands: Commands, mut c: ResMut<Calibration>, roots: Query<Entity, With<CalibrationRoot>>) {
    if !std::mem::take(&mut c.dirty) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(faces) = &c.faces else { return };
    let paper = crate::hdr::paper_nits() as f32;
    let (field, logo, title, value, hint) = match c.step {
        Step::Closed => return,
        Step::Peak => (
            at_nits(c.peak as f32),
            at_nits(LOGO_PEAK_NITS),
            "HDR calibration 1 of 2: peak brightness",
            format!("Peak brightness: {} nits", c.peak),
            "Raise it until the logo just disappears into the white.",
        ),
        Step::Paper => (
            Color::BLACK,
            at_nits(LOGO_DARK * paper),
            "HDR calibration 2 of 2: brightness",
            format!("Brightness: {} nits", paper as u32),
            "Raise it until the logo can just be seen on the black.",
        ),
    };
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    commands.spawn((CalibrationRoot, full, BackgroundColor(field), GlobalZIndex(nvf_play::pause::FORM_Z))).with_children(|root| {
        // The logo: the word NVForever in a frame, all at the logo's brightness.
        root.spawn((
            Node { width: u(LOGO_WIDTH), height: u(LOGO_HEIGHT), border: UiRect::all(u(10.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BorderColor::all(logo),
        ))
        .with_children(|l| text(l, &faces.entry, "NVForever", logo));
        // The words and buttons on a dark strip at the foot, away from the field.
        root.spawn((
            Node { position_type: PositionType::Absolute, bottom: u(40.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: u(8.0), padding: UiRect::all(u(16.0)), ..default() },
            BackgroundColor(Color::BLACK),
        ))
        .with_children(|s| {
            text(s, &faces.entry, title, amber(255.0));
            text(s, &faces.small, &value, amber(255.0));
            text(s, &faces.small, hint, amber(170.0));
            s.spawn(Node { flex_direction: FlexDirection::Row, column_gap: u(16.0), margin: UiRect::top(u(6.0)), ..default() }).with_children(|r| {
                button(r, &faces.small, "< Lower (Left)", Act::Down);
                button(r, &faces.small, "Higher (Right) >", Act::Up);
                if c.step == Step::Paper {
                    button(r, &faces.small, "Back (Backspace)", Act::Back);
                }
                button(r, &faces.small, if c.step == Step::Paper { "Done (Enter)" } else { "Next (Enter)" }, Act::Next);
                button(r, &faces.small, "Cancel (Esc)", Act::Cancel);
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_move_and_cancel_puts_back() {
        let mut c = Calibration { step: Step::Peak, before: (0, 200), peak: 1000, ..default() };
        c.act(Act::Up);
        assert_eq!(c.peak, 1000 + PEAK_STEP);
        c.act(Act::Down);
        c.act(Act::Down);
        assert_eq!(c.peak, 1000 - PEAK_STEP);
        c.act(Act::Next);
        assert_eq!(c.step, Step::Paper);
        c.act(Act::Back);
        assert_eq!(c.step, Step::Peak);
        c.act(Act::Cancel);
        assert_eq!(c.step, Step::Closed);
        assert!(c.closing);
    }

    #[test]
    fn the_peak_stays_in_its_range() {
        let mut c = Calibration { step: Step::Peak, peak: crate::hdr::PEAK_MIN, ..default() };
        c.act(Act::Down);
        assert_eq!(c.peak, crate::hdr::PEAK_MIN);
        c.peak = crate::hdr::PEAK_MAX;
        c.act(Act::Up);
        assert_eq!(c.peak, crate::hdr::PEAK_MAX);
        crate::hdr::set_peak_nits(0);
    }
}
