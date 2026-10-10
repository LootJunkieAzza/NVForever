//! `fnvtool ai packages | show | observed`: AI packages as the game data reads them
//! (`fnv_game::package`), checked against the raw survey and against what the AI observer
//! recorded in the game. JSON schemas: docs/spec-notes/packages.md §5.

use crate::ai_choice::Seen;
use crate::observer::{self, Form};
use fnv_esm::LoadOrder;
use fnv_game::package::{object_type_name, Location, LocationKind, PackageDef, PackageType, Schedule, Target, TargetKind};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value as ScriptValue};
use fnv_sim::{FormId, FormStore, RefTable};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn bump(m: &mut BTreeMap<String, u64>, k: impl Into<String>) {
    *m.entry(k.into()).or_default() += 1;
}

fn bump2(m: &mut BTreeMap<String, BTreeMap<String, u64>>, a: impl Into<String>, b: impl Into<String>) {
    bump(m.entry(a.into()).or_default(), b);
}

/// The survey's names for location and target kinds (the schema's enumeration names).
fn location_kind_name(k: &LocationKind) -> String {
    match k {
        LocationKind::NearReference(_) => "Near reference".into(),
        LocationKind::InCell(_) => "In cell".into(),
        LocationKind::NearCurrent => "Near current location".into(),
        LocationKind::NearEditor => "Near editor location".into(),
        LocationKind::ObjectId(_) => "Object ID".into(),
        LocationKind::ObjectType(_) => "Object Type".into(),
        LocationKind::NearLinked => "Near linked reference".into(),
        LocationKind::AtPackageLocation => "At package location".into(),
        LocationKind::Other(c) => c.to_string(),
    }
}

fn target_kind_name(k: &TargetKind) -> String {
    match k {
        TargetKind::Reference(_) => "Specific Reference".into(),
        TargetKind::ObjectId(_) => "Object ID".into(),
        TargetKind::ObjectType(_) => "Object Type".into(),
        TargetKind::LinkedReference => "Linked Reference".into(),
        TargetKind::Other(c) => c.to_string(),
    }
}

fn type_name(t: PackageType) -> String {
    match t {
        PackageType::Other(c) => c.to_string(),
        t => t.name().to_string(),
    }
}

fn object_type(code: u32) -> String {
    match object_type_name(code) {
        Some(n) => format!("{code} {n}"),
        None => code.to_string(),
    }
}

/// A form with its Editor ID. The player's reference is the game's own form.
fn named(data: &GameData, refs: &RefTable, id: FormId) -> Value {
    let edid = data
        .edid_of
        .get(&id)
        .cloned()
        .or_else(|| refs.get(id).and_then(|r| r.edid.clone()))
        .or_else(|| (id == FormId::PLAYER_REF).then(|| "PlayerRef".to_string()));
    json!({"form": id.to_string(), "edid": edid})
}

fn opt_named(data: &GameData, refs: &RefTable, id: Option<FormId>) -> Value {
    id.map(|i| named(data, refs, i)).unwrap_or(Value::Null)
}

fn location_json(data: &GameData, refs: &RefTable, l: &Location) -> Value {
    let mut v = json!({"kind": location_kind_name(&l.kind), "radius": l.radius});
    match l.kind {
        LocationKind::NearReference(f) | LocationKind::InCell(f) | LocationKind::ObjectId(f) => v["form"] = opt_named(data, refs, f),
        LocationKind::ObjectType(c) => v["object_type"] = json!(object_type(c)),
        _ => {}
    }
    v
}

fn target_json(data: &GameData, refs: &RefTable, t: &Target) -> Value {
    let mut v = json!({"kind": target_kind_name(&t.kind), "count_distance": t.count_distance});
    match t.kind {
        TargetKind::Reference(f) | TargetKind::ObjectId(f) => v["form"] = opt_named(data, refs, f),
        TargetKind::ObjectType(c) => v["object_type"] = json!(object_type(c)),
        _ => {}
    }
    v
}

