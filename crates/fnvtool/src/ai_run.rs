//! `fnvtool ai run`: actors carrying their packages out (`fnv_ai`) over the load order's
//! navmeshes, headless, for some game hours; optionally started from AI observer samples
//! and compared with them. JSON schema: docs/spec-notes/packages.md §5.

use crate::observer::{self, Form};
use fnv_ai::{Activity, Ai, AiConfig};
use fnv_esm::{LoadOrder, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_nav::{NavGraph, NavRecords, Space};
use fnv_sim::{clock, FormId, FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct RunArgs {
    pub actors: Vec<String>,
    /// `ACTOR=PACKAGE`: the actor runs this package as a script package.
    pub force: Vec<String>,
    /// `QUEST:INDEX=VALUE`: a quest script variable set before the run (quest state the
    /// run does not have, e.g. the Strip's VStreetFluffNPC variable 52 that every crowd
    /// package's condition reads).
    pub quest_vars: Vec<String>,
    pub start_hour: Option<f64>,
    pub hours: f64,
    /// Seconds per step.
    pub step: f64,
    pub timescale: f64,
    pub samples: Vec<PathBuf>,
}

fn name(data: &GameData, refs: &RefTable, id: FormId) -> String {
    data.edid_of.get(&id).cloned().or_else(|| refs.get(id).and_then(|r| r.edid.clone())).unwrap_or_else(|| id.to_string())
}

fn space_name(data: &GameData, refs: &RefTable, s: Space) -> String {
    match s {
        Space::World(w) => name(data, refs, FormId(w)),
        Space::Interior(c) => name(data, refs, FormId(c)),
    }
}

fn set_global(g: &mut Game, edid: &str, v: f64) -> anyhow::Result<()> {
    let (id, _) = g.world.forms.global_by_edid(edid).ok_or_else(|| anyhow::anyhow!("the load order has no {edid} global"))?;
    g.world.set_global(id, v as f32).map_err(|e| anyhow::anyhow!("{e}"))
}

fn force(game: &mut Game, forced: &[(FormId, FormId)]) {
    for &(actor, package) in forced {
        game.extra.entry(actor).or_default().script_package = Some(package);
    }
}

/// Set quest script variables (`--quest-var`).
fn set_quest_vars(game: &mut Game, vars: &[(FormId, u16, f64)]) {
    for &(quest, index, value) in vars {
        let script = game.data.form_script.get(&quest).copied();
        game.store(fnv_game::game::InstKey::Quest { quest }, script, index, fnv_script::vm::Value::Num(value), "ai run", false);
    }
}

fn parse_quest_vars(args: &RunArgs, data: &GameData, find: &dyn Fn(&str) -> Option<FormId>) -> anyhow::Result<Vec<(FormId, u16, f64)>> {
    let mut out = Vec::new();
    for v in &args.quest_vars {
        let bad = || anyhow::anyhow!("--quest-var takes QUEST:INDEX=VALUE, got {v}");
        let (q, rest) = v.split_once(':').ok_or_else(bad)?;
        let (i, x) = rest.split_once('=').ok_or_else(bad)?;
        let quest = find(q).filter(|q| data.quests.contains_key(q)).ok_or_else(|| anyhow::anyhow!("{q} is not a quest"))?;
        out.push((quest, i.trim().parse().map_err(|_| bad())?, x.trim().parse().map_err(|_| bad())?));
    }
    Ok(out)
}

struct Sim<'a> {
    game: Game<'a>,
    ai: Ai,
    step: f64,
    timescale: f64,
}

impl<'a> Sim<'a> {
    fn new(data: &'a GameData, forms: &'a FormStore, refs: &'a RefTable, step: f64, timescale: f64) -> Sim<'a> {
        let cfg = GameConfig { frame_seconds: step, package_interval: Some(10.0), ..GameConfig::default() };
        Sim { game: Game::new(data, World::new(forms, refs, SimConfig::default()), cfg), ai: Ai::new(AiConfig::default()), step, timescale }
    }

    /// Advance by `game_hours`: the game's tick (the periodic package choice), the
    /// calendar, then the actors.
    fn advance(&mut self, nav: &NavGraph, game_hours: f64) -> anyhow::Result<()> {
        let mut left = game_hours * 3600.0 / self.timescale;
        while left > 1e-9 {
            let dt = left.min(self.step);
            self.game.cfg.frame_seconds = dt;
            self.game.tick();
            self.game.world.advance_hours(dt * self.timescale / 3600.0).map_err(|e| anyhow::anyhow!("{e}"))?;
            self.ai.update(&mut self.game, nav, dt as f32);
            self.game.events.clear();
            left -= dt;
        }
        Ok(())
    }

    fn state(&self, data: &GameData, refs: &RefTable, actor: FormId) -> Value {
        let a = &self.ai.agents[&actor];
        let package = self.game.extra.get(&actor).and_then(|e| e.current_package);
        json!({
            "space": space_name(data, refs, a.walker.space),
            "pos": a.walker.feet.map(|v| (v * 10.0).round() / 10.0),
            "package": package.map(|p| data.name(p)),
            "activity": a.activity,
            "using": a.using.map(|f| name(data, refs, f)),
            "sandbox": a.sandbox.as_ref().map(|s| serde_json::to_value(s.doing).unwrap_or(Value::Null)),
        })
    }
}

fn events_json(data: &GameData, refs: &RefTable, ai: &Ai, actor: FormId, cap: usize) -> (Vec<Value>, BTreeMap<String, u64>) {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut out = Vec::new();
    for e in ai.events.iter().filter(|e| e.actor == actor) {
        let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
        if let Some(k) = v.get("kind").and_then(Value::as_str) {
            *counts.entry(k.to_string()).or_default() += 1;
        }
        // Names instead of FormIDs.
        for key in ["package", "door", "to_door", "patrol_point", "furniture", "object"] {
            if let Some(f) = v.get(key).and_then(Value::as_str).and_then(|s| crate::parse_formid(s).ok()) {
                v[key] = json!(name(data, refs, FormId(f)));
            }
        }
        if let Some(d) = v.get("doors").and_then(Value::as_array).cloned() {
            v["doors"] = d.iter().filter_map(|x| x.as_str()).filter_map(|s| crate::parse_formid(s).ok()).map(|f| json!(name(data, refs, FormId(f)))).collect();
        }
        if let Some(obj) = v.as_object_mut() {
            obj.remove("actor");
            obj.remove("time");
            if let Some(s) = e_space(&e.event) {
                obj.insert("space".into(), json!(space_name(data, refs, s)));
            }
        }
        if out.len() < cap {
            out.push(v);
        }
    }
    (out, counts)
}

fn e_space(e: &fnv_ai::AiEvent) -> Option<Space> {
    match e {
        fnv_ai::AiEvent::Trip { space, .. } => Some(*space),
        _ => None,
    }
}

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, args: &RunArgs, find: &dyn Fn(&str) -> Option<FormId>) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let ix = fnv_world::WorldIndex::build(lo, schema);
    let nav = NavGraph::build(lo, schema, &ix, &NavRecords::read(lo));
    let mut actors = Vec::new();
    for n in &args.actors {
        actors.push(find(n).filter(|a| refs.get(*a).is_some()).ok_or_else(|| anyhow::anyhow!("{n} is not a placed reference"))?);
    }
    // Packages given to actors as a script would (`AddScriptPackage`): they override the
    // list, which stands in for quest state the run does not have.
    let mut forced = Vec::new();
    for f in &args.force {
        let (a, p) = f.split_once('=').ok_or_else(|| anyhow::anyhow!("--force takes ACTOR=PACKAGE, got {f}"))?;
        let actor = find(a).filter(|x| refs.get(*x).is_some()).ok_or_else(|| anyhow::anyhow!("{a} is not a placed reference"))?;
        let package = find(p).filter(|x| data.package_defs.contains_key(x)).ok_or_else(|| anyhow::anyhow!("{p} is not a package"))?;
        forced.push((actor, package));
    }
    let quest_vars = parse_quest_vars(args, data, find)?;
    if !args.samples.is_empty() {
        return compare(&Loaded { lo, data, forms: &forms, refs, nav: &nav }, args, &actors, &forced, &quest_vars);
    }
    let mut sim = Sim::new(data, &forms, refs, args.step, args.timescale);
    force(&mut sim.game, &forced);
    set_quest_vars(&mut sim.game, &quest_vars);
    if let Some(h) = args.start_hour {
        set_global(&mut sim.game, clock::GAME_HOUR, h)?;
    }
    for &a in &actors {
        sim.ai.add(&sim.game, a).map_err(anyhow::Error::msg)?;
    }
    let start = sim.game.world.calendar().map_err(|e| anyhow::anyhow!("{e}"))?;
    // The state at the start and after every game hour.
    let mut hourly: BTreeMap<FormId, Vec<Value>> = BTreeMap::new();
    let whole = args.hours.ceil() as usize;
    for h in 0..=whole {
        if h > 0 {
            sim.advance(&nav, (args.hours - (h - 1) as f64).min(1.0))?;
        } else {
            // The first choice, so that hour 0 shows a package.
            sim.advance(&nav, 0.0)?;
            sim.ai.update(&mut sim.game, &nav, 0.0);
        }
        let hour = sim.game.world.calendar().map(|c| c.hour).unwrap_or(0.0);
        for &a in &actors {
            let mut v = sim.state(data, refs, a);
            v["hour"] = json!((hour * 100.0).round() / 100.0);
            hourly.entry(a).or_default().push(v);
        }
    }
    let out: Vec<Value> = actors
        .iter()
        .map(|&a| {
            let (events, counts) = events_json(data, refs, &sim.ai, a, 400);
            json!({
                "actor": name(data, refs, a),
                "reference": a.to_string(),
                "travelled": sim.ai.agents[&a].walker.travelled,
                "event_counts": counts,
                "hourly": hourly.get(&a),
                "events": events,
            })
        })
        .collect();
    Ok(json!({
        "schema": "fnv.ai.run/1",
        "state": "the authored game: no quest started; the actors start where they are placed",
        "start": {"hour": start.hour, "days_passed": start.days_passed},
        "hours": args.hours, "step_seconds": args.step, "timescale": args.timescale,
        "nav": {"triangles": nav.tris.len(), "navmeshes": nav.meshes.len()},
        "actors": out,
    }))
}

