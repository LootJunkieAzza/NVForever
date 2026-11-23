//! Which script a form runs, and that script's variables, across a load order.
//!
//! `ref.var` in the bytecode is a reference list entry plus a variable index of *another*
//! script (docs/spec-notes/scripts.md §2.5). That script is the quest's script (`SCRI`
//! on a QUST) or the script of a placed reference's base form (`NAME` → `SCRI`), or a
//! form's own `SCRI`. Plugins can override the target script, so the answer depends on
//! who looks: `Viewer::Plugin(i)` sees plugin `i`, its masters and nothing loaded later
//! (what the source was compiled against); `Viewer::LoadOrder` sees the winners (what
//! runs).

use crate::unit::{ScriptUnit, UnitOwner, VarDecl};
use fnv_esm::decode::{decode_sub, DecodeCtx};
use fnv_esm::{LoadOrder, Schema};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    /// Plugin `i` of the load order and its masters.
    Plugin(usize),
    /// The whole load order (last definition wins).
    LoadOrder,
}

/// One definition of a form: the plugin it is in, record type, its `SCRI`, `NAME`,
/// and (`NPC_`/`CREA`) `TPLT` / `ACBS` `Use Script` flag (global FormIDs).
#[derive(Debug, Clone, Copy)]
struct Def {
    plugin: usize,
    sig: [u8; 4],
    script: Option<u32>,
    base: Option<u32>,
    template: Option<u32>,
    template_script: bool,
}

const MAX_TEMPLATE_DEPTH: usize = 16;
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 22;
const ACBS_TEMPLATE_SCRIPT_BIT: u16 = 1 << 9;

pub struct ScriptIndex {
    plugin_names: Vec<String>,
    /// visible[i][j]: plugin i sees plugin j (j is i or one of its masters).
    visible: Vec<Vec<bool>>,
    defs: HashMap<u32, Vec<Def>>,
    /// Script FormID → (plugin, variables), in load order.
    scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>>,
}

impl ScriptIndex {
    /// Index the scripts among `units` and every form their reference lists name (plus
    /// the base forms of those).
    pub fn build(lo: &LoadOrder<'_>, schema: &Schema, units: &[ScriptUnit]) -> ScriptIndex {
        let plugin_names: Vec<String> = lo.plugins.iter().map(|p| p.name.clone()).collect();
        let pidx = |name: &str| plugin_names.iter().position(|n| n == name);
        let visible = (0..plugin_names.len())
            .map(|i| {
                let mut v = vec![false; plugin_names.len()];
                v[i] = true;
                for &m in lo.master_map(i) {
                    v[m as usize] = true;
                }
                v
            })
            .collect();
        let mut scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>> = HashMap::new();
        for u in units {
            if let (UnitOwner::Script { form }, Some(pi)) = (&u.owner, pidx(&u.plugin)) {
                scripts.entry(*form).or_default().push((pi, u.variables.clone()));
            }
        }
        let read = |forms: &HashSet<u32>| -> HashMap<u32, Vec<Def>> {
            forms
                .par_iter()
                .map(|&f| {
                    let defs = lo
                        .index
                        .get(&f)
                        .into_iter()
                        .flatten()
                        .map(|d| {
                            let pi = d.plugin as usize;
                            let r = lo.records[pi][d.ordinal as usize];
                            let mut script = None;
                            let mut base = None;
                            let mut template = None;
                            let mut template_script = false;
                            let actor = matches!(&r.header.sig, b"NPC_" | b"CREA");
                            if let Ok(subs) = r.subrecords() {
                                let mut ctx = DecodeCtx { schema, on_formid: None };
                                for s in &subs {
                                    if actor && &s.sig == b"ACBS" {
                                        template_script = s
                                            .data
                                            .get(ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2)
                                            .and_then(|b| <[u8; 2]>::try_from(b).ok())
                                            .is_some_and(|b| u16::from_le_bytes(b) & ACBS_TEMPLATE_SCRIPT_BIT != 0);
                                        continue;
                                    }
                                    let slot = match &s.sig {
                                        b"SCRI" if script.is_none() => &mut script,
                                        b"NAME" if base.is_none() => &mut base,
                                        b"TPLT" if actor && template.is_none() => &mut template,
                                        _ => continue,
                                    };
                                    let v = decode_sub(&r.header.sig, s, &mut ctx).value;
                                    *slot = v
                                        .as_ref()
                                        .and_then(|v| v.as_str())
                                        .and_then(|t| u32::from_str_radix(t.trim_start_matches("0x"), 16).ok())
                                        .and_then(|l| lo.to_global(pi, l));
                                }
                            }
                            Def { plugin: pi, sig: r.header.sig, script, base, template, template_script }
                        })
                        .collect();
                    (f, defs)
                })
                .collect()
        };
        let wanted: HashSet<u32> = units
            .iter()
            .flat_map(|u| u.refs.iter())
            .filter_map(|r| if let crate::unit::RefSlot::Form(f) = r { Some(*f) } else { None })
            .collect();
        let mut defs = read(&wanted);
        let bases: HashSet<u32> =
            defs.values().flatten().filter_map(|d| d.base).filter(|b| !defs.contains_key(b)).collect();
        defs.extend(read(&bases));
        for _ in 0..MAX_TEMPLATE_DEPTH {
            let templates: HashSet<u32> = defs
                .values()
                .flatten()
                .filter(|d| d.template_script)
                .filter_map(|d| d.template)
                .filter(|t| !defs.contains_key(t))
                .collect();
            if templates.is_empty() {
                break;
            }
            defs.extend(read(&templates));
        }
        ScriptIndex { plugin_names, visible, defs, scripts }
    }

