//! T5 part C: the tiles mods added to `HUDMainMenu`, drawn over NVForever's own HUD
//! [design: ui-tile-tree.md §6.1; what the drawing traits are taken to mean:
//! docs/spec-notes/menu-xml.md §3.1.2].
//!
//! - Which tiles: [`fnv_ui::Ui::mod_added_tiles`] of the HUD, laid out by
//!   [`super::layout`], each item kept only when [`fnv_ui::Ui::mod_tile_drawn`] says it is
//!   the mod's own. Code-made copies (`fnv_game::ui_hud::copies`) never draw. A loose
//!   replacement of the HUD file (or of a prefab it includes) draws none of its file
//!   tiles in v1: the VFS keeps no archived copy under a loose file to compare against
//!   (`fnv_vfs::Vfs` indexes one location per path) [design]; `fnvtool ui mod-tiles`
//!   shows what the comparison would pick where it can read both.
//! - Units: the tree's places are the HUD's interface units, 960 to the view's height
//!   at every size, so `screen()` is 960 × height/width wide [documented: hud.md §14;
//!   `fnv_game::ui_hud::write`]; [`crate::interface::u`] turns them into the window's
//!   pixels (height / 960 a unit), as the current HUD does.
//! - The pieces are worked out every frame (a pure function of the tree, [`pieces`]);
//!   the Bevy nodes are rebuilt only when they differ from the last frame's [design: the
//!   tree's change count moves on every write, so it cannot tell].

use super::layout::{self, Alignment, Kind, Rect as TileRect};
use crate::interface::{self, at, bc_supported, face, line_base, plain_font, set_line, Face, Pictures, FONT_FILES};
use crate::script::ScriptState;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::Font;
use fnv_ui::{TileId, Ui, Value};
use nvf_assets::GameData;
use std::collections::{HashMap, HashSet};

/// The menu drawn from: the HUD's file names it so [documented: hud_main_menu.xml].
pub const HUD_MENU: &str = "HUDMainMenu";

/// Over NVForever's HUD (its root has no stacking level, 0), the sneak gauges (5) and the
/// corner and quest messages (50); under the dialogue menu and the Pip-Boy (90), which
/// hide the HUD anyway [design].
pub const TILES_Z: i32 = 60;

/// A tile's `font` as the INI's font number: 1 to 9 as written; none, 0 or any other
/// number font 1 [hypothesis: the sneak word's tile has no `font`, and its observed widths,
/// 106 for "[HIDDEN]", 136 (the next word) and 14 (a space), are font 1's advances
/// summed, hud.md §14.1; observed 2026-10-29 on the owner's font files].
pub fn font_number(font: f64) -> u8 {
    let n = font.round();
    if (1.0..=9.0).contains(&n) {
        n as u8
    } else {
        1
    }
}

/// A text's width and height in interface units, set as `wrap` allows. A `wrapwidth` of
/// 0 or less does not wrap [observed: the prompt's `button_text` has `wrapwidth` 0 and the
/// original measured "Button Text" 156 wide, one line of font 7, hud.md §14.1]. The height
/// is the lines times the font's line height [hypothesis: fonts.md §2].
pub fn measure(font: &Font, text: &str, wrap: Option<f64>) -> (f64, f64) {
    let lines = font.lines(text, wrap.filter(|w| *w > 0.0).map(|w| w as f32));
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    (f64::from(width), f64::from(lines.len() as f32 * font.size))
}

/// The game's fonts by number, read when first asked for (the renderer and the text widths
/// written back share them).
#[derive(Resource, Default)]
pub struct TileFonts {
    faces: HashMap<u8, Face>,
}

#[derive(SystemParam)]
pub(crate) struct FontKit<'w> {
    fonts: ResMut<'w, TileFonts>,
    data: Option<Res<'w, GameData>>,
    images: ResMut<'w, Assets<Image>>,
    formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

