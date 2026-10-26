//! xNVSE's (and FOSE's) functions that read or change the game, called by name from the
//! script VM through `Host::ext_call` (the remaining-xNVSE phase,
//! docs/handoff/xnvse-remaining.md). The VM keeps the pure ones (strings, arrays, math);
//! these need the game's records, inventories and lists. Behaviour from the wiki export's
//! pages [documented: <page>], record fields from the Layer 1 schema; the rest is marked.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// A record field of `form`: a number, a form or a string (`GameData::record_field`).
fn field(g: &Game, form: FormId, sub: &str, path: &str) -> Option<XVal> {
    use serde_json::Value as Json;
    Some(match g.data.record_field(form, sub, path)? {
        Json::Number(n) => XVal::Num(n.as_f64()?),
        Json::String(s) if s.starts_with("0x") => XVal::Ref(u32::from_str_radix(&s[2..], 16).ok()?),
        Json::String(s) => XVal::Str(s.clone()),
        _ => return None,
    })
}

fn num_of(v: Option<XVal>) -> XVal {
    match v {
        Some(XVal::Num(x)) => XVal::Num(x),
        _ => XVal::Num(0.0),
    }
}

fn form_of(v: Option<XVal>) -> XVal {
    match v {
        Some(XVal::Ref(r)) => XVal::Ref(r),
        _ => XVal::Ref(0),
    }
}

fn as_form(v: Option<&XVal>) -> Option<FormId> {
    match v? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn int_of(v: Option<&XVal>, dflt: i64) -> i64 {
    match v {
        Some(XVal::Num(x)) => *x as i64,
        _ => dflt,
    }
}

/// The WEAP fields of the GetWeapon… getters [documented: each page names the trait;
/// the field is the schema's of that name, schemas/fnv].
fn weapon_field(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "getweaponcliprounds" => ("DATA", "clip_size"),
        "getattackdamage" => ("DATA", "base_damage"),
        "getweaponammo" => ("NAM0", ""),
        "getweaponammouse" => ("DNAM", "ammo_use"),
        "getweaponactionpoints" => ("DNAM", "override_action_points"),
        "getweaponaimarc" => ("DNAM", "aim_arc"),
        "getweaponanimattackmult" => ("DNAM", "animation_attack_multiplier"),
        "getweaponanimjamtime" => ("DNAM", "jam_time"),
        "getweaponanimmult" => ("DNAM", "animation_multiplier"),
        "getweaponanimreloadtime" => ("DNAM", "reload_time"),
        "getweaponanimshotspersec" => ("DNAM", "attack_shots_per_sec"),
        "getweaponattackanimation" => ("DNAM", "attack_animation"),
        "getweaponbasevatschance" => ("DNAM", "base_vats_to_hit_chance"),
        "getweaponcritchance" => ("CRDT", "crit_percent_mult"),
        "getweaponcritdamage" => ("CRDT", "critical_damage"),
        "getweaponcriteffect" => ("CRDT", "effect"),
        "getweaponfiredelaymin" => ("DNAM", "semi_automatic_fire_delay_min"),
        "getweaponfiredelaymax" => ("DNAM", "semi_automatic_fire_delay_max"),
        "getweaponfirerate" => ("DNAM", "fire_rate"),
        "getweaponflags1" => ("DNAM", "flags1"),
        "getweaponflags2" => ("DNAM", "flags2"),
        "getweaponhandgrip" => ("DNAM", "grip_animation"),
        "getweaponlimbdamagemult" => ("DNAM", "limb_dmg_mult"),
        "getweaponmaxrange" => ("DNAM", "max_range"),
        "getweaponminrange" => ("DNAM", "min_range"),
        "getweaponminspread" => ("DNAM", "min_spread"),
        "getweaponnumprojectiles" => ("DNAM", "projectile_count"),
        "getweaponprojectile" => ("DNAM", "projectile"),
        "getweaponreach" => ("DNAM", "reach"),
        "getweaponregenrate" => ("DNAM", "regen_rate"),
        "getweaponreloadanim" => ("DNAM", "reload_animation"),
        "getweaponrequiredskill" => ("DNAM", "skill_req"),
        "getweaponrequiredstrength" => ("DNAM", "strength_req"),
        "getweaponresisttype" => ("DNAM", "resist_type"),
        "getweaponrumbleduration" => ("DNAM", "rumble_duration"),
        "getweaponrumbleleftmotor" => ("DNAM", "rumble_left_motor_strength"),
        "getweaponrumblerightmotor" => ("DNAM", "rumble_right_motor_strength"),
        "getweaponrumblewavelength" => ("DNAM", "rumble_wavelength"),
        "getweaponsightfov" => ("DNAM", "sight_fov"),
        "getweaponsightusage" => ("DNAM", "sight_usage"),
        "getweaponskill" => ("DNAM", "skill"),
        "getweaponspread" => ("DNAM", "spread"),
        "getweapontype" => ("DNAM", "animation_type"),
        _ => return None,
    })
}

