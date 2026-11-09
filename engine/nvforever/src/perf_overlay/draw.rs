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
//! The frame graph row is one image node across the panel's inner width
//! ([`GRAPH_PITCHES`] rows high), tinted in the value's brightness of the HUD's colour;
//! its pixels are `super::graph`'s, as many as the node covers on screen
//! ([`graph_pixels`]), so its line stays one pixel thin.
//!
//! Lengths are the interface's units (960 to the view's height, `nvf_play::interface::u`).

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::{code_of, Font, Line, Placed};
use nvf_assets::GameData;
use nvf_play::interface::{amber, bc_supported, face, font_units, line_base, plain_font, set_line, u, ui_scale, units_at, Face, FONT_FILES};

use super::readings::Row;

/// Above every menu, the HUD (`nvf_play::interface::TOP_MENU_Z` is 107) and the loading
/// screen (200, `nvf_play::loading`), so a load's frame rate can be read too.
const Z: i32 = 201;

/// The overlay's text against the HUD's font 7, which is drawn for messages that are
/// read once: smaller, so five rows stay in a corner [design].
const TEXT_SCALE: f32 = 0.6;

/// The panel's padding (across, down), the room between the label column and the values,
/// and between a label and its mark, in the text's units [design]. The bracket's side gets
/// [`PAD_BRACKET`] instead, so the values keep clear of its line.
const PAD: (f32, f32) = (14.0, 6.0);
const PAD_BRACKET: f32 = 26.0;
const GAP: f32 = 18.0;
const MARK_GAP: f32 = 6.0;
/// The mark of a row that warns.
const MARK: &str = "!";

/// The backing's darkness: faint, so the world shows through [design: issue #1868, 35-40 %].
const BACKING: f32 = 0.38;
/// Brightnesses (0 to 255, as `amber` takes them) of the labels, the values and the
/// bracket [design].
const LABEL: f32 = 185.0;
const VALUE: f32 = 255.0;
const BRACKET: f32 = 170.0;
/// The bracket's line and the short arms it turns in with at the top and the bottom, in
/// the text's units [design].
const BRACKET_LINE: f32 = 2.0;
const BRACKET_ARM: f32 = 14.0;

/// From the view's top and right edge, in the HUD's units at the UI scale
/// [design].
const EDGE: (f32, f32) = (12.0, 16.0);

/// The frame graph row's height, in text rows [design: tall enough for a spike to read,
/// short enough for a corner].
const GRAPH_PITCHES: f32 = 1.6;
/// The graph image's least and greatest size in pixels (a sanity bound for odd views).
const GRAPH_PX: (u32, u32) = (8, 2048);

/// The panel's root, despawned with its rows when the panel is set again.
#[derive(Component)]
pub(super) struct PanelRoot;

/// What the panel is drawn with: font 7 (or its stand-in).
pub(super) struct Kit {
    face: Face,
    /// Whether there was game data to read the font from; a kit made without it is made
    /// again once the data is there.
    pub(super) had_data: bool,
    mark_width: f32,
}

impl Kit {
    pub(super) fn load(data: Option<&GameData>, images: &mut Assets<Image>, formats: Option<&CompressedImageFormatSupport>) -> Kit {
        // Font 7 [documented: Fallout_default.ini `sFontFile_7`]; the stand-in as the
        // corner messages' (`nvf_play::corner_messages`).
        let face = face(FONT_FILES[6], plain_font(36.0, 17.0), data, images, bc_supported(formats));
        let mark_width = face.font.width(MARK);
        Kit { had_data: data.is_some(), face, mark_width }
    }
}

/// The rows' values set with even-width figures, and the label column: the widest label
/// shown, then room for the mark.
pub(super) struct Measured {
    values: Vec<Line>,
    label_column: f32,
}

/// Measure `rows` and grow `width`, the widest the panel was in this showing (the text's
/// units), to them: it never shrinks, so the backing does not jump as the numbers change.
pub(super) fn measure(kit: &Kit, rows: &[Row], width: &mut f32) -> Measured {
    let font = &kit.face.font;
    let values: Vec<_> = rows.iter().map(|r| tabular(font, &r.value)).collect();
    let widest = values.iter().map(|l| l.width).fold(0.0, f32::max);
    let label_width = rows.iter().map(|r| font.width(r.label)).fold(0.0, f32::max);
    let label_column = label_width + MARK_GAP + kit.mark_width;
    if !rows.is_empty() {
        *width = width.max(label_column + GAP + widest + PAD.0 + PAD_BRACKET);
    }
    Measured { values, label_column }
}

