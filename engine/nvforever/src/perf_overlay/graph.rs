//! The frame graph row: the last [`GRAPH_SECS`] of frame times as a thin line, the newest
//! at the right, with a faint line at a 60 Hz frame ([`REF_MS`]) [design: issue #1868].
//!
//! It is one UI image (`super::draw`) whose pixels are written again at each reading,
//! twice a second, not every frame: a changed `Image` asset is prepared for the GPU
//! again (a new texture made and filled), which every frame would cost more than the
//! line is worth, and the readout beside it changes twice a second too [design]. The
//! pixels are white with the line's opacity; the image node tints them in the HUD's
//! colour, so a change of colour needs no new pixels.
//!
//! The x axis is time: each column holds the slowest frame that ran in its slice of the
//! seconds drawn, so a spike shorter than a column is never lost, and a long frame spans
//! the columns its time covers. The y axis runs from 0 at the bottom to [`scale_top`].

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::readings::GRAPH_SECS;

/// The reference line's frame time: a 60 Hz frame, ms.
pub const REF_MS: f32 = 1000.0 / 60.0;

/// The line's and the reference line's opacity (0 to 255) [design: the reference faint].
const LINE_ALPHA: u8 = 255;
const REF_ALPHA: u8 = 70;

/// The slowest frame time (ms) of each of `width` columns over the last [`GRAPH_SECS`],
/// the oldest first, from the frame times newest first; 0 where no frame ran (the
/// overlay has not shown that long). `out` keeps its room from one reading to the next.
pub fn columns(newest_first: impl Iterator<Item = f32>, width: usize, out: &mut Vec<f32>) {
    out.clear();
    out.resize(width, 0.0);
    if width == 0 {
        return;
    }
    let per = GRAPH_SECS / width as f32;
    let mut ago = 0.0;
    for ms in newest_first {
        if ago >= GRAPH_SECS {
            break;
        }
        let end = ago + ms * 1e-3;
        // The columns this frame's time covers, counted from the right.
        let first = ((ago / per) as usize).min(width - 1);
        let last = ((end / per).ceil() as usize).saturating_sub(1).clamp(first, width - 1);
        for c in first..=last {
            let i = width - 1 - c;
            out[i] = out[i].max(ms);
        }
        ago = end;
    }
}

/// The frame time at the graph's top, ms: the slowest frame drawn, or two 60 Hz frames
/// when every frame was faster, so steady frames sit in the lower half and a spike
/// reaches the top whatever its size [design].
pub fn scale_top(cols: &[f32]) -> f32 {
    cols.iter().copied().fold(2.0 * REF_MS, f32::max)
}

/// The pixel row (0 the top) of a frame time on a graph `height` pixels high whose top
/// is `top` ms; a time above the top is drawn at the top.
pub fn pixel_row(ms: f32, top: f32, height: usize) -> usize {
    let span = height.saturating_sub(1) as f32;
    let f = if top > 0.0 { (ms / top).clamp(0.0, 1.0) } else { 0.0 };
    ((1.0 - f) * span).round() as usize
}

/// Write the graph of `cols` ([`columns`]) into RGBA pixels `cols.len()` wide and
/// `height` high: the reference line across, then the line, each column joined to the
/// one before it by a vertical run. Whether any column held a frame.
pub fn paint(cols: &[f32], height: usize, pixels: &mut [u8]) -> bool {
    let width = cols.len();
    pixels.fill(0);
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return false;
    }
    let top = scale_top(cols);
    let mut put = |x: usize, y: usize, alpha: u8| {
        let i = (y * width + x) * 4;
        pixels[i..i + 3].fill(255);
        pixels[i + 3] = pixels[i + 3].max(alpha);
    };
    let ref_row = pixel_row(REF_MS, top, height);
    for x in 0..width {
        put(x, ref_row, REF_ALPHA);
    }
    let mut before: Option<usize> = None;
    let mut any = false;
    for (x, &ms) in cols.iter().enumerate() {
        if ms <= 0.0 {
            before = None;
            continue;
        }
        any = true;
        let y = pixel_row(ms, top, height);
        let (a, b) = before.map_or((y, y), |p| (p.min(y), p.max(y)));
        for row in a..=b {
            put(x, row, LINE_ALPHA);
        }
        before = Some(y);
    }
    any
}

