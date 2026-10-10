//! Dialogue voice in the game (docs/design/dialogue.md step D3, docs/spec-notes/voice.md):
//! the lines scripts make actors say (`Game::speech`, today SayTo and Say) are heard, and
//! last as long as their sound.
//!
//! - **Index.** `fnv_voice::VoiceBank` (the lines of the load order over the archives) is
//!   built on its own thread at the start, like the script data. Once both are ready the
//!   bank becomes the runtime's line-length hook (`Game::voice`): a response lasts as
//!   long as its voice file, and keeps the length fitted to its text when it has none.
//!   A line started before that keeps the length it started with.
//! - **Playback.** Each frame the lines being said (`Game::speech`, and the line of a
//!   conversation with the player, `Game::dialogue`) are compared with what is playing:
//!   a response that starts gets its file read from the archive and played through
//!   Bevy's audio (`AudioPlayer`, Ogg Vorbis by the `vorbis` feature; the 40 lines that
//!   also exist as WAV play their Ogg file); a line that stops before its sound is over
//!   (cut by another line, or its speaker gone) has the sound stopped. One log line per
//!   sound started: `voice: playing <path> (<seconds> s) ...`; a response without a file
//!   logs `voice: no file ...`.
//! - **Held with its line.** Whatever holds a line's timer holds its sound
//!   ([`line_held`]): a SayTo or Say line stands still in menu mode (the inventory, a
//!   container, a message box, a conversation with the player), so its sound is paused
//!   there and resumed after; a line of the conversation itself goes on in real time and
//!   is held only by a message box on top of it; the pause menu holds every line. One log
//!   line each way (`voice: … held` / `goes on`).
//! - Not done [decision]: 3D placement (voices play flat, at full volume wherever the
//!   speaker stands), lip sync (`.lip`).
//!
//! Switches: [`VoiceSettings`] (`nvforever --no-voice`, `--voice-volume`);
//! `NVF_PLAY_VOICE_LENGTHS=0` keeps the fitted line lengths (sounds still play).

use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, PlayWorld};
use bevy::audio::Volume;
use bevy::prelude::*;
use fnv_game::game::Speech;
use fnv_game::Game;
use fnv_sim::FormId;
use fnv_voice::VoiceBank;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How dialogue voice behaves; insert before the play plugins to change the defaults.
#[derive(Resource, Clone, Copy, Debug)]
pub struct VoiceSettings {
    /// Play the sounds (`--no-voice` turns it off; line lengths stay).
    pub play: bool,
    /// Linear volume, 1 = as recorded.
    pub volume: f32,
    /// Lines last as long as their sound (off: the fitted length).
    pub lengths: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings { play: true, volume: 1.0, lengths: std::env::var("NVF_PLAY_VOICE_LENGTHS").map_or(true, |v| v != "0") }
    }
}

/// A sound being played for a line.
struct Playing {
    info: FormId,
    response: usize,
    entity: Option<Entity>,
    /// Real time (seconds since start) at which the sound is over (moved on by the time
    /// the sound is held).
    ends_at: f64,
    /// A line of the conversation with the player (else a SayTo or Say line).
    conversation: bool,
    /// The line's timer stands still, and so does the sound ([`line_held`]).
    held: bool,
    /// The line's time left when last seen: a rise means the line was said again.
    remaining: f64,
}

#[derive(Resource, Default)]
pub struct VoiceState {
    loader: Option<Mutex<Receiver<(VoiceBank, f64)>>>,
    asked: bool,
    pub bank: Option<Arc<VoiceBank>>,
    /// By speaker.
    playing: HashMap<FormId, Playing>,
    /// Sounds started so far.
    pub started: u64,
    /// Responses that started without a file.
    pub missing: u64,
}

/// Marks the audio entity of a line.
#[derive(Component)]
pub struct VoiceSound;

/// Which response of a line is being said `elapsed` seconds into it, the responses
/// lasting `lengths` one after another.
pub fn response_at(lengths: &[f64], elapsed: f64) -> usize {
    let mut end = 0.0;
    for (i, &l) in lengths.iter().enumerate() {
        end += l;
        if elapsed < end {
            return i;
        }
    }
    lengths.len().saturating_sub(1)
}

