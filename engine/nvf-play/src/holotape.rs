//! Holotapes in the game (docs/spec-notes/holotapes.md): a Sound or Voice note picked in
//! the Pip-Boy's Notes list (DATA page, Misc tab) plays.
//!
//! - **One at a time.** Picking a note that plays ([`Holotape::picks`], from the DATA
//!   page's use of the row: a click, Enter, or the test aid's `key=use` / `note=`)
//!   starts it and stops the one playing; picking the one playing stops it
//!   [hypothesis: holotapes.md §3].
//! - **A Voice note** plays its line (`fnv_game::holotapes::tape`) in its speaker's voice:
//!   each response's file in turn (`fnv_voice::VoiceBank::file` with the speaker's base,
//!   the voice type's folder), one after another, at full volume and not placed, in the
//!   Voice category. It goes on while the Pip-Boy is put away [hypothesis] and is held
//!   by the pause menu. A response without a file is skipped (logged).
//! - **A Sound note** plays its sound once at the listener (`crate::sfx::PlaySound`); it
//!   cannot be stopped [hypothesis: no Sound note of the masters names a sound,
//!   holotapes.md §2].
//!
//! Log lines: `holotape: <note> plays <line> said by <speaker>, <n> responses`, then
//! `holotape: playing <path> (<seconds> s) response <k>/<n>` per response, `holotape:
//! stopped <note>` and `holotape: <note> is over`.

use crate::audio_out::SoundCategory;
use crate::script::ScriptState;
use crate::sfx::PlaySound;
use crate::voice::{VoiceSettings, VoiceState};
use crate::PlaySet;
use bevy::audio::Volume;
use bevy::prelude::*;
use fnv_game::holotapes::{tape, Tape};
use fnv_sim::FormId;

/// The holotape that plays, and the picks waiting to be taken.
#[derive(Resource, Default)]
pub struct Holotape {
    /// Notes picked in the Notes list (Sound and Voice notes), taken next frame.
    pub picks: Vec<FormId>,
    /// The note playing.
    pub playing: Option<FormId>,
    /// The responses still to play: path and seconds.
    queue: Vec<(String, f64)>,
    count: usize,
    entity: Option<Entity>,
    ends_at: f64,
    held: bool,
    /// Responses started so far.
    pub started: u64,
}

/// Marks a holotape's audio entity.
#[derive(Component)]
pub struct HolotapeSound;

impl Holotape {
    fn stop(&mut self, commands: &mut Commands) {
        if let Some(e) = self.entity.take() {
            commands.entity(e).try_despawn();
        }
        self.playing = None;
        self.queue.clear();
        self.held = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<VoiceSettings>,
    voice: Res<VoiceState>,
    mut tape_state: ResMut<Holotape>,
    script: Res<ScriptState>,
    paused: Res<crate::pause::Paused>,
    mut sounds: Option<ResMut<Assets<AudioSource>>>,
    sinks: Query<&AudioSink, With<HolotapeSound>>,
    mut effects: MessageWriter<PlaySound>,
) {
    let st = &mut *tape_state;
    let Some(rt) = script.rt.as_ref() else { return };
    let g = &rt.game;
    let now = time.elapsed_secs_f64();
    let picks = std::mem::take(&mut st.picks);
    for (i, note) in picks.iter().enumerate() {
        if st.playing == Some(*note) {
            st.stop(&mut commands);
            info!("holotape: stopped {}", g.data.name(*note));
            continue;
        }
        match tape(g.data, *note) {
            Some(Tape::Sound(sound)) => {
                if st.playing.is_some() {
                    st.stop(&mut commands);
                }
                effects.write(PlaySound { sound, at: None });
                info!("holotape: {} plays its sound {}", g.data.name(*note), g.data.name(sound));
            }
            Some(Tape::Voice { speaker, info }) => {
                // The voice index is built on its own thread at the start: the pick waits.
                let Some(bank) = voice.bank.as_ref() else {
                    st.picks.extend_from_slice(&picks[i..]);
                    break;
                };
                st.stop(&mut commands);
                let n = g.data.infos.get(&info).map_or(0, |d| d.responses.len());
                st.queue = (0..n)
                    .filter_map(|k| match bank.file(info.0, k, speaker.0) {
                        Some(path) => {
                            let seconds = bank.seconds(&path).unwrap_or(0.0);
                            Some((path, seconds))
                        }
                        None => {
                            info!("holotape: no file for {} line {} response {}", g.data.name(speaker), g.data.name(info), k + 1);
                            None
                        }
                    })
                    .collect();
                st.queue.reverse();
                st.count = st.queue.len();
                st.playing = Some(*note);
                st.ends_at = now;
                info!("holotape: {} plays {} said by {}, {} responses", g.data.name(*note), g.data.name(info), g.data.name(speaker), n);
            }
            None => info!("holotape: {} has nothing to play", g.data.name(*note)),
        }
    }
    let Some(note) = st.playing else { return };
    // The pause menu holds it: the sound's end moves on by the time held.
    if paused.0 {
        st.ends_at += time.delta_secs_f64();
    }
    if paused.0 != st.held {
        st.held = paused.0;
        if let Some(sink) = st.entity.and_then(|e| sinks.get(e).ok()) {
            if st.held {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }
    if st.held || now < st.ends_at {
        return;
    }
    st.entity = None;
    let Some((path, seconds)) = st.queue.pop() else {
        st.playing = None;
        info!("holotape: {} is over", g.data.name(note));
        return;
    };
    let k = st.count - st.queue.len();
    let bytes = voice.bank.as_ref().and_then(|b| b.read(&path));
    st.entity = match (settings.play, sounds.as_mut(), bytes) {
        (true, Some(assets), Some(bytes)) => {
            let source = AudioSource { bytes: bytes.into() };
            let playback = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(settings.volume.max(0.0)));
            Some(commands.spawn((AudioPlayer::new(assets.add(source)), playback, HolotapeSound, SoundCategory::Voice)).id())
        }
        _ => None,
    };
    st.started += 1;
    st.ends_at = now + seconds;
    let silent = if st.entity.is_none() { " [silent: voice playback is off]" } else { "" };
    info!("holotape: playing {path} ({seconds:.2} s) response {k}/{}{silent}", st.count);
}

pub struct HolotapePlugin;

impl Plugin for HolotapePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Holotape>().add_systems(Update, play.in_set(PlaySet::Ui));
    }
}
