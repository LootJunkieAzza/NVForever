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
//! - With `[UI] TileMenus=HUDMainMenu` (T12a, `super::vanilla`) the whole menu draws
//!   instead ([`Select::Whole`]), with `zoom`, `tile`, any `texatlas` and the compass
//!   strip ([`Fit`]; menu-xml.md §3.1.3).
//! - The pieces are worked out every frame (a pure function of the tree, [`pieces`]);
//!   the Bevy nodes are rebuilt only when they differ from the last frame's [design: the
//!   tree's change count moves on every write, so it cannot tell].

use super::layout::{self, Alignment, Kind, Rect as TileRect};
use super::vanilla::{kept_pieces, TileMenus, KEPT_PIECES, STRIP_TILE};
use crate::hud::{strip_left, HudView, STRIP};
use crate::interface::{self, at, bc_supported, face, line_base, plain_font, set_line, Face, Pictures, SizedPicture, FONT_FILES};
use crate::script::ScriptState;
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
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

    /// The hand-built HUD's corner label places (`crate::hud::label_places`) in font 7, the
    /// HUD's [documented: hudtemplates.xml's text templates], and how far its letters stand
    /// below a line's top ([`line_base`]): what a text tile's `y` is above the line.
    pub(crate) fn hud_labels(&mut self) -> ([(f32, f32); 4], f32) {
        self.load(7.0);
        match self.face(7.0) {
            Some(face) => (crate::hud::label_places(&face.font), line_base(face)),
            None => (crate::hud::label_places(&plain_font(31.0, 15.0)), 0.0),
        }
    }

    /// Write the HUD text tiles' measured widths into the tree
    /// ([`fnv_game::ui_hud::write_text_widths`]); only with the game's own fonts, a stand-in
    /// font would write made-up widths [design].
    pub(crate) fn write_widths(&mut self, ui: &mut Ui) {
        if self.data.is_none() {
            return;
        }
        let fonts: Vec<f64> = fnv_game::ui_hud::measured_texts(ui).into_iter()
            .map(|t| ui.tile(t).get("font").map_or(0.0, Value::num)).collect();
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

/// How an image fills its box [what the traits are taken to mean: menu-xml.md §3.1.3].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// Stretched over the box: no `zoom` [hypothesis: the brackets' 256 x 128-texel atlas
    /// pieces fill their 512 x 256 boxes in the original, hud.md §14.3], `zoom` -1
    /// [documented: GECK wiki Zoom, "free scaling … regardless its source size"] or a
    /// constant such as `&scale;` [hypothesis: its pictures, the compass ticks and the
    /// condition arrows, are the size of their boxes].
    Stretch,
    /// A number: the picture at its texels' size times `zoom` / 100 (one texel one
    /// interface unit at 100), from the box's top left, cut to the box [documented: GECK
    /// wiki Zoom, a multiplier of the picture's size that "cannot exceed the specified
    /// height and width"; hypothesis: one texel a unit, as the 1024-texel compass strip
    /// turns 1024 units, hud.md §14.5, and the top-left anchor].
    Zoom(f64),
    /// `tile` true: the picture repeated over the box at one texel a unit [hypothesis: by
    /// the trait's name; the bars' 8-texel tick picture in boxes the code sizes in steps
    /// of 8, hud.md §14.3].
    Tiled,
    /// The compass strip: twice side by side, moved across its window by the heading
    /// (nothing in the tree moves it, hud.md §14.4) at 1024 units a turn with the heading
    /// at x 148 of the window [observed: hud.md §14.5; design: `hud::strip_left`, the
    /// hand-built HUD's own].
    Strip,
}

/// What one drawn tile looks like.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    /// A `rect` with its own `red`, `green` or `blue`: a filled box.
    Box,
    /// An `image`'s `filename`, from the atlas its `texatlas` names (`None`: the shared
    /// one, as T5 reads every picture), filling its box as `fit` says.
    Picture { name: String, atlas: Option<String>, fit: Fit },
    Text { string: String, font: u8, wrap: Option<f64>, alignment: Alignment, zoom: f64 },
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

