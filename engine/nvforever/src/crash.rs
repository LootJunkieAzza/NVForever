//! Crash reports and the run log (owner's playtest 2026-10-11: the game crashed outside
//! Crimson Caravan and nothing was kept to tell why).
//!
//! **The run log.** Everything the game logs (the console's lines, without colours) is
//! also written to `<home>\logs\nvforever-<UTC time>-<pid>.log`, NVForever's own home
//! (`fnv_install::EngineHome`: `Documents\My Games\NVForever` or `NVFOREVER_HOME`), never the
//! game's folder nor `Documents\My Games\FalloutNV` (docs/design/install-layout.md;
//! every path is checked by `fnv_install::guard_write`). Lines are written as they come,
//! so the log holds everything up to a crash. The last [`KEEP_LOGS`] run logs are kept.
//! A run that ends normally writes [`CLEAN_EXIT`] as it exits; at the start the
//! newest earlier log is checked, and one without that line and without a crash report
//! is named in the log: the process ended in a way no Rust code saw (an access violation
//! in a driver, a stack overflow, the process killed), and its log shows what it was
//! doing last.
//!
//! **Crash reports.** A panic on any thread writes `<home>\logs\crash-<UTC time>-<pid>.txt`:
//! the thread, the message and where it was raised, the backtrace, the command line,
//! where the camera was (worldspace or interior, cell, position; [`note_place`], kept
//! every frame) and the name of the run log; the panic is logged too, and the default
//! hook prints it to the console as before.
//!
//! Not covered: native crashes (they skip Rust's panic hook; see the run log check above).

