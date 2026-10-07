//! Scripted action sequences and their traces: the Layer 4 parity scenario format
//! ("collision and inventory state after scripted actions", spec section 8.1).
//!
//! Input `fnv.sim.actions/1`:
//! ```json
//! {"format": "fnv.sim.actions/1", "seed": 0, "player_level": 1,
//!  "actions": [{"op": "add_item", "to": "player", "item": "Caps001", "count": 5},
//!              {"op": "activate", "actor": "player", "target": "0x0010A1B2"}],
//!  "report": ["player"]}
//! ```
//! References and forms are named by FormID (global, hex), by Editor ID, or
//! `"player"`. Output `fnv.sim.trace/1`: one result per step (a failed step records
//! its error and the run continues, since errors are behaviour too), the
//! `observations` made by the query ops (`get_item_count`, `get_pos`, `get_disabled`,
//! `get_global`, `get_av`, `get_weight`), and the final
//! state of the reported references (all changed references when `report` is absent).
//!
//! Observations are what the parity scenarios compare: they are the values an in-game
//! capture can also record (console/xNVSE `GetItemCount`, `GetPos`, parent cell), keyed
//! by their `label`, so the oracle does not need to know how this engine names its
//! internal outcomes.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::form::FormId;
use crate::world::{SimConfig, World};

pub const ACTIONS_FORMAT: &str = "fnv.sim.actions/1";
pub const TRACE_FORMAT: &str = "fnv.sim.trace/1";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub format: String,
    #[serde(default)]
    pub seed: u64,
    #[serde(default = "one")]
    pub player_level: u16,
    #[serde(default)]
    pub hardcore: bool,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub report: Option<Vec<String>>,
}

