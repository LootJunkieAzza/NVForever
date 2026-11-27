//! Modern Minimap's map picture, as its plugin (ModernMinimap.dll) draws it for the HUD's
//! `ModernMinimap` window (docs/spec-notes/menu-xml.md §11; the tiles' side is
//! `nvf_play::tiles::modern_minimap`). Without it the window's `MapRender` image stayed the
//! file's placeholder (`Interface\Shared\solid.dds`), a plain square under the icons.
//!
//! A camera of its own renders into a texture [design: as `crate::reflection`], active
//! only in the frames the window asks for a render: orthographic, straight down over the
//! player, north up, `_FrustumSize` game units each way, the world layer only (not the
//! player's body or arms, not the water's own layer), no fog, black where nothing is. The
//! file's classic tint (`systemcolor`, `brightness`) is applied by the tiles themselves.
//! The picture defaults to 1024 texels square [documented: Modern Minimap - ESPless
//! v4.2.1 (by Stentorious & xConfused, `https://www.nexusmods.com/newvegas/mods/90574`),
//! shipped `Data\NVSE\Plugins\ModernMinimap.ini`, comment `"; Controls the texture size
//! of the minimap render"`, default `[General] textureSize=1024`; no DLL or plugin source
//! consulted]. When that INI is present in the VFS, `[General] textureSize` is read
//! dynamically and clamped/snapped to a power of two in `256..=2048` [design], driving
//! both the render-target texture and the size published to `RuntimeImages`.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::light::SimulationLightSystems;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::transform::TransformSystems;
use nvf_play::tiles::modern_minimap::{rendered, request, Request};

/// The runtime picture's name (`nvf_play::tiles::runtime_images`).
const IMAGE: &str = "modern_minimap";
/// Relative VFS path of Modern Minimap's INI (case-insensitive lookup via `Vfs`)
/// [documented: Modern Minimap - ESPless v4.2.1 (`https://www.nexusmods.com/newvegas/mods/90574`),
/// shipped `Data\NVSE\Plugins\ModernMinimap.ini`].
const INI_PATH: &str = r"NVSE\Plugins\ModernMinimap.ini";
/// Default picture size, texels [documented: Modern Minimap - ESPless v4.2.1
/// (`https://www.nexusmods.com/newvegas/mods/90574`), shipped
/// `Data\NVSE\Plugins\ModernMinimap.ini`, comment `"; Controls the texture size of the
/// minimap render"`, default `[General] textureSize=1024`].
const DEFAULT_SIZE: u32 = 1024;
/// Minimum clamped render-target size, texels [design: NVForever render-target bound].
const MIN_SIZE: u32 = 256;
/// Maximum clamped render-target size, texels [design: NVForever render-target bound].
const MAX_SIZE: u32 = 2048;
/// How far below the camera it draws beyond its height over the player, game units
/// [design: the file's camera stands 10,000 over the player].
const DEPTH: f32 = 20000.0;

#[derive(Component)]
struct MinimapCamera;

#[derive(Resource)]
struct MinimapImage {
    handle: Handle<Image>,
    size: u32,
}

/// Outcome of parsing `[General] textureSize` from `ModernMinimap.ini` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IniTextureSize {
    /// Key was not present under `[General]`.
    Absent,
    /// Key was present under `[General]`, but its value was invalid (non-UTF-8, non-numeric,
    /// non-finite, or `< 1`).
    Invalid,
    /// Key parsed and sanitized to a power-of-two texel size in `MIN_SIZE..=MAX_SIZE`.
    Value(u32),
}

