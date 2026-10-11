//! The messages at the top left of the view, drawn as the original draws them
//! (docs/spec-notes/corner-messages.md): a bracket, an icon and the text beside it.
//!
//! - What is shown: messages (MESG) that are not message boxes, with the icon and the
//!   time their record gives, and the other short lines of `ScriptEffect::Notify`
//!   (objectives, quests, items, default activations) with the default icon (§5).
//! - One at a time, in the order they came, each for its record's Display Time [owner's
//!   rule, 2026-10-11]; [`CornerMessages`] is the queue (pure, tested).
//! - [`layout`] sets the text (pure, tested); the numbers are the HUD menu file's and
//!   those measured on the owner's screenshot (§2, §4).
//! - The bracket and the icons come from the interface's texture atlas where its index
//!   has them, else from their own files (§2.2). Without the game's files the text is
//!   drawn with Bevy's font beside a plain bracket.
//! - While a message box or a script's menu is open the area is hidden and its clock
//!   stands still [design], so a message is not used up behind a box.

use crate::interface::{amber, at, atlas_index, bc_supported, face, plain_font, set_text, texture, u, AtlasPiece, Face, Set};
use crate::message_menu::MessagePanel;
use crate::script_events::Note;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::Font;
use nvf_assets::GameData;
use std::collections::{HashMap, VecDeque};

// ---- the HUD menu file's numbers [documented: menus\prefabs\hudtemplates.xml, §2] ------

/// The bracket and the icon, from the area's corner: left, top, width, height. The icon
/// is drawn over the bracket.
const BRACKET: [f32; 4] = [0.0, -20.0, 470.0, 192.0];
const ICON: [f32; 4] = [0.0, -25.0, 160.0, 160.0];
/// The area's width [documented: menus\main\hud_main_menu.xml].
const AREA_WIDTH: f32 = 460.0;

// ---- measured on the owner's screenshot [observed, §4] ---------------------------------

/// The area's corner in the view.
const AREA: (f32, f32) = (25.0, 55.0);
/// Where the text starts, and how far under the area's top its first line stands.
const TEXT_X: f32 = 130.0;
const TEXT_BASE: f32 = 30.0;
/// Lines are the font's size and this much apart.
const LEADING: f32 = 2.0;
/// The text is broken at what is left of the area's width [hypothesis: the screenshot's
/// text breaks between 228 and 336 units, §4].
const WRAP: f32 = AREA_WIDTH - TEXT_X;
/// The bracket is drawn fainter than the icon and the text.
const BRACKET_STRENGTH: f32 = 0.65;

// ---- NVForever's own [design, §5] -------------------------------------------------------

/// The seconds a line without a Display Time stays.
pub const DEFAULT_SECONDS: f32 = 2.0;
/// The seconds a message takes to come and, after its time, to go.
const FADE: f32 = 0.25;
/// Messages waiting at once; more are left out (a script that shows one every frame).
const MAX_WAITING: usize = 64;

const BRACKET_FILE: &str = "interface\\icons\\mood\\glow_messages_seperator_left.dds";
/// The icon of a message without one [documented: GECK wiki, Message].
const DEFAULT_ICON: &str = "interface\\icons\\message icons\\glow_message_vaultboy_neutral.dds";
const ATLAS_INDEX: &str = "textures\\interface\\interfaceshared.tai";
/// Font 7 [documented: the file's text template; Fallout_default.ini `sFontFile_7`].
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

/// How long a message stays at full strength.
fn seconds(note: &Note) -> f32 {
    note.seconds.filter(|s| *s > 0.0).unwrap_or(DEFAULT_SECONDS)
}

/// The messages waiting and the one on screen.
#[derive(Resource, Default)]
pub struct CornerMessages {
    waiting: VecDeque<Note>,
    /// The message on screen and the seconds it has been there.
    showing: Option<(Note, f32)>,
}

impl CornerMessages {
    /// Add a message behind those waiting. One that says what the last one says (the
    /// same text, icon and time) is left out, and so is any when too many wait; `false`
    /// then.
    pub fn push(&mut self, note: Note) -> bool {
        let last = self.waiting.back().or(self.showing.as_ref().map(|s| &s.0));
        if last == Some(&note) || self.waiting.len() >= MAX_WAITING {
            return false;
        }
        self.waiting.push_back(note);
        true
    }

    /// Let `dt` seconds pass; `true` when another message (or none) is now on screen.
    pub fn step(&mut self, dt: f32) -> bool {
        let mut changed = false;
        if let Some((note, age)) = &mut self.showing {
            *age += dt;
            if *age >= seconds(note) + FADE {
                self.showing = None;
                changed = true;
            }
        }
        if self.showing.is_none() {
            if let Some(next) = self.waiting.pop_front() {
                self.showing = Some((next, 0.0));
                changed = true;
            }
        }
        changed
    }

