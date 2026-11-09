//! The overlay's panel: a column of rows, the label left and the value right-aligned, on
//! a faint dark backing with a thin dim bracket on its outer side, in the manner of the
//! HUD's bracket frames [design: issue #1868].
//!
//! The text is the game's font 7 (`baked-in_monofonto_large.fnt`, the HUD's corner
//! messages' font) set as glyphs of its texture as the HUD sets it
//! (`nvf_play::interface::set_line`), at [`TEXT_SCALE`] of the HUD's size and the UI
//! scale; without the game's font (no game data) Bevy's own font stands in, in the same
//! layout. Labels are drawn dim and values full in the HUD's colour
//! (`nvf_play::interface::amber`, so the settings' HUD colour holds here too); a row that
//! warns gets its label at full brightness and a `!` mark beside it, never another
//! colour [design: issue #1868].
//!
//! Lengths are the interface's units (960 to the view's height, `nvf_play::interface::u`).

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;
use nvf_play::interface::{amber, bc_supported, face, font_units, line_base, plain_font, set_line, u, ui_scale, units_at, Face, FONT_FILES};

use super::readings::{Row, LABELS};

/// Above every menu, the HUD (`nvf_play::interface::TOP_MENU_Z` is 107) and the loading
/// screen (200, `nvf_play::loading`), so a load's frame rate can be read too.
const Z: i32 = 201;

/// The overlay's text against the HUD's font 7, which is drawn for messages that are
/// read once: smaller, so five rows stay in a corner [design].
const TEXT_SCALE: f32 = 0.6;

/// The panel's padding (across, down), the room between the label column and the values,
/// and between a label and its mark, in the text's units [design].
const PAD: (f32, f32) = (14.0, 6.0);
const GAP: f32 = 18.0;
const MARK_GAP: f32 = 6.0;
/// The mark of a row that warns.
const MARK: &str = "!";

/// The backing's darkness: faint, so the world shows through [design: issue #1868, 35-40 %].
const BACKING: f32 = 0.38;
/// Brightnesses (0 to 255, as `amber` takes them) of the labels, the values and the
/// bracket [design].
const LABEL: f32 = 150.0;
const VALUE: f32 = 255.0;
const BRACKET: f32 = 110.0;
/// The bracket's line and the short arms it turns in with at the top and the bottom, in
/// the text's units [design].
const BRACKET_LINE: f32 = 2.0;
const BRACKET_ARM: f32 = 14.0;

/// From the view's top and right edge, in the HUD's units at the UI scale
/// [design].
const EDGE: (f32, f32) = (12.0, 16.0);

/// The panel's root, despawned with its rows when the panel is set again.
#[derive(Component)]
pub(super) struct PanelRoot;

/// What the panel is drawn with: font 7 (or its stand-in), and the label column's width.
pub(super) struct Kit {
    face: Face,
    /// Whether there was game data to read the font from; a kit made without it is made
    /// again once the data is there.
    pub(super) had_data: bool,
    label_width: f32,
    mark_width: f32,
}

impl Kit {
    pub(super) fn load(data: Option<&GameData>, images: &mut Assets<Image>, formats: Option<&CompressedImageFormatSupport>) -> Kit {
        // Font 7 [documented: Fallout_default.ini `sFontFile_7`]; the stand-in as the
        // corner messages' (`nvf_play::corner_messages`).
        let face = face(FONT_FILES[6], plain_font(36.0, 17.0), data, images, bc_supported(formats));
        let label_width = LABELS.iter().map(|l| face.font.width(l)).fold(0.0, f32::max);
        let mark_width = face.font.width(MARK);
        Kit { had_data: data.is_some(), face, label_width, mark_width }
    }

    /// The label column: the widest label, then room for the mark.
    fn label_column(&self) -> f32 {
        self.label_width + MARK_GAP + self.mark_width
    }
}

