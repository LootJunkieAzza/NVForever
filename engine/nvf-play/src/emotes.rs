//! Emotes (the fork's; queue item 137): animations of the game's own idles, and of the
//! Animated Greetings mod's when it is installed, that a player plays on their body for
//! every player to see.
//!
//! - **The catalogue** ([`CATALOG`]): each emote's key, name, category and clips. A
//!   one-shot ([`Shape::Once`]) plays one clip of its candidates, picked at random among
//!   those installed (the game's and the mod's `meshes\AG_<name>.kf`, read from the load
//!   order only when there: the mod is never shipped). A loop ([`Shape::Loop`]) plays its
//!   entry clip, then its staying clips until the player moves (one clip loops; several
//!   play in turn), then its exit clip.
//! - **The loadout** ([`EmoteSlots`]): the eight emotes the wheel shows, chosen in the
//!   lobby's Emotes menu (`crate::lobby`), kept in `emote-slots.txt` beside the settings
//!   file (each player's own home).
//! - **The wheel**: Emote (B) held opens it in the weapon wheel's style; letting go on a
//!   slice plays its emote; a tap ends the emote playing.
//! - The emote itself is played by nvforever's `emote_play.rs` ([`EmoteAsk`]).
//!
//! Log lines: `emotes: …`.
//!
//! Test aids:
//! - `NVF_PLAY_TEST_EMOTE="<seconds>:<step>,..."` (world aid seconds): a step is an emote
//!   key or wire id (`sit`, `3`), with `:<seconds>` after it to end it that long after it
//!   starts, and `@<clip>` before that to play a one-shot's clip of that file name rather
//!   than one at random (`wave@AG_WaveShort`, `nod@AG_HeadNod:3`); `cancel`; `1st` / `3rd` (the view); `wheel` (open the wheel), `pick=<slot>`
//!   (1-8, point it at a slot), `release` (let go: play the slot picked), `close`.
//! - `NVF_PLAY_TEST_EMOTE_SLOTS="1:<key>,2:<key>,..."`: the loadout for this run (not
//!   kept); `<n>:` alone empties slot n.

use crate::bindings::{Control, ControlInput};
use crate::{PlaySet, PlayState};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;
use std::path::PathBuf;

/// The slots the wheel shows.
pub const SLOTS: usize = 8;

/// The menu's groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Greet,
    Gesture,
    SitLay,
    Fitness,
    Misc,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Greet, Category::Gesture, Category::SitLay, Category::Fitness, Category::Misc];

    pub fn label(self) -> &'static str {
        match self {
            Category::Greet => "Greet",
            Category::Gesture => "Gesture",
            Category::SitLay => "Sit and Lay",
            Category::Fitness => "Fitness and Dance",
            Category::Misc => "Misc",
        }
    }
}

/// How an emote plays. Clip names are the game's `meshes\characters\_male\idleanims\<n>.kf`
/// ([`vanilla`]); `ag` names the mod's `meshes\AG_<n>.kf` ([`animated_greeting`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Once { vanilla: &'static [&'static str], ag: &'static [&'static str] },
    Loop { enter: Option<&'static str>, stay: &'static [&'static str], exit: Option<&'static str>, fast: Option<&'static str> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emote {
    /// The test aids' and the loadout file's name for it.
    pub key: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub shape: Shape,
}

const fn once(key: &'static str, name: &'static str, category: Category, vanilla: &'static [&'static str], ag: &'static [&'static str]) -> Emote {
    Emote { key, name, category, shape: Shape::Once { vanilla, ag } }
}

