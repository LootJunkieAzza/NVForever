//! What the parts of the original's interface are drawn with, shared by the modules that
//! draw one (`message_menu`, `corner_messages`, `hud`, `dialogue_menu`): lengths in the interface's units (960
//! to the height of the view), the interface's colour, the game's fonts and textures
//! read from the owner's archives at run time, a line of text set as glyphs of the
//! font's texture, and the index of the interface's texture atlas.
//!
//! Without the game's files a font is stood in for by one whose characters are all as
//! wide as each other, and its text is drawn with Bevy's font.

use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats, ImageSampler};
use bevy::prelude::*;
use fnv_font::{Font, Glyph, Line};
use nvf_assets::GameData;
use std::collections::HashMap;

/// The interface's colour [observed: the brightest text of the owner's screenshot,
/// message-menu.md §3].
pub const AMBER: [f32; 3] = [1.0, 182.0 / 255.0, 66.0 / 255.0];

/// How dark the sheet behind the menus drawn over the world in play makes it (the
/// container and barter menus, the tag-skill and trait lists): 0.75 black [design: the
/// owner's wish after the playtest of 2026-10-11, amber text was hard to read over an
/// orange world; the original blurs the world behind these menus instead (not built),
/// its sheet is lighter: about 0.33 to 0.4 measured on the screenshots].
pub const MENU_BACKING: f32 = 0.75;

/// The stacking level of the character and workbench menus (tag skills, traits, name,
/// level-up, recipes, repair services): over the HUD, the corner and quest messages
/// (50), the dialogue menu and the Pip-Boy (90), the terminal and the casino games (106);
/// under the message boxes (108) these menus may ask, the tutorial (109), the pause menu
/// (110), the console (120) and the loading screen [design: the owner's playtest list 7,
/// item 2, "on top level with nothing in front"].
pub const TOP_MENU_Z: i32 = 107;

/// How dark the sheet behind those menus makes the world: the container and barter
/// menus' 0.93 (`crate::menu::look`) [design: list 7 item 2, "the backgrounds need to be
/// darker", the 0.75 of [`MENU_BACKING`] and the files' 32 of 255 let the world show
/// through the text].
pub const TOP_MENU_SHADE: f32 = 0.93;

/// The menu files' shared fill: `_background_fill_alpha`, 32 of 255 [documented:
/// globals.xml]. The sheets the recipe and repair-services menus spread behind themselves
/// (`RM_Background`, `RSM_Background`) take it; at this strength their edge cannot be seen
/// over the world [observed: the owner's screenshots of both menus, 2026-10-19,
/// docs/spec-notes/repair.md §3].
pub const BACKGROUND_FILL_ALPHA: f32 = 32.0 / 255.0;

/// The interface's colour at a brightness (0 to 255, as the interface files give it).
pub fn amber(brightness: f32) -> Color {
    let b = brightness / 255.0;
    Color::srgb(AMBER[0] * b, AMBER[1] * b, AMBER[2] * b)
}

/// A length of the interface (960 units high) as a part of the view's height.
pub fn u(units: f32) -> Val {
    Val::Vh(units / 9.6)
}

pub fn at(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(left), top: u(top), width: u(width), height: u(height), ..default() }
}

/// Whether the device can sample the block-compressed textures the game ships.
pub fn bc_supported(formats: Option<&CompressedImageFormatSupport>) -> bool {
    formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC))
}

/// A line of text: where it starts and the line its letters stand on.
#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub line: Line,
    pub x: f32,
    pub base: f32,
}

/// A font and its texture (the picture and its size in texels); without the texture the
/// text is drawn with Bevy's font.
pub struct Face {
    pub font: Font,
    pub image: Option<(Handle<Image>, Vec2)>,
}

/// A game texture for the interface: not repeated, so that a stretched picture's two
/// ends do not run into each other.
pub fn texture(path: &str, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
    let bytes = data.read(path)?;
    match nvf_assets::texture::dds_image(&bytes, true, bc) {
        Ok(mut image) => {
            image.sampler = ImageSampler::linear();
            let size = image.size().as_vec2();
            Some((images.add(image), size))
        }
        Err(e) => {
            warn!("interface: {path}: {e}");
            None
        }
    }
}

/// A stand-in font for measuring when the game's cannot be read: every character as wide
/// as the next.
pub fn plain_font(size: f32, advance: f32) -> Font {
    let glyph = Glyph { after: advance, ..default() };
    Font { size, texture: String::new(), glyphs: vec![glyph; 256] }
}

