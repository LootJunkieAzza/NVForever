//! Player voices in co-op, and the comments and reactions of the player's own voice
//! (docs/design/player-voices.md §4-§6, phases 2-4; `fnv_game::player_voice`).
//!
//! - **Sending.** In a co-op session with others in it the game picks the player's quips
//!   among all its voice's lines (`QuipSettings::coop`) and lists them
//!   (`QuipState::out`); each is sent to everyone as a table action
//!   (`coop_tables::game::VOICE`, table 0): the voice set's id and the INFO. Log:
//!   `party quip: sent <set> <info>`.
//! - **The player's own lines** (item 106, 2026-11-09): a combat line the player says that
//!   is not its voice set's (the GenericPlayer hit, power-attack and death grunts) is sent
//!   too ([`OWN_LINE`], with the voice type it is said in), once per event: when a set line
//!   went out for the event, that is what the others hear. Log: `party quip: sent own line`.
//! - **Hearing.** A quip from another player is checked against its set (the line must
//!   be one of the set's, resolved here as there: the same load order and sets) and
//!   played from that player's character, at its mouth, by distance and side like any
//!   line said nearby, at the set's rate, with a subtitle "Name: text"; not at all with
//!   Party Quips off, or out of hearing (another place, beyond the dialogue distance).
//!   While it plays, and [`GROUP_COOLDOWN`] after, this player's own quips wait
//!   (`QuipState::hold_until`). Log: `party quip: heard <name> (<set>) ...`.
//! - **Reactions** (phase 3): another player near this one throws a grenade, lays a mine,
//!   fires a gun out of a fight, or holds its sights on this player for
//!   [`AIMED_AT_SECONDS`]: this player's voice answers (`Game::player_reacts`), heard by
//!   the others and, for a first-person line, here. Log: `party quip: <name> <deed>: ...`.
//! - **Comments** (phase 4): every frame the game looks at its place and quest comments
//!   (`Game::player_comments`); also without co-op. Log: `player voice: comment <event>
//!   <line>: <text>`.
//! - **Fight remarks** (item 64, 2026-11-07): as a fight with the player begins and a few
//!   seconds after it ends (`Game::player_fight_lines`); also solo. Log: `player voice:
//!   fight start|fight end <line>: <text>`.
//! - Every trigger that said nothing is logged with why: `player voice: <event>: nothing
//!   said: <why>` (another quip playing, too soon after the last line, the set has no
//!   first-person line for it, ...).

use crate::coop_social::SocialMates;
use crate::coop_tables::{game, Tables, R, W};
use crate::player::Player;
use crate::player_voice::{QuipSwitches, VoiceSets};
use crate::script::ScriptState;
use crate::sfx::{heard_by, heard_distance, heard_pan, Pan, Panned, SecondListener};
use crate::sim::SimState;
use crate::voice::{voice_gain, VoiceSettings, VoiceSound, VoiceState, DIALOG_MAX_DISTANCE, MOUTH_HEIGHT};
use crate::PlaySet;
use crate::audio_out::SoundCategory;
use bevy::audio::Volume;
use bevy::prelude::*;
use fnv_game::player_voice::{Deed, PlayerVoice, AIMED_AT_SECONDS, GROUP_COOLDOWN, REACT_RANGE};
use fnv_sim::FormId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Another player's sights on this one: within this angle of its facing (radians) [design].
const AIM_CONE: f32 = 0.18;

/// The others' voices as heard here, and what they did last.
#[derive(Resource, Default)]
pub struct PartyQuips {
    /// Set id -> the set resolved here (`None`: it does not resolve or is not installed).
    voices: HashMap<String, Option<PlayerVoice>>,
    /// Per player: its attack counter at the last look, and since when it has aimed at
    /// this player (real seconds).
    seen: HashMap<u8, (u16, Option<f64>)>,
    pub sent: u64,
    pub heard: u64,
    /// The comment lines said by the game so far, logged.
    logged: usize,
}

impl PartyQuips {
    fn voice(&mut self, sets: &VoiceSets, data: &fnv_game::data::GameData, id: &str) -> Option<&PlayerVoice> {
        self.voices
            .entry(id.to_string())
            .or_insert_with(|| match sets.get(id).map(|s| PlayerVoice::resolve(s, data)) {
                Some(Ok((v, _))) => Some(v),
                Some(Err(e)) => {
                    warn!("party quip: {e}");
                    None
                }
                None => {
                    warn!("party quip: voice set {id} is not installed here: that player is not heard");
                    None
                }
            })
            .as_ref()
    }
}

/// The blob of a quip: the set's id and the line.
pub fn quip_blob(set: &str, info: u32) -> Vec<u8> {
    let mut w = W::default();
    w.str(set).u32(info);
    w.0
}

