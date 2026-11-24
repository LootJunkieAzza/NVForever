//! The animation folders of the B42 Inject and B42 Interact mods, played natively (the
//! co-op fork's own; docs/spec-notes/b42-anims.md). The mods ship KF clips in folders
//! named for the items they go with and leave the playing to a script extender; this
//! module reads those folders' conventions, and the engine (`consume_anims.rs`) plays them.
//!
//! - **Consume** (B42 Inject): `meshes\characters\_male\idleanims\b42inject\<0 | 1
//!   power armour>\<0 third person | 1 first person>\<item EditorID>\*.kf`, one picked at
//!   random each use [observed: the mods' folders and readme files; the folder names
//!   say which view and armour].
//! - **Interact** (B42 Interact): `meshes\b42interact\edid\<item EditorID>\add\<dflt |
//!   once>\[<creature folder>\]1st\*.kf` beside a `cnd.ini` naming when it plays, on the
//!   item coming into the player's inventory; `once` only the first time [observed].
//! - **Crafting**: `meshes\b42interact\awr\edid\<recipe EditorID>\1st\*.kf` [observed].
//! - **Activation by type** (B42 Interact itself): `meshes\b42interact\type\<form type>\
//!   <category>\<base | key | lock | -1>\<1st | 3rd>\[<stance>\]*.kf`, played before
//!   an activator (21), terminal (23), container (27), door (28), person (42) or creature
//!   (43) is activated ([`Index::type_clips`]; `Config\B42Interact\Settings.ini` turns a
//!   type off, `Blacklist.ini` names objects left out) [observed: the mod's folders, its
//!   ini files and its script].
//! - **Text keys** ([`cues`]): the clips call the mod's script through `ScriptLine:` keys
//!   (attach the model in the left hand, take it away, play the item's own sound, play a
//!   file) and kNVSE's `SoundPath:` key; [`cues`] reads the lines the packs use
//!   [observed: the installed packs' text keys].
//!
//! Paths here are normalised (`fnv_vfs::normalize`: lower case, backslashes).

use std::collections::HashMap;

/// Where the consume clips are.
pub const INJECT_ROOT: &str = "meshes\\characters\\_male\\idleanims\\b42inject\\";
/// Where the interact and crafting clips are.
pub const INTERACT_ROOT: &str = "meshes\\b42interact\\";
/// Where the clips by form type are (under [`INTERACT_ROOT`]).
pub const TYPE_ROOT: &str = "meshes\\b42interact\\type\\";
/// The form types the mod animates [observed: `Settings.ini` [Interactions]].
pub const TYPES: [u32; 6] = [ACTIVATOR, TERMINAL, CONTAINER, DOOR, PERSON, CREATURE];
pub const ACTIVATOR: u32 = 21;
pub const TERMINAL: u32 = 23;
pub const CONTAINER: u32 = 27;
pub const DOOR: u32 = 28;
pub const PERSON: u32 = 42;
pub const CREATURE: u32 = 43;
/// The node the clips hang the held model from, under the left hand [observed: the
/// script inserts `Bip01 L Hand|B42AttachPointL`; the clips carry a track for it].
pub const ATTACH_NODE: &str = "B42AttachPointL";
pub const HAND_NODE: &str = "Bip01 L Hand";
/// The suffix the attached model's node names take, which the clips' tracks name
/// (`Cigar` → `Cigar_B42`) [observed: `AttachModel … "B42AttachPointL|*_B42*"`, the tracks].
pub const NODE_SUFFIX: &str = "_B42";

/// The view a clip is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Third,
    First,
}

/// When an interact clip plays: every time the item comes, or the first time only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Default,
    Once,
}

/// A `cnd.ini`, read simply (module notes): the mod's lines are a script; the two kinds
/// the packs ship are told apart by what they test [observed: 98 test
/// `Player.IsInCombat == 0`, 40 test a dead creature the item came from].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cond {
    pub not_in_combat: bool,
    /// The item must come from a dead creature's body near the player.
    pub corpse: Option<Corpse>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corpse {
    /// How near (units) [observed: `GetDistance3D Player < 500`].
    pub within: f32,
    /// `GetCreatureType == n` (true) or `!= n` (false).
    pub creature_type: Option<(bool, i32)>,
}

/// The item's source as the engine sees it, for [`Cond`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Source {
    /// The container the item came from is a dead creature: its model folder
    /// (`creatures\<folder>\…`), its creature type, how far from the player.
    pub creature: Option<(String, i32, f32)>,
    /// That body was skinned (gave an interact clip) before: each body once
    /// [observed: the conditions mark the body by its LeftMobilityCondition].
    pub used: bool,
}

/// One interact entry: the clips of one `add\<mode>\[<creature>\]` folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Interact {
    pub mode: Mode,
    pub creature: Option<String>,
    pub cond: Cond,
    pub clips: Vec<String>,
}

/// The clips found under the two roots.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// (power armour, view, item EditorID lower case) → clip paths, sorted.
    inject: HashMap<(bool, View, String), Vec<String>>,
    /// Item EditorID lower case → entries.
    interact: HashMap<String, Vec<Interact>>,
    /// Recipe EditorID lower case → clip paths.
    recipes: HashMap<String, Vec<String>>,
    /// Type clips by their folders; all lower case.
    types: HashMap<TypeKey, Vec<String>>,
}

/// The folders of a type clip: form type, category, variant, view, stance.
type TypeKey = (u32, String, String, View, Option<String>);

/// A lock as the player meets it, for the variant folder of a type clip [observed: the
/// script's `Key` (the player carries the key), `Lock` (locked, within the skill) and
/// `-1` (beyond it, or broken) folders].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockSeen {
    Open,
    Key,
    Pickable,
    Beyond,
}

/// The weapon stance folder of a type clip, from the drawn weapon's animation type
/// (WEAP DNAM, `GetEquippedWeaponType`), `holst` when none is drawn [observed: the
/// script's table].
pub fn stance(drawn: Option<i64>) -> &'static str {
    match drawn {
        None => "holst",
        Some(t) if t < 2 => "h2h1hm",
        Some(2) => "2hm",
        Some(t) if t > 9 => "1gt",
        Some(t) if t < 5 => "1hp",
        Some(8) => "2hh",
        Some(6) => "2ha",
        Some(9) => "2hl",
        Some(_) => "2hr",
    }
}

