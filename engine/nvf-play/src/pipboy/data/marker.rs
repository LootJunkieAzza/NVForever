//! The player's custom map marker (Place Marker, #1954; docs/spec-notes/pipboy.md §5.3)
//! and the World Map's dotted lines, as plain data. No Bevy systems here; tested below.
//!
//! - **One marker**, shared by both maps [observed: a capture of the original's MapMenu,
//!   2026-11-10: placed or moved on either map, the other map shows it at the matching
//!   place]: the worldspace (outdoors) or interior cell it was placed in, and the point.
//!   It is the scripts' marker too: JohnnyGuitar's SetCustomMapMarker and
//!   ClearCustomMapMarker set it [documented: GECK wiki, "Functions the same as setting
//!   the marker within the map menu in the Pip-Boy"], so the game keeps it
//!   (`fnv_game::Game::custom_map_marker`) and the Pip-Boy reads and writes that one.
//! - **The question** [observed: the capture]: Place Marker (the maps' X button, a click
//!   on its hint) asks whether to set the marker (two answers, yes first) while there is
//!   none, else whether to move it, remove it or leave it (three answers in that order).
//!   The yes and move answers put it at the highlight box's middle (the capture: its
//!   texture coordinates equal the box's middle within 0.001).
//! - **The words** come from the load order's game settings by name ([`Words`]); the
//!   masters define none of them [observed: fnvtool's GMST inventory of FalloutNV.esm and
//!   the add-ons, 2026-11-10], so an English install shows NVForever's own words, written
//!   for this box (LEGAL.md) [design].
//! - **The dotted line** [documented: map_menu.xml `WaypointTemplate`,
//!   `PathLineTemplate`]: a waypoint node at each end and a line between them from the
//!   start node's middle, the file `waypoint_path.dds` repeated along it, as long as the
//!   path less 4 at a magnification of 1, times the magnification, a third of the node's
//!   height thick ([`dotted_line`]).

use bevy::math::Vec2;
use fnv_sim::FormId;

/// The player's custom map marker: the worldspace (outdoors) or interior cell it is in,
/// and the point in that space's game units (the game's own copy holds the same,
/// `fnv_game::nvse_jg8`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomMarker {
    pub space: u32,
    pub pos: [f32; 3],
}

impl CustomMarker {
    /// The game's copy (`Game::custom_map_marker`).
    pub fn from_game((space, p): (FormId, [f64; 3])) -> CustomMarker {
        CustomMarker { space: space.0, pos: [p[0] as f32, p[1] as f32, p[2] as f32] }
    }

    pub fn to_game(self) -> (FormId, [f64; 3]) {
        (FormId(self.space), self.pos.map(f64::from))
    }
}

/// What Place Marker asks, with the spot under the highlight box when it was asked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerAsk {
    /// No marker yet: set it here? (yes, no)
    Set(CustomMarker),
    /// A marker is placed: move it here, remove it, or leave it.
    Change(CustomMarker),
}

/// What an answer does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerAnswer {
    Placed(CustomMarker),
    Moved(CustomMarker),
    Removed,
    /// The marker stays where it was.
    Left,
    /// No, to setting one: still none.
    Declined,
}

/// The question for the spot `at`, with or without a marker placed now.
pub fn ask(placed: Option<CustomMarker>, at: CustomMarker) -> MarkerAsk {
    if placed.is_some() {
        MarkerAsk::Change(at)
    } else {
        MarkerAsk::Set(at)
    }
}

/// The answer `i` (0-based, in the box's order) to `ask`.
pub fn answer(ask: MarkerAsk, i: usize) -> MarkerAnswer {
    match (ask, i) {
        (MarkerAsk::Set(at), 0) => MarkerAnswer::Placed(at),
        (MarkerAsk::Set(_), _) => MarkerAnswer::Declined,
        (MarkerAsk::Change(at), 0) => MarkerAnswer::Moved(at),
        (MarkerAsk::Change(_), 1) => MarkerAnswer::Removed,
        (MarkerAsk::Change(_), _) => MarkerAnswer::Left,
    }
}