/// What a run works on.
#[derive(Clone, Copy)]
struct Loaded<'a> {
    lo: &'a LoadOrder<'a>,
    data: &'a GameData,
    forms: &'a FormStore,
    refs: &'a RefTable,
    nav: &'a NavGraph,
}

/// One observed sample of an actor.
struct Obs {
    days: f64,
    hour: f64,
    cell: FormId,
    pos: [f32; 3],
    package: Option<FormId>,
    /// The capture saw it enabled (GetDisabled 0).
    enabled: bool,
}

/// Start each actor where a capture first saw it, run the capture's hours, and compare
/// at every sample: the cell, the distance when the cell is the same, the package.
fn compare(w: &Loaded, args: &RunArgs, actors: &[FormId], forced: &[(FormId, FormId)], quest_vars: &[(FormId, u16, f64)]) -> anyhow::Result<Value> {
    let Loaded { lo, data, forms, refs, nav } = *w;
    // Per file, per actor: the samples in order.
    let mut seen: BTreeMap<usize, BTreeMap<FormId, Vec<Obs>>> = BTreeMap::new();
    observer::read(lo, &args.samples, |row| {
        let Some(actor) = row.id("id").map(FormId).filter(|a| actors.contains(a)) else { return };
        let (Some(cell), Some(x), Some(y), Some(z)) = (row.id("cell"), row.num("x"), row.num("y"), row.num("z")) else { return };
        let (Some(days), Some(hour)) = (row.header_num("days"), row.header_num("hour")) else { return };
        let package = match row.form("package") {
            Form::Mapped(p) => Some(FormId(p)),
            _ => None,
        };
        let enabled = row.num("disabled") == Some(0.0);
        seen.entry(row.file).or_default().entry(actor).or_default().push(Obs { days, hour, cell: FormId(cell), pos: [x, y, z], package, enabled });
    })?;
    let mut files = Vec::new();
    for (fi, by_actor) in &seen {
        let mut sim = Sim::new(data, forms, refs, args.step, args.timescale);
        let Some(first) = by_actor.values().filter_map(|v| v.first()).min_by(|a, b| a.days.total_cmp(&b.days)) else { continue };
        set_global(&mut sim.game, clock::GAME_HOUR, first.hour)?;
        set_global(&mut sim.game, clock::GAME_DAYS_PASSED, first.days)?;
        let mut days = first.days;
        force(&mut sim.game, forced);
        set_quest_vars(&mut sim.game, quest_vars);
        // The sample times of the file, in order; an actor joins the run where its
        // first sample saw it, when that sample comes.
        let mut times: Vec<f64> = by_actor.values().flatten().map(|o| o.days).collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        let mut cursor: BTreeMap<FormId, usize> = BTreeMap::new();
        #[derive(Default)]
        struct Tally {
            samples: u64,
            same_cell: u64,
            same_package: u64,
            same_package_same_cell: u64,
            dist: Vec<f32>,
            sim_only: BTreeMap<String, u64>,
        }
        let mut tally: BTreeMap<FormId, Tally> = BTreeMap::new();
        for &time in &times {
            sim.advance(nav, (time - days) * 24.0)?;
            days = time;
            let mut joined = false;
            for (&a, obs) in by_actor {
                if !sim.ai.agents.contains_key(&a) && obs[0].days <= time {
                    // An actor a trigger or a quest enabled in the game (the Strip's crowd
                    // is placed disabled): enabled as the capture saw it.
                    if obs[0].enabled {
                        sim.game.world.set_enabled(a, true).map_err(|e| anyhow::anyhow!("{e}"))?;
                    }
                    sim.game.set_actor_place(a, Some(obs[0].cell), obs[0].pos, 0.0).map_err(anyhow::Error::msg)?;
                    sim.ai.add(&sim.game, a).map_err(anyhow::Error::msg)?;
                    joined = true;
                }
            }
            if joined {
                // The first choice of the newcomers.
                sim.ai.update(&mut sim.game, nav, 0.0);
            }
            for (&a, obs) in by_actor {
                let i = cursor.entry(a).or_default();
                let Some(o) = obs.get(*i).filter(|o| o.days <= time) else { continue };
                *i += 1;
                let agent = &sim.ai.agents[&a];
                let t = tally.entry(a).or_default();
                let sim_cell = fnv_ai::goal::cell_of_place(refs, &fnv_nav::Place { space: agent.walker.space, pos: agent.walker.feet });
                let sim_package = sim.game.extra.get(&a).and_then(|e| e.current_package);
                let same_cell = fnv_ai::goal::space_of_cell(refs, o.cell) == agent.walker.space && (refs.world_of(o.cell).is_none() || sim_cell == Some(o.cell) || {
                    // Outdoors, neighbouring cells count when the two are close.
                    (o.pos[0] - agent.walker.feet[0]).hypot(o.pos[1] - agent.walker.feet[1]) < 1024.0
                });
                let same_package = sim_package == o.package;
                t.samples += 1;
                t.same_cell += same_cell as u64;
                t.same_package += same_package as u64;
                t.same_package_same_cell += (same_package && same_cell) as u64;
                if same_cell {
                    t.dist.push((o.pos[0] - agent.walker.feet[0]).hypot(o.pos[1] - agent.walker.feet[1]));
                }
                if !same_package {
                    let key = format!("{} where the game ran {}", sim_package.map(|p| data.name(p)).unwrap_or_else(|| "none".into()), o.package.map(|p| data.name(p)).unwrap_or_else(|| "a run-time package or none".into()));
                    *t.sim_only.entry(key).or_default() += 1;
                }
            }
        }
        let rows: Vec<Value> = tally
            .iter_mut()
            .map(|(a, t)| {
                t.dist.sort_by(f32::total_cmp);
                let at = |q: f32| t.dist.get(((t.dist.len().max(1) - 1) as f32 * q).round() as usize).copied();
                let mut diffs: Vec<(String, u64)> = t.sim_only.iter().map(|(k, n)| (k.clone(), *n)).collect();
                diffs.sort_by_key(|x| std::cmp::Reverse(x.1));
                let (_, counts) = events_json(data, refs, &sim.ai, *a, 0);
                json!({
                    "actor": name(data, refs, *a),
                    "samples": t.samples,
                    "same_package": t.same_package,
                    "same_cell": t.same_cell,
                    "same_cell_among_same_package": if t.same_package > 0 { json!(t.same_package_same_cell as f64 / t.same_package as f64) } else { Value::Null },
                    "distance_when_in_the_same_cell": {"median": at(0.5), "p90": at(0.9), "max": at(1.0)},
                    "package_differences": diffs.iter().take(6).map(|(k, n)| json!([k, n])).collect::<Vec<_>>(),
                    "event_counts": counts,
                    "end_activity": if sim.ai.agents[a].activity == Activity::Travelling { "travelling" } else { "not travelling" },
                })
            })
            .collect();
        files.push(json!({
            "file": args.samples.get(*fi).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()),
            "game_hours": (days - first.days) * 24.0,
            "actors": rows,
        }));
    }
    Ok(json!({
        "schema": "fnv.ai.run/1",
        "state": "no quest started (but the --quest-var values); each actor starts where its capture first saw it, at the capture's hour, enabled when the capture saw it so",
        "quest_vars": args.quest_vars,
        "compared": "at every sample: the package, the cell (outdoors, also within 1,024 units), and the distance when in the same cell",
        "step_seconds": args.step, "timescale": args.timescale,
        "files": files,
    }))
}

