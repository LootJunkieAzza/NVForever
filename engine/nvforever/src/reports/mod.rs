//! Opt-in player reports (docs/design/player-reports.md; #2055): the choice, the session
//! summary and sending it. The server is LootJunkieAzza/NVForever-Data.
//!
//! - **The choice** ([`decide`], at the start, before the window): `[Reports]` in the home's
//!   `NVForever.ini` (`fnv_install::reports`). A player's start that has no answer for the
//!   current [`CONSENT_VERSION`] shows the popup once; the answer is stored at once. Off,
//!   whatever the INI says, in a developer or smoke run (`--data` / `NVFOREVER_DATA`), a
//!   run a test aid drives, a load handed over from an earlier run, and with
//!   `NVF_REPORTS=0`. `NVF_REPORTS=1` turns it on without asking (tests; such a run sends
//!   only to `NVF_REPORTS_ENDPOINT`, never to the project's server).
//! - **The summary** (summary.rs, collected here): written to
//!   `<home>\reports\pending\session-<UTC time>-<pid>.json` every [`WRITE_EVERY`] and at
//!   exit, so a crash leaves the latest copy. Only while reports are on.
//! - **Sending** (upload.rs): at the next start, on a thread of its own, each earlier
//!   session is finished (how it ended, from the run log and crash report crash.rs left;
//!   its run log gzipped beside it) and sent.
//!
//! Nothing is sent from the game folder or `Documents\My Games\FalloutNV`: every file is
//! in the home and passes `fnv_install::guard_write`.

pub mod summary;
pub mod upload;
mod warnings;

use bevy::prelude::*;
use fnv_install::reports::Reports;
use fnv_install::EngineHome;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use summary::{End, Place, Summary};

pub use warnings::layer as warnings_layer;

/// The version of what is sent (player-reports.md §3) the player is asked about; raised
/// only when what is sent grows in kind, which asks again.
pub const CONSENT_VERSION: u32 = 1;
/// How often the running session's summary is rewritten.
pub const WRITE_EVERY: Duration = Duration::from_secs(60);
/// A run log larger than this is cut to its first and last lines before it is sent (§3.1).
const LOG_CAP_BYTES: usize = 2 * 1024 * 1024;
const LOG_HEAD_LINES: usize = 500;
const LOG_TAIL_LINES: usize = 5000;

/// Reports are on for this run: who to send as and where.
#[derive(Debug, Clone, Resource)]
pub struct Active {
    pub install_id: String,
    pub delete_token: String,
    pub endpoint: String,
    /// Sending is allowed (off in a test run without its own endpoint).
    pub may_send: bool,
}

static ACTIVE: OnceLock<Option<Active>> = OnceLock::new();

/// Folders hidden in everything a report sends, besides the home folder that
/// `crash::redact` hides: the Data folder, the game folder and NVForever's own folder (a
/// game folder outside the home can still carry a name). Set by [`ReportsPlugin`] before
/// the upload thread starts.
static HIDDEN_FOLDERS: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();

/// `text` as a report sends it: the home folder and [`HIDDEN_FOLDERS`] written as names.
pub fn redact(text: &str) -> String {
    redact_with(text, HIDDEN_FOLDERS.get().map_or(&[], Vec::as_slice))
}

