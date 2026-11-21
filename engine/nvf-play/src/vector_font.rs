//! The menus' sharp letters (the owner's menu overhaul, 2026-11-15): a vector font drawn by
//! Bevy's text at the screen's own resolution, in place of the game's raster fonts, whose
//! texels are stretched over two pixels at 4K [observed: the owner's screenshots; the
//! interface is 960 units high and a font texel is one unit].
//!
//! **The font** is Roboto Condensed (variable, weight axis), SIL Open Font License 1.1,
//! Copyright 2011 The Roboto Project Authors, from github.com/google/fonts `ofl/robotocondensed`
//! [documented: its `OFL.txt` beside the file, `engine/nvf-play/assets/fonts/`]; the owner's
//! pick of 2026-11-15 among OFL fonts with Latin and Cyrillic (Fallout 4's own interface
//! font). The game's Monofonto cannot be shipped (a commercial font; its files are game data).
//! The OFL lets the font be bundled with any program; NVForever stays GPL-3.0-or-later and the
//! font stays OFL (its licence text ships with it, tools/release/third_party_licenses.py).
//!
//! **Measured as the raster fonts are**: the pages lay their text out from an
//! `fnv_font::Font` table (`width`, `lines`), never from the texture. [`metric_table`] fills
//! that table from the font file's own advances for this run's code page, so every width,
//! wrap and cut on the page stays as it is. For the drawn text to be as wide as measured,
//! Bevy is asked for no kerning and no ligatures ([`features`]); the regular weight's
//! advances are the `hmtx` table's, the bold titles' (weight 700 on the font's weight axis)
//! those plus the `HVAR` table's deltas [documented: OpenType 1.9 `fvar`, `avar`, `HVAR` and
//! item variation store chapters]. [`read`] reads only what that needs: the `head`, `hhea`,
//! `hmtx`, `cmap`, `OS/2`, `fvar`, `avar` and `HVAR` tables (no new dependency).
//!
//! **Sizes** [design: owner 2026-11-15, second test: "make the overall text size smaller ...
//! do not decrease the line/row height"]: the entries' and rows' letters [`ENTRY_EM`], the
//! small print's [`SMALL_EM`], set here rather than from the game's fonts, whose letter boxes
//! hold their glow too (font 7's H is 30 units, its letter about 24 [observed: the owner's
//! screenshots]); the rows keep their heights.

use bevy::prelude::*;
use fnv_font::{Font, Glyph};

/// The font file, built into the program.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RobotoCondensed[wght].ttf");

/// The em of the entries' and rows' letters and of the small print (the note, the version,
/// "Report a Bug"), in interface units (module notes): cap heights of about 22 and 17.
pub const ENTRY_EM: f32 = 31.0;
pub const SMALL_EM: f32 = 24.0;
/// The weights drawn: the rows' and the titles' (the column, the categories, the tab rail)
/// [design: owner 2026-11-15, "make the menu titles and the category titles bold"].
pub const REGULAR: u16 = 400;
pub const BOLD: u16 = 700;

/// What the page needs of a TrueType font: its em, the line metrics Bevy's text places a
/// line by, the cap and x heights, and each character's advance.
#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub units_per_em: f32,
    /// Above and below the baseline (both positive), as Bevy's text (skrifa) takes them: the
    /// `OS/2` typographic ones where its USE_TYPO_METRICS flag is set, else `hhea`'s.
    pub ascent: f32,
    pub descent: f32,
    pub cap_height: f32,
    pub x_height: f32,
    advances: Vec<u16>,
    /// Character ranges and how each maps to glyphs (cmap format 4 or 12).
    map: Vec<Range>,
    /// How the advances vary with the weight; `None` for a font without a weight axis or
    /// `HVAR` (every weight measured as the default's).
    var: Option<Variation>,
}

