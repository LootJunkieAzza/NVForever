//! The voice index, bank and coverage on synthetic plugins and synthetic sound files
//! written to a temporary folder (no game data): a master with a quest, a topic of three
//! lines and three speakers, and a plugin that adds a line and overrides one.

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin};
use fnv_game::voice::VoiceLengths;
use fnv_sim::FormId;
use fnv_vfs::Vfs;
use fnv_voice::length::synth::{ogg_vorbis, wav_pcm};
use fnv_voice::{VoiceBank, VoiceIndex};
use std::path::PathBuf;
use std::sync::Arc;

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn id(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
        .concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        subs.push(sub(b"MAST", z(m)));
        subs.push(sub(b"DATA", vec![0; 8]));
    }
    record(b"TES4", if masters.is_empty() { 1 } else { 0 }, 0, &subs)
}

/// TRDT: response number at byte 12, sound FormID at 16 (24 bytes).
fn trdt(number: u8, sound: u32) -> Subrecord {
    let mut d = vec![0u8; 24];
    d[12] = number;
    d[16..20].copy_from_slice(&sound.to_le_bytes());
    sub(b"TRDT", d)
}

fn info(fid: u32, quest: u32, speaker: Option<u32>, responses: &[(u8, &str, u32)]) -> Vec<u8> {
    let mut subs = vec![sub(b"DATA", vec![0, 0, 0, 0]), sub(b"QSTI", id(quest))];
    for &(n, text, sound) in responses {
        subs.push(trdt(n, sound));
        subs.push(sub(b"NAM1", z(text)));
    }
    if let Some(s) = speaker {
        subs.push(sub(b"ANAM", id(s)));
    }
    record(b"INFO", 0, fid, &subs)
}

const VOICE_M: u32 = 0x800;
const VOICE_F: u32 = 0x801;
const NPC: u32 = 0x810;
const CREATURE: u32 = 0x811;
const TALKER: u32 = 0x812;
const MUTE: u32 = 0x813;
const QUEST: u32 = 0x820;
const LONG_QUEST: u32 = 0x821;
const TOPIC: u32 = 0x830;
const LINE_A: u32 = 0x840;
const LINE_B: u32 = 0x841;
const LINE_C: u32 = 0x842;
const SOUND_FORM: u32 = 0x850;
/// The plugin's own records (it has one master, so its own index is 1).
const DLC_QUEST: u32 = 0x0100_0800;
const DLC_LINE: u32 = 0x0100_0900;

fn master() -> Vec<u8> {
    let vtyp = [record(b"VTYP", 0, VOICE_M, &[sub(b"EDID", z("MaleTest"))]), record(b"VTYP", 0, VOICE_F, &[sub(b"EDID", z("FemaleTest"))])].concat();
    let npc = record(b"NPC_", 0, NPC, &[sub(b"EDID", z("Speaker")), sub(b"VTCK", id(VOICE_M))]);
    // ACBS with the template flags (u16 at byte 22): bit 0 Traits.
    let acbs = |template_flags: u16| {
        let mut d = vec![0u8; 24];
        d[22..24].copy_from_slice(&template_flags.to_le_bytes());
        sub(b"ACBS", d)
    };
    // A creature that takes its traits (so its voice) from the NPC, although it has a
    // voice type of its own; one templated for its inventory only, without a voice type.
    let crea = [
        record(b"CREA", 0, CREATURE, &[sub(b"EDID", z("Templated")), acbs(0x1), sub(b"VTCK", id(VOICE_F)), sub(b"TPLT", id(NPC))]),
        record(b"CREA", 0, MUTE, &[sub(b"EDID", z("Mute")), acbs(0x100), sub(b"TPLT", id(NPC))]),
    ]
    .concat();
    let tact = record(b"TACT", 0, TALKER, &[sub(b"EDID", z("Radio")), sub(b"VNAM", id(VOICE_F))]);
    let qust = [
        record(b"QUST", 0, QUEST, &[sub(b"EDID", z("TestQuest"))]),
        record(b"QUST", 0, LONG_QUEST, &[sub(b"EDID", z("ALongQuestEditorID"))]),
    ]
    .concat();
    let dial = [
        record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("TestTopicWithALongName"))]),
        group(
            TOPIC.to_le_bytes(),
            7,
            &[
                // Two responses stored out of number order; the second names a sound form.
                info(LINE_A, QUEST, Some(NPC), &[(2, "Second", 0), (1, "First", SOUND_FORM)]),
                info(LINE_B, LONG_QUEST, None, &[(1, "Any voice", 0)]),
                info(LINE_C, QUEST, None, &[(1, "Never recorded", 0)]),
            ]
            .concat(),
        ),
    ]
    .concat();
    [tes4(&[]), group(*b"VTYP", 0, &vtyp), group(*b"NPC_", 0, &npc), group(*b"CREA", 0, &crea), group(*b"TACT", 0, &tact), group(*b"QUST", 0, &qust), group(*b"DIAL", 0, &dial)]
        .concat()
}

