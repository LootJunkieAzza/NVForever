//! The launcher's window: Home (news), Settings, Mods and Game folder pages beside a bar
//! that starts the game.

use crate::catalog::{self, Catalog, Kind, Row};
use crate::launch::{self, Start};
use crate::news::{self, Post, Source};
use crate::theme::{self, BAD, CARD, GOLD, GOOD, LINE, WEAK};
use crate::update::{self, Msg, Phase};
use nvf_settings_layout::menu;
use eframe::egui::{self, load::Bytes, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use fnv_install::{guard_write, ini_get, ini_pairs, ini_set, plugins, write_whole, Config, EngineHome, GameInstall};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

const LOGO: &[u8] = include_bytes!("../assets/logo-256.png");
/// The launcher's own keys in the home's INI.
const LAUNCHER: &str = "Launcher";
/// How much of a long post shows before "Read more".
const PREVIEW_CHARS: usize = 700;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Settings,
    Mods,
    Game,
}

enum NewsState {
    Off,
    Loading,
    Shown(Source),
    Failed(String),
}

type NewsResult = Result<(Vec<Post>, Source), String>;
type NextResult = Result<Option<news::NextRelease>, String>;
type LatestResult = Result<Option<news::Latest>, String>;

/// Where the updater is (docs/design/launcher.md "Updater").
enum Upd {
    Idle,
    Working { phase: Phase, done: u64, total: u64 },
    /// Downloaded and checked; installs as soon as the game is not running.
    Ready(update::Ready),
    Installing,
    Failed(String),
}

/// The launcher's version: its build's, or `NVF_LAUNCHER_VERSION` (a test aid: a launcher
/// that takes itself for an older version offers the newest release as an update, so the
/// updater can be tried against a real release; docs/design/launcher.md).
fn version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        std::env::var("NVF_LAUNCHER_VERSION").ok().filter(|v| update::test_aids() && update::safe_component(v)).unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
    })
}

/// The message line while the game starts, cleared when it ends without an error.
const STARTING: &str = "The game is starting…";
/// On a start with no game folder set (#2938): at the launcher's start, and on Play.
const NEEDS_FOLDER: &str = "Set your Fallout: New Vegas folder here, then Save: the game needs it to start.";
const NEEDS_FOLDER_FIRST: &str = "Set your Fallout: New Vegas folder first (here, then Save): the game cannot start without it.";

#[derive(Clone)]
struct NewestSave {
    path: PathBuf,
    name: String,
    detail: String,
    shot: Option<Arc<[u8]>>,
}

pub struct App {
    page: Page,
    home: Option<EngineHome>,
    ini_text: String,
    message: Option<(String, bool)>,
    // Settings.
    /// The game's settings menu (src/catalog.rs): its rows, its tabs' names and the words shown.
    rows: Vec<Row>,
    tabs: Vec<String>,
    tab_labels: Vec<String>,
    tab: usize,
    words: HashMap<&'static str, &'static [&'static str]>,
    lists: HashMap<&'static str, &'static [&'static str]>,
    stored: BTreeMap<String, String>,
    edits: BTreeMap<String, String>,
    // Game folder.
    game_edit: String,
    /// The Mods page's new-game start fields (src/start.rs), as typed.
    start_edit: crate::start::Fields,
    // Mods.
    mods: Option<plugins::PluginList>,
    mods_dirty: bool,
    /// The row being dragged on the Mods page.
    mods_drag: Option<usize>,
    /// Maximum load-order backups kept. This is the cap, not how many exist.
    backup_max: u32,
    /// The backup the Load backup button puts into the list (`plugins-<this>.txt`).
    backup_pick: Option<String>,
    /// Mod folders on the left of the Mods page. A lower row overwrites on Apply.
    mod_rows: Vec<ModRow>,
    /// The mod-folder row being dragged.
    mod_drag: Option<usize>,
    /// Plugin names found when the Mods page was opened or refreshed.
    mods_present: HashSet<String>,
    /// Lowercase plugin name, then the enabled mod folders that have it, top first.
    plugin_folders: HashMap<String, Vec<String>>,
    /// Lowercase plugin name, then the master names in the file that wins.
    plugin_masters: HashMap<String, Vec<String>>,
    /// Lowercase plugin name, then the folder it belongs to. A lower row wins.
    plugin_home: HashMap<String, String>,
    /// How many plugin files are in each mod folder.
    folder_counts: HashMap<String, usize>,
    /// Files in each mod folder, relative to its Data folder when it has one.
    folder_files: HashMap<String, Vec<String>>,
    /// Which folders overwrite this one, and which it overwrites. A lower row wins.
    clashes: HashMap<String, Vec<Clash>>,
    /// The find box on the plugin list.
    plugin_find: String,
    /// The find box on the mod list.
    mod_find: String,
    /// Lowercase plugin name last clicked, so its folder is marked on the left.
    picked_plugin: Option<String>,
    /// The left list should scroll to that folder once.
    mod_jump: bool,
    /// The colored file-replacement lines and their key are open.
    show_overwrites: bool,
    /// How many older copies of the plugin list to keep.
    backup_keep: usize,
    /// Apply is asking before it saves.
    apply_ask: bool,
    /// Path of a folder or archive dropped on Add. A path is not typed.
    add_path: String,
    /// A profile folder dropped on the Profiles bar. A path is not typed.
    profile_path: String,
    /// Profile name, then the folder that holds its lists.
    profile_choices: Vec<(String, PathBuf)>,
    /// The profile chosen in the list.
    profile_pick: Option<String>,
    /// The Profiles box, so a drop there is not installed as a mod.
    profile_rect: egui::Rect,
    /// The Add row on the left, so a drop there sets a path instead of installing.
    mod_add_rect: egui::Rect,
    /// An open archive installer. Nothing else is added until it is finished.
    wizard: Option<crate::fomod::Wizard>,
    /// Bottom bar height. Saved when the user lets go of the grip.
    bar_h: f32,
    /// The displays, for the Resolution and Monitor lists (asked once at start).
    monitors: Vec<crate::display::Monitor>,
    /// The Language list (the language files found at start).
    languages: Vec<crate::display::Choice>,
    /// "Delete them now": asking first, then the request's answer.
    reports_confirm: bool,
    /// DLSS: what this PC has (read at start and after a download), and the licence open.
    dlss: crate::dlss::Status,
    dlss_licence: bool,
    reports_rx: Option<Receiver<Result<usize, String>>>,
    // News.
    news: Vec<(Post, Option<Arc<[u8]>>)>,
    news_state: NewsState,
    news_rx: Option<Receiver<NewsResult>>,
    next_rx: Option<Receiver<NextResult>>,
    next: Option<news::NextRelease>,
    latest_rx: Option<Receiver<LatestResult>>,
    /// A release newer than this launcher, when GitHub has one.
    latest: Option<news::Latest>,
    expanded: HashSet<usize>,
    md: CommonMarkCache,
    // The updater.
    upd: Upd,
    upd_rx: Option<Receiver<Msg>>,
    upd_cancel: Arc<AtomicBool>,
    upd_open: bool,
    // Starting the game.
    newest: Option<NewestSave>,
    /// Saves in the home, newest first. Continue loads `newest`.
    saves: Vec<NewestSave>,
    exe: Option<PathBuf>,
    child: Option<Child>,
    /// Test aid: `NVF_LAUNCHER_SHOT=<file.png>[;page]` saves the window once the news has
    /// settled and closes (`NVF_LAUNCHER_PAGE`: home, settings, mods, game).
    shot: Option<(PathBuf, u32)>,
}

struct ModRow {
    name: String,
    on: bool,
}

struct Clash {
    other: String,
    files: Vec<String>,
    wins: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        crate::fonts::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        // Test aid: NVF_LAUNCHER_ZOOM=0.6 draws everything smaller (a whole page in one shot).
        if let Some(z) = std::env::var("NVF_LAUNCHER_ZOOM").ok().and_then(|z| z.parse::<f32>().ok()).filter(|z| (0.3..=3.0).contains(z)) {
            cc.egui_ctx.set_zoom_factor(z);
        }
        let home = EngineHome::user();
        let menu = Catalog::load(home.as_ref().map(EngineHome::config_file).as_deref());
        let backup_max = read_backup_max(home.as_ref());
        let mut app = App {
            page: Page::Home,
            home,
            ini_text: String::new(),
            message: None,
            tabs: menu.tab_names(),
            tab_labels: menu.tabs.iter().map(|(_, l)| l.clone()).collect(),
            rows: menu.rows,
            tab: 0,
            words: catalog::step_words(),
            lists: catalog::list_choices(),
            stored: BTreeMap::new(),
            edits: BTreeMap::new(),
            game_edit: String::new(),
            start_edit: crate::start::Fields::default(),
            mods: None,
            mods_dirty: false,
            mods_drag: None,
            backup_max,
            backup_pick: None,
            mod_rows: Vec::new(),
            mod_drag: None,
            mods_present: HashSet::new(),
            plugin_folders: HashMap::new(),
            plugin_masters: HashMap::new(),
            plugin_home: HashMap::new(),
            folder_counts: HashMap::new(),
            folder_files: HashMap::new(),
            clashes: HashMap::new(),
            plugin_find: String::new(),
            mod_find: String::new(),
            picked_plugin: None,
            mod_jump: false,
            show_overwrites: false,
            backup_keep: 5,
            apply_ask: false,
            add_path: String::new(),
            profile_path: String::new(),
            profile_choices: Vec::new(),
            profile_pick: None,
            profile_rect: egui::Rect::NOTHING,
            mod_add_rect: egui::Rect::NOTHING,
            wizard: None,
            bar_h: 86.0,
            monitors: crate::display::monitors(),
            languages: crate::display::language_choices(&fnv_util::words::dirs(
                std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())).as_deref(),
                EngineHome::user().map(|h| h.root).as_deref(),
            )),
            reports_confirm: false,
            dlss: crate::dlss::Status::read(None),
            dlss_licence: false,
            reports_rx: None,
            news: Vec::new(),
            news_state: NewsState::Off,
            news_rx: None,
            next_rx: None,
            next: None,
            latest_rx: None,
            latest: None,
            expanded: HashSet::new(),
            md: CommonMarkCache::default(),
            upd: Upd::Idle,
            upd_rx: None,
            upd_cancel: Arc::new(AtomicBool::new(false)),
            upd_open: false,
            newest: None,
            saves: Vec::new(),
            exe: None,
            child: None,
            shot: std::env::var_os("NVF_LAUNCHER_SHOT").map(|p| (PathBuf::from(p), 0)),
        };
        app.page = match std::env::var("NVF_LAUNCHER_PAGE").as_deref() {
            Ok("settings") => Page::Settings,
            Ok("mods") => Page::Mods,
            Ok("game") => Page::Game,
            Ok("update") => {
                app.upd_open = true;
                Page::Home
            }
            // The "Delete them now" question, open (nothing is sent without its button).
            Ok("delete-reports") => {
                app.reports_confirm = true;
                Page::Settings
            }
            // NVIDIA's licence before Download DLSS, open (nothing is fetched without I accept).
            Ok("dlss-licence") => {
                app.dlss_licence = true;
                app.tab = app.tabs.iter().position(|t| t == "Graphics").unwrap_or(0);
                Page::Settings
            }
            // `settings:<tab>`: that tab of the Settings page.
            Ok(p) if p.starts_with("settings:") => {
                let tab = &p["settings:".len()..];
                app.tab = app.tabs.iter().position(|t| t.eq_ignore_ascii_case(tab)).unwrap_or(0);
                Page::Settings
            }
            _ => Page::Home,
        };
        // The INI first: the mod list's sync needs its game folder.
        app.reload_ini();
        app.bar_h = ini_get(&app.ini_text, LAUNCHER, "BottomBar").and_then(|s| s.parse::<f32>().ok()).unwrap_or(86.0).clamp(28.0, 280.0);
        app.backup_keep = ini_get(&app.ini_text, LAUNCHER, "PluginBackups").and_then(|s| s.parse().ok()).unwrap_or(5).clamp(0, 30);
        if app.page == Page::Mods {
            app.load_mods();
        }
        // No game folder yet (a first start): open on its page (#2938), unless a test aid
        // chose the page.
        if !app.folder_known() && std::env::var_os("NVF_LAUNCHER_PAGE").is_none() {
            app.page = Page::Game;
            app.message = Some((NEEDS_FOLDER.into(), false));
        }
        app.find_newest_save();
        if let Some(h) = &app.home {
            app.news = news::load_cache(&h.cache()).into_iter().map(|p| (p, None)).collect();
            if !app.news.is_empty() {
                app.news_state = NewsState::Shown(Source::Cache);
            }
        }
        if app.launcher_flag("News", true) {
            app.fetch_news(&cc.egui_ctx);
        }
        // Every write of the launcher's (the INI, the updater's swap) is refused inside the game
        // folder the INI names (fnv_install::guard_write), even where no GameInstall is passed.
        if let Some(g) = app.game() {
            fnv_install::remember_game(&g);
        }
        // A swap cut short in its renames (the window closed, the power lost) is finished
        // first; a swap cut short before them leaves only files to tidy away.
        // The folder the launcher runs from, whether or not nvforever is still beside it.
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
            match update::resume_interrupted(&dir, app.game().as_ref()) {
                Some(Ok(v)) => app.message = Some((format!("An update to v{v} was interrupted; it has been finished."), false)),
                Some(Err(e)) => app.message = Some((format!("An interrupted update could not be finished: {e}. Download the release from its page."), true)),
                None => {}
            }
        }
        // Started by the updater (update::restart): say what changed.
        for a in std::env::args().skip(1) {
            match a.as_str() {
                "--updated" | "--rolled-back" => {
                    // The version the swap wrote (an archive without a launcher leaves this one).
                    let installed = install_dir(app.game().as_ref()).ok().and_then(|d| update::read_manifest(&d)).map_or_else(|| version().to_string(), |m| m.version);
                    app.message = Some(if a == "--updated" {
                        (format!("Updated to v{installed}. Your saves, settings and mods are as they were."), false)
                    } else {
                        (format!("Rolled back to v{installed}."), false)
                    });
                }
                _ => {}
            }
        }
        // Downloads of releases no newer than this one are done with.
        if let Some(h) = &app.home {
            let cache = h.cache();
            std::thread::spawn(move || {
                for d in update::stale_staging(&cache, version()) {
                    let _ = std::fs::remove_dir_all(d);
                }
            });
        }
        app
    }

    // ---- The INI ----

    fn ini_path(&self) -> Option<PathBuf> {
        self.home.as_ref().map(EngineHome::config_file)
    }

    fn reload_ini(&mut self) {
        self.ini_text = self.ini_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        self.stored = ini_pairs(&self.ini_text, "Settings").into_iter().collect();
        self.edits.clear();
        self.start_edit = crate::start::Fields::read(&self.ini_text);
        // The game writes its menu at each start (src/catalog.rs): the page follows it, on the
        // same tab.
        let menu = Catalog::load(self.ini_path().as_deref());
        let on = self.tabs.get(self.tab).cloned();
        self.tabs = menu.tab_names();
        self.tab_labels = menu.tabs.iter().map(|(_, l)| l.clone()).collect();
        self.rows = menu.rows;
        self.tab = on.and_then(|on| self.tabs.iter().position(|t| *t == on)).unwrap_or(0);
        self.game_edit = Config::from_ini(&self.ini_text).game_dir.map(|p| p.display().to_string()).unwrap_or_default();
        // None set: the box starts on Steam's usual folder when the game is there; it is
        // stored only by Save (#2938).
        if self.game_edit.is_empty() {
            let usual = launch::usual_game_folders(launch::user_home().as_deref(), cfg!(windows));
            if let Some(f) = launch::first_usable(&usual, |p| GameInstall::open(p).is_ok()) {
                self.game_edit = f.display().to_string();
            }
        }
        let configured = ini_get(&self.ini_text, LAUNCHER, "Exe").filter(|s| !s.is_empty()).map(PathBuf::from);
        self.exe = launch::find_game(configured.as_deref());
        // The DLSS folder is beside the game this INI starts (crate::dlss::folder_for).
        self.dlss = crate::dlss::Status::read(self.exe.as_deref());
    }

    fn launcher_flag(&self, key: &str, default: bool) -> bool {
        ini_get(&self.ini_text, LAUNCHER, key).map_or(default, |v| v != "0")
    }

    fn game(&self) -> Option<GameInstall> {
        Config::from_ini(&self.ini_text).game_dir.and_then(|d| GameInstall::open(d).ok())
    }

    /// Whether the game would find its folder when started now ([`launch::folder_known`]).
    fn folder_known(&self) -> bool {
        launch::folder_known(self.game().is_some(), fnv_install::mo2::usvfs_loaded(), std::env::var_os("NVFOREVER_DATA").as_deref())
    }

    /// Write `edit` over the INI as it is on disk now (the game may have written it since it
    /// was read), keeping every other line.
    fn write_ini(&mut self, edit: impl FnOnce(String) -> String) -> Result<(), String> {
        let path = self.ini_path().ok_or("no home folder (Documents) found")?;
        let now = std::fs::read_to_string(&path).unwrap_or_default();
        let text = edit(now);
        guard_write(&path, self.game().as_ref()).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        write_whole(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn set_launcher_key(&mut self, key: &str, value: &str) {
        let r = self.write_ini(|t| ini_set(&t, LAUNCHER, key, value));
        self.after_write(r, None);
    }

    fn after_write(&mut self, r: Result<(), String>, done: Option<&str>) {
        let edits = std::mem::take(&mut self.edits);
        match r {
            Ok(()) => {
                self.reload_ini();
                if let Some(d) = done {
                    self.message = Some((d.to_string(), false));
                }
            }
            Err(e) => {
                self.edits = edits;
                self.message = Some((e, true));
            }
        }
    }

    // ---- Saves and news ----

    fn find_newest_save(&mut self) {
        let Some(h) = &self.home else { return };
        let keep = self.newest.as_ref().map(|s| s.path.clone());
        self.saves = fnv_save::list(&h.saves()).into_iter().map(|(path, hd)| {
            let shot = fnv_save::read_header(&path).ok().map(|s| s.shot).filter(|s| !s.is_empty()).map(Arc::from);
            let name = if hd.player_name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { hd.player_name.clone() };
            let hours = hd.play_seconds / 3600.0;
            let mut parts = vec![format!("Level {}", hd.level)];
            parts.extend(shown_place(&hd.location).map(str::to_string));
            parts.push(format!("{hours:.0} h played"));
            let detail = parts.join(" · ");
            NewestSave { path, name, detail, shot }
        }).collect();
        self.newest = keep.and_then(|p| self.saves.iter().find(|s| s.path == p).cloned()).or_else(|| self.saves.first().cloned());
    }

    fn fetch_news(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(news::fetch());
            c.request_repaint();
        });
        self.news_rx = Some(rx);
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(news::fetch_next_release());
            c.request_repaint();
        });
        self.next_rx = Some(rx);
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(news::fetch_latest());
            c.request_repaint();
        });
        self.latest_rx = Some(rx);
        self.news_state = NewsState::Loading;
    }

    fn poll_news(&mut self) {
        if let Some(r) = self.next_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.next_rx = None;
            match r {
                Ok(n) => self.next = n,
                Err(e) => eprintln!("launcher: next release: {e}"),
            }
        }
        if let Some(r) = self.latest_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.latest_rx = None;
            match r {
                Ok(l) => self.latest = l.filter(|l| news::is_newer(&l.version, version())),
                Err(e) => eprintln!("launcher: latest release: {e}"),
            }
        }
        let Some(rx) = &self.news_rx else { return };
        let Ok(r) = rx.try_recv() else { return };
        self.news_rx = None;
        match r {
            Ok((posts, source)) => {
                if let Some(h) = &self.home {
                    news::save_cache(&h.cache(), &posts);
                }
                self.news = posts
                    .into_iter()
                    .map(|mut p| {
                        let img = p.image_bytes.take().map(Arc::from);
                        (p, img)
                    })
                    .collect();
                self.expanded.clear();
                self.news_state = NewsState::Shown(source);
            }
            Err(e) => self.news_state = NewsState::Failed(e),
        }
    }

    // ---- Starting ----

    fn running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                ended => {
                    self.child = None;
                    self.reload_ini();
                    self.find_newest_save();
                    // An error is said, with where the logs are; a clean exit only clears
                    // "The game is starting…" (#2938).
                    let said = ended.ok().flatten().and_then(|s| launch::exit_message(s.code(), s.success(), self.home.as_ref().map(EngineHome::logs).as_deref()));
                    match said {
                        Some(m) => self.message = Some((m, true)),
                        None if self.message.as_ref().is_some_and(|(m, _)| m == STARTING) => self.message = None,
                        None => {}
                    }
                    false
                }
            },
            None => false,
        }
    }

    fn start(&mut self, ctx: &egui::Context, start: Start) {
        if matches!(self.upd, Upd::Installing) {
            self.message = Some(("The update is being installed: play once it is done.".into(), true));
            return;
        }
        if !self.edits.is_empty() {
            let r = self.save_settings();
            if r.is_err() {
                self.after_write(r, None);
                return;
            }
            self.after_write(r, None);
        }
        // No game folder: the game would only ask for one (Windows) or quit (elsewhere);
        // the launcher's own page asks instead (#2938).
        if !self.folder_known() {
            self.page = Page::Game;
            self.message = Some((NEEDS_FOLDER_FIRST.into(), true));
            return;
        }
        let Some(exe) = self.exe.clone() else {
            self.message = Some((format!("{} was not found beside the launcher: pick it on the Game folder page.", launch::GAME_EXE), true));
            return;
        };
        match launch::spawn(&exe, &start) {
            Ok(c) => {
                self.child = Some(c);
                self.message = Some((STARTING.into(), false));
                if self.launcher_flag("CloseOnPlay", false) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(e) => self.message = Some((format!("{}: {e}", exe.display()), true)),
        }
    }

    fn save_settings(&mut self) -> Result<(), String> {
        let edits = self.edits.clone();
        self.write_ini(|mut t| {
            for (k, v) in &edits {
                t = ini_set(&t, "Settings", k, v);
            }
            t
        })
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        self.poll_news();
        let running = self.running();
        self.poll_update(ctx, running);
        if matches!(self.upd, Upd::Installing) && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.message = Some(("Installing the update: the launcher closes by itself when it is done.".into(), false));
        }
        if running {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        self.top_bar(ui);
        self.bottom_bar(ui, ctx, running);
        self.nav(ui);
        self.test_shot(ctx);
        self.update_window(ctx, running);
        self.reports_window(ctx, running);
        self.dlss_licence_window(ctx);
        self.installer_window(ctx);
        self.take_dropped(ctx);
        egui::CentralPanel::default().frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(24, 16))).show(ui, |ui| match self.page {
            Page::Home => self.home_page(ui),
            Page::Settings => self.settings_page(ui),
            Page::Mods => self.mods_page(ui),
            Page::Game => self.game_page(ui),
        });
        // Last, after everything that can draw a link (a post's Markdown is in the central
        // panel): egui's own link opening would skip the https rule.
        for url in take_links(ctx) {
            launch::open_url(&url);
        }
    }
}

