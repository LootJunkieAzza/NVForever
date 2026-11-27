//! Clean-room `Data/BaseObjectSwapper/*.ini` loader and placement swap table
//! (`docs/spec-notes/base-object-swapper.md`).
//!
//! Discovers and compiles data-driven base-object swap rules (`[Forms]`, `[References]`,
//! headerless files, `0xHEX~Plugin.esm` and `EditorID` specifiers, comma-separated swap
//! target lists, `NONE` filter placeholders, and `chanceS` / `chanceR` rolls) against the
//! active [`LoadOrder`].
//!
//! When no swap files exist or no valid rules are compiled, [`BosTable::is_empty`] is
//! `true` and [`BosTable::resolve`] returns the original base FormID immediately without
//! hashing or allocating.

use fnv_esm::LoadOrder;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Default session seed for `chanceR(...)` rolls [design].
const DEFAULT_RANDOM_SEED: u64 = 0x5EED_B050_2026_1121;
/// Domain-separation salts for SplitMix64 mixing [design].
const SEEDED_SALT: u64 = 0x9E37_79B9_7F4A_7C15;
const RANDOM_SALT: u64 = 0xD1B5_4A32_D192_ED03;
const VARIANT_SALT: u64 = 0x94D0_49BB_1331_11EB;

/// Chance modifier parsed from the optional fourth (`traits`) column of a swap rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Chance {
    /// Unconditional swap (no trait column, or `NONE`).
    Always,
    /// `chanceS(pct)`: deterministic per-reference percentage in `[0.0, 100.0]`.
    Seeded(f32),
    /// `chanceR(pct)`: session-seeded random percentage in `[0.0, 100.0]`.
    Random(f32),
}

/// One compiled swap rule targeting either a specific reference (`[References]`) or a
/// base form (`[Forms]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SwapRule {
    /// Resolved replacement base FormIDs (one entry for a single target; multiple for
    /// comma-separated variant lists).
    pub targets: Vec<u32>,
    pub chance: Chance,
    /// Monotonic index within the bucket so stacked rules for the same form receive
    /// independent hash streams.
    pub rule_index: u32,
}

/// Load statistics returned by [`BosTable::report`] for concise startup logging and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BosLoadReport {
    pub files_loaded: usize,
    pub form_rules: usize,
    pub reference_rules: usize,
    pub skipped_missing_plugin: usize,
    pub skipped_unknown_form: usize,
    pub skipped_unsupported_section: usize,
    pub skipped_unsupported_filter: usize,
    pub skipped_malformed: usize,
}

impl BosLoadReport {
    pub fn total_rules(&self) -> usize {
        self.form_rules + self.reference_rules
    }

    pub fn total_skipped(&self) -> usize {
        self.skipped_missing_plugin
            + self.skipped_unknown_form
            + self.skipped_unsupported_section
            + self.skipped_unsupported_filter
            + self.skipped_malformed
    }
}

/// Compiled Base Object Swapper lookup table for one load order.
#[derive(Debug, Clone)]
pub struct BosTable {
    by_ref: HashMap<u32, Vec<SwapRule>>,
    by_form: HashMap<u32, Vec<SwapRule>>,
    random_seed: u64,
    report: BosLoadReport,
}

