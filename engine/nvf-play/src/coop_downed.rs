//! The downed player's own screen in co-op (the fork's own; queue item 46, the owner's
//! picks, mockups 4A, 4B and 4D of 2026-11-06): while this player is down the death
//! camera fades the view to black (`nvforever`'s `death_cam.rs`), and on it, centred, the
//! Life Giver perk picture in the HUD's colour with one or two lines in the HUD's font:
//!
//! - while the respawn waits: "Respawn in 42" over "or wait for a revive", the picture
//!   dimmed (4A);
//! - once it can: "E) Respawn by <name>" over "or wait for a revive", E being the
//!   Activate control as bound (4B);
//! - while another player revives this one: "<name> is reviving you" (4D).
//!
//! What to show is written by `nvforever`'s `coop.rs` ([`DownedScreen`]); this module
//! only draws it. No backing and no border: the black is the death camera's.
//!
//! **The syncing screen** (queue item 186, the owner's addition of 2026-11-10): a guest
//! that has just joined, or come back by loading a save after going down, sees a black
//! screen in the same style, the Swift Learner perk picture drawn as the owner's
//! outlines without its "XP" letters (queue item 208, [`crate::sync_icon`]) over
//! "Syncing..." (no other text), until its game holds the host's world as it is now (`nvforever`'s
//! `coop/syncing.rs` decides; [`SyncingScreen`]). Its black is its own, drawn over the
//! game and under the loading screen.

use crate::interface::{amber, at, face, plain_font, Face, Pictures};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

/// The Life Giver perk's Pip-Boy picture [observed: `textures\interface\icons\
/// pipboyimages\perks\perk_life giver.dds` in Fallout - Textures2.bsa; the owner's pick,
/// 2026-11-06].
pub const ICON: &str = r"Interface\Icons\PipboyImages\Perks\perk_life giver.dds";
/// The Swift Learner perk's Pip-Boy picture, the syncing screen's (item 186's pick), drawn
/// as the owner's outlines (item 208, [`crate::sync_icon`]).
pub const SYNC_ICON: &str = crate::sync_icon::SOURCE;
/// The syncing screen's one line [design: the owner's words, item 186].
pub const SYNC_TEXT: &str = "Syncing...";
/// The syncing screen over the game and the HUD, under the loading screen (200).
const SYNC_Z: i32 = 150;
/// The picture's side, interface units (960 high) [design: mockups 4A-4D].
const ICON_SIZE: f32 = 128.0;
/// The picture's strength while the respawn waits [design: mockup 4A, "dimmed"].
const DIM: f32 = 0.45;
/// The second line's size against the first's [design: mockups 4A, 4B].
const SMALL: f32 = 0.72;

/// What the downed screen says now.
#[derive(Debug, Clone, PartialEq)]
pub enum Downed {
    /// The respawn waits this many seconds more.
    Waiting { seconds: u32 },
    /// The player can respawn beside `by` (`None`: where the game puts it).
    Ready { by: Option<String> },
    /// Another player is reviving this one.
    Reviving { by: String },
}

impl Downed {
    /// The lines shown, given the Activate control's prompt (`"E)"`), and whether the
    /// picture is dimmed.
    pub fn lines(&self, key: &str) -> (String, Option<&'static str>, bool) {
        const WAIT: &str = "or wait for a revive";
        match self {
            Downed::Waiting { seconds } => (format!("Respawn in {seconds}"), Some(WAIT), true),
            Downed::Ready { by: Some(n) } => (format!("{key} Respawn by {n}"), Some(WAIT), false),
            Downed::Ready { by: None } => (format!("{key} Respawn"), Some(WAIT), false),
            Downed::Reviving { by } => (format!("{by} is reviving you"), None, false),
        }
    }
}

/// Written by `coop.rs`: what to show, `None` when this player is not down.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct DownedScreen(pub Option<Downed>);

/// Written by `nvforever`'s `coop/syncing.rs`: whether the syncing screen is up.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SyncingScreen(pub bool);

pub struct CoopDownedPlugin;

impl Plugin for CoopDownedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DownedScreen>().init_resource::<SyncingScreen>();
        if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
            app.add_systems(Update, (draw, draw_syncing).in_set(PlaySet::Ui));
        }
    }
}

#[derive(Component)]
struct DownedRoot;

#[derive(Component)]
struct SyncingRoot;

struct Kit {
    large: Face,
    small: Face,
    pictures: Pictures,
}

