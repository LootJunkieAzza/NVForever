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

#[cfg(any(windows, test))]
use std::io::{self, Read, Write};
#[cfg(any(windows, test))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(any(windows, test))]
mod decode;
#[cfg(any(windows, test))]
mod fetch;
mod files;
#[cfg(any(windows, test))]
mod install;
#[cfg(test)]
mod test_zip;
#[cfg(any(windows, test))]
mod zip;

// Off Windows only the tests use the list, and not through these names.
#[cfg_attr(not(windows), allow(unused_imports))]
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
    // Made only off Windows (`start`).
    #[cfg_attr(windows, allow(dead_code))]
    NotWindows,
    /// The download's own code stopped on an error of its own (a panic; its message).
    Internal(String),
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
            Self::Internal(why) => write!(f, "the download stopped on an internal error ({why})"),
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

/// The download started last in this session, if any (a finished one included).
static CURRENT: Mutex<Option<Arc<Shared>>> = Mutex::new(None);

/// The download started last in this session (running or finished), so a dialog opened again
/// finds it.
pub fn current() -> Option<Handle> {
    CURRENT.lock().unwrap_or_else(PoisonError::into_inner).clone().map(|shared| Handle { shared })
}

/// Starts the download on its own thread (`nvf-dlss-download`); the game goes on meanwhile.
/// While one is running, its handle is returned instead of a second one starting.
pub fn start(_accepted: LicenceAccepted) -> Handle {
    let mut current = CURRENT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(running) = current.as_ref().filter(|s| !s.progress.lock().unwrap_or_else(PoisonError::into_inner).phase.finished()) {
        return Handle { shared: running.clone() };
    }
    bevy::log::info!("DLSS download: licence accepted");
    let shared = Arc::new(Shared::new());
    *current = Some(shared.clone());
    drop(current);
    #[cfg(windows)]
    {
        let worker = shared.clone();
        let spawned = std::thread::Builder::new().name("nvf-dlss-download".into()).spawn(move || {
            // A panic ends the download as failed (the staging folder's guard still removes it
            // while unwinding), so the phase never stays unfinished and a new one can start.
            let phase = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match target_folder() {
                Err(why) => Phase::Failed(Failure::Write(why)),
                Ok(target) => {
                    let mut source = fetch::Http::new();
                    run(&mut source, DOWNLOAD_FILES, &target, &worker, &|dir| install::signature_checks(dir, DOWNLOAD_FILES))
                }
            }))
            .unwrap_or_else(|panic| Phase::Failed(Failure::Internal(panic_text(panic.as_ref()))));
            finish(&worker, phase);
        });
        if let Err(e) = spawned {
            finish(&shared, Phase::Failed(Failure::Write(format!("the download thread did not start: {e}"))));
        }
    }
    #[cfg(not(windows))]
    finish(&shared, Phase::Failed(Failure::NotWindows));
    Handle { shared }
}

/// Staging folders an earlier run left beside the DLSS folder removed (`install::remove_stale`:
/// only `.dlss-download-<process id>` folders of runs no longer going), on a thread of its own so
/// the start does not wait. Windows only, like the download.
pub fn clean_up_at_start() {
    #[cfg(windows)]
    {
        let spawned = std::thread::Builder::new().name("nvf-dlss-clean".into()).spawn(|| {
            if let Ok(target) = target_folder() {
                install::remove_stale_beside(&target);
            }
        });
        if let Err(e) = spawned {
            bevy::log::warn!("DLSS download: the staging clean-up did not start: {e}");
        }
    }
}

/// How the game's end left a running download ([`stop_for_exit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStop {
    /// None was running.
    NoneRunning,
    /// It stopped (cancelled, or its install finished) within the time given.
    Stopped,
    /// It did not stop in time; a staging folder may be left for the next start's clean-up.
    StillRunning,
}

/// The game is ending (quit, or a load or the main menu handing over to a new run): a running
/// download is cancelled, unless its files are being moved in, which is let finish, and
/// waited for up to `wait` so its staging folder's guard runs before the process ends.
pub fn stop_for_exit(wait: std::time::Duration) -> ExitStop {
    let Some(shared) = CURRENT.lock().unwrap_or_else(PoisonError::into_inner).clone() else { return ExitStop::NoneRunning };
    stop_shared(&shared, wait)
}