impl Default for BosTable {
    fn default() -> Self {
        Self {
            by_ref: HashMap::new(),
            by_form: HashMap::new(),
            random_seed: DEFAULT_RANDOM_SEED,
            report: BosLoadReport::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Forms,
    References,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveErr {
    MissingPlugin,
    UnknownForm,
    Malformed,
}

struct FormResolver<'a> {
    lo: &'a LoadOrder<'a>,
    plugins: HashMap<String, usize>,
    edids: Option<HashMap<String, u32>>,
}

impl<'a> FormResolver<'a> {
    fn new(lo: &'a LoadOrder<'a>) -> Self {
        let mut plugins = HashMap::with_capacity(lo.plugins.len());
        for (i, p) in lo.plugins.iter().enumerate() {
            plugins.insert(p.name.to_ascii_lowercase(), i);
        }
        Self { lo, plugins, edids: None }
    }

    fn edid_map(&mut self) -> &HashMap<String, u32> {
        self.edids.get_or_insert_with(|| build_edid_map(self.lo))
    }

    fn resolve(&mut self, raw_spec: &str) -> Result<u32, ResolveErr> {
        let spec = raw_spec.trim();
        if spec.is_empty() {
            return Err(ResolveErr::Malformed);
        }
        if let Some((hex_part, plugin_part)) = spec.split_once('~') {
            let plugin = plugin_part.trim();
            if plugin.is_empty() {
                return Err(ResolveErr::Malformed);
            }
            let hex = hex_part
                .trim()
                .strip_prefix("0x")
                .or_else(|| hex_part.trim().strip_prefix("0X"))
                .unwrap_or_else(|| hex_part.trim());
            if hex.is_empty() || hex.len() > 8 {
                return Err(ResolveErr::Malformed);
            }
            let local_raw = u32::from_str_radix(hex, 16).map_err(|_| ResolveErr::Malformed)?;
            let Some(&pi) = self.plugins.get(&plugin.to_ascii_lowercase()) else {
                return Err(ResolveErr::MissingPlugin);
            };
            // Primary mapping: `0xHEX~Plugin.esm` names a record in `Plugin.esm`'s own
            // load-order slot using the lower 24 bits (tolerating 8-digit FormIDs whose
            // top byte held an author's load-order index) [design].
            let primary = ((pi as u32) << 24) | (local_raw & 0x00FF_FFFF);
            if self.lo.exists(primary) {
                return Ok(primary);
            }
            // Fallback: interpret `local_raw` through the plugin's master table when the
            // record is an override defined by a master of `Plugin.esm`.
            if let Some(mapped) = self.lo.to_global(pi, local_raw) {
                if self.lo.exists(mapped) {
                    return Ok(mapped);
                }
            }
            return Err(ResolveErr::UnknownForm);
        }

        // Bare EditorID lookup (case-insensitive).
        let key = spec.to_ascii_lowercase();
        self.edid_map().get(&key).copied().ok_or(ResolveErr::UnknownForm)
    }
}

/// Build a case-insensitive winning `EDID -> global FormID` map directly from raw record
/// subrecords (no schema decode needed; called lazily only when at least one BOS INI uses
/// a bare EditorID).
fn build_edid_map(lo: &LoadOrder<'_>) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for (&fid, defs) in &lo.index {
        let Some(&d) = defs.last() else { continue };
        let rec = lo.records[d.plugin as usize][d.ordinal as usize];
        let Ok(subs) = rec.subrecords() else { continue };
        for s in subs {
            if &s.sig == b"EDID" {
                let end = s.data.iter().position(|&b| b == 0).unwrap_or(s.data.len());
                if end > 0 {
                    if let Ok(edid) = std::str::from_utf8(&s.data[..end]) {
                        let trimmed = edid.trim();
                        if !trimmed.is_empty() {
                            map.insert(trimmed.to_ascii_lowercase(), fid);
                        }
                    }
                }
                break;
            }
        }
    }
    map
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Map a 64-bit hash uniformly into `[0.0, 100.0)`.
fn hash_to_percent(h: u64) -> f32 {
    // 24 bits of mantissa precision in [0, 1), scaled to [0, 100).
    let unit = ((h >> 40) as f32) / ((1u32 << 24) as f32);
    unit * 100.0
}

fn parse_chance_traits(raw: &str) -> Result<Chance, ()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("NONE") {
        return Ok(Chance::Always);
    }
    let mut chance = Chance::Always;
    for token in trimmed.split([',', ' ', '\t']).map(str::trim).filter(|t| !t.is_empty()) {
        if token.eq_ignore_ascii_case("NONE") {
            continue;
        }
        let open = token.find('(').ok_or(())?;
        let close = token.rfind(')').ok_or(())?;
        if close != token.len() - 1 || close <= open + 1 {
            return Err(());
        }
        let name = token[..open].trim();
        let arg = token[open + 1..close].trim();
        let pct: f32 = arg.parse().map_err(|_| ())?;
        if !pct.is_finite() {
            return Err(());
        }
        let clamped = pct.clamp(0.0, 100.0);
        if name.eq_ignore_ascii_case("chanceS") {
            chance = Chance::Seeded(clamped);
        } else if name.eq_ignore_ascii_case("chanceR") {
            chance = Chance::Random(clamped);
        } else {
            return Err(());
        }
    }
    Ok(chance)
}

fn evaluate_rules(rules: &[SwapRule], ref_id: u32, random_seed: u64) -> Option<u32> {
    let mut chosen = None;
    for rule in rules {
        let key = ((ref_id as u64) << 32) | (rule.rule_index as u64);
        let fires = match rule.chance {
            Chance::Always => true,
            Chance::Seeded(pct) => {
                if pct <= 0.0 {
                    false
                } else if pct >= 100.0 {
                    true
                } else {
                    hash_to_percent(splitmix64(key ^ SEEDED_SALT)) < pct
                }
            }
            Chance::Random(pct) => {
                if pct <= 0.0 {
                    false
                } else if pct >= 100.0 {
                    true
                } else {
                    hash_to_percent(splitmix64(key ^ random_seed ^ RANDOM_SALT)) < pct
                }
            }
        };
        if !fires || rule.targets.is_empty() {
            continue;
        }
        let target = if rule.targets.len() == 1 {
            rule.targets[0]
        } else {
            let salt = match rule.chance {
                Chance::Random(_) => random_seed ^ VARIANT_SALT,
                Chance::Always | Chance::Seeded(_) => VARIANT_SALT,
            };
            let idx = (splitmix64(key ^ salt) as usize) % rule.targets.len();
            rule.targets[idx]
        };
        chosen = Some(target);
    }
    chosen
}

impl BosTable {
    /// True when no valid swap rules are loaded (fast no-op path).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.by_ref.is_empty() && self.by_form.is_empty()
    }

