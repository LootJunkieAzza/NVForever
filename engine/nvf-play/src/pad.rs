//! The controller (docs/design/controller.md): its buttons and stick directions, their
//! names, the game's default controller buttons, and the menu keys a controller stands
//! in for.
//!
//! **Which device works the controls** is the player's choice, `Input Device` in
//! `[Controls]` of `NVForever.ini` and the first row of the settings page's Controls tab
//! (`crate::bindings::InputDevice`) [design; owner 2026-10-16: keyboard and mouse, or a
//! controller, "depending on a setting under controls"]. With Controller chosen the
//! controls answer the controller only (`crate::bindings::ControlInput`), the left stick
//! moves, the right stick looks; Esc and the console's key still work.
//!
//! **The game's controller buttons** are the fourth byte `JJ` of each `[Controls]` value
//! of the owner's `FalloutPrefs.ini` (`crate::bindings::GAME_CONTROLS`) [observed
//! 2026-10-09; the byte's meaning is hypothesis, [`game_pad`]].
//!
//! **Menus** read keys, so while a controller works the controls its buttons stand in
//! for them ([`menu_keys`]): Start is Esc everywhere; in a menu (not playing) the D-pad
//! and the left stick are the arrow keys, A Enter, B Esc, LB and RB Page Up and Page
//! Down [design]. Each menu's own buttons come first ([`PadMenu`], [`own_keys`]): the
//! button the original's menu file names for a hint presses the key NVForever's menu
//! reads for it (barter X accepts, container X takes all, the inventory's X drops,
//! lockpick X forces, the level-up and character menus' Y resets and X is done) [observed:
//! the `<xbuttonx>` elements and `_xbox_button` traits of the interface XMLs,
//! docs/spec-notes/controls.md §2.1]; where no file names one the choice is [design].
//!
//! **Hints** in menus and under the crosshair name the key on the keyboard, and with a
//! controller the button: its picture where the original has one ([`XButton`],
//! `Interface\Shared\Button\general_button_*.dds`, pieces of the interface atlas),
//! else its letters ([`hint`]).

use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// A controller input a control can be bound to: a button, or the left stick pushed one
/// way (the movement controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadInput {
    Button(GamepadButton),
    Stick(StickDir),
}

/// A way the left stick is pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StickDir {
    Up,
    Down,
    Left,
    Right,
}

impl StickDir {
    pub const ALL: [StickDir; 4] = [StickDir::Up, StickDir::Down, StickDir::Left, StickDir::Right];

    fn index(self) -> usize {
        self as usize
    }

    /// How far a stick at `v` is pushed this way (0 to 1).
    pub fn amount(self, v: Vec2) -> f32 {
        match self {
            StickDir::Up => v.y,
            StickDir::Down => -v.y,
            StickDir::Left => -v.x,
            StickDir::Right => v.x,
        }
        .max(0.0)
    }
}

/// A stick pushed this far one way counts as that way's button pressed [design].
pub const STICK_PRESS: f32 = 0.5;
/// Below this push of the left stick the player walks, beyond it runs (with Always Run
/// on) [design].
pub const STICK_WALK_BELOW: f32 = 0.6;
/// A push of a stick under this counts as none [design; Bevy's own dead zone comes
/// first].
pub const STICK_DEAD: f32 = 0.12;

