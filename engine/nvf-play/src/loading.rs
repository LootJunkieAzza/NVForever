//! Loading screens for travel (doors, scripts moving the player): the game holds still
//! under a full-screen picture until the destination is loaded
//! (docs/design/loading-screen.md; records: docs/spec-notes/loading-screens.md).
//!
//! - Whoever moves the player sends a [`TravelRequest`]. The screen comes up first
//!   ([`PlayState::Loading`]); [`SHOW_FRAMES`] frames later, once it has been drawn, the
//!   request goes on as the [`Travel`] that physics and the renderer apply, so their
//!   work happens behind the picture and not in front of a frozen view of the old place.
//! - While it is up nothing happens in the world: the systems that wait for
//!   `PlayState::Playing` stand still, scripts do not run (`script::run_scripts`), and
//!   Bevy's virtual clock is paused, so everything driven by `Time` (animations, doors,
//!   NPCs, the sky's clock, test aids) holds too. Streaming goes on: it is what is being
//!   waited for.
//! - It comes down when physics has every cell around the player
//!   (`PhysState::loading_done`) and the renderer has drawn its cells (it reports through
//!   [`Loading::render_busy`]), [`SETTLE_SECS`] later so that textures and meshes reach
//!   the graphics card behind the picture.
//! - The picture and its text are a loading screen record of the plugins
//!   (`fnv_world::loadscreen`): one that belongs to the destination, or one for anywhere.
//!   The layout is ours: the picture fills the window (cropped, not stretched), the text
//!   in a band along the bottom. The original's roulette wheel and progress are not drawn.
//! - A [`CoverRequest`] brings the same screen up with no trip: it holds the game as
//!   above and stays until an [`Uncover`]. nvforever puts it up while loading a save or
//!   going back to the main menu, which start the program again behind it
//!   (nvforever `handoff.rs`); [`Loading::unseen`] is that new run's own start while its
//!   window is still hidden, when streaming may take long frames as behind a screen.

use crate::phys::PhysState;
use crate::{Location, PlaySet, PlayState, PlayWorld, Travel};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::PrimaryWindow;
use fnv_world::loadscreen::{choose, load_screens, Destination, LoadScreen};
use nvf_assets::{GameData, TextureCache};
use std::collections::HashMap;

/// Frames the screen is up before the travel is applied [engineering choice: the frame
/// it is spawned in and one more, so the picture is on screen before the long frames].
pub const SHOW_FRAMES: u32 = 2;

/// Real seconds (and at least [`SETTLE_FRAMES`] frames) the screen stays after the last
/// cell is in [engineering choice].
pub const SETTLE_SECS: f64 = 0.25;
pub const SETTLE_FRAMES: u32 = 3;

/// Real seconds after which a loading screen comes down whatever is still missing
/// (logged as a warning), so a cell that never loads cannot hold the game forever.
pub const TIMEOUT_SECS: f64 = 120.0;

/// Move the player to another place behind a loading screen; see [`Travel`] for the
/// fields. Sent by activation and scripts; [`LoadingPlugin`] turns it into the `Travel`.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct TravelRequest(pub Travel);

/// Bring a loading screen up with no travel (module notes): its picture is one for `to`
/// (or for anywhere without it); it stays until [`Uncover`].
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct CoverRequest {
    pub to: Option<Destination>,
}

/// Take down the screen a [`CoverRequest`] put up, and let the game go on.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Uncover;

/// The trip a loading screen is up for.
#[derive(Debug, Clone, Copy)]
struct Trip {
    travel: Travel,
    /// The `Travel` has gone out.
    sent: bool,
    /// Frames since the screen came up.
    frames: u32,
    /// Real time the screen came up.
    started: f64,
    /// Since when everything has been loaded (real time, frames).
    ready: Option<(f64, u32)>,
}

/// The loading screen's state.
#[derive(Resource, Default)]
pub struct Loading {
    trip: Option<Trip>,
    /// A screen is up for a [`CoverRequest`].
    cover: bool,
    /// Nothing of the game is on screen yet (nvforever `handoff.rs`: a start whose window
    /// is still hidden): loading may take long frames, as behind a screen.
    pub unseen: bool,
    /// The renderer still has cells to draw around the camera. Set by the renderer every
    /// frame after its streaming (nvforever `loading_gate`); stays false without one.
    pub render_busy: bool,
    /// Loading screens shown so far.
    pub shown: u32,
    /// Picture sizes by texture (the image's own data leaves the main world once it is on
    /// the graphics card).
    sizes: HashMap<AssetId<Image>, Vec2>,
}

impl Loading {
    /// Whether a loading screen is up.
    pub fn active(&self) -> bool {
        self.trip.is_some() || self.cover
    }