const fn looped(key: &'static str, name: &'static str, category: Category, enter: Option<&'static str>, stay: &'static [&'static str], exit: Option<&'static str>, fast: Option<&'static str>) -> Emote {
    Emote { key, name, category, shape: Shape::Loop { enter, stay, exit, fast } }
}

use Category::*;

/// Every emote, in the order the wire numbers them (an emote's id is its index plus one,
/// `nvf_coop::net::EmoteState`): new ones go at the end. The clips are those the request
/// of 2026-11-09 named, checked in the load order's BSAs [observed: `Fallout - Meshes.bsa`
/// and `HonestHearts - Main.bsa` listings; the lengths from fnvtool `nif anim`]. Left out:
/// prop idles (a bottle, a guitar) and the hologram and creature clips of the same names.
/// Every clip plays over the standing idle (nvforever `emote_play.rs`), so the mod's
/// clips that key the head alone (its nods and head shakes) or no `Bip01 NonAccum` keep
/// the standing body under them.
pub const CATALOG: &[Emote] = &[
    once("wave", "Wave", Greet, &["wavehello", "wavehelloshort"], &["Wave", "WaveShort"]),
    once("salute", "Salute", Greet, &["salutea", "lazysalute"], &["Salute", "SaluteLazy", "SaluteLight"]),
    looped("sit", "Sit", SitLay, Some("sitfloorenter"), &["sitfloordynamicidle"], Some("sitfloorexit"), Some("sitfloorexitfast")),
    looped("lay", "Lay Down", SitLay, Some("floorsleepleftenter"), &["floorsleepdynamicidle"], Some("floorsleepleftexit"), Some("floorsleepleftexitfast")),
    looped("pushups", "Push-ups", Fitness, None, &["pushups"], Some("pushupsgetup"), None),
    once("shrug", "Shrug", Gesture, &["shouldershrug"], &["ShoulderShrug"]),
    once("cheer", "Cheer", Greet, &["cheeringbotharmsup", "cheeringonearmup"], &[]),
    looped("dance", "Dance", Fitness, Some("specialidle_nvfemale_danceenter"), &["specialidle_nvfemale_danceidle01", "specialidle_nvfemale_danceidle02", "dlc04danceb", "dlc04dancec"], Some("specialidle_nvfemale_danceexit"), None),
    looped("sit-cross", "Sit Cross-legged", SitLay, Some("sitfloorcrossenter"), &["sitfloorcrossdynamicidle"], Some("sitfloorcrossexit"), Some("sitfloorcrossexitfast")),
    looped("lay-back", "Lay on Back", SitLay, None, &["specialidle_nvsleeponback"], None, None),
    looped("kneel", "Kneel", SitLay, Some("dlc05samuraikneelenter"), &["dlc05samuraikneel_dynamicidle"], Some("dlc05samuraikneelexit"), None),
    looped("meditate", "Meditate", SitLay, Some("dlc04meditationsitenter"), &["dlc04meditationdynamicidle"], Some("dlc04meditationsitexit"), None),
    looped("depressed", "Sit, Depressed", SitLay, None, &["dlcpittsitfloordepresseda", "dlcpittsitfloordepressedb"], None, None),
    looped("situps", "Sit-ups", Fitness, None, &["dlcanchsitups"], None, None),
    looped("warmup", "Warm Up", Fitness, None, &["ms09specialidle_warmingup01", "ms09specialidle_warmingup02", "ms09specialidle_warmingup03"], None, None),
    looped("tap-dance", "Tap Dance", Fitness, None, &["specialidle_nvtapdance"], None, None),
    looped("booty-shake", "Shake It", Fitness, None, &["specialidle_nvdancebootyshake"], None, None),
    looped("arms-crossed", "Arms Crossed", Misc, None, &["ttpntrlarmscrossedlistena"], None, None),
    looped("hands-up", "Hands Up", Misc, None, &["handsupnoharm"], None, None),
    looped("cower", "Cower", Misc, None, &["3rdp_cowering"], None, None),
    once("clap", "Clap", Greet, &["cheeringclapping"], &[]),
    once("laugh", "Laugh", Gesture, &["laugha", "jollylaughter"], &[]),
    once("laugh-point", "Laugh and Point", Gesture, &["laughandpoint"], &[]),
    once("point", "Point", Gesture, &["thumbpointright"], &["Point1", "Point2", "Point3", "Point4"]),
    once("point-down", "Point Down", Gesture, &[], &["PointDown"]),
    once("point-self", "Point at Self", Gesture, &["pointatself"], &[]),
    once("cry", "Cry", Misc, &["cryingstandinga"], &[]),
    once("yawn", "Rub Eyes", Misc, &["sleepyrubeyes"], &[]),
    once("legion-salute", "Legion Salute", Greet, &[], &["LegionSalute"]),
    once("legion-kneel", "Legion Kneel", Greet, &[], &["LegionKneel"]),
    once("bow", "Bow", Greet, &[], &["Bow"]),
    once("tip-hat", "Tip Hat", Greet, &[], &["Hat"]),
    once("service", "At Your Service", Greet, &[], &["Service"]),
    once("brotherhood", "Brotherhood", Greet, &[], &["Brotherhood"]),
    once("rebellion", "Rebellion", Greet, &[], &["Rebellion"]),
    once("head-shake", "Shake Head", Gesture, &[], &["HeadShakeNeutral", "HeadShakeSad"]),
    once("stop", "Stop", Gesture, &[], &["Stop"]),
    once("finger-shake", "No No No", Gesture, &[], &["FingerShake"]),
    once("finger-up", "Finger Up", Gesture, &[], &["FingerUp"]),
    once("finger-lips", "Hush", Gesture, &[], &["FingerLips"]),
    once("chin-flick", "Chin Flick", Gesture, &[], &["ChinFlick"]),
    once("shout", "Shout", Gesture, &[], &["Shout"]),
    once("hands-on-hips", "Hands on Hips", Gesture, &[], &["HandsOnHip"]),
    once("one-hand-up", "Hand Up", Gesture, &[], &["OneHandUp"]),
    once("talk", "Talk", Gesture, &[], &["Talk1", "Talk2", "Talk3"]),
    once("hand", "Hand Out", Gesture, &[], &["Hand1", "Hand2"]),
    once("feminine", "Flourish", Gesture, &[], &["Feminine"]),
    once("heart", "Heart", Misc, &[], &["Heart"]),
    once("chest-pound", "Chest Pound", Misc, &[], &["ChestPound1", "ChestPound2"]),
    once("nod", "Nod", Gesture, &[], &["HeadNod", "HeadNodSingle"]),
];

/// The default loadout [design: the request's update, 2026-11-09].
pub const DEFAULT_SLOTS: [&str; SLOTS] = ["wave", "salute", "sit", "lay", "pushups", "shrug", "cheer", "dance"];

/// The game's idle clip `name`.
pub fn vanilla(name: &str) -> String {
    format!("meshes\\characters\\_male\\idleanims\\{name}.kf")
}

/// The Animated Greetings mod's clip `name`, where the mod puts it (loose under
/// `meshes\`) [observed: the mod's archive, 2026-11-09].
pub fn animated_greeting(name: &str) -> String {
    format!("meshes\\AG_{name}.kf")
}

impl Emote {
    /// The wire's id.
    pub fn id(index: usize) -> u8 {
        (index + 1) as u8
    }

    /// A one-shot's clips, the game's first: the wire's `variant` indexes them.
    pub fn candidates(&self) -> Vec<String> {
        match self.shape {
            Shape::Once { vanilla: v, ag } => v.iter().map(|n| vanilla(n)).chain(ag.iter().map(|n| animated_greeting(n))).collect(),
            Shape::Loop { .. } => Vec::new(),
        }
    }

    /// Every clip the emote needs to play at all (a loop's entry, stays and exit; a
    /// one-shot needs one of its candidates).
    pub fn needs(&self) -> Vec<String> {
        match self.shape {
            Shape::Once { .. } => Vec::new(),
            Shape::Loop { enter, stay, exit, .. } => enter.iter().chain(stay.iter()).chain(exit.iter()).map(|n| vanilla(n)).collect(),
        }
    }

    pub fn is_loop(&self) -> bool {
        matches!(self.shape, Shape::Loop { .. })
    }
}

pub fn by_key(key: &str) -> Option<usize> {
    let k = key.trim().to_ascii_lowercase();
    CATALOG.iter().position(|e| e.key == k).or_else(|| k.parse::<usize>().ok().filter(|n| (1..=CATALOG.len()).contains(n)).map(|n| n - 1))
}

/// What the load order has: per emote, which of its one-shot candidates are installed,
/// and whether it can play at all. Filled once the game data is loaded.
#[derive(Resource, Debug, Default, Clone)]
pub struct EmoteBook {
    pub ready: bool,
    /// Per emote: its candidates installed (a one-shot's), in [`Emote::candidates`] order.
    pub installed: Vec<Vec<bool>>,
    pub available: Vec<bool>,
}

impl EmoteBook {
    /// From `has` (whether the load order has a file).
    pub fn of(has: impl Fn(&str) -> bool) -> EmoteBook {
        let mut book = EmoteBook { ready: true, ..default() };
        for e in CATALOG {
            let inst: Vec<bool> = e.candidates().iter().map(|p| has(p)).collect();
            let ok = if e.is_loop() { e.needs().iter().all(|p| has(p)) } else { inst.iter().any(|b| *b) };
            book.installed.push(inst);
            book.available.push(ok);
        }
        book
    }

    pub fn available(&self, i: usize) -> bool {
        !self.ready || self.available.get(i).copied().unwrap_or(false)
    }
}

/// The eight emotes the wheel shows (catalogue indices).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct EmoteSlots {
    pub slots: [Option<usize>; SLOTS],
    /// Where they are kept (`None`: not kept, a test aid's loadout or no home).
    pub file: Option<PathBuf>,
}

