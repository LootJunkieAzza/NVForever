//! The Pip-Boy glance (the co-op fork's own [design: owner 2026-11-03, in place of the
//! Realtime Pip-Boy Map, Real Time Equipment and Holo Panel mods, which need a script
//! extender and fight the Pip-Boy menus]): Pip-Boy Glance (G, rebindable) raises the arm
//! while the player keeps walking and looking around, with no menu and no pause, and
//! puts it down again. The full Pip-Boy (Tab) stays the menus.
//!
//! - **The arm.** The same first-person clip as the full Pip-Boy (`super::RAISE_CLIP`),
//!   held at [`GLANCE_HOLD`] instead of its `Hit` key, so the screen stays lower in the
//!   view and the world is seen over it [design]; going down it plays on from the
//!   matching point after `Hit` (the way down taken as the way up mirrored, as
//!   [`super::PipBoy::raise`] does). The view is first person while it is up and goes
//!   back to what it was. Attacking or aiming puts it down [design].
//! - **When.** Only while playing: no menu open, the game not paused, a Pip-Boy worn
//!   and the Use Pip-Boy setting on (otherwise there is no arm to raise), and the
//!   Gameplay setting Pip-Boy Glance on. Opening the full Pip-Boy takes the arm over.
//! - **The screen** shows two kinds of tab, along its foot: **MAP**, a live local map
//!   centred on the player and turned with the player's heading (forward is up), with
//!   the outline of the place, the doors leading elsewhere, the known map markers, the
//!   active quest's markers and the other co-op players ([`render_map`]; drawn on the
//!   CPU into an image, a few times a second); and the inventory's categories
//!   (**WEAPONS**, **APPAREL**, **AID**, **MISC**, **AMMO**), a wrist inventory to
//!   browse, equip, use and drop from while moving (the inventory menu's own rules,
//!   `crate::menu::model::MenuModel`).
//! - **Keys** [design]: Left/Right change the tab, Up/Down choose a row (on the map: zoom
//!   in and out), Enter equips or uses the row, Backspace or Delete drops one. On a
//!   controller the D-pad does the same, X equips or uses and Y drops; while the glance
//!   is up those buttons are its own (`crate::bindings::ControlHold`), the movement and
//!   the look stay the player's.
//! - Logged: `pipboy glance: up`, `pipboy glance: down`, `pipboy glance: <tab>`,
//!   `pipboy glance: <command> <item>: <status>`.

use super::frame::{Align, Ctx, Kit, FILL, LINE, MAIN, TEXT, THICK};
use super::{Phase, PipBoy, PipBoyScreen, UsePipBoy, CANVAS, CLIP_SECONDS, HOLD_SECONDS};
use crate::bindings::{Control, ControlHold, ControlInput};
use crate::control::{PlayerControl, Pov};
use crate::interface::{at, pipboy_amber as amber};
use crate::menu::model::{Category, Command, MenuModel, Side};
use crate::menu::{ActiveMenu, LiveMenu};
use crate::sim::SimState;
use crate::{ItemUsed, PlayState, RefChanged};
use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_sim::FormId;
use nvf_assets::GameData;

/// Where the arm is held while the glance is up, seconds into the raise clip: before the
/// clip's `Hit` (0.333 s, the screen straight before the eye) [design: the screen low and
/// left in the view, the world seen over it; tuned against the owner's reference
/// screenshot, 2026-11-04, together with the arms' wider view while the glance is up
/// (nvforever `player_body.rs` `GLANCE_FOV`)].
pub const GLANCE_HOLD: f32 = 0.22;

/// The setting's key (`crate::settings`).
pub const SETTING_KEY: &str = "PipBoyGlance";

/// The tabs along the screen's foot: the map, then the inventory's categories.
pub const TABS: [&str; 6] = ["MAP", "WEAPONS", "APPAREL", "AID", "MISC", "AMMO"];

/// The map's picture, pixels, and where it lies on the canvas.
pub const MAP_PX: [usize; 2] = [640, 400];
const MAP_RECT: Rect = Rect { min: Vec2::new(MAIN.min.x, 110.0), max: Vec2::new(MAIN.max.x, 110.0 + 855.0 * 400.0 / 640.0) };
/// Game units a map pixel shows at each zoom step [design: the nearest about the DATA
/// page's local map at its closest, the farthest a little more than the cells loaded].
pub const ZOOMS: [f32; 4] = [8.0, 16.0, 32.0, 64.0];
/// Indoors the map is drawn this much closer, as the DATA page's (`outline::INTERIOR_SCALE`
/// against its 28 outdoors).
const INTERIOR_CLOSER: f32 = 3.5;
/// How often the map's picture is drawn again, seconds.
const MAP_EVERY: f32 = 0.1;
/// How often the wrist inventory is read again while shown, seconds.
const ITEMS_EVERY: f32 = 1.0;
/// Rows of the wrist inventory shown at once.
const ROWS: usize = 9;

/// A mark on the live map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// A door leading elsewhere.
    Door,
    /// A map marker the player knows: found (been there) or only shown.
    Place { found: bool },
    /// The active quest's target.
    Quest,
    /// Another co-op player.
    Mate,
}

/// A mark at a game point (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveMark {
    pub kind: MarkKind,
    pub pos: [f32; 2],
}