/// The game's font at `path`, else `stand_in`.
pub fn face(path: &str, stand_in: Font, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Face {
    let read = data.and_then(|d| {
        let font = match Font::parse(&d.read(path)?) {
            Ok(f) => f,
            Err(e) => {
                warn!("interface: {path}: {e}");
                return None;
            }
        };
        let image = texture(&font.texture_path(), d, images, bc)?;
        Some(Face { font, image: Some(image) })
    });
    read.unwrap_or_else(|| {
        warn!("interface: drawn without the game's font {path}");
        Face { font: stand_in, image: None }
    })
}

/// Draw a line of text at `origin` (what its `x` and `base` are measured from). Every
/// node that carries the colour also gets `mark`.
pub fn set_text(p: &mut ChildSpawnerCommands, face: &Face, set: &Set, origin: Vec2, color: Color, mark: impl Bundle + Clone) {
    set_marked(p, face, &set.line, set.x - origin.x, set.base - origin.y, color, mark);
}

/// Draw a line of text that starts at `x` and stands on `base`, both from the parent's
/// top left corner.
pub fn set_line(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, base: f32, color: Color) {
    set_marked(p, face, line, x, base, color, ());
}

fn set_marked(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, base: f32, color: Color, mark: impl Bundle + Clone) {
    let Some((image, size)) = &face.image else {
        let text: String = line.glyphs.iter().map(|g| g.code as char).collect();
        let size = face.font.size * 0.75;
        p.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(base - size), ..default() }).with_children(|n| {
            n.spawn((Text::new(text), TextFont { font_size: FontSize::Vh(size / 9.6), ..default() }, TextColor(color), mark));
        });
        return;
    };
    for placed in &line.glyphs {
        let g = face.font.glyph(placed.code);
        if !g.drawn() {
            continue;
        }
        let rect = Rect::new(g.uv[0] * size.x, g.uv[1] * size.y, g.uv[2] * size.x, g.uv[3] * size.y);
        p.spawn((
            at(x + placed.x + g.before, base - g.ascent, g.width, g.height),
            ImageNode { rect: Some(rect), color, ..ImageNode::new(image.clone()) },
            mark.clone(),
        ));
    }
}

/// Where a picture lies in an atlas texture: the atlas's file name and the picture's
/// left, top, width and height as parts of the atlas's width and height.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasPiece {
    pub atlas: String,
    pub rect: [f32; 4],
}

/// Read an atlas index (`textures\interface\interfaceshared.tai`), by picture file name
/// in small letters. A line is the picture's file name, then the atlas's file name, a
/// number, a type, and after them the left and top offsets, a depth offset, and the
/// width and height, all as parts of the atlas; `#` starts a comment [documented: the
/// file's own heading; corner-messages.md §2.2]. Lines that do not read so are left out.
pub fn atlas_index(text: &str) -> HashMap<String, AtlasPiece> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = line.split_once(char::is_whitespace) else { continue };
        let fields: Vec<&str> = rest.split(',').map(str::trim).collect();
        let num = |i: usize| fields.get(i).and_then(|f| f.parse::<f32>().ok());
        if let (Some(&atlas), Some(x), Some(y), Some(w), Some(h)) = (fields.first(), num(3), num(4), num(6), num(7)) {
            out.insert(name.to_ascii_lowercase(), AtlasPiece { atlas: atlas.to_ascii_lowercase(), rect: [x, y, w, h] });
        }
    }
    out
}

/// The interface's texture atlas index [documented: corner-messages.md §2.2].
pub const ATLAS_INDEX: &str = "textures\\interface\\interfaceshared.tai";

/// The file name an atlas index knows a picture by.
pub fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase()
}

/// A picture: its texture and the part of it that is the picture (all of it for `None`).
pub type Picture = (Handle<Image>, Option<Rect>);

/// The interface's pictures as its files name them, read when first asked for.
#[derive(Default)]
pub struct Pictures {
    atlas: HashMap<String, AtlasPiece>,
    /// Textures read so far, by path; `None` for one that could not be read.
    textures: HashMap<String, Option<(Handle<Image>, Vec2)>>,
}

impl Pictures {
    pub fn load(data: Option<&GameData>) -> Pictures {
        let atlas = data.and_then(|d| d.read(ATLAS_INDEX)).map(|b| atlas_index(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Pictures { atlas, textures: HashMap::new() }
    }

    fn texture(&mut self, path: String, data: &GameData, images: &mut Assets<Image>, bc: bool) -> Option<(Handle<Image>, Vec2)> {
        self.textures.entry(path).or_insert_with_key(|p| texture(p, data, images, bc)).clone()
    }

    /// A picture named as the interface names it (under the textures folder): its
    /// piece of the atlas when the index has its file name, else its own file.
    pub fn picture(&mut self, path: &str, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Option<Picture> {
        let data = data?;
        if let Some(piece) = self.atlas.get(&file_name(path)).cloned() {
            if let Some((image, size)) = self.texture(format!("textures\\interface\\{}", piece.atlas), data, images, bc) {
                let [x, y, w, h] = piece.rect;
                return Some((image, Some(Rect::new(x * size.x, y * size.y, (x + w) * size.x, (y + h) * size.y))));
            }
        }
        let own = format!("textures\\{}", path.trim_start_matches(['\\', '/']).to_ascii_lowercase());
        self.texture(own, data, images, bc).map(|(image, _)| (image, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atlas_index_is_read() {
        let text = "# Heading\n#\nIcon_A.dds\t\tShared0.dds, 0, 2D, 0.25, 0.5, 0.0, 0.125, 0.0625\n\nnot a line\nb.dds Shared0.dds, 0, 2D, x, 0, 0, 1, 1\n";
        let index = atlas_index(text);
        assert_eq!(index.len(), 1);
        assert_eq!(index["icon_a.dds"], AtlasPiece { atlas: "shared0.dds".into(), rect: [0.25, 0.5, 0.125, 0.0625] });
    }

    #[test]
    fn a_pictures_file_name_is_what_the_atlas_knows_it_by() {
        assert_eq!(file_name("Interface\\Icons\\Message Icons\\Glow_Message_Vaultboy_In_Pain.dds"), "glow_message_vaultboy_in_pain.dds");
        assert_eq!(file_name("a.dds"), "a.dds");
    }
}
