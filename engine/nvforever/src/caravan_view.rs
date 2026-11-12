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
//! - The pot lies on the planks under the cards, as in the original: bottle caps, the
//!   Legion's denarius and aureus and NCR notes cut from the game's own Caravan money
//!   sheet (`caravanmoney_d.dds`, 4096 by 2048: the caps and coins along the top, the
//!   $100, $5 and $20 notes under them, each cut out by its alpha) [observed: the texture].
//!   How many and which follow the pot: more of them as it grows, Legion coins from 20,
//!   notes from 40, bigger notes and gold for bigger pots, each in a place and turn fixed
//!   by its number, so the pile only grows as the bet does [design, owner 2026-11-09:
//!   "fill the caravan play area like they do in the original, matching the bet"].
//!   Caps and coins are the game's own Caravan models (`meshes\terminals\nv_caravan\
//!   currency-coin_1.nif` the cap, `_2` silver, `_3` gold [observed: their textures and
//!   shapes]), so they stand up off the planks and catch the lamp; the notes lie flat. A
//!   piece goes only where no card can ever lie (`card_zones`); one that finds no such
//!   place is left out [design, owner 2026-11-09: "make sure things on the table don't
//!   overlap with the cards, cash can go under everything else just stay away"].

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{AmbientLight, NotShadowCaster};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use nvf_assets::model::ModelCtx;
use nvf_assets::{nif_material, NifMaterial};
use nvf_play::caravan::screen::{card_texture, card_zones, table_cards, view_width, TableCard, FRAME_W};
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
/// The money sheet, and where each piece is on it (u0, v0, u1, v1) [observed: the
/// texture's alpha; 4096 by 2048].
const MONEY_SHEET: &str = "textures\\terminals\\nv_caravan\\caravanmoney_d.dds";
const fn sheet(x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 4] {
    [x0 / 4096.0, y0 / 2048.0, x1 / 4096.0, y1 / 2048.0]
}
const CAP_PLAIN: [f32; 4] = sheet(0.0, 13.0, 506.0, 518.0);
const CAP_NUKA: [f32; 4] = sheet(524.0, 13.0, 1032.0, 518.0);
const DENARIUS: [f32; 4] = sheet(1159.0, 6.0, 1678.0, 526.0);
const AUREUS: [f32; 4] = sheet(2332.0, 1.0, 2852.0, 521.0);
const NOTE_100: [f32; 4] = sheet(26.0, 534.0, 1338.0, 1273.0);
const NOTE_5: [f32; 4] = sheet(1380.0, 534.0, 2706.0, 1276.0);
const NOTE_20: [f32; 4] = sheet(2745.0, 534.0, 4056.0, 1275.0);
/// Real sizes on the table's scale: a card on it is 147 high, a playing card 88.9 mm
/// (3.5 in), so 1.65 units to the millimetre [documented: playing cards are 2.5 by 3.5 in;
/// a US note is 156 by 66.3 mm; a crown cap is about 32 mm across; the Legion coins are
/// given a quarter's 24.3 mm (denarius) and an aureus's 20 mm, hypothesis: the game gives
/// no coin size].
const MM: f32 = 147.0 / 88.9;
/// A cap across, a coin across, a note's length; the note keeps its picture's shape
/// (1312 by 740 on the sheet: 156 by 88 mm), which a real note's 156 by 66 would squash.
const CAP_SIZE: Vec2 = Vec2::new(32.0 * MM, 32.0 * MM);
const DENARIUS_SIZE: Vec2 = Vec2::new(24.3 * MM, 24.3 * MM);
const AUREUS_SIZE: Vec2 = Vec2::new(20.0 * MM, 20.0 * MM);
const NOTE_SIZE: Vec2 = Vec2::new(156.0 * MM, 156.0 * MM * 740.0 / 1312.0);
/// The most pieces on the table.
const MONEY_MAX: usize = 90;
/// How far a piece keeps from where a card can lie, and how many places it tries.
const MONEY_CLEAR: f32 = 10.0;
const MONEY_TRIES: u32 = 96;
/// The least distance between the middles of two caps or coins: a cap across and a
/// little, the same for every round piece so a coin turning into a cap moves nothing.
const ROUND_APART: f32 = CAP_SIZE.x * 1.1;
/// The game's Caravan cap and coin models.
const CAP_MODEL: &str = "meshes\\terminals\\nv_caravan\\currency-coin_1.nif";
const SILVER_MODEL: &str = "meshes\\terminals\\nv_caravan\\currency-coin_2.nif";
const GOLD_MODEL: &str = "meshes\\terminals\\nv_caravan\\currency-coin_3.nif";
/// Each model has a flat disc round its piece (its shadow on the table); the piece itself
/// is this much of the disc across: the cap 0.58 of 0.74, a coin 0.55 [observed: the
/// models' vertices].
const CAP_BODY: f32 = 0.58 / 0.74;
const COIN_BODY: f32 = 0.55 / 0.74;
/// The models drew 31 units across where 53 were asked (a cap, 32 mm) in the first
/// capture, so the scale from their bounds is corrected by that [observed: 2026-11-09].
const MODEL_FIT: f32 = 53.0 / 31.0;

