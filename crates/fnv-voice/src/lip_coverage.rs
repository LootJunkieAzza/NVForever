//! `fnv.voice.lip_coverage/1`: every `.lip` file of an install decoded and counted
//! (docs/spec-notes/lipsync.md quotes the numbers for the owner's install).

use crate::bank::VoiceBank;
use crate::lip::{Lip, CHANNELS, CHANNEL_NAMES, FRAMES_PER_SECOND, LETTER_CHANNELS, PHONEMES};
use crate::path::{parse_voice_path, VOICE_ROOT};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

pub const LIP_COVERAGE_FORMAT: &str = "fnv.voice.lip_coverage/1";

/// A channel counts as used by a line when it reaches this weight (for the letter test).
pub const USED_WEIGHT: f32 = 0.3;

#[derive(Clone, Copy)]
struct ChannelStat {
    files_active: u64,
    min: f32,
    max: f32,
}

/// Counts of the letter test for one channel.
#[derive(Clone, Copy, Default)]
struct LetterStat {
    with_letters: u64,
    with_letters_used: u64,
    without_letters: u64,
    without_letters_used: u64,
}

fn bucket(d: i64) -> &'static str {
    match d {
        i64::MIN..=-31 => "more than 1 s longer",
        -30..=-11 => "11 to 30 frames longer",
        -10..=-6 => "6 to 10 frames longer",
        -5..=-2 => "2 to 5 frames longer",
        -1..=1 => "within 1 frame",
        2..=5 => "2 to 5 frames shorter",
        6..=30 => "6 to 30 frames shorter",
        _ => "more than 1 s shorter",
    }
}

