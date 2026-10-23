//! Hardcore's need words and the limb word on the HUD: `H2O FOD SLP LMB` on the "HP"
//! label's line at the bottom left (docs/spec-notes/hud.md §13).
//!
//! - The words are the game's (`HardcoreMode` in `hud_main_menu.xml`: texts `Dehydration`,
//!   `Hunger`, `Sleep` and `LMBs`, font 7, the HUD's colour, set in code); their strings
//!   are the settings `sStatsH20Abbrev`, `sStatsFODAbbrev`, `sStatsSLPAbbrev` and
//!   `sStatsLMBAbbrev`, which no master sets, so the words of the owner's screenshot stand
//!   in when no plugin does.
//! - Each word has one of four looks ([`Look`]): faded, the HUD's amber, red, flashing red.
//!   A need's look follows the stage the game holds for it (`fnv_game::needs`); the limb
//!   word's follows how many limbs are crippled.
//! - The need words show in Hardcore only; the limb word in every mode [design].
//! - [`need_look`] and [`limb_look`] are pure, tested without the game.

use crate::combat::PlayerLimbs;
use crate::hud::{HudSet, HudView, FONT, HP, HP_LABEL_X, HP_PANEL, MARGIN_X, MARGIN_Y, UPPER_BASE};
use crate::interface::{amber, bc_supported, face, plain_font, set_text, u, Face, Set};
use crate::reticle::crime_colour;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_game::aid::{DEHYDRATION, HUNGER, SLEEP_DEPRIVATION};
use fnv_sim::settings::Setting;
use nvf_assets::GameData;

/// The faded look: the HUD's amber at this opacity [design: owner 2026-10-23, "faded
/// (dim)"; the original's opacity is not measured].
const FADED: f32 = 0.3;
/// Room between "HP" and the first word, in spaces of the font [hypothesis: owner
/// 2026-10-23, the words start "a little right of the HP label"; not measured].
const AFTER_HP: f32 = 2.0;

/// The words: the setting each comes from and the word used without it [hypothesis: the
/// words on the owner's screenshot of the original; the settings are named by
/// `hud_main_menu.xml` and set by no master].
const WORDS: [(&str, &str); 4] = [("sStatsH20Abbrev", "H2O"), ("sStatsFODAbbrev", "FOD"), ("sStatsSLPAbbrev", "SLP"), ("sStatsLMBAbbrev", "LMB")];
/// The needs behind the first three words.
const NEEDS: [u16; 3] = [DEHYDRATION, HUNGER, SLEEP_DEPRIVATION];
/// The limb word's index in [`WORDS`].
const LMB: usize = 3;

/// How a word is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Hidden,
    Faded,
    Normal,
    Red,
    Flashing,
}

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Hidden => "hidden",
            Look::Faded => "faded",
            Look::Normal => "normal",
            Look::Red => "red",
            Look::Flashing => "flashing",
        }
    }
}

/// A need's look from the stage the game holds (`stage`, 0 the lowest, of `count` stages)
/// [design: owner 2026-10-23, "none/lowest faded, then normal, red, flashing for the last
/// ones"]: with the masters' five stages (Minor, Advanced, Critical, Deadly, Fatal) none
/// and Minor are faded, Advanced normal, Critical red, Deadly and Fatal flash. Fewer stages
/// (a plugin's) give up the faded stage first, then the normal one.
pub fn need_look(stage: Option<usize>, count: usize) -> Look {
    let Some(i) = stage else { return Look::Faded };
    // The stage counted from the worst: 0 the last.
    let from_top = count.saturating_sub(1).saturating_sub(i);
    match from_top {
        0 | 1 => Look::Flashing,
        2 => Look::Red,
        3 => Look::Normal,
        _ => Look::Faded,
    }
}

/// The limb word's look from how many limbs are crippled [design: owner 2026-10-23,
/// "0 faded, 1 normal, 2 red, 3 or more flashing red"].
pub fn limb_look(crippled: usize) -> Look {
    match crippled {
        0 => Look::Faded,
        1 => Look::Normal,
        2 => Look::Red,
        _ => Look::Flashing,
    }
}

/// The words' looks, and what each look was read from (for the log).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct NeedWords {
    pub looks: [Look; 4],
    pub why: [String; 4],
    /// Hardcore mode is on (the need words show).
    pub hardcore: bool,
}

impl Default for NeedWords {
    fn default() -> Self {
        NeedWords { looks: [Look::Hidden; 4], why: Default::default(), hardcore: false }
    }
}

pub struct HudNeedsPlugin;

impl Plugin for HudNeedsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NeedWords>().add_systems(Update, (watch, draw).chain().after(HudSet).in_set(PlaySet::Ui));
    }
}

/// Read the needs' stages and the crippled limbs; log a word whose look changes.
fn watch(mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, limbs: Option<Res<PlayerLimbs>>, mut words: ResMut<NeedWords>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let hardcore = with_world(rt, &mut sim, |rt| rt.game.hardcore());
    let mut looks = [Look::Hidden; 4];
    let mut why: [String; 4] = Default::default();
    for (i, code) in NEEDS.into_iter().enumerate() {
        if !hardcore {
            why[i] = "Hardcore off".into();
            continue;
        }
        let g = &rt.game;
        let stages = g.data.need_stages.get(&code).map_or(&[][..], |s| s.as_slice());
        let held = g.player.need_stages.get(&code).and_then(|h| stages.iter().position(|s| s.stage == *h));
        looks[i] = need_look(held, stages.len());
        why[i] = match held {
            Some(s) => format!("stage {} of {}, {}", s + 1, stages.len(), g.data.name(stages[s].spell)),
            None => "no stage".into(),
        };
    }
    let crippled = limbs.map_or(0, |l| (25..=30).filter(|&c| l.crippled(c)).count());
    looks[LMB] = limb_look(crippled);
    why[LMB] = format!("{crippled} limbs crippled");
    for i in 0..4 {
        if looks[i] != words.looks[i] {
            info!("hud needs: {} {} ({})", WORDS[i].1, looks[i].name(), why[i]);
        }
    }
    let now = NeedWords { looks, why, hardcore };
    words.set_if_neq(now);
}

