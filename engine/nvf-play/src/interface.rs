//! What the parts of the original's interface are drawn with, shared by the modules that
//! draw one (`message_menu`, `corner_messages`): lengths in the interface's units (960
//! to the height of the view), the interface's colour, the game's fonts and textures
//! read from the owner's archives at run time, a line of text set as glyphs of the
//! font's texture, and the index of the interface's texture atlas.
//!
//! Without the game's files a font is stood in for by one whose characters are all as
//! wide as each other, and its text is drawn with Bevy's font.

use bevy::image::ImageSampler;
use bevy::prelude::*;
use fnv_font::{Font, Glyph, Line};
use nvf_assets::GameData;
use std::collections::HashMap;

/// The interface's colour [observed: the brightest text of the owner's screenshot,
/// message-menu.md §3].
pub const AMBER: [f32; 3] = [1.0, 182.0 / 255.0, 66.0 / 255.0];

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
    let (x, base) = (set.x - origin.x, set.base - origin.y);
    let Some((image, size)) = &face.image else {
        let text: String = set.line.glyphs.iter().map(|g| g.code as char).collect();
        let size = face.font.size * 0.75;
        p.spawn(Node { position_type: PositionType::Absolute, left: u(x), top: u(base - size), ..default() }).with_children(|n| {
            n.spawn((Text::new(text), TextFont { font_size: FontSize::Vh(size / 9.6), ..default() }, TextColor(color), mark));
        });
        return;
    };
    for placed in &set.line.glyphs {
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
}
