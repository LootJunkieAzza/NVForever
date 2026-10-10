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
//!   pipboy, it is rendered over the outfit using the pipboy on node"]. Whether the
//!   actor wears one is decided where the parts are assembled
//!   ([`crate::mesh::hidden_for_pipboy`]), which leaves that sleeve out otherwise.

use crate::resolve::{slot, Appearance, Part};
use crate::ActorForms;

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
/// and the sleeve worn with a Pip-Boy (`PipBoyOn…`).
pub fn is_first_person_shape(name: &str) -> bool {
    is_arms_shape(name) || crate::mesh::pipboy_sleeve(name) == Some(true)
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

/// The appearance the first-person rig draws: the parts that show, hands swapped for
/// their `1st` variants where `exists` says the file is there, of FaceGen only the
/// texture coefficients (the skin tint of arms and hands, as in third person), the
/// first-person skeleton. Each kept part comes with how it shows.
pub fn appearance(actors: &ActorForms, a: &Appearance, exists: impl Fn(&str) -> bool) -> (Appearance, Vec<Show>) {
    let mut out = a.clone();
    out.skeleton = SKELETON.to_string();
    out.facegen = a.facegen.as_ref().map(|f| crate::records::FaceGen { fgts: f.fgts.clone(), ..Default::default() });
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
        for n in ["Arms", "Arms\t", "PipBoyOn", "PipBoyOn:0", "PipBoy0n:0"] {
            assert!(is_first_person_shape(n), "{n:?}");
        }
        for n in ["UpperBody", "PipBoyOff", "PipBoyOff:0", "MeatCapLimbs"] {
            assert!(!is_first_person_shape(n), "{n:?}");
        }
    }

    #[test]
    fn variants_and_parts() {
        assert_eq!(first_person_variant(r"meshes\characters\_male\lefthand.nif").as_deref(), Some(r"meshes\characters\_male\lefthand1st.nif"));
        assert_eq!(first_person_variant("x.dds"), None);
        let mut actors = ActorForms::default();
        let armor = |slots| Armor { edid: None, slots, worn: [None, None], dt: 0.0, dr_raw: 0, power_armor: false };
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
}
