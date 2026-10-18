//! Blood on the screen: the splats scripts put there (TriggerScreenSplatter,
//! SetScreenSplatterFade, ClearScreenSplatter; docs/spec-notes/screen-splatter.md).
//!
//! The opening is the only caller in the shipped plugins (`QUST 0x00102037` stages 36–39
//! and 50, cleared at stage 100) [observed: decoded scripts]: Benny's shots leave blood
//! over the player's view. Each call puts `count` splats on the screen
//! (`fnv_game::functions::ui::SplatterTrigger`), each at a random place, with a random
//! size and opacity from the game settings (`fBloodSplatter*`, `iBloodSplatterMaxCount`)
//! [observed: the values in FalloutNV.esm; documented: their names; hypothesis: how the
//! names are used, below]:
//!
//! - size: between `fBloodSplatterMinSize` (0.12) and `fBloodSplatterMaxSize` (0.18)
//!   times the call's size factor, as a share of the screen's height (a square);
//! - opacity: between `fBloodSplatterMinOpacity` (0.5) and `fBloodSplatterMaxOpacity`
//!   (1.0), or with the chance `fBloodSplatterOpacityChance` (0.3) between
//!   `fBloodSplatterMinOpacity2` (0.3) and `fBloodSplatterMaxOpacity2` (0.6), times the
//!   call's opacity factor, at most 1;
//! - time: the call's duration (else `fBloodSplatterDuration`, 20 s); from
//!   `fBloodSplatterFadeStart` (0.4) of it, or SetScreenSplatterFade's share, the splat
//!   fades out to nothing at its end;
//! - at most `iBloodSplatterMaxCount` (25) on the screen: a new splat beyond that takes
//!   the place of the oldest.
//!
//! The picture is the game's own textures: the shape is the alpha of a cell of
//! `textures\gore\screenbloodalpha01opt.dds`, the colour the same cell of
//! `textures\gore\screenbloodcolor01opt.dds` (both 2×2 atlases of four splats) [observed:
//! Fallout - Textures2.bsa; hypothesis: the atlas pair, not the single-splat
//! `screenbloodalpha01`–`03` / `screenbloodcolor01`, is the one New Vegas draws, as the
//! Ex variant's "atlas" textures suggest]. The splats lie over the world and under every
//! menu and the HUD. Not built: the flare the settings `fBloodSplatterFlareSize`,
//! `…FlareMult` and `…FlareOffsetScale` describe (its texture and how it is drawn are
//! not known); the blood the engine itself puts on the screen when the player is hurt
//! (ClearScreenBlood's page names it; nothing says when or how much).
//!
//! The splats age with the game's frames while nothing holds the game (the pause menu,
//! a loading screen). `[ScreenSplatter] bScreenSplatterEnabled=1` is the original's INI
//! default [documented: Fallout_default.ini]; there is no switch to turn it off here.
//!
//! Log: `screen splatter: <n> splats on the screen (TriggerScreenSplatter <count>
//! <duration> <size> <opacity>)`, `screen splatter: cleared`, `screen splatter: the last
//! splat has faded`, `screen splatter: <texture>: <why>` when the textures cannot be
//! read (then nothing is drawn).

use crate::sim::SimState;
use crate::PlaySet;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use fnv_game::functions::ui::SplatterTrigger;
use nvf_assets::GameData;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// The splat shapes (alpha) and colours [observed: Fallout - Textures2.bsa].
pub const ALPHA_ATLAS: &str = "textures\\gore\\screenbloodalpha01opt.dds";
pub const COLOR_ATLAS: &str = "textures\\gore\\screenbloodcolor01opt.dds";
/// Cells per side of the atlases (2×2) [observed: four splats in each].
const ATLAS_SIDE: usize = 2;

/// The game settings that shape the splats, with the values FalloutNV.esm gives them
/// [observed 2026-10-18] as defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplatterSettings {
    pub min_size: f32,
    pub max_size: f32,
    pub min_opacity: f32,
    pub max_opacity: f32,
    pub min_opacity2: f32,
    pub max_opacity2: f32,
    pub opacity_chance: f32,
    pub duration: f32,
    pub fade_start: f32,
    pub max_count: usize,
}

