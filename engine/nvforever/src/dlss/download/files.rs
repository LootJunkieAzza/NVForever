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

/// The files the download installs, the interposer first: today the four of `dlss::FILES`
/// (DLSS Frame Generation will add its own). The sizes and hashes are the files of
/// `streamline-sdk-v2.14.16.zip`'s `bin/x64/` [observed: the official release archive,
/// 2026-11-13]; the three `sl.*` hashes are `preflight::RELEASE_DLLS`'s (a test keeps them
/// equal) and `nvngx_dlss.dll`'s is [`NGX_PINS`]'s, kept here only: the loader pins no
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
];

/// The NGX runtimes the download fetches: name, size and SHA-256. `nvngx_dlss.dll` 310.9.16,
/// the DLSS version tested with #2206 [observed: the official release archive, 2026-11-13].
pub(super) const NGX_PINS: [(&str, u64, &str); 1] = [("nvngx_dlss.dll", 58_956_912, "d00b5b0ab29c3ca101b70c3a5ecb3a01b7ef33766a3383a28a18623083e6123e")];

#[cfg(test)]
mod tests {
    use super::super::super::preflight::{RELEASE_DLLS, UPDATER_NAME, parse_sha256};
    use super::*;

    #[test]
    fn the_download_is_the_loaders_four_files_in_its_order() {
        let names: Vec<&str> = DOWNLOAD_FILES.iter().map(|f| f.name).collect();
        assert_eq!(names, super::super::super::FILES);
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
    }

    #[test]
    fn the_updater_is_never_listed() {
        for file in DOWNLOAD_FILES {
            assert!(!file.name.eq_ignore_ascii_case(UPDATER_NAME));
            assert!(!file.zip_path.to_ascii_lowercase().contains("nvngx_update"));
        }
    }
}
