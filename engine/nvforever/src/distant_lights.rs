//! Distant lights: placed lights beyond the loaded cells drawn as small glows at night,
//! and the real lights of a streamed cell faded in and out with the cell.
//!
//! Why [observed: owner's playtest 2026-11-06, `light-popin-road-lamp.webp`]: lighting
//! mods (ELECTRO-CITY - Highways and Byways and CompletedWorkorders) place lamp posts and
//! their LIGH references along the roads with no distant LOD, so at night a road showed
//! nothing beyond the loaded cells and each lamp's light switched on at full strength as
//! its cell loaded, about two cells ahead. The game itself draws no light of an unloaded
//! cell [hypothesis: its distant LOD holds meshes only, world.md §7]; what follows is
//! NVForever's [design], asked for by the owner (overnight queue 2026-11-06, item 16).
//!
//! - **Glows** [design]: every enabled LIGH reference of an exterior cell within
//!   [`RANGE_CELLS`] cells of the camera that its cell does not draw (not loaded, or
//!   still fading in) is a camera-facing additive sprite in the light's colour, about
//!   [`PIXEL_ANGLE`] wide on screen however far, never under [`MIN_SIZE`] units. Its
//!   strength is [`lamps_on`] (0 while the sun is up, 1 just after sunset) × the part
//!   of its cell not yet drawn (1 − `Streamer::cell_level`) × a fade over the last fifth of the range, so a
//!   lamp's glow gives way to the lamp and its light as the cell fades in, and glows
//!   come up at dusk. Lights the game would not light are left out by
//!   `lights::spawn_light`'s rules (Negative, Off by Default, no radius, black). The
//!   light's model colour (black LIGH colours) is not read: such lights get no glow.
//!   Enable state is read once (initially disabled, enable parents), not followed
//!   when a script changes it later.
//! - **Lamp light** [design]: the glows within [`LIT_CELLS`] cells of the camera's cell
//!   also light their surroundings with the light the cell would draw (the same colour,
//!   brightness, range and spot cone as `lights::spawn_light`, no shadows), at the
//!   glow's strength without the range fade, faded out over the last two cells of
//!   [`LIT_CELLS`]. Without it the distant lamp posts (object LOD, the owner's lamp LOD
//!   blocks) stood unlit in the night ambient under their glows and could not be seen
//!   (owner's playtest 2026-11-06, `lamp-lod-missing-night.webp`, overnight queue item
//!   35). As the cell fades in, its own lights come up by the cell's level while these
//!   go down by the same amount, so the light under a lamp does not pop.
//! - **Cell lights** [design]: [`CellLight`] holds a streamed light's full brightness;
//!   [`fade_cell_lights`] scales it by its cell's draw level, so it comes up over the
//!   cell's fade (`--cell-fade`) together with the cell's meshes instead of at once.

use crate::lights::{display_color, lumens, CellLight, LightMotion, MovingLight, FLAME_REACH};
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use fnv_world::lighting::{light_data, light_flags, placed_light_radius, ref_xrds};
use fnv_world::placed::{base_object, placement};
use fnv_world::{cell_of, CELL_SIZE};
use std::collections::HashMap;

/// Cells around the camera's whose lights glow [design: about the reach of the
/// finest distant terrain, 12 × 4,096 ≈ 49,000 units].
pub const RANGE_CELLS: i32 = 12;
/// Screen size of a glow as an angle (radians) [design: a few pixels at 1080p].
const PIXEL_ANGLE: f32 = 0.006;
/// A glow's least size in game units [design].
const MIN_SIZE: f32 = 48.0;
/// Glow brightness per unit of light colour × fade [design, by eye against the lit
/// lamps of a loaded cell; above 1 so the bloom picks it up].
const STRENGTH: f32 = 2.5;
/// Cells around the camera's whose glows also light their surroundings (see the module
/// notes) [design: the lamp posts out to about where a post is still a few pixels wide;
/// a light per lamp costs more than a glow].
pub const LIT_CELLS: i32 = 8;
/// Cells over which the lamp light fades out at the end of [`LIT_CELLS`] [design].
const LIT_FADE_CELLS: f32 = 2.0;
/// Steps the strength is rounded to before materials are touched again.
const LEVELS: f32 = 32.0;