/// Spawn the panel of `rows` at the top right. `width` is the widest the panel was in this
/// showing (the text's units); it grows to these rows and never shrinks, so the backing
/// does not jump as the numbers change.
pub(super) fn spawn(commands: &mut Commands, kit: &Kit, rows: &[Row], width: &mut f32) {
    if rows.is_empty() {
        return;
    }
    let (top, side) = {
        // In the HUD's lengths at the UI scale, as the corner messages'.
        let _hud = units_at(ui_scale());
        (u(EDGE.0), u(EDGE.1))
    };
    let _text = units_at(ui_scale() * TEXT_SCALE);
    let font = &kit.face.font;
    let values: Vec<_> = rows.iter().map(|r| font.lines(&r.value, None).into_iter().next().unwrap_or_default()).collect();
    let widest = values.iter().map(|l| l.width).fold(0.0, f32::max);
    *width = width.max(kit.label_column() + GAP + widest + 2.0 * PAD.0);
    let pitch = font.size;
    let base = line_base(&kit.face);
    let node = Node {
        position_type: PositionType::Absolute,
        top,
        flex_direction: FlexDirection::Column,
        padding: UiRect::axes(u(PAD.0), u(PAD.1)),
        right: side,
        min_width: u(*width),
        ..default()
    };
    commands.spawn((PanelRoot, node, BackgroundColor(Color::BLACK.with_alpha(BACKING)), GlobalZIndex(Z), Pickable::IGNORE)).with_children(|panel| {
        bracket(panel);
        for (row, value) in rows.iter().zip(&values) {
            let line = Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, column_gap: u(GAP), height: u(pitch), ..default() };
            panel.spawn(line).with_children(|line| {
                let label = if row.warn { amber(VALUE) } else { amber(LABEL) };
                line.spawn(Node { width: u(kit.label_column()), height: u(pitch), ..default() }).with_children(|cell| {
                    text(cell, kit, row.label, 0.0, base, label);
                    if row.warn {
                        text(cell, kit, MARK, kit.label_column() - kit.mark_width, base, amber(VALUE));
                    }
                });
                line.spawn(Node { width: u(value.width), height: u(pitch), ..default() }).with_children(|cell| {
                    match kit.face.image {
                        Some(_) => set_line(cell, &kit.face, value, 0.0, base, amber(VALUE)),
                        None => stand_in(cell, kit, &row.value, 0.0, base, amber(VALUE)),
                    }
                });
            });
        }
    });
}

/// A line of text in a cell: the game's glyphs, or the stand-in's text.
fn text(cell: &mut ChildSpawnerCommands, kit: &Kit, s: &str, x: f32, base: f32, color: Color) {
    match kit.face.image {
        Some(_) => {
            let line = kit.face.font.lines(s, None).into_iter().next().unwrap_or_default();
            set_line(cell, &kit.face, &line, x, base, color);
        }
        None => stand_in(cell, kit, s, x, base, color),
    }
}

/// Bevy's font in place of the game's, its spaces kept (`nvf_play::interface::set_line`'s
/// own stand-in drops them, and the values are padded with spaces).
fn stand_in(cell: &mut ChildSpawnerCommands, kit: &Kit, s: &str, x: f32, base: f32, color: Color) {
    let size = kit.face.font.size * 0.75;
    cell.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(base - size), ..default() }).with_children(|n| {
        n.spawn((Text::new(s), TextFont { font_size: font_units(size), ..default() }, TextLayout::no_wrap(), TextColor(color)));
    });
}

/// The bracket on the panel's outer side: a line down its whole height, turning in at the
/// top and the bottom.
fn bracket(panel: &mut ChildSpawnerCommands) {
    let color = BackgroundColor(amber(BRACKET));
    let edge = |n: Node| Node { right: Val::Px(0.0), ..n };
    let down = Node { position_type: PositionType::Absolute, top: Val::Px(0.0), bottom: Val::Px(0.0), width: u(BRACKET_LINE), ..default() };
    let arm = Node { position_type: PositionType::Absolute, width: u(BRACKET_ARM), height: u(BRACKET_LINE), ..default() };
    panel.spawn((edge(down), color));
    panel.spawn((edge(Node { top: Val::Px(0.0), ..arm.clone() }), color));
    panel.spawn((edge(Node { bottom: Val::Px(0.0), ..arm }), color));
}