    /// Whether a loading screen is up for travel (not for a [`CoverRequest`]).
    pub fn travelling(&self) -> bool {
        self.trip.is_some()
    }

    /// Whether a long frame would not be seen: a screen is up, or nothing is shown yet.
    pub fn unseen_frames(&self) -> bool {
        self.active() || self.unseen
    }
}

/// The loading screens of the plugins.
#[derive(Resource)]
struct Screens(Vec<LoadScreen>);

#[derive(Component)]
struct LoadingRoot;

/// The picture, with its size in texels.
#[derive(Component)]
struct Picture(Vec2);

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Loading>()
            .add_message::<TravelRequest>()
            .add_message::<CoverRequest>()
            .add_message::<Uncover>()
            .add_systems(Startup, read_screens)
            .add_systems(Update, (send.in_set(PlaySet::Input), (begin, hold, fit_picture).chain().in_set(PlaySet::Sync)))
            .add_systems(Last, finish);
    }
}

fn read_screens(mut commands: Commands, d: Option<Res<PlayWorld>>) {
    let Some(d) = d else { return };
    let screens = load_screens(d.lo, d.schema);
    let placed = screens.iter().filter(|s| !s.locations.is_empty()).count();
    info!("loading screens: {} in the plugins, {placed} of them for particular places", screens.len());
    commands.insert_resource(Screens(screens));
}

/// The size and offset of a `picture` scaled to cover a `window` without changing its
/// shape: the overflow is cropped evenly on both sides.
pub fn cover(picture: Vec2, window: Vec2) -> (Vec2, Vec2) {
    if picture.x <= 0.0 || picture.y <= 0.0 {
        return (window, Vec2::ZERO);
    }
    let size = picture * (window.x / picture.x).max(window.y / picture.y);
    (size, (window - size) / 2.0)
}

/// Where a travel leaving `from` arrives, for the choice of screen.
pub fn destination(t: &Travel, from: Location) -> Destination {
    let to = t.destination(from);
    Destination { interior: to.interior, exterior: to.interior.is_none().then(|| (to.world, fnv_world::cell_of(t.feet[0], t.feet[1]))) }
}

/// A number that differs from one loading screen to the next.
fn roll(n: u32) -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    // SplitMix64's finaliser.
    let mut z = t.wrapping_add(u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// What drawing a screen needs.
#[derive(bevy::ecs::system::SystemParam)]
struct Art<'w> {
    screens: Option<Res<'w, Screens>>,
    data: Option<Res<'w, GameData>>,
    textures: Option<ResMut<'w, TextureCache>>,
    images: Option<ResMut<'w, Assets<Image>>>,
    formats: Option<Res<'w, CompressedImageFormatSupport>>,
}

/// A travel was asked for: bring the screen up and hold the game.
#[allow(clippy::too_many_arguments)]
fn begin(
    mut commands: Commands,
    mut requests: MessageReader<TravelRequest>,
    mut loading: ResMut<Loading>,
    location: Res<Location>,
    real: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut next: ResMut<NextState<PlayState>>,
    mut art: Art,
) {
    let Some(TravelRequest(travel)) = requests.read().last().copied() else { return };
    if let Some(trip) = loading.trip.as_mut() {
        // Another travel under the screen that is up: it replaces the one waiting, or
        // goes out next frame.
        (trip.travel, trip.sent, trip.frames, trip.ready) = (travel, false, trip.frames.max(SHOW_FRAMES), None);
        return;
    }
    if loading.cover {
        info!("loading screen: travel ignored under a screen held for a restart");
        return;
    }
    let to = destination(&travel, *location);
    spawn_screen(&mut commands, &mut loading, &mut art, Some(&to));
    clock.pause();
    next.set(PlayState::Loading);
    loading.trip = Some(Trip { travel, sent: false, frames: 0, started: real.elapsed_secs_f64(), ready: None });
}

/// A [`CoverRequest`]: the screen with no trip; an [`Uncover`]: down again.
#[allow(clippy::too_many_arguments)]
fn hold(
    mut commands: Commands,
    mut up: MessageReader<CoverRequest>,
    mut down: MessageReader<Uncover>,
    mut loading: ResMut<Loading>,
    mut clock: ResMut<Time<Virtual>>,
    mut next: ResMut<NextState<PlayState>>,
    mut art: Art,
    roots: Query<Entity, With<LoadingRoot>>,
) {
    let asked = up.read().last().copied();
    if down.read().last().is_some() && loading.cover {
        info!("loading screen down (held for a restart)");
        loading.cover = false;
        for e in &roots {
            commands.entity(e).despawn();
        }
        clock.unpause();
        next.set(PlayState::Playing);
        return;
    }
    let Some(CoverRequest { to }) = asked else { return };
    if loading.active() {
        // A travel's screen is up: it becomes this one, and stays.
        loading.trip = None;
        loading.cover = true;
        return;
    }
    spawn_screen(&mut commands, &mut loading, &mut art, to.as_ref());
    clock.pause();
    next.set(PlayState::Loading);
    loading.cover = true;
}

