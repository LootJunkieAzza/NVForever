//! IDLE records (idle animations) read through the Layer 1 schema, as data: the `.kf`
//! they name, their parent/previous links, their animation-group byte and their
//! conditions. Choosing what an actor plays (evaluating the conditions, walking the tree)
//! is Layer 5/6; this module only reads and checks the records.
//!
//! Layouts and field meanings: docs/spec-notes/animation.md §1 (from xEdit's public
//! definitions, `schemas/fnv/gameplay.json` IDLE and `common.json` CTDA).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fnv_esm::{LoadOrder, Schema};
use fnv_sim::extract::{self, all, first, global, int, local_fid, text, Decoded, Issues};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// One CTDA condition, kept as stored. Parameters and the reference are file-local
/// values of the IDLE's defining plugin ([`Idle::plugin`]); their meaning depends on the
/// function and is decided when conditions are evaluated (Layer 5/6).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Condition {
    /// Type byte: comparison operator in the top three bits, flags in the low five
    /// [documented: xEdit wbConditionTypeToStr].
    pub type_raw: u8,
    /// Comparison value as stored: an f32, or a GLOB FormID when [`Self::use_global`].
    pub comparison_raw: u32,
    /// Function index; the script opcode is 0x1000 + index (docs/spec-notes/
    /// script-functions.md §4).
    pub function: u16,
    pub param1_raw: u32,
    pub param2_raw: u32,
    /// Run On (absent in the 20-byte form) [documented: xEdit SetOptionalFrom(7)].
    pub run_on: Option<u32>,
    pub reference_raw: Option<u32>,
}

/// Comparison operators by the top three bits of the type byte [documented: xEdit].
const OPERATORS: [(u8, &str); 6] = [(0, "=="), (1, "!="), (2, ">"), (3, ">="), (4, "<"), (5, "<=")];

impl Condition {
    fn from_value(v: &Value) -> Option<Condition> {
        let u = |k: &str| int(v, k).map(|x| x as u32);
        Some(Condition {
            type_raw: int(v, "type")? as u8,
            comparison_raw: u("comparison_value_raw")?,
            function: int(v, "function")? as u16,
            param1_raw: u("parameter1_raw")?,
            param2_raw: u("parameter2_raw")?,
            run_on: u("run_on"),
            reference_raw: u("reference_raw"),
        })
    }

    /// `==`, `!=`, `>`, `>=`, `<`, `<=`; None for the two undefined operator codes.
    pub fn operator(&self) -> Option<&'static str> {
        let code = self.type_raw >> 5;
        OPERATORS.iter().find(|(c, _)| *c == code).map(|(_, s)| *s)
    }

    /// Bit 0 "Or": combine with the next condition by OR instead of AND [documented:
    /// xEdit flag name; the grouping rule is Layer 5].
    pub fn or(&self) -> bool {
        self.type_raw & 0x01 != 0
    }

    /// Bit 2 "Use Global": the comparison value is a GLOB FormID [documented: xEdit
    /// wbConditionCompValueDecider].
    pub fn use_global(&self) -> bool {
        self.type_raw & 0x04 != 0
    }

    /// The comparison value as a float, when it is not a global.
    pub fn comparison_f32(&self) -> Option<f32> {
        (!self.use_global()).then(|| f32::from_bits(self.comparison_raw))
    }
}

/// What the IDLE's `MODL` names [observed: some records name a folder, no file].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    None,
    /// Ends in `.kf` (case-insensitive).
    Kf,
    /// Anything else (observed: folders such as `Creatures\Dog\IdleAnims`).
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Idle {
    pub id: FormId,
    pub edid: Option<String>,
    /// Load-order index of the plugin whose definition won; condition parameters are
    /// local to it.
    pub plugin: usize,
    /// `MODL` as stored (trimmed), relative to `meshes\`.
    pub model: Option<String>,
    pub conditions: Vec<Condition>,
    /// ANAM parent and previous IDLE (null → None).
    pub parent: Option<FormId>,
    pub previous: Option<FormId>,
    /// DATA "Animation Group Section" byte as stored; see [`Idle::section`].
    pub group_raw: Option<u8>,
    pub loop_min: Option<u8>,
    pub loop_max: Option<u8>,
    pub replay_delay: Option<i16>,
    /// DATA flags (absent in the 6-byte form); bit 0 "No attacking" [documented].
    pub flags: Option<u8>,
}

/// Animation group sections by the low six bits of the DATA byte [documented: xEdit
/// wbIdleAnam].
pub const SECTIONS: [(u8, &str); 10] = [
    (0, "Idle"),
    (1, "Movement"),
    (2, "Left Arm"),
    (3, "Left Hand"),
    (4, "Weapon"),
    (5, "Weapon Up"),
    (6, "Weapon Down"),
    (7, "Special Idle"),
    (20, "Whole Body"),
    (21, "Upper Body"),
];

