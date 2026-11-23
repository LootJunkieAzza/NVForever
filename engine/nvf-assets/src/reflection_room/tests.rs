use super::*;

fn near(a: Vec3, b: Vec3, tol: f32) -> bool {
    (a - b).abs().max_element() <= tol
}

/// A room 400 wide, 300 high, 600 deep around the origin, the eye at its centre, a warm grey
/// ambient, no directional light, no lamps, no fog.
fn empty() -> RoomScene {
    RoomScene {
        eye: Vec3::ZERO,
        room: RoomBox::from_min_max(Vec3::new(-200.0, -150.0, -300.0), Vec3::new(200.0, 150.0, 300.0)),
        albedo: Albedo::default(),
        ambient: Vec3::new(0.4, 0.35, 0.3),
        directional: Vec3::ZERO,
        toward_light: Vec3::Y,
        lamps: Vec::new(),
        fog: None,
    }
}

fn lamp(position: Vec3, radius: f32) -> Lamp {
    Lamp { position, colour: Vec3::new(1.0, 0.8, 0.6), radius, spot: None }
}

#[test]
fn the_attenuation_is_the_games() {
    assert_eq!(attenuation(0.0, 100.0), 1.0);
    assert!((attenuation(50.0 * 50.0, 100.0) - 0.75).abs() < 1e-6);
    assert_eq!(attenuation(100.0 * 100.0, 100.0), 0.0, "0 at the radius");
    assert_eq!(attenuation(150.0 * 150.0, 100.0), 0.0, "and beyond");
    assert_eq!(attenuation(1.0, 0.0), 0.0, "no radius, no light");
}

#[test]
fn a_ray_leaves_the_box_through_the_face_it_points_at() {
    let room = empty().room;
    let up = room.exit(Vec3::ZERO, Vec3::Y);
    assert_eq!(up.surface, Surface::Ceiling);
    assert!((up.distance - 150.0).abs() < 1e-3 && near(up.normal, Vec3::NEG_Y, 1e-6));
    let down = room.exit(Vec3::ZERO, Vec3::NEG_Y);
    assert_eq!(down.surface, Surface::Floor);
    assert!(near(down.normal, Vec3::Y, 1e-6));
    let side = room.exit(Vec3::ZERO, Vec3::NEG_Z);
    assert_eq!(side.surface, Surface::Wall);
    assert!((side.distance - 300.0).abs() < 1e-3 && near(side.point, Vec3::new(0.0, 0.0, -300.0), 1e-2));
    // A box turned 30 degrees about the vertical: its walls turn with it.
    let turned = RoomBox { centre: Vec3::ZERO, axes: Mat3::from_rotation_y(30f32.to_radians()), half: Vec3::new(200.0, 150.0, 300.0) };
    let x = turned.axes.col(0);
    let hit = turned.exit(Vec3::ZERO, x);
    assert!((hit.distance - 200.0).abs() < 1e-2 && near(hit.normal, -x, 1e-5));
    // An eye outside is taken just inside.
    let outside = room.exit(Vec3::new(0.0, 500.0, 0.0), Vec3::NEG_Y);
    assert!((outside.distance - 300.0).abs() < 0.1);
}

/// The stated rule: an empty room (no lamps, no directional light) is each surface's albedo
/// under the ambient by the display-value sum, so its floor is stage 1's ground (the same
/// albedo, the same rule) and its walls and ceiling the same rule with their albedos.
#[test]
fn an_empty_room_is_the_ambient_on_each_surface() {
    let s = empty();
    let a = Albedo::default();
    assert!(near(a.floor, albedo_linear([0.33, 0.30, 0.25]), 1e-7), "the floor is stage 1's ground albedo");
    assert!(near(s.radiance(Vec3::NEG_Y), lit_texel(a.floor, s.ambient), 1e-7));
    assert!(near(s.radiance(Vec3::Y), lit_texel(a.ceiling, s.ambient), 1e-7));
    for d in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z, Vec3::new(1.0, 0.2, 0.5).normalize()] {
        assert!(near(s.radiance(d), lit_texel(a.wall, s.ambient), 1e-7), "{d}");
    }
    // Every level of a room uniform per surface lies between the surfaces' colours.
    let cube = FilteredCube::new(|d| s.radiance(d));
    let lo = lit_texel(a.floor, s.ambient).min(lit_texel(a.wall, s.ambient));
    let hi = lit_texel(a.ceiling, s.ambient).max(lit_texel(a.wall, s.ambient));
    for level in 0..SPECULAR_LEVELS {
        for d in [Vec3::Y, Vec3::NEG_Y, Vec3::X] {
            let c = cube.colour(d, level);
            assert!(c.cmpge(lo - 1e-6).all() && c.cmple(hi + 1e-6).all(), "level {level} {d}: {c:?}");
        }
    }
}