/// The pages egui was asked to open this frame (links in a post's text), taken out of its
/// output so that only [`launch::open_url`] opens them.
fn take_links(ctx: &egui::Context) -> Vec<String> {
    ctx.output_mut(|o| {
        let mut urls = Vec::new();
        o.commands.retain(|c| match c {
            egui::OutputCommand::OpenUrl(u) => {
                urls.push(u.url.clone());
                false
            }
            _ => true,
        });
        urls
    })
}

impl App {
    fn test_shot(&mut self, ctx: &egui::Context) {
        let Some((path, frames)) = &mut self.shot else { return };
        if self.news_rx.is_none() {
            *frames += 1;
        }
        if *frames == 20 {
            let (path, ctx2) = (path.clone(), ctx.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let [w, h] = img.size;
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => println!("launcher: screenshot saved to {}", path.display()),
                    Err(e) => eprintln!("launcher: screenshot {}: {e}", path.display()),
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx2.request_repaint();
            })));
        }
        ctx.request_repaint();
    }
}

// ---- The frame around the pages ----

impl App {
    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top").frame(Frame::new().fill(theme::PANEL).inner_margin(Margin::symmetric(16, 8)).stroke(Stroke::new(1.0, LINE))).show(root, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::from_bytes("bytes://nvf-logo.png", LOGO).fit_to_exact_size(Vec2::splat(44.0)));
                ui.vertical(|ui| {
                    ui.add_space(2.0);
                    ui.label(RichText::new("NVForever").font(crate::fonts::title(22.0)).color(GOLD));
                    ui.label(RichText::new("Launcher").small().color(WEAK));
                });
                self.countdown(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(h) = &self.home {
                        if ui.small_button("Open folder").on_hover_text("Open NVForever's folder (settings, saves, logs)").clicked() {
                            launch::open_folder(&h.root);
                        }
                    }
                    if let Some(l) = &self.latest {
                        let b = egui::Button::new(RichText::new(format!("Update to v{}", l.version)).strong().color(theme::ON_GOLD)).fill(GOLD);
                        if ui.add(b).on_hover_text(format!("v{} is out (this is v{})", l.version, version())).clicked() {
                            self.upd_open = true;
                        }
                    }
                });
            });
        });
    }

    /// The site's next-release countdown in the middle of the top bar, as the site's own bar
    /// shows it; nothing when the site announces none or the moment has passed.
    fn countdown(&self, ui: &mut egui::Ui) {
        let Some(n) = &self.next else { return };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let (lead, time) = match n.at {
            Some(at) if at > now => (format!("NVForever {} releases in", n.version), Some(news::countdown_text(at - now))),
            Some(_) => return,
            None => (format!("NVForever {} is coming soon", n.version), None),
        };
        // The middle of the whole bar, not of what the row has left after the logo.
        let rect = ui.clip_rect();
        let p = ui.painter();
        let lead_g = p.layout_no_wrap(lead, egui::FontId::proportional(15.0), WEAK);
        let time_g = time.map(|t| p.layout_no_wrap(t, egui::FontId::proportional(18.0), GOLD));
        let w = lead_g.size().x + time_g.as_ref().map_or(0.0, |g| g.size().x + 12.0) + 40.0;
        let pill = egui::Rect::from_center_size(rect.center(), Vec2::new(w, 34.0));
        p.rect(pill, CornerRadius::same(17), theme::SELECTED, Stroke::new(1.0, theme::GOLD_DIM), egui::StrokeKind::Inside);
        let lead_w = lead_g.size().x;
        p.galley(egui::pos2(pill.left() + 16.0, pill.center().y - lead_g.size().y / 2.0), lead_g, WEAK);
        if let Some(g) = time_g {
            p.galley(egui::pos2(pill.left() + 28.0 + lead_w, pill.center().y - g.size().y / 2.0), g, GOLD);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }

    fn nav(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("nav").exact_size(210.0).resizable(false).frame(Frame::new().fill(theme::PANEL).inner_margin(Margin::same(12))).show(root, |ui| {
            ui.add_space(4.0);
            for (page, name) in [(Page::Home, "HOME"), (Page::Settings, "SETTINGS"), (Page::Mods, "MODS"), (Page::Game, "GAME FOLDER")] {
                if nav_button(ui, name, self.page == page).clicked() {
                    self.page = page;
                    self.message = None;
                    if page == Page::Mods {
                        self.load_mods();
                    }
                }
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            if nav_button(ui, "WEBSITE", false).on_hover_text(news::SITE).clicked() {
                launch::open_url(news::SITE);
            }
            if nav_button(ui, "GITHUB", false).on_hover_text(REPO).clicked() {
                launch::open_url(REPO);
            }
        });
    }

    fn bottom_bar(&mut self, root: &mut egui::Ui, ctx: &egui::Context, running: bool) {
        let pad_y: i8 = if self.bar_h < 52.0 { 1 } else { 6 };
        egui::Panel::bottom("bottom").exact_size(self.bar_h.clamp(28.0, 280.0)).resizable(false).frame(Frame::new().fill(theme::PANEL).inner_margin(Margin::symmetric(12, pad_y)).stroke(Stroke::new(1.0, LINE))).show(root, |ui| {
            let (grip, grip_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::drag());
            if grip_resp.hovered() || grip_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            let mid = grip.center().x;
            let grip_color = if grip_resp.hovered() || grip_resp.dragged() { GOLD } else { WEAK };
            ui.painter().line_segment([egui::pos2(mid - 22.0, grip.center().y), egui::pos2(mid + 22.0, grip.center().y)], Stroke::new(3.0, grip_color));
            if grip_resp.dragged() {
                self.bar_h = (self.bar_h - grip_resp.drag_delta().y).clamp(28.0, 280.0);
            }
            if grip_resp.double_clicked() {
                self.bar_h = 86.0;
                self.set_launcher_key("BottomBar", "86");
            } else if grip_resp.drag_stopped() {
                self.set_launcher_key("BottomBar", &format!("{:.0}", self.bar_h));
            }
            let bar_h = ui.available_height().clamp(12.0, 240.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("v{}", version())).size((bar_h * 0.42).clamp(10.0, 22.0)).strong().color(WEAK));
                if let Some((m, bad)) = &self.message {
                    ui.add_space(16.0);
                    ui.label(RichText::new(m).color(if *bad { BAD } else { GOOD }));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(!running, |ui| {
                        ui.menu_button(RichText::new("More").size((bar_h * 0.26).clamp(10.0, 28.0)), |ui| {
                            if ui.button("New game").clicked() {
                                self.start(ctx, Start::NewGame);
                                ui.close();
                            }
                            if ui.button("Main menu").clicked() {
                                self.start(ctx, Start::MainMenu);
                                ui.close();
                            }
                            if ui.button("Explorer (free camera)").clicked() {
                                self.start(ctx, Start::Explorer);
                                ui.close();
                            }
                            ui.separator();
                            let mut close = self.launcher_flag("CloseOnPlay", false);
                            if ui.checkbox(&mut close, "Close the launcher when the game starts").changed() {
                                self.set_launcher_key("CloseOnPlay", if close { "1" } else { "0" });
                            }
                        });
                        if !running && self.saves.len() > 1 {
                            let mut picked = None;
                            ui.menu_button(RichText::new("Saves").size((bar_h * 0.26).clamp(10.0, 28.0)), |ui| {
                                for (i, s) in self.saves.iter().enumerate() {
                                    let on = self.newest.as_ref().is_some_and(|n| n.path == s.path);
                                    if ui.selectable_label(on, format!("{} · {}", s.name, s.detail)).clicked() {
                                        picked = Some(i);
                                        ui.close();
                                    }
                                }
                            });
                            if let Some(i) = picked {
                                self.newest = self.saves.get(i).cloned();
                            }
                        }
                        let (title, sub, start) = match &self.newest {
                            _ if running => ("Running…".to_string(), "the game is open".to_string(), None),
                            Some(s) => ("Continue".to_string(), format!("{} · {}", s.name, s.detail), Some(Start::Load(s.path.clone()))),
                            None => ("Play".to_string(), "new game from the main menu".to_string(), Some(Start::MainMenu)),
                        };
                        if big_button(ui, &title, &sub, true, bar_h).clicked() {
                            if let Some(s) = start {
                                self.start(ctx, s);
                            }
                        }
                        if self.newest.is_some() && !running && big_button(ui, "Main menu", "load or start a game", false, bar_h).clicked() {
                            self.start(ctx, Start::MainMenu);
                        }
                        if let Some(s) = self.newest.clone() {
                            if let Some(shot) = s.shot.clone() {
                                let uri = format!("bytes://{}", s.path.file_name().and_then(|n| n.to_str()).unwrap_or("save"));
                                let resp = ui.add(egui::Image::from_bytes(uri, Bytes::Shared(shot)).fit_to_exact_size(Vec2::new(bar_h * 1.7, bar_h * 0.95)).corner_radius(3).sense(Sense::click())).on_hover_text("Load this save");
                                if resp.clicked() {
                                    self.start(ctx, Start::Load(s.path));
                                }
                            }
                        }
                    });
                });
            });
        });
    }
}

const REPO: &str = "https://github.com/LootJunkieAzza/NVForever";

/// A drop that did not come with a real path (the name only). The bytes are written
/// aside and installed from there, so a name alone never becomes an empty mod folder.
fn write_dropped_bytes(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("nvforever-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A page button of the left bar; the open page's has a gold edge and mark.
fn load_mod_rows(home: &EngineHome) -> Vec<ModRow> {
    let mods = home.mods();
    let mut found = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&mods) {
        for entry in rd.flatten() {
            let path = entry.path();
            // A directory link is a mod folder. A file is not.
            if !path.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else { continue };
            if name.starts_with('.') {
                continue;
            }
            found.push(name);
        }
    }
    found.sort_by_key(|a| a.to_ascii_lowercase());
    let text = std::fs::read_to_string(home.root.join("mod-order.txt")).unwrap_or_default();
    let mut order = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        let (on, name) = match t.strip_prefix('*') {
            Some(rest) => (true, rest.trim()),
            None => (false, t),
        };
        if name.is_empty() {
            continue;
        }
        if let Some(real) = found.iter().find(|n| n.eq_ignore_ascii_case(name)) {
            if !order.iter().any(|m: &ModRow| m.name.eq_ignore_ascii_case(real)) {
                order.push(ModRow { name: real.clone(), on });
            }
        }
    }
    for name in found {
        if !order.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
            order.push(ModRow { name, on: true });
        }
    }
    order
}

fn mod_file_keys(folder: &Path) -> Vec<String> {
    let root = if folder.join("Data").is_dir() { folder.join("Data") } else { folder.to_path_buf() };
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(&root) else { continue };
            let key = rel.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
            if !key.is_empty() {
                out.push(key);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn note_files(files: &HashMap<String, Vec<String>>, folder: &str, plugin: &str) -> String {
    let key = plugin.to_ascii_lowercase();
    files.get(folder).and_then(|list| list.iter().find(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).eq_ignore_ascii_case(&key)).cloned()).unwrap_or_else(|| plugin.to_string())
}

fn clash_notes(rows: &[(String, bool)], files: &HashMap<String, Vec<String>>) -> HashMap<String, Vec<Clash>> {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for (name, on) in rows {
        if !on {
            continue;
        }
        let Some(list) = files.get(name) else { continue };
        for file in list {
            let who = owners.entry(file.clone()).or_default();
            if who.last().map(String::as_str) != Some(name.as_str()) {
                who.push(name.clone());
            }
        }
    }
    let mut grouped: HashMap<(String, String, bool), Vec<String>> = HashMap::new();
    for (file, who) in owners {
        if who.len() < 2 {
            continue;
        }
        let winner = who.last().cloned().unwrap_or_default();
        for loser in &who[..who.len() - 1] {
            grouped.entry((winner.clone(), loser.clone(), true)).or_default().push(file.clone());
            grouped.entry((loser.clone(), winner.clone(), false)).or_default().push(file.clone());
        }
    }
    let mut out: HashMap<String, Vec<Clash>> = HashMap::new();
    for ((folder, other, wins), mut found) in grouped {
        found.sort();
        out.entry(folder).or_default().push(Clash { other, files: found, wins });
    }
    for notes in out.values_mut() {
        notes.sort_by(|a, b| b.wins.cmp(&a.wins).then_with(|| a.other.cmp(&b.other)));
    }
    out
}

fn nav_button(ui: &mut egui::Ui, name: &str, on: bool) -> egui::Response {
    let text = RichText::new(name).font(crate::fonts::title(15.0)).color(if on { GOLD } else { theme::TEXT });
    let b = egui::Button::new(text)
        .min_size(Vec2::new(ui.available_width(), 42.0))
        .fill(if on { theme::SELECTED } else { theme::BUTTON })
        .stroke(Stroke::new(1.0, if on { theme::GOLD_DIM } else { LINE }));
    let r = ui.add(b);
    if on {
        let bar = egui::Rect::from_min_size(r.rect.left_top() + Vec2::new(0.0, 8.0), Vec2::new(3.0, r.rect.height() - 16.0));
        ui.painter().rect_filled(bar, CornerRadius::same(1), GOLD);
    }
    r
}

/// A button in the site's primary style when `on` (gold, dark words, 9:1), else a plain one.
fn gold_button(text: &str, on: bool) -> egui::Button<'static> {
    let words = RichText::new(text.to_string()).strong();
    if on {
        egui::Button::new(words.color(theme::ON_GOLD)).fill(GOLD)
    } else {
        egui::Button::new(words)
    }
}

/// A large two-line button (title and a small line under it), gold when `primary`.
fn big_button(ui: &mut egui::Ui, title: &str, sub: &str, primary: bool, height: f32) -> egui::Response {
    let height = height.clamp(12.0, 180.0);
    let size = Vec2::new(if primary { height * 4.8 } else { height * 3.3 }, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    let fill = match (primary, resp.hovered() && enabled) {
        (true, true) => theme::GOLD_HOVER,
        (true, false) => GOLD,
        (false, true) => theme::BUTTON_HOVER,
        (false, false) => theme::BUTTON,
    };
    let fill = if enabled { fill } else { fill.gamma_multiply(0.5) };
    let ink = if primary { theme::ON_GOLD } else { theme::TEXT };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(4), fill, Stroke::new(1.0, if primary { GOLD } else { LINE }), egui::StrokeKind::Inside);
    let pad_x = (height * 0.14).clamp(6.0, 18.0);
    let icon = (height * 0.15).clamp(5.0, 13.0);
    let left = rect.left() + pad_x;
    let c = egui::pos2(left + icon, rect.center().y);
    p.add(egui::Shape::convex_polygon(
        vec![c + Vec2::new(-icon * 0.78, -icon), c + Vec2::new(icon, 0.0), c + Vec2::new(-icon * 0.78, icon)],
        ink,
        Stroke::NONE,
    ));
    let text_x = left + icon * 2.1 + (height * 0.06).clamp(3.0, 8.0);
    let max_w = (rect.right() - pad_x - text_x).max(8.0);
    let title_px = (height * 0.32).clamp(10.0, 30.0);
    let sub_px = (height * 0.18).clamp(8.0, 18.0);
    let (title_draw, title_font) = fit_button_line(p, title, crate::fonts::title(title_px), ink, max_w);
    let (sub_draw, sub_font) = fit_button_line(p, sub, egui::FontId::proportional(sub_px), ink, max_w);
    p.text(egui::pos2(text_x, rect.top() + height * 0.10), egui::Align2::LEFT_TOP, title_draw, title_font, ink);
    p.text(
        egui::pos2(text_x, rect.bottom() - height * 0.08),
        egui::Align2::LEFT_BOTTOM,
        sub_draw,
        sub_font,
        ink.gamma_multiply(0.8),
    );
    resp
}

/// Shrink `text` to `max_w`, then cut it only if the smallest size still does not fit.
fn fit_button_line(p: &egui::Painter, text: &str, mut font: egui::FontId, color: egui::Color32, max_w: f32) -> (String, egui::FontId) {
    let min = 8.0;
    while font.size > min && p.layout_no_wrap(text.to_owned(), font.clone(), color).size().x > max_w {
        font.size = (font.size - 0.5).max(min);
    }
    let mut chars: Vec<char> = text.chars().collect();
    let mut shown = text.to_owned();
    while !chars.is_empty() && p.layout_no_wrap(shown.clone(), font.clone(), color).size().x > max_w {
        chars.pop();
        let stem: String = chars.iter().collect();
        shown = format!("{}…", stem.trim_end());
        if chars.is_empty() {
            break;
        }
    }
    (shown, font)
}

/// An on/off switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = Vec2::new(44.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = theme::BUTTON_ACTIVE.lerp_to_gamma(GOLD, t);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(11), bg);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(egui::pos2(x, rect.center().y), 8.0, theme::TEXT);
    resp
}

/// Whether `rule` holds for its row's INI value `value` (its choice's name `name`), the row
/// of kind `kind` (`None`: built in, by the value).
fn rule_holds(rule: &menu::Rule, kind: Option<&menu::Kind>, value: &str, name: &str) -> bool {
    if !rule.choices.is_empty() {
        return rule.choices.iter().any(|c| c.eq_ignore_ascii_case(name));
    }
    match kind {
        Some(menu::Kind::List { choices }) => choices.first().is_none_or(|c| c.name != name),
        Some(menu::Kind::Slider { .. }) => value.parse::<f32>().is_ok_and(|v| v >= 0.5),
        // A switch, or a row with no kind (built in): off as `0`, a list's Off, or no value.
        _ => !(value.is_empty() || value == "0" || value.eq_ignore_ascii_case("off")),
    }
}

/// The part of 0..1 that `text`, typed into a slider of the evenly spread `words`, stands
/// for: one of the words (any case), else a number; `None`: neither.
fn word_part(words: &[String], text: &str) -> Option<f64> {
    let t = text.trim();
    let n = words.len();
    match words.iter().position(|w| w.trim().eq_ignore_ascii_case(t)) {
        Some(i) if n > 1 => Some(i as f64 / (n - 1) as f64),
        Some(_) => Some(0.0),
        None => t.parse::<f64>().ok().filter(|v| v.is_finite()),
    }
}

/// Whether the INI's value `v` is one the game's row kind `kind` takes (else the page goes by
/// the value, as for a row with no kind).
fn menu_fits(kind: &menu::Kind, v: &str) -> bool {
    match kind {
        menu::Kind::Switch => matches!(v.trim(), "0" | "1"),
        menu::Kind::Slider { .. } => v.trim().parse::<f32>().is_ok(),
        menu::Kind::List { choices } => choices.iter().any(|c| c.name == v),
    }
}

