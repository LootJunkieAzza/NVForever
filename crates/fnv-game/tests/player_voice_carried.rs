//! Voice sets whose lines carry their sound and subtitle (docs/design/player-voices.md,
//! `fnv_game::player_voice::add_packed_lines`): each line becomes a line of its own in the
//! data, numbered alike in every game with the same sets.

use fnv_esm::{LoadOrder, Schema};
use fnv_game::player_voice::{add_packed_lines, PlayerVoice, SetLine, VoiceSet, FORMAT, PACKED_BASE};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, RefTable};
use serde_json::Value as Json;

fn data() -> GameData {
    let lo = LoadOrder::new(&[]).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let refs = RefTable::build(&lo, &schema);
    GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs)
}

fn line(event: &str, file: Option<&str>, text: &str, first_person: bool) -> SetLine {
    SetLine {
        info: "Mod.esp|000ABC".into(),
        event: event.into(),
        first_person,
        use_line: true,
        note: String::new(),
        file: file.map(str::to_string),
        text: Some(text.into()),
        near: vec![],
        inside: vec![],
    }
}

fn set(id: &str, lines: Vec<SetLine>) -> VoiceSet {
    VoiceSet { format: FORMAT.into(), id: id.into(), name: id.into(), sex: "female".into(), race: String::new(), blurb: String::new(), speaker: String::new(), rate: 1.0, companion: None, lines }
}

#[test]
fn carried_lines_become_lines_of_their_own_with_their_sound_and_subtitle() {
    let mut d = data();
    let b = set("nexus.b", vec![line("Attack", Some("sound/voice/mod.esp/v/a_1.ogg"), "Bring it!", true)]);
    let a = set(
        "nexus.a",
        vec![line("Hit", Some("sound\\voice\\mod.esp\\v\\Hit_1.ogg"), "Ow!", true), line("Attack", None, "kept out: no file", true), line("NormalToCombat", Some("x.ogg"), "Ready.", false)],
    );
    // Sets are numbered in id order, whatever order they were read in.
    assert_eq!(add_packed_lines(&mut d, &[b.clone(), a.clone()]), 3);
    let hit = d.packed_lines[&("nexus.a".to_string(), 0)];
    assert_eq!(hit, FormId(PACKED_BASE));
    assert_eq!(d.packed_lines[&("nexus.b".to_string(), 0)], FormId(PACKED_BASE + 2));
    assert_eq!(d.voice_files[&hit], "sound\\voice\\mod.esp\\v\\hit_1.ogg");
    let def = &d.infos[&hit];
    assert_eq!(def.responses.len(), 1);
    assert_eq!(def.responses[0].text, "Ow!");
    assert_eq!(def.response_chars, vec![3]);
    // A set of carried lines needs no speaker in the load order.
    let (v, missing) = PlayerVoice::resolve(&a, &d).unwrap();
    assert_eq!(missing, 1, "the line without a file names an INFO the load order lacks");
    assert_eq!(v.lines.len(), 2);
    assert_eq!(v.pick("Hit", true, 0), Some(hit));
    assert_eq!(v.pick("NormalToCombat", true, 0), None, "a party-only line is not first person");
    // Adding the same sets again makes nothing new.
    assert_eq!(add_packed_lines(&mut d, &[a, b]), 0);
}