/// The outline of the place round the player (`data::outline`), as the map samples it.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveOutline {
    /// The grid's top left corner, game x and y (y the largest).
    pub origin: [f32; 2],
    pub step: f32,
    pub size: [usize; 2],
    /// Alpha row by row from the top.
    pub alpha: std::sync::Arc<Vec<u8>>,
}

impl LiveOutline {
    /// The alpha at game point (x, y), nothing outside the grid.
    pub fn at(&self, x: f32, y: f32) -> u8 {
        let col = ((x - self.origin[0]) / self.step).floor();
        let row = ((self.origin[1] - y) / self.step).floor();
        if col < 0.0 || row < 0.0 || col >= self.size[0] as f32 || row >= self.size[1] as f32 {
            return 0;
        }
        self.alpha.get(row as usize * self.size[0] + col as usize).copied().unwrap_or(0)
    }
}

/// What the live map shows (gathered by `data::live_map`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveMap {
    pub interior: bool,
    pub feet: [f32; 3],
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    pub outline: Option<LiveOutline>,
    pub marks: Vec<LiveMark>,
}

/// The other co-op players, for the map (written by nvforever `coop.rs`).
#[derive(Resource, Default, Debug, Clone)]
pub struct CoopMates(pub Vec<Mate>);

#[derive(Debug, Clone, PartialEq)]
pub struct Mate {
    pub name: String,
    /// The interior it is in, or `None` outdoors in `world`.
    pub interior: Option<u32>,
    pub world: u32,
    pub feet: [f32; 3],
    /// Its facing (radians, compass), for where a trip to it arrives.
    pub yaw: f32,
    /// Its reference in this world: the map's fast travel to this player asks for it
    /// (`crate::pipboy::data::travel`).
    pub reference: u32,
    /// The head it chose for the map (`crate::coop_heads`), empty for the plain Vault Boy.
    pub head: String,
    /// In combat, or somewhere no one may fast travel to it (a conversation, a scene, fast
    /// travel turned off): no trip to it now.
    pub combat: bool,
    pub no_travel: bool,
}

/// The glance's state.
#[derive(Resource, Debug)]
pub struct Glance {
    /// The setting (Gameplay: Pip-Boy Glance).
    pub on: bool,
    pub phase: Phase,
    /// Seconds into the raise clip ([`GLANCE_HOLD`] while up).
    pub clip_time: f32,
    /// The tab shown ([`TABS`]).
    pub tab: usize,
    /// The map's zoom step ([`ZOOMS`]).
    pub zoom: usize,
    /// The view before the glance came up.
    saved_pov: Option<Pov>,
    /// The wrist inventory, while up.
    pub model: Option<MenuModel>,
    /// The last command's result.
    pub status: Option<String>,
    /// Seconds since the inventory and the map were last read.
    items_age: f32,
    map_age: f32,
}

impl Default for Glance {
    fn default() -> Self {
        Glance { on: true, phase: Phase::Down, clip_time: 0.0, tab: 0, zoom: 1, saved_pov: None, model: None, status: None, items_age: 0.0, map_age: f32::MAX }
    }
}

impl Glance {
    /// Whether any of the arm shows.
    pub fn shown(&self) -> bool {
        self.phase != Phase::Down
    }

    /// Up or coming up: the screen takes the keys.
    pub fn open(&self) -> bool {
        matches!(self.phase, Phase::Raising | Phase::Up)
    }

    /// Come up (from as far down as it got).
    pub fn raise(&mut self) {
        match self.phase {
            Phase::Down => (self.phase, self.clip_time) = (Phase::Raising, 0.0),
            Phase::Lowering => (self.phase, self.clip_time) = (Phase::Raising, (2.0 * HOLD_SECONDS - self.clip_time).clamp(0.0, GLANCE_HOLD)),
            Phase::Raising | Phase::Up => {}
        }
    }

    /// Go down: on from the point after `Hit` that mirrors where the arm is.
    pub fn lower(&mut self) {
        if self.open() {
            self.phase = Phase::Lowering;
            self.clip_time = (2.0 * HOLD_SECONDS - self.clip_time).max(HOLD_SECONDS);
        }
    }

    /// Down at once (the full Pip-Boy takes the arm).
    pub fn drop_now(&mut self) {
        (self.phase, self.clip_time) = (Phase::Down, 0.0);
    }

    /// Move the clip on; true when it reached the top or the bottom.
    pub fn advance(&mut self, dt: f32) -> bool {
        match self.phase {
            Phase::Raising => {
                self.clip_time = (self.clip_time + dt).min(GLANCE_HOLD);
                if self.clip_time >= GLANCE_HOLD {
                    self.phase = Phase::Up;
                    return true;
                }
            }
            Phase::Lowering => {
                self.clip_time = (self.clip_time + dt).min(CLIP_SECONDS);
                if self.clip_time >= CLIP_SECONDS {
                    (self.phase, self.clip_time) = (Phase::Down, 0.0);
                    return true;
                }
            }
            Phase::Down | Phase::Up => {}
        }
        false
    }

    /// The inventory category of the tab shown, `None` on the map.
    pub fn category(&self) -> Option<Category> {
        (self.tab > 0).then(|| Category::ORDER.get(self.tab).copied()).flatten()
    }

