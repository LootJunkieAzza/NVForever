//! Reference functions: enable state, position, identity, locks, activation, lists.

use super::{base_of, none, yes, Args, Ctx};
use crate::game::{ActMode, Game};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// Enable / disable; the fade flag is presentation only.
pub fn enable(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let was = g.world.get(r).map_err(|e| e.to_string())?.enabled;
    g.world.set_enabled(r, true).map_err(|e| e.to_string())?;
    if !was {
        g.emit(EventKind::RefEnabled { reference: r });
        g.note_enabled(r);
    }
    none()
}

pub fn disable(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let was = g.world.get(r).map_err(|e| e.to_string())?.enabled;
    g.world.set_enabled(r, false).map_err(|e| e.to_string())?;
    if was {
        g.emit(EventKind::RefDisabled { reference: r });
    }
    none()
}

pub fn get_disabled(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(!g.world.is_enabled(r).map_err(|e| e.to_string())?)
}

/// The current cell, position and rotation of a reference.
type Place = (Option<FormId>, [f32; 3], [f32; 3]);

fn place(g: &mut Game, r: FormId) -> Result<Place, String> {
    let s = g.world.get(r).map_err(|e| e.to_string())?;
    Ok((s.cell, s.pos, s.rot))
}

/// `MoveTo marker [x y z]`: the calling reference goes to the marker's cell and position
/// plus the offsets. Moving the player loads the destination's cells.
/// `MoveToMarkerWithFade` is the same with a fade (presentation).
pub fn move_to(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let marker = a.form(0)?;
    let (cell, mut pos, rot) = place(g, marker)?;
    for (i, p) in pos.iter_mut().enumerate() {
        *p += a.num_or(1 + i, 0.0) as f32;
    }
    let cell = cell.map(|c| g.world.placed.exterior_cell_at(c, pos).unwrap_or(c));
    {
        let s = g.world.get(r).map_err(|e| e.to_string())?;
        s.cell = cell;
        s.pos = pos;
        if r != FormId::PLAYER_REF {
            s.rot = rot;
        }
    }
    if g.ref_script(r).is_some() {
        g.moved.insert(r);
    }
    if r != FormId::PLAYER_REF {
        g.teleported.insert(r);
        // A drawn object (not an actor) is drawn at its new place, as after SetPos.
        if !crate::ai::is_actor_base(g.world.forms, base_of(g, r).unwrap_or(FormId(0))) {
            g.turned.insert(r);
        }
    }
    g.emit(EventKind::Moved { reference: r, cell, pos });
    if r == FormId::PLAYER_REF {
        g.player_entered(cell);
        g.bring_teammates();
    }
    none()
}

pub fn get_self(_g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(ctx.self_ref.map(|r| Value::Ref(r.0)).unwrap_or(Value::NONE))
}

/// 1 when the caller's base form is the given form (or the caller is that form).
pub fn get_is_id(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let id = a.form(0)?;
    yes(r == id || base_of(g, r) == Some(id))
}

pub fn get_is_reference(_g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(ctx.subject()? == a.form(0)?)
}

/// 1 when the caller's cell's Editor ID begins with the given cell's [documented:
/// catalogue summary].
pub fn get_in_cell(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let want = a.form(0)?;
    let Some(cell) = g.cell_of(r) else { return yes(false) };
    if cell == want {
        return yes(true);
    }
    let (Some(have), Some(w)) = (g.data.edid_of.get(&cell), g.data.edid_of.get(&want)) else { return yes(false) };
    yes(have.to_ascii_lowercase().starts_with(&w.to_ascii_lowercase()))
}

pub fn get_in_worldspace(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let ws = a.form(0)?;
    let cell = g.cell_of(r);
    yes(cell.and_then(|c| g.world.placed.world_of(c)) == Some(ws))
}

pub fn get_in_same_cell(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let t = a.form(0)?;
    let (c1, c2) = (g.cell_of(r), g.cell_of(t));
    yes(c1.is_some() && c1 == c2)
}

