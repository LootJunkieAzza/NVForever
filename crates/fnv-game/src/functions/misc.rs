//! Messages, time, globals and numbers.

use super::{none, Args, Ctx};
use crate::game::{menus, Game, MessageBox};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::clock;
use rand_core::RngCore;

/// `ShowMessage msg [vars…] [duration]`: traced with the message text; a message with
/// buttons waits for one (runner input `press_button`, read by GetButtonPressed of the
/// script that showed it).
pub fn show_message(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    let def = g.data.messages.get(&m).ok_or_else(|| format!("{} is not a message", g.data.name(m)))?;
    let (text, buttons, is_box) = (def.text.clone(), def.buttons.clone(), def.message_box);
    if !buttons.is_empty() {
        let owner = Some(MessageBox::owner_of(ctx.inst, &ctx.unit));
        g.message_box = Some(MessageBox { message: m, buttons: buttons.len(), pressed: None, owner });
    }
    g.emit(EventKind::MessageShown { message: m, text, buttons: buttons.clone() });
    // A message box (MESG flag Message Box, or buttons) is a menu until the player closes
    // it [observed: MenuMode 1001 while the pack plugins' boxes were open,
    // layer5-captures.md §10.1]; other messages are corner notifications.
    if is_box || !buttons.is_empty() {
        g.show_box(m);
    }
    none()
}

/// The chosen button once, then −1 [documented: GECK wiki GetButtonPressed]. Only the
/// script that showed the box receives it; for any other script the box is not there
/// (−1) and stays open [hypothesis, from the GECK wiki's ShowMessage and
/// GetButtonPressed pages; the `--play` integration saw an unrelated script (Doc
/// Mitchell's chemistry set) take every box's button while the button was global].
pub fn get_button_pressed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let me = MessageBox::owner_of(ctx.inst, &ctx.unit);
    if let Some(m) = g.message_box.as_ref() {
        if let (Some(b), true) = (m.pressed, m.owner.as_ref().is_none_or(|o| *o == me)) {
            g.message_box = None;
            return Ok(Value::Num(b as f64));
        }
    }
    Ok(Value::Num(-1.0))
}

/// Writes to the warnings log only.
pub fn show_warning(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    if let Ok(s) = a.str(0) {
        tracing::info!(warning = %s, "script ShowWarning");
    }
    let _ = g;
    none()
}

/// 0 to 99 from the game's seeded generator (deterministic per seed).
pub fn get_random_percent(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num((g.rng.next_u32() % 100) as f64))
}

/// The resolution of the game clock GetSecondsPassed reads: 1/64 s [observed: every
/// drop in the delay captures is a multiple of 0.015625 s, layer5-captures.md §5.4;
/// hypothesis: the clock, not the frame, is quantised].
pub const CLOCK_TICK: f64 = 1.0 / 64.0;

/// The game clock in 1/64 s steps.
pub fn clock_now(t: f64) -> f64 {
    (t / CLOCK_TICK + 1e-9).floor() * CLOCK_TICK
}

/// The clock time between the start of this instance's previous per-frame run
/// (GameMode, ScriptEffectUpdate) and the start of the current one; 0 on the first run.
/// Every call in one run gives the same value [documented: GECK wiki GetSecondsPassed],
/// and the value does not depend on when the script last called the function
/// [observed: VCG01's countdowns restarted after a pause in its calls ran their full
/// time, layer5-captures.md §10.7; see [`Game::mark_run`]]. Outside a per-frame run (an
/// event block of the same instance), the time since the latest run; result scripts (no
/// instance) get 0.
pub fn get_seconds_passed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let Some(key) = ctx.inst else { return none() };
    let now = clock_now(g.time);
    let Some(inst) = g.instances.get(&key) else { return none() };
    let v = match (inst.run_last, inst.run_prev) {
        (Some(last), prev) if last == now => prev.map_or(0.0, |p| now - p),
        (Some(last), _) => now - last,
        (None, _) => 0.0,
    };
    Ok(Value::Num(v))
}

