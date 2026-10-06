//! Streams produced by a reference zlib (CPython's zlib module) at levels 0, 1, 6, 9,
//! covering stored, fixed and dynamic Huffman blocks.
use fnv_inflate::{zlib_decompress, zlib_store};
use std::path::Path;

#[test]
fn reference_vectors() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for name in ["text", "random", "mixed"] {
        let raw = std::fs::read(dir.join(format!("{name}.bin"))).unwrap();
        for lvl in [0, 1, 6, 9] {
            let z = std::fs::read(dir.join(format!("{name}.l{lvl}.zlib"))).unwrap();
            let out = zlib_decompress(&z, Some(raw.len())).unwrap_or_else(|e| panic!("{name} l{lvl}: {e}"));
            assert_eq!(out, raw, "{name} l{lvl}");
            assert_eq!(zlib_decompress(&z, None).unwrap(), raw);
        }
        assert_eq!(zlib_decompress(&zlib_store(&raw), Some(raw.len())).unwrap(), raw);
    }
}

#[test]
fn wrong_expected_size_is_an_error() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let z = std::fs::read(dir.join("text.l6.zlib")).unwrap();
    assert!(zlib_decompress(&z, Some(10)).is_err());
}
