//! Where animation files sit relative to skeletons (docs/spec-notes/animation.md §3):
//! every `.kf` belongs to the folder of the nearest skeleton above it, and its tracks are
//! checked against that skeleton's node names. Paths are VFS paths (`meshes\…`, lower
//! case, backslashes; [`normalize`]).

use std::collections::{BTreeMap, BTreeSet};

use fnv_esm::{LoadOrder, Schema};
use fnv_sim::extract::{self, first, text, Issues};
use fnv_sim::FormId;
use serde::Serialize;
use serde_json::Value;

/// How a `.kf` file's skeleton was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkeletonRule {
    /// A folder above the file (or its own) holds a skeleton [observed: the layout of
    /// `characters\_male`, `characters\_1stperson` and the creature folders].
    Ancestor,
    /// No skeleton above it, but its actor folder ends like a folder that has one
    /// (`meshes\update\characters\_male` → `meshes\characters\_male`) [hypothesis].
    SameTail,
    /// Neither.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkeletonMatch {
    /// The group the file is catalogued under: the skeleton's folder, or (rule `None`)
    /// the file's actor folder.
    pub folder: String,
    pub skeleton: Option<String>,
    pub rule: SkeletonRule,
}

/// Skeleton files (`skeleton*.nif`) by folder.
#[derive(Debug, Default, Clone)]
pub struct SkeletonIndex {
    by_folder: BTreeMap<String, Vec<String>>,
}

/// A game path in VFS form: lower case, backslashes, no leading separator (the same
/// rule as `fnv_vfs::normalize`, kept here so this crate does not depend on the VFS).
pub fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase().trim_start_matches('\\').to_string()
}

pub fn parent(path: &str) -> Option<&str> {
    path.rfind('\\').map(|i| &path[..i])
}

pub fn file_name(path: &str) -> &str {
    path.rfind('\\').map_or(path, |i| &path[i + 1..])
}

/// The actor folder of a path: everything up to the segment after `characters` or
/// `creatures` (`meshes\dlc04\creatures\hillfolk1\idleanims\x.kf` →
/// `meshes\dlc04\creatures\hillfolk1`); else the file's own folder.
pub fn actor_folder(path: &str) -> String {
    let segs: Vec<&str> = path.split('\\').collect();
    let dir = &segs[..segs.len().saturating_sub(1)];
    match dir.iter().position(|s| *s == "characters" || *s == "creatures") {
        Some(i) if i + 1 < dir.len() => dir[..=i + 1].join("\\"),
        _ => dir.join("\\"),
    }
}

/// The part of an actor folder from `characters`/`creatures` on (`characters\_male`).
fn tail(folder: &str) -> Option<&str> {
    let i = folder.find("characters\\").or_else(|| folder.find("creatures\\"))?;
    Some(&folder[i..])
}

impl SkeletonIndex {
    /// From every path in the VFS; keeps files named `skeleton*.nif` outside
    /// `meshes\clutter\` (clutter skeletons are props [observed: `clutter\skeleton\`
    /// holds bone piles]).
    pub fn from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> SkeletonIndex {
        let mut ix = SkeletonIndex::default();
        for p in paths {
            let name = file_name(p);
            if !(name.starts_with("skeleton") && name.ends_with(".nif")) || p.starts_with("meshes\\clutter\\") {
                continue;
            }
            if let Some(dir) = parent(p) {
                ix.by_folder.entry(dir.to_string()).or_default().push(name.to_string());
            }
        }
        for v in ix.by_folder.values_mut() {
            v.sort();
        }
        ix
    }

    /// The skeleton of a folder: `skeleton.nif` if present, else the first other
    /// `skeleton*.nif` by name.
    pub fn skeleton_in(&self, folder: &str) -> Option<String> {
        let names = self.by_folder.get(folder)?;
        let pick = names.iter().find(|n| *n == "skeleton.nif").or_else(|| names.first())?;
        Some(format!("{folder}\\{pick}"))
    }

