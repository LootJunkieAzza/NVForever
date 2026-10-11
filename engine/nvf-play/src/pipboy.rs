//! The Pip-Boy (docs/spec-notes/pipboy.md): Tab raises the player's left arm with the
//! Pip-Boy to the view, and its screen shows the Pip-Boy menus, under three buttons on
//! the device (STATS, ITEMS, DATA), each with five tabs along the screen's foot.
//!
//! - **The menus are drawn into a texture** ([`PipBoyScreen`]): a 2D camera renders the
//!   menu's interface nodes into an image, and the player's body (nvforever,
//!   `pipboy_arm.rs`) puts that image on the Pip-Boy model's screen. The menus are laid
//!   out on a canvas 1024 × 768 [documented: `menus\globals.xml` `_pipboy_width` and
//!   `_pipboy_height`; the three menu files lay out inside it]. The image is 960 high so
//!   that a length of the interface (`interface::u`, 960 to the height of the view it is
//!   drawn in) is one of its pixels, and the canvas is its top left 1024 × 768 [design].
//!   Where nothing shows the model's screen (no body, no Pip-Boy worn) the image is drawn
//!   flat in the middle of the window instead [design].
//! - **Opening and closing.** Tab (the control Menu Mode) opens the player's inventory
//!   model (`menu::open_menus`, `Mode::Inventory`), which pauses the game
//!   (`PlayState::Menu`); while that model is open the Pip-Boy is up. One first-person
//!   clip, `characters\_1stperson\locomotion\male\pipboy.kf` (0.73 s,
//!   [`CLIP_SECONDS`]), both brings it up and puts it away: up to its `Hit` key (0.33 s,
//!   [`HOLD_SECONDS`]) the arm comes up until the screen is straight before the eye,
//!   facing it; after the key the arm goes away down [observed: the key; the screen's
//!   place against the camera through the clip, docs/spec-notes/pipboy.md §1]. So the
//!   clip is held at `Hit` while the menus are open and plays on from there when they
//!   close [hypothesis: the original holds it there]. The view is first person while
//!   it is up and goes back to what it was [hypothesis]. A script that has turned the
//!   Pip-Boy off keeps it down (`menu::open_menus`).
//! - **Pages.** [`Page`]: STATS, ITEMS and DATA, chosen by the buttons on the device (the
//!   body reports a click on one as [`PipBoyInput::Page`]) or F1, F2, F3 [hypothesis: the
//!   PC keys are not captured]; each page remembers its tab. The tabs: the names along
//!   the screen's foot, clicked, or A/D and Left/Right [design].
//! - **The pointer.** The window's pointer moves over the device; where it lies on the
//!   screen, it is passed to the menu's nodes as a pointer of their own
//!   ([`PIPBOY_POINTER`], Bevy's picking) so the menus take hovers and clicks as any
//!   other interface does. The body finds the spot on the model's screen
//!   ([`PipBoyScreen::pointer`]) on the shape as the arm's bones pose it (nvforever
//!   `pipboy_arm.rs`; Bevy's mesh ray casts see neither the skinning nor meshes kept on
//!   the GPU only, which is why clicks on the model did nothing until 2026-10-11); for
//!   the flat image this module finds it.
//! - **Drawing.** [`frame`] draws what every page shares (the background, the tab line)
//!   and gives the pages their tools; each page module ([`stats`], [`items`], [`data`])
//!   fills the [`PageBody`] node with its own system, so a page can read what it needs.

pub mod data;
pub mod frame;
pub mod items;
pub mod stats;
mod test_aid;

use crate::control::{PlayerControl, Pov};
use crate::menu::model::Mode;
use crate::menu::ActiveMenu;
use crate::PlaySet;
use bevy::asset::{uuid::Uuid, RenderAssetUsages};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::input::ButtonState;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::picking::PickingSystems;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::{PrimaryWindow, WindowEvent};

/// The Pip-Boy clip's length, seconds [observed: `pipboy.kf` `Pipboy` sequence, stop
/// time 0.7333].
pub const CLIP_SECONDS: f32 = 0.733_333_3;

/// Where the clip is held while the Pip-Boy is up: its `Hit` key [observed: 0.3333 s].
pub const HOLD_SECONDS: f32 = 0.333_333_3;

