//! Music and placed sounds: music types (MUSC), media sets (MSET), media location
//! controllers (ALOC), acoustic spaces (ASPC), the audio markers and the sound markers
//! placed in cells, and the rule that picks what is heard at a place.
//! Facts and tags: docs/spec-notes/audio.md.

use crate::fields::{formid_of, global, num, winner, Fields};
use crate::index::WorldIndex;
use crate::placed::{placement, ref_flags};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;
use std::collections::BTreeMap;

/// MUSC: a file or a folder under `music\` and a level.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicType {
    pub form_id: u32,
    pub edid: Option<String>,
    /// `FNAM` as stored.
    pub file: Option<String>,
    /// `ANAM` "dB (positive = Loop)" [documented: xEdit]; 0 when absent.
    pub db: f32,
}

/// MSET `NAM1`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MediaSetKind {
    #[default]
    None,
    Battle,
    Location,
    Dungeon,
    Incidental,
}

/// The six tracks of a location set, in the record's order.
pub const LAYERS: [&str; 6] = ["day_outer", "day_middle", "day_inner", "night_outer", "night_middle", "night_inner"];

/// MSET. The six slots are the tracks of a location set (day outer, middle, inner; night
/// outer, middle, inner); a battle set uses the first (its loop) and a dungeon set the
/// first three (battle, explore, suspense) [documented: xEdit field names].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaSet {
    pub form_id: u32,
    pub edid: Option<String>,
    pub kind: MediaSetKind,
    /// `NAM2`–`NAM7` as stored, relative to `music\`.
    pub tracks: [Option<String>; 6],
    /// `NAM8`, `NAM9`, `NAM0`, `ANAM`, `BNAM`, `CNAM`: decibels.
    pub db: [f32; 6],
    /// `JNAM`–`ONAM`: boundary in percent (of the marker's radius, [hypothesis]).
    pub boundary_percent: [f32; 6],
    /// `PNAM` bits 0–5: which slots are enabled.
    pub enabled: u8,
    /// `DNAM`, `ENAM`, `FNAM`, `GNAM`; for a location set: minimum time on, looping
    /// crossfade overlap, layer crossfade time (seconds, [hypothesis for the unit]).
    pub times: [f32; 4],
    /// `HNAM`, `INAM`: SOUN.
    pub intro: Option<u32>,
    pub outro: Option<u32>,
}

impl MediaSet {
    /// The track of slot `k` if it is enabled and named.
    pub fn track(&self, k: usize) -> Option<&str> {
        (self.enabled & (1 << k) != 0).then(|| self.tracks[k].as_deref()).flatten()
    }
}

/// ALOC.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaController {
    pub form_id: u32,
    pub edid: Option<String>,
    /// `NAM5`, `NAM6` in hours: the stored value over 256 is the fraction of a day
    /// [documented: xEdit wbAlocTime].
    pub day_start_hour: f32,
    pub night_start_hour: f32,
    /// `NAM4`, `NAM7`.
    pub location_delay: f32,
    pub retrigger_delay: f32,
    /// `HNAM`, `ZNAM`, `XNAM`, `YNAM`, `LNAM`, `GNAM`: MSET lists in stored order.
    pub neutral: Vec<u32>,
    pub ally: Vec<u32>,
    pub friend: Vec<u32>,
    pub enemy: Vec<u32>,
    pub location: Vec<u32>,
    pub battle: Vec<u32>,
    /// `RNAM`: FACT.
    pub faction: Option<u32>,
}

impl MediaController {
    /// Day or night at `hour` (0–24): day from the day start up to the night start.
    pub fn is_day(&self, hour: f32) -> bool {
        let (d, n) = (self.day_start_hour, self.night_start_hour);
        if d <= n {
            hour >= d && hour < n
        } else {
            !(hour >= n && hour < d)
        }
    }