    /// Step the tab (round the ends).
    pub fn step_tab(&mut self, by: i32) {
        self.tab = (self.tab as i32 + by).rem_euclid(TABS.len() as i32) as usize;
    }
}

/// Where the first-person arm's Pip-Boy clip is: the full Pip-Boy's on the arm, else the
/// glance's (nvforever `player_body.rs`).
pub fn arm_clip(pip: &PipBoy, glance: &Glance) -> Option<f32> {
    if pip.on_arm() {
        Some(pip.clip_time)
    } else {
        glance.shown().then_some(glance.clip_time)
    }
}

/// What a player's Pip-Boy is doing, as the other co-op players see it (nvforever
/// `coop.rs`, `pipboy_check.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Check {
    #[default]
    None,
    /// The glance (G).
    Glance,
    /// The full Pip-Boy (Tab), on the arm or the screen menu.
    Full,
}

impl Check {
    /// This game's player's.
    pub fn of(pip: &PipBoy, glance: &Glance) -> Check {
        if pip.open() {
            Check::Full
        } else if glance.open() {
            Check::Glance
        } else {
            Check::None
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Check {
        match c {
            1 => Check::Glance,
            2 => Check::Full,
            _ => Check::None,
        }
    }
}

/// The root of the glance's screen, drawn into the Pip-Boy's image.
#[derive(Component)]
pub struct GlanceRoot;

/// The map's picture node.
#[derive(Component)]
pub(super) struct MapPicture;

/// The picture the map is drawn into.
#[derive(Resource)]
pub struct MapImage(pub Handle<Image>);

pub(super) fn setup(app: &mut App) {
    app.init_resource::<Glance>().init_resource::<CoopMates>().init_resource::<ControlHold>().add_systems(Startup, make_image);
    crate::settings::add_setting(
        app,
        crate::settings::Setting {
            key: SETTING_KEY,
            group: crate::pause::GROUP_MENUS,
            label: "Pip-Boy glance",
            note: "On: Pip-Boy Glance (G) raises the arm while you keep moving: a live map and a wrist inventory, no menu, no pause. Left/Right tabs, Up/Down rows, Enter uses, Backspace drops (D-pad, X, Y on a controller).",
            switches: "",
            get: |w| w.resource::<Glance>().on,
            set: |w, on| w.resource_mut::<Glance>().on = on,
            slider: None,
        },
    );
}

fn make_image(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d { width: MAP_PX[0] as u32, height: MAP_PX[1] as u32, ..default() };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = bevy::image::ImageSampler::linear();
    commands.insert_resource(MapImage(images.add(image)));
}

/// Whether the glance may be up now.
#[allow(clippy::too_many_arguments)]
fn allowed(glance: &Glance, state: PlayState, active: &ActiveMenu, paused: bool, pip: &PipBoy, use_pipboy: &UsePipBoy, sim: Option<&SimState>, control: &PlayerControl) -> bool {
    glance.on && state == PlayState::Playing && active.0.is_none() && !paused && !pip.shown() && use_pipboy.on && sim.is_some_and(super::wears_pipboy) && control.portrait.is_none() && !control.cinematic && !control.end_view
}

/// G raises and lowers the glance; anything that leaves no room for it puts it down. Runs
/// before the full Pip-Boy follows the menus, so the view the glance took is given back
/// first and the full Pip-Boy keeps the player's own.
#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    input: ControlInput,
    time: crate::pause::AidTime,
    state: Res<State<PlayState>>,
    active: Res<ActiveMenu>,
    paused: Res<crate::pause::Paused>,
    pip: Res<PipBoy>,
    use_pipboy: Res<UsePipBoy>,
    sim: Option<Res<SimState>>,
    mut control: ResMut<PlayerControl>,
    mut glance: ResMut<Glance>,
) {
    let ok = allowed(&glance, *state.get(), &active, paused.0, &pip, &use_pipboy, sim.as_deref(), &control);
    if glance.shown() && (!ok || active.0.is_some()) {
        // The full Pip-Boy (or another menu) takes over: the arm is gone at once.
        glance.drop_now();
        glance.model = None;
        if let Some(p) = glance.saved_pov.take() {
            control.pov = p;
        }
        info!("pipboy glance: down (it gives way)");
        return;
    }
    let pressed = input.just_pressed(Control::PipBoyGlance);
    if pressed && ok && !glance.open() {
        if glance.phase == Phase::Down {
            glance.saved_pov = Some(control.pov);
        }
        glance.raise();
        control.pov = Pov::First;
        control.orbit = default();
        glance.items_age = f32::MAX;
        glance.map_age = f32::MAX;
        info!("pipboy glance: up, {}", TABS[glance.tab]);
    } else if glance.open() && (pressed || input.just_pressed(Control::Use) || input.just_pressed(Control::Block)) {
        glance.lower();
        info!("pipboy glance: down");
    }
    if glance.advance(time.delta_secs_f64() as f32) && glance.phase == Phase::Down {
        glance.model = None;
        if let Some(p) = glance.saved_pov.take() {
            control.pov = p;
        }
    }
    // The view stays first person while the arm shows (a view switch is turned back).
    if glance.shown() {
        control.pov = Pov::First;
    }
}

/// The controls an open view holds back: everything under the full Pip-Boy opened
/// without a pause (`crate::menu::LiveMenu`), the glance's own buttons on a controller.
pub(super) fn hold_controls(live: Option<Res<LiveMenu>>, glance: Res<Glance>, bindings: Res<crate::bindings::Bindings>, mut hold: ResMut<ControlHold>) {
    let want = ControlHold { all: live.is_some_and(|l| l.open), glance_pad: glance.open() && bindings.device == crate::bindings::InputDevice::Controller };
    if *hold != want {
        *hold = want;
    }
}

/// What the player asks of the glance's screen this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Tab(i32),
    Row(i32),
    Use,
    Drop,
}