    /// Every skeleton file of a folder (several in e.g. `creatures\dog`).
    pub fn skeletons_in(&self, folder: &str) -> &[String] {
        self.by_folder.get(folder).map_or(&[], Vec::as_slice)
    }

    pub fn folders(&self) -> impl Iterator<Item = &str> {
        self.by_folder.keys().map(String::as_str)
    }

    pub fn skeleton_for(&self, path: &str) -> SkeletonMatch {
        let mut cur = parent(path);
        while let Some(dir) = cur {
            if let Some(s) = self.skeleton_in(dir) {
                return SkeletonMatch { folder: dir.to_string(), skeleton: Some(s), rule: SkeletonRule::Ancestor };
            }
            cur = parent(dir);
        }
        let actor = actor_folder(path);
        if let Some(t) = tail(&actor) {
            let same: Vec<&str> = self.folders().filter(|f| tail(f) == Some(t)).collect();
            if let [only] = same.as_slice() {
                return SkeletonMatch { folder: only.to_string(), skeleton: self.skeleton_in(only), rule: SkeletonRule::SameTail };
            }
        }
        SkeletonMatch { folder: actor, skeleton: None, rule: SkeletonRule::None }
    }
}

/// The subfolder of a `.kf` below its group folder (`idleanims`, `locomotion\hurt`,
/// or empty for files directly in it).
pub fn subfolder(group: &str, path: &str) -> String {
    let dir = parent(path).unwrap_or_default();
    dir.strip_prefix(group).map(|s| s.trim_start_matches('\\').to_string()).unwrap_or_else(|| dir.to_string())
}

/// Tracks of a sequence against a skeleton's node names.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Binding {
    pub tracks: usize,
    /// Tracks whose target node name is a node of the skeleton (exact match).
    pub bound: usize,
    /// Target names not in the skeleton (None: the track has no name).
    pub unbound: BTreeMap<String, usize>,
}

pub fn bind<'a>(targets: impl IntoIterator<Item = Option<&'a str>>, skeleton_nodes: &BTreeSet<String>) -> Binding {
    let mut b = Binding::default();
    for t in targets {
        b.tracks += 1;
        match t {
            Some(n) if skeleton_nodes.contains(n) => b.bound += 1,
            Some(n) => *b.unbound.entry(n.to_string()).or_default() += 1,
            None => *b.unbound.entry("(no name)".to_string()).or_default() += 1,
        }
    }
    b
}

/// Record header flag "Random Anim Start" (bit 16) and the record types that define it
/// [documented: xEdit wbDefinitionsFNV.pas record flag lists of ACTI, CONT, DOOR, FURN,
/// LIGH, MSTT, TACT and TERM; the GECK wiki "World Objects" page: such an object starts
/// its idle animation at a random time so copies do not animate in step].
pub const RANDOM_ANIM_START: u32 = 1 << 16;
pub const RANDOM_ANIM_START_TYPES: [&[u8; 4]; 8] = [b"ACTI", b"CONT", b"DOOR", b"FURN", b"LIGH", b"MSTT", b"TACT", b"TERM"];

/// Whether a base record of type `sig` with header flags `flags` has "Random Anim
/// Start" (on other types bit 16 means something else, e.g. "High Priority LOD" on
/// REFR).
pub fn random_anim_start(sig: &[u8; 4], flags: u32) -> bool {
    flags & RANDOM_ANIM_START != 0 && RANDOM_ANIM_START_TYPES.contains(&sig)
}

/// Where a creature's KFFZ entry (a bare file name) is looked for, in order:
/// `<skeleton folder>\specialanims\<name>`, then `<skeleton folder>\<name>`
/// [observed: Mr Handy, Protectron and Blowfly names are in `specialanims\`].
pub fn kffz_candidates(skeleton: &str, name: &str) -> Vec<String> {
    let folder = parent(skeleton).unwrap_or_default();
    let name = normalize(name);
    vec![format!("{folder}\\specialanims\\{name}"), format!("{folder}\\{name}")]
}