    pub fn showing(&self) -> Option<&Note> {
        self.showing.as_ref().map(|s| &s.0)
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// How strongly the message on screen is drawn: it comes during its first moment
    /// and goes after its time.
    pub fn strength(&self) -> f32 {
        let Some((note, age)) = &self.showing else { return 0.0 };
        let left = seconds(note) + FADE - age;
        (age / FADE).min(left / FADE).clamp(0.0, 1.0)
    }
}

/// The text's lines, from the area's corner.
pub fn layout(text: &str, font: &Font) -> Vec<Set> {
    let pitch = font.size + LEADING;
    let lines = font.lines(text.trim(), Some(WRAP));
    lines.into_iter().enumerate().map(|(i, line)| Set { line, x: TEXT_X, base: TEXT_BASE + pitch * i as f32 }).collect()
}

/// The file name an atlas index knows a picture by.
fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase()
}

// ---- the game's files -------------------------------------------------------------------

type Picture = (Handle<Image>, Option<Rect>);

/// What the area is drawn with, read when the first message comes.
struct Kit {
    face: Face,
    atlas: HashMap<String, AtlasPiece>,
    /// Textures read so far, by path; `None` for one that could not be read.
    textures: HashMap<String, Option<(Handle<Image>, Vec2)>>,
}

impl Kit {
    fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        let atlas = data.and_then(|d| d.read(ATLAS_INDEX)).map(|b| atlas_index(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Kit { face: face(FONT, plain_font(36.0, 17.0), data, images, bc), atlas, textures: HashMap::new() }
    }

    fn texture(&mut self, path: String, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
        self.textures.entry(path).or_insert_with_key(|p| texture(p, data, images, bc)).clone()
    }

    /// A picture named as the interface names it (under the textures folder): its
    /// piece of the atlas when the index has its file name, else its own file.
    fn picture(&mut self, path: &str, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Option<Picture> {
        let data = data?;
        if let Some(piece) = self.atlas.get(&file_name(path)).cloned() {
            if let Some((image, size)) = self.texture(format!("textures\\interface\\{}", piece.atlas), data, images, bc) {
                let [x, y, w, h] = piece.rect;
                return Some((image, Some(Rect::new(x * size.x, y * size.y, (x + w) * size.x, (y + h) * size.y))));
            }
        }
        let own = format!("textures\\{}", path.trim_start_matches(['\\', '/']).to_ascii_lowercase());
        self.texture(own, data, images, bc).map(|(image, _)| (image, None))
    }
}

// ---- drawing ----------------------------------------------------------------------------

pub struct CornerMessagesPlugin;

impl Plugin for CornerMessagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CornerMessages>().add_systems(Update, draw.in_set(crate::PlaySet::Ui));
    }
}

#[derive(Component)]
struct AreaRoot;

/// A picture or text that carries the interface's colour, to come and go with the
/// message, and how strongly it is drawn when the message is fully there.
#[derive(Component, Clone)]
struct Part(f32);

/// A plain line of the stand-in bracket, coloured by its fill.
#[derive(Component)]
struct Fill;