impl Idle {
    /// Low six bits of the group byte.
    pub fn section(&self) -> Option<u8> {
        self.group_raw.map(|g| g & 0x3F)
    }

    pub fn section_name(&self) -> Option<&'static str> {
        let s = self.section()?;
        SECTIONS.iter().find(|(c, _)| *c == s).map(|(_, n)| *n)
    }

    /// Bits 0x40 and 0x80 of the group byte, kept raw. xEdit labels 0x40 "Loose Idle"
    /// and prints "Must return a file" when 0x80 is clear [documented]; what the engine
    /// does with them is open (docs/spec-notes/animation.md §5).
    pub fn group_high_bits(&self) -> Option<u8> {
        self.group_raw.map(|g| g & 0xC0)
    }

    pub fn model_kind(&self) -> ModelKind {
        match &self.model {
            None => ModelKind::None,
            Some(m) if m.to_ascii_lowercase().ends_with(".kf") => ModelKind::Kf,
            Some(_) => ModelKind::Other,
        }
    }

    /// The model as a virtual-file-system path (`meshes\…`, lower case, backslashes).
    pub fn model_path(&self) -> Option<String> {
        self.model.as_deref().map(meshes_path)
    }
}

/// `MODL`-style path (relative to `meshes\`) as a normalised VFS path.
pub fn meshes_path(model: &str) -> String {
    let n = crate::anims::normalize(model);
    if n.starts_with("meshes\\") {
        n
    } else {
        format!("meshes\\{n}")
    }
}

/// Every winning, non-deleted IDLE of a load order.
#[derive(Debug, Default)]
pub struct Idles {
    pub idles: BTreeMap<FormId, Idle>,
    pub issues: Issues,
}

/// Structure of the parent/previous links (docs/spec-notes/animation.md §2).
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct TreeStats {
    pub idles: usize,
    /// No parent.
    pub roots: usize,
    /// Parent or previous names a FormID that is not a loaded IDLE.
    pub dangling_parent: usize,
    pub dangling_previous: usize,
    /// Previous names an IDLE with a different parent.
    pub previous_other_parent: usize,
    /// Siblings whose previous links form exactly one chain from a first sibling
    /// (previous None or outside the group) through all of them.
    pub sibling_groups: usize,
    pub sibling_groups_one_chain: usize,
    /// IDLEs on a parent cycle.
    pub in_parent_cycle: usize,
    /// IDLEs per depth (roots at 0); cycle members excluded.
    pub by_depth: BTreeMap<usize, usize>,
}

impl Idles {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> Idles {
        let mut wanted = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if &r.header.sig != b"IDLE" || r.header.flags & 0x20 != 0 {
                continue;
            }
            wanted.push((FormId(g), d.plugin as usize, r));
        }
        let loaded: Vec<(Option<Idle>, Issues)> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut issues = Issues::default();
                let idle = match extract::decode(r, schema) {
                    Ok(d) => Some(read_idle(lo, pi, id, &d, &mut issues)),
                    Err(e) => {
                        issues.note("IDLE does not decode", || format!("{id}: {e}"));
                        None
                    }
                };
                (idle, issues)
            })
            .collect();
        let mut out = Idles::default();
        for (idle, issues) in loaded {
            out.issues.merge(issues);
            if let Some(i) = idle {
                out.idles.insert(i.id, i);
            }
        }
        out
    }

    /// Children of `parent` (None: the roots), in sibling order: following previous
    /// links from the sibling whose previous is not a sibling, then any left over by
    /// FormID [hypothesis: previous orders siblings; docs/spec-notes/animation.md §2].
    pub fn children(&self, parent: Option<FormId>) -> Vec<FormId> {
        let sibs: Vec<FormId> = self.idles.values().filter(|i| i.parent == parent).map(|i| i.id).collect();
        order_siblings(&sibs, |id| self.idles.get(&id).and_then(|i| i.previous))
    }

    pub fn tree_stats(&self) -> TreeStats {
        let mut st = TreeStats { idles: self.idles.len(), ..TreeStats::default() };
        let mut groups: HashMap<Option<FormId>, Vec<FormId>> = HashMap::new();
        for i in self.idles.values() {
            groups.entry(i.parent).or_default().push(i.id);
            if i.parent.is_none() {
                st.roots += 1;
            }
            if i.parent.is_some_and(|p| !self.idles.contains_key(&p)) {
                st.dangling_parent += 1;
            }
            match i.previous {
                Some(p) if !self.idles.contains_key(&p) => st.dangling_previous += 1,
                Some(p) if self.idles[&p].parent != i.parent => st.previous_other_parent += 1,
                _ => {}
            }
        }
        for sibs in groups.values() {
            st.sibling_groups += 1;
            if one_chain(sibs, |id| self.idles.get(&id).and_then(|i| i.previous)) {
                st.sibling_groups_one_chain += 1;
            }
        }
        for i in self.idles.values() {
            match self.depth(i.id) {
                Some(d) => *st.by_depth.entry(d).or_default() += 1,
                None => st.in_parent_cycle += 1,
            }
        }
        st
    }

    /// Number of parent links up to a root (a dangling parent counts as a root's
    /// child); None on a cycle.
    pub fn depth(&self, id: FormId) -> Option<usize> {
        let mut seen = BTreeSet::new();
        let mut cur = id;
        let mut d = 0;
        while let Some(p) = self.idles.get(&cur).and_then(|i| i.parent) {
            if !seen.insert(cur) {
                return None;
            }
            d += 1;
            cur = p;
        }
        Some(d)
    }
}