/// Where a creature clip at `path` (`<folder>\<name>.kf`) is found when its folder has
/// none: `<folder>\specialanims\<name>.kf`. Only for creature skeleton folders (a path
/// with a `creatures` folder in it): the stance clips of Mister Handy and Gutsy
/// (`1hmaim.kf`, `1hpattackright.kf`, `2hlattackspin.kf` …), the protectron's and the
/// bloatfly's are there and not at the folder's root, and their CREA KFFZ lists name them
/// [observed: `Fallout - Meshes.bsa` listing and FalloutNV.esm CrMisterHandy KFFZ,
/// 2026-10-24]; no other placed creature's folder keeps stance clips there.
pub fn special_anims_path(path: &str) -> Option<String> {
    let p = normalize(path);
    if !p.contains("creatures\\") || p.contains("\\specialanims\\") {
        return None;
    }
    let (folder, name) = p.rsplit_once('\\')?;
    Some(format!("{folder}\\specialanims\\{name}"))
}

/// The animation-related lists of an actor base: `MODL` (skeleton), KFFZ (extra `.kf`
/// names) and, for CREA, NIFZ (body part models) [documented: xEdit; layouts in
/// `schemas/fnv/gameplay.json`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActorAnimLists {
    pub id: FormId,
    /// `NPC_` or `CREA`.
    pub sig: String,
    pub edid: Option<String>,
    /// `MODL` as a VFS path.
    pub skeleton: Option<String>,
    pub kffz: Vec<String>,
    pub nifz: Vec<String>,
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(text).collect()).unwrap_or_default()
}