/// The control for a row of kind `kind` (the game's menu file, src/catalog.rs), drawn as the
/// page draws the other rows; the new value when it changed.
fn menu_control(ui: &mut egui::Ui, key: &str, kind: &menu::Kind, v: &str, control_w: f32) -> Option<String> {
    match kind {
        menu::Kind::Switch => {
            let mut on = v.trim() == "1";
            toggle(ui, &mut on).changed().then(|| if on { "1" } else { "0" }.to_string())
        }
        menu::Kind::Slider { min, step, steps } => {
            // A part of 0..1 whatever the file says (a hand edit, a broken write).
            let min = &if min.is_finite() { min.clamp(0.0, 1.0) } else { 0.0 };
            let mut f: f32 = v.trim().parse().unwrap_or(*min);
            // No egui steps: egui counts them from the range's start, the game from 0
            // (`Slider::fit`), so a value of the game's would move by being drawn. A value the
            // player moves is fitted as the game fits it; one drawn stays as the INI has it.
            let s = egui::Slider::new(&mut f, *min..=1.0).clamping(egui::SliderClamping::Edits);
            let s = if steps.is_empty() {
                s.custom_formatter(|x, _| format!("{:.0}%", x * 100.0)).custom_parser(|s| s.trim().trim_end_matches('%').parse::<f64>().ok().map(|x| x / 100.0))
            } else {
                let (shown, typed) = (steps.clone(), steps.clone());
                // Typed into it: a step's words (any case), else a number.
                s.custom_formatter(move |x, _| menu::Step::nearest(&shown, x as f32).unwrap_or_default().to_string())
                    .custom_parser(move |t| menu::Step::parse(&typed, t).map(f64::from))
            };
            ui.add_sized([control_w, 20.0], s).changed().then(|| part_text(game_fit(f, *min, *step)))
        }
        menu::Kind::List { choices } => {
            let shown = choices.iter().find(|c| c.name == v).map_or(v.to_string(), |c| c.label.clone());
            let mut picked = None;
            egui::ComboBox::from_id_salt(key).width(control_w - 8.0).selected_text(shown).show_ui(ui, |ui| {
                for c in choices {
                    if ui.selectable_label(c.name == v, &c.label).clicked() && c.name != v {
                        picked = Some(c.name.clone());
                    }
                }
            });
            picked
        }
    }
}

/// `v` brought into `min..=1` and onto a step of `step` from 0, to the thousandth, as the game
/// fits a slider's value (engine/nvf-play/src/settings.rs, `Slider::fit`).
fn game_fit(v: f32, min: f32, step: f32) -> f32 {
    let step = step.max(1e-3);
    let v = (v.clamp(min, 1.0) / step).round() * step;
    ((v * 1000.0).round() / 1000.0).clamp(min, 1.0)
}

/// A slider part (`0.50`) as the INI keeps it: to the thousandth, at least two places.
fn part_text(v: f32) -> String {
    let s = format!("{:.3}", v.clamp(0.0, 1.0));
    if s.ends_with('0') {
        s[..s.len() - 1].to_string()
    } else {
        s
    }
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new().fill(CARD).corner_radius(6).stroke(Stroke::new(1.0, LINE)).inner_margin(Margin::same(16)).show(ui, add).inner
}

// ---- The pages ----

