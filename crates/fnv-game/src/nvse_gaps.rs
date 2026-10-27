//! The most called extender functions the mod lists use that no earlier set built
//! (docs/handoff/modlist-function-counts.json) [documented: their pages; observed: the
//! peer's gaps2, gaps3 and gaps4 sets in the original game]. Record reads come from the
//! schema's decoded fields (`GameData::record_fields`, enums as their numbers).

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// This file's session state.
#[derive(Debug, Default)]
pub struct GapsState {
    /// SetCombatDisabled's actors.
    pub combat_disabled: HashSet<FormId>,
    /// The Pip-Boy light, and when a script turned it off: the light reads off from the
    /// next frame on [observed: gaps2, `TogglePipBoyLight 0` still reads 1 in its frame
    /// and 0 in the next; on reads 1 at once].
    pub pipboy_light: bool,
    pub light_off_at: Option<f64>,
    /// SetRefName's names, by reference; GetName does not read them [observed: gaps4].
    pub ref_names: HashMap<FormId, String>,
    /// SetActorGravityMult's multipliers.
    pub gravity: HashMap<FormId, f64>,
    /// ToggleBipedSlotVisibility's hidden slots.
    pub hidden_slots: HashSet<u32>,
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn json_form(v: Option<&Json>) -> XVal {
    match v.and_then(Json::as_str) {
        Some(s) if s.starts_with("0x") => XVal::Ref(u32::from_str_radix(&s[2..], 16).unwrap_or(0)),
        _ => XVal::Ref(0),
    }
}

fn json_num(v: Option<&Json>) -> f64 {
    v.and_then(|v| v.get("value").filter(|_| v.get("set").is_some()).unwrap_or(v).as_f64()).unwrap_or(0.0)
}

/// A list array of the values.
fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

/// The bit `bit` of a flags number.
fn bit(flags: f64, bit: f64) -> XVal {
    yes((0.0..64.0).contains(&bit) && (flags as u64) >> (bit as u64) & 1 != 0)
}

impl Game<'_> {
    /// The Pip-Boy light as scripts see it (`GapsState::light_off_at`).
    fn pipboy_light(&mut self) -> bool {
        if self.gaps.light_off_at.is_some_and(|t| self.time > t) {
            self.gaps.pipboy_light = false;
            self.gaps.light_off_at = None;
        }
        self.gaps.pipboy_light
    }
}