/// The keyboard's and the controller's asks ([`Ask`]).
pub fn asks(keys: &ButtonInput<KeyCode>, pads: &[&Gamepad], controller: bool) -> Vec<Ask> {
    use KeyCode::*;
    let mut out = Vec::new();
    let pad = |b: GamepadButton| controller && pads.iter().any(|g| g.just_pressed(b));
    if keys.just_pressed(ArrowLeft) || pad(GamepadButton::DPadLeft) {
        out.push(Ask::Tab(-1));
    }
    if keys.just_pressed(ArrowRight) || pad(GamepadButton::DPadRight) {
        out.push(Ask::Tab(1));
    }
    if keys.just_pressed(ArrowUp) || pad(GamepadButton::DPadUp) {
        out.push(Ask::Row(-1));
    }
    if keys.just_pressed(ArrowDown) || pad(GamepadButton::DPadDown) {
        out.push(Ask::Row(1));
    }
    if keys.any_just_pressed([Enter, NumpadEnter]) || pad(GamepadButton::West) {
        out.push(Ask::Use);
    }
    if keys.any_just_pressed([Backspace, Delete]) || pad(GamepadButton::North) {
        out.push(Ask::Drop);
    }
    out
}

/// The glance's keys: tabs, rows, use and drop on the wrist inventory (the inventory
/// menu's rules), the map's zoom.
#[allow(clippy::too_many_arguments)]
pub(super) fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    bindings: Res<crate::bindings::Bindings>,
    time: Res<Time<Real>>,
    mut glance: ResMut<Glance>,
    sim: Option<ResMut<SimState>>,
    mut used: MessageWriter<ItemUsed>,
    mut changed: MessageWriter<RefChanged>,
    script: Option<ResMut<crate::script::ScriptState>>,
) {
    if !glance.open() {
        return;
    }
    let Some(mut sim) = sim else { return };
    let g = &mut *glance;
    let pads: Vec<&Gamepad> = pads.iter().collect();
    let asks = asks(&keys, &pads, bindings.device == crate::bindings::InputDevice::Controller);
    let dt = time.delta_secs();
    g.items_age += dt;
    let tab_before = g.tab;
    let mut script = script.map(|s| s.into_inner());
    for a in &asks {
        if let Ask::Tab(by) = a {
            g.step_tab(*by);
        }
    }
    if g.tab != tab_before {
        g.status = None;
        info!("pipboy glance: {}", TABS[g.tab]);
    }
    let Some(cat) = g.category() else {
        // The map: Up zooms in, Down out.
        for a in &asks {
            if let Ask::Row(by) = a {
                g.zoom = (g.zoom as i32 + by).clamp(0, ZOOMS.len() as i32 - 1) as usize;
                g.map_age = f32::MAX;
            }
        }
        return;
    };
    // The wrist inventory: opened when first shown, read again now and then.
    if g.model.is_none() {
        match MenuModel::open_inventory(&mut sim.world) {
            Ok(m) => g.model = Some(m),
            Err(e) => {
                warn_once!("pipboy glance: the inventory cannot be read: {e}");
                return;
            }
        }
        g.items_age = 0.0;
    }
    let Some(m) = g.model.as_mut() else { return };
    if m.category[Side::Player as usize] != cat || g.tab != tab_before {
        m.apply(&mut sim.world, Command::SetCategory(Side::Player, cat));
    } else if g.items_age >= ITEMS_EVERY {
        g.items_age = 0.0;
        if let Err(e) = m.refresh(&mut sim.world) {
            warn_once!("pipboy glance: the inventory cannot be read again: {e}");
        }
    }
    for a in asks {
        let cmd = match a {
            Ask::Row(-1) => Command::Up,
            Ask::Row(_) => Command::Down,
            Ask::Use => Command::One,
            Ask::Drop => Command::Drop,
            Ask::Tab(_) => continue,
        };
        let item = m.selected_row(Side::Player).map(|r| r.name.clone());
        let done = m.apply(&mut sim.world, cmd);
        if matches!(cmd, Command::One | Command::Drop) {
            // A drop asks no "How many?" here: one at a time.
            m.quantity = None;
            let status = m.status.clone().filter(|s| !s.is_empty());
            info!("pipboy glance: {cmd:?} {}: {}", item.as_deref().unwrap_or("nothing"), status.as_deref().unwrap_or("done"));
            g.status = status;
        }
        if let Some(it) = done.used {
            used.write(ItemUsed { item: it.0, limb: None });
        }
        if done.power_armor_refused {
            if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                let _ = fnv_game::functions::refuse_power_armor(&mut rt.game);
            }
        }
        let heading = sim.world.peek(FormId::PLAYER_REF).map_or(0.0, |p| p.rot[2]);
        for id in &done.created {
            if let Ok(s) = sim.world.get(*id) {
                s.rot = [0.0, 0.0, heading];
            }
            changed.write(RefChanged::new(id.0));
        }
    }
}