fn trim_ascii_bytes(mut bytes: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = bytes.split_first() {
        if matches!(b, b' ' | b'\t' | b'\r') {
            bytes = rest;
        } else {
            break;
        }
    }
    while let Some((&b, rest)) = bytes.split_last() {
        if matches!(b, b' ' | b'\t' | b'\r') {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

/// Sanitize a parsed `textureSize` number into a power-of-two texel size in
/// `MIN_SIZE..=MAX_SIZE` (nearest power of two, rounding up on ties) [design].
/// Rejects non-finite and `< 1.0` values so callers fall back to [`DEFAULT_SIZE`].
fn sanitize_texture_size(raw: f64) -> Option<u32> {
    if !raw.is_finite() || raw < 1.0 {
        return None;
    }
    if raw <= f64::from(MIN_SIZE) {
        return Some(MIN_SIZE);
    }
    if raw >= f64::from(MAX_SIZE) {
        return Some(MAX_SIZE);
    }
    let rounded = raw.round() as u32;
    let hi = rounded.next_power_of_two().min(MAX_SIZE);
    let lo = (hi / 2).max(MIN_SIZE);
    Some(if rounded - lo < hi - rounded { lo } else { hi })
}

/// Parse `[General] textureSize` from raw INI bytes without raw `&str` byte-index slicing
/// (UTF-8 BOM, CRLF, `;`/`#` comments, and invalid-UTF-8 safe) [design].
fn parse_texture_size(bytes: &[u8]) -> IniTextureSize {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut in_general = false;
    let mut result = IniTextureSize::Absent;
    for raw_line in bytes.split(|&b| b == b'\n') {
        let line = raw_line.split(|&b| b == b';' || b == b'#').next().unwrap_or(b"");
        let line = trim_ascii_bytes(line);
        if line.is_empty() {
            continue;
        }
        if let Some(after_open) = line.strip_prefix(b"[") {
            if let Some(close_pos) = after_open.iter().position(|&b| b == b']') {
                let section = trim_ascii_bytes(&after_open[..close_pos]);
                in_general = section.eq_ignore_ascii_case(b"general");
            }
            continue;
        }
        if !in_general {
            continue;
        }
        let Some(eq_pos) = line.iter().position(|&b| b == b'=') else {
            continue;
        };
        let key = trim_ascii_bytes(&line[..eq_pos]);
        if !key.eq_ignore_ascii_case(b"texturesize") {
            continue;
        }
        let val = trim_ascii_bytes(&line[eq_pos + 1..]);
        result = match std::str::from_utf8(val).ok().and_then(|s| s.parse::<f64>().ok()).and_then(sanitize_texture_size) {
            Some(size) => IniTextureSize::Value(size),
            None => IniTextureSize::Invalid,
        };
    }
    result
}

/// Resolve the minimap render-target size from `NVSE\Plugins\ModernMinimap.ini` via the VFS,
/// defaulting to [`DEFAULT_SIZE`] when absent or invalid (vanilla unchanged) [design].
fn resolve_texture_size(data: Option<&nvf_assets::GameData>) -> u32 {
    let Some(bytes) = data.and_then(|d| d.vfs.exists(INI_PATH).then(|| d.read(INI_PATH)).flatten()) else {
        return DEFAULT_SIZE;
    };
    match parse_texture_size(&bytes) {
        IniTextureSize::Value(size) => {
            info!("modern minimap: textureSize={size} from {INI_PATH}");
            size
        }
        IniTextureSize::Absent => DEFAULT_SIZE,
        IniTextureSize::Invalid => {
            warn!("modern minimap: {INI_PATH} has invalid [General] textureSize; using {DEFAULT_SIZE}");
            DEFAULT_SIZE
        }
    }
}

/// The camera's place and view for a request: over the player, looking straight down,
/// north (game +y, Bevy -z) up, the frustum's width each way.
fn camera_for(r: &Request) -> (Transform, OrthographicProjection) {
    let [x, y, z] = r.centre;
    let t = Transform::from_xyz(x, z + r.offset, -y).looking_to(Vec3::NEG_Y, Vec3::NEG_Z);
    let p = OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: 2.0 * r.half, height: 2.0 * r.half },
        near: 0.0,
        far: r.offset.abs() + DEPTH,
        ..OrthographicProjection::default_3d()
    };
    (t, p)
}

pub struct MinimapRenderPlugin;

impl Plugin for MinimapRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            PostUpdate,
            update
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::UpdateFrusta)
                // An active camera needs its sun cascades (as `crate::reflection`).
                .before(SimulationLightSystems::UpdateDirectionalLightCascades),
        );
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, data: Option<Res<nvf_assets::GameData>>) {
    let size = resolve_texture_size(data.as_deref());
    let mut image = Image::new_uninit(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let handle = images.add(image);
    commands.spawn((
        Name::new("Modern Minimap camera"),
        MinimapCamera,
        Camera3d::default(),
        Camera { order: -2, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(handle.clone().into()),
        Msaa::Off,
        Projection::Orthographic(OrthographicProjection::default_3d()),
        RenderLayers::layer(0),
    ));
    commands.insert_resource(MinimapImage { handle, size });
}

fn publish_image(runtime: &mut nvf_play::tiles::runtime_images::RuntimeImages, image: &MinimapImage) -> String {
    runtime.set(IMAGE, image.handle.clone(), Vec2::splat(image.size as f32))
}

/// Render when the window asks (`request`), then write the render's centre and the
/// picture's name to the window (`rendered`); otherwise the camera stays off.
fn update(
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    image: Option<Res<MinimapImage>>,
    runtime: Option<ResMut<nvf_play::tiles::runtime_images::RuntimeImages>>,
    mut camera: Query<(&mut Camera, &mut Transform, &mut GlobalTransform, &mut Projection), With<MinimapCamera>>,
) {
    let Ok((mut cam, mut t, mut gt, mut proj)) = camera.single_mut() else { return };
    let rt = script.as_deref_mut().and_then(|s| s.rt.as_mut());
    let (Some(rt), Some(image), Some(mut runtime)) = (rt, image, runtime) else {
        cam.is_active = false;
        return;
    };
    let Some(r) = request(&rt.game.ui_tree) else {
        cam.is_active = false;
        return;
    };
    let (place, view) = camera_for(&r);
    *t = place;
    *gt = GlobalTransform::from(place);
    *proj = Projection::Orthographic(view);
    if !cam.is_active {
        info!("modern minimap: map picture rendered round ({:.0}, {:.0}), {:.0} units each way", r.centre[0], r.centre[1], r.half);
    }
    cam.is_active = true;
    let name = publish_image(&mut runtime, &image);
    rendered(&mut rt.game.ui_tree, &r, &name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn the_camera_looks_down_with_north_up_over_the_whole_frustum() {
        let r = Request { centre: [1000.0, -500.0, 300.0], half: 2048.0, offset: 10000.0 };
        let (t, p) = camera_for(&r);
        assert_eq!(t.translation, Vec3::new(1000.0, 10300.0, 500.0));
        assert!(t.forward().as_vec3().abs_diff_eq(Vec3::NEG_Y, 1e-5));
        assert!(t.up().as_vec3().abs_diff_eq(Vec3::NEG_Z, 1e-5), "north (game +y) is Bevy -z");
        assert!(matches!(p.scaling_mode, ScalingMode::Fixed { width, height } if width == 4096.0 && height == 4096.0));
        assert!(p.far >= 10300.0);
    }

    #[test]
    fn parse_texture_size_handles_bom_crlf_comments_sections_and_case() {
        let mut bom_crlf = b"\xEF\xBB\xBF[gEnErAl] ; section comment\r\n; full line comment\r\n# hash comment\r\n  TEXTURESIZE = 512 ; inline comment\r\n[Other]\r\ntextureSize=2048\r\n".to_vec();
        assert_eq!(parse_texture_size(&bom_crlf), IniTextureSize::Value(512));

        // Re-entering [General] overrides earlier value.
        bom_crlf.extend_from_slice(b"[General]\r\ntextureSize=2048 # trailing\r\n");
        assert_eq!(parse_texture_size(&bom_crlf), IniTextureSize::Value(2048));

        // Key outside [General] is ignored -> Absent.
        assert_eq!(parse_texture_size(b"textureSize=512\n[Other]\ntextureSize=256\n"), IniTextureSize::Absent);
        assert_eq!(parse_texture_size(b"[General]\notherKey=1\n"), IniTextureSize::Absent);
    }

    #[test]
    fn parse_texture_size_sanitizes_and_clamps_power_of_two() {
        let parse_val = |v: &str| parse_texture_size(format!("[General]\ntextureSize={v}\n").as_bytes());
        assert_eq!(parse_val("256"), IniTextureSize::Value(256));
        assert_eq!(parse_val("512"), IniTextureSize::Value(512));
        assert_eq!(parse_val("512.0"), IniTextureSize::Value(512));
        assert_eq!(parse_val("1024"), IniTextureSize::Value(1024));
        assert_eq!(parse_val("2048"), IniTextureSize::Value(2048));
        // Clamp below MIN_SIZE and above MAX_SIZE.
        assert_eq!(parse_val("1"), IniTextureSize::Value(256));
        assert_eq!(parse_val("64"), IniTextureSize::Value(256));
        assert_eq!(parse_val("4096"), IniTextureSize::Value(2048));
        assert_eq!(parse_val("1e30"), IniTextureSize::Value(2048));
        // Non-power-of-two snaps to nearest power of two (ties round up).
        assert_eq!(parse_val("300"), IniTextureSize::Value(256));
        assert_eq!(parse_val("384"), IniTextureSize::Value(512));
        assert_eq!(parse_val("400"), IniTextureSize::Value(512));
        assert_eq!(parse_val("700"), IniTextureSize::Value(512));
        assert_eq!(parse_val("800"), IniTextureSize::Value(1024));
        assert_eq!(parse_val("1500"), IniTextureSize::Value(1024));
        assert_eq!(parse_val("1600"), IniTextureSize::Value(2048));
    }

    #[test]
    fn parse_texture_size_rejects_invalid_non_ascii_and_non_finite() {
        for bad in ["NaN", "inf", "-inf", "0", "-512", "0.5", "", "abc", "512px", "日本語", "512🔥"] {
            let ini = format!("[General]\ntextureSize={bad}\n");
            assert_eq!(parse_texture_size(ini.as_bytes()), IniTextureSize::Invalid, "expected Invalid for {bad:?}");
        }
        // Invalid UTF-8 bytes in value must not panic and must return Invalid.
        let bad_utf8_val = b"[General]\ntextureSize=\xFF\xFE\n";
        assert_eq!(parse_texture_size(bad_utf8_val), IniTextureSize::Invalid);
        // Invalid UTF-8 / non-ASCII bytes in comments or unrelated lines must not break valid key.
        let bad_utf8_comment = b"[General]\n; \xFF\xFE \xE2\x9C\x93\nother=\x80\x81\ntextureSize=512\n";
        assert_eq!(parse_texture_size(bad_utf8_comment), IniTextureSize::Value(512));
    }

    #[test]
    fn setup_and_publish_wire_ini_texture_size_end_to_end_via_vfs() {
        let base = std::env::temp_dir().join(format!("nvf-minimap-ini-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let modded = base.join("modded");
        let vanilla = base.join("vanilla");
        std::fs::create_dir_all(modded.join("nvse/plugins")).unwrap();
        std::fs::create_dir_all(&vanilla).unwrap();
        // Deliberately lowercase path to verify case-insensitive VFS lookup.
        std::fs::write(modded.join("nvse/plugins/modernminimap.ini"), b"[General]\r\n; comment\r\ntextureSize=512\r\n").unwrap();

        // Modded VFS with textureSize=512 drives both Image extent and RuntimeImages size.
        let modded_data = nvf_assets::GameData::new(Arc::new(fnv_vfs::Vfs::open(&modded, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .init_resource::<nvf_play::tiles::runtime_images::RuntimeImages>()
            .insert_resource(modded_data)
            .add_systems(Startup, setup);
        app.update();
        let (handle, size) = {
            let img_res = app.world().resource::<MinimapImage>();
            (img_res.handle.clone(), img_res.size)
        };
        assert_eq!(size, 512);
        let extent = app.world().resource::<Assets<Image>>().get(&handle).expect("allocated image").texture_descriptor.size;
        assert_eq!((extent.width, extent.height), (512, 512));
        let published = {
            let mut runtime = app.world_mut().resource_mut::<nvf_play::tiles::runtime_images::RuntimeImages>();
            publish_image(&mut runtime, &MinimapImage { handle: handle.clone(), size })
        };
        let entry = app.world().resource::<nvf_play::tiles::runtime_images::RuntimeImages>().get(&published).expect("runtime entry");
        assert_eq!(entry.size, Vec2::splat(512.0));
        assert_eq!(entry.image, handle);

        // Vanilla VFS (absent INI) keeps default 1024x1024 unchanged.
        let vanilla_data = nvf_assets::GameData::new(Arc::new(fnv_vfs::Vfs::open(&vanilla, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
        let mut vanilla_app = App::new();
        vanilla_app
            .init_resource::<Assets<Image>>()
            .insert_resource(vanilla_data)
            .add_systems(Startup, setup);
        vanilla_app.update();
        let vanilla_res = vanilla_app.world().resource::<MinimapImage>();
        assert_eq!(vanilla_res.size, DEFAULT_SIZE);
        let vanilla_extent = vanilla_app.world().resource::<Assets<Image>>().get(&vanilla_res.handle).expect("vanilla image").texture_descriptor.size;
        assert_eq!((vanilla_extent.width, vanilla_extent.height), (1024, 1024));

        let _ = std::fs::remove_dir_all(&base);
    }
}