    /// Concise load counts (files, rules, skipped entries).
    pub fn report(&self) -> &BosLoadReport {
        &self.report
    }

    /// Override the session seed used for `chanceR(...)` rolls.
    pub fn with_random_seed(mut self, seed: u64) -> Self {
        self.random_seed = seed;
        self
    }

    /// Current session seed used for `chanceR(...)` rolls.
    pub fn random_seed(&self) -> u64 {
        self.random_seed
    }

    /// Resolve the effective base FormID for placed reference `ref_id` whose authored base
    /// is `orig_base`.
    ///
    /// Precedence:
    /// 1. When empty, returns `orig_base` immediately with zero lookups `[design]`.
    /// 2. `[References]` rules matching `ref_id` win over `[Forms]` rules `[hypothesis]`.
    /// 3. `[Forms]` rules matching `orig_base` run in deterministic file/line order `[hypothesis]`.
    #[inline]
    pub fn resolve(&self, ref_id: u32, orig_base: u32) -> u32 {
        if self.is_empty() {
            return orig_base;
        }
        if let Some(rules) = self.by_ref.get(&ref_id) {
            if let Some(swapped) = evaluate_rules(rules, ref_id, self.random_seed) {
                return swapped;
            }
        }
        if let Some(rules) = self.by_form.get(&orig_base) {
            if let Some(swapped) = evaluate_rules(rules, ref_id, self.random_seed) {
                return swapped;
            }
        }
        orig_base
    }