fn redact_with(text: &str, folders: &[(String, &'static str)]) -> String {
    let mut out = crate::crash::redact(text).into_owned();
    for (folder, shown) in folders {
        out = crate::crash::redact_folder(&out, folder, shown).into_owned();
    }
    out
}

/// The folders [`redact`] hides for this run, longest (innermost) first; a root or a drive
/// is left out, as it would hide every path.
fn folders_to_hide(data: &Path, exe_dir: Option<&Path>) -> Vec<(String, &'static str)> {
    let data = std::path::absolute(data).unwrap_or_else(|_| data.to_path_buf());
    let mut v: Vec<(String, &'static str)> = Vec::new();
    let mut add = |p: &Path, shown: &'static str| {
        let s = p.to_string_lossy();
        let s = s.trim_end_matches(['\\', '/']);
        if s.len() > 3 && !v.iter().any(|(f, _)| f.eq_ignore_ascii_case(s)) {
            v.push((s.to_string(), shown));
        }
    };
    add(&data, "<Data>");
    if let Some(game) = data.parent() {
        add(game, "<game>");
    }
    if let Some(exe) = exe_dir {
        add(exe, "<NVForever>");
    }
    v.sort_by_key(|(f, _)| std::cmp::Reverse(f.len()));
    v
}

/// This run's choice, once [`decide`] made it.
pub fn active() -> Option<&'static Active> {
    ACTIVE.get().and_then(Option::as_ref)
}

/// A random UUID v4, as text.
fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// A random token for deleting this install's reports (§5.1).
fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Turn reports on or off in the INI (the popup's answer, or the settings switch). On makes
/// a new install id and delete token when there are none; off clears them (§2.2: turning
/// it on again makes new ones). Also keeps the settings page's own `send-reports` row in step.
pub fn store_choice(home: &EngineHome, on: bool) -> Result<Reports, String> {
    let mut r = Reports::load(home);
    r.send = Some(on);
    r.consent_version = Some(CONSENT_VERSION);
    if on {
        r.install_id = r.install_id.or_else(|| Some(new_uuid()));
        r.delete_token = r.delete_token.or_else(|| Some(new_token()));
    } else {
        r.install_id = None;
        r.delete_token = None;
    }
    r.save(home).map_err(|e| e.to_string())?;
    // The settings page stores its rows in [Settings]; its row for this must agree, or the
    // page would put the old value back at the next start.
    let file = home.config_file();
    if let Ok(text) = std::fs::read_to_string(&file) {
        let text = fnv_install::ini_set(&text, "Settings", SETTING_KEY, if on { "1" } else { "0" });
        if fnv_install::guard_write(&file, None).is_ok() {
            let _ = std::fs::write(&file, text);
        }
    }
    Ok(r)
}

/// Why reports are off for a run that may not ask or send, if one of the reasons holds.
fn forced_off(developer_data: bool, handoff: bool) -> Option<&'static str> {
    if std::env::var("NVF_REPORTS").is_ok_and(|v| v.trim() == "0") {
        return Some("NVF_REPORTS=0");
    }
    if handoff {
        return Some("a load handed over from the run before");
    }
    if std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run)) {
        return Some("a test aid drives this run");
    }
    developer_data.then_some("a developer or smoke run (--data)")
}

/// Settle this run's choice (module notes); logs it with `eprintln!` (before the log
/// plugin). `developer_data`: `--data` / `NVFOREVER_DATA` was given.
pub fn decide(developer_data: bool, handoff: bool) {
    let _ = ACTIVE.set(choose(developer_data, handoff));
}

fn choose(developer_data: bool, handoff: bool) -> Option<Active> {
    let home = EngineHome::user()?;
    let test_on = std::env::var("NVF_REPORTS").is_ok_and(|v| v.trim() == "1");
    let endpoint_env = std::env::var("NVF_REPORTS_ENDPOINT").ok().filter(|v| !v.trim().is_empty());
    if !test_on {
        if let Some(why) = forced_off(developer_data, handoff) {
            eprintln!("reports: off ({why})");
            return None;
        }
    }
    let mut r = Reports::load(&home);
    if test_on {
        if r.send != Some(true) || r.install_id.is_none() {
            r = store_choice(&home, true).map_err(|e| eprintln!("reports: off, the choice could not be stored: {e}")).ok()?;
        }
    } else if r.send.is_none() || r.consent_version.unwrap_or(0) < CONSENT_VERSION {
        let yes = ask();
        r = match store_choice(&home, yes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("reports: off, the answer could not be stored: {e}");
                return None;
            }
        };
        eprintln!("reports: the player chose {}", if yes { "to send reports" } else { "not to send reports" });
    }
    if r.send != Some(true) {
        eprintln!("reports: off (the player's choice; Settings > Gameplay > Reports)");
        return None;
    }
    let (Some(install_id), Some(delete_token)) = (r.install_id, r.delete_token) else {
        eprintln!("reports: off ([Reports] has no InstallId or DeleteToken)");
        return None;
    };
    // A test run sends only where it is told to, never to the project's server.
    let test_run = test_on && forced_off(developer_data, handoff).is_some();
    let endpoint = endpoint_env.clone().or(r.endpoint).unwrap_or_else(|| upload::DEFAULT_ENDPOINT.to_string());
    let may_send = !test_run || endpoint_env.is_some();
    eprintln!("reports: on, sending to {endpoint}{}", if may_send { "" } else { " (not in this test run: no NVF_REPORTS_ENDPOINT)" });
    Some(Active { install_id, delete_token, endpoint: endpoint.trim_end_matches('/').to_string(), may_send })
}

