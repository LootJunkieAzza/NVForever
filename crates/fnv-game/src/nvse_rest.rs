//! The last core xNVSE functions (origins NVSE and FOSE1): setters whose getters were
//! built, the menu-mode key holds, the level-up menu, and the debug output functions
//! [documented: each function's page in the wiki export]. Changes last for the session.

use crate::functions::console::{self as con, ConsoleCtx};
use crate::game::{Game, RunCtx};

use fnv_script::vm::XVal;
use fnv_sim::FormId;

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
        Some(XVal::Num(n)) => format!("{n}"),
        _ => String::new(),
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let zero = XVal::Num(0.0);
    let v = match name {
        // `eyes`: the NPC base's eyes (ENAM), for the reference's base and so every
        // reference of it; 1 when set [documented: SetEyes, "Returns 1 for success",
        // "affects their baseform"]. The look changes when the 3D is rebuilt (Update3D).
        "seteyes" => match (form(args, 0), subject.map(|s| base(g, s))) {
            (Some(e), Some(npc)) if g.data.sig(npc) == Some(*b"NPC_") => {
                g.field_overrides.insert((npc, "ENAM/".into()), XVal::Ref(e.0));
                XVal::Num(1.0)
            }
            _ => zero,
        },
        // `flags [npc]`: the ACBS flags' high 16 bits, the low ones kept [documented:
        // SetActorBaseFlagsHigh; GetActorBaseFlagsHigh reads the same bits].
        "setactorbaseflagshigh" => {
            if let Some(b) = form(args, 1).or(subject).map(|f| g.actor_flags_target(f)) {
                g.set_actor_base_flags(b, true, (num(args, 0, 0.0).max(0.0) as u64 & 0xFFFF) as u16);
            }
            zero
        }
        // `rank`: the faction rank a reference's owner needs (XRNK), read through
        // `Game::owner_rank` by GetOwningFactionRequiredRank, `Game::ownership_in`, and
        // IsOwned/HasOwnership [documented: SetOwningFactionRequiredRank].
        "setowningfactionrequiredrank" => {
            if let Some(r) = subject {
                g.other.owner_ranks.insert(r, num(args, 0, 0.0) as i32);
            }
            zero
        }
        // `package value`: the package target's count or distance, for every reader of
        // the package (the AI too) [documented: SetPackageTargetDistance, alias
        // SetPackageTargetCount].
        "setpackagetargetdistance" | "setpackagetargetcount" => {
            if let Some(p) = form(args, 0) {
                let v = num(args, 1, 0.0) as i32;
                g.change_package(p, |d| {
                    if let Some(t) = d.target.as_mut() {
                        t.count_distance = v;
                    }
                });
            }
            zero
        }
        // `ref.SetNthPackage package index`: the package at the index of the actor's base
        // list replaced, for the AI too; it gives the package it replaced; an index past
        // the end changes nothing [observed: jip-probes rules1 and rules2 (#1021): Doc's 9
        // packages, index 0 15ED62 became 10B756 and back, the count stayed 9; index 99
        // left 9; the result was 15ED62. The page gives no arguments; the three statement
        // forms tried did not compile in the game, `let x := ref.SetNthPackage p i` did].
        "setnthpackage" => {
            let (Some(actor), Some(p)) = (subject, form(args, 0)) else { return Some(Ok(XVal::Ref(0))) };
            // Reject non-PACK forms without mutating, and treat index -1 as the last
            // package entry [observed: 2026-11-25, `GSDocMitchellHouse` (`0x00103DF9`),
            // Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30,
            // `w6d4p7_08_set_nth_package_isolated` (`wave6d4_set_nth_package.json`):
            // `cnt=9|ok(m=1,old=1)|oob=00000000|neg(oldEqLast=1,newEq=1)|bad(ret=00000000,kept=1)`;
            // hypothesis: negative indices < -1 and empty-list -1 return 0; design: missing
            // or non-finite index arg returns 0 without mutating].
            if g.data.sig(g.unclone(p)) != Some(*b"PACK") {
                return Some(Ok(XVal::Ref(0)));
            }
            let Some(b) = crate::functions::base_of(g, actor) else { return Some(Ok(XVal::Ref(0))) };
            let mut list = g.package_list(actor);
            let idx_f = num(args, 1, -2.0);
            if !idx_f.is_finite() {
                return Some(Ok(XVal::Ref(0)));
            }
            let raw = idx_f.trunc() as i64;
            let idx = if raw == -1 {
                list.len().checked_sub(1)
            } else {
                usize::try_from(raw).ok().filter(|&i| i < list.len())
            };
            match idx {
                Some(i) => {
                    let old = std::mem::replace(&mut list[i], p);
                    g.other.package_lists.insert(b, list);
                    XVal::Ref(old.0)
                }
                None => XVal::Ref(0),
            }
        }
        // `count`: a placed stack's count [documented: SetRefCount]; an inventory
        // reference's stack is not changed here [hypothesis: the page warns of older
        // versions only].
        "setrefcount" => {
            let n = num(args, 0, 1.0).max(0.0) as i32;
            if let Some(r) = subject.filter(|&r| g.temp_item(r).is_none()) {
                if let Ok(s) = g.world.get(r) {
                    s.count = n;
                }
            }
            zero
        }
        // [documented: SetCurrentQuest, "identical to the vanilla function
        // ForceActiveQuest"].
        "setcurrentquest" => {
            if let Some(q) = form(args, 0).filter(|q| g.data.quests.contains_key(q)) {
                g.ui.active_quest = Some(q);
                g.ui.active_quest_count += 1;
            }
            zero
        }
        // Two typed strings; preserve the text rather than parsing console tokens
        // (docs/spec-notes/string-settings.md).
        "setstringsetting" => {
            let (Some(XVal::Str(name)), Some(XVal::Str(value))) = (args.first(), args.get(1)) else {
                return Some(Err("SetStringSetting requires a setting name and string value".into()));
            };
            match g.world.setting_value(name) {
                Some(fnv_sim::settings::Setting::Str(_)) => {
                    if let Err(e) = g.world.change_setting(name, fnv_sim::settings::Setting::Str(value.clone())) {
                        return Some(Err(e.to_string()));
                    }
                }
                Some(_) => return Some(Err(format!("{name} is not a string setting"))),
                None => return Some(Err(format!("No game setting {name}"))),
            }
            zero
        }
        // `"Name|Value"` (the format already applied): as SetStringSetting [documented:
        // SetStringGameSettingEx; the page calls it broken, its replacement working].
        "setstringgamesettingex" => {
            let t = text(args, 0);
            if let Some((n, v)) = t.split_once('|') {
                let cctx = ConsoleCtx { on: subject, active_quest: g.ui.active_quest };
                let _ = con::set_string_setting(g, &cctx, &[n.trim().to_string(), v.to_string()]);
            }
            zero
        }
        // `control button`: the control's mouse button, as SetControl with the mouse
        // [documented: SetAltControl; "saved to the ini" is not done: NVForever never
        // writes the game's INI files].
        "setaltcontrol" => {
            let a = [args.first().cloned().unwrap_or(XVal::Num(-1.0)), args.get(1).cloned().unwrap_or(XVal::Num(-1.0)), XVal::Num(1.0)];
            crate::script_input::call(&mut g.input, "setcontrol", &a);
            zero
        }
        // `scancode`: held in menu mode until MenuReleaseKey [documented: MenuHoldKey;
        // MenuReleaseKey], the keys a script holds [hypothesis: the menus read the same
        // held keys as HoldKey's].
        "menuholdkey" | "mhk" | "menureleasekey" | "mrk" => {
            let f = if name.starts_with("menuh") || name == "mhk" { "holdkey" } else { "releasekey" };
            crate::script_input::call(&mut g.input, f, args);
            zero
        }
        // [documented: ShowLevelUpMenu, "does NOT cause an increase in level"].
        // The menu for the player's level now, as levelup::open.
        "showlevelupmenu" => {
            crate::levelup::open(g);
            zero
        }
        // The file-writing debug functions write nothing [decision: NVForever writes
        // nothing beside the game; docs/design/install-layout.md]: PrintF, PrintDebugF
        // and Ar_DumpF (to a file in the Fallout folder), DecompileScript (into
        // DecompiledScripts), DumpDocs and DumpCommandWikiDoc (to nvse.log).
        "printf" | "printdebugf" | "printdf" | "ar_dumpf" | "dumpdocs" | "dumpcommandwikidoc" | "dumpwikidoc" => zero,
        // `path [forceRecompile]` [documented: CompileScript, alias GetUDFFromFile].
        "compilescript" | "getudffromfile" => XVal::Ref(g.compile_file(&text(args, 0), num(args, 1, 0.0) != 0.0).unwrap_or(0)),
        "decompilescript" => XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, Vec::new())),
        _ => return None,
    };
    let _ = ctx;
    Some(Ok(v))
}
