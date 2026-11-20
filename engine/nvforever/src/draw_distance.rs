//! The draw-distance sliders of the settings page (Display; owner request 2026-10-16):
//! how close to the camera objects, actors and items must be to be drawn, how far the
//! sun's shadows reach and how far grass grows, each as a part of its full distance. At
//! 100% each draws as before the sliders; 50% draws to half the distance, and so on.
//!
//! The full distances [design]:
//! - **Objects, actors, items**: nothing culled them before but their cells, so the full
//!   distance is the farthest a loaded cell reaches, `(--radius + 1)` cells along both
//!   axes from the camera's cell (the diagonal). A mesh of a placed reference is drawn
//!   while its origin is within the slider's part of that, and dithers out over the
//!   [`FADE`] part beyond it. Actors are the NPCs and creatures (their bodies, worn and
//!   held items); items are the references whose base is an item a player can take
//!   (weapons, armour, ammo, aid, books, notes, keys, mods, chips, caps, cards) and the
//!   items dropped in the game; objects are every other placed model of a loaded cell.
//!   Imposters, distant LOD, terrain, water and trees are not touched.
//! - **Shadows**: `--shadow-distance` (the sun's shadow cascades end at the slider's part
//!   of it).
//! - **Grass**: `--grass-distance` and `--grass-fade-range` together.
//!
//! How it culls: Bevy's visibility range, which cell fades (`fade.rs`) and door fades
//! (`door_fade.rs`) already give every streamed mesh. A mesh at rest (fully faded in)
//! gets the range [`fade::drawn_within`] its class's distance instead of the open-ended
//! one; one still fading is left to its fade and gets its range when the fade ends. A
//! mesh beyond its range is culled on the CPU too, and casts no shadow then. At 100% the
//! range is the open-ended one again, so nothing changes against the sliders' absence.
//!
//! The sliders are kept in `NVForever.ini` [Settings] like the page's switches
//! (`nvf_play::settings`).
//!
//! **The split-screen profile** (plan `split-screen.md` §11; owner 2026-11-20: "lower
//! settings while split ... view distance and shadows drop a bit"): while split screen is
//! on in this game (player 1's while it is on, player 2's always), objects, actors and
//! items are drawn to [`SPLIT_VIEW`] of what their sliders say and the sun's shadows reach
//! [`SPLIT_SHADOWS`] of theirs ([`split_profile`]). The sliders, and so the INI, keep the
//! player's own values; only the distances in use change, and ending split screen gives
//! them back. Grass, imposters, distant LOD, terrain and water are not touched.

use crate::fade;
use crate::grass::GrassConfig;
use crate::limbs::ActorRig;
use crate::sky::SkyState;
use crate::world::{CellEntity, WorldData};
use crate::Args;
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use fnv_world::placed::{base_object, placement};
use fnv_world::CELL_SIZE;
use nvf_assets::NifMaterial;
use nvf_play::dropped::DroppedRoot;
use nvf_play::RefVisual;
use std::collections::HashMap;

/// The part of a mesh's draw distance over which it dithers out beyond it [design].
pub const FADE: f32 = 0.1;

/// The split-screen profile's view and shadow distances, parts of the player's own
/// (module notes) [design: plan `split-screen.md` §11].
pub const SPLIT_VIEW: f32 = 0.75;
pub const SPLIT_SHADOWS: f32 = 0.70;

/// The sliders' least value and step.
pub const MIN: f32 = 0.1;
pub const STEP: f32 = 0.05;

/// The base object types that count as items [documented: the item records, as
/// `nvf_play::world_sounds`' pick-up sounds; MISC is the clutter a player can take].
const ITEM_SIGS: [&[u8; 4]; 12] = [b"MISC", b"KEYM", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"NOTE", b"IMOD", b"CHIP", b"CCRD", b"CMNY"];

/// What a reference's mesh counts as for the sliders.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrawClass {
    Object,
    Actor,
    Item,
    /// Not under a loaded cell (an imposter): drawn as before.
    Unranged,
}

/// The sliders' values (each a part of its full distance, [`MIN`] to 1) and the full
/// distances (module notes).
#[derive(Resource, Clone, Debug)]
pub struct DrawDistances {
    pub objects: f32,
    pub actors: f32,
    pub items: f32,
    pub shadows: f32,
    pub grass: f32,
    /// The farthest a loaded cell reaches from the camera (game units).
    pub reach: f32,
    /// `--shadow-distance`.
    pub shadow_full: f32,
    /// `--grass-distance` and `--grass-fade-range`.
    pub grass_full: (f32, f32),
    /// Split screen is on in this game: the split profile applies ([`split_profile`]).
    pub split: bool,
}