/// The marker after `a`, from `placed`.
pub fn apply(placed: Option<CustomMarker>, a: MarkerAnswer) -> Option<CustomMarker> {
    match a {
        MarkerAnswer::Placed(m) | MarkerAnswer::Moved(m) => Some(m),
        MarkerAnswer::Removed => None,
        MarkerAnswer::Left | MarkerAnswer::Declined => placed,
    }
}

/// How Place Marker answers at once, with no question, under the player's No Place
/// Marker Popup tweak [design: docs/design/tweaks.md]: `Toggle` places the marker while
/// there is none and takes it away while there is one; `ShiftRemoves` always places (or
/// moves) it, and takes it away with Shift held; `RemoveOnIt` always places it, and takes
/// it away when the highlight box is on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quick {
    Toggle,
    ShiftRemoves,
    RemoveOnIt,
}

impl Quick {
    /// The way the tweak's two switches pick (Shift Removes first).
    pub fn of(shift_removes: bool, remove_on_it: bool) -> Quick {
        if shift_removes {
            Quick::ShiftRemoves
        } else if remove_on_it {
            Quick::RemoveOnIt
        } else {
            Quick::Toggle
        }
    }
}

/// The answer Place Marker gives at `at` with no question ([`Quick`]): `shift` whether
/// Shift is held, `on_it` whether the highlight box is on the placed marker.
pub fn quick(placed: Option<CustomMarker>, at: CustomMarker, how: Quick, shift: bool, on_it: bool) -> MarkerAnswer {
    let remove = match how {
        Quick::Toggle => placed.is_some(),
        Quick::ShiftRemoves => shift,
        Quick::RemoveOnIt => on_it,
    };
    match (placed, remove) {
        (Some(_), true) => MarkerAnswer::Removed,
        (None, true) => MarkerAnswer::Declined,
        (Some(_), false) => MarkerAnswer::Moved(at),
        (None, false) => MarkerAnswer::Placed(at),
    }
}

/// Whether the player, at `feet` in `space` (an interior cell or a worldspace), is within
/// `within` game units of the marker `m`, across the ground (height left out: a marker
/// placed on the World Map has the player's height then) [design: docs/design/tweaks.md].
pub fn near(m: CustomMarker, space: u32, feet: [f32; 3], within: f32) -> bool {
    m.space == space && within > 0.0 && Vec2::new(m.pos[0] - feet[0], m.pos[1] - feet[1]).length() <= within
}

/// A word of the box: its game setting's name [documented: GECK wiki, Complete List of
/// GameSettings, names only] and NVForever's own word where the load order defines
/// none [design: written for this box, LEGAL.md].
pub type Word = (&'static str, &'static str);

/// The box's words.
pub struct Words;

impl Words {
    pub const SET_QUESTION: Word = ("sSetMarkerQuestion", "Put your marker on the map here?");
    pub const CHANGE_QUESTION: Word = ("sMoveMarkerQuestion", "Your marker is already on the map. What should happen to it?");
    pub const YES: Word = ("sYes", "Yes");
    pub const NO: Word = ("sNo", "No");
    pub const MOVE: Word = ("sMoveMarker", "Move Here");
    pub const REMOVE: Word = ("sRemoveMarker", "Remove");
    pub const LEAVE: Word = ("sLeaveMarker", "Keep");

    /// The question and its answers for `ask`, each from `look` (a setting's text by name,
    /// else NVForever's word; `super::super::labels::label`).
    pub fn of(ask: MarkerAsk, look: impl Fn(Word) -> String) -> (String, Vec<String>) {
        match ask {
            MarkerAsk::Set(_) => (look(Self::SET_QUESTION), vec![look(Self::YES), look(Self::NO)]),
            MarkerAsk::Change(_) => (look(Self::CHANGE_QUESTION), vec![look(Self::MOVE), look(Self::REMOVE), look(Self::LEAVE)]),
        }
    }
}

