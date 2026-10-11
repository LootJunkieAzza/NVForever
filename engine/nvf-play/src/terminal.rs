//! Terminals (docs/spec-notes/hacking.md): activating one opens its screen. A locked
//! terminal first needs hacking: the word game of `fnv_game::hacking`, drawn as the
//! original's (`screen`). The right word opens the terminal's menu; four wrong words
//! lock it for good; leaving the game before then and coming back gives a new game
//! with the attempts back [the owner, 2026-10-11]. A terminal that is unlocked, hacked
//! before, or whose password note the player holds opens at its menu
//! (`fnv_game::terminal::opening`). The menu lists the items whose conditions hold;
//! choosing one runs its result script, shows its result text at the bottom, shows its
//! note or opens its sub menu.
//!
//! It is a `PlayState::Menu`: the world and the game clock stand still and the cursor is
//! free. To scripts it is menu 1055 while hacking and 1057 in the menu [documented: GECK
//! wiki GetMenuTargetRef, "the terminal being hacked", "the terminal being used"].
//!
//! Keys (hacking.md §7): in the hacking game the arrows (or W A S D) move the cursor a
//! character or a row, over a word at once; Enter or E enters what is under it; the
//! pointer moves it and a click enters. In the menu Up / Down choose, Enter or E
//! (or a click) takes the item. Tab or Backspace go back: from a note to the list, from a
//! sub menu to its parent, else out of the terminal; Esc leaves at once. [hypothesis: the
//! original's PC keys beyond Tab are not captured]
//!
//! Test aid `NVF_PLAY_TEST_TERMINAL=<seconds>:<step>[,…]`: `open:<reference>` opens a
//! terminal as if the player had activated it, `science:<n>` makes the player's Science
//! that for terminals, `wrong` enters a word that is not the password, `right` the
//! password, `brackets` the first bracket pair left, `symbol` a lone symbol,
//! `item:<n>` takes the n-th listed item (from 0), `back` and `exit` as Tab and Esc.
//! At exit, when a terminal was opened: "terminal summary: …".

pub mod screen;

use crate::interface::bc_supported;
use crate::pause::{AidTime, Paused};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use fnv_game::hacking::{Dictionary, Outcome, Puzzle, State as GameState, Target, ROWS, ROW_CHARS};
use fnv_game::terminal::{self as rules, Opening};
use fnv_sim::FormId;
use nvf_assets::GameData;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The menu codes scripts see [documented: GECK wiki GetMenuTargetRef].
pub const MENU_HACKING: u16 = 1055;
pub const MENU_TERMINAL: u16 = 1057;

/// After the password, "Please wait while system is accessed." stays this long before
/// the menu [design]; after the last wrong word, the log this long before the lockout
/// screen [design].
pub const GRANTED_SECONDS: f32 = 2.0;
pub const LOCKED_SECONDS: f32 = 1.5;

/// The game's word list [observed: `Fallout - Misc.bsa`].
pub const DICTIONARY: &str = "menus\\falloutdict.txt";

/// Open the terminal reference (a placed TERM) for the player.
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenTerminal {
    pub reference: u32,
}

/// The game's word list, read once.
#[derive(Resource, Default)]
pub struct Words(pub Dictionary);

/// The open terminal, if any.
#[derive(Resource, Default)]
pub struct TerminalScreen(pub Option<Session>);

pub struct Session {
    pub reference: FormId,
    pub base: FormId,
    pub page: Page,
}

pub enum Page {
    Hacking(Hacking),
    LockedOut,
    Menu(MenuView),
}

pub struct Hacking {
    pub puzzle: Puzzle,
    /// The dump character the cursor is on.
    pub cursor: usize,
    /// Nothing is lit: at the start and while the pointer is off the dump [observed: the
    /// owner's first screenshot, the pointer beside the screen and only the prompt].
    pub hidden: bool,
    /// When the game ended (aid clock seconds): won or locked out.
    pub ended: Option<f32>,
}

