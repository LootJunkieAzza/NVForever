//! xNVSE's ammo, item-condition and inventory-reference functions, and some reference
//! odds and ends (docs/handoff/xnvse-remaining.md, the "other" group) [documented: their
//! pages].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn int(args: &[XVal], k: usize, dflt: i64) -> i64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x as i64,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

impl Game<'_> {
    /// The ammo a weapon can load: its NAM0, an AMMO or a form list of them.
    pub(crate) fn weapon_ammo(&self, weapon: FormId) -> Vec<FormId> {
        let a = match crate::nvse::record_value(self, weapon, "NAM0", "") {
            Some(XVal::Ref(r)) if r != 0 => FormId(r),
            _ => return Vec::new(),
        };
        if self.data.sig(a) == Some(*b"FLST") {
            crate::functions::list_members(self, a)
        } else {
            vec![a]
        }
    }

    /// The ammo an actor's weapon uses: the first of its ammo the actor carries, else the
    /// first (the engine's rule, fnv-combat `CombatForms::ammo_for`).
    pub(crate) fn ammo_in_use(&mut self, actor: FormId) -> Option<FormId> {
        let w = crate::functions::items::equipped_weapon(self, actor)?;
        let choices = self.weapon_ammo(w);
        let carried = |g: &mut Game, a: FormId| g.world.inventory_mut(actor).is_ok_and(|inv| inv.count(a) > 0);
        let mut first = None;
        for a in choices {
            first = first.or(Some(a));
            if carried(self, a) {
                return Some(a);
            }
        }
        first
    }

    /// An item's full health (DATA health).
    fn full_health(&self, item: FormId) -> f64 {
        match crate::nvse::record_value(self, item, "DATA", "health") {
            Some(XVal::Num(x)) => x,
            _ => 0.0,
        }
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let v = match name {
        // Ammo [documented: GetWeaponCanUseAmmo; HasAmmoEquipped;
        // GetEquippedWeaponCanUseAmmo; IsEquippedAmmoInList; GetEquippedWeaponUsesAmmoList;
        // GetPlayerCurrentAmmo].
        "getweaponcanuseammo" => {
            // The weapon and the ammo, in either order [hypothesis: the page lists one].
            let (a, b) = (form(args, 0), form(args, 1).or(subject));
            let (w, ammo) = match (a, b) {
                (Some(x), Some(y)) if g.data.sig(base(g, x)) == Some(*b"WEAP") => (base(g, x), y),
                (Some(x), Some(y)) => (base(g, y), x),
                _ => return Some(Ok(XVal::Num(0.0))),
            };
            yes(g.weapon_ammo(w).contains(&ammo))
        }
        "hasammoequipped" => {
            let a = subject.unwrap_or(FormId(0));
            let want = form(args, 0);
            yes(want.is_some() && g.ammo_in_use(a) == want)
        }
        "getequippedweaponcanuseammo" => {
            let a = subject.unwrap_or(FormId(0));
            let w = crate::functions::items::equipped_weapon(g, a);
            yes(w.zip(form(args, 0)).is_some_and(|(w, ammo)| g.weapon_ammo(w).contains(&ammo)))
        }
        "isequippedammoinlist" => {
            let a = subject.unwrap_or(FormId(0));
            let l = crate::functions::list_members(g, form(args, 0).unwrap_or(FormId(0)));
            yes(g.ammo_in_use(a).is_some_and(|x| l.contains(&x)))
        }
        "getequippedweaponusesammolist" => {
            let a = subject.unwrap_or(FormId(0));
            let w = crate::functions::items::equipped_weapon(g, a);
            let nam0 = w.and_then(|w| crate::nvse::record_value(g, w, "NAM0", ""));
            yes(matches!((nam0, form(args, 0)), (Some(XVal::Ref(r)), Some(l)) if r == l.0))
        }
        "getplayercurrentammo" => XVal::Ref(g.ammo_in_use(FormId::PLAYER_REF).map_or(0, |a| a.0)),
        // Condition [documented: GetEquippedCurrentHealth; GetCurrentHealth;
        // SetCurrentHealth; SetEquippedCurrentHealth]: health points, the item's full
        // health times its condition.
        "getequippedcurrenthealth" | "setequippedcurrenthealth" => {
            let set = name.starts_with("set");
            let a = subject.unwrap_or(FormId(0));
            let slot = int(args, usize::from(set), -1);
            let item = equipped_in_slot(g, a, slot);
            let Some(item) = item else { return Some(Ok(XVal::Num(0.0))) };
            let full = g.full_health(item);
            if set {
                let hp = match args.first() {
                    Some(XVal::Num(x)) => *x,
                    _ => 0.0,
                };
                let c = if full > 0.0 { (hp / full) as f32 } else { 1.0 };
                if let Ok(inv) = g.world.inventory_mut(a) {
                    inv.map_equipped(item, |mut e| {
                        e.condition = Some(c.clamp(0.0, 1.0));
                        e
                    });
                }
                XVal::Num(0.0)
            } else {
                let c = g.world.inventory_mut(a).ok().and_then(|inv| inv.condition_in_use(item)).unwrap_or(1.0);
                XVal::Num(full * f64::from(c))
            }
        }
        "getcurrenthealth" | "setcurrenthealth" => {
            let r = subject.unwrap_or(FormId(0));
            let Some((c, item)) = g.temp_item(r) else {
                // A world reference: its base's full health [hypothesis: XHLT not read].
                return Some(Ok(if name == "getcurrenthealth" { XVal::Num(g.full_health(base(g, r))) } else { XVal::Num(0.0) }));
            };
            let full = g.full_health(item);
            let Ok(inv) = g.world.inventory_mut(c) else { return Some(Ok(XVal::Num(0.0))) };
            let group = inv.groups().find(|(i, _, _)| *i == item).map(|(_, e, _)| e);
            if name == "getcurrenthealth" {
                XVal::Num(full * f64::from(group.map_or(1.0, |e| e.condition_or_whole())))
            } else {
                let hp = match args.first() {
                    Some(XVal::Num(x)) => *x,
                    _ => 0.0,
                };
                let to = if full > 0.0 { (hp / full) as f32 } else { 1.0 };
                if let Some(e) = group {
                    inv.set_condition_one(item, e, to.clamp(0.0, 1.0));
                }
                XVal::Num(0.0)
            }
        }
        // Setters [documented: SetIsPlayable; SetQuestItem] `flag [item]`.
        "setisplayable" => {
            let it = form(args, 1).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            g.playable_overrides.insert(it, args.first().is_some_and(XVal::truthy));
            XVal::Num(0.0)
        }
        "setquestitem" => {
            let it = form(args, 1).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            g.world.set_quest_object(it, args.first().is_some_and(XVal::truthy));
            XVal::Num(0.0)
        }
        // Inventory references [documented: CopyIR; CopyIRAlt; RemoveMeIR; EquipMe;
        // UnequipMe; CreateTempRef]: one stands for a container's whole stack of an item.
        "copyir" | "copyiralt" => {
            let (Some((c, item)), Some(to)) = (subject.and_then(|r| g.temp_item(r)), form(args, 0)) else {
                return Some(Ok(if name == "copyir" { XVal::Num(0.0) } else { XVal::Ref(0) }));
            };
            let n = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(1).max(1);
            let ok = g.world.add_item(to, item, n).is_ok();
            if name == "copyir" { yes(ok) } else { XVal::Ref(if ok { g.temp_ref(to, item).0 } else { 0 }) }
        }
        "removemeir" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(XVal::Num(0.0))) };
            let n = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(0);
            let gone = g.world.remove_item(c, item, n).unwrap_or(0);
            if let (Some(to), true) = (form(args, 0), gone > 0) {
                let _ = g.world.add_item(to, item, gone);
            }
            XVal::Num(0.0)
        }
        "equipme" | "unequipme" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(XVal::Num(0.0))) };
            let f = if name == "equipme" { "EquipItem" } else { "UnequipItem" };
            if let Err(e) = crate::functions::console::call_words(g, Some(c), f, &[format!("{:08X}", item.0), "1".into()], "EquipMe") {
                return Some(Err(e));
            }
            XVal::Num(0.0)
        }
        // [documented: EquipItem2] EquipItem with its OnEquip block run.
        "equipitem2" => {
            let a = subject.unwrap_or(FormId(0));
            let Some(item) = form(args, 0) else { return Some(Ok(XVal::Num(0.0))) };
            let words: Vec<String> = std::iter::once(format!("{:08X}", base(g, item).0)).chain(args.iter().skip(1).map(|v| format!("{}", v.num().unwrap_or(0.0) as i64))).collect();
            if let Err(e) = crate::functions::console::call_words(g, Some(a), "EquipItem", &words, "EquipItem2") {
                return Some(Err(e));
            }
            XVal::Num(0.0)
        }
        // [documented: CreateTempRef] an inventory reference in no container, for one
        // frame.
        "createtempref" => match form(args, 0) {
            Some(f) => XVal::Ref(g.temp_ref(FormId(0), base(g, f)).0),
            None => XVal::Ref(0),
        },
        // [documented: BuildRef; GetLocalRefIndex].
        "buildref" => {
            let id = ((int(args, 0, -1) as u32) << 24) | (int(args, 1, 0) as u32 & 0x00FF_FFFF);
            let ok = int(args, 0, -1) >= 0 && (g.data.sig(FormId(id)).is_some() || id == FormId::PLAYER_REF.0);
            XVal::Ref(if ok { id } else { 0 })
        }
        "getlocalrefindex" => XVal::Num(f64::from(form(args, 0).or(subject).map_or(0, |f| f.0 & 0x00FF_FFFF))),
        // [documented: GetSelfAlt] the calling reference, or none.
        "getselfalt" => XVal::Ref(ctx.self_ref.map_or(0, |r| r.0)),
        // [documented: GetGridsToLoad] uGridsToLoad: 5 [hypothesis: the game's default,
        // NVForever has no such setting].
        "getgridstoload" => XVal::Num(5.0),
        // [documented: GetInGrid; GetInGridInCell] the target within `depth` grid squares
        // of the centre cell (the player's) [hypothesis: depth defaults to the loaded
        // grid's half, 2].
        "getingrid" | "getingridincell" => {
            let k = usize::from(name == "getingridincell");
            let center = if k == 1 { form(args, 0) } else { g.cell_of(FormId::PLAYER_REF) };
            let target = form(args, k);
            let depth = int(args, k + 1, 2);
            yes(match (center, target.and_then(|t| g.cell_of(t))) {
                (Some(c), Some(t)) => c == t || g.cells_within(c, depth).contains(&t),
                _ => false,
            })
        }
        // [documented: GetHeadingAngleX] the angle from the reference's heading to the
        // target, -180 to 180 [hypothesis: as GetHeadingAngle, on any reference].
        "getheadinganglex" => {
            let (Some(r), Some(t)) = (subject, form(args, 0)) else { return Some(Ok(XVal::Num(0.0))) };
            let (Some(a), Some(b)) = (g.ref_pos_rot(r), g.ref_pos_rot(t)) else { return Some(Ok(XVal::Num(0.0))) };
            let to = (b.0[0] - a.0[0]).atan2(b.0[1] - a.0[1]).to_degrees();
            let mut d = to - a.1[2].to_degrees();
            while d > 180.0 {
                d -= 360.0;
            }
            while d < -180.0 {
                d += 360.0;
            }
            XVal::Num(f64::from(d))
        }
        // [documented: GetLevCharacterRefs; GetLevCreatureRefs] the leveled list's entries
        // as an array of forms [hypothesis: forms only, without levels].
        "getlevcharacterrefs" | "getlevcreaturerefs" => {
            let l = form(args, 0).unwrap_or(FormId(0));
            let items: Vec<XVal> = g.data.leveled_actors.get(&l).map(|d| d.entries.iter().map(|(_, f)| XVal::Ref(f.0)).collect()).unwrap_or_default();
            let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        // [documented: CompareScripts] the two scripts' compiled bytes are the same.
        "comparescripts" => {
            let s = |k: usize| form(args, k).and_then(|f| g.data.script(f)).map(|c| c.unit.bytes.clone());
            yes(matches!((s(0), s(1)), (Some(a), Some(b)) if a == b))
        }
        // [documented: ResetAllVariables] every variable of the reference's (or quest's)
        // script to 0.
        "resetallvariables" => {
            let r = subject.unwrap_or(FormId(0));
            g.reset_variables(r);
            XVal::Num(0.0)
        }
        _ => return None,
    };
    Some(Ok(v))
}