impl App {
    fn home_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("News");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let on = self.launcher_flag("News", true);
                if on && !matches!(self.news_state, NewsState::Loading) && ui.small_button("Refresh").clicked() {
                    self.fetch_news(ui.ctx());
                }
                let mut fetch = on;
                if ui.checkbox(&mut fetch, RichText::new("Fetch news").small()).on_hover_text("Ask nv-forever.com (and GitHub) for the news when the launcher opens").changed() {
                    self.set_launcher_key("News", if fetch { "1" } else { "0" });
                    if fetch {
                        self.fetch_news(ui.ctx());
                    } else {
                        self.news_state = NewsState::Off;
                    }
                }
                let note = match &self.news_state {
                    NewsState::Off => "news fetching is off".to_string(),
                    NewsState::Loading => "fetching…".to_string(),
                    NewsState::Shown(Source::Site) => format!("{} posts from nv-forever.com", self.news.len()),
                    NewsState::Shown(Source::Releases) => format!("{} releases from GitHub", self.news.len()),
                    NewsState::Shown(Source::Cache) => "saved posts".to_string(),
                    NewsState::Failed(_) => "offline: showing saved posts".to_string(),
                };
                let l = ui.label(RichText::new(note).small().color(WEAK));
                if let NewsState::Failed(e) = &self.news_state {
                    l.on_hover_text(e);
                }
            });
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            if self.news.is_empty() {
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("No news yet").size(20.0).color(GOLD));
                    ui.label("Posts from nv-forever.com and the release notes show here once the launcher has fetched them.");
                });
            }
            for i in 0..self.news.len() {
                let (post, img) = &self.news[i];
                let (post, img) = (post.clone(), img.clone());
                card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let Some(bytes) = img {
                        let w = ui.available_width();
                        ui.add(egui::Image::from_bytes(format!("bytes://news/{i}/{}", post.title), Bytes::Shared(bytes)).max_width(w).max_height(360.0).corner_radius(4));
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&post.title).font(crate::fonts::title(24.0)).color(GOLD));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let by = [post.shown_date(), post.author.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                            ui.label(RichText::new(by).small().color(WEAK));
                        });
                    });
                    let open = self.expanded.contains(&i);
                    let (text, cut) = preview(&post.body, open);
                    // Room between lines and paragraphs: a bigger body size (the font's own
                    // line height grows with it) and more space between blocks.
                    ui.scope(|ui| {
                        let s = ui.style_mut();
                        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
                        s.text_styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
                        s.spacing.item_spacing.y = 10.0;
                        CommonMarkViewer::new().show(ui, &mut self.md, text);
                    });
                    ui.horizontal(|ui| {
                        if cut && ui.button(if open { "Show less" } else { "Read more" }).clicked() {
                            if open {
                                self.expanded.remove(&i);
                            } else {
                                self.expanded.insert(i);
                            }
                        }
                        if let Some(url) = &post.url {
                            if ui.link("Open in the browser").clicked() {
                                launch::open_url(url);
                            }
                        }
                    });
                });
                ui.add_space(12.0);
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Settings");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dirty = !self.edits.is_empty();
                if ui.add_enabled(dirty, gold_button("Save", dirty)).clicked() {
                    let r = self.save_settings();
                    self.after_write(r, Some("Settings saved: the game uses them at its next start."));
                }
                if ui.add_enabled(dirty, egui::Button::new("Undo changes")).clicked() {
                    self.edits.clear();
                }
                if dirty {
                    ui.label(RichText::new(format!("{} changed", self.edits.len())).small().color(GOLD));
                }
            });
        });
        ui.label(
            RichText::new("The game's own settings page, kept in NVForever.ini. A row the game has not written yet shows its default once you change it in the game.")
                .small()
                .color(WEAK),
        );
        ui.horizontal_wrapped(|ui| {
            for (i, t) in self.tab_labels.clone().iter().enumerate() {
                let on = self.tab == i;
                let b = egui::Button::new(RichText::new(t).strong().color(if on { theme::ON_GOLD } else { theme::TEXT })).fill(if on { GOLD } else { Color32::TRANSPARENT }).stroke(Stroke::NONE);
                if ui.add(b).clicked() {
                    self.tab = i;
                }
            }
        });
        ui.separator();
        let tab = self.tabs.get(self.tab).cloned();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(tab) = tab {
                if tab == "Graphics" {
                    self.dlss_card(ui);
                }
                // A row with no words built in (a Tweaks row before the game wrote its menu) only
                // while the INI holds it.
                let rows: Vec<Row> = self.rows.iter().filter(|r| r.tab == tab && (!r.unlisted || self.stored.contains_key(&r.key))).cloned().collect();
                let mut heading: Option<Option<String>> = None;
                for r in rows {
                    // The game's rules: hidden while the row it goes with is not on its
                    // choice, greyed (with the game's reason) while a row it needs is off.
                    // No tab the launcher shows has only hidden rows (only the game's own
                    // Controls tab can), so a tab is never left empty by them.
                    if r.shown_with.as_ref().is_some_and(|rule| !self.holds(rule)) {
                        continue;
                    }
                    let greyed = r.needs.iter().find(|rule| !self.holds(rule)).map(|rule| rule.why.clone());
                    if let Some(colour) = crate::display::colour_of_part(&r.key) {
                        if !self.current(colour).is_some_and(|c| c.eq_ignore_ascii_case(crate::display::CUSTOM)) {
                            continue;
                        }
                    }
                    if heading.as_ref() != Some(&r.heading) {
                        if let Some(h) = &r.heading {
                            ui.add_space(8.0);
                            ui.label(RichText::new(h.to_uppercase()).small().strong().color(theme::GOLD_TEXT));
                        }
                        heading = Some(r.heading.clone());
                    }
                    self.setting_row(ui, &r.key, &r.label, &r.note, r.kind.as_ref(), greyed.as_deref());
                }
            }
        });
    }

    fn setting_row(&mut self, ui: &mut egui::Ui, key: &str, label: &str, note: &str, kind: Option<&menu::Kind>, greyed: Option<&str>) {
        let stored = self.stored.get(key).cloned();
        let value = self.edits.get(key).cloned().or_else(|| stored.clone());
        Frame::new().fill(CARD).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Greyed as the game greys it: shown, its value kept, not changed here.
                if greyed.is_some() {
                    ui.disable();
                }
                let control_w = 260.0;
                // Resolution's two lists carry their names to the left: room for them.
                let column_w = if key == "resolution" { control_w + 90.0 } else { control_w };
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - column_w - 16.0).max(200.0));
                    let changed = self.edits.contains_key(key);
                    ui.label(RichText::new(if changed { format!("{label}  •") } else { label.to_string() }).strong().color(if changed { GOLD } else { theme::TEXT }));
                    if !note.is_empty() {
                        ui.label(RichText::new(note).small().color(WEAK));
                    }
                    if let Some(why) = greyed {
                        ui.label(RichText::new(format!("Greyed out: {why}")).small().color(WEAK));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if key == "reports-delete" {
                        self.reports_control(ui);
                        return;
                    }
                    let Some(v) = value else {
                        ui.label(RichText::new("game default").small().color(WEAK)).on_hover_text("Not in NVForever.ini yet: change it once on the game's settings page");
                        return;
                    };
                    // Not a value: the game shows what was sent and offers "Delete them now"; the
                    // INI only ever holds "kept". The launcher shows the state (the same file).
                    if key == "reports-delete" {
                        self.reports_control(ui);
                        return;
                    }
                    if crate::display::read_only(key) {
                        ui.label(RichText::new("set in the game").small().color(WEAK))
                            .on_hover_text("The game works this out from the rows below; choosing one there sets them all (a preset from the game folder's files).");
                        ui.label(RichText::new(&v).strong());
                        return;
                    }
                    let new = if key == "resolution" {
                        self.resolution_control(ui, &v, control_w)
                    } else if let Some(kind) = kind.filter(|k| menu_fits(k, &v)) {
                        // The game's own row (its menu file) whenever the INI's value is one it
                        // takes: before the launcher's lists below, which are copies that can
                        // drift (Frame Rate Limit became a list of names in the overhaul).
                        menu_control(ui, key, kind, &v, control_w)
                    } else if let Some(choices) = self.display_choices(key) {
                        let at = crate::display::chosen(key, &choices, &v);
                        let shown = at.map_or(v.clone(), |i| choices[i].label.clone());
                        let mut picked = None;
                        egui::ComboBox::from_id_salt(key).width(control_w - 8.0).selected_text(shown).show_ui(ui, |ui| {
                            for (i, c) in choices.iter().enumerate() {
                                if ui.selectable_label(at == Some(i), &c.label).clicked() {
                                    picked = Some(c.stored.clone());
                                }
                            }
                        });
                        picked.filter(|p| at.is_none_or(|i| &choices[i].stored != p))
                    } else if let Some(&list) = self.lists.get(key) {
                        // A list row before its value's kind: an older 1/0 it took over from
                        // shows as the choice it stands for (catalog::list_value).
                        let v = catalog::list_value(key, &v);
                        let mut t = v.clone();
                        egui::ComboBox::from_id_salt(key).width(control_w - 8.0).selected_text(t.clone()).show_ui(ui, |ui| {
                            for c in list.iter() {
                                ui.selectable_value(&mut t, c.to_string(), *c);
                            }
                        });
                        (t != v).then_some(t)
                    } else {
                        match Kind::of_value(&v) {
                        Kind::Switch => {
                            let mut on = v.trim() == "1";
                            toggle(ui, &mut on).changed().then(|| if on { "1" } else { "0" }.to_string())
                        }
                        Kind::Slider => {
                            let mut f: f32 = v.trim().parse().unwrap_or(0.0);
                            let words: Option<Vec<String>> = self.words.get(key).map(|w| w.iter().map(|s| s.to_string()).collect()).or_else(|| crate::display::steps(key));
                            let s = match words {
                                Some(words) => {
                                    let n = words.len();
                                    let typed = words.clone();
                                    egui::Slider::new(&mut f, 0.0..=1.0)
                                        .step_by(1.0 / (n - 1) as f64)
                                        .custom_formatter(move |x, _| words[((x * (n - 1) as f64).round() as usize).min(n - 1)].clone())
                                        .custom_parser(move |t| word_part(&typed, t))
                                }
                                None => egui::Slider::new(&mut f, 0.0..=1.0).custom_formatter(|x, _| format!("{:.0}%", x * 100.0)).custom_parser(|s| {
                                    s.trim().trim_end_matches('%').parse::<f64>().ok().map(|x| x / 100.0)
                                }),
                            };
                            ui.add_sized([control_w, 20.0], s).changed().then(|| part_text(f))
                        }
                        Kind::Text => {
                            let mut t = v.clone();
                            ui.add_sized([control_w, 22.0], egui::TextEdit::singleline(&mut t)).changed().then_some(t)
                        }
                        }
                    };
                    if let Some(n) = new {
                        // A named colour writes its three parts too: the game reads the parts
                        // after the name, and a part read makes the colour Custom. A part moved
                        // makes it Custom here as in the game.
                        if let (Some(parts), Some(rgb)) = (crate::display::colour_parts(key), crate::display::preset_rgb(&n)) {
                            for (k, c) in parts.into_iter().zip(rgb) {
                                self.set_edit(k, part_text(f32::from(c) / 255.0));
                            }
                        }
                        if let Some(colour) = crate::display::colour_of_part(key) {
                            self.set_edit(colour, crate::display::CUSTOM.to_string());
                        }
                        self.set_edit(key, n);
                    }
                });
            });
        });
        ui.add_space(2.0);
    }

    /// Change `key` to `value` on the page (no change when it is the INI's value).
    fn set_edit(&mut self, key: &str, value: String) {
        if self.stored.get(key) == Some(&value) {
            self.edits.remove(key);
        } else {
            self.edits.insert(key.to_string(), value);
        }
    }

    /// "Reports already sent": what this PC sent, and "Delete them now" (asks first).
    fn reports_control(&mut self, ui: &mut egui::Ui) {
        let sent = self.home.as_ref().is_some_and(crate::reports::any_sent);
        if self.reports_rx.is_some() {
            ui.add(egui::Spinner::new());
            ui.label(RichText::new("Deleting…").small().color(WEAK));
        } else if sent {
            if ui.button("Delete them now").clicked() {
                self.reports_confirm = true;
            }
            ui.label(RichText::new("Kept on the server").small().color(WEAK));
        } else {
            ui.label(RichText::new("None sent from this PC").small().color(WEAK));
        }
    }

    /// The question before deleting, and the request in the background.
    fn reports_window(&mut self, ctx: &egui::Context, running: bool) {
        if let Some(r) = self.reports_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.reports_rx = None;
            self.message = Some(match r {
                Ok(n) => (format!("The server was asked to delete the reports of {n} install ID(s) from this PC (it removes them within 7 days). Later reports get a new ID."), false),
                Err(e) => (format!("Could not delete the reports: {e}"), true),
            });
            self.reload_ini();
        }
        if !self.reports_confirm {
            return;
        }
        let modal = egui::Modal::new(egui::Id::unique("reports-delete")).frame(Frame::window(&ctx.global_style()).inner_margin(Margin::symmetric(24, 20))).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Delete your reports?").size(20.0).strong().color(GOLD));
            ui.add_space(8.0);
            ui.label("NVForever's report server is asked to delete every report this PC sent; it removes them within 7 days. Reports not sent yet are deleted here. Later reports get a new ID, not linked to these.");
            ui.add_space(4.0);
            ui.label(RichText::new("This cannot be undone.").color(BAD));
            if running {
                ui.label(RichText::new("Close the game first: it holds this PC's report ID while it runs.").small().color(WEAK));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let delete = egui::Button::new(RichText::new("Delete them").strong().color(theme::ON_GOLD)).fill(BAD);
                if ui.add_enabled(!running, delete).clicked() {
                    if let Some(home) = self.home.clone() {
                        let (tx, rx) = channel();
                        let c = ctx.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::reports::delete_all(&home));
                            c.request_repaint();
                        });
                        self.reports_rx = Some(rx);
                    }
                    self.reports_confirm = false;
                }
                if ui.button("Cancel").clicked() {
                    self.reports_confirm = false;
                }
            });
        });
        if modal.should_close() {
            self.reports_confirm = false;
        }
    }

    /// The DLSS card (src/dlss.rs): what DLSS needs, what this PC has, and the download.
    fn dlss_card(&mut self, ui: &mut egui::Ui) {
        use nvf_dlss_files::download;
        let running = download::current().map(|h| h.progress()).filter(|p| !p.phase.finished());
        let last = download::current().map(|h| h.progress()).filter(|p| p.phase.finished());
        if matches!(last.as_ref().map(|p| &p.phase), Some(download::Phase::Done(_))) && !self.dlss.complete() {
            self.dlss = crate::dlss::Status::read(self.exe.as_deref());
        }
        let s = self.dlss.clone();
        let need = |ui: &mut egui::Ui, ok: bool, words: String| {
            ui.horizontal(|ui| {
                // A dot, green or red (the system font has no check marks).
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.5, if ok { GOOD } else { BAD });
                ui.label(RichText::new(words).small());
            });
        };
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("NVIDIA DLSS").strong().size(17.0));
            ui.label(RichText::new("An optional upscaler beside FSR 3.1 (and DLSS frame generation on RTX 40 or newer). NVForever ships no NVIDIA file: the four upscaling and four frame generation files come from NVIDIA's own Streamline release, under NVIDIA's licence.").small().color(WEAK));
            ui.add_space(6.0);
            let os_words = if s.windows { "Windows".to_string() } else { "Windows. The DLSS files are the Windows build, so this stays off here.".to_string() };
            need(ui, s.windows, os_words);
            let gpu = if s.gpus.is_empty() {
                if s.windows { "no graphics card named by Windows".to_string() } else { "no card named in the NVIDIA driver".to_string() }
            } else {
                s.gpus.join(", ")
            };
            need(ui, s.rtx(), format!("An NVIDIA RTX graphics card with a current driver ({gpu})"));
            need(ui, s.upscaling == nvf_dlss_files::FILES.len(), format!("Upscaling files: {} of {}", s.upscaling, nvf_dlss_files::FILES.len()));
            need(ui, s.frame_generation == nvf_dlss_files::FG_FILES.len(), format!("Frame generation files: {} of {}", s.frame_generation, nvf_dlss_files::FG_FILES.len()));
            match &s.folder {
                Ok(f) => ui.label(RichText::new(format!("In {}", f.display())).small().color(WEAK)),
                Err(e) => ui.label(RichText::new(e.clone()).small().color(BAD)),
            };
            ui.add_space(6.0);
            if let Some(p) = &running {
                let (words, part) = crate::dlss::progress_words(p);
                let bar = match part {
                    Some(f) => egui::ProgressBar::new(f).text(words),
                    None => egui::ProgressBar::new(0.0).animate(true).text(words),
                };
                ui.add(bar);
                if ui.button("Cancel download").clicked() {
                    if let Some(h) = download::current() {
                        h.cancel();
                    }
                }
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                return;
            }
            if let Some(p) = &last {
                let (words, _) = crate::dlss::progress_words(p);
                let ok = matches!(p.phase, download::Phase::Done(_));
                ui.label(RichText::new(words).small().color(if ok { GOOD } else if matches!(p.phase, download::Phase::Cancelled) { WEAK } else { BAD }));
                if ok {
                    ui.label(RichText::new("Choose DLSS as the Upscaler below (or in the game's settings).").small().color(WEAK));
                }
            }
            ui.horizontal_wrapped(|ui| {
                if s.windows && s.folder.is_ok() && !s.complete() && ui.add(gold_button("Download DLSS…", true)).on_hover_text("Shows NVIDIA's licence first; nothing is downloaded before you accept it.").clicked() {
                    self.dlss_licence = true;
                }
                if let Ok(f) = &s.folder {
                    if ui.button("Open dlss folder").on_hover_text("To copy the files in by hand (docs/dlss.md).").clicked() {
                        match nvf_dlss_files::create_dlss_folder(f) {
                            Ok(()) => launch::open_folder(f),
                            Err(e) => self.message = Some((format!("{}: {e}", f.display()), true)),
                        }
                    }
                }
                if ui.button("NVIDIA's release page").clicked() {
                    launch::open_url(download::RELEASE_PAGE);
                }
                if ui.button("Check again").clicked() {
                    self.dlss = crate::dlss::Status::read(self.exe.as_deref());
                }
            });
        });
        ui.add_space(4.0);
    }

    /// NVIDIA's licence before the download: I accept takes a mouse click only (as the game's
    /// dialog: no key pressed by habit accepts a licence), Cancel closes with nothing done.
    fn dlss_licence_window(&mut self, ctx: &egui::Context) {
        if !self.dlss_licence {
            return;
        }
        let modal = egui::Modal::new(egui::Id::unique("dlss-licence")).frame(Frame::window(&ctx.global_style()).inner_margin(Margin::symmetric(24, 20))).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.label(RichText::new("NVIDIA's licence").size(20.0).strong().color(GOLD));
            ui.add_space(6.0);
            ui.label("Download DLSS fetches eight files from NVIDIA's Streamline 2.14.16 release on GitHub: the six sl.* files (MIT licence) and nvngx_dlss.dll and nvngx_dlssg.dll, under the licence below. Each file is checked against its pinned SHA-256 and NVIDIA's signatures, and nothing is written unless all pass; a file already in the dlss folder is kept.");
            ui.add_space(6.0);
            Frame::new().fill(theme::BG).corner_radius(4).inner_margin(Margin::same(10)).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new(nvf_dlss_files::LICENCE).small().monospace());
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let accept = ui.add(gold_button("I accept", true)).on_hover_text("A mouse click only.");
                if accept.clicked_by(egui::PointerButton::Primary) {
                    nvf_dlss_files::download::start_into(nvf_dlss_files::download::LicenceAccepted::by_player(), None, crate::dlss::folder_for(self.exe.as_deref()));
                    self.dlss_licence = false;
                }
                if ui.button("Cancel").clicked() {
                    self.dlss_licence = false;
                }
                if ui.button("Open on GitHub").clicked() {
                    launch::open_url(nvf_dlss_files::download::RELEASE_PAGE);
                }
            });
        });
        if modal.should_close() {
            self.dlss_licence = false;
        }
    }

    /// The value of `key` as the page shows it now: an unsaved change, else the INI's.
    fn current(&self, key: &str) -> Option<String> {
        self.edits.get(key).or_else(|| self.stored.get(key)).cloned()
    }

    /// Whether the game's rule `rule` holds for the values on the page now (the INI's, or the
    /// changes not saved yet): its row on, or on one of its choices. As the game: a switch on,
    /// a list past its first choice; the input device from `[Controls]`. A parent the INI does
    /// not hold yet is the game's default for it (its menu file); with none known, the rule
    /// holds: the launcher neither greys nor hides a row on a guess the game may not share.
    fn holds(&self, rule: &menu::Rule) -> bool {
        let parent = self.rows.iter().find(|r| r.key == rule.parent);
        let value = if rule.parent == nvf_settings_layout::DEVICE_ROW {
            ini_get(&self.ini_text, "Controls", "Input Device").unwrap_or_default()
        } else {
            match self.current(&rule.parent).or_else(|| parent.and_then(|r| r.default.clone())) {
                Some(v) => v,
                None => return true,
            }
        };
        let kind = parent.and_then(|r| r.kind.as_ref());
        // The choice's name: the INI keeps some lists by another word (frame generation
        // `dlss`), which the launcher's own list names.
        let name = match kind {
            Some(menu::Kind::List { choices }) if choices.iter().any(|c| c.name == value) => value.clone(),
            _ => self.display_choices(&rule.parent).and_then(|l| l.into_iter().find(|c| c.stored == value)).map_or(value.clone(), |c| c.label),
        };
        rule_holds(rule, kind, value.trim(), name.trim())
    }

    /// The list for a Display row the game keeps as a choice (src/display.rs); `None` for
    /// the other rows, and for Monitor when the displays could not be asked.
    fn display_choices(&self, key: &str) -> Option<Vec<crate::display::Choice>> {
        use crate::display;
        match key {
            "display-sync" => Some(display::sync_choices()),
            "frame-cap" => Some(display::cap_choices()),
            "monitor" if !self.monitors.is_empty() => Some(display::monitor_choices(&self.monitors)),
            "physics-rate" => Some(display::physics_choices()),
            "fps-counter-low" => Some(display::counter_low_choices()),
            "crosshair-style" => Some(display::crosshair_choices()),
            "hud-colour" | "pipboy-colour" => Some(display::colour_choices()),
            "language" => Some(self.languages.clone()),
            "frame-generation" => Some(display::frame_gen_choices()),
            k => display::named_choices(k),
        }
    }

    /// Resolution: the window's size and full screen's, as two lists (the INI keeps both in
    /// one value). The sizes are those of the display chosen on the Monitor row.
    fn resolution_control(&mut self, ui: &mut egui::Ui, v: &str, width: f32) -> Option<String> {
        use crate::display;
        let (window, full) = display::split_resolution(v);
        let monitor_value = self.current("monitor");
        let m = display::chosen_monitor(&self.monitors, monitor_value.as_deref());
        let mut out = None;
        ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Max), |ui| {
            for (name, now, choices, is_window) in [("Window", &window, display::window_choices(m, &window), true), ("Full screen", &full, display::full_choices(m, &full), false)] {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let at = display::chosen("resolution", &choices, now);
                        let shown = at.map_or(now.clone(), |i| choices[i].label.clone());
                        egui::ComboBox::from_id_salt(("resolution", name)).width(width - 8.0).selected_text(shown).show_ui(ui, |ui| {
                            for (i, c) in choices.iter().enumerate() {
                                if ui.selectable_label(at == Some(i), &c.label).clicked() && at != Some(i) {
                                    out = Some(if is_window { display::join_resolution(&c.stored, &full) } else { display::join_resolution(&window, &c.stored) });
                                }
                            }
                        });
                        ui.label(RichText::new(name).small().color(WEAK));
                    });
                });
            }
        });
        out
    }

    fn load_mods(&mut self) {
        self.mods_dirty = false;
        self.mod_drag = None;
        let Some(home) = self.home.clone() else { return };
        if let Some(game) = self.game() {
            match plugins::sync(&home, &game) {
                Ok(synced) => {
                    self.mods_present = synced.present.into_iter().map(|s| s.to_ascii_lowercase()).collect();
                }
                Err(e) => {
                    self.message = Some((e.to_string(), true));
                    self.mods_present.clear();
                }
            }
        } else {
            self.mods_present.clear();
        }
        self.mods = std::fs::read_to_string(home.plugins_file()).ok().map(|t| plugins::PluginList::parse(&t));
        self.drop_gone_plugins();
        self.mod_rows = load_mod_rows(&home);
        self.refresh_plugin_notes();
        self.keep_data_plugins();
        self.drop_inactive_mod_plugins();
    }

    /// Plugins that belong only to a mod folder that is off leave the list. A plugin already in Data stays.
    fn drop_inactive_mod_plugins(&mut self) {
        let Some(game) = self.game() else { return };
        let off: HashSet<String> = self.mod_rows.iter().filter(|r| !r.on).map(|r| r.name.clone()).collect();
        if off.is_empty() {
            return;
        }
        let owned: HashSet<String> = self.plugin_home.iter().filter(|(_, folder)| off.contains(*folder)).map(|(p, _)| p.clone()).collect();
        let Some(list) = self.mods.as_mut() else { return };
        let before = list.entries.len();
        list.entries.retain(|entry| {
            let key = entry.name.to_ascii_lowercase();
            if entry.name.eq_ignore_ascii_case("FalloutNV.esm") {
                return true;
            }
            !(owned.contains(&key) && game.data_file(&entry.name).is_none())
        });
        let gone = before - list.entries.len();
        if gone > 0 {
            self.mods_dirty = true;
            self.message = Some((format!("Removed {gone} plugins from mods that are off. Save to keep that."), false));
        }
    }

    /// A folder that is off does not turn off a plugin already in the game Data folder.
    fn keep_data_plugins(&mut self) {
        let Some(game) = self.game() else { return };
        let off: HashSet<String> = self.mod_rows.iter().filter(|r| !r.on).map(|r| r.name.clone()).collect();
        if off.is_empty() {
            return;
        }
        let owned: Vec<String> = self.plugin_home.iter().filter(|(_, folder)| off.contains(*folder)).map(|(plugin, _)| plugin.clone()).collect();
        let Some(list) = self.mods.as_mut() else { return };
        let mut changed = false;
        for entry in &mut list.entries {
            let key = entry.name.to_ascii_lowercase();
            if entry.active || !owned.iter().any(|n| n == &key) {
                continue;
            }
            if game.data_file(&entry.name).is_some() {
                entry.active = true;
                changed = true;
            }
        }
        if changed {
            self.mods_dirty = true;
            self.message = Some(("Plugins already in the game Data folder were left on. Save to keep that.".into(), false));
        }
    }

    /// A plugin whose file is not in Data and not under the mods folder is taken off
    /// the list. FalloutNV.esm stays. An empty scan removes nothing.
    fn drop_gone_plugins(&mut self) {
        if self.mods_present.is_empty() {
            return;
        }
        let Some(list) = self.mods.as_mut() else { return };
        let before = list.entries.len();
        list.entries.retain(|e| {
            let key = e.name.to_ascii_lowercase();
            key == "falloutnv.esm" || self.mods_present.contains(&key)
        });
        let gone = before - list.entries.len();
        if gone > 0 {
            self.mods_dirty = true;
            self.message = Some((format!("Removed {gone} plugins whose files are gone. Save to keep that."), false));
        }
    }

    fn refresh_plugin_notes(&mut self) {
        self.plugin_folders.clear();
        self.plugin_masters.clear();
        self.plugin_home.clear();
        self.folder_counts.clear();
        self.folder_files.clear();
        let Some(home) = self.home.clone() else { return };
        let mods = home.mods();
        let mut files: HashMap<String, PathBuf> = HashMap::new();
        for row in &self.mod_rows {
            let listed = plugins::plugins_in_tree(&mods.join(&row.name));
            self.folder_counts.insert(row.name.clone(), listed.len());
            self.folder_files.insert(row.name.clone(), mod_file_keys(&mods.join(&row.name)));
            for (name, path) in listed {
                let key = name.to_ascii_lowercase();
                self.plugin_home.insert(key.clone(), row.name.clone());
                if !row.on {
                    continue;
                }
                self.plugin_folders.entry(key.clone()).or_default().push(row.name.clone());
                files.insert(key, path);
            }
        }
        if let Some(game) = self.game() {
            if let Ok(rd) = std::fs::read_dir(&game.data) {
                for entry in rd.flatten() {
                    let path = entry.path();
                    if path.is_symlink() || !path.is_file() {
                        continue;
                    }
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
                    if !plugins::is_plugin(name) {
                        continue;
                    }
                    files.entry(name.to_ascii_lowercase()).or_insert(path);
                }
            }
        }
        for (key, path) in files {
            let names = plugins::master_names(&path);
            if !names.is_empty() {
                self.plugin_masters.insert(key, names);
            }
        }
        self.recompute_clashes();
    }

    fn recompute_clashes(&mut self) {
        let rows: Vec<(String, bool)> = self.mod_rows.iter().map(|r| (r.name.clone(), r.on)).collect();
        self.clashes = clash_notes(&rows, &self.folder_files);
    }

    fn mods_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Mods");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(self.mods_dirty, gold_button("Save", self.mods_dirty)).clicked() {
                    let r = self.save_mods();
                    match r {
                        Ok(()) => {
                            self.mods_dirty = false;
                            self.message = Some(("Load order saved.".into(), false));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                if ui.add_enabled(self.mods_dirty, egui::Button::new("Undo changes")).clicked() {
                    self.load_mods();
                }
                if ui.button("Refresh").clicked() {
                    self.load_mods();
                    if self.message.as_ref().is_none_or(|(text, bad)| !*bad && !text.starts_with("Removed ")) {
                        self.message = Some(("Mods folder refreshed.".into(), false));
                    }
                }
                if let Some(h) = &self.home {
                    if ui.button("Open mods folder").clicked() {
                        let _ = std::fs::create_dir_all(h.mods());
                        launch::open_folder(&h.mods());
                    }
                }
            });
        });
        ui.label(
            RichText::new("NVForever's own load order (plugins.txt in its folder; the original game's list is never read or changed). Top loads first: drag a row to move it. A plugin whose file is gone is taken off this list. Using Mod Organizer 2? Start NVForever from MO2 instead.")
                .small()
                .color(WEAK),
        );
        ui.label(RichText::new("Left: mod folders. A check turns that folder's plugins on. A lower folder's copy of a file is the one used. Right: plugins. Click one to mark its folder.").small().color(WEAK));
        self.start_box(ui);
        ui.separator();
        let list_h = (ui.available_height() - 40.0).max(80.0);
        ui.horizontal(|ui| {
            let left_w = (ui.available_width() * 0.42).max(220.0);
            ui.allocate_ui_with_layout(Vec2::new(left_w, list_h), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(Vec2::new(left_w, list_h));
                self.mod_column(ui);
            });
            let right_w = ui.available_width();
            ui.allocate_ui_with_layout(Vec2::new(right_w, list_h), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(Vec2::new(right_w, list_h));
                self.plugin_column(ui);
            });
        });
        ui.add_space(6.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let running = self.running();
            if self.apply_ask {
                if ui.add_enabled(!running, egui::Button::new("Apply")).clicked() {
                    self.apply_ask = false;
                    match self.apply_mods() {
                        Ok(()) => self.message = Some(("Load order saved. Files were left in their folders.".into(), false)),
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.apply_ask = false;
                }
                ui.label(RichText::new("Files stay in their folders.").small().color(WEAK));
            } else if ui.add_enabled(!running, egui::Button::new("Apply")).on_hover_text("Saves the list. Does not copy files.").clicked() {
                self.apply_ask = true;
            }
        });
    }

    fn mod_column(&mut self, ui: &mut egui::Ui) {
        let mods_on = self.mod_rows.iter().filter(|r| r.on).count();
        let mods_all = self.mod_rows.len();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mod Order").small().color(GOLD));
            ui.label(RichText::new(format!("{mods_on} on / {mods_all}")).small().color(WEAK)).on_hover_text("Folders that are on, and folders that exist. There is no limit.");
            if ui.button(if self.show_overwrites { "Hide overwrites" } else { "Show overwrites" }).clicked() {
                self.show_overwrites = !self.show_overwrites;
            }
        });
        self.mod_tools(ui);
        ui.add(egui::TextEdit::singleline(&mut self.mod_find).hint_text("Find a mod").desired_width(ui.available_width()));
        let mod_find = self.mod_find.trim().to_ascii_lowercase();
        let marked = self.picked_plugin.as_ref().and_then(|key| self.plugin_home.get(key)).cloned();
        if self.show_overwrites {
            ui.label(RichText::new("Mods run from the top of this list to the bottom. The game checks the bottom mod last.").small().color(WEAK));
            ui.label(RichText::new("If two checked mods contain the same file, the lower mod's copy is used. The higher mod's copy is skipped.").small().color(WEAK));
            ui.label(RichText::new("Green means the copy in this mod is used. Red means the copy in this mod is skipped. The same result always uses the same color.").small().color(WEAK));
            ui.label(RichText::new("The note names the other mod. Rest the pointer on a note to see which files.").small().color(WEAK));
            if self.clashes.is_empty() {
                ui.label(RichText::new("No two checked folders share a file.").small().color(WEAK));
            }
        }
        egui::ScrollArea::vertical().id_salt("mod-folders").auto_shrink([false, false]).show(ui, |ui| {
            if self.mod_rows.is_empty() {
                ui.label(RichText::new("No folders in the mods folder.").small().color(WEAK));
                return;
            }
            let n = self.mod_rows.len();
            let mut moved: Option<(usize, usize)> = None;
            let mut rects = Vec::with_capacity(n);
            for i in 0..n {
                let name = self.mod_rows[i].name.clone();
                if !mod_find.is_empty() && !name.to_ascii_lowercase().contains(&mod_find) {
                    rects.push(egui::Rect::NOTHING);
                    continue;
                }
                let mut on = self.mod_rows[i].on;
                let dragged = self.mod_drag == Some(i);
                let shown = marked.as_deref() == Some(name.as_str());
                let fill = if dragged || shown { theme::SELECTED } else { CARD };
                let stroke = if dragged || shown { Stroke::new(1.0, if shown && !dragged { GOLD } else { theme::GOLD_DIM }) } else { Stroke::NONE };
                let mut box_changed = false;
                let row = ui.scope_builder(egui::UiBuilder::new().id_salt(("mod-folder", i)).sense(Sense::click_and_drag()), |ui| {
                    Frame::new().fill(fill).stroke(stroke).corner_radius(4).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (grip, _) = ui.allocate_exact_size(Vec2::new(14.0, 18.0), Sense::hover());
                            let c = if dragged { GOLD } else { WEAK };
                            for k in [-4.0, 0.0, 4.0] {
                                let y = grip.center().y + k;
                                ui.painter().line_segment([egui::pos2(grip.left() + 1.0, y), egui::pos2(grip.right() - 1.0, y)], Stroke::new(1.5, c));
                            }
                            if ui.checkbox(&mut on, "").changed() {
                                box_changed = true;
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(&name);
                                    let count = self.folder_counts.get(&name).copied().unwrap_or(0);
                                    if count > 0 {
                                        ui.label(RichText::new(count.to_string()).small().color(WEAK)).on_hover_text("Plugins in this folder.");
                                    }
                                    if shown {
                                        ui.label(RichText::new("clicked plugin").small().color(GOLD));
                                    }
                                });
                                if self.show_overwrites {
                                    if let Some(notes) = self.clashes.get(&name) {
                                        let order: Vec<&str> = self.mod_rows.iter().map(|r| r.name.as_str()).collect();
                                        let place = |mod_name: &str| order.iter().position(|n| *n == mod_name).unwrap_or(usize::MAX);
                                        let mut used_mods: Vec<&str> = notes.iter().filter(|n| n.wins).map(|n| n.other.as_str()).collect();
                                        let mut skipped_mods: Vec<&str> = notes.iter().filter(|n| !n.wins).map(|n| n.other.as_str()).collect();
                                        used_mods.sort_by_key(|m| place(m));
                                        skipped_mods.sort_by_key(|m| place(m));
                                        let used_files: Vec<String> = notes.iter().filter(|n| n.wins).flat_map(|n| n.files.iter().map(|f| format!("{f}  (also in {})", n.other))).collect();
                                        let skipped_files: Vec<String> = notes.iter().filter(|n| !n.wins).flat_map(|n| n.files.iter().map(|f| format!("{f}  (used from {})", n.other))).collect();
                                        let file_word = |n: usize| if n == 1 { "file" } else { "files" };
                                        let be = |n: usize| if n == 1 { "is" } else { "are" };
                                        if !used_mods.is_empty() {
                                            let nfiles = notes.iter().filter(|n| n.wins).flat_map(|n| n.files.iter()).collect::<HashSet<_>>().len();
                                            let who = if used_mods.len() == 1 {
                                                format!("{} is higher in the list, so the copy in that mod is skipped", used_mods[0])
                                            } else {
                                                format!("These mods are higher in the list, so the copy in each of them is skipped: {}", used_mods.join(", "))
                                            };
                                            let line = format!("{nfiles} shared {} in this mod {} used. {who}.", file_word(nfiles), be(nfiles));
                                            ui.label(RichText::new(line).small().color(GOOD)).on_hover_text(used_files.join("\n"));
                                        }
                                        if !skipped_mods.is_empty() {
                                            let nfiles = notes.iter().filter(|n| !n.wins).flat_map(|n| n.files.iter()).collect::<HashSet<_>>().len();
                                            let who = if skipped_mods.len() == 1 {
                                                format!("{} is lower in the list, so the copy in that mod is used", skipped_mods[0])
                                            } else {
                                                format!("These mods are lower in the list, so the copy in each of them is used: {}", skipped_mods.join(", "))
                                            };
                                            let line = format!("{nfiles} shared {} in this mod {} skipped. {who}.", file_word(nfiles), be(nfiles));
                                            ui.label(RichText::new(line).small().color(BAD)).on_hover_text(skipped_files.join("\n"));
                                        }
                                    }
                                }
                            });
                        });
                    });
                });
                if box_changed {
                    self.mod_rows[i].on = on;
                    let folder = self.mod_rows[i].name.clone();
                    let keys: HashSet<String> = self.plugin_home.iter().filter(|(_, owned)| *owned == &folder).map(|(key, _)| key.clone()).collect();
                    // A plugin already in the game Data folder stays on when this folder is turned off.
                    let in_data: HashSet<String> = self.game().map(|g| keys.iter().filter(|name| g.data_file(name).is_some()).cloned().collect()).unwrap_or_default();
                    // Turning the folder off drops its other plugins from the list.
                    if on {
                        if let Some(home) = self.home.clone() {
                            let names: Vec<String> = plugins::plugins_in_tree(&home.mods().join(&folder)).into_iter().map(|(n, _)| n).collect();
                            if let Some(list) = &mut self.mods {
                                list.add_new(&names);
                                for entry in &mut list.entries {
                                    let key = entry.name.to_ascii_lowercase();
                                    if names.iter().any(|n| n.eq_ignore_ascii_case(&key)) {
                                        entry.active = true;
                                    }
                                }
                            }
                        }
                    } else if let Some(list) = &mut self.mods {
                        list.entries.retain(|entry| {
                            if entry.name.eq_ignore_ascii_case("FalloutNV.esm") {
                                return true;
                            }
                            let key = entry.name.to_ascii_lowercase();
                            !(keys.contains(&key) && !in_data.contains(&key))
                        });
                    }
                    self.mods_dirty = true;
                    self.recompute_clashes();
                }
                let resp = row.response;
                if resp.drag_started() && mod_find.is_empty() {
                    self.mod_drag = Some(i);
                }
                if self.mod_jump && shown {
                    ui.scroll_to_rect(resp.rect, Some(Align::Center));
                    self.mod_jump = false;
                }
                let _ = resp.clone().on_hover_cursor(egui::CursorIcon::Grab);
                rects.push(resp.rect);
            }
            if mod_find.is_empty() { if let Some(from) = self.mod_drag {
                let ctx = ui.ctx().clone();
                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                if let Some(pos) = ctx.pointer_latest_pos() {
                    let centers: Vec<f32> = rects.iter().map(|r| r.center().y).collect();
                    let gap = insertion_index(&centers, pos.y);
                    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
                        let y = if gap < rects.len() { rects[gap].top() - ui.spacing().item_spacing.y / 2.0 } else { last.bottom() + ui.spacing().item_spacing.y / 2.0 };
                        ui.painter().line_segment([egui::pos2(first.left(), y), egui::pos2(first.right(), y)], Stroke::new(2.0, GOLD));
                    }
                    let clip = ui.clip_rect();
                    let edge = 36.0;
                    if pos.y < clip.top() + edge {
                        ui.scroll_with_delta(Vec2::new(0.0, 8.0));
                    } else if pos.y > clip.bottom() - edge {
                        ui.scroll_with_delta(Vec2::new(0.0, -8.0));
                    }
                    ctx.request_repaint();
                    if !ctx.input(|i| i.pointer.any_down()) {
                        self.mod_drag = None;
                        let to = move_target(from, gap);
                        if to != from {
                            moved = Some((from, to));
                        }
                    }
                }
            }
            }
            if let Some((from, to)) = moved {
                let row = self.mod_rows.remove(from);
                self.mod_rows.insert(to, row);
                self.mods_dirty = true;
                self.recompute_clashes();
            }
        });
    }

    fn plugin_column(&mut self, ui: &mut egui::Ui) {
        self.profile_row(ui);
        self.backup_row(ui);
        let folders = self.plugin_folders.clone();
        let masters = self.plugin_masters.clone();
        let active = self.mods.as_ref().map(|l| l.entries.iter().filter(|e| e.active).count()).unwrap_or(0);
        // The number is how many plugins are on. There is no slot cap.
        ui.horizontal(|ui| {
            ui.label(RichText::new("PLUGINS").small().color(GOLD));
            ui.label(RichText::new(active.to_string()).small().color(WEAK)).on_hover_text("Plugins turned on.");
        });
        let Some(list) = &mut self.mods else {
            ui.label("No plugin list yet: set the game folder first, or start the game once.");
            return;
        };
        ui.add(egui::TextEdit::singleline(&mut self.plugin_find).hint_text("Find a plugin").desired_width(ui.available_width()));
        let find = self.plugin_find.trim().to_ascii_lowercase();
        let active_names: HashSet<String> = list.entries.iter().filter(|e| e.active).map(|e| e.name.to_ascii_lowercase()).collect();
        let present = self.mods_present.clone();
        let is_esm: Vec<bool> = list.entries.iter().map(|e| e.name.to_ascii_lowercase().ends_with(".esm")).collect();
        let n = list.entries.len();
        let is_base: Vec<bool> = list.entries.iter().map(|e| e.name.eq_ignore_ascii_case("FalloutNV.esm")).collect();
        let fixed = is_base.iter().take_while(|b| **b).count();
        let mut moved: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().id_salt("plugin-rows").auto_shrink([false, false]).show(ui, |ui| {
            let mut rects = Vec::with_capacity(n);
            let mut last_esm = false;
            let mut toggled = false;
            for (i, e) in list.entries.iter_mut().enumerate() {
                if !find.is_empty() && !e.name.to_ascii_lowercase().contains(&find) {
                    continue;
                }
                if last_esm && !is_esm[i] {
                    ui.separator();
                }
                last_esm = is_esm[i];
                let base = is_base[i];
                let dragged = self.mods_drag == Some(i);
                let fill = if dragged { theme::SELECTED } else { CARD };
                let stroke = if dragged { Stroke::new(1.0, theme::GOLD_DIM) } else { Stroke::NONE };
                let sense = if base { Sense::hover() } else { Sense::click_and_drag() };
                let mut name_rect = egui::Rect::NOTHING;
                let row = ui.scope_builder(egui::UiBuilder::new().id_salt(("mod-row", i)).sense(sense), |ui| {
                    let hot = !base && (dragged || ui.response().hovered());
                    Frame::new().fill(fill).stroke(stroke).corner_radius(4).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (grip, _) = ui.allocate_exact_size(Vec2::new(14.0, 18.0), Sense::hover());
                            if !base {
                                let c = if hot { GOLD } else { WEAK };
                                for k in [-4.0, 0.0, 4.0] {
                                    let y = grip.center().y + k;
                                    ui.painter().line_segment([egui::pos2(grip.left() + 1.0, y), egui::pos2(grip.right() - 1.0, y)], Stroke::new(1.5, c));
                                }
                            }
                            ui.label(RichText::new(format!("{:02X}", i)).monospace().color(WEAK));
                            if ui.add_enabled(!base, egui::Checkbox::without_text(&mut e.active)).changed() {
                                self.mods_dirty = true;
                                toggled = true;
                                self.picked_plugin = Some(e.name.to_ascii_lowercase());
                                self.mod_find.clear();
                                self.mod_jump = true;
                            }
                            name_rect = ui.label(&e.name).rect;
                            if let Some(folder) = self.plugin_home.get(&e.name.to_ascii_lowercase()) {
                                ui.label(RichText::new(format!("in {folder}")).small().color(WEAK));
                            }
                            let key = e.name.to_ascii_lowercase();
                            if !present.is_empty() && !present.contains(&key) {
                                ui.label(RichText::new("missing").small().color(BAD));
                            }
                            if let Some(have) = folders.get(&key).filter(|v| v.len() > 1) {
                                if let Some(win) = have.last() {
                                    let lost: Vec<&str> = have[..have.len() - 1].iter().map(String::as_str).collect();
                                    let skipped = if lost.len() == 1 {
                                        format!("The copy in {} is skipped.", lost[0])
                                    } else {
                                        format!("The copies in these mods are skipped: {}.", lost.join(", "))
                                    };
                                    ui.label(RichText::new(format!("The copy in {win} is used.")).small().color(GOOD)).on_hover_text(note_files(&self.folder_files, win, &e.name));
                                    ui.label(RichText::new(skipped).small().color(BAD)).on_hover_text(lost.iter().map(|loser| note_files(&self.folder_files, loser, &e.name)).collect::<Vec<_>>().join("\n"));
                                }
                            }
                            if e.active {
                                if let Some(need) = masters.get(&key) {
                                    let off: Vec<&str> = need.iter().filter(|m| !active_names.contains(&m.to_ascii_lowercase())).map(String::as_str).collect();
                                    if !off.is_empty() {
                                        ui.label(RichText::new(format!("needs {}", off.join(", "))).small().color(BAD));
                                    }
                                }
                            }
                        });
                    });
                });
                let resp = row.response;
                if !base {
                    if resp.drag_started() {
                        self.mods_drag = Some(i);
                    }
                    if resp.clicked() {
                        self.picked_plugin = Some(e.name.to_ascii_lowercase());
                        self.mod_find.clear();
                        self.mod_jump = true;
                    }
                    if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| name_rect.contains(p)) {
                        e.active = !e.active;
                        self.mods_dirty = true;
                        toggled = true;
                    }
                    let _ = resp.clone().on_hover_cursor(egui::CursorIcon::Grab);
                }
                rects.push(resp.rect);
            }
            if toggled {
                let homes = self.plugin_home.clone();
                let active_now: HashSet<String> = list.entries.iter().filter(|e| e.active).map(|e| e.name.to_ascii_lowercase()).collect();
                for row in &mut self.mod_rows {
                    let has = homes.values().any(|folder| folder == &row.name);
                    if !has {
                        continue;
                    }
                    row.on = homes.iter().any(|(plugin, folder)| folder == &row.name && active_now.contains(plugin));
                }
                let rows: Vec<(String, bool)> = self.mod_rows.iter().map(|r| (r.name.clone(), r.on)).collect();
                self.clashes = clash_notes(&rows, &self.folder_files);
            }
            if !find.is_empty() {
                return;
            }
            let Some(from) = self.mods_drag else { return };
            let ctx = ui.ctx().clone();
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let Some(pos) = ctx.pointer_latest_pos() else { return };
            let centers: Vec<f32> = rects.iter().map(|r| r.center().y).collect();
            let gap = insertion_index(&centers, pos.y).max(fixed);
            if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
                let y = if gap < rects.len() { rects[gap].top() - ui.spacing().item_spacing.y / 2.0 } else { last.bottom() + ui.spacing().item_spacing.y / 2.0 };
                ui.painter().line_segment([egui::pos2(first.left(), y), egui::pos2(first.right(), y)], Stroke::new(2.0, GOLD));
            }
            let clip = ui.clip_rect();
            let edge = 36.0;
            if pos.y < clip.top() + edge {
                ui.scroll_with_delta(Vec2::new(0.0, 8.0));
            } else if pos.y > clip.bottom() - edge {
                ui.scroll_with_delta(Vec2::new(0.0, -8.0));
            }
            ctx.request_repaint();
            if !ctx.input(|i| i.pointer.any_down()) {
                self.mods_drag = None;
                let to = move_target(from, gap);
                if to != from {
                    moved = Some((from, to));
                }
            }
        });
        if let Some((from, to)) = moved {
            let e = list.entries.remove(from);
            list.entries.insert(to, e);
            self.mods_dirty = true;
        }
    }

    /// Add a folder, a loose plugin, or an archive. An installer that needs a choice opens a window.
    fn mod_tools(&mut self, ui: &mut egui::Ui) {
        let row = ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.add_path)
                    .interactive(false)
                    .hint_text("Drop a folder or an archive")
                    .desired_width(ui.available_width() - 52.0),
            );
            if ui.button("Add").on_hover_text("Installs the dropped folder or archive. Nothing is copied into the game folder.").clicked() {
                let path = PathBuf::from(self.add_path.trim());
                match self.stage_one(&path) {
                    Ok(Some(name)) => self.finish_import(Ok(name), true),
                    Ok(None) => {
                        self.add_path.clear();
                        self.page = Page::Mods;
                        self.message = Some(("Choose what to install.".into(), false));
                    }
                    Err(e) => self.finish_import(Err(e), false),
                }
            }
        });
        self.mod_add_rect = row.response.rect;
    }

    fn installer_window(&mut self, ctx: &egui::Context) {
        let decision = {
            let Some(w) = self.wizard.as_mut() else { return };
            crate::fomod::draw(ctx, w)
        };
        match decision {
            Some(crate::fomod::Decision::Cancel) => {
                if let Some(w) = self.wizard.take() {
                    w.discard();
                }
            }
            Some(crate::fomod::Decision::Install) => {
                let Some(home) = self.home.clone() else { return };
                let mods = home.mods().to_path_buf();
                let result = self.wizard.as_mut().map(|w| w.install(&mods));
                match result {
                    Some(Ok(name)) => {
                        if let Some(w) = self.wizard.take() {
                            w.discard();
                        }
                        self.finish_import(Ok(name), false);
                    }
                    Some(Err(e)) => {
                        if let Some(w) = self.wizard.as_mut() {
                            w.error = Some(e);
                        }
                    }
                    None => {}
                }
            }
            None => {}
        }
    }

    fn install_facts(&self) -> crate::fomod::FactsInput {
        let active = self.mods.as_ref().map(|l| l.entries.iter().filter(|e| e.active).map(|e| e.name.clone()).collect()).unwrap_or_default();
        let mut paths = HashSet::new();
        for files in self.folder_files.values() {
            for file in files {
                paths.insert(file.clone());
            }
        }
        crate::fomod::FactsInput { active, present: self.mods_present.clone(), paths }
    }

    /// Plugins of a folder just added get a line. A new folder is on, so those plugins are on.
    /// Without this the folder shows in Mod Order and the plugin list does not change.
    fn adopt_new_plugins(&mut self) {
        let Some(home) = self.home.clone() else { return };
        self.mod_rows = load_mod_rows(&home);
        let mods = home.mods();
        let mut found = Vec::new();
        for row in &self.mod_rows {
            for (plugin, _) in plugins::plugins_in_tree(&mods.join(&row.name)) {
                found.push((plugin, row.on));
            }
        }
        if self.mods.is_none() {
            let text = std::fs::read_to_string(home.plugins_file()).unwrap_or_default();
            self.mods = Some(plugins::PluginList::parse(&text));
        }
        if let Some(list) = &mut self.mods {
            let names: Vec<String> = found.iter().map(|(n, _)| n.clone()).collect();
            let added = list.add_new(&names);
            for (plugin, on) in &found {
                if *on && added.iter().any(|n| n.eq_ignore_ascii_case(plugin)) {
                    if let Some(e) = list.entries.iter_mut().find(|e| e.name.eq_ignore_ascii_case(plugin)) {
                        e.active = true;
                    }
                }
            }
        }
        for (plugin, _) in &found {
            self.mods_present.insert(plugin.to_ascii_lowercase());
        }
        self.refresh_plugin_notes();
        self.mods_dirty = true;
    }

    fn finish_import(&mut self, result: Result<String, String>, clear_path: bool) {
        match result {
            Ok(name) => {
                if clear_path {
                    self.add_path.clear();
                }
                self.page = Page::Mods;
                self.adopt_new_plugins();
                self.message = Some((format!("Added {name}. Its plugins are in the list. Save to keep that."), false));
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    fn take_dropped(&mut self, ctx: &egui::Context) {
        let n = ctx.input(|i| i.raw.dropped_files.len());
        if n == 0 {
            return;
        }
        // A folder dropped on the Profiles box is a profile folder, not a mod to install.
        // A drop on the mod Add row only fills that path.
        let over_profile = ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| self.profile_rect.contains(p));
        let over_mod_add = ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| self.mod_add_rect.contains(p));
        if over_profile || over_mod_add {
            let path = ctx.input(|i| {
                i.raw.dropped_files.iter().find_map(|f| {
                    let path = f.path();
                    path.exists().then(|| path.to_path_buf())
                })
            });
            match path {
                Some(path) => {
                    if over_profile {
                        self.profile_path = path.display().to_string();
                        self.read_profiles();
                    } else {
                        self.add_path = path.display().to_string();
                        self.message = Some(("Path set. Add installs it.".into(), false));
                    }
                }
                None => self.message = Some(("That drop had no file path.".into(), true)),
            }
            return;
        }
        if self.wizard.is_some() {
            self.message = Some(("Finish the open installer first.".into(), true));
            return;
        }
        let mut sources = Vec::new();
        for i in 0..n {
            let path = ctx.input(|input| input.raw.dropped_files.get(i).map(|f| f.path().to_path_buf()));
            let Some(path) = path else { continue };
            if path.exists() {
                sources.push(Ok(path));
                continue;
            }
            let loaded = ctx.input(|input| input.raw.dropped_files.get(i).and_then(|f| f.bytes().ok()));
            match loaded {
                Some(bytes) if !bytes.is_empty() => {
                    let name = path.file_name().and_then(|n| n.to_str()).filter(|s| !s.is_empty() && !s.contains("..") && !s.contains('/') && !s.contains('\\')).unwrap_or("dropped.mod");
                    sources.push(write_dropped_bytes(name, &bytes));
                }
                _ => sources.push(Err("That drop had no file path.".into())),
            }
        }
        let mut added = Vec::new();
        let mut failed = None;
        let mut asking = false;
        for source in sources {
            let path = match source {
                Ok(path) => path,
                Err(e) => {
                    failed = Some(e);
                    continue;
                }
            };
            match self.stage_one(&path) {
                Ok(Some(name)) => added.push(name),
                Ok(None) => {
                    asking = true;
                    break;
                }
                Err(e) => failed = Some(e),
            }
        }
        if !added.is_empty() {
            self.page = Page::Mods;
            self.adopt_new_plugins();
        }
        if asking {
            self.page = Page::Mods;
        }
        if let Some(e) = failed {
            self.message = Some((e, true));
        } else if asking {
            self.message = Some(("Choose what to install.".into(), false));
        } else if !added.is_empty() {
            self.message = Some((format!("Added {}. Its plugins are in the list. Save to keep that.", added.join(", ")), false));
        }
    }

    fn stage_one(&mut self, src: &Path) -> Result<Option<String>, String> {
        if self.wizard.is_some() {
            return Err("Finish the open installer first.".into());
        }
        if src.as_os_str().is_empty() || !src.exists() {
            return Err("That path was not found.".into());
        }
        let home = self.home.clone().ok_or("no home folder")?;
        let mods = home.mods().to_path_buf();
        std::fs::create_dir_all(&mods).map_err(|e| format!("{}: {e}", mods.display()))?;
        if let Some(game) = self.game() {
            if let (Ok(s), Ok(g)) = (src.canonicalize(), game.root.canonicalize()) {
                if s == g || s.starts_with(&g) {
                    return Err("That path is in the game folder. It is not copied.".into());
                }
            }
        }
        if let Ok(mods_c) = mods.canonicalize() {
            if let Ok(s) = src.canonicalize() {
                if s == mods_c || s.starts_with(&mods_c) {
                    return Err("That folder is already in the mods folder.".into());
                }
            }
        }
        let facts = self.install_facts();
        match crate::fomod::stage(src, facts)? {
            crate::fomod::Staged::Tree { name, from, temp } => {
                let result = crate::fomod::place_tree(&mods, &name, &from);
                crate::fomod::discard(&temp);
                result.map(Some)
            }
            crate::fomod::Staged::Plan(plan) => {
                let temp = plan.temp.clone();
                let result = crate::fomod::place_plan(&mods, &plan);
                crate::fomod::discard(&temp);
                result.map(Some)
            }
            crate::fomod::Staged::Wizard(w) => {
                self.wizard = Some(w);
                Ok(None)
            }
        }
    }

    fn write_mod_order(&self) -> Result<(), String> {
        let home = self.home.as_ref().ok_or("no home folder")?;
        let path = home.root.join("mod-order.txt");
        let mut body = String::from("; A star means this folder is on.\n");
        for row in &self.mod_rows {
            if row.on {
                body.push('*');
            }
            body.push_str(&row.name);
            body.push('\n');
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn apply_mods(&mut self) -> Result<(), String> {
        self.save_mods()?;
        self.mods_dirty = false;
        Ok(())
    }

    /// A new game's start (src/start.rs, #2945): the start quest and intro film typed in,
    /// written to NVForever.ini's `[General]` by Save; TTW's values one click away while its
    /// master is in the load order.
    fn start_box(&mut self, ui: &mut egui::Ui) {
        use crate::start;
        let ttw = self.mods.as_ref().and_then(|l| l.entries.iter().find(|e| e.name.eq_ignore_ascii_case(start::TTW_PLUGIN))).map(|e| e.active);
        let saved = start::Fields::read(&self.ini_text);
        let mut save = false;
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("New game start").strong());
            ui.label(
                RichText::new("For mods that start somewhere else (their install guide gives the values). Empty: the New Vegas opening and its film. Saved to NVForever.ini's [General].")
                    .small()
                    .color(WEAK),
            );
            let w = (ui.available_width() - 120.0).max(120.0);
            egui::Grid::new("start_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Start quest");
                ui.add(egui::TextEdit::singleline(&mut self.start_edit.quest).hint_text("VCG00 (New Vegas opening); TTW: 001FFFF8").desired_width(w));
                ui.end_row();
                ui.label("Intro film");
                ui.add_enabled(!self.start_edit.no_film, egui::TextEdit::singleline(&mut self.start_edit.film).hint_text(if cfg!(windows) { r"fnvintro.bik (a file in Data\Video)" } else { "fnvintro.bik (a file in Data/Video)" }).desired_width(w));
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.start_edit.no_film, "No intro film");
                ui.end_row();
            });
            ui.horizontal(|ui| {
                save = ui.add_enabled(self.start_edit != saved, egui::Button::new("Save start")).clicked();
                if ttw.is_some() && !self.start_edit.is_ttw() && ui.button("Use Tale of Two Wastelands' start").on_hover_text("Start quest 001FFFF8 and no intro film; Save to keep it").clicked() {
                    self.start_edit = start::ttw();
                }
                if self.start_edit != start::Fields::default() && ui.button("Clear").on_hover_text("Back to the New Vegas opening; Save to keep it").clicked() {
                    self.start_edit = start::Fields::default();
                }
                if self.start_edit != saved && ui.button("Undo").clicked() {
                    self.start_edit = saved.clone();
                }
            });
            if self.start_edit.is_ttw() && ttw == Some(false) {
                ui.label(RichText::new(format!("{} is off in the list below: turn it on, or this start quest is not found.", start::TTW_PLUGIN)).small().color(BAD));
            }
            let quest = self.start_edit.quest.trim();
            if ttw == Some(true) && !self.start_edit.is_ttw() && (quest.is_empty() || quest.eq_ignore_ascii_case("VCG00")) {
                ui.label(RichText::new("Tale of Two Wastelands is on, and a new game still starts at the New Vegas opening.").small().color(BAD));
            }
        });
        if save {
            let fields = self.start_edit.clone();
            // Only the INI's text is read again: unsaved changes on the Settings page stay.
            match self.write_ini(|t| fields.write(&t)) {
                Ok(()) => {
                    self.ini_text = self.ini_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
                    self.start_edit = start::Fields::read(&self.ini_text);
                    let what = if self.start_edit == start::Fields::default() { "New games start at the New Vegas opening." } else { "New game start saved." };
                    self.message = Some((what.to_string(), false));
                }
                Err(e) => self.message = Some((e, true)),
            }
        }
    }

    fn keep_plugin_backup(&self, path: &Path, next: &[u8]) -> Result<(), String> {
        let keep = self.backup_keep;
        if keep == 0 || !path.is_file() {
            return Ok(());
        }
        let old = std::fs::read(path).unwrap_or_default();
        if old.is_empty() || old.as_slice() == next {
            return Ok(());
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("plugins.txt");
        let bak = |i: usize| path.with_file_name(format!("{name}.bak.{i}"));
        if bak(1).is_file() && std::fs::read(bak(1)).ok().as_deref() == Some(old.as_slice()) {
            return Ok(());
        }
        for i in (1..keep).rev() {
            let from = bak(i);
            if from.is_file() {
                std::fs::rename(&from, bak(i + 1)).map_err(|e| format!("{}: {e}", from.display()))?;
            }
        }
        for i in (keep + 1)..(keep + 40) {
            let extra = bak(i);
            if extra.is_file() {
                let _ = std::fs::remove_file(&extra);
            }
        }
        std::fs::write(bak(1), &old).map_err(|e| format!("{}: {e}", bak(1).display()))
    }

    fn store_backup(&self, path: &Path, body: &[u8]) -> Result<bool, String> {
        let keep = self.backup_keep;
        if keep == 0 {
            return Err("Set Max backups kept above 0.".into());
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("plugins.txt");
        let bak = |i: usize| path.with_file_name(format!("{name}.bak.{i}"));
        if let Some(parent) = bak(1).parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        if bak(1).is_file() && std::fs::read(bak(1)).ok().as_deref() == Some(body) {
            return Ok(false);
        }
        for i in (1..keep).rev() {
            let from = bak(i);
            if from.is_file() {
                std::fs::rename(&from, bak(i + 1)).map_err(|e| format!("{}: {e}", from.display()))?;
            }
        }
        for i in (keep + 1)..(keep + 40) {
            let extra = bak(i);
            if extra.is_file() {
                let _ = std::fs::remove_file(&extra);
            }
        }
        std::fs::write(bak(1), body).map_err(|e| format!("{}: {e}", bak(1).display()))?;
        Ok(true)
    }

    fn save_order_backup(&self) -> Result<&'static str, String> {
        if self.backup_keep == 0 {
            return Err("Set Backups above 0.".into());
        }
        let home = self.home.as_ref().ok_or("no home folder")?;
        let list = self.mods.as_ref().ok_or("no plugin list")?;
        let mut order = String::from("; A star means this folder is on.\n");
        for row in &self.mod_rows {
            if row.on {
                order.push('*');
            }
            order.push_str(&row.name);
            order.push('\n');
        }
        let plugins_new = self.store_backup(&home.plugins_file(), list.to_text().as_bytes())?;
        let order_new = self.store_backup(&home.root.join("mod-order.txt"), order.as_bytes())?;
        if plugins_new || order_new { Ok("Backup saved.") } else { Ok("That backup is already the newest.") }
    }

    fn profile_row(&mut self, ui: &mut egui::Ui) {
        let block = ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Profiles").small().color(GOLD));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.profile_path)
                        .interactive(false)
                        .hint_text("Drop a profile folder")
                        .desired_width(ui.available_width() - 52.0),
                );
                if ui.button("Add").on_hover_text("Saves this page's folder order and plugin order as a new profile. Nothing is copied.").clicked() {
                    self.save_new_profile();
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Read").on_hover_text("Finds profiles in that folder.").clicked() {
                    self.read_profiles();
                }
                let shown = self.profile_pick.clone().unwrap_or_else(|| {
                    if self.profile_choices.is_empty() { "no profiles".into() } else { "choose a profile".into() }
                });
                egui::ComboBox::from_id_salt("profile-which").width(140.0).selected_text(shown).show_ui(ui, |ui| {
                    for (name, _) in &self.profile_choices {
                        if ui.selectable_label(self.profile_pick.as_deref() == Some(name.as_str()), name).clicked() {
                            self.profile_pick = Some(name.clone());
                        }
                    }
                });
                if ui.add_enabled(self.profile_pick.is_some(), egui::Button::new("Load")).on_hover_text("Puts that profile's folder order and plugin order into this page. Files stay where they are.").clicked() {
                    self.load_chosen_profile();
                }
            });
        });
        self.profile_rect = block.response.rect;
    }

    fn read_profiles(&mut self) {
        let path = PathBuf::from(self.profile_path.trim());
        if path.as_os_str().is_empty() || !path.exists() {
            self.message = Some(("That folder was not found.".into(), true));
            return;
        }
        let found = Self::profile_folders(&path);
        if found.is_empty() {
            self.profile_choices.clear();
            self.profile_pick = None;
            self.message = Some(("That folder has no profiles.".into(), true));
            return;
        }
        self.profile_pick = found.first().map(|(n, _)| n.clone());
        let n = found.len();
        self.profile_choices = found;
        self.message = Some((format!("Found {n} profiles."), false));
    }

    /// Writes the current folder order and plugin order as a new profile. Files are not copied. The game folder is not changed.
    fn save_new_profile(&mut self) {
        let Some(home) = self.home.clone() else {
            self.message = Some(("No home folder yet.".into(), true));
            return;
        };
        if self.mod_rows.is_empty() && self.mods.as_ref().is_none_or(|list| list.entries.is_empty()) {
            self.message = Some(("Nothing to save.".into(), true));
            return;
        }
        let profiles = home.root.join("profiles");
        let dropped = PathBuf::from(self.profile_path.trim());
        let game_root = self.game().map(|g| g.root);
        let (parent, left_mods) = if dropped.is_dir() {
            let folder = if dropped.join("modlist.txt").is_file() {
                dropped.parent().map(Path::to_path_buf).unwrap_or_else(|| dropped.clone())
            } else {
                dropped
            };
            if game_root.as_ref().is_some_and(|root| self.dir_is_under(&folder, root)) {
                self.message = Some(("That folder is in the game folder. The profile was not written.".into(), true));
                return;
            }
            if self.dir_is_under(&folder, &home.mods()) {
                (profiles, true)
            } else {
                (folder, false)
            }
        } else {
            (profiles, false)
        };
        if let Err(e) = std::fs::create_dir_all(&parent) {
            self.message = Some((format!("{}: {e}", parent.display()), true));
            return;
        }
        let name = Self::fresh_profile_name(&parent);
        let dir = parent.join(&name);
        if let Err(e) = Self::write_profile_folder(&dir, &self.mod_rows, self.mods.as_ref()) {
            self.message = Some((e, true));
            return;
        }
        self.profile_path = parent.display().to_string();
        self.profile_choices = Self::profile_folders(&parent);
        self.profile_pick = Some(name.clone());
        let note = if left_mods { " The mods folder was left alone." } else { "" };
        self.message = Some((format!("Saved profile {name}. Nothing was copied.{note}"), false));
    }

    fn dir_is_under(&self, path: &Path, root: &Path) -> bool {
        if root.as_os_str().is_empty() {
            return false;
        }
        match (path.canonicalize(), root.canonicalize()) {
            (Ok(path), Ok(root)) => path == root || path.starts_with(&root),
            _ => false,
        }
    }

    fn load_chosen_profile(&mut self) {
        let Some(name) = self.profile_pick.clone() else {
            self.message = Some(("Choose a profile.".into(), true));
            return;
        };
        let Some(dir) = self.profile_choices.iter().find(|(n, _)| n == &name).map(|(_, p)| p.clone()) else {
            self.message = Some(("Choose a profile.".into(), true));
            return;
        };
        let Some(game) = self.game() else {
            self.message = Some(("Set the game folder first. Nothing was linked.".into(), true));
            return;
        };
        let mods_text = std::fs::read_to_string(dir.join("modlist.txt")).unwrap_or_default();
        let plugins_text = std::fs::read_to_string(dir.join("plugins.txt")).unwrap_or_default();
        let order_text = std::fs::read_to_string(dir.join("loadorder.txt")).unwrap_or_default();
        if mods_text.trim().is_empty() && plugins_text.trim().is_empty() && order_text.trim().is_empty() {
            self.message = Some(("That profile has no folder list and no plugin list.".into(), true));
            return;
        }
        let profile_mods = Self::profile_mod_rows(&mods_text);
        let home_mods = self.home.as_ref().map(|h| h.mods());
        let mut placed = Vec::new();
        let mut linked = 0usize;
        let mut skipped = 0usize;
        let mut missing = 0usize;
        for (mod_name, on) in &profile_mods {
            let Some(src) = Self::find_mod_dir(&dir, mod_name, home_mods.as_deref()) else {
                missing += 1;
                continue;
            };
            if let Some(mods) = &home_mods {
                Self::ensure_mod_link(mods, mod_name, &src);
            }
            if *on {
                let (n, s) = Self::symlink_mod_into_data(&src, &game.data);
                linked += n;
                skipped += s;
            } else {
                Self::unlink_data_symlinks_into(&game.data, &src);
            }
            placed.push(ModRow { name: mod_name.clone(), on: *on });
        }
        if !placed.is_empty() {
            let mut rows = Vec::new();
            for row in &self.mod_rows {
                if !placed.iter().any(|p| p.name.eq_ignore_ascii_case(&row.name)) {
                    rows.push(ModRow { name: row.name.clone(), on: row.on });
                }
            }
            rows.extend(placed);
            self.mod_rows = rows;
        }
        let active = Self::profile_plugin_names(&plugins_text);
        let mut order = Self::profile_plugin_names(&order_text);
        if order.is_empty() {
            order = active.clone();
        }
        let active_keys: HashSet<String> = active.iter().map(|n| n.to_ascii_lowercase()).collect();
        if !order.is_empty() || !active.is_empty() {
            let mut entries = Vec::new();
            let mut seen = HashSet::new();
            if !order.iter().any(|n| n.eq_ignore_ascii_case("FalloutNV.esm")) {
                entries.push(plugins::PluginEntry { name: "FalloutNV.esm".into(), active: true });
                seen.insert("falloutnv.esm".to_string());
            }
            for plugin in &order {
                let key = plugin.to_ascii_lowercase();
                if !seen.insert(key.clone()) {
                    continue;
                }
                let on = plugin.eq_ignore_ascii_case("FalloutNV.esm") || active_keys.contains(&key);
                entries.push(plugins::PluginEntry { name: plugin.clone(), active: on });
            }
            if let Some(list) = &self.mods {
                for entry in &list.entries {
                    if seen.insert(entry.name.to_ascii_lowercase()) {
                        entries.push(plugins::PluginEntry { name: entry.name.clone(), active: false });
                    }
                }
            }
            self.mods = Some(plugins::PluginList { entries });
        }
        self.mods_dirty = true;
        self.refresh_plugin_notes();
        self.recompute_clashes();
        let folders = self.mod_rows.iter().filter(|r| profile_mods.iter().any(|(n, _)| n.eq_ignore_ascii_case(&r.name))).count();
        self.message = Some((format!("Loaded {name}: {folders} folders, {linked} files linked into Data, {skipped} left in place, {missing} not found. Nothing was copied. Save to keep the order."), false));
    }

    /// Links each turned-on mod that is one mod, not a folder of mods, into the game Data folder. Files are never copied.
    fn link_on_rows_into_data(&self) {
        let Some(game) = self.game() else { return };
        let Some(home) = &self.home else { return };
        let mods = home.mods();
        for row in &self.mod_rows {
            let path = mods.join(&row.name);
            if !path.is_dir() || !Self::looks_like_one_mod(&path) {
                continue;
            }
            if row.on {
                Self::symlink_mod_into_data(&path, &game.data);
            } else {
                Self::unlink_data_symlinks_into(&game.data, &path);
            }
        }
    }

    /// The mod folder named `name`: beside the profile, in the mods folder, or inside a folder there.
    fn find_mod_dir(profile_dir: &Path, name: &str, home_mods: Option<&Path>) -> Option<PathBuf> {
        let mut found = Vec::new();
        if let Some(mods) = home_mods {
            found.push(mods.join(name));
            if let Ok(rd) = std::fs::read_dir(mods) {
                for entry in rd.flatten() {
                    found.push(entry.path().join(name));
                }
            }
        }
        if let Some(instance) = profile_dir.parent().and_then(|p| p.parent()) {
            if let Some(dir) = Self::instance_subdir(instance, "mod_directory", "mods") {
                found.push(dir.join(name));
            }
            found.push(instance.join("mods").join(name));
        }
        found.into_iter().find(|p| p.is_dir())
    }

    fn instance_subdir(instance: &Path, key: &str, default: &str) -> Option<PathBuf> {
        let rd = std::fs::read_dir(instance).ok()?;
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Some(raw) = Self::ini_setting(&text, "Settings", key) else { continue };
            let raw = raw.replace("%BASE_DIR%", &instance.to_string_lossy());
            let dir = PathBuf::from(&raw);
            let dir = if dir.is_absolute() { dir } else { instance.join(dir) };
            if dir.is_dir() {
                return Some(dir);
            }
        }
        let fallback = instance.join(default);
        fallback.is_dir().then_some(fallback)
    }

    /// Makes `mods/name` a link to `src` when that name is not already there.
    fn ensure_mod_link(mods: &Path, name: &str, src: &Path) {
        let dest = mods.join(name);
        if dest.exists() || !src.is_dir() {
            return;
        }
        // The mods folder has to resolve before a link is made.
        if std::fs::canonicalize(mods).is_err() {
            return;
        }
        let Ok(target) = std::fs::canonicalize(src) else { return };
        let _ = std::os::unix::fs::symlink(&target, &dest);
    }

    /// Links the mod's files into `data`. A real file already there is left in place. Nothing is copied.
    /// Returns how many links were made, and how many files were already there.
    fn symlink_mod_into_data(src: &Path, data: &Path) -> (usize, usize) {
        let root = if src.join("Data").is_dir() { src.join("Data") } else { src.to_path_buf() };
        let mut linked = 0usize;
        let mut skipped = 0usize;
        let mut seen = HashSet::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let key = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
            if !seen.insert(key) {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if path.is_dir() {
                    if matches!(name.as_str(), "fomod" | "meta" | "optional" | "docs" | "source" | "__macosx") {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                if !path.is_file() {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&root) else { continue };
                let dest = data.join(rel);
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(meta) = std::fs::symlink_metadata(&dest) {
                    if meta.file_type().is_symlink() {
                        let _ = std::fs::remove_file(&dest);
                    } else {
                        skipped += 1;
                        continue;
                    }
                }
                let target = std::fs::canonicalize(&path).unwrap_or(path);
                if std::os::unix::fs::symlink(&target, &dest).is_ok() {
                    linked += 1;
                }
            }
        }
        (linked, skipped)
    }

    /// Removes links in `data` that point into `src`. Real files are left alone.
    fn unlink_data_symlinks_into(data: &Path, src: &Path) {
        let Ok(src) = std::fs::canonicalize(src) else { return };
        let mut stack = vec![data.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
                if meta.file_type().is_symlink() {
                    if std::fs::canonicalize(&path).is_ok_and(|t| t.starts_with(&src)) {
                        let _ = std::fs::remove_file(&path);
                    }
                    continue;
                }
                if meta.is_dir() {
                    stack.push(path);
                }
            }
        }
    }

    /// One mod has plugins or game folders of its own, and is not a folder full of other mods.
    fn looks_like_one_mod(path: &Path) -> bool {
        let root = if path.join("Data").is_dir() { path.join("Data") } else { path.to_path_buf() };
        let Ok(rd) = std::fs::read_dir(&root) else { return false };
        let mut game_file = false;
        let mut nested = 0usize;
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let child = entry.path();
            if child.is_file() && (name.ends_with(".esp") || name.ends_with(".esm") || name.ends_with(".bsa")) {
                game_file = true;
            }
            if child.is_dir() && matches!(name.as_str(), "meshes" | "textures" | "sound" | "music" | "video" | "menus" | "interface" | "shaders" | "nvse") {
                game_file = true;
            }
            if child.is_dir() && (child.join("Data").is_dir() || Self::dir_has_plugin(&child)) {
                nested += 1;
            }
        }
        game_file && nested < 2
    }

    fn dir_has_plugin(path: &Path) -> bool {
        std::fs::read_dir(path).ok().into_iter().flatten().flatten().any(|e| {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            e.path().is_file() && (n.ends_with(".esp") || n.ends_with(".esm"))
        })
    }

    fn profile_folders(root: &Path) -> Vec<(String, PathBuf)> {
        let mut dirs = vec![root.to_path_buf(), root.join("profiles")];
        if let Some(extra) = Self::profiles_dir_from_settings(root) {
            dirs.push(extra);
        }
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        for dir in dirs {
            if dir.join("modlist.txt").is_file() && seen.insert(dir.clone()) {
                let label = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "profile".into());
                found.push((label, dir));
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("modlist.txt").is_file() && seen.insert(path.clone()) {
                    found.push((entry.file_name().to_string_lossy().into_owned(), path));
                }
            }
        }
        found.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
        found
    }

    fn profiles_dir_from_settings(root: &Path) -> Option<PathBuf> {
        let rd = std::fs::read_dir(root).ok()?;
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Some(raw) = Self::ini_setting(&text, "Settings", "profiles_directory") else { continue };
            let raw = raw.replace("%BASE_DIR%", &root.to_string_lossy());
            let dir = PathBuf::from(&raw);
            let dir = if dir.is_absolute() { dir } else { root.join(dir) };
            if dir.is_dir() {
                return Some(dir);
            }
        }
        None
    }

    fn ini_setting(text: &str, section: &str, key: &str) -> Option<String> {
        let mut here = false;
        for line in text.lines() {
            let t = line.trim();
            if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                here = name.eq_ignore_ascii_case(section);
                continue;
            }
            if !here {
                continue;
            }
            let Some((k, v)) = t.split_once('=') else { continue };
            if !k.trim().eq_ignore_ascii_case(key) {
                continue;
            }
            let v = v.trim();
            let v = v.strip_prefix("@ByteArray(").and_then(|s| s.strip_suffix(')')).unwrap_or(v);
            return Some(v.replace("\\\\", "\\"));
        }
        None
    }

    /// Enabled and disabled folders from a profile list. A line starting with `+` is on, and `-` is off.
    /// The file names the winning folder first. This page keeps the winning folder last.
    fn profile_mod_rows(text: &str) -> Vec<(String, bool)> {
        let mut rows = Vec::new();
        for line in text.lines() {
            let line = line.trim_end().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (on, name) = if let Some(n) = line.strip_prefix('+') {
                (true, n)
            } else if let Some(n) = line.strip_prefix('-') {
                (false, n)
            } else {
                continue;
            };
            if name.is_empty() || name.ends_with("_separator") {
                continue;
            }
            rows.push((name.to_string(), on));
        }
        rows.reverse();
        rows
    }

    fn profile_plugin_names(text: &str) -> Vec<String> {
        let mut names = Vec::new();
        for line in text.lines() {
            let t = line.trim().trim_start_matches('\u{feff}');
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let name = t.strip_prefix('*').unwrap_or(t).trim();
            if name.is_empty() || names.iter().any(|n: &String| n.eq_ignore_ascii_case(name)) {
                continue;
            }
            names.push(name.to_string());
        }
        names
    }

    /// The next free profile name in `parent`.
    fn fresh_profile_name(parent: &Path) -> String {
        let mut names = vec!["Profile".to_string()];
        for n in 2..1000 {
            names.push(format!("Profile {n}"));
        }
        names.into_iter().find(|name| !parent.join(name).exists()).unwrap_or_else(|| format!("Profile {}", std::process::id()))
    }

    /// Writes one profile folder. The winning folder is first in the file. This page keeps that folder last.
    fn write_profile_folder(dir: &Path, rows: &[ModRow], list: Option<&plugins::PluginList>) -> Result<(), String> {
        if dir.exists() {
            return Err(format!("{} already exists.", dir.display()));
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let modlist = Self::modlist_text(rows);
        let (plugins_text, order_text) = Self::plugin_list_text(list);
        for (name, body) in [("modlist.txt", modlist), ("plugins.txt", plugins_text), ("loadorder.txt", order_text)] {
            let path = dir.join(name);
            std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }

    fn modlist_text(rows: &[ModRow]) -> String {
        let mut body = String::new();
        for row in rows.iter().rev() {
            if row.name.is_empty() || row.name.contains('/') || row.name.contains('\\') || row.name.ends_with("_separator") {
                continue;
            }
            body.push(if row.on { '+' } else { '-' });
            body.push_str(&row.name);
            body.push('\n');
        }
        body
    }

    fn plugin_list_text(list: Option<&plugins::PluginList>) -> (String, String) {
        let mut active = String::new();
        let mut order = String::new();
        let Some(list) = list else { return (active, order) };
        for entry in &list.entries {
            if entry.name.is_empty() || entry.name.contains('/') || entry.name.contains('\\') {
                continue;
            }
            order.push_str(&entry.name);
            order.push('\n');
            if entry.active {
                active.push('*');
                active.push_str(&entry.name);
                active.push('\n');
            }
        }
        (active, order)
    }

    fn backup_row(&mut self, ui: &mut egui::Ui) {
        let Some(home) = self.home.clone() else { return };
        if self.backup_keep != self.backup_max as usize {
            self.backup_keep = self.backup_max as usize;
            self.set_launcher_key("PluginBackups", &self.backup_max.to_string());
        }
        let files = plugin_backups(&home);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Max backups kept").small().color(WEAK))
                .on_hover_text("The most load-order copies to keep. Not how many exist now.");
            egui::ComboBox::from_id_salt("plugin-backup-max").selected_text(self.backup_max.to_string()).show_ui(ui, |ui| {
                for n in BACKUP_MAX {
                    if ui.selectable_label(self.backup_max == *n, n.to_string()).clicked() && self.backup_max != *n {
                        match write_backup_max(&home, *n) {
                            Ok(()) => {
                                self.backup_max = *n;
                                self.backup_keep = *n as usize;
                                self.set_launcher_key("PluginBackups", &n.to_string());
                                prune_plugin_backups(&home, *n);
                                self.backup_pick = self.backup_pick.take().filter(|pick| plugin_backups(&home).iter().any(|p| backup_label(p) == *pick));
                                self.message = Some((format!("At most {n} load-order backups are kept."), false));
                            }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                }
            });
            let shown = self.backup_pick.clone().unwrap_or_else(|| if files.is_empty() { "no backups".into() } else { "choose a backup".into() });
            egui::ComboBox::from_id_salt("plugin-backup-which").selected_text(shown).show_ui(ui, |ui| {
                for path in &files {
                    let label = backup_label(path);
                    if ui.selectable_label(self.backup_pick.as_deref() == Some(label.as_str()), &label).clicked() {
                        self.backup_pick = Some(label);
                    }
                }
            });
            if ui.add_enabled(self.backup_pick.is_some(), egui::Button::new("Load backup")).clicked() {
                if let Some(label) = self.backup_pick.clone() {
                    let path = backup_dir(&home).join(format!("plugins-{label}.txt"));
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            self.mods = Some(plugins::PluginList::parse(&text));
                            self.mods_dirty = true;
                            self.message = Some(("Backup loaded into the list. Save to keep it.".into(), false));
                        }
                        Err(e) => self.message = Some((e.to_string(), true)),
                    }
                }
            }
            let can_backup = self.backup_keep > 0 && self.mods.is_some();
            let tip = if self.backup_keep == 0 { "Set Max backups kept above 0." } else { "Stores this mod order and plugin list. The lists the game uses stay as they are." };
            if ui.add_enabled(can_backup, egui::Button::new("Save backup")).on_hover_text(tip).clicked() {
                match self.save_order_backup() {
                    Ok(said) => self.message = Some((said.into(), false)),
                    Err(e) => self.message = Some((e, true)),
                }
            }
        });
    }

    fn save_mods(&mut self) -> Result<(), String> {
        self.link_on_rows_into_data();
        let (Some(home), Some(list)) = (&self.home, &self.mods) else { return Err("no plugin list".into()) };
        let path = home.plugins_file();
        let next = list.to_text();
        let previous = std::fs::read(&path).unwrap_or_default();
        if previous.as_slice() != next.as_bytes() {
            keep_plugin_backup(home, &previous, self.backup_max)?;
        }
        guard_write(&path, self.game().as_ref()).map_err(|e| e.to_string())?;
        let body = list.to_text();
        self.keep_plugin_backup(&path, body.as_bytes())?;
        write_whole(&path, body.as_bytes()).map_err(|e| e.to_string())?;
        self.write_mod_order()
    }

    fn game_folders() -> Vec<(PathBuf, &'static str)> {
        fn add_game(path: &Path, store: &'static str, found: &mut Vec<(PathBuf, &'static str)>, seen: &mut HashSet<PathBuf>) {
            if GameInstall::open(path).is_err() {
                return;
            }
            let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if seen.insert(key) {
                found.push((path.to_path_buf(), store));
            }
        }
        fn consider(path: &Path, store: &'static str, found: &mut Vec<(PathBuf, &'static str)>, seen: &mut HashSet<PathBuf>) {
            if !path.is_dir() {
                return;
            }
            if GameInstall::open(path).is_ok() {
                add_game(path, store, found, seen);
                return;
            }
            let Ok(rd) = std::fs::read_dir(path) else { return };
            for entry in rd.flatten() {
                let child = entry.path();
                if child.is_dir() {
                    add_game(&child, store, found, seen);
                }
            }
        }
        fn paths_in_json(text: &str) -> Vec<PathBuf> {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
            fn walk(value: &serde_json::Value, out: &mut Vec<PathBuf>) {
                match value {
                    serde_json::Value::Object(map) => {
                        for key in ["install_path", "installPath", "install_dir"] {
                            if let Some(serde_json::Value::String(path)) = map.get(key) {
                                if !path.is_empty() {
                                    out.push(PathBuf::from(path));
                                }
                            }
                        }
                        for child in map.values() {
                            walk(child, out);
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for child in items {
                            walk(child, out);
                        }
                    }
                    _ => {}
                }
            }
            let mut out = Vec::new();
            walk(&value, &mut out);
            out
        }
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        let Some(home_os) = std::env::var_os("HOME") else { return found };
        let home = PathBuf::from(home_os);
        let mut parents: Vec<(PathBuf, &'static str)> = Vec::new();
        for rel in [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/data/Steam"] {
            let steam = home.join(rel);
            if !steam.is_dir() {
                continue;
            }
            parents.push((steam.join("steamapps/common"), "Steam"));
            if let Ok(text) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
                for line in text.lines() {
                    let Some(rest) = line.trim().strip_prefix("\"path\"") else { continue };
                    let rest = rest.trim().trim_matches('"');
                    if !rest.is_empty() {
                        parents.push((PathBuf::from(rest).join("steamapps/common"), "Steam"));
                    }
                }
            }
        }
        for rel in ["GOG Games", ".wine/drive_c/GOG Games", ".wine/drive_c/Program Files (x86)/GOG Galaxy/Games"] {
            parents.push((home.join(rel), "GOG"));
        }
        for rel in [".wine/drive_c/Program Files/Epic Games", ".wine/drive_c/Program Files (x86)/Epic Games"] {
            parents.push((home.join(rel), "Epic"));
        }
        for rel in ["Games", "Games/Heroic"] {
            let root = home.join(rel);
            let Ok(rd) = std::fs::read_dir(&root) else { continue };
            for entry in rd.flatten() {
                let child = entry.path();
                if !child.is_dir() {
                    continue;
                }
                let store = match child.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase() {
                    n if n.contains("epic") => "Epic",
                    n if n.contains("gog") => "GOG",
                    n if n.contains("steam") => "Steam",
                    _ => "Heroic",
                };
                parents.push((child, store));
            }
        }
        for (rel, store) in [
            (".config/legendary/installed.json", "Epic"),
            (".config/heroic/legendaryConfig/legendary/installed.json", "Epic"),
            (".var/app/com.heroicgameslauncher.hgl/config/heroic/legendaryConfig/legendary/installed.json", "Epic"),
            (".config/heroic/gog_store/installed.json", "GOG"),
            (".var/app/com.heroicgameslauncher.hgl/config/heroic/gog_store/installed.json", "GOG"),
            (".config/minigalaxy/config.json", "GOG"),
        ] {
            if let Ok(text) = std::fs::read_to_string(home.join(rel)) {
                for folder in paths_in_json(&text) {
                    parents.push((folder, store));
                }
            }
        }
        for (parent, store) in parents {
            consider(&parent, store, &mut found, &mut seen);
        }
        found
    }

    fn game_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Game folder");
        ui.label(RichText::new("Your Fallout: New Vegas folder, the one holding Data. NVForever only reads it.").small().color(WEAK));
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let reserve = if cfg!(windows) { 190.0 } else { 360.0 };
                ui.add(egui::TextEdit::singleline(&mut self.game_edit).desired_width(ui.available_width() - reserve));
                if cfg!(windows) && ui.button("Browse…").clicked() {
                    if let Some(d) = pick_folder("Your Fallout: New Vegas folder") {
                        self.game_edit = d.display().to_string();
                    }
                }
                if !cfg!(windows) && ui.button("Find game folder").clicked() {
                    match Self::game_folders().as_slice() {
                        [] => self.message = Some(("No Fallout: New Vegas folder in Steam, GOG, or Epic.".into(), true)),
                        [one] => {
                            self.game_edit = one.0.display().to_string();
                            self.message = Some((format!("Filled in {} from {}.", one.0.display(), one.1), false));
                        }
                        many => {
                            let pick = many.iter().find(|p| {
                                p.0.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                                    let n = n.to_ascii_lowercase();
                                    n.contains("vegas") || n.contains("fallout")
                                })
                            }).unwrap_or(&many[0]);
                            self.game_edit = pick.0.display().to_string();
                            self.message = Some((format!("Found {}. Filled in {} from {}.", many.len(), pick.0.display(), pick.1), false));
                        }
                    }
                }
                if ui.button(RichText::new("Save").strong()).clicked() {
                    self.save_game_folder();
                }
            });
            match GameInstall::open(self.game_edit.trim()) {
                Ok(g) => {
                    let missing = g.missing_files();
                    if missing.is_empty() {
                        ui.label(RichText::new("Every game file NVForever needs is there.").color(GOOD));
                    } else {
                        ui.label(RichText::new(format!("Missing from Data: {}", missing.join(", "))).color(BAD));
                    }
                    let addons = g.addon_masters();
                    ui.label(RichText::new(format!("Add-ons found: {}", if addons.is_empty() { "none".to_string() } else { addons.join(", ") })).small().color(WEAK));
                }
                Err(e) if !self.game_edit.trim().is_empty() => {
                    ui.label(RichText::new(e.to_string()).color(BAD));
                }
                Err(_) => {}
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("PROGRAM").small().strong().color(theme::GOLD_TEXT));
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            match &self.exe {
                Some(p) => ui.label(format!("Starts {}", p.display())),
                None => ui.label(RichText::new(format!("{} was not found beside the launcher.", launch::GAME_EXE)).color(BAD)),
            };
            if cfg!(windows) && ui.button("Choose the game program…").clicked() {
                if let Some(f) = pick_file("nvforever") {
                    self.set_launcher_key("Exe", &f.display().to_string());
                }
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("VERSION").small().strong().color(theme::GOLD_TEXT));
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(format!("NVForever v{}", version()));
            let install = install_dir(self.game().as_ref());
            let kept = install.as_ref().ok().and_then(|d| update::previous_version(d));
            match (&install, kept) {
                (Ok(dir), Some((v, _))) => {
                    let busy = !matches!(self.upd, Upd::Idle | Upd::Failed(_));
                    let b = ui.add_enabled(!busy, egui::Button::new(format!("Roll back to v{v}")));
                    if b.on_hover_text("Put the version the last update replaced back (this one is kept in its place). Saves, settings and mods are not touched.").clicked() {
                        self.roll_back(dir.clone(), ui.ctx());
                    }
                }
                (Ok(_), None) => {
                    ui.label(RichText::new("No earlier version is kept: an update keeps the one it replaces, to roll back to.").small().color(WEAK));
                }
                (Err(e), _) => {
                    ui.label(RichText::new(format!("Updates: {e}")).small().color(WEAK));
                }
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("FOLDERS").small().strong().color(theme::GOLD_TEXT));
        if let Some(h) = self.home.clone() {
            ui.horizontal_wrapped(|ui| {
                for (name, dir) in [("Saves", h.saves()), ("Logs", h.logs()), ("Screenshots", h.screenshots()), ("Settings", h.root.clone())] {
                    if ui.button(name).clicked() {
                        let _ = std::fs::create_dir_all(&dir);
                        launch::open_folder(&dir);
                    }
                }
            });
        }
    }

    fn save_game_folder(&mut self) {
        let dir = PathBuf::from(self.game_edit.trim());
        let r = match GameInstall::open(&dir) {
            Ok(_) => self.write_ini(|t| Config { game_dir: Some(dir.clone()), code_page: None }.to_ini(&t)),
            Err(e) => Err(e.to_string()),
        };
        self.after_write(r, Some("Game folder saved."));
    }
}

