//! How a part mesh joins the actor's skeleton (docs/spec-notes/actors.md §1.2), read
//! from the decoded NIF:
//! - **skinned**: shapes with a skin instance; their bones are nodes copied into the
//!   part's own file and are matched to the skeleton by name;
//! - **rigid**: unskinned; the root's `NiStringExtraData` named `Prn` names the
//!   skeleton bone the part is attached to [observed: hair, beards, eyes]. Rigid parts
//!   without `Prn` attach to `Bip01 Head` [hypothesis: every such part used by a placed
//!   NPC is headgear or a mouth part, with vertices around the head bone].
//!
//! A rigid part's vertices are in the skeleton's axes with the origin at the bone
//! [observed: hair spans z −0.7…15.3 above `Bip01 Head`, whose own x axis points up];
//! attached to the bone it therefore needs the inverse of the bone's bind rotation.

use std::collections::BTreeSet;

use fnv_nif::file::NifFile;
use fnv_nif::scene::{Decoded, SHAPE_TYPES};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartMesh {
    /// Names of the bones skinned shapes bind to.
    pub skin_bones: BTreeSet<String>,
    /// Shapes with and without a skin instance.
    pub skinned_shapes: usize,
    pub rigid_shapes: usize,
    /// `Prn` string extra data on the root node: the parent bone.
    pub prn: Option<String>,
    /// Shape names (e.g. `NoHat`/`Hat` in hair, dismemberment caps in bodies).
    pub shape_names: Vec<String>,
    /// Every node name in the file (for a skeleton: its bones).
    pub node_names: BTreeSet<String>,
    /// For each unskinned shape, the names of its ancestor nodes from its parent up to,
    /// but not including, the root.
    pub rigid_ancestors: Vec<Vec<String>>,
    /// The root node's name.
    pub root_name: Option<String>,
}

/// Block index of the root: the first block, as the files are laid out [observed:
/// every part file inspected has its root node at block 0].
const ROOT: usize = 0;

pub fn analyse(file: &NifFile, dec: &Decoded) -> PartMesh {
    let mut out = PartMesh::default();
    for (i, b) in file.blocks.iter().enumerate() {
        let Ok(s) = &dec.blocks[i] else { continue };
        if dec.is_a(i, "NiNode") {
            if let Some(n) = s.text("name") {
                out.node_names.insert(n.to_string());
            }
        }
        if !SHAPE_TYPES.contains(&b.type_name.as_str()) {
            continue;
        }
        if let Some(n) = s.text("name") {
            out.shape_names.push(n.to_string());
        }
        let skin = s.block_ref("skin_instance").and_then(|r| dec.get(r));
        match skin {
            Some(skin) => {
                out.skinned_shapes += 1;
                for &bone in skin.refs("bones").unwrap_or_default() {
                    if let Some(n) = dec.get(bone).and_then(|b| b.text("name")) {
                        out.skin_bones.insert(n.to_string());
                    }
                }
            }
            None => {
                out.rigid_shapes += 1;
                out.rigid_ancestors.push(ancestors(file, dec, i));
            }
        }
    }
    out.prn = root_prn(dec);
    out.root_name = dec.blocks.get(ROOT).and_then(|b| b.as_ref().ok()).and_then(|b| b.text("name")).map(str::to_string);
    out
}

/// Names of the nodes above block `i`, from its parent up to (not including) the root.
fn ancestors(file: &NifFile, dec: &Decoded, i: usize) -> Vec<String> {
    let parent = |c: usize| {
        (0..file.blocks.len()).find(|&p| {
            dec.is_a(p, "NiNode") && dec.blocks[p].as_ref().ok().is_some_and(|s| s.refs("children").unwrap_or_default().contains(&(c as i32)))
        })
    };
    let mut out = Vec::new();
    let mut cur = parent(i);
    while let Some(p) = cur.filter(|&p| p != ROOT && out.len() < 256) {
        out.push(dec.blocks[p].as_ref().ok().and_then(|s| s.text("name")).unwrap_or("").to_string());
        cur = parent(p);
    }
    out
}

/// The skeleton bone an unskinned shape of a creature part hangs from (actors.md §10.2):
/// its nearest ancestor in the part file whose name is a skeleton node; else the part's
/// `Prn` when the skeleton has it; else `None` (the skeleton's root).
pub fn creature_rigid_bone<'a>(ancestors: &'a [String], prn: Option<&'a str>, has: impl Fn(&str) -> bool) -> Option<&'a str> {
    ancestors.iter().map(String::as_str).find(|n| has(n)).or(prn.filter(|p| has(p)))
}

/// What is wrong with attaching a creature's `part` to its `skeleton` (actors.md §10.2):
/// skin bones the skeleton lacks. Unskinned shapes that find no bone hang from the root
/// by rule; they are counted by [`creature_rigid_at_root`].
pub fn creature_attach_problems(part: &PartMesh, skeleton: &PartMesh) -> Vec<String> {
    part.skin_bones.iter().filter(|b| !skeleton.node_names.contains(*b)).map(|b| format!("skin bone not in skeleton: {b}")).collect()
}

/// Unskinned shapes of a creature part that hang from the skeleton's root (no ancestor
/// and no `Prn` names a skeleton node).
pub fn creature_rigid_at_root(part: &PartMesh, skeleton: &PartMesh) -> usize {
    part.rigid_ancestors
        .iter()
        .filter(|a| creature_rigid_bone(a, part.prn.as_deref(), |n| skeleton.node_names.contains(n)).is_none())
        .count()
}

fn root_prn(dec: &Decoded) -> Option<String> {
    let root = dec.blocks.get(ROOT)?.as_ref().ok()?;
    for &e in root.refs("extra_data").unwrap_or_default() {
        let Some(x) = dec.get(e) else { continue };
        if x.text("name").is_some_and(|n| n.eq_ignore_ascii_case("Prn")) {
            return x.text("string_data").map(str::to_string);
        }
    }
    None
}

/// The bone a rigid part attaches to: its `Prn`, else [`DEFAULT_RIGID_BONE`].
pub const DEFAULT_RIGID_BONE: &str = "Bip01 Head";

impl PartMesh {
    pub fn attach_bone(&self) -> &str {
        self.prn.as_deref().unwrap_or(DEFAULT_RIGID_BONE)
    }
}

/// What is wrong with attaching `part` to `skeleton`: skin bones the skeleton lacks,
/// or (for a part with unskinned shapes) an attachment bone it lacks.
pub fn attach_problems(part: &PartMesh, skeleton: &PartMesh) -> Vec<String> {
    let mut p: Vec<String> =
        part.skin_bones.iter().filter(|b| !skeleton.node_names.contains(*b)).map(|b| format!("skin bone not in skeleton: {b}")).collect();
    if part.rigid_shapes > 0 && !skeleton.node_names.contains(part.attach_bone()) {
        p.push(format!("attach bone not in skeleton: {}", part.attach_bone()));
    }
    p
}
