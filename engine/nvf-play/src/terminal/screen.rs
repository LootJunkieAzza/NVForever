//! The terminal's screen drawn as the original sets it out (docs/spec-notes/hacking.md
//! §2, §7): the hacking game by `menus\hacking_menu.xml`, the terminal's own menu by
//! `menus\computers_menu.xml`, both in the game's font 5 (`fixedsys_comp_uniform_width`,
//! 17 units a character) on the Pip-Boy background, in the terminal colour.
//!
//! The original draws this into a texture shown on a monitor model
//! (`meshes\terminals\terminalinterface01.nif`, `[RenderedTerminal]` in
//! `Fallout_default.ini`), and so does NVForever: the canvas is drawn into
//! `super::TerminalImage` and the engine puts that on the model's screen (nvforever
//! `terminal_view.rs`), where the texture spans [`TEXTURE_UNITS`] of the canvas. Without
//! the model the picture is drawn flat over the darkened world [design, hacking.md §7].
//! Lengths are the interface's units (960 to the view's height): the owner's screenshots
//! show the screen at almost that scale (0.99).

use super::{Hacking, MenuView, Page, Session};
use crate::interface::{at, face, plain_font, set_line, texture, u, Face};
use bevy::prelude::*;
use fnv_font::Line;
use fnv_game::data::NoteDef;
use fnv_game::hacking::{Puzzle, Target, ROWS, ROW_CHARS};
use nvf_assets::GameData;

// ---- the canvas ------------------------------------------------------------------------

/// The screen: four by three [documented: computers_menu.xml, "screen height × 0.75" and
/// four thirds of that wide].
pub const CANVAS: Vec2 = Vec2::new(960.0, 720.0);
/// Its top, down from the view's, when drawn flat [observed: the owner's screenshots put the first row of
/// the dump 204 units under the screen's top at 0.74 pixels a unit, so the screen starts
/// 62 of 720 pixels down; `fRenderedTerminalVPos` 0.38 moves it up from the middle].
pub const CANVAS_TOP: f32 = 83.0;

/// The text and the lit cells. The menus' colour is the terminal colour
/// [documented: Fallout_default.ini `iSystemColorTerminal*` 33, 231, 121; `systemcolor`
/// `&terminal;`], but the monitor's screen light and glare make it brighter on the
/// screen: the attempt blocks measure 63, 255, 152 on the owner's screenshots
/// [observed], and that is what is drawn until the monitor is.
pub fn bright() -> Color {
    Color::srgb_u8(63, 255, 152)
}

/// The screen behind the text [observed: 16 to 23, 38 to 52, 15 to 32 in the middle of
/// the owner's screenshots; the Pip-Boy background over it gives the shading].
pub fn screen_color() -> Color {
    Color::srgb_u8(19, 45, 26)
}

/// How much of the Pip-Boy background shows over [`screen_color`] [design].
const BACKGROUND_ALPHA: f32 = 0.4;

/// Text on a lit-up cell [documented: the files' "local colour" 0, 5, 0 for it].
pub fn lit_text() -> Color {
    Color::srgb(0.0, 5.0 / 255.0, 0.0)
}

/// How much of the canvas the screen's texture spans, 0 to 1 each way, interface units
/// [observed: the screen shape's texture coordinates run to 0.767 across and 0.755 down
/// (`screen:0`); fitted to the owner's screenshot, where the dump's rows are 0.76 pixels
/// a unit apart at 720 high and its columns 0.73, the header's top 0.08 of the glass
/// down and the dump's last row's foot 0.95; the case hides the glass's edge]. The
/// canvas's right edge falls inside the glass, its bottom (where nothing is drawn) about
/// at the glass's foot.
pub const TEXTURE_UNITS: Vec2 = Vec2::new(1350.0, 960.0);

/// How dark the world is behind the monitor [design: the owner's screenshot shows the
/// room dim and blurred; the blur is not drawn], and behind the flat screen.
const SHADE: f32 = 0.4;
const SHADE_FLAT: f32 = 0.88;

// ---- the hacking menu's numbers [documented: menus\hacking_menu.xml] --------------------

