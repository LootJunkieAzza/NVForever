//! Download DLSS... (#2250), the player's side: the choice in Settings > Graphics > Upscaler,
//! the dialog with NVIDIA's licence, the progress and the result ([`dialog`]), the test aid,
//! and NVIDIA's attribution at the end of the credits. The work itself is
//! `crate::dlss::download`; `docs/dlss.md` says it for players.
//!
//! - **The choice** [design: #2250]: "Download DLSS…" is a choice of the Upscaler row while
//!   any of the eight upscaling or frame generation files are missing (Windows with Vulkan,
//!   [`offered`]) and no download has installed them this session.
//!   Picking it opens the dialog and changes nothing in the row; while a
//!   download runs it reads "Downloading DLSS… n%" and picking it shows the dialog again.
//! - **Licence first** [design: the maintainer's condition on #2250]: the dialog shows NVIDIA's
//!   licence, the file that comes with the downloaded files in NVIDIA's archive
//!   ([`LICENCE`]); only its I accept button makes the `LicenceAccepted` that starts the
//!   download. Nothing touches the network before.
//! - **Attribution** [documented: the licence's supplement, 7.1(b), "attribution ... in
//!   credits"; 7.2(c), NVIDIA's marks need its approval]: while the four files are in the
//!   DLSS folder (downloaded or copied by hand), the credits roll ends with a plain-text block
//!   of our own words ([`ATTRIBUTION`]); no logo, and none of the names the licence lists as
//!   NVIDIA's marks.
//!
//! Test aid `NVF_PLAY_TEST_DLSS_DOWNLOAD=<item>[,…]` (only with `--data` / `NVFOREVER_DATA`):
//! `clear` at the start deletes the eight
//! listed files from the DLSS folder, only when `NVF_DLSS_DIR` names a folder whose name
//! starts with `smoke-dlss-` (never another file, never another folder); `accept` presses I
//! accept a second after the licence is shown; `cancel` presses Cancel there instead;
//! `cancel-at:<s>` cancels the download `s` seconds after it started. With the aid set, the
//! dialog closes itself a second after the result is shown.
//!
//! - **The game's end** [design: #2250's review]: quitting, or loading a save or going to the main
//!   menu (each starts a new run, `handoff.rs`), stops a running download and waits up to two
//!   seconds for it, so no staging folder is left (`dialog::stop_on_exit`); one left anyway goes
//!   at a later start once that run has ended (`dlss::download::clean_up_at_start`), or when the
//!   next download makes its own.
//!
//! Log lines start `DLSS download:` (the work's own lines, `dlss/download/`, too).

mod dialog;

use bevy::prelude::*;
use crate::dlss::download::{self, Phase};
use fnv_util::words::{tr, trf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// NVIDIA's licence for the downloaded files, as it comes in their archive
/// (`streamline-sdk-v2.14.16.zip`, `bin/x64/nvngx_dlss.license.txt`): NVIDIA's text, not under
/// NVForever's licence, here only so the player can read it before accepting (LEGAL.md). Never
/// edited; a test pins its SHA-256.
pub(crate) const LICENCE: &str = include_str!("nvngx_dlss.license.txt");

/// The choice's name, as the test aids and the log use it.
pub(crate) const CHOICE: &str = "Download DLSS...";
/// The choice's words on the page.
const CHOICE_WORDS: &str = "Download DLSS…";
/// The choice's words while a download runs.
const RUNNING_WORDS: &str = "Downloading DLSS… {percent}%";

/// The credits' block while the DLSS files are present: a name, then what it does, in our own
/// words [design].
pub(crate) const ATTRIBUTION: [&str; 2] = ["NVIDIA DLSS", "Upscaling by NVIDIA Corporation, through NVIDIA Streamline"];

/// The test aid's variable.
const AID: &str = "NVF_PLAY_TEST_DLSS_DOWNLOAD";
/// The only folders the aid's `clear` empties: their name starts so.
const AID_FOLDER_PREFIX: &str = "smoke-dlss-";

/// [design] Both download test hooks use the reports' developer-run condition.
pub(crate) fn test_aid_value(variable: &str) -> Option<String> {
    test_aid_for_run(std::env::var(variable).ok(), crate::reports::developer_data())
}

fn test_aid_for_run(value: Option<String>, developer_data: bool) -> Option<String> {
    let value = value.filter(|v| !v.trim().is_empty())?;
    if !developer_data {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| warn!("DLSS download: NVF_PLAY_TEST_DLSS_DOWNLOAD and NVF_DLSS_DOWNLOAD_URL ignored without --data / NVFOREVER_DATA"));
        return None;
    }
    Some(value)
}

