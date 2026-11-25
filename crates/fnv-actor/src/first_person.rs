//! What the player's first-person view draws (docs/design/player-body.md §1.1): the
//! game's separate first-person rig `characters\_1stperson\skeleton.nif` with arms and
//! hands only. Headless rules on a resolved [`Appearance`].
//!
//! - Parts: the upper body and the hands (race parts and worn armour covering Upper Body,
//!   Left Hand or Right Hand) and the Pip-Boy (worn armour covering the PipBoy slot, on
//!   the forearm of the arms rig); no head, hair, head parts or other armour [hypothesis].
//! - Hands: a `…1st.nif` beside the hand model replaces it when it exists
//!   (`characters\_male\lefthand1st.nif`, `righthand1st.nif`,
//!   `lefthandpipboyglove1st.nif` and the female ones [observed: file list]).
//! - Upper body: only shapes whose name begins with `Arms` are drawn [owner statement
//!   2026-10-09: "if the nif is not called arms it's not rendered"; observed: outfits name
//!   their sleeves `Arms`, `Arms02`, `Arms2`, `Arms01:0`, `Arms\t` beside `UpperBody`,
//!   caps and `PipBoyOn`/`PipBoyOff`; the race's `upperbody.nif` names its skin `Arms:0`
//!   and `Arms:1`]. Hand parts are drawn whole.
//! - With a Pip-Boy on, the outfit's `PipBoyOn…` sleeve is drawn too [owner statement
//!   2026-10-10, with two in-game screenshots: "we see a sleeve on the arm with a
//!   pipboy, it is rendered over the outfit using the pipboy on node"]. Without one the
//!   `PipBoyOff…` sleeve is drawn in its place [hypothesis: it is the same stretch of the
//!   left forearm; the original's player always wears a Pip-Boy. #2173: left on the
//!   third-person body, it floated in front of the view while that body's arms were
//!   hidden]. Which of the two an actor wears is decided where the parts are assembled
//!   ([`crate::mesh::hidden_for_pipboy`]), which leaves the other out.
//! - An upper-body model with no `Arms…` shape keeps its sleeves in shapes named for
//!   other things, beside the torso (the NCR Trooper Fatigues' `hest:0`, `objdefault01:0`,
//!   `objdefault02:0`, #2457). Of such a model the arm partitions of the skins are drawn
//!   ([`is_arm_body_part`], [`arms_by_partition`]) [observed: their partitions' bones and
//!   extents, docs/spec-notes/actors.md §1.2; hypothesis: the original draws those
//!   sleeves in first person, as it draws a jumpsuit's].
//! - An upper-body model with neither `Arms…` shapes nor arm partitions (such as a custom
//!   single-shape mod outfit without dismemberment partitions, #3227) falls back to
//!   selecting the triangles of its non-cap, non-sleeve skinned shapes weighted primarily
//!   to arm and hand bones ([`is_arm_bone`], [`partition_arm_triangles`]) [design, #3227].
//!   The third-person body (`--first-person-body`) splits the same arm partitions or
//!   arm-weighted triangles out of mixed shapes so the body's own arms hide while the
//!   first-person arms rig is up without hiding the torso [design, #3227].
//! - A held weapon's own addons (WEAP BIPL, the hand-to-hand weapons' hand models) are
//!   worn by the arms only when the weapon has no model of its own for the first-person
//!   view ([`wears_weapon_addons`], #3226): the Bladed Gauntlet, the knuckles and the
//!   other fist weapons with no MODL are drawn by their addon alone; a weapon with a
//!   MODL or a first-person model (the power fist's rigid gauntlet) is drawn by that
//!   model over the arms' bare hand, as before.

use crate::records::Weapon;
use crate::resolve::{slot, Appearance, Part};
use crate::ActorForms;
use fnv_sim::FormId;

/// The first-person skeleton [observed: the only non-`.kf` file in
/// `meshes\characters\_1stperson`].
pub const SKELETON: &str = r"meshes\characters\_1stperson\skeleton.nif";

/// How a part appears in first person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// Every shape.
    Whole,
    /// Only the shapes [`is_first_person_shape`] accepts.
    Arms,
}