/// The area everything is set in (`hacking_depth_rect`): its corner on the screen.
pub const DEPTH: Vec2 = Vec2::new(32.0, 48.0);
pub const DEPTH_SIZE: Vec2 = Vec2::new(920.0, 630.0);
/// The header above the dump.
pub const HEADER_HEIGHT: f32 = 150.0;
/// Rows of the dump and the log are this high; a character this wide.
pub const ROW: f32 = 28.0;
pub const CHAR: f32 = 17.0;
/// The second column starts here; a column is 19 characters (address, space, 12).
pub const COLUMN_2: f32 = 342.0;
pub const COLUMN_CHARS: f32 = 19.0;
/// Where a row's text starts in its row.
pub const ROW_TEXT_Y: f32 = 6.0;
/// The log starts right of the dump; its prompt 17 in.
pub const LOG_X: f32 = COLUMN_2 + COLUMN_CHARS * CHAR;
pub const PROMPT_X: f32 = 17.0;
/// The entry after the prompt's `>`.
pub const ENTRY_X: f32 = PROMPT_X + 21.0;
pub const FILE_HEIGHT: f32 = ROWS as f32 * ROW;
/// The attempt blocks and the cursor.
pub const BLOCK: f32 = 17.0;
/// The cursor blinks this often [documented: `_blink_interval` 400 ms].
pub const BLINK_SECONDS: f32 = 0.4;

/// The header's lines, down from the header's top [observed: the screenshots, 1, 32 and
/// 93 units: rows 0, 1 and 3 at 31 apart].
pub const HEADER_LINES: [f32; 3] = [0.0, 31.0, 93.0];

/// The baseline under a text's top [design: font 5's capitals stand 18 above it, its
/// tallest characters 21, in a 25-unit line].
pub const BASE: f32 = 20.0;

// ---- the terminal menu's numbers [documented: menus\computers_menu.xml] -----------------

pub const MENU_HEADER_Y: f32 = 60.0;
/// A line of font 5 [documented: the font's own line height, 25].
pub const LINE: f32 = 25.0;
pub const MENU_X: f32 = 100.0;
pub const WELCOME_WRAP: f32 = 600.0;
pub const LIST_WIDTH: f32 = 700.0;
pub const LIST_ROWS: usize = 12;
/// A row of the list [hypothesis: the shared list template's height is not in the file;
/// the screenshot shows one item only].
pub const LIST_ROW: f32 = 30.0;
pub const NOTE_LINES: usize = 16;
/// The result's prompt: 30 above the bottom; its text 34 right of the prompt.
pub const RESULT_GAP: f32 = 30.0;
pub const RESULT_X: f32 = 34.0;

/// The words on the screens that are not in the plugins: exe defaults read off the
/// owner's screenshots [observed], or NVForever's until the original's are seen
/// [hypothesis, hacking.md §8].
pub mod words {
    pub const HACK_TITLE: &str = "ROBCO INDUSTRIES (TM) TERMLINK PROTOCOL";
    pub const HACK_PROMPT: &str = "ENTER PASSWORD NOW";
    /// [hypothesis] The warning on the last attempt.
    pub const HACK_WARNING: &str = "!!! WARNING: LOCKOUT IMMINENT !!!";
    /// `sHackingLockout3` and `sHackingLockout4` [hypothesis: their texts].
    pub const LOCKOUT_1: &str = "TERMINAL LOCKED";
    pub const LOCKOUT_2: &str = "PLEASE CONTACT AN ADMINISTRATOR";
    pub const MENU_TITLE: &str = "ROBCO INDUSTRIES UNIFIED OPERATING SYSTEM";
    pub const MENU_COPYRIGHT: &str = "COPYRIGHT 2075-2077 ROBCO INDUSTRIES";

    pub fn attempts(n: u8) -> String {
        format!("{n} ATTEMPT(S) LEFT:")
    }
}

/// The left edge and the top of a dump character, in the depth area.
pub fn cell_at(index: usize) -> Vec2 {
    let column = index / (ROWS * ROW_CHARS);
    let row = (index / ROW_CHARS) % ROWS;
    let col = index % ROW_CHARS;
    Vec2::new(column as f32 * COLUMN_2 + (7 + col) as f32 * CHAR, HEADER_HEIGHT + row as f32 * ROW)
}