/// A piece of a dotted line: its middle, length and the share of the file it shows (the
/// last piece is cut short), turned by `angle` (radians, the canvas's y running down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub middle: Vec2,
    pub len: f32,
    pub share: f32,
    pub angle: f32,
}

/// What a dotted line leads to (for the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTo {
    Marker,
    Quest,
}

impl LineTo {
    pub fn name(self) -> &'static str {
        match self {
            LineTo::Marker => "custom marker",
            LineTo::Quest => "quest target",
        }
    }
}

/// How much shorter than the path the line is at a magnification of 1 [documented:
/// `PathLineTemplate` width, `user0` less 4, times `_Magnification`].
pub const LINE_SHORT: f32 = 4.0;

/// The line from the node at `from` to the one at `to` (window units), at magnification
/// `mag`, in pieces of `tile` along it: from `from`'s middle, `(d / mag − 4) × mag` long
/// where `d` is the distance shown (the file's length at a magnification of 1 less 4);
/// none when that is not positive (the capture's line to a marker about 2 away was
/// −2.1 wide) [documented: `PathLineTemplate`; observed: the capture].
pub fn dotted_line(from: Vec2, to: Vec2, mag: f32, tile: f32) -> Vec<Piece> {
    let d = to - from;
    let len = d.length() - LINE_SHORT * mag;
    if !(len > 0.0 && len.is_finite() && tile > 0.0) {
        return Vec::new();
    }
    let dir = d / d.length();
    let angle = d.y.atan2(d.x);
    let count = (len / tile).ceil() as usize;
    (0..count)
        .map(|i| {
            let start = i as f32 * tile;
            let piece = tile.min(len - start);
            Piece { middle: from + dir * (start + piece / 2.0), len: piece, share: piece / tile, angle }
        })
        .collect()
}