/// The raise clip [observed: archive file list].
pub const RAISE_CLIP: &str = "meshes\\characters\\_1stperson\\locomotion\\male\\pipboy.kf";

/// The canvas the Pip-Boy menus are laid out on, interface units [documented:
/// globals.xml].
pub const CANVAS: Vec2 = Vec2::new(1024.0, 768.0);

/// The image the menus are drawn into, pixels (one per interface unit; module docs).
pub const TARGET: UVec2 = UVec2::new(1280, 960);

/// The pointer on the Pip-Boy's screen, for Bevy's picking.
pub const PIPBOY_POINTER: PointerId = PointerId::Custom(Uuid::from_u128(0x6e76_665f_7069_7062_6f79_5f70_6f69_6e74));

/// The three pages, in the order of the device's buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Page {
    Stats,
    #[default]
    Items,
    Data,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Stats, Page::Items, Page::Data];

    /// The page's title at the screen's top left [observed: the owner's screenshots].
    pub fn title(self) -> &'static str {
        match self {
            Page::Stats => "STATS",
            Page::Items => "ITEMS",
            Page::Data => "DATA",
        }
    }

    /// The page's tabs, left to right [observed: the owner's screenshots].
    pub fn tabs(self) -> [&'static str; 5] {
        match self {
            Page::Stats => ["Status", "S.P.E.C.I.A.L.", "Skills", "Perks", "General"],
            Page::Items => ["Weapons", "Apparel", "Aid", "Misc", "Ammo"],
            Page::Data => ["Local Map", "World Map", "Quests", "Misc", "Radio"],
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Where the Pip-Boy is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Phase {
    /// Down: not shown.
    #[default]
    Down,
    /// Coming up to the view.
    Raising,
    /// Up, held before the view.
    Up,
    /// Going back down.
    Lowering,
}

/// The Pip-Boy's state.
#[derive(Resource, Debug, Default)]
pub struct PipBoy {
    pub phase: Phase,
    /// Seconds into the clip: from 0 coming up to [`HOLD_SECONDS`], held there while up,
    /// on to [`CLIP_SECONDS`] going down.
    pub clip_time: f32,
    pub page: Page,
    /// Each page's tab (0 to 4).
    pub tabs: [usize; 3],
    /// The view before the Pip-Boy came up, given back when it is down.
    saved_pov: Option<Pov>,
}

impl PipBoy {
    /// Whether any of the Pip-Boy shows (coming up, up, going down).
    pub fn shown(&self) -> bool {
        self.phase != Phase::Down
    }

    /// Whether the menus take input (coming up or up).
    pub fn open(&self) -> bool {
        matches!(self.phase, Phase::Raising | Phase::Up)
    }

    /// The tab of the page shown.
    pub fn tab(&self) -> usize {
        self.tabs[self.page.index()]
    }

    /// Move the clip on by `dt` seconds; true when the Pip-Boy reached the top or the
    /// bottom in this step.
    pub fn advance(&mut self, dt: f32) -> bool {
        match self.phase {
            Phase::Raising => {
                self.clip_time = (self.clip_time + dt).min(HOLD_SECONDS);
                if self.clip_time >= HOLD_SECONDS {
                    self.phase = Phase::Up;
                    return true;
                }
            }
            Phase::Lowering => {
                self.clip_time = (self.clip_time + dt).min(CLIP_SECONDS);
                if self.clip_time >= CLIP_SECONDS {
                    self.phase = Phase::Down;
                    self.clip_time = 0.0;
                    return true;
                }
            }
            Phase::Down | Phase::Up => {}
        }
        false
    }

    /// Come up. Put back up on the way down, it comes up from as far down as it got
    /// [design: the way down mirrored onto the way up].
    pub fn raise(&mut self) {
        match self.phase {
            Phase::Down => (self.phase, self.clip_time) = (Phase::Raising, 0.0),
            Phase::Lowering => (self.phase, self.clip_time) = (Phase::Raising, (2.0 * HOLD_SECONDS - self.clip_time).max(0.0)),
            Phase::Raising | Phase::Up => {}
        }
    }

    /// Go down: on from `Hit` (from where it got to, if it was still coming up
    /// [design]).
    pub fn lower(&mut self) {
        if self.open() {
            self.phase = Phase::Lowering;
            self.clip_time = self.clip_time.max(HOLD_SECONDS);
        }
    }

    /// Show a page, or step a page's tab.
    pub fn apply(&mut self, input: PipBoyInput) {
        match input {
            PipBoyInput::Page(p) => self.page = p,
            PipBoyInput::Tab(t) => self.tabs[self.page.index()] = t.min(4),
            PipBoyInput::StepTab(by) => {
                let t = &mut self.tabs[self.page.index()];
                *t = (*t as i32 + by).rem_euclid(5) as usize;
            }
        }
    }
}

/// What the player asks of the Pip-Boy's pages.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum PipBoyInput {
    /// Show this page (a button on the device, F1–F3).
    Page(Page),
    /// Show this tab of the page shown.
    Tab(usize),
    /// The next (1) or previous (-1) tab, round the ends.
    StepTab(i32),
}

/// The image the menus are drawn into, and where it is shown.
#[derive(Resource)]
pub struct PipBoyScreen {
    pub image: Handle<Image>,
    pub camera: Entity,
    /// The model's screen shows the image (the body sets it each frame).
    pub on_model: bool,
    /// Where the window's pointer lies on the canvas (interface units from its top left),
    /// if on it: set by the body from the model's screen, or here from the flat image.
    pub pointer: Option<Vec2>,
}

/// Test aid (`NVF_PLAY_TEST_PIPBOY`'s `point=` and `click=`): what the pointer is put on
/// in place of the window's, and a click to make there.
#[derive(Resource, Debug, Default)]
pub struct PipBoyAid {
    pub target: Option<AidTarget>,
    /// A click on the target in progress: 3 asked, 2 the pointer is on it, 1 pressed.
    pub click: u8,
}

/// Where the test aid points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AidTarget {
    /// The middle of a page's button on the device.
    Button(Page),
    /// A point of the canvas, on the screen (through the model where it is shown).
    Canvas(Vec2),
}