/// The screen's nodes: a picture for `to` (for anywhere without it) and its text.
fn spawn_screen(commands: &mut Commands, loading: &mut Loading, art: &mut Art, to: Option<&Destination>) {
    let anywhere = Destination { interior: None, exterior: None };
    let to = to.unwrap_or(&anywhere);
    loading.shown += 1;
    let screen = art.screens.as_deref().and_then(|s| choose(&s.0, to, roll(loading.shown)));
    let picture = match (screen.and_then(|s| s.texture.as_deref()), art.data.as_deref(), art.textures.as_mut(), art.images.as_mut()) {
        (Some(path), Some(data), Some(textures), Some(images)) => textures.get(path, true, data, images, art.formats.as_deref()).map(|h| {
            let size = images.get(&h).map(|i| i.size().as_vec2());
            let size = *loading.sizes.entry(h.id()).or_insert(size.unwrap_or(Vec2::new(2.0, 1.0)));
            (h, size)
        }),
        _ => None,
    };
    info!(
        "loading screen up: {} ({}) for {}",
        screen.map_or("none".to_string(), |s| s.edid.clone().unwrap_or_else(|| format!("{:08X}", s.form_id))),
        match (screen.and_then(|s| s.texture.as_deref()), picture.is_some()) {
            (Some(p), true) => p.to_string(),
            (Some(p), false) => format!("{p}: not shown"),
            (None, _) => "no picture".to_string(),
        },
        match (to.interior, to.exterior) {
            (Some(c), _) => format!("interior {c:08X}"),
            (None, Some((w, c))) => format!("worldspace {w:08X} {c:?}"),
            (None, None) => "anywhere".to_string(),
        },
    );
    let font = |size: f32| TextFont { font_size: FontSize::Px(size), ..default() };
    let root = commands
        .spawn((
            LoadingRoot,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), overflow: Overflow::clip(), ..default() },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(200),
        ))
        .id();
    if let Some((image, size)) = picture {
        // Sized by `fit_picture` (this frame too).
        commands.spawn((Picture(size), ImageNode::new(image), Node { position_type: PositionType::Absolute, ..default() }, ChildOf(root)));
    }
    let text = screen.map(|s| s.text.trim()).filter(|t| !t.is_empty());
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::axes(Val::Percent(8.0), Val::Px(18.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        ChildOf(root),
        children![
            (Text::new(text.unwrap_or_default()), font(18.0), TextColor(Color::srgb(1.0, 0.75, 0.30))),
            (Text::new("Loading..."), font(14.0), TextColor(Color::srgb(0.85, 0.95, 0.80))),
        ],
    ));
}

/// Keep the picture covering the window.
fn fit_picture(windows: Query<&Window, With<PrimaryWindow>>, mut pictures: Query<(&Picture, &mut Node)>) {
    let Ok(window) = windows.single() else { return };
    for (p, mut node) in &mut pictures {
        let (size, at) = cover(p.0, Vec2::new(window.width(), window.height()));
        let want = (Val::Px(size.x), Val::Px(size.y), Val::Px(at.x), Val::Px(at.y));
        if (node.width, node.height, node.left, node.top) != want {
            (node.width, node.height, node.left, node.top) = want;
        }
    }
}

