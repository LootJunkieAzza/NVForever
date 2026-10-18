//! The lockpicking help message (MESG `HelpLockpicking`, "Message Box" and "Auto Display",
//! `fnv_game::lockpick::HELP_MESSAGE`) as the original shows it the first time the menu
//! opens: in the tutorial box of `menus\tutorial_menu.xml` over the menu [observed: the
//! owner's screenshot; documented: the menu file], one page, closed with E.
//!
//! The box [documented: tutorial_menu.xml, unless tagged otherwise; lengths in the
//! interface's units, 960 to the view's height]:
//!
//! - A background of 0.8 times the screen, centred, black at the globals' fill alpha 32.
//! - In it, 48 in from the sides and 96 from the top and bottom: a short line (50) at
//!   each top corner with a line fading down (60) under its outer end, the title in font 6
//!   10 after the left one (centred on the line, 5 lower), and the line going on from 10
//!   after the title to the right corner's (no page number: one page).
//! - The text in the default font, 5 in from the title's left, 21 under the corner lines'
//!   ends (their 60), wrapped to the box less the buttons' column. A text longer than the
//!   box scrolls (wheel, Up/Down, Page Up/Down, Home/End, the bar held) with a scroll bar
//!   beside it [design: the owner's request; the original pages with Next/Previous].
//! - At the bottom the same corners fading up, after the left one the VDSG symbol (64
//!   high, its file's first 36 left out) and "VDSG MANUAL", then the line to the right
//!   corner; the Close button right-aligned 81 from the box's right, its foot on the
//!   right corner's fading line.
//!
//! The texts the executable supplies are written as the screenshot shows them: "VDSG
//! MANUAL" (`&-sVDSGManual;`), "Close" (`&-sCloseButton;`) and "E)" after it [observed].
//! The message's own text names keys by the executable's strings (`&-sUActnForward;`,
//! `&sKBF;`); [`expand_keys`] writes the key bound to each, "WSAD" and "F" with the
//! game's default keys as the screenshot has them [observed].

use super::screen::{Ctx, FontKind, LockButton, ParagraphLine, BUTTON_DIM};
use super::{Help, LockpickInput};
use crate::bindings::{Bindings, Control, GAME_CONTROLS};
use crate::interface::{amber, at, u};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;

const BOX_PART: f32 = 0.8;
const SIDE: f32 = 48.0;
const TOP: f32 = 96.0;
const CORNER: f32 = 50.0;
const FADE: f32 = 60.0;
const BUFFER: f32 = 10.0;
const THICK: f32 = 2.0;
/// The text's room: from the title's left plus 5 to the Next/Previous buttons' column
/// less 64. The column's width is not in the file [hypothesis: 120, which wraps the text
/// where the screenshot does: between 968 and 1016 wide in a 16:9 view].
const TEXT_IN: f32 = 5.0;
/// The text's top under the corners' fading lines: the file's 21 ("font adjustment") and
/// 21 more [hypothesis: fitted to the screenshot's first line], its lines 31 apart (the
/// font's size, no leading [observed: screenshot]).
const TEXT_DOWN: f32 = 42.0;
const BUTTONS_FROM_RIGHT: f32 = 81.0;
const BUTTON_COLUMN: f32 = 120.0;
const BEFORE_BUTTONS: f32 = 64.0;
/// The VDSG symbol: 64 high, drawn from 36 short of where it is placed [documented:
/// `_dead_space`]; its file is taken as 128 wide [hypothesis: puts "VDSG MANUAL" where the
/// screenshot has it].
const SYMBOL: &str = "interface\\loading\\glow_loading_vdsgsymbol.dds";
const SYMBOL_H: f32 = 64.0;
const SYMBOL_FILE_W: f32 = 128.0;
const DEAD_SPACE: f32 = 36.0;
const VDSG_DOWN: f32 = 7.0;
/// A text box's room above and below its text [documented: text_box.xml `_verbuf` 20].
const VERBUF: f32 = 20.0;
/// Close stands this much lower than its box's foot on the fading line would put it
/// [hypothesis: fitted to the screenshot].
const CLOSE_LOWER: f32 = 20.0;
const HINT_GAP: f32 = 16.0;
/// The globals' background fill alpha [documented: menus\globals.xml].
const FILL_ALPHA: f32 = 32.0 / 255.0;
/// The scroll bar [design]: this far right of the text's room, this wide; the text's
/// window reaches this far past the room so no glyph's edge is cut.
const BAR_GAP: f32 = 24.0;
const BAR_W: f32 = 6.0;
const CLIP_SPARE: f32 = 10.0;
/// Lines a wheel notch scrolls, and the pixels of a touchpad's scroll that make a line
/// [design].
const WHEEL_LINES: f32 = 3.0;
const PIXELS_PER_LINE: f32 = 20.0;