/// A quip's blob read back.
pub fn read_quip(blob: &[u8]) -> Option<(String, u32)> {
    let mut r = R(blob);
    Some((r.str()?, r.u32()?))
}

/// The set id of a player's own line (a grunt, not a voice set's line; item 106): the blob
/// is a quip's with the voice type the line is said in after it (0: the speaker's
/// record's). A game from before item 106 reads it as a set it does not have, warns once
/// and plays nothing.
pub const OWN_LINE: &str = "@own";

/// The blob of a player's own line.
pub fn own_line_blob(info: u32, voice_type: u32) -> Vec<u8> {
    let mut w = W::default();
    w.str(OWN_LINE).u32(info).u32(voice_type);
    w.0
}

/// The voice type of an own line's blob (after the quip part).
pub fn read_own_voice_type(blob: &[u8]) -> Option<u32> {
    let mut r = R(blob);
    r.str()?;
    r.u32()?;
    r.u32()
}

/// Whether someone at `from` facing `yaw` (compass radians) with its eye at `eye` looks
/// at `at` within [`AIM_CONE`], nearer than [`REACT_RANGE`].
pub fn aims_at(eye: [f32; 3], yaw: f32, at: [f32; 3]) -> bool {
    let d = [at[0] - eye[0], at[1] - eye[1]];
    let len = d[0].hypot(d[1]);
    if !(1.0..REACT_RANGE).contains(&len) {
        return false;
    }
    let cos = (d[0] * yaw.sin() + d[1] * yaw.cos()) / len;
    cos >= AIM_CONE.cos()
}

/// The co-op switch into the game, the player's quips out.
fn send(mates: Option<Res<SocialMates>>, mut tables: Option<ResMut<Tables>>, mut script: ResMut<ScriptState>, mut party: ResMut<PartyQuips>) {
    let Some(rt) = script.rt.as_mut() else { return };
    let g = &mut rt.game;
    let others = mates.as_deref().is_some_and(|m| !m.mates.is_empty());
    let coop = tables.as_deref().is_some_and(|t| t.on) && others;
    if g.quips.coop != coop {
        g.quips.coop = coop;
        info!("party quip: {}", if coop { "the other players hear this player's voice" } else { "no other players: quips are this player's own" });
    }
    let out = std::mem::take(&mut g.quip.out);
    let own = std::mem::take(&mut g.quip.own_out);
    let Some(tables) = tables.as_deref_mut() else { return };
    // The player's own lines (grunts): heard by the others from this player's character
    // too, in the voice it says them in here (item 106).
    for (info, vt) in own {
        tables.send(0, game::VOICE, own_line_blob(info.0, vt.map_or(0, |v| v.0)));
        party.sent += 1;
        info!("party quip: sent own line {} (voice type {})", g.data.name(info), vt.map_or_else(|| "the record's".to_string(), |v| g.data.name(v)));
    }
    let Some(v) = g.player_voice.as_ref() else { return };
    for info in out {
        tables.send(0, game::VOICE, quip_blob(&v.id, info.0));
        party.sent += 1;
        info!("party quip: sent {} {}", v.id, g.data.name(info));
    }
}

