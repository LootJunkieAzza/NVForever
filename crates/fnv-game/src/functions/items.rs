//! Inventory functions over the Layer 4 world, with the OnAdd / OnEquip / OnUnequip
//! events of scripted items.

use super::{none, yes, Args, Ctx};
use crate::game::{event, Game, InstKey};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// The items an item parameter stands for: the form itself, or every member of a form
/// list [documented: GECK wiki AddItem and RemoveItem notes, a form list applies the
/// count to each item in it].
fn items(g: &Game, item: FormId) -> Vec<FormId> {
    if g.data.sig(item).as_ref() == Some(b"FLST") {
        super::refs::list_members(g, item)
    } else {
        vec![item]
    }
}

/// `Container.AddItem item count [silent]`: leveled lists resolve as in Layer 4; OnAdd
/// runs on the item's script [documented: GECK wiki AddItem, once per item in the count;
/// here once per call, hypothesis].
pub fn add_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let to = ctx.subject()?;
    let count = a.int(1)?;
    for item in items(g, a.form(0)?) {
        let added = g.world.add_item(to, item, count).map_err(|e| e.to_string())?;
        for (i, n) in added {
            g.emit(EventKind::ItemAdded { to, item: i, count: n });
            g.item_event(to, i, event::ON_ADD, Some(Value::Ref(to.0)));
        }
    }
    none()
}

pub fn remove_item(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let from = ctx.subject()?;
    let count = a.int(1)?;
    for item in items(g, a.form(0)?) {
        let n = g.world.remove_item(from, item, count).map_err(|e| e.to_string())?;
        if n > 0 {
            g.emit(EventKind::ItemRemoved { from, item, count: n });
        }
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

/// The condition of the equipped weapon in percent [documented: GECK wiki
/// GetWeaponHealthPerc]: its stack's condition, 100 for an undamaged one or with no
/// weapon equipped [hypothesis for "no weapon": the capture saves never reached the
/// tutorial stage that fires below 25 although a new character starts unarmed,
/// layer5-captures.md §2 (CGTutorial stage 218 not done)].
pub fn get_weapon_health_perc(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let forms = g.world.forms;
    let s = g.world.get(actor).map_err(|e| e.to_string())?;
    let cond = s.inventory.as_ref().and_then(|inv| {
        inv.groups()
            .find(|(item, e, _)| e.equipped && forms.item(*item).is_some_and(|i| i.kind == fnv_sim::forms::ItemKind::Weapon))
            .map(|(_, e, _)| e.condition.unwrap_or(1.0))
    });
    Ok(Value::Num(cond.unwrap_or(1.0) as f64 * 100.0))
}

/// `RemoveMe [container]`: the calling inventory item leaves the container holding it
/// (one of the stack); with a container, a copy goes there. It acts like a return
/// [documented: catalogue summary and notes of RemoveMe]. The item's script instance is
/// dropped when the holder has none of the item left (a later copy starts fresh
/// [hypothesis: a copy does not keep the variables, catalogue notes]).
pub fn remove_me(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let Some(key @ InstKey::Item { holder, item }) = ctx.inst else {
        return Err("RemoveMe outside the script of an inventory item".into());
    };
    let n = g.world.remove_item(holder, item, 1).map_err(|e| e.to_string())?;
    if n > 0 {
        g.emit(EventKind::ItemRemoved { from: holder, item, count: n });
    }
    if let Some(to) = a.opt_form(0) {
        for (i, c) in g.world.add_item(to, item, 1).map_err(|e| e.to_string())? {
            g.emit(EventKind::ItemAdded { to, item: i, count: c });
        }
    }
    if g.world.item_count(holder, item).unwrap_or(0) == 0 {
        g.instances.remove(&key);
    }
    g.request_return();
    none()
}
