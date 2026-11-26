use super::*;

fn texel(bytes: &[u8], level: u32, face: usize, row: u32, col: u32) -> [f32; 4] {
    let side = level_side(level);
    let at = (((face as u32 * side + row) * side + col) * TEXEL_BYTES) as usize;
    let mut out = [0.0; 4];
    for (i, v) in out.iter_mut().enumerate() {
        *v = half::f16::from_le_bytes([bytes[at + 2 * i], bytes[at + 2 * i + 1]]).to_f32();
    }
    out
}

/// The axis a direction points along most, as a world face name.
fn dominant(d: Vec3) -> &'static str {
    let a = d.abs();
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { "+X" } else { "-X" }
    } else if a.y >= a.z {
        if d.y > 0.0 { "+Y" } else { "-Y" }
    } else if d.z > 0.0 {
        "+Z"
    } else {
        "-Z"
    }
}

#[test]
fn face_centres_point_along_their_world_axes_with_z_negated() {
    // Layers +X, -X, +Y, -Y keep their axes; the +Z layer stands for world -Z and the -Z
    // layer for world +Z (Bevy negates z before sampling).
    let want = ["+X", "-X", "+Y", "-Y", "-Z", "+Z"];
    for (face, name) in want.iter().enumerate() {
        assert_eq!(dominant(world_direction(face, 0.0, 0.0)), *name, "layer {face}");
    }
    // Every texel of a layer points along that layer's axis.
    for (face, name) in want.iter().enumerate() {
        for (s, t) in [(-0.9, -0.9), (0.9, -0.9), (-0.9, 0.9), (0.9, 0.9)] {
            assert_eq!(dominant(world_direction(face, s, t)), *name);
        }
    }
}

#[test]
fn cube_directions_follow_the_face_table() {
    // s to the right, t down the rows of each layer.
    assert_eq!(cube_direction(0, 1.0, 1.0), Vec3::new(1.0, -1.0, -1.0));
    assert_eq!(cube_direction(2, 1.0, 1.0), Vec3::new(1.0, 1.0, 1.0));
    assert_eq!(cube_direction(3, 1.0, 1.0), Vec3::new(1.0, -1.0, -1.0));
    assert_eq!(cube_direction(4, 1.0, 1.0), Vec3::new(1.0, -1.0, 1.0));
    assert_eq!(cube_direction(5, 1.0, 1.0), Vec3::new(-1.0, -1.0, -1.0));
}

#[test]
fn levels_have_six_faces_of_halving_sides() {
    let levels = specular_levels(|_, _| Vec3::splat(0.5));
    assert_eq!(levels.len(), SPECULAR_LEVELS as usize);
    for (l, bytes) in levels.iter().enumerate() {
        assert_eq!(bytes.len(), level_bytes(l as u32));
    }
    assert_eq!(level_side(0), 32);
    assert_eq!(level_side(5), 1);
    assert_eq!(level_bytes(5), 48);
}

#[test]
fn texels_hold_the_closure_by_world_direction_and_level() {
    let colour = |d: Vec3, level: u32| {
        let base = match dominant(d) {
            "+Z" => Vec3::new(0.1, 0.1, 0.6),
            "-Z" => Vec3::new(0.6, 0.6, 0.1),
            _ => Vec3::splat(0.2),
        };
        base + Vec3::splat(level as f32)
    };
    let levels = specular_levels(colour);
    // World +Z lives in layer 5, world -Z in layer 4, at every level.
    for level in 0..SPECULAR_LEVELS {
        let side = level_side(level);
        let mid = side / 2;
        let plus_z = texel(&levels[level as usize], level, 5, mid.min(side - 1), mid.min(side - 1));
        let minus_z = texel(&levels[level as usize], level, 4, 0, 0);
        let l = level as f32;
        assert!((plus_z[2] - (0.6 + l)).abs() < 0.01 && (plus_z[0] - (0.1 + l)).abs() < 0.01, "level {level}: {plus_z:?}");
        assert!((minus_z[0] - (0.6 + l)).abs() < 0.01 && (minus_z[2] - (0.1 + l)).abs() < 0.01, "level {level}: {minus_z:?}");
        assert_eq!(plus_z[3], 1.0);
    }
}