/// The help text's lines in their window: the first shown, how many there are and fit.
#[derive(Component)]
pub struct HelpScroll {
    first: usize,
    lines: usize,
    shown: usize,
    pitch: f32,
    /// The window's height, the bar's.
    room: f32,
}

/// The scroll bar's thumb (`top`: the bar's), and the strip along the bar that is
/// clicked or dragged.
#[derive(Component)]
pub struct HelpThumb {
    top: f32,
}
#[derive(Component)]
pub struct HelpTrack;

/// Scrolls the help text: the wheel, Up/Down a line, Page Up/Down a window less a line,
/// Home/End, or the bar held where the pointer is.
pub fn scroll(
    keys: Res<ButtonInput<KeyCode>>,
    wheel: Option<Res<AccumulatedMouseScroll>>,
    mut texts: Query<(&mut Node, &mut HelpScroll)>,
    mut lines: Query<(&mut Visibility, &ParagraphLine)>,
    mut thumbs: Query<(&mut Node, &HelpThumb), Without<HelpScroll>>,
    tracks: Query<(&Interaction, &RelativeCursorPosition), With<HelpTrack>>,
    mut spill: Local<f32>,
) {
    let Ok((mut node, mut s)) = texts.single_mut() else {
        *spill = 0.0;
        return;
    };
    let last = s.lines.saturating_sub(s.shown);
    let page = s.shown.saturating_sub(1).max(1) as f32;
    let mut first = s.first as f32;
    if let Some(w) = wheel {
        *spill -= match w.unit {
            MouseScrollUnit::Line => w.delta.y * WHEEL_LINES,
            MouseScrollUnit::Pixel => w.delta.y / PIXELS_PER_LINE,
        };
        first += spill.trunc();
        *spill = spill.fract();
    }
    let steps = [(KeyCode::ArrowDown, 1.0), (KeyCode::ArrowUp, -1.0), (KeyCode::PageDown, page), (KeyCode::PageUp, -page)];
    first += steps.iter().filter(|(k, _)| keys.just_pressed(*k)).map(|(_, d)| d).sum::<f32>();
    if keys.just_pressed(KeyCode::Home) {
        first = 0.0;
    }
    if keys.just_pressed(KeyCode::End) {
        first = last as f32;
    }
    // The bar held: the thumb's middle goes to the pointer.
    for (i, at) in &tracks {
        if let (Interaction::Pressed, Some(p)) = (i, at.normalized) {
            first = ((p.y + 0.5) * s.lines as f32 - s.shown as f32 / 2.0).round();
        }
    }
    let first = (first.max(0.0) as usize).min(last);
    for (mut v, line) in &mut lines {
        v.set_if_neq(if (first..first + s.shown).contains(&line.0) { Visibility::Inherited } else { Visibility::Hidden });
    }
    if first == s.first {
        return;
    }
    s.first = first;
    node.top = u(-(first as f32) * s.pitch);
    let top = s.room * first as f32 / s.lines as f32;
    for (mut t, thumb) in &mut thumbs {
        t.top = u(thumb.top + top);
    }
}