const ASK_TITLE: &str = "Help fix NVForever?";
/// The popup's words (player-reports.md §2.1; our own words).
const ASK_TEXT: &str = "NVForever can send a report after each session: the build, your PC's hardware and settings, \
    your mod list, how the game ran, the game's log and any problems it noticed. No personal details and no saves.\n\n\
    The reports are kept private and used only to find and fix bugs. You can change this at any time in \
    Settings > Gameplay > Reports.\n\nWhat is sent: https://github.com/LootJunkieAzza/NVForever/blob/main/docs/design/player-reports.md";

/// The popup; true: Send reports. Closing it or Esc is Not now.
#[cfg(windows)]
fn ask() -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let r = MessageDialog::new()
        .set_title(ASK_TITLE)
        .set_level(MessageLevel::Info)
        .set_description(ASK_TEXT)
        .set_buttons(MessageButtons::OkCancelCustom("Send reports".into(), "Not now".into()))
        .show();
    matches!(r, MessageDialogResult::Ok) || matches!(&r, MessageDialogResult::Custom(s) if s == "Send reports")
}

/// No dialogs off Windows yet: not asked, so off (§1.1: any run where the popup could not be shown).
#[cfg(not(windows))]
fn ask() -> bool {
    let _ = (ASK_TITLE, ASK_TEXT);
    false
}

/// The reports folders, created and checked; `None` (logged) when they cannot be used.
pub fn folders() -> Option<(PathBuf, PathBuf)> {
    let home = EngineHome::user()?;
    let root = home.reports();
    let (pending, sent) = (root.join("pending"), root.join("sent"));
    for d in [&pending, &sent] {
        if let Err(e) = fnv_install::guard_write(d, None).map_err(|e| e.to_string()).and_then(|()| std::fs::create_dir_all(d).map_err(|e| e.to_string())) {
            warn!("reports: {}: {e}", d.display());
            return None;
        }
    }
    Some((pending, sent))
}

// ------------------------------------------------------------------ the collector

/// The settings page's row (nvf-play's settings_layout names it).
pub const SETTING_KEY: &str = "send-reports";

/// Collects this session's summary while reports are on; adds the settings row always.
pub struct ReportsPlugin {
    /// The plugins loaded, in order.
    pub plugins: Vec<String>,
    pub data: PathBuf,
    pub mo2: bool,
    /// A load handed over from an earlier run: earlier sessions are left for the next start.
    pub handoff: bool,
}

#[derive(Resource)]
struct Collector {
    summary: Summary,
    file: PathBuf,
    started: Instant,
    last_write: Instant,
    places: std::collections::HashMap<String, usize>,
    cells_seen: std::collections::HashSet<u32>,
    last_space: Option<(String, Instant)>,
    space_time: std::collections::HashMap<String, f32>,
}

