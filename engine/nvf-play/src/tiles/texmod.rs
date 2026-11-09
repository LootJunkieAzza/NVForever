//! #1833: ITR's SetUITexOffset and SetUIAlphaMap drawn by the tile renderer
//! [documented: GECK wiki SetUITexOffset, SetUIAlphaMap; docs/spec-notes/menu-xml.md §9].
//!
//! - What: an offset shifts a tile's texture coordinates, wrapping; an alpha map is a
//!   second texture whose alpha masks the tile [documented: the two pages, as issue #1833
//!   reads them]. The scripts' calls are kept by `fnv_game::nvse_itr1` per tile id
//!   (`ItrState::tex_offsets`, `ItrState::alpha_maps`), so a tile unloaded or replaced
//!   at its path has neither [design].
//! - How: a picture piece with either set carries a [`TexMod`] ([`mark`]); when it is
//!   spawned its texels (the atlas piece or its own file, decoded on the CPU, at most
//!   [`MAX_TEXELS`] by the file's header) are sampled shifted by the offset in whole
//!   texels, wrapping inside the piece, and their alpha multiplied by the mask's, into one
//!   image that takes the picture's place ([`compose`]) [design]. The image has the
//!   picture's texels, more where the mask is more detailed, up to a texel an interface
//!   unit of the size it is drawn at ([`resolution`]). Everything else about the piece
//!   stays as it was, so the fits keep their own behaviour:
//!   - stretched and natural pictures draw the new image where the old one stood;
//!   - a `zoom` picture cut to its box (`draw::Placed::At`) and a clip window cut the new
//!     image as they cut the old;
//!   - a `tile` picture repeats the shifted piece over its box, and the mask lies over
//!     the whole box, once [hypothesis: the mask is sampled on its own, not repeated with
//!     the texture]; a box too large to bake (`crate::repeat::MAX_TEXELS`) keeps Bevy's
//!     tiling of the shifted, masked piece, so its mask repeats [design];
//!   - radial tiles are not drawn by the tile renderer (`draw::pieces`), with or without
//!     an offset;
//!   - the compass strip (`draw::Fit::Strip`): its strip moved on by the offset before the
//!     heading scrolls it, the two adding up, and the mask laid over its window, at the
//!     window's texels ([`strip`], `crate::compass_strip`) [design].
//! - Direction [hypothesis: no page gives it]: the tile shows the texel at its texture
//!   coordinate plus the offset, so an offset of 0.25 moves the picture a quarter of its
//!   width to the left. The mask is stretched over the tile's texture area and is not
//!   shifted [hypothesis: "without changing the tile's screen position or crop window",
//!   GECK wiki SetUITexOffset, quoted in hud.md §14.4]. Its path reads as a tile's
//!   `filename` does, under `textures\` [hypothesis].
//! - A texture whose texels cannot be read (missing, or a format `fnv_dds` does not
//!   decode) leaves the tile as it was without it, said once per texture in the log; the
//!   first drawing of each tile's offset and mask is said once too [design].
//! - A scrolling offset: a piece whose offset alone changed keeps its nodes and image; the
//!   image is baked again in place when the new offset is another texel shift, within a
//!   budget of texels a frame ([`BAKE_BUDGET`], [`State::shift_in_place`]). Each tile's
//!   image and told line are let go with the tile ([`State::prune`]) [design: #1833
//!   review].

use super::draw::{Look, Piece, Placed};
use crate::compass_strip::{StripTexels, StripWindow};
use crate::interface::SizedPicture;
use crate::repeat::{self, MAX_TEXELS};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_game::nvse_itr1::{wrap, ItrState};
use fnv_ui::{TileId, Ui};
use nvf_assets::GameData;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A picture tile's ITR texture state, as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct TexMod {
    /// SetUITexOffset's offset, wrapped into 0-1 (0, 0 when none was set).
    pub offset: (f64, f64),
    /// SetUIAlphaMap's texture, as the script named it.
    pub mask: Option<String>,
}

/// The texture state `itr` keeps for `tile`; none when neither was set, or the offset is
/// 0, 0 with no mask (the tile drawn as it always was) [design].
pub fn of(itr: &ItrState, tile: TileId) -> Option<TexMod> {
    let offset = itr.tex_offsets.get(&tile).map_or((0.0, 0.0), |&(x, y)| (wrap(x), wrap(y)));
    let mask = itr.alpha_maps.get(&tile).filter(|m| !m.trim().is_empty()).cloned();
    (offset != (0.0, 0.0) || mask.is_some()).then_some(TexMod { offset, mask })
}

/// Each picture piece's texture state from `itr` (none without a game); other pieces
/// carry none, whatever was set on their tiles.
pub fn mark(pieces: &mut [Piece], itr: Option<&ItrState>) {
    for p in pieces {
        p.tex = match (&p.look, itr) {
            (Look::Picture { .. }, Some(itr)) => of(itr, p.tile),
            _ => None,
        };
    }
}

/// The game path of an alpha map the script named: under `textures\`, in small letters,
/// as a tile's `filename` (`interface::own_texture`); a path already under it is kept
/// [hypothesis].
pub fn mask_path(name: &str) -> String {
    let p = name.trim().trim_start_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase();
    if p.starts_with("textures\\") {
        p
    } else {
        format!("textures\\{p}")
    }
}

/// An offset (0-1) as whole texels of a side `n` texels long: rounded, wrapped [design].
pub fn texel_shift(offset: f64, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    ((wrap(offset) * n as f64).round() as usize) % n
}

/// RGBA8 texels of `piece` (left, top, width, height in texels) of a `width`-texel-wide
/// RGBA8 picture, each texel the one `offset` of the piece further on, wrapping inside the
/// piece ([`texel_shift`]); `None` when the piece is empty or not inside the picture.
pub fn shifted(src: &[u8], width: usize, piece: [usize; 4], offset: (f64, f64)) -> Option<Vec<u8>> {
    let [px, py, pw, ph] = piece;
    let height = src.len().checked_div(width * 4)?;
    if pw == 0 || ph == 0 || px + pw > width || py + ph > height {
        return None;
    }
    let (dx, dy) = (texel_shift(offset.0, pw), texel_shift(offset.1, ph));
    let mut v = Vec::with_capacity(pw * ph * 4);
    for y in 0..ph {
        let row = (py + (y + dy) % ph) * width;
        for x in 0..pw {
            let i = (row + px + (x + dx) % pw) * 4;
            v.extend_from_slice(&src[i..i + 4]);
        }
    }
    Some(v)
}

/// Multiply the alpha of `rgba` (`width` texels wide) by the alpha of `mask` (`mask_width`
/// wide) stretched over it, each texel taking the mask texel under its centre [design].
pub fn masked(rgba: &mut [u8], width: usize, mask: &[u8], mask_width: usize) {
    let (Some(height), Some(mask_height)) = (rgba.len().checked_div(width * 4), mask.len().checked_div(mask_width * 4)) else { return };
    if height == 0 || mask_height == 0 {
        return;
    }
    let at = |i: usize, n: usize, m: usize| (((i as f64 + 0.5) * m as f64 / n as f64) as usize).min(m - 1);
    for y in 0..height {
        let my = at(y, height, mask_height);
        for x in 0..width {
            let mx = at(x, width, mask_width);
            let a = &mut rgba[(y * width + x) * 4 + 3];
            let m = mask[(my * mask_width + mx) * 4 + 3];
            *a = ((u16::from(*a) * u16::from(m) + 127) / 255) as u8;
        }
    }
}

