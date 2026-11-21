//! The DLSS files, shared by the game (`engine/nvforever/src/dlss/`, `dlss_download/`) and the
//! launcher (`crates/nvf-launcher`) so both run the one download (#2765):
//!
//! - [`download`]: Download DLSS… (#2250, #2409): the eight upscaling and frame generation
//!   files from NVIDIA's own Streamline release, only after the player accepted NVIDIA's
//!   licence ([`LICENCE`], [`download::LicenceAccepted`]), each pinned and checked, written only
//!   when all pass;
//! - [`security`] (Windows): the signature checks of `sl.interposer.dll` and NVIDIA's
//!   `nvngx_*.dll`, which the game's loader also runs before loading them;
//! - the DLSS folder ([`resolve_folder`]: `NVF_DLSS_DIR`, else `dlss` beside the program), the
//!   files it must hold ([`FILES`], [`FG_FILES`]) and the release's pins ([`RELEASE_DLLS`]).
//!
//! Moved here unchanged from the game, apart from the paths: its log lines go through `tracing`
//! (the game's log collects them as before) and the developer-only test address of the
//! download is given by the caller ([`download::start`]). The FFI of [`security`] is translated
//! from Streamline's public headers under the MIT licence (`LICENSE-MIT`).

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

pub mod download;
#[cfg(windows)]
pub mod security;

/// NVIDIA's licence for the downloaded `nvngx_*.dll`, as it comes in their archive
/// (`streamline-sdk-v2.14.16.zip`, `bin/x64/nvngx_dlss.license.txt`): NVIDIA's text, not under
/// NVForever's licence, here only so the player can read it before accepting (LEGAL.md). Never
/// edited; the game's test pins its SHA-256.
pub const LICENCE: &str = include_str!("nvngx_dlss.license.txt");

/// The folder holding the files instead of the one beside the program.
pub const ENV_DIR: &str = "NVF_DLSS_DIR";
/// The folder beside the program.
pub const FOLDER_NAME: &str = "dlss";
/// The files the folder must hold for upscaling, the interposer first.
pub const FILES: [&str; 4] = ["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"];
/// DLSS Frame Generation's files beside [`FILES`] (#2317): the feature, its NGX model,
/// and Reflex and PCL, which DLSS-G requires through Streamline [documented:
/// `ProgrammingGuideDLSS_G.md` §8.0, `ProgrammingGuideReflex.md` §6.0;
/// fork-notes `fg-facts.md` §1].
pub const FG_FILES: [&str; 4] = ["sl.dlss_g.dll", "nvngx_dlssg.dll", "sl.reflex.dll", "sl.pcl.dll"];

/// NVIDIA's updater's name, which Streamline looks for beside the interposer [documented:
/// `src/ota.cpp:206`].
pub const UPDATER_NAME: &str = "nvngx_update.exe";

/// The DLLs of Streamline 2.14.16's release archive (`streamline-sdk-v2.14.16.zip`, `bin\x64`)
/// and their SHA-256 in hex [observed: the official release zip, 2026-11-12]. Its
/// `nvngx_dlss.dll` is left out on purpose: `nvngx_*.dll` are not pinned (the game's
/// `preflight.rs` module comment).
pub const RELEASE_DLLS: [(&str, &str); 12] = [
    ("NvLowLatencyVk.dll", "2a77dc3e1c724b7eea5755be0ae7423752e79a2459fae72181a9f00e3507e5d6"),
    ("sl.common.dll", "38eea3b4209afc455636ea372b5933d65fcc13becec6c84d101dcfc4d8648f75"),
    ("sl.deepdvc.dll", "2d493dccfefdc0580deb85afc65e70ae36adf77c6dd3c745c60623d9b8ac54f9"),
    ("sl.directsr.dll", "e3bbaf3e5eb906a63b3f00413c2d1c4c585ab3a2ee1946fcfa9ee2d804001c8e"),
    ("sl.dlss.dll", "18871eea87c90345da97d8109faa81ea7d5840e8d99b51b55b0e46cfa177f84e"),
    ("sl.dlss_d.dll", "8dff36fd063094eb1f659e8d4dcd52c7423859a9c4a3df3796084c48412d60ac"),
    ("sl.dlss_g.dll", "3ed9538df724993bea8a050195a6a78b9c8c8a566d5b05ade477aab3c390e90f"),
    ("sl.interposer.dll", "b1d7395aebd7b9722841d64dcf7433680020b6a8e421f851c16d5ba0c2182edf"),
    ("sl.nis.dll", "f9f3873c00e2737b22df1bf7605d4fdc7978dbf2955df4e152c5b52aada13b79"),
    ("sl.nvperf.dll", "dc5a820b97943c4d697a9ddec59b0dcde86ecadc093b401cf55980f7f89be43b"),
    ("sl.pcl.dll", "5698c38e722318681f215c9fe68e45b9bedd51378acc699d8eee26e961e57770"),
    ("sl.reflex.dll", "443577e869352436ff7ec8f4d9c90418d3e95368436ce219261e0f4f07a38122"),
];

/// 64 hex digits (either case) as 32 bytes; `None` for anything else. The one parser for the
/// listed hashes here, in [`security`] and in the game's preflight.
pub fn parse_sha256(hex: &str) -> Option<[u8; 32]> {
    let (pairs, rest) = hex.as_bytes().as_chunks::<2>();
    if pairs.len() != 32 || !rest.is_empty() {
        return None;
    }
    let mut out = [0u8; 32];
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        let high = char::from(*high).to_digit(16)?;
        let low = char::from(*low).to_digit(16)?;
        *byte = (high * 16 + low) as u8;
    }
    Some(out)
}

