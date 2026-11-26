//! Clean-room `Data\NVSE\Plugins\SoundExtender\*.json` / `*.txt` JSON config loader
//! (`docs/spec-notes/sound-extender.md`).
//!
//! Discovers and resolves per-form collision sound overrides (`mod`, `form`, `sound`)
//! against the active [`LoadOrder`]. When `Data\NVSE\Plugins\SoundExtender\` is absent
//! or contains no valid rules, [`SoundExtenderTable::is_empty`] is `true` and
//! [`SoundExtenderTable::resolve`] returns `None` immediately without hashing or
//! allocating, leaving vanilla collision sounds unchanged.

use fnv_esm::LoadOrder;
use fnv_sim::FormId;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Data-relative directory scanned for SoundExtender JSON configs (lower-case VFS prefix).
const VFS_PREFIX: &str = "nvse\\plugins\\soundextender\\";

/// Data-relative directory name passed to [`crate::mod_files`].
const MOD_FILES_DIR: &str = "nvse\\plugins\\soundextender";

/// ESM record header flag for a deleted record [documented: xEdit record header flags].
const RECORD_FLAG_DELETED: u32 = 0x0000_0020;

/// Load statistics returned by [`SoundExtenderTable::report`] for concise startup logging
/// and tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SoundExtenderReport {
    /// Number of valid JSON array config files (`.json` / `.txt`) parsed.
    pub files_loaded: usize,
    /// Number of config files skipped because they were unreadable, invalid UTF-8,
    /// malformed JSON, or did not have a top-level JSON array.
    pub files_malformed: usize,
    /// Number of valid entries resolved and applied (including overwrites of earlier rules).
    pub rules_applied: usize,
    /// Entries skipped because `"mod"` is not loaded in the active [`LoadOrder`].
    pub skipped_missing_plugin: usize,
    /// Entries skipped because `"form"` does not resolve to a live record in [`LoadOrder`].
    pub skipped_unknown_form: usize,
    /// Entries skipped because `"sound"` does not resolve to a live `SOUN` EditorID.
    pub skipped_unknown_sound: usize,
    /// Entries skipped because the element was not an object, lacked required string
    /// fields, or had a malformed hex FormID.
    pub skipped_malformed: usize,
}

impl SoundExtenderReport {
    /// Total skipped entries across all parsed files.
    pub fn total_skipped(&self) -> usize {
        self.skipped_missing_plugin
            + self.skipped_unknown_form
            + self.skipped_unknown_sound
            + self.skipped_malformed
    }
}

/// Resolved per-form collision sound overrides loaded from
/// `Data\NVSE\Plugins\SoundExtender\` (`*.json` and `*.txt`).
#[derive(Debug, Default, Clone)]
pub struct SoundExtenderTable {
    overrides: HashMap<FormId, FormId>,
    report: SoundExtenderReport,
}

impl SoundExtenderTable {
    /// True when no valid per-form sound overrides are loaded (fast vanilla no-op path).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    /// Number of distinct target forms mapped to a replacement `SOUN`.
    #[inline]
    pub fn len(&self) -> usize {
        self.overrides.len()
    }

    /// Concise load statistics (files parsed, malformed files, rules applied, skipped).
    pub fn report(&self) -> &SoundExtenderReport {
        &self.report
    }

    /// Resolve a SoundExtender per-form collision sound override for `base`.
    ///
    /// Returns `None` immediately with zero lookups when no overrides are loaded
    /// (`[design]`).
    #[inline]
    pub fn resolve(&self, base: FormId) -> Option<FormId> {
        if self.overrides.is_empty() {
            return None;
        }
        self.overrides.get(&base).copied()
    }