/// The parts of their full distances in use for objects, actors, items and the sun's
/// shadows (the sliders' values, or less under the split profile).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InUse {
    pub objects: f32,
    pub actors: f32,
    pub items: f32,
    pub shadows: f32,
}

/// The parts in use for the sliders of `d`, with split screen `split` or not: the
/// sliders' own, or [`SPLIT_VIEW`] of each view distance and [`SPLIT_SHADOWS`] of the
/// shadows' while split (module notes).
pub fn split_profile(d: &DrawDistances, split: bool) -> InUse {
    let (view, shadows) = if split { (SPLIT_VIEW, SPLIT_SHADOWS) } else { (1.0, 1.0) };
    InUse { objects: d.objects * view, actors: d.actors * view, items: d.items * view, shadows: d.shadows * shadows }
}

impl DrawDistances {
    pub fn new(args: &Args) -> DrawDistances {
        DrawDistances {
            objects: 1.0,
            actors: 1.0,
            items: 1.0,
            shadows: 1.0,
            grass: 1.0,
            reach: full_reach(args.radius),
            shadow_full: args.shadow_distance.max(0.0),
            grass_full: (args.grass_distance.max(0.0), args.grass_fade_range.max(0.0)),
            split: false,
        }
    }

    /// The parts in use now ([`split_profile`] with this game's split screen).
    pub fn in_use(&self) -> InUse {
        split_profile(self, self.split)
    }

    /// The part in use for a class (1 for one not ranged).
    pub fn part(&self, class: DrawClass) -> f32 {
        let u = self.in_use();
        match class {
            DrawClass::Object => u.objects,
            DrawClass::Actor => u.actors,
            DrawClass::Item => u.items,
            DrawClass::Unranged => 1.0,
        }
    }

    /// How far the sun's shadows reach now (game units).
    pub fn shadow_distance(&self) -> f32 {
        self.shadow_full * self.in_use().shadows
    }

    /// How far a class is drawn; `None`: as far as its cells (the full slider).
    pub fn distance(&self, class: DrawClass) -> Option<f32> {
        let part = self.part(class);
        (part < 1.0).then_some(self.reach * part)
    }
}

/// The farthest a loaded cell reaches from the camera with `radius` cells loaded round
/// its cell: `radius + 1` cells along both axes.
pub fn full_reach(radius: i32) -> f32 {
    (radius.max(0) + 1) as f32 * CELL_SIZE * std::f32::consts::SQRT_2
}

pub struct DrawDistancePlugin(pub DrawDistances);

impl Plugin for DrawDistancePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .insert_resource(nvf_play::perk_marks::ActorDrawDistance(self.0.distance(DrawClass::Actor)))
            .add_systems(Update, (follow_split.before(apply_ranges), apply_ranges.after(fade::fade_cells), share_actor_distance));
    }
}

/// Split screen is on in this game: player 2's always, player 1's while its menus' split
/// screen is on (`nvf_play::split_screen`).
fn split_on(p1: Option<&nvf_play::split_screen::SplitScreen>, p2: bool) -> bool {
    p2 || p1.is_some_and(|s| s.on)
}

/// The split profile follows split screen (module notes).
fn follow_split(mut commands: Commands, distances: Res<DrawDistances>, p1: Option<Res<nvf_play::split_screen::SplitScreen>>, p2: Option<Res<nvf_play::split_screen::PlayerTwo>>) {
    let on = split_on(p1.as_deref(), p2.is_some());
    if distances.split != on {
        commands.queue(move |w: &mut World| set_split(w, on));
    }
}

/// Turn the split profile on or off and act on it now: the meshes' ranges (as a slider
/// does) and the sun's shadows.
pub fn set_split(world: &mut World, on: bool) {
    world.resource_mut::<DrawDistances>().split = on;
    apply_shadows(world);
    let d = world.resource::<DrawDistances>();
    let u = d.in_use();
    info!(
        "split profile: {}; objects {:.0}%, actors {:.0}%, items {:.0}% of {:.0} units, shadows {:.0}% of {:.0} units (the settings keep their own values)",
        if on { "on" } else { "off" },
        u.objects * 100.0,
        u.actors * 100.0,
        u.items * 100.0,
        d.reach,
        u.shadows * 100.0,
        d.shadow_full
    );
}

