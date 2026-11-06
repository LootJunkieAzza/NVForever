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
//!   strength is the night (1 − `SkyState::daylight`) × the part of its cell not yet
//!   drawn (1 − `Streamer::cell_level`) × a fade over the last fifth of the range, so a
//!   lamp's glow gives way to the lamp and its light as the cell fades in, and glows
//!   come up at dusk. Lights the game would not light are left out by
//!   `lights::spawn_light`'s rules (Negative, Off by Default, no radius, black). The
//!   light's model colour (black LIGH colours) is not read: such lights get no glow.
//!   Enable state is read once (initially disabled, enable parents), not followed
//!   when a script changes it later.
//! - **Cell lights** [design]: [`CellLight`] holds a streamed light's full brightness;
//!   [`fade_cell_lights`] scales it by its cell's draw level, so it comes up over the
//!   cell's fade (`--cell-fade`) together with the cell's meshes instead of at once.

use crate::lights::{CellLight, MovingLight};
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
            Some(Glow { at: Vec3::from_array(p.position), color: l.color.map(|b| b as f32 / 255.0 * fade) })
        })
        .collect()
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
    mut glows: Query<&mut Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let state = &mut *state;
    let night = 1.0 - sky.daylight();
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
    fn glow_image_is_a_soft_spot() {
        let img = glow_image();
        let d = img.data.as_ref().unwrap();
        let alpha = |x: usize, y: usize| d[(y * 64 + x) * 4 + 3];
        assert!(alpha(32, 32) > 240);
        assert_eq!(alpha(0, 0), 0);
        assert!(alpha(32, 32) > alpha(48, 32));
    }
}