/// The label of a key as the help text shows it: a letter or digit as itself, the
/// modifiers and arrows in words, else the key's name [observed: "WSAD", "F"; design:
/// other keys, the words the Controls page's INI values use].
pub fn key_label(k: KeyCode) -> String {
    let words = match k {
        KeyCode::ShiftLeft => "Left Shift",
        KeyCode::ShiftRight => "Right Shift",
        KeyCode::ControlLeft => "Left Ctrl",
        KeyCode::ControlRight => "Right Ctrl",
        KeyCode::AltLeft => "Left Alt",
        KeyCode::AltRight => "Right Alt",
        KeyCode::CapsLock => "Caps Lock",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::PageUp => "Page Up",
        KeyCode::PageDown => "Page Down",
        _ => "",
    };
    if !words.is_empty() {
        return words.into();
    }
    let name = format!("{k:?}");
    name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).map_or(name.clone(), str::to_string)
}

/// The label of a mouse button [design: the words the Controls page's INI values use].
fn mouse_label(m: MouseButton) -> String {
    match m {
        MouseButton::Left => "Left Mouse".into(),
        MouseButton::Right => "Right Mouse".into(),
        MouseButton::Middle => "Middle Mouse".into(),
        MouseButton::Back => "Mouse 4".into(),
        MouseButton::Forward => "Mouse 5".into(),
        MouseButton::Other(n) => format!("Mouse {}", n + 1),
    }
}

