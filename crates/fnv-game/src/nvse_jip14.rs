//! JIP LN NVSE, batch 14 (#1006): the last of JIP's functions. Effects added to magic
//! items, primitives, the auxiliary-variable condition, IsSpellTargetList, DismemberLimb,
//! FireWeaponEx, PlayIdleEx, PlaySound3DNode, per-reference models, texture slots, VATS
//! targets, the gun wobble, the projectiles' fuses and weapons, GetSelfAsInventoryRef,
//! animation sequence frequencies, and the functions left as nothing values by decision
//! (the extra cameras, AttachLine, HasKeywordCond, FreePlayer, UpdatePlayer3D)
//! [documented: each page in the wiki export; docs/spec-notes/jip-last-14.md].

use crate::data::EffectItem;
use crate::game::{Game, RunCtx};
use crate::nif_blocks::node_place;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// A primitive a script gave or changed: sphere or box (or a collision plane), half
/// extents (a sphere's radius first).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptPrimitive {
    /// 1 box, 2 sphere, 3 plane [documented: AddPrimitive].
    pub shape: u8,
    pub bounds: [f32; 3],
}

#[derive(Debug, Default)]
pub struct Jip14State {
    /// AddNewEffect's entries, by magic item: those put at the top (newest first) and
    /// those appended.
    pub added_effects: BTreeMap<FormId, (Vec<EffectItem>, Vec<EffectItem>)>,
    /// AddPrimitive's primitives and SetPrimitiveBound's changes, by reference.
    pub primitives: BTreeMap<FormId, ScriptPrimitive>,
    /// SetRefrModelPath's models (`None`: the base's again), by reference.
    pub ref_models: BTreeMap<FormId, Option<String>>,
    /// SetVATSTargetable 0's actors, for the session [documented: "targetable again once
    /// a saved game is loaded"].
    pub vats_off: BTreeSet<FormId>,
    /// The engine's VATS targets while VATS is up.
    pub vats_targets: Vec<FormId>,
    /// SetNoGunWobble's actors.
    pub no_wobble: BTreeSet<FormId>,
    /// The projectiles' fuses (seconds left) as the engine tells them, and the scripts'
    /// new fuses for it.
    pub fuses: BTreeMap<FormId, f32>,
    pub fuse_sets: Vec<(FormId, f32)>,
    /// SetAnimSequenceFrequency's frequencies, by reference and lower-case sequence ("*":
    /// every sequence).
    pub anim_freq: BTreeMap<(FormId, String), f32>,
    /// The item whose script runs now (its holder and base), for GetSelfAsInventoryRef.
    pub item_running: Option<(FormId, FormId)>,
}

fn num(args: &[XVal], k: usize) -> Option<f64> {
    match args.get(k) {
        Some(XVal::Num(x)) if x.is_finite() => Some(*x),
        _ => None,
    }
}

