//! Checks against a real install (read-only). They run only when `NVFOREVER_DATA` or
//! `FNVLAB_DATA_DIR` names the game's `Data` folder, and pass as skipped otherwise:
//!
//! ```text
//! NVFOREVER_DATA="<...>/Fallout New Vegas/Data" cargo test --release -p fnv-voice --test real_data -- --nocapture
//! ```
//!
//! The numbers are the ones docs/spec-notes/voice.md quotes as observed on the owner's
//! install (FNV 1.4.0.525, Steam, English, all add-ons).

use fnv_esm::{LoadOrder, Plugin};
use fnv_vfs::{Vfs, DEFAULT_ARCHIVE_LIST, DEFAULT_PLUGINS};
use fnv_voice::path::parse_voice_path;
use fnv_voice::VoiceBank;
use std::path::PathBuf;
use std::sync::Arc;

fn data_dir() -> Option<PathBuf> {
    let dir = ["NVFOREVER_DATA", "FNVLAB_DATA_DIR"].iter().find_map(std::env::var_os).map(PathBuf::from)?;
    dir.join("FalloutNV.esm").is_file().then_some(dir)
}

fn with_bank(test: &str, f: impl FnOnce(&VoiceBank)) {
    let Some(dir) = data_dir() else {
        eprintln!("{test}: skipped (no game data: set NVFOREVER_DATA to the game's Data folder)");
        return;
    };
    let plugins: Vec<Plugin> = DEFAULT_PLUGINS.iter().map(|n| Plugin::open(dir.join(n)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let order = Vfs::archive_order(&dir, &DEFAULT_ARCHIVE_LIST, &DEFAULT_PLUGINS);
    let vfs = Arc::new(Vfs::open(&dir, &order).unwrap());
    f(&VoiceBank::build(&lo, vfs));
}

#[test]
fn coverage_of_the_ten_masters() {
    with_bank("coverage_of_the_ten_masters", |bank| {
        let c = fnv_voice::coverage::coverage(bank, false);
        let f = &c["files"];
        assert_eq!(f["by_extension"], serde_json::json!({"lip": 60702, "ogg": 61290, "wav": 40}));
        assert_eq!(f["sounds"], 61290);
        assert_eq!(f["sounds_with_lip"], 60702);
        assert_eq!(f["lip_without_sound"], 0);
        assert_eq!(f["sounds_as_both_ogg_and_wav"], 40);
        assert_eq!(f["voice_folders_not_a_voice_type"], 0);
        assert_eq!(
            f["sounds_no_line_names"],
            serde_json::json!({"form_id_is_no_line_of_the_plugin": 138, "line_exists_under_another_name_or_response": 33, "name_not_by_the_rule": 10})
        );
        let t = &c["lines"]["total"];
        assert_eq!((t["lines"].as_u64(), t["responses"].as_u64()), (Some(28896), Some(37403)));
        assert_eq!(t["responses_with_sound"], 36403);
        assert_eq!(t["share_with_sound"], 0.9733);
        assert_eq!(
            c["lines"]["responses_of_lines_with_a_fixed_speaker"],
            serde_json::json!({"file_in_the_speakers_voice": 22252, "file_only_in_another_voice": 15, "no_file": 317, "speaker_has_no_voice_type": 5})
        );
    });
}

#[test]
fn doc_mitchells_first_line() {
    with_bank("doc_mitchells_first_line", |bank| {
        let ix = &bank.index;
        // The first line of VCG01Intro, said by Doc Mitchell's NPC_ record.
        let (line, doc) = (0x0010_4BEE, 0x0010_4C0C);
        assert_eq!(ix.voice_of(doc).and_then(|v| ix.voice_types.get(&v)).map(String::as_str), Some("MaleUniqueDocMitchell"));
        let path = bank.file(line, 0, doc).expect("the line has a sound");
        assert_eq!(path, "sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\vcg01_vcg01intro_00104bee_1.ogg");
        assert_eq!(bank.vfs().source_of(&path), Some("Fallout - Voices1.bsa"));
        let i = bank.info(&path).unwrap();
        assert_eq!((i.channels, i.sample_rate), (1, 24_000));
        assert!((i.seconds - 2.61).abs() < 0.01, "{}", i.seconds);
    });
}

#[test]
fn the_wav_copies_are_as_long_as_their_ogg_files() {
    with_bank("the_wav_copies_are_as_long_as_their_ogg_files", |bank| {
        let wavs = bank.vfs().paths(|p| p.ends_with(".wav") && parse_voice_path(p).is_some());
        assert_eq!(wavs.len(), 40);
        for w in wavs {
            let ogg = format!("{}.ogg", w.strip_suffix(".wav").unwrap());
            let (a, b) = (bank.info(&w).unwrap(), bank.info(&ogg).unwrap());
            assert_eq!((a.sample_rate, a.channels), (b.sample_rate, b.channels), "{w}");
            assert!((a.seconds - b.seconds).abs() < 0.001, "{w}: {} against {}", a.seconds, b.seconds);
        }
    });
}

/// docs/spec-notes/lipsync.md §2 and §3: every `.lip` decodes by the rules of
/// `fnv_voice::lip`, and the consonant channels follow the letters of the text.
#[test]
fn every_lip_file_decodes() {
    with_bank("every_lip_file_decodes", |bank| {
        let c = fnv_voice::lip_coverage::lip_coverage(bank, true);
        assert_eq!(c["files"], 60702);
        assert_eq!(c["decoded_without_residue"], 60702);
        assert_eq!(c["reencode_byte_identical"], 60702);
        assert_eq!(c["failed"], 0);
        assert_eq!(c["header_version_and_flag"], serde_json::json!({"1/1": 60702}));
        assert_eq!(c["frames"], serde_json::json!({"total": 8436876, "fewest": 15, "most": 1260}));
        assert_eq!((c["start_frame"]["lowest"].as_i64(), c["start_frame"]["highest"].as_i64()), (Some(-36), Some(6)));
        assert_eq!(c["start_frame"]["files_above_zero"], 35);
        assert_eq!(c["files_with_a_value_that_is_not_finite"], 0);
        assert_eq!(c["files_without_any_mouth_shape"], 0);
        // Channels no file uses: brow down, brow in, squint.
        let unused: Vec<u64> = c["channels"].as_array().unwrap().iter().filter(|x| x["files_active"] == 0).map(|x| x["index"].as_u64().unwrap()).collect();
        assert_eq!(unused, vec![18, 19, 20, 21, 28, 29]);
        // Mouth shapes stay within 0..1 (rounding aside); only the head turns go below 0.
        for x in c["channels"].as_array().unwrap() {
            let (i, lo, hi) = (x["index"].as_u64().unwrap(), x["min"].as_f64().unwrap(), x["max"].as_f64().unwrap());
            if i < 16 {
                assert!(lo == 0.0 && hi <= 1.000001, "channel {i}: {lo}..{hi}");
            }
            if i < 30 {
                assert!(lo > -1e-6, "channel {i}: {lo}");
            }
        }
        let r = &c["relations"];
        assert_eq!(r["files_where_blink_left_equals_blink_right_in_every_frame"], 60702);
        assert_eq!(r["files_where_brow_up_left_equals_brow_up_right_in_every_frame"], 60702);
        assert_eq!(r["frames_with_look_down_and_look_up"], 0);
        assert_eq!(r["frames_with_look_left_and_look_right"], 0);
        // The letter test: a consonant channel is used by most lines whose text has its
        // letters and by few of the others.
        let t = &c["letter_test"];
        assert_eq!(t["files_with_the_text_of_their_response"], 60600);
        for x in t["channels"].as_array().unwrap() {
            let n = |k: &str| x[k].as_f64().unwrap();
            let with = n("used_with_the_letters") / n("lines_with_the_letters");
            let without = n("used_without_the_letters") / n("lines_without_the_letters");
            assert!(with > 0.66 && without < 0.14 && with > 5.0 * without, "{}: {with:.3} against {without:.3}", x["name"]);
        }
        // 30 frames a second: the last mouth shape ends within a frame of the sound's end
        // for three files in four.
        let s = &c["against_sound"];
        assert_eq!(s["files_with_a_sound"], 60702);
        assert_eq!(s["last_mouth_shape_against_sound_end"]["within 1 frame"], 45042);
    });
}

#[test]
fn doc_mitchells_first_line_has_a_lip_file() {
    with_bank("doc_mitchells_first_line_has_a_lip_file", |bank| {
        let path = bank.file(0x0010_4BEE, 0, 0x0010_4C0C).expect("the line has a sound");
        let lip = bank.lip(&path).expect("and a lip file");
        assert_eq!((lip.version, lip.flag, lip.start_frame, lip.frames.len()), (1, 1, -4, 127));
        assert_eq!(lip.speech_frames(), Some((0, 89)));
        assert_eq!(lip.active_channels().len(), 17);
        // "You're awake. How about that.": the lips meet for the b of "about" about two
        // seconds in, and nowhere else.
        let bmp = fnv_voice::lip::channel("BMP").unwrap();
        let peak = (0..lip.frames.len()).max_by(|&a, &b| lip.frames[a][bmp].total_cmp(&lip.frames[b][bmp])).unwrap();
        assert!((lip.time_of(peak) - 1.97).abs() < 0.04, "{}", lip.time_of(peak));
        assert!(lip.frames.iter().enumerate().all(|(f, v)| v[bmp] == 0.0 || (58..=65).contains(&f)));
        // No f, v, ch, j or sh in the line.
        for name in ["FV", "ChJSh"] {
            let c = fnv_voice::lip::channel(name).unwrap();
            assert!(lip.frames.iter().all(|v| v[c] == 0.0), "{name}");
        }
    });
}