/// The dump character under a point of the canvas.
pub fn canvas_index(p: Vec2) -> Option<usize> {
    index_at(p - DEPTH - Vec2::new(0.0, HEADER_HEIGHT))
}

/// The dump character under a point of the dump area (`(0, 0)` its top left, which is
/// the depth area's left edge and the header's bottom).
pub fn index_at(p: Vec2) -> Option<usize> {
    if p.y < 0.0 || p.y >= FILE_HEIGHT {
        return None;
    }
    let row = (p.y / ROW) as usize;
    for column in 0..2 {
        let x = p.x - column as f32 * COLUMN_2 - 7.0 * CHAR;
        if (0.0..ROW_CHARS as f32 * CHAR).contains(&x) {
            return Some(Puzzle::index(column, row, (x / CHAR) as usize));
        }
    }
    None
}

/// The log's lines shown, newest last: as many as stand above the prompt, a blank row
/// between [documented: entries stand 3 + n rows up from the bottom and are hidden once
/// above the top; observed: one blank row on the screenshots].
pub fn shown_log(log: &[String]) -> &[String] {
    let room = ROWS - 2;
    &log[log.len().saturating_sub(room)..]
}

// ---- drawing -----------------------------------------------------------------------------

/// What the screens are drawn with.
pub struct Faces {
    pub text: Face,
    pub background: Option<Handle<Image>>,
}

const FONT: &str = "textures\\fonts\\fixedsys_comp_uniform_width.fnt";
const BACKGROUND: &str = "textures\\interface\\shared\\background\\pipboy.dds";

pub fn load_faces(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Faces {
    Faces { text: face(FONT, plain_font(25.0, CHAR), data, images, bc), background: data.and_then(|d| texture(BACKGROUND, d, images, bc)).map(|t| t.0) }
}

#[derive(Component)]
pub struct ScreenRoot;

/// What lies under the screen: the darkened world and the monitor, or the flat picture.
#[derive(Component)]
pub struct Backdrop {
    pub flat: bool,
}

/// The flat picture of the screen, for the pointer.
#[derive(Component)]
pub struct FlatScreen;

/// What [`spawn_backdrop`] draws over the darkened world.
pub enum Under {
    /// The screen's picture (`super::TerminalImage`), flat where the original's screen is.
    Flat(Handle<Image>),
    /// The monitor's picture (`super::MonitorView`), over the whole window.
    Monitor(Handle<Image>),
}

pub fn spawn_backdrop(commands: &mut Commands, under: Under) {
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let (flat, shade) = match under {
        Under::Flat(_) => (true, SHADE_FLAT),
        Under::Monitor(_) => (false, SHADE),
    };
    // The flat picture's top is [`CANVAS_TOP`] down; the monitor's picture fills the window.
    let top = if flat { u(CANVAS_TOP) } else { Val::ZERO };
    let root = Node { justify_content: JustifyContent::Center, align_items: AlignItems::FlexStart, padding: UiRect::top(top), ..full.clone() };
    commands.spawn((Backdrop { flat }, Name::new("Terminal backdrop"), root, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, shade)), GlobalZIndex(106))).with_children(|r| match under {
        Under::Flat(image) => {
            let crop = Rect::new(0.0, 0.0, CANVAS.x, CANVAS.y);
            r.spawn((FlatScreen, Node { width: u(CANVAS.x), height: u(CANVAS.y), ..default() }, ImageNode { rect: Some(crop), ..ImageNode::new(image) }));
        }
        Under::Monitor(image) => {
            r.spawn((full, ImageNode::new(image)));
        }
    });
}

/// The blinking cursor block.
#[derive(Component)]
pub struct Blink;

/// One line of text from its top.
fn text(p: &mut ChildSpawnerCommands, face: &Face, s: &str, x: f32, top: f32, color: Color) -> f32 {
    let Some(line) = face.font.lines(s, None).into_iter().next() else { return 0.0 };
    set_line(p, face, &line, x, top + BASE, color);
    line.width
}

