//! The path of a dialogue line's sound (docs/spec-notes/voice.md §2).
//!
//! `sound\voice\<plugin>\<voice type>\<quest>_<topic>_<form id>_<response>.<ext>`
//! [observed: every one of the 61,290 sounds in the owner's archives sits in a voice
//! type's folder; 61,109 of them are named by this rule from a response of a line of
//! the ten masters, voice.md §2–3].

/// Folder of every voice file.
pub const VOICE_ROOT: &str = "sound\\voice";

/// The longest `<quest>_<topic>` part of a file name [observed: voice.md §2.2]. The two
/// Editor IDs are kept whole when they fit with the underscore between them.
pub const QUEST_TOPIC_CHARS: usize = 26;

/// Characters kept of the quest's Editor ID when the two do not fit; the topic then
/// gets what is left of [`QUEST_TOPIC_CHARS`] [observed: voice.md §2.2].
pub const QUEST_CHARS: usize = 10;

/// Extensions a line's sound was found with, in the order the reader tries them
/// [observed: `.ogg` for every sound; 40 also as `.wav`; decision: `.ogg` first, which
/// one the original takes when both exist is not known, voice.md §4].
pub const SOUND_EXTENSIONS: [&str; 2] = ["ogg", "wav"];

fn head(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// File name without extension, lower case: the quest's and the topic's Editor IDs
/// joined by an underscore (whole when that is at most [`QUEST_TOPIC_CHARS`] long, else
/// the quest cut to [`QUEST_CHARS`] and the topic to what is left), the line's FormID
/// with its load-order byte zeroed as eight hex digits, and the response number in
/// decimal.
pub fn voice_stem(quest: &str, topic: &str, form_id: u32, response: u8) -> String {
    let (q, t) = if quest.chars().count() + 1 + topic.chars().count() <= QUEST_TOPIC_CHARS {
        (quest, topic)
    } else {
        let q = head(quest, QUEST_CHARS);
        (q, head(topic, QUEST_TOPIC_CHARS - 1 - q.chars().count()))
    };
    format!("{q}_{t}_{:08x}_{response}", form_id & 0x00FF_FFFF).to_ascii_lowercase()
}

/// Full path (lower case, backslashes) of a response's file with extension `ext`
/// (no dot). `plugin` is the file name of the plugin that owns the line's FormID.
pub fn voice_path(plugin: &str, voice_type: &str, quest: &str, topic: &str, form_id: u32, response: u8, ext: &str) -> String {
    format!("{VOICE_ROOT}\\{plugin}\\{voice_type}\\{}.{ext}", voice_stem(quest, topic, form_id, response)).to_ascii_lowercase()
}

/// The parts of a voice file's path, as far as the name follows the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceFileName<'a> {
    pub plugin: &'a str,
    pub voice_type: &'a str,
    /// File name without extension.
    pub stem: &'a str,
    pub ext: &'a str,
    /// Quest and topic part (before the FormID), FormID and response number, when the
    /// stem ends in `_<8 hex digits>_<decimal>`.
    pub parsed: Option<(&'a str, u32, u8)>,
}

/// Split a normalised path (`fnv_vfs::normalize`) under [`VOICE_ROOT`]. `None` when it
/// is not `sound\voice\<plugin>\<voice type>\<file>.<ext>`.
pub fn parse_voice_path(path: &str) -> Option<VoiceFileName<'_>> {
    let rest = path.strip_prefix(VOICE_ROOT)?.strip_prefix('\\')?;
    let mut it = rest.split('\\');
    let (plugin, voice_type, file) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let (stem, ext) = file.rsplit_once('.')?;
    let parsed = (|| {
        let (before, n) = stem.rsplit_once('_')?;
        let (prefix, id) = before.rsplit_once('_')?;
        if id.len() != 8 || n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some((prefix, u32::from_str_radix(id, 16).ok()?, n.parse().ok()?))
    })();
    Some(VoiceFileName { plugin, voice_type, stem, ext, parsed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_are_kept_whole() {
        assert_eq!(voice_stem("VCG01", "GREETING", 0x0010_4C3A, 1), "vcg01_greeting_00104c3a_1");
    }

    #[test]
    fn names_that_fit_are_kept_whole_whatever_the_quests_length() {
        // 16 + 1 + 5 = 22 characters.
        assert_eq!(voice_stem("NVDLC01Followers", "Death", 0x0100_5343, 1), "nvdlc01followers_death_00005343_1");
        // Exactly 26: whole. 27: cut.
        assert_eq!(voice_stem("ABCDEFGHIJKLM", "NOPQRSTUVWXY", 1, 1), "abcdefghijklm_nopqrstuvwxy_00000001_1");
        assert_eq!(voice_stem("ABCDEFGHIJKLM", "NOPQRSTUVWXYZ", 1, 1), "abcdefghij_nopqrstuvwxyz_00000001_1");
    }

    #[test]
    fn a_long_quest_is_cut_to_ten_characters() {
        // 21 + 1 + 8 = 30 characters: the quest keeps 10, the topic fits in the 15 left.
        assert_eq!(voice_stem("NVDLC01ElijahDialogue", "GREETING", 0x0100_1234, 2), "nvdlc01eli_greeting_00001234_2");
    }

    #[test]
    fn the_topic_gets_what_the_quest_leaves() {
        // Too long together. Quest of 5: the topic keeps 20.
        assert_eq!(voice_stem("VCG02", "VCG02GSSunnySmilesTopic004", 0xABCDEF, 1), "vcg02_vcg02gssunnysmilesto_00abcdef_1");
        // Quest of 18 cut to 10: the topic keeps 15.
        assert_eq!(voice_stem("NVDLC01RadioElijah", "NVDLC01RadioElijahAct1", 0x0100_08D2, 1), "nvdlc01rad_nvdlc01radioeli_000008d2_1");
        // Quest of exactly 10.
        assert_eq!(voice_stem("VStreetSS6", "VStripLucky38ExitSceneWow06", 0x1, 12), "vstreetss6_vstriplucky38ex_00000001_12");
    }

    #[test]
    fn the_load_order_byte_is_zeroed() {
        assert!(voice_stem("Q", "T", 0x04AB_CDEF, 1).contains("_00abcdef_"));
    }

    #[test]
    fn full_path() {
        let p = voice_path("FalloutNV.esm", "MaleUniqueDocMitchell", "VCG01", "VCG01Intro", 0x0010_5F4C, 3, "ogg");
        assert_eq!(p, "sound\\voice\\falloutnv.esm\\maleuniquedocmitchell\\vcg01_vcg01intro_00105f4c_3.ogg");
    }

    #[test]
    fn paths_parse_back() {
        let p = voice_path("DeadMoney.esm", "NVDLC01MaleUniqueDean", "NVDLC01DeanDialogue", "GREETING", 0x0100_BEEF, 10, "lip");
        let f = parse_voice_path(&p).unwrap();
        assert_eq!((f.plugin, f.voice_type, f.ext), ("deadmoney.esm", "nvdlc01maleuniquedean", "lip"));
        assert_eq!(f.parsed, Some(("nvdlc01dea_greeting", 0x00BEEF, 10)));
        // A stray name keeps its stem and has no parsed part.
        let odd = parse_voice_path("sound\\voice\\a.esm\\v\\quest_topic_0000e397_1_alt.ogg").unwrap();
        assert_eq!((odd.stem, odd.parsed), ("quest_topic_0000e397_1_alt", None));
        assert!(parse_voice_path("sound\\voice\\empty.txt").is_none());
        assert!(parse_voice_path("sound\\fx\\a\\b\\c.ogg").is_none());
    }
}