pub struct MenuView {
    /// The terminal whose items are listed (the placed one's base, or a sub menu).
    pub terminal: FormId,
    /// The terminals above, with the row chosen in each.
    pub parents: Vec<(FormId, usize)>,
    /// Indices of the items listed.
    pub items: Vec<usize>,
    pub selected: usize,
    /// No row is lit until the pointer or a key comes to the list [observed: the owner's
    /// screenshot of a terminal's menu, the pointer beside the screen].
    pub hidden: bool,
    /// The last chosen item's result text.
    pub result: String,
    /// The note shown instead of the list, and its first line shown.
    pub note: Option<FormId>,
    pub scroll: usize,
    /// The top terminal's server and welcome text (a sub menu keeps them).
    pub server: u8,
    pub welcome: String,
}

/// A command for the open terminal.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum TerminalInput {
    /// Put the cursor on a dump character (the pointer).
    Point(usize),
    /// The pointer left the dump.
    Unpoint,
    /// Move the cursor: characters across, rows down.
    Step(i32, i32),
    /// Enter / take what the cursor is on.
    Select,
    /// Choose a menu row (the pointer).
    Row(usize),
    Back,
    Exit,
    /// Test aid: enter a word or a pair of this kind.
    Aid(AidPick),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AidPick {
    Wrong,
    Right,
    Brackets,
    Symbol,
    Item(usize),
}

#[derive(Resource, Default, Debug)]
struct TerminalStats {
    opened: u32,
    hacks: u32,
    hacked: u32,
    locked_out: u32,
    refused: u32,
    guesses: u32,
    pairs: u32,
    duds: u32,
    refills: u32,
    items: u32,
}

/// `NVF_PLAY_TEST_TERMINAL`.
#[derive(Resource, Default)]
struct TerminalAid {
    steps: Vec<(f32, AidStep)>,
    science: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AidStep {
    Open(u32),
    Science(f32),
    Input(TerminalInput),
}

fn parse_aid(v: &str) -> Result<Vec<(f32, AidStep)>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(t, what)| {
            let (op, arg) = what.split_once(':').unwrap_or((what, ""));
            let step = match op {
                "open" => AidStep::Open(u32::from_str_radix(arg.trim().trim_start_matches("0x"), 16).map_err(|_| format!("{what:?}: expected open:<reference hex>"))?),
                "science" => AidStep::Science(arg.trim().parse().map_err(|_| format!("{what:?}: expected science:<n>"))?),
                "wrong" => AidStep::Input(TerminalInput::Aid(AidPick::Wrong)),
                "right" => AidStep::Input(TerminalInput::Aid(AidPick::Right)),
                "brackets" => AidStep::Input(TerminalInput::Aid(AidPick::Brackets)),
                "symbol" => AidStep::Input(TerminalInput::Aid(AidPick::Symbol)),
                "item" => AidStep::Input(TerminalInput::Aid(AidPick::Item(arg.trim().parse().map_err(|_| format!("{what:?}: expected item:<n>"))?))),
                "back" => AidStep::Input(TerminalInput::Back),
                "exit" => AidStep::Input(TerminalInput::Exit),
                _ => return Err(format!("{what:?}: expected open:, science:, wrong, right, brackets, symbol, item:, back or exit")),
            };
            Ok((t, step))
        })
        .collect()
}

pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        let steps = match std::env::var("NVF_PLAY_TEST_TERMINAL").map(|v| parse_aid(&v)) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => {
                warn!("NVF_PLAY_TEST_TERMINAL: {e}");
                Vec::new()
            }
            Err(_) => Vec::new(),
        };
        app.init_resource::<TerminalScreen>()
            .init_resource::<TerminalStats>()
            .init_resource::<Words>()
            .insert_resource(TerminalAid { steps, science: None })
            .add_message::<OpenTerminal>()
            .add_message::<TerminalInput>()
            .add_systems(Startup, read_words)
            .add_systems(
                Update,
                (run_aid, open, (keys, mouse, apply, timers).chain().run_if(in_state(PlayState::Menu)), draw)
                    .chain()
                    .after(crate::menu::open_menus)
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close)
            .add_systems(Last, report);
    }
}

fn read_words(data: Option<Res<GameData>>, mut words: ResMut<Words>) {
    let text = data.as_deref().and_then(|d| d.read(DICTIONARY));
    match text {
        Some(bytes) => {
            words.0 = Dictionary::parse(&String::from_utf8_lossy(&bytes));
            info!("terminal: {} words in {DICTIONARY}", words.0.len());
        }
        None => warn!("terminal: {DICTIONARY} not found; terminals cannot be hacked"),
    }
}