impl Index {
    /// From every path of the data (normalised) and a reader of small text files
    /// (`cnd.ini`).
    pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>, read: impl Fn(&str) -> Option<String>) -> Index {
        let mut ix = Index::default();
        let mut conds: HashMap<String, Cond> = HashMap::new();
        let mut interact: HashMap<(String, Mode, Option<String>), Vec<String>> = HashMap::new();
        for p in paths {
            if let Some(rest) = p.strip_prefix(INJECT_ROOT) {
                let parts: Vec<&str> = rest.split('\\').collect();
                if let [pa @ ("0" | "1"), view @ ("0" | "1"), edid, file] = parts[..] {
                    if file.ends_with(".kf") {
                        let view = if view == "1" { View::First } else { View::Third };
                        ix.inject.entry((pa == "1", view, edid.to_string())).or_default().push(p.to_string());
                    }
                }
            } else if let Some(rest) = p.strip_prefix(TYPE_ROOT) {
                let parts: Vec<&str> = rest.split('\\').collect();
                let (code, cat, variant, view, stance, file) = match parts[..] {
                    [c, cat, v, view, file] => (c, cat, v, view, None, file),
                    [c, cat, v, view, st, file] => (c, cat, v, view, Some(st.to_string()), file),
                    _ => continue,
                };
                let view = match view {
                    "1st" => View::First,
                    "3rd" => View::Third,
                    _ => continue,
                };
                if let (Ok(code), true) = (code.parse::<u32>(), file.ends_with(".kf")) {
                    ix.types.entry((code, cat.to_string(), variant.to_string(), view, stance)).or_default().push(p.to_string());
                }
            } else if let Some(rest) = p.strip_prefix(INTERACT_ROOT) {
                let parts: Vec<&str> = rest.split('\\').collect();
                match parts[..] {
                    ["awr", "edid", recipe, "1st", file] if file.ends_with(".kf") => ix.recipes.entry(recipe.to_string()).or_default().push(p.to_string()),
                    ["edid", edid, "add", mode, ref tail @ ..] => {
                        let Some(mode) = mode_of(mode) else { continue };
                        let (creature, last) = match tail {
                            ["1st", file] => (None, Some(*file)),
                            [c, "1st", file] => (Some(c.to_string()), Some(*file)),
                            ["cnd.ini"] | [_, "cnd.ini"] => {
                                if let Some(text) = read(p) {
                                    conds.insert(p.trim_end_matches("cnd.ini").to_string(), parse_cnd(&text));
                                }
                                (None, None)
                            }
                            _ => (None, None),
                        };
                        if last.is_some_and(|f| f.ends_with(".kf")) {
                            interact.entry((edid.to_string(), mode, creature)).or_default().push(p.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        for ((edid, mode, creature), mut clips) in interact {
            clips.sort();
            let folder = match &creature {
                Some(c) => format!("{INTERACT_ROOT}edid\\{edid}\\add\\{}\\{c}\\", mode_name(mode)),
                None => format!("{INTERACT_ROOT}edid\\{edid}\\add\\{}\\", mode_name(mode)),
            };
            let cond = conds.get(&folder).cloned().unwrap_or_default();
            ix.interact.entry(edid).or_default().push(Interact { mode, creature, cond, clips });
        }
        for v in ix.interact.values_mut() {
            // Those for one creature first, so a body's own clip wins over a general one.
            v.sort_by_key(|e| (e.creature.is_none(), e.creature.clone()));
        }
        for v in ix.inject.values_mut().chain(ix.recipes.values_mut()).chain(ix.types.values_mut()) {
            v.sort();
        }
        ix
    }

    /// The consume clips for an item.
    pub fn inject(&self, edid: &str, power_armor: bool, view: View) -> &[String] {
        self.inject.get(&(power_armor, view, edid.to_ascii_lowercase())).map_or(&[], Vec::as_slice)
    }

    /// The items (EditorIDs, lower case) with any consume clip.
    pub fn inject_items(&self) -> impl Iterator<Item = &str> {
        self.inject.keys().map(|(_, _, e)| e.as_str())
    }

    /// The items with any interact entry.
    pub fn interact_items(&self) -> impl Iterator<Item = &str> {
        self.interact.keys().map(String::as_str)
    }

    pub fn recipe(&self, edid: &str) -> &[String] {
        self.recipes.get(&edid.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// The form types with clips, and how many clips by type there are.
    pub fn type_counts(&self) -> (Vec<u32>, usize) {
        let mut codes: Vec<u32> = self.types.keys().map(|k| k.0).collect();
        codes.sort_unstable();
        codes.dedup();
        (codes, self.types.values().map(Vec::len).sum())
    }

    /// A type has clips for a category (any variant or view).
    pub fn has_category(&self, code: u32, category: &str) -> bool {
        self.types.keys().any(|k| k.0 == code && k.1.eq_ignore_ascii_case(category))
    }

    fn has_variant(&self, code: u32, category: &str, variant: &str) -> bool {
        self.types.keys().any(|k| k.0 == code && k.1 == category && k.2 == variant)
    }

    /// The clips to play before activating an object of form type `code` (module notes):
    /// the variant folder of the lock (`key`, `lock`, else `base`; `key` and `lock` fall
    /// back to `base`, `-1` does not), the view's folder (none there: no clip, as the mod
    /// plays none for an activation it would have to switch the view for), then the
    /// stance's folder if it has clips, else the view folder's own [observed: the script].
    pub fn type_clips(&self, code: u32, category: &str, lock: LockSeen, view: View, stance: &str) -> &[String] {
        let category = category.to_ascii_lowercase();
        let wanted = match lock {
            LockSeen::Open => "base",
            LockSeen::Key => "key",
            LockSeen::Pickable => "lock",
            LockSeen::Beyond => "-1",
        };
        let variant = if matches!(lock, LockSeen::Key | LockSeen::Pickable) && !self.has_variant(code, &category, wanted) { "base" } else { wanted };
        let key = |st: Option<String>| (code, category.clone(), variant.to_string(), view, st);
        if let Some(v) = self.types.get(&key(Some(stance.to_string()))).filter(|v| !v.is_empty()) {
            return v;
        }
        self.types.get(&key(None)).map_or(&[], Vec::as_slice)
    }

    /// The third-person clips of the folder a type clip came from, for another player's
    /// puppet (the third-person folders have no stance folders [observed]).
    pub fn type_third(&self, clip: &str) -> &[String] {
        let Some(rest) = clip.strip_prefix(TYPE_ROOT) else { return &[] };
        let parts: Vec<&str> = rest.split('\\').collect();
        let (Some(code), Some(cat), Some(variant)) = (parts.first().and_then(|c| c.parse::<u32>().ok()), parts.get(1), parts.get(2)) else { return &[] };
        self.types.get(&(code, cat.to_string(), variant.to_string(), View::Third, None)).map_or(&[], Vec::as_slice)
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        (self.inject.values().map(Vec::len).sum(), self.interact.values().flat_map(|v| v.iter().map(|e| e.clips.len())).sum(), self.recipes.values().map(Vec::len).sum())
    }

    /// The third-person clip to show another player using `edid` (the puppet,
    /// module notes of `consume_anims.rs`): the one of the same file name as the clip
    /// played, else the one `roll` (0–1) picks; `None` when the item has none.
    pub fn third_for(&self, edid: &str, power_armor: bool, played: &str, roll: f32) -> Option<&str> {
        let clips = self.inject(edid, power_armor, View::Third);
        let clips = if clips.is_empty() && power_armor { self.inject(edid, false, View::Third) } else { clips };
        let name = file_name(played);
        clips.iter().find(|c| file_name(c) == name).or_else(|| pick(clips, roll)).map(String::as_str)
    }

    /// The interact clips for `edid` coming into the inventory now: the entries whose
    /// creature folder (when they name one) is the body's and whose conditions hold; a
    /// `once` entry only when the item was never had (`first`).
    pub fn interact(&self, edid: &str, source: &Source, in_combat: bool, first: bool) -> Option<&Interact> {
        self.interact.get(&edid.to_ascii_lowercase())?.iter().find(|e| {
            (e.mode == Mode::Default || first)
                && e.creature.as_ref().is_none_or(|c| source.creature.as_ref().is_some_and(|(f, _, _)| f.eq_ignore_ascii_case(c)))
                && holds(&e.cond, source, in_combat)
        })
    }
}

fn mode_of(s: &str) -> Option<Mode> {
    match s {
        "dflt" => Some(Mode::Default),
        "once" => Some(Mode::Once),
        _ => None,
    }
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Default => "dflt",
        Mode::Once => "once",
    }
}

fn file_name(p: &str) -> &str {
    p.rsplit('\\').next().unwrap_or(p)
}

/// The clip a roll (0–1) picks.
pub fn pick(clips: &[String], roll: f32) -> Option<&String> {
    if clips.is_empty() {
        return None;
    }
    let k = (roll.clamp(0.0, 1.0) * clips.len() as f32) as usize;
    clips.get(k.min(clips.len() - 1))
}

/// Whether a [`Cond`] holds.
pub fn holds(c: &Cond, s: &Source, in_combat: bool) -> bool {
    if c.not_in_combat && in_combat {
        return false;
    }
    let Some(k) = c.corpse else { return true };
    let Some((_, kind, distance)) = &s.creature else { return false };
    !s.used
        && *distance < k.within
        && match k.creature_type {
            Some((true, t)) => *kind == t,
            Some((false, t)) => *kind != t,
            None => true,
        }
}

/// Read a `cnd.ini` (module notes): its script line, by what it tests.
pub fn parse_cnd(text: &str) -> Cond {
    let t = text.to_ascii_lowercase();
    let mut c = Cond { not_in_combat: t.contains("isincombat == 0"), corpse: None };
    if t.contains("getdead") {
        let within = t.split("getdistance3d player <").nth(1).and_then(|r| r.split(|ch: char| !ch.is_ascii_digit() && ch != '.').find(|s| !s.is_empty())).and_then(|n| n.parse().ok()).unwrap_or(500.0);
        let creature_type = ["getcreaturetype ==", "getcreaturetype !="].iter().find_map(|k| {
            let n = t.split(k).nth(1)?.trim_start().split(|ch: char| !ch.is_ascii_digit() && ch != '-').next()?.parse().ok()?;
            Some((k.ends_with("=="), n))
        });
        c.corpse = Some(Corpse { within, creature_type });
    }
    c
}

/// What a text key of a clip asks for (module notes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cue {
    /// Draw the model in the left hand: the one named last ([`Cue::Model`]), else the
    /// item's own.
    Attach,
    /// Take it away.
    Detach,
    /// The model the next [`Cue::Attach`] draws, under `meshes\`.
    Model(String),
    /// A sound file, under `sound\`.
    SoundFile(String),
    /// The item's own consume sound (held back from the use, module notes).
    ItemSound,
    /// Activate the object now (a type clip; else at its end) [observed: `*_ACT`].
    Activate,
    /// The mod's control flags (`*_IntD`): 1–3 hide the HUD and block other activations,
    /// 2 and more hold the player still (not at a door, unless a load door), 3 is a
    /// skinning (the player is moved to the body), 4 and more let a second Activate end
    /// it, −1 gives everything back [observed: the script's comments].
    Controls(i32),
    /// A weapon bash's blow lands now (B42 Melee Bash's `Sound: B42BashHitFrameDetect`).
    Strike,
}

/// The cues of one text key. A `ScriptLine:` key may hold several lines, split by `%r`
/// [observed].
pub fn cues(key: &str) -> Vec<Cue> {
    let key = key.trim();
    let Some((head, rest)) = key.split_once(':') else { return Vec::new() };
    let head = head.trim().to_ascii_lowercase();
    let rest = rest.trim();
    match head.as_str() {
        "sound" if rest.split_whitespace().next().is_some_and(|n| n.eq_ignore_ascii_case("B42InjectSound")) => vec![Cue::ItemSound],
        "sound" if rest.split_whitespace().next().is_some_and(|n| n.eq_ignore_ascii_case("B42BashHitFrameDetect")) => vec![Cue::Strike],
        // Some keys repeat the head inside (`SoundPath:fx\Hit\SoundPath:fx\Hit\X.wav`):
        // the file is after the last [observed: the crafting pack].
        "soundpath" if !rest.is_empty() => {
            let low = rest.to_ascii_lowercase();
            let file = low.rfind("soundpath:").map_or(rest, |i| rest[i + "soundpath:".len()..].trim());
            vec![Cue::SoundFile(file.to_string())]
        }
        "scriptline" => rest.split("%r").filter_map(script_cue).collect(),
        _ => Vec::new(),
    }
}

/// The quoted strings of a line.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

fn script_cue(line: &str) -> Option<Cue> {
    let l = line.trim();
    let low = l.to_ascii_lowercase();
    let q = quoted(l);
    let first = q.first().map(|s| s.to_ascii_lowercase());
    let value = || l.rsplit(|c: char| c.is_whitespace() || c == '"').find(|s| !s.is_empty()).and_then(|v| v.parse::<f32>().ok());
    // B42 Inject's names (`*_Inj…`), B42 Interact's (`*_Int…`, `*_ACT`) and the
    // crafting pack's (`*AWR_…`) [observed: the installed packs' text keys].
    if low.contains("auxvarsetflt") {
        return match (first.as_deref(), value()) {
            (Some("*_injm" | "*_intm" | "*awr_attach"), Some(1.0)) => Some(Cue::Attach),
            (Some("*_injm" | "*_intm"), Some(3.0)) | (Some("*awr_attach"), Some(0.0 | 3.0)) => Some(Cue::Detach),
            (Some("*_injs"), Some(v)) if v != 0.0 => Some(Cue::ItemSound),
            (Some("*_act"), Some(v)) if v != 0.0 => Some(Cue::Activate),
            (Some("*_intd"), Some(v)) => Some(Cue::Controls(v as i32)),
            _ => None,
        };
    }
    if low.contains("auxvarsetstr") {
        let v = q.get(1).filter(|v| !v.is_empty())?.to_string();
        return match first.as_deref() {
            Some("*_injmp" | "*_intmp" | "*awr_model") => Some(Cue::Model(v)),
            Some("*_injsp" | "*_intsp") => Some(Cue::SoundFile(v)),
            _ => None,
        };
    }
    if low.contains("dispatcheventalt") {
        return match first.as_deref() {
            Some("b42ia" | "b42ga") => Some(Cue::Attach),
            Some("b42id" | "b42gd") => Some(Cue::Detach),
            Some("b42is") => Some(Cue::ItemSound),
            Some("b42ip") => q.get(1).map(|v| Cue::Model(v.to_string())),
            Some("b42isp") => q.get(1).map(|v| Cue::SoundFile(v.to_string())),
            _ => None,
        };
    }
    if low.contains("b42injectq.irunattach") {
        return Some(Cue::Attach);
    }
    if low.contains("b42injectq.irundetach") {
        return Some(Cue::Detach);
    }
    None
}

/// What stops a consume animation now (the mod's filter, module notes of
/// `consume_anims.rs`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Situation {
    pub swimming: bool,
    pub ragdolled: bool,
    pub reloading: bool,
    pub attacking: bool,
    /// Another consume animation (or a scripted idle) plays.
    pub busy: bool,
    /// A menu is open (the Pip-Boy): the animation waits for it to close.
    pub in_menu: bool,
    /// One is already waiting for the menu to close.
    pub queued: bool,
    pub power_armor: bool,
}

/// The settings (the settings page, `Config\B42Inject.ini`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub on: bool,
    /// Hardcore: an aid item cannot be used at all while busy [observed: the ini's
    /// note, "prevent usage of all registered aid items under specific circumstances"].
    pub hardcore: bool,
    /// Animations in power armour.
    pub in_power_armor: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { on: true, hardcore: false, in_power_armor: true }
    }
}

/// What a use of an item with clips comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Play now.
    Play,
    /// Play once the menu is closed.
    Wait,
    /// No animation; the item is used and its sound plays as usual.
    Skip(&'static str),
    /// Hardcore: not used at all.
    Refuse,
}

/// The mod's rule [observed: gl_B42Inject.txt's filter]: busy (an idle, a reload, an
/// attack, a ragdoll, one waiting while in a menu) counts 1, swimming or power armour
/// without the option 2; anything else plays (after the menu). Hardcore refuses
/// whatever counts 1.
pub fn decide(s: &Situation, o: &Options) -> Decision {
    if !o.on {
        return Decision::Skip("consume animations are off");
    }
    let busy = s.busy || s.reloading || s.attacking || s.ragdolled || (s.queued && s.in_menu);
    if busy && o.hardcore {
        return Decision::Refuse;
    }
    if s.swimming {
        return Decision::Skip("swimming");
    }
    if s.power_armor && !o.in_power_armor {
        return Decision::Skip("in power armour");
    }
    if busy {
        return Decision::Skip("busy");
    }
    if s.in_menu {
        Decision::Wait
    } else {
        Decision::Play
    }
}

/// How another player is seen using an item with no third-person clip (`consume_anims.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Eat,
    Drink,
    Inject,
    Smoke,
    /// Skinning a body (B42 Interact).
    Skin,
    /// At a workbench or a campfire.
    Craft,
    /// Looking an item over (a weapon picked up, an instrument).
    Handle,
}

impl Style {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Style {
        match c {
            1 => Style::Drink,
            2 => Style::Inject,
            3 => Style::Smoke,
            4 => Style::Skin,
            5 => Style::Craft,
            6 => Style::Handle,
            _ => Style::Eat,
        }
    }

    /// The game's own third-person idles for it, under `characters\_male\` [observed:
    /// FalloutNV.esm IDLE `SodaDrinkingStanding`, `BeerDrinkingStanding` →
    /// `DrinkingStandingA.kf`; `LooseSmokingStanding` → `SmokingStanding.kf`;
    /// `3rdPSpecialIdleStimpak` → `3rdP_SpecialIdle_Stempak.kf`; `EatIdle.kf` in the
    /// meshes archive; IDLE models `3rdP_SpecialIdle_RepairingFlr01.kf` and
    /// `3rdP_SpecialIdle_Workbench01.kf`; which suits which is design], first that loads
    /// wins. Not `EatIdle.kf`: it bends the body forward about 90 degrees at the waist
    /// [observed 2026-11-06, a standing third-person player], a pose for eating off the
    /// ground, not a bite while standing; food takes the hand-to-mouth drinking idle
    /// [design].
    pub fn clips(self) -> &'static [&'static str] {
        match self {
            Style::Eat => &["idleanims\\drinkingstandinga.kf"],
            Style::Drink => &["idleanims\\drinkingstandinga.kf"],
            Style::Inject => &["idleanims\\3rdp_specialidle_stempak.kf"],
            Style::Smoke => &["idleanims\\smokingstanding.kf"],
            Style::Skin => &["idleanims\\3rdp_specialidle_repairingflr01.kf", "idleanims\\3rdp_specialidle_workbench01.kf"],
            Style::Craft | Style::Handle => &["idleanims\\3rdp_specialidle_workbench01.kf"],
        }
    }
}