fn schedule_json(s: &Schedule) -> Value {
    const DAYS: [&str; 11] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Weekdays",
        "Weekends",
        "Monday, Wednesday, Friday",
        "Tuesday, Thursday",
    ];
    if s.is_any() && s.duration == 0 {
        return json!("any time");
    }
    let any = |x: i8, none: i8| if x == none || x < 0 { json!("any") } else { json!(x) };
    json!({
        "month": any(s.month, -1),
        "day_of_week": if s.day_of_week < 0 { json!("any") } else { json!(DAYS.get(s.day_of_week as usize).copied().unwrap_or("?")) },
        "date": any(s.date, 0),
        "hour": any(s.hour, -1),
        "duration_hours": s.duration,
    })
}

fn condition_json(data: &GameData, refs: &RefTable, c: &fnv_game::cond::Condition) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
    v["function_name"] = json!(data.table.functions.get(&c.opcode()).map(|f| f.name.clone()));
    v["params"] = c
        .params
        .iter()
        .map(|p| match p {
            ArgValue::Value(ScriptValue::Ref(r)) => named(data, refs, FormId(*r)),
            ArgValue::Value(ScriptValue::Num(n)) => json!(n),
            ArgValue::Enum(e) => json!({"code": e}),
            other => json!(format!("{other:?}")),
        })
        .collect();
    v
}

/// A patrol's route when its location names a reference: that reference and the chain
/// of linked references after it.
fn patrol_route(data: &GameData, refs: &RefTable, p: &PackageDef) -> Value {
    match (p.ty, p.location.map(|l| l.kind)) {
        (PackageType::Patrol, Some(LocationKind::NearReference(Some(start)))) if refs.get(start).is_some() => {
            let (chain, looped) = linked_chain(refs, start);
            json!({"points": chain.len(), "loops": looped, "references": chain.iter().take(40).map(|r| named(data, refs, *r)).collect::<Vec<_>>()})
        }
        _ => Value::Null,
    }
}

/// One package, every field, forms named.
fn package_json(data: &GameData, refs: &RefTable, id: FormId, p: &PackageDef) -> Value {
    let event = |i: usize| {
        let e = &p.events[i];
        json!({
            "idle": opt_named(data, refs, e.idle),
            "topic": opt_named(data, refs, e.topic),
            "result_script": data.packages.get(&id).is_some_and(|u| u[i].is_some()),
        })
    };
    json!({
        "form": id.to_string(),
        "edid": data.edid_of.get(&id),
        "type": type_name(p.ty),
        "type_code": p.ty.code(),
        "runtime_type_code": p.ty.runtime_code(),
        "flags": p.flag_names(),
        "flags_raw": p.flags,
        "fallout_flags_raw": p.fallout_flags,
        "type_flags": p.type_flag_names(),
        "type_flags_raw": p.type_flags,
        "location": p.location.as_ref().map(|l| location_json(data, refs, l)),
        "location2": p.location2.as_ref().map(|l| location_json(data, refs, l)),
        "target": p.target.as_ref().map(|t| target_json(data, refs, t)),
        "target2": p.target2.as_ref().map(|t| target_json(data, refs, t)),
        "schedule": schedule_json(&p.schedule),
        "conditions": p.conditions.iter().map(|c| condition_json(data, refs, c)).collect::<Vec<_>>(),
        "uses_linked_ref": p.uses_linked_ref(),
        "patrol_route": patrol_route(data, refs, p),
        "combat_style": opt_named(data, refs, p.combat_style),
        "idles": p.idles.as_ref().map(|c| json!({
            "run_in_sequence": c.run_in_sequence, "do_once": c.do_once, "timer": c.timer,
            "idles": c.idles.iter().map(|i| named(data, refs, *i)).collect::<Vec<_>>(),
        })),
        "patrol_repeatable": p.patrol_repeatable,
        "escort_distance": p.escort_distance,
        "follow_trigger_radius": p.follow_trigger_radius,
        "use_weapon": p.use_weapon,
        "dialogue": p.dialogue.as_ref().map(|d| json!({
            "fov": d.fov, "topic": opt_named(data, refs, d.topic), "no_headtracking": d.no_headtracking,
            "dont_control_target_movement": d.dont_control_target_movement, "say_to": d.say_to,
        })),
        "begin": event(0), "end": event(1), "change": event(2),
    })
}

