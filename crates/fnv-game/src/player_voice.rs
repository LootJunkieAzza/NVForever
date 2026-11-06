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
        PlayerVoice { id: id.into(), name: name.into(), base, rate, companion: None, by_event: HashMap::new(), lines: HashSet::new(), first_person: HashSet::new(), recent: VecDeque::new() }
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
}

impl Default for QuipSettings {
    fn default() -> Self {
        QuipSettings { first_person: true }
    }
}

impl Game<'_> {
    /// A line of the player's voice for the Combat-tab topic `topic`, marked said; `None`
    /// without a voice, a line for it, or First Person Quips (module notes).
    pub fn player_quip(&mut self, topic: &str) -> Option<FormId> {
        if !self.quips.first_person {
            return None;
        }
        let roll = self.rng.next_u32();
        let v = self.player_voice.as_mut()?;
        let info = v.pick(topic, true, roll)?;
        v.said(info);
        Some(info)
    }

    /// The base form whose voice `r` says `info` in: the player's voice set's speaker for
    /// a line of its set, else [`Game::voice_base_in`].
    pub fn line_voice_base(&self, world: &fnv_sim::World, base: FormId, r: FormId, info: FormId) -> FormId {
        match self.player_voice.as_ref() {
            Some(v) if r == FormId::PLAYER_REF && v.lines.contains(&info) => v.base,
            _ => self.voice_base_in(world, base, r),
        }
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
    fn a_set_reads_and_writes_without_text() {
        let s = VoiceSet {
            format: FORMAT.into(),
            id: "vanilla.cass".into(),
            name: "Cass".into(),
            sex: "female".into(),
            blurb: String::new(),
            speaker: "FalloutNV.esm|133FDD".into(),
            rate: 1.06,
            companion: Some("FalloutNV.esm|133FDD".into()),
            lines: vec![SetLine { info: "FalloutNV.esm|0ABCDE".into(), event: "Hit".into(), first_person: true, use_line: true, note: String::new(), file: None, text: None }],
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(!j.contains("\"use\""), "use is written only when off");
        assert_eq!(serde_json::from_str::<VoiceSet>(&j).unwrap(), s);
    }
}
