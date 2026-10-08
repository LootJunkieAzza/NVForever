//! `nvf_capture`: an xNVSE plugin for the ORIGINAL Fallout: New Vegas that runs parity-lab
//! capture jobs and writes what the game reports (docs/spec-notes/xnvse-capture.md).
//! It is a capture tool, not part of the NVForever engine.
//!
//! Everything goes through xNVSE's plugin interface and the game's script language;
//! the plugin reads no game memory. It writes only into the capture directory.

pub mod auto;
pub mod ffi;
pub mod runner;

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use auto::{Auto, AutoAction, AutoState, AUTO_FILE, AUTO_USED_FILE};
use ffi::*;
use runner::{Event, Host, Job, Runner, Value};

pub const PLUGIN_VERSION: u32 = 4;
const PLUGIN_NAME: &CStr = c"NVForeverCapture";
pub const DEFAULT_DIR: &str = r"C:\fnv-private\capture";

/// The capture directory: `dir=<path>` in `nvf_capture.ini` (next to the DLL, i.e. in
/// `Data\NVSE\Plugins`), else the default.
pub fn capture_dir(ini_text: Option<&str>) -> PathBuf {
    ini_text
        .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("dir=").map(|d| d.trim().to_string())))
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DIR))
}

/// Pending jobs: `<dir>\jobs\*.steps.json`, by file name; unreadable ones are logged.
pub fn scan_jobs(dir: &Path, log: &mut dyn FnMut(String)) -> Vec<Job> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir.join("jobs")) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.to_string_lossy().ends_with(".steps.json")).collect(),
        Err(e) => {
            log(format!("cannot read {}: {e}", dir.join("jobs").display()));
            return Vec::new();
        }
    };
    files.sort();
    let mut jobs = Vec::new();
    for f in files {
        match fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| Job::parse(&t)) {
            Ok(j) => jobs.push(j),
            Err(e) => log(format!("skipping {}: {e}", f.display())),
        }
    }
    jobs
}

/// Reads the automatic-run control file, if any, and renames it so that the next
/// launch is a normal session (docs/spec-notes/xnvse-capture.md §4).
pub fn take_auto(dir: &Path, now: u64, log: &mut dyn FnMut(String)) -> Option<Auto> {
    let path = dir.join(AUTO_FILE);
    let text = fs::read_to_string(&path).ok()?;
    if let Err(e) = fs::rename(&path, dir.join(AUTO_USED_FILE)) {
        log(format!("cannot rename {}: {e}; ignoring it", path.display()));
        return None;
    }
    match Auto::parse(&text, now) {
        Ok(Some(a)) => {
            log(format!("automatic run: {} menu line(s), {} quit line(s)", a.menu_console.len(), a.quit_console.len()));
            Some(a)
        }
        Ok(None) => {
            log(format!("{AUTO_FILE} has expired; normal session"));
            None
        }
        Err(e) => {
            log(format!("{AUTO_FILE} not usable ({e}); normal session"));
            None
        }
    }
}