/// The first-person variant of a hand model: `x.nif` → `x1st.nif`.
pub fn first_person_variant(model: &str) -> Option<String> {
    let stem = model.strip_suffix(".nif").or_else(|| model.strip_suffix(".NIF"))?;
    Some(format!("{stem}1st.nif"))
}

/// Whether a shape of an upper-body model is drawn in first person.
pub fn is_arms_shape(name: &str) -> bool {
    name.trim().to_ascii_lowercase().starts_with("arms")
}

/// Whether a shape of an upper-body model is drawn in first person: the `Arms…` shapes
/// and the left forearm sleeves (`PipBoyOn…` worn with a Pip-Boy, `PipBoyOff…` without;
/// [`crate::mesh::hidden_for_pipboy`] leaves out the one not worn).
pub fn is_first_person_shape(name: &str) -> bool {
    is_arms_shape(name) || crate::mesh::pipboy_sleeve(name).is_some()
}

/// Whether a skin partition of body part `bp` (nif.xml `BSDismemberBodyPartType`,
/// documentation only; docs/spec-notes/dismemberment.md §2.5) is of an arm: BP_LEFTARM,
/// BP_LEFTARM2, BP_RIGHTARM, BP_RIGHTARM2 (3–6) and their torso sections (part × 1000)
/// [documented: the type's names; observed: in the NCR Trooper Fatigues those partitions
/// are skinned to the arm bones, the left one's 3000 from the shoulder down the upper arm
/// and 3 to the elbow, actors.md §1.2].
pub fn is_arm_body_part(bp: u16) -> bool {
    matches!(bp, 3..=6 | 3000 | 4000 | 5000 | 6000)
}

/// Whether an upper-body model whose shapes are named `names` is drawn in first person
/// by its arm partitions rather than by its shapes' names: it has no `Arms…` shape.
pub fn arms_by_partition<'a>(mut names: impl Iterator<Item = &'a str>) -> bool {
    !names.any(is_arms_shape)
}

/// Whether a shape named `name` with partitions of body parts `parts` is drawn, with its
/// arm partitions only, in first person by a model drawn [`arms_by_partition`]: a shape
/// that has one and is no forearm sleeve (those go by their names).
pub fn partition_arm_shape(name: &str, parts: impl IntoIterator<Item = u16>) -> bool {
    crate::mesh::pipboy_sleeve(name).is_none() && parts.into_iter().any(is_arm_body_part)
}

/// How an upper-body model's first-person arms were found [design, #3227].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmsSource {
    /// Shapes named `Arms…` (beside the worn `PipBoyOn…` / `PipBoyOff…` sleeve).
    ByName,
    /// Arm dismemberment partitions (`is_arm_body_part`) of non-cap, non-sleeve shapes (#2457).
    ByPartition,
    /// Triangles of non-cap, non-sleeve skinned shapes weighted primarily to arm/hand bones (#3227).
    ByWeights,
    /// Neither `Arms…` shapes, arm partitions, nor arm-weighted skinned triangles were found.
    None,
}

impl ArmsSource {
    /// Short label for diagnostics and logs (`"by name"`, `"by partition"`, `"by weights"`, `"none"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByName => "by name",
            Self::ByPartition => "by partition",
            Self::ByWeights => "by weights",
            Self::None => "none",
        }
    }
}

/// Summary of one shape in an upper-body model for [`classify_upper_body_arms`].
#[derive(Debug, Clone, Copy)]
pub struct UpperBodyShape<'a> {
    pub name: Option<&'a str>,
    pub cap: bool,
    /// Non-cap dismemberment partition body parts on this shape (empty if unpartitioned).
    pub body_parts: &'a [u16],
    /// Whether this skinned shape has at least one triangle selected by [`partition_arm_triangles`].
    pub has_arm_weights: bool,
}