fn run_aid(time: AidTime, mut aid: ResMut<TerminalAid>, mut open: MessageWriter<OpenTerminal>, mut input: MessageWriter<TerminalInput>) {
    if aid.steps.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let mut due: Vec<(f32, AidStep)> = aid.steps.iter().filter(|(t, _)| *t <= now).copied().collect();
    due.sort_by(|a, b| a.0.total_cmp(&b.0));
    aid.steps.retain(|(t, _)| *t > now);
    for (_, step) in due {
        info!("test aid NVF_PLAY_TEST_TERMINAL: {step:?}");
        match step {
            AidStep::Open(r) => {
                open.write(OpenTerminal { reference: r });
            }
            AidStep::Science(s) => aid.science = Some(s),
            AidStep::Input(i) => {
                input.write(i);
            }
        }
    }
}

/// A seed for a new game: different every time [design: the original's dump is new on
/// every visit, the owner 2026-10-11].
fn seed(reference: FormId, now: f32) -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    t ^ ((reference.0 as u64) << 20) ^ now.to_bits() as u64
}

/// The listed items of a terminal and the view over them.
fn menu_view(rt: &mut crate::script::Runtime, reference: FormId, terminal: FormId, top: FormId) -> MenuView {
    let items = rules::items(&mut rt.game, reference, terminal);
    let (server, welcome) = rt.game.data.terminals.get(&top).map(|t| (t.server, t.welcome.clone())).unwrap_or_default();
    MenuView { terminal, parents: Vec::new(), items, selected: 0, hidden: true, result: String::new(), note: None, scroll: 0, server, welcome }
}

fn log_menu(rt: &crate::script::Runtime, m: &MenuView) {
    let data = rt.game.data;
    let Some(t) = data.terminals.get(&m.terminal) else { return };
    let names: Vec<String> = m.items.iter().filter_map(|&i| t.items.get(i)).map(|it| format!("{:?}", it.text)).collect();
    info!("terminal: menu of {} ({} of {} items listed): {}", data.name(m.terminal), m.items.len(), t.items.len(), names.join(", "));
}

