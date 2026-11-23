//! Runtime outline fallback for the interface (docs/spec-notes/fonts.md §2.3).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{futures::check_ready, Task};
use fnv_font::Glyph;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use swash::scale::{Render, ScaleContext, Source};
use swash::{CacheKey, FontDataRef, FontRef, StringId};

static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
#[cfg(not(test))]
static FALLBACK: OnceLock<Mutex<Option<Fallback>>> = OnceLock::new();
static GENERATION: AtomicU32 = AtomicU32::new(1);

/// Keep the path until layout actually encounters a missing CJK glyph.
pub fn configure(path: Option<PathBuf>) {
    let _ = PATH.set(path.filter(|path| !path.as_os_str().is_empty()));
    fnv_font::set_cjk_provider(enabled, |ch, size| {
        lookup(ch, size).map(|(_, glyph)| glyph.metrics)
    });
}

fn enabled_for(code_page: u16, language: &str, path: Option<&Path>) -> bool {
    code_page == 936 || language == "zh" || path.is_some_and(|path| !path.as_os_str().is_empty())
}

pub fn enabled() -> bool {
    #[cfg(test)]
    if TEST_ENABLED.get() {
        return true;
    }
    enabled_for(
        fnv_util::text::code_page().number(),
        fnv_util::words::language(),
        PATH.get().and_then(Option::as_deref),
    )
}

#[cfg(test)]
thread_local! {
    static TEST_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FALLBACK: OnceLock<Mutex<Option<Fallback>>> = const { OnceLock::new() };
}

#[cfg(test)]
pub(crate) fn with_enabled_for_test<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_ENABLED.set(self.0);
        }
    }
    configure(None);
    let _reset = Reset(TEST_ENABLED.replace(true));
    f()
}

