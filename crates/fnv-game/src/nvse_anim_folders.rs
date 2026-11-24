//! kNVSE's AnimGroupOverride folders (#1461; docs/spec-notes/knvse-anim-folders.md):
//! animation files a mod puts under `meshes\AnimGroupOverride`, with JSON files there
//! saying which forms they are for and on what condition, replace the animations of
//! actors with no plugin and no script [documented: GECK wiki CopyAnimationsToForm,
//! "the kNVSE AnimGroupOverride animations, including conditions and other attributes";
//! kNVSE Animation Plugin Nexus page (mod 71336) for `_1stPerson`/`_male` and `Mod1`–
//! `Mod3` weapon-mod subfolders; observed: the file trees of 25 installed mods,
//! 2026-11-03]. No kNVSE DLL or source code was consulted.
//!
//! The game keeps the entries read once from the folders and, a few times a second, the
//! files each drawn actor's matching entries give it ([`AnimFolders::active`]), which the
//! engine adds to SetActorAnimationPath's replacements (`Game::anim_overrides`).

use crate::game::Game;
use fnv_sim::FormId;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use serde_json::Value as Json;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The folder under meshes.
pub const ROOT: &str = "AnimGroupOverride";

/// Domain-separated seed mix for `AnimFolders`'s dedicated `chances.txt` RNG so weighted
/// animation picks never perturb `Game::rng` [design].
const CHANCES_RNG_MIX: u64 = 0xA11F_01D5;

/// How often non-polled entries are checked again, in frames (at 60 a second, four times
/// a second). Prior recorded [decision: the original checks a condition when an animation
/// starts, or every frame with `pollCondition` (hypothesis); a quarter second is close
/// enough for both and keeps the cost of the conditions small]. Proposed [design; pending
/// owner agreement to overturn the prior decision for `pollCondition: true` entries only]:
/// entries with `pollCondition: true` evaluate every frame while all other entries retain
/// the `REFRESH_FRAMES` cadence and reuse their cached matches between full refreshes.
const REFRESH_FRAMES: u64 = 15;

/// One weighted variant pool parsed from a directory's `chances.txt`: each `(path,
/// weight)` pair is a `.kf` path under meshes with a finite weight `> 0.0`, ordered
/// deterministically by path. Selection picks one file with probability proportional to
/// its weight [hypothesis: explicit file weights in `chances.txt`; design: fail-closed
/// validation and deterministic sampling via a dedicated `AnimFolders` RNG].
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedGroup {
    pub candidates: Vec<(String, f64)>,
}

/// Weapon-mod-gated animation files for a non-zero 3-bit weapon-mod mask (`1` = Mod 1,
/// `2` = Mod 2, `4` = Mod 3, or bitwise combinations `Mod12`, `Mod13`, `Mod23`, `Mod123`)
/// [documented: kNVSE Animation Plugin Nexus page (mod 71336) for `Mod1`–`Mod3`; GECK
/// wiki `GetEquippedWeaponModFlags` for bits 1, 2, 4; hypothesis: combined bitmask
/// folders].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModFilesBucket {
    pub mask: u8,
    pub files: Vec<String>,
    pub weighted: Vec<WeightedGroup>,
}

/// One entry of a JSON file.
#[derive(Debug, Clone, Default)]
pub struct FolderEntry {
    /// The forms it is for: actor references, base actors or weapons; empty for every
    /// actor [observed: 68 of 778 entries have no `mod`/`form`, their conditions test
    /// the actor (`GetCreatureType == -1`, `GetIsSex Female`)].
    pub forms: Vec<FormId>,
    /// The folder under [`ROOT`], as the file spells it.
    pub folder: String,
    /// A script condition, tested on the actor [observed: `this != Player`].
    pub condition: Option<String>,
    /// JSON `pollCondition: true`: evaluate this entry every frame instead of only on
    /// the periodic `REFRESH_FRAMES` cadence [observed: 12 of 778 entries in the 25-mod
    /// survey of 2026-11-03 set `"pollCondition": true`; hypothesis: continual polling
    /// in kNVSE; design: per-frame evaluation for this entry only, pending owner
    /// agreement to overturn the prior decision in `knvse-anim-folders.md` §3].
    pub poll_condition: bool,
    /// Higher wins over lower [hypothesis: values 1, 2 and 10 next to entries without
    /// one, the more special case given the higher number].
    pub priority: i64,
    /// Where it came in the files read (later wins at the same priority).
    pub order: usize,
    /// Its unconditional third-person (`_male` or direct) and first-person (`_1stPerson`
    /// or direct) base files (weapon-mod mask 0), under meshes.
    pub third: Vec<String>,
    pub first: Vec<String>,
    /// Weighted variant pools (`chances.txt`) at weapon-mod mask 0.
    pub third_weighted: Vec<WeightedGroup>,
    pub first_weighted: Vec<WeightedGroup>,
    /// Weapon-mod-gated buckets (`Mod1`–`Mod3` and combinations), sorted by
    /// `(mask.count_ones(), mask)` so more specific mod combinations override single-mod
    /// and base files [design: specificity ordering].
    pub third_mods: Vec<ModFilesBucket>,
    pub first_mods: Vec<ModFilesBucket>,
}

impl FolderEntry {
    fn has_any_files(&self) -> bool {
        !self.third.is_empty()
            || !self.first.is_empty()
            || !self.third_weighted.is_empty()
            || !self.first_weighted.is_empty()
            || self.third_mods.iter().any(|b| !b.files.is_empty() || !b.weighted.is_empty())
            || self.first_mods.iter().any(|b| !b.files.is_empty() || !b.weighted.is_empty())
    }
}

