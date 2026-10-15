//! The main menu (nvforever `--menu`): what the game opens on. Continue loads the newest
//! save, New Game starts the opening in Doc Mitchell's house, Load Game shows the saves,
//! Settings the switches, Quit ends the program. Continue and Load Game are greyed while
//! there is no save.
//!
//! From the original [observed: its main menu file `menus\options\start_menu.xml` and the
//! owner's screenshot of its main menu, both in C:\fnv-private]: the entries in a column
//! at the right, right-aligned, the column centred top to bottom; font 7; the interface's
//! amber; the logo (`textures\interface\main\main_title.dds`, read from the owner's
//! archives at run time) at the left. Its entries Credits and Downloadable Content are
//! left out [owner, 2026-10-15].
//!
//! Ours [design, owner 2026-10-15]: behind the menu the plugins' loading screens (LSCR)
//! take turns, in a shuffled order, each held [`HOLD_SECS`] and faded out to black and the
//! next in over [`FADE_SECS`]; the original shows a 3D scene there. The darkened rounded
//! edges are drawn here, not taken from the original's texture.
//!
//! How it fits: the game starts as a new game (the world is Doc Mitchell's house) with the
//! menu over it; while it is open the game is held as under the pause menu
//! (`crate::pause::Paused`: no scripts, the clock and the picture stand still), so New
//! Game only lets the opening begin. Load Game and Settings are the pause menu's own pages
//! (`crate::pause`), drawn over this menu; going back from them shows it again. Continue
//! and loading a save start NVForever again with `--load` (`crate::save::LoadRequest`).
//!
//! **The theme**: the original's main menu music, the track its INI names
//! (`SMainMenuMusicTrack=special\maintitle.mp3`, at `fMainMenuMusicVolume=0.6`)
//! [documented: Fallout_default.ini], read from the owner's `Data\Music` and played from
//! the start, looping [hypothesis: the original loops it], times the music volume
//! (`crate::music::MusicSettings`; silent with `--no-music`). The place's own music stays
//! silent while the menu is open; New Game fades the theme out over [`THEME_FADE`].
//!
//! The INI's main menu colour (`iSystemColorMainMenu…` 199, 255, 165) is not used: the
//! owner's screenshot shows the entries in the interface's amber.
//!
//! Keys: Up/Down (W/S) choose, Enter or Space take; the pointer chooses, a click takes.
//! Test aid `NVF_PLAY_TEST_MAIN_MENU=<seconds>:<action>[,…]` (`up`, `down`, `pick`, or an
//! entry: `continue`, `new`, `load`, `settings`, `quit`; `shot` saves a screenshot
//! `main_menu.png` in the working folder).
//! Log: `main menu: open (…)`, `main menu: <entry>`, `main menu: slide <texture>`.

use crate::interface::{amber, bc_supported, face, plain_font, set_line, u, Face};
use crate::pause::{AidTime, Page, PageRequest, Paused};
use crate::{PlaySet, PlayState, PlayWorld};
use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use nvf_assets::{GameData, TextureCache};
use std::path::PathBuf;

/// Seconds a loading screen stays at full strength, and each half of the change (out to
/// black, then the next in) [design].
pub const HOLD_SECS: f32 = 7.0;
pub const FADE_SECS: f32 = 0.9;
/// The loading screens are dimmed behind the menu so its amber reads over them [design].
const SLIDE_BRIGHTNESS: f32 = 0.55;

/// Font 7 [documented: Fallout_default.ini `sFontFile_7`; start_menu.xml sets the entries in
/// font 7].
const FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";
const LOGO: &str = "textures\\interface\\main\\main_title.dds";
/// The theme [documented: Fallout_default.ini `SMainMenuMusicTrack`, `fMainMenuMusicVolume`].
const THEME: &str = "music\\special\\maintitle.mp3";
const THEME_VOLUME: f32 = 0.6;
/// Seconds the theme takes to fade out after New Game [design].
pub const THEME_FADE: f32 = 1.5;

/// The column, in interface units (960 to the view's height): its right edge this far
/// from the view's [observed: about 56 of 1080 pixels on the owner's screenshot], and one
/// entry this high [observed: 54 of 1080 pixels].
const RIGHT: f32 = 50.0;
const ITEM: f32 = 48.0;
/// The logo: its picture's left and top, and its width (the texture is 4 to 1; the
/// lettering fills about three quarters of its width) [design, after the screenshot].
const LOGO_LEFT: f32 = 40.0;
const LOGO_TOP: f32 = 400.0;
const LOGO_WIDTH: f32 = 680.0;