/// A `tile` picture's repeats baked into one image and its node's size in units
/// (`draw::Baked`).
pub(super) type Baked = (Handle<Image>, f32, f32);

/// A `tile` picture's repeats in texels and their node in units
/// ([`crate::repeat::texels_for`]).
pub(super) type Repeat = ((u32, u32), (f64, f64));

/// The texels a drawing may bake again in place per frame, accrued while unspent up to
/// [`MAX_TEXELS`] (one largest image): a scrolling offset on a large picture is baked
/// again as often as this allows, every frame for a 1024 x 1024 picture, every fourth
/// for a 2048 x 2048 one, its last image shown meanwhile; several scrolling pieces take
/// turns, the one waiting longest first ([`State`]). A piece's first image is always
/// baked (it is spent from the next frames') [design: #1833 review; at most 4 MiB of
/// texels a frame on average].
pub const BAKE_BUDGET: i64 = 1 << 20;

/// The decoded textures' bytes a drawing keeps: past it, the least recently used that
/// nothing holds are let go as a texture is put in, and decoded again when next asked
/// for [design].
const DECODED_BUDGET: usize = 64 << 20;

/// A texture's level 0 as RGBA8.
#[derive(Debug)]
pub struct Texels {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl Texels {
    /// `rgba`, `width` texels a row; `None` for an empty picture or one not of whole rows.
    pub fn new(rgba: Vec<u8>, width: usize) -> Option<Texels> {
        let height = rgba.len().checked_div(width * 4)?;
        (height > 0 && rgba.len() == width * height * 4).then_some(Texels { rgba, width, height })
    }
}

/// The DDS `bytes`' level 0 as RGBA8 when it has at most `max` texels: the size is read
/// from the header before anything is decoded, so an oversized texture costs no more than
/// its file [design: #1833 review]. The error says why, for the log.
pub fn decode(bytes: &[u8], max: u64) -> Result<Texels, String> {
    let dds = fnv_dds::Dds::parse(bytes).map_err(|e| e.to_string())?;
    let (w, h) = (dds.header.width, dds.header.height);
    if w == 0 || h == 0 {
        return Err("it has no texels".into());
    }
    if u64::from(w) * u64::from(h) > max {
        return Err(format!("it is {w} x {h} texels, more than {max}"));
    }
    let level = dds.levels.first().and_then(|f| f.first()).ok_or("it has no level")?;
    let rgba = fnv_dds::decode_blocks(dds.format, w, h, level).or_else(|| fnv_dds::to_rgba8(dds.format, level)).ok_or("its format is not decoded")?;
    Texels::new(rgba, w as usize).filter(|t| t.height == h as usize).ok_or_else(|| "its texels are not whole rows".into())
}

/// The texels across and down a baked image: the source's `across` (one piece, or a
/// `tile` box's repeats), more where the mask has more detail, up to one texel an
/// interface unit of the size it is drawn at (`drawn`, in units), and the source's alone
/// when that would pass [`MAX_TEXELS`] [design: #1833 review, so a small picture
/// stretched over a panel keeps its mask's detail].
pub fn resolution(across: (usize, usize), mask: Option<(usize, usize)>, drawn: (f64, f64)) -> (usize, usize) {
    let Some((mw, mh)) = mask else { return across };
    let up = |a: usize, m: usize, d: f64| {
        let d = if d.is_finite() && d > 0.0 { d.ceil().min(MAX_TEXELS as f64) as usize } else { 0 };
        a.max(m.min(d))
    };
    let out = (up(across.0, mw, drawn.0), up(across.1, mh, drawn.1));
    if out.0 as u64 * out.1 as u64 > MAX_TEXELS {
        across
    } else {
        out
    }
}

/// The `out` texels of a baked image. Each samples, at its centre, the texel of the
/// source's piece `cut` (left, top, width, height) that `across` texels of the piece's
/// repeats put there (`across` the piece's own size: once; a `tile` box's: repeated),
/// moved on `shift` texels and wrapping inside the piece; its alpha times that of the
/// `mask` texel stretched over the image under the same centre. Nearest texels both: a
/// mask finer than the source keeps its detail, the source's texels grow by the same
/// factor [design]. `None` when the piece is not inside the source or a size is 0.
pub fn compose(src: &Texels, cut: [usize; 4], shift: (usize, usize), across: (usize, usize), out: (usize, usize), mask: Option<&Texels>) -> Option<Vec<u8>> {
    let [px, py, pw, ph] = cut;
    if pw == 0 || ph == 0 || px + pw > src.width || py + ph > src.height || across.0 == 0 || across.1 == 0 || out.0 == 0 || out.1 == 0 {
        return None;
    }
    let at = |i: usize, n: usize, m: usize| (((i as f64 + 0.5) * m as f64 / n as f64) as usize).min(m - 1);
    let columns: Vec<(usize, usize)> = (0..out.0).map(|x| (px + (at(x, out.0, across.0) + shift.0) % pw, mask.map_or(0, |m| at(x, out.0, m.width)))).collect();
    let mut v = Vec::with_capacity(out.0 * out.1 * 4);
    for y in 0..out.1 {
        let sy = py + (at(y, out.1, across.1) + shift.1) % ph;
        let my = mask.map_or(0, |m| at(y, out.1, m.height));
        for &(sx, mx) in &columns {
            let i = (sy * src.width + sx) * 4;
            let t = &src.rgba[i..i + 4];
            let a = match mask {
                Some(m) => ((u16::from(t[3]) * u16::from(m.rgba[(my * m.width + mx) * 4 + 3]) + 127) / 255) as u8,
                None => t[3],
            };
            v.extend_from_slice(&[t[0], t[1], t[2], a]);
        }
    }
    Some(v)
}

/// What a baked image was made from besides the shift: the same plan with another shift
/// is baked again in place.
#[derive(Clone, Debug, PartialEq)]
struct Plan {
    path: String,
    cut: [usize; 4],
    across: (usize, usize),
    out: (usize, usize),
    /// The mask's game path, when it was read.
    mask: Option<String>,
}

/// Where a tile's kept bake shows.
enum Target {
    /// Its image, baked again in place (the same handle, so its nodes stay).
    Image(Handle<Image>),
    /// The compass strip's window (`crate::compass_strip::StripWindow::reshift`), whose
    /// strip is moved on again in place.
    Strip,
}

/// A tile's baked image, kept to bake it again in place when only its shift changes.
struct Entry {
    plan: Plan,
    /// The mask the script named (read or not).
    asked: Option<String>,
    shift: (usize, usize),
    target: Target,
}

/// A decoded texture and the frame it was last used in.
struct Cached {
    texels: Arc<Texels>,
    used: u64,
}

/// A piece waiting for the bake budget: the frame it began to wait, its cost, the frame it
/// last asked.
#[derive(Clone, Copy, Debug)]
struct Waiting {
    since: u64,
    cost: i64,
    asked: u64,
}

/// Reads a game file's bytes (`GameData::read`; tests pass their own).
pub type Read<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

/// The most texels the bake budget accrues: two largest images, the most one bake costs
/// (the compass strip's strip and its window), so every bake can run once it has waited
/// long enough [design: #1833 review].
pub const TOKEN_CEILING: i64 = 2 * MAX_TEXELS as i64;

/// The refused textures remembered ([`State`]).
const REFUSED_KEPT: usize = 256;

/// A drawing's texture offsets and alpha maps: the textures decoded, the images baked,
/// the bake budget and what was said in the log.
pub struct State {
    /// The tiles whose first drawing with an offset or a mask was said.
    told_drawn: HashSet<TileId>,
    /// The textures that could not be read, said.
    told_failed: HashSet<String>,
    decoded: HashMap<String, Cached>,
    decoded_bytes: usize,
    /// The textures refused, and why, so they are not read again at each spawn (at most
    /// [`REFUSED_KEPT`], then forgotten).
    refused: HashMap<String, String>,
    baked: HashMap<TileId, Entry>,
    /// The texels a frame adds to `tokens` ([`BAKE_BUDGET`]; tests set less).
    budget: i64,
    /// The decoded bytes kept ([`DECODED_BUDGET`]; tests set less).
    capacity: usize,
    /// The texels left to bake again in place.
    tokens: i64,
    /// The frames painted so far.
    frame: u64,
    waiting: HashMap<TileId, Waiting>,
}

impl Default for State {
    fn default() -> State {
        State {
            told_drawn: HashSet::new(),
            told_failed: HashSet::new(),
            decoded: HashMap::new(),
            decoded_bytes: 0,
            refused: HashMap::new(),
            baked: HashMap::new(),
            budget: BAKE_BUDGET,
            capacity: DECODED_BUDGET,
            tokens: 0,
            frame: 0,
            waiting: HashMap::new(),
        }
    }
}

impl State {
    /// Once a frame, before the pieces are painted: the budget refilled, and the pieces
    /// that did not ask again last frame no longer wait.
    pub fn frame(&mut self) {
        self.frame += 1;
        self.tokens = (self.tokens + self.budget).min(TOKEN_CEILING);
        let now = self.frame;
        self.waiting.retain(|_, w| w.asked + 1 >= now);
    }

