//! The Caravan table in 3D (`nvf_play::caravan`, docs/spec-notes/caravan.md §8): the
//! planks and the cards as lit, shadowed 3D cards, drawn by a camera of their own into a
//! picture the game screen shows behind its text ([`nvf_play::caravan::CaravanView`]), as
//! the poker table is (`poker_view.rs`).
//!
//! - The original has 3D models for this screen (`meshes\terminals\nv_caravan\`: the
//!   table, one model per caravan place of each side with four cards in it, the decks)
//!   with no animation sequences in them [observed: the files' nodes]; where it puts them
//!   at run time is not known, and the player's places lie off the table model. NVForever
//!   lays the table out itself: every card where the flat screen draws it
//!   (`nvf_play::caravan::screen::table_cards`), so the names, values, outlines and
//!   buttons the screen draws over the picture fall on the cards [design].
//! - The camera looks straight down (orthographic), the frame's units being the table's:
//!   x right, y up the screen (the frame's y turned over), z up off the planks [design].
//! - The planks are the game's table texture (`nv_caravan-table01.dds`, its normal map
//!   `_n`) over the whole view; a warm lamp above the left of the table and a little
//!   ambient light leave the edges darker, as the owner's screenshot of the original
//!   [design].
//! - Cards are the cards' own face and back textures (CCRD TX00/TX01). Each lies a hair
//!   above the one under it, turned by up to [`JITTER_DEG`] (the original's place models
//!   turn theirs by -7.5..+10 degrees [observed: player_*/opponent_* nodes' rotations]).
//!   A face card lies over the card it acts on, a quarter of a card to its right and
//!   lower, as in the original's place models [observed: their Card_02..04 nodes].
//! - Moves slide [design]: a card played goes from the hand to its place (the opponent's
//!   turns face up on the way), a card drawn from the deck to the hand, a card thrown away
//!   (a discard, a jack's victim, a caravan thrown away) off the left of the table. A card
//!   in flight is lifted off the table, so its shadow shows how high.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{AmbientLight, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use nvf_assets::model::ModelCtx;
use nvf_assets::{nif_material, NifMaterial};
use nvf_play::caravan::screen::{card_texture, table_cards, view_width, TableCard, FRAME_W};
use nvf_play::caravan::{CaravanScreen, CaravanView, Session, Stage};
use nvf_play::pause::AidTime;
use std::collections::HashMap;
use std::f32::consts::PI;

/// The render layer of the table, its lights and its camera.
pub const CARAVAN_LAYER: usize = 14;
/// Where the table is put, far from the world (Bevy space) [design].
const PLACE: Vec3 = Vec3::new(300_000.0, -300_000.0, 0.0);
const PLANKS: &str = "textures\\terminals\\nv_caravan\\nv_caravan-table01.dds";
const PLANKS_NORMAL: &str = "textures\\terminals\\nv_caravan\\nv_caravan-table01_n.dds";
/// The camera's height over the planks (orthographic: only the depth range matters).
const EYE_UP: f32 = 3000.0;
/// The lamp: above the table's left, this high, its brightness and reach; the ambient
/// light [design: by eye against the owner's screenshot of the original].
const LAMP_AT: Vec2 = Vec2::new(-500.0, 250.0);
const LAMP_UP: f32 = 1100.0;
const LAMP: f32 = 1.25;
const LAMP_RANGE: f32 = 2400.0;
const AMBIENT: f32 = 0.12;
/// Each card this much higher than the one listed before it.
const CARD_STEP: f32 = 1.2;
/// A raised hand card, this much higher.
const RAISED_UP: f32 = 24.0;
/// The most a card is turned, either way (degrees).
const JITTER_DEG: f32 = 3.0;
/// How fast a card closes on its place (per second, exponential) and how high it is
/// lifted per unit still to go (at most [`LIFT_MAX`]).
const SLIDE_RATE: f32 = 9.0;
const LIFT_PER_UNIT: f32 = 0.18;
const LIFT_MAX: f32 = 70.0;
/// A card thrown away leaves to the left, and is gone after this long.
const GONE_X: f32 = -260.0;
const GONE_SECONDS: f32 = 0.9;

pub struct CaravanViewPlugin;

impl Plugin for CaravanViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync));
    }
}

/// A card drawn: its entities, what it shows, where it is (without the lift) and how it
/// is turned; thrown away since then.
struct CardDrawn {
    root: Entity,
    front: Entity,
    back: Entity,
    form: fnv_sim::FormId,
    pos: Vec3,
    rot: Quat,
    gone: Option<f32>,
}

