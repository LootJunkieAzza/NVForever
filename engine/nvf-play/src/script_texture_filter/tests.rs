use super::*;
use bevy::image::ImageSampler;
use nvf_assets::{
    data::GameData,
    texture::{sampler, set_anisotropy},
};
use std::{path::PathBuf, sync::Mutex};
static LOCK: Mutex<()> = Mutex::new(());
struct OwnedData {
    root: PathBuf,
    data: GameData,
}
impl OwnedData {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nvf-filter-owned-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("textures")).unwrap();
        let mut bytes = vec![0u8; 136];
        bytes[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [
            (4, 124u32),
            (8, 0x21007),
            (12, 4),
            (16, 4),
            (28, 1),
            (76, 32),
            (80, 4),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[84..88].copy_from_slice(b"DXT1");
        std::fs::write(root.join("textures/owned.dds"), bytes).unwrap();
        let schemas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif");
        let data = GameData::open_for_plugins(&root, &[] as &[&str], &schemas).unwrap();
        Self { root, data }
    }
}
impl Drop for OwnedData {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.root.join("textures/owned.dds"));
        let _ = std::fs::remove_dir(self.root.join("textures"));
        let _ = std::fs::remove_dir(&self.root);
    }
}
struct Restore(u16);
impl Drop for Restore {
    fn drop(&mut self) {
        set_anisotropy(self.0);
    }
}
fn saved() -> Restore {
    let ImageSampler::Descriptor(s) = sampler() else {
        unreachable!()
    };
    Restore(s.anisotropy_clamp)
}
fn level(image: &Image) -> u16 {
    let ImageSampler::Descriptor(s) = &image.sampler else {
        unreachable!()
    };
    s.anisotropy_clamp
}
fn script() -> ScriptState {
    let mut body = b"HEDR".to_vec();
    body.extend_from_slice(&12u16.to_le_bytes());
    body.extend_from_slice(&1.34f32.to_le_bytes());
    body.extend_from_slice(&[0; 8]);
    let mut bytes = b"TES4".to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&body);
    let plugin = fnv_esm::Plugin::parse(&bytes, "Filtering.esm".into()).unwrap();
    ScriptState::over_plugins(&[plugin])
}
fn source(script: &mut ScriptState, value: u16) {
    script
        .rt
        .as_mut()
        .unwrap()
        .game
        .run_query(
            &format!("Begin Function {{}}\nSetMaxAniso {value}\nEnd"),
            Default::default(),
        )
        .unwrap();
}

#[test]
fn real_source_request_changes_the_loaded_dds_sampler_once_and_preserves_foreign_images() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = saved();
    set_anisotropy(8);
    let owned = OwnedData::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::default();
    let h = cache
        .get("owned.dds", true, &owned.data, &mut images, None)
        .unwrap();
    let foreign = images.add(images.get(&h).unwrap().clone());
    let bytes = images.get(&h).unwrap().data.clone();
    let mut script = script();
    source(&mut script, 16);
    let mut app = App::new();
    app.insert_resource(script)
        .insert_resource(cache)
        .insert_resource(images)
        .add_plugins(ScriptTextureFilterPlugin);
    app.update();
    let images = app.world().resource::<Assets<Image>>();
    assert_eq!(level(images.get(&h).unwrap()), 16);
    assert_eq!(level(images.get(&foreign).unwrap()), 8);
    assert_eq!(images.get(&h).unwrap().data, bytes);
    assert_eq!(
        app.world_mut()
            .resource_mut::<ScriptState>()
            .rt
            .as_mut()
            .unwrap()
            .game
            .take_texture_filter_level(),
        None
    );
    {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let mut image = images.get_mut(&h).unwrap();
        let ImageSampler::Descriptor(s) = &mut image.sampler else {
            unreachable!()
        };
        s.anisotropy_clamp = 4;
    }
    app.update();
    assert_eq!(
        level(app.world().resource::<Assets<Image>>().get(&h).unwrap()),
        4,
        "no request means no restoration over an external equal-handle write"
    );
}

#[test]
fn absent_image_resource_defers_the_request_until_the_actual_consumer_is_available() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = saved();
    set_anisotropy(8);
    let owned = OwnedData::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::default();
    let h = cache
        .get("owned.dds", true, &owned.data, &mut images, None)
        .unwrap();
    let mut script = script();
    source(&mut script, 16);
    let mut app = App::new();
    app.insert_resource(script)
        .add_plugins(ScriptTextureFilterPlugin);
    app.update();
    app.insert_resource(cache);
    app.update();
    assert_eq!(level(images.get(&h).unwrap()), 8);
    app.insert_resource(images);
    app.update();
    assert_eq!(
        level(app.world().resource::<Assets<Image>>().get(&h).unwrap()),
        16
    );
    assert_eq!(
        app.world_mut()
            .resource_mut::<ScriptState>()
            .rt
            .as_mut()
            .unwrap()
            .game
            .take_texture_filter_level(),
        None
    );
}