/// Asked for by nvforever `--menu`: open the menu as the game starts.
#[derive(Resource)]
pub struct MainMenuStart;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Continue,
    New,
    Load,
    Settings,
    Quit,
}

impl Entry {
    pub const ALL: [Entry; 5] = [Entry::Continue, Entry::New, Entry::Load, Entry::Settings, Entry::Quit];

    pub fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::New => "New Game",
            Entry::Load => "Load Game",
            Entry::Settings => "Settings",
            Entry::Quit => "Quit",
        }
    }

    fn aid_name(self) -> &'static str {
        match self {
            Entry::Continue => "continue",
            Entry::New => "new",
            Entry::Load => "load",
            Entry::Settings => "settings",
            Entry::Quit => "quit",
        }
    }

    /// Whether it can be taken with `saves` saves in the folder.
    pub fn enabled(self, saves: usize) -> bool {
        !matches!(self, Entry::Continue | Entry::Load) || saves > 0
    }
}

/// The next entry that can be taken from `from`, `step` (1 down, -1 up), round the ends.
pub fn step(from: usize, step: i32, saves: usize) -> usize {
    let n = Entry::ALL.len() as i32;
    let mut i = from as i32;
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        if Entry::ALL[i as usize].enabled(saves) {
            return i as usize;
        }
    }
    from
}

/// Where the slideshow is: the screen shown, and how far into its turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// Fading in (0 → 1).
    In(f32),
    Hold(f32),
    /// Fading out (1 → 0); the next comes after.
    Out(f32),
}

impl Phase {
    /// After `dt` seconds; `true` when the next screen is due.
    pub fn step(self, dt: f32) -> (Phase, bool) {
        match self {
            Phase::In(t) if t + dt >= FADE_SECS => (Phase::Hold(0.0), false),
            Phase::In(t) => (Phase::In(t + dt), false),
            Phase::Hold(t) if t + dt >= HOLD_SECS => (Phase::Out(0.0), false),
            Phase::Hold(t) => (Phase::Hold(t + dt), false),
            Phase::Out(t) if t + dt >= FADE_SECS => (Phase::In(0.0), true),
            Phase::Out(t) => (Phase::Out(t + dt), false),
        }
    }

    /// The picture's strength.
    pub fn alpha(self) -> f32 {
        match self {
            Phase::In(t) => (t / FADE_SECS).clamp(0.0, 1.0),
            Phase::Hold(_) => 1.0,
            Phase::Out(t) => 1.0 - (t / FADE_SECS).clamp(0.0, 1.0),
        }
    }
}

#[derive(Resource, Default)]
pub struct MainMenu {
    pub open: bool,
    selected: usize,
    saves: usize,
    newest: Option<PathBuf>,
    /// The loading screens' textures, in the order shown.
    slides: Vec<String>,
    slide: usize,
    phase: Option<Phase>,
    face: Option<Face>,
    theme: Option<Theme>,
}

/// The theme playing: its level (changed while it plays) and how loud it is now.
struct Theme {
    levels: std::sync::Arc<[std::sync::atomic::AtomicU32; 3]>,
    entity: Entity,
    level: f32,
}

/// `NVF_PLAY_TEST_MAIN_MENU`: (seconds, action), soonest first.
#[derive(Resource, Default)]
struct MenuAid(Vec<(f32, String)>);

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct Slide;
#[derive(Component, Clone, Copy)]
struct EntryNode(usize);

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        let mut aid = Vec::new();
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_MAIN_MENU") {
            for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match part.split_once(':').and_then(|(t, a)| Some((t.trim().parse::<f32>().ok()?, a.trim().to_string()))) {
                    Some(x) => aid.push(x),
                    None => warn!("NVF_PLAY_TEST_MAIN_MENU: {part:?}: expected <seconds>:<action>"),
                }
            }
            aid.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        app.init_resource::<MainMenu>().insert_resource(MenuAid(aid)).add_systems(Update, (open_at_start, choose, slideshow, show_entries, theme).chain().in_set(PlaySet::Ui));
    }
}

/// Run condition: the main menu is open.
pub fn main_menu_open(m: Option<Res<MainMenu>>) -> bool {
    m.is_some_and(|m| m.open)
}

