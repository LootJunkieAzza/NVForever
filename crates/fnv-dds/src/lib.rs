//! DDS textures as shipped with Fallout: New Vegas (docs/spec-notes/dds.md).
//!
//! Parses the 128-byte header, names the pixel format, computes the face/mip layout
//! and checks it against the payload size. Block-compressed (DXT) data is meant to be
//! uploaded to the GPU as is; the three uncompressed layouts in the game convert to
//! RGBA8 here.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DdsError {
    #[error("not a DDS file (magic {0:02x?})")]
    BadMagic([u8; 4]),
    #[error("file too short for a DDS header ({0} bytes)")]
    Truncated(usize),
    #[error("unsupported pixel format {0}")]
    Unsupported(String),
    #[error("pixel data is {actual} bytes, the layout needs {expected}")]
    SizeMismatch { expected: usize, actual: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub flags: u32,
    pub fourcc: [u8; 4],
    pub rgb_bits: u32,
    /// Red, green, blue, alpha masks.
    pub masks: [u32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dxt1,
    Dxt3,
    Dxt5,
    A8R8G8B8,
    R8G8B8,
    A4R4G4B4,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Dxt1 => "DXT1",
            Format::Dxt3 => "DXT3",
            Format::Dxt5 => "DXT5",
            Format::A8R8G8B8 => "A8R8G8B8",
            Format::R8G8B8 => "R8G8B8",
            Format::A4R4G4B4 => "A4R4G4B4",
        }
    }

    /// Bytes per 4×4 block for block formats.
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Format::Dxt1 => Some(8),
            Format::Dxt3 | Format::Dxt5 => Some(16),
            _ => None,
        }
    }

    pub fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w as usize, h as usize);
        match self.block_bytes() {
            Some(b) => w.div_ceil(4).max(1) * h.div_ceil(4).max(1) * b,
            None => {
                let bpp = match self {
                    Format::A8R8G8B8 => 4,
                    Format::R8G8B8 => 3,
                    _ => 2,
                };
                w * h * bpp
            }
        }
    }
}

const PF_FOURCC: u32 = 0x4;
const PF_RGB: u32 = 0x40;
const PF_ALPHA_PIXELS: u32 = 0x1;
const CAPS2_CUBEMAP: u32 = 0x200;
const CAPS2_ALL_FACES: u32 = 0xFC00;

