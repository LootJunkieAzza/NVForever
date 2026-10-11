//! Sounds scripts ask for: `PlaySound` and `PlaySound3D` (docs/spec-notes/audio.md §10).
//!
//! The rules only record what is to be heard ([`SoundRequest`], taken from
//! [`Game::sound_requests`] by whoever plays sounds) and trace it as `sound_asked`.
//! Playing it (the SOUN's file, its loudness) is the engine's work. In a headless run
//! nothing takes them: at most [`MAX_PENDING`] are kept. They are not part of the saved
//! state.

use crate::functions::{none, Args, Ctx};
use crate::game::Game;
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use serde::Serialize;

/// Requests kept at most; older ones are dropped when nothing takes them (a headless
/// run) [design].
pub const MAX_PENDING: usize = 256;

/// A sound a script asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SoundRequest {
    /// The SOUN.
    pub sound: FormId,
    /// `PlaySound3D`: the calling reference, where the sound is [documented: GECK wiki
    /// PlaySound3D]. `None`: `PlaySound`, not placed ("non-locational") [documented:
    /// GECK wiki PlaySound].
    pub at: Option<FormId>,
    /// `PlaySound`'s SystemSound flag: it plays in menu mode too; without it the sound
    /// may wait for game mode [documented: GECK wiki PlaySound].
    pub system: bool,
}

impl Game<'_> {
    /// Queue a sound for the engine and trace it.
    pub fn ask_sound(&mut self, r: SoundRequest) {
        self.emit(EventKind::SoundAsked { sound: r.sound, at: r.at, system: r.system });
        if self.sound_requests.len() >= MAX_PENDING {
            self.sound_requests.remove(0);
        }
        self.sound_requests.push(r);
    }
}

/// `PlaySound sound [SystemSound]`: play a SOUN, not placed [documented: GECK wiki
/// PlaySound, owner export].
pub fn play_sound(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let sound = a.form(0)?;
    let system = a.num_or(1, 0.0) != 0.0;
    g.ask_sound(SoundRequest { sound, at: None, system });
    none()
}

/// `ref.PlaySound3D sound`: play a SOUN at the calling reference [documented: GECK wiki
/// PlaySound3D, owner export].
pub fn play_sound_3d(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let sound = a.form(0)?;
    let at = ctx.subject()?;
    g.ask_sound(SoundRequest { sound, at: Some(at), system: false });
    none()
}
