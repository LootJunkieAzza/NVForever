//! Radios: the stations (talking activators marked as radio stations), the placed
//! references of the stations with their broadcast range, and the receivers (activators
//! that name a station). Facts and tags: docs/spec-notes/radio.md.
//!
//! What a station plays (its quests' Radio lines) is dialogue and lives in `fnv-game`
//! (`fnv_game::radio`); how a receiver sounds is the engine's.

use crate::fields::{formid_of, global, num, winner, Fields};
use crate::index::WorldIndex;
use crate::placed::{placement, ref_flags};
use fnv_esm::{LoadOrder, Schema};
use serde_json::Value;
use std::collections::BTreeMap;

/// TACT record-header flag bits seen on stations (docs/spec-notes/radio.md §2).
pub mod station_flags {
    /// Set on every talking activator the editor's tutorial would call a radio station
    /// (the GECK's "Radio Station" box) [hypothesis: by the stations that have it,
    /// radio.md §2].
    pub const RADIO_STATION: u32 = 0x0002_0000;
    /// The GECK's "Continuous Broadcast" box: the station's broadcast state at the start
    /// [hypothesis: radio.md §2].
    pub const CONTINUOUS: u32 = 0x4000_0000;
}

/// A talking activator (TACT).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Station {
    pub form_id: u32,
    pub edid: Option<String>,
    pub name: Option<String>,
    /// The record header's flags.
    pub flags: u32,
    /// `VNAM` voice type.
    pub voice_type: Option<u32>,
    /// `INAM` radio template (SOUN): how far a receiver of the station is heard.
    pub template: Option<u32>,
    /// `SNAM` looping sound.
    pub looping: Option<u32>,
}

impl Station {
    pub fn is_radio(&self) -> bool {
        self.flags & station_flags::RADIO_STATION != 0
    }

    /// Broadcasting when the game starts [hypothesis, see [`station_flags::CONTINUOUS`]].
    pub fn continuous(&self) -> bool {
        self.flags & station_flags::CONTINUOUS != 0
    }
}

/// An activator (ACTI) that names a radio station (`RNAM`): a receiver.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReceiverBase {
    pub form_id: u32,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub station: u32,
    /// `INAM` radio template (SOUN) of the activator itself.
    pub template: Option<u32>,
    /// `SCRI`.
    pub script: Option<u32>,
    /// `DEST` present: the activator can be destroyed.
    pub destructible: bool,
}

/// The radio forms of a load order.
#[derive(Debug, Clone, Default)]
pub struct RadioForms {
    /// Every TACT, radio station or not.
    pub stations: BTreeMap<u32, Station>,
    pub receivers: BTreeMap<u32, ReceiverBase>,
}

impl RadioForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> RadioForms {
        let mut out = RadioForms::default();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let pi = d.plugin as usize;
            let r = lo.records[pi][d.ordinal as usize];
            if !matches!(&r.header.sig, b"TACT" | b"ACTI") || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let edid = f.string(b"EDID");
            let name = f.string(b"FULL").filter(|s| !s.trim().is_empty());
            if &r.header.sig == b"TACT" {
                let station = Station {
                    form_id: g,
                    edid,
                    name,
                    flags: r.header.flags,
                    voice_type: f.formid(b"VNAM", lo),
                    template: f.formid(b"INAM", lo),
                    looping: f.formid(b"SNAM", lo),
                };
                out.stations.insert(g, station);
            } else if let Some(station) = f.formid(b"RNAM", lo) {
                let receiver = ReceiverBase {
                    form_id: g,
                    edid,
                    name,
                    station,
                    template: f.formid(b"INAM", lo),
                    script: f.formid(b"SCRI", lo),
                    destructible: f.raw(b"DEST").is_some(),
                };
                out.receivers.insert(g, receiver);
            }
        }
        out
    }
}

/// REFR `XRDO` broadcast range type [documented: xEdit REFR "Radio Data"; GECK wiki
/// Reference, "Broadcast Range Type"].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BroadcastRange {
    #[default]
    Radius,
    Everywhere,
    WorldspaceAndLinkedInteriors,
    LinkedInteriors,
    CurrentCell,
    Other(u32),
}