impl PixelFormat {
    /// The format named in dds.md §2, or a description of an unknown one.
    pub fn format(&self) -> Result<Format, String> {
        if self.flags & PF_FOURCC != 0 {
            return match &self.fourcc {
                b"DXT1" => Ok(Format::Dxt1),
                b"DXT3" => Ok(Format::Dxt3),
                b"DXT5" => Ok(Format::Dxt5),
                _ => Err(self.describe()),
            };
        }
        if self.flags & PF_RGB != 0 {
            let alpha = self.flags & PF_ALPHA_PIXELS != 0;
            return match (self.rgb_bits, self.masks, alpha) {
                (32, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000], true) => Ok(Format::A8R8G8B8),
                (24, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, _], false) => Ok(Format::R8G8B8),
                (16, [0x0F00, 0x00F0, 0x000F, 0xF000], true) => Ok(Format::A4R4G4B4),
                _ => Err(self.describe()),
            };
        }
        Err(self.describe())
    }

    pub fn describe(&self) -> String {
        format!(
            "unknown(flags=0x{:x},fourcc={},bits={},masks={:08x}/{:08x}/{:08x}/{:08x})",
            self.flags,
            String::from_utf8_lossy(&self.fourcc).replace('\0', ""),
            self.rgb_bits,
            self.masks[0],
            self.masks[1],
            self.masks[2],
            self.masks[3]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsHeader {
    pub flags: u32,
    pub height: u32,
    pub width: u32,
    pub pitch_or_linear_size: u32,
    pub depth: u32,
    pub mip_count: u32,
    pub pixel_format: PixelFormat,
    pub caps: [u32; 4],
}

pub const HEADER_LEN: usize = 128;

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl DdsHeader {
    pub fn parse(data: &[u8]) -> Result<DdsHeader, DdsError> {
        if data.len() >= 4 && &data[..4] != b"DDS " {
            return Err(DdsError::BadMagic(data[..4].try_into().unwrap()));
        }
        if data.len() < HEADER_LEN {
            return Err(DdsError::Truncated(data.len()));
        }
        let h = |i: usize| u32_at(data, 4 + 4 * i);
        Ok(DdsHeader {
            flags: h(1),
            height: h(2),
            width: h(3),
            pitch_or_linear_size: h(4),
            depth: h(5),
            mip_count: h(6),
            pixel_format: PixelFormat {
                flags: u32_at(data, 80),
                fourcc: data[84..88].try_into().unwrap(),
                rgb_bits: u32_at(data, 88),
                masks: [u32_at(data, 92), u32_at(data, 96), u32_at(data, 100), u32_at(data, 104)],
            },
            caps: [u32_at(data, 108), u32_at(data, 112), u32_at(data, 116), u32_at(data, 120)],
        })
    }

    /// Mip levels in the file: the stored count, or 1 when it is 0 (dds.md §3).
    pub fn levels(&self) -> u32 {
        self.mip_count.max(1)
    }

    pub fn is_cubemap(&self) -> bool {
        self.caps[1] & CAPS2_CUBEMAP != 0
    }

    pub fn faces(&self) -> u32 {
        if self.is_cubemap() {
            (self.caps[1] & CAPS2_ALL_FACES).count_ones().max(1)
        } else {
            1
        }
    }

    /// (width, height) of mip level `i`.
    pub fn level_dims(&self, i: u32) -> (u32, u32) {
        (self.width.checked_shr(i).unwrap_or(0).max(1), self.height.checked_shr(i).unwrap_or(0).max(1))
    }

    /// Payload bytes the layout needs (all faces, all levels), saturating on absurd headers.
    pub fn expected_size(&self, f: Format) -> usize {
        let per_face = (0..self.levels().min(32)).fold(0usize, |acc, i| {
            let (w, h) = self.level_dims(i);
            acc.saturating_add(f.level_size(w, h))
        });
        per_face.saturating_mul(self.faces() as usize)
    }
}

/// A parsed texture with its level slices.
#[derive(Debug, Clone)]
pub struct Dds<'a> {
    pub header: DdsHeader,
    pub format: Format,
    /// `levels[face][mip]` slices of the payload.
    pub levels: Vec<Vec<&'a [u8]>>,
}

impl<'a> Dds<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Dds<'a>, DdsError> {
        let header = DdsHeader::parse(data)?;
        let format = header.pixel_format.format().map_err(DdsError::Unsupported)?;
        let payload = &data[HEADER_LEN..];
        let expected = header.expected_size(format);
        if payload.len() != expected || header.levels() > 32 {
            return Err(DdsError::SizeMismatch { expected, actual: payload.len() });
        }
        let mut levels = Vec::new();
        let mut off = 0;
        for _ in 0..header.faces() {
            let mut face = Vec::new();
            for i in 0..header.levels() {
                let (w, h) = header.level_dims(i);
                let n = format.level_size(w, h);
                face.push(&payload[off..off + n]);
                off += n;
            }
            levels.push(face);
        }
        Ok(Dds { header, format, levels })
    }
}