/// The help text with the executable's key strings replaced by the keys bound now:
/// `&-sUActn<control>;` and `&sUActn<control>;` by the control's key (its mouse button
/// when it has no key), `&sKB<key>;` by the key's name and the mouse strings by the
/// button's. Anything else between `&` and `;` is kept as it is.
///
/// The control strings the game's texts use [observed: every `&…;` in FalloutNV.esm,
/// 2026-10-16]: Forward, Back, Sldleft, Sldright, Use, Activate, Block, Rdyitem, Crouch,
/// Run, ToggleRun, Jump, Togglepov, Menumode, Vats, Grab. Those that are not the INI's
/// names are mapped here: Sldleft, Sldright, Rdyitem, Crouch (the sneak tutorial: "To
/// sneak or crouch, &sUActnCrouch;.") and ToggleRun [hypothesis: Always Run, the INI's
/// run toggle; its tutorial reads "To toggle between run and walk, &sUActnToggleRun;."].
/// The mouse strings are `&sMouseLeftButton;`, `&sMouseRightButton;` (the VATS help) and
/// `&sDeviceMouse;` ("Look around"); their words in the executable are not known
/// [hypothesis: "Left Mouse", "Right Mouse", "Mouse"]. The gamepad strings (`&sXB…;`)
/// are in the gamepad copies of the messages, which are not shown.
pub fn expand_keys(text: &str, bindings: &Bindings) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let name = &tail[1..end];
        let action = name.strip_prefix('-').unwrap_or(name).strip_prefix("sUActn").map(str::to_ascii_lowercase);
        let control = match action.as_deref() {
            Some("forward") => Some(Control::Forward),
            Some("back") => Some(Control::Back),
            Some("sldleft") => Some(Control::SlideLeft),
            Some("sldright") => Some(Control::SlideRight),
            Some("rdyitem") => Some(Control::ReadyItem),
            Some("crouch") => Some(Control::CrouchSneak),
            Some("togglerun") => Some(Control::AlwaysRun),
            // Any other control by its INI name without spaces and slashes (Vats,
            // Menumode, Togglepov…).
            Some(other) => GAME_CONTROLS.iter().find(|(_, n, _)| n.replace([' ', '/'], "").eq_ignore_ascii_case(other)).map(|(c, _, _)| *c),
            None => None,
        };
        let mouse = match name {
            "sMouseLeftButton" => Some("Left Mouse"),
            "sMouseRightButton" => Some("Right Mouse"),
            "sDeviceMouse" => Some("Mouse"),
            _ => None,
        };
        match (control, name.strip_prefix("sKB"), mouse) {
            (Some(c), _, _) => {
                let b = bindings.get(c);
                match (b.key, b.mouse) {
                    (Some(k), _) => out.push_str(&key_label(k)),
                    (None, Some(m)) => out.push_str(&mouse_label(m)),
                    (None, None) => out.push_str(Bindings::name(c)),
                }
            }
            (None, Some(key), _) if !key.is_empty() => out.push_str(key),
            (None, _, Some(m)) => out.push_str(m),
            _ => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The box, over the whole view; `view_w`: the view's width in interface units.
pub fn spawn(r: &mut ChildSpawnerCommands, c: &mut Ctx, help: &Help, view_w: f32) {
    let w = view_w * BOX_PART;
    let h = 960.0 * BOX_PART;
    let bright = amber(255.0);
    let bg = Node {
        position_type: PositionType::Absolute,
        left: u((view_w - w) / 2.0),
        top: u((960.0 - h) / 2.0),
        width: u(w),
        height: u(h),
        ..default()
    };
    r.spawn((bg, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, FILL_ALPHA)))).with_children(|b| {
        let solid = "interface\\shared\\line\\solid.dds";
        let down = "interface\\shared\\line\\fade_to_bottom.dds";
        let up = "interface\\shared\\line\\fade_to_top.dds";
        // The top: corners, title, the line on to the right corner.
        c.image(b, down, at(SIDE, TOP, THICK, FADE), bright);
        c.image(b, solid, at(SIDE, TOP, CORNER, THICK), bright);
        let title_x = SIDE + CORNER + BUFFER;
        let title_h = c.font_size(FontKind::Title);
        let title_w = c.text_in(b, FontKind::Title, &help.title, title_x, TOP + THICK / 2.0 - title_h / 2.0 + 5.0, false, bright);
        let after = title_x + title_w + BUFFER;
        let right_corner = w - SIDE - CORNER;
        c.image(b, solid, at(after, TOP, (right_corner - after).max(0.0), THICK), bright);
        c.image(b, solid, at(right_corner, TOP, CORNER, THICK), bright);
        c.image(b, down, at(w - SIDE, TOP, THICK, FADE), bright);
        // The text, in a window of whole lines down to the bottom corners' fading lines;
        // a longer text scrolls, a bar beside it showing where [design: the owner's
        // request, 2026-10-18; the original pages long texts with Next/Previous].
        let bottom = h - TOP;
        let text_x = title_x + TEXT_IN;
        let text_top = TOP + FADE + TEXT_DOWN;
        let wrap = (w - BUTTONS_FROM_RIGHT - BUTTON_COLUMN - BEFORE_BUTTONS - title_x - TEXT_IN).max(100.0);
        let pitch = c.font_size(FontKind::Body);
        let shown = (((bottom - FADE - text_top) / pitch).floor() as usize).max(1);
        // The window runs from the first line's tallest glyph to the last one's deepest,
        // so whole lines show; the lines on either side, whose glyphs may reach into it,
        // are hidden.
        let (rise, fall) = c.ink(FontKind::Body);
        let window_top = text_top - rise;
        let room = rise + (shown - 1) as f32 * pitch + fall.max(pitch - rise);
        let mut lines = 0;
        b.spawn(Node { overflow: Overflow::clip(), ..at(text_x, window_top, wrap + CLIP_SPARE, room) }).with_children(|window| {
            window
                .spawn(at(0.0, 0.0, wrap + CLIP_SPARE, room))
                .with_children(|t| lines = c.paragraph(t, FontKind::Body, &help.text, 0.0, rise, wrap, 0.0, bright))
                .insert(HelpScroll { first: 0, lines, shown, pitch, room });
        });
        if lines > shown {
            let bar_x = text_x + wrap + BAR_GAP;
            b.spawn((at(bar_x + (BAR_W - THICK) / 2.0, window_top, THICK, room), BackgroundColor(amber(BUTTON_DIM))));
            b.spawn((HelpThumb { top: window_top }, at(bar_x, window_top, BAR_W, room * shown as f32 / lines as f32), BackgroundColor(bright)));
            b.spawn((HelpTrack, Interaction::default(), RelativeCursorPosition::default(), at(bar_x - BAR_GAP / 2.0, window_top, BAR_W + BAR_GAP, room)));
        }
        // The bottom: corners, the VDSG symbol and words, the line on.
        c.image(b, up, at(SIDE, bottom - FADE, THICK, FADE), bright);
        c.image(b, solid, at(SIDE, bottom - THICK, CORNER, THICK), bright);
        let symbol_x = SIDE + CORNER + BUFFER - DEAD_SPACE;
        let symbol_y = bottom - THICK + (THICK - SYMBOL_H) / 2.0;
        c.image(b, SYMBOL, at(symbol_x, symbol_y, SYMBOL_FILE_W, SYMBOL_H), bright);
        let vdsg_x = symbol_x + SYMBOL_FILE_W - DEAD_SPACE + BUFFER;
        let body_h = c.font_size(FontKind::Body);
        let vdsg_w = c.text_in(b, FontKind::Body, "VDSG MANUAL", vdsg_x, symbol_y + (SYMBOL_H - body_h) / 2.0 + VDSG_DOWN, false, bright);
        let line_x = vdsg_x + vdsg_w + BUFFER;
        c.image(b, solid, at(line_x, bottom - THICK, (w - SIDE - line_x).max(0.0), THICK), bright);
        c.image(b, up, at(w - SIDE, bottom - FADE, THICK, FADE), bright);
        // Close, its foot on the right corner's fading line.
        let dim = amber(BUTTON_DIM);
        let right = w - BUTTONS_FROM_RIGHT;
        let top = bottom - FADE - body_h - 2.0 * VERBUF + CLOSE_LOWER;
        let cw = c.text_in(b, FontKind::Body, "Close", right, top + VERBUF, true, dim);
        c.text_in(b, FontKind::Body, "E)", right + HINT_GAP, top + VERBUF, false, dim);
        b.spawn((Button, LockButton(LockpickInput::CloseHelp), at(right - cw - 10.0, top, cw + 2.0 * HINT_GAP + 40.0, body_h + 2.0 * VERBUF)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_names_the_keys_bound() {
        let b = Bindings::game_defaults();
        let text = "&-sUActnForward;&-sUActnBack;&-sUActnSldleft;&-sUActnSldright;: Apply torque\r\n&sKBF;: Force Lock &unknown; & so";
        assert_eq!(expand_keys(text, &b), "WSAD: Apply torque\r\nF: Force Lock &unknown; & so");
        assert_eq!(expand_keys("Center VATS &sUActnVats;", &b), "Center VATS V");
        assert_eq!(expand_keys("To sneak or crouch, &sUActnCrouch;.", &b), "To sneak or crouch, Left Ctrl.");
        assert_eq!(expand_keys("&-sUActnRdyitem;: Reload", &b), "R: Reload");
        assert_eq!(expand_keys("&-sUActnMenumode;&-sUActnTogglepov;", &b), "TabF");
        assert_eq!(expand_keys("&-sUActnUse;: Attack", &b), "Left Mouse: Attack");
        assert_eq!(expand_keys("&sMouseLeftButton;, &sDeviceMouse;", &b), "Left Mouse, Mouse");
        assert_eq!(key_label(KeyCode::Digit4), "4");
        assert_eq!(key_label(KeyCode::Space), "Space");
    }
}