impl Default for EmoteSlots {
    fn default() -> Self {
        EmoteSlots { slots: DEFAULT_SLOTS.map(by_key), file: None }
    }
}

impl EmoteSlots {
    /// Put `emote` in `slot`; one already in another slot swaps with what `slot` held.
    pub fn equip(&mut self, slot: usize, emote: usize) {
        if slot >= SLOTS {
            return;
        }
        if let Some(other) = self.slots.iter().position(|s| *s == Some(emote)) {
            self.slots[other] = self.slots[slot];
        }
        self.slots[slot] = Some(emote);
    }

    pub fn empty(&mut self, slot: usize) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = None;
        }
    }

    /// The file's one line: the keys in slot order, empty slots as nothing.
    pub fn line(&self) -> String {
        self.slots.iter().map(|s| s.map_or("", |i| CATALOG[i].key)).collect::<Vec<_>>().join(",")
    }

    /// A file's line; unknown keys leave their slot empty.
    pub fn read_line(line: &str) -> [Option<usize>; SLOTS] {
        let mut slots = [None; SLOTS];
        for (s, k) in slots.iter_mut().zip(line.trim().split(',')) {
            *s = by_key(k).filter(|_| !k.trim().is_empty());
        }
        slots
    }

    /// `NVF_PLAY_TEST_EMOTE_SLOTS`: `<n>:<key>` pairs over the defaults.
    pub fn read_aid(&mut self, aid: &str) {
        for part in aid.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let Some((n, k)) = part.split_once(':') else {
                warn!("NVF_PLAY_TEST_EMOTE_SLOTS: {part:?}: expected <slot>:<emote>");
                continue;
            };
            let Some(slot) = n.trim().parse::<usize>().ok().filter(|n| (1..=SLOTS).contains(n)).map(|n| n - 1) else {
                warn!("NVF_PLAY_TEST_EMOTE_SLOTS: {part:?}: no slot {n}");
                continue;
            };
            if k.trim().is_empty() {
                self.empty(slot);
            } else if let Some(e) = by_key(k) {
                self.equip(slot, e);
            } else {
                warn!("NVF_PLAY_TEST_EMOTE_SLOTS: {part:?}: no emote {k:?}");
            }
        }
    }

    pub fn save(&self) {
        let Some(f) = self.file.as_ref() else { return };
        match std::fs::write(f, self.line() + "\n") {
            Ok(()) => info!("emotes: loadout kept in {}: {}", f.display(), self.line()),
            Err(e) => warn!("emotes: could not keep the loadout in {}: {e}", f.display()),
        }
    }
}