impl Plugin for ReportsPlugin {
    fn build(&self, app: &mut App) {
        nvf_play::settings::add_setting(
            app,
            nvf_play::settings::Setting {
                key: SETTING_KEY,
                group: "Reports",
                label: "Send reports to help fix NVForever",
                note: "After each session: build, hardware, settings, mod list, how it ran and the log; no personal details, no saves. From the next start",
                get: |_| EngineHome::user().is_some_and(|h| Reports::load(&h).send == Some(true)),
                set: |_, on| {
                    if let Some(h) = EngineHome::user() {
                        if Reports::load(&h).send != Some(on) {
                            match store_choice(&h, on) {
                                Ok(_) => info!("reports: turned {} in the settings (from the next start)", if on { "on" } else { "off" }),
                                Err(e) => warn!("reports: the choice could not be stored: {e}"),
                            }
                        }
                    }
                },
                slider: None,
            },
        );
        let Some(active) = active().cloned() else { return };
        let Some((pending, sent)) = folders() else { return };
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
        let _ = HIDDEN_FOLDERS.set(folders_to_hide(&self.data, exe_dir.as_deref()));
        if !self.handoff && active.may_send {
            upload::spawn(active.clone(), pending.clone(), sent, std::process::id());
        } else if !active.may_send {
            info!("reports: sending skipped in this test run (no NVF_REPORTS_ENDPOINT)");
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let file = pending.join(format!("session-{}-{}.json", crate::crash::stamp(now), std::process::id()));
        let summary = base_summary(&active, &self.plugins, &self.data, self.mo2, now);
        app.insert_resource(active)
            .insert_resource(Collector {
                summary,
                file,
                started: Instant::now(),
                last_write: Instant::now(),
                places: Default::default(),
                cells_seen: Default::default(),
                last_space: None,
                space_time: Default::default(),
            })
            .add_systems(Startup, (read_gpu, read_window))
            .add_systems(Update, collect)
            .add_systems(Last, write_summary);
    }
}

/// `YYYY-MM-DDTHH:MM:00Z` of a Unix time, rounded down to the minute (§3.1).
fn iso_minute(unix: u64) -> String {
    let (y, m, d) = crate::crash::civil_from_days((unix / 86_400) as i64);
    let s = unix % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:00Z", s / 3600, s / 60 % 60)
}

fn base_summary(active: &Active, plugins: &[String], data: &Path, mo2: bool, now: u64) -> Summary {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()).with_memory(MemoryRefreshKind::nothing().with_ram()));
    let settings = EngineHome::user()
        .and_then(|h| std::fs::read_to_string(h.config_file()).ok())
        .map(|t| settings_rows(&t))
        .unwrap_or_default();
    Summary {
        install_id: active.install_id.clone(),
        session_id: new_uuid(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: option_env!("NVF_GIT_COMMIT").map(str::to_string),
        profile: if cfg!(debug_assertions) { "dev" } else { "release" },
        dirty: None,
        start_utc: iso_minute(now),
        os: System::name(),
        os_build: System::os_version(),
        cpu: sys.cpus().first().map(|c| c.brand().trim().to_string()).filter(|s| !s.is_empty()),
        cores: Some(sys.cpus().len()).filter(|&n| n > 0),
        ram_gib: Some((sys.total_memory() as f64 / (1u64 << 30) as f64).round() as u64).filter(|&g| g > 0),
        settings,
        plugins: plugins.iter().map(|p| (p.clone(), fnv_install::child_ci(data, p).and_then(|f| std::fs::metadata(f).ok()).map(|m| m.len()), None)).collect(),
        mo2,
        ..Default::default()
    }
}

/// The settings the player changed: the `[Settings]` rows of the INI (the page writes only
/// what was changed) and `[Text] CodePage`; never `[Game] Path` or `[Reports]` (§3.1).
fn settings_rows(ini: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for line in ini.lines().map(str::trim) {
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s.trim().to_string();
        } else if let Some((k, v)) = line.split_once('=').filter(|_| !line.starts_with(';') && !line.starts_with('#')) {
            let keep = section.eq_ignore_ascii_case("Settings") || (section.eq_ignore_ascii_case("Text") && k.trim().eq_ignore_ascii_case("CodePage"));
            if keep && !k.trim().eq_ignore_ascii_case(SETTING_KEY) {
                out.push((format!("{section}.{}", k.trim()), v.trim().to_string()));
            }
        }
    }
    out
}

