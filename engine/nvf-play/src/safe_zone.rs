//! The HUD's safe zone (the owner's ask of 2026-11-06): marks drawn over points of the
//! world (pings and name tags, [`crate::world_marks`]; JAM's JVO quest markers) are moved
//! off the HUD's own pieces, so none is drawn over the compass, the meters, the messages,
//! the prompt or the subtitles [design].
//!
//! - [`SafeZone`] holds the rectangles the hand-built HUD drew last frame (logical
//!   pixels): every shown text, picture and filled box under a UI root, grown by
//!   [`PAD`] and merged where they touch. Left out: roots marked [`SafeZoneIgnore`] (the
//!   world marks themselves, the tile renderer's roots, whose mod tiles are JVO's markers
//!   and the like), pieces covering more than [`MOST_OF_SCREEN`] of the view (fades,
//!   splatter, full-screen overlays), and the crosshair, a small piece on the screen's
//!   middle ([`CROSSHAIR`]).
//! - [`SafeZone::clear`] brings a mark's box onto the screen, then moves it the least way that leaves every rectangle
//!   and stays on screen; with no such way the mark stays where it is.
//! - JVO: after the scripts wrote its markers' places (`HUDMainMenu\JVO\JVORect\
//!   JVOMarker<n>`'s `_X`/`_Y`, parts of the screen [observed: JAM's
//!   JVOCoordinatesEventHandler and JVO.xml]) and before the tree is worked out, each
//!   marker's place is moved the same way, its box taken from last frame's arrow and
//!   texts (the marker's point is the arrow's tip, the texts over it) [design].
//!
//! Log line: `safe zone: JVO marker <n> moved off the HUD by (dx, dy)` once per marker
//! when it first has to move.

use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiSystems};
use bevy::window::PrimaryWindow;

use crate::script::ScriptState;
use crate::PlaySet;

/// How far around the HUD's pieces a mark is kept (logical pixels) [design].
pub const PAD: f32 = 6.0;
/// Pieces bigger than this part of the view are not the HUD's [design].
pub const MOST_OF_SCREEN: f32 = 0.2;
/// The crosshair: a piece no bigger than this (logical pixels) whose middle is this near
/// the screen's middle is not kept clear [design].
pub const CROSSHAIR: f32 = 48.0;
/// How often the HUD's pieces are gathered while marks are shown (seconds) [design].
pub const GATHER_EVERY: f32 = 0.1;

/// A root whose pieces are not the HUD's: nothing is kept clear of them.
#[derive(Component)]
pub struct SafeZoneIgnore;

/// The HUD's rectangles last frame, logical pixels from the window's top left.
#[derive(Resource, Default, Debug, Clone)]
pub struct SafeZone {
    pub rects: Vec<Rect>,
    pub size: Vec2,
    /// A mark asked to be kept clear this frame ([`SafeZone::want`]): only then are the
    /// HUD's pieces gathered, at most every [`GATHER_EVERY`] seconds [design: the owner's
    /// question of 2026-11-06, no work while nothing is marked].
    pub wanted: bool,
    pub since: f32,
}

impl SafeZone {
    /// A mark will be placed: gather the HUD's pieces.
    pub fn want(&mut self) {
        self.wanted = true;
    }

    /// Where a mark whose box is `area` (logical pixels) goes to clear the HUD: the least
    /// move that leaves every rectangle and keeps the box on screen; none when it is
    /// already clear or no move does it.
    pub fn clear(&self, area: Rect) -> Vec2 {
        // First the whole box onto the screen (a mark held at the screen's edge has its
        // arrow and texts past it otherwise).
        let mut inside = Vec2::ZERO;
        if self.size != Vec2::ZERO {
            inside.x = (-area.min.x).max(0.0) - (area.max.x - self.size.x).max(0.0);
            inside.y = (-area.min.y).max(0.0) - (area.max.y - self.size.y).max(0.0);
        }
        let area = Rect { min: area.min + inside, max: area.max + inside };
        let hits = |b: Rect| self.rects.iter().any(|r| overlaps(*r, b));
        if !hits(area) {
            return inside;
        }
        let on_screen = |b: Rect| self.size == Vec2::ZERO || (b.min.x >= 0.0 && b.min.y >= 0.0 && b.max.x <= self.size.x && b.max.y <= self.size.y);
        // Each rectangle's four ways out, alone and with one other's.
        let mut ways: Vec<Vec2> = Vec::new();
        for r in &self.rects {
            ways.push(Vec2::new(r.min.x - area.max.x, 0.0));
            ways.push(Vec2::new(r.max.x - area.min.x, 0.0));
            ways.push(Vec2::new(0.0, r.min.y - area.max.y));
            ways.push(Vec2::new(0.0, r.max.y - area.min.y));
        }
        let singles = ways.clone();
        for a in &singles {
            for b in &singles {
                if a.x != 0.0 && b.y != 0.0 {
                    ways.push(Vec2::new(a.x, b.y));
                }
                // Two moves on one axis: out of one rectangle into the next and out again.
                if a.x == 0.0 && b.x == 0.0 && a.y.signum() == b.y.signum() {
                    ways.push(Vec2::new(0.0, if a.y.abs() > b.y.abs() { a.y } else { b.y }));
                }
            }
        }
        ways.sort_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
        ways.into_iter()
            // Half a pixel past the edge (f32::signum is 1 at 0, so the axis not moved stays).
            .map(|d| d + Vec2::new(nudge(d.x), nudge(d.y)))
            .find(|d| {
                let b = Rect { min: area.min + *d, max: area.max + *d };
                on_screen(b) && !hits(b)
            })
            .map_or(inside, |d| inside + d)
    }
}

