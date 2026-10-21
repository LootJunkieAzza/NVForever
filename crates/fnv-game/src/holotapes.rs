//! Holotapes (docs/spec-notes/holotapes.md): what a Sound or Voice note (NOTE) plays when
//! the player picks it in the Pip-Boy's Notes list. No engine types; the playing itself
//! is the engine's (`nvf-play` `holotape.rs`).
//!
//! - **A Voice note** names a speaker (SNAM, an NPC or creature) and a topic (TNAM)
//!   [documented: xEdit wbDefinitionsFNV.pas NOTE; GECK wiki GetNoteSpeaker,
//!   GetNoteTopic]. It plays one line of the topic in the speaker's voice, every response
//!   of the line in order ([`voice_line`]).
//! - **A Sound note** names a sound (SNAM, a SOUN) [documented: GECK wiki GetNoteSound].

use crate::cond::{Cmp, Condition, Operand, RunOn};
use crate::data::{GameData, InfoDef};
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use std::collections::HashMap;

/// GetIsID's condition function index (opcode 0x1048 less 0x1000) [documented: the
/// function catalogue, `functions::mod` table].
const GET_IS_ID: u16 = 0x48;

/// What a note plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tape {
    /// A line of a topic, said by the speaker (a base form).
    Voice { speaker: FormId, info: FormId },
    /// A sound (SOUN).
    Sound(FormId),
}

/// What `note` plays, if it is a Sound or Voice note that names what it needs.
pub fn tape(data: &GameData, note: FormId) -> Option<Tape> {
    let n = data.notes.get(&note)?;
    match n.kind {
        0 => n.sound.map(Tape::Sound),
        3 => {
            let (speaker, topic) = (n.speaker?, n.topic?);
            let lines = data.topics.get(&topic)?;
            let info = voice_line(lines, &data.infos, speaker)?;
            Some(Tape::Voice { speaker, info })
        }
        _ => None,
    }
}

/// Whether a line names `speaker`: its fixed speaker (ANAM) is the speaker, or one of
/// its conditions is `GetIsID <speaker> == 1` on the subject.
fn names(d: &InfoDef, speaker: FormId) -> bool {
    d.speaker == Some(speaker) || d.conditions.iter().any(|c| is_id(c) == Some(speaker))
}

/// Whether a line names any speaker at all, as [`names`] reads it.
fn names_any(d: &InfoDef) -> bool {
    d.speaker.is_some() || d.conditions.iter().any(|c| is_id(c).is_some())
}

fn is_id(c: &Condition) -> Option<FormId> {
    if c.function != GET_IS_ID || c.run_on != RunOn::Subject || c.cmp != Cmp::Eq || c.rhs != Operand::Value(1.0) {
        return None;
    }
    match c.params.first() {
        Some(ArgValue::Value(Value::Ref(f))) => Some(FormId(*f)),
        _ => None,
    }
}

/// The line of a topic a Voice note plays, from the topic's lines in topic order: the
/// first that names the note's speaker, else the first that names no speaker, else the
/// first [hypothesis: the masters' holotape topics hold one line each, except
/// JacobstownAudioLog01, whose topic is GREETING and whose line is the one with
/// `GetIsID JacobstownSurvivor`; observed, holotapes.md §2]. The line's other conditions
/// are not tested: the speaker is not in the world.
pub fn voice_line(lines: &[FormId], infos: &HashMap<FormId, InfoDef>, speaker: FormId) -> Option<FormId> {
    let defs = || lines.iter().filter_map(|i| infos.get(i).map(|d| (*i, d)));
    defs().find(|(_, d)| names(d, speaker)).or_else(|| defs().find(|(_, d)| !names_any(d))).or_else(|| defs().next()).map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_id_cond(f: u32) -> Condition {
        Condition { cmp: Cmp::Eq, or: false, run_on: RunOn::Subject, function: GET_IS_ID, params: vec![ArgValue::Value(Value::Ref(f))], rhs: Operand::Value(1.0) }
    }

    #[test]
    fn the_line_naming_the_speaker_wins() {
        let mut infos = HashMap::new();
        infos.insert(FormId(1), InfoDef { conditions: vec![is_id_cond(0x50)], ..Default::default() });
        infos.insert(FormId(2), InfoDef::default());
        infos.insert(FormId(3), InfoDef { conditions: vec![is_id_cond(0x60)], ..Default::default() });
        infos.insert(FormId(4), InfoDef { speaker: Some(FormId(0x70)), ..Default::default() });
        let lines = [FormId(1), FormId(2), FormId(3), FormId(4)];
        assert_eq!(voice_line(&lines, &infos, FormId(0x60)), Some(FormId(3)));
        assert_eq!(voice_line(&lines, &infos, FormId(0x70)), Some(FormId(4)));
        // Nobody named: the first line that names no one.
        assert_eq!(voice_line(&lines, &infos, FormId(0x99)), Some(FormId(2)));
    }

    #[test]
    fn a_topic_of_one_line_plays_it() {
        let mut infos = HashMap::new();
        infos.insert(FormId(7), InfoDef { quest: Some(FormId(9)), ..Default::default() });
        assert_eq!(voice_line(&[FormId(7)], &infos, FormId(0x50)), Some(FormId(7)));
        // A lone line fixed to someone else still plays.
        infos.insert(FormId(8), InfoDef { speaker: Some(FormId(0x70)), ..Default::default() });
        assert_eq!(voice_line(&[FormId(8)], &infos, FormId(0x50)), Some(FormId(8)));
        assert_eq!(voice_line(&[], &infos, FormId(0x50)), None);
    }

    #[test]
    fn a_negated_get_is_id_names_no_one() {
        let mut c = is_id_cond(0x50);
        c.rhs = Operand::Value(0.0);
        assert_eq!(is_id(&c), None);
    }
}
