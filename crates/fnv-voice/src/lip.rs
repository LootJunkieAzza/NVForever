//! `.lip` files: the face animation of one voiced response (docs/spec-notes/lipsync.md).
//!
//! The format has no official documentation; everything below is [observed] on the
//! 60,702 `.lip` files of the owner's install unless tagged otherwise (lipsync.md §2
//! says how each point was established).
//!
//! - **Header**, 12 bytes, little-endian: a u32 that is always 1, a u32 that is always
//!   the size of the unpacked body plus 16, a u32 that is always 1.
//! - **Body**, packed: a byte other than 0 stands for itself; a 0 byte is followed by a
//!   u16 count and stands for that many 0 bytes. Packing every run of zeros this way
//!   (longest run first) reproduces all files byte for byte.
//! - **Unpacked body**: a u32 frame count, an i32 start frame, then per frame 33 f32
//!   values: one weight per channel ([`CHANNEL_NAMES`]).
//! - **Time**: 30 frames per second; frame `f` belongs to the moment `(f + start
//!   frame) / 30` seconds after the sound starts (the start frame is negative when the
//!   animation begins before the sound).
//!
//! Which face movement a channel drives is [observed] for the consonant channels (against
//! the spoken text) and for the blink pair, and [hypothesis: by the documented order of
//! the game's phoneme and modifier numbers] for the rest (lipsync.md §3).


/// Values per frame.
pub const CHANNELS: usize = 33;

/// The first [`PHONEMES`] channels are mouth shapes.
pub const PHONEMES: usize = 16;

/// Frames per second [observed: lipsync.md §2.4].
pub const FRAMES_PER_SECOND: f64 = 30.0;

/// Bytes before the packed body.
pub const HEADER_BYTES: usize = 12;

/// What the header's second field exceeds the unpacked body's size by.
pub const DECLARED_EXTRA: u32 = 16;

/// Channel names: the game's phoneme numbers 0–15 followed by its modifier numbers 0–16
/// [documented: GECK wiki, "ModifyFaceGen" (the console command's Phoneme and Modifier
/// IDs); the spelling follows the morph labels of the head `.tri` files where the two
/// differ (`Ee`)]. That a `.lip` channel is the phoneme or modifier of the same number
/// is [observed] for channels 2, 3, 4, 7, 9, 10, 13, 14, 15, 16 and 17 and [hypothesis]
/// for the others (lipsync.md §3).
pub const CHANNEL_NAMES: [&str; CHANNELS] = [
    "Aah",
    "BigAah",
    "BMP",
    "ChJSh",
    "DST",
    "Ee",
    "Eh",
    "FV",
    "I",
    "K",
    "N",
    "Oh",
    "OohQ",
    "R",
    "Th",
    "W",
    "BlinkLeft",
    "BlinkRight",
    "BrowDownLeft",
    "BrowDownRight",
    "BrowInLeft",
    "BrowInRight",
    "BrowUpLeft",
    "BrowUpRight",
    "LookDown",
    "LookLeft",
    "LookRight",
    "LookUp",
    "SquintLeft",
    "SquintRight",
    "HeadPitch",
    "HeadRoll",
    "HeadYaw",
];

/// Index of a channel by name.
pub fn channel(name: &str) -> Option<usize> {
    CHANNEL_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LipError {
    #[error("file too short: {have} bytes, a header needs {HEADER_BYTES}")]
    NoHeader { have: usize },
    #[error("a zero-run marker at byte {at} has no count")]
    CutRun { at: usize },
    #[error("the header declares {declared} bytes (unpacked + {DECLARED_EXTRA}), the body unpacks to {unpacked}")]
    Size { declared: u32, unpacked: usize },
    #[error("the unpacked body is {have} bytes, too short for the frame count and start frame")]
    NoCounts { have: usize },
    #[error("{frames} frames need {need} bytes, the unpacked body has {have}")]
    Frames { frames: u32, need: usize, have: usize },
}

/// One decoded `.lip` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Lip {
    /// The header's first field [observed: always 1; hypothesis: a format version].
    pub version: u32,
    /// The header's third field [observed: always 1; meaning unknown].
    pub flag: u32,
    /// The index of the first frame on the sound's clock: frame `f` is at
    /// `(f + start_frame) / 30` seconds [observed: −36 to 6, negative or zero in all
    /// but 35 files].
    pub start_frame: i32,
    /// One weight per channel and frame.
    pub frames: Vec<[f32; CHANNELS]>,
}