struct Built {
    entities: Vec<Entity>,
    space: Entity,
    camera: Entity,
    planks: Entity,
    layer: RenderLayers,
    card_mesh: Handle<Mesh>,
    cards: HashMap<String, CardDrawn>,
    leaving: Vec<CardDrawn>,
    materials: HashMap<String, Handle<NifMaterial>>,
    view_w: f32,
}

#[derive(Default)]
struct Drawn {
    built: Option<Built>,
    warned: bool,
}

/// The frame's scale in the view (a view narrower than 16:9 shows the whole frame,
/// smaller), as the screen's.
fn frame_scale(view_w: f32) -> f32 {
    (view_w / FRAME_W).min(1.0)
}

/// A frame point (its units, y down) on the table (y up).
fn on_table(p: Vec2) -> Vec2 {
    Vec2::new(p.x, -p.y)
}

/// The view's centre on the table: the frame's centre.
fn centre() -> Vec2 {
    on_table(Vec2::new(FRAME_W / 2.0, 480.0))
}

/// The orthographic projection showing the whole view: `view_w` by 960 of the screen's
/// units, the frame scaled into it.
fn projection(view_w: f32) -> Projection {
    let k = frame_scale(view_w);
    Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: view_w / k, height: 960.0 / k }, near: 1.0, far: EYE_UP * 2.0, ..OrthographicProjection::default_3d() })
}

/// A card's small turn, the same each time for its key.
fn jitter(key: &str) -> f32 {
    let h = key.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    ((h % 1000) as f32 / 999.0 * 2.0 - 1.0) * JITTER_DEG
}

fn turn(key: &str, face_up: bool) -> Quat {
    let flip = if face_up { Quat::IDENTITY } else { Quat::from_rotation_y(PI) };
    Quat::from_rotation_z(jitter(key).to_radians()) * flip
}

/// Where a card should be: its middle on the table, its height.
fn target(c: &TableCard, k: usize) -> Vec3 {
    let mid = on_table(c.at + c.size / 2.0);
    mid.extend(2.0 + CARD_STEP * k as f32 + if c.raised { RAISED_UP } else { 0.0 })
}

fn material(models: &mut ModelCtx, cache: &mut HashMap<String, Handle<NifMaterial>>, path: &str) -> Handle<NifMaterial> {
    if let Some(m) = cache.get(path) {
        return m.clone();
    }
    let full = format!("textures\\{}", path.trim_start_matches("textures\\").trim_start_matches("Textures\\"));
    let m = match models.textures.get(&full, true, &models.data, &mut models.images, models.formats.as_deref()) {
        Some(t) => models.materials.add(nif_material(StandardMaterial { base_color_texture: Some(t), perceptual_roughness: 0.8, reflectance: 0.25, alpha_mode: AlphaMode::Mask(0.5), ..default() })),
        None => {
            warn!("caravan view: no card picture {full}; that card is drawn plain");
            models.materials.add(nif_material(StandardMaterial { base_color: Color::srgb(0.85, 0.82, 0.74), perceptual_roughness: 0.8, ..default() }))
        }
    };
    cache.insert(path.to_string(), m.clone());
    m
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    screen: Res<CaravanScreen>,
    mut models: ModelCtx,
    window: Query<&Window, With<PrimaryWindow>>,
    time: AidTime,
    mut transforms: Query<&mut Transform>,
    mut projections: Query<&mut Projection>,
    mut drawn: Local<Drawn>,
) {
    let session = screen.0.as_ref().filter(|s| s.stage == Stage::Play && s.game.is_some());
    let Some(s) = session else {
        if let Some(b) = drawn.built.take() {
            for e in b.entities {
                commands.entity(e).despawn();
            }
            commands.remove_resource::<CaravanView>();
            info!("caravan view: the table put away");
        }
        return;
    };
    if drawn.built.is_none() {
        match build(&mut commands, &mut models, window.single().ok()) {
            Some(b) => drawn.built = Some(b),
            None => {
                if !drawn.warned {
                    warn!("caravan view: the table's planks are not in the archives; the flat screen is shown");
                    drawn.warned = true;
                }
                return;
            }
        }
    }
    let Some(b) = drawn.built.as_mut() else { return };
    // The view's shape may change (a resized window).
    let view_w = view_width(window.single().ok());
    if (view_w - b.view_w).abs() > 0.5 {
        b.view_w = view_w;
        if let Ok(mut p) = projections.get_mut(b.camera) {
            *p = projection(view_w);
        }
        let k = frame_scale(view_w);
        commands.entity(b.planks).insert(Transform::from_translation(centre().extend(0.0)).with_scale(Vec3::new(view_w / k, 960.0 / k, 1.0)));
    }
    let now = time.elapsed_secs();
    let dt = (time.delta_secs_f64() as f32).clamp(0.0, 0.1);
    place_cards(&mut commands, &mut models, b, s, now);
    let close = 1.0 - (-SLIDE_RATE * dt).exp();
    let cards = table_cards(s);
    let goals: HashMap<&str, (Vec3, Quat)> = cards.iter().enumerate().map(|(k, c)| (c.key.as_str(), (target(c, k), turn(&c.key, c.face_up)))).collect();
    for (key, c) in b.cards.iter_mut() {
        let Some(&(goal, rot)) = goals.get(key.as_str()) else { continue };
        c.pos = c.pos.lerp(goal, close);
        c.rot = c.rot.slerp(rot, close);
        let lift = ((goal - c.pos).truncate().length() * LIFT_PER_UNIT).min(LIFT_MAX);
        set(&mut commands, &mut transforms, c.root, c.pos + Vec3::Z * lift, c.rot, cards.iter().find(|t| t.key == *key).map_or(Vec2::ONE, |t| t.size));
    }
    b.leaving.retain_mut(|c| {
        let since = c.gone.unwrap_or(now);
        if now - since > GONE_SECONDS {
            commands.entity(c.root).despawn();
            return false;
        }
        let goal = Vec3::new(GONE_X, c.pos.y, c.pos.z);
        c.pos = c.pos.lerp(goal, close);
        let lift = ((goal - c.pos).truncate().length() * LIFT_PER_UNIT).min(LIFT_MAX);
        if let Ok(mut t) = transforms.get_mut(c.root) {
            t.translation = c.pos + Vec3::Z * lift;
        }
        true
    });
}