fn stop_shared(shared: &Shared, wait: std::time::Duration) -> ExitStop {
    let phase = |s: &Shared| s.progress.lock().unwrap_or_else(PoisonError::into_inner).phase.clone();
    let now = phase(shared);
    if now.finished() {
        return ExitStop::NoneRunning;
    }
    if now != Phase::Installing {
        shared.cancel.store(true, Ordering::Relaxed);
    }
    let until = std::time::Instant::now() + wait;
    while !phase(shared).finished() {
        if std::time::Instant::now() >= until {
            return ExitStop::StillRunning;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    ExitStop::Stopped
}

/// A panic's message, as far as it is text.
fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic.downcast_ref::<&str>().map(|s| (*s).to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".to_string())
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

/// The archive's bytes by range: HTTP in the game (`fetch.rs`), memory in the tests.
#[cfg(any(windows, test))]
trait RangeSource {
    /// Where the bytes come from, for the log.
    fn describe(&self) -> String;
    /// The archive's size in bytes.
    fn size(&mut self) -> Result<u64, Failure>;
    /// A reader of the `len` bytes from `start`; its errors are the transport's.
    fn range(&mut self, start: u64, len: u64) -> Result<Box<dyn Read + '_>, Failure>;
}

/// How a download stopped before its end.
#[cfg(any(windows, test))]
enum Stop {
    Failed(Failure),
    Cancelled,
}

#[cfg(any(windows, test))]
impl From<Failure> for Stop {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}

/// A range's reader as the download reads it: never more than the range, its bytes counted
/// into the progress, the cancel flag looked at on every read, an early end or a transport
/// error kept as the failure.
#[cfg(any(windows, test))]
struct Tap<'a> {
    inner: Box<dyn Read + 'a>,
    shared: &'a Shared,
    left: u64,
    failed: Option<String>,
}

#[cfg(any(windows, test))]
impl<'a> Tap<'a> {
    fn new(inner: Box<dyn Read + 'a>, shared: &'a Shared, len: u64) -> Self {
        Self { inner, shared, left: len, failed: None }
    }

    /// Why reading stopped: the cancel, else the transport's failure, else `otherwise`.
    fn stop(&mut self, e: io::Error, otherwise: impl FnOnce(io::Error) -> Failure) -> Stop {
        if self.shared.cancelled() {
            Stop::Cancelled
        } else if let Some(why) = self.failed.take() {
            Stop::Failed(Failure::Offline(why))
        } else {
            Stop::Failed(otherwise(e))
        }
    }

    /// `buf` filled from the range.
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), Stop> {
        let read = self.read_exact(buf);
        read.map_err(|e| self.stop(e, |e| Failure::NotAZip(e.to_string())))
    }

    /// `n` bytes read and dropped; all `n` must be left in the range.
    fn skip(&mut self, n: u64) -> Result<(), Stop> {
        match io::copy(&mut self.by_ref().take(n), &mut io::sink()) {
            Ok(copied) if copied == n => Ok(()),
            Ok(copied) => Err(Failure::NotAZip(format!("{n} bytes to pass over, {copied} left in the range")).into()),
            Err(e) => Err(self.stop(e, |e| Failure::NotAZip(e.to_string()))),
        }
    }
}

#[cfg(any(windows, test))]
impl Read for Tap<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.shared.cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let max = buf.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        match self.inner.read(&mut buf[..max]) {
            Ok(0) => {
                let why = format!("the answer ended {} bytes early", self.left);
                self.failed = Some(why.clone());
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, why))
            }
            Ok(n) => {
                self.left -= n as u64;
                self.shared.update(|p| p.fetched += n as u64);
                Ok(n)
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Err(e),
            Err(e) => {
                self.failed = Some(e.to_string());
                Err(e)
            }
        }
    }
}

