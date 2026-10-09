//! Inventory functions over the Layer 4 world, with the OnAdd / OnEquip / OnUnequip
//! events of scripted items.

use super::{none, yes, Args, Ctx};
use crate::game::{event, Game};
use crate::trace::EventKind;
use fnv_script::vm::Value;

/// `Container.AddItem item count [silent]`: leveled lists resolve as in Layer 4; each
/// item added gets OnAdd on its script [hypothesis for OnAdd from AddItem].
pub fn add_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let to = ctx.subject()?;
    let item = a.form(0)?;
    let count = a.int(1)?;
    let added = g.world.add_item(to, item, count).map_err(|e| e.to_string())?;
    for (i, n) in added {
        g.emit(EventKind::ItemAdded { to, item: i, count: n });
        g.item_event(to, i, event::ON_ADD, Some(Value::Ref(to.0)));
    }
    none()
}

pub fn remove_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let from = ctx.subject()?;
    let item = a.form(0)?;
    let count = a.int(1)?;
    let n = g.world.remove_item(from, item, count).map_err(|e| e.to_string())?;
    if n > 0 {
        g.emit(EventKind::ItemRemoved { from, item, count: n });
    }
    none()
}

pub fn get_item_count(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let of = ctx.subject()?;
    let item = a.form(0)?;
    Ok(Value::Num(g.world.item_count(of, item).map_err(|e| e.to_string())? as f64))
}

/// `RemoveAllItems [target]`: into the target container, or destroyed.
pub fn remove_all_items(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let from = ctx.subject()?;
    let stacks = match a.opt_form(0) {
        Some(to) => {
            let moved = g.world.transfer_all(from, to).map_err(|e| e.to_string())?;
            for &(item, count) in &moved {
                g.emit(EventKind::ItemAdded { to, item, count });
            }
            moved
        }
        None => g.world.remove_all_items(from).map_err(|e| e.to_string())?,
    };
    for (item, count) in stacks {
        g.emit(EventKind::ItemRemoved { from, item, count });
    }
    none()
}

pub fn equip_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let item = a.form(0)?;
    let e = g.world.equip(actor, item).map_err(|e| e.to_string())?;
    for u in e.unequipped {
        g.item_event(actor, u, event::ON_UNEQUIP, Some(Value::Ref(actor.0)));
    }
    if !e.consumed {
        g.item_event(actor, item, event::ON_EQUIP, Some(Value::Ref(actor.0)));
    }
    none()
}

pub fn unequip_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let item = a.form(0)?;
    if g.world.unequip(actor, item).map_err(|e| e.to_string())? {
        g.item_event(actor, item, event::ON_UNEQUIP, Some(Value::Ref(actor.0)));
    }
    none()
}

pub fn get_equipped(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let item = a.form(0)?;
    let s = g.world.get(actor).map_err(|e| e.to_string())?;
    yes(s.inventory.as_ref().is_some_and(|i| i.is_equipped(item)))
}
