//! What the screens drawn from the game's own interface files share: the interface's
//! units and colour, its textures and its fonts (docs/spec-notes/fonts.md).
//!
//! Lengths are in the interface's units, 960 to the height of the view
//! (docs/spec-notes/message-menu.md §2). Fonts and textures are read from the owner's
//! archives at run time; without them a text is drawn with Bevy's font.

use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats, ImageSampler};
use bevy::prelude::*;
use fnv_font::{Font, Glyph, Line};
use nvf_assets::GameData;

/// The interface's colour [observed: the brightest text of the owner's screenshots].
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

/// Whether the device can sample the block-compressed textures the game ships.
pub fn bc_supported(formats: Option<&CompressedImageFormatSupport>) -> bool {
    formats.is_none_or(|f| f.0.contains(CompressedImageFormats::BC))
}

/// A game texture for the interface (the picture and its size in texels): not repeated,
/// so that a stretched picture's two ends do not run into each other.
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

/// A font and its texture; without the texture the text is drawn with Bevy's font.
pub struct Face {
    pub font: Font,
    pub image: Option<(Handle<Image>, Vec2)>,
}

/// A stand-in font for measuring when the game's cannot be read: every character as wide
/// as the next.
pub fn plain_font(size: f32, advance: f32) -> Font {
    let glyph = Glyph { after: advance, ..default() };
    Font { size, texture: String::new(), glyphs: vec![glyph; 256] }
}

/// Read a font of the game's and its texture, or take `stand_in`.
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

/// Draw a line of text that starts at `x` and stands on `base`, both from the parent's
/// top left corner.
pub fn set_line(p: &mut ChildSpawnerCommands, face: &Face, line: &Line, x: f32, base: f32, color: Color) {
    let Some((image, size)) = &face.image else {
        let text: String = line.glyphs.iter().map(|g| g.code as char).collect();
        let size = face.font.size * 0.75;
        p.spawn((
            Node { position_type: PositionType::Absolute, left: u(x), top: u(base - size), ..default() },
            children![(Text::new(text), TextFont { font_size: FontSize::Vh(size / 9.6), ..default() }, TextColor(color))],
        ));
        return;
    };
    for placed in &line.glyphs {
        let g = face.font.glyph(placed.code);
        if !g.drawn() {
            continue;
        }
        let rect = Rect::new(g.uv[0] * size.x, g.uv[1] * size.y, g.uv[2] * size.x, g.uv[3] * size.y);
        p.spawn((at(x + placed.x + g.before, base - g.ascent, g.width, g.height), ImageNode { rect: Some(rect), color, ..ImageNode::new(image.clone()) }));
    }
}
