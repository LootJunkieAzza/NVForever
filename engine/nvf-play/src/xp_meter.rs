//! The HUD's XP meter (`XPMeter` of `menus\main\hud_main_menu.xml`;
//! docs/spec-notes/experience.md §4): when the player earns experience, a bar on the
//! right just above the action points with the level the player has at its left end, the next
//! level at its right end, a pointer over it at how far the player has come between the
//! two, and "+n XP" under its right end; "LEVEL UP" when the next level is reached. It is
//! not shown at the level cap.
//!
//! - [`Meter`] is the model (pure, tested): it watches the player's experience and level
//!   ([`fnv_game::experience::Standing`]) and says what is on the screen.
//! - Any rise of the experience shows the meter, whatever gave it (kills, locks, places
//!   found, `RewardXP`): the model compares with what it saw last. A fall (`ResetXP`) or
//!   a new level only resets what it saw.
//! - When the next level is earned, the bar's "LEVEL UP" plays `UILevelUpText` and the
//!   level-up menu opens [`LEVEL_UP_DELAY`] seconds later, once the player is also in the
//!   world, out of every menu and hunted by nobody ([`fnv_game::experience::level_up`]);
//!   one level at a time, the next (sound and wait again) after the menu closes.
//! - Logged: `xp meter: +<n> XP (<xp> in all): level <l>, <p> % to <l+1>`,
//!   `xp meter: LEVEL UP (<l> -> <l+1>)`, `xp meter: +<n> XP at the level cap <max>: not
//!   shown`, `xp: level up taken: level <l> (cap <max>)`, and the cap once:
//!   `xp: level cap <max> (<k> story add-ons)`.

use crate::hud::HudView;
use crate::interface::{amber, at, bc_supported, face, plain_font, set_text, texture, u, Face, Set};
use crate::script::{with_world, ScriptState};
use crate::sfx::{PlaySound, Sfx};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::Font;
use fnv_game::experience::{self, Standing};
use fnv_sim::FormId;
use nvf_assets::GameData;
use std::collections::HashMap;

// ---- the menu file's numbers [documented: hud_main_menu.xml `XPMeter`] -----------------

/// The bracket picture `glow_XP_bar_over.dds`, 512 x 256.
const BRACKET: Vec2 = Vec2::new(512.0, 256.0);
const BRACKET_PATH: &str = "textures\\interface\\hud\\glow_xp_bar_over.dds";
/// The pointer `glow_XP_bar_pointer.dds`, 64 square, its x from `_x_min` (no experience
/// towards the next level) to `_x_max` (all of it).
const POINTER: f32 = 64.0;
const POINTER_PATH: &str = "textures\\interface\\hud\\glow_xp_bar_pointer.dds";
const POINTER_X: (f32, f32) = (55.0, 270.0);
/// `XPLastLevel` ends at x 70 (right-justified), `XPNextLevel` starts at 315, both in the
/// menus' default font 1; `XPAmount` ends at 315 in font 8, `XPLabel` ("XP") at 350 in
/// font 7, `XPLevelUp` ("LEVEL UP") at 360 in font 8.
const LAST_LEVEL_END: f32 = 70.0;
const NEXT_LEVEL_X: f32 = 315.0;
const AMOUNT_END: f32 = 315.0;
const LABEL_END: f32 = 350.0;
const LEVEL_UP_END: f32 = 360.0;
const LEVEL_FONT: &str = "textures\\fonts\\glow_monofonto_large.fnt";
const LABEL_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
const AMOUNT_FONT: &str = "textures\\fonts\\glow_futura_caps_large.fnt";
/// The words: `sStatsXP` and `sLevelUp`, which the masters do not set [observed: not
/// among FalloutNV.esm's GMST; the words: hud.md §14.2, XPLabel and XPLevelUp].
pub(crate) const XP_WORD: &str = "XP";
pub(crate) const LEVEL_UP_WORD: &str = "LEVEL UP";

// ---- what the pictures and the owner's screenshot show [observed] ----------------------