/// Convert one level of an uncompressed format to RGBA8. `None` for block formats.
pub fn to_rgba8(format: Format, src: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        Format::A8R8G8B8 => {
            out.reserve(src.len());
            for p in src.as_chunks::<4>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Format::R8G8B8 => {
            out.reserve(src.len() / 3 * 4);
            for p in src.as_chunks::<3>().0 {
                out.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        Format::A4R4G4B4 => {
            out.reserve(src.len() * 2);
            for p in src.as_chunks::<2>().0 {
                let v = u16::from_le_bytes([p[0], p[1]]);
                let ch = |shift: u16| (((v >> shift) & 0xF) as u8) * 17;
                out.extend_from_slice(&[ch(8), ch(4), ch(0), ch(12)]);
            }
        }
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn build(w: u32, h: u32, mips: u32, pf_flags: u32, fourcc: &[u8; 4], bits: u32, masks: [u32; 4], caps2: u32, payload: usize) -> Vec<u8> {
        let mut d = vec![0u8; HEADER_LEN];
        d[..4].copy_from_slice(b"DDS ");
        let put = |d: &mut Vec<u8>, o: usize, v: u32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut d, 4, 124);
        put(&mut d, 8, 0x1007 | 0x20000);
        put(&mut d, 12, h);
        put(&mut d, 16, w);
        put(&mut d, 28, mips);
        put(&mut d, 76, 32);
        put(&mut d, 80, pf_flags);
        d[84..88].copy_from_slice(fourcc);
        put(&mut d, 88, bits);
        for (k, m) in masks.iter().enumerate() {
            put(&mut d, 92 + 4 * k, *m);
        }
        put(&mut d, 112, caps2);
        d.resize(HEADER_LEN + payload, 0xAB);
        d
    }

    #[test]
    fn dxt1_chain() {
        // 8×4: levels 8×4 (2 blocks), 4×2 (1), 2×1 (1), 1×1 (1) = 5 blocks × 8 bytes.
        let d = build(8, 4, 4, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 40);
        let t = Dds::parse(&d).unwrap();
        assert_eq!(t.format, Format::Dxt1);
        assert_eq!(t.levels[0].iter().map(|l| l.len()).collect::<Vec<_>>(), [16, 8, 8, 8]);
        let short = build(8, 4, 4, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 39);
        assert_eq!(Dds::parse(&short).unwrap_err(), DdsError::SizeMismatch { expected: 40, actual: 39 });
    }

    #[test]
    fn zero_mip_count_means_one_level() {
        let d = build(4, 4, 0, PF_FOURCC, b"DXT5", 0, [0; 4], 0, 16);
        assert_eq!(Dds::parse(&d).unwrap().levels[0].len(), 1);
    }

    #[test]
    fn cubemap_faces() {
        let d = build(4, 4, 1, PF_FOURCC, b"DXT3", 0, [0; 4], CAPS2_CUBEMAP | CAPS2_ALL_FACES, 6 * 16);
        let t = Dds::parse(&d).unwrap();
        assert_eq!(t.header.faces(), 6);
        assert_eq!(t.levels.len(), 6);
    }

    #[test]
    fn uncompressed_formats() {
        let argb = build(1, 1, 1, PF_RGB | PF_ALPHA_PIXELS, &[0; 4], 32, [0xFF0000, 0xFF00, 0xFF, 0xFF000000], 0, 4);
        assert_eq!(Dds::parse(&argb).unwrap().format, Format::A8R8G8B8);
        assert_eq!(to_rgba8(Format::A8R8G8B8, &[1, 2, 3, 4]).unwrap(), [3, 2, 1, 4]);
        assert_eq!(to_rgba8(Format::R8G8B8, &[1, 2, 3]).unwrap(), [3, 2, 1, 255]);
        assert_eq!(to_rgba8(Format::A4R4G4B4, &[0x21, 0xF3]).unwrap(), [0x33, 0x22, 0x11, 0xFF]);
        let odd = build(1, 1, 1, PF_RGB, &[0; 4], 16, [0xF800, 0x7E0, 0x1F, 0], 0, 2);
        assert!(matches!(Dds::parse(&odd), Err(DdsError::Unsupported(_))));
    }

    #[test]
    fn bad_input() {
        assert!(matches!(DdsHeader::parse(b"XXXX"), Err(DdsError::BadMagic(_))));
        assert_eq!(DdsHeader::parse(b"DDS \0\0"), Err(DdsError::Truncated(6)));
    }

    proptest::proptest! {
        #[test]
        fn never_panics(data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..300)) {
            let _ = Dds::parse(&data);
        }

        #[test]
        fn header_fuzz_never_panics(w in 0u32..u32::MAX, h in 0u32..u32::MAX, mips in 0u32..u32::MAX) {
            let d = build(w, h, mips, PF_FOURCC, b"DXT1", 0, [0; 4], 0, 64);
            let _ = Dds::parse(&d);
        }
    }
}