#[allow(clippy::too_many_arguments)]
fn open_at_start(
    mut commands: Commands,
    start: Option<Res<MainMenuStart>>,
    mut menu: ResMut<MainMenu>,
    mut paused: ResMut<Paused>,
    mut next: ResMut<NextState<PlayState>>,
    cfg: Res<crate::save::SaveConfig>,
    play: Res<PlayWorld>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut music_assets: ResMut<Assets<crate::music::LayeredMusic>>,
) {
    if start.is_none() {
        return;
    }
    commands.remove_resource::<MainMenuStart>();
    let list = cfg.dir.as_deref().map(fnv_save::list).unwrap_or_default();
    menu.saves = list.len();
    menu.newest = list.first().map(|(p, _)| p.clone());
    let mut slides: Vec<String> = fnv_world::loadscreen::load_screens(play.0.lo, play.0.schema).into_iter().filter(|s| s.locations.is_empty()).filter_map(|s| s.texture).collect();
    slides.sort();
    slides.dedup();
    shuffle(&mut slides);
    menu.slides = slides;
    menu.slide = 0;
    menu.phase = Some(Phase::In(0.0));
    menu.selected = if menu.saves > 0 { 0 } else { 1 };
    let bc = bc_supported(formats.as_deref());
    menu.face = Some(face(FONT, plain_font(36.0, 17.0), data.as_deref(), &mut images, bc));
    let logo = data.as_deref().and_then(|d| crate::interface::texture(LOGO, d, &mut images, bc));
    let edge = images.add(vignette());
    let theme = data.as_deref().and_then(|d| match d.vfs.read(THEME) {
        Some(Ok(b)) => crate::music::LayeredMusic::single(std::sync::Arc::from(b), true),
        Some(Err(e)) => {
            warn!("main menu: {THEME}: {e}");
            None
        }
        None => {
            warn!("main menu: {THEME}: not in the game's files");
            None
        }
    });
    if let Some((music, levels)) = theme {
        let entity = commands.spawn((AudioPlayer::<crate::music::LayeredMusic>(music_assets.add(music)), PlaybackSettings::DESPAWN)).id();
        menu.theme = Some(Theme { levels, entity, level: 0.0 });
        info!("main menu: theme {THEME}");
    }
    menu.open = true;
    paused.0 = true;
    next.set(PlayState::Menu);
    spawn(&mut commands, &menu, logo, edge);
    info!("main menu: open ({} loading screens, {} saves{})", menu.slides.len(), menu.saves, menu.newest.as_ref().map(|p| format!(", newest {}", p.display())).unwrap_or_default());
}

/// A shuffled order, different each start.
fn shuffle(v: &mut [String]) {
    let mut s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
    for i in (1..v.len()).rev() {
        // xorshift64
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        v.swap(i, (s % (i as u64 + 1)) as usize);
    }
}

/// Darkened rounded edges: black, its alpha rising towards the edges of a rounded
/// rectangle [design].
fn vignette() -> Image {
    const W: u32 = 320;
    const H: u32 = 180;
    let mut data = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            // Distance outside a rounded rectangle inset from the edges, in pixels.
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (inset, radius) = (26.0, 34.0);
            let dx = (inset + radius - px).max(px - (W as f32 - inset - radius)).max(0.0);
            let dy = (inset + radius - py).max(py - (H as f32 - inset - radius)).max(0.0);
            let outside = (dx * dx + dy * dy).sqrt() - radius;
            let a = ((outside + 30.0) / 40.0).clamp(0.0, 1.0);
            let a = a * a * (3.0 - 2.0 * a);
            data.extend_from_slice(&[0, 0, 0, (a * 250.0) as u8]);
        }
    }
    Image::new(Extent3d { width: W, height: H, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD)
}

