//! The tag-skill menu (`menus\chargen\char_gen_menu.xml`, menu 1048) and the trait menu
//! (`menus\trait_menu.xml`, 1084): a list at the left, the chosen row's picture and text
//! at the right, a counter under them, Reset and Done at the top right
//! (docs/spec-notes/chargen.md §3 and §4). Both are the same kind of menu with other
//! numbers, so one model and one drawing serve both.
//!
//! - [`ListMenu`] is the menu's state (the sheet, the start it resets to, the chosen
//!   row, the scroll); [`ListMenu::view`] reads what to draw from it and the game.
//! - Lengths are in the interface's units from the top left of the menus' 1024 × 800
//!   rectangle, centred on the screen.

use super::draw::{Align, Ctx, Which, BODY_H, THICK};
use crate::interface::{amber, at, u};
use bevy::prelude::*;
use fnv_game::chargen::{self, TagSheet, TraitSheet};
use fnv_game::game::menus;
use fnv_game::Game;

// ---- the menu files' numbers [documented: §3, §4] -----------------------------------------

/// The menus' rectangle, centred on the screen.
pub const MAIN: Vec2 = Vec2::new(1024.0, 800.0);
/// The dark picture behind: 1144 × 1070, 50 left of and 125 above the rectangle.
const BEHIND: [f32; 4] = [-50.0, -125.0, 1144.0, 1070.0];
/// The headline: a line 50 long, the title 20 after it and 10 above the line.
const HEAD_LINE: f32 = 50.0;
const TITLE_X: f32 = 70.0;
const TITLE_Y: f32 = -10.0;
/// The headline's corner lines are 60 long (vertical_fade_line.xml).
const HEAD_CORNER: f32 = 60.0;
/// The scroll bar at a list's left: 32 wide; its arrows 16 high, its marker 6 wide.
const BAR_W: f32 = 32.0;
const BAR_ARROW: f32 = 16.0;
/// The highlight box ends 5 short of the list's right side.
const BOX_SHORT: f32 = 5.0;
/// In a row: the mark at 15, 17.5 (25 square), the text 20 down.
const MARK: Vec2 = Vec2::new(15.0, 17.5);
const MARK_SIZE: f32 = 25.0;
const TEXT_Y: f32 = 20.0;
/// Reset and Done: right-aligned at 1014, from 60 down; a button is its text and 20
/// (10 each side), Done 52 under Reset in the tag-skill file and right under it (51) in
/// the trait file.
const BUTTONS: Vec2 = Vec2::new(1014.0, 60.0);
const BUTTON_PAD: f32 = 10.0;
/// A button the player cannot use: its brightness 127 of 255 (char_gen_menu.xml
/// `_line_alpha`).
const DIM: f32 = 127.0 / 255.0;

// ---- measured on the owner's screenshots [observed, §5] -------------------------------------

/// A row: 52 apart (13 rows over 468 pixels at 720 lines).
pub const ROW_H: f32 = 52.0;
/// The tag-skill list's top (the file's "set in code"): its highlight box 222.7 below
/// the rectangle's top on the fourth row.
const TAG_LIST_Y: f32 = 66.0;
/// The key hint ("R)") starts at the button's right end.
const HINT_X: f32 = 0.0;
/// How dark the original's picture behind makes the world: the world in it is 33 of 255 bright
/// against 54 just outside it on the trait screenshot (the world behind is also blurred
/// there; not built).
/// Ours is darker, the owner's wish ([`crate::interface::MENU_BACKING`]).
const SHADE: f32 = crate::interface::MENU_BACKING;
/// The scroll bar's marker: 105 long, its top 23 under the up arrow at the list's top
/// (sixteen traits, thirteen shown) [hypothesis: the same length for any list].
const MARKER_H: f32 = 105.0;
const MARKER_GAP: f32 = 23.0;

/// The numbers that differ between the two files.
struct Shape {
    list: [f32; 3],
    visible: usize,
    text_x: f32,
    wrap: f32,
    value_x: f32,
    icon: [f32; 3],
    info: [f32; 3],
    done_y: f32,
}