/// PostLoadGame's success flag. xNVSE passes the bool *in* the message's data field,
/// not behind a pointer: reading it as a pointer crashed the game (access violation in
/// this DLL on the first save load, 2026-10-07) [observed]. Only the low byte counts.
pub fn post_load_succeeded(data: *mut c_void) -> bool {
    (data as usize) & 0xFF != 0
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Runs one console line. The game may do a lot inside the call: `load <save>` loads
/// the save before returning and sends this plugin messages meanwhile [observed
/// 2026-10-08: the call did not return while the plugin held its lock; the game hung].
fn console_line(console: &ConsoleInterface, line: &str) -> Result<(), String> {
    let c = CString::new(line).map_err(|_| "console line contains NUL".to_string())?;
    // SAFETY: interface pointer from xNVSE; null calling reference is allowed.
    let ok = unsafe {
        if console.version >= 3 {
            (console.run_script_line2)(c.as_ptr(), std::ptr::null_mut(), false)
        } else {
            (console.run_script_line)(c.as_ptr(), std::ptr::null_mut())
        }
    };
    if ok {
        Ok(())
    } else {
        Err("console line failed".into())
    }
}

struct GameHost {
    console: &'static ConsoleInterface,
    script: &'static ScriptInterface,
    compiled: HashMap<String, *mut c_void>,
}

impl GameHost {
    fn compile(&mut self, text: &str) -> Result<*mut c_void, String> {
        if let Some(&s) = self.compiled.get(text) {
            return Ok(s);
        }
        let c = CString::new(text).map_err(|_| "script text contains NUL".to_string())?;
        // SAFETY: interface pointer from xNVSE, valid for the process lifetime.
        let s = unsafe { (self.script.compile_script)(c.as_ptr()) };
        if s.is_null() {
            return Err("script did not compile".into());
        }
        self.compiled.insert(text.to_string(), s);
        Ok(s)
    }
}

impl Host for GameHost {
    fn script(&mut self, text: &str) -> Result<Value, String> {
        let s = self.compile(text)?;
        let mut el = Element::empty();
        // SAFETY: `s` came from CompileScript; `el` outlives the call; no extra args.
        let ok = unsafe { (self.script.call_function)(s, std::ptr::null_mut(), std::ptr::null_mut(), &mut el, 0) };
        if !ok {
            return Err("script call failed".into());
        }
        // A string result is owned by the game's heap; it is copied and left there (a
        // few bytes per query) because freeing it needs game internals.
        unsafe {
            match el.kind {
                ELEMENT_INVALID => Ok(Value::Nothing),
                ELEMENT_NUMBER => Ok(Value::Number(el.data.num)),
                ELEMENT_STRING if !el.data.text.is_null() => {
                    Ok(Value::Text(CStr::from_ptr(el.data.text).to_string_lossy().into_owned()))
                }
                ELEMENT_STRING => Ok(Value::Text(String::new())),
                k => Err(format!("unsupported result type {k} (return numbers or strings)")),
            }
        }
    }

    fn console(&mut self, line: &str) -> Result<(), String> {
        console_line(self.console, line)
    }

    fn key_down(&mut self, scancode: u32) -> bool {
        let text = format!("Begin Function {{ }}\nint iK\nlet iK := IsKeyPressed {scancode}\nSetFunctionValue iK\nEnd\n");
        matches!(self.script(&text), Ok(Value::Number(n)) if n != 0.0)
    }

    fn show(&mut self, text: &str) {
        let clean: String = text.chars().map(|c| if c == '"' || c == '%' || c.is_control() { '\'' } else { c }).collect();
        let script = format!("Begin Function {{ }}\nMessageEx \"{clean}\"\nEnd\n");
        let _ = self.script(&script);
    }
}

struct Plugin {
    dir: PathBuf,
    runner: Runner,
    host: GameHost,
    /// Output file of the running job.
    out: Option<(String, fs::File)>,
    /// Automatic run (`auto.json`), if one was requested.
    auto: Option<AutoState>,
    /// Game loops seen so far (the first one is logged).
    loops: u64,
}

// SAFETY: the plugin state is only touched from xNVSE callbacks on the game's main
// thread; the raw pointers it holds are xNVSE/game objects valid for the process.
unsafe impl Send for Plugin {}

static PLUGIN: Mutex<Option<Plugin>> = Mutex::new(None);
static INI_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn log_to(dir: &Path, msg: &str) {
    let _ = fs::create_dir_all(dir);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("nvf_capture.log")) {
        let _ = writeln!(f, "{} {msg}", now());
    }
}

impl Plugin {
    fn log(&self, msg: &str) {
        log_to(&self.dir, msg);
    }

    /// The automatic run's console lines due in this game loop, if any. They are run by
    /// the caller after the plugin's lock is released (see `console_line`).
    fn auto_tick(&mut self) -> Option<(&'static str, Vec<String>)> {
        let a = self.auto.as_mut()?;
        match a.tick()? {
            AutoAction::RunMenuConsole => Some(("menu", a.cfg.menu_console.clone())),
            AutoAction::RunQuitConsole => Some(("quit", a.cfg.quit_console.clone())),
        }
    }

    fn post_load(&mut self, succeeded: bool) {
        if !succeeded {
            self.log("save load reported failure; nothing to do");
            return;
        }
        let dir = self.dir.clone();
        let jobs = scan_jobs(&dir, &mut |s| log_to(&dir, &s));
        self.log(&format!("save loaded; {} pending job(s)", jobs.len()));
        if let Some(a) = &mut self.auto {
            a.game_loaded();
        }
        let events = self.runner.game_loaded(jobs, &mut self.host);
        self.handle(events);
    }

    /// One game loop; returns automatic-run console lines to run without the lock.
    fn main_loop(&mut self) -> Option<(&'static str, Vec<String>)> {
        self.loops += 1;
        if self.loops == 1 {
            self.log("first game loop");
        }
        let events = self.runner.tick(&mut self.host);
        self.handle(events);
        self.auto_tick()
    }

