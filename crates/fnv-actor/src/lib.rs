//! Actor appearance (workstream 5, docs/design/static-actors.md): which skeleton, race
//! body and head parts, hair, head parts and worn armour make up a placed NPC, and the
//! FaceGen coefficients that shape its face. Headless and deterministic: the engine
//! assembles the parts into one entity; this crate only decides what they are.
//!
//! - [`records`]: NPC_, RACE, HAIR, EYES, HDPT, ARMO and LVLN read through the Layer 1
//!   schema
//! - [`resolve`]: placed actor → [`resolve::Appearance`] (parts list, worn armour)
//! - [`facegen`]: `.egm`/`.egt` FaceGen morphs and applying coefficients to them
//! - [`mesh`]: how a part mesh joins the skeleton (skinned by bone name, rigid by `Prn`)
//!
//! Facts and rules with confidence tags: docs/spec-notes/actors.md.

pub mod facegen;
pub mod mesh;
pub mod records;
pub mod resolve;

pub use records::ActorForms;
pub use resolve::{Appearance, Part, ResolveError, Resolver};