/// The others' quips heard.
#[allow(clippy::too_many_arguments)]
fn hear(
    mut commands: Commands,
    mut tables: Option<ResMut<Tables>>,
    mates: Option<Res<SocialMates>>,
    (sets, switches, settings): (Res<VoiceSets>, Res<QuipSwitches>, Res<VoiceSettings>),
    voice: Res<VoiceState>,
    mut script: ResMut<ScriptState>,
    (player, second): (Option<Res<Player>>, Option<Res<SecondListener>>),
    mut panned: Option<ResMut<Assets<Panned<AudioSource>>>>,
    mut hud: ResMut<crate::script_hud::ScriptHud>,
    (mut party, play): (ResMut<PartyQuips>, Option<Res<crate::PlayWorld>>),
) {
    let now = Instant::now();
    hud.party_lines.retain(|l| l.2 > now);
    let Some(tables) = tables.as_deref_mut() else { return };
    let quips = tables.take_actions(game::VOICE);
    if quips.is_empty() {
        return;
    }
    let (Some(rt), Some(bank)) = (script.rt.as_mut(), voice.bank.clone()) else { return };
    let g = &mut rt.game;
    for (from, _, blob) in quips {
        let Some((set, info)) = read_quip(&blob) else { continue };
        let info = FormId(info);
        let name = mates.as_deref().and_then(|m| m.get(from)).map_or_else(|| tables.name(from), |m| m.name.clone());
        if !switches.party && set != OWN_LINE {
            info!("party quip: {name} said {set} {} [Party Quips is off: not heard]", g.data.name(info));
            continue;
        }
        // A player's own line (a grunt, item 106): in the voice type it was said in, else
        // the Player record's; not held back by the Party Quips switch.
        let (path, rate, own) = if set == OWN_LINE {
            let vt = read_own_voice_type(&blob).filter(|&v| v != 0);
            let path = vt.and_then(|v| bank.file_for_voice(info.0, 0, v)).or_else(|| bank.file(info.0, 0, FormId::PLAYER_BASE.0));
            (path, 1.0f32, true)
        } else {
            let Some(v) = party.voice(&sets, g.data, &set) else { continue };
            if !v.lines.contains(&info) {
                warn!("party quip: {name}: {} is not a line of {set} here", g.data.name(info));
                continue;
            }
            let (base, rate) = (v.base, v.rate);
            (bank.file(info.0, 0, base.0).or_else(|| g.data.voice_files.get(&info).filter(|p| bank.vfs().exists(p)).cloned()), rate, false)
        };
        let text = line_text(g, play.as_deref(), info);
        let mate = mates.as_deref().and_then(|m| m.get(from));
        let heard = match (mate, player.as_deref()) {
            (Some(m), Some(p)) if m.here => {
                let at = [m.feet[0], m.feet[1], m.feet[2] + MOUTH_HEIGHT];
                // Heard by the nearer listener (split screen's player 2 too).
                let (eye, second) = (p.eye(), second.as_deref());
                let d = heard_distance(eye, second, at);
                (d < DIALOG_MAX_DISTANCE).then(|| {
                    let [l, r] = heard_pan(eye, p.heading, second, at);
                    let gain = heard_by(eye, second, at, voice_gain);
                    (d, [l * gain, r * gain])
                })
            }
            _ => None,
        };
        let Some((d, gains)) = heard else {
            info!("party quip: {name} said {set} {} [out of hearing]", g.data.name(info));
            continue;
        };
        let seconds = path.as_deref().and_then(|p| bank.seconds(p)).unwrap_or(2.0) / f64::from(rate);
        // A grunt holds back nobody's quips and shows no subtitle.
        if !own {
            g.quip.hold_until = g.quip.hold_until.max(g.time + seconds + GROUP_COOLDOWN);
        }
        if !text.is_empty() && !own {
            hud.party_lines.push((name.clone(), text.clone(), now + Duration::from_secs_f64(seconds.max(1.5))));
        }
        party.heard += 1;
        let Some(path) = path else {
            info!("party quip: heard {name} ({set}) line {} with no file: \"{text}\"", g.data.name(info));
            continue;
        };
        info!("party quip: heard {name} ({set}) line {}: {path} ({seconds:.2} s) at {d:.0} units, gain {:.2}, speed {rate:.2}: \"{text}\"", g.data.name(info), voice_gain(d));
        if !settings.play {
            continue;
        }
        let (Some(panned), Some(bytes)) = (panned.as_mut(), bank.read(&path).and_then(|b| crate::sfx::checked(&path, b))) else { continue };
        let source = AudioSource { bytes: bytes.into() };
        let playback = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(settings.volume.max(0.0))).with_speed(rate);
        let handle = panned.add(Panned::new(source, Pan::new(gains)));
        commands.spawn((AudioPlayer::<Panned<AudioSource>>(handle), playback, VoiceSound, SoundCategory::Voice));
    }
}

/// What the other players near this one did: the player's voice answers it.
fn react(time: Res<Time<Real>>, mates: Option<Res<SocialMates>>, player: Option<Res<Player>>, mut script: ResMut<ScriptState>, mut sim: ResMut<SimState>, mut party: ResMut<PartyQuips>) {
    let (Some(mates), Some(player)) = (mates, player) else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.player_voice.is_none() || mates.mates.is_empty() {
        return;
    }
    let now = time.elapsed_secs_f64();
    let eye = player.eye();
    let mut deeds: Vec<(String, Deed)> = Vec::new();
    for m in &mates.mates {
        let (last, aim) = party.seen.get(&m.id).copied().unwrap_or((m.attacks, None));
        let near = m.here && !m.down && !m.lost && (m.feet[0] - eye[0]).hypot(m.feet[1] - eye[1]) < REACT_RANGE;
        if near && m.attacks != last {
            let anim = rt.game.data.weapon_anim_types.get(&FormId(m.weapon)).copied();
            let deed = Deed::of_attack(if m.weapon == 0 { None } else { anim });
            info!("party quip: {} attacks with {} (animation type {anim:?}): {}", m.name, rt.game.data.name(FormId(m.weapon)), deed.map_or("no deed", |d| d.event()));
            if let Some(deed) = deed {
                deeds.push((m.name.clone(), deed));
            }
        }
        let mate_eye = [m.feet[0], m.feet[1], m.feet[2] + MOUTH_HEIGHT];
        let aiming = near && m.sights && aims_at(mate_eye, m.yaw, eye);
        let aim = match (aiming, aim) {
            (true, Some(since)) if now - since >= AIMED_AT_SECONDS => {
                deeds.push((m.name.clone(), Deed::AimedAt));
                Some(f64::INFINITY)
            }
            (true, Some(since)) => Some(since),
            (true, None) => Some(now),
            (false, _) => None,
        };
        party.seen.insert(m.id, (m.attacks, aim));
    }
    for (name, deed) in deeds {
        let said = crate::script::with_world(rt, &mut sim, |rt| {
            let fought = rt.game.player_fought();
            rt.game.player_reacts(deed, fought)
        });
        let g = &rt.game;
        match said {
            Some(info) => info!("party quip: {name} {}: {} says {}: \"{}\"", deed.event(), g.player_voice.as_ref().map_or("?", |v| v.name.as_str()), g.data.name(info), first_text(g, info)),
            None => info!("party quip: {name} {}: no answer (in a fight, too soon, another quip playing, or no line)", deed.event()),
        }
    }
}

