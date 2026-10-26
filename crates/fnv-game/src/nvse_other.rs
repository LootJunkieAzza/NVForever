//! The rest of the xNVSE "other" group and the player-control locks
//! (docs/handoff/xnvse-remaining.md §3) [documented: their pages in the wiki export].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What these functions keep for the session (none of it is saved: DisablePlayerControlsAlt
/// "changes are NOT savebaked" [documented]; the rest [hypothesis]).
#[derive(Debug, Default)]
pub struct OtherState {
    /// The plugin and the script whose code made the extender call now running
    /// (`Exec::ext_call`); "" and `None` for the console, a query or a runtime script.
    pub plugin: String,
    pub script: Option<FormId>,
    /// DisablePlayerControlsAlt's locks, by the plugin that set them [documented: "stored
    /// on a per-mod basis"], in the order of `Player::controls_disabled`.
    pub alt_locks: BTreeMap<String, [bool; 7]>,
    /// SetEditorID's names.
    pub edids: HashMap<FormId, String>,
    /// SetNthDefaultForm's changes, by place in the DOBJ list.
    pub defaults: HashMap<usize, FormId>,
    /// SetDoorTeleport's destinations: (door, position, z rotation in radians).
    pub doors: HashMap<FormId, (FormId, [f32; 3], f32)>,
    /// RemoveScript's and SetScript's changes: a base form's script, `None` when taken
    /// away [observed: nvse-fifth, Doc's reference has no script after RemoveScript].
    pub scripts: HashMap<FormId, Option<FormId>>,
    /// The hotkeys 1–8 (SetHotkeyItem).
    pub hotkeys: [Option<FormId>; 8],
    /// Keys SetIsControl made custom controls.
    pub custom_controls: HashSet<u16>,
    /// SetNthFactionRankNameEx's titles: (faction, rank, female).
    pub ranks: HashMap<(FormId, usize, bool), String>,
    /// SetIsPowerArmor's changes.
    pub power_armor: HashMap<FormId, bool>,
    /// SetConsoleEcho's state; off at the start [observed: nvse-fifth, GetConsoleEcho 0].
    pub echo: bool,
    /// SetPlayerCurrentAmmoRounds, for the engine's combat to take.
    pub set_magazine: Option<u32>,
}

/// The bit of DisablePlayerControlsAltEx's flags for each control group, in the order of
/// `Player::controls_disabled` (movement, Pip-Boy, fighting, POV, looking, rollover,
/// sneaking). Movement 0, Pip-Boy 2, fighting 3, POV 4 [documented: DisablePlayerControlsAltEx,
/// the default `(1 << 0) | (1 << 2) | (1 << 3) | (1 << 4)` and "the Fighting flag is the
/// fourth bit"]; looking 1, rollover 5, sneaking 6, bit 7 nothing [observed: nvse-fifth,
/// each bit read back through GetPlayerControlsDisabledAlt].
pub const ALT_BITS: [u32; 7] = [0, 2, 3, 4, 1, 5, 6];

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

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// The groups a DisablePlayerControlsAlt-family call names: the seven flags in order, a
/// left-out one taking its place in `dflt`, or the Ex form's bitmask; `dflt` when there is
/// no argument [observed: nvse-fifth, `DisablePlayerControlsAlt 1` also locked the
/// Pip-Boy; hypothesis: the left-out flags are the no-argument default's]. The getters
/// read only the flags they name (`short` all false) [hypothesis].
fn alt_groups(ex: bool, args: &[XVal], dflt: [bool; 7], short: [bool; 7]) -> [bool; 7] {
    if args.is_empty() {
        return dflt;
    }
    if ex {
        let bits = num(args, 0, 0.0) as u32;
        return ALT_BITS.map(|b| bits >> b & 1 != 0);
    }
    std::array::from_fn(|i| if i < args.len() { num(args, i, 0.0) != 0.0 } else { short[i] })
}

impl Game<'_> {
    /// A base form's script, RemoveScript's and SetScript's changes first.
    pub fn base_script(&self, base: FormId) -> Option<FormId> {
        match self.other.scripts.get(&base) {
            Some(s) => *s,
            None => self.data.script_of_base(base),
        }
    }
}