/// What the wheel and the test aids ask of the emote player (nvforever `emote_play.rs`).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum EmoteAsk {
    /// Play catalogue emote `emote`; `cancel_after`: a test aid ends it that long after;
    /// `variant`: a test aid's choice of a one-shot's clip (else one at random).
    Play { emote: usize, cancel_after: Option<f32>, variant: Option<u8> },
    /// End the emote playing (its exit, if a loop's).
    Cancel,
}

/// Where the other players' heads are (game space, by their puppets' references): their
/// drawn head bones, standing, in an emote or seated on furniture, for their name tags
/// (`crate::world_marks`): written by nvforever `mate_heads.rs`.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct MateHeads(pub HashMap<u32, [f32; 3]>);

/// A slot's slice's middle, degrees clockwise from straight up: slot 1 at the top.
pub fn angle_of(slot: usize) -> f32 {
    slot as f32 * 360.0 / SLOTS as f32
}

/// The slot in direction `p` from the centre (y down).
pub fn slot_at(p: Vec2) -> usize {
    let step = 360.0 / SLOTS as f32;
    let a = p.x.atan2(-p.y).to_degrees().rem_euclid(360.0);
    (((a + step / 2.0) / step).floor() as usize) % SLOTS
}

/// The wheel open.
#[derive(Debug, Clone, PartialEq)]
pub struct Wheel {
    pub picked: Option<usize>,
    pointer: Vec2,
}

impl Wheel {
    fn new() -> Wheel {
        Wheel { picked: None, pointer: Vec2::ZERO }
    }

    fn moved(&mut self, by: Vec2) {
        self.pointer += by;
        if self.pointer.length() > crate::weapon_wheel::REACH {
            self.pointer = self.pointer.normalize() * crate::weapon_wheel::REACH;
        }
        if self.pointer.length() >= crate::weapon_wheel::DEAD_ZONE {
            self.picked = Some(slot_at(self.pointer));
        }
    }
}

#[derive(Resource, Default)]
pub struct EmoteWheel {
    pub wheel: Option<Wheel>,
    /// Real seconds Emote has been held (before the wheel opens).
    held: Option<f32>,
    /// The wheel closed and the buttons that closed it are still down.
    release: bool,
    /// A test aid holds the wheel open (no key down).
    aid_open: bool,
}

impl EmoteWheel {
    pub fn is_open(&self) -> bool {
        self.wheel.is_some()
    }
}

/// `NVF_PLAY_TEST_EMOTE`.
#[derive(Resource, Default)]
struct EmoteAid(Vec<(f32, String)>);

pub struct EmotesPlugin;

impl Plugin for EmotesPlugin {
    fn build(&self, app: &mut App) {
        let mut aid = Vec::new();
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_EMOTE") {
            aid = read_aid(&v);
            info!("test aid NVF_PLAY_TEST_EMOTE: {} steps", aid.len());
        }
        app.init_resource::<EmoteBook>()
            .init_resource::<EmoteSlots>()
            .init_resource::<EmoteWheel>()
            .init_resource::<MateHeads>()
            .insert_resource(EmoteAid(aid))
            .add_message::<EmoteAsk>()
            .add_systems(Startup, load_slots)
            .add_systems(Update, book.in_set(PlaySet::Ui))
            .add_systems(Update, (emote_aid, input).chain().in_set(PlaySet::Input).after(crate::weapon_wheel::read).before(crate::control::read_controls));
        draw::build(app);
    }
}

