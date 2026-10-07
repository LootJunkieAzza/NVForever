//! ArchivePlugin: the game's virtual file system and the NIF schema as resources.

use bevy::prelude::*;
use fnv_nif::Schema;
use fnv_vfs::Vfs;
use std::path::Path;
use std::sync::Arc;

#[derive(Resource, Clone)]
pub struct GameData {
    pub vfs: Arc<Vfs>,
    pub schema: Arc<Schema>,
}

impl GameData {
    /// Open the default archive set of `data_dir` (read-only) and load the schemas.
    pub fn open(data_dir: &Path, schema_dir: &Path) -> Result<GameData, String> {
        let vfs = Vfs::open_default(data_dir).map_err(|e| e.to_string())?;
        let schema = Schema::load_dir(schema_dir).map_err(|e| e.to_string())?;
        Ok(GameData { vfs: Arc::new(vfs), schema: Arc::new(schema) })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.vfs.read(path)? {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("{path}: {e}");
                None
            }
        }
    }
}

pub struct ArchivePlugin(pub GameData);

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone());
    }
}
