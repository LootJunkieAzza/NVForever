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
//!   ends (their 60), wrapped to the box less the buttons' column.
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

use super::screen::{Ctx, FontKind, LockButton, BUTTON_DIM};
use super::{Help, LockpickInput};
use crate::bindings::{Bindings, Control, GAME_CONTROLS};
use crate::interface::{amber, at, u};
use bevy::prelude::*;

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

/// The label of a key as the help text shows it: a letter or digit as itself, else the
/// key's name [observed: "WSAD", "F"; design: other keys].
pub fn key_label(k: KeyCode) -> String {
    let name = format!("{k:?}");
    name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).map_or(name.clone(), str::to_string)
}

/// The help text with the executable's key strings replaced by the keys bound now:
/// `&-sUActn<control>;` and `&sUActn<control>;` (Forward, Back, Sldleft, Sldright, and
/// any control by its INI name, such as Vats) by the control's key and
/// `&sKB<key>;` by the key's name. Anything else between `&` and `;` is kept as it is.
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
            // Any other control by its INI name without spaces and slashes, as
            // `&sUActnVats;` in the VATS hint [hypothesis: the strings follow the INI
            // names; only Vats seen, owner's playtest list 4].
            Some(other) => GAME_CONTROLS.iter().find(|(_, n, _)| n.replace([' ', '/'], "").eq_ignore_ascii_case(other)).map(|(c, _, _)| *c),
            None => None,
        };
        match (control, name.strip_prefix("sKB")) {
            (Some(c), _) => match bindings.get(c).key {
                Some(k) => out.push_str(&key_label(k)),
                None => out.push_str(Bindings::name(c)),
            },
            (None, Some(key)) if !key.is_empty() => out.push_str(key),
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
        // The text.
        let text_x = title_x + TEXT_IN;
        let wrap = (w - BUTTONS_FROM_RIGHT - BUTTON_COLUMN - BEFORE_BUTTONS - title_x - TEXT_IN).max(100.0);
        c.paragraph(b, FontKind::Body, &help.text, text_x, TOP + FADE + TEXT_DOWN, wrap, 0.0, bright);
        // The bottom: corners, the VDSG symbol and words, the line on.
        let bottom = h - TOP;
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
        assert_eq!(key_label(KeyCode::Digit4), "4");
        assert_eq!(key_label(KeyCode::Space), "Space");
    }
}