/// Whether `name` is the download choice (its name, or its words, any case).
pub(crate) fn is_choice(name: &str) -> bool {
    let name = name.trim();
    name.eq_ignore_ascii_case(CHOICE) || name == CHOICE_WORDS
}

/// [design] One shared probe per second across the menu's choice lists and hints. A changed
/// download phase discards the previous probe, including when the install finishes.
#[derive(Default)]
struct OfferCache {
    phase: Option<Phase>,
    // [design] Cache an unresolved folder as None, separately from files being present.
    missing: Option<(Instant, Option<bool>)>,
}

impl OfferCache {
    fn offered(&mut self, windows: bool, reason: Option<crate::upscaler::Unavailable>, phase: Option<Phase>, now: Instant, probe: impl FnOnce() -> Option<bool>) -> bool {
        if self.phase != phase {
            self.phase = phase;
            self.missing = None;
        }
        // [design] Refusals the download cannot mend and a completed install never touch disk.
        if !windows || !matches!(reason, None | Some(crate::upscaler::Unavailable::FilesMissing)) || matches!(self.phase, Some(Phase::Done(_))) {
            return false;
        }
        if let Some((checked, missing)) = self.missing {
            if now.saturating_duration_since(checked) < Duration::from_secs(1) {
                return missing.unwrap_or(reason == Some(crate::upscaler::Unavailable::FilesMissing));
            }
        }
        let missing = probe();
        self.missing = Some((now, missing));
        // [design] Preserve the loader's missing-files offer when resolving the folder fails;
        // the download dialog will report the folder error. Working DLSS needs no offer.
        missing.unwrap_or(reason == Some(crate::upscaler::Unavailable::FilesMissing))
    }
}

static OFFER_CACHE: Mutex<OfferCache> = Mutex::new(OfferCache { phase: None, missing: None });

#[cfg(test)]
thread_local! {
    static TEST_OFFER_PROBE: std::cell::RefCell<Option<(OfferCache, Option<bool>)>> = const { std::cell::RefCell::new(None) };
}

/// [design] A scoped probe and private cache for menu tests, restored even on panic.
/// Some(true/false) injects missing/present files; None injects a folder-resolution error.
#[cfg(test)]
pub(crate) struct TestOfferProbe(Option<(OfferCache, Option<bool>)>);

#[cfg(test)]
impl TestOfferProbe {
    pub(crate) fn new(missing: Option<bool>) -> Self {
        Self(TEST_OFFER_PROBE.with(|probe| probe.replace(Some((OfferCache::default(), missing)))))
    }
}

#[cfg(test)]
impl Drop for TestOfferProbe {
    fn drop(&mut self) {
        TEST_OFFER_PROBE.with(|probe| probe.replace(self.0.take()));
    }
}

/// [design] Offer the download on Windows while any of the eight files is missing and the
/// loader has no other refusal, unless a download installed them this session. If the folder
/// cannot be resolved, retain an offer only for the loader's FilesMissing reason.
pub(crate) fn offered(reason: Option<crate::upscaler::Unavailable>) -> bool {
    let phase = download::current().map(|h| h.progress().phase);
    let now = Instant::now();
    #[cfg(test)]
    if let Some(offered) = TEST_OFFER_PROBE.with(|probe| probe.borrow_mut().as_mut().map(|(cache, missing)| cache.offered(cfg!(windows), reason, phase.clone(), now, || *missing))) {
        return offered;
    }
    // [design] Tests without a TestOfferProbe never look at this machine's DLSS folder: the
    // probe reads as unresolved, so the offer follows the loader's reason alone.
    #[cfg(test)]
    let probe = || None;
    #[cfg(not(test))]
    let probe = || download::target_folder().ok().map(|folder| files_missing(&folder));
    OFFER_CACHE.lock().unwrap_or_else(PoisonError::into_inner).offered(cfg!(windows), reason, phase, now, probe)
}

fn files_missing(folder: &std::path::Path) -> bool {
    download::DOWNLOAD_FILES.iter().any(|file| !folder.join(file.name).is_file())
}