/// A word's letters (they carry its colour).
#[derive(Component, Clone, Copy)]
struct Letter(usize);
/// A word, shown or hidden.
#[derive(Component)]
struct Word(usize);
#[derive(Component)]
struct WordsRoot;

/// The words' places from the piece's left: one after "HP" and a space between words;
/// outside Hardcore the limb word takes the first place.
struct Places([f32; 4], f32);

fn setting_str(sim: &SimState, name: &str) -> Option<String> {
    match sim.forms.settings.get(&name.to_ascii_lowercase()) {
        Some(Setting::Str(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<HudView>,
    words: Res<NeedWords>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut places: Local<Option<Places>>,
    mut root: Query<&mut Visibility, (With<WordsRoot>, Without<Word>)>,
    mut word_nodes: Query<(&Word, &mut Visibility, &mut Node), Without<WordsRoot>>,
    mut letters: Query<(&Letter, Option<&mut ImageNode>, Option<&mut TextColor>)>,
) {
    if places.is_none() {
        // The first frame the HUD is wanted: set the words out.
        if !view.shown {
            return;
        }
        let face_: Face = face(FONT, plain_font(36.0, 15.0), data.as_deref(), &mut images, bc_supported(formats.as_deref()));
        let font = &face_.font;
        let texts: Vec<String> = WORDS.iter().map(|(s, w)| setting_str(&sim, s).unwrap_or_else(|| (*w).to_string())).collect();
        let space = font.width(" ");
        let mut x = HP_LABEL_X + font.width(HP) + AFTER_HP * space;
        let first = x;
        let mut at_x = [0.0; 4];
        for (i, t) in texts.iter().enumerate() {
            at_x[i] = x;
            x += font.width(t) + space;
        }
        let piece = Node { position_type: PositionType::Absolute, left: u(MARGIN_X), bottom: u(MARGIN_Y), width: u(HP_PANEL.x), height: u(HP_PANEL.y), ..default() };
        commands.spawn((WordsRoot, piece, Visibility::Hidden, Pickable::IGNORE)).with_children(|p| {
            for (i, t) in texts.iter().enumerate() {
                let node = Node { position_type: PositionType::Absolute, left: u(at_x[i]), top: u(0.0), ..default() };
                p.spawn((Word(i), node, Visibility::Hidden)).with_children(|w| {
                    if let Some(line) = font.lines(t, None).into_iter().next() {
                        set_text(w, &face_, &Set { line, x: 0.0, base: UPPER_BASE }, Vec2::ZERO, amber(255.0), Letter(i));
                    }
                });
            }
        });
        *places = Some(Places(at_x, first));
        return;
    }
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut root {
        v.set_if_neq(show(view.shown));
    }
    if !view.shown {
        return;
    }
    let Some(Places(at_x, first)) = places.as_ref() else { return };
    for (w, mut vis, mut node) in &mut word_nodes {
        vis.set_if_neq(show(words.looks[w.0] != Look::Hidden));
        let left = u(if w.0 == LMB && !words.hardcore { *first } else { at_x[w.0] });
        if node.left != left {
            node.left = left;
        }
    }
    let flash = crate::hud_limbs::flash(time.elapsed_secs());
    for (l, image, text) in &mut letters {
        let color = match words.looks[l.0] {
            Look::Hidden | Look::Normal => amber(255.0),
            Look::Faded => amber(255.0).with_alpha(FADED),
            Look::Red => crime_colour(),
            Look::Flashing => flash,
        };
        if let Some(mut image) = image {
            if image.color != color {
                image.color = color;
            }
        }
        if let Some(mut text) = text {
            if text.0 != color {
                text.0 = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_masters_five_stages_take_the_four_looks() {
        assert_eq!(need_look(None, 5), Look::Faded);
        let looks: Vec<Look> = (0..5).map(|i| need_look(Some(i), 5)).collect();
        assert_eq!(looks, [Look::Faded, Look::Normal, Look::Red, Look::Flashing, Look::Flashing]);
    }

    #[test]
    fn fewer_stages_keep_the_worst_looks() {
        assert_eq!(need_look(Some(0), 3), Look::Red);
        assert_eq!(need_look(Some(2), 3), Look::Flashing);
        assert_eq!(need_look(Some(0), 1), Look::Flashing);
        // A stage out of range reads as the worst, never a panic.
        assert_eq!(need_look(Some(9), 5), Look::Flashing);
    }

    #[test]
    fn crippled_limbs_count_up_to_flashing() {
        assert_eq!(limb_look(0), Look::Faded);
        assert_eq!(limb_look(1), Look::Normal);
        assert_eq!(limb_look(2), Look::Red);
        assert_eq!(limb_look(3), Look::Flashing);
        assert_eq!(limb_look(6), Look::Flashing);
    }
}