/// Context identifying an actor's current matching `AnimGroupOverride` state: equipped
/// weapon, equipped weapon-mod mask, and sorted matching entry orders. While this stays
/// unchanged across periodic polls, `chances.txt` picks remain stable [design].
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActorPickContext {
    weapon: Option<FormId>,
    weapon_mods: u8,
    entry_orders: Vec<usize>,
}

/// Identifies one `chances.txt` weighted pool within an actor's view:
/// `(first_person, entry.order, mod_mask, pool_index)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PoolKey {
    first: bool,
    entry_order: usize,
    mod_mask: u8,
    pool_index: usize,
}

#[derive(Debug, Clone)]
struct CachedActorPicks {
    context: ActorPickContext,
    chosen: HashMap<PoolKey, String>,
}

/// What the folders give.
#[derive(Debug)]
pub struct AnimFolders {
    loaded: bool,
    pub entries: Vec<FolderEntry>,
    /// Indices into `entries` of non-polled (`!poll_condition`) entries that matched each
    /// actor at the last full `REFRESH_FRAMES` (or `refresh_anim_folders_for`) evaluation,
    /// reused on intermediate frames so per-frame polling evaluates strictly
    /// `pollCondition: true` entries [design].
    cached_non_polled: HashMap<FormId, Vec<usize>>,
    /// The files each actor's entries give now, by (actor, first person), the lowest
    /// priority first (a later file of the same group wins, as with the engine's
    /// replacements).
    pub active: HashMap<(FormId, bool), Vec<String>>,
    /// Condition texts that failed, warned about once.
    warned: HashSet<String>,
    /// Dedicated RNG for `chances.txt` weighted picks, kept separate from `Game::rng` so
    /// animation packs never shift unrelated gameplay/script rolls [design]. Not saved.
    rng: ChaCha8Rng,
    /// Stable `chances.txt` picks per actor, retained until the actor's equipped weapon,
    /// weapon mods, or matching entry set changes [design]. Not saved.
    picks: HashMap<FormId, CachedActorPicks>,
}

impl Default for AnimFolders {
    fn default() -> Self {
        Self::with_seed(0)
    }
}

impl AnimFolders {
    pub(crate) fn with_seed(seed: u64) -> Self {
        Self {
            loaded: false,
            entries: Vec::new(),
            cached_non_polled: HashMap::new(),
            active: HashMap::new(),
            warned: HashSet::new(),
            rng: ChaCha8Rng::seed_from_u64(seed ^ CHANCES_RNG_MIX),
            picks: HashMap::new(),
        }
    }
}

/// Normalized lowercase folder key for deduplicating walked folders and identifying
/// nested sub-sets referenced by another JSON entry.
fn norm_folder(s: &str) -> String {
    s.replace('/', "\\").trim_matches('\\').to_ascii_lowercase()
}

/// An entry's `form`: one hex ID or a list of them [observed: both, 254 lists].
fn form_ids(v: Option<&Json>) -> Vec<u32> {
    let one = |v: &Json| match v {
        Json::String(s) => u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok(),
        Json::Number(n) => n.as_u64().map(|n| n as u32),
        _ => None,
    };
    match v {
        Some(Json::Array(a)) => a.iter().filter_map(one).collect(),
        Some(v) => one(v).into_iter().collect(),
        None => Vec::new(),
    }
}

/// Parse a weapon-mod folder segment (`Mod1`, `Mod2`, `Mod3`, or combined `Mod12`,
/// `Mod13`, `Mod23`, `Mod123`, any ASCII case) into a 3-bit weapon-mod bitmask (`1`, `2`,
/// `4`) [documented: kNVSE Animation Plugin Nexus page (mod 71336) for `Mod1`–`Mod3`;
/// GECK wiki `GetEquippedWeaponModFlags` for bits 1, 2, 4; hypothesis: combined bitmask
/// folder names]. Rejects non-mod names, out-of-range digits, and duplicate digits.
pub fn mod_folder_mask(segment: &str) -> Option<u8> {
    let lower = segment.to_ascii_lowercase();
    let digits = lower.strip_prefix("mod")?;
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    let mut mask = 0u8;
    for b in digits.bytes() {
        let bit = match b {
            b'1' => 1 << 0,
            b'2' => 1 << 1,
            b'3' => 1 << 2,
            _ => return None,
        };
        if (mask & bit) != 0 {
            return None;
        }
        mask |= bit;
    }
    Some(mask)
}

/// Strip an inline comment (`;`, `#`, or `//`) from a `chances.txt` line.
fn strip_chances_comment(line: &str) -> &str {
    let mut end = line.len();
    for marker in [";", "#", "//"] {
        if let Some(idx) = line.find(marker) {
            end = end.min(idx);
        }
    }
    &line[..end]
}

/// Parse a `chances.txt` file into lowercase file/stem keys mapped to non-negative finite
/// weights [hypothesis: `<file> = <weight>`, `<file>: <weight>%`, or `<file> <weight>`
/// lines; design: fail-closed validation ignores blank lines, comments (`;`, `#`, `//`),
/// malformed lines, negative weights, and non-finite weights while preserving explicit
/// `0` weights for exclusion].
pub fn parse_chances(text: &str) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    let text = text.trim_start_matches('\u{feff}');
    for raw_line in text.lines() {
        let line = strip_chances_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let pair = if let Some(idx) = line.find(['=', ':']) {
            Some((&line[..idx], &line[idx + 1..]))
        } else {
            line.rsplit_once(char::is_whitespace)
        };
        let Some((raw_key, raw_weight)) = pair else { continue };
        let key = raw_key
            .trim()
            .trim_matches('"')
            .trim()
            .replace('/', "\\")
            .rsplit('\\')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let weight_text = raw_weight.trim().strip_suffix('%').map_or(raw_weight.trim(), str::trim);
        if weight_text.is_empty() || weight_text.starts_with('-') {
            continue;
        }
        let Ok(weight) = weight_text.parse::<f64>() else { continue };
        if !weight.is_finite() || weight < 0.0 {
            continue;
        }
        out.insert(key, if weight == 0.0 { 0.0 } else { weight });
    }
    out
}