use bevy::log::tracing_subscriber;
use bevy::log::BoxedLayer;
use bevy::prelude::*;
use std::fs::File;
use std::io::{LineWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Run logs kept in the logs folder, the newest ones [our choice].
pub const KEEP_LOGS: usize = 10;

/// Logged when a run ends normally.
pub const CLEAN_EXIT: &str = "run log: the game exited normally";
/// Written when the game stops before it starts (a bad argument, data that cannot be read).
const ERROR_EXIT: &str = "run log: the game stopped at the start:";

/// The run log, once opened: its path and the writer the log layer takes.
struct RunLog {
    path: PathBuf,
    writer: Mutex<Option<LineWriter<File>>>,
}

static RUN_LOG: OnceLock<RunLog> = OnceLock::new();
/// Where crash reports go.
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
/// The command line, for crash reports.
static COMMAND: OnceLock<String> = OnceLock::new();
/// Where the camera is (updated every frame by [`note_place`]).
static PLACE: Mutex<String> = Mutex::new(String::new());

/// The logs folder in NVForever's home, created; `None` (with the reason) when there is
/// no home or it may not be written there.
fn logs_dir(data: &Path) -> Result<PathBuf, String> {
    let root = fnv_install::EngineHome::default_root().ok_or("no NVForever home folder (set NVFOREVER_HOME)")?;
    let dir = fnv_install::EngineHome::new(root).logs();
    let game = fnv_install::GameInstall::open(data).ok();
    fnv_install::guard_write(&dir, game.as_ref()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian (H. Hinnant's
/// `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A UTC time stamp for file names, `YYYYMMDD-HHMMSS`.
fn stamp(unix_secs: u64) -> String {
    let (days, secs) = ((unix_secs / 86_400) as i64, unix_secs % 86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Crash reports kept in the logs folder, the newest ones [our choice].
pub const KEEP_REPORTS: usize = 20;

/// The files named `<prefix>…<ext>` to delete so that `keep` remain, given the names in
/// the folder: they sort by their time stamp, oldest first.
fn old_files(names: &[String], prefix: &str, ext: &str, keep: usize) -> Vec<String> {
    let mut logs: Vec<&String> = names.iter().filter(|n| n.starts_with(prefix) && n.ends_with(ext)).collect();
    logs.sort();
    let n = logs.len().saturating_sub(keep);
    logs.into_iter().take(n).cloned().collect()
}

/// Whether a run log's text shows a run that did not end normally.
fn ended_abnormally(text: &str) -> bool {
    !text.contains(CLEAN_EXIT) && !text.contains(ERROR_EXIT) && !text.contains("crash report written to")
}

/// Open the run log and set the panic hook. Call first thing in `main`; afterwards the
/// log plugin takes [`file_layer`]. Failing to open the log only costs the file.
pub fn install(data: &Path) {
    let _ = COMMAND.set(std::env::args().collect::<Vec<_>>().join(" "));
    let dir = match logs_dir(data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("run log: not written ({e})");
            set_hook();
            return;
        }
    };
    let _ = LOG_DIR.set(dir.clone());
    // The newest earlier run log, before this one is opened and old ones go.
    let mut names: Vec<String> = std::fs::read_dir(&dir).map(|r| r.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default();
    names.sort();
    let previous = names.iter().rev().find(|n| n.starts_with("nvforever-") && n.ends_with(".log")).map(|n| dir.join(n));
    let path = dir.join(format!("nvforever-{}-{}.log", stamp(now_secs()), std::process::id()));
    names.push(path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string());
    for old in old_files(&names, "nvforever-", ".log", KEEP_LOGS).into_iter().chain(old_files(&names, "crash-", ".txt", KEEP_REPORTS)) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    match File::create(&path) {
        Ok(f) => {
            let _ = RUN_LOG.set(RunLog { path: path.clone(), writer: Mutex::new(Some(LineWriter::new(f))) });
        }
        Err(e) => eprintln!("run log: {}: {e}", path.display()),
    }
    set_hook();
    test_panic();
    // Said once the log plugin is up (`report_start`).
    let unclean = previous.filter(|p| std::fs::read_to_string(p).is_ok_and(|t| ended_abnormally(&t)));
    let _ = UNCLEAN.set(unclean);
}

static UNCLEAN: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Test aid `NVF_PLAY_TEST_PANIC=<seconds>`: a thread of its own ("nvf-test-panic")
/// panics that long after the first frame (when the camera's place is known), so the
/// crash report can be checked; the game goes on (a panic on a thread of its own does
/// not stop it).
fn test_panic() {
    let Some(secs) = std::env::var("NVF_PLAY_TEST_PANIC").ok().and_then(|v| v.trim().parse::<f32>().ok()) else { return };
    let _ = std::thread::Builder::new().name("nvf-test-panic".into()).spawn(move || {
        while PLACE.lock().map(|p| p.is_empty()).unwrap_or(true) {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        std::thread::sleep(std::time::Duration::from_secs_f32(secs.max(0.0)));
        panic!("test panic (NVF_PLAY_TEST_PANIC)");
    });
}

/// The log plugin's extra layer: the same lines as the console, into the run log.
pub fn file_layer(_app: &mut App) -> Option<BoxedLayer> {
    let log = RUN_LOG.get()?;
    let writer = move || RunLogWriter(log);
    Some(Box::new(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(writer)))
}

/// One write to the run log (the layer makes one per line).
struct RunLogWriter(&'static RunLog);

impl Write for RunLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // A panic while the lock was held must not stop the log.
        let mut w = self.0.writer.lock().unwrap_or_else(|e| e.into_inner());
        match w.as_mut() {
            Some(f) => f.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut w = self.0.writer.lock().unwrap_or_else(|e| e.into_inner());
        w.as_mut().map_or(Ok(()), |f| f.flush())
    }
}

/// The game stops before the log plugin is up: the reason goes into the run log.
pub fn early_exit(why: &str) {
    let Some(log) = RUN_LOG.get() else { return };
    let mut w = log.writer.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = w.as_mut() {
        let _ = writeln!(f, "{ERROR_EXIT} {why}");
    }
}

/// The run log's path, when there is one.
pub fn run_log_path() -> Option<&'static Path> {
    RUN_LOG.get().map(|l| l.path.as_path())
}

/// The text of a crash report.
fn report_text(thread: &str, message: &str, location: &str, place: &str, command: &str, run_log: Option<&Path>, backtrace: &str) -> String {
    format!(
        "NVForever {} crashed (panic)\ntime (UTC): {}\nthread: {thread}\nmessage: {message}\nat: {location}\nwhere: {}\ncommand line: {command}\nrun log: {}\n\nbacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        stamp(now_secs()),
        if place.is_empty() { "not known yet (before the first frame)" } else { place },
        run_log.map_or_else(|| "none".to_string(), |p| p.display().to_string()),
    )
}

fn set_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("unnamed").to_string();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(not text)".to_string());
        let location = info.location().map_or_else(|| "unknown".to_string(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let place = PLACE.try_lock().map(|p| p.clone()).unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let text = report_text(&thread, &message, &location, &place, COMMAND.get().map_or("", |s| s.as_str()), run_log_path(), &backtrace);
        let written = LOG_DIR.get().map(|dir| {
            let path = dir.join(format!("crash-{}-{}.txt", stamp(now_secs()), std::process::id()));
            (std::fs::write(&path, &text).map(|_| path.clone()), path)
        });
        let place = if place.is_empty() { "not known yet" } else { place.as_str() };
        match &written {
            Some((Ok(p), _)) => error!("panic on thread '{thread}' at {location}: {message}; where: {place}; crash report written to {}", p.display()),
            Some((Err(e), p)) => error!("panic on thread '{thread}' at {location}: {message}; the crash report could not be written to {}: {e}", p.display()),
            None => error!("panic on thread '{thread}' at {location}: {message} (no logs folder for a crash report)"),
        }
        if let Some(l) = RUN_LOG.get() {
            if let Some(f) = l.writer.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                let _ = f.flush();
            }
        }
        default(info);
    }));
}

/// Say where the run log is, and whether the run before this one ended abnormally.
pub fn report_start() {
    match run_log_path() {
        Some(p) => info!("run log: {}", p.display()),
        None => info!("run log: none"),
    }
    if let Some(Some(p)) = UNCLEAN.get() {
        warn!("run log: the previous run did not end normally and left no crash report (a native crash, or the process was ended): {}", p.display());
    }
}

/// Keep where the camera is for a crash report: worldspace or interior, cell, position.
pub fn note_place(cam: Query<&GlobalTransform, With<crate::fly::FlyCam>>, streamer: Res<crate::world::Streamer>, world: Res<crate::world::WorldData>, mut last: Local<String>) {
    let Ok(cam) = cam.single() else { return };
    let g = crate::world::bevy_to_game(cam.translation());
    let space = match streamer.interior {
        Some(c) => format!("interior {}", world.index.cells.get(&c).and_then(|c| c.edid.clone()).unwrap_or_else(|| format!("{c:08X}"))),
        None => {
            let (x, y) = fnv_world::cell_of(g.x, g.y);
            let name = world.grid_cell((x, y)).and_then(|c| c.edid.clone()).map(|n| format!(" {n}")).unwrap_or_default();
            format!("{} cell {x},{y}{name}", world.index.worldspaces.get(&world.world).and_then(|w| w.edid.clone()).unwrap_or_default())
        }
    };
    let text = format!("{space}, position {:.0}, {:.0}, {:.0}", g.x, g.y, g.z);
    if *last != text {
        if let Ok(mut p) = PLACE.try_lock() {
            p.clone_from(&text);
        }
        *last = text;
    }
}

/// Log [`CLEAN_EXIT`] when the run ends normally.
pub fn report_exit(mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_some() {
        info!("{CLEAN_EXIT}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc_calendar_times() {
        assert_eq!(stamp(0), "19700101-000000");
        // 2026-10-12 08:30:05 UTC.
        assert_eq!(stamp(1_791_793_805), "20261012-083005");
        // A leap day.
        assert_eq!(stamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn the_oldest_run_logs_go_and_nothing_else() {
        let names: Vec<String> =
            ["nvforever-20261012-080000-1.log", "crash-20261001-000000-5.txt", "nvforever-20261010-120000-9.log", "notes.txt", "nvforever-20261011-090000-3.log"].map(String::from).to_vec();
        assert_eq!(old_files(&names, "nvforever-", ".log", 2), vec!["nvforever-20261010-120000-9.log".to_string()]);
        assert!(old_files(&names, "nvforever-", ".log", 10).is_empty());
        assert_eq!(old_files(&names, "crash-", ".txt", 0), vec!["crash-20261001-000000-5.txt".to_string()]);
    }

    #[test]
    fn a_log_without_the_exit_line_or_a_crash_report_ended_abnormally() {
        assert!(!ended_abnormally(&format!("line\n{CLEAN_EXIT}\n")));
        assert!(!ended_abnormally("line\nERROR panic on thread 'main' at x: boom; crash report written to C:\\x\n"));
        assert!(ended_abnormally("line\nINFO streaming cell -2,26\n"));
    }

    #[test]
    fn a_report_has_thread_message_place_and_backtrace() {
        let t = report_text("Compute Task Pool (3)", "index out of bounds", "src/x.rs:1:2", "WastelandNV cell -2,26 CrimsonCaravan, position 1, 2, 3", "nvforever --play", Some(Path::new("C:/h/logs/run.log")), "0: main");
        for want in ["thread: Compute Task Pool (3)", "message: index out of bounds", "at: src/x.rs:1:2", "where: WastelandNV cell -2,26 CrimsonCaravan", "run log: C:/h/logs/run.log", "backtrace:\n0: main"] {
            assert!(t.contains(want), "{want} in {t}");
        }
        assert!(report_text("main", "m", "l", "", "", None, "").contains("where: not known yet"));
    }
}