/// The folder the launcher runs from, when the updater may change it ([`update::check_install`]).
fn install_dir(game: Option<&GameInstall>) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("the launcher's folder is unknown")?.to_path_buf();
    update::check_install(&dir, game)?;
    Ok(dir)
}

fn megabytes(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1_048_576.0)
}

// ---- The updater ----

impl App {
    /// Download, check and unpack the newest release in the background.
    fn start_update(&mut self, ctx: &egui::Context) {
        let (Some(latest), Some(home)) = (self.latest.clone(), self.home.clone()) else { return };
        let game = self.game();
        let (tx, rx) = channel();
        self.upd_cancel = Arc::new(AtomicBool::new(false));
        let (cancel, c) = (self.upd_cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            let send = |m: Msg| {
                let _ = tx.send(m);
                c.request_repaint();
            };
            let r = update::prepare(&latest, &home.cache(), game.as_ref(), &send, &cancel);
            send(Msg::Ready(r));
        });
        self.upd_rx = Some(rx);
        self.upd = Upd::Working { phase: Phase::Checking, done: 0, total: 0 };
    }

    fn start_install(&mut self, ctx: &egui::Context, ready: update::Ready) {
        let game = self.game();
        let dir = match install_dir(game.as_ref()) {
            Ok(d) => d,
            Err(e) => {
                self.upd = Upd::Failed(e);
                return;
            }
        };
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Installed(update::install(&dir, &ready, version(), game.as_ref())));
            c.request_repaint();
        });
        self.upd_rx = Some(rx);
        self.upd = Upd::Installing;
    }

    fn roll_back(&mut self, dir: PathBuf, ctx: &egui::Context) {
        if self.running() {
            self.message = Some(("Close the game first, then roll back.".into(), true));
            return;
        }
        match update::roll_back(&dir, version(), self.game().as_ref()) {
            Ok(v) => match update::restart(&dir, "--rolled-back") {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.message = Some((format!("Rolled back to v{v}; start the launcher again ({e})."), false)),
            },
            Err(e) => self.message = Some((format!("Roll back failed, nothing was changed: {e}"), true)),
        }
    }

    fn poll_update(&mut self, ctx: &egui::Context, running: bool) {
        while let Some(m) = self.upd_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            match m {
                Msg::Phase(p) => {
                    if let Upd::Working { phase, .. } = &mut self.upd {
                        *phase = p;
                    }
                }
                Msg::Bytes(n, t) => {
                    if let Upd::Working { done, total, .. } = &mut self.upd {
                        (*done, *total) = (n, t);
                    }
                }
                Msg::Ready(Ok(r)) => self.upd = Upd::Ready(r),
                Msg::Ready(Err(e)) => self.upd = Upd::Failed(e),
                Msg::Installed(Ok(v)) => {
                    self.upd_rx = None;
                    let restarted = install_dir(self.game().as_ref()).and_then(|d| update::restart(&d, "--updated").map_err(|e| e.to_string()));
                    match restarted {
                        Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(e) => {
                            self.upd = Upd::Idle;
                            self.latest = None;
                            self.message = Some((format!("Updated to v{v}; start the launcher again ({e})."), false));
                        }
                    }
                    return;
                }
                Msg::Installed(Err(e)) => self.upd = Upd::Failed(e),
            }
        }
        if matches!(self.upd, Upd::Ready(_)) && !running {
            if let Upd::Ready(r) = std::mem::replace(&mut self.upd, Upd::Idle) {
                self.start_install(ctx, r);
            }
        }
    }

    /// The update dialog: what is new, Update now / Later, then the progress.
    fn update_window(&mut self, ctx: &egui::Context, running: bool) {
        if !self.upd_open {
            return;
        }
        let Some(l) = self.latest.clone() else { return };
        let size = update::target().and_then(|t| update::choose_asset(&l.assets, &l.tag, t)).map(|(a, _)| a.size);
        let mut open = true;
        // A set width (most of the launcher's, at most 760): a window left to size itself
        // shrinks to its text and wraps the notes into a narrow column.
        let width = (ctx.content_rect().width() - 160.0).clamp(420.0, 760.0);
        egui::Window::new(format!("NVForever v{}", l.version))
            .collapsible(false)
            .resizable(false)
            .min_width(width)
            .max_width(width)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            // Room around the text and between the rows: the default window margin is tight.
            .frame(Frame::window(&ctx.global_style()).inner_margin(Margin::symmetric(24, 20)))
            // Our own heading and close button: egui's title bar does not reach across the
            // wider margin.
            .title_bar(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("NVForever v{}", l.version)).size(22.0).strong().color(GOLD));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("×").size(24.0).color(WEAK)).frame(false)).on_hover_text("Close").clicked() {
                            open = false;
                        }
                    });
                });
                ui.separator();
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
                ui.label(format!("v{} is out; this is v{}.{}", l.version, version(), size.map_or(String::new(), |s| format!(" Download: {}.", megabytes(s)))));
                if !l.notes.trim().is_empty() {
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
                        CommonMarkViewer::new().show(ui, &mut self.md, &l.notes);
                    });
                }
                ui.add_space(14.0);
                let install = install_dir(self.game().as_ref());
                match &self.upd {
                    Upd::Idle => {
                        let note = match &install {
                            Ok(_) => "Your saves, settings and mods are not touched. The version you have now is kept, to roll back to from the Game folder page.".to_string(),
                            Err(e) => e.clone(),
                        };
                        ui.label(RichText::new(note).small().color(WEAK));
                        ui.horizontal(|ui| {
                            if install.is_ok() && ui.button(RichText::new("Update now").strong()).clicked() {
                                self.start_update(ui.ctx());
                            }
                            if ui.button("Later").clicked() {
                                self.upd_open = false;
                            }
                            if ui.button("Release page").clicked() {
                                launch::open_url(&l.url);
                            }
                        });
                    }
                    Upd::Working { phase, done, total } => {
                        let (phase, done, total) = (*phase, *done, *total);
                        let bar = if phase == Phase::Downloading && total > 0 {
                            egui::ProgressBar::new(done as f32 / total as f32).text(format!("{} {} of {}", phase.words(), megabytes(done), megabytes(total)))
                        } else {
                            egui::ProgressBar::new(0.0).animate(true).text(phase.words())
                        };
                        ui.add(bar);
                        if phase == Phase::Downloading && ui.button("Cancel").clicked() {
                            self.upd_cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    Upd::Ready(_) => {
                        ui.label(RichText::new(if running { "Downloaded. It installs as soon as the game is closed." } else { "Downloaded." }).color(GOOD));
                    }
                    Upd::Installing => {
                        ui.add(egui::ProgressBar::new(1.0).animate(true).text(Phase::Installing.words()));
                    }
                    Upd::Failed(e) => {
                        ui.label(RichText::new(format!("The update did not go in: {e}")).color(BAD));
                        ui.label(RichText::new("Try again, or download it from the release page.").small().color(WEAK));
                        ui.horizontal(|ui| {
                            if install.is_ok() && ui.button("Try again").clicked() {
                                self.start_update(ui.ctx());
                            }
                            if ui.button("Release page").clicked() {
                                launch::open_url(&l.url);
                            }
                        });
                    }
                }
            });
        // Closing the window while it works lets the download carry on; the Update button
        // shows it again.
        if !open {
            self.upd_open = false;
            if matches!(self.upd, Upd::Failed(_)) {
                self.upd = Upd::Idle;
            }
        }
    }
}

