//! Messages, time, globals and numbers.

use super::{none, Args, Ctx};
use crate::game::{Game, MessageBox};
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
    let (text, buttons) = (def.text.clone(), def.buttons.clone());
    if !buttons.is_empty() {
        let owner = Some(MessageBox::owner_of(ctx.inst, &ctx.unit));
        g.message_box = Some(MessageBox { message: m, buttons: buttons.len(), pressed: None, owner });
    }
    g.emit(EventKind::MessageShown { message: m, text, buttons });
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

fn clock_now(t: f64) -> f64 {
    (t / CLOCK_TICK + 1e-9).floor() * CLOCK_TICK
}

/// Seconds since this instance last called the function, read from the 1/64 s clock;
/// 0 on the first call; the same value for every call in one frame [documented: GECK
/// wiki GetSecondsPassed]. Result scripts (no instance) get 0.
pub fn get_seconds_passed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let Some(key) = ctx.inst else { return none() };
    let (frame, now) = (g.frame, clock_now(g.time));
    let inst = g.instances.entry(key).or_default();
    if let Some((f, v)) = inst.gsp_frame {
        if f == frame {
            return Ok(Value::Num(v));
        }
    }
    let v = inst.gsp_last.map(|l| now - l).unwrap_or(0.0);
    inst.gsp_last = Some(now);
    inst.gsp_frame = Some((frame, v));
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