    /// Parse and resolve `(source_name, json_text)` pairs against `lo` in deterministic
    /// case-insensitive filename order (`[design]`).
    ///
    /// Malformed JSON files fail closed (`files_malformed += 1`) without panicking and do
    /// not prevent sibling valid files from loading.
    pub fn load_from_sources<'a>(
        lo: &LoadOrder<'_>,
        sources: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> SoundExtenderTable {
        let mut items: Vec<(&'a str, &'a str)> = sources.into_iter().collect();
        if items.is_empty() {
            return SoundExtenderTable::default();
        }
        items.sort_by(|a, b| {
            a.0.to_ascii_lowercase()
                .cmp(&b.0.to_ascii_lowercase())
                .then_with(|| a.0.cmp(b.0))
        });

        let mut table = SoundExtenderTable::default();
        let mut resolver = Resolver::new(lo);

        for (name, raw_text) in items {
            let text = raw_text.strip_prefix('\u{feff}').unwrap_or(raw_text);
            let value: Value = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(file = name, error = %e, "sound extender: skipping malformed JSON config");
                    table.report.files_malformed += 1;
                    continue;
                }
            };
            let Some(entries) = value.as_array() else {
                tracing::warn!(file = name, "sound extender: skipping config whose JSON root is not an array");
                table.report.files_malformed += 1;
                continue;
            };
            table.report.files_loaded += 1;

            for entry in entries {
                let Some(obj) = entry.as_object() else {
                    table.report.skipped_malformed += 1;
                    continue;
                };
                let (Some(plugin), Some(form_hex), Some(sound_edid)) = (
                    string_field(obj, "mod"),
                    string_field(obj, "form"),
                    string_field(obj, "sound"),
                ) else {
                    table.report.skipped_malformed += 1;
                    continue;
                };
                let target = match resolver.resolve_form(plugin, form_hex) {
                    Ok(id) => id,
                    Err(FormErr::Malformed) => {
                        table.report.skipped_malformed += 1;
                        continue;
                    }
                    Err(FormErr::MissingPlugin) => {
                        table.report.skipped_missing_plugin += 1;
                        continue;
                    }
                    Err(FormErr::UnknownForm) => {
                        table.report.skipped_unknown_form += 1;
                        continue;
                    }
                };
                let Some(sound) = resolver.resolve_sound(sound_edid) else {
                    table.report.skipped_unknown_sound += 1;
                    continue;
                };
                table.overrides.insert(target, sound);
                table.report.rules_applied += 1;
            }
        }

        table
    }

    /// Discover and load `Data\NVSE\Plugins\SoundExtender\` (`*.json` and `*.txt`) through
    /// the engine [`fnv_vfs::Vfs`] (case-insensitive, deterministic sorted order, layered
    /// roots and archives included).
    pub fn load_from_vfs(lo: &LoadOrder<'_>, vfs: &fnv_vfs::Vfs) -> SoundExtenderTable {
        let paths = vfs.paths(|p| {
            p.strip_prefix(VFS_PREFIX)
                .is_some_and(|rest| !rest.is_empty() && !rest.contains('\\') && is_config_filename(rest))
        });
        if paths.is_empty() {
            return SoundExtenderTable::default();
        }
        let mut files: Vec<(String, String)> = Vec::with_capacity(paths.len());
        let mut malformed = 0usize;
        for path in paths {
            let name = path.strip_prefix(VFS_PREFIX).unwrap_or(&path).to_string();
            let bytes = match vfs.read(&path) {
                Some(Ok(b)) => b,
                Some(Err(e)) => {
                    tracing::warn!(file = %path, error = %e, "sound extender: skipping unreadable config");
                    malformed += 1;
                    continue;
                }
                None => {
                    malformed += 1;
                    continue;
                }
            };
            match String::from_utf8(bytes) {
                Ok(text) => files.push((name, text)),
                Err(e) => {
                    tracing::warn!(file = %path, error = %e, "sound extender: skipping non-UTF-8 config");
                    malformed += 1;
                }
            }
        }
        let mut table = Self::load_from_sources(
            lo,
            files.iter().map(|(n, t)| (n.as_str(), t.as_str())),
        );
        table.report.files_malformed += malformed;
        table
    }

    /// Discover and load `Data\NVSE\Plugins\SoundExtender\` (`*.json` and `*.txt`) through
    /// [`crate::mod_files`] (case-insensitive, deterministic sorted order).
    pub fn load_from_mod_files(lo: &LoadOrder<'_>) -> SoundExtenderTable {
        let names: Vec<String> = crate::mod_files::files_in(MOD_FILES_DIR)
            .into_iter()
            .filter(|name| is_config_filename(name))
            .collect();
        if names.is_empty() {
            return SoundExtenderTable::default();
        }
        let mut files: Vec<(String, String)> = Vec::with_capacity(names.len());
        let mut malformed = 0usize;
        for name in names {
            let rel = format!("{MOD_FILES_DIR}\\{name}");
            let Some(bytes) = crate::mod_files::read_loose(&rel) else {
                tracing::warn!(file = %rel, "sound extender: skipping unreadable config");
                malformed += 1;
                continue;
            };
            match String::from_utf8(bytes) {
                Ok(text) => files.push((name, text)),
                Err(e) => {
                    tracing::warn!(file = %rel, error = %e, "sound extender: skipping non-UTF-8 config");
                    malformed += 1;
                }
            }
        }
        let mut table = Self::load_from_sources(
            lo,
            files.iter().map(|(n, t)| (n.as_str(), t.as_str())),
        );
        table.report.files_malformed += malformed;
        table
    }
}