fn spawn(commands: &mut Commands, menu: &MainMenu, logo: Option<(Handle<Image>, Vec2)>, edge: Handle<Image>) {
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands
        .spawn((MenuRoot, full.clone(), BackgroundColor(Color::BLACK), GlobalZIndex(100)))
        .with_children(|root| {
            root.spawn((Slide, ImageNode { color: Color::srgba(1.0, 1.0, 1.0, 0.0), ..default() }, Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden));
            // Stretched to the window whatever its shape.
            root.spawn((ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(edge) }, full.clone()));
            if let Some((image, size)) = logo {
                let h = LOGO_WIDTH * size.y / size.x.max(1.0);
                root.spawn((ImageNode::new(image), Node { position_type: PositionType::Absolute, left: u(LOGO_LEFT), top: u(LOGO_TOP), width: u(LOGO_WIDTH), height: u(h), ..default() }));
            }
            let Some(f) = menu.face.as_ref() else { return };
            let n = Entry::ALL.len() as f32;
            let top = (960.0 - n * ITEM) / 2.0;
            for (i, e) in Entry::ALL.iter().enumerate() {
                let line = f.font.lines(e.label(), None).into_iter().next();
                let w = f.font.width(e.label());
                root.spawn((
                    EntryNode(i),
                    Button,
                    Node { position_type: PositionType::Absolute, right: u(RIGHT), top: u(top + i as f32 * ITEM), width: u(w), height: u(ITEM), ..default() },
                ))
                .with_children(|p| {
                    if let Some(line) = line {
                        // The glyphs stand on a line at about three quarters of the entry.
                        set_line(p, f, &line, 0.0, ITEM * 0.72, amber(255.0));
                    }
                });
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    time: AidTime,
    mut aid: ResMut<MenuAid>,
    mut menu: ResMut<MainMenu>,
    page: Res<Page>,
    mut request: ResMut<PageRequest>,
    entries: Query<(&Interaction, &EntryNode), Changed<Interaction>>,
    mut paused: ResMut<Paused>,
    mut next: ResMut<NextState<PlayState>>,
    mut loads: MessageWriter<crate::save::LoadRequest>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    mut last_page: Local<Option<Page>>,
) {
    if !menu.open {
        return;
    }
    // The frame a pause page goes away is not this menu's: its click or key was that page's.
    let came_back = last_page.replace(*page).is_some_and(|p| p != Page::Main);
    let now = time.elapsed_secs();
    let mut acts = Vec::new();
    while aid.0.first().is_some_and(|(t, _)| *t <= now) {
        let (_, a) = aid.0.remove(0);
        info!("test aid NVF_PLAY_TEST_MAIN_MENU: {a}");
        acts.push(a);
    }
    // The pause menu's pages (Load Game, Settings) take the keys while they are up.
    if *page != Page::Main || request.0.is_some() || came_back {
        return;
    }
    let saves = menu.saves;
    let mut take = None;
    for (i, e) in &entries {
        let ok = Entry::ALL[e.0].enabled(saves);
        match i {
            Interaction::Hovered if ok => menu.selected = e.0,
            Interaction::Pressed if ok => take = Some(Entry::ALL[e.0]),
            _ => {}
        }
    }
    for a in &acts {
        match a.as_str() {
            "up" => menu.selected = step(menu.selected, -1, saves),
            "down" => menu.selected = step(menu.selected, 1, saves),
            "pick" => take = Some(Entry::ALL[menu.selected]),
            "shot" => {
                info!("main menu: screenshot main_menu.png");
                commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk("main_menu.png"));
            }
            other => match Entry::ALL.iter().find(|e| e.aid_name() == other) {
                Some(e) if e.enabled(saves) => take = Some(*e),
                Some(e) => info!("main menu: {} is greyed out", e.label()),
                None => warn!("NVF_PLAY_TEST_MAIN_MENU: no action {other:?}"),
            },
        }
    }
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        menu.selected = step(menu.selected, -1, saves);
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        menu.selected = step(menu.selected, 1, saves);
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) && Entry::ALL[menu.selected].enabled(saves) {
        take = Some(Entry::ALL[menu.selected]);
    }
    let Some(e) = take else { return };
    info!("main menu: {}", e.label());
    match e {
        Entry::Continue => {
            if let Some(p) = menu.newest.clone() {
                info!("main menu: continue from {}", p.display());
                loads.write(crate::save::LoadRequest { path: p });
            }
        }
        Entry::New => {
            menu.open = false;
            paused.0 = false;
            next.set(PlayState::Playing);
            for r in &roots {
                commands.entity(r).despawn();
            }
            info!("main menu: closed, the opening begins");
        }
        Entry::Load => request.0 = Some(Page::Load { selected: 0 }),
        Entry::Settings => request.0 = Some(Page::Settings { selected: 0 }),
        Entry::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

/// The loading screens behind the menu, one after another with a fade through black.
#[allow(clippy::too_many_arguments)]
fn slideshow(
    real: Res<Time<Real>>,
    mut menu: ResMut<MainMenu>,
    mut slides: Query<(&mut ImageNode, &mut Node, &mut Visibility), With<Slide>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    data: Option<Res<GameData>>,
    textures: Option<ResMut<TextureCache>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
) {
    let Some(phase) = menu.phase.filter(|_| menu.open) else { return };
    let Ok((mut img, mut node, mut vis)) = slides.single_mut() else { return };
    if menu.slides.is_empty() {
        return;
    }
    let (phase, next) = phase.step(real.delta_secs().min(0.1));
    menu.phase = Some(phase);
    if next {
        menu.slide = (menu.slide + 1) % menu.slides.len();
    }
    // A new picture at the start of each turn (and the first).
    let want = &menu.slides[menu.slide];
    let fresh = next || *vis == Visibility::Hidden;
    if fresh {
        if let (Some(d), Some(mut cache)) = (data.as_deref(), textures) {
            match cache.get(want, true, d, &mut images, formats.as_deref()) {
                Some(h) => {
                    img.image = h;
                    *vis = Visibility::Inherited;
                    info!("main menu: slide {want}");
                }
                None => {
                    warn!("main menu: {want}: not shown");
                    menu.slide = (menu.slide + 1) % menu.slides.len();
                }
            }
        }
    }
    img.color = Color::srgba(SLIDE_BRIGHTNESS, SLIDE_BRIGHTNESS, SLIDE_BRIGHTNESS, phase.alpha());
    // Cover the window, cropping the overflow evenly.
    if let (Ok(w), Some(i)) = (windows.single(), images.get(&img.image)) {
        let (size, offset) = crate::loading::cover(i.size().as_vec2(), Vec2::new(w.width(), w.height()));
        node.width = Val::Px(size.x);
        node.height = Val::Px(size.y);
        node.left = Val::Px(offset.x);
        node.top = Val::Px(offset.y);
    }
}

/// The chosen entry bright, the others dimmer, a greyed one dark.
fn show_entries(menu: Res<MainMenu>, entries: Query<(&EntryNode, &Children)>, mut glyphs: Query<&mut ImageNode, Without<Slide>>, mut texts: Query<&mut TextColor>) {
    if !menu.open {
        return;
    }
    for (e, children) in &entries {
        let entry = Entry::ALL[e.0];
        let c = if !entry.enabled(menu.saves) {
            Color::srgba(0.42, 0.40, 0.36, 0.45)
        } else if e.0 == menu.selected {
            amber(255.0)
        } else {
            amber(165.0)
        };
        for &child in children {
            if let Ok(mut g) = glyphs.get_mut(child) {
                if g.color != c {
                    g.color = c;
                }
            }
            if let Ok(mut t) = texts.get_mut(child) {
                t.0 = c;
            }
        }
    }
}

/// The theme's level: up to its volume while the menu is open, down to silence after.
fn theme(real: Res<Time<Real>>, mut menu: ResMut<MainMenu>, settings: Option<Res<crate::music::MusicSettings>>, mut commands: Commands) {
    let open = menu.open;
    let Some(t) = menu.theme.as_mut() else { return };
    let volume = settings.as_deref().map_or(1.0, |s| if s.play { s.volume.max(0.0) } else { 0.0 }) * THEME_VOLUME;
    let dt = real.delta_secs().min(0.1);
    t.level = if open { (t.level + dt / 0.5).min(1.0) } else { (t.level - dt / THEME_FADE).max(0.0) };
    t.levels[0].store((t.level * volume).to_bits(), std::sync::atomic::Ordering::Relaxed);
    if !open && t.level <= 0.0 {
        if let Ok(mut e) = commands.get_entity(t.entity) {
            e.despawn();
        }
        menu.theme = None;
        info!("main menu: theme faded out");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_saves_continue_and_load_are_skipped() {
        assert!(!Entry::Continue.enabled(0) && !Entry::Load.enabled(0));
        assert!(Entry::New.enabled(0) && Entry::Continue.enabled(1));
        // New Game (1) down: Load is greyed, so Settings (3); up from New: round to Quit.
        assert_eq!(step(1, 1, 0), 3);
        assert_eq!(step(1, -1, 0), 4);
        assert_eq!(step(1, -1, 2), 0);
        assert_eq!(step(4, 1, 0), 1);
    }

    #[test]
    fn a_slide_holds_then_fades_out_and_the_next_fades_in() {
        let mut p = Phase::In(0.0);
        let mut changes = 0;
        let mut t = 0.0;
        while t < 2.0 * (HOLD_SECS + 2.0 * FADE_SECS) {
            let (q, next) = p.step(0.05);
            changes += next as u32;
            p = q;
            t += 0.05;
        }
        assert_eq!(changes, 2);
        assert_eq!(Phase::Hold(1.0).alpha(), 1.0);
        assert!((Phase::Out(FADE_SECS / 2.0).alpha() - 0.5).abs() < 1e-6);
    }
}