/// `path` opened for reading, sharing read access only on Windows: no one can write, delete or
/// rename the file while the handle is open (std's default shares all three; `share_mode`
/// replaces it) [documented: Microsoft Learn, "CreateFileW function (fileapi.h)", dwShareMode].
/// With `FILE_FLAG_OPEN_REPARSE_POINT`, a link is opened as itself, not followed ("normal reparse
/// point processing will not occur"; same page, dwFlagsAndAttributes); a link then holds no
/// file's bytes to hash or verify.
pub fn open_shared_read(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options.share_mode(FILE_SHARE_READ);
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

/// Where the folder came from, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSource {
    /// `NVF_DLSS_DIR`.
    EnvDir,
    /// `<program folder>\dlss`.
    BesideProgram,
}

/// The folder: `NVF_DLSS_DIR` when set (it must be absolute: Streamline wants absolute plugin
/// paths [documented: `sl_core_types.h:565`] and the signature check a full path
/// [documented: `sl_security.h:328-330`]), else `dlss` beside the program (`exe_dir`, the
/// folder of `nvforever.exe`; the launcher sits beside it in a release).
pub fn resolve_folder(dir_var: Option<&std::ffi::OsStr>, exe_dir: Option<&Path>) -> Result<(PathBuf, FolderSource), String> {
    match dir_var.filter(|v| !v.is_empty()) {
        Some(v) => {
            let path = PathBuf::from(v);
            if path.is_absolute() {
                Ok((path, FolderSource::EnvDir))
            } else {
                Err(format!("{ENV_DIR}={} is not an absolute path", path.display()))
            }
        }
        None => exe_dir.map(|d| (d.join(FOLDER_NAME), FolderSource::BesideProgram)).ok_or_else(|| "the program's own folder is unknown".to_string()),
    }
}

/// The DLSS folder made, through the same write guard as the download (never inside the game
/// folder or the original game's user folder).
pub fn create_dlss_folder(folder: &Path) -> std::io::Result<()> {
    fnv_install::guard_write(folder, None).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(folder)
}

/// Whether process `pid` is still running and is one of NVForever's programs (its image name
/// starts with `nvforever`: the game or the launcher), so a staging folder of a download still
/// going is never removed.
#[cfg(windows)]
pub fn still_running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the handle is checked before use and closed once; the buffers outlive the calls.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let alive = GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE as u32;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let named = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        let name = String::from_utf16_lossy(&buf[..len as usize]);
        alive && named && Path::new(&name).file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().starts_with("nvforever"))
    }
}

#[cfg(not(windows))]
pub fn still_running(_pid: u32) -> bool {
    false
}

/// The two Windows helpers [`security`] takes from Streamline's header translation (the game
/// keeps its own copy in `dlss/ffi.rs` for the loader).
#[cfg(windows)]
pub(crate) mod win_ffi {
    use std::ffi::{CStr, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::HMODULE;
    use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

    /// `s` as a NUL-terminated UTF-16 string for Windows.
    pub fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// `name`'s address in `module` as the function pointer type `T`.
    ///
    /// # Safety
    /// `module` is a loaded module and `T` is the function pointer type of `name`'s real signature.
    pub unsafe fn get_proc<T: Copy>(module: HMODULE, name: &CStr) -> Option<T> {
        const { assert!(size_of::<T>() == size_of::<usize>()) };
        // SAFETY: a loaded module and a NUL-terminated name.
        let found = unsafe { GetProcAddress(module, name.as_ptr().cast()) }?;
        // SAFETY: the caller names the export's signature; both are plain function pointers.
        Some(unsafe { std::mem::transmute_copy::<unsafe extern "system" fn() -> isize, T>(&found) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn the_folder_is_the_variables_or_beside_the_program() {
        let exe = std::env::temp_dir().join("nvf-dlss-files-exe");
        assert_eq!(resolve_folder(None, Some(&exe)), Ok((exe.join("dlss"), FolderSource::BesideProgram)));
        assert_eq!(resolve_folder(Some(OsStr::new("")), Some(&exe)), Ok((exe.join("dlss"), FolderSource::BesideProgram)), "empty counts as unset");
        let elsewhere = std::env::temp_dir().join("elsewhere-dlss");
        assert_eq!(resolve_folder(Some(elsewhere.as_os_str()), Some(&exe)), Ok((elsewhere.clone(), FolderSource::EnvDir)));
        assert_eq!(resolve_folder(Some(elsewhere.as_os_str()), None), Ok((elsewhere, FolderSource::EnvDir)));
        assert!(resolve_folder(Some(OsStr::new("relative\\dlss")), Some(&exe)).unwrap_err().contains("not an absolute path"));
        assert!(resolve_folder(None, None).is_err());
    }

    #[test]
    fn hashes_parse_strictly() {
        assert_eq!(parse_sha256(&"ab".repeat(32)), Some([0xab; 32]));
        assert_eq!(parse_sha256(&"AB".repeat(32)), Some([0xab; 32]));
        assert_eq!(parse_sha256("ab"), None);
        assert_eq!(parse_sha256(&"zz".repeat(32)), None);
        for (_, h) in RELEASE_DLLS {
            assert!(parse_sha256(h).is_some(), "{h}");
        }
    }

    #[test]
    fn the_licence_is_nvidias() {
        assert!(LICENCE.starts_with("NVIDIA RTX SDKs LICENSE"));
    }
}