fn set(commands: &mut Commands, transforms: &mut Query<&mut Transform>, e: Entity, at: Vec3, rot: Quat, size: Vec2) {
    let tr = Transform::from_translation(at).with_rotation(rot).with_scale(size.extend(1.0));
    match transforms.get_mut(e) {
        Ok(mut t) => *t = tr,
        Err(_) => {
            commands.entity(e).insert(tr);
        }
    }
}

/// Cards that came and went since the last frame: a card played is the hand's card that
/// left (it flies from there), a card drawn comes from its side's deck, a card gone leaves.
fn place_cards(commands: &mut Commands, models: &mut ModelCtx, b: &mut Built, s: &Session, now: f32) {
    let cards = table_cards(s);
    let keys: Vec<&str> = cards.iter().map(|c| c.key.as_str()).collect();
    let gone_keys: Vec<String> = b.cards.keys().filter(|k| !keys.contains(&k.as_str())).cloned().collect();
    let mut gone: Vec<(String, CardDrawn)> = gone_keys.into_iter().filter_map(|k| b.cards.remove(&k).map(|c| (k, c))).collect();
    for (k, c) in cards.iter().enumerate() {
        let face = material(models, &mut b.materials, &card_texture(s, c.form, true));
        let back = material(models, &mut b.materials, &card_texture(s, c.form, false));
        if let Some(d) = b.cards.get_mut(&c.key) {
            if d.form != c.form {
                d.form = c.form;
                commands.entity(d.front).insert(MeshMaterial3d(face));
                commands.entity(d.back).insert(MeshMaterial3d(back));
            }
            continue;
        }
        // Played: the hand's card of the same form that just left.
        let from_hand = (c.key.starts_with('c')).then(|| gone.iter().position(|(gk, g)| gk.starts_with('h') && g.form == c.form)).flatten();
        let drawn = match from_hand {
            Some(i) => {
                let (_, d) = gone.swap_remove(i);
                commands.entity(d.front).insert(MeshMaterial3d(face));
                commands.entity(d.back).insert(MeshMaterial3d(back));
                d
            }
            None => {
                let side = c.key[1..].chars().next().and_then(|ch| ch.to_digit(10)).unwrap_or(0);
                let (pos, rot) = if c.key.starts_with('h') {
                    // Drawn: from its side's deck, face down.
                    let deck = cards.iter().position(|d| d.key == format!("d{side}"));
                    let at = deck.map_or(target(c, k) + Vec3::new(120.0, 0.0, 0.0), |i| target(&cards[i], i) + Vec3::Z * 2.0);
                    (at, turn(&c.key, false))
                } else {
                    (target(c, k), turn(&c.key, c.face_up))
                };
                let root = commands.spawn((Transform::from_translation(pos).with_rotation(rot).with_scale(c.size.extend(1.0)), Visibility::Inherited, ChildOf(b.space))).id();
                let front = commands.spawn((Mesh3d(b.card_mesh.clone()), MeshMaterial3d(face), Transform::from_xyz(0.0, 0.0, 0.15), b.layer.clone(), ChildOf(root))).id();
                let back_e = commands
                    .spawn((Mesh3d(b.card_mesh.clone()), MeshMaterial3d(back), Transform::from_xyz(0.0, 0.0, -0.15).with_rotation(Quat::from_rotation_y(PI)), b.layer.clone(), ChildOf(root)))
                    .id();
                CardDrawn { root, front, back: back_e, form: c.form, pos, rot, gone: None }
            }
        };
        b.cards.insert(c.key.clone(), drawn);
    }
    for (_, mut d) in gone {
        d.gone = Some(now);
        b.leaving.push(d);
    }
}