/// ETYP Chems, Stimpak, Food, Alcohol [documented: xEdit wbEquipTypeEnum].
const ETYP_STIMPAK: i64 = 11;
const ETYP_ALCOHOL: i64 = 13;

/// The style of an aid item, by its equip type and its EditorID's words [design].
pub fn style_of(edid: &str, equip_type: i64) -> Style {
    let e = edid.to_ascii_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| e.contains(w));
    if has(&["cig", "smok", "jet", "pipe", "tobacco"]) {
        Style::Smoke
    } else if equip_type == ETYP_STIMPAK || has(&["stim", "medx", "psycho", "hydra", "slasher", "syringe", "bloodpack", "antivenom", "stealthboy"]) {
        Style::Inject
    } else if equip_type == ETYP_ALCOHOL
        || has(&["water", "nuka", "cola", "sarsaparilla", "beer", "whiskey", "scotch", "vodka", "wine", "milk", "juice", "moonshine", "tequila", "absinthe", "rum", "brew", "drink", "coffee", "sunset", "vim", "bottle"])
    {
        Style::Drink
    } else {
        Style::Eat
    }
}

/// The game's animated object (ANIO, by Editor ID) held while an aid item's third-person
/// game idle ([`Style::clips`]) plays, as the game holds it on actors playing that idle:
/// a beer bottle for alcohol, a Nuka-Cola bottle for other drinks, the stimpak for
/// injections, a cigarette for tobacco [observed: FalloutNV.esm ANIO `aoDrinkingStandingA`
/// `aoBeerBottle.NIF` → IDLE `BeerDrinkingStanding`, `aoDrinkingStandingB`
/// `aoDrinkingNukaBottle.NIF` → `SodaDrinkingStanding` (both `DrinkingStandingA.kf`),
/// `AnimStempax` `Stempax.NIF` → `3rdPSpecialIdleStimpak`, `aoLooseSmokingStanding`
/// `aoCigarette.NIF` → `LooseSmokingStanding`]. Food has no standing idle of its own in
/// the game: it takes the generic food of the seated eating idles (`aoChairEatingGenericFood`
/// `aoGenericFood.NIF` → `SitChairEatB`) [design], whatever its ETYP. Chems that are neither (Jet, pills)
/// hold nothing [design].
pub fn anim_object(edid: &str, equip_type: i64) -> Option<&'static str> {
    let e = edid.to_ascii_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| e.contains(w));
    match style_of(edid, equip_type) {
        Style::Drink if equip_type == ETYP_ALCOHOL || has(&["beer", "whiskey", "scotch", "vodka", "wine", "moonshine", "tequila", "absinthe", "rum", "brew"]) => Some("aoDrinkingStandingA"),
        Style::Drink => Some("aoDrinkingStandingB"),
        Style::Inject if equip_type == ETYP_STIMPAK || has(&["stim"]) => Some("AnimStempax"),
        Style::Smoke if has(&["cig", "smok", "tobacco", "pipe"]) => Some("aoLooseSmokingStanding"),
        Style::Eat if equip_type != ETYP_CHEMS => Some("aoChairEatingGenericFood"),
        _ => None,
    }
}

