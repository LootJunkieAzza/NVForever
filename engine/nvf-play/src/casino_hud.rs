//! The casino games' score panel and end-of-match leaderboard, drawn in the HUD's look:
//! the HUD's font (font 7) and colour, lines in the menus' brackets (a top line with
//! short sides down, a bottom line with short sides up) over a dark glass fill [design,
//! owner's request 2026-11-09: "match hud font and color"].
//!
//! Sizes are interface units (the view is 960 high), as the HUD's.

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

use crate::interface::{amber, bc_supported, scaled_face, u, Face};
use crate::start_menu::{text, Faces};

/// The faces, read once the game's files are there.
#[derive(Resource)]
pub struct CasinoFaces {
    pub faces: Faces,
    /// The leaderboard's title: the HUD font at 1.6 times.
    pub title: Face,
}

impl CasinoFaces {
    fn new(faces: Faces) -> CasinoFaces {
        let title = scaled_face(&faces.entry, 1.6);
        CasinoFaces { faces, title }
    }

    /// Stand-in fonts (tests, and before the game's files are read).
    pub fn plain() -> CasinoFaces {
        CasinoFaces::new(Faces::plain())
    }
}

pub struct CasinoHudPlugin;

impl Plugin for CasinoHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, load.run_if(not(resource_exists::<CasinoFaces>)));
    }
}

fn load(mut commands: Commands, data: Option<Res<GameData>>, mut images: Option<ResMut<Assets<Image>>>, formats: Option<Res<CompressedImageFormatSupport>>) {
    let (Some(data), Some(images)) = (data, images.as_deref_mut()) else { return };
    let bc = bc_supported(formats.as_deref());
    commands.insert_resource(CasinoFaces::new(Faces::load(Some(&data), images, bc)));
}

/// The menus' line width.
const LINE: f32 = 2.0;
/// The brackets' short sides.
const SIDE: f32 = 16.0;
/// The panels' inner margin.
const PAD: f32 = 14.0;
/// The HUD's distance from the view's left and top edges.
const MARGIN: f32 = 40.0;
const FILL: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);

fn line_color() -> Color {
    amber(200.0)
}

fn rule(p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32) {
    p.spawn((Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(w), height: u(h), ..default() }, BackgroundColor(line_color())));
}

/// A box `w` by `h` from its parent's corner: fill, and the brackets.
fn bracket_box(p: &mut ChildSpawnerCommands, w: f32, h: f32) {
    p.spawn((Node { position_type: PositionType::Absolute, left: u(0.0), top: u(0.0), width: u(w), height: u(h), ..default() }, BackgroundColor(FILL)));
    rule(p, 0.0, 0.0, w, LINE);
    rule(p, 0.0, 0.0, LINE, SIDE);
    rule(p, w - LINE, 0.0, LINE, SIDE);
    rule(p, 0.0, h - LINE, w, LINE);
    rule(p, 0.0, h - SIDE, LINE, SIDE);
    rule(p, w - LINE, h - SIDE, LINE, SIDE);
}

/// One side's line on the score panel.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreRow {
    pub name: String,
    pub score: String,
    /// What it threw this turn, or other short notes.
    pub note: String,
    /// Its turn now.
    pub up: bool,
}