/// Select one candidate path from a weighted pool using a uniform sample `unit` in
/// `[0.0, 1.0)`. Entries with weight `<= 0.0` or non-finite weight are never selected;
/// returns `None` when `unit` is non-finite or no positive finite total exists
/// [design: deterministic fail-closed weighted selection].
pub fn select_weighted(candidates: &[(String, f64)], unit: f64) -> Option<&str> {
    if !unit.is_finite() {
        return None;
    }
    let u = unit.clamp(0.0, 1.0 - f64::EPSILON);
    let mut total = 0.0f64;
    let mut last_positive = None;
    for (path, weight) in candidates {
        if weight.is_finite() && *weight > 0.0 {
            total += *weight;
            last_positive = Some(path.as_str());
        }
    }
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let target = u * total;
    let mut acc = 0.0f64;
    for (path, weight) in candidates {
        if weight.is_finite() && *weight > 0.0 {
            acc += *weight;
            if target < acc {
                return Some(path.as_str());
            }
        }
    }
    last_positive
}

fn rng_unit(rng: &mut impl RngCore) -> f64 {
    ((rng.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
}

#[derive(Debug, Clone, Default)]
struct FolderScan {
    third: Vec<String>,
    first: Vec<String>,
    third_weighted: Vec<WeightedGroup>,
    first_weighted: Vec<WeightedGroup>,
    third_mods: Vec<ModFilesBucket>,
    first_mods: Vec<ModFilesBucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DirView {
    Direct,
    Third,
    First,
}

#[derive(Default)]
struct ViewAccum {
    base_files: Vec<String>,
    base_weighted: Vec<WeightedGroup>,
    mod_buckets: BTreeMap<(u32, u8), ModFilesBucket>,
}

impl ViewAccum {
    fn push_dir(&mut self, mask: u8, files: Vec<String>, weighted: Vec<WeightedGroup>) {
        if mask == 0 {
            self.base_files.extend(files);
            self.base_weighted.extend(weighted);
        } else {
            let bucket = self
                .mod_buckets
                .entry((mask.count_ones(), mask))
                .or_insert_with(|| ModFilesBucket { mask, ..Default::default() });
            bucket.files.extend(files);
            bucket.weighted.extend(weighted);
        }
    }
}

/// Look up a `.kf` filename in parsed `chances.txt` weights, checking the exact lowercase
/// filename first and then its stem without `.kf` [design].
fn lookup_chance(weights: &HashMap<String, f64>, file_name: &str) -> Option<f64> {
    let lower = file_name.to_ascii_lowercase();
    if let Some(&w) = weights.get(&lower) {
        return Some(w);
    }
    let stem = lower.strip_suffix(".kf")?;
    weights.get(stem).copied()
}

/// Partition matched weighted files in one directory into variant pools. When multiple
/// files share distinct underscore-delimited base stems (e.g., `2hrAttack3.kf` +
/// `2hrAttack3_a.kf` alongside `2hrReloadA.kf` + `2hrReloadA_a.kf`) and every matched
/// file belongs to a multi-file family, split into one pool per family; otherwise treat
/// all matched files in that directory as a single weighted pool [design].
fn build_weighted_groups(matched: Vec<(String, f64)>) -> Vec<WeightedGroup> {
    if matched.is_empty() {
        return Vec::new();
    }
    let stems: Vec<String> = matched
        .iter()
        .map(|(p, _)| {
            let name = p.rsplit('\\').next().unwrap_or(p).to_ascii_lowercase();
            name.strip_suffix(".kf").unwrap_or(&name).to_string()
        })
        .collect();
    let mut prefix_counts: HashMap<String, usize> = HashMap::new();
    for s in &stems {
        *prefix_counts.entry(s.clone()).or_insert(0) += 1;
        if let Some((prefix, _)) = s.rsplit_once('_') {
            if !prefix.is_empty() {
                *prefix_counts.entry(prefix.to_string()).or_insert(0) += 1;
            }
        }
    }
    let mut families: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for ((path, weight), stem) in matched.iter().cloned().zip(stems.iter()) {
        let fam = match stem.rsplit_once('_') {
            Some((prefix, _)) if !prefix.is_empty() && prefix_counts.get(prefix).copied().unwrap_or(0) >= 2 => prefix.to_string(),
            _ => stem.clone(),
        };
        families.entry(fam).or_default().push((path, weight));
    }
    let split_by_family = families.len() >= 2 && families.values().all(|g| g.len() >= 2);
    let raw_groups: Vec<Vec<(String, f64)>> = if split_by_family {
        families.into_values().collect()
    } else {
        vec![matched]
    };
    raw_groups
        .into_iter()
        .filter_map(|group| {
            let candidates: Vec<(String, f64)> = group.into_iter().filter(|(_, w)| w.is_finite() && *w > 0.0).collect();
            (!candidates.is_empty()).then_some(WeightedGroup { candidates })
        })
        .collect()
}

/// Apply optional `chances.txt` weights in one directory to its `.kf` files. Files not
/// named in `chances.txt` (or when `chances.txt` is absent/empty/author-notes-only with
/// zero matching `.kf` filenames) remain unconditional; matched files with weight `0` are
/// excluded, and positive-weighted matched files become [`WeightedGroup`] pools [design].
fn partition_dir_files(dir_data_path: &str, dir_files: Vec<String>) -> (Vec<String>, Vec<WeightedGroup>) {
    let chances_path = format!("{dir_data_path}\\chances.txt");
    let weights = crate::mod_files::read_loose(&chances_path)
        .map(|bytes| parse_chances(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();
    if weights.is_empty() {
        return (dir_files, Vec::new());
    }
    let mut unconditional = Vec::new();
    let mut matched = Vec::new();
    for path in &dir_files {
        let file_name = path.rsplit('\\').next().unwrap_or(path);
        match lookup_chance(&weights, file_name) {
            Some(w) => matched.push((path.clone(), w)),
            None => unconditional.push(path.clone()),
        }
    }
    if matched.is_empty() {
        return (dir_files, Vec::new());
    }
    (unconditional, build_weighted_groups(matched))
}

/// Walk one `AnimGroupOverride` set folder and classify its `.kf` files by view
/// (`_male`, `_1stPerson`, or view-unqualified direct `.kf`/named subfolders), weapon-mod
/// mask (`Mod1`–`Mod3`, `Mod12`, `Mod13`, `Mod23`, `Mod123`), and per-directory
/// `chances.txt` weights.
///
/// Nested child sets (subfolders containing `_male`/`_1stPerson` at a deeper level, or
/// subfolders named as their own `folder` by another JSON entry) and Mod Organizer
/// `*.mohidden` directories are excluded so child sets never leak into a parent set
/// [observed: nested sets such as `Pack\Flag\_1stPerson` and `_male.mohidden`; design].
fn folder_files(folder: &str, known_folders: &HashSet<String>) -> FolderScan {
    let base = format!("meshes\\{ROOT}\\{folder}");
    let folder_key = norm_folder(folder);
    // Group `.kf` files by `(DirView, mask, dir_rel_lower)` so `DirView::Direct` is
    // ordered before explicit `_male`/`_1stPerson` views (letting explicit view files
    // override direct files of the same clip name) and each directory's `chances.txt` is
    // evaluated once against its sibling `.kf` files.
    let mut dirs: BTreeMap<(DirView, u8, String), (String, Vec<String>)> = BTreeMap::new();
    for rel in crate::mod_files::files_under(&base) {
        if !rel.to_ascii_lowercase().ends_with(".kf") {
            continue;
        }
        let parts: Vec<&str> = rel.split('\\').collect();
        let dir_parts = &parts[..parts.len().saturating_sub(1)];
        if dir_parts.iter().any(|p| p.to_ascii_lowercase().ends_with(".mohidden")) {
            continue;
        }
        // Skip subfolders that belong to a separate JSON entry's `folder`.
        let mut prefix = folder_key.clone();
        let mut is_child_set = false;
        for seg in dir_parts {
            if !prefix.is_empty() {
                prefix.push('\\');
            }
            prefix.push_str(&seg.to_ascii_lowercase());
            if known_folders.contains(&prefix) {
                is_child_set = true;
                break;
            }
        }
        if is_child_set {
            continue;
        }
        let (view, mod_dirs) = match dir_parts.first().map(|p| p.to_ascii_lowercase()) {
            Some(v) if v == "_male" => (DirView::Third, &dir_parts[1..]),
            Some(v) if v == "_1stperson" => (DirView::First, &dir_parts[1..]),
            _ => (DirView::Direct, dir_parts),
        };
        // A nested `_male` or `_1stPerson` below the top directory indicates a child
        // set folder (e.g. `Flag\_1stperson\...`), never files belonging to `folder`.
        if mod_dirs.iter().any(|p| matches!(p.to_ascii_lowercase().as_str(), "_male" | "_1stperson")) {
            continue;
        }
        let mask = mod_dirs.iter().filter_map(|p| mod_folder_mask(p)).fold(0u8, |acc, m| acc | m);
        let dir_rel = dir_parts.join("\\");
        let dir_data_path = if dir_rel.is_empty() { base.clone() } else { format!("{base}\\{dir_rel}") };
        let path = format!("{ROOT}\\{folder}\\{rel}");
        let entry = dirs.entry((view, mask, dir_rel.to_ascii_lowercase())).or_insert_with(|| (dir_data_path, Vec::new()));
        entry.1.push(path);
    }

    let mut third = ViewAccum::default();
    let mut first = ViewAccum::default();
    for ((view, mask, _), (dir_data_path, files)) in dirs {
        let (unconditional, weighted) = partition_dir_files(&dir_data_path, files);
        if unconditional.is_empty() && weighted.is_empty() {
            continue;
        }
        match view {
            DirView::Direct => {
                third.push_dir(mask, unconditional.clone(), weighted.clone());
                first.push_dir(mask, unconditional, weighted);
            }
            DirView::Third => third.push_dir(mask, unconditional, weighted),
            DirView::First => first.push_dir(mask, unconditional, weighted),
        }
    }

    FolderScan {
        third: third.base_files,
        first: first.base_files,
        third_weighted: third.base_weighted,
        first_weighted: first.base_weighted,
        third_mods: third.mod_buckets.into_values().collect(),
        first_mods: first.mod_buckets.into_values().collect(),
    }
}

struct RawEntry {
    file: String,
    folder: String,
    forms: Vec<FormId>,
    condition: Option<String>,
    poll_condition: bool,
    priority: i64,
}

/// Every entry of the JSON files directly under the folder, the files read in name order
/// without regard to case [hypothesis: mods name files `1Another_Millenia_base.json` and
/// `zzz_heavy_fatman.json` to come first or last]. A file that is empty, not JSON, or an
/// entry without a folder, whose plugin is not loaded, that names a plugin but no form
/// [hypothesis: a slip, not an entry for every actor], or whose folder is missing is
/// logged and left out (the broken-files rule) [observed: 59 empty files, 20 entries
/// naming a folder that is not there].
fn load(g: &Game) -> Vec<FolderEntry> {
    let root = format!("meshes\\{ROOT}");
    let files: Vec<String> = crate::mod_files::files_in(&root).into_iter().filter(|f| f.to_ascii_lowercase().ends_with(".json")).collect();
    // Pass 1: parse JSON entries and collect every referenced `folder` path so child sets
    // nested inside a parent folder are never mistaken for direct-KF subfolders of the
    // parent [design].
    let mut raw_entries = Vec::new();
    let mut known_folders: HashSet<String> = HashSet::new();
    for file in &files {
        let Some(bytes) = crate::mod_files::read_loose(&format!("{root}\\{file}")) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}').trim();
        if text.is_empty() {
            continue;
        }
        let items = match serde_json::from_str::<Json>(text) {
            Ok(Json::Array(a)) => a,
            Ok(v @ Json::Object(_)) => vec![v],
            Ok(_) => {
                tracing::warn!("AnimGroupOverride: {file}: not a list of entries");
                continue;
            }
            Err(e) => {
                tracing::warn!("AnimGroupOverride: {file}: {e}");
                continue;
            }
        };
        for it in items {
            let Some(folder) = it
                .get("folder")
                .and_then(Json::as_str)
                .map(|s| s.replace('/', "\\").trim_matches('\\').to_string())
                .filter(|s| !s.is_empty())
            else {
                tracing::warn!("AnimGroupOverride: {file}: an entry without a folder");
                continue;
            };
            known_folders.insert(norm_folder(&folder));
            let ids = form_ids(it.get("form"));
            let forms = match it.get("mod").and_then(Json::as_str) {
                Some(m) => {
                    let Some(index) = g.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(m)) else {
                        tracing::debug!("AnimGroupOverride: {file}: {m} is not loaded");
                        continue;
                    };
                    if ids.is_empty() {
                        // Not every actor: a plugin was named, the form was left out.
                        tracing::warn!("AnimGroupOverride: {file}: an entry for {m} without a form");
                        continue;
                    }
                    ids.iter().map(|id| FormId(((index as u32) << 24) | (id & 0x00FF_FFFF))).collect()
                }
                None if ids.is_empty() => Vec::new(),
                // A form with no plugin: a full ID [hypothesis].
                None => ids.into_iter().map(FormId).collect(),
            };
            let condition = it.get("condition").and_then(Json::as_str).map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
            let poll_condition = it.get("pollCondition").and_then(Json::as_bool).unwrap_or(false) && condition.is_some();
            let priority = it.get("priority").and_then(Json::as_i64).unwrap_or(0);
            raw_entries.push(RawEntry { file: file.clone(), folder, forms, condition, poll_condition, priority });
        }
    }

    // Pass 2: walk each referenced folder once.
    let mut out = Vec::new();
    let mut walked: HashMap<String, FolderScan> = HashMap::new();
    for raw in raw_entries {
        let scan = walked.entry(norm_folder(&raw.folder)).or_insert_with(|| folder_files(&raw.folder, &known_folders)).clone();
        let entry = FolderEntry {
            forms: raw.forms,
            folder: raw.folder,
            condition: raw.condition,
            poll_condition: raw.poll_condition,
            priority: raw.priority,
            order: out.len(),
            third: scan.third,
            first: scan.first,
            third_weighted: scan.third_weighted,
            first_weighted: scan.first_weighted,
            third_mods: scan.third_mods,
            first_mods: scan.first_mods,
        };
        if !entry.has_any_files() {
            tracing::warn!("AnimGroupOverride: {}: folder {} has no animation files", raw.file, entry.folder);
            continue;
        }
        out.push(entry);
    }
    if !out.is_empty() {
        tracing::info!("AnimGroupOverride: {} entries from {} files", out.len(), files.len());
    }
    out
}

impl Game<'_> {
    /// The files an actor's animations are replaced with, in the order they apply (a
    /// later file of a group wins): the folders' first, then SetActorAnimationPath's
    /// [hypothesis: a script's call wins over the files a mod installed].
    ///
    /// A script's path with a condition is left out here (no script can run from a
    /// shared borrow): [`Game::anim_overrides_now`] runs the conditions. A folder path
    /// gives its KF files.
    pub fn anim_overrides(&self, actor: FormId, first: bool) -> Vec<String> {
        let mut v = self.anim_folders.active.get(&(actor, first)).cloned().unwrap_or_default();
        v.extend(self.weapon_overrides(actor, first));
        for p in self.anim_paths.actor.get(&(actor, first)).into_iter().flatten() {
            if !self.anim_paths.conditions.contains_key(&(actor, first, crate::nvse_anim::path_key(p))) {
                v.extend(crate::nvse_anim::kf_files(p));
            }
        }
        v
    }

    /// SetWeaponAnimationPath's files for the weapon the actor has equipped [documented:
    /// SetWeaponAnimationPath, "replaces an animation for a specified weapon base form";
    /// hypothesis: they apply to whoever holds it, before the actor's own paths, which
    /// win over them]. A folder path gives its KF files.
    fn weapon_overrides(&self, actor: FormId, first: bool) -> Vec<String> {
        let held = self.world.peek(actor).and_then(|s| s.inventory.as_ref()).and_then(|inv| {
            inv.equipped().into_iter().find(|&i| self.world.forms.item(i).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Weapon))
        });
        let Some(w) = held else { return Vec::new() };
        self.anim_paths.weapon.get(&(w, first)).into_iter().flatten().flat_map(|p| crate::nvse_anim::kf_files(p)).collect()
    }

    /// As [`Game::anim_overrides`], with SetActorAnimationPath's conditional paths whose
    /// condition holds now: each condition function runs on the actor (`this`) and a
    /// true result keeps its path [documented: SetActorAnimationPath, the condition
    /// function; hypothesis: evaluated each time the engine asks, every frame, which
    /// covers `pollConditionFunction` 1]. A condition that fails to run counts false.
    pub fn anim_overrides_now(&mut self, actor: FormId, first: bool) -> Vec<String> {
        let mut v = self.anim_folders.active.get(&(actor, first)).cloned().unwrap_or_default();
        v.extend(self.weapon_overrides(actor, first));
        let paths = self.anim_paths.actor.get(&(actor, first)).cloned().unwrap_or_default();
        for p in paths {
            let holds = match self.anim_paths.conditions.get(&(actor, first, crate::nvse_anim::path_key(&p))).cloned() {
                None => true,
                Some(c) => self.run_function(&c, Some(fnv_script::vm::Value::Ref(actor.0)), Vec::new(), Some(actor), Default::default()).is_ok_and(|r| r.truthy()),
            };
            if holds {
                v.extend(crate::nvse_anim::kf_files(&p));
            }
        }
        v
    }

    /// Read the folders once (when the Data folder is known), then every
    /// [`REFRESH_FRAMES`] evaluate all entries for the player and each actor the engine
    /// draws. On intermediate frames, evaluate strictly `pollCondition: true` entries
    /// (when any are loaded) while retaining cached non-polled matches from the last
    /// [`REFRESH_FRAMES`] refresh.
    pub(crate) fn anim_folders_tick(&mut self) {
        self.load_anim_folders();
        if self.anim_folders.entries.is_empty() {
            return;
        }
        if self.frame % REFRESH_FRAMES == 1 {
            self.refresh_anim_folders();
        } else if self.anim_folders.entries.iter().any(|e| e.poll_condition) {
            self.refresh_polled_anim_folders();
        }
    }

    /// Read the folders, once the Data folder is known.
    fn load_anim_folders(&mut self) {
        if !self.anim_folders.loaded && crate::mod_files::data_dir().is_some() {
            self.anim_folders.loaded = true;
            self.anim_folders.entries = load(self);
        }
    }

    fn folder_entry_holds(&mut self, e: &FolderEntry, a: FormId, base: Option<FormId>, weapon: Option<FormId>) -> bool {
        if !e.forms.is_empty() && !e.forms.iter().any(|&f| f == a || Some(f) == base || Some(f) == weapon) {
            return false;
        }
        if let Some(c) = &e.condition {
            match self.evaluate_on(c, Some(a)) {
                Ok(true) => {}
                Ok(false) => return false,
                Err(err) => {
                    if self.anim_folders.warned.insert(c.clone()) {
                        tracing::warn!("AnimGroupOverride: {}: condition {c:?}: {err}", e.folder);
                    }
                    return false;
                }
            }
        }
        true
    }

    /// Write an actor's active third- and first-person override lists into `active`.
    /// Matching `Mod1`–`Mod3` (and combined) buckets are appended after base files in
    /// specificity order; any `chances.txt` weighted pools draw from `AnimFolders`'s
    /// dedicated RNG (never `Game::rng`) and keep their pick stable per actor and pool
    /// until the equipped weapon, weapon-mod mask, or matching entry set changes [design].
    fn write_active_for(
        &mut self,
        a: FormId,
        weapon: Option<FormId>,
        weapon_mods: u8,
        mut hold: Vec<&FolderEntry>,
        active: &mut HashMap<(FormId, bool), Vec<String>>,
    ) {
        hold.sort_by_key(|e| (e.priority, e.order));
        let context = ActorPickContext {
            weapon,
            weapon_mods,
            entry_orders: hold.iter().map(|e| e.order).collect(),
        };
        let mut chosen = match self.anim_folders.picks.remove(&a) {
            Some(cached) if cached.context == context => cached.chosen,
            _ => HashMap::new(),
        };
        let mut has_weighted = false;
        for first in [false, true] {
            let mut files = Vec::new();
            for e in &hold {
                let (base_files, base_weighted, mod_buckets) = if first {
                    (&e.first, &e.first_weighted, &e.first_mods)
                } else {
                    (&e.third, &e.third_weighted, &e.third_mods)
                };
                files.extend(base_files.iter().cloned());
                for (pool_index, wg) in base_weighted.iter().enumerate() {
                    has_weighted = true;
                    let key = PoolKey { first, entry_order: e.order, mod_mask: 0, pool_index };
                    let pick = match chosen.get(&key) {
                        Some(existing) => Some(existing.clone()),
                        None => {
                            let u = rng_unit(&mut self.anim_folders.rng);
                            select_weighted(&wg.candidates, u).map(|p| {
                                let owned = p.to_string();
                                chosen.insert(key, owned.clone());
                                owned
                            })
                        }
                    };
                    if let Some(pick) = pick {
                        files.push(pick);
                    }
                }
                if weapon_mods != 0 {
                    for bucket in mod_buckets {
                        if bucket.mask != 0 && (weapon_mods & bucket.mask) == bucket.mask {
                            files.extend(bucket.files.iter().cloned());
                            for (pool_index, wg) in bucket.weighted.iter().enumerate() {
                                has_weighted = true;
                                let key = PoolKey { first, entry_order: e.order, mod_mask: bucket.mask, pool_index };
                                let pick = match chosen.get(&key) {
                                    Some(existing) => Some(existing.clone()),
                                    None => {
                                        let u = rng_unit(&mut self.anim_folders.rng);
                                        select_weighted(&wg.candidates, u).map(|p| {
                                            let owned = p.to_string();
                                            chosen.insert(key, owned.clone());
                                            owned
                                        })
                                    }
                                };
                                if let Some(pick) = pick {
                                    files.push(pick);
                                }
                            }
                        }
                    }
                }
            }
            if files.is_empty() {
                active.remove(&(a, first));
            } else {
                active.insert((a, first), files);
            }
        }
        if has_weighted {
            self.anim_folders.picks.insert(a, CachedActorPicks { context, chosen });
        }
    }

    /// Evaluate all `entries` for one actor `a`, caching its matching non-polled entry
    /// indices and writing its active third- and first-person override lists into `active`.
    fn eval_anim_folders_for(
        &mut self,
        entries: &[FolderEntry],
        a: FormId,
        cached_non_polled: &mut HashMap<FormId, Vec<usize>>,
        active: &mut HashMap<(FormId, bool), Vec<String>>,
    ) {
        let base = crate::functions::base_of(self, a);
        let weapon = crate::functions::items::equipped_weapon(self, a);
        let weapon_mods = weapon.map_or(0, |w| self.world.get(a).ok().and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.equipped_mods(w)));
        let mut non_polled_ids = Vec::new();
        let mut hold: Vec<&FolderEntry> = Vec::new();
        for (idx, e) in entries.iter().enumerate() {
            if !self.folder_entry_holds(e, a, base, weapon) {
                continue;
            }
            if !e.poll_condition {
                non_polled_ids.push(idx);
            }
            hold.push(e);
        }
        cached_non_polled.insert(a, non_polled_ids);
        self.write_active_for(a, weapon, weapon_mods, hold, active);
    }

    /// Evaluate strictly `pollCondition: true` entries for one actor `a`, combining them
    /// with the cached non-polled matches from the last full refresh without re-evaluating
    /// any non-polled entry.
    fn eval_polled_folders_for(
        &mut self,
        entries: &[FolderEntry],
        a: FormId,
        cached_non_polled: &HashMap<FormId, Vec<usize>>,
        active: &mut HashMap<(FormId, bool), Vec<String>>,
    ) {
        let base = crate::functions::base_of(self, a);
        let weapon = crate::functions::items::equipped_weapon(self, a);
        let weapon_mods = weapon.map_or(0, |w| self.world.get(a).ok().and_then(|s| s.inventory.as_ref()).map_or(0, |inv| inv.equipped_mods(w)));
        let mut hold: Vec<&FolderEntry> = cached_non_polled
            .get(&a)
            .into_iter()
            .flatten()
            .filter_map(|&idx| entries.get(idx))
            .filter(|e| !e.poll_condition)
            .collect();
        for e in entries.iter().filter(|e| e.poll_condition) {
            if self.folder_entry_holds(e, a, base, weapon) {
                hold.push(e);
            }
        }
        self.write_active_for(a, weapon, weapon_mods, hold, active);
    }

    /// The entries that hold for each actor now (full evaluation of all entries).
    pub fn refresh_anim_folders(&mut self) {
        let mut actors: Vec<FormId> = self.senses.anim.keys().copied().collect();
        if !actors.contains(&FormId::PLAYER_REF) {
            actors.push(FormId::PLAYER_REF);
        }
        let entries = std::mem::take(&mut self.anim_folders.entries);
        let mut cached_non_polled = HashMap::new();
        let mut active = HashMap::new();
        for &a in &actors {
            self.eval_anim_folders_for(&entries, a, &mut cached_non_polled, &mut active);
        }
        self.anim_folders.picks.retain(|a, _| actors.contains(a));
        self.anim_folders.entries = entries;
        self.anim_folders.cached_non_polled = cached_non_polled;
        self.anim_folders.active = active;
    }

    /// Re-evaluate strictly `pollCondition: true` entries for each drawn actor and the
    /// player, leaving non-polled entries on their cached [`REFRESH_FRAMES`] results.
    fn refresh_polled_anim_folders(&mut self) {
        let mut actors: Vec<FormId> = self.senses.anim.keys().copied().collect();
        if !actors.contains(&FormId::PLAYER_REF) {
            actors.push(FormId::PLAYER_REF);
        }
        let entries = std::mem::take(&mut self.anim_folders.entries);
        let cached_non_polled = std::mem::take(&mut self.anim_folders.cached_non_polled);
        let mut active = std::mem::take(&mut self.anim_folders.active);
        for a in actors {
            self.eval_polled_folders_for(&entries, a, &cached_non_polled, &mut active);
        }
        self.anim_folders.entries = entries;
        self.anim_folders.cached_non_polled = cached_non_polled;
        self.anim_folders.active = active;
    }

    /// Re-evaluate the `AnimGroupOverride` entries for one actor immediately (used by
    /// `RefreshAnimData` and `UpdatePlayer3D` so scripts do not wait for the periodic
    /// `REFRESH_FRAMES` poll) [design].
    pub fn refresh_anim_folders_for(&mut self, actor: FormId) {
        self.load_anim_folders();
        if self.anim_folders.entries.is_empty() {
            return;
        }
        let entries = std::mem::take(&mut self.anim_folders.entries);
        let mut cached_non_polled = std::mem::take(&mut self.anim_folders.cached_non_polled);
        let mut active = std::mem::take(&mut self.anim_folders.active);
        self.eval_anim_folders_for(&entries, actor, &mut cached_non_polled, &mut active);
        self.anim_folders.entries = entries;
        self.anim_folders.cached_non_polled = cached_non_polled;
        self.anim_folders.active = active;
    }

    /// CopyAnimationsToForm: the folders' entries for `from` are for `to` too, their
    /// conditions with them [documented: "including conditions and other attributes"].
    pub(crate) fn copy_folder_entries(&mut self, from: FormId, to: FormId) {
        self.load_anim_folders();
        for e in &mut self.anim_folders.entries {
            if e.forms.contains(&from) && !e.forms.contains(&to) {
                e.forms.push(to);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_folder_mask_parses_single_and_combined_slots() {
        assert_eq!(mod_folder_mask("Mod1"), Some(0b001));
        assert_eq!(mod_folder_mask("mod2"), Some(0b010));
        assert_eq!(mod_folder_mask("MOD3"), Some(0b100));
        assert_eq!(mod_folder_mask("Mod12"), Some(0b011));
        assert_eq!(mod_folder_mask("Mod13"), Some(0b101));
        assert_eq!(mod_folder_mask("Mod23"), Some(0b110));
        assert_eq!(mod_folder_mask("Mod123"), Some(0b111));
        assert_eq!(mod_folder_mask("Mod31"), Some(0b101));

        assert_eq!(mod_folder_mask("Mod"), None);
        assert_eq!(mod_folder_mask("Mod0"), None);
        assert_eq!(mod_folder_mask("Mod4"), None);
        assert_eq!(mod_folder_mask("Mod11"), None);
        assert_eq!(mod_folder_mask("Mod1234"), None);
        assert_eq!(mod_folder_mask("Models"), None);
        assert_eq!(mod_folder_mask("_1stPerson"), None);
    }

    #[test]
    fn parse_chances_handles_formats_comments_and_fail_closed_validation() {
        let text = "\u{feff}
            ; semicolon comment
            # hash comment
            // slash comment
            2haJamF.kf = 50 ; inline comment
            2haJamF_a.kf: 25.5% # inline hash
            2haJamF_b.kf 0 // explicit zero
            \"1hpReloadL v2.14.kf\" = 12.5
            bare_stem = 10
            dup.kf = 5
            dup.kf = 15
            dup.kf = not_a_number
            neg.kf = -10
            neg_zero.kf = -0
            nan.kf = NaN
            inf.kf = inf
            ninf.kf = -inf
            double_pct.kf = 50%%
            = 50
            only_key =
            garbage line without numeric tail
        ";
        let map = parse_chances(text);
        assert_eq!(map.get("2hajamf.kf"), Some(&50.0));
        assert_eq!(map.get("2hajamf_a.kf"), Some(&25.5));
        assert_eq!(map.get("2hajamf_b.kf"), Some(&0.0));
        assert_eq!(map.get("1hpreloadl v2.14.kf"), Some(&12.5));
        assert_eq!(map.get("bare_stem"), Some(&10.0));
        // Valid earlier entry preserved when a later line for the same key is malformed.
        assert_eq!(map.get("dup.kf"), Some(&15.0));
        for bad in ["neg.kf", "neg_zero.kf", "nan.kf", "inf.kf", "ninf.kf", "double_pct.kf", "only_key"] {
            assert!(!map.contains_key(bad), "{bad} must be rejected fail-closed");
        }
    }

    #[test]
    fn select_weighted_matches_exact_distribution_and_excludes_zero() {
        let candidates = vec![
            ("a.kf".to_string(), 75.0),
            ("b.kf".to_string(), 25.0),
            ("zero.kf".to_string(), 0.0),
            ("neg.kf".to_string(), -5.0),
            ("nan.kf".to_string(), f64::NAN),
        ];
        assert_eq!(select_weighted(&candidates, 0.0), Some("a.kf"));
        assert_eq!(select_weighted(&candidates, 0.749_999), Some("a.kf"));
        assert_eq!(select_weighted(&candidates, 0.75), Some("b.kf"));
        assert_eq!(select_weighted(&candidates, 0.999_999), Some("b.kf"));
        assert_eq!(select_weighted(&candidates, f64::NAN), None);
        assert_eq!(select_weighted(&candidates, f64::INFINITY), None);

        // Deterministic Monte Carlo check over 20,000 draws.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let n = 20_000usize;
        for _ in 0..n {
            let pick = select_weighted(&candidates, rng_unit(&mut rng)).unwrap();
            *counts.entry(pick).or_insert(0) += 1;
        }
        assert_eq!(counts.get("zero.kf").copied().unwrap_or(0), 0);
        assert_eq!(counts.get("neg.kf").copied().unwrap_or(0), 0);
        assert_eq!(counts.get("nan.kf").copied().unwrap_or(0), 0);
        let a_share = counts["a.kf"] as f64 / n as f64;
        let b_share = counts["b.kf"] as f64 / n as f64;
        assert!((a_share - 0.75).abs() < 0.015, "a_share={a_share}");
        assert!((b_share - 0.25).abs() < 0.015, "b_share={b_share}");

        // Overflowing or all-zero totals fail closed.
        assert_eq!(select_weighted(&[("z.kf".into(), 0.0)], 0.5), None);
        assert_eq!(select_weighted(&[("x.kf".into(), f64::MAX), ("y.kf".into(), f64::MAX)], 0.5), None);
    }

    #[test]
    fn build_weighted_groups_splits_only_distinct_multi_file_families() {
        let single_pool = build_weighted_groups(vec![
            ("dir\\fast.kf".into(), 70.0),
            ("dir\\slow.kf".into(), 30.0),
            ("dir\\never.kf".into(), 0.0),
        ]);
        assert_eq!(single_pool.len(), 1);
        assert_eq!(single_pool[0].candidates, vec![("dir\\fast.kf".into(), 70.0), ("dir\\slow.kf".into(), 30.0)]);

        let two_families = build_weighted_groups(vec![
            ("dir\\2hrAttack3.kf".into(), 80.0),
            ("dir\\2hrAttack3_a.kf".into(), 20.0),
            ("dir\\2hrReloadA.kf".into(), 60.0),
            ("dir\\2hrReloadA_a.kf".into(), 40.0),
        ]);
        assert_eq!(two_families.len(), 2);
        assert_eq!(two_families[0].candidates.len(), 2);
        assert_eq!(two_families[1].candidates.len(), 2);
    }
}