/// The bracket's drawn part: the long line (rows 64–72) from x 7, the nine ticks centred
/// 86.5 … 302.5, the upright at 364–367 from row 7 to 129 [observed: the texture].
const PIECE: Vec2 = Vec2::new(368.0, 130.0);
const LINE_MIDDLE: f32 = 68.0;
/// The pointer's tip stands on the ticks' tops (row 47), not among them; the picture's
/// triangle has its tip at row 39 [owner, 2026-10-20: "at the top of each notch"; observed:
/// the texture].
const POINTER_Y: f32 = 46.0 - 39.0;
/// The lines the texts stand on [observed: the owner's screenshot, as above]: the levels
/// 49, "+54" 118, "XP" 103; "LEVEL UP" where the amount stands.
const LEVEL_BASE: f32 = 49.0;
const AMOUNT_BASE: f32 = 118.0;
const LABEL_BASE: f32 = 103.0;

/// The meter's place: from the view's right edge to the upright, and from the view's
/// bottom edge to the bracket's bottom (25 units over the action points' piece, the line
/// at 244) [observed: the owner's full-screen screenshot at 1920 x 1080, 1.125 pixels to
/// the unit: the upright at x 1862, the line at y 805].
const MARGIN_X: f32 = 52.0;
const BOTTOM: f32 = 182.0;

// ---- set here [hypothesis: nothing documents the meter's timing] -----------------------

/// Fading in and out: the HUD menu's quarter of a second [documented: hud_main_menu.xml].
const FADE: f32 = 0.25;
/// How long the meter stays after the last gain, and how long the pointer takes to move.
const HOLD: f32 = 3.0;
const SLIDE: f32 = 0.6;
/// From the level-up sound to the level-up menu, counted while the HUD is up [owner,
/// 2026-10-24: "play this sound then wait 5 seconds before showing the level up screen"].
pub const LEVEL_UP_DELAY: f32 = 5.0;
/// The level-up sound, when the bar is filled [owner, 2026-10-24: "when the xp bar is
/// filled it should play the ui_leveluptext.wav"; observed: SOUN `UILevelUpText` names it].
const LEVEL_UP_SOUND: &str = "UILevelUpText";

/// What the meter shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub level: i32,
    /// The experience gained since the meter came up.
    pub amount: f64,
    /// The pointer moves from `from` to `to` (0 to 1 between the two levels).
    pub from: f32,
    pub to: f32,
    /// The next level is earned: "LEVEL UP" once the pointer is at the end.
    pub level_up: bool,
    /// Seconds since it came up, and since the last gain.
    pub age: f32,
    pub since_gain: f32,
}

/// What [`Meter::observe`] saw happen.
#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    /// Experience gained below the cap: the meter shows it.
    Gained { amount: f64, level_up: bool },
    /// Experience gained at the cap: not shown.
    AtCap { amount: f64 },
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Meter {
    /// The experience and level seen last.
    seen: Option<(f64, i32)>,
    pub show: Option<Show>,
    /// "LEVEL UP" has been put up for this show (its sound plays once).
    level_up_shown: bool,
}

impl Meter {
    /// Compare the player's standing with what was seen last.
    pub fn observe(&mut self, now: &Standing) -> Option<Seen> {
        let before = self.seen.replace((now.xp, now.level));
        let (xp, level) = before?;
        if now.xp <= xp {
            if now.level != level {
                self.show = None;
            }
            return None;
        }
        let amount = now.xp - xp;
        if now.at_max() {
            self.show = None;
            return Some(Seen::AtCap { amount });
        }
        let level_up = now.due();
        let pointer = self.pointer();
        match &mut self.show {
            Some(s) if s.level == now.level => {
                // On from where the pointer is.
                s.from = pointer;
                s.amount += amount;
                s.to = now.fraction();
                s.level_up = level_up;
                s.since_gain = 0.0;
                // Back to full strength if it was fading out.
                s.age = s.age.min(FADE);
            }
            _ => {
                let from = if level != now.level { 0.0 } else { now.fraction_of(xp) };
                self.show = Some(Show { level: now.level, amount, from, to: now.fraction(), level_up, age: 0.0, since_gain: 0.0 });
                self.level_up_shown = false;
            }
        }
        Some(Seen::Gained { amount, level_up })
    }

    /// Time passes while the meter is on the screen. True when "LEVEL UP" comes up.
    pub fn step(&mut self, dt: f32) -> bool {
        let Some(s) = &mut self.show else { return false };
        s.age += dt;
        s.since_gain += dt;
        if s.since_gain >= SLIDE.max(FADE) + HOLD + FADE {
            self.show = None;
            return false;
        }
        if self.level_up_word() && !self.level_up_shown {
            self.level_up_shown = true;
            return true;
        }
        false
    }