/// `<seconds>:<step>[:<seconds>]`, soonest first.
fn read_aid(v: &str) -> Vec<(f32, String)> {
    let mut out = Vec::new();
    for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once(':').and_then(|(t, a)| Some((t.trim().parse::<f32>().ok()?, a.trim().to_string()))) {
            Some(x) => out.push(x),
            None => warn!("NVF_PLAY_TEST_EMOTE: {part:?}: expected <seconds>:<step>"),
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// The loadout from the home (or the test aid's).
fn load_slots(mut slots: ResMut<EmoteSlots>, file: Option<Res<crate::settings::SettingsFile>>) {
    let path = file.and_then(|f| f.0.as_ref().and_then(|p| p.parent()).map(|d| d.join("emote-slots.txt")));
    if let Ok(aid) = std::env::var("NVF_PLAY_TEST_EMOTE_SLOTS") {
        slots.read_aid(&aid);
        slots.file = None;
        info!("test aid NVF_PLAY_TEST_EMOTE_SLOTS: loadout {}", slots.line());
        return;
    }
    if let Some(p) = path.as_ref() {
        if let Ok(s) = std::fs::read_to_string(p) {
            slots.slots = EmoteSlots::read_line(s.lines().next().unwrap_or(""));
            info!("emotes: loadout from {}: {}", p.display(), slots.line());
        }
    }
    slots.file = path;
}

/// What the load order has, once, and the id list for the test logs.
fn book(mut book: ResMut<EmoteBook>, data: Option<Res<nvf_assets::GameData>>) {
    if book.ready {
        return;
    }
    let Some(data) = data else { return };
    *book = EmoteBook::of(|p| data.vfs.exists(p));
    let ag = book.installed.iter().zip(CATALOG).filter(|(_, e)| matches!(e.shape, Shape::Once { ag, .. } if !ag.is_empty())).any(|(inst, e)| {
        let Shape::Once { vanilla, .. } = e.shape else { return false };
        inst.iter().skip(vanilla.len()).any(|b| *b)
    });
    let list: Vec<String> = CATALOG
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let n = book.installed[i].iter().filter(|b| **b).count();
            let how = if e.is_loop() { "loop".to_string() } else { format!("once, {n} clips") };
            format!("{} {}{} ({how})", Emote::id(i), e.key, if book.available[i] { "" } else { " MISSING" })
        })
        .collect();
    info!("emotes: {} of {} available (Animated Greetings {}): {}", book.available.iter().filter(|b| **b).count(), CATALOG.len(), if ag { "installed" } else { "not installed" }, list.join(", "));
}

#[allow(clippy::too_many_arguments)]
fn emote_aid(
    aid_time: crate::pause::AidTime,
    mut aid: ResMut<EmoteAid>,
    mut wheel: ResMut<EmoteWheel>,
    slots: Res<EmoteSlots>,
    mut asks: MessageWriter<EmoteAsk>,
    mut control: ResMut<crate::control::PlayerControl>,
) {
    let Some(now) = aid_time.world_secs() else { return };
    while aid.0.first().is_some_and(|(t, _)| *t <= now) {
        let (_, step) = aid.0.remove(0);
        info!("test aid NVF_PLAY_TEST_EMOTE: {step}");
        let (head, rest) = step.split_once(':').map_or((step.as_str(), None), |(a, b)| (a, Some(b)));
        match head {
            "cancel" => {
                asks.write(EmoteAsk::Cancel);
            }
            "1st" => control.pov = crate::control::Pov::First,
            "3rd" => control.pov = crate::control::Pov::Third,
            "wheel" => {
                wheel.wheel = Some(Wheel::new());
                wheel.aid_open = true;
                asks.write(EmoteAsk::Cancel);
            }
            "close" => {
                wheel.wheel = None;
                wheel.aid_open = false;
            }
            "release" => {
                let picked = wheel.wheel.take().and_then(|w| w.picked);
                wheel.aid_open = false;
                match picked.and_then(|s| slots.slots[s]) {
                    Some(e) => {
                        info!("emotes: wheel: slot {} plays {}", picked.unwrap_or(0) + 1, CATALOG[e].key);
                        asks.write(EmoteAsk::Play { emote: e, cancel_after: None, variant: None });
                    }
                    None => info!("emotes: wheel closed: nothing chosen"),
                }
            }
            _ => {
                if let Some(n) = head.strip_prefix("pick=").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=SLOTS).contains(n)) {
                    match wheel.wheel.as_mut() {
                        Some(w) => {
                            w.pointer = crate::weapon_wheel::point(angle_of(n - 1), crate::weapon_wheel::REACH);
                            w.picked = Some(n - 1);
                        }
                        None => warn!("NVF_PLAY_TEST_EMOTE: pick={n}: no wheel open"),
                    }
                } else if let Some(e) = by_key(head.split_once('@').map_or(head, |(k, _)| k)) {
                    let cancel_after = rest.and_then(|r| r.trim().parse::<f32>().ok());
                    let variant = head.split_once('@').and_then(|(_, name)| {
                        let want = format!("\\{}.kf", name.trim().to_ascii_lowercase());
                        let found = CATALOG[e].candidates().iter().position(|p| p.to_ascii_lowercase().ends_with(&want));
                        if found.is_none() {
                            warn!("NVF_PLAY_TEST_EMOTE: {head:?}: {} has no clip {name:?}", CATALOG[e].key);
                        }
                        found.map(|i| i as u8)
                    });
                    asks.write(EmoteAsk::Play { emote: e, cancel_after, variant });
                } else {
                    warn!("NVF_PLAY_TEST_EMOTE: no step {head:?}");
                }
            }
        }
    }
}

