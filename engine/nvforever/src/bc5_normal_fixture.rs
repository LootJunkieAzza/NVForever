//! Authored consumer inputs; no installed textures or decoder-generated oracle.

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use nvf_assets::{GameData, NifMaterial, TextureCache};
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

pub struct Inputs(PathBuf);

pub fn dds(code: &[u8; 4], width: u32, height: u32, block: [u8; 16]) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..4].copy_from_slice(b"DDS ");
    for (at, value) in [(4, 124u32), (8, 0x81007), (12, height), (16, width), (28, 1), (76, 32), (80, 4), (108, 0x1000)] {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[84..88].copy_from_slice(code);
    for _ in 0..width.div_ceil(4) * height.div_ceil(4) { bytes.extend_from_slice(&block); }
    bytes
}

pub fn inputs() -> (Inputs, World) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("nvf-bc5-consumers-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("textures")).unwrap();
    for (name, code, w, h, x, y) in [
        ("signed.dds", *b"BC5S", 4, 4, 64, 192),
        ("partial.dds", *b"BC5S", 3, 5, 64, 192),
        ("unsigned.dds", *b"ATI2", 4, 4, 192, 63),
    ] {
        let mut block = [0; 16]; block[0] = x; block[8] = y;
        std::fs::write(root.join("textures").join(name), dds(&code, w, h, block)).unwrap();
    }
    std::fs::write(root.join("textures/old.dds"), dds(b"DXT5", 4, 4, [255; 16])).unwrap();
    let mut world = headless_world();
    world.insert_resource(GameData { schema: Arc::new(fnv_nif::Schema::default()), vfs: Arc::new(fnv_vfs::Vfs::open(&root, &[]).unwrap()) });
    world.init_resource::<TextureCache>();
    world.init_resource::<nvf_assets::model::ModelCache>();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<NifMaterial>>();
    world.init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>();
    (Inputs(root), world)
}

pub fn normal(images: &Assets<Image>, handle: &Handle<Image>, size: UVec2) {
    let image = images.get(handle).unwrap();
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    assert_eq!(image.size(), size);
    assert_eq!(image.data.as_deref(), Some([192, 63, 217, 255].repeat((size.x * size.y) as usize).as_slice()));
}

impl Drop for Inputs {
    fn drop(&mut self) {
        for name in ["signed.dds", "partial.dds", "unsigned.dds", "old.dds"] {
            let _ = std::fs::remove_file(self.0.join("textures").join(name));
        }
        let _ = std::fs::remove_dir(self.0.join("textures"));
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn headless_world() -> World {
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::pbr::decal::ForwardDecalPlugin;
    use bevy::shader::{Shader, ShaderLoader};

    // The actual ForwardDecal on_add hook needs its plugin-owned mesh resource.
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Shader>()
        .init_asset_loader::<ShaderLoader>()
        .add_plugins(bevy::mesh::MeshPlugin)
        .add_plugins(ForwardDecalPlugin);
    app.finish();
    app.cleanup();
    assert!(app.get_sub_app(bevy::render::RenderApp).is_none());
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
    std::mem::take(app.world_mut())
}
