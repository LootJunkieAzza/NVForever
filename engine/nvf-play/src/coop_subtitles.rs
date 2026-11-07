//! Subtitles of another player's conversation in co-op (queue item 33, owner playtest
//! 2026-11-06: "the guest hears the NPC's lines but sees no subtitles").
//!
//! - **The NPC's lines.** The lines another game's NPCs say reach this one as speech of
//!   its own (`Game::set_remote_speech`); their text is learned here when they first come,
//!   as this game learns the text of a line it starts itself (`crate::script`, on
//!   `LineStarted`), so the HUD subtitles them like any line said nearby: only within
//!   hearing (`crate::voice::audible`), in time with the voice.
//! - **The player's choices.** When this player picks an entry in a conversation, the
//!   topic and line go to everyone as a table action ([`game::CHOICE`], table 0); a game
//!   that hears it shows "<that player's name>: <the entry's text>" as a subtitle, said
//!   from where that player stands: only within `crate::voice::DIALOG_MAX_DISTANCE`, as a
//!   party quip is (`crate::party_quips`). Log: `co-op subtitle: <name> chose "<text>"`.

use crate::coop_social::SocialMates;
use crate::coop_tables::{game, Tables};
use crate::player::Player;
use crate::script::ScriptState;
use crate::script_hud::ScriptHud;
use crate::voice::{DIALOG_MAX_DISTANCE, MOUTH_HEIGHT};
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::time::{Duration, Instant};

/// How long a player's choice stays as a subtitle: the time it would take to say, by
/// the game's line length rule, at least [`MIN_SECONDS`].
const MIN_SECONDS: f64 = 2.0;

pub struct CoopSubtitlesPlugin;

impl Plugin for CoopSubtitlesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (learn_remote_lines, send_choices, hear_choices).in_set(PlaySet::Ui));
    }
}

/// The text of each remote line this game has not started itself.
fn learn_remote_lines(script: Res<ScriptState>, mut hud: ResMut<ScriptHud>, play: Option<Res<crate::PlayWorld>>) {
    let Some(rt) = script.rt.as_ref() else { return };
    for s in rt.game.speech.iter().filter(|s| s.remote) {
        if hud.knows_line(s.info) {
            continue;
        }
        let mut texts = play.as_deref().map(|p| crate::script::responses(p, s.info)).unwrap_or_default();
        if texts.is_empty() {
            texts = rt.game.data.infos.get(&s.info).map(|d| d.responses.iter().map(|r| r.text.clone()).collect()).unwrap_or_default();
        }
        debug!("co-op subtitle: learned the text of {}", rt.game.data.name(s.info));
        hud.remember_line(s.info, texts);
    }
}

/// The choice blob: topic and line, little-endian.
fn choice_blob(topic: FormId, info: FormId) -> Vec<u8> {
    let mut b = topic.0.to_le_bytes().to_vec();
    b.extend_from_slice(&info.0.to_le_bytes());
    b
}

fn read_choice(b: &[u8]) -> Option<(FormId, FormId)> {
    let t = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?);
    let i = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    Some((FormId(t), FormId(i)))
}

/// This player's choices, sent: a conversation's line that starts while the entries
/// offered the frame before had it is the one chosen.
fn send_choices(script: Res<ScriptState>, mates: Option<Res<SocialMates>>, mut tables: Option<ResMut<Tables>>, mut offered: Local<Vec<(FormId, FormId)>>, mut said: Local<Option<FormId>>) {
    let (Some(rt), Some(tables)) = (script.rt.as_ref(), tables.as_deref_mut()) else { return };
    let Some(c) = rt.game.dialogue.as_ref() else {
        offered.clear();
        *said = None;
        return;
    };
    let line = c.line.as_ref().map(|l| l.info);
    if line != *said {
        if let Some(info) = line {
            if let Some(&(topic, _)) = offered.iter().find(|(_, i)| *i == info) {
                if mates.as_deref().is_some_and(|m| m.me.is_some() && !m.mates.is_empty()) {
                    tables.send(0, game::CHOICE, choice_blob(topic, info));
                    info!("co-op subtitle: sent the choice {:?}", rt.game.data.choice_text(topic, info));
                }
            }
        }
        *said = line;
    }
    let now: Vec<(FormId, FormId)> = c.choices.iter().map(|ch| (ch.topic, ch.info)).collect();
    if !now.is_empty() {
        *offered = now;
    }
}

/// The others' choices, subtitled when heard.
fn hear_choices(mut tables: Option<ResMut<Tables>>, mates: Option<Res<SocialMates>>, script: Res<ScriptState>, player: Option<Res<Player>>, mut hud: ResMut<ScriptHud>) {
    let Some(tables) = tables.as_deref_mut() else { return };
    let choices = tables.take_actions(game::CHOICE);
    let Some(rt) = script.rt.as_ref() else { return };
    for (from, _, blob) in choices {
        let Some((topic, info)) = read_choice(&blob) else { continue };
        let text = crate::dialogue::shown_text(rt.game.data.choice_text(topic, info));
        let mate = mates.as_deref().and_then(|m| m.get(from));
        let name = mate.map_or_else(|| tables.name(from), |m| m.name.clone());
        let near = match (mate, player.as_deref()) {
            (Some(m), Some(p)) if m.here => {
                let e = p.eye();
                let at = [m.feet[0], m.feet[1], m.feet[2] + MOUTH_HEIGHT];
                ((at[0] - e[0]).powi(2) + (at[1] - e[1]).powi(2) + (at[2] - e[2]).powi(2)).sqrt() < DIALOG_MAX_DISTANCE
            }
            _ => false,
        };
        if text.is_empty() || !near {
            info!("co-op subtitle: {name} chose {text:?} [out of hearing]");
            continue;
        }
        let seconds = (fnv_game::game::LINE_BASE_SECONDS + text.chars().count() as f64 * fnv_game::game::LINE_SECONDS_PER_CHAR).max(MIN_SECONDS);
        info!("co-op subtitle: {name} chose {text:?}");
        hud.party_lines.push((name, text, Instant::now() + Duration::from_secs_f64(seconds)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_goes_as_its_topic_and_line() {
        let b = choice_blob(FormId(0x0010_A1E4), FormId(0x0010_B755));
        assert_eq!(read_choice(&b), Some((FormId(0x0010_A1E4), FormId(0x0010_B755))));
        assert_eq!(read_choice(&b[..5]), None);
    }
}
