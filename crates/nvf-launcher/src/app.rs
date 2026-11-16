//! The launcher's window: Home (news), Settings, Mods and Game folder pages beside a bar
//! that starts the game.

use crate::catalog::{self, Kind, Row};
use crate::launch::{self, Start};
use crate::news::{self, Post, Source};
use crate::theme::{self, BAD, CARD, GOLD, GOOD, LINE, WEAK};
use eframe::egui::{self, load::Bytes, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use fnv_install::{guard_write, ini_get, ini_pairs, ini_set, plugins, write_whole, Config, EngineHome, GameInstall};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
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
            news: Vec::new(),
            news_state: NewsState::Off,
            news_rx: None,
            next_rx: None,
            next: None,
            latest_rx: None,
            latest: None,
            expanded: HashSet::new(),
            md: CommonMarkCache::default(),
            newest: None,
            exe: None,
            child: None,
            shot: std::env::var_os("NVF_LAUNCHER_SHOT").map(|p| (PathBuf::from(p), 0)),
        };
        app.page = match std::env::var("NVF_LAUNCHER_PAGE").as_deref() {
            Ok("settings") => Page::Settings,
            Ok("mods") => Page::Mods,
            Ok("game") => Page::Game,
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
                Ok(l) => self.latest = l.filter(|l| news::is_newer(&l.version, env!("CARGO_PKG_VERSION"))),
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
        if running {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        self.top_bar(ui);
        self.bottom_bar(ui, ctx, running);
        self.nav(ui);
        self.test_shot(ctx);
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
                        if ui.add(b).on_hover_text(format!("v{} is out (this is v{}): opens its download page", l.version, env!("CARGO_PKG_VERSION"))).clicked() {
                            launch::open_url(&l.url);
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
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).size(20.0).strong().color(WEAK));
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
                    let rows: Vec<Row> = self.rows.iter().filter(|r| r.tab == tab).cloned().collect();
                    let mut heading: Option<Option<String>> = None;
                    for r in rows {
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
        self.stored.keys().filter(|k| !self.rows.iter().any(|r| &r.key == *k)).cloned().collect()
    }

    fn setting_row(&mut self, ui: &mut egui::Ui, key: &str, label: &str, note: &str) {
        let stored = self.stored.get(key).cloned();
        let value = self.edits.get(key).cloned().or_else(|| stored.clone());
        Frame::new().fill(CARD).corner_radius(4).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let control_w = 260.0;
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - control_w - 16.0).max(200.0));
                    let changed = self.edits.contains_key(key);
                    ui.label(RichText::new(if changed { format!("{label}  •") } else { label.to_string() }).strong().color(if changed { GOLD } else { theme::TEXT }));
                    if !note.is_empty() {
                        ui.label(RichText::new(note).small().color(WEAK));
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let Some(v) = value else {
                        ui.label(RichText::new("game default").small().color(WEAK)).on_hover_text("Not in NVForever.ini yet: change it once on the game's settings page");
                        return;
                    };
                    let new = match Kind::of_value(&v) {
                        Kind::Switch => {
                            let mut on = v.trim() == "1";
                            toggle(ui, &mut on).changed().then(|| if on { "1" } else { "0" }.to_string())
                        }
                        Kind::Slider => {
                            let mut f: f32 = v.trim().parse().unwrap_or(0.0);
                            let s = match self.words.get(key) {
                                Some(words) => {
                                    let n = words.len();
                                    let words: &'static [&'static str] = words;
                                    egui::Slider::new(&mut f, 0.0..=1.0).step_by(1.0 / (n - 1) as f64).custom_formatter(move |x, _| {
                                        words[((x * (n - 1) as f64).round() as usize).min(n - 1)].to_string()
                                    })
                                }
                                None => egui::Slider::new(&mut f, 0.0..=1.0).custom_formatter(|x, _| format!("{:.0}%", x * 100.0)).custom_parser(|s| {
                                    s.trim().trim_end_matches('%').parse::<f64>().ok().map(|x| x / 100.0)
                                }),
                            };
                            ui.add_sized([control_w, 20.0], s).changed().then(|| part_text(f))
                        }
                        Kind::Text if self.lists.contains_key(key) => {
                            let mut t = v.clone();
                            egui::ComboBox::from_id_salt(key).width(control_w - 8.0).selected_text(t.clone()).show_ui(ui, |ui| {
                                for c in self.lists[key] {
                                    ui.selectable_value(&mut t, c.to_string(), *c);
                                }
                            });
                            (t != v).then_some(t)
                        }
                        Kind::Text => {
                            let mut t = v.clone();
                            ui.add_sized([control_w, 22.0], egui::TextEdit::singleline(&mut t)).changed().then_some(t)
                        }
                    };
                    if let Some(n) = new {
                        if Some(&n) == stored.as_ref() {
                            self.edits.remove(key);
                        } else {
                            self.edits.insert(key.to_string(), n);
                        }
                    }
                });
            });
        });
        ui.add_space(2.0);
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
            RichText::new("NVForever's own load order (plugins.txt in its folder; the original game's list is never read or changed). Top loads first. Using Mod Organizer 2? Start NVForever from MO2 instead.")
                .small()
                .color(WEAK),
        );
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
        let mut swap: Option<(usize, usize)> = None;
        let n = list.entries.len();
        let is_base: Vec<bool> = list.entries.iter().map(|e| e.name.eq_ignore_ascii_case("FalloutNV.esm")).collect();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, e) in list.entries.iter_mut().enumerate() {
                Frame::new().fill(CARD).corner_radius(4).inner_margin(Margin::symmetric(10, 4)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let base = is_base[i];
                        ui.label(RichText::new(format!("{:02X}", i)).monospace().color(WEAK));
                        if ui.add_enabled(!base, egui::Checkbox::without_text(&mut e.active)).changed() {
                            self.mods_dirty = true;
                        }
                        ui.label(&e.name);
                        if !present.is_empty() && !present.contains(&e.name.to_ascii_lowercase()) {
                            ui.label(RichText::new("missing").small().color(BAD));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(!base && i + 1 < n && !is_base[i + 1], egui::Button::new("Down").small()).clicked() {
                                swap = Some((i, i + 1));
                            }
                            if ui.add_enabled(!base && i > 0 && !is_base[i - 1], egui::Button::new("Up").small()).clicked() {
                                swap = Some((i, i - 1));
                            }
                        });
                    });
                });
            }
        });
        if let Some((a, b)) = swap {
            list.entries.swap(a, b);
            self.mods_dirty = true;
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