/// Accept `.json` ([documented: Nexus Mods mod 78637 v1.0 "a JSON file with a name of your
/// choice"]) and `.txt` ([hypothesis: community configs shipped with `.txt` extension, e.g.
/// `NovaArizona.txt`]) while ignoring unrelated extensions and mod-manager marker files
/// (`[design]`).
fn is_config_filename(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json") || ext.eq_ignore_ascii_case("txt"))
}

fn string_field<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    let s = obj.get(key)?.as_str()?.trim();
    (!s.is_empty()).then_some(s)
}

/// Parse `"form"` as a hexadecimal FormID string (`1..=8` ASCII hex digits with an optional
/// `0x`/`0X` prefix) `[hypothesis: the Nexus Mods mod 78637 v1.0 example `"4353"` matches
/// `WeapSniperRifle` `0x00004353` in `FalloutNV.esm`, though the page text does not state
/// hexadecimal explicitly]`.
fn parse_hex_form_id(raw: &str) -> Option<u32> {
    let hex = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or_else(|| raw.trim());
    if hex.is_empty() || hex.len() > 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let val = u32::from_str_radix(hex, 16).ok()?;
    (val != 0).then_some(val)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormErr {
    Malformed,
    MissingPlugin,
    UnknownForm,
}

struct Resolver<'a> {
    lo: &'a LoadOrder<'a>,
    plugins: Option<HashMap<String, usize>>,
    sounds: Option<HashMap<String, FormId>>,
}

impl<'a> Resolver<'a> {
    fn new(lo: &'a LoadOrder<'a>) -> Self {
        Self { lo, plugins: None, sounds: None }
    }

    fn plugin_index(&mut self, plugin: &str) -> Option<usize> {
        let map = self.plugins.get_or_insert_with(|| {
            let mut m = HashMap::with_capacity(self.lo.plugins.len());
            for (i, p) in self.lo.plugins.iter().enumerate() {
                m.insert(p.name.to_ascii_lowercase(), i);
            }
            m
        });
        map.get(&plugin.to_ascii_lowercase()).copied()
    }

    fn is_live_record(&self, fid: u32) -> bool {
        let Some(defs) = self.lo.index.get(&fid) else {
            return false;
        };
        let Some(d) = defs.last() else {
            return false;
        };
        let rec = self.lo.records[d.plugin as usize][d.ordinal as usize];
        rec.header.flags & RECORD_FLAG_DELETED == 0
    }