impl Game<'_> {
    /// The control groups locked now: DisablePlayerControls' and every plugin's
    /// DisablePlayerControlsAlt's (the engine reads this).
    pub fn controls_locked(&self) -> [bool; 7] {
        let mut out = self.player.controls_disabled;
        for locks in self.other.alt_locks.values() {
            for (o, l) in out.iter_mut().zip(locks) {
                *o |= l;
            }
        }
        out
    }

    /// The DOBJ list with SetNthDefaultForm's changes.
    fn default_forms(&self) -> Vec<Option<FormId>> {
        let mut v = self.data.default_objects.clone();
        for (&i, &f) in &self.other.defaults {
            if v.len() <= i {
                v.resize(i + 1, None);
            }
            v[i] = Some(f);
        }
        v
    }

    /// Where a load door leads: SetDoorTeleport's, else the record's (XTEL).
    fn door_teleport(&self, door: FormId) -> Option<(FormId, [f32; 3], f32)> {
        if let Some(&t) = self.other.doors.get(&door) {
            return Some(t);
        }
        let t = self.world.placed.get(door)?.teleport?;
        Some((t.door, t.pos, t.rot[2]))
    }
}

/// Whether a block's code calls the opcode anywhere, in statements or expressions (the
/// decoded program, searched as data).
fn calls_opcode(v: &serde_json::Value, op: u64) -> bool {
    match v {
        serde_json::Value::Object(m) => m.get("opcode").and_then(serde_json::Value::as_u64) == Some(op) || m.values().any(|x| calls_opcode(x, op)),
        serde_json::Value::Array(a) => a.iter().any(|x| calls_opcode(x, op)),
        _ => false,
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // [documented: AddItemOwnership; AddItemHealthPercentOwner] `item count [health
        // 0–1] owner rank`: the items come owned (the rank is not kept [hypothesis]), at
        // full health, not AddItem's 99.9% [observed: nvse-fifth, pistol 80 of 80, owner
        // Doc; the health a fraction, 0.5 gives 40].
        "additemownership" | "additemhealthpercentowner" => {
            let (Some(to), Some(item)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let n = num(args, 1, 1.0).max(1.0) as i32;
            let health = name == "additemhealthpercentowner";
            let owner = form(args, if health { 3 } else { 2 });
            let condition = health.then(|| num(args, 2, 1.0).clamp(0.0, 1.0) as f32).filter(|&c| c < 1.0);
            match g.world.inventory_mut(to) {
                Ok(inv) => {
                    inv.add_extra(item, n, fnv_sim::inventory::Extra { condition, owner, ..Default::default() });
                }
                Err(e) => return Some(Err(e.to_string())),
            }
            zero
        }
        // [documented: AddSpellNS] AddSpell without its message.
        "addspellns" => {
            let (Some(a), Some(s)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            if let Err(e) = crate::functions::console::call_words(g, Some(a), "AddSpell", &[format!("{:08X}", s.0)], "AddSpellNS") {
                return Some(Err(e));
            }
            zero
        }
        // [documented: EndVATScam] leaves VATS [hypothesis: NVForever has no VATS camera
        // yet; nothing to end].
        "endvatscam" => zero,
        // [documented: ForEachInList] the UDF runs once per form of the list, in order.
        "foreachinlist" => {
            let (Some(l), Some(f)) = (form(args, 0), args.get(1).cloned()) else { return Some(Ok(zero)) };
            for m in crate::functions::list_members(g, l) {
                if let Err(error) = g.run_function(&f, None, vec![XVal::Ref(m.0)], None, *ctx) {
                    g.emit(crate::trace::EventKind::ScriptError { unit: "ForEachInList".into(), error });
                }
            }
            zero
        }
        // [documented: GetBaseSpellListLevSpells] the base's leveled spells; "leveled
        // spells may be unused in vanilla" and NVForever reads none [hypothesis].
        "getbasespelllistlevspells" => XVal::Array(g.ext.new_array(ArrayKind::List, Vec::new())),
        // [documented: GetConsoleEcho; SetConsoleEcho, which gives the state before]
        // [hypothesis: echo is on at the start].
        "getconsoleecho" => yes(g.other.echo),
        "setconsoleecho" => {
            let before = g.other.echo;
            g.other.echo = num(args, 0, 1.0) != 0.0;
            yes(before)
        }
        // [documented: GetConsoleOutputFilename; HasConsoleOutputFileName] no SCOF file.
        "getconsoleoutputfilename" => XVal::Str(String::new()),
        "hasconsoleoutputfilename" => zero,
        // [documented: GetCrosshairRef, the reference the player would activate;
        // GetCrosshairRefEx, the same at any range] as the engine tells them; a null
        // reference otherwise [observed: GetCrosshairRefEx gives 00000000].
        "getcrosshairref" => XVal::Ref(g.senses.crosshair_ref.map_or(0, |r| r.0)),
        "getcrosshairrefex" => XVal::Ref(g.senses.crosshair_far.map_or(0, |r| r.0)),
        // [documented: GetCurrentObjective] the active quest's first objective shown and
        // not done [hypothesis: which objective counts as current], else -1 [hypothesis].
        "getcurrentobjective" => {
            let q = g.ui.active_quest.unwrap_or(FormId(0));
            let st = g.quest_state(q);
            XVal::Num(st.objectives.iter().find(|(_, o)| o.displayed && !o.completed).map_or(-1.0, |(&i, _)| f64::from(i)))
        }
        // [documented: GetDefaultForms; GetNthDefaultForm; SetNthDefaultForm] the DOBJ
        // list [hypothesis: the game's own systems keep reading the record's].
        "getdefaultforms" => {
            let entries = g.default_forms().into_iter().enumerate().map(|(i, f)| (XKey::Num(i as f64), XVal::Ref(f.map_or(0, |f| f.0)))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        "getnthdefaultform" => {
            let i = num(args, 0, -1.0);
            XVal::Ref(usize::try_from(i as i64).ok().and_then(|i| g.default_forms().get(i).copied().flatten()).map_or(0, |f| f.0))
        }
        "setnthdefaultform" => {
            if let (Ok(i), Some(f)) = (usize::try_from(num(args, 0, -1.0) as i64), form(args, 1)) {
                g.other.defaults.insert(i, f);
            }
            zero
        }
        // [documented: GetDoorTeleportX/Y/Z/Rot] where the load door leads, rotation in
        // degrees as GetAngle gives them [hypothesis]; 0 for any other door.
        "getdoorteleportx" | "getdoorteleporty" | "getdoorteleportz" | "getdoorteleportrot" => match subject.and_then(|d| g.door_teleport(d)) {
            Some((_, p, r)) => XVal::Num(f64::from(match name {
                "getdoorteleportx" => p[0],
                "getdoorteleporty" => p[1],
                "getdoorteleportz" => p[2],
                _ => r.to_degrees(),
            })),
            None => zero,
        },
        // [documented: SetDoorTeleport] `door [x y z rot]`: the left-out values are the
        // target's [hypothesis: the player still lands where the record says; the
        // getters see the change].
        "setdoorteleport" => {
            let (Some(d), Some(t)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let (p, r) = g.ref_pos_rot(t).unwrap_or(([0.0; 3], [0.0; 3]));
            let pos = [num(args, 1, f64::from(p[0])) as f32, num(args, 2, f64::from(p[1])) as f32, num(args, 3, f64::from(p[2])) as f32];
            let rot = args.get(4).map_or(r[2], |_| (num(args, 4, 0.0) as f32).to_radians());
            g.other.doors.insert(d, (t, pos, rot));
            zero
        }
        // [documented: GetNthExplicitRef; GetNumExplicitRefs] the script's reference list
        // (SCRO/SCRV), of the calling script when none is given.
        "getnthexplicitref" | "getnumexplicitrefs" => {
            let at = usize::from(name == "getnthexplicitref");
            let s = form(args, at).map(|f| if g.data.sig(f) == Some(*b"SCPT") { f } else { g.data.script_of_base(base(g, f)).unwrap_or(f) }).or(g.other.script);
            let refs: Vec<u32> = s
                .and_then(|s| g.data.script(s))
                .map(|c| c.unit.refs.iter().map(|r| if let fnv_script::unit::RefSlot::Form(f) = r { *f } else { 0 }).collect())
                .unwrap_or_default();
            if at == 0 {
                XVal::Num(refs.len() as f64)
            } else {
                XVal::Ref(usize::try_from(num(args, 0, -1.0) as i64).ok().and_then(|i| refs.get(i).copied()).unwrap_or(0))
            }
        }
        // [documented: GetParentCellOwningFactionRequiredRank] the rank the cell's owner
        // faction asks for; 0 without one [hypothesis].
        "getparentcellowningfactionrequiredrank" => {
            let rank = subject.and_then(|r| g.cell_of(r)).and_then(|c| g.cell_ownership(c)).and_then(|o| o.rank);
            XVal::Num(f64::from(rank.unwrap_or(0)))
        }
        // [documented: GetPermanent] on a reference, persistent; given one, a quest item
        // [observed: nvse-fifth, `GetPermanent rDoc` 1 though Doc is no quest item: the
        // reference's persistence counts too]. [documented: SetPermanent] sets the base
        // form's flag [hypothesis: the quest-item flag GetPermanent reads].
        "getpermanent" => match form(args, 0) {
            Some(r) => {
                let b = base(g, r);
                yes(g.world.is_quest_item(b) || g.persistent(r))
            }
            None => return crate::nvse_refs::call(g, "ispersistent", subject, &[]),
        },
        "setpermanent" => {
            let r = form(args, 1).or(subject).unwrap_or(FormId(0));
            let b = base(g, r);
            g.world.set_quest_object(b, num(args, 0, 0.0) != 0.0);
            zero
        }
        // [documented: GetTFC] the fly camera is on.
        "gettfc" => yes(g.ui.camera.fly_on),
        // [documented: HasScriptCommand] `opcode script [event]`: the script's code calls
        // the command (in that Begin block's event only, when given).
        "hasscriptcommand" => {
            let op = num(args, 0, -1.0);
            let Some(f) = form(args, 1) else { return Some(Ok(zero)) };
            let s = if g.data.sig(f) == Some(*b"SCPT") { Some(f) } else { g.data.script_of_base(base(g, f)) };
            let event = args.get(2).map(|_| num(args, 2, -1.0));
            let found = op >= 0.0
                && s.and_then(|s| g.data.script(s)).is_some_and(|c| {
                    c.program.blocks.iter().filter(|b| event.is_none_or(|e| f64::from(b.event) == e)).any(|b| serde_json::to_value(b).is_ok_and(|v| calls_opcode(&v, op as u64)))
                });
            yes(found)
        }
        // [documented: RemoveScript] the base form's script goes; gives it.
        // [documented: SetScript] `script [form]`: the base form runs that script; gives
        // the one before.
        "removescript" | "setscript" => {
            let set = name == "setscript";
            let new = if set { form(args, 0) } else { None };
            let b = form(args, usize::from(set)).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            let before = g.base_script(b);
            if before.is_some() || set {
                g.other.scripts.insert(b, new);
            }
            XVal::Ref(before.map_or(0, |s| s.0))
        }
        // [documented: RunScript] the script (or the scripted form's) runs once, with no
        // reference; true when it gave a true value [hypothesis: a script without a
        // Function block does not run, and gives 0].
        "runscript" => {
            let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
            let s = if g.data.sig(f) == Some(*b"SCPT") { Some(f) } else { g.data.script_of_base(base(g, f)) };
            let r = s.map(|s| g.run_function(&XVal::Ref(s.0), None, Vec::new(), None, RunCtx::default()));
            yes(matches!(r, Some(Ok(v)) if v.truthy()))
        }
        // [documented: SetEditorID] for new forms; GetEditorID and EditorIDToFormID see it.
        "seteditorid" => {
            if let (Some(f), Some(XVal::Str(id))) = (form(args, 0), args.get(1)) {
                g.other.edids.insert(f, id.clone());
            }
            zero
        }
        // [documented: SetIsPowerArmor] `isPA [form]`: IsPowerArmor sees it [hypothesis:
        // equipping does not].
        "setispowerarmor" => {
            let f = form(args, 1).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            g.other.power_armor.insert(f, num(args, 0, 0.0) != 0.0);
            zero
        }
        // Formatted by the VM (`Vm::host_function`), then here.
        "setnthfactionranknameex" => {
            let (Some(XVal::Str(t)), Some(f)) = (args.first(), form(args, 1)) else { return Some(Ok(zero)) };
            if let Ok(i) = usize::try_from(num(args, 2, -1.0) as i64) {
                g.other.ranks.insert((f, i, num(args, 3, 0.0) != 0.0), t.clone());
            }
            zero
        }
        // [documented: Sv_Set; observed: nvse-fifth, `Sv_Set "n=%g" 3 sText` leaves the
        // string empty] it does nothing [hypothesis: the function is left over].
        "sv_set" => zero,
        // [documented: OutputLocalMapPicturesOverride; SetOutputLocalMapPicturesGrids]
        // NVForever writes no local map pictures.
        "outputlocalmappicturesoverride" | "setoutputlocalmappicturesgrids" => zero,
        // [documented: SetPlayerCurrentAmmoRounds] the rounds in the player's magazine.
        "setplayercurrentammorounds" => {
            let n = num(args, 0, 0.0).max(0.0) as u32;
            g.other.set_magazine = Some(n);
            g.player_magazine = Some(n);
            zero
        }
        // The player-control locks [documented: DisablePlayerControlsAlt(Ex);
        // EnablePlayerControlsAlt(Ex); GetPlayerControlsDisabledAlt(Ex)]: each plugin keeps
        // its own; a group is locked while any plugin (or DisablePlayerControls) locks it.
        "disableplayercontrolsalt" | "disableplayercontrolsaltex" | "enableplayercontrolsalt" | "enableplayercontrolsaltex" => {
            let ex = name.ends_with("ex");
            let disable = name.starts_with("disable");
            let dflt = if disable { [true, true, true, true, false, false, false] } else { [true; 7] };
            let which = alt_groups(ex, args, dflt, dflt);
            let key = g.other.plugin.clone();
            let locks = g.other.alt_locks.entry(key.clone()).or_default();
            for (l, w) in locks.iter_mut().zip(which) {
                if w {
                    *l = disable;
                }
            }
            if !disable && locks.iter().all(|l| !l) {
                g.other.alt_locks.remove(&key);
            }
            zero
        }
        "getplayercontrolsdisabledalt" | "getplayercontrolsdisabledaltex" => {
            let how = num(args, 0, 0.0) as i64;
            let which = alt_groups(name.ends_with("ex"), args.get(1..).unwrap_or(&[]), [true; 7], [false; 7]);
            let mut locked = [false; 7];
            for (p, l) in &g.other.alt_locks {
                if how >= 1 || *p == g.other.plugin {
                    for (o, x) in locked.iter_mut().zip(l) {
                        *o |= x;
                    }
                }
            }
            if how >= 2 {
                for (o, x) in locked.iter_mut().zip(g.player.controls_disabled) {
                    *o |= x;
                }
            }
            yes(locked.iter().zip(which).any(|(l, w)| *l && w))
        }
        // [documented: GetHotkeyItem; SetHotkeyItem, an item the player carries;
        // ClearHotkey] hotkeys 1–8 [hypothesis: NVForever has no hotkey wheel yet; these
        // keep the table].
        "gethotkeyitem" | "sethotkeyitem" | "clearhotkey" => {
            let Some(k) = usize::try_from(num(args, 0, 0.0) as i64 - 1).ok().filter(|&k| k < 8) else { return Some(Ok(zero)) };
            match name {
                "gethotkeyitem" => return Some(Ok(XVal::Ref(g.other.hotkeys[k].map_or(0, |f| f.0)))),
                "clearhotkey" => g.other.hotkeys[k] = None,
                _ => {
                    let item = form(args, 1).map(|f| base(g, f));
                    let carried = item.is_some_and(|i| g.world.inventory_mut(FormId::PLAYER_REF).is_ok_and(|inv| inv.count(i) > 0));
                    if carried {
                        g.other.hotkeys[k] = item;
                    }
                }
            }
            zero
        }
        // [documented: SetIsControl] a key made (or no longer) a custom control; IsControl
        // gives 2 for it.
        "setiscontrol" => {
            if let Ok(code) = u16::try_from(num(args, 0, -1.0) as i64) {
                if num(args, 1, 0.0) != 0.0 {
                    g.other.custom_controls.insert(code);
                } else {
                    g.other.custom_controls.remove(&code);
                }
            }
            zero
        }
        // [documented: GetOpenKey; SetOpenKey] the key of a lockable reference.
        "getopenkey" => {
            let r = subject.unwrap_or(FormId(0));
            let key = match g.world.peek(r) {
                Some(s) => s.lock.and_then(|l| l.key),
                None => g.world.placed.get(r).and_then(|p| p.lock).and_then(|l| l.key),
            };
            XVal::Ref(key.map_or(0, |k| k.0))
        }
        "setopenkey" => {
            let (Some(r), Some(k)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            if let Ok(s) = g.world.get(r) {
                // A reference without a lock gets an open one with the key [hypothesis].
                let lock = s.lock.get_or_insert(fnv_sim::world::LockState { level: 0, key: None, locked: false });
                lock.key = Some(k);
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}
