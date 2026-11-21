//! Runtime outline fallback for the interface (docs/spec-notes/fonts.md §2.3).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_font::Glyph;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use swash::scale::{Render, ScaleContext, Source};
use swash::{CacheKey, FontDataRef, FontRef, StringId};

static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static FALLBACK: OnceLock<Mutex<Fallback>> = OnceLock::new();

/// Keep the path until layout actually encounters a missing CJK glyph.
pub fn configure(path: Option<PathBuf>) {
    let _ = PATH.set(path);
    fnv_font::set_cjk_metrics(|ch, size| lookup(ch, size).map(|(_, glyph)| glyph.metrics));
}

#[cfg(windows)]
fn system_paths() -> Vec<PathBuf> {
    let Some(root) = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")) else {
        return Vec::new();
    };
    let fonts = PathBuf::from(root).join("Fonts");
    ["msyh.ttc", "simhei.ttf", "simsun.ttc"]
        .map(|name| fonts.join(name))
        .into()
}

#[cfg(target_os = "linux")]
fn system_paths() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/wqy-zenhei/wqy-zenhei.ttc",
    ]
    .map(PathBuf::from)
    .into()
}

#[cfg(target_os = "macos")]
fn system_paths() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Supplemental/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/Supplemental/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    ]
    .map(PathBuf::from)
    .into()
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn system_paths() -> Vec<PathBuf> {
    Vec::new()
}

struct FontFile {
    bytes: Vec<u8>,
    // Preserve Swash's cache keys between glyphs. A TTC stores its faces in one file.
    faces: Vec<(u32, CacheKey)>,
}

impl FontFile {
    fn read(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        let font = Self::from_bytes(bytes)?;
        info!("interface: CJK fallback font {}", path.display());
        Some(font)
    }

    fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        let mut faces: Vec<_> = FontDataRef::new(&bytes)?
            .fonts()
            .map(|font| {
                let names = font.localized_strings();
                // Multiregion collections can put a different region first.
                let simplified = [StringId::TypographicFamily, StringId::Family]
                    .into_iter()
                    .any(|id| {
                        names
                            .find_by_id(id, Some("en"))
                            .or_else(|| names.find_by_id(id, None))
                            .is_some_and(|name| {
                                let family: String = name.chars().collect();
                                family.ends_with(" SC") || family.ends_with(" CN")
                            })
                    });
                (!simplified, (font.offset, font.key))
            })
            .collect();
        faces.sort_by_key(|(other, _)| *other);
        (!faces.is_empty()).then(|| Self {
            bytes,
            faces: faces.into_iter().map(|(_, face)| face).collect(),
        })
    }

    fn raster(&self, key: GlyphKey, context: &mut ScaleContext) -> Option<Raster> {
        self.faces.iter().find_map(|&(offset, font_key)| {
            let font = FontRef {
                data: &self.bytes,
                offset,
                key: font_key,
            };
            raster(font, key, context)
        })
    }
}

// Two texels per interface unit let the same masks serve scaled and sharp menus.
const DENSITY: f32 = 2.0;
const PAGE: u32 = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey(char, u16);

pub struct Raster {
    pub metrics: Glyph,
    width: u32,
    height: u32,
    mask: Vec<u8>,
}