/// A short line for package lists.
fn package_brief(data: &GameData, refs: &RefTable, id: FormId) -> Value {
    match data.package_defs.get(&id) {
        None => json!({"form": id.to_string(), "edid": data.edid_of.get(&id), "missing": true}),
        Some(p) => json!({
            "form": id.to_string(),
            "edid": data.edid_of.get(&id),
            "type": type_name(p.ty),
            "schedule": schedule_json(&p.schedule),
            "conditions": p.conditions.len(),
            "location": p.location.as_ref().map(|l| location_json(data, refs, l)),
            "target": p.target.as_ref().map(|t| target_json(data, refs, t)),
            "uses_linked_ref": p.uses_linked_ref(),
        }),
    }
}

/// The references reached from `start` by following linked references (XLKR), `start`
/// first; stops at a reference already visited (a loop) or after 256.
fn linked_chain(refs: &RefTable, start: FormId) -> (Vec<FormId>, bool) {
    let mut chain = vec![start];
    let mut at = start;
    while let Some(next) = refs.get(at).and_then(|r| r.linked) {
        if chain.contains(&next) {
            return (chain, true);
        }
        if chain.len() >= 256 {
            break;
        }
        chain.push(next);
        at = next;
    }
    (chain, false)
}

/// `ai packages`: what the typed reader holds, and whether it agrees with the raw
/// survey (`ai survey`, which reads the schema's decoded values directly).
pub fn packages(data: &GameData, refs: &RefTable, survey: &Value) -> Value {
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut loc: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut tgt: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut scheduled: BTreeMap<String, u64> = BTreeMap::new();
    let mut flags: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut type_flags: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut object_types: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut linked: BTreeMap<String, u64> = BTreeMap::new();
    let mut unmapped: BTreeMap<String, u64> = BTreeMap::new();
    let mut extras: BTreeMap<String, u64> = BTreeMap::new();
    let mut conditions = 0u64;
    let mut shapes: BTreeMap<String, u64> = BTreeMap::new();
    // Everything on a Sandbox or Wander package that bears on what its actor does there.
    let mut sandbox: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut sandbox_radii: Vec<i64> = Vec::new();
    let mut patrol_routes: BTreeMap<String, u64> = BTreeMap::new();
    let mut route_points: Vec<i64> = Vec::new();
    let mut ids: Vec<&FormId> = data.package_defs.keys().collect();
    ids.sort();
    for id in ids {
        let p = &data.package_defs[id];
        let t = type_name(p.ty);
        bump(&mut by_type, t.clone());
        conditions += p.conditions.len() as u64;
        for (slot, l) in [(1, &p.location), (2, &p.location2)] {
            let Some(l) = l else { continue };
            bump2(&mut loc, format!("{t} / location {slot}"), location_kind_name(&l.kind));
            match l.kind {
                LocationKind::NearReference(None) | LocationKind::InCell(None) | LocationKind::ObjectId(None) => {
                    bump(&mut unmapped, format!("location: {}", location_kind_name(&l.kind)))
                }
                LocationKind::ObjectType(c) => bump2(&mut object_types, "location", object_type(c)),
                _ => {}
            }
        }
        for (slot, x) in [(1, &p.target), (2, &p.target2)] {
            let Some(x) = x else { continue };
            bump2(&mut tgt, format!("{t} / target {slot}"), target_kind_name(&x.kind));
            match x.kind {
                TargetKind::Reference(None) | TargetKind::ObjectId(None) => bump(&mut unmapped, format!("target: {}", target_kind_name(&x.kind))),
                TargetKind::ObjectType(c) => bump2(&mut object_types, format!("{t} target {slot}"), object_type(c)),
                _ => {}
            }
        }
        if !p.schedule.is_any() || p.schedule.duration > 0 {
            bump(&mut scheduled, t.clone());
        }
        let sc = &p.schedule;
        bump(&mut shapes, match (sc.hour >= 0, sc.duration) {
            (true, 0) => "a starting hour, duration 0 (never in its window)",
            (true, 1..=23) => "a starting hour and a duration under 24 hours",
            (true, _) => "a starting hour, duration 24 hours or more (any time)",
            (false, 0) => "no starting hour, no duration (any time)",
            (false, _) => "no starting hour, a duration (any time)",
        });
        if sc.day_of_week >= 0 {
            bump(&mut shapes, "a day of the week");
        }
        for n in p.flag_names() {
            bump2(&mut flags, t.clone(), n);
        }
        for n in p.type_flag_names() {
            bump2(&mut type_flags, t.clone(), n);
        }
        if p.uses_linked_ref() {
            bump(&mut linked, t.clone());
        }
        if matches!(p.ty, PackageType::Sandbox | PackageType::Wander) {
            use fnv_game::package::{fallout, flags as general};
            let allowed = fnv_ai::Allowed::of(p);
            bump2(&mut sandbox, t.clone(), "(packages)");
            for (on, name) in [
                (allowed.eating, "eating allowed"),
                (allowed.sleeping, "sleeping allowed"),
                (allowed.conversation, "conversation allowed"),
                (allowed.idle_markers, "idle markers allowed"),
                (allowed.furniture, "furniture allowed"),
                (allowed.wandering, "wandering allowed"),
                (!(allowed.eating || allowed.sleeping || allowed.conversation || allowed.idle_markers || allowed.furniture || allowed.wandering), "nothing allowed"),
                (p.fallout_flags & fallout::RANDOM_CONVERSATIONS != 0, "Fallout behaviour: Random Conversations"),
                (p.fallout_flags & fallout::ALLOW_IDLE_CHATTER != 0, "Fallout behaviour: Allow Idle Chatter"),
                (p.has(general::NO_IDLE_ANIMS), "No Idle Anims"),
                (p.has(general::MUST_COMPLETE), "Must Complete (invalid for a sandbox)"),
                (p.has(general::ALWAYS_RUN), "Always Run"),
                (p.has(general::ALWAYS_SNEAK), "Always Sneak"),
                (p.has(general::WEAPON_DRAWN), "Weapon Drawn"),
                (p.has(general::CONTINUE_IF_PC_NEAR), "Continue If PC Near"),
                (p.idles.as_ref().is_some_and(|c| !c.idles.is_empty()), "has package idles"),
                (matches!(p.location.map(|l| l.kind), Some(LocationKind::InCell(_))), "location: the whole cell"),
            ] {
                if on {
                    bump2(&mut sandbox, t.clone(), name);
                }
            }
            if let Some(l) = p.location.filter(|l| !matches!(l.kind, LocationKind::InCell(_))) {
                sandbox_radii.push(l.radius as i64);
            }
        }
        // Where a patrol's route comes from: the chain of linked references that
        // starts at its location reference.
        if p.ty == PackageType::Patrol {
            match p.location.map(|l| l.kind) {
                Some(LocationKind::NearReference(Some(start))) if refs.get(start).is_some() => {
                    let (chain, looped) = linked_chain(refs, start);
                    route_points.push(chain.len() as i64);
                    let kind = match (chain.len(), looped) {
                        (1, _) => "near reference: the reference has no linked reference",
                        (_, true) => "near reference: a chain of linked references that loops",
                        (_, false) => "near reference: a chain of linked references that ends",
                    };
                    bump(&mut patrol_routes, kind);
                }
                Some(LocationKind::NearReference(_)) => bump(&mut patrol_routes, "near reference: not a placed reference of the load order"),
                Some(k) => bump(&mut patrol_routes, location_kind_name(&k).to_lowercase()),
                None => bump(&mut patrol_routes, "no location"),
            }
        }
        let mut extra = |on: bool, k: &str| {
            if on {
                bump(&mut extras, k)
            }
        };
        extra(p.idles.is_some(), "idle collection");
        extra(p.idles.as_ref().is_some_and(|c| !c.idles.is_empty()), "idle collection with idles");
        extra(p.idles.as_ref().is_some_and(|c| c.run_in_sequence), "idle collection: run in sequence");
        extra(p.idles.as_ref().is_some_and(|c| c.do_once), "idle collection: do once");
        extra(p.patrol_repeatable == Some(true), "patrol: repeatable");
        extra(p.patrol_repeatable == Some(false), "patrol: not repeatable");
        extra(p.escort_distance.is_some(), "escort distance");
        extra(p.follow_trigger_radius.is_some(), "follow trigger radius");
        extra(p.use_weapon.is_some(), "use weapon data");
        extra(p.dialogue.is_some(), "dialogue data");
        extra(p.dialogue.is_some_and(|d| d.say_to), "dialogue: say to");
        extra(p.dialogue.is_some_and(|d| d.topic.is_some()), "dialogue: with topic");
        extra(p.combat_style.is_some(), "combat style");
        extra(p.events.iter().any(|e| e.idle.is_some()), "begin/end/change idle");
        extra(p.events.iter().any(|e| e.topic.is_some()), "begin/end/change topic");
    }
    // The same tallies from the survey.
    let as_map = |v: &Value| -> BTreeMap<String, u64> {
        v.as_object().map(|o| o.iter().filter_map(|(k, n)| Some((k.clone(), n.as_u64()?))).collect()).unwrap_or_default()
    };
    let as_map2 = |v: &Value| -> BTreeMap<String, BTreeMap<String, u64>> {
        v.as_object().map(|o| o.iter().map(|(k, m)| (k.clone(), as_map(m))).collect()).unwrap_or_default()
    };
    let survey_conditions: u64 = as_map(&survey["conditions"]["run_on"]).values().sum();
    let checks = [
        ("packages", survey["packages"].as_u64() == Some(data.package_defs.len() as u64)),
        ("by_type", as_map(&survey["by_type"]) == by_type),
        ("location_types", as_map2(&survey["location_types"]) == loc),
        ("target_types", as_map2(&survey["target_types"]) == tgt),
        ("scheduled_by_type", as_map(&survey["schedule"]["scheduled_by_type"]) == scheduled),
        ("conditions", survey_conditions == conditions),
    ];
    let runtime: Map<String, Value> = PackageType::ALL.iter().map(|t| (t.name().to_string(), json!(t.runtime_code()))).collect();
    json!({
        "schema": "fnv.ai.packages/1",
        "packages": data.package_defs.len(),
        "by_type": by_type,
        "location_kinds": loc,
        "target_kinds": tgt,
        "scheduled_by_type": scheduled,
        "schedule_shapes": shapes,
        "conditions": conditions,
        "general_flags_by_type": flags,
        "type_flags_by_type": type_flags,
        "object_types": object_types,
        "uses_linked_ref_by_type": linked,
        "sandbox": sandbox,
        "sandbox_radius": spread(&mut sandbox_radii),
        "patrol_routes": patrol_routes,
        "patrol_route_points": spread(&mut route_points),
        "forms_not_in_load_order": unmapped,
        "extras": extras,
        "runtime_type_codes": runtime,
        "agrees_with_survey": checks.iter().map(|(k, ok)| (k.to_string(), json!(ok))).collect::<Map<_, _>>(),
        "all_agree": checks.iter().all(|c| c.1),
        "reader_issues": data.issues.to_json(),
    })
}