impl BroadcastRange {
    fn of(code: u32) -> BroadcastRange {
        match code {
            0 => BroadcastRange::Radius,
            1 => BroadcastRange::Everywhere,
            2 => BroadcastRange::WorldspaceAndLinkedInteriors,
            3 => BroadcastRange::LinkedInteriors,
            4 => BroadcastRange::CurrentCell,
            n => BroadcastRange::Other(n),
        }
    }
}

/// A placed reference of a talking activator, with its `XRDO` when it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct StationRef {
    pub ref_id: u32,
    pub base: u32,
    pub cell: u32,
    pub position: [f32; 3],
    pub initially_disabled: bool,
    /// `XRDO`: range radius, range type, static percentage, exterior position reference.
    pub radio: Option<(f32, BroadcastRange, f32, Option<u32>)>,
}

/// A placed receiver.
#[derive(Debug, Clone, PartialEq)]
pub struct Receiver {
    pub ref_id: u32,
    pub base: u32,
    pub station: u32,
    pub cell: u32,
    pub position: [f32; 3],
    pub initially_disabled: bool,
    pub enable_parent: Option<(u32, bool)>,
}

/// The placed stations and receivers of `cells`, by reference.
pub fn placed(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, forms: &RadioForms, cells: impl IntoIterator<Item = u32>) -> (Vec<StationRef>, Vec<Receiver>) {
    let (mut stations, mut receivers) = (Vec::new(), Vec::new());
    for cell in cells {
        let Some(c) = ix.cells.get(&cell) else { continue };
        for &fid in &c.refs {
            let Some((pi, r)) = winner(lo, fid) else { continue };
            if &r.header.sig != b"REFR" || r.header.flags & ref_flags::DELETED != 0 {
                continue;
            }
            let Ok(f) = Fields::new(r, pi, schema) else { continue };
            let Some(base) = f.formid(b"NAME", lo) else { continue };
            let is_station = forms.stations.contains_key(&base);
            let receiver = forms.receivers.get(&base);
            if !is_station && receiver.is_none() {
                continue;
            }
            let Some(p) = placement(lo, schema, fid) else { continue };
            if is_station {
                let radio = f.get(b"XRDO").map(|v| {
                    let range = v.get("broadcast_range_type").map_or(0, enum_code);
                    let pos_ref = v.get("position_reference").and_then(formid_of).and_then(|l| global(lo, pi, l));
                    (num(&v, "range_radius").unwrap_or(0.0) as f32, BroadcastRange::of(range), num(&v, "static_percentage").unwrap_or(0.0) as f32, pos_ref)
                });
                stations.push(StationRef { ref_id: fid, base, cell, position: p.position, initially_disabled: p.initially_disabled(), radio });
            }
            if let Some(rb) = receiver {
                receivers.push(Receiver {
                    ref_id: fid,
                    base,
                    station: rb.station,
                    cell,
                    position: p.position,
                    initially_disabled: p.initially_disabled(),
                    enable_parent: p.enable_parent,
                });
            }
        }
    }
    stations.sort_by_key(|s| s.ref_id);
    receivers.sort_by_key(|r| r.ref_id);
    (stations, receivers)
}

/// An enum field as the decoder gives it: the schema's name of the value, or the number
/// when the schema has no name for it.
fn enum_code(v: &Value) -> u32 {
    match v.as_str() {
        Some("Radius") => 0,
        Some("Everywhere") => 1,
        Some("Worldspace and Linked Interiors") => 2,
        Some("Linked Interiors") => 3,
        Some("Current Cell Only") => 4,
        Some(_) => u32::MAX,
        None => v.as_u64().unwrap_or(0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn station_flag_bits() {
        let s = Station { flags: 0x5002_0000, ..Station::default() };
        assert!(s.is_radio() && s.continuous());
        let s = Station { flags: 0x1002_0000, ..Station::default() };
        assert!(s.is_radio() && !s.continuous());
        assert!(!Station::default().is_radio());
    }

    #[test]
    fn range_codes() {
        assert_eq!(BroadcastRange::of(enum_code(&json!("Everywhere"))), BroadcastRange::Everywhere);
        assert_eq!(BroadcastRange::of(enum_code(&json!("Current Cell Only"))), BroadcastRange::CurrentCell);
        assert_eq!(BroadcastRange::of(enum_code(&json!(4))), BroadcastRange::CurrentCell);
        assert_eq!(BroadcastRange::of(enum_code(&json!(7))), BroadcastRange::Other(7));
        assert_eq!(BroadcastRange::of(9), BroadcastRange::Other(9));
    }
}