pub fn get_global_value(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let gl = a.form(0)?;
    Ok(Value::Num(g.world.global(gl).map_err(|e| e.to_string())? as f64))
}

/// The time of day in hours: the GameHour global.
pub fn get_current_time(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let (id, _) = g.world.forms.global_by_edid(clock::GAME_HOUR).ok_or("no GameHour global")?;
    Ok(Value::Num(g.world.global(id).map_err(|e| e.to_string())? as f64))
}

pub fn get_game_setting(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let name = a.str(0)?;
    let v = g.world.forms.setting_f32(name).ok_or_else(|| format!("game setting {name:?} is not defined"))?;
    Ok(Value::Num(v as f64))
}

pub fn abs(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.abs()))
}

/// The day of the week, 0 Sunday … 6 Saturday [documented: GECK wiki GetDayOfWeek],
/// as the Gregorian weekday of the GameYear / GameMonth (from 0) / GameDay globals
/// [hypothesis: the game counts weekdays on that calendar].
pub fn get_day_of_week(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let c = g.world.calendar().map_err(|e| e.to_string())?;
    Ok(Value::Num(weekday(c.year as i64, c.month as i64 + 1, c.day as i64) as f64))
}

/// Sunday = 0 (Sakamoto's method).
pub fn weekday(year: i64, month: i64, day: i64) -> i64 {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[((month - 1).clamp(0, 11)) as usize] + day).rem_euclid(7)
}

pub fn sqrt(_g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::Num(a.num(0)?.max(0.0).sqrt()))
}

/// 1: the PC version [decision, as IsXBox and IsPS3 read 0].
pub fn one(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(1.0))
}

/// A Layer 6 state with no Layer 6 yet: always 0 (not in combat, not talking).
pub fn zero(_g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    none()
}

/// 1 while a menu is open, or that menu with a menu code [documented: catalogue summary].
pub fn menu_mode(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    Ok(Value::bool(match (g.menu, a.num(0).ok().filter(|c| *c != 0.0)) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(m), Some(c)) => m as f64 == c,
    }))
}

/// `speaker.SayTo target topic`: the speaker says the topic's first line it can say to
/// the target (`Game::say`, docs/spec-notes/scripts.md §7.7).
pub fn say_to(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let (target, topic) = (a.form(0)?, a.form(1)?);
    g.say(speaker, Some(target), topic, true);
    none()
}

/// `speaker.Say topic`: as SayTo, with no listener.
pub fn say(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let speaker = ctx.subject()?;
    let topic = a.form(0)?;
    g.say(speaker, None, topic, false);
    none()
}

/// 1 while the actor is saying a line (SayTo, Say).
pub fn is_talking(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let who = ctx.subject()?;
    Ok(Value::bool(g.is_talking(who)))
}

/// The menu each character-creation function opens (`Game::open_menu`) [observed: the
/// code of each was open after the stage whose script calls it, save nvfnew,
/// layer5-captures.md §10.2–10.3]. What the menu does (the name typed, the race chosen,
/// the tag skills and traits picked, the S.P.E.C.I.A.L. points) is the player's, outside
/// the scripts: closing it keeps the state as it is.
fn open(g: &mut Game, code: u16) -> Result<Value, String> {
    g.open_menu(code);
    none()
}

/// GetPlayerName: the name menu.
pub fn get_player_name(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::NAME)
}

/// ShowRaceMenu: the race menu.
pub fn show_race_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::RACE)
}

/// SetTagSkills: the tag-skill menu.
pub fn set_tag_skills(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::TAG_SKILLS)
}

/// ShowTraitMenu: the trait menu.
pub fn show_trait_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::TRAITS)
}

/// ShowLoveTesterMenuParams: the Vigor Tester's S.P.E.C.I.A.L. menu.
pub fn show_love_tester_menu(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    open(g, menus::LOVE_TESTER)
}
