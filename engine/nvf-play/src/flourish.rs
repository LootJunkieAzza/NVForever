//! Minigame highlights (item 125): a short stamp across the screen and a sound, for the
//! big moments of a match. There are no camera moves [owner 2026-11-08].
//!
//! - **Which moments** [owner 2026-11-08]:
//!   - darts: a bullseye, a triple;
//!   - pool: two or more balls down in one shot;
//!   - blackjack and poker: a hand won.
//! - **Everyone in the match sees and hears it.** No new co-op message is needed. Each
//!   screen finds the moment in the state it already shows: the darts sides' hits, the
//!   pool table's balls, the blackjack seats' settled rounds, the poker table's winners.
//!   The board's or table's owner and the players at it see the same match, so each plays
//!   the flourish itself [design].
//! - **The sounds** are SOUN records of the base game, found by Editor ID. A missing one
//!   falls back to `UIPopUpExperienceUp` [observed: the Editor IDs are in FalloutNV.esm;
//!   which one fits each moment: design, to be tuned by ear].
//! - **The words** are NVForever's own (LEGAL.md).
//!
//! A `flourish:` line in the log for each one.

use crate::minigames::DartsScreen;
use crate::pool::PoolScreen;
use crate::sfx::{PlaySound, Sfx};
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::HashMap;

/// A body's reaction to a card game's result (item 127): who (`None` this player, else
/// another player's id) and whether it won the hand (blackjack, poker) or the game
/// (Caravan). The renderer plays a cheer or a beaten idle on that body
/// (`nvforever::match_end`). Each screen finds the results in the table it already shows,
/// so every screen at the table sends the same ones [design].
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandMood {
    pub who: Option<u8>,
    pub won: bool,
}

/// A big moment of a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Highlight {
    Bullseye,
    Triple,
    MultiBall,
    HandWon,
}

impl Highlight {
    /// The SOUN Editor IDs that may play for it (one picked at a time).
    pub fn sounds(self) -> &'static [&'static str] {
        match self {
            Highlight::Bullseye => &["AMBPittCrowdCheer01", "AMBPittCrowdCheer02", "AMBPittCrowdCheer03"],
            Highlight::Triple => &["UIPopUpExperienceUp"],
            Highlight::MultiBall => &["AMBPittCrowdAhh01", "AMBPittCrowdAhh02", "AMBPittCrowdAhh03"],
            Highlight::HandWon => &["AMBCasinoChipsOS"],
        }
    }

    fn color(self) -> Color {
        match self {
            Highlight::Bullseye => Color::srgb(1.0, 0.42, 0.30),
            Highlight::Triple => Color::srgb(0.55, 0.95, 0.55),
            Highlight::MultiBall => Color::srgb(0.98, 0.82, 0.38),
            Highlight::HandWon => Color::srgb(0.98, 0.86, 0.45),
        }
    }
}

/// Show a highlight: its stamp and its sound.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Flourish {
    pub kind: Highlight,
    pub text: String,
}

const FALLBACK: &str = "UIPopUpExperienceUp";
/// How long a stamp stays: it pops in, holds, then fades out [design].
const POP: f32 = 0.18;
const HOLD: f32 = 1.1;
const FADE: f32 = 0.6;

#[derive(Component)]
struct Stamp {
    born: f32,
    color: Color,
}

/// What each game showed last time, to find what is new.
#[derive(Resource, Default)]
struct Seen {
    /// Darts: the board, and for each side its turn count and how many darts it had.
    darts: Option<(u32, Vec<(u32, usize)>)>,
    /// Pool: the table, and the object balls on it when the balls were last still.
    pool: Option<(u32, usize)>,
}

/// The new highlights of a darts side's turn: its darts from `had` on.
pub fn darts_highlights(hits: &[fnv_game::darts::Hit], had: usize) -> Vec<Flourish> {
    hits.iter()
        .skip(had)
        .filter_map(|h| match (h.number, h.multiplier) {
            (25, 2) => Some(Flourish { kind: Highlight::Bullseye, text: "BULLSEYE".into() }),
            (n, 3) if n > 0 => Some(Flourish { kind: Highlight::Triple, text: format!("TRIPLE {n}") }),
            _ => None,
        })
        .collect()
}

/// The pool highlight when the balls come to rest with `now` object balls on the table,
/// `before` the last time they were still.
pub fn pool_highlight(before: usize, now: usize) -> Option<Flourish> {
    let down = before.checked_sub(now)?;
    (down >= 2).then(|| Flourish { kind: Highlight::MultiBall, text: format!("{down} BALLS IN ONE SHOT") })
}

/// Darts and pool: the highlights in what this screen shows.
fn watch(darts: Res<DartsScreen>, pool: Res<PoolScreen>, mut seen: ResMut<Seen>, mut out: MessageWriter<Flourish>) {
    match darts.0.as_ref().filter(|d| d.page == crate::minigames::Page::Play) {
        Some(d) => {
            let now: Vec<(u32, usize)> = d.m.sides.iter().map(|s| (s.turns, s.darts.len())).collect();
            if let Some((board, before)) = seen.darts.as_ref().filter(|(b, _)| *b == d.board) {
                let _ = board;
                for (i, side) in d.m.sides.iter().enumerate() {
                    // The same turn: the darts after those seen; a new turn: all of them.
                    let had = match before.get(i) {
                        Some(&(turns, n)) if turns == side.turns && n <= side.darts.len() => n,
                        _ => 0,
                    };
                    for f in darts_highlights(&side.darts, had) {
                        out.write(f);
                    }
                }
            }
            seen.darts = Some((d.board, now));
        }
        None => seen.darts = None,
    }
    match pool.0.as_ref().filter(|p| p.page == crate::pool::Page::Play) {
        Some(p) => {
            let still = p.shot.is_none() && !p.table.moving();
            if still {
                let on = p.table.balls[1..].iter().filter(|b| b.on).count();
                if let Some(f) = seen.pool.filter(|(t, _)| *t == p.table_ref).and_then(|(_, before)| pool_highlight(before, on)) {
                    out.write(f);
                }
                seen.pool = Some((p.table_ref, on));
            }
        }
        None => seen.pool = None,
    }
}