/// How far actors are drawn, for Spotter (`nvf_play::perk_marks`): the Actors slider's
/// distance, or as far as their cells at 100%.
fn share_actor_distance(distances: Res<DrawDistances>, mut shared: ResMut<nvf_play::perk_marks::ActorDrawDistance>) {
    if distances.is_changed() {
        shared.set_if_neq(nvf_play::perk_marks::ActorDrawDistance(distances.distance(DrawClass::Actor)));
    }
}

/// The class of a mesh: from its ancestors (an actor's rig, a dropped item, the cell it
/// is in) and the reference's records, kept per reference.
fn classify(e: Entity, r: u32, parents: &Query<&ChildOf>, marks: &Query<(Has<CellEntity>, Has<ActorRig>, Has<DroppedRoot>)>, world: Option<&WorldData>, known: &mut HashMap<u32, DrawClass>) -> DrawClass {
    let mut in_cell = false;
    let mut at = e;
    loop {
        if let Ok((cell, actor, dropped)) = marks.get(at) {
            if actor {
                return DrawClass::Actor;
            }
            if dropped {
                return DrawClass::Item;
            }
            if cell {
                in_cell = true;
                break;
            }
        }
        match parents.get(at) {
            Ok(p) => at = p.parent(),
            Err(_) => break,
        }
    }
    if !in_cell {
        return DrawClass::Unranged;
    }
    *known.entry(r).or_insert_with(|| {
        let Some(w) = world else { return DrawClass::Object };
        let Some(p) = placement(w.lo, w.schema, r) else { return DrawClass::Object };
        if matches!(&p.sig, b"ACHR" | b"ACRE") {
            return DrawClass::Actor;
        }
        match p.base.and_then(|b| base_object(w.lo, w.schema, b)) {
            Some(b) if ITEM_SIGS.contains(&&b.sig) => DrawClass::Item,
            _ => DrawClass::Object,
        }
    })
}

/// Give the meshes of placed references the range of their class's distance: new meshes
/// and those whose range changed (a fade ended) each frame, every one when a slider
/// moved (module notes).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn apply_ranges(
    mut commands: Commands,
    distances: Res<DrawDistances>,
    world: Option<Res<WorldData>>,
    changed: Query<Entity, (With<RefVisual>, Or<(With<MeshMaterial3d<NifMaterial>>, With<MeshMaterial3d<nvf_assets::nif_surface_blend::NifBlendMaterial>>)>, Or<(Added<RefVisual>, Changed<VisibilityRange>)>)>,
    all: Query<Entity, (With<RefVisual>, Or<(With<MeshMaterial3d<NifMaterial>>, With<MeshMaterial3d<nvf_assets::nif_surface_blend::NifBlendMaterial>>)>)>,
    mut meshes: Query<(&RefVisual, Option<&mut MeshMaterial3d<NifMaterial>>, Option<&mut MeshMaterial3d<nvf_assets::nif_surface_blend::NifBlendMaterial>>, Option<&VisibilityRange>, Option<&DrawClass>)>,
    parents: Query<&ChildOf>,
    marks: Query<(Has<CellEntity>, Has<ActorRig>, Has<DroppedRoot>)>,
    mut known: Local<HashMap<u32, DrawClass>>,
    mut work: Option<ResMut<crate::quality_probe::Work>>,
) {
    let entities: Vec<Entity> = if distances.is_changed() { all.iter().collect() } else { changed.iter().collect() };
    if let Some(work) = work.as_mut().filter(|_| distances.is_changed()) {
        work.ranges += 1;
        work.range_meshes += entities.len() as u64;
    }
    if distances.is_changed() && !distances.is_added() {
        // The parts in use (the split profile's, while split).
        let u = distances.in_use();
        info!(
            "draw distances: objects {:.0}%, actors {:.0}%, items {:.0}% of {:.0} units ({} meshes)",
            u.objects * 100.0,
            u.actors * 100.0,
            u.items * 100.0,
            distances.reach,
            entities.len()
        );
    }
    for e in entities {
        let Ok((r, material, blend, range, class)) = meshes.get_mut(e) else { continue };
        let class = match class {
            Some(c) => *c,
            None => {
                let c = classify(e, r.0, &parents, &marks, world.as_deref(), &mut known);
                commands.entity(e).insert(c);
                c
            }
        };
        let want = distances.distance(class);
        match range {
            // Never had a range (cell fades off): one only when it is to be culled.
            None => {
                if let Some(d) = want {
                    commands.entity(e).insert(fade::drawn_within(Some(d), FADE));
                    // Its pipeline is chosen again, with the dither (fade.rs notes).
                    if let Some(mut material) = material { material.set_changed(); }
                    if let Some(mut blend) = blend { blend.set_changed(); }
                    if let Some(work) = work.as_mut() {
                        work.range_materials += 1;
                    }
                }
            }
            // Fading in or out: the fade's own range until it rests.
            Some(r) if !fade::at_rest(r) => {}
            Some(r) => {
                let target = fade::drawn_within(want, FADE);
                if *r != target {
                    commands.entity(e).insert(target);
                }
            }
        }
    }
}