#[allow(clippy::too_many_arguments)]
fn open(
    mut requests: MessageReader<OpenTerminal>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut screen: ResMut<TerminalScreen>,
    paused: Res<Paused>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    words: Res<Words>,
    aid: Res<TerminalAid>,
    time: AidTime,
    mut stats: ResMut<TerminalStats>,
    mut hud: ResMut<crate::activate::HudMessage>,
) {
    let Some(asked) = requests.read().last().copied() else { return };
    if *state.get() != PlayState::Playing || screen.0.is_some() || paused.0 {
        info!("terminal: request for {:08X} ignored, the player is not in the world", asked.reference);
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else {
        warn!("terminal: scripts are not running; terminals need the game's rules");
        return;
    };
    let reference = FormId(asked.reference);
    let Some(base) = sim.base_of(asked.reference) else {
        warn!("terminal: {reference} is not a placed reference");
        return;
    };
    if !rt.game.data.terminals.contains_key(&base) {
        warn!("terminal: {reference} (base {base}) is not a terminal");
        return;
    }
    let now = time.elapsed_secs();
    let name = rt.game.data.name(base);
    let opening = with_world(rt, &mut sim, |rt| {
        let sci = aid.science.unwrap_or_else(|| rules::science(&mut rt.game));
        rules::opening(&mut rt.game, reference, base, sci).map(|o| (o, sci))
    });
    let Some((opening, sci)) = opening else { return };
    stats.opened += 1;
    let page = match opening {
        Opening::Menu => {
            let m = with_world(rt, &mut sim, |rt| menu_view(rt, reference, base, base));
            info!("terminal: open {reference} {name}: its menu (unlocked, hacked before or the password is held)");
            log_menu(rt, &m);
            rt.game.open_menu(MENU_TERMINAL);
            Page::Menu(m)
        }
        Opening::LockedOut => {
            info!("terminal: open {reference} {name}: locked out for good");
            rt.game.open_menu(MENU_HACKING);
            Page::LockedOut
        }
        Opening::Refused(r) => {
            stats.refused += 1;
            // [hypothesis, hacking.md §8] The wording is NVForever's.
            let line = match r {
                fnv_game::hacking::Refusal::RequiresKey => "This terminal requires a password.".to_string(),
                fnv_game::hacking::Refusal::Science { needed } => format!("You need a Science skill of {needed:.0} to hack this terminal."),
            };
            info!("terminal: {reference} {name} refused (Science {sci:.0}): on screen: {line:?}");
            hud.show(line);
            return;
        }
        Opening::Hack { difficulty, words: n } => {
            let mut rng = ChaCha8Rng::seed_from_u64(seed(reference, now));
            let Some(puzzle) = Puzzle::new(&words.0, difficulty, n, &mut rng) else {
                warn!("terminal: {reference} {name}: the word list has too few words for {}", difficulty.name());
                return;
            };
            stats.hacks += 1;
            info!(
                "terminal: open {reference} {name}: hacking, {} (Science {sci:.0}): {} words of {} letters, {} bracket pairs",
                difficulty.name(),
                puzzle.words.len(),
                puzzle.password().len(),
                puzzle.pairs().len()
            );
            rt.game.open_menu(MENU_HACKING);
            let cursor = first_target(&puzzle);
            Page::Hacking(Hacking { puzzle, cursor, hidden: true, ended: None })
        }
    };
    screen.0 = Some(Session { reference, base, page });
    next.set(PlayState::Menu);
}

/// Where the cursor starts: the top left character [design].
fn first_target(_p: &Puzzle) -> usize {
    0
}

/// The cursor moved by a key: across a whole word at once, from the end of a row of the
/// first column into the second and back [hypothesis, hacking.md §7].
pub fn step_cursor(p: &Puzzle, cursor: usize, dx: i32, dy: i32) -> usize {
    let per_column = ROWS * ROW_CHARS;
    let (mut column, mut row, mut col) = ((cursor / per_column) as i32, ((cursor / ROW_CHARS) % ROWS) as i32, (cursor % ROW_CHARS) as i32);
    if dy != 0 {
        row = (row + dy).clamp(0, ROWS as i32 - 1);
        return Puzzle::index(column as usize, row as usize, col as usize);
    }
    if dx == 0 {
        return cursor;
    }
    // Leave the word the cursor is on first.
    let span = p.span(p.target(cursor));
    let mut at = if dx > 0 { span.end as i32 - 1 } else { span.start as i32 };
    column = at / per_column as i32;
    row = (at / ROW_CHARS as i32) % ROWS as i32;
    col = at % ROW_CHARS as i32 + dx.signum();
    if col >= ROW_CHARS as i32 {
        if column == 0 {
            (column, col) = (1, 0);
        } else {
            return cursor;
        }
    } else if col < 0 {
        if column == 1 {
            (column, col) = (0, ROW_CHARS as i32 - 1);
        } else {
            return cursor;
        }
    }
    at = Puzzle::index(column as usize, row as usize, col as usize) as i32;
    // A word that runs on from the row before or into the next: its first character.
    match p.target(at as usize) {
        Target::Word(w) if dx > 0 => p.words[w].start.max(at as usize),
        _ => at as usize,
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, screen: Res<TerminalScreen>, mut out: MessageWriter<TerminalInput>) {
    use KeyCode::*;
    let Some(s) = screen.0.as_ref() else { return };
    let mut send = |i| {
        out.write(i);
    };
    if keys.just_pressed(Escape) {
        send(TerminalInput::Exit);
        return;
    }
    if keys.any_just_pressed([Tab, Backspace]) {
        send(TerminalInput::Back);
        return;
    }
    if keys.any_just_pressed([Enter, NumpadEnter, KeyE]) {
        send(TerminalInput::Select);
    }
    let hacking = matches!(s.page, Page::Hacking(_));
    if keys.any_just_pressed([ArrowUp, KeyW]) {
        send(TerminalInput::Step(0, -1));
    }
    if keys.any_just_pressed([ArrowDown, KeyS]) {
        send(TerminalInput::Step(0, 1));
    }
    if hacking && keys.any_just_pressed([ArrowLeft, KeyA]) {
        send(TerminalInput::Step(-1, 0));
    }
    if hacking && keys.any_just_pressed([ArrowRight, KeyD]) {
        send(TerminalInput::Step(1, 0));
    }
}

fn mouse(
    screen: Res<TerminalScreen>,
    buttons: Res<ButtonInput<MouseButton>>,
    area: Query<&RelativeCursorPosition, With<screen::DumpArea>>,
    rows: Query<(&Interaction, &screen::MenuRow), Changed<Interaction>>,
    mut last: Local<Option<usize>>,
    mut out: MessageWriter<TerminalInput>,
) {
    let Some(s) = screen.0.as_ref() else { return };
    match &s.page {
        Page::Hacking(h) => {
            let under = area.iter().find_map(screen::pointer_index);
            if under != *last {
                *last = under;
                match under {
                    Some(i) if i != h.cursor || h.hidden => {
                        out.write(TerminalInput::Point(i));
                    }
                    None => {
                        out.write(TerminalInput::Unpoint);
                    }
                    _ => {}
                }
            }
            if under.is_some() && buttons.just_pressed(MouseButton::Left) {
                out.write(TerminalInput::Select);
            }
        }
        Page::Menu(_) => {
            for (i, row) in &rows {
                match i {
                    Interaction::Hovered => {
                        out.write(TerminalInput::Row(row.0));
                    }
                    Interaction::Pressed => {
                        out.write(TerminalInput::Row(row.0));
                        out.write(TerminalInput::Select);
                    }
                    Interaction::None => {}
                }
            }
        }
        Page::LockedOut => {
            if buttons.just_pressed(MouseButton::Left) {
                out.write(TerminalInput::Back);
            }
        }
    }
}

/// Close the terminal: back to the world.
fn leave(next: &mut NextState<PlayState>, why: &str) {
    info!("terminal: closed ({why})");
    next.set(PlayState::Playing);
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut input: MessageReader<TerminalInput>,
    mut screen: ResMut<TerminalScreen>,
    mut next: ResMut<NextState<PlayState>>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    time: AidTime,
    mut stats: ResMut<TerminalStats>,
    mut rng: Local<Option<ChaCha8Rng>>,
) {
    let now = time.elapsed_secs();
    for &cmd in input.read() {
        let Some(s) = screen.0.as_mut() else { return };
        let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
        let (reference, base) = (s.reference, s.base);
        match &mut s.page {
            Page::Hacking(h) => {
                if matches!(cmd, TerminalInput::Back | TerminalInput::Exit) {
                    // Leaving keeps nothing: the next visit is a new game with every
                    // attempt [the owner, 2026-10-11]; a lockout stays.
                    leave(&mut next, if h.ended.is_some() { "after the game" } else { "the hacking game was left; attempts come back on the next visit" });
                    return;
                }
                if h.ended.is_some() {
                    continue;
                }
                let p = &mut h.puzzle;
                let pick = match cmd {
                    TerminalInput::Point(i) => {
                        (h.cursor, h.hidden) = (i, false);
                        None
                    }
                    TerminalInput::Unpoint => {
                        h.hidden = true;
                        None
                    }
                    // The first key shows the cursor where it is.
                    TerminalInput::Step(dx, dy) => {
                        if !h.hidden {
                            h.cursor = step_cursor(p, h.cursor, dx, dy);
                        }
                        h.hidden = false;
                        None
                    }
                    TerminalInput::Select if h.hidden => None,
                    TerminalInput::Select => Some(h.cursor),
                    TerminalInput::Aid(a) => aid_pick(p, a),
                    _ => None,
                };
                let Some(at) = pick else { continue };
                h.cursor = at;
                h.hidden = false;
                let entered = p.text(p.target(at));
                let rng = rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(seed(reference, now)));
                let o = p.choose(at, rng);
                match o {
                    Outcome::Denied { likeness } => {
                        stats.guesses += 1;
                        info!("terminal: >{entered}: Entry denied, {likeness}/{} correct, {} attempts left", p.password().len(), p.attempts);
                    }
                    Outcome::Granted => {
                        stats.guesses += 1;
                        stats.hacked += 1;
                        info!("terminal: >{entered}: Exact match! access granted");
                        rules::hacked(&mut rt.game, reference, true);
                        h.ended = Some(now);
                    }
                    Outcome::LockedOut => {
                        stats.guesses += 1;
                        stats.locked_out += 1;
                        info!("terminal: >{entered}: Entry denied, no attempts left: the terminal is locked for good");
                        rules::hacked(&mut rt.game, reference, false);
                        h.ended = Some(now);
                    }
                    Outcome::DudRemoved => {
                        stats.pairs += 1;
                        stats.duds += 1;
                        info!("terminal: >{entered}: Dud removed ({} words left)", p.words.iter().filter(|w| !w.removed).count());
                    }
                    Outcome::Replenished => {
                        stats.pairs += 1;
                        stats.refills += 1;
                        info!("terminal: >{entered}: Allowance replenished ({} attempts)", p.attempts);
                    }
                    Outcome::Error => info!("terminal: >{entered}: Error"),
                    Outcome::Ignored => {}
                }
            }
            Page::LockedOut => {
                if matches!(cmd, TerminalInput::Back | TerminalInput::Exit | TerminalInput::Select) {
                    leave(&mut next, "locked out");
                    return;
                }
            }
            Page::Menu(m) => {
                let take = match cmd {
                    TerminalInput::Exit => {
                        leave(&mut next, "Esc");
                        return;
                    }
                    TerminalInput::Back => {
                        if m.note.take().is_some() {
                            info!("terminal: back from the note to the list");
                        } else if let Some((parent, row)) = m.parents.pop() {
                            let items = with_world(rt, &mut sim, |rt| rules::items(&mut rt.game, reference, parent));
                            (m.terminal, m.items, m.selected) = (parent, items, row);
                            log_menu(rt, m);
                        } else {
                            leave(&mut next, "back from the top menu");
                            return;
                        }
                        None
                    }
                    // Up / Down scroll a note a line [design].
                    TerminalInput::Step(_, dy) if m.note.is_some() => {
                        m.scroll = (m.scroll as i32 + dy).max(0) as usize;
                        None
                    }
                    TerminalInput::Step(_, dy) if m.note.is_none() && !m.items.is_empty() => {
                        if !m.hidden {
                            m.selected = (m.selected as i32 + dy).clamp(0, m.items.len() as i32 - 1) as usize;
                        }
                        m.hidden = false;
                        None
                    }
                    TerminalInput::Row(r) if r < m.items.len() => {
                        (m.selected, m.hidden) = (r, false);
                        None
                    }
                    TerminalInput::Select if m.note.is_none() && m.hidden => {
                        m.hidden = false;
                        None
                    }
                    TerminalInput::Select if m.note.is_none() => m.items.get(m.selected).copied(),
                    TerminalInput::Aid(AidPick::Item(n)) if m.note.is_none() => {
                        (m.selected, m.hidden) = (n.min(m.items.len().saturating_sub(1)), false);
                        m.items.get(n).copied()
                    }
                    _ => None,
                };
                let Some(item) = take else { continue };
                let terminal = m.terminal;
                let from = rt.game.events.len();
                let chosen = with_world(rt, &mut sim, |rt| rules::choose(&mut rt.game, reference, terminal, item));
                let Some(c) = chosen else { continue };
                let ran: Vec<String> = rt.game.events[from..].iter().take(8).map(|e| format!("{:?}", e.kind)).collect();
                if !ran.is_empty() {
                    info!("terminal: its result script: {}", ran.join("; "));
                }
                stats.items += 1;
                let text = rt.game.data.terminals.get(&terminal).and_then(|t| t.items.get(item)).map(|i| i.text.clone()).unwrap_or_default();
                info!(
                    "terminal: chose {text:?} of {}: result {:?}{}{}",
                    rt.game.data.name(terminal),
                    c.result,
                    c.submenu.map(|s| format!(", sub menu {}", rt.game.data.name(s))).unwrap_or_default(),
                    c.note.map(|n| format!(", note {}", rt.game.data.name(n))).unwrap_or_default()
                );
                m.result = c.result;
                if let Some(sub) = c.submenu {
                    m.parents.push((terminal, m.selected));
                    m.terminal = sub;
                    m.selected = 0;
                }
                // [design] The list is read again after every choice: a script may have
                // changed what its conditions see (the "Force Redraw" flag says so too).
                let t = m.terminal;
                m.items = with_world(rt, &mut sim, |rt| rules::items(&mut rt.game, reference, t));
                m.selected = m.selected.min(m.items.len().saturating_sub(1));
                (m.note, m.scroll) = (c.note, 0);
                if c.submenu.is_some() {
                    log_menu(rt, m);
                }
            }
        }
        let _ = base;
    }
}

/// The test aid's pick of a kind of word or pair.
fn aid_pick(p: &Puzzle, a: AidPick) -> Option<usize> {
    match a {
        AidPick::Right => Some(p.words[p.password].start),
        AidPick::Wrong => p.words.iter().enumerate().find(|(i, w)| *i != p.password && !w.removed).map(|(_, w)| w.start),
        AidPick::Brackets => p.pairs().first().map(|&(s, _)| s),
        AidPick::Symbol => (0..p.chars.len()).find(|&i| matches!(p.target(i), Target::Char(_)) && !p.chars[i].is_ascii_alphabetic()),
        AidPick::Item(_) => None,
    }
}

/// The ended game turns into the menu or the lockout screen.
fn timers(time: AidTime, mut screen: ResMut<TerminalScreen>, mut sim: ResMut<SimState>, mut script: Option<ResMut<ScriptState>>) {
    let now = time.elapsed_secs();
    let Some(s) = screen.0.as_mut() else { return };
    let Page::Hacking(h) = &s.page else { return };
    let Some(ended) = h.ended else { return };
    let won = h.puzzle.state == GameState::Granted;
    if now - ended < if won { GRANTED_SECONDS } else { LOCKED_SECONDS } {
        return;
    }
    let Some(rt) = script.as_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.menu == Some(MENU_HACKING) {
        rt.game.close_menu();
    }
    if won {
        let (reference, base) = (s.reference, s.base);
        let m = with_world(rt, &mut sim, |rt| menu_view(rt, reference, base, base));
        log_menu(rt, &m);
        rt.game.open_menu(MENU_TERMINAL);
        s.page = Page::Menu(m);
    } else {
        info!("terminal: on screen: {} / {}", screen::words::LOCKOUT_1, screen::words::LOCKOUT_2);
        rt.game.open_menu(MENU_HACKING);
        s.page = Page::LockedOut;
    }
}

/// A key for what the screen shows: it is drawn again when this changes.
fn view_key(s: &Session) -> String {
    match &s.page {
        Page::Hacking(h) => format!("h{}:{}:{}:{}:{}:{:?}:{:?}", h.cursor, h.hidden, h.puzzle.log.len(), h.puzzle.attempts, h.puzzle.used.len(), h.ended.is_some(), h.puzzle.words.iter().filter(|w| w.removed).count()),
        Page::LockedOut => "lockout".into(),
        Page::Menu(m) => format!("m{}:{:?}:{}:{}:{}:{}:{:?}:{}", m.terminal.0, m.items, m.selected, m.hidden, m.scroll, m.result, m.note, m.parents.len()),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    screen: Res<TerminalScreen>,
    data: Option<Res<GameData>>,
    script: Option<Res<ScriptState>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    roots: Query<Entity, With<screen::ScreenRoot>>,
    mut blinks: Query<&mut Visibility, With<screen::Blink>>,
    time: Res<Time<Real>>,
    mut faces: Local<Option<screen::Faces>>,
    mut shown: Local<Option<String>>,
) {
    let key = screen.0.as_ref().map(view_key);
    let on = ((time.elapsed_secs() / screen::BLINK_SECONDS) as u32).is_multiple_of(2);
    if key == *shown {
        for mut v in &mut blinks {
            v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    *shown = key;
    let (Some(s), Some(rt)) = (screen.0.as_ref(), script.as_ref().and_then(|s| s.rt.as_ref())) else { return };
    let faces = faces.get_or_insert_with(|| screen::load_faces(data.as_deref(), &mut images, bc_supported(formats.as_deref())));
    screen::spawn(&mut commands, faces, s, rt.game.data, on);
}

fn close(mut commands: Commands, mut screen: ResMut<TerminalScreen>, roots: Query<Entity, With<screen::ScreenRoot>>, script: Option<ResMut<ScriptState>>) {
    if screen.0.take().is_some() {
        if let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) {
            if matches!(rt.game.menu, Some(MENU_HACKING | MENU_TERMINAL)) {
                rt.game.close_menu();
            }
        }
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<TerminalStats>) {
    if exit.read().next().is_none() || (stats.opened == 0 && stats.refused == 0) {
        return;
    }
    info!(
        "terminal summary: {} opened ({} refused), {} hacking games ({} hacked, {} locked out), {} words entered, {} bracket pairs ({} duds removed, {} allowances replenished), {} menu items chosen",
        stats.opened, stats.refused, stats.hacks, stats.hacked, stats.locked_out, stats.guesses, stats.pairs, stats.duds, stats.refills, stats.items
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::hacking::{Word, ATTEMPTS, DUMP_CHARS};

    fn puzzle() -> Puzzle {
        let mut chars = vec![b'.'; DUMP_CHARS];
        // Row 0 of column 1 ends in "LOO", row 1 starts with "TING".
        chars[9..12].copy_from_slice(b"LOO");
        chars[12..16].copy_from_slice(b"TING");
        chars[204..211].copy_from_slice(b"MEETING");
        let words = vec![Word { text: "LOOTING".into(), start: 9, removed: false }, Word { text: "MEETING".into(), start: 204, removed: false }];
        Puzzle { chars, address: 0xF650, words, password: 1, attempts: ATTEMPTS, used: Vec::new(), log: Vec::new(), state: GameState::Guessing }
    }

    #[test]
    fn the_cursor_crosses_a_word_at_once() {
        let p = puzzle();
        // Right from just before the word: onto its start; right again: past its end,
        // which is in the next row.
        assert_eq!(step_cursor(&p, 8, 1, 0), 9);
        assert_eq!(step_cursor(&p, 9, 1, 0), 16);
        // Left from just after it: onto it, then before it.
        assert_eq!(step_cursor(&p, 16, -1, 0), 15);
        assert_eq!(step_cursor(&p, 15, -1, 0), 8);
    }

    #[test]
    fn the_cursor_goes_between_the_columns() {
        let p = puzzle();
        // The end of row 5 of column 1 goes on at the start of row 5 of column 2.
        let end = Puzzle::index(0, 5, 11);
        assert_eq!(step_cursor(&p, end, 1, 0), Puzzle::index(1, 5, 0));
        assert_eq!(step_cursor(&p, Puzzle::index(1, 5, 0), -1, 0), end);
        // Off the left of column 1 or the right of column 2: it stays.
        assert_eq!(step_cursor(&p, Puzzle::index(0, 3, 0), -1, 0), Puzzle::index(0, 3, 0));
        assert_eq!(step_cursor(&p, Puzzle::index(1, 3, 11), 1, 0), Puzzle::index(1, 3, 11));
        // Rows: the same column, stopping at the ends.
        assert_eq!(step_cursor(&p, Puzzle::index(0, 0, 4), 0, -1), Puzzle::index(0, 0, 4));
        assert_eq!(step_cursor(&p, Puzzle::index(0, 0, 4), 0, 1), Puzzle::index(0, 1, 4));
        assert_eq!(step_cursor(&p, Puzzle::index(1, 16, 4), 0, 1), Puzzle::index(1, 16, 4));
    }

    #[test]
    fn the_aid_finds_each_kind() {
        let p = puzzle();
        assert_eq!(aid_pick(&p, AidPick::Right), Some(204));
        assert_eq!(aid_pick(&p, AidPick::Wrong), Some(9));
        assert_eq!(aid_pick(&p, AidPick::Brackets), None);
        assert_eq!(aid_pick(&p, AidPick::Symbol), Some(0));
    }

    #[test]
    fn the_aid_reads_its_steps() {
        let s = parse_aid("1:open:0010ABCD,2:science:100,3:wrong,4:item:2,5:back").unwrap();
        assert_eq!(s[0], (1.0, AidStep::Open(0x0010ABCD)));
        assert_eq!(s[1], (2.0, AidStep::Science(100.0)));
        assert_eq!(s[2], (3.0, AidStep::Input(TerminalInput::Aid(AidPick::Wrong))));
        assert_eq!(s[3], (4.0, AidStep::Input(TerminalInput::Aid(AidPick::Item(2)))));
        assert_eq!(s[4], (5.0, AidStep::Input(TerminalInput::Back)));
        assert!(parse_aid("1:dance").is_err());
    }
}