    /// Let go of what belongs to tiles no longer drawn with an offset or a mask: their
    /// baked images (by the pieces about to be painted), the textures nothing kept uses
    /// any more and, with the tree, the told lines of tiles no longer loaded, so none
    /// grows with every tile ever modified [design: #1833 review].
    pub fn prune(&mut self, pieces: &[Piece], ui: Option<&Ui>) {
        if !self.baked.is_empty() || !self.waiting.is_empty() {
            let keep: HashSet<TileId> = pieces.iter().filter(|p| p.tex.is_some()).map(|p| p.tile).collect();
            self.baked.retain(|t, _| keep.contains(t));
            self.waiting.retain(|t, _| keep.contains(t));
        }
        if !self.decoded.is_empty() {
            let unused: Vec<String> = self.decoded.keys().filter(|p| !self.pinned(p) && !self.planned(p)).cloned().collect();
            for p in unused {
                self.release(&p);
            }
        }
        if let Some(ui) = ui.filter(|_| !self.told_drawn.is_empty()) {
            self.told_drawn.retain(|t| ui.is_live(*t));
        }
    }

    /// Whether something besides the cache holds texture `path`'s texels: a drawn node (a
    /// strip window's mask) or a bake under way. Only these are never let go.
    fn pinned(&self, path: &str) -> bool {
        self.decoded.get(path).is_some_and(|c| Arc::strong_count(&c.texels) > 1)
    }

    /// Whether a kept bake names texture `path` (its source or mask): kept while there is
    /// room, decoded again when let go ([`State::kept`]).
    fn planned(&self, path: &str) -> bool {
        self.baked.values().any(|e| e.plan.path == path || e.plan.mask.as_deref() == Some(path))
    }

    /// The bytes the cache alone holds: what its capacity counts.
    fn unpinned_bytes(&self) -> usize {
        self.counted_bytes("")
    }

    /// [`State::unpinned_bytes`], counting texture `new` too, which its caller holds only
    /// while it is put in.
    fn counted_bytes(&self, new: &str) -> usize {
        self.decoded.iter().filter(|(p, _)| p.as_str() == new || !self.pinned(p)).map(|(_, c)| c.texels.rgba.len()).sum()
    }

    fn release(&mut self, path: &str) {
        if let Some(c) = self.decoded.remove(path) {
            self.decoded_bytes -= c.texels.rgba.len();
        }
    }

    /// Put `texels` in as texture `path`, keeping the bytes the cache alone holds within
    /// [`DECODED_BUDGET`]: those no kept bake names go first, then the least recently used
    /// (a kept bake decodes its texture again when it next needs it). Texels a node holds
    /// stay, and are not counted [design: #1833 review].
    fn keep(&mut self, path: &str, texels: Arc<Texels>) {
        self.decoded_bytes += texels.rgba.len();
        self.decoded.insert(path.to_string(), Cached { texels, used: self.frame });
        while self.counted_bytes(path) > self.capacity {
            let oldest = self.decoded.iter().filter(|(p, _)| p.as_str() != path && !self.pinned(p)).min_by_key(|(p, c)| (self.planned(p), c.used)).map(|(p, _)| p.clone());
            match oldest {
                Some(p) => self.release(&p),
                None => break,
            }
        }
    }

    /// Texture `path` decoded (at most [`MAX_TEXELS`]), kept; why not, for the log.
    fn texels(&mut self, path: &str, read: Read<'_>) -> Result<Arc<Texels>, String> {
        let now = self.frame;
        if let Some(c) = self.decoded.get_mut(path) {
            c.used = now;
            return Ok(c.texels.clone());
        }
        if let Some(why) = self.refused.get(path) {
            return Err(why.clone());
        }
        match read(path).ok_or_else(|| "it is not in the data".to_string()).and_then(|b| decode(&b, MAX_TEXELS)) {
            Ok(t) => {
                let t = Arc::new(t);
                self.keep(path, t.clone());
                Ok(t)
            }
            Err(why) => {
                if self.refused.len() >= REFUSED_KEPT {
                    self.refused.clear();
                }
                self.refused.insert(path.to_string(), why.clone());
                Err(why)
            }
        }
    }

    fn failed(&mut self, path: &str, what: &str, who: &str, why: &str) {
        if self.told_failed.insert(path.to_string()) {
            warn!("tiles: {who}: the {what} {path} could not be read; the tile is drawn without it (SetUITexOffset / SetUIAlphaMap: {why})");
        }
    }