/// The score panel, top left at the HUD's margin. `foot` is a dimmer last line (the stake).
pub fn score_panel(p: &mut ChildSpawnerCommands, f: &CasinoFaces, rows: &[ScoreRow], foot: Option<&str>) {
    let face = &f.faces.entry;
    let small = &f.faces.small;
    let step = face.font.size * 1.05;
    let mark = face.font.width("> ");
    let name_w = rows.iter().map(|r| face.font.width(&r.name)).fold(0.0, f32::max);
    let score_w = rows.iter().map(|r| face.font.width(&r.score)).fold(0.0, f32::max);
    let note_w = rows.iter().map(|r| small.font.width(&r.note)).fold(0.0, f32::max);
    let foot_w = foot.map_or(0.0, |s| small.font.width(s));
    let gap = 24.0;
    let w = (PAD * 2.0 + mark + name_w + gap + score_w + if note_w > 0.0 { gap + note_w } else { 0.0 }).max(PAD * 2.0 + foot_w).max(220.0);
    let foot_h = if foot.is_some() { small.font.size * 1.1 } else { 0.0 };
    let h = PAD * 2.0 + step * rows.len() as f32 + foot_h;
    p.spawn(Node { position_type: PositionType::Absolute, left: u(MARGIN), top: u(MARGIN + 30.0), width: u(w), height: u(h), ..default() }).with_children(|b| {
        bracket_box(b, w, h);
        for (i, r) in rows.iter().enumerate() {
            let base = PAD + step * (i as f32 + 0.8);
            let c = if r.up { amber(255.0) } else { amber(190.0) };
            if r.up {
                text(b, face, ">", PAD, base, c);
            }
            text(b, face, &r.name, PAD + mark, base, c);
            let sx = PAD + mark + name_w + gap;
            text(b, face, &r.score, sx + score_w - face.font.width(&r.score), base, c);
            if !r.note.is_empty() {
                text(b, small, &r.note, sx + score_w + gap, base, amber(170.0));
            }
        }
        if let Some(s) = foot {
            text(b, small, s, PAD, PAD + step * rows.len() as f32 + small.font.size * 0.85, amber(170.0));
        }
    });
}

/// One place on the leaderboard.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub detail: String,
}

/// The order of the sides at a match's end: the winner first, then the rest by `key`
/// (smaller first), seat order breaking ties.
pub fn ranked(winner: usize, keys: &[i64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by_key(|&i| (i != winner, keys[i], i));
    order
}

const BOARD_W: f32 = 520.0;
const BOARD_TOP: f32 = 45.0;

/// The leaderboard at the top middle of the view: "<winner> WINS" over a line, then the
/// places numbered from 1, each over a line.
pub fn leaderboard(p: &mut ChildSpawnerCommands, f: &CasinoFaces, winner: &str, places: &[Place]) {
    let title = format!("{} WINS", winner.to_uppercase());
    let face = &f.faces.entry;
    let small = &f.faces.small;
    let title_h = f.title.font.size * 1.2;
    let step = face.font.size * 1.25;
    let w = BOARD_W.max(f.title.font.width(&title) + PAD * 4.0);
    let h = PAD * 2.0 + title_h + step * places.len() as f32;
    p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(-w / 2.0)), top: u(BOARD_TOP), width: u(w), height: u(h), ..default() }).with_children(|b| {
        bracket_box(b, w, h);
        text(b, &f.title, &title, (w - f.title.font.width(&title)) / 2.0, PAD + f.title.font.size * 0.85, amber(255.0));
        rule(b, PAD, PAD + title_h, w - PAD * 2.0, LINE);
        for (i, pl) in places.iter().enumerate() {
            let top = PAD + title_h + step * i as f32;
            let base = top + step * 0.72;
            let c = if i == 0 { amber(255.0) } else { amber(200.0) };
            text(b, face, &format!("{}", i + 1), PAD * 2.0, base, c);
            text(b, face, &pl.name, PAD * 2.0 + face.font.width("4") + 30.0, base, c);
            text(b, small, &pl.detail, w - PAD * 2.0 - small.font.width(&pl.detail), base, c);
            if i + 1 < places.len() {
                rule(b, PAD * 2.0, top + step - LINE, w - PAD * 4.0, 1.0);
            }
        }
    });
}

/// A help line at the HUD's bottom left, in the HUD font.
pub fn help_line(p: &mut ChildSpawnerCommands, f: &CasinoFaces, lines: &str) {
    let face = &f.faces.small;
    let step = face.font.size * 1.1;
    let n = lines.lines().count().max(1) as f32;
    p.spawn(Node { position_type: PositionType::Absolute, left: u(MARGIN), bottom: u(MARGIN + 60.0), width: u(900.0), height: u(step * n + 6.0), ..default() }).with_children(|b| {
        for (i, l) in lines.lines().enumerate() {
            text(b, face, l, 0.0, step * (i as f32 + 0.85), amber(255.0));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_winner_leads_then_the_lowest_left() {
        // 301 counts down: the winner at 0, then 40 left before 100 left.
        assert_eq!(ranked(2, &[100, 40, 0]), vec![2, 1, 0]);
        // Ties keep seat order.
        assert_eq!(ranked(1, &[100, 0, 100, 100]), vec![1, 0, 2, 3]);
    }
}