/// The `len` bytes from `start`, whole.
#[cfg(any(windows, test))]
fn read_all<S: RangeSource>(src: &mut S, shared: &Shared, start: u64, len: u64) -> Result<Vec<u8>, Stop> {
    let size = usize::try_from(len).map_err(|_| Failure::NotAZip(format!("a read of {len} bytes")))?;
    let mut tap = Tap::new(src.range(start, len)?, shared, len);
    let mut bytes = vec![0u8; size];
    tap.fill(&mut bytes)?;
    Ok(bytes)
}

/// One download from `src` into `target`, to its end: the files of `files` fetched, unpacked,
/// checked (their pins, then `checks` on the staging folder) and installed; the phase it ended
/// in (done, failed or cancelled). Nothing is written in `target` unless every file passed.
#[cfg(any(windows, test))]
fn run<S: RangeSource>(src: &mut S, files: &[DownloadFile], target: &Path, shared: &Shared, checks: &dyn Fn(&Path) -> Result<(), Failure>) -> Phase {
    match fetch_and_install(src, files, target, shared, checks) {
        Ok(()) => Phase::Done(target.to_path_buf()),
        Err(Stop::Failed(failure)) => Phase::Failed(failure),
        Err(Stop::Cancelled) => Phase::Cancelled,
    }
}