fn shape(code: u16) -> Shape {
    if code == menus::TAG_SKILLS {
        // [documented: char_gen_menu.xml] x -15, width 450, 14 rows; the template's text
        // at 40 wrapping at 390, its value centred at 360; the picture 400 square at 500,
        // 20; the info rectangle at 445, 420, 580 wide.
        Shape { list: [-15.0, TAG_LIST_Y, 450.0], visible: 14, text_x: 40.0, wrap: 390.0, value_x: 360.0, icon: [500.0, 20.0, 400.0], info: [445.0, 420.0, 580.0], done_y: 52.0 }
    } else {
        // [documented: trait_menu.xml] the list at -15, 60, 470 × 676; the template's
        // text at 45 wrapping at 360; the picture at 520, 0; the info rectangle 10 right
        // of the list, 385 down, to the rectangle's right side.
        let info_x = -15.0 + 470.0 + 10.0;
        Shape { list: [-15.0, 60.0, 470.0], visible: 13, text_x: 45.0, wrap: 360.0, value_x: 0.0, icon: [520.0, 0.0, 400.0], info: [info_x, 385.0, MAIN.x - info_x], done_y: BODY_H + 2.0 * BUTTON_PAD }
    }
}

// ---- the model -------------------------------------------------------------------------------

/// The sheet the menu edits, and the one Reset goes back to.
#[derive(Debug, Clone, PartialEq)]
pub enum Sheet {
    Tags { now: TagSheet, start: TagSheet },
    Traits { now: TraitSheet, start: TraitSheet, allowed: Vec<bool> },
}

/// A clickable part besides the rows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListButton {
    Reset,
    Done,
    ScrollUp,
    ScrollDown,
}

/// A clickable row (its index in the list).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRow(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct ListMenu {
    pub code: u16,
    pub sheet: Sheet,
    pub selected: usize,
    pub scroll: usize,
    /// The button under the pointer (drawn with its box).
    pub hover: Option<ListButton>,
}

/// One row as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct RowView {
    pub text: String,
    pub value: Option<String>,
    pub marked: bool,
    /// Drawn at half brightness: a trait whose requirements the player does not meet
    /// [hypothesis: not seen on a screenshot; the template's `_TextAlpha` says rows can
    /// be drawn fainter].
    pub dim: bool,
}

/// What the menu shows, read from its state and the game.
#[derive(Debug, Clone, PartialEq)]
pub struct ListView {
    pub code: u16,
    pub title: String,
    pub rows: Vec<RowView>,
    pub selected: usize,
    pub scroll: usize,
    pub icon: Option<String>,
    pub info: String,
    pub counter: String,
    pub done: bool,
    pub hover: Option<ListButton>,
}

impl ListMenu {
    /// The menu for `code` (1048 or 1084), from the player as the game holds them.
    pub fn open(code: u16, g: &mut Game) -> ListMenu {
        let sheet = if code == menus::TAG_SKILLS {
            let s = TagSheet::for_menu(g);
            Sheet::Tags { now: s.clone(), start: s }
        } else {
            let s = TraitSheet::of_player(g);
            let allowed = s.traits.iter().map(|&t| chargen::requirements_met(g, t)).collect();
            Sheet::Traits { now: s.clone(), start: s, allowed }
        };
        ListMenu { code, sheet, selected: 0, scroll: 0, hover: None }
    }

    pub fn rows(&self) -> usize {
        match &self.sheet {
            Sheet::Tags { now, .. } => now.skills.len(),
            Sheet::Traits { now, .. } => now.traits.len(),
        }
    }

    fn visible(&self) -> usize {
        shape(self.code).visible
    }

    /// Choose row `i` (pointer or keys), scrolling it into the list.
    pub fn select(&mut self, i: usize) {
        if i >= self.rows() {
            return;
        }
        self.selected = i;
        let v = self.visible();
        if i < self.scroll {
            self.scroll = i;
        } else if i >= self.scroll + v {
            self.scroll = i + 1 - v;
        }
    }