/// One lamp near the +X wall brightens that wall over the far one, and the brightest texel
/// at level 0 is its lobe, toward the lamp.
#[test]
fn a_lamp_brightens_the_wall_nearest_it_and_its_lobe_points_at_it() {
    let mut s = empty();
    let at = Vec3::new(150.0, 60.0, 40.0);
    s.lamps.push(lamp(at, 400.0));
    let empty_wall = empty().radiance(Vec3::X);
    let near_wall = s.radiance(Vec3::new(1.0, -0.3, 0.0).normalize());
    let far_wall = s.radiance(Vec3::new(-1.0, -0.3, 0.0).normalize());
    assert!(near_wall.x > far_wall.x && far_wall.x >= empty_wall.x - 1e-7, "{near_wall:?} {far_wall:?} {empty_wall:?}");
    // The lamp's lobe: the brightest texel of level 0 is within a texel of the lamp.
    let cube = FilteredCube::new(|d| s.radiance(d));
    let side = level_side(0);
    let mut best = (0.0f32, Vec3::ZERO);
    for face in 0..6 {
        for row in 0..side {
            for col in 0..side {
                let (u, v) = centre(side, row, col);
                let d = world_direction(face, u, v);
                let c = cube.colour(d, 0).x;
                if c > best.0 {
                    best = (c, d);
                }
            }
        }
    }
    let toward = at.normalize();
    assert!(best.1.dot(toward) > 4f32.to_radians().cos(), "brightest {:?} against the lamp {toward:?}", best.1);
    // Its lobe stands above the lit wall behind it.
    let behind = s.radiance(toward);
    let off = s.radiance((Mat3::from_rotation_y(0.2) * toward).normalize());
    assert!(behind.x > off.x + 0.5 * LAMP_LOBE_PEAK * display_texel(s.lamps[0].colour).x, "{behind:?} {off:?}");
}

/// A lamp whose sphere does not reach the box adds nothing anywhere: no wall light (the
/// attenuation is 0 beyond the radius) and no lobe (it is outside the box).
#[test]
fn a_lamp_outside_its_radius_adds_nothing() {
    let l = lamp(Vec3::new(2000.0, 0.0, 0.0), 500.0);
    assert_eq!(l.light_at(Vec3::new(200.0, 0.0, 0.0), Vec3::NEG_X), Vec3::ZERO);
    assert!(!l.reaches(&empty().room));
    let mut s = empty();
    s.lamps.push(l);
    let (with, without) = (FilteredCube::new(|d| s.radiance(d)), FilteredCube::new(|d| empty().radiance(d)));
    for level in 0..SPECULAR_LEVELS {
        for d in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::new(0.3, -0.4, 0.8).normalize()] {
            assert_eq!(with.colour(d, level), without.colour(d, level), "level {level} {d}");
        }
    }
    // A lamp facing away from a surface lights it not at all (N.L), one behind its cone
    // neither.
    let behind = lamp(Vec3::new(0.0, 0.0, 10.0), 500.0);
    assert_eq!(behind.light_at(Vec3::ZERO, Vec3::NEG_Z), Vec3::ZERO);
    let spot = Lamp { spot: Some(Spot::new(Vec3::NEG_Y, 0.3, 0.4)), ..lamp(Vec3::new(0.0, 100.0, 0.0), 500.0) };
    assert!(spot.light_at(Vec3::ZERO, Vec3::Y).x > 0.0, "under the spot");
    assert_eq!(spot.light_at(Vec3::new(300.0, 100.0, 0.0), Vec3::NEG_X), Vec3::ZERO, "beside it, outside the cone");
}

/// The levels approach the mean: each level's solid-angle-weighted spread about the mean is
/// below the level before up to level 4, and every level keeps level 0's mean within 1 %.
/// The last level holds one texel a face, its kernel's values at the six face centres, which
/// are the extremes of a broad blur (straight at the floor, the ceiling, a wall); its
/// spread is held below level 2's only (a port of this filter gave 0.0059, 0.0056, 0.0028,
/// 0.0014, 0.0009, 0.0013 for this scene).
#[test]
fn levels_approach_the_mean() {
    let mut s = empty();
    s.lamps.push(lamp(Vec3::new(150.0, 60.0, 40.0), 400.0));
    s.directional = Vec3::new(0.2, 0.2, 0.25);
    s.toward_light = Vec3::new(0.3, 0.8, 0.2).normalize();
    let cube = FilteredCube::new(|d| s.radiance(d));
    let mean0 = cube.mean(0);
    let mut spread = Vec::new();
    for level in 0..SPECULAR_LEVELS {
        let t = texels(level);
        let total: f32 = t.iter().map(|(_, w)| w).sum();
        let mean = cube.mean(level);
        assert!(near(mean, mean0, 0.01 * mean0.max_element()), "level {level}: mean {mean:?} against {mean0:?}");
        let var: f32 = t.iter().zip(&cube.levels[level as usize]).map(|((_, w), c)| (*c - mean).length_squared() * w).sum::<f32>() / total;
        spread.push(var);
    }
    assert!(spread[..5].windows(2).all(|w| w[1] < w[0]), "{spread:?}");
    assert!(spread[5] < spread[2], "{spread:?}");
    // Level 1's lobe is narrow enough to be a box average alone, the others are convolved.
    assert!(kernel_half_angle(level_power(1)) < std::f32::consts::FRAC_PI_2 / level_side(1) as f32);
    for level in 2..SPECULAR_LEVELS {
        assert!(kernel_half_angle(level_power(level)) >= std::f32::consts::FRAC_PI_2 / level_side(level) as f32, "level {level}");
    }
    assert_eq!(level_power(SPECULAR_LEVELS - 1), 0.0, "a cosine lobe at the last level");
}