/// The share of the archive's bytes fetched, in percent (0 until the directory was read).
fn percent(p: &download::Progress) -> u64 {
    (p.fetched.min(p.to_fetch) * 100).checked_div(p.to_fetch).unwrap_or(0)
}

/// The choice's words now: Download DLSS…, or how far the running download is.
pub(crate) fn choice_words() -> String {
    match download::current().map(|h| h.progress()) {
        Some(p) if !p.phase.finished() => trf(RUNNING_WORDS, &[("percent", &percent(&p))]),
        _ => tr(CHOICE_WORDS).to_string(),
    }
}

/// The choice was picked on the settings page: the dialog opens next frame.
#[derive(Resource, Debug, Default)]
pub(crate) struct Asked;

/// Picking the choice: ask for the dialog.
pub(crate) fn ask(world: &mut World) {
    info!("DLSS download: {CHOICE} chosen on the settings page");
    world.insert_resource(Asked);
}

/// The licence as the dialog shows it: NVIDIA's bytes untouched in [`LICENCE`], drawn here with
/// the curly quotes, the apostrophe and the trademark sign as plain letters (the menus' fonts
/// hold one code page's letters, and the stand-in font only ASCII), tabs as spaces and one line
/// end per line.
pub(crate) fn licence_for_display() -> String {
    let mut out = String::with_capacity(LICENCE.len());
    for c in LICENCE.chars() {
        match c {
            '\r' => {}
            '\u{201C}' | '\u{201D}' => out.push('"'),
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{2122}' => out.push_str("(TM)"),
            '\t' => out.push_str("    "),
            c if c.is_ascii() => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

/// Opens `target` (a web address or a folder) the way Explorer would, on a thread of its own so
/// the frame does not wait; the outcome goes to the log.
pub(crate) fn open_in_shell(target: std::ffi::OsString) {
    info!("DLSS download: opening {}", target.to_string_lossy());
    let spawned = std::thread::Builder::new().name("nvf-dlss-open".into()).spawn(move || {
        if let Err(why) = shell_open(&target) {
            warn!("DLSS download: could not open {}: {why}", target.to_string_lossy());
        }
    });
    if let Err(e) = spawned {
        warn!("DLSS download: could not open it: {e}");
    }
}

/// [documented: ShellExecuteW, learn.microsoft.com: "Because ShellExecute can delegate execution
/// to Shell extensions ... that are activated using COM, COM should be initialized before
/// ShellExecute is called", with `COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`; a return
/// value greater than 32 is success.] The thread is this call's own, so COM is set up and torn
/// down around it.
#[cfg(windows)]
fn shell_open(target: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide(std::ffi::OsStr::new("open")), wide(target));
    // SAFETY: COM set up on this thread (no reserved pointer), and torn down once only when
    // that succeeded (S_OK or S_FALSE, both non-negative); both strings are NUL-terminated
    // UTF-16 that outlive the call; no window, no parameters, no working folder.
    unsafe {
        let com = CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
        let result = ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
        if com >= 0 {
            CoUninitialize();
        }
        let code = result as isize;
        if code > 32 { Ok(()) } else { Err(format!("ShellExecuteW answered {code}")) }
    }
}

#[cfg(not(windows))]
fn shell_open(_target: &std::ffi::OsStr) -> Result<(), String> {
    Err("only on Windows".into())
}

/// The DLSS folder, made if it is not there yet, for "Open dlss folder".
pub(crate) fn open_dlss_folder() {
    match download::target_folder() {
        Ok(folder) => match create_dlss_folder(&folder) {
            Ok(()) => open_in_shell(folder.into_os_string()),
            Err(e) => warn!("DLSS download: could not make {}: {e}", folder.display()),
        },
        Err(why) => warn!("DLSS download: no DLSS folder: {why}"),
    }
}

/// [design] Folder opening follows the same install guard as the download, including overrides.
pub(crate) fn create_dlss_folder(folder: &std::path::Path) -> std::io::Result<()> {
    fnv_install::guard_write(folder, None).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(folder)
}

/// The test aid's items (module notes).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AidPlan {
    pub clear: bool,
    pub accept: bool,
    pub cancel: bool,
    pub cancel_at: Option<f64>,
}

impl AidPlan {
    /// The aid's value read; the items it does not know are named.
    fn parse(value: &str) -> (AidPlan, Vec<String>) {
        let mut plan = AidPlan::default();
        let mut unknown = Vec::new();
        for item in value.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            match item {
                "clear" => plan.clear = true,
                "accept" => plan.accept = true,
                "cancel" => plan.cancel = true,
                _ => match item.strip_prefix("cancel-at:").and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s >= 0.0) {
                    Some(s) => plan.cancel_at = Some(s),
                    None => unknown.push(item.to_string()),
                },
            }
        }
        (plan, unknown)
    }

    /// From the environment; `None` without the aid.
    fn from_env() -> Option<AidPlan> {
        let value = test_aid_value(AID)?;
        let (plan, unknown) = Self::parse(&value);
        for item in unknown {
            warn!("test aid {AID}: {item:?} is not clear, accept, cancel or cancel-at:<seconds>");
        }
        Some(plan)
    }
}