    fn handle(&mut self, events: Vec<Event>) {
        for e in events {
            match e {
                Event::Output { job, line } => {
                    if self.out.as_ref().map(|(j, _)| j != &job).unwrap_or(true) {
                        let out_dir = self.dir.join("out");
                        let _ = fs::create_dir_all(&out_dir);
                        let path = out_dir.join(format!("{job}.{}.jsonl", now()));
                        match fs::File::create(&path) {
                            Ok(f) => self.out = Some((job.clone(), f)),
                            Err(err) => {
                                self.log(&format!("cannot create {}: {err}", path.display()));
                                self.out = None;
                                continue;
                            }
                        }
                    }
                    if let Some((_, f)) = &mut self.out {
                        let _ = writeln!(f, "{line}");
                        let _ = f.flush();
                    }
                }
                Event::Finished { job } => {
                    self.out = None;
                    let done = self.dir.join("jobs").join("done");
                    let _ = fs::create_dir_all(&done);
                    let name = format!("{job}.steps.json");
                    if let Err(err) = fs::rename(self.dir.join("jobs").join(&name), done.join(&name)) {
                        self.log(&format!("finished {job}, but could not move its job file: {err}"));
                    } else {
                        self.log(&format!("finished {job}"));
                    }
                }
                Event::Aborted { job } => {
                    self.out = None;
                    self.log(&format!("aborted {job}"));
                }
                Event::AllDone => {
                    if let Some(a) = &mut self.auto {
                        a.all_done();
                    }
                    self.log("automatic run: no job left");
                }
            }
        }
    }
}

/// A save load reported while the plugin was busy (0 none, 1 succeeded, 2 failed),
/// handled at the next message.
static DEFERRED_LOAD: AtomicU8 = AtomicU8::new(0);

unsafe extern "C" fn on_message(msg: *mut Message) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if msg.is_null() {
            return;
        }
        let m = &*msg;
        if m.kind != MSG_MAIN_GAME_LOOP && m.kind != MSG_POST_LOAD_GAME {
            return;
        }
        // The game can send messages from inside a call the plugin makes (a console
        // `load`, perhaps a script); waiting for the lock there would hang the game. A load
        // is kept for later, a game loop is skipped.
        let mut guard = match PLUGIN.try_lock() {
            Ok(g) => g,
            Err(TryLockError::WouldBlock) => {
                if m.kind == MSG_POST_LOAD_GAME {
                    DEFERRED_LOAD.store(if post_load_succeeded(m.data) { 1 } else { 2 }, Ordering::SeqCst);
                }
                return;
            }
            Err(TryLockError::Poisoned(_)) => return,
        };
        let Some(p) = guard.as_mut() else { return };
        let deferred = DEFERRED_LOAD.swap(0, Ordering::SeqCst);
        if deferred != 0 {
            p.log("handling a save load reported while the plugin was busy");
            p.post_load(deferred == 1);
        }
        let lines = if m.kind == MSG_POST_LOAD_GAME {
            p.post_load(post_load_succeeded(m.data));
            None
        } else {
            p.main_loop()
        };
        if let Some((what, lines)) = lines {
            let (console, dir) = (p.host.console, p.dir.clone());
            drop(guard);
            for line in lines {
                log_to(&dir, &format!("automatic run: {what} console {line:?}"));
                let r = console_line(console, &line);
                log_to(&dir, &format!("automatic run: {what} console {line:?}: {}", if r.is_ok() { "ok" } else { "failed" }));
            }
        }
    }));
}

/// # Safety
/// Called by xNVSE with valid pointers.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn NVSEPlugin_Query(nvse: *const NvseInterface, info: *mut PluginInfo) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if nvse.is_null() || info.is_null() {
            return false;
        }
        (*info).info_version = 1;
        (*info).name = PLUGIN_NAME.as_ptr();
        (*info).version = PLUGIN_VERSION;
        let n = &*nvse;
        let game_dir = CStr::from_ptr((n.get_runtime_directory)()).to_string_lossy().into_owned();
        let ini = fs::read_to_string(Path::new(&game_dir).join(r"Data\NVSE\Plugins\nvf_capture.ini")).ok();
        let dir = capture_dir(ini.as_deref());
        let ok = n.is_editor == 0
            && n.nvse_version >= MIN_NVSE
            && (n.runtime_version == RUNTIME_1_4_0_525 || n.runtime_version == RUNTIME_1_4_0_525_NOGORE);
        log_to(
            &dir,
            &format!(
                "query: xNVSE 0x{:08X}, runtime 0x{:08X}, editor {}: {}",
                n.nvse_version,
                n.runtime_version,
                n.is_editor,
                if ok { "loading" } else { "declined (needs xNVSE >= 6.4.9 and game 1.4.0.525)" }
            ),
        );
        if let Ok(mut d) = INI_DIR.lock() {
            *d = Some(dir);
        }
        ok
    }))
    .unwrap_or(false)
}