    /// The sets heard at the controller's places [design, audio.md §4]: with a
    /// conditional faction its neutral sets (the player's standing with the faction is
    /// taken as neutral), else its location sets; whichever of the two it has when the
    /// other is empty.
    pub fn sets(&self) -> &[u32] {
        match (self.faction.is_some() && !self.neutral.is_empty(), self.location.is_empty()) {
            (true, _) | (_, true) => &self.neutral,
            _ => &self.location,
        }
    }

    /// The sets heard when the conditional faction stands `relation` towards the player
    /// (0 neutral, 1 enemy, 2 ally, 3 friend, as GetFactionRelation [documented: GECK wiki
    /// Media Location Controller: "Enemy Sets: … when the player is an Enemy of the chosen
    /// Faction", and so on]). A controller without a faction, or a relation whose list is
    /// empty, plays what [`MediaController::sets`] gives [design: the "Faction Not Found"
    /// choice is in `NAM1`, which is not understood].
    pub fn sets_for(&self, relation: Option<i64>) -> &[u32] {
        let list = match (self.faction, relation) {
            (Some(_), Some(1)) => &self.enemy,
            (Some(_), Some(2)) => &self.ally,
            (Some(_), Some(3)) => &self.friend,
            _ => &self.neutral,
        };
        if self.faction.is_some() && !list.is_empty() {
            list
        } else {
            self.sets()
        }
    }
}

/// The tracks of a dungeon set as layers, quietest first: explore (`NAM3`), suspense
/// (`NAM4`) and battle (`NAM2`) [documented: xEdit field names]; the enable flags are a
/// location set's and are not read [observed: dungeon sets store values above 63 there].
pub fn dungeon_tracks(set: &MediaSet) -> [Option<&str>; 3] {
    [set.tracks[1].as_deref(), set.tracks[2].as_deref(), set.tracks[0].as_deref()]
}

/// ASPC.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcousticSpace {
    pub form_id: u32,
    pub edid: Option<String>,
    /// The first four `SNAM`: dawn (or the default loop), afternoon, dusk, night (SOUN).
    pub loops: [Option<u32>; 4],
    /// The fifth `SNAM` and `WNAM`.
    pub walla: Option<u32>,
    pub walla_trigger_count: u32,
    /// `RDAT`: REGN.
    pub region: Option<u32>,
    /// `ANAM`, by name.
    pub environment: Option<String>,
    /// `INAM`.
    pub interior: bool,
}

impl AcousticSpace {
    /// The loop of a time of day [design, the hours; hypothesis: a space with only its
    /// first sound plays that one all day ("Dawn / Default Loop")]: dawn 5–9, afternoon
    /// 9–17, dusk 17–21, night otherwise.
    pub fn loop_at(&self, hour: f32) -> Option<u32> {
        let k = match hour {
            h if (5.0..9.0).contains(&h) => 0,
            h if (9.0..17.0).contains(&h) => 1,
            h if (17.0..21.0).contains(&h) => 2,
            _ => 3,
        };
        self.loops[k].or(self.loops[0])
    }
}

/// The music a region's sound data names (REGN `RDMO`, `RDSI`, `RDSB` [documented: xEdit
/// "Music", "Incidental MediaSet", "Battle MediaSets"]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionMusic {
    pub form_id: u32,
    pub edid: Option<String>,
    /// MUSC.
    pub music: Option<u32>,
    /// MSET of the incidental kind.
    pub incidental: Option<u32>,
    /// MSET of the battle kind, in stored order.
    pub battle: Vec<u32>,
}

/// The music and acoustic-space forms of a load order.
#[derive(Debug, Default)]
pub struct AudioForms {
    pub music: BTreeMap<u32, MusicType>,
    pub sets: BTreeMap<u32, MediaSet>,
    pub controllers: BTreeMap<u32, MediaController>,
    pub spaces: BTreeMap<u32, AcousticSpace>,
    /// Regions naming any music, by FormID.
    pub regions: BTreeMap<u32, RegionMusic>,
    /// Regions' sounds (REGN `RDSD`), by region FormID.
    pub region_sounds: BTreeMap<u32, Vec<RegionSound>>,
    /// DOBJ DATA, the default objects by their place in the list (0: none) [documented:
    /// xEdit DOBJ]; the music among them: [`DEFAULT_DEATH_MUSIC`].
    pub default_objects: Vec<u32>,
}

