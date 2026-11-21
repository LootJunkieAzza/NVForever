//! The voice files of an install: the index of lines over the archives and loose files
//! (`fnv_vfs::Vfs`), with the lengths read so far.

use crate::index::VoiceIndex;
use crate::length::{sound_info, SoundInfo};
use fnv_esm::LoadOrder;
use fnv_sim::FormId;
use fnv_vfs::Vfs;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The longest a voice file may last before its length is taken for a broken header's.
/// Radio stations play their songs as dialogue lines [observed: radio.rs], so this is above
/// any song's length, and far below the hours a garbled header can claim.
pub const MAX_LINE_SECONDS: f64 = 20.0 * 60.0;

pub struct VoiceBank {
    pub index: VoiceIndex,
    vfs: Arc<Vfs>,
    /// Path → what its headers said (`None`: unreadable).
    lengths: Mutex<HashMap<String, Option<SoundInfo>>>,
    /// The files [`VoiceBank::info`] read lately, kept for their playback
    /// ([`VoiceBank::read`] takes them), so a line's file is read once, not twice (#1993).
    recent: Mutex<Vec<(String, Vec<u8>)>>,
}

/// How many files [`VoiceBank::info`] keeps for playback: a conversation's next lines.
const RECENT: usize = 4;

impl VoiceBank {
    pub fn new(index: VoiceIndex, vfs: Arc<Vfs>) -> VoiceBank {
        VoiceBank { index, vfs, lengths: Mutex::new(HashMap::new()), recent: Mutex::new(Vec::new()) }
    }

    pub fn build(lo: &LoadOrder, vfs: Arc<Vfs>) -> VoiceBank {
        VoiceBank::new(VoiceIndex::build(lo), vfs)
    }

    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// The file of a response (index in record order) for a voice type, if there is one.
    pub fn file_for_voice(&self, info: u32, response: usize, voice_type: u32) -> Option<String> {
        self.index.paths(info, response, voice_type).into_iter().find(|p| self.vfs.exists(p))
    }

    /// The file of a response as said by an actor base or talking activator: the one in
    /// its voice type's folder. No other voice type's file stands in [hypothesis: 15
    /// responses of the masters have a fixed speaker whose voice type has no file while
    /// another's has; whether the game plays that one is not known, voice.md §3].
    pub fn file(&self, info: u32, response: usize, speaker_base: u32) -> Option<String> {
        self.file_for_voice(info, response, self.index.voice_of(speaker_base)?)
    }

    /// The file of a recording's response (a Voice note's, a holotape): as said by its
    /// speaker ([`VoiceBank::file`]), else by the line's fixed speaker (ANAM) when its
    /// voice type has the file. Dead Money's Dog Command Tape names Dog as the note's
    /// speaker while its line is God's (ANAM NVDLC01God), recorded in God's voice and filed
    /// in his folder only [observed: DeadMoney - Main.bsa, #3022; hypothesis: the original
    /// plays it].
    pub fn recording_file(&self, info: u32, response: usize, speaker_base: u32) -> Option<String> {
        self.file(info, response, speaker_base).or_else(|| {
            let fixed = self.index.lines.get(&info)?.speaker.filter(|&s| s != speaker_base)?;
            self.file(info, response, fixed)
        })
    }

    /// The decoded bytes of a file (for playback): those [`VoiceBank::info`] just read
    /// for its length when it did, else read now.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut recent = self.recent.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(i) = recent.iter().position(|(p, _)| p == path) {
            return Some(recent.remove(i).1);
        }
        drop(recent);
        self.vfs.read(path)?.ok()
    }

    /// What a file's headers say; read once. The bytes are kept for the line's playback.
    pub fn info(&self, path: &str) -> Option<SoundInfo> {
        if let Some(known) = self.lengths.lock().unwrap_or_else(|p| p.into_inner()).get(path) {
            return *known;
        }
        let bytes = self.vfs.read(path).and_then(|r| r.ok());
        let info = bytes.as_deref().and_then(|b| sound_info(b).ok());
        self.lengths.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_string(), info);
        if let (Some(b), Some(_)) = (bytes, info) {
            let mut recent = self.recent.lock().unwrap_or_else(|p| p.into_inner());
            if recent.len() >= RECENT {
                recent.remove(0);
            }
            recent.push((path.to_string(), b));
        }
        info
    }

    /// The sound's length; `None` without one or when its header's length cannot be a
    /// voice line's (not a positive number, or past [`MAX_LINE_SECONDS`]: a broken file
    /// that would hold a conversation or quest for hours).
    pub fn seconds(&self, path: &str) -> Option<f64> {
        self.info(path).map(|i| i.seconds).filter(|s| s.is_finite() && *s > 0.0 && *s <= MAX_LINE_SECONDS)
    }

    /// The face animation beside a sound (`<name>.lip`), decoded; `None` when there is
    /// no such file or it does not decode (docs/spec-notes/lipsync.md).
    pub fn lip(&self, sound_path: &str) -> Option<crate::lip::Lip> {
        crate::lip::Lip::parse(&self.read(&crate::lip::lip_path(sound_path))?).ok()
    }
}