fn read_idle(lo: &LoadOrder, pi: usize, id: FormId, d: &[Decoded], issues: &mut Issues) -> Idle {
    let link = |v: Option<&Value>, what: &str, issues: &mut Issues| -> Option<FormId> {
        match global(lo, pi, local_fid(v?)?) {
            Ok(g) => g,
            Err(l) => {
                issues.note("FormID outside master list", || format!("{id} ANAM {what} 0x{l:08X}"));
                None
            }
        }
    };
    let anam = first(d, b"ANAM");
    if anam.is_none() {
        issues.note("IDLE without ANAM", || format!("{id}"));
    }
    let data = first(d, b"DATA");
    if data.is_none() {
        issues.note("IDLE without DATA", || format!("{id}"));
    }
    let byte = |k: &str| data.and_then(|v| int(v, k)).map(|x| x as u8);
    let mut conditions = Vec::new();
    for v in all(d, b"CTDA") {
        match Condition::from_value(v) {
            Some(c) => conditions.push(c),
            None => issues.note("CTDA does not decode", || format!("{id}")),
        }
    }
    Idle {
        id,
        edid: first(d, b"EDID").and_then(text),
        plugin: pi,
        model: first(d, b"MODL").and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        conditions,
        parent: link(anam.and_then(|a| a.get("parent")), "parent", issues),
        previous: link(anam.and_then(|a| a.get("previous")), "previous", issues),
        group_raw: byte("animation_group_section"),
        loop_min: byte("loop_min"),
        loop_max: byte("loop_max"),
        replay_delay: data.and_then(|v| int(v, "replay_delay")).map(|x| x as i16),
        flags: data.and_then(|v| v.get("flags")).and_then(|f| f.get("value")).and_then(Value::as_u64).map(|x| x as u8),
    }
}

/// Sibling order by previous links (see [`Idles::children`]).
fn order_siblings(sibs: &[FormId], previous: impl Fn(FormId) -> Option<FormId>) -> Vec<FormId> {
    let set: BTreeSet<FormId> = sibs.iter().copied().collect();
    // next[p] = siblings whose previous is p
    let mut next: BTreeMap<FormId, Vec<FormId>> = BTreeMap::new();
    let mut starts = Vec::new();
    for &s in sibs {
        match previous(s).filter(|p| set.contains(p)) {
            Some(p) => next.entry(p).or_default().push(s),
            None => starts.push(s),
        }
    }
    starts.sort();
    let mut out = Vec::new();
    let mut placed = BTreeSet::new();
    let mut stack: Vec<FormId> = starts.into_iter().rev().collect();
    while let Some(s) = stack.pop() {
        if !placed.insert(s) {
            continue;
        }
        out.push(s);
        if let Some(n) = next.get(&s) {
            let mut n = n.clone();
            n.sort();
            stack.extend(n.into_iter().rev());
        }
    }
    let mut rest: Vec<FormId> = set.difference(&placed).copied().collect();
    rest.sort();
    out.extend(rest);
    out
}

