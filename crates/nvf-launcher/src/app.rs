//! The launcher's window: Home (news), Settings, Mods and Game folder pages beside a bar
//! that starts the game.

use crate::catalog::{self, Kind, Row};
use crate::launch::{self, Start};
use crate::news::{self, Post, Source};
use crate::theme::{self, BAD, CARD, GOLD, GOOD, LINE, WEAK};
use crate::update::{self, Msg, Phase};
use eframe::egui::{self, load::Bytes, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use fnv_install::{guard_write, ini_get, ini_pairs, ini_set, plugins, write_whole, Config, EngineHome, GameInstall};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
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
    rows: Vec<Row>,
    tabs: Vec<String>,
    tab: usize,
    words: HashMap<&'static str, &'static [&'static str]>,
    lists: HashMap<&'static str, &'static [&'static str]>,
    stored: BTreeMap<String, String>,
    edits: BTreeMap<String, String>,
    // Game folder.
    game_edit: String,
    // Mods.
    mods: Option<plugins::PluginList>,
    mods_dirty: bool,
    /// The row being dragged on the Mods page.
    mods_drag: Option<usize>,
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
    exe: Option<PathBuf>,
    child: Option<Child>,
    /// Test aid: `NVF_LAUNCHER_SHOT=<file.png>[;page]` saves the window once the news has
    /// settled and closes (`NVF_LAUNCHER_PAGE`: home, settings, mods, game).
    shot: Option<(PathBuf, u32)>,
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
        let rows = catalog::rows();
        let mut app = App {
            page: Page::Home,
            home,
            ini_text: String::new(),
            message: None,
            tabs: catalog::tabs(&rows),
            rows,
            tab: 0,
            words: catalog::step_words(),
            lists: catalog::list_choices(),
            stored: BTreeMap::new(),
            edits: BTreeMap::new(),
            game_edit: String::new(),
            mods: None,
            mods_dirty: false,
            mods_drag: None,
            monitors: crate::display::monitors(),
            languages: crate::display::language_choices(&fnv_util::words::dirs(
                std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())).as_deref(),
                EngineHome::user().map(|h| h.root).as_deref(),
            )),
            reports_confirm: false,
            dlss: crate::dlss::Status::read(),
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
                // "other": the tab after the listed ones (the keys the reference does not list).
                app.tab = app.tabs.iter().position(|t| t.eq_ignore_ascii_case(tab)).unwrap_or(if tab.eq_ignore_ascii_case("other") { app.tabs.len() } else { 0 });
                Page::Settings
            }
            _ => Page::Home,
        };
        if app.page == Page::Mods {
            app.load_mods();
        }
        app.reload_ini();
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
        self.game_edit = Config::from_ini(&self.ini_text).game_dir.map(|p| p.display().to_string()).unwrap_or_default();
        let configured = ini_get(&self.ini_text, LAUNCHER, "Exe").filter(|s| !s.is_empty()).map(PathBuf::from);
        self.exe = launch::find_game(configured.as_deref());
    }

    fn launcher_flag(&self, key: &str, default: bool) -> bool {
        ini_get(&self.ini_text, LAUNCHER, key).map_or(default, |v| v != "0")
    }

    fn game(&self) -> Option<GameInstall> {
        Config::from_ini(&self.ini_text).game_dir.and_then(|d| GameInstall::open(d).ok())
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
        self.newest = fnv_save::list(&h.saves()).into_iter().next().map(|(path, hd)| {
            let shot = fnv_save::read_header(&path).ok().map(|s| s.shot).filter(|s| !s.is_empty()).map(Arc::from);
            let name = if hd.player_name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { hd.player_name.clone() };
            let hours = hd.play_seconds / 3600.0;
            let mut parts = vec![format!("Level {}", hd.level)];
            parts.extend(shown_place(&hd.location).map(str::to_string));
            parts.push(format!("{hours:.0} h played"));
            let detail = parts.join(" · ");
            NewestSave { path, name, detail, shot }
        });
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
                _ => {
                    self.child = None;
                    self.reload_ini();
                    self.find_newest_save();
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
        let Some(exe) = self.exe.clone() else {
            self.message = Some((format!("{} was not found beside the launcher: pick it on the Game folder page.", launch::GAME_EXE), true));
            return;
        };
        match launch::spawn(&exe, &start) {
            Ok(c) => {
                self.child = Some(c);
                self.message = Some(("The game is starting…".into(), false));
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
        egui::Panel::bottom("bottom").frame(Frame::new().fill(theme::PANEL).inner_margin(Margin::symmetric(16, 10)).stroke(Stroke::new(1.0, LINE))).show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("v{}", version())).size(20.0).strong().color(WEAK));
                if let Some((m, bad)) = &self.message {
                    ui.add_space(16.0);
                    ui.label(RichText::new(m).color(if *bad { BAD } else { GOOD }));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(!running, |ui| {
                        ui.menu_button(RichText::new("More").size(15.0), |ui| {
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
                        let (title, sub, start) = match &self.newest {
                            _ if running => ("Running…".to_string(), "the game is open".to_string(), None),
                            Some(s) => ("Continue".to_string(), format!("{} · {}", s.name, s.detail), Some(Start::Load(s.path.clone()))),
                            None => ("Play".to_string(), "new game from the main menu".to_string(), Some(Start::MainMenu)),
                        };
                        if big_button(ui, &title, &sub, true).clicked() {
                            if let Some(s) = start {
                                self.start(ctx, s);
                            }
                        }
                        if self.newest.is_some() && !running && big_button(ui, "Main menu", "load or start a game", false).clicked() {
                            self.start(ctx, Start::MainMenu);
                        }
                        if let Some(shot) = self.newest.as_ref().and_then(|s| s.shot.clone()) {
                            ui.add(egui::Image::from_bytes("bytes://newest-save.png", Bytes::Shared(shot)).fit_to_exact_size(Vec2::new(96.0, 54.0)).corner_radius(3));
                        }
                    });
                });
            });
        });
    }
}

