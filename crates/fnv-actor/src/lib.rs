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
//! - [`idle`]: IDLE records (idle animations) as data: model, links, group, raw conditions
//! - [`anims`]: where `.kf` files sit relative to skeletons, and what their tracks bind to
//! - [`locomotion`]: idle, walk, run and turn clips, their root motion, and choosing
//!   and blending between them
//! - [`weapon`]: the weapon an actor draws, its stance clips and the draw cycle
//!
//! Facts and rules with confidence tags: docs/spec-notes/actors.md and (animation data)
//! docs/spec-notes/animation.md.

pub mod anims;
pub mod facegen;
pub mod idle;
pub mod locomotion;
pub mod mesh;
pub mod records;
pub mod resolve;
pub mod weapon;

pub use records::ActorForms;
pub use resolve::{Appearance, Part, ResolveError, Resolver};