/// The response lengths of a line being said, in record order, with the speaker's base
/// taken from the simulation (between frames the runtime's own world is swapped out,
/// `script::with_world`).
pub fn line_lengths(g: &Game, sim: &SimState, s: &Speech) -> Vec<f64> {
    g.response_lengths_as(sim.base_of(s.speaker.0), s.info)
}

/// Which response (index in record order) of a SayTo or Say line is being said: the
/// responses follow one another in response-number order (`InfoDef::response_order`),
/// each for its length.
pub fn speech_response(g: &Game, sim: &SimState, s: &Speech) -> usize {
    let lengths = line_lengths(g, sim, s);
    let order = g.data.infos.get(&s.info).map(|d| d.response_order()).filter(|o| o.len() == lengths.len());
    let order = order.unwrap_or_else(|| (0..lengths.len()).collect());
    let in_order: Vec<f64> = order.iter().map(|&r| lengths[r]).collect();
    let total: f64 = in_order.iter().sum();
    order.get(response_at(&in_order, total - s.remaining)).copied().unwrap_or(0)
}

/// A response being said this frame.
struct Said {
    speaker: FormId,
    info: FormId,
    /// Index in record order.
    response: usize,
    /// Seconds left (of the line for SayTo and Say, of the response in a conversation):
    /// only its direction is used.
    remaining: f64,
    /// A line of the conversation with the player.
    conversation: bool,
}

/// Every response being said: the SayTo and Say lines, and the conversation's line.
fn said_now(g: &Game, sim: &SimState) -> Vec<Said> {
    let mut out: Vec<Said> =
        g.speech.iter().map(|s| Said { speaker: s.speaker, info: s.info, response: speech_response(g, sim, s), remaining: s.remaining, conversation: false }).collect();
    if let Some((c, line)) = g.dialogue.as_ref().and_then(|c| Some((c, c.line.as_ref()?))) {
        let order = g.data.infos.get(&line.info).map(|d| d.response_order()).unwrap_or_default();
        if let Some(&r) = order.get(line.response) {
            out.push(Said { speaker: c.actor, info: line.info, response: r, remaining: line.remaining, conversation: true });
        }
    }
    out
}

/// What to do about one speaker's sound this frame.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// The response already playing goes on.
    Keep,
    /// Start this response (stopping what the speaker was playing, if it is not over).
    Start(usize),
}

/// Compare a line being said with what its speaker is playing.
pub fn step(playing: Option<(FormId, usize, f64)>, info: FormId, response: usize, remaining: f64) -> Step {
    match playing {
        Some((i, r, before)) if i == info && r == response && remaining <= before + 1e-6 => Step::Keep,
        _ => Step::Start(response),
    }
}

/// Whether a line's timer stands still, in which case its sound is paused and resumed
/// with it. A SayTo or Say line is held by menu mode (`Game::tick` advances it only
/// outside menus: the inventory, a container, a message box, a conversation with the
/// player). A line of the conversation itself goes on in real time by design and is held
/// only by a menu on top of the conversation (a message box). The pause menu holds every
/// line (`script.rs` does not tick under it).
pub fn line_held(conversation: bool, pause_menu: bool, in_menu: bool, menu_on_top: bool) -> bool {
    pause_menu || if conversation { menu_on_top } else { in_menu }
}

/// A sound whose line stopped this long before the sound's end is left to finish (the
/// line's clock and the sound's are not the same clock); earlier, it is stopped.
pub const TAIL_SECONDS: f64 = 0.25;

