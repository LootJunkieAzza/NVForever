//! Console-only commands that print (docs/design/console.md): the catalogue lists them
//! without an opcode, so the function table numbers them from
//! `fnv_script::table::CONSOLE_OPCODE_BASE`; the console runs the ones registered here
//! by catalogue name ([`registry`], checked by a test) with the line's words.
//!
//! A command takes the console line's words after its name and returns the lines to print.
//! What only the engine can do (travel, quit, menus, saving) is asked through
//! [`Game::console_requests`]. What each command prints is our design; the original's
//! wording is not copied.

use crate::game::{Game, InstKey};
use fnv_script::vm::Value;
use fnv_sim::settings::Setting;
use fnv_sim::FormId;

/// What a console command runs with.
#[derive(Debug, Clone, Default)]
pub struct ConsoleCtx {
    /// `reference.Command`, else the console's selected reference.
    pub on: Option<FormId>,
    /// The Pip-Boy's active quest, if the engine has one.
    pub active_quest: Option<FormId>,
}

/// Something only the engine can do, asked by a console command.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleRequest {
    /// Put the player at `feet` (game units) in `interior`, or outdoors in `world`.
    Travel { world: Option<FormId>, interior: Option<FormId>, feet: [f32; 3], heading_deg: f32, label: String },
    /// Select a reference (`PickRefByID`).
    Select(FormId),
    Quit,
    CloseMenus,
    Save(String),
    /// A save by name; empty for the quick save.
    Load(String),
}

pub type ConsoleImpl = fn(&mut Game, &ConsoleCtx, &[String]) -> Result<Vec<String>, String>;

/// Every registered console-only command: catalogue name, implementation.
pub fn registry() -> &'static [(&'static str, ConsoleImpl)] {
    &[
        ("ShowQuestTargets", show_quest_targets),
        ("MoveToQuestTarget", move_to_quest_target),
        ("ShowQuestStages", show_quest_stages),
        ("ShowQuestVars", show_quest_vars),
        ("ShowVars", show_vars),
        ("ShowQuests", show_quests),
        ("CompleteAllQuestStages", complete_all_quest_stages),
        ("ShowInventory", show_inventory),
        ("GetActorValueInfo", get_actor_value_info),
        ("PickRefByID", pick_ref_by_id),
        ("SetGameSetting", set_game_setting),
        ("SetStringSetting", set_string_setting),
        ("GetINISetting", no_ini),
        ("SetINISetting", no_ini),
        ("QuitGame", quit_game),
        ("CloseAllMenus", close_all_menus),
        ("SaveGame", save_game),
        ("LoadGame", load_game),
    ]
}

/// The implementation registered under a catalogue name (case-insensitive).
pub fn lookup(name: &str) -> Option<ConsoleImpl> {
    registry().iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|r| r.1)
}

/// A word as a form: `player`, an Editor ID, a FormID with or without `0x`.
pub fn form(g: &Game, s: &str) -> Result<FormId, String> {
    let s = s.trim_matches('"');
    crate::runner::resolve(g, s).or_else(|e| {
        let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        if !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit()) {
            u32::from_str_radix(h, 16).map(FormId).map_err(|_| e)
        } else {
            Err(e)
        }
    })
}

fn quest_arg(g: &Game, a: &[String]) -> Result<FormId, String> {
    let q = form(g, a.first().ok_or("a quest's Editor ID or FormID")?)?;
    if g.data.is_quest(q) {
        Ok(q)
    } else {
        Err(format!("{} is not a quest", g.data.name(q)))
    }
}

/// A reference or record for printing: `FormID EditorID`.
fn label(g: &Game, r: FormId) -> String {
    if r == FormId::PLAYER_REF {
        return format!("{r} player");
    }
    let e = g.data.name(r);
    if e == r.to_string() {
        match g.world.placed.get(r).and_then(|p| p.edid.clone()) {
            Some(e) => format!("{r} {e}"),
            None => format!("{r} ({})", g.data.name(crate::functions::base_of(g, r).unwrap_or(r))),
        }
    } else {
        format!("{r} {e}")
    }
}

fn shown(v: Value, g: &Game) -> String {
    match v {
        Value::Num(n) if n.fract() == 0.0 => format!("{n:.0}"),
        Value::Num(n) => format!("{n:.2}"),
        Value::Ref(0) => "none".into(),
        Value::Ref(r) => label(g, FormId(r)),
    }
}

// ---- quests ------------------------------------------------------------------------------