/// ETYP Chems [documented: xEdit wbEquipTypeEnum]. Food is told from chems this way, not
/// by ETYP Food (12): Cram is ETYP 0 [observed: FalloutNV.esm].
const ETYP_CHEMS: i64 = 10;

/// Parse `Config\B42Inject.ini` into `o` (keys `Hardcore`, `InjectInPA`).
pub fn read_ini(text: &str, o: &mut Options) {
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let on = v.trim().parse::<f32>().map(|v| v != 0.0).ok();
        match (k.trim().to_ascii_lowercase().as_str(), on) {
            ("hardcore", Some(on)) => o.hardcore = on,
            ("injectinpa", Some(on)) => o.in_power_armor = on,
            _ => {}
        }
    }
}

/// `Config\B42Interact\Settings.ini` [Interactions]: the form types turned off
/// (`<type>=Disabled`) [observed].
pub fn disabled_types(text: &str) -> Vec<u32> {
    let mut on = false;
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            on = line.eq_ignore_ascii_case("[interactions]");
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| on && !line.starts_with(';')) else { continue };
        if v.split(';').next().is_some_and(|v| v.trim().eq_ignore_ascii_case("disabled")) {
            out.extend(k.trim().parse::<u32>().ok());
        }
    }
    out
}

/// `Config\B42Interact\Blacklist.ini`: the EditorIDs left out (`<n>=<EditorID>`), lower case.
pub fn blacklist(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with(';') && !l.starts_with('['))
        .filter_map(|l| l.split_once('=').map(|(_, v)| v.split(';').next().unwrap_or("").trim().to_ascii_lowercase()))
        .filter(|v| !v.is_empty())
        .collect()
}

