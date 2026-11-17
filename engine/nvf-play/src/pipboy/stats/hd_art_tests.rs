//! Actual fallback-page image reads and emitted image rectangles, independent of GPU layout.
use super::*;
use super::model::Standing;
use bevy::ecs::system::RunSystemOnce;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

#[derive(Clone, Copy)]
enum PageArt { List, Reputation, Karma }

struct OwnedFiles { root: std::path::PathBuf, files: Vec<std::path::PathBuf> }

impl Drop for OwnedFiles {
    fn drop(&mut self) {
        for file in &self.files { let _ = std::fs::remove_file(file); }
        // These exact directories were created below, under this test's unique root.
        for part in ["textures/interface/icons", "textures/interface", "textures", ""] {
            let _ = std::fs::remove_dir(self.root.join(part));
        }
    }
}

#[derive(Resource)]
struct Selected { image: Handle<Image>, full: Rect }

struct Drawn { node: Node, cut: Rect, full: Rect }

fn draw_art(size: u32, atlas: Option<bool>, page: PageArt, tga: bool) -> Drawn {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!("nvf-owned-stats-art-{}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir_all(root.join("textures/interface/icons")).unwrap();
    let mut files = OwnedFiles { root, files: Vec::new() };
    let path = match page {
        PageArt::Karma => karma_picture("owned"),
        _ => "Interface\\Icons\\owned.dds".into(),
    };
    // Karma's normal path is mapped to our owned file by an authored atlas when needed;
    // otherwise use its exact own-file location with individually tracked directories.
    let own_path = files.root.join("textures").join(path.replace('\\', "/").to_ascii_lowercase());
    std::fs::create_dir_all(own_path.parent().unwrap()).unwrap();
    let bytes = if tga {
        let mut bytes = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        bytes.extend((size as u16).to_le_bytes()); bytes.extend((size as u16).to_le_bytes());
        bytes.extend([32, 40]); bytes.resize(18 + size as usize * size as usize * 4, 255); bytes
    } else { crate::interface::tests::authored_rgba_dds(size, size) };
    std::fs::write(&own_path, bytes).unwrap(); files.files.push(own_path);
    if atlas.is_some() {
        let index = files.root.join("textures/interface/interfaceshared.tai");
        let text = "owned.dds owned-atlas.dds, 0, 2D, 0.125, 0.25, 0, 0.25, 0.25\n";
        std::fs::write(&index, text).unwrap(); files.files.push(index);
        if atlas == Some(true) {
            let image = files.root.join("textures/interface/owned-atlas.dds");
            std::fs::write(&image, crate::interface::tests::authored_rgba_dds(1024, 1024)).unwrap(); files.files.push(image);
        }
    }
    let data = nvf_assets::GameData { vfs: Arc::new(fnv_vfs::Vfs::open(&files.root, &[]).unwrap()), schema: Arc::new(fnv_nif::Schema::default()) };
    let _canvas = crate::interface::original_units();
    let mut world = World::new();
    world.run_system_once(move |mut commands: Commands| {
        let mut kit = Kit::default(); let mut images = Assets::<Image>::default(); let glyphs = crate::pad::PadGlyphs::default();
        let mut c = Ctx::new(&mut kit, Some(&data), &mut images, None, &glyphs);
        let (image, part) = c.picture(&path).expect("actual selected own image or atlas fallback");
        let full = part.unwrap_or_else(|| Rect::from_corners(Vec2::ZERO, c.images.get(&image).unwrap().size().as_vec2()));
        let mut view = StatsView { reputation: matches!(page, PageArt::Reputation), ..Default::default() }; let mut snap = Snapshot::default();
        match page {
            PageArt::List => snap.skills.push(Row { name: "Owned skill".into(), icon: Some(path.clone()), ..default() }),
            PageArt::Reputation => { snap.standings.push(Standing { form: fnv_sim::FormId(1), name: "Owned town".into(), title: "Known", fame: 0.0, infamy: 0.0, icon: Some(path.clone()) }); }
            PageArt::Karma => snap.karma = Some((0.0, "Neutral", "owned")),
        }
        commands.spawn(Node::default()).with_children(|p| match page {
            PageArt::List => listed(p, &mut c, &mut view, &snap, 2),
            _ => general(p, &mut c, &mut view, &snap),
        });
        commands.insert_resource(Selected { image, full });
    }).unwrap();
    let selected = world.resource::<Selected>(); let (handle, full) = (selected.image.clone(), selected.full);
    let (node, cut) = world.query::<(&Node, &ImageNode)>().iter(&world).find_map(|(node, image)| (image.image == handle).then(|| (node.clone(), image.rect.expect("fallback art emits an explicit crop")))).expect("actual fallback page must emit the selected fixture artwork");
    // The karma directory is an exact child created only for this owned fixture.
    if matches!(page, PageArt::Karma) {
        for file in &files.files { let _ = std::fs::remove_file(file); }
        for part in ["textures/interface/icons/pipboyimages/karma icons", "textures/interface/icons/pipboyimages"] { let _ = std::fs::remove_dir(files.root.join(part)); }
    }
    Drawn { node, cut, full }
}

fn same_art(native: &Drawn, hd: &Drawn) {
    assert_eq!((native.node.left, native.node.top), (hd.node.left, hd.node.top), "replacement density must preserve the actual page position");
    // Cutting an atlas subrectangle can round the multiply back to canvas units once
    // differently. Admit one representable f32 step, not a pixel- or percent-sized drift.
    for (dimension, native, hd) in [("width", native.node.width, hd.node.width), ("height", native.node.height, hd.node.height)] {
        let (Val::Vh(native), Val::Vh(hd)) = (native, hd) else { panic!("actual fallback {dimension} must retain canvas-relative units") };
        assert!(native.is_finite() && hd.is_finite() && native > 0.0 && hd > 0.0, "actual fallback {dimension} must be finite and positive");
        assert!(native.to_bits().abs_diff(hd.to_bits()) <= 1, "replacement density must preserve actual {dimension} to one f32 step: {native:?} versus {hd:?}");
    }
    let a = (native.cut.max - native.full.min) / native.full.size();
    let b = (hd.cut.max - hd.full.min) / hd.full.size();
    assert!((a - b).length() < 0.0001, "fallback must sample the same normalized art, not its upper-left quarter: {a:?} versus {b:?}");
}

#[test]
fn high_resolution_skill_art_keeps_the_whole_native_source_region() {
    let native = draw_art(256, None, PageArt::List, false);
    let hd = draw_art(512, None, PageArt::List, true);
    same_art(&native, &hd);
    assert!(hd.cut.max.x > 0.73 * hd.full.max.x, "the right half of the selected art must be sampled");
}

#[test]
fn karma_and_reputation_fallbacks_use_their_selected_image_density() {
    for page in [PageArt::Karma, PageArt::Reputation] { same_art(&draw_art(256, None, page, false), &draw_art(1024, None, page, false)); }
}

#[test]
fn native_atlas_and_missing_atlas_fallback_keep_their_actual_source() {
    let native = draw_art(256, None, PageArt::List, false);
    let atlas = draw_art(1024, Some(true), PageArt::List, false);
    same_art(&native, &atlas);
    assert_eq!(atlas.full, Rect::new(128.0, 256.0, 384.0, 512.0), "shadowed own-file dimensions must not resize atlas art");
    same_art(&native, &draw_art(1024, Some(false), PageArt::List, false));
}