/// Whether `name` is an arm or hand bone of the human skeleton: the `Bip01 L UpperArm` and
/// `Bip01 R UpperArm` branches (upper arm, twist bones, forearm, foretwist, hand, thumb,
/// fingers), excluding torso, spine, clavicle, pauldron, head, leg, and weapon nodes
/// [observed: `meshes\characters\_male\skeleton.nif` and
/// `meshes\characters\_1stperson\skeleton.nif` node hierarchies; design, #3227].
pub fn is_arm_bone(name: &str) -> bool {
    let s = name.trim().to_ascii_lowercase();
    let Some(rest) = s.strip_prefix("bip01 ") else { return false };
    let rest = rest.trim_start();
    let Some(after_side) = rest
        .strip_prefix("l ")
        .or_else(|| rest.strip_prefix("r "))
        .or_else(|| rest.strip_prefix('l'))
        .or_else(|| rest.strip_prefix('r'))
    else {
        return false;
    };
    let part = after_side.trim_start();
    part.starts_with("upperarm")
        || part.starts_with("uparm")
        || part.starts_with("forearm")
        || part.starts_with("foretwist")
        || part.starts_with("hand")
        || part.starts_with("thumb")
        || part.starts_with("finger")
}

/// Sum `(arm_weight, total_weight)` over positive joint weights of one skinned vertex.
pub fn vertex_arm_weight(joints: [u16; 4], weights: [f32; 4], arm_bone: &[bool]) -> (f32, f32) {
    let mut arm = 0.0f32;
    let mut total = 0.0f32;
    for (j, w) in joints.into_iter().zip(weights) {
        if w > 0.0 {
            total += w;
            if arm_bone.get(j as usize).copied().unwrap_or(false) {
                arm += w;
            }
        }
    }
    (arm, total)
}

/// Whether triangle `tri` belongs to the arms by skin weights: its three vertices'
/// combined weight on arm/hand bones is positive and at least half their combined weight
/// [design, #3227]. Chest triangles touching one shoulder vertex stay on the torso; sleeve,
/// upper-arm, forearm, and hand triangles go to the arms.
pub fn triangle_is_arm(tri: [u32; 3], joints: &[[u16; 4]], weights: &[[f32; 4]], arm_bone: &[bool]) -> bool {
    let mut arm = 0.0f32;
    let mut total = 0.0f32;
    for idx in tri {
        let i = idx as usize;
        if let (Some(&j), Some(&w)) = (joints.get(i), weights.get(i)) {
            let (a, t) = vertex_arm_weight(j, w, arm_bone);
            arm += a;
            total += t;
        }
    }
    arm > 0.0 && arm * 2.0 >= total
}

/// Partition triangle-list `indices` of a skinned shape into `(arm_indices, body_indices)`
/// using [`is_arm_bone`] and [`triangle_is_arm`] [design, #3227]. Every complete triangle
/// lands in exactly one output list, so third-person rendering draws the union with no
/// gaps or overlap.
pub fn partition_arm_triangles(indices: &[u32], joints: &[[u16; 4]], weights: &[[f32; 4]], bone_names: &[&str]) -> (Vec<u32>, Vec<u32>) {
    let arm_bone: Vec<bool> = bone_names.iter().map(|n| is_arm_bone(n)).collect();
    if !arm_bone.iter().any(|&b| b) {
        return (Vec::new(), indices.to_vec());
    }
    let mut arm = Vec::new();
    let mut body = Vec::new();
    for tri in indices.chunks_exact(3) {
        let t = [tri[0], tri[1], tri[2]];
        if triangle_is_arm(t, joints, weights, &arm_bone) {
            arm.extend_from_slice(&t);
        } else {
            body.extend_from_slice(&t);
        }
    }
    (arm, body)
}