pub fn is_in_interior(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let cell = g.cell_of(r);
    yes(cell.is_some_and(|c| g.world.placed.world_of(c).is_none()))
}

/// Distance in game units between the two references when they are in the same
/// interior cell or the same worldspace; otherwise a large number [hypothesis].
pub fn get_distance(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let t = a.form(0)?;
    let (c1, p1, _) = place(g, r)?;
    let (c2, p2, _) = place(g, t)?;
    let space = |c: Option<FormId>| c.map(|c| g.world.placed.world_of(c).unwrap_or(c));
    if c1.is_none() || space(c1) != space(c2) {
        return Ok(Value::Num(f32::MAX as f64));
    }
    let d: f32 = (0..3).map(|i| (p1[i] - p2[i]).powi(2)).sum::<f32>().sqrt();
    Ok(Value::Num(d as f64))
}

pub fn get_pos(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let i = a.axis(0)?;
    Ok(Value::Num(place(g, r)?.1[i] as f64))
}

/// `SetPos axis value`: the reference moves along one axis [documented: GECK wiki SetPos].
/// What moves it on screen hears of it as of a MoveTo or a Rotate ([`placed_by_script`]).
pub fn set_pos(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let i = a.axis(0)?;
    let v = a.num(1)? as f32;
    let s = g.world.get(r).map_err(|e| e.to_string())?;
    s.pos[i] = v;
    let (cell, pos) = (s.cell, s.pos);
    let cell = cell.map(|c| g.world.placed.exterior_cell_at(c, pos).unwrap_or(c));
    g.world.get(r).map_err(|e| e.to_string())?.cell = cell;
    placed_by_script(g, r, true);
    none()
}

/// A script put `r` somewhere new (SetPos, SetAngle): an actor other than the player
/// starts again from there, as after MoveTo (`Game::teleported`); another object is
/// drawn at its new place, as after Rotate (`Game::turned`); the player travels there
/// when its position changed (the `Moved` event the engine turns into a teleport).
/// [hypothesis: the original applies SetPos and SetAngle at once, as their pages
/// describe no delay; the earlier code changed only the stored values, which the AI
/// and the engine overwrote, docs/design/script-function-audit.md.]
fn placed_by_script(g: &mut Game, r: FormId, moved: bool) {
    if g.ref_script(r).is_some() {
        g.moved.insert(r);
    }
    if r == FormId::PLAYER_REF {
        if moved {
            if let Ok((cell, pos, _)) = place(g, r) {
                g.emit(EventKind::Moved { reference: r, cell, pos });
            }
        }
    } else if crate::ai::is_actor_base(g.world.forms, base_of(g, r).unwrap_or(FormId(0))) {
        g.teleported.insert(r);
    } else {
        g.turned.insert(r);
    }
}

/// Angles in degrees in scripts, radians in the world state [documented: catalogue,
/// GetAngle/SetAngle take degrees].
pub fn get_angle(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let i = a.axis(0)?;
    Ok(Value::Num((place(g, r)?.2[i] as f64).to_degrees()))
}

pub fn set_angle(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let i = a.axis(0)?;
    let v = a.num(1)?.to_radians() as f32;
    g.world.get(r).map_err(|e| e.to_string())?.rot[i] = v;
    placed_by_script(g, r, false);
    none()
}

pub fn get_linked_ref(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(g.world.placed.get(r).and_then(|p| p.linked).map(|l| Value::Ref(l.0)).unwrap_or(Value::NONE))
}

pub fn get_parent_ref(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(g.world.placed.get(r).and_then(|p| p.enable_parent).map(|e| Value::Ref(e.reference.0)).unwrap_or(Value::NONE))
}

/// The reference is removed (the engine removes it a few frames later; here at once
/// [hypothesis]).
pub fn mark_for_delete(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().marked_for_delete = true;
    g.world.get(r).map_err(|e| e.to_string())?.deleted = true;
    none()
}

