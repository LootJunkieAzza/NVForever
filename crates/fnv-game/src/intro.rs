//! The intro chain's scripts, for the coverage report (docs/design/layer5.md §3.7: the
//! functions the intro chain calls go first).
//!
//! The intro chain is the quests VCG00–VCG04 [observed: layer5.md §2]. Its units are
//! those quests' scripts and stage result scripts, the result scripts of every dialogue
//! line (INFO) that belongs to them, and the scripts of the references and quests those
//! units name in their reference lists (Doc Mitchell, the Vigor Tester, the triggers,
//! Sunny Smiles…), one level deep. Its conditions are those of the quests' stages and
//! objectives and of those dialogue lines.

use crate::cond::Condition;
use crate::data::{GameData, UnitId};
use fnv_script::ir::{Expr, Program, Stmt, Token};
use fnv_script::unit::RefSlot;
use fnv_sim::{FormId, RefTable};
use std::collections::{BTreeMap, BTreeSet};

pub const INTRO_QUESTS: &[&str] = &["VCG00", "VCG01", "VCG02", "VCG03", "VCG04"];

pub struct Intro<'d> {
    pub quests: Vec<FormId>,
    pub units: BTreeSet<UnitId>,
    pub conditions: Vec<&'d Condition>,
}

pub fn intro<'d>(data: &'d GameData, refs: &RefTable) -> Intro<'d> {
    let quests: Vec<FormId> = INTRO_QUESTS.iter().filter_map(|e| data.by_edid(e)).filter(|q| data.is_quest(*q)).collect();
    quest_scope(data, refs, quests)
}

/// The units and conditions of any set of quests, by the intro chain's rule above (the
/// quests' own scripts, stages, objectives and dialogue lines, and one level of the
/// references and quests their units name). `fnvtool script uses` takes each quest alone.
pub fn quest_scope<'d>(data: &'d GameData, refs: &RefTable, quests: Vec<FormId>) -> Intro<'d> {
    let mut units = BTreeSet::new();
    let mut conditions = Vec::new();
    for q in &quests {
        let def = &data.quests[q];
        if let Some(&u) = def.script.and_then(|s| data.scripts.get(&s)) {
            units.insert(u);
        }
        conditions.extend(def.conditions.iter());
        for e in def.stages.values().flatten() {
            units.extend(e.unit);
            conditions.extend(e.conditions.iter());
        }
        for o in def.objectives.values() {
            for t in &o.targets {
                conditions.extend(t.conditions.iter());
            }
        }
    }
    let mut infos: Vec<(&FormId, &crate::data::InfoDef)> = data.infos.iter().filter(|(_, i)| i.quest.is_some_and(|q| quests.contains(&q))).collect();
    infos.sort_by_key(|(f, _)| **f);
    for (_, i) in infos {
        units.extend(i.begin);
        units.extend(i.end);
        conditions.extend(i.conditions.iter());
    }
    // One level of named references and quests.
    let first: Vec<UnitId> = units.iter().copied().collect();
    for u in first {
        for slot in &data.unit(u).unit.refs {
            let RefSlot::Form(f) = slot else { continue };
            let f = FormId(*f);
            let script = match refs.get(f) {
                Some(p) => data.script_of_base(p.base),
                None => data.quests.get(&f).and_then(|q| q.script).or_else(|| data.script_of_base(f)),
            };
            if let Some(&su) = script.and_then(|s| data.scripts.get(&s)) {
                units.insert(su);
            }
        }
    }
    Intro { quests, units, conditions }
}

fn expr_calls(e: &Expr, out: &mut BTreeMap<u16, u64>) {
    for t in &e.0 {
        if let Token::Call { call } = t {
            *out.entry(call.opcode).or_default() += 1;
        }
    }
}

/// Calls per opcode in a program (statements and expressions).
pub fn calls(p: &Program, out: &mut BTreeMap<u16, u64>) {
    let mut st: Vec<&Stmt> = Vec::new();
    p.walk(&mut |s| st.push(s));
    for s in st {
        match s {
            Stmt::Set { expr, .. } => expr_calls(expr, out),
            Stmt::If { arms, .. } => arms.iter().for_each(|a| expr_calls(&a.cond, out)),
            Stmt::Call { call } => *out.entry(call.opcode).or_default() += 1,
            Stmt::Unmatched { cond: Some(c), .. } => expr_calls(c, out),
            _ => {}
        }
    }
}