fn gpu_vendor(id: u32) -> &'static str {
    match id {
        0x10DE => "nvidia",
        0x1002 | 0x1022 => "amd",
        0x8086 => "intel",
        0x5143 => "qualcomm",
        0x106B => "apple",
        _ => "other",
    }
}

fn read_gpu(adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>, mut c: ResMut<Collector>) {
    if let Some(a) = adapter {
        c.summary.gpu_name = Some(a.name.clone());
        c.summary.gpu_vendor = Some(gpu_vendor(a.vendor).to_string());
        c.summary.gpu_driver = Some(format!("{} {}", a.driver, a.driver_info).trim().to_string()).filter(|s| !s.is_empty());
        c.summary.graphics_api = Some(format!("{:?}", a.backend));
    }
}

fn read_window(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut c: ResMut<Collector>) {
    if let Ok(w) = windows.single() {
        c.summary.display = Some((w.resolution.physical_width(), w.resolution.physical_height(), None, format!("{:?}", w.mode)));
    }
}

/// Every frame: its time in the camera's place, cells visited, time in each worldspace.
fn collect(
    time: Res<Time>,
    cam: Query<&GlobalTransform, With<crate::fly::FlyCam>>,
    streamer: Option<Res<crate::world::Streamer>>,
    world: Option<Res<crate::world::WorldData>>,
    mut c: ResMut<Collector>,
) {
    let (Some(streamer), Some(world), Ok(cam)) = (streamer, world, cam.single()) else { return };
    let g = crate::world::bevy_to_game(cam.translation());
    let (place, interior, cell) = match streamer.interior {
        Some(id) => {
            let c = world.index.cells.get(&id);
            (c.and_then(|c| c.edid.clone()).unwrap_or_else(|| format!("{id:08X}")), true, c)
        }
        None => {
            let ws = world.index.worldspaces.get(&world.world).and_then(|w| w.edid.clone()).unwrap_or_else(|| format!("{:08X}", world.world));
            (ws, false, world.grid_cell(fnv_world::cell_of(g.x, g.y)))
        }
    };
    let c = &mut *c;
    let t_s = c.started.elapsed().as_secs();
    if let Some(cell) = cell {
        if c.summary.cells.len() < summary::MAX_CELLS && c.cells_seen.insert(cell.form_id) {
            c.summary.cells.push((format!("{:08X}", cell.form_id), cell.edid.clone()));
        }
    }
    let cell_name = cell.and_then(|c| c.edid.clone()).unwrap_or_default();
    let i = *c.places.entry(place.clone()).or_insert_with(|| {
        c.summary.places.push(Place { name: place.clone(), interior, ..Default::default() });
        c.summary.places.len() - 1
    });
    // The first frames after a load are the load, not play.
    let ms = time.delta_secs() * 1000.0;
    if ms > 0.0 && t_s > 2 {
        c.summary.places[i].add_frame(ms, t_s, &cell_name);
    }
    if !interior {
        *c.space_time.entry(place.clone()).or_default() += time.delta_secs();
    }
    if c.last_space.as_ref().is_none_or(|(p, _)| *p != place) {
        c.last_space = Some((place, Instant::now()));
    }
}