/// The DOBJ list's Death Music and Level Up Music places [documented: xEdit DOBJ].
pub const DEFAULT_DEATH_MUSIC: usize = 11;
pub const DEFAULT_LEVEL_UP_MUSIC: usize = 13;

/// A sound a region plays now and then (REGN `RDSD` [documented: xEdit wbRegionSounds]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionSound {
    pub sound: u32,
    /// Weathers it plays in: 1 Pleasant, 2 Cloudy, 4 Rainy, 8 Snowy (the WTHR
    /// classification bits).
    pub weathers: u8,
    /// Chance in percent (stored × 10,000, wbScaledInt4) [documented: xEdit].
    pub chance: f32,
}

impl RegionSound {
    /// Whether it plays in a weather of this classification (WTHR DATA flags); one naming
    /// no weather plays in any [hypothesis].
    pub fn plays_in(&self, classification: u8) -> bool {
        self.weathers == 0 || self.weathers & classification != 0
    }
}

fn text(v: Option<Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn float(f: &Fields, sig: &[u8; 4]) -> f32 {
    f.get(sig).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32
}

/// Every subrecord with this signature as a load-order FormID; null ones as `None`.
fn formids(f: &Fields, sig: &[u8; 4], lo: &LoadOrder) -> Vec<Option<u32>> {
    f.subs.iter().filter(|s| &s.sig == sig).map(|s| f.decode(s).and_then(|v| global(lo, f.plugin, formid_of(&v)?))).collect()
}

impl AudioForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> AudioForms {
        let mut out = AudioForms::default();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let pi = d.plugin as usize;
            let r = lo.records[pi][d.ordinal as usize];
            if !matches!(&r.header.sig, b"MUSC" | b"MSET" | b"ALOC" | b"ASPC" | b"REGN" | b"DOBJ") || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let edid = f.string(b"EDID");
            match &r.header.sig {
                b"MUSC" => {
                    out.music.insert(g, MusicType { form_id: g, edid, file: text(f.get(b"FNAM")), db: float(&f, b"ANAM") });
                }
                b"MSET" => {
                    let kind = match f.get(b"NAM1").as_ref().and_then(Value::as_str) {
                        Some("Battle Set") => MediaSetKind::Battle,
                        Some("Location Set") => MediaSetKind::Location,
                        Some("Dungeon Set") => MediaSetKind::Dungeon,
                        Some("Incidental Set") => MediaSetKind::Incidental,
                        _ => MediaSetKind::None,
                    };
                    let tracks = [b"NAM2", b"NAM3", b"NAM4", b"NAM5", b"NAM6", b"NAM7"].map(|s| text(f.get(s)));
                    let db = [b"NAM8", b"NAM9", b"NAM0", b"ANAM", b"BNAM", b"CNAM"].map(|s| float(&f, s));
                    let boundary_percent = [b"JNAM", b"KNAM", b"LNAM", b"MNAM", b"NNAM", b"ONAM"].map(|s| float(&f, s));
                    let enabled = f.get(b"PNAM").and_then(|v| num(&v, "value")).map_or(0, |v| v as u8);
                    let times = [b"DNAM", b"ENAM", b"FNAM", b"GNAM"].map(|s| float(&f, s));
                    out.sets.insert(g, MediaSet { form_id: g, edid, kind, tracks, db, boundary_percent, enabled, times, intro: f.formid(b"HNAM", lo), outro: f.formid(b"INAM", lo) });
                }
                b"ALOC" => {
                    let hour = |s: &[u8; 4]| f.get(s).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32 / 256.0 * 24.0;
                    let list = |s: &[u8; 4]| formids(&f, s, lo).into_iter().flatten().collect::<Vec<u32>>();
                    out.controllers.insert(
                        g,
                        MediaController {
                            form_id: g,
                            edid,
                            day_start_hour: hour(b"NAM5"),
                            night_start_hour: hour(b"NAM6"),
                            location_delay: float(&f, b"NAM4"),
                            retrigger_delay: float(&f, b"NAM7"),
                            neutral: list(b"HNAM"),
                            ally: list(b"ZNAM"),
                            friend: list(b"XNAM"),
                            enemy: list(b"YNAM"),
                            location: list(b"LNAM"),
                            battle: list(b"GNAM"),
                            faction: f.formid(b"RNAM", lo),
                        },
                    );
                }
                b"REGN" => {
                    let music = f.formid(b"RDMO", lo);
                    let incidental = f.formid(b"RDSI", lo);
                    let battle: Vec<u32> = formids(&f, b"RDSB", lo).into_iter().flatten().collect();
                    if music.is_some() || incidental.is_some() || !battle.is_empty() {
                        out.regions.insert(g, RegionMusic { form_id: g, edid, music, incidental, battle });
                    }
                    let mut sounds = Vec::new();
                    for s in f.subs.iter().filter(|s| &s.sig == b"RDSD") {
                        for e in s.data.as_chunks::<12>().0 {
                            let word = |o: usize| u32::from_le_bytes([e[o], e[o + 1], e[o + 2], e[o + 3]]);
                            if let Some(sound) = global(lo, f.plugin, word(0)).filter(|&x| x != 0 && word(0) != 0) {
                                sounds.push(RegionSound { sound, weathers: (word(4) & 0xF) as u8, chance: word(8) as f32 / 10_000.0 });
                            }
                        }
                    }
                    if !sounds.is_empty() {
                        out.region_sounds.insert(g, sounds);
                    }
                }
                b"DOBJ" => {
                    let data = f.subs.iter().find(|s| &s.sig == b"DATA").map(|s| s.data.as_slice()).unwrap_or_default();
                    out.default_objects = data.as_chunks::<4>().0.iter().map(|c| global(lo, f.plugin, u32::from_le_bytes(*c)).filter(|&x| x != 0).unwrap_or(0)).collect();
                }
                _ => {
                    let s = formids(&f, b"SNAM", lo);
                    let at = |k: usize| s.get(k).copied().flatten();
                    out.spaces.insert(
                        g,
                        AcousticSpace {
                            form_id: g,
                            edid,
                            loops: [at(0), at(1), at(2), at(3)],
                            walla: at(4),
                            walla_trigger_count: f.get(b"WNAM").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            region: f.formid(b"RDAT", lo),
                            environment: text(f.get(b"ANAM")),
                            interior: f.get(b"INAM").and_then(|v| v.as_u64()).unwrap_or(0) != 0,
                        },
                    );
                }
            }
        }
        out
    }
}