#[cfg(windows)]
fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(windows)]
fn pick_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_file()
}

/// No picker off Windows (Cargo.toml): the path is typed.
#[cfg(not(windows))]
fn pick_folder(_: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
fn pick_file(_: &str) -> Option<PathBuf> {
    None
}

/// A save's place as the Continue button shows it: the cell's name when the save holds one
/// ("The Tops Presidential Suite"), else nothing. The game writes the cell's FULL name, else
/// its Editor ID (`StripFSGate`), else its worldspace and grid (`WastelandNV (-15, -13)`)
/// (`place_name` in engine/nvf-play/src/console.rs); the launcher has no data files to look
/// the last two up in, and an Editor ID is no name to show a player.
fn shown_place(location: &str) -> Option<&str> {
    let l = location.trim();
    let grid = l.ends_with(')') && l.rfind(" (").is_some_and(|i| l[i + 2..l.len() - 1].split(',').all(|n| n.trim().parse::<i32>().is_ok()));
    let editor_id = !l.contains(' ');
    (!l.is_empty() && !grid && !editor_id).then_some(l)
}

/// The gap a dragged row would drop into: the number of rows whose middle is above `y`
/// (0 = before the first row, `centers.len()` = after the last).
fn insertion_index(centers: &[f32], y: f32) -> usize {
    centers.iter().filter(|c| **c < y).count()
}

/// The index row `from` ends at when dropped into gap `gap` (taken out first, so a gap below
/// it is one less).
fn move_target(from: usize, gap: usize) -> usize {
    if gap > from {
        gap - 1
    } else {
        gap
    }
}

/// `body` cut after the paragraph that passes [`PREVIEW_CHARS`], unless `open`; whether it
/// was long enough to cut.
fn preview(body: &str, open: bool) -> (&str, bool) {
    if body.len() <= PREVIEW_CHARS {
        return (body, false);
    }
    // A byte count can fall inside a character (`·`, `—`); start at the next whole one.
    let from = (PREVIEW_CHARS..=body.len()).find(|&i| body.is_char_boundary(i)).unwrap_or(body.len());
    let at = body[from..].find("\n\n").map(|i| i + from);
    match at {
        Some(at) if !open => (&body[..at], true),
        Some(_) => (body, true),
        None => (body, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_as_the_ini_keeps_them() {
        assert_eq!(part_text(0.5), "0.50");
        assert_eq!(part_text(1.0 / 3.0), "0.333");
        assert_eq!(part_text(1.2), "1.00");
        assert_eq!(part_text(0.25), "0.25");
    }

    /// The launcher's controls drawn with no window (egui headless), the pointer driven by
    /// events as a player's mouse would: what each frame's control returned and where its
    /// words were drawn.
    struct Headless {
        ctx: egui::Context,
        time: f64,
    }

    type Words = Vec<(String, egui::Pos2)>;

    impl Headless {
        fn new() -> Headless {
            Headless { ctx: egui::Context::default(), time: 0.0 }
        }

        /// One frame with `events`: the control's answer and the frame's words with their middles.
        fn frame(&mut self, events: Vec<egui::Event>, kind: &menu::Kind, v: &str) -> (Option<String>, Words) {
            self.time += 0.1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 600.0))),
                time: Some(self.time),
                focused: true,
                events,
                ..Default::default()
            };
            let mut got = None;
            let out = self.ctx.run_ui(input, |ui| got = menu_control(ui, "row", kind, v, 260.0));
            let mut words = Vec::new();
            for c in &out.shapes {
                collect_words(&c.shape, &mut words);
            }
            out.drop_without_applying_deltas();
            (got, words)
        }

        /// Frames with no input until the layout settles; every answer they gave.
        fn idle(&mut self, kind: &menu::Kind, v: &str) -> Vec<String> {
            (0..3).filter_map(|_| self.frame(Vec::new(), kind, v).0).collect()
        }

        /// A press at `from`, a move to `to`, a release: every answer on the way.
        fn drag(&mut self, kind: &menu::Kind, v: &str, from: egui::Pos2, to: egui::Pos2) -> Vec<String> {
            let press = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            let steps = [
                vec![egui::Event::PointerMoved(from)],
                vec![press(from, true)],
                vec![egui::Event::PointerMoved(from.lerp(to, 0.5))],
                vec![egui::Event::PointerMoved(to)],
                vec![press(to, false)],
                vec![],
            ];
            steps.into_iter().filter_map(|e| self.frame(e, kind, v).0).collect()
        }

        fn click(&mut self, kind: &menu::Kind, v: &str, at: egui::Pos2) -> Vec<String> {
            self.drag(kind, v, at, at)
        }

        /// The middle of the words `w` as drawn now.
        fn find(&mut self, kind: &menu::Kind, v: &str, w: &str) -> egui::Pos2 {
            let (_, words) = self.frame(Vec::new(), kind, v);
            words.iter().filter(|(t, _)| t == w).map(|(_, p)| *p).max_by(|a, b| a.y.total_cmp(&b.y)).unwrap_or_else(|| panic!("{w:?} not drawn: {words:?}"))
        }
    }

    fn collect_words(s: &egui::Shape, out: &mut Words) {
        match s {
            egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos + t.galley.rect.center().to_vec2())),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect_words(s, out)),
            _ => {}
        }
    }

    fn slider(min: f32, step: f32, words: &[&str]) -> menu::Kind {
        let steps = words.iter().enumerate().map(|(i, w)| menu::Step { value: min + i as f32 * step, label: w.to_string() }).collect();
        menu::Kind::Slider { min, step, steps }
    }

    fn kill_camera() -> menu::Kind {
        let c = |name: &str, label: &str| menu::Choice { name: name.into(), label: label.into() };
        menu::Kind::List { choices: vec![c("Last enemy", "Last Enemy"), c("Every kill", "Every Kill"), c("Off", "Off")] }
    }

    /// Opening a tab changes nothing: no row's control answers without the player.
    #[test]
    fn a_drawn_row_changes_nothing() {
        let cases = [
            (menu::Kind::Switch, "1"),
            (menu::Kind::Switch, "0"),
            // Off the slider's own grid but on the game's (steps from 0): the bug of ee6450871.
            (menu::Kind::Slider { min: 0.1, step: 0.05, steps: vec![] }, "0.35"),
            (menu::Kind::Slider { min: 0.05, step: 1.0 / 30.0, steps: vec![] }, "0.333"),
            // Out of the range (an older build's value): kept, not clamped by drawing it.
            (menu::Kind::Slider { min: 0.2, step: 0.1, steps: vec![] }, "0.10"),
            (menu::Kind::Slider { min: 0.0, step: 0.1, steps: vec![] }, "1.50"),
            (slider(0.0, 0.25, &["Very Easy", "Easy", "Normal", "Hard", "Very Hard"]), "0.75"),
            (kill_camera(), "Last enemy"),
        ];
        for (kind, v) in cases {
            assert_eq!(Headless::new().idle(&kind, v), Vec::<String>::new(), "{kind:?} at {v}");
        }
    }

    #[test]
    fn a_slider_dragged_to_its_ends_is_the_games_first_and_last_value() {
        let kind = menu::Kind::Slider { min: 0.1, step: 0.05, steps: vec![] };
        let rail = egui::Pos2::new(20.0, 10.0);
        let mut h = Headless::new();
        h.idle(&kind, "0.35");
        assert_eq!(h.drag(&kind, "0.35", rail, egui::Pos2::new(2000.0, 10.0)).last().map(String::as_str), Some("1.00"));
        let mut h = Headless::new();
        h.idle(&kind, "0.35");
        assert_eq!(h.drag(&kind, "0.35", rail, egui::Pos2::new(-500.0, 10.0)).last().map(String::as_str), Some("0.10"), "its start, not 0");
        // Moved part of the way: onto a step of the game's (from 0), to the thousandth.
        let mut h = Headless::new();
        h.idle(&kind, "0.35");
        let got = h.drag(&kind, "0.35", rail, egui::Pos2::new(63.0, 10.0));
        let last = got.last().expect("moved");
        let v: f32 = last.parse().unwrap();
        assert!((0.1..=1.0).contains(&v) && ((v / 0.05).round() * 0.05 - v).abs() < 1e-4, "{last} is not on a step of 0.05");
    }

    #[test]
    fn a_slider_shows_the_games_words_for_its_steps() {
        let kind = slider(0.0, 0.25, &["Very Easy", "Easy", "Normal", "Hard", "Very Hard"]);
        let mut h = Headless::new();
        h.idle(&kind, "0.75");
        h.find(&kind, "0.75", "Hard");
        let got = h.drag(&kind, "0.75", egui::Pos2::new(20.0, 10.0), egui::Pos2::new(-500.0, 10.0));
        assert_eq!(got.last().map(String::as_str), Some("0.00"));
        h.find(&kind, "0.00", "Very Easy");
    }

    #[test]
    fn a_list_shows_the_games_words_and_keeps_the_choices_name() {
        let kind = kill_camera();
        let mut h = Headless::new();
        h.idle(&kind, "Last enemy");
        let shown = h.find(&kind, "Last enemy", "Last Enemy");
        assert!(h.click(&kind, "Last enemy", shown).is_empty(), "opening the list changes nothing");
        let every = h.find(&kind, "Last enemy", "Every Kill");
        assert_eq!(h.click(&kind, "Last enemy", every), ["Every kill"], "the INI keeps the name, not the words");
        // The chosen one picked again: no change.
        let mut h = Headless::new();
        h.idle(&kind, "Last enemy");
        let shown = h.find(&kind, "Last enemy", "Last Enemy");
        h.click(&kind, "Last enemy", shown);
        let item = h.find(&kind, "Last enemy", "Last Enemy");
        assert!(item.y > shown.y, "the open list's own line");
        assert!(h.click(&kind, "Last enemy", item).is_empty(), "the chosen one picked again");
    }

    #[test]
    fn a_switch_clicked_flips() {
        for (from, to) in [("0", "1"), ("1", "0")] {
            let mut h = Headless::new();
            h.idle(&menu::Kind::Switch, from);
            assert_eq!(h.click(&menu::Kind::Switch, from, egui::Pos2::new(20.0, 10.0)), [to]);
        }
    }

    /// A menu file the game did not write as it should (a hand edit, a broken write) draws
    /// and answers without a panic, and a moved slider stays a value the game reads.
    #[test]
    fn odd_rows_draw_and_answer_without_a_panic() {
        let c = |n: &str| menu::Choice { name: n.into(), label: n.into() };
        let kinds = [
            menu::Kind::Slider { min: 2.0, step: 0.1, steps: vec![] },
            menu::Kind::Slider { min: -1.0, step: -0.5, steps: vec![] },
            menu::Kind::Slider { min: 0.0, step: 0.0, steps: vec![] },
            menu::Kind::Slider { min: 0.0, step: 0.5, steps: vec![menu::Step { value: 0.0, label: String::new() }] },
            menu::Kind::List { choices: vec![] },
            menu::Kind::List { choices: vec![c("A"), c("A"), c("")] },
        ];
        for kind in &kinds {
            for v in ["0.5", "abc", "", "A", "1e9", "-3"] {
                let mut h = Headless::new();
                h.idle(kind, v);
                let got = h.drag(kind, v, egui::Pos2::new(20.0, 10.0), egui::Pos2::new(2000.0, 10.0));
                if let (menu::Kind::Slider { .. }, Some(n)) = (kind, got.last()) {
                    let x: f32 = n.parse().unwrap_or_else(|_| panic!("{kind:?} at {v}: {n} is not a number"));
                    assert!(x.is_finite() && (0.0..=1.0).contains(&x), "{kind:?} at {v}: {n} out of 0..1");
                }
            }
        }
    }

    #[test]
    fn the_games_rules_hold_as_the_game_has_them() {
        let rule = |parent: &str, choices: &[&str]| menu::Rule { parent: parent.into(), choices: choices.iter().map(|c| c.to_string()).collect(), why: String::new() };
        let c = |n: &str| menu::Choice { name: n.into(), label: n.into() };
        let shadows = menu::Kind::List { choices: vec![c("Off"), c("Performance"), c("Quality")] };
        // A row it needs on: a switch on, a list past its first choice.
        assert!(rule_holds(&rule("glow", &[]), Some(&menu::Kind::Switch), "1", "1"));
        assert!(!rule_holds(&rule("glow", &[]), Some(&menu::Kind::Switch), "0", "0"));
        assert!(!rule_holds(&rule("sun-shadows", &[]), Some(&shadows), "Off", "Off"));
        assert!(rule_holds(&rule("sun-shadows", &[]), Some(&shadows), "Quality", "Quality"));
        // Built in (no kind): by the value.
        assert!(!rule_holds(&rule("sun-shadows", &[]), None, "Off", "Off"));
        assert!(!rule_holds(&rule("glow", &[]), None, "", ""));
        assert!(rule_holds(&rule("glow", &[]), None, "1", "1"));
        // On one of its choices, by name (the frame generation's `dlss` named DLSS).
        assert!(rule_holds(&rule("frame-generation", &["DLSS"]), None, "dlss", "DLSS"));
        assert!(!rule_holds(&rule("upscaler", &["FSR 3.1", "DLSS"]), None, "Off", "Off"));
        assert!(rule_holds(&rule(nvf_settings_layout::DEVICE_ROW, &["Controller"]), None, "controller", "controller"));
        // Typed into a slider of words: the word's part, else a number.
        let words: Vec<String> = ["Low", "Medium", "High"].map(String::from).to_vec();
        assert_eq!((word_part(&words, " high "), word_part(&words, "medium"), word_part(&words, "0.25")), (Some(1.0), Some(0.5), Some(0.25)));
        assert_eq!((word_part(&words, "Ultra"), word_part(&["Only".to_string()], "only")), (None, Some(0.0)));
    }

    #[test]
    fn a_moved_slider_is_fitted_as_the_game_fits_it() {
        // Steps from 0 as the game counts them, not from the slider's start.
        assert_eq!(part_text(game_fit(0.52, 0.1, 0.05)), "0.50");
        assert_eq!(part_text(game_fit(0.01, 0.1, 0.05)), "0.10", "not below the start");
        assert_eq!(part_text(game_fit(0.34, 0.0, 1.0 / 30.0)), "0.333");
        assert_eq!(part_text(game_fit(2.0, 0.0, 0.25)), "1.00");
    }

    #[test]
    fn only_real_place_names_are_shown() {
        assert_eq!(shown_place("The Tops Presidential Suite"), Some("The Tops Presidential Suite"));
        assert_eq!(shown_place("Goodsprings"), None);
        assert_eq!(shown_place("StripFSGate"), None);
        assert_eq!(shown_place("WastelandNV (-15, -13)"), None);
        assert_eq!(shown_place(""), None);
        assert_eq!(shown_place("Prospector Saloon"), Some("Prospector Saloon"));
    }

    #[test]
    fn links_are_taken_from_egui() {
        let ctx = egui::Context::default();
        let mut taken = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://nv-forever.com"));
            ui.ctx().copy_text("kept".into());
            taken = take_links(ui.ctx());
        });
        assert_eq!(taken, ["https://nv-forever.com"]);
        assert!(!out.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::OpenUrl(_))));
        assert!(out.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(_))));
        // The test never draws, so the font atlas is unused. egui panics if it is dropped dirty.
        out.textures_delta.clear();
    }

    #[test]
    fn a_saved_profile_reads_back_in_page_order() {
        let dir = std::env::temp_dir().join(format!("nvforever-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rows = vec![
            super::ModRow { name: "A".into(), on: false },
            super::ModRow { name: "B".into(), on: true },
            super::ModRow { name: "C".into(), on: true },
        ];
        let list = fnv_install::plugins::PluginList {
            entries: vec![
                fnv_install::plugins::PluginEntry { name: "FalloutNV.esm".into(), active: true },
                fnv_install::plugins::PluginEntry { name: "Off.esp".into(), active: false },
                fnv_install::plugins::PluginEntry { name: "On.esp".into(), active: true },
            ],
        };
        let saved = dir.join("Profile");
        super::App::write_profile_folder(&saved, &rows, Some(&list)).unwrap();
        let mods = std::fs::read_to_string(saved.join("modlist.txt")).unwrap();
        let plugins_text = std::fs::read_to_string(saved.join("plugins.txt")).unwrap();
        let order = std::fs::read_to_string(saved.join("loadorder.txt")).unwrap();
        assert_eq!(
            super::App::profile_mod_rows(&mods),
            vec![("A".into(), false), ("B".into(), true), ("C".into(), true)]
        );
        assert_eq!(super::App::profile_plugin_names(&order), vec!["FalloutNV.esm", "Off.esp", "On.esp"]);
        assert_eq!(super::App::profile_plugin_names(&plugins_text), vec!["FalloutNV.esm", "On.esp"]);
        assert!(mods.lines().next().unwrap().starts_with("+C"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dragging_a_row() {
        let centers = [10.0, 30.0, 50.0, 70.0];
        assert_eq!(insertion_index(&centers, 0.0), 0);
        assert_eq!(insertion_index(&centers, 31.0), 2);
        assert_eq!(insertion_index(&centers, 99.0), 4);
        // Row 1 dropped below row 3 (gap 4) ends last; dropped at its own gaps it stays.
        assert_eq!(move_target(1, 4), 3);
        assert_eq!(move_target(1, 1), 1);
        assert_eq!(move_target(1, 2), 1);
        // Row 3 dropped into gap 1 ends second.
        assert_eq!(move_target(3, 1), 1);
        // The same move on a list.
        let mut v = vec!["a", "b", "c", "d"];
        let (from, to) = (0, move_target(0, insertion_index(&centers, 60.0)));
        let x = v.remove(from);
        v.insert(to, x);
        assert_eq!(v, ["b", "c", "a", "d"]);
    }

    #[test]
    fn previews_cut_at_a_paragraph() {
        assert_eq!(preview("short", false), ("short", false));
        let long = format!("{}\n\nrest", "a".repeat(PREVIEW_CHARS + 5));
        assert_eq!(preview(&long, false).0.len(), PREVIEW_CHARS + 5);
        // Byte PREVIEW_CHARS inside a two-byte `·`: no panic, cut at the paragraph after it.
        let wide = format!("{}·b\n\nrest", "a".repeat(PREVIEW_CHARS - 1));
        assert!(!wide.is_char_boundary(PREVIEW_CHARS));
        assert_eq!(preview(&wide, false), (&wide[..wide.len() - 6], true));
        let wide_end = format!("{}—", "a".repeat(PREVIEW_CHARS - 1));
        assert_eq!(preview(&wide_end, false), (wide_end.as_str(), false));
        assert_eq!(preview(&long, true), (long.as_str(), true));
    }
}

const BACKUP_MAX: &[u32] = &[1, 3, 5, 10, 20];
const BACKUP_MAX_DEFAULT: u32 = 5;

fn backup_dir(home: &EngineHome) -> PathBuf {
    home.root.join("plugin-backups")
}

fn backup_label(path: &std::path::Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("backup").trim_start_matches("plugins-").trim_end_matches(".txt").to_string()
}

fn read_backup_max(home: Option<&EngineHome>) -> u32 {
    let Some(home) = home else { return BACKUP_MAX_DEFAULT };
    std::fs::read_to_string(backup_dir(home).join("max")).ok().and_then(|t| t.trim().parse().ok()).filter(|n| BACKUP_MAX.contains(n)).unwrap_or(BACKUP_MAX_DEFAULT)
}

fn write_backup_max(home: &EngineHome, max: u32) -> Result<(), String> {
    let dir = backup_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("max");
    guard_write(&path, None).map_err(|e| e.to_string())?;
    write_whole(&path, max.to_string().as_bytes()).map_err(|e| e.to_string())
}

fn plugin_backups(home: &EngineHome) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(backup_dir(home)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("plugins-") && n.ends_with(".txt"))).collect();
    files.sort();
    files
}

fn prune_plugin_backups(home: &EngineHome, max: u32) {
    let files = plugin_backups(home);
    let keep = max as usize;
    if files.len() > keep {
        for path in &files[..files.len() - keep] {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn backup_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400) as u32;
    let (h, min, sec) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 { y += 1; }
    format!("{y:04}-{m:02}-{d:02}-{h:02}{min:02}{sec:02}")
}

fn keep_plugin_backup(home: &EngineHome, bytes: &[u8], max: u32) -> Result<(), String> {
    if bytes.is_empty() || max == 0 {
        prune_plugin_backups(home, max);
        return Ok(());
    }
    let dir = backup_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = backup_stamp();
    let mut dest = dir.join(format!("plugins-{stamp}.txt"));
    let mut n = 2u32;
    while dest.exists() {
        dest = dir.join(format!("plugins-{stamp}-{n}.txt"));
        n += 1;
    }
    guard_write(&dest, None).map_err(|e| e.to_string())?;
    write_whole(&dest, bytes).map_err(|e| e.to_string())?;
    prune_plugin_backups(home, max);
    Ok(())
}