fn raster(font: FontRef<'_>, key: GlyphKey, context: &mut ScaleContext) -> Option<Raster> {
    let id = font.charmap().map(key.0);
    if id == 0 {
        return None;
    }
    // Fit the face's ascender and descender into the primary face's line spacing.
    let metrics = font.metrics(&[]);
    let span = metrics.ascent.abs() + metrics.descent.abs();
    if span == 0.0 || metrics.units_per_em == 0 {
        return None;
    }
    let ppem = f32::from(key.1) * f32::from(metrics.units_per_em) / span;
    let advance = font.glyph_metrics(&[]).scale(ppem).advance_width(id) / DENSITY;
    if advance < 0.0 {
        return None;
    }
    let mut scaler = context.builder(font).size(ppem).build();
    let image = Render::new(&[Source::Outline]).render(&mut scaler, id);
    let Some(image) = image else {
        // A fullwidth space has an advance and no ink.
        return key.0.is_whitespace().then(|| Raster {
            metrics: Glyph {
                after: advance,
                ..default()
            },
            width: 0,
            height: 0,
            mask: Vec::new(),
        });
    };
    let place = image.placement;
    if place.width >= PAGE - 1 || place.height >= PAGE - 1 {
        return None;
    }
    let width = place.width as f32 / DENSITY;
    let before = place.left as f32 / DENSITY;
    Some(Raster {
        metrics: Glyph {
            width,
            height: place.height as f32 / DENSITY,
            before,
            after: advance - before - width,
            ascent: place.top as f32 / DENSITY,
            ..default()
        },
        width: place.width,
        height: place.height,
        mask: image.data,
    })
}

struct Fallback {
    sources: Vec<(PathBuf, OnceLock<Option<FontFile>>)>,
    explicit: bool,
    context: ScaleContext,
    glyphs: HashMap<GlyphKey, Option<Arc<Raster>>>,
}

impl Fallback {
    fn new(paths: Vec<PathBuf>, explicit: bool) -> Self {
        let sources = paths
            .into_iter()
            .map(|path| (path, OnceLock::new()))
            .collect();
        Self {
            sources,
            explicit,
            context: ScaleContext::new(),
            glyphs: HashMap::new(),
        }
    }

    fn get(&mut self, key: GlyphKey) -> Option<Arc<Raster>> {
        self.glyphs
            .entry(key)
            .or_insert_with(|| {
                self.sources.iter().find_map(|(path, font)| {
                    let file = font.get_or_init(|| {
                        let file = FontFile::read(path);
                        if file.is_none() && self.explicit {
                            warn!(
                                "interface: CjkFontPath cannot be read as a font: {}",
                                path.display()
                            );
                        }
                        file
                    });
                    file.as_ref()?.raster(key, &mut self.context).map(Arc::new)
                })
            })
            .clone()
    }
}

pub fn lookup(ch: char, line: f32) -> Option<(GlyphKey, Arc<Raster>)> {
    let path = PATH.get()?;
    if !line.is_finite() || line <= 0.0 {
        return None;
    }
    let key = GlyphKey(
        ch,
        (line * DENSITY).round().clamp(1.0, (PAGE / 2) as f32) as u16,
    );
    let fallback = FALLBACK.get_or_init(|| {
        let fallback = match path {
            Some(path) => Fallback::new(vec![path.clone()], true),
            None => Fallback::new(system_paths(), false),
        };
        Mutex::new(fallback)
    });
    Some((key, fallback.lock().unwrap().get(key)?))
}

#[derive(Resource, Default)]
struct Atlas {
    pages: Vec<Page>,
    glyphs: HashMap<GlyphKey, (Handle<Image>, Rect)>,
}

struct Page {
    image: Handle<Image>,
    x: u32,
    y: u32,
    row: u32,
}