/// A KEYWORDS list (`keywords\*.ini`: `<EditorID>=<keyword>` lines under any section)
/// [observed: the lists Idle Variety ships]: (EditorID, keyword), lower case.
pub fn keywords(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with(';') && !l.starts_with('['))
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let v = v.split(';').next()?.trim();
            (!k.trim().is_empty() && !v.is_empty()).then(|| (k.trim().to_ascii_lowercase(), v.to_ascii_lowercase()))
        })
        .collect()
}

/// The category of an activator no keyword names, by the game's own names [design:
/// KEYWORDS' lists are not installed; the harvestable plants' EditorIDs end in
/// `Pickable`, switches and buttons are named so in the game's data]: `harvestable`,
/// `buttons`, else `default`.
pub fn activator_category(edid: &str, name: &str) -> &'static str {
    let e = edid.to_ascii_lowercase();
    let n = name.to_ascii_lowercase();
    if e.contains("pickable") {
        "harvestable"
    } else if ["button", "switch", "lever"].iter().any(|w| e.contains(w) || n.contains(w)) {
        "buttons"
    } else {
        "default"
    }
}

// ---- B42 Loot ------------------------------------------------------------------------

/// B42 Loot's clips: `meshes\b42loot\<0 third | 1 first>\loot*.kf` [observed: the mod's
/// `ln_B42Loot.txt`, B42GL].
pub const LOOT_ROOT: &str = "meshes\\b42loot\\";

/// Where an item sits in the left hand while its loot clip plays: the mod's `B42GrbL`
/// node under `Bip01 L Hand`, scaled, moved and turned (degrees) [observed: B42GJ,
/// `Config\B42Loot\Coord`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    pub scale: f32,
    pub at: [f32; 3],
    pub turn: [f32; 3],
}

impl Grip {
    /// A Coord line's value, `scale|x|y|z|rx|ry|rz`.
    pub fn parse(v: &str) -> Option<Grip> {
        let n: Vec<f32> = v.split('|').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
        (n.len() == 7).then(|| Grip { scale: n[0], at: [n[1], n[2], n[3]], turn: [n[4], n[5], n[6]] })
    }
}

/// B42 Loot as installed: its clips, and the grips by model and by type.
#[derive(Debug, Default, Clone)]
pub struct LootSet {
    /// Lower-case file names (`loot5.kf`) by view.
    first: HashMap<String, String>,
    third: HashMap<String, String>,
    by_model: HashMap<String, Grip>,
    by_type: HashMap<String, Grip>,
    /// `Config\B42Loot\Settings.ini` `Pull` (the item flies to the hand first).
    pub pull: bool,
}

/// `key=value` lines of an ini (sections and `;` comments skipped).
fn ini_pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|l| {
        let l = l.split(';').next()?.trim();
        if l.starts_with('[') {
            return None;
        }
        let (k, v) = l.split_once('=')?;
        Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
    })
}

impl LootSet {
    /// From the data's paths under [`LOOT_ROOT`] and the mod's Coord files (`obj` the
    /// texts of `Coord\Obj\*.ini`, `types` of `Coord\Types.ini`, `settings`).
    pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>, obj: &[String], types: Option<&str>, settings: Option<&str>) -> LootSet {
        let mut set = LootSet::default();
        for p in paths {
            let low = p.to_ascii_lowercase();
            let Some(rest) = low.strip_prefix(LOOT_ROOT) else { continue };
            let Some((view, file)) = rest.split_once('\\') else { continue };
            if !file.ends_with(".kf") || file.contains('\\') {
                continue;
            }
            match view {
                "1" => set.first.insert(file.to_string(), low.clone()),
                "0" => set.third.insert(file.to_string(), low.clone()),
                _ => None,
            };
        }
        for text in obj {
            for (k, v) in ini_pairs(text) {
                if let Some(g) = Grip::parse(&v) {
                    set.by_model.insert(k.trim_start_matches("meshes\\").to_string(), g);
                }
            }
        }
        for (k, v) in types.map(ini_pairs).into_iter().flatten() {
            if let Some(g) = Grip::parse(&v) {
                set.by_type.insert(k, g);
            }
        }
        set.pull = settings.is_some_and(|t| ini_pairs(t).any(|(k, v)| k == "pull" && v.trim() != "0"));
        set
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_empty() && self.third.is_empty()
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        (self.first.len() + self.third.len(), self.by_model.len(), self.by_type.len())
    }

    /// The clip for a pick-up [observed: B42GL]: `loot<anim type>.kf` when a weapon is
    /// drawn and the file is there, else `loot.kf`; in first person with nothing drawn,
    /// `lootholst.kf`.
    pub fn clip(&self, view: View, drawn: Option<i64>) -> Option<&str> {
        let set = if view == View::First { &self.first } else { &self.third };
        let own = drawn.and_then(|t| set.get(&format!("loot{t}.kf")));
        let plain = if drawn.is_none() && view == View::First { set.get("lootholst.kf").or_else(|| set.get("loot.kf")) } else { set.get("loot.kf") };
        own.or(plain).map(String::as_str)
    }

    /// The grip for an item: by its model (path under `meshes\`), else by its type code
    /// ([`loot_type`]) [observed: B42GH, B42GK].
    pub fn grip(&self, model: Option<&str>, code: Option<&str>) -> Option<Grip> {
        let m = model.map(|m| m.to_ascii_lowercase());
        let m = m.as_deref().map(|m| m.trim_start_matches("meshes\\"));
        m.and_then(|m| self.by_model.get(m)).or_else(|| code.and_then(|c| self.by_type.get(c))).copied()
    }
}