#[test]
fn the_coarsest_level_filters_across_cube_faces() {
    let cube = FilteredCube::new(|d| {
        if d.x > d.y.abs().max(d.z.abs()) { Vec3::X } else { Vec3::ZERO }
    });
    let last = SPECULAR_LEVELS - 1;
    let facing = cube.colour(Vec3::X, last).x;
    let adjacent = cube.colour(Vec3::Y, last).x;
    assert!(adjacent > 0.0, "the adjacent face receives light from +X");
    assert!(adjacent < facing && facing < 1.0, "facing {facing}, adjacent {adjacent}");
    assert_eq!(cube.colour(Vec3::NEG_X, last), Vec3::ZERO, "the cosine lobe excludes the opposite hemisphere");
}

#[test]
fn the_same_scene_gives_the_same_cube() {
    let mut s = empty();
    s.lamps.push(lamp(Vec3::new(150.0, 60.0, 40.0), 400.0));
    s.lamps.push(Lamp { spot: Some(Spot::new(Vec3::NEG_Y, 0.4, 0.6)), ..lamp(Vec3::new(-100.0, 140.0, -200.0), 300.0) });
    s.fog = Some(Fog { texel: Vec3::new(0.02, 0.03, 0.04), near: 100.0, far: 2000.0, power: 1.5 });
    let (a, b) = (FilteredCube::new(|d| s.radiance(d)), FilteredCube::new(|d| s.radiance(d)));
    assert_eq!(a.levels, b.levels);
}

/// The texel a direction falls in is the one whose centre `world_direction` gives, at every
/// level.
#[test]
fn texel_of_inverts_the_cube_layout() {
    for level in 0..SPECULAR_LEVELS {
        let side = level_side(level);
        for face in 0..6 {
            for row in 0..side {
                for col in 0..side {
                    let (u, v) = centre(side, row, col);
                    assert_eq!(texel_of(world_direction(face, u, v), side), (face, row, col), "level {level}");
                }
            }
        }
    }
}

/// The fog mixes walls toward its colour by distance; none before its near distance.
#[test]
fn fog_mixes_far_walls_toward_its_colour() {
    let mut s = empty();
    let fog = Fog { texel: Vec3::new(0.5, 0.0, 0.0), near: 160.0, far: 400.0, power: 1.0 };
    s.fog = Some(fog);
    assert!(near(s.radiance(Vec3::Y), empty().radiance(Vec3::Y), 1e-7), "the ceiling, 150 away, is before the fog");
    let wall = s.radiance(Vec3::NEG_Z);
    let want = empty().radiance(Vec3::NEG_Z).lerp(fog.texel, fog.share(300.0));
    assert!(near(wall, want, 1e-6) && fog.share(300.0) > 0.5);
}

/// Outdoors the lamps add lit ground below the horizon and nothing above it but their
/// lobes; without lamps nothing.
#[test]
fn lamps_light_the_ground_outdoors() {
    let base = Vec3::splat(0.08);
    let mut g = GroundLamps { eye: Vec3::new(0.0, 120.0, 0.0), ground: 0.0, albedo: albedo_linear(FLOOR_ALBEDO), base, lamps: Vec::new() };
    assert_eq!(g.added(Vec3::NEG_Y), Vec3::ZERO);
    g.lamps.push(lamp(Vec3::new(0.0, 300.0, -200.0), 600.0));
    let below = g.added(Vec3::new(0.0, -0.6, -0.8).normalize());
    assert!(below.x > 0.0, "{below:?}");
    let far_below = g.added(Vec3::new(0.0, -0.05, 1.0).normalize());
    assert!(far_below.x < below.x, "ground beyond the lamp's reach is not lit by it");
    let sky = g.added(Vec3::new(0.0, 0.6, 0.8).normalize());
    assert!(sky.x < 1e-6, "above the horizon, away from the lamp: nothing");
    let toward = (g.lamps[0].position - g.eye).normalize();
    assert!(g.added(toward).x > 0.5 * LAMP_LOBE_PEAK, "the lamp's lobe");
}