// ---- the screen -------------------------------------------------------------------------

/// What the glance's screen shows besides the map's picture.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct View {
    tab: usize,
    /// The heading's right side (the place, or the inventory's weight).
    right: String,
    rows: Vec<(String, bool, bool)>,
    status: Option<String>,
}

/// The game the screen reads besides the inventory.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Reads<'w> {
    location: Option<Res<'w, crate::Location>>,
    mates: Res<'w, CoopMates>,
    map_image: Option<Res<'w, MapImage>>,
    kit: ResMut<'w, Kit>,
    data: Option<Res<'w, GameData>>,
    formats: Option<Res<'w, CompressedImageFormatSupport>>,
    time: Res<'w, Time<Real>>,
    glyphs: Res<'w, crate::pad::PadGlyphs>,
}

/// Draw the glance's screen into the Pip-Boy's image while the arm shows (the screen's
/// camera is on then, `super::camera_on`); the map's picture a few times a second.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    mut commands: Commands,
    mut glance: ResMut<Glance>,
    screen: Option<Res<PipBoyScreen>>,
    roots: Query<Entity, With<GlanceRoot>>,
    pictures: Query<(), With<MapPicture>>,
    data_state: Option<ResMut<super::data::DataState>>,
    mut game: super::data::Game,
    mut reads: Reads,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<Option<View>>,
) {
    let Some(screen) = screen else { return };
    if !glance.shown() {
        for e in &roots {
            commands.entity(e).despawn();
        }
        *shown = None;
        return;
    }
    let Some(mut st) = data_state else {
        commands.init_resource::<super::data::DataState>();
        return;
    };
    // The map's picture.
    glance.map_age += reads.time.delta_secs();
    if glance.tab == 0 && glance.map_age >= MAP_EVERY {
        glance.map_age = 0.0;
        let mates: Vec<[f32; 2]> = match reads.location.as_deref() {
            Some(loc) => reads.mates.0.iter().filter(|m| m.interior == loc.interior && (m.interior.is_some() || m.world == loc.world)).map(|m| [m.feet[0], m.feet[1]]).collect(),
            None => Vec::new(),
        };
        if let Some(mut map) = super::data::live_map(&mut st, &mut game) {
            map.marks.extend(mates.into_iter().map(|pos| LiveMark { kind: MarkKind::Mate, pos }));
            let per_px = ZOOMS[glance.zoom] / if map.interior { INTERIOR_CLOSER } else { 1.0 };
            let rgba = render_map(&map, per_px, MAP_PX, crate::interface::pipboy_rgb());
            if let Some(mut img) = reads.map_image.as_ref().and_then(|h| images.get_mut(&h.0)) {
                img.data = Some(rgba);
            }
        }
    }
    let right = match (glance.tab, glance.model.as_ref()) {
        (0, _) => format!("{} U/PX", ZOOMS[glance.zoom] as u32),
        (_, Some(m)) => match &m.encumbrance {
            Ok(e) => format!("WG {:.0}/{:.0}", e.weight, e.capacity),
            Err(_) => String::new(),
        },
        _ => String::new(),
    };
    let rows = match (glance.category(), glance.model.as_ref()) {
        (Some(_), Some(m)) => {
            let list = m.list(Side::Player);
            let sel = m.selected[Side::Player as usize];
            let first = sel.saturating_sub(ROWS / 2).min(list.rows.len().saturating_sub(ROWS));
            list.rows
                .iter()
                .enumerate()
                .skip(first)
                .take(ROWS)
                .map(|(i, r)| (if r.count > 1 { format!("{} ({})", r.name, r.count) } else { r.name.clone() }, r.equipped, i == sel))
                .collect()
        }
        _ => Vec::new(),
    };
    let view = View { tab: glance.tab, right, rows, status: glance.status.clone() };
    if shown.as_ref() == Some(&view) && !roots.is_empty() && (glance.tab != 0 || !pictures.is_empty()) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let map = reads.map_image.as_ref().map(|m| m.0.clone());
    let mut c = Ctx::new(&mut reads.kit, reads.data.as_deref(), &mut images, reads.formats.as_deref(), &reads.glyphs);
    let root = Node { overflow: Overflow::clip(), ..at(0.0, 0.0, CANVAS.x, CANVAS.y) };
    commands.spawn((GlanceRoot, root, UiTargetCamera(screen.camera), Name::new("Pip-Boy glance"))).with_children(|r| {
        // The heading: the tab's name and its figure.
        let title = if view.tab == 0 { "LOCAL MAP" } else { TABS[view.tab] };
        let top = MAIN.min.y;
        c.text(r, 4, title, MAIN.min.x + 20.0, top, Align::Left, amber(TEXT));
        c.text(r, 2, &view.right, MAIN.max.x, top + 10.0, Align::Right, amber(TEXT));
        c.line(r, MAIN.min.x, top + 45.0, MAIN.width(), THICK, amber(LINE));
        if view.tab == 0 {
            if let Some(m) = map.clone() {
                r.spawn((MapPicture, at(MAP_RECT.min.x, MAP_RECT.min.y, MAP_RECT.width(), MAP_RECT.height()), ImageNode { color: Color::WHITE, ..ImageNode::new(m) }));
            }
        } else if view.rows.is_empty() {
            c.text(r, 2, "NOTHING", MAIN.min.x + 40.0, MAP_RECT.min.y, Align::Left, amber(super::frame::DIMMED));
        } else {
            let pitch = 52.0;
            for (k, (name, equipped, selected)) in view.rows.iter().enumerate() {
                let y = MAP_RECT.min.y + k as f32 * pitch;
                if *selected {
                    let (b, a) = FILL;
                    c.boxed(r, Rect::new(MAIN.min.x + 10.0, y - 6.0, MAIN.max.x - 10.0, y + pitch - 10.0), amber(LINE), Some(amber(b).with_alpha(a / 255.0)));
                }
                if *equipped {
                    r.spawn((at(MAIN.min.x + 25.0, y + 10.0, 18.0, 18.0), BackgroundColor(amber(TEXT))));
                }
                c.text(r, 2, name, MAIN.min.x + 60.0, y, Align::Left, amber(TEXT));
            }
        }
        if let Some(s) = view.status.as_deref() {
            c.text(r, 2, s, MAIN.min.x + 20.0, 600.0, Align::Left, amber(TEXT));
        }
        // The tab line.
        let y = 650.0;
        c.line(r, MAIN.min.x, y, MAIN.width(), THICK, amber(LINE));
        let w = MAIN.width() / TABS.len() as f32;
        for (i, t) in TABS.iter().enumerate() {
            let x = MAIN.min.x + w * (i as f32 + 0.5);
            let b = if i == view.tab { TEXT } else { super::frame::DIMMED };
            c.text(r, 2, t, x, y + 10.0, Align::Centre, amber(b));
            if i == view.tab {
                c.line(r, x - w * 0.4, y + 48.0, w * 0.8, THICK, amber(LINE));
            }
        }
    });
    *shown = Some(view);
}

