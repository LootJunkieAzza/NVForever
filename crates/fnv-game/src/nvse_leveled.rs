//! The leveled-list functions of JIP LN / LU and ShowOff [documented: their pages;
//! observed: the peer's lu-leveled set in the original game]. Item lists (LVLI) are
//! edited in the world, where the leveled-item resolver reads them, so containers and
//! loot see the change; actor lists (LVLN, LVLC) are edited here for the functions
//! [hypothesis: spawning from an edited actor list is not yet hooked]. LU's edits last
//! for the session ("not savebaked"); ShowOff's RemoveFormFromLeveledList, documented
//! as saved, is kept for the session too [decision: the saving is "EXPERIMENTAL" on its
//! page].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::forms::LeveledEntry;
use fnv_sim::FormId;

/// One leveled list as the functions see it.
#[derive(Debug, Clone, Default)]
pub struct LevList {
    pub chance_none: u8,
    pub flags: u8,
    pub global: Option<FormId>,
    /// (level, form, count, condition).
    pub entries: Vec<(u16, FormId, u16, Option<f32>)>,
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

fn item_list_flags(l: &fnv_sim::forms::LeveledList) -> u8 {
    u8::from(l.all_levels) | u8::from(l.each) << 1 | u8::from(l.use_all) << 2
}

impl Game<'_> {
    /// A leveled list (item or actor) as the session has it.
    pub fn lev_list(&self, f: FormId) -> Option<LevList> {
        if let Some(l) = self.world.leveled_list(f) {
            return Some(LevList {
                chance_none: l.chance_none,
                flags: self.other.lev_flags.get(&f).copied().unwrap_or_else(|| item_list_flags(&l)),
                global: l.global,
                entries: l.entries.iter().map(|e| (e.level, e.item, e.count, e.condition)).collect(),
            });
        }
        if let Some(l) = self.other.lev_actors.get(&f) {
            return Some(l.clone());
        }
        let d = self.data.leveled_actors.get(&f)?;
        Some(LevList {
            chance_none: d.chance_none,
            flags: d.flags,
            global: None,
            entries: d.entries.iter().enumerate().map(|(i, (lv, e))| (*lv, *e, d.counts.get(i).copied().unwrap_or(1), None)).collect(),
        })
    }

    /// Store an edited list where its readers look.
    fn set_lev_list(&mut self, f: FormId, l: LevList) {
        let flags = l.flags;
        let item = self.world.edit_leveled(f, |w| {
            w.chance_none = l.chance_none;
            w.all_levels = flags & 1 != 0;
            w.each = flags & 2 != 0;
            w.use_all = flags & 4 != 0;
            w.global = l.global;
            w.entries = l.entries.iter().map(|&(level, item, count, condition)| LeveledEntry { level, item, count, condition }).collect();
        });
        if item {
            self.other.lev_flags.insert(f, flags);
        } else {
            self.other.lev_actors.insert(f, l);
        }
    }