#[cfg(any(windows, test))]
fn fetch_and_install<S: RangeSource>(src: &mut S, files: &[DownloadFile], target: &Path, shared: &Shared, checks: &dyn Fn(&Path) -> Result<(), Failure>) -> Result<(), Stop> {
    let go_on = || if shared.cancelled() { Err(Stop::Cancelled) } else { Ok(()) };
    shared.set_phase(Phase::Connecting);
    let size = src.size()?;
    go_on()?;
    // The directory: the archive's end, the ZIP64 record if there is one, the directory.
    shared.set_phase(Phase::Directory);
    let tail_len = size.min(zip::TAIL_LEN);
    let tail_start = size - tail_len;
    let tail = read_all(src, shared, tail_start, tail_len)?;
    // `len` bytes at `at`: from the tail when it holds them, else fetched.
    let bytes_at = |src: &mut S, at: u64, len: u64| -> Result<Vec<u8>, Stop> {
        if at >= tail_start && at.checked_add(len).is_some_and(|end| end <= size) {
            let from = (at - tail_start) as usize;
            Ok(tail[from..from + len as usize].to_vec())
        } else {
            read_all(src, shared, at, len)
        }
    };
    let dir = match zip::find_end(&tail, tail_start).map_err(Failure::NotAZip)? {
        zip::End::Directory(dir) => dir,
        zip::End::Zip64At(at) => zip::parse_zip64_end(&bytes_at(src, at, zip::ZIP64_EOCD_LEN as u64)?, at).map_err(Failure::NotAZip)?,
    };
    let entries = zip::read_directory(&bytes_at(src, dir.offset, dir.size)?, &dir).map_err(Failure::NotAZip)?;
    let selected = zip::select(&entries, files)?;
    let spans = zip::plan(&selected, &entries, &dir).map_err(Failure::NotAZip)?;
    let to_fetch: u64 = spans.iter().map(zip::Span::len).sum();
    shared.update(|p| {
        p.fetched = 0;
        p.to_fetch = to_fetch;
    });
    bevy::log::info!("DLSS download: {} entries, {to_fetch} bytes to fetch from {}", selected.len(), src.describe());
    go_on()?;
    // The files, unpacked into the staging folder as they arrive.
    let mut staging = install::Staging::create(target)?;
    let mut index = 0;
    for span in &spans {
        let mut tap = Tap::new(src.range(span.start, span.len())?, shared, span.len());
        let mut pos = span.start;
        for &(i, item_end) in &span.items {
            let zip::Selected { file, entry } = &selected[i];
            index += 1;
            shared.set_phase(Phase::File { name: file.name, index, of: selected.len() });
            let gap = entry.local_offset.checked_sub(pos).ok_or_else(|| Failure::NotAZip(format!("{}: its local header is inside the entry before it", file.zip_path)))?;
            tap.skip(gap)?;
            let mut header = [0u8; zip::LOCAL_HEADER_LEN];
            tap.fill(&mut header)?;
            let local = zip::local_header(&header).map_err(|why| Failure::NotAZip(format!("{}: {why}", file.zip_path)))?;
            // Its header, name, extra field and data must end by the next entry's header.
            let data_end = entry.local_offset + (zip::LOCAL_HEADER_LEN + local.name_len + local.extra_len) as u64 + entry.compressed;
            if data_end > item_end {
                return Err(Failure::NotAZip(format!("{}: its data runs past the next entry", file.zip_path)).into());
            }
            let mut name = vec![0u8; local.name_len];
            tap.fill(&mut name)?;
            if local.method != entry.method || String::from_utf8_lossy(&name).replace('\\', "/") != entry.name {
                return Err(Failure::NotAZip(format!("{}: its local header is not its directory entry's", file.zip_path)).into());
            }
            tap.skip(local.extra_len as u64)?;
            let path = staging.path().join(file.name);
            let mut out = io::BufWriter::with_capacity(decode::CHUNK, staging.create_file(file)?);
            let unpacked = {
                let mut packed = (&mut tap).take(entry.compressed);
                decode::decode(&mut packed, entry.method, file.size, &mut out, &|| shared.cancelled())
                    .and_then(|decoded| io::copy(&mut packed, &mut io::sink()).map(|_| decoded).map_err(decode::DecodeError::Source))
            };
            let decoded = unpacked.map_err(|e| match e {
                decode::DecodeError::Cancelled => Stop::Cancelled,
                decode::DecodeError::Oversize => Failure::NotAZip(format!("{} unpacks to more than {} bytes", file.zip_path, file.size)).into(),
                decode::DecodeError::Write(e) => Failure::Write(format!("{}: {e}", path.display())).into(),
                decode::DecodeError::Source(e) => tap.stop(e, |e| Failure::NotAZip(format!("{}: {e}", file.zip_path))),
            })?;
            out.flush().map_err(|e| Failure::Write(format!("{}: {e}", path.display())))?;
            let written = out.into_inner().map_err(|e| Failure::Write(format!("{}: {}", path.display(), e.error())))?;
            written.sync_all().map_err(|e| Failure::Write(format!("{}: {e}", path.display())))?;
            drop(written);
            decode::verify(&decoded, entry.crc32, file)?;
            bevy::log::info!("DLSS download: {} {} bytes, SHA-256 ok", file.name, decoded.size);
            pos = data_end;
        }
        // The rest of the range (a data descriptor) is read too: the progress reaches its end,
        // and the whole range is known to have come.
        let rest = span.end.checked_sub(pos).ok_or_else(|| Failure::NotAZip("an entry runs past its range".into()))?;
        tap.skip(rest)?;
    }
    go_on()?;
    shared.set_phase(Phase::Checking);
    install::verify_staged(&staging, files, checks)?;
    go_on()?;
    shared.set_phase(Phase::Installing);
    install::place(&mut staging, files, target, &mut install::move_new)?;
    Ok(())
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
    fn the_games_end_cancels_a_running_download_and_waits_for_it_but_lets_an_install_finish() {
        use std::time::Duration;
        // Nothing running.
        let done = Shared::new();
        done.set_phase(Phase::Cancelled);
        assert_eq!(stop_shared(&done, Duration::from_millis(50)), ExitStop::NoneRunning);
        // A download that ends once cancelled (a worker standing in for the download thread).
        let shared = Arc::new(Shared::new());
        let worker = shared.clone();
        let thread = std::thread::spawn(move || {
            while !worker.cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            worker.set_phase(Phase::Cancelled);
        });
        assert_eq!(stop_shared(&shared, Duration::from_secs(5)), ExitStop::Stopped);
        thread.join().unwrap();
        // Moving the files in: not cancelled; waited for while it finishes, given up after the bound.
        let installing = Shared::new();
        installing.set_phase(Phase::Installing);
        assert_eq!(stop_shared(&installing, Duration::from_millis(30)), ExitStop::StillRunning);
        assert!(!installing.cancelled(), "an install under way is let finish");
    }

    #[test]
    fn the_pinned_source_is_the_official_release_asset() {
        assert!(ASSET_URL.starts_with("https://github.com/NVIDIA-RTX/Streamline/releases/download/v2.14.16/"));
        assert!(ASSET_URL.ends_with("/streamline-sdk-v2.14.16.zip"));
        assert_eq!(RELEASE_PAGE, "https://github.com/NVIDIA-RTX/Streamline/releases/tag/v2.14.16");
        assert_eq!(Failure::Http(404).to_string(), "the server answered HTTP 404");
    }

    use super::install::tests::scratch;
    use super::test_zip::{self, Options, TestEntry};
    use std::cell::Cell;
    use std::fs;

    /// The archive in memory, each range asked for kept; optionally the cancel flag set, or the
    /// answer cut short, from the second range on (the files' ranges: the test archives' whole
    /// directory is in the first, the tail).
    struct Memory<'a> {
        zip: Vec<u8>,
        ranges: Vec<(u64, u64)>,
        cancel: Option<&'a Shared>,
        cut_to: Option<usize>,
    }

    impl Memory<'_> {
        fn new(zip: Vec<u8>) -> Self {
            Self { zip, ranges: Vec::new(), cancel: None, cut_to: None }
        }
    }

    impl RangeSource for Memory<'_> {
        fn describe(&self) -> String {
            "memory".into()
        }

        fn size(&mut self) -> Result<u64, Failure> {
            Ok(self.zip.len() as u64)
        }

        fn range(&mut self, start: u64, len: u64) -> Result<Box<dyn Read + '_>, Failure> {
            self.ranges.push((start, len));
            let mut bytes = &self.zip[start as usize..(start + len) as usize];
            if self.ranges.len() > 1 {
                if let Some(shared) = self.cancel {
                    shared.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(cut) = self.cut_to {
                    bytes = &bytes[..cut.min(bytes.len())];
                }
            }
            Ok(Box::new(bytes))
        }
    }

    fn a_data() -> Vec<u8> {
        (0..200_000u32).map(|i| ((i / 3) ^ (i * 13)) as u8).collect()
    }

    const B_DATA: &[u8] = b"the second file, stored";

    fn hex(data: &[u8]) -> &'static str {
        Box::leak(fnv_util::sha256::hex(data).into_boxed_str())
    }

    fn files() -> Vec<DownloadFile> {
        let a = a_data();
        vec![
            DownloadFile { name: "a.dll", zip_path: "bin/x64/a.dll", size: a.len() as u64, sha256: hex(&a) },
            DownloadFile { name: "b.dll", zip_path: "bin/x64/b.dll", size: B_DATA.len() as u64, sha256: hex(B_DATA) },
        ]
    }

    fn archive(options: &Options) -> Vec<u8> {
        let a = a_data();
        test_zip::build(
            &[
                TestEntry::deflated("bin/x64/a.dll", &a),
                TestEntry::stored("readme.txt", b"not listed"),
                TestEntry::stored("bin/x64/b.dll", B_DATA),
                TestEntry::deflated("bin/x64/nvngx_update.exe", b"never written"),
            ],
            options,
        )
    }

    /// The local header offsets of the archive's entries, by name.
    fn offsets(zip: &[u8]) -> Vec<(String, u64)> {
        let dir = match zip::find_end(zip, 0).unwrap() {
            zip::End::Directory(dir) => dir,
            zip::End::Zip64At(at) => zip::parse_zip64_end(&zip[at as usize..], at).unwrap(),
        };
        let entries = zip::read_directory(&zip[dir.offset as usize..(dir.offset + dir.size) as usize], &dir).unwrap();
        entries.into_iter().map(|e| (e.name, e.local_offset)).collect()
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_download_fetches_only_the_listed_entries_and_installs_them() {
        for options in [Options::default(), Options { zip64: true, descriptor: true, comment: b"a comment".to_vec() }] {
            let root = scratch("run");
            let target = root.join("dlss");
            let zip = archive(&options);
            let mut src = Memory::new(zip.clone());
            let shared = Shared::new();
            let checked = Cell::new(false);
            let phase = run(&mut src, &files(), &target, &shared, &|dir| {
                assert_eq!(listing(dir), ["a.dll", "b.dll"], "the checks see both staged files");
                checked.set(true);
                Ok(())
            });
            assert_eq!(phase, Phase::Done(target.clone()));
            assert!(checked.get());
            assert_eq!(listing(&target), ["a.dll", "b.dll"]);
            assert_eq!(fs::read(target.join("a.dll")).unwrap(), a_data());
            assert_eq!(fs::read(target.join("b.dll")).unwrap(), B_DATA);
            assert_eq!(listing(&root), ["dlss"], "no staging folder left");
            // The tail, then one range per listed entry, from its local header.
            let at = offsets(&zip);
            let of = |name: &str| at.iter().find(|(n, _)| n == name).unwrap().1;
            assert_eq!(src.ranges[0].0 + src.ranges[0].1, zip.len() as u64, "the first range is the archive's end");
            let starts: Vec<u64> = src.ranges[1..].iter().map(|r| r.0).collect();
            assert_eq!(starts, [of("bin/x64/a.dll"), of("bin/x64/b.dll")]);
            assert_eq!(src.ranges[1].0 + src.ranges[1].1, of("readme.txt"), "a.dll's range stops at the next header");
            let updater = of("bin/x64/nvngx_update.exe");
            assert!(src.ranges[1..].iter().all(|&(s, l)| s + l <= updater), "the updater's bytes are never asked for");
            let progress = shared.progress.lock().unwrap().clone();
            assert_eq!(progress.fetched, progress.to_fetch);
            assert_eq!(progress.to_fetch, src.ranges[1..].iter().map(|r| r.1).sum::<u64>());
            let _ = fs::remove_dir_all(root);
        }
    }

    /// A run that must fail or be cancelled, and leave nothing behind.
    fn refused(label: &str, src: &mut Memory<'_>, files: &[DownloadFile], shared: &Shared, checks: &dyn Fn(&Path) -> Result<(), Failure>) -> Phase {
        let root = scratch(label);
        let target = root.join("dlss");
        let phase = run(src, files, &target, shared, checks);
        assert!(!target.exists(), "{label}: nothing written");
        assert!(listing(&root).is_empty(), "{label}: no staging folder left");
        let _ = fs::remove_dir_all(root);
        phase
    }

    #[test]
    fn a_file_other_than_its_pin_or_refused_by_the_checks_writes_nothing() {
        let mut wrong = files();
        wrong[1].sha256 = hex(b"another build");
        let phase = refused("mismatch", &mut Memory::new(archive(&Options::default())), &wrong, &Shared::new(), &|_| Ok(()));
        assert_eq!(phase, Phase::Failed(Failure::Mismatch { name: "b.dll", sha256: fnv_util::sha256::hex(B_DATA) }));
        let phase = refused("signature", &mut Memory::new(archive(&Options::default())), &files(), &Shared::new(), &|_| {
            Err(Failure::Signature { name: "a.dll", why: "not signed".into() })
        });
        assert_eq!(phase, Phase::Failed(Failure::Signature { name: "a.dll", why: "not signed".into() }));
        let mut missing = files();
        missing[0].zip_path = "bin/x64/missing.dll";
        let phase = refused("missing", &mut Memory::new(archive(&Options::default())), &missing, &Shared::new(), &|_| Ok(()));
        assert_eq!(phase, Phase::Failed(Failure::EntryMissing("bin/x64/missing.dll")));
        let phase = refused("not-a-zip", &mut Memory::new(b"not an archive at all".to_vec()), &files(), &Shared::new(), &|_| Ok(()));
        assert!(matches!(phase, Phase::Failed(Failure::NotAZip(_))), "{phase:?}");
    }

    #[test]
    fn a_cancel_or_a_cut_answer_writes_nothing() {
        let shared = Shared::new();
        let mut src = Memory::new(archive(&Options::default()));
        src.cancel = Some(&shared);
        assert_eq!(refused("cancel", &mut src, &files(), &shared, &|_| Ok(())), Phase::Cancelled);
        let mut src = Memory::new(archive(&Options::default()));
        src.cut_to = Some(1000);
        let phase = refused("cut", &mut src, &files(), &Shared::new(), &|_| Ok(()));
        assert!(matches!(&phase, Phase::Failed(Failure::Offline(why)) if why.contains("ended")), "{phase:?}");
    }

    /// An archive whose two listed entries touch (one merged range); `claim` is an extra field
    /// length a.dll's local header claims without the bytes being there.
    fn touching(options: &Options, claim: u16) -> Vec<u8> {
        let a = a_data();
        test_zip::build(
            &[
                TestEntry::stored("readme.txt", b"not listed"),
                TestEntry::deflated("bin/x64/a.dll", &a).claiming_extra(claim),
                TestEntry::stored("bin/x64/b.dll", B_DATA),
                TestEntry::deflated("bin/x64/nvngx_update.exe", b"never written"),
            ],
            options,
        )
    }

    #[test]
    fn touching_entries_come_in_one_range_and_install() {
        for options in [Options::default(), Options { descriptor: true, ..Options::default() }] {
            let root = scratch("run-touching");
            let target = root.join("dlss");
            let mut src = Memory::new(touching(&options, 0));
            let shared = Shared::new();
            assert_eq!(run(&mut src, &files(), &target, &shared, &|_| Ok(())), Phase::Done(target.clone()));
            assert_eq!(src.ranges.len(), 2, "the tail and one merged range");
            assert_eq!(listing(&target), ["a.dll", "b.dll"]);
            assert_eq!(fs::read(target.join("a.dll")).unwrap(), a_data());
            let progress = shared.progress.lock().unwrap().clone();
            assert_eq!(progress.fetched, progress.to_fetch, "the whole range read, data descriptors too");
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn a_local_extra_field_reaching_into_the_next_entry_is_refused() {
        for claim in [20u16, 0xFFFF] {
            let phase = refused("extra", &mut Memory::new(touching(&Options::default(), claim)), &files(), &Shared::new(), &|_| Ok(()));
            assert!(matches!(&phase, Phase::Failed(Failure::NotAZip(why)) if why.contains("runs past the next entry")), "claim {claim}: {phase:?}");
        }
    }

    #[test]
    fn a_panic_ends_as_an_internal_failure_with_its_message() {
        let text = std::panic::catch_unwind(|| panic!("broken {}", 7)).unwrap_err();
        assert_eq!(panic_text(text.as_ref()), "broken 7");
        let fixed = std::panic::catch_unwind(|| panic!("fixed words")).unwrap_err();
        assert_eq!(panic_text(fixed.as_ref()), "fixed words");
        let other = std::panic::catch_unwind(|| std::panic::panic_any(5u8)).unwrap_err();
        assert_eq!(panic_text(other.as_ref()), "a panic");
        assert_eq!(Failure::Internal("broken 7".into()).to_string(), "the download stopped on an internal error (broken 7)");
    }

    #[test]
    fn a_folder_holding_the_updater_is_left_as_it_is() {
        let root = scratch("run-updater");
        let target = root.join("dlss");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("nvngx_update.exe"), b"x").unwrap();
        let phase = run(&mut Memory::new(archive(&Options::default())), &files(), &target, &Shared::new(), &|_| Ok(()));
        assert_eq!(phase, Phase::Failed(Failure::FolderHolds("nvngx_update.exe".into())));
        assert_eq!(listing(&target), ["nvngx_update.exe"]);
        assert_eq!(listing(&root), ["dlss"]);
        let _ = fs::remove_dir_all(root);
    }

    /// A server on this machine answering every request with `answer`; its address.
    #[cfg(windows)]
    fn serve(answer: &'static str) -> String {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    line.clear();
                }
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}/streamline-sdk-v2.14.16.zip")
    }

    #[cfg(windows)]
    #[test]
    fn a_404_or_an_answer_without_ranges_fails_with_nothing_written() {
        let not_found = serve("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let root = scratch("http");
        let target = root.join("dlss");
        let phase = run(&mut fetch::Http::with_url(not_found, true), DOWNLOAD_FILES, &target, &Shared::new(), &|_| Ok(()));
        assert_eq!(phase, Phase::Failed(Failure::Http(404)));
        let whole = serve("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
        let phase = run(&mut fetch::Http::with_url(whole, true), DOWNLOAD_FILES, &target, &Shared::new(), &|_| Ok(()));
        assert_eq!(phase, Phase::Failed(Failure::NoRanges));
        assert!(listing(&root).is_empty(), "nothing written");
        let _ = fs::remove_dir_all(root);
    }
}