/// An item's type code in the mod's Types.ini [observed: B42GK, GetType]: armour 24
/// (upper body; 241 not, 242 power armour), book 25, misc 31, weapon 40 and its
/// animation type (4013 a spear), ammo 41, key 46, aid 47, note 49, weapon mod 103,
/// casino chip 108, Caravan card 115, currency 116.
pub fn loot_type(sig: &[u8; 4], weapon_anim: Option<i64>, upper_body: Option<bool>, power_armor: bool) -> Option<String> {
    Some(match sig {
        b"ARMO" => match (upper_body, power_armor) {
            (Some(false), _) => "241".into(),
            (_, true) => "242".into(),
            _ => "24".into(),
        },
        b"BOOK" => "25".into(),
        b"MISC" => "31".into(),
        b"WEAP" => format!("40{}", weapon_anim.unwrap_or(0)),
        b"AMMO" => "41".into(),
        b"KEYM" => "46".into(),
        b"ALCH" => "47".into(),
        b"NOTE" => "49".into(),
        b"IMOD" => "103".into(),
        b"CHIP" => "108".into(),
        b"CCRD" => "115".into(),
        b"CMNY" => "116".into(),
        _ => return None,
    })
}

// ---- B42 Melee Bash --------------------------------------------------------------------

/// B42 Melee Bash's clips: `<root><alt>\<1 first | 0 third>\<weapon type>\*.kf` and the
/// fail clips `<root><alt>\bashfail.kf` [observed: the mod's `B42BashS`]. Only the main set
/// (alt 0) is used.
pub const BASH_ROOT: &str = "meshes\\characters\\_male\\idleanims\\b42bash\\";

/// B42 Melee Bash as installed: its swing clips by view and weapon type.
#[derive(Debug, Default, Clone)]
pub struct BashClips {
    first: HashMap<i64, Vec<String>>,
    third: HashMap<i64, Vec<String>>,
    /// The first-person fail clip (no Action Points).
    pub fail: Option<String>,
}

