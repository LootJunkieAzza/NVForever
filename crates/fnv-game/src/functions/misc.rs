//! Messages, time, globals and numbers.

use super::{none, Args, Ctx};
use crate::game::{Game, MessageBox};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::clock;
use rand_core::RngCore;

/// `ShowMessage msg [vars…] [duration]`: traced with the message text; a message with
/// buttons waits for one (runner input `press_button`, read by GetButtonPressed).
pub fn show_message(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let m = a.form(0)?;
    let def = g.data.messages.get(&m).ok_or_else(|| format!("{} is not a message", g.data.name(m)))?;
    let (text, buttons) = (def.text.clone(), def.buttons.clone());
    if !buttons.is_empty() {
        g.message_box = Some(MessageBox { message: m, buttons: buttons.len(), pressed: None });
    }
    g.emit(EventKind::MessageShown { message: m, text, buttons });
    none()
}

/// The chosen button once, then −1 [documented: GECK wiki GetButtonPressed].
pub fn get_button_pressed(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    if let Some(b) = g.message_box.as_ref().and_then(|m| m.pressed) {
        g.message_box = None;
        return Ok(Value::Num(b as f64));
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