fn build(commands: &mut Commands, models: &mut ModelCtx, window: Option<&Window>) -> Option<Built> {
    let planks_tex = models.textures.get(PLANKS, true, &models.data, &mut models.images, models.formats.as_deref())?;
    let normal = models.textures.get(PLANKS_NORMAL, false, &models.data, &mut models.images, models.formats.as_deref());
    let has_normal = normal.is_some();
    let layer = RenderLayers::layer(CARAVAN_LAYER);
    let view_w = view_width(window);
    let holder = commands.spawn((Name::new("Caravan table"), Transform::from_translation(PLACE), Visibility::Inherited)).id();
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let space = commands.spawn((Transform::from_rotation(Quat::from_rotation_x(-PI / 2.0)), Visibility::Inherited, ChildOf(holder))).id();
    let quad = || Mesh::from(Rectangle::new(1.0, 1.0)).with_generated_tangents().ok();
    let planks_mesh = models.meshes.add(quad()?);
    let planks_mat = models.materials.add(nif_material(StandardMaterial { base_color_texture: Some(planks_tex), normal_map_texture: normal, perceptual_roughness: 0.9, reflectance: 0.15, ..default() }));
    let k = frame_scale(view_w);
    let planks = commands
        .spawn((
            Mesh3d(planks_mesh),
            MeshMaterial3d(planks_mat),
            Transform::from_translation(centre().extend(0.0)).with_scale(Vec3::new(view_w / k, 960.0 / k, 1.0)),
            NotShadowCaster,
            layer.clone(),
            ChildOf(space),
        ))
        .id();
    let card_mesh = models.meshes.add(quad()?);
    let size = window.map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 });
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
    let c = centre();
    let camera = commands
        .spawn((
            Name::new("Caravan camera"),
            Camera3d::default(),
            Camera { order: -4, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            projection(view_w),
            Tonemapping::Linear,
            Msaa::Sample4,
            AmbientLight { color: Color::srgb(1.0, 0.95, 0.88), brightness: AMBIENT * crate::sky::light_unit(), ..default() },
            // Straight down, the table's +y up the picture.
            Transform::from_translation(to(c.extend(EYE_UP))).looking_at(to(c.extend(0.0)), nvf_play::space::to_bevy([0.0, 1.0, 0.0])),
            layer.clone(),
        ))
        .id();
    let lamp_at = (c + LAMP_AT).extend(LAMP_UP);
    let lamp = commands
        .spawn((
            SpotLight { color: Color::srgb(1.0, 0.9, 0.76), intensity: crate::lights::lumens(LAMP, crate::sky::light_unit()), range: LAMP_RANGE, radius: 40.0, shadow_maps_enabled: true, inner_angle: 0.5, outer_angle: 1.2, ..default() },
            Transform::from_translation(to(lamp_at)).looking_at(to(c.extend(0.0)), Vec3::Z),
            layer.clone(),
        ))
        .id();
    commands.insert_resource(CaravanView { image });
    info!("caravan view: the table built ({PLANKS}, normal map {}), view {view_w:.0} wide", if has_normal { "found" } else { "missing" });
    Some(Built { entities: vec![holder, camera, lamp], space, camera, planks, layer, card_mesh, cards: HashMap::new(), leaving: Vec::new(), materials: HashMap::new(), view_w })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cards_turn_is_small_and_fixed_by_its_key() {
        for key in ["c0:0:0", "c1:2:5:f1", "h0:0012ABCD:0", "d1"] {
            let a = jitter(key);
            assert!(a.abs() <= JITTER_DEG, "{key}: {a}");
            assert_eq!(a, jitter(key));
        }
        assert_ne!(jitter("c0:0:0"), jitter("c0:0:1"));
    }

    #[test]
    fn the_frame_maps_onto_the_table_turned_over() {
        assert_eq!(on_table(Vec2::new(10.0, 20.0)), Vec2::new(10.0, -20.0));
        assert_eq!(centre(), Vec2::new(FRAME_W / 2.0, -480.0));
        // A view narrower than 16:9 shows the whole frame, smaller.
        assert!(frame_scale(FRAME_W * 0.8) < 1.0);
        assert_eq!(frame_scale(FRAME_W * 1.2), 1.0);
    }
}
