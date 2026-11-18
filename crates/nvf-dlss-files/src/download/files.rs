//! The files the download fetches, and their pins: the entry each comes from in NVIDIA's
//! archive, its size and its SHA-256. Changed only with an NVForever release that was tested
//! with the new files (the module's "Pins").

/// One file of the download: its name in the DLSS folder (from this list only, never from the
/// archive), its entry in the archive, its size and its SHA-256 in lower-case hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadFile {
    pub name: &'static str,
    pub zip_path: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// [design] The files the download installs: `dlss::FILES` first, then `dlss::FG_FILES`.
/// The sizes and hashes are the files of
/// `streamline-sdk-v2.14.16.zip`'s `bin/x64/` [observed: the official release archive,
/// 2026-11-13]; the six `sl.*` hashes are `preflight::RELEASE_DLLS`'s (a test keeps them
/// equal) and the two NGX hashes are [`NGX_PINS`]'s, kept here only: the loader pins no
/// `nvngx_*.dll`, as players may bring another DLSS version (`preflight.rs`).
pub const DOWNLOAD_FILES: &[DownloadFile] = &[
    DownloadFile {
        name: "sl.interposer.dll",
        zip_path: "bin/x64/sl.interposer.dll",
        size: 652_928,
        sha256: "b1d7395aebd7b9722841d64dcf7433680020b6a8e421f851c16d5ba0c2182edf",
    },
    DownloadFile {
        name: "sl.common.dll",
        zip_path: "bin/x64/sl.common.dll",
        size: 843_904,
        sha256: "38eea3b4209afc455636ea372b5933d65fcc13becec6c84d101dcfc4d8648f75",
    },
    DownloadFile { name: "sl.dlss.dll", zip_path: "bin/x64/sl.dlss.dll", size: 422_016, sha256: "18871eea87c90345da97d8109faa81ea7d5840e8d99b51b55b0e46cfa177f84e" },
    DownloadFile { name: NGX_PINS[0].0, zip_path: "bin/x64/nvngx_dlss.dll", size: NGX_PINS[0].1, sha256: NGX_PINS[0].2 },
    DownloadFile { name: "sl.dlss_g.dll", zip_path: "bin/x64/sl.dlss_g.dll", size: 637_056, sha256: "3ed9538df724993bea8a050195a6a78b9c8c8a566d5b05ade477aab3c390e90f" },
    DownloadFile { name: NGX_PINS[1].0, zip_path: "bin/x64/nvngx_dlssg.dll", size: NGX_PINS[1].1, sha256: NGX_PINS[1].2 },
    DownloadFile { name: "sl.reflex.dll", zip_path: "bin/x64/sl.reflex.dll", size: 388_736, sha256: "443577e869352436ff7ec8f4d9c90418d3e95368436ce219261e0f4f07a38122" },
    DownloadFile { name: "sl.pcl.dll", zip_path: "bin/x64/sl.pcl.dll", size: 360_064, sha256: "5698c38e722318681f215c9fe68e45b9bedd51378acc699d8eee26e961e57770" },
];

/// The NGX runtimes the download fetches: name, size and SHA-256 [observed: the official
/// release archive, 2026-11-13]. `nvngx_dlss.dll` is 310.9.16 (tested with #2206).
pub(super) const NGX_PINS: [(&str, u64, &str); 2] = [
    ("nvngx_dlss.dll", 58_956_912, "d00b5b0ab29c3ca101b70c3a5ecb3a01b7ef33766a3383a28a18623083e6123e"),
    ("nvngx_dlssg.dll", 7_506_544, "8d59d49b36410dd6b62fef2503a5871d347415bd85d943fa0848563a95dd50c7"),
];

#[cfg(test)]
mod tests {
    use crate::{RELEASE_DLLS, UPDATER_NAME, parse_sha256};
    use super::*;

    #[test]
    fn the_download_is_the_loaders_four_files_then_frame_generation_in_order() {
        let names: Vec<&str> = DOWNLOAD_FILES.iter().map(|f| f.name).collect();
        assert_eq!(names.len(), 8);
        assert_eq!(names[..4], crate::FILES);
        assert_eq!(names[4..], crate::FG_FILES);
    }

    #[test]
    fn every_pin_parses_and_each_entry_is_bin_x64_of_its_name() {
        for file in DOWNLOAD_FILES {
            assert!(parse_sha256(file.sha256).is_some(), "{}", file.name);
            assert_eq!(file.sha256, file.sha256.to_ascii_lowercase(), "{}: lower-case hex", file.name);
            assert_eq!(file.zip_path, format!("bin/x64/{}", file.name));
            assert!(file.size > 0);
        }
    }

    #[test]
    fn the_streamline_hashes_are_the_loaders_release_pins() {
        for file in DOWNLOAD_FILES.iter().filter(|f| f.name.starts_with("sl.")) {
            let pinned = RELEASE_DLLS.iter().find(|(name, _)| *name == file.name).map(|(_, hex)| *hex);
            assert_eq!(pinned, Some(file.sha256), "{}", file.name);
        }
        let ngx: Vec<&str> = DOWNLOAD_FILES.iter().filter(|f| !f.name.starts_with("sl.")).map(|f| f.name).collect();
        assert_eq!(ngx, NGX_PINS.iter().map(|p| p.0).collect::<Vec<_>>(), "every other file is an NGX pin");
        for (name, size, sha256) in NGX_PINS {
            let file = DOWNLOAD_FILES.iter().find(|f| f.name == name).unwrap();
            assert_eq!((file.size, file.sha256), (size, sha256));
        }
    }

    #[test]
    fn the_updater_is_never_listed() {
        for file in DOWNLOAD_FILES {
            assert!(!file.name.eq_ignore_ascii_case(UPDATER_NAME));
            assert!(!file.zip_path.to_ascii_lowercase().contains("nvngx_update"));
            assert!(!file.name.eq_ignore_ascii_case("NvLowLatencyVk.dll"), "Reflex uses the driver's VK_NV_low_latency2; no fallback DLL downloaded");
        }
    }
}
