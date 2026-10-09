//! Bevy asset plugins shared by the viewer and the game: the game's archives (VFS), NIF
//! scene graphs and prepared static models, DDS textures, KF playback.

pub mod actor;
pub mod anim_play;
pub mod data;
pub mod model;
pub mod nif_material;
pub mod nif_spawn;
pub mod texture;

pub use data::{ArchivePlugin, GameData};
pub use model::{ModelCache, PreparedModel, PreparedShape};
pub use nif_material::{fade_prepass, nif_material, NifMaterial, NifMaterialPlugin};
pub use nif_spawn::{NifHidden, NifRoot, SpawnCtx, SpawnReport};
pub use texture::TextureCache;