/// A piece of the pot on the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Money {
    Cap,
    NukaCap,
    Denarius,
    Aureus,
    Note5,
    Note20,
    Note100,
}

impl Money {
    fn rect(self) -> [f32; 4] {
        match self {
            Money::Cap => CAP_PLAIN,
            Money::NukaCap => CAP_NUKA,
            Money::Denarius => DENARIUS,
            Money::Aureus => AUREUS,
            Money::Note5 => NOTE_5,
            Money::Note20 => NOTE_20,
            Money::Note100 => NOTE_100,
        }
    }

    fn size(self) -> Vec2 {
        match self {
            Money::Cap | Money::NukaCap => CAP_SIZE,
            Money::Denarius => DENARIUS_SIZE,
            Money::Aureus => AUREUS_SIZE,
            _ => NOTE_SIZE,
        }
    }

    fn note(self) -> bool {
        matches!(self, Money::Note5 | Money::Note20 | Money::Note100)
    }

    /// The game's model for a cap or coin, and how much of the model across the piece is.
    fn model(self) -> Option<(&'static str, f32)> {
        match self {
            Money::Cap | Money::NukaCap => Some((CAP_MODEL, CAP_BODY)),
            Money::Denarius => Some((SILVER_MODEL, COIN_BODY)),
            Money::Aureus => Some((GOLD_MODEL, COIN_BODY)),
            _ => None,
        }
    }
}

/// Whether a piece of `size` at `at`, turned by `turn`, keeps out of every zone: its
/// corners, the middles of its sides and its middle are tested (a zone is far bigger than
/// the gaps between those points).
fn clear_of(zones: &[(Vec2, Vec2)], at: Vec2, size: Vec2, turn: f32) -> bool {
    let (sin, cos) = turn.sin_cos();
    let h = size / 2.0;
    let on_frame = (0..9).map(|k| {
        let local = Vec2::new(h.x * ((k % 3) as f32 - 1.0), h.y * ((k / 3) as f32 - 1.0));
        at + Vec2::new(local.x * cos - local.y * sin, local.x * sin + local.y * cos)
    });
    on_frame.into_iter().all(|p| zones.iter().all(|(a, b)| p.x < a.x || p.x > b.x || p.y < a.y || p.y > b.y))
}

/// A number from 0 to 1, the same each time for `i` and `salt`.
fn scatter(i: usize, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 10_000) as f32 / 9_999.0
}