/// The node a page fills, under the menus' root (canvas units from its top left).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBody {
    pub page: Page,
    pub tab: usize,
}

/// The root of the menus drawn into the image.
#[derive(Component)]
pub struct PipBoyRoot;

/// The flat image in the window (no model screen).
#[derive(Component)]
struct FlatScreen;

pub struct PipBoyPlugin;

impl Plugin for PipBoyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PipBoy>()
            .init_resource::<PipBoyAid>()
            .init_resource::<frame::Kit>()
            .add_message::<PipBoyInput>()
            .add_systems(Startup, setup_screen)
            .add_systems(
                Update,
                (
                    follow_menu.after(crate::menu::MenuSystems),
                    (keys, apply_input).chain().run_if(pipboy_open),
                    items::keys,
                    advance,
                    camera_on,
                    flat_screen,
                    frame::draw,
                    (stats::draw, items::draw, data::draw),
                )
                    .chain()
                    .in_set(PlaySet::Ui),
            )
            .add_systems(First, drive_pointer.in_set(PickingSystems::Input));
        test_aid::setup(app);
    }
}

/// Whether the Pip-Boy's menus take input.
pub fn pipboy_open(pip: Res<PipBoy>) -> bool {
    pip.open()
}

fn setup_screen(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d { width: TARGET.x, height: TARGET.y, ..default() };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    let camera = commands
        .spawn((
            Name::new("Pip-Boy screen camera"),
            Camera2d,
            Camera { order: -2, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
        ))
        .id();
    commands.insert_resource(PipBoyScreen { image, camera, on_model: false, pointer: None });
}

/// The Pip-Boy is up while the player's inventory is open, and in first person.
fn follow_menu(active: Res<ActiveMenu>, mut pip: ResMut<PipBoy>, mut control: ResMut<PlayerControl>) {
    let inventory = active.0.as_ref().is_some_and(|m| m.mode == Mode::Inventory);
    if inventory && !pip.open() {
        if pip.phase == Phase::Down {
            pip.saved_pov = Some(control.pov);
            info!("pipboy: up, page {} tab {}", pip.page.title(), pip.page.tabs()[pip.tab()]);
        }
        pip.raise();
        control.pov = Pov::First;
        control.orbit = default();
    } else if !inventory && pip.open() {
        info!("pipboy: down");
        pip.lower();
    }
}

fn advance(time: Res<Time>, mut pip: ResMut<PipBoy>, mut control: ResMut<PlayerControl>) {
    if !matches!(pip.phase, Phase::Raising | Phase::Lowering) {
        return;
    }
    if pip.advance(time.delta_secs()) && pip.phase == Phase::Down {
        if let Some(p) = pip.saved_pov.take() {
            control.pov = p;
        }
    }
}

/// The screen camera draws only while the Pip-Boy shows.
fn camera_on(pip: Res<PipBoy>, screen: Option<Res<PipBoyScreen>>, mut cams: Query<&mut Camera>) {
    let Some(screen) = screen else { return };
    if let Ok(mut c) = cams.get_mut(screen.camera) {
        if c.is_active != pip.shown() {
            c.is_active = pip.shown();
        }
    }
}

/// Keys of the Pip-Boy: Tab, Esc and E close it; F1–F3 the pages; A/D and Left/Right
/// the tabs. The ITEMS page's own keys are in [`items`].
fn keys(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<PipBoyInput>, mut menu: MessageWriter<crate::menu::MenuInput>) {
    use KeyCode::*;
    if keys.any_just_pressed([Tab, Escape]) {
        menu.write(crate::menu::MenuInput(crate::menu::model::Command::Close));
        return;
    }
    for (k, p) in [(F1, Page::Stats), (F2, Page::Items), (F3, Page::Data)] {
        if keys.just_pressed(k) {
            out.write(PipBoyInput::Page(p));
        }
    }
    if keys.any_just_pressed([KeyA, ArrowLeft]) {
        out.write(PipBoyInput::StepTab(-1));
    }
    if keys.any_just_pressed([KeyD, ArrowRight]) {
        out.write(PipBoyInput::StepTab(1));
    }
}

fn apply_input(mut input: MessageReader<PipBoyInput>, mut pip: ResMut<PipBoy>) {
    for &i in input.read() {
        let before = (pip.page, pip.tab());
        pip.apply(i);
        if (pip.page, pip.tab()) != before {
            info!("pipboy: {} {}", pip.page.title(), pip.page.tabs()[pip.tab()]);
        }
    }
}

/// Where nothing shows the model's screen, the image is drawn flat in the middle of the
/// window, 4:3 and as high as the window allows; the pointer on it is found here.
fn flat_screen(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<ResMut<PipBoyScreen>>,
    flat: Query<(Entity, &ComputedNode, &UiGlobalTransform), With<FlatScreen>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    aid: Res<PipBoyAid>,
) {
    let Some(mut screen) = screen else { return };
    let want = pip.open() && !screen.on_model;
    match (want, flat.iter().next()) {
        (true, None) => {
            let node = Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                height: Val::Vh(90.0),
                aspect_ratio: Some(CANVAS.x / CANVAS.y),
                margin: UiRect { left: Val::Vh(-60.0), top: Val::Vh(-45.0), ..default() },
                ..default()
            };
            let crop = Rect::new(0.0, 0.0, CANVAS.x, CANVAS.y);
            commands.spawn((FlatScreen, node, ImageNode { rect: Some(crop), ..ImageNode::new(screen.image.clone()) }, GlobalZIndex(90)));
        }
        (false, Some((e, ..))) => {
            commands.entity(e).despawn();
        }
        (true, Some((_, computed, at))) => {
            if let Some(AidTarget::Canvas(p)) = aid.target {
                screen.pointer = Some(p);
                return;
            }
            let cursor = windows.single().ok().and_then(|w| w.physical_cursor_position());
            let size = computed.size();
            screen.pointer = cursor.and_then(|c| {
                let local = c - (at.translation - size / 2.0);
                let p = local / size * CANVAS;
                (p.x >= 0.0 && p.y >= 0.0 && p.x < CANVAS.x && p.y < CANVAS.y).then_some(p)
            });
        }
        (false, None) => {}
    }
}