const REPO: &str = "https://github.com/LootJunkieAzza/NVForever";

/// A page button of the left bar; the open page's has a gold edge and mark.
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
fn big_button(ui: &mut egui::Ui, title: &str, sub: &str, primary: bool) -> egui::Response {
    let size = Vec2::new(if primary { 280.0 } else { 190.0 }, 58.0);
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
    let left = rect.left() + 18.0;
    let c = egui::pos2(left + 10.0, rect.center().y);
    p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-7.0, -9.0), c + Vec2::new(9.0, 0.0), c + Vec2::new(-7.0, 9.0)], ink, Stroke::NONE));
    p.text(egui::pos2(left + 34.0, rect.top() + 10.0), egui::Align2::LEFT_TOP, title, crate::fonts::title(20.0), ink);
    let sub_font = egui::FontId::proportional(11.5);
    let max = rect.right() - left - 44.0;
    let mut s = sub.to_string();
    let mut chars: Vec<char> = sub.chars().collect();
    while chars.len() > 4 && p.layout_no_wrap(s.clone(), sub_font.clone(), ink).size().x > max {
        chars.pop();
        s = format!("{}…", chars.iter().collect::<String>().trim_end());
    }
    p.text(egui::pos2(left + 34.0, rect.bottom() - 10.0), egui::Align2::LEFT_BOTTOM, s, sub_font, ink.gamma_multiply(0.8));
    resp
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
            let mut tabs = self.tabs.clone();
            let others = self.other_keys();
            if !others.is_empty() {
                tabs.push("Other".into());
            }
            for (i, t) in tabs.iter().enumerate() {
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
            match tab {
                Some(tab) => {
                    if tab == "Graphics" {
                        self.dlss_card(ui);
                    }
                    let rows: Vec<Row> = self.rows.iter().filter(|r| r.tab == tab).cloned().collect();
                    let mut heading: Option<Option<String>> = None;
                    for r in rows {
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
                        self.setting_row(ui, &r.key, &r.label, &r.note);
                    }
                }
                None => {
                    for k in self.other_keys() {
                        self.setting_row(ui, &k, &k, "");
                    }
                }
            }
        });
    }

    /// `[Settings]` keys the reference does not list (rows of a later build, the editor's).
    fn other_keys(&self) -> Vec<String> {
        self.stored
            .keys()
            .filter(|k| !self.rows.iter().any(|r| &r.key == *k))
            .filter(|k| crate::display::superseded_by(k).is_none_or(|newer| !self.stored.contains_key(newer)))
            .cloned()
            .collect()
    }

    fn setting_row(&mut self, ui: &mut egui::Ui, key: &str, label: &str, note: &str) {
        let stored = self.stored.get(key).cloned();
        let value = self.edits.get(key).cloned().or_else(|| stored.clone());
        Frame::new().fill(CARD).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
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
                                    egui::Slider::new(&mut f, 0.0..=1.0).step_by(1.0 / (n - 1) as f64).custom_formatter(move |x, _| {
                                        words[((x * (n - 1) as f64).round() as usize).min(n - 1)].clone()
                                    })
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
            self.dlss = crate::dlss::Status::read();
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
            need(ui, s.windows, "Windows".into());
            let gpu = if s.gpus.is_empty() { "no graphics card named by Windows".to_string() } else { s.gpus.join(", ") };
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
                    self.dlss = crate::dlss::Status::read();
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
                    nvf_dlss_files::download::start(nvf_dlss_files::download::LicenceAccepted::by_player(), None);
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
        let Some(home) = self.home.clone() else { return };
        // Give the plugins new in Data or the mods folder their lines, as the game's start does.
        if let Some(game) = self.game() {
            if let Err(e) = plugins::sync(&home, &game) {
                self.message = Some((e.to_string(), true));
            }
        }
        self.mods = std::fs::read_to_string(home.plugins_file()).ok().map(|t| plugins::PluginList::parse(&t));
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
                if let Some(h) = &self.home {
                    if ui.button("Open mods folder").clicked() {
                        let _ = std::fs::create_dir_all(h.mods());
                        launch::open_folder(&h.mods());
                    }
                }
            });
        });
        ui.label(
            RichText::new("NVForever's own load order (plugins.txt in its folder; the original game's list is never read or changed). Top loads first: drag a row to move it. Using Mod Organizer 2? Start NVForever from MO2 instead.")
                .small()
                .color(WEAK),
        );
        self.ttw_box(ui);
        ui.separator();
        let Some(list) = &mut self.mods else {
            ui.label("No plugin list yet: set the game folder first, or start the game once.");
            return;
        };
        let present: HashSet<String> = self
            .home
            .as_ref()
            .zip(Config::from_ini(&self.ini_text).game_dir.and_then(|d| GameInstall::open(d).ok()))
            .and_then(|(h, g)| plugins::present_plugins(&g, &h.mods()).ok())
            .map(|v| v.into_iter().map(|s| s.to_ascii_lowercase()).collect())
            .unwrap_or_default();
        let n = list.entries.len();
        let is_base: Vec<bool> = list.entries.iter().map(|e| e.name.eq_ignore_ascii_case("FalloutNV.esm")).collect();
        // The rows at the top that stay there (FalloutNV.esm): nothing moves above them.
        let fixed = is_base.iter().take_while(|b| **b).count();
        let mut moved: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let mut rects = Vec::with_capacity(n);
            for (i, e) in list.entries.iter_mut().enumerate() {
                let base = is_base[i];
                let dragged = self.mods_drag == Some(i);
                let fill = if dragged { theme::SELECTED } else { CARD };
                let stroke = if dragged { Stroke::new(1.0, theme::GOLD_DIM) } else { Stroke::NONE };
                // The whole row drags (its sense sits below the checkbox's, so the box keeps its
                // clicks); a click on the name turns the plugin on or off.
                let sense = if base { Sense::hover() } else { Sense::click_and_drag() };
                let mut name_rect = egui::Rect::NOTHING;
                let row = ui.scope_builder(egui::UiBuilder::new().id_salt(("mod-row", i)).sense(sense), |ui| {
                    let hot = !base && (dragged || ui.response().hovered());
                    Frame::new().fill(fill).stroke(stroke).corner_radius(4).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // The grip: three short lines, drawn (the system font has no grip glyph).
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
                            }
                            name_rect = ui.label(&e.name).rect;
                            if !present.is_empty() && !present.contains(&e.name.to_ascii_lowercase()) {
                                ui.label(RichText::new("missing").small().color(BAD));
                            }
                        });
                    });
                });
                let resp = row.response;
                if !base {
                    if resp.drag_started() {
                        self.mods_drag = Some(i);
                    }
                    if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| name_rect.contains(p)) {
                        e.active = !e.active;
                        self.mods_dirty = true;
                    }
                    let _ = resp.clone().on_hover_cursor(egui::CursorIcon::Grab);
                }
                rects.push(resp.rect);
            }
            let Some(from) = self.mods_drag else { return };
            let ctx = ui.ctx().clone();
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            let Some(pos) = ctx.pointer_latest_pos() else { return };
            let centers: Vec<f32> = rects.iter().map(|r| r.center().y).collect();
            let gap = insertion_index(&centers, pos.y).max(fixed);
            // The gold line where the row will land.
            if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
                let y = if gap < rects.len() { rects[gap].top() - ui.spacing().item_spacing.y / 2.0 } else { last.bottom() + ui.spacing().item_spacing.y / 2.0 };
                ui.painter().line_segment([egui::pos2(first.left(), y), egui::pos2(first.right(), y)], Stroke::new(2.0, GOLD));
            }
            // Near the list's top or bottom edge, scroll.
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

    /// Tale of Two Wastelands' start (src/ttw.rs): shown while its master is in the load
    /// order; ticking it writes TTW's two `[General]` lines to NVForever.ini at once.
    fn ttw_box(&mut self, ui: &mut egui::Ui) {
        let Some(active) = self.mods.as_ref().and_then(|l| l.entries.iter().find(|e| e.name.eq_ignore_ascii_case(crate::ttw::PLUGIN))).map(|e| e.active) else { return };
        let mut on = crate::ttw::is_on(&self.ini_text);
        let other = crate::ttw::other_start(&self.ini_text);
        let mut changed = false;
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            changed = ui.checkbox(&mut on, RichText::new("Tale of Two Wastelands: start new games in Vault 101").strong()).changed();
            ui.label(
                RichText::new("Sets TTW's start in NVForever.ini for you ([General] sCharGenQuest=001FFFF8 and no New Vegas intro film). Off: New Game is the New Vegas opening.")
                    .small()
                    .color(WEAK),
            );
            if on && !active {
                ui.label(RichText::new(format!("{} is off in the list below: turn it on, or New Game stays at the New Vegas opening.", crate::ttw::PLUGIN)).small().color(BAD));
            }
            if let Some(q) = &other {
                ui.label(RichText::new(format!("NVForever.ini names another start quest ({q}); ticking the box replaces it.")).small().color(WEAK));
            }
        });
        if changed {
            // Only the INI's text is read again: unsaved changes on the Settings page stay.
            match self.write_ini(|t| if on { crate::ttw::turn_on(&t) } else { crate::ttw::turn_off(&t) }) {
                Ok(()) => {
                    self.ini_text = self.ini_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
                    self.message = Some(((if on { "New games start in Vault 101 (Tale of Two Wastelands)." } else { "New games start at the New Vegas opening." }).to_string(), false));
                }
                Err(e) => self.message = Some((e, true)),
            }
        }
    }

    fn save_mods(&mut self) -> Result<(), String> {
        let (Some(home), Some(list)) = (&self.home, &self.mods) else { return Err("no plugin list".into()) };
        let path = home.plugins_file();
        guard_write(&path, self.game().as_ref()).map_err(|e| e.to_string())?;
        write_whole(&path, list.to_text().as_bytes()).map_err(|e| e.to_string())
    }

    fn game_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Game folder");
        ui.label(RichText::new("Your Fallout: New Vegas folder, the one holding Data. NVForever only reads it.").small().color(WEAK));
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.game_edit).desired_width(ui.available_width() - 190.0));
                if cfg!(windows) && ui.button("Browse…").clicked() {
                    if let Some(d) = pick_folder("Your Fallout: New Vegas folder") {
                        self.game_edit = d.display().to_string();
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
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://nv-forever.com"));
            ui.ctx().copy_text("kept".into());
            taken = take_links(ui.ctx());
        });
        assert_eq!(taken, ["https://nv-forever.com"]);
        assert!(!out.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::OpenUrl(_))));
        assert!(out.platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(_))));
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