// ---- the map's picture ------------------------------------------------------------------

/// A canvas of RGBA pixels the map is drawn on.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
    rgb: [u8; 3],
}

impl Canvas {
    /// Lay `alpha` (0–255) of the colour at brightness `b` (0–1) over the pixel.
    fn put(&mut self, x: i64, y: i64, b: f32, alpha: u8) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || alpha == 0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let a = alpha.max(self.px[i + 3]);
        for k in 0..3 {
            let v = (f32::from(self.rgb[k]) * b).round().clamp(0.0, 255.0) as u8;
            self.px[i + k] = self.px[i + k].max(v);
        }
        self.px[i + 3] = a;
    }

    fn disc(&mut self, c: Vec2, r: f32, b: f32, hollow: bool) {
        let (x0, x1, y0, y1) = ((c.x - r - 1.0) as i64, (c.x + r + 1.0) as i64, (c.y - r - 1.0) as i64, (c.y + r + 1.0) as i64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(c);
                if d <= r && (!hollow || d >= r - 1.6) {
                    self.put(x, y, b, 255);
                }
            }
        }
    }

    fn square(&mut self, c: Vec2, half: f32, b: f32, hollow: bool) {
        let (x0, x1, y0, y1) = ((c.x - half).round() as i64, (c.x + half).round() as i64, (c.y - half).round() as i64, (c.y + half).round() as i64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if !hollow || x == x0 || x == x1 || y == y0 || y == y1 {
                    self.put(x, y, b, 255);
                }
            }
        }
    }

    /// A filled triangle.
    fn triangle(&mut self, p: [Vec2; 3], b: f32) {
        let min = p[0].min(p[1]).min(p[2]);
        let max = p[0].max(p[1]).max(p[2]);
        let edge = |a: Vec2, c: Vec2, q: Vec2| (c - a).perp_dot(q - a);
        let area = edge(p[0], p[1], p[2]);
        if area.abs() < 1e-6 {
            return;
        }
        for y in min.y.floor() as i64..=max.y.ceil() as i64 {
            for x in min.x.floor() as i64..=max.x.ceil() as i64 {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let (e0, e1, e2) = (edge(p[1], p[2], q) / area, edge(p[2], p[0], q) / area, edge(p[0], p[1], q) / area);
                if e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0 {
                    self.put(x, y, b, 255);
                }
            }
        }
    }
}

/// Where a game point (x, y) is drawn on a map `size` pixels round the player at `feet`
/// facing `heading`, `per_px` game units a pixel: forward is up.
pub fn to_map(p: [f32; 2], feet: [f32; 3], heading: f32, per_px: f32, size: [usize; 2]) -> Vec2 {
    let d = Vec2::new(p[0] - feet[0], p[1] - feet[1]);
    let (s, c) = heading.sin_cos();
    let (fwd, right) = (Vec2::new(s, c), Vec2::new(c, -s));
    Vec2::new(size[0] as f32, size[1] as f32) / 2.0 + Vec2::new(d.dot(right), -d.dot(fwd)) / per_px
}