/// The game's controller code `JJ` of a `[Controls]` value as a controller input
/// [hypothesis]. The fit: the original's console layout puts Activate on A, the Pip-Boy
/// on B, Ready/Reload on X, Jump on Y, Aim on LT, Attack on RT, V.A.T.S. on RB, Wait on
/// Back and Sneak on a press of the left stick [documented: published Xbox 360 control
/// lists for the game, e.g. magicgameworld.com "Fallout: New Vegas Controls"], and the
/// owner's values give those controls `0A`, `0B`, `0C`, `0D`, `10`, `11`, `0E`, `07`,
/// `08` [observed]. A to Y follow the order of XInput's button bits, as Back then the
/// left then the right stick's press do [documented: Microsoft `XINPUT_GAMEPAD`
/// `wButtons`], so `09` (Grab) is taken for the right stick's press and `0F` (Toggle POV)
/// for the one shoulder left, LB. `13`, `14`, `16`, `17` (Forward, Back, Slide Right,
/// Slide Left) are taken for the left stick's directions. `01` (Ammo Swap) is not
/// placed; `FF` is none.
pub fn game_pad(code: u8) -> Option<PadInput> {
    use GamepadButton::*;
    Some(match code {
        0x07 => PadInput::Button(Select),
        0x08 => PadInput::Button(LeftThumb),
        0x09 => PadInput::Button(RightThumb),
        0x0A => PadInput::Button(South),
        0x0B => PadInput::Button(East),
        0x0C => PadInput::Button(West),
        0x0D => PadInput::Button(North),
        0x0E => PadInput::Button(RightTrigger),
        0x0F => PadInput::Button(LeftTrigger),
        0x10 => PadInput::Button(LeftTrigger2),
        0x11 => PadInput::Button(RightTrigger2),
        0x13 => PadInput::Stick(StickDir::Up),
        0x14 => PadInput::Stick(StickDir::Down),
        0x16 => PadInput::Stick(StickDir::Right),
        0x17 => PadInput::Stick(StickDir::Left),
        _ => return None,
    })
}

/// The inputs' names in the INI and on screen ("A)" under the crosshair) [design: an
/// Xbox controller's, as the original's button pictures are (`general_button_a.dds` …
/// `general_button_rs.dds`) [observed: `button_xbox_hint.xml`]].
pub const PAD_NAMES: &[(PadInput, &str)] = {
    use GamepadButton::*;
    use PadInput::{Button as B, Stick as S};
    &[
        (B(South), "A"), (B(East), "B"), (B(West), "X"), (B(North), "Y"),
        (B(LeftTrigger), "LB"), (B(RightTrigger), "RB"), (B(LeftTrigger2), "LT"), (B(RightTrigger2), "RT"),
        (B(LeftThumb), "LS"), (B(RightThumb), "RS"), (B(Select), "Back"), (B(Start), "Start"),
        (B(DPadUp), "D-Pad Up"), (B(DPadDown), "D-Pad Down"), (B(DPadLeft), "D-Pad Left"), (B(DPadRight), "D-Pad Right"),
        (S(StickDir::Up), "Left Stick Up"), (S(StickDir::Down), "Left Stick Down"),
        (S(StickDir::Left), "Left Stick Left"), (S(StickDir::Right), "Left Stick Right"),
    ]
};

/// Start opens and closes the pause menu: no control may take it.
pub const RESERVED_PAD: [PadInput; 1] = [PadInput::Button(GamepadButton::Start)];

pub fn pad_name(p: PadInput) -> &'static str {
    PAD_NAMES.iter().find(|(x, _)| *x == p).map_or("?", |(_, n)| n)
}

/// The input of a name (without case); `None` for none or an unknown name.
pub fn parse_pad(name: &str) -> Option<PadInput> {
    PAD_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(p, _)| *p)
}

/// The buttons the original's interface has pictures of, in its files' order
/// [documented: `prefabs/text_box.xml` `_filename_1` … `_filename_11` and
/// `prefabs/button_xbox_hint.xml` `_filename_1` … `_filename_10`: a, b, x, y, lt, rt, lb,
/// rb, ls, rs, then back]. The pictures are pieces of the interface's atlas
/// (`textures\interface\interfaceshared.tai`, `interfaceshared0.dds`, in
/// `Fallout - Textures2.bsa`) [observed 2026-10-16]; the HUD's are the `glow_` ones
/// (`_glow` of `text_box.xml` adds 13 to the file's number) [documented].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XButton {
    A,
    B,
    X,
    Y,
    Lt,
    Rt,
    Lb,
    Rb,
    Ls,
    Rs,
    Back,
}