/// Emote held opens the wheel, let go plays the slot picked; a tap ends the emote
/// playing. The game's own controls stay off while the wheel is open (as the weapon
/// wheel's, `PlayerControl::wheel`).
#[allow(clippy::too_many_arguments)]
fn input(
    input: ControlInput,
    motion: Res<AccumulatedMouseMotion>,
    real: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut w: ResMut<EmoteWheel>,
    slots: Res<EmoteSlots>,
    book: Res<EmoteBook>,
    mut asks: MessageWriter<EmoteAsk>,
    mut control: ResMut<crate::control::PlayerControl>,
) {
    let playing = *state.get() == PlayState::Playing;
    let dt = real.delta_secs();
    if !playing {
        if w.wheel.take().is_some() {
            info!("emotes: wheel closed: the game left play");
        }
        w.held = None;
        w.aid_open = false;
        return;
    }
    let other_wheel = control.wheel && w.wheel.is_none();
    if w.wheel.is_none() && !other_wheel {
        if input.just_pressed(Control::Emote) {
            w.held = Some(0.0);
        }
        if let Some(h) = w.held.as_mut() {
            *h += dt;
            if !input.pressed(Control::Emote) {
                // A tap: end the emote playing.
                w.held = None;
                asks.write(EmoteAsk::Cancel);
            } else if *h >= crate::weapon_wheel::HOLD {
                w.held = None;
                w.wheel = Some(Wheel::new());
                // Opening the wheel ends the emote playing (the request's behaviour).
                asks.write(EmoteAsk::Cancel);
                info!("emotes: wheel open: {}", slots.line());
            }
        }
    }
    let k = windows.single().map_or(1.0, |win| 960.0 / win.height().max(1.0));
    let aid_open = w.aid_open;
    let mut close = None;
    if let Some(wheel) = w.wheel.as_mut() {
        if !aid_open {
            wheel.moved(motion.delta * k);
            // A controller's right stick points at a slot (as the weapon wheel's); let go,
            // the last slot pointed at stays picked.
            if let Some(st) = crate::weapon_wheel::wheel_stick(&input).filter(|v| v.length() >= 0.5) {
                wheel.pointer = st.normalize() * crate::weapon_wheel::REACH;
                wheel.picked = Some(slot_at(wheel.pointer));
            }
        }
        let pad_back = input.bindings.device == crate::bindings::InputDevice::Controller && input.pads.iter().any(|g| g.just_pressed(GamepadButton::East));
        if input.mouse.just_pressed(MouseButton::Right) || input.keys.just_pressed(KeyCode::Escape) || pad_back {
            close = Some(None);
        } else if !aid_open && (!input.pressed(Control::Emote) || input.mouse.just_pressed(MouseButton::Left)) {
            close = Some(wheel.picked);
        }
    }
    if let Some(picked) = close {
        w.wheel = None;
        w.aid_open = false;
        w.release = true;
        match picked.and_then(|s| slots.slots[s].map(|e| (s, e))) {
            Some((s, e)) if book.available(e) => {
                info!("emotes: wheel: slot {} plays {}", s + 1, CATALOG[e].key);
                asks.write(EmoteAsk::Play { emote: e, cancel_after: None, variant: None });
            }
            Some((s, e)) => info!("emotes: wheel: slot {} ({}) is not installed", s + 1, CATALOG[e].key),
            None => info!("emotes: wheel closed: nothing chosen"),
        }
    }
    if w.release && !input.mouse.pressed(MouseButton::Left) && !input.mouse.pressed(MouseButton::Right) && !input.pressed(Control::Use) {
        w.release = false;
    }
    if w.wheel.is_some() || w.release {
        control.wheel = true;
        (control.trigger, control.aiming) = (false, false);
    }
}

pub mod draw {
    //! The emote wheel's look: the co-op interaction wheel's (`crate::coop_social::draw`),
    //! eight slices with the emotes' names.

    use super::{angle_of, EmoteBook, EmoteSlots, EmoteWheel, CATALOG, SLOTS};
    use crate::interface::{amber, at, face, plain_font, Face, Pictures};
    use crate::weapon_wheel::draw::{slice_uniform, slice_uniform_gap, text, SliceMaterial, FONT_1, FONT_3, HUB, INNER, LINE, MATTE, OUTER, RAISE, RIM, SIZE};
    use bevy::image::CompressedImageFormatSupport;
    use bevy::prelude::*;
    use bevy::ui_render::prelude::MaterialNode;
    use bevy::window::PrimaryWindow;
    use nvf_assets::GameData;

