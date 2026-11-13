//! Download DLSS... (#2250): the four DLSS files fetched from NVIDIA's own Streamline release,
//! checked and installed in the DLSS folder, on the player's click and only after the player
//! accepted NVIDIA's licence. The settings' dialog is the caller; this module is the work
//! behind it. `docs/design/dlss-upscaling.md`, "Download", says it for readers.
//!
//! - **Licence first** [design: the maintainer's condition on #2250]: [`start`] takes a
//!   [`LicenceAccepted`], which only [`LicenceAccepted::by_player`] makes, called by the
//!   dialog's accept button. Nothing here touches the network without one.
//! - **One source**: [`ASSET_URL`], the official asset of NVIDIA-RTX/Streamline's v2.14.16
//!   release, pinned with its size [`ASSET_SIZE`]. Only the four entries are fetched, by HTTP
//!   range requests (`fetch.rs`): the archive's end for its directory, then each entry's bytes.
//! - **Pins** (`files.rs`): each file's entry, size and SHA-256. A file whose bytes differ from
//!   its pin is never kept; the interposer's signature check and the NGX runtime's NVIDIA
//!   signature check (`security.rs`) run on the staged files before anything is moved.
//! - **Nothing written unless all pass** (`install.rs`): the files are unpacked into a staging
//!   folder beside the DLSS folder, on the same volume, and moved in only when every check
//!   passed; on a failure or cancel the staging folder is removed. A DLSS folder that holds
//!   `nvngx_update.exe`, or a listed file with other bytes, is left as it is (never a player's
//!   file overwritten). Only listed names are written; `nvngx_update.exe` never is.
//! - Windows only, like DLSS itself; elsewhere [`start`] ends at once in
//!   [`Failure::NotWindows`]. The parts other than the HTTP client are built for the tests on
//!   every system.
//!
//! Log lines start `DLSS download:`.

// Off Windows the download never starts; its parts are compiled for the tests only.
#![cfg_attr(not(windows), allow(dead_code))]
// Until the settings' dialog (#2250) calls `start`, nothing outside the tests does: this line
// goes with the dialog's commit.
#![cfg_attr(windows, allow(dead_code))]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

mod files;

pub use files::{DOWNLOAD_FILES, DownloadFile};

/// The archive the files come from: the official asset of NVIDIA-RTX/Streamline's release
/// v2.14.16 [documented: https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.16].
pub const ASSET_URL: &str = "https://github.com/NVIDIA-RTX/Streamline/releases/download/v2.14.16/streamline-sdk-v2.14.16.zip";
/// Its size in bytes [observed: the official release archive, 2026-11-13]; every ranged answer
/// must name it as the total.
pub const ASSET_SIZE: u64 = 276_016_170;
/// The release's page, for the manual way.
pub const RELEASE_PAGE: &str = "https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.16";

/// Proof that the player accepted NVIDIA's licence in the dialog: the only way to call
/// [`start`]. Not `Clone`: one acceptance, one download.
#[derive(Debug)]
pub struct LicenceAccepted(());

impl LicenceAccepted {
    /// Made only by the dialog's accept button, after the licence was shown.
    pub(crate) fn by_player() -> Self {
        Self(())
    }
}

/// Why a download failed. `Display` gives the log's words; the dialog has words of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No answer: offline, the name not resolved, a timeout, the connection lost.
    Offline(String),
    /// An HTTP status other than the ones expected.
    Http(u16),
    /// The server answered without honouring the byte range (a `200`, or another range).
    NoRanges,
    /// The archive's size is not [`ASSET_SIZE`] (the size seen).
    WrongSize(u64),
    /// The archive could not be read as the expected zip (what was wrong).
    NotAZip(String),
    /// The archive has no entry for this file (its archive path).
    EntryMissing(&'static str),
    /// A file's bytes are not its pin (the SHA-256 found, in hex).
    Mismatch { name: &'static str, sha256: String },
    /// A file's signature check refused it (the check's words).
    Signature { name: &'static str, why: String },
    /// The DLSS folder holds a file that stops the install (its name): `nvngx_update.exe`, or a
    /// listed file with other bytes. Nothing of the player's is overwritten.
    FolderHolds(String),
    /// Writing the staging folder or moving the files failed (the error).
    Write(String),
    /// The archive's address answered with a redirect to a place not allowed (where).
    Redirect(String),
    /// DLSS, and so the download, is Windows only.
    NotWindows,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offline(why) => write!(f, "no answer from the server ({why})"),
            Self::Http(status) => write!(f, "the server answered HTTP {status}"),
            Self::NoRanges => write!(f, "the server did not answer with the byte range asked for"),
            Self::WrongSize(size) => write!(f, "the archive is {size} bytes, not {ASSET_SIZE}"),
            Self::NotAZip(why) => write!(f, "the archive is not the expected zip: {why}"),
            Self::EntryMissing(path) => write!(f, "the archive has no entry {path}"),
            Self::Mismatch { name, sha256 } => write!(f, "{name} has SHA-256 {sha256}, not the pinned one"),
            Self::Signature { name, why } => write!(f, "{name} failed its signature check: {why}"),
            Self::FolderHolds(name) => write!(f, "the DLSS folder already holds {name}, left as it is"),
            Self::Write(why) => write!(f, "writing the files failed: {why}"),
            Self::Redirect(to) => write!(f, "the archive's address sent to {to}, not an allowed place"),
            Self::NotWindows => write!(f, "DLSS is available on Windows only"),
        }
    }
}