/// A line's first response as shown: from its record, or from the set for a carried line.
fn line_text(g: &fnv_game::Game, play: Option<&crate::PlayWorld>, info: FormId) -> String {
    let from_record = play.and_then(|p| crate::script::responses(p, info).into_iter().next());
    from_record.map(|t| crate::dialogue::shown_text(&t)).filter(|t| !t.is_empty()).unwrap_or_else(|| first_text(g, info))
}

fn first_text(g: &fnv_game::Game, info: FormId) -> String {
    g.data.infos.get(&info).and_then(|d| d.responses.first()).map(|r| crate::dialogue::shown_text(&r.text)).unwrap_or_default()
}

/// The player's place and quest comments.
fn comments(mut script: ResMut<ScriptState>, mut sim: ResMut<SimState>, mut party: ResMut<PartyQuips>, mut places: Local<Vec<FormId>>) {
    let Some(rt) = script.rt.as_mut() else { return };
    if rt.game.player_voice.is_none() {
        return;
    }
    crate::script::with_world(rt, &mut sim, |rt| {
        let fought = rt.game.player_fought();
        rt.game.player_fight_lines(fought);
        rt.game.player_comments(fought)
    });
    let g = &rt.game;
    // The comments whose place the player is in, when that changes.
    let mut here: Vec<FormId> = g.quip.at.iter().copied().collect();
    here.sort();
    if here != *places {
        info!("player voice: in the place of {} comment(s): {}", here.len(), here.iter().take(6).map(|&i| g.data.name(i)).collect::<Vec<_>>().join(", "));
        *places = here;
    }
    for (event, info) in g.quip.said.iter().skip(party.logged) {
        if event == "place" || event == "quest" {
            info!("player voice: comment {event} {}: \"{}\"", g.data.name(*info), first_text(g, *info));
        } else if event.starts_with("fight ") {
            info!("player voice: {event} {}: \"{}\"", g.data.name(*info), first_text(g, *info));
        } else if let Some(topic) = event.strip_prefix("companion.") {
            info!("player voice: the voice's companion takes the player's {topic} line: {} \"{}\"", g.data.name(*info), first_text(g, *info));
        }
    }
    party.logged = g.quip.said.len();
    // Every trigger that said nothing, and why (item 64).
    for (event, why) in std::mem::take(&mut rt.game.quip.skipped) {
        info!("player voice: {event}: nothing said: {why}");
    }
}

pub struct PartyQuipsPlugin;

impl Plugin for PartyQuipsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyQuips>().add_systems(Update, (comments, react, send, hear).chain().in_set(PlaySet::Scripts).after(crate::player_voice::Applied));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quip_reads_back() {
        assert_eq!(read_quip(&quip_blob("nexus.willow", 0xFE00_0012)), Some(("nexus.willow".to_string(), 0xFE00_0012)));
        assert_eq!(read_quip(&[1]), None);
        // A player's own line: read as a quip of the set "@own", then its voice type.
        let own = own_line_blob(0x0004_1E64, 0x0001_2345);
        assert_eq!(read_quip(&own), Some((OWN_LINE.to_string(), 0x0004_1E64)));
        assert_eq!(read_own_voice_type(&own), Some(0x0001_2345));
        assert_eq!(read_own_voice_type(&quip_blob("nexus.willow", 1)), None);
    }

    #[test]
    fn sights_on_someone_ahead_only() {
        // Facing north (yaw 0): someone 500 units north is aimed at, someone east is not.
        assert!(aims_at([0.0, 0.0, 0.0], 0.0, [0.0, 500.0, 0.0]));
        assert!(!aims_at([0.0, 0.0, 0.0], 0.0, [500.0, 0.0, 0.0]));
        // Facing east.
        assert!(aims_at([0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, [500.0, 10.0, 0.0]));
        // Too far.
        assert!(!aims_at([0.0, 0.0, 0.0], 0.0, [0.0, 5000.0, 0.0]));
    }
}