/// Which tiles of the menu draw.
#[derive(Clone, Copy, Debug)]
pub enum Select<'a> {
    /// T5: only the tiles mods added ([`fnv_ui::Ui::mod_added_tiles`], each kept when
    /// [`fnv_ui::Ui::mod_tile_drawn`] accepts it against these vanilla names), every
    /// picture stretched over its box from the shared atlas or its own file, as T5 drew
    /// them.
    Mods { vanilla: Option<&'a HashSet<String>> },
    /// T12a, `[UI] TileMenus=HUDMainMenu`: every tile of the menu (the file's, the
    /// code-made copies and the mods'), but the file's and the code's under `kept` (the
    /// pieces NVForever still draws by hand, `super::vanilla::KEPT_PIECES`); `zoom`,
    /// `tile` and `texatlas` applied, `strip` scrolled by the heading [design].
    Whole { kept: &'a [TileId], strip: Option<TileId> },
}

/// A system colour by its constant, as the base colour: `None` for the tile's own red,
/// green and blue.
fn system_colour(v: &Value) -> Option<[f32; 3]> {
    let rgb = |c: [u8; 3]| c.map(|x| f32::from(x) / 255.0);
    let alt = || {
        let c = crate::reticle::crime_colour().to_srgba();
        [c.red, c.green, c.blue]
    };
    match v {
        Value::Sym(name) => match name.to_ascii_lowercase().as_str() {
            // [hypothesis: by its name, the HUD's main colour; NVForever draws its HUD in
            // `interface::hud_rgb`, the settings' HUD colour].
            "hudmain" => Some(rgb(interface::hud_rgb())),
            // [observed: Fallout_default.ini `iSystemColorHUDAlt*` 255, 67, 42, as the
            // radiation meter (radiation.md §6) and `reticle::crime_colour`].
            "hudalt" => Some(alt()),
            // [hypothesis: by its name, the Pip-Boy's screen colour].
            "pipboy" => Some(rgb(interface::pipboy_rgb())),
            // [hypothesis: by their names, the tile's own colour].
            "nosystemcolor" | "uselocalcolor" => None,
            // `&terminal;`, `&mainmenu;` and unknown names: the HUD's colour [hypothesis].
            _ => Some(rgb(interface::hud_rgb())),
        },
        // [observed: private jobs hud_jip file_0032, hud_hardcore2 hc4-hc5]
        Value::Num(n) if *n == 1.0 => Some(rgb(interface::hud_rgb())),
        Value::Num(n) if *n == 2.0 => Some(alt()),
        // Other nonzero IDs retain the HUD colour [hypothesis: menu-xml.md §3.1.2].
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

/// How an image fills its box, by its `tile` and `zoom` ([`Fit`]).
pub fn fit(tile: &fnv_ui::Tile) -> Fit {
    if tile.get("tile").is_some_and(Value::truthy) {
        return Fit::Tiled;
    }
    match tile.get("zoom") {
        None | Some(Value::Sym(_)) => Fit::Stretch,
        Some(v) if v.num() < 0.0 => Fit::Stretch,
        Some(v) => Fit::Zoom(v.num()),
    }
}

/// Whether `id` is one of `of` or under one.
fn under(ui: &Ui, id: TileId, of: &[TileId]) -> bool {
    let mut at = Some(id);
    while let Some(t) = at {
        if of.contains(&t) {
            return true;
        }
        at = ui.tile(t).parent;
    }
    false
}

/// The pieces to draw of `menu`, in drawing order (stable by summed `depth`
/// [hypothesis: depth is the stacking order, ui-tile-tree.md §6.1]), and how many of the
/// mod roots ([`fnv_ui::Ui::mod_added_tiles`]) draw at least one. `select` says which
/// tiles ([`Select`]).
pub fn pieces(ui: &Ui, menu: TileId, measure: &layout::Measure<'_>, select: Select<'_>) -> (Vec<Piece>, usize) {
    let roots = ui.mod_added_tiles(menu);
    let items: Vec<(Option<TileId>, layout::DrawItem)> = match select {
        Select::Mods { .. } => roots.iter().flat_map(|&r| layout::layout(ui, r, measure).into_iter().map(move |i| (Some(r), i))).collect(),
        Select::Whole { .. } => layout::layout(ui, menu, measure).into_iter().map(|i| (roots.iter().copied().find(|&r| under(ui, i.tile, &[r])), i)).collect(),
    };
    let whole = matches!(select, Select::Whole { .. });
    let mut out = Vec::new();
    let mut drawn_roots = HashSet::new();
    for (root, item) in items {
        let wanted = match select {
            Select::Mods { vanilla } => ui.mod_tile_drawn(item.tile, vanilla),
            // A mod's own tiles draw under the kept pieces too [design].
            Select::Whole { kept, .. } => !under(ui, item.tile, kept) || (root.is_some() && ui.mod_tile_drawn(item.tile, None)),
        };
        if !item.visible || item.alpha <= 0.0 || !wanted {
            continue;
        }
        let tile = ui.tile(item.tile);
        let look = match &item.kind {
            Kind::Rect if ["red", "green", "blue"].iter().any(|n| tile.get(n).is_some()) => Look::Box,
            Kind::Image(image) if !image.filename.trim().is_empty() => {
                let strip = matches!(select, Select::Whole { strip: Some(s), .. } if s == item.tile);
                let fit = match () {
                    _ if strip => Fit::Strip,
                    _ if whole => fit(tile),
                    _ => Fit::Stretch,
                };
                let atlas = image.texatlas.as_ref().map(|a| a.text().trim().to_string()).filter(|a| whole && !a.is_empty());
                Look::Picture { name: image.filename.trim().to_string(), atlas, fit }
            }
            Kind::Text(text) if !text.string.is_empty() => {
                Look::Text { string: text.string.clone(), font: font_number(text.font), wrap: text.wrapwidth.filter(|w| *w > 0.0), alignment: text.alignment, zoom: text.zoom }
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
        if let Some(r) = root {
            drawn_roots.insert(r);
        }
    }
    out.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    (out, drawn_roots.len())
}

/// The renderer's node over the HUD.
#[derive(Component)]
struct TileRoot;

/// The compass strip of a [`Fit::Strip`] picture, moved by [`scroll`].
#[derive(Component)]
pub(crate) struct TileStrip;

#[derive(Default)]
pub(crate) struct Drawn {
    root: Option<Entity>,
    pieces: Vec<Piece>,
    /// Logical pixels to an interface unit the pieces were set for (the tiled pictures').
    scale: f32,
    counts: (usize, usize),
    /// The whole menu's start line is written (T12a).
    told: bool,
    pictures: Option<Pictures>,
}

fn f(v: f64) -> f32 {
    v as f32
}

/// What setting a piece out reads besides the piece.
struct Ctx<'a> {
    fonts: &'a TileFonts,
    /// Logical pixels to an interface unit.
    scale: f32,
    heading: f32,
}

fn spawn_piece(p: &mut ChildSpawnerCommands, piece: &Piece, ctx: &Ctx<'_>, picture: Option<SizedPicture>) {
    let r = piece.rect;
    let (dx, dy) = piece.clip.map_or((0.0, 0.0), |c| (c.left, c.top));
    let [cr, cg, cb, ca] = piece.colour;
    let colour = Color::srgba(cr, cg, cb, ca);
    let node = at(f(r.left - dx), f(r.top - dy), f(r.width), f(r.height));
    match &piece.look {
        Look::Box => {
            p.spawn((node, BackgroundColor(colour), Pickable::IGNORE));
        }
        Look::Picture { fit, .. } => {
            let Some((image, rect, size)) = picture else { return };
            let picture = |image_mode| ImageNode { color: colour, rect, image_mode, ..ImageNode::new(image.clone()) };
            let mut cut = node.clone();
            cut.overflow = Overflow::clip();
            match *fit {
                Fit::Stretch => {
                    p.spawn((node, picture(NodeImageMode::Stretch), Pickable::IGNORE));
                }
                Fit::Tiled => {
                    let mode = NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: ctx.scale };
                    p.spawn((node, picture(mode), Pickable::IGNORE));
                }
                Fit::Zoom(zoom) => {
                    let z = f(zoom / 100.0);
                    p.spawn((cut, Pickable::IGNORE)).with_children(|c| {
                        c.spawn((at(0.0, 0.0, size.x * z, size.y * z), picture(NodeImageMode::Stretch), Pickable::IGNORE));
                    });
                }
                Fit::Strip => {
                    p.spawn((cut, Pickable::IGNORE)).with_children(|c| {
                        c.spawn((TileStrip, at(strip_left(ctx.heading), 0.0, 2.0 * STRIP.x, STRIP.y), Pickable::IGNORE)).with_children(|s| {
                            for i in 0..2 {
                                s.spawn((at(i as f32 * STRIP.x, 0.0, STRIP.x, STRIP.y), picture(NodeImageMode::Stretch), Pickable::IGNORE));
                            }
                        });
                    });
                }
            }
        }
        Look::Text { string, font, wrap, alignment, zoom } => {
            let Some(face) = ctx.fonts.faces.get(font) else { return };
            let base = line_base(face);
            p.spawn((node, Pickable::IGNORE)).with_children(|outer| {
              // [design] A zero-size wrapper scales glyphs about the text's top left.
              outer.spawn((at(0.0, 0.0, 0.0, 0.0), UiTransform { scale: Vec2::splat(*zoom as f32), ..default() }, Pickable::IGNORE)).with_children(|t| {
                for (i, line) in face.font.lines(string, wrap.map(|w| w as f32)).iter().enumerate() {
                    // Each line inside the box by the tile's justification [hypothesis].
                    let x = match alignment {
                        Alignment::Left => 0.0,
                        Alignment::Center => (f(r.width / zoom) - line.width) / 2.0,
                        Alignment::Right => f(r.width / zoom) - line.width,
                    };
                    set_line(t, face, line, x, base + i as f32 * face.font.size, colour);
                }
              });
            });
        }
    }
}

/// How many of the menu's live tiles come from its files and how many from the code.
fn origins(ui: &Ui, menu: TileId) -> (usize, usize) {
    let (mut file, mut code) = (0, 0);
    let mut stack = vec![menu];
    while let Some(t) = stack.pop() {
        let tile = ui.tile(t);
        if !tile.live {
            continue;
        }
        match tile.origin {
            fnv_ui::Origin::File { .. } => file += 1,
            fnv_ui::Origin::Code => code += 1,
            _ => {}
        }
        stack.extend(tile.children.iter().copied());
    }
    (file, code)
}

#[derive(SystemParam)]
pub(crate) struct View<'w, 's> {
    menus: Option<Res<'w, TileMenus>>,
    hud: Option<Res<'w, HudView>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

pub(crate) fn draw(mut commands: Commands, script: Res<ScriptState>, view: View, mut kit: FontKit, mut drawn: Local<Drawn>) {
    let whole = view.menus.as_ref().is_some_and(|m| m.hud);
    let ui = script.rt.as_ref().filter(|rt| rt.game.ui_files.is_some()).map(|rt| &rt.game.ui_tree);
    let menu = ui.and_then(|ui| ui.menu(HUD_MENU).map(|m| (ui, m)));
    let (pieces, roots) = match menu {
        Some((ui, menu)) => {
            // The fonts of the text tiles drawn, read before measuring.
            let mut stack = if whole { vec![menu] } else { ui.mod_added_tiles(menu) };
            while let Some(t) = stack.pop() {
                let tile = ui.tile(t);
                if tile.kind == "text" {
                    kit.load(tile.get("font").map_or(0.0, Value::num));
                }
                stack.extend(tile.children.iter().copied());
            }
            let measured = |font: f64, text: &str, wrap: Option<f64>| kit.face(font).map_or((0.0, 0.0), |face| measure(&face.font, text, wrap));
            if whole {
                if !drawn.told {
                    let (file, code) = origins(ui, menu);
                    info!("tiles: {HUD_MENU} drawn from tiles ({file} file tiles, {code} code tiles); hand-built HUD hidden but for {}", KEPT_PIECES.join(", "));
                    drawn.told = true;
                }
                let kept = kept_pieces(ui);
                pieces(ui, menu, &measured, Select::Whole { kept: &kept, strip: ui.find_tile(STRIP_TILE) })
            } else {
                pieces(ui, menu, &measured, Select::Mods { vanilla: None })
            }
        }
        None => (Vec::new(), 0),
    };
    // The whole menu's pieces come and go with the HUD's states; only the mods' count is
    // told then [design].
    let counts = (roots, if whole { 0 } else { pieces.len() });
    if counts != drawn.counts {
        if whole {
            info!("tiles: {HUD_MENU}: {} mod tiles drawn (whole menu)", counts.0);
        } else {
            info!("tiles: {HUD_MENU}: {} mod tiles drawn ({} items)", counts.0, counts.1);
        }
        drawn.counts = counts;
    }
    let scale = view.windows.single().map_or(1.0, |w| w.height() / 960.0);
    if pieces == drawn.pieces && scale == drawn.scale {
        return;
    }
    let root = *drawn.root.get_or_insert_with(|| {
        let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
        commands.spawn((TileRoot, full, GlobalZIndex(TILES_Z), Pickable::IGNORE)).id()
    });
    let bc = bc_supported(kit.formats.as_deref());
    let data = kit.data.as_deref();
    let pictures = drawn.pictures.get_or_insert_with(|| Pictures::load(data));
    let mut looked: Vec<Option<SizedPicture>> = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        looked.push(match &piece.look {
            Look::Picture { name, atlas, .. } => pictures.picture_sized(name, atlas.as_deref(), data, &mut kit.images, bc),
            _ => None,
        });
    }
    let ctx = Ctx { fonts: &kit.fonts, scale, heading: view.hud.as_ref().map_or(0.0, |h| h.heading) };
    commands.entity(root).despawn_related::<Children>().with_children(|r| {
        for (piece, picture) in pieces.iter().zip(looked) {
            match piece.clip {
                Some(c) => {
                    let mut wrapper = at(f(c.left), f(c.top), f(c.width), f(c.height));
                    wrapper.overflow = Overflow::clip();
                    r.spawn((wrapper, Pickable::IGNORE)).with_children(|w| spawn_piece(w, piece, &ctx, picture));
                }
                None => spawn_piece(r, piece, &ctx, picture),
            }
        }
    });
    drawn.pieces = pieces;
    drawn.scale = scale;
}

/// Move the tile-drawn compass strip to the heading [design: as the hand-built HUD's
/// `hud::draw` moves its own, `hud::strip_left`; hud.md §14.4-§14.5].
pub(crate) fn scroll(hud: Option<Res<HudView>>, mut strips: Query<&mut Node, With<TileStrip>>) {
    let Some(hud) = hud else { return };
    let left = interface::u(strip_left(hud.heading));
    for mut node in &mut strips {
        if node.left != left {
            node.left = left;
        }
    }
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
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }), (Vec::new(), 0));
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
        let (drawn, roots) = pieces(&ui, menu, &plain, Select::Mods { vanilla: None });
        assert_eq!(roots, 1);
        let names: Vec<_> = drawn.iter().map(|p| ui.tile(p.tile).name.as_str()).collect();
        assert_eq!(names, ["Mod", "Pic", "Words"]);
        assert_eq!(drawn[0].look, Look::Box);
        assert_eq!(drawn[0].rect, TileRect { left: 105.0, top: 56.0, width: 50.0, height: 20.0 });
        // Its own colour, not the piece's system colour; alpha 128 of 255.
        assert_eq!(drawn[0].colour, [0.0, 128.0 / 255.0, 1.0, 128.0 / 255.0]);
        // The picture takes the piece's colour (the HUD's); the text's brightness 0 is black.
        let hud = interface::hud_rgb().map(|c| f32::from(c) / 255.0);
        assert_eq!(drawn[1].look, Look::Picture { name: "solid.dds".into(), atlas: None, fit: Fit::Stretch });
        assert_eq!(drawn[1].colour[..3], hud);
        assert_eq!(drawn[2].look, Look::Text { string: "abc".into(), font: 1, wrap: None, alignment: Alignment::Right, zoom: 1.0 });
        assert_eq!(drawn[2].rect, TileRect { left: 75.0, top: 56.0, width: 30.0, height: 20.0 });
        assert_eq!(drawn[2].colour, [0.0, 0.0, 0.0, 128.0 / 255.0]);
        // The HUD hidden hides them; a zero alpha too.
        let hidden = ui.find_tile("HUDMainMenu").unwrap();
        ui.set_trait(hidden, "visible", Value::Num(0.0));
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }).1, 0);
        ui.set_trait(hidden, "visible", Value::Num(1.0));
        ui.set_float("HUDMainMenu/Piece/Mod/alpha", 0.0);
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }).1, 0);
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
        let (drawn, roots) = pieces(&ui, menu, &plain, Select::Mods { vanilla: None });
        assert_eq!(roots, 2);
        let names: Vec<_> = drawn.iter().map(|p| ui.tile(p.tile).name.as_str()).collect();
        assert_eq!(names, ["Back", "Clip", "Zero", "Front"]);
        assert_eq!(drawn[3].clip, Some(TileRect { left: 0.0, top: 0.0, width: 40.0, height: 40.0 }));
        assert_eq!(drawn[1].clip, None);
        assert_eq!(drawn[2].look, Look::Text { string: "x".into(), font: 1, wrap: None, alignment: Alignment::Left, zoom: 1.0 });
    }

    #[test]
    fn a_loose_hud_draws_nothing_of_its_file_without_the_archived_names() {
        let src = MemSource::default().with_loose("hud.xml", r#"<menu name="HUDMainMenu"><rect name="New"><red>9</red><width>4</width><height>4</height></rect></menu>"#);
        let mut ui = Ui::new();
        let menu = ui.load_menu(&src, "hud.xml").unwrap();
        ui.update();
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }).1, 0);
        let vanilla: HashSet<String> = ["hudmainmenu".to_string()].into();
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: Some(&vanilla) }).1, 1);
    }

    #[test]
    fn font_numbers_and_measuring() {
        assert_eq!([0.0, 1.0, 7.0, 9.0, 10.0, -3.0, 7.4].map(font_number), [1, 1, 7, 9, 1, 1, 7]);
        let font = plain_font(31.0, 15.0);
        assert_eq!(measure(&font, "ab cd", None), (75.0, 31.0));
        assert_eq!(measure(&font, "ab cd", Some(0.0)), (75.0, 31.0));
        assert_eq!(measure(&font, "ab cd", Some(40.0)), (30.0, 62.0));
    }

    #[test]
    fn tiles_numeric_colours_match_symbols_and_flow_to_enemy_children() {
        let src = MemSource::default().with("c.xml", r#"<menu name="HUDMainMenu"><rect name="EnemyHealth"><systemcolor>2</systemcolor>
            <image name="meter"><filename>tick.dds</filename><brightness>175</brightness><width>32</width><height>20</height></image>
            </rect><text name="Word"><systemcolor>1</systemcolor><string>H20</string><zoom>75</zoom><justify>&right;</justify></text></menu>"#);
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "c.xml").unwrap();
        ui.update();
        let (drawn, _) = pieces(&ui, root, &plain, Select::Whole { kept: &kept_pieces(&ui), strip: None });
        assert_eq!(drawn.len(), 2);
        for (number, name) in [(1.0, "hudmain"), (2.0, "hudalt")] {
            assert_eq!(system_colour(&Value::Num(number)), system_colour(&Value::Sym(name.into())));
        }
        let red = system_colour(&Value::Num(2.0)).unwrap().map(|c| c * (175.0 / 255.0));
        assert_eq!(drawn[0].colour[..3], red);
        assert_eq!(drawn[1].rect, TileRect { left: -22.5, top: 0.0, width: 22.5, height: 15.0 });
        assert!(matches!(drawn[1].look, Look::Text { zoom: 0.75, .. }));
        for name in ["EnemyHealth", "Info", "HardcoreMode", "XPMeter", "crippled_limb_indicator"] {
            assert!(!KEPT_PIECES.contains(&name));
        }
    }

    /// [design] A synthetic HUD after the vanilla file's shapes: a corner piece in the
    /// HUD's colour with the compass window and a meter the code copies from templates, a
    /// kept piece (the prompt) with a file text, and the radiation meter's alt colour.
    fn vanilla_hud() -> (Ui, MemSource, TileId) {
        let src = MemSource::default().with(
            "hud.xml",
            r#"<menu name="HUDMainMenu"><visible>&false;</visible>
              <rect name="HitPoints"><x>40</x><y>803</y><systemcolor>&hudmain;</systemcolor></rect>
              <rect name="Info"><systemcolor>&hudmain;</systemcolor><text name="button_text"><string>Talk</string></text></rect>
              <rect name="RadiationMeter"><systemcolor>&hudalt;</systemcolor>
                <image name="radiation_bracket"><filename>Interface\HUD\bracket.dds</filename><width>567</width><height>120</height></image></rect>
              <template name="template_compass_window"><image name="compass_window"><filename> Interface\HUD\strip.dds </filename>
                <texatlas> Interface\InterfaceShared.tai </texatlas><width>345</width><height>64</height><zoom>100</zoom><x>20</x><y>65</y></image></template>
              <template name="template_meter"><image name="meter"><filename>Interface\HUD\hud_tick_mark.dds</filename><tile>&true;</tile><width>144</width><height>20</height></image></template>
              <template name="template_justify_left_text"><text name="justify_left_text"><string>HP</string><zoom>&scale;</zoom></text></template>
            </menu>"#,
        );
        let mut ui = Ui::new();
        let menu = ui.load_menu(&src, "hud.xml").unwrap();
        for template in ["template_compass_window", "template_meter", "template_justify_left_text"] {
            ui.add_code_tile_from_template(&src, "HUDMainMenu/HitPoints", template, None).unwrap();
        }
        ui.add_code_tile_from_template(&src, "HUDMainMenu/Info", "template_justify_left_text", None).unwrap();
        ui.set_float("HUDMainMenu/visible", 1.0);
        ui.update();
        (ui, src, menu)
    }

    fn names(ui: &Ui, drawn: &[Piece]) -> Vec<String> {
        drawn.iter().map(|p| ui.path_of(p.tile)).collect()
    }

    #[test]
    fn with_the_switch_on_the_files_and_the_codes_tiles_draw_but_the_kept_pieces() {
        let (mut ui, src, menu) = vanilla_hud();
        // Off (T5): none of them.
        assert_eq!(pieces(&ui, menu, &plain, Select::Mods { vanilla: None }).0, Vec::new());
        let kept = [ui.find_tile("HUDMainMenu/Info").unwrap()];
        let strip = ui.find_tile("HUDMainMenu/HitPoints/compass_window");
        let whole = |ui: &Ui| pieces(ui, menu, &plain, Select::Whole { kept: &kept, strip });
        let (drawn, roots) = whole(&ui);
        assert_eq!(roots, 0);
        assert_eq!(
            names(&ui, &drawn),
            ["HUDMainMenu/HitPoints/compass_window", "HUDMainMenu/HitPoints/meter", "HUDMainMenu/HitPoints/justify_left_text", "HUDMainMenu/RadiationMeter/radiation_bracket"]
        );
        // The strip scrolls; the tiled meter repeats; `&scale;` and no zoom stretch; the
        // texatlas goes to the picture lookup, trimmed.
        assert_eq!(drawn[0].look, Look::Picture { name: "Interface\\HUD\\strip.dds".into(), atlas: Some("Interface\\InterfaceShared.tai".into()), fit: Fit::Strip });
        assert_eq!(drawn[0].rect, TileRect { left: 60.0, top: 868.0, width: 345.0, height: 64.0 });
        assert_eq!(drawn[1].look, Look::Picture { name: "Interface\\HUD\\hud_tick_mark.dds".into(), atlas: None, fit: Fit::Tiled });
        assert_eq!(drawn[3].look, Look::Picture { name: "Interface\\HUD\\bracket.dds".into(), atlas: None, fit: Fit::Stretch });
        // Colours as the hand-built HUD's: the HUD's at full brightness and alpha, the
        // radiation meter's the alt colour.
        let hud = interface::hud_rgb().map(|c| f32::from(c) / 255.0);
        assert_eq!(drawn[2].colour, [hud[0], hud[1], hud[2], 1.0]);
        let alt = crate::reticle::crime_colour().to_srgba();
        assert_eq!(drawn[3].colour, [alt.red, alt.green, alt.blue, 1.0]);
        // Without the strip named, the compass window is its zoom's.
        let (plain_strip, _) = pieces(&ui, menu, &plain, Select::Whole { kept: &kept, strip: None });
        assert_eq!(plain_strip[0].look, Look::Picture { name: "Interface\\HUD\\strip.dds".into(), atlas: Some("Interface\\InterfaceShared.tai".into()), fit: Fit::Zoom(100.0) });
        // A mod's tile under a kept piece still draws, and counts as a mod root.
        assert!(ui.inject_text(&src, "HUDMainMenu/Info", r#"<rect name="Outline"><red>1</red><width>4</width><height>4</height></rect>"#));
        ui.update();
        let (drawn, roots) = whole(&ui);
        assert_eq!(roots, 1);
        assert!(names(&ui, &drawn).contains(&"HUDMainMenu/Info/Outline".to_string()));
        assert!(!names(&ui, &drawn).iter().any(|n| n.contains("button_text") || n.contains("Info/justify_left_text")));
        // The HUD hidden hides all of them.
        ui.set_float("HUDMainMenu/visible", 0.0);
        assert!(whole(&ui).0.is_empty());
    }

    #[test]
    fn zoom_and_tile_say_how_a_picture_fills_its_box() {
        let src = MemSource::default().with(
            "z.xml",
            r#"<menu name="Z"><image name="none"/><image name="free"><zoom>-1</zoom></image><image name="scale"><zoom>&scale;</zoom></image>
              <image name="hundred"><zoom>100</zoom></image><image name="more"><zoom> 125 </zoom></image><image name="tiled"><tile>&true;</tile><zoom>50</zoom></image>
              <image name="untiled"><tile>&false;</tile></image></menu>"#,
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "z.xml").unwrap();
        ui.update();
        let fit_of = |name: &str| fit(ui.tile(ui.find_tile(&format!("Z/{name}")).unwrap()));
        assert_eq!(
            ["none", "free", "scale", "hundred", "more", "tiled", "untiled"].map(fit_of),
            [Fit::Stretch, Fit::Stretch, Fit::Stretch, Fit::Zoom(100.0), Fit::Zoom(125.0), Fit::Tiled, Fit::Stretch]
        );
    }

    #[test]
    fn a_texatlas_names_the_index_its_picture_is_found_in() {
        // A fake shared index and a fake second one under textures\interface\mine\.
        let shared = interface::atlas_index("glow_a.dds\tInterfaceShared0.dds, 0, 2D, 0.5, 0.25, 0.0, 0.25, 0.125\n");
        let mut pictures = Pictures::with_atlas(shared);
        let reads = std::cell::RefCell::new(Vec::new());
        let read = |p: &str| -> Option<Vec<u8>> {
            reads.borrow_mut().push(p.to_string());
            (p == "textures\\interface\\mine\\mine.tai").then(|| b"# mine\nGlow_B.dds\tMine0.dds, 0, 2D, 0.0, 0.5, 0.0, 0.5, 0.5\n".to_vec())
        };
        let mine = Some(" Interface\\Mine\\Mine.tai ");
        assert_eq!(pictures.locate("Interface\\HUD\\glow_b.dds", mine, &read), interface::Located { texture: "textures\\interface\\mine\\mine0.dds".into(), part: Some([0.0, 0.5, 0.5, 0.5]) });
        // Not in the named index: the shared one's piece, else its own file.
        assert_eq!(pictures.locate("Interface\\HUD\\Glow_A.dds", mine, &read), interface::Located { texture: "textures\\interface\\interfaceshared0.dds".into(), part: Some([0.5, 0.25, 0.25, 0.125]) });
        assert_eq!(pictures.locate("\\Interface\\HUD\\c.dds", mine, &read), interface::Located { texture: "textures\\interface\\hud\\c.dds".into(), part: None });
        // The shared index by name is the one already read; a named index is read once.
        assert_eq!(pictures.locate("glow_a.dds", Some("Interface\\InterfaceShared.tai"), &read).part, Some([0.5, 0.25, 0.25, 0.125]));
        assert_eq!(*reads.borrow(), ["textures\\interface\\mine\\mine.tai"]);
        assert_eq!(interface::atlas_index_path("Interface/InterfaceShared.tai").as_deref(), Some(interface::ATLAS_INDEX));
        assert_eq!(interface::atlas_index_path("  "), None);
    }
}