/// Distribution summary of a list of numbers.
fn spread(v: &mut [i64]) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    v.sort_unstable();
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    json!({"count": v.len(), "min": v[0], "median": at(0.5), "p90": at(0.9), "max": v[v.len() - 1]})
}

pub(crate) fn find_form(data: &GameData, refs: &RefTable, name: &str) -> Option<FormId> {
    // An Editor ID first: some are valid hex.
    data.by_edid(name).or_else(|| refs.by_edid(name)).or_else(|| crate::parse_formid(name).ok().map(FormId))
}

/// `ai show`: packages in full, and actors with their package lists.
pub fn show(data: &GameData, refs: &RefTable, packages: &[String], actors: &[String]) -> anyhow::Result<Value> {
    let mut out_p = Vec::new();
    for name in packages {
        let id = find_form(data, refs, name).ok_or_else(|| anyhow::anyhow!("no form named {name}"))?;
        let p = data.package_defs.get(&id).ok_or_else(|| anyhow::anyhow!("{name} ({id}) is not a package"))?;
        out_p.push(package_json(data, refs, id, p));
    }
    let mut out_a = Vec::new();
    for name in actors {
        let id = find_form(data, refs, name).ok_or_else(|| anyhow::anyhow!("no form named {name}"))?;
        let placed = refs.get(id);
        let base = placed.map(|r| r.base).unwrap_or(id);
        if !data.actors.contains_key(&base) {
            anyhow::bail!("{name} ({id}) is not an actor or a placed actor");
        }
        let linked = placed.and_then(|r| r.linked);
        let list = data.base_package_list(base);
        let rows: Vec<Value> = list
            .iter()
            .map(|p| {
                let mut v = package_brief(data, refs, *p);
                // A package that needs a linked reference is skipped for an actor
                // without one (packages.md §3); only known for a placed actor.
                if placed.is_some() && linked.is_none() && data.package_defs.get(p).is_some_and(|d| d.uses_linked_ref()) {
                    v["needs_linked_ref_the_actor_lacks"] = json!(true);
                }
                v
            })
            .collect();
        out_a.push(json!({
            "reference": placed.map(|r| named(data, refs, r.id)),
            "base": named(data, refs, base),
            "template_supplies_packages": data.actors.get(&base).is_some_and(|a| a.template_packages && a.template.is_some()),
            "linked_ref": opt_named(data, refs, linked),
            "packages": rows,
        }));
    }
    Ok(json!({"schema": "fnv.ai.show/1", "packages": out_p, "actors": out_a}))
}