/// A reference with audio marker data (REFR `MMRK`): a place with a radius whose media
/// location controller chooses the music around it.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMarker {
    pub ref_id: u32,
    pub cell: u32,
    pub position: [f32; 3],
    /// `XRDS`; 0 when absent.
    pub radius: f32,
    /// `FULL` "Audio Marker Location Name".
    pub name: Option<String>,
    /// `CNAM`: ALOC.
    pub controller: Option<u32>,
    /// `BNAM` bit 0 "Use Controller Values".
    pub use_controller_values: bool,
    /// `MNAM`, `NNAM`: "Layer 2 Trigger %", "Layer 3 Trigger %", stored as fractions.
    pub layer2_percent: f32,
    pub layer3_percent: f32,
    pub initially_disabled: bool,
}

/// A reference whose base is a SOUN: a sound marker.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundMarker {
    pub ref_id: u32,
    pub cell: u32,
    pub position: [f32; 3],
    pub sound: u32,
    pub initially_disabled: bool,
    pub enable_parent: Option<(u32, bool)>,
}

/// What a cell says about its own sound: CELL `XCMO` (MUSC) and `XCAS` (ASPC).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellAudio {
    pub music: Option<u32>,
    pub acoustic_space: Option<u32>,
}

pub fn cell_audio(lo: &LoadOrder, schema: &Schema, cell: u32) -> CellAudio {
    let Some((pi, r)) = winner(lo, cell) else { return CellAudio::default() };
    let Ok(f) = Fields::new(r, pi, schema) else { return CellAudio::default() };
    CellAudio { music: f.formid(b"XCMO", lo), acoustic_space: f.formid(b"XCAS", lo) }
}