impl Default for SplatterSettings {
    fn default() -> Self {
        SplatterSettings { min_size: 0.12, max_size: 0.18, min_opacity: 0.5, max_opacity: 1.0, min_opacity2: 0.3, max_opacity2: 0.6, opacity_chance: 0.3, duration: 20.0, fade_start: 0.4, max_count: 25 }
    }
}

impl SplatterSettings {
    /// From the load order's game settings; a missing one keeps its default.
    pub fn from_settings(get: impl Fn(&str) -> Option<f32>) -> SplatterSettings {
        let d = SplatterSettings::default();
        let f = |name: &str, v: f32| get(name).unwrap_or(v);
        SplatterSettings {
            min_size: f("fBloodSplatterMinSize", d.min_size),
            max_size: f("fBloodSplatterMaxSize", d.max_size),
            min_opacity: f("fBloodSplatterMinOpacity", d.min_opacity),
            max_opacity: f("fBloodSplatterMaxOpacity", d.max_opacity),
            min_opacity2: f("fBloodSplatterMinOpacity2", d.min_opacity2),
            max_opacity2: f("fBloodSplatterMaxOpacity2", d.max_opacity2),
            opacity_chance: f("fBloodSplatterOpacityChance", d.opacity_chance),
            duration: f("fBloodSplatterDuration", d.duration),
            fade_start: f("fBloodSplatterFadeStart", d.fade_start),
            max_count: f("iBloodSplatterMaxCount", d.max_count as f32).max(0.0) as usize,
        }
    }
}

/// One splat on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Splat {
    /// Its centre, as shares of the screen's width and height from the top left.
    pub center: Vec2,
    /// Its side, as a share of the screen's height.
    pub size: f32,
    pub opacity: f32,
    pub duration: f32,
    pub age: f32,
    /// Its atlas cell (0..4).
    pub cell: usize,
}

impl Splat {
    /// Its opacity now: full until the fade starts (`fade`: the share of its time, or
    /// none), then down to 0 at its end.
    pub fn alpha(&self, fade: Option<f32>) -> f32 {
        if self.age >= self.duration {
            return 0.0;
        }
        let Some(start) = fade.map(|s| s.clamp(0.0, 1.0) * self.duration) else { return self.opacity };
        if self.age <= start {
            return self.opacity;
        }
        self.opacity * (1.0 - (self.age - start) / (self.duration - start).max(1e-6))
    }
}