/// A line of text whose characters have colours of their own, by the column each stands
/// in (every character of font 5 is [`CHAR`] wide): the lit cells of the dump.
fn text_colored(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, top: f32, color: impl Fn(Option<usize>) -> Color) {
    let base = top + BASE;
    let Some((image, size)) = &face.image else {
        set_line(p, face, line, x, base, color(None));
        return;
    };
    for placed in &line.glyphs {
        let k = (placed.x / CHAR).round() as usize;
        let g = face.font.glyph(placed.code);
        if !g.drawn() {
            continue;
        }
        let rect = Rect::new(g.uv[0] * size.x, g.uv[1] * size.y, g.uv[2] * size.x, g.uv[3] * size.y);
        p.spawn((at(x + placed.x + g.before, base - g.ascent, g.width, g.height), ImageNode { rect: Some(rect), color: color(Some(k)), ..ImageNode::new(image.clone()) }));
    }
}

/// Text centred across the screen.
fn centred(p: &mut ChildSpawnerCommands, face: &Face, s: &str, width: f32, top: f32, color: Color) {
    let w = face.font.width(s);
    text(p, face, s, (width - w) / 2.0, top, color);
}

/// Draw the screen into the picture `camera` renders; returns the menu's rows on the
/// canvas, for the pointer.
pub fn spawn(commands: &mut Commands, camera: Entity, faces: &Faces, session: &Session, terminals: &fnv_game::GameData, cursor_on: bool) -> Vec<(usize, Rect)> {
    let mut rows = Vec::new();
    let root = Node { overflow: Overflow::clip(), ..at(0.0, 0.0, CANVAS.x, CANVAS.y) };
    commands.spawn((ScreenRoot, root, UiTargetCamera(camera), Name::new("Terminal screen"))).with_children(|c| {
        // [documented] The Pip-Boy background (at brightness 90 in the file; here over
        // the screen's measured colour).
        c.spawn((at(0.0, 0.0, CANVAS.x, CANVAS.y), BackgroundColor(screen_color())));
        if let Some(image) = &faces.background {
            c.spawn((at(0.0, 0.0, CANVAS.x, CANVAS.y), ImageNode { color: bright().with_alpha(BACKGROUND_ALPHA), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }));
        }
        match &session.page {
            Page::Hacking(h) => hacking_page(c, &faces.text, h, cursor_on),
            Page::LockedOut => lockout_page(c, &faces.text),
            Page::Menu(m) => menu_page(c, &faces.text, m, terminals, cursor_on, &mut rows),
        }
    });
    rows
}

fn hacking_page(c: &mut ChildSpawnerCommands, face: &Face, h: &Hacking, cursor_on: bool) {
    let p = &h.puzzle;
    let bright = bright();
    c.spawn(at(DEPTH.x, DEPTH.y, DEPTH_SIZE.x, DEPTH_SIZE.y)).with_children(|d| {
        // The header.
        text(d, face, words::HACK_TITLE, 0.0, HEADER_LINES[0], bright);
        let prompt = if p.attempts == 1 { words::HACK_WARNING } else { words::HACK_PROMPT };
        text(d, face, prompt, 0.0, HEADER_LINES[1], bright);
        let w = text(d, face, &words::attempts(p.attempts), 0.0, HEADER_LINES[2], bright);
        // [documented] Blocks one apart, starting one block after the text.
        for i in 0..p.attempts {
            let x = w + (2 * i as usize + 1) as f32 * BLOCK;
            d.spawn((at(x, HEADER_LINES[2] + BASE - 18.0, BLOCK, BLOCK), BackgroundColor(bright)));
        }
        // The dump: the lit span under the cursor, while the game goes on.
        let lit = (h.ended.is_none() && !h.hidden).then(|| p.span(p.target(h.cursor)));
        let is_lit = |i: usize| lit.as_ref().is_some_and(|r| r.contains(&i));
        for column in 0..2 {
            for row in 0..ROWS {
                let x = column as f32 * COLUMN_2;
                let top = HEADER_HEIGHT + row as f32 * ROW + ROW_TEXT_Y;
                let first = Puzzle::index(column, row, 0);
                let s = format!("0x{:04X} {}", p.row_address(column, row), p.row_text(column, row));
                for k in 0..ROW_CHARS {
                    if is_lit(first + k) {
                        let cell = cell_at(first + k);
                        d.spawn((at(cell.x, cell.y + ROW_TEXT_Y - 2.0, CHAR, ROW - 4.0), BackgroundColor(bright)));
                    }
                }
                if let Some(line) = face.font.lines(&s, None).into_iter().next() {
                    text_colored(d, face, &line, x, top, |k| if k.is_some_and(|k| k >= 7 && is_lit(first + k - 7)) { lit_text() } else { bright });
                }
            }
        }
        // The log, the prompt and what the cursor would enter.
        d.spawn(at(LOG_X, HEADER_HEIGHT, DEPTH_SIZE.x - LOG_X, FILE_HEIGHT)).with_children(|l| {
            let shown = shown_log(&p.log);
            for (i, entry) in shown.iter().rev().enumerate() {
                let top = FILE_HEIGHT - ROW * (3 + i) as f32 + ROW_TEXT_Y + 1.0;
                text(l, face, entry, PROMPT_X, top, bright);
            }
            let top = FILE_HEIGHT - ROW + ROW_TEXT_Y + 1.0;
            text(l, face, ">", PROMPT_X, top, bright);
            let entry = if lit.is_some() { p.text(p.target(h.cursor)) } else { String::new() };
            let w = text(l, face, &entry, ENTRY_X, top, bright);
            if entry.is_empty() {
                l.spawn((Blink, at(ENTRY_X + w, top + BASE - 18.0, BLOCK, BLOCK), BackgroundColor(bright), if cursor_on { Visibility::Inherited } else { Visibility::Hidden }));
            }
        });
    });
}