/// Changes when the gate changes or a newly loaded source can change layout.
pub(crate) fn revision() -> u32 {
    if enabled() {
        GENERATION.load(Ordering::Relaxed)
    } else {
        0
    }
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

/// A worker skips unreadable candidates and returns just the next usable file.
enum SourcePaths {
    System,
    Explicit(PathBuf),
    Remaining(std::vec::IntoIter<PathBuf>),
}

impl SourcePaths {
    fn load(self) -> Option<LoadedFont> {
        let mut paths = match self {
            Self::System => system_paths().into_iter(),
            Self::Remaining(paths) => paths,
            Self::Explicit(path) => {
                let Some(font) = FontFile::read(&path) else {
                    warn!(
                        "interface: CjkFontPath cannot be read as a font: {}",
                        path.display()
                    );
                    return None;
                };
                return Some(LoadedFont { font, rest: None });
            }
        };
        let font = paths.find_map(|path| FontFile::read(&path))?;
        let rest = (!paths.as_slice().is_empty()).then_some(Self::Remaining(paths));
        Some(LoadedFont { font, rest })
    }
}

struct LoadedFont {
    font: FontFile,
    rest: Option<SourcePaths>,
}

struct Fallback {
    next: Option<SourcePaths>,
    loading: Option<Task<Option<LoadedFont>>>,
    fonts: Vec<FontFile>,
    context: ScaleContext,
    glyphs: HashMap<GlyphKey, Option<Arc<Raster>>>,
}

impl Fallback {
    fn new(next: Option<SourcePaths>) -> Self {
        Self {
            next,
            loading: None,
            fonts: Vec::new(),
            context: ScaleContext::new(),
            glyphs: HashMap::new(),
        }
    }

    fn start(&mut self, load: impl FnOnce() -> Option<LoadedFont> + Send + 'static) {
        self.loading = Some(nvf_assets::read_pool::read_pool().spawn(async move { load() }));
    }

    /// Publish completed reads at the start of a frame so measurement and painting agree.
    fn poll(&mut self) -> bool {
        let Some(loaded) = self.loading.as_mut().and_then(check_ready) else {
            return false;
        };
        self.loading = None;
        let Some(LoadedFont { font, rest }) = loaded else {
            return false;
        };
        self.next = rest;
        self.fonts.push(font);
        self.glyphs.retain(|_, glyph| glyph.is_some());
        true
    }

    fn get(&mut self, key: GlyphKey) -> Option<Arc<Raster>> {
        if let Some(glyph) = self.glyphs.get(&key) {
            return glyph.clone();
        }
        let glyph = self
            .fonts
            .iter()
            .find_map(|font| font.raster(key, &mut self.context))
            .map(Arc::new);
        if glyph.is_none() && self.loading.is_none() {
            if let Some(paths) = self.next.take() {
                self.start(move || paths.load());
            }
        }
        self.glyphs.insert(key, glyph.clone());
        glyph
    }
}

fn with_fallback<T>(f: impl FnOnce(&OnceLock<Mutex<Option<Fallback>>>) -> T) -> T {
    #[cfg(not(test))]
    {
        f(&FALLBACK)
    }
    #[cfg(test)]
    {
        FALLBACK.with(f)
    }
}

/// File reads and parsing happen out of this lock. A pending lookup returns immediately
/// and keeps using the primary face's question mark until the next published result.
pub fn lookup(ch: char, line: f32) -> Option<(GlyphKey, Arc<Raster>)> {
    lookup_when_enabled(enabled(), ch, line, |key| {
        with_fallback(|fallback| {
            let mut slot = fallback.get_or_init(Mutex::default).lock().unwrap();
            let fallback = slot.get_or_insert_with(|| {
                let paths = PATH
                    .get()
                    .and_then(Option::as_ref)
                    .cloned()
                    .map_or(SourcePaths::System, SourcePaths::Explicit);
                Fallback::new(Some(paths))
            });
            fallback.get(key)
        })
    })
}

fn lookup_when_enabled(
    enabled: bool,
    ch: char,
    line: f32,
    get: impl FnOnce(GlyphKey) -> Option<Arc<Raster>>,
) -> Option<(GlyphKey, Arc<Raster>)> {
    if !enabled || !line.is_finite() || line <= 0.0 {
        return None;
    }
    let key = GlyphKey(
        ch,
        (line * DENSITY).round().clamp(1.0, (PAGE / 2) as f32) as u16,
    );
    Some((key, get(key)?))
}

/// Readiness is published here, before UI systems measure or paint anything.
pub(crate) fn update(world: &mut World, mut was_enabled: Local<bool>) {
    let enabled = enabled();
    let disabled = std::mem::replace(&mut *was_enabled, enabled) && !enabled;
    if disabled {
        with_fallback(|fallback| {
            if let Some(fallback) = fallback.get() {
                fallback.lock().unwrap().take();
            }
        });
        world.remove_resource::<Atlas>();
    }
    let loaded = enabled
        && with_fallback(|fallback| {
            fallback.get().is_some_and(|fallback| {
                fallback
                    .lock()
                    .unwrap()
                    .as_mut()
                    .is_some_and(Fallback::poll)
            })
        });
    if loaded {
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    if disabled || loaded {
        crate::pause::letters_changed(world);
        crate::interface::redraw_cjk_lines(world);
    }
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
    fn fallback_requires_code_page_936_chinese_ui_or_a_path() {
        let calls = |page, language, path| {
            let mut calls = 0;
            let active = enabled_for(page, language, path);
            assert!(lookup_when_enabled(active, '中', 24.0, |_| {
                calls += 1;
                None
            })
            .is_none());
            calls
        };
        for (page, language) in [
            (1252, "en"),
            (1251, "ru"),
            (1250, "pl"),
            (1252, "ja"),
            (65001, "de"),
        ] {
            assert_eq!(calls(page, language, None), 0);
            assert_eq!(calls(page, language, Some(Path::new(""))), 0);
        }
        assert_eq!(calls(936, "en", None), 1);
        assert_eq!(calls(1252, "zh", None), 1);
        assert_eq!(calls(1252, "en", Some(Path::new("custom-font.otf"))), 1);
        assert_eq!(calls(1252, "en", None), 0);
    }

    /// Make one synthetic CJK mapping in the already bundled OFL test font.
    fn test_font() -> FontFile {
        let mut bytes = crate::vector_font::FONT_BYTES.to_vec();
        let glyph = FontRef::from_index(&bytes, 0).unwrap().charmap().map('A');
        assert_ne!(glyph, 0);
        let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
        let cmap = bytes[12..12 + 16 * count]
            .as_chunks::<16>()
            .0
            .iter()
            .find(|record| &record[..4] == b"cmap")
            .unwrap();
        let offset = u32::from_be_bytes(cmap[8..12].try_into().unwrap()) as usize;
        let table = [
            0,
            0,
            0,
            1, // cmap version and number of encoding records
            0,
            3,
            0,
            10,
            0,
            0,
            0,
            12, // Windows Unicode full repertoire
            0,
            12,
            0,
            0,
            0,
            0,
            0,
            28, // format 12, size
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            1, // language and number of groups
            0,
            0,
            0x4e,
            0x2d,
            0,
            0,
            0x4e,
            0x2d, // one-code-point range
            0,
            0,
            glyph.to_be_bytes()[0],
            glyph.to_be_bytes()[1],
        ];
        bytes[offset..offset + table.len()].copy_from_slice(&table);
        FontFile::from_bytes(bytes).unwrap()
    }

    #[test]
    fn pending_font_keeps_the_question_mark_then_redraws_the_visible_line() {
        use crate::interface::{original_units, plain_font, set_marked, u, Face};
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        #[derive(Clone, Component)]
        struct Mark;

        with_enabled_for_test(|| {
            let _units = original_units();
            let mut app = App::new();
            app.init_resource::<Assets<Image>>();
            app.add_systems(First, update);
            app.update();
            let bitmap = app
                .world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::default());
            let mut font = plain_font(24.0, 7.0);
            for (ch, uv) in [
                (b'?', [0.0, 0.0, 0.25, 0.25]),
                (b'A', [0.25, 0.0, 0.5, 0.25]),
            ] {
                font.glyphs[ch as usize] = Glyph {
                    uv,
                    width: 4.0,
                    height: 5.0,
                    ascent: 4.0,
                    after: 3.0,
                    ..default()
                };
            }
            let face = Face {
                font,
                image: Some((bitmap.clone(), Vec2::splat(16.0))),
                vector: None,
            };
            let wait = Duration::from_secs(5);
            let (started, running) = mpsc::channel();
            let (resume, pending) = mpsc::channel();
            {
                let mut fallback = Fallback::new(None);
                fallback.start(move || {
                    assert!(std::thread::current()
                        .name()
                        .is_some_and(|name| name.starts_with("NVF Read Pool")));
                    started.send(()).unwrap();
                    pending.recv_timeout(wait).unwrap();
                    Some(LoadedFont {
                        font: test_font(),
                        rest: None,
                    })
                });
                with_fallback(|state| {
                    let mut slot = state.get_or_init(Mutex::default).lock().unwrap();
                    assert!(slot.is_none());
                    *slot = Some(fallback);
                });
            }
            running.recv_timeout(wait).unwrap();
            let line = face.font.lines("中A", None).remove(0);
            assert_eq!(line.width, 14.0);
            assert_eq!(line.glyphs[1].x, 7.0);
            let root = app
                .world_mut()
                .commands()
                .spawn(Node::default())
                .with_children(|p| set_marked(p, &face, &line, 0.0, 20.0, Color::WHITE, Mark))
                .id();
            app.world_mut().flush();

            let drawn = |world: &mut World| {
                let mut query = world.query_filtered::<(&Node, &ImageNode), With<Mark>>();
                query
                    .iter(world)
                    .map(|(node, image)| (node.left, image.image.clone(), image.rect))
                    .collect::<Vec<_>>()
            };
            app.update();
            let before = drawn(app.world_mut());
            assert_eq!(before.len(), 2);
            assert!(before.contains(&(
                u(0.0),
                bitmap.clone(),
                Some(Rect::new(0.0, 0.0, 4.0, 4.0))
            )));
            assert!(before.contains(&(
                u(7.0),
                bitmap.clone(),
                Some(Rect::new(4.0, 0.0, 8.0, 4.0))
            )));

            resume.send(()).unwrap();
            let until = Instant::now() + wait;
            while !with_fallback(|fallback| {
                fallback
                    .get()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .loading
                    .as_ref()
                    .unwrap()
                    .is_finished()
            }) {
                assert!(Instant::now() < until, "font loader did not finish");
                std::thread::yield_now();
            }
            app.update();
            let after = drawn(app.world_mut());
            assert_eq!(after.len(), 2);
            let width = face.font.glyph_of('中').advance();
            assert_ne!(width, 7.0);
            assert!(after.iter().any(|(_, image, _)| *image != bitmap));
            assert!(after.contains(&(
                u(width),
                bitmap.clone(),
                Some(Rect::new(4.0, 0.0, 8.0, 4.0))
            )));
            assert!(app.world().get_entity(root).is_ok());

            TEST_ENABLED.set(false);
            app.update();
            let restored = drawn(app.world_mut());
            assert_eq!(restored.len(), before.len());
            assert!(before.iter().all(|glyph| restored.contains(glyph)));
            assert!(!app.world().contains_resource::<Atlas>());
            assert!(with_fallback(|fallback| fallback
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .is_none()));
        });
    }

    #[test]
    fn outline_metrics_and_atlas_entries_are_reused() {
        let mut fallback = Fallback::new(None);
        let file = FontFile::from_bytes(crate::vector_font::FONT_BYTES.to_vec()).unwrap();
        fallback.fonts.push(file);
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
        let mut fallback = Fallback::new(None);
        let key = GlyphKey('中', 48);
        assert!(FontFile::from_bytes(vec![0; 32]).is_none());
        assert!(fallback.get(key).is_none());
        assert!(matches!(fallback.glyphs.get(&key), Some(None)));
    }
}