    /// The pointer's place now, 0 to 1.
    pub fn pointer(&self) -> f32 {
        self.show.as_ref().map_or(0.0, |s| {
            let t = (s.since_gain / SLIDE).clamp(0.0, 1.0);
            // Eased out: fast at first, settling on its place.
            let t = 1.0 - (1.0 - t) * (1.0 - t);
            s.from + (s.to - s.from) * t
        })
    }

    /// How strong the meter is drawn, 0 to 1: fading in, full, fading out.
    pub fn strength(&self) -> f32 {
        self.show.as_ref().map_or(0.0, |s| {
            let left = SLIDE.max(FADE) + HOLD + FADE - s.since_gain;
            (s.age / FADE).min(left / FADE).clamp(0.0, 1.0)
        })
    }

    /// "LEVEL UP" stands in place of the amount: the next level is earned and the pointer
    /// has reached it.
    pub fn level_up_word(&self) -> bool {
        self.show.as_ref().is_some_and(|s| s.level_up && s.since_gain >= SLIDE)
    }

    /// The meter is gone (the level-up menu waits for this).
    pub fn idle(&self) -> bool {
        self.show.is_none()
    }

    /// What the texts read: the two levels, and the amount or "LEVEL UP".
    pub fn texts(&self) -> Option<(String, String, String)> {
        let s = self.show.as_ref()?;
        let third = if self.level_up_word() { LEVEL_UP_WORD.to_string() } else { format!("+{}", s.amount.round() as i64) };
        Some((s.level.to_string(), (s.level + 1).to_string(), third))
    }
}

/// Where the pointer picture stands for a place 0 to 1 [documented: `_x_min`, `_x_max`].
pub fn pointer_x(fraction: f32) -> f32 {
    POINTER_X.0 + (POINTER_X.1 - POINTER_X.0) * fraction.clamp(0.0, 1.0)
}

pub struct XpMeterPlugin;

impl Plugin for XpMeterPlugin {
    fn build(&self, app: &mut App) {
        // The standing is read before the compass's finds of the frame: on the first frame
        // the scripts run, the start's place found would otherwise be taken as where the
        // meter began, and its +10 XP never shown.
        app.init_resource::<Meter>().add_systems(Update, (watch.before(crate::compass::discover), level_up, draw).chain().in_set(PlaySet::Ui).after(crate::hud::HudSet));
    }
}

/// Read the player's standing, show gains, move time on while the HUD is up.
#[allow(clippy::too_many_arguments)]
fn watch(
    time: Res<Time>,
    view: Res<HudView>,
    mut script: Option<ResMut<ScriptState>>,
    mut meter: ResMut<Meter>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut sound_forms: Local<HashMap<&'static str, Option<FormId>>>,
    mut cap_logged: Local<bool>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        meter.set_if_neq(Meter::default());
        return;
    };
    let now = experience::standing(&rt.game);
    if !*cap_logged {
        *cap_logged = true;
        let dlcs = experience::STORY_DLCS.iter().filter(|d| rt.game.data.load_order.iter().any(|p| p.eq_ignore_ascii_case(d))).count();
        info!("xp: level cap {} ({dlcs} story add-ons)", now.max);
    }
    let mut play = |name: &'static str| play_sound(sfx.as_deref(), &mut sound_forms, &mut sounds, name);
    match meter.observe(&now) {
        Some(Seen::Gained { amount, level_up }) => {
            info!("xp meter: +{amount} XP ({} in all): level {}, {:.0} % to {}{}", now.xp, now.level, now.fraction() * 100.0, now.level + 1, if level_up { ", next level earned" } else { "" });
            // Every gain [owner, 2026-10-16: "every time you get experience there is also a
            // sound effect that plays"; hypothesis: this record, by its name].
            play("UIPopUpExperienceUp");
        }
        Some(Seen::AtCap { amount }) => info!("xp meter: +{amount} XP at the level cap {}: not shown", now.max),
        None => {}
    }
    // Held, its time standing still, while the HUD is not up (menus, conversations).
    if view.shown && meter.bypass_change_detection().step(time.delta_secs()) {
        // The sound and the wait for the menu are `level_up`'s.
        info!("xp meter: LEVEL UP ({} -> {})", now.level, now.level + 1);
    }
}

