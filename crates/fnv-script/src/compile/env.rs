//! [`Env`] over a load order: Editor IDs, record types, the script a form runs and that
//! script's variables, as one plugin sees them (the plugin and its masters, last
//! definition wins), the way the editor saw them when it compiled that plugin's scripts
//! [hypothesis: the editor resolves names against the active file and its masters].

use super::{Env, FormInfo};
use crate::unit::{ScriptUnit, UnitOwner, VarDecl};
use fnv_esm::LoadOrder;
use rayon::prelude::*;
use std::collections::HashMap;

/// One definition of a form.
#[derive(Debug, Clone)]
struct Def {
    plugin: usize,
    sig: [u8; 4],
    edid: Option<String>,
    /// `SCRI` (global).
    script: Option<u32>,
    /// `NAME` of a placed reference (global).
    base: Option<u32>,
    /// `TPLT` of an actor base (`NPC_`/`CREA`, global).
    template: Option<u32>,
    /// `ACBS` `template_flags` bit 9 (`Use Script`) on `NPC_`/`CREA` [documented:
    /// `schemas/fnv/gameplay.json`, xEdit `wbTemplateFlags`; GECK wiki Template Data].
    template_script: bool,
}

/// Template chain limit, matching `fnv_game::templates::MAX_DEPTH`.
const MAX_TEMPLATE_DEPTH: usize = 16;

/// `ACBS` `template_flags` offset (bytes 22..24) and bit 9 (`Script` / `Use Script`)
/// [documented: `schemas/fnv/gameplay.json` NPC_/CREA ACBS].
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 22;
const ACBS_TEMPLATE_SCRIPT_BIT: u16 = 1 << 9;

pub struct LoadOrderIndex<'a> {
    lo: &'a LoadOrder<'a>,
    visible: Vec<Vec<bool>>,
    defs: HashMap<u32, Vec<Def>>,
    /// Lower-case Editor ID → FormIDs that carry it, in load order.
    edids: HashMap<String, Vec<u32>>,
    /// Script FormID → (plugin, variables), in load order.
    scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>>,
}