fn load(mut state: ResMut<VoiceState>, play: Option<Res<PlayWorld>>, assets: Option<Res<nvf_assets::GameData>>) {
    if state.bank.is_some() {
        return;
    }
    if !state.asked {
        let (Some(play), Some(assets)) = (play, assets) else { return };
        state.asked = true;
        let (lo, vfs) = (play.lo, assets.vfs.clone());
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("nvf-voice".into()).spawn(move || {
            let t = Instant::now();
            let bank = VoiceBank::build(lo, vfs);
            let _ = tx.send((bank, t.elapsed().as_secs_f64()));
        });
        match spawned {
            Ok(_) => state.loader = Some(Mutex::new(rx)),
            Err(e) => warn!("voice is off: cannot start the index thread: {e}"),
        }
        return;
    }
    let got = state.loader.as_ref().and_then(|rx| rx.lock().ok().and_then(|r| r.try_recv().ok()));
    if let Some((bank, secs)) = got {
        info!(
            "voice ready in {secs:.2} s: {} lines, {} voice types, {} speakers with a voice type",
            bank.index.lines.len(),
            bank.index.voice_types.len(),
            bank.index.speakers.len()
        );
        state.bank = Some(Arc::new(bank));
        state.loader = None;
    }
}

/// Hand the bank to the runtime as its line-length hook, once both exist.
fn hook(state: Res<VoiceState>, settings: Res<VoiceSettings>, mut script: ResMut<ScriptState>) {
    if !settings.lengths {
        return;
    }
    let Some(bank) = state.bank.as_ref() else { return };
    if !script.rt.as_ref().is_some_and(|rt| rt.game.voice.is_none()) {
        return;
    }
    if let Some(rt) = script.rt.as_mut() {
        rt.game.voice = Some(bank.clone());
        info!("voice: lines last as long as their sound (fitted length without one)");
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<VoiceSettings>,
    mut state: ResMut<VoiceState>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    sounds: Option<ResMut<Assets<AudioSource>>>,
    paused: Res<crate::pause::Paused>,
    sinks: Query<&AudioSink, With<VoiceSound>>,
) {
    let st = &mut *state;
    let (Some(bank), Some(rt)) = (st.bank.clone(), script.rt.as_ref()) else { return };
    let g = &rt.game;
    let now = time.elapsed_secs_f64();
    let mut sounds = sounds;
    let said_now = said_now(g, &sim);
    // Lines no longer said: stop a sound that is not nearly over (the line was cut).
    // A held sound cannot finish by itself: it is stopped as well.
    st.playing.retain(|speaker, p| {
        let said = said_now.iter().any(|s| s.speaker == *speaker && s.info == p.info);
        if !said && (p.held || now < p.ends_at - TAIL_SECONDS) {
            if let Some(e) = p.entity.take() {
                commands.entity(e).try_despawn();
            }
            info!("voice: stopped {} line {} ({:.2} s before its end)", g.data.name(*speaker), g.data.name(p.info), p.ends_at - now);
        }
        said
    });
    // Whatever holds a line's timer holds its sound: the sink is paused, the sound's end
    // moves on by the time held, and both go on together.
    for (speaker, p) in st.playing.iter_mut() {
        let hold = line_held(p.conversation, paused.0, g.in_menu(), g.menu.is_some());
        if hold {
            p.ends_at += time.delta_secs_f64();
        }
        if hold != p.held {
            p.held = hold;
            info!("voice: {} line {} {}", g.data.name(*speaker), g.data.name(p.info), if hold { "held (its timer stands still)" } else { "goes on" });
        }
        // The sink appears a frame or two after the sound was spawned.
        if let Some(sink) = p.entity.and_then(|e| sinks.get(e).ok()) {
            if hold && !sink.is_paused() {
                sink.pause();
            } else if !hold && sink.is_paused() {
                sink.play();
            }
        }
    }
    for s in &said_now {
        let k = s.response;
        let was = st.playing.get(&s.speaker).map(|p| (p.info, p.response, p.remaining));
        if step(was, s.info, k, s.remaining) == Step::Keep {
            if let Some(p) = st.playing.get_mut(&s.speaker) {
                p.remaining = s.remaining;
            }
            continue;
        }
        if let Some(old) = st.playing.remove(&s.speaker) {
            if old.held || now < old.ends_at - TAIL_SECONDS {
                if let Some(e) = old.entity {
                    commands.entity(e).try_despawn();
                }
            }
        }
        let base = sim.base_of(s.speaker.0);
        let fitted = g.response_seconds_as(base, s.info, k);
        let who = g.data.name(s.speaker);
        let number = bank.index.lines.get(&s.info.0).and_then(|l| l.responses.get(k)).map_or(0, |r| r.number);
        let file = base.and_then(|b| bank.file(s.info.0, k, b.0));
        let (entity, seconds) = match &file {
            Some(path) => {
                let seconds = bank.seconds(path).unwrap_or(fitted);
                let entity = match (settings.play, sounds.as_mut(), bank.read(path)) {
                    (true, Some(assets), Some(bytes)) => {
                        let source = assets.add(AudioSource { bytes: bytes.into() });
                        let volume = Volume::Linear(settings.volume.max(0.0));
                        Some(commands.spawn((AudioPlayer::new(source), PlaybackSettings::DESPAWN.with_volume(volume), VoiceSound)).id())
                    }
                    _ => None,
                };
                st.started += 1;
                info!(
                    "voice: playing {path} ({seconds:.2} s) {who} line {} response {number}{}",
                    g.data.name(s.info),
                    if entity.is_some() { "" } else { " [silent: voice playback is off]" }
                );
                (entity, seconds)
            }
            None => {
                st.missing += 1;
                let voice = base.and_then(|b| bank.index.voice_of(b.0)).and_then(|v| bank.index.voice_types.get(&v));
                info!("voice: no file for {who} line {} response {number} (voice type {})", g.data.name(s.info), voice.map_or("none", |v| v.as_str()));
                (None, fitted)
            }
        };
        st.playing.insert(s.speaker, Playing { info: s.info, response: k, entity, ends_at: now + seconds, remaining: s.remaining, conversation: s.conversation, held: false });
    }
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoiceSettings>().init_resource::<VoiceState>().add_systems(Update, (load, hook, play).chain().in_set(PlaySet::Ui));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_follow_one_another() {
        let l = [2.0, 3.0, 1.5];
        assert_eq!(response_at(&l, 0.0), 0);
        assert_eq!(response_at(&l, 1.99), 0);
        assert_eq!(response_at(&l, 2.0), 1);
        assert_eq!(response_at(&l, 4.99), 1);
        assert_eq!(response_at(&l, 5.0), 2);
        // Past the end (the last frame of a line): the last response.
        assert_eq!(response_at(&l, 99.0), 2);
        assert_eq!(response_at(&[], 1.0), 0);
    }

    #[test]
    fn a_sound_is_held_with_its_lines_timer() {
        // (conversation, pause menu, menu mode, a menu on top of the conversation)
        // A SayTo line: free in the world, held by any menu and by a conversation.
        assert!(!line_held(false, false, false, false));
        assert!(line_held(false, false, true, false));
        assert!(line_held(false, false, true, true));
        // The conversation's own line: menu mode is the conversation itself, so it plays;
        // a message box on top holds it.
        assert!(!line_held(true, false, true, false));
        assert!(line_held(true, false, true, true));
        // The pause menu holds everything.
        assert!(line_held(true, true, true, false));
        assert!(line_held(false, true, false, false));
    }

    #[test]
    fn a_sound_starts_once_per_response() {
        let (a, b) = (FormId(0x10), FormId(0x11));
        // Nothing playing: start.
        assert_eq!(step(None, a, 0, 4.0), Step::Start(0));
        // The same response, time running down: keep.
        assert_eq!(step(Some((a, 0, 4.0)), a, 0, 3.9), Step::Keep);
        // A menu holds script time: still the same sound.
        assert_eq!(step(Some((a, 0, 3.9)), a, 0, 3.9), Step::Keep);
        // The line's next response.
        assert_eq!(step(Some((a, 0, 2.1)), a, 1, 1.9), Step::Start(1));
        // Another line by the same speaker.
        assert_eq!(step(Some((a, 1, 1.9)), b, 0, 5.0), Step::Start(0));
        // The same line said again: its time left went up.
        assert_eq!(step(Some((a, 0, 0.4)), a, 0, 4.0), Step::Start(0));
    }
}