/// `ai observed`: what the game reported for packages we can read. Per sample of an
/// actor: its current package form, `GetCurrentAIPackage` and the package target.
pub fn observed(lo: &LoadOrder, data: &GameData, forms: &FormStore, refs: &RefTable, files: &[PathBuf]) -> anyhow::Result<Value> {
    // (record type, observed code) -> samples and the packages seen.
    let mut codes: BTreeMap<(PackageType, i64), (u64, BTreeSet<FormId>)> = BTreeMap::new();
    let mut runtime: BTreeMap<String, u64> = BTreeMap::new();
    let mut no_package: BTreeMap<String, u64> = BTreeMap::new();
    let mut other_plugin = 0u64;
    let mut not_a_package = 0u64;
    let mut samples = 0u64;
    // (actor, package) pairs and whether the package is in the actor's list.
    let mut pairs: BTreeMap<(FormId, FormId), u64> = BTreeMap::new();
    let mut target_check: BTreeMap<String, u64> = BTreeMap::new();
    let mut target_examples: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut days = (f64::INFINITY, f64::NEG_INFINITY);
    let mut seen_rows: Vec<Seen> = Vec::new();
    observer::read(lo, files, |row| {
        samples += 1;
        if let Some(d) = row.header_num("days") {
            days = (days.0.min(d), days.1.max(d));
        }
        let code = row.num("package_type").map(|c| c as i64).unwrap_or(-1);
        let pkg = match row.form("package") {
            Form::None => return bump(&mut no_package, code.to_string()),
            Form::Runtime => return bump(&mut runtime, code.to_string()),
            Form::Unmapped => return other_plugin += 1,
            Form::Mapped(g) => FormId(g),
        };
        let Some(def) = data.package_defs.get(&pkg) else { return not_a_package += 1 };
        let e = codes.entry((def.ty, code)).or_default();
        e.0 += 1;
        e.1.insert(pkg);
        let Some(actor) = row.id("id").map(FormId) else { return };
        *pairs.entry((actor, pkg)).or_default() += 1;
        if let (Some(hour), Some(days), Some(scale)) = (row.header_num("hour"), row.header_num("days"), row.header_num("timescale")) {
            seen_rows.push(Seen { file: row.file, actor, package: pkg, hour, days, timescale: scale });
        }
        // The package target the game reports against the record's first target.
        let seen = row.id("package_target").map(FormId);
        let expect = match def.target.map(|t| t.kind) {
            Some(TargetKind::Reference(f)) => Some(("specific reference", f)),
            Some(TargetKind::LinkedReference) => Some(("linked reference", refs.get(actor).and_then(|r| r.linked))),
            Some(TargetKind::ObjectId(_)) => Some(("object id", None)),
            Some(TargetKind::ObjectType(_)) => Some(("object type", None)),
            _ => None,
        };
        let key = match (expect, seen) {
            (None, None) => "no target in the record: none reported".to_string(),
            (None, Some(_)) => "no target in the record: one reported".to_string(),
            (Some((k, Some(want))), Some(got)) if want == got => format!("{k}: reported target is the record's"),
            (Some((k, Some(_))), Some(_)) => format!("{k}: reported target differs"),
            (Some((k, _)), None) => format!("{k}: none reported"),
            (Some((k, None)), Some(_)) => format!("{k}: one reported"),
        };
        let ex = target_examples.entry(key.clone()).or_default();
        if ex.len() < 6 {
            ex.insert(data.name(pkg));
        }
        bump(&mut target_check, key);
    })?;
    // Record type against run-time code.
    let mut by_type: BTreeMap<String, Value> = BTreeMap::new();
    let mut disagreements = Vec::new();
    let mut confirmed: BTreeSet<String> = BTreeSet::new();
    for ((ty, code), (n, pkgs)) in &codes {
        let expected = ty.runtime_code().map(|c| c as i64);
        let entry = by_type.entry(type_name(*ty)).or_insert_with(|| json!({"expected_code": expected, "observed": {}}));
        entry["observed"][code.to_string()] = json!({
            "samples": n,
            "packages": pkgs.len(),
            "examples": pkgs.iter().take(4).map(|p| data.name(*p)).collect::<Vec<_>>(),
        });
        if expected == Some(*code) {
            confirmed.insert(type_name(*ty));
        } else {
            disagreements.push(json!({"type": type_name(*ty), "expected_code": expected, "observed_code": code, "samples": n,
                                      "examples": pkgs.iter().take(4).map(|p| data.name(*p)).collect::<Vec<_>>()}));
        }
    }
    // Current package against the actor's list.
    let (mut in_list, mut not_in_list) = ((0u64, 0u64), (0u64, 0u64));
    let mut outside: BTreeMap<String, u64> = BTreeMap::new();
    for ((actor, pkg), n) in &pairs {
        let listed = refs.get(*actor).is_some_and(|r| data.base_package_list(r.base).contains(pkg));
        let c = if listed { &mut in_list } else { &mut not_in_list };
        c.0 += 1;
        c.1 += n;
        if !listed {
            let t = data.package_defs.get(pkg).map(|d| type_name(d.ty)).unwrap_or_default();
            *outside.entry(format!("{} {} ({t})", data.name(*actor), data.name(*pkg))).or_default() += n;
        }
    }
    let mut outside: Vec<(String, u64)> = outside.into_iter().collect();
    outside.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(json!({
        "schema": "fnv.ai.observed/1",
        "files": files.len(),
        "actor_samples": samples,
        "game_days_passed": if days.0.is_finite() { json!([days.0, days.1]) } else { Value::Null },
        "type_codes": by_type,
        "types_confirmed": confirmed,
        "type_code_disagreements": disagreements,
        "runtime_packages_by_code": runtime,
        "no_package_by_code": no_package,
        "package_of_a_plugin_not_loaded": other_plugin,
        "current_form_not_a_package": not_a_package,
        "package_lists": {
            "pairs_in_the_actors_list": in_list.0, "samples_in_list": in_list.1,
            "pairs_outside_the_list": not_in_list.0, "samples_outside": not_in_list.1,
            "outside": outside.iter().take(25).map(|(k, n)| json!([k, n])).collect::<Vec<_>>(),
        },
        "targets": target_check,
        "target_examples": target_examples,
        "schedules": schedules(data, refs, &seen_rows),
        "choice_replay": crate::ai_choice::replay(data, forms, refs, &seen_rows)?,
    }))
}