/// One target of an objective shown and not done.
struct QuestTarget {
    quest: FormId,
    objective: i32,
    target: FormId,
    cell: Option<FormId>,
    pos: [f32; 3],
    rot_z: f32,
}

/// The quests whose targets the console shows: the active quest, else every running quest
/// with an objective shown and not done [hypothesis: the original's ShowQuestTargets
/// reports the active quest; with none picked we show them all rather than nothing].
fn target_quests(g: &Game, ctx: &ConsoleCtx) -> Vec<FormId> {
    if let Some(q) = ctx.active_quest.filter(|q| g.quests.get(q).is_some_and(|s| s.running)) {
        return vec![q];
    }
    g.quests.iter().filter(|(_, s)| s.running && s.objectives.values().any(|o| o.displayed && !o.completed)).map(|(&q, _)| q).collect()
}

/// Every target of the objectives shown and not done, where it is now. The targets'
/// conditions are not tested (as the compass markers, `pipboy::data::quest_targets`).
fn quest_targets(g: &Game, ctx: &ConsoleCtx) -> Vec<QuestTarget> {
    let mut out = Vec::new();
    for q in target_quests(g, ctx) {
        let (Some(def), Some(state)) = (g.data.quests.get(&q), g.quests.get(&q)) else { continue };
        for (i, o) in &state.objectives {
            if !o.displayed || o.completed {
                continue;
            }
            for t in def.objectives.get(i).into_iter().flat_map(|d| &d.targets) {
                let Some(r) = t.target else { continue };
                let (cell, pos, rot) = match g.world.peek(r) {
                    Some(s) => (s.cell, s.pos, s.rot),
                    None => match g.world.placed.get(r) {
                        Some(p) => (p.cell, p.pos, p.rot),
                        None => continue,
                    },
                };
                out.push(QuestTarget { quest: q, objective: *i, target: r, cell, pos, rot_z: rot[2] });
            }
        }
    }
    out
}

/// The door outdoors into an interior cell, if one leads there: (its cell, position).
fn door_into(g: &Game, cell: FormId) -> Option<(FormId, [f32; 3])> {
    let refs = g.world.placed;
    refs.refs.values().find_map(|d| {
        let t = d.teleport?;
        (refs.get(t.door)?.cell == Some(cell)).then_some(())?;
        Some((d.cell?, d.pos))
    })
}

fn cell_label(g: &Game, cell: Option<FormId>) -> String {
    match cell {
        None => "no cell".into(),
        Some(c) => match g.world.placed.cell_world.get(&c) {
            Some(w) => format!("{} {} (exterior)", c, g.data.name(*w)),
            None => format!("{} {}", c, g.data.name(c)),
        },
    }
}

/// `ShowQuestTargets` (`sqt`): each target of the active quest's objectives shown and not
/// done: quest, objective, target reference, its cell and position, and for a target
/// indoors the door outdoors that leads to it.
pub fn show_quest_targets(g: &mut Game, ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    let ts = quest_targets(g, ctx);
    if ts.is_empty() {
        return Ok(vec!["No quest targets (no objective shown and not done)".into()]);
    }
    let mut out = Vec::new();
    for t in &ts {
        out.push(format!(
            "{} objective {}: target {} in {} at {:.0} {:.0} {:.0}",
            g.data.name(t.quest),
            t.objective,
            label(g, t.target),
            cell_label(g, t.cell),
            t.pos[0],
            t.pos[1],
            t.pos[2]
        ));
        if let Some(c) = t.cell.filter(|c| !g.world.placed.cell_world.contains_key(c)) {
            if let Some((dc, p)) = door_into(g, c) {
                out.push(format!("  load door outdoors in {} at {:.0} {:.0} {:.0}", cell_label(g, Some(dc)), p[0], p[1], p[2]));
            }
        }
    }
    Ok(out)
}

/// `MoveToQuestTarget [index]` (`movetoqt`): the player travels to a target of the active
/// quest (`index` from 0 among `sqt`'s list, else the first), arriving at the target's
/// position [hypothesis: the original puts the player at the target; collision then
/// moves the player out of it].
pub fn move_to_quest_target(g: &mut Game, ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let ts = quest_targets(g, ctx);
    let i = match a.first() {
        Some(w) => w.parse::<usize>().map_err(|_| format!("{w:?} is not an index"))?,
        None => 0,
    };
    let t = ts.get(i).ok_or_else(|| if ts.is_empty() { "No quest target to move to".to_string() } else { format!("Only {} quest target(s)", ts.len()) })?;
    let cell = t.cell.ok_or("the target is in no cell")?;
    let world = g.world.placed.cell_world.get(&cell).copied();
    let label = format!("{} objective {} ({})", g.data.name(t.quest), t.objective, label(g, t.target));
    g.console_requests.push(ConsoleRequest::Travel {
        world,
        interior: if world.is_some() { None } else { Some(cell) },
        feet: t.pos,
        heading_deg: t.rot_z.to_degrees(),
        label: label.clone(),
    });
    Ok(vec![format!("Moving to {label}")])
}