impl Game<'_> {
    /// A reference's position and rotation (radians).
    pub(crate) fn ref_pos_rot(&mut self, r: FormId) -> Option<([f32; 3], [f32; 3])> {
        self.world.get(r).ok().map(|s| (s.pos, s.rot))
    }

    /// The cells within `depth` grid squares of an exterior cell (the cell itself only
    /// indoors).
    pub(crate) fn cells_within(&self, cell: FormId, depth: i64) -> Vec<FormId> {
        self.scan_cells(cell, depth)
    }

    /// Every variable of a reference's (or a quest's) script to 0 (ResetAllVariables).
    pub(crate) fn reset_variables(&mut self, r: FormId) {
        if let Some((key, _)) = self.instance_of(r) {
            if let Some(i) = self.instances.get_mut(&key) {
                i.vars.clear();
            }
        }
    }
}

/// The item an actor wears in an equipment slot: 5 the weapon, else the worn item whose
/// BMDT biped flags include the slot (as GetEquippedObject).
fn equipped_in_slot(g: &mut Game, actor: FormId, slot: i64) -> Option<FormId> {
    if slot == 5 {
        return crate::functions::items::equipped_weapon(g, actor);
    }
    let worn = g.world.inventory_mut(actor).map(|inv| inv.equipped()).unwrap_or_default();
    worn.into_iter().find(|&i| match crate::nvse::record_value(g, i, "BMDT", "biped_flags") {
        Some(XVal::Num(bits)) => (0..32).contains(&slot) && (bits as u64) & (1 << slot) != 0,
        _ => false,
    })
}