impl FontKit<'_> {
    fn load(&mut self, font: f64) {
        let n = font_number(font);
        if !self.fonts.faces.contains_key(&n) {
            let bc = bc_supported(self.formats.as_deref());
            let f = face(FONT_FILES[n as usize - 1], plain_font(31.0, 15.0), self.data.as_deref(), &mut self.images, bc);
            self.fonts.faces.insert(n, f);
        }
    }

    fn face(&self, font: f64) -> Option<&Face> {
        self.fonts.faces.get(&font_number(font))
    }

    /// Write the HUD text tiles' measured widths into the tree
    /// ([`fnv_game::ui_hud::write_text_widths`]); only with the game's own fonts, a stand-in
    /// font would write made-up widths [design].
    pub(crate) fn write_widths(&mut self, ui: &mut Ui) {
        if self.data.is_none() {
            return;
        }
        let fonts: Vec<f64> = fnv_game::ui_hud::MEASURED_TEXTS
            .iter()
            .filter_map(|p| ui.find_tile(&format!("{HUD_MENU}/{p}")))
            .map(|t| ui.tile(t).get("font").map_or(0.0, Value::num))
            .collect();
        if fonts.is_empty() {
            return;
        }
        for &f in &fonts {
            self.load(f);
        }
        if fonts.iter().any(|&f| self.face(f).is_none_or(|face| face.image.is_none())) {
            return;
        }
        fnv_game::ui_hud::write_text_widths(ui, &|font, text| self.face(font).map_or(0.0, |f| measure(&f.font, text, None).0));
    }
}

/// What one drawn tile looks like.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// A `rect` with its own `red`, `green` or `blue`: a filled box.
    Box,
    /// An `image`'s `filename`.
    Picture(String),
    Text { string: String, font: u8, wrap: Option<f64>, alignment: Alignment },
}

/// One tile to draw, in interface units from the view's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub tile: TileId,
    pub look: Look,
    pub rect: TileRect,
    pub clip: Option<TileRect>,
    /// sRGB and alpha, 0 to 1.
    pub colour: [f32; 4],
    pub depth: f64,
}

/// A system colour by its constant, as the base colour: `None` for the tile's own red,
/// green and blue.
fn system_colour(v: &Value) -> Option<[f32; 3]> {
    let rgb = |c: [u8; 3]| c.map(|x| f32::from(x) / 255.0);
    match v {
        Value::Sym(name) => match name.to_ascii_lowercase().as_str() {
            // [hypothesis: by its name, the HUD's main colour; NVForever draws its HUD in
            // `interface::hud_rgb`, the settings' HUD colour].
            "hudmain" => Some(rgb(interface::hud_rgb())),
            // [observed: Fallout_default.ini `iSystemColorHUDAlt*` 255, 67, 42, as the
            // radiation meter (radiation.md §6) and `reticle::crime_colour`].
            "hudalt" => {
                let c = crate::reticle::crime_colour().to_srgba();
                Some([c.red, c.green, c.blue])
            }
            // [hypothesis: by its name, the Pip-Boy's screen colour].
            "pipboy" => Some(rgb(interface::pipboy_rgb())),
            // [hypothesis: by their names, the tile's own colour].
            "nosystemcolor" | "uselocalcolor" => None,
            // `&terminal;`, `&mainmenu;` and unknown names: the HUD's colour [hypothesis].
            _ => Some(rgb(interface::hud_rgb())),
        },
        // The constants' numbers are not known (menu-xml.md §2.3): 0 the tile's own colour,
        // any other the HUD's [hypothesis].
        Value::Num(n) if *n != 0.0 => Some(rgb(interface::hud_rgb())),
        _ => None,
    }
}

/// A tile's base colour [hypothesis: the order is NVForever's, menu-xml.md §3.1.2]: its own
/// `systemcolor`; else its own `red`/`green`/`blue` when it sets any; else the nearest
/// tile above's `systemcolor`; else its own (255 each by default).
fn base_colour(ui: &Ui, id: TileId, own: [f32; 3]) -> [f32; 3] {
    let tile = ui.tile(id);
    if let Some(v) = tile.get("systemcolor") {
        return system_colour(v).unwrap_or(own);
    }
    if ["red", "green", "blue"].iter().any(|n| tile.get(n).is_some()) {
        return own;
    }
    let mut at = tile.parent;
    while let Some(p) = at {
        if let Some(v) = ui.tile(p).get("systemcolor") {
            return system_colour(v).unwrap_or(own);
        }
        at = ui.tile(p).parent;
    }
    own
}