impl XButton {
    pub const ALL: [XButton; 11] = [XButton::A, XButton::B, XButton::X, XButton::Y, XButton::Lt, XButton::Rt, XButton::Lb, XButton::Rb, XButton::Ls, XButton::Rs, XButton::Back];

    pub fn gamepad(self) -> GamepadButton {
        use GamepadButton::*;
        match self {
            XButton::A => South,
            XButton::B => East,
            XButton::X => West,
            XButton::Y => North,
            XButton::Lt => LeftTrigger2,
            XButton::Rt => RightTrigger2,
            XButton::Lb => LeftTrigger,
            XButton::Rb => RightTrigger,
            XButton::Ls => LeftThumb,
            XButton::Rs => RightThumb,
            XButton::Back => Select,
        }
    }

    /// The button with a picture a controller input is; `None` for the D-pad, Start and
    /// the stick's directions.
    pub fn of(p: PadInput) -> Option<XButton> {
        XButton::ALL.into_iter().find(|b| p == PadInput::Button(b.gamepad()))
    }

    fn file(self) -> &'static str {
        match self {
            XButton::A => "a",
            XButton::B => "b",
            XButton::X => "x",
            XButton::Y => "y",
            XButton::Lt => "lt",
            XButton::Rt => "rt",
            XButton::Lb => "lb",
            XButton::Rb => "rb",
            XButton::Ls => "ls",
            XButton::Rs => "rs",
            XButton::Back => "back",
        }
    }

    /// The picture's path as the menu files name it; `glow`: the HUD's.
    pub fn picture(self, glow: bool) -> String {
        format!("Interface\\Shared\\Button\\{}general_button_{}.dds",if glow { "glow_" } else { "" }, self.file())
    }

    /// Its letters and ")", as written where no picture is drawn: "A)", "LB)".
    pub fn text(self) -> String {
        format!("{})", pad_name(PadInput::Button(self.gamepad())))
    }
}

/// A menu's button picture: 22 across, 30 down [documented: `button_xbox_hint.xml`'s
/// `button_icon`].
pub const HINT_PICTURE: Vec2 = Vec2::new(22.0, 30.0);
/// The HUD's: 75 by 75 [documented: `text_box.xml`'s `xbox_button`].
pub const HUD_PICTURE: f32 = 75.0;

/// Whether menus draw their hints for a controller (the Input Device setting), kept by
/// [`follow_device`]: the menus' drawing code is many calls deep under their systems,
/// and asks [`hint`] rather than carry the bindings down.
static CONTROLLER_HINTS: AtomicBool = AtomicBool::new(false);

pub fn controller_hints() -> bool {
    CONTROLLER_HINTS.load(Ordering::Relaxed)
}

fn follow_device(bindings: Option<Res<crate::bindings::Bindings>>) {
    let on = bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller);
    if CONTROLLER_HINTS.swap(on, Ordering::Relaxed) != on {
        info!("controller: hints show {}", if on { "the controller's buttons" } else { "the keyboard's keys" });
    }
}

/// What a hint shows: the keyboard's key ("E)"), or the controller's button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Key(&'static str),
    Button(XButton),
}

/// The hint for a menu button whose key is `key` and whose controller button is `button`
/// (`None`: none of its own, the key is shown either way).
pub fn hint(key: &'static str, button: Option<XButton>) -> Hint {
    hint_on(controller_hints(), key, button)
}

pub fn hint_on(controller: bool, key: &'static str, button: Option<XButton>) -> Hint {
    match button {
        Some(b) if controller => Hint::Button(b),
        _ => Hint::Key(key),
    }
}

impl Hint {
    /// As text: the key, or the button's letters.
    pub fn text(self) -> String {
        match self {
            Hint::Key(k) => k.to_string(),
            Hint::Button(b) => b.text(),
        }
    }
}

