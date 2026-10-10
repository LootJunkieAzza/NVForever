//! Reading AI observer output (docs/spec-notes/ai-observer.md §3): one row per actor and
//! sample, FormIDs mapped from the game's load order to ours by plugin name. The captures
//! are game-derived and stay private; commands report summaries only.

use anyhow::anyhow;
use fnv_esm::LoadOrder;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

/// One actor in one sample.
pub struct Row<'a> {
    /// Index of the file among those read.
    pub file: usize,
    /// The sample's header values by name (`days`, `hour`, `player_cell`, …).
    header: &'a BTreeMap<String, Value>,
    fields: BTreeMap<&'a str, &'a str>,
    game_index: &'a BTreeMap<u8, usize>,
}

/// A FormID of the game's load order.
pub enum Form {
    /// Zero: none.
    None,
    /// In our load order.
    Mapped(u32),
    /// Made by the game while it runs (index 0xFF).
    Runtime,
    /// Of a plugin we did not load.
    Unmapped,
}

impl Row<'_> {
    pub fn num(&self, k: &str) -> Option<f32> {
        self.fields.get(k).and_then(|x| x.parse::<f32>().ok())
    }

    pub fn flag(&self, k: &str) -> bool {
        self.num(k).is_some_and(|v| v != 0.0)
    }

    /// A reference field, by what it is.
    pub fn form(&self, k: &str) -> Form {
        let Some(raw) = self.fields.get(k).and_then(|h| u32::from_str_radix(h, 16).ok()) else { return Form::None };
        match (raw, (raw >> 24) as u8) {
            (0, _) => Form::None,
            (_, 0xFF) => Form::Runtime,
            (_, i) => match self.game_index.get(&i) {
                Some(&pi) => Form::Mapped(((pi as u32) << 24) | (raw & 0x00FF_FFFF)),
                None => Form::Unmapped,
            },
        }
    }

    /// A reference field as a FormID of our load order.
    pub fn id(&self, k: &str) -> Option<u32> {
        match self.form(k) {
            Form::Mapped(g) => Some(g),
            _ => None,
        }
    }

    /// A number of the sample's header (`days`: GameDaysPassed, `hour`: GameHour).
    pub fn header_num(&self, k: &str) -> Option<f64> {
        self.header.get(k).and_then(Value::as_f64)
    }
}

/// Call `each` for every actor of every sample of `files`, in file order. An actor both
/// tracked and nearby is in a sample twice; it is passed once.
pub fn read(lo: &LoadOrder, files: &[PathBuf], mut each: impl FnMut(&Row)) -> anyhow::Result<()> {
    for (file, f) in files.iter().enumerate() {
        let text = std::fs::read_to_string(f).map_err(|e| anyhow!("{}: {e}", f.display()))?;
        let mut game_index: BTreeMap<u8, usize> = BTreeMap::new();
        let mut parts: Vec<Vec<String>> = Vec::new();
        let mut header_names: Vec<String> = Vec::new();
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            match v["kind"].as_str() {
                Some("result") if v["label"] == "__modindex__" => {
                    if let (Some(name), Some(i)) = (v["field"].as_str(), v["value"].as_f64()) {
                        if let Some(pi) = lo.plugins.iter().position(|p| p.name.eq_ignore_ascii_case(name)) {
                            game_index.insert(i as u8, pi);
                        }
                    }
                }
                Some("observing") => {
                    let names = |list: &Value| -> Vec<String> {
                        list.as_array().map(|fs| fs.iter().filter_map(|f| f[0].as_str().map(str::to_string)).collect()).unwrap_or_default()
                    };
                    parts = v["fields"]["parts"].as_array().map(|ps| ps.iter().map(names).collect()).unwrap_or_default();
                    header_names = names(&v["fields"]["header"]);
                }
                Some("sample") => {
                    let Some(groups) = v["groups"].as_object() else { continue };
                    let header: BTreeMap<String, Value> =
                        header_names.iter().cloned().zip(v["header"].as_array().into_iter().flatten().cloned()).collect();
                    let mut this_sample = HashSet::new();
                    for subjects in groups.values().filter_map(Value::as_array) {
                        for s in subjects.iter().filter_map(Value::as_array) {
                            let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
                            for (pi, part) in s.iter().enumerate() {
                                let (Some(text), Some(names)) = (part.as_str(), parts.get(pi)) else { continue };
                                for (n, val) in names.iter().zip(text.split('|')) {
                                    fields.insert(n.as_str(), val);
                                }
                            }
                            let Some(key) = fields.get("id").copied() else { continue };
                            if !this_sample.insert(key.to_string()) {
                                continue;
                            }
                            each(&Row { file, header: &header, fields, game_index: &game_index });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}