/// A uniform number in [0, 1).
fn unit(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

/// The splats one call puts on the screen.
pub fn splats_for(t: &SplatterTrigger, s: &SplatterSettings, rng: &mut ChaCha8Rng) -> Vec<Splat> {
    let duration = t.duration.unwrap_or(s.duration).max(0.0);
    let n = (t.count as usize).min(s.max_count);
    (0..n)
        .map(|_| {
            let lerp = |a: f32, b: f32, x: f32| a + (b - a) * x;
            let center = Vec2::new(unit(rng), unit(rng));
            let size = lerp(s.min_size, s.max_size, unit(rng)) * t.size;
            let second = unit(rng) < s.opacity_chance;
            let base = if second { lerp(s.min_opacity2, s.max_opacity2, unit(rng)) } else { lerp(s.min_opacity, s.max_opacity, unit(rng)) };
            let cell = (rng.next_u32() as usize) % (ATLAS_SIDE * ATLAS_SIDE);
            Splat { center, size, opacity: (base * t.opacity).clamp(0.0, 1.0), duration, age: 0.0, cell }
        })
        .collect()
}

/// Add new splats; beyond `max` the oldest go.
pub fn add_splats(on: &mut Vec<Splat>, new: Vec<Splat>, max: usize) {
    on.extend(new);
    if on.len() > max {
        on.drain(..on.len() - max);
    }
}

/// The four splat pictures: each atlas cell's colour with the shape's alpha (RGBA8).
pub fn compose(alpha: &[u8], color: &[u8], side: usize) -> Vec<Vec<u8>> {
    let cell = side / ATLAS_SIDE;
    (0..ATLAS_SIDE * ATLAS_SIDE)
        .map(|c| {
            let (cx, cy) = ((c % ATLAS_SIDE) * cell, (c / ATLAS_SIDE) * cell);
            let mut px = Vec::with_capacity(cell * cell * 4);
            for y in 0..cell {
                for x in 0..cell {
                    let i = ((cy + y) * side + cx + x) * 4;
                    px.extend_from_slice(&[color[i], color[i + 1], color[i + 2], alpha[i + 3]]);
                }
            }
            px
        })
        .collect()
}

#[derive(Resource)]
pub struct ScreenSplatter {
    pub splats: Vec<Splat>,
    /// SetScreenSplatterFade's share and switch, else the game setting's share.
    fade: Option<(f32, bool)>,
    settings: Option<SplatterSettings>,
    /// The four splat pictures, once read (`Err`: they cannot be).
    pictures: Option<Result<Vec<Handle<Image>>, String>>,
    clears_seen: u64,
    rng: ChaCha8Rng,
    root: Option<Entity>,
}

impl Default for ScreenSplatter {
    fn default() -> Self {
        ScreenSplatter { splats: Vec::new(), fade: None, settings: None, pictures: None, clears_seen: 0, rng: ChaCha8Rng::seed_from_u64(0x5B1A_77E2), root: None }
    }
}

impl ScreenSplatter {
    /// The share of a splat's time after which it fades, or none.
    fn fade_share(&self, s: &SplatterSettings) -> Option<f32> {
        match self.fade {
            Some((share, true)) => Some(share),
            Some((_, false)) => None,
            None => Some(s.fade_start),
        }
    }
}

#[derive(Component)]
struct SplatRoot;
#[derive(Component)]
struct SplatPicture(usize);

pub struct ScreenSplatterPlugin;

impl Plugin for ScreenSplatterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScreenSplatter>().add_systems(Update, (take_requests, age, draw).chain().in_set(PlaySet::Ui));
    }
}

/// Read and compose the splat pictures.
fn read_pictures(data: &GameData, images: &mut Assets<Image>) -> Result<Vec<Handle<Image>>, String> {
    let decode = |path: &str| -> Result<(usize, Vec<u8>), String> {
        let bytes = data.read(path).ok_or_else(|| format!("{path}: not in the game's files"))?;
        let dds = fnv_dds::Dds::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let (w, h) = (dds.header.width, dds.header.height);
        if w != h || !(w as usize).is_multiple_of(ATLAS_SIDE) {
            return Err(format!("{path}: {w}x{h}, not a square atlas"));
        }
        let rgba = fnv_dds::decode_blocks(dds.format, w, h, dds.levels[0][0]).or_else(|| fnv_dds::to_rgba8(dds.format, dds.levels[0][0])).ok_or_else(|| format!("{path}: its format is not decoded"))?;
        Ok((w as usize, rgba))
    };
    let (side, alpha) = decode(ALPHA_ATLAS)?;
    let (side2, color) = decode(COLOR_ATLAS)?;
    if side != side2 {
        return Err(format!("{COLOR_ATLAS}: {side2} wide, the shapes {side}"));
    }
    let cell = (side / ATLAS_SIDE) as u32;
    Ok(compose(&alpha, &color, side)
        .into_iter()
        .map(|px| images.add(Image::new(Extent3d { width: cell, height: cell, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)))
        .collect())
}

/// The scripts' calls (`UiRequests::splatter`): clear, fade, new splats.
fn take_requests(mut splatter: ResMut<ScreenSplatter>, script: Option<ResMut<crate::script::ScriptState>>, sim: Option<Res<SimState>>) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let asked = &mut rt.game.ui.splatter;
    let sp = &mut *splatter;
    if asked.clears != sp.clears_seen {
        sp.clears_seen = asked.clears;
        if !sp.splats.is_empty() {
            info!("screen splatter: cleared");
        }
        sp.splats.clear();
    }
    sp.fade = asked.fade;
    if asked.triggers.is_empty() {
        return;
    }
    let settings = *sp.settings.get_or_insert_with(|| sim.as_deref().map_or_else(SplatterSettings::default, |s| SplatterSettings::from_settings(|n| s.forms.setting_f32(n))));
    for t in std::mem::take(&mut asked.triggers) {
        let new = splats_for(&t, &settings, &mut sp.rng);
        add_splats(&mut sp.splats, new, settings.max_count);
        info!(
            "screen splatter: {} splats on the screen (TriggerScreenSplatter {} {} {} {})",
            sp.splats.len(),
            t.count,
            t.duration.unwrap_or(settings.duration),
            t.size,
            t.opacity
        );
    }
}