/// Draw a hint's button picture (a menu's, [`HINT_PICTURE`]): its left edge at `left`,
/// its middle at `mid`. False when the hint is a key or the picture could not be read:
/// the caller writes [`Hint::text`] instead.
pub fn spawn_button(p: &mut ChildSpawnerCommands, hint: Hint, picture: impl FnOnce(&str) -> Option<crate::interface::Picture>, left: f32, mid: f32, color: Color) -> bool {
    let Hint::Button(b) = hint else { return false };
    let Some((image, rect)) = picture(&b.picture(false)) else { return false };
    let node = crate::interface::at(left, mid - HINT_PICTURE.y / 2.0, HINT_PICTURE.x, HINT_PICTURE.y);
    p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
    true
}

/// The hint as text, for hints written within a line ("TAB) Goodbye").
pub fn hint_text(key: &'static str, button: Option<XButton>) -> String {
    hint(key, button).text()
}

/// The left stick's directions as buttons, this frame and the last (for a just-pressed),
/// any controller's.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct StickButtons {
    pub now: [bool; 4],
    pub before: [bool; 4],
}

impl StickButtons {
    pub fn pressed(&self, d: StickDir) -> bool {
        self.now[d.index()]
    }

    pub fn just_pressed(&self, d: StickDir) -> bool {
        self.now[d.index()] && !self.before[d.index()]
    }

    pub fn just_released(&self, d: StickDir) -> bool {
        !self.now[d.index()] && self.before[d.index()]
    }
}

/// The furthest-pushed stick of the connected controllers (`left` or right), dead zone
/// taken off.
pub fn stick(pads: &Query<&Gamepad>, left: bool) -> Vec2 {
    let v = pads.iter().map(|g| if left { g.left_stick() } else { g.right_stick() }).max_by(|a, b| a.length_squared().total_cmp(&b.length_squared())).unwrap_or(Vec2::ZERO);
    if v.length() < STICK_DEAD {
        Vec2::ZERO
    } else {
        v.clamp_length_max(1.0)
    }
}

fn update_stick_buttons(pads: Query<&Gamepad>, mut s: ResMut<StickButtons>) {
    let v = stick(&pads, true);
    s.before = s.now;
    for d in StickDir::ALL {
        s.now[d.index()] = d.amount(v) >= STICK_PRESS;
    }
}

/// The menu keys a controller's buttons hold down now (module notes).
#[derive(Resource, Debug, Default)]
pub struct MenuKeys {
    held: Vec<KeyCode>,
    menu: PadMenu,
}

/// Start: Esc, always.
const ALWAYS: [(GamepadButton, KeyCode); 1] = [(GamepadButton::Start, KeyCode::Escape)];
/// In a menu.
const IN_MENUS: [(GamepadButton, KeyCode); 8] = [
    (GamepadButton::DPadUp, KeyCode::ArrowUp),
    (GamepadButton::DPadDown, KeyCode::ArrowDown),
    (GamepadButton::DPadLeft, KeyCode::ArrowLeft),
    (GamepadButton::DPadRight, KeyCode::ArrowRight),
    (GamepadButton::South, KeyCode::Enter),
    (GamepadButton::East, KeyCode::Escape),
    (GamepadButton::LeftTrigger, KeyCode::PageUp),
    (GamepadButton::RightTrigger, KeyCode::PageDown),
];
/// The left stick in a menu: the arrow keys, as the D-pad [design].
const STICK_ARROWS: [(StickDir, KeyCode); 4] = [(StickDir::Up, KeyCode::ArrowUp), (StickDir::Down, KeyCode::ArrowDown), (StickDir::Left, KeyCode::ArrowLeft), (StickDir::Right, KeyCode::ArrowRight)];