impl BashClips {
    pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>) -> BashClips {
        let mut set = BashClips::default();
        for p in paths {
            let low = p.to_ascii_lowercase();
            let Some(rest) = low.strip_prefix(BASH_ROOT).and_then(|r| r.strip_prefix("0\\")) else { continue };
            if !rest.ends_with(".kf") {
                continue;
            }
            let parts: Vec<&str> = rest.split('\\').collect();
            match parts.as_slice() {
                ["bashfail.kf"] => set.fail = Some(low.clone()),
                [view, ty, _file] => {
                    let Ok(ty) = ty.parse::<i64>() else { continue };
                    let into = match *view {
                        "1" => &mut set.first,
                        "0" => &mut set.third,
                        _ => continue,
                    };
                    into.entry(ty).or_default().push(low.clone());
                }
                _ => {}
            }
        }
        for v in set.first.values_mut().chain(set.third.values_mut()) {
            v.sort();
        }
        set
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_empty() && self.third.is_empty()
    }

    /// The swing clips for a view and clip type (`fnv_combat::bash::clip_type`).
    pub fn clips(&self, view: View, clip_type: i64) -> &[String] {
        let set = if view == View::First { &self.first } else { &self.third };
        set.get(&clip_type).map_or(&[], Vec::as_slice)
    }

    /// The clip type of a clip of the set (its folder's number).
    pub fn type_of(path: &str) -> Option<i64> {
        let low = path.to_ascii_lowercase();
        let rest = low.strip_prefix(BASH_ROOT)?.strip_prefix("0\\")?;
        let mut parts = rest.split('\\');
        parts.next()?;
        parts.next()?.parse().ok()
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.first.values().map(Vec::len).sum(), self.third.values().map(Vec::len).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index {
        let paths = [
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\stimpak\\stimpak.kf",
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\0\\stimpak\\stimpak.kf",
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\brahminsteak\\genericmeatcooked.kf",
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\brahminsteak\\genericmeatcooked2.kf",
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\stimpak\\readme.txt",
            "meshes\\characters\\_male\\idleanims\\b42inject\\0\\0 = 3rd person.txt",
            "meshes\\b42interact\\edid\\brahminmeat\\add\\dflt\\brahmin\\1st\\skinningmeat.kf",
            "meshes\\b42interact\\edid\\brahminmeat\\add\\dflt\\brahmin\\cnd.ini",
            "meshes\\b42interact\\edid\\weap10mmpistol\\add\\once\\1st\\10mmpistol.kf",
            "meshes\\b42interact\\edid\\weap10mmpistol\\add\\once\\cnd.ini",
            "meshes\\b42interact\\awr\\edid\\recipe10mm\\1st\\craftingammo10mm.kf",
            "meshes\\textures\\other.kf",
        ];
        Index::build(paths, |p| {
            Some(if p.contains("brahmin") {
                "[0]\n0=ref rSrc = Goo1.AuxVarGetRef %q*_SRC%q%rint iGo = rSrc.IsActor && rSrc.GetIsCreature == 1 && rSrc.GetCreatureType != 4 && rSrc.GetDead && rSrc.GetDistance3D Player < 500".into()
            } else {
                "[0]\n0=ref rSrc = Goo1.AuxVarGetRef %q_SRC%q%rint iGo = Player.IsInCombat == 0%rGoo1.AuxVarSetFlt %q*_OK%q iGo".into()
            })
        })
    }

    #[test]
    fn the_consume_folders_are_found_by_view_and_armour() {
        let ix = index();
        assert_eq!(ix.inject("Stimpak", false, View::First), ["meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\stimpak\\stimpak.kf"]);
        assert_eq!(ix.inject("BrahminSteak", false, View::First).len(), 2);
        assert!(ix.inject("BrahminSteak", false, View::Third).is_empty());
        assert!(ix.inject("Stimpak", true, View::First).is_empty());
        assert_eq!(ix.counts(), (4, 2, 1));
        assert_eq!(ix.recipe("Recipe10mm").len(), 1);
    }

    #[test]
    fn a_puppet_gets_the_third_person_clip_of_the_same_name() {
        let ix = index();
        let first = &ix.inject("Stimpak", false, View::First)[0];
        assert_eq!(ix.third_for("Stimpak", false, first, 0.9), Some("meshes\\characters\\_male\\idleanims\\b42inject\\0\\0\\stimpak\\stimpak.kf"));
        // Power armour without its own: the plain one.
        assert!(ix.third_for("Stimpak", true, first, 0.0).is_some());
        assert_eq!(ix.third_for("BrahminSteak", false, "x.kf", 0.5), None);
    }

    #[test]
    fn a_roll_picks_each_clip() {
        let c = vec!["a".to_string(), "b".to_string()];
        assert_eq!(pick(&c, 0.0).unwrap(), "a");
        assert_eq!(pick(&c, 0.49).unwrap(), "a");
        assert_eq!(pick(&c, 0.5).unwrap(), "b");
        assert_eq!(pick(&c, 1.0).unwrap(), "b");
        assert_eq!(pick(&[], 0.3), None);
    }

    #[test]
    fn interact_entries_follow_their_conditions() {
        let ix = index();
        let body = Source { creature: Some(("brahmin".into(), 0, 120.0)), used: false };
        assert!(ix.interact("BrahminMeat", &body, true, false).is_some());
        assert!(ix.interact("BrahminMeat", &Source { used: true, ..body.clone() }, false, false).is_none());
        assert!(ix.interact("BrahminMeat", &Source { creature: Some(("dog".into(), 0, 120.0)), used: false }, false, false).is_none());
        assert!(ix.interact("BrahminMeat", &Source { creature: Some(("brahmin".into(), 4, 120.0)), used: false }, false, false).is_none());
        assert!(ix.interact("BrahminMeat", &Source { creature: Some(("brahmin".into(), 0, 900.0)), used: false }, false, false).is_none());
        assert!(ix.interact("BrahminMeat", &Source::default(), false, false).is_none());
        // A weapon: the first time only, not in a fight.
        assert!(ix.interact("Weap10mmPistol", &Source::default(), false, true).is_some());
        assert!(ix.interact("Weap10mmPistol", &Source::default(), false, false).is_none());
        assert!(ix.interact("Weap10mmPistol", &Source::default(), true, true).is_none());
    }

    #[test]
    fn cnd_files_are_read() {
        let c = parse_cnd("0=... rSrc.GetCreatureType == 6 && rSrc.GetDead && rSrc.GetDistance3D Player < 500 && ...");
        assert_eq!(c, Cond { not_in_combat: false, corpse: Some(Corpse { within: 500.0, creature_type: Some((true, 6)) }) });
        assert_eq!(parse_cnd("int iGo = Player.IsInCombat == 0"), Cond { not_in_combat: true, corpse: None });
    }

    #[test]
    fn the_packs_text_keys_are_read() {
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetStr \"*_InjMP\" \"B42Inject\\GenericMeatUncooked.nif\""), [Cue::Model("B42Inject\\GenericMeatUncooked.nif".into())]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_InjM\" 1"), [Cue::Attach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_InjM\" 3%rprint \"=== BYE ===\""), [Cue::Detach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_InJS\" 1"), [Cue::ItemSound]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetStr \"*_InJSP\" \"fx\\Hit\\Draw.wav\""), [Cue::SoundFile("fx\\Hit\\Draw.wav".into())]);
        assert_eq!(cues("SoundPath:fx\\Hit\\CigarLight.wav"), [Cue::SoundFile("fx\\Hit\\CigarLight.wav".into())]);
        assert_eq!(cues("Sound: B42InjectSound"), [Cue::ItemSound]);
        assert_eq!(cues("Sound: NPCHumanEatingFoodChewy"), []);
        assert_eq!(cues("ScriptLine: DispatchEventAlt \"B42IP\" \"B42Inject\\Stimpak01.nif\""), [Cue::Model("B42Inject\\Stimpak01.nif".into())]);
        assert_eq!(cues("ScriptLine: DispatchEventAlt \"B42IS\""), [Cue::ItemSound]);
        assert_eq!(cues("ScriptLine: DispatchEventAlt \"B42ISP\" \"fx\\Hit\\StimDraw.wav\""), [Cue::SoundFile("fx\\Hit\\StimDraw.wav".into())]);
        assert_eq!(cues("scriptLine: B42InjectQ.iRunDetach = 1"), [Cue::Detach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetStr \"*_IntSP\" \"fx\\Hit\\SkinningCut1.wav\""), [Cue::SoundFile("fx\\Hit\\SkinningCut1.wav".into())]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetStr \"*_IntMP\" \"HitMesh\\SkinningMeat.nif\""), [Cue::Model("HitMesh\\SkinningMeat.nif".into())]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_IntM\" 1"), [Cue::Attach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_IntM\" 3"), [Cue::Detach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_ACT\" 1"), [Cue::Activate]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*_IntD\" -1"), [Cue::Controls(-1)]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetFlt \"*AWR_Attach\" 1"), [Cue::Attach]);
        assert_eq!(cues("ScriptLine: Goo1.AuxVarSetStr \"*AWR_Model\" \"HitMesh\\CraftingAmmo45.nif\""), [Cue::Model("HitMesh\\CraftingAmmo45.nif".into())]);
        assert_eq!(cues("SoundPath:fx\\Hit\\SoundPath:fx\\Hit\\CraftingBulletAway.wav"), [Cue::SoundFile("fx\\Hit\\CraftingBulletAway.wav".into())]);
        assert_eq!(cues("start"), []);
        assert_eq!(cues("BlendIn:1"), []);
    }

    #[test]
    fn the_rules_choose_as_the_mod() {
        let o = Options::default();
        let calm = Situation::default();
        assert_eq!(decide(&calm, &o), Decision::Play);
        assert_eq!(decide(&Situation { in_menu: true, ..calm }, &o), Decision::Wait);
        assert_eq!(decide(&Situation { in_menu: true, queued: true, ..calm }, &o), Decision::Skip("busy"));
        assert_eq!(decide(&Situation { swimming: true, ..calm }, &o), Decision::Skip("swimming"));
        assert_eq!(decide(&Situation { reloading: true, ..calm }, &o), Decision::Skip("busy"));
        assert_eq!(decide(&Situation { power_armor: true, ..calm }, &o), Decision::Play);
        assert_eq!(decide(&Situation { power_armor: true, ..calm }, &Options { in_power_armor: false, ..o }), Decision::Skip("in power armour"));
        let hard = Options { hardcore: true, ..o };
        assert_eq!(decide(&Situation { attacking: true, ..calm }, &hard), Decision::Refuse);
        assert_eq!(decide(&Situation { ragdolled: true, swimming: true, ..calm }, &hard), Decision::Refuse);
        // Swimming alone is not refused, only not animated.
        assert_eq!(decide(&Situation { swimming: true, ..calm }, &hard), Decision::Skip("swimming"));
        assert_eq!(decide(&calm, &Options { on: false, ..o }), Decision::Skip("consume animations are off"));
    }

    #[test]
    fn styles_by_type_and_name() {
        assert_eq!(style_of("Stimpak", 11), Style::Inject);
        assert_eq!(style_of("MedX", 10), Style::Inject);
        assert_eq!(style_of("NukaCola", 12), Style::Drink);
        assert_eq!(style_of("Scotch", 13), Style::Drink);
        assert_eq!(style_of("AidCigar", 10), Style::Smoke);
        assert_eq!(style_of("BrahminSteak", 12), Style::Eat);
        assert_eq!(style_of("Buffout", 10), Style::Eat);
        assert_eq!(style_of("SSBottleFull", 12), Style::Drink);
        assert_eq!(anim_object("NukaCola", 12), Some("aoDrinkingStandingB"));
        assert_eq!(anim_object("Beer", 13), Some("aoDrinkingStandingA"));
        assert_eq!(anim_object("Stimpak", 11), Some("AnimStempax"));
        assert_eq!(anim_object("AidCigarette", 10), Some("aoLooseSmokingStanding"));
        assert_eq!(anim_object("Cram", 12), Some("aoChairEatingGenericFood"));
        assert_eq!(anim_object("Cram", 0), Some("aoChairEatingGenericFood"));
        assert_eq!(anim_object("Jet", 10), None);
        assert_eq!(anim_object("Buffout", 10), None);
        assert!(!Style::Eat.clips().iter().any(|c| c.contains("eatidle")), "EatIdle bends the body over");
        for s in [Style::Eat, Style::Drink, Style::Inject, Style::Smoke, Style::Skin, Style::Craft, Style::Handle] {
            assert_eq!(Style::from_code(s.code()), s);
            assert!(!s.clips().is_empty());
        }
    }

    #[test]
    fn the_ini_sets_the_options() {
        let mut o = Options::default();
        read_ini("[Settings]\n;hardcore will …\nHardcore=1\n;Enable\nInjectInPA=0\n", &mut o);
        assert_eq!(o, Options { on: true, hardcore: true, in_power_armor: false });
    }

    #[test]
    fn type_clips_follow_the_mods_folders() {
        let root = TYPE_ROOT;
        let paths: Vec<String> = [
            r"28\default\base\1st\holst\holst.kf",
            r"28\default\base\1st\1hp\a.kf",
            r"28\default\base\1st\base.kf",
            r"28\default\base\3rd\third.kf",
            r"28\default\key\1st\key.kf",
            r"27\default\base\1st\holst\loot.kf",
            r"21\harvestable\base\1st\holst\pick.kf",
            r"43\lootable\base\1st\holst\loot.kf",
        ]
        .iter()
        .map(|p| format!("{root}{p}"))
        .collect();
        let ix = Index::build(paths.iter().map(String::as_str), |_| None);
        let f = |v: &[String]| v.iter().map(|p| p.rsplit('\\').next().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(f(ix.type_clips(DOOR, "Default", LockSeen::Open, View::First, "holst")), ["holst.kf"]);
        assert_eq!(f(ix.type_clips(DOOR, "default", LockSeen::Open, View::First, "1hp")), ["a.kf"]);
        // A stance with no folder: the view folder's own.
        assert_eq!(f(ix.type_clips(DOOR, "default", LockSeen::Open, View::First, "2hr")), ["base.kf"]);
        assert_eq!(f(ix.type_clips(DOOR, "default", LockSeen::Key, View::First, "holst")), ["key.kf"]);
        // No lock folder: the base one; beyond the skill: none.
        assert_eq!(f(ix.type_clips(DOOR, "default", LockSeen::Pickable, View::First, "holst")), ["holst.kf"]);
        assert!(ix.type_clips(DOOR, "default", LockSeen::Beyond, View::First, "holst").is_empty());
        assert_eq!(f(ix.type_clips(DOOR, "default", LockSeen::Open, View::Third, "holst")), ["third.kf"]);
        // No third-person folder: none.
        assert!(ix.type_clips(CONTAINER, "default", LockSeen::Open, View::Third, "holst").is_empty());
        assert!(ix.has_category(ACTIVATOR, "Harvestable") && !ix.has_category(ACTIVATOR, "buttons"));
        assert_eq!(f(ix.type_third(&paths[0])), ["third.kf"]);
        assert_eq!(ix.type_counts(), (vec![ACTIVATOR, CONTAINER, DOOR, CREATURE], 8));
        assert_eq!(stance(None), "holst");
        assert_eq!(stance(Some(0)), "h2h1hm");
        assert_eq!(stance(Some(3)), "1hp");
        assert_eq!(stance(Some(5)), "2hr");
        assert_eq!(activator_category("BananaYuccaPickable", "Banana Yucca"), "harvestable");
        assert_eq!(activator_category("vSwitch", "Automatic Door Override"), "buttons");
        assert_eq!(activator_category("NVRadio", "Radio"), "default");
    }

    #[test]
    fn b42_bash_clips_by_view_and_type() {
        let paths = [
            r"meshes\characters\_male\idleanims\B42Bash\0\1\5\SpecialIdle_1st2hrBash.kf",
            r"meshes\characters\_male\idleanims\B42Bash\0\1\5\SpecialIdle_1st2hrBash2.kf",
            r"meshes\characters\_male\idleanims\B42Bash\0\0\5\SpecialIdle_3rd2hrBash.kf",
            r"meshes\characters\_male\idleanims\B42Bash\0\BashFail.kf",
            r"meshes\characters\_male\idleanims\B42Bash\1\1\5\SpecialIdle_1st2hrBash.kf",
            r"meshes\characters\_male\idleanims\B42Bash\HitReaction\HitA.kf",
        ];
        let set = BashClips::build(paths);
        assert_eq!(set.counts(), (2, 1));
        assert_eq!(set.clips(View::First, 5).len(), 2);
        assert_eq!(set.clips(View::Third, 5), [r"meshes\characters\_male\idleanims\b42bash\0\0\5\specialidle_3rd2hrbash.kf"]);
        assert!(set.clips(View::First, 3).is_empty());
        assert_eq!(set.fail.as_deref(), Some(r"meshes\characters\_male\idleanims\b42bash\0\bashfail.kf"));
        assert_eq!(BashClips::type_of(&set.clips(View::First, 5)[0]), Some(5));
        assert_eq!(cues("Sound: B42BashHitFrameDetect"), [Cue::Strike]);
        assert!(cues("Sound: HitFrameDetect").is_empty());
    }

    #[test]
    fn b42_loot_picks_its_clip_and_grip() {
        let paths = [r"meshes\B42Loot\1\Loot.kf", r"meshes\B42Loot\1\Loot5.kf", r"meshes\B42Loot\1\LootHolst.kf", r"meshes\B42Loot\0\Loot.kf"];
        let obj = vec!["[0]\r\nclutter\\food\\apple02.nif=0.9|11|5|-1|-164|-4|-180\r\n".to_string()];
        let types = "[0]\r\n; Misc\r\n31=0.7|11|5|-2|-85|-45|-51\r\n; 2hr\r\n405=1.0|9|6|-3|174|63|-3\r\n";
        let set = LootSet::build(paths, &obj, Some(types), Some("[0]\r\nPull=1"));
        assert!(set.pull);
        assert_eq!(set.clip(View::First, None), Some(r"meshes\b42loot\1\lootholst.kf"));
        assert_eq!(set.clip(View::First, Some(5)), Some(r"meshes\b42loot\1\loot5.kf"));
        assert_eq!(set.clip(View::First, Some(3)), Some(r"meshes\b42loot\1\loot.kf"));
        assert_eq!(set.clip(View::Third, None), Some(r"meshes\b42loot\0\loot.kf"));
        let apple = set.grip(Some(r"meshes\Clutter\Food\Apple02.nif"), Some("47")).unwrap();
        assert_eq!(apple, Grip { scale: 0.9, at: [11.0, 5.0, -1.0], turn: [-164.0, -4.0, -180.0] });
        assert_eq!(set.grip(Some(r"meshes\x.nif"), loot_type(b"WEAP", Some(5), None, false).as_deref()).unwrap().scale, 1.0);
        assert_eq!(set.grip(None, loot_type(b"MISC", None, None, false).as_deref()).unwrap().scale, 0.7);
        assert!(set.grip(None, loot_type(b"STAT", None, None, false).as_deref()).is_none());
        assert_eq!(cues("ScriptLine: DispatchEventAlt \"B42GA\""), [Cue::Attach]);
        assert_eq!(cues("ScriptLine: DispatchEventAlt \"B42GD\""), [Cue::Detach]);
    }

    #[test]
    fn the_interact_inis_are_read() {
        let settings = "[0]\nClearOnce=0\n\n[Interactions]\n; ACTIVATORS\n21=Enabled\n23=Disabled\n27=disabled ; off\n";
        assert_eq!(disabled_types(settings), [23, 27]);
        assert_eq!(blacklist("[0]\n; note\n0=BlackCaveDoor\n1=CaveDoor\n"), ["blackcavedoor", "cavedoor"]);
        assert_eq!(keywords("[0]\n;Arrogant\n188Alexander=IV_Arrogant\n"), [("188alexander".to_string(), "iv_arrogant".to_string())]);
    }
}