pub fn set_destroyed(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().destroyed = a.num_or(0, 1.0) != 0.0;
    none()
}

pub fn get_destroyed(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.extra.get(&r).is_some_and(|e| e.destroyed))
}

pub fn lock(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if is_terminal(g, r) {
        crate::terminal::lock(g, r, a.num_or(0, 0.0) as i32);
        return none();
    }
    let level = a.num(0).ok().map(|l| l.clamp(0.0, 255.0) as u8);
    g.world.lock(r, level).map_err(|e| e.to_string())?;
    none()
}

pub fn unlock(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if is_terminal(g, r) {
        crate::terminal::unlock(g, r);
        return none();
    }
    g.world.unlock(r).map_err(|e| e.to_string())?;
    none()
}

/// A reference whose base is a terminal (TERM): its lock is the terminal's own
/// (`crate::terminal::{lock, unlock, locked, lock_level}`), not the world's lock data.
pub fn is_terminal(g: &Game, r: FormId) -> bool {
    super::base_of(g, r).is_some_and(|b| g.data.terminals.contains_key(&b))
}

/// Whether the reference's cell is one of the rules' loaded cells.
pub fn in_loaded_cell(g: &Game, r: FormId) -> bool {
    g.cell_of(r).is_some_and(|c| g.loaded_cells.contains(&c))
}

/// 0 not locked, 1 locked; 2 for a lock broken by forcing it [documented: GECK wiki
/// GetLocked, "locked out"; docs/spec-notes/lockpicking.md §5]. A terminal answers by
/// `crate::terminal::locked` while its cell is loaded, else 0 [observed, function probes
/// `fp1_queries` / `fp3_terminal` 2026-10-17].
pub fn get_locked(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if is_terminal(g, r) {
        return Ok(Value::Num(if in_loaded_cell(g, r) { crate::terminal::locked(g, r) as f64 } else { 0.0 }));
    }
    let status = g.world.lock_status(r).map_err(|e| e.to_string())?;
    Ok(Value::Num(if status == 1 && g.locks.broken.contains(&r) { 2.0 } else { status as f64 }))
}

/// 1 for a lock broken by forcing it (`crate::lockpick`) [documented: GECK wiki
/// GetIsLockBroken].
pub fn get_is_lock_broken(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::bool(g.locks.broken.contains(&r)))
}

/// `Ref.Activate [actor] [flag]`: with no actor the calling reference's action reference
/// (inside OnActivate: whoever activated it) is used, and without one nothing happens;
/// flag 0 runs the default action, bypassing OnActivate; flag 1 runs the OnActivate block
/// instead [documented: GECK wiki Activate].
pub fn activate(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let target = ctx.subject()?;
    let actor = a.opt_form(0).or(ctx.action_ref).ok_or("no action reference")?;
    let mode = if a.num_or(1, 0.0) != 0.0 { ActMode::Block } else { ActMode::Default };
    g.activate(actor, target, mode)?;
    none()
}

pub fn get_action_ref(_g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(ctx.action_ref.map(|r| Value::Ref(r.0)).unwrap_or(Value::NONE))
}

pub fn is_action_ref(_g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(ctx.action_ref.is_some() && ctx.action_ref == a.opt_form(0))
}

fn base_sig(g: &Game, r: FormId) -> Option<[u8; 4]> {
    base_of(g, r).and_then(|b| g.data.sig(b))
}

pub fn is_actor(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(matches!(base_sig(g, r).as_ref(), Some(b"NPC_") | Some(b"CREA")))
}

pub fn get_is_creature(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(base_sig(g, r).as_ref() == Some(b"CREA"))
}

/// The members of a form list: authored, plus those added at run time.
pub fn list_members(g: &Game, list: FormId) -> Vec<FormId> {
    let mut v = g.form_lists.get(&list).cloned().unwrap_or_default();
    v.extend(g.data.form_lists.get(&list).into_iter().flatten().copied());
    v
}