fn formid(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

fn acbs_template_script(data: &[u8]) -> bool {
    data.get(ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .is_some_and(|b| u16::from_le_bytes(b) & ACBS_TEMPLATE_SCRIPT_BIT != 0)
}

impl<'a> LoadOrderIndex<'a> {
    pub fn build(lo: &'a LoadOrder<'a>, units: &[ScriptUnit]) -> LoadOrderIndex<'a> {
        let n = lo.plugins.len();
        let visible = (0..n)
            .map(|i| {
                let mut v = vec![false; n];
                v[i] = true;
                for &m in lo.master_map(i) {
                    v[m as usize] = true;
                }
                v
            })
            .collect();
        let read: Vec<(u32, Def)> = (0..n)
            .into_par_iter()
            .flat_map_iter(|pi| {
                lo.records[pi]
                    .par_iter()
                    .filter_map(|r| {
                        let form = lo.to_global(pi, r.header.form_id)?;
                        let actor = matches!(&r.header.sig, b"NPC_" | b"CREA");
                        let mut d = Def { plugin: pi, sig: r.header.sig, edid: None, script: None, base: None, template: None, template_script: false };
                        r.visit_subrecords(|sig, data| match sig {
                            b"EDID" if d.edid.is_none() => d.edid = Some(fnv_util::text::decode_z(data)),
                            b"SCRI" if d.script.is_none() => d.script = formid(data).and_then(|l| lo.to_global(pi, l)),
                            b"NAME" if d.base.is_none() && matches!(&r.header.sig, b"REFR" | b"ACHR" | b"ACRE") => {
                                d.base = formid(data).and_then(|l| lo.to_global(pi, l))
                            }
                            b"TPLT" if actor && d.template.is_none() => d.template = formid(data).and_then(|l| lo.to_global(pi, l)),
                            b"ACBS" if actor => d.template_script = acbs_template_script(data),
                            _ => {}
                        })
                        .ok()?;
                        Some((form, d))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut defs: HashMap<u32, Vec<Def>> = HashMap::new();
        let mut edids: HashMap<String, Vec<u32>> = HashMap::new();
        for (form, d) in read {
            if let Some(e) = &d.edid {
                let v = edids.entry(e.to_ascii_lowercase()).or_default();
                if !v.contains(&form) {
                    v.push(form);
                }
            }
            defs.entry(form).or_default().push(d);
        }
        for v in defs.values_mut() {
            v.sort_by_key(|d| d.plugin);
        }
        let mut scripts: HashMap<u32, Vec<(usize, Vec<VarDecl>)>> = HashMap::new();
        for u in units {
            if let (UnitOwner::Script { form }, Some(pi)) = (&u.owner, lo.plugins.iter().position(|p| p.name == u.plugin)) {
                scripts.entry(*form).or_default().push((pi, u.variables.clone()));
            }
        }
        LoadOrderIndex { lo, visible, defs, edids, scripts }
    }

    /// The names as plugin `plugin` (an index into the load order) sees them.
    pub fn env(&self, plugin: usize) -> PluginEnv<'_> {
        PluginEnv { ix: self, plugin }
    }

    pub fn plugin_index(&self, name: &str) -> Option<usize> {
        self.lo.plugins.iter().position(|p| p.name == name)
    }

    fn def(&self, form: u32, viewer: usize) -> Option<&Def> {
        self.defs.get(&form)?.iter().rev().find(|d| self.visible[viewer][d.plugin])
    }

    /// Every record the winning plugin holds under `form`, last first. A plugin can hold
    /// two records under one FormID: oHUD.esm has QUST `aHUD` and MESG `iHUDMainMenu`
    /// both at 0x01000801, and its scripts' compiled data name `aHUD` by that FormID and
    /// read its quest script's variables [observed: SCDA of SCPT 0x0100080F decompiles to
    /// `set 0x…801.f19`, its source says `set aHUD.mcmVersion`]; so the editor found the
    /// name and the script through the record that carries them, not only the last.
    fn defs_here(&self, form: u32, viewer: usize) -> impl Iterator<Item = &Def> {
        let last = self.def(form, viewer).map(|d| d.plugin);
        self.defs.get(&form).into_iter().flat_map(|v| v.iter().rev()).filter(move |d| Some(d.plugin) == last)
    }

    /// The script an actor base (`NPC_`/`CREA`) runs as `viewer` sees it: follow `TPLT`
    /// while `ACBS` template flag `Use Script` (bit 9) is set, stopping on a missing
    /// template or non-actor (such as a leveled list), and falling back to `base` on a
    /// cycle, matching `fnv_game::GameData::base_template_source`.
    fn actor_script(&self, base: u32, viewer: usize) -> Option<u32> {
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

    /// The script a base form runs as `viewer` sees it (following `TPLT` `Use Script` on
    /// `NPC_`/`CREA`).
    fn base_script(&self, base: u32, viewer: usize) -> Option<u32> {
        let d = self.def(base, viewer)?;
        if matches!(&d.sig, b"NPC_" | b"CREA") {
            self.actor_script(base, viewer)
        } else {
            d.script
        }
    }
}

/// Forms of the game itself that scripts name and no plugin defines: Editor ID, FormID
/// (below 0x800, the game's own range), record type. Only what was read:
/// `MapMarker` is 0x00000010 [observed: CASM with MCM 1.8's compiled script lists form
/// 0x00000010 where its source says `rRef.gbo == MapMarker`; the base game's map marker
/// references have that base, refs.md]; a static [hypothesis: nothing here reads the type].
const BUILT_IN: [(&str, u32, [u8; 4]); 1] = [("MapMarker", 0x0000_0010, *b"STAT")];

fn built_in(editor_id: &str) -> Option<FormInfo> {
    BUILT_IN.iter().find(|b| b.0.eq_ignore_ascii_case(editor_id)).map(|b| FormInfo { form: b.1, sig: b.2 })
}

pub struct PluginEnv<'a> {
    ix: &'a LoadOrderIndex<'a>,
    plugin: usize,
}

impl Env for PluginEnv<'_> {
    fn form(&self, editor_id: &str) -> Option<FormInfo> {
        let Some(cands) = self.ix.edids.get(&editor_id.to_ascii_lowercase()) else {
            return built_in(editor_id);
        };
        // The last visible definition whose Editor ID (as the viewer sees the form) matches.
        let mut best: Option<(usize, FormInfo)> = None;
        for &f in cands {
            if let Some(d) = self.ix.defs_here(f, self.plugin).find(|d| d.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(editor_id))) {
                if best.as_ref().is_none_or(|b| d.plugin >= b.0) {
                    best = Some((d.plugin, FormInfo { form: f, sig: d.sig }));
                }
            }
        }
        best.map(|b| b.1).or_else(|| built_in(editor_id))
    }

    fn info(&self, form: u32) -> Option<FormInfo> {
        self.ix.def(form, self.plugin).map(|d| FormInfo { form, sig: d.sig }).or_else(|| BUILT_IN.iter().find(|b| b.1 == form).map(|b| FormInfo { form, sig: b.2 }))
    }

    fn editor_id(&self, form: u32) -> Option<String> {
        match self.ix.def(form, self.plugin) {
            Some(d) => d.edid.clone(),
            None => BUILT_IN.iter().find(|b| b.1 == form).map(|b| b.0.to_string()),
        }
    }

    fn local_form(&self, local: u32) -> Option<u32> {
        let g = self.ix.lo.to_global(self.plugin, local)?;
        self.ix.def(g, self.plugin).map(|_| g)
    }

    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>> {
        let script = self.ix.defs_here(form, self.plugin).find_map(|d| {
            if matches!(&d.sig, b"NPC_" | b"CREA") {
                self.ix.actor_script(form, self.plugin)
            } else {
                d.script.or_else(|| self.ix.base_script(d.base?, self.plugin))
            }
        })?;
        let vis = &self.ix.visible[self.plugin];
        // The version the viewer sees; when only a later plugin's is at hand (a game reads
        // the winning records only), that one.
        let versions = self.ix.scripts.get(&script)?;
        versions.iter().rev().find(|(p, _)| vis[*p]).or_else(|| versions.last()).map(|(_, v)| v.clone())
    }
}