/// The pieces to draw for the mod tiles of `menu`, in drawing order (stable by summed
/// `depth` [hypothesis: depth is the stacking order, ui-tile-tree.md §6.1]), and how many
/// mod roots draw at least one. `vanilla`: the archived file's name paths for a loose
/// replacement ([`fnv_ui::Ui::mod_tile_drawn`]).
pub fn pieces(ui: &Ui, menu: TileId, measure: &layout::Measure<'_>, vanilla: Option<&HashSet<String>>) -> (Vec<Piece>, usize) {
    let mut out = Vec::new();
    let mut roots = 0;
    for root in ui.mod_added_tiles(menu) {
        let before = out.len();
        for item in layout::layout(ui, root, measure) {
            if !item.visible || item.alpha <= 0.0 || !ui.mod_tile_drawn(item.tile, vanilla) {
                continue;
            }
            let tile = ui.tile(item.tile);
            let look = match &item.kind {
                Kind::Rect if ["red", "green", "blue"].iter().any(|n| tile.get(n).is_some()) => Look::Box,
                Kind::Image(image) if !image.filename.trim().is_empty() => Look::Picture(image.filename.trim().to_string()),
                Kind::Text(text) if !text.string.is_empty() => {
                    Look::Text { string: text.string.clone(), font: font_number(text.font), wrap: text.wrapwidth.filter(|w| *w > 0.0), alignment: text.alignment }
                }
                // Empty rects only place their children; nif, radial and window tiles
                // are not drawn in v1 [design].
                _ => continue,
            };
            if item.clip.is_some_and(|c| c.width <= 0.0 || c.height <= 0.0) {
                continue;
            }
            let c = &item.colour;
            let own = [c.red, c.green, c.blue].map(|x| x as f32 / 255.0);
            // `brightness` scales the colour, 255 the colour itself [hypothesis: as the
            // interface's `amber(brightness)`]; clamped to 0..=255 [design].
            let b = (c.brightness.clamp(0.0, 255.0) / 255.0) as f32;
            let [r, g, bl] = base_colour(ui, item.tile, own).map(|x| x * b);
            let alpha = (item.alpha / 255.0).clamp(0.0, 1.0) as f32;
            out.push(Piece { tile: item.tile, look, rect: item.rect, clip: item.clip, colour: [r, g, bl, alpha], depth: item.depth });
        }
        if out.len() > before {
            roots += 1;
        }
    }
    out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    (out, roots)
}

/// The renderer's node over the HUD.
#[derive(Component)]
struct TileRoot;

#[derive(Default)]
pub(crate) struct Drawn {
    root: Option<Entity>,
    pieces: Vec<Piece>,
    counts: (usize, usize),
    pictures: Option<Pictures>,
}

fn f(v: f64) -> f32 {
    v as f32
}