/// The menu up, as far as its own controller buttons go ([`own_keys`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadMenu {
    /// None, or one with no buttons of its own (the pause menu, the wait menu, the recipe
    /// menu): the D-pad, A and B only.
    #[default]
    Other,
    Barter,
    Container,
    /// The "How many?" prompt.
    Quantity,
    /// The Pip-Boy's ITEMS page.
    Inventory,
    /// The Pip-Boy's STATS page, its tab and its status page (0 CND, 1 RAD, 2 EFF).
    Stats { tab: usize, status: usize },
    /// The Pip-Boy's DATA page; `map`: the local or the world map.
    Data { map: bool },
    Lockpick,
    Dialogue,
    /// The character menus (traits, tags, S.P.E.C.I.A.L.).
    Chargen,
    LevelUp,
}

/// The keys a menu's own buttons press; they come before the D-pad's, A's and B's.
pub fn own_keys(menu: PadMenu) -> Vec<(GamepadButton, KeyCode)> {
    use GamepadButton::{East as B, North as Y, South as A, West as X};
    use KeyCode::*;
    match menu {
        // [observed: barter_menu.xml `<xbuttonx>` Accept, `<xbuttonb>` Exit; NVForever's
        // barter menu accepts on A and exits on X, Esc and Tab].
        PadMenu::Barter => vec![(X, KeyA)],
        // [observed: container_menu.xml CM_TakeAllButton `&xbuttonx;`, CM_ExitButton
        // `&xbuttonb;`].
        PadMenu::Container => vec![(X, KeyR)],
        // [observed: inventory_menu.xml `<xbuttonx>` IM_DropButton; A equips, B cancels;
        // Y repair, LB mod, RB hotkey are not built in NVForever].
        PadMenu::Inventory => vec![(X, KeyX)],
        // [observed: stats_menu.xml stats_stimpak_button and stats_radaway_button
        // `&xbuttona;`, stats_radx_button `&xbuttonx;`, stats_genrep_button `&xbuttonx;`;
        // the doctor's bag's is set in code: X, as Rad-X's beside it [hypothesis]].
        PadMenu::Stats { tab: 0, status: 0 } => vec![(A, KeyS), (X, KeyE)],
        PadMenu::Stats { tab: 0, status: 1 } => vec![(A, KeyA), (X, KeyX)],
        PadMenu::Stats { tab: 4, .. } => vec![(X, KeyR)],
        // [observed: lockpick_menu.xml `<xbuttonx>` LPM_ForceLockButton, `<xbuttonb>`
        // LPM_ExitButton].
        PadMenu::Lockpick => vec![(X, KeyF)],
        // [observed: map_menu.xml `<xbuttonx>` MM_ButtonX (Place Marker, Show Location,
        // the notes filter), `<xbuttony>` MM_ButtonY (Challenges, `_PCButton_R`)].
        PadMenu::Data { .. } => vec![(X, KeyX), (Y, KeyR)],
        // B says goodbye (Tab), as Esc would open the pause menu [design: no dialogue
        // menu file was read].
        PadMenu::Dialogue => vec![(B, Tab)],
        // [observed: char_gen_menu.xml `<xbuttony>` CGM_ResetButton, Done `&xbuttonx;`;
        // trait_menu.xml (the level-up menu, LUM_) Reset Y, Continue X]; B goes back a
        // page in the level-up menu [design].
        PadMenu::Chargen => vec![(Y, KeyR), (X, KeyA)],
        PadMenu::LevelUp => vec![(Y, KeyR), (X, KeyA), (B, KeyE)],
        _ => Vec::new(),
    }
}

/// The controller button of a hint in `menu` for what `key` does there (the inverse of
/// [`own_keys`], with A for Enter and B for Esc).
pub fn own_button(menu: PadMenu, key: KeyCode) -> Option<XButton> {
    let b = own_keys(menu).into_iter().find(|(_, k)| *k == key).map(|(b, _)| b).or(match key {
        KeyCode::Enter => Some(GamepadButton::South),
        KeyCode::Escape => Some(GamepadButton::East),
        _ => None,
    })?;
    XButton::of(PadInput::Button(b))
}