    fn told(&mut self, tile: TileId, line: impl FnOnce() -> String) {
        if self.told_drawn.insert(tile) {
            info!("{}", line());
        }
    }

    /// Spend `cost` texels for `tile` if the budget allows. The piece that has waited
    /// longest is served first: the others spend only what is left over its cost, so one
    /// scrolling piece cannot take every frame's budget from another, whatever order they
    /// are painted in [design: #1833 review]. A piece refused waits from this frame on.
    fn spend(&mut self, tile: TileId, cost: i64) -> bool {
        let now = self.frame;
        let since = self.waiting.get(&tile).map_or(now, |w| w.since);
        let reserve = self.waiting.iter().filter(|(t, w)| (w.since, **t) < (since, tile)).min_by_key(|(t, w)| (w.since, **t)).map_or(0, |(_, w)| w.cost);
        if self.tokens - reserve >= cost {
            self.tokens -= cost;
            self.waiting.remove(&tile);
            true
        } else {
            self.waiting.insert(tile, Waiting { since, cost, asked: now });
            false
        }
    }

    /// A kept texture, or decoded again when it was let go (its texels spent from the
    /// budget).
    fn kept(&mut self, path: &str, read: Option<Read<'_>>) -> Option<Arc<Texels>> {
        let now = self.frame;
        if let Some(c) = self.decoded.get_mut(path) {
            c.used = now;
            return Some(c.texels.clone());
        }
        let t = self.texels(path, read?).ok()?;
        self.tokens -= (t.rgba.len() / 4) as i64;
        Some(t)
    }

    /// `piece`, whose offset alone changed since its image was baked
    /// ([`only_offset_differs`]), shown at its new offset by baking its kept image (or the
    /// compass strip's window, `strip`) again in place: `Some(true)` when it shows it (or
    /// the new offset is the same texel shift), `Some(false)` when the budget waits a frame
    /// (its last image stays), `None` when nothing is kept to bake again (spawn it). A
    /// texture let go since is decoded again through `read`.
    pub fn shift_in_place(&mut self, piece: &Piece, images: &mut Assets<Image>, read: Option<Read<'_>>, strip: Option<&mut StripWindow>) -> Option<bool> {
        let tex = piece.tex.as_ref()?;
        let e = self.baked.get(&piece.tile)?;
        if e.asked != tex.mask || (matches!(e.target, Target::Strip) && strip.is_none()) {
            return None;
        }
        let plan = e.plan.clone();
        let shift = (texel_shift(tex.offset.0, plan.cut[2]), texel_shift(tex.offset.1, plan.cut[3]));
        if shift == e.shift {
            self.waiting.remove(&piece.tile);
            return Some(true);
        }
        // The image's texels; for the compass strip the strip moved on plus its window
        // baked again under its mask.
        let window = strip.as_ref().map_or(0, |w| {
            let (columns, rows) = w.texels();
            columns * rows
        });
        let cost = (plan.out.0 * plan.out.1) as i64 + if matches!(e.target, Target::Strip) { window as i64 } else { 0 };
        if !self.spend(piece.tile, cost) {
            return Some(false);
        }
        let src = self.kept(&plan.path, read)?;
        let mask = match &plan.mask {
            Some(m) => Some(self.kept(m, read)?),
            None => None,
        };
        match (&self.baked.get(&piece.tile)?.target, strip) {
            (Target::Image(handle), _) => {
                let rgba = compose(&src, plan.cut, shift, plan.across, plan.out, mask.as_deref())?;
                images.get_mut(handle)?.data = Some(rgba);
            }
            (Target::Strip, Some(window)) => {
                let texels = StripTexels::new(shifted(&src.rgba, src.width, plan.cut, tex.offset)?, plan.cut[2])?;
                if !window.reshift(texels, images) {
                    return None;
                }
            }
            (Target::Strip, None) => return None,
        }
        self.baked.get_mut(&piece.tile)?.shift = shift;
        Some(true)
    }
}

/// Whether `new` differs from `old` only by its offset, mask the same: what
/// [`State::shift_in_place`] can answer without spawning the piece again.
pub fn only_offset_differs(old: &Piece, new: &Piece) -> bool {
    match (&old.tex, &new.tex) {
        (Some(a), Some(b)) => a.mask == b.mask && a.offset != b.offset && super::draw::change(&Piece { tex: new.tex.clone(), ..old.clone() }, new) != super::draw::Change::Spawn,
        _ => false,
    }
}

/// What is read besides the piece to bake it.
pub(super) struct Bake<'a> {
    pub state: &'a mut State,
    pub data: &'a GameData,
    pub images: &'a mut Assets<Image>,
    pub ui: Option<&'a Ui>,
}

/// Where a piece's picture comes from and how it stands in its box.
pub(super) struct Drawing<'a> {
    /// Its texture's game path (`interface::Pictures::texture_path`).
    pub path: &'a str,
    pub placed: Placed,
    /// A `tile` picture's repeats when they are baked into one image.
    pub repeat: Option<Repeat>,
}

fn image(texels: Vec<u8>, width: usize, height: usize, images: &mut Assets<Image>) -> Handle<Image> {
    let mut image = Image::new(
        Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        texels,
        TextureFormat::Rgba8UnormSrgb,
        // Kept in the main world too, as it is baked again in place (`compass_strip`'s).
        RenderAssetUsages::default(),
    );
    // Linear, as the baked repeats (`crate::repeat`) [design].
    image.sampler = ImageSampler::linear();
    images.add(image)
}

fn who(piece: &Piece, ui: Option<&Ui>) -> String {
    ui.filter(|ui| ui.is_live(piece.tile)).map_or_else(|| format!("{:?}", piece.tile), |ui| ui.path_of(piece.tile))
}

/// The mask `tex` names, decoded, with its game path; none without one or when it could
/// not be read (said once).
fn read_mask(tex: &TexMod, who: &str, b: &mut Bake<'_>) -> Option<(String, Arc<Texels>)> {
    let path = mask_path(tex.mask.as_deref()?);
    match b.state.texels(&path, &|p| b.data.read(p)) {
        Ok(t) => Some((path, t)),
        Err(why) => {
            b.state.failed(&path, "alpha map", who, &why);
            None
        }
    }
}