fn lockout_page(c: &mut ChildSpawnerCommands, face: &Face) {
    let bright = bright();
    c.spawn(at(DEPTH.x, DEPTH.y, DEPTH_SIZE.x, DEPTH_SIZE.y)).with_children(|d| {
        // [documented] One line above the middle, one below.
        centred(d, face, words::LOCKOUT_1, DEPTH_SIZE.x, DEPTH_SIZE.y / 2.0 - LINE, bright);
        centred(d, face, words::LOCKOUT_2, DEPTH_SIZE.x, DEPTH_SIZE.y / 2.0 + LINE, bright);
    });
}

/// The text a note shows on the screen: a text note's text, else its name.
pub fn note_text(n: Option<&NoteDef>) -> String {
    match n {
        Some(NoteDef { text: Some(t), .. }) => t.replace('\r', ""),
        Some(n) => n.name.clone().unwrap_or_default(),
        None => String::new(),
    }
}

fn menu_page(c: &mut ChildSpawnerCommands, face: &Face, m: &MenuView, data: &fnv_game::GameData, cursor_on: bool, rows: &mut Vec<(usize, Rect)>) {
    let bright = bright();
    let Some(t) = data.terminals.get(&m.terminal) else { return };
    let mut y = MENU_HEADER_Y;
    for line in [words::MENU_TITLE, words::MENU_COPYRIGHT, &fnv_game::terminal::server_text(m.server)] {
        centred(c, face, line, CANVAS.x, y, bright);
        y += LINE;
    }
    y += 20.0;
    // The welcome text (the top terminal's, or the sub menu's own when it has one).
    let welcome = if t.welcome.trim().is_empty() { &m.welcome } else { &t.welcome };
    let mut widest: f32 = 0.0;
    for para in welcome.replace('\r', "").split('\n') {
        for line in face.font.lines(para, Some(WELCOME_WRAP)) {
            widest = widest.max(line.width);
            set_line(c, face, &line, MENU_X, y + BASE, bright);
            y += LINE;
        }
        if para.trim().is_empty() {
            y += LINE;
        }
    }
    // [documented] The separator 10 under the welcome text, as long as it is wide.
    y += 10.0;
    c.spawn((at(MENU_X, y, widest.max(1.0), 2.0), BackgroundColor(bright)));
    y += 12.0;
    match m.note {
        Some(n) => {
            let text = note_text(data.notes.get(&n));
            let mut lines = Vec::new();
            for para in text.split('\n') {
                let l = face.font.lines(para, Some(WELCOME_WRAP));
                if l.is_empty() {
                    lines.push(None);
                }
                lines.extend(l.into_iter().map(Some));
            }
            // [documented] At most 16 lines at a time; here no more than stand above the
            // result line [design], from the line scrolled to.
            let room = (((CANVAS.y - LINE - RESULT_GAP) - (y + 10.0)) / LINE).floor().max(1.0) as usize;
            let first = m.scroll.min(lines.len().saturating_sub(room.min(NOTE_LINES)));
            for (i, line) in lines.into_iter().skip(first).take(NOTE_LINES.min(room)).enumerate() {
                if let Some(line) = line {
                    set_line(c, face, &line, MENU_X, y + 10.0 + i as f32 * LINE + BASE, bright);
                }
            }
        }
        None => {
            let first = m.selected.saturating_sub(LIST_ROWS - 1);
            for (row, &item) in m.items.iter().enumerate().skip(first).take(LIST_ROWS) {
                let Some(it) = t.items.get(item) else { continue };
                let top = y + (row - first) as f32 * LIST_ROW;
                let lit = row == m.selected && !m.hidden;
                rows.push((row, Rect::new(MENU_X, top, MENU_X + LIST_WIDTH, top + LIST_ROW)));
                c.spawn(at(MENU_X, top, LIST_WIDTH, LIST_ROW)).with_children(|b| {
                    let s = format!("> {}", it.text);
                    let w = face.font.width(&s);
                    if lit {
                        // [documented] A filled box (224 of 255) behind the chosen row.
                        b.spawn((at(0.0, 2.0, w + CHAR, LIST_ROW - 4.0), BackgroundColor(bright.with_alpha(224.0 / 255.0))));
                    }
                    text(b, face, &s, 0.0, (LIST_ROW - LINE) / 2.0, if lit { lit_text() } else { bright });
                });
            }
        }
    }
    // The result line at the bottom: the prompt, the last result, the cursor.
    let top = CANVAS.y - LINE - RESULT_GAP;
    text(c, face, ">", MENU_X, top, bright);
    let w = text(c, face, &m.result, MENU_X + RESULT_X, top, bright);
    let x = MENU_X + RESULT_X + if m.result.is_empty() { 0.0 } else { w + CHAR };
    c.spawn((Blink, at(x, top + BASE - 18.0, BLOCK, BLOCK), BackgroundColor(bright), if cursor_on { Visibility::Inherited } else { Visibility::Hidden }));
}