fn dlc() -> Vec<u8> {
    let qust = record(b"QUST", 0, DLC_QUEST, &[sub(b"EDID", z("DlcQuest"))]);
    // The plugin adds a line to the master's topic and overrides line C (still unvoiced).
    let dial = [
        record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("TestTopicWithALongName"))]),
        group(TOPIC.to_le_bytes(), 7, &[info(DLC_LINE, DLC_QUEST, None, &[(1, "Added", 0)]), info(LINE_C, QUEST, None, &[(1, "Changed text", 0)])].concat()),
    ]
    .concat();
    [tes4(&["Voice.esm"]), group(*b"QUST", 0, &qust), group(*b"DIAL", 0, &dial)].concat()
}

/// A temporary data folder with loose voice files; removed on drop.
struct DataDir(PathBuf);

impl DataDir {
    fn new(tag: &str) -> DataDir {
        let dir = std::env::temp_dir().join(format!("fnv-voice-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        DataDir(dir)
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn plugins() -> Vec<Plugin> {
    vec![Plugin::parse(&master(), "Voice.esm".into()).unwrap(), Plugin::parse(&dlc(), "Dlc.esm".into()).unwrap()]
}

fn data_dir(tag: &str) -> DataDir {
    let d = DataDir::new(tag);
    let m = "sound/voice/voice.esm/maletest";
    let f = "sound/voice/voice.esm/femaletest";
    // Line A, both responses, in the NPC's voice: 1.5 s and 0.75 s. Response 1 also as WAV.
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_1.ogg"), &ogg_vorbis(1, 24_000, 36_000, 3));
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_1.wav"), &wav_pcm(1, 24_000, 16, 36_000));
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_1.lip"), b"lip");
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_2.ogg"), &ogg_vorbis(1, 24_000, 18_000, 2));
    // Line B in both voices (2 s male, 3 s female, the female one as WAV only).
    d.put(&format!("{m}/alongquest_testtopicwithal_00000841_1.ogg"), &ogg_vorbis(2, 22_050, 44_100, 2));
    d.put(&format!("{f}/alongquest_testtopicwithal_00000841_1.wav"), &wav_pcm(1, 8_000, 16, 24_000));
    // The plugin's own line, in its own folder, FormID with the load-order byte zeroed.
    d.put("sound/voice/dlc.esm/femaletest/dlcquest_testtopicwithalon_00000900_1.ogg", &ogg_vorbis(1, 24_000, 6_000, 1));
    // Sounds no line names: a deleted line's, a stray name, and a file that is no sound.
    d.put(&format!("{m}/testquest_gone_00000999_1.ogg"), &ogg_vorbis(1, 24_000, 100, 1));
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_1_alt.ogg"), &ogg_vorbis(1, 24_000, 100, 1));
    d.put(&format!("{m}/testquest_testtopicwithalo_00000840_3.ogg"), b"not a sound");
    d.put("sound/voice/empty.txt", b"");
    d
}

fn bank(dir: &DataDir, plugins: &[Plugin]) -> VoiceBank {
    let lo = LoadOrder::new(plugins).unwrap();
    VoiceBank::build(&lo, Arc::new(Vfs::open(&dir.0, &[]).unwrap()))
}

#[test]
fn the_index_reads_names_responses_and_voices() {
    let plugins = plugins();
    let ix = VoiceIndex::build(&LoadOrder::new(&plugins).unwrap());
    assert_eq!(ix.plugins, ["Voice.esm", "Dlc.esm"]);
    assert_eq!(ix.lines.len(), 4);
    let a = &ix.lines[&LINE_A];
    assert_eq!((a.quest, a.topic, a.speaker), (Some(QUEST), Some(TOPIC), Some(NPC)));
    assert_eq!(a.responses.iter().map(|r| (r.number, r.chars, r.sound)).collect::<Vec<_>>(), [(2, 6, None), (1, 5, Some(SOUND_FORM))]);
    // The override wins: line C's text is the plugin's.
    assert_eq!(ix.lines[&LINE_C].responses[0].chars, "Changed text".len());
    assert_eq!(ix.voice_types[&VOICE_M], "MaleTest");
    assert_eq!(ix.voice_of(NPC), Some(VOICE_M));
    assert_eq!(ix.voice_of(TALKER), Some(VOICE_F));
    // Traits from the template: the template's voice, not its own.
    assert_eq!(ix.voice_of(CREATURE), Some(VOICE_M));
    // Templated without the Traits flag and no voice type of its own.
    assert_eq!(ix.voice_of(MUTE), None);
    // Index 0 is the response stored first, number 2.
    assert_eq!(ix.stem(LINE_A, 0).as_deref(), Some("testquest_testtopicwithalo_00000840_2"));
    assert_eq!(ix.stem(LINE_A, 1).as_deref(), Some("testquest_testtopicwithalo_00000840_1"));
    assert_eq!(ix.stem(LINE_A, 2), None);
    // The plugin's line: its own folder, load-order byte zeroed.
    assert_eq!(ix.plugin_of(DLC_LINE), Some("Dlc.esm"));
    assert_eq!(
        ix.paths(DLC_LINE, 0, VOICE_F),
        ["sound\\voice\\dlc.esm\\femaletest\\dlcquest_testtopicwithalon_00000900_1.ogg", "sound\\voice\\dlc.esm\\femaletest\\dlcquest_testtopicwithalon_00000900_1.wav"]
    );
}

#[test]
fn the_bank_finds_a_speakers_file_and_its_length() {
    let dir = data_dir("bank");
    let plugins = plugins();
    let b = bank(&dir, &plugins);
    // Line A as its speaker says it: response index 0 is number 2 (0.75 s), index 1 number 1.
    let p0 = b.file(LINE_A, 0, NPC).unwrap();
    assert!(p0.ends_with("_00000840_2.ogg"), "{p0}");
    assert_eq!(b.seconds(&p0), Some(0.75));
    // Both an Ogg and a WAV exist: the Ogg is taken.
    let p1 = b.file(LINE_A, 1, NPC).unwrap();
    assert!(p1.ends_with("_00000840_1.ogg"), "{p1}");
    assert_eq!(b.seconds(&p1), Some(1.5));
    assert_eq!(b.info(&p1).unwrap().sample_rate, 24_000);
    assert!(b.read(&p1).unwrap().starts_with(b"OggS"));
    // The templated creature speaks with the NPC's voice; the talking activator has no
    // file for this line in its own voice, and no other voice's file stands in.
    assert_eq!(b.file(LINE_A, 0, CREATURE), Some(p0));
    assert_eq!(b.file(LINE_A, 0, TALKER), None);
    assert_eq!(b.file(LINE_A, 0, MUTE), None);
    // Line B per voice: WAV when that is all there is.
    assert_eq!(b.seconds(&b.file(LINE_B, 0, NPC).unwrap()), Some(2.0));
    let w = b.file(LINE_B, 0, TALKER).unwrap();
    assert!(w.ends_with(".wav"), "{w}");
    assert_eq!(b.seconds(&w), Some(3.0));
    // Never recorded; no such response; no such line.
    assert_eq!(b.file(LINE_C, 0, NPC), None);
    assert_eq!(b.file(LINE_A, 2, NPC), None);
    assert_eq!(b.file(0x999, 0, NPC), None);
    // A file that is no sound has no length.
    assert_eq!(b.seconds("sound\\voice\\voice.esm\\maletest\\testquest_testtopicwithalo_00000840_3.ogg"), None);
}

#[test]
fn the_bank_is_the_games_line_length_hook() {
    let dir = data_dir("hook");
    let plugins = plugins();
    let b = bank(&dir, &plugins);
    let hook: &dyn VoiceLengths = &b;
    assert_eq!(hook.response_seconds(FormId(LINE_A), 0, Some(FormId(NPC))), Some(0.75));
    assert_eq!(hook.response_seconds(FormId(LINE_A), 1, Some(FormId(NPC))), Some(1.5));
    assert_eq!(hook.response_seconds(FormId(DLC_LINE), 0, Some(FormId(TALKER))), Some(0.25));
    // No file, no speaker, a speaker without a voice type: the game keeps its fitted length.
    assert_eq!(hook.response_seconds(FormId(LINE_C), 0, Some(FormId(NPC))), None);
    assert_eq!(hook.response_seconds(FormId(LINE_A), 0, None), None);
    assert_eq!(hook.response_seconds(FormId(LINE_A), 0, Some(FormId(MUTE))), None);
}

#[test]
fn coverage_counts_files_and_lines() {
    let dir = data_dir("coverage");
    let plugins = plugins();
    let b = bank(&dir, &plugins);
    let c = fnv_voice::coverage::coverage(&b, true);
    assert_eq!(c["format"], "fnv.voice.coverage/1");
    let f = &c["files"];
    assert_eq!(f["under_voice_root"], 11);
    assert_eq!(f["not_in_a_voice_folder"], 1);
    assert_eq!(f["by_extension"], serde_json::json!({"lip": 1, "ogg": 7, "wav": 2}));
    assert_eq!(f["by_source"], serde_json::json!({"loose": 10}));
    assert_eq!(f["voice_folders"], serde_json::json!({"dlc.esm": 1, "voice.esm": 2}));
    assert_eq!(f["voice_folders_not_a_voice_type"], 0);
    // Sounds by (folder, voice, name): 8; one with a lip file; one as both types.
    assert_eq!(f["sounds"], 8);
    assert_eq!(f["sounds_with_lip"], 1);
    assert_eq!(f["sounds_as_both_ogg_and_wav"], 1);
    assert_eq!(f["sound_names_not_by_the_rule"], 1);
    assert_eq!(
        f["sounds_no_line_names"],
        serde_json::json!({"form_id_is_no_line_of_the_plugin": 1, "line_exists_under_another_name_or_response": 1, "name_not_by_the_rule": 1})
    );
    let l = &c["lines"];
    assert_eq!(l["total"]["lines"], 4);
    assert_eq!(l["total"]["responses"], 5);
    assert_eq!(l["total"]["responses_with_sound"], 4);
    assert_eq!(l["total"]["responses_without_sound"], 1);
    assert_eq!(l["total"]["share_with_sound"], 0.8);
    assert_eq!(l["by_plugin"]["Dlc.esm"]["responses_with_sound"], 1);
    assert_eq!(l["by_plugin"]["Voice.esm"]["responses_without_sound"], 1);
    assert_eq!(l["lines_numbered_1_to_n_in_order"], 3);
    assert_eq!(l["lines_numbered_otherwise"], 1);
    assert_eq!(l["voices_per_response_with_sound"], serde_json::json!({"1": 3, "2": 1}));
    assert_eq!(l["responses_of_lines_with_a_fixed_speaker"], serde_json::json!({"file_in_the_speakers_voice": 2}));
    assert_eq!(l["responses_with_a_sound_form"], serde_json::json!({"total": 1, "with_a_voice_file": 1}));
    assert_eq!(l["most_responses_without_sound"], serde_json::json!([{"quest": "TestQuest", "responses": 1}]));
    // Lengths of the sounds lines name: 0.75 + 1.5 + 2 + 3 + 0.25 s.
    assert_eq!(c["lengths"]["sounds_read"], 5);
    assert_eq!(c["lengths"]["unreadable"], 0);
    assert_eq!(c["lengths"]["seconds_total"], 7.5);
    assert_eq!(c["lengths"]["sample_rates"], serde_json::json!({"22050": 1, "24000": 3, "8000": 1}));
}