fn spawn_piece(p: &mut ChildSpawnerCommands, piece: &Piece, fonts: &TileFonts, picture: Option<interface::Picture>) {
    let r = piece.rect;
    let (dx, dy) = piece.clip.map_or((0.0, 0.0), |c| (c.left, c.top));
    let [cr, cg, cb, ca] = piece.colour;
    let colour = Color::srgba(cr, cg, cb, ca);
    let node = at(f(r.left - dx), f(r.top - dy), f(r.width), f(r.height));
    match &piece.look {
        Look::Box => {
            p.spawn((node, BackgroundColor(colour), Pickable::IGNORE));
        }
        Look::Picture(_) => {
            if let Some((image, rect)) = picture {
                p.spawn((node, ImageNode { color: colour, rect, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, Pickable::IGNORE));
            }
        }
        Look::Text { string, font, wrap, alignment } => {
            let Some(face) = fonts.faces.get(font) else { return };
            let base = line_base(face);
            p.spawn((node, Pickable::IGNORE)).with_children(|t| {
                for (i, line) in face.font.lines(string, wrap.map(|w| w as f32)).iter().enumerate() {
                    // Each line inside the box by the tile's justification [hypothesis].
                    let x = match alignment {
                        Alignment::Left => 0.0,
                        Alignment::Center => (f(r.width) - line.width) / 2.0,
                        Alignment::Right => f(r.width) - line.width,
                    };
                    set_line(t, face, line, x, base + i as f32 * face.font.size, colour);
                }
            });
        }
    }
}

pub(crate) fn draw(mut commands: Commands, script: Res<ScriptState>, mut kit: FontKit, mut drawn: Local<Drawn>) {
    let ui = script.rt.as_ref().filter(|rt| rt.game.ui_files.is_some()).map(|rt| &rt.game.ui_tree);
    let menu = ui.and_then(|ui| ui.menu(HUD_MENU).map(|m| (ui, m)));
    let (pieces, roots) = match menu {
        Some((ui, menu)) => {
            // The fonts of the text tiles under the mod roots, read before measuring.
            let mut stack = ui.mod_added_tiles(menu);
            while let Some(t) = stack.pop() {
                let tile = ui.tile(t);
                if tile.kind == "text" {
                    kit.load(tile.get("font").map_or(0.0, Value::num));
                }
                stack.extend(tile.children.iter().copied());
            }
            let measured = |font: f64, text: &str, wrap: Option<f64>| kit.face(font).map_or((0.0, 0.0), |face| measure(&face.font, text, wrap));
            pieces(ui, menu, &measured, None)
        }
        None => (Vec::new(), 0),
    };
    let counts = (roots, pieces.len());
    if counts != drawn.counts {
        info!("tiles: {HUD_MENU}: {} mod tiles drawn ({} items)", counts.0, counts.1);
        drawn.counts = counts;
    }
    if pieces == drawn.pieces {
        return;
    }
    let root = *drawn.root.get_or_insert_with(|| {
        let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
        commands.spawn((TileRoot, full, GlobalZIndex(TILES_Z), Pickable::IGNORE)).id()
    });
    let bc = bc_supported(kit.formats.as_deref());
    let data = kit.data.as_deref();
    let pictures = drawn.pictures.get_or_insert_with(|| Pictures::load(data));
    let mut looked: Vec<Option<interface::Picture>> = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        looked.push(match &piece.look {
            Look::Picture(name) => pictures.picture(name, data, &mut kit.images, bc),
            _ => None,
        });
    }
    let fonts = &*kit.fonts;
    commands.entity(root).despawn_related::<Children>().with_children(|r| {
        for (piece, picture) in pieces.iter().zip(looked) {
            match piece.clip {
                Some(c) => {
                    let mut wrapper = at(f(c.left), f(c.top), f(c.width), f(c.height));
                    wrapper.overflow = Overflow::clip();
                    r.spawn((wrapper, Pickable::IGNORE)).with_children(|w| spawn_piece(w, piece, fonts, picture));
                }
                None => spawn_piece(r, piece, fonts, picture),
            }
        }
    });
    drawn.pieces = pieces;
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    /// Every character 10 wide, lines 20 high [design].
    fn plain(_: f64, text: &str, _: Option<f64>) -> (f64, f64) {
        (10.0 * text.chars().count() as f64, 20.0)
    }

    fn hud() -> (Ui, MemSource) {
        // [design] A synthetic HUD: one piece in the HUD's colour with a file text, and a
        // template the code copies from.
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><rect name="Piece"><x>100</x><y>50</y><systemcolor>&hudmain;</systemcolor>
              <text name="Label"><string>HP</string></text>
              <template name="row"><text name="Row"><string>copy</string></text></template></rect></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "hud.xml").unwrap();
        ui.update();
        (ui, src)
    }

    #[test]
    fn only_the_mods_tiles_draw_and_code_copies_never() {
        let (mut ui, src) = hud();
        let menu = ui.menu(HUD_MENU).unwrap();
        assert_eq!(pieces(&ui, menu, &plain, None), (Vec::new(), 0));
        ui.add_code_tile_from_template(&src, "HUDMainMenu/Piece", "row", None).unwrap();
        assert!(ui.inject_text(
            &src,
            "HUDMainMenu/Piece",
            r#"<rect name="Mod"><x>5</x><y>6</y><width>50</width><height>20</height><red>0</red><green>128</green><blue>255</blue><alpha>128</alpha>
                <image name="Pic"><filename>solid.dds</filename><width>8</width><height>8</height></image>
                <text name="Words"><string>abc</string><justify>&right;</justify><brightness>0</brightness></text>
                <rect name="Empty"/><nif name="Model"/></rect>"#
        ));
        ui.update();
        let (drawn, roots) = pieces(&ui, menu, &plain, None);
        assert_eq!(roots, 1);
        let names: Vec<_> = drawn.iter().map(|p| ui.tile(p.tile).name.as_str()).collect();
        assert_eq!(names, ["Mod", "Pic", "Words"]);
        assert_eq!(drawn[0].look, Look::Box);
        assert_eq!(drawn[0].rect, TileRect { left: 105.0, top: 56.0, width: 50.0, height: 20.0 });
        // Its own colour, not the piece's system colour; alpha 128 of 255.
        assert_eq!(drawn[0].colour, [0.0, 128.0 / 255.0, 1.0, 128.0 / 255.0]);
        // The picture takes the piece's colour (the HUD's); the text's brightness 0 is black.
        let hud = interface::hud_rgb().map(|c| f32::from(c) / 255.0);
        assert_eq!(drawn[1].look, Look::Picture("solid.dds".into()));
        assert_eq!(drawn[1].colour[..3], hud);
        assert_eq!(drawn[2].look, Look::Text { string: "abc".into(), font: 1, wrap: None, alignment: Alignment::Right });
        assert_eq!(drawn[2].rect, TileRect { left: 75.0, top: 56.0, width: 30.0, height: 20.0 });
        assert_eq!(drawn[2].colour, [0.0, 0.0, 0.0, 128.0 / 255.0]);
        // The HUD hidden hides them; a zero alpha too.
        let hidden = ui.find_tile("HUDMainMenu").unwrap();
        ui.set_trait(hidden, "visible", Value::Num(0.0));
        assert_eq!(pieces(&ui, menu, &plain, None).1, 0);
        ui.set_trait(hidden, "visible", Value::Num(1.0));
        ui.set_float("HUDMainMenu/Piece/Mod/alpha", 0.0);
        assert_eq!(pieces(&ui, menu, &plain, None).1, 0);
    }

    #[test]
    fn depth_orders_the_drawing_and_clips_carry_to_the_pieces() {
        let (mut ui, src) = hud();
        let menu = ui.menu(HUD_MENU).unwrap();
        assert!(ui.inject_text(
            &src,
            "HUDMainMenu",
            r#"<rect name="Clip"><width>40</width><height>40</height><clips>&true;</clips><red>1</red>
                <image name="Front"><filename>a.dds</filename><depth>5</depth><width>80</width><height>80</height></image>
                <image name="Back"><filename>b.dds</filename><depth>-5</depth></image></rect>
              <text name="Zero"><string>x</string><font>0</font><wrapwidth>0</wrapwidth></text>"#
        ));
        ui.update();
        let (drawn, roots) = pieces(&ui, menu, &plain, None);
        assert_eq!(roots, 2);
        let names: Vec<_> = drawn.iter().map(|p| ui.tile(p.tile).name.as_str()).collect();
        assert_eq!(names, ["Back", "Clip", "Zero", "Front"]);
        assert_eq!(drawn[3].clip, Some(TileRect { left: 0.0, top: 0.0, width: 40.0, height: 40.0 }));
        assert_eq!(drawn[1].clip, None);
        assert_eq!(drawn[2].look, Look::Text { string: "x".into(), font: 1, wrap: None, alignment: Alignment::Left });
    }

    #[test]
    fn a_loose_hud_draws_nothing_of_its_file_without_the_archived_names() {
        let src = MemSource::default().with_loose("hud.xml", r#"<menu name="HUDMainMenu"><rect name="New"><red>9</red><width>4</width><height>4</height></rect></menu>"#);
        let mut ui = Ui::new();
        let menu = ui.load_menu(&src, "hud.xml").unwrap();
        ui.update();
        assert_eq!(pieces(&ui, menu, &plain, None).1, 0);
        let vanilla: HashSet<String> = ["hudmainmenu".to_string()].into();
        assert_eq!(pieces(&ui, menu, &plain, Some(&vanilla)).1, 1);
    }

    #[test]
    fn font_numbers_and_measuring() {
        assert_eq!([0.0, 1.0, 7.0, 9.0, 10.0, -3.0, 7.4].map(font_number), [1, 1, 7, 9, 1, 1, 7]);
        let font = plain_font(31.0, 15.0);
        assert_eq!(measure(&font, "ab cd", None), (75.0, 31.0));
        assert_eq!(measure(&font, "ab cd", Some(0.0)), (75.0, 31.0));
        assert_eq!(measure(&font, "ab cd", Some(40.0)), (30.0, 62.0));
    }
}
