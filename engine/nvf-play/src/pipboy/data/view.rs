//! The DATA page's maps' one viewport (`MapView`), as plain data: the window the map is seen
//! through, where the World Map's picture lies in it and the transform from the picture's
//! texture coordinates to the window's units and back. The hand-built page and the page drawn
//! from `map_menu.xml` (`crate::tiles::map_menu`) both paint from it, and the model's picking
//! (`super::place_cursor`, `super::travel_ask`) reads the same window, so what is under the
//! pointer is what is drawn (docs/spec-notes/pipboy.md §5.3, §5.3.1) [design]. No Bevy here
//! but its maths types; tested below.
//!
//! - **The window**: the hand-built page's is the file's numbers (`super::MAP`: 50 down in the
//!   menus' rectangle, 855 × 500) [documented: map_menu.xml `MM_WorldMap_ClipWindow`]; the tile
//!   page publishes the clip window as laid out ([`super::DataState::publish_map`]), for the tab
//!   it drew, which the model reads instead while it stands.
//! - **Pan and zoom** (NVForever's own [design]): the World Map opens centred on the player, a
//!   drag moves the point shown at the window's middle, a wheel step zooms 1.25 times between 1
//!   and 4; the picture's near edge stays no further in than the window's middle and its far
//!   edge no further out [documented: map_menu.xml `MM_WorldMap_ParentImage` x and y,
//!   `super::model::pan`].
//! - **The controller's cursor**: the right stick moves the highlight box in the window, kept
//!   inside it; what is pushed past an edge pans ([`MapView::stick`]).

use super::model;
use bevy::math::{Rect, Vec2};

/// The maps' viewport: the window on the Pip-Boy's canvas, and the World Map's picture in it
/// (its top left and size in the window's units; none on the Local Map).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapView {
    /// The window, canvas units.
    pub window: Rect,
    /// The picture's top left and its size, in the window's units (from its top left).
    pub left: Vec2,
    pub size: Vec2,
    /// The World Map's point (texture coordinates) wanted at the window's middle.
    pub centre: [f32; 2],
}

impl MapView {
    /// A window with no picture (the Local Map's, or before a map is read).
    pub fn bare(window: Rect) -> MapView {
        MapView { window, left: Vec2::ZERO, size: Vec2::ZERO, centre: [0.5, 0.5] }
    }

    /// The same view seen through another window: the picture's size kept, its place worked out
    /// again about the new window's middle ([`MapView::world`]), so a drawing that measured its
    /// window apart from the one the view was made in shows the same point at its middle
    /// [design].
    pub fn with_window(&self, window: Rect) -> MapView {
        let mid = window.size() / 2.0;
        if self.size == Vec2::ZERO {
            return MapView { window, ..*self };
        }
        MapView { window, left: Vec2::new(model::pan(self.centre[0], self.size.x, mid.x), model::pan(self.centre[1], self.size.y, mid.y)), ..*self }
    }

    /// The World Map's view: a picture `usable` across at a magnification of 1 (the root's
    /// `MNAM`, `super::model::WorldMap::usable`), magnified `mag`, its point `centre` (texture
    /// coordinates 0..1) wanted at the window's middle, held as the file holds it
    /// ([`model::pan`]).
    pub fn world(window: Rect, centre: [f32; 2], usable: Vec2, mag: f32) -> MapView {
        let size = usable * mag;
        let mid = window.size() / 2.0;
        MapView { window, left: Vec2::new(model::pan(centre[0], size.x, mid.x), model::pan(centre[1], size.y, mid.y)), size, centre }
    }

    /// The window's size.
    pub fn extent(&self) -> Vec2 {
        self.window.size()
    }

    /// The window's middle, in its own units: where the highlight box starts.
    pub fn mid(&self) -> Vec2 {
        self.window.size() / 2.0
    }

    /// Where texture coordinates `uv` of the picture fall in the window.
    pub fn to_window(&self, uv: [f32; 2]) -> Vec2 {
        self.left + Vec2::new(uv[0] * self.size.x, uv[1] * self.size.y)
    }

