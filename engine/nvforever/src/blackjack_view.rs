//! The blackjack table (`nvf_play::blackjack`, docs/spec-notes/blackjack.md §6): the
//! game's models of the casino's table, the dealer's cards and the player's two hands,
//! drawn by a camera of their own into a picture the menu shows behind its text
//! ([`nvf_play::blackjack::BlackjackView`]).
//!
//! - The table is the casino's MOD3 model (`NV_Blackjack-Table<x>.NIF`); the cards are
//!   `NV_Blackjack-Dealer.NIF`, `-Hand1.NIF` and `-Hand2.NIF`, seven cards each, all at the
//!   shoe until a sequence moves them [observed: the files' nodes and sequences].
//! - A card dealt is posed by its own deal sequence (`Hand1_03` for the player's third)
//!   from when it left the shoe and held at its end; the hole card (`Dealer_02`) by
//!   `Dealer_Reveal` once turned; the last round's cards by `<model>_Discard` as the next
//!   deal sweeps them away. Cards not on the table are hidden [hypothesis: each sequence
//!   holds every card of its model; NVForever takes from it only its own card's track].
//! - A card's face is the shape named `<card>:n`: its picture is set to the card's face
//!   from the casino's deck (`CasinoDef::face_texture`) [hypothesis: the face shapes
//!   carry a stand-in picture]. The back and shadow are the file's.
//! - The bet's chips: the game's chip models (`NV_Blackjack-Chip_100/025/010/005/001.NIF`).
//!   Each is a column 2 units high and 1.7 across whose top lies just under the felt, each
//!   value at its own place [observed: the files' nodes and bounds], so the original places
//!   them itself. NVForever draws each chip as its model squeezed to [`CHIP_THICK`], stacked
//!   by value on the spot the 1-chip model's node marks, (−9.9, 0.25), beside the player's
//!   cards where the owner's screenshot has the bet [hypothesis].
//! - The table's two green arrows (`Hand1_Arrow:0`, `Hand1_Arrow:0@#2`) are shown only
//!   after a split, the one at the hand being played [hypothesis: they mark the active
//!   hand; the owner's screenshot of an unsplit hand shows none; which arrow is which hand
//!   by their places].
//! - The camera looks from the player's side over the player's cards towards the dealer's
//!   [design: fitted to the screenshot], with lights of its own on render layer
//!   [`TABLE_LAYER`].

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_game::blackjack::{Card, Seat};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::{ModelCtx, Rig};
use nvf_assets::nif_material::NifMaterial;
use nvf_play::blackjack::{BlackjackScreen, BlackjackView, Shown, Session};
use nvf_play::pause::AidTime;
use std::collections::HashMap;

pub const DEALER_MODEL: &str = "meshes\\terminals\\nv_blackjack\\nv_blackjack-dealer.nif";
pub const HAND_MODELS: [&str; 2] = ["meshes\\terminals\\nv_blackjack\\nv_blackjack-hand1.nif", "meshes\\terminals\\nv_blackjack\\nv_blackjack-hand2.nif"];
/// The chips by value, highest first [observed: the CSNO records' model list].
pub const CHIPS: [(u32, &str); 5] = [
    (100, "meshes\\terminals\\nv_blackjack\\nv_blackjack-chip_100.nif"),
    (25, "meshes\\terminals\\nv_blackjack\\nv_blackjack-chip_025.nif"),
    (10, "meshes\\terminals\\nv_blackjack\\nv_blackjack-chip_010.nif"),
    (5, "meshes\\terminals\\nv_blackjack\\nv_blackjack-chip_005.nif"),
    (1, "meshes\\terminals\\nv_blackjack\\nv_blackjack-chip_001.nif"),
];
/// The render layer of the table, its lights and its camera (lockpicking has 7).
pub const TABLE_LAYER: usize = 9;
/// Where the table is put, far from the world (Bevy space) [design].
const PLACE: Vec3 = Vec3::new(200_000.0, -200_000.0, 0.0);
/// The camera (game units, the table's space): aimed between the player's first card and
/// the dealer's, from this far back towards the player and this high, with this field of
/// view [design: fitted by eye to the owner's screenshot].
const CAMERA_BACK: f32 = 26.0;
const CAMERA_UP: f32 = 26.0;
const AIM_TOWARDS_DEALER: f32 = 0.4;
const FOV_DEG: f32 = 50.0;
/// Each chip this thick, the stacks this far apart [hypothesis: by the screenshot's look].
const CHIP_THICK: f32 = 0.3;
const CHIP_STACK_GAP: f32 = 2.0;
/// The bet's spot in the table's space [observed: `Chips_1`'s place in
/// NV_Blackjack-Chip_001.NIF], and the felt's height under it [hypothesis: a little under
/// where the cards land].
const BET_SPOT: Vec2 = Vec2::new(-9.93, 0.25);
const FELT_BELOW_CARDS: f32 = 0.12;
/// Most chips in one stack [design].
const STACK_MAX: u32 = 20;