fn text(args: &[XVal], k: usize) -> Option<String> {
    match args.get(k) {
        Some(XVal::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn axis(args: &[XVal], k: usize) -> Option<usize> {
    match args.get(k)? {
        XVal::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
            "x" => Some(0),
            "y" => Some(1),
            "z" => Some(2),
            _ => None,
        },
        XVal::Num(x) if (0.0..3.0).contains(x) => Some(*x as usize),
        XVal::Num(x) if [88.0, 89.0, 90.0, 120.0, 121.0, 122.0].contains(x) => Some(((*x as u32 - 88) % 32) as usize),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

fn list(g: &mut Game, items: Vec<XVal>) -> XVal {
    if items.is_empty() {
        return XVal::Array(0);
    }
    let entries = items.into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}

const MAGIC_ITEMS: [&[u8; 4]; 4] = [b"ALCH", b"ENCH", b"SPEL", b"INGR"];

impl<'a> Game<'a> {
    /// A magic item's effects as scripts list them: AddNewEffect's at the top, newest
    /// first, the record's, AddNewEffect's appended.
    pub fn effect_list(&self, item: FormId) -> std::borrow::Cow<'a, [EffectItem]> {
        let data: &'a crate::data::GameData = self.data;
        let own = data.effect_items.get(&item).map(Vec::as_slice).unwrap_or(&[]);
        match self.other.jip14.added_effects.get(&item) {
            None => std::borrow::Cow::Borrowed(own),
            Some((front, back)) => std::borrow::Cow::Owned(front.iter().rev().chain(own).chain(back).cloned().collect()),
        }
    }

    /// A magic item's effects as they are applied: the record's at their own places (the
    /// active effects keep a record index), then AddNewEffect's [decision: the order the
    /// effects take hold in is not the list's].
    pub fn applied_effects(&self, item: FormId) -> std::borrow::Cow<'a, [EffectItem]> {
        let data: &'a crate::data::GameData = self.data;
        let own = data.effect_items.get(&item).map(Vec::as_slice).unwrap_or(&[]);
        match self.other.jip14.added_effects.get(&item) {
            None => std::borrow::Cow::Borrowed(own),
            Some((front, back)) => std::borrow::Cow::Owned(own.iter().chain(front.iter().rev()).chain(back).cloned().collect()),
        }
    }

    /// AddNewEffect's entries of an item: at the top, appended.
    pub fn added_effect_counts(&self, item: FormId) -> (usize, usize) {
        self.other.jip14.added_effects.get(&item).map_or((0, 0), |(f, b)| (f.len(), b.len()))
    }

    /// The base effect of a list entry in the scripts' order (AddNewEffect's top entries
    /// first); `None` for an entry of the record's (the caller reads it) or none.
    pub fn added_effect_at(&self, item: FormId, i: usize) -> Option<Result<FormId, usize>> {
        let own = self.data.effect_items.get(&item).map_or(0, Vec::len);
        let (front, back) = match self.other.jip14.added_effects.get(&item) {
            Some((f, b)) => (f.as_slice(), b.as_slice()),
            None => (&[][..], &[][..]),
        };
        if i < front.len() {
            Some(Ok(front[front.len() - 1 - i].effect))
        } else if i < front.len() + own {
            Some(Err(i - front.len()))
        } else {
            back.get(i - front.len() - own).map(|e| Ok(e.effect))
        }
    }

    /// A reference's primitive as the scripts left it, if they gave or changed one:
    /// whether a sphere and its half extents.
    pub fn script_primitive(&self, r: FormId) -> Option<(bool, [f32; 3])> {
        self.other.jip14.primitives.get(&r).filter(|p| p.shape != 3).map(|p| (p.shape == 2, p.bounds))
    }

    /// SetRefrModelPath's model of a reference (under meshes): `Some(None)` its base's.
    pub fn ref_model(&self, r: FormId) -> Option<Option<&str>> {
        self.other.jip14.ref_models.get(&r).map(|m| m.as_deref())
    }

    /// The references SetRefrModelPath changed.
    pub fn ref_models(&self) -> &BTreeMap<FormId, Option<String>> {
        &self.other.jip14.ref_models
    }

    /// Whether VATS may mark the actor (SetVATSTargetable).
    pub fn actor_vats_targetable(&self, r: FormId) -> bool {
        !self.other.jip14.vats_off.contains(&r)
    }

    /// The engine's VATS targets while VATS is up (empty when it is not).
    pub fn set_vats_targets(&mut self, targets: Vec<FormId>) {
        self.other.jip14.vats_targets = targets;
    }

    /// Whether SetNoGunWobble switched the actor's gun wobble off.
    pub fn gun_wobble_off(&self, r: FormId) -> bool {
        self.other.jip14.no_wobble.contains(&r)
    }

    /// A projectile's fuse as the engine has it (seconds left; `None`: no fuse).
    pub fn set_fuse(&mut self, r: FormId, left: Option<f32>) {
        match left {
            Some(s) => self.other.jip14.fuses.insert(r, s),
            None => self.other.jip14.fuses.remove(&r),
        };
    }

    /// SetDetonationTimer's fuses, for the engine.
    pub fn take_fuse_sets(&mut self) -> Vec<(FormId, f32)> {
        std::mem::take(&mut self.other.jip14.fuse_sets)
    }

    /// SetAnimSequenceFrequency's frequency for a reference's sequence (a lower-case
    /// name or kf path), else "*"'s; `None`: its own.
    pub fn anim_frequency(&self, r: FormId, sequence: &str) -> Option<f32> {
        let j = &self.other.jip14.anim_freq;
        j.get(&(r, sequence.to_ascii_lowercase())).or_else(|| j.get(&(r, "*".to_string()))).copied()
    }
}

pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // ---- magic items ----
        // A new entry at the top, or appended (1), of an ingestible's, object effect's or
        // actor effect's list; the new count, -1 when not added [documented]; for the
        // session [hypothesis: the page says nothing of saving].
        "addneweffect" => {
            let (Some(item), Some(mgef), Some(dur), Some(mag)) = (form(args, 0), form(args, 1), num(args, 2), num(args, 3)) else { return Some(Ok(XVal::Num(-1.0))) };
            let kind_ok = g.data.sig(item).is_some_and(|s| MAGIC_ITEMS.contains(&&s));
            if !kind_ok || g.data.sig(mgef) != Some(*b"MGEF") {
                return Some(Ok(XVal::Num(-1.0)));
            }
            // [decision: the range is not kept, as NVForever's effect entries have none]
            let e = EffectItem { effect: mgef, magnitude: mag.max(0.0) as u32, area: num(args, 5).unwrap_or(0.0).max(0.0) as u32, duration: dur.max(0.0) as u32, actor_value: None, conditions: Vec::new() };
            let entry = g.other.jip14.added_effects.entry(item).or_default();
            if num(args, 6).unwrap_or(0.0) != 0.0 {
                entry.1.push(e);
            } else {
                entry.0.push(e);
            }
            XVal::Num(g.effect_list(item).len() as f64)
        }
        // Any effect of the list's items on the actor [documented], as IsSpellTarget.
        "isspelltargetlist" => {
            let (Some(r), Some(l)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let items = crate::functions::list_members(g, l);
            g.sync_actor_effects(r);
            yes(g.effects.iter().any(|e| e.target == r && items.contains(&e.source)))
        }
        // ---- primitives ----
        // On an activator, a box or sphere trigger; on a CollisionMarker, a collision box,
        // sphere or plane [documented]. NVForever's triggers take it (`nvf_play::
        // script_trigger`); a CollisionMarker's is kept [decision: its collider is not
        // built, a follow-up].
        "addprimitive" => {
            let (Some(r), Some(t), Some(x), Some(y), Some(z)) = (subject, num(args, 0), num(args, 1), num(args, 2), num(args, 3)) else { return Some(Ok(zero)) };
            let shape = t as u8;
            let base = crate::functions::base_of(g, r);
            let acti = base.and_then(|b| g.data.sig(b)) == Some(*b"ACTI");
            let marker = base.and_then(|b| g.data.edid_of.get(&b)).is_some_and(|e| e.eq_ignore_ascii_case("CollisionMarker"));
            if !(1..=3).contains(&shape) || !(acti && shape != 3 || marker) {
                return Some(Ok(zero));
            }
            g.other.jip14.primitives.insert(r, ScriptPrimitive { shape, bounds: [x as f32, y as f32, z as f32] });
            yes(true)
        }
        "hasprimitive" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            yes(g.other.jip14.primitives.contains_key(&r) || g.world.placed.get(r).is_some_and(|p| p.primitive.is_some()))
        }
        // The bound on the axis [documented]; half extents as the record keeps them
        // [hypothesis: the page does not say half or whole].
        "getprimitivebound" | "setprimitivebound" => {
            let (Some(r), Some(a)) = (subject, axis(args, 0)) else { return Some(Ok(zero)) };
            let own = g.world.placed.get(r).and_then(|p| p.primitive).map(|p| ScriptPrimitive { shape: if p.sphere { 2 } else { 1 }, bounds: p.bounds });
            let Some(mut p) = g.other.jip14.primitives.get(&r).copied().or(own) else { return Some(Ok(zero)) };
            if name == "setprimitivebound" {
                let Some(v) = num(args, 1) else { return Some(Ok(zero)) };
                p.bounds[a] = v as f32;
                g.other.jip14.primitives.insert(r, p);
            }
            XVal::Num(f64::from(p.bounds[a]))
        }
        // ---- the auxiliary-variable condition ----
        // The float at index 0 of the calling reference's auxiliary variable named by the
        // quest's string variable at `index` [documented]; 0 otherwise [documented]. A
        // name with no owning mod's prefix [decision: a condition has no calling script
        // to take a private name's mod from].
        "auxvargetfltcond" => {
            let (Some(r), Some(q), Some(i)) = (subject, form(args, 0), num(args, 1)) else { return Some(Ok(zero)) };
            let v = g.var(crate::game::InstKey::Quest { quest: q }, i as u16);
            let Some(aux) = g.ext.text(v.num() as u32).map(str::to_string) else { return Some(Ok(zero)) };
            let store = if aux.starts_with('*') { &g.ext.jip_temp } else { &g.ext.jip };
            match store.aux.get(&r.0).and_then(|m| m.get(&aux.to_ascii_lowercase())).and_then(|v| v.first()) {
                Some(XVal::Num(x)) => XVal::Num(*x),
                _ => zero,
            }
        }
        // ---- actors ----
        // Severed, or exploded (1), a limb 0-13 [documented]; the engine's dismemberment
        // (`nvf_play::dismember`).
        "dismemberlimb" => {
            let (Some(r), Some(part)) = (subject, num(args, 0)) else { return Some(Ok(zero)) };
            if (0.0..14.0).contains(&part) {
                g.combat_requests.push(crate::combat::CombatRequest::Dismember { actor: r, location: part as i32, by: None, explode: Some(num(args, 1).unwrap_or(0.0) != 0.0) });
            }
            zero
        }
        // FireWeapon's shot, from the node when its place is known [documented].
        "fireweaponex" => {
            let (Some(r), Some(w)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let muzzle = text(args, 1).filter(|n| !n.is_empty()).and_then(|n| node_place(g, r, &n, true)).map(|(p, _)| p);
            g.combat_requests.push(crate::combat::CombatRequest::Fire { from: r, weapon: w, muzzle });
            zero
        }
        // A parent idle: the first child whose conditions pass; a child: played whatever
        // its conditions; none: PickIdle [documented].
        "playidleex" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            match form(args, 0) {
                None => {
                    if !g.idle_picks.contains(&r) {
                        g.idle_picks.push(r);
                    }
                }
                Some(idle) => {
                    let pick = if g.data.idle_children.contains_key(&Some(idle)) { g.first_idle_leaf(r, idle) } else { Some(idle) };
                    if let Some(i) = pick {
                        g.ask_idle(r, i, crate::idles::IdleSource::Script);
                    }
                }
            }
            zero
        }
        // At the node's place in the reference's 3D [documented]; at the reference while
        // its 3D is not known yet [hypothesis].
        "playsound3dnode" => {
            let (Some(r), Some(s), Some(node)) = (subject, form(args, 0), text(args, 1)) else { return Some(Ok(zero)) };
            let point = node_place(g, r, &node, true).map(|(p, _)| p);
            g.ask_sound(crate::sound::SoundRequest { sound: s, at: Some(r), system: false, id: 0, point });
            zero
        }
        // Whether the actor wobbles its gun [documented: ToggleNoGunWobble, alias
        // SetNoGunWobble, "0/1"]: 1, or no argument as the page's example, no wobble; 0
        // the wobble again [hypothesis: 1 is "no wobble", by the name]. The catalogue's
        // parameter has no name and does not reach here yet (a follow-up): every call
        // switches the wobble off.
        "setnogunwobble" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            if num(args, 0).unwrap_or(1.0) != 0.0 {
                g.other.jip14.no_wobble.insert(r);
            } else {
                g.other.jip14.no_wobble.remove(&r);
            }
            zero
        }
        // ---- models and textures ----
        // The reference's own model (under meshes), the base's without one [documented];
        // drawn when its 3D is made again [documented: "followed by a call to Update3D";
        // NVForever's engine draws it at the next drawing of the reference].
        "setrefrmodelpath" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let m = text(args, 0).filter(|m| !m.is_empty()).map(|m| {
                let m = m.replace('/', "\\");
                match m.to_ascii_lowercase().strip_prefix("meshes\\") {
                    Some(_) => m[7..].to_string(),
                    None => m,
                }
            });
            g.other.jip14.ref_models.insert(r, m);
            zero
        }
        // SwapTexture with a slot [documented]: slot 0, the diffuse map, is SwapTexture's;
        // the others are not drawn [decision: NVForever's texture swaps change the diffuse
        // map only].
        "swaptextureex" => {
            let (Some(on), Some(block), Some(path), Some(slot)) = (form(args, 0), text(args, 1), text(args, 2), num(args, 3)) else { return Some(Ok(zero)) };
            if slot == 0.0 {
                let (shape, texture) = crate::functions::misc::texture_swap_names(&block, &path);
                g.texture_swaps.swap(on, shape, texture);
            }
            zero
        }
        // [decision: NVForever's scene mirror keeps no texture sets: no array, the page's
        // answer for a block without one]
        "getblocktextureset" => XVal::Array(0),
        // ---- VATS ----
        "getvatstargets" => {
            let t: Vec<XVal> = g.other.jip14.vats_targets.iter().map(|r| XVal::Ref(r.0)).collect();
            list(g, t)
        }
        "setvatstargetable" => {
            let (Some(r), Some(t)) = (subject, num(args, 0)) else { return Some(Ok(zero)) };
            if t != 0.0 {
                g.other.jip14.vats_off.remove(&r);
            } else {
                g.other.jip14.vats_off.insert(r);
            }
            zero
        }
        // ---- projectiles ----
        // Seconds to the detonation; -1 for a projectile without a fuse [documented]; a
        // mine not triggered yet FLOAT_MAX [documented].
        "getdetonationtimer" => {
            let Some(r) = subject.filter(|r| g.gaps.projs.shots.contains_key(r)) else { return Some(Ok(XVal::Num(-1.0))) };
            XVal::Num(g.other.jip14.fuses.get(&r).map_or(-1.0, |&s| f64::from(s)))
        }
        "setdetonationtimer" => {
            let (Some(r), Some(t)) = (subject, num(args, 0)) else { return Some(Ok(zero)) };
            if g.gaps.projs.shots.contains_key(&r) {
                g.other.jip14.fuse_sets.push((r, t.max(0.0) as f32));
            }
            zero
        }
        // The weapon that fired the live projectile [documented]; the shot's record, read
        // by the projectile getters and at the impact [hypothesis: what the impact
        // inherits]. Returns the weapon.
        "setprojectilerefweapon" => {
            let (Some(r), Some(w)) = (subject, form(args, 0)) else { return Some(Ok(XVal::Ref(0))) };
            match g.gaps.projs.shots.get_mut(&r) {
                Some(s) if g.data.sig(w) == Some(*b"WEAP") => {
                    s.weapon = w;
                    XVal::Ref(w.0)
                }
                _ => XVal::Ref(0),
            }
        }
        // ---- inventory references ----
        // From the script of an item in a container: the item as an inventory reference
        // [documented]; 0 elsewhere.
        "getselfasinventoryref" => match g.other.jip14.item_running {
            Some((holder, item)) => XVal::Ref(g.temp_ref(holder, item).0),
            None => XVal::Ref(0),
        },
        // ---- animation sequences ----
        // The frequency a script set, else 1 [hypothesis: a sequence's own is 1 unless its
        // file says otherwise]; the engine plays the placed objects' sequences at it.
        "getanimsequencefrequency" | "getanimseqfreq" => {
            let (Some(r), Some(s)) = (subject, text(args, 0)) else { return Some(Ok(zero)) };
            XVal::Num(f64::from(g.anim_frequency(r, &s).unwrap_or(1.0)))
        }
        // "*" every sequence [documented].
        "setanimsequencefrequency" | "setanimseqfreq" => {
            let (Some(r), Some(s), Some(f)) = (subject, text(args, 0), num(args, 1)) else { return Some(Ok(zero)) };
            g.other.jip14.anim_freq.insert((r, s.to_ascii_lowercase()), f.max(0.0) as f32);
            zero
        }
        // ---- nothing values by decision ----
        // [decision: render-to-texture cameras are not built; 0, the pages' failure]
        "attachextracamera" | "projectextracamera" => zero,
        // [decision: line meshes are not built; 0, the page's failure]
        "attachline" => zero,
        // [decision: the page is marked Incomplete]
        "haskeywordcond" => zero,
        // [decision: NVForever does not crash the game on purpose; the page: "don't use
        // this"]
        "freeplayer" | "goodbyeworldspace" => zero,
        // [decision: NVForever draws the player's 3D from the game each frame; nothing to
        // reload, as Update3D]
        "updateplayer3d" => zero,
        _ => return None,
    };
    Some(Ok(v))
}