    fn resolve_form(&mut self, plugin: &str, form_hex: &str) -> Result<FormId, FormErr> {
        let local_raw = parse_hex_form_id(form_hex).ok_or(FormErr::Malformed)?;
        let pi = self.plugin_index(plugin).ok_or(FormErr::MissingPlugin)?;

        // Primary mapping: interpret lower 24 bits in the named plugin's active load-order
        // slot (tolerating 8-digit FormIDs whose top byte carried an author's load-order
        // index) [design].
        let local24 = local_raw & 0x00FF_FFFF;
        if local24 != 0 {
            let primary = ((pi as u32) << 24) | local24;
            if self.is_live_record(primary) {
                return Ok(FormId(primary));
            }
        }
        // Fallback: interpret through the plugin's master table for overrides of a master
        // record referenced under the overriding plugin's name [design].
        if let Some(mapped) = self.lo.to_global(pi, local_raw) {
            if self.is_live_record(mapped) {
                return Ok(FormId(mapped));
            }
        }
        Err(FormErr::UnknownForm)
    }

    fn resolve_sound(&mut self, edid: &str) -> Option<FormId> {
        let map = self.sounds.get_or_insert_with(|| build_sound_edid_map(self.lo));
        map.get(&edid.to_ascii_lowercase()).copied()
    }
}

