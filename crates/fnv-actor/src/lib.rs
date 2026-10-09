//! Actor appearance (workstream 5, docs/design/static-actors.md): which skeleton, race
//! body and head parts, hair, head parts and worn armour make up a placed NPC, and the
//! FaceGen coefficients that shape its face. Headless and deterministic: the engine
//! assembles the parts into one entity; this crate only decides what they are.
//!
//! - [`records`]: NPC_, RACE, HAIR, EYES, HDPT, ARMO, LVLN and WEAP read through the Layer 1
//!   schema
//! - [`resolve`]: placed actor → [`resolve::Appearance`] (parts list, worn armour)
//! - [`facegen`]: `.egm`/`.egt` FaceGen morphs and applying coefficients to them
//! - [`mesh`]: how a part mesh joins the skeleton (skinned by bone name, rigid by `Prn`)
//! - [`idle`]: IDLE records (idle animations) as data: model, links, group, raw conditions
//! - [`anims`]: where `.kf` files sit relative to skeletons, and what their tracks bind to
//! - [`first_person`]: what the player's first-person rig draws (arms and hands)
//! - [`locomotion`]: which clips a humanoid under direct control plays (moving, sneaking,
//!   drawing and holstering a weapon), and the draw/holster sequence
//!
//! Facts and rules with confidence tags: docs/spec-notes/actors.md and (animation data)
//! docs/spec-notes/animation.md.

pub mod anims;
pub mod facegen;
pub mod first_person;
pub mod idle;
pub mod locomotion;
pub mod mesh;
pub mod records;
pub mod resolve;

pub use records::ActorForms;
pub use resolve::{Appearance, Part, ResolveError, Resolver};
