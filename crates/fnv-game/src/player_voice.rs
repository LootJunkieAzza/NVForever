//! Player voices (docs/design/player-voices.md): a character may speak with the voice of a
//! companion or of a voiced follower mod, saying that voice's lines for what happens to it.
//!
//! - **A voice set** ([`VoiceSet`], JSON `nvforever.voice_set/1`) names the actor whose
//!   voice type the files are in (`speaker`), the speed the voice plays at (`rate`) and
//!   its lines, each tagged with the event it answers (`event`: a Combat-tab topic's
//!   Editor ID such as `Hit` or `HealthQuarter`, or a later phase's tag). Sets hold
//!   FormIDs and tags only, no text or audio; they live in the MO2 mod folder
//!   `NVForever Player Voices` (`nvforever\voices\<id>.json`), never in the repository.
//!   FormIDs are written as `<plugin>|<id within the plugin>` so they do not depend on
//!   the load order.
//! - **Choosing** ([`PlayerState::voice`](crate::game::PlayerState::voice), saved with the
//!   player): the set's id, picked in the character creator. The engine resolves the set
//!   against the load order ([`PlayerVoice::resolve`]) into [`Game::player_voice`].
//! - **Saying** ([`Game::player_quip`], from `crate::combat_lines`): when the player would
//!   say a Combat-tab line, a line of the set for that topic is said instead, picked at
//!   random among the set's lines not said lately ([`RECENT`]). Its conditions are not
//!   tested: a set keeps only lines that need nothing but their speaker's identity
//!   (`fnvtool voice set-draft`). Without a line for the topic, or with
//!   [`QuipSettings::first_person`] off, the player says its own line as before.
//! - **Heard in that voice** ([`Game::line_voice_base`]): a set's line said by the player
//!   is found in the set's speaker's voice folder, and lasts its file's length ÷ the set's
//!   rate ([`Game::line_rate`]); the engine plays it at that speed (pitch and speed move
//!   together).
//! - **Lines carried with their sound** ([`SetLine::file`], [`add_packed_lines`]): a voiced
//!   follower mod's lines are taken without its plugin, which would put the follower in
//!   the world. Such a line names its sound file (shipped beside the set, in the same mod
//!   folder) and its subtitle; when the data is built each becomes an INFO of its own,
//!   numbered from [`PACKED_BASE`], with [`GameData::voice_files`] naming its sound.

use crate::data::GameData;
use crate::game::Game;
use fnv_sim::FormId;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// The `format` of a voice set file.
pub const FORMAT: &str = "nvforever.voice_set/1";

/// Where the engine looks for voice sets in the data (loose files of a mod folder).
pub const SETS_DIR: &str = "nvforever\\voices\\";

/// How many of a voice's lines are kept from being said again soon: a line is not picked
/// while it is among the last `min(RECENT, half the event's lines)` said [design].
pub const RECENT: usize = 8;

/// A voice set file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceSet {
    pub format: String,
    /// `vanilla.cass`, `nexus.willow`: what a player's choice stores.
    pub id: String,
    /// The name the character creator shows (NVForever's own words).
    pub name: String,
    /// `female`, `male` or `other`: how the creator groups the voices.
    #[serde(default)]
    pub sex: String,
    /// `human` or `ghoul`: the characters the voice is offered to ([`VoiceSet::fits`]);
    /// empty is human.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub race: String,
    /// One line on the personality, NVForever's own words.
    #[serde(default)]
    pub blurb: String,
    /// The actor (NPC or creature base) whose voice type holds the files: `<plugin>|<id>`.
    /// Empty for a set whose lines all carry their sound ([`SetLine::file`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub speaker: String,
    /// Playback speed, which moves the pitch with it: a vanilla voice plays a little off
    /// 1.0 so it is not taken for the companion [owner 2026-11-06].
    #[serde(default = "one")]
    pub rate: f32,
    /// The companion the voice belongs to, when it is one (`<plugin>|<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion: Option<String>,
    pub lines: Vec<SetLine>,
}

/// What a race is for the voices: `ghoul` for the ghoul races (the editor id has "ghoul"
/// in it: Ghoul, made playable by a ghoul race mod (Fully Functional Ghoul Race); a follower mod's
/// WillowsGhoulRace), otherwise `human` [design].
pub fn race_kind(race_edid: &str) -> &'static str {
    if race_edid.to_ascii_lowercase().contains("ghoul") {
        "ghoul"
    } else {
        "human"
    }
}