/// The picture and the baked repeats a piece with texture state `tex` draws, from the ones
/// it would draw without it (`picture`, `baked`). Either stays as it was when its texture
/// is not read.
pub(super) fn apply(tex: &TexMod, piece: &Piece, d: Drawing<'_>, picture: Option<SizedPicture>, baked: Option<Baked>, mut b: Bake<'_>) -> (Option<SizedPicture>, Option<Baked>) {
    let Some((_, rect, size)) = picture.clone() else { return (picture, baked) };
    let who = who(piece, b.ui);
    let src = match b.state.texels(d.path, &|p| b.data.read(p)) {
        Ok(t) => t,
        Err(why) => {
            b.state.failed(d.path, "texture", &who, &why);
            return (picture, baked);
        }
    };
    let cut = rect.map_or([0, 0, src.width, src.height], |r| repeat::piece_texels(r).map(|v| v as usize));
    let (pw, ph) = (cut[2], cut[3]);
    let mask = read_mask(tex, &who, &mut b);
    // The texels across the image and the size it is drawn at. Bevy's tiling (a `tile` box
    // too large to bake) sizes its repeats by the image's texels, so it keeps the piece's.
    let (across, out, node) = match (d.placed, d.repeat, baked.is_some()) {
        (Placed::Repeat { .. }, Some((count, node)), true) => {
            let across = (count.0 as usize, count.1 as usize);
            (across, resolution(across, mask.as_ref().map(|(_, m)| (m.width, m.height)), node), Some(node))
        }
        (Placed::Repeat { .. }, _, _) => ((pw, ph), (pw, ph), None),
        (Placed::At { width, height }, _, _) => ((pw, ph), resolution((pw, ph), mask.as_ref().map(|(_, m)| (m.width, m.height)), (width, height)), None),
        _ => ((pw, ph), resolution((pw, ph), mask.as_ref().map(|(_, m)| (m.width, m.height)), (piece.rect.width, piece.rect.height)), None),
    };
    let shift = (texel_shift(tex.offset.0, pw), texel_shift(tex.offset.1, ph));
    let Some(rgba) = compose(&src, cut, shift, across, out, mask.as_ref().map(|(_, m)| m.as_ref())) else { return (picture, baked) };
    b.state.tokens = (b.state.tokens - (out.0 * out.1) as i64).max(-(MAX_TEXELS as i64));
    b.state.told(piece.tile, || {
        let alpha = mask.as_ref().map_or(String::new(), |(m, _)| format!(", alpha map {m}"));
        format!("tiles: {who}: texture offset {:.3}, {:.3}{alpha} drawn ({} x {} texels)", tex.offset.0, tex.offset.1, out.0, out.1)
    });
    let handle = image(rgba, out.0, out.1, b.images);
    let plan = Plan { path: d.path.to_string(), cut, across, out, mask: mask.map(|(m, _)| m) };
    b.state.baked.insert(piece.tile, Entry { plan, asked: tex.mask.clone(), shift, target: Target::Image(handle.clone()) });
    match node {
        Some(node) => (picture, Some((handle, node.0 as f32, node.1 as f32))),
        // The new image is the piece alone: an atlas piece's rect becomes all of it, so
        // the piece still reads as one (`draw::placed`); its size stays the texels'.
        None => (Some((handle, rect.map(|_| Rect::new(0.0, 0.0, out.0 as f32, out.1 as f32)), size)), baked),
    }
}