/// The splats age while nothing holds the game; spent ones go.
fn age(time: Res<Time>, paused: Res<crate::pause::Paused>, loading: Res<crate::loading::Loading>, mut splatter: ResMut<ScreenSplatter>) {
    if splatter.splats.is_empty() || paused.0 || loading.active() {
        return;
    }
    let dt = time.delta_secs();
    for s in &mut splatter.splats {
        s.age += dt;
    }
    splatter.splats.retain(|s| s.age < s.duration);
    if splatter.splats.is_empty() {
        info!("screen splatter: the last splat has faded");
    }
}

/// Draw the splats: one picture each over the screen, under the HUD and menus.
fn draw(
    mut commands: Commands,
    mut splatter: ResMut<ScreenSplatter>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pictures: Query<(&SplatPicture, &mut Node, &mut ImageNode)>,
) {
    let sp = &mut *splatter;
    if sp.splats.is_empty() {
        if let Some(root) = sp.root.take() {
            commands.entity(root).despawn();
        }
        return;
    }
    let Some(data) = data else { return };
    let handles = match sp.pictures.get_or_insert_with(|| read_pictures(&data, &mut images)) {
        Ok(h) => h.clone(),
        Err(e) => {
            warn!("screen splatter: {e}; nothing is drawn");
            sp.splats.clear();
            return;
        }
    };
    let Ok(w) = windows.single() else { return };
    let (width, height) = (w.width(), w.height());
    let settings = sp.settings.unwrap_or_default();
    let fade = sp.fade_share(&settings);
    // The pictures follow the splats: rebuilt when their number changed.
    let count = pictures.iter().count();
    if sp.root.is_none() || count != sp.splats.len() {
        if let Some(root) = sp.root.take() {
            commands.entity(root).despawn();
        }
        let splats = sp.splats.clone();
        let root = commands
            .spawn((SplatRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, GlobalZIndex(-1), Pickable::IGNORE))
            .with_children(|r| {
                for (i, s) in splats.iter().enumerate() {
                    let (node, image) = layout(s, &handles, fade, width, height);
                    r.spawn((SplatPicture(i), image, node, Pickable::IGNORE));
                }
            })
            .id();
        sp.root = Some(root);
    }
    for (pic, mut node, mut image) in &mut pictures {
        let Some(s) = sp.splats.get(pic.0) else { continue };
        (*node, *image) = layout(s, &handles, fade, width, height);
    }
}

