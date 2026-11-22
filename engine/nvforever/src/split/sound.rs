//! Split screen's sound between the two games (the owner's list of 2026-11-21: "player 2
//! should get sounds when they aren't in the same area as the host ... lowering one
//! player's sound when the other is in dialogue").
//!
//! - **Together** (player 2 in player 1's place): player 1's game plays the world for
//!   both, as loud as the nearer player hears it (`nvf_play::sfx::SecondListener`), and
//!   player 2's game plays only its interface and the voice of its own conversation
//!   (`nvf_play::audio_out::SplitSound`; a conversation runs in that player's own game,
//!   `crate::coop` "What the host's NPCs say", so no other game plays its voice).
//! - **Apart** (player 1's game has no ear for player 2 in its place): player 2's game
//!   plays its own world too. Which one it is follows player 1's game ([`P1_FILE`]
//!   `heard`), so a sound is never heard from both games, nor from neither.
//! - **A conversation**: while one player talks and the other doesn't, the other's game
//!   plays at [`DUCK`] (`nvf_play::audio_out::SoundVolumes::set_duck`, eased as a volume
//!   change is), so the conversation is heard over the other half.
//!
//! The two games tell each other in two small files beside `p1.txt` ([`P1_FILE`],
//! [`P2_FILE`]), every [`EVERY`] seconds.

use super::{kv, write_file, P1, P2};
use bevy::prelude::*;
use nvf_play::audio_out::{SoundVolumes, SplitSound};
use nvf_play::sfx::SecondListener;
use nvf_play::PlayState;

/// Player 1's sound file: `heard=1` while it plays the world for player 2 (its ear is in
/// player 1's place), `talk=1` while player 1 is in a conversation.
pub const P1_FILE: &str = "p1-sound.txt";
/// Player 2's: `talk=1` while player 2 is in a conversation.
pub const P2_FILE: &str = "p2-sound.txt";

/// How often the files are written and read, seconds [design: a quarter second, a
/// conversation's start is ducked within it].
pub const EVERY: f32 = 0.25;

/// The level the other half's sound is played at during a conversation: the original
/// lowers its radio so during one, `fRadioDialogMute=0.50` [observed: the install's
/// `Fallout_default.ini`, docs/spec-notes/radio.md §10; what it does is hypothesis]; here
/// the whole other game [design].
pub const DUCK: f32 = 0.5;

/// The duck this game takes: [`DUCK`] while the other player talks and this one doesn't.
pub fn duck_for(me_talking: bool, other_talking: bool) -> f32 {
    if other_talking && !me_talking {
        DUCK
    } else {
        1.0
    }
}

fn flag(text: &str, key: &str) -> bool {
    kv(text, key).as_deref() == Some("1")
}

fn talking(state: Option<&State<PlayState>>) -> bool {
    state.is_some_and(|s| *s.get() == PlayState::Dialogue)
}

/// Player 1's game (and a game that was, after split screen ends: back to full volume).
pub(super) fn p1_sound(
    time: Res<Time<Real>>,
    p1: Option<Res<P1>>,
    second: Option<Res<SecondListener>>,
    state: Option<Res<State<PlayState>>>,
    volumes: Option<Res<SoundVolumes>>,
    mut next: Local<f32>,
    mut was: Local<Option<(bool, f32)>>,
) {
    let Some(volumes) = volumes else { return };
    let Some(p1) = p1 else {
        if was.take().is_some_and(|(_, d)| d != 1.0) {
            volumes.set_duck(1.0);
            info!("split: sound: this game at full volume again (split screen ended)");
        }
        return;
    };
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + EVERY;
    let heard = SecondListener::ear(second.as_deref()).is_some();
    let me = talking(state.as_deref());
    write_file(&p1.dir.join(P1_FILE), &format!("heard={}\ntalk={}\n", u8::from(heard), u8::from(me)));
    let theirs = std::fs::read_to_string(p1.dir.join(P2_FILE)).unwrap_or_default();
    let duck = duck_for(me, flag(&theirs, "talk"));
    if volumes.duck() != duck {
        volumes.set_duck(duck);
    }
    if *was != Some((heard, duck)) {
        info!(
            "split: sound: {}; {}",
            if heard { "this game plays the world for both players (player 2 is here)" } else { "player 2 is elsewhere: each game plays its own world" },
            if duck < 1.0 { format!("player 2 talks: this game at {duck:.2}") } else { "full volume".to_string() }
        );
        *was = Some((heard, duck));
    }
}

/// Player 2's game.
pub(super) fn p2_sound(
    time: Res<Time<Real>>,
    p2: Res<P2>,
    state: Option<Res<State<PlayState>>>,
    volumes: Option<Res<SoundVolumes>>,
    mut split: ResMut<SplitSound>,
    mut next: Local<f32>,
    mut was: Local<Option<(bool, f32)>>,
) {
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + EVERY;
    let me = talking(state.as_deref());
    write_file(&p2.dir.join(P2_FILE), &format!("talk={}\n", u8::from(me)));
    // No file yet (player 1's game is starting again for a load): as it was.
    let Ok(theirs) = std::fs::read_to_string(p2.dir.join(P1_FILE)) else { return };
    let own_world = !flag(&theirs, "heard");
    if split.own_world != own_world {
        split.own_world = own_world;
    }
    let duck = duck_for(me, flag(&theirs, "talk"));
    if let Some(v) = volumes.as_deref().filter(|v| v.duck() != duck) {
        v.set_duck(duck);
    }
    if *was != Some((own_world, duck)) {
        info!(
            "split: sound: {}; {}",
            if own_world { "player 1's game doesn't hear for this player here: this game plays its own world" } else { "player 1's game plays the world for both: this game plays its interface and its own conversation" },
            if duck < 1.0 { format!("player 1 talks: this game at {duck:.2}") } else { "full volume".to_string() }
        );
        *was = Some((own_world, duck));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_one_not_talking_is_lowered() {
        assert_eq!(duck_for(false, true), DUCK);
        assert_eq!(duck_for(true, false), 1.0);
        assert_eq!(duck_for(true, true), 1.0, "both in a conversation: neither lowered");
        assert_eq!(duck_for(false, false), 1.0);
    }
}