    /// The texture coordinates under a point `at` of the window ([`MapView::to_window`] undone);
    /// a picture of no size answers its top left.
    pub fn to_uv(&self, at: Vec2) -> [f32; 2] {
        let d = at - self.left;
        let axis = |d: f32, s: f32| if s > 0.0 { d / s } else { 0.0 };
        [axis(d.x, self.size.x), axis(d.y, self.size.y)]
    }

    /// The picture's place in the window's units.
    pub fn picture(&self) -> Rect {
        Rect::from_corners(self.left, self.left + self.size)
    }

    /// The pointer (canvas units) in the window's units, while it is over the window.
    pub fn pointer(&self, canvas: Option<Vec2>) -> Option<Vec2> {
        let p = canvas? - self.window.min;
        let s = self.window.size();
        (p.x >= 0.0 && p.y >= 0.0 && p.x < s.x && p.y < s.y).then_some(p)
    }

    /// The window's point `at` on the canvas.
    pub fn to_canvas(&self, at: Vec2) -> Vec2 {
        self.window.min + at
    }

    /// The highlight box `at` moved by the right stick `v` for `dt` seconds at `speed` canvas
    /// units a second, kept in the window (the stick's up is the screen's up, the window's y runs
    /// down); and how far it was pushed past an edge, which pans the map (`super::scroll_map`).
    pub fn stick(&self, at: Vec2, v: Vec2, dt: f32, speed: f32) -> (Vec2, Vec2) {
        let raw = at + Vec2::new(v.x, -v.y) * (speed * dt);
        let s = self.window.size();
        let cursor = Vec2::new(raw.x.clamp(0.0, s.x), raw.y.clamp(0.0, s.y));
        (cursor, raw - cursor)
    }
}