impl Game<'_> {
    /// The members of a form list, with the session's ListAddForm/ListRemove… edits.
    fn edited_list(&mut self, list: FormId) -> &mut Vec<FormId> {
        if !self.list_edits.contains_key(&list) {
            let now = crate::functions::list_members(self, list);
            self.list_edits.insert(list, now);
        }
        self.list_edits.get_mut(&list).expect("inserted above")
    }
}

/// An xNVSE function by catalogue name (lower case); `None` when it is not built here.
/// `on` is the reference it was called on, else the calling reference.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, on: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    if let Some(r) = crate::delayed::call(g, ctx, name, on, args) {
        return Some(r);
    }
    let subject = on.or(ctx.self_ref);
    if let Some(r) = crate::nvse_refs::call(g, name, subject, args) {
        return Some(r);
    }
    // The item a getter reads: its argument, else the calling reference's base form.
    let item = |g: &Game, k: usize| -> Option<FormId> {
        as_form(args.get(k)).or(subject).map(|f| crate::functions::base_of(g, f).unwrap_or(f))
    };
    if let Some((sub, path)) = weapon_field(name) {
        let w = item(g, 0).unwrap_or(FormId(0));
        let v = field(g, w, sub, path);
        return Some(Ok(match name {
            "getweaponammo" | "getweaponprojectile" | "getweaponcriteffect" => form_of(v),
            _ => num_of(v),
        }));
    }
    let v = match name {
        // [documented: GetWeaponIsAutomatic, GetWeaponHasScope, GetWeaponLongBursts] flag
        // bits of DNAM (schema WeaponDNAM: flags1 bit 1 Is Automatic, bit 2 Has Scope;
        // flags2 bit 11 Long Burst).
        "getweaponisautomatic" | "getweaponhasscope" | "getweaponlongbursts" => {
            let w = item(g, 0).unwrap_or(FormId(0));
            let (sub, bit) = match name {
                "getweaponisautomatic" => ("flags1", 1),
                "getweaponhasscope" => ("flags1", 2),
                _ => ("flags2", 11),
            };
            let bits = match field(g, w, "DNAM", sub) {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            XVal::Num(f64::from(u8::from(bits & (1 << bit) != 0)))
        }
        // [documented: GetWeight; GetItemValue; GetBaseHealth; GetArmorArmorRating;
        // GetArmorDT] the item's DATA / DNAM fields; ALCH keeps its weight in DATA and
        // its value in ENIT.
        "getweight" | "getitemvalue" | "getbasehealth" | "getarmorarmorrating" | "getarmordt" => {
            let f = item(g, 0).unwrap_or(FormId(0));
            let alch = g.data.sig(f) == Some(*b"ALCH");
            let (sub, path) = match (name, alch) {
                ("getweight", true) => ("DATA", ""),
                ("getweight", false) => ("DATA", "weight"),
                ("getitemvalue", true) => ("ENIT", "value"),
                ("getitemvalue", false) => ("DATA", "value"),
                ("getbasehealth", _) => ("DATA", "health"),
                ("getarmorarmorrating", _) => ("DNAM", "dr"),
                _ => ("DNAM", "dt"),
            };
            num_of(field(g, f, sub, path))
        }
        // [documented: GetAmmoSpeed; GetAmmoConsumedPercent; GetAmmoCasing].
        "getammospeed" | "getammoconsumedpercent" | "getammocasing" => {
            let a = item(g, 0).unwrap_or(FormId(0));
            match name {
                "getammospeed" => num_of(field(g, a, "DATA", "speed")),
                "getammoconsumedpercent" => num_of(field(g, a, "DAT2", "consumed_percentage")),
                _ => form_of(field(g, a, "DAT2", "consumed_ammo")),
            }
        }
        // [documented: GetName] the full name of a reference's base or a form; "" none.
        "getname" => {
            let f = as_form(args.first()).or(subject).unwrap_or(FormId(0));
            let names = &g.data.full_names;
            let n = names.get(&f).cloned().or_else(|| crate::functions::base_of(g, f).and_then(|b| names.get(&b).cloned()));
            XVal::Str(n.unwrap_or_default())
        }
        // [documented: GetParentCell] the cell the reference is in.
        "getparentcell" => XVal::Ref(subject.and_then(|r| g.cell_of(r)).map_or(0, |c| c.0)),
        // [documented: GetInventoryObject] the base form of the container's nth kind of
        // item, in FormID order [hypothesis: the order is not documented]; 0 past the end.
        "getinventoryobject" => {
            let c = subject.unwrap_or(FormId(0));
            let n = int_of(args.first(), 0);
            let items: Vec<FormId> = g.world.inventory_mut(c).map(|inv| inv.iter().filter(|(_, k)| *k > 0).map(|(i, _)| i).collect()).unwrap_or_default();
            XVal::Ref(usize::try_from(n).ok().and_then(|n| items.get(n)).map_or(0, |f| f.0))
        }
        // [documented: GetEquippedObject] the item in an equipment slot: 5 the weapon,
        // otherwise the worn item whose biped flags (BMDT) include the slot.
        "getequippedobject" => {
            let a = subject.unwrap_or(FormId(0));
            let slot = int_of(args.first(), -1);
            if slot == 5 {
                XVal::Ref(crate::functions::items::equipped_weapon(g, a).map_or(0, |w| w.0))
            } else {
                let worn = g.world.inventory_mut(a).map(|inv| inv.equipped()).unwrap_or_default();
                let hit = worn.into_iter().find(|&i| match field(g, i, "BMDT", "biped_flags") {
                    Some(XVal::Num(bits)) => (0..32).contains(&slot) && (bits as u64) & (1 << slot) != 0,
                    _ => false,
                });
                XVal::Ref(hit.map_or(0, |f| f.0))
            }
        }
        // Form lists [documented: ListGetNthForm; ListGetFormIndex; ListAddForm;
        // ListRemoveForm; ListRemoveNthForm; ListClear]. The changes last for the session:
        // "Forms added to a list with this function will not persist across savegames"
        // [documented: ListAddForm].
        "listgetnthform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let n = int_of(args.get(1), 0);
            let m = crate::functions::list_members(g, l);
            XVal::Ref(usize::try_from(n).ok().and_then(|n| m.get(n)).map_or(0, |f| f.0))
        }
        "listgetformindex" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let f = as_form(args.get(1));
            let m = crate::functions::list_members(g, l);
            XVal::Num(f.and_then(|f| m.iter().position(|&x| x == f)).map_or(-1.0, |i| i as f64))
        }
        "listaddform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let Some(f) = as_form(args.get(1)) else { return Some(Ok(XVal::Num(-1.0))) };
            let at = int_of(args.get(2), -2);
            let dupes = args.get(3).is_some_and(XVal::truthy);
            let list = g.edited_list(l);
            if dupes && list.contains(&f) {
                return Some(Ok(XVal::Num(-1.0)));
            }
            // -2 or past the end: at the end [documented]; another negative index fails
            // [hypothesis].
            let i = match usize::try_from(at) {
                Ok(i) if i <= list.len() => i,
                _ if at == -2 => list.len(),
                Ok(_) => list.len(),
                Err(_) => return Some(Ok(XVal::Num(-1.0))),
            };
            list.insert(i, f);
            XVal::Num(i as f64)
        }
        "listremoveform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let f = as_form(args.get(1));
            let list = g.edited_list(l);
            match f.and_then(|f| list.iter().position(|&x| x == f)) {
                Some(i) => {
                    list.remove(i);
                    XVal::Num(i as f64)
                }
                None => XVal::Num(-1.0),
            }
        }
        // Without an index, the last form [hypothesis: the page gives the index as
        // optional without a default].
        "listremoventhform" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            let list = g.edited_list(l);
            let n = int_of(args.get(1), list.len() as i64 - 1);
            match usize::try_from(n).ok().filter(|&n| n < list.len()) {
                Some(n) => XVal::Ref(list.remove(n).0),
                None => XVal::Ref(0),
            }
        }
        "listclear" => {
            let l = as_form(args.first()).unwrap_or(FormId(0));
            g.edited_list(l).clear();
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}