/// The game point (x, y) a map pixel's middle shows ([`to_map`] backwards).
pub fn from_map(px: Vec2, feet: [f32; 3], heading: f32, per_px: f32, size: [usize; 2]) -> [f32; 2] {
    let m = (px - Vec2::new(size[0] as f32, size[1] as f32) / 2.0) * per_px;
    let (s, c) = heading.sin_cos();
    let (fwd, right) = (Vec2::new(s, c), Vec2::new(c, -s));
    let d = right * m.x - fwd * m.y;
    [feet[0] + d.x, feet[1] + d.y]
}

/// Draw the live map: the outline of the place, a ring at the edge, the marks (quest
/// targets and other players kept inside the edge when farther), north's tick and the
/// player's arrow in the middle pointing up. RGBA, row by row from the top.
pub fn render_map(map: &LiveMap, per_px: f32, size: [usize; 2], rgb: [u8; 3]) -> Vec<u8> {
    let mut cv = Canvas { w: size[0], h: size[1], px: vec![0; size[0] * size[1] * 4], rgb };
    let mid = Vec2::new(size[0] as f32, size[1] as f32) / 2.0;
    let radius = mid.x.min(mid.y) - 6.0;
    // The outline, inside the round window.
    if let Some(o) = &map.outline {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                if q.distance(mid) > radius {
                    continue;
                }
                let [gx, gy] = from_map(q, map.feet, map.heading, per_px, size);
                let a = o.at(gx, gy);
                if a > 0 {
                    cv.put(x as i64, y as i64, 1.0, (u16::from(a) * 3 / 2).min(255) as u8);
                }
            }
        }
    }
    // The window's edge, faint.
    let steps = (radius * std::f32::consts::TAU) as usize;
    for k in 0..steps {
        let t = k as f32 / steps as f32 * std::f32::consts::TAU;
        let p = mid + Vec2::new(t.cos(), t.sin()) * radius;
        cv.put(p.x as i64, p.y as i64, 0.55, 200);
    }
    // North: a tick on the edge.
    let north = to_map([map.feet[0], map.feet[1] + 1.0], map.feet, map.heading, per_px, size) - mid;
    if north.length() > 0.0 {
        let n = north.normalize();
        let tip = mid + n * (radius + 4.0);
        let base = mid + n * (radius - 10.0);
        let side = n.perp() * 6.0;
        cv.triangle([tip, base + side, base - side], 0.8);
    }
    for m in &map.marks {
        let mut at = to_map(m.pos, map.feet, map.heading, per_px, size);
        let keep_in = matches!(m.kind, MarkKind::Quest | MarkKind::Mate);
        let off = at.distance(mid) > radius - 8.0;
        if off && !keep_in {
            continue;
        }
        if off {
            at = mid + (at - mid).normalize_or_zero() * (radius - 8.0);
        }
        match m.kind {
            MarkKind::Door => cv.square(at, 3.0, 0.8, true),
            MarkKind::Place { found } => {
                let d = 5.0;
                cv.triangle([at + Vec2::new(0.0, -d), at + Vec2::new(d, 0.0), at + Vec2::new(0.0, d)], if found { 1.0 } else { 0.6 });
                cv.triangle([at + Vec2::new(0.0, -d), at + Vec2::new(0.0, d), at + Vec2::new(-d, 0.0)], if found { 1.0 } else { 0.6 });
            }
            MarkKind::Quest => {
                cv.triangle([at + Vec2::new(-7.0, -10.0), at + Vec2::new(7.0, -10.0), at + Vec2::new(0.0, 2.0)], 1.0);
            }
            MarkKind::Mate => {
                cv.disc(at, 6.0, 1.0, false);
                cv.disc(at, 9.0, 0.8, true);
            }
        }
    }
    // The player: an arrow pointing up.
    cv.triangle([mid + Vec2::new(0.0, -11.0), mid + Vec2::new(8.0, 9.0), mid + Vec2::new(-8.0, 9.0)], 1.0);
    cv.px
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(px: &[u8], size: [usize; 2], at: Vec2) -> bool {
        let (x, y) = (at.x as usize, at.y as usize);
        px[(y * size[0] + x) * 4 + 3] > 0
    }

    #[test]
    fn the_glance_holds_lower_than_the_full_raise_and_mirrors_going_down() {
        let mut g = Glance::default();
        g.raise();
        assert_eq!((g.phase, g.clip_time), (Phase::Raising, 0.0));
        assert!(!g.advance(0.1));
        assert!(g.advance(1.0));
        assert_eq!((g.phase, g.clip_time), (Phase::Up, GLANCE_HOLD));
        const { assert!(GLANCE_HOLD < HOLD_SECONDS) };
        g.lower();
        assert_eq!(g.phase, Phase::Lowering);
        assert!((g.clip_time - (2.0 * HOLD_SECONDS - GLANCE_HOLD)).abs() < 1e-6);
        // Raised again on the way down: from the mirrored point, at most the hold.
        g.advance(0.05);
        g.raise();
        assert_eq!(g.phase, Phase::Raising);
        assert!(g.clip_time <= GLANCE_HOLD && g.clip_time > 0.0);
        g.lower();
        assert!(g.advance(10.0));
        assert_eq!((g.phase, g.clip_time), (Phase::Down, 0.0));
        assert!(!g.shown());
        g.lower();
        assert_eq!(g.phase, Phase::Down, "lowering a lowered glance does nothing");
    }

    #[test]
    fn the_arm_follows_the_full_pipboy_first() {
        let mut pip = PipBoy::default();
        let mut g = Glance::default();
        assert_eq!(arm_clip(&pip, &g), None);
        g.raise();
        g.advance(0.1);
        assert_eq!(arm_clip(&pip, &g), Some(0.1));
        pip.raise();
        assert_eq!(arm_clip(&pip, &g), Some(0.0));
        assert_eq!(Check::of(&pip, &g), Check::Full);
        pip.lower();
        pip.advance(10.0);
        assert_eq!(Check::of(&pip, &g), Check::Glance);
        for c in [Check::None, Check::Glance, Check::Full] {
            assert_eq!(Check::from_code(c.code()), c);
        }
        assert_eq!(Check::from_code(9), Check::None);
    }

    #[test]
    fn the_tabs_are_the_map_then_the_categories() {
        let mut g = Glance::default();
        assert_eq!(g.category(), None);
        g.step_tab(1);
        assert_eq!(g.category(), Some(Category::Weapons));
        g.step_tab(-2);
        assert_eq!((g.tab, g.category()), (5, Some(Category::Ammo)));
        assert_eq!(TABS[g.tab], Category::Ammo.title());
        g.step_tab(1);
        assert_eq!(g.tab, 0);
    }

    #[test]
    fn the_map_turns_with_the_heading() {
        let size = [200, 100];
        let feet = [1000.0, 2000.0, 0.0];
        // Facing north: a point north is straight up, one east to the right.
        let up = to_map([1000.0, 2100.0], feet, 0.0, 10.0, size);
        assert!((up - Vec2::new(100.0, 40.0)).length() < 1e-3, "{up}");
        let right = to_map([1100.0, 2000.0], feet, 0.0, 10.0, size);
        assert!((right - Vec2::new(110.0, 50.0)).length() < 1e-3, "{right}");
        // Facing east: east is up, north to the left.
        let h = std::f32::consts::FRAC_PI_2;
        assert!((to_map([1100.0, 2000.0], feet, h, 10.0, size) - Vec2::new(100.0, 40.0)).length() < 1e-3);
        assert!((to_map([1000.0, 2100.0], feet, h, 10.0, size) - Vec2::new(90.0, 50.0)).length() < 1e-3);
        // And back.
        for heading in [0.0, 0.7, -2.0, 3.0] {
            let p = to_map([1234.0, 1876.0], feet, heading, 7.0, size);
            let back = from_map(p, feet, heading, 7.0, size);
            assert!((back[0] - 1234.0).abs() < 1e-2 && (back[1] - 1876.0).abs() < 1e-2, "{back:?}");
        }
    }

    #[test]
    fn the_outline_is_sampled_with_y_down_its_rows() {
        // 2 x 2 samples of 100 from (0, 200): the top row is y 100..200.
        let o = LiveOutline { origin: [0.0, 200.0], step: 100.0, size: [2, 2], alpha: std::sync::Arc::new(vec![10, 20, 30, 40]) };
        assert_eq!(o.at(50.0, 150.0), 10);
        assert_eq!(o.at(150.0, 150.0), 20);
        assert_eq!(o.at(50.0, 50.0), 30);
        assert_eq!(o.at(150.0, 50.0), 40);
        assert_eq!(o.at(-1.0, 50.0), 0);
        assert_eq!(o.at(50.0, 201.0), 0);
    }

    #[test]
    fn the_picture_shows_the_player_marks_and_the_outline_where_they_are() {
        let size = [160, 120];
        let feet = [0.0, 0.0, 0.0];
        // Outline: only the square north-east of the player, 0..400 both ways.
        let outline = LiveOutline { origin: [0.0, 400.0], step: 400.0, size: [1, 1], alpha: std::sync::Arc::new(vec![100]) };
        let map = LiveMap {
            interior: false,
            feet,
            heading: 0.0,
            outline: Some(outline),
            marks: vec![
                LiveMark { kind: MarkKind::Door, pos: [-300.0, 0.0] },
                // A quest target far to the south: kept inside the edge, below the middle.
                LiveMark { kind: MarkKind::Quest, pos: [0.0, -100_000.0] },
                LiveMark { kind: MarkKind::Mate, pos: [0.0, 300.0] },
            ],
        };
        let px = render_map(&map, 10.0, size, [255, 180, 60]);
        let mid = Vec2::new(80.0, 60.0);
        assert!(lit(&px, size, mid), "the player's arrow");
        assert!(lit(&px, size, mid + Vec2::new(20.0, -20.0)), "the outline north-east");
        assert!(!lit(&px, size, mid + Vec2::new(-20.0, 20.0)), "nothing south-west");
        assert!(lit(&px, size, mid + Vec2::new(-33.0, 0.0)), "the door west (its frame)");
        assert!(lit(&px, size, mid + Vec2::new(0.0, -30.0)), "the other player north");
        let quest = (95..110).any(|y| lit(&px, size, Vec2::new(80.0, y as f32)));
        assert!(quest, "the quest marker held at the south edge");
        // Turned to face east, the other player (north) is on the left.
        let turned = LiveMap { heading: std::f32::consts::FRAC_PI_2, ..map };
        let px = render_map(&turned, 10.0, size, [255, 180, 60]);
        assert!(lit(&px, size, mid + Vec2::new(-30.0, 0.0)), "north is left facing east");
    }
}
