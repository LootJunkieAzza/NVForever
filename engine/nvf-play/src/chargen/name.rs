//! The name menu (`menus\dialog\texteditmenu.xml`, menu 1051, opened by `GetPlayerName`
//! in the opening): a box in the middle of the screen asking for the character's name,
//! the name as typed, and Ok (docs/spec-notes/chargen.md §2).

use super::draw::{Align, Ctx, Which, BODY_BASE, BODY_H, LINE_PITCH, THICK};
use crate::interface::{amber, at, set_line, u};
use bevy::prelude::*;
use fnv_game::chargen;

// ---- the menu file's numbers [documented: texteditmenu.xml, §2] ---------------------------

/// The box: 720 × 180, centred on the screen.
pub const BOX: Vec2 = Vec2::new(720.0, 180.0);
/// The dark picture behind: 25 out on every side.
const BEHIND: f32 = 25.0;
/// The question, centred, 32 down.
const PROMPT_Y: f32 = 32.0;
/// The name wraps at 250, centred, in the middle of the box's height.
const TEXT_WRAP: f32 = 250.0;
/// Ok: right-aligned 16 in from the box's right side, its bottom 16 above the box's;
/// a button is its text and 20 (10 each side).
const OK_IN: f32 = 16.0;
const BUTTON_PAD: f32 = 10.0;
/// The top bracket's corner lines (top_bracket.xml, 50 high).
const TOP_CORNER: f32 = 50.0;

// ---- measured on the owner's screenshot [observed, §5] ----------------------------------------

/// The question (the exe's `sEnterName`, not in the masters).
pub const PROMPT: &str = "Enter character name.";
/// The Ok button's word (the exe's `sOk`).
pub const OK: &str = "Ok";
/// The question and the name are in font 1, the glowing one [observed: their letters
/// have the glow on the screenshot; hypothesis: font 1 is the default of a text the file
/// gives no font]; Ok in font 2, a text box's default [documented: text_box.xml].
///
/// How dark the box makes what is behind it: by eye, about the message box's.
const SHADE: f32 = 0.85;

/// The name menu's state.
#[derive(Debug, Clone, PartialEq)]
pub struct NameMenu {
    pub text: String,
    pub hover_ok: bool,
}

/// The Ok button.
#[derive(Component, Clone, Copy, Debug)]
pub struct OkButton;

impl NameMenu {
    pub fn open(offered: String) -> NameMenu {
        NameMenu { text: chargen::clean_name(&offered), hover_ok: false }
    }

    /// Typed text goes after the name, as far as the name may be long.
    pub fn type_text(&mut self, s: &str) -> bool {
        let next = chargen::clean_name(&format!("{}{s}", self.text));
        let changed = next != self.text;
        self.text = next;
        changed
    }

    pub fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    pub fn can_finish(&self) -> bool {
        chargen::name_acceptable(&self.text)
    }
}

/// Spawn the box under `root` (a full-screen node that centres its child).
pub fn spawn(root: &mut ChildSpawnerCommands, m: &NameMenu, c: &mut Ctx) {
    let bright = amber(255.0);
    root.spawn(Node { width: u(BOX.x), height: u(BOX.y), ..default() }).with_children(|p| {
        let behind = at(-BEHIND, -BEHIND, BOX.x + 2.0 * BEHIND, BOX.y + 2.0 * BEHIND);
        let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
        if c.image(p, "interface\\shared\\background\\solid_black.dds", behind.clone(), shade, ()).is_none() {
            p.spawn((behind, BackgroundColor(shade)));
        }
        c.top_bracket(p, 0.0, 0.0, BOX.x, TOP_CORNER, bright);
        c.bottom_bracket(p, 0.0, BOX.y - THICK, BOX.x, bright);
        c.text(p, Which::Glow, PROMPT, BOX.x / 2.0, PROMPT_Y, Align::Centre, bright);
        // The name, each line centred, the whole in the middle of the box's height.
        let lines = c.kit.glow.font.lines(&m.text, Some(TEXT_WRAP));
        let h = BODY_H + LINE_PITCH * lines.len().saturating_sub(1) as f32;
        let top = (BOX.y - h) / 2.0;
        for (k, line) in lines.iter().enumerate() {
            set_line(p, &c.kit.glow, line, (BOX.x - line.width) / 2.0, top + BODY_BASE + LINE_PITCH * k as f32, bright);
        }
        let ok_w = c.width(Which::Body, OK) + 2.0 * BUTTON_PAD;
        let ok_h = BODY_H + 2.0 * BUTTON_PAD;
        let (ok_x, ok_y) = (BOX.x - OK_IN - ok_w, BOX.y - ok_h - OK_IN);
        let color = bright.with_alpha(if m.can_finish() { 1.0 } else { 128.0 / 255.0 });
        if m.hover_ok && m.can_finish() {
            c.outline(p, ok_x, ok_y, ok_w, ok_h, amber(200.0));
        }
        c.text(p, Which::Body, OK, ok_x + ok_w - BUTTON_PAD, ok_y + BUTTON_PAD, Align::Right, color);
        p.spawn((Button, OkButton, at(ok_x, ok_y, ok_w, ok_h)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_rubbing_out() {
        let mut m = NameMenu::open("Courier".into());
        assert!(m.type_text(" Six"));
        assert_eq!(m.text, "Courier Six");
        for _ in 0..11 {
            assert!(m.backspace());
        }
        assert!(!m.backspace());
        assert!(!m.can_finish());
        assert!(m.type_text("\u{8}A\r"));
        assert_eq!(m.text, "A");
        m.type_text(&"b".repeat(60));
        assert_eq!(m.text.chars().count(), chargen::NAME_MAX_CHARS);
    }
}