impl VoiceSet {
    /// `human` or `ghoul` (an empty tag is human).
    pub fn race_kind(&self) -> &str {
        if self.race.is_empty() {
            "human"
        } else {
            &self.race
        }
    }

    /// The voice is offered to a character of this race kind ([`race_kind`]) and sex: a
    /// ghoul gets only ghoul voices, a human only human ones, each of its own sex [owner
    /// 2026-11-06]. A voice of sex `other` fits nobody.
    pub fn fits(&self, race_kind: &str, female: bool) -> bool {
        voice_fits(&self.sex, self.race_kind(), race_kind, female)
    }
}

/// [`VoiceSet::fits`] on the tags alone (the creator's list has no sets).
pub fn voice_fits(sex: &str, race: &str, race_kind: &str, female: bool) -> bool {
    let race = if race.is_empty() { "human" } else { race };
    sex == if female { "female" } else { "male" } && race == race_kind
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// One line of a set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLine {
    /// The INFO: `<plugin>|<id>`.
    pub info: String,
    /// What it answers: a Combat-tab topic's Editor ID (`Hit`, `Attack`, …), or a tag of a
    /// later phase (`react.grenade`, `place`, `quest`).
    pub event: String,
    /// Generic enough to be heard by the speaker itself (First Person Quips).
    #[serde(default)]
    pub first_person: bool,
    /// Off: kept in the file for review only.
    #[serde(default = "yes", rename = "use", skip_serializing_if = "is_true")]
    pub use_line: bool,
    /// Why the draft tagged it so.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The line's sound, a path in the data (`sound\voice\<plugin>\<voice type>\<file>`),
    /// for a line taken without its plugin; `info` then only says where it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Its subtitle, with [`SetLine::file`] (the mod author's words, credited in the mod
    /// folder's `mods-used.txt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A `place` line's places: references (`<plugin>|<id>`) near which it is a comment on
    /// where the speaker is. In the original, the trigger boxes whose scripts start the
    /// companion's line ([`PLACE_RADIUS`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub near: Vec<String>,
    /// A `place` line's cells, worldspaces or map regions (`<plugin>|<id>`): it is a
    /// comment on being in one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inside: Vec<String>,
}

/// Condition functions that only say who the speaker is or pick by chance, which a set's
/// comment drops when it is tested: the player says it in that voice [design].
pub const IDENTITY: &[&str] = &["GetIsID", "GetIsVoiceType", "GetIsSex", "GetIsRace", "GetIsReference", "GetPlayerTeammate", "GetIsCreature", "GetInFaction", "GetRandomPercent", "GetIsPlayableRace"];