/// Whether a row's character `index` is drawn lit (tests).
pub fn lit(p: &Puzzle, cursor: usize, index: usize) -> bool {
    let t: Target = p.target(cursor);
    p.span(t).contains(&index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_fits_the_depth_area() {
        // Two columns of 19 characters end where the log starts, 665 in.
        assert_eq!(LOG_X, 665.0);
        const { assert!(HEADER_HEIGHT + FILE_HEIGHT <= DEPTH_SIZE.y) };
        const { assert!(DEPTH.x + DEPTH_SIZE.x <= CANVAS.x && DEPTH.y + DEPTH_SIZE.y <= CANVAS.y) };
    }

    #[test]
    fn cells_and_points_agree() {
        for i in [0, 11, 12, 203, 204, 215, 407] {
            let c = cell_at(i);
            let p = Vec2::new(c.x + CHAR / 2.0, c.y - HEADER_HEIGHT + ROW / 2.0);
            assert_eq!(index_at(p), Some(i), "{i}");
        }
        // On an address or between the columns: nothing.
        assert_eq!(index_at(Vec2::new(10.0, 10.0)), None);
        assert_eq!(index_at(Vec2::new(330.0, 10.0)), None);
        assert_eq!(index_at(Vec2::new(500.0, FILE_HEIGHT + 1.0)), None);
    }

    #[test]
    fn the_log_keeps_fifteen_lines() {
        let log: Vec<String> = (0..20).map(|i| format!(">{i}")).collect();
        let shown = shown_log(&log);
        assert_eq!(shown.len(), 15);
        assert_eq!(shown.last().map(String::as_str), Some(">19"));
        assert_eq!(shown_log(&log[..3]).len(), 3);
    }
}