impl DistantLights {
    /// Glows drawn (`enabled`) or not.
    pub fn new(enabled: bool) -> DistantLights {
        DistantLights { enabled, ..default() }
    }
}

/// One distant light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    /// Position in game units.
    pub at: Vec3,
    /// Display colour (0..1) × the light's fade.
    pub color: [f32; 3],
    /// The light the cell would draw for it.
    pub lamp: Lamp,
}

/// The light a distant lamp sheds, as `lights::spawn_light` would draw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    /// Display colour (0..1) of the light record.
    pub color: [f32; 3],
    /// Full brightness (Bevy lumens).
    pub intensity: f32,
    /// Range (game units).
    pub range: f32,
    /// Spot lights: the cone's half angle (radians).
    pub spot: Option<f32>,
}

/// A glow's entity, its material and the light it stands for.
type DrawnGlow = (Entity, Handle<StandardMaterial>, Glow);

/// A streamed light's full brightness and the light components it sets.
type CellLightParts<'a> = (&'a CellLight, Option<&'a mut PointLight>, Option<&'a mut SpotLight>, Option<&'a mut MovingLight>);

/// The glows drawn now.
#[derive(Resource, Default)]
pub struct DistantLights {
    /// Off with `--no-distant-lights`.
    pub enabled: bool,
    /// The worldspace and centre cell the drawn set was chosen for.
    chosen: Option<(u32, (i32, i32))>,
    /// Each cell's lights, read once per worldspace.
    read: HashMap<(i32, i32), Vec<Glow>>,
    read_world: u32,
    /// Each cell's glow entities (children of the world root) and their materials.
    drawn: HashMap<(i32, i32), Vec<DrawnGlow>>,
    /// The strength step each cell was last drawn at.
    applied: HashMap<(i32, i32), u32>,
    /// Each cell's lamp lights (children of the world root) and their full brightness.
    lit: HashMap<(i32, i32), Vec<(Entity, f32)>>,
    quad: Option<Handle<Mesh>>,
    texture: Option<Handle<Image>>,
}

/// The glows of exterior cell `c`, by `lights::spawn_light`'s rules.
pub fn cell_glows(world: &WorldData, c: (i32, i32)) -> Vec<Glow> {
    let mut refs: Vec<u32> = world.grid_cell(c).map(|cell| cell.refs.clone()).unwrap_or_default();
    refs.extend(world.persistent_by_grid.get(&c).into_iter().flatten().copied());
    refs.iter()
        .filter_map(|&r| {
            let p = placement(world.lo, world.schema, r)?;
            let base = p.base.and_then(|b| base_object(world.lo, world.schema, b))?;
            if &base.sig != b"LIGH" || !world.enabled(&p, 0) {
                return None;
            }
            let l = light_data(world.lo, world.schema, base.form_id)?;
            let radius = placed_light_radius(l.radius, ref_xrds(world.lo, world.schema, r));
            if l.flags & (light_flags::NEGATIVE | light_flags::OFF_BY_DEFAULT) != 0 || radius <= 0.0 || l.color == [0; 3] {
                return None;
            }
            let fade = l.fade.clamp(0.0, 4.0);
            let flame = matches!(LightMotion::of(l.flags), Some(LightMotion::Flicker | LightMotion::FlickerSlow));
            let lamp = Lamp {
                color: l.color.map(|b| b as f32 / 255.0),
                intensity: lumens(l.fade.max(0.0) * p.scale, crate::sky::light_unit()),
                range: if flame { radius * FLAME_REACH } else { radius },
                spot: (l.flags & light_flags::SPOT != 0).then(|| (l.fov.clamp(1.0, 179.0) / 2.0).to_radians()),
            };
            Some(Glow { at: Vec3::from_array(p.position), color: l.color.map(|b| b as f32 / 255.0 * fade), lamp })
        })
        .collect()
}

/// Sun elevations (degrees) between which lamps come on: off with the sun at or above
/// the first, fully on at or below the second. The lamps are off by day (owner,
/// 2026-11-06, overnight queue items 16 and 34: glows showed at part strength all
/// morning and afternoon while this followed the 40° daylight ramp) [design].
const LAMPS_ON_DEG: (f32, f32) = (4.0, -4.0);