fn only_identity(data: &GameData, c: &crate::cond::Condition) -> bool {
    let name = data.table.functions.get(&c.opcode()).map(|f| f.name.as_str()).unwrap_or("");
    IDENTITY.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Whether a condition names one of `forms` among its parameters.
fn names_any(c: &crate::cond::Condition, forms: &[FormId]) -> bool {
    use fnv_script::vm::{ArgValue, Value};
    c.params.iter().any(|p| matches!(p, ArgValue::Value(Value::Ref(r)) if forms.contains(&FormId(*r))))
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The first FormID of the INFOs made for lines carried with their sound: load-order
/// index 0xFE, which no plugin has (the game loads at most 255 files, and 0xFF is the
/// index of references made while playing) [design].
pub const PACKED_BASE: u32 = 0xFE00_0000;

/// The Player's base form: the speaker of a set whose lines all carry their sound.
const PLAYER_BASE: FormId = FormId(0x0000_0007);

/// Make an INFO for every line of `sets` that carries its sound ([`SetLine::file`]), in
/// set id order then line order, so every game with the same sets numbers them alike
/// (co-op). Each has one response with the line's text and the topic of its event when
/// one has that Editor ID. Returns how many were made.
pub fn add_packed_lines(data: &mut GameData, sets: &[VoiceSet]) -> usize {
    let mut order: Vec<&VoiceSet> = sets.iter().collect();
    order.sort_by(|a, b| a.id.cmp(&b.id));
    let mut next = PACKED_BASE + data.voice_files.len() as u32;
    let mut made = 0;
    for set in order {
        for (k, line) in set.lines.iter().enumerate() {
            let Some(file) = line.file.as_ref().filter(|f| !f.is_empty()) else { continue };
            if data.packed_lines.contains_key(&(set.id.clone(), k)) || next > PACKED_BASE + 0x00FF_FFFF {
                continue;
            }
            let id = FormId(next);
            next += 1;
            let text = line.text.clone().unwrap_or_default();
            let def = crate::data::InfoDef {
                topic: data.by_edid(&line.event).filter(|t| data.topic_defs.contains_key(t)),
                response_chars: vec![text.chars().count()],
                responses: vec![crate::data::Response { number: 1, text, ..Default::default() }],
                ..Default::default()
            };
            data.infos.insert(id, def);
            data.voice_files.insert(id, file.replace('/', "\\").to_ascii_lowercase());
            data.packed_lines.insert((set.id.clone(), k), id);
            made += 1;
        }
    }
    made
}

/// `<plugin>|<id>` written for a load-order FormID.
pub fn form_ref(data: &GameData, id: FormId) -> String {
    let index = (id.0 >> 24) as usize;
    let plugin = data.load_order.get(index).map_or("?", String::as_str);
    format!("{plugin}|{:06X}", id.0 & 0x00FF_FFFF)
}

/// A `<plugin>|<id>` reference in the load order: `None` when its plugin is not loaded or
/// the text is no such reference.
pub fn resolve_ref(data: &GameData, r: &str) -> Option<FormId> {
    let (plugin, id) = r.split_once('|')?;
    let id = u32::from_str_radix(id.trim().trim_start_matches("0x"), 16).ok()?;
    let index = data.load_order.iter().position(|p| p.eq_ignore_ascii_case(plugin.trim()))?;
    Some(FormId(((index as u32) << 24) | (id & 0x00FF_FFFF)))
}

/// A set resolved against the load order.
#[derive(Debug, Clone)]
pub struct PlayerVoice {
    pub id: String,
    /// The set's `place` lines: the references near which each is said, and the cells or
    /// worldspaces in which.
    pub near: HashMap<FormId, Vec<FormId>>,
    pub inside: HashMap<FormId, Vec<FormId>>,
    pub name: String,
    /// The base form whose voice type the lines are heard in.
    pub base: FormId,
    pub rate: f32,
    pub companion: Option<FormId>,
    /// The lines in use, by event.
    pub by_event: HashMap<String, Vec<FormId>>,
    /// Every line in use.
    pub lines: HashSet<FormId>,
    pub first_person: HashSet<FormId>,
    /// The last lines said, newest last.
    pub recent: VecDeque<FormId>,
}

impl PlayerVoice {
    /// A voice with no lines yet.
    pub fn new(id: &str, name: &str, base: FormId, rate: f32) -> PlayerVoice {
        PlayerVoice {
            id: id.into(),
            near: HashMap::new(),
            inside: HashMap::new(),
            name: name.into(),
            base,
            rate,
            companion: None,
            by_event: HashMap::new(),
            lines: HashSet::new(),
            first_person: HashSet::new(),
            recent: VecDeque::new(),
        }
    }

    /// The set against the load order: its speaker must be loaded; lines whose plugin is
    /// not loaded, or that the data has no INFO for, are left out (the count says how
    /// many).
    pub fn resolve(set: &VoiceSet, data: &GameData) -> Result<(PlayerVoice, usize), String> {
        if set.format != FORMAT {
            return Err(format!("voice set {}: format {:?}, expected {FORMAT}", set.id, set.format));
        }
        let base = if set.speaker.is_empty() {
            PLAYER_BASE
        } else {
            resolve_ref(data, &set.speaker).ok_or_else(|| format!("voice set {}: speaker {} is not in the load order", set.id, set.speaker))?
        };
        let rate = if set.rate.is_finite() && set.rate > 0.0 { set.rate.clamp(0.5, 2.0) } else { 1.0 };
        let mut v = PlayerVoice::new(&set.id, &set.name, base, rate);
        v.companion = set.companion.as_deref().and_then(|c| resolve_ref(data, c));
        let mut missing = 0;
        for (k, l) in set.lines.iter().enumerate().filter(|(_, l)| l.use_line) {
            let info = if l.file.is_some() { data.packed_lines.get(&(set.id.clone(), k)).copied() } else { resolve_ref(data, &l.info) };
            let Some(info) = info.filter(|i| data.infos.contains_key(i)) else {
                missing += 1;
                continue;
            };
            v.by_event.entry(l.event.clone()).or_default().push(info);
            v.lines.insert(info);
            let near: Vec<FormId> = l.near.iter().filter_map(|r| resolve_ref(data, r)).collect();
            if !near.is_empty() {
                v.near.insert(info, near);
            }
            let inside: Vec<FormId> = l.inside.iter().filter_map(|r| resolve_ref(data, r)).collect();
            if !inside.is_empty() {
                v.inside.insert(info, inside);
            }
            if l.first_person {
                v.first_person.insert(info);
            }
        }
        Ok((v, missing))
    }

    /// A line for `event` the speaker may say, avoiding the ones said lately; `roll` picks
    /// among them. `first_person`: only lines for the speaker's own ears.
    pub fn pick(&self, event: &str, first_person: bool, roll: u32) -> Option<FormId> {
        let all: Vec<FormId> = self.by_event.get(event)?.iter().copied().filter(|i| !first_person || self.first_person.contains(i)).collect();
        if all.is_empty() {
            return None;
        }
        let keep_out = RECENT.min(all.len() / 2);
        let recent: Vec<FormId> = self.recent.iter().rev().take(keep_out.max(1).min(self.recent.len())).copied().collect();
        let fresh: Vec<FormId> = all.iter().copied().filter(|i| keep_out == 0 || !recent.contains(i)).collect();
        let pool = if fresh.is_empty() { &all } else { &fresh };
        Some(pool[roll as usize % pool.len()])
    }

    /// `info` was said.
    pub fn said(&mut self, info: FormId) {
        self.recent.retain(|&i| i != info);
        self.recent.push_back(info);
        while self.recent.len() > RECENT {
            self.recent.pop_front();
        }
    }
}

/// The player's switches for quips (the Gameplay settings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuipSettings {
    /// First Person Quips: the player's own character says its voice's lines.
    pub first_person: bool,
    /// Other players hear this one (a co-op session with others in it): quips are picked
    /// among every line of the voice, the party-only ones too, and sent ([`QuipState::out`]).
    pub coop: bool,
}

impl Default for QuipSettings {
    fn default() -> Self {
        QuipSettings { first_person: true, coop: false }
    }
}

/// How near one of a `place` line's references the player must be for the line, and how
/// near a companion must be to share its voice's lines (game units) [design: about the
/// size of the trigger boxes the original starts its place comments from].
pub const PLACE_RADIUS: f32 = 1500.0;
/// At most one place or quest comment per player this often (game seconds) [owner
/// 2026-11-05: chattiness Normal; design: the value].
pub const COMMENT_EVERY: f64 = 150.0;
/// The same comment is not said again for this long (game seconds) [design].
pub const COMMENT_AGAIN: f64 = 20.0 * 60.0;
/// How often the player's places are looked at (game seconds) [design].
pub const PLACE_LOOK_EVERY: f64 = 1.0;
/// Another player this near is reacted to (game units) [design: within a shout].
pub const REACT_RANGE: f32 = 1500.0;
/// At most one reaction this often (game seconds) [design].
pub const REACT_EVERY: f64 = 20.0;
/// How long another player must hold its sights on this one before it is told off
/// (seconds) [design].
pub const AIMED_AT_SECONDS: f64 = 1.0;
/// A quip waits this long after another player's quip has ended [owner plan 2026-11-05:
/// the group cooldown].
pub const GROUP_COOLDOWN: f64 = 2.0;

/// What another player does near this one, for a `react.*` line (the companions'
/// PLAYER* topics turned round: one player's word on another's deed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deed {
    /// Fired a gun outside a fight (PLAYERFIREWEAPON).
    FireWeapon,
    /// Threw a grenade (PLAYERTHROWGRENADE).
    ThrowGrenade,
    /// Laid a mine (PLAYERLAYMINE).
    LayMine,
    /// Held its sights on this player (PLAYERINIRONSITES).
    AimedAt,
}