/// # Safety
/// Called by xNVSE with a valid pointer.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn NVSEPlugin_Load(nvse: *const NvseInterface) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let n = &*nvse;
        let dir = INI_DIR.lock().ok().and_then(|d| d.clone()).unwrap_or_else(|| PathBuf::from(DEFAULT_DIR));
        let console = (n.query_interface)(INTERFACE_CONSOLE) as *const ConsoleInterface;
        let messaging = (n.query_interface)(INTERFACE_MESSAGING) as *const MessagingInterface;
        let script = (n.query_interface)(INTERFACE_SCRIPT) as *const ScriptInterface;
        if console.is_null() || messaging.is_null() || script.is_null() {
            log_to(&dir, "load failed: console, messaging or script interface missing");
            return false;
        }
        let handle = (n.get_plugin_handle)();
        let host = GameHost { console: &*console, script: &*script, compiled: HashMap::new() };
        let mut runner = Runner::new(PLUGIN_VERSION, n.nvse_version, n.runtime_version);
        let auto = take_auto(&dir, now(), &mut |s| log_to(&dir, &s));
        runner.set_auto_start(auto.as_ref().map(|a| a.start_frames));
        let auto = auto.map(AutoState::new);
        if let Ok(mut g) = PLUGIN.lock() {
            *g = Some(Plugin { dir: dir.clone(), runner, host, out: None, auto, loops: 0 });
        }
        let ok = ((*messaging).register_listener)(handle, c"NVSE".as_ptr(), on_message);
        log_to(&dir, &format!("loaded (listener registered: {ok}); capture directory {}", dir.display()));
        ok
    }))
    .unwrap_or(false)
}

/// Keeps `c_char` imported for the FFI signatures on every target.
#[allow(dead_code)]
fn _types(_: *const c_char) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_load_flag_is_the_data_value() {
        assert!(post_load_succeeded(std::ptr::without_provenance_mut(1)));
        assert!(!post_load_succeeded(std::ptr::null_mut()));
        assert!(!post_load_succeeded(std::ptr::without_provenance_mut(0x100)), "only the low byte is the bool");
    }

    #[test]
    fn capture_dir_from_ini() {
        assert_eq!(capture_dir(None), PathBuf::from(DEFAULT_DIR));
        assert_eq!(capture_dir(Some("; comment\ndir= D:\\caps \n")), PathBuf::from("D:\\caps"));
        assert_eq!(capture_dir(Some("dir=\n")), PathBuf::from(DEFAULT_DIR));
    }

    #[test]
    fn take_auto_renames_the_file_and_honours_expiry() {
        let base = std::env::temp_dir().join(format!("nvf_capture_auto_{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let mut logged = Vec::new();
        assert!(take_auto(&base, 0, &mut |s| logged.push(s)).is_none(), "no file: normal session");
        let file = r#"{"format": "fnv.capture.auto/1", "expires": 100, "menu_console": ["load x"]}"#;
        fs::write(base.join(AUTO_FILE), file).unwrap();
        let a = take_auto(&base, 50, &mut |s| logged.push(s)).unwrap();
        assert_eq!(a.menu_console, ["load x"]);
        assert!(!base.join(AUTO_FILE).exists() && base.join(AUTO_USED_FILE).exists());
        fs::write(base.join(AUTO_FILE), file).unwrap();
        assert!(take_auto(&base, 101, &mut |s| logged.push(s)).is_none(), "expired");
        assert!(!base.join(AUTO_FILE).exists(), "an expired file is renamed too");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn scan_jobs_sorts_and_skips_bad_files() {
        let base = std::env::temp_dir().join(format!("nvf_capture_test_{}", std::process::id()));
        let jobs = base.join("jobs");
        fs::create_dir_all(&jobs).unwrap();
        let job = |n: &str| format!(r#"{{"format": "fnv.capture.steps/1", "name": "{n}", "steps": []}}"#);
        fs::write(jobs.join("b.steps.json"), job("b")).unwrap();
        fs::write(jobs.join("a.steps.json"), job("a")).unwrap();
        fs::write(jobs.join("c.steps.json"), "{").unwrap();
        fs::write(jobs.join("notes.txt"), "x").unwrap();
        let mut logged = Vec::new();
        let got = scan_jobs(&base, &mut |s| logged.push(s));
        assert_eq!(got.iter().map(|j| j.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(logged.len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }
}