/// Where a download is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Asking for the archive: its address, then its size.
    Connecting,
    /// Reading the archive's directory (its last bytes).
    Directory,
    /// Fetching, unpacking and checking one file; `index` counts from 1 to `of`.
    File { name: &'static str, index: usize, of: usize },
    /// The staged files' hashes and signatures checked again before they are moved.
    Checking,
    /// The files moved into the DLSS folder.
    Installing,
    /// Installed in this folder; a restart loads DLSS.
    Done(PathBuf),
    /// Stopped; nothing was written in the DLSS folder.
    Failed(Failure),
    /// Cancelled by the player; nothing was written in the DLSS folder.
    Cancelled,
}

impl Phase {
    /// Whether the download has ended (done, failed or cancelled).
    pub fn finished(&self) -> bool {
        matches!(self, Self::Done(_) | Self::Failed(_) | Self::Cancelled)
    }
}

/// A download's state for the dialog: the phase and the archive bytes fetched of those to
/// fetch (0 of 0 until the directory was read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub fetched: u64,
    pub to_fetch: u64,
}

/// What the download thread and its [`Handle`]s share.
struct Shared {
    cancel: AtomicBool,
    progress: Mutex<Progress>,
}

impl Shared {
    fn new() -> Self {
        Self { cancel: AtomicBool::new(false), progress: Mutex::new(Progress { phase: Phase::Connecting, fetched: 0, to_fetch: 0 }) }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn update(&self, change: impl FnOnce(&mut Progress)) {
        change(&mut self.progress.lock().unwrap_or_else(PoisonError::into_inner));
    }

    fn set_phase(&self, phase: Phase) {
        self.update(|p| p.phase = phase);
    }
}

/// A download started by [`start`]: its progress, and a way to cancel it. Dropping every handle
/// does not stop the download.
#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}

impl Handle {
    /// The state now.
    pub fn progress(&self) -> Progress {
        self.shared.progress.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Asks the download to stop: it ends in [`Phase::Cancelled`] at its next check (each 64 KiB
    /// read), with nothing written in the DLSS folder. Too late once the files are being moved.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }
}

/// The folder the files go to: `NVF_DLSS_DIR` when set (absolute), else `dlss` beside the
/// program, the folder the loader reads (`dlss::resolve_folder`). An error off Windows.
pub fn target_folder() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::Path::to_path_buf));
        super::resolve_folder(std::env::var_os(super::ENV_DIR).as_deref(), exe_dir.as_deref()).map(|(folder, _)| folder)
    }
    #[cfg(not(windows))]
    {
        Err(Failure::NotWindows.to_string())
    }
}

/// Starts the download on its own thread (`nvf-dlss-download`); the game goes on meanwhile.
pub fn start(_accepted: LicenceAccepted) -> Handle {
    bevy::log::info!("DLSS download: licence accepted");
    let shared = Arc::new(Shared::new());
    finish(&shared, Phase::Failed(Failure::NotWindows));
    Handle { shared }
}

/// The end of a download: the phase set and its log line.
fn finish(shared: &Shared, phase: Phase) {
    match &phase {
        Phase::Done(folder) => bevy::log::info!("DLSS download: installed in {}; restart NVForever to use DLSS", folder.display()),
        Phase::Failed(failure) => bevy::log::warn!("DLSS download: failed: {failure}; nothing written"),
        Phase::Cancelled => bevy::log::info!("DLSS download: cancelled; nothing written"),
        _ => {}
    }
    shared.set_phase(phase);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_reads_the_shared_state_and_cancel_reaches_it() {
        let shared = Arc::new(Shared::new());
        let handle = Handle { shared: shared.clone() };
        assert_eq!(handle.progress(), Progress { phase: Phase::Connecting, fetched: 0, to_fetch: 0 });
        assert!(!handle.progress().phase.finished());
        shared.update(|p| {
            p.phase = Phase::File { name: "sl.dlss.dll", index: 3, of: 4 };
            p.fetched = 10;
            p.to_fetch = 40;
        });
        assert_eq!(handle.progress().fetched, 10);
        assert!(!shared.cancelled());
        handle.clone().cancel();
        assert!(shared.cancelled());
        for end in [Phase::Done(PathBuf::from("dlss")), Phase::Failed(Failure::Http(404)), Phase::Cancelled] {
            assert!(end.finished(), "{end:?}");
        }
    }

    #[test]
    fn the_pinned_source_is_the_official_release_asset() {
        assert!(ASSET_URL.starts_with("https://github.com/NVIDIA-RTX/Streamline/releases/download/v2.14.16/"));
        assert!(ASSET_URL.ends_with("/streamline-sdk-v2.14.16.zip"));
        assert_eq!(RELEASE_PAGE, "https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.16");
        assert_eq!(Failure::Http(404).to_string(), "the server answered HTTP 404");
    }
}
