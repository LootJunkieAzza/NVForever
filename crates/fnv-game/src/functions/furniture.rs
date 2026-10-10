//! Furniture state functions (docs/spec-notes/furniture.md §2–3): what an actor on a
//! chair, stool or bed answers. The state itself is set by whoever moves the actor
//! (the engine, AI packages) through [`Game::set_furniture_use`].

use super::{base_of, none, yes, Args, Ctx};
use crate::game::{FurnitureUse, Game};
use fnv_script::vm::Value;
use fnv_sim::FormId;

impl Game<'_> {
    /// The furniture `actor` is using now, or `None` when it stops.
    pub fn set_furniture_use(&mut self, actor: FormId, using: Option<FurnitureUse>) {
        if using.is_none() && !self.extra.contains_key(&actor) {
            return;
        }
        self.extra.entry(actor).or_default().furniture = using;
    }

    pub fn furniture_use(&self, actor: FormId) -> Option<FurnitureUse> {
        self.extra.get(&actor).and_then(|e| e.furniture)
    }
}

fn using(g: &Game, ctx: &Ctx) -> Result<Option<FurnitureUse>, String> {
    Ok(g.furniture_use(ctx.subject()?))
}

/// Marker IDs 1 to 10 are beds [documented: GECK wiki GetFurnitureMarkerID, catalogue
/// notes]: their users sleep, the others sit.
fn bed(u: &FurnitureUse) -> bool {
    (1..=10).contains(&u.marker_id)
}

/// 0 when not on a seat; else 2 while getting on, 3 while on it, 4 while getting off
/// [observed: the IDLE tree and the AI observer, furniture.md §3].
pub fn get_sitting(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(using(g, ctx)?.filter(|u| !bed(u)).map_or(0.0, |u| u.state as f64)))
}

/// As [`get_sitting`], for beds.
pub fn get_sleeping(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(using(g, ctx)?.filter(bed).map_or(0.0, |u| u.state as f64)))
}

pub fn get_furniture_marker_id(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(using(g, ctx)?.map_or(0.0, |u| u.marker_id as f64)))
}

pub fn is_current_furniture_ref(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let Some(r) = a.opt_form(0) else { return none() };
    yes(using(g, ctx)?.is_some_and(|u| u.reference == r))
}

pub fn is_current_furniture_obj(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let Some(b) = a.opt_form(0) else { return none() };
    yes(using(g, ctx)?.is_some_and(|u| base_of(g, u.reference) == Some(b)))
}