/// A place on the Local Map's window (`at`) in game units: the map shows `centre` at the
/// window's middle `mid`, `scale` game units a canvas unit at magnification `mag`, north
/// up (the Local Map's `on_map` undone).
pub fn local_point(centre: [f32; 2], mid: Vec2, mag: f32, scale: f32, at: Vec2) -> [f32; 2] {
    let k = scale / mag.max(f32::EPSILON);
    [centre[0] + (at.x - mid.x) * k, centre[1] - (at.y - mid.y) * k]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> CustomMarker {
        CustomMarker { space: 0x0DA726, pos: [x, y, 0.0] }
    }

    #[test]
    fn with_no_marker_yes_places_it_and_no_does_nothing() {
        let q = ask(None, at(1.0, 2.0));
        assert_eq!(q, MarkerAsk::Set(at(1.0, 2.0)));
        assert_eq!(apply(None, answer(q, 0)), Some(at(1.0, 2.0)));
        assert_eq!(answer(q, 1), MarkerAnswer::Declined);
        assert_eq!(apply(None, answer(q, 1)), None);
    }

    #[test]
    fn with_a_marker_it_moves_is_removed_or_stays() {
        let placed = Some(at(1.0, 2.0));
        let q = ask(placed, at(5.0, 6.0));
        assert_eq!(q, MarkerAsk::Change(at(5.0, 6.0)));
        assert_eq!(apply(placed, answer(q, 0)), Some(at(5.0, 6.0)));
        assert_eq!(apply(placed, answer(q, 1)), None);
        assert_eq!(answer(q, 2), MarkerAnswer::Left);
        assert_eq!(apply(placed, answer(q, 2)), placed);
    }

    #[test]
    fn the_box_has_two_answers_or_three_from_the_settings() {
        // A setting the load order defines wins; the others are NVForever's words.
        let look = |(name, ours): Word| if name == "sYes" { "Da".to_string() } else { ours.to_string() };
        let (q, b) = Words::of(MarkerAsk::Set(at(0.0, 0.0)), look);
        assert_eq!((q.as_str(), b.len(), b[0].as_str()), (Words::SET_QUESTION.1, 2, "Da"));
        let (q, b) = Words::of(MarkerAsk::Change(at(0.0, 0.0)), look);
        assert_eq!(q, Words::CHANGE_QUESTION.1);
        assert_eq!(b, [Words::MOVE.1, Words::REMOVE.1, Words::LEAVE.1]);
    }

    #[test]
    fn with_no_question_the_marker_toggles_or_follows_its_switches() {
        let (placed, here) = (Some(at(1.0, 2.0)), at(5.0, 6.0));
        assert_eq!(quick(None, here, Quick::Toggle, false, false), MarkerAnswer::Placed(here));
        assert_eq!(quick(placed, here, Quick::Toggle, false, false), MarkerAnswer::Removed);
        assert_eq!(quick(placed, here, Quick::ShiftRemoves, false, false), MarkerAnswer::Moved(here));
        assert_eq!(quick(placed, here, Quick::ShiftRemoves, true, false), MarkerAnswer::Removed);
        assert_eq!(quick(None, here, Quick::ShiftRemoves, true, false), MarkerAnswer::Declined);
        assert_eq!(quick(placed, here, Quick::RemoveOnIt, false, false), MarkerAnswer::Moved(here));
        assert_eq!(quick(placed, here, Quick::RemoveOnIt, false, true), MarkerAnswer::Removed);
        assert_eq!(Quick::of(true, true), Quick::ShiftRemoves);
        assert_eq!(Quick::of(false, true), Quick::RemoveOnIt);
        assert_eq!(Quick::of(false, false), Quick::Toggle);
    }

    #[test]
    fn near_is_across_the_ground_in_the_same_space() {
        let m = CustomMarker { space: 7, pos: [100.0, 0.0, 5000.0] };
        assert!(near(m, 7, [0.0, 0.0, 0.0], 100.0));
        assert!(!near(m, 7, [0.0, 0.0, 0.0], 99.0));
        assert!(!near(m, 8, [100.0, 0.0, 0.0], 100.0));
        assert!(!near(m, 7, [100.0, 0.0, 0.0], 0.0));
    }

    #[test]
    fn the_game_keeps_the_same_marker() {
        let m = CustomMarker { space: 0x0DA726, pos: [-70000.5, 2560.0, 8700.0] };
        assert_eq!(CustomMarker::from_game(m.to_game()), m);
    }

    #[test]
    fn the_dotted_line_runs_from_the_start_and_stops_short() {
        // 104 across at magnification 1: 100 long in pieces of 25.6, the last cut short.
        let p = dotted_line(Vec2::new(10.0, 20.0), Vec2::new(114.0, 20.0), 1.0, 25.6);
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].middle, Vec2::new(10.0 + 12.8, 20.0));
        let total: f32 = p.iter().map(|q| q.len).sum();
        assert!((total - 100.0).abs() < 1e-3);
        assert!((p[3].share - (100.0 - 3.0 * 25.6) / 25.6).abs() < 1e-4);
        assert_eq!(p[0].angle, 0.0);
        // At magnification 2 the same path shown 208 long is 200 long.
        let p = dotted_line(Vec2::ZERO, Vec2::new(0.0, 208.0), 2.0, 25.6);
        assert!((p.iter().map(|q| q.len).sum::<f32>() - 200.0).abs() < 1e-3);
        // Down the canvas (y runs down) is a quarter turn clockwise.
        assert!((p[0].angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // A target nearer than 4 has no line (the capture's −2.1 wide line drew nothing).
        assert!(dotted_line(Vec2::ZERO, Vec2::new(2.0, 0.0), 1.0, 25.6).is_empty());
    }

    #[test]
    fn a_local_map_point_goes_back_to_game_units() {
        let mid = Vec2::new(427.5, 250.0);
        let centre = [1000.0, 2000.0];
        // The window's middle is the centre; right is east, down is south.
        assert_eq!(local_point(centre, mid, 1.0, 28.0, mid), centre);
        assert_eq!(local_point(centre, mid, 2.0, 28.0, mid + Vec2::new(10.0, 10.0)), [1140.0, 1860.0]);
    }
}