/// 1 when the caller's base form is in the list.
pub fn is_in_list(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let list = a.form(0)?;
    let b = base_of(g, r);
    let m = list_members(g, list);
    yes(m.contains(&r) || b.is_some_and(|b| m.contains(&b)))
}

/// Inserts at index 0 [documented: catalogue summary].
pub fn add_form_to_form_list(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let list = a.form(0)?;
    let f = a.form(1)?;
    if g.data.sig(list).as_ref() != Some(b"FLST") {
        return Err(format!("{} is not a form list", g.data.name(list)));
    }
    if !list_members(g, list).contains(&f) {
        g.form_lists.entry(list).or_default().insert(0, f);
    }
    none()
}

/// Opens (1) or closes (0) a door [documented: GECK wiki SetOpenState].
pub fn set_open_state(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.world.get(r).map_err(|e| e.to_string())?.open = a.num(0)? != 0.0;
    none()
}

/// 0 none, 1 open, 2 opening, 3 closed, 4 closing [documented: GECK wiki GetOpenState
/// notes]. An object the engine animates open and closed reads the state it told
/// (`Senses::open_states`); any other object 1 or 3 by its open flag, and an actor 0
/// [observed: a load door, a cash register, crates, a duffle bag, sacks, a metal box, a
/// safe and a dialogue trigger activator in the Goodsprings gas station all read 3, and
/// Ringo 0, captures `fa_batch1_queries` and `fa_batch3_actions`].
pub fn get_open_state(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    if let Some(&code) = g.senses.open_states.get(&r) {
        return Ok(Value::Num(code as f64));
    }
    if r == FormId::PLAYER_REF || crate::ai::is_actor_base(g.world.forms, base_of(g, r).unwrap_or(FormId(0))) {
        return Ok(Value::Num(0.0));
    }
    let open = g.world.get(r).map_err(|e| e.to_string())?.open;
    Ok(Value::Num(if open { 1.0 } else { 3.0 }))
}

/// A trigger-enter event on the calling trigger, by the given reference [documented:
/// catalogue summary].
pub fn enter_trigger(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let t = ctx.subject()?;
    let by = a.form(0)?;
    g.ref_event(t, crate::game::event::ON_TRIGGER_ENTER, Some(Value::Ref(by.0)), Some(by));
    none()
}

/// `ref.Rotate axis degreesPerSecond`: turns the reference about the world's X, Y or Z
/// axis at that speed [documented: GECK wiki Rotate]. One call turns it for one frame:
/// the shipped turners call it from GameMode every frame [observed: `SCPT 0x0013414C`
/// and its two siblings, `linkedRef.Rotate z ±300/900`], and a reference turns about
/// one axis per frame, the last call of the frame counting [documented: the page's
/// note]. So a call records the turn and [`Game::tick`] applies speed × frame time to
/// the angle at the end of the frame [hypothesis: one frame per call]. The angle is the
/// reference's own about that axis (Z is the heading; X and Y are only "world" for an
/// unturned reference, which the page warns of [documented: "not reliable for complex
/// rotations"]). Collision does not follow [documented: "Collision is not updated with
/// rotation until the cell is reloaded"].
pub fn rotate(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let axis = a.axis(0)?;
    let speed = a.num(1)?;
    g.world.get(r).map_err(|e| e.to_string())?;
    g.rotations.insert(r, (axis, speed));
    none()
}

impl Game<'_> {
    /// Apply this frame's Rotate calls ([`rotate`]), angles kept within ±π.
    pub(crate) fn apply_rotations(&mut self, dt: f64) {
        for (r, (axis, speed)) in std::mem::take(&mut self.rotations) {
            let Ok(s) = self.world.get(r) else { continue };
            let a = s.rot[axis] as f64 + (speed * dt).to_radians();
            s.rot[axis] = (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) as f32 - std::f32::consts::PI;
            self.turned.insert(r);
        }
    }
}