/// The aid's `clear`: the listed files deleted from `folder` when its name starts with
/// [`AID_FOLDER_PREFIX`]; how many went, or why nothing did.
fn clear_folder(folder: &std::path::Path) -> Result<usize, String> {
    fnv_install::guard_write(folder, None).map_err(|e| e.to_string())?;
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.starts_with(AID_FOLDER_PREFIX) {
        return Err(format!("{} is not a {AID_FOLDER_PREFIX}* folder; nothing deleted", folder.display()));
    }
    let mut cleared = 0;
    for file in download::DOWNLOAD_FILES {
        let path = folder.join(file.name);
        fnv_install::guard_write(&path, None).map_err(|e| e.to_string())?;
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            cleared += 1;
        }
    }
    Ok(cleared)
}

fn aid_clear(plan: Option<Res<dialog::Aid>>) {
    if !plan.is_some_and(|p| p.0.clear) {
        return;
    }
    // Only a folder NVF_DLSS_DIR names: never the one beside the program.
    let Some(dir) = std::env::var_os("NVF_DLSS_DIR").filter(|v| !v.is_empty()) else {
        warn!("test aid {AID}: clear needs NVF_DLSS_DIR; nothing deleted");
        return;
    };
    match clear_folder(std::path::Path::new(&dir)) {
        Ok(n) => info!("test aid {AID}: clear: {n} of the listed files deleted from {}", std::path::Path::new(&dir).display()),
        Err(why) => warn!("test aid {AID}: clear: {why}"),
    }
}

/// The credits' extra block follows the DLSS folder: set while the four files are there,
/// looked at every few seconds (cheap: four file checks).
fn attribution(time: Res<Time<Real>>, mut next: Local<f64>, mut extra: ResMut<nvf_play::credits::ExtraCredits>) {
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 3.0;
    let want = if crate::dlss::bundle_present() { attribution_lines() } else { Vec::new() };
    if extra.0 != want {
        info!("DLSS download: NVIDIA's attribution {} the credits", if want.is_empty() { "left out of" } else { "added to" });
        extra.0 = want;
    }
}

/// [`ATTRIBUTION`] as the credits show it: the name as it is, what it does in the language
/// shown.
fn attribution_lines() -> Vec<String> {
    vec![ATTRIBUTION[0].to_string(), tr(ATTRIBUTION[1]).to_string()]
}