fn nudge(v: f32) -> f32 {
    if v == 0.0 { 0.0 } else { v.signum() * 0.5 }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

/// The pieces' rectangles grown by `pad` and merged until none touches another.
pub fn merged(mut rects: Vec<Rect>, pad: f32) -> Vec<Rect> {
    for r in &mut rects {
        *r = Rect { min: r.min - Vec2::splat(pad), max: r.max + Vec2::splat(pad) };
    }
    loop {
        let mut joined = false;
        let mut out: Vec<Rect> = Vec::with_capacity(rects.len());
        for r in rects {
            if let Some(o) = out.iter_mut().find(|o| overlaps(**o, r)) {
                *o = o.union(r);
                joined = true;
            } else {
                out.push(r);
            }
        }
        rects = out;
        if !joined {
            return rects;
        }
    }
}

pub struct SafeZonePlugin;

impl Plugin for SafeZonePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SafeZone>()
            .add_systems(PostUpdate, gather.after(UiSystems::Layout))
            .add_systems(Update, clear_jvo.after(PlaySet::Scripts).before(PlaySet::Ui));
    }
}

type Piece<'a> = (
    &'a ComputedNode,
    &'a UiGlobalTransform,
    &'a InheritedVisibility,
    Option<&'a Children>,
    Option<&'a TextColor>,
    Option<&'a ImageNode>,
    Option<&'a BackgroundColor>,
);

/// The UI's roots that may hold the HUD's pieces.
type Roots = (With<Node>, Without<ChildOf>, Without<SafeZoneIgnore>);

fn gather(
    roots: Query<Entity, Roots>,
    pieces: Query<Piece<'_>>,
    ignored: Query<(), With<SafeZoneIgnore>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time<Real>>,
    mut zone: ResMut<SafeZone>,
) {
    zone.since += time.delta_secs();
    if !std::mem::take(&mut zone.wanted) {
        // Nothing marked: no work, and the next mark gathers at once.
        zone.since = GATHER_EVERY;
        return;
    }
    if zone.since < GATHER_EVERY {
        return;
    }
    zone.since = 0.0;
    let Ok(window) = windows.single() else { return };
    let size = window.size();
    let middle = size / 2.0;
    let mut rects = Vec::new();
    let mut stack: Vec<Entity> = roots.iter().collect();
    while let Some(e) = stack.pop() {
        let Ok((node, t, seen, children, text, image, fill)) = pieces.get(e) else { continue };
        if !seen.get() || ignored.contains(e) {
            continue;
        }
        if let Some(c) = children {
            stack.extend(c.iter());
        }
        let shows = text.is_some_and(|c| c.0.alpha() > 0.02) || image.is_some_and(|i| i.color.alpha() > 0.02) || fill.is_some_and(|f| f.0.alpha() > 0.02);
        if !shows {
            continue;
        }
        let inv = node.inverse_scale_factor();
        let half = node.size() * inv / 2.0;
        if half.x <= 0.0 || half.y <= 0.0 {
            continue;
        }
        let c = t.translation * inv;
        let r = Rect { min: c - half, max: c + half };
        if r.width() * r.height() > MOST_OF_SCREEN * size.x * size.y {
            continue;
        }
        if r.width() <= CROSSHAIR && r.height() <= CROSSHAIR && c.distance(middle) <= CROSSHAIR {
            continue;
        }
        rects.push(r);
    }
    zone.rects = merged(rects, PAD);
    zone.size = size;
}