/// How far the lamps are on (0..1) for a sun elevation in degrees: 0 while the sun is
/// up, rising through sunset to 1 at [`LAMPS_ON_DEG`]`.1`.
pub fn lamps_on(sun_elevation: f32) -> f32 {
    let (a, b) = LAMPS_ON_DEG;
    let t = ((a - sun_elevation) / (a - b)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of its light a distant lamp sheds (0..1) at `d` horizontal game units from
/// the camera: all of it out to [`LIT_FADE_CELLS`] cells short of [`LIT_CELLS`], none at it.
pub fn lit_reach(d: f32) -> f32 {
    let (end, fade) = (LIT_CELLS as f32 * CELL_SIZE, LIT_FADE_CELLS * CELL_SIZE);
    (1.0 - (d - (end - fade)) / fade).clamp(0.0, 1.0)
}

/// A glow's strength (0..1): night × the part of its cell not drawn × the fade at the
/// end of the range (`d` horizontal distance from the camera, game units).
pub fn strength(night: f32, cell_level: f32, d: f32) -> f32 {
    let range = RANGE_CELLS as f32 * CELL_SIZE;
    let edge = 1.0 - ((d - 0.8 * range) / (0.2 * range)).clamp(0.0, 1.0);
    (night * (1.0 - cell_level) * edge).clamp(0.0, 1.0)
}

/// A soft round spot, white, alpha falling off from the centre.
fn glow_image() -> Image {
    const N: u32 = 64;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            let a = (1.0 - r).clamp(0.0, 1.0).powi(2);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    let mut img = Image::new(Extent3d { width: N, height: N, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    sky: Res<crate::sky::SkyState>,
    mut state: ResMut<DistantLights>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut glows: Query<&mut Transform, Without<DistantLamp>>,
    mut lamp_lights: Query<(Option<&mut PointLight>, Option<&mut SpotLight>), With<DistantLamp>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let state = &mut *state;
    let night = lamps_on(sky.sun_elevation());
    let off = !state.enabled || streamer.interior.is_some() || night <= 0.0;
    let eye = bevy_to_game(cam.translation());
    let centre = cell_of(eye.x, eye.y);
    let key = (world.world, centre);
    if off || state.read_world != world.world {
        for (_, list) in state.drawn.drain() {
            for (e, m, _) in list {
                commands.entity(e).despawn();
                materials.remove(&m);
            }
        }
        state.applied.clear();
        for (_, list) in state.lit.drain() {
            for (e, _) in list {
                commands.entity(e).despawn();
            }
        }
        state.chosen = None;
        if state.read_world != world.world {
            state.read.clear();
            state.read_world = world.world;
        }
        if off {
            return;
        }
    }
    if state.chosen != Some(key) {
        let want = |c: &(i32, i32)| (c.0 - centre.0).abs().max((c.1 - centre.1).abs()) <= RANGE_CELLS;
        let gone: Vec<(i32, i32)> = state.drawn.keys().filter(|c| !want(c)).copied().collect();
        for c in gone {
            for (e, m, _) in state.drawn.remove(&c).unwrap_or_default() {
                commands.entity(e).despawn();
                materials.remove(&m);
            }
            state.applied.remove(&c);
        }
        let quad = state.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
        let texture = state.texture.get_or_insert_with(|| images.add(glow_image())).clone();
        let (mut cells, mut added) = (0, 0);
        for x in centre.0 - RANGE_CELLS..=centre.0 + RANGE_CELLS {
            for y in centre.1 - RANGE_CELLS..=centre.1 + RANGE_CELLS {
                let c = (x, y);
                if state.drawn.contains_key(&c) {
                    continue;
                }
                let list = state.read.entry(c).or_insert_with(|| cell_glows(&world, c)).clone();
                let drawn: Vec<_> = list
                    .into_iter()
                    .map(|g| {
                        let m = materials.add(StandardMaterial {
                            base_color: Color::BLACK,
                            base_color_texture: Some(texture.clone()),
                            unlit: true,
                            alpha_mode: AlphaMode::Add,
                            fog_enabled: false,
                            ..default()
                        });
                        let e = commands
                            .spawn((Name::new("Distant light"), Mesh3d(quad.clone()), MeshMaterial3d(m.clone()), Transform::from_translation(g.at), Visibility::Hidden, ChildOf(root)))
                            .id();
                        (e, m, g)
                    })
                    .collect();
                cells += 1;
                added += drawn.len();
                state.drawn.insert(c, drawn);
            }
        }
        if cells > 0 {
            info!("distant lights: {added} glows in {cells} more cells around {centre:?} ({} cells drawn)", state.drawn.len());
        }
        // Lamp lights for the cells near the camera.
        let near = |c: &(i32, i32)| (c.0 - centre.0).abs().max((c.1 - centre.1).abs()) <= LIT_CELLS;
        let gone: Vec<(i32, i32)> = state.lit.keys().filter(|c| !near(c)).copied().collect();
        for c in gone {
            for (e, _) in state.lit.remove(&c).unwrap_or_default() {
                commands.entity(e).despawn();
            }
        }
        let mut lamps = 0;
        for x in centre.0 - LIT_CELLS..=centre.0 + LIT_CELLS {
            for y in centre.1 - LIT_CELLS..=centre.1 + LIT_CELLS {
                let c = (x, y);
                if state.lit.contains_key(&c) {
                    continue;
                }
                let list = state.read.get(&c).cloned().unwrap_or_default();
                let spawned: Vec<(Entity, f32)> = list.iter().map(|g| (spawn_lamp(&mut commands, root, g), g.lamp.intensity)).collect();
                lamps += spawned.len();
                state.lit.insert(c, spawned);
            }
        }
        if lamps > 0 {
            info!("distant lights: {lamps} lamp lights added around {centre:?} ({} lit in all)", state.lit.values().map(Vec::len).sum::<usize>());
        }
        state.chosen = Some(key);
    }
    // Strength per cell, materials touched only when its step changes.
    for (c, list) in &state.drawn {
        let mid = Vec2::new((c.0 as f32 + 0.5) * CELL_SIZE, (c.1 as f32 + 0.5) * CELL_SIZE);
        let s = strength(night, streamer.cell_level(*c), mid.distance(eye.truncate()));
        let step = (s * LEVELS).round() as u32;
        if state.applied.get(c) == Some(&step) {
            continue;
        }
        state.applied.insert(*c, step);
        let k = step as f32 / LEVELS * STRENGTH;
        for (e, m, g) in list {
            if let Some(mut mat) = materials.get_mut(m) {
                mat.base_color = Color::linear_rgb(g.color[0] * k, g.color[1] * k, g.color[2] * k);
            }
            commands.entity(*e).insert(if step == 0 { Visibility::Hidden } else { Visibility::Inherited });
        }
    }
    // Lamp light: the glow's strength without the range fade, faded by `lit_reach`.
    for (c, list) in &state.lit {
        let mid = Vec2::new((c.0 as f32 + 0.5) * CELL_SIZE, (c.1 as f32 + 0.5) * CELL_SIZE);
        let k = (night * (1.0 - streamer.cell_level(*c)) * lit_reach(mid.distance(eye.truncate()))).clamp(0.0, 1.0);
        for (e, full) in list {
            let Ok((point, spot)) = lamp_lights.get_mut(*e) else { continue };
            let i = full * k;
            if let Some(mut p) = point {
                if p.intensity != i {
                    p.intensity = i;
                }
            }
            if let Some(mut s) = spot {
                if s.intensity != i {
                    s.intensity = i;
                }
            }
        }
    }
    // Face the camera, sized to a few pixels.
    for list in state.drawn.values() {
        for (e, _, g) in list {
            let Ok(mut t) = glows.get_mut(*e) else { continue };
            let to_eye = eye - g.at;
            let d = to_eye.length();
            if d < 1.0 {
                continue;
            }
            t.rotation = Quat::from_rotation_arc(Vec3::Z, to_eye / d);
            t.scale = Vec3::splat((d * PIXEL_ANGLE).max(MIN_SIZE));
        }
    }
}

/// A distant lamp's light (see the module notes).
#[derive(Component)]
pub struct DistantLamp;

/// Spawn the light of distant lamp `g` under the world root, dark until `update` sets it.
fn spawn_lamp(commands: &mut Commands, root: Entity, g: &Glow) -> Entity {
    let l = &g.lamp;
    let color = display_color(l.color);
    let t = Transform::from_translation(g.at);
    match l.spot {
        // Spot lights point down their reference's -z [hypothesis, as `spawn_light`].
        Some(a) => commands
            .spawn((
                Name::new("Distant lamp"),
                SpotLight { color, intensity: 0.0, range: l.range, outer_angle: a, inner_angle: a * 0.8, shadow_maps_enabled: false, ..default() },
                t.looking_to(-Vec3::Z, Vec3::Y),
                ChildOf(root),
                DistantLamp,
            ))
            .id(),
        None => commands
            .spawn((Name::new("Distant lamp"), PointLight { color, intensity: 0.0, range: l.range, shadow_maps_enabled: false, ..default() }, t, ChildOf(root), DistantLamp))
            .id(),
    }
}

/// Give each streamed light its cell's draw level of its full brightness.
pub fn fade_cell_lights(
    streamer: Res<Streamer>,
    mut lights: Query<CellLightParts<'_>>,
) {
    for (cl, point, spot, moving) in &mut lights {
        let level = match cl.cell {
            Some(c) if streamer.interior.is_none() => streamer.cell_level(c),
            _ => 1.0,
        };
        let i = cl.full * level;
        if let Some(mut m) = moving {
            if m.full != i {
                m.full = i;
            }
            continue;
        }
        if let Some(mut p) = point {
            if p.intensity != i {
                p.intensity = i;
            }
        }
        if let Some(mut s) = spot {
            if s.intensity != i {
                s.intensity = i;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glows_give_way_to_their_cell_and_the_day() {
        assert_eq!(strength(1.0, 0.0, 0.0), 1.0);
        assert_eq!(strength(1.0, 1.0, 0.0), 0.0);
        assert_eq!(strength(0.0, 0.0, 0.0), 0.0);
        assert!((strength(1.0, 0.25, 0.0) - 0.75).abs() < 1e-6);
        let range = RANGE_CELLS as f32 * CELL_SIZE;
        assert_eq!(strength(1.0, 0.0, 0.8 * range), 1.0);
        assert!((strength(1.0, 0.0, 0.9 * range) - 0.5).abs() < 1e-4);
        assert!(strength(1.0, 0.0, range) < 1e-4);
        assert_eq!(strength(1.0, 0.0, 1.1 * range), 0.0);
    }

    #[test]
    fn lamp_light_fades_over_the_last_lit_cell() {
        let end = LIT_CELLS as f32 * CELL_SIZE;
        assert_eq!(lit_reach(0.0), 1.0);
        assert_eq!(lit_reach(end - LIT_FADE_CELLS * CELL_SIZE), 1.0);
        assert!((lit_reach(end - 0.5 * LIT_FADE_CELLS * CELL_SIZE) - 0.5).abs() < 1e-4);
        assert_eq!(lit_reach(end), 0.0);
        assert_eq!(lit_reach(2.0 * end), 0.0);
    }

    #[test]
    fn lamps_are_off_while_the_sun_is_up() {
        for e in [4.0, 10.0, 20.0, 39.0, 60.0] {
            assert_eq!(lamps_on(e), 0.0, "sun at {e}°");
        }
        assert_eq!(lamps_on(-4.0), 1.0);
        assert_eq!(lamps_on(-30.0), 1.0);
        let mid = lamps_on(0.0);
        assert!(mid > 0.4 && mid < 0.6, "{mid}");
        assert!(lamps_on(2.0) < lamps_on(-2.0));
    }

    #[test]
    fn glow_image_is_a_soft_spot() {
        let img = glow_image();
        let d = img.data.as_ref().unwrap();
        let alpha = |x: usize, y: usize| d[(y * 64 + x) * 4 + 3];
        assert!(alpha(32, 32) > 240);
        assert_eq!(alpha(0, 0), 0);
        assert!(alpha(32, 32) > alpha(48, 32));
    }
}