    /// Move the choice by `by` rows, stopping at the ends.
    pub fn step(&mut self, by: i32) {
        let n = self.rows() as i32;
        if n > 0 {
            self.select((self.selected as i32 + by).clamp(0, n - 1) as usize);
        }
    }

    /// Scroll the list by `by` rows (the wheel, the scroll bar's arrows).
    pub fn scroll_by(&mut self, by: i32) {
        let most = self.rows().saturating_sub(self.visible()) as i32;
        self.scroll = (self.scroll as i32 + by).clamp(0, most) as usize;
    }

    /// Tag or untag, take or give back the chosen row. Whether anything changed.
    pub fn toggle(&mut self) -> bool {
        let i = self.selected;
        match &mut self.sheet {
            Sheet::Tags { now, .. } => match now.skills.get(i).copied() {
                Some(s) => now.toggle(s),
                None => false,
            },
            Sheet::Traits { now, allowed, .. } => match now.traits.get(i).copied() {
                Some(t) => now.toggle(t, allowed.get(i).copied().unwrap_or(true)),
                None => false,
            },
        }
    }

    /// Reset: the sheet as the menu opened with it [hypothesis: the original's Reset
    /// may clear every choice instead; not captured].
    pub fn reset(&mut self) {
        match &mut self.sheet {
            Sheet::Tags { now, start } => *now = start.clone(),
            Sheet::Traits { now, start, .. } => *now = start.clone(),
        }
    }

    /// Whether Done may be pressed: three skills tagged; any number of traits up to the
    /// most [observed: "Done A)" dimmed at 2/3, bright at 3/3 and with no trait taken].
    pub fn can_finish(&self) -> bool {
        match &self.sheet {
            Sheet::Tags { now, .. } => now.complete(),
            Sheet::Traits { .. } => true,
        }
    }

    /// The name of row `i` (for the log and the test aid).
    pub fn row_name(&self, g: &Game, i: usize) -> Option<String> {
        match &self.sheet {
            Sheet::Tags { now, .. } => now.skills.get(i).map(|&s| chargen::skill_name(g, s)),
            Sheet::Traits { now, .. } => now.traits.get(i).and_then(|t| g.data.perks.get(t)).map(|p| p.name.clone()),
        }
    }

    /// What is chosen, for the log: the tagged skills' or taken traits' names.
    pub fn chosen_names(&self, g: &Game) -> Vec<String> {
        match &self.sheet {
            Sheet::Tags { now, .. } => now.skills.iter().filter(|s| now.tagged.contains(s)).map(|&s| chargen::skill_name(g, s)).collect(),
            Sheet::Traits { now, .. } => now.traits.iter().filter(|t| now.chosen.contains(t)).filter_map(|t| g.data.perks.get(t)).map(|p| p.name.clone()).collect(),
        }
    }

    pub fn view(&self, g: &mut Game) -> ListView {
        let (title, rows, icon, info, counter) = match &self.sheet {
            Sheet::Tags { now, .. } => {
                let rows: Vec<RowView> = now
                    .skills
                    .clone()
                    .into_iter()
                    .map(|s| RowView { text: chargen::skill_name(g, s), value: Some(now.value(g, s).to_string()), marked: now.tagged.contains(&s), dim: false })
                    .collect();
                let info = now.skills.get(self.selected).and_then(|&s| g.data.actor_value_info(s));
                (
                    tag_title(now.tagged.len(), now.count),
                    rows,
                    info.and_then(|i| i.icon.clone()),
                    info.map(|i| i.description.clone()).unwrap_or_default(),
                    tag_counter(now.remaining()),
                )
            }
            Sheet::Traits { now, allowed, .. } => {
                let rows = now
                    .traits
                    .iter()
                    .enumerate()
                    .map(|(i, t)| RowView {
                        text: g.data.perks.get(t).map(|p| p.name.clone()).unwrap_or_else(|| t.to_string()),
                        value: None,
                        marked: now.chosen.contains(t),
                        dim: !allowed.get(i).copied().unwrap_or(true) && !now.chosen.contains(t),
                    })
                    .collect();
                let t = now.traits.get(self.selected).copied();
                let p = t.and_then(|t| g.data.perks.get(&t));
                let info = match (t, p) {
                    (Some(t), Some(p)) => trait_info(&chargen::requirement_line(g, t), p.ranks, &p.description),
                    _ => String::new(),
                };
                (trait_title(g, now.max), rows, p.and_then(|p| p.icon.clone()), info, trait_counter(now.left()))
            }
        };
        ListView { code: self.code, title, rows, selected: self.selected, scroll: self.scroll, icon, info, counter, done: self.can_finish(), hover: self.hover }
    }
}