/// JVO's markers moved off the HUD (the module's notes).
pub(crate) fn clear_jvo(mut script: ResMut<ScriptState>, mut zone: ResMut<SafeZone>, mut told: Local<std::collections::HashSet<String>>) {
    let Some(rt) = script.rt.as_mut() else { return };
    let ui = &mut rt.game.ui_tree;
    let Some(rect) = ui.find_tile("HUDMainMenu/JVO/JVORect") else { return };
    let shown = |ui: &fnv_ui::Ui, m: fnv_ui::TileId| ui.tile(m).get("_X").is_some_and(|v| v.num() > -1.0);
    if !ui.tile(rect).children.iter().any(|&m| shown(ui, m)) {
        return;
    }
    zone.want();
    if zone.size == Vec2::ZERO {
        return;
    }
    let screen = ui.screen();
    let num = |ui: &fnv_ui::Ui, id: fnv_ui::TileId, name: &str| ui.tile(id).get(name).map(|v| v.num() as f32);
    let (Some(sw), Some(sh)) = (num(ui, screen, "width"), num(ui, screen, "height")) else { return };
    if sw <= 0.0 || sh <= 0.0 {
        return;
    }
    // Tile units to logical pixels.
    let per = Vec2::new(zone.size.x / sw, zone.size.y / sh);
    let markers = ui.tile(rect).children.clone();
    for m in markers {
        let (Some(x), Some(y)) = (num(ui, m, "_X"), num(ui, m, "_Y")) else { continue };
        if x <= -1.0 || y <= -1.0 || num(ui, m, "visible") == Some(0.0) {
            continue;
        }
        // The box over the point: the arrow, then each shown text over it.
        let (mut wide, mut high) = (0.0f32, 0.0f32);
        for c in ui.tile(m).children.clone() {
            if num(ui, c, "visible") == Some(0.0) {
                continue;
            }
            wide = wide.max(num(ui, c, "width").unwrap_or(0.0));
            high += num(ui, c, "height").unwrap_or(0.0);
        }
        if wide <= 0.0 || high <= 0.0 {
            continue;
        }
        let p = Vec2::new(x, y) * zone.size;
        let half = wide * per.x / 2.0;
        let area = Rect { min: Vec2::new(p.x - half, p.y - high * per.y), max: Vec2::new(p.x + half, p.y) };
        let d = zone.clear(area);
        if d == Vec2::ZERO {
            continue;
        }
        let to = (p + d) / zone.size;
        ui.set_trait(m, "_X", fnv_ui::Value::Num(f64::from(to.x)));
        ui.set_trait(m, "_Y", fnv_ui::Value::Num(f64::from(to.y)));
        let name = ui.tile(m).name.clone();
        if told.insert(name.clone()) {
            info!("safe zone: JVO marker {name} moved off the HUD by ({:.0}, {:.0})", d.x, d.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { min: Vec2::new(x0, y0), max: Vec2::new(x1, y1) }
    }

    #[test]
    fn a_mark_clear_of_the_hud_stays_and_one_on_it_moves_the_least_way_off() {
        let zone = SafeZone { rects: vec![r(0.0, 600.0, 300.0, 720.0)], size: Vec2::new(1280.0, 720.0), ..Default::default() };
        assert_eq!(zone.clear(r(500.0, 300.0, 560.0, 360.0)), Vec2::ZERO);
        // Over the compass's top edge by 20: up 20 (and a half) beats right 210.
        let d = zone.clear(r(250.0, 560.0, 290.0, 620.0));
        assert!(d.x == 0.0 && (-21.0..=-20.0).contains(&d.y), "{d}");
    }

    #[test]
    fn a_mark_is_not_moved_off_the_screen_or_into_another_piece() {
        // A piece at the top: up would leave the screen, so it goes down.
        let zone = SafeZone { rects: vec![r(400.0, 0.0, 900.0, 50.0)], size: Vec2::new(1280.0, 720.0), ..Default::default() };
        let d = zone.clear(r(600.0, 10.0, 640.0, 40.0));
        assert!(d.y > 0.0 && d.x == 0.0, "{d}");
        // Two pieces stacked: out of both.
        let zone = SafeZone { rects: vec![r(0.0, 500.0, 300.0, 600.0), r(0.0, 400.0, 300.0, 499.0)], size: Vec2::new(1280.0, 720.0), ..Default::default() };
        let area = r(100.0, 520.0, 140.0, 560.0);
        let d = zone.clear(area);
        let moved = Rect { min: area.min + d, max: area.max + d };
        assert!(zone.rects.iter().all(|z| !overlaps(*z, moved)), "{d}");
    }

    #[test]
    fn a_mark_past_the_screens_edge_is_brought_onto_it() {
        let zone = SafeZone { rects: vec![], size: Vec2::new(1280.0, 720.0), ..Default::default() };
        assert_eq!(zone.clear(r(600.0, -50.0, 640.0, 7.0)), Vec2::new(0.0, 50.0));
        assert_eq!(zone.clear(r(1270.0, 300.0, 1300.0, 340.0)), Vec2::new(-20.0, 0.0));
    }

    #[test]
    fn touching_pieces_merge_into_one() {
        let m = merged(vec![r(0.0, 0.0, 10.0, 10.0), r(12.0, 0.0, 20.0, 10.0), r(100.0, 100.0, 110.0, 110.0)], 2.0);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], r(-2.0, -2.0, 22.0, 12.0));
    }
}