/// CELL `XCLR`: the regions an exterior cell is in.
pub fn cell_regions(lo: &LoadOrder, schema: &Schema, cell: u32) -> Vec<u32> {
    let Some((pi, r)) = winner(lo, cell) else { return Vec::new() };
    let Ok(f) = Fields::new(r, pi, schema) else { return Vec::new() };
    let Some(Value::Array(v)) = f.get(b"XCLR") else { return Vec::new() };
    v.iter().filter_map(|x| global(lo, pi, formid_of(x)?)).collect()
}

/// WRLD `ZNAM`: the worldspace's music type.
pub fn world_music(lo: &LoadOrder, schema: &Schema, world: u32) -> Option<u32> {
    let (pi, r) = winner(lo, world)?;
    Fields::new(r, pi, schema).ok()?.formid(b"ZNAM", lo)
}

/// The audio markers and sound markers among `cells`' references, in FormID order.
pub fn markers(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, cells: impl IntoIterator<Item = u32>) -> (Vec<AudioMarker>, Vec<SoundMarker>) {
    let (mut audio, mut sounds) = (Vec::new(), Vec::new());
    for cell in cells {
        let Some(c) = ix.cells.get(&cell) else { continue };
        for &fid in &c.refs {
            let Some((pi, r)) = winner(lo, fid) else { continue };
            if &r.header.sig != b"REFR" || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            // Cheap tests on the raw subrecords before any decoding: a cell holds
            // thousands of references and few of them sound.
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let is_marker = f.raw(b"MMRK").is_some();
            let base = f.formid(b"NAME", lo);
            let base_is_sound = base.and_then(|b| winner(lo, b)).is_some_and(|(_, b)| &b.header.sig == b"SOUN");
            if !is_marker && !base_is_sound {
                continue;
            }
            let Some(p) = placement(lo, schema, fid) else { continue };
            if is_marker {
                audio.push(AudioMarker {
                    ref_id: fid,
                    cell,
                    position: p.position,
                    radius: float(&f, b"XRDS"),
                    name: text(f.get(b"FULL")),
                    controller: f.formid(b"CNAM", lo),
                    use_controller_values: f.get(b"BNAM").and_then(|v| num(&v, "value")).is_some_and(|v| v as u32 & 1 != 0),
                    layer2_percent: float(&f, b"MNAM"),
                    layer3_percent: float(&f, b"NNAM"),
                    initially_disabled: p.initially_disabled(),
                });
            }
            if let (true, Some(sound)) = (base_is_sound, base) {
                sounds.push(SoundMarker { ref_id: fid, cell, position: p.position, sound, initially_disabled: p.initially_disabled(), enable_parent: p.enable_parent });
            }
        }
    }
    audio.sort_by_key(|m| m.ref_id);
    sounds.sort_by_key(|m| m.ref_id);
    (audio, sounds)
}