impl Deed {
    /// The set's event for it.
    pub fn event(self) -> &'static str {
        match self {
            Deed::FireWeapon => "react.fireweapon",
            Deed::ThrowGrenade => "react.throwgrenade",
            Deed::LayMine => "react.laymine",
            Deed::AimedAt => "react.inironsites",
        }
    }

    /// The deed of an attack with a weapon of animation type `anim` (WEAP DNAM, xEdit's
    /// weapon animation type enumeration [documented]: 3-9 guns and launchers, 10 grenade
    /// throw, 11 land mine, 12 mine drop): `None` for a melee or thrown weapon, or fists.
    pub fn of_attack(anim: Option<i64>) -> Option<Deed> {
        match anim? {
            3..=9 => Some(Deed::FireWeapon),
            10 => Some(Deed::ThrowGrenade),
            11 | 12 => Some(Deed::LayMine),
            _ => None,
        }
    }
}

/// The player's quips as they go (not saved).
#[derive(Debug, Clone, Default)]
pub struct QuipState {
    /// Lines the player's voice said or picked, for the other players: the engine sends
    /// them ([`QuipSettings::coop`]).
    pub out: Vec<FormId>,
    /// Game seconds until which no quip of this player starts: another player's quip is
    /// playing in hearing (the group cooldown, set by the engine).
    pub hold_until: f64,
    /// The `place` lines whose place the player was in at the last look.
    pub at: HashSet<FormId>,
    pub next_look: f64,
    /// When the last place or quest comment was said.
    pub last_comment: Option<f64>,
    /// Comment line -> when it was said.
    pub said_at: HashMap<FormId, f64>,
    /// Quests whose stage was set since the last look.
    pub stages: Vec<FormId>,
    /// When the last reaction was said.
    pub last_react: Option<f64>,
    /// Comments and reactions said, for the log: (event, line).
    pub said: Vec<(String, FormId)>,
}

