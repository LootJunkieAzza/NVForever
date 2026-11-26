use crate::source::process;
use crate::DdsCmd;
use fnv_dds::DdsHeader;
use serde_json::{json, Value};

pub fn run(cmd: DdsCmd) -> anyhow::Result<Value> {
    match cmd {
        DdsCmd::List { archive } => list(&archive),
    }
}

/// One file's row, from its headers alone (no pixel data is decoded).
fn row(path: &str, data: &[u8]) -> Value {
    match DdsHeader::parse(data) {
        Ok(h) => {
            // Sized whole (every array element, face, level and volume slice), or `null`
            // for a DX10 layout the listing does not size (dds.md §5).
            let (format, expected) = match (h.format_name(), h.format()) {
                (Ok(name), Ok(f)) => (name.to_string(), h.layout_sized().then(|| h.expected_size(f))),
                (Err(desc), _) | (_, Err(desc)) => (desc, None),
            };
            let (dxgi, array) = (h.dx10.map(|x| x.dxgi_format), h.dx10.map(|x| x.array_size));
            json!({"path": path, "width": h.width, "height": h.height, "mip_count": h.mip_count,
                   "levels": h.levels(), "format": format, "dxgi_format": dxgi, "cubemap": h.is_cubemap(), "faces": h.faces(),
                   "array_size": array, "expected_size": expected, "actual_size": data.len() - h.data_offset(), "error": null})
        }
        Err(e) => error_row(path, &e.to_string()),
    }
}

fn error_row(path: &str, e: &str) -> Value {
    json!({"path": path, "width": null, "height": null, "mip_count": null, "levels": null, "format": null,
           "dxgi_format": null, "cubemap": null, "faces": null, "array_size": null, "expected_size": null, "actual_size": null, "error": e})
}

fn list(archive: &std::path::Path) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["dds"], None, row)?;
    let files: Vec<Value> = items
        .into_iter()
        .map(|it| it.result.unwrap_or_else(|e| error_row(&it.path, &e)))
        .collect();
    let name = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({"schema": "fnv.dds.listing/1", "archive": name, "files": files}))
}

#[cfg(test)]
mod tests {
    use super::row;
    use serde_json::json;

    /// A header generated here: `fourcc` with the FourCC flag, then for `DX10` the
    /// extension (DXGI code, 2D, no flags, an array of one), then `payload` bytes.
    fn file(fourcc: &[u8; 4], dxgi: u32, w: u32, h: u32, mips: u32, payload: usize) -> Vec<u8> {
        layout(fourcc, [dxgi, 3, 0, 1, 0], w, h, 0, mips, payload)
    }