/// Replace every zero-run marker by its zeros.
pub fn unpack(packed: &[u8]) -> Result<Vec<u8>, LipError> {
    let mut out = Vec::with_capacity(packed.len() * 4);
    let mut i = 0;
    while i < packed.len() {
        if packed[i] != 0 {
            out.push(packed[i]);
            i += 1;
            continue;
        }
        let Some(n) = packed.get(i + 1..i + 3) else { return Err(LipError::CutRun { at: i }) };
        out.resize(out.len() + usize::from(u16::from_le_bytes([n[0], n[1]])), 0);
        i += 3;
    }
    Ok(out)
}

/// Pack every run of zero bytes as a 0 byte and a u16 count. A run longer than 65,535
/// is split [hypothesis: the longest run in the owner's files is 21,428 bytes].
pub fn pack(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() / 2);
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != 0 {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        let run = raw[i..].iter().take(usize::from(u16::MAX)).take_while(|&&b| b == 0).count();
        out.push(0);
        out.extend((run as u16).to_le_bytes());
        i += run;
    }
    out
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl Lip {
    pub fn parse(bytes: &[u8]) -> Result<Lip, LipError> {
        if bytes.len() < HEADER_BYTES {
            return Err(LipError::NoHeader { have: bytes.len() });
        }
        let (version, declared, flag) = (u32_at(bytes, 0), u32_at(bytes, 4), u32_at(bytes, 8));
        let raw = unpack(&bytes[HEADER_BYTES..])?;
        if u64::from(declared) != raw.len() as u64 + u64::from(DECLARED_EXTRA) {
            return Err(LipError::Size { declared, unpacked: raw.len() });
        }
        if raw.len() < 8 {
            return Err(LipError::NoCounts { have: raw.len() });
        }
        let count = u32_at(&raw, 0);
        let start_frame = u32_at(&raw, 4) as i32;
        let need = 8 + count as usize * CHANNELS * 4;
        if raw.len() != need {
            return Err(LipError::Frames { frames: count, need, have: raw.len() });
        }
        let frames = raw[8..]
            .chunks_exact(CHANNELS * 4)
            .map(|f| {
                let mut v = [0.0f32; CHANNELS];
                for (c, x) in v.iter_mut().enumerate() {
                    *x = f32::from_bits(u32_at(f, c * 4));
                }
                v
            })
            .collect();
        Ok(Lip { version, flag, start_frame, frames })
    }

    /// The file this decodes from.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw = Vec::with_capacity(8 + self.frames.len() * CHANNELS * 4);
        raw.extend((self.frames.len() as u32).to_le_bytes());
        raw.extend(self.start_frame.to_le_bytes());
        for f in &self.frames {
            for x in f {
                raw.extend(x.to_bits().to_le_bytes());
            }
        }
        let mut out = Vec::new();
        out.extend(self.version.to_le_bytes());
        out.extend((raw.len() as u32 + DECLARED_EXTRA).to_le_bytes());
        out.extend(self.flag.to_le_bytes());
        out.extend(pack(&raw));
        out
    }

    /// When frame `frame` is due, in seconds after the sound starts.
    pub fn time_of(&self, frame: usize) -> f64 {
        (frame as f64 + f64::from(self.start_frame)) / FRAMES_PER_SECOND
    }

    /// The time of the first frame (negative: before the sound).
    pub fn start_seconds(&self) -> f64 {
        self.time_of(0)
    }

    /// The time of the last frame.
    pub fn end_seconds(&self) -> f64 {
        self.time_of(self.frames.len().saturating_sub(1))
    }

    /// The channel weights `seconds` after the sound starts: the two frames around that
    /// moment mixed linearly [hypothesis: how the game fills in between frames is not
    /// known]. `None` before the first and after the last frame.
    pub fn sample(&self, seconds: f64) -> Option<[f32; CHANNELS]> {
        let x = seconds * FRAMES_PER_SECOND - f64::from(self.start_frame);
        let last = self.frames.len().checked_sub(1)?;
        if x < -1e-9 || x > last as f64 + 1e-9 {
            return None;
        }
        let x = x.clamp(0.0, last as f64);
        let i = (x.floor() as usize).min(last);
        let j = (i + 1).min(last);
        let t = (x - i as f64) as f32;
        let mut out = [0.0; CHANNELS];
        for (c, o) in out.iter_mut().enumerate() {
            *o = self.frames[i][c] + (self.frames[j][c] - self.frames[i][c]) * t;
        }
        Some(out)
    }

    /// Channels that are not 0 in every frame.
    pub fn active_channels(&self) -> Vec<usize> {
        (0..CHANNELS).filter(|&c| self.frames.iter().any(|f| f[c] != 0.0)).collect()
    }

    /// The first and last frame in which a mouth-shape channel is not 0.
    pub fn speech_frames(&self) -> Option<(usize, usize)> {
        let talks = |f: &[f32; CHANNELS]| f[..PHONEMES].iter().any(|&x| x != 0.0);
        Some((self.frames.iter().position(talks)?, self.frames.iter().rposition(talks)?))
    }

    /// `fnv.voice.lip/1`: the tracks of the file. A channel that never leaves 0 has no
    /// `values`.
    pub fn to_json(&self, path: Option<&str>, sound_seconds: Option<f64>) -> serde_json::Value {
        let channels: Vec<serde_json::Value> = (0..CHANNELS)
            .map(|c| {
                let col: Vec<f32> = self.frames.iter().map(|f| f[c]).collect();
                let active = col.iter().any(|&x| x != 0.0);
                let (lo, hi) = col.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &x| (lo.min(x), hi.max(x)));
                serde_json::json!({
                    "index": c,
                    "name": CHANNEL_NAMES[c],
                    "kind": if c < PHONEMES { "phoneme" } else { "modifier" },
                    "min": if active { serde_json::json!(lo) } else { serde_json::json!(0.0) },
                    "max": if active { serde_json::json!(hi) } else { serde_json::json!(0.0) },
                    "first_frame": col.iter().position(|&x| x != 0.0),
                    "last_frame": col.iter().rposition(|&x| x != 0.0),
                    "values": if active { serde_json::json!(col) } else { serde_json::Value::Null },
                })
            })
            .collect();
        serde_json::json!({
            "format": LIP_FORMAT,
            "path": path,
            "version": self.version,
            "flag": self.flag,
            "frames": self.frames.len(),
            "frames_per_second": FRAMES_PER_SECOND,
            "start_frame": self.start_frame,
            "start_seconds": self.start_seconds(),
            "end_seconds": self.end_seconds(),
            "speech_frames": self.speech_frames().map(|(a, b)| serde_json::json!([a, b])),
            "sound_seconds": sound_seconds,
            "active_channels": self.active_channels().len(),
            "channels": channels,
        })
    }
}