/// What the samples say about schedules, list order and the linked-reference rule
/// (packages.md §4). Day-of-week, month and date parts of schedules are not checked
/// (the samples carry no calendar date).
fn schedules(data: &GameData, refs: &RefTable, rows: &[Seen]) -> Value {
    // Per scheduled package: where in its window the samples fall.
    struct Window {
        samples: u64,
        inside: u64,
        min: f64,
        max: f64,
    }
    let mut windows: BTreeMap<FormId, Window> = BTreeMap::new();
    let mut order: BTreeMap<String, u64> = BTreeMap::new();
    let mut order_examples: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut linked_skips: BTreeMap<String, u64> = BTreeMap::new();
    let mut linked_examples: BTreeSet<String> = BTreeSet::new();
    let mut lists: BTreeMap<FormId, (Vec<FormId>, bool)> = BTreeMap::new();
    // Package changes per (file, actor), in sample order.
    let mut last: BTreeMap<(usize, FormId), (FormId, f64)> = BTreeMap::new();
    let mut start_delays: Vec<(f64, f64, String)> = Vec::new();
    let mut end_delays: Vec<(f64, f64, String)> = Vec::new();
    for r in rows {
        let Some(def) = data.package_defs.get(&r.package) else { continue };
        if let Some(h) = def.schedule.hours_in(r.hour) {
            let w = windows.entry(r.package).or_insert(Window { samples: 0, inside: 0, min: f64::INFINITY, max: f64::NEG_INFINITY });
            w.samples += 1;
            w.inside += (h < def.schedule.duration as f64) as u64;
            w.min = w.min.min(h);
            w.max = w.max.max(h);
        }
        let (list, has_link) = lists.entry(r.actor).or_insert_with(|| {
            let placed = refs.get(r.actor);
            (placed.map(|p| data.base_package_list(p.base)).unwrap_or_default(), placed.is_some_and(|p| p.linked.is_some()))
        });
        if let Some(at) = list.iter().position(|p| *p == r.package) {
            // Earlier packages that nothing we can see keeps from running: no
            // conditions, in their window, and not short of a linked reference.
            let mut blocked_by: Option<FormId> = None;
            for e in &list[..at] {
                let Some(d) = data.package_defs.get(e) else { continue };
                if !d.conditions.is_empty() || !d.schedule.in_window(r.hour) {
                    continue;
                }
                if d.uses_linked_ref() && !*has_link {
                    bump(&mut linked_skips, type_name(d.ty));
                    if linked_examples.len() < 8 {
                        linked_examples.insert(format!("{} ran {} past {}", data.name(r.actor), data.name(r.package), data.name(*e)));
                    }
                    continue;
                }
                blocked_by.get_or_insert(*e);
            }
            let own = def.schedule.in_window(r.hour);
            let key = match (blocked_by, own) {
                (None, true) => "in its window, no earlier unconditional package in its window".to_string(),
                (None, false) => "outside its own window".to_string(),
                (Some(_), true) => "an earlier unconditional package was in its window".to_string(),
                (Some(_), false) => "outside its own window, and an earlier unconditional package was in its window".to_string(),
            };
            if key != "in its window, no earlier unconditional package in its window" {
                let ex = order_examples.entry(key.clone()).or_default();
                if ex.len() < 8 {
                    let earlier = blocked_by.map(|e| format!(" (earlier: {})", data.name(e))).unwrap_or_default();
                    ex.insert(format!("{} {} at {:.2}{earlier}", data.name(r.actor), data.name(r.package), r.hour));
                }
            }
            bump(&mut order, key);
        }
        // Changes of package near a schedule boundary.
        let real = |game_hours: f64| game_hours * 3600.0 / r.timescale.max(1.0);
        if let Some((prev, _)) = last.insert((r.file, r.actor), (r.package, r.hour)).filter(|(p, _)| *p != r.package) {
            if let Some(h) = def.schedule.hours_in(r.hour).filter(|h| *h < 3.0) {
                start_delays.push((h * 60.0, real(h), format!("{} to {}", data.name(r.actor), data.name(r.package))));
            }
            if let Some(pd) = data.package_defs.get(&prev) {
                if let Some(h) = pd.schedule.hours_in(r.hour).map(|h| h - pd.schedule.duration as f64).filter(|h| (0.0..3.0).contains(h)) {
                    end_delays.push((h * 60.0, real(h), format!("{} from {}", data.name(r.actor), data.name(prev))));
                }
            }
        }
    }
    let delays = |v: &mut Vec<(f64, f64, String)>| {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = |q: f64| v.get(((v.len().max(1) - 1) as f64 * q).round() as usize).map(|x| json!({"game_minutes": x.0, "real_seconds": x.1}));
        json!({
            "count": v.len(), "min": at(0.0), "median": at(0.5), "p90": at(0.9), "max": at(1.0),
            "all": v.iter().take(60).map(|x| json!([format!("{:.1}", x.0), format!("{:.1}", x.1), x.2])).collect::<Vec<_>>(),
        })
    };
    json!({
        "rule": "a package with a starting hour may run from that hour for its duration; one without, at any time (Schedule::in_window)",
        "windows": windows.iter().map(|(p, w)| {
            let s = &data.package_defs[p].schedule;
            json!({"package": data.name(*p), "start_hour": s.hour, "duration_hours": s.duration, "day_of_week": s.day_of_week,
                   "samples": w.samples, "inside": w.inside, "outside": w.samples - w.inside,
                   "hours_after_start": [format!("{:.2}", w.min), format!("{:.2}", w.max)]})
        }).collect::<Vec<_>>(),
        "list_order": order,
        "list_order_examples": order_examples,
        "skipped_for_no_linked_ref_by_type": linked_skips,
        "skipped_for_no_linked_ref_examples": linked_examples,
        "switch_to_a_scheduled_package_after_its_start": delays(&mut start_delays),
        "switch_away_after_the_window_end": delays(&mut end_delays),
    })
}
