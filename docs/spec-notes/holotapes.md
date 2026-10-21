# Holotapes: Sound and Voice notes

How NVForever plays the notes (`NOTE`) that are not read but heard: holotapes and audio
logs, from the Pip-Boy's Notes list (DATA page, Misc tab; [pipboy](pipboy.md) §5.3).
ESM gap audit row 14 (docs/handoff/esm-gap-audit-2026-10-21.md). Code:
`crates/fnv-game/src/holotapes.rs` (what a note plays), `engine/nvf-play/src/holotape.rs`
(playing it), `engine/nvf-play/src/pipboy/data.rs` (the list).

## 1. The record

- `DATA` is the note's type: 0 Sound, 1 Text, 2 Image, 3 Voice [documented: xEdit
  wbDefinitionsFNV.pas NOTE, as `schemas/fnv/objects.json` records it; GECK wiki
  GetNoteType]. The GECK's Note window: "Sound: to select a sound file … Voice: to select
  the topic" [documented: GECK wiki Notes].
- `SNAM` is a union by type: a Sound note's sound (SOUN), a Voice note's speaker (an NPC
  or creature) [documented: xEdit; GECK wiki GetNoteSound "the Sound parameter of a
  Sound-type Note", GetNoteSpeaker "the Speaker parameter of a Voice-type Note
  (holotape)"].
- `TNAM` is a union by type too: a Text note's text, a Voice note's topic (DIAL)
  [documented: xEdit; GECK wiki GetNoteTopic].
- `ONAM` is the note's quest. The wiki says a holotape's topic's quest must be among the
  note's quests "for it to play" (SetNoteTopic notes) [documented]; NVForever does not
  test it [design: every master's holotape has the line it names, §2].

## 2. The masters' holotapes [observed: the masters' NOTE records, 2026-10-21]

- FalloutNV.esm: 894 notes, 853 Text, 5 Image, 34 Voice, 2 Sound. Both Sound notes
  (NoteText01, NoteText02) name no sound. Of the 34 Voice notes, 24 are left over from
  Fallout 3 (Lamplight, Keller, Jaime's holotapes …): they name no speaker and their
  topics have no line in the load order, so they have nothing to play. The other 10
  name a speaker and a topic.
- Nine of those topics hold one line, the holotape's, whose responses (2 to 6) are the
  tape read out in order (e.g. VMQ01BeagleJournalNote: PrimmDeputy, topic
  VMQ01BeagleJournal, line 001618B7, six responses, about 32.5 s in MaleAdult11).
- One, JacobstownAudioLog01, names the topic GREETING (5,300 lines) and the speaker
  JacobstownSurvivor; the line meant is the one with the condition
  `GetIsID JacobstownSurvivor == 1` (0013DBBF, quest vDialogueJacobstown).
- The add-ons: Dead Money 5, Old World Blues 3, Lonesome Road 8 Voice notes, each with
  speaker and topic; Honest Hearts and Gun Runners' Arsenal none. No master has a Sound
  note that names a sound.

## 3. What NVForever does

- **Which line.** From the topic's lines in topic order (`GameData::topics`): the first
  whose fixed speaker (ANAM) is the note's speaker or that has `GetIsID <speaker> == 1`
  on the subject; else the first that names no speaker that way; else the first
  [hypothesis: fits every master's holotape, §2; the line's other conditions are not
  tested, as the speaker is not in the world].
- **Its sound.** Each response's voice file as for any line said by the speaker's base
  (`fnv_voice::VoiceBank::file`: `Sound\Voice\<plugin>\<voice type>\<file>.ogg`, the
  voice type from the speaker or its template; [voice](voice.md)), played one after
  another in record order, at full volume, not placed, in the Voice category (the voice
  volume and the "Dialogue voice" switch apply) [hypothesis: the tape plays through the
  Pip-Boy as a conversation's line does]. A response without a file is skipped and
  logged.
- **Picking.** Use on a Sound or Voice note's row (click, Enter, Space, the test aid's
  `key=use` or `note=<FormID>`) starts it; one holotape plays at a time, so another
  pick stops the one playing first; using the one playing stops it [hypothesis]. A pick
  made before the voice index is built (its thread, at start) waits for it.
- **While it plays** it goes on with the Pip-Boy put away and is held by the pause menu
  [hypothesis]. The Pip-Boy radio is left as it is [hypothesis: whether the original
  mutes the radio under a holotape is not known].
- **On the page.** A Sound or Voice note shows the Radio tab's wave panel at the right:
  the still wave while it plays, the flat line otherwise [design: the original's audio
  note panel is not measured; pipboy.md §5.3 had the empty meter].
- **A Sound note** plays its SOUN once at the listener (`crate::sfx::PlaySound`) and
  cannot be stopped [hypothesis; no master note uses it].
- Not built: the subtitles of a holotape's responses (whether the original shows them is
  not known), the JIP note functions (GetNoteSpeaker, SetNoteTopic …), the read flag
  (GetNoteRead).

## 4. Checks

- `cargo test -p fnv-game holotapes` (the line choice) and the pipboy test aid's parse.
- Smoke `pipboy-holotape`: AddNote VMQ01BeagleJournalNote by the console, the Pip-Boy's
  Notes tab, `note=001618BF`: the log names the line and plays
  `sound\voice\falloutnv.esm\maleadult11\nvprimmdep_vmq01beaglejour_001618b7_1.ogg`, then
  the second response; a second `note=` stops it.