/// The choice's dialog, its test aid and the credits' attribution (a played game's settings
/// page: `main` adds it beside `settings::register`).
pub fn add(app: &mut App) {
    if let Some(plan) = AidPlan::from_env() {
        app.insert_resource(dialog::Aid(plan));
    }
    // Staging folders an earlier run left beside the DLSS folder go at the start
    // (`dlss::download::clean_up_at_start`).
    crate::dlss::download::clean_up_at_start();
    app.init_resource::<nvf_play::credits::ExtraCredits>().add_systems(Startup, aid_clear).add_systems(Update, attribution);
    dialog::add(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The licence file's SHA-256 [observed: the release archive's file, 2026-11-13].
    const LICENCE_SHA256: &str = "3027f23ca5a46dd9cb8183fbd522983a86f64d7daac5982912bf9f214671f294";

    #[test]
    fn the_licence_is_nvidias_file_unchanged() {
        assert_eq!(fnv_util::sha256::hex(LICENCE.as_bytes()), LICENCE_SHA256, "nvngx_dlss.license.txt must stay NVIDIA's bytes");
        assert!(LICENCE.starts_with("NVIDIA RTX SDKs LICENSE"));
    }

    #[test]
    fn the_licence_is_shown_in_letters_every_menu_font_has() {
        let shown = licence_for_display();
        assert!(shown.chars().all(|c| c == '\n' || (' '..='~').contains(&c)), "only printable ASCII and line ends");
        assert!(!shown.contains('?'), "no letter left without a stand-in");
        assert_eq!(shown.lines().count(), LICENCE.lines().count(), "the same lines");
        assert!(shown.contains("(in each case, the \"SDK\")"), "the curly quotes as plain ones");
    }

    #[test]
    fn the_choice_is_offered_only_while_the_files_are_missing_on_windows() {
        use crate::upscaler::Unavailable;
        let mut cache = OfferCache::default();
        let now = Instant::now();
        for phase in [None, Some(Phase::Failed(download::Failure::Http(404))), Some(Phase::Cancelled), Some(Phase::Connecting)] {
            assert!(cache.offered(true, None, phase, now, || Some(true)), "missing files: offer or retry");
        }
        for reason in [Unavailable::NotWindows, Unavailable::NotVulkan, Unavailable::UpdaterInFolder, Unavailable::SignatureRejected, Unavailable::StreamlineRefused, Unavailable::DeviceUnsuitable, Unavailable::GpuNotSupported, Unavailable::EvaluateFailed, Unavailable::FsrUnsupported] {
            assert!(!cache.offered(true, Some(reason), None, now, || panic!("no probe for another refusal")));
        }
        assert!(!cache.offered(false, None, None, now, || panic!("Windows only")));
        assert!(!cache.offered(true, Some(Unavailable::FilesMissing), Some(Phase::Done(PathBuf::from("dlss"))), now, || panic!("installed: no probe")));
        assert!(cache.missing.is_none(), "Done discards the cached probe");
        assert!(!cache.offered(true, Some(Unavailable::FilesMissing), None, now, || Some(false)), "all eight present even if the loader still needs a restart");
        assert!(is_choice("Download DLSS...") && is_choice("download dlss...") && is_choice("Download DLSS…"));
        assert!(!is_choice("DLSS") && !is_choice("Download"));
    }

    #[test]
    fn the_offer_caches_the_probe_until_a_second_or_a_phase_change() {
        let mut cache = OfferCache::default();
        let now = Instant::now();
        assert!(cache.offered(true, None, None, now, || Some(true)));
        assert!(cache.offered(true, Some(crate::upscaler::Unavailable::FilesMissing), None, now + Duration::from_millis(999), || panic!("reuse the probe")));
        assert!(!cache.offered(true, None, None, now + Duration::from_secs(1), || Some(false)), "files appeared");
        assert!(!cache.offered(true, None, None, now + Duration::from_millis(1999), || panic!("cache present files too")));
        assert!(cache.offered(true, None, None, now + Duration::from_secs(2), || Some(true)), "files disappeared");
        assert!(!cache.offered(true, None, Some(Phase::Installing), now + Duration::from_secs(2), || Some(false)), "phase changes discard the probe immediately");
        assert!(cache.offered(true, None, Some(Phase::Failed(download::Failure::Http(404))), now + Duration::from_secs(2), || Some(true)), "a failure also discards it");
        assert!(!cache.offered(true, None, Some(Phase::Done(PathBuf::from("dlss"))), now + Duration::from_secs(3), || panic!("Done skips even an expired probe")));
        assert!(cache.missing.is_none(), "the cached missing result is discarded on Done");
    }

    #[test]
    fn an_unresolved_folder_preserves_only_the_loaders_missing_files_offer() {
        use crate::upscaler::Unavailable::FilesMissing;
        let now = Instant::now();
        // [design] Exercise both initial reasons and both directions of a reason change
        // while the resolution error is cached; never consult or change the environment.
        for reason in [None, Some(FilesMissing)] {
            let mut cache = OfferCache::default();
            assert_eq!(cache.offered(true, reason, None, now, || None), reason.is_some());
            for current in [Some(FilesMissing), None, Some(FilesMissing)] {
                assert_eq!(cache.offered(true, current, None, now + Duration::from_millis(999), || panic!("reuse the resolution error")), current.is_some());
            }
            assert!(!cache.offered(true, Some(FilesMissing), None, now + Duration::from_secs(1), || Some(false)), "resolution recovered: all files present");
            assert!(cache.offered(true, None, None, now + Duration::from_secs(2), || Some(true)), "resolution recovered: missing frame generation files");
        }
        let mut cache = OfferCache::default();
        assert!(cache.offered(true, Some(FilesMissing), None, now, || None));
        assert!(!cache.offered(true, None, Some(Phase::Connecting), now, || Some(false)), "phase changes also discard a cached resolution error");
        assert!(!cache.offered(true, Some(FilesMissing), Some(Phase::Done(PathBuf::from("dlss"))), now, || panic!("Done still skips probing")));
    }

    #[test]
    fn the_offer_checks_all_eight_files_for_ready_and_missing_loaders() {
        let root = std::env::temp_dir().join(format!("nvf-download-offer-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let now = Instant::now();
        for file in download::DOWNLOAD_FILES {
            std::fs::write(root.join(file.name), b"test fixture").unwrap();
        }
        for reason in [None, Some(crate::upscaler::Unavailable::FilesMissing)] {
            assert!(!OfferCache::default().offered(true, reason, None, now, || Some(files_missing(&root))));
            for file in download::DOWNLOAD_FILES {
                let path = root.join(file.name);
                std::fs::remove_file(&path).unwrap();
                assert!(OfferCache::default().offered(true, reason, None, now, || Some(files_missing(&root))), "{} missing", file.name);
                std::fs::write(path, b"test fixture").unwrap();
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_running_choice_counts_the_bytes_fetched() {
        let p = |fetched, to_fetch| download::Progress { phase: Phase::Directory, fetched, to_fetch };
        assert_eq!(percent(&p(0, 0)), 0);
        assert_eq!(percent(&p(30, 60)), 50);
        assert_eq!(percent(&p(90, 60)), 100, "never over");
    }

    #[test]
    fn the_test_aid_reads_its_items() {
        assert_eq!(AidPlan::parse("clear,accept").0, AidPlan { clear: true, accept: true, ..Default::default() });
        assert_eq!(AidPlan::parse("accept, cancel-at:2.5").0, AidPlan { accept: true, cancel_at: Some(2.5), ..Default::default() });
        let (plan, unknown) = AidPlan::parse("cancel,cancel-at:x,skip");
        assert!(plan.cancel && plan.cancel_at.is_none());
        assert_eq!(unknown, ["cancel-at:x", "skip"]);
    }

    #[test]
    fn download_test_hooks_are_ignored_in_player_runs() {
        for value in ["clear,accept,cancel,cancel-at:2", "http://127.0.0.1:9/test.zip"] {
            assert_eq!(test_aid_for_run(Some(value.into()), false), None);
            assert_eq!(test_aid_for_run(Some(value.into()), true), Some(value.into()));
        }
        assert_eq!(test_aid_for_run(None, true), None);
        assert_eq!(test_aid_for_run(Some("  ".into()), true), None);
    }

    #[test]
    fn the_aids_clear_empties_only_a_smoke_folder_and_only_the_listed_files() {
        let root = std::env::temp_dir().join(format!("nvf-dlss-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let smoke = root.join("smoke-dlss-test");
        let other = root.join("dlss");
        for dir in [&smoke, &other] {
            std::fs::create_dir_all(dir).unwrap();
            for f in download::DOWNLOAD_FILES {
                std::fs::write(dir.join(f.name), b"x").unwrap();
            }
            std::fs::write(dir.join("keep.txt"), b"x").unwrap();
        }
        assert_eq!(clear_folder(&smoke), Ok(download::DOWNLOAD_FILES.len()));
        assert!(download::DOWNLOAD_FILES.iter().all(|f| !smoke.join(f.name).exists()));
        assert!(smoke.join("keep.txt").exists(), "an unlisted file stays");
        assert!(clear_folder(&other).is_err(), "another folder is never emptied");
        assert!(download::DOWNLOAD_FILES.iter().all(|f| other.join(f.name).exists()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_attribution_uses_none_of_the_marks_that_need_nvidias_approval() {
        for line in ATTRIBUTION {
            for mark in ["NVIDIA RTX", "GeForce RTX", "Quadro RTX"] {
                assert!(!line.contains(mark), "{line:?} names {mark}");
            }
        }
        assert_eq!(attribution_lines()[0], "NVIDIA DLSS");
    }
}
