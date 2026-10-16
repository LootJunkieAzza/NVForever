//! Inventory functions over the Layer 4 world, with the OnAdd / OnEquip / OnUnequip
//! events of scripted items.

use super::{none, yes, Args, Ctx};
use crate::game::{event, Game, InstKey};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::forms::ItemKind;
use fnv_sim::inventory::Extra;
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

/// `Container.GetItemCount item`: how many are carried; for a form list, the sum over its
/// members [documented: GECK wiki GetItemCount notes]. Notes do not count ("does not
/// work with Notes" [documented: same notes]; observed 0 for a holotape just added,
/// capture `fa_batch3_actions`).
pub fn get_item_count(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let of = ctx.subject()?;
    let mut n = 0;
    for item in items(g, a.form(0)?) {
        if g.data.sig(item).as_ref() == Some(b"NOTE") {
            continue;
        }
        n += g.world.item_count(of, item).map_err(|e| e.to_string())?;
    }
    Ok(Value::Num(n as f64))
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
    if e.consumed {
        // An aid item is consumed: it is used as from the inventory (Game::consume), with
        // potency and the addiction roll [documented: GECK wiki EquipItem, "the item is
        // treated as though it were used normally"].
        g.emit(EventKind::ItemRemoved { from: actor, item, count: 1 });
        g.ingest(actor, item);
    } else {
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

/// `actor.GetEquipped item`: 1 when the actor has the item equipped; for a form list,
/// when any member is [documented: GECK wiki GetEquipped notes]. An NPC has its outfit
/// and weapon equipped once it has been loaded (`fnv_sim::World::outfit`).
pub fn get_equipped(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let wanted = items(g, a.form(0)?);
    let s = g.world.get(actor).map_err(|e| e.to_string())?;
    yes(s.inventory.as_ref().is_some_and(|i| wanted.iter().any(|&w| i.is_equipped(w))))
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

/// The weapon an actor has equipped, if any.
pub fn equipped_weapon(g: &mut Game, actor: FormId) -> Option<FormId> {
    let forms = g.world.forms;
    let s = g.world.get(actor).ok()?;
    let inv = s.inventory.as_ref()?;
    inv.equipped().into_iter().find(|&i| forms.item(i).is_some_and(|it| it.kind == ItemKind::Weapon))
}

/// Inventory form types by the codes RemoveAllTypedItems takes. 40 is the weapons
/// [documented: GECK wiki RemoveAllTypedItems example]; the other codes follow the
/// form type numbering published for this engine by the script extender community
/// [hypothesis: not checked against a source we can cite here; the shipped scripts pass
/// 24, 26, 31, 40, 41 and 47 together to empty an inventory, observed].
pub fn form_type_code(sig: &[u8; 4]) -> Option<i32> {
    Some(match sig {
        b"ARMO" => 24,
        b"BOOK" => 25,
        b"INGR" => 29,
        b"LIGH" => 30,
        b"MISC" => 31,
        b"WEAP" => 40,
        b"AMMO" => 41,
        b"KEYM" => 46,
        b"ALCH" => 47,
        b"NOTE" => 49,
        _ => return None,
    })
}

/// `container.RemoveAllTypedItems [target] [retainOwnership] [suppressMessages]
/// [typeCode] [exceptionList]` [documented: GECK wiki RemoveAllTypedItems]: the items
/// of the form type (all with no code or −1, none with 0) leave the container, except
/// non-playable biped items, members of the exception list and, from the player, quest
/// items. With a target they move there keeping their condition (and their owner with
/// the retain flag, otherwise the owner is cleared); without one they are destroyed.
pub fn remove_all_typed_items(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let from = ctx.subject()?;
    let to = a.opt_form(0);
    let retain = a.num_or(1, 0.0) != 0.0;
    let code = a.num(3).map(|c| c as i32).unwrap_or(-1);
    if code == 0 {
        return none();
    }
    let except = a.opt_form(4).map(|l| super::refs::list_members(g, l)).unwrap_or_default();
    let forms = g.world.forms;
    let stacks: Vec<(FormId, i32)> = {
        let s = g.world.get(from).map_err(|e| e.to_string())?;
        s.inventory.as_ref().map(|i| i.iter().collect()).unwrap_or_default()
    };
    for (item, n) in stacks {
        let Some(it) = forms.item(item) else { continue };
        if it.non_playable || except.contains(&item) {
            continue;
        }
        if from == FormId::PLAYER_REF && g.world.is_quest_item(item) {
            continue;
        }
        if code != -1 && forms.sig(item).as_ref().and_then(form_type_code) != Some(code) {
            continue;
        }
        let groups = fnv_sim::inventory::unequipped(g.world.inventory_mut(from).map_err(|e| e.to_string())?.take(item, n));
        g.emit(EventKind::ItemRemoved { from, item, count: n });
        if let Some(to) = to {
            for (mut extra, c) in groups {
                if !retain {
                    extra.owner = None;
                }
                g.world.add_item_extra(to, item, c, extra).map_err(|e| e.to_string())?;
            }
            g.emit(EventKind::ItemAdded { to, item, count: n });
        }
    }
    none()
}

/// `container.AddItemHealthPercent item count health [hidden]`: AddItem with the items
/// at that condition (a fraction, 0.85 for 85%) [documented: catalogue notes]. A
/// leveled list resolves as AddItem and keeps its own condition [hypothesis].
pub fn add_item_health_percent(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let to = ctx.subject()?;
    let item = a.form(0)?;
    let count = a.int(1)?;
    let health = a.num(2)?.clamp(0.0, 1.0) as f32;
    if g.world.forms.leveled(item).is_some() {
        return add_item(g, ctx, &Args(&a.0[..2]));
    }
    let extra = Extra { condition: (health < 1.0).then_some(health), ..Extra::default() };
    g.world.add_item_extra(to, item, count, extra).map_err(|e| e.to_string())?;
    g.emit(EventKind::ItemAdded { to, item, count });
    g.item_event(to, item, event::ON_ADD, Some(Value::Ref(to.0)));
    none()
}

fn set_weapon_condition(g: &mut Game, actor: FormId, f: impl Fn(f32) -> f32) -> Result<(), String> {
    let w = equipped_weapon(g, actor).ok_or("no weapon equipped")?;
    let inv = g.world.inventory_mut(actor).map_err(|e| e.to_string())?;
    inv.map_equipped(w, |e| {
        let c = f(e.condition.unwrap_or(1.0)).clamp(0.0, 1.0);
        Extra { condition: (c < 1.0).then_some(c), ..e }
    });
    Ok(())
}

/// `actor.ModWeaponHealthPerc amount`: the equipped weapon's condition changes by
/// `amount` percentage points [hypothesis: the unit of GetWeaponHealthPerc], within
/// 0–100.
pub fn mod_weapon_health_perc(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let d = a.num(0)? as f32 / 100.0;
    set_weapon_condition(g, actor, |c| c + d)?;
    none()
}

/// `actor.SetWeaponHealthPerc percent` (the 99.83% cap the catalogue reports is an
/// original-engine quirk, not reproduced without a lab case).
pub fn set_weapon_health_perc(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let v = a.num(0)? as f32 / 100.0;
    set_weapon_condition(g, actor, |_| v)?;
    none()
}

/// 1 when the actor's equipped weapon is in the list [documented: catalogue summary].
pub fn is_weapon_in_list(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let actor = ctx.subject()?;
    let list = a.form(0)?;
    let Some(w) = equipped_weapon(g, actor) else { return yes(false) };
    yes(super::refs::list_members(g, list).contains(&w))
}

/// 1 when the weapon (the subject itself when it is a weapon, else the subject's
/// equipped weapon [hypothesis for the second case]) uses the given skill (WEAP DNAM
/// skill) [documented: catalogue summary].
pub fn is_weapon_skill_type(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let skill = a.code(0)? as i64;
    let base = super::base_of(g, r);
    let w = match base.filter(|b| g.data.sig(*b).as_ref() == Some(b"WEAP")) {
        Some(b) => Some(b),
        None => equipped_weapon(g, r),
    };
    yes(w.and_then(|w| g.data.weapon_skills.get(&w)).copied() == Some(skill))
}

/// `ref.ResetInventory`: a fresh roll of the base's items replaces the inventory
/// [documented: catalogue summary].
pub fn reset_inventory(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.world.reset_inventory(r).map_err(|e| e.to_string())?;
    none()
}

/// The number of items the container holds [documented: catalogue summary].
pub fn get_container_inventory_count(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let s = g.world.get(r).map_err(|e| e.to_string())?;
    Ok(Value::Num(s.inventory.as_ref().map(|i| i.iter().map(|(_, n)| n as f64).sum()).unwrap_or(0.0)))
}

/// `item.SetItemValue value`: the base item's value for every reference [documented:
/// catalogue summary]; kept with the form changes.
pub fn set_item_value(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let base = super::base_of(g, r).unwrap_or(r);
    g.forms.item_values.insert(base, a.int(0)?);
    none()
}

/// `SetQuestObject form flag`: the form's quest-item flag [documented: catalogue
/// summary]. It replaces the record's "Quest Item" header flag in the simulation
/// (`fnv_sim::World::set_quest_object`, the one source of truth, saved with the world):
/// quest items stay with the player in RemoveAllTypedItems and the player cannot drop
/// them by hand (a scripted `Drop` still can, as in the game). `FormChanges::quest_items`
/// is no longer written.
pub fn set_quest_object(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let f = a.form(0)?;
    g.world.set_quest_object(f, a.num_or(1, 1.0) != 0.0);
    none()
}

/// `AddItemToLeveledList list form level count health`: from now on `form` (an item or
/// a leveled list) is an entry of the leveled item list `list` at `level`, `count` at a
/// time; kept in the saved state and never taken back [documented: GECK wiki
/// AddItemToLeveledList] (`fnv_sim::World::add_to_leveled_list`). The health (0–100 in
/// New Vegas [documented]) is not kept: leveled entries here carry no condition, and the
/// shipped calls all ask for 1 or 100 [observed: the 20 decoded calls of Honest Hearts
/// and Old World Blues]. Count and level below 1 are read as 1 [hypothesis].
pub fn add_item_to_leveled_list(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let list = a.form(0)?;
    let item = a.form(1)?;
    let level = a.num_or(2, 1.0).clamp(1.0, u16::MAX as f64) as u16;
    let count = a.num_or(3, 1.0).clamp(1.0, u16::MAX as f64) as u16;
    g.world.add_to_leveled_list(list, fnv_sim::forms::LeveledEntry { level, item, count }).map_err(|e| e.to_string())?;
    none()
}