/// Set a slider's value and act on it now (the settings page).
pub fn set_part(world: &mut World, class: DrawClass, v: f32) {
    let mut d = world.resource_mut::<DrawDistances>();
    match class {
        DrawClass::Object => d.objects = v,
        DrawClass::Actor => d.actors = v,
        DrawClass::Item => d.items = v,
        DrawClass::Unranged => {}
    }
}

/// The sun's shadows reach `v` of `--shadow-distance` from now (less under the split
/// profile).
pub fn set_shadows(world: &mut World, v: f32) {
    world.resource_mut::<DrawDistances>().shadows = v;
    apply_shadows(world);
}

/// The sun's shadows reach [`DrawDistances::shadow_distance`] from now.
fn apply_shadows(world: &mut World) {
    let (full, distance) = {
        let d = world.resource::<DrawDistances>();
        (d.shadow_full, d.shadow_distance())
    };
    if full <= 0.0 {
        return;
    }
    let on = {
        let Some(mut sky) = world.get_resource_mut::<SkyState>() else { return };
        sky.shadow_distance = distance;
        sky.shadows
    };
    // The cascades again, if the sun casts shadows now.
    if on {
        crate::sky::set_shadows(world, true);
    }
}

/// Grass grows out to `v` of `--grass-distance` (and its fade range) from now
/// (`grass.rs` gives its materials the distances).
pub fn set_grass(world: &mut World, v: f32) {
    let (start, range) = {
        let mut d = world.resource_mut::<DrawDistances>();
        d.grass = v;
        d.grass_full
    };
    let mut g = world.resource_mut::<GrassConfig>();
    g.fade_start = start * v;
    g.fade_range = range * v;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_mesh_keeps_fading_then_receives_object_slider_updates() {
        let mut app = App::new();
        let mut d = distances(); d.objects = 0.5;
        app.insert_resource(d).add_systems(Update, apply_ranges);
        let e = app.world_mut().spawn((RefVisual(42), DrawClass::Object,
            MeshMaterial3d::<nvf_assets::nif_surface_blend::NifBlendMaterial>(Handle::default()), fade::drawn(8))).id();
        app.update();
        assert!(app.world().get::<VisibilityRange>(e).unwrap() == &fade::drawn(8));
        app.world_mut().entity_mut(e).insert(fade::drawn(fade::STEPS));
        app.update();
        let reach = app.world().resource::<DrawDistances>().reach;
        assert!(app.world().get::<VisibilityRange>(e).unwrap() == &fade::drawn_within(Some(reach * 0.5), FADE));
        app.world_mut().resource_mut::<DrawDistances>().objects = 0.25;
        app.update();
        assert!(app.world().get::<VisibilityRange>(e).unwrap() == &fade::drawn_within(Some(reach * 0.25), FADE));
        app.world_mut().entity_mut(e).remove::<VisibilityRange>();
        app.world_mut().resource_mut::<DrawDistances>().objects = 0.75;
        app.update();
        assert!(app.world().get::<VisibilityRange>(e).unwrap() == &fade::drawn_within(Some(reach * 0.75), FADE));
    }

    fn distances() -> DrawDistances {
        DrawDistances { objects: 1.0, actors: 0.5, items: 0.25, shadows: 1.0, grass: 1.0, reach: full_reach(2), shadow_full: 8000.0, grass_full: (7000.0, 1000.0), split: false }
    }

    #[test]
    fn the_split_profile_lowers_view_and_shadows_on_top_of_the_sliders() {
        let mut d = distances();
        d.shadows = 0.8;
        assert_eq!(split_profile(&d, false), InUse { objects: 1.0, actors: 0.5, items: 0.25, shadows: 0.8 }, "off: the sliders' own");
        let u = split_profile(&d, true);
        assert_eq!((u.objects, u.actors, u.items), (0.75, 0.375, 0.1875));
        assert!((u.shadows - 0.56).abs() < 1e-6);
        // The sliders themselves (what the INI keeps) are untouched.
        assert_eq!((d.objects, d.actors, d.items, d.shadows), (1.0, 0.5, 0.25, 0.8));
        // A full slider is culled at 75% while split, open-ended otherwise.
        assert_eq!(d.distance(DrawClass::Object), None);
        d.split = true;
        assert_eq!(d.distance(DrawClass::Object), Some(d.reach * 0.75));
        assert_eq!(d.distance(DrawClass::Unranged), None, "imposters as before");
        assert!((d.shadow_distance() - 8000.0 * 0.56).abs() < 1e-3);
    }

    #[test]
    fn split_is_on_for_player_two_and_for_player_one_while_on() {
        use nvf_play::split_screen::SplitScreen;
        assert!(!split_on(None, false));
        assert!(split_on(None, true));
        assert!(!split_on(Some(&SplitScreen { available: true, ..default() }), false));
        assert!(split_on(Some(&SplitScreen { on: true, ..default() }), false));
    }

    #[test]
    fn split_screen_turns_the_profile_on_and_off_and_moves_the_shadows() {
        let mut app = App::new();
        let mut d = distances();
        d.shadows = 0.6;
        let mut sky = SkyState::default();
        (sky.shadows, sky.shadow_distance) = (false, 8000.0 * 0.6);
        app.insert_resource(d).insert_resource(sky);
        app.insert_resource(nvf_play::split_screen::SplitScreen { available: true, ..default() });
        app.add_systems(Update, follow_split);
        app.update();
        assert!(!app.world().resource::<DrawDistances>().split);
        app.world_mut().resource_mut::<nvf_play::split_screen::SplitScreen>().on = true;
        app.update();
        let w = app.world();
        assert!(w.resource::<DrawDistances>().split);
        assert!((w.resource::<SkyState>().shadow_distance - 8000.0 * 0.6 * SPLIT_SHADOWS).abs() < 1e-2);
        assert_eq!(w.resource::<DrawDistances>().shadows, 0.6, "the slider keeps the player's value");
        app.world_mut().resource_mut::<nvf_play::split_screen::SplitScreen>().on = false;
        app.update();
        let w = app.world();
        assert!(!w.resource::<DrawDistances>().split);
        assert!((w.resource::<SkyState>().shadow_distance - 8000.0 * 0.6).abs() < 1e-2, "given back");
    }

    #[test]
    fn the_full_distance_is_the_loaded_cells_reach() {
        assert!((full_reach(2) - 3.0 * CELL_SIZE * std::f32::consts::SQRT_2).abs() < 1e-3);
        let d = distances();
        assert_eq!(d.distance(DrawClass::Object), None, "100%: as far as the cells");
        assert_eq!(d.distance(DrawClass::Actor), Some(d.reach * 0.5));
        assert_eq!(d.distance(DrawClass::Item), Some(d.reach * 0.25));
        assert_eq!(d.distance(DrawClass::Unranged), None);
    }

    #[test]
    fn a_resting_range_ends_at_the_distance_and_full_is_the_open_one() {
        let open = fade::drawn(fade::STEPS);
        assert!(fade::drawn_within(None, FADE) == open);
        let r = fade::drawn_within(Some(4000.0), FADE);
        assert!(fade::at_rest(&r) && fade::at_rest(&open));
        assert!(!fade::at_rest(&fade::drawn(8)));
        assert!(r.is_visible_at_all(3999.0) && r.is_visible_at_all(4300.0));
        assert!(!r.is_visible_at_all(4401.0), "culled past the fade");
    }

    #[test]
    fn the_sliders_set_the_grass_and_the_parts() {
        let mut app = App::new();
        app.insert_resource(distances());
        app.insert_resource(GrassConfig { enabled: true, fade_start: 7000.0, fade_range: 1000.0, light: (1.0, 1.0), generate: Default::default() });
        let w = app.world_mut();
        set_grass(w, 0.5);
        let g = w.resource::<GrassConfig>();
        assert_eq!((g.fade_start, g.fade_range), (3500.0, 500.0));
        set_part(w, DrawClass::Actor, 0.3);
        assert_eq!(w.resource::<DrawDistances>().actors, 0.3);
    }
}