impl Page {
    fn new(images: &mut Assets<Image>) -> Self {
        let size = Extent3d {
            width: PAGE,
            height: PAGE,
            depth_or_array_layers: 1,
        };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[255, 255, 255, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::linear();
        Self {
            image: images.add(image),
            x: 0,
            y: 0,
            row: 0,
        }
    }

    fn put(&mut self, glyph: &Raster, images: &mut Assets<Image>) -> Option<(Handle<Image>, Rect)> {
        let (w, h) = (glyph.width + 2, glyph.height + 2);
        if self.x + w > PAGE {
            self.x = 0;
            self.y += self.row;
            self.row = 0;
        }
        if self.y + h > PAGE {
            return None;
        }
        // A transparent texel around the rectangle isolates adjacent masks when sampled.
        let (x, y) = (self.x + 1, self.y + 1);
        let mut image = images.get_mut(&self.image)?;
        let data = image.data.as_mut()?;
        for (row, mask) in glyph.mask.chunks_exact(glyph.width as usize).enumerate() {
            let at = 4 * ((y as usize + row) * PAGE as usize + x as usize);
            let dst = &mut data[at..at + 4 * mask.len()];
            for (pixel, alpha) in dst.as_chunks_mut::<4>().0.iter_mut().zip(mask) {
                pixel[3] = *alpha;
            }
        }
        self.x += w;
        self.row = self.row.max(h);
        Some((
            self.image.clone(),
            Rect::new(
                x as f32,
                y as f32,
                (x + glyph.width) as f32,
                (y + glyph.height) as f32,
            ),
        ))
    }
}

/// Called by a deferred draw command, where the current world's image assets are available.
pub fn image_node(
    world: &mut World,
    key: GlyphKey,
    glyph: &Raster,
    color: Color,
) -> Option<ImageNode> {
    world.get_resource_or_insert_with(Atlas::default);
    let (image, rect) = world.try_resource_scope(|world, mut atlas: Mut<Atlas>| {
        let Atlas { pages, glyphs } = &mut *atlas;
        if let Some(entry) = glyphs.get(&key) {
            return Some(entry.clone());
        }
        let mut images = world.get_resource_mut::<Assets<Image>>()?;
        let entry = pages
            .last_mut()
            .and_then(|page| page.put(glyph, &mut images))
            .or_else(|| {
                pages.push(Page::new(&mut images));
                pages.last_mut()?.put(glyph, &mut images)
            })?;
        glyphs.insert(key, entry.clone());
        Some(entry)
    })??;
    Some(ImageNode {
        rect: Some(rect),
        color,
        image_mode: NodeImageMode::Stretch,
        ..ImageNode::new(image)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_metrics_and_atlas_entries_are_reused() {
        let mut fallback = Fallback::new(Vec::new(), false);
        let file = FontFile::from_bytes(crate::vector_font::FONT_BYTES.to_vec()).unwrap();
        fallback
            .sources
            .push((PathBuf::new(), OnceLock::from(Some(file))));
        // Use the bundled OFL font to exercise the rasterizer without adding a fixture font.
        let a = GlyphKey('A', 48);
        let glyph = fallback.get(a).unwrap();
        assert!(glyph.metrics.advance() > 0.0 && glyph.metrics.ascent > 0.0);
        assert!(glyph.mask.iter().any(|&alpha| alpha != 0));
        assert!(Arc::ptr_eq(&glyph, &fallback.get(a).unwrap()));
        let larger = fallback.get(GlyphKey('A', 96)).unwrap();
        assert!((larger.metrics.advance() - 2.0 * glyph.metrics.advance()).abs() < 0.01);
        assert!(fallback.get(GlyphKey('中', 48)).is_none());

        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        let latin = image_node(&mut world, a, &glyph, Color::WHITE).unwrap();
        let again = image_node(&mut world, a, &glyph, Color::BLACK).unwrap();
        assert_eq!((latin.image.clone(), latin.rect), (again.image, again.rect));
        let cyrillic = GlyphKey('Ж', 48);
        let other = image_node(
            &mut world,
            cyrillic,
            &fallback.get(cyrillic).unwrap(),
            Color::WHITE,
        )
        .unwrap();
        assert_eq!(latin.image, other.image);
        assert_ne!(latin.rect, other.rect);
    }

    #[test]
    fn a_missing_or_invalid_font_has_no_glyph() {
        let mut fallback = Fallback::new(vec![PathBuf::new()], true);
        let key = GlyphKey('中', 48);
        assert!(FontFile::from_bytes(vec![0; 32]).is_none());
        assert!(fallback.get(key).is_none());
        assert!(matches!(fallback.glyphs.get(&key), Some(None)));
    }
}