#[test]
fn negative_and_non_finite_values_are_written_as_zero() {
    let levels = specular_levels(|_, _| Vec3::new(-1.0, f32::NAN, f32::INFINITY));
    let t = texel(&levels[0], 0, 0, 0, 0);
    assert_eq!(t, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn intensity_is_the_light_unit() {
    // At the default exposure E, texel x intensity x E is the texel (module notes, "Units").
    let e = bevy::camera::Exposure::default().exposure();
    assert!((light_unit() * e - 1.0).abs() < 1e-6);
}

#[test]
fn an_upload_waits_for_the_texture_and_is_written_once_per_fill() {
    let mut images = Assets::<Image>::default();
    let mut cube = ReflectionCube::new(&mut images);
    let mut up = Upload::<u32>::default();
    // Nothing before the first fill.
    up.receive(&cube);
    assert!(up.levels_to_write(Some(1)).is_none());
    assert!(!cube.ready());
    cube.fill(|_, _| Vec3::splat(0.25));
    up.receive(&cube);
    // Pending until the texture exists.
    assert!(up.levels_to_write(None).is_none());
    let levels = up.levels_to_write(Some(1)).expect("written once the texture exists");
    assert_eq!(levels.len(), SPECULAR_LEVELS as usize);
    assert!(!cube.ready(), "not ready before the write is recorded");
    up.written(1);
    assert!(cube.ready());
    // The same generation again: nothing more to write.
    up.receive(&cube);
    assert!(up.levels_to_write(Some(1)).is_none());
    // A new fill: written again, and the cube is not ready until it is.
    cube.fill(|_, _| Vec3::splat(0.5));
    assert!(!cube.ready());
    up.receive(&cube);
    assert!(up.levels_to_write(Some(1)).is_some());
    up.written(1);
    assert!(cube.ready());
}

/// Review finding 3: the GPU image made again under the same handle is a new, empty texture;
/// the retained levels are written into it with no new fill, once.
#[test]
fn a_replaced_texture_gets_the_retained_levels_again() {
    let mut images = Assets::<Image>::default();
    let mut cube = ReflectionCube::new(&mut images);
    let mut up = Upload::<u32>::default();
    cube.fill(|_, _| Vec3::splat(0.25));
    up.receive(&cube);
    let first = up.levels_to_write(Some(1)).unwrap();
    up.written(1);
    // The same texture: nothing.
    up.receive(&cube);
    assert!(up.levels_to_write(Some(1)).is_none());
    // Gone for a frame (not yet prepared again): no ready texture, and no content lost.
    assert!(up.levels_to_write(None).is_none());
    assert!(!cube.ready());
    // Another texture under the same handle: the same levels again, then nothing more.
    up.receive(&cube);
    let again = up.levels_to_write(Some(2)).expect("the retained levels into the new texture");
    assert!(Arc::ptr_eq(&first, &again));
    up.written(2);
    assert!(up.levels_to_write(Some(2)).is_none());
    assert!(cube.ready());
}

#[test]
fn an_incompatible_replacement_does_not_lose_the_pending_levels() {
    let mut images = Assets::<Image>::default();
    let mut cube = ReflectionCube::new(&mut images);
    let mut up = Upload::<u32>::default();
    cube.fill(|_, _| Vec3::splat(0.25));
    up.receive(&cube);
    let first = up.levels_to_write(Some(1)).expect("the first texture");
    up.written(1);
    assert!(cube.ready());

    // The render system finds a replacement, but rejects its descriptor instead of writing.
    assert!(up.levels_to_write(Some(2)).is_some());
    assert!(up.reject(2));
    assert!(!up.reject(2), "report the incompatible texture only once");
    assert!(!cube.ready(), "the previous texture's upload does not make the replacement ready");

    // A valid replacement gets the same fill, even though the producer has not filled again.
    up.receive(&cube);
    let retained = up.levels_to_write(Some(3)).expect("the retained levels into the valid texture");
    assert!(Arc::ptr_eq(&first, &retained));
    up.written(3);
    assert!(cube.ready());
    assert!(up.levels_to_write(Some(3)).is_none());
}

#[test]
fn the_pair_is_two_cube_views_with_a_linear_mip_sampler() {
    let mut images = Assets::<Image>::default();
    let cube = ReflectionCube::new(&mut images);
    let specular = images.get(&cube.specular).unwrap();
    assert_eq!(specular.texture_descriptor.mip_level_count, SPECULAR_LEVELS);
    assert_eq!(specular.texture_descriptor.size.depth_or_array_layers, 6);
    assert_eq!(specular.texture_descriptor.format, TextureFormat::Rgba16Float);
    assert!(specular.texture_descriptor.usage.contains(TextureUsages::COPY_DST));
    assert!(specular.data.is_none(), "written by the render world, not prepared from data");
    let diffuse = images.get(&cube.diffuse).unwrap();
    assert_eq!(diffuse.texture_descriptor.size, Extent3d { width: 1, height: 1, depth_or_array_layers: 6 });
    assert!(diffuse.data.as_ref().is_some_and(|d| d.iter().all(|&b| b == 0)), "black");
    for image in [specular, diffuse] {
        assert_eq!(image.texture_view_descriptor.as_ref().and_then(|v| v.dimension), Some(TextureViewDimension::Cube));
        let ImageSampler::Descriptor(s) = &image.sampler else { panic!("explicit sampler") };
        assert_eq!(s.mipmap_filter, ImageFilterMode::Linear);
    }
    let light = cube.light();
    assert_eq!(light.intensity, light_unit());
    assert_eq!(light.rotation, Quat::IDENTITY);
}

#[test]
fn gate_state_round_trips() {
    // The one global: restore Pending at the end for other tests in this process.
    for s in [GateState::Installed, GateState::Failed, GateState::Pending] {
        set_gate_state(s);
        assert_eq!(gate_state(), s);
    }
}