/// Every winning, non-deleted NPC_ and CREA with its lists.
pub fn actor_anim_lists(lo: &LoadOrder, schema: &Schema, issues: &mut Issues) -> Vec<ActorAnimLists> {
    let mut out = Vec::new();
    for (&g, defs) in &lo.index {
        let Some(d) = defs.last() else { continue };
        let r = lo.records[d.plugin as usize][d.ordinal as usize];
        if !(&r.header.sig == b"NPC_" || &r.header.sig == b"CREA") || r.header.flags & 0x20 != 0 {
            continue;
        }
        let d = match extract::decode(r, schema) {
            Ok(d) => d,
            Err(e) => {
                issues.note("actor base does not decode", || format!("{}: {e}", FormId(g)));
                continue;
            }
        };
        out.push(ActorAnimLists {
            id: FormId(g),
            sig: String::from_utf8_lossy(&r.header.sig).into_owned(),
            edid: first(&d, b"EDID").and_then(text),
            skeleton: first(&d, b"MODL").and_then(text).map(|m| crate::idle::meshes_path(m.trim())).filter(|m| m != "meshes\\"),
            kffz: strings(first(&d, b"KFFZ")),
            nifz: strings(first(&d, b"NIFZ")),
        });
    }
    out.sort_by_key(|a| a.id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_clips_fall_back_to_their_special_anims_folder() {
        assert_eq!(special_anims_path(r"Meshes\Creatures\MisterGutsy\1hmAim.kf").as_deref(), Some(r"meshes\creatures\mistergutsy\specialanims\1hmaim.kf"));
        assert_eq!(special_anims_path(r"meshes\creatures\mistergutsy\specialanims\1hmaim.kf"), None);
        assert_eq!(special_anims_path(r"meshes\characters\_male\1hmaim.kf"), None);
    }

    #[test]
    fn random_anim_start_only_on_its_record_types() {
        assert!(random_anim_start(b"ACTI", 0x0001_0000));
        assert!(random_anim_start(b"MSTT", 0x0001_0020));
        assert!(!random_anim_start(b"ACTI", 0x0000_0400));
        assert!(!random_anim_start(b"STAT", 0x0001_0000));
        assert!(!random_anim_start(b"REFR", 0x0001_0000));
    }

    fn index() -> SkeletonIndex {
        SkeletonIndex::from_paths([
            "meshes\\characters\\_male\\skeleton.nif",
            "meshes\\characters\\_1stperson\\skeleton.nif",
            "meshes\\creatures\\dog\\skeleton.nif",
            "meshes\\creatures\\dog\\skeleton_sonicbark.nif",
            "meshes\\nvdlc03\\creatures\\roboscorpion\\skeleton_roboscopion.nif",
            "meshes\\clutter\\skeleton\\skeletontorsonaked.nif",
            "meshes\\characters\\_male\\upperbody.nif",
        ])
    }

    #[test]
    fn skeleton_by_ancestor_and_preference() {
        let ix = index();
        let m = ix.skeleton_for("meshes\\characters\\_male\\idleanims\\specialidle_shrug.kf");
        assert_eq!(m.folder, "meshes\\characters\\_male");
        assert_eq!(m.skeleton.as_deref(), Some("meshes\\characters\\_male\\skeleton.nif"));
        assert_eq!(m.rule, SkeletonRule::Ancestor);
        assert_eq!(ix.skeleton_in("meshes\\creatures\\dog").as_deref(), Some("meshes\\creatures\\dog\\skeleton.nif"));
        assert_eq!(ix.skeletons_in("meshes\\creatures\\dog").len(), 2);
        let m = ix.skeleton_for("meshes\\nvdlc03\\creatures\\roboscorpion\\locomotion\\hurt\\x.kf");
        assert_eq!(m.skeleton.as_deref(), Some("meshes\\nvdlc03\\creatures\\roboscorpion\\skeleton_roboscopion.nif"));
        assert!(ix.skeletons_in("meshes\\clutter\\skeleton").is_empty());
    }

    #[test]
    fn skeleton_by_tail_or_none() {
        let ix = index();
        let m = ix.skeleton_for("meshes\\update\\characters\\_male\\idleanims\\x.kf");
        assert_eq!((m.folder.as_str(), m.rule), ("meshes\\characters\\_male", SkeletonRule::SameTail));
        let m = ix.skeleton_for("meshes\\dlc04\\creatures\\hillfolk1\\idleanims\\x.kf");
        assert_eq!((m.folder.as_str(), m.skeleton, m.rule), ("meshes\\dlc04\\creatures\\hillfolk1", None, SkeletonRule::None));
    }

    #[test]
    fn folders_and_names() {
        assert_eq!(actor_folder("meshes\\characters\\_male\\locomotion\\mtidle.kf"), "meshes\\characters\\_male");
        assert_eq!(actor_folder("meshes\\misc\\x.kf"), "meshes\\misc");
        assert_eq!(subfolder("meshes\\characters\\_male", "meshes\\characters\\_male\\locomotion\\hurt\\a.kf"), "locomotion\\hurt");
        assert_eq!(subfolder("meshes\\characters\\_male", "meshes\\characters\\_male\\a.kf"), "");
        assert_eq!(file_name("meshes\\a\\b.kf"), "b.kf");
        assert_eq!(normalize("/Meshes/Creatures\\Dog/Skeleton.NIF"), "meshes\\creatures\\dog\\skeleton.nif");
        assert_eq!(
            kffz_candidates("meshes\\creatures\\mistergutsy\\skeleton.nif", "1HMAim.kf"),
            ["meshes\\creatures\\mistergutsy\\specialanims\\1hmaim.kf", "meshes\\creatures\\mistergutsy\\1hmaim.kf"]
        );
    }

    #[test]
    fn binding_counts() {
        let nodes: BTreeSet<String> = ["Bip01", "Bip01 Head"].iter().map(|s| s.to_string()).collect();
        let b = bind([Some("Bip01"), Some("Bip01 Head"), Some("Weapon"), None, Some("Weapon")], &nodes);
        assert_eq!((b.tracks, b.bound), (5, 2));
        assert_eq!(b.unbound.get("Weapon"), Some(&2));
        assert_eq!(b.unbound.get("(no name)"), Some(&1));
    }
}
