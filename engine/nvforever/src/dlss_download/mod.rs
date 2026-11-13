//! Download DLSS... (#2250), the player's side: the choice in Settings > Graphics > Upscaler,
//! the dialog with NVIDIA's licence, the progress and the result ([`dialog`]), the test aid,
//! and NVIDIA's attribution at the end of the credits. The work itself is
//! `crate::dlss::download`; `docs/dlss.md` says it for players.
//!
//! - **The choice** [design: #2250]: "Download DLSS…" is a choice of the Upscaler row while
//!   DLSS is held out of reach because its files are missing (`upscaler::Unavailable::
//!   FilesMissing`, which only Windows with Vulkan reaches) and no download has installed them
//!   this session. Picking it opens the dialog and changes nothing in the row; while a
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
//! Test aid `NVF_PLAY_TEST_DLSS_DOWNLOAD=<item>[,…]`: `clear` at the start deletes the four
//! listed files from the DLSS folder, only when `NVF_DLSS_DIR` names a folder whose name
//! starts with `smoke-dlss-` (never another file, never another folder); `accept` presses I
//! accept a second after the licence is shown; `cancel` presses Cancel there instead;
//! `cancel-at:<s>` cancels the download `s` seconds after it started. With the aid set, the
//! dialog closes itself a second after the result is shown.
//!
//! Log lines start `DLSS download:` (the work's own lines, `dlss/download/`, too).

mod dialog;

use bevy::prelude::*;
use crate::dlss::download::{self, Phase};
use fnv_util::words::{tr, trf};

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

/// Whether `name` is the download choice (its name, or its words, any case).
pub(crate) fn is_choice(name: &str) -> bool {
    let name = name.trim();
    name.eq_ignore_ascii_case(CHOICE) || name == CHOICE_WORDS
}

/// The rule for offering the choice: on Windows, with DLSS's files missing, and no download
/// that installed them this session (the status may still say missing for a second after).
fn offer(windows: bool, files_missing: bool, last: Option<&Phase>) -> bool {
    windows && files_missing && !matches!(last, Some(Phase::Done(_)))
}

/// Whether the Upscaler row offers Download DLSS… now, `files_missing` being DLSS's reason
/// (`upscaler::Unavailable::FilesMissing`).
pub(crate) fn offered(files_missing: bool) -> bool {
    offer(cfg!(windows), files_missing, download::current().map(|h| h.progress().phase).as_ref())
}

/// The share of the archive's bytes fetched, in percent (0 until the directory was read).
fn percent(p: &download::Progress) -> u64 {
    if p.to_fetch == 0 { 0 } else { (p.fetched.min(p.to_fetch) * 100) / p.to_fetch }
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

#[cfg(windows)]
fn shell_open(target: &std::ffi::OsStr) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide(std::ffi::OsStr::new("open")), wide(target));
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call; no window, no
    // parameters, no working folder.
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    // [documented: ShellExecuteW, learn.microsoft.com: a value greater than 32 is success]
    let code = result as isize;
    if code > 32 { Ok(()) } else { Err(format!("ShellExecuteW answered {code}")) }
}

#[cfg(not(windows))]
fn shell_open(_target: &std::ffi::OsStr) -> Result<(), String> {
    Err("only on Windows".into())
}

/// The DLSS folder, made if it is not there yet, for "Open dlss folder".
pub(crate) fn open_dlss_folder() {
    match download::target_folder() {
        Ok(folder) => match std::fs::create_dir_all(&folder) {
            Ok(()) => open_in_shell(folder.into_os_string()),
            Err(e) => warn!("DLSS download: could not make {}: {e}", folder.display()),
        },
        Err(why) => warn!("DLSS download: no DLSS folder: {why}"),
    }
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
        let value = std::env::var(AID).ok().filter(|v| !v.trim().is_empty())?;
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
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.starts_with(AID_FOLDER_PREFIX) {
        return Err(format!("{} is not a {AID_FOLDER_PREFIX}* folder; nothing deleted", folder.display()));
    }
    let mut cleared = 0;
    for file in download::DOWNLOAD_FILES {
        let path = folder.join(file.name);
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
        assert!(shown.contains("(the \"SDK\")"), "the curly quotes as plain ones");
    }

    #[test]
    fn the_choice_is_offered_only_while_the_files_are_missing_on_windows() {
        assert!(offer(true, true, None));
        assert!(offer(true, true, Some(&Phase::Failed(download::Failure::Http(404)))), "after a failure: try again");
        assert!(offer(true, true, Some(&Phase::Cancelled)));
        assert!(offer(true, true, Some(&Phase::Connecting)), "while it runs: the dialog again");
        assert!(!offer(true, true, Some(&Phase::Done(PathBuf::from("dlss")))), "installed: a restart, not another download");
        assert!(!offer(true, false, None), "the files are there, or DLSS is out of reach for another reason");
        assert!(!offer(false, true, None), "Windows only");
        assert!(is_choice("Download DLSS...") && is_choice("download dlss...") && is_choice("Download DLSS…"));
        assert!(!is_choice("DLSS") && !is_choice("Download"));
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
