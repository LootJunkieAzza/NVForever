//! The look the main menu (`crate::main_menu`) and the pause menu (`crate::pause`) share,
//! after the original's one file for both, `menus\options\start_menu.xml` [observed: the
//! file in C:\fnv-private\menus-extract, and the owner's screenshots of the original's
//! pause menu and its settings pages, C:\fnv-private\Pause Menu]:
//!
//! - **The column of entries** at the right, right-aligned, its right edge [`RIGHT`] from
//!   the view's, one entry [`ITEM`] high [documented: `_item_height` 48], the column
//!   [`COLUMN`] wide [documented: `main_container` width 375] and centred top to bottom
//!   [documented: its `y` is half of what the screen leaves].
//! - **Text in font 7** in the interface's amber: full strength in the list that has the
//!   keys; half (alpha 127, brightness 127) in a list that has lost them, but for its
//!   chosen entry; half too for an entry that cannot be taken [documented:
//!   `ListItemText` alpha `255 - 128` unless the list is enabled or the item selected,
//!   brightness = alpha].
//! - **The highlight box** round the chosen entry of the list that has the keys: lines of
//!   [`LINE`] at brightness 200, its inside filled at alpha 40 [documented:
//!   list_box.xml `lb_highlight_box` (`_line_alpha` 255, `_fill_alpha` 40), box.xml,
//!   globals.xml `_line_brightness` 200; the box is as wide as the list less 5]. The owner
//!   asked for it on the main menu [owner, todo 5 item 46].
//! - **Sub-lists** (the settings' categories) to the left of the column, their right edge
//!   [`SUB_RIGHT`] from the view's, [`SUB_WIDTH`] wide [documented: `settings_container`
//!   x = main x − 155 + 150, width 155].
//!
//! Font 3 (`glow_monofonto_medium`) for small print (the version line) [documented:
//! start_menu.xml `main_version` font 3; Fallout_default.ini font 3].

use crate::interface::{amber, face, plain_font, set_line, u, Face};
use bevy::prelude::*;
use nvf_assets::GameData;

/// Font 7 [documented: Fallout_default.ini `sFontFile_7`].
pub const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
/// Font 3 [documented: Fallout_default.ini `sFontFile_3`].
pub const SMALL_FONT: &str = "textures\\fonts\\glow_monofonto_medium.fnt";

/// One entry's height [documented: start_menu.xml `_item_height`].
pub const ITEM: f32 = 48.0;
/// The column's right edge from the view's [observed: about 56 of 1080 pixels on the
/// owner's main menu screenshot, 37 of 720 on the pause menu's].
pub const RIGHT: f32 = 50.0;
/// The column's width [documented: `main_container` 375].
pub const COLUMN: f32 = 375.0;
/// The sub-list's right edge from the view's and its width [documented:
/// `settings_container`: main x − 155 + 150, width 155].
pub const SUB_RIGHT: f32 = RIGHT + COLUMN - 150.0;
pub const SUB_WIDTH: f32 = 155.0;
/// The highlight box's lines [hypothesis: globals.xml `_line_thickness` is 2 times the
/// resolution converter on the PC; taken as 2 interface units].
pub const LINE: f32 = 2.0;
/// Where the glyphs stand in an entry: on a line at about three quarters of it [observed
/// on the screenshots].
pub const BASE: f32 = ITEM * 0.72;

/// How an entry's text is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shade {
    /// In the list that has the keys, or the chosen entry of one that lost them.
    Bright,
    /// In a list that lost the keys, or an entry that cannot be taken.
    Dim,
}

/// The text's colour [documented: alpha 255 or 127, brightness the same].
pub fn color(shade: Shade) -> Color {
    match shade {
        Shade::Bright => amber(255.0),
        Shade::Dim => amber(127.0).with_alpha(127.0 / 255.0),
    }
}

/// The highlight box's lines and fill [documented: list_box.xml, box.xml, globals.xml].
pub fn highlight_colors() -> (Color, Color) {
    (amber(200.0), amber(255.0).with_alpha(40.0 / 255.0))
}

/// A highlight box filling its node.
pub fn highlight(node: Node) -> impl Bundle {
    let (line, fill) = highlight_colors();
    (Node { border: UiRect::all(u(LINE)), ..node }, BorderColor::all(line), BackgroundColor(fill))
}

/// The fonts of the menus: font 7 for the entries, font 3 for small print.
pub struct Faces {
    pub entry: Face,
    pub small: Face,
}

impl Faces {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Faces {
        Faces { entry: face(FONT, plain_font(36.0, 17.0), data, images, bc), small: face(SMALL_FONT, plain_font(24.0, 11.0), data, images, bc) }
    }

    /// Stand-in fonts (tests).
    pub fn plain() -> Faces {
        Faces { entry: Face { font: plain_font(36.0, 17.0), image: None }, small: Face { font: plain_font(24.0, 11.0), image: None } }
    }
}

/// A line of `text` in `face` starting at the parent's left edge and standing on `base`.
pub fn text(p: &mut ChildSpawnerCommands, face: &Face, text: &str, x: f32, base: f32, color: Color) {
    if let Some(line) = face.font.lines(text, None).into_iter().next() {
        set_line(p, face, &line, x, base, color);
    }
}

/// Lines of `text` wrapped at `width`, from the parent's top left, `step` apart.
pub fn wrapped(p: &mut ChildSpawnerCommands, face: &Face, text: &str, width: f32, step: f32, color: Color) {
    for (i, line) in face.font.lines(text, Some(width)).iter().enumerate() {
        set_line(p, face, line, 0.0, step * (i as f32 + 0.8), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_that_lost_the_keys_is_half_as_bright() {
        let (b, d) = (color(Shade::Bright).to_srgba(), color(Shade::Dim).to_srgba());
        assert_eq!(b.alpha, 1.0);
        assert!((d.alpha - 127.0 / 255.0).abs() < 1e-6);
        assert!((d.red / b.red - 127.0 / 255.0).abs() < 1e-3);
        // The sub-list's right edge: 225 units left of the column's.
        assert_eq!(SUB_RIGHT - RIGHT, 225.0);
    }
}