    fn sees(&self, viewer: Viewer, plugin: usize) -> bool {
        match viewer {
            Viewer::LoadOrder => true,
            Viewer::Plugin(i) => self.visible.get(i).and_then(|v| v.get(plugin)).copied().unwrap_or(false),
        }
    }

    /// The viewer for a unit: the plugin it was read from.
    pub fn viewer_of(&self, unit: &ScriptUnit) -> Viewer {
        self.plugin_names.iter().position(|n| *n == unit.plugin).map(Viewer::Plugin).unwrap_or(Viewer::LoadOrder)
    }

    fn def(&self, form: u32, viewer: Viewer) -> Option<&Def> {
        self.defs.get(&form)?.iter().rev().find(|d| self.sees(viewer, d.plugin))
    }

    fn actor_script(&self, base: u32, viewer: Viewer) -> Option<u32> {
        let mut id = base;
        for _ in 0..MAX_TEMPLATE_DEPTH {
            let d = self.def(id, viewer)?;
            if !matches!(&d.sig, b"NPC_" | b"CREA") {
                return self.def(base, viewer)?.script;
            }
            match d.template.filter(|_| d.template_script) {
                Some(t) if self.def(t, viewer).is_some_and(|td| matches!(&td.sig, b"NPC_" | b"CREA")) => id = t,
                _ => return d.script,
            }
        }
        self.def(base, viewer)?.script
    }

    /// The script `form` runs: its own `SCRI`, or its base form's (following `TPLT` `Use
    /// Script` on `NPC_`/`CREA`).
    pub fn script_of(&self, form: u32, viewer: Viewer) -> Option<u32> {
        let d = self.def(form, viewer)?;
        if matches!(&d.sig, b"NPC_" | b"CREA") {
            return self.actor_script(form, viewer);
        }
        d.script.or_else(|| {
            let base = d.base?;
            let bd = self.def(base, viewer)?;
            if matches!(&bd.sig, b"NPC_" | b"CREA") {
                self.actor_script(base, viewer)
            } else {
                bd.script
            }
        })
    }

    /// The variables of a script as `viewer` sees it.
    pub fn variables(&self, script: u32, viewer: Viewer) -> Option<&[VarDecl]> {
        self.scripts.get(&script)?.iter().rev().find(|(p, _)| self.sees(viewer, *p)).map(|(_, v)| v.as_slice())
    }

    /// Variable `index` of the script attached to `form`.
    pub fn remote_var(&self, form: u32, index: u32, viewer: Viewer) -> Option<&VarDecl> {
        let s = self.script_of(form, viewer)?;
        self.variables(s, viewer)?.iter().find(|v| v.index == index)
    }
}

impl crate::decompile::Names for ScriptIndex {
    fn remote_var(&self, unit: &ScriptUnit, form: u32, index: u16) -> Option<String> {
        ScriptIndex::remote_var(self, form, index as u32, self.viewer_of(unit)).map(|v| v.name.clone())
    }
}