/// Rewrite the summary every [`WRITE_EVERY`], and at exit as a clean end.
fn write_summary(mut exit: MessageReader<AppExit>, script: Option<Res<nvf_play::script::ScriptState>>, mut c: ResMut<Collector>) {
    let exiting = exit.read().next().is_some();
    if !exiting && c.last_write.elapsed() < WRITE_EVERY {
        return;
    }
    let c = &mut *c;
    c.last_write = Instant::now();
    c.summary.duration_s = c.started.elapsed().as_secs() / 60 * 60;
    c.summary.end = if exiting { End::Clean } else { End::Unknown };
    c.summary.worldspace_time_s = c.space_time.iter().map(|(k, v)| (k.clone(), *v as u64)).collect();
    c.summary.warnings = warnings::groups();
    if let Some(rt) = script.as_ref().and_then(|s| s.rt.as_ref()) {
        let g = &rt.game;
        let mut stubs: std::collections::BTreeMap<String, u64> = Default::default();
        for (op, n) in g.stubbed.iter().chain(g.unhandled.iter()) {
            let name = g.data.table.get(*op).map(|f| f.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("0x{op:04X}"));
            *stubs.entry(name).or_default() += n;
        }
        c.summary.script_stubs = stubs.into_iter().collect();
    }
    let bytes = c.summary.to_bytes(redact);
    match fnv_install::guard_write(&c.file, None).map_err(|e| e.to_string()).and_then(|()| std::fs::write(&c.file, &bytes).map_err(|e| e.to_string())) {
        Ok(()) if exiting => info!("reports: session summary written to {}", c.file.display()),
        Ok(()) => debug!("reports: session summary written to {}", c.file.display()),
        Err(e) => warn!("reports: the session summary could not be written to {}: {e}", c.file.display()),
    }
}

// ------------------------------------------------------------------ earlier sessions