/// The page keys LT and RT press on the Pip-Boy: the page before and after this one (F1
/// STATS, F2 ITEMS, F3 DATA) [design: no Pip-Boy menu file names its page buttons].
fn page_keys(page: crate::pipboy::Page) -> Vec<(GamepadButton, KeyCode)> {
    const KEYS: [KeyCode; 3] = [KeyCode::F1, KeyCode::F2, KeyCode::F3];
    let i = crate::pipboy::Page::ALL.iter().position(|p| *p == page).unwrap_or(1);
    let mut v = Vec::new();
    if i > 0 {
        v.push((GamepadButton::LeftTrigger2, KEYS[i - 1]));
    }
    if i + 1 < KEYS.len() {
        v.push((GamepadButton::RightTrigger2, KEYS[i + 1]));
    }
    v
}

/// What the bridge looks at to know the menu up.
#[derive(bevy::ecs::system::SystemParam)]
pub struct MenuNow<'w> {
    state: Option<Res<'w, State<crate::PlayState>>>,
    paused: Option<Res<'w, crate::pause::Paused>>,
    page: Option<Res<'w, crate::pause::Page>>,
    active: Option<Res<'w, crate::menu::ActiveMenu>>,
    pip: Option<Res<'w, crate::pipboy::PipBoy>>,
    stats: Option<Res<'w, crate::pipboy::stats::StatsView>>,
    lockpick: Option<Res<'w, crate::lockpick::LockpickScreen>>,
    chargen: Option<Res<'w, crate::chargen::ChargenMenus>>,
    levelup: Option<Res<'w, crate::levelup::LevelUpMenu>>,
}

impl MenuNow<'_> {
    /// In a menu (not playing), or under the pause menu.
    fn in_menu(&self) -> bool {
        self.state.as_ref().is_some_and(|s| *s.get() != crate::PlayState::Playing) || self.paused()
    }

    fn paused(&self) -> bool {
        self.paused.as_ref().is_some_and(|p| p.0)
    }

    /// The Controls tab waits for a new button: it takes the buttons themselves.
    fn listening(&self) -> bool {
        self.page.as_ref().is_some_and(|p| matches!(**p, crate::pause::Page::Controls { listening: true, .. }))
    }

    pub fn menu(&self) -> PadMenu {
        use crate::menu::model::Mode;
        use crate::pipboy::Page;
        if !self.in_menu() || self.paused() {
            return PadMenu::Other;
        }
        let active = self.active.as_ref().and_then(|a| a.0.as_ref());
        if active.is_some_and(|m| m.quantity.is_some()) {
            return PadMenu::Quantity;
        }
        if self.lockpick.as_ref().is_some_and(|l| l.0.is_some()) {
            return PadMenu::Lockpick;
        }
        if self.levelup.as_ref().is_some_and(|l| l.is_open()) {
            return PadMenu::LevelUp;
        }
        if self.chargen.as_ref().is_some_and(|c| c.is_open()) {
            return PadMenu::Chargen;
        }
        if let Some(pip) = self.pip.as_ref().filter(|p| p.open()) {
            return match pip.page {
                Page::Stats => PadMenu::Stats { tab: pip.tab(), status: self.stats.as_ref().map_or(0, |s| s.status) },
                Page::Items => PadMenu::Inventory,
                Page::Data => PadMenu::Data { map: crate::pipboy::data::is_map_tab(pip.tab()) },
            };
        }
        match active.map(|m| m.mode) {
            Some(Mode::Barter(_)) => return PadMenu::Barter,
            Some(Mode::Container(_)) => return PadMenu::Container,
            _ => {}
        }
        if self.state.as_ref().is_some_and(|s| *s.get() == crate::PlayState::Dialogue) {
            return PadMenu::Dialogue;
        }
        PadMenu::Other
    }
}

