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

#[test]
fn every_truncation_is_an_error() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for lvl in [1, 6, 9] {
        let z = std::fs::read(dir.join(format!("mixed.l{lvl}.zlib"))).unwrap();
        for n in 0..z.len() {
            assert!(zlib_decompress(&z[..n], None).is_err(), "l{lvl} prefix {n}");
        }
    }
}

mod properties {
    use fnv_inflate::{zlib_decompress, zlib_decompress_ex, zlib_store, Trailer};
    use proptest::prelude::*;
    use std::io::Write;

    fn deflate(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    proptest! {
        #[test]
        fn arbitrary_input_is_rejected_or_decoded(data in prop::collection::vec(any::<u8>(), 0..300), hint in prop::option::of(0usize..5000)) {
            let _ = zlib_decompress_ex(&data, hint);
        }

        #[test]
        fn compressed_roundtrip(data in prop::collection::vec(0u8..4, 0..20000), level in 0u32..10) {
            let z = deflate(&data, level);
            prop_assert_eq!(zlib_decompress_ex(&z, Some(data.len())).unwrap(), (data.clone(), Trailer::Verified));
            prop_assert_eq!(zlib_decompress(&z, None).unwrap(), data.clone());
            prop_assert_eq!(zlib_decompress(&zlib_store(&data), Some(data.len())).unwrap(), data);
        }

        /// A corrupted stream is rejected, or its output has the expected size and the
        /// trailer status is exactly what the stored Adler-32 says. (Adler-32 is weak:
        /// a corruption that preserves it cannot be detected by any zlib reader; one
        /// such single-bit case was found by this test.)
        #[test]
        fn corrupted_streams_report_their_trailer(data in prop::collection::vec(0u8..4, 1..5000), at in any::<prop::sample::Index>(), flip in 1u8..=255) {
            let mut z = deflate(&data, 6);
            let i = at.index(z.len());
            z[i] ^= flip;
            if let Ok((out, t)) = zlib_decompress_ex(&z, Some(data.len())) {
                // The trailer is read right after the deflate data, which a corruption
                // can end early, so it is not necessarily the last 4 bytes of `z`.
                prop_assert_eq!(out.len(), data.len());
                let computed = fnv_inflate::adler32(&out);
                let present = z.windows(4).any(|w| w == computed.to_be_bytes());
                match t {
                    Trailer::Verified => prop_assert!(present),
                    Trailer::Mismatch { stored, computed: c } => {
                        prop_assert_eq!(c, computed);
                        prop_assert_ne!(stored, c);
                    }
                    Trailer::Missing => {}
                }
            }
        }
    }
}