/// Finish an earlier session's upload before it is sent: how it ended (a summary still
/// saying `unknown` was cut short: `panic` when crash.rs wrote a crash report for its
/// process, else `native`), its crash report and its run log beside it. Returns the
/// upload, or `None` when the summary cannot be read.
pub fn finish_earlier(summary: &Path, logs: &Path) -> Option<upload::Pending> {
    let name = summary.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json")?;
    let pid = stem.rsplit('-').next()?;
    let dir = summary.parent()?;
    let names: Vec<String> = std::fs::read_dir(logs).map(|r| r.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default();
    let find = |prefix: &str, ext: &str| names.iter().filter(|n| n.starts_with(prefix) && n.ends_with(&format!("-{pid}{ext}"))).max().map(|n| logs.join(n));
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(summary).ok()?).ok()?;
    let crash = find("crash-", ".txt");
    // A crash report makes it a panic even when the game went on and exited normally (a
    // panic on a thread of its own does not stop the game, crash.rs).
    let end = match (&crash, v["end"].as_str()) {
        (Some(_), _) => Some("panic"),
        (None, Some("unknown")) => Some("native"),
        _ => None,
    };
    if let Some(end) = end.filter(|e| v["end"] != *e) {
        v["end"] = serde_json::Value::from(end);
        std::fs::write(summary, v.to_string()).ok()?;
    }
    let panic_part = dir.join(format!("{stem}.panic.txt"));
    if let Some(c) = crash.filter(|_| v["end"] == "panic" && !panic_part.exists()) {
        if let Ok(text) = std::fs::read_to_string(c) {
            let _ = std::fs::write(&panic_part, redact(&text));
        }
    }
    let log_part = dir.join(format!("{stem}.log.gz"));
    if !log_part.exists() {
        if let Some(text) = find("nvforever-", ".log").and_then(|l| std::fs::read_to_string(l).ok()) {
            let _ = std::fs::write(&log_part, upload::gzip(cut_log(&redact(&text)).as_bytes()));
        }
    }
    let names: Vec<String> = std::fs::read_dir(dir).map(|r| r.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default();
    upload::pending_in(dir, &names).into_iter().find(|p| p.summary == summary)
}

/// A run log within the cap (§3.1): whole when small, else its first and last lines.
fn cut_log(text: &str) -> std::borrow::Cow<'_, str> {
    // Logs compress about 10:1; the cap is on the compressed size.
    if text.len() <= LOG_CAP_BYTES * 8 {
        return std::borrow::Cow::Borrowed(text);
    }
    let lines: Vec<&str> = text.lines().collect();
    let head = &lines[..LOG_HEAD_LINES.min(lines.len())];
    let tail = &lines[lines.len().saturating_sub(LOG_TAIL_LINES).max(head.len())..];
    std::borrow::Cow::Owned(format!("{}\n[nvforever: {} lines left out]\n{}\n", head.join("\n"), lines.len() - head.len() - tail.len(), tail.join("\n")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_hide_the_game_data_and_exe_folders() {
        let f = super::folders_to_hide(std::path::Path::new(r"D:\Games\Alex Smith\Fallout New Vegas\Data"), Some(std::path::Path::new(r"D:\Tools\NVForever")));
        let r = |t: &str| super::redact_with(t, &f);
        assert_eq!(r(r"loaded D:\Games\Alex Smith\Fallout New Vegas\Data\FalloutNV.esm"), r"loaded <Data>\FalloutNV.esm");
        assert_eq!(r("ini at d:/games/alex smith/fallout new vegas/Fallout.ini"), "ini at <game>/Fallout.ini");
        assert_eq!(r(r"exe D:\Tools\NVForever\nvforever.exe"), r"exe <NVForever>\nvforever.exe");
        assert_eq!(r("nothing here"), "nothing here");
        // A drive is never hidden whole.
        assert!(super::folders_to_hide(std::path::Path::new(r"C:\Data"), None).iter().all(|(p, _)| p.len() > 3));
    }

    use super::*;

    #[test]
    fn only_changed_settings_are_reported_never_paths() {
        let ini = "[Game]\nPath=C:\\Games\\FNV\n[Settings]\nmenu-pause=1\nsend-reports=1\n; x=1\n[Text]\nCodePage=1251\n[Reports]\nInstallId=abc\nEndpoint=https://x\n";
        assert_eq!(settings_rows(ini), vec![("Settings.menu-pause".to_string(), "1".to_string()), ("Text.CodePage".to_string(), "1251".to_string())]);
    }

    #[test]
    fn minute_times_and_vendors() {
        assert_eq!(iso_minute(1_791_793_805), "2026-10-12T08:30:00Z");
        assert_eq!(gpu_vendor(0x10DE), "nvidia");
        assert_eq!(gpu_vendor(0x1234), "other");
    }

    #[test]
    fn a_long_log_keeps_its_head_and_tail() {
        let text: String = (0..200_000).map(|i| format!("line {i} {}\n", "x".repeat(100))).collect();
        let cut = cut_log(&text);
        assert!(cut.starts_with("line 0 ") && cut.contains("line 199999 ") && cut.contains("lines left out"));
        assert_eq!(cut_log("short\n"), "short\n");
    }

    #[test]
    fn an_earlier_session_cut_short_is_marked_and_given_its_files() {
        let root = std::env::temp_dir().join(format!("nvf-reports-test-{}", std::process::id()));
        let (pending, logs) = (root.join("pending"), root.join("logs"));
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let s = pending.join("session-20261110-080000-4242.json");
        std::fs::write(&s, r#"{"schema":"nvf-session/1","end":"unknown"}"#).unwrap();
        std::fs::write(logs.join("nvforever-20261110-080000-4242.log"), "INFO a\n").unwrap();
        std::fs::write(logs.join("crash-20261110-081500-4242.txt"), "message: boom\n").unwrap();
        let p = finish_earlier(&s, &logs).unwrap();
        assert_eq!(upload::end_of(&std::fs::read(&s).unwrap()).as_deref(), Some("panic"));
        assert!(p.log.is_some() && p.panic.is_some());
        // Without a crash report: a native crash.
        let s2 = pending.join("session-20261110-090000-77.json");
        std::fs::write(&s2, r#"{"end":"unknown"}"#).unwrap();
        let p2 = finish_earlier(&s2, &logs).unwrap();
        assert_eq!(upload::end_of(&std::fs::read(&s2).unwrap()).as_deref(), Some("native"));
        assert!(p2.panic.is_none() && p2.log.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