impl Game<'_> {
    /// A line of the player's voice for the Combat-tab topic `topic`, marked said; `None`
    /// without a voice, a line for it, or First Person Quips (module notes).
    ///
    /// In co-op ([`QuipSettings::coop`]) the line is picked among all the voice's lines and
    /// sent to the others ([`QuipState::out`]); this player hears it only when it is a
    /// first-person line and First Person Quips is on (else `None`, and the player says
    /// its own line as without a voice). While another player's quip plays nearby none is
    /// picked (the group cooldown).
    pub fn player_quip(&mut self, topic: &str) -> Option<FormId> {
        let coop = self.quips.coop;
        if (!coop && !self.quips.first_person) || self.time < self.quip.hold_until {
            return None;
        }
        let roll = self.rng.next_u32();
        let v = self.player_voice.as_mut()?;
        let info = v.pick(topic, !coop, roll)?;
        v.said(info);
        let local = self.quips.first_person && v.first_person.contains(&info);
        if coop {
            self.quip.out.push(info);
        }
        local.then_some(info)
    }

    /// The real companion the player's voice belongs to, when it follows the player within
    /// [`PLACE_RADIUS`]: its reference.
    pub fn voice_companion_here(&self) -> Option<FormId> {
        let c = self.player_voice.as_ref()?.companion?;
        let me = crate::awareness::position(self, FormId::PLAYER_REF)?;
        self.teammates().into_iter().find(|&r| {
            crate::functions::base_of(self, r) == Some(c) && crate::awareness::position(self, r).is_some_and(|p| distance(p, me) < PLACE_RADIUS)
        })
    }

    /// The player would say a combat line of `topic` while its voice's companion is here:
    /// a coin flip gives it to the companion, who says its own line of the topic (at its
    /// own speed) while the player says nothing [owner 2026-11-05: split 50/50]. `Some`
    /// when the companion took it.
    pub(crate) fn companion_takes(&mut self, topic: FormId) -> Option<FormId> {
        let companion = self.voice_companion_here()?;
        if self.rng.next_u32() & 1 == 0 {
            return None;
        }
        let line = self.say(companion, None, topic, false)?;
        self.quip.said.push((format!("companion.{}", self.data.name(topic)), line));
        Some(line)
    }

    /// Another player did `deed` near this one: this player's voice answers it with a
    /// `react.*` line, at most every [`REACT_EVERY`] seconds, sent to the others (who hear
    /// this player's character say it) and heard here when it is a first-person line. A
    /// fired gun is answered out of a fight only (in a fight everyone shoots).
    pub fn player_reacts(&mut self, deed: Deed, in_combat: bool) -> Option<FormId> {
        if deed == Deed::FireWeapon && in_combat {
            return None;
        }
        if self.time < self.quip.hold_until || self.quip.last_react.is_some_and(|t| self.time - t < REACT_EVERY) {
            return None;
        }
        let roll = self.rng.next_u32();
        let v = self.player_voice.as_mut()?;
        let info = v.pick(deed.event(), false, roll)?;
        v.said(info);
        self.quip.last_react = Some(self.time);
        self.quip.said.push((deed.event().to_string(), info));
        self.speak_quip(info);
        Some(info)
    }

    /// `info`, a line of the player's voice, is said: sent to the others in co-op, and
    /// heard here when it is a first-person line and First Person Quips is on.
    fn speak_quip(&mut self, info: FormId) {
        if self.quips.coop {
            self.quip.out.push(info);
        }
        let local = self.quips.first_person && self.player_voice.as_ref().is_some_and(|v| v.first_person.contains(&info));
        if local {
            let topic = self.data.infos.get(&info).and_then(|d| d.topic).unwrap_or(FormId(0));
            self.say_line(FormId::PLAYER_REF, None, topic, info, crate::game::LineKind::Say);
        }
    }

    /// Where `r` is: its cell and position (materialised or as placed).
    fn place_of(&self, r: FormId) -> Option<(Option<FormId>, [f32; 3])> {
        match self.world.peek(r) {
            Some(s) => Some((s.cell, s.pos)),
            None => self.world.placed.get(r).map(|p| (p.cell, p.pos)),
        }
    }

    /// The `place` lines whose place the player is in now.
    fn voice_places_here(&self) -> HashSet<FormId> {
        let mut here = HashSet::new();
        let (Some(v), Some((cell, pos))) = (self.player_voice.as_ref(), self.place_of(FormId::PLAYER_REF)) else { return here };
        let world = cell.and_then(|c| self.world.placed.world_of(c));
        for (&info, refs) in &v.near {
            let near = refs.iter().any(|&r| {
                self.place_of(r).is_some_and(|(c, p)| {
                    let same = (c.is_some() && c == cell) || (world.is_some() && c.and_then(|c| self.world.placed.world_of(c)) == world);
                    same && distance(p, pos) < PLACE_RADIUS
                })
            });
            if near {
                here.insert(info);
            }
        }
        for (&info, places) in &v.inside {
            let in_region = |x: FormId| self.data.regions.contains_key(&x) && crate::functions::objects::player_in_region(self, x).unwrap_or(false);
            if places.iter().any(|&x| Some(x) == cell || Some(x) == world || in_region(x)) {
                here.insert(info);
            }
        }
        here
    }

    /// Place and quest comments (docs/design/player-voices.md §4), looked at every
    /// [`PLACE_LOOK_EVERY`] seconds: a `place` line whose place the player has just come
    /// into, or a `quest` line naming a quest whose stage was just set, whose conditions
    /// pass but for the ones that only say who the speaker is ([`IDENTITY`]). Out of a
    /// fight, at most one comment every [`COMMENT_EVERY`] seconds, each line at most every
    /// [`COMMENT_AGAIN`]. Returns the line said.
    pub fn player_comments(&mut self, in_combat: bool) -> Option<FormId> {
        if self.player_voice.is_none() || self.time < self.quip.next_look {
            return None;
        }
        self.quip.next_look = self.time + PLACE_LOOK_EVERY;
        let stages = std::mem::take(&mut self.quip.stages);
        let here = self.voice_places_here();
        let new_places: Vec<FormId> = here.iter().copied().filter(|i| !self.quip.at.contains(i)).collect();
        self.quip.at = here;
        let data = self.data;
        let quest_lines: Vec<FormId> = match (stages.is_empty(), self.player_voice.as_ref()) {
            (false, Some(v)) => v
                .by_event
                .get("quest")
                .into_iter()
                .flatten()
                .copied()
                .filter(|i| data.infos.get(i).is_some_and(|d| d.conditions.iter().any(|c| names_any(c, &stages))))
                .collect(),
            _ => Vec::new(),
        };
        if in_combat || self.time < self.quip.hold_until || self.quip.last_comment.is_some_and(|t| self.time - t < COMMENT_EVERY) {
            return None;
        }
        let mut candidates: Vec<(&'static str, FormId)> = new_places.into_iter().map(|i| ("place", i)).chain(quest_lines.into_iter().map(|i| ("quest", i))).collect();
        candidates.retain(|(_, i)| self.quip.said_at.get(i).is_none_or(|t| self.time - t >= COMMENT_AGAIN));
        // Alone, a comment for the others' ears has nobody to hear it.
        let (coop, own) = (self.quips.coop, self.quips.first_person);
        if let Some(v) = self.player_voice.as_ref() {
            candidates.retain(|(_, i)| coop || (own && v.first_person.contains(i)));
        }
        candidates.sort_by_key(|(_, i)| *i);
        let mut passing = Vec::new();
        for (event, info) in candidates {
            let conds: Vec<crate::cond::Condition> = data.infos.get(&info).map(|d| d.conditions.iter().filter(|c| !only_identity(data, c)).cloned().collect()).unwrap_or_default();
            if self.conditions_pass(&conds, Some(FormId::PLAYER_REF), None, "player voice comment") {
                passing.push((event, info));
            }
        }
        if passing.is_empty() {
            return None;
        }
        let (event, info) = passing[self.rng.next_u32() as usize % passing.len()];
        self.quip.last_comment = Some(self.time);
        self.quip.said_at.insert(info, self.time);
        self.quip.said.push((event.to_string(), info));
        if let Some(v) = self.player_voice.as_mut() {
            v.said(info);
        }
        self.speak_quip(info);
        Some(info)
    }

    /// Whether anyone fights the player now (an actor in combat with it as a target).
    pub fn player_fought(&self) -> bool {
        self.awareness.actors.values().any(|a| a.state.in_combat() && a.targets.contains_key(&FormId::PLAYER_REF))
    }

    /// A quest's stage was set: its comments are looked at next ([`Game::player_comments`]).
    pub(crate) fn note_stage_for_voice(&mut self, quest: FormId) {
        if self.player_voice.as_ref().is_some_and(|v| v.by_event.contains_key("quest")) && !self.quip.stages.contains(&quest) {
            self.quip.stages.push(quest);
        }
    }

    /// The base form whose voice `r` says `info` in: the player's voice set's speaker for
    /// a line of its set, else [`Game::voice_base_in`].
    pub fn line_voice_base(&self, world: &fnv_sim::World, base: FormId, r: FormId, info: FormId) -> FormId {
        match self.player_voice.as_ref() {
            Some(v) if r == FormId::PLAYER_REF && v.lines.contains(&info) => v.base,
            _ => self.voice_base_in(world, base, r),
        }
    }

    /// The voice type the player's own lines (not a voice set's) are said in, by the
    /// player's sex and race now (the creator's choice, SexChange, a race change):
    /// PlayerVoiceFemale for a woman, PlayerVoiceMale for a man, the generic ghoul
    /// voices for a ghoul (overnight item 21, 2026-11-07).
    /// - [observed: the Player record (0x7) lists VTCK PlayerVoiceMale whatever the sex
    ///   chosen; Voices1.bsa has every GenericPlayer line (hit, power attack, death) in
    ///   both PlayerVoiceMale and PlayerVoiceFemale, under the same INFO ids, so the female
    ///   set is meant for a female player; hypothesis: the original picks the folder by
    ///   the player's sex].
    /// - [observed: no RACE lists a player voice, and no ghoul voice type has GenericPlayer
    ///   files; MaleGenericGhoul and FemaleGenericGhoul have the Generic quest's Hit,
    ///   PowerAttack and Death lines; design: a ghoul player says those].
    ///
    /// `None` when the data has no such voice type (the record's own voice is used).
    pub fn player_own_voice_type(&self) -> Option<FormId> {
        let female = self.actor_female(FormId::PLAYER_REF);
        let edid = match (self.player_is_ghoul(), female) {
            (true, true) => "FemaleGenericGhoul",
            (true, false) => "MaleGenericGhoul",
            (false, true) => "PlayerVoiceFemale",
            (false, false) => "PlayerVoiceMale",
        };
        self.data.by_edid(edid).filter(|&v| self.data.sig(v) == Some(*b"VTYP"))
    }

    /// Whether the player's race now is a ghoul race ([`race_kind`]).
    pub fn player_is_ghoul(&self) -> bool {
        self.actor_race(FormId::PLAYER_REF).and_then(|r| self.data.edid_of.get(&r)).is_some_and(|e| race_kind(e) == "ghoul")
    }

    /// The voice type `r` says `info` in when it is not the one of `r`'s record: the
    /// player's own voice type ([`Game::player_own_voice_type`]) for a line the player
    /// says that is not one of the voice set's. `None` otherwise (the voice of
    /// [`Game::line_voice_base`]).
    pub fn line_voice_type(&self, r: FormId, info: FormId) -> Option<FormId> {
        if r != FormId::PLAYER_REF || self.player_voice.as_ref().is_some_and(|v| v.lines.contains(&info)) {
            return None;
        }
        self.player_own_voice_type()
    }

    /// The speed `r` says `info` at: the set's rate for a line of the player's voice, else 1.
    pub fn line_rate(&self, r: FormId, info: FormId) -> f32 {
        match self.player_voice.as_ref() {
            Some(v) if r == FormId::PLAYER_REF && v.lines.contains(&info) => v.rate,
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(lines: &[(u32, &str, bool)]) -> PlayerVoice {
        let mut v = PlayerVoice::new("t", "T", FormId(1), 1.0);
        for &(i, e, fp) in lines {
            v.by_event.entry(e.to_string()).or_default().push(FormId(i));
            v.lines.insert(FormId(i));
            if fp {
                v.first_person.insert(FormId(i));
            }
        }
        v
    }

    #[test]
    fn a_line_is_not_picked_again_while_it_is_recent() {
        let mut v = voice(&[(1, "Hit", true), (2, "Hit", true), (3, "Hit", true), (4, "Hit", true)]);
        let mut seen = Vec::new();
        for roll in 0..4 {
            let i = v.pick("Hit", true, roll).unwrap();
            assert!(!seen.ends_with(&[i]), "the same line twice in a row");
            v.said(i);
            seen.push(i);
        }
        // Two of four are kept out: the last two said are never picked next.
        for roll in 0..20 {
            let i = v.pick("Hit", true, roll).unwrap();
            assert!(!seen[seen.len() - 2..].contains(&i));
        }
    }

    #[test]
    fn first_person_takes_only_generic_lines_and_one_line_is_always_there() {
        let mut v = voice(&[(1, "Attack", false), (2, "Attack", true)]);
        for roll in 0..5 {
            assert_eq!(v.pick("Attack", true, roll), Some(FormId(2)));
        }
        v.said(FormId(2));
        // A single fitting line is still said after itself.
        assert_eq!(v.pick("Attack", true, 0), Some(FormId(2)));
        assert_eq!(v.pick("Death", true, 0), None);
    }

    #[test]
    fn a_voice_fits_its_own_race_and_sex_only() {
        assert_eq!(race_kind("Ghoul"), "ghoul");
        assert_eq!(race_kind("GhoulOld"), "ghoul");
        assert_eq!(race_kind("WillowsGhoulRace"), "ghoul");
        assert_eq!(race_kind("Caucasian"), "human");
        assert_eq!(race_kind("AfricanAmericanOld"), "human");
        assert!(voice_fits("male", "ghoul", "ghoul", false));
        assert!(!voice_fits("male", "ghoul", "human", false), "no ghoul voice for a human");
        assert!(!voice_fits("male", "ghoul", "ghoul", true), "no man's voice for a woman");
        assert!(voice_fits("female", "", "human", true), "untagged is human");
        assert!(!voice_fits("female", "", "ghoul", true));
        assert!(!voice_fits("other", "human", "human", false) && !voice_fits("other", "human", "human", true));
    }

    #[test]
    fn a_set_reads_and_writes_without_text() {
        let s = VoiceSet {
            format: FORMAT.into(),
            id: "vanilla.cass".into(),
            name: "Cass".into(),
            sex: "female".into(),
            race: String::new(),
            blurb: String::new(),
            speaker: "FalloutNV.esm|133FDD".into(),
            rate: 1.06,
            companion: Some("FalloutNV.esm|133FDD".into()),
            lines: vec![SetLine { info: "FalloutNV.esm|0ABCDE".into(), event: "Hit".into(), first_person: true, use_line: true, note: String::new(), file: None, text: None, near: vec![], inside: vec![] }],
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("\"use\""), "use is written only when off");
        assert_eq!(serde_json::from_str::<VoiceSet>(&j).unwrap(), s);
    }
}