/// Pass the pointer on the screen, and the mouse's buttons, to the menus' nodes (as Bevy's
/// `render_ui_to_texture` example does).
fn drive_pointer(
    mut commands: Commands,
    pip: Res<PipBoy>,
    screen: Option<Res<PipBoyScreen>>,
    mut last: Local<Option<Vec2>>,
    mut spawned: Local<bool>,
    mut window_events: MessageReader<WindowEvent>,
    mut out: MessageWriter<PointerInput>,
    mut aid: ResMut<PipBoyAid>,
) {
    let Some(screen) = screen else { return };
    if !*spawned {
        commands.spawn(PIPBOY_POINTER);
        *spawned = true;
    }
    let target = RenderTarget::Image(ImageRenderTarget::from(screen.image.clone()));
    let Some(target) = target.normalize(None) else { return };
    let at = screen.pointer.filter(|_| pip.open());
    // Off the screen the pointer goes to a corner no node is in.
    let position = at.unwrap_or(Vec2::new(TARGET.x as f32 - 1.0, TARGET.y as f32 - 1.0));
    if Some(position) != *last {
        let delta = last.map_or(Vec2::ZERO, |l| position - l);
        out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, PointerAction::Move { delta }));
        *last = Some(position);
    }
    for e in window_events.read() {
        let WindowEvent::MouseButtonInput(input) = e else { continue };
        if at.is_none() {
            continue;
        }
        let button = match input.button {
            MouseButton::Left => PointerButton::Primary,
            MouseButton::Right => PointerButton::Secondary,
            MouseButton::Middle => PointerButton::Middle,
            _ => continue,
        };
        let action = match input.state {
            ButtonState::Pressed => PointerAction::Press(button),
            ButtonState::Released => PointerAction::Release(button),
        };
        out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, action));
    }
    // The test aid's click: a frame on the spot first, then a press, then a release.
    match aid.click {
        3 => aid.click = 2,
        1 | 2 if at.is_none() => {
            warn!("test aid: pipboy click at {:?}: not on the screen", aid.target);
            aid.click = 0;
        }
        2 => {
            out.write(PointerInput::new(PIPBOY_POINTER, Location { target: target.clone(), position }, PointerAction::Press(PointerButton::Primary)));
            aid.click = 1;
        }
        1 => {
            out.write(PointerInput::new(PIPBOY_POINTER, Location { target, position }, PointerAction::Release(PointerButton::Primary)));
            info!("test aid: pipboy clicked the screen at ({:.0}, {:.0})", position.x, position.y);
            aid.click = 0;
        }
        _ => {}
    }
}