    #[derive(Debug, Clone, PartialEq)]
    struct Shown {
        picked: Option<usize>,
        names: Vec<Option<(String, bool)>>,
        width: i32,
        key: String,
    }

    struct Kit {
        large: Face,
        medium: Face,
        pictures: Pictures,
    }

    #[derive(Component)]
    struct EmoteWheelRoot;

    /// A slice not picked: its name this strong (the weapon wheel's).
    const DIM: f32 = 0.62;
    /// The gap between slices (the interaction wheel's).
    const SLICE_GAP: (f32, f32) = (8.0, 13.0);

    fn gap_at(r: f32) -> f32 {
        SLICE_GAP.0 + (SLICE_GAP.1 - SLICE_GAP.0) * (r - INNER) / (OUTER - INNER)
    }

    /// A name in at most two lines of about `room` characters.
    fn lines(name: &str, room: usize) -> (String, String) {
        if name.len() <= room {
            return (name.to_string(), String::new());
        }
        let words: Vec<&str> = name.split(' ').collect();
        let half = words.len().div_ceil(2);
        (words[..half].join(" "), words[half..].join(" "))
    }

    pub(super) fn build(app: &mut App) {
        if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
            app.add_systems(Update, draw_wheel.in_set(crate::PlaySet::Ui));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_wheel(
        mut commands: Commands,
        wheel: Res<EmoteWheel>,
        slots: Res<EmoteSlots>,
        book: Res<EmoteBook>,
        roots: Query<Entity, With<EmoteWheelRoot>>,
        windows: Query<&Window, With<PrimaryWindow>>,
        (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<SliceMaterial>>,
        bindings: Option<Res<crate::bindings::Bindings>>,
        mut last: Local<Option<Shown>>,
        mut kit: Local<Option<Kit>>,
    ) {
        let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
        let key = bindings.as_deref().map_or_else(|| "B".to_string(), |b| b.prompt(crate::bindings::Control::Emote).trim_end_matches(')').to_string());
        let now = wheel.wheel.as_ref().map(|w| Shown {
            picked: w.picked,
            names: slots.slots.iter().map(|s| s.map(|e| (CATALOG[e].name.to_string(), book.available(e)))).collect(),
            width: width.round() as i32,
            key: key.clone(),
        });
        if now == *last {
            return;
        }
        *last = now.clone();
        for e in &roots {
            commands.entity(e).despawn();
        }
        let Some(s) = now else { return };
        let bc = crate::interface::bc_supported(formats.as_deref());
        let data = data.as_deref();
        let kit = kit.get_or_insert_with(|| Kit {
            large: face(FONT_1, plain_font(31.0, 12.0), data, &mut images, bc),
            medium: face(FONT_3, plain_font(25.0, 10.0), data, &mut images, bc),
            pictures: Pictures::load(data),
        });
        let bright = amber(255.0);
        let dark = Color::srgb(0.08, 0.055, 0.025);
        let left = (width - SIZE) / 2.0;
        let top = (960.0 - SIZE) / 2.0 - RAISE;
        let c = SIZE / 2.0;
        let half = 180.0 / SLOTS as f32;
        let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
        let matte = kit.pictures.picture(MATTE, data, &mut images, bc);
        commands.spawn((EmoteWheelRoot, full, BackgroundColor(Color::BLACK.with_alpha(crate::weapon_wheel::DARKEN)), GlobalZIndex(105))).with_children(|r| {
            r.spawn(at(left, top, SIZE, SIZE)).with_children(|m| {
                if let Some((image, rect)) = matte {
                    m.spawn((at(0.0, 0.0, SIZE, SIZE), ImageNode { rect, color: Color::BLACK.with_alpha(0.88), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
                }
                for i in 0..SLOTS {
                    let mid = angle_of(i);
                    let (from, to) = (mid - half, mid + half);
                    let lit = Some(i) == s.picked;
                    let empty = s.names[i].as_ref().is_none_or(|(_, ok)| !ok);
                    let (fill, edge) = if lit { (bright.with_alpha(if empty { 0.08 } else { 0.15 }), bright.with_alpha(0.9)) } else { (dark.with_alpha(0.6), bright.with_alpha(0.35)) };
                    m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform_gap(from, to, INNER, OUTER, fill, edge, if lit { 1.5 * LINE } else { LINE }, SLICE_GAP) }))));
                    if lit {
                        m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform_gap(from, to, RIM[0], RIM[1], bright, bright, 0.0, (gap_at(RIM[0]), gap_at(RIM[1]))) }))));
                    }
                }
                m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform(0.0, 360.0, 0.0, HUB, dark.with_alpha(0.92), bright.with_alpha(0.63), LINE) }))));
                let size = kit.medium.font.size;
                for i in 0..SLOTS {
                    let lit = Some(i) == s.picked;
                    let p = crate::weapon_wheel::point(angle_of(i), (INNER + OUTER) / 2.0) + Vec2::splat(c);
                    let (name, ok) = s.names[i].clone().unwrap_or_else(|| ("Empty".to_string(), false));
                    let strength = if lit { 1.0 } else { DIM } * if ok { 1.0 } else { 0.5 };
                    let (a, b) = lines(&name.to_uppercase(), 10);
                    let y = if b.is_empty() { p.y - size / 2.0 } else { p.y - size };
                    text(m, &kit.medium, &format!("{}", i + 1), p.x, y - size, 0, bright.with_alpha(strength * 0.7));
                    text(m, &kit.medium, &a, p.x, y, 0, bright.with_alpha(strength));
                    if !b.is_empty() {
                        text(m, &kit.medium, &b, p.x, y + size, 0, bright.with_alpha(strength));
                    }
                }
                let centre = s.picked.and_then(|i| s.names[i].as_ref()).map_or(String::new(), |(n, ok)| if *ok { n.clone() } else { format!("{n} (not installed)") });
                text(m, &kit.medium, "EMOTES", c, c - 40.0, 0, bright.with_alpha(0.7));
                text(m, &kit.large, &centre, c, c - 10.0, 0, bright);
            });
            // Above the wheel: under it the line ran into the compass and the AP bar.
            let hint = format!("Mouse: pick     Let go of {}: play     Esc: cancel", s.key);
            text(r, &kit.medium, &hint, width / 2.0, (top - 8.0).max(4.0), 0, bright.with_alpha(0.8));
        });
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn long_names_go_on_two_lines() {
            assert_eq!(super::lines("SIT", 10), ("SIT".into(), String::new()));
            assert_eq!(super::lines("SIT CROSS-LEGGED", 10), ("SIT".into(), "CROSS-LEGGED".into()));
            assert_eq!(super::lines("LAUGH AND POINT", 10), ("LAUGH AND".into(), "POINT".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_the_defaults_are_known() {
        for (i, e) in CATALOG.iter().enumerate() {
            assert_eq!(by_key(e.key), Some(i), "{}", e.key);
            assert!(CATALOG.iter().filter(|o| o.key == e.key).count() == 1);
            match e.shape {
                Shape::Once { vanilla, ag } => assert!(!vanilla.is_empty() || !ag.is_empty()),
                Shape::Loop { stay, .. } => assert!(!stay.is_empty()),
            }
        }
        assert!(CATALOG.len() < 255, "ids fit a byte");
        let d = EmoteSlots::default();
        assert!(d.slots.iter().all(Option::is_some));
        assert_eq!(d.line(), "wave,salute,sit,lay,pushups,shrug,cheer,dance");
        assert_eq!(by_key("3"), by_key("sit"));
    }

    #[test]
    fn equipping_swaps_and_slots_empty() {
        let mut s = EmoteSlots::default();
        let sit = by_key("sit").unwrap();
        s.equip(0, sit);
        assert_eq!(s.slots[0], Some(sit));
        assert_eq!(s.slots[2], by_key("wave"), "the slot sit left holds what slot 1 held");
        s.empty(7);
        assert_eq!(s.slots[7], None);
        let line = s.line();
        assert!(line.ends_with(','), "{line}");
        assert_eq!(EmoteSlots::read_line(&line), s.slots);
        let mut a = EmoteSlots::default();
        a.read_aid("1:kneel, 8:, 2:nothing");
        assert_eq!(a.slots[0], by_key("kneel"));
        assert_eq!(a.slots[7], None);
        assert_eq!(a.slots[1], by_key("salute"));
    }

    #[test]
    fn the_book_knows_what_is_installed() {
        let book = EmoteBook::of(|p| !p.starts_with("meshes\\AG_") && !p.contains("sitfloorcross"));
        let wave = by_key("wave").unwrap();
        assert_eq!(book.installed[wave], vec![true, true, false, false]);
        assert!(book.available(wave));
        assert!(!book.available(by_key("bow").unwrap()), "the mod's alone");
        assert!(!book.available(by_key("sit-cross").unwrap()), "Honest Hearts not loaded");
        assert_eq!(CATALOG[wave].candidates()[2], "meshes\\AG_Wave.kf");
        assert_eq!(vanilla("sitfloorenter"), "meshes\\characters\\_male\\idleanims\\sitfloorenter.kf");
    }

    #[test]
    fn slices_by_direction() {
        for i in 0..SLOTS {
            let a = angle_of(i);
            assert_eq!(slot_at(crate::weapon_wheel::point(a, 100.0)), i);
            assert_eq!(slot_at(crate::weapon_wheel::point(a + 20.0, 100.0)), i);
            assert_eq!(slot_at(crate::weapon_wheel::point(a - 20.0, 100.0)), i);
        }
    }

    #[test]
    fn the_aid_reads_in_order() {
        let a = read_aid("9:sit:4, 3:wave, x, 5:1st");
        assert_eq!(a, vec![(3.0, "wave".into()), (5.0, "1st".into()), (9.0, "sit:4".into())]);
    }
}