/// A clear image `width` x `height` for the graph, kept on the CPU too so its pixels can
/// be written again, drawn pixel for pixel (nearest sampling).
pub fn new_image(width: u32, height: u32) -> Image {
    let mut image = Image::new_fill(size(width, height), TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = ImageSampler::nearest();
    image
}

pub fn size(width: u32, height: u32) -> Extent3d {
    Extent3d { width, height, depth_or_array_layers: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_time_has_its_row_from_the_bottom_up() {
        // 33 rows: 0 at the bottom (row 32), the top at row 0, half in the middle.
        assert_eq!(pixel_row(0.0, 40.0, 33), 32);
        assert_eq!(pixel_row(40.0, 40.0, 33), 0);
        assert_eq!(pixel_row(20.0, 40.0, 33), 16);
        assert_eq!(pixel_row(10.0, 40.0, 33), 24);
        assert_eq!(pixel_row(400.0, 40.0, 33), 0, "above the top: at the top");
        assert_eq!(pixel_row(10.0, 0.0, 33), 32);
        assert_eq!(pixel_row(10.0, 40.0, 1), 0);
    }

    #[test]
    fn the_top_is_the_slowest_frame_or_two_sixty_hertz_frames() {
        assert_eq!(scale_top(&[]), 2.0 * REF_MS);
        assert_eq!(scale_top(&[8.0, 16.0, 0.0]), 2.0 * REF_MS);
        assert_eq!(scale_top(&[16.0, 120.0, 16.0]), 120.0);
        // A steady 60 Hz sits at the middle; the spike reaches the top.
        assert_eq!(pixel_row(REF_MS, scale_top(&[REF_MS]), 33), 16);
        assert_eq!(pixel_row(120.0, scale_top(&[16.0, 120.0]), 33), 0);
    }

    #[test]
    fn columns_are_slices_of_time_holding_their_slowest_frame() {
        // 64 frames a second (15.625 ms, exact in binary) for 6 s, with a 125 ms spike
        // that ended 1 s before the newest frame; 10 columns of half a second each.
        let mut frames = vec![15.625; 64];
        frames.push(125.0);
        frames.extend(std::iter::repeat_n(15.625, 64 * 5));
        let mut cols = Vec::new();
        columns(frames.iter().copied(), 10, &mut cols);
        assert_eq!(cols.len(), 10);
        // The spike ran from 1 s to 1.125 s ago: the third column from the right.
        assert_eq!(cols[7], 125.0);
        for (i, &c) in cols.iter().enumerate().filter(|(i, _)| *i != 7) {
            assert_eq!(c, 15.625, "column {i}");
        }
        // 1 s of frames: the older columns are empty.
        columns(std::iter::repeat_n(15.625, 64), 10, &mut cols);
        assert_eq!(cols, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 15.625, 15.625]);
        // A frame longer than a column fills each column it covers.
        columns([1250.0].into_iter(), 10, &mut cols);
        assert_eq!(&cols[7..], [1250.0; 3]);
        assert_eq!(&cols[..7], [0.0; 7]);
        columns(std::iter::empty(), 0, &mut cols);
        assert!(cols.is_empty());
    }

    #[test]
    fn the_line_is_painted_joined_with_the_reference_faint() {
        let (w, h) = (4, 9);
        let mut px = vec![7u8; w * h * 4];
        // Top 40 ms: 10 ms at row 6, 40 ms at row 0.
        let cols = [0.0, 10.0, 40.0, 10.0];
        assert!(paint(&cols, h, &mut px));
        let alpha = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        let ref_row = pixel_row(REF_MS, 40.0, h);
        assert_eq!(ref_row, 5);
        // The empty first column: only the reference line.
        assert_eq!((0..h).map(|y| alpha(0, y)).collect::<Vec<_>>(), [0, 0, 0, 0, 0, REF_ALPHA, 0, 0, 0]);
        // The second: a point at row 6.
        assert_eq!((0..h).filter(|&y| alpha(1, y) == LINE_ALPHA).collect::<Vec<_>>(), [6]);
        // The third: joined from row 6 up to row 0.
        assert_eq!((0..h).filter(|&y| alpha(2, y) == LINE_ALPHA).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6]);
        // The fourth: joined back down.
        assert_eq!((0..h).filter(|&y| alpha(3, y) == LINE_ALPHA).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6]);
        // Nothing to draw: no line, the reference only.
        assert!(!paint(&[0.0; 4], h, &mut px));
        assert!(px.chunks(4).all(|p| p[3] == 0 || p[3] == REF_ALPHA));
    }
}