pub struct BlackjackViewPlugin;

impl Plugin for BlackjackViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync));
    }
}

/// A card model as drawn: its rig, its nodes' entities by name, and its seven cards' node
/// indices, face shapes and the picture each face shows now.
struct CardModel {
    rig: Rig,
    /// The model's prefix: `Dealer`, `Hand1`, `Hand2`.
    prefix: &'static str,
    cards: Vec<CardSlot>,
}

struct CardSlot {
    node: usize,
    entity: Entity,
    faces: Vec<Entity>,
    showing: Option<Card>,
}

impl CardModel {
    fn new(rig: Rig, prefix: &'static str, by_name: &HashMap<String, Entity>, shapes: &[Entity]) -> Option<CardModel> {
        let mut cards = Vec::new();
        for n in 1..=7 {
            let name = format!("{prefix}_{n:02}");
            let node = node_index(&rig, &name)?;
            let entity = *by_name.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name))?.1;
            // The face: a shape named `<card>:n` (not the back's or the shadow's).
            let face_prefix = format!("{name}:").to_ascii_lowercase();
            let faces = rig
                .shapes
                .iter()
                .zip(shapes)
                .filter(|(s, _)| rig.nodes[s.node].name.as_deref().is_some_and(|m| m.to_ascii_lowercase().starts_with(&face_prefix)) || (s.node == node))
                .map(|(_, e)| *e)
                .collect();
            cards.push(CardSlot { node, entity, faces, showing: None });
        }
        Some(CardModel { rig, prefix, cards })
    }

    fn clip(&self, name: &str) -> Option<&Clip> {
        self.rig.group(name)
    }
}

#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    models: Vec<CardModel>,
    /// The table's space (its root) and where the player's first card and the dealer's
    /// land, in it.
    chips_root: Option<Entity>,
    /// The table's arrows at the first and the split hand.
    arrows: [Option<Entity>; 2],
    chips_for: Option<u32>,
    player_spot: Vec3,
    dealer_spot: Vec3,
    materials: HashMap<(Entity, Card), Handle<NifMaterial>>,
    warned: bool,
}

fn node_index(rig: &Rig, name: &str) -> Option<usize> {
    rig.nodes.iter().position(|n| n.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name)))
}

/// A node's local transform at `t` of `clip`, else its stored one.
fn sampled(rig: &Rig, clip: Option<&Clip>, i: usize, t: f32) -> Transform {
    let mut tr = rig.nodes[i].local;
    let Some(name) = rig.nodes[i].name.as_deref() else { return tr };
    let Some((_, track)) = clip.and_then(|c| c.tracks.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))) else { return tr };
    if let Some(v) = track.translation.sample(t) {
        tr.translation = Vec3::from_array(v);
    }
    if let Some([w, x, y, z]) = track.rotation_at(t) {
        tr.rotation = Quat::from_xyzw(x, y, z, w).normalize();
    }
    if let Some([s]) = track.scale.sample(t) {
        tr.scale = Vec3::splat(s);
    }
    tr
}

/// Time `t` seconds into a clip, played once and held at its end.
fn clip_time(clip: Option<&Clip>, t: f32) -> f32 {
    clip.map_or(0.0, |c| (c.start + t.max(0.0) * c.frequency).min(c.stop))
}

/// Where a node ends up (model space) at the end of a clip.
fn end_of(rig: &Rig, clip: Option<&Clip>, i: usize) -> Vec3 {
    let t = clip.map_or(0.0, |c| c.stop);
    let mut chain = vec![i];
    while let Some(p) = rig.nodes[*chain.last().unwrap()].parent {
        if p == 0 {
            break;
        }
        chain.push(p);
    }
    let mut m = Affine3A::IDENTITY;
    for &n in chain.iter().rev() {
        m *= sampled(rig, clip, n, t).compute_affine();
    }
    m.translation.into()
}

/// A card on the table: seconds since it was dealt, and since it started to be swept off.
type Place = (f32, Option<f32>);