/// Press and release the keys the controller's buttons stand for, after the input
/// systems have read the keyboard (so a menu sees a just-pressed key this frame). A key
/// goes down only as its button does (a button still held from before, as after binding
/// it on the Controls tab, presses nothing); none while the Controls tab waits for a new
/// button, which takes the buttons themselves. The left stick is the arrow keys except
/// where it does something of its own (the lock's tension, the Pip-Boy's maps).
fn menu_keys(pads: Query<&Gamepad>, sticks: Res<StickButtons>, bindings: Option<Res<crate::bindings::Bindings>>, now: MenuNow, mut keys: ResMut<ButtonInput<KeyCode>>, mut mk: ResMut<MenuKeys>) {
    let on = bindings.is_some_and(|b| b.device == crate::bindings::InputDevice::Controller) && !now.listening();
    let in_menu = now.in_menu();
    let menu = now.menu();
    if menu != mk.menu {
        if on {
            info!("controller: menu buttons for {menu:?}");
        }
        mk.menu = menu;
    }
    let mut table: Vec<(PadInput, KeyCode)> = Vec::new();
    if on {
        table.extend(ALWAYS.iter().map(|&(b, k)| (PadInput::Button(b), k)));
        if in_menu {
            let mut own = own_keys(menu);
            if let Some(pip) = now.pip.as_ref().filter(|p| p.open() && !now.paused()) {
                own.extend(page_keys(pip.page));
            }
            table.extend(own.iter().map(|&(b, k)| (PadInput::Button(b), k)));
            table.extend(IN_MENUS.iter().filter(|(b, _)| !own.iter().any(|(o, _)| o == b)).map(|&(b, k)| (PadInput::Button(b), k)));
            if !matches!(menu, PadMenu::Lockpick | PadMenu::Data { map: true }) {
                table.extend(STICK_ARROWS.iter().map(|&(d, k)| (PadInput::Stick(d), k)));
            }
        }
    }
    let mut want: Vec<KeyCode> = Vec::new();
    for (input, k) in table {
        let held = mk.held.contains(&k);
        let down = match input {
            PadInput::Button(b) => pads.iter().any(|g| if held { g.pressed(b) } else { g.just_pressed(b) }),
            PadInput::Stick(d) => {
                if held {
                    sticks.pressed(d)
                } else {
                    sticks.just_pressed(d)
                }
            }
        };
        if down && !want.contains(&k) {
            want.push(k);
        }
    }
    for k in mk.held.clone() {
        if !want.contains(&k) {
            keys.release(k);
        }
    }
    for k in &want {
        if !mk.held.contains(k) {
            keys.press(*k);
        }
    }
    mk.held = want;
}

/// A controller's arrival and departure, in the log.
fn log_pads(added: Query<(Entity, Option<&Name>), Added<Gamepad>>, mut gone: RemovedComponents<Gamepad>) {
    for (e, name) in &added {
        info!("controller: {} connected ({e})", name.map_or("a controller", |n| n.as_str()));
    }
    for e in gone.read() {
        info!("controller: {e} disconnected");
    }
}

pub struct PadPlugin;