/// Play a SOUN by Editor ID at the listener, its FormID looked up once.
fn play_sound(sfx: Option<&Sfx>, cache: &mut HashMap<&'static str, Option<FormId>>, sounds: &mut MessageWriter<PlaySound>, name: &'static str) {
    let Some(sfx) = sfx else { return };
    let found = *cache.entry(name).or_insert_with(|| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(name))).map(|(f, _)| *f));
    match found {
        Some(sound) => {
            info!("xp meter: sound {name}");
            sounds.write(PlaySound { sound, at: None });
        }
        None => warn!("xp meter: sound {name} not found"),
    }
}

/// For an earned level: the level-up sound once the bar shows "LEVEL UP" (or at once when
/// no bar is up, as for the second of two levels earned together), then the level-up menu
/// [`LEVEL_UP_DELAY`] seconds later once the player can take it: in the world, out of
/// every menu, and nobody fighting or searching for them ([`experience::hunters`], the
/// owner's rule). The wait counts only while the HUD is up.
#[allow(clippy::too_many_arguments)]
fn level_up(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    view: Res<HudView>,
    meter: Res<Meter>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    sfx: Option<Res<Sfx>>,
    mut sounds: MessageWriter<PlaySound>,
    mut sound_forms: Local<HashMap<&'static str, Option<FormId>>>,
    mut since_sound: Local<Option<f32>>,
    mut waiting: Local<usize>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        *since_sound = None;
        return;
    };
    if !experience::level_up_due(&rt.game) {
        *since_sound = None;
        return;
    }
    if *state.get() != PlayState::Playing || !view.shown {
        return;
    }
    let Some(since) = since_sound.as_mut() else {
        if meter.level_up_word() || meter.idle() {
            play_sound(sfx.as_deref(), &mut sound_forms, &mut sounds, LEVEL_UP_SOUND);
            *since_sound = Some(0.0);
        }
        return;
    };
    *since += time.delta_secs();
    if *since < LEVEL_UP_DELAY {
        return;
    }
    let g = &rt.game;
    if g.in_menu() {
        return;
    }
    let hunters = experience::hunters(g);
    if hunters != *waiting {
        if hunters > 0 {
            info!("xp: level up waits: {hunters} actors fighting or searching for the player");
        }
        *waiting = hunters;
    }
    if hunters > 0 {
        return;
    }
    *since_sound = None;
    let (level, max) = with_world(rt, &mut sim, |rt| (experience::level_up(&mut rt.game), experience::max_level(&rt.game)));
    info!("xp: level up taken: level {level} (cap {max})");
}

/// The meter's pieces.
#[derive(Component)]
struct MeterRoot;
#[derive(Component)]
struct Pointer;
#[derive(Component)]
struct Texts;
/// Drawn at the meter's strength: pictures and letters, and the plain blocks that stand
/// in for the pictures without the game's files (every node has a background colour, so
/// the two are kept apart).
#[derive(Component, Clone)]
struct Faded;
#[derive(Component)]
struct FadedFill;