    /// Whether `target` is in the list or any list inside it.
    fn lev_has_deep(&self, list: FormId, target: FormId, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        self.lev_list(list).is_some_and(|l| l.entries.iter().any(|e| e.1 == target || self.lev_has_deep(e.1, target, depth + 1)))
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let is_lev = matches!(name,
        "getnumlevitems" | "getnthlevitem" | "getnthlevitemcount" | "getnthlevitemlevel" | "getlevitemindexbylevel"
        | "getlevitemindexbyform" | "leveledlisthasformdeep" | "getchancenone" | "setchancenone" | "setchancenoneglobal"
        | "getleveledlistflags" | "setleveledlistflags" | "leveledlistaddform" | "leveledlistremoveform"
        | "removeformfromleveledlist" | "leveledlistreplaceform" | "leveledlistclear" | "removenthlevitem"
        | "setnthlevitem" | "setnthlevitemcount" | "setnthlevitemlevel" | "dumplevlist");
    if !is_lev {
        return None;
    }
    let Some(f) = form(args, 0) else { return Some(Ok(zero)) };
    // On anything but a leveled list the getters give 0 [observed: the stimpak].
    let Some(mut l) = g.lev_list(f) else {
        return Some(Ok(match name {
            "getnthlevitem" => XVal::Ref(0),
            "getlevitemindexbylevel" | "getlevitemindexbyform" => XVal::Num(-1.0),
            _ => zero,
        }));
    };
    let idx = |k: usize| usize::try_from(num(args, k, -1.0) as i64).ok();
    let n = l.entries.len();
    let v = match name {
        "getnumlevitems" => return Some(Ok(XVal::Num(n as f64))),
        // Past the end: 0 [observed].
        "getnthlevitem" => return Some(Ok(XVal::Ref(idx(1).and_then(|i| l.entries.get(i)).map_or(0, |e| e.1 .0)))),
        "getnthlevitemcount" => return Some(Ok(XVal::Num(idx(1).and_then(|i| l.entries.get(i)).map_or(0.0, |e| f64::from(e.2))))),
        "getnthlevitemlevel" => return Some(Ok(XVal::Num(idx(1).and_then(|i| l.entries.get(i)).map_or(0.0, |e| f64::from(e.0))))),
        // The first entry of exactly that level [observed: level 1 gives -1 on a list
        // whose lowest entry is 33; "<=" would give 0].
        "getlevitemindexbylevel" => {
            let lv = num(args, 1, -1.0) as i64;
            return Some(Ok(XVal::Num(l.entries.iter().position(|e| i64::from(e.0) == lv).map_or(-1.0, |i| i as f64))));
        }
        "getlevitemindexbyform" => {
            let t = form(args, 1);
            return Some(Ok(XVal::Num(l.entries.iter().position(|e| Some(e.1) == t).map_or(-1.0, |i| i as f64))));
        }
        "leveledlisthasformdeep" => {
            let Some(t) = form(args, 1) else { return Some(Ok(zero)) };
            return Some(Ok(XVal::Num(f64::from(u8::from(g.lev_has_deep(f, t, 0))))));
        }
        "getchancenone" => return Some(Ok(XVal::Num(f64::from(l.chance_none)))),
        "getleveledlistflags" => return Some(Ok(XVal::Num(f64::from(l.flags)))),
        "dumplevlist" => {
            for (i, e) in l.entries.iter().enumerate() {
                g.console_out.push(format!("{i}: {} level {} count {}", g.data.name(e.1), e.0, e.2));
            }
            return Some(Ok(zero));
        }
        // Over 100 is refused [observed: 250 left 35].
        "setchancenone" => {
            let c = num(args, 1, -1.0);
            if !(0.0..=100.0).contains(&c) {
                return Some(Ok(zero));
            }
            l.chance_none = c as u8;
            zero
        }
        "setchancenoneglobal" => {
            l.global = form(args, 1);
            zero
        }
        // Kept as given, not masked to the three known bits [observed: 15 reads 15].
        "setleveledlistflags" => {
            l.flags = num(args, 1, 0.0) as i64 as u8;
            zero
        }
        // `list form level count health`: in level order, after the entries of the same
        // or a lower level [observed: an NPC at level 1 went in at index 0 of a list of
        // level-33 entries]; health 0–1, 0 or 1 meaning whole [documented: "pass in 0.0
        // as the health argument" for a list].
        "leveledlistaddform" => {
            let Some(item) = form(args, 1) else { return Some(Ok(zero)) };
            let level = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16;
            let count = num(args, 3, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16;
            let h = num(args, 4, 0.0);
            let condition = (h > 0.0 && h < 1.0).then_some(h as f32);
            let at = l.entries.iter().position(|e| e.0 > level).unwrap_or(n);
            l.entries.insert(at, (level, item, count, condition));
            zero
        }
        // Every entry of the form; gives how many [documented].
        "leveledlistremoveform" | "removeformfromleveledlist" => {
            let t = form(args, 1);
            l.entries.retain(|e| Some(e.1) != t);
            XVal::Num((n - l.entries.len()) as f64)
        }
        "leveledlistreplaceform" => {
            let (Some(old), Some(new)) = (form(args, 1), form(args, 2)) else { return Some(Ok(zero)) };
            for e in l.entries.iter_mut().filter(|e| e.1 == old) {
                e.1 = new;
            }
            zero
        }
        "leveledlistclear" => {
            l.entries.clear();
            zero
        }
        "removenthlevitem" => {
            match idx(1).filter(|&i| i < n) {
                Some(i) => {
                    l.entries.remove(i);
                }
                None => return Some(Ok(zero)),
            }
            zero
        }
        "setnthlevitem" | "setnthlevitemcount" | "setnthlevitemlevel" => {
            let Some(e) = idx(1).and_then(|i| l.entries.get_mut(i)) else { return Some(Ok(zero)) };
            match name {
                "setnthlevitem" => match form(args, 2) {
                    Some(new) => e.1 = new,
                    None => return Some(Ok(zero)),
                },
                "setnthlevitemcount" => e.2 = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16,
                _ => e.0 = num(args, 2, 1.0).clamp(0.0, f64::from(u16::MAX)) as u16,
            }
            zero
        }
        _ => return None,
    };
    g.set_lev_list(f, l);
    Some(Ok(v))
}