impl Plugin for PadPlugin {
    fn build(&self, app: &mut App) {
        use bevy::input::InputSystems;
        app.init_resource::<StickButtons>()
            .init_resource::<MenuKeys>()
            .add_systems(PreUpdate, (update_stick_buttons, menu_keys).chain().after(InputSystems))
            .add_systems(Update, (log_pads, follow_device));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_games_codes_give_the_console_layout() {
        use GamepadButton::*;
        assert_eq!(game_pad(0x0A), Some(PadInput::Button(South)));
        assert_eq!(game_pad(0x11), Some(PadInput::Button(RightTrigger2)));
        assert_eq!(game_pad(0x0E), Some(PadInput::Button(RightTrigger)));
        assert_eq!(game_pad(0x13), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(game_pad(0xFF), None);
        assert_eq!(game_pad(0x01), None);
    }

    #[test]
    fn names_read_back() {
        for (p, n) in PAD_NAMES {
            assert_eq!(parse_pad(n), Some(*p));
            assert_eq!(pad_name(*p), *n);
        }
        assert_eq!(parse_pad(" left stick up "), Some(PadInput::Stick(StickDir::Up)));
        assert_eq!(parse_pad("Z"), None);
    }

    #[test]
    fn button_pictures_are_the_menu_files() {
        assert_eq!(XButton::A.picture(false), "Interface\\Shared\\Button\\general_button_a.dds");
        assert_eq!(XButton::Rs.picture(true), "Interface\\Shared\\Button\\glow_general_button_rs.dds");
        assert_eq!(XButton::of(PadInput::Button(GamepadButton::LeftTrigger)), Some(XButton::Lb));
        assert_eq!(XButton::of(PadInput::Button(GamepadButton::DPadUp)), None);
        assert_eq!(XButton::of(PadInput::Stick(StickDir::Up)), None);
        assert_eq!(XButton::Lt.text(), "LT)");
        for b in XButton::ALL {
            assert_eq!(XButton::of(PadInput::Button(b.gamepad())), Some(b));
        }
    }

    #[test]
    fn hints_follow_the_device() {
        assert_eq!(hint_on(false, "R)", Some(XButton::X)), Hint::Key("R)"));
        assert_eq!(hint_on(true, "R)", Some(XButton::X)), Hint::Button(XButton::X));
        assert_eq!(hint_on(true, "TAB)", None).text(), "TAB)");
        assert_eq!(hint_on(true, "TAB)", Some(XButton::B)).text(), "B)");
    }

    #[test]
    fn each_menu_has_the_buttons_its_file_names() {
        use KeyCode::*;
        assert_eq!(own_button(PadMenu::Barter, KeyA), Some(XButton::X));
        assert_eq!(own_button(PadMenu::Container, KeyR), Some(XButton::X));
        assert_eq!(own_button(PadMenu::Container, Escape), Some(XButton::B));
        assert_eq!(own_button(PadMenu::Inventory, KeyX), Some(XButton::X));
        assert_eq!(own_button(PadMenu::Lockpick, KeyF), Some(XButton::X));
        assert_eq!(own_button(PadMenu::Chargen, KeyR), Some(XButton::Y));
        assert_eq!(own_button(PadMenu::LevelUp, KeyE), Some(XButton::B));
        assert_eq!(own_button(PadMenu::Stats { tab: 0, status: 1 }, KeyX), Some(XButton::X));
        assert_eq!(own_button(PadMenu::Data { map: false }, KeyR), Some(XButton::Y));
        // B says goodbye in a conversation rather than opening the pause menu.
        assert_eq!(own_keys(PadMenu::Dialogue), vec![(GamepadButton::East, Tab)]);
        assert!(own_keys(PadMenu::Other).is_empty());
        // No menu gives one button two keys.
        for m in [PadMenu::Barter, PadMenu::Container, PadMenu::Inventory, PadMenu::Lockpick, PadMenu::Chargen, PadMenu::LevelUp, PadMenu::Data { map: true }, PadMenu::Stats { tab: 0, status: 0 }] {
            let own = own_keys(m);
            for (i, (b, _)) in own.iter().enumerate() {
                assert!(own[i + 1..].iter().all(|(o, _)| o != b), "{m:?}");
            }
        }
    }

    #[test]
    fn the_triggers_step_the_pip_boys_pages() {
        use crate::pipboy::Page;
        assert_eq!(page_keys(Page::Stats), vec![(GamepadButton::RightTrigger2, KeyCode::F2)]);
        assert_eq!(page_keys(Page::Items), vec![(GamepadButton::LeftTrigger2, KeyCode::F1), (GamepadButton::RightTrigger2, KeyCode::F3)]);
        assert_eq!(page_keys(Page::Data), vec![(GamepadButton::LeftTrigger2, KeyCode::F2)]);
    }

    #[test]
    fn a_stick_pushed_one_way() {
        let v = Vec2::new(0.3, -0.8);
        assert_eq!(StickDir::Down.amount(v), 0.8);
        assert_eq!(StickDir::Up.amount(v), 0.0);
        assert!((StickDir::Right.amount(v) - 0.3).abs() < 1e-6);
    }
}