/// `ShowQuestStages <quest>` (`sqs`): every stage the quest defines, and whether it is done.
pub fn show_quest_stages(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let q = quest_arg(g, a)?;
    let def = &g.data.quests[&q];
    let state = g.quests.get(&q);
    let mut out = vec![format!(
        "{}: {}{}stage {}",
        g.data.name(q),
        if state.is_some_and(|s| s.running) { "running, " } else { "not running, " },
        if state.is_some_and(|s| s.completed) { "completed, " } else { "" },
        state.map(|s| s.stage()).unwrap_or(0)
    )];
    for st in def.stages.keys() {
        let done = state.is_some_and(|s| s.stages_done.contains(st));
        out.push(format!("  stage {st}: {}", if done { "done" } else { "-" }));
    }
    Ok(out)
}

/// The variables of an instance as lines: every variable its script declares, with its
/// value now (0 when never set).
fn var_lines(g: &Game, key: InstKey, script: FormId) -> Vec<String> {
    let Some(c) = g.data.script(script) else { return vec![format!("script {} is not compiled", g.data.name(script))] };
    let mut vars: Vec<(u16, String)> = c.unit.variables.iter().map(|v| (v.index as u16, v.name.clone())).collect();
    vars.sort();
    if vars.is_empty() {
        return vec![format!("script {} declares no variables", g.data.name(script))];
    }
    vars.into_iter().map(|(i, n)| format!("  {n} = {}", shown(g.var(key, i), g))).collect()
}

/// `ShowQuestVars <quest>` (`sqv`): the quest script's variables and values.
pub fn show_quest_vars(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let q = quest_arg(g, a)?;
    let (key, script) = g.instance_of(q).ok_or_else(|| format!("{} runs no script", g.data.name(q)))?;
    let mut out = vec![format!("{} (script {}):", g.data.name(q), g.data.name(script))];
    out.extend(var_lines(g, key, script));
    Ok(out)
}

/// `ShowVars` (`sv`): the selected reference's script variables (or a quest's, named).
pub fn show_vars(g: &mut Game, ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let r = match a.first() {
        Some(w) => form(g, w)?,
        None => ctx.on.ok_or("select a reference first (click it, or prid)")?,
    };
    let (key, script) = g.instance_of(r).ok_or_else(|| format!("{} runs no script", label(g, r)))?;
    let mut out = vec![format!("{} (script {}):", label(g, r), g.data.name(script))];
    out.extend(var_lines(g, key, script));
    Ok(out)
}

/// `ShowQuests` (`sq`): the running quests with their stage, and the completed ones.
pub fn show_quests(g: &mut Game, _ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for (q, s) in g.quests.iter().filter(|(_, s)| s.running) {
        let name = g.data.quests.get(q).and_then(|d| d.name.clone()).map(|n| format!(" \"{n}\"")).unwrap_or_default();
        out.push(format!("{} {}{name}: stage {}{}", q, g.data.name(*q), s.stage(), if s.completed { ", completed" } else { "" }));
    }
    if out.is_empty() {
        out.push("No quest is running".into());
    }
    let done = g.quests.values().filter(|s| !s.running && (s.completed || s.failed)).count();
    out.push(format!("{} running, {done} finished and stopped", out.len()));
    Ok(out)
}

/// `CompleteAllQuestStages` (`caqs`): every stage of every quest is marked done
/// [hypothesis: their result scripts are not run, so the world is not changed by
/// hundreds of stage scripts at once; quests are not marked completed].
pub fn complete_all_quest_stages(g: &mut Game, _ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    let data = g.data;
    let mut n = 0usize;
    for (q, def) in &data.quests {
        let s = g.quest(*q);
        for st in def.stages.keys() {
            n += usize::from(s.stages_done.insert(*st));
        }
    }
    Ok(vec![format!("{n} stage(s) of {} quests marked done (stage scripts not run)", data.quests.len())])
}

// ---- references and actors ---------------------------------------------------------------