/// The compass strip of a piece with texture state `tex` (`draw::Fit::Strip`): its texels
/// (texture `path`, `rect` its piece) moved on by the offset, wrapping, so the heading's
/// scroll (`crate::compass_strip`) starts from the moved strip and the two add up; and its
/// mask, laid over the strip's window [design: #1833 review]. `None` when its texels are
/// not read (the strip drawn as it was).
pub(super) fn strip(tex: &TexMod, piece: &Piece, path: &str, rect: Option<Rect>, mut b: Bake<'_>) -> Option<(StripTexels, Option<Arc<Texels>>)> {
    let who = who(piece, b.ui);
    let src = match b.state.texels(path, &|p| b.data.read(p)) {
        Ok(t) => t,
        Err(why) => {
            b.state.failed(path, "texture", &who, &why);
            return None;
        }
    };
    let cut = rect.map_or([0, 0, src.width, src.height], |r| repeat::piece_texels(r).map(|v| v as usize));
    let strip = StripTexels::new(shifted(&src.rgba, src.width, cut, tex.offset)?, cut[2])?;
    let mask = read_mask(tex, &who, &mut b);
    // Kept so a new offset moves the strip on in place, the whole strip's texels its cost.
    let shift = (texel_shift(tex.offset.0, cut[2]), texel_shift(tex.offset.1, cut[3]));
    let plan = Plan { path: path.to_string(), cut, across: (cut[2], cut[3]), out: (cut[2], cut[3]), mask: mask.as_ref().map(|(m, _)| m.clone()) };
    b.state.baked.insert(piece.tile, Entry { plan, asked: tex.mask.clone(), shift, target: Target::Strip });
    b.state.told(piece.tile, || {
        let alpha = mask.as_ref().map_or(String::new(), |(m, _)| format!(", alpha map {m}"));
        format!("tiles: {who}: texture offset {:.3}, {:.3}{alpha} drawn on the compass strip", tex.offset.0, tex.offset.1)
    });
    Some((strip, mask.map(|(_, m)| m)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::draw::{Fit, Source};
    use crate::tiles::layout::Rect as TileRect;

    /// A `width` x `height` picture whose texels' red is their number and alpha 255.
    fn numbered(width: usize, height: usize) -> Vec<u8> {
        (0..width * height).flat_map(|i| [i as u8, 0, 0, 255]).collect()
    }

    fn reds(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).map(|t| t[0]).collect()
    }

    fn picture(tile: TileId, fit: Fit) -> Piece {
        Piece {
            tile,
            look: Look::Picture { name: r"Interface\Test\pic.dds".into(), source: Source::Own, fit },
            rect: TileRect { left: 0.0, top: 0.0, width: 64.0, height: 16.0 },
            clip: None,
            colour: [1.0; 4],
            depth: 0.0,
            tex: None,
        }
    }

    #[test]
    fn an_offset_shifts_the_texels_wrapping_inside_the_piece() {
        let src = numbered(4, 2);
        // A quarter across: each texel shows the next one, the last the first.
        assert_eq!(reds(&shifted(&src, 4, [0, 0, 4, 2], (0.25, 0.0)).unwrap()), [1, 2, 3, 0, 5, 6, 7, 4]);
        // Half down: the rows swap.
        assert_eq!(reds(&shifted(&src, 4, [0, 0, 4, 2], (0.0, 0.5)).unwrap()), [4, 5, 6, 7, 0, 1, 2, 3]);
        // An atlas piece (the right 2 x 2) wraps inside itself, never into its neighbours.
        assert_eq!(reds(&shifted(&src, 4, [2, 0, 2, 2], (0.5, 0.5)).unwrap()), [7, 6, 3, 2]);
        // No offset is the piece itself; a piece outside the picture is none.
        assert_eq!(reds(&shifted(&src, 4, [1, 0, 2, 1], (0.0, 0.0)).unwrap()), [1, 2]);
        assert_eq!(shifted(&src, 4, [3, 0, 2, 2], (0.0, 0.0)), None);
        assert_eq!(shifted(&src, 4, [0, 0, 0, 2], (0.0, 0.0)), None);
    }

    #[test]
    fn whole_texels_are_rounded_and_wrapped() {
        assert_eq!(texel_shift(0.5, 64), 32);
        assert_eq!(texel_shift(0.999, 64), 0, "nearer a whole turn than the last texel");
        assert_eq!(texel_shift(-0.25, 4), 3);
        assert_eq!(texel_shift(1.25, 4), 1);
        assert_eq!(texel_shift(0.3, 0), 0);
    }

    #[test]
    fn a_mask_multiplies_the_alpha_stretched_over_the_picture() {
        // A 4 x 1 picture at alpha 255 and 128, under a 2 x 1 mask: opaque, then clear.
        let mut rgba: Vec<u8> = [255u8, 255, 128, 128].iter().flat_map(|&a| [9, 9, 9, a]).collect();
        let mask = [0, 0, 0, 255, 0, 0, 0, 0];
        masked(&mut rgba, 4, &mask, 2);
        assert_eq!(rgba.chunks(4).map(|t| t[3]).collect::<Vec<_>>(), [255, 255, 0, 0]);
        // Colours stay; half alpha over half alpha is a quarter.
        let mut half = vec![10, 20, 30, 128];
        masked(&mut half, 1, &[0, 0, 0, 128], 1);
        assert_eq!(half, [10, 20, 30, 64]);
        // A larger mask: each texel takes the mask texel under its centre.
        let mut one = vec![0, 0, 0, 255, 0, 0, 0, 255];
        let wide: Vec<u8> = [0u8, 50, 100, 150].iter().flat_map(|&a| [0, 0, 0, a]).collect();
        masked(&mut one, 2, &wide, 4);
        assert_eq!(one.chunks(4).map(|t| t[3]).collect::<Vec<_>>(), [50, 150]);
    }

    #[test]
    fn the_stored_state_is_what_the_pieces_carry() {
        let ui = Ui::new();
        let (a, b, c) = (ui.screen(), ui.globals(), ui.strings());
        let mut itr = ItrState::default();
        assert_eq!(of(&itr, a), None);
        itr.tex_offsets.insert(a, (0.25, 0.5));
        itr.alpha_maps.insert(b, r"Interface\mask.dds".into());
        // A zero offset alone draws as before; an offset of a whole turn is zero.
        itr.tex_offsets.insert(c, (1.0, 0.0));
        assert_eq!(of(&itr, a), Some(TexMod { offset: (0.25, 0.5), mask: None }));
        assert_eq!(of(&itr, b), Some(TexMod { offset: (0.0, 0.0), mask: Some(r"Interface\mask.dds".into()) }));
        assert_eq!(of(&itr, c), None);
        // Pictures carry it; a box on a tile with an offset does not; no game, none.
        let mut pieces = vec![picture(a, Fit::Tiled(100.0)), picture(b, Fit::Zoom(50.0)), Piece { look: Look::Box, ..picture(a, Fit::Stretch) }];
        mark(&mut pieces, Some(&itr));
        assert_eq!(pieces[0].tex, Some(TexMod { offset: (0.25, 0.5), mask: None }));
        assert_eq!(pieces[1].tex.as_ref().and_then(|t| t.mask.as_deref()), Some(r"Interface\mask.dds"));
        assert_eq!(pieces[2].tex, None);
        mark(&mut pieces, None);
        assert!(pieces.iter().all(|p| p.tex.is_none()));
        // The piece's fit, box and clip are left alone: only its texture state changes.
        assert_eq!(pieces[0].look, picture(a, Fit::Tiled(100.0)).look);
    }

    #[test]
    fn a_new_offset_or_mask_spawns_the_piece_again() {
        use crate::tiles::draw::{change, Change};
        let ui = Ui::new();
        let plain = picture(ui.screen(), Fit::Stretch);
        let shifted = Piece { tex: Some(TexMod { offset: (0.5, 0.0), mask: None }), ..plain.clone() };
        assert_eq!(change(&plain, &shifted), Change::Spawn);
        let further = Piece { tex: Some(TexMod { offset: (0.75, 0.0), mask: None }), ..plain.clone() };
        assert_eq!(change(&shifted, &further), Change::Spawn);
        let mut moved = further.clone();
        moved.rect.left += 3.0;
        assert_eq!(change(&further, &moved), Change::InPlace, "only moved: kept in place");
    }

    #[test]
    fn mask_paths_read_as_a_tiles_filename() {
        assert_eq!(mask_path(r"Interface\HUD\Mask.dds"), r"textures\interface\hud\mask.dds");
        assert_eq!(mask_path(r"\Textures\Interface/m.dds"), r"textures\interface\m.dds");
    }

    /// `alphas` as a picture one texel high, white.
    fn row(alphas: &[u8]) -> Texels {
        Texels::new(alphas.iter().flat_map(|&a| [255, 255, 255, a]).collect(), alphas.len()).unwrap()
    }

    fn alphas(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).map(|t| t[3]).collect()
    }

    #[test]
    fn a_small_picture_keeps_a_detailed_masks_detail() {
        // One texel stretched over a 4-unit box under a 4-texel mask: four texels, the
        // mask's own alphas, not the one under the picture's single centre.
        let one = row(&[255]);
        let mask = row(&[0, 85, 170, 255]);
        let out = resolution((1, 1), Some((4, 1)), (4.0, 1.0));
        assert_eq!(out, (4, 1));
        assert_eq!(alphas(&compose(&one, [0, 0, 1, 1], (0, 0), (1, 1), out, Some(&mask)).unwrap()), [0, 85, 170, 255]);
        // Drawn 2 units wide: one texel a unit, each the mask texel under its centre.
        let out = resolution((1, 1), Some((4, 1)), (2.0, 1.0));
        assert_eq!(out, (2, 1));
        assert_eq!(alphas(&compose(&one, [0, 0, 1, 1], (0, 0), (1, 1), out, Some(&mask)).unwrap()), [85, 255]);
        // No mask, or one coarser than the picture: the picture's own texels.
        assert_eq!(resolution((8, 2), None, (100.0, 100.0)), (8, 2));
        assert_eq!(resolution((8, 2), Some((2, 1)), (100.0, 100.0)), (8, 2));
        // Past the largest image: the picture's texels alone.
        assert_eq!(resolution((1, 1), Some((4096, 4096)), (5000.0, 5000.0)), (1, 1));
        assert_eq!(resolution((1, 1), Some((4, 4)), (f64::NAN, -1.0)), (1, 1));
    }

    #[test]
    fn composing_shifts_repeats_and_masks_in_one_pass() {
        let src = Texels::new(numbered(4, 2), 4).unwrap();
        // The same as shifting the piece: once across, no mask.
        assert_eq!(compose(&src, [2, 0, 2, 2], (1, 1), (2, 2), (2, 2), None), shifted(&src.rgba, 4, [2, 0, 2, 2], (0.5, 0.5)));
        // A `tile` box of 5 x 1 repeats of the 2 x 1 piece, shifted one texel.
        assert_eq!(reds(&compose(&src, [0, 0, 2, 1], (1, 0), (5, 1), (5, 1), None).unwrap()), [1, 0, 1, 0, 1]);
        // Twice the texels across: each source texel twice.
        assert_eq!(reds(&compose(&src, [0, 0, 2, 1], (0, 0), (2, 1), (4, 1), None).unwrap()), [0, 0, 1, 1]);
        assert_eq!(compose(&src, [3, 0, 2, 2], (0, 0), (2, 2), (2, 2), None), None);
        assert_eq!(compose(&src, [0, 0, 2, 2], (0, 0), (2, 2), (0, 2), None), None);
    }

    /// A 4 x 4 DXT1 texture, one block (written for the test).
    fn dxt1_4x4() -> Vec<u8> {
        let mut b = vec![0u8; 128 + 8];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&4u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(b"DXT1");
        b
    }

    #[test]
    fn a_texture_past_the_budget_is_refused_before_it_is_decoded() {
        let t = decode(&dxt1_4x4(), 16).unwrap();
        assert_eq!((t.width, t.height, t.rgba.len()), (4, 4, 64));
        let err = decode(&dxt1_4x4(), 15).unwrap_err();
        assert!(err.contains("4 x 4"), "{err}");
        assert!(decode(b"not a texture", 16).is_err());
    }

    /// A state that baked `tile`'s 4 x 1 picture (texture "src") unshifted, under the mask
    /// it names, and its image.
    fn baked(tile: TileId, asked: Option<&str>, images: &mut Assets<Image>) -> (State, Handle<Image>) {
        let mut state = State::default();
        let src = Arc::new(Texels::new(numbered(4, 1), 4).unwrap());
        let plan = Plan { path: "src".into(), cut: [0, 0, 4, 1], across: (4, 1), out: (4, 1), mask: None };
        let handle = image(compose(&src, plan.cut, (0, 0), plan.across, plan.out, None).unwrap(), 4, 1, images);
        state.keep("src", src);
        state.baked.insert(tile, Entry { plan, asked: asked.map(String::from), shift: (0, 0), target: Target::Image(handle.clone()) });
        (state, handle)
    }

    #[test]
    fn a_scrolling_offset_bakes_its_image_again_in_place_within_the_budget() {
        let ui = Ui::new();
        let tile = ui.screen();
        let mut images = Assets::<Image>::default();
        let (mut state, handle) = baked(tile, None, &mut images);
        let at = |offset: f64| Piece { tex: Some(TexMod { offset: (offset, 0.0), mask: None }), ..picture(tile, Fit::Stretch) };
        let data = |images: &Assets<Image>| reds(images.get(&handle).and_then(|i| i.data.as_deref()).unwrap());
        // A new offset that rounds to the same texel: nothing baked.
        assert_eq!(state.shift_in_place(&at(0.1), &mut images, None, None), Some(true));
        assert_eq!(data(&images), [0, 1, 2, 3]);
        // A new texel with no budget left: the last image stays a frame.
        assert_eq!(state.shift_in_place(&at(0.25), &mut images, None, None), Some(false));
        assert_eq!(data(&images), [0, 1, 2, 3]);
        // The next frame's budget: baked again into the same image.
        state.frame();
        assert_eq!(state.shift_in_place(&at(0.25), &mut images, None, None), Some(true));
        assert_eq!(data(&images), [1, 2, 3, 0]);
        assert_eq!(images.len(), 1);
        // Another mask, or nothing kept: spawned again.
        let masked = Piece { tex: Some(TexMod { offset: (0.5, 0.0), mask: Some("m.dds".into()) }), ..picture(tile, Fit::Stretch) };
        assert_eq!(state.shift_in_place(&masked, &mut images, None, None), None);
        assert_eq!(State::default().shift_in_place(&at(0.5), &mut images, None, None), None);
    }

    #[test]
    fn only_an_offset_change_is_answered_in_place() {
        let ui = Ui::new();
        let plain = picture(ui.screen(), Fit::Stretch);
        let at = |offset: f64, mask: Option<&str>| Piece { tex: Some(TexMod { offset: (offset, 0.0), mask: mask.map(String::from) }), ..plain.clone() };
        assert!(only_offset_differs(&at(0.25, None), &at(0.5, None)));
        let mut moved = at(0.5, None);
        moved.rect.left += 2.0;
        assert!(only_offset_differs(&at(0.25, None), &moved), "moved as well: still in place");
        assert!(!only_offset_differs(&at(0.25, None), &at(0.25, None)));
        assert!(!only_offset_differs(&at(0.25, None), &at(0.5, Some("m.dds"))));
        assert!(!only_offset_differs(&plain, &at(0.5, None)));
        let mut wider = at(0.5, None);
        wider.rect.width += 2.0;
        assert!(!only_offset_differs(&at(0.25, None), &wider));
    }

    #[test]
    fn what_is_kept_goes_with_the_tiles() {
        let mut ui = Ui::new();
        let src = fnv_ui::MemSource::default().with("menus\\t.xml", r#"<menu name="T"><image name="A"/><image name="B"/></menu>"#);
        ui.load_menu(&src, "menus\\t.xml").unwrap();
        let (a, b) = (ui.find_tile("T/A").unwrap(), ui.find_tile("T/B").unwrap());
        let mut images = Assets::<Image>::default();
        let (mut state, _) = baked(a, None, &mut images);
        state.baked.insert(b, baked(b, None, &mut images).0.baked.remove(&b).unwrap());
        state.told_drawn.extend([a, b]);
        // B drawn with no offset now: its image goes; A keeps its own.
        let pieces = [Piece { tex: Some(TexMod { offset: (0.5, 0.0), mask: None }), ..picture(a, Fit::Stretch) }, picture(b, Fit::Stretch)];
        state.prune(&pieces, Some(&ui));
        assert!(state.baked.contains_key(&a) && !state.baked.contains_key(&b));
        assert_eq!(state.told_drawn.len(), 2, "both still loaded");
        // A unloaded: its told line goes too.
        ui.unload_tile(a);
        state.prune(&[], Some(&ui));
        assert!(state.baked.is_empty());
        assert_eq!(state.told_drawn.iter().collect::<Vec<_>>(), [&b]);
    }

    #[test]
    fn two_scrolling_pieces_both_advance_on_one_budget() {
        let ui = Ui::new();
        let (a, b) = (ui.screen(), ui.globals());
        let mut images = Assets::<Image>::default();
        let (mut state, ha) = baked(a, None, &mut images);
        let (other, hb) = baked(b, None, &mut images);
        state.baked.extend(other.baked);
        // One bake a frame for two pieces scrolling every frame, A always painted first.
        state.budget = 4;
        let at = |t: TileId, k: usize| Piece { tex: Some(TexMod { offset: (k as f64 * 0.25, 0.0), mask: None }), ..picture(t, Fit::Stretch) };
        let first = |images: &Assets<Image>, h: &Handle<Image>| reds(images.get(h).and_then(|i| i.data.as_deref()).unwrap())[0];
        let (mut was_a, mut was_b, mut moved_a, mut moved_b) = (0, 0, 0, 0);
        for k in 1..=8 {
            state.frame();
            state.shift_in_place(&at(a, k), &mut images, None, None);
            state.shift_in_place(&at(b, k), &mut images, None, None);
            let (now_a, now_b) = (first(&images, &ha), first(&images, &hb));
            moved_a += usize::from(now_a != was_a);
            moved_b += usize::from(now_b != was_b);
            (was_a, was_b) = (now_a, now_b);
        }
        // They take turns: neither freezes behind the other.
        assert_eq!((moved_a, moved_b), (4, 4));
    }

    #[test]
    fn the_decoded_textures_stay_within_their_budget_and_go_with_their_tiles() {
        let mut state = State::default();
        // Room for one 32-byte texture besides what nodes hold (not counted).
        state.capacity = 32;
        let texture = || Arc::new(Texels::new(vec![0; 32], 1).unwrap());
        state.frame();
        state.keep("a", texture());
        let node = state.decoded["a"].texels.clone();
        state.frame();
        state.keep("b", texture());
        // A third past the budget: the least recently used that no node holds goes (b; a
        // is held by a node and not counted).
        state.frame();
        state.keep("c", texture());
        assert!(state.decoded.contains_key("a") && !state.decoded.contains_key("b") && state.decoded.contains_key("c"));
        assert_eq!(state.decoded_bytes, 64);
        // Nothing kept uses c: let go; a only while its node holds it.
        state.prune(&[], None);
        assert_eq!(state.decoded.keys().collect::<Vec<_>>(), ["a"]);
        drop(node);
        state.prune(&[], None);
        assert!(state.decoded.is_empty());
        assert_eq!(state.decoded_bytes, 0);
    }

    #[test]
    fn a_bake_whose_texture_was_let_go_is_spawned_again_without_its_data() {
        let ui = Ui::new();
        let tile = ui.screen();
        let mut images = Assets::<Image>::default();
        let (mut state, handle) = baked(tile, None, &mut images);
        state.release("src");
        state.frame();
        let at = Piece { tex: Some(TexMod { offset: (0.5, 0.0), mask: None }), ..picture(tile, Fit::Stretch) };
        // No data to decode it again: spawned (its last image untouched meanwhile).
        assert_eq!(state.shift_in_place(&at, &mut images, None, None), None);
        assert_eq!(reds(images.get(&handle).and_then(|i| i.data.as_deref()).unwrap()), [0, 1, 2, 3]);
    }

    /// A 4 x 1 A8B8G8R8 texture whose texels' red is their number (written for the test).
    fn numbered_dds() -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&1u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&0x41u32.to_le_bytes());
        b[88..92].copy_from_slice(&32u32.to_le_bytes());
        for (k, m) in [0xFFu32, 0xFF00, 0xFF_0000, 0xFF00_0000].iter().enumerate() {
            b[92 + 4 * k..96 + 4 * k].copy_from_slice(&m.to_le_bytes());
        }
        b.extend(numbered(4, 1));
        b
    }

    #[test]
    fn kept_bakes_do_not_pin_their_textures_past_the_capacity() {
        let mut ui = Ui::new();
        let src = fnv_ui::MemSource::default().with("menus\\t.xml", r#"<menu name="T"><image name="A"/><image name="B"/><image name="C"/><image name="D"/><image name="E"/></menu>"#);
        ui.load_menu(&src, "menus\\t.xml").unwrap();
        let tiles: Vec<TileId> = ["A", "B", "C", "D", "E"].iter().map(|n| ui.find_tile(&format!("T/{n}")).unwrap()).collect();
        let bytes = numbered_dds();
        let read = |_: &str| Some(bytes.clone());
        let mut images = Assets::<Image>::default();
        let mut state = State::default();
        // Room for one 16-byte texture: five bakes of five textures keep at most one.
        state.capacity = 16;
        let mut handles = Vec::new();
        for (i, &t) in tiles.iter().enumerate() {
            let path = format!("s{i}");
            let texels = state.texels(&path, &read).unwrap();
            let plan = Plan { path, cut: [0, 0, 4, 1], across: (4, 1), out: (4, 1), mask: None };
            let handle = image(compose(&texels, plan.cut, (0, 0), plan.across, plan.out, None).unwrap(), 4, 1, &mut images);
            state.baked.insert(t, Entry { plan, asked: None, shift: (0, 0), target: Target::Image(handle.clone()) });
            handles.push(handle);
            assert!(state.unpinned_bytes() <= 16);
        }
        assert_eq!(state.decoded.len(), 1);
        // Each still moves on in place, its texture decoded again within the budget.
        state.frame();
        for (t, handle) in tiles.iter().zip(&handles) {
            let at = Piece { tex: Some(TexMod { offset: (0.25, 0.0), mask: None }), ..picture(*t, Fit::Stretch) };
            assert_eq!(state.shift_in_place(&at, &mut images, Some(&read), None), Some(true));
            assert_eq!(reds(images.get(handle).and_then(|i| i.data.as_deref()).unwrap()), [1, 2, 3, 0]);
            assert!(state.unpinned_bytes() <= 16 && state.decoded.len() <= 1);
        }
        assert_eq!(state.tokens, BAKE_BUDGET - 5 * (4 + 4), "each a bake and a decode");
    }

    #[test]
    fn the_compass_strip_is_charged_its_strip_and_its_window() {
        use crate::compass_strip::{bake, texel_offset};
        use crate::hud::COMPASS;
        let ui = Ui::new();
        let tile = ui.screen();
        let mut images = Assets::<Image>::default();
        let rgba: Vec<u8> = (0..2048usize).flat_map(|i| [(i % 251) as u8, (i / 251) as u8, 0, 255]).collect();
        let src = Arc::new(Texels::new(rgba.clone(), 1024).unwrap());
        let mut window = StripWindow::new(StripTexels::new(rgba, 1024).unwrap(), COMPASS.x, 0.0, &mut images).unwrap();
        let (columns, rows) = window.texels();
        let mut state = State::default();
        state.keep("strip", src.clone());
        let plan = Plan { path: "strip".into(), cut: [0, 0, 1024, 2], across: (1024, 2), out: (1024, 2), mask: None };
        state.baked.insert(tile, Entry { plan, asked: None, shift: (0, 0), target: Target::Strip });
        let cost = (1024 * 2 + columns * rows) as i64;
        // A refill smaller than the strip and its window: it waits a frame, then runs.
        state.budget = 2048;
        let at = Piece { tex: Some(TexMod { offset: (0.25, 0.0), mask: None }), ..picture(tile, Fit::Strip) };
        state.frame();
        assert!(cost > state.tokens);
        assert_eq!(state.shift_in_place(&at, &mut images, None, Some(&mut window)), Some(false));
        state.frame();
        assert_eq!(state.shift_in_place(&at, &mut images, None, Some(&mut window)), Some(true));
        assert_eq!(state.tokens, 4096 - cost, "both the strip and its window are paid for");
        let moved = StripTexels::new(shifted(&src.rgba, 1024, [0, 0, 1024, 2], (0.25, 0.0)).unwrap(), 1024).unwrap();
        assert_eq!(images.get(&window.image()).and_then(|i| i.data.clone()).unwrap(), bake(&moved, texel_offset(0.0, 1024), columns));
        // The budget accrues to two largest images, the most one bake costs.
        state.budget = MAX_TEXELS as i64;
        for _ in 0..4 {
            state.frame();
        }
        assert_eq!(state.tokens, TOKEN_CEILING);
        assert!(TOKEN_CEILING >= 2 * MAX_TEXELS as i64);
    }
}