/// The pot's pieces: what each is, its middle (frame units, y down) and its turn
/// (radians). Piece `i` is the same for every pot that has it, so the pile grows.
pub fn money(pot: u32) -> Vec<(Money, Vec2, f32)> {
    if pot == 0 {
        return Vec::new();
    }
    let n = (((pot as f32).sqrt() * 3.0).ceil() as usize).clamp(2, MONEY_MAX);
    let zones = card_zones(MONEY_CLEAR);
    // The caps and coins placed so far: one never lands on another (the owner,
    // 2026-11-12: "Overlapping caps!"); notes may lie under them [design].
    let mut round: Vec<Vec2> = Vec::new();
    (0..n)
        .filter_map(|i| {
            let kind = if pot >= 40 && i % 4 == 3 {
                match (pot, i % 3) {
                    (0..=149, _) => Money::Note5,
                    (150..=599, 0) => Money::Note5,
                    (150..=599, _) => Money::Note20,
                    (_, 0) => Money::Note20,
                    _ => Money::Note100,
                }
            } else if pot >= 20 && i % 5 == 2 {
                // Legion coins; gold among them once the pot is big.
                if pot >= 300 && i % 3 == 0 { Money::Aureus } else { Money::Denarius }
            } else if scatter(i, 7) < 0.6 {
                Money::NukaCap
            } else {
                Money::Cap
            };
            // Anywhere on the planks where no card can lie; left out if nowhere is found
            // (the same tries for the same piece, so the pile still only grows) [design].
            let turn = scatter(i, 53) * 2.0 * PI;
            let h = kind.size().max_element() / 2.0;
            let is_round = !kind.note();
            let at = (0..MONEY_TRIES)
                .map(|k| Vec2::new(h + scatter(i, 11 + k) * (FRAME_W - 2.0 * h), h + scatter(i, 131 + k) * (960.0 - 2.0 * h)))
                .find(|at| clear_of(&zones, *at, kind.size(), turn) && (!is_round || round.iter().all(|r| r.distance(*at) >= ROUND_APART)))?;
            if is_round {
                round.push(at);
            }
            Some((kind, at, turn))
        })
        .collect()
}

/// A unit square showing `rect` of its picture.
fn atlas_quad(rect: [f32; 4]) -> Option<Mesh> {
    let mut m = Mesh::from(Rectangle::new(1.0, 1.0));
    if let Some(VertexAttributeValues::Float32x2(uv)) = m.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for p in uv.iter_mut() {
            p[0] = rect[0] + p[0] * (rect[2] - rect[0]);
            p[1] = rect[1] + p[1] * (rect[3] - rect[1]);
        }
    }
    m.with_generated_tangents().ok()
}

/// Lays the pot out again when it changed.
fn place_money(commands: &mut Commands, models: &mut ModelCtx, b: &mut Built, pot: u32) {
    if b.money_pot == Some(pot) {
        return;
    }
    b.money_pot = Some(pot);
    for e in b.money.drain(..) {
        commands.entity(e).despawn();
    }
    let pieces = money(pot);
    for (i, (kind, at, turn)) in pieces.iter().enumerate() {
        if let Some((path, body)) = kind.model() {
            if let Some(e) = spawn_piece(commands, models, b, path, body * kind.size().x * MODEL_FIT, on_table(*at), *turn) {
                b.money.push(e);
                continue;
            }
        }
        let Some(mat) = money_material(models, &mut b.materials, MONEY_SHEET) else { continue };
        let mesh = match b.money_meshes.iter().find(|(k, _)| k == kind) {
            Some((_, m)) => m.clone(),
            None => {
                let Some(m) = atlas_quad(kind.rect()) else { continue };
                let m = models.meshes.add(m);
                b.money_meshes.push((*kind, m.clone()));
                m
            }
        };
        // Notes first, caps over them; each a hair over the last, all under the cards.
        let up = if kind.note() { 0.3 } else { 0.9 } + 0.004 * i as f32;
        let tr = Transform::from_translation(on_table(*at).extend(up)).with_rotation(Quat::from_rotation_z(*turn)).with_scale(kind.size().extend(1.0));
        b.money.push(commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), tr, b.layer.clone(), ChildOf(b.space))).id());
    }
    info!("caravan view: the pot of {pot} on the table as {} pieces", pieces.len());
}