/// Once the screen has been drawn: the travel itself, for physics and the renderer.
/// Test aid `NVF_PLAY_TEST_LOADING_SHOT=<png>`: save the window as it is then (the
/// second screen of a run to `<name>-2.png`, and so on).
fn send(mut loading: ResMut<Loading>, mut travel: MessageWriter<Travel>, mut commands: Commands) {
    let shown = loading.shown;
    let Some(trip) = loading.trip.as_mut().filter(|t| !t.sent) else { return };
    trip.frames += 1;
    if trip.frames < SHOW_FRAMES {
        return;
    }
    trip.sent = true;
    travel.write(trip.travel);
    if let Some(path) = std::env::var_os("NVF_PLAY_TEST_LOADING_SHOT").map(std::path::PathBuf::from) {
        let path = match (shown, path.file_stem().and_then(|s| s.to_str())) {
            (2.., Some(stem)) => path.with_file_name(format!("{stem}-{shown}.png")),
            _ => path,
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        info!("test aid: loading screen screenshot {}", path.display());
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
}

/// Everything is in: take the screen down and let the game go on.
fn finish(
    mut commands: Commands,
    mut loading: ResMut<Loading>,
    phys: Option<Res<PhysState>>,
    real: Res<Time<Real>>,
    mut clock: ResMut<Time<Virtual>>,
    mut next: ResMut<NextState<PlayState>>,
    roots: Query<Entity, With<LoadingRoot>>,
) {
    let busy = loading.render_busy;
    let Some(trip) = loading.trip.as_mut().filter(|t| t.sent) else { return };
    let now = real.elapsed_secs_f64();
    let waited = now - trip.started;
    if busy || !phys.is_none_or(|p| p.loading_done()) {
        trip.ready = None;
        if waited < TIMEOUT_SECS {
            return;
        }
        warn!("loading screen: still loading after {waited:.0} s (renderer busy: {busy}); going on without it");
    } else {
        let (since, frames) = trip.ready.get_or_insert((now, 0));
        *frames += 1;
        if *frames < SETTLE_FRAMES || now - *since < SETTLE_SECS {
            return;
        }
    }
    info!("loading screen down after {waited:.2} s");
    loading.trip = None;
    for e in &roots {
        commands.entity(e).despawn();
    }
    clock.unpause();
    next.set(PlayState::Playing);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_covers_the_window() {
        let picture = Vec2::new(1024.0, 512.0);
        // A window less wide than the picture: full height, cropped left and right.
        let (size, at) = cover(picture, Vec2::new(1280.0, 800.0));
        assert_eq!((size, at), (Vec2::new(1600.0, 800.0), Vec2::new(-160.0, 0.0)));
        // A wider one: full width, cropped above and below.
        let (size, at) = cover(picture, Vec2::new(2400.0, 1000.0));
        assert_eq!((size, at), (Vec2::new(2400.0, 1200.0), Vec2::new(0.0, -100.0)));
        assert_eq!(cover(picture, Vec2::new(1000.0, 500.0)), (Vec2::new(1000.0, 500.0), Vec2::ZERO));
        assert_eq!(cover(Vec2::ZERO, Vec2::new(8.0, 6.0)), (Vec2::new(8.0, 6.0), Vec2::ZERO));
    }

    #[test]
    fn where_a_travel_arrives() {
        let outside = Location { world: 0xDA726, interior: None };
        let indoors = Travel { world: None, interior: Some(0x100), feet: [0.0; 3], heading_deg: 0.0 };
        assert_eq!(destination(&indoors, outside), Destination { interior: Some(0x100), exterior: None });
        let out = Travel { world: None, interior: None, feet: [-69_500.0, -5_000.0, 0.0], heading_deg: 0.0 };
        let inside = Location { world: 0xDA726, interior: Some(0x100) };
        assert_eq!(destination(&out, inside), Destination { interior: None, exterior: Some((0xDA726, (-17, -2))) });
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_resource(Location { world: 0xDA726, interior: None })
            .init_state::<PlayState>()
            .add_message::<Travel>()
            .configure_sets(Update, (PlaySet::Input, PlaySet::Sync).chain())
            .add_plugins(LoadingPlugin);
        // No plugins to read: no picture, the screen is still shown.
        app
    }

    fn sent(app: &mut App) -> usize {
        app.world_mut().resource_mut::<Messages<Travel>>().drain().count()
    }

    #[test]
    fn the_screen_is_up_before_the_travel_and_holds_the_game() {
        let mut app = app();
        app.update();
        let t = Travel { world: None, interior: Some(0x100), feet: [1.0, 2.0, 3.0], heading_deg: 90.0 };
        app.world_mut().write_message(TravelRequest(t));
        app.update();
        // Up at once, the clock stopped; the travel has not gone out.
        assert!(app.world().resource::<Loading>().active());
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(app.world_mut().query::<&LoadingRoot>().iter(app.world()).count(), 1);
        assert_eq!(sent(&mut app), 0);
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Loading);
        assert_eq!(sent(&mut app), 0);
        // The renderer has work from the travel on.
        app.world_mut().resource_mut::<Loading>().render_busy = true;
        app.update();
        assert_eq!(sent(&mut app), 1);
        for _ in 0..10 {
            app.update();
        }
        assert!(app.world().resource::<Loading>().active(), "held while the renderer loads");
        assert_eq!(sent(&mut app), 0);
        // Loaded: down after the settle time, and the game goes on.
        app.world_mut().resource_mut::<Loading>().render_busy = false;
        let t0 = std::time::Instant::now();
        while app.world().resource::<Loading>().active() {
            assert!(t0.elapsed().as_secs_f64() < 10.0, "the screen never came down");
            app.update();
        }
        assert!(t0.elapsed().as_secs_f64() >= SETTLE_SECS * 0.9);
        app.update();
        assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Playing);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(app.world_mut().query::<&LoadingRoot>().iter(app.world()).count(), 0);
    }
}