/// A splat's place, size and picture on a window of `width` × `height`.
fn layout(s: &Splat, handles: &[Handle<Image>], fade: Option<f32>, width: f32, height: f32) -> (Node, ImageNode) {
    let side = s.size * height;
    let node = Node { position_type: PositionType::Absolute, width: Val::Px(side), height: Val::Px(side), left: Val::Px(s.center.x * width - side / 2.0), top: Val::Px(s.center.y * height - side / 2.0), ..default() };
    let image = ImageNode { image_mode: NodeImageMode::Stretch, color: Color::srgba(1.0, 1.0, 1.0, s.alpha(fade)), ..ImageNode::new(handles[s.cell % handles.len()].clone()) };
    (node, image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(count: u32, duration: Option<f32>, size: f32, opacity: f32) -> SplatterTrigger {
        SplatterTrigger { count, duration, size, opacity }
    }

    #[test]
    fn a_call_puts_its_count_on_the_screen_within_the_settings_ranges() {
        let s = SplatterSettings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let splats = splats_for(&trigger(12, Some(25.0), 2.0, 1.0), &s, &mut rng);
        assert_eq!(splats.len(), 12);
        for p in &splats {
            assert!(p.size >= s.min_size * 2.0 && p.size <= s.max_size * 2.0, "{p:?}");
            assert!(p.opacity >= s.min_opacity2 && p.opacity <= s.max_opacity, "{p:?}");
            assert!((0.0..1.0).contains(&p.center.x) && (0.0..1.0).contains(&p.center.y));
            assert_eq!(p.duration, 25.0);
            assert!(p.cell < 4);
        }
        // The opacity factor, at most 1; no duration: the setting's.
        let strong = splats_for(&trigger(5, None, 1.0, 2.0), &s, &mut rng);
        assert!(strong.iter().all(|p| p.opacity >= 0.6 && p.opacity <= 1.0 && p.duration == 20.0));
    }

    #[test]
    fn the_screen_holds_at_most_the_max_count_and_new_splats_replace_the_oldest() {
        let s = SplatterSettings::default();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut on = Vec::new();
        add_splats(&mut on, splats_for(&trigger(50, Some(25.0), 2.0, 2.0), &s, &mut rng), s.max_count);
        assert_eq!(on.len(), 25);
        add_splats(&mut on, splats_for(&trigger(10, Some(21.0), 2.0, 2.0), &s, &mut rng), s.max_count);
        assert_eq!(on.len(), 25);
        assert_eq!(on.iter().filter(|p| p.duration == 21.0).count(), 10);
    }

    #[test]
    fn splats_fade_from_the_fade_share_to_nothing_at_their_end() {
        let mut p = Splat { center: Vec2::splat(0.5), size: 0.2, opacity: 0.8, duration: 10.0, age: 0.0, cell: 0 };
        assert_eq!(p.alpha(Some(0.5)), 0.8);
        p.age = 5.0;
        assert_eq!(p.alpha(Some(0.5)), 0.8);
        p.age = 7.5;
        assert!((p.alpha(Some(0.5)) - 0.4).abs() < 1e-6);
        assert_eq!(p.alpha(None), 0.8, "no fade: full to the end");
        p.age = 10.0;
        assert_eq!(p.alpha(None), 0.0);
    }

    #[test]
    fn settings_come_from_the_game_settings() {
        let s = SplatterSettings::from_settings(|n| match n {
            "fBloodSplatterDuration" => Some(5.0),
            "iBloodSplatterMaxCount" => Some(3.0),
            _ => None,
        });
        assert_eq!(s.duration, 5.0);
        assert_eq!(s.max_count, 3);
        assert_eq!(s.min_size, 0.12);
    }

    #[test]
    fn each_picture_is_a_cell_colour_with_the_shape_alpha() {
        // A 4×4 atlas: 2×2 cells of 2×2 texels; colour red = the cell index, alpha = 10 + cell.
        let side = 4;
        let mut alpha = vec![0u8; side * side * 4];
        let mut color = vec![0u8; side * side * 4];
        for y in 0..side {
            for x in 0..side {
                let c = (y / 2) * 2 + x / 2;
                let i = (y * side + x) * 4;
                color[i] = c as u8;
                color[i + 3] = 255;
                alpha[i] = 99;
                alpha[i + 3] = 10 + c as u8;
            }
        }
        let cells = compose(&alpha, &color, side);
        assert_eq!(cells.len(), 4);
        for (c, px) in cells.iter().enumerate() {
            assert_eq!(px.len(), 16);
            assert!(px.chunks(4).all(|t| t[0] == c as u8 && t[3] == 10 + c as u8));
        }
    }
}