/// A cap or coin from the game's model, `across` wide (its piece, not the disc round it),
/// standing on the planks at `at`, turned by `turn`.
fn spawn_piece(commands: &mut Commands, models: &mut ModelCtx, b: &Built, path: &str, across: f32, at: Vec2, turn: f32) -> Option<Entity> {
    let pm = models.model(path)?;
    let (lo, hi) = pm.bounds?;
    let wide = (hi.x - lo.x).max(hi.y - lo.y);
    if wide <= 0.0 {
        return None;
    }
    let k = across / wide;
    // On the planks, under the cards (which lie from 2 up), its bottom on the wood.
    let holder = commands
        .spawn((Transform::from_translation(at.extend(0.2 - lo.z * k)).with_rotation(Quat::from_rotation_z(turn)).with_scale(Vec3::splat(k)), Visibility::Inherited, ChildOf(b.space)))
        .id();
    let root = commands.spawn((Transform::from_matrix(Mat4::from(pm.root)), Visibility::Inherited, ChildOf(holder))).id();
    let nodes: Vec<Entity> = pm.nodes.iter().map(|n| commands.spawn((Transform::from_matrix(Mat4::from(n.local)), Visibility::Inherited)).id()).collect();
    for (n, e) in pm.nodes.iter().zip(&nodes) {
        commands.entity(n.parent.map_or(root, |p| nodes[p])).add_child(*e);
    }
    for s in &pm.shapes {
        let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.own)), Visibility::Inherited, b.layer.clone())).id();
        commands.entity(s.node.map_or(root, |n| nodes[n])).add_child(e);
    }
    Some(holder)
}

fn money_material(models: &mut ModelCtx, cache: &mut HashMap<String, Handle<NifMaterial>>, path: &str) -> Option<Handle<NifMaterial>> {
    if let Some(m) = cache.get(path) {
        return Some(m.clone());
    }
    let t = models.textures.get(path, true, &models.data, &mut models.images, models.formats.as_deref())?;
    let m = models.materials.add(nif_material(StandardMaterial { base_color_texture: Some(t), perceptual_roughness: 0.55, reflectance: 0.35, alpha_mode: AlphaMode::Mask(0.5), ..default() }));
    cache.insert(path.to_string(), m.clone());
    Some(m)
}

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
    /// The pot's pieces on the table, the pot they show, their meshes by kind.
    money: Vec<Entity>,
    money_pot: Option<u32>,
    money_meshes: Vec<(Money, Handle<Mesh>)>,
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
    place_money(&mut commands, &mut models, b, s.bet.pot());
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
    Some(Built { entities: vec![holder, camera, lamp], space, camera, planks, layer, card_mesh, cards: HashMap::new(), leaving: Vec::new(), materials: HashMap::new(), view_w, money: Vec::new(), money_pot: None, money_meshes: Vec::new() })
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
    fn the_pot_grows_into_more_money_and_bigger_notes() {
        assert!(money(0).is_empty());
        let small = money(10);
        assert!(small.iter().all(|m| !m.0.note()), "a small pot is caps");
        let mid = money(164);
        let big = money(2000);
        assert!(mid.len() > small.len() && big.len() > mid.len());
        assert!(mid.iter().any(|m| m.0.note()));
        assert!(big.iter().any(|m| m.0 == Money::Note100));
        assert!(mid.iter().any(|m| m.0 == Money::Denarius), "Legion coins in the pot");
        assert!(big.iter().any(|m| m.0 == Money::Aureus));
        assert!(money(100_000).len() <= MONEY_MAX);
        // The pile only grows: a piece is where it was in the smaller pot.
        assert_eq!(&money(170)[..4].iter().map(|m| m.1).collect::<Vec<_>>(), &mid[..4].iter().map(|m| m.1).collect::<Vec<_>>());
        let zones = card_zones(0.0);
        for (kind, at, turn) in &big {
            assert!(at.x > 0.0 && at.x < FRAME_W && at.y > 0.0 && at.y < 960.0);
            assert!(clear_of(&zones, *at, kind.size(), *turn), "{kind:?} at {at} lies where a card can");
        }
        assert!(big.len() > 40, "the big pot still fills the free planks: {}", big.len());
    }

    #[test]
    fn no_cap_or_coin_lies_on_another() {
        for pot in [5, 20, 54, 108, 164, 400, 2000, 100_000] {
            let round: Vec<Vec2> = money(pot).iter().filter(|m| !m.0.note()).map(|m| m.1).collect();
            for (i, a) in round.iter().enumerate() {
                for b in &round[i + 1..] {
                    assert!(a.distance(*b) >= CAP_SIZE.x, "pot {pot}: caps at {a} and {b} overlap");
                }
            }
        }
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