/// The tag-skill menu's title [observed: "SKILLS:  2/3 Selected"].
/// `count` is [`chargen::TAG_COUNT`], or more after AddTagSkills.
pub fn tag_title(tagged: usize, count: usize) -> String {
    format!("SKILLS:  {tagged}/{count} Selected")
}

/// The counter under the tag-skill menu's text [observed: "Tag 1 Skill" at 2/3, nothing
/// at 3/3; hypothesis: "Tag 2 Skills", "Tag 3 Skills"].
pub fn tag_counter(remaining: usize) -> String {
    match remaining {
        0 => String::new(),
        1 => "Tag 1 Skill".into(),
        n => format!("Tag {n} Skills"),
    }
}

/// The trait menu's title: the game's `sTraitMenuTitleText` with the most traits in its
/// `%d` [observed: "CHOOSE UP TO %d TRAITS (OR NONE)" in the masters, drawn "CHOOSE UP TO 2
/// TRAITS (OR NONE)"].
pub fn trait_title(g: &Game, max: usize) -> String {
    let t = match g.world.forms.settings.get("straitmenutitletext") {
        Some(fnv_sim::settings::Setting::Str(s)) => s.clone(),
        _ => "CHOOSE UP TO %d TRAITS (OR NONE)".into(),
    };
    t.replacen("%d", &max.to_string(), 1)
}

/// The counter under the trait menu's text [observed: "2 TRAITS LEFT", "0 TRAITS LEFT";
/// hypothesis: "1 TRAIT LEFT"].
pub fn trait_counter(left: usize) -> String {
    if left == 1 {
        "1 TRAIT LEFT".into()
    } else {
        format!("{left} TRAITS LEFT")
    }
}

/// The trait menu's text for a trait [observed: "Req: PER < 10", "Ranks: 1", an empty
/// line, then the description].
pub fn trait_info(requirements: &str, ranks: u8, description: &str) -> String {
    format!("Req: {requirements}\nRanks: {ranks}\n\n{}", description.trim())
}

/// The scroll bar's marker: its top (from the list's top) and height, or `None` when the
/// list fits [hypothesis: it moves evenly, one step per row scrolled].
pub fn marker(rows: usize, visible: usize, first: usize, list_h: f32) -> Option<(f32, f32)> {
    if rows <= visible {
        return None;
    }
    let (top, bottom) = (BAR_ARROW + MARKER_GAP, list_h - BAR_ARROW - MARKER_GAP);
    let h = MARKER_H.min(bottom - top);
    let steps = (rows - visible) as f32;
    let at = (first as f32).min(steps);
    Some((top + at / steps * (bottom - top - h), h))
}

// ---- drawing ---------------------------------------------------------------------------------