fn scaled(face: &Face, k: f32) -> Face {
    let mut font = face.font.clone();
    font.size *= k;
    for g in &mut font.glyphs {
        (g.width, g.height, g.before, g.after, g.ascent) = (g.width * k, g.height * k, g.before * k, g.after * k, g.ascent * k);
    }
    Face { font, image: face.image.clone() }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    screen: Res<DownedScreen>,
    roots: Query<Entity, With<DownedRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
    mut images: ResMut<Assets<Image>>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    mut last: Local<Option<(Downed, String, i32)>>,
    mut kit: Local<Option<Kit>>,
) {
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let key = bindings.as_deref().map_or_else(|| "E)".to_string(), |b| b.prompt(crate::bindings::Control::Activate));
    let now = screen.0.clone().map(|d| (d, key.clone(), width.round() as i32));
    if now == *last {
        return;
    }
    *last = now.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some((shown, key, _)) = now else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let data = data.as_deref();
    let kit = kit.get_or_insert_with(|| {
        let large = face(crate::hud::FONT, plain_font(36.0, 15.0), data, &mut images, bc);
        Kit { small: scaled(&large, SMALL), large, pictures: Pictures::load(data) }
    });
    let (first, second, dim) = shown.lines(&key);
    let bright = amber(255.0);
    let (cx, top) = (width / 2.0, 960.0 / 2.0 - ICON_SIZE + 20.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands.spawn((DownedRoot, full, GlobalZIndex(104))).with_children(|r| {
        if let Some((image, rect)) = kit.pictures.picture(ICON, data, &mut images, bc) {
            let color = bright.with_alpha(if dim { DIM } else { 1.0 });
            r.spawn((at(cx - ICON_SIZE / 2.0, top, ICON_SIZE, ICON_SIZE), ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
        }
        let y = top + ICON_SIZE + 10.0;
        crate::weapon_wheel::draw::text(r, &kit.large, &first, cx, y, 0, bright);
        if let Some(s) = second {
            crate::weapon_wheel::draw::text(r, &kit.small, s, cx, y + kit.large.font.size + 2.0, 0, bright.with_alpha(0.75));
        }
    });
    info!("co-op downed screen: {first}{}", second.map_or(String::new(), |s| format!(" / {s}")));
}

/// The syncing screen's picture once made: None before, then the image or, when it could
/// not be made, nothing to draw.
type MadeIcon = Option<Option<Handle<Image>>>;

/// The syncing screen (module notes): the revive screen's look on its own black.
#[allow(clippy::too_many_arguments)]
fn draw_syncing(
    mut commands: Commands,
    screen: Res<SyncingScreen>,
    roots: Query<Entity, With<SyncingRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<i32>>,
    (mut kit, mut icon): (Local<Option<Kit>>, Local<MadeIcon>),
) {
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let now = screen.0.then_some(width.round() as i32);
    if now == *last {
        return;
    }
    let was = last.is_some();
    *last = now;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if now.is_none() {
        if was {
            info!("co-op syncing screen: down");
        }
        return;
    }
    let bc = crate::interface::bc_supported(formats.as_deref());
    let data = data.as_deref();
    let kit = kit.get_or_insert_with(|| {
        let large = face(crate::hud::FONT, plain_font(36.0, 15.0), data, &mut images, bc);
        Kit { small: scaled(&large, SMALL), large, pictures: Pictures::load(data) }
    });
    // Made once from the player's own perk picture, decoded to its pixels so its outlines
    // can be drawn from it.
    let icon = icon
        .get_or_insert_with(|| {
            let bytes = data?.read(SYNC_ICON)?;
            match crate::sync_icon::outline_image(&bytes) {
                Ok(i) => Some(images.add(i)),
                Err(e) => {
                    warn!("co-op syncing screen: {SYNC_ICON}: {e}");
                    None
                }
            }
        })
        .clone();
    let bright = amber(255.0);
    let (cx, top) = (width / 2.0, 960.0 / 2.0 - ICON_SIZE + 20.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands.spawn((SyncingRoot, full, BackgroundColor(Color::BLACK), GlobalZIndex(SYNC_Z))).with_children(|r| {
        if let Some(image) = icon {
            r.spawn((at(cx - ICON_SIZE / 2.0, top, ICON_SIZE, ICON_SIZE), ImageNode { color: bright, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
        }
        crate::weapon_wheel::draw::text(r, &kit.large, SYNC_TEXT, cx, top + ICON_SIZE + 10.0, 0, bright);
    });
    if !was {
        info!("co-op syncing screen: up ({SYNC_TEXT})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_follow_the_mockups() {
        assert_eq!(Downed::Waiting { seconds: 42 }.lines("E)"), ("Respawn in 42".into(), Some("or wait for a revive"), true));
        assert_eq!(Downed::Ready { by: Some("Emi".into()) }.lines("F)"), ("F) Respawn by Emi".into(), Some("or wait for a revive"), false));
        assert_eq!(Downed::Reviving { by: "Emi".into() }.lines("E)"), ("Emi is reviving you".into(), None, false));
    }
}
