//! Load order, FormID remapping and record overriding.
//!
//! A FormID stored in a plugin is *file-local*: its top byte indexes that file's own
//! master list (MAST order), and the value equal to the master count means "this
//! file". At runtime the top byte is replaced by the load-order index of the file it
//! refers to. Later files in the load order override earlier definitions of the same
//! (global) FormID. [documented: UESP "Tes4Mod:Mod File Format", FormID section;
//! behaviour consistent with every shipped master, observed]

use crate::plugin::Plugin;
use crate::raw::{visit_records, Record};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadOrderError {
    #[error("{0} plugins; at most 255 can be loaded")]
    TooManyPlugins(usize),
    #[error("{plugin} requires master {master}, which is not in the load order")]
    MissingMaster { plugin: String, master: String },
    #[error("{plugin} requires master {master}, which loads after it")]
    MasterAfterDependent { plugin: String, master: String },
    #[error("{0} appears twice in the load order")]
    Duplicate(String),
}

/// Location of one record definition: (plugin load index, ordinal in that plugin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefRef {
    pub plugin: u8,
    pub ordinal: u32,
}

pub struct LoadOrder<'a> {
    pub plugins: &'a [Plugin],
    /// For each plugin, maps its master-list index -> load-order index.
    master_map: Vec<Vec<u8>>,
    /// Records of each plugin in file order.
    pub records: Vec<Vec<&'a Record>>,
    /// Global FormID -> every definition in load order (last one wins).
    pub index: HashMap<u32, Vec<DefRef>>,
    /// Records whose own FormID could not be mapped (none since an out-of-range master
    /// index means the plugin itself; kept for reports).
    pub unmappable: Vec<(u8, u32)>,
}

impl<'a> LoadOrder<'a> {
    pub fn new(plugins: &'a [Plugin]) -> Result<LoadOrder<'a>, LoadOrderError> {
        if plugins.len() > 255 {
            return Err(LoadOrderError::TooManyPlugins(plugins.len()));
        }
        let lower: Vec<String> = plugins.iter().map(|p| p.name.to_ascii_lowercase()).collect();
        for (i, n) in lower.iter().enumerate() {
            if lower[..i].contains(n) {
                return Err(LoadOrderError::Duplicate(plugins[i].name.clone()));
            }
        }
        let mut master_map = Vec::with_capacity(plugins.len());
        for (i, p) in plugins.iter().enumerate() {
            let mut m = Vec::with_capacity(p.header.masters.len());
            for mr in &p.header.masters {
                let ml = mr.name.to_ascii_lowercase();
                match lower.iter().position(|n| *n == ml) {
                    Some(j) if j < i => m.push(j as u8),
                    Some(_) => {
                        return Err(LoadOrderError::MasterAfterDependent {
                            plugin: p.name.clone(),
                            master: mr.name.clone(),
                        })
                    }
                    None => {
                        return Err(LoadOrderError::MissingMaster { plugin: p.name.clone(), master: mr.name.clone() })
                    }
                }
            }
            master_map.push(m);
        }
        let mut lo = LoadOrder { plugins, master_map, records: Vec::new(), index: HashMap::new(), unmappable: Vec::new() };
        for (pi, p) in plugins.iter().enumerate() {
            let mut recs: Vec<&'a Record> = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, _| recs.push(r));
            for (ord, r) in recs.iter().enumerate() {
                match lo.to_global(pi, r.header.form_id) {
                    Some(g) => lo.index.entry(g).or_default().push(DefRef { plugin: pi as u8, ordinal: ord as u32 }),
                    None => {
                        tracing::debug!(
                            plugin = %p.name,
                            form_id = format_args!("{:08X}", r.header.form_id),
                            "FormID's master index is outside this plugin's master list"
                        );
                        lo.unmappable.push((pi as u8, r.header.form_id))
                    }
                }
            }
            lo.records.push(recs);
        }
        Ok(lo)
    }

    /// Convert a FormID local to plugin `pi` into a load-order (global) FormID.
    ///
    /// A master index at or beyond the plugin's master list means the plugin itself, as
    /// the game reads it [observed: capture `l1_oob_formid`, GRA's 0x02000801 placed as
    /// GRA's own; a mod whose own records all carry 0x12 with one master loads them in
    /// the game; docs/spec-notes/esm.md]. So every FormID maps; `None` is kept for callers.
    pub fn to_global(&self, pi: usize, local: u32) -> Option<u32> {
        let idx = (local >> 24) as usize;
        let masters = &self.master_map[pi];
        let target = if idx < masters.len() { masters[idx] } else { pi as u8 };
        Some(((target as u32) << 24) | (local & 0x00FF_FFFF))
    }

    /// Convert a global FormID into one local to plugin `pi`, if `pi` can refer to it.
    pub fn to_local(&self, pi: usize, global: u32) -> Option<u32> {
        let target = (global >> 24) as u8;
        let low = global & 0x00FF_FFFF;
        if target as usize == pi {
            return Some(((self.master_map[pi].len() as u32) << 24) | low);
        }
        self.master_map[pi].iter().position(|&m| m == target).map(|i| ((i as u32) << 24) | low)
    }

    /// The winning (last-loaded) definition of a global FormID.
    pub fn winner(&self, global: u32) -> Option<(&'a Plugin, &'a Record)> {
        let d = *self.index.get(&global)?.last()?;
        Some((&self.plugins[d.plugin as usize], self.records[d.plugin as usize][d.ordinal as usize]))
    }

    pub fn exists(&self, global: u32) -> bool {
        self.index.contains_key(&global)
    }

    pub fn master_map(&self, pi: usize) -> &[u8] {
        &self.master_map[pi]
    }
}