/// The World Map's point at the window's middle after a drag by `d` window units, the picture
/// `size` across (the drag moves the picture by hand), held on the picture.
pub fn dragged(centre: [f32; 2], d: Vec2, size: Vec2) -> [f32; 2] {
    let axis = |c: f32, d: f32, s: f32| if s > 0.0 { (c - d / s).clamp(0.0, 1.0) } else { c };
    [axis(centre[0], d.x, size.x), axis(centre[1], d.y, size.y)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-built page's window: 50 down in the menus' rectangle (50, 50), 855 × 500.
    fn window() -> Rect {
        Rect::new(50.0, 100.0, 905.0, 600.0)
    }

    /// [design] Texture coordinates go to the window and back, at every zoom and pan, in the
    /// middle and at the picture's edges.
    #[test]
    fn texture_coordinates_round_trip_through_the_window() {
        for mag in [1.0, 1.25, 2.0, 4.0] {
            for centre in [[0.5, 0.5], [0.0, 0.0], [1.0, 1.0], [0.2, 0.9]] {
                let v = MapView::world(window(), centre, Vec2::splat(2048.0), mag);
                for uv in [[0.0, 0.0], [1.0, 1.0], [0.33236843, 0.5636847], [0.5, 0.25]] {
                    let back = v.to_uv(v.to_window(uv));
                    assert!((back[0] - uv[0]).abs() < 1e-5 && (back[1] - uv[1]).abs() < 1e-5, "{mag} {centre:?} {uv:?}: {back:?}");
                }
            }
        }
    }

    /// [documented: map_menu.xml, the picture's x between 427.5 - width and 427.5] The picture
    /// keeps its edges at most to the window's middle at both zoom extremes; the point wanted is
    /// at the middle wherever the picture allows.
    #[test]
    fn pan_and_zoom_hold_the_picture_at_the_windows_middle() {
        let w = window();
        // At 1: a 2048 picture centred on its middle: its middle at the window's.
        let v = MapView::world(w, [0.5, 0.5], Vec2::splat(2048.0), 1.0);
        assert_eq!(v.to_window([0.5, 0.5]), v.mid());
        assert_eq!(v.picture(), Rect::new(427.5 - 1024.0, 250.0 - 1024.0, 427.5 + 1024.0, 250.0 + 1024.0));
        // At 4, a corner wanted: the corner at the middle, no further.
        let v = MapView::world(w, [0.0, 0.0], Vec2::splat(2048.0), 4.0);
        assert_eq!((v.left, v.size), (Vec2::new(427.5, 250.0), Vec2::splat(8192.0)));
        let v = MapView::world(w, [1.0, 1.0], Vec2::splat(2048.0), 4.0);
        assert_eq!(v.left, Vec2::new(427.5 - 8192.0, 250.0 - 8192.0));
        // A point past the edge is held at it.
        assert_eq!(MapView::world(w, [-3.0, 7.0], Vec2::splat(2048.0), 1.0).left, Vec2::new(427.5, 250.0 - 2048.0));
        // A broken record's size keeps the picture at the middle, never panics.
        let v = MapView::world(w, [0.5, 0.5], Vec2::new(f32::NAN, -5.0), 2.0);
        assert_eq!(v.left, v.mid());
        // A measured window elsewhere: the same rule about its own middle.
        let other = Rect::new(10.0, 20.0, 410.0, 320.0);
        let v = MapView::world(other, [0.5, 0.5], Vec2::splat(1000.0), 1.0);
        assert_eq!(v.to_window([0.5, 0.5]), Vec2::new(200.0, 150.0));
        assert_eq!(v.to_canvas(v.mid()), Vec2::new(210.0, 170.0));
    }

    /// [design] Another window: the same point at its middle, the picture's size kept.
    #[test]
    fn a_view_seen_through_another_window_keeps_its_middle_point() {
        let v = MapView::world(window(), [0.3, 0.6], Vec2::splat(2048.0), 2.0);
        let other = Rect::new(10.0, 20.0, 610.0, 420.0);
        let w = v.with_window(other);
        assert_eq!((w.window, w.size, w.centre), (other, v.size, v.centre));
        assert_eq!(w.to_window([0.3, 0.6]), w.mid());
        assert_eq!(w.with_window(window()), v, "and back");
        let bare = MapView::bare(window()).with_window(other);
        assert_eq!((bare.window, bare.size), (other, Vec2::ZERO));
    }

    #[test]
    fn the_pointer_is_in_the_window_only_over_it() {
        let v = MapView::bare(window());
        assert_eq!(v.pointer(Some(Vec2::new(50.0, 100.0))), Some(Vec2::ZERO));
        assert_eq!(v.pointer(Some(Vec2::new(904.0, 599.0))), Some(Vec2::new(854.0, 499.0)));
        assert_eq!(v.pointer(Some(Vec2::new(905.0, 300.0))), None, "the right edge is outside");
        assert_eq!(v.pointer(Some(Vec2::new(49.0, 300.0))), None);
        assert_eq!(v.pointer(None), None);
    }

    #[test]
    fn the_stick_keeps_the_cursor_in_the_window_and_reports_the_overflow() {
        let v = MapView::bare(window());
        let (at, over) = v.stick(Vec2::new(850.0, 10.0), Vec2::new(1.0, 1.0), 0.1, 600.0);
        assert_eq!((at, over), (Vec2::new(855.0, 0.0), Vec2::new(55.0, -50.0)));
        let (at, over) = v.stick(Vec2::new(400.0, 200.0), Vec2::new(-0.5, 0.0), 0.1, 600.0);
        assert_eq!((at, over), (Vec2::new(370.0, 200.0), Vec2::ZERO));
    }

    #[test]
    fn a_drag_moves_the_middle_point_and_holds_it_on_the_picture() {
        let c = dragged([0.5, 0.5], Vec2::new(256.0, -512.0), Vec2::splat(2048.0));
        assert_eq!(c, [0.375, 0.75]);
        assert_eq!(dragged([0.1, 0.9], Vec2::new(1000.0, -1000.0), Vec2::splat(2048.0)), [0.0, 1.0]);
        assert_eq!(dragged([0.3, 0.3], Vec2::new(5.0, 5.0), Vec2::ZERO), [0.3, 0.3], "no picture: nothing moves");
    }
}