    /// Parse and compile `(source_name, ini_text)` pairs against `lo` in deterministic
    /// case-insensitive name order.
    pub fn load_from_sources<'a>(
        lo: &LoadOrder<'_>,
        sources: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> BosTable {
        let mut items: Vec<(&'a str, &'a str)> = sources.into_iter().collect();
        if items.is_empty() {
            return BosTable::default();
        }
        items.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()).then_with(|| a.0.cmp(b.0)));

        let mut table = BosTable::default();
        let mut resolver = FormResolver::new(lo);

        for (_name, text) in items {
            table.report.files_loaded += 1;
            let text = text.strip_prefix('\u{feff}').unwrap_or(text);
            let mut section = Section::Forms; // Headerless files default to [Forms].

            for raw_line in text.lines() {
                let line = raw_line.split(';').next().unwrap_or("").trim();
                if line.is_empty() {
                    continue;
                }
                if line.starts_with('[') && line.ends_with(']') {
                    let header = line[1..line.len() - 1].trim();
                    section = if header.eq_ignore_ascii_case("Forms") {
                        Section::Forms
                    } else if header.eq_ignore_ascii_case("References") {
                        Section::References
                    } else {
                        Section::Unsupported
                    };
                    continue;
                }
                if section == Section::Unsupported {
                    table.report.skipped_unsupported_section += 1;
                    continue;
                }

                let cols: Vec<&str> = line.split('|').map(str::trim).collect();
                if cols.len() < 2 || cols.len() > 4 || cols[0].is_empty() || cols[1].is_empty() {
                    table.report.skipped_malformed += 1;
                    continue;
                }
                if let Some(&filters) = cols.get(2) {
                    if !filters.is_empty() && !filters.eq_ignore_ascii_case("NONE") {
                        table.report.skipped_unsupported_filter += 1;
                        continue;
                    }
                }
                let chance = match cols.get(3) {
                    Some(&traits) => match parse_chance_traits(traits) {
                        Ok(c) => c,
                        Err(()) => {
                            table.report.skipped_malformed += 1;
                            continue;
                        }
                    },
                    None => Chance::Always,
                };

                let orig = match resolver.resolve(cols[0]) {
                    Ok(fid) => fid,
                    Err(ResolveErr::MissingPlugin) => {
                        table.report.skipped_missing_plugin += 1;
                        continue;
                    }
                    Err(ResolveErr::UnknownForm) => {
                        table.report.skipped_unknown_form += 1;
                        continue;
                    }
                    Err(ResolveErr::Malformed) => {
                        table.report.skipped_malformed += 1;
                        continue;
                    }
                };

                let mut targets = Vec::new();
                let mut had_missing_plugin = false;
                let mut had_unknown = false;
                for spec in cols[1].split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    match resolver.resolve(spec) {
                        Ok(fid) => targets.push(fid),
                        Err(ResolveErr::MissingPlugin) => had_missing_plugin = true,
                        Err(ResolveErr::UnknownForm) => had_unknown = true,
                        Err(ResolveErr::Malformed) => {}
                    }
                }
                if targets.is_empty() {
                    if had_missing_plugin {
                        table.report.skipped_missing_plugin += 1;
                    } else if had_unknown {
                        table.report.skipped_unknown_form += 1;
                    } else {
                        table.report.skipped_malformed += 1;
                    }
                    continue;
                }

                let bucket = match section {
                    Section::Forms => {
                        table.report.form_rules += 1;
                        table.by_form.entry(orig).or_default()
                    }
                    Section::References => {
                        table.report.reference_rules += 1;
                        table.by_ref.entry(orig).or_default()
                    }
                    Section::Unsupported => unreachable!(),
                };
                let rule_index = bucket.len() as u32;
                bucket.push(SwapRule { targets, chance, rule_index });
            }
        }

        table
    }

    /// Discover and load all `*.ini` files directly inside `bos_dir` (ignoring non-`.ini`
    /// files and missing directories).
    pub fn load_from_dir(lo: &LoadOrder<'_>, bos_dir: &Path) -> BosTable {
        let Ok(rd) = std::fs::read_dir(bos_dir) else {
            return BosTable::default();
        };
        let mut files: Vec<(String, String)> = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let name = entry.file_name().to_string_lossy().into_owned();
            files.push((name, text));
        }
        if files.is_empty() {
            return BosTable::default();
        }
        BosTable::load_from_sources(lo, files.iter().map(|(n, t)| (n.as_str(), t.as_str())))
    }

    /// Discover and load `BaseObjectSwapper/*.ini` across layered data roots (lowest
    /// precedence first; a later root with the same INI filename overrides an earlier one).
    pub fn load_from_roots(lo: &LoadOrder<'_>, data_roots: &[&Path]) -> BosTable {
        let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
        for root in data_roots {
            let Some(bos_dir) = find_child_ci(root, "BaseObjectSwapper") else { continue };
            let Ok(rd) = std::fs::read_dir(&bos_dir) else { continue };
            for entry in rd.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
                    continue;
                }
                let key = entry.file_name().to_string_lossy().to_ascii_lowercase();
                by_name.insert(key, path);
            }
        }
        if by_name.is_empty() {
            return BosTable::default();
        }
        let mut files: Vec<(String, String)> = Vec::with_capacity(by_name.len());
        for (name, path) in by_name {
            if let Ok(bytes) = std::fs::read(&path) {
                files.push((name, String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
        BosTable::load_from_sources(lo, files.iter().map(|(n, t)| (n.as_str(), t.as_str())))
    }
}

fn find_child_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    let direct = parent.join(name);
    if direct.is_dir() {
        return Some(direct);
    }
    let rd = std::fs::read_dir(parent).ok()?;
    for entry in rd.flatten() {
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            let p = entry.path();
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_esm::{Plugin, Schema};

    fn sub(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 + data.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn z(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    fn rec(sig: &[u8; 4], fid: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.iter().flat_map(|s| s.iter().copied()).collect();
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&fid.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn grup(label: [u8; 4], gtype: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(&label);
        out.extend_from_slice(&gtype.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn tes4(masters: &[&str]) -> Vec<u8> {
        let mut subs = vec![sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0])];
        for m in masters {
            subs.push(sub(b"MAST", &z(m)));
            subs.push(sub(b"DATA", &[0; 8]));
        }
        rec(b"TES4", 0, &subs)
    }

    fn stat(fid: u32, edid: &str, model: &str) -> Vec<u8> {
        rec(b"STAT", fid, &[sub(b"EDID", &z(edid)), sub(b"MODL", &z(model))])
    }

    fn refr(fid: u32, base: u32, edid: Option<&str>) -> Vec<u8> {
        let mut subs = Vec::new();
        if let Some(e) = edid {
            subs.push(sub(b"EDID", &z(e)));
        }
        subs.push(sub(b"NAME", &base.to_le_bytes()));
        subs.push(sub(b"DATA", &[0u8; 24]));
        rec(b"REFR", fid, &subs)
    }

    fn build_two_plugin_fixture() -> Vec<Plugin> {
        // Master plugin (index 0): BaseMain.esm
        let cell_id: u32 = 0x0000_0200;
        let stats0 = [
            stat(0x0000_1001, "OrigBarrel", "clutter\\orig_barrel.nif"),
            stat(0x0000_1002, "OrigCrate", "clutter\\orig_crate.nif"),
            stat(0x0000_1003, "OrigCar", "vehicles\\orig_car.nif"),
        ]
        .concat();
        let cell = rec(b"CELL", cell_id, &[sub(b"EDID", &z("TestRoom")), sub(b"DATA", &[1])]);
        let refrs = [
            refr(0x0000_2001, 0x0000_1001, Some("RefBarrelA")),
            refr(0x0000_2002, 0x0000_1001, None),
            refr(0x0000_2003, 0x0000_1002, None),
            refr(0x0000_2004, 0x0000_1003, None),
        ]
        .concat();
        let cell_children = grup(cell_id.to_le_bytes(), 6, &grup(cell_id.to_le_bytes(), 9, &refrs));
        let cells = grup(*b"CELL", 0, &grup(0i32.to_le_bytes(), 2, &grup(0i32.to_le_bytes(), 3, &[cell, cell_children].concat())));
        let p0_bytes = [tes4(&[]), grup(*b"STAT", 0, &stats0), cells].concat();

        // Mod plugin (index 1): ModSwap.esp (master: BaseMain.esm)
        let stats1 = [
            stat(0x0100_0801, "SwapBarrel", "clutter\\swap_barrel.nif"),
            stat(0x0100_0802, "SwapCrate", "clutter\\swap_crate.nif"),
            stat(0x0100_0803, "SwapCarRed", "vehicles\\car_red.nif"),
            stat(0x0100_0804, "SwapCarBlue", "vehicles\\car_blue.nif"),
            stat(0x0100_0805, "SwapSpecialRef", "clutter\\special.nif"),
        ]
        .concat();
        let p1_bytes = [tes4(&["BaseMain.esm"]), grup(*b"STAT", 0, &stats1)].concat();

        vec![
            Plugin::parse(&p0_bytes, "BaseMain.esm".into()).unwrap(),
            Plugin::parse(&p1_bytes, "ModSwap.esp".into()).unwrap(),
        ]
    }

    #[test]
    fn empty_table_is_fast_noop() {
        let plugins = build_two_plugin_fixture();
        let lo = LoadOrder::new(&plugins).unwrap();
        let table = BosTable::load_from_sources(&lo, std::iter::empty());
        assert!(table.is_empty());
        assert_eq!(table.report(), &BosLoadReport::default());
        assert_eq!(table.resolve(0x0000_2001, 0x0000_1001), 0x0000_1001);
    }

    #[test]
    fn forms_references_headerless_hex_edid_and_precedence() {
        let plugins = build_two_plugin_fixture();
        let lo = LoadOrder::new(&plugins).unwrap();
        let headerless = "\u{feff}; headerless defaults to [Forms]\nOrigCrate|SwapCrate|NONE|chanceS(100.0)\n";
        let main_ini = "\
[Forms]
; 6-digit hex and 8-digit hex with author load-order prefix, case-insensitive plugin names
0x001001~basemain.esm|0x01000801~MODSWAP.ESP|NONE|NONE
; Missing plugin typo and unknown EditorID are skipped safely
0x001001~BaseMainn.esm|SwapBarrel
MissingForm|SwapBarrel
OrigBarrel|MissingTarget
; Non-NONE filters and unsupported sections are skipped safely
OrigBarrel|SwapCrate|SomeCellFilter|NONE

[References]
; Reference override wins over [Forms] for Ref 0x00002001
0x00002001~BaseMain.esm|SwapSpecialRef

[Transforms]
0x00002001~BaseMain.esm|pos(1,2,3)
";
        let table = BosTable::load_from_sources(&lo, [("01_headerless.ini", headerless), ("02_main.ini", main_ini)]);
        assert!(!table.is_empty());
        let r = table.report();
        assert_eq!(r.files_loaded, 2);
        assert_eq!(r.form_rules, 2);
        assert_eq!(r.reference_rules, 1);
        assert_eq!(r.skipped_missing_plugin, 1);
        assert_eq!(r.skipped_unknown_form, 2);
        assert_eq!(r.skipped_unsupported_filter, 1);
        assert_eq!(r.skipped_unsupported_section, 1);
        assert_eq!(r.skipped_malformed, 0);

        // [References] wins over [Forms] for 0x2001.
        assert_eq!(table.resolve(0x0000_2001, 0x0000_1001), 0x0100_0805);
        // [Forms] swaps 0x2002 (same OrigBarrel base, no [References] override).
        assert_eq!(table.resolve(0x0000_2002, 0x0000_1001), 0x0100_0801);
        // Headerless file swapped OrigCrate -> SwapCrate.
        assert_eq!(table.resolve(0x0000_2003, 0x0000_1002), 0x0100_0802);
        // Untouched base form passes through unchanged.
        assert_eq!(table.resolve(0x0000_2004, 0x0000_1003), 0x0000_1003);
    }

    #[test]
    fn chance_s_determinism_chance_r_bounds_and_multi_variant_selection() {
        let plugins = build_two_plugin_fixture();
        let lo = LoadOrder::new(&plugins).unwrap();

        // chanceS(50) is deterministic per RefID across repeated queries and reloads.
        let ini_seeded = "[Forms]\nOrigCar|SwapCarRed|NONE|chanceS(50)\nOrigCar|SwapCarBlue|NONE|chanceS(50)\n";
        let t1 = BosTable::load_from_sources(&lo, [("cars.ini", ini_seeded)]);
        let t2 = BosTable::load_from_sources(&lo, [("cars.ini", ini_seeded)]);
        let (mut kept, mut red, mut blue) = (0usize, 0usize, 0usize);
        for rid in 0x0001_0000u32..0x0001_0400 {
            let a = t1.resolve(rid, 0x0000_1003);
            let b = t2.resolve(rid, 0x0000_1003);
            assert_eq!(a, b, "chanceS must be deterministic for ref 0x{rid:08X}");
            match a {
                0x0000_1003 => kept += 1,
                0x0100_0803 => red += 1,
                0x0100_0804 => blue += 1,
                other => panic!("unexpected target 0x{other:08X}"),
            }
        }
        assert!(kept > 100 && red > 100 && blue > 250, "expected spread across original/red/blue: kept={kept} red={red} blue={blue}");

        // chanceR(0) never fires, chanceR(100) always fires, chanceR(50) responds to random_seed.
        let ini_zero = "[Forms]\nOrigCar|SwapCarRed|NONE|chanceR(0)\n";
        let t_zero = BosTable::load_from_sources(&lo, [("z.ini", ini_zero)]);
        assert!((0x100..0x180).all(|r| t_zero.resolve(r, 0x0000_1003) == 0x0000_1003));

        let ini_full = "[Forms]\nOrigCar|SwapCarRed|NONE|chanceR(100)\n";
        let t_full = BosTable::load_from_sources(&lo, [("f.ini", ini_full)]);
        assert!((0x100..0x180).all(|r| t_full.resolve(r, 0x0000_1003) == 0x0100_0803));

        let ini_half = "[Forms]\nOrigCar|SwapCarRed|NONE|chanceR(50)\n";
        let s1 = BosTable::load_from_sources(&lo, [("h.ini", ini_half)]).with_random_seed(1);
        let s2 = BosTable::load_from_sources(&lo, [("h.ini", ini_half)]).with_random_seed(2);
        let differs = (0x200..0x280).any(|r| s1.resolve(r, 0x0000_1003) != s2.resolve(r, 0x0000_1003));
        assert!(differs, "distinct random seeds should vary chanceR(50) outcomes");

        // Comma-separated RHS target list picks deterministically across all valid targets.
        let ini_csv = "[Forms]\nOrigCar|SwapCarRed,SwapCarBlue\n";
        let t_csv = BosTable::load_from_sources(&lo, [("csv.ini", ini_csv)]);
        let (mut c_red, mut c_blue) = (0usize, 0usize);
        for r in 0x300..0x380 {
            let pick = t_csv.resolve(r, 0x0000_1003);
            assert_eq!(pick, t_csv.resolve(r, 0x0000_1003));
            match pick {
                0x0100_0803 => c_red += 1,
                0x0100_0804 => c_blue += 1,
                other => panic!("unexpected csv pick 0x{other:08X}"),
            }
        }
        assert!(c_red > 15 && c_blue > 15, "csv list should distribute across targets: red={c_red} blue={c_blue}");
    }

    #[test]
    fn dir_loader_ignores_non_ini_and_world_index_placement_applies_swaps() {
        let plugins = build_two_plugin_fixture();
        let lo = LoadOrder::new(&plugins).unwrap();
        let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();

        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("nvf_bos_test_{stamp}"));
        let bos_dir = dir.join("BaseObjectSwapper");
        std::fs::create_dir_all(&bos_dir).unwrap();
        std::fs::write(bos_dir.join("__folder_managed_by_vortex"), b"").unwrap();
        std::fs::write(bos_dir.join("01_barrels.ini"), b"[Forms]\nOrigBarrel|SwapBarrel\n").unwrap();
        std::fs::write(bos_dir.join("02_ref.ini"), b"[References]\n0x002001~BaseMain.esm|SwapSpecialRef\n").unwrap();

        // Missing folder returns empty table.
        assert!(BosTable::load_from_dir(&lo, &dir.join("DoesNotExist")).is_empty());

        let bos = BosTable::load_from_roots(&lo, &[dir.as_path()]);
        assert_eq!(bos.report().files_loaded, 2);
        assert_eq!(bos.report().form_rules, 1);
        assert_eq!(bos.report().reference_rules, 1);

        let mut ix = crate::WorldIndex::build(&lo, &schema);
        // Without BOS: authored base form.
        assert_eq!(ix.placement(&lo, &schema, 0x0000_2001).unwrap().base, Some(0x0000_1001));
        assert_eq!(ix.placement(&lo, &schema, 0x0000_2002).unwrap().base, Some(0x0000_1001));

        // With BOS applied on WorldIndex: [References] beats [Forms], [Forms] swaps remaining refs.
        ix.apply_bos(bos);
        assert_eq!(ix.placement(&lo, &schema, 0x0000_2001).unwrap().base, Some(0x0100_0805));
        assert_eq!(ix.placement(&lo, &schema, 0x0000_2002).unwrap().base, Some(0x0100_0801));
        assert_eq!(ix.placement(&lo, &schema, 0x0000_2003).unwrap().base, Some(0x0000_1002));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_sessions_with_different_seeds_vary_chance_r_while_keeping_chance_s_identical() {
        let plugins = build_two_plugin_fixture();
        let lo = LoadOrder::new(&plugins).unwrap();
        let ini = "[Forms]\nOrigBarrel|SwapBarrel|NONE|chanceS(50)\nOrigCar|SwapCarRed|NONE|chanceR(50)\n";

        let session_a = BosTable::load_from_sources(&lo, [("session.ini", ini)]).with_random_seed(0x1111_2222_3333_4444);
        let session_b = BosTable::load_from_sources(&lo, [("session.ini", ini)]).with_random_seed(0x9999_AAAA_BBBB_CCCC);
        assert_eq!(session_a.random_seed(), 0x1111_2222_3333_4444);
        assert_eq!(session_b.random_seed(), 0x9999_AAAA_BBBB_CCCC);

        // chanceS(50) stays identical for every reference across both sessions.
        for rid in 0x0002_0000u32..0x0002_0100 {
            assert_eq!(
                session_a.resolve(rid, 0x0000_1001),
                session_b.resolve(rid, 0x0000_1001),
                "chanceS must stay identical across sessions for ref 0x{rid:08X}"
            );
        }

        // chanceR(50) varies across sessions with different seeds.
        let r_diffs = (0x0002_0000u32..0x0002_0100)
            .filter(|&rid| session_a.resolve(rid, 0x0000_1003) != session_b.resolve(rid, 0x0000_1003))
            .count();
        assert!(r_diffs > 20, "expected distinct chanceR outcomes across sessions, got {r_diffs} diffs");
    }
}