fn model_of(seat: Seat) -> usize {
    match seat {
        Seat::Dealer => 0,
        Seat::Hand(h) => 1 + h.min(1),
    }
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    screen: Res<BlackjackScreen>,
    view: Option<Res<BlackjackView>>,
    mut models: ModelCtx,
    window: Query<&Window, With<PrimaryWindow>>,
    time: AidTime,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut mats: Query<&mut MeshMaterial3d<NifMaterial>>,
    mut drawn: Local<Drawn>,
) {
    let Some(s) = screen.0.as_ref() else {
        if !drawn.entities.is_empty() || view.is_some() {
            for e in drawn.entities.drain(..) {
                commands.entity(e).despawn();
            }
            *drawn = Drawn { warned: drawn.warned, ..default() };
            commands.remove_resource::<BlackjackView>();
        }
        return;
    };
    if drawn.entities.is_empty() && !build(&mut commands, s, &mut models, &window, &mut drawn) {
        return;
    }
    let now = time.elapsed_secs();
    pose(s, now, &drawn, &mut transforms, &mut visibility);
    // The arrow at the hand being played, after a split.
    let split = s.table.hands.len() == 2 && s.table.phase == fnv_game::blackjack::Phase::Playing;
    for (i, a) in drawn.arrows.iter().enumerate() {
        if let Some(mut v) = a.and_then(|e| visibility.get_mut(e).ok()) {
            v.set_if_neq(if split && s.table.active == i { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    faces(s, &mut drawn, &mut models, &mut mats);
    chips(&mut commands, s, &mut drawn, &mut models);
}

/// Spawn the table, the card models, the camera and the lights.
fn build(commands: &mut Commands, s: &Session, models: &mut ModelCtx, window: &Query<&Window, With<PrimaryWindow>>, drawn: &mut Drawn) -> bool {
    let table_path = s.def.table_model.as_deref().map(|p| format!("meshes\\{p}")).unwrap_or_else(|| "meshes\\terminals\\nv_blackjack\\nv_blackjack-table.nif".into());
    let paths = [table_path.as_str(), DEALER_MODEL, HAND_MODELS[0], HAND_MODELS[1]];
    let loaded: Vec<_> = paths.iter().map(|p| models.model(p)).collect();
    let rigs: Option<Vec<Rig>> = loaded.iter().map(|m| m.as_ref().and_then(|m| m.rig.clone())).collect();
    let Some(mut rigs) = rigs else {
        if !drawn.warned {
            warn!("blackjack view: a model of {paths:?} is missing or has no sequences; the menu is drawn without the table");
            drawn.warned = true;
        }
        return false;
    };
    let size = window.single().map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 });
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let layer = RenderLayers::layer(TABLE_LAYER);
    let holder = commands.spawn((Name::new("Blackjack table"), Transform::from_translation(PLACE), Visibility::Inherited)).id();
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let to_bevy = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    let hand2 = rigs.pop().unwrap();
    let hand1 = rigs.pop().unwrap();
    let dealer = rigs.pop().unwrap();
    let table = rigs.pop().unwrap();
    let (_, table_drawn, _) = table.spawn_nodes(commands, holder, to_bevy, &table_path, 0.0);
    let mut arrows = [None, None];
    for (sh, e) in table.shapes.iter().zip(&table_drawn) {
        match table.nodes[sh.node].name.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("hand1_arrow:0") => arrows[0] = Some(*e),
            Some("hand1_arrow:0@#2") => arrows[1] = Some(*e),
            _ => continue,
        }
        commands.entity(*e).insert(Visibility::Hidden);
    }
    let mut shapes_total = table_drawn.len();
    for e in &table_drawn {
        commands.entity(*e).insert(layer.clone());
    }
    let mut card_models = Vec::new();
    for (rig, prefix, path) in [(dealer, "Dealer", DEALER_MODEL), (hand1, "Hand1", HAND_MODELS[0]), (hand2, "Hand2", HAND_MODELS[1])] {
        let (_, shapes, by_name) = rig.spawn_nodes(commands, holder, to_bevy, path, 0.0);
        shapes_total += shapes.len();
        for e in &shapes {
            commands.entity(*e).insert(layer.clone());
        }
        match CardModel::new(rig, prefix, &by_name, &shapes) {
            Some(m) => {
                for c in &m.cards {
                    commands.entity(c.entity).insert(Visibility::Hidden);
                }
                card_models.push(m);
            }
            None => {
                warn!("blackjack view: {path} lacks the nodes {prefix}_01 to _07");
                commands.entity(holder).despawn();
                drawn.warned = true;
                return false;
            }
        }
    }
    let first = |m: &CardModel, clip: &str| end_of(&m.rig, m.clip(clip), m.cards[0].node);
    let player_spot = first(&card_models[1], "Hand1_01");
    let dealer_spot = first(&card_models[0], "Dealer_01");
    // From the dealer towards the player, flat on the felt.
    let towards_player = Vec3::new(player_spot.x - dealer_spot.x, player_spot.y - dealer_spot.y, 0.0).normalize_or(Vec3::NEG_X);
    let aim = player_spot.lerp(dealer_spot, AIM_TOWARDS_DEALER);
    let eye = aim + towards_player * CAMERA_BACK + Vec3::Z * CAMERA_UP;
    let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
    let camera = commands
        .spawn((
            Name::new("Blackjack camera"),
            Camera3d::default(),
            Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            Projection::Perspective(PerspectiveProjection { fov: FOV_DEG.to_radians(), near: 1.0, far: 3000.0, ..default() }),
            Tonemapping::None,
            Msaa::Sample4,
            AmbientLight { color: Color::WHITE, brightness: 120.0, ..default() },
            Transform::from_translation(to(eye)).looking_at(to(aim), Vec3::Y),
            layer.clone(),
        ))
        .id();
    // A warm light from above the dealer's side and a dim fill from the player's
    // [design: the casino's lamps are not in the files; the screenshot is dim and warm].
    let centre = player_spot.lerp(dealer_spot, 0.5);
    let key = commands
        .spawn((
            DirectionalLight { illuminance: 2200.0, color: Color::srgb(1.0, 0.88, 0.72), ..default() },
            Transform::from_translation(to(centre + Vec3::new(20.0, 10.0, 80.0))).looking_at(to(centre), Vec3::Y),
            layer.clone(),
        ))
        .id();
    let fill = commands
        .spawn((
            DirectionalLight { illuminance: 500.0, ..default() },
            Transform::from_translation(to(eye + Vec3::Z * 30.0)).looking_at(to(centre), Vec3::Y),
            layer.clone(),
        ))
        .id();
    let chips_root = commands.spawn((Transform::default(), Visibility::Inherited)).id();
    commands.entity(holder).add_child(chips_root);
    drawn.entities = vec![holder, camera, key, fill];
    drawn.models = card_models;
    drawn.player_spot = player_spot;
    drawn.dealer_spot = dealer_spot;
    drawn.chips_root = Some(chips_root);
    drawn.arrows = arrows;
    commands.insert_resource(BlackjackView { image });
    info!("blackjack view: the table built for {} ({table_path}, {shapes_total} shapes); first cards land at {player_spot:?} (player) and {dealer_spot:?} (dealer)", s.def.name);
    true
}

/// Each card on the table posed by its sequence; the others hidden.
fn pose(s: &Session, now: f32, drawn: &Drawn, transforms: &mut Query<&mut Transform>, visibility: &mut Query<&mut Visibility>) {
    let mut on_table: [[Option<Place>; 7]; 3] = [[None; 7]; 3];
    // The last round's cards: their discard sequence.
    if let Some((since, cards)) = &s.discard {
        for c in cards {
            if c.index < 7 {
                on_table[model_of(c.seat)][c.index] = Some((-1.0, Some(now - since)));
            }
        }
    }
    for c in &s.shown {
        if c.index < 7 && now >= c.since {
            on_table[model_of(c.seat)][c.index] = Some((now - c.since, None));
        }
    }
    for (m, slots) in drawn.models.iter().zip(&on_table) {
        for (k, (card, at)) in m.cards.iter().zip(slots).enumerate() {
            let shown = at.is_some();
            if let Ok(mut v) = visibility.get_mut(card.entity) {
                v.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
            let Some((dealt, discard)) = *at else { continue };
            let tr = match discard {
                Some(t) => {
                    let clip = m.clip(&format!("{}_Discard", m.prefix));
                    sampled(&m.rig, clip, card.node, clip_time(clip, t))
                }
                None if m.prefix == "Dealer" && k == 1 && s.reveal.is_some_and(|r| now >= r) => {
                    let clip = m.clip("Dealer_Reveal");
                    sampled(&m.rig, clip, card.node, clip_time(clip, now - s.reveal.unwrap_or(now)))
                }
                None => {
                    let clip = m.clip(&format!("{}_{:02}", m.prefix, k + 1));
                    sampled(&m.rig, clip, card.node, clip_time(clip, dealt))
                }
            };
            if let Ok(mut t) = transforms.get_mut(card.entity) {
                *t = tr;
            }
        }
    }
}

/// Each card's face picture: the card it shows now.
fn faces(s: &Session, drawn: &mut Drawn, models: &mut ModelCtx, mats: &mut Query<&mut MeshMaterial3d<NifMaterial>>) {
    let mut want: [[Option<Card>; 7]; 3] = [[None; 7]; 3];
    if let Some((_, cards)) = &s.discard {
        for c in cards.iter().filter(|c| c.index < 7) {
            want[model_of(c.seat)][c.index] = Some(c.card);
        }
    }
    for c in s.shown.iter().filter(|c: &&Shown| c.index < 7) {
        want[model_of(c.seat)][c.index] = Some(c.card);
    }
    let Drawn { models: card_models, materials, .. } = drawn;
    for (m, want) in card_models.iter_mut().zip(want) {
        for (slot, card) in m.cards.iter_mut().zip(want) {
            let Some(card) = card else { continue };
            if slot.showing == Some(card) {
                continue;
            }
            slot.showing = Some(card);
            let Some(path) = s.def.face_texture(card).map(|p| format!("textures\\{p}")) else { continue };
            for &e in &slot.faces {
                let Ok(mut mm) = mats.get_mut(e) else { continue };
                let handle = match materials.get(&(e, card)) {
                    Some(h) => h.clone(),
                    None => {
                        let Some(tex) = models.textures.get(&path, true, &models.data, &mut models.images, models.formats.as_deref()) else {
                            warn!("blackjack view: no card picture {path}");
                            continue;
                        };
                        let Some(mut mat) = models.materials.get(&mm.0).cloned() else { continue };
                        mat.base.base_color_texture = Some(tex);
                        let h = models.materials.add(mat);
                        materials.insert((e, card), h.clone());
                        h
                    }
                };
                mm.0 = handle;
            }
        }
    }
}

/// The chips staked, in stacks by value in front of the player's cards.
fn chips(commands: &mut Commands, s: &Session, drawn: &mut Drawn, models: &mut ModelCtx) {
    let staked = if s.betting() { s.bet } else { s.table.staked() };
    if drawn.chips_for == Some(staked) {
        return;
    }
    drawn.chips_for = Some(staked);
    let Some(root) = drawn.chips_root else { return };
    commands.entity(root).despawn_related::<Children>();
    let towards_player = Vec3::new(drawn.player_spot.x - drawn.dealer_spot.x, drawn.player_spot.y - drawn.dealer_spot.y, 0.0).normalize_or(Vec3::NEG_X);
    // Stacks side by side across the player's view, away from the cards.
    let side = Vec3::Z.cross(towards_player);
    let layer = RenderLayers::layer(TABLE_LAYER);
    let mut left = staked;
    let mut stack = 0.0;
    for (value, path) in CHIPS {
        let mut n = left / value;
        left -= n * value;
        let Some(model) = models.model(path) else { continue };
        while n > 0 {
            let here = n.min(STACK_MAX);
            n -= here;
            for i in 0..here {
                let Some((lo, hi)) = model.bounds else { continue };
                let felt = drawn.player_spot.z - FELT_BELOW_CARDS;
                let p = BET_SPOT.extend(felt) + side * stack + Vec3::Z * (i as f32 * CHIP_THICK);
                let centre = Vec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
                let squeeze = CHIP_THICK / (hi.z - lo.z).max(0.01);
                // In the game's space: the model's foot centre to the origin, squeezed, then
                // to its place; into Bevy's space as the table's root does.
                let game = Mat4::from_translation(p) * Mat4::from_scale(Vec3::new(1.0, 1.0, squeeze)) * Mat4::from_translation(-centre);
                let tr = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)) * Transform::from_matrix(game);
                let chip = commands.spawn((tr, Visibility::Inherited)).id();
                commands.entity(root).add_child(chip);
                for sh in &model.shapes {
                    let shape = commands.spawn((Transform::from_matrix(Mat4::from(sh.transform)), Mesh3d(sh.mesh.clone()), MeshMaterial3d(sh.material.clone()), layer.clone())).id();
                    commands.entity(chip).add_child(shape);
                }
            }
            stack += CHIP_STACK_GAP;
        }
    }
}