/// Poker: the hands won whose time came (`poker::Session::wins`, aid seconds).
fn poker_wins(time: crate::pause::AidTime, mut poker: ResMut<crate::poker::PokerScreen>, mut out: MessageWriter<Flourish>, mut moods: MessageWriter<HandMood>) {
    let Some(s) = poker.0.as_mut() else { return };
    let now = time.elapsed_secs();
    if !s.moods.is_empty() {
        let (due, later): (Vec<_>, Vec<_>) = s.moods.drain(..).partition(|m| m.0 <= now);
        s.moods = later;
        for (_, who, won) in due {
            moods.write(HandMood { who, won });
        }
    }
    if s.wins.is_empty() {
        return;
    }
    let (due, later): (Vec<_>, Vec<_>) = s.wins.drain(..).partition(|w| w.0 <= now);
    s.wins = later;
    for (_, text) in due {
        out.write(Flourish { kind: Highlight::HandWon, text });
    }
}

/// The stamps and the sounds.
#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut asked: MessageReader<Flourish>,
    sfx: Option<Res<Sfx>>,
    mut cache: Local<HashMap<&'static str, Option<FormId>>>,
    mut turn: Local<u32>,
    mut sounds: MessageWriter<PlaySound>,
    old: Query<Entity, With<Stamp>>,
) {
    let now = time.elapsed_secs();
    for f in asked.read() {
        info!("flourish: {:?} {:?}", f.kind, f.text);
        // A new stamp takes the place of one still showing.
        for e in &old {
            commands.entity(e).despawn();
        }
        let color = f.kind.color();
        commands
            .spawn((
                Stamp { born: now, color },
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: Val::Percent(20.0), justify_content: JustifyContent::Center, ..default() },
                GlobalZIndex(60),
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn((Text::new(f.text.clone()), TextFont { font_size: FontSize::Px(46.0), ..default() }, TextColor(color)));
            });
        let names = f.kind.sounds();
        *turn = turn.wrapping_add(1);
        let name = names[*turn as usize % names.len()];
        let Some(sfx) = sfx.as_deref() else { continue };
        let mut find = |n: &'static str| *cache.entry(n).or_insert_with(|| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(n))).map(|(id, _)| *id));
        match find(name).or_else(|| find(FALLBACK)) {
            Some(sound) => {
                info!("flourish: sound {name}");
                sounds.write(PlaySound { sound, at: None });
            }
            None => warn!("flourish: sound {name} not found"),
        }
    }
}

/// The stamp pops in a little larger, settles, holds, then fades.
fn animate(mut commands: Commands, time: Res<Time<Real>>, stamps: Query<(Entity, &Stamp, &Children)>, mut looks: Query<(&mut TextColor, &mut UiTransform)>) {
    let now = time.elapsed_secs();
    for (e, s, kids) in &stamps {
        let age = now - s.born;
        if age >= POP + HOLD + FADE {
            commands.entity(e).despawn();
            continue;
        }
        let scale = if age < POP { 1.6 - 0.6 * (age / POP) } else { 1.0 + 0.04 * (1.0 - ((age - POP) / HOLD).min(1.0)) };
        let alpha = if age < POP + HOLD { (age / 0.08).min(1.0) } else { 1.0 - (age - POP - HOLD) / FADE };
        for &k in kids {
            if let Ok((mut c, mut t)) = looks.get_mut(k) {
                c.0 = s.color.with_alpha(alpha.clamp(0.0, 1.0));
                t.scale = Vec2::splat(scale);
            }
        }
    }
}

pub struct FlourishPlugin;

impl Plugin for FlourishPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Seen>().add_message::<Flourish>().add_message::<HandMood>().add_systems(Update, (watch, poker_wins, show, animate).chain().after(PlaySet::Ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::darts::Hit;

    fn hit(number: u8, multiplier: u8) -> Hit {
        Hit { number, multiplier, off_board: false }
    }

    #[test]
    fn darts_bull_and_triples_are_highlights_only_once() {
        let darts = [hit(20, 3), hit(5, 1), hit(25, 2)];
        let all = darts_highlights(&darts, 0);
        assert_eq!(all.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), ["TRIPLE 20", "BULLSEYE"]);
        assert_eq!(darts_highlights(&darts, 1).len(), 1);
        assert!(darts_highlights(&darts, 3).is_empty());
        // The outer bull (25) and a miss are not.
        assert!(darts_highlights(&[hit(25, 1), hit(0, 3)], 0).is_empty());
    }

    #[test]
    fn two_balls_or_more_in_one_shot() {
        assert!(pool_highlight(15, 14).is_none());
        assert_eq!(pool_highlight(15, 13).map(|f| f.text), Some("2 BALLS IN ONE SHOT".into()));
        // A new rack puts balls back: nothing.
        assert!(pool_highlight(3, 15).is_none());
    }
}