/// `ai reach`: for every placed actor and every package of its list, whether the actor
/// can get from where it is placed to where the package sends it, over the navmeshes
/// and through doors (any number of interiors and worldspaces).
pub fn reach(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable) -> anyhow::Result<Value> {
    use fnv_ai::goal::{self, Goal};
    use fnv_nav::{PathError, PathParams, Place};
    let stay_places = |g: &Goal| -> Vec<Place> {
        match g {
            Goal::Near { place, .. } => vec![*place],
            Goal::Inside { at, .. } => vec![*at],
            _ => Vec::new(),
        }
    };
    let forms = FormStore::build(lo, schema);
    let ix = fnv_world::WorldIndex::build(lo, schema);
    let nav = NavGraph::build(lo, schema, &ix, &NavRecords::read(lo));
    let mut game = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let open = PathParams::default();
    let any_key = PathParams { open_locked: true, ..PathParams::default() };
    // A creature flagged "Can't Open Doors" uses no door (docs/spec-notes/packages.md §7.1).
    let shy = PathParams { opens_doors: false, ..PathParams::default() };
    let mut verdicts: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_type: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut doors_used: BTreeMap<String, u64> = BTreeMap::new();
    let mut longest: Vec<(usize, String)> = Vec::new();
    let (mut actors, mut with_packages, mut pairs) = (0u64, 0u64, 0u64);
    let ids: Vec<FormId> = refs.refs.iter().filter(|(_, p)| matches!(&p.sig, b"ACHR" | b"ACRE")).map(|(id, _)| *id).collect();
    for actor in ids {
        actors += 1;
        let placed = &refs.refs[&actor];
        let list = data.base_package_list(placed.base, actor);
        if list.is_empty() {
            continue;
        }
        with_packages += 1;
        let from = goal::editor_place(refs, actor);
        let mut seen = std::collections::BTreeSet::new();
        for p in list {
            let Some(def) = data.package_defs.get(&p).filter(|_| seen.insert(p)) else { continue };
            pairs += 1;
            let ty = def.ty.name().to_string();
            if def.uses_linked_ref() && placed.linked.is_none() {
                // Not a failure: the choice never takes such a package (packages.md §4).
                *verdicts.entry("never chosen: needs a linked reference the actor lacks".into()).or_default() += 1;
                *by_type.entry(ty).or_default().entry("never chosen (no linked reference)".into()).or_default() += 1;
                continue;
            }
            // The places the package needs the actor to get to.
            let targets: Vec<Place> = match goal::resolve(&game, &nav, actor, def) {
                Goal::Here => Vec::new(),
                Goal::Near { place, .. } => vec![place],
                Goal::Inside { at, .. } => vec![at],
                Goal::Route(r) => r.points.iter().map(|x| x.1).collect(),
                // A Dialogue package: the wait location is judged; the way to whom the
                // actor talks to (the player, mostly) depends on where that one is.
                Goal::Talk(t) => match *t.wait {
                    Goal::Near { place, .. } => vec![place],
                    Goal::Inside { at, .. } => vec![at],
                    _ => {
                        *verdicts.entry("talks: goes up to its target from where it is".into()).or_default() += 1;
                        *by_type.entry(ty).or_default().entry("goes up to its target".into()).or_default() += 1;
                        continue;
                    }
                },
                // A Follow or Accompany package: where it goes is where its target is.
                Goal::Follow(_) => {
                    *verdicts.entry("follows: goes after its target from where it is".into()).or_default() += 1;
                    *by_type.entry(ty).or_default().entry("goes after its target".into()).or_default() += 1;
                    continue;
                }
                // The five kinds with places of their own: where they go first is judged
                // (the destination, the search location, the wait location); a Flee
                // without a destination runs from wherever its threat is.
                Goal::Escort(e) => stay_places(&e.to),
                Goal::Find(f) => stay_places(&f.search),
                Goal::UseAt(u) => stay_places(&u.at),
                Goal::Ambush(a) => stay_places(&a.wait),
                Goal::Flee(f) => f.to.as_deref().map(stay_places).unwrap_or_default(),
                Goal::NotCarriedOut(why) => {
                    *verdicts.entry(format!("not carried out yet: {why}")).or_default() += 1;
                    *by_type.entry(ty).or_default().entry("not carried out yet".into()).or_default() += 1;
                    continue;
                }
                Goal::Missing(why) => {
                    let k = format!("nowhere to go: {why}");
                    let ex = examples.entry(k.clone()).or_default();
                    if ex.len() < 8 {
                        ex.push(format!("{} {}", name(data, refs, actor), data.name(p)));
                    }
                    *verdicts.entry(k).or_default() += 1;
                    *by_type.entry(ty).or_default().entry("nowhere to go".into()).or_default() += 1;
                    continue;
                }
            };
            let verdict = match from {
                None => "the actor is in no cell".to_string(),
                Some(_) if targets.is_empty() => "stays where it is".to_string(),
                Some(from) => {
                    // Every target from the actor's place (a patrol: every point).
                    let mut worst = 0u8;
                    let mut why = String::new();
                    let mut most_doors = 0usize;
                    let cant_open = game.cant_open_doors(actor);
                    for to in &targets {
                        let (rank, text) = match nav.door_route(from, *to, &open) {
                            Ok(_) if cant_open && nav.door_route(from, *to, &shy).is_err() => (2, "only through a door the creature cannot open (Can't Open Doors)".to_string()),
                            Ok(doors) => {
                                most_doors = most_doors.max(doors.len());
                                (0, String::new())
                            }
                            Err(PathError::NoTriangle { which, .. }) => (4, format!("no navmesh near the {which}")),
                            Err(PathError::TooFar { .. }) => (3, "search limit".to_string()),
                            Err(PathError::Unreachable { .. }) => match nav.door_route(from, *to, &any_key) {
                                Ok(_) => {
                                    // Locked doors on the way: does the actor carry a key?
                                    let mut keyed = PathParams::default();
                                    if let Ok(s) = game.world.get(actor) {
                                        keyed.keys.extend(s.inventory.iter().flat_map(|inv| inv.iter()).map(|(item, _)| item.0));
                                    }
                                    if nav.door_route(from, *to, &keyed).is_ok() {
                                        (1, "reachable with a key the actor carries".to_string())
                                    } else {
                                        (2, "only through a locked door the actor has no key for".to_string())
                                    }
                                }
                                Err(_) => (5, "no way there (no chain of doors joins the two places)".to_string()),
                            },
                        };
                        if rank > worst {
                            worst = rank;
                            why = text;
                        }
                    }
                    if worst == 0 {
                        let k = match most_doors {
                            0 => "no door",
                            1 => "1 door",
                            2 => "2 doors",
                            3 => "3 doors",
                            _ => "4 doors or more",
                        };
                        *doors_used.entry(k.to_string()).or_default() += 1;
                        if most_doors >= 3 {
                            longest.push((most_doors, format!("{} {} ({most_doors} doors)", name(data, refs, actor), data.name(p))));
                        }
                        "reachable".to_string()
                    } else {
                        why
                    }
                }
            };
            if verdict != "reachable" && verdict != "stays where it is" {
                let ex = examples.entry(verdict.clone()).or_default();
                if ex.len() < 12 {
                    ex.push(format!("{} {}", name(data, refs, actor), data.name(p)));
                }
            }
            *by_type.entry(ty).or_default().entry(verdict.clone()).or_default() += 1;
            *verdicts.entry(verdict).or_default() += 1;
        }
    }
    longest.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    Ok(json!({
        "schema": "fnv.ai.reach/1",
        "what": "each placed actor, each package of its list: can it get from where it is placed to where the package sends it (doors as authored; quest state and schedules ignored)",
        "placed_actors": actors,
        "actors_with_packages": with_packages,
        "actor_package_pairs": pairs,
        "verdicts": verdicts,
        "doors_on_reachable_routes": doors_used,
        "most_doors": longest.iter().take(12).map(|x| x.1.clone()).collect::<Vec<_>>(),
        "by_package_type": by_type,
        "examples": examples,
    }))
}