/// WEAP sound IDs of GetWeaponSound: the subrecord and which of its repeats [observed:
/// gaps4, 0–2 the 10mm pistol's two SNAM then XNAM, 3 none on it; hypothesis: the rest in
/// the record's order of sound fields].
fn weapon_sound_sub(id: i64) -> Option<(&'static str, usize)> {
    Some(match id {
        0 => ("SNAM", 0),
        1 => ("SNAM", 1),
        2 => ("XNAM", 0),
        3 => ("NAM7", 0),
        4 => ("TNAM", 0),
        5 => ("NAM6", 0),
        6 => ("UNAM", 0),
        7 => ("NAM9", 0),
        8 => ("NAM8", 0),
        9 => ("WMS1", 0),
        10 => ("WMS1", 1),
        11 => ("WMS2", 0),
        _ => return None,
    })
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- magic items and effects ----
        // The item's effects in record order [observed: gaps2, the stimpak 4, the first
        // RestoreHealth, past the end 0].
        "getnumeffects" => XVal::Num(form(args, 0).and_then(|f| g.data.effect_items.get(&f)).map_or(0, Vec::len) as f64),
        "getntheffectbase" => {
            let i = num(args, 1, 0.0);
            let e = form(args, 0).and_then(|f| g.data.effect_items.get(&f)).and_then(|l| if i >= 0.0 { l.get(i as usize) } else { None });
            XVal::Ref(e.map_or(0, |e| e.effect.0))
        }
        // ENIT flags, bit by bit [observed: gaps3, the stimpak's flags 5 read 1 0 1].
        "geteffectflag" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            bit(json_num(g.data.record_field(f, "ENIT", "flags")), num(args, 1, -1.0))
        }
        // MGEF DATA [observed: gaps3, RestoreHealth archetype 0, actor value 16, flags
        // 112 so bits 0–3 read 0].
        "getbaseeffectarchetype" => XVal::Num(form(args, 0).and_then(|f| g.data.magic_effects.get(&f)).map_or(0, |m| m.archetype) as f64),
        "getbaseeffectav" => XVal::Num(form(args, 0).and_then(|f| g.data.magic_effects.get(&f)).and_then(|m| m.actor_value).map_or(-1.0, f64::from)),
        "getbaseeffectflag" => {
            let flags = form(args, 0).and_then(|f| g.data.magic_effects.get(&f)).map_or(0, |m| m.flags);
            bit(f64::from(flags), num(args, 1, -1.0))
        }
        // ---- ammunition ----
        // AMMO RCIL in record order [observed: gaps2, 10mm JHP hand load 2, the first
        // 00160C45; plain 10mm 0].
        "getnumammoeffects" => XVal::Num(form(args, 0).map_or(0, |f| g.data.record_all(f, "RCIL").len()) as f64),
        "getnthammoeffect" => {
            let i = num(args, 1, 0.0);
            let all = form(args, 0).map(|f| g.data.record_all(f, "RCIL")).unwrap_or_default();
            if i >= 0.0 { json_form(all.get(i as usize).copied()) } else { XVal::Ref(0) }
        }
        // AMEF DATA: 0 the type, 1 the operation, 2 the value [observed: gaps2, the DT
        // reduction (DT Mod, Subtract, 2) reads 2 2 2, trait 3 reads 0].
        "getammoeffecttraitnumeric" => {
            let f = form(args, 0).unwrap_or(FormId(0));
            let key = match num(args, 1, -1.0) as i64 {
                0 => "type",
                1 => "operation",
                2 => "value",
                _ => return Some(Ok(zero)),
            };
            XVal::Num(json_num(g.data.record_field(f, "DATA", key)))
        }
        // ---- globals ----
        "getglobalvariable" => XVal::Num(form(args, 0).and_then(|f| g.world.global(f).ok()).map_or(0.0, f64::from)),
        // Returns 0 [observed: gaps3, TimeScale set to 12 reads back 12].
        "setglobalvariable" => {
            if let Some(f) = form(args, 0) {
                let _ = g.world.set_global(f, num(args, 1, 0.0) as f32);
            }
            zero
        }
        // ---- perks ----
        // PERK DATA: 0 trait, 1 playable, 2 hidden [observed: gaps2, Toughness reads
        // 0 1 0 0 0; hypothesis: the order of the flags on the page, 3 and on none].
        "getperkflag" => {
            let p = form(args, 0).and_then(|f| g.data.perks.get(&f));
            yes(p.is_some_and(|p| match num(args, 1, -1.0) as i64 {
                0 => p.is_trait,
                1 => p.playable,
                2 => p.hidden,
                _ => false,
            }))
        }
        // [observed: gaps3, Toughness 2 entries]
        "getperkentrycount" => XVal::Num(form(args, 0).and_then(|f| g.data.perks.get(&f)).map_or(0, |p| p.entries.len()) as f64),
        // An entry point reads 1 [observed: gaps3, Toughness's first; hypothesis: quest
        // stage 0 and ability 2].
        "getnthperkentrytype" => {
            let e = form(args, 0).and_then(|f| g.data.perks.get(&f)).and_then(|p| p.entries.get(num(args, 1, 0.0).max(0.0) as usize));
            XVal::Num(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::EntryPoint(_)) => 1.0,
                Some(crate::data::PerkEffect::Ability(_)) => 2.0,
                Some(_) => 0.0,
                None => -1.0,
            })
        }
        // The entry point's code [observed: gaps4, Toughness's first 56].
        "getnthperkentryfunction" => {
            let e = form(args, 0).and_then(|f| g.data.perks.get(&f)).and_then(|p| p.entries.get(num(args, 1, 0.0).max(0.0) as usize));
            XVal::Num(match e.map(|e| &e.effect) {
                Some(crate::data::PerkEffect::EntryPoint(d)) => f64::from(d.point),
                _ => -1.0,
            })
        }
        // ---- base forms ----
        // CREA DATA type; not a creature -1 [observed: gaps2, the street brahmin 1
        // (Mutated Animal), Doc -1].
        "getcreaturetype" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            if g.data.sig(b) == Some(*b"CREA") { XVal::Num(json_num(g.data.record_field(b, "DATA", "type"))) } else { XVal::Num(-1.0) }
        }
        // [observed: gaps4, the street brahmin's PNAM 00015833]
        "getbodypartdata" => {
            let b = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            if g.data.sig(b) == Some(*b"CREA") { json_form(g.data.record_field(b, "PNAM", "")) } else { XVal::Ref(0) }
        }
        // YNAM [observed: gaps2, the 10mm pistol 000864D5, the stimpak none].
        "getpickupsound" => json_form(form(args, 0).and_then(|f| g.data.record_field(f, "YNAM", ""))),
        // DEST stages [observed: gaps3, the 10mm pistol 0].
        "getdestructiondatanumstages" => XVal::Num(form(args, 0).and_then(|f| g.data.destructibles.get(&f)).map_or(0, |d| d.stages.len()) as f64),
        // ---- weapons ----
        // ShowOff's index into DNAM flags1 [observed: gaps4, the 10mm pistol reads 0 on
        // 0–5 and SetWeaponFlag 5 1 reads back 1; hypothesis: flags1's bits].
        "weaponhasflag" | "setweaponflag" => {
            let set = name == "setweaponflag";
            let w = form(args, if set { 2 } else { 1 }).or_else(|| subject.and_then(|r| crate::functions::items::equipped_weapon(g, r)));
            let Some(w) = w else { return Some(Ok(zero)) };
            let flags = match crate::nvse::record_value(g, w, "DNAM", "flags1") {
                Some(XVal::Num(x)) => x as u64,
                _ => 0,
            };
            let k = num(args, 0, -1.0);
            if !(0.0..32.0).contains(&k) {
                return Some(Ok(zero));
            }
            if !set {
                return Some(Ok(yes(flags >> (k as u64) & 1 != 0)));
            }
            let new = if num(args, 1, 0.0) != 0.0 { flags | 1 << (k as u64) } else { flags & !(1 << (k as u64)) };
            g.field_overrides.insert((w, "DNAM/flags1".into()), XVal::Num(new as f64));
            zero
        }
        "getweaponsound" | "setweaponsound" => {
            let w = form(args, 0).unwrap_or(FormId(0));
            let id = num(args, 1, -1.0) as i64;
            let Some((sub, k)) = weapon_sound_sub(id) else { return Some(Ok(XVal::Ref(0))) };
            let key = (w, format!("weaponsound/{id}"));
            if name == "setweaponsound" {
                g.field_overrides.insert(key, XVal::Ref(form(args, 2).map_or(0, |f| f.0)));
                return Some(Ok(zero));
            }
            match g.field_overrides.get(&key) {
                Some(v) => v.clone(),
                None => json_form(g.data.record_all(w, sub).get(k).copied()),
            }
        }
        // JohnnyGuitar's DPS: damage at the actor's skill (0.5 + skill / 200) times shots
        // a second [observed: gaps2, the 10mm pistol (22 damage, 2.75 shots) reads 41.44;
        // hypothesis: the skill factor, which gives that with Guns 37].
        "getcalculatedweapondps" => {
            let w = form(args, 0).or_else(|| subject.and_then(|r| crate::functions::items::equipped_weapon(g, r))).unwrap_or(FormId(0));
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            let damage = json_num(g.data.record_field(w, "DATA", "base_damage"));
            let shots = json_num(g.data.record_field(w, "DNAM", "attack_shots_per_sec"));
            let skill = g.data.weapon_skills.get(&w).and_then(|&s| crate::functions::actor::actor_value(g, a, s as u16).ok()).unwrap_or(0.0);
            XVal::Num(damage * (0.5 + skill / 200.0) * shots)
        }
        // ---- containers and inventories ----
        // `item count base` on the base's carried items [documented: BaseRemoveItem].
        "baseremoveitem" | "baseadditem" => {
            let (Some(item), Some(b)) = (form(args, 0), form(args, 2)) else { return Some(Ok(zero)) };
            let n = num(args, 1, 1.0).max(1.0) as i32;
            let add = name == "baseadditem";
            g.world.edit_base_entries(b, |e| {
                if add {
                    e.push(fnv_sim::forms::Entry::new(item, n));
                    return;
                }
                let mut left = n;
                for x in e.iter_mut().filter(|x| x.item == item) {
                    let take = left.min(x.count);
                    x.count -= take;
                    left -= take;
                }
                e.retain(|x| x.count > 0);
            });
            zero
        }
        // The weight of the carried items, with 1 only the playable ones [observed:
        // gaps2, the player 93 either way].
        "getinventoryweight" => {
            let a = subject.unwrap_or(FormId::PLAYER_REF);
            XVal::Num(g.world.carried_weight(a).unwrap_or(0.0))
        }
        // The base items carried, one each, with a type code only that type [observed:
        // gaps2, 27; 40 (weapons) 8].
        "getallitems" => {
            let a = subject.unwrap_or(FormId(0));
            let want = num(args, 0, 0.0) as i32;
            let items: Vec<FormId> = g.world.inventory_mut(a).map(|inv| inv.iter().filter(|&(_, n)| n > 0).map(|(i, _)| i).collect()).unwrap_or_default();
            let items: Vec<XVal> = items
                .into_iter()
                .filter(|&i| want == 0 || g.data.sig(i).and_then(|s| crate::functions::items::form_type_code(&s)) == Some(want))
                .map(|i| XVal::Ref(i.0))
                .collect();
            list(g, items)
        }
        // The inventory reference of what is in the slot [observed: gaps2, an empty slot
        // reads 0].
        "getequippeditemref" => {
            let a = subject.unwrap_or(FormId(0));
            match crate::nvse::call(g, ctx, "getequippedobject", subject, args) {
                Some(Ok(XVal::Ref(i))) if i != 0 => XVal::Ref(g.temp_ref(a, FormId(i)).0),
                _ => XVal::Ref(0),
            }
        }
        // `item [count] [clearOwnership]`: as Drop [documented: DropAlt; observed: gaps3,
        // nine stimpaks leave eight].
        "dropalt" => {
            let (Some(a), Some(item)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let n = g.world.item_count(a, item).unwrap_or(0).min(num(args, 1, 1.0).max(1.0) as i32);
            if n > 0 {
                if let Ok(made) = g.world.drop_item(a, item, n) {
                    g.emit(crate::trace::EventKind::ItemRemoved { from: a, item, count: n, silent: false });
                    for r in made {
                        crate::functions::objects::note_created(g, r);
                    }
                }
            }
            zero
        }
        // ---- references and actors ----
        // On the full name of the base [observed: gaps2, GetName reads it on the
        // reference and the base].
        "setfullnamealt" => {
            let b = form(args, 1).or(subject).map(|f| base(g, f));
            if let Some(b) = b {
                g.name_overrides.insert(b, text(args, 0));
            }
            zero
        }
        "setrefname" => {
            if let Some(r) = subject {
                g.gaps.ref_names.insert(r, text(args, 0));
            }
            zero
        }
        // Returns 0 either way [observed: gaps2]; GetCombatDisabled reads it [observed:
        // gaps3].
        "setcombatdisabled" => {
            if let Some(r) = subject {
                if num(args, 0, 0.0) != 0.0 {
                    g.gaps.combat_disabled.insert(r);
                } else {
                    g.gaps.combat_disabled.remove(&r);
                }
            }
            zero
        }
        "getcombatdisabled" => yes(subject.is_some_and(|r| g.gaps.combat_disabled.contains(&r))),
        // The reference's (or the base's) script has a block of that ID [observed: gaps2,
        // Doc's GameMode (0) reads 1; hypothesis: the IDs are the compiled block codes].
        "hasscriptblock" => {
            let s = form(args, 1).or(subject).and_then(|f| g.ref_script(f).or_else(|| g.base_script(base(g, f))));
            let ev = num(args, 0, -1.0);
            yes(ev >= 0.0 && s.and_then(|s| g.data.script(s)).is_some_and(|u| u.has_block(ev as u16)))
        }
        // In other cells the distance is not a number to compare [observed: gaps2,
        // Floor of it reads -2147483648]: the largest float [hypothesis].
        "getdistance2d" => {
            let (Some(a), Some(b)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let same = match (g.cell_of(a), g.cell_of(b)) {
                (Some(x), Some(y)) => x == y || (g.world.placed.world_of(x).is_some() && g.world.placed.world_of(x) == g.world.placed.world_of(y)),
                _ => false,
            };
            match (same, g.ref_pos_rot(a), g.ref_pos_rot(b)) {
                (true, Some((p, _)), Some((q, _))) => XVal::Num(f64::from((p[0] - q[0]).hypot(p[1] - q[1]))),
                _ => XVal::Num(f64::from(f32::MAX)),
            }
        }
        // The player's teammates; none is no array [observed: gaps2, ar_Null].
        "getteammates" => {
            let t: Vec<XVal> = g.extra.iter().filter(|(_, e)| e.teammate).map(|(r, _)| XVal::Ref(r.0)).collect();
            if t.is_empty() { zero } else { list(g, t) }
        }
        // 0 the actors in the loaded cells, 3 the others; 1 and 2 none here, no array
        // [observed: gaps3, 16, ar_Null, ar_Null, over 1000; hypothesis: NVForever runs
        // every loaded actor fully].
        "getactorsbyprocessinglevel" => {
            let high = g.package_actors();
            let actors: Vec<XVal> = match num(args, 0, -1.0) as i64 {
                0 => high.iter().map(|r| XVal::Ref(r.0)).collect(),
                3 => g.world.placed.refs.iter().filter(|(r, p)| matches!(&p.sig, b"ACHR" | b"ACRE") && !high.contains(r)).map(|(r, _)| XVal::Ref(r.0)).collect(),
                _ => Vec::new(),
            };
            if actors.is_empty() { zero } else { list(g, actors) }
        }
        // The references of a base: in the loaded cells, persistent ones anywhere
        // [observed: gaps4, no loaded stimpak 0, Doc's base with exteriors 1; hypothesis:
        // the persistent ones count].
        "getformrefs" => {
            let Some(b) = form(args, 0) else { return Some(Ok(zero)) };
            let loaded: Vec<FormId> = g.loaded_cells.iter().copied().collect();
            let mut refs: Vec<FormId> = loaded.iter().flat_map(|&c| g.refs_in_cell(c, false)).filter(|&r| base(g, r) == b).collect();
            refs.extend(g.world.placed.refs.iter().filter(|(_, p)| p.base == b && (p.in_persistent_group || p.flags & fnv_sim::refs::FLAG_PERSISTENT != 0)).map(|(&r, _)| r));
            refs.sort();
            refs.dedup();
            let refs = refs.into_iter().map(|r| XVal::Ref(r.0)).collect();
            list(g, refs)
        }
        "isincombatwith" => yes(subject.zip(form(args, 0)).is_some_and(|(a, t)| g.combat_target_of(a) == Some(t))),
        "setactorgravitymult" => {
            if let Some(r) = subject {
                g.gaps.gravity.insert(r, num(args, 0, 1.0));
            }
            zero
        }
        // ---- keywords ----
        // The base forms with the keyword [observed: gaps3, one after AssignKeyword, an
        // unknown keyword an empty array].
        "getkeywordforms" => {
            let k = text(args, 0).to_ascii_lowercase();
            let mut forms: Vec<FormId> = g.jip2.keywords.iter().filter(|(_, s)| s.contains(&k)).map(|(&f, _)| f).collect();
            forms.sort();
            let forms = forms.into_iter().map(|f| XVal::Ref(f.0)).collect();
            list(g, forms)
        }
        // ---- cells, climates, recipes ----
        // CELL DATA flags bit by bit [observed: gaps3, Doc's house (Interior, Public)
        // reads 1 0 0 0].
        "getcellflag" => {
            let c = form(args, 0).unwrap_or(FormId(0));
            bit(json_num(g.data.record_field(c, "DATA", "")), num(args, 1, -1.0))
        }
        // XCIM [observed: gaps4, Doc's house 0001507A].
        "getcellimagespace" => json_form(form(args, 0).and_then(|c| g.data.record_field(c, "XCIM", ""))),
        // CLMT WLST [observed: gaps4, the Strip's climate 1 weather, 00161241, global
        // none, past the end 0].
        "getclimatenumweathertypes" => XVal::Num(form(args, 0).and_then(|c| g.data.record_field(c, "WLST", "")).and_then(Json::as_array).map_or(0, Vec::len) as f64),
        "getclimatenthweathertype" | "getclimatenthweatherglobal" | "getclimatenthweatherchance" => {
            let i = num(args, 1, -1.0);
            let all = form(args, 0).and_then(|c| g.data.record_field(c, "WLST", "")).and_then(Json::as_array);
            let e = all.and_then(|l| if i >= 0.0 { l.get(i as usize) } else { None });
            match name {
                "getclimatenthweathertype" => json_form(e.and_then(|e| e.get("weather"))),
                "getclimatenthweatherglobal" => json_form(e.and_then(|e| e.get("global"))),
                _ => XVal::Num(json_num(e.and_then(|e| e.get("chance")))),
            }
        }
        // Night outside the climate's sunrise begin to sunset end (TNAM, ten-minute steps)
        // [observed: gaps4, the Strip's climate at 10 reads 0; hypothesis: the edges].
        "isnight" => {
            let c = form(args, 0).unwrap_or(FormId(0));
            let hour = g.world.global(FormId(0x38)).map_or(12.0, f64::from);
            let t = |k: &str| json_num(g.data.record_field(c, "TNAM", k)) / 6.0;
            let (rise, set) = (t("sunrise_begin"), t("sunset_end"));
            yes(rise != set && (hour < rise || hour >= set))
        }
        "getrecipeoutputforms" => {
            let outs: Vec<XVal> = form(args, 0).and_then(|f| g.data.recipes.get(&f)).map(|r| r.outputs.iter().map(|(f, _)| XVal::Ref(f.0)).collect()).unwrap_or_default();
            list(g, outs)
        }
        "getactivequest" => XVal::Ref(g.ui.active_quest.map_or(0, |q| q.0)),
        // ---- the player and the interface ----
        // [observed: gaps2, off at the start; on at once, off from the next frame]
        "togglepipboylight" => {
            match args.first() {
                Some(XVal::Num(x)) if *x != 0.0 => {
                    g.gaps.pipboy_light = true;
                    g.gaps.light_off_at = None;
                }
                Some(XVal::Num(_)) if g.gaps.pipboy_light && g.gaps.light_off_at.is_none() => g.gaps.light_off_at = Some(g.time),
                _ => {}
            }
            yes(g.pipboy_light())
        }
        // `slot [toggle]`: the slot's visibility, 1 shown [observed: gaps3, slot 2 reads 1].
        "togglebipedslotvisibility" => {
            let slot = num(args, 0, -1.0).max(0.0) as u32;
            if let Some(XVal::Num(t)) = args.get(1) {
                if *t == 0.0 {
                    g.gaps.hidden_slots.insert(slot);
                } else {
                    g.gaps.hidden_slots.remove(&slot);
                }
            }
            yes(!g.gaps.hidden_slots.contains(&slot))
        }
        // Nothing to reload: the engine draws the equipped models as they are.
        "reloadequippedmodels" => zero,
        // NVForever is no 32-bit program [observed: gaps4, 0].
        "getislaa" => zero,
        // The open menu's code, 0 in game mode [observed: gaps3, 0].
        "getactivemenumode" => XVal::Num(g.menu.map_or(0.0, f64::from)),
        // [observed: gaps3, 1]
        "getglobaltimemultiplier" => XVal::Num(g.ui.time_multiplier.unwrap_or(1.0)),
        // No water under the crosshair here [observed: gaps2, 0; hypothesis: the engine
        // tells no crosshair water].
        "getcrosshairwater" => XVal::Ref(0),
        // Return 0 [observed: gaps4].
        "setsoundflag" | "stopsoundalt" => zero,
        // An unknown patch [observed: gaps2 and gaps4, 0 for both].
        "getoptionalpatch" | "setoptionalpatch" => zero,
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_sound_ids_follow_the_observed_order() {
        assert_eq!(weapon_sound_sub(0), Some(("SNAM", 0)));
        assert_eq!(weapon_sound_sub(1), Some(("SNAM", 1)));
        assert_eq!(weapon_sound_sub(2), Some(("XNAM", 0)));
        assert_eq!(weapon_sound_sub(12), None);
    }

    #[test]
    fn flag_bits() {
        assert_eq!(bit(5.0, 0.0), XVal::Num(1.0));
        assert_eq!(bit(5.0, 1.0), XVal::Num(0.0));
        assert_eq!(bit(5.0, 2.0), XVal::Num(1.0));
        assert_eq!(bit(5.0, -1.0), XVal::Num(0.0));
    }
}
