//! Placed references (REFR, ACHR, ACRE) as authored in the plugins.
//!
//! This is the simulation's read-only view of placement data: what a reference is an
//! instance of, where it starts, and the extra data that drives activation (count,
//! lock, teleport, ownership, enable parent). Layer 3 owns streaming and drawing the
//! same references; this table only needs to exist once per load order.
//!
//! A reference's cell is the label of the innermost cell-children group around it
//! [documented: UESP "Tes4Mod:Mod File Format", group types 8–10 carry the parent cell's
//! FormID; group nesting observed in every shipped master].

use fnv_esm::raw::{group_type, record_flags, visit_records};
use fnv_esm::{GroupHeader, LoadOrder, Record, Schema};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use crate::extract::{self, first, global, has_flag, int, local_fid, num, text, Issues};
use crate::form::FormId;
use crate::forms::ser_sig;

/// Record flag 0x400 on references. [hypothesis: "Persistent", UESP record-flag tables;
/// `fnvtool sim forms` reports how it correlates with persistent-children groups]
pub const FLAG_PERSISTENT: u32 = 0x0000_0400;
/// Record flag 0x800 on references. [hypothesis: "Initially Disabled", UESP]
pub const FLAG_INITIALLY_DISABLED: u32 = 0x0000_0800;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Lock {
    pub level: u8,
    pub key: Option<FormId>,
    pub leveled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Teleport {
    /// The door reference at the destination.
    pub door: FormId,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EnableParent {
    pub reference: FormId,
    pub opposite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedRef {
    pub id: FormId,
    #[serde(serialize_with = "ser_sig")]
    pub sig: [u8; 4],
    pub edid: Option<String>,
    pub base: FormId,
    pub cell: Option<FormId>,
    /// Load-order index of the plugin whose definition won.
    pub plugin: u8,
    pub flags: u32,
    /// Found in a cell's persistent-children group.
    pub in_persistent_group: bool,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: Option<f32>,
    /// XCNT: stack size of an item reference.
    pub count: Option<i32>,
    pub owner: Option<FormId>,
    pub lock: Option<Lock>,
    pub teleport: Option<Teleport>,
    pub enable_parent: Option<EnableParent>,
    pub linked: Option<FormId>,
}

impl PlacedRef {
    pub fn initially_disabled(&self) -> bool {
        self.flags & FLAG_INITIALLY_DISABLED != 0
    }
}

pub const REF_SIGS: &[&[u8; 4]] = &[b"REFR", b"ACHR", b"ACRE"];

#[derive(Default)]
pub struct RefTable {
    pub refs: BTreeMap<FormId, PlacedRef>,
    /// Lower-case Editor ID -> reference.
    pub edids: HashMap<String, FormId>,
    /// References whose winning definition carries the Deleted flag.
    pub deleted: u64,
    pub issues: Issues,
}

impl RefTable {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> RefTable {
        let mut t = RefTable::default();
        for (pi, p) in lo.plugins.iter().enumerate() {
            let mut found: Vec<(&Record, Option<u32>, bool)> = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, stack: &[&GroupHeader]| {
                if REF_SIGS.contains(&&r.header.sig) {
                    let g = stack.iter().rev().find(|g| {
                        matches!(
                            g.group_type,
                            group_type::CELL_PERSISTENT_CHILDREN
                                | group_type::CELL_TEMPORARY_CHILDREN
                                | group_type::CELL_VISIBLE_DISTANT_CHILDREN
                        )
                    });
                    let persistent = g.is_some_and(|g| g.group_type == group_type::CELL_PERSISTENT_CHILDREN);
                    found.push((r, g.map(|g| g.label_u32()), persistent));
                }
            });
            for (r, cell_local, persistent) in found {
                let Some(id) = lo.to_global(pi, r.header.form_id).map(FormId) else {
                    t.issues.note("reference FormID outside master list", || {
                        format!("{} 0x{:08X}", p.name, r.header.form_id)
                    });
                    continue;
                };
                if r.header.flags & record_flags::DELETED != 0 {
                    if t.refs.remove(&id).is_some() {
                        t.deleted += 1;
                    }
                    continue;
                }
                if let Some(pr) = load_ref(lo, schema, pi, id, r, cell_local, persistent, &mut t.issues) {
                    t.refs.insert(id, pr);
                }
            }
        }
        for (id, r) in &t.refs {
            if let Some(e) = &r.edid {
                t.edids.insert(e.to_ascii_lowercase(), *id);
            }
        }
        t
    }

    pub fn get(&self, id: FormId) -> Option<&PlacedRef> {
        self.refs.get(&id)
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }
}

fn field_fid(
    lo: &LoadOrder,
    pi: usize,
    id: FormId,
    v: Option<&serde_json::Value>,
    what: &str,
    issues: &mut Issues,
) -> Option<FormId> {
    match global(lo, pi, local_fid(v?)?) {
        Ok(g) => g,
        Err(l) => {
            issues.note("FormID outside master list", || format!("{id} {what} 0x{l:08X}"));
            None
        }
    }
}

fn vec3(v: Option<&serde_json::Value>) -> Option<[f32; 3]> {
    let v = v?;
    Some([num(v, "x")? as f32, num(v, "y")? as f32, num(v, "z")? as f32])
}

#[allow(clippy::too_many_arguments)]
fn load_ref(
    lo: &LoadOrder,
    schema: &Schema,
    pi: usize,
    id: FormId,
    r: &Record,
    cell_local: Option<u32>,
    in_persistent_group: bool,
    issues: &mut Issues,
) -> Option<PlacedRef> {
    let d = match extract::decode(r, schema) {
        Ok(d) => d,
        Err(e) => {
            issues.note("reference undecodable", || format!("{id}: {e}"));
            return None;
        }
    };
    let cell = match cell_local.map(|c| global(lo, pi, c)) {
        Some(Ok(g)) => g,
        Some(Err(l)) => {
            issues.note("FormID outside master list", || format!("{id} cell 0x{l:08X}"));
            None
        }
        None => None,
    };
    let fid = |v: Option<&serde_json::Value>, what: &str, issues: &mut Issues| field_fid(lo, pi, id, v, what, issues);
    let Some(base) = fid(first(&d, b"NAME"), "NAME", issues) else {
        issues.note("reference without base (NAME)", || format!("{id}"));
        return None;
    };
    let posrot = first(&d, b"DATA");
    let pos = vec3(posrot.and_then(|v| v.get("position")));
    let rot = vec3(posrot.and_then(|v| v.get("rotation_radians")));
    if pos.is_none() {
        issues.note("reference without position (DATA)", || format!("{id}"));
    }
    let lock = first(&d, b"XLOC").map(|v| Lock {
        level: int(v, "level").unwrap_or(0) as u8,
        key: fid(v.get("key"), "XLOC key", issues),
        leveled: v.get("flags").is_some_and(|f| has_flag(f, "Leveled Lock")),
    });
    let teleport = match first(&d, b"XTEL") {
        Some(v) => {
            let door = fid(v.get("door"), "XTEL door", issues);
            let pr = v.get("pos_rot");
            match (door, vec3(pr.and_then(|p| p.get("position"))), vec3(pr.and_then(|p| p.get("rotation_radians")))) {
                (Some(door), Some(pos), Some(rot)) => Some(Teleport { door, pos, rot }),
                _ => {
                    issues.note("XTEL incomplete", || format!("{id}"));
                    None
                }
            }
        }
        None => None,
    };
    let enable_parent = first(&d, b"XESP").and_then(|v| {
        Some(EnableParent {
            reference: fid(v.get("reference"), "XESP", issues)?,
            opposite: v.get("flags").is_some_and(|f| has_flag(f, "Set Enable State to Opposite of Parent")),
        })
    });
    Some(PlacedRef {
        id,
        sig: r.header.sig,
        edid: first(&d, b"EDID").and_then(text),
        base,
        cell,
        plugin: pi as u8,
        flags: r.header.flags,
        in_persistent_group,
        pos: pos.unwrap_or_default(),
        rot: rot.unwrap_or_default(),
        scale: first(&d, b"XSCL").and_then(serde_json::Value::as_f64).map(|s| s as f32),
        count: first(&d, b"XCNT").and_then(serde_json::Value::as_i64).map(|c| c as i32),
        owner: fid(first(&d, b"XOWN"), "XOWN", issues),
        lock,
        teleport,
        enable_parent,
        linked: fid(first(&d, b"XLKR"), "XLKR", issues),
    })
}