/// The summary. With `lengths`, each file's sound is measured too (its headers are read
/// from the archive) and the animation's end is compared with the sound's.
pub fn lip_coverage(bank: &VoiceBank, lengths: bool) -> Value {
    let ix = &bank.index;
    let vfs = bank.vfs();
    let prefix = format!("{VOICE_ROOT}\\");
    let mut paths = vfs.paths(|p| p.starts_with(&prefix) && p.ends_with(".lip"));
    paths.sort();

    // (plugin folder, file name) → letter groups of the response's text.
    let mut letters: HashMap<(String, String), u16> = HashMap::new();
    for (&info, line) in &ix.lines {
        let Some(plugin) = ix.plugin_of(info) else { continue };
        for (k, r) in line.responses.iter().enumerate() {
            if let Some(stem) = ix.stem(info, k) {
                letters.insert((plugin.to_ascii_lowercase(), stem), r.letters);
            }
        }
    }

    let mut failed: Vec<Value> = Vec::new();
    let (mut decoded, mut identical) = (0u64, 0u64);
    let (mut stored_bytes, mut frame_total) = (0u64, 0u64);
    let (mut frames_min, mut frames_max) = (u64::MAX, 0u64);
    let mut versions: BTreeMap<String, u64> = BTreeMap::new();
    let mut start_frames: BTreeMap<i32, u64> = BTreeMap::new();
    let mut ch = [ChannelStat { files_active: 0, min: 0.0, max: 0.0 }; CHANNELS];
    let (mut blink_equal, mut brow_up_equal) = (0u64, 0u64);
    let (mut look_down_and_up, mut look_left_and_right) = (0u64, 0u64);
    let mut look_follows_pitch = 0u64;
    let mut not_finite = 0u64;
    let mut no_speech = 0u64;
    let mut letter_stats = [LetterStat::default(); CHANNELS];
    let mut with_text = 0u64;
    // Animation against sound.
    let mut with_sound = 0u64;
    let mut end_vs_sound: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut speech_vs_sound: BTreeMap<&'static str, u64> = BTreeMap::new();
    let (mut anim_seconds, mut sound_seconds) = (0.0f64, 0.0f64);

    for p in &paths {
        let Some(Ok(bytes)) = vfs.read(p) else {
            failed.push(json!({"path": p, "error": "unreadable"}));
            continue;
        };
        stored_bytes += bytes.len() as u64;
        let lip = match Lip::parse(&bytes) {
            Ok(l) => l,
            Err(e) => {
                failed.push(json!({"path": p, "error": e.to_string()}));
                continue;
            }
        };
        decoded += 1;
        identical += u64::from(lip.to_bytes() == bytes);
        *versions.entry(format!("{}/{}", lip.version, lip.flag)).or_insert(0) += 1;
        *start_frames.entry(lip.start_frame).or_insert(0) += 1;
        let n = lip.frames.len() as u64;
        frame_total += n;
        frames_min = frames_min.min(n);
        frames_max = frames_max.max(n);
        let mut peak = [0.0f32; CHANNELS];
        let mut active = [false; CHANNELS];
        let (mut blink, mut brow, mut finite, mut pitch) = (true, true, true, true);
        for f in &lip.frames {
            for c in 0..CHANNELS {
                let x = f[c];
                finite &= x.is_finite();
                active[c] |= x != 0.0;
                peak[c] = peak[c].max(x.abs());
                ch[c].min = ch[c].min.min(x);
                ch[c].max = ch[c].max.max(x);
            }
            blink &= f[16] == f[17];
            brow &= f[22] == f[23];
            look_down_and_up += u64::from(f[24] != 0.0 && f[27] != 0.0);
            look_left_and_right += u64::from(f[25] != 0.0 && f[26] != 0.0);
            pitch &= ((f[24] - f[27]) + f[30]).abs() <= 1e-4;
        }
        for c in 0..CHANNELS {
            ch[c].files_active += u64::from(active[c]);
        }
        blink_equal += u64::from(blink);
        brow_up_equal += u64::from(brow);
        look_follows_pitch += u64::from(pitch);
        not_finite += u64::from(!finite);
        let speech = lip.speech_frames();
        no_speech += u64::from(speech.is_none());

        let name = parse_voice_path(p);
        if let Some(&mask) = name.as_ref().and_then(|f| letters.get(&(f.plugin.to_string(), f.stem.to_string()))) {
            with_text += 1;
            for &c in &LETTER_CHANNELS {
                let used = u64::from(peak[c] >= USED_WEIGHT);
                let s = &mut letter_stats[c];
                if mask & (1 << c) != 0 {
                    s.with_letters += 1;
                    s.with_letters_used += used;
                } else {
                    s.without_letters += 1;
                    s.without_letters_used += used;
                }
            }
        }

        if lengths {
            let sound = crate::path::SOUND_EXTENSIONS.iter().find_map(|ext| bank.seconds(&format!("{}.{ext}", &p[..p.len() - 4])));
            if let Some(secs) = sound {
                with_sound += 1;
                sound_seconds += secs;
                anim_seconds += n as f64 / FRAMES_PER_SECOND;
                let sound_frames = (secs * FRAMES_PER_SECOND).round() as i64;
                // Frames from the sound's start to the animation's end, and to the end
                // of its last mouth shape.
                let end = n as i64 + i64::from(lip.start_frame);
                *end_vs_sound.entry(bucket(sound_frames - end)).or_insert(0) += 1;
                if let Some((_, last)) = speech {
                    let end = last as i64 + 1 + i64::from(lip.start_frame);
                    *speech_vs_sound.entry(bucket(sound_frames - end)).or_insert(0) += 1;
                }
            }
        }
    }

    let channels: Vec<Value> = (0..CHANNELS)
        .map(|c| {
            json!({
                "index": c,
                "name": CHANNEL_NAMES[c],
                "kind": if c < PHONEMES { "phoneme" } else { "modifier" },
                "files_active": ch[c].files_active,
                "min": ch[c].min,
                "max": ch[c].max,
            })
        })
        .collect();
    let letter_test: Vec<Value> = LETTER_CHANNELS
        .iter()
        .map(|&c| {
            let s = letter_stats[c];
            json!({
                "channel": c,
                "name": CHANNEL_NAMES[c],
                "lines_with_the_letters": s.with_letters,
                "used_with_the_letters": s.with_letters_used,
                "lines_without_the_letters": s.without_letters,
                "used_without_the_letters": s.without_letters_used,
            })
        })
        .collect();
    let starts: Map<String, Value> = start_frames.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let mut out = json!({
        "format": LIP_COVERAGE_FORMAT,
        "files": paths.len(),
        "decoded_without_residue": decoded,
        "reencode_byte_identical": identical,
        "failed": failed.len(),
        "failed_first": failed.iter().take(10).collect::<Vec<_>>(),
        "header_version_and_flag": versions,
        "stored_bytes": stored_bytes,
        "channels_per_frame": CHANNELS,
        "frames": {
            "total": frame_total,
            "fewest": if decoded > 0 { json!(frames_min) } else { Value::Null },
            "most": frames_max,
        },
        "start_frame": {
            "lowest": start_frames.keys().next(),
            "highest": start_frames.keys().next_back(),
            "files_above_zero": start_frames.iter().filter(|(k, _)| **k > 0).map(|(_, v)| v).sum::<u64>(),
            "files_by_value": starts,
        },
        "channels": channels,
        "files_with_a_value_that_is_not_finite": not_finite,
        "files_without_any_mouth_shape": no_speech,
        "relations": {
            "files_where_blink_left_equals_blink_right_in_every_frame": blink_equal,
            "files_where_brow_up_left_equals_brow_up_right_in_every_frame": brow_up_equal,
            "frames_with_look_down_and_look_up": look_down_and_up,
            "frames_with_look_left_and_look_right": look_left_and_right,
            "files_where_look_down_minus_look_up_is_minus_head_pitch_in_every_frame": look_follows_pitch,
        },
        "letter_test": {
            "used_means_peak_weight_at_least": USED_WEIGHT,
            "files_with_the_text_of_their_response": with_text,
            "channels": letter_test,
        },
    });
    if lengths {
        out["against_sound"] = json!({
            "files_with_a_sound": with_sound,
            "frames_per_second": FRAMES_PER_SECOND,
            "animation_seconds": (anim_seconds * 10.0).round() / 10.0,
            "sound_seconds": (sound_seconds * 10.0).round() / 10.0,
            "animation_end_against_sound_end": end_vs_sound,
            "last_mouth_shape_against_sound_end": speech_vs_sound,
        });
    }
    out
}