/// `ShowInventory` (`inv`): the selected reference's items (the player without one): count,
/// FormID, Editor ID and name, equipped, and condition where it is kept.
pub fn show_inventory(g: &mut Game, ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    let r = ctx.on.unwrap_or(FormId::PLAYER_REF);
    let who = label(g, r);
    let groups: Vec<_> = g.world.inventory_mut(r).map_err(|e| format!("{who}: {e}"))?.groups().collect();
    let mut out = vec![format!("{who}: {} stack(s)", groups.len())];
    for (item, extra, n) in groups {
        let name = g.world.forms.get(item).and_then(|i| i.name.clone()).map(|n| format!(" \"{n}\"")).unwrap_or_default();
        let worn = if extra.equipped { " (equipped)" } else { "" };
        let cond = extra.condition.map(|c| format!(" condition {c:.0}")).unwrap_or_default();
        out.push(format!("  {n} x {} {}{name}{worn}{cond}", item, g.data.name(item)));
    }
    Ok(out)
}

/// `GetActorValueInfo <actor value>` (`getavinfo`): the actor value's parts on the selected
/// actor (the player without one): base, modifier, damage and the value now.
pub fn get_actor_value_info(g: &mut Game, ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let r = ctx.on.unwrap_or(FormId::PLAYER_REF);
    let name = a.first().ok_or("an actor value: GetAVInfo Strength")?;
    let code = crate::functions::av_code(g, name)?;
    let [base, modifier, damage] = crate::functions::actor_value_parts(g, r, code)?;
    let now = crate::functions::actor_value(g, r, code)?;
    let av = g.data.table.enum_name("actor_value", code).unwrap_or(name).to_string();
    Ok(vec![format!("{av} ({code}) on {}: now {now:.2} = base {base:.2} + modifier {modifier:.2} - damage {damage:.2}", label(g, r))])
}

/// `PickRefByID <reference>` (`prid`): select it for the console.
pub fn pick_ref_by_id(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let r = form(g, a.first().ok_or("prid <reference>")?)?;
    g.console_requests.push(ConsoleRequest::Select(r));
    Ok(Vec::new())
}

// ---- settings ----------------------------------------------------------------------------

fn setting_text(s: &Setting) -> String {
    match s {
        Setting::Float(v) => format!("{v}"),
        Setting::Int(v) => format!("{v}"),
        Setting::Str(v) => format!("\"{v}\""),
        Setting::Raw(b) => format!("{} bytes", b.len()),
    }
}

/// `SetGameSetting <setting> <value>` (`setgs`): change a game setting for this run. The
/// value's kind follows the setting's (its Editor ID's first letter). Readers of the game
/// layer (`World::setting_f32`, `GetGameSetting`) see it; the change is not saved, and
/// values the engine read at start (from the records) do not change [hypothesis: the
/// original keeps it until it quits, not in the save].
pub fn set_game_setting(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let (Some(name), Some(v)) = (a.first(), a.get(1)) else { return Err("setgs <setting> <value>".into()) };
    let old = g.world.setting_value(name).cloned().ok_or_else(|| format!("No game setting {name}"))?;
    let new = match old {
        Setting::Float(_) => Setting::Float(v.parse().map_err(|_| format!("{name} takes a number"))?),
        Setting::Int(_) => Setting::Int(v.parse::<f64>().map_err(|_| format!("{name} takes a whole number"))? as i32),
        Setting::Str(_) => Setting::Str(a[1..].join(" ").trim_matches('"').to_string()),
        Setting::Raw(_) => return Err(format!("{name}: its value is not read by this engine")),
    };
    g.world.change_setting(name, new.clone()).map_err(|e| e.to_string())?;
    Ok(vec![format!("{name}: {} -> {}", setting_text(&old), setting_text(&new))])
}

/// `SetStringSetting <setting> <text>`: as `setgs` for a string setting.
pub fn set_string_setting(g: &mut Game, ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    match a.first().and_then(|n| g.world.setting_value(n)) {
        Some(Setting::Str(_)) => set_game_setting(g, ctx, a),
        Some(_) => Err(format!("{} is not a string setting", a[0])),
        None => Err("SetStringSetting <setting> <text>".into()),
    }
}

/// `GetINISetting` / `SetINISetting`: the engine keeps no INI settings store (the values
/// it takes from the original's INIs are written into the code where they are used).
pub fn no_ini(_g: &mut Game, _ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    Err("INI settings are not built: the engine keeps no store of the original's INI values".into())
}