struct Kit {
    bracket: Option<Handle<Image>>,
    pointer: Option<Handle<Image>>,
    level: Face,
    label: Face,
    amount: Face,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    view: Res<HudView>,
    meter: Res<Meter>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut kit: Local<Option<Kit>>,
    mut shown_texts: Local<Option<(String, String, String)>>,
    mut root: Query<&mut Visibility, With<MeterRoot>>,
    texts: Query<Entity, With<Texts>>,
    mut pointer: Query<&mut Node, With<Pointer>>,
    mut pictures: Query<&mut ImageNode, With<Faded>>,
    mut words: Query<&mut TextColor, With<Faded>>,
    mut fills: Query<&mut BackgroundColor, With<FadedFill>>,
) {
    let on = view.shown && meter.show.is_some();
    if kit.is_none() {
        if !on {
            return;
        }
        let bc = bc_supported(formats.as_deref());
        let d = data.as_deref();
        let mut read = |path: &str| d.and_then(|d| texture(path, d, &mut images, bc)).map(|t| t.0);
        let (bracket, pointer) = (read(BRACKET_PATH), read(POINTER_PATH));
        let loaded = Kit {
            bracket,
            pointer,
            level: face(LEVEL_FONT, plain_font(36.0, 15.0), d, &mut images, bc),
            label: face(LABEL_FONT, plain_font(36.0, 15.0), d, &mut images, bc),
            amount: face(AMOUNT_FONT, plain_font(52.0, 22.0), d, &mut images, bc),
        };
        spawn(&mut commands, &loaded);
        *kit = Some(loaded);
        *shown_texts = None;
        return;
    }
    for mut v in &mut root {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !on {
        return;
    }
    let Some(kit) = kit.as_ref() else { return };
    let colour = amber(255.0).with_alpha(meter.strength());
    let now = meter.texts();
    if *shown_texts != now {
        if let (Ok(e), Some((last, next, third))) = (texts.single(), &now) {
            commands.entity(e).despawn_related::<Children>().with_children(|p| {
                let line = |f: &Font, t: &str| f.lines(t, None).into_iter().next();
                if let Some(l) = line(&kit.level.font, last) {
                    let x = LAST_LEVEL_END - l.width;
                    set_text(p, &kit.level, &Set { line: l, x, base: LEVEL_BASE }, Vec2::ZERO, colour, Faded);
                }
                if let Some(l) = line(&kit.level.font, next) {
                    set_text(p, &kit.level, &Set { line: l, x: NEXT_LEVEL_X, base: LEVEL_BASE }, Vec2::ZERO, colour, Faded);
                }
                if third == LEVEL_UP_WORD {
                    if let Some(l) = line(&kit.amount.font, third) {
                        let x = LEVEL_UP_END - l.width;
                        set_text(p, &kit.amount, &Set { line: l, x, base: AMOUNT_BASE }, Vec2::ZERO, colour, Faded);
                    }
                } else {
                    if let Some(l) = line(&kit.amount.font, third) {
                        let x = AMOUNT_END - l.width;
                        set_text(p, &kit.amount, &Set { line: l, x, base: AMOUNT_BASE }, Vec2::ZERO, colour, Faded);
                    }
                    if let Some(l) = line(&kit.label.font, XP_WORD) {
                        let x = LABEL_END - l.width;
                        set_text(p, &kit.label, &Set { line: l, x, base: LABEL_BASE }, Vec2::ZERO, colour, Faded);
                    }
                }
            });
        }
        *shown_texts = now;
    }
    let left = u(pointer_x(meter.pointer()));
    for mut n in &mut pointer {
        if n.left != left {
            n.left = left;
        }
    }
    for mut p in &mut pictures {
        if p.color != colour {
            p.color = colour;
        }
    }
    for mut t in &mut words {
        t.set_if_neq(TextColor(colour));
    }
    for mut f in &mut fills {
        f.set_if_neq(BackgroundColor(colour));
    }
}

/// The meter: the bracket, the pointer and a holder for the texts, its upright
/// [`MARGIN_X`] from the right edge and its bottom [`BOTTOM`] over the view's bottom.
fn spawn(commands: &mut Commands, kit: &Kit) {
    let colour = amber(255.0).with_alpha(0.0);
    let piece = Node {
        position_type: PositionType::Absolute,
        right: u(MARGIN_X),
        bottom: u(BOTTOM),
        width: u(PIECE.x),
        height: u(PIECE.y),
        ..default()
    };
    commands.spawn((MeterRoot, piece, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
        match &kit.bracket {
            Some(image) => {
                p.spawn((at(0.0, 0.0, BRACKET.x, BRACKET.y), ImageNode { color: colour, ..ImageNode::new(image.clone()) }, Faded));
            }
            None => {
                // Without the picture: the line, the ticks and the upright as plain blocks.
                p.spawn((at(7.0, LINE_MIDDLE - 2.0, 360.0, 4.0), BackgroundColor(colour), FadedFill));
                for i in 0..9 {
                    p.spawn((at(85.0 + 27.0 * i as f32, 48.0, 4.0, 16.0), BackgroundColor(colour), FadedFill));
                }
                p.spawn((at(364.0, 7.0, 4.0, 122.0), BackgroundColor(colour), FadedFill));
            }
        }
        let mut place = at(pointer_x(0.0), POINTER_Y, POINTER, POINTER);
        match &kit.pointer {
            Some(image) => {
                p.spawn((Pointer, place, ImageNode { color: colour, ..ImageNode::new(image.clone()) }, Faded));
            }
            None => {
                // A small block where the triangle would be.
                place.width = u(POINTER);
                p.spawn((Pointer, place)).with_children(|b| {
                    b.spawn((at(24.0, 29.0, 16.0, 10.0), BackgroundColor(colour), FadedFill));
                });
            }
        }
        p.spawn((Texts, at(0.0, 0.0, PIECE.x, PIECE.y)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player at `level` with `xp`, between the default totals (200 for level 2, 550
    /// for 3, 1050 for 4), under a cap of 30.
    fn at(level: i32, xp: f64) -> Standing {
        let t = |n| experience::total_for(n, experience::DEFAULT_XP_BASE, experience::DEFAULT_XP_BUMP);
        Standing { xp, level, max: 30, this: t(level), next: t(level + 1) }
    }

    fn run(m: &mut Meter, seconds: f32) -> bool {
        let mut up = false;
        for _ in 0..(seconds * 60.0) as usize {
            up |= m.step(1.0 / 60.0);
        }
        up
    }

    #[test]
    fn the_first_look_and_no_change_show_nothing() {
        let mut m = Meter::default();
        assert_eq!(m.observe(&at(2, 300.0)), None);
        assert_eq!(m.observe(&at(2, 300.0)), None);
        assert!(m.idle());
    }

    /// The owner's screenshot: level 2 to 3, the pointer at about nine tenths, "+54".
    #[test]
    fn a_gain_shows_the_levels_the_pointer_and_the_amount() {
        let mut m = Meter::default();
        m.observe(&at(2, 450.0));
        assert_eq!(m.observe(&at(2, 504.0)), Some(Seen::Gained { amount: 54.0, level_up: false }));
        let s = m.show.clone().unwrap();
        assert!((s.from - 250.0 / 350.0).abs() < 1e-6 && (s.to - 304.0 / 350.0).abs() < 1e-6);
        assert_eq!(m.texts(), Some(("2".into(), "3".into(), "+54".into())));
        // The pointer starts where the player was and settles where the player is.
        assert_eq!(m.pointer(), s.from);
        assert!(!run(&mut m, 1.0));
        assert!((m.pointer() - s.to).abs() < 1e-6);
        assert_eq!(m.strength(), 1.0);
        // Gone some seconds after.
        run(&mut m, 4.0);
        assert!(m.idle() && m.strength() == 0.0);
    }

    #[test]
    fn gains_while_shown_add_up_and_keep_it_up() {
        let mut m = Meter::default();
        m.observe(&at(1, 0.0));
        m.observe(&at(1, 20.0));
        run(&mut m, 3.0);
        m.observe(&at(1, 30.0));
        assert_eq!(m.texts().unwrap().2, "+30");
        run(&mut m, 3.0);
        assert!(!m.idle());
    }

    #[test]
    fn reaching_the_next_level_says_level_up_once_the_pointer_is_there() {
        let mut m = Meter::default();
        m.observe(&at(1, 150.0));
        assert_eq!(m.observe(&at(1, 210.0)), Some(Seen::Gained { amount: 60.0, level_up: true }));
        assert_eq!(m.show.as_ref().unwrap().to, 1.0);
        assert_eq!(m.texts().unwrap().2, "+60");
        assert!(run(&mut m, 1.0));
        assert_eq!(m.texts().unwrap().2, "LEVEL UP");
        // The word's moment comes once.
        assert!(!run(&mut m, 1.0));
    }

    #[test]
    fn nothing_is_shown_at_the_cap() {
        let mut m = Meter::default();
        m.observe(&Standing { max: 30, ..at(30, 70_000.0) });
        assert_eq!(m.observe(&Standing { max: 30, ..at(30, 70_100.0) }), Some(Seen::AtCap { amount: 100.0 }));
        assert!(m.idle());
        // The cap with the four story add-ons: level 30 is shown again.
        m.observe(&Standing { max: 50, ..at(30, 70_200.0) });
        assert!(!m.idle());
    }

    #[test]
    fn a_reset_or_a_new_level_only_moves_what_was_seen() {
        let mut m = Meter::default();
        m.observe(&at(3, 600.0));
        assert_eq!(m.observe(&at(3, 0.0)), None);
        assert_eq!(m.observe(&at(4, 0.0)), None);
        assert!(m.idle());
        // A gain on a new level starts the pointer at the level's start.
        m.observe(&at(4, 1100.0));
        assert!(m.show.as_ref().unwrap().from == 0.0);
    }

    #[test]
    fn the_pointer_runs_between_the_files_bounds() {
        assert_eq!(pointer_x(0.0), 55.0);
        assert_eq!(pointer_x(1.0), 270.0);
        assert_eq!(pointer_x(2.0), 270.0);
        // Its triangle's middle (31 of 64) over the first and last ticks (86.5, 302.5).
        assert!((pointer_x(0.0) + 31.0 - 86.5).abs() <= 1.0 && (pointer_x(1.0) + 31.0 - 302.5).abs() <= 1.5);
    }
}