/// True when the siblings' previous links form one chain covering all of them.
fn one_chain(sibs: &[FormId], previous: impl Fn(FormId) -> Option<FormId>) -> bool {
    let set: BTreeSet<FormId> = sibs.iter().copied().collect();
    let starts = sibs.iter().filter(|&&s| previous(s).filter(|p| set.contains(p)).is_none()).count();
    starts == 1 && order_siblings(sibs, &previous).windows(2).all(|w| previous(w[1]) == Some(w[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cond(type_raw: u8, comparison_raw: u32) -> Condition {
        Condition {
            type_raw,
            comparison_raw,
            function: 72,
            param1_raw: 0,
            param2_raw: 0,
            run_on: None,
            reference_raw: None,
        }
    }

    #[test]
    fn condition_type_byte() {
        let c = cond(0x00, 1.0f32.to_bits());
        assert_eq!((c.operator(), c.or(), c.use_global(), c.comparison_f32()), (Some("=="), false, false, Some(1.0)));
        let c = cond(0x60 | 0x01, 0);
        assert_eq!((c.operator(), c.or()), (Some(">="), true));
        let c = cond(0xA0 | 0x04, 0x0001_2345);
        assert_eq!((c.operator(), c.use_global(), c.comparison_f32()), (Some("<="), true, None));
        assert_eq!(cond(0xC0, 0).operator(), None);
    }

    #[test]
    fn condition_from_decoded_value() {
        let v = json!({"type": 0x40, "unused": [0, 0, 0], "comparison_value_raw": 0x3F80_0000u32, "function": 14,
                       "unused2": [0, 0], "parameter1_raw": 5, "parameter2_raw": 0});
        let c = Condition::from_value(&v).unwrap();
        assert_eq!((c.function, c.param1_raw, c.run_on, c.operator()), (14, 5, None, Some(">")));
        assert_eq!(c.comparison_f32(), Some(1.0));
    }

    fn idle(id: u32, parent: Option<u32>, previous: Option<u32>) -> Idle {
        Idle {
            id: FormId(id),
            edid: None,
            plugin: 0,
            model: None,
            conditions: Vec::new(),
            parent: parent.map(FormId),
            previous: previous.map(FormId),
            group_raw: Some(0x87),
            loop_min: None,
            loop_max: None,
            replay_delay: None,
            flags: None,
        }
    }

    #[test]
    fn sections_and_models() {
        let mut i = idle(1, None, None);
        assert_eq!((i.section(), i.section_name(), i.group_high_bits()), (Some(7), Some("Special Idle"), Some(0x80)));
        i.group_raw = Some(0x40 | 21);
        assert_eq!((i.section_name(), i.group_high_bits()), (Some("Upper Body"), Some(0x40)));
        i.group_raw = Some(9);
        assert_eq!(i.section_name(), None);
        assert_eq!(i.model_kind(), ModelKind::None);
        i.model = Some("Characters\\_Male\\IdleAnims\\SpecialIdle_Shrug.KF".into());
        assert_eq!(i.model_kind(), ModelKind::Kf);
        assert_eq!(i.model_path().as_deref(), Some("meshes\\characters\\_male\\idleanims\\specialidle_shrug.kf"));
        i.model = Some("Creatures\\Dog\\IdleAnims".into());
        assert_eq!(i.model_kind(), ModelKind::Other);
    }

    fn tree(list: &[Idle]) -> Idles {
        Idles { idles: list.iter().map(|i| (i.id, i.clone())).collect(), issues: Issues::default() }
    }

    #[test]
    fn sibling_order_follows_previous_links() {
        // Root 1 with children 4 → 2 → 3 (by previous), root 10 after root 1.
        let t = tree(&[
            idle(1, None, None),
            idle(10, None, Some(1)),
            idle(2, Some(1), Some(4)),
            idle(3, Some(1), Some(2)),
            idle(4, Some(1), None),
        ]);
        assert_eq!(t.children(None), vec![FormId(1), FormId(10)]);
        assert_eq!(t.children(Some(FormId(1))), vec![FormId(4), FormId(2), FormId(3)]);
        let st = t.tree_stats();
        assert_eq!((st.idles, st.roots, st.sibling_groups, st.sibling_groups_one_chain), (5, 2, 2, 2));
        assert_eq!(st.by_depth, BTreeMap::from([(0, 2), (1, 3)]));
        assert_eq!((st.dangling_parent, st.dangling_previous, st.previous_other_parent, st.in_parent_cycle), (0, 0, 0, 0));
    }

    #[test]
    fn broken_links_are_counted() {
        let t = tree(&[
            idle(1, None, None),
            idle(2, Some(1), None),
            idle(3, Some(1), None), // second chain start: not one chain
            idle(4, Some(99), Some(98)),
            idle(5, Some(1), Some(4)), // previous has another parent
            idle(6, Some(7), None),
            idle(7, Some(6), None), // parent cycle
        ]);
        let st = t.tree_stats();
        assert_eq!((st.dangling_parent, st.dangling_previous, st.previous_other_parent), (1, 1, 1));
        assert_eq!(st.in_parent_cycle, 2);
        assert_eq!(t.depth(FormId(4)), Some(1));
        assert_eq!(t.depth(FormId(6)), None);
        assert_eq!(t.children(Some(FormId(1))), vec![FormId(2), FormId(3), FormId(5)]);
        assert!(st.sibling_groups_one_chain < st.sibling_groups);
    }
}