// ---- the game ----------------------------------------------------------------------------

/// `QuitGame` (`qqq`): the engine exits.
pub fn quit_game(g: &mut Game, _ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    g.console_requests.push(ConsoleRequest::Quit);
    Ok(vec!["Quitting".into()])
}

/// `CloseAllMenus` (`cam`): every open menu closes, the console too.
pub fn close_all_menus(g: &mut Game, _ctx: &ConsoleCtx, _a: &[String]) -> Result<Vec<String>, String> {
    g.console_requests.push(ConsoleRequest::CloseMenus);
    Ok(Vec::new())
}

/// `SaveGame <name>` (`save`): a manual save under that name.
pub fn save_game(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    let name = a.first().map(|s| s.trim_matches('"').to_string()).ok_or("save <name>")?;
    g.console_requests.push(ConsoleRequest::Save(name));
    Ok(Vec::new())
}

/// `LoadGame [name]` (`load`): the named save, else the quick save.
pub fn load_game(g: &mut Game, _ctx: &ConsoleCtx, a: &[String]) -> Result<Vec<String>, String> {
    g.console_requests.push(ConsoleRequest::Load(a.first().map(|s| s.trim_matches('"').to_string()).unwrap_or_default()));
    Ok(Vec::new())
}

// ---- `set <variable> to <value>` -----------------------------------------------------------

/// The console's `set <variable> to <value>` statement:
///
/// - `set quest.var to 5` / `set ref.var to 5`: a quest's or a reference's script variable;
/// - `set Global to 5`: a global by Editor ID;
/// - `set var to 5`: the selected reference's script variable.
///
/// The value is a number or a form (`player`, an Editor ID, a FormID). It is stored as a
/// script's `set` stores it (an integer variable truncates).
pub fn set_statement(g: &mut Game, ctx: &ConsoleCtx, words: &[String]) -> Result<Vec<String>, String> {
    let usage = "set <variable> to <value>";
    let (target, value) = match words {
        [t, to, v] if to.eq_ignore_ascii_case("to") => (t.as_str(), v.as_str()),
        _ => return Err(usage.into()),
    };
    let v = match value.parse::<f64>() {
        Ok(n) => Value::Num(n),
        Err(_) => Value::Ref(form(g, value)?.0),
    };
    let (owner, var) = match target.rsplit_once('.') {
        Some((o, v)) if !o.is_empty() && !v.is_empty() => (Some(form(g, o)?), v),
        _ => (None, target),
    };
    if owner.is_none() {
        if let Some((id, _)) = g.world.forms.global_by_edid(var) {
            g.world.set_global(id, v.num() as f32).map_err(|e| e.to_string())?;
            return Ok(vec![format!("{var} = {}", shown(Value::Num(g.world.global(id).map_err(|e| e.to_string())? as f64), g))]);
        }
    }
    let owner = owner.or(ctx.on).ok_or_else(|| format!("No global {var}, and no reference selected"))?;
    let (key, script) = g.instance_of(owner).ok_or_else(|| format!("{} runs no script", label(g, owner)))?;
    let index = g.data.script(script).and_then(|c| c.var_index(var)).ok_or_else(|| format!("{} has no variable {var}", g.data.name(script)))?;
    g.store(key, Some(script), index, v, "console", false);
    Ok(vec![format!("{}.{var} = {}", label(g, owner), shown(g.var(key, index), g))])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered name is a console-only command of the catalogue (one numbered
    /// from `CONSOLE_OPCODE_BASE`, without a catalogue opcode), spelled as the catalogue
    /// spells it.
    #[test]
    fn registered_names_are_catalogue_console_commands() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/script/functions.json");
        let t = fnv_script::FunctionTable::load(std::path::Path::new(path)).expect("functions.json");
        for (name, _) in registry() {
            let op = t.by_name(name).unwrap_or_else(|| panic!("{name} is not in the catalogue"));
            assert!(op >= fnv_script::table::CONSOLE_OPCODE_BASE, "{name} has a catalogue opcode: register it in functions::registry");
            assert_eq!(&t.get(op).unwrap().name, name);
        }
        // Aliases find them too.
        for alias in ["sqt", "MoveToQT", "sqs", "sqv", "sv", "caqs", "inv", "getavinfo", "prid", "setgs", "qqq", "cam"] {
            let f = t.by_name(alias).and_then(|op| t.get(op)).unwrap_or_else(|| panic!("{alias}"));
            assert!(lookup(&f.name).is_some(), "{alias}");
        }
    }
}
