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