/// Spawn the menu under `root` (a full-screen node).
pub fn spawn(root: &mut ChildSpawnerCommands, v: &ListView, c: &mut Ctx) {
    let s = shape(v.code);
    let bright = amber(255.0);
    let main = Node { width: u(MAIN.x), height: u(MAIN.y), ..default() };
    root.spawn(main).with_children(|p| {
        let [bx, by, bw, bh] = BEHIND;
        let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
        if c.image(p, "interface\\shared\\background\\solid_black.dds", at(bx, by, bw, bh), shade, ()).is_none() {
            p.spawn((at(bx, by, bw, bh), BackgroundColor(shade)));
        }
        // The headline: the short line, the title, the long line, a line fading down at
        // each end.
        c.line(p, 0.0, 0.0, HEAD_LINE, THICK, bright);
        c.fade(p, 0.0, 0.0, HEAD_CORNER, false, bright);
        let w = c.text(p, Which::Title, &v.title, TITLE_X, TITLE_Y, Align::Left, bright);
        let h2 = TITLE_X + w + 20.0;
        c.line(p, h2, 0.0, MAIN.x - h2, THICK, bright);
        c.fade(p, MAIN.x - THICK, 0.0, HEAD_CORNER, false, bright);
        list(p, c, v, &s);
        // The chosen row's picture.
        if let Some(icon) = v.icon.as_deref() {
            let [x, y, size] = s.icon;
            c.image(p, icon, at(x, y, size, size), bright, ());
        }
        info(p, c, v, &s);
        buttons(p, c, v, &s);
        c.bottom_bracket(p, 0.0, MAIN.y, MAIN.x, bright);
    });
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &ListView, s: &Shape) {
    let bright = amber(255.0);
    let [x, y, w] = s.list;
    let h = s.visible as f32 * ROW_H;
    let node = Node { overflow: Overflow::clip(), ..at(x, y, w, h) };
    p.spawn(node).with_children(|l| {
        if let Some((my, mh)) = marker(v.rows.len(), s.visible, v.scroll, h) {
            c.image(l, "arrow_up.dds", at(0.0, 0.0, BAR_W, BAR_ARROW), bright, (Button, ListButton::ScrollUp));
            c.image(l, "arrow_down.dds", at(0.0, h - BAR_ARROW, BAR_W, BAR_ARROW), bright, (Button, ListButton::ScrollDown));
            if c.image(l, "vert_marker.dds", at(12.0, my, 6.0, mh), bright, ()).is_none() {
                l.spawn((at(12.0, my, 6.0, mh), BackgroundColor(bright)));
            }
        }
        let row_w = w - BAR_W - BOX_SHORT;
        for (k, i) in (v.scroll..v.rows.len()).take(s.visible).enumerate() {
            let r = &v.rows[i];
            let ry = k as f32 * ROW_H;
            l.spawn((Button, ListRow(i), at(BAR_W, ry, row_w, ROW_H))).with_children(|row| {
                if i == v.selected {
                    c.highlight(row, 0.0, 0.0, row_w, ROW_H - 1.0, 1.0);
                }
                let color = if r.dim { amber(128.0) } else { bright };
                if r.marked {
                    c.image(row, "square_filled.dds", at(MARK.x, MARK.y, MARK_SIZE, MARK_SIZE), color, ());
                }
                c.wrapped(row, &r.text, s.text_x, TEXT_Y, s.wrap, color);
                if let Some(val) = &r.value {
                    c.text(row, Which::Body, val, s.value_x, TEXT_Y, Align::Centre, color);
                }
            });
        }
    });
}

fn info(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &ListView, s: &Shape) {
    let bright = amber(255.0);
    let [x, y, w] = s.info;
    if v.code == menus::TAG_SKILLS {
        // [documented] Lines 560 long at the top and 300 down; the text at 20, 20
        // wrapping at 540; the counter 20 under the second line.
        c.line(p, x, y, 560.0, THICK, bright);
        c.line(p, x, y + 300.0 - THICK, 560.0, THICK, bright);
        c.wrapped(p, &v.info, x + 20.0, y + 20.0, 540.0, bright);
        if !v.counter.is_empty() {
            c.text(p, Which::Body, &v.counter, x + 20.0, y + 300.0 - THICK + 20.0, Align::Left, bright);
        }
    } else {
        // [documented] Lines 3 short of the width at the top and at 335, each with a line
        // fading down from its right end; the text 20 down, wrapping short of the scroll
        // bar, whose arrows stand at the right; the counter (title font) 20 under the
        // second line.
        let len = w - 3.0;
        let b = y + 335.0 - THICK;
        for yy in [y, b] {
            c.line(p, x, yy, len, THICK, bright);
            c.fade(p, x + w - THICK, yy, HEAD_CORNER, false, bright);
        }
        let text_w = w - BAR_W;
        let node = Node { overflow: Overflow::clip(), ..at(x, y + 20.0, text_w, 275.0) };
        p.spawn(node).with_children(|t| {
            c.wrapped(t, &v.info, 0.0, 5.0, text_w - BAR_W, bright);
        });
        // [documented: the bar 130 high, at 95 in the box 20 down; observed: its arrows at
        // 432 and 515 pixels on the screenshot, shown with a short text]
        let bar_y = y + 20.0 + 95.0;
        c.image(p, "arrow_up.dds", at(x + text_w, bar_y, BAR_W, BAR_ARROW), bright, ());
        c.image(p, "arrow_down.dds", at(x + text_w, bar_y + 130.0 - BAR_ARROW, BAR_W, BAR_ARROW), bright, ());
        c.text(p, Which::Title, &v.counter, x, b + 20.0, Align::Left, bright);
    }
}