/// The game's hook (`fnv_game::voice`): a response lasts as long as its sound.
impl fnv_game::voice::VoiceLengths for VoiceBank {
    fn response_seconds(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<f64> {
        let path = self.file(info.0, response, speaker_base?.0)?;
        self.seconds(&path).filter(|s| s.is_finite() && *s > 0.0)
    }

    fn response_path(&self, info: FormId, response: usize, speaker_base: Option<FormId>) -> Option<String> {
        self.file(info.0, response, speaker_base?.0)
    }

    fn response_seconds_in_voice(&self, info: FormId, response: usize, voice_type: FormId) -> Option<f64> {
        let path = self.file_for_voice(info.0, response, voice_type.0)?;
        self.seconds(&path).filter(|s| s.is_finite() && *s > 0.0)
    }

    fn response_path_in_voice(&self, info: FormId, response: usize, voice_type: FormId) -> Option<String> {
        self.file_for_voice(info.0, response, voice_type.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1993: the file a line's length was read from is handed to its playback, not read
    /// again; a file read for playback alone is read then.
    #[test]
    fn playback_takes_the_bytes_the_length_was_read_from() {
        let dir = std::env::temp_dir().join(format!("fnv-voice-bank-{}", std::process::id()));
        let file = dir.join("sound").join("voice").join("a.wav");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let wav = crate::length::synth::wav_pcm(1, 22050, 16, 22050);
        std::fs::write(&file, &wav).unwrap();
        for k in 0..RECENT + 2 {
            std::fs::write(dir.join("sound").join("voice").join(format!("b{k}.wav")), &wav).unwrap();
        }
        let bank = VoiceBank::new(VoiceIndex::default(), Arc::new(Vfs::open(&dir, &[]).unwrap()));
        let path = "sound\\voice\\a.wav";
        assert!((bank.seconds(path).unwrap() - 1.0).abs() < 1e-9);
        // Gone from the disk: playback still gets the bytes the length came from, once.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(bank.read(path).as_deref(), Some(&wav[..]));
        assert!(bank.read(path).is_none(), "taken: the next read goes to the files");
        // Its length is still known without the file.
        assert!(bank.seconds(path).is_some());
        // Only the last few are kept.
        for k in 0..RECENT + 2 {
            bank.info(&format!("sound\\voice\\b{k}.wav"));
        }
        assert_eq!(bank.recent.lock().unwrap().len(), RECENT);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// #3022: a recording whose speaker's voice type has no file for a response plays the
    /// one in the line's fixed speaker's (ANAM) folder; the speaker's own file comes first;
    /// a line without a fixed speaker finds nothing else.
    #[test]
    fn a_recording_falls_back_to_the_line_s_fixed_speaker() {
        use crate::index::{Line, Response};
        const DOG: u32 = 0x0100_1301;
        const GOD: u32 = 0x0100_E413;
        const DOG_VOICE: u32 = 0x0100_3572;
        const GOD_VOICE: u32 = 0x0100_E415;
        const INFO: u32 = 0x0100_DFC7;
        const PLAIN: u32 = 0x0100_DFC8;
        let mut ix = VoiceIndex { plugins: vec!["FalloutNV.esm".into(), "DeadMoney.esm".into()], ..VoiceIndex::default() };
        ix.voice_types.insert(DOG_VOICE, "DogVoice".into());
        ix.voice_types.insert(GOD_VOICE, "GodVoice".into());
        ix.speakers.insert(DOG, DOG_VOICE);
        ix.speakers.insert(GOD, GOD_VOICE);
        ix.edids.insert(0x0100_1304, "DogQuest".into());
        ix.edids.insert(0x0100_DFBB, "DogTopic".into());
        let response = Response { number: 1, sound: None, chars: 10, letters: 0 };
        let line = |speaker| Line { quest: Some(0x0100_1304), topic: Some(0x0100_DFBB), speaker, responses: vec![response.clone()] };
        ix.lines.insert(INFO, line(Some(GOD)));
        ix.lines.insert(PLAIN, line(None));
        let dir = std::env::temp_dir().join(format!("fnv-voice-fixed-{}", std::process::id()));
        let god_file = crate::path::voice_path("DeadMoney.esm", "GodVoice", "DogQuest", "DogTopic", INFO, 1, "ogg");
        let write = |path: &str| {
            let f = dir.join(path.replace('\\', "/"));
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, b"OggS").unwrap();
        };
        write(&god_file);
        write(&crate::path::voice_path("DeadMoney.esm", "GodVoice", "DogQuest", "DogTopic", PLAIN, 1, "ogg"));
        let bank = VoiceBank::new(ix, Arc::new(Vfs::open(&dir, &[]).unwrap()));
        assert_eq!(bank.file(INFO, 0, DOG), None, "Dog's folder has nothing");
        assert_eq!(bank.recording_file(INFO, 0, DOG), Some(god_file.clone()), "the fixed speaker's file");
        assert_eq!(bank.recording_file(PLAIN, 0, DOG), None, "no fixed speaker: nothing else");
        // Dog's own file, once there, comes first.
        let dog_file = crate::path::voice_path("DeadMoney.esm", "DogVoice", "DogQuest", "DogTopic", INFO, 1, "ogg");
        write(&dog_file);
        let bank = VoiceBank::new(bank.index.clone(), Arc::new(Vfs::open(&dir, &[]).unwrap()));
        assert_eq!(bank.recording_file(INFO, 0, DOG), Some(dog_file));
        std::fs::remove_dir_all(&dir).ok();
    }
}