/// The frame graph's size in pixels on a view `view_height` pixels high, for a panel
/// `width` text units wide: the panel's inner width by [`GRAPH_PITCHES`] rows.
pub(super) fn graph_pixels(kit: &Kit, width: f32, view_height: u32) -> (u32, u32) {
    // `u`: a unit is 1/960 of the view's height times the scale.
    let px = view_height as f32 / 960.0 * ui_scale() * TEXT_SCALE;
    let side = |units: f32| ((units * px).round().max(0.0) as u32).clamp(GRAPH_PX.0, GRAPH_PX.1);
    (side(width - PAD.0 - PAD_BRACKET), side(kit.face.font.size * GRAPH_PITCHES))
}

/// Spawn the panel of `rows` at the top right, `width` wide ([`measure`]); the graph row
/// shows `graph`.
pub(super) fn spawn(commands: &mut Commands, kit: &Kit, rows: &[Row], measured: &Measured, width: f32, graph: Option<Handle<Image>>) {
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
    let Measured { values, label_column } = measured;
    let label_column = *label_column;
    let pitch = font.size;
    let base = line_base(&kit.face);
    let node = Node {
        position_type: PositionType::Absolute,
        top,
        flex_direction: FlexDirection::Column,
        padding: UiRect { left: u(PAD.0), right: u(PAD_BRACKET), top: u(PAD.1), bottom: u(PAD.1) },
        right: side,
        min_width: u(width),
        ..default()
    };
    commands.spawn((PanelRoot, node, BackgroundColor(Color::BLACK.with_alpha(BACKING)), GlobalZIndex(Z), Pickable::IGNORE)).with_children(|panel| {
        bracket(panel);
        for (row, value) in rows.iter().zip(values) {
            if row.graph {
                // Across the inner width (the panel's column stretches its rows).
                let node = Node { width: Val::Percent(100.0), height: u(pitch * GRAPH_PITCHES), ..default() };
                match &graph {
                    Some(image) => panel.spawn((node, ImageNode::new(image.clone()).with_color(amber(VALUE)))),
                    None => panel.spawn(node),
                };
                continue;
            }
            let line = Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, column_gap: u(GAP), height: u(pitch), ..default() };
            panel.spawn(line).with_children(|line| {
                let label = if row.warn { amber(VALUE) } else { amber(LABEL) };
                line.spawn(Node { width: u(label_column), height: u(pitch), ..default() }).with_children(|cell| {
                    text(cell, kit, row.label, 0.0, base, label);
                    if row.warn {
                        text(cell, kit, MARK, label_column - kit.mark_width, base, amber(VALUE));
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

/// `s` set with even-width figures: each digit, and each space (the values are padded
/// with spaces to their usual width), takes the room of the font's widest digit, its glyph
/// centred in it. Font 7's digits are not all as wide (its `1` is narrower), so a value
/// set as the font sets it would shift as its digits change [design: issue #1868].
fn tabular(font: &Font, s: &str) -> Line {
    let cell = (b'0'..=b'9').map(|c| font.glyph(c).advance()).fold(0.0, f32::max);
    let mut line = Line::default();
    for c in s.chars() {
        let code = code_of(c);
        let advance = font.glyph(code).advance();
        if c.is_ascii_digit() || c == ' ' {
            line.glyphs.push(Placed { code, x: line.width + (cell - advance) / 2.0 });
            line.width += cell;
        } else {
            line.glyphs.push(Placed { code, x: line.width });
            line.width += advance;
        }
    }
    line
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A font whose `1` is narrower than its other digits, as font 7's.
    fn uneven() -> Font {
        let mut glyphs = vec![fnv_font::Glyph::default(); 256];
        for c in b'0'..=b'9' {
            glyphs[c as usize] = fnv_font::Glyph { width: 10.0, height: 12.0, before: 1.0, after: 1.0, ..Default::default() };
        }
        glyphs[b'1' as usize].width = 4.0;
        glyphs[b' ' as usize].after = 5.0;
        glyphs[b'.' as usize] = fnv_font::Glyph { width: 3.0, height: 3.0, before: 1.0, after: 1.0, ..Default::default() };
        glyphs[b'm' as usize] = fnv_font::Glyph { width: 11.0, height: 8.0, before: 1.0, after: 1.0, ..Default::default() };
        Font { size: 20.0, texture: String::new(), glyphs }
    }

    #[test]
    fn values_of_one_pattern_are_as_wide_whatever_their_digits() {
        let f = uneven();
        let a = tabular(&f, " 90  11.1 ms");
        let b = tabular(&f, " 88  20.4 ms");
        assert_eq!(a.width, b.width);
        // The decimal point and the unit stand at the same place in both.
        for (ga, gb) in a.glyphs.iter().zip(&b.glyphs).filter(|(g, _)| g.code == b'.' || g.code == b'm') {
            assert_eq!(ga.x, gb.x);
        }
    }

    #[test]
    fn a_narrow_digit_is_centred_in_its_cell() {
        let f = uneven();
        let line = tabular(&f, "71");
        // Cells of 12: the 1 (advance 6) starts 3 into the second cell.
        assert_eq!(line.width, 24.0);
        assert_eq!(line.glyphs[1].x, 12.0 + 3.0);
    }
}