/// The font's axes and its advances' variation (module notes): `fvar`'s axes, `avar`'s maps,
/// `HVAR`'s item variation store and advance mapping.
#[derive(Debug, Clone, PartialEq)]
struct Variation {
    /// Each axis's least, default and most value; the weight axis's index.
    axes: Vec<[f32; 3]>,
    weight: usize,
    /// Each axis's `avar` map, (from, to) in normalized coordinates (empty: none).
    avar: Vec<Vec<(f32, f32)>>,
    /// The regions: for each, each axis's start, peak and end.
    regions: Vec<Vec<[f32; 3]>>,
    /// The item variation data: its regions and each item's deltas, one per region.
    data: Vec<(Vec<u16>, Vec<Vec<i32>>)>,
    /// Glyph → (outer, inner); `None`: (0, glyph).
    map: Option<Vec<(u16, u16)>>,
}

fn f2dot14(b: &[u8], at: usize) -> Option<f32> {
    i16_at(b, at).map(|v| f32::from(v) / 16384.0)
}

fn fixed(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(|v| v as i32 as f32 / 65536.0)
}

/// Read the weight axis and the advances' variation; `None` where the font has no weight
/// axis or no `HVAR`, or either does not read.
fn read_variation(b: &[u8]) -> Option<Variation> {
    let (fvar, hvar) = (table(b, b"fvar")?, table(b, b"HVAR")?);
    let (axes_at, count, size) = (u16_at(fvar, 4)? as usize, u16_at(fvar, 8)? as usize, u16_at(fvar, 10)? as usize);
    let mut axes = Vec::with_capacity(count);
    let mut weight = None;
    for i in 0..count {
        let at = axes_at + i * size;
        if fvar.get(at..at + 4)? == b"wght" {
            weight = Some(i);
        }
        axes.push([fixed(fvar, at + 4)?, fixed(fvar, at + 8)?, fixed(fvar, at + 12)?]);
    }
    let weight = weight?;
    let mut avar = vec![Vec::new(); count];
    if let Some(a) = table(b, b"avar") {
        let mut at = 8;
        for map in avar.iter_mut().take(u16_at(a, 6)? as usize) {
            let n = u16_at(a, at)? as usize;
            *map = (0..n).map(|k| Some((f2dot14(a, at + 2 + 4 * k)?, f2dot14(a, at + 4 + 4 * k)?))).collect::<Option<_>>()?;
            at += 2 + 4 * n;
        }
    }
    let store = hvar.get(u32_at(hvar, 4)? as usize..)?;
    let list = store.get(u32_at(store, 2)? as usize..)?;
    let (region_axes, region_count) = (u16_at(list, 0)? as usize, u16_at(list, 2)? as usize);
    let regions = (0..region_count)
        .map(|r| (0..region_axes).map(|a| {
            let at = 4 + 6 * (r * region_axes + a);
            Some([f2dot14(list, at)?, f2dot14(list, at + 2)?, f2dot14(list, at + 4)?])
        }).collect::<Option<Vec<_>>>())
        .collect::<Option<Vec<_>>>()?;
    let data = (0..u16_at(store, 6)? as usize)
        .map(|i| {
            let d = store.get(u32_at(store, 8 + 4 * i)? as usize..)?;
            let (items, word_field, n) = (u16_at(d, 0)? as usize, u16_at(d, 2)?, u16_at(d, 4)? as usize);
            let (long, words) = (word_field & 0x8000 != 0, (word_field & 0x7FFF) as usize);
            let indexes: Vec<u16> = (0..n).map(|k| u16_at(d, 6 + 2 * k)).collect::<Option<_>>()?;
            let (big, small) = if long { (4, 2) } else { (2, 1) };
            let row = words * big + n.saturating_sub(words) * small;
            let first = 6 + 2 * n;
            let rows = (0..items)
                .map(|item| {
                    let mut at = first + item * row;
                    (0..n).map(|k| {
                        let size = if k < words { big } else { small };
                        let v = match size {
                            4 => u32_at(d, at)? as i32,
                            2 => i32::from(i16_at(d, at)?),
                            _ => i32::from(*d.get(at)? as i8),
                        };
                        at += size;
                        Some(v)
                    }).collect::<Option<Vec<i32>>>()
                })
                .collect::<Option<Vec<_>>>()?;
            Some((indexes, rows))
        })
        .collect::<Option<Vec<_>>>()?;
    let map = match u32_at(hvar, 8)? as usize {
        0 => None,
        at => {
            let m = hvar.get(at..)?;
            let (format, entry) = (*m.first()?, *m.get(1)?);
            let (count, first) = if format == 0 { (u16_at(m, 2)? as usize, 4) } else { (u32_at(m, 2)? as usize, 6) };
            let (size, inner_bits) = (usize::from((entry >> 4) & 3) + 1, u32::from(entry & 0xF) + 1);
            let entries = (0..count)
                .map(|i| {
                    let v = m.get(first + i * size..first + (i + 1) * size)?.iter().fold(0u32, |v, b| (v << 8) | u32::from(*b));
                    Some(((v >> inner_bits) as u16, (v & ((1 << inner_bits) - 1)) as u16))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(entries)
        }
    };
    Some(Variation { axes, weight, avar, regions, data, map })
}

impl Variation {
    /// The normalized coordinates of weight `w` (the other axes at their defaults): `fvar`'s
    /// normalization, then `avar`'s map, kept to F2DOT14 as a renderer keeps them.
    fn coords(&self, w: f32) -> Vec<f32> {
        let mut out = vec![0.0; self.axes.len()];
        let [min, def, max] = self.axes[self.weight];
        let v = w.clamp(min, max);
        let mut n = if v < def && def > min {
            (v - def) / (def - min)
        } else if v > def && max > def {
            (v - def) / (max - def)
        } else {
            0.0
        };
        let map = &self.avar[self.weight];
        if let Some(k) = map.windows(2).position(|p| n >= p[0].0 && n <= p[1].0) {
            let ((f0, t0), (f1, t1)) = (map[k], map[k + 1]);
            n = if f1 > f0 { t0 + (n - f0) * (t1 - t0) / (f1 - f0) } else { t0 };
        }
        out[self.weight] = (n * 16384.0).round() / 16384.0;
        out
    }

    /// Glyph `g`'s advance delta at `coords`, in font units, rounded as shapers round it.
    fn delta(&self, g: u16, coords: &[f32]) -> f32 {
        let (outer, inner) = match &self.map {
            Some(m) => m.get(usize::from(g)).or(m.last()).copied().unwrap_or((0, g)),
            None => (0, g),
        };
        let Some((indexes, rows)) = self.data.get(usize::from(outer)) else { return 0.0 };
        let Some(row) = rows.get(usize::from(inner)) else { return 0.0 };
        let sum: f32 = indexes.iter().zip(row).map(|(r, d)| self.regions.get(usize::from(*r)).map_or(0.0, |reg| scalar(reg, coords)) * *d as f32).sum();
        sum.round()
    }
}

/// How much a region counts at `coords` [documented: OpenType item variation store,
/// "Algorithm for interpolation of instance values"].
fn scalar(region: &[[f32; 3]], coords: &[f32]) -> f32 {
    let mut s = 1.0;
    for (&[start, peak, end], &v) in region.iter().zip(coords) {
        if peak == 0.0 || start > peak || peak > end || (start < 0.0 && end > 0.0) || v == peak {
            continue;
        }
        if v <= start || v >= end {
            return 0.0;
        }
        s *= if v < peak { (v - start) / (peak - start) } else { (end - v) / (end - peak) };
    }
    s
}

#[derive(Debug, Clone, PartialEq)]
struct Range {
    start: u32,
    end: u32,
    /// The glyph of `start` (format 12), or the format 4 rule.
    to: Map,
}

#[derive(Debug, Clone, PartialEq)]
enum Map {
    /// Glyph = code + delta (format 4 without a glyph array; format 12 as start glyph).
    Delta(i64),
    /// Glyph read from a format-4 glyph array, by offset from `start`.
    Array(Vec<u16>, i32),
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn i16_at(b: &[u8], at: usize) -> Option<i16> {
    u16_at(b, at).map(|v| v as i16)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

/// The table `tag`'s bytes.
fn table<'a>(b: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let n = u16_at(b, 4)? as usize;
    (0..n).find_map(|i| {
        let at = 12 + 16 * i;
        (b.get(at..at + 4)? == tag).then_some(())?;
        let (off, len) = (u32_at(b, at + 8)? as usize, u32_at(b, at + 12)? as usize);
        b.get(off..off + len)
    })
}

/// Read a TrueType font's metrics (module notes); the reason when it is not one.
pub fn read(b: &[u8]) -> Result<Metrics, String> {
    let need = |tag: &[u8; 4]| table(b, tag).ok_or_else(|| format!("no {} table", String::from_utf8_lossy(tag)));
    let head = need(b"head")?;
    let hhea = need(b"hhea")?;
    let hmtx = need(b"hmtx")?;
    let cmap = need(b"cmap")?;
    let os2 = table(b, b"OS/2");
    let units_per_em = f32::from(u16_at(head, 18).ok_or("short head")?);
    let metrics = hhea.len() >= 36;
    if !metrics || units_per_em <= 0.0 {
        return Err("short hhea or no em".into());
    }
    let n_metrics = u16_at(hhea, 34).unwrap_or(0) as usize;
    let advances: Vec<u16> = (0..n_metrics).filter_map(|i| u16_at(hmtx, 4 * i)).collect();
    let (mut ascent, mut descent) = (f32::from(i16_at(hhea, 4).unwrap_or(0)), -f32::from(i16_at(hhea, 6).unwrap_or(0)));
    let (mut cap_height, mut x_height) = (0.7 * units_per_em, 0.5 * units_per_em);
    if let Some(os2) = os2 {
        let selection = u16_at(os2, 62).unwrap_or(0);
        if selection & (1 << 7) != 0 {
            ascent = f32::from(i16_at(os2, 68).unwrap_or(0));
            descent = -f32::from(i16_at(os2, 70).unwrap_or(0));
        }
        if u16_at(os2, 0).unwrap_or(0) >= 2 {
            x_height = f32::from(i16_at(os2, 86).unwrap_or(0)).max(1.0);
            cap_height = f32::from(i16_at(os2, 88).unwrap_or(0)).max(1.0);
        }
    }
    Ok(Metrics { units_per_em, ascent, descent, cap_height, x_height, advances, map: read_cmap(cmap)?, var: read_variation(b) })
}

/// The Unicode subtable's ranges: format 12 (3,10 or 0,4…) before format 4 (3,1 or 0,3).
fn read_cmap(c: &[u8]) -> Result<Vec<Range>, String> {
    let n = u16_at(c, 2).ok_or("short cmap")? as usize;
    let subtables: Vec<(u16, u16, usize)> = (0..n).filter_map(|i| Some((u16_at(c, 4 + 8 * i)?, u16_at(c, 6 + 8 * i)?, u32_at(c, 8 + 8 * i)? as usize))).collect();
    let unicode = |p: u16, e: u16| p == 0 || (p == 3 && (e == 1 || e == 10));
    let format = |off: usize| u16_at(c, off).unwrap_or(0);
    if let Some(&(_, _, off)) = subtables.iter().find(|(p, e, off)| unicode(*p, *e) && format(*off) == 12) {
        let groups = u32_at(c, off + 12).ok_or("short cmap 12")? as usize;
        return Ok((0..groups)
            .filter_map(|g| {
                let at = off + 16 + 12 * g;
                let (start, end, glyph) = (u32_at(c, at)?, u32_at(c, at + 4)?, u32_at(c, at + 8)?);
                Some(Range { start, end, to: Map::Delta(i64::from(glyph) - i64::from(start)) })
            })
            .collect());
    }
    let &(_, _, off) = subtables.iter().find(|(p, e, off)| unicode(*p, *e) && format(*off) == 4).ok_or("no Unicode cmap (format 4 or 12)")?;
    let segs = u16_at(c, off + 6).ok_or("short cmap 4")? as usize / 2;
    let ends = off + 14;
    let starts = ends + 2 * segs + 2;
    let deltas = starts + 2 * segs;
    let offsets = deltas + 2 * segs;
    let mut out = Vec::new();
    for s in 0..segs {
        let (Some(end), Some(start), Some(delta), Some(range_off)) = (u16_at(c, ends + 2 * s), u16_at(c, starts + 2 * s), i16_at(c, deltas + 2 * s), u16_at(c, offsets + 2 * s)) else { continue };
        // The last segment (0xFFFF), and a broken one ending before it starts.
        if start == 0xFFFF || end < start {
            continue;
        }
        let to = if range_off == 0 {
            Map::Delta(i64::from(delta))
        } else {
            // The glyph array entry of `start` lies `range_off` bytes after this offset's slot.
            let first = offsets + 2 * s + range_off as usize;
            let glyphs = (0..=(end - start) as usize).map(|k| u16_at(c, first + 2 * k).unwrap_or(0)).collect();
            Map::Array(glyphs, i32::from(delta))
        };
        out.push(Range { start: u32::from(start), end: u32::from(end), to });
    }
    Ok(out)
}

impl Metrics {
    /// The glyph of character `c` (0, the missing glyph, when the font has none).
    pub fn glyph(&self, c: char) -> u16 {
        let u = c as u32;
        self.map
            .iter()
            .find(|r| (r.start..=r.end).contains(&u))
            .map_or(0, |r| match &r.to {
                Map::Delta(d) => ((i64::from(u) + d) & 0xFFFF) as u16,
                Map::Array(glyphs, d) => match glyphs.get((u - r.start) as usize) {
                    Some(0) | None => 0,
                    Some(&g) => ((i32::from(g) + d) & 0xFFFF) as u16,
                },
            })
    }

    /// Character `c`'s advance as a part of the em, at the default weight.
    pub fn advance(&self, c: char) -> f32 {
        let g = self.glyph(c) as usize;
        let a = self.advances.get(g).or(self.advances.last()).copied().unwrap_or(0);
        f32::from(a) / self.units_per_em
    }

    /// Character `c`'s advance as a part of the em at weight `w` (the default's where the
    /// font does not vary).
    pub fn advance_at(&self, c: char, w: u16) -> f32 {
        let base = self.advance(c);
        match &self.var {
            Some(v) => base + v.delta(self.glyph(c), &v.coords(f32::from(w))) / self.units_per_em,
            None => base,
        }
    }

    /// Whether the font's advances vary with its weight.
    pub fn varies(&self) -> bool {
        self.var.is_some()
    }

    /// Whether the font has a glyph of its own for `c`.
    pub fn has(&self, c: char) -> bool {
        self.glyph(c) != 0
    }

    /// Every character the font has a glyph of its own for.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.map.iter().flat_map(|r| r.start..=r.end).filter_map(char::from_u32).filter(|&c| self.has(c))
    }
}

/// The font's metrics, read once.
pub fn metrics() -> Option<&'static Metrics> {
    static READ: std::sync::OnceLock<Option<Metrics>> = std::sync::OnceLock::new();
    READ.get_or_init(|| match read(FONT_BYTES) {
        Ok(m) => Some(m),
        Err(e) => {
            warn!("interface: the menus' font does not read: {e}");
            None
        }
    })
    .as_ref()
}

/// A metric table of the font at `em` interface units and weight `w` for this run's code
/// page, its lines `line` apart (the raster font's), as `fnv_font::Font` measures (module
/// notes): each character's advance its width, a space's its room after; capitals and figures
/// as tall as the cap height, small letters as the x-height. The font's characters the code
/// page has no code for are measured too (`Font::extra`): the menus' text is laid out in the
/// code page's codes, and a language the install was not made for (Russian on an English
/// install, whose code page is Windows-1252) would otherwise read as `?`.
pub fn metric_table(m: &Metrics, em: f32, line: f32, w: u16) -> Font {
    let page = fnv_util::text::code_page();
    let glyph = |c: char| {
        let adv = m.advance_at(c, w) * em;
        let rise = if c.is_lowercase() { m.x_height } else { m.cap_height } / m.units_per_em * em;
        if c.is_whitespace() || c.is_control() {
            Glyph { after: adv, ..default() }
        } else {
            Glyph { width: adv, height: rise, ascent: rise, ..default() }
        }
    };
    let glyphs = (0..=255u8).map(|code| glyph(page.char_of(code))).collect();
    let extra = m.chars().filter(|&c| page.code_of(c).is_none()).map(|c| (c, glyph(c))).collect();
    Font { size: line, texture: String::new(), glyphs, extra }
}

/// A face of the font at `em` units and weight `w`, measured by [`metric_table`], its lines
/// `line` apart.
pub fn face(m: &Metrics, font: Handle<bevy::text::Font>, em: f32, w: u16, line: f32) -> crate::interface::Face {
    let vector = crate::interface::VectorFace { font, em, weight: w, base: baseline_below_top(m, em) };
    crate::interface::Face { font: metric_table(m, em, line, w), image: None, vector: Some(vector) }
}

/// The OpenType features turned off so the drawn text is as wide as [`metric_table`]
/// measures: kerning and the ligatures (Roboto has `fi`, `fl`).
pub fn features() -> bevy::text::FontFeatures {
    use bevy::text::{FontFeatureTag, FontFeatures};
    FontFeatures::builder()
        .set(FontFeatureTag::new(b"kern"), 0)
        .set(FontFeatureTag::STANDARD_LIGATURES, 0)
        .set(FontFeatureTag::CONTEXTUAL_LIGATURES, 0)
        .set(FontFeatureTag::CONTEXTUAL_ALTERNATES, 0)
        .build()
}

/// Where a line's baseline lies below the top of the text Bevy lays out at `em` with a line
/// height of one em: half the room the line leaves over the ascent and descent, then the
/// ascent [documented: parley 0.11 `line_break.rs`, baseline = ascent + half the leading].
pub fn baseline_below_top(m: &Metrics, em: f32) -> f32 {
    let (asc, desc) = (m.ascent / m.units_per_em * em, m.descent / m.units_per_em * em);
    (em - asc - desc) / 2.0 + asc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_font_reads_and_has_latin_and_cyrillic() {
        let m = read(FONT_BYTES).expect("Roboto Condensed reads");
        assert_eq!(m.units_per_em, 2048.0);
        assert!(m.ascent > 1500.0 && m.descent > 300.0, "{m:?}");
        assert!((m.cap_height / m.units_per_em - 0.71).abs() < 0.02, "cap {}", m.cap_height);
        for c in ['A', 'a', 'Q', 'z', '0', '%', 'é', 'Ж', 'я', 'ß'] {
            assert!(m.has(c), "{c}");
            assert!(m.advance(c) > 0.1 && m.advance(c) < 1.0, "{c}: {}", m.advance(c));
        }
        // Figures are tabular; capitals wider than small letters.
        assert_eq!(m.advance('0'), m.advance('8'));
        assert!(m.advance('M') > m.advance('i'));
        assert!(!m.has('\u{E000}'), "a private-use code: the missing glyph");
    }

    #[test]
    fn the_table_measures_as_the_font_advances() {
        let m = read(FONT_BYTES).unwrap();
        let t = metric_table(&m, 32.0, 36.0, REGULAR);
        // Russian on an English install (code page Windows-1252, no code for Cyrillic): the
        // sharp letters measure and draw the words, not `?` (owner 2026-11-16).
        assert_eq!(fnv_util::text::code_page(), fnv_util::text::CodePage::Windows1252);
        assert!(t.extra.contains_key(&'Ж') && !t.extra.contains_key(&'A'), "only what the code page lacks");
        assert_eq!(t.lines("Частота кадров", None)[0].text_spaced(&t), "Частота кадров");
        let cyr: f32 = "Жя".chars().map(|c| m.advance(c) * 32.0).sum();
        assert!((t.width("Жя") - cyr).abs() < 1e-3 && t.width("Жя") > t.width("??"), "measured as the font's own");
        let text = "Ambient Occlusion 16x";
        let sum: f32 = text.chars().map(|c| m.advance(c) * 32.0).sum();
        assert!((t.width(text) - sum).abs() < 1e-3, "{} vs {sum}", t.width(text));
        assert!(!t.glyph(b' ').drawn() && t.glyph(b' ').advance() > 0.0, "a space has room, no picture");
        assert_eq!(t.size, 36.0);
        assert!(t.glyph(b'H').ascent > t.glyph(b'x').ascent);
        let base = baseline_below_top(&m, 32.0);
        assert!(base > 20.0 && base < 32.0, "{base}");
    }

    #[test]
    fn bold_is_measured_from_the_weight_axis() {
        let m = read(FONT_BYTES).unwrap();
        assert!(m.varies(), "Roboto Condensed has a weight axis and HVAR");
        let v = m.var.as_ref().unwrap();
        assert_eq!(v.axes[v.weight], [100.0, 400.0, 900.0]);
        // The default weight is the hmtx table's advance.
        for c in "AMWaimo0%".chars() {
            assert_eq!(m.advance_at(c, REGULAR), m.advance(c), "{c}");
        }
        // H by hand from the file: hmtx 1270; HVAR's deltas -12, -22, -5, 2 for the regions
        // peaking at -1, 1, -0.5 and 0.694; avar takes 700 (0.6) to 0.6776. So 100: 1270 - 12;
        // 700: 1270 - 22 × 0.6776 + 2 × 0.6776 / 0.6938, rounded; 900: 1270 - 22. The
        // condensed bold is a little narrower, not wider.
        let em = m.units_per_em;
        for (w, units) in [(100, 1258.0), (BOLD, 1257.0), (900, 1248.0)] {
            assert_eq!(m.advance_at('H', w), units / em, "H at {w}");
        }
        // Never more than a tenth of the em apart; the figures stay tabular.
        for c in "AHMWabdemnosw".chars() {
            assert!((m.advance_at(c, BOLD) - m.advance(c)).abs() < 0.1, "{c}");
        }
        let figures: Vec<f32> = "0123456789".chars().map(|c| m.advance_at(c, BOLD)).collect();
        assert!(figures.windows(2).all(|w| w[0] == w[1]), "{figures:?}");
        // A bold title is measured as Bevy draws it.
        let b = metric_table(&m, ENTRY_EM, 36.0, BOLD);
        let sum: f32 = "Graphics".chars().map(|c| m.advance_at(c, BOLD) * ENTRY_EM).sum();
        assert!((b.width("Graphics") - sum).abs() < 1e-3);
        assert_ne!(b.width("Graphics"), metric_table(&m, ENTRY_EM, 36.0, REGULAR).width("Graphics"));
    }

    #[test]
    fn a_broken_file_says_why() {
        assert!(read(b"not a font").is_err());
        assert!(read(&[]).is_err());
    }
}