pub const LIP_FORMAT: &str = "fnv.voice.lip/1";

/// The `.lip` path beside a sound's path.
pub fn lip_path(sound: &str) -> String {
    match sound.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.lip"),
        None => format!("{sound}.lip"),
    }
}

/// Letter groups of a response text that go with a consonant channel (bit = channel):
/// BMP 2, ChJSh 3, DST 4, FV 7, K 9, N 10, R 13, Th 14, W 15. A crude spelling test, used
/// only to check which channel is which over many lines (lipsync.md §3.1).
pub fn letter_groups(text: &str) -> u16 {
    let t = text.to_ascii_lowercase();
    let any = |set: &[&str]| set.iter().any(|s| t.contains(s));
    let mut m = 0u16;
    for (ch, set) in [
        (2usize, &["b", "m", "p"][..]),
        (3, &["ch", "j", "sh", "ge", "tio"]),
        (4, &["d", "s", "t", "z"]),
        (7, &["f", "v", "ph"]),
        (9, &["k", "g", "q", "x", "c"]),
        (10, &["n", "l"]),
        (13, &["r"]),
        (14, &["th"]),
        (15, &["w", "qu", "one"]),
    ] {
        if any(set) {
            m |= 1 << ch;
        }
    }
    m
}

/// The channels [`letter_groups`] tests.
pub const LETTER_CHANNELS: [usize; 9] = [2, 3, 4, 7, 9, 10, 13, 14, 15];

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(set: &[(usize, f32)]) -> [f32; CHANNELS] {
        let mut f = [0.0; CHANNELS];
        for &(c, v) in set {
            f[c] = v;
        }
        f
    }

    #[test]
    fn zero_runs_pack_and_unpack() {
        let raw = [7u8, 0, 0, 0, 0xFC, 0xFF, 0xFF, 0xFF, 0, 1, 0, 0, 0, 0, 0, 9];
        let packed = pack(&raw);
        assert_eq!(packed, [7, 0, 3, 0, 0xFC, 0xFF, 0xFF, 0xFF, 0, 1, 0, 1, 0, 5, 0, 9]);
        assert_eq!(unpack(&packed).unwrap(), raw);
        // A count that is cut off.
        assert_eq!(unpack(&[1, 0, 5]), Err(LipError::CutRun { at: 1 }));
        // A run longer than a u16 holds is split.
        let long = vec![0u8; 70_000];
        let p = pack(&long);
        assert_eq!(p, [0, 0xFF, 0xFF, 0, 0x71, 0x11]);
        assert_eq!(unpack(&p).unwrap().len(), 70_000);
    }

    #[test]
    fn a_file_round_trips() {
        let lip = Lip {
            version: 1,
            flag: 1,
            start_frame: -4,
            frames: vec![frame(&[(0, 0.25)]), frame(&[(0, 0.5), (2, 1.0)]), frame(&[]), frame(&[(16, 1.0), (17, 1.0), (30, -0.05)])],
        };
        let bytes = lip.to_bytes();
        // Header: 1, unpacked size + 16, 1.
        assert_eq!(&bytes[..12], &[1, 0, 0, 0, (8 + 4 * 132 + 16) as u8, 2, 0, 0, 1, 0, 0, 0]);
        // The body starts with the frame count (4, then three zeros packed) and −4.
        assert_eq!(&bytes[12..20], &[4, 0, 3, 0, 0xFC, 0xFF, 0xFF, 0xFF]);
        assert_eq!(Lip::parse(&bytes).unwrap(), lip);
        assert_eq!(lip.active_channels(), vec![0, 2, 16, 17, 30]);
        assert_eq!(lip.speech_frames(), Some((0, 1)));
    }

    #[test]
    fn broken_files_are_refused() {
        let lip = Lip { version: 1, flag: 1, start_frame: 0, frames: vec![frame(&[(1, 1.0)]); 3] };
        let mut bytes = lip.to_bytes();
        assert!(matches!(Lip::parse(&bytes[..8]), Err(LipError::NoHeader { .. })));
        // A wrong declared size.
        bytes[4] ^= 1;
        assert!(matches!(Lip::parse(&bytes), Err(LipError::Size { .. })));
        bytes[4] ^= 1;
        // A frame count the body does not hold: 2 frames declared, 3 present.
        let mut raw = unpack(&bytes[12..]).unwrap();
        raw[0] = 2;
        let mut wrong = bytes[..12].to_vec();
        wrong.extend(pack(&raw));
        assert_eq!(Lip::parse(&wrong), Err(LipError::Frames { frames: 2, need: 8 + 2 * 132, have: 8 + 3 * 132 }));
        // Cut inside the body.
        let cut = &bytes[..bytes.len() - 5];
        assert!(Lip::parse(cut).is_err());
    }

    #[test]
    fn frames_sit_on_the_sounds_clock() {
        let lip = Lip { version: 1, flag: 1, start_frame: -3, frames: (0..10).map(|i| frame(&[(0, i as f32)])).collect() };
        assert!((lip.start_seconds() + 0.1).abs() < 1e-12);
        assert!((lip.end_seconds() - 0.2).abs() < 1e-12);
        // Frame 3 is the moment the sound starts.
        assert_eq!(lip.sample(0.0).unwrap()[0], 3.0);
        // Halfway between frames 4 and 5.
        assert!((lip.sample(1.5 / 30.0).unwrap()[0] - 4.5).abs() < 1e-5);
        assert_eq!(lip.sample(0.2).unwrap()[0], 9.0);
        assert!(lip.sample(-0.11).is_none());
        assert!(lip.sample(0.21).is_none());
        assert!(Lip { frames: Vec::new(), ..lip }.sample(0.0).is_none());
    }

    #[test]
    fn names_and_letters() {
        assert_eq!(CHANNEL_NAMES.len(), CHANNELS);
        assert_eq!(channel("bmp"), Some(2));
        assert_eq!(channel("BlinkLeft"), Some(PHONEMES));
        assert_eq!(channel("HeadYaw"), Some(32));
        assert_eq!(channel("Tongue"), None);
        assert_eq!(lip_path(r"sound\voice\a\b\c_1.ogg"), r"sound\voice\a\b\c_1.lip");
        let m = letter_groups("Why, hello.");
        assert_eq!(m, (1 << 15) | (1 << 10));
        assert_eq!(letter_groups("Bath"), (1 << 2) | (1 << 4) | (1 << 14));
        assert_eq!(letter_groups("A... uh."), 0);
    }

    #[test]
    fn json_lists_tracks() {
        let lip = Lip { version: 1, flag: 1, start_frame: -1, frames: vec![frame(&[]), frame(&[(2, 0.5)]), frame(&[(2, 1.0)])] };
        let j = lip.to_json(Some("x.lip"), Some(0.05));
        assert_eq!(j["format"], LIP_FORMAT);
        assert_eq!(j["frames"], 3);
        assert_eq!(j["active_channels"], 1);
        assert_eq!(j["channels"][2]["name"], "BMP");
        assert_eq!(j["channels"][2]["values"], serde_json::json!([0.0, 0.5, 1.0]));
        assert_eq!(j["channels"][2]["first_frame"], 1);
        assert!(j["channels"][3]["values"].is_null());
        assert_eq!(j["speech_frames"], serde_json::json!([1, 2]));
    }
}