    /// [`file`] with the extension's five values and the header's depth given.
    fn layout(fourcc: &[u8; 4], ext: [u32; 5], w: u32, h: u32, depth: u32, mips: u32, payload: usize) -> Vec<u8> {
        let mut d = vec![0u8; 128];
        d[..4].copy_from_slice(b"DDS ");
        for (o, v) in [(4usize, 124u32), (8, 0x2_1007), (12, h), (16, w), (24, depth), (28, mips), (76, 32), (80, 4), (108, 0x1000)] {
            d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        d[84..88].copy_from_slice(fourcc);
        if fourcc == b"DX10" {
            for v in ext {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.resize(d.len() + payload, 0);
        d
    }

    /// The listing sizes DX10 arrays, cube arrays and volumes whole, from the headers alone.
    #[test]
    fn dx10_arrays_cube_arrays_and_volumes_are_listed_whole() {
        let pick = |r: &serde_json::Value| (r["faces"].clone(), r["array_size"].clone(), r["expected_size"].clone(), r["cubemap"].clone());
        // Two 4×4 BC7 textures: two blocks.
        let r = row("arr.dds", &layout(b"DX10", [98, 3, 0, 2, 0], 4, 4, 0, 1, 32));
        assert_eq!(pick(&r), (json!(1), json!(2), json!(32), json!(false)));
        assert_eq!(r["actual_size"], json!(32));
        // Two cubes of 4×4 BC7, 2 levels: 6 faces × 2 cubes × (1 + 1) blocks.
        let r = row("cubes.dds", &layout(b"DX10", [98, 3, 0x4, 2, 0], 4, 4, 0, 2, 0));
        assert_eq!(pick(&r), (json!(6), json!(2), json!(6 * 2 * 2 * 16), json!(true)));
        // A 4×4×4 BC7 volume, 3 levels: 4, 2 and 1 slices of one block.
        let r = row("vol.dds", &layout(b"DX10", [98, 4, 0, 1, 0], 4, 4, 4, 3, 0));
        assert_eq!(pick(&r), (json!(1), json!(1), json!(7 * 16), json!(false)));
        // Layouts not sized: an array of none, an unknown dimension, a volume array.
        for ext in [[98, 3, 0, 0, 0], [98, 0, 0, 1, 0], [98, 4, 0, 2, 0]] {
            assert_eq!(row("x.dds", &layout(b"DX10", ext, 4, 4, 4, 1, 16))["expected_size"], json!(null), "{ext:?}");
        }
        // A volume with only the legacy cube bits: listed as a cube, so not sized.
        let mut cube_bits = layout(b"DX10", [98, 4, 0, 1, 0], 4, 4, 4, 1, 16);
        cube_bits[112..116].copy_from_slice(&0xFE00u32.to_le_bytes());
        let r = row("vc.dds", &cube_bits);
        assert_eq!((r["cubemap"].clone(), r["expected_size"].clone()), (json!(true), json!(null)));
        // A legacy volume (capability bit 0x200000) is not sized either.
        let mut legacy_volume = file(b"DXT1", 0, 4, 4, 1, 32);
        legacy_volume[24..28].copy_from_slice(&4u32.to_le_bytes());
        legacy_volume[112..116].copy_from_slice(&0x20_0000u32.to_le_bytes());
        assert_eq!(row("lv.dds", &legacy_volume)["expected_size"], json!(null));
        // A legacy file has no array size.
        assert_eq!(row("l.dds", &file(b"DXT1", 0, 4, 4, 1, 8))["array_size"], json!(null));
    }

    #[test]
    fn dx10_rows_name_the_dxgi_format_and_count_from_148() {
        // 8×8 BC7 sRGB, 4 levels: 4 + 1 + 1 + 1 blocks of 16 bytes, 3 bytes over.
        let r = row(r"textures\pbr\a.dds", &file(b"DX10", 99, 8, 8, 4, 7 * 16 + 3));
        assert_eq!(r["format"], json!("BC7_UNORM_SRGB"));
        assert_eq!(r["dxgi_format"], json!(99));
        assert_eq!(r["expected_size"], json!(112));
        assert_eq!(r["actual_size"], json!(115));
        assert_eq!((r["cubemap"].clone(), r["faces"].clone(), r["error"].clone()), (json!(false), json!(1), json!(null)));
        // BC4 through DX10: 8 bytes a block.
        let r = row("b.dds", &file(b"DX10", 80, 4, 4, 1, 8));
        assert_eq!((r["format"].clone(), r["expected_size"].clone(), r["actual_size"].clone()), (json!("BC4_UNORM"), json!(8), json!(8)));
        // An unknown code is described, with no expected size.
        let r = row("c.dds", &file(b"DX10", 95, 4, 4, 1, 16));
        assert_eq!((r["format"].clone(), r["dxgi_format"].clone(), r["expected_size"].clone()), (json!("unknown(dxgi=95)"), json!(95), json!(null)));
        // A legacy file keeps its name, no DXGI code, and counts from 128.
        let r = row("d.dds", &file(b"DXT1", 0, 4, 4, 1, 8));
        assert_eq!((r["format"].clone(), r["dxgi_format"].clone(), r["actual_size"].clone()), (json!("DXT1"), json!(null), json!(8)));
    }

    #[test]
    fn a_dx10_file_cut_inside_its_extension_is_an_error_row() {
        let whole = file(b"DX10", 98, 4, 4, 1, 16);
        for len in [128, 147] {
            let r = row("e.dds", &whole[..len]);
            assert_eq!(r["error"], json!(format!("file too short for a DDS header ({len} bytes)")));
            assert_eq!((r["format"].clone(), r["dxgi_format"].clone()), (json!(null), json!(null)));
        }
        // Both headers whole and no pixel data: listed, with the shortfall visible.
        let r = row("e.dds", &whole[..148]);
        assert_eq!((r["expected_size"].clone(), r["actual_size"].clone(), r["error"].clone()), (json!(16), json!(0), json!(null)));
    }
}