/// Build a case-insensitive winning `EDID -> SOUN FormId` index over live (non-deleted)
/// `SOUN` records in `lo`. Built lazily only when at least one SoundExtender config entry
/// resolves a sound.
fn build_sound_edid_map(lo: &LoadOrder<'_>) -> HashMap<String, FormId> {
    let mut map = HashMap::new();
    for (&fid, defs) in &lo.index {
        let Some(&d) = defs.last() else { continue };
        let rec = lo.records[d.plugin as usize][d.ordinal as usize];
        if &rec.header.sig != b"SOUN" || rec.header.flags & RECORD_FLAG_DELETED != 0 {
            continue;
        }
        let Ok(subs) = rec.subrecords() else { continue };
        for s in subs {
            if &s.sig == b"EDID" {
                let end = s.data.iter().position(|&b| b == 0).unwrap_or(s.data.len());
                if end > 0 {
                    if let Ok(edid) = std::str::from_utf8(&s.data[..end]) {
                        let trimmed = edid.trim();
                        if !trimmed.is_empty() {
                            map.insert(trimmed.to_ascii_lowercase(), FormId(fid));
                        }
                    }
                }
                break;
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_esm::Plugin;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nvf-sound-extender-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    fn rec_with_flags(sig: &[u8; 4], flags: u32, id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&body);
        out
    }

    fn rec(sig: &[u8; 4], id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        rec_with_flags(sig, 0, id, subs)
    }

    fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(sig);
        out.extend_from_slice(&[0u8; 12]);
        out.extend_from_slice(body);
        out
    }

    fn synthetic_plugins() -> Vec<Plugin> {
        // Plugin 0: Base.esm (master)
        let mut base = rec(
            b"TES4",
            0,
            &[sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0])],
        );
        let mut base_sounds = Vec::new();
        base_sounds.extend(rec(b"SOUN", 0x0000_0801, &[sub(b"EDID", &z("SynthSndOne"))]));
        base_sounds.extend(rec(b"SOUN", 0x0000_0802, &[sub(b"EDID", &z("SynthSndTwo"))]));
        base_sounds.extend(rec_with_flags(
            b"SOUN",
            RECORD_FLAG_DELETED,
            0x0000_0803,
            &[sub(b"EDID", &z("SynthDeletedSnd"))],
        ));
        base.extend(top(b"SOUN", &base_sounds));
        let mut base_misc = Vec::new();
        base_misc.extend(rec(b"MISC", 0x0000_4353, &[sub(b"EDID", &z("SynthBottle"))]));
        base_misc.extend(rec_with_flags(
            b"MISC",
            RECORD_FLAG_DELETED,
            0x0000_4354,
            &[sub(b"EDID", &z("SynthDeletedItem"))],
        ));
        base.extend(top(b"MISC", &base_misc));
        base.extend(top(
            b"STAT",
            &rec(b"STAT", 0x0000_4400, &[sub(b"EDID", &z("NotASound"))]),
        ));

        // Plugin 1: Addon.esp (master: Base.esm)
        let mut addon = rec(
            b"TES4",
            0,
            &[
                sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0]),
                sub(b"MAST", &z("Base.esm")),
                sub(b"DATA", &[0; 8]),
            ],
        );
        addon.extend(top(
            b"SOUN",
            &rec(b"SOUN", 0x0100_0901, &[sub(b"EDID", &z("AddonCustomPhys"))]),
        ));
        addon.extend(top(
            b"MISC",
            &rec(b"MISC", 0x0100_0E4B, &[sub(b"EDID", &z("AddonDrum"))]),
        ));

        vec![
            Plugin::parse(&base, "Base.esm".into()).unwrap(),
            Plugin::parse(&addon, "Addon.esp".into()).unwrap(),
        ]
    }

    #[test]
    fn absent_or_empty_sound_extender_dir_leaves_table_empty() {
        let plugins = synthetic_plugins();
        let lo = LoadOrder::new(&plugins).unwrap();

        let dir = temp_dir("absent");
        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let table = SoundExtenderTable::load_from_vfs(&lo, &vfs);
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
        assert_eq!(table.report(), &SoundExtenderReport::default());
        assert_eq!(table.resolve(FormId(0x0000_4353)), None);

        // Directory exists but holds only unrelated extensions/marker files and nested subdirectories.
        let se_dir = dir.join("NVSE").join("Plugins").join("SoundExtender");
        std::fs::create_dir_all(se_dir.join("nested")).unwrap();
        std::fs::write(se_dir.join("readme.md"), "not a sound extender config").unwrap();
        std::fs::write(se_dir.join("__folder_managed_by_vortex"), "").unwrap();
        std::fs::write(se_dir.join("nested").join("deep.json"), r#"[{"mod":"Base.esm","form":"4353","sound":"SynthSndOne"}]"#).unwrap();
        std::fs::write(se_dir.join("nested").join("deep.txt"), r#"[{"mod":"Base.esm","form":"4353","sound":"SynthSndOne"}]"#).unwrap();
        let vfs2 = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let table2 = SoundExtenderTable::load_from_vfs(&lo, &vfs2);
        assert!(table2.is_empty());
        assert_eq!(table2.report().files_loaded, 0);

        assert!(is_config_filename("custom.json"));
        assert!(is_config_filename("CUSTOM.JSON"));
        assert!(is_config_filename("NovaArizona.txt"));
        assert!(is_config_filename("LEGACY.TXT"));
        assert!(!is_config_filename("readme.md"));
        assert!(!is_config_filename("__folder_managed_by_vortex"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vfs_discovers_case_insensitively_with_sorted_precedence_and_layering() {
        let plugins = synthetic_plugins();
        let lo = LoadOrder::new(&plugins).unwrap();

        let base_data = temp_dir("vfs-base");
        let mod_layer = temp_dir("vfs-layer");
        let base_se = base_data.join("nvse").join("PlUgInS").join("soundextender");
        std::fs::create_dir_all(&base_se).unwrap();

        // 01_first.txt maps both Base.esm:4353 and Addon.esp:E4B (with UTF-8 BOM).
        std::fs::write(
            base_se.join("01_first.txt"),
            "\u{feff}[\n  {\"mod\": \"base.esm\", \"form\": \"4353\", \"sound\": \"synthSndOne\"},\n  {\"mod\": \"Addon.esp\", \"form\": \"0xE4B\", \"sound\": \"AddonCustomPhys\"}\n]\n",
        )
        .unwrap();
        // 02_SECOND.JSON overrides Base.esm:4353 deterministically because "02_second.json" > "01_first.txt".
        std::fs::write(
            base_se.join("02_SECOND.JSON"),
            r#"[{"mod": "Base.esm", "form": "00004353", "sound": "SynthSndTwo"}]"#,
        )
        .unwrap();

        let vfs = fnv_vfs::Vfs::open(&base_data, &[]).unwrap();
        let table = SoundExtenderTable::load_from_vfs(&lo, &vfs);
        assert_eq!(table.len(), 2);
        assert_eq!(table.report().files_loaded, 2);
        assert_eq!(table.report().files_malformed, 0);
        assert_eq!(table.report().rules_applied, 3);
        assert_eq!(table.report().total_skipped(), 0);
        assert_eq!(table.resolve(FormId(0x0000_4353)), Some(FormId(0x0000_0802)));
        assert_eq!(table.resolve(FormId(0x0100_0E4B)), Some(FormId(0x0100_0901)));

        // Higher-precedence VFS layer replacing 02_SECOND.JSON overrides base_data's copy.
        let layer_se = mod_layer.join("NVSE").join("Plugins").join("SoundExtender");
        std::fs::create_dir_all(&layer_se).unwrap();
        std::fs::write(
            layer_se.join("02_second.json"),
            r#"[{"mod": "Base.esm", "form": "4353", "sound": "AddonCustomPhys"}]"#,
        )
        .unwrap();
        let layered = fnv_vfs::Vfs::open_for_plugins_layered(&base_data, std::slice::from_ref(&mod_layer), &["Base.esm", "Addon.esp"]).unwrap();
        let layered_table = SoundExtenderTable::load_from_vfs(&lo, &layered);
        assert_eq!(layered_table.resolve(FormId(0x0000_4353)), Some(FormId(0x0100_0901)));

        let _ = std::fs::remove_dir_all(&base_data);
        let _ = std::fs::remove_dir_all(&mod_layer);
    }

    #[test]
    fn malformed_json_unknown_plugins_and_invalid_entries_fail_closed() {
        let plugins = synthetic_plugins();
        let lo = LoadOrder::new(&plugins).unwrap();

        let dir = temp_dir("malformed");
        let se_dir = dir.join("NVSE").join("Plugins").join("SoundExtender");
        std::fs::create_dir_all(&se_dir).unwrap();

        // Broken JSON, non-array root, invalid UTF-8 bytes, and one valid file with mixed bad entries.
        std::fs::write(se_dir.join("01_broken.txt"), "[{\"mod\": \"Base.esm\"").unwrap();
        std::fs::write(se_dir.join("02_object.txt"), r#"{"mod":"Base.esm","form":"4353","sound":"SynthSndOne"}"#).unwrap();
        std::fs::write(se_dir.join("03_bad_utf8.txt"), [0xFFu8, 0xFE, 0x00]).unwrap();
        std::fs::write(
            se_dir.join("04_mixed.txt"),
            r#"[
                "not-an-object",
                {"mod": "Base.esm", "form": 4353, "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "+4353", "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "0", "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "ZZZZ", "sound": "SynthSndOne"},
                {"mod": "MissingMod.esp", "form": "4353", "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "9999", "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "4354", "sound": "SynthSndOne"},
                {"mod": "Base.esm", "form": "4353", "sound": "NoSuchSound"},
                {"mod": "Base.esm", "form": "4353", "sound": "NotASound"},
                {"mod": "Base.esm", "form": "4353", "sound": "SynthDeletedSnd"},
                {"mod": "Addon.esp", "form": "00004353", "sound": "SynthSndOne"},
                {"mod": "Addon.esp", "form": "FF000E4B", "sound": "AddonCustomPhys"}
            ]"#,
        )
        .unwrap();

        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let table = SoundExtenderTable::load_from_vfs(&lo, &vfs);
        let rep = table.report();
        assert_eq!(rep.files_loaded, 1);
        assert_eq!(rep.files_malformed, 3);
        assert_eq!(rep.skipped_malformed, 5);
        assert_eq!(rep.skipped_missing_plugin, 1);
        assert_eq!(rep.skipped_unknown_form, 2);
        assert_eq!(rep.skipped_unknown_sound, 3);
        assert_eq!(rep.rules_applied, 2);
        // Master-override fallback: Addon.esp + 00004353 resolved Base.esm's 0x00004353.
        assert_eq!(table.resolve(FormId(0x0000_4353)), Some(FormId(0x0000_0801)));
        // 8-digit author load-order prefix FF000E4B mapped into Addon.esp's active slot 0x01.
        assert_eq!(table.resolve(FormId(0x0100_0E4B)), Some(FormId(0x0100_0901)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