/// The page or tab clicked on the screen: a node with this is clickable.
#[derive(Component, Clone, Copy, Debug)]
pub struct Clickable(pub PipBoyInput);

/// Send a [`Clickable`]'s input when it is clicked.
pub fn on_click(click: On<Pointer<Click>>, nodes: Query<&Clickable>, mut out: MessageWriter<PipBoyInput>) {
    if let Ok(c) = nodes.get(click.entity) {
        out.write(c.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clip_holds_at_hit_while_up() {
        let mut p = PipBoy::default();
        p.raise();
        assert_eq!(p.phase, Phase::Raising);
        assert!(!p.advance(0.2));
        assert!(p.advance(0.2));
        assert_eq!((p.phase, p.clip_time), (Phase::Up, HOLD_SECONDS));
        // Held there.
        assert!(!p.advance(5.0));
        assert_eq!(p.clip_time, HOLD_SECONDS);
        // Down: on from Hit to the end.
        p.lower();
        assert!(!p.advance(0.1));
        assert!((p.clip_time - (HOLD_SECONDS + 0.1)).abs() < 1e-6);
        // Put back up on the way down: up from as far down as it got.
        p.raise();
        assert_eq!(p.phase, Phase::Raising);
        assert!((p.clip_time - (HOLD_SECONDS - 0.1)).abs() < 1e-6);
        assert!(p.advance(0.1));
        p.lower();
        assert!(p.advance(1.0));
        assert_eq!((p.phase, p.clip_time), (Phase::Down, 0.0));
    }

    #[test]
    fn each_page_keeps_its_tab() {
        let mut p = PipBoy::default();
        p.apply(PipBoyInput::Tab(2));
        p.apply(PipBoyInput::Page(Page::Stats));
        assert_eq!(p.tab(), 0);
        p.apply(PipBoyInput::StepTab(-1));
        assert_eq!(p.tab(), 4);
        p.apply(PipBoyInput::Page(Page::Items));
        assert_eq!(p.tab(), 2);
        p.apply(PipBoyInput::StepTab(3));
        assert_eq!(p.tab(), 0);
    }
}