fn one() -> u16 {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Activate { actor: String, target: String },
    AddItem { to: String, item: String, count: i32 },
    RemoveItem { from: String, item: String, count: i32 },
    Transfer { from: String, to: String, item: String, count: i32 },
    TransferAll { from: String, to: String },
    DropItem { from: String, item: String, count: i32 },
    GetItemCount { of: String, item: String, #[serde(default)] label: Option<String> },
    GetPos { of: String, #[serde(default)] label: Option<String> },
    GetDisabled { of: String, #[serde(default)] label: Option<String> },
    GetGlobal { name: String, #[serde(default)] label: Option<String> },
    SetGlobal { name: String, value: f32 },
    AdvanceTime { hours: f64 },
    EquipItem { actor: String, item: String },
    UnequipItem { actor: String, item: String },
    GetAv { of: String, name: String, #[serde(default)] label: Option<String> },
    SetAv { of: String, name: String, value: f32 },
    GetWeight { of: String, #[serde(default)] label: Option<String> },
    RemoveAllItems { from: String },
    Lock { target: String, #[serde(default)] level: Option<u8> },
    Unlock { target: String },
    Enable { target: String },
    Disable { target: String },
}

impl Action {
    fn op(&self) -> &'static str {
        match self {
            Action::Activate { .. } => "activate",
            Action::AddItem { .. } => "add_item",
            Action::RemoveItem { .. } => "remove_item",
            Action::Transfer { .. } => "transfer",
            Action::TransferAll { .. } => "transfer_all",
            Action::DropItem { .. } => "drop_item",
            Action::GetItemCount { .. } => "get_item_count",
            Action::GetPos { .. } => "get_pos",
            Action::GetDisabled { .. } => "get_disabled",
            Action::GetGlobal { .. } => "get_global",
            Action::SetGlobal { .. } => "set_global",
            Action::AdvanceTime { .. } => "advance_time",
            Action::EquipItem { .. } => "equip_item",
            Action::UnequipItem { .. } => "unequip_item",
            Action::GetAv { .. } => "get_av",
            Action::SetAv { .. } => "set_av",
            Action::GetWeight { .. } => "get_weight",
            Action::RemoveAllItems { .. } => "remove_all_items",
            Action::Lock { .. } => "lock",
            Action::Unlock { .. } => "unlock",
            Action::Enable { .. } => "enable",
            Action::Disable { .. } => "disable",
        }
    }
}

/// Resolve a reference name: "player", a hex FormID, or a reference's Editor ID.
pub fn resolve_ref(w: &World, s: &str) -> Result<FormId, String> {
    if s.eq_ignore_ascii_case("player") {
        return Ok(FormId::PLAYER_REF);
    }
    if let Ok(id) = s.parse::<FormId>() {
        if s.starts_with("0x") || s.starts_with("0X") {
            return Ok(id);
        }
    }
    w.placed.by_edid(s).ok_or_else(|| format!("no reference named {s:?}"))
}

/// Resolve a base form name: a hex FormID or an Editor ID.
pub fn resolve_form(w: &World, s: &str) -> Result<FormId, String> {
    if let Ok(id) = s.parse::<FormId>() {
        if s.starts_with("0x") || s.starts_with("0X") {
            return Ok(id);
        }
    }
    w.forms.by_edid(s).ok_or_else(|| format!("no base form named {s:?}"))
}

fn step(w: &mut World, a: &Action) -> Result<Value, String> {
    let r = |w: &World, s: &str| resolve_ref(w, s);
    let f = |w: &World, s: &str| resolve_form(w, s);
    let e = |e: crate::world::SimError| e.to_string();
    Ok(match a {
        Action::Activate { actor, target } => {
            let (a, t) = (r(w, actor)?, r(w, target)?);
            serde_json::to_value(w.activate(a, t).map_err(e)?).expect("serialisable")
        }
        Action::AddItem { to, item, count } => {
            let (to, item) = (r(w, to)?, f(w, item)?);
            let added: Vec<_> = w.add_item(to, item, *count).map_err(e)?.into_iter().map(|(i, n)| json!([i, n])).collect();
            json!({"added": added})
        }
        Action::RemoveItem { from, item, count } => {
            let (from, item) = (r(w, from)?, f(w, item)?);
            json!({"removed": w.remove_item(from, item, *count).map_err(e)?})
        }
        Action::Transfer { from, to, item, count } => {
            let (from, to, item) = (r(w, from)?, r(w, to)?, f(w, item)?);
            json!({"moved": w.transfer(from, to, item, *count).map_err(e)?})
        }
        Action::TransferAll { from, to } => {
            let (from, to) = (r(w, from)?, r(w, to)?);
            let moved: Vec<_> = w.transfer_all(from, to).map_err(e)?.into_iter().map(|(i, n)| json!([i, n])).collect();
            json!({"moved": moved})
        }
        Action::DropItem { from, item, count } => {
            let (from, item) = (r(w, from)?, f(w, item)?);
            json!({"references": w.drop_item(from, item, *count).map_err(e)?})
        }
        Action::GetItemCount { of, item, .. } => {
            let (of, item) = (r(w, of)?, f(w, item)?);
            json!({"count": w.item_count(of, item).map_err(e)?})
        }
        Action::GetPos { of, .. } => {
            let s = w.get(r(w, of)?).map_err(e)?;
            json!({"cell": s.cell, "pos": s.pos, "rot": s.rot})
        }
        Action::GetDisabled { of, .. } => {
            json!({"disabled": !w.is_enabled(r(w, of)?).map_err(e)?})
        }
        Action::GetGlobal { name, .. } => {
            json!({"value": w.global(f(w, name)?).map_err(e)?})
        }
        Action::SetGlobal { name, value } => {
            w.set_global(f(w, name)?, *value).map_err(e)?;
            json!({})
        }
        Action::AdvanceTime { hours } => {
            let c = w.advance_hours(*hours).map_err(e)?;
            json!({"year": c.year, "month": c.month, "day": c.day, "hour": c.hour, "days_passed": c.days_passed})
        }
        Action::EquipItem { actor, item } => {
            let (a, i) = (r(w, actor)?, f(w, item)?);
            serde_json::to_value(w.equip(a, i).map_err(e)?).expect("serialisable")
        }
        Action::UnequipItem { actor, item } => {
            let (a, i) = (r(w, actor)?, f(w, item)?);
            json!({"was_equipped": w.unequip(a, i).map_err(e)?})
        }
        Action::GetAv { of, name, .. } => {
            json!({"value": w.actor_value(r(w, of)?, name).map_err(e)?})
        }
        Action::SetAv { of, name, value } => {
            w.set_actor_value(r(w, of)?, name, *value).map_err(e)?;
            json!({})
        }
        Action::GetWeight { of, .. } => {
            serde_json::to_value(w.encumbrance(r(w, of)?).map_err(e)?).expect("serialisable")
        }
        Action::RemoveAllItems { from } => {
            let removed: Vec<_> = w.remove_all_items(r(w, from)?).map_err(e)?.into_iter().map(|(i, n)| json!([i, n])).collect();
            json!({"removed": removed})
        }
        Action::Lock { target, level } => {
            w.lock(r(w, target)?, *level).map_err(e)?;
            json!({})
        }
        Action::Unlock { target } => {
            w.unlock(r(w, target)?).map_err(e)?;
            json!({})
        }
        Action::Enable { target } => {
            w.set_enabled(r(w, target)?, true).map_err(e)?;
            json!({})
        }
        Action::Disable { target } => {
            w.set_enabled(r(w, target)?, false).map_err(e)?;
            json!({})
        }
    })
}

pub fn config(s: &Script) -> SimConfig {
    SimConfig { seed: s.seed, player_level: s.player_level, hardcore: s.hardcore }
}

/// Run every action on `w` and build the trace document.
pub fn run(w: &mut World, s: &Script) -> Result<Value, String> {
    if s.format != ACTIONS_FORMAT {
        return Err(format!("unsupported actions format {:?} (expected {ACTIONS_FORMAT})", s.format));
    }
    let mut steps = Vec::with_capacity(s.actions.len());
    let mut observations = Vec::new();
    for (i, a) in s.actions.iter().enumerate() {
        let mut o = json!({"step": i, "op": a.op()});
        let res = step(w, a);
        if let Action::GetItemCount { label, .. }
        | Action::GetPos { label, .. }
        | Action::GetDisabled { label, .. }
        | Action::GetGlobal { label, .. }
        | Action::GetAv { label, .. }
        | Action::GetWeight { label, .. } = a
        {
            let mut ob = json!({"label": label.clone().unwrap_or_else(|| format!("step{i}")), "step": i});
            match &res {
                Ok(v) => {
                    for (k, x) in v.as_object().expect("query results are objects") {
                        ob[k] = x.clone();
                    }
                }
                Err(err) => ob["error"] = json!(err),
            }
            observations.push(ob);
        }
        match res {
            Ok(v) => o["result"] = v,
            Err(err) => o["error"] = json!(err),
        }
        steps.push(o);
    }
    let mut fin = serde_json::Map::new();
    match &s.report {
        Some(names) => {
            for n in names {
                let id = resolve_ref(w, n)?;
                let st = w.get(id).map_err(|e| e.to_string())?;
                fin.insert(id.to_string(), serde_json::to_value(&*st).expect("serialisable"));
            }
        }
        None => {
            for (id, st) in w.changed() {
                fin.insert(id.to_string(), serde_json::to_value(st).expect("serialisable"));
            }
        }
    }
    Ok(json!({
        "format": TRACE_FORMAT,
        "seed": s.seed,
        "player_level": s.player_level,
        "steps": steps,
        "observations": observations,
        "final": fin,
    }))
}
