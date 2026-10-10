//! How long a response of a dialogue line lasts (docs/design/dialogue.md step D3,
//! docs/spec-notes/voice.md §8): as long as its voice file when the game was given a
//! way to ask ([`Game::voice`], crate `fnv-voice`), else the length fitted to its text
//! ([`crate::game::fitted_seconds`], docs/spec-notes/scripts.md §7.7). Every response is
//! timed through `Game::response_seconds` (`crate::dialogue`), which comes here.

use crate::game::{fitted_seconds, Game};
use fnv_sim::FormId;

/// The lengths of voice files. Implemented outside this crate (`fnv_voice::VoiceBank`),
/// so the rules stay free of archives and sound formats.
pub trait VoiceLengths: Send + Sync {
    /// Seconds of the sound of response `response` (index in record order, as in
    /// `InfoDef::response_chars`) of line `info`, said by an actor or talking activator
    /// whose base form is `speaker_base`. `None`: no sound is known for it.
    fn response_seconds(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<f64>;
}

impl Game<'_> {
    /// [`Game::response_seconds`] for a speaker given by its base form (for callers that
    /// hold the world themselves, as the engine does between frames): the length of the
    /// response's voice file [hypothesis: the game ends a line when its sound ends; the
    /// intro's stage timings compare both lengths with the capture, voice.md §8], else
    /// the length fitted to its text.
    pub fn response_seconds_as(&self, speaker_base: Option<FormId>, info: FormId, response: usize) -> f64 {
        let sound = self.voice.as_ref().and_then(|v| v.response_seconds(info, response, speaker_base));
        sound.unwrap_or_else(|| {
            let chars = self.data.infos.get(&info).and_then(|d| d.response_chars.get(response)).copied().unwrap_or(0);
            fitted_seconds(chars)
        })
    }

    /// The length of each response of a line, in record order. A line without responses
    /// lasts as one empty response does ([`Game::line_seconds`] is their sum).
    pub fn response_lengths(&self, speaker: FormId, info: FormId) -> Vec<f64> {
        self.response_lengths_as(crate::functions::base_of(self, speaker), info)
    }

    /// [`Game::response_lengths`] for a speaker given by its base form.
    pub fn response_lengths_as(&self, speaker_base: Option<FormId>, info: FormId) -> Vec<f64> {
        let n = self.data.infos.get(&info).map_or(0, |d| d.response_chars.len());
        (0..n.max(1)).map(|k| self.response_seconds_as(speaker_base, info, k)).collect()
    }
}