fn buttons(p: &mut ChildSpawnerCommands, c: &mut Ctx, v: &ListView, s: &Shape) {
    let list = [(ListButton::Reset, "Reset", "R)", 0.0, true), (ListButton::Done, "Done", "A)", s.done_y, v.done)];
    for (b, label, hint, dy, on) in list {
        let color = amber(255.0).with_alpha(if on { 1.0 } else { DIM });
        let y = BUTTONS.y + dy;
        let w = c.width(Which::Body, label);
        let bw = w + 2.0 * BUTTON_PAD;
        let bh = BODY_H + 2.0 * BUTTON_PAD;
        if v.hover == Some(b) && on {
            c.outline(p, BUTTONS.x - bw, y, bw, bh, amber(200.0));
        }
        c.text(p, Which::Body, label, BUTTONS.x - BUTTON_PAD, y + BUTTON_PAD, Align::Right, color);
        c.text(p, Which::Body, hint, BUTTONS.x + HINT_X, y + BUTTON_PAD, Align::Left, color);
        p.spawn((Button, b, at(BUTTONS.x - bw, y, bw + 40.0, bh)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_sim::FormId;
    use std::collections::BTreeSet;

    fn traits(n: usize) -> ListMenu {
        let list: Vec<FormId> = (1..=n as u32).map(FormId).collect();
        let sheet = TraitSheet { traits: list, chosen: BTreeSet::new(), max: 2 };
        ListMenu { code: menus::TRAITS, sheet: Sheet::Traits { now: sheet.clone(), start: sheet, allowed: vec![true; n] }, selected: 0, scroll: 0, hover: None }
    }

    #[test]
    fn texts_as_the_screenshots_have_them() {
        assert_eq!(tag_title(2, chargen::TAG_COUNT), "SKILLS:  2/3 Selected");
        assert_eq!(tag_counter(1), "Tag 1 Skill");
        assert_eq!(tag_counter(0), "");
        assert_eq!(trait_counter(2), "2 TRAITS LEFT");
        assert_eq!(trait_counter(0), "0 TRAITS LEFT");
        assert_eq!(trait_info("PER < 10", 1, "While wearing glasses."), "Req: PER < 10\nRanks: 1\n\nWhile wearing glasses.");
    }

    #[test]
    fn the_list_scrolls_with_the_choice() {
        // Sixteen traits, thirteen shown.
        let mut m = traits(16);
        m.step(12);
        assert_eq!((m.selected, m.scroll), (12, 0));
        m.step(1);
        assert_eq!((m.selected, m.scroll), (13, 1));
        m.step(10);
        assert_eq!((m.selected, m.scroll), (15, 3));
        m.step(-15);
        assert_eq!((m.selected, m.scroll), (0, 0));
        m.scroll_by(10);
        assert_eq!(m.scroll, 3);
        assert!(marker(16, 13, 0, 676.0).is_some());
        assert!(marker(13, 14, 0, 728.0).is_none());
    }

    #[test]
    fn reset_goes_back_to_the_start() {
        let mut m = traits(4);
        assert!(m.toggle());
        m.select(2);
        assert!(m.toggle());
        m.select(3);
        assert!(!m.toggle(), "a third trait is refused");
        m.reset();
        match &m.sheet {
            Sheet::Traits { now, .. } => assert!(now.chosen.is_empty()),
            _ => unreachable!(),
        }
        assert!(m.can_finish());
    }
}