/// The cells whose references can sound for someone in `cell`: the cell itself, and for
/// an exterior cell every cell of its worldspace (markers reach across cells and many
/// sit in the worldspace's persistent cell).
pub fn cells_in_earshot(ix: &WorldIndex, cell: u32) -> Vec<u32> {
    let Some(c) = ix.cells.get(&cell) else { return Vec::new() };
    let Some(w) = c.world.and_then(|w| ix.worldspaces.get(&w)) else { return vec![cell] };
    let mut v: Vec<u32> = w.grid.values().copied().chain(w.persistent_cell).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Which layers of a location set sound at a distance from its marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layers {
    /// Slots of [`LAYERS`] (the day three or the night three), outer first; `false`:
    /// out of that layer's reach.
    pub on: [bool; 3],
    pub night: bool,
}

/// The layers of `set` heard `distance` units from `marker` [hypothesis, audio.md §4]:
/// a layer sounds within its boundary percentage of the marker's radius, the outer one
/// from the edge inwards, and the nearer ones are added to it. A marker that does not
/// use its controller's values gives the middle and inner boundaries itself, as the
/// fraction of the way in from the edge at which the layer starts (every marker stores
/// 0.33 and 0.67 [observed]), i.e. 67 and 33 percent of the radius.
pub fn layers(set: &MediaSet, marker: &AudioMarker, distance: f32, day: bool) -> Layers {
    let base = if day { 0 } else { 3 };
    let mut percent = [set.boundary_percent[base], set.boundary_percent[base + 1], set.boundary_percent[base + 2]];
    if !marker.use_controller_values {
        percent[1] = (1.0 - marker.layer2_percent) * 100.0;
        percent[2] = (1.0 - marker.layer3_percent) * 100.0;
    }
    let mut on = [false; 3];
    for k in 0..3 {
        on[k] = marker.radius > 0.0 && distance <= marker.radius * percent[k] / 100.0;
    }
    Layers { on, night: !day }
}

/// The marker whose music is heard at `at` [design, audio.md §4]: of the enabled markers
/// with a controller whose radius reaches `at`, the one the listener is deepest inside
/// (smallest distance over radius); ties go to the lower FormID.
pub fn marker_at(markers: &[AudioMarker], at: [f32; 3], enabled: impl Fn(&AudioMarker) -> bool) -> Option<(&AudioMarker, f32)> {
    let mut best: Option<(&AudioMarker, f32)> = None;
    for m in markers {
        if m.controller.is_none() || m.radius <= 0.0 || !enabled(m) {
            continue;
        }
        let d = ((at[0] - m.position[0]).powi(2) + (at[1] - m.position[1]).powi(2) + (at[2] - m.position[2]).powi(2)).sqrt();
        if d <= m.radius && best.is_none_or(|(b, bd)| d / m.radius < bd / b.radius) {
            best = Some((m, d));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_sounds_play_in_their_weathers() {
        let rainy = RegionSound { sound: 1, weathers: 4, chance: 40.0 };
        assert!(rainy.plays_in(4) && rainy.plays_in(4 | 2));
        assert!(!rainy.plays_in(1));
        let any = RegionSound { weathers: 0, ..rainy };
        assert!(any.plays_in(1) && any.plays_in(8));
    }

    fn marker(ref_id: u32, x: f32, radius: f32) -> AudioMarker {
        AudioMarker { ref_id, cell: 1, position: [x, 0.0, 0.0], radius, name: None, controller: Some(9), use_controller_values: true, layer2_percent: 0.5, layer3_percent: 0.9, initially_disabled: false }
    }

    #[test]
    fn day_and_night() {
        let c = MediaController { day_start_hour: 6.0, night_start_hour: 23.9, ..Default::default() };
        assert!(c.is_day(6.0) && c.is_day(12.0) && !c.is_day(5.9) && !c.is_day(23.95));
        // A day that runs over midnight.
        let c = MediaController { day_start_hour: 20.0, night_start_hour: 4.0, ..Default::default() };
        assert!(c.is_day(22.0) && c.is_day(2.0) && !c.is_day(12.0));
    }

    #[test]
    fn the_sets_heard() {
        let c = MediaController { neutral: vec![1], location: vec![2, 3], ..Default::default() };
        assert_eq!(c.sets(), [2, 3]);
        assert_eq!(MediaController { faction: Some(9), ..c.clone() }.sets(), [1]);
        assert_eq!(MediaController { location: vec![], ..c.clone() }.sets(), [1]);
        assert_eq!(MediaController { faction: Some(9), neutral: vec![], ..c }.sets(), [2, 3]);
        assert!(MediaController::default().sets().is_empty());
    }

    #[test]
    fn the_sets_by_standing() {
        let c = MediaController { faction: Some(9), neutral: vec![1], ally: vec![2], friend: vec![], enemy: vec![4], location: vec![5], ..Default::default() };
        assert_eq!(c.sets_for(Some(1)), [4]);
        assert_eq!(c.sets_for(Some(2)), [2]);
        // No friend sets: the neutral ones.
        assert_eq!(c.sets_for(Some(3)), [1]);
        assert_eq!(c.sets_for(Some(0)), [1]);
        assert_eq!(c.sets_for(None), [1]);
        // No faction: the location sets whatever the standing.
        let c = MediaController { faction: None, ..c };
        assert_eq!(c.sets_for(Some(1)), [5]);
    }

    #[test]
    fn dungeon_layers() {
        let set = MediaSet { kind: MediaSetKind::Dungeon, tracks: [Some("3high".into()), Some("1low".into()), Some("2mid".into()), None, None, None], enabled: 80, ..Default::default() };
        assert_eq!(dungeon_tracks(&set), [Some("1low"), Some("2mid"), Some("3high")]);
    }

    #[test]
    fn layers_by_distance() {
        let set = MediaSet { boundary_percent: [100.0, 66.6, 33.3, 100.0, 80.0, 20.0], enabled: 63, ..Default::default() };
        let m = marker(1, 0.0, 1000.0);
        assert_eq!(layers(&set, &m, 900.0, true).on, [true, false, false]);
        assert_eq!(layers(&set, &m, 600.0, true).on, [true, true, false]);
        assert_eq!(layers(&set, &m, 300.0, true).on, [true, true, true]);
        assert_eq!(layers(&set, &m, 1001.0, true).on, [false, false, false]);
        // Night has its own boundaries.
        assert_eq!(layers(&set, &m, 700.0, false), Layers { on: [true, true, false], night: true });
        // The marker's own trigger percentages.
        let own = AudioMarker { use_controller_values: false, ..m };
        assert_eq!(layers(&set, &own, 400.0, true).on, [true, true, false]);
        assert_eq!(layers(&set, &own, 90.0, true).on, [true, true, true]);
    }

    #[test]
    fn deepest_marker_wins() {
        let ms = [marker(1, 0.0, 1000.0), marker(2, 400.0, 500.0), AudioMarker { controller: None, ..marker(3, 300.0, 5000.0) }];
        // 300 from the first (0.3 of its radius), 100 from the second (0.2).
        assert_eq!(marker_at(&ms, [300.0, 0.0, 0.0], |_| true).map(|(m, d)| (m.ref_id, d)), Some((2, 100.0)));
        assert_eq!(marker_at(&ms, [-200.0, 0.0, 0.0], |_| true).map(|(m, _)| m.ref_id), Some(1));
        assert_eq!(marker_at(&ms, [300.0, 0.0, 0.0], |m| m.ref_id != 2).map(|(m, _)| m.ref_id), Some(1));
        assert!(marker_at(&ms, [5000.0, 0.0, 0.0], |_| true).is_none());
    }

    #[test]
    fn acoustic_space_loops() {
        let a = AcousticSpace { loops: [Some(1), Some(2), None, Some(4)], ..Default::default() };
        assert_eq!(a.loop_at(6.0), Some(1));
        assert_eq!(a.loop_at(12.0), Some(2));
        // No dusk sound: the default loop.
        assert_eq!(a.loop_at(18.0), Some(1));
        assert_eq!(a.loop_at(23.0), Some(4));
        assert_eq!(a.loop_at(2.0), Some(4));
    }
}