fn spawn_area(commands: &mut Commands, note: &Note, kit: &mut Kit, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool, strength: f32) {
    let color = amber(255.0).with_alpha(strength);
    let faint = amber(255.0).with_alpha(strength * BRACKET_STRENGTH);
    let bracket = kit.picture(BRACKET_FILE, data, images, bc);
    let icon = note.icon.as_deref().and_then(|i| kit.picture(i, data, images, bc)).or_else(|| kit.picture(DEFAULT_ICON, data, images, bc));
    let lines = layout(&note.text, &kit.face.font);
    let picture = |(image, rect): Picture, color: Color| ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) };
    let area = Node { position_type: PositionType::Absolute, left: u(AREA.0), top: u(AREA.1), ..default() };
    commands.spawn((AreaRoot, area, GlobalZIndex(50))).with_children(|a| {
        let [x, y, w, h] = BRACKET;
        match bracket {
            Some(p) => {
                a.spawn((at(x, y, w, h), picture(p, faint), Part(BRACKET_STRENGTH)));
            }
            // Without the game's picture: a line across and a shorter one down.
            None => {
                a.spawn((at(x + 12.0, y + 14.0, 400.0, 2.0), BackgroundColor(faint), Fill));
                a.spawn((at(x + 12.0, y + 14.0, 2.0, 120.0), BackgroundColor(faint), Fill));
            }
        }
        if let Some(p) = icon {
            let [x, y, w, h] = ICON;
            a.spawn((at(x, y, w, h), picture(p, color), Part(1.0)));
        }
        for line in &lines {
            set_text(a, &kit.face, line, Vec2::ZERO, color, Part(1.0));
        }
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    mut corner: ResMut<CornerMessages>,
    panel: Res<MessagePanel>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut roots: Query<(Entity, &mut Visibility), With<AreaRoot>>,
    mut pictures: Query<(&mut ImageNode, &Part)>,
    mut texts: Query<(&mut TextColor, &Part)>,
    mut fills: Query<&mut BackgroundColor, With<Fill>>,
    mut kit: Local<Option<Kit>>,
) {
    // Behind a message box or a script's menu: hidden, and no time passes.
    let held = panel.view.is_some();
    for (_, mut v) in &mut roots {
        v.set_if_neq(if held { Visibility::Hidden } else { Visibility::Inherited });
    }
    if held {
        return;
    }
    let changed = corner.bypass_change_detection().step(time.delta_secs());
    let strength = corner.strength();
    if changed {
        for (e, _) in &roots {
            commands.entity(e).despawn();
        }
        if let Some(note) = corner.showing() {
            let bc = bc_supported(formats.as_deref());
            let kit = kit.get_or_insert_with(|| Kit::load(data.as_deref(), &mut images, bc));
            info!(
                "corner message: \"{}\" for {} s, icon {} ({} waiting)",
                note.text.replace('\n', " / "),
                seconds(note),
                note.icon.as_deref().map_or("default".into(), file_name),
                corner.waiting()
            );
            spawn_area(&mut commands, note, kit, data.as_deref(), &mut images, bc, strength);
        }
        return;
    }
    let color = |part: f32| amber(255.0).with_alpha(strength * part);
    for (mut p, part) in &mut pictures {
        if p.color != color(part.0) {
            p.color = color(part.0);
        }
    }
    for (mut t, part) in &mut texts {
        t.set_if_neq(TextColor(color(part.0)));
    }
    for mut f in &mut fills {
        f.set_if_neq(BackgroundColor(color(BRACKET_STRENGTH)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, seconds: Option<f32>) -> Note {
        Note { text: text.into(), icon: None, seconds }
    }

    /// Letters that take 17, spaces 13, lines of 36: about the game's font 7.
    fn font() -> Font {
        let mut f = plain_font(36.0, 17.0);
        f.glyphs[b' ' as usize].after = 13.0;
        f
    }

    #[test]
    fn the_screenshots_text_is_set_as_measured() {
        // Too wide for one line: broken after "Well", as on the screenshot.
        let l = layout("You are now Well Rested!", &font());
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].line.glyphs.len(), l[0].line.width), (13, 260.0));
        assert_eq!((l[0].x, l[0].base), (130.0, 30.0));
        assert_eq!((l[1].x, l[1].base, l[1].line.width), (130.0, 68.0, 119.0));
        // A text that fits is one line; the room around it is not set.
        let one = layout("  Bomb disarmed.\n", &font());
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].line.width, 13.0 * 17.0 + 13.0);
    }

    #[test]
    fn messages_are_shown_one_at_a_time_for_their_own_time() {
        let mut c = CornerMessages::default();
        assert!(c.push(note("first", Some(10.0))));
        assert!(c.push(note("second", None)));
        assert!(c.push(note("third", Some(0.0))));
        assert!(c.showing().is_none());
        // The first comes at once and stays its ten seconds, then goes.
        assert!(c.step(0.0));
        assert_eq!(c.showing().unwrap().text, "first");
        assert_eq!(c.strength(), 0.0);
        assert!(!c.step(0.125));
        assert_eq!(c.strength(), 0.5);
        assert!(!c.step(9.875));
        assert_eq!((c.showing().unwrap().text.as_str(), c.strength(), c.waiting()), ("first", 1.0, 2));
        assert!(!c.step(0.125));
        assert_eq!(c.strength(), 0.5);
        // Only then the second, for the time of a line without one; a time of 0 is
        // taken as none.
        assert!(c.step(0.125));
        assert_eq!(c.showing().unwrap().text, "second");
        assert!(!c.step(DEFAULT_SECONDS));
        assert!(c.step(FADE));
        assert_eq!(c.showing().unwrap().text, "third");
        assert!(!c.step(DEFAULT_SECONDS));
        assert!(c.step(FADE));
        assert!(c.showing().is_none());
        assert_eq!(c.strength(), 0.0);
        assert!(!c.step(1.0));
    }

    #[test]
    fn a_message_said_twice_running_is_shown_once() {
        let mut c = CornerMessages::default();
        assert!(c.push(note("a", None)));
        assert!(!c.push(note("a", None)));
        assert!(c.push(note("b", None)));
        assert!(c.push(note("a", None)));
        assert_eq!(c.waiting(), 3);
        // The one on screen counts as the last when none waits.
        let mut c = CornerMessages::default();
        c.push(note("a", None));
        c.step(0.0);
        assert!(!c.push(note("a", None)));
        assert!(c.push(note("a", Some(4.0))));
        // No more than the most that may wait.
        let mut c = CornerMessages::default();
        for i in 0..MAX_WAITING + 5 {
            c.push(note(&i.to_string(), None));
        }
        assert_eq!(c.waiting(), MAX_WAITING);
    }

    #[test]
    fn a_pictures_file_name_is_what_the_atlas_knows_it_by() {
        assert_eq!(file_name("Interface\\Icons\\Message Icons\\Glow_Message_Vaultboy_In_Pain.dds"), "glow_message_vaultboy_in_pain.dds");
        assert_eq!(file_name("a.dds"), "a.dds");
    }
}