/// Classify how an upper-body model's first-person arms are found [design, #3227]:
/// 1. `ByName`: at least one shape is named `Arms…`.
/// 2. `ByPartition`: no `Arms…` shape, and at least one non-cap, non-sleeve shape has an
///    arm dismemberment partition (`is_arm_body_part`, #2457).
/// 3. `ByWeights`: neither of the above, and at least one non-cap, non-sleeve skinned
///    shape has arm-weighted triangles (`partition_arm_triangles`, #3227).
/// 4. `None`: none of the above.
pub fn classify_upper_body_arms(shapes: &[UpperBodyShape<'_>]) -> ArmsSource {
    if shapes.iter().any(|s| s.name.is_some_and(is_arms_shape)) {
        return ArmsSource::ByName;
    }
    if shapes.iter().any(|s| {
        !s.cap
            && s.name.is_none_or(|n| crate::mesh::pipboy_sleeve(n).is_none())
            && s.body_parts.iter().copied().any(is_arm_body_part)
    }) {
        return ArmsSource::ByPartition;
    }
    if shapes.iter().any(|s| {
        !s.cap
            && s.name.is_none_or(|n| crate::mesh::pipboy_sleeve(n).is_none())
            && s.has_arm_weights
    }) {
        return ArmsSource::ByWeights;
    }
    ArmsSource::None
}

/// How part `p` appears in first person, if at all.
pub fn show(actors: &ActorForms, p: &Part) -> Option<Show> {
    match p.role.as_str() {
        "body:upper_body" => Some(Show::Arms),
        "body:left_hand" | "body:right_hand" => Some(Show::Whole),
        "armor" => {
            let slots = actors.armors.get(&p.form)?.slots;
            if slots & slot::UPPER_BODY != 0 {
                Some(Show::Arms)
            } else if slots & (slot::LEFT_HAND | slot::RIGHT_HAND | slot::PIPBOY) != 0 {
                Some(Show::Whole)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether the first-person rig draws the shape named `name` of a part that shows as
/// `show` (`None`: the part is not in first person at all). The third-person body hides
/// the same shapes while the arms rig is up in front of it (player-body.md §1.5).
pub fn shape_shows(show: Option<Show>, name: Option<&str>) -> bool {
    match show {
        Some(Show::Whole) => true,
        Some(Show::Arms) => name.is_some_and(is_first_person_shape),
        None => false,
    }
}

/// Whether the first-person arms wear held weapon `w`'s own addons (WEAP BIPL,
/// [`Weapon::addons`]): it has some and no model for the first-person view
/// ([`Weapon::model_for`] with no mods on: no WNAM "1st Person Model", no MODL). An ARMA
/// has no first-person model of its own, only a male and a female biped model
/// [documented: GECK wiki ArmorAddon; `schemas/fnv/objects.json` ARMA], so its biped
/// model is the one worn, a `…1st.nif` beside it taking its place as for any hand part
/// ([`appearance`]; none of the fist weapons' addon models has one [documented: asset
/// inspection, archive listings]). In FalloutNV.esm and the add-ons 21 weapons have a
/// BIPL and no MODL, all hand-to-hand weapons whose only model besides the ground object
/// is a Right Hand addon (the Bladed Gauntlet's `ARMABladedGuantlet`, the brass and
/// spiked knuckles, the deathclaw, mantis and yao guai gauntlets, Old World Blues'
/// gloves, …) [documented: record decode]. With the weapon's addons left off, the
/// first-person view drew nothing of the Bladed Gauntlet (#3226, [observed: a player's
/// log]); the others have the same record shape [hypothesis: the same happened to
/// them]. [hypothesis: the original draws the addon on the first-person arms, as it does
/// on the third-person body.] A weapon with a model of its
/// own (the power fist's `PowerFistRigid.NIF`, the boxing gloves, the Shishkebab) keeps
/// the bare hand under that model in first person, as before [hypothesis].
pub fn wears_weapon_addons(w: &Weapon) -> bool {
    !w.addons.is_empty() && w.model_for(true, 0).is_none()
}

/// The items the first-person rig is dressed in, of those `worn` (armour and the held
/// weapon, as given to the resolver): all but a weapon with addons the arms do not wear
/// ([`wears_weapon_addons`]). The resolver gives a kept weapon's addons their slots as on
/// the third-person body: the race's hand and a worn addon on the same slot (an outfit's
/// glove) give way to them, one hand model per hand.
pub fn first_person_worn(actors: &ActorForms, worn: &[FormId]) -> Vec<FormId> {
    worn.iter().copied().filter(|id| actors.weapons.get(id).is_none_or(|w| w.addons.is_empty() || wears_weapon_addons(w))).collect()
}

/// The addons (members of an ARMO's or a WEAP's BIPL) of the items `worn` that
/// appearance `a` draws, in its parts' order, each once.
pub fn worn_addons(actors: &ActorForms, worn: &[FormId], a: &Appearance) -> Vec<FormId> {
    let lists = |w: &FormId| actors.armors.get(w).map(|x| &x.addons).or_else(|| actors.weapons.get(w).map(|x| &x.addons));
    let listed = |id: &FormId| worn.iter().any(|w| lists(w).is_some_and(|l| l.contains(id)));
    let mut out: Vec<FormId> = Vec::new();
    for p in a.parts.iter().filter(|p| p.role == "armor") {
        if listed(&p.form) && !out.contains(&p.form) {
            out.push(p.form);
        }
    }
    out
}

/// The appearance the first-person rig draws: the parts that show, hands swapped for
/// their `1st` variants where `exists` says the file is there, of FaceGen only the
/// texture coefficients (the skin tint of arms and hands, as in third person), the
/// first-person skeleton. Each kept part comes with how it shows.
pub fn appearance(actors: &ActorForms, a: &Appearance, exists: impl Fn(&str) -> bool) -> (Appearance, Vec<Show>) {
    let mut out = a.clone();
    out.skeleton = SKELETON.to_string();
    let tint_only = |f: &crate::records::FaceGen| crate::records::FaceGen { fgts: f.fgts.clone(), ..Default::default() };
    out.facegen = a.facegen.as_ref().map(tint_only);
    out.race_facegen = a.race_facegen.as_ref().map(tint_only);
    out.parts.clear();
    let mut shows = Vec::new();
    for p in &a.parts {
        let Some(s) = show(actors, p) else { continue };
        let mut p = p.clone();
        if s == Show::Whole {
            if let Some(v) = first_person_variant(&p.model).filter(|v| exists(v)) {
                p.model = v;
            }
        }
        out.parts.push(p);
        shows.push(s);
    }
    (out, shows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Armor;
    use fnv_sim::FormId;

    fn part(role: &str, form: u32, model: &str) -> Part {
        Part { role: role.into(), form: FormId(form), model: model.into(), texture: None }
    }

    #[test]
    fn arms_shapes() {
        for n in ["Arms", "Arms02", "Arms2", "Arms01:0", "Arms\t", "arms:1"] {
            assert!(is_arms_shape(n), "{n:?}");
        }
        for n in ["UpperBody", "PipBoyOn", "MeatCapLimbs", "body:0", "RightGlove:0"] {
            assert!(!is_arms_shape(n), "{n:?}");
        }
        for n in ["Arms", "Arms\t", "PipBoyOn", "PipBoyOn:0", "PipBoy0n:0", "PipBoyOff", "PipBoyOff:0", "pipboyoffmm_ncr:0"] {
            assert!(is_first_person_shape(n), "{n:?}");
        }
        for n in ["UpperBody", "MeatCapLimbs", "PipBoyArm:0", "PipboyLightEffect:0"] {
            assert!(!is_first_person_shape(n), "{n:?}");
        }
    }

    /// The left forearm sleeve goes with the arms whether a Pip-Boy is worn or not: one
    /// of the two is drawn by the arms rig, and so hidden on the third-person body under
    /// it, never left behind in the air (#2173).
    #[test]
    fn the_forearm_sleeve_is_an_arm_shape_with_and_without_a_pipboy() {
        let sleeves = ["PipBoyOn", "PipBoyOff"];
        for wears in [true, false] {
            let drawn: Vec<&str> = sleeves.iter().copied().filter(|n| !crate::mesh::hidden_for_pipboy(n, wears)).collect();
            assert_eq!(drawn, [if wears { "PipBoyOn" } else { "PipBoyOff" }]);
            assert!(drawn.iter().all(|n| shape_shows(Some(Show::Arms), Some(n))), "wears {wears}");
        }
    }

    #[test]
    fn an_outfit_without_arms_shapes_draws_its_arm_partitions() {
        // armor	roupersrmornvncrtrooperplain.nif (#2457): no `Arms…` shape; the
        // sleeves are partitions 3/3000 and 5/5000 of shapes named for other things.
        let names = ["meathead:0", "body:0", "upperbody2:0", "pipboyoff:0", "hest:0", "objdefault01:0", "objdefault02:0", "pipboyon:0"];
        assert!(arms_by_partition(names.iter().copied()));
        assert!(!arms_by_partition(["UpperBody", "Arms", "PipBoyOn", "PipBoyOff"].into_iter()), "a jumpsuit goes by its names");
        assert!(partition_arm_shape("hest:0", [0, 5000, 3000, 3]));
        assert!(partition_arm_shape("objdefault01:0", [3000, 5000]));
        assert!(partition_arm_shape("objdefault02:0", [0, 5]));
        assert!(!partition_arm_shape("body:0", [0, 7000, 10000, 10, 7]), "legs and torso");
        assert!(!partition_arm_shape("upperbody2:0", [0]));
        assert!(!partition_arm_shape("meatcap1:0", [103]), "a section cap");
        assert!(!partition_arm_shape("meatcap2:0", [203]), "a torso cap");
        assert!(!partition_arm_shape("pipboyoff:0", [3]), "the forearm sleeves go by their names");
        assert!(!partition_arm_shape("pipboyon:0", [3]));
        assert!((3..=6).all(is_arm_body_part) && [3000, 4000, 5000, 6000].into_iter().all(is_arm_body_part));
        assert!(![0, 1, 2, 7, 10, 103, 203, 1000, 7000, 65535].into_iter().any(is_arm_body_part));
    }

    #[test]
    fn shapes_of_a_part() {
        assert!(shape_shows(Some(Show::Whole), None));
        assert!(shape_shows(Some(Show::Whole), Some("RightGlove:0")));
        assert!(shape_shows(Some(Show::Arms), Some("Arms:0")));
        assert!(shape_shows(Some(Show::Arms), Some("PipBoyOn")));
        assert!(shape_shows(Some(Show::Arms), Some("PipBoyOff:0")));
        assert!(!shape_shows(Some(Show::Arms), Some("UpperBody")));
        assert!(!shape_shows(Some(Show::Arms), None));
        assert!(!shape_shows(None, Some("Arms:0")));
    }

    #[test]
    fn variants_and_parts() {
        assert_eq!(first_person_variant(r"meshes\characters\_male\lefthand.nif").as_deref(), Some(r"meshes\characters\_male\lefthand1st.nif"));
        assert_eq!(first_person_variant("x.dds"), None);
        let mut actors = ActorForms::default();
        let armor = |slots| Armor { edid: None, slots, worn: [None, None], dt: 0.0, dr_raw: 0, power_armor: false, ragdoll_template: None, addons: Vec::new() };
        actors.armors.insert(FormId(0x10), armor(slot::UPPER_BODY | slot::LEFT_HAND));
        actors.armors.insert(FormId(0x11), armor(slot::RIGHT_HAND));
        actors.armors.insert(FormId(0x12), armor(slot::HAT));
        actors.armors.insert(FormId(0x13), armor(slot::PIPBOY));
        let parts = [
            part("body:upper_body", 1, r"meshes\characters\_male\upperbody.nif"),
            part("body:right_hand", 1, r"meshes\characters\_male\righthand.nif"),
            part("head:head", 1, r"meshes\characters\head\headhuman.nif"),
            part("hair", 2, r"meshes\characters\hair\hairbase.nif"),
            part("armor", 0x10, r"meshes\armor\x\outfit.nif"),
            part("armor", 0x11, r"meshes\armor\x\glover.nif"),
            part("armor", 0x12, r"meshes\armor\x\hat.nif"),
            part("armor", 0x13, r"meshes\pipboy3000\pipboyarm.nif"),
        ];
        let shows: Vec<_> = parts.iter().map(|p| show(&actors, p)).collect();
        assert_eq!(shows, [Some(Show::Arms), Some(Show::Whole), None, None, Some(Show::Arms), Some(Show::Whole), None, Some(Show::Whole)]);
    }

    #[test]
    fn arm_bone_classification_matches_skeleton_arm_and_hand_branches() {
        for bone in [
            "Bip01 L UpperArm",
            "Bip01 R UpperArm",
            "Bip01 LUpArmTwistBone",
            "Bip01 RUpArmTwistBone",
            "Bip01 L Forearm",
            "Bip01 R Forearm",
            "Bip01 L ForeTwist",
            "Bip01 R ForeTwist",
            "Bip01 R ForeTwistDriver",
            "Bip01 L Hand",
            "Bip01 R Hand",
            "Bip01 L Hand01",
            "Bip01 R Hand04",
            "Bip01 L Thumb1",
            "Bip01 R Thumb12",
            "Bip01 L Finger1",
            "Bip01 R Finger42",
        ] {
            assert!(is_arm_bone(bone), "expected arm bone: {bone:?}");
        }
        for bone in [
            "Bip01 Spine",
            "Bip01 Spine1",
            "Bip01 Spine2",
            "Bip01 Pelvis",
            "Bip01 Neck",
            "Bip01 Head",
            "Bip01 L Clavicle",
            "Bip01 R Clavicle",
            "Bip01 LPauldron",
            "Bip01 RPauldron",
            "Bip01 L Thigh",
            "Bip01 R Calf",
            "Bip01 L Foot",
            "Bip01 R Toe0",
            "Weapon",
            "Scene Root",
        ] {
            assert!(!is_arm_bone(bone), "expected non-arm bone: {bone:?}");
        }
    }

    /// Regression test for #3227: a custom outfit model with one unnamed skinned shape and
    /// no dismemberment arm partitions selects its arm/hand-weighted triangles by weights
    /// and cleanly separates them from the torso triangles.
    #[test]
    fn unnamed_single_shape_without_arm_partitions_selects_arms_by_weights() {
        let bones = ["Bip01 Spine2", "Bip01 L Clavicle", "Bip01 L UpperArm", "Bip01 L Forearm", "Bip01 R Hand"];
        // Vertices:
        // 0, 1: pure torso (Spine2)
        // 2: shoulder seam (0.6 Clavicle, 0.4 L UpperArm)
        // 3: upper arm (1.0 L UpperArm)
        // 4, 5: forearm / hand (1.0 L Forearm, 1.0 R Hand)
        let joints = [
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            [1, 2, 0, 0],
            [2, 0, 0, 0],
            [3, 0, 0, 0],
            [4, 0, 0, 0],
        ];
        let weights = [
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.6, 0.4, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];
        // Four triangles:
        // [0, 1, 2]: chest touching shoulder (arm 0.4 / 3.0) -> torso
        // [0, 1, 3]: chest touching upper arm (arm 1.0 / 3.0) -> torso
        // [2, 3, 4]: shoulder-to-forearm sleeve (arm 2.4 / 3.0) -> arm
        // [3, 4, 5]: pure arm/hand (arm 3.0 / 3.0) -> arm
        let indices = [0, 1, 2, 0, 1, 3, 2, 3, 4, 3, 4, 5];
        let (arm, body) = partition_arm_triangles(&indices, &joints, &weights, &bones);
        assert_eq!(arm, [2, 3, 4, 3, 4, 5]);
        assert_eq!(body, [0, 1, 2, 0, 1, 3]);

        // Single unnamed skinned shape with no arm partitions -> ByWeights.
        let model = [UpperBodyShape { name: None, cap: false, body_parts: &[], has_arm_weights: !arm.is_empty() }];
        assert_eq!(classify_upper_body_arms(&model), ArmsSource::ByWeights);
        assert_eq!(ArmsSource::ByWeights.as_str(), "by weights");

        // Precedence check: ByName > ByPartition > ByWeights > None.
        let by_name = [
            UpperBodyShape { name: Some("Arms:0"), cap: false, body_parts: &[3], has_arm_weights: true },
            UpperBodyShape { name: Some("UpperBody"), cap: false, body_parts: &[0], has_arm_weights: false },
        ];
        assert_eq!(classify_upper_body_arms(&by_name), ArmsSource::ByName);
        let by_partition = [UpperBodyShape { name: Some("hest:0"), cap: false, body_parts: &[0, 3000], has_arm_weights: true }];
        assert_eq!(classify_upper_body_arms(&by_partition), ArmsSource::ByPartition);
        let torso_only = [UpperBodyShape { name: None, cap: false, body_parts: &[0], has_arm_weights: false }];
        assert_eq!(classify_upper_body_arms(&torso_only), ArmsSource::None);
    }
}
